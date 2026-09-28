//! Builder behaviour against the retail WADs.
//!
//! Game-gated: built by the `retail` feature (`cargo xtask retail-test`), these read the retail
//! vz.wad named by the repo-root `.mercs2-local.toml` and fail if it is absent. The hermetic builder
//! tests are in `build.rs`.

mod common {
    pub mod build;
}

use common::build::{
    anim_tracks, anim_trnm, fake_png, raw_shipment, read_back_animation, read_back_movie, scratch,
    shipment, solid_png, tiny_gfx_movie, ANIM_CLIP,
};
use mercs2_quartermaster::build::{self, BuildError, Destination};
use mercs2_quartermaster::compat::PlanInput;
use mercs2_quartermaster::discover;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// edit_state_machine — the destruction writer, end to end
// ---------------------------------------------------------------------------

/// The FIRST destructible model in the stack — scanned by hash so the test never depends on knowing
/// a retail name. Returns a `0x…` target (the manifest accepts a bare hash anywhere a name goes) and
/// the model's edit inputs. Panics when the stack carries none.
fn a_destructible(game: &mut mercs2_quartermaster::GameStack) -> (String, mercs2_quartermaster::game::ModelEditInputs) {
    use mercs2_formats::types::TYPE_ID_MODEL;
    for hash in game.asset_hashes(TYPE_ID_MODEL) {
        if let Some(inputs) = game.model_container_for_edit(hash) {
            if mercs2_formats::orchestrator::parse_state_machine(&inputs.container).is_some() {
                return (format!("0x{hash:08X}"), inputs);
            }
        }
    }
    panic!("no model in the retail stack carries a state machine")
}

/// ★ edit_state_machine, end to end against retail: a NO-OP edit (extract the machine, change
/// nothing, rebuild) must produce an overlay whose model container is BYTE-IDENTICAL to the base,
/// carried as a single-model block with a primary ASET row — no block-mate shadowed, no dangling
/// rung. This is the whole "recompute the row, don't shadow everything" claim, checked on real bytes.
#[test]
fn edit_state_machine_noop_ships_a_byte_identical_single_model_block() {
    let mut game = retail_game();
    let (target, inputs) = a_destructible(&mut game);
    let hash = mercs2_quartermaster::manifest::asset_hash(&target);

    // Extract the machine to the `states:` baseline and ship it UNCHANGED — the no-op.
    let sm = mercs2_formats::orchestrator::parse_state_machine(&inputs.container).unwrap();
    let yaml = mercs2_quartermaster::states::extract(&sm, |_| None);

    let dir = scratch("esm_noop");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/states.yaml"), &yaml).unwrap();
    let s = shipment(
        &dir,
        &format!("  - kind: edit_state_machine\n    target: \"{target}\"\n    states: src/states.yaml\n"),
    );

    let report = build::build(&s, Some(&mut game), None, None, None).expect("edit_state_machine must build");
    let wad_path = report.wad.expect("a WAD must be emitted");
    let on_disk = std::fs::read(&wad_path).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1, "one model, one block — nothing else shadowed");
    let block = &contents.blocks[0];

    // The ASET row: primary rung re-pointed to our block, every finer rung sentinelled to coarse
    // tier (we carry only the primary), so nothing dangles.
    let row = &block.aset_entries[0];
    assert_eq!(row.asset_hash, hash);
    assert_eq!(row.u32_2 & 0xFFFF, 0xFFFF, "_P001 must sentinel — the rung is not carried");
    assert_eq!(row.u32_1, 0xFFFF_FFFF, "_P002/_P003 must sentinel — not dangle");

    // The block is a proper single-entry table carrying the MODEL container with its original
    // field_c, and — this is the no-op claim — the container is byte-identical to the base.
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    assert_eq!(count, 1);
    assert_eq!(entries[0].name_hash, hash);
    assert_eq!(entries[0].field_c, inputs.field_c, "field_c must be preserved, not guessed");
    let pos = 4 + entries.len() * 16;
    let container = &dec[pos..pos + entries[0].chunk_size as usize];
    assert_eq!(container, inputs.container.as_slice(), "a no-op edit must reproduce the base container");
}

/// A real edit — rename a state — lands in the shipped container and still parses, while every other
/// byte of the container stays put (same length, only the one hash changed).
#[test]
fn edit_state_machine_renames_a_state_end_to_end() {
    let mut game = retail_game();
    let (target, inputs) = a_destructible(&mut game);
    let sm = mercs2_formats::orchestrator::parse_state_machine(&inputs.container).unwrap();
    let (ni, si) = sm
        .nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| (!n.states.is_empty()).then_some((i, 0)))
        .expect("a node with a state");

    // Extract, rename one state to a NOVEL name, ship it. Extract shows a state by its vocabulary
    // name where one exists, else hex — replace whichever token it used.
    let mut yaml = mercs2_quartermaster::states::extract(&sm, |_| None);
    let old_token = mercs2_formats::orchestrator::state_name(sm.nodes[ni].states[si].name_hash)
        .map(String::from)
        .unwrap_or_else(|| format!("0x{:08X}", sm.nodes[ni].states[si].name_hash));
    assert!(yaml.contains(&old_token), "extract should show the state token: {yaml}");
    yaml = yaml.replacen(&old_token, "qm_test_renamed_state", 1);

    let dir = scratch("esm_rename");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/states.yaml"), &yaml).unwrap();
    let s = shipment(
        &dir,
        &format!("  - kind: edit_state_machine\n    target: \"{target}\"\n    states: src/states.yaml\n"),
    );
    let report = build::build(&s, Some(&mut game), None, None, None).expect("build the rename");
    // Renaming to a novel hash decouples the state from the engine's SetState — M0193 must warn.
    assert!(
        report.log.iter().any(|l| l.contains("M0193")),
        "renaming a state off-vocabulary must warn (M0193): {:?}",
        report.log
    );
    let on_disk = std::fs::read(report.wad.unwrap()).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let dec = mercs2_formats::sges::decompress_sges(&contents.blocks[0].compressed_data).unwrap();
    let (_c, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    let pos = 4 + entries.len() * 16;
    let container = &dec[pos..pos + entries[0].chunk_size as usize];

    // A hash is fixed-width, so a rename does not resize the container.
    assert_eq!(container.len(), inputs.container.len(), "a rename must not resize the container");
    // The shipped machine reads back with the new name and nothing else moved.
    let reparsed = mercs2_formats::orchestrator::parse_state_machine(container).expect("re-parse");
    assert_eq!(
        reparsed.nodes[ni].states[si].name_hash,
        mercs2_formats::hash::pandemic_hash_m2("qm_test_renamed_state"),
    );
    assert_eq!(reparsed.nodes.len(), sm.nodes.len());
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

/// The retail game stack, opened from the vz.wad the repo-root `.mercs2-local.toml` names. Panics
/// when the config, the archive, or the stack cannot be had.
fn retail_game() -> mercs2_quartermaster::GameStack {
    let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    eprintln!("game stack: {}", vz.display());
    mercs2_quartermaster::GameStack::open(std::slice::from_ref(&vz))
        .unwrap_or_else(|e| panic!("could not open the game stack {}: {e}", vz.display()))
}

// ---------------------------------------------------------------------------
// Against the real game
// ---------------------------------------------------------------------------

/// End-to-end texture replacement against the retail WADs.
#[test]
fn a_texture_replacement_builds_end_to_end() {
    let mut game = retail_game();

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

/// Nobody resizes a texture. An image whose size differs from the shipped texture is a
/// hard error, and the message names BOTH sizes — the image's and the target's — so the author knows
/// what to export at without looking it up.
#[test]
fn size_mismatch_refused_message_names_both_sizes() {
    let mut game = retail_game();
    let hash = mercs2_formats::hash::pandemic_hash_m2("al_hum_boss_ub");
    let existing = game.texture(hash).expect("al_hum_boss_ub must exist in vz.wad");
    let (w, h) = (existing.width, existing.height);
    // Half the width, same height: a different size, and still a multiple of 4.
    let (iw, ih) = (w / 2, h);
    assert_ne!((iw, ih), (w, h));

    let dir = scratch("size_mismatch");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/t.png"), solid_png(iw, ih)).unwrap();
    let s = shipment(
        &dir,
        "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/t.png\n",
    );
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains(&format!("{iw}x{ih}")), "names the image size: {m}");
            assert!(m.contains(&format!("{w}x{h}")), "names the target size: {m}");
        }
        other => panic!("expected a Lower refusal, got {other:?}"),
    }
    assert!(!dir.join("_build/test-shipment.wad").exists(), "nothing is written");
}

