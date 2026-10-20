//! Which models use a texture? The reverse of `mtrl_probe`.
//!
//! A model declares its textures only as hashes inside its own `MTRL`; there is no manifest, so
//! the only way to answer "who uses `al_hum_soldier_elitea_ub`" is to read every model. This walks
//! EVERY block's entry table (not the ASET table), so it reaches every model container at every LOD
//! rung, including containers with no ASET row of their own, and parses each one's `MTRL`.
//!
//! Second pass, reported separately: any OTHER chunk (not a model, not the queried texture itself)
//! whose bytes hold a queried hash as an aligned little-endian u32. That only says the value occurs;
//! the field it sits in is not decoded. `--raw-all` widens that pass to model chunks and to every byte
//! offset, for hashes that are not textures.
//!
//! `--unnamed-models`: which skin does each UNNAMED model wear? A model with no known name sits in
//! the catalog as a bare hash, while the textures its own `MTRL` cites are named — so the textures
//! look orphaned and the model looks anonymous, though the link between them is in the data. For
//! each model hash with no name, the textures it cites are listed (`--all`: named models too;
//! `--tsv`: also written as a table). `groups` is display only: each named texture's first four `_`
//! tokens (`vz_veh_helicopter_mi26`), counted. The citations themselves are the evidence.
//!
//! Names come from `docs/data/aset_names.csv` under the `notes_docs_dir` of `.mercs2-local.toml`.
//!
//! Exit 0 = every block and every model MTRL was read; 1 = something could not be read (listed);
//! 2 = usage or setup.
//!
//! ```text
//! cargo run -p mercs2_probe --bin tex_users -- al_hum_soldier_elitea_ub 0x21A0C4D6 [--raw-all] [--wad path]
//! cargo run -p mercs2_probe --bin tex_users -- --unnamed-models [--all] [--tsv out.tsv] [--wad path]
//! ```

use mercs2_engine::wad;
use mercs2_engine::worldutil::{dev_aset_names, hash_or_name};
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;

