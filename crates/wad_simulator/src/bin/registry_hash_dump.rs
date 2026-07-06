//! Decode a live dump of the engine's global name-hash registry (the table
//! `Pg.Spawn` / `Pg.GetGuidByName` probe, descriptor at `0x00DF6B88`) into a
//! name-resolved CSV so players can spawn entities by hash.
//!
//! Inputs are raw x32dbg `savedata` dumps of the registry arrays taken while
//! the process is paused with a world loaded:
//!   reg_keys.bin      u32[capacity]  pandemic_hash_m2 of each entry's name
//!   reg_values.bin    u32[capacity]  per-entry payload (GUID/handle)
//!   reg_buckets.bin   u32[buckets]   bucket -> entry index, 0xFFFFFFFF = empty
//!   reg_aux_bd0.bin   u32[capacity]  descriptor +0x48 array (aux)
//!   reg_aux_bf4.bin   u32[capacity]  descriptor +0x6C array (aux)
//!
//! Usage:
//!   cargo run -p wad_simulator --bin registry_hash_dump -- \
//!       <dump_dir> <rainbow_table.json> <out.csv> [extra_names.txt]
//!
//! `extra_names.txt` (one candidate string per line) supplements the rainbow
//! table — e.g. `strings` dumps of the resident blocks, which carry the entity
//! template names the rainbow harvest misses.

use mercs2_formats::hash::pandemic_hash_m2;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct Rainbow {
    #[serde(rename = "pandemic_hash_m2")]
    m2: HashMap<String, Vec<String>>,
}

const VEHICLE_TOKENS: [&str; 25] = [
    "veh_", "_veh", "truck", "jeep", "tank", "apc", "heli", "chopper", "boat",
    "ship", "humvee", "motorcycle", "buggy", "technical", "ambulance",
    "blackhawk", "huey", "apache", "gunship", "gunboat", "destroyer",
    "carrier", "dinghy", "hovercraft", "forklift",
];
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

fn is_vehicle_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    VEHICLE_TOKENS.iter().any(|t| lower.contains(t))
        || MODEL_TOKENS.iter().any(|t| lower.contains(t))
}

fn read_u32s(path: &Path) -> Vec<u32> {
    let data = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    data.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: registry_hash_dump <dump_dir> <rainbow_table.json> <out.csv>");
        std::process::exit(2);
    }
    let dir = Path::new(&args[1]);
    let keys = read_u32s(&dir.join("reg_keys.bin"));
    let values = read_u32s(&dir.join("reg_values.bin"));
    let buckets = read_u32s(&dir.join("reg_buckets.bin"));
    let aux0 = read_u32s(&dir.join("reg_aux_bd0.bin"));
    let aux1 = read_u32s(&dir.join("reg_aux_bf4.bin"));

    // Ground truth for occupancy: every non-empty bucket names one entry index.
    let mut occupied = vec![false; keys.len()];
    let mut live = 0usize;
    for &b in &buckets {
        if b != u32::MAX {
            let i = b as usize;
            if i < keys.len() {
                if !occupied[i] {
                    live += 1;
                }
                occupied[i] = true;
            }
        }
    }
    eprintln!(
        "capacity {} entries, {} buckets, {} live entries",
        keys.len(),
        buckets.len(),
        live
    );

    // Sanity: a known template must be present (ZTZ98).
    let ztz98 = pandemic_hash_m2("ZTZ98");
    let found = keys
        .iter()
        .zip(&occupied)
        .any(|(&k, &o)| o && k == ztz98);
    eprintln!("ZTZ98 (0x{ztz98:08X}) present: {found}");

    let rainbow: Rainbow = serde_json::from_str(
        &fs::read_to_string(&args[2]).expect("read rainbow table"),
    )
    .expect("parse rainbow table");
    let mut names: HashMap<u32, &str> = HashMap::with_capacity(rainbow.m2.len());
    for (hex, cands) in &rainbow.m2 {
        if let (Ok(h), Some(first)) = (
            u32::from_str_radix(hex.trim_start_matches("0x"), 16),
            cands.first(),
        ) {
            names.insert(h, first.as_str());
        }
    }
    eprintln!("rainbow m2 entries: {}", names.len());

    let extra_text = args
        .get(4)
        .map(|p| fs::read_to_string(p).expect("read extra names"))
        .unwrap_or_default();
    let mut extra_added = 0usize;
    for line in extra_text.lines() {
        let name = line.trim();
        if name.len() < 2 {
            continue;
        }
        // Extra names win: they come from the game's own resident blocks.
        if names.insert(pandemic_hash_m2(name), name).is_none() {
            extra_added += 1;
        }
    }
    if !extra_text.is_empty() {
        eprintln!("extra names added: {extra_added}");
    }

    let mut rows: Vec<(usize, u32, Option<&str>)> = (0..keys.len())
        .filter(|&i| occupied[i])
        .map(|i| (i, keys[i], names.get(&keys[i]).copied()))
        .collect();
    rows.sort_by(|a, b| {
        a.2.unwrap_or("~").to_ascii_lowercase().cmp(&b.2.unwrap_or("~").to_ascii_lowercase())
    });

    let resolved = rows.iter().filter(|r| r.2.is_some()).count();
    let vehicles = rows
        .iter()
        .filter(|r| r.2.map(is_vehicle_name).unwrap_or(false))
        .count();
    eprintln!("resolved {resolved}/{} names, {vehicles} vehicle-classified", rows.len());

    let mut csv =
        String::from("name,pandemic_hash_m2_hex,pandemic_hash_m2_dec,is_vehicle,entry_index,value_hex,aux0_hex,aux1_hex\n");
    for (i, h, name) in &rows {
        let n = name.unwrap_or("");
        csv.push_str(&format!(
            "{},0x{h:08X},{h},{},{i},0x{:08X},0x{:08X},0x{:08X}\n",
            csv_field(n),
            if n.is_empty() { "" } else if is_vehicle_name(n) { "1" } else { "0" },
            values.get(*i).copied().unwrap_or(0),
            aux0.get(*i).copied().unwrap_or(0),
            aux1.get(*i).copied().unwrap_or(0),
        ));
    }
    fs::write(&args[3], &csv).expect("write csv");
    println!(
        "{} entries ({resolved} named, {vehicles} vehicles) -> {}",
        rows.len(),
        &args[3]
    );
}
