//! `save_inspect` — print structured facts about a `.profile`'s Lua AST.
//!
//! Not a trim or edit. Pure read-side inspection using the proper AST parser
//! (`mercs2_formats::save_lua`), so broken saves don't confuse it the way
//! loose text-grepping does.
//!
//! Usage: `save_inspect <profile> [profile2 …]`
//!   - for each file: top-level keys, size of key subtables, surviving Act-2
//!     flow keys, surviving Act-2 culled bindings, surviving Act-2 layers,
//!     active-mission ids.
//!   - with 2+ files: `diff` section listing which top-level keys differ in
//!     presence or child-count across the files (SHA-anchored).

use mercs2_formats::save::{self, parse};
use mercs2_formats::save_lua::{self, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: save_inspect <profile> [profile2 ...]");
        std::process::exit(2);
    }
    let mut reports: Vec<Report> = Vec::new();
    for path in &args {
        match inspect(path) {
            Ok(r) => reports.push(r),
            Err(e) => {
                eprintln!("inspect {path}: {e}");
                std::process::exit(1);
            }
        }
    }
    for r in &reports {
        print_report(r);
    }
    if reports.len() >= 2 {
        print_diff(&reports);
    }
}

struct Report {
    path: String,
    sha256: String,
    size: usize,
    header: save::Profile,
    top_level_keys: Vec<String>,
    section_sizes: BTreeMap<String, usize>,
    surviving_act2_flow: Vec<String>,
    surviving_act2_bindings: Vec<String>,
    surviving_act2_layers: Vec<String>,
    active_mission_ids: Vec<String>,
}

fn inspect(path: &str) -> Result<Report, String> {
    let bytes = fs::read(path).map_err(|e| format!("read: {e}"))?;
    let header = parse(&bytes)?;
    let lua = header.decompress_lua()?;
    let text = std::str::from_utf8(&lua).map_err(|e| format!("utf8: {e}"))?;
    let v = save_lua::parse_chunk(text).map_err(|e| format!("parse_chunk: {e}"))?;
    let root = v.as_table().map_err(|e| e.to_string())?;

    let top_level_keys: Vec<String> = root.iter_str().map(|(k, _)| k.to_string()).collect();
    let section_sizes: BTreeMap<String, usize> = root
        .iter_str()
        .map(|(k, v)| {
            let n = match v {
                Value::Table(t) => t.entries.len(),
                _ => 0,
            };
            (k.to_string(), n)
        })
        .collect();

    let flow_table = root.get("tFlowData").and_then(|v| v.as_table().ok());

    let surviving_act2_flow = flow_table
        .and_then(|t| t.get("tMyFlowData"))
        .and_then(|v| v.as_table().ok())
        .map(|t| {
            t.iter_str()
                .map(|(k, _)| k)
                .filter(|k| is_act2_key(k))
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let surviving_act2_bindings = flow_table
        .and_then(|t| t.get("tCulledBindings"))
        .and_then(|v| v.as_table().ok())
        .map(|t| {
            t.entries
                .iter()
                .filter_map(|e| match &e.value {
                    Value::String(s) if is_act2_binding(&s.value) => Some(s.value.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let surviving_act2_layers = root
        .get("tLayerData")
        .and_then(|v| v.as_table().ok())
        .map(|t| {
            t.entries
                .iter()
                .filter_map(|e| match &e.value {
                    Value::String(s) if is_act2_layer(&s.value) => Some(s.value.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let active_mission_ids: Vec<String> = flow_table
        .and_then(|t| t.get("tActiveMissions"))
        .and_then(|v| v.as_table().ok())
        .map(|t| t.iter_str().map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default();

    let mut sha = Sha256::new();
    sha.update(&bytes);
    let digest = sha.finalize();
    let mut sha_hex = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write;
        write!(&mut sha_hex, "{b:02x}").unwrap();
    }

    Ok(Report {
        path: path.to_string(),
        sha256: sha_hex,
        size: bytes.len(),
        header,
        top_level_keys,
        section_sizes,
        surviving_act2_flow,
        surviving_act2_bindings,
        surviving_act2_layers,
        active_mission_ids,
    })
}

fn print_report(r: &Report) {
    println!("=== {} ===", r.path);
    println!("size              : {} bytes", r.size);
    println!("sha256            : {}", r.sha256);
    println!(
        "header            : hero={} upgrade={} costumes={} cash={} fuel={}/{} contract={} saveName={}",
        r.header.character_index,
        r.header.upgrade_index,
        r.header.unlocked_costumes,
        r.header.cash,
        r.header.fuel,
        r.header.fuel_capacity,
        r.header.active_contract(),
        r.header.save_name()
    );
    println!(
        "top-level keys ({}): {:?}",
        r.top_level_keys.len(),
        r.top_level_keys
    );
    println!("section sizes:");
    for (k, n) in &r.section_sizes {
        println!("  {k:30} = {n}");
    }
    println!(
        "surviving Act-2 flow keys  ({}): {:?}",
        r.surviving_act2_flow.len(),
        r.surviving_act2_flow
    );
    println!(
        "surviving Act-2 culled      ({}): {:?}",
        r.surviving_act2_bindings.len(),
        r.surviving_act2_bindings
    );
    println!(
        "surviving Act-2 layers      ({}): {:?}",
        r.surviving_act2_layers.len(),
        r.surviving_act2_layers
    );
    println!(
        "active missions ({}): {:?}",
        r.active_mission_ids.len(),
        r.active_mission_ids
    );
    println!();
}

fn print_diff(reports: &[Report]) {
    println!("=== top-level key diff ===");
    let keyset: std::collections::BTreeSet<String> = reports
        .iter()
        .flat_map(|r| r.top_level_keys.iter().cloned())
        .collect();
    for k in &keyset {
        let presence: Vec<String> = reports
            .iter()
            .map(|r| {
                let n = r.section_sizes.get(k).copied();
                match n {
                    None => format!("{}=ABSENT", short(&r.path)),
                    Some(sz) => format!("{}={sz}", short(&r.path)),
                }
            })
            .collect();
        let differs = {
            let first = reports[0].section_sizes.get(k).copied();
            reports.iter().any(|r| r.section_sizes.get(k).copied() != first)
        };
        let marker = if differs { "!!" } else { "  " };
        println!("{marker} {k:30} {}", presence.join("  "));
    }
}

fn short(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_act2_key(key: &str) -> bool {
    key == "AllChiIntro"
        || key.starts_with("AllCon")
        || key.starts_with("AllJob")
        || key.starts_with("ChiCon")
        || key.starts_with("ChiJob")
}
fn is_act2_binding(name: &str) -> bool {
    is_act2_key(name) || name == "AllOutposts" || name == "AllCon050_ChiCon050"
}
fn is_act2_layer(layer: &str) -> bool {
    layer.contains("_act2chi") || layer.contains("_act2all")
}
