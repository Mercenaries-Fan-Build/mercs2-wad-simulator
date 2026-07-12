//! LOSSLESS model-export bundle — the community-editable interchange for a game asset.
//!
//! **The preservation rule: nothing is discarded.** An editable format can only carry what it
//! understands, and we have NOT fully reversed everything a vehicle container holds (Havok `PHY2`
//! hulls, the `CHDR`/`CEXE` destruction scripts, `SWIT`/`STAT`/`NODE`, `MTRL` shader params, plus
//! any tag we have not decoded at all). So the bundle keeps BOTH:
//!
//!   * `raw/*.ucfx`   — every LOD rung's ORIGINAL container bytes, verbatim. This is the guarantee:
//!                      whatever we failed to understand is still here, byte-exact, and can be put
//!                      straight back. A bundle can always be reassembled into a working asset.
//!   * `model.gltf`   — the EDITABLE view: geometry + the real HIER node tree + materials + UVs.
//!                      Community tools (Blender/Max/Maya/Unity/Unreal) all read glTF 2.0.
//!   * `textures/`    — skins decoded to PNG, editable, with their source hash in the manifest.
//!   * `manifest.json`— the reassembly map: every draw group's binding
//!                      (group -> sub_object -> INDX -> seg_id -> SEGM{node, lod_mask} -> material),
//!                      the model header, the LOD-block chain, and a FULL CHUNK INVENTORY of each
//!                      rung (every tag, its size) so nothing is silently invisible.
//!
//! Geometry is emitted in each group's NODE-LOCAL space and parented to that node in the glTF, so
//! the rig is real: moving the rotor node in Blender moves the rotor. (`build_indexed_all` hands us
//! world-space verts, so we divide by the node's world-rest to get back to local.)

use std::collections::BTreeMap;
use std::path::Path;

use mercs2_engine::mesh::DrawGroup;
use mercs2_engine::render::TexMap;
use mercs2_formats::skeleton::{affine_inverse, transform_dir, transform_point, Skeleton};

/// Walk a UCFX container's descriptor rows -> (tag, is_container_marker, size).
fn chunk_inventory(c: &[u8]) -> Vec<(String, bool, u32)> {
    let mut out = Vec::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let rd = |o: usize| u32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
    let ndesc = rd(16) as usize;
    let max = c.len().saturating_sub(20) / 20;
    for i in 0..ndesc.min(max) {
        let ro = 20 + i * 20;
        let tag = String::from_utf8_lossy(&c[ro..ro + 4]).to_string();
        let u0 = rd(ro + 4);
        let size = rd(ro + 8);
        out.push((tag, u0 == 0xFFFF_FFFF, size));
    }
    out
}

fn f32s(v: &[f32]) -> Vec<serde_json::Value> {
    v.iter().map(|x| serde_json::json!(x)).collect()
}

