//! Foreign-model import: `.obj` / `.gltf` / `.glb` → the same in-memory `ModelData` a WAD model
//! loads into, so an imported mesh previews on the engine renderer like any game asset.
//!
//! Conventions: the engine's asset space is right-handed, +Y up, metres (see the handedness note
//! in `scene.rs`) — glTF matches directly, OBJ is taken verbatim. Imported textures are
//! BC-compressed on the fly (`texenc`) under synthetic `m2(<file>#<n>)` hashes.
//!
//! The glTF path also reads the SOURCE RIG when the file carries one: the first skin's joint node
//! names (`skin_joints`) plus per-vertex `JOINTS_0`/`WEIGHTS_0`. This is what the Skeleton workbench
//! retargets — a ValveBiped/Mixamo/Unreal skin's bones mapped onto a Mercs2 HIER skeleton. For the
//! static preview a skinned mesh is baked to its BIND POSE (`Σ w·(jointWorld·IBM)·POSITION`) — the
//! same transform a correct glTF viewer applies, which carries a Z-up rig's `Z_UP` root correction
//! so foreign characters preview upright, not face-down. The joints/weights ride alongside for anim.

use std::collections::HashMap;
use std::path::Path;

use mercs2_engine::mesh::{BoneRig, DrawGroup, ModelStats, SkinData, Vertex};
use mercs2_engine::render::TexMap;
use mercs2_formats::char_skin::{expand_ranges, CharGlbData, CharSkin, MeshPart};
use mercs2_formats::hash::pandemic_hash_m2;

/// An imported model, mirroring the WAD loader's output.
pub struct Imported {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<DrawGroup>,
    pub textures: TexMap,
    pub stats: ModelStats,
    pub skin: SkinData,
    /// Source-rig joint node names (first glTF skin), in palette order — the input the Skeleton
    /// workbench's retarget classifier keys on. Empty for OBJ / unrigged glTF. When non-empty, the
    /// per-vertex `Vertex.joints` index INTO this list.
    pub skin_joints: Vec<String>,
    /// Source bind-pose bone positions (skin space), index-aligned to `skin_joints`. Derived from the
    /// glTF skin's inverse-bind matrices; empty for OBJ / unrigged glTF. Feeds the spatial retarget.
    pub skin_joint_pos: Vec<[f32; 3]>,
    /// Source inverse-bind matrices (glTF column-major), index-aligned to `skin_joints`. The retarget
    /// rebind re-anchors each vertex from its source bone's bind space into the target bone's, so the
    /// target skeleton's animation deforms the foreign mesh correctly. Empty for OBJ / unrigged glTF.
    pub skin_ibm: Vec<[[f32; 4]; 4]>,
    /// Parent JOINT index for each source joint (`-1` = root), index-aligned to `skin_joints` — the
    /// hierarchy the Skeleton workbench draws as a tree. Derived from the glTF node parent chain.
    pub skin_parents: Vec<i32>,
}

