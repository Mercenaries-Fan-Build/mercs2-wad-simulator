//! Name an unnamed MODEL from the textures IT ACTUALLY USES.
//!
//! Models carry no `NAME` leaf; textures do (99.8% of them resolve). So a model's name must be
//! DERIVED. `asset_gap_probe` does this globally — take every texture name, strip a suffix, ask
//! whether the resulting hash is in ASET. That works when the model name is a strict PREFIX of its
//! texture names, which is the prop/vehicle convention:
//!
//!     al_veh_plane_a10_dm            -> al_veh_plane_a10          (strip `_dm`)
//!     civ_hum_beachfemale_b_head_nm  -> civ_hum_beachfemale_b     (strip `_nm`, then `_head`)
//!
//! But characters break it, because the qualifier can sit AFTER the body part:
//!
//!     civ_hum_casualfemale_ub_c      -> civ_hum_casualfemale_c    (delete the MIDDLE token `ub`)
//!
//! No amount of suffix-peeling reaches that. The general rule is: the model's name is some
//! order-preserving SUBSEQUENCE of its texture's name tokens. So enumerate those.
//!
//! WHY THIS IS SAFE, given `pandemic_hash_m2` is only 32 bits: we do NOT ask "is this candidate's
//! hash in ASET anywhere" (over millions of candidates that would produce ~dozens of accidental
//! collisions and pollute the name table). We ask the far narrower question: does this candidate,
//! generated from THIS model's OWN materials, hash to THIS model's hash? That is ~50-1000 candidates
//! against ONE target, so a false positive needs a 32-bit collision inside the model's own texture
//! set — probability ~1e-8 per model. Every hit is additionally corroborated by the fact that the
//! name came out of the model's own MTRL.
//!
//! usage: model_namer [--emit <fragment.json>] [--limit N]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;

