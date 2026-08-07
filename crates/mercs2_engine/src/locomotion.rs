//! `SceneLocomotion` — the engine's composition of the streamed world behind
//! [`mercs2_core::LocomotionQuery`].
//!
//! On-foot movement needs three different world sources: the collision world for walls, the terrain
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
    /// The streamed collision world — walls/props/buildings, and the interior floor probe. This no longer
    /// carries terrain triangles: the hi-res terrain surface is the baked [`terrain`](Self::terrain)
    /// heightfield now (retail `hkpHeightFieldShape`), so the collider stays small.
    pub tris: &'a [[Vec3; 3]],
    /// The LOW-RES terrain heightmap — the far/fallback outdoor surface where no hi-res tile is resident.
    /// `None` indoors or before the world streams in.
    pub hmap: Option<&'a HeightMap>,
    /// The BAKED HI-RES terrain heightfield: the resident `terrainmesh` tiles' actual near surface
    /// (≈1 m above the low-res `hmap`), as an O(1) height grid rather than collision triangles. `None`
    /// indoors, on the static `--interior` boot, or before any hi-res tile has woken.
    pub terrain: Option<&'a crate::game_world::TerrainHeightField>,
    /// The loaded watermap. `None` when the level has no water data.
    pub water: Option<&'a mercs2_water::Watermap>,
    /// Interiors have no terrain heightfield, so ground comes from a downward capsule probe against
    /// [`tris`](Self::tris) instead. This flag is *engine data* — which is exactly why it lives on the
    /// adapter and not as a parameter the controller has to thread through.
    pub interior: bool,
}

impl PhysicsQuery for SceneLocomotion<'_> {
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        // `broadphase::raycast` yields the hit distance only; the controller does not read the normal or the
        // entity off this path, and inventing a normal would be worse than reporting the ray's own.
        mercs2_physics::broadphase::raycast(self.tris, origin, dir, max).map(|d| RayHit {
            point: origin + dir * d,
            normal: -dir,
            distance: d,
            entity: None,
        })
    }

    fn closest_point(&self, point: Vec3, max: f32) -> Option<ClosestPoint> {
        // Not used by on-foot locomotion. `StaticCollision` owns the real implementation; a caller
        // that needs proximity queries should take that instead of this adapter.
        let _ = (point, max);
        None
    }

    fn move_character(&self, pos: Vec3, delta: Vec3, radius: f32, height: f32, step: f32) -> Vec3 {
        // `follow_ground = false`: the controller owns the Y axis (jump / gravity / buoyancy), so the
        // swept move must not also snap to the floor or the two fight each other.
        mercs2_physics::broadphase::move_character(self.tris, pos, delta, radius, height, step, false)
    }
}

