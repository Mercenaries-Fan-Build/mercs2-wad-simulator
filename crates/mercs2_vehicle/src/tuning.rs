//! Vehicle tuning block → actor-field map (`vehicle_code_map.md` §4), with the **recovered**
//! `_CarPhysicsV2` / `_TankPhysics` reflection schema-default values.
//!
//! The retail exe loads a `_CarPhysicsV2` tuning block of **0x18c bytes / 99 dwords** (registrar
//! `FUN_0063e8b0`, ctor `FUN_00449460`) and a `_TankPhysics` block of 0x78 / 30 dwords
//! (`FUN_0063e980`). The ctor scatters those dwords into the runtime actor fields:
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
//! ## How the default VALUES were recovered (this pass)
//! Field NAMES are stripped on PC, but the **field-by-field stream loaders** carry the authored
//! defaults as call arguments: `FUN_00658f60` (Car, 99 dwords) and `FUN_00659a80` (Tank, 30 dwords)
//! call `FUN_00656320(default)` per float / `FUN_00656610(&default)` per vec3, **in stream order**.
//! Those default args are either immediates (`0x3f800000` = 1.0) or `.data` globals (`DAT_00xxxxxx`).
//! We resolved every global by reading the unpacked-exe memory image
//! (`output/_ghidra/securom_dump/image.bin`, `file_offset = VA − 0x400000` — the validated method,
//! cross-checked against the independently-decoded `CameraCarPreset` scalars 55/65/150/0.1).
//!
//! The self-consistency anchor: the CoM vec3 (`FUN_00656610`) is the **73rd** loader field, landing
//! at stream dwords 72..74 = `[0x48..0x4a]` — exactly the scatter-map CenterOfMassOffset slot. That
//! pins loader-call-index == stream-dword-index, so every scatter-map `[idx]` reads directly off the
//! loader. See [`schema`] for the full recovered dword table.
//!
//! ## What is still CONFIRM-LIVE
//! - **Engine → model UNIT bridge.** The recovered numbers are in the engine's own units (e.g.
//!   MaxSpeed `150.0`, suspension `50.0`/`40.1`). The approximation drive model in [`crate::drive`]
//!   runs in SI-ish m/s / kg / N·m, so speed / suspension / friction / brake fields keep a
//!   model-scale value whose doc cites the exact recovered engine-unit default; the unit conversion
//!   itself needs a live confirm (`bp 0x0044a970`, read `+0x180` vs actual chassis m/s).
//! - **Intra-wheel-block field labelling.** The recovered front block is
//!   `[50.0, 0.42, 40.1, 3.0, 0.2, 0.2, 0.5, 1.5, 2.0, 1.0]` and the rear is identical except the
//!   first slot (`0.1`). `0.42` is the physically-sane wheel radius, i.e. the scatter map's
//!   "radius at block[0]" is likely off by one — but only block[0] differs front↔rear (a
//!   steer/handling per-axle scalar), so the exact per-slot names are the live confirm.
//! - **Gear/engine table** ([0x4c..0x5b]) is **all-zero in the schema** — per-vehicle drive torque
//!   is authored in the per-vehicle WAD stream, not the schema default, so a drivable default here
//!   is an explicit per-vehicle stand-in.
//! - **`_TankPhysics` field map** — no §4 scatter map exists for the tank block; [`schema::TANK_BLOCK`]
//!   records the raw recovered values, field identity inferred.

use glam::Vec3;

