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
//! | [`PhysicsQuery::move_character`]| `hkpCharacterProxy` swept capsule / `HumanPhysics::Activate` `FUN_004255c0` | **swept linear cast** — conservative-advancement collide-and-slide + ground snap (tunnel-free) |
//!
//! The full on-foot controller — the recovered OnGround/InAir/Jumping state machine
//! (`hkpCharacterContext FUN_0094d2e0`) with gravity, jump, slope + step limits — lives in
//! [`CharacterController`]; minimal prop/debris dynamics live in [`RigidBody`] +
//! [`StaticSoupPhysics::step_rigid_body`].
//!
//! ## Fidelity note (what the physics silo must replace for full Havok)
//!
//! * `move_character` is now a **swept linear cast** ([`StaticSoupPhysics::move_swept`] via
//!   [conservative advancement][`StaticSoupPhysics::linear_cast`]): it never steps further than the
//!   current wall clearance, so the capsule cannot tunnel a thin wall regardless of `|delta|` (the
//!   tunnel-free upgrade over W1-C's depenetration). This matches the retail `hkpCharacterProxy` /
//!   `HumanLinearCastJob` sweep *behaviour*; row 22 swaps the static soup for the real
//!   MOPP/heightfield world + the full 5-state controller (adding the Climbing/Ladder game states).
//!   `// CONFIRM-LIVE:` the per-frame integrator (`hkpWorld::step`) is VMX128-undecoded — the
//!   semi-implicit Euler here is a faithful equivalent, not the exe's exact solver.
//! * There is no broadphase acceleration structure (MOPP BV-tree); this is a linear scan with a cheap
//!   sphere cull. Fine for Wave-1 sim silos; the Havok path brings the BV-tree.
//! * Only static world geometry is modelled, so [`RayHit::entity`] / `ClosestPoint::entity` are always
//!   `None` (per the trait doc — MOPP/heightfield report no owning entity). Dynamic rigid bodies
//!   (`hkpRigidBody`) and ragdolls arrive with the physics silo.

use mercs2_core::glam::Vec3;
use mercs2_core::physics_query::{ClosestPoint, PhysicsQuery, RayHit};

/// Lightweight direct-triangle-soup collision (capsule controller + camera raycast over `&[[Vec3;3]]`),
/// folded from the game's on-foot collision. Bbox-culled (large-triangle-safe). See [`soup`].
pub mod soup;

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

