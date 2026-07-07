//! Vehicle ECS components (defined in **this** crate per the silo carve rule).
//!
//! These mirror the retail `hkpUnaryAction`-derived actor runtime layout (`vehicle_code_map.md`
//! §3/§4). An in-world vehicle entity carries: [`Vehicle`] (class tag), [`ChassisBody`] (the chassis
//! rigid body = `hkpUnaryAction.m_entity`), [`WheelSet`] (the pool-allocated `_CarWheel` 0x130
//! objects), [`VehicleControls`] (the drive-obj input fields `+0x28..`), [`VehicleRuntime`]
//! (per-frame scratch: fwd speed, donut heat, round-robin index…), the [`VehicleTuning`], and
//! [`Seating`] (ring-1 seat/enter-exit state).

use glam::{Quat, Vec3};
use hecs::Entity;

pub use crate::tuning::VehicleTuning;

/// Which of the nine actor classes this vehicle simulates as (`vehicle_code_map.md` §3).
/// `Car`/`Tank` are simulated here; the rest are recognised (for the command-ring routing +
/// camera-mode selection) and land in later passes (see `DEFERRED.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleClass {
    /// `CarPhysicsV2` — actor C, applyAction `FUN_0044db60`.
    Car,
    /// 2-wheel car (`numWheels==2` → bike variant `FUN_00437260`).
    Bike,
    /// `TankPhysics` — actor G, applyAction `FUN_00454d80`.
    Tank,
    /// `BoatPhysics` — actor A, applyAction `FUN_00447260`.
    Boat,
    /// `HelicopterPhysics` — actor B, applyAction `FUN_00453760`.
    Helicopter,
    /// `JetPhysics` — flies kinematically (no dedicated action).
    Jet,
}

impl VehicleClass {
    /// True for the classes whose control records travel on the **car/tank ring** (`0x011C0230`,
    /// cap 0x200); the rest ride the **boat/heli ring** (`0x011C2478`, cap 100). Turret is a
    /// separate ring. (§1.2 table.)
    pub fn uses_car_ring(self) -> bool {
        matches!(self, VehicleClass::Car | VehicleClass::Bike | VehicleClass::Tank)
    }
}

/// Vehicle tag component: the class + a stable gameplay handle (the Lua GUID / ring target id).
#[derive(Clone, Copy, Debug)]
pub struct Vehicle {
    pub class: VehicleClass,
    /// Gameplay handle used as the command-ring record `+0` target and the Lua `uGuid`.
    pub handle: u32,
}

impl Vehicle {
    pub fn new(class: VehicleClass, handle: u32) -> Self {
        Self { class, handle }
    }
}

/// The chassis rigid body — `hkpUnaryAction.m_entity` (`+0x18`), with the motion fields the
/// applyAction path touches: linvel `+0x1a0`, angvel `+0x1b0`, gravityFactor `+0x84`, and the
/// motion vtbl setLinVel/applyImpulse ops. We integrate it ourselves (the physics silo owns the
/// full solver; we only need a chassis + the `PhysicsQuery` wheel rays).
#[derive(Clone, Copy, Debug)]
pub struct ChassisBody {
    /// World-space linear velocity (`m_entity+0x1a0`).
    pub linvel: Vec3,
    /// World-space angular velocity (`m_entity+0x1b0`).
    pub angvel: Vec3,
    pub mass: f32,
    pub inv_mass: f32,
    /// Scalar inertia approximation (uniform). CONFIRM-LIVE: the exe carries a full inertia tensor
    /// on the rigid body; a scalar is a modern simplification, adequate for the chase-cam / drive
    /// feel and marked as such.
    pub inv_inertia: f32,
    /// `m_entity+0x84` gravityFactor — flipped by applyAction when the chassis is upside-down
    /// (`up.y < 0`, `FUN_0044db60`).
    pub gravity_factor: f32,
}

