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
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0140"));
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(
        !dir.join("build").join("test-shipment.wad").exists(),
        "nothing may be emitted"
    );
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
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::GameRequired { .. }) => {
            let msg = e.to_string();
            assert!(
                msg.contains("qm lint"),
                "should say lint still works: {msg}"
            );
            assert!(
                msg.contains("game folder"),
                "should point at configuration: {msg}"
            );
        }
        other => panic!("expected GameRequired, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Honest unsupported reporting
// ---------------------------------------------------------------------------

/// A kind we cannot lower yet must FAIL LOUDLY with the reason, never be skipped — a silently
/// dropped contribution produces a WAD that looks fine and does nothing.
///
/// `add_outfit`, `patch_lua`, `raw` and `native_hook` all used to be here. `edit_state_machine` is
/// what is left, and it is the one kind still genuinely unlowerable.
///
/// The refusal is asserted for CONTENT, not just for its variant. "Not implemented" tells an author
/// nothing they can act on; what they need is which of the four gaps they hit, and that a
/// hand-built block can ship through `raw` today.
#[test]
fn edit_state_machine_refuses_with_the_reason_and_a_way_forward() {
    let dir = scratch("unsupported");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.bin"), b"x").unwrap();
    let s = shipment(
        &dir,
        "  - kind: edit_state_machine\n    target: al_veh_boat_destroyer\n    states: src/a.bin\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Unsupported { .. }) => {
            let m = e.to_string();
            // What is missing: no writer for the chunk family, and no schema for `states:`.
            assert!(m.contains("no serializer"), "{m}");
            assert!(m.contains("`states:` has no schema"), "{m}");
            // And the escape hatch that exists today.
            assert!(m.contains("kind: raw"), "{m}");
            assert!(m.contains("al_veh_boat_destroyer"), "{m}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

#[test]
fn an_empty_shipment_still_emits_a_record_and_a_log() {
    let dir = scratch("empty");
    let s = shipment(&dir, "  []\n");
    let report = build::build(&s, None, None, None, None).expect("empty shipment builds");
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
    build::build(&s, None, None, Some(&out), None).expect("build");
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
    build::build(&s, None, None, None, None).expect("build");
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
            eprintln!(
                "game stack: {} (via {:?})",
                found.path.display(),
                found.origin
            );
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
    let Some(mut game) = discovered_game() else {
        return;
    };

    // Read the target's real dimensions so the fixture matches; a replacement is same-hash and
    // fully resident, so mismatched dimensions are a legitimate hard error.
    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let existing = game
        .texture(hash)
        .expect("al_hum_boss_ub must exist in vz.wad");

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

    let report = build::build(&s, Some(&mut game), None, None, None).expect("build");

    // This target turns out to be a 4-rung STREAMED texture with no primary row of its own, so the
    // game-aware rules fire — and the build still completes, because they are warnings. That pairing
    // is the point: the author is told what changed without being blocked from shipping it.
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.rule.code).collect();
    assert!(
        codes.contains(&"M0007") && codes.contains(&"M0009"),
        "{codes:?}"
    );
    assert!(report
        .diagnostics
        .iter()
        .all(|d| d.severity < mercs2_quartermaster::Severity::Error));

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
    let decompressed = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
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
    let again = build::build(&s, Some(&mut game), None, Some(&dir.join("second")), None)
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
    let Some(game) = discovered_game() else {
        return;
    };
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_quartermaster::lint::{self, aset_row_is_single_block};
    const TEX: u32 = mercs2_formats::types::TYPE_ID_TEXTURE;

    // A hero texture really is single-block AND primary — replacing it changes no residency.
    let (p, s, primary) = *game
        .aset_rows(pandemic_hash_m2("pmc_hum_mattias_v3_ub"), TEX)
        .first()
        .expect("mattias_v3_ub row");
    assert!(
        primary && aset_row_is_single_block(p, s),
        "packed 0x{p:08X} secondary 0x{s:08X}"
    );

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
    let codes: Vec<&str> = lint::game_checks(&fires.manifest, &game)
        .iter()
        .map(|d| d.rule.code)
        .collect();
    assert!(
        codes.contains(&"M0007"),
        "streamed target must warn: {codes:?}"
    );
    assert!(
        codes.contains(&"M0009"),
        "shared target must warn: {codes:?}"
    );
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
    for p in &pos {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let n_off = bin.len();
    for p in &nrm {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let t_off = bin.len();
    for p in &uv {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let i_off = bin.len();
    for i in &idx {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }

    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for c in 0..3 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
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
        lo[0],
        lo[1],
        lo[2],
        hi[0],
        hi[1],
        hi[2],
        idx.len(),
        n_off,
        t_off - n_off,
        i_off - t_off,
        idx.len() * 2,
        bin.len()
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }

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
    let Some(mut game) = discovered_game() else {
        return;
    };
    let dir = scratch("add_model");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/prop.glb"), cube_glb()).unwrap();
    // NOT `deliverycrate` — Plan 04's example donor has NO ASET row of any type in vz.wad, so it
    // cannot host anything. `oc_veh_helicopter_md500` is a real model (type_id 19).
    let s = shipment(
        &dir,
        "  - kind: add_model\n    name: qm_test_prop\n    model: src/prop.glb\n    donor: oc_veh_helicopter_md500\n",
    );

    let report = build::build(&s, Some(&mut game), None, None, None).expect("add_model must build");
    let wad_path = report.wad.expect("a WAD must be emitted");
    let on_disk = std::fs::read(&wad_path).unwrap();
    assert_eq!(report.placements[0].sha256, build::sha256_hex(&on_disk));

    // The same two structural properties the texture path has to hold.
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let block = &contents.blocks[0];
    assert_eq!(
        block.aset_entries[0].u32_2 & 0xFFFF,
        0xFFFF,
        "must register as primary"
    );
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    assert_eq!(count, 1);
    assert_eq!(
        entries[0].name_hash,
        mercs2_formats::hash::pandemic_hash_m2("qm_test_prop")
    );

    // The log records what was injected, so a silently-empty mesh cannot pass unnoticed.
    let log = report.log.join("\n");
    assert!(log.contains("add_model qm_test_prop"), "{log}");
    assert!(
        !log.contains("0 verts"),
        "geometry must have survived the import: {log}"
    );
}

/// Auto-pick is not implemented, so an omitted donor must ASK rather than guess — a wrong host
/// silently produces a prop with the wrong rig and materials.
#[test]
fn add_model_without_a_donor_asks_rather_than_guessing() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let dir = scratch("add_model_nodonor");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/prop.glb"), cube_glb()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_model\n    name: qm_x\n    model: src/prop.glb\n",
    );
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e @ BuildError::Unsupported { .. }) => {
            assert!(e.to_string().contains("auto-pick"), "{e}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// ★ `add_outfit` end to end — the recipe Plan 01 phase 5 is defined by.
///
/// It is the composed case: a Data half (the model, injected into a hero-rigged donor) and a Script
/// half (the `_tOutfits` row), and the Script half only works because it goes through the linker
/// rather than shipping its own block.
#[test]
fn add_outfit_builds_model_and_wardrobe_row_together() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let Some(corpus) = corpus_for_tests() else {
        eprintln!("SKIPPING: no Lua corpus");
        return;
    };
    let dir = scratch("add_outfit");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/sean.glb"), cube_glb()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_outfit\n    name: qm_sean_devlin\n    slug: SeanDevlin\n\
         \x20   display: Sean Devlin\n    wearer: mattias\n    model: src/sean.glb\n\
         \x20   donor: pmc_hum_mattias\n",
    );

    let report = build::build(&s, Some(&mut game), None, None, Some(&corpus))
        .expect("add_outfit must build");
    let log = report.log.join("\n");
    eprintln!("{log}");

    // Both halves must appear: the model injected, and the wardrobe script linked.
    assert!(log.contains("add_outfit qm_sean_devlin"), "{log}");
    assert!(log.contains("wardrobe row mattias/SeanDevlin"), "{log}");
    assert!(
        log.contains("linked wifpmcinterior"),
        "the Script half must go through the linker: {log}"
    );

    // The overlay carries BOTH blocks — the model and the relinked scripts_vz.
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    assert_eq!(
        contents.blocks.len(),
        2,
        "expected a model block and a scripts_vz block"
    );

    // And the linked script really contains our row plus the derived availability lift.
    let script_blk = contents
        .blocks
        .iter()
        .find(|b| b.path_string.to_lowercase().contains("scripts_vz"))
        .expect("a scripts_vz block");
    let dec = mercs2_formats::sges::decompress_sges(&script_blk.compressed_data).expect("sges");
    let parsed = mercs2_formats::scripts_block::ScriptsBlock::parse(&dec).expect("parse");
    parsed.verify_csums().expect("CSUMs must verify");
    let idx = parsed
        .find_by_name("wifpmcinterior")
        .expect("wifpmcinterior present");
    let luaq = parsed.extract_lua(idx).expect("extract");
    assert!(
        luaq.starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER),
        "game dialect"
    );

    // The strings we appended survive into the compiled chunk's constant table.
    let hay = String::from_utf8_lossy(&luaq);
    assert!(
        hay.contains("SeanDevlin"),
        "the outfit Name must be in the constants"
    );
    assert!(
        hay.contains("qm_sean_devlin"),
        "the Model name must be in the constants"
    );
    assert!(
        hay.contains("GetAvailableCostumes"),
        "the derived availability lift must be present, or the outfit is unreachable"
    );
}

/// The lift is emitted by the Quartermaster ONCE, not once per Shipment. Two outfits in one
/// Shipment must still yield exactly one definition.
#[test]
fn the_availability_lift_is_emitted_exactly_once() {
    use mercs2_quartermaster::link;
    let a = link::ScriptMutation {
        shipment: "a".into(),
        target: "wifpmcinterior".into(),
        append: link::outfit_row_append("mattias", "One", "m_one", "One"),
    };
    let b = link::ScriptMutation {
        shipment: "b".into(),
        target: "wifpmcinterior".into(),
        append: link::outfit_row_append("mattias", "Two", "m_two", "Two"),
    };
    let (src, _) = link::linked_source("base\n", &[&a, &b]);
    let epilogue = link::derived_epilogue("wifpmcinterior").unwrap();
    let full = format!("{src}{epilogue}");
    assert_eq!(
        full.matches("function GetAvailableCostumes()").count(),
        1,
        "two hard-coded counts is exactly the bug the derived lift removes"
    );
    assert!(
        full.contains("\"One\"") && full.contains("\"Two\""),
        "both rows must survive"
    );
}

/// An author-supplied display string cannot escape its Lua literal and inject code.
///
/// Asserted by COMPILING the generated row rather than by pattern-matching the text: a substring
/// check cannot tell `\"` from `"`, which is exactly the distinction that matters here. If the
/// escaping failed, the hostile text would become statements and the whole thing would still be
/// valid Lua — so the property is that the payload survives as one *string constant*.
#[test]
fn a_hostile_display_string_stays_a_string() {
    use mercs2_quartermaster::link::outfit_row_append;
    const HOSTILE: &str = "evil\" ) end print(\"pwned";
    let row = outfit_row_append("mattias", "S", "m", HOSTILE);

    // It must compile as a single statement against a table that exists.
    let program = format!("_tOutfits = {{ mattias = {{}} }}\n{row}");
    let chunk = mercs2_luac::compile(&program, "escape_test").expect("generated row must compile");

    // And the hostile text must appear in the constant table verbatim — i.e. as data, not code.
    let hay = String::from_utf8_lossy(&chunk);
    assert!(
        hay.contains(HOSTILE),
        "the payload should survive as one string constant, meaning it was escaped, not executed"
    );
}

fn corpus_for_tests() -> Option<PathBuf> {
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let c = d.join("crates/mercs2_script/corpus/mercs2-luacd/src");
        if c.is_dir() {
            return Some(c);
        }
        dir = d.parent();
    }
    None
}

// ---------------------------------------------------------------------------
// Cross-Shipment link (deploy)
// ---------------------------------------------------------------------------

fn outfit_shipment(dir: &Path, name: &str, asset: &str, slug: &str) -> discover::LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/m.glb"), cube_glb()).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 1\nshipment: {{ name: {name}, version: 1.0.0, target: retail }}\n\
             contributions:\n  - kind: add_outfit\n    name: {asset}\n    slug: {slug}\n\
             \x20   display: {slug}\n    wearer: mattias\n    model: src/m.glb\n\
             \x20   donor: pmc_hum_mattias\n"
        ),
    )
    .unwrap();
    discover::open(dir).expect("open")
}

