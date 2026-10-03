//! glTF/GLB → [`ExternalMesh`] for the **rigid** path.
//!
//! Scope is deliberately narrow. `add_model` hosts its geometry in a donor container, so the donor
//! supplies the rig, the materials and the state machine — this reader needs positions, normals,
//! UVs and triangles, and nothing else. That is why the `gltf` dependency can stay at
//! `default-features = false`: no embedded-texture decoding, no `image` crate.
//!
//! **This is NOT the character path.** A skinned import needs palette-relative BLENDINDICES and the
//! matching `INFO(56)` range table, which [`crate::char_skin`] produces and
//! `inject_character_into_donor_block` consumes. Hand-authored global joint indices on a character
//! group are wrong (see [`ExternalMesh::joints`]); this reader leaves `joints`/`weights` empty so a
//! caller gets the documented rigid bone-0 fallback rather than plausible-looking nonsense.
//!
//! The Workshop keeps its own richer importer (materials, images, skin, source-rig joint names) —
//! that feeds a preview and a retarget workbench, which is a different job from lowering one prop.

use crate::model_inject::ExternalMesh;
use std::path::Path;

/// Read every mesh primitive in the file, flattened into one [`ExternalMesh`] in file order.
///
/// Node transforms ARE applied: a glTF authored with its parts positioned by node transform would
/// otherwise collapse to the origin. Primitives are concatenated with their indices rebased, so a
/// multi-part prop arrives as one welded triangle set — correct for a rigid host group, and the reason the
/// per-material split the Workshop does is not reproduced here.
/// Open a `.glb`/`.gltf` and resolve its buffers **without** gltf's `import` feature.
///
/// `import` would pull `image` + `base64` in just to decode embedded textures neither reader wants:
///   - GLB → the BIN chunk, handed back as buffer 0.
///   - `.gltf` with an external `.bin` URI → read relative to the file.
///   - a base64 `data:` URI → refused by name, since decoding it is the dependency we declined.
///
/// Shared with [`crate::char_import`] so the rigid and skinned readers cannot disagree about what
/// counts as a loadable file.
pub(crate) fn open_gltf(path: &Path) -> Result<(gltf::Document, Vec<Vec<u8>>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let gltf::Gltf {
        document: doc,
        blob,
    } = gltf::Gltf::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for buffer in doc.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                buffers.push(blob.clone().unwrap_or_default());
            }
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with("data:") {
                    return Err(format!(
                        "{}: has a base64 `data:` buffer. Export as BINARY .glb (self-contained) \
                         or keep the .bin beside the .gltf",
                        path.display()
                    ));
                }
                let sibling = path.parent().unwrap_or(Path::new(".")).join(uri);
                buffers.push(
                    std::fs::read(&sibling).map_err(|e| format!("{}: {e}", sibling.display()))?,
                );
            }
        }
    }
    Ok((doc, buffers))
}

pub fn external_mesh_from_gltf(path: &Path) -> Result<ExternalMesh, String> {
    read_gltf(path).map(|(mesh, _)| mesh)
}

/// The glTF custom vertex attribute carrying a vertex's AmbientWind sway weight: a float scalar,
/// 0 (still) to 1 (full sway). The leading underscore is glTF's spelling for an application-specific
/// attribute.
pub const SWAY_WEIGHT_ATTRIBUTE: &str = "_SWAY_WEIGHT";

/// The glTF custom vertex attribute naming the `TINY` far-LOD slot a vertex belongs to: an unsigned
/// integer scalar (`UNSIGNED_BYTE`, `UNSIGNED_SHORT` or `UNSIGNED_INT`, not normalized), the index
/// of the world object in the host container's `TINY` id list.
pub const TINY_SLOT_ATTRIBUTE: &str = "_TINY_SLOT";

/// The custom vertex attributes of a glTF, each per vertex in the vertex order of
/// [`external_mesh_from_gltf`], or `None` when no primitive carries it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomAttributes {
    /// [`SWAY_WEIGHT_ATTRIBUTE`], each 0..1.
    pub sway: Option<Vec<f32>>,
    /// [`TINY_SLOT_ATTRIBUTE`].
    pub tiny_slot: Option<Vec<u32>>,
}

