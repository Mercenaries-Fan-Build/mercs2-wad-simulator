//! The custom raycast drive model (`vehicle_code_map.md` §4) — **NOT** the Havok Vehicle Kit.
//!
//! Faithful to the decoded CarPhysicsV2 `applyAction` flow (`FUN_0044db60`):
//!   1. flip gravityFactor when upside-down; average wheel contact normals → ground normal
//!      (`FUN_0044cc90`); read chassis speed at the front-axle point (`v + ω×r`);
//!   2. wheel raycast scheduler (`FUN_0044d9b0`) — full when fast, else ONE wheel/frame round-robin;
//!      per-wheel ray from the hardpoint along `−up × (restLen+radius)` (`FUN_0044e2c0`);
//!   3. per-wheel suspension spring + per-**axle** friction (`FUN_00449dc0` / `FUN_004571b0`), lateral
//!      grip scaled by the cubic steering blend `1 − (1−s)³`;
//!   4. drive/traction impulse (`FUN_0044a970`): needs ≥ half the wheels grounded; axle force with the
//!      **linear torque falloff** `speedRatio = clamp01((vmax − v)/(vmax·K))` → zero at MaxSpeed, plus
//!      the donut sine-LUT lateral wobble when handbrake+throttle.
//!
//! The chassis body is integrated here against `&dyn PhysicsQuery` (the wheel rays). Tank
//! (`FUN_00454d80`) shares the raycast/suspension/friction machinery and swaps Ackermann steering
//! for track-differential yaw.

use glam::{Quat, Vec3};
use mercs2_core::{PhysicsQuery, Transform};

use crate::components::{ChassisBody, VehicleControls, VehicleRuntime, Wheel};
use crate::lut::DonutLut;
use crate::tuning::{AxleTuning, VehicleTuning};

/// Above this squared chassis speed the scheduler raycasts every wheel each frame; below it, one
/// wheel/frame round-robins (`FUN_0044d9b0`, threshold `_DAT_00beb5dc`). CONFIRM-LIVE value.
const FULL_RAYCAST_SPEED2: f32 = 25.0; // (5 m/s)²

/// Donut wobble phase step per frame (indexes the sine LUT). CONFIRM-LIVE.
const DONUT_PHASE_STEP: u32 = 0x140;
/// Angular velocity damping per second (chassis stabilisation). CONFIRM-LIVE.
const ANG_DAMP: f32 = 2.5;
/// Rolling resistance (horizontal linear damping /s) — small, so top speed settles just under vmax.
const ROLL_DAMP: f32 = 0.25;
/// Tank track-differential thrust per side at full stick (N). Left/right tracks push opposite ways
/// to yaw the hull; the thrust is longitudinal, so the (strong) lateral track friction resists the
/// resulting slide without cancelling the turn. CONFIRM-LIVE.
const TANK_DIFF_FORCE: f32 = 60_000.0;

/// The per-frame drive context: mutable chassis + wheels + runtime, read-only inputs + tuning +
/// physics query + the sine LUT. Bundles what one `applyAction` call needs.
pub struct DriveSim<'a> {
    pub xform: &'a mut Transform,
    pub body: &'a mut ChassisBody,
    pub ctrl: &'a VehicleControls,
    pub wheels: &'a mut [Wheel],
    pub tuning: &'a VehicleTuning,
    pub rt: &'a mut VehicleRuntime,
    pub phys: &'a dyn PhysicsQuery,
    pub lut: &'a DonutLut,
}

impl<'a> DriveSim<'a> {
    #[inline]
    fn up(&self) -> Vec3 {
        (self.xform.rotation * Vec3::Y).normalize_or_zero()
    }
    #[inline]
    fn forward(&self) -> Vec3 {
        (self.xform.rotation * Vec3::Z).normalize_or_zero()
    }
    #[inline]
    fn right(&self) -> Vec3 {
        (self.xform.rotation * Vec3::X).normalize_or_zero()
    }
    /// World-space centre of mass (`translation + R·comOffset`).
    #[inline]
    fn com(&self) -> Vec3 {
        self.xform.translation + self.xform.rotation * self.tuning.com_offset
    }
    #[inline]
    fn axle(&self, front: bool) -> AxleTuning {
        if front {
            self.tuning.front
        } else {
            self.tuning.rear
        }
    }

    /// Step 1 — flip gravityFactor when upside-down (`FUN_0044db60`: `+0xf4 < 0`).
    fn pre_step(&mut self) {
        self.body.gravity_factor = if self.up().y < 0.0 { -1.0 } else { 1.0 };
    }

