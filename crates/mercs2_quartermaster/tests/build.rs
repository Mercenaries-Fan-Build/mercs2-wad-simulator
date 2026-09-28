//! Builder behaviour.
//!
//! These tests pin the GATE and the emission contract — those must hold with no game present,
//! since that is the state template CI runs in.
//!
//! The tests that exercise the real format against the retail WADs are game-gated and live in
//! `build_retail.rs`, built by the `retail` feature (`cargo xtask retail-test`).

mod common {
    pub mod build;
}

use common::build::{
    anim_tracks, anim_trnm, fake_png, pcm16_wav, raw_shipment, read_back_animation, read_back_movie,
    scratch, shipment, sound_cue_yaml, tiny_gfx_movie, ANIM_CLIP,
};
use mercs2_quartermaster::build::{self, BuildError, Destination};
use mercs2_quartermaster::discover;
use std::path::Path;

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
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0140"));
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(
        !dir.join("_build").join("test-shipment.wad").exists(),
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
    match build::build(&s, None, None, None, None, None) {
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
// Emission
// ---------------------------------------------------------------------------

#[test]
fn an_empty_shipment_still_emits_a_record_and_a_log() {
    let dir = scratch("empty");
    let s = shipment(&dir, "  []\n");
    let report = build::build(&s, None, None, None, None, None).expect("empty shipment builds");
    assert!(report.wad.is_none(), "nothing to put in a WAD");
    assert!(report.placements.is_empty());
    assert!(dir.join("_build/placement.json").is_file());
    assert!(dir.join("_build/build.log").is_file());
}

#[test]
fn the_output_directory_can_be_redirected() {
    let dir = scratch("outdir");
    let out = dir.join("elsewhere");
    let s = shipment(&dir, "  []\n");
    build::build(&s, None, None, Some(&out), None, None).expect("build");
    assert!(out.join("placement.json").is_file());
    assert!(!dir.join("_build").exists());
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
    build::build(&s, None, None, None, None, None).expect("build");
    let text = std::fs::read_to_string(dir.join("_build/placement.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(doc["format"], build::PLACEMENT_FORMAT);
    assert_eq!(doc["format"], 2);
    assert!(doc["placements"].is_array());
}

// ---------------------------------------------------------------------------
// add_model
// ---------------------------------------------------------------------------

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
    let (src, _) = link::linked_source("base\n", &[&a, &b], &["a".into(), "b".into()])
        .expect("both contributors are in the order");
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

// ---------------------------------------------------------------------------
// Cross-Shipment link (deploy)
// ---------------------------------------------------------------------------

/// With no game, a Shipment that declares `supersedes` cannot be checked, so it does not build.
#[test]
fn build_with_supersedes_and_no_game_is_refused() {
    let dir = scratch("superseded_nogame");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: sup, version: 1.0.0, target: retail }\n\
         supersedes:\n  - { dest: on_load, file: 1_Sup.lua }\ncontributions: []\n",
    )
    .unwrap();
    let s = discover::open(&dir).expect("open");
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Compat(e)) => assert!(e.to_string().contains("supersedes"), "{e}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
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

    let report = build::build(&s, None, None, None, None, None).expect("raw must build without a game");
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
        build::build(&s, None, None, Some(&dir.join("second")), None, None).expect("second build");
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
    let report = build::build(&s, None, None, None, None, None).expect("build");
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
        match build::build(&s, None, None, None, None, None) {
            Err(e @ BuildError::Unsupported { .. }) => {
                assert!(e.to_string().contains(expect), "{layer}: {e}");
            }
            other => panic!("expected Unsupported for {layer}, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// native_hook — the Code layer, which emits no WAD at all
// ---------------------------------------------------------------------------

/// A PE image carrying only the headers the loadability check reads.
///
/// Deliberately header-only. `pe::pe_dll_load_blocker` inspects exactly four things — `MZ`, `e_lfanew`,
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

    let report = build::build(&s, None, None, None, None, None).expect("native_hook must build");
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
        dir.join("_build")
            .join(format!("{}/mybridge.asi", build::ASI_SUBDIR)),
    )
    .expect("the .asi must be emitted");
    assert_eq!(written, asi, "the plugin must be copied verbatim");
    assert_eq!(p.sha256, build::sha256_hex(&written));
    assert_eq!(p.bytes, written.len());

    // The record deploy consumes has to carry all three, or an undo cannot verify what it removes.
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/placement.json")).unwrap())
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
        match build::build(&s, None, None, None, None, None) {
            Err(e @ BuildError::Lower { .. }) => {
                assert!(e.to_string().contains(expect), "{e}");
            }
            other => panic!("expected Lower for {expect}, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// add_runtime_dll — a runtime DLL in the game root, named after its Shipment
// ---------------------------------------------------------------------------

/// A Shipment (`test-shipment`) whose contributions are one `add_runtime_dll` per `(path, bytes)`.
fn runtime_shipment(dir: &Path, dlls: &[(&str, Vec<u8>)]) -> discover::LoadedShipment {
    let mut contributions = String::new();
    for (path, bytes) in dlls {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, bytes).unwrap();
        contributions.push_str(&format!("  - kind: add_runtime_dll\n    dll: {path}\n"));
    }
    shipment(dir, &contributions)
}

/// The lint findings a build was blocked by, or a panic naming what happened instead.
fn blocked_codes(result: Result<build::BuildReport, BuildError>) -> Vec<(String, String)> {
    match result {
        Err(BuildError::Blocked(d)) => d
            .iter()
            .map(|x| (x.rule.code.to_string(), x.message.clone()))
            .collect(),
        other => panic!("expected Blocked, got {other:?}"),
    }
}

/// ★ The DLL lands in the game root — the directory Windows searches first for a plugin's imports —
/// under its own name, recorded with its digest like every other game-folder file.
#[test]
fn add_runtime_dll_places_in_game_root_with_placement_record() {
    let dir = scratch("rtdll_ok");
    let dll = loadable_asi();
    let s = runtime_shipment(&dir, &[("src/test-shipment.dll", dll.clone())]);
    let report = build::build(&s, None, None, None, None, None).expect("add_runtime_dll must build");
    assert!(report.wad.is_none(), "a runtime DLL contributes nothing to a WAD");
    assert_eq!(report.placements.len(), 1);
    let p = &report.placements[0];
    assert_eq!(p.name, "test-shipment.dll");
    assert_eq!(
        p.destination,
        Destination::GameFolder {
            relative: "test-shipment.dll".into()
        },
        "the game root: no directory in the relative path"
    );
    let written = std::fs::read(dir.join("_build/test-shipment.dll")).expect("emitted");
    assert_eq!(written, dll);
    assert_eq!(p.sha256, build::sha256_hex(&written));
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/placement.json")).unwrap())
            .unwrap();
    assert_eq!(doc["placements"][0]["destination"]["kind"], "game_folder");
    assert_eq!(doc["placements"][0]["destination"]["relative"], "test-shipment.dll");
    assert!(report.log.join("\n").contains("UNRESTRICTED NATIVE CODE"));
}

#[test]
fn add_runtime_dll_refuses_non_dll() {
    let dir = scratch("rtdll_ext");
    let s = runtime_shipment(&dir, &[("src/test-shipment.asi", loadable_asi())]);
    let found = blocked_codes(build::build(&s, None, None, None, None, None));
    assert!(
        found.iter().any(|(c, m)| c == "M0162" && m.contains("not a `.dll`")),
        "{found:?}"
    );
}

/// A runtime DLL is named `<shipment.name>.dll`, so one runtime Shipment ships one DLL.
#[test]
fn add_runtime_dll_refuses_name_not_equal_to_shipment_name() {
    let dir = scratch("rtdll_name");
    let s = runtime_shipment(&dir, &[("src/m2-sdk.dll", loadable_asi())]);
    let found = blocked_codes(build::build(&s, None, None, None, None, None));
    assert!(
        found
            .iter()
            .any(|(c, m)| c == "M0162" && m.contains("`test-shipment.dll`")),
        "{found:?}"
    );
}

/// The name is compared lowercased, and the file keeps the spelling the author gave it.
#[test]
fn add_runtime_dll_accepts_name_differing_only_in_case() {
    let dir = scratch("rtdll_case");
    let s = runtime_shipment(&dir, &[("src/Test-Shipment.DLL", loadable_asi())]);
    let report = build::build(&s, None, None, None, None, None).expect("case differs only");
    assert_eq!(
        report.placements[0].destination,
        Destination::GameFolder {
            relative: "Test-Shipment.DLL".into()
        }
    );
}

/// A second `add_runtime_dll` in one Shipment would need the same name, and the Exclusive
/// FileArtifact claim refuses that within one Shipment (M0120).
#[test]
fn add_runtime_dll_refuses_second_runtime_dll_in_one_shipment() {
    let dir = scratch("rtdll_two");
    let s = runtime_shipment(
        &dir,
        &[
            ("src/a/test-shipment.dll", loadable_asi()),
            ("src/b/TEST-SHIPMENT.dll", loadable_asi()),
        ],
    );
    let found = blocked_codes(build::build(&s, None, None, None, None, None));
    assert!(
        found
            .iter()
            .any(|(c, m)| c == "M0120" && m.contains("file artifact test-shipment.dll")),
        "{found:?}"
    );
}

/// The deny list: the loader, its sidecar and the DLLs the loader reports, in any case.
#[test]
fn add_runtime_dll_refuses_deny_listed_dlls() {
    for file in ["pmc_bb.dll", "cruise.dll", "dxwrapper.dll", "binkw32.dll", "BinkW32.DLL"] {
        let dir = scratch("rtdll_deny");
        let path = format!("src/{file}");
        let s = runtime_shipment(&dir, &[(path.as_str(), loadable_asi())]);
        let found = blocked_codes(build::build(&s, None, None, None, None, None));
        assert!(
            found
                .iter()
                .any(|(c, m)| c == "M0162" && m.contains("is a DLL no Shipment may ship")),
            "{file}: {found:?}"
        );
    }
}

#[test]
fn add_runtime_dll_refuses_amd64() {
    let dir = scratch("rtdll_amd64");
    let s = runtime_shipment(&dir, &[("src/test-shipment.dll", fake_asi(0x8664, 0x230E))]);
    let found = blocked_codes(build::build(&s, None, None, None, None, None));
    assert!(
        found
            .iter()
            .any(|(c, m)| c == "M0178" && m.contains("32-bit process")),
        "{found:?}"
    );
}

/// `add_runtime_dll` is the one route for a DLL; `place_file` still refuses one.
#[test]
fn place_file_still_refuses_dll() {
    let dir = scratch("placefile_dll");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/test-shipment.dll"), loadable_asi()).unwrap();
    let s = shipment(
        &dir,
        "  - kind: place_file\n    file: src/test-shipment.dll\n    dest: game_root\n",
    );
    let found = blocked_codes(build::build(&s, None, None, None, None, None));
    assert!(
        found
            .iter()
            .any(|(c, m)| c == "M0162" && m.contains("add_runtime_dll")),
        "the refusal names the route that does exist: {found:?}"
    );
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0160"), "{d:?}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(
        !dir.join("_build")
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
    let report = build::build(&s, None, None, None, None, None).expect("build");
    assert_eq!(report.placements.len(), 2);
    assert_eq!(report.placements[0].destination, Destination::Overlay);
    assert!(matches!(
        report.placements[1].destination,
        Destination::GameFolder { .. }
    ));
    // Determinism covers the file half too: a placement record whose digests move between builds
    // cannot be verified at deploy.
    let again = build::build(&s, None, None, Some(&dir.join("second")), None, None).expect("second");
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

    let report = build::build(&s, None, None, None, None, None).expect("place_file must build");
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
    let written = std::fs::read(dir.join("_build/scripts/quiet_freeplay_vo.ini"))
        .expect("the companion must be emitted, mirroring the tree it is copied into");
    assert_eq!(written, ini, "the companion is copied verbatim");
    assert_eq!(p.sha256, build::sha256_hex(&written));
    assert_eq!(p.bytes, written.len());

    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/placement.json")).unwrap())
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
            build::build(&s, None, None, None, None, None).expect("every destination must build");
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
        assert!(dir.join("_build").join(relative).is_file(), "{relative}");
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
                "format: 2
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
        match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
        match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
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
    let report = build::build(&s, None, None, None, None, None).expect("build");
    let paths: Vec<String> = report
        .placements
        .iter()
        .map(|p| match &p.destination {
            Destination::GameFolder { relative }
            | Destination::DataWad { relative, .. }
            | Destination::LanguagePatch { relative, .. } => relative.clone(),
            Destination::StreamCopy { to, .. } => to.clone(),
            Destination::DataFile { relative, .. } => relative.relative().to_string(),
            Destination::Overlay => "overlay".into(),
            Destination::ShellPatch => "shell_patch".into(),
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
    let report = build::build(&s, None, None, None, None, None).expect("two rungs must build");
    assert_eq!(report.placements.len(), 2);
    assert_ne!(
        report.placements[0].sha256, report.placements[1].sha256,
        "each record must describe its own file"
    );
    assert_eq!(
        std::fs::read(dir.join("_build/scripts/OnBoot/init.lua")).unwrap(),
        b"-- boot\n"
    );
    assert_eq!(
        std::fs::read(dir.join("_build/scripts/OnLoad/init.lua")).unwrap(),
        b"-- load\n"
    );
}

// ---------------------------------------------------------------------------
// The emitted-artifact self-check
// ---------------------------------------------------------------------------

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

    let report = build::build(&s, None, None, None, None, None).expect("must build with no game");
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
    let report = build::build(&s, None, None, None, None, None).expect("a CFX movie must build");
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
    match build::build(&s, None, None, None, None, None) {
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
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Blocked(d)) => {
            assert!(d.iter().any(|x| x.rule.code == "M0120"), "{d:?}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

// ──────────────────────────────────────────────────────────────────────────────── add_sound

/// The two-cue `add_sound` Shipment the tests below build: a mono and a stereo WAV.
fn sound_shipment(dir: &Path) -> (discover::LoadedShipment, Vec<(String, u16, Vec<i16>)>) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let cues = vec![
        ("mod_click".to_string(), 1u16, (0..1001).map(|i| (i * 7) as i16).collect::<Vec<_>>()),
        ("mod_whoosh".to_string(), 2u16, (0..2000).map(|i| (i * -3) as i16).collect::<Vec<_>>()),
    ];
    let mut yaml =
        String::from("  - kind: add_sound\n    bank: mod_ui_sounds\n    category: ui\n    load_in: [gameplay]\n    cues:\n");
    for (i, (name, channels, samples)) in cues.iter().enumerate() {
        let file = format!("src/{name}.wav");
        std::fs::write(dir.join(&file), pcm16_wav(*channels, 22050, samples)).unwrap();
        yaml.push_str(&sound_cue_yaml(name, &file, 0x100 + i as u32, 0x200 + i as u32));
    }
    (shipment(dir, &yaml), cues)
}

/// ★ An `add_sound` bank, lowered and assembled into a patch WAD with no game present, reads back as
/// one block of three entries — soundbank, sounddb, wavebank, all under `m2(bank)`, each with its
/// primary ASET row — whose tables parse, and the audio engine resolves every cue by name to the
/// WAV's samples, with every authored field at its offset bit for bit.
#[test]
fn add_sound_lowers_to_one_block_the_engine_plays() {
    use mercs2_audio::soundbank::{CueBody, GroupForm, Soundbank};
    use mercs2_audio::sounddb::SoundDb;
    use mercs2_audio::AudioEngine;
    use mercs2_formats::hash::pandemic_hash_m2 as m2;
    use mercs2_formats::types::*;
    use mercs2_quartermaster::manifest::Contribution;

    let dir = scratch("add_sound_e2e");
    let (s, cues) = sound_shipment(&dir);
    let Contribution::AddSound { bank, category, cues: authored, .. } = &s.manifest.contributions[0] else {
        panic!("the fixture is an add_sound");
    };
    let mut log = Vec::new();
    let block = mercs2_quartermaster::sound::lower_add_sound(bank, category, authored, &s.root, &mut log)
        .expect("lowers without a game");
    let wad = mercs2_formats::patch_wad::build_patch_wad_multi(
        &[block],
        0,
        None,
        &mercs2_formats::patch_wad::FFCS_CERT_BLOB,
    )
    .expect("assembles");

    let contents = mercs2_formats::patch_wad::read_patch_wad(&wad).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];
    let hash = m2("mod_ui_sounds");
    let rows: Vec<(u32, u32, u32, u32)> =
        block.aset_entries.iter().map(|r| (r.asset_hash, r.u32_1, r.u32_2, r.u32_3)).collect();
    assert_eq!(
        rows,
        vec![
            (hash, 0xFFFF_FFFF, rows[0].2, TYPE_ID_SOUNDBANK),
            (hash, 0xFFFF_FFFF, rows[1].2, 13),
            (hash, 0xFFFF_FFFF, rows[2].2, TYPE_ID_WAVEBANK),
        ]
    );
    for r in &block.aset_entries {
        assert_eq!(r.u32_2 & 0xFFFF, 0xFFFF, "a bank has no LOD chain (M0001)");
    }
    assert_eq!(block.path_string, format!("blocks\\VZ\\mod_{hash:08x}.block"));

    let decompressed = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (parsed, issues) = mercs2_formats::ucfx::walk_decompressed_block(&decompressed, "add_sound");
    assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.detail).collect::<Vec<_>>());
    let types: Vec<(u32, u32)> = parsed.entries.iter().map(|e| (e.name_hash, e.type_hash)).collect();
    assert_eq!(
        types,
        vec![(hash, TYPE_HASH_SOUNDBANK), (hash, 0xE527_3C14), (hash, TYPE_HASH_WAVEBANK)]
    );
    let body = |i: usize| mercs2_formats::ucfx::extract_data_chunk(&parsed.containers[i]).expect("data leaf");
    let (sb, db, wb) = (body(0), body(1), body(2));
    let soundbank = Soundbank::parse(&sb).expect("soundbank parses");
    let sounddb = SoundDb::parse(&db).expect("sounddb parses");

    let mut eng = AudioEngine::default();
    eng.set_sounddb(sounddb.clone());
    eng.load_soundbank(&sb).expect("the engine loads the soundbank");
    eng.load_wavebank(&wb).expect("the engine loads the wavebank");
    for (i, (name, channels, samples)) in cues.iter().enumerate() {
        let entry = *eng.sounddb.find_cue_by_name(name).expect("the cue routes by name");
        let resolved = eng.resolve_cue(&entry).expect("the cue resolves");
        let waves: Vec<_> = resolved.waves().collect();
        assert_eq!(waves.len(), 1, "{name}: one single-wave group");
        let clip = eng.clip(waves[0].wavebank, waves[0].index).expect("the wave is resident");
        assert_eq!(clip.samples, *samples, "{name}: the WAV's samples");
        assert_eq!((clip.channels, clip.sample_rate), (*channels as u8, 22050));
        assert_eq!(clip.clip_hash, 0x200 + i as u32, "{name}: clip_hash");

        let cue = &soundbank.cues[entry.cue_index as usize];
        assert_eq!(cue.guid, m2(name));
        assert_eq!(cue.gain.to_bits(), 0x3F00_4DCE, "{name}: -6 dB");
        assert_eq!(cue.byte_06, 3, "{name}: start_limit");
        let CueBody::SingleTrack { group_index, unknown_16, .. } = cue.body else { panic!("single-track") };
        assert_eq!(unknown_16, 0x3E99, "{name}: cue_16");
        let g = &soundbank.groups[group_index as usize];
        assert_eq!(g.head.sound_id, 0x100 + i as u32, "{name}: sound_id");
        assert_eq!(g.head.category, m2("ui"));
        assert_eq!(g.head.unknown_10.to_bits(), 0.95f32.to_bits(), "{name}: priority");
        assert_eq!(g.head.unknown_14, 1, "{name}: positional");
        assert_eq!((g.head.min_distance, g.head.max_distance), (10.0, 1000.0));
        assert_eq!(g.head.unknown_20, 1.0, "{name}: group_20");
        assert_eq!((g.head.distance_exponent, g.head.doppler_scale), (2.0, 0.5));
        let GroupForm::Single { gain, unknown_30, wave } = &g.form else { panic!("single-wave") };
        assert_eq!(gain.to_bits(), 0x3F21_866C, "{name}: -4 dB");
        assert_eq!(*unknown_30, 1.5, "{name}: pitch_semitones");
        assert_eq!(wave.weight, 1.0);
        assert_eq!(cue.length_s, (samples.len() as f64 / f64::from(*channels) / 22050.0) as f32);
    }
    assert_eq!(sounddb.cues.len(), 2);
}

/// Two lowerings of one Shipment produce the same bytes, which verify-by-hash relies on.
#[test]
fn add_sound_is_reproducible() {
    use mercs2_quartermaster::manifest::Contribution;
    let dir = scratch("add_sound_repro");
    let (s, _) = sound_shipment(&dir);
    let Contribution::AddSound { bank, category, cues, .. } = &s.manifest.contributions[0] else {
        panic!("the fixture is an add_sound");
    };
    let lower = || {
        mercs2_quartermaster::sound::lower_add_sound(bank, category, cues, &s.root, &mut Vec::new())
            .expect("lowers")
            .compressed_data
    };
    assert_eq!(lower(), lower(), "two lowerings of one bank must be byte-identical");
}

/// The bank loads through the mod loader, which the build links into the scripts, so a build with
/// no game stack is refused with `GameRequired`.
#[test]
fn add_sound_needs_the_game_to_link_its_loader() {
    let dir = scratch("add_sound_nogame");
    let (s, _) = sound_shipment(&dir);
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::GameRequired { .. }) => {}
        other => panic!("expected GameRequired, got {other:?}"),
    }
}

/// A WAV the wavebank cannot embed blocks the build under M0214, before anything is lowered.
#[test]
fn an_unusable_wav_blocks_the_build() {
    let dir = scratch("add_sound_badwav");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut wav = pcm16_wav(1, 22050, &[1, 2, 3]);
    wav[34..36].copy_from_slice(&8u16.to_le_bytes()); // 8-bit
    std::fs::write(dir.join("src/a.wav"), wav).unwrap();
    let s = shipment(
        &dir,
        &format!(
            "  - kind: add_sound\n    bank: mod_bad\n    category: ui\n    load_in: [gameplay]\n    cues:\n{}",
            sound_cue_yaml("mod_bad_cue", "src/a.wav", 0, 0)
        ),
    );
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Blocked(d)) => assert!(d.iter().any(|x| x.rule.code == "M0214"), "{d:?}"),
        other => panic!("expected Blocked by M0214, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// add_animation / replace_animation — the retail clip container
// ---------------------------------------------------------------------------

/// A new clip needs nothing from retail, so it builds hermetically — into the container every
/// retail clip uses, with the three sources verbatim and `info` = `01 00`.
#[test]
fn add_animation_builds_the_retail_clip_container_without_a_game() {
    use mercs2_formats::anim_container::{build_evnt, AnimEvent};
    let dir = scratch("add_animation");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let trnm = anim_trnm(anim_tracks());
    let evnt = build_evnt(&[AnimEvent { time: 0.1, name: "ahj_foot_contact".into(), category: "sound".into() }])
        .unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), &trnm).unwrap();
    std::fs::write(dir.join("src/c.evnt"), &evnt).unwrap();
    let s = shipment(
        &dir,
        "  - kind: add_animation\n    name: qm_test_clip\n    clip: src/c.hkx\n    trnm: src/c.trnm\n    \
         events: src/c.evnt\n",
    );
    let report = build::build(&s, None, None, None, None, None).expect("add_animation builds with no game");
    let on_disk = std::fs::read(report.wad.expect("a WAD")).unwrap();
    let (_, hash, chunks) = read_back_animation(&on_disk);
    assert_eq!(hash, mercs2_formats::hash::pandemic_hash_m2("qm_test_clip"));
    let tags: Vec<&[u8; 4]> = chunks.iter().map(|c| &c.tag).collect();
    assert_eq!(tags, [b"info", b"data", b"trnm", b"evnt"]);
    assert_eq!(chunks[0].body, [0x01, 0x00]);
    assert_eq!(chunks[1].body, ANIM_CLIP);
    assert_eq!(chunks[2].body, trnm);
    assert_eq!(chunks[3].body, evnt);
}

/// Without `events` there is no `evnt` chunk — the other retail clip shape.
#[test]
fn add_animation_without_events_ships_no_evnt() {
    let dir = scratch("add_animation_plain");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), anim_trnm(anim_tracks())).unwrap();
    let s = shipment(&dir, "  - kind: add_animation\n    name: qm_test_clip2\n    clip: src/c.hkx\n    trnm: src/c.trnm\n");
    let report = build::build(&s, None, None, None, None, None).expect("builds");
    let (_, _, chunks) = read_back_animation(&std::fs::read(report.wad.unwrap()).unwrap());
    let tags: Vec<&[u8; 4]> = chunks.iter().map(|c| &c.tag).collect();
    assert_eq!(tags, [b"info", b"data", b"trnm"]);
}

/// A trnm that binds a different number of tracks than the clip has is refused before lowering —
/// M0213 blocks the build.
#[test]
fn add_animation_with_a_mismatched_trnm_is_blocked_by_m0213() {
    let dir = scratch("add_animation_bad");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), anim_trnm(anim_tracks() + 1)).unwrap();
    let s = shipment(&dir, "  - kind: add_animation\n    name: qm_bad\n    clip: src/c.hkx\n    trnm: src/c.trnm\n");
    match build::build(&s, None, None, None, None, None) {
        Err(BuildError::Blocked(d)) => assert!(d.iter().any(|x| x.rule.code == "M0213"), "{d:?}"),
        other => panic!("expected Blocked by M0213, got {other:?}"),
    }
}

/// A replace needs the game stack: it must check its target is a Havok clip.
#[test]
fn replace_animation_without_a_game_says_so() {
    let dir = scratch("replace_animation_nogame");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/c.hkx"), ANIM_CLIP).unwrap();
    std::fs::write(dir.join("src/c.trnm"), anim_trnm(anim_tracks())).unwrap();
    let s = shipment(&dir, "  - kind: replace_animation\n    target: x\n    clip: src/c.hkx\n    trnm: src/c.trnm\n");
    assert!(matches!(
        build::build(&s, None, None, None, None, None),
        Err(BuildError::GameRequired { kind: "replace_animation", .. })
    ));
}