/// ★ The deploy-side failure this design exists to prevent.
///
/// Each Shipment's own overlay carries a `scripts_vz` linked from ITS mutations only. WAD
/// resolution is last-mounted-wins, so installing two of them means one Shipment's Lua disappears
/// silently. `link_installed` sees all of them at once and emits one overlay that supersedes both.
#[test]
fn two_installed_shipments_both_survive_the_deploy_link() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let Some(corpus) = corpus_for_tests() else {
        return;
    };
    let root = scratch("deploy_link");

    let a = outfit_shipment(&root.join("sean"), "sean-devlin", "qm_sean", "SeanDevlin");
    let b = outfit_shipment(&root.join("roze"), "roze-skin", "qm_roze", "Roze");

    // Each on its own links only its own row — the standalone-valid case, and the trap.
    for (s, mine, theirs) in [(&a, "SeanDevlin", "Roze"), (&b, "Roze", "SeanDevlin")] {
        let muts = build::script_mutations(&s.manifest, &s.root).expect("mutations");
        assert_eq!(muts.len(), 1);
        assert!(muts[0].append.contains(mine));
        assert!(
            !muts[0].append.contains(theirs),
            "a Shipment must not know about the other"
        );
    }

    let report = build::link_installed(&[&a, &b], &mut game, &corpus, &root.join("deploy"))
        .expect("deploy link");
    eprintln!("{}", report.log.join("\n"));

    assert_eq!(
        report.linked.len(),
        1,
        "one target, compiled once for both Shipments"
    );
    assert_eq!(
        report.linked[0].contributors,
        vec!["roze-skin", "sean-devlin"],
        "sorted by Shipment name so the bytes do not depend on install order"
    );

    // The emitted overlay must carry BOTH rows.
    let wad = report.wad.expect("a link WAD");
    assert!(
        wad.ends_with(build::LINK_WAD_NAME),
        "must be named to mount last: {}",
        wad.display()
    );
    let bytes = std::fs::read(&wad).unwrap();
    assert_eq!(report.placements[0].sha256, build::sha256_hex(&bytes));

    let contents = mercs2_formats::patch_wad::read_patch_wad(&bytes).expect("re-read");
    let blk = contents
        .blocks
        .iter()
        .find(|b| b.path_string.contains("scripts_vz"))
        .expect("block");
    let dec = mercs2_formats::sges::decompress_sges(&blk.compressed_data).expect("sges");
    let parsed = mercs2_formats::scripts_block::ScriptsBlock::parse(&dec).expect("parse");
    parsed.verify_csums().expect("CSUMs");
    let idx = parsed.find_by_name("wifpmcinterior").unwrap();
    let luaq = parsed.extract_lua(idx).unwrap();
    let hay = String::from_utf8_lossy(&luaq);

    assert!(
        hay.contains("SeanDevlin"),
        "the first Shipment's outfit must survive"
    );
    assert!(
        hay.contains("Roze"),
        "the SECOND Shipment's outfit must survive — this is the bug"
    );
    assert!(
        hay.contains("qm_sean") && hay.contains("qm_roze"),
        "both models must be referenced"
    );
}

