//! Harvest plaintext identifiers from every decompressed WAD block and hash them
//! against the unresolved ASET hashes.
//!
//! The rainbow table was built from corpora OUTSIDE the WAD payloads — exe strings,
//! the decompiled Lua, PTHS block-path stems, ECS class names. The block CONTENTS
//! were never mined, yet several chunk types embed authored identifiers:
//!   * `scrub`  (SCRB+MTRL+STRM+INFO) — shader/material names
//!   * `scaleformgfx` — GFx symbol/export names
//!   * `facefxactor` / `facefxanimationset` — rig + curve names
//!   * `animstatemachine` (SINF/AINF/TRNS/stns/actn) — state/transition names
//!   * `effect` / `fxdict` — emitter + parameter names
//!   * `materialparam`, `sounddb`, `chatter`, `path`, `lineregion` — table keys
//!
//! Any of those strings may be the preimage of an ASET hash. This walks every block,
//! extracts ASCII identifier runs, hashes each (plus a few convention-driven variants),
//! and reports/emits the ones that HIT a real ASET hash — so every name recorded is a
//! verified preimage, never a guess.
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin block_string_harvest -- \
//!       --wad game-files/vz.wad --export docs/data/aset_export.csv \
//!       --emit docs/data/aset_block_strings.json --corpus output/block_strings.txt

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use clap::Parser;
use rayon::prelude::*;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;

#[derive(Parser)]
#[command(about = "Mine plaintext identifiers out of decompressed WAD blocks")]
struct Cli {
    #[arg(long, default_value = "game-files/vz.wad")]
    wad: PathBuf,

    /// aset_export.csv — supplies the target hashes and which are already named.
    #[arg(long, default_value = "docs/data/aset_export.csv")]
    export: PathBuf,

    /// Rainbow fragment of confirmed hits (merge with `aset_export --rainbow`).
    #[arg(long)]
    emit: Option<PathBuf>,

    /// Dump every harvested string (a reusable corpus for the rainbow table).
    #[arg(long)]
    corpus: Option<PathBuf>,

    /// Minimum identifier length to consider.
    #[arg(long, default_value_t = 4)]
    min_len: usize,
}

fn csv_split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Identifier characters: the asset-name alphabet (plus path/extension chars, so we
/// also catch things like "foo/bar.dds" that we can split later).
fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b'/' || b == b'\\'
}

/// Pull ASCII identifier runs out of a raw chunk payload.
fn scan_strings(data: &[u8], min_len: usize, out: &mut Vec<String>) {
    let mut start: Option<usize> = None;
    for i in 0..=data.len() {
        let ident = i < data.len() && is_ident(data[i]);
        match (ident, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= min_len {
                    if let Ok(t) = std::str::from_utf8(&data[s..i]) {
                        // require at least one letter — skips pure numeric/hex noise
                        if t.bytes().any(|b| b.is_ascii_alphabetic()) {
                            out.push(t.to_string());
                        }
                    }
                }
                start = None;
            }
            _ => {}
        }
    }
}