/// Build an [`Imported`] from a faithful [`CharSkin`] result so the Skeleton-workbench PREVIEW
/// renders EXACTLY what the shipped/injected character looks like — the same data
/// `inject_character_into_donor_block` writes, but with the palette-relative BLENDINDICES
/// **expanded back to GLOBAL** (the engine/GPU skins with global indices; only the WAD reader
/// `model_cubeize` expands the INFO(56) range table at load, and imports never pass through it).
///
/// The mesh is `char_skin`'s re-posed geometry (mesh0/prim0), skinned to `target_rig` (the target
/// character's HIER BoneRig, in HIER order). Since the mesh is conformed into the target's bind
/// space, the target's own animation clips drive it directly — no cross-skeleton retarget.
pub fn char_skin_to_imported(cs: &CharSkin, glb: &CharGlbData, target_rig: Vec<BoneRig>) -> Imported {
    let palette = expand_ranges(&cs.ranges); // slot -> global HIER (the reader's expansion)
    let nv = cs.pos.len();
    let mut verts = Vec::with_capacity(nv);
    let mut max_joint = 0u32;
    for i in 0..nv {
        let mut joints = [0u8; 4];
        let mut weights = [0u8; 4];
        for k in 0..4 {
            let slot = cs.skin_bytes[i * 8 + k] as usize;
            let g = palette.get(slot).copied().unwrap_or(0);
            joints[k] = g.min(255) as u8;
            weights[k] = cs.skin_bytes[i * 8 + 4 + k];
            if weights[k] > 0 {
                max_joint = max_joint.max(g as u32);
            }
        }
        verts.push(Vertex {
            pos: cs.pos[i],
            color: [1.0, 1.0, 1.0],
            uv: glb.uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            // CONFORMED normal. The preview renders `cs.pos`, so it must light them with the
            // normals of THAT surface; the source glTF's field belongs to the pre-conform mesh.
            normal: cs
                .nrm
                .get(i)
                .copied()
                .or_else(|| glb.normals.get(i).copied())
                .unwrap_or([0.0, 0.0, 1.0]),
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints,
            weights,
        });
    }
    let indices = glb.indices.clone();
    let draws = vec![DrawGroup {
        index_start: 0,
        index_count: indices.len() as u32,
        diffuse: None,
        group_index: 0,
        ..Default::default()
    }];
    // identity bind palette sized to cover every referenced global bone
    const IDENT: [[f32; 4]; 4] =
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let bone_count = (target_rig.len()).max(max_joint as usize + 1);
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &verts {
        for k in 0..3 {
            bmin[k] = bmin[k].min(v.pos[k]);
            bmax[k] = bmax[k].max(v.pos[k]);
        }
    }
    if verts.is_empty() {
        (bmin, bmax) = ([0.0; 3], [0.0; 3]);
    }
    let stats = ModelStats {
        meshes: 1,
        vertices: verts.len(),
        skipped: 0,
        bbox_min: bmin,
        bbox_max: bmax,
        fit_center: [0.0; 3],
        fit_scale: 1.0,
        bones: vec![IDENT; bone_count],
        rig: target_rig.clone(),
        prelit: false,
    };
    let skin = SkinData {
        center: [0.0; 3],
        scale: 1.0,
        bones: vec![IDENT; bone_count],
        rig: target_rig,
        prelit: false,
    };
    Imported {
        verts,
        indices,
        draws,
        textures: TexMap::new(),
        stats,
        skin,
        skin_joints: Vec::new(),
        skin_joint_pos: Vec::new(),
        skin_ibm: Vec::new(),
        skin_parents: Vec::new(),
    }
}

pub fn import_model(path: &Path) -> Result<Imported, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "obj" => import_obj(path),
        "gltf" | "glb" => import_gltf(path),
        other => Err(format!("unsupported model format '.{other}' (obj / gltf / glb)")),
    }
}

fn base_vertex(pos: [f32; 3], normal: [f32; 3], uv: [f32; 2], color: [f32; 3]) -> Vertex {
    Vertex {
        pos,
        color,
        uv,
        normal,
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0, 0, 0, 0],
        weights: [255, 0, 0, 0],
    }
}

/// f32 skin weights → u8 quantised, guaranteed to sum to 255 (residue folded into the max weight).
fn quantise_weights(w: [f32; 4]) -> [u8; 4] {
    let sum: f32 = w.iter().sum();
    if sum <= 1e-6 {
        return [255, 0, 0, 0];
    }
    let mut q = [0u8; 4];
    let mut acc = 0u16;
    let mut maxi = 0usize;
    for i in 0..4 {
        let v = ((w[i] / sum) * 255.0).round().clamp(0.0, 255.0) as u16;
        q[i] = v as u8;
        acc += v;
        if w[i] > w[maxi] {
            maxi = i;
        }
    }
    // Fold the rounding residue into the dominant weight.
    let target = 255i32;
    let delta = target - acc as i32;
    q[maxi] = (q[maxi] as i32 + delta).clamp(0, 255) as u8;
    q
}

