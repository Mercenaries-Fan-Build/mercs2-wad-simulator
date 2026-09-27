//! **Ground cover** (`scrub`, UCFX `type_hash` `0x600B904E`, ASET `type_id` 12): a typed decoder/encoder
//! for the instanced grass/rock packages a c3 cell block carries next to its terrain cell, and the edit
//! that keeps them on the ground when the cell is displaced ([`ScrubPack::follow_ground`]).
//!
//! # Container layout
//!
//! 1,020 of the 1,026 retail containers have exactly this tree (bodies packed in row order, `CSUM`
//! trailer); the other 6 are empty (no rows at all) and round-trip as [`ScrubPack::Empty`]:
//!
//! ```text
//! INFO  16 B  {u32 mesh_count, u32 patch_count, u32 range_count, u32 instance_count}
//! SCRB × mesh_count     one instanced mesh
//!   INFO  20 B          not decoded
//!   MTRL                one material record (terrainmesh `MTRL` record layout)
//!   STRM → info · decl · data     vertex stream (`POSITION` is FLOAT16_4 at offset 0)
//!   IBUF → info · data            u16 indices
//! INST  instance_count × 24 B     3 × 4 f16 row-major affine matrix, patch-local
//! PTCH  patch_count × 56 B        {vec3 origin, vec3 sphere_centre, f32 radius, vec3 aabb_min,
//!                                  vec3 aabb_max, u16 first_range, u16 range_count}
//! PTMS  range_count × 8 B         {u32 first_instance, u16 instance_count, u16 mesh}
//! ```
//!
//! The four `INFO` counts and the three record sizes are the retail loader's own:
//! `FUN_004a4c40` (Ghidra decomp of the unpacked `Mercenaries2.exe`) reads the 16-byte `INFO` as the
//! `SCRB`/`PTCH`/`PTMS`/`INST` counts and allocates `count × 0x1d0` / `× 0x38` / `× 8` / `× 0x18`.
//! `FUN_004a53d0` copies `PTCH +0x0c..+0x1c` (sphere) and `+0x1c..+0x34` (box) into the patch object,
//! `FUN_004a5670` walks a patch's `PTMS` ranges from `PTCH +0x34`/`+0x36` and converts each range's
//! `INST` rows (3 × 4 halves) to floats, and `FUN_004a5830` resolves `PTMS +6` as the `SCRB` index.
//!
//! # Placement, frame and bounds
//!
//! A container is placed by a `ScrubObject` entity (`{key, scrub hash}` joined to `Transform`,
//! [`crate::placement::load_scrub_placements`]). Every one of the 1,043 retail `ScrubObject`
//! placements sits at exactly a terrain tile's `TerrainObject` position with no rotation, so an instance
//! sits at `origin + translation` in that terrain cell's **cell-local** frame — measured over all
//! 1,767,683 retail instances: median height above that cell's render surface 0.000 m, 98.7 % within
//! 25 cm, 19 a few millimetres past the cell edge (`tests/scrub_retail.rs`). The container is NOT
//! always in its cell's block (the `vz_state_mar_city_ruined` ground cover lives in blocks with no
//! terrain cell), so pair a container with its cell through its placement, not its block. A patch's box is the union of its instances' transformed mesh vertices
//! (patch-local; within 1.6 cm of retail, which was computed before the matrices were quantized to f16),
//! and its sphere is the box's centre and half-diagonal (2,028/2,028 patches sampled; the same rule as a
//! terrain `GEOM`).

use crate::model_inject::{f16_le, read_f16_le};
use crate::terrainmesh::{
    children_of, decode_material, decode_stream, emit_tree, encode_material, exact_len,
    expect_children, f32_at, leaf, parse_tree, put_vec3, sphere_of, stream_nodes, u16_at, u32_at,
    vec3_at, Aabb, DeclElement, Material, Node, TerrainCell,
};

/// UCFX `type_hash` of a ground-cover package (`scrub`).
pub const TYPE_HASH: u32 = 0x600B_904E;

