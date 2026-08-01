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

use std::path::{Path, PathBuf};

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
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
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        return;
    };
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
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
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