/// M0007/M0009 against real ASET rows, in both directions.
///
/// The classes are the opposite of what "character texture" intuition suggests, which is exactly
/// why this is measured rather than assumed:
///   `pmc_hum_mattias_v3_ub`  primary, single-block         -> silent
///   `al_hum_boss_ub`         NON-primary, 4-rung streamed  -> M0007 + M0009
#[test]
fn streamed_and_shared_targets_are_flagged_and_resident_ones_are_not() {
    let game = retail_game();
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
    let mut game = retail_game();
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
    let mut game = retail_game();
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

/// ★ `add_outfit` end to end — the recipe from the mod-model plan (`workshop-mods-rebuild-01-mod-model.md`).
///
/// It is the composed case: a Data half (the model, injected into a hero-rigged donor) and a Script
/// half (the `_tOutfits` row), and the Script half only works because it goes through the linker
/// rather than shipping its own block.
#[test]
fn add_outfit_builds_model_and_wardrobe_row_together() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
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

/// The vendored Lua corpus. Panics when the corpus is missing.
fn corpus_for_tests() -> PathBuf {
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let c = d.join("crates/mercs2_script/corpus/mercs2-luacd/src");
        if c.is_dir() {
            return c;
        }
        dir = d.parent();
    }
    panic!("the vendored Lua corpus crates/mercs2_script/corpus/mercs2-luacd/src is missing")
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
            "format: 2\nshipment: {{ name: {name}, version: 1.0.0, target: retail }}\n\
             contributions:\n  - kind: add_outfit\n    name: {asset}\n    slug: {slug}\n\
             \x20   display: {slug}\n    wearer: mattias\n    model: src/m.glb\n\
             \x20   donor: pmc_hum_mattias\n"
        ),
    )
    .unwrap();
    discover::open(dir).expect("open")
}

/// A request over opened Shipments, with `arg:<n>` ids in the given order — what `qm link` builds
/// from positional directories.
fn request<'a>(shipments: &[&'a discover::LoadedShipment], ids: &'a [String]) -> Vec<PlanInput<'a>> {
    shipments
        .iter()
        .zip(ids)
        .map(|(s, id)| PlanInput { id, shipment: s })
        .collect()
}

fn arg_ids(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("arg:{i}")).collect()
}

/// ★ The deploy-side failure this design exists to prevent.
///
/// Each Shipment's own overlay carries a `scripts_vz` linked from ITS mutations only. WAD
/// resolution is last-mounted-wins, so installing two of them means one Shipment's Lua disappears
/// silently. `link_installed` sees all of them at once and emits one overlay that supersedes both.
#[test]
fn two_installed_shipments_both_survive_the_deploy_link() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
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

    let deploy = root.join("deploy");
    let ids = arg_ids(2);
    let report = build::link_installed(&request(&[&a, &b], &ids), &mut game, &corpus, &deploy)
        .expect("deploy link");
    eprintln!("{}", report.log.join("\n"));

    // ONE contract for locating outputs. `build` has always written a placement record; the link
    // step wrote a bare `zz-quartermaster-link.wad` and nothing else, so a deploy tool had to
    // special-case a filename for this directory and read the record for every other one. A second
    // undocumented output path is how a deploy step ends up guessing which files to mount — and a
    // wrong guess there does not fail loudly, it mounts the wrong set and the game boots.
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(deploy.join("placement.json")).unwrap())
            .expect("the link step writes a placement record too");
    assert_eq!(record["format"], 1);
    let placed = record["placements"].as_array().expect("placements array");
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["name"], build::LINK_WAD_NAME);
    assert_eq!(
        placed[0]["destination"]["kind"], "overlay",
        "the mount instruction has to be IN the record — 'zz-' sorting last is a convention a \
         reader would have to know, not a contract it can read"
    );
    assert_eq!(placed[0]["sha256"], report.placements[0].sha256);

    assert_eq!(
        report.linked.len(),
        1,
        "one target, compiled once for both Shipments"
    );
    assert_eq!(
        report.linked[0].contributors,
        vec!["sean-devlin", "roze-skin"],
        "neither requires the other, so the request order is the load order"
    );
    // The plan the link followed is written beside the placement record.
    assert!(deploy.join("load-plan.json").is_file(), "the link writes its load plan");

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

    let mut game = retail_game();
    let corpus = corpus_for_tests();
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
        "format: 2\nshipment: { name: resident-lua, version: 1.0.0, target: retail }\n\
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

/// The request order is the tie-break: with no `requires` between two Shipments, reversing the
/// request reverses the link order, and the same request always gives the same bytes.
#[test]
fn the_deploy_link_follows_the_request_order() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    let root = scratch("deploy_order");
    let a = outfit_shipment(&root.join("a"), "aaa-mod", "qm_a", "Aaa");
    let b = outfit_shipment(&root.join("b"), "zzz-mod", "qm_b", "Zzz");
    let ids = arg_ids(2);

    let one = build::link_installed(&request(&[&a, &b], &ids), &mut game, &corpus, &root.join("one"))
        .unwrap();
    let again = build::link_installed(&request(&[&a, &b], &ids), &mut game, &corpus, &root.join("again"))
        .unwrap();
    let two = build::link_installed(&request(&[&b, &a], &ids), &mut game, &corpus, &root.join("two"))
        .unwrap();
    assert_eq!(
        one.placements[0].sha256, again.placements[0].sha256,
        "one request must always link to the same bytes"
    );
    assert_eq!(one.linked[0].contributors, vec!["aaa-mod", "zzz-mod"]);
    assert_eq!(
        two.linked[0].contributors,
        vec!["zzz-mod", "aaa-mod"],
        "reversing the request reverses the order"
    );
}

