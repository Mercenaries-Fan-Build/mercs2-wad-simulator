//! `chunk-compare` — every chunk of a big-endian archive, converted, against its PC twin.
//!
//! Each block goes through `convert_block`; each chunk joins the PC bake's by (name, type); each row
//! compares byte for byte under its path in the UCFX tree (`MIXR/NAME`). A row of one tree that the
//! other lacks counts too. The result lists every converter gap at once: one row per chunk type and
//! path, with how many bodies are equal, differ, or differ in size, so a console chunk read wrongly
//! shows up without waiting for something to fail on it.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::Ordering;

use mercs2_formats::be_to_le::convert::{convert_block, QUIET};
use mercs2_formats::be_to_le::ecs::ComponentSchemas;
use mercs2_formats::be_to_le::lua::LuaTools;
use mercs2_formats::be_to_le::texture::{archive_textures, rung_of_path, BlockTables, TextureTable};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::type_name_from_hash;
use mercs2_formats::ucfx::{block_chunk_spans, parse_ucfx_tree, UcfxNode};

#[derive(Default)]
struct Tally {
    equal: usize,
    differ: usize,
    size: usize,
    only_console: usize,
    only_pc: usize,
    /// One example of a differing body: chunk name.
    example: Option<u32>,
}

fn walk<'a>(nodes: &'a [UcfxNode], prefix: &str, out: &mut BTreeMap<String, Vec<&'a [u8]>>) {
    for n in nodes {
        let path = if prefix.is_empty() { n.tag_str() } else { format!("{prefix}/{}", n.tag_str()) };
        if let Some(b) = &n.body {
            out.entry(path.clone()).or_default().push(b);
        }
        walk(&n.children, &path, out);
    }
}

/// Paths of the input and tools, and `only`, a type name that limits the run to blocks holding it.
pub struct Inputs<'a> {
    pub archive: &'a Path,
    pub vz_wad: &'a Path,
    pub java: &'a Path,
    pub unluac: &'a Path,
    pub only: Option<&'a str>,
}

