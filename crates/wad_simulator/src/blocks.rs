//! Parallel WAD block decompression and per-block UCFX container parse cache.
//!
//! This module handles the core I/O and decompression pipeline for WAD block processing:
//! 1. Discover unique blocks referenced by ASET entries
//! 2. Decompress SGES (Streaming Global Equipment System) blocks in parallel
//! 3. Parse UCFX (Universal Container Format eXtended) headers and entries
//! 4. Cache parsed blocks to avoid redundant decompression
//!
//! # Block Processing Stages
//!
//! ## Prefetch
//! [`prefetch_blocks_parallel`] decompresses all referenced blocks in parallel using Rayon,
//! storing results in a thread-safe cache. This stage is I/O and decompression bound.
//!
//! ## Parsing
//! [`parse_blocks_parallel`] parses cached decompressed blocks into [`ParsedBlock`] structures,
//! validating UCFX header structure and walking entry tables.
//!
//! # Block Key
//!
//! [`BlockKey`] uniquely identifies a block:
//! - `path`: WAD file path
//! - `block_idx`: Block index within the WAD
//! - `source`: Base or Patch WAD
//!
//! # Parallelism Strategy
//!
//! Uses Rayon's thread pool (configurable via `--jobs` CLI flag):
//! - Default: Auto-detect core count
//! - 0: Use rayon's default
//! - N: Explicit thread count
//!
//! # Usage
//!
//! ```no_run
//! use wad_simulator::blocks::{prefetch_blocks_parallel, collect_block_keys};
//! use std::path::Path;
//!
//! let entries = vec![/* resolved assets */];
//! let keys = collect_block_keys(&entries, Some(Path::new("base.wad")), Some(Path::new("patch.wad")));
//! let cache = prefetch_blocks_parallel(
//!     &keys,
//!     Some(Path::new("base.wad")),
//!     Some(Path::new("patch.wad")),
//!     8, // threads
//! ).expect("Prefetch failed");
//! ```

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

use crate::overlay::{AsetSource, ResolvedAset, VirtualDisk};
use crate::progress::{log, log_every};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{walk_decompressed_block, ParsedBlock, UcfxWalkIssue};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct BlockKey {
    pub path: String,
    pub block_idx: u16,
    pub source: AsetSource,
}

pub fn block_key_for_entry(
    entry: &ResolvedAset,
    base_wad: Option<&Path>,
    patch_wad: Option<&Path>,
) -> Option<BlockKey> {
    let path = match entry.source {
        AsetSource::Base => base_wad?.display().to_string(),
        AsetSource::Patch => patch_wad?.display().to_string(),
    };
    Some(BlockKey {
        path,
        block_idx: entry.block_index(),
        source: entry.source,
    })
}

pub fn collect_block_keys(
    entries: &[ResolvedAset],
    base_wad: Option<&Path>,
    patch_wad: Option<&Path>,
) -> Vec<BlockKey> {
    let mut set = HashSet::new();
    for entry in entries {
        if let Some(k) = block_key_for_entry(entry, base_wad, patch_wad) {
            set.insert(k);
        }
    }
    set.into_iter().collect()
}

fn decompress_one(
    key: &BlockKey,
    vd: &VirtualDisk,
) -> Result<Vec<u8>, String> {
    let mut file = File::open(&key.path).map_err(|e| format!("open {}: {e}", key.path))?;
    let indx = match key.source {
        AsetSource::Base => vd
            .base
            .as_ref()
            .map(|a| a.indx.as_slice())
            .ok_or("missing base INDX")?,
        AsetSource::Patch => vd
            .patch
            .as_ref()
            .map(|a| a.indx.as_slice())
            .ok_or("missing patch INDX")?,
    };
    decompress_block(&mut file, indx, key.block_idx)
}

