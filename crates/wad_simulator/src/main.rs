//! Mercenaries 2 WAD engine consumption simulator.
//!
//! Walks the same load path the game engine does over a WAD (optionally overlaid on a base WAD):
//! validates ASET hash ownership, decompresses and parses every referenced block, dispatches each
//! asset to a consumer mirroring what the engine's handler for that type actually reads, and
//! aggregates the findings into a JSON-exportable report.
//!
//! # Key Concepts
//!
//! - **ASET**: Asset Set section of a WAD; array of 16-byte asset metadata rows
//!   `{ asset_hash, secondary_ref, packed_ref, type_id }`, `packed_ref = { block_index:hi16,
//!   sub_offset:lo16 }`. `sub_offset == 0xFFFF` = PRIMARY (resolve by hash in `block_index`);
//!   otherwise `sub_offset` is the BYTE OFFSET of the asset's sub-resource descriptor within the
//!   decompressed block — NOT an index into the entry table (see `aset_validate`).
//! - **Hash ownership**: the authoritative ASET check — does each row's `asset_hash` actually exist
//!   in the block the row claims? Rows are verified / misrouted (hash lives elsewhere, remappable)
//!   / true ghost (hash in no block). This REPLACED the old "OOB" validator (see below).
//! - **WAD Overlay**: Patch WAD entries override base WAD entries (last-opened-file-wins semantics)
//! - **Block**: SGES compressed container; decompressed into UCFX asset format
//! - **UCFX Container**: Asset format wrapper with header and typed chunks
//! - **Fatal vs advisory**: only fatal findings drive the exit code (see
//!   `simulate::simulate_exit_code`); the heuristic `*_advisory`, `needs_investigation` and
//!   `dlc_texture_provenance` results are reported but excluded from the verdict.
//!
//! # Typical Usage
//!
//! ```bash
//! wad_simulator \
//!   --wad patch.wad \
//!   --base-wad base.wad \
//!   --base-wad-dir game_data/ \
//!   --json-output report.json
//! ```
//!
//! Exit code 0 = no fatal finding, 1 = fatal finding (or a pass failed outright).
//!
//! # Simulation Stages
//!
//! 1. Load base and patch WADs via FFCS archive interface
//! 2. Build virtual disk with overlay resolution (patch > base)
//! 3. Validate ASET hash ownership (`--skip-aset` to skip)
//! 4. Discover and load auxiliary base WADs for cross-ref resolution (`--base-wad-dir`, optional;
//!    their ASET hash sets are loaded, their assets are NOT consumed)
//! 5. Prefetch and decompress SGES blocks in parallel (rayon, `--jobs`)
//! 6. Parse UCFX containers and dispatch to type-specific consumers
//! 7. Aggregate diagnostic results and export report
//!
//! # Module Map
//!
//! - `aset_validate` — ASET hash-ownership pass (verified / misrouted / true ghost)
//! - `overlay` — virtual disk; patch ASET wins over base
//! - `blocks` — parallel SGES decompression + per-block UCFX parse cache
//! - `simulate` — pipeline orchestration, `SimulateReport`, exit-code verdict
//! - `consume` — per-asset-type consumer trait and result aggregation
//! - `chunk_invariants` — exe-derived structural invariants run on every UCFX chunk
//! - `model` / `texture` / `material` / `animation` / `script` — per-type consumers
//! - `placement` — layer/ECS_NODE Transform + `flgs` vz_state placement validation
//! - `action_table` — ActionTable 1024-slot overflow check (the world-load livelock)
//! - `resident` — resident singletons (watermap, fxdict)
//! - `audio` / `pws` — wavebank + soundbank consumption; external `.pws` streaming audit
//! - `names` — rainbow-table hash → name resolver
//!
//! # Removed: the "OOB" validator
//!
//! Earlier versions led with an ASET out-of-bounds pass (`run_aset_oob`) that read `packed_ref`'s
//! low 16 bits as an entry-table INDEX and flagged `sub >= entry_count` as heap corruption. RE of
//! retail `vz.wad` showed the low 16 bits are a byte offset: the old model held for 10 of 10,798
//! non-primary entries, i.e. it false-flagged ~10,788 valid rows. It has been removed. The
//! `--oob-only` flag it drove is retained on the CLI for compatibility but is no longer read.