/// A walkable-ground query result: the surface height under the feet and its (upward-oriented) normal.
/// Mirrors what the retail floor-probe feeds `hkpCharacterStateOnGround` (ground height + slope normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    /// World-space Y of the walkable surface under the query point.
    pub y: f32,
    /// Upward-oriented unit surface normal (used for the slope limit).
    pub normal: Vec3,
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

    /// Highest walkable surface (soup floor OR heightmap) under `pos`, searched in the vertical band
    /// `[pos.y - down, pos.y + up]`. A surface is *walkable* only when its normal is within `min_cos`
    /// of vertical — the slope limit, matching `hkpCharacterStateOnGround`'s `maxSlopeCosine`
    /// (`FUN_0094ce90`, cinfo `+0xa4`). This makes the feet follow stairs/ramps/terrain and climb low
    /// ledges (step-up). Returns `None` when nothing walkable is in range (edge/gap → fall).
    fn ground_probe(&self, pos: Vec3, radius: f32, up: f32, down: f32, min_cos: f32) -> Option<GroundHit> {
        let origin = pos + Vec3::Y * up;
        let max_t = up + down;
        let cull2 = (radius + 2.0) * (radius + 2.0);
        let mut best: Option<GroundHit> = None;
        for t in &self.tris {
            let n = tri_normal(t);
            let nl = n.length();
            if nl <= 1e-6 {
                continue;
            }
            let ny = n.y / nl;
            if ny.abs() < min_cos {
                continue; // too steep to stand on (slope limit)
            }
            let horiz = ((t[0] - pos) * Vec3::new(1.0, 0.0, 1.0)).length_squared();
            if horiz > cull2 {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b.y) {
                        best = Some(GroundHit { y, normal: n / nl * ny.signum() });
                    }
                }
            }
        }
        // Terrain heightmap is another walkable candidate under the feet (treated as ~flat locally).
        if let Some(hm) = &self.heightmap {
            if let Some(hy) = hm.sample(pos.x, pos.z) {
                if hy >= pos.y - down && hy <= pos.y + up && best.map_or(true, |b| hy > b.y) {
                    best = Some(GroundHit { y: hy, normal: Vec3::Y });
                }
            }
        }
        best
    }

    /// Nearest WALL contact to the capsule (feet `pos`, `radius`, `height`): the signed clearance
    /// `gap` (distance from the capsule *surface* to the wall — negative when penetrating) and the
    /// contact `normal` (unit, pointing from the wall toward the capsule / into free space). `None`
    /// when there is no wall geometry. Floors are excluded (they never block horizontal motion; the
    /// ground probe owns Y). This is the primitive the swept linear cast advances against.
    fn closest_wall(&self, pos: Vec3, radius: f32, height: f32) -> Option<(f32, Vec3)> {
        let a = pos + Vec3::Y * radius;
        let b = pos + Vec3::Y * (height - radius);
        let mut best: Option<(f32, Vec3)> = None; // (surface-to-surface distance, normal)
        for t in &self.tris {
            if !is_wall(t) {
                continue;
            }
            let (sp, tp) = seg_tri_closest(a, b, t[0], t[1], t[2]);
            let diff = sp - tp;
            let dist = diff.length();
            let n = if dist > 1e-4 {
                diff / dist
            } else {
                // Grazing/coincident: use the wall's face normal, oriented toward the capsule axis.
                let fn_ = tri_normal(t);
                let fnl = fn_.length();
                if fnl <= 1e-6 {
                    continue;
                }
                let f = fn_ / fnl;
                if f.dot(sp - tp).abs() < 1e-6 && f.dot((a + b) * 0.5 - tp) < 0.0 { -f } else { f }
            };
            if best.map_or(true, |(bd, _)| dist < bd) {
                best = Some((dist, n));
            }
        }
        best.map(|(dist, n)| (dist - radius, n))
    }

    /// Swept **linear cast** of the character capsule from `pos` along `delta` (the
    /// `hkpCharacterProxy` / `HumanLinearCastJob` sweep). Returns the fraction `toi ∈ [0,1]` of `delta`
    /// travelled before the first *blocking* wall contact, plus that contact normal — or `None` if the
    /// whole `delta` is clear. Implemented by **conservative advancement**: never step further than the
    /// current wall clearance, so a capsule can never tunnel a thin wall regardless of `|delta|` (this
    /// is the tunnel-free upgrade over pure depenetration). A contact only *blocks* when the capsule is
    /// actually moving into it (`dir·normal < 0`); grazing/separating contacts are skipped.
    pub fn linear_cast(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32) -> Option<(f32, Vec3)> {
        let dist = delta.length();
        if dist < 1e-6 {
            return None;
        }
        let dir = delta / dist;
        const SKIN: f32 = 1e-3;
        let mut t = 0.0f32;
        for _ in 0..64 {
            let p = pos + delta * t;
            let (gap, n) = match self.closest_wall(p, radius, height) {
                Some(g) => g,
                None => return None, // no walls at all → clear
            };
            if gap <= SKIN {
                // Only a contact the capsule is actually moving *into* blocks; a grazing / parallel /
                // separating contact (|dir·n| ~ 0) does not — otherwise a slide along a wall would
                // stall on its own spurious sub-microradian normal. The remaining sweep is clear.
                if dir.dot(n) < -1e-3 {
                    return Some((t, n)); // blocking contact
                }
                return None;
            }
            // Safe conservative advance: move at most the clearance in world units (never tunnels).
            t += (gap / dist).max(SKIN / dist);
            if t >= 1.0 {
                return None;
            }
        }
        None // budget exhausted without a blocking contact → treat as clear (final depenetrate cleans up)
    }

    /// Swept collide-and-slide move of the character capsule (feet `pos`) by desired `delta`, WITHOUT
    /// any ground snap (the vertical/ground handling is the caller's — see [`CharacterController`]).
    /// Uses [`Self::linear_cast`] to advance to each wall, removes the into-wall velocity component
    /// (the slide), and repeats; a final depenetration pass resolves inside corners. Tunnel-free.
    pub fn move_swept(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32) -> Vec3 {
        let mut p = pos;
        let mut rem = delta;
        for _ in 0..4 {
            match self.linear_cast(p, rem, radius, height) {
                Some((toi, n)) => {
                    p += rem * toi;
                    rem *= 1.0 - toi;
                    let into = rem.dot(n);
                    if into < 0.0 {
                        rem -= n * into; // slide along the wall
                    }
                    if rem.length_squared() < 1e-8 {
                        break;
                    }
                }
                None => {
                    p += rem;
                    break;
                }
            }
        }
        self.depenetrate(p, radius, height)
    }

    /// Public slope-aware ground probe: highest walkable surface under `pos` in the band
    /// `[pos.y - down, pos.y + up]`, rejecting surfaces steeper than `acos(min_cos)`. See
    /// [`GroundHit`]. Used by the character controller and available to sim silos.
    pub fn ground_hit(&self, pos: Vec3, radius: f32, up: f32, down: f32, min_cos: f32) -> Option<GroundHit> {
        self.ground_probe(pos, radius, up, down, min_cos)
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
    /// Now a **swept linear cast** collide-and-slide (tunnel-free — see [`StaticSoupPhysics::move_swept`]),
    /// followed by a ground snap within `step`. The state machine / gravity / jump live in
    /// [`CharacterController`]; this trait method is the stateless "move + step-follow" the seam exposes.
    fn move_character(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32, step: f32) -> Vec3 {
        let mut p = self.move_swept(pos, delta, radius, height);
        // Snap the feet to the ground within `step` (climbs low ledges, follows ramps/terrain).
        if let Some(g) = self.ground_probe(p, radius, step, step, 0.5) {
            p.y = g.y;
        }
        p
    }
}

