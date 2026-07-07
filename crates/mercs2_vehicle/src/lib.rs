//! `mercs2_vehicle` — the Vehicle actor family + custom **raycast** drive model, command-ring
//! control transport, and vehicle/on-foot camera modes.
//!
//! **Silo 25** (rows 25/19). The exe is the oracle: `docs/reverse_engineer/vehicle_code_map.md`
//! (+ `docs/data/vehicle_code_map.json`, `camera_code_map.md`). The drive model is DECODED as a
//! **custom raycast car / tank / boat / heli sim** — nine `hkpUnaryAction`-derived actor classes —
//! **NOT** the Havok Vehicle Kit (there are zero `hkpVehicle*` classes on PC). A prior Ghidra
//! noreturn-on-sqrt defect had truncated every physics fn at its first `sqrt`; the decoded math
//! below is now trustworthy (code map §0).
//!
//! Layout:
//! - [`lut`]       — the donut/turn sine LUT `DAT_00cf2900` (8192-entry, `& 0x1fff`).
//! - [`tuning`]    — the `_CarPhysicsV2` (0x18c) / `_TankPhysics` (0x78) tuning-block → field map.
//! - [`components`]— the vehicle ECS component set (defined in THIS crate per the carve rule).
//! - [`command`]   — the 6-ring broadcast command transport + per-class `HandleCommand` switch.
//! - [`drive`]     — the decoded per-axle-friction + torque-falloff + donut-LUT drive math.
//! - [`camera`]    — the data-driven camera modes (`CameraCarPreset`/`Tank`/`Turret`/`Helicopter`).
//! - [`lua_surface`]— real engine bodies for the `Vehicle`/`Camera` Lua namespaces (binding seam).
//! - [`system`]    — the ECS drive-step system + command pump.
//!
//! Wheel raycasts are grounded on `mercs2_core::PhysicsQuery` (the silo-7 seam); this crate never
//! depends on `mercs2_physics`.

pub mod camera;
pub mod command;
pub mod components;
pub mod drive;
pub mod lua_surface;
pub mod lut;
pub mod system;
pub mod tuning;

