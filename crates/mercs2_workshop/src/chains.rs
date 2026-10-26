//! Where the game archive and the reference bundle come from: two chains of sources, first match
//! wins.
//!
//! A source a person set — `--wad`, this run's choice, the saved setting, an environment variable —
//! that turns out broken STOPS its chain. It is never passed over for the next source, because the
//! next source is not what they asked for: that silent step is how a test run once read a stale
//! bundle from the download cache while `MERCS2_WORKSHOP_DATA` pointed at a folder that had lost
//! its files. The searched steps (beside the exe, the working directory, the registry, the
//! download cache) are passed over when they find nothing.
//!
//! The [`Chain`] rows are what the Settings page draws and what a headless run prints when a chain
//! is unresolved. [`game_chain`] and [`data_chain`] are pure over their inputs (plus file checks);
//! [`game`] and [`data`] read the real environment, settings and run overrides.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One source's state in its chain.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// The first usable source: the chain's answer.
    InUse,
    /// A source that was set but cannot be used. `tag` is the badge (`FILE MISSING`), `why` the
    /// sentence naming the path.
    Broken { tag: &'static str, why: String },
    /// A source a person sets, not set.
    NotSet,
    /// A searched step that found nothing.
    NotFound,
    /// Below the source in use, or below the broken source that stopped the chain.
    NotReached,
    /// A step this platform does not have (the registry off Windows).
    NotOnPlatform,
}

/// One step of a chain, as drawn.
#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    /// The path the source names, or, in use, the path it resolved to.
    pub path: Option<PathBuf>,
    pub status: Status,
}

/// A resolved chain: one row per step, in precedence order.
#[derive(Clone, Debug)]
pub struct Chain {
    pub rows: Vec<Row>,
}

impl Chain {
    /// The chain's answer, if it has one.
    pub fn in_use(&self) -> Option<&Path> {
        self.rows.iter().find(|r| r.status == Status::InUse).and_then(|r| r.path.as_deref())
    }

    /// The broken source that left the chain without an answer.
    pub fn blocker(&self) -> Option<&Row> {
        if self.in_use().is_some() {
            return None;
        }
        self.rows.iter().find(|r| matches!(r.status, Status::Broken { .. }))
    }

    /// A source below the one in use that is set but broken: worth a look, not blocking.
    pub fn attention(&self) -> Option<&Row> {
        self.in_use()?;
        self.rows.iter().find(|r| matches!(r.status, Status::Broken { .. }))
    }

    /// Nothing was set and nothing was found — the one state the reference data downloads in.
    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|r| matches!(r.status, Status::NotSet | Status::NotFound | Status::NotOnPlatform))
    }

    /// Opening the source in use failed: it becomes the source that stops the chain.
    pub fn fail_in_use(&mut self, tag: &'static str, why: String) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.status == Status::InUse) {
            r.status = Status::Broken { tag, why };
        }
    }

    /// Why the chain has no answer, with every step, for a headless run's stderr and an error
    /// value. `what` names the chain ("the game archive").
    pub fn unresolved(&self, what: &str) -> String {
        let head = match self.blocker() {
            Some(r) => match &r.status {
                Status::Broken { why, .. } => format!("{what} is not usable: {} — {why}", r.label),
                _ => unreachable!("a blocker is broken"),
            },
            None => format!("{what} is not set and was not found"),
        };
        let mut out = head;
        for (i, r) in self.rows.iter().enumerate() {
            let state = match &r.status {
                Status::InUse => "in use".to_string(),
                Status::Broken { tag, .. } => tag.to_string(),
                Status::NotSet => "not set".to_string(),
                Status::NotFound => "not found".to_string(),
                Status::NotReached => "not reached".to_string(),
                Status::NotOnPlatform => "not on this platform".to_string(),
            };
            let path = r.path.as_ref().map(|p| format!("  {}", p.display())).unwrap_or_default();
            out.push_str(&format!("\n  {} {:<26} {state}{path}", i + 1, r.label));
        }
        out
    }
}

/// One step, before resolution.
pub enum Step {
    /// Set by a person; `None` = not set. Broken when set, it stops the chain.
    Set { label: String, set: Option<PathBuf> },
    /// Searched for; `None` = nothing found. A found path is already usable.
    Found { label: String, found: Option<PathBuf> },
    /// A step this platform does not have.
    Absent { label: String },
}

