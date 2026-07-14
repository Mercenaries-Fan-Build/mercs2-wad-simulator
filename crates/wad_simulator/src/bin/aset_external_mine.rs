//! Mine asset-name preimages out of sources OUTSIDE the PC WAD.
//!
//! `block_string_harvest` mines the PC vz.wad's own decompressed block payloads. That
//! is not enough: the PC WAD carries almost no authored names for build-GENERATED
//! assets. Grep the retail PC WAD for `tinygeometry` (the per-region LOD imposters)
//! and you get ZERO hits — the names were stripped from the PC bake. Grep the PS3
//! WAD and you get 1,548: the console builds ship an uncompressed name table the PC
//! build drops. Same hashes, same assets — only the console still spells them out.
//!
//! So the console WADs / prototype images are a NAME ORACLE for the PC WAD, and this
//! tool streams any binary, extracts identifier runs, and keeps the ones whose
//! `pandemic_hash_m2` lands on an unresolved ASET hash. Every emitted name is a
//! verified preimage, never a guess.
//!
//! ── why this is a mine and not a brute force ──────────────────────────────────
//! The hash is only 32 bits, so a candidate set of size S tested against T targets
//! yields ~S*T/2^32 preimages BY CHANCE. Guessing names from a grammar puts S in the
//! billions and drowns the real hits in collisions — `aset_expanded_names.json` (from
//! `name_expand`) contains exactly such garbage: `..._tinygeometry_6x00022cdb` has a
//! `6x` where the format string emits `_0x%08x`, so it is structurally impossible yet
//! it "verified". Mining keeps S small (real strings only), and `--shape` additionally
//! rejects tokens that cannot be asset names (hex literals, dotted paths). The residual
//! collision expectation is printed at the end — treat it as the error bar it is.
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin aset_external_mine -- \
//!       --names docs/data/aset_names.csv \
//!       --source game-files/ps3-VZ.WAD --source game-files/xbox-vz.strings.txt \
//!       --emit docs/data/aset_external_names.json

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;

use clap::Parser;
use rayon::prelude::*;

use mercs2_formats::hash::pandemic_hash_m2;

#[derive(Parser)]
#[command(about = "Mine ASET name preimages from console WADs / prototype images")]
struct Cli {
    /// Grouped ASET export (asset_hash,name,...,resolved,types,...) — supplies the targets.
    #[arg(long, default_value = "docs/data/aset_names.csv")]
    names: PathBuf,

    /// Any binary or text file to mine (repeatable): console WAD, ISO, xex, strings dump.
    #[arg(long)]
    source: Vec<PathBuf>,

    /// Extra name JSON fragments to fold in ({"pandemic_hash_m2": {"0x..": ["name"]}}).
    #[arg(long)]
    merge: Vec<PathBuf>,

    /// Game-roster CSV(s) whose first column is a vehicle/weapon/prop NAME (e.g.
    /// docs/data/lua_vehicle_hashes.csv, docs/data/spawnable_templates.csv). The name token is
    /// crossed with the authored `<faction>_veh_<class>_<name>` / `global_*_<name>` grammar and
    /// matched against unresolved model hashes — a structured, low-collision channel (own error bar).
    #[arg(long)]
    roster: Vec<PathBuf>,

    #[arg(long, default_value = "docs/data/aset_external_names.json")]
    emit: Option<PathBuf>,

