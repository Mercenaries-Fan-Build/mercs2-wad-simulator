//! The capture service's sessions: one capture at a time, started and stopped by requests in the protocol
//! of `mercs2_formats::profd`. How a capture is run is a [`Launcher`]; the real one is
//! [`tracy::TracyLauncher`].

pub mod tracy;

use mercs2_formats::profd::{Reply, Request};
use std::path::{Path, PathBuf};

/// A capture that is running. `Send`: the service's samples threads reach the supervisor that holds it.
pub trait Capture: Send {
    /// The file it writes.
    fn path(&self) -> &Path;
    /// Whether it is still running (a capture ends by itself when the game disconnects).
    fn running(&mut self) -> bool;
    /// End it and save its file; the text describes what was saved.
    fn stop(self: Box<Self>) -> Result<String, String>;
}

/// Starts captures. `start` returns once the capture is connected to the game, or fails.
pub trait Launcher {
    fn start(&self, path: &Path) -> Result<Box<dyn Capture>, String>;
}

/// The service's state: the captures directory, the capture that is running, if any, and the session
/// whose samples stream is open, if any.
pub struct Supervisor<L: Launcher> {
    launcher: L,
    captures: PathBuf,
    current: Option<Box<dyn Capture>>,
    samples_open: Option<String>,
}

impl<L: Launcher> Supervisor<L> {
    pub fn new(launcher: L, captures: PathBuf) -> Self {
        Supervisor { launcher, captures, current: None, samples_open: None }
    }

    /// Open the samples stream of `session`: the file it is written to. The session must be the capture
    /// that is running, without a stream already open, and its samples file must not exist.
    pub fn begin_samples(&mut self, session: &str) -> Result<PathBuf, String> {
        self.reap();
        let tracy = self.captures.join(format!("{session}.tracy"));
        match &self.current {
            Some(c) if c.path() == tracy => {}
            Some(c) => return Err(format!("samples for {session}, but the capture running is {}", c.path().display())),
            None => return Err(format!("samples for {session}, but no capture is running")),
        }
        if let Some(open) = &self.samples_open {
            return Err(format!("the samples stream of {open} is already open"));
        }
        let path = self.captures.join(format!("{session}.samples"));
        if path.exists() {
            return Err(format!("{} exists; a session never overwrites a capture", path.display()));
        }
        self.samples_open = Some(session.to_owned());
        Ok(path)
    }

    /// The samples stream of `session` has ended.
    pub fn end_samples(&mut self, session: &str) {
        if self.samples_open.as_deref() == Some(session) {
            self.samples_open = None;
        }
    }

    /// The running capture, after dropping one that ended by itself; `Some(text)` describes the one that
    /// ended.
    fn reap(&mut self) -> Option<String> {
        let ended = self.current.as_mut().is_some_and(|c| !c.running());
        if !ended {
            return None;
        }
        let c = self.current.take().expect("checked above");
        let path = c.path().display().to_string();
        Some(match c.stop() {
            Ok(saved) => format!("the capture of {path} ended by itself: {saved}"),
            Err(e) => format!("the capture of {path} ended by itself and failed: {e}"),
        })
    }

