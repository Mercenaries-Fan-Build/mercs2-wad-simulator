//! Mode-based camera rig — mirrors Mercs2's reflection-driven camera.
//!
//! The retail engine picks a camera **mode** from the object the player is riding: on foot uses the
//! `HumanCameraModifier`, and each vehicle class swaps in its own preset component — `CameraCarPreset`,
//! `CameraTank`, `CameraTurret`, `CameraHelicopter` (all present as reflection classes in the exe). A
//! single hardcoded over-the-shoulder offset is the wrong shape; this module models the real one: a
//! `CameraPreset` (the reflected `CameraOffset*` / FOV / near-far field set) selected by `CameraMode`,
//! plus the boom-collision math that is pure engine mechanism.
//!
//! ## Every preset value is reverse-engineered, not invented
//! (see `docs/reverse_engineer/camera_code_map.md` + memory `camera-live-capture-and-mode-system`)
//!  - **On-foot offset + rest pitch:** live x32dbg capture of the *paused* PMC-interior on-foot camera —
//!    eye ≈ up +2.1, back −2.2, ~0 side; rest pitch ≈ −7.5° (from the owner-record quat @+0x54).
//!  - **near / far:** the game's own Lua — `Graphics.Camera.SetNearFar(0, 0.3, 500, 0)`
//!    (`wifpmcinterior.lua`), i.e. near 0.3 / far 500 in the PMC interior.
//!  - **On-foot blend + weights:** `HumanCameraModifier` reflection-schema defaults read from the exe
//!    `.rdata` (`FUN_0065eaf0`): blend `DAT_00bbb99c` = 0.5; weight scalars 0.6 / 0.75.
//!  - **Car FOV / blend / far:** `CameraCarPreset` schema defaults (`FUN_0065e1d0`): f11 = 55 & f12 = 65
//!    (FOV min/max, deg), f8/f10 = 0.1 (blend), f13 = 150 (far). Its three boom **offset vec3s** are a
//!    reflection *stream* default that lives in per-vehicle WAD component data, not an exe literal — so
//!    the vehicle offsets are pinned by a **live trace while driving** (same method as on-foot), NOT
//!    guessed. Until that trace, vehicle modes fall back to the on-foot geometry rather than shipping
//!    fabricated numbers.

use mercs2_core::glam::{Mat4, Vec3};

/// One camera mode's reflected field set. Names mirror the engine reflection vocabulary found in the
/// exe string table (`CameraOffset`, `CameraPitchOffset`, `CameraBlendTime`, `CameraZoomOffset`, …).
#[derive(Clone, Copy, Debug)]
pub struct CameraPreset {
    /// `CameraOffset` — eye offset from the followed origin in the follow frame:
    /// `x` = lateral (shoulder, +right), `y` = up (focus height), `z` = back (boom, behind the subject).
    pub offset: Vec3,
    /// `CameraPitchOffset` — rest pitch bias added to the look pitch, radians (down is negative).
    pub pitch_offset: f32,
    /// `CameraBlendTime` — seconds to blend when this mode becomes active.
    pub blend_time: f32,
    /// Vertical FOV, degrees.
    pub fov: f32,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
}

impl CameraPreset {
    /// On-foot (`HumanCameraModifier`). Offset + rest pitch from the live PMC capture; near/far from the
    /// PMC-interior Lua `SetNearFar(0, 0.3, 500, 0)`; blend from the schema default `DAT_00bbb99c` = 0.5.
    /// Lateral offset is 0 as measured (the paused capture had ~0 yaw; a shoulder offset, if any, awaits
    /// a yaw-varied capture — not invented here).
    pub const ON_FOOT: CameraPreset = CameraPreset {
        offset: Vec3::new(0.0, 2.1, 2.2),
        pitch_offset: -0.1309, // −7.5° rest pitch (live owner-record quat)
        blend_time: 0.5,       // HumanCameraModifier DAT_00bbb99c
        fov: 55.0,             // shared default FOV (CameraCarPreset f11 = 55)
        near: 0.3,             // wifpmcinterior.lua SetNearFar
        far: 500.0,
    };
}

/// The camera mode, selected by the object the player is riding (on foot → `OnFoot`). Mirrors the
/// per-class reflection presets the engine swaps in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    OnFoot,
    Car,
    Tank,
    Turret,
    Helicopter,
}

impl CameraMode {
    /// Select the mode from the reflection class name of the object the player is riding. `None`
    /// (nothing ridden) → on foot. Names match the exe camera-preset class strings.
    pub fn for_ridden(ridden_class: Option<&str>) -> CameraMode {
        match ridden_class {
            None => CameraMode::OnFoot,
            Some(c) if c.contains("Tank") => CameraMode::Tank,
            Some(c) if c.contains("Turret") => CameraMode::Turret,
            Some(c) if c.contains("Helicopter") || c.contains("Heli") => CameraMode::Helicopter,
            // Any other ridden vehicle uses the car preset (the generic CameraCarPreset).
            Some(_) => CameraMode::Car,
        }
    }