/// M0209 reaches the plan: `qm link` appends a warning per unresolved literal import to the plan's
/// findings, on the item whose source carries it, and the plan stays ok.
#[test]
fn an_unresolved_literal_import_is_a_warning_in_the_link_plan() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    let root = scratch("deploy_m0209");
    std::fs::create_dir_all(root.join("mod/src")).unwrap();
    std::fs::write(
        root.join("mod/src/probe.lua"),
        "local lib = import(\"qm_no_such_module\")\nreturn {}\n",
    )
    .unwrap();
    let s = shipment(
        &root.join("mod"),
        "  - kind: add_script\n    name: qm_m0209_probe\n    source: src/probe.lua\n",
    );
    let out = root.join("out");
    let ids = arg_ids(1);
    let report = build::link_installed(&request(&[&s], &ids), &mut game, &corpus, &out)
        .expect("an unresolved import is a warning, not a refusal");
    assert!(report.plan.ok);
    let m0209: Vec<_> = report.plan.findings.iter().filter(|f| f.code == "M0209").collect();
    assert_eq!(m0209.len(), 1, "{:?}", report.plan.findings);
    assert_eq!(m0209[0].items, vec!["arg:1".to_string()]);
    assert!(m0209[0].message.contains("qm_no_such_module"), "{}", m0209[0].message);
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("load-plan.json")).unwrap()).unwrap();
    assert_eq!(written["ok"], true);
    assert_eq!(written["findings"][0]["code"], "M0209");
    assert_eq!(written["findings"][0]["severity"], "warning");
    assert_eq!(
        written["findings"][0]["refs"],
        serde_json::json!([{ "section": "items", "index": 0 }])
    );
}

/// `(key hash, text)` for every entry of a stringdb container, read through the codec.
fn string_entries(container: &[u8]) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    mercs2_formats::stringdb::apply_container(container, |db| {
        out = db.entries.iter().map(|e| (e.key_hash, e.text.clone())).collect();
        Ok(())
    })
    .expect("a readable stringdb container");
    out
}

/// A Shipment that edits `english` with `edits` and adds `adds` to it (one `0xKEY = text` each).
fn english_editor(
    dir: &Path,
    name: &str,
    edits: &[String],
    adds: &[String],
) -> discover::LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/english.txt"), edits.join("\n") + "\n").unwrap();
    std::fs::write(dir.join("src/english-new.txt"), adds.join("\n") + "\n").unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 2\nshipment: {{ name: {name}, version: 1.0.0, target: retail }}\n\
             contributions:\n  - kind: edit_stringdb\n    target: english\n    strings: src/english.txt\n\
             \x20 - kind: add_stringdb_keys\n    target: english\n    strings: src/english-new.txt\n"
        ),
    )
    .unwrap();
    discover::open(dir).expect("open")
}

/// Each `edit_stringdb` Shipment's own overlay carries a WHOLE edited copy of the
/// table, so installed together the last mounted would silently drop the other's edits. The link
/// merges every Shipment's edits into ONE table: disjoint keys both survive, and a key both edit
/// takes the text of the later Shipment in the load order — for an edited key and for a key both
/// add alike.
#[test]
fn disjoint_stringdb_edits_both_survive_link() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    use mercs2_formats::types::{TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB};
    let english = mercs2_formats::hash::pandemic_hash_m2("english");
    let base = game
        .container_for_asset(english, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
        .expect("retail vz.wad must carry the english string table");
    let keys: Vec<u32> = string_entries(&base).iter().map(|(k, _)| *k).take(3).collect();
    let [only_a, both, only_b] = [keys[0], keys[1], keys[2]];
    let added = mercs2_formats::stringdb::key_hash("[QmMergeTest.Added]");
    assert!(!keys.contains(&added) && string_entries(&base).iter().all(|(k, _)| *k != added));

    let root = scratch("deploy_stringdb");
    let a = english_editor(
        &root.join("a"),
        "strings-a",
        &[format!("0x{only_a:08X} = QM A ONLY"), format!("0x{both:08X} = QM A BOTH")],
        &["[QmMergeTest.Added] = QM A ADDED".to_string()],
    );
    let b = english_editor(
        &root.join("b"),
        "strings-b",
        &[format!("0x{both:08X} = QM B BOTH"), format!("0x{only_b:08X} = QM B ONLY")],
        &["[QmMergeTest.Added] = QM B ADDED".to_string()],
    );
    let table_path = build::stringdb_block_path(english);

    let merged_text = |out: &Path, key: u32| -> String {
        let wad = std::fs::read(out.join(build::LINK_WAD_NAME)).expect("a link WAD");
        let contents = mercs2_formats::patch_wad::read_patch_wad(&wad).expect("re-read");
        let blocks: Vec<_> = contents.blocks.iter().filter(|b| b.path_string == table_path).collect();
        assert_eq!(blocks.len(), 1, "exactly one merged english table in the link WAD");
        let dec = mercs2_formats::sges::decompress_sges(&blocks[0].compressed_data).expect("sges");
        string_entries(&dec[20..])
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, t)| t)
            .expect("key present")
    };

    let forward = root.join("forward");
    let ids = arg_ids(2);
    let report = build::link_installed(&request(&[&a, &b], &ids), &mut game, &corpus, &forward)
        .expect("link");
    assert!(report.plan.ok, "{:?}", report.plan.findings);
    assert!(report.plan.link_block_paths.contains(&table_path));
    assert_eq!(merged_text(&forward, only_a), "QM A ONLY");
    assert_eq!(merged_text(&forward, only_b), "QM B ONLY");
    assert_eq!(merged_text(&forward, both), "QM B BOTH", "the later Shipment in the order wins");
    assert_eq!(merged_text(&forward, added), "QM B ADDED", "an added key merges the same way");

    let reverse = root.join("reverse");
    build::link_installed(&request(&[&b, &a], &ids), &mut game, &corpus, &reverse).expect("link");
    assert_eq!(merged_text(&reverse, both), "QM A BOTH", "reversed order, reversed winner");
    assert_eq!(merged_text(&reverse, added), "QM A ADDED");
    assert_eq!(merged_text(&reverse, only_b), "QM B ONLY");
}

