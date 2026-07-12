//! What chunks does each LOD rung of a model actually contain? The resident (coarse) rung carries
//! HIER + the destruction machine; the streamed rungs carry 28k triangles with no HIER at all. The
//! object is assembled ACROSS blocks, so we need to know exactly which pieces each block ships.
//!
//!   cargo run -p mercs2_probe --bin lod_chunks -- ch_veh_tank_ztz98

use mercs2_engine::wad;
use std::collections::BTreeMap;

/// Tally the UCFX descriptor table: 20-byte rows of {tag[4], body_off, body_size, ...}.
fn chunk_tally(c: &[u8]) -> BTreeMap<String, (usize, usize)> {
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let u32at = |o: usize| u32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]) as usize;
    let n_desc = u32at(16).min(c.len().saturating_sub(20) / 20);
    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > c.len() {
            break;
        }
        let name: String =
            c[row..row + 4].iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '.' }).collect();
        let e = out.entry(name).or_insert((0, 0));
        e.0 += 1;
        e.1 += u32at(row + 8);
    }
    out
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let lods = wad::extract_model_lods(&mut w, hash).expect("lod chain");

    let tallies: Vec<(u8, BTreeMap<String, (usize, usize)>)> =
        lods.iter().map(|l| (l.level, chunk_tally(&l.container))).collect();

    let mut all: Vec<String> = tallies.iter().flat_map(|(_, t)| t.keys().cloned()).collect();
    all.sort();
    all.dedup();

    print!("{name}\n\n{:8}", "chunk");
    for (lv, _) in &tallies {
        print!("  {:>18}", format!("P{lv:03}"));
    }
    println!();
    for k in &all {
        print!("{k:8}");
        for (_, t) in &tallies {
            match t.get(k) {
                Some((n, b)) => print!("  {:>18}", format!("{n} x, {b} B")),
                None => print!("  {:>18}", "-"),
            }
        }
        println!();
    }
}