/// Write the full lossless bundle for one model.
#[allow(clippy::too_many_arguments)]
pub fn export_bundle(
    outdir: &Path,
    label: &str,
    hash: u32,
    lods: &[mercs2_engine::wad::ModelLod],
    verts: &[mercs2_engine::mesh::Vertex],
    indices: &[u32],
    draws: &[DrawGroup],
    textures: &TexMap,
    hier: &[mercs2_formats::orchestrator::HierNode],
    header: Option<&mercs2_formats::model_cubeize::ModelHeader>,
) -> Result<(), String> {
    std::fs::create_dir_all(outdir.join("raw")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(outdir.join("textures")).map_err(|e| e.to_string())?;

    // ---- 1. RAW containers, verbatim. The losslessness guarantee. ----
    let mut rungs_json = Vec::new();
    for l in lods {
        let name = format!("raw/block{}_P{:03}.ucfx", l.block, l.level);
        std::fs::write(outdir.join(&name), &l.container).map_err(|e| e.to_string())?;
        let inv = chunk_inventory(&l.container);
        let mut tally: BTreeMap<String, (usize, u64)> = BTreeMap::new();
        for (tag, marker, size) in &inv {
            if *marker {
                continue;
            }
            let e = tally.entry(tag.clone()).or_insert((0, 0));
            e.0 += 1;
            e.1 += *size as u64;
        }
        rungs_json.push(serde_json::json!({
            "block": l.block,
            "lod_level": l.level,
            "file": name,
            "bytes": l.container.len(),
            // Every chunk the rung carries — including ones we do not decode (PHY2, CHDR, CEXE,
            // SWIT, STAT...). Listed so nothing is invisible; the bytes are in the .ucfx above.
            "chunks": tally.iter().map(|(t,(n,b))| serde_json::json!({"tag":t,"count":n,"bytes":b})).collect::<Vec<_>>(),
        }));
    }

    // ---- 2. Textures -> PNG (editable), hash recorded for rebind. ----
    let mut tex_json = Vec::new();
    let mut tex_file: BTreeMap<u32, String> = BTreeMap::new();
    for (h, td) in textures.iter() {
        let file = format!("textures/tex_0x{h:08X}.png");
        let rgba = crate::texpng::decode_bc(td);
        crate::texpng::write_png(
            outdir.join(&file).to_str().ok_or("bad path")?,
            td.width,
            td.height,
            &rgba,
        )?;
        tex_json.push(serde_json::json!({
            "hash": format!("0x{h:08X}"), "file": file, "width": td.width, "height": td.height,
        }));
        tex_file.insert(*h, file);
    }

    // ---- 3. glTF: real node tree + geometry parented to its binding node. ----
    // Skeleton (world-rest per node) so we can put each group back into NODE-LOCAL space.
    let skel = lods
        .iter()
        .min_by_key(|l| l.level)
        .and_then(|l| {
            let mut b = Vec::with_capacity(20 + l.container.len());
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&hash.to_le_bytes());
            b.extend_from_slice(&0x5B72_4250u32.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&(l.container.len() as u32).to_le_bytes());
            b.extend_from_slice(&l.container);
            Skeleton::from_block(&b).ok()
        });

    let mut bin: Vec<u8> = Vec::new();
    let mut accessors = Vec::new();
    let mut views = Vec::new();
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    let mut images = Vec::new();
    let mut gtextures = Vec::new();
    let mut mat_of_tex: BTreeMap<u32, usize> = BTreeMap::new();

    // A material (+image/texture) per distinct diffuse.
    for (h, file) in &tex_file {
        let img = images.len();
        images.push(serde_json::json!({ "uri": file }));
        let t = gtextures.len();
        gtextures.push(serde_json::json!({ "source": img }));
        mat_of_tex.insert(*h, materials.len());
        materials.push(serde_json::json!({
            "name": format!("mat_0x{h:08X}"),
            "pbrMetallicRoughness": {
                "baseColorTexture": { "index": t },
                "metallicFactor": 0.0, "roughnessFactor": 0.9
            },
            "doubleSided": true
        }));
    }
    let fallback_mat = materials.len();
    materials.push(serde_json::json!({
        "name": "untextured",
        "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 0.9 },
        "doubleSided": true
    }));

    // One glTF mesh per draw group, geometry pushed back into its node's local space.
    let mut mesh_of_group: Vec<(usize, i16)> = Vec::new(); // (mesh index, node)
    for d in draws.iter() {
        let s = d.index_start as usize;
        let e = ((d.index_start + d.index_count) as usize).min(indices.len());
        if s >= e {
            continue;
        }
        let inv = if d.node >= 0 {
            skel.as_ref().and_then(|sk| sk.bones.get(d.node as usize)).map(|b| affine_inverse(&b.world))
        } else {
            None
        };

        // Re-index this group's verts compactly.
        let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
        let mut gi: Vec<u32> = Vec::with_capacity(e - s);
        for &ix in &indices[s..e] {
            let n = remap.len() as u32;
            let id = *remap.entry(ix).or_insert(n);
            gi.push(id);
        }
        let mut order: Vec<(u32, u32)> = remap.iter().map(|(k, v)| (*v, *k)).collect();
        order.sort();

        let (mut pmin, mut pmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        let pos_off = bin.len();
        for (_, src) in &order {
            let v = &verts[*src as usize];
            let p = match &inv {
                Some(m) => transform_point(m, v.pos),
                None => v.pos,
            };
            for k in 0..3 {
                pmin[k] = pmin[k].min(p[k]);
                pmax[k] = pmax[k].max(p[k]);
                bin.extend_from_slice(&p[k].to_le_bytes());
            }
        }
        let nrm_off = bin.len();
        for (_, src) in &order {
            let v = &verts[*src as usize];
            let n = match &inv {
                Some(m) => transform_dir(m, v.normal),
                None => v.normal,
            };
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-8);
            for k in 0..3 {
                bin.extend_from_slice(&(n[k] / l).to_le_bytes());
            }
        }
        let uv_off = bin.len();
        for (_, src) in &order {
            let v = &verts[*src as usize];
            bin.extend_from_slice(&v.uv[0].to_le_bytes());
            bin.extend_from_slice(&v.uv[1].to_le_bytes());
        }
        let idx_off = bin.len();
        for i in &gi {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let nv = order.len();

        let mk_view = |views: &mut Vec<serde_json::Value>, off: usize, len: usize, target: u32| {
            views.push(serde_json::json!({"buffer":0,"byteOffset":off,"byteLength":len,"target":target}));
            views.len() - 1
        };
        let vp = mk_view(&mut views, pos_off, nv * 12, 34962);
        let vn = mk_view(&mut views, nrm_off, nv * 12, 34962);
        let vt = mk_view(&mut views, uv_off, nv * 8, 34962);
        let vi = mk_view(&mut views, idx_off, gi.len() * 4, 34963);
        let ap = accessors.len();
        accessors.push(serde_json::json!({"bufferView":vp,"componentType":5126,"count":nv,"type":"VEC3",
            "min":f32s(&pmin),"max":f32s(&pmax)}));
        let an = accessors.len();
        accessors.push(serde_json::json!({"bufferView":vn,"componentType":5126,"count":nv,"type":"VEC3"}));
        let at = accessors.len();
        accessors.push(serde_json::json!({"bufferView":vt,"componentType":5126,"count":nv,"type":"VEC2"}));
        let ai = accessors.len();
        accessors.push(serde_json::json!({"bufferView":vi,"componentType":5125,"count":gi.len(),"type":"SCALAR"}));

        let mat = d.diffuse.and_then(|h| mat_of_tex.get(&h).copied()).unwrap_or(fallback_mat);
        mesh_of_group.push((meshes.len(), d.node));
        meshes.push(serde_json::json!({
            "name": format!("group{}_seg{}_node{}", d.group_index, d.seg_id, d.node),
            "primitives": [{
                "attributes": {"POSITION":ap,"NORMAL":an,"TEXCOORD_0":at},
                "indices": ai, "material": mat
            }],
            // The engine bindings ride along so a re-import knows exactly how to rebuild this group.
            "extras": {
                "group_index": d.group_index, "sub_object": d.sub_object, "seg_id": d.seg_id,
                "segm_node": d.node, "lod_mask": d.lod_mask,
                "diffuse": d.diffuse.map(|h| format!("0x{h:08X}")),
                "specular": d.specular.map(|h| format!("0x{h:08X}")),
                "normal": d.normal.map(|h| format!("0x{h:08X}")),
            }
        }));
    }

    // glTF nodes = the HIER tree (local transforms preserved), meshes attached to their SEGM node.
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); hier.len()];
    let mut roots: Vec<usize> = Vec::new();
    for h in hier {
        match h.parent {
            Some(p) if p < hier.len() => children[p].push(h.index),
            _ => roots.push(h.index),
        }
    }
    // Meshes hang off their node; node<0 meshes become extra root nodes.
    let mut extra_roots: Vec<usize> = Vec::new();
    let mut mesh_children: Vec<Vec<usize>> = vec![Vec::new(); hier.len()];
    let node_base = hier.len();
    let mut mesh_nodes: Vec<serde_json::Value> = Vec::new();
    for (mi, node) in &mesh_of_group {
        let gni = node_base + mesh_nodes.len();
        mesh_nodes.push(serde_json::json!({ "name": format!("mesh{mi}"), "mesh": mi }));
        if *node >= 0 && (*node as usize) < hier.len() {
            mesh_children[*node as usize].push(gni);
        } else {
            extra_roots.push(gni);
        }
    }
    for h in hier {
        let mut kids: Vec<usize> = children[h.index].clone();
        kids.extend(mesh_children[h.index].iter().copied());
        let m = h.local;
        let mut n = serde_json::json!({
            "name": format!("node{}_0x{:08X}", h.index, h.hash),
            // glTF matrix is COLUMN-major; HierNode.local is row-major row-vector -> transpose.
            "matrix": f32s(&[
                m[0], m[4], m[8], m[12],
                m[1], m[5], m[9], m[13],
                m[2], m[6], m[10], m[14],
                m[3], m[7], m[11], m[15],
            ]),
            "extras": { "hier_index": h.index, "hier_hash": format!("0x{:08X}", h.hash) }
        });
        if !kids.is_empty() {
            n["children"] = serde_json::json!(kids);
        }
        nodes.push(n);
    }
    nodes.extend(mesh_nodes);
    let mut scene_roots = roots;
    scene_roots.extend(extra_roots);

    std::fs::write(outdir.join("model.bin"), &bin).map_err(|e| e.to_string())?;
    let gltf = serde_json::json!({
        "asset": {"version":"2.0","generator":format!("mercs2_workshop export_bundle ({label})")},
        "scene": 0,
        "scenes": [{"nodes": scene_roots}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "images": images,
        "textures": gtextures,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"uri":"model.bin","byteLength": bin.len()}]
    });
    std::fs::write(
        outdir.join("model.gltf"),
        serde_json::to_string_pretty(&gltf).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // ---- 4. Manifest: the reassembly map. ----
    let manifest = serde_json::json!({
        "name": label,
        "model_hash": format!("0x{hash:08X}"),
        "preservation": {
            "rule": "The .gltf is the EDITABLE view; raw/*.ucfx are the ORIGINAL container bytes. \
                     Chunks we do not (yet) decode — PHY2 Havok collision, CHDR/CEXE destruction \
                     scripts, SWIT/STAT/NODE, MTRL shader params, and any unlisted tag — are NOT \
                     represented in the glTF but ARE preserved byte-exact in raw/. Reassembly \
                     replaces only the geometry/material chunks and keeps the rest verbatim.",
            "editable_in_gltf": ["geometry","normals","uvs","HIER node tree","material->texture binding"],
            "preserved_only_in_raw": ["PHY2","CHDR","CEXE","SWIT","STAT","NODE","MTRL params","shader bindings","any undecoded tag"]
        },
        "header": header.map(|h| serde_json::json!({
            "aabb_min": f32s(&h.aabb_min), "aabb_max": f32s(&h.aabb_max),
            "node_count": h.node_count, "lod_count": h.lod_count, "lod_distance": h.lod_distance
        })),
        "lod_chain": rungs_json,
        "textures": tex_json,
        "hier_nodes": hier.iter().map(|h| serde_json::json!({
            "index": h.index, "hash": format!("0x{:08X}", h.hash), "parent": h.parent
        })).collect::<Vec<_>>(),
        // group -> sub_object -> INDX -> seg_id -> SEGM{node, lod_mask}: the binding chain
        // (docs/modernization/vehicle_model_spec.md §2). This is what a rebuild must reproduce.
        "draw_groups": draws.iter().map(|d| serde_json::json!({
            "group_index": d.group_index, "sub_object": d.sub_object, "seg_id": d.seg_id,
            "segm_node": d.node, "lod_mask": d.lod_mask, "triangles": d.index_count / 3,
            "diffuse": d.diffuse.map(|h| format!("0x{h:08X}")),
        })).collect::<Vec<_>>(),
    });
    std::fs::write(
        outdir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