/// ★ A `patch_lua` on a RESIDENT module builds end-to-end into a valid overlay.
///
/// Every script the fix pack needs (`mrxplayer`, `mrxguipda`, `mrxtaskjobcollecttype`) lives in the
/// resident block, which the linker could not reach at all before. This drives the whole path:
/// discover the block, splice, emit, and re-read the emitted WAD.
///
/// It also pins the two properties that make the resident case different from `scripts_vz`:
/// **only the touched block is republished**, and **only SCRIPT rows are claimed** — the resident
/// block's ~6,800 non-script entries must not get sentinel-rung ASET rows, which would republish
/// streaming assets as single-block and stop them streaming.
#[test]
fn a_resident_patch_lua_builds_into_a_valid_overlay() {
    const TYPE_ID_SCRIPT: u32 = 35;

    let Some(mut game) = discovered_game() else {
        return;
    };
    let Some(corpus) = corpus_for_tests() else {
        return;
    };
    let root = scratch("resident_lua");
    std::fs::create_dir_all(root.join("src")).unwrap();
    // A string LITERAL, not a comment: comments do not survive compilation, so a `-- marker` append
    // would leave nothing to assert on in the emitted bytecode.
    std::fs::write(
        root.join("src/append.lua"),
        "_QM_RESIDENT_MARKER = \"fixpack-resident-marker\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("manifest.yaml"),
        "format: 1\nshipment: { name: resident-lua, version: 1.0.0, target: retail }\n\
         contributions:\n  - kind: patch_lua\n    target: mrxplayer\n    append: src/append.lua\n",
    )
    .unwrap();
    let s = discover::open(&root).expect("open shipment");

    let out = root.join("build");
    let report =
        build::build(&s, Some(&mut game), None, Some(&out), Some(&corpus)).expect("build");
    eprintln!("{}", report.log.join("\n"));

    let bytes = std::fs::read(report.wad.as_ref().expect("a wad")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&bytes).expect("re-read");

    // Only the resident block is republished — patching a resident script must not drag
    // `scripts_vz` along.
    assert_eq!(
        contents.blocks.len(),
        1,
        "expected only the touched block, got {:?}",
        contents
            .blocks
            .iter()
            .map(|b| b.path_string.clone())
            .collect::<Vec<_>>()
    );
    let blk = &contents.blocks[0];
    assert!(
        blk.path_string.to_lowercase().contains(r"\resident_p000_q3.block"),
        "wrong block: {}",
        blk.path_string
    );

    // ★ A row for EVERY entry. Claiming only the scripts is the M0004 HANG: an asset carried in a
    // block with no row naming it cannot be resolved by hash, and the world load silently never
    // completes. The build refuses that shape, so this assertion is what keeps it refused.
    let dec = mercs2_formats::sges::decompress_sges(&blk.compressed_data).expect("sges");
    let parsed = mercs2_formats::scripts_block::ScriptsBlock::parse(&dec).expect("parse");
    parsed.verify_csums().expect("CSUMs");
    assert_eq!(
        blk.aset_entries.len(),
        parsed.entries.len(),
        "every entry the block carries needs a row, or the loader wedges (M0004)"
    );

    // And the rows are the BASE WAD's, not synthesised: a mixed block must carry mixed type ids.
    // All-script here would mean we had guessed, and a wrong type_id dispatches the wrong loader.
    let script_rows = blk
        .aset_entries
        .iter()
        .filter(|e| e.u32_3 == TYPE_ID_SCRIPT)
        .count();
    assert!(
        script_rows > 0 && script_rows < blk.aset_entries.len(),
        "expected mixed type ids from the base WAD, got {script_rows}/{} script rows",
        blk.aset_entries.len()
    );

    // And the payload really is our append, compiled.
    let idx = parsed.find_script_by_name("mrxplayer").expect("mrxplayer present");
    let luaq = parsed.extract_lua(idx).unwrap();
    assert!(
        String::from_utf8_lossy(&luaq).contains("fixpack-resident-marker"),
        "the appended source must be in the compiled chunk"
    );
}

/// Deploy order must not change the bytes, or verify-by-hash is meaningless and a saved costume
/// index can shift under a player between deploys.
#[test]
fn the_deploy_link_is_order_independent() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let Some(corpus) = corpus_for_tests() else {
        return;
    };
    let root = scratch("deploy_order");
    let a = outfit_shipment(&root.join("a"), "aaa-mod", "qm_a", "Aaa");
    let b = outfit_shipment(&root.join("b"), "zzz-mod", "qm_b", "Zzz");

    let one = build::link_installed(&[&a, &b], &mut game, &corpus, &root.join("one")).unwrap();
    let two = build::link_installed(&[&b, &a], &mut game, &corpus, &root.join("two")).unwrap();
    assert_eq!(
        one.placements[0].sha256, two.placements[0].sha256,
        "install order must not change the linked bytes"
    );
}

/// Nothing to link means no overlay — an overlay that merely restates the base block is noise a
/// user would have to reason about.
#[test]
fn a_set_with_no_script_mods_emits_no_link_wad() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let Some(corpus) = corpus_for_tests() else {
        return;
    };
    let root = scratch("deploy_none");
    std::fs::create_dir_all(root.join("tex/src")).unwrap();
    std::fs::write(root.join("tex/src/t.png"), fake_png()).unwrap();
    let s = shipment(
        &root.join("tex"),
        "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/t.png\n",
    );
    let report = build::link_installed(&[&s], &mut game, &corpus, &root.join("out")).expect("link");
    assert!(report.wad.is_none());
    assert!(report.linked.is_empty());
}

// ---------------------------------------------------------------------------
// raw — the open lower bound
// ---------------------------------------------------------------------------
//
// `raw` is the only kind with no encoder behind it, so its tests are mostly about REFUSALS. The
// declared `touches` is the sole source of the ASET rows, which is why it has to agree with the
// payload's own entry table in both directions — and why each disagreement gets its own fixture.

/// A payload shaped exactly as a patch block: `[u32 count][count × 16-byte rows][containers…]`.
///
/// Built with `build_texture_block` rather than hand-rolled bytes so the container carries a real
/// UCFX header and a verifying CSUM — the raw lowering runs the engine's own reader over it, and a
/// fixture that could not survive that check would only be testing the check.
fn raw_payload(hash: u32) -> Vec<u8> {
    const DIM: usize = 64;
    const MIPS: usize = 5;
    let body = mercs2_formats::texsize::linear_mip_chain_size(DIM, DIM, b"DXT1", MIPS);
    let td = mercs2_formats::texture::TextureData {
        width: DIM as u32,
        height: DIM as u32,
        format: mercs2_formats::texture::TexFormat::Bc1,
        mip0: Vec::new(),
        all_mips: vec![0u8; body],
        mip_count: MIPS as u32,
    };
    mercs2_formats::texture::build_texture_block(hash, &td)
}

/// Two single-entry payloads spliced into one two-entry block. Splicing by hand is what makes the
/// `[count][rows…][containers…]` layout visible, and a two-entry payload is the only way to pose
/// "the payload carries something `touches` does not claim" WITHOUT also posing the converse.
fn two_entry_payload(a: u32, b: u32) -> Vec<u8> {
    let (pa, pb) = (raw_payload(a), raw_payload(b));
    let mut out = 2u32.to_le_bytes().to_vec();
    out.extend_from_slice(&pa[4..20]);
    out.extend_from_slice(&pb[4..20]);
    out.extend_from_slice(&pa[20..]);
    out.extend_from_slice(&pb[20..]);
    out
}

fn raw_shipment(
    dir: &Path,
    payload: &[u8],
    touches: &str,
    layer: &str,
) -> discover::LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/state.block"), payload).unwrap();
    shipment(
        dir,
        &format!(
            "  - kind: raw\n    description: hand-built block\n    payload: src/state.block\n\
             \x20   target_layer: {layer}\n    touches: [{touches}]\n"
        ),
    )
}

/// `raw` lowers with NO game stack — nothing about opaque bytes needs the retail WADs. That makes
/// this the one end-to-end emission path template CI can exercise, so it asserts the full structural
/// contract rather than just "it returned Ok".
#[test]
fn a_raw_block_lowers_into_the_overlay_as_a_primary_single_entry_block() {
    const HASH: u32 = 0x00C0_FFEE;
    let dir = scratch("raw_ok");
    // A BARE HASH in `touches` — legal, and the spelling that exercises `asset_hash`: `0x00C0FFEE`
    // must resolve to that hash, not to the hash of the string "0x00C0FFEE".
    let s = raw_shipment(&dir, &raw_payload(HASH), "\"0x00C0FFEE\"", "data");

    let report = build::build(&s, None, None, None, None).expect("raw must build without a game");
    let wad_path = report.wad.expect("a WAD must be emitted");
    let on_disk = std::fs::read(&wad_path).unwrap();
    assert_eq!(report.placements[0].sha256, build::sha256_hex(&on_disk));

    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];

    // The row must be PRIMARY: low-16 `0xFFFF`. `0x0000` is not "no rung", it is a rung naming
    // block 0 — the dangling-rung HANG.
    let row = &block.aset_entries[0];
    assert_eq!(row.asset_hash, HASH, "the bare hash IS the hash");
    assert_eq!(row.u32_2 & 0xFFFF, 0xFFFF, "must register as primary");
    assert_eq!(row.u32_1, 0xFFFF_FFFF, "_P002/_P003 must both be sentinel");
    // The type id is derived from the payload's own entry table, never from the author.
    assert_eq!(row.u32_3, mercs2_formats::types::TYPE_ID_TEXTURE);

    // A patch block is `[entry table][containers…]`, never a bare container.
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    assert_eq!(count, 1, "expected a single-entry block table");
    assert_eq!(entries[0].name_hash, HASH);
    assert_eq!(
        &dec[20..24],
        b"UCFX",
        "container starts after the entry table"
    );

    // The bytes must be carried VERBATIM — `raw` promising opaque passthrough and then re-encoding
    // would be the worst of both worlds.
    assert_eq!(
        dec,
        raw_payload(HASH),
        "the payload must survive byte for byte"
    );

    let log = report.log.join("\n");
    assert!(log.contains("raw hand-built block"), "{log}");
    assert!(log.contains("0x00C0FFEE"), "{log}");

    // Determinism: verify-by-hash means nothing if two builds disagree.
    let again =
        build::build(&s, None, None, Some(&dir.join("second")), None).expect("second build");
    assert_eq!(report.placements[0].sha256, again.placements[0].sha256);
}

