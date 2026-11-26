//! The drive, steering and flight math of the four vehicle classes, as the PC executable computes
//! it. Pure functions over a template's named physics fields; the Driving page draws their results.
//!
//! Read from pc-v10. Car: engine speed `FUN_0044d430`, the per-wheel torque shares `FUN_0044fc10`,
//! the shift rule `FUN_0044fda0`, the torque curve `FUN_0044fe20`, steering `FUN_0044f680` and its
//! smoothing `FUN_00714d90`. Tank: drive `FUN_00455aa0`, steering `FUN_004555e0`. Boat: thrust
//! `FUN_00447350` with the engine curve `FUN_004464f0`, steering `FUN_00447f00`. Helicopter:
//! translation, lift and yaw `FUN_00451ca0`, the yaw curve `FUN_00452fe0`, attitude `FUN_00452540`.
//! Plane: the `Airstrike.Flyby` worker `0x62A440` and its flight update `0x62C130`. Each constant
//! below names the address it was read at.

/// Wheel angular speed in rad/s to rpm, 60 / 2π (`0xBEB430`).
pub const RPM_PER_RAD_S: f32 = 9.549_296_4;
/// The wheels' torque shares add up to this (`0xB92874`).
pub const SHARE_SUM: f32 = 2.0;
/// Drive fades to nothing over this fraction of the speed cap, below the cap (`0xB98748`).
pub const CAP_FADE: f32 = 0.05;
/// Steering is smoothed in steps of this many seconds (`0xBCD278`).
pub const STEER_STEP: f32 = 1.0 / 60.0;
/// km/h per m/s (`0xBA8734`).
pub const KMH_PER_MS: f32 = 3.6;

/// How many wheels each axle has, and their radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axles {
    pub front_wheels: usize,
    pub rear_wheels: usize,
    pub front_radius: f32,
    pub rear_radius: f32,
}

/// The fields the gearbox and the torque curve read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gearbox {
    pub primary: f32,
    pub reverse: f32,
    pub forward: [f32; 5],
    pub upshift_rpm: f32,
    pub downshift_rpm: f32,
    pub min_rpm: f32,
    pub opt_rpm: f32,
    pub max_rpm: f32,
    pub engine_torque: f32,
    /// `TorqueFactorAtMinRPM`. The executable fills the max-rpm slot from this name too.
    pub torque_factor: f32,
    /// `RearToFrontTorqueDistrib`: the rear axle's share of drive torque.
    pub rear_share: f32,
    /// km/h.
    pub max_speed: f32,
}

/// One wheel's share of drive torque: the rear axle's wheels split `rear_share × 2`, the front
/// axle's the rest.
pub fn wheel_share(axles: &Axles, rear_share: f32, rear: bool) -> f32 {
    let (part, wheels) = if rear { (rear_share, axles.rear_wheels) } else { (1.0 - rear_share, axles.front_wheels) };
    if wheels == 0 { 0.0 } else { part * SHARE_SUM / wheels as f32 }
}

/// Engine rpm per km/h of road speed in a gear of this ratio, rolling straight: every wheel turns
/// at the chassis speed over its radius, weighted by its torque share.
pub fn rpm_per_kmh(g: &Gearbox, axles: &Axles, ratio: f32) -> f32 {
    let rear = wheel_share(axles, g.rear_share, true) * axles.rear_wheels as f32 / axles.rear_radius;
    let front = wheel_share(axles, g.rear_share, false) * axles.front_wheels as f32 / axles.front_radius;
    g.primary * ratio * RPM_PER_RAD_S * (rear + front) / KMH_PER_MS
}

/// The road speed in km/h at which forward gear `gear` (1 to 5) reaches the upshift rpm.
pub fn upshift_kmh(g: &Gearbox, axles: &Axles, gear: usize) -> f32 {
    g.upshift_rpm / rpm_per_kmh(g, axles, g.forward[gear - 1])
}

