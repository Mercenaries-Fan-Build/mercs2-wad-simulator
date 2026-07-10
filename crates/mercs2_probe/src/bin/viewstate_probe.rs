//! Dev bin: sweep every candidate `view_state` for a model and report what clause 2 + clause 3 draw.
//!
//! We do NOT read the engine's `model+0x80` (minLOD) / `model+0x7c` (maxLOD) clamps, nor the flags
//! bit-9 cross-fade that composes `view_state` as `1<<(n-1) | 1<<n | 1<<(n+1)`. So our hardcoded
//! `view_state = 0x01` ("rung 0") is an assumption. This sweeps the space and prints, per candidate,
//! the drawn triangle count and the union bounding box — a correct `view_state` reconstructs the
//! model's full envelope once, with no duplicated geometry.
//!
//!   cargo run -p mercs2_probe --bin viewstate_probe -- ch_veh_tank_ztz98

use mercs2_engine::render_state::RenderState;
use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = name
        .strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));

    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");
    let (verts, indices, draws, _) = mesh::build_indexed_all(&c).expect("build all");
    let hier = orch::parse_hier(&c);
    let node_enable = match orch::parse_state_machine(&c) {
        Some(sm) => {
            let chosen: Vec<usize> = sm.nodes.iter().map(orch::default_state_index).collect();
            orch::machine_node_enable(&sm, &hier, &chosen)
        }
        None => Vec::new(),
    };

    let bbox = |ds: &[&mesh::DrawGroup]| -> ([f32; 3], [f32; 3], usize) {
        let (mut lo, mut hi, mut tris) = ([f32::MAX; 3], [f32::MIN; 3], 0usize);
        for d in ds {
            tris += (d.index_count / 3) as usize;
            for i in d.index_start..d.index_start + d.index_count {
                let p = verts[indices[i as usize] as usize].pos;
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi, tris)
    };

    // The model's full envelope, from every non-break-piece (node-enabled) group.
    let live: Vec<&mesh::DrawGroup> = draws
        .iter()
        .filter(|d| d.node < 0 || node_enable.get(d.node as usize).copied().unwrap_or(true))
        .collect();
    let (flo, fhi, ftris) = bbox(&live);
    println!("{name} (0x{hash:08X})");
    println!(
        "  full node-enabled envelope: {:.1}x{:.1}x{:.1} m, {ftris} tris across {} groups\n",
        fhi[0] - flo[0],
        fhi[1] - flo[1],
        fhi[2] - flo[2],
        live.len()
    );

    println!("  {:>10} {:>6} {:>6} {:>20}  {}", "view_state", "groups", "tris", "envelope", "note");
    let mut rows: Vec<(u8, usize, usize, [f32; 3])> = Vec::new();
    for vs in 1..=u8::MAX {
        let rs = RenderState { lod: 0, view_state: vs, node_enable: node_enable.clone() };
        let sel: Vec<&mesh::DrawGroup> =
            draws.iter().filter(|d| rs.segment_visible(d.lod_mask, d.node)).collect();
        if sel.is_empty() {
            continue;
        }
        let (lo, hi, tris) = bbox(&sel);
        rows.push((vs, sel.len(), tris, [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]));
    }

    // Candidates worth a human's attention: the 7 single bits, and the 3-rung cross-fade windows.
    let single: Vec<u8> = (0..7).map(|n| 1u8 << n).collect();
    let windows: Vec<u8> = (0..7)
        .map(|n: u32| {
            let b = |i: i32| -> u8 {
                if (0..8).contains(&i) {
                    1u8 << i
                } else {
                    0
                }
            };
            b(n as i32 - 1) | b(n as i32) | b(n as i32 + 1)
        })
        .collect();

    for (vs, groups, tris, env) in &rows {
        let mut note = String::new();
        if let Some(n) = single.iter().position(|b| b == vs) {
            note.push_str(&format!("single bit -> rung {n}"));
        }
        if let Some(n) = windows.iter().position(|b| b == vs) {
            if !note.is_empty() {
                note.push_str("; ");
            }
            note.push_str(&format!("cross-fade window n={n}"));
        }
        if note.is_empty() {
            continue;
        }
        let cover = (env[0] * env[1] * env[2])
            / (((fhi[0] - flo[0]) * (fhi[1] - flo[1]) * (fhi[2] - flo[2])).max(1e-6));
        println!(
            "  {:>#10x} {groups:>6} {tris:>6} {:>20}  {note} (envelope {:.0}% of full)",
            vs,
            format!("{:.1}x{:.1}x{:.1}m", env[0], env[1], env[2]),
            cover * 100.0
        );
    }
}
