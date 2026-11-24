//! The car model's gearbox, torque and steering math, as the PC executable computes it. Pure
//! functions over a template's named `_CarPhysicsV2` fields; the Driving page draws their results.
//!
//! Read from pc-v10: engine speed `FUN_0044d430`, the per-wheel torque shares `FUN_0044fc10`, the
//! shift rule `FUN_0044fda0`, the torque curve `FUN_0044fe20`, steering `FUN_0044f680` and its
//! smoothing `FUN_00714d90`. Each constant below names the address it was read at.

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

#[cfg(test)]
mod tests {
    use super::*;

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
