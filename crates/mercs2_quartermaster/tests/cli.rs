//! `qm` CLI tests, exercised as a subprocess.
//!
//! These test the thing the library tests cannot: **the exit code**. The standing mandate is that a
//! build is gated on exit code and never on a printed count, which is a claim about the process, not
//! about `BuildReport`. A caller that pipes stdout to /dev/null must still be unable to ship a
//! Shipment with a HANG-class finding.
//!
//! Three codes, and 1 vs 2 is the one that matters in CI: "this Shipment is wrong" has to be
//! distinguishable from "this runner has no game install", or a misconfigured runner reads as a
//! failing mod.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXIT_FINDINGS: i32 = 1;
const EXIT_UNUSABLE: i32 = 2;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Write a manifest with the given contributions block.
fn shipment(dir: &Path, contributions: &str) -> PathBuf {
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 1
shipment: {{ name: cli-test, version: 1.0.0, target: retail }}
contributions:
{contributions}"
        ),
    )
    .unwrap();
    dir.to_path_buf()
}

fn qm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qm"))
        .args(args)
        .output()
        .expect("qm must run")
}

fn code(out: &Output) -> i32 {
    out.status
        .code()
        .expect("qm must exit normally, not by signal")
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

/// A clean Shipment exits 0. Without this the other tests prove nothing — an exit code that is
/// always nonzero gates just as well as one that is always zero, and is equally useless.
#[test]
fn a_clean_shipment_exits_zero() {
    let dir = scratch("clean");
    std::fs::write(dir.join("src/t.png"), b"not really a png, but present").unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    let out = qm(&["lint", s.to_str().unwrap()]);
    assert_eq!(
        code(&out),
        0,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A finding at Error or above exits 1 — even though nothing reads stdout.
#[test]
fn a_finding_exits_nonzero_without_anyone_reading_stdout() {
    let dir = scratch("finding");
    // No src/t.png: M0110, an Error.
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    let out = qm(&["lint", s.to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_FINDINGS);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("M0110"),
        "the rule code must be in the output so it can be looked up"
    );
}

/// "This Shipment is wrong" (1) must be distinguishable from "this runner cannot run" (2).
/// Collapsing them makes a CI runner with no game install look like a failing mod.
#[test]
fn an_unusable_environment_is_a_different_code_from_a_finding() {
    let dir = scratch("unusable");
    std::fs::write(dir.join("src/t.png"), b"present").unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    let out = qm(&[
        "build",
        s.to_str().unwrap(),
        "--game",
        "/definitely/not/a/game",
    ]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
}

/// A directory with no manifest cannot be linted, and that is a usage problem, not a finding.
#[test]
fn a_missing_manifest_is_unusable_not_a_finding() {
    let dir = scratch("nomanifest");
    let out = qm(&["lint", dir.to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
}

// ---------------------------------------------------------------------------
// Hermetic operation — the property the template repo's CI depends on
// ---------------------------------------------------------------------------

/// `qm lint` must work with NO game install. This is the whole reason the linter is split into a
/// hermetic set and a game-stack set: a public runner will never have the retail WADs, and CI is
/// where the linter matters most.
#[test]
fn lint_needs_no_game_install() {
    let dir = scratch("hermetic");
    std::fs::write(dir.join("src/t.png"), b"present").unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    // Point discovery at a directory with no game in it and confirm lint still succeeds.
    let out = Command::new(env!("CARGO_BIN_EXE_qm"))
        .args(["lint", s.to_str().unwrap()])
        .env("HOME", dir.to_str().unwrap())
        .output()
        .unwrap();
    assert_eq!(
        code(&out),
        0,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("no game install found"),
        "lint must not even LOOK for a game: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Rule discoverability
// ---------------------------------------------------------------------------

/// `qm rules` must list the rules that are NOT implemented alongside the ones that are.
///
/// A linter that silently omits its most dangerous checks reads as a clean bill of health, which is
/// worse than no linter. The unimplemented HANG-class traps have to be visible to anyone asking the
/// tool what it covers.
#[test]
fn rules_lists_the_unimplemented_traps_too() {
    let out = qm(&["rules"]);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);

    // Implemented, in each of the three stages.
    for expected in ["M0100", "M0007", "M0001", "M0002"] {
        assert!(text.contains(expected), "{expected} must be listed");
    }
    // Known and NOT checked — and labelled as such.
    for expected in ["M0003", "M0005", "M0008"] {
        assert!(text.contains(expected), "{expected} must be listed");
    }
    assert!(
        text.contains("KNOWN AND NOT YET CHECKED"),
        "the unimplemented rules must be labelled, not silently mixed in"
    );
}

/// Every rule the CLI prints carries its OWN doc link, so a modder can go read the trap.
///
/// Checks the line following each rule, not merely that "docs/" appears somewhere in the output —
/// a whole-text check passes even when the rule you care about has no link at all.
#[test]
fn every_listed_rule_carries_its_doc() {
    let out = qm(&["rules"]);
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();

    let mut checked = 0;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let is_rule =
            t.len() > 5 && t.starts_with('M') && t[1..5].chars().all(|c| c.is_ascii_digit());
        if !is_rule {
            continue;
        }
        let doc = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
        // A URL, not a repo-relative path: whoever is reading this has the tool, not a checkout of
        // the repo the docs live in.
        assert!(
            doc.starts_with("https://"),
            "{} has no doc URL; the next line was {doc:?}",
            &t[..5]
        );
        checked += 1;
    }

    // Guard the guard: a parser that matched nothing would pass the loop vacuously.
    let registered = mercs2_quartermaster::lint::RULES.len()
        + mercs2_quartermaster::lint::PENDING.len()
        + mercs2_quartermaster::lint::ARTIFACT_RULES.len()
        + 3; // the game-stack rules: M0007, M0009, M0192
    assert_eq!(checked, registered, "every registered rule must be printed");
}

// ---------------------------------------------------------------------------
// The real build, through the CLI
// ---------------------------------------------------------------------------

fn solid_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![0x80u8; (width * height * 4) as usize])
            .unwrap();
    }
    out
}

/// `qm build` produces a real overlay WAD against the retail stack.
///
/// Runs when a PC `vz.wad` is discoverable and SKIPS loudly otherwise, matching `tests/build.rs`.
/// The skip is detected from the CLI's own exit code rather than by re-implementing discovery here,
/// which also checks that the no-game path stays distinguishable.
#[test]
fn build_emits_a_wad_and_its_digest() {
    // Dimensions must match the target: a replacement is same-hash and fully resident, so a
    // mismatch is a legitimate hard error rather than something to paper over.
    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let Some((w, h)) = target_dimensions(hash) else {
        eprintln!("SKIP: no PC vz.wad discoverable — run scripts/find-vz-wad.sh --write");
        return;
    };

    let dir = scratch("realbuild");
    std::fs::write(dir.join("src/t.png"), solid_png(w, h)).unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    let out_dir = dir.join("out");
    let out = qm(&[
        "build",
        s.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(
        code(&out),
        0,
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let wad = out_dir.join("cli-test.wad");
    assert!(wad.is_file(), "the WAD must be on disk");

    // Verified BY HASH: the recorded digest must be the digest of what was written.
    let recorded = std::fs::read_to_string(out_dir.join("cli-test.wad.sha256")).unwrap();
    let actual = mercs2_quartermaster::sha256_hex(&std::fs::read(&wad).unwrap());
    assert!(
        recorded.starts_with(&actual),
        "recorded {recorded:?} does not match the file's {actual}"
    );

    // The placement record is what makes a deploy reversible.
    assert!(out_dir.join("placement.json").is_file());
    assert!(out_dir.join("build.log").is_file());
}

/// The target's real dimensions, or None when there is no discoverable game.
fn target_dimensions(hash: u32) -> Option<(u32, u32)> {
    let found = mercs2_quartermaster::game::discover()?;
    let mut stack = mercs2_quartermaster::GameStack::open(&[found.path]).ok()?;
    let tex = stack.texture(hash)?;
    Some((tex.width, tex.height))
}

// ---------------------------------------------------------------------------
// Data resolution — no build-machine paths
// ---------------------------------------------------------------------------

/// The name table must be found by walking up from the EXECUTABLE or the working directory, never
/// from a path baked in at compile time.
///
/// `CARGO_MANIFEST_DIR` resolves to whatever machine built the binary, so a released `qm` would look
/// for its data on a CI runner's filesystem and silently run one rule short — the same class of bug
/// as the hardcoded asset paths that only worked on one dev machine.
///
/// Running with the working directory inside this checkout must therefore find the real table.
#[test]
fn the_name_table_is_found_by_walking_up_not_by_a_compiled_in_path() {
    let dir = scratch("names");
    std::fs::write(dir.join("src/t.png"), b"present").unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    // CARGO_MANIFEST_DIR is this crate's directory, which is inside the workspace that owns
    // data/production_names.json — so a correct walk-up finds it.
    let out = Command::new(env!("CARGO_BIN_EXE_qm"))
        .args(["lint", s.to_str().unwrap()])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("no name table found"),
        "the table is in this checkout and must be found: {stderr}"
    );
}
