//! Engine-side bodies for the `Vehicle` / `Camera` Lua namespaces (`vehicle_code_map.md` §1.3,
//! `camera_code_map.md` §6).
//!
//! **Wiring seam.** These are the *real* engine functions the `mercs2_script` binding layer wraps —
//! it does **not** live here (leaf crates never depend on `mercs2_script`). The binding installer in
//! `crates/mercs2_script/src/bindings/vehicle.rs` (`REQUIRED` table VA `0xB98918`, 40 cfuncs) and
//! `.../camera.rs` (table VA `0xB9A530`, 7 cfuncs) should call these from inside `install(..)` via
//! `b.real(..)`, converting Lua `uGuid`/`Entity` args to/from the `World`. The name→fn map:
//!
//! | Lua cfunc                | engine fn (here)                    |
//! |--------------------------|-------------------------------------|
//! | `Vehicle.GetDriver`      | [`get_driver`]                      |
//! | `Vehicle.GetRiders`      | [`get_riders`]                      |
//! | `Vehicle.GetFromRider`   | [`get_from_rider`]                  |
//! | `Vehicle.GetSeatFromRider`| [`get_seat_from_rider`]            |
//! | `Vehicle.GetSeatByType`  | [`get_seat_by_type`]                |
//! | `Vehicle.GetSeatParams`  | [`get_seat_params`]                 |
//! | `Vehicle.Enter`          | [`enter`]                           |
//! | `Vehicle.EnterBySeatGuid`| [`enter_by_seat`]                   |
//! | `Vehicle.Exit`           | [`exit`]                            |
//! | `Vehicle.IsSeatBlocked`  | [`is_seat_blocked`]                 |
//! | `Vehicle.IsSeatALadder`  | [`is_seat_ladder`]                  |
//! | `Vehicle.SetCanPlayerUse`| [`set_can_player_use`]              |
//! | `Vehicle.Usable`         | [`usable`]                          |
//! | `Vehicle.SetParts`       | [`set_parts`]                       |
//! | `Vehicle.ClearControls`  | [`clear_controls`] (+ enqueue `cmd::CLEAR_CONTROLS`) |
//! | `Camera.*` mode select   | [`camera_mode`] / [`vehicle_camera_pose`] |
//!
//! Everything here is a **real body** (no stubs): it mutates the [`Seating`] component that the
//! ring-1 seat/enter-exit applier (`FUN_0053f110`) produces.

use hecs::Entity;
use mercs2_core::{Transform, World};

use crate::camera::{chase_pose, CameraMode, CameraPose, CameraPreset};
use crate::components::{
    ChassisBody, Seat, SeatKind, Seating, Vehicle, VehicleControls, VehicleRuntime,
};

/// Resolve a gameplay handle (`uGuid`) to its ECS entity.
pub fn resolve(world: &World, handle: u32) -> Option<Entity> {
    world
        .query::<&Vehicle>()
        .iter()
        .find(|(_e, v)| v.handle == handle)
        .map(|(e, _)| e)
}

/// `Vehicle.GetDriver(uVehicle)` — the entity in the driver seat, if any.
pub fn get_driver(world: &World, vehicle: Entity) -> Option<Entity> {
    let seating = world.get::<&Seating>(vehicle).ok()?;
    let idx = seating.driver_seat()?;
    seating.seats[idx].occupant
}

/// `Vehicle.GetRiders(uVehicle)` — every occupant across all seats.
pub fn get_riders(world: &World, vehicle: Entity) -> Vec<Entity> {
    match world.get::<&Seating>(vehicle) {
        Ok(seating) => seating.seats.iter().filter_map(|s| s.occupant).collect(),
        Err(_) => Vec::new(),
    }
}

/// `Vehicle.GetFromRider(uRider)` — which vehicle an actor is riding (scan seats).
pub fn get_from_rider(world: &World, rider: Entity) -> Option<Entity> {
    world
        .query::<&Seating>()
        .iter()
        .find(|(_v, s)| s.seats.iter().any(|seat| seat.occupant == Some(rider)))
        .map(|(v, _)| v)
}

/// `Vehicle.GetSeatFromRider(uRider)` — `(vehicle, seat index)` the actor occupies.
pub fn get_seat_from_rider(world: &World, rider: Entity) -> Option<(Entity, usize)> {
    for (v, s) in world.query::<&Seating>().iter() {
        if let Some(i) = s.seats.iter().position(|seat| seat.occupant == Some(rider)) {
            return Some((v, i));
        }
    }
    None
}

