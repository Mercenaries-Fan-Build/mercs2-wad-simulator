//! Load real geometry from a Mercenaries 2 model container into engine vertices.
//!
//! Milestone 1b: pull FLOAT16 vertex positions from a model UCFX container (via
//! `mercs2_formats::model_cubeize::read_model_positions`) and fit them into clip space so the
//! engine can display real WAD geometry. Positions only for now (point cloud); index buffers and
//! the vertex declaration for proper triangles/UVs come in 1c.

use mercs2_formats::model_cubeize::{read_model_meshes, read_model_positions};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    /// BLENDINDICES (global HIER bone indices) + BLENDWEIGHT (0..255) for LBS.
    pub joints: [u8; 4],
    pub weights: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct ModelStats {
    pub meshes: usize,
    pub vertices: usize,
    /// Bone-local accessory groups skipped (unplaced; need skeleton bind-pose — see below).
    pub skipped: usize,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// Model-space centre and uniform scale that fit the model into view. Applied by the
    /// camera MVP (NOT baked into vertex positions) so skinning runs in model space.
    pub fit_center: [f32; 3],
    pub fit_scale: f32,
    /// Per-bone skinning palette `Skin[b] = InvBind[b] · Pose[b]` (row-vector). At bind pose
    /// (Pose = world-rest) every entry is identity — the LBS gate. Empty when no skeleton.
    pub bones: Vec<[[f32; 4]; 4]>,
    /// Per-bone rig (parent/inv-bind/local-bind) for re-posing under animation. Empty when no skeleton.
    pub rig: Vec<BoneRig>,
    /// PRELIT: the mesh bakes monochrome lighting into per-vertex COLOR (interior/building shells).
    /// The renderer then uses that baked term as the lighting and skips the fixed exterior sun. Detected
    /// data-driven (non-white, near-grayscale colors) so terrain SPLAT weights — colored, also in COLOR —
    /// don't trip it. False for characters/props (white vertex color) and terrain.
    pub prelit: bool,
}

/// One bone's rig data — enough to recompose an animated pose. All matrices are row-major,
/// row-vector convention (`world = local · world_parent`), matching `skeleton.rs`.
#[derive(Debug, Clone)]
pub struct BoneRig {
    pub parent: i32, // -1 = root
    pub name_hash: u32,
    /// Bind-pose world-rest transform.
    pub world_bind: [[f32; 4]; 4],
    /// Inverse of `world_bind` (the InvBind used in `Skin[b] = InvBind[b] · Pose[b]`).
    pub inv_bind: [[f32; 4]; 4],
    /// Bind-pose LOCAL transform (relative to parent). Animation replaces this per bone; bones
    /// with no track keep it, so an un-animated skeleton recomposes to the exact bind pose.
    pub local_bind: [[f32; 4]; 4],
}

/// Everything the renderer needs to skin + place a model: the fit transform, the bind-pose bone
/// palette (identity at bind), and the per-bone rig for re-posing under animation.
#[derive(Debug, Clone)]
pub struct SkinData {
    pub center: [f32; 3],
    pub scale: f32,
    pub bones: Vec<[[f32; 4]; 4]>,
    pub rig: Vec<BoneRig>,
    /// Baked-lighting flag (see [`ModelStats::prelit`]) — the renderer skips the exterior sun for it.
    pub prelit: bool,
}

impl SkinData {
    /// Identity skin (single bone) for un-skinned geometry (placeholder triangle, point clouds).
    pub fn identity() -> Self {
        SkinData {
            center: [0.0, 0.0, 0.0],
            scale: 1.0,
            bones: vec![[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]],
            rig: Vec::new(),
            prelit: false,
        }
    }
}

impl ModelStats {
    pub fn skin_data(&self) -> SkinData {
        SkinData {
            center: self.fit_center,
            scale: self.fit_scale,
            bones: self.bones.clone(),
            rig: self.rig.clone(),
            prelit: self.prelit,
        }
    }
}

/// Parse a model block/container file and return engine-space vertices (fitted to clip space)
/// plus raw stats. Accepts either a raw `UCFX` container or a model-only block
/// (`[u32 count][16B entry][UCFX ...]`) — we locate the `UCFX` magic and read from there.
pub fn load_model_block(path: &str) -> Result<(Vec<Vertex>, ModelStats), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let ucfx = bytes
        .windows(4)
        .position(|w| w == b"UCFX")
        .ok_or("no UCFX container found in file")?;
    build_from_container(&bytes[ucfx..])
}

