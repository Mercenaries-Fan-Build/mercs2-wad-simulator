//! Integration tests against the four real captures in `storage/`. These lock in the
//! end-state classifier so the verdicts can't silently regress.
//!
//! Run from the repo root via the workspace; the fixtures are read from `../../../storage`
//! relative to this crate (tools/wad_simulator/crates/loadprobe/).

use std::path::PathBuf;
use std::process::Command;

fn storage(name: &str) -> PathBuf {
    // crate dir = tools/wad_simulator/crates/loadprobe ; repo root is 4 levels up.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..4 { p.pop(); }
    p.push("storage");
    p.push(name);
    p
}

/// Run the built binary with --json and return parsed exit code + stdout.
fn run_json(log: &str) -> (i32, String) {
    let bin = env!("CARGO_BIN_EXE_loadprobe");
    let path = storage(log);
    if !path.exists() {
        // Fixtures live outside the crate; skip gracefully if not present.
        eprintln!("skip: fixture {} not found", path.display());
        return (-1, String::new());
    }
    let out = Command::new(bin).arg("--json").arg("--no-color").arg(&path).output().expect("run loadprobe");
    let code = out.status.code().unwrap_or(-1);
    (code, String::from_utf8_lossy(&out.stdout).to_string())
}

fn field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    // tiny extractor: find "\"key\":" then the next token (string or number)
    let pat = format!("\"{}\"", key);
    let i = json.find(&pat)? + pat.len();
    let rest = json[i..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    if let Some(s) = rest.strip_prefix('"') {
        let end = s.find('"')?;
        Some(&s[..end])
    } else {
        let end = rest.find(|c: char| c == ',' || c == '}' || c.is_whitespace()).unwrap_or(rest.len());
        Some(&rest[..end])
    }
}

#[test]
fn vanilla_boots_into_game_postload_crash() {
    let (code, json) = run_json("pmc_blackbox-vanilla-boot-into-game.log");
    if code < 0 { return; }
    assert_eq!(field(&json, "kind"), Some("ReachedWorld"), "vanilla reached the world");
    assert_eq!(field(&json, "pct"), Some("100"), "vanilla loaded 100%");
    assert_eq!(code, 0, "REACHED-WORLD exit code");
    // the 0x874E7D crash is present but POST-LOAD (doesn't override the verdict)
    assert!(json.contains("874E7D") || json.contains("8749") , "crash captured in report");
}

#[test]
fn most_recent_is_hang_not_crash() {
    // The 0x874E7D at the end is a hard-close (teardown) artifact, NOT a load crash —
    // the load wedged at phase 10 (like never-finished) and the user killed it.
    let (code, json) = run_json("pmc_blackbox-most-recent-run.log");
    if code < 0 { return; }
    assert_eq!(field(&json, "kind"), Some("Hang"), "most-recent is a hang (then hard-closed), not a crash");
    assert_eq!(code, 11, "HANG exit code");
    assert_eq!(field(&json, "pct"), Some("50"), "wedged at world-load start");
    // the teardown crash is still captured for inspection, just not the verdict
    assert!(json.contains("8867453") || json.contains("874E7D"), "teardown crash still recorded");
}

#[test]
fn never_finished_is_hang() {
    let (code, json) = run_json("pmc_blackbox-more-recent-log-never-finished-loading.log");
    if code < 0 { return; }
    assert_eq!(field(&json, "kind"), Some("Hang"), "never-finished is a hang");
    assert_eq!(code, 11, "HANG exit code");
    assert_eq!(field(&json, "steady_free"), Some("4805"), "steady pool free at the wedge");
}

#[test]
fn old_old_parses_and_reports() {
    let (code, _json) = run_json("pmc_blackbox-old-old-run.log");
    if code < 0 { return; }
    // any non-error classification is fine; just must not panic / must produce a verdict
    assert!(code == 0 || code == 12, "old-old classifies without error (got {})", code);
}
