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
    Ok(Wad { file, archive })
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
    decompress_block(&mut wad.file, &wad.archive.indx, block)
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

/// Extract the UCFX model container for `name_hash` (via its primary ASET block).
pub fn extract_container(wad: &mut Wad, name_hash: u32) -> Result<Vec<u8>, String> {
    let block = wad
        .archive
        .aset
        .iter()
        .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID && e.is_primary())
        .map(|e| e.block_index())
        .ok_or_else(|| format!("no primary model ASET for 0x{name_hash:08X}"))?;
    let dec = decompress_block(&mut wad.file, &wad.archive.indx, block)?;
    let (s, e) = find_model_span(&dec, Some(name_hash))
        .ok_or_else(|| format!("model 0x{name_hash:08X} not found in block {block}"))?;
    Ok(dec[s..e].to_vec())
}