/// One draw call: a contiguous index range + the diffuse texture hash to bind for it.
#[derive(Debug, Clone)]
pub struct DrawGroup {
    pub index_start: u32,
    pub index_count: u32,
    /// Diffuse (albedo) texture asset hash for this group's material, if any.
    pub diffuse: Option<u32>,
    /// Specular/gloss (`_sm`) texture asset hash (MTRL slot 1), if any.
    pub specular: Option<u32>,
    /// Normal-map texture asset hash (MTRL slot 2), if any.
    pub normal: Option<u32>,
    /// The container's drawing-group (PRMG) index this draw came from — the MESH-order index that
    /// `INDX`/destruction (`orchestrator::Destruction::state_of_mesh`) keys on. Lets a caller hide a
    /// group by destruction state (e.g. drop the `break_piece` rubble to show the pristine building).
    pub group_index: usize,
}

/// Build INDEXED triangle geometry, selecting the render/destruction tier bit `0x01` (the default
/// active LOD / intact-for-most-models state). See [`build_indexed_state`].
pub fn build_indexed_from_container(
    container: &[u8],
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_state(container, 0x01)
}

/// Build INDEXED triangle geometry from a model container (1d/1e): per-`PRMG` drawing group
/// vertices + de-stripped triangles, accessory groups skipped, fitted to a common transform, plus
/// per-group draw ranges tagged with the group's diffuse texture (via MTRL).
///
/// `active_bit` selects which SEGM `state_mask` tier to render: a segment is kept iff its mask is 0
/// (always-on) or shares a bit with `active_bit`. Default `0x01` is the top LOD / the intact state for
/// most models — but destructible "livedin" building shells invert this (mask `0x03` = ruined, `0x04`
/// = intact), so the PMC interior loads them with `active_bit = 0x04` to show the pristine building.
/// Returns (vertices, indices, draw-groups, stats).
pub fn build_indexed_state(
    container: &[u8],
    active_bit: u8,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    use mercs2_formats::model_cubeize::ModelMesh;
    use mercs2_formats::skeleton::{
        affine_inverse, mat4_mul, transform_dir, transform_point, Skeleton,
    };

    let meshes = read_model_meshes(container)?;
    let materials = mercs2_formats::texture::parse_mtrl(container);
    let group_mat = mercs2_formats::texture::group_material_indices(container);

    // Skeleton world-rest per bone, for placing rigid MESH accessories. from_block wants a
    // 20-byte wrapper + UCFX.
    let mut block = Vec::with_capacity(20 + container.len());
    block.extend_from_slice(&[0u8; 20]);
    block[16..20].copy_from_slice(&(container.len() as u32).to_le_bytes());
    block.extend_from_slice(container);
    let skel = Skeleton::from_block(&block).ok();

    // Skinning palette: Skin[b] = InvBind[b] · Pose[b] (row-vector). Phase A is the bind-pose gate:
    // Pose = world-rest, so Skin[b] = InvBind[b] · WorldBind[b] = identity (up to fp). Rebuilding it
    // from the real matrices (rather than emitting literal identities) exercises the matmul/inverse
    // so a convention bug detonates the model instead of hiding. Phase B swaps Pose per animation frame.
    let bones: Vec<[[f32; 4]; 4]> = match &skel {
        Some(s) => (0..s.bones.len())
            .map(|b| mat4_mul(&s.inv_world_bind(b), &s.world_bind(b)))
            .collect(),
        None => Vec::new(),
    };

    // Per-bone rig for animation: parent, inv-bind, and the bind-pose LOCAL transform
    // (local_b = world_b · inv(world_parent); root's local = its world).
    let rig: Vec<BoneRig> = match &skel {
        Some(s) => s
            .bones
            .iter()
            .map(|b| {
                let inv_bind = affine_inverse(&b.world);
                let local_bind = if b.parent < 0 {
                    b.world
                } else {
                    mat4_mul(&b.world, &affine_inverse(&s.bones[b.parent as usize].world))
                };
                BoneRig {
                    parent: b.parent,
                    name_hash: b.name_hash,
                    world_bind: b.world,
                    inv_bind,
                    local_bind,
                }
            })
            .collect(),
        None => Vec::new(),
    };

    // Active LOD/state tier: body sub-objects carry a single-bit mask (0x01/02/04/08), accessories
    // 0x0f (all). Render only the caller's tier + accessories → no LOD/state overdraw (the triple hair
    // / the intact-vs-ruined building states). `active_bit` defaults to 0x01 (see build_indexed_state).
    let lod_bit = active_bit;

    // Per kept group: world-space geometry (rigid MESH groups transformed by their bone's rest).
    struct Placed<'a> {
        m: &'a ModelMesh,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
    }

    // Per-PRMG-group POFF (parent GEOM position offset). Needed so multi-GEOM meshes (the terrainmesh:
    // 16 sub-tiles at ±150/±50) don't collapse onto each other; single-GEOM models get [0,0,0].
    let poffs = mercs2_formats::model_cubeize::prmg_geom_offsets(container);

    let mut skipped = 0usize;
    let mut kept: Vec<Placed> = Vec::new();
    for m in &meshes {
        if m.state_mask != 0 && (m.state_mask & lod_bit) == 0 {
            skipped += 1; // inactive LOD/state tier
            continue;
        }
        // Rigid accessories are authored in bone-local space -> apply the bone's world-rest.
        let bonemat = if m.rigid {
            skel.as_ref().and_then(|s| s.bones.get(m.bone as usize)).map(|b| b.world)
        } else {
            None
        };
        let (mut positions, normals, tangents) = if let Some(w) = bonemat {
            (
                m.positions.iter().map(|&p| transform_point(&w, p)).collect(),
                m.normals.iter().map(|&n| transform_dir(&w, n)).collect(),
                m.tangents
                    .iter()
                    .map(|&t| {
                        let d = transform_dir(&w, [t[0], t[1], t[2]]);
                        [d[0], d[1], d[2], t[3]]
                    })
                    .collect(),
            )
        } else {
            (m.positions.clone(), m.normals.clone(), m.tangents.clone())
        };
        // Apply the group's POFF (parent-GEOM translation). Non-zero only for multi-GEOM meshes.
        if let Some(off) = poffs.get(m.group_index) {
            if *off != [0.0, 0.0, 0.0] {
                for p in positions.iter_mut() {
                    p[0] += off[0];
                    p[1] += off[1];
                    p[2] += off[2];
                }
            }
        }
        kept.push(Placed { m, positions, normals, tangents });
    }
    if kept.is_empty() {
        return Err("model container had no placed drawing groups".into());
    }

    // Common fit across all kept (world-space) positions.
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for pl in &kept {
        for p in &pl.positions {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let extent = (0..3).map(|k| max[k] - min[k]).fold(0.0f32, f32::max).max(1e-6);
    let scale = 1.5 / extent;

    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut draws: Vec<DrawGroup> = Vec::new();
    for pl in &kept {
        let m = pl.m;
        let base = verts.len() as u32;
        // Rigid accessories are pre-transformed into bind space above; bind them 100% to their
        // attach bone so the same LBS palette carries them (and follows the bone under animation).
        // Skinned bodies keep their extracted BLENDINDICES/BLENDWEIGHT. Positions stay in model
        // space — the fit (center/scale) is applied by the camera MVP so skinning is model-space.
        let rigid_bind: Option<([u8; 4], [u8; 4])> =
            m.rigid.then_some(([m.bone as u8, 0, 0, 0], [255, 0, 0, 0]));
        for (vi, p) in pl.positions.iter().enumerate() {
            let (joints, weights) = match rigid_bind {
                Some(jw) => jw,
                None => (
                    m.joints.get(vi).copied().unwrap_or([0, 0, 0, 0]),
                    m.weights.get(vi).copied().unwrap_or([255, 0, 0, 0]),
                ),
            };
            verts.push(Vertex {
                pos: [p[0], p[1], p[2]],
                // Per-vertex COLOR (D3DCOLOR, stored B,G,R,A) → RGB. For static interior/building meshes
                // this is BAKED vertex lighting (the Pandemic-era interior light); the shader multiplies
                // it into albedo. Default WHITE (no-op) for meshes with no COLOR element — NOT 0.5, which
                // was silently halving every texture.
                color: m
                    .colors
                    .get(vi)
                    .map(|c| [c[2] as f32 / 255.0, c[1] as f32 / 255.0, c[0] as f32 / 255.0])
                    .unwrap_or([1.0, 1.0, 1.0]),
                uv: m.uvs.get(vi).copied().unwrap_or([0.0, 0.0]),
                normal: pl.normals.get(vi).copied().unwrap_or([0.0, 1.0, 0.0]),
                tangent: pl.tangents.get(vi).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]),
                joints,
                weights,
            });
        }
        // One DrawGroup per PRMT sub-strip material. A PRMG group frequently concatenates several
        // sub-strips with DIFFERENT materials (the PMC hall's floor/walls/trim share group 1's 23
        // materials); binding only the first textured the floor with a neighbour's map. `submeshes`
        // is empty for clean single-strip groups → fall back to the group's first material.
        let emit = |draws: &mut Vec<DrawGroup>,
                    indices: &[u32],
                    index_start: u32,
                    mat: Option<&mercs2_formats::texture::MtrlMaterial>| {
            let index_count = indices.len() as u32 - index_start;
            if index_count == 0 {
                return;
            }
            draws.push(DrawGroup {
                index_start,
                index_count,
                diffuse: mat.and_then(|m| m.diffuse()),
                specular: mat.and_then(|m| m.specular()),
                normal: mat.and_then(|m| m.textures.get(2).copied()),
                group_index: m.group_index,
            });
        };
        if m.submeshes.is_empty() {
            let index_start = indices.len() as u32;
            for t in &m.tris {
                indices.push(base + t[0]);
                indices.push(base + t[1]);
                indices.push(base + t[2]);
            }
            let material = group_mat.get(m.group_index).and_then(|&mi| materials.get(mi));
            emit(&mut draws, &indices, index_start, material);
        } else {
            for sm in &m.submeshes {
                let index_start = indices.len() as u32;
                for t in &m.tris[sm.tri_start..sm.tri_start + sm.tri_count] {
                    indices.push(base + t[0]);
                    indices.push(base + t[1]);
                    indices.push(base + t[2]);
                }
                emit(&mut draws, &indices, index_start, materials.get(sm.material_index));
            }
        }
    }

    // PRELIT detection: a vertex COLOR that is non-white AND near-grayscale = baked monochrome interior
    // lighting (the shells), distinct from colored terrain SPLAT weights (also in COLOR). Gate on
    // RIGID (non-skinned) geometry: SKINNED characters carry baked ambient-occlusion in COLOR too
    // (Mattias mean ~0.72, blended across bones) but must keep their dynamic sun/point lighting — only
    // the static building shells (each vertex rigidly bound to ONE bone) bake their FULL lighting and
    // should drop the sun. Skinned = any vertex weights across >1 bone.
    let skinned = meshes
        .iter()
        .any(|m| m.weights.iter().any(|w| w[1] != 0 || w[2] != 0 || w[3] != 0));
    let prelit = !skinned
        && meshes.iter().any(|m| {
            m.colors.iter().any(|c| {
                let (r, g, b) = (c[2] as i32, c[1] as i32, c[0] as i32); // D3DCOLOR stored B,G,R,A
                let mx = r.max(g).max(b);
                let mn = r.min(g).min(b);
                mx < 242 && (mx - mn) < 20
            })
        });
    let stats = ModelStats {
        meshes: kept.len(),
        vertices: verts.len(),
        skipped,
        bbox_min: min,
        bbox_max: max,
        fit_center: center,
        fit_scale: scale,
        bones,
        rig,
        prelit,
    };
    Ok((verts, indices, draws, stats))
}

