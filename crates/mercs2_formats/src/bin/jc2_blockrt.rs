//! jc2_blockrt — block-3185 round-trip check: does [count][16B entries][bodies] reproduce the whole
//! decompressed block, or is there trailing/padding data the rebuild drops?
use std::fs::File;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::sges::decompress_block;
fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into());
    let mut f = File::open(&path).unwrap();
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).unwrap();
    let mut blk = None;
    for bi in 0..arch.indx.len() {
        let Ok(d) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
        if d.len() < 4 { continue; }
        let c = read_u32_le(&d, 0) as usize;
        if c==0 || c>100_000 { continue; }
        if (0..c).any(|e| 4+e*16+16<=d.len() && read_u32_le(&d,4+e*16+4)==0x5647_C35D) { blk=Some(d); break; }
    }
    let blk = blk.unwrap();
    let count = read_u32_le(&blk,0) as usize;
    let mut expected = 4 + count*16;
    for ei in 0..count { expected += read_u32_le(&blk, 4+ei*16+12) as usize; }
    println!("block len {} | header+entries+bodies = {} | trailing = {}", blk.len(), expected, blk.len() as i64 - expected as i64);
    // Write the UNMODIFIED decompressed block for a null-override diagnostic patch.
    std::fs::write("../../output/block3185_original.bin", &blk).unwrap();
    println!("wrote output/block3185_original.bin ({} bytes, verbatim)", blk.len());
    if expected < blk.len() {
        let t = &blk[expected..];
        println!("TRAILING {} bytes, first 32: {:02X?}", t.len(), &t[..32.min(t.len())]);
        println!("all-zero trailing? {}", t.iter().all(|&b| b==0));
    }
}
