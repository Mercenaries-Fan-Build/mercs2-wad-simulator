//! The Workshop Missions debugger's client: the debug protocol (`mercs2_formats::lua_debug`) over the
//! lua-bridge's WebSocket, on the same port as the REPL.
//!
//! [`DebugClient`] is one WebSocket connection. It sends requests, reads the host's frames in order, checks
//! every run event for gaps (`lua_debug::Sequence`, so a recording never has a hole), and keeps the
//! bridge's `ws` feed lines beside them. [`DebugClient::ask`] sends a request and returns its answer
//! (`debug.host` or `debug.ack`), with every frame that arrived first, in order; a `debug.refused` is an
//! error naming the request, which carries the frames that arrived first too. Each attach numbers its
//! events from 1, so the gap check starts again after `debug.detached`. Requests are handled on the game's
//! Lua tick, so a loading screen holds the answer: the deadline is the caller's.
//!
//! Like [`crate::Bridge`] it is a blocking client for a worker thread, never the UI's frame loop.

use base64::Engine as _;
use mercs2_formats::lua_debug::{Event, Request, Sequence, MAX_FRAME_BYTES};
use sha1::Digest as _;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// RFC 6455 section 1.3: appended to the client's key to derive `Sec-WebSocket-Accept`.
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// How long one socket read waits before the deadline is looked at again.
const POLL: Duration = Duration::from_millis(100);

/// Everything the debug client can fail with.
#[derive(Debug)]
pub enum DebugError {
    /// No WebSocket on the port: the game is not running, or the bridge is not loaded.
    Connect(String),
    /// The connection broke or the bridge closed it.
    Closed(String),
    /// A frame broke the protocol (a gap in the events, a malformed message, a bad handshake).
    Protocol(String),
    /// The host refused a request; `before` holds every frame that arrived before the refusal, in order.
    Refused { id: String, error: String, before: Vec<Frame> },
    /// No answer by the deadline.
    Timeout(String),
}

impl std::fmt::Display for DebugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebugError::Connect(e) => write!(f, "{e} (is the game running with the lua-bridge?)"),
            DebugError::Closed(e) => write!(f, "{e}"),
            DebugError::Protocol(e) => write!(f, "debug protocol: {e}"),
            DebugError::Refused { id, error, .. } => write!(f, "{id} refused: {error}"),
            DebugError::Timeout(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DebugError {}

/// One frame from the bridge, in arrival order.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// A debug answer or run event (run events are checked for gaps before they are returned).
    Debug(Event),
    /// A line of the bridge's `log` or `ws` feed.
    Feed { kind: String, line: String },
}

/// One WebSocket connection speaking the debug protocol.
pub struct DebugClient {
    s: TcpStream,
    buf: Vec<u8>,
    mask: u32,
    seq: Sequence,
    next_id: u32,
}

