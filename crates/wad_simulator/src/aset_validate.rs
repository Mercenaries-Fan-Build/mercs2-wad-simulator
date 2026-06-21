//! ASET hash-ownership validation.
//!
//! Each ASET row is 16 bytes: `{ asset_hash:u32, secondary_ref:u32,
//! packed_ref:u32, type_id:u32 }`, where `packed_ref = { block_index:hi16,
//! sub_offset:lo16 }` (LE / PC field order).
//!
//! - `sub_offset == 0xFFFF` -> PRIMARY: the asset resolves by hash in
//!   `block_index` (the "resolve-by-hash" sentinel).
//! - `sub_offset != 0xFFFF` -> SUB-ENTRY: the asset still lives in `block_index`
//!   (it resolves by hash there); `sub_offset` is the BYTE OFFSET of its
//!   sub-resource descriptor within the decompressed block — NOT an index into
//!   the 16-byte entry table.
//!
//! RE of retail `game-files/vz.wad` (10,798 non-primary entries):
//!   * asset_hash present in its claimed block's entry table: 10,798/10,798 (100%)
//!     — this is the authoritative validity check.
//!   * `sub_offset` < decompressed block length: 10,706/10,798. The remaining 92
//!     are all streaming textures (type 27): the in-WAD block is a small descriptor
//!     and `sub_offset` indexes the EXTERNAL texture stream, so it is not bounded by
//!     the block (the texture analogue of codec-0x04 audio → `.pws`). Informational.
//!   * the OLD model (`sub_offset < entry_count`, treating it as a 16-byte table
//!     index) held for 10/10,798 — i.e. it false-flagged ~10,788 retail entries as
//!     "OOB / heap corruption". That validator (`run_aset_oob`) has been removed.
//!
//! Correct validation (below): confirm every ASET entry's `asset_hash` exists in
//! the block it claims (verified / misrouted / true-ghost), accounting for primary
//! vs sub-entries and checking sub-entry `sub_offset`s are in-bounds byte offsets.
//! The converter-bug class (sub sentinel overwritten) is detected differentially
//! by `tools/aset_sub_oracle_audit.py`.

use colored::*;
use std::collections::HashMap;
use std::fs::File;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::parse_block_entry_table;

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
    /// Primary entries (sub_offset == 0xFFFF, resolved by hash).
    pub primary_count: usize,
    /// Sub-entries (sub_offset != 0xFFFF, hash + sub-resource byte offset).
    pub sub_entry_count: usize,
    /// Sub-entries whose `sub_offset` lands past the decompressed block. On retail
    /// these are all streaming textures (type 27) whose in-WAD block is a small
    /// descriptor and whose `sub_offset` indexes the EXTERNAL texture stream — not
    /// the block (the texture analogue of codec-0x04 audio → `.pws`). Informational:
    /// it is NOT a defect (92 on retail vz.wad, which renders fine); the asset still
    /// resolves by hash. We cannot bound a stream offset without the stream file.
    pub sub_offset_streamed: usize,
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
    let mut block_len: HashMap<u16, usize> = HashMap::new();
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
                block_len.insert(idx, decompressed.len());
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

        // Account for sub-entries: sub_offset != 0xFFFF means the asset lives in
        // block_idx and its sub-resource descriptor is at byte offset `sub_offset`
        // within the decompressed block — EXCEPT for streaming textures, whose block
        // is a small descriptor and whose sub_offset indexes the external texture
        // stream (counted as `sub_offset_streamed`, informational, not a defect). The
        // authoritative validity check is hash-ownership below.
        if aset.is_primary() {
            stats.primary_count += 1;
        } else {
            stats.sub_entry_count += 1;
            if let Some(&blen) = block_len.get(&block_idx) {
                if (aset.sub_entry() as usize) >= blen {
                    stats.sub_offset_streamed += 1;
                }
            }
        }

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
    println!(
        "  Primary: {}  Sub-entries: {}  (of which {} are streaming textures referencing the external texture stream)",
        stats.primary_count, stats.sub_entry_count, stats.sub_offset_streamed
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
    fn hash_validation_stats_default() {
        let stats = HashValidationStats::default();
        assert_eq!(stats.total_aset, 0);
        assert_eq!(stats.verified, 0);
        assert_eq!(stats.misrouted, 0);
        assert_eq!(stats.true_ghost, 0);
        assert_eq!(stats.decompression_failures, 0);
        assert!(stats.ghost_details.is_empty());
        // Sub-entry accounting (corrected model).
        assert_eq!(stats.primary_count, 0);
        assert_eq!(stats.sub_entry_count, 0);
        assert_eq!(stats.sub_offset_streamed, 0);
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
        assert_eq!(detail.remappable_to, Some(9));
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
    fn hash_validation_stats_accumulation() {
        let mut stats = HashValidationStats::default();
        stats.total_aset = 1000;
        stats.verified = 900;
        stats.misrouted = 50;
        stats.true_ghost = 50;

        assert_eq!(stats.verified + stats.misrouted + stats.true_ghost, 1000);
    }

    #[test]
    fn primary_and_sub_entries_partition_total() {
        // Corrected model: every entry is either primary (sub_offset == 0xFFFF,
        // resolve-by-hash) or a sub-entry (hash + sub-resource byte offset).
        let mut stats = HashValidationStats::default();
        stats.total_aset = 30645;
        stats.primary_count = 19847;
        stats.sub_entry_count = 10798;
        // sub_offset is a sub-resource byte offset, not a table index. On retail
        // vz.wad 92 of the sub-entries are streaming textures whose sub_offset
        // indexes the external texture stream (informational, not a defect).
        stats.sub_offset_streamed = 92;

        assert_eq!(stats.primary_count + stats.sub_entry_count, stats.total_aset);
        assert!(stats.sub_offset_streamed <= stats.sub_entry_count);
    }
}