/// `replace_stringdb_text` in the merge: a text replacement resolves
/// against the table AS MERGED SO FAR in load order, and one that matches nothing is an error.
/// Shipment `setter` sets a key's text to a free-text marker; `replacer` replaces that text. Setter
/// first: the replacement sees the marker and wins. Replacer first: it runs against the base, where
/// no entry has that text, and the link fails naming the Shipment, the table and the text.
#[test]
fn a_text_replacement_resolves_against_the_table_merged_so_far() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    use mercs2_formats::types::{TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB};
    let english = mercs2_formats::hash::pandemic_hash_m2("english");
    let base = game
        .container_for_asset(english, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
        .expect("retail vz.wad must carry the english string table");
    let entries = string_entries(&base);
    let key = entries[0].0;
    // Free text, with the characters a key-based parser would have choked on.
    let marker = "QM merge marker: set by text-setter = 100%";
    assert!(entries.iter().all(|(_, t)| t != marker));

    let root = scratch("deploy_replace_text");
    let setter = english_editor(&root.join("setter"), "text-setter", &[format!("0x{key:08X} = {marker}")], &[]);
    let rdir = root.join("replacer");
    std::fs::create_dir_all(rdir.join("src")).unwrap();
    std::fs::write(rdir.join("src/pairs.txt"), format!("# the setter's text\n{marker}\tQM REPLACED\n")).unwrap();
    std::fs::write(
        rdir.join("manifest.yaml"),
        "format: 2\nshipment: { name: text-replacer, version: 1.0.0, target: retail }\n\
         contributions:\n  - kind: replace_stringdb_text\n    target: english\n    pairs: src/pairs.txt\n",
    )
    .unwrap();
    let replacer = discover::open(&rdir).expect("open");

    let table_path = build::stringdb_block_path(english);
    let ids = arg_ids(2);

    let forward = root.join("forward");
    let report = build::link_installed(&request(&[&setter, &replacer], &ids), &mut game, &corpus, &forward)
        .expect("link");
    assert!(report.plan.ok, "{:?}", report.plan.findings);
    let wad = std::fs::read(forward.join(build::LINK_WAD_NAME)).expect("a link WAD");
    let contents = mercs2_formats::patch_wad::read_patch_wad(&wad).expect("re-read");
    let block = contents.blocks.iter().find(|b| b.path_string == table_path).expect("table");
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let text = string_entries(&dec[20..]).into_iter().find(|(k, _)| *k == key).map(|(_, t)| t);
    assert_eq!(text.as_deref(), Some("QM REPLACED"), "the replacement sees the earlier write");

    let reverse = root.join("reverse");
    match build::link_installed(&request(&[&replacer, &setter], &ids), &mut game, &corpus, &reverse) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("text-replacer"), "names the Shipment: {m}");
            assert!(m.contains("english"), "names the table: {m}");
            assert!(m.contains(marker), "names the unmatched text: {m}");
        }
        other => panic!("run first, the replacement matches nothing and must fail, got {other:?}"),
    }
    assert!(!reverse.join(build::LINK_WAD_NAME).exists(), "nothing is linked");
}

/// The kind's own lowering applies the same rule: a pair whose old text no entry of the shipped
/// table has is refused, naming the Shipment, the table and the text.
#[test]
fn a_text_replacement_that_matches_nothing_fails_the_build() {
    let mut game = retail_game();
    let dir = scratch("replace_text_miss");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/pairs.txt"), "No retail string reads like this: qm-miss\tX\n").unwrap();
    let s = shipment(&dir, "  - kind: replace_stringdb_text\n    target: english\n    pairs: src/pairs.txt\n");
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("test-shipment") && m.contains("english"), "{m}");
            assert!(m.contains("No retail string reads like this: qm-miss"), "{m}");
        }
        other => panic!("expected a Lower refusal, got {other:?}"),
    }
}

/// A Shipment's own `qm build` applies its string contributions in contribution order, against the
/// table as edited so far — the same code as `qm link`. So a text
/// replacement of the text the Shipment's own earlier `edit_stringdb` wrote builds, into ONE
/// block for the table; in the reverse order the replacement runs first, matches nothing, and the
/// build fails naming the Shipment, the table and the text.
#[test]
fn a_shipment_build_applies_its_string_writes_in_order() {
    let mut game = retail_game();
    use mercs2_formats::types::{TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB};
    let english = mercs2_formats::hash::pandemic_hash_m2("english");
    let base = game
        .container_for_asset(english, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
        .expect("retail vz.wad must carry the english string table");
    let entries = string_entries(&base);
    let key = entries[0].0;
    let marker = "QM own-build marker: written by edit_stringdb = step 1";
    assert!(entries.iter().all(|(_, t)| t != marker));

    let make = |label: &str, edit_first: bool| {
        let dir = scratch(label);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/english.txt"), format!("0x{key:08X} = {marker}\n")).unwrap();
        std::fs::write(dir.join("src/pairs.txt"), format!("{marker}\tQM OWN BUILD REPLACED\n")).unwrap();
        let edit = "  - kind: edit_stringdb\n    target: english\n    strings: src/english.txt\n";
        let replace = "  - kind: replace_stringdb_text\n    target: english\n    pairs: src/pairs.txt\n";
        let body = if edit_first { format!("{edit}{replace}") } else { format!("{replace}{edit}") };
        (dir.clone(), shipment(&dir, &body))
    };

    let (dir, s) = make("own_order_ok", true);
    let report = build::build(&s, Some(&mut game), None, None, None).expect("edit then replace builds");
    let wad = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&wad).expect("re-read");
    let table_path = build::stringdb_block_path(english);
    let tables: Vec<_> = contents.blocks.iter().filter(|b| b.path_string == table_path).collect();
    assert_eq!(tables.len(), 1, "ONE block for the table, carrying both contributions");
    assert_eq!(contents.blocks.len(), 1);
    let dec = mercs2_formats::sges::decompress_sges(&tables[0].compressed_data).expect("sges");
    let text = string_entries(&dec[20..]).into_iter().find(|(k, _)| *k == key).map(|(_, t)| t);
    assert_eq!(text.as_deref(), Some("QM OWN BUILD REPLACED"));
    assert!(dir.join("_build/test-shipment.wad").is_file());

    let (_, s) = make("own_order_reversed", false);
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e @ BuildError::Lower { .. }) => {
            let m = e.to_string();
            assert!(m.contains("test-shipment") && m.contains("english") && m.contains(marker), "{m}");
        }
        other => panic!("replace before edit must fail with the no-match error, got {other:?}"),
    }
}

/// A plan that is not ok links nothing: the plan is written as the explanation, and no link WAD or
/// placement record appears.
#[test]
fn an_unsatisfied_requirement_refuses_the_link() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    let root = scratch("deploy_refused");
    let a = outfit_shipment(&root.join("a"), "needs-ess", "qm_a", "Aaa");
    std::fs::write(
        root.join("a/manifest.yaml"),
        std::fs::read_to_string(root.join("a/manifest.yaml"))
            .unwrap()
            .replace("contributions:", "load: { requires: [ess] }\ncontributions:"),
    )
    .unwrap();
    let a = discover::open(&a.root).expect("reopen");
    let out = root.join("out");
    let ids = arg_ids(1);
    match build::link_installed(&request(&[&a], &ids), &mut game, &corpus, &out) {
        Err(BuildError::Plan(plan)) => {
            assert!(!plan.ok);
            assert!(plan.findings.iter().any(|f| f.code == "M0204"), "{:?}", plan.findings);
        }
        other => panic!("expected BuildError::Plan, got {other:?}"),
    }
    assert!(out.join("load-plan.json").is_file(), "the refused plan is written");
    assert!(!out.join(build::LINK_WAD_NAME).exists(), "nothing is linked");
    assert!(!out.join("placement.json").exists(), "nothing is placed");
}

