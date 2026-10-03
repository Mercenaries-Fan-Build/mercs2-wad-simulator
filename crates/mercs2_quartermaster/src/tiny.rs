//! `add_tiny_geometry`: a TINY far-distance stand-in and the `TinyGeometryObject` placement that
//! loads it.
//!
//! A TINY model ([`mercs2_formats::tiny_model`]) draws the world objects of one 200 m grid cell of a
//! layer from far away. Its top-level `TINY` chunk lists the objects' GUIDs in ascending order; each
//! vertex's `POSITION.w` is the slot of its object in that list and in `ObjectIDScaleArray`, whose
//! entry is the object's state: 1 intact and drawn, 2 intact and hidden, 3 ruined and drawn, 4
//! ruined and hidden. `PgMeshTinyVP` keeps a vertex while its slot's state is 1, `PgMeshTinyVP_Ruin`
//! while it is 3.
//!
//! **Slot 3 of every register is never used.** The shaders read slot `w` as component `w mod 4` of
//! constant `trunc(w / 4)`, and the compiler computes component 3 as `2·.w − .y`: a vertex at a slot
//! `≡ 3 (mod 4)` is kept only when twice its own state minus the state of the slot two below is 1
//! (intact) or 3 (ruined), so it disappears or shows in the wrong role whenever the two objects'
//! states differ. [`plan_slots`] gives the objects the other slots and fills each skipped one with
//! a GUID no object has.
//!
//! **The list is searched.** The engine finds an object's slot by binary search over the list
//! (`0x004ADF40`, `0x00515BA0`, the relocated bodies of `0x0050F530` and `0x0050F590`), so it is
//! ascending, and a filler between two consecutive GUIDs repeats a neighbour only where the search
//! still finds every object at its own slot ([`engine_slot`] runs that search).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mesh_import::{source_from_gltf, AlphaMode, SourceMaterial, SourcePrimitive};
use mercs2_formats::model_inject::{f16_le, read_f16_le, to_strip};
use mercs2_formats::placement_build::{insert_entity, NewEntity, FLGS_TINY_GEOMETRY_OBJECT};
use mercs2_formats::tiny_model::{self as tm, PrimGroup, SegmRecord, SubObject, TinyModel, TinyVertex};

/// Cells per side of the grid.
pub const GRID: u32 = 40;
/// A cell's side, metres.
pub const CELL: f32 = 200.0;
/// The grid's low corner on x and z.
pub const ORIGIN: f32 = -4000.0;
/// The slot list's count is a byte (`0x0050F42B`), so it holds at most 255 GUIDs.
pub const MAX_SLOTS: usize = 255;
/// Of slots 0..255, the ones not `≡ 3 (mod 4)`: the most objects one stand-in draws.
pub const MAX_OBJECTS: usize = 192;
/// The registry of slot lists holds 1,400 containers (`0x0050F1BE`, `0x0050F26C`).
pub const MAX_CONTAINERS: usize = 1400;
/// `ObjectIDScaleArray` is 200 registers (`c0`..`c199`) in all six TINY vertex shaders.
pub const SCALE_REGISTERS: usize = 200;

/// The extras key a primitive declares its role under.
pub const ROLE_KEY: &str = "tiny_role";

/// A TINY primitive's role: which shader draws it, and so which state of its object shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// `PgMeshTinyVP`: drawn while the object is intact and drawn (state 1).
    Intact,
    /// `PgMeshTinyVP_Ruin`: drawn while the object is ruined and drawn (state 3).
    Ruined,
}

impl Role {
    pub fn token(self) -> &'static str {
        match self {
            Role::Intact => "intact",
            Role::Ruined => "ruined",
        }
    }
    fn parse(s: &str) -> Option<Role> {
        match s {
            "intact" => Some(Role::Intact),
            "ruined" => Some(Role::Ruined),
            _ => None,
        }
    }
    /// The `HIER` node name.
    pub fn node(self) -> &'static str {
        match self {
            Role::Intact => tm::NODE_PRISTINE,
            Role::Ruined => tm::NODE_RUIN,
        }
    }
    pub fn vertex_shader(self) -> &'static str {
        match self {
            Role::Intact => "PgMeshTinyVP",
            Role::Ruined => "PgMeshTinyVP_Ruin",
        }
    }
    /// The shadow vertex shader: a `Tex` one when the material is alpha-tested.
    pub fn shadow_vertex_shader(self, alphatest: bool) -> &'static str {
        match (self, alphatest) {
            (Role::Intact, false) => "PgMeshTinyShadowVP",
            (Role::Intact, true) => "PgMeshTinyShadowTexVP",
            (Role::Ruined, false) => "PgMeshTinyShadowVP_Ruin",
            (Role::Ruined, true) => "PgMeshTinyShadowTexVP_Ruin",
        }
    }
}

// ── the grid ────────────────────────────────────────────────────────────────────────────────────

/// The cell holding `pos`, as `(row, col)`, or `None` outside the grid.
pub fn cell_of(pos: [f32; 3]) -> Option<(u32, u32)> {
    let col = ((pos[0] - ORIGIN) / CELL).floor();
    let row = ((pos[2] - ORIGIN) / CELL).floor();
    let inside = |v: f32| (0.0..GRID as f32).contains(&v);
    (inside(col) && inside(row)).then_some((row as u32, col as u32))
}

/// The centre of a cell, where its `TinyGeometryObject` placement sits: `(-3900 + 200·col, 0,
/// -3900 + 200·row)`.
pub fn cell_centre(row: u32, col: u32) -> [f32; 3] {
    [ORIGIN + CELL * 0.5 + CELL * col as f32, 0.0, ORIGIN + CELL * 0.5 + CELL * row as f32]
}

/// The model's name: `<layer>_tinygeometry_tgr<row>_tgc<col>_0x<key>`.
pub fn model_name(layer: &str, row: u32, col: u32, key: u32) -> String {
    format!("{layer}_tinygeometry_tgr{row:02}_tgc{col:02}_0x{key:08x}")
}

/// The placement's name: `tinygeometry_tgr<row>_tgc<col>` (its `Name` record adds ` 0x<key>`).
pub fn placement_name(row: u32, col: u32) -> String {
    format!("tinygeometry_tgr{row:02}_tgc{col:02}")
}

/// `(layer, row, col, key)` of a model named by [`model_name`].
pub fn parse_model_name(name: &str) -> Option<(String, u32, u32, u32)> {
    let (head, key) = name.rsplit_once("_0x")?;
    let key = u32::from_str_radix(key, 16).ok().filter(|_| key.len() == 8)?;
    let (head, col) = head.rsplit_once("_tgc")?;
    let (layer, row) = head.rsplit_once("_tinygeometry_tgr")?;
    let (row, col) = (row.parse().ok()?, col.parse().ok()?);
    (!layer.is_empty()).then(|| (layer.to_string(), row, col, key))
}

// ── the slot list ───────────────────────────────────────────────────────────────────────────────

/// The engine's slot search: a binary search over the list with a signed low/high pair and an
/// unsigned compare (`0x004ADF40`: `mid = (lo + hi) >> 1`; above the key, `hi = mid - 1`; equal,
/// found; below, `lo = mid + 1`).
pub fn engine_slot(list: &[u32], key: u32) -> Option<usize> {
    let (mut lo, mut hi) = (0i32, list.len() as i32 - 1);
    while lo <= hi {
        let mid = (lo + hi) >> 1;
        let v = list[mid as usize];
        if v > key {
            hi = mid - 1;
        } else if v == key {
            return Some(mid as usize);
        } else {
            lo = mid + 1;
        }
    }
    None
}