// ---------------------------------------------------------------------------
//   CharacterController — faithful hkpCharacterProxy 5-state machine
// ---------------------------------------------------------------------------

/// Default gravity magnitude (world units/s²), downward.
///
/// `// CONFIRM-LIVE:` the retail value is `hkpWorldCinfo`'s gravity `hkVector4` (`FUN_008e2da0`),
/// initialized in a VMX128/SSE setup path that **does not decode** in either build
/// (`physics_code_map.md` §1/§10). This is a faithful modern default, not the exe's exact constant —
/// read it live off `hkpWorldCinfo` to pin. Overridable per controller (`CharacterController::gravity`).
pub const DEFAULT_GRAVITY: f32 = -9.81;

/// Default slope limit: cosine of the max walkable incline (~50°).
///
/// `// CONFIRM-LIVE:` retail source is the proxy cinfo `maxSlopeCosine` at `+0xa4`
/// (`FUN_0094dd30`, computed via `_CIcos`) — a VMX-heavy numeric not statically decoded. Faithful
/// default; read live to pin the exact angle.
pub const DEFAULT_MAX_SLOPE_COS: f32 = 0.642_787_6; // cos(50°)

/// The three core states of the recovered `hkpCharacterContext` machine (`FUN_0094d2e0`).
///
/// The retail machine registers five states — OnGround (id 2, `FUN_0094ce90`), InAir (id 3,
/// `FUN_0094d7b0`), Jumping (id 1, `FUN_00951ef0`), plus two game states Climbing/Ladder-Flying
/// (ids 5/6, built inline in `HumanPhysics::Activate FUN_004255c0`). This controller implements the
/// three **locomotion** states faithful to that machine; Climbing/Ladder are a `// CONFIRM-LIVE:`
/// deferred item (they need the ladder-volume game data, not physics — see `DEFERRED.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterState {
    /// State 2 — feet on a walkable surface; horizontal intent drives movement, no gravity.
    OnGround,
    /// State 1 — ascending from a jump impulse; gravity decelerates until apex, then → `InAir`.
    Jumping,
    /// State 3 — falling / airborne with reduced air control; lands → `OnGround`.
    InAir,
}

/// Per-frame movement intent handed to [`CharacterController::step`] (the game's `ControllerPlayer`
/// input, `0x6ca511b2`). `move_dir` is a desired *horizontal* direction (need not be unit; Y is
/// ignored) scaled internally by `move_speed`; `jump` is an edge-triggered jump request.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterInput {
    /// Desired horizontal move direction in world space (Y ignored). Zero = stand still.
    pub move_dir: Vec3,
    /// Request a jump this frame (only honored while `OnGround`).
    pub jump: bool,
}

/// A faithful reimplementation of the retail on-foot controller: a swept-capsule
/// `hkpCharacterProxy` (`FUN_0094f2c0`) driven by the `hkpCharacterContext` OnGround/InAir/Jumping
/// state machine (`FUN_0094d2e0`), assembled by `HumanPhysics::Activate` (`FUN_004255c0`).
///
/// It queries the world through the [`PhysicsQuery`] seam only (`raycast` for ground/landing,
/// `move_character` for the swept collide-and-slide), so it runs against **any** `&dyn PhysicsQuery`
/// — the [`StaticSoupPhysics`] bridge today, the full `hkpWorld` later. Gravity, jump, slope and step
/// limits are handled here (the state machine); the swept move + step-follow come from the query impl.
///
/// `// CONFIRM-LIVE:` the per-frame *integrator* (`hkpWorld::step`) is VMX128 and does not decode
/// (`physics_code_map.md` §10); the semi-implicit Euler here is a faithful modern equivalent, not the
/// exe's exact solver. Tunables (`gravity`, `jump_speed`, `move_speed`, `air_control`, `max_slope_cos`)
/// are `// CONFIRM-LIVE:` defaults — data-driven, overridable, and to be pinned live.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterController {
    /// Capsule radius.
    pub radius: f32,
    /// Capsule total height (feet to crown).
    pub height: f32,
    /// Max ledge the feet auto-climb (`hkpCharacterProxy` step height).
    pub step_height: f32,
    /// Slope limit: cosine of the steepest walkable incline. See [`DEFAULT_MAX_SLOPE_COS`].
    pub max_slope_cos: f32,
    /// Horizontal ground/air speed applied to a unit `move_dir`.
    pub move_speed: f32,
    /// Upward launch speed on jump.
    pub jump_speed: f32,
    /// Gravity (negative = down). See [`DEFAULT_GRAVITY`].
    pub gravity: f32,
    /// Fraction `[0,1]` of horizontal authority retained while airborne.
    pub air_control: f32,
    /// Feet position (world space).
    pub position: Vec3,
    /// World velocity (horizontal is intent-driven; `y` is integrated by gravity/jump).
    pub velocity: Vec3,
    /// Current locomotion state.
    pub state: CharacterState,
}

