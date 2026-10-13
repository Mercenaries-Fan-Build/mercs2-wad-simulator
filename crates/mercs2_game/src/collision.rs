//! CPU collision for the TPS world — a proper capsule character controller over a world-space triangle
//! soup, not a point-push hack.
//!
//! The player is a vertical CAPSULE (a core segment from `feet+radius` to `feet+height-radius`, swept by
//! `radius`). Movement is **collide-and-slide**: attempt the move, then depenetrate the capsule out of
//! WALL triangles — pushing perpendicular to each contact preserves the tangential motion, i.e. the
//! capsule slides along walls. FLOORS are handled separately by a **downward ground probe** that places
//! the feet on the surface underneath (within a step tolerance), so stairs, ramps and thresholds all
//! work: a step shorter than the capsule radius is cleared by the bottom sphere with no special case,
//! and taller steps within `step` are climbed/descended by the ground probe. This mirrors how the retail
//! engine used Havok capsule-vs-geometry (`MatchCapsuleToPose`) rather than a heightmap.
//!
//! The camera boom uses the same soup via `raycast` (a thick spherecast margin), matching the exe's
//! `CameraCollisionCastRay` (a radius² probe that keeps the camera out of geometry).

use mercs2_core::glam::Vec3;

/// A triangle is a WALL if its normal is more horizontal than vertical (steep surface). Walls block +
/// slide; walkable surfaces (floors/ramps) are left to the ground probe.
fn is_wall(t: &[Vec3; 3]) -> bool {
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    let nl = n.length();
    nl > 1e-6 && (n.y / nl).abs() < 0.5
}

// ---------------------------------------------------------------------------
//   Ray / spherecast (camera boom)
// ---------------------------------------------------------------------------

/// Ray/triangle intersection (Möller–Trumbore). Returns hit distance `t ≥ 0` along `dir`, or `None`.
pub fn ray_tri(o: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
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

/// Nearest triangle hit along `[o, o + dir*max_t]` (double-sided), with a cheap sphere broad-phase.
pub fn raycast(tris: &[[Vec3; 3]], o: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
    let cull2 = (max_t + 30.0) * (max_t + 30.0);
    let mut best: Option<f32> = None;
    for t in tris {
        if (t[0] - o).length_squared() > cull2 {
            continue;
        }
        if let Some(d) = ray_tri(o, dir, t[0], t[1], t[2]) {
            if d <= max_t && best.map_or(true, |b| d < b) {
                best = Some(d);
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------
//   Closest-point primitives
// ---------------------------------------------------------------------------

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
//   Capsule character controller
// ---------------------------------------------------------------------------

/// Push the capsule (feet `pos`, `radius`, `height`) out of every WALL triangle it penetrates. Pushing
/// perpendicular to each contact preserves tangential motion → the capsule slides along walls. A few
/// relaxation passes resolve inside corners. Floors are excluded (the ground probe owns Y).
fn depenetrate(tris: &[[Vec3; 3]], mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
    let cull2 = (radius + height + 4.0) * (radius + height + 4.0);
    for _ in 0..4 {
        let mut moved = false;
        for t in tris {
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
                    let n = (t[1] - t[0]).cross(t[2] - t[0]);
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

/// Downward ground probe: the highest WALKABLE surface under `pos` within `[pos.y - step, pos.y + step]`.
/// This is what makes the feet follow stairs/ramps and clears low thresholds without any height hack.
fn ground_y(tris: &[[Vec3; 3]], pos: Vec3, radius: f32, step: f32) -> Option<f32> {
    let origin = pos + Vec3::Y * step;
    let max_t = step * 2.0;
    let cull2 = (radius + 2.0) * (radius + 2.0);
    let mut best: Option<f32> = None;
    for t in tris {
        // Only walkable (near-horizontal) surfaces are ground; skip walls.
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
    best
}

/// Move the player capsule by a horizontal displacement with collide-and-slide against walls, then
/// (when `follow_ground`) place the feet on the surface underneath within `step`. Returns the new feet
/// position. `follow_ground=false` leaves Y to the caller (e.g. the exterior terrain heightmap).
pub fn move_character(
    tris: &[[Vec3; 3]],
    feet: Vec3,
    horiz_move: Vec3,
    radius: f32,
    height: f32,
    step: f32,
    follow_ground: bool,
) -> Vec3 {
    // Attempt the move, then depenetrate out of walls — perpendicular push-out is the slide.
    let mut pos = feet + Vec3::new(horiz_move.x, 0.0, horiz_move.z);
    pos = depenetrate(tris, pos, radius, height);
    if follow_ground {
        if let Some(gy) = ground_y(tris, pos, radius, step) {
            pos.y = gy;
        } else {
            pos.y = feet.y; // no ground within step (edge/gap): hold Y (no fall yet)
        }
    }
    pos
}
