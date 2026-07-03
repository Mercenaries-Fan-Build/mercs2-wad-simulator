//! `mercs2_probe` — headless WAD diagnostic / export tool.
//!
//! Each subcommand runs one of the diagnostics carved out of the engine binary into
//! `mercs2_engine::diag`. `vz.wad` is auto-discovered from the EA Games registry key (same as the
//! engine), or passed explicitly with `--wad <path>`. No window is ever opened.
//!
//! Examples:
//!   mercs2_probe c3-meta out.ndjson
//!   mercs2_probe placement-hashes out.json
//!   mercs2_probe export-c3-obj [outdir]
//!   mercs2_probe terrainmesh-probe [block]
//!   mercs2_probe wad-meshes --model 0x1234ABCD
//!   mercs2_probe trackmap --model 0x.. --index 0 --clip 0x..
//!   mercs2_probe entity-find 0x000d3c77 0x..
//!   mercs2_probe scan-hash 0xA3CD72A7 0x..
//!   mercs2_probe block-probe 1234
//!   mercs2_probe --wad D:/game/data/vz.wad comp-probe

use mercs2_engine::diag;
use mercs2_engine::wad;
use mercs2_engine::worldutil::parse_hash;

fn usage() -> ! {
    eprintln!(
        "usage: mercs2_probe <subcommand> [args] [--wad <path>]\n\
         subcommands:\n\
         \x20 c3-meta <out.ndjson>          placement-hashes <out.json>   export-c3-obj [outdir]\n\
         \x20 terrainmesh-probe [block]     terrain-probe                 terrain-consumer\n\
         \x20 wad-list                      wad-meshes [--model H]        placement-probe\n\
         \x20 world-index                   stream-probe                  lod is engine-only\n\
         \x20 animdiag/animcheck/skincheck [--model H] [--index I]        trackmap [... --clip H]\n\
         \x20 entity-find [0xKEY ...]       comp-probe                    comp-dump [Name]\n\
         \x20 block-grep <needle>           scan-hash <0xH ...>           find-ref <0xH ...>\n\
         \x20 block-probe <index>           placement-names\n\
         (vz.wad auto-discovers from the registry; override with --wad <path>)"
    );
    std::process::exit(2);
}

/// Value following a named flag, e.g. `--model 0x..` -> Some("0x..").
fn flag_val(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .filter(|v| !v.starts_with("--"))
}

/// All `0x..`-prefixed hex u32 arguments (for scan-hash / find-ref / entity-find).
fn hex_args(args: &[String]) -> Vec<u32> {
    args.iter()
        .filter_map(|a| a.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()))
        .collect()
}

/// First positional (non-flag, non-flag-value) argument after the subcommand.
fn first_positional(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for a in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a == "--wad" || a == "--model" || a == "--index" || a == "--clip" {
            skip_next = true;
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        return Some(a.clone());
    }
    None
}

/// Headless verification of vz_state overlay activation: parse a `.profile` → its active `SaveState`
/// layers → resolve each to a WAD block → run the REAL streaming fold (`add_overlay_to_catalog`) and
/// report how many resolve + how many entities each adds. Proves the boot-state overlay path without
/// a window (the engine's streaming render uses the same `worldutil` fold).
fn overlays_report(wadpath: &str, profile_path: &str) -> Result<(), String> {
    use mercs2_core::streaming::{StreamingConfig, StreamingManager};
    use mercs2_engine::worldutil::{add_overlay_to_catalog, resolve_overlay_block, PropSpawn};
    use std::collections::HashMap;

    let bytes = std::fs::read(profile_path).map_err(|e| format!("read {profile_path}: {e}"))?;
    let profile = mercs2_formats::save::parse(&bytes)?;
    let state = profile.save_state()?;
    let layers = &state.layers;
    println!(
        "[overlays] profile '{}' — contract {}, {} active vz_state layers",
        profile.save_name(),
        profile.active_contract(),
        layers.len()
    );

    let mut w = wad::open(wadpath)?;
    let cfg = StreamingConfig::default();
    let mut mgr = StreamingManager::new(cfg);
    let mut props: HashMap<u32, PropSpawn> = HashMap::new();

    let (mut resolved, mut tot_mn, mut tot_named) = (0usize, 0usize, 0usize);
    let mut unresolved: Vec<&str> = Vec::new();
    let mut per: Vec<(String, usize)> = Vec::new();
    for l in layers {
        match resolve_overlay_block(&w, l) {
            Some(bi) => {
                let dec = wad::decompress_block_index(&mut w, bi).map_err(|e| format!("block {bi}: {e}"))?;
                let (mn, nm) = add_overlay_to_catalog(&dec, cfg.default_distances, &mut mgr, &mut props);
                resolved += 1;
                tot_mn += mn;
                tot_named += nm;
                if mn + nm > 0 {
                    per.push((l.clone(), mn + nm));
                }
            }
            None => unresolved.push(l),
        }
    }
    println!("[overlays] resolved {resolved}/{} layers to blocks", layers.len());
    println!(
        "[overlays] entities added: {tot_mn} ModelName + {tot_named} named = {} ({} distinct streamable keys)",
        tot_mn + tot_named,
        props.len()
    );
    per.sort_by(|a, b| b.1.cmp(&a.1));
    println!("[overlays] top overlays by entity count:");
    for (n, c) in per.iter().take(10) {
        println!("    {c:>5}  {n}");
    }
    if !unresolved.is_empty() {
        let sample: Vec<&str> = unresolved.iter().take(6).copied().collect();
        println!("[overlays] {} unresolved (sample): {sample:?}", unresolved.len());
    }
    Ok(())
}