/// The bug that has actually shipped from this crate, now posed as author input: a bare container
/// where a block was required. The loader reads the `UCFX` magic as an entry count, so the WAD
/// hashes fine and is structural nonsense — the message has to name the shape, not just refuse.
#[test]
fn a_bare_container_payload_is_refused_by_name() {
    let dir = scratch("raw_bare_container");
    let container = raw_payload(0x00C0_FFEE)[20..].to_vec();
    assert_eq!(
        &container[0..4],
        b"UCFX",
        "fixture must be a bare container"
    );
    let s = raw_shipment(&dir, &container, "\"0x00C0FFEE\"", "data");
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("bare CONTAINER"), "{m}");
            assert!(m.contains("entry table"), "must say what to add: {m}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// An already-compressed payload would be compressed twice AND carry a `packed_field` computed from
/// the wrong length — M0002's heap overrun, arrived at from the author's side.
#[test]
fn an_sges_compressed_payload_is_refused() {
    let dir = scratch("raw_sges");
    let packed = mercs2_formats::sges::compress_sges(&raw_payload(0x00C0_FFEE)).unwrap();
    let s = raw_shipment(&dir, &packed, "\"0x00C0FFEE\"", "data");
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            assert!(e.to_string().contains("DECOMPRESSED"), "{e}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// `touches` claiming something the payload does not carry publishes an ASET row pointing at a block
/// with no such asset in it. The lookup resolves, the block loads, and the asset is simply absent.
#[test]
fn a_touch_the_payload_does_not_carry_is_refused() {
    let dir = scratch("raw_missing");
    let s = raw_shipment(
        &dir,
        &raw_payload(0x00C0_FFEE),
        "\"0x00C0FFEE\", \"0xDEADBEEF\"",
        "data",
    );
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("0xDEADBEEF"), "must name the hash: {m}");
            assert!(m.contains("does not carry"), "{m}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// The converse, and the more dangerous direction: an asset in the payload that `touches` omits gets
/// no ASET row (M0004's silent wedge) and is invisible to the conflict system, so two Shipments
/// could overwrite one asset with neither being told.
#[test]
fn an_asset_the_payload_carries_but_does_not_claim_is_refused() {
    let dir = scratch("raw_extra");
    let s = raw_shipment(
        &dir,
        &two_entry_payload(0x00C0_FFEE, 0x0000_BEEF),
        "\"0x00C0FFEE\"",
        "data",
    );
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("0x0000BEEF"), "must name the hash: {m}");
            assert!(m.contains("does not claim"), "{m}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// Both entries claimed: a multi-entry raw payload is legal and mints one row per entry.
#[test]
fn a_multi_entry_payload_mints_a_row_for_every_entry() {
    let dir = scratch("raw_two");
    let s = raw_shipment(
        &dir,
        &two_entry_payload(0x00C0_FFEE, 0x0000_BEEF),
        "\"0x00C0FFEE\", \"0x0000BEEF\"",
        "data",
    );
    let report = build::build(&s, None, None, None, None).expect("build");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let rows = &contents.blocks[0].aset_entries;
    assert_eq!(rows.len(), 2);
    let mut hashes: Vec<u32> = rows.iter().map(|r| r.asset_hash).collect();
    hashes.sort_unstable();
    assert_eq!(hashes, vec![0x0000_BEEF, 0x00C0_FFEE]);
    assert!(
        rows.iter().all(|r| r.u32_2 & 0xFFFF == 0xFFFF),
        "every row must be primary"
    );
}

/// The overlay is a WAD, and only the Data layer lives in one. The other three are refused BY NAME
/// with what to use instead — the script case in particular, where lowering the obvious thing would
/// silently delete every other installed Shipment's Lua.
#[test]
fn the_non_data_layers_are_refused_with_the_kind_to_use_instead() {
    for (layer, expect) in [
        ("script", "patch_lua"),
        ("code", "native_hook"),
        ("runtime", "no artifact"),
    ] {
        let dir = scratch(&format!("raw_layer_{layer}"));
        let s = raw_shipment(&dir, &raw_payload(0x00C0_FFEE), "\"0x00C0FFEE\"", layer);
        match build::build(&s, None, None, None, None) {
            Err(e @ BuildError::Unsupported { .. }) => {
                assert!(e.to_string().contains(expect), "{layer}: {e}");
            }
            other => panic!("expected Unsupported for {layer}, got {other:?}"),
        }
    }
}

/// ★ A real retail block carried through `raw` verbatim, against the real `vz.wad`.
///
/// The synthetic fixtures above prove the checks; this proves the passthrough on bytes we did not
/// author. A donor block is the right subject because it is a shape the engine demonstrably loads,
/// so anything the lowering breaks shows up as a difference from something known-good.
#[test]
fn a_retail_block_survives_being_carried_through_raw() {
    let Some(game) = discovered_game() else {
        return;
    };
    let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
    let hash = mercs2_formats::hash::pandemic_hash_m2("oc_veh_helicopter_md500");
    let donor = mercs2_formats::donor::donor_block(&paths, hash).expect("donor block");

    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&donor);
    assert!(count >= 1);
    let touches = entries
        .iter()
        .map(|e| format!("\"0x{:08X}\"", e.name_hash))
        .collect::<Vec<_>>()
        .join(", ");

    let dir = scratch("raw_retail");
    let s = raw_shipment(&dir, &donor, &touches, "data");
    let report = build::build(&s, None, None, None, None).expect("a retail block must carry");
    eprintln!("{}", report.log.join("\n"));

    let wad = report.wad.expect("a WAD");
    let on_disk = std::fs::read(&wad).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let block = &contents.blocks[0];
    assert_eq!(block.aset_entries.len(), entries.len());
    assert!(block
        .aset_entries
        .iter()
        .all(|r| r.u32_2 & 0xFFFF == 0xFFFF));

    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    assert_eq!(dec, donor, "retail bytes must survive verbatim");

    // The self-check must be clean on our own output, and `verify_emitted` already required it
    // before the write — this asserts the same thing from outside, so a regression in that call
    // site is visible here too.
    assert_eq!(
        mercs2_quartermaster::lint::artifact_checks(&contents.blocks),
        vec![]
    );
    eprintln!(
        "wad_simulator subject: cargo run --bin wad_simulator -- --wad {} --base-wad {} \
         --skip-audio",
        wad.display(),
        paths[0].display()
    );
}

// ---------------------------------------------------------------------------
// native_hook — the Code layer, which emits no WAD at all
// ---------------------------------------------------------------------------

/// A PE image carrying only the headers the loadability check reads.
///
/// Deliberately header-only. `asi_load_blocker` inspects exactly four things — `MZ`, `e_lfanew`,
/// the `PE\0\0` signature, and the COFF `Machine`/`Characteristics` words — so a fixture with a
/// real body would add bytes no assertion depends on. The offsets are pinned against the real
/// `pmc_bb.dll` v3.0.0, which reads `e_lfanew=0x80, machine=0x014C, characteristics=0x230E`.
fn fake_asi(machine: u16, characteristics: u16) -> Vec<u8> {
    let pe_at = 0x80usize;
    let mut out = vec![0u8; pe_at + 24];
    out[0..2].copy_from_slice(b"MZ");
    out[0x3C..0x40].copy_from_slice(&(pe_at as u32).to_le_bytes());
    out[pe_at..pe_at + 4].copy_from_slice(b"PE\0\0");
    let coff = pe_at + 4;
    out[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
    out[coff + 18..coff + 20].copy_from_slice(&characteristics.to_le_bytes());
    // A distinguishable tail, so "the bytes that were written are the bytes we supplied" is a real
    // assertion rather than one two all-zero buffers would also satisfy.
    out.extend_from_slice(b"quartermaster-asi-fixture");
    out
}

/// A 32-bit DLL, the shape the loader can actually load.
fn loadable_asi() -> Vec<u8> {
    fake_asi(0x014C, 0x230E)
}

fn hook_shipment(dir: &Path, file: &str, bytes: &[u8], extra: &str) -> discover::LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join(file), bytes).unwrap();
    shipment(
        dir,
        &format!("  - kind: native_hook\n    target: retail\n    plugin: src/{file}\n{extra}"),
    )
}

/// ★ `native_hook` produces a PLACEMENT, not a block — and the record is what makes the drop
/// reversible. An overlay is undone by deleting one file; an `.asi` in the game folder is not
/// backable-out unless something wrote down what was put where.
#[test]
fn a_native_hook_places_a_file_and_records_its_digest() {
    let dir = scratch("hook_ok");
    let asi = loadable_asi();
    let s = hook_shipment(
        &dir,
        "mybridge.asi",
        &asi,
        "    touches: [\"0x004CF340\"]\n",
    );

    let report = build::build(&s, None, None, None, None).expect("native_hook must build");
    assert!(
        report.wad.is_none(),
        "the Code layer contributes nothing to a WAD"
    );
    assert_eq!(report.placements.len(), 1);
    let p = &report.placements[0];
    assert_eq!(p.name, "mybridge.asi");
    assert_eq!(
        p.destination,
        Destination::GameFolder {
            relative: format!("{}/mybridge.asi", build::ASI_SUBDIR)
        },
        "the builder chooses the path; there is no manifest field that could name the exe"
    );

    // Verified BY HASH against what is on disk, not against the buffer the builder held. The output
    // directory MIRRORS the tree this is copied into, so the same relative path names it in both.
    let written = std::fs::read(
        dir.join("build")
            .join(format!("{}/mybridge.asi", build::ASI_SUBDIR)),
    )
    .expect("the .asi must be emitted");
    assert_eq!(written, asi, "the plugin must be copied verbatim");
    assert_eq!(p.sha256, build::sha256_hex(&written));
    assert_eq!(p.bytes, written.len());

    // The record deploy consumes has to carry all three, or an undo cannot verify what it removes.
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("build/placement.json")).unwrap())
            .unwrap();
    let entry = &doc["placements"][0];
    assert_eq!(entry["name"], "mybridge.asi");
    assert_eq!(entry["sha256"], p.sha256);
    assert_eq!(entry["destination"]["kind"], "game_folder");
    assert_eq!(
        entry["destination"]["relative"],
        format!("{}/mybridge.asi", build::ASI_SUBDIR)
    );

    // The log must state what an ASI is. A recorded digest proves the bytes are unmodified and
    // nothing else, and a green "verified" reads as "safe" to someone installing by clicking.
    let log = report.log.join("\n");
    assert!(log.contains("UNRESTRICTED NATIVE CODE"), "{log}");
    assert!(log.contains(&p.sha256), "{log}");
    assert!(
        log.contains("0x004CF340"),
        "the hooks must be recorded: {log}"
    );
}

/// The chosen destination must be one the loader actually searches. `pmc_bb.dll` v3.0.0 globs
/// `%s*.asi`, `%sscripts\`, `%splugins\` and `%supdate\` — read from the binary, not assumed — so a
/// subdir outside that set would place the file where nothing looks for it.
#[test]
fn the_chosen_subdir_is_one_the_loader_searches() {
    assert!(
        ["", "scripts", "plugins", "update"].contains(&build::ASI_SUBDIR),
        "{} is not a directory pmc_bb.dll globs",
        build::ASI_SUBDIR
    );
}

/// The loader skips its own name, so a plugin shipped as `pmc_bb.asi` is placed correctly, hashes
/// correctly, and is never even considered — nothing is logged, because nothing was tried.
#[test]
fn the_loaders_own_name_is_refused() {
    let dir = scratch("hook_reserved");
    let s = hook_shipment(&dir, "pmc_bb.asi", &loadable_asi(), "");
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            assert!(e.to_string().contains("reserved"), "{e}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// The loader globs `*.asi`. Any other extension is placed and never considered — the quietest
/// failure available, with the file sitting there looking installed.
#[test]
fn a_plugin_that_is_not_an_asi_is_refused() {
    let dir = scratch("hook_ext");
    let s = hook_shipment(&dir, "mybridge.dll", &loadable_asi(), "");
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            assert!(e.to_string().contains("globs `*.asi`"), "{e}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// A plugin the game could not load, caught by its PE header. Both cases fail at `LoadLibrary` —
/// the game is a 32-bit process, and an executable image is not a DLL — and both are visible only
/// in a log the modder has to know to read.
#[test]
fn a_plugin_the_game_cannot_load_is_refused() {
    for (bytes, expect) in [
        (fake_asi(0x8664, 0x230E), "32-bit process"),
        (fake_asi(0x014C, 0x010E), "IMAGE_FILE_DLL"),
        (
            b"not a pe image at all, just some bytes here".to_vec(),
            "PE image",
        ),
    ] {
        let dir = scratch("hook_badpe");
        let s = hook_shipment(&dir, "mybridge.asi", &bytes, "");
        match build::build(&s, None, None, None, None) {
            Err(e @ BuildError::Lower { .. }) => {
                assert!(e.to_string().contains(expect), "{e}");
            }
            other => panic!("expected Lower for {expect}, got {other:?}"),
        }
    }
}

/// A `symbol` with no payload asks the Quartermaster to produce native code, which it does not do.
/// The reason has to point at both real options rather than just refusing.
#[test]
fn a_symbol_without_a_plugin_says_what_to_do_instead() {
    let dir = scratch("hook_symbol");
    let s = shipment(
        &dir,
        "  - kind: native_hook\n    target: retail\n    symbol: MyDetour\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Unsupported { .. }) => {
            let m = e.to_string();
            assert!(m.contains("MyDetour"), "{m}");
            assert!(m.contains("load.requires"), "{m}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// An ASI on a reimpl target is blocked by M0160 before lowering is reached — asserted here so the
/// two mechanisms cannot both be removed on the assumption the other covers it.
#[test]
fn an_asi_on_a_reimpl_target_never_reaches_lowering() {
    let dir = scratch("hook_reimpl");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/mybridge.asi"), loadable_asi()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: native_hook\n    target: reimpl\n    plugin: src/mybridge.asi\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0160"), "{d:?}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(
        !dir.join("build")
            .join(format!("{}/mybridge.asi", build::ASI_SUBDIR))
            .exists(),
        "nothing may be placed"
    );
}

/// A Shipment may carry both layers at once, and both must appear in one record — that is the
/// composite case Modkit's deploy has to handle.
#[test]
fn a_shipment_can_emit_a_wad_and_a_file_together() {
    let dir = scratch("hook_and_wad");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/state.block"), raw_payload(0x00C0_FFEE)).unwrap();
    std::fs::write(dir.join("src/mybridge.asi"), loadable_asi()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: raw\n    payload: src/state.block\n    target_layer: data\n\
         \x20   touches: [\"0x00C0FFEE\"]\n\
         \x20 - kind: native_hook\n    target: retail\n    plugin: src/mybridge.asi\n",
    );
    let report = build::build(&s, None, None, None, None).expect("build");
    assert_eq!(report.placements.len(), 2);
    assert_eq!(report.placements[0].destination, Destination::Overlay);
    assert!(matches!(
        report.placements[1].destination,
        Destination::GameFolder { .. }
    ));
    // Determinism covers the file half too: a placement record whose digests move between builds
    // cannot be verified at deploy.
    let again = build::build(&s, None, None, Some(&dir.join("second")), None).expect("second");
    assert_eq!(
        report
            .placements
            .iter()
            .map(|p| p.sha256.clone())
            .collect::<Vec<_>>(),
        again
            .placements
            .iter()
            .map(|p| p.sha256.clone())
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// place_file — companion files, and the escapes that must not be expressible
// ---------------------------------------------------------------------------
//
// Every test here is hermetic. A companion needs no donor, no target dimensions and no base block,
// so this kind lowers where the retail WADs will never exist — which is where template CI runs, and
// therefore where an author actually finds out.

/// A Shipment with one `place_file`. `file` is written under `src/` and may carry subdirectories.
fn place_shipment(dir: &Path, file: &str, dest: &str, bytes: &[u8]) -> discover::LoadedShipment {
    let path = dir.join("src").join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    shipment(
        dir,
        &format!("  - kind: place_file\n    file: src/{file}\n    dest: {dest}\n"),
    )
}

/// ★ The gap this kind exists to close: an `.asi` whose `.ini` cannot ship is useless. A companion
/// is placed, and the record carries its digest so the drop can be backed out and verified.
#[test]
fn a_place_file_places_a_companion_and_records_its_digest() {
    let dir = scratch("place_ok");
    let ini = b"[GlobalSets]\nmode=quiet\n";
    let s = place_shipment(&dir, "quiet_freeplay_vo.ini", "scripts", ini);

    let report = build::build(&s, None, None, None, None).expect("place_file must build");
    assert!(
        report.wad.is_none(),
        "a companion contributes nothing to a WAD"
    );
    assert_eq!(report.placements.len(), 1);
    let p = &report.placements[0];
    assert_eq!(p.name, "quiet_freeplay_vo.ini");
    assert_eq!(
        p.destination,
        Destination::GameFolder {
            relative: "scripts/quiet_freeplay_vo.ini".into()
        }
    );

    // The digest must match the bytes actually on disk, not the buffer the builder held — a digest
    // of the intended bytes would still verify after a truncated write.
    let written = std::fs::read(dir.join("build/scripts/quiet_freeplay_vo.ini"))
        .expect("the companion must be emitted, mirroring the tree it is copied into");
    assert_eq!(written, ini, "the companion is copied verbatim");
    assert_eq!(p.sha256, build::sha256_hex(&written));
    assert_eq!(p.bytes, written.len());

    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("build/placement.json")).unwrap())
            .unwrap();
    let entry = &doc["placements"][0];
    assert_eq!(entry["name"], "quiet_freeplay_vo.ini");
    assert_eq!(entry["sha256"], p.sha256);
    assert_eq!(entry["destination"]["kind"], "game_folder");
    assert_eq!(
        entry["destination"]["relative"],
        "scripts/quiet_freeplay_vo.ini"
    );
}

/// ★ No destination, for any spelling of `dest:`, can name anything outside the game folder.
///
/// Asserted over `PlaceIn::ALL` rather than over the arms somebody remembered to list, so a
/// destination added later cannot quietly skip the check. Each emitted path must be relative and
/// made only of ordinary components — no root, no `..`, no drive prefix.
#[test]
fn every_destination_stays_inside_the_game_folder() {
    for (i, dest) in mercs2_quartermaster::PlaceIn::ALL.iter().enumerate() {
        let yaml_name = [
            "game_root",
            "scripts",
            "plugins",
            "update",
            "on_boot",
            "on_load",
            "on_key",
        ][i];
        let dir = scratch(&format!("place_dest_{yaml_name}"));
        let s = place_shipment(&dir, "config.ini", yaml_name, b"x");
        let report =
            build::build(&s, None, None, None, None).expect("every destination must build");
        let Destination::GameFolder { relative } = &report.placements[0].destination else {
            panic!("a companion is always a game-folder placement");
        };

        assert_eq!(
            *relative,
            build::place_path(dest.relative_dir(), "config.ini"),
            "the emitted path must be the destination's own literal plus the source filename"
        );
        let p = Path::new(relative);
        assert!(p.is_relative(), "{relative} is not relative");
        assert!(
            p.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "{relative} has a component that is not a plain name"
        );
        assert!(!relative.contains(".."), "{relative}");
        assert!(!relative.contains(':'), "{relative}");
        assert!(!relative.contains('\\'), "{relative}");
        // And the file really lands there, under the build directory that mirrors the game folder.
        assert!(dir.join("build").join(relative).is_file(), "{relative}");
    }
}

/// A destination is a NAME, not a path — so a path is not "rejected", it does not parse. This is
/// the property that makes the exe and the WADs unreachable by construction: there is no field a
/// path could go in.
#[test]
fn a_destination_that_is_a_path_does_not_parse() {
    for attempt in [
        "'..'",
        "'../..'",
        "'/etc'",
        "'C:\\Windows'",
        "'\\\\host\\share'",
        "'scripts/../..'",
        "'data'",
        "'.'",
    ] {
        let dir = scratch("place_dest_path");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/config.ini"), b"x").unwrap();
        std::fs::write(
            dir.join("manifest.yaml"),
            format!(
                "format: 1
shipment: {{ name: test-shipment, version: 1.0.0, target: retail }}
contributions:
  - kind: place_file
    file: src/config.ini
    dest: {attempt}
"
            ),
        )
        .unwrap();
        assert!(
            discover::open(&dir).is_err(),
            "dest: {attempt} must not parse"
        );
    }
}

/// The source path goes through the same checks as every other source, so climbing out of the
/// Shipment is an M0111 error rather than a bespoke rule that could drift from that one.
#[test]
fn a_source_path_that_leaves_the_shipment_is_refused() {
    for file in [
        "../../../etc/passwd",
        "/etc/passwd",
        "src/../../secrets.ini",
    ] {
        let dir = scratch("place_escape");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let s = shipment(
            &dir,
            &format!("  - kind: place_file\n    file: {file}\n    dest: scripts\n"),
        );
        match build::build(&s, None, None, None, None) {
            Err(BuildError::Blocked(d)) => {
                assert!(d.iter().any(|x| x.rule.code == "M0111"), "{file}: {d:?}")
            }
            other => panic!("{file}: expected Blocked, got {other:?}"),
        }
    }
}

/// The lexical check cannot see a symlink; canonicalization can. Without this a Shipment could
/// place `/etc/passwd` into the game folder while every path in the manifest looked local.
#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_shipment_is_refused() {
    let dir = scratch("place_symlink");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let outside = dir.parent().unwrap().join("qm_outside_secret.ini");
    std::fs::write(&outside, b"secret").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("src/config.ini")).unwrap();

    let s = shipment(
        &dir,
        "  - kind: place_file\n    file: src/config.ini\n    dest: scripts\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => assert!(d.iter().any(|x| x.rule.code == "M0111"), "{d:?}"),
        other => panic!("expected Blocked, got {other:?}"),
    }
    let _ = std::fs::remove_file(outside);
}

/// ★ The exe and the WADs. `dest: game_root` is a real destination the loader really globs, and it
/// is also where `Mercenaries2.exe` lives — so the destination being closed is only half the
/// guarantee, and the filename is the other half.
#[test]
fn the_game_executable_and_the_wads_cannot_be_written() {
    for name in [
        "Mercenaries2.exe",
        "Mercenaries2.EXE",
        "vz.wad",
        "shell.WAD",
        "pmc_bb.dll",
        "d3d9.dll",
    ] {
        let dir = scratch("place_forbidden");
        let s = place_shipment(&dir, name, "game_root", b"x");
        match build::build(&s, None, None, None, None) {
            Err(BuildError::Blocked(d)) => {
                assert!(d.iter().any(|x| x.rule.code == "M0162"), "{name}: {d:?}")
            }
            other => panic!("{name}: expected Blocked, got {other:?}"),
        }
    }
}

/// The loader skips its own name, so a file shipped under it is placed correctly and never even
/// considered. The refusal is the one `native_hook` already carries — the same function, so the two
/// kinds cannot drift into disagreeing about what is reserved.
#[test]
fn the_loaders_own_name_cannot_be_placed_as_a_companion() {
    let dir = scratch("place_reserved");
    let s = place_shipment(&dir, build::RESERVED_ASI, "scripts", b"x");
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0162"), "{d:?}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

/// A plugin is not a companion. Allowing one here would route around `native_hook`'s PE checks, its
/// reserved-name refusal and its hooked-address claims, while still producing a file the loader
/// globs and `LoadLibrary`s.
#[test]
fn a_plugin_cannot_be_smuggled_in_as_a_companion() {
    let dir = scratch("place_asi");
    let s = place_shipment(&dir, "evil.asi", "scripts", &loadable_asi());
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            let hit = d.iter().find(|x| x.rule.code == "M0162").expect("M0162");
            assert!(hit.message.contains("native_hook"), "{hit}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

/// A filename that is not a single path component. Every one of these is an ordinary filename on
/// the macOS box that builds the Shipment and an escape on the Windows box that deploys it, which
/// is exactly why it cannot be left to the host filesystem to notice.
#[test]
fn a_filename_that_is_not_one_component_is_refused() {
    for name in [
        "..\\..\\Mercenaries2.exe",
        "../../Mercenaries2.exe",
        "C:\\evil.ini",
        "\\\\host\\share\\evil.ini",
        "sub/dir.ini",
        ".",
        "..",
        "",
    ] {
        assert!(
            build::companion_name_refusal(name).is_some(),
            "{name:?} must be refused"
        );
    }
    // ...and an ordinary companion is not.
    assert_eq!(build::companion_name_refusal("lua_bridge_DEV.ini"), None);
    assert_eq!(build::companion_name_refusal("lua_console.py"), None);
    assert_eq!(build::companion_name_refusal("00_core.lua"), None);
}

/// ★ The real shape: a plugin and the companion it reads, in one Shipment, landing in one record.
#[test]
fn a_plugin_and_its_companion_build_together() {
    let dir = scratch("place_with_hook");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lua_bridge_DEV.asi"), loadable_asi()).unwrap();
    std::fs::write(dir.join("src/lua_bridge_DEV.ini"), b"port=27050\n").unwrap();
    std::fs::write(dir.join("src/lua_console.py"), b"# client\n").unwrap();
    let s = shipment(
        &dir,
        "  - kind: native_hook\n    target: retail\n    plugin: src/lua_bridge_DEV.asi\n\
         \x20 - kind: place_file\n    file: src/lua_bridge_DEV.ini\n    dest: scripts\n\
         \x20 - kind: place_file\n    file: src/lua_console.py\n    dest: scripts\n",
    );
    let report = build::build(&s, None, None, None, None).expect("build");
    let paths: Vec<String> = report
        .placements
        .iter()
        .map(|p| match &p.destination {
            Destination::GameFolder { relative } => relative.clone(),
            Destination::Overlay => "overlay".into(),
        })
        .collect();
    assert_eq!(
        paths,
        vec![
            format!("{}/lua_bridge_DEV.asi", build::ASI_SUBDIR),
            "scripts/lua_bridge_DEV.ini".to_string(),
            "scripts/lua_console.py".to_string(),
        ],
        "the companion has to land in the directory the plugin reads it from"
    );
    // No warning: the .ini is beside its plugin, which is the whole point of M0163.
    assert!(
        !report.diagnostics.iter().any(|d| d.rule.code == "M0163"),
        "{:?}",
        report.diagnostics
    );
}

/// One filename in two destinations is TWO files, not a conflict — and the output mirrors that, so
/// neither can overwrite the other while both records claim their own digest.
#[test]
fn one_filename_in_two_destinations_is_two_files() {
    let dir = scratch("place_two_rungs");
    std::fs::create_dir_all(dir.join("src/boot")).unwrap();
    std::fs::create_dir_all(dir.join("src/load")).unwrap();
    std::fs::write(dir.join("src/boot/init.lua"), b"-- boot\n").unwrap();
    std::fs::write(dir.join("src/load/init.lua"), b"-- load\n").unwrap();
    let s = shipment(
        &dir,
        "  - kind: place_file\n    file: src/boot/init.lua\n    dest: on_boot\n\
         \x20 - kind: place_file\n    file: src/load/init.lua\n    dest: on_load\n",
    );
    let report = build::build(&s, None, None, None, None).expect("two rungs must build");
    assert_eq!(report.placements.len(), 2);
    assert_ne!(
        report.placements[0].sha256, report.placements[1].sha256,
        "each record must describe its own file"
    );
    assert_eq!(
        std::fs::read(dir.join("build/scripts/OnBoot/init.lua")).unwrap(),
        b"-- boot\n"
    );
    assert_eq!(
        std::fs::read(dir.join("build/scripts/OnLoad/init.lua")).unwrap(),
        b"-- load\n"
    );
}

// ---------------------------------------------------------------------------
// The emitted-artifact self-check
// ---------------------------------------------------------------------------

/// Every WAD the builder writes must have been read back and checked first.
///
/// This asserts the WIRING, which is the part that can silently rot: `artifact_checks` has its own
/// unit tests, but a self-check nobody calls is worth nothing. A real build's log must therefore
/// show the WAD came back out of `read_patch_wad` cleanly.
#[test]
fn a_built_wad_is_read_back_and_self_checked() {
    let Some(mut game) = discovered_game() else {
        return;
    };

    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let existing = game
        .texture(hash)
        .expect("al_hum_boss_ub must exist in vz.wad");
    let dir = scratch("selfcheck");
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

    let report = build::build(&s, Some(&mut game), None, None, None).expect("build");
    let wad = report.wad.expect("a WAD must be emitted");
    let contents = mercs2_formats::patch_wad::read_patch_wad(&std::fs::read(&wad).unwrap())
        .expect("the WAD we wrote must read back — verify_emitted already required this");

    // The self-check must be CLEAN on our own output. A finding here is a builder bug: this is the
    // exact shape (single-entry block, sentinel rungs, honest packed_field) our lowering emits.
    let found = mercs2_quartermaster::lint::artifact_checks(&contents.blocks);
    assert_eq!(
        found,
        vec![],
        "our own lowering must not trip the artifact rules"
    );

    // And the build recorded that it ran, so a future refactor that drops the call is visible.
    assert!(
        report.log.iter().any(|l| l.contains("wrote")),
        "the build log must record the emit it verified: {:?}",
        report.log
    );
}

/// The self-check must REFUSE, not warn. A HANG-class defect that still writes a file is worse than
/// no check, because the file's presence reads as success to everything downstream.
#[test]
fn a_hang_class_defect_fails_the_build_rather_than_warning() {
    use mercs2_formats::patch_wad::{AsetEntry, PatchBlock};
    // A rung naming block 9 in a one-block WAD — the M0001 trap, built directly because no manifest
    // can express it (the lowering paths all emit sentinels).
    let blk = PatchBlock::from_decompressed(
        b"payload",
        "blocks\\a.block".into(),
        vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_0009, 19)],
        None,
    )
    .unwrap();
    let found = mercs2_quartermaster::lint::artifact_checks(&[blk]);
    assert!(
        mercs2_quartermaster::lint::blocks_build(&found),
        "a dangling rung must BLOCK: the game gives the modder a frozen loading screen, not an error"
    );
}

// ---------------------------------------------------------------------------
// add_movie
// ---------------------------------------------------------------------------

/// A small, valid, uncompressed `GFX` movie: an AVM1 `DoAction` and a `GFx_ExporterInfo`, which is
/// the shape a GFx 2.x authoring tool emits.
///
/// Synthetic on purpose. A checked-in `.gfx` would be an opaque blob in the tree, and the property
/// under test is that whatever bytes go in come out again unchanged — which a fixture nobody can
/// read makes harder to believe, not easier.
fn tiny_gfx_movie() -> Vec<u8> {
    let mut body = Vec::new();
    body.push(0); // stage RECT: nbits = 0 in the top 5 bits, so the bounds are empty
    body.extend_from_slice(&(30u16 << 8).to_le_bytes()); // 30 fps
    body.extend_from_slice(&1u16.to_le_bytes()); // one frame
    let mut tag = |code: u16, b: &[u8]| {
        assert!(b.len() < 0x3F);
        body.extend_from_slice(&((code << 6) | b.len() as u16).to_le_bytes());
        body.extend_from_slice(b);
    };
    tag(1000, &[0x07, 0x02, 0x00, 0x00]); // GFx_ExporterInfo
    tag(12, &[0x00]); // DoAction: a bare End-of-actions
    tag(1, &[]); // ShowFrame
    tag(0, &[]); // End
    let mut file = Vec::new();
    file.extend_from_slice(b"GFX");
    file.push(8);
    file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

/// Pull the `cfx_pack` container back out of an emitted WAD and return `(name_hash, movie bytes)`.
fn read_back_movie(wad: &[u8]) -> (u32, Vec<u8>) {
    let contents = mercs2_formats::patch_wad::read_patch_wad(wad).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];

    // (1) The ASET row must be PRIMARY — low-16 `0xFFFF`. Any other value names a `_P001` rung one
    // level finer, and a movie has no LOD chain for such a rung to be, so it would dangle. `0x0000`
    // in particular is the M0001 HANG rather than "no rung".
    let row = &block.aset_entries[0];
    assert_eq!(
        row.u32_2 & 0xFFFF,
        0xFFFF,
        "a new movie must register as primary, not as a dangling LOD rung"
    );
    // `patch_wad::AsetEntry` names its words positionally; `u32_3` is the type id the reader side
    // dispatches on. It is what picks the loader, so a movie must resolve to the GFx one.
    assert_eq!(
        row.u32_3,
        mercs2_formats::types::TYPE_ID_CFX_PACK,
        "the row's type id is what picks the loader; a movie must dispatch to the GFx one"
    );

    // (2) A patch block is `[entry table][containers…]`, NOT a bare container. Handing over a raw
    // container makes the loader read the `UCFX` magic as an entry-table field: the WAD hashes fine
    // and is structurally nonsense.
    let decompressed = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&decompressed);
    assert_eq!(count, 1, "expected a single-entry block table");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].type_hash,
        mercs2_formats::types::TYPE_HASH_CFX_PACK
    );
    assert_eq!(
        &decompressed[20..24],
        b"UCFX",
        "the container must start AFTER the 20-byte entry table"
    );
    assert_eq!(
        entries[0].name_hash, row.asset_hash,
        "the row and the block must be talking about the same asset, or the lookup resolves to a \
         block that does not contain it"
    );

    let container = &decompressed[20..];
    let movie = mercs2_formats::ucfx::extract_chunk_body(container, b"data")
        .expect("the container must carry a `data` leaf");
    (entries[0].name_hash, movie)
}

/// ★ `add_movie` end to end against the retail WADs — the injector the Scaleform work was missing.
///
/// Runs automatically when a PC `vz.wad` is discoverable and SKIPS loudly otherwise.
#[test]
fn add_movie_builds_end_to_end() {
    let Some(mut game) = discovered_game() else {
        return;
    };
    let dir = scratch("add_movie");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let movie = tiny_gfx_movie();
    std::fs::write(dir.join("src/ui.gfx"), &movie).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_movie\n    name: qm_test_hud\n    movie: src/ui.gfx\n",
    );

    let report = build::build(&s, Some(&mut game), None, None, None).expect("add_movie must build");
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity < mercs2_quartermaster::Severity::Error),
        "{:?}",
        report.diagnostics
    );

    let wad_path = report.wad.expect("a WAD must be emitted");
    let on_disk = std::fs::read(&wad_path).unwrap();
    assert_eq!(report.placements[0].sha256, build::sha256_hex(&on_disk));

    let (hash, carried) = read_back_movie(&on_disk);
    assert_eq!(
        hash,
        mercs2_formats::hash::pandemic_hash_m2("qm_test_hud"),
        "the asset must be reachable under the name the author wrote"
    );

    // (3) The movie survives verbatim. Not "the same length" — the same bytes, and still a movie:
    // an injector that mangled the payload would still produce a WAD that loads and a container
    // that checksums, and the only symptom would be `GFxLoader read failed` in-game.
    assert_eq!(carried, movie, "the movie must be carried byte for byte");
    let reparsed = mercs2_formats::gfx::GfxMovie::parse(&carried).expect("still a movie");
    assert_eq!(&reparsed.magic, b"GFX");
    assert_eq!(reparsed.version, 8, "retail movies are all version 8");

    // The log records the tag census, so a movie that arrived empty cannot pass unnoticed.
    let log = report.log.join("\n");
    assert!(log.contains("add_movie qm_test_hud"), "{log}");
    assert!(log.contains("4 tag(s)"), "{log}");

    // Determinism: the verify-by-hash mandate only means something if two builds agree byte for byte.
    let again = build::build(&s, Some(&mut game), None, Some(&dir.join("second")), None)
        .expect("second build");
    assert_eq!(
        report.placements[0].sha256, again.placements[0].sha256,
        "two builds of one Shipment must be byte-identical"
    );
}

