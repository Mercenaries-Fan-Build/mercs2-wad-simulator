# mercs2_vehicle

Silo 25 of the Mercenaries 2 reimplementation: the vehicle actor family, the game's custom raycast
drive model, the command-ring control transport, and the vehicle/on-foot camera modes.

## What it is

A leaf library crate. It owns everything a drivable vehicle is in the reimplemented engine:

- **The vehicle ECS component set** — `Vehicle` (class tag + gameplay handle), `ChassisBody` (the
  chassis rigid body), `WheelSet`/`Wheel` (per-wheel hardpoint, contact, suspension state),
  `VehicleControls` (the drive-object input fields), `VehicleRuntime` (per-frame scratch: forward
  speed, donut phase, round-robin wheel index), `VehicleTuning`/`AxleTuning`, and `Seating`/`Seat`.
- **The drive model** (`drive.rs`) — a custom raycast car/tank simulation: wheel rays → suspension
  spring → per-axle friction → traction/torque impulse, integrated on a minimal chassis body. Cars
  (and the 2-wheel bike variant) and tanks are simulated; boats, helicopters and jets are recognised
  for routing and camera selection, with their full `applyAction` simulation still to land
  (`DEFERRED.md`).
- **The command ring** (`command.rs`) — a bounded multi-consumer *broadcast* queue of
  `{target handle, command-id, float payload, aux}` records, plus the per-class `handle_command`
  switch that turns a command-id into a field write on `VehicleControls`.
- **The camera modes** (`camera.rs`) — mode selection driven by the *ridden object* (`CameraMode`),
  a per-mode `CameraPreset`, and `chase_pose` producing eye/target/FOV.
- **The hijack FSM + turret aim** (`hijack.rs`) and the **engine-side bodies** for the `Vehicle.*` /
  `Camera.*` Lua namespaces (`lua_surface.rs`).
- **The ECS glue** (`system.rs`) — `pump_car_ring` / `pump_boat_heli_ring` then `drive_step_system`.

Wheel raycasts go through `mercs2_core::PhysicsQuery` (the silo-7 seam), so the crate never depends
on a physics engine crate.

## Where it comes from

The retail PC exe is the oracle. The crate's source cites
`docs/reverse_engineer/vehicle_code_map.md` (+ `docs/data/vehicle_code_map.json`) and
`camera_code_map.md` throughout.

Key decoded facts the source records:

- The drive model is a **custom raycast sim built from nine `hkpUnaryAction`-derived actor classes —
  NOT the Havok Vehicle Kit** (there are zero `hkpVehicle*` classes on PC). `CarPhysicsV2` applyAction
  = `FUN_0044db60`, `TankPhysics` = `FUN_00454d80`, `BoatPhysics` = `FUN_00447260`,
  `HelicopterPhysics` = `FUN_00453760`.
- A Ghidra **noreturn-on-sqrt defect** had previously truncated every physics function at its first
  `sqrt`; the math decoded here is post-fix (code map §0).
- Wheel raycast scheduler `FUN_0044d9b0` (full raycast when fast, else one wheel/frame round-robin);
  per-wheel ray `FUN_0044e2c0`; ground-normal average `FUN_0044cc90`; per-axle friction
  `FUN_00449dc0` / `FUN_004571b0`; traction impulse `FUN_0044a970`.
- The donut / turn **sine LUT** is `DAT_00cf2900`, indexed `(t·f) & 0x1fff` ⇒ 0x2000 entries over
  one full turn. Only the size and mask are read from the exe; the values are a plain sine.
- Tuning blocks: `_CarPhysicsV2` = 0x18c bytes / 99 dwords (registrar `FUN_0063e8b0`, ctor
  `FUN_00449460`), `_TankPhysics` = 0x78 (`FUN_0063e980`). `tuning.rs` carries the decoded
  dword → runtime-field scatter map.
- The command rings are **standalone broadcast queues** on PC (not fields inside the per-class
  controller singletons — that was the Xbox layout): car/tank ring `0x011C0230` (cap 0x200, enqueue
  `FUN_00538c90`), boat/heli ring `0x011C2478` (cap 100, enqueue `FUN_00538d20`), subscriber
  registration `FUN_004068d0`, pump `FUN_00532f80` from the game layer (idx 4) of the master tick
  (call site `0x004c990c`). Command IDs (`cmd::TURN = 0x3483DBF1`, …) are bit-identical Xbox↔PC and
  used verbatim.
- Camera **modes are data, not code branches**: each controlled object carries a mode-specific
  component (`CameraCarPreset` `FUN_006401b0` / `CameraTank` / `CameraTurret` / `CameraHelicopter` /
  `HumanCameraModifier`), and `FUN_0060f6d0` selects by the ridden object (`4` = vehicle chase,
  `3` = on-foot).
- The Lua binding seam: `crates/mercs2_script/src/bindings/vehicle.rs` (`REQUIRED` table VA
  `0xB98918`, 40 cfuncs) and `.../camera.rs` (table VA `0xB9A530`, 7 cfuncs) wrap the real bodies in
  `lua_surface.rs`. Seat/enter-exit state mirrors the ring-1 applier `FUN_0053f110`.