pub fn chunk_compare(i: Inputs, out: &Path) -> Result<(), String> {
    let Inputs { archive, vz_wad, java, unluac, only } = i;
    QUIET.store(true, Ordering::Relaxed);
    let schemas = ComponentSchemas::from_vz_wad(vz_wad)?;
    let lua = LuaTools { java: java.to_path_buf(), unluac: unluac.to_path_buf() };
    let mut f = File::open(vz_wad).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    // A name repeats across its LOD rungs, so a chunk's twin is the one in the block of the same path.
    let mut homes: HashMap<(u32, u32), Vec<u16>> = HashMap::new();
    for b in 0..pc.indx.len() {
        let dec = decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("pc block {b}: {e}"))?;
        for (e, _) in block_chunk_spans(&dec).map_err(|e| format!("pc block {b}: {e}"))? {
            homes.entry((e.name_hash, e.type_hash)).or_default().push(b as u16);
        }
    }
    let same_path = |b: u16, path: &str| pc.paths.get(b as usize).is_some_and(|p| p.eq_ignore_ascii_case(path));
    println!("{} PC chunks in {} blocks", homes.len(), pc.indx.len());
    let (doh, _) = load_stfs_or_doh(archive)?;
    let (_, rows) = parse_be_ffcs(&doh)?;
    let indx = rows.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = rows.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    let textures = TextureTable::from_doh(&doh, &entries, &paths, &archive_textures(vz_wad)?)?;
    let mut tallies: BTreeMap<(String, String), Tally> = BTreeMap::new();
    let mut refused: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pc_block: Option<(u16, Vec<u8>)> = None;
    let (mut no_twin, mut unreadable) = (0usize, BTreeMap::<String, usize>::new());
    for (bi, e) in entries.iter().enumerate() {
        let path = paths.get(bi).map_or("", String::as_str);
        let wanted = |b: &[u8]| {
            only.is_none_or(|ty| {
                (0..u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).any(|i| {
                    let o = 8 + i * 16;
                    type_name_from_hash(u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])) == ty
                })
            })
        };
        let be = match read_be_block(&doh, e) {
            Ok(b) if !wanted(&b) => continue,
            other => other,
        };
        let block_textures = BlockTables { textures: &textures, rung: rung_of_path(path), streams: None };
        let le = match be.and_then(|b| convert_block(&b, false, None, Some(&lua), Some(&schemas), Some(block_textures))) {
            Ok(le) => le,
            Err(err) => {
                let class: String = err.chars().take(60).collect();
                refused.entry(class).or_default().push(format!("[{bi}] {path}"));
                continue;
            }
        };
        for (entry, range) in block_chunk_spans(&le).map_err(|err| format!("block {bi}: {err}"))? {
            let Some(blocks) = homes.get(&(entry.name_hash, entry.type_hash)) else {
                no_twin += 1;
                continue;
            };
            let home = blocks.iter().copied().find(|&b| same_path(b, path)).unwrap_or(blocks[0]);
            if pc_block.as_ref().map(|(b, _)| *b) != Some(home) {
                pc_block = Some((home, decompress_block(&mut f, &pc.indx, home).map_err(|err| format!("pc block {home}: {err}"))?));
            }
            let dec = &pc_block.as_ref().expect("set above").1;
            let spans = block_chunk_spans(dec).map_err(|err| format!("pc block {home}: {err}"))?;
            let twin = spans
                .iter()
                .find(|(t, _)| t.name_hash == entry.name_hash && t.type_hash == entry.type_hash)
                .map(|(_, r)| &dec[r.clone()])
                .ok_or("a PC home lost its chunk")?;
            let type_name = type_name_from_hash(entry.type_hash).to_string();
            let (a, b) = match (parse_ucfx_tree(&le[range]), parse_ucfx_tree(twin)) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(err), _) | (_, Err(err)) => {
                    *unreadable.entry(format!("{type_name}: {}", err.chars().take(60).collect::<String>())).or_default() += 1;
                    continue;
                }
            };
            let (mut x, mut y) = (BTreeMap::new(), BTreeMap::new());
            walk(&a, "", &mut x);
            walk(&b, "", &mut y);
            for (p, bodies) in &x {
                let t = tallies.entry((type_name.clone(), p.clone())).or_default();
                let Some(twins) = y.get(p) else {
                    t.only_console += bodies.len();
                    continue;
                };
                for (i, body) in bodies.iter().enumerate() {
                    match twins.get(i) {
                        None => t.only_console += 1,
                        Some(w) if w == body => t.equal += 1,
                        // A collider's only padding bytes differ per bake, unread by anything.
                        Some(w) if p.ends_with("PHY2") && w.len() == body.len() && {
                            let pads = mercs2_formats::be_to_le::havok::phy2_pool_padding(w);
                            (0..w.len()).all(|o| w[o] == body[o] || pads.binary_search(&o).is_ok())
                        } => t.equal += 1,
                        Some(w) if w.len() != body.len() => {
                            t.size += 1;
                            t.example.get_or_insert(entry.name_hash);
                        }
                        Some(_) => {
                            t.differ += 1;
                            t.example.get_or_insert(entry.name_hash);
                        }
                    }
                }
                t.only_pc += twins.len().saturating_sub(bodies.len());
            }
            for (p, bodies) in &y {
                if !x.contains_key(p) {
                    tallies.entry((type_name.clone(), p.clone())).or_default().only_pc += bodies.len();
                }
            }
        }
        if bi % 1000 == 0 {
            println!("block {bi} of {}", entries.len());
        }
    }
    let mut tsv = String::from("type\tpath\tequal\tdiffer\tsize\tonly_console\tonly_pc\texample\n");
    for ((ty, path), t) in &tallies {
        let ex = t.example.map_or(String::new(), |h| format!("0x{h:08X}"));
        tsv.push_str(&format!("{ty}\t{path}\t{}\t{}\t{}\t{}\t{}\t{ex}\n", t.equal, t.differ, t.size, t.only_console, t.only_pc));
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("no PC twin: {no_twin} chunks");
    for (class, blocks) in &refused {
        println!("refused {} blocks, e.g. {}: {class}", blocks.len(), blocks[0]);
    }
    for (class, n) in &unreadable {
        println!("unreadable {n}: {class}");
    }
    Ok(())
}
