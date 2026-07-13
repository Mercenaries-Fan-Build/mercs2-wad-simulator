//! Crack ONE unnamed asset hash at a time, from the vocabulary of the whole corpus.
//!
//! The residual unnamed models are authored props that live inside shared blocks, so no
//! block path spells them and no texture stem hands them over (`asset_gap_probe` /
//! `model_namer` already took everything those conventions can reach). What is left is a
//! genuine search — and a search against a 32-bit hash is only sound if it is NARROW.
//!
//! ── the soundness argument (this is the whole design) ─────────────────────────
//! Testing S candidates against T targets mints ~S*T/2^32 preimages BY CHANCE. The trap
//! is sweeping a big grammar against ALL unnamed hashes at once: T=565 forces S below
//! ~7M to stay clean, which is far too small a net. Invert it — search ONE target at a
//! time (T=1) and the same cleanliness allows S ~ 4 BILLION. The identical grammar that
//! is reckless against 565 targets is safe against 1.
//!
//! And a chance hit here is far more dangerous than the gibberish a raw string-mine throws
//! (`cbjoxg`, `qxcvzq` — obviously junk). A word-grammar collision comes out looking like
//! `global_doorwindow07`: plausible, and indistinguishable from a real name by eye. So the
//! expected-collision count is printed for every run and MUST be << 1. Treat any run whose
//! `expected-noise` approaches 1 as having produced fiction.
//!
//! ── the grammar ──────────────────────────────────────────────────────────────
//! Mercs2 object names CONCATENATE their words with no separator, which is why token
//! permutation joined by `_` finds nothing:
//!     commercial_walllong   pmcoutpost_statuediscus   global_sandbagsstraightgr
//! So: `<prefix>_<w1><w2><w3><suffix>`, words joined by "" or "_", suffix from the
//! observed variant set (`01`, `a`, `_lod`, `_ruin`, ...).
//!
//! Vocabulary comes from the corpus itself (every token of every KNOWN asset name), so it
//! is the real authoring vocabulary rather than a guess, plus `--word` hints (e.g. the
//! tokens of the textures the model actually uses — mesh_probe prints them).
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin aset_target_crack -- \
//!       --target 0xBA18A1DF --prefix global \
//!       --word money --word cash --word pile --word stack --word bundle

use std::collections::HashSet;
use std::path::PathBuf;

use clap::Parser;
use rayon::prelude::*;

use mercs2_formats::hash::pandemic_hash_m2;

#[derive(Parser)]
#[command(about = "Narrow per-hash name search over the corpus vocabulary")]
struct Cli {
    /// Target hash(es). Keep this SMALL — the error bar scales with the count.
    #[arg(long, value_parser = parse_hex, required = true)]
    target: Vec<u32>,

    /// Name prefix(es) to try, e.g. `global`, `commercial`. Repeatable.
    #[arg(long)]
    prefix: Vec<String>,

    /// Extra vocabulary words (the model's own texture tokens, semantic hints). Repeatable.
    #[arg(long)]
    word: Vec<String>,

    /// Also pull the whole corpus vocabulary out of this names CSV.
    #[arg(long, default_value = "docs/data/aset_names.csv")]
    names: PathBuf,

    /// Max words concatenated into the body.
    #[arg(long, default_value_t = 3)]
    depth: usize,

    /// Use only `--word` vocabulary (skip the corpus tokens) — for a very tight search.
    #[arg(long, default_value_t = false)]
    words_only: bool,

    /// Corpus tokens must appear at least this often to join the pool (trims one-offs).
    #[arg(long, default_value_t = 3)]
    min_freq: usize,

    /// SEMANTIC ANCHOR: force one body word to be this (repeatable, tried at each position).
    ///
    /// This is what makes a big search sound. We know what the model IS — we can look at the
    /// textures it draws (`global_money01_dm`) or simply render it. A chance 32-bit collision
    /// that ALSO happens to spell the model's actual subject is a coincidence on top of a
    /// coincidence, so requiring the anchor turns "a hash matched" into two independent
    /// witnesses. It also collapses the space: pinning one word of a 3-word body removes a
    /// whole vocabulary factor, which is what buys the depth to reach compounds like
    /// `global_sandbagsstraightgr`.
    #[arg(long)]
    anchor: Vec<String>,
}