/// Bounding box + identity skin for a finished vertex set.
fn finish(
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<DrawGroup>,
    textures: TexMap,
    skin_joints: Vec<String>,
    skin_joint_pos: Vec<[f32; 3]>,
    skin_ibm: Vec<[[f32; 4]; 4]>,
    skin_parents: Vec<i32>,
) -> Imported {
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &verts {
        for k in 0..3 {
            bmin[k] = bmin[k].min(v.pos[k]);
            bmax[k] = bmax[k].max(v.pos[k]);
        }
    }
    if verts.is_empty() {
        (bmin, bmax) = ([0.0; 3], [0.0; 3]);
    }
    let stats = ModelStats {
        meshes: draws.len(),
        vertices: verts.len(),
        skipped: 0,
        bbox_min: bmin,
        bbox_max: bmax,
        fit_center: [0.0; 3],
        fit_scale: 1.0,
        bones: Vec::new(),
        rig: Vec::new(),
        prelit: false,
    };
    let mut skin = SkinData::identity();
    skin.center = [0.0; 3];
    skin.scale = 1.0;
    Imported { verts, indices, draws, textures, stats, skin, skin_joints, skin_joint_pos, skin_ibm, skin_parents }
}

// ───────────────────────────── OBJ ─────────────────────────────

/// Minimal OBJ: v / vt / vn / f (fan-triangulated), one draw group per `usemtl`/`o` run.
/// No MTL resolution (imported OBJs preview untextured; glTF carries materials).
fn import_obj(path: &Path) -> Result<Imported, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let (mut vs, mut vts, mut vns) = (Vec::new(), Vec::new(), Vec::new());
    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut draws: Vec<DrawGroup> = Vec::new();
    let mut group_start = 0u32;
    let mut dedup: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let flush_group = |indices: &mut Vec<u32>, draws: &mut Vec<DrawGroup>, group_start: &mut u32| {
        if indices.len() as u32 > *group_start {
            draws.push(DrawGroup {
                index_start: *group_start,
                index_count: indices.len() as u32 - *group_start,
                diffuse: None,
                group_index: draws.len(),
                ..Default::default()
            });
            *group_start = indices.len() as u32;
        }
    };

    let f3 = |t: &mut std::str::SplitAsciiWhitespace| -> [f32; 3] {
        let mut o = [0f32; 3];
        for v in o.iter_mut() {
            *v = t.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        }
        o
    };
    for line in text.lines() {
        let mut t = line.split_ascii_whitespace();
        match t.next() {
            Some("v") => vs.push(f3(&mut t)),
            Some("vn") => vns.push(f3(&mut t)),
            Some("vt") => {
                let u = t.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let v: f32 = t.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                vts.push([u, 1.0 - v]); // OBJ V is bottom-up; the engine samples top-down
            }
            Some("usemtl") | Some("o") | Some("g") => {
                flush_group(&mut indices, &mut draws, &mut group_start)
            }
            Some("f") => {
                let refs: Vec<&str> = t.collect();
                let mut face: Vec<u32> = Vec::with_capacity(refs.len());
                for r in &refs {
                    let mut it = r.split('/');
                    let pi = it.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                    let ti = it.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                    let ni = it.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                    let key = (pi, ti, ni);
                    let idx = *dedup.entry(key).or_insert_with(|| {
                        let res = |i: i64, n: usize| -> usize {
                            if i > 0 { (i - 1) as usize } else { (n as i64 + i) as usize }
                        };
                        let p = vs.get(res(pi, vs.len())).copied().unwrap_or([0.0; 3]);
                        let uv = if ti != 0 {
                            vts.get(res(ti, vts.len())).copied().unwrap_or([0.0; 2])
                        } else {
                            [0.0; 2]
                        };
                        let nm = if ni != 0 {
                            vns.get(res(ni, vns.len())).copied().unwrap_or([0.0, 1.0, 0.0])
                        } else {
                            [0.0, 1.0, 0.0]
                        };
                        verts.push(base_vertex(p, nm, uv, [1.0, 1.0, 1.0]));
                        (verts.len() - 1) as u32
                    });
                    face.push(idx);
                }
                for k in 1..face.len().saturating_sub(1) {
                    indices.extend_from_slice(&[face[0], face[k], face[k + 1]]);
                }
            }
            _ => {}
        }
    }
    flush_group(&mut indices, &mut draws, &mut group_start);
    if verts.is_empty() {
        return Err("OBJ contained no faces".into());
    }
    Ok(finish(verts, indices, draws, TexMap::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()))
}

// ───────────────────────────── glTF ─────────────────────────────

