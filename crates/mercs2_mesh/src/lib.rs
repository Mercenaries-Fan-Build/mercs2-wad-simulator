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
    /// The vertex COLOR, RGBA. `PgMeshVP` passes it to COLOR0 unchanged; the `*AmbOcc*` pixel shaders
    /// multiply their output alpha by its alpha.
    pub color: [f32; 4],
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
    /// Per-bone REST palette `Skin[b] = InvBind[b] · Pose[b]` (row-vector) with Pose = the `+16`
    /// reference pose retail holds for a node no clip drives. It is identity only where `+16` and
    /// `+80` agree. Empty when no skeleton.
    pub bones: Vec<[[f32; 4]; 4]>,
    /// Per-bone rig (parent/inv-bind/local-bind) for re-posing under animation. Empty when no skeleton.
    pub rig: Vec<BoneRig>,
}

/// One bone's rig data — enough to recompose an animated pose. All matrices are row-major,
/// row-vector convention (`world = local · world_parent`), matching `skeleton.rs`.
#[derive(Debug, Clone)]
pub struct BoneRig {
    pub parent: i32, // -1 = root
    pub name_hash: u32,
    /// SKIN bind world transform: the inverse of the HIER record's `+80` inverse-bind — the pose the
    /// geometry was authored against (see `build_indexed_source`).
    pub world_bind: [[f32; 4]; 4],
    /// Inverse of `world_bind` (the InvBind used in `Skin[b] = InvBind[b] · Pose[b]`).
    pub inv_bind: [[f32; 4]; 4],
    /// REFERENCE-pose LOCAL transform (relative to parent): the HIER `+16` local, which is what
    /// retail's `hkaSkeleton` reference pose holds. Animation replaces it per bone; bones with no
    /// track keep it. It recomposes to `world_bind` only where `+16` and `+80` agree — on most models
    /// everywhere, on `pmc_hum_chris` not on the arm chain.
    pub local_bind: [[f32; 4]; 4],
}

/// Everything the renderer needs to skin + place a model: the fit transform, the rest palette
/// ([`ModelStats::bones`]), and the per-bone rig for re-posing under animation.
#[derive(Debug, Clone)]
pub struct SkinData {
    pub center: [f32; 3],
    pub scale: f32,
    pub bones: Vec<[[f32; 4]; 4]>,
    pub rig: Vec<BoneRig>,
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
        }
    }

    /// The palette an instance of a model with these `verts` rests at when no clip drives it. It
    /// covers every joint the verts reference, not just the entries the skin ships: an unrigged mesh
    /// carrying the 1-entry identity skin can still index a full palette, and a short palette makes
    /// those verts read a zero matrix and collapse onto the origin. A rigged skin rests at
    /// [`Self::bones`], which must already cover every joint; an unrigged one (terrain, cells) at
    /// identity.
    pub fn rest_palette(&self, verts: &[Vertex]) -> Result<Vec<[[f32; 4]; 4]>, String> {
        const IDENT: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let max_joint = verts.iter().flat_map(|v| v.joints).map(|j| j as usize).max().unwrap_or(0);
        let joints = self.bones.len().max(max_joint + 1);
        if self.rig.is_empty() {
            return Ok(vec![IDENT; joints]);
        }
        if self.bones.len() != joints {
            return Err(format!(
                "a {}-bone rig ships a {}-entry rest palette for {joints} referenced joints",
                self.rig.len(),
                self.bones.len()
            ));
        }
        Ok(self.bones.clone())
    }
}

impl ModelStats {
    /// Fold another LOD rung's stats in. A model's geometry spans several blocks (see
    /// [`crate::model::Model`]); each rung is built separately but they are one object, so counts and
    /// bounds accumulate. The rig and bones come from the RESIDENT rung — the only block
    /// that ships a skeleton — so they are left alone here.
    pub fn absorb(&mut self, other: &ModelStats) {
        self.meshes += other.meshes;
        self.vertices += other.vertices;
        self.skipped += other.skipped;
        for i in 0..3 {
            self.bbox_min[i] = self.bbox_min[i].min(other.bbox_min[i]);
            self.bbox_max[i] = self.bbox_max[i].max(other.bbox_max[i]);
        }
    }

