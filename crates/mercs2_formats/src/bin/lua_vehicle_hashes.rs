//! Extract vehicle names referenced by the decompiled Lua corpora and emit a
//! CSV with their `pandemic_hash_m2` values.
//!
//! Scans `docs/mercs2-luacd/src` + `docs/mercs2-dlc-luacd/src` for string
//! literals passed to the vehicle-carrying calls:
//!   `Pg.Spawn("NAME", ...)`          -> spawn   (classified; spawns also cover
//!                                                particles/characters/ordnance)
//!   `:SetCargo("NAME")`              -> cargo   (support-store vehicle deliveries)
//!   `:SetDeliveryVehicle("NAME")`    -> delivery
//!   `Airstrike.Flyby("NAME", ...)`   -> flyby
//!   `GetGuidByName("NAME 0x......")` -> placed  (world-placed instance; the
//!                                                per-instance hex suffix is
//!                                                stripped before hashing)
//!   `GetGuidByName("NAME")`          -> named   (only if it classifies as a vehicle)
//!
//! Usage (from the repo root):
//!   cargo run --manifest-path tools/wad_simulator/Cargo.toml -p mercs2_formats \
//!       --bin lua_vehicle_hashes [-- <out.csv>]

use mercs2_formats::hash::pandemic_hash_m2;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CORPORA: [&str; 2] = ["docs/mercs2-luacd/src", "docs/mercs2-dlc-luacd/src"];
const DEFAULT_OUT: &str = "docs/data/lua_vehicle_hashes.csv";

const CALLS: [(&str, &str); 5] = [
    ("Pg.Spawn(", "spawn"),
    (":SetCargo(", "cargo"),
    (":SetDeliveryVehicle(", "delivery"),
    ("Airstrike.Flyby(", "flyby"),
    ("GetGuidByName(", "guid"),
];

/// Category substrings (lowercase) — same spirit as the placement-category
/// vehicle matcher in tools/analyze_placement_categories.py.
const VEHICLE_TOKENS: [&str; 25] = [
    "veh_", "_veh", "truck", "jeep", "tank", "apc", "heli", "chopper", "boat",
    "ship", "humvee", "motorcycle", "buggy", "technical", "ambulance",
    "blackhawk", "huey", "apache", "gunship", "gunboat", "destroyer",
    "carrier", "dinghy", "hovercraft", "forklift",
];

/// Model-name tokens (lowercase) harvested from the resident-block
/// "Vehicle Entrance (Action Hijack) (X)" list and the support-store cargo set.
const MODEL_TOKENS: [&str; 60] = [
    "ah1z", "ah64", "alouette", "amx30", "anaconda", "armored bank", "bell47",
    "bmp", "bora", "brutus", "btr", "c130", "coanda", "cobra", "corrida",
    "duster", "ee11", "f35", "fav", "fiero", "gaz", "harrier", "havoc",
    "hind", "huang", "hummer", "ka29", "ka50", "kruk", "lav", "landstalker",
    "m1025", "m113", "m151", "m1a2", "m2a3", "m35", "m551", "md500", "mh53",
    "mi17", "mi24", "mi26", "mi35", "mirage", "montador", "mule", "neco",
    "nervoso", "patrol boat", "pavelow", "pgz95", "piranha", "plz45",
    "scorpion", "seahorse", "stingray", "uh1", "wz551", "ztz",
];

/// Spawned-but-not-a-vehicle noise (lowercase substrings / prefixes).
const EXCLUDE_TOKENS: [&str; 18] = [
    "particle", "explosion", "munitions", "supply drop", "soldier", "civilian",
    "sailor", "shell", "missile", "rocket", "smoke", "crate", "weapon",
    "destroy_", "_target", "vz_state", "wpn", "ordnance",
];
const EXCLUDE_PREFIXES: [&str; 7] = ["loc_", "pth_", "pa_", "path", "lnrg", "rgn_", "hp_"];

