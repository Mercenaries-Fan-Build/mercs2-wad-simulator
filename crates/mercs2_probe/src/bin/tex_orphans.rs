//! Which textures does NO material cite? The census side of `tex_users`.
//!
//! Every block's entry table is walked (not the ASET table). Each container whose loader reads an
//! `MTRL` (model, terrainmesh, font, lowresterrain, scrub — `MtrlSource`) is parsed, and every
//! texture hash it cites is collected. A texture chunk whose hash no MTRL cites is an orphan.
//!
//! An orphan is not proven unused: UI, FX, sky and decal code can take a texture by hash outside any
//! MTRL. So for each orphan the remaining chunks are also searched for the hash as an aligned LE u32
//! and the holders' type hashes are printed — that says the value occurs, not what field it is.
//!
//! Any OTHER container type that carries an `MTRL` row is not parsed (its loader is not decoded) and
//! is listed; it makes the run exit 1, because its citations are missing from the census.
//!
//! Exit 0 = every block, every MTRL read; 1 = something could not be read (listed); 2 = usage.
//!
//! ```text
//! cargo run -p mercs2_probe --bin tex_orphans -- [--wad path] [--tsv out.tsv]
//! ```

use mercs2_engine::wad;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::types::TYPE_HASH_TEXTURE;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;

fn mtrl_source(type_hash: u32) -> Option<MtrlSource> {
    match type_hash {
        0x5B72_4250 => Some(MtrlSource::Model),
        0x7C56_9307 => Some(MtrlSource::TerrainMesh),
        0x99E7_7ACE => Some(MtrlSource::Font),
        0x1602_815C => Some(MtrlSource::LowResTerrain),
        0x600B_904E => Some(MtrlSource::Scrub),
        _ => None,
    }
}

