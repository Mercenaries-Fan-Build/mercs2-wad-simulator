//! Authored Havok `PHY2` collision → triangle soup.
//!
//! The streaming collision path used to derive every asset's collider from its **render** mesh
//! (`extract_local_tris`). Retail streams **authored** `hkpConvexVerticesShape` / mesh / box shapes per
//! asset in the model's `PHY2` chunk. This module turns the shapes decoded by
//! [`mercs2_formats::havok`] into **model-local** collision triangles the soup broadphase consumes.
//!
//! What is wired here:
//! * [`Shape::Convex`] — the authored convex break-piece hull. Faithful: each authored plane is a face;
//!   its on-plane vertices are ordered around the plane normal and fan-triangulated ([`hull_tris`]).
//!   These hulls are far lower-poly than the render mesh (a crate: 6 hulls vs its full render shell).
//! * [`Shape::Box`] — half-extents at the shape's local origin → 12 tris ([`box_tris`]).
//!
//! What is **not** wired here (and why the caller must fall back to the render mesh for a unit that
//! carries it):
//! * [`Shape::Mesh`] (`WpMeshShape16`) — the static building/terrain collision mesh, now DECODED
//!   ([`mercs2_formats::havok::MeshShape`]: dequantized verts + index triples). A **building-scale** mesh
//!   (small XZ span / low tri count) drops straight into the soup, so the unit stops needing the render
//!   fallback. A **terrain-scale** mesh (a full c3 cell, ≈400 m XZ span, ~10k–18k tris) is deliberately
//!   NOT fed into the soup — that would re-create the 273k-triangle rebuild churn the heightfield fix
//!   removed. The terrain heightfield already covers those cells, so a terrain-scale mesh is counted
//!   ([`AuthoredCollision::n_terrain_mesh`]) and skipped. See [`TERRAIN_MESH_SPAN_M`]/[`TERRAIN_MESH_TRIS`].
//! * [`Shape::Mopp`] — the MOPP BV-tree (`hkpMoppBvTreeShape` + `hkpMoppCode`) WRAPPING a `WpMeshShape16`
//!   — its geometry IS the wrapped mesh (retail pairs 2 MOPP classes per WpMesh). So a MOPP alongside a
//!   decoded WpMesh is NOT extra undecoded geometry; only a MOPP with NO decoded WpMesh in the body still
//!   counts as an undecoded static mesh (verified live: building props carry mesh + its MOPP wrapper, and
//!   treating the MOPP as undecoded wrongly vetoed the whole authored collider).
//! * [`Shape::Capsule`] / [`Shape::Sphere`] — ragdoll-limb / character colliders, authored in a body's
//!   local frame with no model-space transform in the shape itself; they are NOT static world geometry,
//!   so they are counted and skipped rather than misplaced at the model origin.

use mercs2_core::glam::Vec3;
use mercs2_formats::havok::{ConvexHull, MeshShape, Shape};

/// XZ-span threshold (m) above which a decoded `WpMeshShape16` is treated as a **terrain-scale** cell
/// collider and kept OUT of the triangle soup (the terrain heightfield covers it). A full c3 terrain cell
/// spans ≈400 m in both X and Z; a building collider is ≤ ~40 m — so 300 m cleanly separates them.
pub const TERRAIN_MESH_SPAN_M: f32 = 300.0;
/// Triangle-count threshold above which a decoded `WpMeshShape16` is treated as terrain-scale (a second,
/// independent guard on the same split: buildings run 15–~400 tris, terrain cells 9k–18k). Either guard
/// tripping routes the mesh away from the soup.
pub const TERRAIN_MESH_TRIS: usize = 2000;

