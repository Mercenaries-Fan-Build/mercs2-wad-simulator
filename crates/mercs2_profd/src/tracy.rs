//! The real [`Launcher`]: one `tracy-capture` process per capture.
//!
//! `tracy-capture` (Tracy's `capture/src/capture.cpp`) prints `Connecting to <addr>:<port>...`, retries until
//! the client answers, then prints `Timer resolution: …` — the line [`TracyLauncher::start`] waits for as
//! proof the capture is connected. It ends on SIGINT (the handler disconnects and the trace is saved) or by
//! itself when the client disconnects, and prints `Frames: N` before saving. It writes with C stdio, which
//! is block-buffered into a pipe, so it runs under `stdbuf -oL`. Its progress line is rewritten with `\r`,
//! so the output is split on `\r` as well as `\n`.

use crate::{Capture, Launcher};
use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The line `tracy-capture` prints once connected.
const CONNECTED: &str = "Timer resolution:";
/// The line that counts the frames saved.
const FRAMES: &str = "Frames:";
/// Output lines kept for error messages and the save summary.
const TAIL_LINES: usize = 16;
const POLL: Duration = Duration::from_millis(50);

pub struct TracyLauncher {
    pub tracy_capture: PathBuf,
    pub game_host: String,
    pub game_port: u16,
    pub connect_timeout: Duration,
    pub save_timeout: Duration,
}

type Tail = Arc<Mutex<VecDeque<String>>>;

fn tail_text(tail: &Tail) -> String {
    let t = tail.lock().unwrap_or_else(|p| p.into_inner());
    t.iter().cloned().collect::<Vec<_>>().join(" | ")
}

/// Read `out` to its end, sending and keeping each `\r`- or `\n`-terminated piece.
fn drain(mut out: impl Read, tx: mpsc::Sender<String>, tail: Tail) {
    let mut buf = [0u8; 4096];
    let mut line = Vec::new();
    let emit = |line: &mut Vec<u8>| {
        if line.is_empty() {
            return;
        }
        let s = String::from_utf8_lossy(line).trim().to_owned();
        line.clear();
        if s.is_empty() {
            return;
        }
        let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
        if t.len() == TAIL_LINES {
            t.pop_front();
        }
        t.push_back(s.clone());
        drop(t);
        // The receiver is gone once the capture is connected; keep draining regardless.
        let _ = tx.send(s);
    };
    loop {
        match out.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        emit(&mut line);
                    } else {
                        line.push(b);
                    }
                }
            }
        }
    }
    emit(&mut line);
}

impl Launcher for TracyLauncher {
    fn start(&self, path: &Path) -> Result<Box<dyn Capture>, String> {
        let mut child = Command::new("stdbuf")
            .arg("-oL")
            .arg(&self.tracy_capture)
            .args(["-a", &self.game_host, "-p", &self.game_port.to_string(), "-o"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("running stdbuf -oL {}: {e}", self.tracy_capture.display()))?;
        let out = child.stdout.take().expect("stdout is piped");
        let (tx, rx) = mpsc::channel();
        let tail: Tail = Arc::new(Mutex::new(VecDeque::with_capacity(TAIL_LINES)));
        let t = tail.clone();
        std::thread::Builder::new()
            .name("tracy-capture output".into())
            .spawn(move || drain(out, tx, t))
            .map_err(|e| format!("starting the output reader: {e}"))?;

        let deadline = Instant::now() + self.connect_timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) if line.starts_with(CONNECTED) => {
                    return Ok(Box::new(TracyCapture { child, path: path.to_owned(), tail, save_timeout: self.save_timeout }));
                }
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let status = child.wait().map_err(|e| format!("waiting for tracy-capture: {e}"))?;
                    return Err(format!("tracy-capture exited ({status}) before connecting: {}", tail_text(&tail)));
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "no game answered at {}:{} within {} s (is the game running with the profiler?): {}",
                        self.game_host,
                        self.game_port,
                        self.connect_timeout.as_secs(),
                        tail_text(&tail)
                    ));
                }
            }
        }
    }
}

pub struct TracyCapture {
    child: Child,
    path: PathBuf,
    tail: Tail,
    save_timeout: Duration,
}

impl TracyCapture {
    fn summary(&self) -> Result<String, String> {
        let bytes = std::fs::metadata(&self.path)
            .map_err(|e| format!("tracy-capture finished but {} is missing: {e}; {}", self.path.display(), tail_text(&self.tail)))?
            .len();
        let tail = self.tail.lock().unwrap_or_else(|p| p.into_inner());
        let frames = tail.iter().rev().find(|l| l.starts_with(FRAMES)).map(|l| l.to_lowercase()).unwrap_or_else(|| "frames: unreported".into());
        let name = self.path.file_name().map_or_else(|| self.path.display().to_string(), |n| n.to_string_lossy().into_owned());
        Ok(format!("saved {name} ({bytes} bytes, {frames})"))
    }
}

impl Capture for TracyCapture {
    fn path(&self) -> &Path {
        &self.path
    }

    fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(mut self: Box<Self>) -> Result<String, String> {
        if self.running() {
            // SIGINT: tracy-capture disconnects and saves (its SigInt handler).
            if unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGINT) } != 0 {
                return Err(format!("signalling tracy-capture: {}", std::io::Error::last_os_error()));
            }
        }
        let deadline = Instant::now() + self.save_timeout;
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                Ok(None) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return Err(format!(
                        "tracy-capture did not finish saving {} within {} s: {}",
                        self.path.display(),
                        self.save_timeout.as_secs(),
                        tail_text(&self.tail)
                    ));
                }
                Err(e) => return Err(format!("waiting for tracy-capture: {e}")),
            }
        };
        if !status.success() {
            return Err(format!("tracy-capture exited ({status}): {}", tail_text(&self.tail)));
        }
        self.summary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_splits_on_carriage_returns_and_newlines() {
        let out: &[u8] = b"Connecting to h:8086...\nTimer resolution: 41 ns\r12.5 Mbps | 3 frames\r\nFrames: 657\n";
        let (tx, rx) = mpsc::channel();
        let tail: Tail = Arc::new(Mutex::new(VecDeque::new()));
        drain(out, tx, tail.clone());
        let lines: Vec<String> = rx.try_iter().collect();
        assert_eq!(lines, ["Connecting to h:8086...", "Timer resolution: 41 ns", "12.5 Mbps | 3 frames", "Frames: 657"]);
        assert_eq!(tail.lock().unwrap().len(), 4);
    }

    #[test]
    fn the_tail_keeps_the_newest_lines() {
        let out: Vec<u8> = (0..40).flat_map(|i| format!("line {i}\n").into_bytes()).collect();
        let (tx, _rx) = mpsc::channel();
        let tail: Tail = Arc::new(Mutex::new(VecDeque::new()));
        drain(&out[..], tx, tail.clone());
        let t = tail.lock().unwrap();
        assert_eq!(t.len(), TAIL_LINES);
        assert_eq!(t.back().unwrap(), "line 39");
    }
}
