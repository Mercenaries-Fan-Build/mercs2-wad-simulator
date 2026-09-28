//! `qm` CLI tests that need the retail game stack, exercised as a subprocess.
//!
//! `qm build` and `qm link` open the game stack, so these are game-gated: built by the `retail`
//! feature (`cargo xtask retail-test`), they read the retail vz.wad named by the repo-root
//! `.mercs2-local.toml`, fail if it is absent, and hand it to `qm` explicitly with `--game` — never
//! through `qm`'s own discovery. The hermetic CLI tests are in `cli.rs`.

mod common {
    pub mod cli;
}

use common::cli::{code, fixture, link, qm, read_json, scratch, shipment, EXIT_FINDINGS};
use std::path::{Path, PathBuf};

/// The retail `vz.wad` the repo-root `.mercs2-local.toml` names; panics when it cannot.
fn retail_vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
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
#[test]
fn build_emits_a_wad_and_its_digest() {
    let vz = retail_vz_wad();
    // Dimensions must match the target: a replacement is same-hash and fully resident, so a
    // mismatch is a legitimate hard error rather than something to paper over.
    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let (w, h) = target_dimensions(&vz, hash);

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
        "--game",
        vz.to_str().unwrap(),
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

/// The target texture's real dimensions in the retail stack.
fn target_dimensions(vz: &Path, hash: u32) -> (u32, u32) {
    let mut stack = mercs2_quartermaster::GameStack::open(&[vz.to_path_buf()])
        .unwrap_or_else(|e| panic!("open the game stack {}: {e}", vz.display()));
    let tex = stack
        .texture(hash)
        .unwrap_or_else(|| panic!("texture 0x{hash:08X} is not in {}", vz.display()));
    (tex.width, tex.height)
}

// ---------------------------------------------------------------------------
// qm build's default output
// ---------------------------------------------------------------------------

/// With no `--out`, `qm build` writes under `<shipment>/_build`. `qm build` needs a game stack.
#[test]
fn build_default_out_is_root_underscore_build() {
    let vz = retail_vz_wad();
    let dir = scratch("default-out");
    std::fs::write(dir.join("src/cli-test.ini"), b"[x]\n").unwrap();
    let s = shipment(&dir, "  - kind: place_file\n    file: src/cli-test.ini\n    dest: scripts\n");
    let out = qm(&["build", s.to_str().unwrap(), "--game", vz.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("_build/placement.json").is_file());
    assert!(dir.join("_build/scripts/cli-test.ini").is_file());
    assert!(!dir.join("build").exists(), "the old default is not written");
}

// ---------------------------------------------------------------------------
// `qm link` — exit codes and the plan file
// ---------------------------------------------------------------------------

/// The names in a directory, sorted.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

/// Exit 1: the set's load plan is not ok (`dup-runtime` and `m2-sdk` both ship `m2-sdk.dll`:
/// M0162 and M0207). The plan is written and is the explanation; nothing else is — no link WAD, no
/// placement record.
///
/// `qm link` opens the game stack before it plans. The corpus only has to be a directory: a plan
/// that is not ok never reaches the linker.
#[test]
fn link_plan_not_ok_exits_1_writes_the_plan_and_links_nothing() {
    let vz = retail_vz_wad();
    let dir = scratch("ln-not-ok");
    let out = dir.join("out");
    let o = link(
        &[
            "--request",
            &fixture("request.dup-runtime.json"),
            "--game",
            vz.to_str().unwrap(),
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
/// Needs a game stack for the same reason as the exit-1 case.
#[test]
fn link_ok_plan_exits_0_and_writes_the_plan() {
    let vz = retail_vz_wad();
    let dir = scratch("ln-ok");
    let out = dir.join("out");
    let o = link(
        &[
            &fixture("shipments/m2-sdk"),
            "--game",
            vz.to_str().unwrap(),
            "--corpus",
            dir.join("src").to_str().unwrap(),
        ],
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