impl LocomotionQuery for SceneLocomotion<'_> {
    fn ground_height(&self, feet: Vec3, radius: f32, probe: f32) -> Option<f32> {
        if self.interior {
            mercs2_physics::broadphase::ground_below(self.tris, feet, radius, probe)
        } else {
            // Exterior ground has THREE sources that must agree with what's drawn:
            //  * the BAKED HI-RES terrain heightfield — the resident `terrainmesh` tiles' actual near
            //    surface (retail `hkpHeightFieldShape`), sampled O(1). This is the surface the hero
            //    stands on wherever a hi-res tile is resident (it sits ≈1 m above the low-res);
            //  * the LOW-RES terrain HEIGHTMAP — the far/fallback surface, used where no hi-res tile
            //    covers the XZ; and
            //  * the (now small, terrain-free) COLLISION SET — woken exterior props/buildings, so a
            //    prop floor / building step still grounds the feet.
            // Terrain surface = hi-res if a tile covers the feet, else the low-res heightmap. Then stand
            // on the HIGHER of that and the prop collider, so props on the terrain still block. The collider
            // probe starts a little ABOVE the feet (they may be grounded on the lower low-res surface) so
            // a prop surface just overhead is still caught. `height_at_near` keeps the low-res term's
            // overhang-aware pick.
            //
            // WHY a heightfield, not collision tris: feeding every resident hi-res tile's ~thousands of
            // triangles into the collider made the streamer rebuild a ~273k-tri broadphase FROM SCRATCH on
            // every terrain wake/hibernate (constant while moving). The baked heightfield reproduces the
            // exact same "stand on the hi-res surface" behaviour for free.
            let hi = self.terrain.and_then(|t| t.height_at(feet.x, feet.z));
            let lo = self.hmap.map(|h| h.height_at_near(feet.x, feet.z, feet.y));
            let terrain_ground = hi.or(lo);
            let probe_origin = mercs2_core::glam::Vec3::new(feet.x, feet.y + 2.0, feet.z);
            let prop = mercs2_physics::broadphase::ground_below(self.tris, probe_origin, radius, probe + 2.0);
            match (terrain_ground, prop) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            }
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

    /// Indoors the ground comes from the collision world; with no floor under the feet there is nothing
    /// to stand on and the controller falls.
    #[test]
    fn interior_ground_comes_from_the_collision_world() {
        let tris = floor(0.0);
        let q = SceneLocomotion { tris: &tris, hmap: None, terrain: None, water: None, interior: true };
        let g = q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0);
        assert_eq!(g, Some(0.0), "stands on the floor");

        let empty: Vec<[Vec3; 3]> = Vec::new();
        let q = SceneLocomotion { tris: &empty, hmap: None, terrain: None, water: None, interior: true };
        assert_eq!(q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0), None, "a gap -> falls");
    }

    /// A baked hi-res terrain heightfield holding one flat tile at the given height, covering the origin.
    fn terrain_field(y: f32) -> crate::game_world::TerrainHeightField {
        use crate::game_world::{TerrainHeightField, TileHeightGrid};
        // A flat tile of world-space triangles spanning [-8, 8]² around the origin.
        let mut tris = Vec::new();
        for xi in -2..2 {
            for zi in -2..2 {
                let (x0, x1) = (xi as f32 * 4.0, xi as f32 * 4.0 + 4.0);
                let (z0, z1) = (zi as f32 * 4.0, zi as f32 * 4.0 + 4.0);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1)]);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1)]);
            }
        }
        let mut field = TerrainHeightField::default();
        field.insert(1, TileHeightGrid::bake(&tris).unwrap());
        field
    }

    /// Outdoors the rendered near surface is the resident hi-res `terrainmesh` tile — now a BAKED
    /// heightfield (retail `hkpHeightFieldShape`), NOT collision triangles. The hero grounds on that hi-res
    /// surface (≈1 m above the low-res), so the terrain heightfield term wins over a lower prop-collider floor.
    #[test]
    fn exterior_stands_on_the_hires_terrain_heightfield() {
        // Hi-res terrain surface at y=1, a (prop) collider floor at y=0. The hero stands on the HI-RES
        // surface, not the lower collider floor — the anti-sink behaviour, now heightfield-driven.
        let tri_floor = floor(0.0);
        let field = terrain_field(1.0);
        let q = SceneLocomotion {
            tris: &tri_floor,
            hmap: None,
            terrain: Some(&field),
            water: None,
            interior: false,
        };
        assert_eq!(
            q.ground_height(Vec3::new(0.0, 1.5, 0.0), 0.35, 4.0),
            Some(1.0),
            "exterior ground follows the baked hi-res terrain heightfield, not the lower prop collider"
        );

        // With NO hi-res tile resident but a prop floor present, the feet still rest on the prop collider.
        let empty_field = crate::game_world::TerrainHeightField::default();
        let q = SceneLocomotion {
            tris: &tri_floor,
            hmap: None,
            terrain: Some(&empty_field),
            water: None,
            interior: false,
        };
        assert_eq!(
            q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0),
            Some(0.0),
            "no hi-res tile here → prop collider still grounds the feet"
        );
    }

    /// With no terrain heightfield, no heightmap, and no collider surface under the feet there is nothing to
    /// stand on — the controller must fall rather than be snapped to y = 0.
    #[test]
    fn exterior_with_no_ground_source_falls() {
        let empty: Vec<[Vec3; 3]> = Vec::new();
        let q = SceneLocomotion { tris: &empty, hmap: None, terrain: None, water: None, interior: false };
        assert_eq!(
            q.ground_height(Vec3::new(0.0, 1.0, 0.0), 0.35, 4.0),
            None,
            "no terrain, no heightmap and no collider surface -> a gap -> falls"
        );
    }

    /// A dry cell yields `None`, so the controller never needs a separate `is_water` predicate.
    #[test]
    fn water_column_is_none_when_dry_or_absent() {
        let tris = floor(0.0);
        let q = SceneLocomotion { tris: &tris, hmap: None, terrain: None, water: None, interior: false };
        assert!(q.water_column(0.0, 0.0).is_none(), "no watermap loaded");

        let dry = mercs2_water::Watermap::uniform(5, 32.0, 100.0, false);
        let q = SceneLocomotion { tris: &tris, hmap: None, terrain: None, water: Some(&dry), interior: false };
        assert!(q.water_column(0.0, 0.0).is_none(), "loaded but dry");

        let wet = mercs2_water::Watermap::uniform(5, 32.0, 3.5, true);
        let q = SceneLocomotion { tris: &tris, hmap: None, terrain: None, water: Some(&wet), interior: false };
        assert_eq!(q.water_column(0.0, 0.0).map(|c| c.surface_height), Some(3.5));
    }
}
