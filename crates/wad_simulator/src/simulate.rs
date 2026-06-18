//! End-to-end engine consumption simulation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use colored::*;

use crate::action_table::consume_action_table;
use crate::animation::consume_animation;
use crate::blocks::{
    block_key_for_entry, collect_block_keys, merge_block_issues, parse_blocks_parallel,
    prefetch_blocks_parallel, BlockKey, ParsedBlockCache,
};
use crate::audio::{
    consume_soundbank, consume_wavebank_with_options, LoadedWavebank, WavebankConsumeOptions,
};
use crate::audio::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_WAVEBANK};
use crate::consume::{consume_structural, ConsumeResult};
use crate::material::consume_material;
use crate::model::consume_model;
use crate::resident::{consume_fxdict, consume_watermap};
use crate::overlay::{overlay_stats, ResolvedAset, VirtualDisk};
use crate::placement::consume_layer;
use crate::names::RainbowTable;
use crate::progress::{log, log_every};
use crate::pws::audit_audios_dir;
use crate::script::consume_script;
use crate::texture::{consume_texture, texture_buffer_too_small};
use mercs2_formats::safe_slice::SafeSlice;
use mercs2_formats::types::{
    type_hash_for_type_id, type_name, TYPE_HASH_FX_DICTIONARY, TYPE_HASH_TEXTURE,
    TYPE_HASH_WATERMAP, TYPE_ID_ANIMATION, TYPE_ID_FX_DICTIONARY, TYPE_ID_LAYER,
    TYPE_ID_LOWRES_TERRAIN, TYPE_ID_MATERIAL_PARAMS, TYPE_ID_MODEL, TYPE_ID_SCRIPT,
    TYPE_ID_TERRAIN_MESH, TYPE_ID_TEXTURE, TYPE_ID_STANCE, TYPE_ID_WORLD_ENTITY_DATA,
};
use mercs2_formats::ucfx::{
    extract_chunk_body, extract_data_chunk, get_container_by_type_hash, ParsedBlock,
};

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct TypeStats {
    pub type_id: u32,
    pub type_name: String,
    pub consumed: usize,
    pub issues: usize,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct SimulateReport {
    pub access_violations: Vec<String>,
    pub decode_errors: Vec<String>,
    pub unresolved_hashes: Vec<String>,
    pub ucfx_issues: Vec<String>,
    pub wavebanks_loaded: usize,
    pub soundbanks_loaded: usize,
    pub overlay_total_aset: usize,
    pub assets_by_type: HashMap<u32, TypeStats>,
    pub total_assets_consumed: usize,
    pub xref_checks: usize,
    pub xref_unresolved: usize,
    pub pws_files_found: usize,
    pub pws_files_validated: usize,
    pub streaming_clips: usize,
    pub has_base_wad: bool,
    pub placements_checked: usize,
    pub position_violations: usize,
    pub flgs_placements_checked: usize,
    pub vertex_violations: usize,
    pub bounds_violations: usize,
    pub structural_violations: u32,
    pub ecs_float_violations: usize,
    /// FATAL — engine-accurate streaming buffer-too-small: a texture sub-resource
    /// whose BODY is shorter than the DXT mip chain the engine instantiates from
    /// the dimensions. This is the world-load livelock signal.
    pub texture_buffer_too_small: usize,
    /// Headline messages for `texture_buffer_too_small` (printed untruncated).
    pub texture_buffer_issues: Vec<String>,
    // --- Advisory (NON-fatal) — heuristic checks, excluded from the verdict ---
    pub vertex_advisory: usize,
    pub bounds_advisory: usize,
    pub structural_advisory: u32,
    pub position_advisory: usize,
}

pub struct SimulateOptions<'a> {
    pub audios_dir: Option<&'a Path>,
    /// Optional clip_hash → .pws filename map (from dlc_audio_manifest.json).
    pub clip_pws_map: Option<std::collections::HashMap<u32, String>>,
    pub skip_audio: bool,
    /// Skip pass-1 mesh/texture/layer consumption (audio + PWS only).
    pub audio_only: bool,
    /// Max non-audio assets to process (0 = unlimited).
    pub asset_limit: usize,
    /// How often to print pass-1 progress (every N assets).
    pub progress_interval: usize,
    /// Parallel threads for block prefetch (0 = auto).
    pub jobs: usize,
    /// Rainbow table for naming texture sub-resources in the buffer sweep.
    pub rainbow: Option<&'a RainbowTable>,
    /// Sibling base WADs (English/shell/Loading/…) whose ASET hashes resolve
    /// cross-references, so refs into other WADs don't false-report as unresolved.
    /// Their assets are NOT consumed — only their ASET hash set is loaded.
    pub aux_wads: Vec<std::path::PathBuf>,
}

