//! `LocomotionQuery` — the world-sampling seam on-foot character movement needs beyond
//! [`PhysicsQuery`](crate::PhysicsQuery).
//!
//! Same role and same reasoning as `physics_query.rs`: `mercs2_player`'s character controller
//! compiles against the *contract*, never against `mercs2_physics` / `mercs2_water` / the engine's
//! terrain heightmap (no leaf→leaf edge; the carve rule in
//! `reimplementation_parallelization_plan.md` §4). `mercs2_engine` is the one crate that already
//! depends on all three, so it owns the composing impl.
//!
//! It is a **supertrait of [`PhysicsQuery`]** so a caller passes ONE `&dyn` covering the swept-capsule
//! move and these two probes, exactly as `mercs2_vehicle::drive_step_system` takes one
//! `&dyn PhysicsQuery`.
//!
//! **Two methods, not four.** The choice between an exterior terrain heightfield and an interior
//! downward capsule probe is *engine data*, not controller policy, so it is folded into
//! [`LocomotionQuery::ground_height`]'s impl rather than exposed as an `interior` flag the caller has
//! to thread through. Likewise `water_column` returns `Option`, so callers never need a separate
//! `is_water` predicate.
//!
//! **Honesty boundary.** Retail's on-foot controller is Havok — `HumanPhysics::Activate`
//! `FUN_004255c0` building 7 capsules + a phantom + an `hkpCharacterProxy`, driven by the 5-state
//! `hkpCharacterContext` machine (`docs/reverse_engineer/physics_code_map.md`). This trait is *not* a
//! recovered interface; it is the seam the reimpl's stand-in controller needs, sized so a later
//! Havok-faithful implementation can satisfy it unchanged.

use glam::Vec3;

use crate::physics_query::PhysicsQuery;

/// A water column sampled over a world position.
///
/// [`LocomotionQuery::water_column`] returns `None` for "dry, or no water data loaded", so this type
/// never has to carry an `is_water` flag — the absence *is* the answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterColumn {
    /// World-space height of the water surface, in metres. Depth under the surface is the caller's
    /// `surface_height - feet_y`.
    pub surface_height: f32,
}

/// The world-sampling surface on-foot locomotion needs on top of the shared collision queries.
///
/// Implemented engine-side (`mercs2_engine::locomotion::SceneLocomotion`) over the streamed collision
/// soup + terrain heightmap + watermap; consumed by `mercs2_player`'s controller as `&dyn`.
pub trait LocomotionQuery: PhysicsQuery {
    /// Height of the walkable surface under `feet` (a world-space *feet* position, not the entity
    /// origin), searching at most `probe` metres downward with a capsule of `radius`.
    ///
    /// Implementations should prefer a surface at or just above `feet.y` so a character standing
    /// *under* an overhang is not snapped on top of it.
    ///
    /// `None` means there is nothing to stand on — a gap, an interior with no floor geometry, or no
    /// world streamed yet — and the caller falls.
    fn ground_height(&self, feet: Vec3, radius: f32, probe: f32) -> Option<f32>;

    /// The water column at world `(x, z)`. `None` = dry, outside the grid, or no watermap loaded.
    fn water_column(&self, x: f32, z: f32) -> Option<WaterColumn>;
}
