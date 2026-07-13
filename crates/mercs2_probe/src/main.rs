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
         \x20 block-probe <index>           placement-names               hier --model H [names.txt]\n\
         \x20 gfx-extract [outdir]          (Scaleform movies -> output/gfx_movies)\n\
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
        if a == "--wad" || a == "--model" || a == "--index" || a == "--clip" || a == "--csv" {
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

/// Dump a save `.profile`: all parsed header fields (including the raw bytes around the
/// character/costume region, 0x240..0x260) + the DECOMPRESSED SaveSingleton Lua payload — the
/// grep-able ground truth for header-byte meanings. `needle` filters payload lines
/// (case-insensitive); `None` prints the whole payload.
fn save_dump(profile_path: &str, needle: Option<&str>) -> Result<(), String> {
    let bytes = std::fs::read(profile_path).map_err(|e| format!("read {profile_path}: {e}"))?;
    let p = mercs2_formats::save::parse(&bytes)?;
    println!(
        "[save-dump] '{}' | contract {} | playtime {}s | cash {} | fuel {}/{} | character(0x4D) {} | upgrade(0x4F) {} | unlocked_costumes(0x24A) {} | unk(0x24B) {} | flags4C {:#x} | ts {}",
        p.save_name(),
        p.active_contract(),
        p.play_time_seconds,
        p.cash,
        p.fuel,
        p.fuel_capacity,
        p.character_index,
        p.upgrade_index,
        p.unlocked_costumes,
        p.unknown_0x24b,
        p.flags_0x4c,
        p.timestamp
    );
    let hex: Vec<String> = bytes[0x240..0x260.min(bytes.len())].iter().map(|b| format!("{b:02x}")).collect();
    println!("[save-dump] header[0x240..0x260]: {}", hex.join(" "));
    let lua = p.decompress_lua()?;
    let text = String::from_utf8_lossy(&lua);
    match needle {
        Some(n) => {
            for line in text.lines().filter(|l| l.to_lowercase().contains(n)) {
                println!("{line}");
            }
        }
        None => println!("{text}"),
    }
    Ok(())
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

/// `hier` — dump a model container's HIER bone tree with name resolution.
/// Names come from the rainbow table plus an optional plain-text candidate
/// file (one string per line, hashed with pandemic_hash_m2). With `--csv <path>`
/// the tree is also written as CSV (DFS order, parent_index column).
fn hier_report(
    wadpath: &str,
    model: u32,
    names_file: Option<String>,
    csv_out: Option<String>,
) -> Result<(), String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_formats::orchestrator;
    use std::collections::{BTreeSet, HashMap};

    let mut w = wad::open(wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;
    let c = wad::extract_container(&mut w, model).map_err(|e| format!("extract_container: {e}"))?;
    let hier = orchestrator::parse_hier(&c);
    let swit: BTreeSet<u32> = orchestrator::parse_swit(&c).into_iter().collect();
    let dest = orchestrator::classify(&c);

    let all_hashes: BTreeSet<u32> = hier.iter().map(|n| n.hash).collect();
    let mut names = mercs2_engine::worldutil::rainbow_names(&all_hashes);
    if let Some(p) = names_file {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("read {p}: {e}"))?;
        for line in text.lines() {
            let cand = line.trim();
            if cand.len() < 2 {
                continue;
            }
            let h = pandemic_hash_m2(cand);
            if all_hashes.contains(&h) {
                names.entry(h).or_insert_with(|| cand.to_string());
            }
        }
    }

    println!(
        "model 0x{model:08X}: {} HIER nodes, {} in SWIT, {} resolved names",
        hier.len(),
        hier.iter().filter(|n| swit.contains(&n.hash)).count(),
        hier.iter().filter(|n| names.contains_key(&n.hash)).count()
    );

    // Depth-first print. parent==None are roots.
    let mut kids: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for n in &hier {
        match n.parent {
            Some(p) => kids.entry(p).or_default().push(n.index),
            None => roots.push(n.index),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn walk(
        i: usize,
        depth: usize,
        hier: &[orchestrator::HierNode],
        kids: &HashMap<usize, Vec<usize>>,
        names: &HashMap<u32, String>,
        swit: &BTreeSet<u32>,
        dest: &Option<orchestrator::Destruction>,
        csv: &mut Option<String>,
    ) {
        let n = &hier[i];
        let name = names.get(&n.hash).map(|s| s.as_str()).unwrap_or("?");
        let state = dest
            .as_ref()
            .and_then(|d| d.state_of_node(i))
            .map(|s| s.as_str())
            .unwrap_or("-");
        let t = [n.local[12], n.local[13], n.local[14]];
        let dim = [
            n.bbox_max[0] - n.bbox_min[0],
            n.bbox_max[1] - n.bbox_min[1],
            n.bbox_max[2] - n.bbox_min[2],
        ];
        println!(
            "{:indent$}[{i:>3}] 0x{:08X} {name:<44} {}{state:<12} t=({:.2},{:.2},{:.2}) bbox {:.1}x{:.1}x{:.1}",
            "",
            n.hash,
            if swit.contains(&n.hash) { "SWIT " } else { "" },
            t[0],
            t[1],
            t[2],
            dim[0],
            dim[1],
            dim[2],
            indent = depth * 2
        );
        if let Some(out) = csv {
            let parent = n.parent.map(|p| p.to_string()).unwrap_or_default();
            out.push_str(&format!(
                "{i},{parent},{depth},0x{:08X},{},{},{state},{:.4},{:.4},{:.4},{:.3},{:.3},{:.3}\n",
                n.hash,
                if name == "?" { "" } else { name },
                u8::from(swit.contains(&n.hash)),
                t[0], t[1], t[2],
                dim[0], dim[1], dim[2],
            ));
        }
        for &k in kids.get(&i).map(|v| v.as_slice()).unwrap_or(&[]) {
            walk(k, depth + 1, hier, kids, names, swit, dest, csv);
        }
    }
    let mut csv = csv_out.as_ref().map(|_| {
        String::from("index,parent_index,depth,hash,name,in_swit,state,tx,ty,tz,bbox_dx,bbox_dy,bbox_dz\n")
    });
    for &r in &roots {
        walk(r, 0, &hier, &kids, &names, &swit, &dest, &mut csv);
    }
    if let (Some(path), Some(out)) = (&csv_out, &csv) {
        std::fs::write(path, out).map_err(|e| format!("write {path}: {e}"))?;
        println!("csv -> {path}");
    }
    if let Some(d) = &dest {
        println!(
            "destruction: {} switch groups, {} hulls, INDX {} mesh->node entries",
            d.switch_group_count,
            d.hull_count,
            d.indx.len()
        );
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
        "gfx-extract" => {
            let out = first_positional(&args).unwrap_or_else(|| {
                "c:/Users/Shadow/Desktop/notes-on-the-released-game/output/gfx_movies".into()
            });
            run(diag::gfx_extract(&wadpath, &out));
        }
        "extract" => {
            let mh = model.as_deref().and_then(parse_hash).unwrap_or_else(|| {
                eprintln!("extract: usage: mercs2_probe extract --model 0xHASH <out.bin>");
                std::process::exit(2);
            });
            let out = first_positional(&args).unwrap_or_else(|| format!("{mh:08X}.bin"));
            run((|| -> Result<(), String> {
                let mut w = wad::open(&wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;
                let c = wad::extract_container(&mut w, mh).map_err(|e| format!("extract_container: {e}"))?;
                std::fs::write(&out, &c).map_err(|e| format!("write {out}: {e}"))?;
                println!("0x{mh:08X}: {} bytes -> {out}", c.len());
                Ok(())
            })());
        }
        "dump-block" => {
            let bi = first_positional(&args).and_then(|s| s.parse::<u16>().ok()).unwrap_or_else(|| {
                eprintln!("dump-block: usage: mercs2_probe dump-block <block-index> <out.bin>");
                std::process::exit(2);
            });
            let out = args.iter().rev().find(|a| a.ends_with(".bin")).cloned().unwrap_or_else(|| format!("block_{bi}.bin"));
            run((|| -> Result<(), String> {
                let mut w = wad::open(&wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;
                let dec = wad::decompress_block_index(&mut w, bi).map_err(|e| format!("block {bi}: {e}"))?;
                std::fs::write(&out, &dec).map_err(|e| format!("write {out}: {e}"))?;
                println!("block {bi}: {} bytes decompressed -> {out}", dec.len());
                Ok(())
            })());
        }
        "find-placement" => {
            let query = first_positional(&args).unwrap_or_default().to_lowercase();
            run((|| -> Result<(), String> {
                if query.is_empty() {
                    return Err("usage: mercs2_probe find-placement <name-substring>".into());
                }
                let mut w = wad::open(&wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;
                let (mut scanned, mut hits, mut miss) = (0usize, 0usize, 0u32);
                for bi in 0u16..6000 {
                    match wad::decompress_block_index(&mut w, bi) {
                        Ok(dec) => {
                            miss = 0;
                            scanned += 1;
                            if let Ok(ps) = mercs2_formats::placement::load_placements(&dec) {
                                for p in ps {
                                    if let Some(n) = &p.name {
                                        if n.to_lowercase().contains(&query) {
                                            println!(
                                                "block {bi:>5}  key=0x{:08X}  pos=[{:9.2},{:9.2},{:9.2}]  '{}'",
                                                p.key, p.pos[0], p.pos[1], p.pos[2], n
                                            );
                                            hits += 1;
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            miss += 1;
                            if miss > 96 && scanned > 0 {
                                break;
                            }
                        }
                    }
                }
                eprintln!("[find-placement] scanned {scanned} blocks, {hits} match '{query}'");
                Ok(())
            })());
        }
        "hier" => {
            let mh = model.as_deref().and_then(parse_hash).unwrap_or_else(|| {
                eprintln!("hier: usage: mercs2_probe hier --model 0xHASH [names.txt]");
                std::process::exit(2);
            });
            run(hier_report(&wadpath, mh, first_positional(&args), flag_val(&args, "--csv")));
        }
        "overlays" => {
            let prof = first_positional(&args).unwrap_or_else(|| {
                eprintln!("overlays: usage: mercs2_probe overlays <profile-path>");
                std::process::exit(2);
            });
            run(overlays_report(&wadpath, &prof));
        }
        "save-dump" => {
            let prof = first_positional(&args).unwrap_or_else(|| {
                eprintln!("save-dump: usage: mercs2_probe save-dump <profile-path> [grep-needle]");
                std::process::exit(2);
            });
            // Second positional = case-insensitive needle to filter the Lua payload lines.
            let needle = {
                let mut seen = false;
                let mut n = None;
                let mut skip = false;
                for a in &args {
                    if skip { skip = false; continue; }
                    if a == "--wad" { skip = true; continue; }
                    if a.starts_with("--") { continue; }
                    if !seen { seen = true; continue; } // the profile path
                    n = Some(a.to_lowercase());
                    break;
                }
                n
            };
            run(save_dump(&prof, needle.as_deref()));
        }
        "-h" | "--help" | "help" => usage(),
        other => {
            eprintln!("unknown subcommand '{other}'");
            usage();
        }
    }
}