pub use camera::{chase_pose, CameraMode, CameraPose, CameraPreset};
pub use command::{cmd, handle_command, CommandRecord, CommandRing};
pub use components::{
    ChassisBody, Seat, SeatKind, Seating, Vehicle, VehicleClass, VehicleControls, VehicleRuntime,
    VehicleTuning, Wheel, WheelSet,
};
pub use drive::{CarActor, DriveSim, TankActor, VehicleActor};
pub use lut::DonutLut;
pub use system::{drive_step_system, pump_boat_heli_ring, pump_car_ring};
pub use tuning::AxleTuning;

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};
    use mercs2_core::physics_query::ClosestPoint;
    use mercs2_core::{PhysicsQuery, RayHit, Transform, World};

    /// A flat ground plane at `y` for the wheel raycasts.
    struct FlatGround {
        y: f32,
    }
    impl PhysicsQuery for FlatGround {
        fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
            if dir.y.abs() < 1e-6 {
                return None;
            }
            let t = (self.y - origin.y) / dir.y;
            if t < 0.0 || t > max {
                return None;
            }
            Some(RayHit {
                point: origin + dir * t,
                normal: Vec3::Y,
                distance: t,
                entity: None,
            })
        }
        fn closest_point(&self, _p: Vec3, _max: f32) -> Option<ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            pos + delta
        }
    }

    /// A standard 4-wheel car: front (steered, unpowered) + rear (powered) at the four corners.
    fn car_wheels() -> WheelSet {
        WheelSet(vec![
            Wheel::new(Vec3::new(-0.8, 0.0, 1.3), true, true, false),
            Wheel::new(Vec3::new(0.8, 0.0, 1.3), true, true, false),
            Wheel::new(Vec3::new(-0.8, 0.0, -1.3), false, false, true),
            Wheel::new(Vec3::new(0.8, 0.0, -1.3), false, false, true),
        ])
    }

    fn spawn_test_car(world: &mut World, handle: u32, y: f32) -> hecs::Entity {
        lua_surface::spawn_vehicle(
            world,
            Transform::from_translation(Vec3::new(0.0, y, 0.0)),
            Vehicle::new(VehicleClass::Car, handle),
            ChassisBody::new(1200.0),
            VehicleControls::default(),
            car_wheels(),
            VehicleTuning::default(),
            VehicleRuntime::new(),
            lua_surface::default_car_seating(),
        )
    }

    // ---- Deliverable 5, test 3: wheel ray finds ground and suspends the body ----
    #[test]
    fn wheel_ray_grounds_and_suspends() {
        let mut world = World::new();
        let e = spawn_test_car(&mut world, 0x1000, 0.85);
        let ground = FlatGround { y: 0.0 };
        let lut = DonutLut::new();
        for _ in 0..240 {
            drive_step_system(&mut world, &ground, &lut, 1.0 / 60.0);
        }
        let ws = world.get::<&WheelSet>(e).unwrap();
        assert!(ws.0.iter().all(|w| w.contact), "all wheels should contact");
        let t = world.get::<&Transform>(e).unwrap();
        assert!(
            t.translation.y > 0.2 && t.translation.y < 1.2,
            "chassis should rest on its suspension, got y={}",
            t.translation.y
        );
    }

    // ---- Deliverable 5, test 1: car accelerates toward MaxSpeed and clamps ----
    #[test]
    fn car_accelerates_and_clamps_to_max_speed() {
        let mut world = World::new();
        let e = spawn_test_car(&mut world, 0x2000, 0.8);
        world.get::<&mut VehicleControls>(e).unwrap().accel = 1.0;
        let ground = FlatGround { y: 0.0 };
        let lut = DonutLut::new();

        let vmax = VehicleTuning::default().max_speed;
        let mut prev = 0.0f32;
        let mut rose = false;
        for i in 0..900 {
            drive_step_system(&mut world, &ground, &lut, 1.0 / 60.0);
            let v = world.get::<&VehicleRuntime>(e).unwrap().fwd_speed;
            assert!(v <= vmax * 1.02, "step {i}: speed {v} exceeded MaxSpeed {vmax}");
            if i < 120 && v > prev {
                rose = true;
            }
            prev = v;
        }
        assert!(rose, "car should accelerate from rest");
        let v = world.get::<&VehicleRuntime>(e).unwrap().fwd_speed;
        assert!(
            v > vmax * 0.5,
            "car should approach MaxSpeed under full throttle, got {v} / {vmax}"
        );
    }

    // ---- Deliverable 5, test 2: steering turns the car via the sine LUT ----
    #[test]
    fn steering_turns_via_lut() {
        let mut world = World::new();
        let e = spawn_test_car(&mut world, 0x3000, 0.8);
        world.get::<&mut ChassisBody>(e).unwrap().linvel = Vec3::new(0.0, 0.0, 10.0);
        world.get::<&mut VehicleControls>(e).unwrap().turn = -0.8;
        let ground = FlatGround { y: 0.0 };
        let lut = DonutLut::new();

        let heading0 = (world.get::<&Transform>(e).unwrap().rotation * Vec3::Z).x;
        for _ in 0..180 {
            drive_step_system(&mut world, &ground, &lut, 1.0 / 60.0);
        }
        let heading1 = (world.get::<&Transform>(e).unwrap().rotation * Vec3::Z).x;
        assert!(
            (heading1 - heading0).abs() > 0.05,
            "steering should change heading (fwd.x {heading0} -> {heading1})"
        );
    }

    // Opposite steer should turn the other way (LUT sign symmetry).
    #[test]
    fn opposite_steer_turns_other_way() {
        let turn_dir = |steer: f32| {
            let mut world = World::new();
            let e = spawn_test_car(&mut world, 0x3100, 0.8);
            world.get::<&mut ChassisBody>(e).unwrap().linvel = Vec3::new(0.0, 0.0, 10.0);
            world.get::<&mut VehicleControls>(e).unwrap().turn = steer;
            let ground = FlatGround { y: 0.0 };
            let lut = DonutLut::new();
            for _ in 0..120 {
                drive_step_system(&mut world, &ground, &lut, 1.0 / 60.0);
            }
            let x = (world.get::<&Transform>(e).unwrap().rotation * Vec3::Z).x;
            x
        };
        let left = turn_dir(-0.8);
        let right = turn_dir(0.8);
        assert!(
            left.signum() != right.signum() && left.abs() > 0.02 && right.abs() > 0.02,
            "left ({left}) and right ({right}) steer should yaw opposite ways"
        );
    }

    // ---- Deliverable 5, test 4: a HandleCommand routes a Turn command ----
    #[test]
    fn handle_command_routes_turn() {
        // Turn id, payload 1.5 -> +0x28 = clamp(1 - 1.5) = -0.5.
        let mut ctrl = VehicleControls::default();
        let rec = CommandRecord::new(0x4000, cmd::TURN, 1.5);
        assert!(handle_command(VehicleClass::Car, &mut ctrl, &rec));
        assert!((ctrl.turn - (-0.5)).abs() < 1e-6, "turn = {}", ctrl.turn);
    }

    // End-to-end: enqueue on the ring, pump, and see the target vehicle's controls updated.
    #[test]
    fn ring_pump_delivers_turn_to_vehicle() {
        let mut world = World::new();
        let e = spawn_test_car(&mut world, 0x5000, 0.8);
        let mut ring = CommandRing::car();
        let channel = ring.subscribe().expect("channel");
        assert!(ring.enqueue(CommandRecord::new(0x5000, cmd::TURN, 0.25)));
        pump_car_ring(&mut world, &mut ring, channel);
        let turn = world.get::<&VehicleControls>(e).unwrap().turn;
        assert!((turn - 0.75).abs() < 1e-6, "turn routed to vehicle = {turn}");
        assert!(ring.is_empty());
    }

    // The broadcast ring shows every command once to every subscriber.
    #[test]
    fn ring_broadcasts_to_all_subscribers() {
        let mut ring = CommandRing::car();
        let a = ring.subscribe().unwrap();
        let b = ring.subscribe().unwrap();
        assert_ne!(a, b);
        ring.enqueue(CommandRecord::new(1, cmd::BRAKE, 1.0));
        assert_eq!(ring.drain_channel(a).len(), 1);
        assert_eq!(ring.len(), 1); // still present for b (broadcast)
        assert_eq!(ring.drain_channel(b).len(), 1);
        assert!(ring.is_empty()); // now compacted
    }

    // The tank shares the actor abstraction and yaws by track differential.
    #[test]
    fn tank_turns_in_place_by_differential() {
        let mut world = World::new();
        let e = lua_surface::spawn_vehicle(
            &mut world,
            Transform::from_translation(Vec3::new(0.0, 0.8, 0.0)),
            Vehicle::new(VehicleClass::Tank, 0x6000),
            ChassisBody::new(30_000.0),
            VehicleControls::default(),
            car_wheels(),
            VehicleTuning::tank_default(),
            VehicleRuntime::new(),
            lua_surface::default_car_seating(),
        );
        world.get::<&mut VehicleControls>(e).unwrap().turn = 1.0;
        let ground = FlatGround { y: 0.0 };
        let lut = DonutLut::new();
        let yaw0 = (world.get::<&Transform>(e).unwrap().rotation * Vec3::Z).x;
        for _ in 0..120 {
            drive_step_system(&mut world, &ground, &lut, 1.0 / 60.0);
        }
        let yaw1 = (world.get::<&Transform>(e).unwrap().rotation * Vec3::Z).x;
        assert!(
            (yaw1 - yaw0).abs() > 0.05,
            "tank should yaw in place ({yaw0} -> {yaw1})"
        );
    }

    // Seat / enter-exit surface (the Vehicle Lua bodies).
    #[test]
    fn enter_and_exit_seats() {
        let mut world = World::new();
        let v = spawn_test_car(&mut world, 0x7000, 0.8);
        let driver = world.spawn((Transform::IDENTITY,));
        let rider = world.spawn((Transform::IDENTITY,));

        assert_eq!(
            lua_surface::enter(&mut world, v, driver, SeatKind::Driver),
            Some(0)
        );
        assert_eq!(lua_surface::get_driver(&world, v), Some(driver));

        lua_surface::enter(&mut world, v, rider, SeatKind::Passenger);
        assert_eq!(lua_surface::get_riders(&world, v).len(), 2);
        assert_eq!(lua_surface::get_from_rider(&world, rider), Some(v));

        assert_eq!(lua_surface::exit(&mut world, driver), Some(v));
        assert_eq!(lua_surface::get_driver(&world, v), None);
    }

    // SetParts toggles a named part and reports state.
    #[test]
    fn set_parts_toggles() {
        let mut world = World::new();
        let v = spawn_test_car(&mut world, 0x7100, 0.8);
        assert!(lua_surface::set_parts(&mut world, v, "LightFront", true));
        assert!(!lua_surface::set_parts(&mut world, v, "LightFront", false));
        let seating = world.get::<&Seating>(v).unwrap();
        assert_eq!(seating.parts, vec![("LightFront".to_string(), false)]);
    }

    // Camera mode follows the ridden vehicle; produces a pose behind + above it.
    #[test]
    fn camera_mode_and_pose_from_ridden_vehicle() {
        let mut world = World::new();
        let v = spawn_test_car(&mut world, 0x8000, 0.8);
        let player = world.spawn((Transform::IDENTITY,));
        assert_eq!(lua_surface::camera_mode(&world, player), CameraMode::OnFoot);
        assert!(lua_surface::vehicle_camera_pose(&world, player, 0.0, 0.0).is_none());

        lua_surface::enter(&mut world, v, player, SeatKind::Driver);
        assert_eq!(lua_surface::camera_mode(&world, player), CameraMode::Car);
        let pose = lua_surface::vehicle_camera_pose(&world, player, 0.0, 0.0).unwrap();
        assert!(pose.position.z < 0.0, "camera should be behind, z={}", pose.position.z);
        assert!(pose.position.y > 0.8, "camera should be above, y={}", pose.position.y);
        assert!(pose.fov > 0.5);
    }

    // The donut sine LUT is a real sine table indexed with the 0x1fff mask.
    #[test]
    fn donut_lut_is_sine() {
        let lut = DonutLut::new();
        assert!(lut.sin(0.0).abs() < 1e-3);
        assert!((lut.sin(std::f32::consts::FRAC_PI_2) - 1.0).abs() < 1e-2);
        assert!((lut.cos(0.0) - 1.0).abs() < 1e-2);
        assert_eq!(lut.sample(0), lut.sample(0x2000)); // mask wraps at 0x2000
    }

    // Forward-axis convention sanity (canonical game space, +Z forward).
    #[test]
    fn forward_axis_is_plus_z() {
        let q = Quat::IDENTITY;
        assert!(((q * Vec3::Z) - Vec3::Z).length() < 1e-6);
    }
}
