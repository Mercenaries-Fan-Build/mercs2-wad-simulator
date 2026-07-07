//! Buoyancy + water-drag — the physics the engine applies to bodies in water.
//!
//! Two recovered sources:
//!
//! - **Generic flotation** = the `Buoyancy` ECS reflection component (code map §5): m2 hash
//!   **`0xb9659f7b`**, builder `FUN_006395e0`, **stride `0x14` = 20 B = ~5 floats** — "waterline
//!   offset + up-force + damping". It applies an up-force vs the waterline for boats / floating debris.
//!   Modelled here as [`Buoyancy`] (5 named floats).
//! - **Boat handling tunables** on the Boat vehicle-definition (code map §5, `vehicle_code_map.md`):
//!   `WaterDragFwd/Side/Up`, `OutOfWaterGravityFactorUp/Down`, `Wake{Offset,Size,LifeTime,Speed,Rate}`,
//!   `Shallow{Depth,LinDamp,AngDamp,T}`, `InWaterT`, `WaterDepth`, `HullFriction`. Modelled as
//!   [`WaterDragTunables`].
//!
//! The consumer that applies them is the buoyancy hkpUnaryAction `FUN_00458ac0` (`vehicle_code_map.md`
//! §3, action I): **8 AABB-corner sample points + volume weights**, the waterline query `FUN_00480440`
//! every frame, buoyant impulses every *other* frame, a "sunk" latch. [`submersion_fraction`] models
//! the corner-sampling; the boat driver `FUN_00447260` then applies buoyancy + `WaterDrag{Fwd,Side,Up}`
//! + control forces.
//!
//! **Honesty boundary — the numeric tunable defaults are NOT recovered.** `vehicle_code_map.md` §2/§5
//! is explicit: the tuning field *names* are stripped on PC and the authored default values are an
//! open extraction work item (field identity comes from stream order / Xbox registrars, not from the
//! PC image). So the field *set* here is faithful, but [`WaterDragTunables::default`] and the
//! [`Buoyancy`] defaults use **documented neutral placeholders, not exe constants** — they are marked
//! as such, and are pure config a real per-boat definition overrides. Nothing here invents a compiled
//! constant the code map does not have.

use mercs2_core::glam::Vec3;

/// `Buoyancy` reflection hash (code map §5).
pub const BUOYANCY_HASH: u32 = 0xb965_9f7b;

/// `Buoyancy` stride in bytes (`0x14` = 20 = five f32 columns).
pub const BUOYANCY_STRIDE: usize = 0x14;

/// The generic-flotation `Buoyancy` component — five floats: waterline offset + up-force + damping
/// (code map §5, stride 0x14). Applies an up-force vs the water surface for floating bodies.
///
/// The five columns follow the code map's "waterline offset + up-force + damping" description. The
/// **numeric defaults are placeholders, not exe-recovered** (per the module honesty boundary): a
/// neutral, gently-floating body — a real `Buoyancy` definition on an asset overrides them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buoyancy {
    /// Waterline offset (m): shifts the body's neutral float line relative to the water surface. A
    /// positive value floats the body higher (its rest waterline sits this far below the surface).
    pub waterline_offset: f32,
    /// Up-force strength: the buoyant force per metre of submersion (N/m, arbitrary sim units).
    pub up_force: f32,
    /// Linear (vertical) damping applied to the body's vertical velocity while submerged.
    pub linear_damping: f32,
    /// Angular damping applied while submerged (settles bobbing rotation).
    pub angular_damping: f32,
    /// Extra translational drag applied while fully submerged (below-surface bulk resistance).
    pub submerged_drag: f32,
}

impl Default for Buoyancy {
    fn default() -> Self {
        // NEUTRAL PLACEHOLDERS — not exe constants (see module docs).
        Buoyancy {
            waterline_offset: 0.0,
            up_force: 9.81,
            linear_damping: 1.0,
            angular_damping: 1.0,
            submerged_drag: 0.5,
        }
    }
}

impl Buoyancy {
    /// The vertical buoyant + damping force on a body whose reference point is at world height `body_y`
    /// with vertical velocity `vel_y`, given the water surface height `surface_y`. Positive = up.
    ///
    /// Model (pure Archimedes-style spring-damper against the waterline): the effective waterline is
    /// `surface_y + waterline_offset`; submersion `d = waterline − body_y`. When submerged (`d > 0`) the
    /// force is `up_force·d − linear_damping·vel_y` (a restoring up-force proportional to how deep the
    /// point is, minus vertical damping). Above the waterline (`d ≤ 0`) there is no buoyant force.
    pub fn vertical_force(&self, body_y: f32, vel_y: f32, surface_y: f32) -> f32 {
        let waterline = surface_y + self.waterline_offset;
        let d = waterline - body_y;
        if d > 0.0 {
            self.up_force * d - self.linear_damping * vel_y
        } else {
            0.0
        }
    }
}

