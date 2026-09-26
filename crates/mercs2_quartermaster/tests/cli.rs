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
            "format: 2
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
        + 4; // the game-stack rules: M0007, M0009, M0192, M0193
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

// ---------------------------------------------------------------------------
// `qm preflight` — exit codes and the plan file
// ---------------------------------------------------------------------------

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/load_plan")
}

fn preflight(args: &[&str], out: &Path) -> Output {
    let mut all = vec!["preflight", "--out", out.to_str().unwrap()];
    all.extend_from_slice(args);
    qm(&all)
}

fn fixture(rel: &str) -> String {
    fixtures().join(rel).to_string_lossy().into_owned()
}

/// Exit 0: an ok plan is written.
#[test]
fn preflight_ok_exits_0_and_writes_the_plan() {
    let out = scratch("pf-ok");
    let o = preflight(
        &["--request", &fixture("request.chain.json"), "--game", &fixture("game-clean")],
        &out,
    );
    assert_eq!(code(&o), 0, "stderr: {}", String::from_utf8_lossy(&o.stderr));
    let plan: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("load-plan.json")).unwrap()).unwrap();
    assert_eq!(plan["ok"], true);
    assert_eq!(plan["producer"], "preflight");
}

/// Exit 1: findings, and the plan is still written with its order.
#[test]
fn preflight_findings_exit_1_and_still_write_the_plan() {
    let out = scratch("pf-findings");
    let o = preflight(
        &["--request", &fixture("request.chain-old-ess.json"), "--game", &fixture("game-clean")],
        &out,
    );
    assert_eq!(code(&o), EXIT_FINDINGS, "stderr: {}", String::from_utf8_lossy(&o.stderr));
    let plan: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("load-plan.json")).unwrap()).unwrap();
    assert_eq!(plan["ok"], false);
    assert!(plan["order"].is_array(), "the order survives a non-cycle error");
    assert!(String::from_utf8_lossy(&o.stderr).contains("M0204"));
}

/// The positional form: `arg:<n>` ids in argument order.
#[test]
fn preflight_takes_shipment_directories() {
    let out = scratch("pf-dirs");
    let o = preflight(
        &[&fixture("shipments/my-mod"), &fixture("shipments/ess-0.7.0"), &fixture("shipments/lua-bridge"), "--game", &fixture("game-clean")],
        &out,
    );
    assert_eq!(code(&o), 0, "stderr: {}", String::from_utf8_lossy(&o.stderr));
    let plan: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("load-plan.json")).unwrap()).unwrap();
    assert_eq!(plan["order"], serde_json::json!(["arg:3", "arg:2", "arg:1"]));
}

/// The plan's `quartermaster` is the number `qm --version` prints.
#[test]
fn the_plan_names_the_running_qm() {
    let out = scratch("pf-version");
    preflight(
        &["--request", &fixture("request.chain.json"), "--game", &fixture("game-clean")],
        &out,
    );
    let plan: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("load-plan.json")).unwrap()).unwrap();
    let version = String::from_utf8_lossy(&qm(&["--version"]).stdout).into_owned();
    assert_eq!(version.trim(), format!("qm {}", plan["quartermaster"].as_str().unwrap()));
}