/// glTF/GLB via the `gltf` crate: default-scene nodes traversed with baked world transforms,
/// one draw group per primitive, base-color textures BC-compressed under synthetic hashes.
fn import_gltf(path: &Path) -> Result<Imported, String> {
    let (doc, buffers, images) =
        gltf::import(path).map_err(|e| format!("gltf {}: {e}", path.display()))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("import");

    // Base-color image index → (synthetic hash, TextureData), encoded once.
    let mut textures: TexMap = TexMap::new();
    let mut tex_hash_for_image: HashMap<usize, u32> = HashMap::new();
    let mut ensure_tex = |img_idx: usize, textures: &mut TexMap| -> Option<u32> {
        if let Some(&h) = tex_hash_for_image.get(&img_idx) {
            return Some(h);
        }
        let img = images.get(img_idx)?;
        let rgba = to_rgba8(img)?;
        let h = pandemic_hash_m2(&format!("{stem}#tex{img_idx}"));
        textures.insert(h, crate::texenc::encode_rgba(img.width, img.height, &rgba));
        tex_hash_for_image.insert(img_idx, h);
        Some(h)
    };

    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut draws: Vec<DrawGroup> = Vec::new();

    // A multi-skin file's per-primitive JOINTS_0 index into that primitive's OWN skin palette, but the
    // retarget keys on the FIRST skin's joint names only — mixing palettes would mis-map. Warn; the
    // common character case is a single skin.
    if doc.skins().count() > 1 {
        eprintln!(
            "[import] WARNING: {} has {} skins; the retarget uses only the first skin's palette — \
             other skins' vertices may bind wrong",
            path.display(),
            doc.skins().count()
        );
    }

    // Source skeleton: the first skin's joint node names, in palette order. Per-vertex JOINTS_0
    // index into THIS list — the retarget classifier keys on the names.
    let skin_joints: Vec<String> = doc
        .skins()
        .next()
        .map(|sk| {
            sk.joints()
                .map(|j| j.name().map(str::to_string).unwrap_or_else(|| format!("joint{}", j.index())))
                .collect()
        })
        .unwrap_or_default();

    // Source inverse-bind matrices (glTF column-major), padded to the joint count. The bind world
    // matrix of joint j is the inverse of its IBM; its translation is the bone's position.
    let skin_ibm: Vec<[[f32; 4]; 4]> = doc
        .skins()
        .next()
        .map(|sk| {
            let reader = sk.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let ibms: Vec<[[f32; 4]; 4]> =
                reader.read_inverse_bind_matrices().map(|it| it.collect()).unwrap_or_default();
            const IDENT: [[f32; 4]; 4] =
                [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
            (0..sk.joints().count()).map(|i| ibms.get(i).copied().unwrap_or(IDENT)).collect()
        })
        .unwrap_or_default();
    let skin_joint_pos: Vec<[f32; 3]> = skin_ibm
        .iter()
        .map(|ibm| {
            let w = glam::Mat4::from_cols_array_2d(ibm).inverse().w_axis;
            [w.x, w.y, w.z]
        })
        .collect();

    // Parent JOINT index per joint: walk the glTF node parent chain, skipping intermediate nodes that
    // aren't themselves joints, until we hit another joint (or the root, -1).
    let skin_parents: Vec<i32> = doc
        .skins()
        .next()
        .map(|sk| {
            let joints: Vec<usize> = sk.joints().map(|j| j.index()).collect();
            let node_to_joint: HashMap<usize, usize> =
                joints.iter().enumerate().map(|(ji, &ni)| (ni, ji)).collect();
            let mut parent_node: HashMap<usize, usize> = HashMap::new();
            for n in doc.nodes() {
                for c in n.children() {
                    parent_node.insert(c.index(), n.index());
                }
            }
            joints
                .iter()
                .map(|&ni| {
                    let mut p = parent_node.get(&ni).copied();
                    while let Some(pn) = p {
                        if let Some(&jp) = node_to_joint.get(&pn) {
                            return jp as i32;
                        }
                        p = parent_node.get(&pn).copied();
                    }
                    -1
                })
                .collect()
        })
        .unwrap_or_default();

    // Precompute every node's GLOBAL transform (column-major), so a SKINNED mesh can be baked
    // to its bind pose = Σ_k w_k · (jointWorld_k · IBM_k) · POSITION — exactly what a correct
    // glTF viewer renders. This applies the skin's ancestor transform (e.g. a Z-up rig's `Z_UP`
    // root node that maps (x,y,z)→(x,-z,y)); without it a Z-up character (Khronos RiggedFigure,
    // COLLADA exports) previews FACE-DOWN because that correction was dropped. glTF requires the
    // MESH node's own transform to be ignored for skinning (the joints carry placement), so a
    // skinned mesh is never `mat_point(world)`-baked — only the bind skin is applied.
    let node_count = doc.nodes().count();
    let mut nparent = vec![usize::MAX; node_count];
    for n in doc.nodes() {
        for c in n.children() {
            nparent[c.index()] = n.index();
        }
    }
    let nlocal: Vec<[[f32; 4]; 4]> = {
        let mut v = vec![IDENT4; node_count];
        for n in doc.nodes() {
            v[n.index()] = n.transform().matrix();
        }
        v
    };
    let mut nglobal = vec![IDENT4; node_count];
    let mut ndone = vec![false; node_count];
    fn resolve_global(
        i: usize,
        par: &[usize],
        loc: &[[[f32; 4]; 4]],
        out: &mut [[[f32; 4]; 4]],
        done: &mut [bool],
    ) {
        if done[i] {
            return;
        }
        let p = par[i];
        out[i] = if p == usize::MAX {
            loc[i]
        } else {
            resolve_global(p, par, loc, out, done);
            mat_mul(&out[p], &loc[i])
        };
        done[i] = true;
    }
    for i in 0..node_count {
        resolve_global(i, &nparent, &nlocal, &mut nglobal, &mut ndone);
    }

    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("gltf: no scene")?;
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> =
        scene.nodes().map(|n| (n, IDENT4)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = mat_mul(&parent, &node.transform().matrix());
        for child in node.children() {
            stack.push((child, world));
        }
        let Some(mesh) = node.mesh() else { continue };
        // Bind skin matrices for this node's skin: `jointWorld · IBM` per joint (column-major).
        let skin_mats: Option<Vec<[[f32; 4]; 4]>> = node.skin().map(|sk| {
            let r = sk.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let ibms: Vec<[[f32; 4]; 4]> =
                r.read_inverse_bind_matrices().map(|it| it.collect()).unwrap_or_default();
            sk.joints()
                .enumerate()
                .map(|(j, jn)| mat_mul(&nglobal[jn.index()], &ibms.get(j).copied().unwrap_or(IDENT4)))
                .collect()
        });
        // A skinned mesh is placed by its bind skin (below), not the mesh node's own world.
        let world = if node.skin().is_some() { IDENT4 } else { world };
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(pos_iter) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = pos_iter.collect();
            let base = verts.len() as u32;
            let normals: Vec<[f32; 3]> =
                reader.read_normals().map(|it| it.collect()).unwrap_or_default();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect())
                .unwrap_or_default();
            let colors: Vec<[f32; 3]> = reader
                .read_colors(0)
                .map(|c| c.into_rgb_f32().collect())
                .unwrap_or_default();
            let jv: Vec<[u16; 4]> =
                reader.read_joints(0).map(|j| j.into_u16().collect()).unwrap_or_default();
            let wv: Vec<[f32; 4]> =
                reader.read_weights(0).map(|w| w.into_f32().collect()).unwrap_or_default();
            for (i, &p) in positions.iter().enumerate() {
                // Skinned prim → bake the bind pose (applies the Z-up→Y-up / root correction).
                let (wp, nm) = match (&skin_mats, jv.get(i), wv.get(i)) {
                    (Some(sm), Some(j), Some(w)) if !sm.is_empty() && w.iter().any(|&x| x > 0.0) => {
                        let (mut acc, mut nacc, mut tot) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
                        let nrm = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                        for k in 0..4 {
                            if w[k] <= 0.0 {
                                continue;
                            }
                            let m = sm.get(j[k] as usize).copied().unwrap_or(IDENT4);
                            let pp = mat_point(&m, p);
                            let nn = mat_dir(&m, nrm);
                            for t in 0..3 {
                                acc[t] += w[k] * pp[t];
                                nacc[t] += w[k] * nn[t];
                            }
                            tot += w[k];
                        }
                        if tot > 0.0 {
                            for t in 0..3 {
                                acc[t] /= tot;
                            }
                        }
                        let l = (nacc[0] * nacc[0] + nacc[1] * nacc[1] + nacc[2] * nacc[2]).sqrt().max(1e-6);
                        (acc, [nacc[0] / l, nacc[1] / l, nacc[2] / l])
                    }
                    _ => (
                        mat_point(&world, p),
                        normals.get(i).map(|n| mat_dir(&world, *n)).unwrap_or([0.0, 1.0, 0.0]),
                    ),
                };
                let uv = uvs.get(i).copied().unwrap_or([0.0; 2]);
                let col = colors.get(i).copied().unwrap_or([1.0; 3]);
                verts.push(base_vertex(wp, nm, uv, col));
            }
            // Per-vertex skin binding for the RUNTIME skin (JOINTS_0 index the source palette).
            for (i, j) in jv.iter().enumerate() {
                let vi = base as usize + i;
                if vi < verts.len() {
                    verts[vi].joints =
                        [j[0].min(255) as u8, j[1].min(255) as u8, j[2].min(255) as u8, j[3].min(255) as u8];
                    verts[vi].weights = quantise_weights(wv.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]));
                }
            }
            let start = indices.len() as u32;
            match reader.read_indices() {
                Some(ind) => indices.extend(ind.into_u32().map(|i| base + i)),
                None => indices.extend(base..verts.len() as u32),
            }
            let diffuse = prim
                .material()
                .pbr_metallic_roughness()
                .base_color_texture()
                .and_then(|t| ensure_tex(t.texture().source().index(), &mut textures));
            draws.push(DrawGroup {
                index_start: start,
                index_count: indices.len() as u32 - start,
                diffuse,
                group_index: draws.len(),
                ..Default::default()
            });
        }
    }
    if verts.is_empty() {
        return Err("gltf contained no mesh primitives".into());
    }
    Ok(finish(verts, indices, draws, textures, skin_joints, skin_joint_pos, skin_ibm, skin_parents))
}