    /// Reject tokens that cannot be asset names (hex literals, dotted paths, digit-led).
    #[arg(long, default_value_t = true)]
    shape: bool,

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

/// Could this token be an authored asset name? Asset names are lowercase identifier runs
/// that start with a letter. This screen is what separates a mine from a lottery.
///
/// The bare channel gets a STRICTER screen, and it has to. Scanning a 6 GB disc image
/// tests millions of tokens against ~13k targets, so ~10-25 of them hit a target by pure
/// 32-bit chance. Every one of those collisions observed here was short, underscore-less
/// gibberish — `cbjoxg`, `kdwjc`, `qxcvzq`, `rfwuf`, `xf4d8` — because that is what most
/// of a binary's byte-soup looks like. Real Mercs2 asset names are `zone_category_thing`.
/// Demanding an underscore and >=8 chars of a bare token removes the noise wholesale
/// while keeping `global_sandbagsstraightgr` and `pmcoutpost_statuediscus`.
///
/// Block-path and texture-sibling stems are EXEMPT: those tokens were structurally an
/// asset path already, so the hash is a second, independent witness rather than the only one.
fn plausible(s: &str, min_len: usize, prov: Prov) -> bool {
    if s.len() < min_len || s.len() > 90 {
        return false;
    }
    let b = s.as_bytes();
    if !b[0].is_ascii_alphabetic() {
        return false;
    }
    if s.contains('.') {
        return false;
    }
    // a hex blob like "266bc62d" or "0x2f" is never an asset name
    if !s.bytes().any(|c| c.is_ascii_alphabetic() && !c.is_ascii_hexdigit()) && !s.contains('_') {
        return false;
    }
    if prov == Prov::Bare && (!s.contains('_') || s.len() < 8) {
        return false;
    }
    true
}

/// Names an asset can be reached under: itself, its model stem (textures are
/// `<model>_dm/_nm/_sm`), and its block stem.
///
/// The block stem is the load-bearing one. The console WADs keep an uncompressed
/// block-path table, and a block path SPELLS its asset's name:
///   `blocks\vz\vz_state_alljob001_01_captured_tinygeometry_tgr21_tgc16_0x001439c1_P000_Q3.block`
/// Strip the `_P%03d_Q%d` LOD-rung suffix and the `.block` extension and what is left
/// is the preimage of the model hash. (Models that live INSIDE a shared c3 region
/// library have no block of their own, which is precisely why they stay unnamed.)
fn variants(s: &str) -> Vec<(String, Prov)> {
    let mut v = vec![(s.to_string(), Prov::Bare)];
    for suf in ["_dm", "_nm", "_sm", "_lod"] {
        if let Some(stem) = s.strip_suffix(suf) {
            v.push((stem.to_string(), Prov::TexStem));
        }
    }
    let t = s.strip_suffix(".block").unwrap_or(s);
    let tb = t.as_bytes();
    // `_P000_Q3` is 8 bytes: _ P d d d _ Q d
    if tb.len() > 8 {
        let tail = &tb[tb.len() - 8..];
        if tail[0] == b'_'
            && (tail[1] | 0x20) == b'p'
            && tail[2..5].iter().all(|c| c.is_ascii_digit())
            && tail[5] == b'_'
            && (tail[6] | 0x20) == b'q'
            && tail[7].is_ascii_digit()
        {
            v.push((t[..t.len() - 8].to_string(), Prov::BlockStem));
        }
    }
    v
}

/// Where a preimage came from — which is what its error bar is made of.
///
/// `BlockStem` is self-verifying: the token was a real block path, so the name is
/// corroborated by the path STRUCTURE as well as by the hash. The block-path table is
/// small (~40k tokens), so its chance-collision expectation is ~0.1 across all targets.
/// `Bare` tokens come from anywhere in a multi-GB image and carry the real risk — a hex
/// blob or a fragment of unrelated data can hash onto a target. Keep them apart.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Prov {
    BlockStem,
    TexStem,
    Bare,
    /// A name GENERATED by crossing a game-roster token with the authored naming grammar (not a
    /// literal string witnessed in a binary). Least trustworthy on a tie — a bare literal that
    /// appears in shipped bytes beats a generated combination — but the roster token is real and
    /// the grammar is authored, so the space is small and the printed error bar stays well under 1.
    Roster,
}

impl Prov {
    fn label(self) -> &'static str {
        match self {
            Prov::BlockStem => "block-path stem (self-verifying)",
            Prov::TexStem => "texture-sibling stem",
            Prov::Bare => "bare token (carries the collision risk)",
            Prov::Roster => "game-roster x grammar (generated; structured, own error bar)",
        }
    }
}

