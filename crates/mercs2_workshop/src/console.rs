//! The Lua console — the live bridge (`mercs2_bridge`) as a Craft surface.
//!
//! A console is the Script craft mode with a live target instead of a file: you type a Lua chunk, it
//! runs in the *running game*, and the result comes back. The bridge is blocking and the game is
//! frame-sensitive, so — exactly like `ClipLoader` and the publisher — the socket lives on a **worker
//! thread** and the UI talks to it over channels, never touching it from the frame loop.
//!
//! The one enrichment that is nearly free and genuinely useful: the bridge speaks hashes, and the
//! Workshop holds the name pack, so `0x8B7DE1F5` in a result is rewritten to `0x8B7DE1F5 (my_helipad)`
//! on the way to the screen (`enrich`).

use mercs2_bridge::{Bridge, BridgeError};
use std::sync::mpsc;

/// One line of console history.
#[derive(Debug, Clone)]
pub enum Line {
    /// A chunk we sent (echoed so the scrollback reads as a transcript).
    Sent(String),
    /// The game's answer.
    Result(String),
    /// A bridge error (connect refused, timeout, closed).
    Error(String),
    /// A connect / disconnect notice.
    Status(String),
}

/// A live console: a worker thread holding the bridge, plus the UI-owned scrollback and input.
pub struct LiveConsole {
    tx: mpsc::Sender<String>,
    rx: mpsc::Receiver<Line>,
    /// Accumulated transcript, oldest first. Owned here so the UI just renders it.
    pub history: Vec<Line>,
    /// The edit buffer for the next chunk.
    pub input: String,
    /// Whether the worker has reported a live connection (and not since died).
    pub connected: bool,
    /// True once the worker thread has ended (connect failed, or the game closed the socket) — the
    /// UI offers a reconnect.
    pub finished: bool,
}

impl LiveConsole {
    /// Spawn a console against `addr` (e.g. [`mercs2_bridge::DEFAULT_ADDR`]). The connect happens on
    /// the worker thread, so this never blocks the UI even when the game is not running.
    pub fn spawn(addr: String) -> LiveConsole {
        let (tx, jrx) = mpsc::channel::<String>();
        let (ltx, rx) = mpsc::channel::<Line>();
        std::thread::spawn(move || {
            let mut bridge = match Bridge::connect_at(&addr) {
                Ok(b) => {
                    let _ = ltx.send(Line::Status(format!("connected to {addr}")));
                    b
                }
                Err(e) => {
                    let _ = ltx.send(Line::Error(e.to_string()));
                    return;
                }
            };
            // Run each queued chunk in order. A hard failure (the game closed, or the socket is gone)
            // ends the session; a Timeout is soft — the chunk may still run, so keep the console open.
            for chunk in jrx {
                match bridge.eval(&chunk) {
                    Ok(out) => {
                        let _ = ltx.send(Line::Result(out));
                    }
                    Err(e @ (BridgeError::Closed | BridgeError::Connect(_))) => {
                        let _ = ltx.send(Line::Error(e.to_string()));
                        break;
                    }
                    Err(e) => {
                        let _ = ltx.send(Line::Error(e.to_string()));
                    }
                }
            }
            let _ = ltx.send(Line::Status("disconnected".into()));
        });
        LiveConsole {
            tx,
            rx,
            history: Vec::new(),
            input: String::new(),
            connected: false,
            finished: false,
        }
    }

    /// Queue a chunk to run, echoing it into the transcript. A dead worker (the send fails) flips
    /// `finished` so the UI can offer a reconnect rather than silently dropping input.
    pub fn send(&mut self, chunk: String) {
        if chunk.trim().is_empty() {
            return;
        }
        self.history.push(Line::Sent(chunk.clone()));
        if self.tx.send(chunk).is_err() {
            self.finished = true;
            self.connected = false;
        }
    }

    /// Drain everything the worker produced since the last frame. Call once per UI frame; never
    /// blocks. `resolve` turns a hash into a name for the `0xHASH (name)` enrichment.
    pub fn poll(&mut self, resolve: impl Fn(u32) -> Option<String>) {
        loop {
            match self.rx.try_recv() {
                Ok(line) => {
                    match &line {
                        Line::Status(s) if s.starts_with("connected") => self.connected = true,
                        Line::Status(s) if s == "disconnected" => {
                            self.connected = false;
                            self.finished = true;
                        }
                        _ => {}
                    }
                    let line = match line {
                        Line::Result(s) => Line::Result(enrich(&s, &resolve)),
                        other => other,
                    };
                    self.history.push(line);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finished = true;
                    self.connected = false;
                    break;
                }
            }
        }
    }
}

/// Rewrite each `0xHHHHHHHH` in `text` to `0xHHHHHHHH (name)` when `resolve` knows the name. The
/// bridge speaks hashes; the reader thinks in names, and we hold the pack — so bridge the two here.
pub fn enrich(text: &str, resolve: &impl Fn(u32) -> Option<String>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        // A hex literal is `0x` (or `0X`) then 1..=8 hex digits.
        if i + 2 < bytes.len()
            && bytes[i] == b'0'
            && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let start = i;
            let mut j = i + 2;
            while j < bytes.len() && j - (i + 2) < 8 && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            let lit = &text[start..j];
            out.push_str(lit);
            if let Ok(h) = u32::from_str_radix(&lit[2..], 16) {
                if let Some(name) = resolve(h) {
                    out.push_str(&format!(" ({name})"));
                }
            }
            i = j;
        } else {
            // Push one UTF-8 char at a time so a multi-byte char is not split.
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_names_known_hashes_and_leaves_the_rest() {
        let resolve = |h: u32| (h == 0x8B7D_E1F5).then(|| "my_helipad".to_string());
        assert_eq!(
            enrich("gate 0x8B7DE1F5 opened", &resolve),
            "gate 0x8B7DE1F5 (my_helipad) opened"
        );
        // Unknown hash: left as-is.
        assert_eq!(enrich("0xDEADBEEF", &resolve), "0xDEADBEEF");
        // Not a hash, and a short 0x that must not panic on the bounds.
        assert_eq!(enrich("value = 42; 0x", &resolve), "value = 42; 0x");
    }
}
