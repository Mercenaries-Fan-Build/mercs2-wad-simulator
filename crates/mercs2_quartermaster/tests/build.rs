//! Builder behaviour.
//!
//! The hermetic tests pin the GATE and the emission contract — those must hold with no game
//! present, since that is the state template CI runs in. The one test that needs the retail WADs is
//! `#[ignore]`d behind the workspace's usual convention.

use mercs2_quartermaster::build::{self, BuildError, Destination};
use mercs2_quartermaster::discover;
use std::path::{Path, PathBuf};

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm_build_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn shipment(dir: &Path, contributions: &str) -> discover::LoadedShipment {
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 1
shipment: {{ name: test-shipment, version: 1.0.0, target: retail }}
contributions:
{contributions}"
        ),
    )
    .expect("write manifest");
    discover::open(dir).expect("open shipment")
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

/// The gate is the RETURN TYPE, not a field a caller might forget to read.
#[test]
fn a_blocking_diagnostic_fails_the_build() {
    let dir = scratch("blocked");
    let s = shipment(
        &dir,
        "  - kind: add_outfit
    name: x
    slug: X
    display: X
    wearer: bulldog
    model: src/m.glb
",
    );
    match build::build(&s, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0140"));
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(!dir.join("build").join("test-shipment.wad").exists(), "nothing may be emitted");
}

/// A build that needs the WADs must say so plainly, and point at where to configure them.
#[test]
fn a_texture_replacement_without_a_game_stack_reports_what_is_missing() {
    let dir = scratch("nogame");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/t.png"), fake_png()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );
    match build::build(&s, None, None, None) {
        Err(e @ BuildError::GameRequired { .. }) => {
            let msg = e.to_string();
            assert!(msg.contains("qm lint"), "should say lint still works: {msg}");
            assert!(msg.contains("game folder"), "should point at configuration: {msg}");
        }
        other => panic!("expected GameRequired, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Honest unsupported reporting
// ---------------------------------------------------------------------------

/// A kind we cannot lower yet must FAIL LOUDLY with the reason, never be skipped — a silently
/// dropped contribution produces a WAD that looks fine and does nothing.
#[test]
fn unsupported_kinds_fail_loudly_with_a_reason() {
    for (contribution, expect) in [
        (
            "  - kind: add_model\n    name: thing\n    model: src/m.glb\n",
            "BINARY-ONLY",
        ),
        (
            "  - kind: patch_lua\n    target: wifpmcinterior\n    append: src/a.lua\n",
            "LINK time",
        ),
    ] {
        let dir = scratch("unsupported");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/m.glb"), b"x").unwrap();
        std::fs::write(dir.join("src/a.lua"), b"x").unwrap();
        let s = shipment(&dir, contribution);
        match build::build(&s, None, None, None) {
            Err(e @ BuildError::Unsupported { .. }) => {
                assert!(e.to_string().contains(expect), "unhelpful reason: {e}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

#[test]
fn an_empty_shipment_still_emits_a_record_and_a_log() {
    let dir = scratch("empty");
    let s = shipment(&dir, "  []\n");
    let report = build::build(&s, None, None, None).expect("empty shipment builds");
    assert!(report.wad.is_none(), "nothing to put in a WAD");
    assert!(report.placements.is_empty());
    assert!(dir.join("build/placement.json").is_file());
    assert!(dir.join("build/build.log").is_file());
}

#[test]
fn the_output_directory_can_be_redirected() {
    let dir = scratch("outdir");
    let out = dir.join("elsewhere");
    let s = shipment(&dir, "  []\n");
    build::build(&s, None, None, Some(&out)).expect("build");
    assert!(out.join("placement.json").is_file());
    assert!(!dir.join("build").exists());
}

/// Known SHA-256 vectors — the mandate is verify-BY-HASH, so a wrong digest silently defeats every
/// downstream integrity check.
#[test]
fn sha256_matches_known_vectors() {
    assert_eq!(
        build::sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        build::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn the_placement_record_is_well_formed_json() {
    let dir = scratch("record");
    let s = shipment(&dir, "  []\n");
    build::build(&s, None, None, None).expect("build");
    let text = std::fs::read_to_string(dir.join("build/placement.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(doc["format"], 1);
    assert!(doc["placements"].is_array());
}

// ---------------------------------------------------------------------------
// Against the real game
// ---------------------------------------------------------------------------

/// End-to-end texture replacement against the retail WADs.
///
/// `#[ignore]`d by the workspace convention for game-dependent tests. Point `MERCS2_VZ_WAD` at a
/// `vz.wad` and run:
///   `cargo test -p mercs2_quartermaster --test build -- --ignored --nocapture`
#[test]
#[ignore = "needs the retail vz.wad"]
fn a_texture_replacement_builds_end_to_end() {
    let Ok(wad) = std::env::var("MERCS2_VZ_WAD") else {
        eprintln!("MERCS2_VZ_WAD unset; skipping");
        return;
    };
    let mut game = mercs2_quartermaster::GameStack::open(&[PathBuf::from(&wad)])
        .expect("open the game stack");

    // Read the target's real dimensions so the fixture matches; a replacement is same-hash and
    // fully resident, so mismatched dimensions are a legitimate hard error.
    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let existing = game.texture(hash).expect("al_hum_boss_ub must exist in vz.wad");

    let dir = scratch("real");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/t.png"),
        solid_png(existing.width, existing.height),
    )
    .unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/t.png
",
    );

    let report = build::build(&s, Some(&mut game), None, None).expect("build");

    // This target turns out to be a 4-rung STREAMED texture with no primary row of its own, so the
    // game-aware rules fire — and the build still completes, because they are warnings. That pairing
    // is the point: the author is told what changed without being blocked from shipping it.
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.rule.code).collect();
    assert!(codes.contains(&"M0007") && codes.contains(&"M0009"), "{codes:?}");
    assert!(report.diagnostics.iter().all(|d| d.severity < mercs2_quartermaster::Severity::Error));

    let wad_path = report.wad.expect("a WAD must be emitted");
    assert!(wad_path.is_file());

    let placement = &report.placements[0];
    assert_eq!(placement.destination, Destination::Overlay);
    // Verified BY HASH: the recorded digest must be the digest of what is on disk.
    let on_disk = std::fs::read(&wad_path).unwrap();
    assert_eq!(placement.sha256, build::sha256_hex(&on_disk));

    // --- structural regressions, both found by wad_simulator and invisible to any digest check ---
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];

    // (1) The ASET row must be PRIMARY. `is_primary()` tests low-16 == 0xFFFF; any other value
    // names a `_P001` LOD block one level finer, and a row pointing at a rung that does not exist
    // is the dangling-LOD-rung trap — a 549 GB buffer request and an open-world stream HANG.
    // NOTE `patch_wad::AsetEntry` is a different type from `ffcs::AsetEntry` and names its fields
    // positionally; `u32_2` is the `packed_block_ref` the reader side calls it.
    let row = &block.aset_entries[0];
    assert_eq!(
        row.u32_2 & 0xFFFF,
        0xFFFF,
        "a replacement must register as primary, not as a dangling LOD rung"
    );

    // (2) A patch block is `[entry table][containers…]`, NOT a bare container. Handing over a raw
    // container makes the loader read the `UCFX` magic as an entry-table field — the WAD hashes
    // fine and is structurally nonsense.
    let decompressed =
        mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&decompressed);
    assert_eq!(count, 1, "expected a single-entry block table");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name_hash, hash);
    assert_eq!(
        &decompressed[20..24],
        b"UCFX",
        "the container must start AFTER the 20-byte entry table"
    );

    // Full structural validation is `wad_simulator`, which is what caught both of the above:
    //   cargo run --bin wad_simulator -- --wad <out.wad> --base-wad <vz.wad> --skip-audio
    // Expect "UCFX / FORMAT" to be absent and the verdict to report no violations.

    // Determinism: the mandate only means something if two builds agree byte for byte.
    let again = build::build(&s, Some(&mut game), None, Some(&dir.join("second")))
        .expect("second build");
    assert_eq!(
        placement.sha256, again.placements[0].sha256,
        "two builds of one Shipment must be byte-identical"
    );
}

// --- fixtures --------------------------------------------------------------

/// A 1x1 PNG — enough to exist, deliberately the wrong size for any real target.
fn fake_png() -> Vec<u8> {
    solid_png(1, 1)
}

fn solid_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data = vec![0x80u8; (width * height * 4) as usize];
        writer.write_image_data(&data).unwrap();
    }
    out
}

/// M0007/M0009 against real ASET rows, in both directions.
///
/// The classes are the opposite of what "character texture" intuition suggests, which is exactly
/// why this is measured rather than assumed:
///   `pmc_hum_mattias_v3_ub`  primary, single-block         -> silent
///   `al_hum_boss_ub`         NON-primary, 4-rung streamed  -> M0007 + M0009
#[test]
#[ignore = "needs the retail vz.wad"]
fn streamed_and_shared_targets_are_flagged_and_resident_ones_are_not() {
    let Ok(wad) = std::env::var("MERCS2_VZ_WAD") else {
        eprintln!("MERCS2_VZ_WAD unset; skipping");
        return;
    };
    let game = mercs2_quartermaster::GameStack::open(&[PathBuf::from(&wad)]).expect("stack");
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_quartermaster::lint::{self, aset_row_is_single_block};
    const TEX: u32 = mercs2_formats::types::TYPE_ID_TEXTURE;

    // A hero texture really is single-block AND primary — replacing it changes no residency.
    let (p, s, primary) = *game
        .aset_rows(pandemic_hash_m2("pmc_hum_mattias_v3_ub"), TEX)
        .first()
        .expect("mattias_v3_ub row");
    assert!(primary && aset_row_is_single_block(p, s), "packed 0x{p:08X} secondary 0x{s:08X}");

    let dir = scratch("m0007_quiet");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/t.png"), solid_png(4, 4)).unwrap();
    let quiet = shipment(
        &dir,
        "  - kind: replace_texture\n    target: pmc_hum_mattias_v3_ub\n    image: src/t.png\n",
    );
    assert!(
        lint::game_checks(&quiet.manifest, &game).is_empty(),
        "a resident, primary target must not be flagged"
    );

    // al_hum_boss_ub is neither: four rungs, and no primary row of its own.
    let dir2 = scratch("m0007_fires");
    std::fs::create_dir_all(dir2.join("src")).unwrap();
    std::fs::write(dir2.join("src/t.png"), solid_png(4, 4)).unwrap();
    let fires = shipment(
        &dir2,
        "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/t.png\n",
    );
    let codes: Vec<&str> =
        lint::game_checks(&fires.manifest, &game).iter().map(|d| d.rule.code).collect();
    assert!(codes.contains(&"M0007"), "streamed target must warn: {codes:?}");
    assert!(codes.contains(&"M0009"), "shared target must warn: {codes:?}");
}