    pub fn skin_data(&self) -> SkinData {
        SkinData {
            center: self.fit_center,
            scale: self.fit_scale,
            bones: self.bones.clone(),
            rig: self.rig.clone(),
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
    /// **Every** texture slot of this group's material, in authored order (slot 0 = diffuse,
    /// 1 = specular, 2 = normal, then whatever else the material binds — up to 10).
    ///
    /// The three named fields above are a convenience for the renderer, which only samples
    /// those. They are NOT the whole picture: a tool that asks "which parts of this model use
    /// texture X" and consults only those three gets the wrong answer for every texture bound
    /// in a later slot.
    pub textures: Vec<u32>,
    /// The material's on-disk `flags@104` word: blend mode, alpha test, two-sided and the
    /// refraction bit (`mercs2_formats::texture::runtime_material_flags` decodes it). 0 = an opaque,
    /// untested, culled material, which is also what a draw with no material gets.
    pub material_flags: u16,
    /// The material's pixel-shader key (`pandemic_hash_m2` of a registered pixel shader name, e.g.
    /// `PgDiffEmisFP`), which decides what the texture slots mean. 0 = no material.
    pub shader_key: u32,
    /// The material's pixel constants `materialData` c93..c98 (diffuse, specular, ambient, emissive,
    /// subsurface, constant0; `MtrlMaterial::material_data`): c98.w is the alpha multiplier (pixel
    /// alpha = `diffuse.a × c98.w`). [`NO_MATERIAL_DATA`] for a draw with no material.
    pub material_data: [[f32; 4]; 6],
    /// The container's drawing-group (PRMG) index this draw came from — the MESH-order index that
    /// `INDX`/destruction (`orchestrator::Destruction::state_of_mesh`) keys on. Lets a caller hide a
    /// group by destruction state (e.g. drop the `break_piece` rubble to show the pristine building).
    pub group_index: usize,
    /// `SEGM[INDX[group]].state_mask` — the LOD tier bitmask. Tested against the object's `view_state`
    /// with an ANY-bit overlap. Clause 2 of the draw gate. NOT a destruction state.
    pub lod_mask: u8,
    /// `SEGM[INDX[group]].node` — the HIER node, SIGNED (negative = no node = always visible). This is
    /// the mesh's real ATTACHMENT MOUNT (rigid meshes are authored in its local space) and the key
    /// into the object's node-enable table. Clause 3 of the draw gate.
    pub node: i16,
    /// Parent `SKIN`/`MESH` sub-object ordinal under `GEOM`. NOT the SEGM index — see `seg_id`.
    pub sub_object: usize,
    /// `INDX[group]` — this group's index into the SEGM record array.
    pub seg_id: usize,
    /// The `SEGM` row this draw serves. Retail's draw loop (`FUN_00472a50`) walks SEGM rows, not
    /// geometry: row `r` draws `renderable[SEGM[r].seg_id]` at `SEGM[r].node` under
    /// `SEGM[r].state_mask`. Equal to `seg_id` on the row that owns the geometry; different on an
    /// INSTANCE row, which draws another row's geometry again at its own node.
    pub segm_row: usize,
    /// The LOD-block rung this geometry came from: `0` = the resident (coarsest) block, higher = a
    /// finer `_P00N_` block. Stamped by [`crate::model::Model::load`]; a single-block model (every
    /// character) is all rung 0. The renderer ignores it — `lod_mask` already carries the supersede
    /// resolution — but an EXPORTER needs it, because the rungs re-author the same nodes and must be
    /// written as separate, labelled detail levels rather than stacked in one space.
    pub rung: u8,
    /// This group carries its OWN `BLENDINDICES`/`BLENDWEIGHT` — a deforming skin (a character's
    /// body). `false` means the group was NODE-RIGID and this builder already baked it into its
    /// bone's bind-space and bound it 100% to that bone, so the same LBS palette carries it.
    ///
    /// Both kinds therefore end up in ONE space, driven by ONE palette. The distinction matters to
    /// an exporter: a rigid group can be un-baked and re-parented under its bone (what a vehicle
    /// wants), but a skinned group must NOT be — its vertices are shared across many bones and only
    /// a real skin + inverse-bind matrices can reproduce them.
    pub skinned: bool,
    /// A terrain ground draw: its material is a `PgTerrainMeshFP*` shader, whose texture slots and
    /// constants are not a model material's.
    pub ground: Option<TerrainGround>,
    /// A ground cover draw ([`build_scrub`]): the depths it fades and culls at.
    pub scrub: Option<ScrubDepths>,
}

/// What a terrain ground draw's `PgTerrainMeshVP*`/`PgTerrainMeshFP*` pair reads. The material's slots
/// are the shader's sampler stages: 0 `GlobalMap`, 1 `DetailMap`, then `Layer1..4Map_Diff` at 2, 4, 6
/// and 8, with the unused odd stages holding `texture::TERRAIN_LAYER_MARKER`.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainGround {
    /// The cell's own map, stretched over the whole 400 m cell.
    pub global: u32,
    pub detail: Option<u32>,
    pub layers: [Option<u32>; 4],
    /// Each layer's planar projection of the world position ([`mercs2_formats::terrainmesh::Mtrl::layer_uv`]).
    pub layer_uv: [[[f32; 4]; 2]; 4],
    pub uv_scale: [f32; 2],
    /// The cell's centre, world x and z.
    pub centre: [f32; 2],
    pub fade: mercs2_formats::terrainmesh::TerrainFade,
}

/// The `materialData` of a draw with no material (terrain tiles, imports, debug draws): no retail value
/// exists for it, so it is the block that leaves the renderer's lighting unchanged — diffuse and ambient
/// 1, no emissive, alpha multiplier 1.
pub const NO_MATERIAL_DATA: [[f32; 4]; 6] = [
    [1.0, 1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 1.0],
];

impl Default for DrawGroup {
    /// An unconditionally-visible group: present at every LOD rung, bound to no HIER node. This is
    /// the right shape for geometry that has no `SEGM` record at all — terrain tiles, imported
    /// meshes, procedural debug draws — so the draw gate never suppresses it.
    fn default() -> DrawGroup {
        DrawGroup {
            index_start: 0,
            index_count: 0,
            diffuse: None,
            specular: None,
            normal: None,
            textures: Vec::new(),
            material_flags: 0,
            shader_key: 0,
            material_data: NO_MATERIAL_DATA,
            group_index: 0,
            lod_mask: 0xFF,
            node: -1,
            sub_object: 0,
            seg_id: 0,
            segm_row: 0,
            rung: 0,
            skinned: false,
            ground: None,
            scrub: None,
        }
    }
}

/// One ground cover instance as the GPU reads it: the matrix rows, cell-local, then
/// [`Instance::vertical_shear`](mercs2_formats::scrub::Instance::vertical_shear) for a shearing mesh
/// and 0 otherwise.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ScrubInstance {
    pub rows: [[f32; 4]; 3],
    pub shear: [f32; 4],
}