/// Exit 2 cases: nothing could be checked, no plan is written, and a stale plan is removed.
#[test]
fn preflight_that_cannot_run_exits_2_and_leaves_no_plan() {
    let cases: Vec<(Vec<String>, &str)> = vec![
        (vec!["--request".into(), fixture("invalid/bad-format.json")], "request format"),
        (vec!["--request".into(), fixture("invalid/duplicate-id.json")], "duplicate id"),
        (vec!["--request".into(), fixture("invalid/id-backslash.json")], "id with a backslash"),
        (vec!["--request".into(), fixture("invalid/unknown-key.json")], "unknown key"),
        (vec!["--request".into(), fixture("invalid/self-requires.json")], "self-requires"),
        (vec!["--request".into(), fixture("invalid/bad-range.json")], "bad range"),
        (vec!["--request".into(), fixture("invalid/name-version.json")], "{ name, version }"),
        (vec!["--request".into(), fixture("invalid/format-1.json")], "format 1"),
        (vec!["--request".into(), fixture("invalid/url-sha256.json")], "{ url, sha256 }"),
        (vec!["--request".into(), fixture("invalid/reserved-name.json")], "reserved name"),
        (
            vec![fixture("shipments/ess-0.7.0"), "--game".into(), fixture("game-bad/vz.wad")],
            "game root not under data/",
        ),
        (
            vec!["--request".into(), fixture("request.chain.json"), fixture("shipments/my-mod")],
            "both --request and directories",
        ),
        (vec![], "neither --request nor directories"),
    ];
    for (args, what) in cases {
        let out = scratch("pf-unusable");
        std::fs::write(out.join("load-plan.json"), "stale").unwrap();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = preflight(&refs, &out);
        assert_eq!(
            code(&o),
            EXIT_UNUSABLE,
            "{what}: stderr: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(!out.join("load-plan.json").exists(), "{what}: a plan (or the stale one) is left");
    }
}

/// A manifest failure is reported as `error: <id>: <code or ->: <message>`, never with the path.
#[test]
fn preflight_names_the_item_and_code_not_the_path() {
    let out = scratch("pf-stderr");
    for (request, id, code_) in [
        ("invalid/self-requires.json", "shipment:self-requires", "M0173"),
        ("invalid/bad-range.json", "shipment:bad-range", "M0172"),
        ("invalid/reserved-name.json", "shipment:reserved-name", "M0211"),
        ("invalid/format-1.json", "shipment:format-1", "-"),
        ("invalid/url-sha256.json", "shipment:url-sha256", "-"),
    ] {
        let o = preflight(&["--request", &fixture(request)], &out);
        let stderr = String::from_utf8_lossy(&o.stderr);
        assert!(stderr.contains(&format!("error: {id}: {code_}: ")), "{request}: {stderr}");
        assert!(!stderr.contains("invalid/shipments"), "{request}: a request path leaked: {stderr}");
    }
}

// ---------------------------------------------------------------------------
// lint --report: lint-report.json
// ---------------------------------------------------------------------------

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The version `qm --version` prints, without the `qm ` prefix.
fn running_qm() -> String {
    let out = qm(&["--version"]);
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .strip_prefix("qm ")
        .expect("`qm --version` prints `qm <version>`")
        .to_string()
}

/// Every key of the shared finding element, and nothing else.
fn assert_finding_shape(f: &serde_json::Value) {
    let mut keys: Vec<&str> = f.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["code", "fix", "items", "message", "refs", "severity"], "{f}");
    assert_eq!(f["items"], serde_json::json!([]), "a lint report has no request ids");
}

fn lint_with_report(s: &Path, report: &Path) -> Output {
    qm(&["lint", s.to_str().unwrap(), "--report", report.to_str().unwrap()])
}

#[test]
fn lint_report_clean_shipment_exits_0_with_ok_true() {
    let dir = scratch("lr-clean");
    let s = shipment(&dir, "  - kind: add_outfit\n    name: x\n    slug: X\n    display: X\n    wearer: mattias\n");
    let report = dir.join("lint-report.json");
    let out = lint_with_report(&s, &report);
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let r = read_json(&report);
    let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["findings", "format", "manifest", "ok", "producer", "quartermaster"]);
    assert_eq!(r["format"], 1);
    assert_eq!(r["producer"], "lint");
    assert_eq!(r["ok"], true);
    assert_eq!(r["quartermaster"], running_qm(), "`quartermaster` equals `qm --version`");
    assert_eq!(r["manifest"]["shipment"]["name"], "cli-test");
    assert_eq!(r["manifest"]["format"], 2);
    assert_eq!(r["findings"], serde_json::json!([]));
}

/// An error finding: exit 1, `ok: false`, and the finding with its `contributions` ref.
#[test]
fn lint_report_error_exits_1_with_ok_false() {
    let dir = scratch("lr-error");
    let s = shipment(&dir, "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/missing.png\n");
    let report = dir.join("r.json");
    let out = lint_with_report(&s, &report);
    assert_eq!(code(&out), EXIT_FINDINGS);
    let r = read_json(&report);
    assert_eq!(r["ok"], false);
    assert_finding_shape(&r["findings"][0]);
    assert_eq!(r["findings"][0]["code"], "M0110");
    assert_eq!(r["findings"][0]["severity"], "error");
    assert_eq!(
        r["findings"][0]["refs"],
        serde_json::json!([{ "section": "contributions", "index": 0 }])
    );
    let text = r["findings"][0]["message"].as_str().unwrap();
    assert!(!text.contains(dir.to_str().unwrap()), "no absolute path in a message: {text}");
}

