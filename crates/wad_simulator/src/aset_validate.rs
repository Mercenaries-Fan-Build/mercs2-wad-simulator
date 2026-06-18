//! ASET sub_entry OOB resolution (heap crash diagnostic).

use colored::*;
use std::collections::HashMap;
use std::fs::File;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{parse_block_entry_table, BlockTableEntry};

#[derive(Debug)]
pub struct OobDetail {
    pub aset_index: usize,
    pub asset_hash: u32,
    pub type_id: u32,
    pub block_index: u16,
    pub sub_entry: u16,
    pub entry_count: u32,
    pub garbage_entry: BlockTableEntry,
}

#[derive(Debug, Default)]
pub struct AsetStats {
    pub total_aset: usize,
    pub primary_count: usize,
    pub sub_entry_count: usize,
    pub in_bounds: usize,
    pub out_of_bounds: usize,
    pub oob_beyond_buffer: usize,
    pub decompression_failures: usize,
    pub garbage_alloc_total: u64,
    pub xbox_pattern_count: usize,
    pub oob_details: Vec<OobDetail>,
}

fn read_garbage_entry(decompressed: &[u8], sub_entry: u16) -> Option<BlockTableEntry> {
    let offset = 4 + (sub_entry as usize) * 16;
    if offset + 16 > decompressed.len() {
        return None;
    }
    Some(BlockTableEntry {
        name_hash: mercs2_formats::ffcs::read_u32_le(decompressed, offset),
        type_hash: mercs2_formats::ffcs::read_u32_le(decompressed, offset + 4),
        field_c: mercs2_formats::ffcs::read_u32_le(decompressed, offset + 8),
        chunk_size: mercs2_formats::ffcs::read_u32_le(decompressed, offset + 12),
    })
}

pub fn run_aset_oob(
    wad_path: &std::path::Path,
    oob_only: bool,
    limit: usize,
) -> Result<AsetStats, Box<dyn std::error::Error>> {
    let mut file = File::open(wad_path)?;
    let file_size = file.metadata()?.len();
    let arch = load_ffcs_archive(&mut file, file_size)?;
    let aset_entries = arch.aset;
    let indx_entries = arch.indx;

    let mut block_cache: HashMap<u16, Result<Vec<u8>, String>> = HashMap::new();
    let mut stats = AsetStats {
        total_aset: aset_entries.len(),
        ..Default::default()
    };

    let process_count = if limit > 0 {
        limit.min(aset_entries.len())
    } else {
        aset_entries.len()
    };

    for (i, aset) in aset_entries.iter().enumerate().take(process_count) {
        let block_idx = aset.block_index();
        let sub_entry = aset.sub_entry();

        if aset.is_primary() {
            stats.primary_count += 1;
            if !oob_only {
                println!(
                    "  ASET[{i:5}] hash=0x{:08X} type={:2} block={block_idx:4} → {}",
                    aset.asset_hash,
                    aset.type_id,
                    "OK".green()
                );
            }
            continue;
        }

        stats.sub_entry_count += 1;

        if !block_cache.contains_key(&block_idx) {
            let result = decompress_block(&mut file, &indx_entries, block_idx);
            block_cache.insert(block_idx, result);
        }

        match block_cache.get(&block_idx).unwrap() {
            Err(_) => {
                stats.decompression_failures += 1;
            }
            Ok(decompressed) => {
                let (entry_count, _) = parse_block_entry_table(decompressed);
                if (sub_entry as u32) < entry_count {
                    stats.in_bounds += 1;
                } else {
                    stats.out_of_bounds += 1;
                    if sub_entry == block_idx {
                        stats.xbox_pattern_count += 1;
                    }
                    if let Some(g) = read_garbage_entry(decompressed, sub_entry) {
                        stats.garbage_alloc_total += g.chunk_size as u64;
                        stats.oob_details.push(OobDetail {
                            aset_index: i,
                            asset_hash: aset.asset_hash,
                            type_id: aset.type_id,
                            block_index: block_idx,
                            sub_entry,
                            entry_count,
                            garbage_entry: g,
                        });
                    } else {
                        stats.oob_beyond_buffer += 1;
                    }
                    if !oob_only {
                        println!(
                            "  ASET[{i:5}] hash=0x{:08X} → {}",
                            aset.asset_hash,
                            "OOB ACCESS".red().bold()
                        );
                    }
                }
            }
        }
    }

    Ok(stats)
}

pub fn print_aset_summary(stats: &AsetStats) {
    println!(
        "  Total ASET: {}  Primary: {}  OOB: {}",
        stats.total_aset,
        stats.primary_count,
        stats.out_of_bounds
    );
    if stats.out_of_bounds > 0 {
        println!(
            "  {} Heap corruption risk from OOB sub_entry indices",
            "WARNING:".red().bold()
        );
    } else {
        println!("  {} No OOB sub_entry accesses", "OK:".green().bold());
    }
}

// ── ASET hash ownership validation ──────────────────────────────────

