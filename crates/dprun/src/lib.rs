//! DPRun is a utility application for starting DirectPlay lobbyable applications. This crate wraps
//! it in a Rust API!
//!
//! On Linux, this crate uses Wine to start dprun.
//!
//! The DPRun executable must be available separately.

mod inspect;
mod server;
pub mod structs;

use crate::server::HostServer;
use crate::structs::*;
use std::io::{Error as IOError, ErrorKind as IOErrorKind};
use std::path::PathBuf;
use std::process::Command;
use tokio::prelude::*;
use tokio_process::CommandExt; // spawn_async
use uuid::Uuid;

pub use crate::server::{AppController, SPFuture, ServiceProvider};
pub use crate::structs::DPID;
pub use uuid::Uuid as GUID;

// TODO move these to consts again when uuid has const fns
lazy_static::lazy_static! {
    /// The GUID of the DPRun Service Provider.
    static ref GUID_DPRUNSP: Uuid = Uuid::parse_str("B1ED2367-609B-4C5C-8755-D2A29BB9A554").unwrap();
    /// The GUID of the DirectPlay address type containing a port number.
    static ref GUID_INETPORT: Uuid = Uuid::parse_str("E4524541-8EA5-11D1-8A96-006097B01411").unwrap();
}

/// The type of DirectPlay session to create; either joining or hosting a session.
#[derive(Clone, Copy)]
enum SessionType {
    /// Host a DirectPlay session. Optionally specify a GUID for the session; if none is given, a
    /// random one is generated by DPRun.
    Host(Option<Uuid>),
    /// Join a DirectPlay session.
    Join(Uuid),
}

/// A GUID identifying some DirectPlay related object. dprun supports some named aliases for common
/// GUIDs.
#[derive(PartialEq, Eq)]
enum DPGUIDOrNamed {
    /// Any GUID.
    GUID(Uuid),
    /// One of the named GUID aliases supported by DPRun.
    Named(String),
}

impl DPGUIDOrNamed {
    /// Turn this GUID or name into a string that can be passed to the dprun CLI.
    fn into_string(self) -> String {
        match self {
            DPGUIDOrNamed::GUID(guid) => to_braced(&guid),
            DPGUIDOrNamed::Named(string) => string,
        }
    }
}

/// Represents a DirectPlay address value. DirectPlay stores all address parts
/// as memory pointers, but the dprun CLI supports some typed arguments.
pub enum DPAddressValue {
    /// A DirectPlay address part with a numeric value.
    Number(i32),
    /// A DirectPlay address part with a string value.
    String(String),
    /// A DirectPlay address part with a binary value.
    Binary(Vec<u8>),
}

/// Represents a part of a DirectPlay address, akin to DPCOMPOUNDADDRESSELEMENT in the DirectPlay
/// C API. Each address part has a data type and a value.
struct DPAddressPart {
    /// The data type.
    data_type: DPGUIDOrNamed,
    /// The value of this address. These are untyped in DirectPlay. If you need something other
    /// than i32 or String, put raw bytes in DPAddressValue::Binary.
    value: DPAddressValue,
}

/// Create a DPRunOptions struct instance.
#[derive(Default)]
pub struct DPRunOptionsBuilder {
    session_type: Option<SessionType>,
    player_name: Option<String>,
    service_provider: Option<DPGUIDOrNamed>,
    service_provider_handler: Option<Box<ServiceProvider>>,
    application: Option<Uuid>,
    address: Vec<DPAddressPart>,
    session_name: Option<String>,
    session_password: Option<String>,
    cwd: Option<PathBuf>,
}

/// Holds options for running DPRun. DPRunOptions instances can be created using
/// DPRunOptions::builder().
pub struct DPRunOptions {
    session_type: SessionType,
    player_name: String,
    service_provider: DPGUIDOrNamed,
    service_provider_handler: Option<Box<ServiceProvider>>,
    application: Uuid,
    address: Vec<DPAddressPart>,
    session_name: Option<String>,
    session_password: Option<String>,
    cwd: Option<PathBuf>,
}

impl DPRunOptions {
    /// Create options for dprun.
    pub fn builder() -> DPRunOptionsBuilder {
        DPRunOptionsBuilder::default()
    }
}

impl DPRunOptionsBuilder {
    /// Host a DirectPlay session. Optionally specify a GUID for the session; if none is given, a
    /// random one is generated by DPRun.
    pub fn host(self, session_id: Option<Uuid>) -> Self {
        Self {
            session_type: Some(SessionType::Host(session_id)),
            ..self
        }
    }

    /// Join a session.
    pub fn join(self, session_id: Uuid) -> Self {
        Self {
            session_type: Some(SessionType::Join(session_id)),
            ..self
        }
    }

