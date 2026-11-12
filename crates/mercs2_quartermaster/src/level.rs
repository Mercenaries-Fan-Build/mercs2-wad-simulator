//! What `add_level` ships: a level the game boots by name.
//!
//! The engine opens exactly two WADs for a level `L`: `.\Data\L.wad`, which must exist, then
//! `.\Data\L-patch.wad` above it, which may not (`FUN_004BFCE0`, `FUN_004BFDA0`, the same name). Both
//! console builds mount the DLC archive above the level and its patch instead (X360 `FUN_82215bb0`,
//! PS3 `FUN_00372e20`), and on PS3's `IsDLC` path the level under it is `vz`. A level does not run
//! alone: the PS3 archive lacks 25 of the 32 modules `dlc01.lua` imports. So the PC form is `L.wad`, a
//! copy of `vz.wad` made at deploy, and `L-patch.wad`, compiled from the Shipment's `level_asset`
//! rows, whose assets win (`FUN_00875E80`) while everything else resolves from the copy. A Shipment
//! ships no WAD: `vz-patch.wad` is not mounted in a level, so a shipped one would shut every other
//! Shipment out of it.
//!
//! Assets are a level when they hold the master script `L` and the layer `L_base`, the two assets
//! `LevelBootstrap.LoadLevel` asks for first (`levelbootstrap.lua:14-15`). What a level references
//! but neither it nor the game holds is an error here, naming each one: a script's `DEPS`, a model's
//! `MTRL` textures, and every material's pixel shader and model group's vertex shaders, which must be
//! registered, are read from the bytes. What Lua asks for by a name it computes is not visible to a
//! build.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::be_to_le::texture::rung_of_path;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::patch_wad::{rung_chain, AsetEntry, PatchBlock};
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::types::{
    type_by_name, TYPE_HASH_FONT, TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_MODEL, TYPE_HASH_SCRIPT, TYPE_HASH_TERRAIN_MESH,
    TYPE_HASH_WAVEBANK, TYPE_ID_LAYER, TYPE_ID_SCRIPT, TYPE_ID_TEXTURE,
};

use crate::game::GameStack;
use crate::manifest::{Contribution, LevelRung, Manifest};

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

/// A level's streams install in its own folder, the one `Net.DLCGetMountPoint` names, so they never touch
/// the game's `data/Audios` ([`mercs2_formats::dlc_mount`]).
pub use mercs2_formats::dlc_mount::{mount_point, stream_relative};

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
    /// Each pixel shader a material names, with the chunk that names it.
    pub pixel: BTreeMap<u32, u32>,
    /// Each vertex shader a model's group names, with the model that names it.
    pub vertex: BTreeMap<u32, u32>,
}

/// How a chunk of `type_hash` stores its materials, when it has any.
/// The vertex shader of each draw in a terrain chunk: the first word of every 20-byte `PRMT` record.
fn draw_shaders(nodes: &[mercs2_formats::ucfx::UcfxNode]) -> Vec<u32> {
    let mut keys = Vec::new();
    for n in nodes {
        if let (b"PRMT", Some(body)) = (&n.tag, &n.body) {
            keys.extend(body.chunks_exact(20).map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]])));
        }
        keys.extend(draw_shaders(&n.children));
    }
    keys
}

