//! Every retail effect, re-encoded byte for byte from the typed model.
//!
//! `mercs2_formats::fxdict` parses an effect (type `0x5608BD5A`) into [`EffectContainer`] and writes
//! it back with a COMPUTED `EFCT` header, computed `x2`/`x3` descriptor words and no stored
//! padding. These tests hold that against the whole retail population in `vz.wad`:
//!
//! * all 314 effects parse and re-encode to the identical bytes (tree, bodies, CSUM);
//! * the computed `EFCT` equals the stored one in all 314;
//! * the resident fxdict container re-encodes identically too;
//! * the C4 explosion's effect asset resolves by name hash.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};
use mercs2_formats::fxdict::{
    parse_effect_container, parse_fxdict_container, write_effect_container, write_fxdict_container,
    EFCT_BYTES,
};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::schema::parse_comp_groups;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{
    TYPE_HASH_EFFECT, TYPE_HASH_FX_DICTIONARY, TYPE_HASH_WORLD_ENTITY_DATA, TYPE_ID_EFFECT,
};
use mercs2_formats::ucfx::parse_block_entry_table;

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn open() -> (File, FfcsArchive) {
    let wad = vz_wad();
    let mut f = File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");
    (f, archive)
}

/// Every container of `type_hash` in the given blocks, by name hash. A name that appears twice
/// resolves to the LAST entry, as the engine's asset map does.
fn containers(f: &mut File, archive: &FfcsArchive, blocks: &[u16], type_hash: u32) -> BTreeMap<u32, Vec<u8>> {
    let mut out = BTreeMap::new();
    for &bi in blocks {
        let dec = decompress_block(f, &archive.indx, bi).unwrap_or_else(|e| panic!("block {bi}: {e}"));
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            assert!(end <= dec.len(), "block {bi}: entry 0x{:08X} overruns the block", e.name_hash);
            if e.type_hash == type_hash {
                out.insert(e.name_hash, dec[pos..end].to_vec());
            }
            pos = end;
        }
    }
    out
}

/// The blocks an ASET selection lives in (all LOD rungs).
fn blocks_of(archive: &FfcsArchive, pick: impl Fn(&mercs2_formats::ffcs::AsetEntry) -> bool) -> Vec<u16> {
    let mut b: Vec<u16> = archive
        .aset
        .iter()
        .filter(|a| pick(a))
        .flat_map(|a| a.lod_chain())
        .filter(|&b| b != 0xFFFF)
        .collect();
    b.sort_unstable();
    b.dedup();
    b
}

fn retail_effects(f: &mut File, archive: &FfcsArchive) -> BTreeMap<u32, Vec<u8>> {
    let names: Vec<u32> =
        archive.aset.iter().filter(|a| a.type_id == TYPE_ID_EFFECT).map(|a| a.asset_hash).collect();
    let blocks = blocks_of(archive, |a| a.type_id == TYPE_ID_EFFECT);
    let fx = containers(f, archive, &blocks, TYPE_HASH_EFFECT);
    let mut want = names.clone();
    want.sort_unstable();
    want.dedup();
    assert_eq!(fx.keys().copied().collect::<Vec<_>>(), want, "effect containers != effect ASET rows");
    fx
}

#[test]
fn every_retail_effect_reencodes_byte_identically_with_a_computed_efct() {
    let (mut f, archive) = open();
    let fx = retail_effects(&mut f, &archive);
    assert_eq!(fx.len(), 314, "retail vz.wad ships 314 effects");

    let mut identical = 0usize;
    let mut efct_equal = 0usize;
    let mut failures = Vec::new();
    for (name, bytes) in &fx {
        let effect = match parse_effect_container(bytes) {
            Ok(e) => e,
            Err(e) => {
                failures.push(format!("0x{name:08X}: parse: {e}"));
                continue;
            }
        };
        // The stored EFCT body is the first `EFCT_BYTES` of the data area.
        let rows = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let data = 20 + 20 * rows;
        let stored: Vec<u16> =
            bytes[data..data + EFCT_BYTES].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        match effect.efct_words() {
            Ok(w) if w.as_slice() == stored.as_slice() => efct_equal += 1,
            Ok(w) => failures.push(format!("0x{name:08X}: EFCT stored {stored:?} computed {w:?}")),
            Err(e) => failures.push(format!("0x{name:08X}: EFCT: {e}")),
        }
        match write_effect_container(&effect) {
            Ok(out) if out == *bytes => identical += 1,
            Ok(out) => {
                let at = out.iter().zip(bytes.iter()).position(|(a, b)| a != b).unwrap_or(out.len().min(bytes.len()));
                failures.push(format!(
                    "0x{name:08X}: re-encode differs at byte {at} ({} vs {} bytes)",
                    out.len(),
                    bytes.len()
                ));
            }
            Err(e) => failures.push(format!("0x{name:08X}: write: {e}")),
        }
    }
    eprintln!("effects: {identical}/{} byte-identical, EFCT computed = stored {efct_equal}/{}", fx.len(), fx.len());
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(identical, 314);
    assert_eq!(efct_equal, 314);
}

