//! A model's collision (`PHY2`), as the Depot's collision swap board shows it
//! (`docs/modernization/workshop_pages/depot.md`, board `ShipmentRawKind`).
//!
//! The shipped body is read the way `qm build` reads it for `replace_phy2`: the model's `_P000`
//! container from its LOD chain on the game stack (`GameStack::model_chain`), then the `PHY2` row inside it
//! (`phy2_container::phy2_span_in_container`). A body is decoded by `havok::parse_phy2_body`; what it
//! cannot decode is counted, not guessed.

use std::path::PathBuf;

use mercs2_formats::havok::{self, Shape};

/// What a `PHY2` body holds.
#[derive(Debug, Clone)]
pub struct Collision {
    /// The body's size in bytes.
    pub bytes: usize,
    /// The Havok packfile's version string and size.
    pub havok: String,
    pub packfile_bytes: usize,
    /// `WpMeshShape16` meshes, and how many of them decoded to triangles.
    pub meshes: usize,
    pub meshes_decoded: usize,
    pub tris: usize,
    /// `hkpConvexVerticesShape` hulls.
    pub hulls: usize,
    /// `hkpMoppBvTreeShape` BV-trees.
    pub mopp_trees: usize,
    pub boxes: usize,
    pub capsules: usize,
    pub spheres: usize,
    /// Shape classes recognised by name but not decoded, with how many.
    pub other: Vec<(String, usize)>,
    /// The size of the box holding every decoded vertex, metres (x, y, z).
    pub extent: Option<[f32; 3]>,
    pub shapes: Vec<Shape>,
}

/// Decode a `PHY2` body.
pub fn read(body: &[u8]) -> Result<Collision, String> {
    let pk = havok::parse_phy2_body(body)?;
    let mut c = Collision {
        bytes: body.len(),
        havok: pk.version.clone(),
        packfile_bytes: pk.size,
        meshes: 0,
        meshes_decoded: 0,
        tris: 0,
        hulls: 0,
        mopp_trees: pk.class_counts.get("hkpMoppBvTreeShape").copied().unwrap_or(0) as usize,
        boxes: 0,
        capsules: 0,
        spheres: 0,
        other: Vec::new(),
        extent: None,
        shapes: Vec::new(),
    };
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut grow = |v: &[f32; 3]| {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    };
    for s in &pk.shapes {
        match s {
            Shape::Mesh(m) => {
                c.meshes += 1;
                if !m.indices.is_empty() {
                    c.meshes_decoded += 1;
                }
                c.tris += m.indices.len();
                m.vertices.iter().for_each(&mut grow);
            }
            Shape::Convex(h) => {
                c.hulls += 1;
                h.vertices.iter().for_each(&mut grow);
            }
            Shape::Box { .. } => c.boxes += 1,
            Shape::Capsule(_) => c.capsules += 1,
            Shape::Sphere { .. } => c.spheres += 1,
            Shape::Mopp => {}
            Shape::Other(name) => match c.other.iter_mut().find(|(n, _)| n == name) {
                Some((_, k)) => *k += 1,
                None => c.other.push((name.clone(), 1)),
            },
        }
    }
    if lo[0] <= hi[0] {
        c.extent = Some([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]);
    }
    c.shapes = pk.shapes;
    Ok(c)
}

/// The shipped `PHY2` body of model `target` (a name or `0xHHHHHHHH`), from the game stack `wads`,
/// read from the `_P000` container the way `qm build`'s `replace_phy2` reads it
/// (`GameStack::model_chain`).
pub fn shipped_body(wads: &[PathBuf], target: &str) -> Result<Vec<u8>, String> {
    let hash = mercs2_quartermaster::manifest::asset_hash(target);
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let chain = game
        .model_chain(hash)
        .map_err(|e| format!("{target}: {e}"))?
        .ok_or_else(|| format!("{target} (0x{hash:08X}) is not a model in the game stack"))?;
    let container = &chain.rungs[0].container;
    let (at, len) = mercs2_formats::phy2_container::phy2_span_in_container(container)
        .ok_or_else(|| format!("the model {target} has no PHY2 chunk"))?;
    Ok(container[at..at + len].to_vec())
}

/// Every point a shape's outline is drawn through, model-local, by shape: a mesh as its triangles
/// (`Edges`), a convex shape as points whose projected convex hull is its silhouette (`Hull`).
pub enum Outline {
    Edges(Vec<[[f32; 3]; 2]>),
    Hull(Vec<[f32; 3]>),
}

