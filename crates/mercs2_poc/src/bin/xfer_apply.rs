//! Apply donor weight transfer to a conformed import and write the injected block.
//!
//! Experiment, measured rather than argued: does sampling the SHIPPED donor's skinning reach the
//! retail multi-influence distribution (82.8% / 93.3%) where inheriting the source rig's own
//! weights reaches only 14.6%?
//!
//!   xfer_apply <model.glb> <donor.block> <out.bin> [--name 0xHASH] [--group N] [-k N]

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::build::build_palette_ranges;
use mercs2_formats::char_skin::transfer::{transfer_weights, DonorSample};
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::model_inject::{
    inject_character_into_donor_block, inject_character_multi_into_donor_block, ExternalMesh,
};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: xfer_apply <model.glb> <donor.block> <out.bin> [--name 0xHASH] [--group N] [-k N]");
        std::process::exit(2);
    }
    let (glb_path, donor_path, out_path) = (&a[1], &a[2], &a[3]);
    let name = flag(&a, "--name")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0xDFDF_5B5D);
    // Comma-separated ordinals route through the MULTI-group path, which gives each host group its
    // OWN INFO(56) palette. That is what keeps a 70-bone transfer inside the format: one whole-model
    // palette needs 76 slots (retail max measured 48), three per-group palettes need far fewer each.
    let groups: Vec<usize> = flag(&a, "--group")
        .unwrap_or("3")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let k: usize = flag(&a, "-k").and_then(|s| s.parse().ok()).unwrap_or(4);

    let glb = gltf::load_char_glb(glb_path).expect("glb");
    let donor_block = std::fs::read(donor_path).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let mut cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");

    // Donor surface samples: every skinned vertex the shipped model has, with the GLOBAL bones the
    // reader expands its per-group palette to. All groups — group 3 alone is only the torso, and a
    // partial source would leave limbs sampling from nothing.
    let ucfx_len = u32::from_le_bytes(donor_block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&donor_block[20..20 + ucfx_len]).expect("donor meshes");
    let mut donor: Vec<DonorSample> = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let mut infl = Vec::new();
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            for c in 0..4 {
                let w = m.weights[i][c] as f64;
                if w > 0.0 {
                    infl.push((m.joints[i][c] as u32, w / tot));
                }
            }
            let p = m.positions[i];
            donor.push(DonorSample { pos: [p[0] as f64, p[1] as f64, p[2] as f64], infl });
        }
    }
    println!("donor samples: {}", donor.len());

    let t = transfer_weights(&donor, &cs.posed, k, target.height);
    println!(
        "transfer: k={k}  median nearest {:.4} m ({:.1}% of height)  far {} ({:.1}%)",
        t.median_dist, 100.0 * t.median_dist / target.height, t.far,
        100.0 * t.far as f64 / cs.posed.len() as f64
    );

    // Rebuild the palette from the bones the transfer actually used, then re-encode skin bytes.
    let mut used: Vec<u32> = t.per_vertex.iter().flatten().map(|x| x.0).collect();
    used.sort_unstable();
    used.dedup();
    let (ranges32, slot_of, slots) = build_palette_ranges(&used);
    println!("transferred bones: {}  palette: {} slots / {} runs", used.len(), slots, ranges32.len());

    let nv = cs.posed.len();
    let mut skin = vec![0u8; nv * 8];
    let mut multi = 0usize;
    for (vi, infl) in t.per_vertex.iter().enumerate() {
        // quantise to 255 with the residual on the largest fractional part (same policy as build.rs)
        let scaled: Vec<(u8, f64)> = infl
            .iter()
            .filter_map(|(b, w)| slot_of.get(b).map(|&s| (s, 255.0 * w)))
            .collect();
        let mut q: Vec<(u8, i64)> = scaled.iter().map(|&(s, x)| (s, x.floor() as i64)).collect();
        let rem = 255 - q.iter().map(|p| p.1).sum::<i64>();
        let mut order: Vec<usize> = (0..scaled.len()).collect();
        order.sort_by(|&x, &y| {
            let fy = scaled[y].1 - scaled[y].1.floor();
            let fx = scaled[x].1 - scaled[x].1.floor();
            fy.partial_cmp(&fx).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in 0..rem.max(0) as usize {
            if q.is_empty() { break; }
            let idx = order[i % q.len()];
            q[idx].1 += 1;
        }
        if q.iter().filter(|p| p.1 > 0).count() > 1 {
            multi += 1;
        }
        for (i, (s, w)) in q.iter().take(4).enumerate() {
            skin[vi * 8 + i] = *s;
            skin[vi * 8 + 4 + i] = (*w).clamp(0, 255) as u8;
        }
    }
    println!(
        "multi-influence AFTER transfer: {:.1}% ({} of {}) — shipped is 82.8% / 93.3%",
        100.0 * multi as f64 / nv as f64, multi, nv
    );

    cs.skin_bytes = skin;
    cs.ranges = ranges32.iter().map(|&(b, c)| (b as u16, c as u16)).collect();

    // For the MULTI path `joints` must be GLOBAL bone indices: the injector derives each group's
    // palette from the bones that group actually uses. For the single path it must be palette
    // SLOTS, because the whole-model palette is supplied alongside. Same bytes, different meaning.
    let global_joints: Vec<[u8; 4]> = t
        .per_vertex
        .iter()
        .map(|infl| {
            let mut o = [0u8; 4];
            for (i, (b, _)) in infl.iter().take(4).enumerate() {
                o[i] = *b as u8;
            }
            o
        })
        .collect();
    let weights: Vec<[u8; 4]> = (0..nv)
        .map(|i| {
            [
                cs.skin_bytes[i * 8 + 4],
                cs.skin_bytes[i * 8 + 5],
                cs.skin_bytes[i * 8 + 6],
                cs.skin_bytes[i * 8 + 7],
            ]
        })
        .collect();
    let slot_joints: Vec<[u8; 4]> = (0..nv)
        .map(|i| {
            [
                cs.skin_bytes[i * 8],
                cs.skin_bytes[i * 8 + 1],
                cs.skin_bytes[i * 8 + 2],
                cs.skin_bytes[i * 8 + 3],
            ]
        })
        .collect();

    // FAITHFUL PARTITION: sub-object first, then contiguous bone span.
    //
    // Retail authors a character as many small draw groups (shipped mattias: 22, bone counts
    // 2/9/2/48/27/6/2/4/13/48/...). Crucially their bone sets are near-CONTIGUOUS in HIER index:
    // group 3 packs 48 bones into 48 slots over 5 runs, chris 45 into 45 over 7 — zero gap
    // bridging. A group is a body REGION, and HIER indices are hierarchical, so a region is an
    // index range.
    //
    // Partitioning only by source primitive is too coarse: 50 Cent's "body" is one primitive
    // spanning the whole body, needing 47 scattered bones -> 8 runs -> 51 slots (+4 bridged).
    // So order triangles by (part, dominant bone) and cut into groups along that order: each
    // group then covers one part and a contiguous bone span, which is the retail shape.
    let dom_of = |tri: &[u32; 3]| -> u32 {
        let mut best = (0u8, u32::MAX);
        for &v in tri {
            let vi = v as usize;
            for c in 0..4 {
                let w = cs.skin_bytes[vi * 8 + 4 + c];
                if w > best.0 {
                    best = (w, global_joints[vi][c] as u32);
                }
            }
        }
        best.1
    };
    let mut part_of = vec![0usize; glb.tris.len()];
    for (pi, part) in glb.parts.iter().enumerate() {
        for t in part.tri_start..(part.tri_start + part.tri_count).min(part_of.len()) {
            part_of[t] = pi;
        }
    }
    let mut order: Vec<usize> = (0..glb.tris.len()).collect();
    order.sort_by_key(|&i| (part_of[i], dom_of(&glb.tris[i])));
    let tris: Vec<[u32; 3]> = order.iter().map(|&i| glb.tris[i]).collect();
    let per = (tris.len() + groups.len() - 1) / groups.len().max(1);
    let tri_group: Vec<usize> = (0..tris.len()).map(|i| (i / per).min(groups.len() - 1)).collect();
    if groups.len() > 1 {
        println!("partition: {} source parts, ordered by (part, dominant bone) -> {} groups",
            glb.parts.len(), groups.len());
    }

    let mut mesh = ExternalMesh {
        positions: cs.pos.clone(),
        normals: glb.normals.clone(),
        uvs: glb.uvs.clone(),
        tris,
        joints: slot_joints,
        weights,
    };

    let (block, target_group, vcount) = if groups.len() == 1 {
        let (b, st) = inject_character_into_donor_block(
            &donor_block, &mesh, &cs.ranges, groups[0], &[], name,
        )
        .expect("inject");
        (b, st.target_group, st.vertex_count)
    } else {
        mesh.joints = global_joints;
        let (b, _audits, st) = inject_character_multi_into_donor_block(
            &donor_block, &mesh, &groups, &[], name, true, Some(&tri_group),
        )
        .expect("inject multi");
        (b, st.target_group, st.vertex_count)
    };
    std::fs::write(out_path, &block).expect("write");
    println!(
        "wrote {out_path} ({} bytes): groups {:?} <- {} verts (stats group {}, {} verts)",
        block.len(), groups, nv, target_group, vcount
    );
}
