//! `PhysicsQuery` — the collision-query seam between the sim silos and the physics impl.
//!
//! This trait is the **interface** that `mercs2_vehicle`, `mercs2_combat`, and `mercs2_anim`
//! (ragdoll) depend on, so those leaf crates compile against the *contract*, never against
//! `mercs2_physics` directly (no leaf→leaf edge; the carve rule in
//! `reimplementation_parallelization_plan.md` §4). The physics silo (row 22,
//! `docs/reverse_engineer/physics_code_map.md`) implements it; an early impl can be backed by the
//! existing terrain-heightmap + `collision_tris` raycast we already have and later swapped for the
//! full Havok `hkpCharacterProxy` path.
//!
//! The three methods are grounded 1:1 in the shared query layer the engine exposes
//! (`physics_code_map.md` §3/§4):
//!   * [`PhysicsQuery::raycast`]        ← `hkpWorldRayCaster` / the Pangea `CastRay` API.
//!   * [`PhysicsQuery::closest_point`]  ← `LthkpWorld::getClosestPoints` (`FUN_008db880`) +
//!                                        `hkpClosestCdPointCollector`.
//!   * [`PhysicsQuery::move_character`] ← the `hkpCharacterProxy` swept-capsule controller
//!                                        (`HumanPhysics::Activate` builder `FUN_004255c0`).

use glam::Vec3;
use hecs::Entity;

/// A single ray/query hit against the physics world.
///
/// Mirrors what an `hkpRayHitCollector` / `hkpClosestCdPointCollector` yields: the world-space
/// contact point, the surface normal there, the hit distance along the ray, and — for gameplay
/// queries (AI LOS, weapon hit-tests) — the ECS entity the shape belongs to, when one is known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// World-space hit point.
    pub point: Vec3,
    /// Unit surface normal at the hit.
    pub normal: Vec3,
    /// Distance from the ray origin to `point` (in the ray's units).
    pub distance: f32,
    /// The entity owning the struck collider, if the query resolved one. Static world geometry
    /// (MOPP / heightfield) may report `None`.
    pub entity: Option<Entity>,
}

/// The closest surface point returned by a proximity query.
///
/// Mirrors `LthkpWorld::getClosestPoints`: the nearest point on world geometry to the query point,
/// its normal, the separating distance (negative ⇒ penetrating), and the owning entity if known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosestPoint {
    /// Nearest world-space point on the queried geometry.
    pub point: Vec3,
    /// Unit surface normal at `point`.
    pub normal: Vec3,
    /// Separating distance; negative means the query point is inside the geometry (penetration).
    pub distance: f32,
    /// The entity owning the nearest collider, if the query resolved one.
    pub entity: Option<Entity>,
}

/// The read-only collision-query surface shared by AI, camera, weapons, vehicles, and character
/// movement. The physics silo owns the implementation; sim silos take `&dyn PhysicsQuery`.
pub trait PhysicsQuery {
    /// Cast a ray from `origin` along unit `dir` for up to `max` units. Returns the nearest hit,
    /// or `None` if the ray reaches `max` unobstructed. (`hkpWorldRayCaster` / `CastRay`.)
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit>;

    /// Find the closest point on world geometry within `max` units of `point`. Returns `None` if
    /// nothing is within range. (`LthkpWorld::getClosestPoints` + closest-point collector.)
    fn closest_point(&self, point: Vec3, max: f32) -> Option<ClosestPoint>;

    /// Sweep a character capsule (`radius` × `height`) from `pos` by desired `delta`, resolving
    /// collisions and climbing steps up to `step` height, and return the resolved end position.
    /// This is the `hkpCharacterProxy` swept-capsule move (slope/step limits + ground/air/jump
    /// state machine); the returned `Vec3` is the post-collision position for `pos`.
    fn move_character(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32, step: f32) -> Vec3;
}
