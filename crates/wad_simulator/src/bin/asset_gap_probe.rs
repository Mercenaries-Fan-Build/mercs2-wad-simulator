//! Find assets whose TEXTURES shipped but whose MODEL did not — and brute-force
//! the vehicle/aircraft roster using the engine's own naming convention.
//!
//! Two facts make this possible, both established by `aset_export`:
//!
//!   1. Asset names follow a strict convention: a model is `X`, and its textures
//!      are `X_dm` (diffuse), `X_nm` (normal), `X_sm` (specular), plus `X_lod_dm`,
//!      `X_ruin*`, etc. So a resolved TEXTURE name hands us the exact preimage of
//!      its MODEL's hash — no guessing.
//!   2. `pandemic_hash_m2` is a pure function, so we can ask the decisive question
//!      directly: does `pandemic_hash_m2(X)` appear in ASET as a model row?
//!
//! A base name whose textures are present but whose model hash is absent from ASET
//! is content that was CUT or is MISSING from the shipped WAD — the textures got
//! left behind. `vz_veh_helicopter_mi26_wheels` is the motivating case: it ships
//! _dm/_nm/_sm and no model.
//!
//! Family 2 brute-forces `<faction>_veh_<class>_<airframe>` combinations against the
//! ASET hash set, which surfaces vehicles whose names are absent from the rainbow
//! table entirely (so `aset_export` could never have named them).
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin asset_gap_probe -- \
//!       --export docs/data/aset_export.csv --filter helicopter,vtol,plane

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use clap::Parser;
use mercs2_formats::hash::pandemic_hash_m2;

#[derive(Parser)]
#[command(about = "Find assets with textures but no model; brute-force the vehicle roster")]
struct Cli {
    #[arg(long, default_value = "docs/data/aset_export.csv")]
    export: PathBuf,

    /// Only report base names containing one of these comma-separated tokens.
    /// Empty = report every gap found.
    #[arg(long, default_value = "")]
    filter: String,

    /// Write confirmed convention hits (names whose hash IS in ASET) as a
    /// rainbow-table fragment that `aset_export --rainbow` can merge.
    #[arg(long)]
    emit: Option<PathBuf>,
}

/// Every base name a texture's name can legitimately derive, longest first.
///
/// A PROP or VEHICLE model `X` names its textures `X_dm`/`X_nm`/`X_sm`, so stripping the material-map
/// suffix lands straight on the model. A CHARACTER does not: its textures are per BODY PART —
/// `civ_hum_beachfemale_b_head`, `_ub`, `_lb`, `_hair` — and those stack with a map suffix
/// (`civ_hum_beachfemale_b_head_nm`). Stripping only the map suffix therefore lands on ANOTHER
/// TEXTURE's name, never the model's. That is why all three beachfemale models sat unnamed in the
/// workshop while their 33 textures resolved: the model is two suffixes up, not one.
///
/// Rather than curate a body-part list and miss whatever it omits, peel trailing `_token`s
/// progressively and let the WAD arbitrate. This CANNOT mis-name anything: the caller records
/// `hash(candidate) -> candidate`, i.e. a verified PREIMAGE, and only when that hash is really an
/// asset in the WAD. A wrong guess hashes to nothing and is dropped.
fn candidate_bases(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    // Longest matching map suffix wins ("_lod_dm" beats "_dm").
    let mut best: Option<&str> = None;
    for s in TEX_SUFFIXES {
        if lower.ends_with(s) && best.map(|b| s.len() > b.len()).unwrap_or(true) {
            best = Some(s);
        }
    }
    let mut base = name.to_string();
    if let Some(sfx) = best {
        base.truncate(base.len() - sfx.len());
    }
    if base.is_empty() {
        return Vec::new();
    }

    // `civ_hum_beachfemale_b_head` -> `civ_hum_beachfemale_b` -> `civ_hum_beachfemale`. Stop at
    // MIN_TOKENS so we never walk up to a meaningless stem ("civ_hum") that could belong to anything.
    const MIN_TOKENS: usize = 3;
    let mut out = vec![base.clone()];
    let mut cur = base;
    while cur.matches('_').count() + 1 > MIN_TOKENS {
        let Some(i) = cur.rfind('_') else { break };
        cur.truncate(i);
        out.push(cur.clone());
    }
    out
}

