//! Builder behaviour.
//!
//! The hermetic tests pin the GATE and the emission contract — those must hold with no game
//! present, since that is the state template CI runs in.
//!
//! The game-dependent tests are **not** `#[ignore]`d. They discover a PC `vz.wad`
//! (`scripts/find-vz-wad.sh --write`, then `game::discover`) and run automatically when one is
//! present, skipping loudly when it is not. `#[ignore]` was the wrong default here: it means the
//! tests that exercise the real format never run unless someone remembers a flag, and those are
//! precisely the ones that caught every structural bug so far.

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
            "  - kind: add_outfit\n    name: o\n    slug: O\n    display: O\n    wearer: mattias\n    model: src/m.glb\n",
            "LINK time",
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


/// Open the discovered game stack, or skip with a message that says how to fix it.
///
/// Skipping rather than failing keeps CI green where the retail WADs will never exist, but the
/// message has to be loud: a test that quietly does nothing is worse than one that is absent,
/// because it reads as coverage.
fn discovered_game() -> Option<mercs2_quartermaster::GameStack> {
    match mercs2_quartermaster::game::discover() {
        Some(found) => {
            eprintln!("game stack: {} (via {:?})", found.path.display(), found.origin);
            match mercs2_quartermaster::GameStack::open(&[found.path]) {
                Ok(g) => Some(g),
                Err(e) => panic!("discovered a WAD but could not open it: {e}"),
            }
        }
        None => {
            eprintln!(
                "SKIPPING: no PC vz.wad discovered. Run `scripts/find-vz-wad.sh --write` \
                 or set MERCS2_VZ_WAD."
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Against the real game
// ---------------------------------------------------------------------------

/// End-to-end texture replacement against the retail WADs.
///
/// Runs automatically when a PC `vz.wad` is discoverable (`scripts/find-vz-wad.sh --write`), and
/// SKIPS loudly otherwise — see `discovered_game`.
#[test]
fn a_texture_replacement_builds_end_to_end() {
    let Some(mut game) = discovered_game() else { return };

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
fn streamed_and_shared_targets_are_flagged_and_resident_ones_are_not() {
    let Some(game) = discovered_game() else { return };
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

// ---------------------------------------------------------------------------
// add_model
// ---------------------------------------------------------------------------

/// Build a minimal, self-contained binary glTF holding one axis-aligned cube.
///
/// Written by hand rather than committed as a binary fixture: it keeps the repo free of an opaque
/// blob, and it exercises the reader against a file whose every byte is accounted for here.
fn cube_glb() -> Vec<u8> {
    // 6 faces x 4 verts. Positions/normals/uvs are generated so the data stays inspectable.
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // +Z
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // -Z
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]), // +X
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), // -X
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]), // +Y
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), // -Y
    ];
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();
    for (n, u, v) in FACES {
        let base = pos.len() as u16;
        for (su, sv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pos.push([
                n[0] + u[0] * su + v[0] * sv,
                n[1] + u[1] * su + v[1] * sv,
                n[2] + u[2] * su + v[2] * sv,
            ]);
            nrm.push(n);
            uv.push([(su + 1.0) * 0.5, (sv + 1.0) * 0.5]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut bin: Vec<u8> = Vec::new();
    for p in &pos { for c in p { bin.extend_from_slice(&c.to_le_bytes()); } }
    let n_off = bin.len();
    for p in &nrm { for c in p { bin.extend_from_slice(&c.to_le_bytes()); } }
    let t_off = bin.len();
    for p in &uv { for c in p { bin.extend_from_slice(&c.to_le_bytes()); } }
    let i_off = bin.len();
    for i in &idx { bin.extend_from_slice(&i.to_le_bytes()); }
    while !bin.len().is_multiple_of(4) { bin.push(0); }

    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for c in 0..3 { lo[c] = lo[c].min(p[c]); hi[c] = hi[c].max(p[c]); }
    }
    let vcount = pos.len();
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"mode":4}}]}}],
"accessors":[
{{"bufferView":0,"componentType":5126,"count":{vcount},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}},
{{"bufferView":1,"componentType":5126,"count":{vcount},"type":"VEC3"}},
{{"bufferView":2,"componentType":5126,"count":{vcount},"type":"VEC2"}},
{{"bufferView":3,"componentType":5123,"count":{},"type":"SCALAR"}}],
"bufferViews":[
{{"buffer":0,"byteOffset":0,"byteLength":{}}},
{{"buffer":0,"byteOffset":{n_off},"byteLength":{}}},
{{"buffer":0,"byteOffset":{t_off},"byteLength":{}}},
{{"buffer":0,"byteOffset":{i_off},"byteLength":{}}}],
"buffers":[{{"byteLength":{}}}]}}"#,
        lo[0], lo[1], lo[2], hi[0], hi[1], hi[2],
        idx.len(),
        n_off, t_off - n_off, i_off - t_off, idx.len() * 2,
        bin.len()
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) { json.push(b' '); }

    let mut glb = Vec::new();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
    glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&[b'B', b'I', b'N', 0]);
    glb.extend_from_slice(&bin);
    glb
}

/// `add_model` end to end: glTF in, donor resolved from the real WAD, overlay out.
#[test]
fn add_model_builds_end_to_end() {
    let Some(mut game) = discovered_game() else { return };
    let dir = scratch("add_model");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/prop.glb"), cube_glb()).unwrap();
    // NOT `deliverycrate` — Plan 04's example donor has NO ASET row of any type in vz.wad, so it
    // cannot host anything. `oc_veh_helicopter_md500` is a real model (type_id 19).
    let s = shipment(
        &dir,
        "  - kind: add_model\n    name: qm_test_prop\n    model: src/prop.glb\n    donor: oc_veh_helicopter_md500\n",
    );

    let report = build::build(&s, Some(&mut game), None, None).expect("add_model must build");
    let wad_path = report.wad.expect("a WAD must be emitted");
    let on_disk = std::fs::read(&wad_path).unwrap();
    assert_eq!(report.placements[0].sha256, build::sha256_hex(&on_disk));

    // The same two structural properties the texture path has to hold.
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let block = &contents.blocks[0];
    assert_eq!(block.aset_entries[0].u32_2 & 0xFFFF, 0xFFFF, "must register as primary");
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    assert_eq!(count, 1);
    assert_eq!(entries[0].name_hash, mercs2_formats::hash::pandemic_hash_m2("qm_test_prop"));

    // The log records what was injected, so a silently-empty mesh cannot pass unnoticed.
    let log = report.log.join("\n");
    assert!(log.contains("add_model qm_test_prop"), "{log}");
    assert!(!log.contains("0 verts"), "geometry must have survived the import: {log}");
}

/// Auto-pick is not implemented, so an omitted donor must ASK rather than guess — a wrong host
/// silently produces a prop with the wrong rig and materials.
#[test]
fn add_model_without_a_donor_asks_rather_than_guessing() {
    let Some(mut game) = discovered_game() else { return };
    let dir = scratch("add_model_nodonor");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/prop.glb"), cube_glb()).unwrap();
    let s = shipment(&dir, "  - kind: add_model\n    name: qm_x\n    model: src/prop.glb\n");
    match build::build(&s, Some(&mut game), None, None) {
        Err(e @ BuildError::Unsupported { .. }) => {
            assert!(e.to_string().contains("auto-pick"), "{e}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

