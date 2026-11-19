//! The DLC as components: one PC-form file per asset rung, which `qm` compiles back into a level.
//!
//! Every chunk converts on its own, so a chunk one archive cannot convert comes from the other without
//! touching what shares its block, all its rungs together: the two chains need not match.
//!
//! A chunk is carried when the base lacks its (name, type), or when its type is one a level overrides
//! and its bytes are not PC's. Models, textures, animations and paths are left to the base when it holds
//! them: each platform baked its own, so their bytes say nothing (`docs/modernization/blow_it_up_again.md`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::Ordering;

use mercs2_formats::be_to_le::audio::StreamTable;
use mercs2_formats::be_to_le::texture::{archive_textures, rung_of_path, BlockTables, TextureTable};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block, IndxEntry};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use ucfx_byteswap::convert::QUIET;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{
    type_name_from_hash, TYPE_HASH_CFX_PACK, TYPE_HASH_FOLIAGE, TYPE_HASH_GUIDMAP, TYPE_HASH_SCRIPT, TYPE_HASH_STANCE,
    TYPE_HASH_WATERMAP, TYPE_HASH_WORLD_ENTITY_DATA,
};
use mercs2_formats::ucfx::block_chunk_spans;
use ucfx_byteswap::convert::convert_block;
use ucfx_byteswap::ecs::ComponentSchemas;

use crate::xbox_doh_oracle::is_ps3_compact_decl_rejection;

/// `sounddb`: `TYPE_HASH_REGISTRY` row 13.
const TYPE_HASH_SOUNDDB: u32 = 0xE527_3C14;

/// The types a level overrides under the base's names: each is a singleton or a script, so its bytes
/// are the level's own.
const OVERRIDES: [u32; 8] = [
    TYPE_HASH_SCRIPT,
    TYPE_HASH_STANCE,
    TYPE_HASH_CFX_PACK,
    TYPE_HASH_GUIDMAP,
    TYPE_HASH_FOLIAGE,
    TYPE_HASH_WATERMAP,
    TYPE_HASH_WORLD_ENTITY_DATA,
    TYPE_HASH_SOUNDDB,
];

/// One asset rung in PC form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub name: u32,
    pub ty: u32,
    /// The block's path stem, `c30457-c20204_P001_Q2`.
    pub block: String,
    pub bytes: Vec<u8>,
}

/// A big-endian archive and what its chunks need to convert.
pub struct Archive<'a> {
    pub doh: &'a [u8],
    pub indx: &'a [IndxEntry],
    pub paths: &'a [String],
    pub textures: &'a TextureTable,
    pub streams: Option<&'a StreamTable>,
}

/// What converted, and the (name, type) of each chunk the converter refused as a PS3 compact vertex
/// stream. Any other refusal is an error.
pub struct Converted {
    pub chunks: Vec<Chunk>,
    pub refused: HashSet<(u32, u32)>,
}

/// The path stem of a block: `blocks\dlc01\x_P000_Q3.block` is `x_P000_Q3`.
pub fn block_stem(path: &str) -> Result<String, String> {
    let file = path.rsplit('\\').next().unwrap_or(path);
    file.strip_suffix(".block").map(str::to_string).ok_or_else(|| format!("{path} is not a block path"))
}

fn be32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("short read at +0x{o:X}"))
}

/// Convert every chunk `wanted` accepts, one at a time.
pub fn convert(a: &Archive, schemas: &ComponentSchemas, wanted: impl Fn(u32, u32) -> bool) -> Result<Converted, String> {
    let (mut chunks, mut refused) = (Vec::new(), HashSet::new());
    for (bi, e) in a.indx.iter().enumerate() {
        let path = a.paths.get(bi).ok_or_else(|| format!("block {bi} has no path"))?;
        let be = read_be_block(a.doh, e).map_err(|err| format!("{path}: {err}"))?;
        let rung = rung_of_path(path).ok_or_else(|| format!("{path} names no rung"))?;
        let block = block_stem(path)?;
        let count = be32(&be, 0)? as usize;
        let mut off = 4 + count * 16;
        for i in 0..count {
            let row = 4 + i * 16;
            let (name, ty, size) = (be32(&be, row)?, be32(&be, row + 4)?, be32(&be, row + 12)? as usize);
            let chunk = be.get(off..off + size).ok_or_else(|| format!("{path}: chunk 0x{name:08X} runs past the block"))?;
            off += size;
            if !wanted(name, ty) {
                continue;
            }
            let mut one = Vec::with_capacity(20 + size);
            one.extend_from_slice(&1u32.to_be_bytes());
            one.extend_from_slice(&be[row..row + 16]);
            one.extend_from_slice(chunk);
            let tables = BlockTables { textures: a.textures, rung: Some(rung), streams: a.streams };
            let le = match convert_block(&one, false, None, Some(schemas), Some(tables)) {
                Ok(le) => le,
                Err(err) if is_ps3_compact_decl_rejection(&err) => {
                    refused.insert((name, ty));
                    continue;
                }
                Err(err) => return Err(format!("{path}: chunk 0x{name:08X}: {err}")),
            };
            // A texture stub converts to nothing: the base game serves it.
            for (_, range) in block_chunk_spans(&le).map_err(|err| format!("{path}: {err}"))? {
                chunks.push(Chunk { name, ty, block: block.clone(), bytes: le[range].to_vec() });
            }
        }
    }
    Ok(Converted { chunks, refused })
}

