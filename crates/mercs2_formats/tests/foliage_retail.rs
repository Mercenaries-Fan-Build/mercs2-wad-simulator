//! Retail gates for [`mercs2_formats::foliage`]: the `foliage` chunk of `vz.wad`.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::foliage::{Foliage, TYPE_HASH};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::block_chunk_spans;

const RESIDENT_BLOCK: &str = "blocks\\VZ\\resident_P000_Q3.block";

fn retail() -> Foliage {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
    let index = archive
        .paths
        .iter()
        .position(|p| p == RESIDENT_BLOCK)
        .expect("the resident block");
    let block = decompress_block(&mut f, &archive.indx, index as u16).expect("decompress the resident block");
    let spans = block_chunk_spans(&block).expect("the block's chunks");
    let (_, range) = spans
        .iter()
        .find(|(e, _)| e.type_hash == TYPE_HASH)
        .expect("a foliage chunk");
    Foliage::decode(&block[range.clone()]).expect("decode foliage")
}

#[test]
fn every_species_reads_to_the_end_of_the_chunk() {
    let foliage = retail();
    assert_eq!(foliage.species.len(), 30);
    let palm = foliage
        .species
        .iter()
        .find(|s| s.template == 0x8000_4DAC)
        .expect("_global_env_palmtree01");
    assert_eq!(palm.range(), 150.0);
    for s in &foliage.species {
        assert!(s.tree.bit_count.div_ceil(8) <= s.tree.bits.len() as u32);
        assert!((0..3).all(|k| s.tree.root_min[k] <= s.tree.root_max[k]));
    }
}

/// Every species' billboard has a quad or more, its corners inside `FUN_004a24a0`'s rectangle and its
/// texture coordinates inside the atlas page. Retail authors them a texel or so past a tile's edge, -0.00035
/// on `_global_env_palmtree01`'s neighbours, which the bound allows.
#[test]
fn every_billboard_has_quads_inside_a_tile() {
    const BLEED: f32 = 0.01;
    for s in &retail().species {
        let views = s.views();
        assert!((1..=3).contains(&views.len()), "0x{:08X}: {} views", s.template, views.len());
        for v in views {
            assert!(v.corners.iter().flatten().all(|c| c.is_finite() && c.abs() < 200.0), "{v:?}");
            assert!(v.uvs.iter().flatten().all(|c| (-BLEED..=1.0 + BLEED).contains(c)), "{v:?}");
        }
        assert_eq!(s.billboard_material().preamble.len(), 26);
    }
}

/// Every tree of every species decodes, the stream holds exactly the bits its counts size, and every tree
/// lies in its species' root box, inside the world.
#[test]
fn every_species_decodes_its_trees_inside_the_world() {
    let foliage = retail();
    let mut total = 0;
    for s in &foliage.species {
        let leaves = s.tree.leaves().unwrap_or_else(|e| panic!("0x{:08X}: {e}", s.template));
        total += leaves.len();
        for l in &leaves {
            for k in 0..3 {
                assert!(
                    (s.tree.root_min[k] - 1.0..=s.tree.root_max[k] + 1.0).contains(&l.position[k]),
                    "0x{:08X}: {:?} outside its box",
                    s.template,
                    l.position
                );
            }
        }
    }
    // Retail's batches hold 37,023 at `+0x00` in every capture, each its leaf count plus 1.
    assert_eq!(total, 37_023 - 30);
}
