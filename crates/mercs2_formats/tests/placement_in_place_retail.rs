//! Retail gate for [`mercs2_formats::placement_build::insert_entity`] / `remove_entity`: each of
//! retail's 1,208 `TinyGeometryObject` placements, taken out of its layer and put back, gives back the
//! layer's own bytes but for the placement's `Transform` tail, which the writer leaves zero.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement_build::{
    insert_entity, read_layer_records, remove_entity, NewEntity, FLGS_TINY_GEOMETRY_OBJECT,
};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_LAYER;
use mercs2_formats::ucfx::{parse_ucfx_tree, walk_decompressed_block, write_ucfx_tree};

/// `container` with the `Transform` tail of entity `key` set to zero.
fn zero_tail(container: &[u8], key: u32) -> Vec<u8> {
    let mut roots = parse_ucfx_tree(container).expect("a layer tree");
    for comp in roots.iter_mut().filter(|n| &n.tag == b"COMP") {
        let is_transform = comp.children[0].body.as_deref().is_some_and(|b| b.starts_with(b"Transform\0"));
        if !is_transform {
            continue;
        }
        let data = comp.children.iter_mut().find(|n| &n.tag == b"data").unwrap().body.as_mut().unwrap();
        for rec in data.chunks_exact_mut(42) {
            if u32::from_le_bytes(rec[0..4].try_into().unwrap()) == key {
                rec[36..42].fill(0);
            }
        }
    }
    write_ucfx_tree(&roots)
}

#[test]
fn every_retail_tiny_geometry_object_comes_out_and_goes_back() {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");

    let (mut placements, mut layers) = (0usize, 0usize);
    let mut failures = Vec::new();
    for block in 0..archive.indx.len() {
        let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
        let (parsed, _) = walk_decompressed_block(&dec, "block");
        for (entry, c) in parsed.entries.iter().zip(parsed.containers.iter()) {
            if entry.type_hash != TYPE_HASH_LAYER {
                continue;
            }
            let Ok(records) = read_layer_records(c) else { continue };
            // A TinyGeometryObject placement: named `tinygeometry_tgr<row>_tgc<col> 0x<key>`, state
            // FLGS_TINY_GEOMETRY_OBJECT.
            let tgo: Vec<u32> = records
                .flags
                .iter()
                .filter(|(k, p)| {
                    *p == FLGS_TINY_GEOMETRY_OBJECT
                        && records.names.iter().any(|(nk, n)| nk == k && n.starts_with("tinygeometry_tgr"))
                })
                .map(|(k, _)| *k)
                .collect();
            if tgo.is_empty() {
                continue;
            }
            layers += 1;
            for key in tgo {
                placements += 1;
                let at = format!("block {block} layer {:#010X} entity {key:#010X}", entry.name_hash);
                let name = records.names.iter().find(|n| n.0 == key).map(|n| n.1.clone());
                let model = records.models.iter().find(|m| m.0 == key).map(|m| m.1);
                let tf = records.transforms.iter().find(|t| t.0 == key).copied();
                let (Some(name), Some(model), Some(tf)) = (name, model, tf) else {
                    failures.push(format!("{at}: lacks a Name, ModelName or Transform record"));
                    continue;
                };
                let suffix = format!(" 0x{key:08x}");
                let Some(stem) = name.strip_suffix(&suffix) else {
                    failures.push(format!("{at}: Name {name:?} does not end in {suffix:?}"));
                    continue;
                };
                let base = match remove_entity(c, key) {
                    Ok(b) => b,
                    Err(e) => {
                        failures.push(format!("{at}: remove: {e}"));
                        continue;
                    }
                };
                let e = NewEntity { key, model_hash: model, pos: tf.1, quat: tf.2, name: stem.to_string() };
                match insert_entity(&base, &e, FLGS_TINY_GEOMETRY_OBJECT) {
                    Ok(back) if back == zero_tail(c, key) => {}
                    Ok(_) => failures.push(format!("{at}: put back, the layer differs beyond the Transform tail")),
                    Err(e) => failures.push(format!("{at}: insert: {e}")),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(placements, 1208);
    eprintln!("{placements} TinyGeometryObject placements in {layers} layers");
}
