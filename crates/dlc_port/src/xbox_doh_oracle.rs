//! Xbox-DOH side oracle for `dlc_port`.
//!
//! When the PS3 UCFX descriptor walker (`ucfx_byteswap::convert_block`) refuses
//! a block with the `PS3 compact decl detected` error class — the walker's
//! loud-fail arm for the non-interleaved multi-stream vertex layout the PS3
//! DLC ships (see `docs/_descriptor_walker_oracle.md`) — the sibling Xbox 360
//! DOH holds a cross-platform-identical asset in LE-translatable form. This
//! module parses the Xbox DOH, converts every block through the same walker,
//! and indexes the result by its PRIMARY `name_hash` (the first entry in the
//! block's LE entry table). The driver in `main.rs` looks up the rejected
//! PS3 block's primary `name_hash` here and uses the Xbox-DOH LE body in its
//! place.
//!
//! The key is the primary `name_hash`, not the block path, because PS3 and
//! Xbox ship nearly-identical path strings but with LOD-tier / platform-suffix
//! divergences (29 PS3-only paths plus `_P003_Q0` ↔ `_P001_Q2` rung shuffles
//! documented in `docs/_dlc01_pipeline_readiness.md` §4). The first-entry
//! `name_hash` is the engine's own identifier for an asset and is identical
//! across PS3 and Xbox for the same content.
//!
//! The sges round-trip, ASET resolution and CSUM accounting all run on the
//! Xbox-DOH LE body with no special-casing: the per-block CSUM trailer that
//! `convert_block` embeds is already in the LE body, and the output WAD's
//! CSUM / ASET layout is rebuilt by `build_patch_wad_multi` regardless of
//! which input the LE body came from.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::ucfx::parse_block_entry_table;
use ucfx_byteswap::convert::{convert_block, QUIET};

/// Marker substring produced by `mercs2_formats::be_to_le::convert::convert_decl_ps3_compact`.
///
/// Anchoring on this exact phrase keeps the side-oracle scoped to the one
/// rejection class the Xbox DOH can resolve. Other `convert_block` errors
/// (wavebank-schema mismatch, `unluac.jar not found`, truncated UCFX, etc.)
/// are shared between the PS3 and Xbox DOH inputs on the DLC01 corpus and are
/// out of scope for this oracle — they stay on the existing skip path.
pub const PS3_COMPACT_DECL_MARKER: &str = "PS3 compact decl detected";

/// One decoded Xbox-DOH block, ready to drop into the output WAD in place of
/// a PS3 block the descriptor walker refused.
pub struct OracleBlock {
    /// Primary (first-entry) `name_hash` of the Xbox-DOH LE block. The lookup
    /// key used by [`XboxDohOracle::lookup`].
    pub primary_name_hash: u32,
    /// `type_hash` of the primary entry — propagated into log messages so a
    /// miss / hit can be classified at a glance (`model` vs `terrainmesh` …).
    pub primary_type_hash: u32,
    /// The Xbox DOH's PTHS entry for this block. Used only for diagnostics;
    /// a miss / collision message cites this path alongside the PS3 path.
    pub xbox_path: String,
    /// Decompressed LE bytes — the direct output of `convert_block` on the
    /// Xbox 360 BE body. Ready for `sges::compress_sges` + the round-trip
    /// verify the main loop performs on every converted block.
    pub le_decompressed: Vec<u8>,
}

/// Fully-indexed Xbox-DOH side oracle. Build with [`XboxDohOracle::load`] from
/// a DOH / STFS / BE SCFF container, then call [`XboxDohOracle::lookup`] with
/// the primary `name_hash` of a PS3 block to retrieve the LE body.
pub struct XboxDohOracle {
    /// Container path recorded at `load` time; cited in loud-fail messages.
    pub source_path: PathBuf,
    /// Count of Xbox blocks the walker converted and the oracle indexed.
    /// Separate from `by_name_hash.len()` only when first-wins collisions
    /// dropped later entries.
    pub converted_blocks: usize,
    /// `name_hash` → oracle block. First-wins on collision (matches the
    /// engine's own `first-writer-wins` registry semantics for name_hash).
    by_name_hash: HashMap<u32, OracleBlock>,
}