/// Replace every chunk of each `refused` key with `donor`'s, the whole chain.
pub fn substitute(chunks: &mut Vec<Chunk>, refused: &HashSet<(u32, u32)>, donor: Vec<Chunk>) -> Result<(), String> {
    chunks.retain(|c| !refused.contains(&(c.name, c.ty)));
    let held: HashSet<(u32, u32)> = donor.iter().map(|c| (c.name, c.ty)).collect();
    if let Some((n, t)) = refused.iter().find(|k| !held.contains(k)) {
        return Err(format!("the second archive has no {} 0x{n:08X}, which the first cannot convert", type_name_from_hash(*t)));
    }
    chunks.extend(donor);
    Ok(())
}

/// Every chunk of a PC archive, by (name, type), with its bytes when `keep` accepts the type.
fn base_chunks(wad: &Path, keep: impl Fn(u32) -> bool) -> Result<HashMap<(u32, u32), Vec<Vec<u8>>>, String> {
    let mut f = File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut out: HashMap<(u32, u32), Vec<Vec<u8>>> = HashMap::new();
    for b in 0..pc.indx.len() {
        let dec = decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("pc block {b}: {e}"))?;
        for (e, range) in block_chunk_spans(&dec).map_err(|e| format!("pc block {b}: {e}"))? {
            let bodies = out.entry((e.name_hash, e.type_hash)).or_default();
            if keep(e.type_hash) {
                bodies.push(dec[range].to_vec());
            }
        }
    }
    Ok(out)
}

/// The chunks a level carries, of every chunk the archive holds.
pub fn carried(chunks: Vec<Chunk>, vz_wad: &Path) -> Result<Vec<Chunk>, String> {
    let base = base_chunks(vz_wad, |t| OVERRIDES.contains(&t))?;
    Ok(chunks
        .into_iter()
        .filter(|c| match base.get(&(c.name, c.ty)) {
            None => true,
            Some(twins) => OVERRIDES.contains(&c.ty) && !twins.iter().any(|t| *t == c.bytes),
        })
        .collect())
}

/// One package's DLC in PC form: the chunks a level carries over the player's `vz.wad`, and its
/// streams re-encoded.
pub struct Ported {
    pub chunks: Vec<Chunk>,
    pub streams: StreamTable,
}

/// Convert a package's big-endian archive `doh` and the `package_files` beside it into what a level
/// carries over `vz_wad`. A chunk the archive cannot convert (a PS3 compact vertex stream) comes from
/// `donor`, a second archive of the same DLC; without one, any such chunk fails the run, naming how
/// many of each type.
pub fn port(doh: &[u8], package_files: &[(String, Vec<u8>)], vz_wad: &Path, donor: Option<&Path>) -> Result<Ported, String> {
    let schemas = ComponentSchemas::from_vz_wad(vz_wad)?;
    let base_textures = archive_textures(vz_wad)?;
    let (_, rows) = parse_be_ffcs(doh)?;
    let indx_row = rows.iter().find(|r| r.tag == "INDX").ok_or("Missing INDX")?;
    let num_blocks = indx_row.meta as usize;
    let indx = parse_be_indx(doh, indx_row.offset as usize, num_blocks);
    let pths = rows.iter().find(|r| r.tag == "PTHS").map(|r| parse_be_pths(doh, r.offset as usize, r.meta as usize)).unwrap_or_default();
    let textures = TextureTable::from_doh(doh, &indx, &pths, &base_textures)?;
    let streams = StreamTable::read(num_blocks, |i| read_be_block(doh, &indx[i]), package_files)?;
    QUIET.store(true, Ordering::Relaxed);
    let source = Archive { doh, indx: &indx, paths: &pths, textures: &textures, streams: Some(&streams) };
    let mut converted = convert(&source, &schemas, |_, _| true)?;
    if !converted.refused.is_empty() {
        let Some(second) = donor else {
            let mut kinds = BTreeMap::new();
            for (_, ty) in &converted.refused {
                *kinds.entry(type_name_from_hash(*ty)).or_insert(0usize) += 1;
            }
            return Err(format!("{} chunks do not convert from this package ({kinds:?}) and no second archive was named", converted.refused.len()));
        };
        let (xbox, _) = load_stfs_or_doh(second)?;
        let (_, tables) = parse_be_ffcs(&xbox)?;
        let row = tables.iter().find(|r| r.tag == "INDX").ok_or("Missing INDX")?;
        let x_indx = parse_be_indx(&xbox, row.offset as usize, row.meta as usize);
        let x_pths = tables.iter().find(|r| r.tag == "PTHS").map(|r| parse_be_pths(&xbox, r.offset as usize, r.meta as usize)).unwrap_or_default();
        let x_textures = TextureTable::from_doh(&xbox, &x_indx, &x_pths, &base_textures)?;
        let x = Archive { doh: &xbox, indx: &x_indx, paths: &x_pths, textures: &x_textures, streams: None };
        let refused = converted.refused.clone();
        let donated = convert(&x, &schemas, |n, t| refused.contains(&(n, t)))?;
        substitute(&mut converted.chunks, &refused, donated.chunks)?;
    }
    Ok(Ported { chunks: carried(converted.chunks, vz_wad)?, streams })
}