/// Model-local authored collision recovered from a `PHY2` body, plus the census the caller needs to
/// decide whether it is COMPLETE enough to replace the render-mesh collider.
#[derive(Debug, Clone, Default)]
pub struct AuthoredCollision {
    /// Model-local collision triangles (convex hulls + boxes). Empty when the body has no decodable
    /// static shape.
    pub tris: Vec<[Vec3; 3]>,
    /// The body carries a static collision **mesh** we cannot triangulate (an undecodable
    /// `WpMeshShape16`, or a `hkpMoppBvTreeShape` whose wrapped WpMesh we don't reach). When true the
    /// convex/box tris are only PART of the collider (e.g. a building's break pieces without its walls),
    /// so the caller must keep the render-mesh fallback.
    pub has_undecoded_mesh: bool,
    pub n_hulls: usize,
    pub n_box: usize,
    pub n_sphere: usize,
    pub n_capsule: usize,
    /// Building-scale `WpMeshShape16` meshes emitted into [`Self::tris`].
    pub n_mesh: usize,
    /// Terrain-scale `WpMeshShape16` meshes recognised and DELIBERATELY skipped from the soup (covered by
    /// the terrain heightfield). Their presence still makes the collider "complete" — the caller must NOT
    /// fall back to the render mesh for a terrain cell (that is the soup flood we are avoiding).
    pub n_terrain_mesh: usize,
    /// MOPP shapes (`hkpMoppBvTreeShape` + `hkpMoppCode`) — the BV-tree WRAPPER around a `WpMeshShape16`,
    /// not separate geometry. Only an UNACCOUNTED MOPP (one with no decoded WpMesh in the body) marks the
    /// collider incomplete.
    pub n_mopp: usize,
}

impl AuthoredCollision {
    /// Whether these authored tris are a COMPLETE static collider — no undecoded static mesh left to
    /// account for, AND either it produced soup tris OR it accounted for the body's geometry with a
    /// terrain-scale mesh routed to the heightfield (empty soup, but NOT a case for the render fallback).
    /// The streaming loader uses this to choose authored PHY2 over the render-mesh soup for a unit.
    pub fn is_complete(&self) -> bool {
        !self.has_undecoded_mesh && (!self.tris.is_empty() || self.n_terrain_mesh > 0)
    }
}

/// Triangulate every authored shape in a decoded `PHY2` body into model-local collision tris.
///
/// See the module docs for the per-shape policy. The returned [`AuthoredCollision::has_undecoded_mesh`]
/// flag is the signal the caller uses to keep the render-mesh fallback for a mesh-collider unit.
pub fn authored_collision(shapes: &[Shape]) -> AuthoredCollision {
    let mut out = AuthoredCollision::default();
    // A `WpMeshShape16` that failed to decode (empty verts/indices) — a genuinely undecoded static mesh.
    let mut saw_undecoded_mesh = false;
    for s in shapes {
        match s {
            Shape::Convex(h) => {
                out.n_hulls += 1;
                out.tris.extend(hull_tris(h));
            }
            Shape::Box { half_extents } => {
                out.n_box += 1;
                out.tris.extend(box_tris(*half_extents));
            }
            // Decoded `WpMeshShape16`. Route by scale: a building-scale mesh drops its tris into the soup
            // (the unit stops needing the render fallback); a terrain-scale mesh (full c3 cell) is skipped
            // — the terrain heightfield covers it, and flooding the soup with ~10k–18k tris per cell would
            // re-create the broadphase-rebuild churn the heightfield fix removed. An empty (undecoded)
            // mesh keeps the render fallback.
            Shape::Mesh(mesh) => {
                if mesh.indices.is_empty() {
                    saw_undecoded_mesh = true;
                } else {
                    let [sx, sz] = mesh.xz_span();
                    let terrain_scale = sx > TERRAIN_MESH_SPAN_M
                        || sz > TERRAIN_MESH_SPAN_M
                        || mesh.indices.len() > TERRAIN_MESH_TRIS;
                    if terrain_scale {
                        out.n_terrain_mesh += 1;
                    } else {
                        out.n_mesh += 1;
                        out.tris.extend(mesh_tris(mesh));
                    }
                }
            }
            // MOPP is the BV-TREE WRAPPER around a `WpMeshShape16` — NOT separate geometry. In retail each
            // WpMesh is paired with `hkpMoppBvTreeShape` + `hkpMoppCode` (so `n_mopp == 2 * n_meshes`).
            // Counted here; whether it leaves an UNACCOUNTED static mesh is decided after the loop: only a
            // MOPP with NO decoded WpMesh in the same body is an undecoded static mesh (keep the fallback).
            Shape::Mopp => out.n_mopp += 1,
            // Ragdoll/character body colliders — not static world geometry (no model-space transform in
            // the shape). Counted, not emitted, so they are never misplaced at the model origin.
            Shape::Sphere { .. } => out.n_sphere += 1,
            Shape::Capsule(_) => out.n_capsule += 1,
            Shape::Other(_) => {}
        }
    }
    // Decide undecoded-static-mesh status ONCE, after seeing the whole body: a WpMesh that failed to
    // decode is always undecoded; a MOPP counts as undecoded ONLY when no WpMesh in the body decoded
    // (otherwise the MOPP is just the BV-tree wrapping the mesh we DID decode — not extra geometry).
    let decoded_any_mesh = out.n_mesh > 0 || out.n_terrain_mesh > 0;
    out.has_undecoded_mesh = saw_undecoded_mesh || (out.n_mopp > 0 && !decoded_any_mesh);
    out
}