fn material_source(type_hash: u32) -> Option<MtrlSource> {
    match type_hash {
        TYPE_HASH_MODEL => Some(MtrlSource::Model),
        TYPE_HASH_TERRAIN_MESH => Some(MtrlSource::TerrainMesh),
        TYPE_HASH_FONT => Some(MtrlSource::Font),
        TYPE_HASH_LOWRES_TERRAIN => Some(MtrlSource::LowResTerrain),
        mercs2_formats::scrub::TYPE_HASH => Some(MtrlSource::Scrub),
        _ => None,
    }
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
            } else if let Some(source) = material_source(e.type_hash) {
                for m in parse_mtrl(chunk, source).map_err(|err| at(format!("{err:?}")))? {
                    r.pixel.entry(m.shader_key).or_insert(e.name_hash);
                    if e.type_hash == TYPE_HASH_MODEL {
                        for t in m.textures.iter().filter(|t| **t != 0) {
                            r.textures.entry(*t).or_insert(e.name_hash);
                        }
                    }
                }
                if e.type_hash == TYPE_HASH_MODEL {
                    for g in crate::shader_import::read_model(chunk).map_err(at)?.0 {
                        r.vertex.entry(g.vertex).or_insert(e.name_hash);
                        r.vertex.entry(g.shadow).or_insert(e.name_hash);
                    }
                }
                if e.type_hash == TYPE_HASH_TERRAIN_MESH || e.type_hash == TYPE_HASH_LOWRES_TERRAIN {
                    for key in draw_shaders(&mercs2_formats::ucfx::parse_ucfx_tree(chunk).map_err(at)?) {
                        r.vertex.entry(key).or_insert(e.name_hash);
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
    /// The distinct pixel and vertex shaders the level names, every one registered.
    pub pixel_shaders: usize,
    pub vertex_shaders: usize,
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

/// One asset of a level, as its `level_asset` row states it.
#[derive(Debug, Clone, Copy)]
pub struct Asset<'a> {
    pub asset_type: &'a str,
    pub name: &'a str,
    pub rungs: &'a [LevelRung],
}

impl Asset<'_> {
    /// Its `(name hash, type hash)`.
    pub fn key(&self) -> Result<(u32, u32), String> {
        let (type_hash, _) = type_by_name(self.asset_type).ok_or_else(|| format!("{}: no asset type is named {:?}", self.name, self.asset_type))?;
        Ok((crate::manifest::asset_hash(self.name), type_hash))
    }
}

/// One `add_level` of a set, with the Shipment that adds it.
#[derive(Debug, Clone, Copy)]
pub struct LevelOf<'a> {
    pub name: &'a str,
    pub display: &'a str,
    pub root: &'a Path,
    pub manifest: &'a Manifest,
}

/// Every `add_level` of `shipments`, in load order.
pub fn levels_of<'a>(shipments: &[&'a crate::discover::LoadedShipment]) -> Vec<LevelOf<'a>> {
    let mut out = Vec::new();
    for s in shipments {
        for c in &s.manifest.contributions {
            if let Contribution::AddLevel { name, display, .. } = c {
                out.push(LevelOf { name, display, root: &s.root, manifest: &s.manifest });
            }
        }
    }
    out
}

/// One chunk of a block: `(name, type hash, table word, bytes)`.
type BlockChunk = (u32, u32, u32, Vec<u8>);

/// The rows of `assets` the manifest names for `level`. A row naming `templates` is no asset: its
/// templates are appended to the game's worldentity ([`vz_templates`]).
pub fn assets_of<'a>(manifest: &'a Manifest, level: &str) -> Vec<Asset<'a>> {
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::LevelAsset { level: l, asset_type, name, rungs, templates, .. } if l == level && templates.is_empty() => {
                Some(Asset { asset_type, name, rungs })
            }
            _ => None,
        })
        .collect()
}

/// The main campaign. A `level_asset` row of this level needs no `add_level`: its assets ship in the
/// Shipment's overlay, in `blocks\VZ\<block>.block` ([`compile_vz`]), and may only add.
pub const VZ: &str = "vz";

/// The type of the one `level: vz` row that ships nothing: it names templates of its container.
pub const WORLDENTITY_TYPE: &str = "world_entity_data";

/// One `level: vz` worldentity row: its contribution index, the container's rungs, the templates it
/// names and the field edits it sets on them.
#[derive(Debug, Clone, Copy)]
pub struct VzTemplates<'a> {
    pub index: usize,
    pub rungs: &'a [LevelRung],
    pub templates: &'a [String],
    pub edits: &'a BTreeMap<String, crate::template::FieldEdits>,
}

/// Every `level: vz` row of `manifest` that names templates.
pub fn vz_templates(manifest: &Manifest) -> Vec<VzTemplates<'_>> {
    manifest
        .contributions
        .iter()
        .enumerate()
        .filter_map(|(index, c)| match c {
            Contribution::LevelAsset { level, rungs, templates, edits, .. } if level == VZ && !templates.is_empty() => {
                Some(VzTemplates { index, rungs, templates, edits })
            }
            _ => None,
        })
        .collect()
}