impl CharacterController {
    /// Build a controller standing on the ground at `position` (feet), with the given capsule.
    /// Tunables default to the `// CONFIRM-LIVE:` faithful values; set the fields to override.
    pub fn new(position: Vec3, radius: f32, height: f32) -> Self {
        Self {
            radius,
            height,
            step_height: 0.4,
            max_slope_cos: DEFAULT_MAX_SLOPE_COS,
            move_speed: 5.0,
            jump_speed: 6.0,
            gravity: DEFAULT_GRAVITY,
            air_control: 0.35,
            position,
            velocity: Vec3::ZERO,
            state: CharacterState::OnGround,
        }
    }

    /// Horizontal desired velocity for this frame from the input intent.
    fn desired_horizontal(&self, input: &CharacterInput) -> Vec3 {
        let flat = Vec3::new(input.move_dir.x, 0.0, input.move_dir.z);
        if flat.length_squared() > 1e-8 {
            flat.normalize() * self.move_speed
        } else {
            Vec3::ZERO
        }
    }

    /// Blend the current horizontal velocity toward `intent` by `air_control` (reduced authority
    /// while airborne, per `hkpCharacterStateInAir` air-control setter `FUN_0094d780`).
    fn air_horizontal(&self, intent: Vec3) -> Vec3 {
        let cur = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
        cur + (intent - cur) * self.air_control.clamp(0.0, 1.0)
    }

    /// Advance the character one fixed step of `dt` seconds against `phys`, running the state machine
    /// (gravity, jump, ground/air transitions), the swept move, and slope/step limits. Mutates
    /// `position`, `velocity`, and `state`. Faithful to the recovered 5-state machine (three
    /// locomotion states); see the type docs for the `// CONFIRM-LIVE:` integrator note.
    pub fn step(&mut self, phys: &dyn PhysicsQuery, input: CharacterInput, dt: f32) {
        let intent = self.desired_horizontal(&input);
        match self.state {
            CharacterState::OnGround => {
                self.velocity.y = 0.0;
                if input.jump {
                    // OnGround → Jumping: apply the launch impulse, then move as airborne this frame.
                    self.velocity.y = self.jump_speed;
                    self.state = CharacterState::Jumping;
                    self.velocity.x = intent.x;
                    self.velocity.z = intent.z;
                    self.airborne_move(phys, dt);
                } else {
                    self.velocity.x = intent.x;
                    self.velocity.z = intent.z;
                    // Grounded move: swept collide-and-slide + step-follow within `step_height`.
                    let delta = Vec3::new(intent.x, 0.0, intent.z) * dt;
                    let moved = phys.move_character(self.position, delta, self.radius, self.height, self.step_height);
                    // Re-probe: is there still walkable ground under the feet? (walk-off-edge test)
                    if let Some(gy) = self.probe_ground_below(phys, moved) {
                        self.position = Vec3::new(moved.x, gy, moved.z);
                    } else {
                        // Stepped off an edge → fall.
                        self.position = moved;
                        self.state = CharacterState::InAir;
                    }
                }
            }
            CharacterState::Jumping => {
                self.velocity.y += self.gravity * dt;
                let h = self.air_horizontal(intent);
                self.velocity.x = h.x;
                self.velocity.z = h.z;
                self.airborne_move(phys, dt);
                // Apex reached → fall.
                if self.state == CharacterState::Jumping && self.velocity.y <= 0.0 {
                    self.state = CharacterState::InAir;
                }
            }
            CharacterState::InAir => {
                self.velocity.y += self.gravity * dt;
                let h = self.air_horizontal(intent);
                self.velocity.x = h.x;
                self.velocity.z = h.z;
                self.airborne_move(phys, dt);
            }
        }
    }

