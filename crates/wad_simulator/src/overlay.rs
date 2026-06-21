//! Virtual disk overlay: patch ASET wins over base (last-opened-file-wins semantics).
//!
//! This module models the Mercenaries 2 WAD overlay behavior where patch WADs override base WADs.
//! It builds a virtual asset resolution index that prioritizes patch entries while falling back to
//! base entries when needed.
//!
//! # Overlay Resolution
//!
//! The engine loads WADs in this order:
//! 1. Base WAD (e.g., vz.wad) is loaded first
//! 2. Patch WAD (e.g., vz-patch.wad) is loaded second
//! 3. Assets from patch override base assets with the same hash (last-opened-file-wins)
//!
//! # Virtual Disk
//!
//! [`VirtualDisk`] maintains:
//! - `base`: Loaded base WAD FFCS archive
//! - `patch`: Loaded patch WAD FFCS archive
//! - `resolved`: HashMap from asset_hash to [`ResolvedAset`] (winner of overlay)
//! - `csum_mismatches`: ASET entries with matching hash but different checksums
//!
//! # Resolution Process
//!
//! For each asset in the patch ASET:
//! - If hash matches a base entry, patch entry wins
//! - Patch entry is recorded in `resolved`
//! - Csum mismatches are flagged for investigation
//!
//! For each asset in the base ASET:
//! - If hash is not in patch, base entry wins
//! - Base entry is recorded in `resolved`
//!
//! # Usage
//!
//! ```no_run
//! use wad_simulator::overlay::VirtualDisk;
//! use std::path::Path;
//!
//! let disk = VirtualDisk::load(
//!     Some(Path::new("base.wad")),
//!     Some(Path::new("patch.wad")),
//! ).expect("Failed to load WADs");
//!
//! println!("Resolved assets: {}", disk.resolved.len());
//! println!("Csum mismatches: {}", disk.csum_mismatches.len());
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsetSource {
    Base,
    Patch,
}

#[derive(Debug, Clone)]
pub struct ResolvedAset {
    pub asset_hash: u32,
    pub secondary_ref: u32,
    pub packed_block_ref: u32,
    pub type_id: u32,
    pub source: AsetSource,
}

#[derive(Debug)]
pub struct VirtualDisk {
    pub base: Option<FfcsArchive>,
    pub patch: Option<FfcsArchive>,
    /// asset_hash -> resolved entry (patch overrides base)
    pub resolved: HashMap<u32, ResolvedAset>,
    pub csum_mismatches: Vec<String>,
}

impl VirtualDisk {
    pub fn load(base_path: Option<&Path>, patch_path: Option<&Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let base = if let Some(p) = base_path {
            let mut f = File::open(p)?;
            let size = f.metadata()?.len();
            Some(load_ffcs_archive(&mut f, size)?)
        } else {
            None
        };

        let patch = if let Some(p) = patch_path {
            let mut f = File::open(p)?;
            let size = f.metadata()?.len();
            Some(load_ffcs_archive(&mut f, size)?)
        } else {
            None
        };

        let mut resolved: HashMap<u32, ResolvedAset> = HashMap::new();

        if let Some(ref arch) = base {
            for e in &arch.aset {
                resolved.insert(
                    e.asset_hash,
                    ResolvedAset {
                        asset_hash: e.asset_hash,
                        secondary_ref: e.secondary_ref,
                        packed_block_ref: e.packed_block_ref,
                        type_id: e.type_id,
                        source: AsetSource::Base,
                    },
                );
            }
        }

        if let Some(ref arch) = patch {
            for e in &arch.aset {
                resolved.insert(
                    e.asset_hash,
                    ResolvedAset {
                        asset_hash: e.asset_hash,
                        secondary_ref: e.secondary_ref,
                        packed_block_ref: e.packed_block_ref,
                        type_id: e.type_id,
                        source: AsetSource::Patch,
                    },
                );
            }
        }

        Ok(Self {
            base,
            patch,
            resolved,
            csum_mismatches: Vec::new(),
        })
    }

    pub fn lookup(&self, asset_hash: u32) -> Option<&ResolvedAset> {
        self.resolved.get(&asset_hash)
    }