/// Compile the `level: vz` `assets` under `root` into overlay blocks `blocks\VZ\<block>.block`, checked
/// against `game`: refused, each listed, an asset whose name and type `vz.wad` holds, a block whose path
/// `vz.wad` has, and a reference neither the rows nor the game resolve ([`reach`]). The blocks number
/// their rungs from 0 ([`compile`]); `mercs2_formats::patch_wad::rebase_after` renumbers them after the
/// overlay's other blocks.
pub fn compile_vz(root: &Path, assets: &[Asset], game: &GameStack) -> Result<Vec<PatchBlock>, String> {
    let mut bad = Vec::new();
    for a in assets {
        let (_, type_id) = type_by_name(a.asset_type).ok_or_else(|| format!("{}: no asset type is named {:?}", a.name, a.asset_type))?;
        if game.base_has_asset(crate::manifest::asset_hash(a.name), type_id) {
            bad.push(format!("{} ({}): vz.wad holds it, and a level: vz row only adds", a.name, a.asset_type));
        }
    }
    let blocks = compile(root, "VZ", assets, &BTreeMap::new())?;
    for b in &blocks {
        if game.base_has_path(&b.path_string) {
            bad.push(format!("{}: vz.wad has a block at this path", b.path_string));
        }
    }
    let held: BTreeSet<(u32, u32)> = blocks.iter().flat_map(|b| b.aset_entries.iter().map(|e| (e.asset_hash, e.u32_3))).collect();
    bad.extend(unresolved(&reach(&blocks)?, &held, game));
    if !bad.is_empty() {
        return Err(format!("the level: vz rows have {} problem(s):\n  {}", bad.len(), bad.join("\n  ")));
    }
    Ok(blocks)
}

/// Each reference of `refs` that neither `held` nor `game` resolves, and each shader retail does not
/// register, as a line.
fn unresolved(refs: &Reach, held: &BTreeSet<(u32, u32)>, game: &GameStack) -> Vec<String> {
    let mut bad = missing("script", &refs.scripts, |h| held.contains(&(h, TYPE_ID_SCRIPT)) || game.has_asset(h, TYPE_ID_SCRIPT));
    bad.extend(missing("texture", &refs.textures, |h| held.contains(&(h, TYPE_ID_TEXTURE)) || game.has_asset(h, TYPE_ID_TEXTURE)));
    let keys = crate::shader::ShaderKeys::with(&[]);
    bad.extend(missing("pixel shader", &refs.pixel, |h| keys.pixel.contains(&h)));
    bad.extend(missing("vertex shader", &refs.vertex, |h| keys.vertex.contains(&h)));
    bad
}

/// Why `block` cannot be a rung of a level, or `None`. It becomes a filename's stem in the WAD's path
/// table, and its `_P00N` is the rung.
pub fn block_refusal(block: &str) -> Option<String> {
    if block.is_empty() || !block.is_ascii() || block.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c)) {
        return Some(format!("{block:?} is no block stem: it is one ASCII path component"));
    }
    rung_of_path(block).is_none().then(|| format!("{block:?} names no rung: it ends `_P00N_Qn`"))
}