    /// Set the in-game name of the local player.
    pub fn player_name(self, player_name: String) -> Self {
        Self {
            player_name: Some(player_name),
            ..self
        }
    }

    /// Set the service provider to use.
    pub fn service_provider(self, service_provider: Uuid) -> Self {
        Self {
            service_provider: Some(DPGUIDOrNamed::GUID(service_provider)),
            ..self
        }
    }

    /// Set the service provider to use, by name.
    pub fn named_service_provider(self, service_provider: &str) -> Self {
        Self {
            service_provider: Some(DPGUIDOrNamed::Named(service_provider.to_string())),
            ..self
        }
    }

    /// Set a service provider handler. A service provider handler is used for relaying the
    /// DirectPlay messages generated by the application.
    ///
    /// This automatically enables the DPRUN service provider if it's not enabled already.
    pub fn service_provider_handler(mut self, service_provider: Box<ServiceProvider>) -> Self {
        if self.service_provider.is_none() {
            self = self.named_service_provider("DPRUN");
        }
        self.service_provider_handler = Some(service_provider);
        self
    }

    /// Set the application to start.
    pub fn application(self, application: Uuid) -> Self {
        Self {
            application: Some(application),
            ..self
        }
    }

    /// Set the name of the session (optional).
    pub fn session_name(self, session_name: String) -> Self {
        Self {
            session_name: Some(session_name),
            ..self
        }
    }

    /// Password protect the session (optional).
    pub fn session_password(self, session_password: String) -> Self {
        Self {
            session_password: Some(session_password),
            ..self
        }
    }

    /// Set the directory dprun is in (optional, defaults to current working directory).
    pub fn cwd(self, cwd: PathBuf) -> Self {
        Self {
            cwd: Some(cwd),
            ..self
        }
    }

    /// Add an address part.
    pub fn address_part(mut self, data_type: Uuid, value: DPAddressValue) -> Self {
        self.address.push(DPAddressPart {
            data_type: DPGUIDOrNamed::GUID(data_type),
            value,
        });
        self
    }

    /// Add an address part.
    pub fn named_address_part(mut self, data_type: &str, value: DPAddressValue) -> Self {
        self.address.push(DPAddressPart {
            data_type: DPGUIDOrNamed::Named(data_type.to_string()),
            value,
        });
        self
    }

    /// Check the options and build the DPRunOptions struct.
    pub fn finish(self) -> DPRunOptions {
        assert!(self.session_type.is_some());
        assert!(self.player_name.is_some());
        assert!(self.service_provider.is_some());
        assert!(self.application.is_some());

        if self.service_provider == Some(DPGUIDOrNamed::GUID(*GUID_DPRUNSP))
            || self.service_provider == Some(DPGUIDOrNamed::Named("DPRUN".to_string()))
        {
            assert!(
                self.service_provider_handler.is_some(),
                "Must register a service provider handler to use the DPRun service provider."
            );
        }

        DPRunOptions {
            session_type: self.session_type.unwrap(),
            player_name: self.player_name.unwrap(),
            service_provider: self.service_provider.unwrap(),
            service_provider_handler: self.service_provider_handler,
            application: self.application.unwrap(),
            address: self.address,
            session_name: self.session_name,
            session_password: self.session_password,
            cwd: self.cwd,
        }
    }
}

/// Represents a dprun game session.
pub struct DPRun {
    command: Command,
    host_server_port: Option<u16>,
    service_provider: Option<Box<ServiceProvider>>,
}

impl DPRun {
    /// Get the command that will be executed (for debugging).
    pub fn command(&self) -> String {
        format!("{:?}", self.command)
    }

    /// Start a game without the host server for the DPRun Service Provider.
    fn start_without_server(mut self) -> impl Future<Item = (), Error = IOError> {
        future::result(self.command.spawn_async())
            .flatten()
            .and_then(|result| {
                if result.success() {
                    return future::finished(());
                }
                future::err(IOError::new(
                    IOErrorKind::Other,
                    format!("dprun exited with status {}", result.code().unwrap_or(0)),
                ))
            })
    }

    /// Start a game that uses the host server for the DPRun Service Provider.
    fn start_with_server(mut self) -> impl Future<Item = (), Error = IOError> {
        let server = HostServer::new(
            self.host_server_port.unwrap_or(2197),
            self.service_provider.unwrap(),
        );
        let server_result = future::result(server.start());
        let child_result = future::result(self.command.spawn_async());
        server_result
            .join(child_result)
            .and_then(|((server, mut controller), child)| {
                child
                    .and_then(|result| {
                        if result.success() {
                            return future::finished(());
                        }
                        future::err(IOError::new(
                            IOErrorKind::Other,
                            format!("dprun exited with status {}", result.code().unwrap_or(0)),
                        ))
                    })
                    .then(move |result| {
                        println!(
                            "[DPRun::start_with_server] waiting for host server to shut down..."
                        );
                        controller.stop();
                        result
                    })
                    .join(server)
                    .map(|_| ())
            })
    }

