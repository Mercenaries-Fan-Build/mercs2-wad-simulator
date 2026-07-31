//! How COARSE is a character's skinning, measured the same way for shipped and injected models?
//!
//! An imported character renders correctly at rest and tears apart under animation. The skinning
//! was proven faithful (every influence decodes to the bone char_skin intended), so the remaining
//! hypothesis is that it is faithful but too COARSE: too few bones, too many vertices bound rigidly
//! to exactly one of them. Rigid chunks meeting at a joint look perfect in bind pose and visibly
//! split the moment that joint bends.
//!
//! That is a claim about a DISTRIBUTION, so it needs a shipped baseline rather than intuition —
//! "27 bones sounds low" is not evidence. This reports, per drawing group and overall:
//!   * distinct bones actually referenced by non-zero-weight influences
//!   * the share of vertices with 2+, 3+, 4 influences (rigid = exactly 1)
//!
//! Reads the container through `model_cubeize::read_model_meshes`, which expands each group's
//! INFO(56) palette to GLOBAL bone indices exactly as the engine does — so shipped and injected
//! models are measured on the same axis.
//!
//!   skin_census [--group N] <container-or-block.bin> [more.bin ...]
//!
//! # Per-group palette census (`--per-group`, `--wad`)
//!
//! The influence distribution above answers "how coarse". A second question needs the same
//! machinery: **what palette size does retail actually ship?** `char_skin::build::PALETTE_CAP` was
//! set from a single `--group 3` reading of one model, and its own doc comment records that it was
//! measured as a BONE count while every call site compares it against SLOTS — which the packer's
//! run-length merge inflates by bridging gaps. One measurement of one group cannot settle a limit
//! that rejects author content.
//!
//! `--per-group` emits CSV of the values the container itself declares — `range_count` and the
//! slots its runs expand to, read from the shipped `INFO(56)` table rather than reconstructed —
//! alongside the distinct bones actually weighted. `--wad` walks every model asset in the game
//! instead of taking file paths, so the answer comes from the whole corpus:
//!
//!   skin_census --wad --per-group --filter _hum_ > palette_census.csv

use mercs2_formats::model_cubeize::read_model_meshes;
use std::collections::BTreeSet;

/// `hash → name` from the workspace's curated `data/production_names.json`, for labelling census
/// rows and for the character filter. Absent table = hashes, which is degraded but not wrong.
fn load_names(explicit: Option<&str>) -> std::collections::HashMap<u32, String> {
    let mut out = std::collections::HashMap::new();
    let path = match explicit {
        Some(p) => Some(std::path::PathBuf::from(p)),
        None => {
            let mut dir: Option<&std::path::Path> = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
            let mut found = None;
            while let Some(d) = dir {
                let c = d.join("data/production_names.json");
                if c.is_file() {
                    found = Some(c);
                    break;
                }
                dir = d.parent();
            }
            found
        }
    };
    let Some(path) = path else {
        eprintln!("note: no data/production_names.json found — rows will be labelled by hash");
        return out;
    };
    let Ok(text) = std::fs::read_to_string(&path) else { return out };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return out };
    if let Some(map) = v.get("pandemic_hash_m2").and_then(|m| m.as_object()) {
        for (k, name) in map {
            let Some(h) = k.strip_prefix("0x").and_then(|s| u32::from_str_radix(s, 16).ok()) else {
                continue;
            };
            if let Some(n) = name.as_str() {
                out.insert(h, n.to_string());
            }
        }
    }
    out
}

/// Accept either a bare UCFX container or a wrapped block (20-byte header + UCFX).
fn unwrap_container(raw: &[u8]) -> &[u8] {
    if raw.len() > 4 && &raw[0..4] == b"UCFX" {
        raw
    } else if raw.len() > 20 {
        let n = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as usize;
        if 20 + n <= raw.len() && &raw[20..24] == b"UCFX" {
            &raw[20..20 + n]
        } else {
            raw
        }
    } else {
        raw
    }
}

