//! Dev bin: dump EVERY ASET row a name-hash owns — type_id, is_primary, block — plus a
//! whole-archive census of how many model hashes are striped across multiple blocks.
//!
//! `wad::extract_container` resolves ONE row (primary, else any) and slices one span out of one
//! block. If an asset's rows span several blocks, that call returns a FRAGMENT, not the asset.
//!
//!   cargo run -p mercs2_probe --bin aset_probe -- 0x9FCAE910 0x89D8DE72

use mercs2_engine::wad;
use std::collections::{BTreeMap, BTreeSet};

const MODEL_ASET_TYPE_ID: u32 = 19;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wants: Vec<u32> = args[1..]
        .iter()
        .filter_map(|a| a.strip_prefix("0x"))
        .filter_map(|h| u32::from_str_radix(h, 16).ok())
        .collect();

    let w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    // ---- Per-hash detail ----
    for want in &wants {
        let rows = wad::aset_types(&w, *want); // (type_id, is_primary, block_index)
        println!("\n=== 0x{want:08X}: {} ASET row(s) ===", rows.len());
        let blocks: BTreeSet<u16> = rows.iter().map(|(_, _, b)| *b).collect();
        println!("    distinct blocks: {} {:?}", blocks.len(), blocks);
        let model_blocks: BTreeSet<u16> = rows
            .iter()
            .filter(|(t, _, _)| *t == MODEL_ASET_TYPE_ID)
            .map(|(_, _, b)| *b)
            .collect();
        println!("    distinct blocks (type-19 only): {} {:?}", model_blocks.len(), model_blocks);
        let mut by_type: BTreeMap<u32, Vec<(bool, u16)>> = BTreeMap::new();
        for (t, p, b) in &rows {
            by_type.entry(*t).or_default().push((*p, *b));
        }
        for (t, v) in &by_type {
            let prim = v.iter().filter(|(p, _)| *p).count();
            let blks: BTreeSet<u16> = v.iter().map(|(_, b)| *b).collect();
            println!(
                "      type={t:<3} rows={:<4} primary={prim:<3} blocks={:?}",
                v.len(),
                blks
            );
        }
    }

    // ---- Whole-archive census over model-type hashes ----
    let model_hashes: Vec<u32> = wad::all_asets(&w)
        .into_iter()
        .filter(|(_, t, _)| *t == MODEL_ASET_TYPE_ID)
        .map(|(h, _, _)| h)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut multi_row = 0usize;
    let mut multi_block = 0usize;
    let mut no_primary = 0usize;
    let mut rows_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut blocks_hist: BTreeMap<usize, usize> = BTreeMap::new();

    for h in &model_hashes {
        let rows: Vec<_> =
            wad::aset_types(&w, *h).into_iter().filter(|(t, _, _)| *t == MODEL_ASET_TYPE_ID).collect();
        let blocks: BTreeSet<u16> = rows.iter().map(|(_, _, b)| *b).collect();
        *rows_hist.entry(rows.len()).or_default() += 1;
        *blocks_hist.entry(blocks.len()).or_default() += 1;
        if rows.len() > 1 {
            multi_row += 1;
        }
        if blocks.len() > 1 {
            multi_block += 1;
        }
        if !rows.iter().any(|(_, p, _)| *p) {
            no_primary += 1;
        }
    }

    println!("\n=== model-type (19) ASET census over vz.wad ===");
    println!("  distinct model hashes            : {}", model_hashes.len());
    println!("  with >1 ASET row                 : {multi_row}");
    println!("  with rows in >1 BLOCK (striped)  : {multi_block}");
    println!("  with NO primary row (sub only)   : {no_primary}");
    println!("  rows-per-hash histogram   : {rows_hist:?}");
    println!("  blocks-per-hash histogram : {blocks_hist:?}");
}
