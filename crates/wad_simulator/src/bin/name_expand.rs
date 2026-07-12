//! Expand KNOWN asset names into their unknown siblings by exploiting the fact that
//! most Mercs 2 asset names are GENERATED from a grammar with enumerable slots.
//!
//! `block_string_harvest` mined real names out of the WAD payloads and exposed the
//! grammars:
//!   * animation — `all_wz10_driverdoor_actionhijack_getin_section03_fb`
//!                 (`all_<vehicle>_<part>_action<verb>_<phase>_<section|loop>_fb`)
//!   * path      — `commercial_road5t10`  (`<district>_road<N>t<M>` — road segments)
//!   * texture   — `japanese_20_8`, `pause_menu_i117` (enumerated grids/atlases)
//!
//! A slot-grammar is brute-forceable EXACTLY, which free-form name guessing is not
//! (that failed: `aset_namehunt` scored 2/14,169). Two expansions, both seeded only
//! by names we have already CONFIRMED:
//!
//!   1. numeric-slot expansion — swap each digit run for every value in range,
//!      preserving zero-pad width (`japanese_20_8` -> `japanese_20_9`, `japanese_21_8`, …)
//!   2. token substitution — split on `_`; for names of the same shape (same token
//!      count), substitute one token at a time from the vocabulary observed at that
//!      position (`..._wz10_...` -> `..._mi35_...`, `section03` -> `section04`, …)
//!
//! Both run to a FIXPOINT: every newly confirmed name is fed back as a fresh seed, so
//! one cracked animation clip can unroll its whole family. A candidate is only ever
//! accepted when its hash is present in ASET — so every emitted name is a verified
//! preimage, never a guess.
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin name_expand -- \
//!       --export docs/data/aset_export.csv \
//!       --seed docs/data/aset_block_strings.json --seed docs/data/aset_discovered_names.json \
//!       --emit docs/data/aset_expanded_names.json

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use clap::Parser;
use rayon::prelude::*;

use mercs2_formats::hash::pandemic_hash_m2;

#[derive(Parser)]
#[command(about = "Expand known asset names into unknown siblings via slot grammars")]
struct Cli {
    #[arg(long, default_value = "docs/data/aset_export.csv")]
    export: PathBuf,

    /// Extra confirmed-name JSON fragments to seed from (repeatable).
    #[arg(long)]
    seed: Vec<PathBuf>,

    #[arg(long, default_value = "docs/data/aset_expanded_names.json")]
    emit: PathBuf,

    /// Max value tried in a numeric slot.
    #[arg(long, default_value_t = 256)]
    num_max: u32,

    /// Stop after this many fixpoint rounds.
    #[arg(long, default_value_t = 12)]
    rounds: usize,
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

/// Split a name into alternating text / digit-run pieces.
#[derive(Clone)]
enum Piece {
    Text(String),
    Num { width: usize },
}

fn pieces(name: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let b: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Piece::Num { width: i - s });
        } else {
            let s = i;
            while i < b.len() && !b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Piece::Text(b[s..i].iter().collect()));
        }
    }
    out
}

