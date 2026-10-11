//! Procedural humanoid "mannequin" generator: skeleton rest positions ->
//! clean low-poly primitive body, auto-weighted to the owning bone.
//!
//! REUSABLE DRIVER PIECE (separate from any cesium/foreign-import code). Given a
//! [`crate::skeleton::Skeleton`] and a [`BodyMap`] of resolved body-part bone
//! indices, [`build_mannequin`] emits an [`ExternalMesh`] (positions baked to the
//! donor frame: Y-up, feet at Y=0, scaled to a target height) ready to feed
//! straight into `model_inject::inject_multi_into_donor_block`.
//!
//! Geometry is deliberately low-poly and fully owned so the mesh isolates
//! pipeline bugs from foreign-import quirks. Auto-weighting baseline is RIGID:
//! every vertex of a primitive is weighted 1.0 to that primitive's bone (global
//! 95-bone index, no palette/remap — mattias_v2 uses direct global BLENDINDICES).
//! Limb segment primitives optionally do a NEAREST-2 distance blend along their
//! axis so elbows/knees bend smoothly.

use crate::model_inject::ExternalMesh;
use crate::skeleton::Skeleton;

/// Resolved body-part bone indices (into the donor's global 95-bone array).
/// Names are by anatomical role; resolved from the rainbow-table bone names +
/// hierarchy/position by the driver.
#[derive(Debug, Clone, Copy)]
pub struct BodyMap {
    pub pelvis: usize,
    pub chest: usize,
    pub neck: usize,
    pub head: usize,
    pub clav_l: usize,
    pub clav_r: usize,
    pub upperarm_l: usize,
    pub upperarm_r: usize,
    pub forearm_l: usize,
    pub forearm_r: usize,
    pub hand_l: usize,
    pub hand_r: usize,
    pub thigh_l: usize,
    pub thigh_r: usize,
    pub shin_l: usize,
    pub shin_r: usize,
    pub foot_l: usize,
    pub foot_r: usize,
}