fn parse_hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .map_err(|e| e.to_string())
}

/// Split a name into its authoring tokens: on `_`, and at letter/digit boundaries.
fn tokenize(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in name.split('_') {
        let mut cur = String::new();
        let mut last_digit = None;
        for c in part.chars() {
            let d = c.is_ascii_digit();
            if last_digit.is_some() && last_digit != Some(d) {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            cur.push(c);
            last_digit = Some(d);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let targets: HashSet<u32> = cli.target.iter().copied().collect();

    // ── vocabulary ───────────────────────────────────────────────────────────
    let mut pool: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for w in &cli.word {
        for t in tokenize(&w.to_ascii_lowercase()) {
            if seen.insert(t.clone()) {
                pool.push(t);
            }
        }
    }
    let hint_count = pool.len();

    let mut prefixes: Vec<String> = cli.prefix.iter().map(|p| p.to_ascii_lowercase()).collect();

    if !cli.words_only {
        let text = std::fs::read_to_string(&cli.names)?;
        let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut pfreq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for line in text.lines().skip(1) {
            // name is field 2 of the CSV and never contains a comma
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 2 || f[1].is_empty() {
                continue;
            }
            let name = f[1].to_ascii_lowercase();
            if let Some(p) = name.split('_').next() {
                *pfreq.entry(p.to_string()).or_default() += 1;
            }
            for t in tokenize(&name) {
                if t.len() >= 3 && t.chars().all(|c| c.is_ascii_alphabetic()) {
                    *freq.entry(t).or_default() += 1;
                }
            }
        }
        for (t, n) in freq {
            if n >= cli.min_freq && seen.insert(t.clone()) {
                pool.push(t);
            }
        }
        if prefixes.is_empty() {
            let mut ps: Vec<(String, usize)> = pfreq.into_iter().filter(|(_, n)| *n >= 20).collect();
            ps.sort_by(|a, b| b.1.cmp(&a.1));
            prefixes = ps.into_iter().map(|(p, _)| p).collect();
        }
    }

    // variant suffixes observed across the corpus
    let mut suffixes: Vec<String> = vec![String::new()];
    for i in 0..=24 {
        suffixes.push(format!("{i:02}"));
        suffixes.push(format!("{i}"));
    }
    for c in b'a'..=b'h' {
        suffixes.push((c as char).to_string());
    }
    for s in ["_lod", "_ruin", "_int", "_dm", "_nm", "_sm", "_a", "_b", "_c", "_01", "_02"] {
        suffixes.push(s.to_string());
    }
    suffixes.sort();
    suffixes.dedup();

    eprintln!(
        "targets: {}   prefixes: {}   vocabulary: {} words ({} from --word hints)   suffixes: {}",
        targets.len(),
        prefixes.len(),
        pool.len(),
        hint_count,
        suffixes.len()
    );

    // ── search ───────────────────────────────────────────────────────────────
    // Bodies are w1[sep]w2[sep]w3 with sep in {"", "_"}: the concatenated compound is the
    // one the engine actually uses, but both are cheap.
    let anchors: Vec<String> = cli.anchor.iter().map(|a| a.to_ascii_lowercase()).collect();
    let n = pool.len() as u64;
    let a = anchors.len().max(1) as u64;
    // With an anchor pinned, one body slot is fixed, so a k-word body costs vocab^(k-1).
    let bodies: u64 = if anchors.is_empty() {
        match cli.depth {
            1 => n,
            2 => n + n * n * 2,
            _ => n + n * n * 2 + n * n * n * 4,
        }
    } else {
        match cli.depth {
            1 => a,
            2 => a * (1 + 2 * n * 2),
            _ => a * (1 + 2 * n * 2 + 3 * n * n * 4),
        }
    };
    let space = bodies * prefixes.len() as u64 * suffixes.len() as u64;
    let noise = space as f64 * targets.len() as f64 / 4_294_967_296.0;
    eprintln!(
        "search space: ~{space} candidates -> expected chance collisions ~{noise:.4}{}",
        if noise > 0.5 {
            "   *** HIGH — a hit here needs the anchor + your eyes to be trustworthy ***"
        } else {
            ""
        }
    );

    // Body shapes searched. With anchors, the anchor occupies one slot and the pool fills
    // the rest; without, the pool fills every slot.
    let shapes: Vec<Vec<Option<usize>>> = if anchors.is_empty() {
        match cli.depth {
            1 => vec![vec![None]],
            2 => vec![vec![None], vec![None, None]],
            _ => vec![vec![None], vec![None, None], vec![None, None, None]],
        }
    } else {
        let mut v = vec![vec![Some(0usize)]];
        if cli.depth >= 2 {
            v.push(vec![Some(0), None]);
            v.push(vec![None, Some(0)]);
        }
        if cli.depth >= 3 {
            v.push(vec![Some(0), None, None]);
            v.push(vec![None, Some(0), None]);
            v.push(vec![None, None, Some(0)]);
        }
        v
    };

    let empty = String::new();
    let anchor_list: Vec<&String> = if anchors.is_empty() {
        vec![&empty]
    } else {
        anchors.iter().collect()
    };

    let hits: Vec<(u32, String)> = pool
        .par_iter()
        .flat_map_iter(|wa| {
            let mut out: Vec<(u32, String)> = Vec::new();
            let mut test = |cand: &str, out: &mut Vec<(u32, String)>| {
                let h = pandemic_hash_m2(cand);
                if targets.contains(&h) {
                    out.push((h, cand.to_string()));
                }
            };
            // `wa` is this thread's slice of the vocabulary; it fills the FIRST free slot,
            // the remaining free slot (if any) iterates the whole pool.
            for anc in &anchor_list {
                for shape in &shapes {
                    let free = shape.iter().filter(|s| s.is_none()).count();
                    let words = |fill: (&str, &str)| -> Vec<String> {
                        let mut it = [fill.0, fill.1].into_iter();
                        shape
                            .iter()
                            .map(|s| match s {
                                Some(_) => (*anc).clone(),
                                None => it.next().unwrap().to_string(),
                            })
                            .collect()
                    };
                    let seps: &[&str] = &["", "_"];
                    let mut emit = |ws: &[String], out: &mut Vec<(u32, String)>| {
                        for p in &prefixes {
                            match ws.len() {
                                1 => {
                                    for suf in &suffixes {
                                        test(&format!("{p}_{}{suf}", ws[0]), out);
                                    }
                                }
                                2 => {
                                    for s1 in seps {
                                        for suf in &suffixes {
                                            test(&format!("{p}_{}{s1}{}{suf}", ws[0], ws[1]), out);
                                        }
                                    }
                                }
                                _ => {
                                    for s1 in seps {
                                        for s2 in seps {
                                            for suf in &suffixes {
                                                test(
                                                    &format!(
                                                        "{p}_{}{s1}{}{s2}{}{suf}",
                                                        ws[0], ws[1], ws[2]
                                                    ),
                                                    out,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };
                    match free {
                        0 => emit(&words(("", "")), &mut out),
                        1 => emit(&words((wa, "")), &mut out),
                        _ => {
                            for wb in &pool {
                                emit(&words((wa, wb)), &mut out);
                            }
                        }
                    }
                }
            }
            out
        })
        .collect();

    eprintln!();
    if hits.is_empty() {
        eprintln!("no preimage found — widen --word / --prefix, or the name is outside this grammar");
    }
    let mut uniq: Vec<(u32, String)> = hits;
    uniq.sort();
    uniq.dedup();
    for (h, name) in &uniq {
        eprintln!("  HIT  0x{h:08X}  {name}");
    }
    Ok(())
}