/// The depths one mesh of a pack draws at ([`mercs2_formats::scrub::ScrubMesh`]).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScrubDepths {
    pub alpha_zero: f32,
    pub cull: f32,
    /// Alpha-tested: drawn solid at every depth. Otherwise blended once a patch crosses `alpha_zero`.
    pub cutout: bool,
}

/// A run of one mesh's instances.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScrubRange {
    /// Into the pack's draws.
    pub mesh: u32,
    pub first_instance: u32,
    pub instance_count: u32,
}

/// One patch: what the renderer culls. Cell-local.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrubPatch {
    /// What the retail draw measures depth from.
    pub origin: [f32; 3],
    pub centre: [f32; 3],
    pub radius: f32,
    pub ranges: Vec<ScrubRange>,
}

/// A ground cover pack, ready to upload: each mesh's one tuft in `verts` and `indices`, instanced.
#[derive(Clone, Default)]
pub struct ScrubGeometry {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// One per mesh.
    pub draws: Vec<DrawGroup>,
    pub instances: Vec<ScrubInstance>,
    pub patches: Vec<ScrubPatch>,
}

/// One billboard tree as the GPU reads it: `PgBillboardTreeVP`'s `POSITION`, less the corner index its
/// `w` also carries.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TreeInstance {
    pub position: [f32; 3],
    /// A fraction of a turn ([`mercs2_formats::foliage::Leaf::yaw`]).
    pub yaw: f32,
}

/// A species' billboard, ready to upload: its quads, each corner's `pos` the offset (across, up, 0) and
/// `color` the atlas tiles (across, down, 0, 0), and one draw over them all.
#[derive(Clone)]
pub struct TreeGeometry {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draw: DrawGroup,
}