#[derive(Debug)]
pub struct GhostDetail {
    pub aset_index: usize,
    pub asset_hash: u32,
    pub type_id: u32,
    pub block_index: u16,
    /// If the hash exists in another block, this is `Some(correct_block)`.
    pub remappable_to: Option<u16>,
}

#[derive(Debug, Default)]
pub struct HashValidationStats {
    pub total_aset: usize,
    pub verified: usize,
    /// Hash exists in another block — wrong block_index, remappable.
    pub misrouted: usize,
    /// Hash not found in any block — true ghost / base-game-only.
    pub true_ghost: usize,
    pub decompression_failures: usize,
    pub ghost_details: Vec<GhostDetail>,
}

/// Check that every ASET entry's `asset_hash` actually exists in the
/// block entry table it claims to own.
///
/// Entries whose hash is absent from their claimed block are classified:
/// - **misrouted** — hash exists in a *different* block (wrong block_index,
///   the build pipeline should remap these)
/// - **true ghost** — hash not found in *any* block (base-game-only asset,
///   should be removed)
pub fn run_aset_hash_validation(
    wad_path: &std::path::Path,
    limit: usize,
) -> Result<HashValidationStats, Box<dyn std::error::Error>> {
    let mut file = File::open(wad_path)?;
    let file_size = file.metadata()?.len();
    let arch = load_ffcs_archive(&mut file, file_size)?;
    let aset_entries = arch.aset;
    let indx_entries = arch.indx;

    // Pre-decompress all blocks and build a global hash → block_index map
    let block_count = indx_entries.len();
    let mut block_hash_cache: HashMap<u16, Vec<u32>> = HashMap::new();
    let mut global_hash_map: HashMap<u32, u16> = HashMap::new();

    for blk_idx in 0..block_count {
        let idx = blk_idx as u16;
        match decompress_block(&mut file, &indx_entries, idx) {
            Ok(decompressed) => {
                let (entry_count, entries) = parse_block_entry_table(&decompressed);
                let hashes: Vec<u32> = entries
                    .iter()
                    .take(entry_count as usize)
                    .map(|e| e.name_hash)
                    .collect();
                for &h in &hashes {
                    global_hash_map.entry(h).or_insert(idx);
                }
                block_hash_cache.insert(idx, hashes);
            }
            Err(_) => {}
        }
    }

    let mut stats = HashValidationStats {
        total_aset: aset_entries.len(),
        ..Default::default()
    };

    let process_count = if limit > 0 {
        limit.min(aset_entries.len())
    } else {
        aset_entries.len()
    };

    for (i, aset) in aset_entries.iter().enumerate().take(process_count) {
        let block_idx = aset.block_index();

        match block_hash_cache.get(&block_idx) {
            None => {
                stats.decompression_failures += 1;
            }
            Some(block_hashes) => {
                if block_hashes.contains(&aset.asset_hash) {
                    stats.verified += 1;
                } else {
                    // Not in claimed block — check global map
                    let remap_target = global_hash_map.get(&aset.asset_hash).copied();
                    if let Some(_correct_blk) = remap_target {
                        stats.misrouted += 1;
                    } else {
                        stats.true_ghost += 1;
                    }
                    if stats.ghost_details.len() < 200 {
                        stats.ghost_details.push(GhostDetail {
                            aset_index: i,
                            asset_hash: aset.asset_hash,
                            type_id: aset.type_id,
                            block_index: block_idx,
                            remappable_to: remap_target,
                        });
                    }
                }
            }
        }
    }

    Ok(stats)
}

pub fn print_hash_validation_summary(stats: &HashValidationStats) {
    let orphan_total = stats.misrouted + stats.true_ghost;
    println!(
        "  Total ASET: {}  Verified: {}  Misrouted: {}  True ghost: {}",
        stats.total_aset, stats.verified, stats.misrouted, stats.true_ghost
    );
    if orphan_total > 0 {
        if stats.misrouted > 0 {
            println!(
                "  {} {} ASET entries point to the wrong block (hash exists in another block)",
                "ERROR:".red().bold(),
                stats.misrouted
            );
        }
        if stats.true_ghost > 0 {
            println!(
                "  {} {} ASET entries reference assets not in any block (base-game ghosts)",
                "WARNING:".yellow().bold(),
                stats.true_ghost
            );
        }
        let show = stats.ghost_details.len().min(20);
        for d in &stats.ghost_details[..show] {
            let tag = match d.remappable_to {
                Some(target) => format!("MISROUTED → block {target}"),
                None => "TRUE GHOST".to_string(),
            };
            println!(
                "    ASET[{:5}] hash=0x{:08X} type={:2} block={:4} — {}",
                d.aset_index, d.asset_hash, d.type_id, d.block_index, tag
            );
        }
        if stats.ghost_details.len() > 20 {
            println!("    ... and {} more", stats.ghost_details.len() - 20);
        }
        if orphan_total > stats.ghost_details.len() {
            println!(
                "    (detail capped at {}; {} total orphan entries)",
                stats.ghost_details.len(),
                orphan_total
            );
        }
    } else {
        println!(
            "  {} All ASET entries verified against block content",
            "OK:".green().bold()
        );
    }
}
