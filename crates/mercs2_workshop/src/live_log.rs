//! The Live page's tail of `pmc_blackbox.log`, read on its own thread.
//!
//! The log is parsed with `loadprobe::parse`, and the furthest load rung comes from `loadprobe`'s ladder,
//! so the page and `loadprobe` cannot disagree about either. The game rewrites the file each run, so a
//! file that shrinks starts the tail over.

use loadprobe::phases::LADDER;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// How often the file is checked.
const POLL: Duration = Duration::from_millis(250);
/// The lines kept for the view.
const KEEP: usize = 400;
/// How long a file may go without growing before the row reads stale.
const STALE: Duration = Duration::from_secs(5);

/// One line of the tail.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub ts: String,
    pub source: String,
    pub msg: String,
}

/// How the log stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogState {
    /// No file at the path.
    #[default]
    Missing,
    Growing,
    Stale,
}

/// What the tail knows.
#[derive(Debug, Clone, Default)]
pub struct Tail {
    pub state: LogState,
    pub lines: VecDeque<Line>,
    /// The furthest ladder rung this run reached, and its name.
    pub rung: Option<(usize, &'static str)>,
    /// Since the file last changed.
    pub age: Option<Duration>,
    /// Bumped on every new line, so the view knows to follow.
    pub total: u64,
    /// A debugger process is running. Whether it has the game is not known from here.
    pub debugger: bool,
}

impl Tail {
    /// Take the bytes the file has gained: whole lines only, `partial` holding the rest.
    fn take(&mut self, new: &str) {
        for l in loadprobe::parse::parse_log(new) {
            self.total += 1;
            if let Some(p) = LADDER.iter().rev().find(|p| p.matches.iter().any(|m| l.msg.contains(m))) {
                if self.rung.is_none_or(|(i, _)| p.idx > i) {
                    self.rung = Some((p.idx, p.name));
                }
            }
            if self.lines.len() == KEEP {
                self.lines.pop_front();
            }
            self.lines.push_back(Line { ts: l.raw_ts, source: l.source, msg: l.msg });
        }
    }
}

/// The tail's handle; the thread lives as long as the app.
pub struct LogTail {
    tail: Arc<Mutex<Tail>>,
    open: Arc<AtomicBool>,
    path: Arc<Mutex<Option<PathBuf>>>,
}

impl LogTail {
    pub fn spawn() -> LogTail {
        let tail = Arc::new(Mutex::new(Tail::default()));
        let open = Arc::new(AtomicBool::new(false));
        let path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let (t, o, p) = (tail.clone(), open.clone(), path.clone());
        std::thread::Builder::new()
            .name("live-log".into())
            .spawn(move || follow(t, o, p))
            .expect("spawn the live log thread");
        LogTail { tail, open, path }
    }

    /// Follow `path` while the page is open.
    pub fn set(&self, open: bool, path: Option<PathBuf>) {
        self.open.store(open, Ordering::Relaxed);
        *self.path.lock().expect("the log path") = path;
    }

