//! `SceneLocomotion` — the engine's composition of the streamed world behind
//! [`mercs2_core::LocomotionQuery`].
//!
//! On-foot movement needs three different world sources: the collision soup for walls, the terrain
//! heightfield for outdoor ground, and the watermap for the swim FSM. Those live in three crates that
//! must not know about each other, so the character controller in `mercs2_player` takes the
//! `LocomotionQuery` *contract* and this type supplies it.
//!
//! **Why the adapter lives here.** `mercs2_engine` is the one crate that already depends on all three
//! (`mercs2_physics`, `mercs2_water`, and its own `worldutil::HeightMap`). Putting the composition in
//! `mercs2_physics` instead would drag a `mercs2_water` edge into it — a leaf→leaf dependency the carve
//! rule forbids (`reimplementation_parallelization_plan.md` §4).
//!
//! It **borrows** rather than owns, so constructing one per frame costs nothing.

use mercs2_core::glam::Vec3;
use mercs2_core::locomotion_query::{LocomotionQuery, WaterColumn};
use mercs2_core::physics_query::{ClosestPoint, PhysicsQuery, RayHit};

use crate::worldutil::HeightMap;

/// The streamed scene, as the character controller needs to see it.
pub struct SceneLocomotion<'a> {
    /// The streamed collision soup — walls, and the interior floor probe.
    pub tris: &'a [[Vec3; 3]],
    /// The terrain heightfield. `None` indoors or before the world streams in.
    pub hmap: Option<&'a HeightMap>,
    /// The loaded watermap. `None` when the level has no water data.
    pub water: Option<&'a mercs2_water::Watermap>,
    /// Interiors have no terrain heightfield, so ground comes from a downward capsule probe against
    /// [`tris`](Self::tris) instead. This flag is *engine data* — which is exactly why it lives on the
    /// adapter and not as a parameter the controller has to thread through.
    pub interior: bool,
}

impl PhysicsQuery for SceneLocomotion<'_> {
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        // `soup::raycast` yields the hit distance only; the controller does not read the normal or the
        // entity off this path, and inventing a normal would be worse than reporting the ray's own.
        mercs2_physics::soup::raycast(self.tris, origin, dir, max).map(|d| RayHit {
            point: origin + dir * d,
            normal: -dir,
            distance: d,
            entity: None,
        })
    }

    fn closest_point(&self, point: Vec3, max: f32) -> Option<ClosestPoint> {
        // Not used by on-foot locomotion. `StaticSoupPhysics` owns the real implementation; a caller
        // that needs proximity queries should take that instead of this adapter.
        let _ = (point, max);
        None
    }

    fn move_character(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32, step: f32) -> Vec3 {
        // `follow_ground = false`: the controller owns the Y axis (jump / gravity / buoyancy), so the
        // swept move must not also snap to the floor or the two fight each other.
        mercs2_physics::soup::move_character(self.tris, pos, delta, radius, height, step, false)
    }
}

impl LocomotionQuery for SceneLocomotion<'_> {
    fn ground_height(&self, feet: Vec3, radius: f32, probe: f32) -> Option<f32> {
        if self.interior {
            mercs2_physics::soup::ground_below(self.tris, feet, radius, probe)
        } else {
            // `height_at_near` picks the terrain sample nearest the given Y, which is what keeps a
            // character under an overhang from being snapped onto it.
            self.hmap.map(|h| h.height_at_near(feet.x, feet.z, feet.y))
        }
    }

    fn water_column(&self, x: f32, z: f32) -> Option<WaterColumn> {
        let s = self.water?.sample(x, z);
        s.is_water.then_some(WaterColumn { surface_height: s.surface_height })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat floor of unit triangles at `y`, spanning the origin.
    fn floor(y: f32) -> Vec<[Vec3; 3]> {
        let mut tris = Vec::new();
        for xi in -3..3 {
            for zi in -3..3 {
                let (x0, x1) = (xi as f32, xi as f32 + 1.0);
                let (z0, z1) = (zi as f32, zi as f32 + 1.0);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1)]);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1)]);
            }
        }
        tris
    }

    /// Indoors the ground comes from the collision soup; with no floor under the feet there is nothing
    /// to stand on and the controller falls.
    #[test]
    fn interior_ground_comes_from_the_collision_soup() {
        let tris = floor(0.0);
        let q = SceneLocomotion { tris: &tris, hmap: None, water: None, interior: true };
        let g = q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0);
        assert_eq!(g, Some(0.0), "stands on the floor");

        let empty: Vec<[Vec3; 3]> = Vec::new();
        let q = SceneLocomotion { tris: &empty, hmap: None, water: None, interior: true };
        assert_eq!(q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0), None, "a gap -> falls");
    }

    /// Outdoors with no heightmap streamed there is likewise no ground — the controller must fall
    /// rather than be snapped to y = 0.
    #[test]
    fn exterior_without_a_heightmap_reports_no_ground() {
        let tris = floor(0.0);
        let q = SceneLocomotion { tris: &tris, hmap: None, water: None, interior: false };
        assert_eq!(
            q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0),
            None,
            "exterior ground is the heightfield's answer, not the soup's"
        );
    }

    /// A dry cell yields `None`, so the controller never needs a separate `is_water` predicate.
    #[test]
    fn water_column_is_none_when_dry_or_absent() {
        let tris = floor(0.0);
        let q = SceneLocomotion { tris: &tris, hmap: None, water: None, interior: false };
        assert!(q.water_column(0.0, 0.0).is_none(), "no watermap loaded");

        let dry = mercs2_water::Watermap::uniform(5, 32.0, 100.0, false);
        let q = SceneLocomotion { tris: &tris, hmap: None, water: Some(&dry), interior: false };
        assert!(q.water_column(0.0, 0.0).is_none(), "loaded but dry");

        let wet = mercs2_water::Watermap::uniform(5, 32.0, 3.5, true);
        let q = SceneLocomotion { tris: &tris, hmap: None, water: Some(&wet), interior: false };
        assert_eq!(q.water_column(0.0, 0.0).map(|c| c.surface_height), Some(3.5));
    }
}
