//! Vehicle tuning block → actor-field map (`vehicle_code_map.md` §4).
//!
//! The retail exe loads a `_CarPhysicsV2` tuning block of **0x18c bytes / 99 dwords** (registrar
//! `FUN_0063e8b0`, ctor `FUN_00449460`) and a `_TankPhysics` block of 0x78 (`FUN_0063e980`). The
//! ctor scatters those dwords into the runtime actor fields:
//!
//! ```text
//!  [0x10] -> +0x180 MaxSpeed          [0x13] -> +0x184 MaxSpeedReverse
//!  [0x14..0x1d] front-wheel block: radius +0x50, susp strength/damp +0x54/+0x58,
//!              cmp/exp lengths +0x5c/+0x60, frictions fwd/side +0x64/+0x68, brake +0x6c..
//!  [0x1e..0x27] rear-wheel block (mirror +0x78..0x9c)
//!  [0x3f,0x40] -> +0x1e8/+0x1ec DonutBoost / DonutSidePower
//!  [0x48..0x4a] CenterOfMassOffset -> FUN_008d5290 setCenterOfMass
//!  [0x4c..0x5b] 16-dword gear/engine table @+0x218 (consumer not yet read — §5)
//! ```
//!
//! **The authored default VALUES are not yet decoded** (field names are stripped on PC; recover by
//! breaking `0x00449460` and diffing the block — code map §5). Everything below is a *plausible
//! placeholder* so the model runs; each is marked `CONFIRM-LIVE`.

use glam::Vec3;

/// Per-axle wheel tuning (front block `[0x14..0x1d]` / rear block `[0x1e..0x27]`).
#[derive(Clone, Copy, Debug)]
pub struct AxleTuning {
    /// Wheel radius (tuning `+0x50`).
    pub radius: f32,
    /// Suspension spring strength (`+0x54`).
    pub susp_strength: f32,
    /// Suspension damping (`+0x58`).
    pub susp_damp: f32,
    /// Suspension rest length — compressed/extended travel (`+0x5c/+0x60`).
    pub rest_length: f32,
    /// Forward (rolling) friction coefficient (`+0x64`).
    pub friction_fwd: f32,
    /// Lateral (cornering) friction coefficient (`+0x68`).
    pub friction_side: f32,
    /// Brake torque (`+0x6c`).
    pub brake_torque: f32,
    /// Drive torque delivered to this axle's wheels (from the engine/gear table; consumer unread).
    pub drive_torque: f32,
}

impl AxleTuning {
    // CONFIRM-LIVE: authored front-axle defaults (tuning dwords 0x14..0x1d).
    fn front_default() -> Self {
        Self {
            radius: 0.40,
            susp_strength: 30_000.0,
            susp_damp: 4_000.0,
            rest_length: 0.50,
            friction_fwd: 1.0,
            friction_side: 1.2,
            brake_torque: 3_000.0,
            drive_torque: 0.0, // FWD-off front by default; RWD car
        }
    }
    // CONFIRM-LIVE: authored rear-axle defaults (tuning dwords 0x1e..0x27).
    fn rear_default() -> Self {
        Self {
            radius: 0.42,
            susp_strength: 32_000.0,
            susp_damp: 4_200.0,
            rest_length: 0.50,
            friction_fwd: 1.1,
            friction_side: 1.1,
            brake_torque: 3_000.0,
            drive_torque: 900.0, // rear-wheel drive
        }
    }
}

/// The decoded (structurally) car/tank tuning. Values are placeholders (`CONFIRM-LIVE`); the field
/// *layout* mirrors the ctor scatter in §4.
#[derive(Clone, Copy, Debug)]
pub struct VehicleTuning {
    /// `+0x180` — top forward speed; the linear torque falloff drives to zero here.
    pub max_speed: f32,
    /// `+0x184` — top reverse speed.
    pub max_speed_reverse: f32,
    /// Falloff shaping constant `K` in `speedRatio = clamp01((vmax − v) / (vmax·K))`.
    /// `K = 1` ⇒ torque hits zero exactly at `vmax`. (CONFIRM-LIVE: exact K unread.)
    pub falloff_k: f32,
    /// Peak drive-force blend (`+0x178`).
    pub drive_blend: f32,
    /// `+0x1e8` DonutBoost — extra drive-force ramp while a donut is heating.
    pub donut_boost: f32,
    /// `+0x1ec` DonutSidePower — amplitude of the sine-LUT lateral wobble in donut mode.
    pub donut_side_power: f32,
    /// `[0x48..0x4a]` CenterOfMassOffset (local).
    pub com_offset: Vec3,
    /// Front axle block.
    pub front: AxleTuning,
    /// Rear axle block.
    pub rear: AxleTuning,
    /// Chassis mass (kg). CONFIRM-LIVE.
    pub mass: f32,
    /// Gravity acceleration (m/s²); the exe uses its own world gravity. CONFIRM-LIVE.
    pub gravity: f32,
    /// Max front-wheel steer angle (radians). CONFIRM-LIVE.
    pub max_steer: f32,
}

impl Default for VehicleTuning {
    /// Placeholder car tuning (RWD sedan-ish). CONFIRM-LIVE across the board.
    fn default() -> Self {
        Self {
            max_speed: 30.0,
            max_speed_reverse: 10.0,
            falloff_k: 1.0,
            drive_blend: 2.5,
            donut_boost: 1.5,
            donut_side_power: 6_000.0,
            com_offset: Vec3::new(0.0, -0.2, 0.1),
            front: AxleTuning::front_default(),
            rear: AxleTuning::rear_default(),
            mass: 1_200.0,
            gravity: -9.81,
            max_steer: 0.6,
        }
    }
}

impl VehicleTuning {
    /// Placeholder tank tuning (`_TankPhysics`, 0x78 block, loader `FUN_00659a80`). Heavier, slower,
    /// both tracks powered, no Ackermann steer (differential). CONFIRM-LIVE.
    pub fn tank_default() -> Self {
        let mut t = Self {
            max_speed: 14.0,
            max_speed_reverse: 7.0,
            drive_blend: 14.0,
            mass: 30_000.0,
            max_steer: 0.0, // tank steers by track differential, not a steer angle
            ..Self::default()
        };
        t.front.drive_torque = 4_000.0;
        t.rear.drive_torque = 4_000.0;
        t.front.friction_side = 2.5; // tracks resist sideways strongly
        t.rear.friction_side = 2.5;
        t
    }
}
