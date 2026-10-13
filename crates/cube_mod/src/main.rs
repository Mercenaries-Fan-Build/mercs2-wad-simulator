//! cube_mod — custom-geometry / asset-override patch builder for Mercenaries 2 (PC).
//!
//! Builds a `vz-patch.wad` overlay that overrides existing model (and texture)
//! assets BY HASH (last-opened-wins). It sources each model from the block its
//! ASET entry actually points to (so the HIER/structure the engine instantiates
//! is preserved), then either:
//!   * `--inject-container <file>`: uses a pre-built model UCFX container (e.g. the
//!     output of `tools/gltf_to_ucfx_model.py`), or
//!   * default: cube-izes the model in place (`--shape corner|clamp`), or
//!   * `--no-cubeize`: identity passthrough.
//! `--inject-extra HASH:TYPEID:file` adds extra override blocks (e.g. a texture
//! from `tools/dds_to_ucfx_texture.py`).
//!
//! Overriding by hash sidesteps the unsolved ASET name-hash (modding deep-dive
//! Open Q#6). Default target: every delivery-crate / aid-crate model.

use std::fs::File;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use mercs2_formats::ffcs::{find_chunk, load_ffcs_archive, FfcsArchive};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::model_cubeize::{cubeize_model_container_with, CubeShape};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_block};
use mercs2_formats::ucfx::parse_block_entry_table;

#[derive(Parser)]
#[command(
    name = "cube_mod",
    about = "Build a vz-patch.wad overriding model/texture assets (cube-ize or inject custom containers)"
)]
struct Cli {
    /// Source vz.wad to read the target block(s) from.
    #[arg(long)]
    source_wad: PathBuf,
    /// Output patch WAD path (typically <game>/data/vz-patch.wad).
    #[arg(short, long)]
    output: PathBuf,
    /// Explicit block index(es) to target (repeatable). Overrides --target-name.
    #[arg(long)]
    block_index: Vec<usize>,
    /// Comma-separated path substrings to auto-select when --block-index is absent.
    #[arg(long, default_value = "deliverycrate,crateaid")]
    target_name: String,
    /// List blocks matching --target-name (that contain a model) and exit.
    #[arg(long)]
    list: bool,
    /// Identity passthrough (no cube-ize) — isolates geometry vs plumbing issues.
    #[arg(long)]
    no_cubeize: bool,
    /// Cube shape: "corner" (sharp 8-corner cube, default) or "clamp".
    #[arg(long, default_value = "corner")]
    shape: String,
    /// Inject a pre-built model UCFX container instead of cube-izing. Single-target.
    #[arg(long)]
    inject_container: Option<PathBuf>,
    /// Add an extra override block from a raw UCFX container, as "0xHASH:TYPEID:path"
    /// (repeatable). E.g. a texture: "0x21A2AFD1:27:heart.bin".
    #[arg(long)]
    inject_extra: Vec<String>,
    #[arg(short, long)]
    verbose: bool,
}

const MODEL_TYPE_HASH: u32 = 0x5B72_4250; // pandemic_hash_m2("model")
const MODEL_ASET_TYPE_ID: u32 = 19; // ASET type_id for "model"

struct Built {
    block: PatchBlock,
    model_hash: u32,
}

/// UCFX type_hash for an ASET type_id (inverse of aset_type_ids, the few we emit).
fn type_hash_for_type_id(type_id: u32) -> Option<u32> {
    match type_id {
        19 => Some(0x5B72_4250), // model
        27 => Some(0xF011_157A), // texture
        35 => Some(0x4249_8680), // script (pandemic_hash_m2("script"))
        _ => None,
    }
}

/// Find a model-type container in a decompressed block (by name hash, or first).
fn find_model(decompressed: &[u8], want: Option<u32>) -> Option<(usize, usize, u32, u32, u32)> {
    let (count, entries) = parse_block_entry_table(decompressed);
    let mut offset = 4 + count as usize * 16;
    let mut found = None;
    for e in &entries {
        let span = (offset, offset + e.chunk_size as usize);
        let is_model = e.type_hash == MODEL_TYPE_HASH && want.map_or(true, |w| e.name_hash == w);
        if is_model && found.is_none() && span.1 <= decompressed.len() {
            found = Some((span.0, span.1, e.name_hash, e.type_hash, e.field_c));
        }
        offset = span.1;
    }
    found
}