const INFO_LEN: usize = 16;
const MESH_INFO_LEN: usize = 20;
const INST_LEN: usize = 24;
const PTCH_LEN: usize = 56;
const PTMS_LEN: usize = 8;
const DECLTYPE_FLOAT16_4: u8 = 16;
const USAGE_POSITION: u8 = 0;

/// One decoded ground-cover container.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrubPack {
    /// A container with no descriptor rows (6 retail entries).
    Empty,
    Pack(Scrub),
}

/// The populated form.
#[derive(Debug, Clone, PartialEq)]
pub struct Scrub {
    pub meshes: Vec<ScrubMesh>,
    pub instances: Vec<Instance>,
    pub patches: Vec<Patch>,
    pub ranges: Vec<Range>,
}

/// One `SCRB`: an instanced mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrubMesh {
    /// The 20-byte `INFO`, not decoded (two hash-like words, then `30.0` and `50.0` in the samples read).
    pub info: [u32; 5],
    pub material: Material,
    pub stride: u32,
    pub decl: Vec<DeclElement>,
    pub vertices: Vec<u8>,
    pub indices: Vec<u16>,
}

/// One `INST` record: a 3 × 4 row-major affine matrix stored as IEEE half floats (kept as their bits so
/// they round-trip exactly). Column 3 is the translation from the owning patch's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instance {
    pub rows: [[u16; 4]; 3],
}

/// One `PTCH` record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    /// Cell-local origin every instance of the patch is relative to.
    pub origin: [f32; 3],
    /// Bounding sphere, patch-local.
    pub sphere_centre: [f32; 3],
    pub sphere_radius: f32,
    /// Bounding box, patch-local.
    pub aabb: Aabb,
    pub first_range: u16,
    pub range_count: u16,
}

/// One `PTMS` record: a run of instances of one mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub first_instance: u32,
    pub instance_count: u16,
    /// Index into [`Scrub::meshes`].
    pub mesh: u16,
}

impl Instance {
    /// The matrix as floats.
    pub fn matrix(&self) -> [[f32; 4]; 3] {
        self.rows
            .map(|r| r.map(|h| read_f16_le(&h.to_le_bytes(), 0)))
    }

    /// The translation (patch-local).
    pub fn translation(&self) -> [f32; 3] {
        let m = self.matrix();
        [m[0][3], m[1][3], m[2][3]]
    }
}

impl ScrubMesh {
    /// Mesh-local `POSITION` of every vertex.
    pub fn positions(&self) -> Result<Vec<[f32; 3]>, String> {
        let pos: Vec<&DeclElement> = self
            .decl
            .iter()
            .filter(|e| e.usage == USAGE_POSITION && e.usage_index == 0)
            .collect();
        let off = match pos.as_slice() {
            [e] if e.ty == DECLTYPE_FLOAT16_4 && e.stream == 0 => e.offset as usize,
            _ => return Err("mesh decl has no single FLOAT16_4 POSITION on stream 0".into()),
        };
        let s = self.stride as usize;
        Ok((0..self.vertices.len() / s)
            .map(|i| {
                let o = i * s + off;
                [
                    read_f16_le(&self.vertices, o),
                    read_f16_le(&self.vertices, o + 2),
                    read_f16_le(&self.vertices, o + 4),
                ]
            })
            .collect())
    }
}