    /// Start dprun.
    pub fn start(self) -> impl Future<Item = (), Error = IOError> {
        if self.service_provider.is_some() {
            future::Either::A(self.start_with_server())
        } else {
            future::Either::B(self.start_without_server())
        }
    }
}

fn to_braced(uuid: &Uuid) -> String {
    let mut res = &mut [0u8; 38];
    res[0] = b'{';
    res[37] = b'}';
    uuid.to_hyphenated().encode_upper(&mut res[1..=36]);
    String::from_utf8_lossy(res).to_string()
}

/// Run a game using DPRun. The options can be created using DPRunOptions::builder().
pub fn run(options: DPRunOptions) -> DPRun {
    let mut command = if cfg!(target_os = "windows") {
        Command::new("dprun.exe")
    } else {
        let mut wine = Command::new("wine");
        wine.arg("dprun.exe");
        wine
    };

    if let Some(cwd) = options.cwd {
        command.current_dir(cwd);
    }

    match options.session_type {
        SessionType::Host(Some(guid)) => command.args(&["--host", &to_braced(&guid)]),
        SessionType::Host(None) => command.arg("--host"),
        SessionType::Join(guid) => command.args(&["--join", &to_braced(&guid)]),
    };

    let service_provider = options.service_provider_handler;

    let host_server_port = if service_provider.is_some() {
        options
            .address
            .iter()
            .find(|part| {
                part.data_type == DPGUIDOrNamed::GUID(*GUID_INETPORT)
                    || part.data_type == DPGUIDOrNamed::Named("INetPort".to_string())
            })
            .and_then(|part| {
                if let DPAddressValue::Number(val) = part.value {
                    Some(val as u16)
                } else {
                    Some(2197)
                }
            })
    } else {
        None
    };

    command.args(&[
        "--player",
        &options.player_name,
        "--service-provider",
        &options.service_provider.into_string(),
        "--application",
        &to_braced(&options.application),
    ]);

    for part in options.address {
        let key = part.data_type.into_string();
        let value = match part.value {
            DPAddressValue::Number(val) => format!("i:{}", val),
            DPAddressValue::String(val) => val,
            DPAddressValue::Binary(val) => format!(
                "b:{}",
                val.iter().map(|c| format!("{:02x}", c)).collect::<String>()
            ),
        };
        command.args(&["--address", &format!("{}={}", key, value)]);
    }

    if let Some(name) = options.session_name {
        command.args(&["--session-name", &name]);
    }

    if let Some(password) = options.session_password {
        command.args(&["--session-password", &password]);
    }

    DPRun {
        command,
        host_server_port,
        service_provider,
    }
}

#[cfg(test)]
mod tests {
    use super::{run, DPAddressValue, DPRunOptions};
    use uuid::Uuid;

    #[test]
    fn build_command_line_args() {
        let dpchat = Uuid::parse_str("5BFDB060-06A4-11d0-9C4F-00A0C905425E").unwrap();
        let tcpip = Uuid::parse_str("36E95EE0-8577-11cf-960c-0080c7534e82").unwrap();
        let inet_port = Uuid::parse_str("E4524541-8EA5-11d1-8A96-006097B01411").unwrap();

        let options = DPRunOptions::builder()
            .host(None)
            .player_name("Test".into())
            .application(dpchat)
            .service_provider(tcpip)
            .named_address_part("INet", DPAddressValue::String("127.0.0.1".into()))
            .address_part(inet_port, DPAddressValue::Number(2197))
            .finish();

        let dp_run = run(options);
        if cfg!(target_os = "windows") {
            assert_eq!(dp_run.command(), r#""dprun.exe" "--host" "--player" "Test" "--service-provider" "{36E95EE0-8577-11CF-960C-0080C7534E82}" "--application" "{5BFDB060-06A4-11D0-9C4F-00A0C905425E}" "--address" "INet=127.0.0.1" "--address" "{E4524541-8EA5-11D1-8A96-006097B01411}=i:2197""#);
        } else {
            assert_eq!(dp_run.command(), r#""wine" "dprun.exe" "--host" "--player" "Test" "--service-provider" "{36E95EE0-8577-11CF-960C-0080C7534E82}" "--application" "{5BFDB060-06A4-11D0-9C4F-00A0C905425E}" "--address" "INet=127.0.0.1" "--address" "{E4524541-8EA5-11D1-8A96-006097B01411}=i:2197""#);
        }
    }
}
