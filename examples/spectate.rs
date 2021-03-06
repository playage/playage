use aoc_spectate::SpectateStream;
use async_std::{
    fs::{self, File},
    net::TcpStream,
    prelude::*,
    task,
};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};
use structopt::StructOpt;

/// Spectate an ongoing Age of  Empires 2 game.
#[derive(Debug, StructOpt)]
struct Cli {
    /// IP Address to connect to.
    address: String,
    /// Path to the Age of Empires 2 game directory.
    #[structopt(
        long = "game-path",
        short = "p",
        default_value = r"c:\Program Files (x86)\Microsoft Games\Age of Empires II"
    )]
    game_path: PathBuf,
}

#[cfg(target_os = "windows")]
fn start_aoc(aoc_path: &Path, game_name: &str, spec_file: &Path) -> io::Result<Child> {
    Command::new(aoc_path)
        .arg(format!("GAME={}", game_name))
        .arg(format!(r#""{}""#, spec_file.to_string_lossy()))
        .spawn()
}

#[cfg(not(target_os = "windows"))]
fn start_aoc(aoc_path: &Path, game_name: &str, spec_file: &Path) -> io::Result<Child> {
    use winepath::WineConfig;
    let convert = WineConfig::from_env().unwrap();

    Command::new("wine")
        .arg(aoc_path.to_string_lossy().to_string())
        .arg(format!("GAME={}", game_name))
        .arg(format!(r#""{}""#, convert.to_wine_path(spec_file).unwrap()))
        .spawn()
}

/// Find a UserPatched Age of Empires 2 executable.
///
/// `basedir` is the install directory of Age of Empires 2.
async fn find_aoc(basedir: impl AsRef<Path>) -> io::Result<PathBuf> {
    let exedir = basedir.as_ref().join("Age2_x1");
    for candidate in &["age2_x1.5.exe", "age2_x1.exe"] {
        let filename = exedir.join(candidate);
        match fs::metadata(&filename).await {
            Ok(meta) if meta.is_file() => return Ok(filename),
            _ => (),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("could not find aoc exe in {:?}", basedir.as_ref()),
    ))
}

async fn amain(args: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let game_path = find_aoc(&args.game_path).await?;

    let addr = format!("{}:53754", args.address);
    let stream = TcpStream::connect(addr).await?;
    let mut sesh = SpectateStream::connect_stream(Box::new(stream)).await?;

    println!("Game: {}", sesh.game_name());
    println!("Ext: {}", sesh.file_type());
    println!("Streaming from: {}", sesh.player_name());

    let spec_file = game_path
        .parent() // "/Age2_x1"
        .unwrap()
        .parent() // "/"
        .unwrap()
        .join("SaveGame") // "/SaveGame"
        .join(format!("spec.{}", sesh.file_type()));
    println!("{:?}", spec_file);
    let mut file = File::create(&spec_file).await?;
    let header = sesh.read_rec_header().await?;
    file.write_all(&header).await?;
    file.sync_data().await?;

    println!("Starting...");

    let running = Arc::new(AtomicBool::new(true));
    let thread = thread::spawn({
        let running = Arc::clone(&running);
        let game_name = sesh.game_name().to_string();
        move || {
            let mut aoc =
                start_aoc(&args.game_path, &game_name, &spec_file).expect("could not start aoc");
            let result = aoc.wait();
            running.store(false, Ordering::SeqCst);
            result.unwrap();
        }
    });

    println!("Receiving recorded game data...");

    let mut buffer = [0; 16 * 1024];
    while let Ok(num) = sesh.inner().read(&mut buffer).await {
        file.write_all(&buffer[0..num]).await?;
        file.sync_data().await?;
        if num == 0 {
            break;
        }
        if !running.load(Ordering::Relaxed) {
            println!("AoC exited! Stopping spec feed...");
            break;
        }
    }

    println!("No more actions! Waiting for AoC to close...");

    thread.join().unwrap();

    Ok(())
}

fn main() {
    let args = Cli::from_args();
    let task = task::spawn(async move {
        amain(args).await.unwrap();
    });
    task::block_on(task);
}
