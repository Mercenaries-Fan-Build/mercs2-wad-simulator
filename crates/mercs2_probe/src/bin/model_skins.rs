//! Which skin does each UNNAMED model wear? The association the catalog cannot show.
//!
//! A model with no known name sits in the catalog as a bare hash, while the textures its own `MTRL`
//! cites are named — so the textures look orphaned and the model looks anonymous, though the link
//! between them is in the data. Every block's entry table is walked, every model `MTRL` parsed, and
//! for each model hash with no name the textures it cites are listed.
//!
//! `groups` is display only: each named texture's first four `_` tokens (`vz_veh_helicopter_mi26`),
//! counted. The citations themselves are the evidence.
//!
//! Exit 0 = every block, every model MTRL read; 1 = something could not be read (listed); 2 = usage.
//!
//! ```text
//! cargo run -p mercs2_probe --bin model_skins -- [--wad path] [--tsv out.tsv]
//! ```

use mercs2_engine::wad;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();

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
        eprintln!("model_skins: no vz.wad found (pass --wad, or write .mercs2-local.toml)");
        std::process::exit(2);
    });
    let mut w = wad::open(&wadpath).unwrap_or_else(|e| {
        eprintln!("model_skins: cannot open {wadpath}: {e}");
        std::process::exit(2);
    });
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    eprintln!("[model_skins] scanning {} blocks of {wadpath}", paths.len());

    // model hash -> (blocks, cited textures, material count of the first container seen)
    let mut models: BTreeMap<u32, (BTreeSet<u16>, BTreeSet<u32>, usize)> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
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
            if e.type_hash != wad::MODEL_TYPE_HASH {
                continue;
            }
            match parse_mtrl(chunk, MtrlSource::Model) {
                Ok(mats) => {
                    let slot = models.entry(e.name_hash).or_insert_with(|| (BTreeSet::new(), BTreeSet::new(), mats.len()));
                    slot.0.insert(b);
                    for m in mats {
                        slot.1.extend(m.textures.iter().copied().filter(|t| *t != 0 && *t != 0xFFFF_FFFF));
                    }
                }
                Err(err) => failures.push(format!("block {b} {path}: model 0x{:08X} MTRL: {err}", e.name_hash)),
            }
        }
    }

    let leaf = |b: u16| paths[b as usize].rsplit(['\\', '/']).next().unwrap_or("").to_string();
    let group_of = |n: &str| n.split('_').take(4).collect::<Vec<_>>().join("_");

    // `--all` lists named models too (to study how the named ones were spelled).
    let all = args.iter().any(|a| a == "--all");
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

    if let Some(p) = get("--tsv") {
        let mut f = std::fs::File::create(&p).expect("create --tsv");
        writeln!(f, "model\tname\tfirst_block\tblocks\tmaterials\ttextures\tnamed_textures\tgroups\ttexture_names").unwrap();
        for (h, groups, blk, nb, nm, nt, named, own) in &rows {
            writeln!(f, "0x{h:08X}\t{own}\t{blk}\t{nb}\t{nm}\t{nt}\t{}\t{groups}\t{}", named.len(), named.join(" ")).unwrap();
        }
        println!("\nwrote {p}");
    }

    if !failures.is_empty() {
        eprintln!("\n{} item(s) could not be read:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::exit(1);
    }
}
