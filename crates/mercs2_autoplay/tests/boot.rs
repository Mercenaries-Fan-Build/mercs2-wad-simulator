//! Game-gated: reads the retail `vz.wad` named by the repo-root `.mercs2-local.toml` and fails when it
//! is absent. Run with `cargo xtask retail-test`.

use std::path::{Path, PathBuf};

use mercs2_autoplay::boot::{boot_new_game, Hero, Session};
use mercs2_autoplay::tasks::{self, TaskState};
use serde_json::Value as Json;

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn trace_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn boot(hero: Hero, trace: &Path) -> Session {
    boot_new_game(&vz_wad(), hero, trace).unwrap_or_else(|e| panic!("boot: {e}"))
}

fn records(trace: &Path) -> Vec<Json> {
    let text = std::fs::read_to_string(trace).unwrap_or_else(|e| panic!("read {}: {e}", trace.display()));
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad trace line {l:?}: {e}")))
        .collect()
}

fn str_arg(rec: &Json, i: usize) -> Option<&str> {
    let a = rec.get("args")?.get(i)?;
    (a.get("t")?.as_str()? == "str").then(|| a.get("v")?.as_str()).flatten()
}

/// A new game reaches the opening contract's first objective, live in the task tree.
///
/// `boot_new_game` only returns `Ok` once the world load reached `GlobalExit`; the tree then has to
/// show what the shipped Lua does next: the `Start` flow binding unlocks `VzaCon001`
/// (`wifmissionflow.lua:42`), and the mission posts `VZA001: Go to the Beach` as an active child.
#[test]
fn new_game_reaches_the_first_objective() {
    let session = boot(Hero::Chris, &trace_path("first_objective.ndjson"));
    let nodes = tasks::snapshot(&session).unwrap_or_else(|e| panic!("task tree: {e}"));

    let objective = nodes
        .iter()
        .find(|n| n.lineage.ends_with("VzaCon001Mission.VZA001: Go to the Beach"))
        .unwrap_or_else(|| panic!("no VZA001 objective in the task tree: {nodes:#?}"));
    assert_eq!(objective.state, TaskState::Active, "the first objective must be live: {objective:?}");
}

/// The trace carries all three call shapes, each pinned to a shipped source line.
#[test]
fn trace_records_engine_cross_module_and_engine_dispatched_calls() {
    let path = trace_path("shapes.ndjson");
    let session = boot(Hero::Chris, &path);
    let recs = records(&path);

    // Every record is numbered in order with no gaps, and the counts match what was written.
    for (i, r) in recs.iter().enumerate() {
        assert_eq!(r["seq"].as_u64(), Some(i as u64 + 1), "seq gap at line {}: {r}", i + 1);
    }
    let c = session.trace.counts();
    assert_eq!((c.engine + c.lua + c.stub) as usize, recs.len(), "counts {c:?} vs {} lines", recs.len());

    // Engine call: the boat-wake gate `vzacon001.lua:120`,
    // `Event.Create(Event.ObjectHibernation, {uBoat, "a"}, _PutPlayersInBoat, {uBoat})`.
    let gate = recs
        .iter()
        .find(|r| r["kind"] == "engine" && r["ns"] == "Event" && r["fn"] == "Create" && r["caller_src"] == "@vzacon001:120")
        .unwrap_or_else(|| panic!("no Event.Create from @vzacon001:120"));
    assert_eq!(gate["argc"], 4, "Create(kind, params, callback, args): {gate}");
    assert_eq!(gate["args"][2]["t"], "tt6", "the callback is a function: {gate}");

    // Cross-module call: the `Start` binding's `UnlockMission("VzaCon001")` (`wifmissionflow.lua:42`),
    // defined in the base module `mrxmissionflow.lua:115`.
    let unlock = recs
        .iter()
        .find(|r| r["kind"] == "lua" && r["fn"] == "UnlockMission" && r["caller_src"] == "@wifmissionflow:42")
        .unwrap_or_else(|| panic!("no UnlockMission from @wifmissionflow:42"));
    assert_eq!(unlock["callee"], "@mrxmissionflow:115", "{unlock}");
    assert_eq!(str_arg(unlock, 0), Some("VzaCon001"), "{unlock}");

    // Engine-dispatched call: the woken boat fires `_PutPlayersInBoat` (`vzacon001.lua:84`) from the
    // engine's hibernation pump, with no Lua frame above it. It is a `local function`, so it has no
    // module-level name and no call-site name: the source location is its identity.
    let dispatched = recs
        .iter()
        .find(|r| r["kind"] == "lua" && r["callee"] == "@vzacon001:84")
        .unwrap_or_else(|| panic!("no call into @vzacon001:84 (_PutPlayersInBoat) recorded"));
    assert_eq!(dispatched["caller_src"], "", "dispatched by the engine, not by Lua: {dispatched}");
    assert_eq!(dispatched["fn"], Json::Null, "a local function has no module-level name: {dispatched}");
    assert_eq!(dispatched["args"][0]["t"], "tt2", "its argument is the boat's guid: {dispatched}");
}

/// Two boots of the same hero write byte-identical traces. The explorer's replay and its fork
/// children both depend on the scripts doing the same thing given the same inputs.
#[test]
fn trace_is_deterministic() {
    let a = trace_path("determinism_a.ndjson");
    let b = trace_path("determinism_b.ndjson");
    drop(boot(Hero::Mattias, &a));
    drop(boot(Hero::Mattias, &b));
    let (ta, tb) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    if ta != tb {
        let (la, lb): (Vec<&[u8]>, Vec<&[u8]>) = (ta.split(|c| *c == b'\n').collect(), tb.split(|c| *c == b'\n').collect());
        let first = la.iter().zip(&lb).position(|(x, y)| x != y).unwrap_or(la.len().min(lb.len()));
        panic!(
            "traces differ at line {} ({} vs {} lines):\n  a: {}\n  b: {}",
            first + 1,
            la.len(),
            lb.len(),
            la.get(first).map(|l| String::from_utf8_lossy(l)).unwrap_or_default(),
            lb.get(first).map(|l| String::from_utf8_lossy(l)).unwrap_or_default()
        );
    }
}