/// The slot list of a stand-in and where each object sits in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPlan {
    /// The container's `TINY` list: the objects' GUIDs ascending, with a filler at every slot
    /// `≡ 3 (mod 4)` below the last object.
    pub list: Vec<u32>,
    /// `slot_of[i]` is the slot of `objects[i]`.
    pub slot_of: Vec<u16>,
}

/// Lay `objects` (GUIDs, in manifest order) out in a slot list: ascending, never at a slot
/// `≡ 3 (mod 4)`. Each skipped slot holds a filler: the first GUID between its neighbours that
/// `taken` does not hold, or, where there is none, a repeat of a neighbour that leaves
/// [`engine_slot`] finding every object at its own slot.
pub fn plan_slots(objects: &[u32], taken: &dyn Fn(u32) -> bool) -> Result<SlotPlan, String> {
    if objects.is_empty() {
        return Err("a stand-in draws at least one object".into());
    }
    let unique: BTreeSet<u32> = objects.iter().copied().collect();
    if unique.len() != objects.len() {
        return Err("two objects are the same GUID".into());
    }
    if objects.len() > MAX_OBJECTS {
        return Err(format!(
            "{} objects; a stand-in draws at most {MAX_OBJECTS}: the slot list holds {MAX_SLOTS} and \
             every slot ≡ 3 (mod 4) is skipped",
            objects.len()
        ));
    }
    let sorted: Vec<u32> = unique.into_iter().collect();
    // Slots: objects at the slots not ≡ 3; a filler at each ≡ 3 slot that has an object after it.
    let mut list: Vec<Option<u32>> = Vec::new();
    for &g in &sorted {
        if list.len() % 4 == 3 {
            list.push(None);
        }
        list.push(Some(g));
    }
    let mut choices: Vec<(usize, Vec<u32>)> = Vec::new();
    for p in 0..list.len() {
        if list[p].is_some() {
            continue;
        }
        let left = list[p - 1].expect("an object before a filler");
        let right = list[p + 1].expect("an object after a filler");
        let free = (left.saturating_add(1)..right).take(4096).find(|&v| !taken(v));
        let options = match free {
            Some(v) => vec![v],
            None => vec![left, right],
        };
        choices.push((p, options));
    }
    // Repeats are tried left neighbour first, each choice kept when every object is still found.
    let mut out: Vec<u32> = list.iter().map(|s| s.unwrap_or(0)).collect();
    for (p, options) in &choices {
        out[*p] = options[0];
    }
    let finds_all = |l: &[u32]| {
        list.iter().enumerate().all(|(i, s)| s.is_none_or(|g| engine_slot(l, g) == Some(i)))
    };
    for (p, options) in &choices {
        if options.len() == 1 {
            continue;
        }
        let ok = options.iter().any(|&v| {
            out[*p] = v;
            finds_all(&out)
        });
        if !ok {
            return Err(format!(
                "no filler at slot {p} between 0x{:08X} and 0x{:08X} lets the engine's search find \
                 every object at its slot",
                out[p - 1],
                out[p + 1]
            ));
        }
    }
    if !finds_all(&out) {
        return Err("the engine's search does not find every object at its slot".into());
    }
    let slot_of = objects
        .iter()
        .map(|g| list.iter().position(|s| *s == Some(*g)).expect("every object is laid out") as u16)
        .collect();
    Ok(SlotPlan { list: out, slot_of })
}

// ── the source ──────────────────────────────────────────────────────────────────────────────────

/// A material of the source, as the stand-in takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMat {
    /// `alphaMode: MASK`.
    pub alphatest: bool,
    /// `extras.texture`: the diffuse texture, a name or a bare `0xHASH`.
    pub texture: Option<String>,
    /// `extras.pixel_shader`.
    pub pixel_shader: Option<String>,
    /// `alphaMode: BLEND`, which a stand-in has no material for.
    pub blend: bool,
}

/// A primitive of the source with its role read.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePrim {
    pub prim: SourcePrimitive,
    /// `extras.tiny_role`, as written.
    pub role: Option<String>,
}

impl SourcePrim {
    fn at(&self) -> String {
        format!("mesh {} primitive {}", self.prim.at.0, self.prim.at.1)
    }

    /// The primitive's triangles, as vertex triples; a strip's degenerate triangles left out.
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        let i = &self.prim.indices;
        if self.prim.strip {
            (0..i.len().saturating_sub(2))
                .map(|k| if k % 2 == 0 { [i[k], i[k + 1], i[k + 2]] } else { [i[k + 1], i[k], i[k + 2]] })
                .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
                .collect()
        } else {
            i.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect()
        }
    }
}

/// A stand-in's source glTF.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub prims: Vec<SourcePrim>,
    pub materials: Vec<SourceMat>,
}

fn extras_object(raw: &Option<String>, what: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match raw {
        None => Ok(serde_json::Map::new()),
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(serde_json::Value::Object(m)) => Ok(m),
            Ok(_) => Err(format!("{what}: extras is not an object")),
            Err(e) => Err(format!("{what}: extras: {e}")),
        },
    }
}

fn extras_string(
    m: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    what: &str,
) -> Result<Option<String>, String> {
    match m.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{what}: extras.{key} is not a string")),
    }
}