/// Structured naming candidates: the game roster supplies the NAME token, the authored grammar
/// supplies the frame. Vehicles are `<faction>_veh_<class>_<name>[<suffix>]`; weapons/props are
/// `global[_weapon|_prop|_env]_<name>[<suffix>]`. The roster token is a real name; the cross-product
/// is what carries the (bounded, printed) collision risk.
fn roster_candidates(rosters: &[PathBuf]) -> HashSet<String> {
    let mut tokens: HashSet<String> = HashSet::new();
    for p in rosters {
        let Ok(text) = std::fs::read_to_string(p) else {
            eprintln!("  (skipping absent roster {})", p.display());
            continue;
        };
        for line in text.lines().skip(1) {
            let f = csv_split(line);
            if f.is_empty() {
                continue;
            }
            // first column = display/name; drop parenthetical qualifiers, split on space/-//
            let nm = f[0].to_lowercase();
            let nm = nm.split('(').next().unwrap_or("").trim();
            let parts: Vec<&str> = nm
                .split(|c: char| c == ' ' || c == '-' || c == '/')
                .filter(|s| !s.is_empty())
                .collect();
            if parts.is_empty() {
                continue;
            }
            tokens.insert(parts.concat()); // amx30aa
            tokens.insert(parts.join("_")); // amx30_aa
            for p in &parts {
                if p.len() > 2 && p.chars().all(|c| c.is_ascii_alphanumeric()) {
                    tokens.insert((*p).to_string());
                }
            }
        }
    }
    const FACTIONS: [&str; 10] =
        ["al", "ch", "oc", "vz", "civ", "pmc", "pr", "gr", "us", "police"];
    const CLASSES: [&str; 15] = [
        "tank", "apc", "ifv", "car", "truck", "boat", "helicopter", "vtol", "plane",
        "motorcycle", "artillery", "spaag", "semi", "mlrs", "aa",
    ];
    const SUFFIXES: [&str; 11] = [
        "", "_command", "_elite", "_base", "_cargo", "_mlrs", "_semi", "_tanker", "_aa",
        "_racing", "_solano",
    ];
    const GLOBAL_PRE: [&str; 4] = ["global_weapon_", "global_", "global_prop_", "global_env_"];
    let mut cands: HashSet<String> = HashSet::new();
    for name in &tokens {
        for f in FACTIONS {
            for c in CLASSES {
                for s in SUFFIXES {
                    cands.insert(format!("{f}_veh_{c}_{name}{s}"));
                }
            }
        }
        for pre in GLOBAL_PRE {
            for s in SUFFIXES {
                cands.insert(format!("{pre}{name}{s}"));
            }
        }
    }
    cands
}