impl XboxDohOracle {
    /// Load an Xbox DOH / STFS / BE SCFF, convert every block BE→LE, and index
    /// the results by primary `name_hash`.
    ///
    /// A block that fails any step (segs-decompress, non-SCFF payload, BE→LE
    /// convert) is **not** silently dropped from the pipeline: it is omitted
    /// from the oracle's index with a one-line stderr trace. The loud-fail
    /// contract is on the caller — if the PS3 side later asks for a hash the
    /// oracle does not hold, [`XboxDohOracle::lookup`] returns `None` and the
    /// caller must fail the run.
    pub fn load(path: &Path) -> Result<Self, String> {
        let (doh, src) = load_stfs_or_doh(path)
            .map_err(|e| format!("xbox-doh-oracle: open {}: {e}", path.display()))?;
        println!(
            "  Xbox-DOH oracle: loaded {} ({} bytes, source={src})",
            path.display(),
            doh.len()
        );

        let (_version, rows) = parse_be_ffcs(&doh)
            .map_err(|e| format!("xbox-doh-oracle: parse FFCS {}: {e}", path.display()))?;
        let chunk = |t: &str| rows.iter().find(|r| r.tag == t);
        let indx_row = chunk("INDX")
            .ok_or_else(|| format!("xbox-doh-oracle: missing INDX in {}", path.display()))?
            .clone();
        let num_blocks = indx_row.meta as usize;
        let indx = parse_be_indx(&doh, indx_row.offset as usize, num_blocks);
        let pths = chunk("PTHS")
            .map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize))
            .unwrap_or_default();

        // The main loop toggles QUIET to silence per-block logging; mirror that
        // here. Harmless to leave set — the caller sets it unconditionally.
        QUIET.store(true, Ordering::Relaxed);

        let mut by_name_hash: HashMap<u32, OracleBlock> = HashMap::new();
        let mut collisions: Vec<(u32, String, String)> = Vec::new();
        let mut converted = 0usize;
        let mut decompress_fail = 0usize;
        let mut convert_fail = 0usize;
        let mut unindexable = 0usize;

        for (blk_idx, e) in indx.iter().enumerate() {
            let path_s = pths
                .get(blk_idx)
                .cloned()
                .unwrap_or_else(|| format!("xbox_block_{blk_idx:05}"));
            let block_offset = e.file_offset();
            let block_size = e.page_count as usize * PAGE_SIZE;
            if block_offset + 4 > doh.len() {
                decompress_fail += 1;
                continue;
            }
            let slice = &doh[block_offset..(block_offset + block_size).min(doh.len())];

            // Mirror main.rs's two-path decompressor: BE segs with the "segs"
            // magic, or an XFCU-headed raw block. Anything else is left out.
            let decompressed: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
                match decompress_be_sges(slice, 0, slice.len()) {
                    Ok(d) => d,
                    Err(err) => {
                        eprintln!("  xbox-doh-oracle: segs-decompress {path_s}: {err}");
                        decompress_fail += 1;
                        continue;
                    }
                }
            } else if slice.len() >= 8 {
                let rec =
                    u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
                let header_end = 4 + rec * 16;
                let first_tag = slice.get(header_end..header_end + 4);
                if rec > 0 && rec < 5000 && first_tag == Some(b"XFCU") {
                    let mut d = slice.to_vec();
                    let mut z = d.len();
                    while z > 4 && d[z - 1] == 0 {
                        z -= 1;
                    }
                    z = (z + 3) & !3;
                    d.truncate(z);
                    d
                } else {
                    decompress_fail += 1;
                    continue;
                }
            } else {
                decompress_fail += 1;
                continue;
            };

            let swapped = match convert_block(&decompressed, false, None) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("  xbox-doh-oracle: convert_block {path_s}: {err}");
                    convert_fail += 1;
                    continue;
                }
            };

            let (_, entries) = parse_block_entry_table(&swapped);
            let Some(first) = entries.first() else {
                eprintln!(
                    "  xbox-doh-oracle: {path_s}: converted block has an empty entry table"
                );
                unindexable += 1;
                continue;
            };
            let name_hash = first.name_hash;
            if name_hash == 0 {
                eprintln!(
                    "  xbox-doh-oracle: {path_s}: primary entry name_hash is zero (unindexable)"
                );
                unindexable += 1;
                continue;
            }

            let block = OracleBlock {
                primary_name_hash: name_hash,
                primary_type_hash: first.type_hash,
                xbox_path: path_s.clone(),
                le_decompressed: swapped,
            };
            match by_name_hash.entry(name_hash) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(block);
                    converted += 1;
                }
                std::collections::hash_map::Entry::Occupied(occ) => {
                    collisions.push((name_hash, occ.get().xbox_path.clone(), path_s));
                    converted += 1;
                }
            }
        }

        if decompress_fail + convert_fail + unindexable > 0 {
            println!(
                "  Xbox-DOH oracle: segs-decompress skipped {decompress_fail}, convert_block skipped {convert_fail}, entry-table unindexable {unindexable}"
            );
        }
        if !collisions.is_empty() {
            let sample = &collisions[0];
            eprintln!(
                "  Xbox-DOH oracle: {} primary name_hash collision(s) in {} — first-wins. Sample: 0x{:08X} first={} dropped={}",
                collisions.len(),
                path.display(),
                sample.0,
                sample.1,
                sample.2
            );
        }
        println!(
            "  Xbox-DOH oracle: {} blocks indexed by primary name_hash ({} input blocks total)",
            by_name_hash.len(),
            num_blocks
        );

        Ok(Self {
            source_path: path.to_path_buf(),
            converted_blocks: converted,
            by_name_hash,
        })
    }

    /// Test / probe constructor: wrap an in-memory block list without reading
    /// any file. Preserves the first-wins semantics of [`XboxDohOracle::load`].
    pub fn from_blocks(source_path: PathBuf, blocks: Vec<OracleBlock>) -> Self {
        let mut by_name_hash: HashMap<u32, OracleBlock> = HashMap::new();
        let mut converted = 0usize;
        for b in blocks {
            let nh = b.primary_name_hash;
            if nh == 0 {
                continue;
            }
            by_name_hash.entry(nh).or_insert(b);
            converted += 1;
        }
        Self {
            source_path,
            converted_blocks: converted,
            by_name_hash,
        }
    }

    /// Resolve a PS3 block's primary `name_hash` to the Xbox-DOH-sourced LE
    /// body. Returns `None` when the Xbox DOH does not hold the asset, which
    /// is the signal for the caller to fail the whole run per the loud-fail
    /// contract.
    pub fn lookup(&self, name_hash: u32) -> Option<&OracleBlock> {
        self.by_name_hash.get(&name_hash)
    }

    /// Number of distinct primary name_hashes the oracle can resolve.
    pub fn indexed_count(&self) -> usize {
        self.by_name_hash.len()
    }
}