fn build_block(
    file: &mut File,
    archive: &FfcsArchive,
    block_index: usize,
    no_cubeize: bool,
    shape: CubeShape,
    inject: Option<&[u8]>,
    verbose: bool,
) -> Result<Option<Built>, String> {
    let probe = decompress_block(file, &archive.indx, block_index as u16)
        .map_err(|e| format!("decompress block {block_index}: {e}"))?;
    let model_name = match find_model(&probe, None) {
        Some((_, _, name, _, _)) => name,
        None => return Ok(None),
    };

    // Source from the block the engine instantiates (ASET primary block_index).
    let src_block_index = archive
        .aset
        .iter()
        .find(|e| e.asset_hash == model_name && e.type_id == MODEL_ASET_TYPE_ID)
        .map(|e| e.block_index() as usize)
        .unwrap_or(block_index);

    let (src_bytes, from_index) = if src_block_index != block_index {
        let b = decompress_block(file, &archive.indx, src_block_index as u16)
            .map_err(|e| format!("decompress ASET block {src_block_index}: {e}"))?;
        (b, src_block_index)
    } else {
        (probe, block_index)
    };

    let (mstart, mend, model_name, model_type, model_field_c) =
        find_model(&src_bytes, Some(model_name))
            .ok_or_else(|| format!("model 0x{model_name:08X} not in source block {from_index}"))?;

    let path_string = archive
        .paths
        .get(from_index)
        .cloned()
        .unwrap_or_else(|| format!("block_{from_index}"));

    let container: Vec<u8> = if let Some(bytes) = inject {
        if bytes.len() < 20 || &bytes[0..4] != b"UCFX" {
            return Err("--inject-container is not a UCFX container".into());
        }
        println!("  0x{model_name:08X}: injecting external container ({} bytes)", bytes.len());
        bytes.to_vec()
    } else if no_cubeize {
        src_bytes[mstart..mend].to_vec()
    } else {
        let (cubed, stats) = cubeize_model_container_with(&src_bytes[mstart..mend], shape)?;
        if cubed.len() != mend - mstart {
            return Err("cube-ize changed container length (unexpected)".into());
        }
        if stats.vertices_snapped == 0 {
            return Err(format!("block {from_index}: model has no vertex meshes"));
        }
        if verbose {
            println!(
                "  0x{model_name:08X} from block {from_index}: {} meshes, {} verts reshaped",
                stats.strm_meshes, stats.vertices_snapped
            );
        }
        cubed
    };

    // MODEL-ONLY block: [u32 count=1][16-byte entry][model container].
    let mut new_block = Vec::with_capacity(4 + 16 + container.len());
    new_block.extend_from_slice(&1u32.to_le_bytes());
    new_block.extend_from_slice(&model_name.to_le_bytes());
    new_block.extend_from_slice(&model_type.to_le_bytes());
    new_block.extend_from_slice(&model_field_c.to_le_bytes());
    new_block.extend_from_slice(&(container.len() as u32).to_le_bytes());
    new_block.extend_from_slice(&container);

    let compressed = compress_sges(&new_block).map_err(|e| format!("sges compress: {e}"))?;

    let secondary_ref = archive
        .aset
        .iter()
        .find(|e| e.asset_hash == model_name && e.type_id == MODEL_ASET_TYPE_ID)
        .map(|e| e.secondary_ref)
        .unwrap_or(0xFFFF_FFFF);
    let aset = vec![AsetEntry::new(model_name, secondary_ref, 0x0000_FFFF, MODEL_ASET_TYPE_ID)];

    let decomp_pages = ((new_block.len() + 0x7FFF) / 0x8000) as u32;
    let mut block = PatchBlock::new(compressed, path_string, aset);
    block.packed_field = decomp_pages;

    Ok(Some(Built { block, model_hash: model_name }))
}