/// Read a stand-in's source.
pub fn read_source(path: &Path) -> Result<Source, String> {
    let (prims, mats) = source_from_gltf(path)?;
    let materials = mats
        .iter()
        .enumerate()
        .map(|(k, m): (usize, &SourceMaterial)| {
            let what = format!("{}: material {k}", path.display());
            let ex = extras_object(&m.extras, &what)?;
            Ok(SourceMat {
                alphatest: m.alpha_mode == AlphaMode::Mask,
                texture: extras_string(&ex, "texture", &what)?,
                pixel_shader: extras_string(&ex, "pixel_shader", &what)?,
                blend: m.alpha_mode == AlphaMode::Blend,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let prims = prims
        .into_iter()
        .map(|p| {
            let what = format!("{}: mesh {} primitive {}", path.display(), p.at.0, p.at.1);
            let ex = extras_object(&p.extras, &what)?;
            let role = extras_string(&ex, ROLE_KEY, &what)?;
            Ok(SourcePrim { prim: p, role })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Source { prims, materials })
}

/// A problem with a source, under the rule that reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    pub message: String,
}

/// The source's role and slot problems for a stand-in of `objects` objects: a primitive with no
/// role or another (M0244), a vertex with no slot or one past the objects (M0242), a triangle whose
/// vertices name two slots (M0243).
pub fn source_problems(source: &Source, objects: usize) -> Vec<Problem> {
    let mut out = Vec::new();
    for p in &source.prims {
        match p.role.as_deref() {
            None => out.push(Problem {
                code: "M0244",
                message: format!("{} declares no extras.{ROLE_KEY} (intact or ruined)", p.at()),
            }),
            Some(r) if Role::parse(r).is_none() => out.push(Problem {
                code: "M0244",
                message: format!("{} declares extras.{ROLE_KEY} {r:?}; it is intact or ruined", p.at()),
            }),
            Some(_) => {}
        }
        let Some(slots) = &p.prim.slots else {
            out.push(Problem {
                code: "M0242",
                message: format!(
                    "{} has no {} attribute; every vertex names the object it draws",
                    p.at(),
                    mercs2_formats::mesh_import::TINY_SLOT_ATTRIBUTE
                ),
            });
            continue;
        };
        if let Some((v, s)) = slots.iter().enumerate().find(|(_, &s)| s as usize >= objects) {
            out.push(Problem {
                code: "M0242",
                message: format!(
                    "{} vertex {v} has slot {s}; the stand-in lists {objects} object(s), slots 0 to {}",
                    p.at(),
                    objects.saturating_sub(1)
                ),
            });
        }
        if let Some(t) = p.triangles().into_iter().find(|t| {
            let s = |i: u32| slots.get(i as usize).copied();
            s(t[0]) != s(t[1]) || s(t[1]) != s(t[2])
        }) {
            let s = |i: u32| slots.get(i as usize).copied().unwrap_or(u32::MAX);
            out.push(Problem {
                code: "M0243",
                message: format!(
                    "{} triangle ({}, {}, {}) spans slots {}, {} and {}; a triangle draws one object",
                    p.at(),
                    t[0],
                    t[1],
                    t[2],
                    s(t[0]),
                    s(t[1]),
                    s(t[2])
                ),
            });
        }
    }
    out
}

/// The source's shape problems (M0251): a primitive with no material, NORMAL or TEXCOORD_0, or
/// more than 65,535 vertices; a material drawn by no primitive, alpha-blended, without a texture, or
/// whose pixel shader the convention cannot name and the file does not declare.
pub fn shape_problems(source: &Source) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut p = |message: String| out.push(Problem { code: "M0251", message });
    for prim in &source.prims {
        let at = prim.at();
        match prim.prim.material {
            None => p(format!("{at} has no material")),
            Some(m) if m >= source.materials.len() => p(format!("{at} names material {m}, which the file lacks")),
            Some(_) => {}
        }
        if prim.prim.normals.is_none() {
            p(format!("{at} has no NORMAL"));
        }
        if prim.prim.uvs.is_none() {
            p(format!("{at} has no TEXCOORD_0"));
        }
        if prim.prim.positions.len() > u16::MAX as usize {
            p(format!("{at} has {} vertices; a group indexes at most 65535", prim.prim.positions.len()));
        }
    }
    let used: BTreeSet<usize> = source.prims.iter().filter_map(|p| p.prim.material).collect();
    for k in (0..source.materials.len()).filter(|k| !used.contains(k)) {
        p(format!("material {k} is drawn by no primitive"));
    }
    if let Err(e) = resolve_materials(source) {
        p(e);
    }
    out
}

// ── the model ───────────────────────────────────────────────────────────────────────────────────

fn half(v: f32, what: &str) -> Result<u16, String> {
    if !v.is_finite() || v.abs() > 65504.0 {
        return Err(format!("{what} {v} is outside the half-float range"));
    }
    Ok(u16::from_le_bytes(f16_le(v)))
}

fn unhalf(h: u16) -> f32 {
    read_f16_le(&h.to_le_bytes(), 0)
}

/// Bounds of the stored (half-float) positions: `(min, max)`.
fn bounds(vertices: &[TinyVertex]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for v in vertices {
        for a in 0..3 {
            let x = unhalf(v.position[a]);
            lo[a] = lo[a].min(x);
            hi[a] = hi[a].max(x);
        }
    }
    (lo, hi)
}

fn union(boxes: impl Iterator<Item = ([f32; 3], [f32; 3])>) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for (l, h) in boxes {
        for a in 0..3 {
            lo[a] = lo[a].min(l[a]);
            hi[a] = hi[a].max(h[a]);
        }
    }
    (lo, hi)
}

/// What each material resolves to: its texture's hash and its pixel shader's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMaterial {
    pub texture: u32,
    pub pixel_shader: String,
}

/// Resolve each material's texture and pixel shader: the texture is required; the pixel shader is
/// the declared one or, when the material declares none, the convention's one choice for a
/// material with one texture.
pub fn resolve_materials(source: &Source) -> Result<Vec<ResolvedMaterial>, String> {
    source
        .materials
        .iter()
        .enumerate()
        .map(|(k, m)| {
            if m.blend {
                return Err(format!(
                    "material {k} is alphaMode BLEND; a stand-in material is OPAQUE or MASK (alpha-tested)"
                ));
            }
            let texture = m
                .texture
                .as_deref()
                .map(crate::manifest::asset_hash)
                .ok_or_else(|| format!("material {k} declares no extras.texture (its diffuse texture)"))?;
            let pixel_shader = crate::shader_import::resolve(
                crate::shader_import::Rule::Pixel,
                &crate::shader_import::pixel_input(&[texture]),
                m.pixel_shader.as_deref(),
            )
            .map_err(|e| format!("material {k}: {e}"))?;
            Ok(ResolvedMaterial { texture, pixel_shader })
        })
        .collect()
}

/// Build the stand-in for cell `(row, col)` from its source, slot plan and resolved materials.
///
/// The intact sub-object comes first, then the ruined one, each holding its role's primitives in
/// the file's order, one group per primitive. Every material of the file is used by a primitive and
/// is kept in the file's order. A triangle-strip primitive keeps its strip; a triangle primitive is
/// stitched into one ([`to_strip`]). Positions, texture coordinates and normals are stored as half
/// floats; each vertex's `POSITION.w` is its object's slot and `NORMAL.w` is 1. Bounds are those of
/// the stored positions.
pub fn build_model(
    source: &Source,
    row: u32,
    col: u32,
    plan: &SlotPlan,
    materials: &[ResolvedMaterial],
) -> Result<TinyModel, String> {
    let mut problems = source_problems(source, plan.slot_of.len());
    problems.extend(shape_problems(source));
    if !problems.is_empty() {
        return Err(problems.iter().map(|p| format!("[{}] {}", p.code, p.message)).collect::<Vec<_>>().join("; "));
    }
    if materials.len() != source.materials.len() {
        return Err("every material resolves to a texture and a pixel shader".into());
    }
    let used: BTreeSet<usize> = source.prims.iter().filter_map(|p| p.prim.material).collect();
    if let Some(k) = (0..materials.len()).find(|k| !used.contains(k)) {
        return Err(format!("material {k} is drawn by no primitive"));
    }

    let mut subs: Vec<(Role, Vec<PrimGroup>)> = Vec::new();
    for role in [Role::Intact, Role::Ruined] {
        let mut groups = Vec::new();
        for p in source.prims.iter().filter(|p| p.role.as_deref().and_then(Role::parse) == Some(role)) {
            let at = p.at();
            let mat = p.prim.material.ok_or_else(|| format!("{at} has no material"))?;
            let m = source.materials.get(mat).ok_or_else(|| format!("{at} names material {mat}, which the file lacks"))?;
            let n = p.prim.positions.len();
            if n > u16::MAX as usize {
                return Err(format!("{at} has {n} vertices; a group indexes at most 65535"));
            }
            let normals = p.prim.normals.as_ref().ok_or_else(|| format!("{at} has no NORMAL"))?;
            let uvs = p.prim.uvs.as_ref().ok_or_else(|| format!("{at} has no TEXCOORD_0"))?;
            let slots = p.prim.slots.as_ref().expect("source_problems checked the slots");
            let mut vertices = Vec::with_capacity(n);
            for v in 0..n {
                let pos = p.prim.positions[v];
                let slot = plan.slot_of[slots[v] as usize];
                vertices.push(TinyVertex {
                    position: [
                        half(pos[0], &format!("{at} vertex {v} x"))?,
                        half(pos[1], &format!("{at} vertex {v} y"))?,
                        half(pos[2], &format!("{at} vertex {v} z"))?,
                        half(slot as f32, "a slot")?,
                    ],
                    uv: [half(uvs[v][0], &format!("{at} vertex {v} u"))?, half(uvs[v][1], &format!("{at} vertex {v} v"))?],
                    normal: [
                        half(normals[v][0], &format!("{at} vertex {v} normal x"))?,
                        half(normals[v][1], &format!("{at} vertex {v} normal y"))?,
                        half(normals[v][2], &format!("{at} vertex {v} normal z"))?,
                        tm::NORMAL_W,
                    ],
                });
            }
            let strip: Vec<u32> = if p.prim.strip { p.prim.indices.clone() } else { to_strip(&p.triangles()) };
            if strip.len() < 3 {
                return Err(format!("{at} draws no triangle"));
            }
            if let Some(i) = strip.iter().find(|&&i| i as usize >= n) {
                return Err(format!("{at} indexes vertex {i} of {n}"));
            }
            let (lo, hi) = bounds(&vertices);
            let center: [f32; 3] = std::array::from_fn(|a| (lo[a] + hi[a]) * 0.5);
            let d: [f32; 3] = std::array::from_fn(|a| hi[a] - lo[a]);
            groups.push(PrimGroup {
                header: tm::GROUP_HEADER,
                vertex_shader: pandemic_hash_m2(role.vertex_shader()),
                shadow_vertex_shader: pandemic_hash_m2(role.shadow_vertex_shader(m.alphatest)),
                center,
                radius: (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() * 0.5,
                bbox_min: lo,
                bbox_max: hi,
                stream_flag: tm::STREAM_FLAG,
                prims: tm::group_prims(mat as u32, strip.len(), n),
                strip: strip.iter().map(|&i| i as u16).collect(),
                vertices,
            });
        }
        if !groups.is_empty() {
            subs.push((role, groups));
        }
    }
    if subs.is_empty() {
        return Err("the source has no primitive".into());
    }

    let count = subs.len();
    let mut nodes = Vec::with_capacity(count);
    let mut sub_objects = Vec::with_capacity(count);
    for (k, (role, groups)) in subs.into_iter().enumerate() {
        let (lo, hi) = union(groups.iter().map(|g| (g.bbox_min, g.bbox_max)));
        nodes.push(tm::hier_node(pandemic_hash_m2(role.node()), k, count, lo, hi));
        sub_objects.push(SubObject { node: k as u16, groups });
    }
    let (lo, hi) = union(nodes.iter().map(|n| ([n.bbox_min[0], n.bbox_min[1], n.bbox_min[2]], [n.bbox_max[0], n.bbox_max[1], n.bbox_max[2]])));
    let materials = source
        .materials
        .iter()
        .zip(materials)
        .map(|(m, r)| tm::TinyMaterial {
            name_hash: pandemic_hash_m2(&tm::material_name(row, col, m.alphatest)),
            preamble: tm::MATERIAL_PREAMBLE,
            flags: if m.alphatest { tm::MATERIAL_ALPHATEST } else { tm::MATERIAL_OPAQUE },
            textures: vec![r.texture],
            pixel_shader: pandemic_hash_m2(&r.pixel_shader),
            trailing: pandemic_hash_m2(tm::MATERIAL_TRAILING),
        })
        .collect();
    Ok(TinyModel {
        info: tm::ModelInfo {
            flags: tm::INFO_FLAGS,
            bbox_min: lo,
            bbox_max: hi,
            word_1c: tm::INFO_WORD_1C,
            tail: tm::INFO_TAIL,
        },
        nodes,
        materials,
        slots: plan.list.clone(),
        segments: (0..count).map(|k| SegmRecord { bone: k as u16, segment: k as u8, state_mask: 1 }).collect(),
        sub_objects,
    })
}

/// The layer sub-block with the stand-in's `TinyGeometryObject` placement added: keyed `key`,
/// named [`placement_name`], drawing `model_hash`, at the cell's centre, unrotated.
pub fn layer_with_placement(container: &[u8], key: u32, row: u32, col: u32, model_hash: u32) -> Result<Vec<u8>, String> {
    insert_entity(
        container,
        &NewEntity {
            key,
            model_hash,
            pos: cell_centre(row, col),
            quat: [0.0, 0.0, 0.0, 1.0],
            name: placement_name(row, col),
        },
        FLGS_TINY_GEOMETRY_OBJECT,
    )
}

// ── the world the stand-ins live in ─────────────────────────────────────────────────────────────

/// What the game's layers say about stand-ins and the objects they draw.
#[derive(Debug, Clone, Default)]
pub struct World {
    /// Every placement's positions by key, with the layer (name hash) holding each.
    pub placed: BTreeMap<u32, Vec<(u32, [f32; 3])>>,
    /// Each layer's placements by name (the `Name` record without its ` 0x<key>`), as keys.
    pub names: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    /// Every `TinyGeometryObject` placement: `(layer, key, cell)`.
    pub stand_ins: Vec<(u32, u32, Option<(u32, u32)>)>,
}

impl World {
    /// Read every layer sub-block of the stack. A layer without a `Name`, `ModelName` or
    /// `Transform` COMP or a `flgs` holds no stand-in; its positions still count.
    pub fn read(layers: &BTreeMap<u32, Vec<u8>>) -> Result<World, String> {
        let mut w = World::default();
        for (&layer, c) in layers {
            let places = mercs2_formats::placement::load_placements(c)
                .map_err(|e| format!("layer 0x{layer:08X}: {e}"))?;
            for p in places {
                w.placed.entry(p.key).or_default().push((layer, p.pos));
                if let Some(n) = &p.name {
                    w.names.entry(layer).or_default().entry(n.clone()).or_default().push(p.key);
                }
            }
            let r = match mercs2_formats::placement_build::read_layer_records(c) {
                Ok(r) => r,
                Err(e) if e.starts_with("the layer has no ") => continue,
                Err(e) => return Err(format!("layer 0x{layer:08X}: {e}")),
            };
            for (key, state) in &r.flags {
                let named = r.names.iter().any(|(k, n)| k == key && n.starts_with("tinygeometry_tgr"));
                if *state == FLGS_TINY_GEOMETRY_OBJECT && named {
                    let cell = r.transforms.iter().find(|t| t.0 == *key).and_then(|t| cell_of(t.1));
                    w.stand_ins.push((layer, *key, cell));
                }
            }
        }
        Ok(w)
    }

    /// The GUID an object reference names: a bare `0xGUID`, or the name of exactly one placement of
    /// `layer`.
    pub fn resolve(&self, layer: u32, reference: &str) -> Result<u32, String> {
        if let Some(g) = crate::manifest::bare_hash(reference) {
            return Ok(g);
        }
        match self.names.get(&layer).and_then(|n| n.get(reference)).map(Vec::as_slice) {
            Some([g]) => Ok(*g),
            Some(many) => Err(format!(
                "{reference:?} names {} placements of the layer; name the object by its 0xGUID",
                many.len()
            )),
            None => Err(format!("{reference:?} is neither a 0xGUID nor the name of a placement of the layer")),
        }
    }

    /// Whether object `guid` is placed in cell `(row, col)`: by its placement in `layer` when that
    /// layer has one, else by any layer's.
    pub fn in_cell(&self, layer: u32, guid: u32, row: u32, col: u32) -> Result<(), String> {
        let at = self.placed.get(&guid).ok_or_else(|| format!("0x{guid:08X} is placed in no layer of the game"))?;
        let own: Vec<&(u32, [f32; 3])> = at.iter().filter(|(l, _)| *l == layer).collect();
        let candidates: Vec<&(u32, [f32; 3])> = if own.is_empty() { at.iter().collect() } else { own };
        if candidates.iter().any(|(_, p)| cell_of(*p) == Some((row, col))) {
            return Ok(());
        }
        let p = candidates[0].1;
        Err(format!(
            "0x{guid:08X} is placed at ({:.1}, {:.1}, {:.1}), cell {:?}, not in cell (row {row}, col {col})",
            p[0],
            p[1],
            p[2],
            cell_of(p)
        ))
    }
}

// ── a Shipment's stand-ins ──────────────────────────────────────────────────────────────────────

/// One `add_tiny_geometry` of a manifest.
#[derive(Debug, Clone)]
pub struct Contribution<'a> {
    pub index: usize,
    pub layer: &'a str,
    pub row: u32,
    pub col: u32,
    pub key: u32,
    pub objects: &'a [String],
    pub model: &'a Path,
}

/// Every `add_tiny_geometry` of a manifest, in order.
pub fn contributions(manifest: &crate::manifest::Manifest) -> Vec<Contribution<'_>> {
    manifest
        .contributions
        .iter()
        .enumerate()
        .filter_map(|(index, c)| match c {
            crate::manifest::Contribution::AddTinyGeometry { layer, cell, key, objects, model } => Some(Contribution {
                index,
                layer,
                row: cell.row,
                col: cell.col,
                key: *key,
                objects,
                model,
            }),
            _ => None,
        })
        .collect()
}