fn setup_fail(msg: String) -> ! {
    eprintln!("tex_users: {msg}");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let wad_arg = get("--wad");
    let tsv = get("--tsv");
    // `--raw-all`: the raw pass also covers model chunks and matches at every byte offset — for a hash
    // that is not a texture (a record payload in a grouped component need not be 4-aligned).
    let raw_all = args.iter().any(|a| a == "--raw-all");
    let unnamed_models = args.iter().any(|a| a == "--unnamed-models");
    // `--all` lists named models too (to study how the named ones were spelled).
    let all = args.iter().any(|a| a == "--all");
    let queries: Vec<u32> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && matches!(args[i - 1].as_str(), "--wad" | "--tsv")))
        .map(|(_, a)| hash_or_name(a).unwrap_or_else(|e| setup_fail(e)))
        .collect();
    if queries.is_empty() && !unnamed_models {
        eprintln!("usage: tex_users <texture name|0xHASH>... [--raw-all] [--wad <path>]");
        eprintln!("       tex_users --unnamed-models [--all] [--tsv <out.tsv>] [--wad <path>]");
        std::process::exit(2);
    }

    // hash -> name, from the ASET export, to label what the scan finds.
    let name_of = dev_aset_names(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| setup_fail(e));
    let label = |h: u32| name_of.get(&h).cloned().unwrap_or_else(|| "(unnamed)".into());

    let wadpath = wad::resolve_vz_wad(wad_arg.as_deref())
        .unwrap_or_else(|| setup_fail("no vz.wad found (pass --wad, or write .mercs2-local.toml)".into()));
    let mut w = wad::open(&wadpath).unwrap_or_else(|e| setup_fail(format!("cannot open {wadpath}: {e}")));

    for q in &queries {
        println!("query 0x{q:08X}  {}", label(*q));
    }
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    eprintln!("[tex_users] scanning {} blocks of {wadpath}", paths.len());

    // query -> [(model hash, block, material indices)]
    let mut model_hits: BTreeMap<u32, Vec<(u32, u16, Vec<usize>)>> = BTreeMap::new();
    // query -> [(chunk name hash, chunk type hash, block)]
    let mut raw_hits: BTreeMap<u32, Vec<(u32, u32, u16)>> = BTreeMap::new();
    // model hash -> (blocks, cited textures, material count of the first container seen)
    let mut models: BTreeMap<u32, (BTreeSet<u16>, BTreeSet<u32>, usize)> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let (mut models_read, mut chunks_read) = (0usize, 0usize);

    let block_failures = wad::for_each_chunk(&mut w, |c| {
        let (b, e, chunk) = (c.block, c.entry, c.bytes);
        chunks_read += 1;

        if e.type_hash == wad::MODEL_TYPE_HASH {
            match parse_mtrl(chunk, MtrlSource::Model) {
                Ok(mats) => {
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
                    let slot = models.entry(e.name_hash).or_insert_with(|| (BTreeSet::new(), BTreeSet::new(), mats.len()));
                    slot.0.insert(b);
                    for m in &mats {
                        slot.1.extend(m.textures.iter().copied().filter(|t| *t != 0 && *t != 0xFFFF_FFFF));
                    }
                }
                Err(err) => {
                    failures.push(format!("block {b} {}: model 0x{:08X} MTRL: {err}", c.path, e.name_hash));
                    return;
                }
            }
        }
        if e.type_hash != wad::MODEL_TYPE_HASH || raw_all {
            for q in &queries {
                if e.name_hash == *q {
                    continue; // the texture's own chunk
                }
                let needle = q.to_le_bytes();
                let found = if raw_all {
                    chunk.windows(4).any(|w| w == needle)
                } else {
                    chunk.chunks_exact(4).any(|word| word == needle)
                };
                if found {
                    raw_hits.entry(*q).or_default().push((e.name_hash, e.type_hash, b));
                }
            }
        }
    });
    failures.extend(block_failures);

    let leaf = |b: u16| paths[b as usize].rsplit(['\\', '/']).next().unwrap_or("").to_string();
    for q in &queries {
        println!("\n=== 0x{q:08X}  {} ===", label(*q));
        let ms = model_hits.get(q).map(Vec::as_slice).unwrap_or(&[]);
        println!("models whose MTRL uses it: {}", ms.len());
        for (m, b, mats) in ms {
            println!("  model 0x{m:08X}  {:40}  block {b:5} {}  materials {mats:?}", label(*m), leaf(*b));
        }
        let rs = raw_hits.get(q).map(Vec::as_slice).unwrap_or(&[]);
        if raw_all {
            println!("chunks (models included) holding it as a LE u32 at any offset (field not decoded): {}", rs.len());
        } else {
            println!("other chunks holding it as an aligned LE u32 (field not decoded): {}", rs.len());
        }
        for (n, t, b) in rs {
            println!("  chunk 0x{n:08X} type 0x{t:08X}  {:40}  block {b:5} {}", label(*n), leaf(*b));
        }
    }
    if !queries.is_empty() {
        println!("\nread {} blocks, {chunks_read} chunks, {models_read} model MTRLs", paths.len());
    }

    if unnamed_models {
        unnamed_report(&models, &name_of, all, tsv.as_deref(), &leaf);
    }

    if !failures.is_empty() {
        eprintln!("\n{} item(s) could not be read:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}

/// `--unnamed-models`: each model with no name (`all`: every model), with the named textures its
/// `MTRL`s cite.
fn unnamed_report(
    models: &BTreeMap<u32, (BTreeSet<u16>, BTreeSet<u32>, usize)>,
    name_of: &HashMap<u32, String>,
    all: bool,
    tsv: Option<&str>,
    leaf: &dyn Fn(u16) -> String,
) {
    let group_of = |n: &str| n.split('_').take(4).collect::<Vec<_>>().join("_");

    let unnamed: Vec<(&u32, &(BTreeSet<u16>, BTreeSet<u32>, usize))> =
        models.iter().filter(|(h, _)| all || !name_of.contains_key(h)).collect();
    let mut rows: Vec<(u32, String, String, usize, usize, usize, Vec<String>, String)> = Vec::new();
    let (mut with_named, mut without_named) = (0usize, 0usize);
    for (h, (blocks, texes, nmat)) in &unnamed {
        let named: Vec<&String> = texes.iter().filter_map(|t| name_of.get(t)).collect();
        if named.is_empty() {
            without_named += 1;
        } else {
            with_named += 1;
        }
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for n in &named {
            *groups.entry(group_of(n)).or_default() += 1;
        }
        let mut g: Vec<(String, usize)> = groups.into_iter().collect();
        g.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let first = *blocks.iter().next().expect("model has a block");
        rows.push((
            **h,
            g.iter().map(|(s, n)| format!("{s} x{n}")).collect::<Vec<_>>().join("; "),
            leaf(first),
            blocks.len(),
            *nmat,
            texes.len(),
            named.iter().map(|s| s.to_string()).collect(),
            name_of.get(*h).cloned().unwrap_or_default(),
        ));
    }
    rows.sort_by(|a, b| a.1.cmp(&b.1));

    println!(
        "model hashes: {}  named: {}  unnamed: {}  (unnamed wearing >=1 named texture: {with_named}; none named: {without_named})",
        models.len(),
        models.len() - unnamed.len(),
        unnamed.len()
    );
    println!("\nmodel        blocks  mats  texs named  first block                     skin groups (first 4 tokens of each named texture)");
    for (h, groups, blk, nb, nm, nt, named, own) in &rows {
        println!("0x{h:08X} {own:30} {nb:5}  {nm:4}  {nt:4} {:5}  {blk:30}  {groups}", named.len());
    }

    if let Some(p) = tsv {
        let mut f = std::fs::File::create(p).expect("create --tsv");
        writeln!(f, "model\tname\tfirst_block\tblocks\tmaterials\ttextures\tnamed_textures\tgroups\ttexture_names").unwrap();
        for (h, groups, blk, nb, nm, nt, named, own) in &rows {
            writeln!(f, "0x{h:08X}\t{own}\t{blk}\t{nb}\t{nm}\t{nt}\t{}\t{groups}\t{}", named.len(), named.join(" ")).unwrap();
        }
        println!("\nwrote {p}");
    }
}