/// 3-component vector helpers.
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len3(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn norm3(a: [f32; 3]) -> [f32; 3] {
    let l = len3(a).max(1e-8);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// A growable mesh accumulator with per-vertex skin (single global bone or a
/// nearest-2 blend). Tris index into the shared vertex array.
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tris: Vec<[u32; 3]>,
    joints: Vec<[u8; 4]>,
    weights: Vec<[u8; 4]>,
}

impl MeshBuilder {
    fn new() -> Self {
        MeshBuilder {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tris: Vec::new(),
            joints: Vec::new(),
            weights: Vec::new(),
        }
    }

    fn push_vert(&mut self, p: [f32; 3], n: [f32; 3], uv: [f32; 2], skin: ([u8; 4], [u8; 4])) -> u32 {
        let id = self.positions.len() as u32;
        self.positions.push(p);
        self.normals.push(norm3(n));
        self.uvs.push(uv);
        self.joints.push(skin.0);
        self.weights.push(skin.1);
        id
    }

    fn vcount(&self) -> usize {
        self.positions.len()
    }
}

/// Rigid skin: weight 1.0 (0xFF) to a single global bone index.
fn rigid(bone: usize) -> ([u8; 4], [u8; 4]) {
    ([bone as u8, 0, 0, 0], [0xff, 0, 0, 0])
}

/// Nearest-2 blend along a segment: `t` in [0,1] from bone `a` (t=0) to bone `b`
/// (t=1). Weight goes to whichever bone is nearer, blended near the joint so the
/// limb bends. Indices are global; weights are u8x4n summing to 255.
fn blend2(a: usize, b: usize, t: f32) -> ([u8; 4], [u8; 4]) {
    let t = t.clamp(0.0, 1.0);
    let wb = (t * 255.0).round() as i32;
    let wb = wb.clamp(0, 255) as u32;
    let wa = 255 - wb;
    (
        [a as u8, b as u8, 0, 0],
        [wa as u8, wb as u8, 0, 0],
    )
}

/// Append a capped cylinder (segment) from `p0` (bone `b0`) to `p1` (bone `b1`),
/// radius `r`, `seg` radial segments, `rings` length rings. Skin blends from b0
/// at the p0 end to b1 at the p1 end (nearest-2). If `blend` is false the whole
/// segment is rigid to `b0`.
fn add_segment(
    m: &mut MeshBuilder,
    p0: [f32; 3],
    p1: [f32; 3],
    r0: f32,
    r1: f32,
    b0: usize,
    b1: usize,
    seg: usize,
    rings: usize,
    blend: bool,
    cap0: bool,
    cap1: bool,
) {
    let axis = sub(p1, p0);
    let axlen = len3(axis).max(1e-6);
    let dir = scale(axis, 1.0 / axlen);
    // build an orthonormal frame (u,v) perpendicular to dir
    let up = if dir[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = norm3(cross(up, dir));
    let v = cross(dir, u);

    let ring_base: Vec<u32> = Vec::new();
    let mut rings_idx: Vec<Vec<u32>> = Vec::with_capacity(rings + 1);
    let _ = ring_base;
    for ri in 0..=rings {
        let t = ri as f32 / rings as f32;
        let center = add(p0, scale(axis, t));
        let r = r0 + (r1 - r0) * t;
        let skin = if blend { blend2(b0, b1, t) } else { rigid(b0) };
        let mut row = Vec::with_capacity(seg + 1);
        for si in 0..=seg {
            let a = (si as f32 / seg as f32) * std::f32::consts::TAU;
            let radial = add(scale(u, a.cos()), scale(v, a.sin()));
            let p = add(center, scale(radial, r));
            let uv = [si as f32 / seg as f32, t];
            row.push(m.push_vert(p, radial, uv, skin));
        }
        rings_idx.push(row);
    }
    for ri in 0..rings {
        for si in 0..seg {
            let a = rings_idx[ri][si];
            let b = rings_idx[ri][si + 1];
            let c = rings_idx[ri + 1][si];
            let d = rings_idx[ri + 1][si + 1];
            m.tris.push([a, b, c]);
            m.tris.push([b, d, c]);
        }
    }
    // end caps (fans) so the body is watertight
    if cap0 {
        let skin = if blend { blend2(b0, b1, 0.0) } else { rigid(b0) };
        let center = m.push_vert(p0, scale(dir, -1.0), [0.5, 0.0], skin);
        for si in 0..seg {
            let a = rings_idx[0][si];
            let b = rings_idx[0][si + 1];
            m.tris.push([center, b, a]);
        }
    }
    if cap1 {
        let skin = if blend { blend2(b0, b1, 1.0) } else { rigid(b1) };
        let center = m.push_vert(p1, dir, [0.5, 1.0], skin);
        for si in 0..seg {
            let a = rings_idx[rings][si];
            let b = rings_idx[rings][si + 1];
            m.tris.push([center, a, b]);
        }
    }
}

/// Append a UV sphere of radius `r` at `center`, rigid to `bone`.
fn add_sphere(m: &mut MeshBuilder, center: [f32; 3], r: f32, bone: usize, stacks: usize, slices: usize) {
    let skin = rigid(bone);
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(stacks + 1);
    for st in 0..=stacks {
        let phi = (st as f32 / stacks as f32) * std::f32::consts::PI; // 0..PI
        let y = phi.cos();
        let rr = phi.sin();
        let mut row = Vec::with_capacity(slices + 1);
        for sl in 0..=slices {
            let theta = (sl as f32 / slices as f32) * std::f32::consts::TAU;
            let n = [rr * theta.cos(), y, rr * theta.sin()];
            let p = add(center, scale(n, r));
            let uv = [sl as f32 / slices as f32, st as f32 / stacks as f32];
            row.push(m.push_vert(p, n, uv, skin));
        }
        grid.push(row);
    }
    for st in 0..stacks {
        for sl in 0..slices {
            let a = grid[st][sl];
            let b = grid[st][sl + 1];
            let c = grid[st + 1][sl];
            let d = grid[st + 1][sl + 1];
            m.tris.push([a, b, c]);
            m.tris.push([b, d, c]);
        }
    }
}

/// Append an axis-aligned box centred at `center` with half-extents `he`, rigid
/// to `bone`. 24 verts (flat-shaded faces) / 12 tris.
fn add_box(m: &mut MeshBuilder, center: [f32; 3], he: [f32; 3], bone: usize) {
    let skin = rigid(bone);
    // 6 faces, each: normal + 4 corners (CCW)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]]),
        ([0.0, 0.0, -1.0], [[1.0, -1.0, -1.0], [-1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, -1.0]]),
        ([1.0, 0.0, 0.0], [[1.0, -1.0, 1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]]),
        ([-1.0, 0.0, 0.0], [[-1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, 1.0], [-1.0, 1.0, -1.0]]),
        ([0.0, 1.0, 0.0], [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]]),
        ([0.0, -1.0, 0.0], [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]]),
    ];
    for (n, corners) in faces.iter() {
        let uvc = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let mut ids = [0u32; 4];
        for (k, c) in corners.iter().enumerate() {
            let p = [
                center[0] + c[0] * he[0],
                center[1] + c[1] * he[1],
                center[2] + c[2] * he[2],
            ];
            ids[k] = m.push_vert(p, *n, uvc[k], skin);
        }
        m.tris.push([ids[0], ids[1], ids[2]]);
        m.tris.push([ids[0], ids[2], ids[3]]);
    }
}

/// Per-part vertex counts for reporting.
#[derive(Debug, Clone, Default)]
pub struct PartCounts {
    pub parts: Vec<(String, usize, usize)>, // (name, vcount, tricount)
}