pub fn outlines(shapes: &[Shape]) -> Vec<Outline> {
    let mut out = Vec::new();
    for s in shapes {
        match s {
            Shape::Mesh(m) if !m.indices.is_empty() => {
                let mut edges = Vec::with_capacity(m.indices.len() * 3);
                for t in &m.indices {
                    let p = |i: u16| m.vertices.get(i as usize).copied();
                    if let (Some(a), Some(b), Some(c)) = (p(t[0]), p(t[1]), p(t[2])) {
                        edges.push([a, b]);
                        edges.push([b, c]);
                        edges.push([c, a]);
                    }
                }
                out.push(Outline::Edges(edges));
            }
            Shape::Convex(h) if !h.vertices.is_empty() => out.push(Outline::Hull(h.vertices.clone())),
            Shape::Box { half_extents: e } => {
                let mut pts = Vec::with_capacity(8);
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        for sz in [-1.0, 1.0] {
                            pts.push([sx * e[0], sy * e[1], sz * e[2]]);
                        }
                    }
                }
                out.push(Outline::Hull(pts));
            }
            Shape::Sphere { radius } => out.push(Outline::Hull(ball([0.0; 3], *radius))),
            Shape::Capsule(c) => {
                let mut pts = ball(c.vertex_a, c.radius);
                pts.extend(ball(c.vertex_b, c.radius));
                out.push(Outline::Hull(pts));
            }
            _ => {}
        }
    }
    out
}

/// Points on a sphere, dense enough that their projected hull reads as its circle.
fn ball(c: [f32; 3], r: f32) -> Vec<[f32; 3]> {
    let mut pts = Vec::new();
    for i in 0..8 {
        let lat = std::f32::consts::PI * (i as f32 + 0.5) / 8.0 - std::f32::consts::FRAC_PI_2;
        for j in 0..16 {
            let lon = std::f32::consts::TAU * j as f32 / 16.0;
            pts.push([c[0] + r * lat.cos() * lon.cos(), c[1] + r * lat.sin(), c[2] + r * lat.cos() * lon.sin()]);
        }
    }
    pts
}

/// The convex hull of 2D points, counter-clockwise (Andrew's monotone chain).
pub fn hull_2d(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hull_of_a_square_and_its_centre_is_the_square() {
        let h = hull_2d(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5], [1.0, 0.0]]);
        assert_eq!(h, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    }

    #[test]
    fn a_body_with_no_packfile_is_refused() {
        assert!(read(b"PHY2 not havok").is_err());
    }

    #[test]
    fn a_built_phy2_reads_back_its_triangles() {
        let verts = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]];
        let tris = [[0, 1, 2], [0, 2, 3]];
        let body = mercs2_formats::phy2_build::build_phy2("depot_test", &tris, &verts).expect("builds");
        let c = read(&body).expect("reads");
        assert_eq!((c.meshes, c.meshes_decoded, c.tris), (1, 1, 2));
        let e = c.extent.expect("an extent");
        assert!((e[0] - 2.0).abs() < 0.01 && (e[1] - 3.0).abs() < 0.01 && (e[2] - 4.0).abs() < 0.01, "{e:?}");
        assert!(matches!(&outlines(&c.shapes)[0], Outline::Edges(e) if e.len() == 6));
    }

    /// The retail destroyer: the shipped body the board's "Shipped collision" card reads.
    #[cfg(feature = "retail")]
    #[test]
    fn the_shipped_destroyer_collision_reads() {
        let wad = PathBuf::from(crate::app::retail_vz_wad());
        let body = shipped_body(&[wad], "al_veh_boat_destroyer").expect("the destroyer has a PHY2");
        let c = read(&body).expect("it decodes");
        eprintln!(
            "destroyer PHY2: {} B, havok {} ({} B), {} meshes ({} decoded, {} tris), {} hulls, {} MOPP, extent {:?}, other {:?}",
            c.bytes, c.havok, c.packfile_bytes, c.meshes, c.meshes_decoded, c.tris, c.hulls, c.mopp_trees, c.extent, c.other
        );
        assert!(c.meshes + c.hulls > 0, "a destroyer collides with something");
        assert!(c.extent.is_some());
    }
}