/// Every vertex's custom attributes, in the vertex order of [`external_mesh_from_gltf`].
///
/// A file where some primitives carry an attribute and others do not is an error, as is an accessor
/// of the wrong type or a sway weight outside 0..1: each names the attribute.
pub fn custom_attributes_from_gltf(path: &Path) -> Result<CustomAttributes, String> {
    let (_, per_primitive) = read_gltf(path)?;
    join_custom(path, per_primitive)
}

/// One primitive's custom attributes.
#[derive(Debug, Clone, Default)]
pub(crate) struct PrimitiveCustom {
    sway: Option<Vec<f32>>,
    tiny_slot: Option<Vec<u32>>,
}

/// Per-primitive custom attributes joined into per-vertex ones, by the rules of
/// [`custom_attributes_from_gltf`]. Shared with [`crate::char_import`], which reads its primitives
/// in its own order.
pub(crate) fn join_custom(path: &Path, per_primitive: Vec<PrimitiveCustom>) -> Result<CustomAttributes, String> {
    let (sway, tiny_slot): (Vec<_>, Vec<_>) = per_primitive.into_iter().map(|p| (p.sway, p.tiny_slot)).unzip();
    let sway = join(path, SWAY_WEIGHT_ATTRIBUTE, sway)?;
    if let Some((v, x)) = sway.iter().flatten().enumerate().find(|(_, x)| !(0.0..=1.0).contains(*x)) {
        return Err(format!(
            "{}: vertex {v} has {SWAY_WEIGHT_ATTRIBUTE} {x}; a sway weight is 0 to 1",
            path.display()
        ));
    }
    Ok(CustomAttributes { sway, tiny_slot: join(path, TINY_SLOT_ATTRIBUTE, tiny_slot)? })
}

/// Every primitive's values of one attribute, concatenated; `None` when no primitive has it, an
/// error when only some do.
fn join<T>(path: &Path, attribute: &str, per_primitive: Vec<Option<Vec<T>>>) -> Result<Option<Vec<T>>, String> {
    if per_primitive.iter().all(|p| p.is_none()) {
        return Ok(None);
    }
    let mut out = Vec::new();
    for (i, p) in per_primitive.into_iter().enumerate() {
        let values = p.ok_or_else(|| {
            format!(
                "{}: triangle primitive {i} has no {attribute} attribute and another primitive does; \
                 every vertex takes one",
                path.display()
            )
        })?;
        out.extend(values);
    }
    Ok(Some(out))
}

/// The mesh, and each triangle primitive's custom attributes (in the order the primitives are read).
type Read = (ExternalMesh, Vec<PrimitiveCustom>);

fn read_gltf(path: &Path) -> Result<Read, String> {
    let (doc, buffers) = open_gltf(path)?;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut custom: Vec<PrimitiveCustom> = Vec::new();

    const IDENTITY: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for scene in doc.scenes() {
        for node in scene.nodes() {
            visit(
                &node,
                IDENTITY,
                &buffers,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut tris,
                &mut custom,
            )?;
        }
    }

    if positions.is_empty() {
        return Err(format!("{}: no mesh primitives found", path.display()));
    }
    if tris.is_empty() {
        return Err(format!(
            "{}: geometry has vertices but no triangles — point/line primitives cannot be injected",
            path.display()
        ));
    }
    // Pad the optional streams so every vertex has one, which is what the injector expects.
    normals.resize(positions.len(), [0.0, 1.0, 0.0]);
    uvs.resize(positions.len(), [0.0, 0.0]);

    Ok((
        ExternalMesh {
            positions,
            normals,
            uvs,
            tris,
            joints: Vec::new(),
            weights: Vec::new(),
        },
        custom,
    ))
}