impl ChassisBody {
    pub fn new(mass: f32) -> Self {
        let inv_mass = if mass > 0.0 { 1.0 / mass } else { 0.0 };
        // Uniform box inertia guess I ≈ m·(1.8²)/6; inv is what we apply.
        let inertia = mass * (1.8 * 1.8) / 6.0;
        Self {
            linvel: Vec3::ZERO,
            angvel: Vec3::ZERO,
            mass,
            inv_mass,
            inv_inertia: if inertia > 0.0 { 1.0 / inertia } else { 0.0 },
            gravity_factor: 1.0,
        }
    }

    /// Velocity of the point `p` on the body (`v + ω × r`, the exact quantity `FUN_0044db60`
    /// computes at the front-axle point). `com` is the world-space centre of mass.
    #[inline]
    pub fn point_velocity(&self, p: Vec3, com: Vec3) -> Vec3 {
        self.linvel + self.angvel.cross(p - com)
    }

    /// Apply a world-space impulse at world point `p` (motion vtbl `+0x4c` applyPointImpulse).
    #[inline]
    pub fn apply_impulse_at(&mut self, impulse: Vec3, p: Vec3, com: Vec3) {
        self.linvel += impulse * self.inv_mass;
        let r = p - com;
        self.angvel += r.cross(impulse) * self.inv_inertia;
    }

    /// Apply a pure angular impulse (motion vtbl `+0x44` applyAngularImpulse).
    #[inline]
    pub fn apply_angular_impulse(&mut self, ang_impulse: Vec3) {
        self.angvel += ang_impulse * self.inv_inertia;
    }
}

/// A single wheel — the pool-allocated `_CarWheel` (0x130 bytes, pool `DAT_017d50b0`). The tuning
/// (radius/suspension/friction) lives on the axle in [`VehicleTuning`]; this is the per-wheel
/// runtime + placement.
#[derive(Clone, Copy, Debug)]
pub struct Wheel {
    /// Hardpoint: suspension top attachment in chassis-local space (the `hp_wheel_*` hardpoints).
    pub hardpoint: Vec3,
    /// Belongs to the front axle (`true`) or rear (`false`). Middle wheels clone the front block.
    pub front: bool,
    /// This wheel steers with the steering input (front wheels of a car).
    pub steered: bool,
    /// This wheel receives drive torque.
    pub powered: bool,

    // ---- per-frame runtime (written by the raycast + suspension pass) ----
    /// Ground contact found by the last raycast.
    pub contact: bool,
    /// World-space contact point.
    pub contact_point: Vec3,
    /// World-space contact normal.
    pub contact_normal: Vec3,
    /// Suspension compression this frame ((restLen+radius) − rayDistance, clamped ≥ 0).
    pub compression: f32,
    /// Compression last frame (for the damper's compression velocity).
    pub prev_compression: f32,
    /// Normal load carried this frame (spring force), fed to the friction clamp.
    pub normal_load: f32,
}

impl Wheel {
    pub fn new(hardpoint: Vec3, front: bool, steered: bool, powered: bool) -> Self {
        Self {
            hardpoint,
            front,
            steered,
            powered,
            contact: false,
            contact_point: Vec3::ZERO,
            contact_normal: Vec3::Y,
            compression: 0.0,
            prev_compression: 0.0,
            normal_load: 0.0,
        }
    }
}

/// The wheel array as one ECS component (hecs stores one value per component type per entity).
#[derive(Clone, Debug, Default)]
pub struct WheelSet(pub Vec<Wheel>);

/// The drive-object input fields the per-class `HandleCommand` switch writes into
/// (`vehicle_code_map.md` §1.4). Car/tank use `+0x28..+0x38`; heli uses four axes `+0xF8..+0x104`;
/// boat uses two `+0x28/+0x2C`.
#[derive(Clone, Copy, Debug, Default)]
pub struct VehicleControls {
    // ---- car / tank (drive obj `this[0x56]`) ----
    /// `+0x28` Turn — `clamp(1.0 − payload)` for id `0x3483DBF1`. Signed: <0 left, >0 right.
    pub turn: f32,
    /// `+0x2c` forward accel channel (`0x0490757F`, or the combined `0x460C5913`).
    pub accel: f32,
    /// `+0x30` brake (`0x55B8E0A1`).
    pub brake: f32,
    /// `+0x34` 5th/open channel (`0x37086E0A`).
    pub aux5: f32,
    /// `+0x38` handbrake (`0x574220AC`).
    pub handbrake: f32,