/// Resolve `steps` in order. `check` turns a set path into the usable path, or the badge and the
/// reason it is broken.
pub fn resolve(steps: Vec<Step>, check: impl Fn(&Path) -> Result<PathBuf, (&'static str, String)>) -> Chain {
    let mut rows = Vec::new();
    // Whether an earlier step answered (`in_use`) or stopped the chain (`stopped`).
    let (mut in_use, mut stopped) = (false, false);
    for step in steps {
        let done = in_use || stopped;
        rows.push(match step {
            Step::Set { label, set: None } => {
                Row { label, path: None, status: if done { Status::NotReached } else { Status::NotSet } }
            }
            Step::Set { label, set: Some(p) } => match check(&p) {
                Ok(_) if done => Row { label, path: Some(p), status: Status::NotReached },
                Ok(r) => {
                    in_use = true;
                    Row { label, path: Some(r), status: Status::InUse }
                }
                // Below the source in use, a broken set source is still reported: it is the
                // attention the Settings page flags. Below a stop, nothing is looked at.
                Err((tag, why)) if !stopped => {
                    stopped = !in_use;
                    Row { label, path: Some(p), status: Status::Broken { tag, why } }
                }
                Err(_) => Row { label, path: Some(p), status: Status::NotReached },
            },
            Step::Found { label, found } => match found {
                _ if done => Row { label, path: None, status: Status::NotReached },
                Some(p) => {
                    in_use = true;
                    Row { label, path: Some(p), status: Status::InUse }
                }
                None => Row { label, path: None, status: Status::NotFound },
            },
            Step::Absent { label } => Row { label, path: None, status: Status::NotOnPlatform },
        });
    }
    Chain { rows }
}

/// The `--wad` step's label.
pub const FLAG: &str = "--wad";
/// The label of the step a Settings "Use for this run only…" fills.
pub const RUN: &str = "this run's choice";
/// The label of the step `settings.json` fills.
pub const SAVED: &str = "saved setting";

impl Chain {
    /// The row a step's label names.
    pub fn row(&self, label: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.label == label)
    }
}