/// `Vehicle.GetSeatByType(uVehicle, kind)` — first seat index of the given role.
pub fn get_seat_by_type(world: &World, vehicle: Entity, kind: SeatKind) -> Option<usize> {
    let seating = world.get::<&Seating>(vehicle).ok()?;
    seating.seats.iter().position(|s| s.kind == kind)
}

/// Read-only snapshot of a seat (`Vehicle.GetSeatParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeatParams {
    pub kind: SeatKind,
    pub is_ladder: bool,
    pub blocked: bool,
    pub player_usable: bool,
    pub occupied: bool,
}

/// `Vehicle.GetSeatParams(uVehicle, seat)` — the seat's role + flags.
pub fn get_seat_params(world: &World, vehicle: Entity, seat: usize) -> Option<SeatParams> {
    let seating = world.get::<&Seating>(vehicle).ok()?;
    let s = seating.seats.get(seat)?;
    Some(SeatParams {
        kind: s.kind,
        is_ladder: s.is_ladder,
        blocked: s.blocked,
        player_usable: s.player_usable,
        occupied: s.occupant.is_some(),
    })
}

/// `Vehicle.IsSeatBlocked(uVehicle, seat)`.
pub fn is_seat_blocked(world: &World, vehicle: Entity, seat: usize) -> bool {
    world
        .get::<&Seating>(vehicle)
        .ok()
        .and_then(|s| s.seats.get(seat).map(|x| x.blocked))
        .unwrap_or(true)
}

/// `Vehicle.IsSeatALadder(uVehicle, seat)`.
pub fn is_seat_ladder(world: &World, vehicle: Entity, seat: usize) -> bool {
    world
        .get::<&Seating>(vehicle)
        .ok()
        .and_then(|s| s.seats.get(seat).map(|x| x.is_ladder))
        .unwrap_or(false)
}

/// `Vehicle.SetCanPlayerUse(uVehicle, seat, bCan)`.
pub fn set_can_player_use(world: &mut World, vehicle: Entity, seat: usize, can: bool) -> bool {
    if let Ok(mut s) = world.get::<&mut Seating>(vehicle) {
        if let Some(seat) = s.seats.get_mut(seat) {
            seat.player_usable = can;
            return true;
        }
    }
    false
}

/// `Vehicle.Usable(uVehicle)` — any non-blocked, player-usable, empty seat exists.
pub fn usable(world: &World, vehicle: Entity) -> bool {
    world
        .get::<&Seating>(vehicle)
        .map(|s| {
            s.seats
                .iter()
                .any(|seat| seat.player_usable && !seat.blocked && seat.occupant.is_none())
        })
        .unwrap_or(false)
}

/// `Vehicle.Enter(uVehicle, uActor, kind)` — seat `actor` in the first free seat of `kind` (defaults
/// to the driver seat when `kind == Driver`). Returns the seat index taken. This is the applied
/// mount the ring-1 handler `FUN_00540690` performs.
pub fn enter(world: &mut World, vehicle: Entity, actor: Entity, kind: SeatKind) -> Option<usize> {
    let mut seating = world.get::<&mut Seating>(vehicle).ok()?;
    let idx = seating.seats.iter().position(|s| {
        s.kind == kind && s.occupant.is_none() && !s.blocked && s.player_usable
    })?;
    seating.seats[idx].occupant = Some(actor);
    Some(idx)
}

/// `Vehicle.EnterBySeatGuid(uVehicle, uActor, seat)` — seat `actor` in a specific seat index.
pub fn enter_by_seat(world: &mut World, vehicle: Entity, actor: Entity, seat: usize) -> bool {
    if let Ok(mut seating) = world.get::<&mut Seating>(vehicle) {
        if let Some(s) = seating.seats.get_mut(seat) {
            if s.occupant.is_none() && !s.blocked {
                s.occupant = Some(actor);
                return true;
            }
        }
    }
    false
}

/// `Vehicle.Exit(uActor)` — remove `actor` from whatever seat it occupies (dismount `FUN_00538fe0`).
/// Returns the vehicle it left, if any.
pub fn exit(world: &mut World, actor: Entity) -> Option<Entity> {
    let (vehicle, idx) = get_seat_from_rider(world, actor)?;
    if let Ok(mut seating) = world.get::<&mut Seating>(vehicle) {
        seating.seats[idx].occupant = None;
    }
    Some(vehicle)
}