#[test]
fn the_retail_fxdict_container_reencodes_byte_identically() {
    let (mut f, archive) = open();
    let fx = pandemic_hash_m2("fx");
    let blocks = blocks_of(&archive, |a| a.asset_hash == fx);
    let found = containers(&mut f, &archive, &blocks, TYPE_HASH_FX_DICTIONARY);
    let bytes = found.get(&fx).expect("fxdict container 0x86BF6C5B");
    let params = parse_fxdict_container(bytes).expect("parse fxdict");
    assert_eq!(params.len(), 630);
    assert_eq!(write_fxdict_container(&params), *bytes);
}

/// The C4 explosion is spawned from Lua as the template `global_particle_explosion_c4`
/// (`Pg.Spawn` in the decompiled `vz/pmccon002.lua`). Which of the 314 effect assets is it?
///
/// Result: `global_explosion_c4` = `0x41B4326E` is an effect asset; none of the other spellings is.
/// That is the same `particle_`-dropping rule the god-ray placements follow
/// (`global_particle_env_godray2` → `global_env_godray2`). The template's world-entity record says
/// the same: in the worldentity container the `RedEffectComponent` record of entity `0x80008028`
/// (the handle the retail template registry gives `global_particle_explosion_c4`, see
/// `docs/data/spawnable_templates.csv`) carries `0x41B4326E` in its `name` (`0x1DE5C824`) field.
#[test]
fn the_c4_explosion_effect_resolves_by_name_hash() {
    let (mut f, archive) = open();
    let fx = retail_effects(&mut f, &archive);

    let candidates = [
        "global_particle_explosion_c4",
        "global_explosion_c4",
        "global_c4_explosion",
        "global_particle_c4_explosion",
        "explosion_c4",
        "c4_explosion",
    ];
    let hits: Vec<(&str, u32)> = candidates
        .iter()
        .map(|c| (*c, pandemic_hash_m2(c)))
        .filter(|(_, h)| fx.contains_key(h))
        .collect();
    eprintln!("C4 candidates that are effect assets: {hits:08X?}");
    assert_eq!(hits, vec![("global_explosion_c4", 0x41B4326E)]);
    assert_eq!(
        pandemic_hash_m2(&"global_particle_explosion_c4".replace("particle_", "")),
        0x41B4326E
    );
    parse_effect_container(&fx[&0x41B4326E]).expect("the C4 effect parses");

    // The world-entity side: RedEffectComponent { entity 0x80008028, name 0x41B4326E }.
    let we_blocks = blocks_of(&archive, |a| a.asset_hash == 0x50075B3B);
    let we = containers(&mut f, &archive, &we_blocks, TYPE_HASH_WORLD_ENTITY_DATA);
    let container = we.get(&0x50075B3B).expect("worldentity container 0x50075B3B");
    let groups = parse_comp_groups(container);
    let red = groups
        .iter()
        .find(|g| g.name.as_deref() == Some("RedEffectComponent"))
        .expect("RedEffectComponent COMP");
    let data = red.data.as_ref().expect("RedEffectComponent data");
    let words: Vec<u32> = data.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    assert!(
        words.windows(2).any(|w| w == [0x8000_8028, 0x41B4_326E]),
        "no RedEffectComponent record pairs entity 0x80008028 with effect 0x41B4326E"
    );
}
