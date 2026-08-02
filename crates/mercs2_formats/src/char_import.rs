//! glTF/GLB → [`CharGlbData`] for the **skinned character** path.
//!
//! The counterpart to [`crate::mesh_import`], which reads a rigid prop. This one keeps everything
//! [`crate::char_skin`] needs to re-pose a character onto a Mercenaries rig: the whole node graph,
//! the skin's joints and inverse-bind matrices, per-vertex `JOINTS_0`/`WEIGHTS_0`, and the source
//! model's own sub-object partition.
//!
//! # Why it lives here and not in the Workshop
//!
//! It used to live in `mercs2_workshop::import`, with a second hand-rolled `serde_json` copy in
//! `mercs2_poc::gltf`. Both are binary crates, so **nothing headless could read a skinned `.glb`** —
//! which is exactly why `mercs2_quartermaster` rejected a manifest's `retarget:` as unsupported
//! rather than lowering it. A Shipment cannot ship a character until this is library code.
//!
//! Two readers of the same file format also drifted: only the Workshop's grew the rigid-part pass
//! below, so the CLI silently dropped eyes, teeth and equipment.

use std::collections::HashMap;
use std::path::Path;

use crate::char_skin::{CharGlbData, MeshPart};

type Mat4 = [[f32; 4]; 4];

const IDENT4: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// glTF's own convention: column-major storage, column-vector math, `out = a · b`.
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

