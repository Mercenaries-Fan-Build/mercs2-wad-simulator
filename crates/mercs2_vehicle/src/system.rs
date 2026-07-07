//! The ECS drive-step system and the command-ring pump — the runtime glue that ties the command
//! transport (`command.rs`) to the drive model (`drive.rs`) over a `hecs::World`.
//!
//! Placement (`vehicle_code_map.md` §1.4): the pump runs in the game layer (idx 4) of the master
//! tick (`FUN_00532f80`, call site `0x004c990c`), just before the vehicle drive step. So a host
//! schedule calls [`pump_car_ring`]/[`pump_boat_heli_ring`] first, then [`drive_step_system`].

use mercs2_core::PhysicsQuery;
use mercs2_core::{Transform, World};

use crate::command::{handle_command, CommandRing};
use crate::components::{
    ChassisBody, Vehicle, VehicleControls, VehicleRuntime, VehicleTuning, WheelSet,
};
use crate::drive::{CarActor, DriveSim, TankActor, VehicleActor};
use crate::lut::DonutLut;

/// Advance every simulated vehicle one fixed step. Iterates entities carrying the full drive
/// component set and dispatches by [`Vehicle::class`] to the matching actor (`applyAction`). Wheel
/// rays are grounded on `phys` (the silo-7 `PhysicsQuery` seam).
pub fn drive_step_system(world: &mut World, phys: &dyn PhysicsQuery, lut: &DonutLut, dt: f32) {
    for (_e, (veh, xform, body, ctrl, wheels, tuning, rt)) in world
        .query::<(
            &Vehicle,
            &mut Transform,
            &mut ChassisBody,
            &VehicleControls,
            &mut WheelSet,
            &VehicleTuning,
            &mut VehicleRuntime,
        )>()
        .iter()
    {
        let actor: &dyn VehicleActor = match veh.class {
            crate::components::VehicleClass::Car | crate::components::VehicleClass::Bike => {
                &CarActor
            }
            crate::components::VehicleClass::Tank => &TankActor,
            // Boat/Heli/Jet full applyAction sim lands in a later pass (see DEFERRED.md); their
            // controls are still routed by the pump so the seam is exercised end-to-end.
            _ => continue,
        };
        let mut sim = DriveSim {
            xform,
            body,
            ctrl,
            wheels: &mut wheels.0,
            tuning,
            rt,
            phys,
            lut,
        };
        actor.apply_action(&mut sim, dt);
    }
}

/// Drain `channel` of a control ring and dispatch every record to its target vehicle's
/// [`VehicleControls`] via the per-class `HandleCommand` switch (`FUN_00532f80` → `FUN_00437300`/…).
/// Records whose target handle matches no vehicle are dropped (as the exe does).
fn pump_ring(world: &mut World, ring: &mut CommandRing, channel: u8) {
    let records = ring.drain_channel(channel);
    if records.is_empty() {
        return;
    }
    for rec in records {
        for (_e, (veh, ctrl)) in world.query::<(&Vehicle, &mut VehicleControls)>().iter() {
            if veh.handle == rec.target {
                handle_command(veh.class, ctrl, &rec);
            }
        }
    }
}

/// Pump the car/tank control ring (`0x011C0230`) for the drive model's subscriber channel.
pub fn pump_car_ring(world: &mut World, ring: &mut CommandRing, channel: u8) {
    pump_ring(world, ring, channel);
}

/// Pump the boat/heli control ring (`0x011C2478`).
pub fn pump_boat_heli_ring(world: &mut World, ring: &mut CommandRing, channel: u8) {
    pump_ring(world, ring, channel);
}
