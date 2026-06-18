//! Virtual disk overlay: patch ASET wins over base (last-opened-file-wins).

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