/// `qm build` refuses while a superseded file is in the game folder. The probe is read-only, so this
/// names a file every game folder has — the `data` directory `vz.wad` sits in — rather than writing
/// anything into the real install.
#[test]
fn build_refuses_a_superseded_file() {
    let mut game = retail_game();
    let dir = scratch("superseded");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: sup, version: 1.0.0, target: retail }\n\
         supersedes:\n  - { dest: game_root, file: DATA }\ncontributions: []\n",
    )
    .unwrap();
    let s = discover::open(&dir).expect("open");
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(BuildError::Superseded { shipment, relative }) => {
            assert_eq!(shipment, "sup");
            assert_eq!(relative, "DATA");
        }
        other => panic!("expected BuildError::Superseded, got {other:?}"),
    }
}

/// Nothing to link means no overlay — an overlay that merely restates the base block is noise a
/// user would have to reason about. It does NOT mean no placement record.
#[test]
fn a_set_with_no_script_mods_emits_no_link_wad() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    let root = scratch("deploy_none");
    std::fs::create_dir_all(root.join("tex/src")).unwrap();
    std::fs::write(root.join("tex/src/t.png"), fake_png()).unwrap();
    let s = shipment(
        &root.join("tex"),
        "  - kind: replace_texture\n    target: al_hum_boss_ub\n    image: src/t.png\n",
    );
    let out = root.join("out");
    let ids = arg_ids(1);
    let report = build::link_installed(&request(&[&s], &ids), &mut game, &corpus, &out).expect("link");
    assert!(report.wad.is_none());
    assert!(report.linked.is_empty());

    // …but the RECORD is still written. "No overlay to mount" and "link never ran" are opposite
    // facts a deploy step must act on differently, and an absent file cannot tell them apart. The
    // empty array says which one it is.
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("placement.json")).unwrap())
            .expect("a placement record exists even with nothing to place");
    assert_eq!(record["format"], 1);
    assert_eq!(
        record["placements"].as_array().map(|a| a.len()),
        Some(0),
        "nothing to place, stated rather than implied"
    );
}

// ---------------------------------------------------------------------------
// raw — the open lower bound
// ---------------------------------------------------------------------------
//
// `raw` is the only kind with no encoder behind it, so its tests are mostly about REFUSALS. The
// declared `touches` is the sole source of the ASET rows, which is why it has to agree with the
// payload's own entry table in both directions — and why each disagreement gets its own fixture.

/// ★ A real retail block carried through `raw` verbatim, against the real `vz.wad`.
///
/// The synthetic fixtures above prove the checks; this proves the passthrough on bytes we did not
/// author. A donor block is the right subject because it is a shape the engine demonstrably loads,
/// so anything the lowering breaks shows up as a difference from something known-good.
#[test]
fn a_retail_block_survives_being_carried_through_raw() {
    let game = retail_game();
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
// The emitted-artifact self-check
// ---------------------------------------------------------------------------

/// Every WAD the builder writes must have been read back and checked first.
///
/// This asserts the WIRING, which is the part that can silently rot: `artifact_checks` has its own
/// unit tests, but a self-check nobody calls is worth nothing. A real build's log must therefore
/// show the WAD came back out of `read_patch_wad` cleanly.
#[test]
fn a_built_wad_is_read_back_and_self_checked() {
    let mut game = retail_game();

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

// ---------------------------------------------------------------------------
// add_movie
// ---------------------------------------------------------------------------

/// ★ `add_movie` end to end against the retail WADs — the injector the Scaleform work was missing.
#[test]
fn add_movie_builds_end_to_end() {
    let mut game = retail_game();
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

// ──────────────────────────────────────────────────────────────────────────── edit_stringdb

/// ★ `edit_stringdb` end to end against retail vz.wad, then re-read.
///
/// The corpus already proved the CODEC byte-identical against all six retail language tables; this
/// proves the CONTRIBUTION — reading the base table from the game stack, splicing the edit into a
/// same-hash block, and re-reading it out of the emitted WAD. Edits a key by its HASH (read live
/// from the table it is about to edit) so the test does not depend on knowing a bracket-key name.
#[test]
fn edit_stringdb_builds_end_to_end() {
    let mut game = retail_game();
    use mercs2_formats::types::{TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB};
    let english = mercs2_formats::hash::pandemic_hash_m2("english");

    // Read the base table and pick a real key + its shipped text, live.
    let base = game
        .container_for_asset(english, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
        .expect("retail vz.wad must carry the english string table");
    let (ks, kl) = {
        // Re-extract KEYS to read a real key hash.
        let find = |tag: &[u8; 4]| -> (usize, usize) {
            let le = |o: usize| u32::from_le_bytes([base[o], base[o + 1], base[o + 2], base[o + 3]]) as usize;
            let data = le(4);
            for i in 0..le(16) {
                let row = 20 + i * 20;
                if &base[row..row + 4] == tag {
                    return (data + le(row + 4), le(row + 8));
                }
            }
            panic!("no {tag:?} chunk");
        };
        find(b"KEYS")
    };
    let key_hash = u32::from_le_bytes([base[ks + 4], base[ks + 5], base[ks + 6], base[ks + 7]]);
    let _ = kl;

    let dir = scratch("edit_stringdb_e2e");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // The new text is longer than most single strings, exercising the resize path.
    std::fs::write(
        dir.join("src/english.txt"),
        format!("0x{key_hash:08X} = QUARTERMASTER EDIT — a deliberately long replacement string\n"),
    )
    .unwrap();
    let s = shipment(
        &dir,
        "  - kind: edit_stringdb\n    target: english\n    strings: src/english.txt\n",
    );

    let report = build::build(&s, Some(&mut game), None, None, None).expect("must build");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();

    // Re-read the emitted overlay and confirm the ONE key changed and the table still parses.
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];
    let row = &block.aset_entries[0];
    assert_eq!(row.asset_hash, english, "the row must name the english table");
    assert_eq!(row.u32_3, TYPE_ID_STRINGDB, "type id must dispatch to the stringdb loader");
    assert_eq!(row.u32_2 & 0xFFFF, 0xFFFF, "a string table has no LOD rung; must be primary");

    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let container = &dec[20..]; // past the single-entry block table
    let ex = |tag: &[u8; 4]| {
        let le = |o: usize| u32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]) as usize;
        let data = le(4);
        for i in 0..le(16) {
            let r = 20 + i * 20;
            if &container[r..r + 4] == tag {
                return container[data + le(r + 4)..data + le(r + 4) + le(r + 8)].to_vec();
            }
        }
        panic!("no {tag:?}");
    };
    let db = mercs2_formats::stringdb::parse(&ex(b"KEYS"), &ex(b"STRS")).expect("parse edited table");
    let edited = db.entries.iter().find(|e| e.key_hash == key_hash).expect("key present");
    assert!(
        edited.text.contains("QUARTERMASTER EDIT"),
        "the edited key must carry the new text, got {:?}",
        edited.text
    );
}

/// A key that is not in the table is refused BY NAME rather than silently dropped — a dropped
/// correction is the exact failure the SYEK/KEYS tag confusion once produced.
#[test]
fn edit_stringdb_refuses_an_unknown_key() {
    let mut game = retail_game();
    let dir = scratch("edit_stringdb_unknown");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/english.txt"), "[No.Such.Key.At.All] = x\n").unwrap();
    let s = shipment(
        &dir,
        "  - kind: edit_stringdb\n    target: english\n    strings: src/english.txt\n",
    );
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e) => assert!(format!("{e:?}").contains("No.Such.Key"), "must name the key: {e:?}"),
        Ok(_) => panic!("an unknown key must not build"),
    }
}