impl ScrubPack {
    /// Decode a ground-cover container (`CSUM` included).
    pub fn decode(container: &[u8]) -> Result<ScrubPack, String> {
        let roots = parse_tree(container)?;
        if roots.is_empty() {
            return Ok(ScrubPack::Empty);
        }
        let info = leaf(&roots[0], "root")?;
        if roots[0].tag() != *b"INFO" {
            return Err("scrub root[0] is not INFO".into());
        }
        exact_len(info, INFO_LEN, "scrub INFO")?;
        let [mesh_count, patch_count, range_count, instance_count] =
            [0, 4, 8, 12].map(|o| u32_at(info, o) as usize);
        let mut want: Vec<&[u8; 4]> = vec![b"INFO"];
        want.extend(std::iter::repeat_n(b"SCRB", mesh_count));
        want.extend([b"INST", b"PTCH", b"PTMS"]);
        expect_children(&roots, &want, "scrub root")?;

        let mut meshes = Vec::with_capacity(mesh_count);
        for (m, node) in roots[1..1 + mesh_count].iter().enumerate() {
            let ctx = format!("SCRB[{m}]");
            let kids = children_of(node, &ctx)?;
            expect_children(kids, &[b"INFO", b"MTRL", b"STRM", b"IBUF"], &ctx)?;
            let minfo = leaf(&kids[0], &ctx)?;
            exact_len(minfo, MESH_INFO_LEN, &format!("{ctx} INFO"))?;
            let mtrl = leaf(&kids[1], &ctx)?;
            let (material, end) = decode_material(mtrl, 0, &format!("{ctx} MTRL"))?;
            if end != mtrl.len() {
                return Err(format!(
                    "{ctx}: MTRL is {} bytes, its one record {end}",
                    mtrl.len()
                ));
            }
            let (stride, decl, vertices, indices) = decode_stream(&kids[2], &kids[3], &ctx)?;
            let mesh = ScrubMesh {
                info: [0, 4, 8, 12, 16].map(|o| u32_at(minfo, o)),
                material,
                stride,
                decl,
                vertices,
                indices,
            };
            mesh.positions().map_err(|e| format!("{ctx}: {e}"))?;
            meshes.push(mesh);
        }

        let body = |k: usize, len: usize, count: usize, tag: &str| -> Result<&[u8], String> {
            let b = leaf(&roots[1 + mesh_count + k], tag)?;
            exact_len(b, len * count, tag)?;
            Ok(b)
        };
        let inst = body(0, INST_LEN, instance_count, "INST")?;
        let ptch = body(1, PTCH_LEN, patch_count, "PTCH")?;
        let ptms = body(2, PTMS_LEN, range_count, "PTMS")?;
        let instances = (0..instance_count)
            .map(|i| Instance {
                rows: [0, 1, 2]
                    .map(|r| [0, 1, 2, 3].map(|c| u16_at(inst, i * INST_LEN + 8 * r + 2 * c))),
            })
            .collect();
        let patches = (0..patch_count)
            .map(|i| {
                let o = i * PTCH_LEN;
                Patch {
                    origin: vec3_at(ptch, o),
                    sphere_centre: vec3_at(ptch, o + 12),
                    sphere_radius: f32_at(ptch, o + 24),
                    aabb: Aabb {
                        min: vec3_at(ptch, o + 28),
                        max: vec3_at(ptch, o + 40),
                    },
                    first_range: u16_at(ptch, o + 52),
                    range_count: u16_at(ptch, o + 54),
                }
            })
            .collect();
        let ranges = (0..range_count)
            .map(|i| {
                let o = i * PTMS_LEN;
                Range {
                    first_instance: u32_at(ptms, o),
                    instance_count: u16_at(ptms, o + 4),
                    mesh: u16_at(ptms, o + 6),
                }
            })
            .collect();
        let scrub = Scrub {
            meshes,
            instances,
            patches,
            ranges,
        };
        scrub.validate()?;
        Ok(ScrubPack::Pack(scrub))
    }