/// Compile `assets` into the level's blocks. Every asset naming one block ships in it, its row in the
/// block of its first rung; blocks run coarsest rung first, by name. An asset's rungs must start at
/// `_P000` and run without a gap, and no block may hold one (name, type) twice.
///
/// An asset whose `(name hash, type hash)` is in `linked` ships those bytes instead of its file: the
/// level's master script, which the link splices qm's hooks onto. Such an asset has one rung.
pub fn compile(root: &Path, level: &str, assets: &[Asset], linked: &BTreeMap<(u32, u32), Vec<u8>>) -> Result<Vec<PatchBlock>, String> {
    let mut held: BTreeMap<(usize, &str), Vec<BlockChunk>> = BTreeMap::new();
    for a in assets {
        let (type_hash, _) = type_by_name(a.asset_type).ok_or_else(|| format!("{}: no asset type is named {:?}", a.name, a.asset_type))?;
        let name = crate::manifest::asset_hash(a.name);
        let replaced = linked.get(&(name, type_hash));
        if replaced.is_some() && a.rungs.len() != 1 {
            return Err(format!("{}: the link replaces its bytes, and it has {} rungs, not 1", a.name, a.rungs.len()));
        }
        // A rung's table word is the bytes of the rungs before it: all 1,236 model and 9,562 texture
        // chains of the retail WAD.
        let mut below = 0u32;
        for (i, r) in a.rungs.iter().enumerate() {
            if let Some(why) = block_refusal(&r.block) {
                return Err(format!("{}: {why}", a.name));
            }
            let rung = rung_of_path(&r.block).expect("block_refusal saw a rung");
            if rung != i {
                return Err(format!("{}: rung {i} of its chain is in {}, a block of rung {rung}", a.name, r.block));
            }
            let bytes = match replaced {
                Some(b) => b.clone(),
                None => {
                    let at = root.join(&r.file);
                    std::fs::read(&at).map_err(|e| format!("{}: {e}", at.display()))?
                }
            };
            let rows = held.entry((rung, r.block.as_str())).or_default();
            if rows.iter().any(|c| c.0 == name && c.1 == type_hash) {
                return Err(format!("{}: {} holds it twice", a.name, r.block));
            }
            let size = bytes.len() as u32;
            rows.push((name, type_hash, below, bytes));
            below += size;
        }
    }
    let at: BTreeMap<&str, u32> = held.keys().enumerate().map(|(i, (_, b))| (*b, i as u32)).collect();
    let mut rows: BTreeMap<&str, Vec<AsetEntry>> = BTreeMap::new();
    for a in assets.iter().filter(|a| !a.rungs.is_empty()) {
        let (_, type_id) = type_by_name(a.asset_type).expect("checked above");
        let slot = |r: usize| a.rungs.get(r).map(|x| at[x.block.as_str()]);
        let (secondary, finer) = rung_chain([slot(0), slot(1), slot(2), slot(3)]);
        rows.entry(a.rungs[0].block.as_str()).or_default().push(AsetEntry::new(crate::manifest::asset_hash(a.name), secondary, finer, type_id));
    }
    let mut blocks = Vec::with_capacity(held.len());
    for (i, ((rung, stem), chunks)) in held.iter().enumerate() {
        let mut raw = (chunks.len() as u32).to_le_bytes().to_vec();
        for (name, ty, word, bytes) in chunks {
            for w in [*name, *ty, *word, bytes.len() as u32] {
                raw.extend_from_slice(&w.to_le_bytes());
            }
        }
        for (.., bytes) in chunks {
            raw.extend_from_slice(bytes);
        }
        let path = format!("blocks\\{level}\\{stem}.block");
        let entries = rows.remove(stem).unwrap_or_default();
        let mut block = PatchBlock::from_decompressed(&raw, path, entries, Some((*rung as u32) << 24))?;
        block.source_block_index = Some(i as u32);
        blocks.push(block);
    }
    Ok(blocks)
}

