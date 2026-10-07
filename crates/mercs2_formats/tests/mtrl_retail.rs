//! Retail gate for [`mercs2_formats::texture::parse_mtrl`]: every `MTRL` leaf in `vz.wad` parses
//! with its loader's count source and fills its leaf exactly.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::collections::BTreeMap;
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::types::{
    TYPE_HASH_FONT, TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_MODEL, TYPE_HASH_TERRAIN_MESH,
};
use mercs2_formats::ucfx::{read_ucfx_rows, walk_decompressed_block};

/// The `scrub` container type (ASET type id 12).
const TYPE_HASH_SCRUB: u32 = 0x600B_904E;

fn source_for(type_hash: u32) -> Option<MtrlSource> {
    match type_hash {
        TYPE_HASH_MODEL => Some(MtrlSource::Model),
        TYPE_HASH_TERRAIN_MESH => Some(MtrlSource::TerrainMesh),
        TYPE_HASH_FONT => Some(MtrlSource::Font),
        TYPE_HASH_LOWRES_TERRAIN => Some(MtrlSource::LowResTerrain),
        TYPE_HASH_SCRUB => Some(MtrlSource::Scrub),
        _ => None,
    }
}

#[test]
fn every_retail_mtrl_parses_with_its_loaders_count() {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");

    let mut containers: BTreeMap<String, usize> = BTreeMap::new();
    let mut materials: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for block in 0..archive.indx.len() {
        let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
        let (parsed, _) = walk_decompressed_block(&dec, "block");
        for (entry, c) in parsed.entries.iter().zip(parsed.containers.iter()) {
            let Ok(rows) = read_ucfx_rows(c) else { continue };
            if !rows.iter().any(|r| &r.tag == b"MTRL") {
                continue;
            }
            let Some(source) = source_for(entry.type_hash) else {
                failures.push(format!(
                    "block {block}: container {:#010X} of type {:#010X} has an MTRL and no known loader",
                    entry.name_hash, entry.type_hash
                ));
                continue;
            };
            match parse_mtrl(c, source) {
                Ok(m) => {
                    *containers.entry(format!("{source:?}")).or_default() += 1;
                    *materials.entry(format!("{source:?}")).or_default() += m.len();
                }
                Err(e) => failures.push(format!(
                    "block {block}: container {:#010X} ({source:?}): {e}",
                    entry.name_hash
                )),
            }
        }
    }
    eprintln!("MTRL containers by loader: {containers:?}");
    eprintln!("materials by loader: {materials:?}");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(containers.get("Model"), Some(&3007));
    assert_eq!(containers.get("TerrainMesh"), Some(&400));
    assert_eq!(containers.get("Scrub"), Some(&1020));
    assert_eq!(materials.get("Scrub"), Some(&4000));
    assert_eq!(containers.get("LowResTerrain"), Some(&400));
    assert_eq!(containers.get("Font"), Some(&9));
}