/// The **recovered** `_CarPhysicsV2` / `_TankPhysics` reflection schema defaults (exact values, in
/// the engine's own units). Source: field-by-field loaders `FUN_00658f60` / `FUN_00659a80`, defaults
/// resolved from `image.bin`. These are the *schema* fallbacks; per-vehicle WAD blocks override
/// individual fields at load. See the module docs for the recovery method + provenance.
pub mod schema {
    /// `_CarPhysicsV2` block — 99 dwords, stream order (== ctor `FUN_00449460` scatter index). Floats
    /// only (the 3 trailing enum fields are omitted). Verified against the scatter-map anchor at
    /// `[0x48..0x4a]` (CenterOfMassOffset vec3). A handful of trailing gear-table slots are `0.0`.
    pub const CAR_BLOCK: [f32; 92] = [
        /* 0x00 */ 1500.0, 1.0, 1.0, 0.0, 0.9, -1.0, 10.0, 5.0, // idx0=chassis mass
        /* 0x08 */ 30.0, 0.1, 0.5, 0.8, 0.0, 0.5, 0.5, 0.5, //
        /* 0x10 */ 150.0, 500.0, 1.0, 0.8, // [0x10]=MaxSpeed  [0x13]=MaxSpeedReverse
        /* 0x14 front wheel block */ 50.0, 0.42, 40.1, 3.0, 0.2, 0.2, 0.5, 1.5, 2.0, 1.0,
        /* 0x1e rear wheel block  */ 0.1, 0.42, 40.1, 3.0, 0.2, 0.2, 0.5, 1.5, 2.0, 1.0,
        /* 0x28 */ 0.8, 8.0, 1.0, 3.0, 2.25, 1.5, 0.75, 0.4, //
        /* 0x30 */ 8000.0, 1000.0, 5000.0, 7000.0, 2000.0, 2000.0, 1.0, 1.0, //
        /* 0x38 */ 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, //
        /* 0x3f */ 1.0, 8.0, // [0x3f]=DonutBoost  [0x40]=DonutSidePower
        /* 0x41 */ 5.0, 10.0, 2.0, 0.0, 2.0, 1.0, 1.5, //
        /* 0x48 CenterOfMassOffset vec3 */ 0.0, 0.0, 0.0,
        /* 0x4b */ 0.0,
        /* 0x4c gear/engine table (16 dwords, all schema-zero) */ 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];

    /// `_TankPhysics` block — 30 dwords, stream order (loader `FUN_00659a80`). No §4 scatter map;
    /// field identity is inferred. idx19..21 is the vec3 (CoM-analogue), recorded as `0,0,0`.
    pub const TANK_BLOCK: [f32; 30] = [
        -1.0, 40.0, 20.0, 15.0, 0.0, 0.0, 0.4, 9.0, 1.0, 30.0, //
        0.17, 0.3, -0.25, 2.0, 0.0, 1.0, 0.5, 1.0, 5.0, //
        0.0, 0.0, 0.0, // vec3
        1000.0, 0.95, 1.0, 0.55, 0.55, 0.1, 0.25, 0.5,
    ];

    // --- Scatter-mapped named car fields (recovered engine-unit defaults) ---
    /// `[0x10]` → `+0x180` MaxSpeed (engine speed units).
    pub const CAR_MAX_SPEED: f32 = 150.0;
    /// `[0x13]` → `+0x184` MaxSpeedReverse.
    pub const CAR_MAX_SPEED_REVERSE: f32 = 0.8;
    /// `[0x3f]` → `+0x1e8` DonutBoost.
    pub const CAR_DONUT_BOOST: f32 = 1.0;
    /// `[0x40]` → `+0x1ec` DonutSidePower.
    pub const CAR_DONUT_SIDE_POWER: f32 = 8.0;
    /// idx0 — chassis mass (kg), recovered.
    pub const CAR_MASS: f32 = 1500.0;
    /// The recovered wheel radius (front/rear wheel-block slot [1] = `0.42`; the physically-sane
    /// value in a block whose slot [0] differs front↔rear).
    pub const CAR_WHEEL_RADIUS: f32 = 0.42;
}

/// Per-axle wheel tuning (front block `[0x14..0x1d]` / rear block `[0x1e..0x27]`).
#[derive(Clone, Copy, Debug)]
pub struct AxleTuning {
    /// Wheel radius (recovered wheel-block slot, `schema::CAR_WHEEL_RADIUS` = 0.42).
    pub radius: f32,
    /// Suspension spring strength (`+0x54`). CONFIRM-LIVE unit bridge: recovered engine-unit default
    /// = `50.0` (front) / `0.1` (rear block[0]); model uses an N/m-scale value.
    pub susp_strength: f32,
    /// Suspension damping (`+0x58`). Recovered engine-unit default = `40.1`; model uses an N·s/m scale.
    pub susp_damp: f32,
    /// Suspension rest length (`+0x5c/+0x60`). Recovered cmp/exp block slots = `3.0` / `0.2`.
    pub rest_length: f32,
    /// Forward (rolling) friction coefficient (`+0x64`). Recovered block slot = `0.2`.
    pub friction_fwd: f32,
    /// Lateral (cornering) friction coefficient (`+0x68`). Recovered block slot = `0.5`.
    pub friction_side: f32,
    /// Brake torque (`+0x6c`). Recovered block slot = `1.5` (engine units); model uses an N·m scale.
    pub brake_torque: f32,
    /// Drive torque delivered to this axle's wheels. The schema gear/engine table `[0x4c..0x5b]` is
    /// **all-zero** — real per-axle torque is authored per-vehicle in the WAD stream, so this is an
    /// explicit per-vehicle stand-in (`// CONFIRM-LIVE:` per-vehicle WAD / live).
    pub drive_torque: f32,
}

