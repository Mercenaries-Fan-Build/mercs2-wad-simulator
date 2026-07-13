//! inject_parts — conform a MULTI-PART novel model into a real vehicle template.
//!
//! `inject_static` hosts ONE rigid mesh in ONE drawing group. A vehicle needs more:
//!
//!  * **Per-part materials.** Mercs2 binds a material per PRMG group (PRMT record word 0 IS the
//!    MTRL index — `model_cubeize::SubMesh::material_index`). One group = one skin, so the body,
//!    the gear, the glass and the rotor each need their own group to get their own texture.
//!  * **Moving parts.** A mesh only moves if its `SEGM` row names the HIER node that moves it
//!    (`BoneCtrlLocalRotation` spins the rotor NODE). And a rigid `MESH` sub-object is authored in
//!    that node's LOCAL space — so rotor geometry must be pushed through `inverse(node.world)`,
//!    or the engine's node matrix flings it across the map.
//!  * **Static parts** get `node = -1`: model space, always visible (draw-gate clause 3 can't gate
//!    a negative node), every LOD tier, and never superseded by a finer LOD rung.
//!
//! See docs/modernization/vehicle_model_spec.md §2/§4 for the binding chain this implements.
//!
//! Usage:
//!   inject_parts <template.ucfx> <out.ucfx> --name-hash 0xH \
//!       --part <mesh.mesh>:<group>:<node>:<mtrl_idx> [--part ...] \
//!       [--repoint 0xFROM:0xTO] [--scale S]
//!
//! `node` = -1 for a static part, else the HIER node index that should carry it.

use mercs2_formats::model_inject::{inject_parts_into_template, ExternalMesh, PartSpec};

fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn rd_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn parse_hash(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

fn load_mesh(path: &str) -> Result<ExternalMesh, String> {
    let d = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    if d.len() < 12 || &d[0..4] != b"MESH" {
        return Err(format!("{path} is not a MESH blob"));
    }
    let nv = rd_u32(&d, 4) as usize;
    let nt = rd_u32(&d, 8) as usize;
    let mut o = 12;
    let mut positions = Vec::with_capacity(nv);
    for _ in 0..nv {
        positions.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]);
        o += 12;
    }
    let mut normals = Vec::with_capacity(nv);
    for _ in 0..nv {
        normals.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]);
        o += 12;
    }
    let mut uvs = Vec::with_capacity(nv);
    for _ in 0..nv {
        uvs.push([rd_f32(&d, o), rd_f32(&d, o + 4)]);
        o += 8;
    }
    let mut tris = Vec::with_capacity(nt);
    for _ in 0..nt {
        tris.push([rd_u32(&d, o), rd_u32(&d, o + 4), rd_u32(&d, o + 8)]);
        o += 12;
    }
    Ok(ExternalMesh { positions, normals, uvs, tris, joints: vec![], weights: vec![] })
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<String> = Vec::new();
    let mut name_hash = 0u32;
    let mut scale_mult = 1.0f32;
    let mut flip = true;
    let mut y_offset = 0.0f32;
    let mut fit_percentile = 100.0f32;
    let mut parts: Vec<PartSpec> = Vec::new();
    let mut repoints: Vec<(u32, u32)> = Vec::new();
    let mut mtrl_sets: Vec<(usize, usize, u32)> = Vec::new();
    let mut mtrl_adds: Vec<(usize, u32)> = Vec::new();
    let mut mtrl_replaces: Vec<(usize, usize, u32)> = Vec::new();
    let mut node_ats: Vec<(usize, [f32; 3])> = Vec::new();
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--name-hash" => name_hash = it.next().and_then(|s| parse_hash(s)).unwrap_or(0),
            "--scale" => scale_mult = it.next().and_then(|s| s.parse().ok()).unwrap_or(1.0),
            "--y-offset" => y_offset = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            "--fit-percentile" => fit_percentile = it.next().and_then(|s| s.parse().ok()).unwrap_or(100.0),
            "--no-flip" => flip = false,
            "--add-mtrl" => {
                // <clone_from_index>:<0xTEXTURE> — append a copy of a KNOWN-GOOD material with a
                // new diffuse. Its index is the old material count (printed below).
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    if let (Some(i), Some(t)) =
                        (p.next().and_then(|s| s.parse::<usize>().ok()), p.next().and_then(parse_hash))
                    {
                        mtrl_adds.push((i, t));
                    }
                }
            }
            "--node-at" => {
                // <node>:<x>,<y>,<z> — move a HIER node to this MODEL-SPACE point (post-fit), so the
                // donor's rig lands on OUR tank's real axis (turret ring, gun trunnion). The node's
                // whole subtree moves with it. This is the RIGHT way to re-rig a novel model: do NOT
                // shove the geometry onto the donor's node (that displaces the part off the body).
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    let node = p.next().and_then(|s| s.parse::<usize>().ok());
                    let xyz: Option<Vec<f32>> = p.next().map(|c| {
                        c.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect()
                    });
                    match (node, xyz) {
                        (Some(n), Some(v)) if v.len() == 3 => node_ats.push((n, [v[0], v[1], v[2]])),
                        _ => {
                            eprintln!("--node-at needs <node>:<x>,<y>,<z>");
                            return 2;
                        }
                    }
                }
            }
            "--replace-mtrl" => {
                // <dst>:<src>:<0xTEXTURE> — overwrite material `dst` with a clone of `src` and a new
                // diffuse, keeping `dst`'s own name hash. Unlike --add-mtrl this does NOT grow the
                // material count, which is what a 9th material cannot survive (NULL shader slot ->
                // draw-time crash 0x00855691). Use an unused/untextured record as the `dst`.
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    if let (Some(d), Some(s), Some(t)) = (
                        p.next().and_then(|s| s.parse::<usize>().ok()),
                        p.next().and_then(|s| s.parse::<usize>().ok()),
                        p.next().and_then(parse_hash),
                    ) {
                        mtrl_replaces.push((d, s, t));
                    }
                }
            }
            "--set-mtrl" => {
                // <material_index>:<0xTEXTURE> — set the DIFFUSE (texture slot 0) of one MTRL record.
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    if let (Some(i), Some(t)) =
                        (p.next().and_then(|s| s.parse::<usize>().ok()), p.next().and_then(parse_hash))
                    {
                        mtrl_sets.push((i, 0, t));
                    }
                }
            }
            "--set-tex" => {
                // <material_index>:<slot>:<0xTEXTURE> — set ANY texture slot. slot 0 = diffuse,
                // 1 = NORMAL, 2 = specular. The donor's normal map sampled through our UVs is what
                // makes a novel model look like crumpled foil; repoint slot 1 as well as slot 0.
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    if let (Some(i), Some(sl), Some(t)) = (
                        p.next().and_then(|s| s.parse::<usize>().ok()),
                        p.next().and_then(|s| s.parse::<usize>().ok()),
                        p.next().and_then(parse_hash),
                    ) {
                        mtrl_sets.push((i, sl, t));
                    } else {
                        eprintln!("--set-tex needs <mtrl>:<slot>:<0xTEX>");
                        return 2;
                    }
                }
            }
            "--repoint" => {
                if let Some(v) = it.next() {
                    let mut p = v.split(':');
                    if let (Some(f), Some(t)) = (p.next().and_then(parse_hash), p.next().and_then(parse_hash)) {
                        repoints.push((f, t));
                    }
                }
            }
            "--part" => {
                let Some(v) = it.next() else { continue };
                let f: Vec<&str> = v.split(':').collect();
                if f.len() < 4 {
                    eprintln!("--part needs <mesh>:<group>:<node>:<mtrl_idx>[:spin], got '{v}'");
                    return 2;
                }
                let mesh = match load_mesh(f[0]) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("{e}");
                        return 1;
                    }
                };
                parts.push(PartSpec {
                    label: f[0].rsplit(['/', '\\']).next().unwrap_or(f[0]).to_string(),
                    mesh,
                    group: f[1].parse().unwrap_or(0),
                    node: f[2].parse().unwrap_or(-1),
                    material_index: f[3].parse().unwrap_or(0),
                    // 5th field "spin": re-centre X/Z onto the node axis. Required for any part on a
                    // ROTATING node — otherwise it orbits the node instead of spinning in place.
                    recenter_xz: f.get(4).is_some_and(|s| *s == "spin"),
                });
            }
            s => pos.push(s.to_string()),
        }
    }
    if pos.len() != 2 || name_hash == 0 || parts.is_empty() {
        eprintln!(
            "usage: inject_parts <template.ucfx> <out.ucfx> --name-hash 0xH \\\n  \
             --part <mesh>:<group>:<node>:<mtrl_idx> [--part ...] [--repoint 0xF:0xT] [--scale S]"
        );
        return 2;
    }

    let ucfx = match std::fs::read(&pos[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {}: {e}", pos[0]);
            return 1;
        }
    };
    let (out, stats) = match inject_parts_into_template(&ucfx, &parts, &repoints, &mtrl_sets, &mtrl_adds, &mtrl_replaces, &node_ats, name_hash, scale_mult, flip, y_offset, fit_percentile) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("inject_parts: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&pos[1], &out) {
        eprintln!("write {}: {e}", pos[1]);
        return 1;
    }
    println!(
        "fit: scale {:.4}, model bbox [{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]",
        stats.fit_scale,
        stats.bbox_min[0], stats.bbox_min[1], stats.bbox_min[2],
        stats.bbox_max[0], stats.bbox_max[1], stats.bbox_max[2]
    );
    for p in &stats.parts {
        println!(
            "  {:<16} grp {:>2}  node {:>4}  mtrl {}  seg_id {:>3}  {} verts / {} tris{}",
            p.label, p.group, p.node, p.material_index, p.seg_id, p.vertex_count, p.triangle_count,
            if p.node >= 0 { "  [node-local: spins with its node]" } else { "  [model space, all tiers]" }
        );
    }
    if stats.phy2_hulls_scaled > 0 {
        println!("  PHY2: rescaled {} collision hull(s) by {scale_mult}x", stats.phy2_hulls_scaled);
    }
    for (n, p) in &stats.nodes_moved {
        println!("  node {n:3} RETARGETED -> ({:.2}, {:.2}, {:.2})  [subtree moved with it]", p[0], p[1], p[2]);
    }
    println!("neutralised {} other group(s); MTRL repoints: {}", stats.emptied_groups.len(), stats.mtrl_repoints);
    println!("-> {} ({} bytes)", pos[1], out.len());
    0
}