impl DebugClient {
    pub fn connect(addr: impl ToSocketAddrs, handshake: Duration) -> Result<DebugClient, DebugError> {
        let addr = addr.to_socket_addrs().map_err(|e| DebugError::Connect(e.to_string()))?.next();
        let addr = addr.ok_or_else(|| DebugError::Connect("the address resolved to nothing".into()))?;
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|e| DebugError::Connect(format!("connecting to {addr}: {e}")))?;
        let _ = s.set_nodelay(true);
        let clock = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| DebugError::Connect(e.to_string()))?
            .as_nanos();
        let key = base64::engine::general_purpose::STANDARD.encode(clock.to_le_bytes());
        write!(
            s,
            "GET / HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        )
        .map_err(|e| DebugError::Closed(format!("the upgrade request: {e}")))?;
        s.set_read_timeout(Some(handshake)).map_err(|e| DebugError::Closed(e.to_string()))?;
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match s.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                Ok(_) => return Err(DebugError::Closed("the bridge closed the connection during the handshake".into())),
                Err(e) => return Err(DebugError::Closed(format!("the handshake answer: {e}"))),
            }
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let accept = base64::engine::general_purpose::STANDARD.encode(sha1::Sha1::digest(format!("{key}{GUID}").as_bytes()));
        if !head.starts_with("HTTP/1.1 101") || !head.contains(&format!("Sec-WebSocket-Accept: {accept}\r\n")) {
            return Err(DebugError::Protocol(format!("not a WebSocket upgrade: {head:?}")));
        }
        s.set_read_timeout(Some(POLL)).map_err(|e| DebugError::Closed(e.to_string()))?;
        Ok(DebugClient { s, buf: Vec::new(), mask: 0x5a3c_9e17, seq: Sequence::default(), next_id: 0 })
    }

    /// A fresh request id, `prefix` and a counter.
    pub fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    /// Send one text frame (a debug request's JSON, or a REPL `{id, code}`).
    pub fn send_text(&mut self, payload: &[u8]) -> Result<(), DebugError> {
        if payload.len() > MAX_FRAME_BYTES {
            return Err(DebugError::Protocol(format!(
                "a {}-byte frame is over the {MAX_FRAME_BYTES}-byte limit",
                payload.len()
            )));
        }
        self.mask = self.mask.rotate_left(7) ^ 0x9e37_79b9;
        let f = client_frame(0x1, payload, self.mask.to_be_bytes());
        self.s.write_all(&f).map_err(|e| DebugError::Closed(format!("sending a frame: {e}")))
    }

    pub fn send(&mut self, req: &Request) -> Result<(), DebugError> {
        self.send_text(req.to_json().as_bytes())
    }

    /// The next frame by `until`, or `None` at the deadline. A ping is answered.
    pub fn frame(&mut self, until: Instant) -> Result<Option<Frame>, DebugError> {
        loop {
            if let Some((opcode, payload, used)) = server_frame(&self.buf)? {
                self.buf.drain(..used);
                match opcode {
                    0x1 => {
                        if let Some(f) = self.text(&payload)? {
                            return Ok(Some(f));
                        }
                    }
                    0x9 => {
                        self.mask = self.mask.rotate_left(7) ^ 0x9e37_79b9;
                        let pong = client_frame(0xA, &payload, self.mask.to_be_bytes());
                        self.s.write_all(&pong).map_err(|e| DebugError::Closed(format!("answering a ping: {e}")))?;
                    }
                    0x8 => return Err(DebugError::Closed("the bridge closed the WebSocket".into())),
                    other => return Err(DebugError::Protocol(format!("unexpected frame opcode {other:#x}"))),
                }
                continue;
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            let mut chunk = [0u8; 65536];
            match self.s.read(&mut chunk) {
                Ok(0) => return Err(DebugError::Closed("the bridge closed the connection".into())),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => return Err(DebugError::Closed(format!("reading: {e}"))),
            }
        }
    }

    /// A text frame as a [`Frame`]; `None` for a REPL frame that is neither (a chunk's ack).
    fn text(&mut self, body: &[u8]) -> Result<Option<Frame>, DebugError> {
        let shown = || String::from_utf8_lossy(body).into_owned();
        if let Some(ev) = Event::parse(body).map_err(|e| DebugError::Protocol(format!("{e}: {}", shown())))? {
            self.seq.check(&ev).map_err(|e| DebugError::Protocol(e.to_string()))?;
            return Ok(Some(Frame::Debug(ev)));
        }
        let v: serde_json::Value =
            serde_json::from_slice(body).map_err(|e| DebugError::Protocol(format!("{e}: {}", shown())))?;
        let field = |k: &str| v.get(k).and_then(serde_json::Value::as_str).map(str::to_owned);
        match (field("type").as_deref(), field("line")) {
            (Some(kind @ ("log" | "ws")), Some(line)) => Ok(Some(Frame::Feed { kind: kind.to_string(), line })),
            (Some("ack"), _) => Ok(None),
            _ => Err(DebugError::Protocol(format!("an unknown bridge frame: {}", shown()))),
        }
    }

    /// Send `req` and wait until `until` for its answer: `debug.host` for a hello, `debug.names` for a
    /// bindings request, `debug.ack` otherwise.
    /// Returns the answer and every frame that arrived before it, in order.
    pub fn ask(&mut self, req: &Request, until: Instant) -> Result<(Event, Vec<Frame>), DebugError> {
        let id = req.id().to_string();
        self.send(req)?;
        let mut before = Vec::new();
        loop {
            let Some(f) = self.frame(until)? else {
                return Err(DebugError::Timeout(format!(
                    "no answer to {id}: the host handles requests on a Lua tick, so a loading screen or a stalled \
                     game holds them"
                )));
            };
            match f {
                Frame::Debug(ev @ (Event::Host { .. } | Event::Names { .. } | Event::Ack { .. })) if answer_id(&ev) == Some(id.as_str()) => {
                    return Ok((ev, before));
                }
                Frame::Debug(Event::Refused { id: got, error }) if got == id => {
                    return Err(DebugError::Refused { id, error, before });
                }
                other => before.push(other),
            }
        }
    }
}

