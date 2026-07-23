//! THROWAWAY (2026-07-21): count type-0x7C569307 assets + dump a BE block.
use mercs2_formats::{ffcs, sges, dlc_input};
use std::fs::File;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut f = File::open(&a[0]).unwrap();
    let sz = f.metadata().unwrap().len();
    let arc = ffcs::load_ffcs_archive(&mut f, sz).unwrap();
    // ASET type_id is the ASSET-TYPE ordinal, not the UCFX type_hash; count via blocks instead.
    let mut n_tm = 0usize; let mut n_ent = 0usize;
    let be = matches!(arc.endian, ffcs::Endian::Big);
    let mut blocks: Vec<u16> = arc.aset.iter().map(|r| { let b = r.block_index(); if b == 0xFFFF { (r.packed_block_ref & 0xFFFF) as u16 } else { b } }).collect();
    blocks.sort(); blocks.dedup();
    for b in blocks {
        let mut raw = match sges::decompress_block(&mut f, &arc.indx, b) { Ok(v) => v, Err(_) => continue };
        if raw.len() >= 4 && &raw[..4] == b"segs" { let n0 = raw.len(); raw = match dlc_input::decompress_be_sges(&raw, 0, n0) { Ok(v) => v, Err(_) => continue }; }
        if raw.len() < 4 { continue; }
        let rd = |o: usize| if be { ffcs::read_u32_be(&raw, o) } else { ffcs::read_u32_le(&raw, o) };
        let n = rd(0) as usize;
        if n == 0 || n > 200 || 4 + n * 16 > raw.len() { continue; }
        for i in 0..n { n_ent += 1; if rd(8 + i * 16) == 0x7C56_9307 { n_tm += 1; } }
        if a.len() > 1 && b.to_string() == a[1] { std::fs::write(&a[2], &raw).unwrap(); println!("  dumped block {b} ({} bytes) -> {}", raw.len(), a[2]); }
    }
    println!("{}: entries={n_ent} terrainmesh(0x7C569307)={n_tm}", a[0]);
}