fn load_fragment(p: &PathBuf) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(p) else {
        return out;
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return out;
    };
    // accept both {"pandemic_hash_m2": {..}} and a bare {"0x..": ".."} map
    let obj = root
        .get("pandemic_hash_m2")
        .and_then(|v| v.as_object())
        .or_else(|| root.as_object());
    if let Some(o) = obj {
        for (k, v) in o {
            let Ok(h) = u32::from_str_radix(k.trim_start_matches("0x"), 16) else {
                continue;
            };
            let name = match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(a) => a.first().and_then(|x| x.as_str()).map(String::from),
                _ => None,
            };
            if let Some(n) = name {
                out.insert(h, n);
            }
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // ── targets ──────────────────────────────────────────────────────────────
    let text = std::fs::read_to_string(&cli.names)?;
    let mut lines = text.lines();
    let header = csv_split(lines.next().ok_or("empty names csv")?);
    let col = |n: &str| {
        header
            .iter()
            .position(|h| h == n)
            .ok_or(format!("no column {n}"))
    };
    let (c_hash, c_name, c_res, c_types) = (
        col("asset_hash")?,
        col("name")?,
        col("resolved")?,
        col("types")?,
    );

    let mut targets: HashSet<u32> = HashSet::new();
    let mut ttype: HashMap<u32, String> = HashMap::new();
    for line in lines {
        let f = csv_split(line);
        if f.len() <= c_types {
            continue;
        }
        let h = u32::from_str_radix(f[c_hash].trim_start_matches("0x"), 16)?;
        if !(f[c_res] == "1" && !f[c_name].is_empty()) {
            targets.insert(h);
            ttype.insert(h, f[c_types].clone());
        }
    }
    let n_model = targets.iter().filter(|h| ttype[h].contains("model")).count();
    eprintln!(
        "targets: {} unresolved ASET hashes ({} of type model)",
        targets.len(),
        n_model
    );

    let mut found: BTreeMap<u32, (String, Prov, String)> = BTreeMap::new();
    let mut tokens_tested: u64 = 0;
    // tokens per (source, channel) — noise is a property of the CHANNEL THAT FOUND IT,
    // so a 187-token wordlist must not inherit the error bar of a 6 GB disc image
    let mut chan_tokens: BTreeMap<(String, Prov), u64> = BTreeMap::new();

    // ── mine each source ─────────────────────────────────────────────────────
    for p in &cli.source {
        if !p.is_file() {
            eprintln!("  (skipping absent {})", p.display());
            continue;
        }
        let mut f = std::fs::File::open(p)?;
        let mut buf = vec![0u8; 64 << 20];
        let mut carry: Vec<u8> = Vec::new();
        let mut toks: HashSet<Vec<u8>> = HashSet::new();
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let mut cur = std::mem::take(&mut carry);
            for &b in &buf[..n] {
                if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
                    cur.push(b.to_ascii_lowercase());
                } else {
                    if cur.len() >= cli.min_len {
                        toks.insert(std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
            }
            // a token straddling the chunk boundary must survive into the next read
            carry = cur;
        }
        if carry.len() >= cli.min_len {
            toks.insert(carry);
        }

        let scanned: Vec<(Vec<(u32, String, Prov)>, [u64; 3])> = toks
            .par_iter()
            .map(|t| {
                let s = String::from_utf8_lossy(t).into_owned();
                let mut out = Vec::new();
                let mut counts = [0u64; 3];
                for (v, prov) in variants(&s) {
                    if cli.shape && !plausible(&v, cli.min_len, prov) {
                        continue;
                    }
                    counts[prov as usize] += 1;
                    let h = pandemic_hash_m2(&v);
                    if targets.contains(&h) {
                        out.push((h, v, prov));
                    }
                }
                (out, counts)
            })
            .collect();

        let src = p.file_name().unwrap().to_string_lossy().into_owned();
        let before = found.len();
        for (hits, counts) in scanned {
            for (h, s, prov) in hits {
                // keep the most trustworthy provenance we ever see for a hash
                match found.get(&h) {
                    Some((_, p, _)) if *p <= prov => {}
                    _ => {
                        found.insert(h, (s, prov, src.clone()));
                    }
                }
            }
            for (i, c) in counts.iter().enumerate() {
                let pv = [Prov::BlockStem, Prov::TexStem, Prov::Bare][i];
                *chan_tokens.entry((src.clone(), pv)).or_insert(0) += c;
            }
        }
        tokens_tested += toks.len() as u64;
        eprintln!(
            "  {:<44} {:>10} tokens -> +{} preimages",
            p.file_name().unwrap().to_string_lossy(),
            toks.len(),
            found.len() - before
        );
    }

    // ── roster cross-product channel (structured game-roster names) ──────────
    if !cli.roster.is_empty() {
        // Roster candidates ARE model names, so they only ever match MODEL-type targets. Restricting
        // the match keeps a texture/animation hash from being mis-named and, more importantly, shrinks
        // the effective target count from all-unresolved to just the models — the error bar is
        // n_cand * n_models / 2^32, not * all_targets.
        let model_targets: HashSet<u32> =
            targets.iter().copied().filter(|h| ttype[h].contains("model")).collect();
        let cands = roster_candidates(&cli.roster);
        let before = found.len();
        let mut n_cand: u64 = 0;
        for cand in &cands {
            if cli.shape && !plausible(cand, cli.min_len, Prov::Roster) {
                continue;
            }
            n_cand += 1;
            let h = pandemic_hash_m2(cand);
            if model_targets.contains(&h) {
                match found.get(&h) {
                    Some((_, p, _)) if *p <= Prov::Roster => {}
                    _ => {
                        found.insert(h, (cand.clone(), Prov::Roster, "roster".into()));
                    }
                }
            }
        }
        // The channel's error bar is model-restricted; store scaled tokens so the generic noise
        // table (which multiplies by all targets) reports the correct model-only expectation.
        let scaled = if targets.is_empty() {
            0
        } else {
            n_cand * model_targets.len() as u64 / targets.len().max(1) as u64
        };
        *chan_tokens.entry(("roster".into(), Prov::Roster)).or_insert(0) += scaled;
        tokens_tested += n_cand;
        let noise = n_cand as f64 * model_targets.len() as f64 / 4_294_967_296.0;
        eprintln!(
            "  {:<44} {:>10} candidates -> +{} preimages  (vs {} models, noise {:.2})",
            "roster x grammar",
            n_cand,
            found.len() - before,
            model_targets.len(),
            noise
        );
    }

    // ── fold in externally-supplied fragments ────────────────────────────────
    for m in &cli.merge {
        let frag = load_fragment(m);
        let mut new = 0;
        let mut conflict = 0;
        for (h, n) in frag {
            if !targets.contains(&h) {
                continue;
            }
            let src = m.file_name().unwrap().to_string_lossy().into_owned();
            match found.get(&h) {
                Some((existing, _, _)) if *existing != n => conflict += 1,
                Some(_) => {}
                None => {
                    found.insert(h, (n, Prov::Bare, src));
                    new += 1;
                }
            }
        }
        eprintln!(
            "  merge {:<38} +{new} preimages{}",
            m.file_name().unwrap().to_string_lossy(),
            if conflict > 0 {
                format!("  ({conflict} DISAGREE with the mine — inspect)")
            } else {
                String::new()
            }
        );
    }

    // ── emit ─────────────────────────────────────────────────────────────────
    let models = found.keys().filter(|h| ttype[h].contains("model")).count();
    if let Some(out) = &cli.emit {
        let map: BTreeMap<String, Vec<String>> = found
            .iter()
            .map(|(h, (n, _, _))| (format!("0x{h:08X}"), vec![n.clone()]))
            .collect();
        let mut root = serde_json::Map::new();
        root.insert("pandemic_hash_m2".into(), serde_json::to_value(&map)?);
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(out, serde_json::to_string_pretty(&root)?)?;
        eprintln!("\nWROTE {}", out.display());

        // Sidecar audit trail: every name, and exactly which witness produced it.
        let audit = out.with_extension("csv");
        let mut w = String::from("asset_hash,name,provenance,source,types\n");
        for (h, (n, p, s)) in &found {
            w.push_str(&format!(
                "0x{h:08X},{n},{},{s},{}\n",
                match p {
                    Prov::BlockStem => "block-stem",
                    Prov::TexStem => "tex-stem",
                    Prov::Bare => "bare",
                    Prov::Roster => "roster",
                },
                ttype[h]
            ));
        }
        std::fs::write(&audit, w)?;
        eprintln!("WROTE {}", audit.display());
    }

    eprintln!(
        "\n{} preimages ({} MODEL: {}/{} unresolved models named, {} still unnamed)\n",
        found.len(),
        models,
        models,
        n_model,
        n_model - models
    );

    // Per-(source, channel) error bar. S*T/2^32 is the number of preimages that channel
    // would have produced BY CHANCE — read it as "how many of these hits are noise".
    eprintln!(
        "  {:<34} {:<12} {:>6} {:>9} {:>8}",
        "source", "channel", "names", "tokens", "noise"
    );
    let mut rows: Vec<(String, Prov, usize, u64, f64)> = Vec::new();
    for ((src, prov), toks) in &chan_tokens {
        let n = found
            .values()
            .filter(|(_, p, s)| p == prov && s == src)
            .count();
        if n == 0 && *toks == 0 {
            continue;
        }
        let noise = *toks as f64 * targets.len() as f64 / 4_294_967_296.0;
        rows.push((src.clone(), *prov, n, *toks, noise));
    }
    rows.sort_by(|a, b| b.2.cmp(&a.2));
    for (src, prov, n, toks, noise) in &rows {
        if *n == 0 {
            continue;
        }
        let src = if src.len() > 33 { &src[..33] } else { src };
        let ch = match prov {
            Prov::BlockStem => "block-stem",
            Prov::TexStem => "tex-stem",
            Prov::Bare => "bare",
            Prov::Roster => "roster",
        };
        eprintln!("  {src:<34} {ch:<12} {n:>6} {toks:>9} {noise:>8.2}");
    }
    eprintln!("\n  channels, most→least trustworthy:");
    for p in [Prov::BlockStem, Prov::TexStem, Prov::Bare, Prov::Roster] {
        eprintln!("    {:<12} {}", format!("{p:?}"), p.label());
    }
    eprintln!(
        "\n  A name is trustworthy exactly to the degree its row's `noise` sits below its\n  \
         `names`. block-stem rows are self-verifying (the token WAS the asset's path);\n  \
         a bare row over a multi-GB image is where the collisions live. {} tokens total.",
        tokens_tested
    );
    Ok(())
}