/// `fix` carries the replacement text when the fix is mechanical (M0140's suggested hero), and is
/// `null` otherwise. Findings are sorted by code.
#[test]
fn lint_report_carries_fix_or_null() {
    let dir = scratch("lr-fix");
    let s = shipment(
        &dir,
        "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/missing.png\n  - kind: add_outfit\n    name: x\n    slug: X\n    display: X\n    wearer: mattius\n",
    );
    let report = dir.join("r.json");
    let out = lint_with_report(&s, &report);
    assert_eq!(code(&out), EXIT_FINDINGS);
    let r = read_json(&report);
    let findings = r["findings"].as_array().unwrap();
    for f in findings {
        assert_finding_shape(f);
    }
    let codes: Vec<&str> = findings.iter().map(|f| f["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["M0110", "M0140"], "sorted by code");
    assert_eq!(findings[0]["fix"], serde_json::Value::Null);
    assert_eq!(findings[1]["fix"], "mattias");
    assert_eq!(findings[1]["refs"][0]["index"], 1);
}

/// A manifest that does not parse or validate is exit 2, and no report is left — including a
/// stale one from an earlier run.
#[test]
fn lint_report_parse_failure_exits_2_and_leaves_no_report() {
    let dir = scratch("lr-bad");
    std::fs::write(dir.join("manifest.yaml"), "format: 1\nshipment: { name: x, version: 1.0.0, target: retail }\n").unwrap();
    let report = dir.join("r.json");
    std::fs::write(&report, "stale").unwrap();
    let out = lint_with_report(&dir, &report);
    assert_eq!(code(&out), EXIT_UNUSABLE);
    assert!(!report.exists(), "the stale report must be gone");
    assert!(!String::from_utf8_lossy(&out.stderr).trim().is_empty(), "exit 2 explains itself");
}

/// An unwritable report is exit 2, not a lint verdict.
#[test]
fn lint_report_unwritable_exits_2() {
    let dir = scratch("lr-unwritable");
    let s = shipment(&dir, "");
    let report = dir.join("no-such-dir").join("r.json");
    let out = lint_with_report(&s, &report);
    assert_eq!(code(&out), EXIT_UNUSABLE);
    assert!(!report.exists());
}

// ---------------------------------------------------------------------------
// check-range: range-report.json
// ---------------------------------------------------------------------------

#[test]
fn check_range_valid_ranges_exit_0_with_no_findings() {
    let dir = scratch("cr-ok");
    let report = dir.join("range-report.json");
    let out = qm(&["check-range", "--report", report.to_str().unwrap(), "--", ">=1", "^1.0.0", ">=0.7, <1"]);
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let r = read_json(&report);
    assert_eq!(
        r,
        serde_json::json!({
            "format": 1, "producer": "check-range", "quartermaster": running_qm(),
            "ok": true, "findings": []
        })
    );
}

/// One bad range among good ones: exit 1, one M0172 at that range's index. A range starting with
/// `-` is still a range after `--`.
#[test]
fn check_range_one_bad_range_exits_1_with_its_index() {
    let dir = scratch("cr-bad");
    let report = dir.join("range-report.json");
    let out = qm(&["check-range", "--report", report.to_str().unwrap(), "--", "^1.0.0", "-1", ">=2"]);
    assert_eq!(code(&out), EXIT_FINDINGS);
    let r = read_json(&report);
    assert_eq!(r["ok"], false);
    let findings = r["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{r}");
    assert_finding_shape(&findings[0]);
    assert_eq!(findings[0]["code"], "M0172");
    assert_eq!(findings[0]["severity"], "error");
    assert_eq!(findings[0]["refs"], serde_json::json!([{ "section": "ranges", "index": 1 }]));
    assert_eq!(findings[0]["fix"], serde_json::Value::Null);
    let expected = semver::VersionReq::parse("-1").unwrap_err().to_string();
    assert_eq!(findings[0]["message"], expected, "the message is the parse error");
}

#[test]
fn check_range_with_no_ranges_exits_2_and_leaves_no_report() {
    let dir = scratch("cr-none");
    let report = dir.join("range-report.json");
    std::fs::write(&report, "stale").unwrap();
    let out = qm(&["check-range", "--report", report.to_str().unwrap(), "--"]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
    assert!(!report.exists(), "the stale report must be gone");
}

// ---------------------------------------------------------------------------
// compile-lua
// ---------------------------------------------------------------------------

#[test]
fn compile_lua_ok_exit_0_header_verified() {
    let dir = scratch("cl-ok");
    std::fs::write(dir.join("src/ess.lua"), "Ess = {}\nreturn Ess\n").unwrap();
    std::fs::write(dir.join("src/ess_names.lua"), "return {}\n").unwrap();
    let out_dir = dir.join("_build/compile-lua");
    let out = qm(&[
        "compile-lua",
        dir.join("src/ess.lua").to_str().unwrap(),
        dir.join("src/ess_names.lua").to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    for (line, chunk) in lines.iter().zip(["ess", "ess_names"]) {
        let bytes = std::fs::read(out_dir.join(format!("{chunk}.luac"))).unwrap();
        assert_eq!(&bytes[..12], &mercs2_luac::MERCS2_LUAQ_HEADER);
        let fields: Vec<&str> = line.split(' ').collect();
        assert_eq!(fields[0], "ok");
        assert!(fields[1].ends_with(&format!("{chunk}.lua")), "{line}");
        assert_eq!(fields[2], bytes.len().to_string());
        assert_eq!(fields[3], mercs2_quartermaster::sha256_hex(&bytes));
    }
}

#[test]
fn compile_lua_syntax_exit_1() {
    let dir = scratch("cl-syntax");
    std::fs::write(dir.join("src/broken.lua"), "function oops(\nreturn 1\n").unwrap();
    let out = qm(&["compile-lua", dir.join("src/broken.lua").to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_FINDINGS);
    assert!(String::from_utf8_lossy(&out.stderr).contains(":2:"), "the Lua error has its line number");
}

/// A file stem that is not a bare chunk name is exit 1, like a syntax error: the author fixes it.
#[test]
fn compile_lua_bad_stem_exit_1() {
    let dir = scratch("cl-stem");
    std::fs::write(dir.join("src/@ess.lua"), "return 1\n").unwrap();
    let out = qm(&["compile-lua", dir.join("src/@ess.lua").to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_FINDINGS);
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a bare chunk name"));
}

#[test]
fn compile_lua_missing_exit_2() {
    let dir = scratch("cl-missing");
    let out = qm(&["compile-lua", dir.join("src/nope.lua").to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
}

#[test]
fn compile_lua_multi_with_chunk_name_exit_2() {
    let dir = scratch("cl-multi");
    std::fs::write(dir.join("src/a.lua"), "return 1\n").unwrap();
    std::fs::write(dir.join("src/b.lua"), "return 2\n").unwrap();
    let out = qm(&[
        "compile-lua",
        dir.join("src/a.lua").to_str().unwrap(),
        dir.join("src/b.lua").to_str().unwrap(),
        "--chunk-name",
        "a",
    ]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
}

#[test]
fn compile_lua_non_utf8_exit_2() {
    let dir = scratch("cl-utf8");
    std::fs::write(dir.join("src/bin.lua"), [0xFFu8, 0xFE, 0x00, 0x80]).unwrap();
    let out = qm(&["compile-lua", dir.join("src/bin.lua").to_str().unwrap()]);
    assert_eq!(code(&out), EXIT_UNUSABLE);
}

// ---------------------------------------------------------------------------
// qm build's default output
// ---------------------------------------------------------------------------

/// With no `--out`, `qm build` writes under `<shipment>/_build`. `qm build` needs a game stack, so
/// this runs when one is discoverable and SKIPS loudly otherwise, like `build_emits_a_wad_and_its_digest`.
#[test]
fn build_default_out_is_root_underscore_build() {
    if mercs2_quartermaster::game::discover().is_none() {
        eprintln!("SKIP: no PC vz.wad discoverable — run scripts/find-vz-wad.sh --write");
        return;
    }
    let dir = scratch("default-out");
    std::fs::write(dir.join("src/cli-test.ini"), b"[x]\n").unwrap();
    let s = shipment(&dir, "  - kind: place_file\n    file: src/cli-test.ini\n    dest: scripts\n");
    let out = qm(&["build", s.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("_build/placement.json").is_file());
    assert!(dir.join("_build/scripts/cli-test.ini").is_file());
    assert!(!dir.join("build").exists(), "the old default is not written");
}

// ---------------------------------------------------------------------------
// manifest-info: a Shipment's name and version, for choosing its release before a full lint
// ---------------------------------------------------------------------------

/// A valid manifest in each format: exit 0, and stdout is exactly one JSON object with the name and
/// version.
#[test]
fn manifest_info_prints_name_and_version() {
    let dir = scratch("mi-ok");
    let cases = [
        (
            "manifest.yaml",
            "format: 2\nshipment: { name: my-mod, version: 1.2.3, target: retail }\n".to_string(),
        ),
        (
            "manifest.json",
            r#"{"format":2,"shipment":{"name":"my-mod","version":"1.2.3","target":"retail"}}"#
                .to_string(),
        ),
        (
            "manifest.toml",
            "format = 2\n[shipment]\nname = \"my-mod\"\nversion = \"1.2.3\"\ntarget = \"retail\"\n"
                .to_string(),
        ),
    ];
    for (file, text) in cases {
        let path = dir.join(file);
        std::fs::write(&path, text).unwrap();
        let out = qm(&["manifest-info", path.to_str().unwrap()]);
        assert_eq!(code(&out), 0, "{file}: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1, "{file}: one line: {stdout:?}");
        let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(v, serde_json::json!({ "name": "my-mod", "version": "1.2.3" }), "{file}");
    }
}

/// Every failure is exit 2 with nothing on stdout and a reason on stderr.
#[test]
fn manifest_info_failures_exit_2_with_empty_stdout() {
    let dir = scratch("mi-bad");
    let write = |name: &str, text: &str| {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    };
    let cases: Vec<(PathBuf, &str)> = vec![
        (dir.join("missing.yaml"), "unreadable"),
        (write("manifest.txt", "format: 2\n"), "not a manifest extension"),
        (write("broken.yaml", "format: 2\nshipment: [not, a, map]\n"), "does not parse"),
        (
            write("format1.yaml", "format: 1\nshipment: { name: x, version: 1.0.0, target: retail }\n"),
            "fails validation (format 1)",
        ),
        (
            write("semver.yaml", "format: 2\nshipment: { name: x, version: one, target: retail }\n"),
            "fails validation (version)",
        ),
        (
            write(
                "range.yaml",
                "format: 2\nshipment: { name: x, version: 1.0.0, target: retail }\n\
                 load: { requires: [{ shipment: y, version: \"nope\" }] }\n",
            ),
            "fails validation (M0172)",
        ),
    ];
    for (path, what) in cases {
        let out = qm(&["manifest-info", path.to_str().unwrap()]);
        assert_eq!(code(&out), EXIT_UNUSABLE, "{what}");
        assert!(out.stdout.is_empty(), "{what}: stdout must be empty, got {:?}", out.stdout);
        assert!(!out.stderr.is_empty(), "{what}: the reason goes to stderr");
    }
}

// ---------------------------------------------------------------------------
// `qm link` — exit codes and the plan file
// ---------------------------------------------------------------------------

fn link(args: &[&str], out: &Path) -> Output {
    let mut all = vec!["link", "--out", out.to_str().unwrap()];
    all.extend_from_slice(args);
    qm(&all)
}

/// The names in a directory, sorted.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

/// Exit 2: a usage error or a request that cannot be read. Nothing was checked, no plan is written,
/// and a stale plan from an earlier run is removed. Hermetic: the request is read before the game
/// stack is resolved, so every case fails on the request and not on a missing game.
#[test]
fn link_that_cannot_run_exits_2_and_leaves_no_plan() {
    let missing = scratch("ln-missing-request").join("load-request.json");
    let cases: Vec<(Vec<String>, &str, &str)> = vec![
        (
            vec!["--request".into(), missing.to_string_lossy().into_owned()],
            "reading the request",
            "unreadable request",
        ),
        (
            vec!["--request".into(), fixture("invalid/bad-format.json")],
            "format",
            "request format",
        ),
        (
            vec!["--request".into(), fixture("request.chain.json"), fixture("shipments/my-mod")],
            "give either --request or Shipment directories, not both",
            "both --request and directories",
        ),
        (
            vec![],
            "give --request <load-request.json> or at least one Shipment directory",
            "neither --request nor directories",
        ),
    ];
    for (args, says, what) in cases {
        let out = scratch("ln-unusable");
        std::fs::write(out.join("load-plan.json"), "stale").unwrap();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = link(&refs, &out);
        let stderr = String::from_utf8_lossy(&o.stderr);
        assert_eq!(code(&o), EXIT_UNUSABLE, "{what}: stderr: {stderr}");
        assert!(stderr.contains(says), "{what}: expected {says:?} in stderr: {stderr}");
        assert!(!out.join("load-plan.json").exists(), "{what}: a plan (or the stale one) is left");
    }
}

/// `--out` is required: without it clap refuses the command line, which is exit 2 as well.
#[test]
fn link_without_out_exits_2() {
    let o = qm(&["link", &fixture("shipments/m2-sdk")]);
    assert_eq!(code(&o), EXIT_UNUSABLE, "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--out"));
}

/// Exit 1: the set's load plan is not ok (`dup-runtime` and `m2-sdk` both ship `m2-sdk.dll`:
/// M0162 and M0207). The plan is written and is the explanation; nothing else is — no link WAD, no
/// placement record.
///
/// `qm link` opens the game stack before it plans, so this runs when a PC `vz.wad` is discoverable
/// and SKIPS loudly otherwise, like `build_default_out_is_root_underscore_build`. The corpus only
/// has to be a directory: a plan that is not ok never reaches the linker.
#[test]
fn link_plan_not_ok_exits_1_writes_the_plan_and_links_nothing() {
    if mercs2_quartermaster::game::discover().is_none() {
        eprintln!("SKIP: no PC vz.wad discoverable — run scripts/find-vz-wad.sh --write");
        return;
    }
    let dir = scratch("ln-not-ok");
    let out = dir.join("out");
    let o = link(
        &[
            "--request",
            &fixture("request.dup-runtime.json"),
            "--corpus",
            dir.join("src").to_str().unwrap(),
        ],
        &out,
    );
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert_eq!(code(&o), EXIT_FINDINGS, "stderr: {stderr}");
    assert!(stderr.contains("M0207"), "{stderr}");
    let plan = read_json(&out.join("load-plan.json"));
    assert_eq!(plan["ok"], false);
    assert_eq!(plan["producer"], "link");
    assert_eq!(listing(&out), ["load-plan.json"], "only the plan is written");
}

/// Exit 0: an ok plan. `m2-sdk` touches no script and no string table, so there is nothing to link:
/// the plan and an empty placement record are written, and no link WAD.
///
/// Needs a game stack for the same reason as the exit-1 case, and SKIPS loudly without one.
#[test]
fn link_ok_plan_exits_0_and_writes_the_plan() {
    if mercs2_quartermaster::game::discover().is_none() {
        eprintln!("SKIP: no PC vz.wad discoverable — run scripts/find-vz-wad.sh --write");
        return;
    }
    let dir = scratch("ln-ok");
    let out = dir.join("out");
    let o = link(
        &[&fixture("shipments/m2-sdk"), "--corpus", dir.join("src").to_str().unwrap()],
        &out,
    );
    assert_eq!(code(&o), 0, "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("nothing to link"));
    let plan = read_json(&out.join("load-plan.json"));
    assert_eq!(plan["ok"], true);
    assert_eq!(plan["producer"], "link");
    let placement = read_json(&out.join("placement.json"));
    assert_eq!(placement["placements"], serde_json::json!([]));
    assert_eq!(listing(&out), ["load-plan.json", "placement.json"], "no link WAD");
}