// ──────────────────────────────────────────────────────────────────────────── donor auto-pick

/// Omitting `donor:` on an add_outfit picks the wearer's hero model and proceeds. With a placeholder model the build then fails at model
/// IMPORT, which is exactly the proof: auto-pick ran and handed off.
#[test]
fn add_outfit_without_donor_auto_picks_and_proceeds() {
    let mut game = retail_game();
    let dir = scratch("auto_donor");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // Not a real glTF — enough to prove control reached the importer past auto-pick.
    std::fs::write(dir.join("src/model.glb"), b"not a real glb").unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_outfit\n    name: pmc_hum_auto\n    slug: AutoFit\n    \
         display: Auto\n    wearer: mattias\n    model: src/model.glb\n",
    );

    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e) => {
            let m = format!("{e:?}");
            assert!(
                !m.contains("auto-pick"),
                "auto-pick should have run, not refused: {m}"
            );
            // It got as far as trying to read the (bogus) model — the importer, past donor.
            assert!(
                m.to_lowercase().contains("glb")
                    || m.to_lowercase().contains("gltf")
                    || m.to_lowercase().contains("model")
                    || m.to_lowercase().contains("import"),
                "expected a model-import failure after auto-pick, got: {m}"
            );
        }
        Ok(_) => panic!("a bogus model should not build"),
    }
}

/// A wearer the auto-pick does not know is refused clearly rather than silently hosting on nothing.
#[test]
fn add_outfit_without_donor_and_unknown_wearer_is_refused() {
    let mut game = retail_game();
    let dir = scratch("auto_donor_bad");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/model.glb"), b"x").unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_outfit\n    name: pmc_hum_x\n    slug: X\n    display: X\n    \
         wearer: bulldog\n    model: src/model.glb\n",
    );
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(e) => assert!(
            format!("{e:?}").contains("bulldog"),
            "the refusal should name the unknown wearer: {e:?}"
        ),
        Ok(_) => panic!("an unknown wearer with no donor must not build"),
    }
}

// ──────────────────────────────────────────────────────────────────────── M0192 (add_movie)