/// The full-throttle run from rest to the speed cap as (km/h, rpm) points: up each gear to the
/// upshift rpm, straight down to the next gear's rpm at that speed, and on. The last point is at
/// the cap. Rpm is the wheel-derived value the shift rule compares, not the clamped engine rpm.
pub fn full_throttle_path(g: &Gearbox, axles: &Axles) -> Vec<(f32, f32)> {
    let mut path = vec![(0.0, 0.0)];
    for gear in 1..=5 {
        let slope = rpm_per_kmh(g, axles, g.forward[gear - 1]);
        let shift_at = g.upshift_rpm / slope;
        if gear == 5 || shift_at >= g.max_speed {
            path.push((g.max_speed, g.max_speed * slope));
            break;
        }
        path.push((shift_at, g.upshift_rpm));
        path.push((shift_at, shift_at * rpm_per_kmh(g, axles, g.forward[gear])));
    }
    path
}

/// Engine torque and engine friction at an rpm, in the units of `EngineTorque`. Net drive at a
/// throttle is `torque × throttle − friction`.
pub fn torque_and_friction(g: &Gearbox, rpm: f32) -> (f32, f32) {
    let e = g.engine_torque;
    if rpm >= g.max_rpm {
        return (0.0, 0.3 * e);
    }
    if rpm < g.opt_rpm {
        let x = (g.opt_rpm - rpm) / (g.opt_rpm - g.min_rpm);
        (e * (1.0 + (g.torque_factor - 1.0) * x * x), e * (0.1 - 0.05 * x))
    } else {
        let y = (rpm - g.opt_rpm) / (g.max_rpm - g.opt_rpm);
        (e * (1.0 + (g.torque_factor - 1.0) * y * y), e * (0.1 + 0.2 * y))
    }
}

/// What the driver's feet are doing, as the steering loss rule sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pedals {
    Throttle,
    Coasting,
    Braking,
}

/// The fields the steering lock reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steering {
    /// Radians.
    pub max_angle: f32,
    pub high_speed_loss: f32,
    pub loss_brake_multi: f32,
    pub loss_no_throttle_multi: f32,
    /// km/h.
    pub max_speed: f32,
}

/// The steering lock in radians at a road speed: the full angle at rest, less the loss in
/// proportion to speed over the cap, the loss scaled when braking or coasting.
pub fn lock(s: &Steering, pedals: Pedals, kmh: f32) -> f32 {
    let loss = s.high_speed_loss.clamp(0.0, 1.0)
        * match pedals {
            Pedals::Throttle => 1.0,
            Pedals::Coasting => s.loss_no_throttle_multi,
            Pedals::Braking => s.loss_brake_multi,
        };
    s.max_angle * (1.0 - loss * (kmh / s.max_speed).clamp(0.0, 1.0))
}

/// The share of its error the steering angle still has after `seconds`, when it keeps the fraction
/// `inertia` each step. Turning in reaches `1 − remaining`; a release has `remaining` of the lock.
pub fn remaining(inertia: f32, seconds: f32) -> f32 {
    inertia.powf(seconds / STEER_STEP)
}

/// The steering rate in rad/s of the rate-limited mode (`SteeringInertia` below 0) at a road speed.
pub fn steer_rate(steer_speed: f32, low_speed_steer_speed: f32, low_speed_limit: f32, s: &Steering, kmh: f32) -> f32 {
    let low = if low_speed_limit > 0.0 { 1.0 - (kmh / low_speed_limit).min(1.0) } else { 0.0 };
    let loss = s.high_speed_loss.clamp(0.0, 1.0) * kmh / s.max_speed;
    (steer_speed + (low_speed_steer_speed - steer_speed) * low) * (1.0 - loss)
}

// ── tank and boat engines ────────────────────────────────────────────────────────────────────────

/// A tank's or a boat's thrust is its torque-curve value times `EngineTorque` times this
/// (`0xB984A8`).
pub const THRUST_SCALE: f32 = 200.0;

/// The engine torque curve a tank and a boat share: `EngineTorqueRatio0..3` at speed ratios 0,
/// `EngineTorquePos1`, `EngineTorquePos2` and 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorqueCurve {
    pub ratios: [f32; 4],
    pub pos: [f32; 2],
}

