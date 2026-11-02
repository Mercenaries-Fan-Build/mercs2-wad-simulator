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