/// The property that separates this kind from every other Data lowering: it needs NO game stack.
///
/// `replace_texture` reads the target's dimensions and `add_model` borrows a donor's rig, so both
/// fail with `GameRequired`. A movie is self-contained, so this one builds in template CI — where
/// the retail WADs will never exist — and that is worth pinning rather than rediscovering.
#[test]
fn add_movie_needs_no_game_stack() {
    let dir = scratch("add_movie_nogame");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let movie = tiny_gfx_movie();
    std::fs::write(dir.join("src/ui.gfx"), &movie).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_movie\n    name: qm_ci_hud\n    movie: src/ui.gfx\n",
    );

    let report = build::build(&s, None, None, None, None).expect("must build with no game");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let (hash, carried) = read_back_movie(&on_disk);
    assert_eq!(hash, mercs2_formats::hash::pandemic_hash_m2("qm_ci_hud"));
    assert_eq!(carried, movie);
}

/// A compressed `CFX` movie is injected exactly as an uncompressed one is.
///
/// Retail ships 61 `CFX` and 3 `GFX`, so the loader takes either and there is nothing to normalise
/// to. The temptation is to zlib everything "because that is what retail does"; doing so would
/// replace bytes the author verified with bytes nobody has run.
#[test]
fn a_compressed_movie_is_not_re_encoded() {
    let dir = scratch("add_movie_cfx");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // Deflate the body of the same movie into the `CFX` container form: magic, version, declared
    // length, then the zlib stream.
    let plain = tiny_gfx_movie();
    let deflated = {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(&plain[8..]).unwrap();
        e.finish().unwrap()
    };
    let mut cfx = b"CFX\x08".to_vec();
    cfx.extend_from_slice(&(plain.len() as u32).to_le_bytes());
    cfx.extend_from_slice(&deflated);
    std::fs::write(dir.join("src/ui.gfx"), &cfx).unwrap();

    let s = shipment(
        &dir,
        "  - kind: add_movie\n    name: qm_cfx_hud\n    movie: src/ui.gfx\n",
    );
    let report = build::build(&s, None, None, None, None).expect("a CFX movie must build");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let (_, carried) = read_back_movie(&on_disk);
    assert_eq!(
        carried, cfx,
        "a CFX movie must ship as the CFX it arrived as"
    );
    assert!(
        mercs2_formats::gfx::GfxMovie::parse(&carried)
            .expect("still a movie")
            .compressed
    );
}

