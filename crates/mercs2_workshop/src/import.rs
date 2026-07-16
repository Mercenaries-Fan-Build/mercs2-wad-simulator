//! Foreign-model import: `.obj` / `.gltf` / `.glb` → the same in-memory `ModelData` a WAD model
//! loads into, so an imported mesh previews on the engine renderer like any game asset.
//!
//! Conventions: the engine's asset space is right-handed, +Y up, metres (see the handedness note
//! in `scene.rs`) — glTF matches directly, OBJ is taken verbatim. Imported textures are
//! BC-compressed on the fly (`texenc`) under synthetic `m2(<file>#<n>)` hashes.
//!
//! The glTF path also reads the SOURCE RIG when the file carries one: the first skin's joint node
//! names (`skin_joints`) plus per-vertex `JOINTS_0`/`WEIGHTS_0`. This is what the Skeleton workbench
//! retargets — a ValveBiped/Mixamo/Unreal skin's bones mapped onto a Mercs2 HIER skeleton. Positions
//! are still baked to the rest pose for the static preview; the joints/weights ride alongside.

use std::collections::HashMap;
use std::path::Path;

use mercs2_engine::mesh::{DrawGroup, ModelStats, SkinData, Vertex};
use mercs2_engine::render::TexMap;
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
    Imported { verts, indices, draws, textures, stats, skin, skin_joints }
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
    Ok(finish(verts, indices, draws, TexMap::new(), Vec::new()))
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

    // Node traversal with baked world transforms (column-major 4x4).
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("gltf: no scene")?;
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> =
        scene.nodes().map(|n| (n, IDENT4)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = mat_mul(&parent, &node.transform().matrix());
        for child in node.children() {
            stack.push((child, world));
        }
        let Some(mesh) = node.mesh() else { continue };
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(pos_iter) = reader.read_positions() else { continue };
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
            for (i, p) in pos_iter.enumerate() {
                let wp = mat_point(&world, p);
                let nm = normals.get(i).map(|n| mat_dir(&world, *n)).unwrap_or([0.0, 1.0, 0.0]);
                let uv = uvs.get(i).copied().unwrap_or([0.0; 2]);
                let col = colors.get(i).copied().unwrap_or([1.0; 3]);
                verts.push(base_vertex(wp, nm, uv, col));
            }
            // Per-vertex skin binding (JOINTS_0 / WEIGHTS_0), when the primitive is rigged. Joints
            // index into the source skin's palette (== `skin_joints` for single-skin files, the
            // common character case). Retarget remaps these onto the target HIER skeleton.
            if let Some(joints) = reader.read_joints(0) {
                let jv: Vec<[u16; 4]> = joints.into_u16().collect();
                let wv: Vec<[f32; 4]> = reader
                    .read_weights(0)
                    .map(|w| w.into_f32().collect())
                    .unwrap_or_default();
                for (i, j) in jv.iter().enumerate() {
                    let vi = base as usize + i;
                    if vi < verts.len() {
                        verts[vi].joints =
                            [j[0].min(255) as u8, j[1].min(255) as u8, j[2].min(255) as u8, j[3].min(255) as u8];
                        let w = wv.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
                        verts[vi].weights = quantise_weights(w);
                    }
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
    Ok(finish(verts, indices, draws, textures, skin_joints))
}

const IDENT4: [[f32; 4]; 4] =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

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
