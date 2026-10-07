//! The ASET `type_id` tables must agree with the WAD's OWN table.
//!
//! `type_id` is the byte the engine indexes its loader table with, so a wrong one dispatches an
//! asset to the wrong loader. Two tables in this crate claim to know the mapping —
//! `types::TYPE_ID_*` and `aset_type_ids::type_id_for_type_hash` — and both were hand-maintained
//! against `docs/type_hash_registry.md`, which was itself derived rather than read.
//!
//! It had drifted. Measured 2026-08-01 against retail `vz.wad`: **12 of 35 rows in
//! `aset_type_ids` and 7 of 23 paired constants in `types` were wrong** — `fxdict` and `watermap`
//! were transposed, `level` said 20 for 26, `musicstatemap` 26 for 8, `worldentity` 8 for 17. This
//! reproduces a finding `docs/fixpack/wad_duplicate_inventory.md` Appendix C recorded in July
//! ("wrong for 12 of 36 type ids … validated 139 hit / 0 miss"), which had never been applied to
//! the code.
//!
//! The authoritative table is **inside every WAD at file offset `0x48`**, immediately after the
//! 0x48-byte FFCS header: a flat array of `type_hash` u32s indexed by `type_id`, whose length is
//! header dword 8 (36 in retail). It is identical across all four shipped WADs.
//!
//! So this test does not encode a table of its own — it reads the game's. A hand-kept mapping that
//! nothing checks is a mapping that drifts, which is precisely how the last one did.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::path::{Path, PathBuf};

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// The WAD's own `type_id -> type_hash` table.
fn wad_type_table(wad: &Path) -> Vec<u32> {
    let head = {
        use std::io::Read;
        let mut f = std::fs::File::open(wad).expect("open wad");
        let mut b = vec![0u8; 0x48 + 36 * 4 + 16];
        f.read_exact(&mut b).expect("read header");
        b
    };
    // Header dword 8 is the entry count — the `DATA` chunk-row's `meta` word.
    let count = u32_at(&head, 8 * 4) as usize;
    assert!(
        (1..=256).contains(&count),
        "implausible type-table count {count}; the header layout must have changed"
    );
    (0..count).map(|i| u32_at(&head, 0x48 + i * 4)).collect()
}