/// A payload that is not a movie is refused with a message naming what a `.gfx` looks like.
///
/// The alternative is the quiet one: the container would still checksum, the ASET row would still
/// resolve, and the only sign would be a `GFxLoader read failed` line in-game that names no asset.
#[test]
fn a_payload_that_is_not_a_movie_is_refused() {
    let dir = scratch("add_movie_notamovie");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/ui.gfx"),
        b"\x89PNG\r\n\x1a\n this is a texture",
    )
    .unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_movie\n    name: qm_bad\n    movie: src/ui.gfx\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let text = e.to_string();
            assert!(text.contains("Scaleform"), "{text}");
            assert!(text.contains("GFX"), "{text}");
        }
        other => panic!("expected Lower, got {other:?}"),
    }
}

/// Two movies under one name in one Shipment is a self-conflict, not a load-order question: the
/// chunk registry is first-writer-wins, so the second one simply is not there.
#[test]
fn two_movies_under_one_name_are_a_self_conflict() {
    let dir = scratch("add_movie_dup");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.gfx"), tiny_gfx_movie()).unwrap();
    std::fs::write(dir.join("src/b.gfx"), tiny_gfx_movie()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_movie\n    name: qm_dup\n    movie: src/a.gfx\n\
         \x20 - kind: add_movie\n    name: qm_dup\n    movie: src/b.gfx\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0120"), "{d:?}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

// ──────────────────────────────────────────────────────────────────────────────── add_sound

/// Re-read an emitted audio-bank WAD and check the row the loader will dispatch on.
///
/// Same shape as `read_back_movie`, and for the same reason: the survey found `soundbank`,
/// `sounddb` and `wavebank` are the identical opaque `data` container `cfx_pack` is, so they share
/// a builder and must share the assertions that builder's output has to satisfy.
fn read_back_sound(wad: &[u8], type_id: u32, type_hash: u32) -> (u32, Vec<u8>) {
    let contents = mercs2_formats::patch_wad::read_patch_wad(wad).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];
    let row = &block.aset_entries[0];
    assert_eq!(
        row.u32_2 & 0xFFFF,
        0xFFFF,
        "an audio bank has no LOD chain, so anything but the primary sentinel dangles (M0001)"
    );
    assert_eq!(row.u32_3, type_id, "the row's type id is what picks the loader");

    let decompressed = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&decompressed);
    assert_eq!(count, 1, "expected a single-entry block table");
    assert_eq!(entries[0].type_hash, type_hash);
    assert_eq!(&decompressed[20..24], b"UCFX", "container must start after the 20-byte table");
    assert_eq!(entries[0].name_hash, row.asset_hash);

    let container = &decompressed[20..];
    let bytes = mercs2_formats::ucfx::extract_chunk_body(container, b"data")
        .expect("the container must carry a `data` leaf");
    (entries[0].name_hash, bytes)
}