/// Texture suffixes appended to a model's base name (engine convention).
const TEX_SUFFIXES: &[&str] = &[
    "_dm", "_nm", "_sm", "_em", "_lod_dm", "_lod_nm", "_lod_sm", "_lod",
    "_ruin_dm", "_ruin_nm", "_ruin_sm", "_ruin", "_ruin_lod_dm", "_ruin_lod",
    "_dm1", "_dm2", "_nm1", "_nm2", "_detail", "_spec", "_mask",
];

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let tokens: Vec<String> = cli
        .filter
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let want = |name: &str| -> bool {
        tokens.is_empty() || tokens.iter().any(|t| name.to_lowercase().contains(t))
    };

    // ── load the ASET export ────────────────────────────────────────
    // columns: wad,row_index,asset_hash,name,name_candidates,candidate_count,
    //          type_id,type_name,type_hash,block_index,block_path,...
    let text = std::fs::read_to_string(&cli.export)?;
    let mut hash_types: HashMap<u32, BTreeSet<String>> = HashMap::new();
    let mut name_of: HashMap<u32, String> = HashMap::new();
    // base name -> (texture names seen, blocks seen)
    let mut tex_bases: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    /// Deeper name candidates (see `candidate_bases`) — the character body-part convention needs
    /// more than one suffix peeled to reach the model.
    let mut derived_bases: BTreeSet<String> = BTreeSet::new();

    for line in text.lines().skip(1) {
        let f = csv_split(line);
        if f.len() < 11 {
            continue;
        }
        let h = match u32::from_str_radix(f[2].trim().trim_start_matches("0x"), 16) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = f[3].clone();
        let tname = f[7].clone();
        let block = f[10].clone();

        hash_types.entry(h).or_default().insert(tname.clone());
        if !name.is_empty() {
            name_of.insert(h, name.clone());
        }

        if tname == "texture" && !name.is_empty() {
            let lower = name.to_lowercase();
            // Longest matching suffix wins (so "_lod_dm" beats "_dm").
            let mut best: Option<&str> = None;
            for s in TEX_SUFFIXES {
                if lower.ends_with(s)
                    && best.map(|b| s.len() > b.len()).unwrap_or(true)
                {
                    best = Some(s);
                }
            }
            if let Some(sfx) = best {
                let base = name[..name.len() - sfx.len()].to_string();
                if !base.is_empty() {
                    let e = tex_bases.entry(base).or_default();
                    e.0.insert(name.clone());
                    e.1.insert(block.clone());
                }
            }
            // Kept SEPARATE from `tex_bases` on purpose: `tex_bases` drives the "textures shipped,
            // model missing" gap report, whose meaning depends on the strict one-suffix rule. These
            // deeper candidates only feed the name emit, where the ASET hash set verifies each one.
            derived_bases.extend(candidate_bases(&name));
        }
    }
    eprintln!(
        "{} distinct ASET hashes, {} named; {} texture base names",
        hash_types.len(),
        name_of.len(),
        tex_bases.len()
    );

    let has_model = |h: u32| -> bool {
        hash_types
            .get(&h)
            .map(|t| t.contains("model"))
            .unwrap_or(false)
    };

    // ── family 1: textures present, model absent ────────────────────
    println!("\n=== TEXTURES SHIPPED, MODEL MISSING FROM ASET ===");
    println!("(base name is a verified preimage taken from its own texture names)\n");
    let mut gaps = 0usize;
    let mut gaps_shown = 0usize;
    for (base, (texes, blocks)) in &tex_bases {
        let h = pandemic_hash_m2(base);
        if hash_types.contains_key(&h) {
            continue; // the base name exists in ASET as *something*
        }
        gaps += 1;
        if !want(base) {
            continue;
        }
        gaps_shown += 1;
        let blk = blocks
            .iter()
            .next()
            .map(|s| s.as_str())
            .unwrap_or("");
        println!(
            "  0x{h:08X}  {base}\n              textures: {}\n              block: {blk}",
            texes.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    println!(
        "\n  {gaps} base names have textures but NO ASET entry of any type ({gaps_shown} shown)"
    );

    // Same, but the base name IS in ASET yet not as a model (e.g. only audio).
    println!("\n=== BASE NAME IN ASET, BUT NOT AS A MODEL ===");
    for (base, (texes, _)) in &tex_bases {
        let h = pandemic_hash_m2(base);
        if let Some(types) = hash_types.get(&h) {
            if !has_model(h) && want(base) {
                println!(
                    "  0x{h:08X}  {base}  [present as: {}]  textures: {}",
                    types.iter().cloned().collect::<Vec<_>>().join(";"),
                    texes.len()
                );
            }
        }
    }

    // ── family 2: vehicle-convention brute force ────────────────────
    // <faction>_veh_<class>_<airframe>, using the engine's own taxonomy.
    println!("\n=== VEHICLE-CONVENTION BRUTE FORCE (hits present in ASET) ===");
    let factions = [
        "al", "ch", "oc", "vz", "civ", "pmc", "gur", "ven", "us", "un", "mil",
        "pla", "gov", "reb", "aa", "chi", "allied", "pirate", "pr", "global",
    ];
    let classes = ["helicopter", "vtol", "plane", "jet", "heli", "aircraft", "airplane"];
    let airframes = [
        // rotary
        "mi26", "mi26_wheels", "mi8", "mi17", "mi24", "mi28", "mi35hind", "mi35hind_solano",
        "ka29b", "ka50", "ka52", "wz10", "md500", "alouetteiii", "alouette", "uh1", "uh1h",
        "uh60", "ah1", "ah1z", "ah6", "ah64", "apache", "hind", "huey", "blackhawk", "cobra",
        "littlebird", "chinook", "ch47", "seaknight", "sealion", "lynx", "gazelle", "puma",
        "hip", "hokum", "havoc", "werewolf", "z9", "z10", "ec135", "bell",
        "mh53", "mh53pavelow", "pavelow", "ch53", "ka29",
        // fixed wing
        "f35b", "f35", "harrier", "av8b", "c130", "c17", "an124", "an225", "il76",
        "su25", "su27", "mig29", "mig27", "f16", "f18", "a10", "dc3", "cessna", "learjet",
        "osprey", "v22", "predator", "uav",
        "ac130", "ac130_spooky", "spooky", "ov10bronco", "ov10", "bronco",
        "tucano", "tucano_001", "tucano_002", "727jet", "727", "transport",
    ];
    let mut hits: BTreeMap<u32, (String, String)> = BTreeMap::new();
    for f in &factions {
        for c in &classes {
            for a in &airframes {
                for cand in [
                    format!("{f}_veh_{c}_{a}"),
                    format!("veh_{c}_{a}"),
                    format!("{f}_veh_{c}{a}"),
                ] {
                    let h = pandemic_hash_m2(&cand);
                    if let Some(types) = hash_types.get(&h) {
                        hits.entry(h).or_insert((
                            cand,
                            types.iter().cloned().collect::<Vec<_>>().join(";"),
                        ));
                    }
                }
            }
        }
    }
    for (h, (name, types)) in &hits {
        let known = if name_of.contains_key(h) {
            "already-named"
        } else {
            "*** NEW (absent from rainbow table) ***"
        };
        println!("  0x{h:08X}  {name:<40} [{types}]  {known}");
    }
    println!("\n  {} vehicle-convention names found in ASET", hits.len());

    // ── emit the discoveries as a rainbow fragment ──────────────────
    // Two sources, both VERIFIED preimages (we only record a name whose hash is
    // actually present in ASET):
    //   a) texture-base derivation — `X_dm` names its own model `X`. This is the
    //      bulk win: textures are ~95% named but models only ~46%, and the gap is
    //      pure convention, so most unnamed models are recoverable this way.
    //   b) the vehicle-convention brute force above.
    if let Some(path) = &cli.emit {
        // Emit EVERY confirmed hit, including ones the export already names.
        //
        // Do NOT skip already-named hashes: this probe reads `aset_export.csv`, and that export
        // is itself produced with this fragment merged in. Skipping "already named" would treat
        // our own previous discoveries as pre-existing and drop them from the next fragment —
        // a self-referential loop that silently loses names on every second run. The fragment is
        // therefore a self-contained superset, and merging is idempotent (the main rainbow table
        // is merged first and wins; this only ever fills gaps).
        let mut map = serde_json::Map::new();
        let mut by_tex = 0usize;
        let mut fresh = 0usize;
        let mut record = |h: u32, name: &str, map: &mut serde_json::Map<String, serde_json::Value>| -> bool {
            let added = map
                .insert(
                    format!("0x{h:08X}"),
                    serde_json::Value::Array(vec![serde_json::Value::String(name.to_string())]),
                )
                .is_none();
            if added && !name_of.contains_key(&h) {
                fresh += 1;
            }
            added
        };

        for base in tex_bases.keys() {
            let h = pandemic_hash_m2(base);
            if !hash_types.contains_key(&h) {
                continue; // the derived name is not a real asset in this WAD
            }
            if record(h, base, &mut map) {
                by_tex += 1;
            }
        }
        // The deeper candidates: a character's model sits TWO suffixes up from its textures
        // (`civ_hum_beachfemale_b_head_nm` -> `civ_hum_beachfemale_b`). Same verification gate.
        let mut by_deep = 0usize;
        let mut deep_models = 0usize;
        for base in &derived_bases {
            let h = pandemic_hash_m2(base);
            if !hash_types.contains_key(&h) {
                continue;
            }
            if record(h, base, &mut map) {
                by_deep += 1;
                if has_model(h) {
                    deep_models += 1;
                }
            }
        }
        for (h, (name, _)) in &hits {
            record(*h, name, &mut map);
        }
        println!(
            "\n  emit: {} names ({by_tex} from texture-base derivation, \
             {by_deep} from deeper body-part derivation of which {deep_models} are MODELS); \
             {fresh} not otherwise resolvable",
            map.len()
        );
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let root = serde_json::json!({ "pandemic_hash_m2": map });
        std::fs::write(path, serde_json::to_string_pretty(&root)?)?;
        println!("Wrote {}", path.display());
    }

    Ok(())
}