/// Build fitted, colored engine vertices from a raw UCFX model container.
/// Game space is left-handed, +Y up (docs/coordinate_systems.md) — Y maps to screen height.
pub fn build_from_container(container: &[u8]) -> Result<(Vec<Vertex>, ModelStats), String> {
    let meshes = read_model_positions(container)?;

    // Skip bone-local (unplaced) accessory groups. Rigid props attached to a single bone are
    // authored in that bone's LOCAL space (bbox clustered at the origin) and must be placed by the
    // skeleton's bind-pose (bone world-rest) transform — which we don't apply yet. Rendered raw they
    // pile up at the origin (the blob near the feet). Detect by a bbox centered near the origin.
    // TODO(skeleton): place these via `skeleton::Skeleton` bone world-rest instead of skipping.
    let mut skipped = 0usize;
    let mut raw: Vec<[f32; 3]> = Vec::new();
    for m in &meshes {
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for p in m {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let c = [
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        ];
        if (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() < 0.3 {
            skipped += 1;
        } else {
            raw.extend_from_slice(m);
        }
    }
    if raw.is_empty() {
        return Err("model container had no placed vertex positions".into());
    }

    // Bounding box of the raw (model-local) positions.
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in &raw {
        for k in 0..3 {
            if p[k].is_finite() {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let extent = (0..3)
        .map(|k| max[k] - min[k])
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let scale = 1.5 / extent; // center at origin, largest axis spans ~[-0.75, 0.75]

    // Keep real 3D positions (Y-up); the camera/projection handles the view. Model is centered
    // at the origin so an orbit camera can spin around it.
    let verts = raw
        .iter()
        .map(|p| {
            let x = (p[0] - center[0]) * scale;
            let y = (p[1] - center[1]) * scale;
            let z = (p[2] - center[2]) * scale;
            Vertex {
                pos: [x, y, z],
                // colour by normalized position so structure is visible
                color: [
                    (x * 0.5 + 0.5).clamp(0.0, 1.0),
                    (y * 0.5 + 0.5).clamp(0.0, 1.0),
                    (z * 0.5 + 0.5).clamp(0.0, 1.0),
                ],
                uv: [0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            }
        })
        .collect();

    let stats = ModelStats {
        meshes: meshes.len() - skipped,
        vertices: raw.len(),
        skipped,
        bbox_min: min,
        bbox_max: max,
        // This path bakes the fit into positions and has no skeleton palette.
        fit_center: [0.0, 0.0, 0.0],
        fit_scale: 1.0,
        bones: Vec::new(),
        rig: Vec::new(),
        prelit: false, // position-only path carries no vertex colors
    };
    Ok((verts, stats))
}
