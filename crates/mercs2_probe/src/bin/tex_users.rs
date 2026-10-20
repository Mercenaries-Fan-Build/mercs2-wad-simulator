//! Which models use a texture? The reverse of `mtrl_probe`.
//!
//! A model declares its textures only as hashes inside its own `MTRL`; there is no manifest, so
//! the only way to answer "who uses `al_hum_soldier_elitea_ub`" is to read every model. This walks
//! EVERY block's entry table (not the ASET table), so it reaches every model container at every LOD
//! rung, including containers with no ASET row of their own, and parses each one's `MTRL`.
//!
//! Second pass, reported separately: any OTHER chunk (not a model, not the queried texture itself)
//! whose bytes hold a queried hash as an aligned little-endian u32. That only says the value occurs;
//! the field it sits in is not decoded.
//!
//! Exit 0 = every block and every model MTRL was read; 1 = something could not be read (listed);
//! 2 = usage.
//!
//! ```text
//! cargo run -p mercs2_probe --bin tex_users -- al_hum_soldier_elitea_ub 0x21A0C4D6 [--wad path]
//! ```

use mercs2_engine::wad;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use std::collections::{BTreeMap, HashMap};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wad_arg = args.iter().position(|a| a == "--wad").and_then(|i| args.get(i + 1)).cloned();
    let queries: Vec<u32> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && args[i - 1] == "--wad"))
        .map(|(_, a)| match a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16).unwrap_or_else(|e| {
                eprintln!("tex_users: {a:?} is not a hex hash: {e}");
                std::process::exit(2);
            }),
            None => mercs2_formats::hash::pandemic_hash_m2(a),
        })
        .collect();
    if queries.is_empty() {
        eprintln!("usage: tex_users <texture name|0xHASH>... [--wad <path>]");
        std::process::exit(2);
    }

    // hash -> name, from the ASET export, to label what the scan finds.
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
    let label = |h: u32| name_of.get(&h).cloned().unwrap_or_else(|| "(unnamed)".into());

    let wadpath = wad::resolve_vz_wad(wad_arg.as_deref()).unwrap_or_else(|| {
        eprintln!("tex_users: no vz.wad found (pass --wad, or write .mercs2-local.toml)");
        std::process::exit(2);
    });
    let mut w = wad::open(&wadpath).unwrap_or_else(|e| {
        eprintln!("tex_users: cannot open {wadpath}: {e}");
        std::process::exit(2);
    });

    for q in &queries {
        println!("query 0x{q:08X}  {}", label(*q));
    }
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    eprintln!("[tex_users] scanning {} blocks of {wadpath}", paths.len());

    // query -> [(model hash, block, material indices)]
    let mut model_hits: BTreeMap<u32, Vec<(u32, u16, Vec<usize>)>> = BTreeMap::new();
    // query -> [(chunk name hash, chunk type hash, block)]
    let mut raw_hits: BTreeMap<u32, Vec<(u32, u32, u16)>> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let (mut models_read, mut chunks_read) = (0usize, 0usize);

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
                failures.push(format!(
                    "block {b} {path}: chunk 0x{:08X} runs to {end}, block is {} bytes",
                    e.name_hash,
                    dec.len()
                ));
                break;
            }
            let chunk = &dec[off..end];
            off = end;
            chunks_read += 1;

            if e.type_hash == wad::MODEL_TYPE_HASH {
                let mats = match parse_mtrl(chunk, MtrlSource::Model) {
                    Ok(m) => m,
                    Err(err) => {
                        failures.push(format!("block {b} {path}: model 0x{:08X} MTRL: {err:?}", e.name_hash));
                        continue;
                    }
                };
                models_read += 1;
                for q in &queries {
                    let used: Vec<usize> = mats
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.textures.contains(q))
                        .map(|(i, _)| i)
                        .collect();
                    if !used.is_empty() {
                        model_hits.entry(*q).or_default().push((e.name_hash, b, used));
                    }
                }
            } else {
                for q in &queries {
                    if e.name_hash == *q {
                        continue; // the texture's own chunk
                    }
                    let needle = q.to_le_bytes();
                    if chunk.chunks_exact(4).any(|word| word == needle) {
                        raw_hits.entry(*q).or_default().push((e.name_hash, e.type_hash, b));
                    }
                }
            }
        }
    }

    let leaf = |b: u16| paths[b as usize].rsplit(['\\', '/']).next().unwrap_or("").to_string();
    for q in &queries {
        println!("\n=== 0x{q:08X}  {} ===", label(*q));
        let ms = model_hits.get(q).map(Vec::as_slice).unwrap_or(&[]);
        println!("models whose MTRL uses it: {}", ms.len());
        for (m, b, mats) in ms {
            println!("  model 0x{m:08X}  {:40}  block {b:5} {}  materials {mats:?}", label(*m), leaf(*b));
        }
        let rs = raw_hits.get(q).map(Vec::as_slice).unwrap_or(&[]);
        println!("other chunks holding it as an aligned LE u32 (field not decoded): {}", rs.len());
        for (n, t, b) in rs {
            println!("  chunk 0x{n:08X} type 0x{t:08X}  {:40}  block {b:5} {}", label(*n), leaf(*b));
        }
    }

    println!("\nread {} blocks, {chunks_read} chunks, {models_read} model MTRLs", paths.len());
    if !failures.is_empty() {
        eprintln!("\n{} item(s) could not be read:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}
