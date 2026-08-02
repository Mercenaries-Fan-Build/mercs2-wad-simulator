//! Serving the live-bridge protocol FROM the reimpl engine (Plan 03 phase 5).
//!
//! The Workshop console drives the retail game through Wally's ASI, which answers the `<<<RUN>>>` /
//! `<<<END>>>` REPL protocol on loopback `127.0.0.1:27050`. This module makes *our* engine answer the
//! same protocol, so the same console can inspect and drive the reimpl — the phase-5 goal.
//!
//! The one constraint that shapes everything: the Lua VM ([`mercs2_engine::script::ScriptHost`]) is
//! `Rc<RefCell<…>>`, so it lives on and may only be touched from the engine's **main thread**. The
//! bridge accepts connections on a **worker thread** (blocking sockets must not stall the render
//! loop), then hands each chunk across a channel to the main thread, which evaluates it in
//! [`Mercs2Game::update`] and answers. This is exactly the ASI's own model — "the chunk queues to the
//! next engine frame and runs on the main thread" — so a client sees identical behaviour.

use std::sync::mpsc::{channel, Receiver, Sender};

/// One queued REPL request, waiting for the main thread to evaluate it. Holds the reply channel back
/// to the worker thread that is blocking on this exact request.
pub struct Request {
    chunk: String,
    reply: Sender<String>,
}

impl Request {
    /// The Lua the client sent.
    pub fn chunk(&self) -> &str {
        &self.chunk
    }

    /// Answer it — unblocks the worker thread, which frames this and writes it to the socket. A send
    /// failure means the client already hung up; harmless.
    pub fn respond(self, output: String) {
        let _ = self.reply.send(output);
    }
}

/// The main-thread handle to a running bridge server: it owns the receiver end of the request queue
/// and keeps the worker thread alive.
pub struct BridgeHost {
    rx: Receiver<Request>,
    _thread: std::thread::JoinHandle<()>,
}

impl BridgeHost {
    /// Start the loopback bridge server on the default REPL port. Returns `None` if the port is
    /// already held — the retail ASI is running, or another reimpl instance — in which case the game
    /// simply runs without a console attach rather than failing to boot.
    pub fn start() -> Option<BridgeHost> {
        let server = mercs2_bridge::Server::bind_default().ok()?;
        let (tx, rx) = channel::<Request>();
        let thread = std::thread::spawn(move || {
            // `serve` blocks per request inside the handler until the main thread answers — the
            // serialization the REPL wants, and the backpressure that keeps the game unhammered.
            let _ = server.serve(|chunk| {
                let (reply_tx, reply_rx) = channel::<String>();
                if tx
                    .send(Request { chunk: chunk.to_string(), reply: reply_tx })
                    .is_err()
                {
                    return "<engine shut down>".to_string();
                }
                reply_rx.recv().unwrap_or_else(|_| "<no reply from engine>".to_string())
            });
        });
        Some(BridgeHost { rx, _thread: thread })
    }

    /// Every request queued since the last frame. Called once per frame on the main thread; returns a
    /// `Vec` (rather than taking a closure) so the caller can evaluate each against `&self` without
    /// overlapping the borrow of the `BridgeHost` field.
    pub fn take_pending(&self) -> Vec<Request> {
        self.rx.try_iter().collect()
    }
}
