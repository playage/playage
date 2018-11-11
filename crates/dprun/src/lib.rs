mod server;
pub mod structs;

use std::process::Command;
use std::path::PathBuf;
use std::io::{Error as IOError, ErrorKind as IOErrorKind};
use tokio::io::Result;
use tokio::prelude::*;
use tokio_process::{Child, CommandExt};
use crate::server::HostServer;
use crate::structs::*;

pub use crate::server::ServiceProvider;

/// GUID structure, for identifying DirectPlay interfaces, applications, and address types.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GUID(pub u32, pub u16, pub u16, pub u8, pub u8, pub u8, pub u8, pub u8, pub u8, pub u8, pub u8);

impl std::fmt::Display for GUID {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{{{:08X?}-{:04X?}-{:04x?}-{:02X?}{:02X?}-{:02X?}{:02X?}{:02X?}{:02X?}{:02X?}{:02X?}}}",
               self.0,
               self.1,
               self.2,
               self.3, self.4,
               self.5, self.6, self.7, self.8, self.9, self.10)
    }
}

/// The type of DirectPlay session to create; either joining or hosting a session.
#[derive(Clone, Copy)]
enum SessionType {
    /// Host a DirectPlay session. Optionally specify a GUID for the session; if none is given, a
    /// random one is generated by DPRun.
    Host(Option<GUID>),
    /// Join a DirectPlay session.
    Join(GUID),
}

#[derive(PartialEq, Eq)]
enum DPGUIDOrNamed {
    GUID(GUID),
    Named(String),
}

impl DPGUIDOrNamed {
    fn into_string(self) -> String {
        match self {
            DPGUIDOrNamed::GUID(guid) => guid.to_string(),
            DPGUIDOrNamed::Named(string) => string,
        }
    }
}

pub enum DPAddressValue {
    /// A DirectPlay address part with a numeric value.
    Number(i32),
    /// A DirectPlay address part with a string value.
    String(String),
    /// A DirectPlay address part with a binary value.
    Binary(Vec<u8>),
}

/// Represents a part of a DirectPlay address, akin to DPCOMPOUNDADDRESSELEMENT in the DirectPlay
/// C API.
struct DPAddressPart {
    data_type: DPGUIDOrNamed,
    value: DPAddressValue,
}

#[derive(Default)]
pub struct DPRunOptionsBuilder {
    session_type: Option<SessionType>,
    player_name: Option<String>,
    service_provider: Option<DPGUIDOrNamed>,
    application: Option<GUID>,
    address: Vec<DPAddressPart>,
    session_name: Option<String>,
    session_password: Option<String>,
    cwd: Option<PathBuf>,
}

pub struct DPRunOptions {
    session_type: SessionType,
    player_name: String,
    service_provider: DPGUIDOrNamed,
    application: GUID,
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
    pub fn host(self, session_id: Option<GUID>) -> Self {
        Self { session_type: Some(SessionType::Host(session_id)), ..self }
    }

    /// Join a session.
    pub fn join(self, session_id: GUID) -> Self {
        Self { session_type: Some(SessionType::Join(session_id)), ..self }
    }

    /// Set the in-game name of the local player.
    pub fn player_name(self, player_name: String) -> Self {
        Self { player_name: Some(player_name), ..self }
    }

    /// Set the service provider to use.
    pub fn service_provider(self, service_provider: GUID) -> Self {
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

    /// Set the application to start.
    pub fn application(self, application: GUID) -> Self {
        Self { application: Some(application), ..self }
    }

    /// Set the name of the session (optional).
    pub fn session_name(self, session_name: String) -> Self {
        Self { session_name: Some(session_name), ..self }
    }

    /// Password protect the session (optional).
    pub fn session_password(self, session_password: String) -> Self {
        Self { session_password: Some(session_password), ..self }
    }

    /// Set the directory dprun is in (optional, defaults to current working directory).
    pub fn cwd(self, cwd: PathBuf) -> Self {
        Self { cwd: Some(cwd), ..self }
    }

    /// Add an address part.
    pub fn address_part(mut self, data_type: GUID, value: DPAddressValue) -> Self {
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

        DPRunOptions {
            session_type: self.session_type.unwrap(),
            player_name: self.player_name.unwrap(),
            service_provider: self.service_provider.unwrap(),
            application: self.application.unwrap(),
            address: self.address,
            session_name: self.session_name,
            session_password: self.session_password,
            cwd: self.cwd,
        }
    }
}

const GUID_DPRUNSP: GUID = GUID(0xb1ed2367, 0x609b, 0x4c5c, 0x87, 0x55, 0xd2, 0xa2, 0x9b, 0xb9, 0xa5, 0x54);

struct DPRunSP;
impl ServiceProvider for DPRunSP {
    fn open(&mut self, data: OpenData) {
        println!("Got Open message: {:?}", data);
    }