/// ★ Audio becomes shippable — and it needs no game stack, so template CI can run it.
///
/// This kind exists because the container was MEASURED to be an opaque wrapper, not because
/// anything here understands audio: `soundbank` 98/98, `sounddb` 58/58 and `wavebank` 92/93 of
/// retail's containers are a bare `data` leaf. That is 248 assets and ~366 MB of game content that
/// previously had no way into a Shipment at all.
#[test]
fn add_sound_builds_for_every_table_without_a_game() {
    use mercs2_formats::types::*;
    for (yaml_name, type_id, type_hash) in [
        ("wavebank", TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK),
        ("soundbank", TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK),
        ("sounddb", 13, 0xE527_3C14u32),
    ] {
        let dir = scratch(&format!("add_sound_{yaml_name}"));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        // Opaque on purpose: nothing in the pipeline parses these bytes, and a test that fed it a
        // real bank would be asserting the fixture rather than the wrapper.
        let bank: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
        std::fs::write(dir.join("src/bank.bin"), &bank).unwrap();
        let s = shipment(
            &dir,
            &format!(
                "  - kind: add_sound\n    name: amb_ci_test\n    bank: src/bank.bin\n    \
                 sound: {yaml_name}\n"
            ),
        );

        let report = build::build(&s, None, None, None, None).expect("must build with no game");
        let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
        let (hash, carried) = read_back_sound(&on_disk, type_id, type_hash);
        assert_eq!(hash, mercs2_formats::hash::pandemic_hash_m2("amb_ci_test"));
        // VERBATIM. There is no encoder here, so any difference would be corruption.
        assert_eq!(carried, bank, "{yaml_name}: the bank must ship byte-for-byte");
    }
}