## Usage

```rust
use glam::Vec3;
use mercs2_core::{Transform, World};
use mercs2_vehicle::{
    cmd, drive_step_system, pump_car_ring, ChassisBody, CommandRecord, CommandRing, DonutLut,
    Vehicle, VehicleClass, VehicleControls, VehicleRuntime, VehicleTuning, Wheel, WheelSet,
};
use mercs2_vehicle::lua_surface;

let mut world = World::new();

// Spawn a car with the full drive + seating component set.
let car = lua_surface::spawn_vehicle(
    &mut world,
    Transform::from_translation(Vec3::new(0.0, 0.85, 0.0)),
    Vehicle::new(VehicleClass::Car, 0x1000),   // gameplay handle (uGuid)
    ChassisBody::new(1200.0),                  // mass, kg
    VehicleControls::default(),
    WheelSet(vec![
        Wheel::new(Vec3::new(-0.8, 0.0,  1.3), /*front*/ true,  /*steered*/ true,  /*powered*/ false),
        Wheel::new(Vec3::new( 0.8, 0.0,  1.3), true,  true,  false),
        Wheel::new(Vec3::new(-0.8, 0.0, -1.3), false, false, true),
        Wheel::new(Vec3::new( 0.8, 0.0, -1.3), false, false, true),
    ]),
    VehicleTuning::default(),
    VehicleRuntime::new(),
    lua_surface::default_car_seating(),
);

// Drive it over the command ring: subscribe once, enqueue, pump, then step.
let mut ring = CommandRing::car();
let channel = ring.subscribe().expect("ring has a free subscriber slot");
ring.enqueue(CommandRecord::new(0x1000, cmd::TURN, 0.25));

let lut = DonutLut::new();
// `phys` is any &dyn mercs2_core::PhysicsQuery (the silo-7 seam) — the wheel rays land on it.
pump_car_ring(&mut world, &mut ring, channel);
drive_step_system(&mut world, phys, &lut, 1.0 / 60.0);

let speed = world.get::<&VehicleRuntime>(car).unwrap().fwd_speed;
```

Seat / camera surface (the Lua bodies), same `World`:

```rust
use mercs2_vehicle::{lua_surface, CameraMode, SeatKind};

lua_surface::enter(&mut world, car, player, SeatKind::Driver);   // -> Option<seat index>
assert_eq!(lua_surface::camera_mode(&world, player), CameraMode::Car);
let pose = lua_surface::vehicle_camera_pose(&world, player, /*look_yaw*/ 0.0, /*look_pitch*/ 0.0);
lua_surface::exit(&mut world, player);
```

Order matters: pump the rings **first**, then run the drive step — that is where `FUN_00532f80`
sits relative to the vehicle drive step in the master tick (`system.rs`).

## Modules

| module | owns |
|---|---|
| `lut` | the donut/turn sine LUT (`DAT_00cf2900`, 8192 entries, `& 0x1fff`). |
| `tuning` | the `_CarPhysicsV2` (0x18c) / `_TankPhysics` (0x78) tuning-block → actor-field map. |
| `components` | the vehicle ECS component set (defined here, per the silo carve rule). |
| `command` | the broadcast command rings + the per-class `HandleCommand` switch. |
| `drive` | the decoded raycast/suspension/per-axle-friction/torque-falloff/donut drive math. |
| `camera` | the data-driven camera modes, presets, and the chase pose. |
| `hijack` | the hijack lifecycle FSM (`HijackFsm`/`HijackState`) and `TurretAim`. |
| `lua_surface` | real engine bodies behind the `Vehicle.*` / `Camera.*` Lua namespaces. |
| `system` | the ECS drive-step system + the command-ring pumps. |

## Notes / gotchas

- **Tuning defaults and the camera preset math are placeholders, not decoded values.** The
  `_CarPhysicsV2` field *names* are stripped on the PC build, and the camera preset float layout /
  look-axis apply / pitch clamp are string-stripped. Everything unread is marked `CONFIRM-LIVE` in
  the source and listed in `DEFERRED.md` under "Faithful blockers" (recover by breaking
  `0x00449460` and diffing the 0x18c block). Treat the numbers as structural, not authored.
- The command-**ID hash function** is unknown; the constants in `command::cmd` are used verbatim
  because they are exactly what the ring compares against.
- The rings are **broadcast**: every subscriber sees every record once, and a record is only
  compacted away when no subscriber still carries its bit. Subscribe once (`CommandRing::subscribe`)
  and keep the channel id — do not re-subscribe per frame.
- The exe's wheel-ray scheduler is **amortised** (one wheel per frame when slow); the reimpl mirrors
  that rather than raycasting every wheel every frame. Raycasting all wheels always is listed in
  `DEFERRED.md` as a non-faithful improvement.
- Only `Car`/`Bike` and `Tank` are simulated today. `Boat`/`Helicopter`/`Jet` are routed by the
  command pump and select camera modes, but their `applyAction` sims are a later pass.
- Canonical space is game space (left-handed, +Y up, +Z forward) — see the `forward_axis_is_plus_z`
  test.