mod action_table;
mod animation;
mod aset_validate;
mod audio;
mod blocks;
mod chunk_invariants;
mod consume;
mod material;
mod model;
mod resident;
pub mod names;
mod overlay;
mod placement;
mod progress;
mod pws;
mod simulate;
mod script;
mod texture;

use clap::Parser;
use colored::*;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "wad_simulator")]
#[command(about = "Engine-accurate WAD consumption simulator (ASET + full asset load path)")]
struct Cli {
    /// Primary WAD (patch or single-WAD analysis)
    #[arg(long, default_value = "output/data/vz-patch.wad")]
    wad: PathBuf,

    /// Base game WAD for overlay simulation (vz.wad)
    #[arg(long)]
    base_wad: Option<PathBuf>,

    /// External streaming audio directory (PC Data/Audios)
    #[arg(long)]
    audios_dir: Option<PathBuf>,

    /// NO-OP (retained for compatibility). The ASET OOB validator this drove was removed:
    /// it read the packed-ref low-16 as an entry-table index and false-flagged ~10,788 of
    /// 10,798 valid retail entries. Hash-ownership validation replaces it and always runs.
    #[arg(long, default_value_t = false)]
    oob_only: bool,

    /// Max ASET rows to validate in the hash-ownership pass (0 = all)
    #[arg(long, default_value_t = 0)]
    limit: usize,

    #[arg(long, default_value_t = false)]
    skip_aset: bool,

    #[arg(long, default_value_t = false)]
    skip_audio: bool,

    /// Only run audio + PWS validation (skip mesh/texture/layer scan)
    #[arg(long, default_value_t = false)]
    audio_only: bool,

    /// Max non-audio assets to consume (0 = all)
    #[arg(long, default_value_t = 0)]
    asset_limit: usize,

    /// Progress log interval for asset/block steps (default 100)
    #[arg(long, default_value_t = 100)]
    progress_interval: usize,

    /// Parallel worker threads for block prefetch (0 = auto)
    #[arg(long, default_value_t = 0)]
    jobs: usize,

    /// Skip full asset consumption (ASET-only mode)
    #[arg(long, default_value_t = false)]
    skip_assets: bool,

    /// Write simulation report as JSON to path
    #[arg(long)]
    json_output: Option<PathBuf>,

    /// Path to dlc_audio_manifest.json for streaming clip → .pws mapping
    #[arg(long)]
    audio_manifest: Option<PathBuf>,

    /// Path to rainbow_table.json for annotating unresolved hashes with asset names
    #[arg(long)]
    rainbow_table: Option<PathBuf>,

    /// Directory of the game's WADs (e.g. the install `data/` dir). Every non-patch
    /// WAD found here (English/shell/Loading/vz) has its ASET loaded for cross-ref
    /// resolution, so refs into sibling WADs don't false-report as unresolved. The
    /// patch (`--wad`) and the primary base (`--base-wad`) are skipped (not reloaded).
    #[arg(long)]
    base_wad_dir: Option<PathBuf>,
}