fn answer_id(ev: &Event) -> Option<&str> {
    match ev {
        Event::Host { id, .. } | Event::Names { id, .. } | Event::Ack { id } | Event::Refused { id, .. } => Some(id),
        _ => None,
    }
}

/// One masked, final client frame.
fn client_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut f = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126 => f.push(0x80 | n as u8),
        n if n < 65536 => {
            f.push(0x80 | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(0x80 | 127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(&mask);
    f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i & 3]));
    f
}

/// One complete server frame at the start of `b`: (opcode, payload, bytes used). The bridge sends only
/// final, unmasked frames.
fn server_frame(b: &[u8]) -> Result<Option<(u8, Vec<u8>, usize)>, DebugError> {
    if b.len() < 2 {
        return Ok(None);
    }
    if b[0] & 0x80 == 0 {
        return Err(DebugError::Protocol("the bridge sent a fragmented frame".into()));
    }
    if b[1] & 0x80 != 0 {
        return Err(DebugError::Protocol("the bridge sent a masked frame".into()));
    }
    let (len, at) = match b[1] & 0x7f {
        126 if b.len() >= 4 => (u16::from_be_bytes([b[2], b[3]]) as usize, 4),
        127 if b.len() >= 10 => (u64::from_be_bytes(b[2..10].try_into().expect("8 bytes")) as usize, 10),
        126 | 127 => return Ok(None),
        n => (n as usize, 2),
    };
    if b.len() < at + len {
        return Ok(None);
    }
    Ok(Some((b[0] & 0x0f, b[at..at + len].to_vec(), at + len)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::lua_debug::{HostKind, PROTOCOL};
    use std::net::TcpListener;

    /// A bridge stand-in: accepts one client, completes the handshake, then sends `frames` after each
    /// request it reads (one batch per request), and returns what it read.
    fn bridge(batches: Vec<Vec<String>>) -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let head = String::from_utf8(head).unwrap();
            let key = head.lines().find_map(|l| l.strip_prefix("Sec-WebSocket-Key: ")).unwrap().trim().to_string();
            let accept = base64::engine::general_purpose::STANDARD.encode(sha1::Sha1::digest(format!("{key}{GUID}").as_bytes()));
            write!(s, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").unwrap();
            let mut got = Vec::new();
            for batch in batches {
                let mut hdr = [0u8; 2];
                s.read_exact(&mut hdr).unwrap();
                let mut len = (hdr[1] & 0x7f) as usize;
                if len == 126 {
                    let mut e = [0u8; 2];
                    s.read_exact(&mut e).unwrap();
                    len = u16::from_be_bytes(e) as usize;
                }
                let mut mask = [0u8; 4];
                s.read_exact(&mut mask).unwrap();
                let mut p = vec![0u8; len];
                s.read_exact(&mut p).unwrap();
                p.iter_mut().enumerate().for_each(|(i, b)| *b ^= mask[i & 3]);
                got.push(String::from_utf8(p).unwrap());
                for f in batch {
                    let mut out = vec![0x81];
                    if f.len() < 126 {
                        out.push(f.len() as u8);
                    } else {
                        out.push(126);
                        out.extend_from_slice(&(f.len() as u16).to_be_bytes());
                    }
                    out.extend_from_slice(f.as_bytes());
                    s.write_all(&out).unwrap();
                }
            }
            // Hold the connection until the client leaves, as the bridge does.
            let _ = s.read_to_end(&mut Vec::new());
            got
        });
        (addr, h)
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn ask_returns_the_answer_with_the_events_and_feed_lines_before_it() {
        let host = Event::Host { id: "h1".into(), protocol: PROTOCOL, kind: HostKind::Retail, build: "b".into(), attached: None };
        let (addr, h) = bridge(vec![
            vec![host.to_json()],
            vec![
                r#"{"type":"ws","line":"hi"}"#.into(),
                r#"{"type":"ack","id":"r","status":"queued"}"#.into(),
                Event::Resumed { seq: 1 }.to_json(),
                Event::Ack { id: "c2".into() }.to_json(),
            ],
        ]);
        let mut c = DebugClient::connect(addr, Duration::from_secs(5)).unwrap();
        let id = c.id("h");
        let (ev, before) = c.ask(&Request::Hello { id, protocol: PROTOCOL }, soon()).unwrap();
        assert_eq!((ev, before), (host, vec![]));
        let id = c.id("c");
        let (ev, before) = c.ask(&Request::Continue { id }, soon()).unwrap();
        assert_eq!(ev, Event::Ack { id: "c2".into() });
        assert_eq!(
            before,
            vec![Frame::Feed { kind: "ws".into(), line: "hi".into() }, Frame::Debug(Event::Resumed { seq: 1 })]
        );
        drop(c);
        let sent = h.join().unwrap();
        assert_eq!(sent[1], r#"{"type":"debug.continue","id":"c2"}"#);
    }

    #[test]
    fn a_refusal_keeps_what_came_before_it_and_a_gap_is_an_error() {
        let (addr, h) = bridge(vec![
            // The player resumed just as Continue was sent: the resume arrives first, then the refusal.
            vec![Event::Resumed { seq: 1 }.to_json(), Event::Refused { id: "c1".into(), error: "the run is not paused".into() }.to_json()],
            vec![Event::Resumed { seq: 3 }.to_json()],
        ]);
        let mut c = DebugClient::connect(addr, Duration::from_secs(5)).unwrap();
        let id = c.id("c");
        let e = c.ask(&Request::Continue { id }, soon()).unwrap_err();
        let DebugError::Refused { error, before, .. } = &e else { panic!("{e}") };
        assert_eq!(error, "the run is not paused");
        assert_eq!(before, &vec![Frame::Debug(Event::Resumed { seq: 1 })]);
        let id = c.id("c");
        let e = c.ask(&Request::Continue { id }, soon()).unwrap_err();
        assert!(e.to_string().contains("seq 3 arrived where 2 was due"), "{e}");
        drop(c);
        h.join().unwrap();
    }

    #[test]
    fn a_second_attach_on_the_connection_numbers_its_events_from_one() {
        let ack = |id: &str| Event::Ack { id: id.into() }.to_json();
        let resumed = |seq| Event::Resumed { seq }.to_json();
        let (addr, h) = bridge(vec![
            vec![ack("a1")],
            vec![resumed(1), resumed(2), resumed(3), ack("d2"), Event::Detached { seq: 4, error: None }.to_json()],
            vec![ack("a3")],
            vec![resumed(1), ack("c4")],
        ]);
        let mut c = DebugClient::connect(addr, Duration::from_secs(5)).unwrap();
        let attach = |c: &mut DebugClient| {
            let id = c.id("a");
            Request::Attach { id, protocol: PROTOCOL, client: "t".into(), modules: vec!["m".into()], breakpoints: vec![], engine: vec![] }
        };
        let req = attach(&mut c);
        c.ask(&req, soon()).unwrap();
        let id = c.id("d");
        let (_, before) = c.ask(&Request::Detach { id }, soon()).unwrap();
        assert_eq!(before.len(), 3);
        // The detached event follows the detach's ack; the next frame read takes it.
        assert!(matches!(c.frame(soon()).unwrap(), Some(Frame::Debug(Event::Detached { .. }))));
        let req = attach(&mut c);
        c.ask(&req, soon()).unwrap();
        let id = c.id("c");
        let (_, before) = c.ask(&Request::Continue { id }, soon()).unwrap();
        assert_eq!(before, vec![Frame::Debug(Event::Resumed { seq: 1 })]);
        drop(c);
        h.join().unwrap();
    }

    #[test]
    fn no_answer_by_the_deadline_names_the_tick() {
        let (addr, h) = bridge(vec![vec![]]);
        let mut c = DebugClient::connect(addr, Duration::from_secs(5)).unwrap();
        let id = c.id("h");
        let e = c.ask(&Request::Hello { id, protocol: PROTOCOL }, Instant::now() + Duration::from_millis(300)).unwrap_err();
        assert!(matches!(&e, DebugError::Timeout(m) if m.contains("Lua tick")), "{e}");
        drop(c);
        h.join().unwrap();
    }
}