/// Build the full procedural mannequin in the donor frame. `target_height` is the
/// donor bind height (~1.847); positions are scaled+translated so feet sit at
/// Y=0 and the model is `target_height` tall. Returns the mesh plus per-part
/// counts. `blend_limbs` enables nearest-2 weight blends on the four limb chains.
pub fn build_mannequin(
    skel: &Skeleton,
    map: &BodyMap,
    target_height: f32,
    blend_limbs: bool,
) -> (ExternalMesh, PartCounts) {
    let pos = |i: usize| skel.bones[i].world_pos();
    let mut m = MeshBuilder::new();
    let mut counts = PartCounts::default();
    let mark = |m: &MeshBuilder, c: &mut PartCounts, name: &str, v0: usize, t0: usize| {
        c.parts
            .push((name.to_string(), m.vcount() - v0, m.tris.len() - t0));
    };

    // --- torso: tapered cylinder pelvis -> chest, rigid (chest); plus a short
    //     pelvis cap segment so the hips are covered. Keep low-poly.
    let (v0, t0) = (m.vcount(), m.tris.len());
    let pelvis = pos(map.pelvis);
    let chest = pos(map.chest);
    add_segment(&mut m, pelvis, chest, 0.16, 0.15, map.pelvis, map.chest, 10, 4, blend_limbs, true, false);
    mark(&m, &mut counts, "torso", v0, t0);

    // --- neck + head ---
    let (v0, t0) = (m.vcount(), m.tris.len());
    let neck = pos(map.neck);
    let head = pos(map.head);
    add_segment(&mut m, chest, neck, 0.15, 0.05, map.chest, map.neck, 8, 2, blend_limbs, false, false);
    add_segment(&mut m, neck, head, 0.05, 0.05, map.neck, map.head, 8, 1, blend_limbs, false, false);
    mark(&m, &mut counts, "neck", v0, t0);

    let (v0, t0) = (m.vcount(), m.tris.len());
    // head sphere a bit above the head bone (skull centre)
    let skull = add(head, [0.0, 0.09, 0.0]);
    add_sphere(&mut m, skull, 0.10, map.head, 8, 8);
    mark(&m, &mut counts, "head", v0, t0);

    // --- arms (per side) ---
    for (side, clav, ua, fa, hand) in [
        ("L", map.clav_l, map.upperarm_l, map.forearm_l, map.hand_l),
        ("R", map.clav_r, map.upperarm_r, map.forearm_r, map.hand_r),
    ] {
        let (v0, t0) = (m.vcount(), m.tris.len());
        let pua = pos(ua);
        let pfa = pos(fa);
        let phand = pos(hand);
        let _ = clav;
        // upper arm: shoulder(upperarm bone) -> elbow(forearm bone)
        add_segment(&mut m, pua, pfa, 0.055, 0.045, ua, fa, 8, 3, blend_limbs, true, false);
        // forearm: elbow -> wrist(hand bone)
        add_segment(&mut m, pfa, phand, 0.045, 0.035, fa, hand, 8, 3, blend_limbs, false, false);
        mark(&m, &mut counts, &format!("arm_{side}"), v0, t0);
        // hand box at wrist
        let (v0, t0) = (m.vcount(), m.tris.len());
        add_box(&mut m, phand, [0.04, 0.025, 0.06], hand);
        mark(&m, &mut counts, &format!("hand_{side}"), v0, t0);
    }

    // --- legs (per side) ---
    for (side, thigh, shin, foot) in [
        ("L", map.thigh_l, map.shin_l, map.foot_l),
        ("R", map.thigh_r, map.shin_r, map.foot_r),
    ] {
        let (v0, t0) = (m.vcount(), m.tris.len());
        let pthigh = pos(thigh);
        let pshin = pos(shin);
        let pfoot = pos(foot);
        // thigh: hip -> knee
        add_segment(&mut m, pthigh, pshin, 0.08, 0.06, thigh, shin, 8, 3, blend_limbs, true, false);
        // shin: knee -> ankle
        add_segment(&mut m, pshin, pfoot, 0.06, 0.045, shin, foot, 8, 3, blend_limbs, false, true);
        mark(&m, &mut counts, &format!("leg_{side}"), v0, t0);
        // foot box: forward (+Z a bit) and below ankle
        let (v0, t0) = (m.vcount(), m.tris.len());
        let foot_c = add(pfoot, [0.0, -0.03, 0.06]);
        add_box(&mut m, foot_c, [0.045, 0.03, 0.10], foot);
        mark(&m, &mut counts, &format!("foot_{side}"), v0, t0);
    }

    // --- bake to donor frame: feet to Y=0, scale to target height ---
    let (mut ymin, mut ymax) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &m.positions {
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let height = (ymax - ymin).max(1e-6);
    let s = target_height / height;
    for p in m.positions.iter_mut() {
        p[0] *= s;
        p[1] = (p[1] - ymin) * s;
        p[2] *= s;
    }
    // normals are direction-only (uniform scale preserves direction; renormalise)
    for n in m.normals.iter_mut() {
        *n = norm3(*n);
    }

    let mesh = ExternalMesh {
        positions: m.positions,
        normals: m.normals,
        uvs: m.uvs,
        tris: m.tris,
        joints: m.joints,
        weights: m.weights,
    };
    (mesh, counts)
}
