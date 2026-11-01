//! What `add_level` ships: a level the game boots by name.
//!
//! The engine opens exactly two WADs for a level `L`: `.\Data\L.wad`, which must exist, then
//! `.\Data\L-patch.wad` above it, which may not (`FUN_004BFCE0`, `FUN_004BFDA0`, the same name). Both
//! console builds mount the DLC archive above the level and its patch instead (X360 `FUN_82215bb0`,
//! PS3 `FUN_00372e20`), and on PS3's `IsDLC` path the level under it is `vz`. A level does not run
//! alone: the PS3 archive lacks 25 of the 32 modules `dlc01.lua` imports. So the PC form is `L.wad`, a
//! copy of `vz.wad` made at deploy, and the level's own WAD as `L-patch.wad`, whose assets win
//! (`FUN_00875E80`) while everything else resolves from the copy.
//!
//! A WAD is a level when it holds the master script `L` and the layer `L_base`, the two assets
//! `LevelBootstrap.LoadLevel` asks for first (`levelbootstrap.lua:14-15`). What a level references
//! but neither it nor the game holds is an error here, naming each one: a script's `DEPS` and a
//! model's `MTRL` textures are read from the bytes. What Lua asks for by a name it computes is not
//! visible to a build.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::patch_wad::{read_patch_wad, PatchBlock};
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::types::{
    TYPE_HASH_MODEL, TYPE_HASH_SCRIPT, TYPE_HASH_WAVEBANK, TYPE_ID_LAYER, TYPE_ID_SCRIPT, TYPE_ID_TEXTURE,
};

use crate::game::GameStack;

/// The level's own WAD, relative to the game folder: the slot above `L.wad`.
pub fn patch_relative(name: &str) -> String {
    format!("data/{name}-patch.wad")
}

/// The level slot, relative to the game folder.
pub fn base_relative(name: &str) -> String {
    format!("data/{name}.wad")
}

/// The game's own WAD the level slot copies.
pub const BASE_FROM: &str = "data/vz.wad";

/// Whether ASET rows `(hash, type id)` hold level `name`'s master script and base layer.
pub fn holds_level(rows: impl Iterator<Item = (u32, u32)>, name: &str) -> bool {
    let (script, base) = (m2(name), m2(&format!("{name}_base")));
    let (mut has_script, mut has_base) = (false, false);
    for (hash, ty) in rows {
        has_script |= hash == script && ty == TYPE_ID_SCRIPT;
        has_base |= hash == base && ty == TYPE_ID_LAYER;
    }
    has_script && has_base
}

/// Refuse a `.pws` that would replace one the game ships: `music`, `ambience`, a `vo_stream` language.
pub fn stream_name_refusal(file: &str) -> Option<String> {
    let Some(stem) = file.strip_suffix(".pws") else {
        return Some(format!("{file:?} is not a .pws file"));
    };
    let shipped = stem == "music" || stem == "ambience" || stem.starts_with("vo_stream.");
    shipped.then(|| format!("{file:?} is a stream the game ships, and add_level only adds"))
}

/// What a level's chunks name that it may not hold itself.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reach {
    /// Each script a script's `DEPS` names, with the script that names it.
    pub scripts: BTreeMap<u32, u32>,
    /// Each texture a model's `MTRL` names, with the model that names it.
    pub textures: BTreeMap<u32, u32>,
    /// The entry hash of each streamed wavebank.
    pub streamed: BTreeSet<u32>,
}

/// Read what `blocks` reference.
pub fn reach(blocks: &[PatchBlock]) -> Result<Reach, String> {
    let mut r = Reach::default();
    for b in blocks {
        let raw = mercs2_formats::sges::decompress_sges(&b.compressed_data).map_err(|e| format!("{}: {e}", b.path_string))?;
        for (e, span) in mercs2_formats::ucfx::block_chunk_spans(&raw).map_err(|m| format!("{}: {m}", b.path_string))? {
            let chunk = &raw[span];
            let at = |m: String| format!("{}: chunk 0x{:08X}: {m}", b.path_string, e.name_hash);
            if e.type_hash == TYPE_HASH_SCRIPT {
                for dep in mercs2_formats::scripts_block::container_deps(chunk).map_err(at)? {
                    r.scripts.entry(dep).or_insert(e.name_hash);
                }
            } else if e.type_hash == TYPE_HASH_MODEL {
                for m in parse_mtrl(chunk, MtrlSource::Model).map_err(|err| at(format!("{err:?}")))? {
                    for t in m.textures.iter().filter(|t| **t != 0) {
                        r.textures.entry(*t).or_insert(e.name_hash);
                    }
                }
            } else if e.type_hash == TYPE_HASH_WAVEBANK {
                let body = mercs2_formats::ucfx::extract_chunk_body(chunk, b"data").ok_or_else(|| at("no data chunk".into()))?;
                let flag = body.get(10..12).ok_or_else(|| at("wavebank header is short".into()))?;
                if u16::from_le_bytes([flag[0], flag[1]]) != 0 {
                    r.streamed.insert(e.name_hash);
                }
            }
        }
    }
    Ok(r)
}

/// Each `(what, name, by)` of `refs` that `held` does not hold, as a line.
fn missing(what: &str, refs: &BTreeMap<u32, u32>, held: impl Fn(u32) -> bool) -> Vec<String> {
    refs.iter().filter(|(h, _)| !held(**h)).map(|(h, by)| format!("{what} 0x{h:08X}, named by 0x{by:08X}")).collect()
}

