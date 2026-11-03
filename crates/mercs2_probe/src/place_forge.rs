//! place_forge — append a NEW SceneObject placement to a decompressed placement
//! block and round-trip-verify it via the reader.
//!
//! Usage:
//!   place_forge <in_block.bin> <out_block.bin> --template <sub_idx> \
//!       --name <entity_name> --model 0x<hash> --pos X,Y,Z [--quat X,Y,Z,W] \
//!       [--layer-name <layer> | --layer-hash 0x<H>]
//!
//! `--layer-name`/`--layer-hash` set the appended sub-block's ENTRY-TABLE NAME to a
//! chosen layer hash `H`. The retail engine loads a layer by name-hash through the
//! asset system, so `H` must ALSO be advertised by a matching ASET row (feed the
//! printed `H` to `override_base_blocks --add-layer 0xH`). Without a flag, `H`
//! defaults to the first authored entity key (a self-referential name that no ASET
//! row advertises — the sub-block will parse but the engine can never resolve it).

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::placement::{entity_key_set, load_model_placements};
use mercs2_formats::placement_build::{append_placements, NewEntity};

fn parse_vec3(s: &str) -> [f32; 3] {
    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    [
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
        v.get(2).copied().unwrap_or(0.0),
    ]
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
                let v: Vec<f32> = it
                    .next()
                    .cloned()
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if v.len() == 2 {
                    near = Some([v[0], v[1]]);
                }
            }
            "--radius" => {
                radius = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(f32::INFINITY)
            }
            s if !s.starts_with("--") => path = s.to_string(),
            _ => {}
        }
    }
    let block = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return 1;
        }
    };
    let ps = load_model_placements(&block);
    let mut shown = 0;
    for p in &ps {
        if let Some([nx, nz]) = near {
            let dx = p.pos[0] - nx;
            let dz = p.pos[2] - nz;
            if (dx * dx + dz * dz).sqrt() > radius {
                continue;
            }
        }
        println!(
            "key=0x{:08X} model=0x{:08X} pos=({:.2}, {:.2}, {:.2}) name={:?}",
            p.key, p.model_hash, p.pos[0], p.pos[1], p.pos[2], p.name
        );
        shown += 1;
        if shown >= 2000 {
            println!("... ({} total)", ps.len());
            break;
        }
    }
    if shown == 0 {
        println!("(no model placements matched; {} total in block)", ps.len());
    }
    0
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--list") {
        return list_mode(&argv);
    }
    let mut args = std::env::args().skip(1);
    let mut pos_args: Vec<String> = Vec::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let (mut template, mut name, mut model, mut quat) = (
        15usize,
        String::new(),
        0u32,
        [0.0f32, 0.0, 0.0, 1.0],
    );
    // The appended sub-block's entry-table name (the layer's ASET asset hash H). When
    // unset it falls back to the first entity key (prior behaviour). `--layer-name`
    // derives H = pandemic_hash_m2(name); `--layer-hash` sets it directly.
    let mut layer_hash: Option<u32> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--template" => template = args.next().and_then(|s| s.parse().ok()).unwrap_or(15),
            "--name" => name = args.next().unwrap_or_default(),
            "--layer-name" => layer_hash = args.next().map(|s| pandemic_hash_m2(&s)),
            "--layer-hash" => {
                layer_hash = args
                    .next()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            }
            "--model" => {
                model = args
                    .next()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .unwrap_or(0)
            }
            // Repeatable single position.
            "--pos" => positions.push(parse_vec3(&args.next().unwrap_or_default())),
            // `X,Y,Z;X,Y,Z;...` — N positions in one flag (place N entities at once).
            "--pos-list" => {
                for tri in args.next().unwrap_or_default().split(';') {
                    let t = tri.trim();
                    if !t.is_empty() {
                        positions.push(parse_vec3(t));
                    }
                }
            }
            "--quat" => {
                let v: Vec<f32> = args
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .collect();
                if v.len() == 4 {
                    quat = [v[0], v[1], v[2], v[3]];
                }
            }
            s => pos_args.push(s.to_string()),
        }
    }
    if pos_args.len() != 2 || name.is_empty() || model == 0 || positions.is_empty() {
        eprintln!("usage: place_forge <in.bin> <out.bin> --template <sub> --name <n> --model 0x<h> --pos X,Y,Z [--pos ...] [--pos-list X,Y,Z;X,Y,Z] [--quat X,Y,Z,W]");
        return 2;
    }
    let block = match std::fs::read(&pos_args[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {}: {e}", pos_args[0]);
            return 1;
        }
    };

    // Allocate one fresh entity key per position: dense mid-range ids from the free
    // 0x00F0_0000.. band, skipping any that collide with an existing block key and
    // any whose low byte is 0x00/0x01 (the Name-parser reserves those as flags).
    let used = entity_key_set(&block);
    let mut keys: Vec<u32> = Vec::with_capacity(positions.len());
    let mut cand = 0x00F0_0000u32;
    while keys.len() < positions.len() {
        let low = cand & 0xFF;
        if low != 0x00 && low != 0x01 && !used.contains(&cand) {
            keys.push(cand);
        }
        cand += 1;
    }

    let ents: Vec<NewEntity> = keys
        .iter()
        .zip(&positions)
        .enumerate()
        .map(|(i, (&key, &pos))| NewEntity {
            key,
            model_hash: model,
            pos,
            quat,
            name: if positions.len() == 1 {
                name.clone()
            } else {
                format!("{name}_{i}")
            },
        })
        .collect();

    // The layer's ASET asset hash H (entry-table name of the appended sub-block). Feed
    // this to `override_base_blocks --add-layer 0xH` so the engine can resolve it.
    let h = layer_hash.unwrap_or(ents[0].key);

    let before = load_model_placements(&block).len();
    let out = match append_placements(&block, template, &ents, h) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("append_placements: {e}");
            return 1;
        }
    };
    // round-trip verify: every (key -> model) we authored must now parse back.
    let placements = load_model_placements(&out);
    let missing: Vec<u32> = ents
        .iter()
        .filter(|e| {
            !placements
                .iter()
                .any(|p| p.key == e.key && p.model_hash == e.model_hash && p.pos == e.pos)
        })
        .map(|e| e.key)
        .collect();
    if missing.is_empty() {
        if let Err(e) = std::fs::write(&pos_args[1], &out) {
            eprintln!("write {}: {e}", pos_args[1]);
            return 1;
        }
        println!(
            "placed {} entity(ies) '{name}' model=0x{model:08X} keys=0x{:08X}..=0x{:08X}; ModelName placements {} -> {} (round-trip OK) -> {} ({} bytes)",
            ents.len(),
            ents.first().map(|e| e.key).unwrap_or(0),
            ents.last().map(|e| e.key).unwrap_or(0),
            before,
            placements.len(),
            pos_args[1],
            out.len()
        );
        println!(
            "  layer entry-table name (H) = 0x{h:08X}{}  <- pass to `override_base_blocks --add-layer 0x{h:08X}`",
            layer_hash
                .map(|_| String::new())
                .unwrap_or_else(|| " (defaulted to first entity key; no ASET row advertises this)".into())
        );
        0
    } else {
        eprintln!(
            "ROUND-TRIP FAILED: {} of {} authored entities not found after append (e.g. 0x{:08X}); {} model placements parsed",
            missing.len(),
            ents.len(),
            missing[0],
            placements.len()
        );
        1
    }
}
