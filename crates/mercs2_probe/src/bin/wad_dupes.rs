//! Dev bin: cross-WAD / intra-WAD duplicate ASET inventory, with byte-level payload comparison.
//!
//! An asset's registry identity is `(asset_hash, type_hash)` — the ASET row's `asset_hash` plus the
//! type hash looked up in the WAD's own 36-entry type table (file offset `0x48`, count = the DATA
//! chunk-row's `meta` field, `0x24`; the engine copies it to `reader+0x458` in `FUN_00875140` and
//! keys its per-archive lookup table on `type_hash ^ asset_hash` in `FUN_008751d0`).
//!
//! The ASET hash is of the asset NAME, not of its bytes, so "same hash in two WADs" says nothing
//! about whether the payloads agree. This tool decompresses the owning block on both sides and
//! compares the container bytes exactly.
//!
//!   cargo run --release -p mercs2_probe --bin wad_dupes -- \
//!       --wad <Loading.wad> --wad <shell.wad> --wad <vz.wad> --wad <English.wad> [--out f.tsv]
//!
//! Pass `--wad` in engine MOUNT order; the report labels the winner accordingly (the engine walks
//! its archive array from the highest index down — `FUN_00875e80` / `FUN_00876150`).

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;

const TYPE_TABLE_OFF: u64 = 0x48;

/// The WAD's own `type_id -> type_hash` table. Read from the file, never assumed.
fn read_type_table(file: &mut File) -> Vec<u32> {
    let mut hdr = [0u8; 0x48];
    file.seek(SeekFrom::Start(0)).expect("seek");
    file.read_exact(&mut hdr).expect("read header");
    // hdr[8] is the DATA chunk row's `meta` word; the engine uses it as the type count.
    let count = u32::from_le_bytes([hdr[0x20], hdr[0x21], hdr[0x22], hdr[0x23]]) as usize;
    let mut raw = vec![0u8; count * 4];
    file.seek(SeekFrom::Start(TYPE_TABLE_OFF)).expect("seek");
    file.read_exact(&mut raw).expect("read type table");
    (0..count).map(|i| u32::from_le_bytes([raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]])).collect()
}

fn type_name(h: u32) -> &'static str {
    match h {
        0xFA46D8A8 => "fxdict",
        0x140E8728 => "guidmap",
        0x7131D39A => "type02",
        0x8F0A54E2 => "binary",
        0x3B0AABF8 => "decaltable",
        0x665EF13E => "facefxanimationset",
        0xF753F6D0 => "wavebank",
        0x39E5E978 => "stringdb",
        0xC122545A => "musicstatemap",
        0xE6B81A54 => "layer",
        0xE8DF4D87 => "musiccue",
        0x207359C7 => "animationtable",
        0x600B904E => "scrub",
        0xE5273C14 => "sounddb",
        0xDE982D61 => "materialparam",
        0x99E77ACE => "font",
        0x18166555 => "animation",
        0x5647C35D => "worldentity",
        0xFA0B8DBC => "chatter",
        0x5B724250 => "model",
        0x34612F86 => "type20",
        0x9F8BCA10 => "soundbank",
        0x1602815C => "lowresterrain",
        0xFE0E8320 => "scaleformgfx",
        0xACCE47F2 => "sequencetable",
        0x4D7D30C4 => "type25",
        0xEA4829D5 => "level",
        0xF011157A => "texture",
        0xBCFE6314 => "path",
        0x5608BD5A => "effect",
        0x6310807F => "lineregion",
        0x59B9DF6A => "materialtable",
        0x7C569307 => "terrainmesh",
        0xECE70371 => "animstatemachine",
        0x1CF649BB => "facefxactor",
        0x42498680 => "script",
        _ => "?",
    }
}

#[derive(Clone)]
struct Row {
    wad: usize,
    row_index: usize,
    block: u16,
    lod_rungs: usize,
}

struct Wad {
    label: String,
    path: String,
    file: File,
    types: Vec<u32>,
    blocks: usize,
    aset_rows: usize,
    paths: Vec<String>,
    indx: Vec<mercs2_formats::ffcs::IndxEntry>,
    /// decompressed-block cache: block index -> containers keyed by (name_hash, type_hash)
    cache: HashMap<u16, HashMap<(u32, u32), Vec<u8>>>,
}

