//! Third-person camera rig — the over-the-shoulder framing + boom-collision math, extracted from the
//! world loop so the geometry is unit-testable without the GUI.
//!
//! This was inline in `world::run_scene_world_loading`'s view-matrix `match`. It is a pure function of
//! the player position and the look angles (plus the collision soup for boom pull-in), so pulling it
//! out lets us assert the framing invariants (eye sits behind + above the focus; the boom retracts when
//! a wall is close) in isolation. Behaviour is preserved verbatim from the closure.

use mercs2_core::glam::{Mat4, Vec3};

/// Focus height above the player origin: the character's upper back/shoulder.
const FOCUS_UP: f32 = 1.6;
/// Eye distance behind the focus (6 m / 3 m both read too far back for the retail framing).
const BOOM: f32 = 2.2;
/// Lateral shoulder offset (puts the character slightly left-of-centre).
const SHOULDER: f32 = 0.55;
/// Camera collision radius — the boom stops this far short of the nearest wall.
const CAM_RADIUS: f32 = 0.35;
/// Minimum eye distance the boom is allowed to retract to (so it never ends up inside the focus).
const MIN_BOOM: f32 = 0.6;

/// The over-the-shoulder view matrix for a player at `player_pos` looking at (`yaw`, `pitch`).
///
/// The eye sits a short boom behind + above the focus with a small lateral shoulder offset; when
/// `collision` is non-empty the boom is cast from the focus toward the desired eye and pulled in to
/// just short of the nearest wall (so it doesn't clip through geometry). `yaw = 0` looks toward +Z with
/// the eye on the -Z side (behind a +Z-facing player).
///
/// The retail exact framing values live in the binary's .rdata (CameraOffset / CameraOffsetZ /
/// HumanCameraModifier) — recoverable via x32dbg; the constants here are the tuned placeholders.
pub fn third_person_view(player_pos: Vec3, yaw: f32, pitch: f32, collision: &[[Vec3; 3]]) -> Mat4 {
    let dir = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()).normalize();
    let focus = player_pos + Vec3::Y * FOCUS_UP;
    let right = Vec3::Y.cross(dir).normalize();
    let want_eye = focus - dir * BOOM + right * SHOULDER;
    let eye = if collision.is_empty() {
        want_eye
    } else {
        let boom_vec = want_eye - focus;
        let boom_len = boom_vec.length();
        let boom_dir = boom_vec / boom_len;
        match crate::collision::raycast(collision, focus, boom_dir, boom_len) {
            Some(hit) => focus + boom_dir * (hit - CAM_RADIUS).max(MIN_BOOM),
            None => want_eye,
        }
    };
    Mat4::look_to_lh(eye, (focus - eye).normalize(), Vec3::Y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the eye position back out of a look-to-lh view matrix (inverse translation).
    fn eye_of(view: Mat4) -> Vec3 {
        view.inverse().transform_point3(Vec3::ZERO)
    }

    /// With no collision the eye sits behind (-Z, since yaw=0 looks +Z) and above the player, offset to
    /// the shoulder side — the retail over-the-shoulder framing.
    #[test]
    fn eye_sits_behind_and_above() {
        let view = third_person_view(Vec3::ZERO, 0.0, 0.0, &[]);
        let eye = eye_of(view);
        assert!(eye.z < 0.0, "eye should be behind a +Z-facing player, z = {}", eye.z);
        assert!(eye.y > 0.0, "eye should be raised toward the focus height, y = {}", eye.y);
        assert!(eye.x.abs() > 0.0, "eye should carry the lateral shoulder offset, x = {}", eye.x);
    }

    /// A wall between the focus and the desired eye retracts the boom: the eye ends up closer to the
    /// focus than the free-boom eye would be.
    #[test]
    fn boom_retracts_against_a_wall() {
        let focus = Vec3::Y * FOCUS_UP;
        let free_eye = eye_of(third_person_view(Vec3::ZERO, 0.0, 0.0, &[]));
        let free_dist = (free_eye - focus).length();

        // A wall ~1 m behind the focus (spanning the boom path), as two triangles at z = -1.
        let wall = vec![
            [Vec3::new(-5.0, -5.0, -1.0), Vec3::new(5.0, -5.0, -1.0), Vec3::new(5.0, 5.0, -1.0)],
            [Vec3::new(-5.0, -5.0, -1.0), Vec3::new(5.0, 5.0, -1.0), Vec3::new(-5.0, 5.0, -1.0)],
        ];
        let hit_eye = eye_of(third_person_view(Vec3::ZERO, 0.0, 0.0, &wall));
        let hit_dist = (hit_eye - focus).length();
        assert!(hit_dist < free_dist, "boom must pull in against the wall: {hit_dist} vs free {free_dist}");
        assert!(hit_dist >= MIN_BOOM - 1e-3, "boom must not retract past the minimum, got {hit_dist}");
    }
}