const IDENT4: [[f32; 4]; 4] =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Load a rigged glTF/GLB into [`CharGlbData`] for the FAITHFUL skinning writer
/// (`mercs2_formats::char_skin`). Unlike [`import_model`], this keeps the RAW per-vertex
/// POSITION/JOINTS_0/WEIGHTS_0 (f32, not preview-quantised), the whole node graph, per-node
/// ROW-MAJOR world matrices and per-joint inverse-bind matrices — exactly `build_character`'s
/// inputs. Reads mesh 0 / primitive 0 only (single-group scope, matching the mesher). The
/// Skeleton-workbench "Export faithful character" action re-loads the source file through this
/// so its output matches the CLI's exact path rather than the lossy preview import.
pub fn load_char_glb(path: &Path) -> Result<mercs2_formats::char_skin::CharGlbData, String> {
    use mercs2_formats::char_skin::CharGlbData;
    let (doc, buffers, _images) =
        gltf::import(path).map_err(|e| format!("gltf {}: {e}", path.display()))?;
    let get = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d.0[..]);

    // node graph over ALL nodes
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
    // per-node ROW-MAJOR world matrix (world = world_parent · local, glTF column-major math)
    let locals: Vec<[[f32; 4]; 4]> = {
        let mut v = vec![IDENT4; node_count];
        for n in doc.nodes() {
            v[n.index()] = n.transform().matrix();
        }
        v
    };
    let mut world_cm = vec![IDENT4; node_count];
    let mut done = vec![false; node_count];
    fn resolve(
        i: usize,
        parent: &[i32],
        local: &[[[f32; 4]; 4]],
        world: &mut [[[f32; 4]; 4]],
        done: &mut [bool],
    ) {
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
    // column-major [col][row] → ROW-MAJOR-colvec flat rm[r*4+c] = m[c][r]
    let cm_to_rm = |m: &[[f32; 4]; 4]| -> [f64; 16] {
        let mut f = [0.0f64; 16];
        for r in 0..4 {
            for c in 0..4 {
                f[r * 4 + c] = m[c][r] as f64;
            }
        }
        f
    };
    let node_world: Vec<[f64; 16]> = world_cm.iter().map(cm_to_rm).collect();

    // skin 0
    let skin = doc.skins().next().ok_or("glb has no skin — not rigged")?;
    let joint_nodes: Vec<usize> = skin.joints().map(|j| j.index()).collect();
    let reader = skin.reader(get);
    let ibm: Vec<Option<[f64; 16]>> = {
        let ibms: Vec<[[f32; 4]; 4]> = reader
            .read_inverse_bind_matrices()
            .map(|it| it.collect())
            .unwrap_or_default();
        (0..joint_nodes.len())
            .map(|i| ibms.get(i).map(cm_to_rm))
            .collect()
    };

    // Merge ALL meshes / primitives — a character ships body, head and accessories as separate meshes;
    // reading only mesh0 drops the head. All primitives of a single-skin file share the joint palette,
    // so it is a concat with an index offset. Non-skinned primitives are skipped.
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut vjoints: Vec<[u16; 4]> = Vec::new();
    let mut vweights: Vec<[f64; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // The source model's own sub-object partition, one entry per skinned primitive. This is the
    // authoring unit and it is how the game partitions a character too, so keeping it lets an
    // import be authored the way retail authors one: primitive -> draw group, each with its own
    // bone palette and exactly one material.
    let mut parts: Vec<MeshPart> = Vec::new();
    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let r = prim.reader(get);
            let (Some(pos), Some(joints), Some(weights)) =
                (r.read_positions(), r.read_joints(0), r.read_weights(0))
            else {
                continue; // not a skinned primitive → nothing for char_skin to re-pose
            };
            let base = positions.len() as u32;
            let ps: Vec<[f64; 3]> = pos.map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
            let m = ps.len();
            let nm: Vec<[f32; 3]> = r.read_normals().map(|it| it.collect()).unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; m]);
            let uv: Vec<[f32; 2]> = r.read_tex_coords(0).map(|tc| tc.into_f32().collect()).unwrap_or_else(|| vec![[0.0, 0.0]; m]);
            let jv: Vec<[u16; 4]> = joints.into_u16().collect();
            let wv: Vec<[f64; 4]> = weights.into_f32().map(|w| [w[0] as f64, w[1] as f64, w[2] as f64, w[3] as f64]).collect();
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