/// One asset's rungs in manifest order: `(block stem, file)`, finest last.
pub type Rungs = Vec<(String, String)>;

/// What [`write_components`] wrote, as `src/`-relative paths: each asset's rungs by `(type name, name
/// hash)`, and each stream.
pub struct Written {
    pub assets: BTreeMap<(String, u32), Rungs>,
    pub streams: Vec<String>,
}

/// Why a stream's name, which comes from the package's file table, cannot be a file name in one folder:
/// empty, `.` or `..`, or holding a `/`, `\`, `:` or NUL, on any host.
pub fn stream_name_refusal(name: &str) -> Option<&'static str> {
    if name.is_empty() || name == "." || name == ".." {
        Some("not a file name")
    } else if name.contains(['/', '\\', ':', '\0']) {
        Some("holds a path separator, `:` or NUL")
    } else {
        None
    }
}

/// Write `chunks` and `streams` under `root`: one file per asset rung at
/// `src/level/<type>/0x<name>.<rung>.ucfx`, and each stream at `src/level/Audios/<name>`. A stream name
/// [`stream_name_refusal`] refuses fails the call before anything is written.
pub fn write_components(root: &Path, chunks: &[Chunk], streams: &StreamTable) -> Result<Written, String> {
    for (name, _) in streams.streams() {
        if let Some(why) = stream_name_refusal(name) {
            return Err(format!("stream {name:?}: {why}"));
        }
    }
    let mut assets: BTreeMap<(String, u32), Rungs> = BTreeMap::new();
    for c in chunks {
        let ty = type_name_from_hash(c.ty);
        if ty == "unknown" {
            return Err(format!("chunk 0x{:08X} has type 0x{:08X}, which has no name", c.name, c.ty));
        }
        let rung = rung_of_path(&c.block).ok_or_else(|| format!("{} names no rung", c.block))?;
        let relative = format!("src/level/{ty}/0x{:08X}.{rung}.ucfx", c.name);
        let to = root.join(&relative);
        let dir = to.parent().ok_or("a file has a parent")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&to, &c.bytes).map_err(|e| format!("{}: {e}", to.display()))?;
        assets.entry((ty.to_string(), c.name)).or_default().push((c.block.clone(), relative));
    }
    for rungs in assets.values_mut() {
        rungs.sort_by(|a, b| a.1.cmp(&b.1));
    }
    let audios = root.join("src").join("level").join("Audios");
    let mut written = Vec::new();
    for (name, stream) in streams.streams() {
        std::fs::create_dir_all(&audios).map_err(|e| format!("{}: {e}", audios.display()))?;
        std::fs::write(audios.join(name), &stream.pws).map_err(|e| format!("{name}: {e}"))?;
        written.push(format!("src/level/Audios/{name}"));
    }
    Ok(Written { assets, streams: written })
}

/// The line that ends a manifest's authored contributions: the rows above it (the Shipment's own plugin, its
/// requirements' rows) are kept, and every row below it is written here.
pub const GENERATED_MARKER: &str = "  # dlc_port --package writes every row below this line.";

/// `text` with its contributions after [`GENERATED_MARKER`] replaced by `rows`. A manifest without the
/// marker has no authored rows: everything after `contributions:` is replaced and the marker added.
pub fn with_generated_rows(text: &str, rows: &str) -> Result<String, String> {
    let (head, body) = text.split_once("\ncontributions:\n").ok_or("the manifest has no contributions line")?;
    let authored = match body.split_once(GENERATED_MARKER) {
        Some((authored, _)) => authored,
        None => "",
    };
    Ok(format!("{head}\ncontributions:\n{authored}{GENERATED_MARKER}\n{rows}"))
}