impl Default for SimulateOptions<'_> {
    fn default() -> Self {
        Self {
            audios_dir: None,
            clip_pws_map: None,
            skip_audio: false,
            audio_only: false,
            asset_limit: 0,
            progress_interval: 100,
            jobs: 0,
            rainbow: None,
            aux_wads: Vec::new(),
        }
    }
}

/// Load only a WAD's ASET asset-hash set (no block decompression) for cross-ref
/// resolution against sibling base WADs.
fn load_aux_aset_hashes(path: &Path) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size)?;
    Ok(arch.aset.iter().map(|e| e.asset_hash).collect())
}

pub fn run_simulate(
    base_wad: Option<&Path>,
    patch_wad: Option<&Path>,
) -> Result<SimulateReport, Box<dyn std::error::Error>> {
    run_simulate_with_options(base_wad, patch_wad, SimulateOptions::default())
}

pub fn run_simulate_with_options(
    base_wad: Option<&Path>,
    patch_wad: Option<&Path>,
    opts: SimulateOptions<'_>,
) -> Result<SimulateReport, Box<dyn std::error::Error>> {
    log("  Loading virtual disk overlay...");
    if let Some(p) = base_wad {
        log(format!("    base: {}", p.display()));
    }
    if let Some(p) = patch_wad {
        log(format!("    patch: {}", p.display()));
    }
    let vd = VirtualDisk::load(base_wad, patch_wad)?;
    let (base_n, patch_n, total) = overlay_stats(&vd);
    log(format!(
        "  Overlay ready: {total} resolved ASET entries (base {base_n}, patch-only overrides {patch_n})"
    ));
    let mut report = SimulateReport {
        overlay_total_aset: total,
        has_base_wad: base_wad.is_some(),
        ..Default::default()
    };

    if let Some(dir) = opts.audios_dir {
        log(format!("  Auditing PWS files in {}...", dir.display()));
        let audit = audit_audios_dir(dir);
        log(format!(
            "  PWS audit: {}/{} files validated",
            audit.files_validated, audit.files_found
        ));
        report.pws_files_found = audit.files_found;
        report.pws_files_validated = audit.files_validated;
        for iss in audit.issues {
            report.ucfx_issues.push(format!("pws: {iss}"));
        }
    }

    let all_entries: Vec<_> = vd.resolved.values().cloned().collect();
    let mut wavebanks: HashMap<u32, LoadedWavebank> = HashMap::new();
    // xref hash → label of the first asset (model/material block) that referenced it,
    // so an unresolved ref names the source block/model instead of a bare hash.
    let mut xref_sources: HashMap<u32, String> = HashMap::new();
    // Texture sub-resources dispatched in Pass 1, keyed (block, name_hash), so the
    // post-pass full-block texture sweep doesn't re-report them.
    let mut dispatched_textures: HashSet<(BlockKey, u32)> = HashSet::new();
    let loaded_hashes: HashSet<u32> = vd.resolved.keys().copied().collect();

    // Sibling base WADs (English/shell/Loading) — ASET hashes only, for cross-ref
    // resolution. A ref into one of these is NOT a fault; the engine loads them too.
    let mut aux_aset_hashes: HashSet<u32> = HashSet::new();
    for p in &opts.aux_wads {
        match load_aux_aset_hashes(p) {
            Ok(hashes) => {
                log(format!("  Aux WAD: {} ({} ASET hashes for xref resolution)", p.display(), hashes.len()));
                aux_aset_hashes.extend(hashes);
            }
            Err(e) => log(format!("  Aux WAD: {} — load failed: {e}", p.display())),
        }
    }

    let progress_every = opts.progress_interval.max(1);
    let non_audio_total = all_entries
        .iter()
        .filter(|e| e.type_id != TYPE_ID_WAVEBANK && e.type_id != TYPE_ID_SOUNDBANK)
        .count();
    let audio_total = all_entries.len() - non_audio_total;

    let prefetch_entries = entries_for_prefetch(&all_entries, &opts);
    let block_keys = collect_block_keys(&prefetch_entries, base_wad, patch_wad);
    let raw_blocks = prefetch_blocks_parallel(block_keys, &vd, opts.jobs, progress_every);
    let parsed_cache = parse_blocks_parallel(&raw_blocks, opts.jobs, progress_every);
    merge_block_issues(&parsed_cache, &mut report.ucfx_issues);

    // Pass 1: non-audio asset consumption
    let mut asset_processed = 0usize;
    if !opts.audio_only {
        let limit_note = if opts.asset_limit > 0 {
            format!(" (limit {})", opts.asset_limit)
        } else {
            String::new()
        };
        log(format!(
            "  Pass 1: consuming up to {non_audio_total} non-audio assets{limit_note}..."
        ));
    }
    for entry in &all_entries {
        if opts.audio_only {
            break;
        }
        if opts.asset_limit > 0 && asset_processed >= opts.asset_limit {
            break;
        }
        if entry.type_id == TYPE_ID_WAVEBANK || entry.type_id == TYPE_ID_SOUNDBANK {
            continue;
        }
        let Some((parsed, label)) = get_parsed_for_entry(
            entry,
            base_wad,
            patch_wad,
            &raw_blocks,
            &parsed_cache,
            &mut report,
        ) else {
            continue;
        };

        let type_hash = resolve_type_hash(parsed, entry);
        let container = match get_container_by_type_hash(&parsed, type_hash, Some(entry.asset_hash))
        {
            Some(c) => c,
            None => continue,
        };

        let data_body = extract_data_chunk(&container);
        let result = dispatch_consume(
            entry.type_id,
            type_hash,
            entry.asset_hash,
            &container,
            data_body.as_deref(),
            &label,
        );
        record_type_stats(&mut report, entry.type_id, &result);
        report.placements_checked += result.placements_validated;
        report.flgs_placements_checked += result.flgs_placements_validated;
        report.vertex_violations += result.vertex_violations;
        report.bounds_violations += result.bounds_violations;
        report.structural_violations += result.structural_violations;
        report.ecs_float_violations += result.ecs_float_violations;
        report.vertex_advisory += result.vertex_advisory;
        report.bounds_advisory += result.bounds_advisory;
        report.structural_advisory += result.structural_advisory;
        report.position_advisory += result.position_advisory;
        // Buffer-too-small from consume_texture (ASET) + check_embedded_texture_buffers
        // (layer/model embedded) → the headline counter, same as the block sweep.
        for m in &result.texture_buffer_issues {
            report.texture_buffer_too_small += 1;
            report.texture_buffer_issues.push(format!("BUFFER_TOO_SMALL: {m}"));
        }
        for h in &result.xref_hashes {
            xref_sources.entry(*h).or_insert_with(|| label.clone());
        }
        for iss in &result.issues {
            // FATAL position violations come only from the verified 42-byte
            // Transform check (strings formatted `Transform[{i}] ...`). flgs
            // (heuristic stride) and ECS-schema position strings are advisory
            // (counted into position_advisory / ecs_float_violations).
            if iss.contains("Transform[")
                && (iss.contains("position NaN/Inf")
                    || iss.contains("position out of world bounds")
                    || iss.contains("quaternion NaN/Inf")
                    || iss.contains("quaternion not unit"))
            {
                report.position_violations += 1;
            }
            report.ucfx_issues.push(format!("{}: {iss}", label));
        }
        if entry.type_id == TYPE_ID_TEXTURE {
            if let Some(bk) = block_key_for_entry(entry, base_wad, patch_wad) {
                dispatched_textures.insert((bk, entry.asset_hash));
            }
        }
        report.total_assets_consumed += 1;
        asset_processed += 1;
        log_every(asset_processed, progress_every, || {
            format!(
                "  Pass 1: {asset_processed}/{non_audio_total} non-audio assets ({} blocks parsed)",
                parsed_cache.blocks.len()
            )
        });
    }
    if !opts.audio_only {
        log(format!(
            "  Pass 1 complete: {asset_processed} assets, {} blocks in cache",
            parsed_cache.blocks.len()
        ));
        // Texture buffer-too-small sweep: validate EVERY texture sub-resource in
        // every parsed block's entry table — including ones with no ASET entry,
        // which Pass 1 never dispatches. This is the world-load livelock site.
        log("  Texture sweep: scanning all parsed-block texture sub-resources for buffer-too-small...");
        sweep_texture_buffers(&parsed_cache, &dispatched_textures, opts.rainbow, &mut report);
        if report.texture_buffer_too_small > 0 {
            log(format!(
                "  Texture sweep: {} BUFFER_TOO_SMALL texture(s) found",
                report.texture_buffer_too_small
            ));
        }
    }

    // Pass 2: audio (wavebank then soundbank)
    if !opts.skip_audio {
        log(format!(
            "  Pass 2: loading {audio_total} audio assets (wavebank + soundbank)..."
        ));
        let wb_opts = WavebankConsumeOptions {
            audios_dir: opts.audios_dir,
            clip_pws_map: opts.clip_pws_map.as_ref(),
        };
        let audio_entries: Vec<_> = all_entries
            .iter()
            .filter(|e| e.type_id == TYPE_ID_WAVEBANK || e.type_id == TYPE_ID_SOUNDBANK)
            .collect();

        let mut audio_done = 0usize;
        for entry in audio_entries {
            let Some((parsed, label)) = get_parsed_for_entry(
                entry,
                base_wad,
                patch_wad,
                &raw_blocks,
                &parsed_cache,
                &mut report,
            ) else {
                continue;
            };

            let type_hash = if entry.type_id == TYPE_ID_WAVEBANK {
                TYPE_HASH_WAVEBANK
            } else {
                TYPE_HASH_SOUNDBANK
            };

            let container =
                match get_container_by_type_hash(&parsed, type_hash, Some(entry.asset_hash)) {
                    Some(c) => c,
                    None => continue,
                };

            let body_bytes = match extract_data_chunk(&container) {
                Some(b) => b,
                None => {
                    report.ucfx_issues.push(format!(
                        "{label}: no data chunk for type_hash 0x{type_hash:08X}"
                    ));
                    continue;
                }
            };

            let body = SafeSlice::new(body_bytes, format!("{label}/data"));

            if entry.type_id == TYPE_ID_WAVEBANK {
                match consume_wavebank_with_options(&body, wb_opts) {
                    Ok(wb) => {
                        report.streaming_clips += wb.streaming_clip_count;
                        for iss in &wb.issues {
                            report
                                .ucfx_issues
                                .push(format!("wavebank 0x{:08X}: {iss}", wb.self_hash));
                        }
                        wavebanks.insert(wb.self_hash, wb);
                        report.wavebanks_loaded += 1;
                        record_type_stats(
                            &mut report,
                            TYPE_ID_WAVEBANK,
                            &ConsumeResult {
                                consumed: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(crate::audio::wavebank::ConsumeError::Access(v)) => {
                        report.access_violations.push(v.to_string());
                    }
                    Err(crate::audio::wavebank::ConsumeError::Decode { clip_index, detail }) => {
                        report.decode_errors.push(format!(
                            "wavebank 0x{:08X} clip[{clip_index}]: {detail}",
                            entry.asset_hash
                        ));
                    }
                }
            } else {
                let resolve = |h: u32| -> bool {
                    wavebanks
                        .values()
                        .any(|wb| crate::audio::wavebank::clip_by_hash(wb, h).is_some())
                        || vd.lookup(h).is_some()
                };
                match consume_soundbank(&body, &resolve) {
                    Ok(sb) => {
                        for iss in &sb.issues {
                            report
                                .ucfx_issues
                                .push(format!("soundbank 0x{:08X}: {iss}", sb.self_hash));
                        }
                        if !sb.unresolved_hashes.is_empty() {
                            for h in &sb.unresolved_hashes {
                                report
                                    .unresolved_hashes
                                    .push(format!("0x{h:08X} (soundbank)"));
                            }
                        }
                        report.soundbanks_loaded += 1;
                        record_type_stats(
                            &mut report,
                            TYPE_ID_SOUNDBANK,
                            &ConsumeResult {
                                consumed: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(v) => report.access_violations.push(v.to_string()),
                }
            }
            report.total_assets_consumed += 1;
            audio_done += 1;
            log_every(audio_done, progress_every.min(10), || {
                format!(
                    "  Pass 2: {audio_done}/{audio_total} audio assets ({} wavebanks, {} soundbanks)",
                    report.wavebanks_loaded, report.soundbanks_loaded
                )
            });
        }
        log(format!(
            "  Pass 2 complete: {} wavebanks, {} soundbanks",
            report.wavebanks_loaded, report.soundbanks_loaded
        ));
    }

    // Pass 3: cross-reference resolution (placement/model/texture refs → ASET)
    if !xref_sources.is_empty() {
        log(format!(
            "  Pass 3: checking {} cross-references...",
            xref_sources.len()
        ));
    }
    // The engine resolves a ref to ANY loaded resource, not just top-level ASET
    // assets: a MTRL/model ref commonly names a texture EMBEDDED in a block's own
    // entry table (resolved there, not via ASET). So a hash is "present" if it is a
    // top-level ASET asset (base/patch/aux) OR the name_hash of any parsed block's
    // entry. Without this the validator false-reports every embedded sub-resource.
    let block_internal_hashes: HashSet<u32> = parsed_cache
        .blocks
        .values()
        .flat_map(|p| p.entries.iter().map(|e| e.name_hash))
        .collect();
    for (h, source) in &xref_sources {
        report.xref_checks += 1;
        if !loaded_hashes.contains(h)
            && !aux_aset_hashes.contains(h)
            && !block_internal_hashes.contains(h)
        {
            report.xref_unresolved += 1;
            // Name the referencing model/block so a corrupt MTRL hash array is
            // traceable to its source (the {source} is the asset's "block[N] hash=…").
            report
                .unresolved_hashes
                .push(format!("0x{h:08X} (xref from {source})"));
        }
    }

    if report.xref_checks > 0 {
        log(format!(
            "  Pass 3 complete: {} resolved, {} unresolved",
            report.xref_checks - report.xref_unresolved,
            report.xref_unresolved
        ));
    }

    Ok(report)
}

/// Engine-accurate buffer-too-small sweep over EVERY parsed block.
///
/// The engine's per-sub-resource create (worker thread) instantiates the full DXT
/// mip chain from the texture's DIMENSIONS (`dxt_mip_count` down to 4x4) and reads
/// that many bytes from BODY; a BODY shorter than that chain over-reads →
/// `STATUS_BUFFER_TOO_SMALL` → the `STATE_WAITFORSTREAMING` world-load livelock.
/// Pass 1 only dispatches ASET-referenced containers, so a texture that is its own
/// entry-table row but has no ASET entry (incl. the converter's Python-path
/// ECS-layer-embedded textures) is never checked. This walks every block's entry
/// table and validates each `TYPE_HASH_TEXTURE` sub-resource, deduping against the
/// ones Pass 1 already covered.
fn sweep_texture_buffers(
    parsed_cache: &ParsedBlockCache,
    dispatched: &HashSet<(BlockKey, u32)>,
    rainbow: Option<&RainbowTable>,
    report: &mut SimulateReport,
) {
    for (key, parsed) in &parsed_cache.blocks {
        for (i, entry) in parsed.entries.iter().enumerate() {
            if entry.type_hash != TYPE_HASH_TEXTURE {
                continue;
            }
            if dispatched.contains(&(key.clone(), entry.name_hash)) {
                continue; // already validated by Pass 1's consume_texture
            }
            let Some(container) = parsed.containers.get(i) else {
                continue;
            };
            let Some(info) = extract_chunk_body(container, b"INFO") else {
                continue;
            };
            // Same body precedence as consume_texture (texture.rs).
            let Some(body) = extract_chunk_body(container, b"BODY")
                .or_else(|| extract_chunk_body(container, b"DXT1"))
                .or_else(|| extract_chunk_body(container, b"data"))
            else {
                continue;
            };
            let label = match rainbow.and_then(|rt| rt.resolve(entry.name_hash)) {
                Some(name) => format!(
                    "texture 0x{:08X} ({name}) [block {} entry {i}]",
                    entry.name_hash, key.block_idx
                ),
                None => format!(
                    "texture 0x{:08X} [block {} entry {i}]",
                    entry.name_hash, key.block_idx
                ),
            };
            if let Some(msg) = texture_buffer_too_small(&info, body.len(), &label) {
                report.texture_buffer_too_small += 1;
                report
                    .texture_buffer_issues
                    .push(format!("BUFFER_TOO_SMALL: {msg}"));
            }
        }
    }
}

/// Entries whose blocks we prefetch before consumption passes.
fn entries_for_prefetch(
    all_entries: &[ResolvedAset],
    opts: &SimulateOptions<'_>,
) -> Vec<ResolvedAset> {
    let mut out = Vec::new();
    if !opts.audio_only {
        let mut n = 0usize;
        for entry in all_entries {
            if entry.type_id == TYPE_ID_WAVEBANK || entry.type_id == TYPE_ID_SOUNDBANK {
                continue;
            }
            if opts.asset_limit > 0 && n >= opts.asset_limit {
                break;
            }
            out.push(entry.clone());
            n += 1;
        }
    }
    if !opts.skip_audio {
        for entry in all_entries {
            if entry.type_id == TYPE_ID_WAVEBANK || entry.type_id == TYPE_ID_SOUNDBANK {
                out.push(entry.clone());
            }
        }
    }
    out
}

fn get_parsed_for_entry<'a>(
    entry: &ResolvedAset,
    base_wad: Option<&Path>,
    patch_wad: Option<&Path>,
    raw_blocks: &HashMap<BlockKey, Result<Vec<u8>, String>>,
    parsed_cache: &'a ParsedBlockCache,
    report: &mut SimulateReport,
) -> Option<(&'a ParsedBlock, String)> {
    let key = block_key_for_entry(entry, base_wad, patch_wad)?;
    let block_idx = key.block_idx;

    if let Some(Err(e)) = raw_blocks.get(&key) {
        report
            .access_violations
            .push(format!("block {block_idx} decompress: {e}"));
        return None;
    }

    let parsed = parsed_cache.blocks.get(&key)?;
    let label = format!("block[{block_idx}] hash=0x{:08X}", entry.asset_hash);
    Some((parsed, label))
}

fn resolve_type_hash(parsed: &ParsedBlock, entry: &ResolvedAset) -> u32 {
    for e in &parsed.entries {
        if e.name_hash == entry.asset_hash {
            return e.type_hash;
        }
    }
    type_hash_for_type_id(entry.type_id).unwrap_or(0)
}

fn dispatch_consume(
    type_id: u32,
    type_hash: u32,
    asset_hash: u32,
    container: &[u8],
    data_body: Option<&[u8]>,
    label: &str,
) -> ConsumeResult {
    match type_hash {
        TYPE_HASH_WATERMAP => return consume_watermap(container, data_body, label),
        TYPE_HASH_FX_DICTIONARY => return consume_fxdict(container, data_body, label),
        _ => {}
    }
    match type_id {
        TYPE_ID_MODEL | TYPE_ID_LOWRES_TERRAIN | TYPE_ID_TERRAIN_MESH => {
            consume_model(container, data_body, label)
        }
        TYPE_ID_TEXTURE => consume_texture(container, data_body, label),
        TYPE_ID_LAYER => consume_layer(container, data_body, label),
        TYPE_ID_SCRIPT => consume_script(container, data_body, label),
        TYPE_ID_ANIMATION => consume_animation(container, data_body, label),
        TYPE_ID_STANCE => consume_action_table(asset_hash, container, data_body, label),
        TYPE_ID_MATERIAL_PARAMS => consume_material(container, data_body, label),
        TYPE_ID_FX_DICTIONARY => consume_fxdict(container, data_body, label),
        TYPE_ID_WORLD_ENTITY_DATA => consume_structural(container, data_body, label),
        _ => consume_structural(container, data_body, label),
    }
}

fn record_type_stats(report: &mut SimulateReport, type_id: u32, result: &ConsumeResult) {
    let stats = report
        .assets_by_type
        .entry(type_id)
        .or_insert_with(|| TypeStats {
            type_id,
            type_name: type_name(type_id).to_string(),
            ..Default::default()
        });
    if result.consumed {
        stats.consumed += 1;
    }
    stats.issues += result.issues.len();
}

pub fn print_simulate_report(report: &SimulateReport, rainbow: Option<&crate::names::RainbowTable>) {
    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║              ENGINE CONSUMPTION SIMULATION                   ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();
    println!(
        "  Overlay ASET entries: {}",
        report.overlay_total_aset.to_string().bright_white()
    );
    println!(
        "  Assets consumed:      {}",
        report.total_assets_consumed.to_string().bright_white()
    );
    println!(
        "  Wavebanks loaded:     {}",
        report.wavebanks_loaded.to_string().bright_white()
    );
    println!(
        "  Soundbanks loaded:    {}",
        report.soundbanks_loaded.to_string().bright_white()
    );
    if report.streaming_clips > 0 {
        println!(
            "  Streaming clips:      {}",
            report.streaming_clips.to_string().bright_white()
        );
    }
    if report.pws_files_found > 0 {
        println!(
            "  PWS files:            {} validated / {}",
            report.pws_files_validated.to_string().bright_white(),
            report.pws_files_found
        );
    }
    if report.xref_checks > 0 {
        println!(
            "  Cross-refs:           {} resolved, {} unresolved",
            (report.xref_checks - report.xref_unresolved).to_string().bright_white(),
            report.xref_unresolved
        );
    }
    if report.placements_checked > 0 || report.position_violations > 0 {
        println!(
            "  Placements checked:   {}",
            report.placements_checked.to_string().bright_white()
        );
        if report.position_violations > 0 {
            println!(
                "  Position violations:  {}",
                report.position_violations.to_string().red().bold()
            );
        } else {
            println!(
                "  Position violations:  {}",
                "0".green()
            );
        }
    }
    if report.flgs_placements_checked > 0 {
        println!(
            "  Flgs placements:      {}",
            report.flgs_placements_checked.to_string().bright_white()
        );
    }
    if report.vertex_violations > 0 {
        println!(
            "  Vertex violations:    {}",
            report.vertex_violations.to_string().red().bold()
        );
    }
    if report.bounds_violations > 0 {
        println!(
            "  Bounds violations:    {}",
            report.bounds_violations.to_string().red().bold()
        );
    }
    if report.structural_violations > 0 {
        println!(
            "  Structural violations:{}",
            report.structural_violations.to_string().red().bold()
        );
    }
    if report.ecs_float_violations > 0 {
        println!(
            "  ECS float (advisory): {}  (schema-driven non-Transform Vec3/Blob32; diff vs retail oracle — not fatal)",
            report.ecs_float_violations.to_string().yellow()
        );
    }
    // Heuristic checks (unverified offsets/strides) — advisory, excluded from the
    // verdict. They false-positive on WADs that load fine in-game.
    for (name, n) in [
        ("Vertex (advisory, heuristic)    ", report.vertex_advisory),
        ("Bounds (advisory, heuristic)    ", report.bounds_advisory),
        ("Structural (advisory, heuristic)", report.structural_advisory as usize),
        ("flgs pos (advisory, heuristic)  ", report.position_advisory),
    ] {
        if n > 0 {
            println!("  {name}: {}  (not fatal)", n.to_string().yellow());
        }
    }
    println!();

    if !report.assets_by_type.is_empty() {
        println!("  {}", "ASSETS BY TYPE:".bright_white().bold());
        let mut types: Vec<_> = report.assets_by_type.values().collect();
        types.sort_by_key(|t| t.type_id);
        for t in types.iter().take(20) {
            println!(
                "    type_id {:2} {:16} consumed={:5} issues={}",
                t.type_id, t.type_name, t.consumed, t.issues
            );
        }
        if types.len() > 20 {
            println!("    ... and {} more types", types.len() - 20);
        }
        println!();
    }

    let xref_fatal = report.has_base_wad && !report.unresolved_hashes.is_empty();
    let has_issues = !report.access_violations.is_empty()
        || !report.decode_errors.is_empty()
        || report.texture_buffer_too_small > 0
        || report.position_violations > 0
        || report.vertex_violations > 0
        || report.bounds_violations > 0
        || report.structural_violations > 0
        // NOTE: ecs_float_violations is intentionally NOT fatal — without per-field
        // world-position semantics it false-positives on retail-valid non-Transform
        // Vec3/Blob32 fields (e.g. Road@0x0 ref data). Use it differentially vs a
        // retail oracle (tools/diff_ecs_violations.py) to find DLC-specific deltas.
        || xref_fatal;

    if !report.access_violations.is_empty() {
        println!(
            "  {} {}",
            "ACCESS VIOLATIONS:".red().bold(),
            report.access_violations.len()
        );
        for v in report.access_violations.iter().take(20) {
            println!("    {}", v.red());
        }
        if report.access_violations.len() > 20 {
            println!("    ... and {} more", report.access_violations.len() - 20);
        }
        println!();
    }

    if !report.decode_errors.is_empty() {
        println!(
            "  {} {}",
            "DECODE ERRORS:".red().bold(),
            report.decode_errors.len()
        );
        for e in report.decode_errors.iter().take(20) {
            println!("    {}", e.red());
        }
        if report.decode_errors.len() > 20 {
            println!("    ... and {} more", report.decode_errors.len() - 20);
        }
        println!();
    }

    if !report.unresolved_hashes.is_empty() {
        if report.has_base_wad {
            println!(
                "  {} {}",
                "UNRESOLVED HASHES:".red().bold(),
                report.unresolved_hashes.len()
            );
        } else {
            println!(
                "  {} {} (no --base-wad; these likely resolve in vz.wad)",
                "UNRESOLVED HASHES:".yellow().bold(),
                report.unresolved_hashes.len()
            );
        }
        for h_str in report.unresolved_hashes.iter().take(15) {
            let annotated = if let Some(rt) = rainbow {
                if let Some(hex_part) = h_str.split_whitespace().next() {
                    let hex_clean = hex_part.trim_start_matches("0x");
                    if let Ok(val) = u32::from_str_radix(hex_clean, 16) {
                        if let Some(name) = rt.resolve(val) {
                            format!("{h_str} → {name}")
                        } else {
                            h_str.clone()
                        }
                    } else {
                        h_str.clone()
                    }
                } else {
                    h_str.clone()
                }
            } else {
                h_str.clone()
            };
            println!("    {}", annotated.yellow());
        }
        if report.unresolved_hashes.len() > 15 {
            println!("    ... and {} more", report.unresolved_hashes.len() - 15);
        }
        println!();
    }

    if report.texture_buffer_too_small > 0 {
        println!(
            "  {} {} texture sub-resource(s) — the world-load streaming livelock",
            "BUFFER_TOO_SMALL:".red().bold(),
            report.texture_buffer_too_small
        );
        // Headline — print every one untruncated; this is the fix target.
        for m in &report.texture_buffer_issues {
            println!("    {}", m.red());
        }
        println!();
    }

    if !report.ucfx_issues.is_empty() {
        println!(
            "  {} {}",
            "UCFX / FORMAT:".yellow().bold(),
            report.ucfx_issues.len()
        );
        for u in report.ucfx_issues.iter().take(15) {
            println!("    {}", u.dimmed());
        }
        if report.ucfx_issues.len() > 15 {
            println!("    ... and {} more", report.ucfx_issues.len() - 15);
        }
        println!();
    }

    if has_issues {
        println!(
            "  {} Engine would likely fault or misbehave loading this WAD.",
            "VERDICT:".red().bold()
        );
    } else {
        println!(
            "  {} Full consumption path completed without violations.",
            "VERDICT:".green().bold()
        );
    }
    println!();
}

pub fn simulate_exit_code(report: &SimulateReport) -> i32 {
    let has_fatal_ucfx = report.ucfx_issues.iter().any(|u| {
        u.contains("codec 0x05")
            || u.contains("codec 0x01")
            || u.contains("XMA")
            || u.contains("streaming clip")
    });
    let xref_fatal = report.has_base_wad && !report.unresolved_hashes.is_empty();
    if report.access_violations.is_empty()
        && report.decode_errors.is_empty()
        && report.texture_buffer_too_small == 0
        && report.position_violations == 0
        && report.vertex_violations == 0
        && report.bounds_violations == 0
        && report.structural_violations == 0
        && !xref_fatal
        && !has_fatal_ucfx
    {
        0
    } else {
        1
    }
}

/// Load clip_hash → pws filename from manifest JSON (first candidate per clip).
pub fn load_clip_pws_map(manifest_path: &Path) -> Option<std::collections::HashMap<u32, String>> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let clips = root.get("clips")?.as_array()?;
    let mut map = std::collections::HashMap::new();
    for clip in clips {
        let hash = clip.get("clip_hash")?.as_u64()? as u32;
        if let Some(cands) = clip.get("pws_candidates").and_then(|c| c.as_array()) {
            if let Some(first) = cands.first().and_then(|v| v.as_str()) {
                map.entry(hash).or_insert_with(|| first.to_string());
            }
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}