impl Wad {
    /// The container bytes for `(name_hash, type_hash)` inside `block`, or None if the block does
    /// not actually carry it (an ASET row can point at a block that only references the asset).
    fn container(&mut self, block: u16, name: u32, ty: u32) -> Option<Vec<u8>> {
        if !self.cache.contains_key(&block) {
            let mut map = HashMap::new();
            if let Ok(d) = decompress_block(&mut self.file, &self.indx, block) {
                let label = self.paths.get(block as usize).cloned().unwrap_or_default();
                let (parsed, _) = walk_decompressed_block(&d, &label);
                for (i, e) in parsed.entries.iter().enumerate() {
                    if let Some(c) = parsed.containers.get(i) {
                        map.entry((e.name_hash, e.type_hash)).or_insert_with(|| c.clone());
                    }
                }
            }
            self.cache.insert(block, map);
        }
        self.cache.get(&block).and_then(|m| m.get(&(name, ty))).cloned()
    }
}

/// The UCFX chunk-descriptor tags of a container, in table order.
fn chunk_tags(c: &[u8]) -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let n = u32::from_le_bytes([c[16], c[17], c[18], c[19]]) as usize;
    for i in 0..n {
        let o = 20 + i * 20;
        if o + 20 > c.len() {
            break;
        }
        out.push([c[o], c[o + 1], c[o + 2], c[o + 3]]);
    }
    out
}

fn describe_container(c: &[u8]) -> String {
    chunk_tags(c).iter().map(|t| String::from_utf8_lossy(t).to_string()).collect::<Vec<_>>().join(",")
}