    /// Walkable-ground height directly under `feet` within the step band, honoring the slope limit.
    /// Uses a downward raycast (the `hkpWorldRayCaster` floor-probe the retail controller shares).
    fn probe_ground_below(&self, phys: &dyn PhysicsQuery, feet: Vec3) -> Option<f32> {
        let hit = phys.raycast(feet + Vec3::Y * self.step_height, -Vec3::Y, self.step_height * 2.0)?;
        if hit.normal.y.abs() < self.max_slope_cos {
            return None; // too steep to stand on
        }
        let y = hit.point.y;
        if y <= feet.y + self.step_height && y >= feet.y - self.step_height {
            Some(y)
        } else {
            None
        }
    }

    /// Airborne integration: horizontal collide-and-slide (no ground snap) + vertical gravity/jump
    /// integration, with a landing test on the way down that transitions back to `OnGround`.
    fn airborne_move(&mut self, phys: &dyn PhysicsQuery, dt: f32) {
        // Horizontal collide-and-slide only (step = 0 disables step-up while airborne). We keep our
        // own integrated Y, so any ground snap the query applies is discarded here.
        let hdelta = Vec3::new(self.velocity.x, 0.0, self.velocity.z) * dt;
        let slid = phys.move_character(self.position, hdelta, self.radius, self.height, 0.0);
        let mut np = Vec3::new(slid.x, self.position.y, slid.z);

        let vy_dt = self.velocity.y * dt;
        let target_y = np.y + vy_dt;

        if self.velocity.y <= 0.0 {
            // Descending: probe for a floor within the fall this frame (+ a step of tolerance).
            let descent = (-vy_dt).max(0.0);
            if let Some(hit) = phys.raycast(np + Vec3::Y * self.step_height, -Vec3::Y, self.step_height + descent) {
                let ground_y = hit.point.y;
                if target_y <= ground_y && hit.normal.y.abs() >= self.max_slope_cos {
                    // Landed.
                    np.y = ground_y;
                    self.velocity.y = 0.0;
                    self.state = CharacterState::OnGround;
                    self.position = np;
                    return;
                }
            }
        }
        np.y = target_y;
        self.position = np;
    }
}

// ---------------------------------------------------------------------------
//   RigidBody — minimal hkpWorld::step analog for props / debris
// ---------------------------------------------------------------------------

/// A minimal dynamic rigid body (modelled as a sphere) for props / debris — the reimpl stand-in for a
/// retail `hkpRigidBody` (`FUN_008d4be0`) stepped by `hkpWorld::step`. Enough to let props fall,
/// bounce, and settle on the static soup; full inertia-tensor / contact-manifold dynamics arrive with
/// the Havok world (`DEFERRED.md`).
///
/// `// CONFIRM-LIVE:` the exact integrator + restitution/friction model is VMX128-undecoded
/// (`physics_code_map.md` §10); [`StaticSoupPhysics::step_rigid_body`] is a faithful semi-implicit
/// Euler + impulse resolve, not the exe's solver. `restitution`/`friction`/`mass` defaults are
/// `// CONFIRM-LIVE:` gameplay values (read the `hkpRigidBody` material fields live to pin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBody {
    /// Center-of-mass world position.
    pub position: Vec3,
    /// Linear velocity.
    pub velocity: Vec3,
    /// Bounding sphere radius (collision shape stand-in).
    pub radius: f32,
    /// Bounciness `[0,1]` (fraction of normal speed retained on impact).
    pub restitution: f32,
    /// Tangential friction `[0,1]` (fraction of tangential speed removed on contact).
    pub friction: f32,
    /// Mass (currently informational; reserved for pairwise dynamics).
    pub mass: f32,
    /// Set once the body has come to rest on a support (skips integration until disturbed).
    pub resting: bool,
}

impl RigidBody {
    /// A sphere body at `position` with sensible `// CONFIRM-LIVE:` material defaults.
    pub fn new(position: Vec3, radius: f32) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            radius,
            restitution: 0.2,
            friction: 0.4,
            mass: 1.0,
            resting: false,
        }
    }
}