impl AxleTuning {
    /// Front axle. `radius`/`friction`/`rest` from the recovered front wheel block `[0x14..0x1d]`;
    /// suspension/brake keep a model-scale magnitude (recovered engine-unit defaults noted per field).
    fn front_default() -> Self {
        Self {
            radius: schema::CAR_WHEEL_RADIUS, // recovered 0.42
            susp_strength: 30_000.0,          // CONFIRM-LIVE unit: recovered engine default 50.0
            susp_damp: 4_000.0,               // CONFIRM-LIVE unit: recovered engine default 40.1
            rest_length: 0.50,                // recovered cmp/exp block 3.0 / 0.2 (engine units)
            friction_fwd: 0.2,                // recovered block slot
            friction_side: 0.5,               // recovered block slot ([0x1a])
            brake_torque: 3_000.0,            // CONFIRM-LIVE unit: recovered engine default 1.5
            drive_torque: 0.0,                // RWD: front unpowered
        }
    }
    /// Rear axle. Recovered rear block `[0x1e..0x27]` is identical to front except block slot [0].
    fn rear_default() -> Self {
        Self {
            radius: schema::CAR_WHEEL_RADIUS,
            susp_strength: 32_000.0, // CONFIRM-LIVE unit: recovered engine default (rear block[0] 0.1)
            susp_damp: 4_200.0,      // CONFIRM-LIVE unit: recovered engine default 40.1
            rest_length: 0.50,
            friction_fwd: 0.2,
            friction_side: 0.5,
            brake_torque: 3_000.0,
            // Gear/engine table is schema-zero (§4/§5); real value is per-vehicle WAD.
            // `// CONFIRM-LIVE:` per-vehicle drive torque — stand-in so the RWD model drives.
            drive_torque: 900.0,
        }
    }
}

/// The car/tank tuning. Field *layout* mirrors the ctor scatter in §4; VALUES are the recovered
/// `_CarPhysicsV2` schema defaults where the unit is model-compatible, else a model-scale value whose
/// doc cites the exact recovered engine-unit default (unit bridge = the remaining confirm-live).
#[derive(Clone, Copy, Debug)]
pub struct VehicleTuning {
    /// `+0x180` — top forward speed. Recovered `_CarPhysicsV2` schema default = `schema::CAR_MAX_SPEED`
    /// (150.0, engine units). The model runs in m/s and keeps a 30 m/s cap pending the live unit
    /// bridge (`// CONFIRM-LIVE:` unit of the recovered MaxSpeed).
    pub max_speed: f32,
    /// `+0x184` — top reverse speed. Recovered schema default = `schema::CAR_MAX_SPEED_REVERSE` (0.8;
    /// likely a ratio of forward — CONFIRM-LIVE). Model uses a m/s reverse cap.
    pub max_speed_reverse: f32,
    /// Falloff shaping constant `K` in `speedRatio = clamp01((vmax − v) / (vmax·K))`.
    /// `K = 1` ⇒ torque hits zero exactly at `vmax`. (CONFIRM-LIVE: exact K unread.)
    pub falloff_k: f32,
    /// Peak drive-force blend (`+0x178`). CONFIRM-LIVE: not in the §4 scatter map.
    pub drive_blend: f32,
    /// `+0x1e8` DonutBoost — recovered schema default `schema::CAR_DONUT_BOOST` (1.0).
    pub donut_boost: f32,
    /// `+0x1ec` DonutSidePower — recovered schema default `schema::CAR_DONUT_SIDE_POWER` (8.0).
    pub donut_side_power: f32,
    /// `[0x48..0x4a]` CenterOfMassOffset (local). Recovered schema default = `(0,0,0)`.
    pub com_offset: Vec3,
    /// Front axle block.
    pub front: AxleTuning,
    /// Rear axle block.
    pub rear: AxleTuning,
    /// Chassis mass (kg). Recovered `_CarPhysicsV2` idx0 = `schema::CAR_MASS` (1500.0).
    pub mass: f32,
    /// Gravity acceleration (m/s²); the exe uses its own world gravity. CONFIRM-LIVE.
    pub gravity: f32,
    /// Max front-wheel steer angle (radians). CONFIRM-LIVE (SteerMaxAngle is a per-vehicle field).
    pub max_steer: f32,
}