/// One primitive's custom attributes. `vertices` is the primitive's vertex count; each attribute
/// it carries must have one value per vertex.
pub(crate) fn primitive_custom(
    prim: &gltf::Primitive,
    buffers: &[Vec<u8>],
    vertices: usize,
) -> Result<PrimitiveCustom, String> {
    use gltf::accessor::{DataType, Dimensions, Iter};
    let get = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d[..]);
    let accessor = |name: &str| prim.get(&gltf::Semantic::Extras(name[1..].to_string()));
    let counted = |name: &str, n: usize| -> Result<(), String> {
        if n == vertices {
            Ok(())
        } else {
            Err(format!("{name} has {n} values for {vertices} vertices"))
        }
    };
    let mut out = PrimitiveCustom::default();
    if let Some(acc) = accessor(SWAY_WEIGHT_ATTRIBUTE) {
        if acc.dimensions() != Dimensions::Scalar || acc.data_type() != DataType::F32 {
            return Err(format!(
                "{SWAY_WEIGHT_ATTRIBUTE} is {:?} {:?}; it is a float scalar",
                acc.dimensions(),
                acc.data_type()
            ));
        }
        let values: Vec<f32> = Iter::<f32>::new(acc, get)
            .ok_or_else(|| format!("{SWAY_WEIGHT_ATTRIBUTE}: its accessor has no buffer data"))?
            .collect();
        counted(SWAY_WEIGHT_ATTRIBUTE, values.len())?;
        out.sway = Some(values);
    }
    if let Some(acc) = accessor(TINY_SLOT_ATTRIBUTE) {
        let no_data = || format!("{TINY_SLOT_ATTRIBUTE}: its accessor has no buffer data");
        if acc.dimensions() != Dimensions::Scalar || acc.normalized() {
            return Err(format!(
                "{TINY_SLOT_ATTRIBUTE} is {:?}{}; it is an unsigned integer scalar",
                acc.dimensions(),
                if acc.normalized() { " normalized" } else { "" }
            ));
        }
        let values: Vec<u32> = match acc.data_type() {
            DataType::U8 => Iter::<u8>::new(acc, get).ok_or_else(no_data)?.map(u32::from).collect(),
            DataType::U16 => Iter::<u16>::new(acc, get).ok_or_else(no_data)?.map(u32::from).collect(),
            DataType::U32 => Iter::<u32>::new(acc, get).ok_or_else(no_data)?.collect(),
            other => {
                return Err(format!("{TINY_SLOT_ATTRIBUTE} is {other:?}; it is an unsigned integer scalar"));
            }
        };
        counted(TINY_SLOT_ATTRIBUTE, values.len())?;
        out.tiny_slot = Some(values);
    }
    Ok(out)
}

/// A glTF material's `alphaMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

/// One glTF material as the TINY reader takes it: its alpha mode and its `extras`, as raw JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceMaterial {
    pub alpha_mode: AlphaMode,
    pub extras: Option<String>,
}

/// One triangle or triangle-strip primitive, kept apart from the others, with its vertices as the
/// file stores them.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePrimitive {
    /// `(mesh index, primitive index)`, for messages.
    pub at: (usize, usize),
    /// The primitive's `extras`, as raw JSON.
    pub extras: Option<String>,
    pub material: Option<usize>,
    /// `true` for `TRIANGLE_STRIP`, `false` for `TRIANGLES`.
    pub strip: bool,
    /// The index list, or `0..n` for an un-indexed primitive.
    pub indices: Vec<u32>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    /// [`TINY_SLOT_ATTRIBUTE`].
    pub slots: Option<Vec<u32>>,
}

/// Every triangle and triangle-strip primitive of a `.glb`/`.gltf`, each kept apart, and every
/// material, for a model built without a donor (the TINY far-distance stand-in).
///
/// Positions take their node's transform. That transform must be a rigid motion (a rotation and a
/// translation): normals turn with it and keep the lengths the file stores, which a scale would
/// change. A transform with a scale or shear is an error, as is a primitive of any other mode.
pub fn source_from_gltf(path: &Path) -> Result<(Vec<SourcePrimitive>, Vec<SourceMaterial>), String> {
    let (doc, buffers) = open_gltf(path)?;
    let materials = doc
        .materials()
        .map(|m| SourceMaterial {
            alpha_mode: match m.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => AlphaMode::Mask,
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            extras: m.extras().as_ref().map(|r| r.get().to_string()),
        })
        .collect();
    let mut out = Vec::new();
    const IDENTITY: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for scene in doc.scenes() {
        for node in scene.nodes() {
            visit_source(path, &node, IDENTITY, &buffers, &mut out)?;
        }
    }
    if out.is_empty() {
        return Err(format!("{}: no mesh primitives found", path.display()));
    }
    Ok((out, materials))
}