/// Numeric-slot expansion.
///
/// Names like `commercial_road5t10` (road segment 5->10) and `japanese_20_8` carry TWO
/// independent numeric slots, so varying one at a time cannot reach most of the family —
/// we need the CROSS PRODUCT. With 1-2 slots we enumerate the full product; with 3 we cap
/// the range to keep the candidate count sane (hashing is ~ns, but memory is not free).
/// Each slot is emitted both zero-padded to the original width and unpadded, since the
/// pipeline used both (`section03` vs `road5`).
fn numeric_variants(name: &str, num_max: u32, out: &mut Vec<String>) {
    let ps = pieces(name);
    let slots: Vec<usize> = ps
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p, Piece::Num { .. }))
        .map(|(i, _)| i)
        .collect();
    if slots.is_empty() || slots.len() > 3 {
        return;
    }
    let cap = match slots.len() {
        1 => num_max,
        2 => num_max.min(128),
        _ => 24, // 3 slots: 25^3 = 15,625 per seed
    };

    // odometer over the numeric slots
    let n = slots.len();
    let mut counter = vec![0u32; n];
    loop {
        for pad in [true, false] {
            let mut s = String::with_capacity(name.len() + 4);
            let mut si = 0usize;
            for p in &ps {
                match p {
                    Piece::Text(t) => s.push_str(t),
                    Piece::Num { width } => {
                        let v = counter[si];
                        if pad {
                            s.push_str(&format!("{:0width$}", v, width = *width));
                        } else {
                            s.push_str(&v.to_string());
                        }
                        si += 1;
                    }
                }
            }
            out.push(s);
        }
        // increment odometer
        let mut i = 0usize;
        loop {
            counter[i] += 1;
            if counter[i] <= cap {
                break;
            }
            counter[i] = 0;
            i += 1;
            if i == n {
                return;
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // ── targets + already-known names ───────────────────────────────
    let text = std::fs::read_to_string(&cli.export)?;
    let mut target_type: HashMap<u32, String> = HashMap::new();
    let mut known: HashMap<u32, String> = HashMap::new();
    for line in text.lines().skip(1) {
        let f = csv_split(line);
        if f.len() < 8 {
            continue;
        }
        if let Ok(h) = u32::from_str_radix(f[2].trim().trim_start_matches("0x"), 16) {
            target_type.insert(h, f[7].clone());
            if !f[3].is_empty() {
                known.insert(h, f[3].clone());
            }
        }
    }
    for p in &cli.seed {
        if !p.exists() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p)?)?;
        if let Some(o) = v.get("pandemic_hash_m2").and_then(|x| x.as_object()) {
            for (k, names) in o {
                if let (Ok(h), Some(n)) = (
                    u32::from_str_radix(k.trim_start_matches("0x"), 16),
                    names.as_array().and_then(|a| a.first()).and_then(|s| s.as_str()),
                ) {
                    known.insert(h, n.to_string());
                }
            }
        }
    }
    let unnamed_total = target_type.len() - known.len();
    eprintln!(
        "{} ASET hashes; {} known, {} unnamed",
        target_type.len(),
        known.len(),
        unnamed_total
    );

    // ── fixpoint expansion ──────────────────────────────────────────
    let mut seeds: Vec<String> = known.values().cloned().collect();
    let mut found: BTreeMap<u32, String> = BTreeMap::new();

    for round in 1..=cli.rounds {
        // position-wise token vocabulary, grouped by token count (the name "shape")
        let mut vocab: HashMap<(usize, usize), HashSet<String>> = HashMap::new();
        for n in seeds.iter().chain(found.values()) {
            let toks: Vec<&str> = n.split('_').collect();
            for (i, t) in toks.iter().enumerate() {
                vocab
                    .entry((toks.len(), i))
                    .or_default()
                    .insert((*t).to_string());
            }
        }

        let hits: Vec<(u32, String)> = seeds
            .par_iter()
            .flat_map_iter(|seed| {
                let mut cands: Vec<String> = Vec::new();
                numeric_variants(seed, cli.num_max, &mut cands);

                let toks: Vec<&str> = seed.split('_').collect();
                if toks.len() >= 2 && toks.len() <= 10 {
                    for i in 0..toks.len() {
                        if let Some(alts) = vocab.get(&(toks.len(), i)) {
                            for a in alts {
                                if a.as_str() == toks[i] {
                                    continue;
                                }
                                let mut t2 = toks.clone();
                                t2[i] = a;
                                cands.push(t2.join("_"));
                            }
                        }
                    }
                }

                cands
                    .into_iter()
                    .filter_map(|c| {
                        let h = pandemic_hash_m2(&c);
                        if target_type.contains_key(&h) && !known.contains_key(&h) {
                            Some((h, c))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let before = found.len();
        for (h, n) in hits {
            found.entry(h).or_insert(n);
        }
        let fresh = found.len() - before;
        println!("round {round}: +{fresh} new names (total {})", found.len());
        if fresh == 0 {
            break;
        }
        // feed the new names back in as seeds — one cracked clip unrolls its family
        seeds = found.values().cloned().collect();
    }

    // ── report ──────────────────────────────────────────────────────
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for h in found.keys() {
        let t = target_type.get(h).map(|s| s.as_str()).unwrap_or("?");
        *by_type.entry(t).or_default() += 1;
    }
    println!("\nNEWLY NAMED, by type:");
    let mut rows: Vec<_> = by_type.into_iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (t, n) in rows {
        println!("  {t:<18} {n}");
    }
    println!("\nsamples:");
    for (h, n) in found.iter().take(20) {
        println!("  0x{h:08X}  {n}");
    }
    println!(
        "\nremaining unnamed: {} -> {}",
        unnamed_total,
        unnamed_total - found.len()
    );

    if let Some(d) = cli.emit.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut map = serde_json::Map::new();
    for (h, n) in &found {
        map.insert(
            format!("0x{h:08X}"),
            serde_json::Value::Array(vec![serde_json::Value::String(n.clone())]),
        );
    }
    std::fs::write(
        &cli.emit,
        serde_json::to_string_pretty(&serde_json::json!({ "pandemic_hash_m2": map }))?,
    )?;
    println!("Wrote {}", cli.emit.display());
    Ok(())
}
