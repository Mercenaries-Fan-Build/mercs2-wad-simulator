//! The Live page: the running game.
//!
//! The page is blank until the Live World design (a read-only mirror of the running game) is built
//! to match it. What it shows already is on the rail: whether the game answers on its lua-bridge
//! port, polled by [`Probe`] off the frame loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How often the probe asks whether the game answers.
const POLL: Duration = Duration::from_secs(3);
/// How long one ask waits for the game to accept.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

/// Whether the game's lua-bridge accepts connections, kept current by a worker thread.
pub struct Probe {
    up: Arc<AtomicBool>,
}

impl Probe {
    /// Start polling `addr` every few seconds.
    pub fn spawn(addr: String) -> Probe {
        let up = Arc::new(AtomicBool::new(false));
        let shared = up.clone();
        std::thread::Builder::new()
            .name("live-probe".into())
            .spawn(move || loop {
                shared.store(mercs2_bridge::reachable(addr.as_str(), CONNECT_TIMEOUT), Ordering::Relaxed);
                std::thread::sleep(POLL);
            })
            .expect("spawn the live probe thread");
        Probe { up }
    }

    /// The last answer: the game is reachable.
    pub fn reachable(&self) -> bool {
        self.up.load(Ordering::Relaxed)
    }
}

/// The page itself: blank.
pub fn center(ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(crate::gui::theme::G0))
        .show(ctx, |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe reports a listening host as reachable once its first poll lands.
    #[test]
    fn the_probe_sees_a_listening_host() {
        let server = mercs2_bridge::Server::bind("127.0.0.1:0").expect("bind ephemeral");
        let addr = server.local_addr().expect("addr").to_string();
        std::thread::spawn(move || {
            let _ = server.serve(|_| String::new());
        });
        let probe = Probe::spawn(addr);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !probe.reachable() {
            assert!(std::time::Instant::now() < deadline, "the probe never saw the host");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
