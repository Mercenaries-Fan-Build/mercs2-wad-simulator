//! Every placed entity key of retail `vz.wad`, read layer by layer.
//!
//! Two facts the quartermaster's M0198 rests on, pinned against the shipped bytes:
//!
//! 1. A `Name` COMP record is `[u32 key][string][NUL][u8 flag]`, the flag `0x00` or `0x01`, and
//!    that layout consumes every `Name` COMP of every layer exactly. `entity_key_set` reads the
//!    same keys a strict read of that layout does.
//! 2. Retail places every key once: no key is in two layers, and none twice in one COMP.
//!
//! A layer is an entry of type `0xE6B81A54` in a block a `type_id 9` ASET row names.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement::{comp_inventory, entity_key_set, TRANSFORM_STRIDE};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_LAYER, TYPE_ID_LAYER};
use mercs2_formats::ucfx::parse_block_entry_table;

fn rd(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Every layer entry of the archive: `(block, name hash, container bytes)`.
fn layers() -> Vec<(u16, u32, Vec<u8>)> {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut file = std::fs::File::open(&wad).unwrap_or_else(|e| panic!("open {}: {e}", wad.display()));
    let size = file.metadata().unwrap().len();
    let archive = load_ffcs_archive(&mut file, size).unwrap_or_else(|e| panic!("read {}: {e:?}", wad.display()));
    let blocks: BTreeSet<u16> =
        archive.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER).map(|e| e.block_index()).collect();
    let mut out = Vec::new();
    for b in blocks {
        let dec = decompress_block(&mut file, &archive.indx, b).unwrap_or_else(|e| panic!("block {b}: {e:?}"));
        let (count, entries) = parse_block_entry_table(&dec);
        let mut at = 4 + count as usize * 16;
        for e in &entries {
            let end = at + e.chunk_size as usize;
            if e.type_hash == TYPE_HASH_LAYER {
                out.push((b, e.name_hash, dec[at..end].to_vec()));
            }
            at = end;
        }
    }
    out
}

/// A strict read of a `Name` COMP: its keys, or why the bytes do not tile as
/// `[u32 key][string][NUL][u8 flag]` with a flag of `0x00` or `0x01`.
fn strict_name_keys(blob: &[u8]) -> Result<Vec<u32>, String> {
    let mut keys = Vec::new();
    let mut p = 0;
    while p < blob.len() {
        if p + 4 > blob.len() {
            return Err(format!("{} bytes left over at +0x{p:X}", blob.len() - p));
        }
        let key = rd(blob, p);
        p += 4;
        let nul = blob[p..].iter().position(|&b| b == 0).ok_or(format!("no NUL after key 0x{key:08X}"))?;
        p += nul + 1;
        match blob.get(p) {
            Some(0x00 | 0x01) => p += 1,
            Some(f) => return Err(format!("key 0x{key:08X} has flag 0x{f:02X}")),
            None => return Err(format!("key 0x{key:08X} has no flag byte")),
        }
        keys.push(key);
    }
    Ok(keys)
}

#[test]
fn every_retail_entity_key_is_placed_once() {
    let layers = layers();
    let mut layers_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut repeated_in_a_comp = Vec::new();
    for (block, name, container) in &layers {
        let mut keys: HashSet<u32> = HashSet::new();
        for c in comp_inventory(container) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            let data = &container[off..off + size];
            let comp_keys: Vec<u32> = match c.info_name.as_deref() {
                Some("Transform") => (0..size / TRANSFORM_STRIDE).map(|i| rd(data, i * TRANSFORM_STRIDE)).collect(),
                Some("ModelName") => (0..size / 8).map(|i| rd(data, i * 8)).collect(),
                Some("Name") => strict_name_keys(data)
                    .unwrap_or_else(|e| panic!("layer 0x{name:08X} (block {block}) Name COMP: {e}")),
                _ => continue,
            };
            let mut seen = BTreeSet::new();
            for k in &comp_keys {
                if !seen.insert(*k) {
                    repeated_in_a_comp.push((*name, *k));
                }
            }
            keys.extend(comp_keys);
        }
        assert_eq!(
            entity_key_set(container),
            keys,
            "layer 0x{name:08X} (block {block}): entity_key_set disagrees with a strict read"
        );
        for k in keys {
            layers_of.entry(k).or_default().push(*name);
        }
    }
    let shared: Vec<(&u32, &Vec<u32>)> = layers_of.iter().filter(|(_, l)| l.len() > 1).collect();
    eprintln!("{} layers, {} distinct entity keys", layers.len(), layers_of.len());
    assert_eq!(layers.len(), 923, "layer entries in vz.wad");
    assert_eq!(layers_of.len(), 101_299, "distinct entity keys in vz.wad's layers");
    assert!(shared.is_empty(), "keys in more than one layer: {:?}", &shared[..shared.len().min(5)]);
    assert!(repeated_in_a_comp.is_empty(), "keys twice in one COMP: {:?}", &repeated_in_a_comp[..repeated_in_a_comp.len().min(5)]);
}