    /// Encode back to a container. A decoded retail container re-encodes byte-for-byte.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let ScrubPack::Pack(s) = self else {
            return Ok(emit_tree(&[]));
        };
        s.validate()?;
        let mut roots = Vec::with_capacity(s.meshes.len() + 4);
        let mut info = Vec::with_capacity(INFO_LEN);
        for n in [
            s.meshes.len(),
            s.patches.len(),
            s.ranges.len(),
            s.instances.len(),
        ] {
            info.extend_from_slice(&(n as u32).to_le_bytes());
        }
        roots.push(Node::Leaf(*b"INFO", info));
        for m in &s.meshes {
            let mut minfo = Vec::with_capacity(MESH_INFO_LEN);
            for w in m.info {
                minfo.extend_from_slice(&w.to_le_bytes());
            }
            let mut mtrl = Vec::new();
            encode_material(&m.material, &mut mtrl)?;
            let [strm, ibuf] = stream_nodes(m.stride, &m.decl, &m.vertices, &m.indices);
            roots.push(Node::Container(
                *b"SCRB",
                vec![
                    Node::Leaf(*b"INFO", minfo),
                    Node::Leaf(*b"MTRL", mtrl),
                    strm,
                    ibuf,
                ],
            ));
        }
        let mut inst = Vec::with_capacity(INST_LEN * s.instances.len());
        for i in &s.instances {
            for r in i.rows {
                for h in r {
                    inst.extend_from_slice(&h.to_le_bytes());
                }
            }
        }
        let mut ptch = Vec::with_capacity(PTCH_LEN * s.patches.len());
        for p in &s.patches {
            put_vec3(&mut ptch, p.origin);
            put_vec3(&mut ptch, p.sphere_centre);
            ptch.extend_from_slice(&p.sphere_radius.to_le_bytes());
            put_vec3(&mut ptch, p.aabb.min);
            put_vec3(&mut ptch, p.aabb.max);
            ptch.extend_from_slice(&p.first_range.to_le_bytes());
            ptch.extend_from_slice(&p.range_count.to_le_bytes());
        }
        let mut ptms = Vec::with_capacity(PTMS_LEN * s.ranges.len());
        for r in &s.ranges {
            ptms.extend_from_slice(&r.first_instance.to_le_bytes());
            ptms.extend_from_slice(&r.instance_count.to_le_bytes());
            ptms.extend_from_slice(&r.mesh.to_le_bytes());
        }
        roots.push(Node::Leaf(*b"INST", inst));
        roots.push(Node::Leaf(*b"PTCH", ptch));
        roots.push(Node::Leaf(*b"PTMS", ptms));
        Ok(emit_tree(&roots))
    }
}

impl Scrub {
    /// Every patch names ranges that exist, and every range names instances and a mesh that exist.
    fn validate(&self) -> Result<(), String> {
        for (i, p) in self.patches.iter().enumerate() {
            let end = p.first_range as usize + p.range_count as usize;
            if end > self.ranges.len() {
                return Err(format!(
                    "PTCH[{i}] names ranges up to {end}; PTMS has {}",
                    self.ranges.len()
                ));
            }
        }
        for (i, r) in self.ranges.iter().enumerate() {
            let end = r.first_instance as usize + r.instance_count as usize;
            if end > self.instances.len() {
                return Err(format!(
                    "PTMS[{i}] names instances up to {end}; INST has {}",
                    self.instances.len()
                ));
            }
            if r.mesh as usize >= self.meshes.len() {
                return Err(format!(
                    "PTMS[{i}] names mesh {}; there are {}",
                    r.mesh,
                    self.meshes.len()
                ));
            }
        }
        Ok(())
    }

    /// Cell-local position of every instance of patch `p`, as `(instance index, position)`.
    pub fn instance_positions(&self, p: usize) -> Vec<(usize, [f32; 3])> {
        let patch = &self.patches[p];
        let mut out = Vec::new();
        for r in &self.ranges
            [patch.first_range as usize..(patch.first_range + patch.range_count) as usize]
        {
            for k in
                r.first_instance as usize..r.first_instance as usize + r.instance_count as usize
            {
                let t = self.instances[k].translation();
                out.push((k, [0, 1, 2].map(|a| patch.origin[a] + t[a])));
            }
        }
        out
    }

    /// The patch's box as the union of its instances' transformed mesh vertices (patch-local).
    pub fn patch_bounds(&self, p: usize) -> Result<Aabb, String> {
        let patch = &self.patches[p];
        let mut b = Aabb {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
        for r in &self.ranges
            [patch.first_range as usize..(patch.first_range + patch.range_count) as usize]
        {
            let verts = self.meshes[r.mesh as usize].positions()?;
            for k in
                r.first_instance as usize..r.first_instance as usize + r.instance_count as usize
            {
                let m = self.instances[k].matrix();
                for v in &verts {
                    for (a, row) in m.iter().enumerate() {
                        let w = row[0] * v[0] + row[1] * v[1] + row[2] * v[2] + row[3];
                        b.min[a] = b.min[a].min(w);
                        b.max[a] = b.max[a].max(w);
                    }
                }
            }
        }
        if !b.min.iter().chain(&b.max).all(|v| v.is_finite()) {
            return Err(format!("PTCH[{p}] has no instances to bound"));
        }
        Ok(b)
    }
}

/// What [`ScrubPack::follow_ground`] changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FollowGround {
    /// Instances whose stored height changed.
    pub moved_instances: usize,
    /// Patches whose box and sphere were recomputed.
    pub rebounded_patches: usize,
    /// Instances outside the cell's footprint, left as they are.
    pub outside_footprint: usize,
}