    // ---- helicopter (drive obj `this+0x140`, 4 axes) ----
    /// `+0xF8` lift/collective (`0x7D3B632C`).
    pub heli_lift: f32,
    /// `+0xFC` (`0x0490757F`).
    pub heli_a: f32,
    /// `+0x100` yaw (`0x3483DBF1`).
    pub heli_yaw: f32,
    /// `+0x104` (`0x37086E0A`).
    pub heli_b: f32,

    // ---- boat (drive obj `this+0x140`, 2 axes) ----
    /// `+0x28` throttle (`0x7D3B632C`).
    pub boat_throttle: f32,
    /// `+0x2C` turn, stored as `1.0 − payload` (BoatTurn reuses the CarTurn id `0x3483DBF1`).
    pub boat_turn: f32,
}

/// Per-frame drive scratch — the runtime actor fields that persist across frames but are not tuning
/// and not input (`vehicle_code_map.md` §4 field map).
#[derive(Clone, Copy, Debug, Default)]
pub struct VehicleRuntime {
    /// `+0x98` reverse flag — the car is in its reverse regime.
    pub reverse: bool,
    /// `+0x170` round-robin raycast index (which single wheel gets raycast on a slow frame).
    pub rr_index: usize,
    /// `+0x174` forward speed (chassis velocity along the ground-forward axis).
    pub fwd_speed: f32,
    /// `+0x40` donut heat — ramps while handbrake+throttle held; scales `DonutBoost`.
    pub donut_heat: f32,
    /// Donut wobble phase counter (indexes the sine LUT `& 0x1fff`).
    pub donut_phase: u32,
    /// Averaged ground normal from the wheel contacts (`FUN_0044cc90`).
    pub ground_normal: Vec3,
    /// `+0x280` moving-forward flag.
    pub moving_fwd: bool,
    /// Number of wheels that found ground on the last raycast pass (drive gate needs ≥ half).
    pub grounded: usize,
}

impl VehicleRuntime {
    pub fn new() -> Self {
        Self {
            ground_normal: Vec3::Y,
            ..Self::default()
        }
    }
}

/// A single seat slot on a vehicle.
#[derive(Clone, Copy, Debug)]
pub struct Seat {
    /// Seat role: `Driver` is the control seat; others are riders/gunners.
    pub kind: SeatKind,
    /// The actor (human entity) currently occupying the seat, if any.
    pub occupant: Option<Entity>,
    /// The seat is a ladder rung (climb, not ride).
    pub is_ladder: bool,
    /// The seat is currently blocked (destroyed/obstructed) and cannot be entered.
    pub blocked: bool,
    /// The player is allowed to use this seat (`Vehicle.SetCanPlayerUse`).
    pub player_usable: bool,
}

/// Seat role classification (`Vehicle.GetSeatByType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Driver,
    Passenger,
    Gunner,
    Turret,
}

impl Seat {
    pub fn new(kind: SeatKind) -> Self {
        Self {
            kind,
            occupant: None,
            is_ladder: false,
            blocked: false,
            player_usable: true,
        }
    }
}

/// Ring-1 seat / enter-exit / driver-assignment state (`vehicle_code_map.md` §1.2 ring 1, applier
/// `FUN_0053f110`). The command ring is the *transport*; this component is the applied result the
/// mount/dismount handlers write.
#[derive(Clone, Debug, Default)]
pub struct Seating {
    pub seats: Vec<Seat>,
    /// Named parts toggled by `Vehicle.SetParts` (e.g. `LightFront`, `CtrlRotation`, `main_turret`).
    pub parts: Vec<(String, bool)>,
}

impl Seating {
    /// Index of the driver seat, if the vehicle has one.
    pub fn driver_seat(&self) -> Option<usize> {
        self.seats.iter().position(|s| s.kind == SeatKind::Driver)
    }
}

/// The chassis world transform helper: rotate a local axis into world space.
#[inline]
pub fn world_axis(rotation: Quat, local: Vec3) -> Vec3 {
    rotation * local
}
