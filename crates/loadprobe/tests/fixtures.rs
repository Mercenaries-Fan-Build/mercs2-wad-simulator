//! Integration tests against the five real `pmc_blackbox.log` captures in `tests/fixtures/`.
//! These lock in the end-state classifier so the verdicts cannot silently regress.
//!
//! # Why the fixtures live inside the crate
//!
//! They used to be read from `../../../storage` — four `pop()`s from `CARGO_MANIFEST_DIR`,
//! which resolved only from the old vendored layout (`mercenaries-game/tools/wad_simulator/
//! crates/loadprobe`). Once loadprobe became its own repo the same arithmetic pointed at
//! `~/storage`, and every test below quietly returned instead of running. `gen_ladder.py` had
//! already met this exact problem and solved it by trying both layouts; this file never got the
//! same treatment.
//!
//! Fixtures now sit under `CARGO_MANIFEST_DIR/tests/fixtures`, which is correct from any
//! checkout depth, in a worktree, and in a vendored copy. There is no layout to get wrong.
//!
//! # Why a missing fixture is a failure, not a skip
//!
//! The previous version returned early when a file was absent, so the suite passed whether or
//! not it had tested anything — and it tested nothing, in every checkout, for as long as the
//! path was wrong. A test that cannot fail is worse than an absent one, because it reports
//! coverage it does not have. These fixtures are committed, so absence means a broken checkout
//! and the suite says so.

use std::path::PathBuf;
use std::process::Command;

/// Absolute path to a committed fixture. Anchored on the crate, not on a guess about how far
/// up the repository root sits.
fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

/// Run the built binary with `--json` and return `(exit code, stdout)`.
fn run_json(log: &str) -> (i32, String) {
    let path = fixture(log);
    assert!(
        path.exists(),
        "fixture {} is missing — it is committed to this repo, so a checkout without it is \
         broken. Do not make this a skip: that is how these tests came to run against nothing.",
        path.display()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_loadprobe"))
        .arg("--json")
        .arg("--no-color")
        .arg(&path)
        .output()
        .expect("run loadprobe");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).to_string())
}

/// Tiny extractor: find `"key"`, then the next token (string or number). Tolerates the
/// pretty-printer's spaces after the colon.
fn field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\"");
    let i = json.find(&pat)? + pat.len();
    let rest = json[i..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    if let Some(s) = rest.strip_prefix('"') {
        Some(&s[..s.find('"')?])
    } else {
        let end = rest.find(|c: char| c == ',' || c == '}' || c.is_whitespace()).unwrap_or(rest.len());
        Some(&rest[..end])
    }
}

/// Every committed capture, with the record count that identifies it.
const CAPTURES: &[(&str, &str)] = &[
    ("pmc_blackbox.log", "6508"),
    ("pmc_blackbox-second-baseline.log", "6223"),
    ("pmc_blackbox-chris-save-0-percent-pre-pmc-takeover.log", "3988"),
    ("pmc_blackbox-jen-save-early-percent.log", "4103"),
    ("pmc_blackbox-mattias-save-end-game.log", "6783"),
];

/// All five captures are successful loads, and the classifier must say so for every one.
///
/// Uniform verdicts are the point rather than a weakness: the captures span **three**
/// `pmc_blackbox` version families (v0.4.1, v0.4.3, v3.0.0) and four different save states,
/// from 0%-completion to end-game. A ladder change that stopped matching one family's marker
/// text would drop that capture below phase 20 while leaving the others intact.
#[test]
fn every_capture_reaches_the_world() {
    for (name, records) in CAPTURES {
        let (code, json) = run_json(name);
        assert_eq!(code, 0, "{name}: REACHED-WORLD exit code");
        assert_eq!(field(&json, "kind"), Some("ReachedWorld"), "{name}");
        assert_eq!(field(&json, "pct"), Some("100"), "{name}: fully loaded");
        assert_eq!(field(&json, "furthest_idx"), Some("20"), "{name}: last rung of the ladder");
        assert_eq!(field(&json, "records"), Some(*records), "{name}: capture identity");
    }
}

