//! The engine's block-entry install ring, against a mounted WAD stack.
//!
//! PC v1.0 `FUN_00875140` sizes a ring of 16-byte records at `[0x0117662C]` from each mounted WAD's
//! FFCS header CSUM row `meta` (header +0x2C); `FUN_00872c50` only ever grows it, so its capacity is the
//! largest `meta` of every WAD mounted so far. Before installing a streamed block's entries,
//! `FUN_00875cc0` returns "retry next tick" while the block's entry count exceeds the ring's free
//! slots — a block with more entries than every mounted `meta` is never installed and its level never
//! starts (`docs/format_reference.md`, CSUM row). Retail WADs set `meta` to their largest block's
//! entry count.
//!
//! [`check_install_ring`] takes the ring as the largest `meta` of the WADs it is given and reports every
//! block of those WADs with more entries than that.

use mercs2_formats::ffcs::{find_chunk, load_ffcs_archive};
use std::fs::File;
use std::path::{Path, PathBuf};

/// A block the ring cannot hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overflow {
    pub wad: PathBuf,
    pub block: String,
    pub entries: u32,
}

#[derive(Debug, Clone)]
pub struct RingReport {
    /// The largest CSUM `meta` of the WADs checked, and the WAD it comes from.
    pub ring: u32,
    pub ring_from: PathBuf,
    /// Blocks read across every WAD.
    pub blocks: usize,
    pub overflows: Vec<Overflow>,
}

impl RingReport {
    /// `"<n> block(s) never installed (<block>: <entries> entries, …, ring <ring>)"`; `None` when every
    /// block fits the ring.
    pub fn failure(&self) -> Option<String> {
        if self.overflows.is_empty() {
            return None;
        }
        let blocks: Vec<String> = self.overflows.iter().map(|o| format!("{}: {} entries", o.block, o.entries)).collect();
        Some(format!(
            "{} never installed ({}, ring {})",
            crate::verdict::count(self.overflows.len(), "block", "blocks"),
            blocks.join(", "),
            self.ring
        ))
    }
}

/// Every block of `wads` against the largest CSUM `meta` among them. An error when a WAD cannot be read,
/// has no CSUM row, or a block's entry count cannot be read.
pub fn check_install_ring(wads: &[&Path]) -> Result<RingReport, String> {
    let mut opened = Vec::with_capacity(wads.len());
    for &path in wads {
        let mut file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let size = file.metadata().map_err(|e| format!("stat {}: {e}", path.display()))?.len();
        let archive = load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", path.display()))?;
        let meta = find_chunk(&archive.chunks, b"CSUM").ok_or_else(|| format!("{}: no CSUM chunk row", path.display()))?.meta;
        opened.push((path, file, archive, meta));
    }
    let (ring, ring_from) = opened
        .iter()
        .map(|(p, _, _, m)| (*m, p.to_path_buf()))
        .max_by_key(|(m, _)| *m)
        .ok_or("no WAD to check")?;
    let mut blocks = 0;
    let mut overflows = Vec::new();
    for (path, file, archive, _) in &mut opened {
        for i in 0..archive.indx.len() {
            let head = mercs2_formats::sges::decompress_block_head(file, &archive.indx, i as u16, 4)
                .map_err(|m| format!("{}: {m}", path.display()))?;
            let word = head.get(0..4).ok_or_else(|| format!("{}: block {i} decompresses to {} bytes", path.display(), head.len()))?;
            let entries = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            blocks += 1;
            if entries > ring {
                overflows.push(Overflow { wad: path.to_path_buf(), block: archive.paths.get(i).cloned().unwrap_or_else(|| format!("block {i}")), entries });
            }
        }
    }
    Ok(RingReport { ring, ring_from, blocks, overflows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::patch_wad::{build_patch_wad_multi, PatchBlock, FFCS_CERT_BLOB};

    fn wad_with(entries: u32, meta: u32, name: &str) -> PathBuf {
        let mut raw = entries.to_le_bytes().to_vec();
        raw.resize(64, 0x5A);
        let block = PatchBlock::from_decompressed(&raw, "blocks\\VZ\\resident_P000_Q3.block".into(), vec![], None).unwrap();
        let mut wad = build_patch_wad_multi(&[block], 0, &FFCS_CERT_BLOB).unwrap();
        wad[0x2C..0x30].copy_from_slice(&meta.to_le_bytes());
        let dir = std::env::temp_dir().join(format!("install_ring_{}_{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.wad"));
        std::fs::write(&path, wad).unwrap();
        path
    }

    /// A patch whose block outgrows every mounted meta is reported; another WAD of the stack with a
    /// larger meta (vz.wad's 7018 in the game) covers it.
    #[test]
    fn a_block_past_every_mounted_meta_is_reported() {
        let patch = wad_with(7020, 0, "patch");
        let report = check_install_ring(&[&patch]).unwrap();
        assert_eq!(report.ring, 0);
        assert_eq!(report.overflows, [Overflow { wad: patch.clone(), block: "blocks\\VZ\\resident_P000_Q3.block".into(), entries: 7020 }]);

        let base = wad_with(3, 7018, "base");
        let report = check_install_ring(&[&base, &patch]).unwrap();
        assert_eq!((report.ring, report.ring_from.clone()), (7018, base.clone()));
        assert_eq!(report.overflows.len(), 1, "7020 entries still outgrow a 7018 ring");

        let fixed = wad_with(7020, 7020, "fixed");
        let report = check_install_ring(&[&base, &fixed]).unwrap();
        assert!(report.overflows.is_empty(), "{:?}", report.overflows);
        assert_eq!(report.blocks, 2);
    }
}