/// Triangulate ONE convex hull's authored faces into model-local collision triangles.
///
/// Faithful to the authored half-space representation: each plane `n·x + w = 0` (from
/// [`ConvexHull::planes`], `w = -support`) is a face; the hull vertices lying on it
/// (`|n·v + w| ≤ eps`) are ordered CCW around `n` and fan-triangulated, with each triangle's winding
/// oriented so its geometric normal agrees with the outward plane normal `n`. `eps` scales with the
/// hull's diagonal so it is robust across a matchstick and a building-sized break piece alike.
pub fn hull_tris(hull: &ConvexHull) -> Vec<[Vec3; 3]> {
    let verts: Vec<Vec3> = hull.vertices.iter().map(|v| Vec3::from(*v)).collect();
    if verts.len() < 3 || hull.planes.is_empty() {
        return Vec::new();
    }
    // Scale-aware on-plane tolerance from the hull's bbox diagonal.
    let (mut lo, mut hi) = (verts[0], verts[0]);
    for v in &verts {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let diag = (hi - lo).length();
    let eps = (diag * 1e-3).max(1e-4);

    let mut tris = Vec::new();
    for p in &hull.planes {
        let n = Vec3::new(p[0], p[1], p[2]);
        let nl = n.length();
        if nl < 1e-6 {
            continue;
        }
        let n = n / nl;
        let w = p[3] / nl;
        // Vertices on this face.
        let mut face: Vec<Vec3> = verts.iter().copied().filter(|v| (n.dot(*v) + w).abs() <= eps).collect();
        if face.len() < 3 {
            continue;
        }
        // Order them around the face normal. Basis: u ⟂ n, then v_axis = n × u.
        let c = face.iter().copied().fold(Vec3::ZERO, |a, v| a + v) / face.len() as f32;
        let seed = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let u = (seed - n * seed.dot(n)).normalize_or_zero();
        if u == Vec3::ZERO {
            continue;
        }
        let vx = n.cross(u);
        face.sort_by(|a, b| {
            let pa = *a - c;
            let pb = *b - c;
            let aa = pa.dot(vx).atan2(pa.dot(u));
            let ab = pb.dot(vx).atan2(pb.dot(u));
            aa.total_cmp(&ab)
        });
        // Fan-triangulate, orienting each tri outward (normal along n).
        for i in 1..face.len() - 1 {
            let (a, mut b, mut cc) = (face[0], face[i], face[i + 1]);
            if (b - a).cross(cc - a).dot(n) < 0.0 {
                std::mem::swap(&mut b, &mut cc);
            }
            // Drop degenerate (collinear) slivers.
            if (b - a).cross(cc - a).length() > 1e-9 {
                tris.push([a, b, cc]);
            }
        }
    }
    tris
}

/// The 12 triangles of an axis-aligned box of the given half-extents, centered at the shape's local
/// origin (outward-facing winding).
pub fn box_tris(half: [f32; 3]) -> Vec<[Vec3; 3]> {
    let (x, y, z) = (half[0], half[1], half[2]);
    // 8 corners.
    let c = [
        Vec3::new(-x, -y, -z),
        Vec3::new(x, -y, -z),
        Vec3::new(x, y, -z),
        Vec3::new(-x, y, -z),
        Vec3::new(-x, -y, z),
        Vec3::new(x, -y, z),
        Vec3::new(x, y, z),
        Vec3::new(-x, y, z),
    ];
    // 6 faces, each two tris, wound CCW outward.
    let quads = [
        [0, 3, 2, 1], // -z
        [4, 5, 6, 7], // +z
        [0, 1, 5, 4], // -y
        [3, 7, 6, 2], // +y
        [0, 4, 7, 3], // -x
        [1, 2, 6, 5], // +x
    ];
    let mut tris = Vec::with_capacity(12);
    for q in quads {
        tris.push([c[q[0]], c[q[1]], c[q[2]]]);
        tris.push([c[q[0]], c[q[2]], c[q[3]]]);
    }
    tris
}

/// Model-local collision triangles of a decoded `WpMeshShape16`: each index triple gathers its three
/// dequantized vertices (skipping any triple with an out-of-range index — a malformed decode). Same
/// model-local soup contribution as [`hull_tris`]/[`box_tris`].
pub fn mesh_tris(mesh: &MeshShape) -> Vec<[Vec3; 3]> {
    mesh.indices
        .iter()
        .filter_map(|t| {
            let a = mesh.vertices.get(t[0] as usize)?;
            let b = mesh.vertices.get(t[1] as usize)?;
            let c = mesh.vertices.get(t[2] as usize)?;
            Some([Vec3::from(*a), Vec3::from(*b), Vec3::from(*c)])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube expressed as an `hkpConvexVerticesShape` (8 verts, 6 axis planes) triangulates to
    /// exactly 12 tris that enclose the cube volume (centroids of the 6 faces sit on the box surface).
    #[test]
    fn hull_tris_triangulates_a_cube() {
        let v = |x: f32, y: f32, z: f32| [x, y, z];
        let hull = ConvexHull {
            vertices: vec![
                v(-1., -1., -1.), v(1., -1., -1.), v(1., 1., -1.), v(-1., 1., -1.),
                v(-1., -1., 1.), v(1., -1., 1.), v(1., 1., 1.), v(-1., 1., 1.),
            ],
            // plane n·x + w = 0, w = -support: for +X face support=1 → w=-1.
            planes: vec![
                [1., 0., 0., -1.], [-1., 0., 0., -1.],
                [0., 1., 0., -1.], [0., -1., 0., -1.],
                [0., 0., 1., -1.], [0., 0., -1., -1.],
            ],
        };
        let tris = hull_tris(&hull);
        assert_eq!(tris.len(), 12, "6 quad faces → 12 tris, got {}", tris.len());
        // Every triangle vertex is a cube corner (|coord| == 1 on each axis).
        for t in &tris {
            for p in t {
                assert!(p.x.abs() == 1.0 && p.y.abs() == 1.0 && p.z.abs() == 1.0, "off-cube vertex {p:?}");
            }
        }
        // Outward orientation: each face's tri normal points away from the cube center (origin).
        for t in &tris {
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            let centroid = (t[0] + t[1] + t[2]) / 3.0;
            assert!(n.dot(centroid) > 0.0, "face wound inward: n={n:?} c={centroid:?}");
        }
    }

    #[test]
    fn box_tris_is_twelve_and_outward() {
        let tris = box_tris([2.0, 1.0, 0.5]);
        assert_eq!(tris.len(), 12);
        for t in &tris {
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            let centroid = (t[0] + t[1] + t[2]) / 3.0;
            assert!(n.dot(centroid) > 0.0, "box face wound inward");
        }
    }

    /// The gating census: a body with only convex hulls is COMPLETE; adding an undecoded static mesh
    /// makes it incomplete (caller must keep the render fallback); capsules/spheres are counted, never
    /// emitted as world tris.
    #[test]
    fn authored_collision_gates_on_undecoded_mesh() {
        let cube = ConvexHull {
            vertices: vec![
                [-1., -1., -1.], [1., -1., -1.], [1., 1., -1.], [-1., 1., -1.],
                [-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.],
            ],
            planes: vec![
                [1., 0., 0., -1.], [-1., 0., 0., -1.], [0., 1., 0., -1.],
                [0., -1., 0., -1.], [0., 0., 1., -1.], [0., 0., -1., -1.],
            ],
        };
        let convex_only = authored_collision(&[Shape::Convex(cube.clone())]);
        assert!(convex_only.is_complete(), "a pure-convex body is a complete authored collider");
        assert_eq!(convex_only.n_hulls, 1);
        assert_eq!(convex_only.tris.len(), 12);

        // An UNDECODED mesh (empty verts/indices) forces the render fallback.
        let empty_mesh = Shape::Mesh(MeshShape { vertices: Vec::new(), indices: Vec::new() });
        let with_mesh = authored_collision(&[Shape::Convex(cube.clone()), empty_mesh]);
        assert!(!with_mesh.is_complete(), "an undecoded static mesh forces the render fallback");
        assert!(with_mesh.has_undecoded_mesh);

        // A BUILDING-scale decoded mesh (small span, few tris) drops its tris into the soup and needs no
        // fallback. Two tris forming a small quad at the origin.
        let bldg = Shape::Mesh(MeshShape {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            indices: vec![[0, 1, 2], [0, 2, 3]],
        });
        let with_bldg = authored_collision(&[bldg]);
        assert!(with_bldg.is_complete(), "a decoded building mesh is a complete authored collider");
        assert_eq!(with_bldg.n_mesh, 1);
        assert_eq!(with_bldg.tris.len(), 2, "both building-mesh tris go into the soup");

        // A TERRAIN-scale decoded mesh (≈400 m XZ span) is recognised and kept OUT of the soup, yet still
        // counts as complete so the caller does not render-fallback a terrain cell into the soup.
        let terrain = Shape::Mesh(MeshShape {
            vertices: vec![[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 0.0, 400.0]],
            indices: vec![[0, 1, 2]],
        });
        let with_terrain = authored_collision(&[terrain]);
        assert!(with_terrain.tris.is_empty(), "a terrain-scale mesh must NOT flood the soup");
        assert_eq!(with_terrain.n_terrain_mesh, 1);
        assert!(with_terrain.is_complete(), "a terrain cell is complete (heightfield covers it), no fallback");

        // A MOPP alongside a DECODED building mesh is that mesh's BV-tree wrapper, NOT extra geometry — the
        // body stays complete (this is the real-data shape of a building prop: WpMesh + its MOPP pair).
        let bldg2 = Shape::Mesh(MeshShape {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            indices: vec![[0, 1, 2]],
        });
        let wrapped = authored_collision(&[bldg2, Shape::Mopp, Shape::Mopp]);
        assert!(!wrapped.has_undecoded_mesh, "MOPP wrapping a decoded mesh is not undecoded geometry");
        assert!(wrapped.is_complete(), "mesh + its MOPP wrapper is a complete authored collider");
        assert_eq!((wrapped.n_mesh, wrapped.n_mopp), (1, 2));

        // A MOPP with NO decoded mesh IS an undecoded static mesh → keep the render fallback.
        let mopp_only = authored_collision(&[Shape::Mopp, Shape::Mopp]);
        assert!(mopp_only.has_undecoded_mesh, "a MOPP with no decoded WpMesh is an undecoded static mesh");
        assert!(!mopp_only.is_complete());

        let ragdoll = authored_collision(&[Shape::Sphere { radius: 0.2 }]);
        assert!(ragdoll.tris.is_empty(), "sphere/capsule colliders are not emitted as world tris");
        assert_eq!(ragdoll.n_sphere, 1);
        assert!(!ragdoll.is_complete());
    }
}