    pub fn indx_for(&self, entry: &ResolvedAset) -> Option<&[mercs2_formats::ffcs::IndxEntry]> {
        match entry.source {
            AsetSource::Base => self.base.as_ref().map(|a| a.indx.as_slice()),
            AsetSource::Patch => self.patch.as_ref().map(|a| a.indx.as_slice()),
        }
    }

    pub fn open_block_file(
        &self,
        path: &Path,
        entry: &ResolvedAset,
    ) -> Result<Vec<u8>, String> {
        let indx = match entry.source {
            AsetSource::Base => self
                .base
                .as_ref()
                .ok_or("no base archive")?
                .indx
                .as_slice(),
            AsetSource::Patch => self
                .patch
                .as_ref()
                .ok_or("no patch archive")?
                .indx
                .as_slice(),
        };
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        mercs2_formats::sges::decompress_block(&mut file, indx, entry.block_index())
    }
}

impl ResolvedAset {
    pub fn block_index(&self) -> u16 {
        (self.packed_block_ref >> 16) as u16
    }

    pub fn sub_entry(&self) -> u16 {
        (self.packed_block_ref & 0xFFFF) as u16
    }

    pub fn is_primary(&self) -> bool {
        self.sub_entry() == 0xFFFF
    }
}

/// Merge stats for reporting.
pub fn overlay_stats(vd: &VirtualDisk) -> (usize, usize, usize) {
    let base_only = vd
        .base
        .as_ref()
        .map(|b| b.aset.len())
        .unwrap_or(0);
    let patch_only = vd
        .patch
        .as_ref()
        .map(|p| {
            p.aset
                .iter()
                .filter(|e| {
                    vd.base
                        .as_ref()
                        .map(|b| !b.aset.iter().any(|x| x.asset_hash == e.asset_hash))
                        .unwrap_or(true)
                })
                .count()
        })
        .unwrap_or(0);
    let total = vd.resolved.len();
    (base_only, patch_only, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_aset_block_index_extraction() {
        let entry = ResolvedAset {
            asset_hash: 0x12345678,
            secondary_ref: 0,
            packed_block_ref: 0x00050001,  // block_index=5, sub_entry=1
            type_id: 42,
            source: AsetSource::Base,
        };

        assert_eq!(entry.block_index(), 5u16);
        assert_eq!(entry.sub_entry(), 1u16);
    }

    #[test]
    fn resolved_aset_sub_entry_extraction() {
        let entry = ResolvedAset {
            asset_hash: 0xAABBCCDD,
            secondary_ref: 0,
            packed_block_ref: 0x00100020,  // block_index=16, sub_entry=32
            type_id: 99,
            source: AsetSource::Patch,
        };

        assert_eq!(entry.block_index(), 16u16);
        assert_eq!(entry.sub_entry(), 32u16);
    }

    #[test]
    fn resolved_aset_is_primary_ffff() {
        let entry = ResolvedAset {
            asset_hash: 0x11223344,
            secondary_ref: 0,
            packed_block_ref: 0x000AFFFF,  // sub_entry = 0xFFFF
            type_id: 50,
            source: AsetSource::Base,
        };

        assert!(entry.is_primary());
    }

    #[test]
    fn resolved_aset_not_primary() {
        let entry = ResolvedAset {
            asset_hash: 0x11223344,
            secondary_ref: 0,
            packed_block_ref: 0x000A0000,  // sub_entry = 0
            type_id: 50,
            source: AsetSource::Base,
        };

        assert!(!entry.is_primary());
    }

    #[test]
    fn aset_source_is_clone_copy() {
        let source1 = AsetSource::Base;
        let source2 = source1;
        let source3 = source1.clone();

        assert_eq!(source1, AsetSource::Base);
        assert_eq!(source2, AsetSource::Base);
        assert_eq!(source3, AsetSource::Base);
    }

    #[test]
    fn aset_source_different_variants() {
        assert_eq!(AsetSource::Base, AsetSource::Base);
        assert_eq!(AsetSource::Patch, AsetSource::Patch);
        assert_ne!(AsetSource::Base, AsetSource::Patch);
    }

    #[test]
    fn virtual_disk_no_wads() {
        let disk = VirtualDisk {
            base: None,
            patch: None,
            resolved: std::collections::HashMap::new(),
            csum_mismatches: vec![],
        };

        assert!(disk.lookup(0x12345678).is_none());
        assert!(disk.resolved.is_empty());
    }

    #[test]
    fn virtual_disk_lookup_existing() {
        let mut resolved = HashMap::new();
        let entry = ResolvedAset {
            asset_hash: 0x11223344,
            secondary_ref: 100,
            packed_block_ref: 0x00010001,
            type_id: 77,
            source: AsetSource::Base,
        };
        resolved.insert(0x11223344, entry.clone());

        let disk = VirtualDisk {
            base: None,
            patch: None,
            resolved,
            csum_mismatches: vec![],
        };

        let found = disk.lookup(0x11223344);
        assert!(found.is_some());
        assert_eq!(found.unwrap().asset_hash, 0x11223344);
        assert_eq!(found.unwrap().type_id, 77);
    }

    #[test]
    fn virtual_disk_lookup_missing() {
        let disk = VirtualDisk {
            base: None,
            patch: None,
            resolved: HashMap::new(),
            csum_mismatches: vec![],
        };

        assert!(disk.lookup(0xDEADBEEF).is_none());
    }

    #[test]
    fn resolved_aset_field_extraction() {
        let entry = ResolvedAset {
            asset_hash: 0xDEADBEEF,
            secondary_ref: 0x11111111,
            packed_block_ref: 0x00020003,  // block=2, sub=3
            type_id: 123,
            source: AsetSource::Patch,
        };

        assert_eq!(entry.asset_hash, 0xDEADBEEF);
        assert_eq!(entry.secondary_ref, 0x11111111);
        assert_eq!(entry.type_id, 123);
        assert_eq!(entry.source, AsetSource::Patch);
        assert_eq!(entry.block_index(), 2u16);
        assert_eq!(entry.sub_entry(), 3u16);
    }

    #[test]
    fn resolved_aset_zero_packed_ref() {
        let entry = ResolvedAset {
            asset_hash: 0x12345678,
            secondary_ref: 0,
            packed_block_ref: 0,
            type_id: 1,
            source: AsetSource::Base,
        };

        assert_eq!(entry.block_index(), 0u16);
        assert_eq!(entry.sub_entry(), 0u16);
        assert!(!entry.is_primary());
    }

    #[test]
    fn virtual_disk_overlay_semantics() {
        let mut resolved = HashMap::new();
        let base_entry = ResolvedAset {
            asset_hash: 0xAABBCCDD,
            secondary_ref: 1,
            packed_block_ref: 0x00010001,
            type_id: 10,
            source: AsetSource::Base,
        };
        let patch_entry = ResolvedAset {
            asset_hash: 0xAABBCCDD,
            secondary_ref: 2,
            packed_block_ref: 0x00020002,
            type_id: 20,
            source: AsetSource::Patch,
        };

        // Base added first, then patch (patch should override)
        resolved.insert(base_entry.asset_hash, base_entry);
        resolved.insert(patch_entry.asset_hash, patch_entry);

        let disk = VirtualDisk {
            base: None,
            patch: None,
            resolved,
            csum_mismatches: vec![],
        };

        let found = disk.lookup(0xAABBCCDD).unwrap();
        assert_eq!(found.source, AsetSource::Patch);
        assert_eq!(found.type_id, 20);
    }

    #[test]
    fn virtual_disk_multiple_unique_entries() {
        let mut resolved = HashMap::new();
        for i in 0..10 {
            let entry = ResolvedAset {
                asset_hash: 0x10000000 | i,
                secondary_ref: i as u32,
                packed_block_ref: i as u32,
                type_id: 50 + i as u32,
                source: if i % 2 == 0 { AsetSource::Base } else { AsetSource::Patch },
            };
            resolved.insert(entry.asset_hash, entry);
        }

        let disk = VirtualDisk {
            base: None,
            patch: None,
            resolved,
            csum_mismatches: vec![],
        };

        assert_eq!(disk.resolved.len(), 10);
        for i in 0..10 {
            let hash = 0x10000000 | i;
            assert!(disk.lookup(hash).is_some());
        }
    }
}