/// Column-major 4x4 multiply (`a * b`, glTF convention).
fn mat_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0f32; 4]; 4];
    for (c, oc) in o.iter_mut().enumerate() {
        for (r, v) in oc.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

fn mat_point(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let mut o = [0f32; 3];
    for (r, v) in o.iter_mut().enumerate() {
        *v = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    o
}

fn mat_dir(m: &[[f32; 4]; 4], d: [f32; 3]) -> [f32; 3] {
    let mut o = [0f32; 3];
    for (r, v) in o.iter_mut().enumerate() {
        *v = m[0][r] * d[0] + m[1][r] * d[1] + m[2][r] * d[2];
    }
    let l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt().max(1e-6);
    [o[0] / l, o[1] / l, o[2] / l]
}

/// gltf image data → straight RGBA8.
fn to_rgba8(img: &gltf::image::Data) -> Option<Vec<u8>> {
    use gltf::image::Format;
    let n = (img.width * img.height) as usize;
    Some(match img.format {
        Format::R8G8B8A8 => img.pixels.clone(),
        Format::R8G8B8 => {
            let mut o = Vec::with_capacity(n * 4);
            for p in img.pixels.chunks_exact(3) {
                o.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            o
        }
        Format::R8 => {
            let mut o = Vec::with_capacity(n * 4);
            for &g in &img.pixels {
                o.extend_from_slice(&[g, g, g, 255]);
            }
            o
        }
        Format::R8G8 => {
            let mut o = Vec::with_capacity(n * 4);
            for p in img.pixels.chunks_exact(2) {
                o.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
            o
        }
        _ => return None, // 16-bit / float formats: skip (preview falls back to white)
    })
}