fn main() {
    let all: Vec<String> = std::env::args().collect();
    if all.len() < 2 {
        usage();
    }
    let cmd = all[1].clone();
    let args: Vec<String> = all[2..].to_vec();

    // Resolve vz.wad: explicit --wad, else the EA Games registry key.
    let wadpath = match flag_val(&args, "--wad").filter(|v| !v.is_empty()).or_else(wad::registry_vz_wad) {
        Some(p) => p,
        None => {
            eprintln!(
                "no vz.wad found — pass --wad <path>, or install so that\n  \
                 HKLM\\SOFTWARE\\WOW6432Node\\EA Games\\Mercenaries 2 World in Flames\\Install Dir\n  \
                 resolves to a folder containing data\\vz.wad"
            );
            std::process::exit(1);
        }
    };
    eprintln!("vz.wad: {wadpath}");

    let model = flag_val(&args, "--model");
    let index = flag_val(&args, "--index");
    let clip = flag_val(&args, "--clip").and_then(|c| parse_hash(&c));

    // Run a diagnostic that returns Result and exit(1) on error.
    let run = |r: Result<(), String>| {
        if let Err(e) = r {
            eprintln!("{cmd} failed: {e}");
            std::process::exit(1);
        }
    };

    match cmd.as_str() {
        "c3-meta" => {
            let out = first_positional(&args).unwrap_or_else(|| "c3_meta.ndjson".to_string());
            run(diag::c3_meta(&wadpath, &out));
        }
        "placement-hashes" => {
            let out = first_positional(&args).unwrap_or_else(|| "placement_hashes.json".to_string());
            run(diag::placement_hashes(&wadpath, &out));
        }
        "export-c3-obj" => {
            let out = first_positional(&args)
                .unwrap_or_else(|| "c:/Users/Shadow/Desktop/notes-on-the-released-game/output/review".into());
            run(diag::export_c3_obj(&wadpath, &out));
        }
        "terrainmesh-probe" => {
            let block = first_positional(&args).and_then(|s| s.parse::<u16>().ok());
            run(diag::terrainmesh_probe(&wadpath, block));
        }
        "terrain-probe" => run(diag::terrain_probe(&wadpath)),
        "terrain-consumer" => run(diag::terrain_consumer_scan(&wadpath)),
        "wad-list" => run(diag::wad_list(&wadpath)),
        "wad-meshes" => run(diag::wad_meshes(&wadpath, model)),
        "placement-probe" => run(diag::placement_probe(&wadpath)),
        "world-index" => run(diag::world_index_probe(&wadpath)),
        "stream-probe" => run(diag::stream_probe(&wadpath)),
        "animdiag" => run(diag::animdiag(&wadpath, model, index)),
        "animcheck" => run(diag::animcheck(&wadpath, model, index)),
        "skincheck" => run(diag::skincheck(&wadpath, model, index)),
        "trackmap" => run(diag::trackmap(&wadpath, model, index, clip)),
        "entity-find" => run(diag::entity_find(&wadpath, &hex_args(&args))),
        "comp-probe" => run(diag::comp_probe(&wadpath)),
        "comp-dump" => {
            let name = first_positional(&args).unwrap_or_else(|| "HibernationControl".into());
            run(diag::comp_dump(&wadpath, &name));
        }
        "block-grep" => diag::block_grep(&wadpath, &first_positional(&args).unwrap_or_default()),
        "scan-hash" => diag::scan_hash(&wadpath, &hex_args(&args)),
        "find-ref" => diag::find_ref(&wadpath, &hex_args(&args)),
        "block-probe" => match first_positional(&args).and_then(|s| s.parse::<u16>().ok()) {
            Some(bi) => diag::block_probe(&wadpath, bi),
            None => {
                eprintln!("block-probe: usage: mercs2_probe block-probe <block-index>");
                std::process::exit(2);
            }
        },
        "placement-names" => diag::placement_names(&wadpath),
        "overlays" => {
            let prof = first_positional(&args).unwrap_or_else(|| {
                eprintln!("overlays: usage: mercs2_probe overlays <profile-path>");
                std::process::exit(2);
            });
            run(overlays_report(&wadpath, &prof));
        }
        "-h" | "--help" | "help" => usage(),
        other => {
            eprintln!("unknown subcommand '{other}'");
            usage();
        }
    }
}