/// Whether a UCFX container's descriptor table has an `MTRL` row (the `mtrl_probe` layout:
/// `u32 ndesc @16`, 20-byte rows from @20, tag first).
fn has_mtrl_row(c: &[u8]) -> bool {
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return false;
    }
    let ndesc = u32::from_le_bytes([c[16], c[17], c[18], c[19]]) as usize;
    (0..ndesc).any(|i| {
        let ro = 20 + i * 20;
        ro + 4 <= c.len() && &c[ro..ro + 4] == b"MTRL"
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let tsv = get("--tsv");

    let csv = ["docs/data/aset_names.csv", "../../docs/data/aset_names.csv"]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.is_file())
        .expect("aset_names.csv (run from the repo root or tools/wad_simulator)");
    let text = std::fs::read_to_string(&csv).expect("read aset_names.csv");
    let mut name_of: HashMap<u32, String> = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let Some(h) = f.first().and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        else {
            continue;
        };
        if let Some(n) = f.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            name_of.insert(h, n.to_string());
        }
    }

    let wadpath = wad::resolve_vz_wad(get("--wad").as_deref()).unwrap_or_else(|| {
        eprintln!("tex_orphans: no vz.wad found (pass --wad, or write .mercs2-local.toml)");
        std::process::exit(2);
    });
    let mut w = wad::open(&wadpath).unwrap_or_else(|e| {
        eprintln!("tex_orphans: cannot open {wadpath}: {e}");
        std::process::exit(2);
    });
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    eprintln!("[tex_orphans] pass 1: {} blocks of {wadpath}", paths.len());

    // texture hash -> blocks holding a texture chunk of it
    let mut tex_blocks: BTreeMap<u32, BTreeSet<u16>> = BTreeMap::new();
    let mut cited: HashSet<u32> = HashSet::new();
    let mut cited_by_source: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut unparsed_mtrl: BTreeMap<u32, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();

    // Pass 1: texture inventory + every MTRL citation.
    for (b, path) in paths.iter().enumerate() {
        let b = b as u16;
        let dec = match wad::decompress_block_index(&mut w, b) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("block {b} {path}: decompress: {e}"));
                continue;
            }
        };
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        if entries.len() != count as usize {
            failures.push(format!("block {b} {path}: entry table claims {count}, holds {}", entries.len()));
            continue;
        }
        let mut off = 4 + count as usize * 16;
        for e in &entries {
            let end = off + e.chunk_size as usize;
            if end > dec.len() {
                failures.push(format!("block {b} {path}: chunk 0x{:08X} overruns the block", e.name_hash));
                break;
            }
            let chunk = &dec[off..end];
            off = end;
            if e.type_hash == TYPE_HASH_TEXTURE {
                tex_blocks.entry(e.name_hash).or_default().insert(b);
                continue;
            }
            match mtrl_source(e.type_hash) {
                Some(src) => match parse_mtrl(chunk, src) {
                    Ok(mats) => {
                        *cited_by_source.entry(format!("{src:?}").leak()).or_default() += 1;
                        for m in mats {
                            cited.extend(m.textures.iter().copied().filter(|t| *t != 0 && *t != 0xFFFF_FFFF));
                        }
                    }
                    Err(err) => failures.push(format!(
                        "block {b} {path}: {src:?} 0x{:08X} MTRL: {err}",
                        e.name_hash
                    )),
                },
                None => {
                    if has_mtrl_row(chunk) {
                        *unparsed_mtrl.entry(e.type_hash).or_default() += 1;
                    }
                }
            }
        }
    }

    let orphans: BTreeSet<u32> = tex_blocks.keys().copied().filter(|h| !cited.contains(h)).collect();
    eprintln!(
        "[tex_orphans] {} texture hashes, {} cited by an MTRL, {} orphans; pass 2: raw-u32 search",
        tex_blocks.len(),
        tex_blocks.len() - orphans.len(),
        orphans.len()
    );

    // Pass 2: does any non-texture chunk hold an orphan's hash as an aligned LE u32?
    let mut raw_holders: HashMap<u32, BTreeMap<u32, usize>> = HashMap::new();
    for (b, path) in paths.iter().enumerate() {
        let b = b as u16;
        let dec = match wad::decompress_block_index(&mut w, b) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("block {b} {path}: decompress (pass 2): {e}"));
                continue;
            }
        };
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        let mut off = 4 + count as usize * 16;
        for e in &entries {
            let end = (off + e.chunk_size as usize).min(dec.len());
            let chunk = &dec[off..end];
            off = end;
            if e.type_hash == TYPE_HASH_TEXTURE {
                continue;
            }
            for word in chunk.chunks_exact(4) {
                let v = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
                if orphans.contains(&v) {
                    *raw_holders.entry(v).or_default().entry(e.type_hash).or_default() += 1;
                }
            }
        }
    }

    let label = |h: u32| name_of.get(&h).cloned().unwrap_or_else(|| "(unnamed)".into());
    let leaf = |b: u16| paths[b as usize].rsplit(['\\', '/']).next().unwrap_or("").to_string();

    // Group orphans by the block their first texture chunk lives in (co-location is the evidence
    // that they belong together; no name heuristic).
    let mut by_block: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for h in &orphans {
        let b = *tex_blocks[h].iter().next().expect("texture has a block");
        by_block.entry(b).or_default().push(*h);
    }
    let mut groups: Vec<(u16, Vec<u32>)> = by_block.into_iter().collect();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

    let no_raw = orphans.iter().filter(|h| !raw_holders.contains_key(h)).count();
    println!("textures: {}  cited by an MTRL: {}  orphans: {}", tex_blocks.len(), tex_blocks.len() - orphans.len(), orphans.len());
    println!("  orphans with no raw-u32 holder anywhere: {no_raw}");
    println!("  MTRL containers parsed by source: {cited_by_source:?}");
    println!("  orphan groups (by block): {}", groups.len());

    println!("\n=== ORPHAN GROUPS, largest first ===");
    for (b, hs) in &groups {
        println!("\n-- block {b:5} {}  ({} orphans)", leaf(*b), hs.len());
        for h in hs {
            let raw = raw_holders
                .get(h)
                .map(|m| m.iter().map(|(t, n)| format!("{}x{:08X}", n, t)).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            println!("   0x{h:08X}  {:50} {raw}", label(*h));
        }
    }

    if let Some(p) = &tsv {
        let mut f = std::fs::File::create(p).expect("create --tsv");
        writeln!(f, "hash\tname\tblock\tblock_path\traw_holders").unwrap();
        for (b, hs) in &groups {
            for h in hs {
                let raw = raw_holders
                    .get(h)
                    .map(|m| m.iter().map(|(t, n)| format!("{n}x{t:08X}")).collect::<Vec<_>>().join(" "))
                    .unwrap_or_default();
                writeln!(f, "0x{h:08X}\t{}\t{b}\t{}\t{raw}", label(*h), leaf(*b)).unwrap();
            }
        }
        println!("\nwrote {p}");
    }

    if !unparsed_mtrl.is_empty() {
        failures.push(format!(
            "containers with an MTRL row this census cannot parse (type_hash -> count): {}",
            unparsed_mtrl.iter().map(|(t, n)| format!("0x{t:08X}->{n}")).collect::<Vec<_>>().join(", ")
        ));
    }
    if !failures.is_empty() {
        eprintln!("\n{} item(s) could not be read:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}