fn fnv1a64(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut wad_paths: Vec<String> = Vec::new();
    let mut out: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--wad" => {
                wad_paths.push(args.get(i + 1).cloned().unwrap_or_default());
                i += 2;
            }
            "--out" => {
                out = args.get(i + 1).cloned();
                i += 2;
            }
            "--validate" => {
                i += 1;
            }
            // Handy while auditing a duplicate: what does this NAME actually hash to?
            "--hash" => {
                let n = args.get(i + 1).cloned().unwrap_or_default();
                println!("0x{:08X}  {n}", mercs2_formats::hash::pandemic_hash_m2(&n));
                i += 2;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    if wad_paths.is_empty() {
        eprintln!("usage: wad_dupes --wad <wad> [--wad <wad> ...] [--out <tsv>]   (mount order)");
        std::process::exit(2);
    }

    let mut wads: Vec<Wad> = Vec::new();
    // key -> rows.  key = (asset_hash, type_hash)
    let mut index: BTreeMap<(u32, u32), Vec<Row>> = BTreeMap::new();

    for (wi, p) in wad_paths.iter().enumerate() {
        let mut file = File::open(p).unwrap_or_else(|e| {
            eprintln!("open {p}: {e}");
            std::process::exit(1);
        });
        let size = file.metadata().expect("stat").len();
        let types = read_type_table(&mut file);
        let arch = load_ffcs_archive(&mut file, size).unwrap_or_else(|e| {
            eprintln!("parse {p}: {e}");
            std::process::exit(1);
        });
        let label = std::path::Path::new(p)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.clone());
        println!(
            "[{wi}] {label}: {} blocks, {} ASET rows, {} types",
            arch.indx.len(),
            arch.aset.len(),
            types.len()
        );
        for (ri, e) in arch.aset.iter().enumerate() {
            let ty = *types.get(e.type_id as usize).unwrap_or(&0);
            index.entry((e.asset_hash, ty)).or_default().push(Row {
                wad: wi,
                row_index: ri,
                block: e.block_index(),
                lod_rungs: e.lod_chain().len(),
            });
        }
        wads.push(Wad {
            label,
            path: p.clone(),
            types,
            blocks: arch.indx.len(),
            aset_rows: arch.aset.len(),
            paths: arch.paths,
            indx: arch.indx,
            file,
            cache: HashMap::new(),
        });
    }

    // ---- validate the WAD's own type table against block reality ---------------------------
    // Sample up to N rows per type_id and check that the owning block really carries a UCFX entry
    // `(asset_hash, types[type_id])`. If the table were mis-ordered these lookups would all miss.
    if std::env::args().any(|a| a == "--validate") {
        for wi in 0..wads.len() {
            let types = wads[wi].types.clone();
            let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
            let mut probe: Vec<(u32, u32, u16)> = Vec::new();
            for (k, rows) in &index {
                for r in rows.iter().filter(|r| r.wad == wi) {
                    // recover type_id from the type hash
                    let Some(tid) = types.iter().position(|t| *t == k.1) else { continue };
                    let c = seen.entry(tid as u32).or_default();
                    if *c < 3 {
                        *c += 1;
                        probe.push((k.0, k.1, r.block));
                    }
                }
            }
            let (mut hit, mut miss) = (0usize, 0usize);
            let mut missing_types: BTreeMap<u32, usize> = BTreeMap::new();
            for (ah, th, blk) in probe {
                if wads[wi].container(blk, ah, th).is_some() {
                    hit += 1;
                } else {
                    miss += 1;
                    *missing_types.entry(th).or_default() += 1;
                }
            }
            println!("  [{}] type-table validation: {hit} hit / {miss} miss {missing_types:X?}", wads[wi].label);
            wads[wi].cache.clear();
        }
    }

    // ---- engine lookup-key collisions -------------------------------------------------------
    // `FUN_008751d0` keys the per-archive table on `type_hash ^ asset_hash` (falling back to
    // `asset_hash` when that is 0) and stores ONLY that key, so two distinct assets whose XOR
    // agrees are indistinguishable to `FUN_00874e20`. Measure it rather than assume it is safe.
    for (wi, w) in wads.iter().enumerate() {
        let mut keys: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (k, rows) in &index {
            if rows.iter().any(|r| r.wad == wi) {
                let xk = if k.0 ^ k.1 == 0 { k.0 } else { k.0 ^ k.1 };
                keys.entry(xk).or_default().push(*k);
            }
        }
        let clashes: Vec<_> = keys.iter().filter(|(_, v)| v.len() > 1).collect();
        println!("  [{}] engine XOR lookup-key clashes: {}", w.label, clashes.len());
        for (xk, v) in clashes {
            println!("      key 0x{xk:08X} <- {v:X?}");
        }
    }

    // ---- partition ------------------------------------------------------------------------
    let mut intra: Vec<((u32, u32), Vec<Row>)> = Vec::new();
    let mut cross: Vec<((u32, u32), Vec<Row>)> = Vec::new();
    for (k, rows) in &index {
        if rows.len() < 2 {
            continue;
        }
        let distinct_wads: std::collections::BTreeSet<usize> = rows.iter().map(|r| r.wad).collect();
        if distinct_wads.len() > 1 {
            cross.push((*k, rows.clone()));
        }
        if distinct_wads.len() < rows.len() {
            intra.push((*k, rows.clone()));
        }
    }
    println!("\nduplicate keys: {} cross-WAD, {} with an intra-WAD repeat", cross.len(), intra.len());

    let mut sink: Box<dyn std::io::Write> = match &out {
        Some(p) => Box::new(std::io::BufWriter::new(File::create(p).expect("create out"))),
        None => Box::new(std::io::BufWriter::new(std::io::stdout())),
    };
    writeln!(
        sink,
        "asset_hash\ttype_hash\ttype_name\tscope\twad\trow_index\tblock_index\tblock_path\tlod_rungs\tpayload_bytes\tpayload_fnv1a64\tverdict"
    )
    .expect("w");

    let mut by_type_identical: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_type_divergent: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_type_unresolved: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut pairs: BTreeMap<String, usize> = BTreeMap::new();

    for (key, rows) in cross.iter().chain(intra.iter()) {
        let (ah, th) = *key;
        let scope = if rows.iter().map(|r| r.wad).collect::<std::collections::BTreeSet<_>>().len() > 1 {
            "cross"
        } else {
            "intra"
        };
        // pull payloads
        let mut payloads: Vec<Option<Vec<u8>>> = Vec::new();
        for r in rows {
            let p = wads[r.wad].container(r.block, ah, th);
            payloads.push(p);
        }
        let resolved: Vec<&Vec<u8>> = payloads.iter().filter_map(|p| p.as_ref()).collect();
        let verdict = if resolved.len() < rows.len() {
            "UNRESOLVED"
        } else if resolved.windows(2).all(|w| w[0] == w[1]) {
            "IDENTICAL"
        } else {
            "DIVERGENT"
        };
        let tn = type_name(th);
        match verdict {
            "IDENTICAL" => *by_type_identical.entry(tn).or_default() += 1,
            "DIVERGENT" => *by_type_divergent.entry(tn).or_default() += 1,
            _ => *by_type_unresolved.entry(tn).or_default() += 1,
        }
        if scope == "cross" {
            let mut labels: Vec<&str> =
                rows.iter().map(|r| wads[r.wad].label.as_str()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            labels.sort();
            *pairs.entry(labels.join("+")).or_default() += 1;
        }
        if verdict == "DIVERGENT" {
            println!("\nDIVERGENT 0x{ah:08X} ({tn})");
            for (r, p) in rows.iter().zip(payloads.iter()) {
                let Some(c) = p else { continue };
                println!(
                    "  {:<12} block {:>5} {:<44} {} B  {}",
                    wads[r.wad].label,
                    r.block,
                    wads[r.wad].paths.get(r.block as usize).cloned().unwrap_or_default(),
                    c.len(),
                    describe_container(c)
                );
                if let Some(n) = mercs2_formats::ucfx::extract_chunk_body(c, b"NAME") {
                    println!("      NAME = {:?}", String::from_utf8_lossy(&n).trim_end_matches('\0'));
                }
            }
            // first differing byte between the first two resolved payloads
            if let (Some(a), Some(b)) = (payloads[0].as_ref(), payloads[1].as_ref()) {
                let n = a.len().min(b.len());
                let d = (0..n).find(|&i| a[i] != b[i]);
                match d {
                    Some(i) => println!("  first byte difference at container offset 0x{i:X}"),
                    None => println!("  common prefix identical; lengths differ by {}", b.len() as i64 - a.len() as i64),
                }
                // compare each chunk body pairwise
                for tag in chunk_tags(a) {
                    let ba = mercs2_formats::ucfx::extract_chunk_body(a, &tag);
                    let bb = mercs2_formats::ucfx::extract_chunk_body(b, &tag);
                    let t = String::from_utf8_lossy(&tag).to_string();
                    match (ba, bb) {
                        (Some(x), Some(y)) => println!(
                            "    {t}: {} vs {} B -> {}",
                            x.len(),
                            y.len(),
                            if x == y { "same" } else { "DIFFERENT" }
                        ),
                        (a2, b2) => println!("    {t}: present={} vs {}", a2.is_some(), b2.is_some()),
                    }
                }
            }
        }
        for (r, p) in rows.iter().zip(payloads.iter()) {
            let bp = wads[r.wad].paths.get(r.block as usize).cloned().unwrap_or_default();
            let (len, h) = match p {
                Some(b) => (b.len().to_string(), format!("{:016X}", fnv1a64(b))),
                None => ("-".to_string(), "-".to_string()),
            };
            writeln!(
                sink,
                "0x{ah:08X}\t0x{th:08X}\t{tn}\t{scope}\t{}\t{}\t{}\t{}\t{}\t{len}\t{h}\t{verdict}",
                wads[r.wad].label, r.row_index, r.block, bp, r.lod_rungs
            )
            .expect("w");
        }
    }
    drop(sink);

    println!("\n-- cross-WAD duplicate keys by WAD pair --");
    for (k, v) in &pairs {
        println!("  {k}: {v}");
    }
    println!("\n-- verdict by type (IDENTICAL / DIVERGENT / UNRESOLVED) --");
    let mut all: std::collections::BTreeSet<&'static str> = Default::default();
    all.extend(by_type_identical.keys());
    all.extend(by_type_divergent.keys());
    all.extend(by_type_unresolved.keys());
    for t in all {
        println!(
            "  {t:20} {:4} / {:4} / {:4}",
            by_type_identical.get(t).copied().unwrap_or(0),
            by_type_divergent.get(t).copied().unwrap_or(0),
            by_type_unresolved.get(t).copied().unwrap_or(0)
        );
    }
    println!(
        "\n  TOTAL {:4} identical / {:4} divergent / {:4} unresolved",
        by_type_identical.values().sum::<usize>(),
        by_type_divergent.values().sum::<usize>(),
        by_type_unresolved.values().sum::<usize>()
    );
    for w in &wads {
        println!("  [{}] {} blocks={} aset={}", w.label, w.path, w.blocks, w.aset_rows);
    }
}