impl TorqueCurve {
    /// The curve's four knots, as speed ratio and torque ratio.
    pub fn knots(&self) -> [(f32, f32); 4] {
        [(0.0, self.ratios[0]), (self.pos[0], self.ratios[1]), (self.pos[1], self.ratios[2]), (1.0, self.ratios[3])]
    }

    /// The torque ratio at a speed ratio: straight lines between the knots, in the segment that ends
    /// at the first of the two middle knots past `x`, else the last (`0x455f20`, `0x4560aa`).
    pub fn at(&self, x: f32) -> f32 {
        let k = self.knots();
        let end = if x < k[1].0 { 1 } else if x < k[2].0 { 2 } else { 3 };
        let ((x0, y0), (x1, y1)) = (k[end - 1], k[end]);
        if x1 == x0 {
            return y1;
        }
        y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    }
}

/// A tank's or a boat's thrust at a speed with the throttle held and settled: the curve value at
/// the speed over the cap, times the engine torque, times [`THRUST_SCALE`]. Nothing at or past the
/// cap: both classes stop driving there (tank `0x45616b`, boat `0x44754b`).
pub fn thrust(curve: &TorqueCurve, engine_torque: f32, max_speed: f32, kmh: f32) -> f32 {
    if max_speed <= 0.0 || kmh >= max_speed {
        return 0.0;
    }
    curve.at(kmh / max_speed) * engine_torque * THRUST_SCALE
}

/// What a tank's steering keeps at a speed: `1 − HiSpeedSteerLoss × speed / MaxSpeed`, held between
/// 0 and 1 (`0x4556da`).
pub fn tank_steer_factor(hi_speed_steer_loss: f32, max_speed: f32, kmh: f32) -> f32 {
    (1.0 - hi_speed_steer_loss * (kmh / max_speed).min(1.0)).clamp(0.0, 1.0)
}

/// The fields a boat's steering gain reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatSteering {
    pub steering: f32,
    pub no_speed_steering: f32,
    pub low_speed_steering: f32,
    /// km/h.
    pub low_speed_limit: f32,
    pub high_speed_steer_loss: f32,
    /// km/h.
    pub max_speed: f32,
    pub max_speed_reverse: f32,
}

/// The speed in km/h under which a boat counts as stopped, and past which backwards it counts as
/// reversing: a quarter of `MaxSpeedReverse`, at most 10 (`0x447f06`).
pub fn boat_stopped_under(max_speed_reverse: f32) -> f32 {
    (0.25 * max_speed_reverse).min(10.0)
}

/// A boat's steering gain at a speed, off the handbrake, with the engine state and throttle at
/// `throttle` (0 coasting, 1 held): the `Steering` term that grows to `LowSpeedLimit` and loses
/// `HighSpeedSteerLoss` toward the cap, the `LowSpeedSteering` term the engine adds below
/// `LowSpeedLimit`, and `NoSpeedSteering` while stopped (`FUN_00447f00`, `0x448081`–`0x4480e0`).
pub fn boat_steer_gain(s: &BoatSteering, throttle: f32, kmh: f32) -> f32 {
    let a = if s.low_speed_limit > 0.0 { (kmh / s.low_speed_limit).min(1.0) } else { 1.0 };
    let at_cap = if s.max_speed > 0.0 { (kmh / s.max_speed).min(1.0) } else { 1.0 };
    let stopped = if kmh < boat_stopped_under(s.max_speed_reverse) { s.no_speed_steering } else { 0.0 };
    s.steering * (1.0 - s.high_speed_steer_loss * at_cap) * a + s.low_speed_steering * throttle * throttle * (1.0 - a) + stopped
}

// ── helicopter ───────────────────────────────────────────────────────────────────────────────────

/// The fields a helicopter's yaw rate and banking read. Speeds are the body's, in the game's
/// units per second; angles are radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeliTurn {
    pub max_yaw_speed: f32,
    pub low_speed_yaw_speed: f32,
    pub low_speed_limit: f32,
    pub hi_speed_yaw_loss: f32,
    pub max_fwd_speed: f32,
    pub bank_speed_limit: f32,
    pub max_roll_bank: f32,
}