    pub fn snapshot(&self) -> Tail {
        self.tail.lock().expect("the log tail").clone()
    }
}

fn follow(tail: Arc<Mutex<Tail>>, open: Arc<AtomicBool>, path: Arc<Mutex<Option<PathBuf>>>) {
    let mut reader = Reader::default();
    let mut polls = 0u32;
    loop {
        std::thread::sleep(POLL);
        if !open.load(Ordering::Relaxed) {
            continue;
        }
        if polls % DEBUGGER_EVERY == 0 {
            tail.lock().expect("the log tail").debugger = debugger_running();
        }
        polls = polls.wrapping_add(1);
        let Some(p) = path.lock().expect("the log path").clone() else { continue };
        reader.step(&p, &tail);
    }
}

/// How many polls pass between looks at the process table.
const DEBUGGER_EVERY: u32 = 12;

/// Whether `x32dbg` appears in the process table, Wine's included: the workshop has no channel to the
/// debugger itself. A process table that cannot be listed reads as none.
fn debugger_running() -> bool {
    let (cmd, args): (&str, &[&str]) = if cfg!(windows) { ("tasklist", &[]) } else { ("ps", &["-axo", "command="]) };
    std::process::Command::new(cmd).args(args).output().is_ok_and(|o| lists_debugger(&String::from_utf8_lossy(&o.stdout)))
}

fn lists_debugger(table: &str) -> bool {
    table.to_ascii_lowercase().contains("x32dbg")
}

/// How far the file has been read.
#[derive(Default)]
struct Reader {
    read: u64,
    partial: Vec<u8>,
}

impl Reader {
    fn step(&mut self, path: &std::path::Path, tail: &Mutex<Tail>) {
        let Ok(meta) = std::fs::metadata(path) else {
            *self = Reader::default();
            *tail.lock().expect("the log tail") = Tail::default();
            return;
        };
        let len = meta.len();
        let age = meta.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok());
        if len < self.read {
            *self = Reader::default();
            *tail.lock().expect("the log tail") = Tail::default();
        }
        if len > self.read {
            use std::io::{Read, Seek, SeekFrom};
            let mut new = Vec::with_capacity((len - self.read) as usize);
            let opened = std::fs::File::open(path).and_then(|mut f| {
                f.seek(SeekFrom::Start(self.read))?;
                f.take(len - self.read).read_to_end(&mut new)
            });
            if let Ok(n) = opened {
                self.read += n as u64;
                self.partial.extend_from_slice(&new);
                if let Some(cut) = self.partial.iter().rposition(|&b| b == b'\n') {
                    let whole: Vec<u8> = self.partial.drain(..=cut).collect();
                    tail.lock().expect("the log tail").take(&String::from_utf8_lossy(&whole));
                }
            }
        }
        let mut t = tail.lock().expect("the log tail");
        t.age = age;
        t.state = if age.is_some_and(|a| a <= STALE) { LogState::Growing } else { LogState::Stale };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("live_log_{name}_{}.log", std::process::id()))
    }

    fn append(p: &std::path::Path, s: &str) {
        std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap().write_all(s.as_bytes()).unwrap();
    }

    #[test]
    fn it_reads_lines_as_they_arrive_and_holds_a_partial_one() {
        let p = temp("grow");
        std::fs::write(&p, "").unwrap();
        let tail = Mutex::new(Tail::default());
        let mut r = Reader::default();
        append(&p, "[12:04:17.221] [world] one\n[12:04:17.2");
        r.step(&p, &tail);
        assert_eq!(tail.lock().unwrap().lines.len(), 1, "the partial line waits for its end");
        append(&p, "84] [lua] two  @mrxfoo:12\n");
        r.step(&p, &tail);
        let t = tail.lock().unwrap();
        assert_eq!(t.lines[1], Line { ts: "12:04:17.284".into(), source: "lua".into(), msg: "two".into() });
        assert_eq!(t.state, LogState::Growing);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn a_new_run_starts_the_tail_over() {
        let p = temp("trunc");
        std::fs::write(&p, "[12:04:17.221] [world] Loading vz level with vz masterscript\n[12:04:18.000] [lua] after\n").unwrap();
        let tail = Mutex::new(Tail::default());
        let mut r = Reader::default();
        r.step(&p, &tail);
        assert_eq!(tail.lock().unwrap().rung, Some((10, "WORLD LOAD START")));
        std::fs::write(&p, "[01:00:00.000] [blackbox] x\n").unwrap();
        r.step(&p, &tail);
        let t = tail.lock().unwrap();
        assert_eq!(t.lines.len(), 1);
        assert_eq!(t.rung, None, "the rung is this run's");
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn the_rung_is_the_furthest_reached() {
        let mut t = Tail::default();
        t.take("[00:00:01.000] [lua] GlobalExit - Complete\n[00:00:02.000] [lua] STATE_WAITFORGAME (refcount=1)\n");
        assert_eq!(t.rung, Some((20, "World fully loaded (GlobalExit)")));
    }

    #[test]
    fn the_process_table_finds_a_debugger_in_any_case() {
        assert!(lists_debugger("wine-preloader C:\\tools\\X32DBG.exe"));
        assert!(!lists_debugger("wine-preloader Mercenaries2.exe"));
    }

    #[test]
    fn no_file_is_missing_not_empty() {
        let tail = Mutex::new(Tail::default());
        Reader::default().step(&temp("absent"), &tail);
        assert_eq!(tail.lock().unwrap().state, LogState::Missing);
    }

    #[test]
    fn the_view_keeps_the_newest_lines() {
        let mut t = Tail::default();
        let many: String = (0..KEEP + 5).map(|i| format!("[00:00:00.000] [lua] line{i}\n")).collect();
        t.take(&many);
        assert_eq!(t.lines.len(), KEEP);
        assert_eq!(t.lines.back().unwrap().msg, format!("line{}", KEEP + 4));
    }
}