    /// Step 2 — wheel raycast scheduler + per-wheel raycast. Full pass when fast, else one wheel.
    fn raycast_wheels(&mut self) {
        let up = self.up();
        let full = self.body.linvel.length_squared() > FULL_RAYCAST_SPEED2;
        let n = self.wheels.len();
        if n == 0 {
            return;
        }
        let indices: Vec<usize> = if full {
            (0..n).collect()
        } else {
            self.rt.rr_index = (self.rt.rr_index + 1) % n;
            vec![self.rt.rr_index]
        };
        for i in indices {
            let front = self.wheels[i].front;
            let radius = self.axle(front).radius;
            let rest = self.axle(front).rest_length;
            let max_dist = rest + radius;
            let hp_world = self.xform.translation + self.xform.rotation * self.wheels[i].hardpoint;
            let down = -up;
            let w = &mut self.wheels[i];
            // The raycast only detects/updates the contact PLANE (point + normal). The actual
            // compression is recomputed every frame in `suspension` from this cached plane, so a
            // wheel not raycast this frame (the round-robin amortisation) still suspends correctly.
            match self.phys.raycast(hp_world, down, max_dist) {
                Some(hit) => {
                    w.contact = true;
                    w.contact_point = hit.point;
                    w.contact_normal = hit.normal.normalize_or_zero();
                }
                None => {
                    w.contact = false;
                    w.compression = 0.0;
                    w.prev_compression = 0.0;
                    w.normal_load = 0.0;
                }
            }
        }
    }

    /// Step 1b — average the grounded wheel contact normals into the ground normal (`FUN_0044cc90`)
    /// and count grounded wheels (the drive gate reads this).
    fn ground_state(&mut self) {
        let mut acc = Vec3::ZERO;
        let mut count = 0usize;
        for w in self.wheels.iter() {
            if w.contact {
                acc += w.contact_normal;
                count += 1;
            }
        }
        self.rt.ground_normal = if count > 0 {
            acc.normalize_or_zero()
        } else {
            Vec3::Y
        };
        self.rt.grounded = count;
    }

    /// Step 3a — per-wheel suspension spring (`FUN_0044f680`): `F = k·compression + c·d(compression)`
    /// along `up`, applied at the hardpoint. Records the normal load for the friction clamp.
    fn suspension(&mut self, dt: f32) {
        let com = self.com();
        let n = self.wheels.len();
        for i in 0..n {
            if !self.wheels[i].contact {
                continue;
            }
            let front = self.wheels[i].front;
            let axle = self.axle(front);
            let max_dist = axle.rest_length + axle.radius;
            let hp_world = self.xform.translation + self.xform.rotation * self.wheels[i].hardpoint;
            // Ride height along the contact normal above the cached contact plane; compression is
            // how far the wheel is pushed up into its travel.
            let normal = self.wheels[i].contact_normal;
            let ride = (hp_world - self.wheels[i].contact_point).dot(normal);
            let comp = (max_dist - ride).clamp(0.0, max_dist);
            let prev = self.wheels[i].prev_compression;
            self.wheels[i].prev_compression = comp;
            self.wheels[i].compression = comp;
            let comp_vel = (comp - prev) / dt.max(1e-6);
            let force = (axle.susp_strength * comp + axle.susp_damp * comp_vel).max(0.0);
            self.wheels[i].normal_load = force;
            // Spring pushes along the contact normal (≈ up on flat ground).
            self.body.apply_impulse_at(normal * force * dt, hp_world, com);
        }
    }

    /// Step 3b — per-wheel lateral (cornering) friction, clamped by the wheel's normal load and the
    /// cubic steering blend `1 − (1−s)³` (`FUN_004571b0`). `steer_angle` rotates the front-wheel side
    /// axis (Ackermann) using the sine LUT; rear/tank wheels pass `0`.
    fn lateral_friction(&mut self, steer_angle: f32, dt: f32) {
        let com = self.com();
        let right = self.right();
        let forward = self.forward();
        let grounded = self.rt.grounded.max(1);
        let eff_mass = self.body.mass / grounded as f32;
        let s = (steer_angle / self.tuning.max_steer.max(1e-3)).abs().clamp(0.0, 1.0);
        let steer_blend = 1.0 - (1.0 - s).powi(3); // cubic grip blend

        let n = self.wheels.len();
        for i in 0..n {
            let w = self.wheels[i];
            if !w.contact {
                continue;
            }
            let axle = self.axle(w.front);
            // Steered wheels rotate their side axis toward forward by `steer_angle` (LUT sin/cos).
            let side = if w.steered && steer_angle != 0.0 {
                (right * self.lut.cos(steer_angle) + forward * self.lut.sin(steer_angle))
                    .normalize_or_zero()
            } else {
                right
            };
            let v_at = self.body.point_velocity(w.contact_point, com);
            let side_speed = v_at.dot(side);
            let blend = if w.steered { steer_blend.max(0.15) } else { 1.0 };
            let max_j = w.normal_load * axle.friction_side * blend * dt;
            let j = (-side_speed * eff_mass).clamp(-max_j, max_j);
            self.body.apply_impulse_at(side * j, w.contact_point, com);
        }
    }