/// `Vehicle.SetParts(uVehicle, sPart, bEnabled)` — toggle a named vehicle part (e.g. `LightFront`,
/// `CtrlRotation`, `main_turret`). Returns the new state. (Used by `alarm.lua`, `mrxactionhijack`.)
pub fn set_parts(world: &mut World, vehicle: Entity, part: &str, enabled: bool) -> bool {
    if let Ok(mut seating) = world.get::<&mut Seating>(vehicle) {
        if let Some(entry) = seating.parts.iter_mut().find(|(name, _)| name == part) {
            entry.1 = enabled;
        } else {
            seating.parts.push((part.to_string(), enabled));
        }
        return enabled;
    }
    false
}

/// `Vehicle.ClearControls(uVehicle)` — zero the drive-obj input fields immediately (also mirrors the
/// `cmd::CLEAR_CONTROLS` ring broadcast, so subscribers see the clear).
pub fn clear_controls(world: &mut World, vehicle: Entity) -> bool {
    if let Ok(mut ctrl) = world.get::<&mut VehicleControls>(vehicle) {
        *ctrl = VehicleControls::default();
        return true;
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Camera surface (`Camera.*` / vehicle-camera mode selection).
// ---------------------------------------------------------------------------------------------

/// The camera mode the owner's currently-ridden object selects (`camera_code_map.md` §4 mode gate).
/// If `owner` (a player/human) is riding a vehicle, returns that vehicle's chase mode; otherwise
/// on-foot.
pub fn camera_mode(world: &World, owner: Entity) -> CameraMode {
    match get_from_rider(world, owner) {
        Some(vehicle) => match world.get::<&Vehicle>(vehicle) {
            Ok(v) => CameraMode::for_class(v.class),
            Err(_) => CameraMode::OnFoot,
        },
        None => CameraMode::OnFoot,
    }
}

/// Resolve the chase-cam pose for the vehicle `owner` is riding, given the look axis. Returns `None`
/// when `owner` is on foot (the on-foot camera is another silo's).
pub fn vehicle_camera_pose(
    world: &World,
    owner: Entity,
    look_yaw: f32,
    look_pitch: f32,
) -> Option<CameraPose> {
    let vehicle = get_from_rider(world, owner)?;
    let v = world.get::<&Vehicle>(vehicle).ok()?;
    let xform = world.get::<&Transform>(vehicle).ok()?;
    let mode = CameraMode::for_class(v.class);
    // Prefer an authored preset component if the entity carries one; else the mode default.
    let preset = world
        .get::<&CameraPreset>(vehicle)
        .map(|p| *p)
        .unwrap_or_else(|_| CameraPreset::for_mode(mode));
    // Forward speed for the FOV widening.
    let speed = world
        .get::<&VehicleRuntime>(vehicle)
        .map(|rt| rt.fwd_speed)
        .unwrap_or(0.0);
    Some(chase_pose(
        &preset,
        xform.translation,
        xform.rotation,
        look_yaw,
        look_pitch,
        speed,
    ))
}

/// Convenience: spawn a bare vehicle entity with the full drive + seating component set, ready for
/// [`crate::system::drive_step_system`]. The binding layer / world loader uses this shape when it
/// materialises a `_CarPhysicsV2` / `_TankPhysics` object.
#[allow(clippy::too_many_arguments)]
pub fn spawn_vehicle(
    world: &mut World,
    transform: Transform,
    vehicle: Vehicle,
    body: ChassisBody,
    controls: VehicleControls,
    wheels: crate::components::WheelSet,
    tuning: crate::components::VehicleTuning,
    runtime: VehicleRuntime,
    seating: Seating,
) -> Entity {
    world.spawn((
        transform, vehicle, body, controls, wheels, tuning, runtime, seating,
    ))
}

/// Build a standard 4-seat car seating (driver + 3 passengers). Used by tests and the loader.
pub fn default_car_seating() -> Seating {
    Seating {
        seats: vec![
            Seat::new(SeatKind::Driver),
            Seat::new(SeatKind::Passenger),
            Seat::new(SeatKind::Passenger),
            Seat::new(SeatKind::Gunner),
        ],
        parts: Vec::new(),
    }
}
