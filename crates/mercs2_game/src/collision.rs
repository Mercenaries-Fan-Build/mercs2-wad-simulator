//! Minimal CPU collision for the TPS world: a flat world-space triangle soup + a ray cast (for the
//! third-person camera boom, so it doesn't clip through walls) and a sphere push-out (so the player
//! doesn't walk through buildings). Brute-force over the collected triangles — fine for the interior /
//! nearby buildings (a few thousand tris); a BVH is the later optimization if the set grows large.

use mercs2_core::glam::Vec3;

/// Ray/triangle intersection (Möller–Trumbore). Returns the hit distance `t ≥ 0` along `dir` (which
/// need not be normalized — `t` is in units of `dir`), or `None`.
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

/// Nearest triangle hit along `[o, o + dir*max_t]` (double-sided). `dir` normalized; returns the hit
/// distance in metres. A cheap sphere broad-phase skips triangles whose first vertex is well beyond
/// the ray's reach, so an oversized soup (all the c3 cells) stays cheap.
pub fn raycast(tris: &[[Vec3; 3]], o: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
    let cull = max_t + 30.0; // slack for triangle edge length
    let cull2 = cull * cull;
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

/// Push a vertical capsule (feet at `pos`, `height` tall, `radius`) out of the WALLS it penetrates,
/// horizontally only (Y is owned by the terrain/floor snap). Two rules keep a detailed interior mesh
/// from spawning phantom blocks:
///  * Only NEAR-VERTICAL triangles count as walls (`|n.y| < 0.5`). Floors, ceilings, ramps, low
///    thresholds and overhead beams (horizontal-ish surfaces) never block horizontal movement — so
///    the player walks over door sills and under lintels, and through archways, freely.
///  * The capsule is tested at CHEST height only, so a floor step or a head-height moulding can't
///    catch it. A couple of relaxation passes resolve inside corners.
pub fn push_out(tris: &[[Vec3; 3]], mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
    let chest = height * 0.55;
    let cull = radius + height + 4.0;
    let cull2 = cull * cull;
    for _ in 0..2 {
        let c = pos + Vec3::Y * chest;
        for t in tris {
            if (t[0] - pos).length_squared() > cull2 {
                continue;
            }
            // Wall test: skip triangles whose normal is more vertical than horizontal.
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            let nlen = n.length();
            if nlen < 1e-6 || (n.y / nlen).abs() > 0.5 {
                continue;
            }
            let cp = closest_on_tri(c, t[0], t[1], t[2]);
            let mut d = c - cp;
            d.y = 0.0; // horizontal resolution only
            let dist = d.length();
            if dist < radius && dist > 1e-4 {
                pos += d / dist * (radius - dist);
            }
        }
    }
    pos
}
