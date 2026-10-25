//! mercs2_profd: the profiler's capture service. Listens for the profiler ASI's requests
//! (`mercs2_formats::profd`) and runs one `tracy-capture` per session, each writing a new
//! `<session>.tracy` in the captures directory; a session's `SAMPLES` stream becomes `<session>.samples`
//! beside it.

use clap::Parser;
use mercs2_formats::prof_samples;
use mercs2_formats::profd::{LINE_MAX, Reply, Request};
use mercs2_profd::Supervisor;
use mercs2_profd::tracy::TracyLauncher;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Parser)]
#[command(about = "Runs tracy-capture for each profiler session the game starts and stops")]
struct Args {
    /// Address the service listens on, e.g. 0.0.0.0:8099.
    #[arg(long, env = "PROFD_LISTEN")]
    listen: String,
    /// Host the game's Tracy client is reached at, e.g. host.docker.internal.
    #[arg(long, env = "PROFD_GAME_HOST")]
    game_host: String,
    /// Port of the game's Tracy client.
    #[arg(long, env = "PROFD_GAME_PORT")]
    game_port: u16,
    /// Directory each session's .tracy and .samples files are written to.
    #[arg(long, env = "PROFD_CAPTURES")]
    captures: PathBuf,
    /// The tracy-capture executable.
    #[arg(long, env = "PROFD_TRACY_CAPTURE")]
    tracy_capture: PathBuf,
    /// Seconds a START waits for the game to answer.
    #[arg(long, env = "PROFD_CONNECT_TIMEOUT", default_value_t = 15)]
    connect_timeout: u64,
    /// Seconds a STOP waits for the trace to be saved.
    #[arg(long, env = "PROFD_SAVE_TIMEOUT", default_value_t = 120)]
    save_timeout: u64,
}

/// How long a connection may take to send its request line.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a samples stream may go without sending a byte before it is taken as dead.
const SAMPLES_IDLE: Duration = Duration::from_secs(60);

type Shared = Arc<Mutex<Supervisor<TracyLauncher>>>;

fn read_line(reader: &mut BufReader<TcpStream>) -> Result<String, String> {
    let mut line = String::new();
    reader.by_ref().take(LINE_MAX as u64 + 2).read_line(&mut line).map_err(|e| format!("reading the request: {e}"))?;
    if !line.ends_with('\n') {
        return Err(format!("the request did not end in a newline within {LINE_MAX} bytes"));
    }
    Ok(line)
}

/// The rest of the stream into `path`, then check it reads as a samples file.
fn receive_samples(reader: &mut BufReader<TcpStream>, path: &Path) -> Result<String, String> {
    reader.get_ref().set_read_timeout(Some(SAMPLES_IDLE)).map_err(|e| format!("setting the read timeout: {e}"))?;
    let mut file = File::create_new(path).map_err(|e| format!("creating {}: {e}", path.display()))?;
    let bytes = std::io::copy(reader, &mut file).map_err(|e| format!("receiving {}: {e}", path.display()))?;
    file.sync_all().map_err(|e| format!("writing {}: {e}", path.display()))?;
    drop(file);
    let data = std::fs::read(path).map_err(|e| format!("reading back {}: {e}", path.display()))?;
    let (header, records) = prof_samples::decode(&data).map_err(|e| format!("{} ({bytes} bytes) is not a whole samples file: {e}", path.display()))?;
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    Ok(format!("wrote {name} ({} samples of {} threads, {bytes} bytes)", records.len(), header.threads.len()))
}

fn reply(mut stream: &TcpStream, peer: &str, shown: &str, r: &Reply) {
    let line = r.line();
    println!("[profd] {peer} {shown} -> {}", line.trim_end());
    if let Err(e) = stream.write_all(line.as_bytes()) {
        eprintln!("[profd] replying to {peer}: {e}");
    }
}

fn connection(stream: TcpStream, sup: &Shared) {
    let peer = stream.peer_addr().map_or_else(|_| "?".into(), |p| p.to_string());
    let req = (|| {
        stream.set_read_timeout(Some(REQUEST_TIMEOUT)).map_err(|e| format!("setting the read timeout: {e}"))?;
        let reader = BufReader::new(stream.try_clone().map_err(|e| format!("cloning the connection: {e}"))?);
        let mut reader = reader;
        let line = read_line(&mut reader)?;
        Ok::<_, String>((Request::parse(&line)?, reader))
    })();
    let (req, mut reader) = match req {
        Ok(r) => r,
        Err(e) => return reply(&stream, &peer, "(unreadable)", &Reply::Err(e)),
    };
    let shown = req.line().trim_end().to_owned();
    match req {
        Request::Samples { session } => {
            let path = sup.lock().unwrap_or_else(|p| p.into_inner()).begin_samples(&session);
            match path {
                Err(e) => reply(&stream, &peer, &shown, &Reply::Err(e)),
                Ok(path) => {
                    println!("[profd] {peer} {shown} -> receiving {}", path.display());
                    let sup = sup.clone();
                    let spawned = std::thread::Builder::new().name(format!("samples {session}")).spawn(move || {
                        let got = receive_samples(&mut reader, &path);
                        sup.lock().unwrap_or_else(|p| p.into_inner()).end_samples(&session);
                        let r = match got {
                            Ok(m) => Reply::Ok(m),
                            Err(e) => Reply::Err(e),
                        };
                        reply(&stream, &peer, &shown, &r);
                    });
                    if let Err(e) = spawned {
                        eprintln!("[profd] starting the samples thread: {e}");
                    }
                }
            }
        }
        other => {
            let r = sup.lock().unwrap_or_else(|p| p.into_inner()).handle(&other);
            reply(&stream, &peer, &shown, &r);
        }
    }
}

fn main() -> ExitCode {
    let a = Args::parse();
    if !a.captures.is_dir() {
        eprintln!("[profd] the captures directory {} does not exist", a.captures.display());
        return ExitCode::from(2);
    }
    if !a.tracy_capture.is_file() {
        eprintln!("[profd] tracy-capture {} does not exist", a.tracy_capture.display());
        return ExitCode::from(2);
    }
    let listener = match TcpListener::bind(&a.listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[profd] binding {}: {e}", a.listen);
            return ExitCode::from(2);
        }
    };
    let launcher = TracyLauncher {
        tracy_capture: a.tracy_capture,
        game_host: a.game_host.clone(),
        game_port: a.game_port,
        connect_timeout: Duration::from_secs(a.connect_timeout),
        save_timeout: Duration::from_secs(a.save_timeout),
    };
    let sup: Shared = Arc::new(Mutex::new(Supervisor::new(launcher, a.captures.clone())));
    println!(
        "[profd] listening on {}; game at {}:{}; captures in {}",
        a.listen,
        a.game_host,
        a.game_port,
        a.captures.display()
    );
    for conn in listener.incoming() {
        match conn {
            Ok(s) => connection(s, &sup),
            Err(e) => eprintln!("[profd] accepting a connection: {e}"),
        }
    }
    ExitCode::SUCCESS
}