/// Whether `m` is a rotation and a translation: its 3×3 part orthonormal with determinant 1.
fn is_rigid(m: &Mat4) -> bool {
    let col = |c: usize| [m[c][0], m[c][1], m[c][2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (x, y, z) = (col(0), col(1), col(2));
    let close = |v: f32, w: f32| (v - w).abs() < 1e-5;
    let cross = [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
    close(dot(x, x), 1.0)
        && close(dot(y, y), 1.0)
        && close(dot(z, z), 1.0)
        && close(dot(x, y), 0.0)
        && close(dot(x, z), 0.0)
        && close(dot(y, z), 0.0)
        && close(dot(cross, z), 1.0)
        && m[0][3] == 0.0
        && m[1][3] == 0.0
        && m[2][3] == 0.0
        && m[3][3] == 1.0
}

/// A direction under a rigid transform's rotation, not renormalised.
fn rotate(m: &Mat4, d: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * d[0] + m[1][0] * d[1] + m[2][0] * d[2],
        m[0][1] * d[0] + m[1][1] * d[1] + m[2][1] * d[2],
        m[0][2] * d[0] + m[1][2] * d[1] + m[2][2] * d[2],
    ]
}

fn visit_source(
    path: &Path,
    node: &gltf::Node,
    parent: Mat4,
    buffers: &[Vec<u8>],
    out: &mut Vec<SourcePrimitive>,
) -> Result<(), String> {
    let world = mul(parent, node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        if !is_rigid(&world) {
            return Err(format!(
                "{}: mesh {} sits under a node transform with a scale or shear; apply it to the \
                 vertices, so the stored normals stay the ones the file means",
                path.display(),
                mesh.index()
            ));
        }
        let identity = world == [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        for prim in mesh.primitives() {
            let at = (mesh.index(), prim.index());
            let strip = match prim.mode() {
                gltf::mesh::Mode::Triangles => false,
                gltf::mesh::Mode::TriangleStrip => true,
                other => {
                    return Err(format!(
                        "{}: mesh {} primitive {} is {other:?}; a TINY model takes TRIANGLES or \
                         TRIANGLE_STRIP",
                        path.display(),
                        at.0,
                        at.1
                    ));
                }
            };
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d[..]));
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| format!("{}: mesh {} primitive {} has no POSITION", path.display(), at.0, at.1))?
                .map(|p| if identity { p } else { transform_point(&world, p) })
                .collect();
            let normals = reader
                .read_normals()
                .map(|n| n.map(|v| if identity { v } else { rotate(&world, v) }).collect());
            let uvs = reader.read_tex_coords(0).map(|t| t.into_f32().collect());
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            let custom = primitive_custom(&prim, buffers, positions.len())
                .map_err(|e| format!("{}: mesh {} primitive {}: {e}", path.display(), at.0, at.1))?;
            out.push(SourcePrimitive {
                at,
                extras: prim.extras().as_ref().map(|r| r.get().to_string()),
                material: prim.material().index(),
                strip,
                indices,
                positions,
                normals,
                uvs,
                slots: custom.tiny_slot,
            });
        }
    }
    for child in node.children() {
        visit_source(path, &child, world, buffers, out)?;
    }
    Ok(())
}

type Mat4 = [[f32; 4]; 4];

fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][j] * b[i][k]).sum();
        }
    }
    out
}

/// glTF matrices are column-major; `p` is treated as a point (w = 1).
fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

