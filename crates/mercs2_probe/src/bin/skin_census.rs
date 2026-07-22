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
//!   skin_census <container-or-block.bin> [more.bin ...]

use mercs2_formats::model_cubeize::read_model_meshes;
use std::collections::BTreeSet;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
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
        eprintln!("usage: skin_census [--group N] <container-or-block.bin> [...]");
        std::process::exit(2);
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
        // Accept either a bare UCFX container or a wrapped block (20-byte header + UCFX).
        let container: &[u8] = if raw.len() > 4 && &raw[0..4] == b"UCFX" {
            &raw
        } else if raw.len() > 20 {
            let n = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as usize;
            if 20 + n <= raw.len() && &raw[20..24] == b"UCFX" { &raw[20..20 + n] } else { &raw }
        } else {
            &raw
        };

        let meshes = match read_model_meshes(container) {
            Ok(m) => m,
            Err(e) => { println!("{path}: read_model_meshes: {e}"); continue; }
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
            println!("{:<34} {:>6} {:>7}  (no skinned groups)", short(path), 0, 0);
            continue;
        }
        let pct = |a: usize| 100.0 * a as f64 / verts as f64;
        println!(
            "{:<34} {:>6} {:>7} {:>6} {:>6.1}% {:>6.1}% {:>6.1}%",
            short(path), bones.len(), verts, skinned_groups, pct(rigid), pct(multi), pct(four)
        );
    }
    println!("\nrigid% = vertices bound to exactly ONE bone. High rigid% with few bones is the shape\nthat survives bind pose and tears when a joint bends.");
}

fn short(p: &str) -> String {
    let b = p.rsplit(['/', '\\']).next().unwrap_or(p);
    b.chars().take(34).collect()
}