/// The yaw rate a full yaw input asks for at a horizontal speed: from `LowSpeedYawSpeed` at rest to
/// `MaxYawSpeed` at `LowSpeedLimit`, then losing `HiSpeedYawLoss` of it by `MaxFwdSpeed`
/// (`FUN_00452fe0`).
pub fn heli_yaw_rate(t: &HeliTurn, speed: f32) -> f32 {
    let limit = t.low_speed_limit;
    if limit > speed {
        return (1.0 - speed / limit) * t.low_speed_yaw_speed + (speed / limit) * t.max_yaw_speed;
    }
    let span = t.max_fwd_speed - limit;
    let lost = if span > 0.0 { ((speed - limit) / span).min(1.0) } else { 1.0 };
    t.max_yaw_speed * (1.0 - lost * t.hi_speed_yaw_loss)
}

/// The extra roll a helicopter banks with at a horizontal speed: none up to `BankSpeedLimit`,
/// `MaxRollBank` at `MaxFwdSpeed` (`FUN_00451ca0`, `0x4522db`–`0x45233e`).
pub fn heli_bank(t: &HeliTurn, speed: f32) -> f32 {
    let span = t.max_fwd_speed - t.bank_speed_limit;
    if span <= 0.0 {
        return 0.0;
    }
    t.max_roll_bank * ((speed - t.bank_speed_limit) / span).clamp(0.0, 1.0)
}

/// The share of the forward stick in a helicopter's pitch, and of its forward speed over
/// `MaxFwdSpeed` (`0xDFDD74`, `0xBEB42C`).
pub const HELI_PITCH_STICK: f32 = 0.33;
pub const HELI_PITCH_SPEED: f32 = 0.66;

/// How far a helicopter pitches, as a positive angle in radians, flying at `speed` with the forward
/// stick at `stick`: `MaxPitch × (0.33 × stick + 0.66 × speed / MaxFwdSpeed)` (`FUN_00452540`).
pub fn heli_pitch(max_pitch: f32, max_fwd_speed: f32, stick: f32, speed: f32) -> f32 {
    let share = if max_fwd_speed > 0.0 { (speed / max_fwd_speed).clamp(-1.0, 1.0) } else { 0.0 };
    max_pitch * (HELI_PITCH_STICK * stick + HELI_PITCH_SPEED * share)
}

/// The share of a target a servo with `gain` per second has reached after `seconds`: the game adds
/// `gain × (target − value) × dt` each step (`0x452155`), which in the limit of small steps is
/// `1 − e^(−gain × seconds)`.
pub fn servo_reached(gain: f32, seconds: f32) -> f32 {
    1.0 - (-gain * seconds).exp()
}

// ── plane ────────────────────────────────────────────────────────────────────────────────────────

/// The speed `Airstrike.Flyby` flies a plane at when its caller gives none, in m/s: each update
/// moves the plane this far times the frame's seconds along its path (binding `0x5AD650`, default
/// at `0x5AD795` from `0xB984A8`).
pub const FLYBY_DEFAULT_SPEED: f32 = 200.0;
/// Past its target a plane's path runs two more spans, each as long as the run from spawn to
/// target and each climbing this share of that run (`0xBBC7EC`, worker `0x62A440`).
pub const FLYBY_EXIT_CLIMB: f32 = 0.25;
/// A plane banks by this many radians per rad/s it yaws (`0xB9B688` at `0x62C89F`), up to
/// [`FLYBY_BANK_LIMIT`] either way (0.7 × π/2: `0xDFDDD8`, `0xDF6AD4` at `0x62C911`).
pub const FLYBY_BANK_GAIN: f32 = 0.3;
pub const FLYBY_BANK_LIMIT: f32 = 0.7 * std::f32::consts::FRAC_PI_2;

/// The five control points of a fly-by's path, as distance along the ground and height above the
/// fly-by altitude, both in runs (the distance from spawn to target): the spawn, the middle of
/// the run, the target, and the two exit points.
pub fn flyby_points() -> [(f32, f32); 5] {
    [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (2.0, FLYBY_EXIT_CLIMB), (3.0, 2.0 * FLYBY_EXIT_CLIMB)]
}