/// What the game-archive chain is resolved from.
pub struct GameInputs {
    /// `--wad`.
    pub flag: Option<PathBuf>,
    /// This run's choice (Settings, "Use for this run only…").
    pub run: Option<PathBuf>,
    /// `settings.json` `wad_path`.
    pub saved: Option<PathBuf>,
    /// An install the workshop sits inside ([`colocated_vz_wad`]).
    pub colocated: Option<PathBuf>,
    /// The game-directory environment variables that are set, in precedence order.
    pub env: Vec<(&'static str, PathBuf)>,
    /// The Windows registry key's archive; `None` off Windows.
    pub registry: Option<Option<PathBuf>>,
}

/// A path a person gave for the game: the install root, its `data` folder, or the archive itself.
fn check_wad(p: &Path) -> Result<PathBuf, (&'static str, String)> {
    mercs2_formats::game_paths::wad_under(p, "vz.wad").ok_or_else(|| {
        ("FILE MISSING", format!("no vz.wad at {} or in its data folder", p.display()))
    })
}

/// The game-archive chain: `--wad` → this run's choice → saved → beside the exe → environment →
/// registry.
pub fn game_chain(i: GameInputs) -> Chain {
    use mercs2_formats::game_paths::GAME_DIR_VARS;
    // The first variable that is set is the source; a second one set below it is not consulted.
    let (env_label, env_set) = match i.env.into_iter().next() {
        Some((name, p)) => (name.to_string(), Some(p)),
        None => (GAME_DIR_VARS.join(" / "), None),
    };
    let registry = match i.registry {
        Some(found) => Step::Found { label: "Windows registry".into(), found },
        None => Step::Absent { label: "Windows registry".into() },
    };
    resolve(
        vec![
            Step::Set { label: FLAG.into(), set: i.flag },
            Step::Set { label: RUN.into(), set: i.run },
            Step::Set { label: SAVED.into(), set: i.saved },
            Step::Found { label: "beside the exe".into(), found: i.colocated },
            Step::Set { label: env_label, set: env_set },
            registry,
        ],
        check_wad,
    )
}

/// What the reference-data chain is resolved from.
pub struct DataInputs {
    /// This run's choice (Settings, "Use for this run only…").
    pub run: Option<PathBuf>,
    /// `MERCS2_WORKSHOP_DATA`, when set and not empty.
    pub env: Option<PathBuf>,
    /// `settings.json` `data_dir`.
    pub saved: Option<PathBuf>,
    /// The executable's folder.
    pub exe_dir: Option<PathBuf>,
    /// The working directory.
    pub cwd: Option<PathBuf>,
    /// Where the download installs the bundle ([`crate::fetch::installed_dir`]).
    pub cache: Option<PathBuf>,
}

/// The environment variable naming the reference bundle.
pub const DATA_ENV: &str = "MERCS2_WORKSHOP_DATA";

/// Whether `dir` is a complete reference bundle: it holds [`crate::index::BUNDLE_MANIFEST`], which
/// `--pack-data` writes after every other file.
pub fn is_complete_bundle(dir: &Path) -> bool {
    dir.join(crate::index::BUNDLE_MANIFEST).is_file()
}

/// A folder a person gave for the reference bundle.
fn check_data(p: &Path) -> Result<PathBuf, (&'static str, String)> {
    if !p.is_dir() {
        return Err(("MISSING", format!("{} does not exist", p.display())));
    }
    if !is_complete_bundle(p) {
        return Err((
            "NO MANIFEST",
            format!("{} is not a complete reference bundle (no {})", p.display(), crate::index::BUNDLE_MANIFEST),
        ));
    }
    Ok(p.to_path_buf())
}

/// The reference-data chain: this run's choice → `MERCS2_WORKSHOP_DATA` → saved → beside the exe →
/// the working directory or a parent → the downloaded copy. The committed, partial `workshop_data/`
/// of a repo checkout has no manifest and is passed over by the searched steps.
pub fn data_chain(i: DataInputs) -> Chain {
    let complete = |d: PathBuf| is_complete_bundle(&d).then_some(d);
    let beside = i.exe_dir.map(|d| d.join("workshop_data")).and_then(complete);
    let walk = i.cwd.and_then(|c| c.ancestors().map(|d| d.join("workshop_data")).find(|d| is_complete_bundle(d)));
    resolve(
        vec![
            Step::Set { label: RUN.into(), set: i.run },
            Step::Set { label: DATA_ENV.into(), set: i.env },
            Step::Set { label: SAVED.into(), set: i.saved },
            Step::Found { label: "beside the exe".into(), found: beside },
            Step::Found { label: "working directory".into(), found: walk },
            Step::Found { label: "downloaded copy".into(), found: i.cache.and_then(complete) },
        ],
        check_data,
    )
}

/// This run's choices, set from Settings ("Use for this run only…"). Process-wide because the
/// reference data is resolved from worker threads as well as the UI.
static RUN_WAD: Mutex<Option<PathBuf>> = Mutex::new(None);
static RUN_DATA: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Use `p` as the game archive for the rest of this run, ahead of the saved setting.
pub fn set_run_wad(p: PathBuf) {
    *RUN_WAD.lock().expect("run override lock") = Some(p);
}

/// Use `p` as the reference bundle for the rest of this run, ahead of the environment and the
/// saved setting.
pub fn set_run_data(p: PathBuf) {
    *RUN_DATA.lock().expect("run override lock") = Some(p);
}

/// `vz.wad` from an install the Workshop is sitting INSIDE — the executable's own directory first,
/// then the working directory.
///
/// Plan 02 rev 2's resolution step 2. Each candidate has to carry `Mercenaries2.exe` before its
/// archive counts: the exe is what makes a folder an install, whereas a bare `vz.wad` beside the
/// binary could be a copy someone was extracting from. Both the root and its `data/` are probed,
/// via `game_paths::wad_under` — the shared rule for what "a path to the game" means, so this does
/// not become a fourth private copy of it.
pub fn colocated_vz_wad() -> Option<PathBuf> {
    use mercs2_formats::game_paths::wad_under;
    let roots = [
        std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())),
        std::env::current_dir().ok(),
    ];
    roots
        .into_iter()
        .flatten()
        .filter(|root| root.join("Mercenaries2.exe").is_file())
        .find_map(|root| wad_under(&root, "vz.wad"))
}