/// A level ready to place: its blocks and its streams, `(file name, bytes)`.
#[derive(Debug)]
pub struct Level {
    pub blocks: Vec<PatchBlock>,
    pub streams: Vec<(String, Vec<u8>)>,
    /// The scripts and textures the level references, and how many of them only the base holds.
    pub references: usize,
    pub from_base: usize,
}

/// The stems of `files` that name no streamed bank, and the banks no file names, as error lines.
fn stream_problems(files: &[String], streamed: &BTreeSet<u32>) -> Vec<String> {
    let stems: BTreeMap<u32, &str> =
        files.iter().filter_map(|f| f.strip_suffix(".pws").map(|s| (m2(s), f.as_str()))).collect();
    let mut out: Vec<String> =
        stems.iter().filter(|(h, _)| !streamed.contains(h)).map(|(_, f)| format!("stream {f} is no streamed bank's")).collect();
    out.extend(streamed.iter().filter(|h| !stems.contains_key(h)).map(|h| format!("streamed bank 0x{h:08X} has no stream listed")));
    out
}

/// Read and check level `name` from its WAD and streams, under `root`, against `game`.
pub fn read(root: &Path, name: &str, wad: &Path, streams: &[PathBuf], game: &GameStack) -> Result<Level, String> {
    let at = root.join(wad);
    let raw = std::fs::read(&at).map_err(|e| format!("{}: {e}", at.display()))?;
    let blocks = read_patch_wad(&raw).map_err(|e| format!("{}: {e}", at.display()))?.blocks;
    let rows = || blocks.iter().flat_map(|b| b.aset_entries.iter().map(|e| (e.asset_hash, e.u32_3)));
    if !holds_level(rows(), name) {
        return Err(format!("{}: holds no script {name:?} and layer \"{name}_base\", so it is not level {name:?}", at.display()));
    }
    let held: BTreeSet<(u32, u32)> = rows().collect();
    let refs = reach(&blocks)?;
    let mut bad = missing("script", &refs.scripts, |h| held.contains(&(h, TYPE_ID_SCRIPT)) || game.has_asset(h, TYPE_ID_SCRIPT));
    bad.extend(missing("texture", &refs.textures, |h| held.contains(&(h, TYPE_ID_TEXTURE)) || game.has_asset(h, TYPE_ID_TEXTURE)));
    let mut files = Vec::with_capacity(streams.len());
    for s in streams {
        let file = s.file_name().and_then(|f| f.to_str()).ok_or_else(|| format!("{}: no file name", s.display()))?.to_ascii_lowercase();
        if let Some(why) = stream_name_refusal(&file) {
            bad.push(why);
        }
        files.push(file);
    }
    bad.extend(stream_problems(&files, &refs.streamed));
    if !bad.is_empty() {
        return Err(format!("level {name:?} has {} unresolved reference(s):\n  {}", bad.len(), bad.join("\n  ")));
    }
    let mut out = Vec::with_capacity(streams.len());
    for (s, file) in streams.iter().zip(files) {
        let from = root.join(s);
        out.push((file, std::fs::read(&from).map_err(|e| format!("{}: {e}", from.display()))?));
    }
    let references = refs.scripts.len() + refs.textures.len();
    let own = refs.scripts.keys().filter(|h| held.contains(&(**h, TYPE_ID_SCRIPT))).count()
        + refs.textures.keys().filter(|h| held.contains(&(**h, TYPE_ID_TEXTURE))).count();
    Ok(Level { blocks, streams: out, references, from_base: references - own })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_needs_its_script_and_its_base_layer() {
        let (s, b) = ((m2("dlc01"), TYPE_ID_SCRIPT), (m2("dlc01_base"), TYPE_ID_LAYER));
        assert!(holds_level([s, b].into_iter(), "dlc01"));
        assert!(!holds_level([s].into_iter(), "dlc01"));
        assert!(!holds_level([b].into_iter(), "dlc01"));
        assert!(!holds_level([s, (m2("dlc01_base"), TYPE_ID_TEXTURE)].into_iter(), "dlc01"));
        assert!(!holds_level([s, b].into_iter(), "english"));
    }

    #[test]
    fn a_stream_may_not_replace_a_shipped_one() {
        assert!(stream_name_refusal("vo_stream_dlctest.english.pws").is_none());
        assert!(stream_name_refusal("dlctest_streaming.pws").is_none());
        for shipped in ["music.pws", "ambience.pws", "vo_stream.english.pws"] {
            assert!(stream_name_refusal(shipped).is_some(), "{shipped}");
        }
        assert!(stream_name_refusal("notes.txt").unwrap().contains("not a .pws"));
    }

    #[test]
    fn every_stream_and_every_streamed_bank_must_pair_up() {
        let bank = m2("vo_x.english");
        let banks: BTreeSet<u32> = [bank].into();
        assert!(stream_problems(&["vo_x.english.pws".to_string()], &banks).is_empty());
        assert!(stream_problems(&[], &banks)[0].contains("no stream listed"));
        assert!(stream_problems(&["other.pws".to_string()], &BTreeSet::new())[0].contains("no streamed bank"));
    }

    #[test]
    fn a_missing_reference_names_what_named_it() {
        let refs = BTreeMap::from([(1u32, 9u32), (2, 9)]);
        let bad = missing("texture", &refs, |h| h == 1);
        assert_eq!(bad, vec!["texture 0x00000002, named by 0x00000009".to_string()]);
    }
}