/// M0192: a movie under a name retail ships is a REPLACEMENT and stays quiet; a novel name warns,
/// because the engine references movies by fixed name and nothing points at a new one.
#[test]
fn add_movie_replacement_is_quiet_but_a_novel_name_warns() {
    let game = retail_game();
    use mercs2_quartermaster::lint;

    // `MINIMAP` is a real cfx_pack in vz.wad (the mounted stack) — replacing it is proven.
    let dir = scratch("m0192_quiet");
    let quiet = shipment(&dir, "  - kind: add_movie\n    name: MINIMAP\n    movie: src/x.gfx\n");
    assert!(
        !lint::game_checks(&quiet.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0192"),
        "replacing a shipped movie must not warn"
    );

    // A name retail does not ship: the movie would sit in the WAD, referenced by nothing.
    let dir2 = scratch("m0192_fires");
    let fires = shipment(
        &dir2,
        "  - kind: add_movie\n    name: qm_totally_novel_movie\n    movie: src/x.gfx\n",
    );
    assert!(
        lint::game_checks(&fires.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0192"),
        "a novel movie name must warn that nothing references it"
    );

    // ★ The SAME novel name via `add_ui` must stay quiet — add_ui bakes the FlashWidget that plays
    // it, so the movie IS referenced. This is exactly the gap M0192 exists to catch, now closed by a
    // typed kind rather than a hand-written patch_lua.
    let dir3 = scratch("m0192_add_ui_quiet");
    let ui = shipment(
        &dir3,
        "  - kind: add_ui\n    name: qm_totally_novel_movie\n    movie: src/x.gfx\n",
    );
    assert!(
        !lint::game_checks(&ui.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0192"),
        "add_ui wires its own movie up, so a novel name must NOT warn"
    );
}

// ──────────────────────────────────────────────────────────────────────── M0194 (activate_layer)

/// M0194: activating a layer retail actually ships stays quiet; a name no layer carries warns,
/// because `MrxLayerManager.MarkForAddition` keys on the layer NAME and a wrong one reaches nothing.
#[test]
fn activate_layer_of_a_real_layer_is_quiet_but_an_unknown_name_warns() {
    let game = retail_game();
    use mercs2_quartermaster::lint;
    use mercs2_quartermaster::manifest::asset_hash;
    use mercs2_quartermaster::names::NameTable;

    // Reverse a real layer hash to the name that produced it, so the quiet fixture names a layer the
    // stack genuinely carries — verified by re-hashing (the name→hash→row round-trip M0194 walks).
    let names = NameTable::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/production_names.json"),
    )
    .expect("the vendored name table must load");
    let layer_type = mercs2_formats::types::TYPE_ID_LAYER;
    let real_layer = game
        .asset_hashes(layer_type)
        .into_iter()
        .find_map(|h| {
            let n = names.reverse(h)?;
            (n.starts_with("vz_state") && asset_hash(n) == h).then(|| n.to_string())
        })
        .expect("at least one vz_state layer hash must reverse to its name");

    let dir = scratch("m0194_quiet");
    let quiet = shipment(&dir, &format!("  - kind: activate_layer\n    layer: {real_layer}\n"));
    assert!(
        !lint::game_checks(&quiet.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0194"),
        "activating a layer the stack ships ({real_layer}) must not warn"
    );

    // A name no layer carries — MarkForAddition would reach nothing at runtime.
    let dir2 = scratch("m0194_fires");
    let fires = shipment(
        &dir2,
        "  - kind: activate_layer\n    layer: vz_state_qm_totally_novel\n",
    );
    assert!(
        lint::game_checks(&fires.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0194"),
        "an unknown layer name must warn that MarkForAddition reaches nothing"
    );

    // The warning also covers `replaces:` names — a typo in the layer being removed is just as dead.
    let dir3 = scratch("m0194_replaces");
    let repl = shipment(
        &dir3,
        &format!(
            "  - kind: activate_layer\n    layer: {real_layer}\n    replaces:\n      - vz_state_qm_no_such_layer\n"
        ),
    );
    assert!(
        lint::game_checks(&repl.manifest, &game)
            .iter()
            .any(|d| d.rule.code == "M0194"),
        "an unknown replaces: name must warn too"
    );
}

/// ★ activate_layer end to end: a Shipment with no Data half builds into the same `qm_modloader`
/// script `add_ui` mints, whose compiled bytecode carries the `MrxLayerManager` marks — the layer to
/// add and the one it replaces — reached by the one-line trampoline on `wifpmcinterior`.
#[test]
fn activate_layer_builds_the_layer_marks_into_the_mod_loader() {
    let mut game = retail_game();
    let corpus = corpus_for_tests();
    let root = scratch("activate_layer_e2e");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("manifest.yaml"),
        "format: 2\nshipment: { name: layer-mod, version: 1.0.0, target: retail }\n\
         contributions:\n  - kind: activate_layer\n    layer: vz_state_pmccon004_destroyed\n\
         \x20   replaces:\n      - vz_state_pmccon004_pristine\n",
    )
    .unwrap();
    let s = discover::open(&root).expect("open shipment");

    let out = root.join("build");
    let report =
        build::build(&s, Some(&mut game), None, Some(&out), Some(&corpus)).expect("build");
    eprintln!("{}", report.log.join("\n"));

    let bytes = std::fs::read(report.wad.as_ref().expect("a wad")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&bytes).expect("re-read");

    // The block carrying `wifpmcinterior` (scripts_vz) is republished, now with `qm_modloader`.
    let mut found = false;
    for blk in &contents.blocks {
        let dec = mercs2_formats::sges::decompress_sges(&blk.compressed_data).expect("sges");
        let Ok(parsed) = mercs2_formats::scripts_block::ScriptsBlock::parse(&dec) else {
            continue;
        };
        parsed.verify_csums().expect("CSUMs");
        let Some(idx) = parsed.find_script_by_name("qm_modloader") else {
            continue;
        };
        found = true;
        let luaq = parsed.extract_lua(idx).unwrap();
        // Lua 5.1 keeps string constants in the clear, so the marks and layer names are in the bytes.
        let text = String::from_utf8_lossy(&luaq);
        for needle in [
            "MrxLayerManager",
            "MarkForAddition",
            "vz_state_pmccon004_destroyed",
            "MarkForRemoval",
            "vz_state_pmccon004_pristine",
        ] {
            assert!(text.contains(needle), "qm_modloader bytecode missing {needle:?}");
        }
        // The resident still gets the one-line trampoline that imports and runs it.
        let ti = parsed
            .find_script_by_name("wifpmcinterior")
            .expect("wifpmcinterior present in the same block");
        let tramp = String::from_utf8_lossy(&parsed.extract_lua(ti).unwrap()).to_string();
        assert!(tramp.contains("qm_modloader"), "the trampoline must import qm_modloader");
    }
    assert!(found, "the build must mint a qm_modloader script");
}

// ---------------------------------------------------------------------------
// edit_world — the placement-layer overlay (vz_state / layers_static)
// ---------------------------------------------------------------------------

/// ★ Editing a placement layer, end to end: load a real vz_state block, move one entity in place,
/// and emit an overlay that shadows the base by PTHS path — the edited placement reads back moved,
/// and a NO-OP emit reproduces the layer's decoded bytes exactly.
#[test]
fn edit_world_emits_a_shadowing_layer_overlay_and_no_op_round_trips() {
    let mut game = retail_game();
    // A vz_state block is small and carries placements; find one.
    let mut inputs = None;
    for needle in ["vz_state_pmccon004", "vz_state_pmc", "vz_state"] {
        if let Some(i) = game.layer_block_for_edit(needle) {
            if mercs2_formats::placement::load_placements(&i.block).map(|v| !v.is_empty()).unwrap_or(false) {
                inputs = Some(i);
                break;
            }
        }
    }
    let inputs = inputs.expect(
        "no vz_state block with placements in the retail stack (tried vz_state_pmccon004, vz_state_pmc, vz_state)",
    );

    let places = mercs2_formats::placement::load_placements(&inputs.block).expect("parse");
    let target = places[0].clone();

    // Move it, emit the overlay.
    let mut edited = inputs.block.clone();
    let new_pos = [target.pos[0] + 40.0, target.pos[1], target.pos[2] - 15.0];
    let moved_n = mercs2_formats::placement::patch_transform(&mut edited, target.key, Some(new_pos), None);
    assert!(moved_n >= 1);
    let block = build::emit_edited_layer(&inputs, &edited).expect("emit the overlay");

    // The overlay shadows the base block at its own PTHS path, and restates its ASET rows.
    assert_eq!(block.path_string, inputs.path, "the overlay must carry the base's path to shadow it");
    assert_eq!(block.aset_entries.len(), inputs.rows.len(), "every ASET row must be restated");

    // The emitted block decodes to the edited content, and the moved entity reads back.
    let decoded = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    assert_eq!(decoded, edited, "the overlay must carry exactly the edited block");
    let after = mercs2_formats::placement::load_placements(&decoded).expect("re-parse");
    let m = after.iter().find(|p| p.key == target.key).expect("entity present");
    assert_eq!(m.pos, new_pos, "the moved entity must read back at the new position");

    // NO-OP: emitting the unedited layer decodes byte-for-byte to the base.
    let noop = build::emit_edited_layer(&inputs, &inputs.block).expect("emit no-op");
    let noop_decoded = mercs2_formats::sges::decompress_sges(&noop.compressed_data).expect("sges");
    assert_eq!(noop_decoded, inputs.block, "a no-op layer edit must reproduce the block's decoded bytes");
}

/// ★ edit_world end to end: a Shipment that moves one entity in a real layer builds into an overlay
/// that shadows the base layer and reads the entity back at its new position.
#[test]
fn edit_world_builds_an_overlay_that_moves_an_entity() {
    let mut game = retail_game();
    // Find a layer needle that resolves to a block with placements.
    let mut found = None;
    for needle in ["vz_state_pmccon004", "vz_state_pmc", "vz_state"] {
        if let Some(inp) = game.layer_block_for_edit(needle) {
            if let Ok(p) = mercs2_formats::placement::load_placements(&inp.block) {
                if !p.is_empty() {
                    found = Some((needle.to_string(), inp.block.clone(), p[0].clone(), inp.path.clone()));
                    break;
                }
            }
        }
    }
    let (needle, base_block, target, base_path) = found.expect(
        "no vz_state layer with placements in the retail stack (tried vz_state_pmccon004, vz_state_pmc, vz_state)",
    );
    let new_pos = [target.pos[0] + 55.0, target.pos[1], target.pos[2] - 10.0];

    let dir = scratch("edit_world");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/world.yaml"),
        format!(
            "edits:\n  - entity: \"0x{:08X}\"\n    pos: [{}, {}, {}]\n",
            target.key, new_pos[0], new_pos[1], new_pos[2]
        ),
    )
    .unwrap();
    let s = shipment(
        &dir,
        &format!("  - kind: edit_world\n    layer: \"{needle}\"\n    edits: src/world.yaml\n"),
    );

    let report = build::build(&s, Some(&mut game), None, None, None).expect("edit_world must build");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    // The overlay shadows the base layer block at its PTHS path.
    let block = contents.blocks.iter().find(|b| b.path_string == base_path).expect("layer overlay present");
    let decoded = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let after = mercs2_formats::placement::load_placements(&decoded).expect("re-parse");
    let moved = after.iter().find(|p| p.key == target.key).expect("entity present");
    assert_eq!(moved.pos, new_pos, "the entity must read back at the new position");
    // Every other entity is where it was.
    let before = mercs2_formats::placement::load_placements(&base_block).unwrap();
    for (a, b) in before.iter().zip(&after) {
        if a.key != target.key {
            assert_eq!(a.pos, b.pos, "a non-target entity moved");
        }
    }
}

// ---------------------------------------------------------------------------
// add_animation / replace_animation — the retail clip container
// ---------------------------------------------------------------------------

/// Against retail: a replace of a shipped Havok clip builds under the target's own hash, and a
/// replace of a MANM keyframe animation is refused by kind rather than overwritten with a clip.
#[test]
fn replace_animation_replaces_a_clip_and_refuses_a_keyframe_animation() {
    use mercs2_formats::anim_container::{classify, parse_container, AnimContainerKind};
    use mercs2_formats::types::{TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION};
    let mut game = retail_game();
    let (mut clip_target, mut keyframe_target) = (None, None);
    for h in game.asset_hashes(TYPE_ID_ANIMATION) {
        let c = game
            .container_for_asset(h, TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION)
            .expect("every animation row resolves to a container");
        match classify(&parse_container(&c).expect("retail container reads")).expect("known kind") {
            AnimContainerKind::HavokClip { .. } if clip_target.is_none() => clip_target = Some(h),
            AnimContainerKind::Keyframe if keyframe_target.is_none() => keyframe_target = Some(h),
            _ => {}
        }
        if clip_target.is_some() && keyframe_target.is_some() {
            break;
        }
    }
    let clip_target = clip_target.expect("retail ships Havok clips");
    let keyframe_target = keyframe_target.expect("retail ships 29 MANM keyframe animations");

    let dir = scratch("replace_animation");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), anim_trnm(anim_tracks())).unwrap();
    let s = shipment(
        &dir,
        &format!("  - kind: replace_animation\n    target: \"0x{clip_target:08X}\"\n    clip: src/c.hkx\n    trnm: src/c.trnm\n"),
    );
    let report = build::build(&s, Some(&mut game), None, None, None).expect("a clip replace builds");
    let (_, hash, chunks) = read_back_animation(&std::fs::read(report.wad.unwrap()).unwrap());
    assert_eq!(hash, clip_target, "same hash");
    assert_eq!(chunks[1].body, ANIM_CLIP);

    let dir = scratch("replace_animation_manm");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), anim_trnm(anim_tracks())).unwrap();
    let s = shipment(
        &dir,
        &format!("  - kind: replace_animation\n    target: \"0x{keyframe_target:08X}\"\n    clip: src/c.hkx\n    trnm: src/c.trnm\n"),
    );
    match build::build(&s, Some(&mut game), None, None, None) {
        Err(BuildError::Lower { message, .. }) => assert!(message.contains("MANM"), "{message}"),
        other => panic!("a MANM target must be refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// add_model (rigid) with `textures:`
// ---------------------------------------------------------------------------

/// Against retail: a rigid `add_model` with a diffuse map ships the map as its own texture block and
/// repoints the HOST group's material at it. Asserted in the emitted MTRL itself: the host material's
/// diffuse slot names the new texture's hash.
#[test]
fn a_rigid_add_model_with_textures_repoints_the_host_material() {
    let mut game = retail_game();
    let donor = "oc_veh_helicopter_md500";
    let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
    let donor_blk = mercs2_formats::donor::donor_block(&paths, mercs2_formats::hash::pandemic_hash_m2(donor))
        .expect("donor block");
    let n = u32::from_le_bytes(donor_blk[16..20].try_into().unwrap()) as usize;
    let ucfx = &donor_blk[20..20 + n];
    let groups = mercs2_formats::texture::group_prmt_material_indices(ucfx);
    let mats = mercs2_formats::texture::parse_mtrl(ucfx);
    // A host whose every material samples a texture and names a diffuse.
    let host = groups
        .iter()
        .position(|ms| {
            !ms.is_empty()
                && ms.iter().all(|&m| {
                    mats.get(m).is_some_and(|x| {
                        x.flags & build::MTRL_TEXTURED != 0 && x.textures.first().is_some_and(|&h| h != 0)
                    })
                })
        })
        .expect("the donor has a textured group");
    let host_mat = groups[host][0];

    let dir = scratch("add_model_textures");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/prop.glb"), cube_glb()).unwrap();
    std::fs::write(dir.join("src/prop_d.png"), solid_png(64, 64)).unwrap();
    let s = shipment(
        &dir,
        &format!(
            "  - kind: add_model\n    name: qm_test_tex_prop\n    model: src/prop.glb\n    donor: {donor}\n    \
             group: {host}\n    textures:\n      diffuse: src/prop_d.png\n"
        ),
    );
    let report = build::build(&s, Some(&mut game), None, None, None).expect("a textured rigid prop builds");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).expect("re-read");
    let want = mercs2_formats::hash::pandemic_hash_m2("qm_test_tex_prop_dm");
    let model = mercs2_formats::hash::pandemic_hash_m2("qm_test_tex_prop");

    let mut seen_model = false;
    let mut seen_texture = false;
    for block in &contents.blocks {
        let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
        let (_, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        if entries[0].name_hash == want {
            assert_eq!(entries[0].type_hash, mercs2_formats::types::TYPE_HASH_TEXTURE);
            seen_texture = true;
        }
        if entries[0].name_hash == model {
            let emitted = mercs2_formats::texture::parse_mtrl(&dec[20..]);
            assert_eq!(
                emitted[host_mat].textures[0], want,
                "the host material's diffuse must name the new texture"
            );
            let emitted_groups = mercs2_formats::texture::group_prmt_material_indices(&dec[20..]);
            assert_eq!(emitted_groups[host][0], host_mat, "the host keeps its material record");
            seen_model = true;
        }
    }
    assert!(seen_model && seen_texture, "model {seen_model}, texture {seen_texture}");
    let log = report.log.join("\n");
    assert!(log.contains("qm_test_tex_prop_dm"), "{log}");
}
