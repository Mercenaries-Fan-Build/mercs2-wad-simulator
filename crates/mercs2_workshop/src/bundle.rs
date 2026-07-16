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

use mercs2_engine::mesh::{BoneRig, DrawGroup};
use mercs2_engine::render::{ClipAnim, TexMap};
use mercs2_formats::skeleton::{affine_inverse, transform_dir, transform_point, Skeleton};

/// An engine matrix (row-major storage, ROW-vector convention: `p' = p · M`) as the 16 floats glTF
/// wants (column-major storage, COLUMN-vector convention: `p' = M · p`).
///
/// The two conventions are transposes of each other, so `M_gltf = M_engine^T`. Column-major storage
/// of `M^T` is byte-identical to row-major storage of `M`, so the engine matrix is emitted VERBATIM,
/// in its own row-major order. Transposing here instead would land the translation in the bottom row,
/// which glTF reads as a non-affine matrix (`ACCESSOR_INVALID_IBM` / `NODE_MATRIX_NON_TRS`).
fn gltf_mat(m: &[[f32; 4]; 4]) -> Vec<f32> {
    let mut o = Vec::with_capacity(16);
    for r in 0..4 {
        for c in 0..4 {
            o.push(m[r][c]);
        }
    }
    o
}

/// Near-equality for eliding an animation channel that never moves.
fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-6)
}

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
    // Per-bone rig (parent / inverse-bind / bind-local). Empty for a model with no skeleton.
    rig: &[BoneRig],
    // Clips that bind to this model's rig. Exported as glTF animations; empty for a static asset.
    clips: &[ClipAnim],
    // Catalog hash -> name, so an animation is `walk_forward` and not `clip_0x4A4E244E`.
    names: &std::collections::HashMap<u32, String>,
) -> Result<(), String> {
    std::fs::create_dir_all(outdir.join("raw")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(outdir.join("textures")).map_err(|e| e.to_string())?;

    // The two kinds of geometry are handled PER GROUP, not per model, because real assets mix them:
    // Mattias is 29 deforming groups plus 12 rigid accessories; the ZTZ98 tank is 136 rigid panels
    // plus 2 deforming ones (1.8% of its triangles).
    //
    //   * DEFORMING (`d.skinned`) — vertices carry BLENDINDICES/BLENDWEIGHT and are shared across
    //     many bones. They cannot be baked into any one node's space; they need the `skin` +
    //     inverse-bind matrices. (Baking them was a real defect: the tank's 2 deforming groups were
    //     being divided by a single bone's world-rest, which is not what drives them.)
    //   * RIGID — the mesh builder already baked these into their bone's bind space. Un-bake and
    //     parent them under that HIER node: the node tree stays real (move the turret node, the
    //     turret moves) and, because a glTF child follows its parent joint, they still ride the
    //     animation exactly as the engine's 100%-weight-to-one-bone palette does.
    //
    // A skin is emitted whenever ANY group deforms; the rigid groups keep their node parenting.
    let has_skin = !rig.is_empty() && draws.iter().any(|d| d.skinned);
    // An animated node must use TRS, not `matrix`. A purely rigid, clipless asset keeps the matrix
    // form the vehicle path has always emitted.
    let use_trs = has_skin || !clips.is_empty();

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
        // Write at the DECODED size. `td.width`/`td.height` are what the texture declares; if its
        // higher mips were never assembled, the bytes only cover a smaller surface and writing at
        // the declared size produces a near-empty plate. `decode_bc` reports what it actually built.
        let (w, h_px, rgba) = crate::texpng::decode_bc(td);
        crate::texpng::write_png(outdir.join(&file).to_str().ok_or("bad path")?, w, h_px, &rgba)?;
        tex_json.push(serde_json::json!({
            "hash": format!("0x{h:08X}"), "file": file, "width": w, "height": h_px,
            // Flagged when the PNG is coarser than the texture's real size — i.e. the mip chain did
            // not fully assemble. A re-import must not treat this as the authored resolution.
            "declared_width": td.width, "declared_height": td.height,
            "full_resolution": w == td.width && h_px == td.height,
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
    // One glTF texture per source hash, shared by every material that binds it.
    let mut tex_index: BTreeMap<u32, usize> = BTreeMap::new();
    for (h, file) in &tex_file {
        let img = images.len();
        images.push(serde_json::json!({ "uri": file }));
        let t = gtextures.len();
        gtextures.push(serde_json::json!({ "source": img }));
        tex_index.insert(*h, t);
    }

    // A material per distinct (diffuse, normal, specular) SLOT SET — that triple is what the MTRL
    // record actually is. Keying materials on the diffuse alone bound only the albedo and silently
    // dropped every normal and specular map, so an exported asset lost half its surface detail.
    // glTF core has no specular-map slot under metallic-roughness, so that hash rides in `extras`
    // (its PNG is still written) rather than being quietly discarded.
    let mut mat_of_slots: BTreeMap<(Option<u32>, Option<u32>, Option<u32>), usize> = BTreeMap::new();
    let hx = |h: Option<u32>| h.map(|v| format!("0x{v:08X}"));
    for d in draws.iter() {
        let key = (d.diffuse, d.normal, d.specular);
        if mat_of_slots.contains_key(&key) {
            continue;
        }
        let mut m = serde_json::json!({
            "name": format!("mat_d{}_n{}_s{}",
                hx(d.diffuse).unwrap_or_else(|| "none".into()),
                hx(d.normal).unwrap_or_else(|| "none".into()),
                hx(d.specular).unwrap_or_else(|| "none".into())),
            "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 0.9 },
            "doubleSided": true,
            "extras": { "diffuse": hx(d.diffuse), "normal": hx(d.normal), "specular": hx(d.specular) }
        });
        if let Some(t) = d.diffuse.and_then(|h| tex_index.get(&h)) {
            m["pbrMetallicRoughness"]["baseColorTexture"] = serde_json::json!({ "index": t });
        }
        if let Some(t) = d.normal.and_then(|h| tex_index.get(&h)) {
            m["normalTexture"] = serde_json::json!({ "index": t });
        }
        mat_of_slots.insert(key, materials.len());
        materials.push(m);
    }
    let fallback_mat = materials.len();
    materials.push(serde_json::json!({
        "name": "untextured",
        "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 0.9 },
        "doubleSided": true
    }));

    // One glTF mesh per draw group, geometry pushed back into its node's local space.
    // (mesh index, SEGM node, LOD rung, deforming?)
    let mut mesh_of_group: Vec<(usize, i16, u8, bool)> = Vec::new();
    for d in draws.iter() {
        let s = d.index_start as usize;
        let e = ((d.index_start + d.index_count) as usize).min(indices.len());
        if s >= e {
            continue;
        }
        // Rigid: un-bake from its bone's world-rest so it can be re-parented under that node.
        // Deforming: leave it in bind space — the inverse-bind matrices and the per-vertex weights
        // are what move it, and pre-transforming by one bone would double-apply that bone.
        let inv = if d.skinned || d.node < 0 {
            None
        } else {
            skel.as_ref().and_then(|sk| sk.bones.get(d.node as usize)).map(|b| affine_inverse(&b.world))
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
        // Skin binding: BLENDINDICES verbatim (they already index the HIER, which IS the glTF joint
        // list), and BLENDWEIGHT renormalized to floats summing to 1 as glTF requires.
        let (jnt_off, wgt_off) = (bin.len(), 0usize);
        let mut wgt_off = wgt_off;
        if d.skinned {
            for (_, src) in &order {
                let v = &verts[*src as usize];
                // A slot with zero weight contributes nothing, but glTF still requires its joint
                // index to be 0 — a stale index beside a zero weight is what the engine leaves in
                // the unused slots, and every such vertex trips ACCESSOR_JOINTS_USED_ZERO_WEIGHT.
                let mut j = v.joints;
                for k in 0..4 {
                    if v.weights[k] == 0 {
                        j[k] = 0;
                    }
                }
                bin.extend_from_slice(&j);
            }
            wgt_off = bin.len();
            for (_, src) in &order {
                let w = verts[*src as usize].weights;
                let sum: f32 = w.iter().map(|&x| x as f32).sum();
                let inv_sum = if sum > 0.0 { 1.0 / sum } else { 0.0 };
                for k in 0..4 {
                    bin.extend_from_slice(&(w[k] as f32 * inv_sum).to_le_bytes());
                }
            }
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

        let mut attrs = serde_json::json!({"POSITION":ap,"NORMAL":an,"TEXCOORD_0":at});
        if d.skinned {
            let vj = mk_view(&mut views, jnt_off, nv * 4, 34962);
            let vw = mk_view(&mut views, wgt_off, nv * 16, 34962);
            let aj = accessors.len();
            // 5121 = UNSIGNED_BYTE: BLENDINDICES are HIER bone indices (<= 255 bones).
            accessors.push(serde_json::json!({"bufferView":vj,"componentType":5121,"count":nv,"type":"VEC4"}));
            let aw = accessors.len();
            accessors.push(serde_json::json!({"bufferView":vw,"componentType":5126,"count":nv,"type":"VEC4"}));
            attrs["JOINTS_0"] = serde_json::json!(aj);
            attrs["WEIGHTS_0"] = serde_json::json!(aw);
        }

        let mat = mat_of_slots.get(&(d.diffuse, d.normal, d.specular)).copied().unwrap_or(fallback_mat);
        mesh_of_group.push((meshes.len(), d.node, d.rung, d.skinned));
        meshes.push(serde_json::json!({
            "name": format!("LOD{}_group{}_seg{}_node{}", d.rung, d.group_index, d.seg_id, d.node),
            "primitives": [{
                "attributes": attrs,
                "indices": ai, "material": mat
            }],
            // The engine bindings ride along so a re-import knows exactly how to rebuild this group.
            "extras": {
                "group_index": d.group_index, "sub_object": d.sub_object, "seg_id": d.seg_id,
                "segm_node": d.node, "lod_mask": d.lod_mask,
                // Which LOD block this geometry belongs to. A rebuild must write it back to THAT
                // rung's container — geometry from `_P002_Q1` does not belong in the resident block.
                "lod_rung": d.rung,
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
    //
    // But a node is re-authored by EVERY rung that covers it — the resident block's coarse version of
    // the van body and P001's fine version are both children of node 3, in the same space. Parenting
    // them directly stacks the detail levels on top of each other, which is what made exports look
    // wrong. Nothing may be dropped (the finer rungs are the mod content, the coarse ones are the
    // shipped far-LOD), so instead each node gets one identity-transform LOD group per rung it has
    // geometry in — `node3_LOD0`, `node3_LOD2` — and the meshes hang off those. The tree then reads
    // as "this bone, at this detail level", and a modeller can isolate or hide a whole rung.
    //
    // A DEFORMING group is different: it is driven by the skin, not by a parent bone, so it must NOT
    // hang off the HIER tree (glTF ignores a skinned mesh node's own transform, and parenting it to
    // an animated joint reads as a double transform in some importers). Those sit at the scene root,
    // grouped by rung. Rigid groups keep the per-node LOD parents.
    let node_base = hier.len();
    let mut mesh_nodes: Vec<serde_json::Value> = Vec::new();
    let mut by_node_rung: BTreeMap<(usize, u8), Vec<usize>> = BTreeMap::new();
    let mut skinned_by_rung: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
    let mut unbound_by_rung: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
    for (mi, node, rung, is_skinned) in &mesh_of_group {
        let gni = node_base + mesh_nodes.len();
        let mut mn = serde_json::json!({
            "name": format!("LOD{rung}_mesh{mi}"), "mesh": mi,
            "extras": { "lod_rung": rung, "deforming": is_skinned }
        });
        if *is_skinned {
            mn["skin"] = serde_json::json!(0);
        }
        mesh_nodes.push(mn);
        if *is_skinned {
            skinned_by_rung.entry(*rung).or_default().push(gni);
        } else if *node >= 0 && (*node as usize) < hier.len() {
            by_node_rung.entry((*node as usize, *rung)).or_default().push(gni);
        } else {
            unbound_by_rung.entry(*rung).or_default().push(gni);
        }
    }

    // LOD group nodes are appended AFTER the mesh nodes, so `nodes[0..hier.len()]` stays aligned with
    // the HIER index — a re-import keeps reading node N as HIER node N.
    let group_base = node_base + mesh_nodes.len();
    let mut lod_groups: Vec<serde_json::Value> = Vec::new();
    let mut extra_roots: Vec<usize> = Vec::new();
    let mut mesh_children: Vec<Vec<usize>> = vec![Vec::new(); hier.len()];
    for ((node, rung), kids) in &by_node_rung {
        let gi = group_base + lod_groups.len();
        lod_groups.push(serde_json::json!({
            "name": format!("node{node}_LOD{rung}"), "children": kids,
            "extras": { "lod_rung": rung, "hier_index": node, "lod_group": true }
        }));
        mesh_children[*node].push(gi);
    }
    for (rung, kids) in &skinned_by_rung {
        let gi = group_base + lod_groups.len();
        lod_groups.push(serde_json::json!({
            "name": format!("skin_LOD{rung}"), "children": kids,
            "extras": { "lod_rung": rung, "lod_group": true, "deforming": true }
        }));
        extra_roots.push(gi);
    }
    for (rung, kids) in &unbound_by_rung {
        let gi = group_base + lod_groups.len();
        lod_groups.push(serde_json::json!({
            "name": format!("unbound_LOD{rung}"), "children": kids,
            "extras": { "lod_rung": rung, "lod_group": true }
        }));
        extra_roots.push(gi);
    }

    // The bind-pose LOCAL transform of each bone, as TRS. An animated node MUST use TRS — glTF
    // forbids animating a node that declares a `matrix` — and a bone with no track in a given clip
    // keeps exactly this, which is what makes an un-animated skeleton recompose to the bind pose.
    let bind_trs: Vec<mercs2_formats::anim::QsTransform> =
        rig.iter().map(|b| mercs2_engine::pose::mat_to_qs(&b.local_bind)).collect();

    for h in hier {
        let mut kids: Vec<usize> = children[h.index].clone();
        kids.extend(mesh_children[h.index].iter().copied());
        let m = h.local;
        let mut n = serde_json::json!({
            "name": format!("node{}_0x{:08X}", h.index, h.hash),
            "extras": { "hier_index": h.index, "hier_hash": format!("0x{:08X}", h.hash) }
        });
        match bind_trs.get(h.index) {
            // Skinned: TRS, so animation channels can target this joint.
            Some(qs) if use_trs => {
                n["translation"] = serde_json::json!(f32s(&qs.translation));
                n["rotation"] = serde_json::json!(f32s(&qs.rotation));
                n["scale"] = serde_json::json!(f32s(&qs.scale));
            }
            // Rigid: keep the matrix form the vehicle path has always used.
            // glTF stores column-major COLUMN-vector; HierNode.local is row-major ROW-vector. Those
            // are transposes, and column-major storage of the transpose IS the row-major original —
            // so `local` goes out verbatim. See `gltf_mat`.
            _ => {
                n["matrix"] = serde_json::json!(f32s(&m));
            }
        }
        if !kids.is_empty() {
            n["children"] = serde_json::json!(kids);
        }
        nodes.push(n);
    }
    nodes.extend(mesh_nodes);
    nodes.extend(lod_groups);
    let mut scene_roots = roots.clone();
    scene_roots.extend(extra_roots);

    // ---- 3b. Skin: the inverse-bind matrices that turn the HIER into a deformer. ----
    let mut skins_json: Vec<serde_json::Value> = Vec::new();
    if has_skin {
        let ibm_off = bin.len();
        for b in rig {
            for f in gltf_mat(&b.inv_bind) {
                bin.extend_from_slice(&f.to_le_bytes());
            }
        }
        let vb = views.len();
        views.push(serde_json::json!({
            "buffer": 0, "byteOffset": ibm_off, "byteLength": rig.len() * 64
        }));
        let ab = accessors.len();
        accessors.push(serde_json::json!({
            "bufferView": vb, "componentType": 5126, "count": rig.len(), "type": "MAT4"
        }));
        skins_json.push(serde_json::json!({
            "name": format!("{label}_skin"),
            // Joint j of the skin is HIER node j — BLENDINDICES index the HIER directly, so the
            // joint list is the identity mapping and JOINTS_0 needs no remap.
            "joints": (0..rig.len()).collect::<Vec<_>>(),
            "inverseBindMatrices": ab,
            "skeleton": roots.first().copied().unwrap_or(0),
        }));
    }

    // ---- 3c. Animations: one glTF animation per clip that binds to this rig. ----
    //
    // The clip is sampled at its own frame rate and each transform track is written to the HIER bone
    // its binding names (`ClipAnim::track_to_hier`). A channel whose value never changes AND equals
    // the bone's bind-local is dropped — the node's default TRS already says it, and a character's
    // bones overwhelmingly only rotate, so this is most of the file size.
    let mut animations: Vec<serde_json::Value> = Vec::new();
    let mut clips_json: Vec<serde_json::Value> = Vec::new();
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in clips {
        let frames = c.clip.num_frames.max(1);
        let dur = c.clip.duration.max(1e-4);
        let times: Vec<f32> = (0..frames)
            .map(|f| if frames > 1 { f as f32 * dur / (frames - 1) as f32 } else { 0.0 })
            .collect();

        // Sample the whole clip once: [frame][track] local TRS.
        let sampled: Vec<Vec<mercs2_formats::anim::QsTransform>> =
            times.iter().map(|&t| c.clip.sample_local(t)).collect();

        // time accessor, shared by every channel in this clip.
        let t_off = bin.len();
        for t in &times {
            bin.extend_from_slice(&t.to_le_bytes());
        }
        let tv = views.len();
        views.push(serde_json::json!({"buffer":0,"byteOffset":t_off,"byteLength":times.len()*4}));
        let ta = accessors.len();
        accessors.push(serde_json::json!({
            "bufferView": tv, "componentType": 5126, "count": times.len(), "type": "SCALAR",
            "min": [times.first().copied().unwrap_or(0.0)],
            "max": [times.last().copied().unwrap_or(0.0)]
        }));

        let mut samplers: Vec<serde_json::Value> = Vec::new();
        let mut channels: Vec<serde_json::Value> = Vec::new();
        let mut bones_driven = 0u32;

        for track in 0..c.num_transform_tracks.min(c.track_to_hier.len()) {
            let Some(bone) = c.track_to_hier[track] else { continue };
            if bone >= rig.len() {
                continue;
            }
            let bind = &bind_trs[bone];
            let mut driven = false;

            // (path, component count, per-frame value, the bind value it would default to)
            for (path, n, get, def) in [
                ("translation", 3usize,
                 (|q: &mercs2_formats::anim::QsTransform| q.translation.to_vec()) as fn(&mercs2_formats::anim::QsTransform) -> Vec<f32>,
                 bind.translation.to_vec()),
                ("rotation", 4,
                 (|q: &mercs2_formats::anim::QsTransform| q.rotation.to_vec()) as fn(&mercs2_formats::anim::QsTransform) -> Vec<f32>,
                 bind.rotation.to_vec()),
                ("scale", 3,
                 (|q: &mercs2_formats::anim::QsTransform| q.scale.to_vec()) as fn(&mercs2_formats::anim::QsTransform) -> Vec<f32>,
                 bind.scale.to_vec()),
            ] {
                let mut vals: Vec<Vec<f32>> = sampled
                    .iter()
                    .map(|fr| fr.get(track).map(&get).unwrap_or_else(|| def.clone()))
                    .collect();
                // glTF REQUIRES a rotation to be a unit quaternion; a non-unit one is not a rotation
                // at all and the validator rejects the clip. A few decoded tracks come off the wire
                // slightly (occasionally badly) off unit — the engine's own `slerp` documents that it
                // assumes unit inputs, and at an exact keyframe it passes the raw frame straight
                // through, so nothing upstream ever renormalizes them. Do it here, at the one place
                // the value has to be legal.
                if path == "rotation" {
                    for v in vals.iter_mut() {
                        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if n > 1e-6 {
                            for x in v.iter_mut() {
                                *x /= n;
                            }
                        }
                    }
                }
                // Constant and equal to the bind default -> the node already says it.
                if vals.iter().all(|v| close(v, &def)) {
                    continue;
                }
                let off = bin.len();
                for v in &vals {
                    for x in v {
                        bin.extend_from_slice(&x.to_le_bytes());
                    }
                }
                let vw = views.len();
                views.push(serde_json::json!({
                    "buffer": 0, "byteOffset": off, "byteLength": vals.len() * n * 4
                }));
                let av = accessors.len();
                accessors.push(serde_json::json!({
                    "bufferView": vw, "componentType": 5126, "count": vals.len(),
                    "type": if n == 4 { "VEC4" } else { "VEC3" }
                }));
                let si = samplers.len();
                samplers.push(serde_json::json!({
                    "input": ta, "output": av, "interpolation": "LINEAR"
                }));
                channels.push(serde_json::json!({
                    "sampler": si, "target": { "node": bone, "path": path }
                }));
                driven = true;
            }
            if driven {
                bones_driven += 1;
            }
        }

        // A glTF animation name is what a modder picks from a dropdown, so use the clip's real name
        // where the catalog knows it. Names are unique-ified: two lookup rows can share a name.
        let base = names.get(&c.name_hash).cloned().unwrap_or_else(|| format!("clip_0x{:08X}", c.name_hash));
        let mut name = base.clone();
        let mut n = 2;
        while used_names.contains(&name) {
            name = format!("{base}.{n}");
            n += 1;
        }
        used_names.insert(name.clone());
        // A clip is written ONLY if it actually drives THIS rig. It can fail to in two ways:
        //   * it did not decode (it would sample to the NEUTRAL pose, not the bind pose), or
        //   * none of its transform tracks bind to this model's HIER — the character's lookup table
        //     names the clip, but it was authored against a different skeleton.
        // Either would ship an animation that snaps the character into a dead T-pose. Such a clip is
        // RECORDED here with the reason rather than written, or silently dropped.
        let exported = c.clip.decoded && !channels.is_empty();
        if exported {
            animations.push(serde_json::json!({
                "name": name, "samplers": samplers, "channels": channels
            }));
        }
        let bound_tracks = c
            .track_to_hier
            .iter()
            .take(c.num_transform_tracks)
            .filter(|t| t.is_some())
            .count();
        clips_json.push(serde_json::json!({
            "name": name,
            "hash": format!("0x{:08X}", c.name_hash),
            "named": names.contains_key(&c.name_hash),
            "exported": exported,
            "skipped_reason": if exported {
                serde_json::Value::Null
            } else if !c.clip.decoded {
                serde_json::json!("clip frames did not decode")
            } else {
                serde_json::json!(format!(
                    "0 of {} transform tracks bind to this model's HIER — the clip is authored \
                     against a different skeleton; its bytes are still in the WAD",
                    c.num_transform_tracks
                ))
            },
            "bound_tracks": bound_tracks,
            "duration": c.clip.duration,
            "frames": c.clip.num_frames,
            "transform_tracks": c.num_transform_tracks,
            "bones_driven": bones_driven,
            // FALSE = the clip's frames could not be decoded and it would sample to the neutral
            // pose. Such a clip is reported here and NOT written as an animation, rather than
            // shipped as a silently-dead T-pose.
            "decoded": c.clip.decoded,
        }));
    }

    std::fs::write(outdir.join("model.bin"), &bin).map_err(|e| e.to_string())?;
    let mut gltf = serde_json::json!({
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
    if !skins_json.is_empty() {
        gltf["skins"] = serde_json::json!(skins_json);
    }
    if !animations.is_empty() {
        gltf["animations"] = serde_json::json!(animations);
    }
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
            "preserved_only_in_raw": ["PHY2","CHDR","CEXE","SWIT","STAT","NODE","MTRL params","shader bindings","any undecoded tag"],
            "lod_rungs": "EVERY rung is exported, none is dropped. The rungs REFINE each other rather \
                          than summing: the resident block (LOD0) is a complete low-detail model, and \
                          each finer `_P00N_` block RE-AUTHORS some of those same HIER nodes at a \
                          higher detail. They therefore occupy the SAME space. In the glTF each node's \
                          meshes are grouped under `node<N>_LOD<rung>` parents (identity transform) so \
                          the detail levels are separable instead of stacked; the OBJ does the same \
                          with one `o LOD<rung>` object per rung. Geometry edited under a given rung \
                          must be written back to THAT rung's container — `lod_rung` on each draw \
                          group and mesh says which. `lod_mask` is the engine's tier gate and already \
                          carries the supersede resolution (a bit cleared = a finer rung owns that \
                          tier); do not recompute it from scratch."
        },
        "header": header.map(|h| serde_json::json!({
            "aabb_min": f32s(&h.aabb_min), "aabb_max": f32s(&h.aabb_max),
            "node_count": h.node_count, "lod_count": h.lod_count, "lod_distance": h.lod_distance
        })),
        "lod_chain": rungs_json,
        "textures": tex_json,
        // A skinned export binds geometry through `skin` + inverse-bind matrices (a character);
        // a rigid one bakes each group into its HIER node and parents it there (a vehicle).
        "skinned": has_skin,
        "bones": rig.len(),
        "clips": clips_json,
        "hier_nodes": hier.iter().map(|h| serde_json::json!({
            "index": h.index, "hash": format!("0x{:08X}", h.hash), "parent": h.parent
        })).collect::<Vec<_>>(),
        // group -> sub_object -> INDX -> seg_id -> SEGM{node, lod_mask}: the binding chain
        // (docs/modernization/vehicle_model_spec.md §2). This is what a rebuild must reproduce.
        "draw_groups": draws.iter().map(|d| serde_json::json!({
            "group_index": d.group_index, "sub_object": d.sub_object, "seg_id": d.seg_id,
            "segm_node": d.node, "lod_mask": d.lod_mask, "triangles": d.index_count / 3,
            // Which LOD block this group's geometry came from (0 = resident). The `lod_chain` entry
            // at this level is the container it must be written back into.
            "lod_rung": d.rung,
            "diffuse": d.diffuse.map(|h| format!("0x{h:08X}")),
            "normal": d.normal.map(|h| format!("0x{h:08X}")),
            "specular": d.specular.map(|h| format!("0x{h:08X}")),
        })).collect::<Vec<_>>(),
    });
    std::fs::write(
        outdir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read 16 emitted floats the way glTF does — column-major — and return `[row][col]`.
    fn as_gltf(v: &[f32]) -> [[f32; 4]; 4] {
        let mut m = [[0.0f32; 4]; 4];
        for (k, x) in v.iter().enumerate() {
            m[k % 4][k / 4] = *x;
        }
        m
    }

    /// The convention boundary this file sits on: the engine is row-major/ROW-vector, glTF is
    /// column-major/COLUMN-vector. Those are transposes, so the engine's own row-major bytes are
    /// ALREADY glTF's column-major bytes and must go out verbatim. Transposing instead lands the
    /// translation in the bottom row, which is not an affine transform — the Khronos validator
    /// rejects it (`ACCESSOR_INVALID_IBM`, `NODE_MATRIX_NON_TRS`) and importers either refuse the
    /// file or silently drop every node offset. `anim_export_parity` guards the TRS/animation path;
    /// this guards the matrix path (inverse-bind matrices and rigid `node.matrix`).
    #[test]
    fn emitted_matrix_is_affine_with_translation_in_the_last_column() {
        // A row-vector translate-by-(1,2,3): translation occupies the last ROW engine-side.
        let engine = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [1.0, 2.0, 3.0, 1.0],
        ];
        let g = as_gltf(&gltf_mat(&engine));

        // glTF-side it must read as an affine matrix: bottom row exactly [0,0,0,1] ...
        assert_eq!(g[3], [0.0, 0.0, 0.0, 1.0], "bottom row must be [0,0,0,1]");
        // ... with the translation moved into the last COLUMN.
        assert_eq!([g[0][3], g[1][3], g[2][3]], [1.0, 2.0, 3.0], "translation must be the last column");
    }

    /// The same invariant for a matrix that also rotates: a wrong transpose inverts the rotation
    /// (R^T = R^-1 for a rotation), which a translation-only fixture cannot see.
    #[test]
    fn emitted_matrix_preserves_rotation_handedness() {
        // Row-vector 90° about +Z, then translate: p' = p · M.
        let engine = [
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [5.0, 0.0, 0.0, 1.0],
        ];
        let g = as_gltf(&gltf_mat(&engine));
        assert_eq!(g[3], [0.0, 0.0, 0.0, 1.0]);

        // Engine: x-axis (1,0,0) · M = (0,1,0) + translation. glTF must agree via M · p.
        let p = [1.0f32, 0.0, 0.0, 1.0];
        let got = [
            (0..4).map(|k| g[0][k] * p[k]).sum::<f32>(),
            (0..4).map(|k| g[1][k] * p[k]).sum::<f32>(),
            (0..4).map(|k| g[2][k] * p[k]).sum::<f32>(),
        ];
        assert_eq!(got, [5.0, 1.0, 0.0], "glTF M·p must reproduce the engine's p·M");
    }
}
