//! Load real base-game models directly from a Mercenaries 2 WAD (`vz.wad`).
//!
//! Mirrors `cube_mod`'s extraction: open the FFCS archive, find model assets via the ASET table
//! (`type_id == 19`, primary), decompress the owning block, and slice out the model container
//! (`type_hash == "model"`) — which is the UCFX container `mesh::build_from_container` consumes.

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};
use mercs2_formats::sges::decompress_block;
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