    /// Step 4 — drive/traction impulse (`FUN_0044a970`): needs ≥ half the wheels grounded; applies
    /// the axle force with the **linear torque falloff to MaxSpeed** and, in donut mode, the sine-LUT
    /// lateral wobble. Also folds in braking. Returns the forward speed for the runtime.
    fn forward_drive(&mut self, dt: f32) {
        let n = self.wheels.len();
        if n == 0 || self.rt.grounded * 2 < n {
            self.rt.fwd_speed = self.body.linvel.dot(self.forward());
            return;
        }
        let up = self.rt.ground_normal;
        let forward = self.forward();
        // Forward direction projected onto the ground plane.
        let fwd_ground = (forward - up * forward.dot(up)).normalize_or_zero();
        let com = self.com();
        let v = self.body.linvel.dot(fwd_ground);
        self.rt.fwd_speed = v;

        // Reverse regime: brake-as-reverse when nearly stopped and only the brake is held.
        let mut throttle = self.ctrl.accel;
        self.rt.reverse = false;
        if throttle.abs() < 1e-3 && self.ctrl.brake > 1e-3 && v <= 0.2 {
            self.rt.reverse = true;
            throttle = -self.ctrl.brake;
        }
        let vmax = if self.rt.reverse {
            self.tuning.max_speed_reverse
        } else {
            self.tuning.max_speed
        };
        // Linear torque falloff → zero at MaxSpeed.
        let speed_ratio = ((vmax - v.abs()) / (vmax * self.tuning.falloff_k)).clamp(0.0, 1.0);

        // Donut heat ramps while handbrake + throttle are held.
        let donut_active = self.ctrl.handbrake > 0.5 && throttle.abs() > 0.1;
        if donut_active {
            self.rt.donut_heat = (self.rt.donut_heat + dt).min(1.0);
        } else {
            self.rt.donut_heat = (self.rt.donut_heat - dt * 2.0).max(0.0);
        }
        let donut_ramp = 1.0 + self.rt.donut_heat * self.tuning.donut_boost;

        // Σ(wheelDriveTorque · contact) / radius over powered, grounded wheels.
        let mut sum = 0.0f32;
        let mut centroid = Vec3::ZERO;
        let mut cn = 0.0f32;
        for w in self.wheels.iter() {
            if w.powered && w.contact {
                let axle = self.axle(w.front);
                sum += axle.drive_torque / axle.radius.max(1e-3);
                centroid += w.contact_point;
                cn += 1.0;
            }
        }
        if cn > 0.0 {
            centroid /= cn;
            let sign = if self.rt.reverse { -1.0 } else { 1.0 };
            let axle_force =
                self.tuning.drive_blend * donut_ramp * speed_ratio * throttle.abs() * sum;
            // Apply the traction force at the axle's longitudinal position but at COM height, so a
            // pure forward force produces pure forward acceleration (no spurious pitch lever). The
            // squat/dive that the exe gets from the low contact point is a CONFIRM-LIVE refinement.
            let apply_pt = Vec3::new(centroid.x, com.y, centroid.z);
            self.body
                .apply_impulse_at(fwd_ground * axle_force * sign * dt, apply_pt, com);

            // Donut sine-LUT lateral wobble × DonutSidePower.
            if self.rt.donut_heat > 0.0 {
                self.rt.donut_phase = self.rt.donut_phase.wrapping_add(DONUT_PHASE_STEP);
                let wobble = self.lut.sample(self.rt.donut_phase)
                    * self.tuning.donut_side_power
                    * self.rt.donut_heat;
                let right = self.right();
                self.body.apply_impulse_at(right * wobble * dt, centroid, com);
            }
        }

        // Braking: oppose forward velocity, clamped by brake torque, when not using brake-as-reverse.
        if self.ctrl.brake > 1e-3 && !self.rt.reverse {
            let eff_mass = self.body.mass / self.rt.grounded.max(1) as f32;
            for w in self.wheels.iter() {
                if !w.contact {
                    continue;
                }
                let axle = self.axle(w.front);
                let v_at = self.body.point_velocity(w.contact_point, com);
                let long_speed = v_at.dot(fwd_ground);
                let max_j = axle.brake_torque * self.ctrl.brake * dt;
                let j = (-long_speed * eff_mass).clamp(-max_j, max_j);
                self.body
                    .apply_impulse_at(fwd_ground * j, w.contact_point, com);
            }
        }

        self.rt.moving_fwd = v > 0.0;
    }