/// The problems a manifest's stand-ins show without the game: the cell (M0249), the object count
/// (M0240), an object named twice (M0241), and, with the Shipment's root, each source's roles,
/// slots and shape (M0244, M0242, M0243, M0251).
pub fn hermetic_problems(manifest: &crate::manifest::Manifest, root: Option<&Path>, skip: &[usize]) -> Vec<(usize, Problem)> {
    let mut out = Vec::new();
    for c in contributions(manifest) {
        if c.row >= GRID || c.col >= GRID {
            out.push((c.index, Problem {
                code: "M0249",
                message: format!("cell (row {}, col {}) is outside the {GRID} × {GRID} grid (0 to {})", c.row, c.col, GRID - 1),
            }));
        }
        if c.objects.len() > MAX_OBJECTS {
            out.push((c.index, Problem {
                code: "M0240",
                message: format!(
                    "{} objects; a stand-in draws at most {MAX_OBJECTS}: its slot list holds {MAX_SLOTS} GUIDs \
                     and every slot ≡ 3 (mod 4) is skipped",
                    c.objects.len()
                ),
            }));
        }
        if c.objects.is_empty() {
            out.push((c.index, Problem { code: "M0240", message: "no objects; a stand-in draws at least one".into() }));
        }
        let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
        for (i, o) in c.objects.iter().enumerate() {
            let id = crate::manifest::bare_hash(o).unwrap_or_else(|| pandemic_hash_m2(o));
            if let Some(first) = seen.insert(id, i) {
                out.push((c.index, Problem {
                    code: "M0241",
                    message: format!("objects[{i}] {o:?} names the object objects[{first}] names"),
                }));
            }
        }
        if let Some(root) = root {
            if skip.contains(&c.index) {
                continue;
            }
            match read_source(&root.join(c.model)) {
                Ok(src) => {
                    out.extend(source_problems(&src, c.objects.len()).into_iter().map(|p| (c.index, p)));
                    out.extend(shape_problems(&src).into_iter().map(|p| (c.index, p)));
                }
                Err(e) => out.push((c.index, Problem { code: "M0251", message: e })),
            }
        }
    }
    out
}