/// Is cell-local `pos` outside the XZ footprint `([min_x, min_z], [max_x, max_z])`?
fn outside((lo, hi): ([f32; 2], [f32; 2]), pos: [f32; 3]) -> bool {
    pos[0] < lo[0] || pos[0] > hi[0] || pos[2] < lo[1] || pos[2] > hi[1]
}

/// A cell's render triangles bucketed on a 4 m XZ grid, each with its three vertices' `(g, p, i)`.
struct Surface {
    buckets: std::collections::HashMap<(i32, i32), Vec<usize>>,
    tris: Vec<[(usize, usize, usize); 3]>,
}

const BUCKET_M: f32 = 4.0;

impl Surface {
    fn of(cell: &TerrainCell) -> Result<Surface, String> {
        let mut s = Surface {
            buckets: Default::default(),
            tris: Vec::new(),
        };
        for (g, geom) in cell.geoms.iter().enumerate() {
            for (p, prmg) in geom.prmgs.iter().enumerate() {
                for t in prmg.triangles()? {
                    let refs = t.map(|v| (g, p, v as usize));
                    let pts = refs.map(|(g, p, i)| cell.cell_position(g, p, i));
                    let id = s.tris.len();
                    s.tris.push(refs);
                    let lo = |a: usize| pts.iter().map(|q| q[a]).fold(f32::INFINITY, f32::min);
                    let hi = |a: usize| pts.iter().map(|q| q[a]).fold(f32::NEG_INFINITY, f32::max);
                    for bx in (lo(0) / BUCKET_M).floor() as i32..=(hi(0) / BUCKET_M).floor() as i32
                    {
                        for bz in
                            (lo(2) / BUCKET_M).floor() as i32..=(hi(2) / BUCKET_M).floor() as i32
                        {
                            s.buckets.entry((bx, bz)).or_default().push(id);
                        }
                    }
                }
            }
        }
        Ok(s)
    }

    /// Triangles whose XZ projection contains `(x, z)`, with the barycentric weights of the point.
    fn under(&self, cell: &TerrainCell, x: f32, z: f32) -> Vec<(usize, [f64; 3])> {
        let key = ((x / BUCKET_M).floor() as i32, (z / BUCKET_M).floor() as i32);
        let mut out = Vec::new();
        for &id in self.buckets.get(&key).into_iter().flatten() {
            let [a, b, c] =
                self.tris[id].map(|(g, p, i)| cell.cell_position(g, p, i).map(|v| v as f64));
            let (x, z) = (x as f64, z as f64);
            let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
            if d == 0.0 {
                continue;
            }
            let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
            let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 >= -1e-6 && l2 >= -1e-6 && l3 >= -1e-6 {
                out.push((id, [l1, l2, l3]));
            }
        }
        out
    }

    /// The triangle an instance at cell-local `pos` stands on: of the triangles under its XZ, the one
    /// whose surface height there is nearest its height. Fails when no triangle is under it.
    fn stood_on(&self, cell: &TerrainCell, pos: [f32; 3]) -> Result<(usize, [f64; 3]), String> {
        let under = self.under(cell, pos[0], pos[2]);
        under
            .iter()
            .min_by(|a, b| {
                let da = (self.height(cell, a.0, a.1) - pos[1] as f64).abs();
                let db = (self.height(cell, b.0, b.1) - pos[1] as f64).abs();
                da.total_cmp(&db)
            })
            .copied()
            .ok_or_else(|| {
                format!(
                    "at cell-local ({}, {}, {}) has no ground under it",
                    pos[0], pos[1], pos[2]
                )
            })
    }