    /// Tank track differential (`TankPhysics` steering): left vs right tracks push opposite ways
    /// along the ground-forward axis, yawing the hull. Longitudinal, so lateral friction resists the
    /// slide without cancelling the turn.
    fn tank_differential(&mut self, turn: f32, dt: f32) {
        if turn.abs() < 1e-3 {
            return;
        }
        let up = self.rt.ground_normal;
        let forward = self.forward();
        let fwd_ground = (forward - up * forward.dot(up)).normalize_or_zero();
        let com = self.com();
        let n = self.wheels.len();
        for i in 0..n {
            let w = self.wheels[i];
            if !w.contact {
                continue;
            }
            // Left track (hardpoint.x < 0) and right track (> 0) thrust opposite ways.
            let side_sign = if w.hardpoint.x >= 0.0 { 1.0 } else { -1.0 };
            let thrust = turn * side_sign * TANK_DIFF_FORCE * dt;
            self.body
                .apply_impulse_at(fwd_ground * thrust, w.contact_point, com);
        }
    }

    /// Step 5 — integrate the chassis body: gravity, linear/angular advance, damping.
    fn integrate(&mut self, dt: f32) {
        // Gravity (× gravityFactor).
        self.body.linvel.y += self.tuning.gravity * self.body.gravity_factor * dt;
        // Rolling resistance on the horizontal velocity only.
        let horiz = Vec3::new(self.body.linvel.x, 0.0, self.body.linvel.z);
        self.body.linvel -= horiz * (ROLL_DAMP * dt).min(1.0);
        // Angular damping.
        self.body.angvel *= (1.0 - ANG_DAMP * dt).max(0.0);

        // Advance transform.
        self.xform.translation += self.body.linvel * dt;
        if self.body.angvel.length_squared() > 0.0 {
            let dq = Quat::from_scaled_axis(self.body.angvel * dt);
            self.xform.rotation = (dq * self.xform.rotation).normalize();
        }
    }
}

/// The `hkpUnaryAction::applyAction(stepInfo)` abstraction (vtable slot 3; `stepInfo+8 = dt`). Each
/// actor class implements its per-frame drive step over the shared raycast/suspension/friction
/// helpers on [`DriveSim`].
pub trait VehicleActor {
    /// Class name (for diagnostics / the reflection registrar).
    fn class_name(&self) -> &'static str;
    /// Apply one fixed step of drive physics.
    fn apply_action(&self, sim: &mut DriveSim<'_>, dt: f32);
}

/// `CarPhysicsV2` — actor C (`applyAction FUN_0044db60`). Ackermann front-wheel steering through the
/// steered-wheel lateral friction; rear-wheel drive with the torque-falloff engine.
pub struct CarActor;

impl VehicleActor for CarActor {
    fn class_name(&self) -> &'static str {
        "CarPhysicsV2"
    }
    fn apply_action(&self, sim: &mut DriveSim<'_>, dt: f32) {
        sim.pre_step();
        sim.raycast_wheels();
        sim.ground_state();
        sim.suspension(dt);
        let steer_angle = sim.ctrl.turn.clamp(-1.0, 1.0) * sim.tuning.max_steer;
        sim.lateral_friction(steer_angle, dt);
        sim.forward_drive(dt);
        sim.integrate(dt);
    }
}

/// `TankPhysics` — actor G (`applyAction FUN_00454d80`). Shares the raycast/suspension/friction
/// machinery (6 track-contact raycasts round-robin) but steers by **track differential**: a direct
/// yaw impulse proportional to the turn input (turns in place), and no Ackermann steer angle.
pub struct TankActor;

impl VehicleActor for TankActor {
    fn class_name(&self) -> &'static str {
        "TankPhysics"
    }
    fn apply_action(&self, sim: &mut DriveSim<'_>, dt: f32) {
        sim.pre_step();
        sim.raycast_wheels();
        sim.ground_state();
        sim.suspension(dt);
        // No steer angle — tracks resist sideways and the turn is a differential yaw.
        sim.lateral_friction(0.0, dt);
        sim.forward_drive(dt);
        // Track-differential yaw: the two tracks apply opposite LONGITUDINAL thrust (left track vs
        // right track), which yaws the hull. Because it is longitudinal, the strong lateral track
        // friction resists the induced slide without cancelling the turn — so the tank turns in
        // place. Applied at each grounded track-contact point.
        if sim.rt.grounded * 2 >= sim.wheels.len().max(1) {
            let turn = sim.ctrl.turn.clamp(-1.0, 1.0);
            sim.tank_differential(turn, dt);
        }
        sim.integrate(dt);
    }
}