/// Build a tree species' billboard.
pub fn build_billboard(species: &mercs2_formats::foliage::Species) -> TreeGeometry {
    let m = species.billboard_material();
    let (mut verts, mut indices) = (Vec::new(), Vec::new());
    for view in species.views() {
        let base = verts.len() as u32;
        verts.extend(view.corners.iter().zip(&view.uvs).map(|(c, uv)| Vertex {
            pos: [c[0], c[1], 0.0],
            color: [species.atlas[0] as f32, species.atlas[1] as f32, 0.0, 0.0],
            uv: *uv,
            normal: [0.0; 3],
            tangent: [0.0; 4],
            joints: [0; 4],
            weights: [255, 0, 0, 0],
        }));
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    let draw = DrawGroup {
        index_count: indices.len() as u32,
        diffuse: m.diffuse(),
        normal: m.slot(2),
        textures: m.textures.clone(),
        material_flags: m.flags,
        material_data: m.material_data(),
        ..Default::default()
    };
    TreeGeometry { verts, indices, draw }
}

/// Build a ground cover container's geometry. An empty container builds none.
pub fn build_scrub(container: &[u8]) -> Result<ScrubGeometry, String> {
    use mercs2_formats::scrub::{ScrubPack, Tuft};
    use mercs2_formats::shader_registrations::retail_name;

    let ScrubPack::Pack(pack) = ScrubPack::decode(container)? else {
        return Ok(ScrubGeometry::default());
    };
    let materials = mercs2_formats::texture::parse_mtrl(container, mercs2_formats::texture::MtrlSource::Scrub)
        .map_err(|e| format!("MTRL: {e}"))?;
    if materials.len() != pack.meshes.len() {
        return Err(format!("{} materials for {} meshes", materials.len(), pack.meshes.len()));
    }
    let mut g = ScrubGeometry::default();
    let mut shears = Vec::new();
    for (i, (mesh, m)) in pack.meshes.iter().zip(&materials).enumerate() {
        let Tuft { vertices, triangles } = mesh.tuft().map_err(|e| format!("mesh {i}: {e}"))?;
        let name = retail_name(mesh.info[1]).ok_or_else(|| format!("mesh {i}: vertex shader 0x{:08X} is not registered", mesh.info[1]))?;
        shears.push(name.ends_with("_ShearVertically"));
        let base = g.verts.len() as u32;
        g.verts.extend(vertices.iter().map(|v| Vertex {
            pos: v.position,
            color: [1.0; 4],
            uv: v.uv,
            normal: v.normal,
            tangent: v.tangent.unwrap_or([0.0; 4]),
            joints: [0; 4],
            weights: [255, 0, 0, 0],
        }));
        let index_start = g.indices.len() as u32;
        g.indices.extend(triangles.iter().flatten().map(|&v| base + v as u32));
        g.draws.push(DrawGroup {
            index_start,
            index_count: g.indices.len() as u32 - index_start,
            diffuse: m.diffuse(),
            specular: m.specular(),
            normal: m.slot(2),
            textures: m.textures.clone(),
            material_flags: m.flags,
            shader_key: m.shader_key,
            material_data: m.material_data(),
            scrub: Some(ScrubDepths { alpha_zero: mesh.alpha_zero_depth(), cull: mesh.cull_depth(), cutout: m.alpha_tested() }),
            ..Default::default()
        });
    }
    for patch in &pack.patches {
        let first = patch.first_range as usize;
        let mut ranges = Vec::with_capacity(patch.range_count as usize);
        for r in &pack.ranges[first..first + patch.range_count as usize] {
            let first_instance = g.instances.len() as u32;
            let at = r.first_instance as usize;
            g.instances.extend(pack.instances[at..at + r.instance_count as usize].iter().map(|inst| {
                let mut rows = inst.matrix();
                for (row, origin) in rows.iter_mut().zip(patch.origin) {
                    row[3] += origin;
                }
                let [x, y, z] = if shears[r.mesh as usize] { inst.vertical_shear() } else { [0.0; 3] };
                ScrubInstance { rows, shear: [x, y, z, 0.0] }
            }));
            ranges.push(ScrubRange { mesh: r.mesh as u32, first_instance, instance_count: r.instance_count as u32 });
        }
        g.patches.push(ScrubPatch {
            origin: patch.origin,
            centre: [0, 1, 2].map(|k| patch.origin[k] + patch.sphere_centre[k]),
            radius: patch.sphere_radius,
            ranges,
        });
    }
    Ok(g)
}

/// Build INDEXED triangle geometry, selecting the render/destruction tier bit `0x01` (the default
/// active LOD / intact-for-most-models state). See [`build_indexed_state`].
pub fn build_indexed_from_container(
    container: &[u8],
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_state(container, 0x01)
}

/// The distinct SEGM state/LOD tier bits a model container carries (always-on `mask == 0` groups
/// excluded). `[0x01]` for single-tier props; `[0x01, 0x02, 0x04]`-style for LOD chains; the
/// destructible "livedin" shells pair `0x04` (intact) with `0x03` (ruined). Each returned bit is
/// a valid `active_bit` for [`build_indexed_state`].
pub fn state_tiers(container: &[u8]) -> Vec<u8> {
    let Ok(meshes) = read_model_meshes(container) else {
        return Vec::new();
    };
    let mut bits: Vec<u8> = Vec::new();
    for m in &meshes {
        for b in 0..8u8 {
            let bit = 1u8 << b;
            if m.state_mask & bit != 0 && !bits.contains(&bit) {
                bits.push(bit);
            }
        }
    }
    bits.sort_unstable();
    bits
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
/// Build the model WHOLE — every drawing group, every segment, nothing filtered — with each
/// [`DrawGroup`] tagged by its `lod_mask` / `node` so visibility can be decided per-frame at DRAW
/// time against the object's [`crate::render_state::RenderState`], the way the engine does it.
///
/// This is the target shape: the retail engine uploads one vertex buffer per model and gates each
/// segment in the draw loop. Baking a visibility decision into the buffer (as [`build_indexed_state`]
/// does) freezes LOD forever and makes two instances of the same model unable to differ in damage
/// state. Prefer this.
pub fn build_indexed_all(
    container: &[u8],
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_filtered(container, None)
}

pub fn build_indexed_state(
    container: &[u8],
    active_bit: u8,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_filtered(container, Some(active_bit))
}

/// `lod_filter = Some(bit)` keeps the legacy build-time filter (a segment survives if its mask is 0
/// or shares a bit with `bit`); `None` keeps everything. The `mask == 0 → always keep` special case is
/// a LEGACY quirk: under the engine's real ANY-bit rule a zero mask overlaps nothing and would never
/// draw. It is preserved here only so `build_indexed_state` stays bit-identical to what shipped.
fn build_indexed_filtered(
    container: &[u8],
    lod_filter: Option<u8>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_rung(container, None, lod_filter)
}

/// Build one LOD rung of a model. A vehicle's geometry is split across blocks: the RESIDENT block
/// ships the object (HIER, SEGM, MTRL, physics, destruction machine) plus the coarsest meshes, and
/// each finer block ships geometry + `INDX`/`PRMT` only. So a fine rung binds its groups against the
/// resident block's SEGM (segment -> node + LOD mask) and MTRL (material table), while its own PRMT
/// rows still choose which material each group uses. `resident = None` for a self-contained
/// container — the resident rung itself, or a character, which ships no chain.
pub fn build_indexed_rung(
    container: &[u8],
    resident: Option<&[u8]>,
    lod_filter: Option<u8>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    build_indexed_source(container, resident, lod_filter, mercs2_formats::texture::MtrlSource::Model)
}

/// The registered pixel shaders of terrain ground, by the detail layers they blend.
const TERRAIN_PIXEL_SHADERS: [&str; 5] =
    ["PgTerrainMeshFP", "PgTerrainMeshFP1D", "PgTerrainMeshFP2D", "PgTerrainMeshFP3D", "PgTerrainMeshFP4D"];

/// Build a hi-res terrain cell (`terrainmesh`, `0x7C569307`) at its world position `pos`, the way
/// `FUN_004AAAB0` draws it near the camera: every patch's draw groups at tier 0, with the material each
/// `PRMT` record names, one [`DrawGroup`] per material.
///
/// A cell is more than its ground. Each patch has one ground group, whose vertices have no texture
/// coordinate and whose materials are `PgTerrainMeshFP*` ([`TerrainGround`]), and overlay groups (the
/// roads, paving and decals, on `PgMeshVP`/`PgDiffSpecNorm*` materials) with authored coordinates and
/// tangents, which are ordinary material draws.
pub fn build_terrain_cell(
    container: &[u8],
    pos: [f32; 3],
    fade: mercs2_formats::terrainmesh::TerrainFade,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>), String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_formats::terrainmesh::{destrip, TerrainCell};

    let cell = TerrainCell::decode(container)?;
    let materials = mercs2_formats::texture::parse_mtrl(container, mercs2_formats::texture::MtrlSource::TerrainMesh)
        .map_err(|e| format!("MTRL: {e}"))?;
    if materials.len() != cell.mtrl.materials.len() {
        return Err(format!("MTRL: {} materials parsed, {} decoded", materials.len(), cell.mtrl.materials.len()));
    }
    let ground_shaders = TERRAIN_PIXEL_SHADERS.map(pandemic_hash_m2);
    let mut verts = Vec::new();
    // One material's triangles across the cell's patches, in the order the materials first draw: a
    // retail draw per patch exists to cull the patch, which the renderer does not do.
    let mut by_material: Vec<(u32, Vec<u32>)> = Vec::new();
    for geom in &cell.geoms {
        for p in &geom.prmgs {
            let base = verts.len() as u32;
            for i in 0..p.vertex_count() {
                let at = p.position(i);
                verts.push(Vertex {
                    pos: [0, 1, 2].map(|k| at[k] + geom.poff[k] + pos[k]),
                    color: p.color(i)?.unwrap_or([1.0; 4]),
                    uv: p.texcoord(i)?.unwrap_or([0.0; 2]),
                    normal: p.normal(i),
                    tangent: p.tangent(i)?.unwrap_or([1.0, 0.0, 0.0, 1.0]),
                    joints: [0; 4],
                    weights: [255, 0, 0, 0],
                });
            }
            let pass = &p.pass_groups[0];
            let first = pass.first_draw as usize;
            for d in &p.draws[first..first + pass.draw_count as usize] {
                if d.material as usize >= materials.len() {
                    return Err(format!("a draw names material {} of {}", d.material, materials.len()));
                }
                let at = match by_material.iter().position(|(m, _)| *m == d.material) {
                    Some(at) => at,
                    None => {
                        by_material.push((d.material, Vec::new()));
                        by_material.len() - 1
                    }
                };
                for t in destrip(&p.indices[p.draw_range(d)?]) {
                    by_material[at].1.extend(t.map(|i| base + i as u32));
                }
            }
        }
    }
    let (mut indices, mut draws) = (Vec::new(), Vec::new());
    for (material, tris) in by_material {
        let m = &materials[material as usize];
        let ground = if ground_shaders.contains(&m.shader_key) {
            Some(TerrainGround {
                global: m.slot(0).ok_or_else(|| format!("ground material {material} has no global map"))?,
                detail: m.slot(1),
                layers: [0, 1, 2, 3].map(|n| m.slot(2 + 2 * n)),
                layer_uv: [0, 1, 2, 3].map(|n| cell.mtrl.layer_uv(material as usize, n)),
                uv_scale: cell.mtrl.materials[material as usize].uv_scale(),
                centre: [pos[0], pos[2]],
                fade,
            })
        } else {
            None
        };
        let index_start = indices.len() as u32;
        indices.extend(tris);
        draws.push(DrawGroup {
            index_start,
            index_count: indices.len() as u32 - index_start,
            diffuse: m.diffuse(),
            specular: m.specular(),
            normal: m.slot(2),
            textures: m.textures.clone(),
            material_flags: m.flags,
            shader_key: m.shader_key,
            material_data: m.material_data(),
            ground,
            ..Default::default()
        });
    }
    Ok((verts, indices, draws))
}

fn build_indexed_source(
    container: &[u8],
    resident: Option<&[u8]>,
    lod_filter: Option<u8>,
    mtrl_source: mercs2_formats::texture::MtrlSource,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, ModelStats), String> {
    use mercs2_formats::model_cubeize::{read_model_meshes_segm, ModelMesh};
    use mercs2_formats::skeleton::{
        affine_inverse, mat4_mul, transform_dir, transform_point, Skeleton,
    };

    let res_segm = resident.map(mercs2_formats::model_cubeize::parse_segm);
    let meshes = read_model_meshes_segm(container, res_segm.as_deref())?;
    let materials = mercs2_formats::texture::parse_mtrl(resident.unwrap_or(container), mtrl_source)
        .map_err(|e| format!("MTRL: {e}"))?;
    let group_mat = mercs2_formats::texture::group_material_indices(container);

    // Skeleton world-rest per bone, for placing rigid MESH accessories. from_block wants a
    // 20-byte wrapper + UCFX. HIER lives only in the resident block, so a fine rung — whose SEGM
    // records name nodes in that same hierarchy — must mount against the resident skeleton.
    let skel_src = resident.unwrap_or(container);
    let mut block = Vec::with_capacity(20 + skel_src.len());
    block.extend_from_slice(&[0u8; 20]);
    block[16..20].copy_from_slice(&(skel_src.len() as u32).to_le_bytes());
    block.extend_from_slice(skel_src);
    let skel = Skeleton::from_block(&block).ok();

    // The SKIN bind of each bone: the inverse of the HIER record's `+80` inverse-bind matrix, taken
    // RAW (`Bone::bind_world_raw`) — read off retail's live skin palette: in the
    // `23fe7f38_cracked-cruise/pmc` capture, solving `palette · pose⁻¹` for `pmc_hum_mattias` gives
    // the raw `+80` on every bone where `+80` and the `+16` chain differ (#3, #5–#8, #22–#24, #61,
    // #83), including the records `Skeleton` calls stale (an ancestor's matrix copied onto a child).
    // Those are rigid accessory pieces retail places offset from their authored spot on purpose —
    // its live pose keeps their `+16` local (bone #5 sits 158 mm from #4) while skinning them with
    // the parent's inverse-bind. `Skeleton`'s staleness rejection is an AUTHORING judgement and is
    // not applied here. Test: `mercs2_probe` `retail_skin_matches_the_live_palette`.
    //
    // It is NOT the `+16` local chain (`Bone::world`). The two agree on most models but not all:
    // `pmc_hum_chris`'s `+16` locals hold a weapon-carry pose that puts his forearm-roll bones at
    // local T (0.0805, -0.0859, -0.0685), while his `+80` binds — and every clip — put them at
    // (0.1721, 0, 0). Skinning against the `+16` chain tore his arms by 143 mm under animation;
    // retail renders him intact. `+16` remains the Havok REFERENCE pose (`local_bind` below; read
    // live from retail's `hkaSkeleton`), which drives bones a clip does not animate.
    //
    // A node whose `+80` is absent/singular has no authored bind at all. That is only legal on a
    // node no skinned vertex depends on: a node-placed or rigid group is positioned by
    // `skin_bind[b]` and skinned by its inverse, so whichever bind it carries cancels exactly. A
    // skinned vertex that weights such a node is a load error (checked below, once the meshes are
    // read).
    let skin_bind: Vec<[[f32; 4]; 4]> = match &skel {
        Some(s) => s.bones.iter().map(|b| b.bind_world_raw.unwrap_or(b.world)).collect(),
        None => Vec::new(),
    };

    // The REST palette: Skin[b] = InvBind[b] · Pose[b] (row-vector) with Pose = the REFERENCE pose,
    // the `+16` local chain — what retail holds for a node no clip drives. Read off retail's live
    // per-node matrices (`23fe7f38_cracked-cruise/pmc`, model `0x1FBFBB4B`, the array at OBJ+0x2B8):
    // all 73 equal the `+16` chain, while `+80` matches only the 8 nodes that own geometry. It is NOT
    // the `+80` bind (Skin = identity): a vehicle's part nodes carry their PARENT's `+80` (the M1A2
    // turret's is the hull origin, 1.63 m below its `+16` mount), and an instance node carries a copy
    // of its source's, so an identity rest sank every turret and stacked every instanced wheel.
    // Animation swaps Pose per frame.
    let bones: Vec<[[f32; 4]; 4]> = match &skel {
        Some(s) => s.bones.iter().zip(&skin_bind).map(|(b, sb)| mat4_mul(&affine_inverse(sb), &b.world)).collect(),
        None => Vec::new(),
    };

    // Per-bone rig for animation: parent, the skin bind + its inverse, and the REFERENCE-pose LOCAL
    // transform (`+16`: local_b = world_b · inv(world_parent); root's local = its world).
    let rig: Vec<BoneRig> = match &skel {
        Some(s) => s
            .bones
            .iter()
            .zip(&skin_bind)
            .map(|(b, sb)| {
                let local_bind = if b.parent < 0 {
                    b.world
                } else {
                    mat4_mul(&b.world, &affine_inverse(&s.bones[b.parent as usize].world))
                };
                BoneRig {
                    parent: b.parent,
                    name_hash: b.name_hash,
                    world_bind: *sb,
                    inv_bind: affine_inverse(sb),
                    local_bind,
                }
            })
            .collect(),
        None => Vec::new(),
    };

    // NOTE on `lod_filter`: `SEGM.state_mask` is the set of LOD RUNGS a segment appears at (md500:
    // 7 = rungs 0-2, 112 = rungs 4-6, 127 = all), so filtering to one bit bakes one rung into the
    // vertex buffer. It also does NOT hide the wreck — that is the node-enable table, clause 3 of the
    // gate, which this builder cannot see. `None` = keep everything and gate at draw time.

    // Per kept group: world-space geometry (rigid MESH groups transformed by their bone's rest).
    struct Placed<'a> {
        m: &'a ModelMesh,
        /// The bone this group was placed by (SEGM bone for MESH accessories, INDX node for
        /// blend-less PRMG groups) — the whole group binds 100% to it.
        placed_bone: Option<usize>,
        /// The SEGM row this placement serves, and that row's node + LOD mask — the source's own
        /// row for geometry reached through INDX, another row for an instance.
        row: usize,
        node: i16,
        lod_mask: u8,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
    }

    // Per-PRMG-group POFF (parent GEOM position offset). Needed so multi-GEOM meshes (the terrainmesh:
    // 16 sub-tiles at ±150/±50) don't collapse onto each other; single-GEOM models get [0,0,0].
    let poffs = mercs2_formats::model_cubeize::prmg_geom_offsets(container);

    // A group's geometry in model space: bone-local vertices through the placing bone's bind (none =
    // already model-space), then the group's POFF (parent-GEOM translation; non-zero only for
    // multi-GEOM meshes).
    let place = |m: &ModelMesh, bonemat: Option<[[f32; 4]; 4]>| {
        let (mut positions, normals, tangents): (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 4]>) =
            if let Some(w) = bonemat {
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
        if let Some(off) = poffs.get(m.group_index) {
            if *off != [0.0, 0.0, 0.0] {
                for p in positions.iter_mut() {
                    p[0] += off[0];
                    p[1] += off[1];
                    p[2] += off[2];
                }
            }
        }
        (positions, normals, tangents)
    };

    // INDX: mesh-group → HIER node. Groups with NO blend data (no BLENDINDICES/WEIGHT elements
    // in their decl — game data, not inference) are NODE-RIGID: authored node-local and placed
    // by their INDX node's world-rest, exactly the "INDX-based direct mapping (authoritative)"
    // rule the export codec (`ucfx_mesh_codec.py`) applies. Without it, vehicle panels/wheels
    // render piled around the origin instead of assembled.
    let indx = mercs2_formats::orchestrator::parse_indx(container);

    let mut skipped = 0usize;
    let mut kept: Vec<Placed> = Vec::new();
    for m in &meshes {
        // Clause (1) of the engine's draw gate: a group whose PRMT draw count is zero is not drawn,
        // whatever its index buffer still contains. This is the field injection zeroes to neutralise
        // the host groups it did not fill, and everything else here derives from PRMT index SPANS,
        // which neutralisation leaves intact -- so without this test an injected character renders
        // the DONOR's leftover head, hair and gear inside the import.
        //
        // Safe by measurement, not by assumption: across 600 retail models / 4,871 groups
        // (`drawgate_scan`), not one group has a zero draw count, so this can only ever drop
        // geometry the engine was already refusing to draw.
        if m.prmt_draw == 0 {
            skipped += 1;
            continue;
        }
        if let Some(bit) = lod_filter {
            if m.state_mask != 0 && (m.state_mask & bit) == 0 {
                skipped += 1; // segment absent from the selected LOD rung
                continue;
            }
        }
        // Placement bone: MESH rigid accessories use their SEGM bone; blend-less PRMG groups use
        // their INDX node (see above). Skinned groups (blend data present) are model-space.
        let placed_bone: Option<usize> = if m.rigid {
            Some(m.bone as usize)
        } else if m.joints.is_empty() {
            indx.get(m.group_index).copied()
        } else {
            None
        };
        // Placed by the SAME bind the palette inverts (see `skin_bind`), so pose·local is exact.
        let bonemat = placed_bone.and_then(|b| skin_bind.get(b)).copied();
        // A skinned vertex must weight only nodes that carry an authored `+80` bind.
        if placed_bone.is_none() {
            if let Some(s) = &skel {
                for (j, w) in m.joints.iter().zip(&m.weights) {
                    for k in 0..4 {
                        let b = j[k] as usize;
                        if w[k] > 0 && s.bones.get(b).is_some_and(|bone| bone.bind_world_raw.is_none()) {
                            return Err(format!(
                                "group {}: skinned vertex weights HIER node {b} (0x{:08X}), which has no invertible +80 inverse-bind",
                                m.group_index,
                                s.bones[b].name_hash
                            ));
                        }
                    }
                }
            }
        }
        let (positions, normals, tangents) = place(m, bonemat);
        kept.push(Placed {
            m,
            placed_bone,
            row: m.seg_id,
            node: m.bone as i16,
            lod_mask: m.state_mask,
            positions,
            normals,
            tangents,
        });
    }

    // INSTANCE rows. Retail draws per SEGM ROW (`FUN_00472a50`): row r draws `renderable[seg_id]`,
    // and the matrix slot comes from the row's own node alone (`FUN_004778b0` reads rec+0, never the
    // seg_id). So a row whose `seg_id` names ANOTHER row's geometry draws that geometry again, at its
    // own node, under its own LOD mask. Reaching SEGM only through INDX (above) never finds these:
    // `SEGM[i].seg_id == i` is NOT an invariant. 251 retail models carry 3,739 such rows — the M1A2's
    // rows 74..99 put its one road-wheel mesh (segs 72/73) on the other 13 hubs; cars re-draw three
    // wheels. A source lives in exactly one rung's geometry, so a rung that lacks it draws none here.
    let segm_rows = match &res_segm {
        Some(s) => s.clone(),
        None => mercs2_formats::model_cubeize::parse_segm(container),
    };
    let mut instances: Vec<Placed> = Vec::new();
    for (row, rec) in segm_rows.iter().enumerate() {
        let src = rec.seg_id as usize;
        if src == row {
            continue;
        }
        if let Some(bit) = lod_filter {
            if rec.state_mask != 0 && (rec.state_mask & bit) == 0 {
                continue;
            }
        }
        for pl in kept.iter().filter(|pl| pl.row == src) {
            // Every instance measured has a node-placed source. A deforming skin carries its own
            // per-vertex bones, and what re-drawing it at another node means is not known.
            if pl.placed_bone.is_none() {
                return Err(format!(
                    "SEGM row {row} instances seg {src} (group {}), a deforming skin — instancing a skin is not modelled",
                    pl.m.group_index
                ));
            }
            let node = rec.bone as i16;
            let bonemat = usize::try_from(node).ok().and_then(|b| skin_bind.get(b)).copied().ok_or_else(|| {
                format!("SEGM row {row} instances seg {src} at node {node}, which has no bind ({} HIER nodes)", skin_bind.len())
            })?;
            let (positions, normals, tangents) = place(pl.m, Some(bonemat));
            instances.push(Placed {
                m: pl.m,
                placed_bone: Some(node as usize),
                row,
                node,
                lod_mask: rec.state_mask,
                positions,
                normals,
                tangents,
            });
        }
    }
    kept.extend(instances);
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
        // Placed groups (MESH accessories + blend-less node-rigid PRMG groups) are
        // pre-transformed into bind space above; bind them 100% to their placement bone so the
        // same LBS palette carries them (and follows the bone under animation). Skinned bodies
        // keep their extracted BLENDINDICES/BLENDWEIGHT. Positions stay in model space — the fit
        // (center/scale) is applied by the camera MVP so skinning is model-space.
        let rigid_bind: Option<([u8; 4], [u8; 4])> =
            pl.placed_bone.map(|b| ([b as u8, 0, 0, 0], [255, 0, 0, 0]));
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
                // Per-vertex COLOR (D3DCOLOR, stored B,G,R,A) → RGBA. For static interior/building meshes
                // this is BAKED vertex lighting (the Pandemic-era interior light); the shader multiplies
                // it into albedo. Default WHITE (no-op) for meshes with no COLOR element — NOT 0.5, which
                // was silently halving every texture.
                color: m
                    .colors
                    .get(vi)
                    .map(|c| [c[2] as f32 / 255.0, c[1] as f32 / 255.0, c[0] as f32 / 255.0, c[3] as f32 / 255.0])
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
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
                normal: mat.and_then(|m| m.slot(2)),
                // Every slot, not just the first three. A material can bind up to 10 textures,
                // and a tool asking "which parts use texture X" gets the wrong answer if it can
                // only see slots 0-2 — on retail that alone is the difference between 40% and
                // 89% of textures being attributable to a surface.
                textures: mat.map(|m| m.textures.clone()).unwrap_or_default(),
                material_flags: mat.map_or(0, |m| m.flags),
                shader_key: mat.map_or(0, |m| m.shader_key),
                material_data: mat.map_or(NO_MATERIAL_DATA, |m| m.material_data()),
                group_index: m.group_index,
                lod_mask: pl.lod_mask,
                // SEGM +0 is a SIGNED i16: 0xFFFF is node -1 ("no node"), not node 65535.
                node: pl.node,
                sub_object: m.sub_object,
                seg_id: m.seg_id,
                segm_row: pl.row,
                // This builder reads ONE container and can't know where it sits in the chain;
                // `Model::load` stamps the real rung once it has walked the LOD blocks.
                rung: 0,
                // No placement bone = the group kept its own blend data = a real deforming skin.
                skinned: pl.placed_bone.is_none(),
                ground: None,
                scrub: None,
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
                    1.0,
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
    };
    Ok((verts, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENT: [[f32; 4]; 4] =
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

    fn vert(joint: u8) -> Vertex {
        Vertex {
            pos: [0.0; 3],
            color: [1.0; 4],
            uv: [0.0; 2],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [joint, 0, 0, 0],
            weights: [255, 0, 0, 0],
        }
    }

    fn bone(parent: i32) -> BoneRig {
        BoneRig { parent, name_hash: 0, world_bind: IDENT, inv_bind: IDENT, local_bind: IDENT }
    }

    #[test]
    fn an_unrigged_skin_rests_at_identity_over_every_referenced_joint() {
        let rest = SkinData::identity().rest_palette(&[vert(0), vert(3)]).unwrap();
        assert_eq!(rest, vec![IDENT; 4]);
    }

    #[test]
    fn a_rigged_skin_rests_at_its_bones() {
        let mut lifted = IDENT;
        lifted[3][1] = 1.63;
        let skin = SkinData { bones: vec![IDENT, lifted], rig: vec![bone(-1), bone(0)], ..SkinData::identity() };
        assert_eq!(skin.rest_palette(&[vert(1)]).unwrap(), vec![IDENT, lifted]);
    }

    #[test]
    fn a_rigged_skin_whose_verts_outrun_its_palette_is_an_error() {
        let skin = SkinData { bones: vec![IDENT; 2], rig: vec![bone(-1), bone(0)], ..SkinData::identity() };
        let e = skin.rest_palette(&[vert(4)]).unwrap_err();
        assert!(e.contains("2-entry rest palette for 5 referenced joints"), "{e}");
    }
}