fn is_excluded(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with('_') // concatenation prefix, not a complete name
        || lower.starts_with('_') // prop/template naming convention
        || EXCLUDE_TOKENS.iter().any(|t| lower.contains(t))
        || EXCLUDE_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn is_vehicle(name: &str) -> bool {
    if is_excluded(name) {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    VEHICLE_TOKENS.iter().any(|t| lower.contains(t))
        || MODEL_TOKENS.iter().any(|t| lower.contains(t))
}

/// Strip a trailing per-instance suffix: ` 0x` + 6..=8 hex digits.
fn strip_instance_suffix(name: &str) -> (&str, bool) {
    if let Some(pos) = name.rfind(" 0x") {
        let hex = &name[pos + 3..];
        if (6..=8).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return (&name[..pos], true);
        }
    }
    (name, false)
}

/// Read the first string literal directly after a call-open paren (tolerates
/// leading whitespace). Returns None if the first argument isn't a literal.
fn quoted_arg(text: &str, after: usize) -> Option<&str> {
    let rest = &text[after..];
    let trimmed = rest.trim_start();
    let body = trimmed.strip_prefix('"')?;
    let end = body.find('"')?;
    Some(&body[..end])
}

fn walk_lua(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_lua(&path, files);
        } else if path.extension().is_some_and(|e| e == "lua") {
            files.push(path);
        }
    }
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[derive(Default)]
struct Entry {
    sources: BTreeSet<&'static str>,
    files: BTreeSet<String>,
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_OUT.to_string());

    let mut files = Vec::new();
    for corpus in CORPORA {
        walk_lua(Path::new(corpus), &mut files);
    }
    if files.is_empty() {
        eprintln!("no .lua files found under {CORPORA:?} — run from the repo root");
        std::process::exit(1);
    }

    let mut rows: BTreeMap<String, Entry> = BTreeMap::new();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else { continue };
        let rel = path.to_string_lossy().replace('\\', "/");
        for (needle, kind) in CALLS {
            let mut from = 0;
            while let Some(hit) = text[from..].find(needle) {
                let call_end = from + hit + needle.len();
                from = call_end;
                let Some(raw) = quoted_arg(&text, call_end) else { continue };
                let (name, placed) = strip_instance_suffix(raw);
                if name.is_empty() {
                    continue;
                }
                let label = match kind {
                    "guid" => {
                        if !is_vehicle(name) {
                            continue;
                        }
                        if placed { "placed" } else { "named" }
                    }
                    "spawn" if !is_vehicle(name) => continue,
                    // Cargo/delivery/flyby are vehicles by contract, but the
                    // support store also delivers troops and supply crates.
                    _ if is_excluded(name) => continue,
                    other => other,
                };
                let e = rows.entry(name.to_string()).or_default();
                e.sources.insert(label);
                if e.files.len() < 3 {
                    e.files.insert(rel.clone());
                }
            }
        }
    }

    let mut csv = String::from(
        "vehicle_name,pandemic_hash_m2_hex,pandemic_hash_m2_dec,name_kind,lua_call_sources,example_files\n",
    );
    for (name, e) in &rows {
        let h = pandemic_hash_m2(name);
        // A name seen only via non-suffixed GetGuidByName is a scene-specific
        // instance label; anything spawned/delivered/placed is a type name.
        let kind = if e.sources.iter().all(|s| *s == "named") {
            "instance_label"
        } else {
            "type"
        };
        let sources = e.sources.iter().copied().collect::<Vec<_>>().join(";");
        let files = e.files.iter().cloned().collect::<Vec<_>>().join(";");
        csv.push_str(&format!(
            "{},0x{h:08X},{h},{kind},{},{}\n",
            csv_field(name),
            csv_field(&sources),
            csv_field(&files),
        ));
    }

    if let Some(parent) = Path::new(&out_path).parent() {
        fs::create_dir_all(parent).expect("create output dir");
    }
    fs::write(&out_path, &csv).expect("write csv");
    println!("{} vehicle names -> {out_path}", rows.len());
}