/// Write `chunks` and `streams` under `root` and the manifest's generated contributions
/// ([`with_generated_rows`]).
pub fn write_shipment(root: &Path, level: &str, display: &str, chunks: &[Chunk], streams: &StreamTable) -> Result<(), String> {
    let written = write_components(root, chunks, streams)?;
    let mut rows = format!("  - kind: add_level\n    name: {level}\n    display: {display}\n    streams:\n");
    for stream in &written.streams {
        rows += &format!("      - {stream}\n");
    }
    for ((ty, name), rungs) in written.assets {
        rows += &format!("  - kind: level_asset\n    level: {level}\n    type: {ty}\n    name: \"0x{name:08X}\"\n    rungs:\n");
        for (block, file) in rungs {
            rows += &format!("      - {{ block: {block}, file: {file} }}\n");
        }
    }
    let manifest = root.join("manifest.yaml");
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let out = with_generated_rows(&text, &rows).map_err(|e| format!("{}: {e}", manifest.display()))?;
    std::fs::write(&manifest, out).map_err(|e| format!("{}: {e}", manifest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(name: u32, block: &str) -> Chunk {
        Chunk { name, ty: 1, block: block.to_string(), bytes: vec![name as u8] }
    }

    #[test]
    fn a_block_stem_drops_the_folder_and_extension() {
        assert_eq!(block_stem(r"blocks\dlc01\c30457-c20204_P001_Q2.block").unwrap(), "c30457-c20204_P001_Q2");
        assert!(block_stem("nothing").is_err());
    }

    #[test]
    fn a_substituted_key_takes_the_donors_whole_chain() {
        let mut chunks = vec![chunk(1, "a_P000_Q3"), chunk(2, "b_P000_Q3"), chunk(2, "b_P001_Q2")];
        let refused = HashSet::from([(2, 1)]);
        let donor = vec![chunk(2, "c_P000_Q3"), chunk(2, "c_P001_Q2"), chunk(2, "c_P002_Q1")];
        substitute(&mut chunks, &refused, donor).unwrap();
        assert_eq!(chunks.iter().filter(|c| c.name == 1).count(), 1);
        let blocks: Vec<&str> = chunks.iter().filter(|c| c.name == 2).map(|c| c.block.as_str()).collect();
        assert_eq!(blocks, ["c_P000_Q3", "c_P001_Q2", "c_P002_Q1"]);
    }

    #[test]
    fn a_chunk_the_donor_lacks_fails_the_run() {
        let mut chunks = vec![chunk(2, "b_P000_Q3")];
        let refused = HashSet::from([(2, 1)]);
        assert!(substitute(&mut chunks, &refused, Vec::new()).is_err());
    }

    #[test]
    fn a_regenerate_keeps_the_authored_rows_and_replaces_the_rest() {
        let head = "format: 2\nload:\n  requires:\n    - m2-sdk\n";
        let authored = "  - kind: native_hook\n    target: retail\n    plugin: src/x.asi\n    touches: [\"Net.F\"]\n";
        let before = format!("{head}\ncontributions:\n{authored}{GENERATED_MARKER}\n  - kind: level_asset\n    name: old\n");
        let after = with_generated_rows(&before, "  - kind: level_asset\n    name: new\n").unwrap();
        assert_eq!(after, format!("{head}\ncontributions:\n{authored}{GENERATED_MARKER}\n  - kind: level_asset\n    name: new\n"));
        assert_eq!(with_generated_rows(&after, "  - kind: level_asset\n    name: new\n").unwrap(), after);
    }

    #[test]
    fn a_stream_name_is_one_file_name_on_every_host() {
        assert_eq!(stream_name_refusal("vo_stream_dlctest.english.pws"), None);
        for name in ["", ".", "..", "../../x.pws", "a/b.pws", r"..\..\x.pws", r"C:\x.pws", "c:x.pws", "/x.pws", "x\0.pws"] {
            assert!(stream_name_refusal(name).is_some(), "{name:?}");
        }
    }

    #[test]
    fn a_manifest_without_the_marker_has_no_authored_rows() {
        let after = with_generated_rows("format: 2\n\ncontributions:\n  - kind: level_asset\n", "  - kind: add_level\n").unwrap();
        assert_eq!(after, format!("format: 2\n\ncontributions:\n{GENERATED_MARKER}\n  - kind: add_level\n"));
        assert!(with_generated_rows("format: 2\n", "").is_err());
    }
}
