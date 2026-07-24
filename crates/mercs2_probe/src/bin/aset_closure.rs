//! How many base blocks must a patch carry to keep every LOD chain it claims INTACT?
//!
//! An ASET row owned by block B can name finer rungs `_P001`/`_P002`/`_P003` in other blocks. If a
//! patch carries B but not those rungs, the choices are: sentinel them (stream is safe, but the
//! asset silently degrades to its coarse tier = base-game content loss), carry them too (complete
//! chain, costs blocks), or drop B's rows for that asset entirely so it resolves from base.
//!
//! This measures option 2: seed with the blocks a patch carries and take the transitive closure over
//! ASET rung references, reporting how many extra base blocks and bytes come along — i.e. whether
//! "just carry the referenced blocks" is bounded or pulls in the archive.
//!
//! Usage: `aset_closure <base.wad> <seed_block_index> [<seed> ...]`

use mercs2_formats::ffcs::load_ffcs_archive;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: aset_closure <base.wad> <seed_block_index> [<seed> ...]");
        std::process::exit(2);
    }
    let mut file = File::open(&args[0]).expect("open base wad");
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let archive = load_ffcs_archive(&mut file, size).expect("parse base wad");

    // rows grouped by OWNING block (_P000), so we can ask "what does block B pull in?"
    let mut rows_by_owner: HashMap<u16, Vec<usize>> = HashMap::new();
    for (i, e) in archive.aset.iter().enumerate() {
        rows_by_owner.entry(e.block_index()).or_default().push(i);
    }

    let seeds: Vec<u16> = args[1..].iter().filter_map(|s| s.parse::<u16>().ok()).collect();
    let mut seen: HashSet<u16> = seeds.iter().copied().collect();
    let mut queue: VecDeque<u16> = seeds.iter().copied().collect();
    let mut added_by_level: Vec<usize> = Vec::new();
    let mut level = 0usize;

    while !queue.is_empty() {
        let mut next: VecDeque<u16> = VecDeque::new();
        let mut added = 0usize;
        while let Some(b) = queue.pop_front() {
            for &ri in rows_by_owner.get(&b).map(|v| v.as_slice()).unwrap_or(&[]) {
                // lod_chain()[0] is _P000 (== b); the rest are the finer rungs
                for rung in archive.aset[ri].lod_chain().into_iter().skip(1) {
                    if seen.insert(rung) {
                        next.push_back(rung);
                        added += 1;
                    }
                }
            }
        }
        if added > 0 {
            added_by_level.push(added);
            level += 1;
            if level > 16 {
                println!("  (stopped at depth 16 — chain is not converging)");
                break;
            }
        }
        queue = next;
    }

    let seed_set: HashSet<u16> = seeds.iter().copied().collect();
    let extra: Vec<u16> = seen.difference(&seed_set).copied().collect();
    let bytes: u64 = extra
        .iter()
        .filter_map(|b| archive.indx.get(*b as usize))
        .map(|e| ((e.packed_field & 0x00FF_FFFF) as u64) * 0x8000)
        .sum();

    println!("seeds: {} block(s)", seeds.len());
    println!("closure: {} block(s) total ({} EXTRA to carry)", seen.len(), extra.len());
    println!("growth per level: {added_by_level:?}");
    println!(
        "extra decompressed bytes (declared): {} ({:.1} MB)",
        bytes,
        bytes as f64 / (1024.0 * 1024.0)
    );
    let mut sample: Vec<u16> = extra.clone();
    sample.sort_unstable();
    for b in sample.iter().take(12) {
        let path = archive.paths.get(*b as usize).map(|s| s.as_str()).unwrap_or("?");
        println!("   + block {b}: {path}");
    }
    if extra.len() > 12 {
        println!("   ... and {} more", extra.len() - 12);
    }
}