impl Default for VehicleTuning {
    /// Car tuning. Recovered schema defaults where unit-compatible (mass, CoM, donut, wheel radius);
    /// speed/suspension/friction/brake carry the recovered engine-unit default in their doc + a
    /// model-scale magnitude pending the live unit bridge.
    fn default() -> Self {
        Self {
            // Recovered schema default = 150.0 engine units; model m/s cap (CONFIRM-LIVE unit).
            max_speed: 30.0,
            max_speed_reverse: 10.0,
            falloff_k: 1.0,
            drive_blend: 2.5,
            donut_boost: schema::CAR_DONUT_BOOST,           // recovered 1.0
            donut_side_power: schema::CAR_DONUT_SIDE_POWER, // recovered 8.0
            com_offset: Vec3::ZERO,                         // recovered (0,0,0)
            front: AxleTuning::front_default(),
            rear: AxleTuning::rear_default(),
            mass: schema::CAR_MASS, // recovered 1500.0
            gravity: -9.81,
            max_steer: 0.6,
        }
    }
}

impl VehicleTuning {
    /// Tank tuning (`_TankPhysics`, 0x78 block, loader `FUN_00659a80`). The recovered raw block is
    /// [`schema::TANK_BLOCK`]; with no §4 scatter map the per-field identity is inferred, so the tank
    /// keeps model-scale handling (heavier, slower, both tracks powered, no Ackermann steer) sourced
    /// against the recovered block magnitudes. CONFIRM-LIVE: tank field map.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The recovered scatter-mapped car fields land at their documented values (a guard so a future
    /// edit can't silently drop the recovered `_CarPhysicsV2` schema defaults).
    #[test]
    fn recovered_car_schema_defaults() {
        assert_eq!(schema::CAR_BLOCK[0x10], schema::CAR_MAX_SPEED, "MaxSpeed [0x10]");
        assert_eq!(schema::CAR_BLOCK[0x13], schema::CAR_MAX_SPEED_REVERSE, "MaxSpeedReverse [0x13]");
        assert_eq!(schema::CAR_BLOCK[0x3f], schema::CAR_DONUT_BOOST, "DonutBoost [0x3f]");
        assert_eq!(schema::CAR_BLOCK[0x40], schema::CAR_DONUT_SIDE_POWER, "DonutSidePower [0x40]");
        assert_eq!(schema::CAR_BLOCK[0], schema::CAR_MASS, "chassis mass idx0");
        // CenterOfMassOffset vec3 lands at the scatter-map anchor [0x48..0x4a] = (0,0,0).
        assert_eq!(
            [schema::CAR_BLOCK[0x48], schema::CAR_BLOCK[0x49], schema::CAR_BLOCK[0x4a]],
            [0.0, 0.0, 0.0],
            "CoM vec3 anchor"
        );
        // Gear/engine table [0x4c..0x5b] is schema-zero.
        assert!(schema::CAR_BLOCK[0x4c..0x5c].iter().all(|&x| x == 0.0), "gear table all-zero");
        // The tuning consumes the recovered donut + CoM + mass values.
        let d = VehicleTuning::default();
        assert_eq!(d.donut_boost, 1.0);
        assert_eq!(d.donut_side_power, 8.0);
        assert_eq!(d.com_offset, Vec3::ZERO);
        assert_eq!(d.mass, 1500.0);
        assert_eq!(d.front.radius, 0.42);
    }
}