    fn height(&self, cell: &TerrainCell, id: usize, w: [f64; 3]) -> f64 {
        let ys = self.tris[id].map(|(g, p, i)| cell.cell_position(g, p, i)[1] as f64);
        w[0] * ys[0] + w[1] * ys[1] + w[2] * ys[2]
    }
}

impl ScrubPack {
    /// For every instance (in patch order): its height above the render surface of `cell` it stands on
    /// (negative = sunk into it), or `None` for an instance outside the cell's footprint (it stands on a
    /// neighbouring cell). Measures which cell's frame a container is in, and how its instances were
    /// authored against the ground. Fails when an instance inside the footprint has no triangle under it.
    pub fn ground_offsets(&self, cell: &TerrainCell) -> Result<Vec<Option<f64>>, String> {
        let ScrubPack::Pack(s) = self else {
            return Ok(Vec::new());
        };
        let surface = Surface::of(cell)?;
        let footprint = cell.edge_extent()?;
        let mut out = Vec::with_capacity(s.instances.len());
        for p in 0..s.patches.len() {
            for (k, pos) in s.instance_positions(p) {
                if outside(footprint, pos) {
                    out.push(None);
                    continue;
                }
                let (id, w) = surface
                    .stood_on(cell, pos)
                    .map_err(|e| format!("instance {k} (patch {p}) {e}"))?;
                out.push(Some(pos[1] as f64 - surface.height(cell, id, w)));
            }
        }
        Ok(out)
    }

    /// Keep every instance on the ground after `before` was edited into `after` (the same cell — the
    /// one this container's `ScrubObject` placement sits on — with only vertex heights changed).
    ///
    /// An instance outside the cell's footprint (retail places a few up to a few millimetres past the
    /// edge) stands on a neighbouring cell's ground; [`TerrainCell::displace`] never moves the shared
    /// edge, so that ground is unchanged and the instance is left as it is (counted in
    /// [`FollowGround::outside_footprint`]).
    ///
    /// For each instance, the render triangles of `before` under its cell-local XZ are found, and the one
    /// whose surface height there is nearest the instance's height is the one it stands on. The
    /// instance's translation `y` moves by that triangle's height change at the same barycentric point, so
    /// an instance authored half-buried stays half-buried. Patches with a moved instance get their box and
    /// sphere recomputed ([`Scrub::patch_bounds`], [`sphere_of`]).
    ///
    /// Fails — leaving `self` unchanged — when an instance inside the footprint has no render triangle
    /// under it, when `before`
    /// and `after` differ in anything but vertex positions' heights, or when a new height does not fit an
    /// f16.
    pub fn follow_ground(
        &mut self,
        before: &TerrainCell,
        after: &TerrainCell,
    ) -> Result<FollowGround, String> {
        let ScrubPack::Pack(s) = self else {
            return Ok(FollowGround::default());
        };
        same_topology(before, after)?;
        let surface = Surface::of(before)?;
        let footprint = before.edge_extent()?;
        let mut next = s.clone();
        let mut report = FollowGround::default();
        for p in 0..next.patches.len() {
            let mut moved = false;
            for (k, pos) in next.instance_positions(p) {
                if outside(footprint, pos) {
                    report.outside_footprint += 1;
                    continue;
                }
                let (id, w) = surface
                    .stood_on(before, pos)
                    .map_err(|e| format!("instance {k} (patch {p}) {e}"))?;
                let delta = surface.height(after, id, w) - surface.height(before, id, w);
                if delta == 0.0 {
                    continue;
                }
                let ty = next.instances[k].translation()[1] as f64 + delta;
                if !ty.is_finite() || ty.abs() > 65504.0 {
                    return Err(format!(
                        "instance {k} would sit at local y = {ty}, outside f16"
                    ));
                }
                let bits = u16::from_le_bytes(f16_le(ty as f32));
                if bits != next.instances[k].rows[1][3] {
                    next.instances[k].rows[1][3] = bits;
                    report.moved_instances += 1;
                    moved = true;
                }
            }
            if moved {
                let aabb = next.patch_bounds(p)?;
                let (centre, radius) = sphere_of(&aabb);
                let patch = &mut next.patches[p];
                patch.aabb = aabb;
                patch.sphere_centre = centre;
                patch.sphere_radius = radius;
                report.rebounded_patches += 1;
            }
        }
        *s = next;
        Ok(report)
    }
}