/// Directions ignore translation. Not a full inverse-transpose: a non-uniform scale would skew
/// these, which is acceptable for a rigid prop whose normals the engine re-derives per group, and
/// is called out here so nobody mistakes it for correct under shear.
fn transform_dir(m: &Mat4, d: [f32; 3]) -> [f32; 3] {
    let out = [
        m[0][0] * d[0] + m[1][0] * d[1] + m[2][0] * d[2],
        m[0][1] * d[0] + m[1][1] * d[1] + m[2][1] * d[2],
        m[0][2] * d[0] + m[1][2] * d[1] + m[2][2] * d[2],
    ];
    let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
    if len > 1e-6 {
        [out[0] / len, out[1] / len, out[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

#[allow(clippy::too_many_arguments)]
fn visit(
    node: &gltf::Node,
    parent: Mat4,
    buffers: &[Vec<u8>],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    tris: &mut Vec<[u32; 3]>,
    custom: &mut Vec<PrimitiveCustom>,
) -> Result<(), String> {
    let world = mul(parent, node.transform().matrix());

    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                // Skipped rather than errored: a file may legitimately carry helper geometry.
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d[..]));
            let Some(pos) = reader.read_positions() else {
                continue;
            };
            let base = positions.len() as u32;

            for p in pos {
                positions.push(transform_point(&world, p));
            }
            let added = positions.len() as u32 - base;

            custom.push(primitive_custom(&prim, buffers, added as usize)?);

            if let Some(n) = reader.read_normals() {
                for v in n {
                    normals.push(transform_dir(&world, v));
                }
            }
            normals.resize(positions.len(), [0.0, 1.0, 0.0]);

            if let Some(t) = reader.read_tex_coords(0) {
                for v in t.into_f32() {
                    uvs.push(v);
                }
            }
            uvs.resize(positions.len(), [0.0, 0.0]);

            match reader.read_indices() {
                Some(idx) => {
                    let flat: Vec<u32> = idx.into_u32().collect();
                    for c in flat.chunks_exact(3) {
                        tris.push([base + c[0], base + c[1], base + c[2]]);
                    }
                }
                // Un-indexed primitives are sequential triples.
                None => {
                    for i in (0..added).step_by(3) {
                        if i + 2 < added {
                            tris.push([base + i, base + i + 1, base + i + 2]);
                        }
                    }
                }
            }
        }
    }

    for child in node.children() {
        visit(&child, world, buffers, positions, normals, uvs, tris, custom)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_translated_node_moves_its_vertices() {
        let mut m: Mat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        m[3] = [10.0, 20.0, 30.0, 1.0];
        assert_eq!(transform_point(&m, [1.0, 2.0, 3.0]), [11.0, 22.0, 33.0]);
        // A direction must ignore the translation, or every normal points at the origin offset.
        assert_eq!(transform_dir(&m, [0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn directions_come_back_normalised() {
        let mut m: Mat4 = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        m[3] = [5.0, 5.0, 5.0, 1.0];
        let d = transform_dir(&m, [3.0, 0.0, 0.0]);
        assert!((d[0] - 1.0).abs() < 1e-5 && d[1].abs() < 1e-5, "{d:?}");
    }

    /// One custom attribute of a fixture primitive: `(name, glTF componentType, value bytes)`.
    type Attr = (&'static str, u32, Vec<u8>);

    /// A `.gltf` with one three-vertex triangle primitive per entry of `prims`, each carrying the
    /// listed custom attributes, its buffer beside it.
    fn gltf_with(dir: &Path, prims: &[Vec<Attr>]) -> std::path::PathBuf {
        let mut bin: Vec<u8> = Vec::new();
        let mut views = Vec::new();
        let mut accessors = Vec::new();
        let mut out = Vec::new();
        for attrs in prims {
            let pos_at = bin.len();
            for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
                for c in v {
                    bin.extend_from_slice(&c.to_le_bytes());
                }
            }
            views.push(format!(r#"{{"buffer":0,"byteOffset":{pos_at},"byteLength":36}}"#));
            accessors.push(format!(
                r#"{{"bufferView":{},"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}"#,
                views.len() - 1
            ));
            let mut json_attrs = format!(r#""POSITION":{}"#, accessors.len() - 1);
            for (name, component, bytes) in attrs {
                while bin.len() % 4 != 0 {
                    bin.push(0);
                }
                let at = bin.len();
                bin.extend_from_slice(bytes);
                views.push(format!(r#"{{"buffer":0,"byteOffset":{at},"byteLength":{}}}"#, bytes.len()));
                accessors.push(format!(
                    r#"{{"bufferView":{},"componentType":{component},"count":3,"type":"SCALAR"}}"#,
                    views.len() - 1
                ));
                json_attrs.push_str(&format!(r#","{name}":{}"#, accessors.len() - 1));
            }
            out.push(format!(r#"{{"attributes":{{{json_attrs}}}}}"#));
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        std::fs::write(dir.join("m.bin"), &bin).unwrap();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{}]}}],"buffers":[{{"uri":"m.bin","byteLength":{}}}],
"bufferViews":[{}],"accessors":[{}]}}"#,
            out.join(","),
            bin.len(),
            views.join(","),
            accessors.join(",")
        );
        let path = dir.join("m.gltf");
        std::fs::write(&path, json).unwrap();
        path
    }

    fn sway(w: [f32; 3]) -> Attr {
        ("_SWAY_WEIGHT", 5126, w.iter().flat_map(|x| x.to_le_bytes()).collect())
    }

    fn slot_u8(s: [u8; 3]) -> Attr {
        ("_TINY_SLOT", 5121, s.to_vec())
    }

    fn slot_u16(s: [u16; 3]) -> Attr {
        ("_TINY_SLOT", 5123, s.iter().flat_map(|x| x.to_le_bytes()).collect())
    }

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mesh_import_{}_{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn custom_attributes_follow_the_vertex_order() {
        let dir = scratch("custom");
        let path = gltf_with(
            &dir,
            &[vec![sway([0.0, 0.5, 1.0]), slot_u8([0, 0, 0])], vec![sway([0.25, 0.25, 0.75]), slot_u16([2, 2, 2])]],
        );
        assert_eq!(external_mesh_from_gltf(&path).unwrap().positions.len(), 6);
        let c = custom_attributes_from_gltf(&path).unwrap();
        assert_eq!(c.sway, Some(vec![0.0, 0.5, 1.0, 0.25, 0.25, 0.75]));
        assert_eq!(c.tiny_slot, Some(vec![0, 0, 0, 2, 2, 2]));
    }

    #[test]
    fn absent_custom_attributes_read_as_none() {
        let dir = scratch("none");
        let path = gltf_with(&dir, &[vec![]]);
        assert_eq!(custom_attributes_from_gltf(&path).unwrap(), CustomAttributes::default());
    }

    #[test]
    fn a_partial_or_out_of_range_custom_attribute_is_refused() {
        let dir = scratch("partial");
        let path = gltf_with(&dir, &[vec![sway([0.0, 0.5, 1.0])], vec![]]);
        let e = custom_attributes_from_gltf(&path).unwrap_err();
        assert!(e.contains("_SWAY_WEIGHT") && e.contains("primitive 1"), "{e}");
        let dir = scratch("range");
        let path = gltf_with(&dir, &[vec![sway([0.0, 1.5, 1.0])]]);
        let e = custom_attributes_from_gltf(&path).unwrap_err();
        assert!(e.contains("_SWAY_WEIGHT") && e.contains("1.5"), "{e}");
        let dir = scratch("slot_partial");
        let path = gltf_with(&dir, &[vec![], vec![slot_u8([1, 1, 1])]]);
        let e = custom_attributes_from_gltf(&path).unwrap_err();
        assert!(e.contains("_TINY_SLOT") && e.contains("primitive 0"), "{e}");
        let dir = scratch("slot_float");
        let path = gltf_with(&dir, &[vec![("_TINY_SLOT", 5126, [1.0f32; 3].iter().flat_map(|x| x.to_le_bytes()).collect())]]);
        let e = custom_attributes_from_gltf(&path).unwrap_err();
        assert!(e.contains("_TINY_SLOT") && e.contains("unsigned integer"), "{e}");
    }

    /// A `.gltf` with one primitive of `mode` (four vertices, a normal and a uv each, a `_TINY_SLOT`)
    /// on a node with `matrix`, and two materials.
    fn source_gltf(dir: &Path, mode: u32, matrix: Option<[f32; 16]>) -> std::path::PathBuf {
        let mut bin: Vec<u8> = Vec::new();
        let pos = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        for v in pos {
            for c in v {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for _ in 0..4 {
            for c in [0.0f32, 0.6, 0.8] {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for k in 0..4 {
            for c in [k as f32 * 0.25, 0.5] {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        bin.extend_from_slice(&[3, 3, 3, 3]);
        let node = match matrix {
            Some(m) => format!(
                r#"{{"mesh":0,"matrix":[{}]}}"#,
                m.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
            ),
            None => r#"{"mesh":0}"#.to_string(),
        };
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{node}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2,"_TINY_SLOT":3}},"mode":{mode},"material":1,"extras":{{"tiny_role":"ruined"}}}}]}}],
"materials":[{{"alphaMode":"OPAQUE"}},{{"alphaMode":"MASK","extras":{{"texture":"t"}}}}],
"accessors":[
{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
{{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},
{{"bufferView":2,"componentType":5126,"count":4,"type":"VEC2"}},
{{"bufferView":3,"componentType":5121,"count":4,"type":"SCALAR"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},
{{"buffer":0,"byteOffset":96,"byteLength":32}},{{"buffer":0,"byteOffset":128,"byteLength":4}}],
"buffers":[{{"uri":"s.bin","byteLength":{}}}]}}"#,
            bin.len()
        );
        std::fs::write(dir.join("s.bin"), &bin).unwrap();
        let path = dir.join("s.gltf");
        std::fs::write(&path, json).unwrap();
        path
    }

    #[test]
    fn a_source_primitive_keeps_its_mode_vertices_material_and_extras() {
        let dir = scratch("source_strip");
        let (prims, mats) = source_from_gltf(&source_gltf(&dir, 5, None)).unwrap();
        assert_eq!(prims.len(), 1);
        let p = &prims[0];
        assert!(p.strip);
        assert_eq!(p.indices, vec![0, 1, 2, 3]);
        assert_eq!(p.material, Some(1));
        assert_eq!(p.positions[3], [1.0, 1.0, 0.0]);
        assert_eq!(p.normals.as_ref().unwrap()[0], [0.0, 0.6, 0.8]);
        assert_eq!(p.uvs.as_ref().unwrap()[2], [0.5, 0.5]);
        assert_eq!(p.slots, Some(vec![3, 3, 3, 3]));
        assert_eq!(p.extras.as_deref(), Some(r#"{"tiny_role":"ruined"}"#));
        assert_eq!(mats[0], SourceMaterial { alpha_mode: AlphaMode::Opaque, extras: None });
        assert_eq!(mats[1].alpha_mode, AlphaMode::Mask);
        assert_eq!(mats[1].extras.as_deref(), Some(r#"{"texture":"t"}"#));
    }

    #[test]
    fn a_rigid_node_moves_positions_and_rotates_normals_and_a_scale_is_refused() {
        let dir = scratch("source_rigid");
        // a quarter turn about +X, then a translation of (10, 0, 0)
        let m = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 10.0, 0.0, 0.0, 1.0];
        let (prims, _) = source_from_gltf(&source_gltf(&dir, 4, Some(m))).unwrap();
        assert!(!prims[0].strip);
        assert_eq!(prims[0].positions[2], [10.0, 0.0, 1.0]);
        let n = prims[0].normals.as_ref().unwrap()[0];
        assert!((n[0]).abs() < 1e-6 && (n[1] + 0.8).abs() < 1e-6 && (n[2] - 0.6).abs() < 1e-6, "{n:?}");

        let dir = scratch("source_scaled");
        let s = [2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let e = source_from_gltf(&source_gltf(&dir, 4, Some(s))).unwrap_err();
        assert!(e.contains("scale"), "{e}");

        let dir = scratch("source_points");
        let e = source_from_gltf(&source_gltf(&dir, 0, None)).unwrap_err();
        assert!(e.contains("TRIANGLES or TRIANGLE_STRIP"), "{e}");
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let err = external_mesh_from_gltf(Path::new("/nope/model.glb")).unwrap_err();
        assert!(err.contains("/nope/model.glb"), "{err}");
    }
}