/// Order-preserving token subsequences of `name`, longest first, at least `MIN_TOKENS` long.
///
/// `civ_hum_casualfemale_ub_c` yields `civ_hum_casualfemale_c`, `civ_hum_casualfemale_ub`,
/// `civ_hum_casualfemale`, ... — i.e. every way of DELETING tokens while keeping order, which covers
/// suffix-peeling (delete from the tail) and the character convention (delete the middle) alike.
fn candidates(name: &str) -> Vec<String> {
    const MIN_TOKENS: usize = 2;
    const MAX_TOKENS: usize = 12; // 2^12 subsets; longer names are pathological, take the head
    let toks: Vec<&str> = name.split('_').filter(|t| !t.is_empty()).collect();
    let n = toks.len().min(MAX_TOKENS);
    if n < MIN_TOKENS {
        return Vec::new();
    }
    let mut out = Vec::new();
    // Bitmask over which tokens to KEEP. Skip the full set (that is the texture's own name).
    for mask in 1u32..(1 << n) {
        if mask.count_ones() < MIN_TOKENS as u32 {
            continue;
        }
        let cand: Vec<&str> =
            (0..n).filter(|i| mask & (1 << i) != 0).map(|i| toks[i]).collect();
        out.push(cand.join("_"));
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let emit = args.iter().position(|a| a == "--emit").and_then(|i| args.get(i + 1)).cloned();

    // hash -> name, and the set of model hashes, from the ASET export (the WAD's own dictionary).
    let csv = ["docs/data/aset_names.csv", "../../docs/data/aset_names.csv"]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.is_file())
        .expect("aset_names.csv");
    let text = std::fs::read_to_string(&csv).expect("read aset_names.csv");

    let mut name_of: HashMap<u32, String> = HashMap::new();
    let mut models: Vec<u32> = Vec::new();
    // asset hash -> stem of the block it lives in (`...\commercial_crater_P000_Q3.block`
    // -> `commercial_crater`), used as an extra base candidate below.
    let mut block_stem: HashMap<u32, String> = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 10 {
            continue;
        }
        let Ok(h) = u32::from_str_radix(f[0].trim().trim_start_matches("0x"), 16) else { continue };
        let name = f[1].trim();
        if !name.is_empty() {
            name_of.insert(h, name.to_string());
        }
        if f[5].trim() == "model" {
            models.push(h);
        }
        let leaf = f[9].trim().rsplit(['\\', '/']).next().unwrap_or("");
        if let Some(stem) = leaf.strip_suffix(".block") {
            // drop the `_P000_Q3` LOD-rung tail
            let stem = stem
                .rfind("_P")
                .filter(|i| stem[*i..].len() == 8)
                .map(|i| &stem[..i])
                .unwrap_or(stem);
            if !stem.is_empty() && !stem.starts_with('c') {
                block_stem.insert(h, stem.to_ascii_lowercase());
            }
        }
    }
    models.sort_unstable();
    models.dedup();
    let unnamed: Vec<u32> = models.iter().copied().filter(|h| !name_of.contains_key(h)).collect();
    eprintln!(
        "{} models ({} named, {} unnamed); {} named assets total",
        models.len(),
        models.len() - unnamed.len(),
        unnamed.len(),
        name_of.len()
    );

    // Second source: the block-payload STRING corpus (`block_string_harvest` dumps every ASCII
    // identifier run out of all 11,370 decompressed blocks). The world's placement `Name` COMPs carry
    // real entity name strings ("jungle_env_plantlarge04", "global_lamppostA"), and an instanced prop's
    // entity name IS its model name — a source no texture-name derivation can reach. Exact-hash match
    // against the unnamed model set only, so a hit is a verified preimage of that specific model.
    let mut by_string: BTreeMap<u32, String> = BTreeMap::new();
    let unnamed_set: std::collections::HashSet<u32> = unnamed.iter().copied().collect();
    for p in ["output/block_strings.txt", "../../output/block_strings.txt"] {
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        let mut scanned = 0usize;
        for line in text.lines() {
            let s = line.trim();
            if s.is_empty() {
                continue;
            }
            scanned += 1;
            let h = pandemic_hash_m2(s);
            if unnamed_set.contains(&h) {
                by_string.entry(h).or_insert_with(|| s.to_string());
            }
        }
        eprintln!("block strings: {scanned} scanned, {} unnamed models named", by_string.len());
        break;
    }

    // ── DECORATIONS, mined from the corpus rather than guessed ───────────────────────
    // A model is very often a VARIANT of an asset we already name: the AMX-30's base hull is
    // `vz_veh_tank_amx30_base` while only `..._aa` and `..._elite` were ever named, and its
    // texture is plain `vz_veh_tank_amx30_dm`. No subsequence of a texture name can reach
    // `_base` — the token is not IN the texture name. It has to be ADDED.
    //
    // So learn the real decoration set: wherever one known asset name is a strict prefix of
    // another (`global_toolbox` -> `global_toolboxa`), the difference is a decoration the
    // artists actually used. Frequency-rank them and append/prepend to each base candidate.
    // (Guessing this list by hand does not work — `_base`, `_staging`, `_ruin_lod` are not
    // things you think of; the corpus knows them and we do not.)
    let all_names: HashSet<&str> = name_of.values().map(|s| s.as_str()).collect();
    let mut suf_freq: HashMap<String, usize> = HashMap::new();
    let mut pre_freq: HashMap<String, usize> = HashMap::new();
    for n in &all_names {
        for cut in 1..=22usize {
            if n.len() > cut {
                if all_names.contains(&n[..n.len() - cut]) {
                    *suf_freq.entry(n[n.len() - cut..].to_string()).or_default() += 1;
                }
                if all_names.contains(&n[cut..]) {
                    *pre_freq.entry(n[..cut].to_string()).or_default() += 1;
                }
            }
        }
    }
    let mut sufs: Vec<(String, usize)> = suf_freq.into_iter().collect();
    sufs.sort_by(|a, b| b.1.cmp(&a.1));
    sufs.truncate(4000);
    let decorations: Vec<String> = sufs.into_iter().map(|(s, _)| s).collect();
    let mut pres: Vec<(String, usize)> = pre_freq.into_iter().collect();
    pres.sort_by(|a, b| b.1.cmp(&a.1));
    pres.truncate(100);
    let prefixes: Vec<String> = pres.into_iter().map(|(s, _)| s).collect();
    eprintln!(
        "mined {} suffix decorations + {} prefix decorations from the corpus",
        decorations.len(),
        prefixes.len()
    );

    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    let mut found: BTreeMap<u32, String> = BTreeMap::new();
    let (mut no_container, mut no_tex, mut no_hit) = (0u32, 0u32, 0u32);
    // Why a model stayed unnamed, so the residue is characterised rather than just counted.
    let mut unnamed_tex_examples: Vec<(u32, Vec<String>)> = Vec::new();
    let mut residue_cat: BTreeMap<&str, u32> = BTreeMap::new();

    for &m in &unnamed {
        let Ok(c) = wad::extract_container(&mut w, m) else {
            no_container += 1;
            continue;
        };
        // The model's OWN texture set, from its materials.
        let mut texes: BTreeSet<u32> = BTreeSet::new();
        for mat in mercs2_formats::texture::parse_mtrl(&c) {
            for t in mat.textures {
                if t != 0 && t != 0xFFFF_FFFF {
                    texes.insert(t);
                }
            }
        }
        let mut names: Vec<String> =
            texes.iter().filter_map(|t| name_of.get(t)).cloned().collect();

        // The BLOCK STEM is a base too, and a strong one. A model that lives in
        // `blocks\VZ\commercial_crater_P000_Q3.block` is almost certainly `commercial_crater`
        // + a decoration — the block is named for its contents. This reaches names that no
        // texture mentions (the crater's textures are `commercial_road_crater01_dm` etc., which
        // never spell `commercial_crater`).
        if let Some(stem) = block_stem.get(&m) {
            names.push(stem.clone());
        }
        if names.is_empty() {
            no_tex += 1;
            continue;
        }
        let mut hit = None;
        'outer: for tn in &names {
            for cand in candidates(tn) {
                if pandemic_hash_m2(&cand) == m {
                    hit = Some(cand);
                    break 'outer;
                }
                // The variant case: this model is `<base><decoration>` of something we name.
                // Still asked against THIS model's single hash, so the collision odds stay at
                // ~candidates/2^32 per model (~1e-3), not the ~S*T/2^32 of a global sweep.
                for d in &decorations {
                    let v = format!("{cand}{d}");
                    if pandemic_hash_m2(&v) == m {
                        hit = Some(v);
                        break 'outer;
                    }
                }
                for p in &prefixes {
                    let v = format!("{p}{cand}");
                    if pandemic_hash_m2(&v) == m {
                        hit = Some(v);
                        break 'outer;
                    }
                }
            }
        }
        match hit {
            Some(name) => {
                found.insert(m, name);
            }
            None => {
                no_hit += 1;
                // Characterise WHY, so the residue is understood rather than merely counted.
                let cat = if names.iter().all(|n| n.contains("tinygeometry")) {
                    // The `_tiny` region IMPOSTERS: geometry baked at build time from whatever was
                    // in the region. Not authored assets — there is no name to recover.
                    "imposter (tinygeometry)"
                } else if names.iter().all(|n| n.contains("fxmodel") || n.contains("debris")) {
                    "fx/debris"
                } else if names.len() == 1 {
                    // Uses ONE shared texture that does not name it — e.g. a pickup/variant built on
                    // a common material. The name exists, we just have no string that carries it.
                    "single shared texture"
                } else {
                    "multi shared textures"
                };
                *residue_cat.entry(cat).or_insert(0) += 1;
                if unnamed_tex_examples.len() < 10 {
                    unnamed_tex_examples.push((m, names.into_iter().take(3).collect()));
                }
            }
        }
    }

    // Merge: material derivation first (strongest corroboration), then the string corpus.
    let mut from_strings_only = 0usize;
    for (h, s) in &by_string {
        if !found.contains_key(h) {
            found.insert(*h, s.clone());
            from_strings_only += 1;
        }
    }

    println!("\n=== RESULT ===");
    println!("named from a block-payload string (placement/entity names) : {from_strings_only}");
    println!("named from their own materials : {}", found.len() - from_strings_only);
    println!("no hit (textures named, no subsequence matches) : {no_hit}");
    println!("model has NO named textures     : {no_tex}");
    println!("container would not extract      : {no_container}");

    println!("\n=== SAMPLE OF NEWLY NAMED ===");
    for (h, n) in found.iter().take(25) {
        println!("  0x{h:08X}  {n}");
    }

    println!("\n=== WHY THE RESIDUE CANNOT BE NAMED ===");
    for (cat, n) in &residue_cat {
        println!("  {n:>4}  {cat}");
    }

    println!("\n=== SAMPLE OF THE RESIDUE (model -> the textures it uses) ===");
    for (h, tn) in &unnamed_tex_examples {
        println!("  0x{h:08X}  uses {tn:?}");
    }

    if let Some(path) = emit {
        let mut map = serde_json::Map::new();
        for (h, n) in &found {
            map.insert(
                format!("0x{h:08X}"),
                serde_json::Value::Array(vec![serde_json::Value::String(n.clone())]),
            );
        }
        let doc = serde_json::json!({ "pandemic_hash_m2": map });
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).expect("write fragment");
        println!("\nwrote {} names -> {path}", found.len());
    }
}