/// Two builds of the same Shipment produce the same bytes, or verify-by-hash means nothing.
#[test]
fn add_sound_is_reproducible() {
    let dir = scratch("add_sound_repro");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/bank.bin"), vec![7u8; 1024]).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_sound\n    name: amb_repro\n    bank: src/bank.bin\n    sound: soundbank\n",
    );
    let a = std::fs::read(
        build::build(&s, None, None, None, None).expect("build 1").wad.expect("wad"),
    )
    .unwrap();
    let b = std::fs::read(
        build::build(&s, None, None, None, None).expect("build 2").wad.expect("wad"),
    )
    .unwrap();
    assert_eq!(a, b, "two builds of one Shipment must be byte-identical");
}

/// An empty bank is refused by NAME rather than emitted as a valid-looking container with nothing
/// in it — the loader would be the first thing to find out, and it does not say which asset.
#[test]
fn an_empty_sound_bank_is_refused() {
    let dir = scratch("add_sound_empty");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/bank.bin"), b"").unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_sound\n    name: amb_empty\n    bank: src/bank.bin\n    sound: wavebank\n",
    );
    match build::build(&s, None, None, None, None) {
        Err(e) => {
            let m = format!("{e:?}");
            assert!(m.contains("empty"), "the refusal should name the problem: {m}");
        }
        Ok(_) => panic!("an empty bank must not build"),
    }
}
