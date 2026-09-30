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

/// Every vertex's [`SWAY_WEIGHT_ATTRIBUTE`] value, in the vertex order of
/// [`external_mesh_from_gltf`], or `None` when no primitive carries the attribute.
///
/// A file where some primitives carry it and others do not is an error, as is an accessor that is
/// not a float scalar or a value outside 0..1: each names the attribute.
pub fn sway_weights_from_gltf(path: &Path) -> Result<Option<Vec<f32>>, String> {
    let (_, per_primitive) = read_gltf(path)?;
    if per_primitive.iter().all(|p| p.is_none()) {
        return Ok(None);
    }
    let mut out = Vec::new();
    for (i, p) in per_primitive.into_iter().enumerate() {
        let w = p.ok_or_else(|| {
            format!(
                "{}: triangle primitive {i} has no {SWAY_WEIGHT_ATTRIBUTE} attribute and another \
                 primitive does; every vertex takes a sway weight",
                path.display()
            )
        })?;
        out.extend(w);
    }
    if let Some((v, x)) = out.iter().enumerate().find(|(_, x)| !(0.0..=1.0).contains(*x)) {
        return Err(format!(
            "{}: vertex {v} has {SWAY_WEIGHT_ATTRIBUTE} {x}; a sway weight is 0 to 1",
            path.display()
        ));
    }
    Ok(Some(out))
}

/// The mesh, and each triangle primitive's sway weights (in the order the primitives are read).
type Read = (ExternalMesh, Vec<Option<Vec<f32>>>);

fn read_gltf(path: &Path) -> Result<Read, String> {
    let (doc, buffers) = open_gltf(path)?;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut sway: Vec<Option<Vec<f32>>> = Vec::new();

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
                &mut sway,
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
        sway,
    ))
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
    sway: &mut Vec<Option<Vec<f32>>>,
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

            let semantic = gltf::Semantic::Extras(SWAY_WEIGHT_ATTRIBUTE[1..].to_string());
            match prim.get(&semantic) {
                None => sway.push(None),
                Some(acc) => {
                    if acc.dimensions() != gltf::accessor::Dimensions::Scalar
                        || acc.data_type() != gltf::accessor::DataType::F32
                    {
                        return Err(format!(
                            "{SWAY_WEIGHT_ATTRIBUTE} is {:?} {:?}; it is a float scalar",
                            acc.dimensions(),
                            acc.data_type()
                        ));
                    }
                    let values: Vec<f32> =
                        gltf::accessor::Iter::<f32>::new(acc, |b| buffers.get(b.index()).map(|d| &d[..]))
                            .ok_or_else(|| format!("{SWAY_WEIGHT_ATTRIBUTE}: its accessor has no buffer data"))?
                            .collect();
                    if values.len() != added as usize {
                        return Err(format!(
                            "{SWAY_WEIGHT_ATTRIBUTE} has {} values for {added} vertices",
                            values.len()
                        ));
                    }
                    sway.push(Some(values));
                }
            }

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
        visit(&child, world, buffers, positions, normals, uvs, tris, sway)?;
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

    /// A `.gltf` with one triangle primitive per entry of `sway`, each with or without the
    /// attribute, its buffer beside it.
    fn gltf_with_sway(dir: &Path, sway: &[Option<[f32; 3]>]) -> std::path::PathBuf {
        let mut bin: Vec<u8> = Vec::new();
        let mut views = Vec::new();
        let mut accessors = Vec::new();
        let mut prims = Vec::new();
        for s in sway {
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
            let mut attrs = format!(r#""POSITION":{}"#, accessors.len() - 1);
            if let Some(w) = s {
                let at = bin.len();
                for c in w {
                    bin.extend_from_slice(&c.to_le_bytes());
                }
                views.push(format!(r#"{{"buffer":0,"byteOffset":{at},"byteLength":12}}"#));
                accessors.push(format!(r#"{{"bufferView":{},"componentType":5126,"count":3,"type":"SCALAR"}}"#, views.len() - 1));
                attrs.push_str(&format!(r#","_SWAY_WEIGHT":{}"#, accessors.len() - 1));
            }
            prims.push(format!(r#"{{"attributes":{{{attrs}}}}}"#));
        }
        std::fs::write(dir.join("m.bin"), &bin).unwrap();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{}]}}],"buffers":[{{"uri":"m.bin","byteLength":{}}}],
"bufferViews":[{}],"accessors":[{}]}}"#,
            prims.join(","),
            bin.len(),
            views.join(","),
            accessors.join(",")
        );
        let path = dir.join("m.gltf");
        std::fs::write(&path, json).unwrap();
        path
    }

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mesh_import_{}_{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sway_weights_follow_the_vertex_order() {
        let dir = scratch("sway");
        let path = gltf_with_sway(&dir, &[Some([0.0, 0.5, 1.0]), Some([0.25, 0.25, 0.75])]);
        assert_eq!(external_mesh_from_gltf(&path).unwrap().positions.len(), 6);
        assert_eq!(sway_weights_from_gltf(&path).unwrap(), Some(vec![0.0, 0.5, 1.0, 0.25, 0.25, 0.75]));
    }

    #[test]
    fn no_sway_attribute_reads_as_none() {
        let dir = scratch("no_sway");
        let path = gltf_with_sway(&dir, &[None]);
        assert_eq!(sway_weights_from_gltf(&path).unwrap(), None);
    }

    #[test]
    fn a_partial_or_out_of_range_sway_attribute_is_refused() {
        let dir = scratch("partial");
        let path = gltf_with_sway(&dir, &[Some([0.0, 0.5, 1.0]), None]);
        let e = sway_weights_from_gltf(&path).unwrap_err();
        assert!(e.contains("_SWAY_WEIGHT") && e.contains("primitive 1"), "{e}");
        let dir = scratch("range");
        let path = gltf_with_sway(&dir, &[Some([0.0, 1.5, 1.0])]);
        let e = sway_weights_from_gltf(&path).unwrap_err();
        assert!(e.contains("_SWAY_WEIGHT") && e.contains("1.5"), "{e}");
    }

    #[test]
    fn a_missing_file_names_the_path() {
        let err = external_mesh_from_gltf(Path::new("/nope/model.glb")).unwrap_err();
        assert!(err.contains("/nope/model.glb"), "{err}");
    }
}