/// An environment variable's value as a path, when set and not empty.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// The game-archive chain as this process sees it, with `flag` the `--wad` value.
pub fn game(flag: Option<&str>) -> Chain {
    let registry = if cfg!(windows) {
        Some(mercs2_engine::wad::registry_vz_wad().map(PathBuf::from))
    } else {
        None
    };
    game_chain(GameInputs {
        flag: flag.filter(|s| !s.is_empty()).map(PathBuf::from),
        run: RUN_WAD.lock().expect("run override lock").clone(),
        saved: crate::settings::load().wad_path,
        colocated: colocated_vz_wad(),
        env: mercs2_formats::game_paths::GAME_DIR_VARS
            .iter()
            .filter_map(|v| env_path(v).map(|p| (*v, p)))
            .collect(),
        registry,
    })
}

/// The reference-data chain as this process sees it.
pub fn data() -> Chain {
    data_chain(DataInputs {
        run: RUN_DATA.lock().expect("run override lock").clone(),
        env: env_path(DATA_ENV),
        saved: crate::settings::load().data_dir,
        exe_dir: std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())),
        cwd: std::env::current_dir().ok(),
        cache: crate::fetch::installed_dir(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the temp dir, unique to this test and process.
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_chains_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    /// A complete bundle folder at `dir`.
    fn bundle(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).expect("bundle dir");
        std::fs::write(dir.join(crate::index::BUNDLE_MANIFEST), "{}").expect("manifest");
        dir.to_path_buf()
    }

    /// An install root holding `data/vz.wad`.
    fn install(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("data")).expect("install dir");
        std::fs::write(dir.join("data/vz.wad"), b"FFCS").expect("vz.wad");
        dir.join("data/vz.wad")
    }

    fn no_game() -> GameInputs {
        GameInputs { flag: None, run: None, saved: None, colocated: None, env: Vec::new(), registry: None }
    }

    fn no_data() -> DataInputs {
        DataInputs { run: None, env: None, saved: None, exe_dir: None, cwd: None, cache: None }
    }

    fn statuses(c: &Chain) -> Vec<&Status> {
        c.rows.iter().map(|r| &r.status).collect()
    }

    #[test]
    fn a_saved_archive_that_is_gone_stops_the_chain_instead_of_passing_to_the_environment() {
        let d = scratch("saved_gone");
        let good = install(&d.join("game"));
        let c = game_chain(GameInputs {
            saved: Some(d.join("moved/vz.wad")),
            env: vec![("MERCS2_GAME_DIR", d.join("game"))],
            ..no_game()
        });
        assert_eq!(c.in_use(), None, "the environment's {} must not be used", good.display());
        let b = c.blocker().expect("the saved setting blocks");
        assert_eq!(b.label, "saved setting");
        assert!(matches!(b.status, Status::Broken { tag: "FILE MISSING", .. }));
        assert_eq!(c.rows[4].status, Status::NotReached, "the environment is not reached");
        assert!(!c.is_empty());
        assert!(c.unresolved("the game archive").contains("saved setting"));
    }

    #[test]
    fn the_wad_flag_wins_and_a_missing_saved_path_below_it_is_flagged_not_blocking() {
        let d = scratch("flag_wins");
        let good = install(&d.join("game"));
        let c = game_chain(GameInputs {
            flag: Some(d.join("game")),
            saved: Some(d.join("moved/vz.wad")),
            registry: Some(None),
            ..no_game()
        });
        assert_eq!(c.in_use(), Some(good.as_path()), "an install root resolves to its data/vz.wad");
        assert_eq!(c.attention().map(|r| r.label.as_str()), Some("saved setting"));
        assert!(c.blocker().is_none());
        assert_eq!(
            statuses(&c)[3..],
            [&Status::NotReached, &Status::NotReached, &Status::NotReached],
            "beside the exe, the environment and the registry are not reached"
        );
    }

    #[test]
    fn this_runs_choice_beats_the_saved_setting() {
        let d = scratch("run_wins");
        let run = install(&d.join("run"));
        let saved = install(&d.join("saved"));
        let c = game_chain(GameInputs { run: Some(run.clone()), saved: Some(saved), ..no_game() });
        assert_eq!(c.in_use(), Some(run.as_path()));
        assert_eq!(c.rows[2].status, Status::NotReached);
    }

    #[test]
    fn searched_steps_that_find_nothing_are_passed_over() {
        let d = scratch("searched");
        let found = install(&d.join("game"));
        let c = game_chain(GameInputs { registry: Some(Some(found.clone())), ..no_game() });
        assert_eq!(c.in_use(), Some(found.as_path()));
        assert_eq!(c.rows[3].status, Status::NotFound, "beside the exe found nothing");
        assert_eq!(c.rows[4].status, Status::NotSet, "no environment variable is set");
        let none = game_chain(no_game());
        assert!(none.is_empty() && none.in_use().is_none() && none.blocker().is_none());
        assert_eq!(none.rows[5].status, Status::NotOnPlatform);
    }

    #[test]
    fn an_incomplete_bundle_in_the_environment_stops_the_chain_before_the_downloaded_copy() {
        let d = scratch("env_incomplete");
        std::fs::create_dir_all(d.join("partial")).expect("partial");
        let cache = bundle(&d.join("cache"));
        let c = data_chain(DataInputs { env: Some(d.join("partial")), cache: Some(cache.clone()), ..no_data() });
        assert_eq!(c.in_use(), None, "the downloaded copy {} must not be used", cache.display());
        let b = c.blocker().expect("the environment blocks");
        assert_eq!(b.label, DATA_ENV);
        assert!(matches!(b.status, Status::Broken { tag: "NO MANIFEST", .. }));
        assert_eq!(c.rows[5].status, Status::NotReached);
    }

    #[test]
    fn this_runs_data_choice_wins_over_a_broken_environment_variable() {
        let d = scratch("run_data");
        let run = bundle(&d.join("run"));
        let c = data_chain(DataInputs { run: Some(run.clone()), env: Some(d.join("gone")), ..no_data() });
        assert_eq!(c.in_use(), Some(run.as_path()));
        assert!(matches!(c.attention().map(|r| &r.status), Some(Status::Broken { tag: "MISSING", .. })));
    }

    #[test]
    fn searched_bundles_count_only_when_complete() {
        let d = scratch("searched_data");
        // The repo checkout's partial workshop_data: no manifest, passed over.
        std::fs::create_dir_all(d.join("repo/workshop_data")).expect("partial");
        std::fs::create_dir_all(d.join("repo/crate")).expect("cwd");
        let cache = bundle(&d.join("cache"));
        let c = data_chain(DataInputs {
            exe_dir: Some(d.join("bin")),
            cwd: Some(d.join("repo/crate")),
            cache: Some(cache.clone()),
            ..no_data()
        });
        assert_eq!(c.in_use(), Some(cache.as_path()));
        assert_eq!(statuses(&c)[3..5], [&Status::NotFound, &Status::NotFound]);
        let walked = bundle(&d.join("repo2/workshop_data"));
        std::fs::create_dir_all(d.join("repo2/a/b")).expect("cwd");
        let c = data_chain(DataInputs { cwd: Some(d.join("repo2/a/b")), ..no_data() });
        assert_eq!(c.in_use(), Some(walked.as_path()), "the walk-up finds a parent's bundle");
    }

    #[test]
    fn nothing_set_and_nothing_found_is_the_empty_chain_that_downloads() {
        let c = data_chain(no_data());
        assert!(c.is_empty() && c.blocker().is_none() && c.in_use().is_none());
        let d = scratch("not_empty");
        let c = data_chain(DataInputs { saved: Some(d.join("gone")), ..no_data() });
        assert!(!c.is_empty(), "a broken saved folder is not a reason to download over it");
    }

    #[test]
    fn a_failed_open_turns_the_source_in_use_into_the_blocker() {
        let d = scratch("fail_in_use");
        let good = install(&d.join("game"));
        let mut c = game_chain(GameInputs { saved: Some(good), ..no_game() });
        c.fail_in_use("NOT AN ARCHIVE", "parse FFCS archive: bad magic".into());
        assert_eq!(c.in_use(), None);
        assert_eq!(c.blocker().map(|r| r.label.as_str()), Some("saved setting"));
    }
}