/// One CSV row per SKINNED drawing group. Rigid groups carry no palette and are skipped — counting
/// them would dilute the maxima this exists to find.
fn per_group_csv(label: &str, container: &[u8]) {
    let meshes = match read_model_meshes(container) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{label}: read_model_meshes: {e}");
            return;
        }
    };
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() || m.rigid {
            continue;
        }
        println!(
            "{label},{},{},{},{},{},{},0x{:02X}",
            m.group_index,
            m.range_count,
            m.palette_slots,
            m.distinct_bones,
            m.positions.len(),
            m.tris.len(),
            m.state_mask
        );
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // --- census flags, drained before the positional file list ---
    let take_flag = |args: &mut Vec<String>, name: &str| -> bool {
        if let Some(i) = args.iter().position(|a| a == name) {
            args.remove(i);
            true
        } else {
            false
        }
    };
    let take_val = |args: &mut Vec<String>, name: &str| -> Option<String> {
        if let Some(i) = args.iter().position(|a| a == name) {
            let v = args.get(i + 1).cloned();
            args.drain(i..=(i + 1).min(args.len() - 1));
            v
        } else {
            None
        }
    };
    let per_group = take_flag(&mut args, "--per-group");
    let from_wad = take_flag(&mut args, "--wad");
    let names_path = take_val(&mut args, "--names");
    let filter = take_val(&mut args, "--filter");
    let wad_path = take_val(&mut args, "--wad-path");

    if from_wad {
        let names = load_names(names_path.as_deref());
        let path = match wad_path.or_else(|| mercs2_engine::wad::resolve_vz_wad(None)) {
            Some(p) => p,
            None => {
                eprintln!("no vz.wad — pass --wad-path, or set MERCS2_GAME_DIR/VZ_WAD");
                std::process::exit(2);
            }
        };
        let mut w = match mercs2_engine::wad::open(&path) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("open {path}: {e}");
                std::process::exit(2);
            }
        };
        let hashes: Vec<u32> = mercs2_engine::wad::model_list_all(&w)
            .into_iter()
            .map(|(h, _)| h)
            .collect();
        // A name filter, not a name REQUIREMENT: an unnamed model still gets censused unless a
        // filter was asked for. Silently dropping the 21 bones no corpus can name would bias the
        // maxima downward, which is the one direction that matters here.
        let keep = |h: u32| -> Option<String> {
            let name = names.get(&h).cloned();
            match (&filter, &name) {
                (Some(f), Some(n)) => n.contains(f.as_str()).then(|| n.clone()),
                (Some(_), None) => None,
                (None, Some(n)) => Some(n.clone()),
                (None, None) => Some(format!("0x{h:08X}")),
            }
        };
        if per_group {
            println!("model,group,range_count,slots,bones,verts,tris,state_mask");
        }
        let (mut scanned, mut skipped) = (0usize, 0usize);
        for h in hashes {
            let Some(label) = keep(h) else { continue };
            let Ok(c) = mercs2_engine::wad::extract_container(&mut w, h) else {
                skipped += 1;
                continue;
            };
            scanned += 1;
            if per_group {
                per_group_csv(&label, unwrap_container(&c));
            } else {
                census_one(&label, unwrap_container(&c), None);
            }
        }
        eprintln!("censused {scanned} model assets ({skipped} unreadable)");
        return;
    }

    // `--group N` restricts to one drawing group. Needed to compare like with like: an injected
    // block still carries the donor's OTHER groups (their draw counts are zeroed, but the geometry
    // and its skinning are still in the container), so a whole-container census silently averages
    // our injected mesh together with ~18k vertices of shipped Mattias.
    let mut only_group: Option<usize> = None;
    if let Some(i) = args.iter().position(|a| a == "--group") {
        only_group = args.get(i + 1).and_then(|s| s.parse().ok());
        args.drain(i..=i + 1);
    }
    if args.is_empty() {
        eprintln!(
            "usage: skin_census [--group N] <container-or-block.bin> [...]\n\
             \x20      skin_census --wad [--wad-path <vz.wad>] [--filter <substr>] [--per-group]"
        );
        std::process::exit(2);
    }
    if per_group {
        println!("model,group,range_count,slots,bones,verts,tris,state_mask");
        for path in &args {
            let Ok(raw) = std::fs::read(path) else {
                eprintln!("{path}: unreadable");
                continue;
            };
            per_group_csv(&short(path), unwrap_container(&raw));
        }
        return;
    }
    println!(
        "{:<34} {:>6} {:>7} {:>6} {:>7} {:>7} {:>7}",
        "model", "bones", "verts", "grps", "rigid%", "2+%", "4-inf%"
    );
    println!("{}", "-".repeat(80));
    for path in &args {
        let raw = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => { println!("{path}: {e}"); continue; }
        };
        census_one(&short(path), unwrap_container(&raw), only_group);
    }
    println!("\nrigid% = vertices bound to exactly ONE bone. High rigid% with few bones is the shape\nthat survives bind pose and tears when a joint bends.");
}

/// The influence-distribution census for one container, printed as a table row.
fn census_one(label: &str, container: &[u8], only_group: Option<usize>) {
    {
        let meshes = match read_model_meshes(container) {
            Ok(m) => m,
            Err(e) => { println!("{label}: read_model_meshes: {e}"); return; }
        };

        let mut bones: BTreeSet<u16> = BTreeSet::new();
        let (mut verts, mut rigid, mut multi, mut four, mut skinned_groups) = (0usize, 0usize, 0usize, 0usize, 0usize);
        for m in &meshes {
            if let Some(g) = only_group { if m.group_index != g { continue; } }
            if m.joints.is_empty() || m.weights.is_empty() { continue; }
            skinned_groups += 1;
            for (j, w) in m.joints.iter().zip(m.weights.iter()) {
                verts += 1;
                let mut n = 0;
                for k in 0..4 {
                    if w[k] > 0 {
                        n += 1;
                        bones.insert(j[k] as u16);
                    }
                }
                match n {
                    0 | 1 => rigid += 1,
                    _ => {
                        multi += 1;
                        if n == 4 { four += 1; }
                    }
                }
            }
        }
        if verts == 0 {
            println!("{:<34} {:>6} {:>7}  (no skinned groups)", label, 0, 0);
            return;
        }
        // What would OUR palette packer need for this exact bone set? If retail's own groups
        // require more slots than our cap allows, the cap is comparing the wrong quantities:
        // PALETTE_CAP was measured as retail's BONE count, but the check gates on SLOTS, and the
        // RLE merge down to 8 runs bridges gaps so slots > bones.
        let mut bl: Vec<u32> = bones.iter().map(|&b| b as u32).collect();
        bl.sort_unstable();
        let (rr, _, slots) = mercs2_formats::char_skin::build::build_palette_ranges(&bl);
        let pct = |a: usize| 100.0 * a as f64 / verts as f64;
        println!(
            "{:<34} {:>6} {:>7} {:>6} {:>6.1}% {:>6.1}% {:>6.1}%",
            label, bones.len(), verts, skinned_groups, pct(rigid), pct(multi), pct(four)
        );
        println!("        -> our packer would need {slots} slots over {} runs for those {} bones",
            rr.len(), bones.len());
    }
}

fn short(p: &str) -> String {
    let b = p.rsplit(['/', '\\']).next().unwrap_or(p);
    b.chars().take(34).collect()
}