/// The problems a manifest's stand-ins show against the game: an object that does not resolve or
/// is not placed in the cell (M0245, also for a layer the game lacks), two references to one object
/// (M0241), more stand-ins than the registry holds (M0246), a cell the layer already has a stand-in
/// for (M0247), and a key the game already uses (M0250).
pub fn game_problems(manifest: &crate::manifest::Manifest, game: &mut crate::game::GameStack) -> Vec<(usize, Problem)> {
    let cs = contributions(manifest);
    if cs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let world = match game.layer_containers().and_then(|l| World::read(&l)) {
        Ok(w) => w,
        Err(e) => {
            for c in &cs {
                out.push((c.index, Problem { code: "M0245", message: format!("the game's layers could not be read: {e}") }));
            }
            return out;
        }
    };
    let total = world.stand_ins.len() + cs.len();
    if total > MAX_CONTAINERS {
        out.push((cs[0].index, Problem {
            code: "M0246",
            message: format!(
                "the game places {} stand-ins and this Shipment adds {}: {total} slot lists, and the registry \
                 holds {MAX_CONTAINERS} (`0x0050F26C` drops the rest)",
                world.stand_ins.len(),
                cs.len()
            ),
        }));
    }
    let mut keys: BTreeMap<u32, usize> = BTreeMap::new();
    for c in &cs {
        let layer = crate::manifest::asset_hash(c.layer);
        if !world.names.contains_key(&layer) && !world.placed.values().flatten().any(|(l, _)| *l == layer) {
            out.push((c.index, Problem { code: "M0245", message: format!("layer {:?} is not in the game", c.layer) }));
            continue;
        }
        if let Some(prev) = keys.insert(c.key, c.index) {
            out.push((c.index, Problem {
                code: "M0250",
                message: format!("key 0x{:08X} is contributions[{prev}]'s key too", c.key),
            }));
        }
        if world.placed.contains_key(&c.key) {
            out.push((c.index, Problem {
                code: "M0250",
                message: format!("key 0x{:08X} is a placement the game already has", c.key),
            }));
        }
        if world.stand_ins.iter().any(|(l, _, cell)| *l == layer && *cell == Some((c.row, c.col))) {
            out.push((c.index, Problem {
                code: "M0247",
                message: format!(
                    "layer {:?} already has a stand-in for cell (row {}, col {})",
                    c.layer, c.row, c.col
                ),
            }));
        }
        let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
        for (i, o) in c.objects.iter().enumerate() {
            let guid = match world.resolve(layer, o) {
                Ok(g) => g,
                Err(e) => {
                    out.push((c.index, Problem { code: "M0245", message: format!("objects[{i}]: {e}") }));
                    continue;
                }
            };
            if let Some(first) = seen.insert(guid, i) {
                if crate::manifest::bare_hash(o).is_none() {
                    out.push((c.index, Problem {
                        code: "M0241",
                        message: format!("objects[{i}] {o:?} is 0x{guid:08X}, which objects[{first}] names"),
                    }));
                }
                continue;
            }
            if let Err(e) = world.in_cell(layer, guid, c.row, c.col) {
                out.push((c.index, Problem { code: "M0245", message: format!("objects[{i}]: {e}") }));
            }
        }
    }
    out
}

