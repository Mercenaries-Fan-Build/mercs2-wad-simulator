//! Game space to Unreal space: the one place an axis, a unit or a triangle's winding changes.
//!
//! The game's world is Y-up and in metres; Unreal's is Z-up and in centimetres. The map is
//!
//! ```text
//! (x, y, z) m  →  (100·x, 100·z, 100·y) cm
//! ```
//!
//! Exchanging two axes is a reflection, and these two spaces need one. Retail's projection carries a
//! mirror — `m00` = -1.92 at viewport `+0xB30` with the camera's right vector at +X, in the
//! `23fe7f38` PMC capture — so looking along +Z, +X is on the LEFT of the screen
//! ([`mercs2_engine::camera::screen_right`]). Unreal's axes are X forward, Y right, Z up, with no
//! mirror in its projection. A rotation between the two would put every prop on the wrong side of the
//! room; this reflection puts the game's screen-right on Unreal's screen-right for every view direction
//! (`screen_right_is_preserved` below).
//!
//! Everything else follows from the map being a reflection `M` with a scale:
//!
//! * a direction (normal, tangent, bitangent) goes through `M` alone;
//! * a rotation `R` becomes `M·R·M`, which as a quaternion is `(-x, -z, -y, w)`: the axis goes through
//!   `M` and the angle changes sign;
//! * a triangle keeps its vertex order, and with it lands on the side Unreal's renderer calls the front
//!   (see [`triangle`]).

/// Unreal units (centimetres) per game unit (metre).
pub const UNITS_PER_METRE: f32 = 100.0;

/// A point.
pub fn position(p: [f32; 3]) -> [f32; 3] {
    [p[0] * UNITS_PER_METRE, p[2] * UNITS_PER_METRE, p[1] * UNITS_PER_METRE]
}

/// A direction: a normal, a tangent, a bitangent, an axis. No scale.
pub fn direction(d: [f32; 3]) -> [f32; 3] {
    [d[0], d[2], d[1]]
}

/// A length.
pub fn length(metres: f32) -> f32 {
    metres * UNITS_PER_METRE
}

/// A rotation quaternion `(x, y, z, w)`. Where the game rotates a model-space point `p` to `q·p`, the
/// result rotates [`position`]`(p)` to [`position`]`(q·p)`.
pub fn rotation(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[2], -q[1], q[3]]
}

/// A triangle's vertex order.
///
/// The game's triangles have `cross(b - a, c - a)` pointing ALONG the surface normal (measured on the
/// shipped models: `tests/retail.rs`, `model_triangles_face_the_way_unreal_draws`). A reflection turns
/// that cross product around, so in Unreal's space the same vertex order has it pointing AGAINST the
/// normal, which is the order Unreal's renderer treats as front-facing (`EFaceWindingOrder::CW` in
/// Epic's API reference: "cross product of consecutive face vertices opposes the stored normal
/// direction (DirectX / UE renderer convention)"). So the order is kept.
pub fn triangle(t: [u32; 3]) -> [u32; 3] {
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat3, Quat, Vec3};

    /// The linear part of [`position`], without the scale.
    fn m() -> Mat3 {
        Mat3::from_cols(Vec3::from(direction([1.0, 0.0, 0.0])), Vec3::from(direction([0.0, 1.0, 0.0])), Vec3::from(direction([0.0, 0.0, 1.0])))
    }

    /// A small deterministic spread of vectors and rotations (no RNG dependency).
    fn samples() -> Vec<(Vec3, Quat)> {
        let mut out = Vec::new();
        for i in 0..64u32 {
            let f = |k: u32| ((i * 37 + k * 101) % 97) as f32 / 48.0 - 1.0;
            let v = Vec3::new(f(1) * 30.0, f(2) * 30.0, f(3) * 30.0);
            let axis = Vec3::new(f(4), f(5), f(6) + 0.013).normalize();
            out.push((v, Quat::from_axis_angle(axis, f(7) * 3.0)));
        }
        out
    }

    #[test]
    fn up_is_unreals_z_and_a_metre_is_a_hundred_units() {
        assert_eq!(position([0.0, 1.0, 0.0]), [0.0, 0.0, 100.0]);
        assert_eq!(position([1.0, 0.0, 0.0]), [100.0, 0.0, 0.0]);
        assert_eq!(position([0.0, 0.0, 1.0]), [0.0, 100.0, 0.0]);
        assert_eq!(length(2.5), 250.0);
    }

    #[test]
    fn the_map_is_a_reflection() {
        assert_eq!(m().determinant(), -1.0);
        assert_eq!(m() * m(), Mat3::IDENTITY);
    }

    /// Unreal's axes are X forward, Y right, Z up, so its screen-right for a view direction `f` is
    /// `Z × f` (`Z × X = Y`). The game's is the engine's `screen_right`, which carries retail's mirror.
    #[test]
    fn screen_right_is_preserved() {
        let unreal_right = |f: Vec3| Vec3::Z.cross(f).normalize();
        assert_eq!(unreal_right(Vec3::X), Vec3::Y);
        for (v, _) in samples() {
            let forward = Vec3::new(v.x, 0.0, v.z).normalize();
            let game_right = mercs2_engine::camera::screen_right(forward);
            let got = Vec3::from(direction(game_right.to_array()));
            let want = unreal_right(Vec3::from(direction(forward.to_array())));
            assert!((got - want).length() < 1e-5, "forward {forward:?}: {got:?} is not {want:?}");
        }
    }

    #[test]
    fn a_converted_rotation_carries_converted_points() {
        for (p, q) in samples() {
            let want = Vec3::from(position((q * p).to_array()));
            let r = rotation(q.to_array());
            let got = Quat::from_xyzw(r[0], r[1], r[2], r[3]) * Vec3::from(position(p.to_array()));
            assert!((got - want).length() < 2e-3, "{got:?} is not {want:?}");
        }
    }

    #[test]
    fn a_converted_rotation_is_a_unit_quaternion() {
        for (_, q) in samples() {
            let r = rotation(q.to_array());
            let n = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
            assert!((n - 1.0).abs() < 1e-5);
        }
    }

    /// The reflection turns a triangle's face normal around relative to its converted vertex normal.
    #[test]
    fn a_kept_vertex_order_faces_the_other_way_after_the_map() {
        let (a, b, c) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
        let n = (b - a).cross(c - a).normalize();
        assert_eq!(n, Vec3::Y, "in game space this order's cross product is the +Y normal");
        let [i, j, k] = triangle([0, 1, 2]);
        let pts = [a, b, c].map(|p| Vec3::from(position(p.to_array())));
        let face = (pts[j as usize] - pts[i as usize]).cross(pts[k as usize] - pts[i as usize]).normalize();
        let normal = Vec3::from(direction(n.to_array()));
        assert_eq!(normal, Vec3::Z);
        assert_eq!(face, -normal, "cross(b - a, c - a) opposes the normal: Unreal's front face");
    }
}
