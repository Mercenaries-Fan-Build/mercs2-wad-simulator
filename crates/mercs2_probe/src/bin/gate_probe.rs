//! Dev bin: dump, per DRAW GROUP, everything the engine's three-clause draw gate tests —
//! the SEGM node (clause 3 key), the LOD-rung mask (clause 2), the INDX node, and whether the
//! destruction state machine's default state leaves that node enabled.
//!
//! Purpose: confirm which node field actually keys the node-enable table. `SEGM[sub_object].bone`
//! and `INDX[group]` are NOT always the same node, and clause 3 indexes `OBJ+0x2a0` by the SEGM
//! record's signed `node` field.
//!
//!   cargo run -p mercs2_probe --bin gate_probe -- oc_veh_helicopter_md500

use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or_else(|| "oc_veh_helicopter_md500".into());
    let hash = name
        .strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));

    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");
    let (verts, indices, draws, _) = mesh::build_indexed_all(&c).expect("build all");

    // World-space extent of one draw group's actual triangles — the honest way to tell a hull from a
    // break piece when the bone names aren't in the rainbow table.
    let extent = |d: &mesh::DrawGroup| -> (f32, f32, f32, [f32; 3]) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for i in d.index_start..d.index_start + d.index_count {
            let p = verts[indices[i as usize] as usize].pos;
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if d.index_count == 0 {
            return (0.0, 0.0, 0.0, [0.0; 3]);
        }
        (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], [
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        ])
    };

    let hier = orchestrator::parse_hier(&c);
    let indx = orchestrator::parse_indx(&c);
    let machine = orchestrator::parse_state_machine(&c);

    let node_enable: Vec<bool> = match &machine {
        Some(sm) => {
            let chosen: Vec<usize> =
                sm.nodes.iter().map(orchestrator::default_state_index).collect();
            orchestrator::machine_node_enable(sm, &hier, &chosen)
        }
        None => Vec::new(),
    };

    println!("{name} (0x{hash:08X})");
    println!(
        "  {} draw groups (whole build), {} HIER nodes, {} INDX rows, machine: {}",
        draws.len(),
        hier.len(),
        indx.len(),
        machine.as_ref().map_or("none".into(), |m| format!("{} switch nodes", m.nodes.len()))
    );
    if !node_enable.is_empty() {
        let on = node_enable.iter().filter(|b| **b).count();
        println!("  node_enable @ default state: {on} of {} nodes enabled", node_enable.len());
    }

    // The state machine's SHOW/Hide targets, by name hash — the things that actually decide clause 3.
    if let Some(sm) = &machine {
        let show = mercs2_formats::hash::pandemic_hash_m2("show");
        let hide = mercs2_formats::hash::pandemic_hash_m2("hide");
        for (ni, node) in sm.nodes.iter().enumerate() {
            let si = orchestrator::default_state_index(node);
            let Some(st) = node.states.get(si) else { continue };
            let (mut shows, mut hides) = (Vec::new(), Vec::new());
            let (mut args, mut i) = (Vec::new(), 0usize);
            while i < st.enter.len() {
                match st.enter[i] {
                    1 if i + 1 < st.enter.len() => {
                        args.push(st.enter[i + 1]);
                        i += 2;
                    }
                    2 if i + 1 < st.enter.len() => {
                        let cmd = st.enter[i + 1];
                        if cmd == show {
                            shows.extend(args.iter().copied());
                        }
                        if cmd == hide {
                            hides.extend(args.iter().copied());
                        }
                        args.clear();
                        i += 2;
                    }
                    3 => i += 1,
                    _ => {
                        args.push(st.enter[i]);
                        i += 1;
                    }
                }
            }
            if !shows.is_empty() || !hides.is_empty() {
                println!(
                    "  switch node {ni} (0x{:08X}) default state 0x{:08X}:",
                    node.name_hash, st.name_hash
                );
                for h in &shows {
                    println!("      SHOW 0x{h:08X}  (hier idx {:?})", hier.iter().position(|n| n.hash == *h));
                }
                for h in &hides {
                    println!("      Hide 0x{h:08X}  (hier idx {:?})", hier.iter().position(|n| n.hash == *h));
                }
            }
        }
    }

    // Which texture hashes actually exist in the wad, so a group that renders WHITE (unbound
    // material) is distinguishable from a low-poly LOD — the user's "damaged/transparent" look.
    let mut tex_ok = |h: Option<u32>| -> String {
        match h {
            None => "no-mat".into(),
            Some(hash) => {
                let bound = wad::extract_texture(&mut w, hash).is_ok();
                format!("0x{hash:08X}{}", if bound { "" } else { "!MISSING" })
            }
        }
    };

    println!("\n  {:>3} {:>4} {:>9} {:>6} {:>7} {:>6} {:>7} {:>19} {:>17}", "grp", "sub", "node", "mask", "seg_on", "tris", "size", "center", "diffuse");
    let en = |n: i64| -> String {
        if n < 0 {
            return "always".into();
        }
        match node_enable.get(n as usize) {
            Some(true) => "ON".into(),
            Some(false) => "off".into(),
            None => "-".into(),
        }
    };
    let mut disagree = 0;
    for d in &draws {
        let inode = indx.get(d.group_index).copied();
        let i_s = inode.map_or("-".to_string(), |n| n.to_string());
        let seg_on = en(d.node as i64);
        let indx_on = inode.map_or("-".into(), |n| en(n as i64));
        if seg_on != indx_on {
            disagree += 1;
        }
        let nhash = if d.node >= 0 {
            hier.get(d.node as usize).map_or("-".into(), |n| format!("0x{:08X}", n.hash))
        } else {
            "-".into()
        };
        let _ = (nhash, i_s, indx_on);
        let (sx, sy, sz, ctr) = extent(d);
        println!(
            "  {:>3} {:>4} {:>9} {:>#6x} {:>7} {:>6} {:>19} {:>17} {}",
            d.group_index,
            d.sub_object,
            d.node,
            d.lod_mask,
            seg_on,
            d.index_count / 3,
            format!("{sx:.1}x{sy:.1}x{sz:.1}m"),
            format!("({:.1},{:.1},{:.1})", ctr[0], ctr[1], ctr[2]),
            tex_ok(d.diffuse),
        );
    }
    println!("\n  groups where SEGM.node and INDX.node disagree on enablement: {disagree}");
}
