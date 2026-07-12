//! Load real base-game models directly from a Mercenaries 2 WAD (`vz.wad`).
//!
//! Mirrors `cube_mod`'s extraction: open the FFCS archive, find model assets via the ASET table
//! (`type_id == 19`, primary), decompress the owning block, and slice out the model container
//! (`type_hash == "model"`) — which is the UCFX container `mesh::build_from_container` consumes.

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};
use mercs2_formats::sges::{decompress_block, decompress_block_head};
use mercs2_formats::types::TYPE_ID_ANIMATION;
use mercs2_formats::ucfx::parse_block_entry_table;
use std::fs::File;

pub const MODEL_TYPE_HASH: u32 = 0x5B72_4250; // pandemic_hash_m2("model")
pub const MODEL_ASET_TYPE_ID: u32 = 19;

pub struct Wad {
    file: File,
    archive: FfcsArchive,
    /// Small MRU cache of recently decompressed blocks (most-recent last). The hi-res texture
    /// assembler re-reads the same c3 finer-LOD blocks for every texture of a model that shares a
    /// cell subtree; caching the (often multi-MB) decompressed blocks avoids re-inflating them.
    block_cache: Vec<(u16, std::sync::Arc<Vec<u8>>)>,
}

/// Discover `vz.wad` from the game's EA Games registry key
/// (`HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames` → `Install Dir` + `data\vz.wad`).
/// Returns the path only if the file actually exists.
#[cfg(windows)]
pub fn registry_vz_wad() -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames")
        .ok()?;
    let dir: String = key.get_value("Install Dir").ok()?;
    let mut p = std::path::PathBuf::from(dir);
    p.push("data");
    p.push("vz.wad");
    p.exists().then(|| p.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
pub fn registry_vz_wad() -> Option<String> {
    None
}

pub fn open(path: &str) -> Result<Wad, String> {
    let mut file = File::open(path).map_err(|e| format!("open {path}: {e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive =
        load_ffcs_archive(&mut file, size).map_err(|e| format!("parse FFCS archive: {e}"))?;
    Ok(Wad { file, archive, block_cache: Vec::new() })
}

/// Every ASET `(type_id, is_primary, block_index)` a hash appears under (any type). For diagnosing
/// what asset type a name-hash resolves to (e.g. model=19 vs a foliage/other type).
pub fn aset_types(wad: &Wad, name_hash: u32) -> Vec<(u32, bool, u16)> {
    wad.archive
        .aset
        .iter()
        .filter(|e| e.asset_hash == name_hash)
        .map(|e| (e.type_id, e.is_primary(), e.block_index()))
        .collect()
}

/// Every distinct ASET `(name_hash, type_id, is_primary)` in the archive — for reverse-name hunts over
/// assets that aren't primary models (e.g. props whose mesh isn't a type-19 primary ASET, like the
/// recruitheli mesh). Sorted + deduped.
pub fn all_asets(wad: &Wad) -> Vec<(u32, u32, bool)> {
    let mut v: Vec<(u32, u32, bool)> = wad
        .archive
        .aset
        .iter()
        .map(|e| (e.asset_hash, e.type_id, e.is_primary()))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// All model assets as `(name_hash, block_index)` from primary ASET entries, sorted + deduped.
pub fn model_list(wad: &Wad) -> Vec<(u32, u16)> {
    let mut v: Vec<(u32, u16)> = wad
        .archive
        .aset
        .iter()
        .filter(|e| e.type_id == MODEL_ASET_TYPE_ID && e.is_primary())
        .map(|e| (e.asset_hash, e.block_index()))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Slice the model container (`type_hash == "model"`) out of a decompressed block.
/// The model container for `name_hash` inside an already-decompressed block, if that block carries
/// one. A model's LODs are not all in its primary ASET block (see `extract_texture_hires` for the
/// same pattern on mips), so callers that assemble across a cell subtree need to ask block-by-block.
pub fn model_span_in(decompressed: &[u8], name_hash: u32) -> Option<Vec<u8>> {
    let (s, e) = find_model_span(decompressed, Some(name_hash))?;
    Some(decompressed[s..e].to_vec())
}

fn find_model_span(decompressed: &[u8], want: Option<u32>) -> Option<(usize, usize)> {
    let (count, entries) = parse_block_entry_table(decompressed);
    let mut offset = 4 + count as usize * 16;
    for e in &entries {
        let end = offset + e.chunk_size as usize;
        if e.type_hash == MODEL_TYPE_HASH
            && want.map_or(true, |w| e.name_hash == w)
            && end <= decompressed.len()
        {
            return Some((offset, end));
        }
        offset = end;
    }
    None
}

/// Extract + decode the texture chunk `name_hash` from ONE specific block (not the ASET-resolved
/// primary). For auditing texture streaming: the same texture hash appears in several blocks (a
/// resident low-res tail in the model's block + finer mips in the c3 subtree's deeper blocks).
///
/// The runtime registry keeps the FIRST block to register a hash, not the last: `FUN_004cc130` probes
/// the 5120-cell pool and, on an occupied slot, returns the existing cell and creates nothing. Mips
/// accumulate into that one cell (see `extract_texture_hires`); later blocks do not clobber it. (The
/// last-wins rule that *does* exist is WAD-overlay file resolution — a different layer. See
/// `registry.rs` and `docs/modernization/model_render_gate_spec.md` §2b.)
///
/// Returns None if the block has no matching texture chunk.
pub fn tex_from_block(
    wad: &mut Wad,
    block: u16,
    name_hash: u32,
) -> Option<mercs2_formats::texture::TextureData> {
    let dec = decompress_block_index(wad, block).ok()?;
    let (count, entries) = parse_block_entry_table(&dec);
    let mut off = 4 + count as usize * 16;
    for e in &entries {
        let end = off + e.chunk_size as usize;
        if e.type_hash == mercs2_formats::types::TYPE_HASH_TEXTURE && e.name_hash == name_hash && end <= dec.len() {
            return mercs2_formats::texture::parse_texture_container(&dec[off..end]).ok();
        }
        off = end;
    }
    None
}

/// Cell prefix of a block path (`…\c33286_P000_Q3.block` → `c33286`), or None for non-cell blocks.
/// c3 streaming blocks name their cell subtree before the `_P<level>` LOD suffix.
fn block_cell_prefix(paths: &[String], blk: u16) -> Option<String> {
    let p = paths.get(blk as usize)?;
    let fname = p.rsplit(['\\', '/']).next()?;
    let idx = fname.find("_P")?;
    Some(fname[..idx].to_string())
}

/// Full-resolution texture extraction: the resident ASET block ships only a coarse mip tail; the
/// higher mips stream from FINER LOD blocks in the SAME c3 cell subtree (`c33286_P000_Q3` resident →
/// `c33286-…_P00N_Q(3-N)` finer, each a lone BODY chunk = one finer mip). Gather every BODY the hash
/// carries across its subtree and assemble the full linear chain. Falls back to the resident (low-res)
/// texture for non-cell/global textures that don't stream. See memory `world-streaming-spec` §6.
pub fn extract_texture_hires(
    wad: &mut Wad,
    name_hash: u32,
) -> Result<mercs2_formats::texture::TextureData, String> {
    let resident = extract_texture(wad, name_hash)?;
    // Resident block (primary texture ASET row, else any) → its cell subtree prefix.
    let rblk = wad.archive.aset.iter()
        .find(|e| e.asset_hash == name_hash && e.type_id == mercs2_formats::types::TYPE_ID_TEXTURE && e.is_primary())
        .or_else(|| wad.archive.aset.iter().find(|e| e.asset_hash == name_hash && e.type_id == mercs2_formats::types::TYPE_ID_TEXTURE))
        .map(|e| e.block_index());
    let Some(rblk) = rblk else { return Ok(resident) };
    let Some(prefix) = block_cell_prefix(&wad.archive.paths, rblk) else { return Ok(resident) };
    let dash_prefix = format!("{prefix}-");

    // Blocks in this cell subtree (the cell itself + finer descendants that append child cell ids).
    let subtree: Vec<u16> = (0..wad.archive.paths.len() as u16)
        .filter(|&b| match block_cell_prefix(&wad.archive.paths, b) {
            Some(cp) => cp == prefix || cp.starts_with(&dash_prefix),
            None => false,
        })
        .collect();

    let mut bodies: Vec<Vec<u8>> = Vec::new();
    for b in subtree {
        if let Some(dec) = decompress_block_index(wad, b).ok() {
            let (count, entries) = parse_block_entry_table(&dec);
            let mut off = 4 + count as usize * 16;
            for e in &entries {
                let end = off + e.chunk_size as usize;
                if e.type_hash == mercs2_formats::types::TYPE_HASH_TEXTURE && e.name_hash == name_hash && end <= dec.len() {
                    if let Some(body) = mercs2_formats::texture::texture_body(&dec[off..end]) {
                        bodies.push(body);
                    }
                    break;
                }
                off = end;
            }
        }
    }
    if bodies.len() <= 1 {
        return Ok(resident); // only the resident tail present → nothing finer to assemble
    }
    Ok(mercs2_formats::texture::assemble_hires(resident.width, resident.height, resident.format, bodies))
}

/// Extract + decode a texture asset (DXT/BC bytes + dims) by its hash.
pub fn extract_texture(
    wad: &mut Wad,
    name_hash: u32,
) -> Result<mercs2_formats::texture::TextureData, String> {
    mercs2_formats::texture::extract_texture(&mut wad.file, &wad.archive, name_hash)
}

/// The real loading-screen background from `shell.wad` (sibling of the given `vz.wad`):
/// the `lti_precache1` plate (2048x1024 DXT1, hash 0x7329D083), cropped to its content.
pub fn shell_loading_plate(vz_wadpath: &str) -> Result<mercs2_formats::texture::TextureData, String> {
    let shell = std::path::Path::new(vz_wadpath).with_file_name("shell.wad");
    let mut wad = open(&shell.to_string_lossy())?;
    let hash = mercs2_formats::hash::pandemic_hash_m2("lti_precache1");
    let td = extract_texture(&mut wad, hash)?;
    Ok(crop_loading_plate(td))
}

/// Crop the plate to its content rect: the art is a 1280x720 frame inset at (384,149) in the
/// 2048x1024 texture, surrounded by transparent black (measured non-black bbox). Cropping on
/// 4px block boundaries (x 384, y 148) makes the quad's aspect match the art. Returns the
/// texture unchanged if it isn't the expected plate layout.
fn crop_loading_plate(td: mercs2_formats::texture::TextureData) -> mercs2_formats::texture::TextureData {
    use mercs2_formats::texture::TexFormat;
    const RECT: (u32, u32, u32, u32) = (384, 148, 1280, 720); // x, y, w, h
    let (cx, cy, cw, ch) = RECT;
    let block_bytes = match td.format {
        TexFormat::Bc1 => 8usize,
        TexFormat::Bc3 => 16usize,
    };
    let src_blocks_wide = (td.width / 4) as usize;
    let need = src_blocks_wide * (td.height / 4) as usize * block_bytes;
    if td.width < cx + cw || td.height < cy + ch || td.mip0.len() < need {
        return td;
    }
    let row_bytes = (cw / 4) as usize * block_bytes;
    let mut mip0 = Vec::with_capacity((ch / 4) as usize * row_bytes);
    for by in (cy / 4)..((cy + ch) / 4) {
        let s = (by as usize * src_blocks_wide + (cx / 4) as usize) * block_bytes;
        mip0.extend_from_slice(&td.mip0[s..s + row_bytes]);
    }
    mercs2_formats::texture::TextureData {
        width: cw,
        height: ch,
        format: td.format,
        all_mips: mip0.clone(),
        mip0,
        mip_count: 1,
    }
}

/// Decompress a raw block by index (for animgroup blocks, which `animgroup::parse_animgroup`
/// consumes whole — they are not `type_hash=="model"` containers).
pub fn decompress_block_index(wad: &mut Wad, block: u16) -> Result<Vec<u8>, String> {
    if let Some((_, data)) = wad.block_cache.iter().find(|(b, _)| *b == block) {
        return Ok((**data).clone());
    }
    let data = decompress_block(&mut wad.file, &wad.archive.indx, block)?;
    wad.block_cache.push((block, std::sync::Arc::new(data.clone())));
    if wad.block_cache.len() > 6 {
        wad.block_cache.remove(0);
    }
    Ok(data)
}

/// Block path strings (PTHS), indexed by block index (e.g. `…\c30123\…` names cell blocks).
pub fn block_paths(wad: &Wad) -> &[String] {
    &wad.archive.paths
}

/// Decompress only the head of a block (enough for its entry table) — a cheap format probe
/// that avoids inflating multi-MB texture/stream blocks during scans.
pub fn peek_block_head(wad: &mut Wad, block: u16, max_out: usize) -> Result<Vec<u8>, String> {
    decompress_block_head(&mut wad.file, &wad.archive.indx, block, max_out)
}

/// Distinct block indices that hold an animation-type ASET asset (candidate animgroups),
/// found directly from the ASET table — no need to decompress every block.
pub fn animgroup_blocks(wad: &Wad) -> Vec<u16> {
    let mut v: Vec<u16> = wad
        .archive
        .aset
        .iter()
        .filter(|e| e.type_id == TYPE_ID_ANIMATION)
        .map(|e| e.block_index())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Split the `Wad` into its raw FFCS archive + backing file, for consumers (e.g.
/// `mercs2_formats::world_index::WorldIndex::build`) that read blocks directly through the
/// format-crate primitives rather than the engine wrapper.
pub fn archive_and_file(wad: &mut Wad) -> (&FfcsArchive, &mut File) {
    (&wad.archive, &mut wad.file)
}

/// Extract the UCFX model container for `name_hash` via its ASET block. Prefers the PRIMARY model
/// entry, but falls back to a SUB-ENTRY (`is_primary == false`) — the instanced world content
/// (trees/rocks/bushes/lamps like `jungle_env_plantlarge04`) is a shared model carried as a sub-entry
/// in another block, not a primary asset; those were previously unresolvable.
pub fn extract_container(wad: &mut Wad, name_hash: u32) -> Result<Vec<u8>, String> {
    let block = wad
        .archive
        .aset
        .iter()
        .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID && e.is_primary())
        .or_else(|| {
            wad.archive
                .aset
                .iter()
                .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID)
        })
        .map(|e| e.block_index())
        .ok_or_else(|| format!("no model ASET for 0x{name_hash:08X}"))?;
    let dec = decompress_block(&mut wad.file, &wad.archive.indx, block)?;
    let (s, e) = find_model_span(&dec, Some(name_hash))
        .ok_or_else(|| format!("model 0x{name_hash:08X} not found in block {block}"))?;
    Ok(dec[s..e].to_vec())
}

/// One rung of a model's on-disk LOD chain: the block it lives in and its container bytes.
pub struct ModelLod {
    pub block: u16,
    /// `P000_Q3` (coarsest, resident) … `P002_Q1` (finest, streamed). Parsed from the block path.
    pub level: u8,
    pub container: Vec<u8>,
}

/// A model's FULL LOD chain, coarsest first. `extract_container` returns only the primary ASET
/// block, which for every vehicle is the COARSEST rung — a low-poly proxy skinned `*_lod_dm`. The
/// finer rungs stream from sibling blocks named `<model>_P00N_Q(3-N)`, the same c3 LOD-block scheme
/// that carries a texture's higher mips (see [`extract_texture_hires`]). A tank's resident block
/// holds 4,435 triangles; its `_P002_Q1` block holds 28,620. Characters ship a single block and have
/// no chain. Coarse rungs carry per-mesh LOD masks; the streamed rungs do not — their rung IS the
/// block.
pub fn extract_model_lods(wad: &mut Wad, name_hash: u32) -> Result<Vec<ModelLod>, String> {
    let resident = wad
        .archive
        .aset
        .iter()
        .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID && e.is_primary())
        .or_else(|| {
            wad.archive
                .aset
                .iter()
                .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID)
        })
        .map(|e| e.block_index())
        .ok_or_else(|| format!("no model ASET for 0x{name_hash:08X}"))?;

    // The chain is the resident block's cell subtree: the cell itself plus finer descendants, which
    // append child cell ids (`c31664` -> `c31664-c20959`). Model-named blocks share the bare stem.
    let stem = block_lod_stem(&wad.archive.paths, resident);
    let candidates: Vec<u16> = match &stem {
        None => vec![resident],
        Some(s) => {
            let dashed = format!("{s}-");
            (0..wad.archive.paths.len() as u16)
                .filter(|&b| match block_lod_stem(&wad.archive.paths, b) {
                    Some(c) => c == *s || c.starts_with(&dashed),
                    None => false,
                })
                .collect()
        }
    };

    let mut lods: Vec<ModelLod> = Vec::new();
    for b in candidates {
        let level = block_lod_level(&wad.archive.paths, b).unwrap_or(0);
        let Ok(dec) = decompress_block_index(wad, b) else { continue };
        if let Some(container) = model_span_in(&dec, name_hash) {
            lods.push(ModelLod { block: b, level, container });
        }
    }
    if lods.is_empty() {
        return Err(format!("model 0x{name_hash:08X} in no block"));
    }
    lods.sort_by_key(|l| l.level);
    Ok(lods)
}

/// Stem of a LOD block path: `ch_veh_tank_ztz98_P002_Q1.block` -> `ch_veh_tank_ztz98`, and
/// `c31664-c20959_P001_Q2.block` -> `c31664-c20959`. None if the block isn't `_P<level>_Q<n>`-named.
fn block_lod_stem(paths: &[String], blk: u16) -> Option<String> {
    let f = paths.get(blk as usize)?.rsplit(['\\', '/']).next()?;
    Some(f[..f.find("_P")?].to_string())
}

/// The `N` in a `_P00N_` block name — the LOD rung, 0 = coarsest/resident.
fn block_lod_level(paths: &[String], blk: u16) -> Option<u8> {
    let f = paths.get(blk as usize)?.rsplit(['\\', '/']).next()?;
    let i = f.find("_P")? + 2;
    let digits: String = f[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Extract a container chunk of a given CHDR class `chunk_type` (e.g. the `0x7C569307` terrainmesh)
/// by its asset hash — resolves the block via ANY primary ASET for the hash (not just MODEL), then
/// walks the block entry table to the chunk whose `type_hash == chunk_type` and `name_hash` matches.
pub fn extract_container_typed(
    wad: &mut Wad,
    name_hash: u32,
    chunk_type: u32,
) -> Result<Vec<u8>, String> {
    let block = wad
        .archive
        .aset
        .iter()
        .find(|e| e.asset_hash == name_hash && e.is_primary())
        .map(|e| e.block_index())
        .ok_or_else(|| format!("no primary ASET for 0x{name_hash:08X}"))?;
    let dec = decompress_block(&mut wad.file, &wad.archive.indx, block)?;
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    let mut pos = 4 + count as usize * 16;
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.type_hash == chunk_type && e.name_hash == name_hash && end <= dec.len() {
            return Ok(dec[pos..end].to_vec());
        }
        pos = end;
    }
    Err(format!("chunk type 0x{chunk_type:08X} name 0x{name_hash:08X} not found in block {block}"))
}