/// Extract the primary (first-entry) `name_hash` from a decompressed BE UCFX
/// block, using the same entry-table layout `convert_block` reads: a BE u32
/// entry count at offset 0, then N × 16-byte entries whose first 4 bytes are
/// the BE `name_hash`.
///
/// Returns `None` when the block is shorter than one header + one entry
/// (20 bytes) or claims zero entries. `None` is the signal for the caller to
/// fail loud: a PS3 block whose primary `name_hash` cannot be recovered also
/// cannot be routed through the oracle.
pub fn primary_name_hash_be(be_decompressed: &[u8]) -> Option<u32> {
    if be_decompressed.len() < 20 {
        return None;
    }
    let count = u32::from_be_bytes([
        be_decompressed[0],
        be_decompressed[1],
        be_decompressed[2],
        be_decompressed[3],
    ]);
    if count == 0 {
        return None;
    }
    let name_hash = u32::from_be_bytes([
        be_decompressed[4],
        be_decompressed[5],
        be_decompressed[6],
        be_decompressed[7],
    ]);
    Some(name_hash)
}

/// `true` when `convert_block`'s error message signals the one rejection
/// class the Xbox-DOH side oracle is designed to resolve. Anchored on the
/// exact phrase `convert_decl_ps3_compact` emits.
pub fn is_ps3_compact_decl_rejection(err: &str) -> bool {
    err.contains(PS3_COMPACT_DECL_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_with_primary(name_hash: u32, path: &str) -> OracleBlock {
        OracleBlock {
            primary_name_hash: name_hash,
            primary_type_hash: 0x5B72_4250, // model
            xbox_path: path.to_string(),
            le_decompressed: vec![0u8; 32],
        }
    }

    #[test]
    fn lookup_by_name_hash_resolves_a_known_block() {
        let oracle = XboxDohOracle::from_blocks(
            PathBuf::from("fake_xbox.doh"),
            vec![
                block_with_primary(0xDEAD_BEEF, "blocks\\dlc01\\deadbeef_P000_Q3.block"),
                block_with_primary(0x1234_5678, "blocks\\dlc01\\12345678_P000_Q3.block"),
            ],
        );
        assert_eq!(oracle.indexed_count(), 2);
        let hit = oracle.lookup(0xDEAD_BEEF).expect("expected hit on known hash");
        assert_eq!(hit.primary_name_hash, 0xDEAD_BEEF);
        assert_eq!(hit.xbox_path, "blocks\\dlc01\\deadbeef_P000_Q3.block");
    }

    #[test]
    fn lookup_misses_report_none_not_a_fallback_block() {
        let oracle = XboxDohOracle::from_blocks(
            PathBuf::from("fake_xbox.doh"),
            vec![block_with_primary(0xAAAA_AAAA, "blocks\\dlc01\\a_P000_Q3.block")],
        );
        // Unknown hash must miss — the dispatcher turns a miss into a loud
        // whole-run failure. A silent fallback here would defeat the point.
        assert!(oracle.lookup(0xBBBB_BBBB).is_none());
    }

    #[test]
    fn first_wins_on_duplicate_primary_name_hash() {
        let oracle = XboxDohOracle::from_blocks(
            PathBuf::from("fake_xbox.doh"),
            vec![
                block_with_primary(0xCAFE_F00D, "blocks\\dlc01\\first.block"),
                block_with_primary(0xCAFE_F00D, "blocks\\dlc01\\second.block"),
            ],
        );
        assert_eq!(oracle.indexed_count(), 1);
        let hit = oracle.lookup(0xCAFE_F00D).unwrap();
        assert_eq!(hit.xbox_path, "blocks\\dlc01\\first.block");
    }

    #[test]
    fn primary_name_hash_be_reads_offset_4_as_be_u32() {
        // entry_count=1 at BE offset 0, name_hash=0xDEADBEEF at BE offset 4,
        // then 12 bytes of entry payload (type/field_c/chunk_size) to reach
        // the 20-byte minimum this function requires.
        let mut be = Vec::new();
        be.extend_from_slice(&1u32.to_be_bytes());
        be.extend_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        be.extend_from_slice(&[0u8; 12]);
        assert_eq!(primary_name_hash_be(&be), Some(0xDEAD_BEEF));
    }

    #[test]
    fn primary_name_hash_be_returns_none_when_entry_count_is_zero() {
        let mut be = Vec::new();
        be.extend_from_slice(&0u32.to_be_bytes()); // count = 0
        be.extend_from_slice(&[0u8; 16]);
        assert!(primary_name_hash_be(&be).is_none());
    }

    #[test]
    fn primary_name_hash_be_returns_none_when_block_too_short() {
        // 19 bytes — one short of header (4) + one entry (16).
        let be = vec![0u8; 19];
        assert!(primary_name_hash_be(&be).is_none());
    }

    #[test]
    fn ps3_compact_decl_rejection_matches_the_live_error_text() {
        // Exact substring the mercs2_formats loud-fail arm emits — anchored
        // on this phrase so unrelated convert_block errors (wavebank,
        // unluac, truncated UCFX) are left on the existing skip path.
        let live = "convert_block: PS3 compact decl detected (bytes=`0300000304060304060a0201`, \
                    terrain stride-14 ground (xbox-oracle: COLOR@8 D3DCOLOR + NORMAL@12 F16x4)): \
                    the PS3 DLC ships a non-interleaved multi-stream vertex layout; …";
        assert!(is_ps3_compact_decl_rejection(live));
        assert!(!is_ps3_compact_decl_rejection(
            "wavebank transcode: wavebank records_offset 56, expected 40"
        ));
        assert!(!is_ps3_compact_decl_rejection("unluac.jar not found"));
    }
}