/// Read a rigged `.glb`/`.gltf` into the glTF-free holder [`char_skin`](crate::char_skin) consumes.
///
/// Every matrix comes back **row-major** `f64`, which is what `char_skin` expects.
pub fn load_char_glb(path: &Path) -> Result<CharGlbData, String> {
    let (doc, buffers) = crate::mesh_import::open_gltf(path)?;
    let get = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d[..]);

    // ── node graph over ALL nodes ────────────────────────────────────────────────────────────
    let node_count = doc.nodes().count();
    let mut node_parent = vec![-1i32; node_count];
    let mut node_children = vec![Vec::new(); node_count];
    let mut node_name = vec![String::new(); node_count];
    for n in doc.nodes() {
        node_name[n.index()] = n.name().unwrap_or("").to_string();
        for c in n.children() {
            node_parent[c.index()] = n.index() as i32;
            node_children[n.index()].push(c.index());
        }
    }

    // Per-node world matrix (`world = world_parent · local`).
    let locals: Vec<Mat4> = {
        let mut v = vec![IDENT4; node_count];
        for n in doc.nodes() {
            v[n.index()] = n.transform().matrix();
        }
        v
    };
    let mut world_cm = vec![IDENT4; node_count];
    let mut done = vec![false; node_count];
    fn resolve(i: usize, parent: &[i32], local: &[Mat4], world: &mut [Mat4], done: &mut [bool]) {
        if done[i] {
            return;
        }
        let p = parent[i];
        world[i] = if p < 0 {
            local[i]
        } else {
            resolve(p as usize, parent, local, world, done);
            mat_mul(&world[p as usize], &local[i])
        };
        done[i] = true;
    }
    for i in 0..node_count {
        resolve(i, &node_parent, &locals, &mut world_cm, &mut done);
    }

    // column-major `[col][row]` → row-major, column-vector flat: `rm[r*4+c] = m[c][r]`.
    let cm_to_rm = |m: &Mat4| -> [f64; 16] {
        let mut f = [0.0f64; 16];
        for r in 0..4 {
            for c in 0..4 {
                f[r * 4 + c] = m[c][r] as f64;
            }
        }
        f
    };
    let node_world: Vec<[f64; 16]> = world_cm.iter().map(cm_to_rm).collect();

    // ── EVERY skin, unified ──────────────────────────────────────────────────────────────────
    //
    // A file may ship more than one. The CoD/Valve "Roze" rip binds its body to a 103-joint CoD rig
    // (skin 0) and its face+hair to an 11-joint ValveBiped rig (skin 1). `JOINTS_0` indexes the
    // PRIMITIVE'S OWN skin, so reading skin 0 and concatenating every primitive silently rebinds
    // skin 1's head onto skin 0's joints 0..10 — the root and spine. That is 35% of that model,
    // wrong, with no error.
    //
    // This was implemented once, in the `mercs2_poc::gltf` copy, and lost when that copy was
    // deleted as redundant: the drift ran BOTH ways, and only the direction that favoured keeping
    // this file was checked. Restored here, against the same rig it was written for.
    if doc.skins().len() == 0 {
        return Err("glb has no skin — not rigged".into());
    }
    let mut joint_nodes: Vec<usize> = Vec::new();
    let mut node_to_joint: HashMap<usize, usize> = HashMap::new();
    let mut ibm: Vec<Option<[f64; 16]>> = Vec::new();
    // skin index → (local joint index → unified joint index)
    let mut skin_local_to_unified: Vec<Vec<usize>> = Vec::new();
    for skin in doc.skins() {
        let ibms: Vec<Mat4> = skin
            .reader(get)
            .read_inverse_bind_matrices()
            .map(|it| it.collect())
            .unwrap_or_default();
        let mut map = Vec::new();
        for (local, j) in skin.joints().enumerate() {
            let node = j.index();
            let uni = *node_to_joint.entry(node).or_insert_with(|| {
                joint_nodes.push(node);
                ibm.push(None);
                joint_nodes.len() - 1
            });
            // First skin to define a joint's inverse-bind wins; a later skin sharing that node
            // keeps it rather than overwriting with its own bind pose.
            if ibm[uni].is_none() {
                if let Some(m) = ibms.get(local) {
                    ibm[uni] = Some(cm_to_rm(m));
                }
            }
            map.push(uni);
        }
        skin_local_to_unified.push(map);
    }

    // mesh index → the skin the node drawing it uses. A mesh drawn by no skinned node falls back to
    // skin 0, which is the previous single-skin behaviour.
    let mut mesh_skin: HashMap<usize, usize> = HashMap::new();
    for node in doc.nodes() {
        if let (Some(m), Some(s)) = (node.mesh(), node.skin()) {
            mesh_skin.insert(m.index(), s.index());
        }
    }
    if skin_local_to_unified.len() > 1 {
        eprintln!(
            "char_import: {} skins unified into {} joints — per-primitive JOINTS_0 remapped",
            skin_local_to_unified.len(),
            joint_nodes.len()
        );
    }

    // ── skinned primitives ───────────────────────────────────────────────────────────────────
    //
    // Merge ALL meshes/primitives: a character ships body, head and accessories as separate meshes,
    // so reading only mesh 0 drops the head. All primitives of a single-skin file share the joint
    // palette, making this a concat with an index offset.
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut vjoints: Vec<[u16; 4]> = Vec::new();
    let mut vweights: Vec<[f64; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // The source's own sub-object partition, one entry per primitive. This is the authoring unit
    // and it is how the game partitions a character too, so keeping it lets an import be authored
    // the way retail authors one: primitive → draw group, each with its own bone palette and
    // exactly one material.
    let mut parts: Vec<MeshPart> = Vec::new();

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let r = prim.reader(get);
            let (Some(pos), Some(joints), Some(weights)) =
                (r.read_positions(), r.read_joints(0), r.read_weights(0))
            else {
                continue; // not skinned → nothing for char_skin to re-pose
            };
            let base = positions.len() as u32;
            let ps: Vec<[f64; 3]> = pos.map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
            let m = ps.len();
            let nm: Vec<[f32; 3]> = r
                .read_normals()
                .map(|it| it.collect())
                .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; m]);
            let uv: Vec<[f32; 2]> = r
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; m]);
            // Remap this primitive's joint indices out of ITS OWN skin's local space and into the
            // unified list. For a single-skin file the map is the identity and this is a no-op.
            let smap = mesh_skin
                .get(&mesh.index())
                .and_then(|s| skin_local_to_unified.get(*s))
                .or_else(|| skin_local_to_unified.first());
            let jv: Vec<[u16; 4]> = joints
                .into_u16()
                .map(|j| match smap {
                    Some(map) => [
                        map.get(j[0] as usize).copied().unwrap_or(0) as u16,
                        map.get(j[1] as usize).copied().unwrap_or(0) as u16,
                        map.get(j[2] as usize).copied().unwrap_or(0) as u16,
                        map.get(j[3] as usize).copied().unwrap_or(0) as u16,
                    ],
                    None => j,
                })
                .collect();
            let wv: Vec<[f64; 4]> = weights
                .into_f32()
                .map(|w| [w[0] as f64, w[1] as f64, w[2] as f64, w[3] as f64])
                .collect();
            positions.extend(ps);
            normals.extend(nm);
            uvs.extend(uv);
            vjoints.extend(jv);
            vweights.extend(wv);
            let tri_start = indices.len() / 3;
            match r.read_indices() {
                Some(ind) => indices.extend(ind.into_u32().map(|i| base + i)),
                None => indices.extend(base..base + m as u32),
            }
            parts.push(MeshPart {
                name: mesh.name().unwrap_or("").to_string(),
                tri_start,
                tri_count: indices.len() / 3 - tri_start,
                material: prim.material().index(),
            });
        }
    }

    // ── rigid, bone-parented primitives ──────────────────────────────────────────────────────
    //
    // The eyes, teeth and equipment packs. Retail authors these as `MESH` sub-objects in their mount
    // node's LOCAL space rather than as `SKIN`, so they carry no JOINTS_0/WEIGHTS_0 and the loop
    // above skipped them: re-importing `pmc_hum_mattias` silently lost 603 verts across 6 parts —
    // both eyes, both eye reflections and both hip packs. That is precisely the equipment-and-face
    // system an authoring kit exists to expose, so dropping it is not an option.
    //
    // A rigid part mounted on a bone IS a single-bone skin: bake its node's world transform to put
    // the vertices in bind space, then bind them 100% to the nearest ancestor joint. At bind
    // `Skin_J == I` so they land exactly where authored, and under animation they ride that bone
    // rigidly — which is what the engine does with them anyway. Everything downstream then treats
    // them as ordinary geometry, with no special case.
    {
        let joint_of_node: HashMap<usize, u16> = joint_nodes
            .iter()
            .enumerate()
            .map(|(j, &n)| (n, j as u16))
            .collect();
        // Row-major, column-vector (the layout `cm_to_rm` produces): `p' = M · p`.
        let xform = |m: &[f64; 16], p: [f64; 3]| -> [f64; 3] {
            [
                m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
                m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
                m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
            ]
        };
        let xform_dir = |m: &[f64; 16], p: [f32; 3]| -> [f32; 3] {
            let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
            let v = [
                m[0] * x + m[1] * y + m[2] * z,
                m[4] * x + m[5] * y + m[6] * z,
                m[8] * x + m[9] * y + m[10] * z,
            ];
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            [(v[0] / n) as f32, (v[1] / n) as f32, (v[2] / n) as f32]
        };

        for node in doc.nodes() {
            let Some(mesh) = node.mesh() else { continue };
            if node.skin().is_some() {
                continue; // already handled as a real skin
            }
            // Walk up to the mount bone. A part whose chain reaches no joint has nothing to ride.
            let mut cur = node.index() as i32;
            let mut mount = None;
            while cur >= 0 {
                if let Some(&j) = joint_of_node.get(&(cur as usize)) {
                    mount = Some(j);
                    break;
                }
                cur = node_parent[cur as usize];
            }
            let Some(mount) = mount else { continue };
            let nw = node_world[node.index()];

            for prim in mesh.primitives() {
                let r = prim.reader(get);
                let Some(pos) = r.read_positions() else { continue };
                if prim.get(&gltf::Semantic::Joints(0)).is_some() {
                    continue; // skinned after all — the first loop owns it
                }
                let base = positions.len() as u32;
                let ps: Vec<[f64; 3]> = pos
                    .map(|p| xform(&nw, [p[0] as f64, p[1] as f64, p[2] as f64]))
                    .collect();
                let m = ps.len();
                let nm: Vec<[f32; 3]> = r
                    .read_normals()
                    .map(|it| it.map(|n| xform_dir(&nw, n)).collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; m]);
                let uv: Vec<[f32; 2]> = r
                    .read_tex_coords(0)
                    .map(|tc| tc.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; m]);
                positions.extend(ps);
                normals.extend(nm);
                uvs.extend(uv);
                vjoints.extend(std::iter::repeat([mount, 0, 0, 0]).take(m));
                vweights.extend(std::iter::repeat([1.0, 0.0, 0.0, 0.0]).take(m));
                let tri_start = indices.len() / 3;
                match r.read_indices() {
                    Some(ind) => indices.extend(ind.into_u32().map(|i| base + i)),
                    None => indices.extend(base..base + m as u32),
                }
                parts.push(MeshPart {
                    name: mesh.name().unwrap_or("").to_string(),
                    tri_start,
                    tri_count: indices.len() / 3 - tri_start,
                    material: prim.material().index(),
                });
            }
        }
    }

    if positions.is_empty() {
        return Err("glb has no skinned mesh primitive".into());
    }
    let tris: Vec<[u32; 3]> = indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();

    Ok(CharGlbData {
        positions,
        parts,
        normals,
        uvs,
        tris,
        indices,
        vjoints,
        vweights,
        joint_nodes,
        node_parent,
        node_name,
        node_children,
        node_world,
        ibm,
    })
}