/// Decompress all unique blocks in parallel; returns raw bytes per key.
pub fn prefetch_blocks_parallel(
    keys: Vec<BlockKey>,
    vd: &VirtualDisk,
    jobs: usize,
    progress_every: usize,
) -> HashMap<BlockKey, Result<Vec<u8>, String>> {
    let total = keys.len();
    if total == 0 {
        return HashMap::new();
    }

    let thread_count = if jobs == 0 {
        rayon::current_num_threads()
    } else {
        jobs
    };

    log(format!(
        "  Prefetch: decompressing {total} unique blocks on {thread_count} threads..."
    ));

    let done = AtomicUsize::new(0);
    let progress_every = progress_every.max(1);

    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .expect("thread pool");

    pool.install(|| {
        keys.par_iter()
            .map(|key| {
                let result = decompress_one(key, vd);

                // P2-8: packed_field page_count verification
                if let Ok(ref decompressed) = result {
                    let indx = match key.source {
                        AsetSource::Base => vd.base.as_ref().map(|a| a.indx.as_slice()),
                        AsetSource::Patch => vd.patch.as_ref().map(|a| a.indx.as_slice()),
                    };
                    if let Some(entries) = indx {
                        let idx = key.block_idx as usize;
                        if idx < entries.len() {
                            let actual_pages = entries[idx].decompressed_page_count() as usize;
                            let expected_pages = (decompressed.len() + 32767) / 32768;
                            if expected_pages != actual_pages && actual_pages > 0 {
                                let src = if matches!(key.source, AsetSource::Patch) { "patch" } else { "base" };
                                log(format!(
                                    "  [P2-8] {src} block[{}] page_count mismatch: \
                                     INDX says {actual_pages}, decompressed needs {expected_pages} \
                                     (len={})",
                                    key.block_idx,
                                    decompressed.len()
                                ));
                            }
                        }
                    }
                }

                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                log_every(n, progress_every, || {
                    format!("  Prefetch decompress: {n}/{total} blocks")
                });
                (key.clone(), result)
            })
            .collect()
    })
}

pub struct ParsedBlockCache {
    pub blocks: HashMap<BlockKey, ParsedBlock>,
    pub issues: HashMap<BlockKey, Vec<UcfxWalkIssue>>,
}

/// Walk UCFX once per decompressed block (parallel over successful decompressions).
pub fn parse_blocks_parallel(
    raw: &HashMap<BlockKey, Result<Vec<u8>, String>>,
    jobs: usize,
    progress_every: usize,
) -> ParsedBlockCache {
    let ok_keys: Vec<BlockKey> = raw
        .iter()
        .filter_map(|(k, v)| v.as_ref().ok().map(|_| k.clone()))
        .collect();
    let total = ok_keys.len();
    if total == 0 {
        return ParsedBlockCache {
            blocks: HashMap::new(),
            issues: HashMap::new(),
        };
    }

    let thread_count = if jobs == 0 {
        rayon::current_num_threads()
    } else {
        jobs
    };
    log(format!(
        "  Prefetch: parsing UCFX for {total} blocks on {thread_count} threads..."
    ));

    let done = AtomicUsize::new(0);
    let progress_every = progress_every.max(1);

    let pool = ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .expect("thread pool");

    let parsed: Vec<(BlockKey, ParsedBlock, Vec<UcfxWalkIssue>)> = pool.install(|| {
        ok_keys
            .par_iter()
            .map(|key| {
                let bytes = raw[key].as_ref().expect("ok key");
                let src = if matches!(key.source, AsetSource::Patch) { "patch" } else { "base" };
                let label = format!("{src} block[{}]", key.block_idx);
                let (parsed, mut issues) = walk_decompressed_block(bytes, &label);

                // P2-9: entry table sum(chunk_sizes) == data region
                let header_size = 4 + parsed.entry_count as usize * 16;
                let sum_chunks: usize = parsed.entries.iter().map(|e| e.chunk_size as usize).sum();
                let data_region = bytes.len().saturating_sub(header_size);
                if sum_chunks != data_region && !parsed.entries.is_empty() {
                    issues.push(UcfxWalkIssue {
                        context: label.clone(),
                        detail: format!(
                            "entry table sum(chunk_sizes)={sum_chunks} != data_region={data_region}"
                        ),
                    });
                }

                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                log_every(n, progress_every, || {
                    format!("  Prefetch parse: {n}/{total} blocks")
                });
                (key.clone(), parsed, issues)
            })
            .collect()
    });

    let mut blocks = HashMap::new();
    let mut issues = HashMap::new();
    for (k, p, iss) in parsed {
        blocks.insert(k.clone(), p);
        if !iss.is_empty() {
            issues.insert(k, iss);
        }
    }
    ParsedBlockCache { blocks, issues }
}