/// Fraction of a set of sample points (e.g. the boat's 8 AABB corners, `vehicle_code_map.md` §3 action
/// I) that sit below the water surface — `submerged_count / total`, in `[0,1]`. This is the
/// corner-sampling the buoyancy action uses to scale its buoyant impulse (a fully-submerged hull → 1,
/// a hull riding on the surface → ~0.5, airborne → 0). Empty input → 0.
pub fn submersion_fraction(sample_ys: &[f32], surface_y: f32) -> f32 {
    if sample_ys.is_empty() {
        return 0.0;
    }
    let below = sample_ys.iter().filter(|&&y| y < surface_y).count();
    below as f32 / sample_ys.len() as f32
}

/// Boat water-handling tunables (code map §5 / `vehicle_code_map.md`). The field **set** is faithful
/// to the Xbox registrars; the **numeric defaults are placeholders, not exe-recovered** (extraction is
/// the vehicle-map §5 open item). Neutral defaults = no drag / normal gravity, so an unconfigured boat
/// is inert rather than silently wrong.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterDragTunables {
    /// Longitudinal (forward/back) water drag coefficient.
    pub water_drag_fwd: f32,
    /// Lateral (sideways) water drag coefficient — resists sliding sideways.
    pub water_drag_side: f32,
    /// Vertical water drag coefficient — resists bobbing.
    pub water_drag_up: f32,
    /// Gravity multiplier applied to the up component when out of water (rising).
    pub out_of_water_gravity_factor_up: f32,
    /// Gravity multiplier applied to the down component when out of water (falling).
    pub out_of_water_gravity_factor_down: f32,
    /// Wake emitter: spawn offset behind the hull.
    pub wake_offset: f32,
    /// Wake emitter: particle/decal size.
    pub wake_size: f32,
    /// Wake emitter: lifetime (s).
    pub wake_life_time: f32,
    /// Wake emitter: propagation speed.
    pub wake_speed: f32,
    /// Wake emitter: spawn rate.
    pub wake_rate: f32,
    /// Depth below which "shallow-water" handling engages.
    pub shallow_depth: f32,
    /// Shallow-water linear damping.
    pub shallow_lin_damp: f32,
    /// Shallow-water angular damping.
    pub shallow_ang_damp: f32,
    /// Shallow-water transition time constant (`ShallowT`).
    pub shallow_t: f32,
    /// In-water transition time constant (`InWaterT`) — how fast in/out-of-water state ramps.
    pub in_water_t: f32,
    /// Reference hull draft / `WaterDepth`.
    pub water_depth: f32,
    /// Hull friction against the water surface.
    pub hull_friction: f32,
}

impl Default for WaterDragTunables {
    fn default() -> Self {
        // NEUTRAL PLACEHOLDERS — the authored per-boat defaults are unrecovered (see module docs).
        WaterDragTunables {
            water_drag_fwd: 0.0,
            water_drag_side: 0.0,
            water_drag_up: 0.0,
            out_of_water_gravity_factor_up: 1.0,
            out_of_water_gravity_factor_down: 1.0,
            wake_offset: 0.0,
            wake_size: 0.0,
            wake_life_time: 0.0,
            wake_speed: 0.0,
            wake_rate: 0.0,
            shallow_depth: 0.0,
            shallow_lin_damp: 0.0,
            shallow_ang_damp: 0.0,
            shallow_t: 0.0,
            in_water_t: 0.0,
            water_depth: 0.0,
            hull_friction: 0.0,
        }
    }
}

impl WaterDragTunables {
    /// Body-frame water drag force for a hull moving at `body_vel` (body axes: **x = right/side, y =
    /// up, z = forward**). The boat driver `FUN_00447260` applies `WaterDrag{Fwd,Side,Up}` as a
    /// per-axis linear resistance; here that is `-vel_axis · drag_axis` per component, returned in body
    /// frame for the caller to rotate into world space. Scale by `submersion` so a hull only barely in
    /// the water feels proportionally less drag (the buoyancy action weights forces by submersion).
    pub fn water_drag(&self, body_vel: Vec3, submersion: f32) -> Vec3 {
        let s = submersion.clamp(0.0, 1.0);
        Vec3::new(
            -body_vel.x * self.water_drag_side,
            -body_vel.y * self.water_drag_up,
            -body_vel.z * self.water_drag_fwd,
        ) * s
    }