/// Discover sibling base WADs in a game `data/` dir: every `*.wad` except the patch
/// (`--wad`), the primary base (`--base-wad`), and anything whose name contains
/// "patch" (overlay WADs are not base resolution sources).
fn discover_aux_wads(dir: &std::path::Path, patch: &std::path::Path, primary_base: Option<&std::path::Path>) -> Vec<PathBuf> {
    let patch_name = patch.file_name();
    let base_name = primary_base.and_then(|p| p.file_name());
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        eprintln!("WARNING: --base-wad-dir {} not readable", dir.display());
        return out;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) != Some("wad") {
            continue;
        }
        let name = p.file_name();
        if name == patch_name || (base_name.is_some() && name == base_name) {
            continue;
        }
        let lname = name.and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
        if lname.contains("patch") {
            continue;
        }
        out.push(p);
    }
    out.sort();
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║   Mercenaries 2 WAD Engine Consumption Simulator           ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();
    println!("WAD: {}", cli.wad.display().to_string().yellow());
    if let Some(ref base) = cli.base_wad {
        println!("Base WAD: {}", base.display().to_string().yellow());
    }
    if let Some(ref audios) = cli.audios_dir {
        println!("Audios: {}", audios.display().to_string().yellow());
    }
    println!();

    let rainbow = cli.rainbow_table.as_ref().and_then(|p| {
        match names::RainbowTable::load(p) {
            Ok(rt) => {
                println!("Rainbow table: {} entries from {}", rt.len(), p.display());
                Some(rt)
            }
            Err(e) => {
                eprintln!("WARNING: failed to load rainbow table: {e}");
                None
            }
        }
    });

    let mut exit_code = 0i32;

    if !cli.skip_aset {
        // NOTE: the former "ASET OOB Validation" pass (run_aset_oob) was removed.
        // It treated the packed-ref low-16 as a 16-byte-entry-table INDEX and flagged
        // `sub >= entry_count` as "heap corruption", which false-positived on 10,798
        // retail entries. RE of retail vz.wad shows the low-16 is `sub_offset` — a
        // byte offset to the asset's sub-resource within the block, not a table index
        // (the asset resolves by hash in its block: 100% of non-primary entries verify
        // below). Correct validation is hash-ownership, which already runs here.
        println!(
            "{}",
            "=== ASET Hash Ownership Validation ===".bright_white().bold()
        );
        match aset_validate::run_aset_hash_validation(&cli.wad, cli.limit) {
            Ok(stats) => {
                aset_validate::print_hash_validation_summary(&stats);
                if stats.misrouted > 0 || stats.true_ghost > 0 {
                    exit_code = 1;
                }
            }
            Err(e) => {
                eprintln!("ASET hash validation failed: {e}");
                exit_code = 1;
            }
        }
        println!();
    }

    if !cli.skip_assets {
        println!(
            "{}",
            "=== Engine Asset Consumption ===".bright_white().bold()
        );
        let base = cli.base_wad.as_deref();
        let patch = Some(cli.wad.as_path());
        let aux_wads = cli
            .base_wad_dir
            .as_ref()
            .map(|d| discover_aux_wads(d, &cli.wad, base))
            .unwrap_or_default();
        let manifest_path = cli.audio_manifest.clone().or_else(|| {
            Some(PathBuf::from("output/analysis/dlc_audio_manifest.json"))
        });
        let clip_pws_map = manifest_path
            .as_ref()
            .and_then(|p| simulate::load_clip_pws_map(p));
        let opts = simulate::SimulateOptions {
            audios_dir: cli.audios_dir.as_deref(),
            clip_pws_map,
            skip_audio: cli.skip_audio,
            audio_only: cli.audio_only,
            asset_limit: cli.asset_limit,
            progress_interval: cli.progress_interval,
            jobs: cli.jobs,
            rainbow: rainbow.as_ref(),
            aux_wads,
        };
        match simulate::run_simulate_with_options(base, patch, opts) {
            Ok(report) => {
                simulate::print_simulate_report(&report, rainbow.as_ref());
                let sim_code = simulate::simulate_exit_code(&report);
                if sim_code != 0 {
                    exit_code = sim_code;
                }
                if let Some(ref json_path) = cli.json_output {
                    let json = serde_json::to_string_pretty(&report)?;
                    let mut f = File::create(json_path)?;
                    f.write_all(json.as_bytes())?;
                    println!("Wrote JSON report to {}", json_path.display());
                }
            }
            Err(e) => {
                eprintln!("Simulation failed: {e}");
                exit_code = 1;
            }
        }
        println!();
    }

    std::process::exit(exit_code);
}