pub fn merge_block_issues(cache: &ParsedBlockCache, report_ucfx: &mut Vec<String>) {
    for issues in cache.issues.values() {
        for issue in issues {
            report_ucfx.push(format!("{}: {}", issue.context, issue.detail));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_key_equality() {
        let key1 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let key2 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        assert_eq!(key1, key2);
    }

    #[test]
    fn block_key_different_block_idx() {
        let key1 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let key2 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 6,
            source: AsetSource::Base,
        };
        assert_ne!(key1, key2);
    }

    #[test]
    fn block_key_different_source() {
        let key1 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let key2 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Patch,
        };
        assert_ne!(key1, key2);
    }

    #[test]
    fn block_key_different_path() {
        let key1 = BlockKey {
            path: "base.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let key2 = BlockKey {
            path: "patch.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        assert_ne!(key1, key2);
    }

    #[test]
    fn block_key_is_hash() {
        use std::collections::HashSet;
        let key1 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let key2 = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 5,
            source: AsetSource::Base,
        };
        let mut set = HashSet::new();
        set.insert(key1);
        assert!(set.contains(&key2));
    }

    #[test]
    fn collect_block_keys_empty_entries() {
        let entries = vec![];
        let keys = collect_block_keys(&entries, None, None);
        assert!(keys.is_empty());
    }

    #[test]
    fn collect_block_keys_filters_duplicates() {
        let entry1 = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,  // block=0, sub=1
            type_id: 10,
            source: AsetSource::Base,
        };
        let entry2 = ResolvedAset {
            asset_hash: 0x22222222,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,  // same block
            type_id: 20,
            source: AsetSource::Base,
        };

        let entries = vec![entry1, entry2];
        let keys = collect_block_keys(&entries, Some(std::path::Path::new("base.wad")), None);

        // Should have only 1 unique block key
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].block_idx, 0);
    }

    #[test]
    fn collect_block_keys_different_sources() {
        let entry1 = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,
            type_id: 10,
            source: AsetSource::Base,
        };
        let entry2 = ResolvedAset {
            asset_hash: 0x22222222,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,  // same block index but different source
            type_id: 20,
            source: AsetSource::Patch,
        };

        let entries = vec![entry1, entry2];
        let keys = collect_block_keys(
            &entries,
            Some(std::path::Path::new("base.wad")),
            Some(std::path::Path::new("patch.wad")),
        );

        // Should have 2 keys (different sources)
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn collect_block_keys_no_base_path() {
        let entry = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,
            type_id: 10,
            source: AsetSource::Base,
        };

        let entries = vec![entry];
        let keys = collect_block_keys(&entries, None, Some(std::path::Path::new("patch.wad")));

        // Base entry but no base path -> no key
        assert!(keys.is_empty());
    }

    #[test]
    fn collect_block_keys_no_patch_path() {
        let entry = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00000001,
            type_id: 10,
            source: AsetSource::Patch,
        };

        let entries = vec![entry];
        let keys = collect_block_keys(&entries, Some(std::path::Path::new("base.wad")), None);

        // Patch entry but no patch path -> no key
        assert!(keys.is_empty());
    }

    #[test]
    fn block_key_for_entry_base() {
        let entry = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00050005,
            type_id: 10,
            source: AsetSource::Base,
        };

        let key = block_key_for_entry(&entry, Some(std::path::Path::new("base.wad")), None);

        assert!(key.is_some());
        let k = key.unwrap();
        assert_eq!(k.block_idx, 5);
        assert_eq!(k.source, AsetSource::Base);
        assert!(k.path.contains("base.wad"));
    }

    #[test]
    fn block_key_for_entry_patch() {
        let entry = ResolvedAset {
            asset_hash: 0x22222222,
            secondary_ref: 0,
            packed_block_ref: 0x00030003,
            type_id: 20,
            source: AsetSource::Patch,
        };

        let key = block_key_for_entry(&entry, None, Some(std::path::Path::new("patch.wad")));

        assert!(key.is_some());
        let k = key.unwrap();
        assert_eq!(k.block_idx, 3);
        assert_eq!(k.source, AsetSource::Patch);
        assert!(k.path.contains("patch.wad"));
    }

    #[test]
    fn block_key_for_entry_missing_path() {
        let entry = ResolvedAset {
            asset_hash: 0x11111111,
            secondary_ref: 0,
            packed_block_ref: 0x00050005,
            type_id: 10,
            source: AsetSource::Base,
        };

        let key = block_key_for_entry(&entry, None, None);
        assert!(key.is_none());
    }

    #[test]
    fn parsed_block_cache_empty() {
        let cache = ParsedBlockCache {
            blocks: HashMap::new(),
            issues: HashMap::new(),
        };

        assert!(cache.blocks.is_empty());
        assert!(cache.issues.is_empty());
    }

    #[test]
    fn merge_block_issues_empty() {
        let cache = ParsedBlockCache {
            blocks: HashMap::new(),
            issues: HashMap::new(),
        };

        let mut report = Vec::new();
        merge_block_issues(&cache, &mut report);
        assert!(report.is_empty());
    }

    #[test]
    fn merge_block_issues_collects_all() {
        use mercs2_formats::ucfx::UcfxWalkIssue;

        let key = BlockKey {
            path: "test.wad".to_string(),
            block_idx: 0,
            source: AsetSource::Base,
        };

        let issues = vec![
            UcfxWalkIssue {
                context: "block[0]".to_string(),
                detail: "issue 1".to_string(),
            },
            UcfxWalkIssue {
                context: "block[0]".to_string(),
                detail: "issue 2".to_string(),
            },
        ];

        let mut issue_map = HashMap::new();
        issue_map.insert(key, issues);

        let cache = ParsedBlockCache {
            blocks: HashMap::new(),
            issues: issue_map,
        };

        let mut report = Vec::new();
        merge_block_issues(&cache, &mut report);

        assert_eq!(report.len(), 2);
        assert!(report[0].contains("issue 1"));
        assert!(report[1].contains("issue 2"));
    }

    #[test]
    fn collect_block_keys_multiple_blocks() {
        let entries = vec![
            ResolvedAset {
                asset_hash: 0x11111111,
                secondary_ref: 0,
                packed_block_ref: 0x00000001,
                type_id: 10,
                source: AsetSource::Base,
            },
            ResolvedAset {
                asset_hash: 0x22222222,
                secondary_ref: 0,
                packed_block_ref: 0x00010002,
                type_id: 20,
                source: AsetSource::Base,
            },
            ResolvedAset {
                asset_hash: 0x33333333,
                secondary_ref: 0,
                packed_block_ref: 0x00020003,
                type_id: 30,
                source: AsetSource::Base,
            },
        ];

        let keys = collect_block_keys(&entries, Some(std::path::Path::new("base.wad")), None);

        // 3 different block indices
        assert_eq!(keys.len(), 3);
    }
}