/// One glTF material's embedded texture image bytes — the raw PNG/JPEG the file carries, ready to be
/// decoded and encoded to a game texture. `None` for a slot the material does not use (or one that
/// references an external file rather than an embedded image, which an authoring GLB never does).
#[derive(Default, Clone)]
pub struct MaterialTextures {
    pub diffuse: Option<Vec<u8>>,
    pub normal: Option<Vec<u8>>,
}

/// The embedded texture bytes of every glTF material, indexed by material index (the same index
/// [`MeshPart::material`] carries). This is what lets an imported character wear its OWN per-part
/// skins: each part names a material, and this hands back that material's diffuse + normal images.
///
/// Reads the GLB's own buffers (no image DECODE here — that is the texture encoder's job downstream),
/// so it needs only the `gltf` reader already used by [`load_char_glb`], not the `import` feature.
pub fn load_char_material_textures(path: &Path) -> Result<Vec<MaterialTextures>, String> {
    let (doc, buffers) = crate::mesh_import::open_gltf(path)?;
    // The raw bytes behind an image, when it is embedded in a buffer view (the GLB case).
    let bytes_of = |img: gltf::Image| -> Option<Vec<u8>> {
        match img.source() {
            gltf::image::Source::View { view, .. } => {
                let buf = buffers.get(view.buffer().index())?;
                let (off, len) = (view.offset(), view.length());
                buf.get(off..off + len).map(|s| s.to_vec())
            }
            gltf::image::Source::Uri { .. } => None,
        }
    };
    let mut out = vec![MaterialTextures::default(); doc.materials().count()];
    for m in doc.materials() {
        let Some(idx) = m.index() else { continue }; // the default material has no index / no part
        if idx >= out.len() {
            continue;
        }
        out[idx] = MaterialTextures {
            diffuse: m
                .pbr_metallic_roughness()
                .base_color_texture()
                .and_then(|t| bytes_of(t.texture().source())),
            normal: m.normal_texture().and_then(|t| bytes_of(t.texture().source())),
        };
    }
    Ok(out)
}