    fn create_player(&mut self, data: CreatePlayerData) {
        println!("Got CreatePlayer message: {:?}", data);
    }
}

/// Represents a dprun game session.
pub struct DPRun {
    command: Command,
    service_provider: Option<DPRunSP>,
}

impl DPRun {
    /// Get the command that will be executed (for debugging).
    pub fn command(&self) -> String {
        format!("{:?}", self.command)
    }

    fn start_without_server(mut self) -> impl Future<Item = (), Error = IOError> {
        future::result(self.command.spawn_async())
            .flatten()
            .and_then(|result| {
                if result.success() {
                    return future::finished(());
                }
                future::err(IOError::new(IOErrorKind::Other, format!("dprun exited with status {}", result.code().unwrap_or(0))))
            })
    }

    fn start_with_server(mut self) -> impl Future<Item = (), Error = IOError> {
        let server = HostServer::new(2197);
        let server = future::result(server.start());
        let child = future::result(self.command.spawn_async());
        server.join(child).and_then(|((server, mut controller), child)| {
            child.and_then(|result| {
                if result.success() {
                    return future::finished(());
                }
                future::err(IOError::new(IOErrorKind::Other, format!("dprun exited with status {}", result.code().unwrap_or(0))))
            }).then(move |result| {
                println!("waiting for host server to shut down...");
                controller.stop();
                result
            }).and_then(|_| server)
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
        SessionType::Host(Some(guid)) => {
            command.args(&["--host", &guid.to_string()])
        },
        SessionType::Host(None) => {
            command.arg("--host")
        },
        SessionType::Join(guid) => {
            command.args(&["--join", &guid.to_string()])
        },
    };

    let service_provider = if options.service_provider == DPGUIDOrNamed::GUID(GUID_DPRUNSP) || options.service_provider == DPGUIDOrNamed::Named("DPRUN".to_string()) {
        Some(DPRunSP)
    } else {
        None
    };

    command.args(&[
        "--player", &options.player_name,
        "--service-provider", &options.service_provider.into_string(),
        "--application", &options.application.to_string(),
    ]);

    for part in options.address {
        let key = part.data_type.into_string();
        let value = match part.value {
            DPAddressValue::Number(val) => format!("i:{}", val),
            DPAddressValue::String(val) => val,
            DPAddressValue::Binary(val) => format!("b:{}",
                val.iter().map(|c| format!("{:02x}", c)).collect::<String>()),
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
        service_provider,
    }
}

#[cfg(test)]
mod tests {
    use crate::{run, DPAddressValue, DPRunOptions, GUID};

    #[test]
    fn build_command_line_args() {
        let dpchat = GUID(0x5BFDB060, 0x06A4, 0x11D0, 0x9C, 0x4F, 0x00, 0xA0, 0xC9, 0x05, 0x42, 0x5E);
        let tcpip = GUID(0x36E95EE0, 0x8577, 0x11cf, 0x96, 0x0c, 0x00, 0x80, 0xc7, 0x53, 0x4e, 0x82);
        let inet_port = GUID(0xe4524541, 0x8ea5, 0x11d1, 0x8a, 0x96, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11);

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
            assert_eq!(dp_run.command(), r#""dprun.exe" "--host" "--player" "Test" "--service-provider" "{36E95EE0-8577-11cf-960C-0080C7534E82}" "--application" "{5BFDB060-06A4-11d0-9C4F-00A0C905425E}" "--address" "INet=127.0.0.1" "--address" "{E4524541-8EA5-11d1-8A96-006097B01411}=i:2197""#);
        } else {
            assert_eq!(dp_run.command(), r#""wine" "dprun.exe" "--host" "--player" "Test" "--service-provider" "{36E95EE0-8577-11cf-960C-0080C7534E82}" "--application" "{5BFDB060-06A4-11d0-9C4F-00A0C905425E}" "--address" "INet=127.0.0.1" "--address" "{E4524541-8EA5-11d1-8A96-006097B01411}=i:2197""#);
        }
    }
}