    /// The gravity multiplier to use for a body that is out of the water, chosen by whether it is
    /// rising (`vel_y > 0` → `OutOfWaterGravityFactorUp`) or falling (`OutOfWaterGravityFactorDown`).
    /// The boat feels heavier/lighter leaving the surface per these tunables.
    pub fn out_of_water_gravity_factor(&self, vel_y: f32) -> f32 {
        if vel_y > 0.0 {
            self.out_of_water_gravity_factor_up
        } else {
            self.out_of_water_gravity_factor_down
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_hash_and_stride() {
        assert_eq!(BUOYANCY_HASH, 0xb965_9f7b);
        assert_eq!(BUOYANCY_STRIDE, 20);
    }

    #[test]
    fn buoyancy_pushes_up_when_submerged_and_zero_when_above() {
        let b = Buoyancy { waterline_offset: 0.0, up_force: 10.0, linear_damping: 0.0, ..Buoyancy::default() };
        // 1 m below the surface, at rest → +10 up.
        assert_eq!(b.vertical_force(-1.0, 0.0, 0.0), 10.0);
        // Above the surface → no buoyant force.
        assert_eq!(b.vertical_force(1.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn buoyancy_damps_vertical_velocity() {
        let b = Buoyancy { waterline_offset: 0.0, up_force: 10.0, linear_damping: 2.0, ..Buoyancy::default() };
        // 1 m under, rising at 3 m/s → 10*1 - 2*3 = 4.
        assert_eq!(b.vertical_force(-1.0, 3.0, 0.0), 4.0);
    }

    #[test]
    fn waterline_offset_shifts_the_float_line() {
        // Offset +1 lifts the effective waterline to surface+1, so a point at y=0 (surface) is 1 m under.
        let b = Buoyancy { waterline_offset: 1.0, up_force: 5.0, linear_damping: 0.0, ..Buoyancy::default() };
        assert_eq!(b.vertical_force(0.0, 0.0, 0.0), 5.0);
    }

    #[test]
    fn submersion_fraction_counts_corners_below() {
        // 8 AABB corners, half below the surface → 0.5.
        let ys = [-1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0];
        assert_eq!(submersion_fraction(&ys, 0.0), 0.5);
        assert_eq!(submersion_fraction(&[-1.0; 8], 0.0), 1.0);
        assert_eq!(submersion_fraction(&[1.0; 8], 0.0), 0.0);
        assert_eq!(submersion_fraction(&[], 0.0), 0.0);
    }

    #[test]
    fn water_drag_opposes_body_velocity_per_axis_and_scales_with_submersion() {
        let t = WaterDragTunables {
            water_drag_fwd: 2.0,
            water_drag_side: 3.0,
            water_drag_up: 1.0,
            ..WaterDragTunables::default()
        };
        // Fully submerged: drag opposes each axis by its coefficient.
        let d = t.water_drag(Vec3::new(1.0, 2.0, 4.0), 1.0);
        assert_eq!(d, Vec3::new(-3.0, -2.0, -8.0));
        // Half submerged → half the drag.
        let half = t.water_drag(Vec3::new(1.0, 2.0, 4.0), 0.5);
        assert_eq!(half, Vec3::new(-1.5, -1.0, -4.0));
    }

    #[test]
    fn out_of_water_gravity_factor_picks_up_vs_down() {
        let t = WaterDragTunables {
            out_of_water_gravity_factor_up: 0.5,
            out_of_water_gravity_factor_down: 2.0,
            ..WaterDragTunables::default()
        };
        assert_eq!(t.out_of_water_gravity_factor(3.0), 0.5, "rising uses the up factor");
        assert_eq!(t.out_of_water_gravity_factor(-3.0), 2.0, "falling uses the down factor");
    }

    #[test]
    fn neutral_defaults_are_inert() {
        let t = WaterDragTunables::default();
        assert_eq!(t.water_drag(Vec3::new(5.0, 5.0, 5.0), 1.0), Vec3::ZERO);
        assert_eq!(t.out_of_water_gravity_factor(1.0), 1.0);
    }
}