/// Lower every `add_tiny_geometry` of a Shipment: each stand-in's model block, and one overlay of
/// each layer block the stand-ins' placements go into, carrying all of them.
pub fn lower_shipment(
    manifest: &crate::manifest::Manifest,
    root: &Path,
    game: &mut crate::game::GameStack,
    log: &mut Vec<String>,
) -> Result<Vec<mercs2_formats::patch_wad::PatchBlock>, (usize, String)> {
    use mercs2_formats::types::{TYPE_HASH_MODEL, TYPE_ID_MODEL, TYPE_ID_TEXTURE};
    let cs = contributions(manifest);
    if cs.is_empty() {
        return Ok(Vec::new());
    }
    let first = cs[0].index;
    let layers = game.layer_containers().map_err(|e| (first, e))?;
    let world = World::read(&layers).map_err(|e| (first, e))?;
    let added_textures: BTreeSet<u32> = manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            crate::manifest::Contribution::AddTexture { name, .. } => Some(crate::manifest::asset_hash(name)),
            _ => None,
        })
        .collect();
    let mut blocks = Vec::new();
    // block path -> (inputs, [(entry, key, row, col, model hash)])
    let mut edits: BTreeMap<String, (crate::game::LayerEditInputs, Vec<(usize, u32, u32, u32, u32)>)> = BTreeMap::new();
    for c in &cs {
        let fail = |m: String| (c.index, m);
        let layer = crate::manifest::asset_hash(c.layer);
        let source = read_source(&root.join(c.model)).map_err(fail)?;
        let guids = c.objects.iter().map(|o| world.resolve(layer, o)).collect::<Result<Vec<u32>, String>>().map_err(fail)?;
        for &g in &guids {
            world.in_cell(layer, g, c.row, c.col).map_err(fail)?;
        }
        let plan = plan_slots(&guids, &|g| world.placed.contains_key(&g) || g == c.key).map_err(fail)?;
        let materials = resolve_materials(&source).map_err(fail)?;
        for (k, m) in materials.iter().enumerate() {
            if !game.has_asset(m.texture, TYPE_ID_TEXTURE) && !added_textures.contains(&m.texture) {
                return Err(fail(format!(
                    "material {k}'s texture 0x{:08X} is neither in the game nor an add_texture of this Shipment",
                    m.texture
                )));
            }
        }
        let model = build_model(&source, c.row, c.col, &plan, &materials).map_err(fail)?;
        let name = model_name(c.layer, c.row, c.col, c.key);
        let hash = pandemic_hash_m2(&name);
        let container = model.encode();
        blocks.push(
            crate::build::opaque_container_block(hash, TYPE_HASH_MODEL, TYPE_ID_MODEL, &container, c.index, "add_tiny_geometry")
                .map_err(|e| fail(e.to_string()))?,
        );
        let (inputs, entry) = game
            .layer_by_name(c.layer)
            .map_err(fail)?
            .ok_or_else(|| fail(format!("layer {:?} is not in the game", c.layer)))?;
        log.push(format!(
            "contributions[{}] add_tiny_geometry {name} 0x{hash:08X}: {} object(s) in {} slot(s), {} \
             sub-object(s), {} group(s); placement 0x{:08X} into {} ({})",
            c.index,
            guids.len(),
            plan.list.len(),
            model.sub_objects.len(),
            model.sub_objects.iter().map(|s| s.groups.len()).sum::<usize>(),
            c.key,
            c.layer,
            inputs.path
        ));
        edits
            .entry(inputs.path.clone())
            .or_insert_with(|| (inputs, Vec::new()))
            .1
            .push((entry, c.key, c.row, c.col, hash));
    }
    for (path, (inputs, places)) in edits {
        let mut block = inputs.block.clone();
        for (entry, key, row, col, hash) in places {
            let (_, entries) = mercs2_formats::ucfx::parse_block_entry_table(&block);
            let container = mercs2_formats::placement_build::find_entry(&block, entries[entry].name_hash, entries[entry].type_hash)
                .map(|(_, c)| c.to_vec())
                .ok_or_else(|| (first, format!("{path}: entry {entry} vanished")))?;
            let edited = layer_with_placement(&container, key, row, col, hash).map_err(|e| (first, format!("{path}: {e}")))?;
            block = mercs2_formats::placement_build::replace_entry(&block, entry, &edited).map_err(|e| (first, e))?;
        }
        blocks.push(crate::build::emit_edited_layer(&inputs, &block).map_err(|e| (first, e))?);
    }
    Ok(blocks)
}

// ── decomposing a stand-in ──────────────────────────────────────────────────────────────────────

/// A stand-in taken apart into what `add_tiny_geometry` declares: its objects (the slot list's
/// GUIDs) and its model as a binary glTF.
#[derive(Debug, Clone, PartialEq)]
pub struct Decomposed {
    pub objects: Vec<u32>,
    pub glb: Vec<u8>,
}

