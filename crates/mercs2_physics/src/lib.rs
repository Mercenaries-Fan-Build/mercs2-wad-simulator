//! `mercs2_physics` — Havok character controller, rigid bodies, collision queries, MOPP/heightfield.
//!
//! **Silo 7** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 22.
//! **Code map:** `docs/reverse_engineer/physics_code_map.md`.
//! **Owned Lua namespace(s):** — (none; this silo has no Lua surface of its own — it backs the `PhysicsQuery` seam in `mercs2_core` for vehicle/combat/anim).
//! Implements `mercs2_core::PhysicsQuery` (raycast / getClosestPoints / hkpCharacterProxy move).
//!
//! # Wave-1 bridge — static triangle soup + heightmap
//!
//! [`StaticSoupPhysics`] is the **first concrete [`PhysicsQuery`] impl**: the cheap-but-real
//! collision bridge that lets the vehicle / combat / anim silos run against *live* geometry queries
//! before the full Havok world exists. It is backed by a static world-space triangle soup (buildings,
//! terrain mesh, roads) plus an optional [`Heightmap`] for low-res terrain — a faithful stand-in for
//! the retail engine's `hkpMoppBvTreeShape` + `hkpSampledHeightFieldShape` world.
//!
//! The three query methods map 1:1 onto the engine's shared query layer
//! (`physics_code_map.md` §3/§4), with the exe as the oracle for their semantics:
//!
//! | trait method                    | engine oracle                                                  | this impl |
//! |---------------------------------|----------------------------------------------------------------|-----------|
//! | [`PhysicsQuery::raycast`]       | `hkpWorldRayCaster` / Pangea `CastRay`                          | **exact math** — Möller–Trumbore ray/triangle, nearest hit + oriented normal |
//! | [`PhysicsQuery::closest_point`] | `LthkpWorld::getClosestPoints` `FUN_008db880` + closest collector | **exact math** — point/triangle (Ericson §5.1.5), signed by face normal |
//! | [`PhysicsQuery::move_character`]| `hkpCharacterProxy` swept capsule / `HumanPhysics::Activate` `FUN_004255c0` | **approximated** — collide-and-slide by depenetration + ground snap (see below) |
//!
//! ## Fidelity note (what the physics silo must replace for full Havok)
//!
//! * `move_character` here is **depenetration-based** (apply the desired delta, then push the capsule
//!   out of penetrated walls, then snap to the ground within `step`). This matches the retail
//!   collide-and-slide *behaviour* for per-frame moves smaller than the capsule radius, but is **not**
//!   the swept linear cast the retail `hkpCharacterProxy` / `HumanLinearCastJob` performs — a single
//!   delta larger than the radius can tunnel through thin geometry. Row 22 replaces this static soup
//!   with the real MOPP/heightfield world and the 5-state character controller
//!   (`HumanPhysics::Activate`), at which point the swept cast + slope/state machine come with it.
//! * There is no broadphase acceleration structure (MOPP BV-tree); this is a linear scan with a cheap
//!   sphere cull. Fine for Wave-1 sim silos; the Havok path brings the BV-tree.
//! * Only static world geometry is modelled, so [`RayHit::entity`] / `ClosestPoint::entity` are always
//!   `None` (per the trait doc — MOPP/heightfield report no owning entity). Dynamic rigid bodies
//!   (`hkpRigidBody`) and ragdolls arrive with the physics silo.

use mercs2_core::glam::Vec3;
use mercs2_core::physics_query::{ClosestPoint, PhysicsQuery, RayHit};

// ---------------------------------------------------------------------------
//   Triangle primitives (ported from mercs2_game::collision — leaf crate must
//   not depend on mercs2_game, so the math lives here directly).
// ---------------------------------------------------------------------------

/// Ray/triangle intersection (Möller–Trumbore). Returns hit distance `t ≥ 0` along `dir`, or `None`.
/// Double-sided (the retail `CastRay` hits back-faces too).
fn ray_tri(o: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-7 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = o - a;
    let u = tvec.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = tvec.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4).then_some(t)
}