/// The ladder matches across every `pmc_blackbox` version that has ever produced a capture.
///
/// Phase 0 matches on the banner, and the banner carries the version — so its match string is
/// the one rung coupled to a value that changes on every release. `LADDER` pins the literal
/// `"PMC Blackbox v3"`, and these captures were produced by v0.4.1, v0.4.3 and v3.0.0, so the
/// coupling is not hypothetical: it already discriminates between real logs.
///
/// Phase 0 is also the *only* rung reachable without `PMC_VERBOSE_LOG=1`, because every other
/// marker arrives through the verbose-gated Lua hook while the banner is always logged. A
/// version bump that breaks it therefore blinds progression reporting for the default
/// configuration, which is the case least likely to be noticed.
#[test]
fn the_banner_rung_is_version_coupled() {
    let mut banners = Vec::new();
    for (name, _) in CAPTURES {
        let text = std::fs::read_to_string(fixture(name)).expect("fixture readable");
        let line = text
            .lines()
            .find(|l| l.contains("PMC Blackbox v"))
            .unwrap_or_else(|| panic!("{name}: no banner line"));
        let v = line.split("PMC Blackbox v").nth(1).unwrap().split_whitespace().next().unwrap();
        banners.push(v.to_string());
    }
    banners.sort();
    banners.dedup();
    assert!(banners.len() > 1, "captures should span several versions, got {banners:?}");

    let rung = loadprobe::phases::LADDER[0].matches;
    let matched: Vec<_> = banners
        .iter()
        .filter(|v| rung.iter().any(|m| format!("PMC Blackbox v{v}").contains(m)))
        .collect();
    assert!(
        !matched.is_empty(),
        "phase 0 matches no captured version — banners {banners:?} vs ladder {rung:?}"
    );
    // Documented, not asserted away: the rung matches only the v3 family. Widening it to a
    // version-independent prefix is the fix; until then this records which versions are blind.
    eprintln!("phase 0 matches {}/{} captured versions: {matched:?}", matched.len(), banners.len());
}

/// `ladder_version` is stamped on a report derived from a **real** log, not only a synthetic one.
///
/// An ordinal without the table that gives it meaning is unreadable to a consumer, so the field
/// riding along on every report is the invariant, not merely its presence on the struct.
#[test]
fn real_reports_carry_the_ladder_version() {
    for (name, _) in CAPTURES {
        let (_, json) = run_json(name);
        assert_eq!(
            field(&json, "ladder_version"),
            Some(loadprobe::LADDER_VERSION.to_string().as_str()),
            "{name}"
        );
    }
}

/// The parser understands essentially all of a real capture.
///
/// `unparsed_lines` is the honest measure of drift between what `pmc_blackbox` writes and what
/// loadprobe reads. It is 0 for four captures and 4 for the end-game one; a jump means the log
/// format moved and the reader did not.
#[test]
fn captures_parse_essentially_completely() {
    for (name, records) in CAPTURES {
        let (_, json) = run_json(name);
        let unparsed: u32 = field(&json, "unparsed_lines").unwrap().parse().unwrap();
        let total: u32 = records.parse().unwrap();
        assert!(
            unparsed * 1000 < total,
            "{name}: {unparsed} unparsed of {total} records — the log format has moved"
        );
    }
}

/// `LADDER_VERSION` is reachable from a LINKED consumer, not just from inside the crate.
///
/// That is the whole point of it: Modkit runs `loadprobe` as a library and has to stamp
/// `ladder_version` onto a report next to `phase_idx`, so an ordinal is never sent without the
/// table that gives it meaning. A constant only the binary could see would not do that, and this
/// test is in the integration suite precisely because it links the crate the way a consumer does.
#[test]
fn ladder_version_is_exported_to_consumers() {
    assert_eq!(loadprobe::LADDER_VERSION, loadprobe::phases::LADDER_VERSION);
    // And it rides on the report, so the `--json` path carries it too.
    let lines = loadprobe::parse::parse_log("[00:00:01.000] [blackbox] PMC Blackbox v3\n");
    let r = loadprobe::report::analyze("x.log", "0".into(), &lines, &[], &[], 30, 1);
    assert_eq!(r.ladder_version, loadprobe::LADDER_VERSION);
}

/// What these captures do **not** cover, recorded so it is not mistaken for coverage.
///
/// Not one of the five contains a fault: every `[crash]` line in them is the handler's own
/// "armed" notice. So the crash path — the anchor that decides `Verdict::Crash` versus
/// `Verdict::Truncated`, the `AV ` parse, and `module`/`offset` extraction — is exercised only
/// by synthetic input elsewhere in this suite.
///
/// This test asserts the gap rather than describing it in a comment nobody re-reads: when a
/// capture containing a real fault is finally committed, it fails, and whoever adds that fixture
/// is told to come here and write the assertions it makes possible.
#[test]
fn no_capture_exercises_the_crash_path_yet() {
    for (name, _) in CAPTURES {
        let text = std::fs::read_to_string(fixture(name)).expect("fixture readable");
        assert!(
            !text.contains("VEH EXCEPTION") && !text.contains("UNHANDLED EXCEPTION"),
            "{name} contains a real fault — the crash path now has a fixture. Replace this test \
             with assertions on the verdict, the AV operation, and module/offset attribution."
        );
    }
}