#[test]
fn every_type_id_constant_matches_the_wads_own_table() {
    let wad = vz_wad();
    let table = wad_type_table(&wad);
    let id_of = |h: u32| table.iter().position(|t| *t == h);

    use mercs2_formats::types::*;
    // Every pair this crate publishes. Listed explicitly rather than scraped, so adding a constant
    // without adding it here is the one failure mode left — and that one is visible in review.
    let pairs: &[(&str, u32, u32)] = &[
        ("WAVEBANK", TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK),
        ("SOUNDBANK", TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK),
        ("LAYER", TYPE_ID_LAYER, TYPE_HASH_LAYER),
        ("MODEL", TYPE_ID_MODEL, TYPE_HASH_MODEL),
        ("TEXTURE", TYPE_ID_TEXTURE, TYPE_HASH_TEXTURE),
        ("SCRIPT", TYPE_ID_SCRIPT, TYPE_HASH_SCRIPT),
        ("ANIMATION", TYPE_ID_ANIMATION, TYPE_HASH_ANIMATION),
        ("LOWRES_TERRAIN", TYPE_ID_LOWRES_TERRAIN, TYPE_HASH_LOWRES_TERRAIN),
        ("TERRAIN_MESH", TYPE_ID_TERRAIN_MESH, TYPE_HASH_TERRAIN_MESH),
        ("FONT", TYPE_ID_FONT, TYPE_HASH_FONT),
        ("PATH", TYPE_ID_PATH, TYPE_HASH_PATH),
        ("EFFECT", TYPE_ID_EFFECT, TYPE_HASH_EFFECT),
        ("STRINGDB", TYPE_ID_STRINGDB, TYPE_HASH_STRINGDB),
        ("LEVEL", TYPE_ID_LEVEL, TYPE_HASH_LEVEL),
        ("STANCE", TYPE_ID_STANCE, TYPE_HASH_STANCE),
        ("MATERIAL_PARAMS", TYPE_ID_MATERIAL_PARAMS, TYPE_HASH_MATERIAL_PARAMS),
        ("MUSIC_STATE_MAP", TYPE_ID_MUSIC_STATE_MAP, TYPE_HASH_MUSIC_STATE_MAP),
        ("MUSIC_CUE_TABLE", TYPE_ID_MUSIC_CUE_TABLE, TYPE_HASH_MUSIC_CUE_TABLE),
        ("ANIM_STATE_MACHINE", TYPE_ID_ANIM_STATE_MACHINE, TYPE_HASH_ANIM_STATE_MACHINE),
        ("WORLD_ENTITY_DATA", TYPE_ID_WORLD_ENTITY_DATA, TYPE_HASH_WORLD_ENTITY_DATA),
        ("FX_DICTIONARY", TYPE_ID_FX_DICTIONARY, TYPE_HASH_FX_DICTIONARY),
        ("CFX_PACK", TYPE_ID_CFX_PACK, TYPE_HASH_CFX_PACK),
        ("WATERMAP", TYPE_ID_WATERMAP, TYPE_HASH_WATERMAP),
    ];

    let mut wrong = Vec::new();
    for (name, id, hash) in pairs {
        match id_of(*hash) {
            Some(real) if real as u32 == *id => {}
            Some(real) => wrong.push(format!("TYPE_ID_{name}: constant {id}, WAD says {real}")),
            None => wrong.push(format!(
                "TYPE_HASH_{name} (0x{hash:08X}) is not in the WAD's type table at all"
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "type-id constants disagree with the WAD's own table at 0x48:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn the_type_hash_to_id_map_matches_the_wads_own_table() {
    let wad = vz_wad();
    let table = wad_type_table(&wad);

    // Forward: every entry in the WAD's table must map back to its own index.
    let mut wrong = Vec::new();
    for (id, hash) in table.iter().enumerate() {
        match mercs2_formats::aset_type_ids::type_id_for_type_hash(*hash) {
            Some(got) if got as usize == id => {}
            Some(got) => wrong.push(format!("0x{hash:08X}: map says {got}, WAD says {id}")),
            None => wrong.push(format!("0x{hash:08X} (WAD id {id}) is missing from the map")),
        }
    }
    assert!(
        wrong.is_empty(),
        "type_id_for_type_hash disagrees with the WAD's own table:\n  {}",
        wrong.join("\n  ")
    );
}

/// `types::TYPE_HASH_REGISTRY` is the table `type_hash_for_type_id` / `type_id_for_type_hash`
/// answer from, so it must BE the WAD's table: every pair, no extras.
#[test]
fn the_type_hash_registry_is_the_wads_own_table() {
    let wad = vz_wad();
    let table = wad_type_table(&wad);
    let mut registry = mercs2_formats::types::TYPE_HASH_REGISTRY.to_vec();
    registry.sort_by_key(|&(_, id)| id);
    let wad_pairs: Vec<(u32, u32)> = table.iter().enumerate().map(|(id, h)| (*h, id as u32)).collect();
    assert_eq!(registry, wad_pairs, "TYPE_HASH_REGISTRY != the WAD's type table at 0x48");
}

/// The fxdict type id, from both directions: the WAD's type table puts `0xFA46D8A8` at 0, and every
/// ASET row with type id 0 names an fxdict container in its block — and no fxdict container is
/// registered under any other type id.
#[test]
fn every_fxdict_aset_row_uses_type_id_0() {
    use mercs2_formats::types::{TYPE_HASH_FX_DICTIONARY, TYPE_ID_FX_DICTIONARY};
    let wad = vz_wad();
    let table = wad_type_table(&wad);
    assert_eq!(table[TYPE_ID_FX_DICTIONARY as usize], TYPE_HASH_FX_DICTIONARY);
    assert_eq!(mercs2_formats::types::type_id_for_type_hash(TYPE_HASH_FX_DICTIONARY), Some(0));
    assert_eq!(mercs2_formats::aset_type_ids::type_id_for_type_hash(TYPE_HASH_FX_DICTIONARY), Some(0));

    let mut f = std::fs::File::open(&wad).expect("open wad");
    let size = f.metadata().expect("stat").len();
    let archive = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).expect("read FFCS");
    let rows: Vec<_> = archive.aset.iter().filter(|a| a.type_id == TYPE_ID_FX_DICTIONARY).collect();
    assert!(!rows.is_empty(), "no ASET row carries the fxdict type id");

    // Each row's block holds its fxdict container, and every fxdict container there is registered
    // under type id 0.
    let row_blocks: Vec<u16> = rows.iter().flat_map(|a| a.lod_chain()).collect();
    for row in &rows {
        let bi = row.block_index();
        let dec = mercs2_formats::sges::decompress_block(&mut f, &archive.indx, bi).expect("block");
        let (_n, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        assert!(
            entries.iter().any(|e| e.name_hash == row.asset_hash && e.type_hash == TYPE_HASH_FX_DICTIONARY),
            "ASET row 0x{:08X} (type id 0) has no fxdict container in block {bi}",
            row.asset_hash
        );
        for e in entries.iter().filter(|e| e.type_hash == TYPE_HASH_FX_DICTIONARY) {
            let ids: Vec<u32> = archive
                .aset
                .iter()
                .filter(|a| a.asset_hash == e.name_hash && a.lod_chain().contains(&bi))
                .map(|a| a.type_id)
                .collect();
            assert!(
                ids.contains(&TYPE_ID_FX_DICTIONARY),
                "fxdict container 0x{:08X} in block {bi} is registered under type ids {ids:?}, not 0",
                e.name_hash
            );
        }
    }
    eprintln!("fxdict ASET rows with type id 0: {} (blocks {row_blocks:?})", rows.len());
}