/// `after` must be `before` with only vertex heights changed: same patches, groups, decls, strides,
/// indices, draws and vertex counts, and the same x/z everywhere.
fn same_topology(before: &TerrainCell, after: &TerrainCell) -> Result<(), String> {
    if before.geoms.len() != after.geoms.len() {
        return Err("the edited cell has a different patch count".into());
    }
    for (g, (a, b)) in before.geoms.iter().zip(&after.geoms).enumerate() {
        if a.poff != b.poff || a.prmgs.len() != b.prmgs.len() {
            return Err(format!("GEOM[{g}] differs in offset or draw groups"));
        }
        for (p, (pa, pb)) in a.prmgs.iter().zip(&b.prmgs).enumerate() {
            if pa.decl != pb.decl
                || pa.stride != pb.stride
                || pa.indices != pb.indices
                || pa.draws != pb.draws
                || pa.alt_draws != pb.alt_draws
                || pa.vertex_count() != pb.vertex_count()
            {
                return Err(format!(
                    "GEOM[{g}] PRMG[{p}] differs in more than vertex data"
                ));
            }
            for i in 0..pa.vertex_count() {
                let (va, vb) = (pa.position(i), pb.position(i));
                if va[0] != vb[0] || va[2] != vb[2] {
                    return Err(format!("GEOM[{g}] PRMG[{p}] vertex {i} moved in x or z"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn half(v: f32) -> u16 {
        u16::from_le_bytes(f16_le(v))
    }

    /// A one-mesh pack over the synthetic terrain: a 1 m quad mesh, one patch of 3 instances at
    /// cell-local (10, h, 10), (12, h, 10), (10, h - 0.5, 14).
    fn pack(ground_y: impl Fn(f32, f32) -> f32) -> Scrub {
        let el = |offset: u16, ty: u8, usage: u8| DeclElement {
            stream: 0,
            offset,
            ty,
            method: 0,
            usage,
            usage_index: 0,
        };
        let decl = vec![el(0, 16, 0), el(8, 16, 5), el(16, 16, 3)];
        let mut vertices = Vec::new();
        for (x, y, z) in [
            (-0.5, 0.0, -0.5),
            (0.5, 0.0, -0.5),
            (0.5, 1.0, 0.5),
            (-0.5, 1.0, 0.5),
        ] {
            for c in [x, y, z, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0] {
                vertices.extend_from_slice(&f16_le(c));
            }
        }
        let origin = [10.0, ground_y(10.0, 10.0), 10.0];
        let place = |x: f32, z: f32, sink: f32| Instance {
            rows: [
                [half(1.0), 0, 0, half(x - origin[0])],
                [0, half(1.0), 0, half(ground_y(x, z) - sink - origin[1])],
                [0, 0, half(1.0), half(z - origin[2])],
            ],
        };
        let mut s = Scrub {
            meshes: vec![ScrubMesh {
                info: [1, 2, 0x41F0_0000, 0x4248_0000, 0],
                material: Material {
                    word_0: 7,
                    params: [1.0; 25],
                    flags: 0x98,
                    textures: vec![0xCD2C_E651],
                    tail: [1, 2],
                },
                stride: 24,
                decl,
                vertices,
                indices: vec![0, 1, 3, 2],
            }],
            instances: vec![
                place(10.0, 10.0, 0.0),
                place(12.0, 10.0, 0.0),
                place(10.0, 14.0, 0.5),
            ],
            patches: vec![Patch {
                origin,
                sphere_centre: [0.0; 3],
                sphere_radius: 0.0,
                aabb: Aabb {
                    min: [0.0; 3],
                    max: [0.0; 3],
                },
                first_range: 0,
                range_count: 1,
            }],
            ranges: vec![Range {
                first_instance: 0,
                instance_count: 3,
                mesh: 0,
            }],
        };
        let b = s.patch_bounds(0).unwrap();
        let (c, r) = sphere_of(&b);
        s.patches[0].aabb = b;
        s.patches[0].sphere_centre = c;
        s.patches[0].sphere_radius = r;
        s
    }

    #[test]
    fn a_pack_and_an_empty_container_round_trip() {
        let p = ScrubPack::Pack(pack(|_, _| 3.0));
        let bytes = p.encode().unwrap();
        assert_eq!(ScrubPack::decode(&bytes).unwrap(), p);
        let empty = ScrubPack::Empty.encode().unwrap();
        assert_eq!(empty.len(), 28);
        assert_eq!(ScrubPack::decode(&empty).unwrap(), ScrubPack::Empty);
    }

    #[test]
    fn a_range_past_the_instances_is_refused() {
        let mut s = pack(|_, _| 3.0);
        s.ranges[0].instance_count = 4;
        assert!(ScrubPack::Pack(s).encode().unwrap_err().contains("PTMS[0]"));
    }

    /// Instances ride the edited ground by the surface's own height change, keep their authored sink, and
    /// the patch bounds follow.
    #[test]
    fn instances_follow_the_displaced_ground() {
        let flat = crate::terrainmesh::tests::synthetic_cell_with(|_, _| 2.0);
        let mut raised = flat.clone();
        raised
            .displace(|x, z| {
                if x.abs() < 60.0 && z.abs() < 60.0 {
                    4.0
                } else {
                    0.0
                }
            })
            .unwrap();
        let mut p = ScrubPack::Pack(pack(|_, _| 2.0));
        let r = p.follow_ground(&flat, &raised).unwrap();
        assert_eq!(
            r,
            FollowGround {
                moved_instances: 3,
                rebounded_patches: 1,
                outside_footprint: 0
            }
        );
        let ScrubPack::Pack(s) = &p else {
            unreachable!()
        };
        let ys: Vec<f32> = s
            .instance_positions(0)
            .into_iter()
            .map(|(_, q)| q[1])
            .collect();
        assert_eq!(ys, vec![6.0, 6.0, 5.5]);
        assert_eq!(s.patches[0].aabb, s.patch_bounds(0).unwrap());
        // Round-trips after the edit.
        assert_eq!(ScrubPack::decode(&p.encode().unwrap()).unwrap(), p);
    }

    /// Past the footprint is a neighbour's ground: left alone and counted.
    #[test]
    fn an_instance_past_the_cell_edge_is_left_alone() {
        let flat = crate::terrainmesh::tests::synthetic_cell_with(|_, _| 2.0);
        let mut raised = flat.clone();
        raised
            .displace(|x, z| {
                if x.abs() < 60.0 && z.abs() < 60.0 {
                    4.0
                } else {
                    0.0
                }
            })
            .unwrap();
        let mut s = pack(|_, _| 2.0);
        s.instances[2].rows[0][3] = half(900.0);
        let mut p = ScrubPack::Pack(s);
        let r = p.follow_ground(&flat, &raised).unwrap();
        assert_eq!((r.moved_instances, r.outside_footprint), (2, 1));
    }

    /// Inside the footprint with nothing under it (a hole in the ground) is an error.
    #[test]
    fn an_instance_over_a_hole_is_an_error() {
        let mut holed = crate::terrainmesh::tests::synthetic_cell_with(|_, _| 2.0);
        // Drop every draw of the patch at (+50, +50): its 100 m square renders nothing.
        for prmg in &mut holed.geoms[3].prmgs {
            for d in prmg.draws.iter_mut().chain(prmg.alt_draws.iter_mut()) {
                d.prim_count = 0;
            }
        }
        let mut s = pack(|_, _| 2.0);
        s.instances[2].rows[0][3] = half(50.0);
        s.instances[2].rows[2][3] = half(50.0);
        let mut p = ScrubPack::Pack(s);
        let before = p.clone();
        let err = p.follow_ground(&holed, &holed).unwrap_err();
        assert!(err.contains("no ground under it"), "{err}");
        assert_eq!(p, before);
    }
}