/// Take a TINY model apart for cell `(row, col)`. Each group becomes a `TRIANGLE_STRIP` primitive
/// with its own strip, its vertices' half floats widened to floats, each vertex's slot as
/// `_TINY_SLOT` and its role as `extras.tiny_role`; each material a material with its alpha mode,
/// `extras.texture` (a name `texture_name` gives, else `0xHASH`) and `extras.pixel_shader`. A
/// container that holds something `add_tiny_geometry` does not express is refused, naming it.
pub fn decompose(model: &TinyModel, row: u32, col: u32, texture_name: &dyn Fn(u32) -> Option<String>) -> Result<Decomposed, String> {
    let objects = model.slots.clone();
    if objects.windows(2).any(|w| w[0] >= w[1]) {
        return Err("the slot list is not strictly ascending".into());
    }
    let pristine = pandemic_hash_m2(tm::NODE_PRISTINE);
    let ruin = pandemic_hash_m2(tm::NODE_RUIN);
    let mut materials = Vec::new();
    for (k, m) in model.materials.iter().enumerate() {
        let alphatest = match m.flags {
            tm::MATERIAL_OPAQUE => false,
            tm::MATERIAL_ALPHATEST => true,
            f => return Err(format!("material {k} has flags 0x{f:04X}, neither opaque nor alpha-tested")),
        };
        if m.name_hash != pandemic_hash_m2(&tm::material_name(row, col, alphatest))
            || m.preamble != tm::MATERIAL_PREAMBLE
            || m.trailing != pandemic_hash_m2(tm::MATERIAL_TRAILING)
            || m.textures.len() != 1
        {
            return Err(format!("material {k} is not a stand-in material of cell (row {row}, col {col})"));
        }
        let pixel = crate::shader::retail_name(m.pixel_shader)
            .ok_or_else(|| format!("material {k}'s pixel shader 0x{:08X} is no retail registration", m.pixel_shader))?;
        let texture = texture_name(m.textures[0]).unwrap_or_else(|| format!("0x{:08X}", m.textures[0]));
        let mut extras = serde_json::Map::new();
        extras.insert("texture".into(), texture.into());
        extras.insert("pixel_shader".into(), pixel.into());
        materials.push(serde_json::json!({
            "alphaMode": if alphatest { "MASK" } else { "OPAQUE" },
            "extras": extras,
        }));
    }

    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut prims) = (Vec::new(), Vec::new(), Vec::new());
    let push_view = |bin: &mut Vec<u8>, bytes: &[u8], views: &mut Vec<serde_json::Value>| {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push(serde_json::json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()}));
        bin.extend_from_slice(bytes);
        views.len() - 1
    };
    for (k, sub) in model.sub_objects.iter().enumerate() {
        let node = model.nodes.get(sub.node as usize).ok_or_else(|| format!("sub-object {k} hangs from no node"))?;
        let role = match node.name_hash {
            h if h == pristine => Role::Intact,
            h if h == ruin => Role::Ruined,
            h => return Err(format!("sub-object {k}'s node 0x{h:08X} is neither pristine nor ruin")),
        };
        for (g, grp) in sub.groups.iter().enumerate() {
            let at = format!("sub-object {k} group {g}");
            let mat = grp.prims.first().map(|p| p.material).ok_or_else(|| format!("{at} has no PRMT record"))?;
            let alphatest = model.materials.get(mat as usize).map(|m| m.flags == tm::MATERIAL_ALPHATEST);
            if grp.header != tm::GROUP_HEADER
                || grp.stream_flag != tm::STREAM_FLAG
                || grp.vertex_shader != pandemic_hash_m2(role.vertex_shader())
                || Some(grp.shadow_vertex_shader) != alphatest.map(|a| pandemic_hash_m2(role.shadow_vertex_shader(a)))
                || grp.prims != tm::group_prims(mat, grp.strip.len(), grp.vertices.len())
                || grp.vertices.iter().any(|v| v.normal[3] != tm::NORMAL_W)
            {
                return Err(format!("{at} is not a stand-in group (header, shaders, PRMT or NORMAL.w)"));
            }
            let mut pos = Vec::with_capacity(grp.vertices.len() * 12);
            let mut nrm = Vec::with_capacity(grp.vertices.len() * 12);
            let mut uv = Vec::with_capacity(grp.vertices.len() * 8);
            let mut slot = Vec::with_capacity(grp.vertices.len() * 2);
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for v in &grp.vertices {
                for a in 0..3 {
                    let x = unhalf(v.position[a]);
                    lo[a] = lo[a].min(x);
                    hi[a] = hi[a].max(x);
                    pos.extend_from_slice(&x.to_le_bytes());
                    nrm.extend_from_slice(&unhalf(v.normal[a]).to_le_bytes());
                }
                for a in 0..2 {
                    uv.extend_from_slice(&unhalf(v.uv[a]).to_le_bytes());
                }
                let w = unhalf(v.position[3]);
                if w.fract() != 0.0 || w < 0.0 || w as usize >= objects.len() {
                    return Err(format!("{at} has a vertex at slot {w}, which the list does not hold"));
                }
                slot.extend_from_slice(&(w as u16).to_le_bytes());
            }
            let idx: Vec<u8> = grp.strip.iter().flat_map(|i| i.to_le_bytes()).collect();
            let n = grp.vertices.len();
            let vp = push_view(&mut bin, &pos, &mut views);
            let vn = push_view(&mut bin, &nrm, &mut views);
            let vt = push_view(&mut bin, &uv, &mut views);
            let vs = push_view(&mut bin, &slot, &mut views);
            let vi = push_view(&mut bin, &idx, &mut views);
            let a0 = accessors.len();
            accessors.push(serde_json::json!({"bufferView": vp, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi}));
            accessors.push(serde_json::json!({"bufferView": vn, "componentType": 5126, "count": n, "type": "VEC3"}));
            accessors.push(serde_json::json!({"bufferView": vt, "componentType": 5126, "count": n, "type": "VEC2"}));
            accessors.push(serde_json::json!({"bufferView": vs, "componentType": 5123, "count": n, "type": "SCALAR"}));
            accessors.push(serde_json::json!({"bufferView": vi, "componentType": 5123, "count": grp.strip.len(), "type": "SCALAR"}));
            prims.push(serde_json::json!({
                "attributes": {"POSITION": a0, "NORMAL": a0 + 1, "TEXCOORD_0": a0 + 2, mercs2_formats::mesh_import::TINY_SLOT_ATTRIBUTE: a0 + 3},
                "indices": a0 + 4,
                "mode": 5,
                "material": mat,
                "extras": {ROLE_KEY: role.token()},
            }));
        }
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let json = serde_json::json!({
        "asset": {"version": "2.0", "generator": "qm extract-tiny"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": prims}],
        "materials": materials,
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": views,
        "accessors": accessors,
    });
    let mut text = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(text.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&text);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&bin);
    Ok(Decomposed { objects, glb })
}

