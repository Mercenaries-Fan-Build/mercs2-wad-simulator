//! place_forge — append a NEW SceneObject placement to a decompressed placement
//! block and round-trip-verify it via the reader.
//!
//! Usage:
//!   place_forge <in_block.bin> <out_block.bin> --template <sub_idx> \
//!       --name <entity_name> --model 0x<hash> --pos X,Y,Z [--quat X,Y,Z,W]

use mercs2_formats::placement::load_model_placements;
use mercs2_formats::placement_build::append_placement;

fn parse_vec3(s: &str) -> [f32; 3] {
    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)]
}

fn main() {
    std::process::exit(run());
}

/// `place_forge --list <block.bin> [near X,Z] [radius R]` — dump existing model
/// placements (key, model hash, pos, name), optionally only those within R of an
/// XZ point. Used to pick a known-good base-game model hash + a valid exterior
/// spot for an isolation test, without inventing anything.
fn list_mode(argv: &[String]) -> i32 {
    let mut path = String::new();
    let mut near: Option<[f32; 2]> = None;
    let mut radius = f32::INFINITY;
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--list" => {}
            "--near" => {
                let v: Vec<f32> = it.next().cloned().unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if v.len() == 2 { near = Some([v[0], v[1]]); }
            }
            "--radius" => radius = it.next().and_then(|s| s.parse().ok()).unwrap_or(f32::INFINITY),
            s if !s.starts_with("--") => path = s.to_string(),
            _ => {}
        }
    }
    let block = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {path}: {e}"); return 1; }
    };
    let ps = load_model_placements(&block);
    let mut shown = 0;
    for p in &ps {
        if let Some([nx, nz]) = near {
            let dx = p.pos[0] - nx;
            let dz = p.pos[2] - nz;
            if (dx * dx + dz * dz).sqrt() > radius { continue; }
        }
        println!("key=0x{:08X} model=0x{:08X} pos=({:.2}, {:.2}, {:.2}) name={:?}",
            p.key, p.model_hash, p.pos[0], p.pos[1], p.pos[2], p.name);
        shown += 1;
        if shown >= 2000 { println!("... ({} total)", ps.len()); break; }
    }
    if shown == 0 { println!("(no model placements matched; {} total in block)", ps.len()); }
    0
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--list") {
        return list_mode(&argv);
    }
    let mut args = std::env::args().skip(1);
    let mut pos_args: Vec<String> = Vec::new();
    let (mut template, mut name, mut model, mut pos, mut quat) =
        (15usize, String::new(), 0u32, [0.0f32; 3], [0.0f32, 0.0, 0.0, 1.0]);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--template" => template = args.next().and_then(|s| s.parse().ok()).unwrap_or(15),
            "--name" => name = args.next().unwrap_or_default(),
            "--model" => {
                model = args
                    .next()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .unwrap_or(0)
            }
            "--pos" => pos = parse_vec3(&args.next().unwrap_or_default()),
            "--quat" => {
                let v: Vec<f32> = args.next().unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() == 4 {
                    quat = [v[0], v[1], v[2], v[3]];
                }
            }
            s => pos_args.push(s.to_string()),
        }
    }
    if pos_args.len() != 2 || name.is_empty() || model == 0 {
        eprintln!("usage: place_forge <in.bin> <out.bin> --template <sub> --name <n> --model 0x<h> --pos X,Y,Z [--quat X,Y,Z,W]");
        return 2;
    }
    let block = match std::fs::read(&pos_args[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {}: {e}", pos_args[0]);
            return 1;
        }
    };
    // Entity key: derive a stable id from the name hash (never 0x00/0x01 low byte).
    let key = {
        let h = mercs2_formats::hash::pandemic_hash_m2(&name);
        (h & 0x00FF_FFFF) | 0x0020_0000 | 0x0000_0002 // mid-range, low byte != 0/1
    };
    let before = load_model_placements(&block).len();
    let out = match append_placement(&block, template, key, &name, model, pos, quat) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("append_placement: {e}");
            return 1;
        }
    };
    // round-trip verify: our (key -> model) must now parse.
    let placements = load_model_placements(&out);
    let found = placements.iter().find(|p| p.key == key && p.model_hash == model);
    match found {
        Some(p) => {
            if let Err(e) = std::fs::write(&pos_args[1], &out) {
                eprintln!("write {}: {e}", pos_args[1]);
                return 1;
            }
            println!(
                "placed '{name}' key=0x{key:08X} model=0x{model:08X} at {:?}; ModelName placements {} -> {} (round-trip OK) -> {} ({} bytes)",
                p.pos, before, placements.len(), pos_args[1], out.len()
            );
            0
        }
        None => {
            eprintln!(
                "ROUND-TRIP FAILED: our entity key=0x{key:08X} model=0x{model:08X} not found after append ({} model placements parsed)",
                placements.len()
            );
            1
        }
    }
}
