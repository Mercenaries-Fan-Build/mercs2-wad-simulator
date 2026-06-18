//! Parallel WAD block decompression and per-block UCFX parse cache.

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