/// Convention-driven variants of a harvested string (all cheap, all verified before use).
fn variants(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let lower = s.to_lowercase();
    v.push(lower.clone());

    // strip a directory path and/or an extension: "art/veh/foo.dds" -> "foo"
    let base = lower
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&lower)
        .to_string();
    if base != lower {
        v.push(base.clone());
    }
    if let Some(dot) = base.rfind('.') {
        let stem = base[..dot].to_string();
        if stem.len() >= 3 {
            v.push(stem);
        }
    }
    v
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // ── targets from the export ─────────────────────────────────────
    let text = std::fs::read_to_string(&cli.export)?;
    let mut all_hashes: HashMap<u32, String> = HashMap::new(); // hash -> type
    let mut named: HashSet<u32> = HashSet::new();
    for line in text.lines().skip(1) {
        let f = csv_split(line);
        if f.len() < 8 {
            continue;
        }
        if let Ok(h) = u32::from_str_radix(f[2].trim().trim_start_matches("0x"), 16) {
            all_hashes.insert(h, f[7].clone());
            if !f[3].is_empty() {
                named.insert(h);
            }
        }
    }
    eprintln!(
        "{} ASET hashes ({} still unnamed)",
        all_hashes.len(),
        all_hashes.len() - named.len()
    );

    // ── walk every block ────────────────────────────────────────────
    let mut file = File::open(&cli.wad)?;
    let file_size = file.metadata()?.len();
    let arch = load_ffcs_archive(&mut file, file_size)?;
    let n_blocks = arch.indx.len();
    eprintln!("decompressing {n_blocks} blocks (parallel)…");

    let hits: Mutex<BTreeMap<u32, String>> = Mutex::new(BTreeMap::new());
    let corpus: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    let done = std::sync::atomic::AtomicUsize::new(0);

    (0..n_blocks).into_par_iter().for_each(|blk| {
        // each worker needs its own handle
        let Ok(mut f) = File::open(&cli.wad) else { return };
        let Ok(data) = decompress_block(&mut f, &arch.indx, blk as u16) else {
            return;
        };
        let mut strs = Vec::new();
        scan_strings(&data, cli.min_len, &mut strs);

        let mut local_hits: Vec<(u32, String)> = Vec::new();
        for s in &strs {
            for cand in variants(s) {
                let h = pandemic_hash_m2(&cand);
                if all_hashes.contains_key(&h) {
                    local_hits.push((h, cand));
                }
            }
        }
        if !local_hits.is_empty() {
            let mut g = hits.lock().unwrap();
            for (h, n) in local_hits {
                g.entry(h).or_insert(n);
            }
        }
        {
            let mut c = corpus.lock().unwrap();
            for s in strs {
                c.insert(s);
            }
        }
        let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if d % 1000 == 0 {
            eprintln!("  {d}/{n_blocks} blocks");
        }
    });

    let hits = hits.into_inner().unwrap();
    let corpus = corpus.into_inner().unwrap();

    // ── report ──────────────────────────────────────────────────────
    let fresh: Vec<(&u32, &String)> = hits.iter().filter(|(h, _)| !named.contains(h)).collect();
    println!(
        "\nharvested {} distinct strings; {} hit a real ASET hash, {} of them NOT otherwise named",
        corpus.len(),
        hits.len(),
        fresh.len()
    );

    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for (h, _) in &fresh {
        let t = all_hashes.get(h).map(|s| s.as_str()).unwrap_or("?");
        *by_type.entry(t).or_default() += 1;
    }
    println!("\nNEWLY NAMED, by type:");
    let mut rows: Vec<_> = by_type.into_iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (t, n) in rows {
        println!("  {t:<18} {n}");
    }
    println!("\nsamples:");
    for (h, n) in fresh.iter().take(30) {
        let t = all_hashes.get(*h).map(|s| s.as_str()).unwrap_or("?");
        println!("  0x{h:08X}  [{t:<14}] {n}");
    }

    // ── outputs ─────────────────────────────────────────────────────
    if let Some(p) = &cli.emit {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut map = serde_json::Map::new();
        for (h, n) in &hits {
            map.insert(
                format!("0x{h:08X}"),
                serde_json::Value::Array(vec![serde_json::Value::String(n.clone())]),
            );
        }
        let root = serde_json::json!({ "pandemic_hash_m2": map });
        std::fs::write(p, serde_json::to_string_pretty(&root)?)?;
        println!("\nWrote {} ({} verified preimages)", p.display(), hits.len());
    }
    if let Some(p) = &cli.corpus {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut v: Vec<&String> = corpus.iter().collect();
        v.sort();
        let mut f = File::create(p)?;
        for s in v {
            writeln!(f, "{s}")?;
        }
        println!("Wrote {} ({} strings)", p.display(), corpus.len());
    }
    Ok(())
}