/// Geometric (un-normalised) normal of triangle `abc`, following its winding.
fn tri_normal(t: &[Vec3; 3]) -> Vec3 {
    (t[1] - t[0]).cross(t[2] - t[0])
}

/// A triangle is a WALL if its normal is more horizontal than vertical (steep surface). Walls block +
/// slide; walkable surfaces (floors/ramps) are left to the ground probe.
fn is_wall(t: &[Vec3; 3]) -> bool {
    let n = tri_normal(t);
    let nl = n.length();
    nl > 1e-6 && (n.y / nl).abs() < 0.5
}

/// Closest point on triangle `abc` to `p` (Ericson, *Real-Time Collision Detection* §5.1.5).
fn closest_on_tri(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Closest points between segments `[p1,q1]` and `[p2,q2]` (Ericson §5.1.9).
fn closest_seg_seg(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    const EPS: f32 = 1e-8;
    let (s, t);
    if a <= EPS && e <= EPS {
        return (p1, p2);
    }
    if a <= EPS {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let s0 = if denom.abs() > EPS { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            } else {
                t = t0;
                s = s0;
            }
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

/// Closest points between segment `[a,b]` and triangle `t0 t1 t2` (segment point, triangle point).
fn seg_tri_closest(a: Vec3, b: Vec3, t0: Vec3, t1: Vec3, t2: Vec3) -> (Vec3, Vec3) {
    // If the segment crosses the triangle's face, the distance is zero there.
    let n = (t1 - t0).cross(t2 - t0);
    let denom = n.dot(b - a);
    if denom.abs() > 1e-8 {
        let s = n.dot(t0 - a) / denom;
        if (0.0..=1.0).contains(&s) {
            let hit = a + (b - a) * s;
            if (hit - closest_on_tri(hit, t0, t1, t2)).length_squared() < 1e-6 {
                return (hit, hit);
            }
        }
    }
    // Otherwise the closest pair is on the boundary: segment vs each edge, and each endpoint vs face.
    let mut best = (a, closest_on_tri(a, t0, t1, t2));
    let mut best_d = (best.0 - best.1).length_squared();
    let consider = |sp: Vec3, tp: Vec3, best: &mut (Vec3, Vec3), best_d: &mut f32| {
        let d = (sp - tp).length_squared();
        if d < *best_d {
            *best_d = d;
            *best = (sp, tp);
        }
    };
    let qb = closest_on_tri(b, t0, t1, t2);
    consider(b, qb, &mut best, &mut best_d);
    for (e0, e1) in [(t0, t1), (t1, t2), (t2, t0)] {
        let (sp, tp) = closest_seg_seg(a, b, e0, e1);
        consider(sp, tp, &mut best, &mut best_d);
    }
    best
}

// ---------------------------------------------------------------------------
//   Heightmap — low-res terrain stand-in for hkpSampledHeightFieldShape
// ---------------------------------------------------------------------------

/// A regular-grid terrain heightfield: the Wave-1 stand-in for `hkpSampledHeightFieldShape`.
///
/// Heights are stored row-major (`heights[z * width + x]`) over a grid whose cell `(0,0)` sits at
/// world `(origin.x, _, origin.z)` with `cell` spacing on both axes. [`Heightmap::sample`] does
/// bilinear interpolation, so the character controller's ground snap follows terrain smoothly.
#[derive(Clone, Debug, PartialEq)]
pub struct Heightmap {
    /// World-space (x, z) of grid cell `(0, 0)`.
    pub origin_x: f32,
    pub origin_z: f32,
    /// Grid spacing on both axes (world units per cell).
    pub cell: f32,
    /// Number of samples along +X.
    pub width: usize,
    /// Number of samples along +Z.
    pub depth: usize,
    /// Row-major heights, length `width * depth`.
    pub heights: Vec<f32>,
}

impl Heightmap {
    /// Build a heightmap from a row-major height grid. `heights.len()` must equal `width * depth`.
    pub fn new(origin_x: f32, origin_z: f32, cell: f32, width: usize, depth: usize, heights: Vec<f32>) -> Self {
        assert_eq!(heights.len(), width * depth, "heightmap heights.len() must equal width*depth");
        assert!(cell > 0.0, "heightmap cell spacing must be positive");
        Self { origin_x, origin_z, cell, width, depth, heights }
    }

    #[inline]
    fn at(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz * self.width + ix]
    }

    /// Bilinearly-interpolated terrain height at world `(x, z)`, or `None` outside the grid.
    pub fn sample(&self, x: f32, z: f32) -> Option<f32> {
        if self.width < 2 || self.depth < 2 {
            return None;
        }
        let fx = (x - self.origin_x) / self.cell;
        let fz = (z - self.origin_z) / self.cell;
        if fx < 0.0 || fz < 0.0 {
            return None;
        }
        let ix = fx.floor() as usize;
        let iz = fz.floor() as usize;
        if ix >= self.width - 1 || iz >= self.depth - 1 {
            return None;
        }
        let tx = fx - ix as f32;
        let tz = fz - iz as f32;
        let h00 = self.at(ix, iz);
        let h10 = self.at(ix + 1, iz);
        let h01 = self.at(ix, iz + 1);
        let h11 = self.at(ix + 1, iz + 1);
        let a = h00 + (h10 - h00) * tx;
        let b = h01 + (h11 - h01) * tx;
        Some(a + (b - a) * tz)
    }
}

// ---------------------------------------------------------------------------
//   StaticSoupPhysics — the concrete PhysicsQuery impl
// ---------------------------------------------------------------------------

/// A [`PhysicsQuery`] backed by a static world-space triangle soup plus an optional terrain
/// [`Heightmap`]. This is the Wave-1 collision bridge (see the crate docs): real ray/closest/character
/// queries against baked geometry, standing in for the retail Havok MOPP + heightfield world until the
/// physics silo (row 22) swaps in the full `hkpWorld`.
///
/// Construct one with [`StaticSoupPhysics::new`] (soup only), [`StaticSoupPhysics::from_heightmap`]
/// (terrain only), or [`StaticSoupPhysics::with_heightmap`] (both), then hand a `&dyn PhysicsQuery`
/// to the vehicle / combat / anim silos.
#[derive(Clone, Debug, Default)]
pub struct StaticSoupPhysics {
    tris: Vec<[Vec3; 3]>,
    heightmap: Option<Heightmap>,
}

impl StaticSoupPhysics {
    /// Build from a world-space triangle list (buildings/roads/terrain mesh). No terrain heightmap.
    pub fn new(tris: Vec<[Vec3; 3]>) -> Self {
        Self { tris, heightmap: None }
    }

    /// Build from a terrain heightmap only (no triangle geometry).
    pub fn from_heightmap(heightmap: Heightmap) -> Self {
        Self { tris: Vec::new(), heightmap: Some(heightmap) }
    }

    /// Build from both a triangle soup and a terrain heightmap.
    pub fn with_heightmap(tris: Vec<[Vec3; 3]>, heightmap: Heightmap) -> Self {
        Self { tris, heightmap: Some(heightmap) }
    }

    /// The triangle soup this impl queries.
    pub fn tris(&self) -> &[[Vec3; 3]] {
        &self.tris
    }

    /// The terrain heightmap, if any.
    pub fn heightmap(&self) -> Option<&Heightmap> {
        self.heightmap.as_ref()
    }

    /// Replace the triangle soup (e.g. after a world-streaming block loads/unloads).
    pub fn set_tris(&mut self, tris: Vec<[Vec3; 3]>) {
        self.tris = tris;
    }

    /// Attach or replace the terrain heightmap.
    pub fn set_heightmap(&mut self, heightmap: Option<Heightmap>) {
        self.heightmap = heightmap;
    }

    // --- character-controller internals (ported from mercs2_game::collision) ---

    /// Push the capsule (feet `pos`, `radius`, `height`) out of every WALL triangle it penetrates.
    /// Pushing perpendicular to each contact preserves tangential motion → the capsule slides along
    /// walls. A few relaxation passes resolve inside corners. Floors are excluded (ground snap owns Y).
    fn depenetrate(&self, mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
        let cull2 = (radius + height + 4.0) * (radius + height + 4.0);
        for _ in 0..4 {
            let mut moved = false;
            for t in &self.tris {
                if (t[0] - pos).length_squared() > cull2 || !is_wall(t) {
                    continue;
                }
                let a = pos + Vec3::Y * radius;
                let b = pos + Vec3::Y * (height - radius);
                let (sp, tp) = seg_tri_closest(a, b, t[0], t[1], t[2]);
                let d = sp - tp;
                let dist = d.length();
                if dist < radius {
                    if dist > 1e-4 {
                        pos += d / dist * (radius - dist);
                    } else {
                        let n = tri_normal(t);
                        if n.length() > 1e-6 {
                            pos += n.normalize() * radius;
                        }
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        pos
    }

    /// Highest walkable surface (soup floor OR heightmap) under `pos` within `[pos.y - step, pos.y +
    /// step]`. This is what makes the feet follow stairs/ramps/terrain and climb low ledges (step-up)
    /// without a height hack. Returns `None` when nothing walkable is within `step` (edge/gap → fall).
    fn ground_y(&self, pos: Vec3, radius: f32, step: f32) -> Option<f32> {
        let origin = pos + Vec3::Y * step;
        let max_t = step * 2.0;
        let cull2 = (radius + 2.0) * (radius + 2.0);
        let mut best: Option<f32> = None;
        for t in &self.tris {
            if is_wall(t) {
                continue;
            }
            let horiz = ((t[0] - pos) * Vec3::new(1.0, 0.0, 1.0)).length_squared();
            if horiz > cull2 {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        // Terrain heightmap is another walkable candidate under the feet.
        if let Some(hm) = &self.heightmap {
            if let Some(hy) = hm.sample(pos.x, pos.z) {
                if hy >= pos.y - step && hy <= pos.y + step && best.map_or(true, |b| hy > b) {
                    best = Some(hy);
                }
            }
        }
        best
    }
}

impl PhysicsQuery for StaticSoupPhysics {
    /// `hkpWorldRayCaster` / `CastRay`: nearest triangle hit along `[origin, origin + dir*max]`.
    /// `dir` is treated as a unit direction; `max` is the cast length. Normal is oriented to oppose
    /// the ray (front-facing to the caster). `entity` is `None` — static world geometry.
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        let cull2 = (max + 30.0) * (max + 30.0);
        let mut best: Option<(f32, &[Vec3; 3])> = None;
        for t in &self.tris {
            if (t[0] - origin).length_squared() > cull2 {
                continue;
            }
            if let Some(d) = ray_tri(origin, dir, t[0], t[1], t[2]) {
                if d <= max && best.map_or(true, |(b, _)| d < b) {
                    best = Some((d, t));
                }
            }
        }
        best.map(|(d, t)| {
            let mut n = tri_normal(t);
            let nl = n.length();
            n = if nl > 1e-6 { n / nl } else { -dir };
            // Face the normal against the ray (the surface the caster sees).
            if n.dot(dir) > 0.0 {
                n = -n;
            }
            RayHit { point: origin + dir * d, normal: n, distance: d, entity: None }
        })
    }

    /// `LthkpWorld::getClosestPoints`: nearest point on world geometry within `max` of `point`.
    /// `distance` is signed — negative when `point` is behind the nearest face (penetrating). Sign is
    /// taken from the triangle's wound normal, so it flips inside↔outside for consistently-wound
    /// (outward-facing) closed shells. `entity` is `None` — static world geometry.
    fn closest_point(&self, point: Vec3, max: f32) -> Option<ClosestPoint> {
        let cull2 = (max + 30.0) * (max + 30.0);
        let mut best: Option<(f32, Vec3, &[Vec3; 3])> = None; // (unsigned dist², cp, tri)
        for t in &self.tris {
            if (t[0] - point).length_squared() > cull2 {
                continue;
            }
            let cp = closest_on_tri(point, t[0], t[1], t[2]);
            let d2 = (point - cp).length_squared();
            if best.map_or(true, |(b, _, _)| d2 < b) {
                best = Some((d2, cp, t));
            }
        }
        best.and_then(|(d2, cp, t)| {
            let dist = d2.sqrt();
            if dist > max {
                return None;
            }
            let mut n = tri_normal(t);
            let nl = n.length();
            n = if nl > 1e-6 { n / nl } else { Vec3::Y };
            // Signed separation: negative when the query point is behind the wound face.
            let signed = if (point - cp).dot(n) < 0.0 { -dist } else { dist };
            Some(ClosestPoint { point: cp, normal: n, distance: signed, entity: None })
        })
    }

    /// `hkpCharacterProxy` swept-capsule move (`HumanPhysics::Activate`): apply desired `delta`,
    /// depenetrate the capsule out of walls (collide-and-slide), then snap the feet to the walkable
    /// surface underneath within `step` (step-up / ground follow). Returns the resolved feet position.
    ///
    /// APPROXIMATION: depenetration-based, not a swept linear cast — faithful for per-frame moves
    /// smaller than `radius` (see crate-level fidelity note).
    fn move_character(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32, step: f32) -> Vec3 {
        // Apply the desired displacement, then push out of walls (perpendicular push-out is the slide).
        let mut p = pos + delta;
        p = self.depenetrate(p, radius, height);
        // Snap the feet to the ground within `step` (climbs low ledges, follows ramps/terrain).
        if let Some(gy) = self.ground_y(p, radius, step) {
            p.y = gy;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A double triangle forming an axis-aligned quad on the XZ plane at height `y`, spanning
    // [x0,x1] × [z0,z1], wound so the normal points +Y (walkable floor).
    fn floor_quad(y: f32, x0: f32, x1: f32, z0: f32, z1: f32) -> [[Vec3; 3]; 2] {
        let a = Vec3::new(x0, y, z0);
        let b = Vec3::new(x1, y, z0);
        let c = Vec3::new(x1, y, z1);
        let d = Vec3::new(x0, y, z1);
        // CCW seen from above → +Y normal.
        [[a, c, b], [a, d, c]]
    }

    // A vertical wall quad in the plane x = `xw`, spanning [z0,z1] × [y0,y1]; normal points -X.
    fn wall_quad(xw: f32, z0: f32, z1: f32, y0: f32, y1: f32) -> [[Vec3; 3]; 2] {
        let a = Vec3::new(xw, y0, z0);
        let b = Vec3::new(xw, y0, z1);
        let c = Vec3::new(xw, y1, z1);
        let d = Vec3::new(xw, y1, z0);
        [[a, b, c], [a, c, d]]
    }

    #[test]
    fn ray_hits_known_tri_at_right_distance_and_normal() {
        // Floor at y = 0; cast straight down from 5 units up.
        let tris: Vec<_> = floor_quad(0.0, -10.0, 10.0, -10.0, 10.0).into();
        let phys = StaticSoupPhysics::new(tris);
        let hit = phys
            .raycast(Vec3::new(1.0, 5.0, 2.0), -Vec3::Y, 100.0)
            .expect("ray should hit the floor");
        assert!((hit.distance - 5.0).abs() < 1e-3, "distance {}", hit.distance);
        assert!((hit.point - Vec3::new(1.0, 0.0, 2.0)).length() < 1e-3, "point {:?}", hit.point);
        // Normal faces the ray (upward, opposing the downward cast).
        assert!(hit.normal.dot(Vec3::Y) > 0.99, "normal {:?}", hit.normal);
        assert!(hit.entity.is_none());
    }

    #[test]
    fn ray_miss_returns_none() {
        let tris: Vec<_> = floor_quad(0.0, -1.0, 1.0, -1.0, 1.0).into();
        let phys = StaticSoupPhysics::new(tris);
        // Cast upward, away from the floor → miss.
        assert!(phys.raycast(Vec3::new(0.0, 5.0, 0.0), Vec3::Y, 100.0).is_none());
        // Cast horizontally past the small quad → miss.
        assert!(phys.raycast(Vec3::new(50.0, 5.0, 0.0), Vec3::X, 100.0).is_none());
    }

    #[test]
    fn ray_max_range_is_respected() {
        let tris: Vec<_> = floor_quad(0.0, -10.0, 10.0, -10.0, 10.0).into();
        let phys = StaticSoupPhysics::new(tris);
        // Floor is 5 down, but max is only 3 → no hit.
        assert!(phys.raycast(Vec3::new(0.0, 5.0, 0.0), -Vec3::Y, 3.0).is_none());
    }

    #[test]
    fn move_character_slides_along_wall_instead_of_penetrating() {
        // Wall at x = 1 (normal -X). Feet just touching at x = 0.7 (radius 0.3), then push into it
        // diagonally by a small delta (< radius) with tangential (+Z) motion.
        let wall: Vec<_> = wall_quad(1.0, -5.0, 5.0, 0.0, 3.0).into();
        let phys = StaticSoupPhysics::new(wall);
        let radius = 0.3;
        let start = Vec3::new(0.7, 0.0, 0.0);
        let end = phys.move_character(start, Vec3::new(0.2, 0.0, 0.5), radius, 1.8, 0.4);
        // Did not penetrate the wall: capsule core stays >= radius away from x = 1.
        assert!(end.x <= 1.0 - radius + 1e-3, "penetrated: x = {}", end.x);
        // Tangential motion preserved: slid along +Z.
        assert!(end.z > 0.45, "did not slide: z = {}", end.z);
    }

    #[test]
    fn move_character_steps_up_low_ledge() {
        // Low floor y=0 for x<1, raised floor y=0.4 for x>=1 (a 0.4 ledge, step limit 0.5). Tiles are
        // kept small (near the path) to match the game's small-triangle soup that the ground-probe cull
        // is tuned for.
        let mut tris: Vec<[Vec3; 3]> = Vec::new();
        tris.extend(floor_quad(0.0, -1.0, 1.0, -1.0, 1.0));
        tris.extend(floor_quad(0.4, 1.0, 3.0, -1.0, 1.0));
        let phys = StaticSoupPhysics::new(tris);
        // Start on the low floor, walk +X across the ledge boundary.
        let start = Vec3::new(0.5, 0.0, 0.0);
        let end = phys.move_character(start, Vec3::new(1.0, 0.0, 0.0), 0.3, 1.8, 0.5);
        assert!(end.x > 1.0, "did not advance onto the ledge: x = {}", end.x);
        assert!((end.y - 0.4).abs() < 1e-3, "did not climb the ledge: y = {}", end.y);
    }

    #[test]
    fn move_character_does_not_climb_ledge_above_step_limit() {
        // Low floor under the whole path plus a 0.8-tall ledge slab on top for x>=1. With a 0.5 step
        // limit the ledge is unreachable, so ground snap keeps the feet on the low floor (y=0).
        let mut tris: Vec<[Vec3; 3]> = Vec::new();
        tris.extend(floor_quad(0.0, 1.0, 3.0, -1.0, 1.0));
        tris.extend(floor_quad(0.8, 1.0, 3.0, -1.0, 1.0));
        let phys = StaticSoupPhysics::new(tris);
        let start = Vec3::new(0.5, 0.0, 0.0);
        let end = phys.move_character(start, Vec3::new(1.0, 0.0, 0.0), 0.3, 1.8, 0.5);
        // Ground snap finds the low floor (y=0), not the too-high ledge.
        assert!((end.y - 0.0).abs() < 1e-3, "should not climb 0.8 with 0.5 step: y = {}", end.y);
    }

    #[test]
    fn closest_point_sign_flips_inside_vs_outside() {
        // Build a closed axis-aligned box [-1,1]³ with outward-facing normals.
        let phys = StaticSoupPhysics::new(unit_box());
        // A point well outside → positive separation.
        let outside = phys
            .closest_point(Vec3::new(3.0, 0.0, 0.0), 10.0)
            .expect("box within range");
        assert!(outside.distance > 0.0, "outside should be positive: {}", outside.distance);
        assert!((outside.distance - 2.0).abs() < 1e-3, "dist to +X face: {}", outside.distance);
        // A point at the centre → penetrating → negative separation.
        let inside = phys
            .closest_point(Vec3::ZERO, 10.0)
            .expect("nearest face within range");
        assert!(inside.distance < 0.0, "inside should be negative: {}", inside.distance);
    }

    #[test]
    fn closest_point_out_of_range_returns_none() {
        let phys = StaticSoupPhysics::new(unit_box());
        assert!(phys.closest_point(Vec3::new(100.0, 0.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn heightmap_ground_snap_follows_terrain() {
        // Flat terrain sloping in X: h(x) = x. Character should snap onto it.
        // 3x3 grid, cell 1, origin (0,0): heights = x index.
        let heights = vec![
            0.0, 1.0, 2.0, // z=0
            0.0, 1.0, 2.0, // z=1
            0.0, 1.0, 2.0, // z=2
        ];
        let hm = Heightmap::new(0.0, 0.0, 1.0, 3, 3, heights);
        assert!((hm.sample(1.5, 1.0).unwrap() - 1.5).abs() < 1e-4);
        let phys = StaticSoupPhysics::from_heightmap(hm);
        // Start near terrain at x=1 (h=1), move to x=1.5 (h=1.5); step tolerance covers the rise.
        let end = phys.move_character(Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.5, 0.0, 0.0), 0.3, 1.8, 1.0);
        assert!((end.y - 1.5).abs() < 1e-3, "did not follow terrain: y = {}", end.y);
    }

    // Unit box [-1,1]³, six quads. Winding is forced so every triangle normal points OUTWARD (away
    // from the box centre at the origin), so the closest_point sign test is unambiguous.
    fn unit_box() -> Vec<[Vec3; 3]> {
        let mut t: Vec<[Vec3; 3]> = Vec::new();
        let mut quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            for mut tri in [[a, b, c], [a, c, d]] {
                let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
                // Force outward: flip winding if the normal points back toward the box centre.
                if tri_normal(&tri).dot(centroid) < 0.0 {
                    tri.swap(1, 2);
                }
                t.push(tri);
            }
        };
        let (p, n) = (1.0f32, -1.0f32);
        quad(Vec3::new(p, n, n), Vec3::new(p, n, p), Vec3::new(p, p, p), Vec3::new(p, p, n)); // +X
        quad(Vec3::new(n, n, p), Vec3::new(n, n, n), Vec3::new(n, p, n), Vec3::new(n, p, p)); // -X
        quad(Vec3::new(n, p, n), Vec3::new(p, p, n), Vec3::new(p, p, p), Vec3::new(n, p, p)); // +Y
        quad(Vec3::new(n, n, p), Vec3::new(p, n, p), Vec3::new(p, n, n), Vec3::new(n, n, n)); // -Y
        quad(Vec3::new(p, n, p), Vec3::new(n, n, p), Vec3::new(n, p, p), Vec3::new(p, p, p)); // +Z
        quad(Vec3::new(n, n, n), Vec3::new(p, n, n), Vec3::new(p, p, n), Vec3::new(n, p, n)); // -Z
        t
    }
}
