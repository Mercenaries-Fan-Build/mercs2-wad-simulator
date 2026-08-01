//! The live bridge — a rate-limited client for the running game's Lua REPL.
//!
//! Wally's `lua-bridge` ASI (`loganw234/Merc2-Mods-Exp/mods/lua-bridge-DEV`, built on with
//! permission) hosts a REPL inside the game process, listening on **loopback `127.0.0.1:27050`**.
//! It auto-detects the transport on that one port: a raw-TCP request/response protocol, or a full
//! WebSocket for browser clients. This crate speaks the **raw-TCP** side, because that is the clean
//! request→response transport and a native tool can open a socket directly — the WebSocket exists
//! for browsers, and there is nothing for us to bridge (the shim Plan 03 once called its first move
//! is obsolete now that his ASI serves both).
//!
//! ## The protocol (confirmed from `lua_bridge_DEV.c`)
//!
//! ```text
//!   client → <lua chunk> <<<RUN>>>          request: a chunk, terminated by the RUN marker
//!   (chunk queues to the next engine frame and runs on the main thread)
//!   client ← <result> <<<END>>>            response: the result, terminated by the END marker
//! ```
//!
//! ## The discipline this crate enforces, so callers cannot forget it
//!
//! The retail game is a 15-year-old, frame-sensitive process, and the ASI's own measurements are
//! `Tcp.Send ≈ 15 ms`, `Loader.Printf ≈ 5 ms`. So a bridge that is hammered per-frame degrades the
//! game it is inspecting. This crate is therefore **rate-limited** (a minimum interval between
//! sends) and **timeout-bounded** (a hung game returns [`BridgeError::Timeout`], never an infinite
//! block). It is a blocking client meant to be driven from a **worker thread**, off the UI's frame
//! loop — the same shape as the Workshop's other background loaders.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Where the ASI listens by default — loopback only, matching its bind.
pub const DEFAULT_ADDR: &str = "127.0.0.1:27050";

/// Terminates a request chunk on the wire.
const RUN: &[u8] = b"<<<RUN>>>";
/// Terminates a response on the wire.
const END: &[u8] = b"<<<END>>>";

/// Everything that can go wrong talking to the game.
#[derive(Debug)]
pub enum BridgeError {
    /// Could not open the socket — the game is not running, or the ASI is not loaded.
    Connect(std::io::Error),
    /// A read or write failed mid-exchange.
    Io(std::io::Error),
    /// The game did not answer within the deadline. Not fatal — the chunk may still run; the caller
    /// can retry or reconnect. A frozen loading screen looks exactly like this.
    Timeout,
    /// The ASI closed the connection.
    Closed,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Connect(e) => write!(
                f,
                "cannot reach the game bridge on the REPL port: {e}. Is the game running with the \
                 lua-bridge ASI loaded?"
            ),
            BridgeError::Io(e) => write!(f, "bridge I/O error: {e}"),
            BridgeError::Timeout => write!(
                f,
                "the game did not answer in time — it may be loading or hung. The chunk may still run"
            ),
            BridgeError::Closed => write!(f, "the game bridge closed the connection"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// A single live connection to the game's REPL.
///
/// One `Bridge` is one TCP connection; hold it on a worker thread and feed it chunks. Dropping it
/// closes the socket. It is deliberately **not** `Clone` or `Sync` — a REPL is a serialized
/// conversation, and sharing one socket across threads would interleave requests.
#[derive(Debug)]
pub struct Bridge {
    stream: TcpStream,
    min_interval: Duration,
    last_send: Option<Instant>,
    timeout: Duration,
}

impl Bridge {
    /// Connect to the default loopback REPL with sane defaults (60 ms between sends — comfortably
    /// above the ASI's ~15 ms `Tcp.Send` floor — and a 5 s answer deadline).
    pub fn connect_default() -> Result<Self, BridgeError> {
        Self::connect(DEFAULT_ADDR, Duration::from_millis(60), Duration::from_secs(5))
    }

    /// Connect to a specific address with the default timing — for a non-standard host/port.
    pub fn connect_at(addr: impl ToSocketAddrs) -> Result<Self, BridgeError> {
        Self::connect(addr, Duration::from_millis(60), Duration::from_secs(5))
    }

    /// Connect to `addr`, enforcing `min_interval` between sends and `timeout` per exchange.
    ///
    /// A 3 s connect timeout keeps a wrong address or a firewall from stalling the caller; the
    /// socket is left in blocking mode with per-operation timeouts for the exchanges.
    pub fn connect(
        addr: impl ToSocketAddrs,
        min_interval: Duration,
        timeout: Duration,
    ) -> Result<Self, BridgeError> {
        // Resolve then connect with a bounded timeout, so an unreachable host fails fast.
        let mut last_err =
            std::io::Error::new(std::io::ErrorKind::NotFound, "no address resolved");
        let addrs = addr.to_socket_addrs().map_err(BridgeError::Connect)?;
        let mut stream = None;
        for a in addrs {
            match TcpStream::connect_timeout(&a, Duration::from_secs(3)) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        let stream = stream.ok_or(BridgeError::Connect(last_err))?;
        // Nagle off: a REPL turn is one small request then a wait, so batching only adds latency.
        let _ = stream.set_nodelay(true);
        Ok(Self {
            stream,
            min_interval,
            last_send: None,
            timeout,
        })
    }

    /// Run one Lua chunk in the live game and return its printed result.
    ///
    /// Blocks until the game answers or `timeout` elapses — call it from a worker thread. Enforces
    /// the rate limit by waiting out any remaining interval since the last send (bounded by
    /// `min_interval`), so a caller cannot accidentally flood the game.
    pub fn eval(&mut self, chunk: &str) -> Result<String, BridgeError> {
        if let Some(last) = self.last_send {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                std::thread::sleep(self.min_interval - elapsed);
            }
        }
        self.stream
            .set_write_timeout(Some(self.timeout))
            .map_err(BridgeError::Io)?;
        self.stream
            .write_all(chunk.as_bytes())
            .and_then(|()| self.stream.write_all(RUN))
            .and_then(|()| self.stream.flush())
            .map_err(BridgeError::Io)?;
        self.last_send = Some(Instant::now());
        self.read_until_end()
    }

    /// Read the socket until the `<<<END>>>` terminator, returning everything before it. The game's
    /// results are CP1252, but a console only displays them, so this decodes lossily as UTF-8.
    fn read_until_end(&mut self) -> Result<String, BridgeError> {
        let deadline = Instant::now() + self.timeout;
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        let mut chunk = [0u8; 4096];
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(BridgeError::Timeout);
            };
            // A zero timeout is "block forever" to the OS — floor it so the deadline is real.
            self.stream
                .set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
                .map_err(BridgeError::Io)?;
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(BridgeError::Closed),
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = find_subslice(&buf, END) {
                        buf.truncate(pos);
                        return Ok(String::from_utf8_lossy(&buf).into_owned());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(BridgeError::Timeout)
                }
                Err(e) => return Err(BridgeError::Io(e)),
            }
        }
    }
}

/// The first index at which `needle` occurs in `hay`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_terminator_including_across_read_boundaries() {
        assert_eq!(find_subslice(b"abc<<<END>>>xyz", END), Some(3));
        assert_eq!(find_subslice(b"no terminator here", END), None);
        // The marker split over two reads is why we search the ACCUMULATED buffer, not each read.
        let mut buf = b"result<<<EN".to_vec();
        assert_eq!(find_subslice(&buf, END), None);
        buf.extend_from_slice(b"D>>>");
        assert_eq!(find_subslice(&buf, END), Some(6));
    }
}
