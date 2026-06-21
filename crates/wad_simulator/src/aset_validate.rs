//! ASET sub_entry OOB (out-of-bounds) resolution and heap crash diagnostics.
//!
//! This module detects and analyzes out-of-bounds ASET (Asset Set) entries that would cause
//! heap violations if the engine attempted to load them. An OOB entry occurs when the sub_entry
//! offset exceeds the actual entry count in the decompressed block, referencing garbage memory.
//!
//! # ASET Structure
//!
//! Each ASET block contains a header (4 bytes: entry count) followed by 16-byte entries:
//! - **name_hash** (u32): Asset identifier
//! - **type_hash** (u32): Asset type classifier
//! - **field_c** (u32): Secondary reference or metadata
//! - **chunk_size** (u32): Reserved/metadata field
//!
//! # OOB Detection
//!
//! For each ASET entry:
//! 1. Extract `sub_entry` offset from the packed_block_ref
//! 2. Decompress the target SGES block
//! 3. Read the block's entry count from first 4 bytes
//! 4. Check if `sub_entry >= entry_count`
//! 5. If yes, read garbage memory at that offset and log as OOB violation
//!
//! # Statistics
//!
//! [`AsetStats`] aggregates results across all ASET entries:
//! - `in_bounds`: Valid entries
//! - `out_of_bounds`: OOB entries with block data
//! - `oob_beyond_buffer`: OOB offsets that don't even fit in the decompressed buffer
//! - `decompression_failures`: Blocks that failed to decompress (likely corrupt)
//! - `xbox_pattern_count`: Entries with Xbox debug/pattern markers in garbage memory
//! - `garbage_alloc_total`: Estimated bytes of garbage memory pooled by OOB entries
//!
//! # Usage
//!
//! ```no_run
//! use wad_simulator::aset_validate::run_aset_oob;
//! use std::path::Path;
//!
//! let stats = run_aset_oob(
//!     Path::new("patch.wad"),
//!     Path::new("base.wad"),
//!     Path::new("base.wad"),
//!     false, // skip_aset
//! ).expect("ASET validation failed");
//!
//! println!("OOB entries: {}", stats.out_of_bounds);
//! ```

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aset_stats_default() {
        let stats = AsetStats::default();
        assert_eq!(stats.total_aset, 0);
        assert_eq!(stats.primary_count, 0);
        assert_eq!(stats.sub_entry_count, 0);
        assert_eq!(stats.in_bounds, 0);
        assert_eq!(stats.out_of_bounds, 0);
        assert_eq!(stats.oob_beyond_buffer, 0);
        assert_eq!(stats.decompression_failures, 0);
        assert_eq!(stats.garbage_alloc_total, 0);
        assert_eq!(stats.xbox_pattern_count, 0);
        assert!(stats.oob_details.is_empty());
    }

    #[test]
    fn oob_detail_construction() {
        let entry = BlockTableEntry {
            name_hash: 0x11111111,
            type_hash: 0x22222222,
            field_c: 0x33333333,
            chunk_size: 1024,
        };

        let detail = OobDetail {
            aset_index: 5,
            asset_hash: 0xAABBCCDD,
            type_id: 10,
            block_index: 3,
            sub_entry: 7,
            entry_count: 5,
            garbage_entry: entry,
        };

        assert_eq!(detail.aset_index, 5);
        assert_eq!(detail.asset_hash, 0xAABBCCDD);
        assert_eq!(detail.type_id, 10);
        assert_eq!(detail.block_index, 3);
        assert_eq!(detail.sub_entry, 7);
        assert_eq!(detail.entry_count, 5);
        assert_eq!(detail.garbage_entry.name_hash, 0x11111111);
        assert_eq!(detail.garbage_entry.chunk_size, 1024);
    }

    #[test]
    fn read_garbage_entry_valid() {
        let mut data = vec![0u8; 100];
        // Entry count at offset 0
        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        // Garbage entry at offset 4 + (1 * 16) = 20
        let offset = 20;
        data[offset..offset+4].copy_from_slice(&0x11111111u32.to_le_bytes());
        data[offset+4..offset+8].copy_from_slice(&0x22222222u32.to_le_bytes());
        data[offset+8..offset+12].copy_from_slice(&0x33333333u32.to_le_bytes());
        data[offset+12..offset+16].copy_from_slice(&512u32.to_le_bytes());

        let entry = read_garbage_entry(&data, 1);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.name_hash, 0x11111111);
        assert_eq!(e.type_hash, 0x22222222);
        assert_eq!(e.field_c, 0x33333333);
        assert_eq!(e.chunk_size, 512);
    }

    #[test]
    fn read_garbage_entry_beyond_buffer() {
        let data = vec![0u8; 30];
        let entry = read_garbage_entry(&data, 10);  // Would need offset 164
        assert!(entry.is_none());
    }

    #[test]
    fn read_garbage_entry_zero_sub_entry() {
        let mut data = vec![0u8; 100];
        data[4..8].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data[8..12].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
        data[12..16].copy_from_slice(&0x11223344u32.to_le_bytes());
        data[16..20].copy_from_slice(&2048u32.to_le_bytes());

        let entry = read_garbage_entry(&data, 0);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.name_hash, 0xDEADBEEF);
        assert_eq!(e.chunk_size, 2048);
    }

    #[test]
    fn hash_validation_stats_default() {
        let stats = HashValidationStats::default();
        assert_eq!(stats.total_aset, 0);
        assert_eq!(stats.verified, 0);
        assert_eq!(stats.misrouted, 0);
        assert_eq!(stats.true_ghost, 0);
        assert_eq!(stats.decompression_failures, 0);
        assert!(stats.ghost_details.is_empty());
    }

    #[test]
    fn ghost_detail_construction() {
        let detail = GhostDetail {
            aset_index: 10,
            asset_hash: 0x12345678,
            type_id: 50,
            block_index: 7,
            remappable_to: Some(9),
        };

        assert_eq!(detail.aset_index, 10);
        assert_eq!(detail.asset_hash, 0x12345678);
        assert_eq!(detail.type_id, 50);
        assert_eq!(detail.block_index, 7);
        assert!(detail.remappable_to.is_some());
        assert_eq!(detail.remappable_to.unwrap(), 9);
    }

    #[test]
    fn ghost_detail_no_remap() {
        let detail = GhostDetail {
            aset_index: 15,
            asset_hash: 0xFFFFFFFF,
            type_id: 99,
            block_index: 255,
            remappable_to: None,
        };

        assert!(detail.remappable_to.is_none());
    }

    #[test]
    fn aset_stats_accumulation() {
        let mut stats = AsetStats::default();
        stats.total_aset = 100;
        stats.primary_count = 50;
        stats.sub_entry_count = 50;
        stats.in_bounds = 40;
        stats.out_of_bounds = 10;

        assert_eq!(stats.in_bounds + stats.out_of_bounds, stats.sub_entry_count);
    }

    #[test]
    fn hash_validation_stats_accumulation() {
        let mut stats = HashValidationStats::default();
        stats.total_aset = 1000;
        stats.verified = 900;
        stats.misrouted = 50;
        stats.true_ghost = 50;

        assert_eq!(stats.verified + stats.misrouted + stats.true_ghost, 1000);
    }

    #[test]
    fn read_garbage_entry_exact_boundary() {
        let mut data = vec![0u8; 36];  // 4 + (1 * 16) = 20 is start, need 20 + 16 = 36 total
        data[4..8].copy_from_slice(&0x11111111u32.to_le_bytes());
        data[8..12].copy_from_slice(&0x22222222u32.to_le_bytes());
        data[12..16].copy_from_slice(&0x33333333u32.to_le_bytes());
        data[16..20].copy_from_slice(&100u32.to_le_bytes());
        // Now at offset 20 (where sub_entry=1 starts)
        data[20..24].copy_from_slice(&0xAAAAAAAAu32.to_le_bytes());
        data[24..28].copy_from_slice(&0xBBBBBBBBu32.to_le_bytes());
        data[28..32].copy_from_slice(&0xCCCCCCCCu32.to_le_bytes());
        data[32..36].copy_from_slice(&200u32.to_le_bytes());

        let entry = read_garbage_entry(&data, 1);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.name_hash, 0xAAAAAAAA);
        assert_eq!(e.chunk_size, 200);
    }

    #[test]
    fn read_garbage_entry_just_beyond() {
        let data = vec![0u8; 23];  // One byte short
        let entry = read_garbage_entry(&data, 1);
        assert!(entry.is_none());
    }

    #[test]
    fn oob_detail_multiple_instances() {
        let mut details = Vec::new();
        for i in 0..5 {
            details.push(OobDetail {
                aset_index: i,
                asset_hash: 0x10000000 | i as u32,
                type_id: 10 + i as u32,
                block_index: i as u16,
                sub_entry: (i * 2) as u16,
                entry_count: i as u32,
                garbage_entry: BlockTableEntry {
                    name_hash: 0,
                    type_hash: 0,
                    field_c: 0,
                    chunk_size: (i as u32) * 100,
                },
            });
        }

        assert_eq!(details.len(), 5);
        for (i, d) in details.iter().enumerate() {
            assert_eq!(d.aset_index, i);
            assert_eq!(d.garbage_entry.chunk_size, (i as u32) * 100);
        }
    }
}