/// Parse "0xHASH:TYPEID:path" -> single-asset PRIMARY override block.
fn build_extra(spec: &str) -> Result<PatchBlock, String> {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(format!("--inject-extra '{spec}' must be HASH:TYPEID:path"));
    }
    let hash = u32::from_str_radix(parts[0].trim_start_matches("0x"), 16)
        .map_err(|e| format!("bad hash in '{spec}': {e}"))?;
    let type_id: u32 = parts[1].parse().map_err(|e| format!("bad type_id in '{spec}': {e}"))?;
    let type_hash = type_hash_for_type_id(type_id)
        .ok_or_else(|| format!("unsupported type_id {type_id} (need 19 model / 27 texture)"))?;
    let container = std::fs::read(parts[2]).map_err(|e| format!("read {}: {e}", parts[2]))?;
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err(format!("{} is not a UCFX container", parts[2]));
    }
    let mut block = Vec::with_capacity(4 + 16 + container.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&hash.to_le_bytes());
    block.extend_from_slice(&type_hash.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes()); // field_c
    block.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block.extend_from_slice(&container);

    let compressed = compress_sges(&block).map_err(|e| format!("sges: {e}"))?;
    let decomp_pages = ((block.len() + 0x7FFF) / 0x8000) as u32;
    let aset = vec![AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, type_id)];
    let mut pb = PatchBlock::new(compressed, format!("blocks\\VZ\\inject_{hash:08x}.block"), aset);
    pb.packed_field = decomp_pages;
    println!("  extra: 0x{hash:08X} type_id={type_id} ({} container bytes)", container.len());
    Ok(pb)
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    debug_assert_eq!(pandemic_hash_m2("model"), MODEL_TYPE_HASH);

    let shape = match cli.shape.to_lowercase().as_str() {
        "corner" => CubeShape::Corner,
        "clamp" => CubeShape::Clamp,
        other => return Err(format!("unknown --shape '{other}' (use corner|clamp)")),
    };

    let inject_bytes: Option<Vec<u8>> = match &cli.inject_container {
        Some(p) => Some(std::fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))?),
        None => None,
    };

    let mut file = File::open(&cli.source_wad)
        .map_err(|e| format!("open {}: {e}", cli.source_wad.display()))?;
    let file_size = file.metadata().map_err(|e| format!("metadata: {e}"))?.len();
    let archive = load_ffcs_archive(&mut file, file_size).map_err(|e| format!("FFCS: {e}"))?;

    let needles: Vec<String> = cli
        .target_name
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    if cli.list {
        println!("Blocks matching {needles:?} that contain a model:");
        for (i, p) in archive.paths.iter().enumerate() {
            let lp = p.to_lowercase();
            if needles.iter().any(|n| lp.contains(n)) {
                println!("  [{i}] {p}");
            }
        }
        return Ok(());
    }

    let indices: Vec<usize> = if !cli.block_index.is_empty() {
        cli.block_index.clone()
    } else {
        archive
            .paths
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let lp = p.to_lowercase();
                needles.iter().any(|n| lp.contains(n))
            })
            .map(|(i, _)| i)
            .collect()
    };
    if indices.is_empty() {
        return Err(format!("no blocks matched {needles:?} (try --list)"));
    }

    let mut blocks: Vec<PatchBlock> = Vec::new();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut skipped_no_model = 0usize;
    for &idx in &indices {
        if idx >= archive.indx.len() {
            return Err(format!("block_index {idx} >= INDX count {}", archive.indx.len()));
        }
        match build_block(&mut file, &archive, idx, cli.no_cubeize, shape, inject_bytes.as_deref(), cli.verbose)? {
            Some(b) => {
                if seen.insert(b.model_hash) {
                    blocks.push(b.block);
                }
            }
            None => skipped_no_model += 1,
        }
    }
    for spec in &cli.inject_extra {
        blocks.push(build_extra(spec)?);
    }
    if blocks.is_empty() {
        return Err("no model-bearing blocks among the targets".into());
    }
    println!(
        "Override {} block(s){}{}",
        blocks.len(),
        if skipped_no_model > 0 {
            format!(" ({skipped_no_model} target blocks had no model)")
        } else {
            String::new()
        },
        if cli.no_cubeize { " [identity]" } else { "" }
    );

    let csum_value = find_chunk(&archive.chunks, b"CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = find_chunk(&archive.chunks, b"CSUM").map(|r| r.meta);

    let wad = build_patch_wad_multi(&blocks, csum_value, csum_meta, &FFCS_CERT_BLOB);
    if let Some(parent) = cli.output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    std::fs::write(&cli.output, &wad).map_err(|e| format!("write: {e}"))?;
    println!(
        "Wrote {} ({} bytes / {:.2} MB, {} blocks)",
        cli.output.display(),
        wad.len(),
        wad.len() as f64 / 1024.0 / 1024.0,
        blocks.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cube_mod error: {e}");
            ExitCode::FAILURE
        }
    }
}