    pub fn handle(&mut self, req: &Request) -> Reply {
        let ended = self.reap();
        match req {
            Request::Start { session } => {
                if let Some(c) = &self.current {
                    return Reply::Err(format!("a capture is already running: {}", c.path().display()));
                }
                let path = self.captures.join(format!("{session}.tracy"));
                if path.exists() {
                    return Reply::Err(format!("{} exists; a session never overwrites a capture", path.display()));
                }
                match self.launcher.start(&path) {
                    Ok(c) => {
                        self.current = Some(c);
                        Reply::Ok(format!("recording {session}.tracy"))
                    }
                    Err(e) => Reply::Err(format!("starting the capture of {session}.tracy: {e}")),
                }
            }
            Request::Samples { session } => Reply::Err(format!("SAMPLES {session} is a stream; it is not answered here")),
            Request::Stop if self.samples_open.is_some() => Reply::Err(format!(
                "the samples stream of {} is still open; it ends before STOP",
                self.samples_open.as_deref().unwrap_or_default()
            )),
            Request::Stop => match self.current.take() {
                Some(c) => match c.stop() {
                    Ok(saved) => Reply::Ok(saved),
                    Err(e) => Reply::Err(e),
                },
                None => Reply::Err(ended.unwrap_or_else(|| "no capture is running".into())),
            },
            Request::Status => match (&self.current, ended) {
                (Some(c), _) => Reply::Ok(format!("recording {}", c.path().display())),
                (None, Some(e)) => Reply::Ok(format!("idle; {e}")),
                (None, None) => Reply::Ok("idle".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A capture that runs until `alive` is cleared or it is stopped.
    struct Fake {
        path: PathBuf,
        alive: Arc<AtomicBool>,
    }

    impl Capture for Fake {
        fn path(&self) -> &Path {
            &self.path
        }
        fn running(&mut self) -> bool {
            self.alive.load(Ordering::Relaxed)
        }
        fn stop(self: Box<Self>) -> Result<String, String> {
            Ok(format!("saved {}", self.path.display()))
        }
    }

    #[derive(Default)]
    struct FakeLauncher {
        alive: Arc<AtomicBool>,
        refuse: Option<String>,
    }

    impl Launcher for FakeLauncher {
        fn start(&self, path: &Path) -> Result<Box<dyn Capture>, String> {
            if let Some(e) = &self.refuse {
                return Err(e.clone());
            }
            self.alive.store(true, Ordering::Relaxed);
            Ok(Box::new(Fake { path: path.to_owned(), alive: self.alive.clone() }))
        }
    }

    fn start(s: &str) -> Request {
        Request::Start { session: s.into() }
    }

    fn dir() -> PathBuf {
        std::env::temp_dir().join(format!("mercs2_profd_test_{}", std::process::id()))
    }

    #[test]
    fn one_capture_at_a_time_each_to_its_own_file() {
        let mut s = Supervisor::new(FakeLauncher::default(), dir());
        assert_eq!(s.handle(&Request::Status), Reply::Ok("idle".into()));
        assert_eq!(s.handle(&start("a")), Reply::Ok("recording a.tracy".into()));
        assert!(matches!(s.handle(&start("b")), Reply::Err(e) if e.contains("already running")));
        assert!(matches!(s.handle(&Request::Status), Reply::Ok(m) if m.contains("a.tracy")));
        assert!(matches!(s.handle(&Request::Stop), Reply::Ok(m) if m.contains("a.tracy")));
        assert_eq!(s.handle(&start("b")), Reply::Ok("recording b.tracy".into()));
    }

    #[test]
    fn stop_without_a_capture_is_an_error() {
        let mut s = Supervisor::new(FakeLauncher::default(), dir());
        assert_eq!(s.handle(&Request::Stop), Reply::Err("no capture is running".into()));
    }

    #[test]
    fn a_capture_that_ended_by_itself_is_reported_and_cleared() {
        let launcher = FakeLauncher::default();
        let alive = launcher.alive.clone();
        let mut s = Supervisor::new(launcher, dir());
        s.handle(&start("a"));
        alive.store(false, Ordering::Relaxed);
        assert!(matches!(s.handle(&Request::Stop), Reply::Err(e) if e.contains("ended by itself")));
        assert_eq!(s.handle(&Request::Status), Reply::Ok("idle".into()));
    }

    #[test]
    fn a_failed_start_leaves_no_capture() {
        let launcher = FakeLauncher { refuse: Some("no game listening".into()), ..Default::default() };
        let mut s = Supervisor::new(launcher, dir());
        assert!(matches!(s.handle(&start("a")), Reply::Err(e) if e.contains("no game listening")));
        assert_eq!(s.handle(&Request::Status), Reply::Ok("idle".into()));
    }

    #[test]
    fn a_samples_stream_belongs_to_the_running_capture_and_ends_before_stop() {
        let mut s = Supervisor::new(FakeLauncher::default(), dir());
        assert!(s.begin_samples("a").unwrap_err().contains("no capture is running"));
        s.handle(&start("a"));
        assert!(s.begin_samples("b").unwrap_err().contains("the capture running is"));
        assert_eq!(s.begin_samples("a").unwrap(), dir().join("a.samples"));
        assert!(s.begin_samples("a").unwrap_err().contains("already open"));
        assert!(matches!(s.handle(&Request::Stop), Reply::Err(e) if e.contains("still open")));
        s.end_samples("a");
        assert!(matches!(s.handle(&Request::Stop), Reply::Ok(_)));
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let d = dir();
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("taken.tracy"), b"x").unwrap();
        let mut s = Supervisor::new(FakeLauncher::default(), d.clone());
        assert!(matches!(s.handle(&start("taken")), Reply::Err(e) if e.contains("never overwrites")));
        std::fs::remove_file(d.join("taken.tracy")).unwrap();
    }
}