/// Read and check level `name` from its assets and streams, under `root`, against `game`.
pub fn read(root: &Path, name: &str, assets: &[Asset], streams: &[PathBuf], game: &GameStack) -> Result<Level, String> {
    let blocks = compile(root, name, assets, &BTreeMap::new())?;
    let rows = || blocks.iter().flat_map(|b| b.aset_entries.iter().map(|e| (e.asset_hash, e.u32_3)));
    if !holds_level(rows(), name) {
        return Err(format!("the level_asset rows hold no script {name:?} and layer \"{name}_base\", so they are not level {name:?}"));
    }
    let held: BTreeSet<(u32, u32)> = rows().collect();
    let refs = reach(&blocks)?;
    let mut bad = unresolved(&refs, &held, game);
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
    Ok(Level { blocks, streams: out, references, from_base: references - own, pixel_shaders: refs.pixel.len(), vertex_shaders: refs.vertex.len() })
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

    fn rung(block: &str, file: &str) -> LevelRung {
        LevelRung { block: block.to_string(), file: PathBuf::from(file) }
    }

    /// `(root, its chunk files)`: a scratch folder with three one-chunk files.
    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("qm_level_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for (f, b) in [("a", b"aaaa"), ("b", b"bbbb"), ("c", b"cccc")] {
            std::fs::write(root.join(f), b).unwrap();
        }
        root
    }

    #[test]
    fn assets_of_one_block_compile_into_it_with_the_chain_on_the_coarsest_row() {
        let root = scratch("chain");
        let (a, b) = ([rung("x_P000_Q3", "a"), rung("x_P001_Q2", "b")], [rung("x_P000_Q3", "c")]);
        let assets = [
            Asset { asset_type: "model", name: "0x00000001", rungs: &a },
            Asset { asset_type: "texture", name: "0x00000002", rungs: &b },
        ];
        let blocks = compile(&root, "lvl", &assets, &BTreeMap::new()).unwrap();
        assert_eq!(blocks.iter().map(|b| b.path_string.as_str()).collect::<Vec<_>>(), [r"blocks\lvl\x_P000_Q3.block", r"blocks\lvl\x_P001_Q2.block"]);
        assert_eq!(blocks.iter().map(|b| b.packed_field >> 24).collect::<Vec<_>>(), [0, 1], "the tier is the rung");
        let raw = mercs2_formats::sges::decompress_sges(&blocks[0].compressed_data).unwrap();
        let chunks = mercs2_formats::ucfx::block_chunk_spans(&raw).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!((chunks[0].0.field_c, &raw[chunks[0].1.clone()]), (0, &b"aaaa"[..]));
        let rows = &blocks[0].aset_entries;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].asset_hash, 1);
        assert_eq!((rows[0].u32_1, rows[0].u32_2), (0xFFFF_FFFF, 1), "rung 1 is block 1, and the coarsest block holds the row");
        assert!(blocks[1].aset_entries.is_empty());
    }

    #[test]
    fn a_chain_must_start_at_the_coarsest_rung_and_run_in_order() {
        let root = scratch("order");
        let late = [rung("x_P001_Q2", "a")];
        let e = compile(&root, "lvl", &[Asset { asset_type: "model", name: "a", rungs: &late }], &BTreeMap::new()).unwrap_err();
        assert!(e.contains("rung 0"), "{e}");
    }

    /// The link's bytes for an asset (the level's master with qm's hooks spliced on) ship in place of its
    /// file, and only for an asset of one rung.
    #[test]
    fn an_asset_the_link_replaced_ships_the_link_s_bytes() {
        let root = scratch("linked");
        let one = [rung("x_P000_Q3", "a")];
        let (ty, _) = type_by_name("script").expect("script type");
        let linked = BTreeMap::from([((crate::manifest::asset_hash("lvl"), ty), b"spliced".to_vec())]);
        let blocks = compile(&root, "lvl", &[Asset { asset_type: "script", name: "lvl", rungs: &one }], &linked).unwrap();
        let raw = mercs2_formats::sges::decompress_sges(&blocks[0].compressed_data).unwrap();
        let chunks = mercs2_formats::ucfx::block_chunk_spans(&raw).unwrap();
        assert_eq!(&raw[chunks[0].1.clone()], &b"spliced"[..]);
        let two = [rung("x_P000_Q3", "a"), rung("x_P001_Q2", "b")];
        let e = compile(&root, "lvl", &[Asset { asset_type: "script", name: "lvl", rungs: &two }], &linked).unwrap_err();
        assert!(e.contains("has 2 rungs, not 1"), "{e}");
    }

    #[test]
    fn a_block_may_not_hold_an_asset_twice_or_an_unknown_type() {
        let root = scratch("twice");
        let r = [rung("x_P000_Q3", "a")];
        let twice = [Asset { asset_type: "model", name: "a", rungs: &r }, Asset { asset_type: "model", name: "a", rungs: &r }];
        assert!(compile(&root, "lvl", &twice, &BTreeMap::new()).unwrap_err().contains("twice"));
        assert!(compile(&root, "lvl", &[Asset { asset_type: "nothing", name: "a", rungs: &r }], &BTreeMap::new()).is_err());
    }

    #[test]
    fn a_rung_table_word_is_the_bytes_of_the_rungs_before_it() {
        let root = scratch("rung_words");
        for (file, bytes) in [("src/a", &[0u8; 24][..]), ("src/b", &[0u8; 40][..])] {
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::write(root.join(file), bytes).unwrap();
        }
        let rungs = [rung("x_P000_Q3", "src/a"), rung("x_P001_Q2", "src/b")];
        let blocks = compile(&root, "lv", &[Asset { asset_type: "model", name: "0x00000001", rungs: &rungs }], &BTreeMap::new()).unwrap();
        let word = |b: &PatchBlock| {
            let raw = mercs2_formats::sges::decompress_sges(&b.compressed_data).unwrap();
            u32::from_le_bytes(raw[12..16].try_into().unwrap())
        };
        assert_eq!(blocks.iter().map(word).collect::<Vec<_>>(), [0, 24]);
    }

    #[test]
    fn every_draw_of_a_terrain_chunk_names_its_vertex_shader() {
        use mercs2_formats::ucfx::UcfxNode;
        let draw = |key: u32| [key.to_le_bytes().to_vec(), vec![0; 16]].concat();
        let prmt = UcfxNode::leaf(*b"PRMT", [draw(0xAA), draw(0xBB)].concat());
        let prmg = UcfxNode { tag: *b"PRMG", body: None, children: vec![prmt] };
        assert_eq!(draw_shaders(&[prmg]), [0xAA, 0xBB]);
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