/// The bank in radians a plane holds while yawing at `yaw_rate` rad/s.
pub fn flyby_bank(yaw_rate: f32) -> f32 {
    (FLYBY_BANK_GAIN * yaw_rate).clamp(-FLYBY_BANK_LIMIT, FLYBY_BANK_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fly_by_runs_level_to_its_target_and_climbs_away() {
        let p = flyby_points();
        assert!(p[..3].iter().all(|(_, h)| *h == 0.0));
        assert_eq!(p[2], (1.0, 0.0));
        assert_eq!(p[4], (3.0, 0.5));
        assert!(near(flyby_bank(1.0), 0.3, 1e-6));
        assert!(near(flyby_bank(10.0), 1.099_557, 1e-5));
        assert!(near(flyby_bank(-10.0), -1.099_557, 1e-5));
    }

    /// The AMX30's torque curve: template `0x80007255`.
    const AMX30: TorqueCurve = TorqueCurve { ratios: [1.0, 0.45, 0.3, 0.1], pos: [0.4, 0.6] };

    #[test]
    fn a_torque_curve_runs_straight_between_its_knots() {
        assert_eq!(AMX30.at(0.0), 1.0);
        assert!(near(AMX30.at(0.2), 0.725, 1e-6));
        assert!(near(AMX30.at(0.4), 0.45, 1e-6));
        assert!(near(AMX30.at(0.5), 0.375, 1e-6));
        assert!(near(AMX30.at(0.8), 0.2, 1e-6));
        assert!(near(AMX30.at(1.0), 0.1, 1e-6));
    }

    #[test]
    fn thrust_is_the_curve_times_the_engine_and_stops_at_the_cap() {
        assert_eq!(thrust(&AMX30, 2000.0, 45.0, 0.0), 400_000.0);
        assert!(near(thrust(&AMX30, 2000.0, 45.0, 18.0), 180_000.0, 0.5));
        assert_eq!(thrust(&AMX30, 2000.0, 45.0, 45.0), 0.0);
        assert_eq!(thrust(&AMX30, 2000.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn a_tank_keeps_less_steering_the_faster_it_goes() {
        assert_eq!(tank_steer_factor(0.1, 45.0, 0.0), 1.0);
        assert!(near(tank_steer_factor(0.1, 45.0, 45.0), 0.9, 1e-6));
        assert!(near(tank_steer_factor(0.1, 45.0, 90.0), 0.9, 1e-6));
        assert_eq!(tank_steer_factor(3.0, 45.0, 45.0), 0.0);
    }

    /// The RIB36: template `0x8000638F`.
    const RIB36: BoatSteering = BoatSteering {
        steering: 1.0,
        no_speed_steering: 0.1,
        low_speed_steering: 1.5,
        low_speed_limit: 20.0,
        high_speed_steer_loss: 0.25,
        max_speed: 90.0,
        max_speed_reverse: 35.0,
    };

    #[test]
    fn a_boat_steers_with_its_engine_at_low_speed_and_its_speed_above_it() {
        assert_eq!(boat_stopped_under(35.0), 8.75);
        assert_eq!(boat_stopped_under(100.0), 10.0);
        // stopped, coasting: the stopped term alone
        assert!(near(boat_steer_gain(&RIB36, 0.0, 0.0), 0.1, 1e-6));
        // stopped, throttle held: the engine term at its whole
        assert!(near(boat_steer_gain(&RIB36, 1.0, 0.0), 1.6, 1e-6));
        // half way to the low speed limit
        assert!(near(boat_steer_gain(&RIB36, 1.0, 10.0), 1.0 * (1.0 - 0.25 * 10.0 / 90.0) * 0.5 + 0.75, 1e-6));
        // at the cap
        assert!(near(boat_steer_gain(&RIB36, 1.0, 90.0), 0.75, 1e-6));
        assert!(near(boat_steer_gain(&RIB36, 0.0, 90.0), 0.75, 1e-6));
    }

    /// The Mi-35's player record: template `0x80008531`.
    const MI35: HeliTurn = HeliTurn {
        max_yaw_speed: 3.0,
        low_speed_yaw_speed: 4.0,
        low_speed_limit: 10.0,
        hi_speed_yaw_loss: 0.4,
        max_fwd_speed: 35.0,
        bank_speed_limit: 20.0,
        max_roll_bank: 0.4,
    };

    #[test]
    fn a_helicopters_yaw_rate_follows_its_speed() {
        assert_eq!(heli_yaw_rate(&MI35, 0.0), 4.0);
        assert!(near(heli_yaw_rate(&MI35, 5.0), 3.5, 1e-6));
        assert!(near(heli_yaw_rate(&MI35, 10.0), 3.0, 1e-6));
        assert!(near(heli_yaw_rate(&MI35, 22.5), 3.0 * (1.0 - 0.5 * 0.4), 1e-6));
        assert!(near(heli_yaw_rate(&MI35, 35.0), 1.8, 1e-6));
        assert!(near(heli_yaw_rate(&MI35, 60.0), 1.8, 1e-6));
        // a record with no low speed band and no span above it
        let flat = HeliTurn { low_speed_limit: 0.0, max_fwd_speed: 0.0, ..MI35 };
        assert!(near(heli_yaw_rate(&flat, 5.0), 1.8, 1e-6));
    }

    #[test]
    fn a_helicopter_banks_past_its_bank_speed() {
        assert_eq!(heli_bank(&MI35, 10.0), 0.0);
        assert_eq!(heli_bank(&MI35, 20.0), 0.0);
        assert!(near(heli_bank(&MI35, 27.5), 0.2, 1e-6));
        assert!(near(heli_bank(&MI35, 35.0), 0.4, 1e-6));
        assert_eq!(heli_bank(&HeliTurn { bank_speed_limit: 35.0, ..MI35 }, 35.0), 0.0);
    }

    #[test]
    fn a_helicopter_pitches_with_its_stick_and_its_speed() {
        assert!(near(heli_pitch(0.5, 35.0, 1.0, 0.0), 0.165, 1e-6));
        assert!(near(heli_pitch(0.5, 35.0, 1.0, 35.0), 0.495, 1e-6));
        assert!(near(heli_pitch(0.5, 35.0, 0.0, 17.5), 0.165, 1e-6));
    }

    #[test]
    fn a_servo_closes_on_its_target() {
        assert_eq!(servo_reached(0.5, 0.0), 0.0);
        assert!(near(servo_reached(0.5, 2.0), 0.632_12, 1e-4));
        assert!(servo_reached(0.5, 20.0) > 0.9999);
    }

    /// The Austin (CIV) semi tractor, template `0x800075ED` of retail `vz.wad`.
    fn austin() -> (Gearbox, Axles) {
        (
            Gearbox {
                primary: 3.6,
                reverse: 3.0,
                forward: [3.7, 3.3, 2.5, 2.2, 1.5],
                upshift_rpm: 5000.0,
                downshift_rpm: 3000.0,
                min_rpm: 1000.0,
                opt_rpm: 4500.0,
                max_rpm: 8000.0,
                engine_torque: 1600.0,
                torque_factor: 1.0,
                rear_share: 0.5,
                max_speed: 90.0,
            },
            Axles { front_wheels: 2, rear_wheels: 4, front_radius: 0.6, rear_radius: 0.6 },
        )
    }

    fn near(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn the_wheel_shares_add_up_to_two() {
        let (g, a) = austin();
        assert!(near(wheel_share(&a, g.rear_share, true), 0.25, 1e-6));
        assert!(near(wheel_share(&a, g.rear_share, false), 0.5, 1e-6));
        let sum = 4.0 * wheel_share(&a, g.rear_share, true) + 2.0 * wheel_share(&a, g.rear_share, false);
        assert!(near(sum, SHARE_SUM, 1e-6));
    }

    #[test]
    fn the_austin_shifts_up_where_the_gearing_says() {
        let (g, a) = austin();
        for (gear, kmh) in [(1, 42.45), (2, 47.60), (3, 62.83), (4, 71.40)] {
            assert!(near(upshift_kmh(&g, &a, gear), kmh, 0.01), "gear {gear}: {}", upshift_kmh(&g, &a, gear));
        }
    }

    #[test]
    fn the_austin_reaches_its_cap_in_fifth_below_the_upshift_rpm() {
        let (g, a) = austin();
        let path = full_throttle_path(&g, &a);
        // rest, four shifts of two points each, the cap
        assert_eq!(path.len(), 10);
        let (kmh, rpm) = *path.last().unwrap();
        assert_eq!(kmh, 90.0);
        assert!(near(rpm, 4297.2, 0.5), "{rpm}");
        // each shift lands above the downshift rpm, so the box does not hunt
        for drop in [path[2], path[4], path[6], path[8]] {
            assert!(drop.1 > g.downshift_rpm, "{drop:?}");
        }
        assert!(near(path[2].1, 4459.5, 1.0) && near(path[8].1, 3409.1, 1.0));
    }

    #[test]
    fn a_slow_vehicle_meets_its_cap_before_its_first_upshift() {
        let (mut g, a) = austin();
        g.max_speed = 30.0;
        assert_eq!(full_throttle_path(&g, &a), vec![(0.0, 0.0), (30.0, 30.0 * rpm_per_kmh(&g, &a, 3.7))]);
    }

    #[test]
    fn torque_is_flat_and_friction_rises_for_the_austin() {
        let (g, _) = austin();
        let (t, f) = torque_and_friction(&g, 1000.0);
        assert!(near(t, 1600.0, 1e-3) && near(f, 80.0, 1e-3), "{t} {f}");
        let (t, f) = torque_and_friction(&g, 4500.0);
        assert!(near(t, 1600.0, 1e-3) && near(f, 160.0, 1e-3));
        let (t, f) = torque_and_friction(&g, 7999.0);
        assert!(near(t, 1600.0, 1e-3) && near(f, 479.9, 0.1));
        let (t, f) = torque_and_friction(&g, 8000.0);
        assert!(t == 0.0 && near(f, 480.0, 1e-3), "{t} {f}");
    }

    #[test]
    fn a_torque_factor_below_one_sags_both_ends_of_the_curve() {
        let (mut g, _) = austin();
        g.torque_factor = 0.5;
        assert!(near(torque_and_friction(&g, 1000.0).0, 800.0, 1e-3));
        assert!(near(torque_and_friction(&g, 4500.0).0, 1600.0, 1e-3));
        assert!(near(torque_and_friction(&g, 7999.9).0, 800.0, 1.0));
    }

    #[test]
    fn the_austin_loses_lock_with_speed() {
        let s = Steering { max_angle: 0.8, high_speed_loss: 0.77, loss_brake_multi: 0.5, loss_no_throttle_multi: 0.76, max_speed: 90.0 };
        assert_eq!(lock(&s, Pedals::Throttle, 0.0), 0.8);
        assert!(near(lock(&s, Pedals::Throttle, 90.0), 0.184, 1e-3));
        assert!(near(lock(&s, Pedals::Coasting, 90.0), 0.332, 1e-3));
        assert!(near(lock(&s, Pedals::Braking, 90.0), 0.492, 1e-3));
        // flat past the cap
        assert_eq!(lock(&s, Pedals::Throttle, 120.0), lock(&s, Pedals::Throttle, 90.0));
    }

    #[test]
    fn steering_inertia_sets_how_fast_the_wheels_follow() {
        assert!(near(1.0 - remaining(0.98, 1.0), 0.702, 1e-3));
        assert!(near(remaining(0.97, 1.0), 0.161, 1e-3));
        assert_eq!(remaining(0.98, 0.0), 1.0);
    }

    #[test]
    fn the_rate_mode_is_fastest_at_rest() {
        let s = Steering { max_angle: 0.9, high_speed_loss: 0.75, loss_brake_multi: 0.5, loss_no_throttle_multi: 0.8, max_speed: 80.0 };
        assert_eq!(steer_rate(1.0, 5.0, 30.0, &s, 0.0), 5.0);
        assert!(near(steer_rate(1.0, 5.0, 30.0, &s, 30.0), 1.0 * (1.0 - 0.75 * 30.0 / 80.0), 1e-5));
    }
}