    /// The reflected preset for this mode. Only `OnFoot` is RE-pinned today; vehicle presets await a
    /// live drive-trace (their boom offsets live in per-vehicle WAD data) and reuse the on-foot geometry
    /// until then — a placeholder that is honest RE-sourced data, never a fabricated vehicle offset.
    pub fn preset(self) -> CameraPreset {
        match self {
            CameraMode::OnFoot => CameraPreset::ON_FOOT,
            // TODO(live-trace): pin CameraCarPreset / CameraTank / CameraTurret / CameraHelicopter boom
            // offsets from a paused capture while riding, exactly as on-foot was pinned.
            _ => CameraPreset::ON_FOOT,
        }
    }
}

// --- Boom-collision mechanism (engine "how", not a reflected gameplay value) ---
/// Camera collision radius — the boom stops this far short of the nearest wall.
const CAM_RADIUS: f32 = 0.35;
/// Minimum eye distance the boom may retract to (so the eye never ends up inside the focus).
const MIN_BOOM: f32 = 0.6;

/// The over-the-shoulder view matrix for a follow camera at `player_pos` looking at (`yaw`, `pitch`),
/// using `preset`'s reflected offset + rest pitch. The eye sits a boom behind + above the focus; when
/// `collision` is non-empty the boom is cast from the focus toward the desired eye and pulled in to just
/// short of the nearest wall. `yaw = 0` looks toward +Z with the eye on the −Z side (behind a +Z player).
pub fn view_with_preset(
    preset: &CameraPreset,
    player_pos: Vec3,
    yaw: f32,
    pitch: f32,
    collision: &[[Vec3; 3]],
) -> Mat4 {
    let p = pitch + preset.pitch_offset;
    let dir = Vec3::new(p.cos() * yaw.sin(), p.sin(), p.cos() * yaw.cos()).normalize();
    let focus = player_pos + Vec3::Y * preset.offset.y;
    let right = Vec3::Y.cross(dir).normalize();
    let want_eye = focus - dir * preset.offset.z + right * preset.offset.x;
    let eye = if collision.is_empty() {
        want_eye
    } else {
        let boom_vec = want_eye - focus;
        let boom_len = boom_vec.length();
        let boom_dir = boom_vec / boom_len;
        match crate::physics::soup::raycast(collision, focus, boom_dir, boom_len) {
            Some(hit) => focus + boom_dir * (hit - CAM_RADIUS).max(MIN_BOOM),
            None => want_eye,
        }
    };
    Mat4::look_to_lh(eye, (focus - eye).normalize(), Vec3::Y)
}

/// Back-compat convenience: the on-foot third-person view (the `CameraMode::OnFoot` preset).
pub fn third_person_view(player_pos: Vec3, yaw: f32, pitch: f32, collision: &[[Vec3; 3]]) -> Mat4 {
    view_with_preset(&CameraPreset::ON_FOOT, player_pos, yaw, pitch, collision)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the eye position back out of a look-to-lh view matrix (inverse translation).
    fn eye_of(view: Mat4) -> Vec3 {
        view.inverse().transform_point3(Vec3::ZERO)
    }

    /// With no collision the eye sits behind (−Z, since yaw=0 looks +Z) and above the player — the
    /// over-the-shoulder framing from the on-foot preset.
    #[test]
    fn eye_sits_behind_and_above() {
        let view = third_person_view(Vec3::ZERO, 0.0, 0.0, &[]);
        let eye = eye_of(view);
        assert!(eye.z < 0.0, "eye should be behind a +Z-facing player, z = {}", eye.z);
        assert!(eye.y > 0.0, "eye should be raised toward the focus height, y = {}", eye.y);
    }

    /// The on-foot preset carries the RE-sourced PMC values (near/far from Lua, blend from the schema
    /// default) — a guard so a future edit can't silently swap in invented numbers.
    #[test]
    fn on_foot_preset_is_re_sourced() {
        let p = CameraPreset::ON_FOOT;
        assert_eq!(p.near, 0.3, "PMC near from wifpmcinterior.lua SetNearFar");
        assert_eq!(p.far, 500.0, "PMC far from wifpmcinterior.lua SetNearFar");
        assert_eq!(p.blend_time, 0.5, "HumanCameraModifier DAT_00bbb99c");
        assert!((p.pitch_offset + 0.1309).abs() < 1e-3, "−7.5° live rest pitch");
    }

    /// Nothing ridden → on foot; a `Tank`-class ridden object selects the tank mode.
    #[test]
    fn mode_selection_by_ridden_class() {
        assert_eq!(CameraMode::for_ridden(None), CameraMode::OnFoot);
        assert_eq!(CameraMode::for_ridden(Some("CameraTank")), CameraMode::Tank);
        assert_eq!(CameraMode::for_ridden(Some("some_apc")), CameraMode::Car);
    }

    /// A wall between the focus and the desired eye retracts the boom: the eye ends up closer to the
    /// focus than the free-boom eye would be, but never past the minimum.
    #[test]
    fn boom_retracts_against_a_wall() {
        let focus = Vec3::Y * CameraPreset::ON_FOOT.offset.y;
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
