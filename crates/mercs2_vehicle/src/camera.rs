//! Vehicle / on-foot camera modes (`camera_code_map.md` §4).
//!
//! Camera **modes are data, not code branches**: each controlled object carries a mode-specific ECS
//! component (`CameraCarPreset` `FUN_006401b0` / `CameraTank` / `CameraTurret` / `CameraHelicopter`
//! + `HumanCameraModifier`). At runtime the engine selects the mode by the currently-ridden object
//! (`(*(owner+0x2c))()` returns 4 = a vehicle chase mode, 3 = on-foot — the same "which vehicle am I
//! in" resolution the seat/ride rings produce). We reproduce: the ridden vehicle's [`VehicleClass`]
//! picks the mode; the mode's preset + the look axis yield a chase pose.
//!
//! The preset float LAYOUT and the exact look-axis apply / pitch clamp are string-stripped on PC →
//! `CONFIRM-LIVE` (code map §4/§5). Values here are structural placeholders.

use glam::{Quat, Vec3};

use crate::components::VehicleClass;

/// The per-vehicle camera mode selected by the ridden object (`camera_code_map.md` §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    /// `(*(owner+0x2c))() == 3` — on-foot (`HumanCameraModifier`).
    OnFoot,
    /// `CameraCarPreset` chase cam (cars, bikes).
    Car,
    /// `CameraTank` (pitch/yaw readouts, `Tank Camera`).
    Tank,
    /// `CameraTurret` (`CamOffset`, pitch/yaw clamp).
    Turret,
    /// `CameraHelicopter` (`CamDistToHeli`).
    Helicopter,
    /// Boat chase (shares the car-style preset shape).
    Boat,
}

impl CameraMode {
    /// The mode a vehicle of `class` selects when it is the ridden object (`== 4` vehicle-chase
    /// branch of `FUN_0060f6d0`); the on-foot `== 3` branch is [`CameraMode::OnFoot`].
    pub fn for_class(class: VehicleClass) -> Self {
        match class {
            VehicleClass::Car | VehicleClass::Bike | VehicleClass::Jet => CameraMode::Car,
            VehicleClass::Tank => CameraMode::Tank,
            VehicleClass::Helicopter => CameraMode::Helicopter,
            VehicleClass::Boat => CameraMode::Boat,
        }
    }
}

/// A chase-cam preset — the bag of tunables `camera.md` lists (`CameraOffset`, `CameraPitchOffset`,
/// `CameraYawOffset`, `CameraZoomOffset`, `CameraBlendTime`, `DefaultFov`/`MaxFov`/`FovMaxSpeed`).
/// One ECS component per mode (`CameraCarPreset` `0x50`, `CameraTank` `0xc0`, …). Values CONFIRM-LIVE.
#[derive(Clone, Copy, Debug)]
pub struct CameraPreset {
    /// `CameraOffset` — chase offset in the vehicle's local frame (behind + above).
    pub offset: Vec3,
    /// `CameraPitchOffset` — base downward tilt (radians).
    pub pitch_offset: f32,
    /// `CameraYawOffset` — base yaw bias (radians).
    pub yaw_offset: f32,
    /// `CameraZoomOffset` — extra pull-back along the view axis.
    pub zoom_offset: f32,
    /// `CameraBlendTime` — seconds to blend into this preset (`CameraBlendTime @825109e0`).
    pub blend_time: f32,
    /// `DefaultFov` (radians).
    pub default_fov: f32,
    /// `MaxFov` (radians) — the speed-widened cap.
    pub max_fov: f32,
    /// `FovMaxSpeed` — speed at which FOV reaches `max_fov`.
    pub fov_max_speed: f32,
    /// Look-height above the vehicle origin the camera aims at.
    pub look_height: f32,
}

impl CameraPreset {
    /// Placeholder car chase preset. CONFIRM-LIVE.
    pub fn car() -> Self {
        Self {
            offset: Vec3::new(0.0, 2.2, -6.0),
            pitch_offset: 0.15,
            yaw_offset: 0.0,
            zoom_offset: 0.0,
            blend_time: 0.4,
            default_fov: 1.0472,      // 60°
            max_fov: 1.2217,          // 70°
            fov_max_speed: 30.0,
            look_height: 1.2,
        }
    }
    /// Placeholder tank preset — higher, further back, wider. CONFIRM-LIVE.
    pub fn tank() -> Self {
        Self {
            offset: Vec3::new(0.0, 3.5, -8.5),
            pitch_offset: 0.22,
            look_height: 1.8,
            ..Self::car()
        }
    }
    /// Placeholder helicopter preset. CONFIRM-LIVE.
    pub fn helicopter() -> Self {
        Self {
            offset: Vec3::new(0.0, 3.0, -10.0),
            pitch_offset: 0.1,
            look_height: 0.0,
            ..Self::car()
        }
    }
    /// Placeholder turret preset (`CamOffset`, tight). CONFIRM-LIVE.
    pub fn turret() -> Self {
        Self {
            offset: Vec3::new(0.0, 1.5, -3.5),
            look_height: 1.4,
            ..Self::car()
        }
    }

    pub fn for_mode(mode: CameraMode) -> Self {
        match mode {
            CameraMode::Tank => Self::tank(),
            CameraMode::Helicopter => Self::helicopter(),
            CameraMode::Turret => Self::turret(),
            _ => Self::car(),
        }
    }
}

/// The resolved camera pose a mode produces this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPose {
    /// World-space eye position.
    pub position: Vec3,
    /// World-space point the camera looks at.
    pub target: Vec3,
    /// Vertical field of view (radians).
    pub fov: f32,
}

/// Compute the chase pose for a vehicle at `vehicle_pos`/`vehicle_rot`, given the player's look
/// axis (`look_yaw`/`look_pitch`, radians — right-stick / mouse delta accumulated by the input
/// layer) and the vehicle's forward `speed` (for the FOV widening).
///
/// Mirrors the mode-cluster apply (§4/§5): orbit the preset offset by (vehicle yaw + look yaw),
/// tilt by (preset pitch + look pitch, clamped), pull back by the zoom, aim at the vehicle + look
/// height, widen FOV with speed. The exact per-axis math is `CONFIRM-LIVE` (string-stripped).
pub fn chase_pose(
    preset: &CameraPreset,
    vehicle_pos: Vec3,
    vehicle_rot: Quat,
    look_yaw: f32,
    look_pitch: f32,
    speed: f32,
) -> CameraPose {
    // Orbit basis: vehicle yaw composed with the look yaw/pitch, about world up / camera right.
    let yaw = preset.yaw_offset + look_yaw;
    // CONFIRM-LIVE: pitch clamp band (Xbox `WARNING [PitchMin > PitchMax]`); placeholder ±80°.
    let pitch = (preset.pitch_offset + look_pitch).clamp(-1.4, 1.4);

    let orbit = vehicle_rot * Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    let mut eye_offset = orbit * preset.offset;
    // Extra pull-back along the offset direction.
    if preset.zoom_offset != 0.0 {
        eye_offset += eye_offset.normalize_or_zero() * preset.zoom_offset;
    }
    let position = vehicle_pos + eye_offset;
    let target = vehicle_pos + Vec3::Y * preset.look_height;

    // Speed-tied FOV widening (`FovMaxSpeed` → `MaxFov`).
    let t = (speed.abs() / preset.fov_max_speed.max(1e-3)).clamp(0.0, 1.0);
    let fov = preset.default_fov + (preset.max_fov - preset.default_fov) * t;

    CameraPose { position, target, fov }
}