/// The `add_tiny_geometry` contribution, as manifest YAML, that rebuilds a decomposed stand-in.
pub fn contribution_yaml(layer: &str, row: u32, col: u32, key: u32, objects: &[u32], model: &str) -> String {
    let objects = objects.iter().map(|g| format!("\"0x{g:08X}\"")).collect::<Vec<_>>().join(", ");
    format!(
        "  - kind: add_tiny_geometry\n    layer: {layer}\n    cell: {{ row: {row}, col: {col} }}\n    key: {key}\n    \
         objects: [{objects}]\n    model: {model}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_matches_retail_placements() {
        // vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece sits at (1900, 0, -1500)
        assert_eq!(cell_centre(12, 29), [1900.0, 0.0, -1500.0]);
        assert_eq!(cell_of([1900.0, 0.0, -1500.0]), Some((12, 29)));
        assert_eq!(cell_of([1999.9, 5.0, -1600.0]), Some((12, 29)));
        assert_eq!(cell_of([-4000.0, 0.0, 3999.0]), Some((39, 0)));
        assert_eq!(cell_of([4000.0, 0.0, 0.0]), None);
        assert_eq!(model_name("vz_merida_tiny", 12, 29, 0x144ece), "vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece");
        assert_eq!(placement_name(3, 7), "tinygeometry_tgr03_tgc07");
        assert_eq!(
            parse_model_name("vz_state_a_b_tinygeometry_tgr03_tgc07_0x0000abcd"),
            Some(("vz_state_a_b".into(), 3, 7, 0xabcd))
        );
        assert_eq!(parse_model_name("vz_state_a_b_tinygeometry_tgr03_tgc07"), None);
    }

    #[test]
    fn the_engine_search_finds_each_key_of_an_ascending_list() {
        let list = [10, 20, 30, 40, 50];
        for (i, k) in list.iter().enumerate() {
            assert_eq!(engine_slot(&list, *k), Some(i));
        }
        assert_eq!(engine_slot(&list, 25), None);
        assert_eq!(engine_slot(&[], 1), None);
        // with a repeat, the search lands on whichever copy its midpoints reach first
        assert_eq!(engine_slot(&[1, 2, 3, 3, 4], 3), Some(2));
    }

    #[test]
    fn slots_skip_three_mod_four_and_fill_with_a_free_guid() {
        let objects = [500, 100, 300, 200, 400, 600, 700];
        let plan = plan_slots(&objects, &|_| false).unwrap();
        assert_eq!(plan.list, vec![100, 200, 300, 301, 400, 500, 600, 601, 700]);
        assert_eq!(plan.slot_of, vec![5, 0, 2, 1, 4, 6, 8]);
        assert!(plan.slot_of.iter().all(|s| s % 4 != 3));
        // a free GUID skips the taken ones
        let plan = plan_slots(&objects, &|g| g == 301 || g == 302).unwrap();
        assert_eq!(plan.list[3], 303);
    }

    #[test]
    fn consecutive_guids_take_a_repeat_the_search_still_resolves() {
        let objects = [1, 2, 3, 4, 5, 6, 7];
        let plan = plan_slots(&objects, &|_| false).unwrap();
        assert_eq!(plan.list.len(), 9);
        for (i, g) in objects.iter().enumerate() {
            assert_eq!(engine_slot(&plan.list, *g), Some(plan.slot_of[i] as usize));
        }
        assert!(plan.slot_of.iter().all(|s| s % 4 != 3));
    }

    #[test]
    fn a_full_list_of_192_objects_fits_and_193_do_not() {
        let objects: Vec<u32> = (0..192).map(|k| 10 * k + 10).collect();
        let plan = plan_slots(&objects, &|_| false).unwrap();
        assert_eq!(plan.list.len(), MAX_SLOTS);
        let objects: Vec<u32> = (0..193).map(|k| 10 * k + 10).collect();
        assert!(plan_slots(&objects, &|_| false).unwrap_err().contains("at most 192"));
        assert!(plan_slots(&[5, 5], &|_| false).unwrap_err().contains("same GUID"));
    }

    fn prim(role: Option<&str>, strip: bool, indices: Vec<u32>, slots: Option<Vec<u32>>, n: usize) -> SourcePrim {
        SourcePrim {
            prim: SourcePrimitive {
                at: (0, 0),
                extras: None,
                material: Some(0),
                strip,
                indices,
                positions: (0..n).map(|k| [k as f32, 1.0, 2.0]).collect(),
                normals: Some(vec![[0.0, 1.0, 0.0]; n]),
                uvs: Some(vec![[0.5, 0.25]; n]),
                slots,
            },
            role: role.map(str::to_string),
        }
    }

    fn mat(alphatest: bool) -> SourceMat {
        SourceMat { alphatest, texture: Some("0x11112222".into()), pixel_shader: Some("PgDiffFP".into()), blend: false }
    }

    #[test]
    fn source_problems_name_the_rule() {
        let s = Source {
            prims: vec![
                prim(None, false, vec![0, 1, 2], Some(vec![0, 0, 0]), 3),
                prim(Some("broken"), false, vec![0, 1, 2], Some(vec![0, 0, 0]), 3),
                prim(Some("intact"), false, vec![0, 1, 2], None, 3),
                prim(Some("intact"), false, vec![0, 1, 2], Some(vec![0, 0, 2]), 3),
                prim(Some("ruined"), false, vec![0, 1, 2], Some(vec![0, 1, 1]), 3),
                prim(Some("ruined"), true, vec![0, 1, 2, 2, 3, 4], Some(vec![0, 0, 0, 1, 1, 1]), 5),
            ],
            materials: vec![mat(false)],
        };
        let codes: Vec<&str> = source_problems(&s, 2).iter().map(|p| p.code).collect();
        assert_eq!(codes, ["M0244", "M0244", "M0242", "M0242", "M0243", "M0243", "M0243"]);
        // a strip's degenerate joins are no triangle: this strip draws (0,1,2) and (3,4,5)
        let ok = Source { prims: vec![prim(Some("ruined"), true, vec![0, 1, 2, 2, 3, 3, 4, 5], Some(vec![0, 0, 0, 1, 1, 1]), 6)], materials: vec![mat(false)] };
        assert!(source_problems(&ok, 2).is_empty());
    }

    #[test]
    fn a_built_stand_in_has_the_retail_shape() {
        let s = Source {
            prims: vec![
                prim(Some("ruined"), false, vec![0, 1, 2], Some(vec![1, 1, 1]), 3),
                prim(Some("intact"), true, vec![0, 1, 2, 3], Some(vec![0, 0, 0, 0]), 4),
            ],
            materials: vec![mat(true)],
        };
        let plan = plan_slots(&[0x200, 0x100], &|_| false).unwrap();
        let mats = resolve_materials(&s).unwrap();
        let m = build_model(&s, 11, 30, &plan, &mats).unwrap();
        // the intact sub-object first, whatever the file's order
        assert_eq!(m.nodes[0].name_hash, pandemic_hash_m2("pristine"));
        assert_eq!(m.nodes[1].name_hash, pandemic_hash_m2("ruin"));
        assert_eq!(m.slots, vec![0x100, 0x200]);
        let g0 = &m.sub_objects[0].groups[0];
        assert_eq!(g0.vertex_shader, pandemic_hash_m2("PgMeshTinyVP"));
        assert_eq!(g0.shadow_vertex_shader, pandemic_hash_m2("PgMeshTinyShadowTexVP"));
        assert_eq!(g0.strip, vec![0, 1, 2, 3]);
        // objects[0] = 0x200 sits at slot 1
        assert_eq!(unhalf(g0.vertices[0].position[3]), 1.0);
        assert_eq!(g0.vertices[0].normal[3], tm::NORMAL_W);
        let g1 = &m.sub_objects[1].groups[0];
        assert_eq!(g1.vertex_shader, pandemic_hash_m2("PgMeshTinyVP_Ruin"));
        assert_eq!(unhalf(g1.vertices[0].position[3]), 0.0);
        assert_eq!(m.materials[0].name_hash, pandemic_hash_m2("tinygeometry_tgr11_tgc30_alphatest"));
        assert_eq!(m.materials[0].flags, tm::MATERIAL_ALPHATEST);
        assert_eq!(m.materials[0].textures, vec![0x1111_2222]);
        assert_eq!(g0.bbox_min, [0.0, 1.0, 2.0]);
        assert_eq!(g0.bbox_max, [3.0, 1.0, 2.0]);
        assert_eq!(g0.center, [1.5, 1.0, 2.0]);
        // and the container it encodes to decodes back to it
        assert_eq!(TinyModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn a_material_without_a_texture_or_with_blend_is_refused() {
        let mut s = Source { prims: vec![prim(Some("intact"), false, vec![0, 1, 2], Some(vec![0, 0, 0]), 3)], materials: vec![mat(false)] };
        s.materials[0].texture = None;
        assert!(resolve_materials(&s).unwrap_err().contains("extras.texture"));
        s.materials[0] = SourceMat { blend: true, ..mat(false) };
        assert!(resolve_materials(&s).unwrap_err().contains("BLEND"));
        s.materials[0] = SourceMat { pixel_shader: None, ..mat(false) };
        assert!(resolve_materials(&s).unwrap_err().contains("extras.pixel_shader"));
    }

    #[test]
    fn a_decomposed_stand_in_rebuilds_to_itself() {
        let s = Source {
            prims: vec![
                prim(Some("intact"), false, vec![0, 1, 2], Some(vec![0, 0, 0]), 3),
                prim(Some("ruined"), false, vec![0, 1, 2], Some(vec![1, 1, 1]), 3),
            ],
            materials: vec![mat(false)],
        };
        let plan = plan_slots(&[0x100, 0x200], &|_| false).unwrap();
        let built = build_model(&s, 4, 5, &plan, &resolve_materials(&s).unwrap()).unwrap();
        let model = TinyModel::decode(&built.encode()).unwrap();
        let d = decompose(&model, 4, 5, &|_| None).unwrap();
        assert_eq!(d.objects, vec![0x100, 0x200]);
        let dir = std::env::temp_dir().join(format!("qm_tiny_decompose_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.glb"), &d.glb).unwrap();
        let src = read_source(&dir.join("m.glb")).unwrap();
        assert!(src.prims.iter().all(|p| p.prim.strip));
        let again = plan_slots(&d.objects, &|_| false).unwrap();
        let rebuilt = build_model(&src, 4, 5, &again, &resolve_materials(&src).unwrap()).unwrap();
        assert_eq!(rebuilt.encode(), built.encode());
        assert_eq!(
            contribution_yaml("vz_l", 4, 5, 9, &d.objects, "src/m.glb"),
            "  - kind: add_tiny_geometry\n    layer: vz_l\n    cell: { row: 4, col: 5 }\n    key: 9\n    \
             objects: [\"0x00000100\", \"0x00000200\"]\n    model: src/m.glb\n"
        );
    }
}