impl StaticSoupPhysics {
    /// Step one [`RigidBody`] by `dt` against the static soup + heightmap — the minimal
    /// `hkpWorld::step` analog (props/debris). Semi-implicit Euler integrate, then resolve every
    /// penetration against the soup (push out along the contact normal, kill the into-surface velocity
    /// with `restitution`, shave tangential velocity by `friction`), and park the body as `resting`
    /// once it is slow and supported.
    ///
    /// `// CONFIRM-LIVE:` the integrator/solver is a faithful equivalent, not the exe's VMX128 math
    /// (`physics_code_map.md` §10). `gravity` is passed in (data-driven; e.g. `Vec3::Y * DEFAULT_GRAVITY`).
    pub fn step_rigid_body(&self, body: &mut RigidBody, dt: f32, gravity: Vec3) {
        if body.resting {
            return;
        }
        // Semi-implicit (symplectic) Euler.
        body.velocity += gravity * dt;
        body.position += body.velocity * dt;

        let mut grounded = false;
        // Resolve the deepest penetration a few times (relaxation for corners/multiple contacts).
        for _ in 0..4 {
            let mut best: Option<(f32, Vec3)> = None; // (penetration depth, contact normal)
            let cull2 = (body.radius + 4.0) * (body.radius + 4.0);
            for t in &self.tris {
                if (t[0] - body.position).length_squared() > cull2 {
                    continue;
                }
                let cp = closest_on_tri(body.position, t[0], t[1], t[2]);
                let d = body.position - cp;
                let dist = d.length();
                if dist < body.radius {
                    let n = if dist > 1e-4 {
                        d / dist
                    } else {
                        let fn_ = tri_normal(t);
                        if fn_.length() <= 1e-6 {
                            continue;
                        }
                        fn_.normalize()
                    };
                    let pen = body.radius - dist;
                    if best.map_or(true, |(bp, _)| pen > bp) {
                        best = Some((pen, n));
                    }
                }
            }
            // Heightmap floor as a contact plane (normal +Y).
            if let Some(hm) = &self.heightmap {
                if let Some(hy) = hm.sample(body.position.x, body.position.z) {
                    let pen = body.radius - (body.position.y - hy);
                    if pen > 0.0 && best.map_or(true, |(bp, _)| pen > bp) {
                        best = Some((pen, Vec3::Y));
                    }
                }
            }
            match best {
                Some((pen, n)) => {
                    body.position += n * pen; // depenetrate
                    let vn = body.velocity.dot(n);
                    if vn < 0.0 {
                        let normal_v = n * vn;
                        let tangent_v = body.velocity - normal_v;
                        // Suppress restitution for slow (resting) contacts so the body settles instead
                        // of micro-bouncing forever on the gravity it re-accumulates each frame.
                        let e = if -vn < 1.0 { 0.0 } else { body.restitution };
                        body.velocity = tangent_v * (1.0 - body.friction).clamp(0.0, 1.0) - normal_v * e;
                    }
                    if n.y > 0.3 {
                        grounded = true;
                    }
                }
                None => break,
            }
        }

        // Settle: slow and supported → park it so it stops jittering.
        if grounded && body.velocity.length_squared() < 0.02 * 0.02 {
            body.velocity = Vec3::ZERO;
            body.resting = true;
        }
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

#[cfg(test)]
mod controller_tests {
    use super::*;

    // A flat floor at height `y` over [x0,x1]×[z0,z1], tiled into 1-unit quads (small triangles, like
    // the game's world soup — the impl's proximity culls are tuned for small triangles). Wound +Y.
    fn tiled_floor(y: f32, x0: f32, x1: f32, z0: f32, z1: f32) -> Vec<[Vec3; 3]> {
        let mut out = Vec::new();
        let mut x = x0;
        while x < x1 - 1e-3 {
            let nx = (x + 1.0).min(x1);
            let mut z = z0;
            while z < z1 - 1e-3 {
                let nz = (z + 1.0).min(z1);
                let a = Vec3::new(x, y, z);
                let b = Vec3::new(nx, y, z);
                let c = Vec3::new(nx, y, nz);
                let d = Vec3::new(x, y, nz);
                out.push([a, c, b]);
                out.push([a, d, c]);
                z = nz;
            }
            x = nx;
        }
        out
    }

    // A flat walkable floor centred on the origin (tiled small triangles).
    fn big_floor(y: f32) -> Vec<[Vec3; 3]> {
        tiled_floor(y, -12.0, 12.0, -12.0, 12.0)
    }

    // Thin double-sided wall quad in the plane x = `xw`, spanning z in [z0,z1], y in [y0,y1].
    fn wall(xw: f32, z0: f32, z1: f32, y0: f32, y1: f32) -> Vec<[Vec3; 3]> {
        let a = Vec3::new(xw, y0, z0);
        let b = Vec3::new(xw, y0, z1);
        let c = Vec3::new(xw, y1, z1);
        let d = Vec3::new(xw, y1, z0);
        // both windings so the wall blocks from either side
        vec![[a, b, c], [a, c, d], [a, c, b], [a, d, c]]
    }

    // swept move: no tunneling through a thin wall at speed
    #[test]
    fn swept_move_does_not_tunnel_thin_wall_at_speed() {
        // Thin wall at x = 0. Capsule starts well behind it and is shoved 10 units in ONE step.
        let phys = StaticSoupPhysics::new(wall(0.0, -5.0, 5.0, 0.0, 3.0));
        let radius = 0.3;
        let start = Vec3::new(-2.0, 0.0, 0.0);
        let end = phys.move_swept(start, Vec3::new(10.0, 0.0, 0.0), radius, 1.8);
        // Must be stopped on the near side of the wall, not teleported through it.
        assert!(end.x <= -radius + 1e-2, "tunneled through the wall: x = {}", end.x);
        // Sanity: the OLD depenetration-only move WOULD have tunneled (ends far past the wall).
        let naive = phys.depenetrate(start + Vec3::new(10.0, 0.0, 0.0), radius, 1.8);
        assert!(naive.x > 5.0, "expected naive depenetration to tunnel, x = {}", naive.x);
    }

    #[test]
    fn linear_cast_reports_toi_and_normal() {
        let phys = StaticSoupPhysics::new(wall(0.0, -5.0, 5.0, 0.0, 3.0));
        let radius = 0.3;
        let (toi, n) = phys
            .linear_cast(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), radius, 1.8)
            .expect("should hit the wall");
        // Contact ~1.7 units in (2.0 gap minus 0.3 radius) out of a 10-unit sweep.
        assert!((toi - 0.17).abs() < 0.05, "toi = {}", toi);
        // Normal points back toward the capsule (-X), i.e. into the free side.
        assert!(n.x < -0.9, "normal = {:?}", n);
    }

    // state machine: OnGround -> InAir on walk-off
    #[test]
    fn walk_off_edge_transitions_ground_to_air() {
        // Floor only for x < 0 (a ledge at x = 0).
        let phys = StaticSoupPhysics::new(tiled_floor(0.0, -12.0, 0.0, -12.0, 12.0));
        let mut cc = CharacterController::new(Vec3::new(-0.2, 0.0, 0.0), 0.3, 1.8);
        cc.move_speed = 5.0;
        assert_eq!(cc.state, CharacterState::OnGround);
        // Walk +X off the ledge.
        for _ in 0..20 {
            cc.step(&phys, CharacterInput { move_dir: Vec3::X, jump: false }, 1.0 / 60.0);
            if cc.state == CharacterState::InAir {
                break;
            }
        }
        assert_eq!(cc.state, CharacterState::InAir, "should fall off the ledge");
        assert!(cc.position.x > 0.0, "should have crossed the edge: x = {}", cc.position.x);
    }

    // state machine: air -> ground on landing
    #[test]
    fn falling_lands_on_ground() {
        let phys = StaticSoupPhysics::new(big_floor(0.0));
        let mut cc = CharacterController::new(Vec3::new(0.0, 5.0, 0.0), 0.3, 1.8);
        cc.state = CharacterState::InAir; // spawned in the air
        let mut landed = false;
        for _ in 0..600 {
            cc.step(&phys, CharacterInput::default(), 1.0 / 60.0);
            if cc.state == CharacterState::OnGround {
                landed = true;
                break;
            }
            assert!(cc.position.y > -1.0, "fell through the floor: y = {}", cc.position.y);
        }
        assert!(landed, "never landed");
        assert!(cc.position.y.abs() < 1e-2, "feet not resting on floor: y = {}", cc.position.y);
        assert!(cc.velocity.y.abs() < 1e-3, "vertical velocity not cleared on land");
    }

    // state machine: jump takes off, arcs, and lands
    #[test]
    fn jump_launches_arcs_and_returns_to_ground() {
        let phys = StaticSoupPhysics::new(big_floor(0.0));
        let mut cc = CharacterController::new(Vec3::ZERO, 0.3, 1.8);
        cc.jump_speed = 6.0;
        // Jump.
        cc.step(&phys, CharacterInput { move_dir: Vec3::ZERO, jump: true }, 1.0 / 60.0);
        assert_eq!(cc.state, CharacterState::Jumping, "jump should enter the Jumping state");
        assert!(cc.position.y > 0.0, "should leave the ground: y = {}", cc.position.y);
        let mut max_y = cc.position.y;
        let mut saw_inair = false;
        let mut back_on_ground = false;
        for _ in 0..600 {
            cc.step(&phys, CharacterInput::default(), 1.0 / 60.0);
            max_y = max_y.max(cc.position.y);
            if cc.state == CharacterState::InAir {
                saw_inair = true;
            }
            if saw_inair && cc.state == CharacterState::OnGround {
                back_on_ground = true;
                break;
            }
        }
        assert!(max_y > 0.5, "jump apex too low: {}", max_y);
        assert!(saw_inair, "never transitioned Jumping -> InAir at apex");
        assert!(back_on_ground, "never landed after the jump");
        assert!(cc.position.y.abs() < 1e-2, "did not settle on the floor: y = {}", cc.position.y);
    }

    // step limit: climbs a low ledge, refuses a tall one
    #[test]
    fn step_limit_climbs_low_refuses_tall() {
        // Raised slab for x >= 1 (tiled small triangles).
        let slab = |y: f32| tiled_floor(y, 1.0, 12.0, -12.0, 12.0);
        // 0.3 ledge (below 0.4 step) -> climbs.
        let mut low = big_floor(0.0);
        low.extend(slab(0.3));
        let phys_low = StaticSoupPhysics::new(low);
        let mut cc = CharacterController::new(Vec3::new(0.5, 0.0, 0.0), 0.3, 1.8);
        cc.step_height = 0.4;
        for _ in 0..30 {
            cc.step(&phys_low, CharacterInput { move_dir: Vec3::X, jump: false }, 1.0 / 60.0);
        }
        assert!(cc.position.x > 1.0, "did not advance onto the low ledge: x = {}", cc.position.x);
        assert!((cc.position.y - 0.3).abs() < 1e-2, "did not climb the low ledge: y = {}", cc.position.y);

        // 0.9 ledge (above 0.4 step) -> cannot climb; stays on the low floor (feet ~0).
        let mut hi = big_floor(0.0);
        hi.extend(slab(0.9));
        let phys_hi = StaticSoupPhysics::new(hi);
        let mut cc2 = CharacterController::new(Vec3::new(0.5, 0.0, 0.0), 0.3, 1.8);
        cc2.step_height = 0.4;
        for _ in 0..30 {
            cc2.step(&phys_hi, CharacterInput { move_dir: Vec3::X, jump: false }, 1.0 / 60.0);
        }
        assert!(cc2.position.y < 0.4, "should not climb a ledge above the step limit: y = {}", cc2.position.y);
    }

    // slope limit: follows a shallow ramp, stays grounded
    #[test]
    fn slope_limit_walks_shallow_ramp() {
        // Shallow 20-degree ramp rising in +X: y = tan(20) * x, for x in [0, 10].
        let s = 20.0f32.to_radians().tan();
        let strip = |x0: f32, x1: f32| {
            let (z0, z1) = (-5.0, 5.0);
            let a = Vec3::new(x0, s * x0, z0);
            let b = Vec3::new(x1, s * x1, z0);
            let c = Vec3::new(x1, s * x1, z1);
            let d = Vec3::new(x0, s * x0, z1);
            [[a, c, b], [a, d, c]]
        };
        let mut tris: Vec<[Vec3; 3]> = Vec::new();
        for i in 0..20 {
            let x = i as f32 * 0.5;
            tris.extend(strip(x, x + 0.5));
        }
        let phys = StaticSoupPhysics::new(tris);
        let mut cc = CharacterController::new(Vec3::new(0.25, s * 0.25, 0.0), 0.3, 1.8);
        cc.step_height = 0.5;
        cc.move_speed = 3.0;
        for _ in 0..60 {
            cc.step(&phys, CharacterInput { move_dir: Vec3::X, jump: false }, 1.0 / 60.0);
        }
        // Advanced up the ramp and stayed glued to its surface (y ~ slope*x), still grounded.
        assert!(cc.position.x > 1.0, "did not climb the ramp: x = {}", cc.position.x);
        assert!(
            (cc.position.y - s * cc.position.x).abs() < 0.05,
            "not following the ramp surface: y = {} expected {}",
            cc.position.y,
            s * cc.position.x
        );
        assert_eq!(cc.state, CharacterState::OnGround, "should stay grounded on a walkable slope");
    }

    // rigid body: falls and settles on the ground
    #[test]
    fn rigid_body_settles_on_ground() {
        let phys = StaticSoupPhysics::new(big_floor(0.0));
        let mut body = RigidBody::new(Vec3::new(0.0, 5.0, 0.0), 0.5);
        let g = Vec3::Y * DEFAULT_GRAVITY;
        for _ in 0..600 {
            phys.step_rigid_body(&mut body, 1.0 / 60.0, g);
            if body.resting {
                break;
            }
            assert!(body.position.y > -1.0, "fell through the floor: y = {}", body.position.y);
        }
        assert!(body.resting, "body never came to rest");
        // Sphere of radius 0.5 rests with its center 0.5 above the floor.
        assert!((body.position.y - 0.5).abs() < 1e-2, "did not settle at radius height: y = {}", body.position.y);
        assert!(body.velocity.length() < 1e-3, "resting body still moving");
    }

    #[test]
    fn rigid_body_bounces_before_settling() {
        let phys = StaticSoupPhysics::new(big_floor(0.0));
        let mut body = RigidBody::new(Vec3::new(0.0, 3.0, 0.0), 0.5);
        body.restitution = 0.6;
        let g = Vec3::Y * DEFAULT_GRAVITY;
        let mut bounced_up_after_contact = false;
        let mut touched = false;
        for _ in 0..600 {
            phys.step_rigid_body(&mut body, 1.0 / 60.0, g);
            if body.position.y <= 0.55 {
                touched = true;
            }
            if touched && body.velocity.y > 0.1 {
                bounced_up_after_contact = true;
            }
            if body.resting {
                break;
            }
        }
        assert!(bounced_up_after_contact, "restitution should have produced an upward bounce");
        assert!(body.resting, "should still come to rest eventually");
    }
}
