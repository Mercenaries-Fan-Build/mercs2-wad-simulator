//! `mercs2_water` — Water (scoreboard row 7): the engine-owned water *mechanism*.
//!
//! **Code map:** `docs/reverse_engineer/water_code_map.md` (the sky/decal/water PC code maps), with
//! `docs/watermap_format.md` for the static watermap layout and `vehicle_code_map.md` §3/§5 for the
//! boat buoyancy tunables. **Scoreboard row:** 7. **Scope:** the water-and-swimming scope
//! ([[water-and-swimming-scope]]).
//!
//! This crate implements the parts of the water system the **engine owns as compiled logic**, not the
//! render pass:
//!
//! - [`watermap`] — the static watermap ([`Watermap`], type `0x4D7D30C4`): a height field + wet mask
//!   over a 257×257 / 32 m grid, with the world XZ → wet/height query (`FUN_00480440`'s engine-owned
//!   job, code map §4/§5).
//! - [`swim`] — the TPS character swim-state FSM ([`SwimState`] / [`Swimmer`]), depth-driven off the
//!   watermap query. Thresholds are gameplay-derived (the map does not pin them) — see [`swim`].
//! - [`buoyancy`] — the flotation physics: the [`Buoyancy`] component (`0xb9659f7b`) + the boat
//!   [`WaterDragTunables`] (`WaterDrag*` etc.), as pure force math over a body vs the water surface.
//! - [`zone`] — the [`AiWaterZone`] reflection component (`0xdf6533de`), the AI water-type tag.
//!
//! At the root, [`WaterWorld`] bundles what is world-global (the loaded [`Watermap`] + the
//! [`SwimConfig`]) and drives the per-fixed-step swim update over the ECS `World`; per-entity water
//! state lives on the [`Swimmer`] / [`Buoyancy`] components.
//!
//! **Render seam.** [`Watermap::surface_mesh`] is the CPU side of that seam: one flat quad per wet
//! cell at its Layer-0 height, in world space. `mercs2_engine`'s water render node consumes it and
//! draws the translucent surface; no GPU or `wgpu` dependency exists here.
//!
//! **Deliberately NOT built (data / render-time / confirm-live per the code map):**
//! - The **water render pass** (wake→occlusion→reflection→surface, the ping-pong `pHeightS`/`pNormalS`/
//!   `pFoamMas` sim RTs, the reflection mirror-matrix, `PgWater*` shaders, `OWater::LOD` tessellation
//!   banding) — code map §1–§3. That is the seam handled against `mercs2_engine`; the **dynamic wave
//!   displacement** lives there, so this crate models only the *static* waterline.
//! - The **exact `FUN_00480440` return packing** (height-vs-boolean) — a SecuROM-island thunk, code map
//!   §5 confirm-live; we expose both facts ([`watermap::WaterSample`]) and let the caller pick.
//! - **`AiWaterZoneEnum` member names** — the table exists but is not itemised (ai_code_map §4); the
//!   zone value is carried raw.
//! - **Authored `WaterDrag*` / `Buoyancy` numeric defaults** — stripped on PC / extraction is the
//!   vehicle-map §5 open item; the field *set* is faithful, the numbers are documented placeholders.
//! - Motion-blur — the code map notes it is **ABSENT on PC**; nothing here fabricates it.

pub mod buoyancy;
pub mod swim;
pub mod watermap;
pub mod wave;
pub mod zone;

pub use buoyancy::{
    submersion_fraction, Buoyancy, WaterDragTunables, BUOYANCY_HASH, BUOYANCY_STRIDE,
};
pub use swim::{update_swim_state, SwimConfig, SwimState, Swimmer};
pub use wave::{WaveComponent, WaveModel};
pub use watermap::{
    Watermap, WaterSample, WatermapError, CELL_SIZE_M, GRID_DIM, HEIGHT_MIN_M, OPEN_WATER_SURFACE_M,
    WATERMAP_HASH,
};
pub use zone::{AiWaterZone, AI_WATER_ZONE_HASH, AI_WATER_ZONE_STRIDE};

use mercs2_core::World;

/// The host-owned water mechanism: the loaded static [`Watermap`] + the swim [`SwimConfig`]. This is
/// the world-global water state the sim holds — the analogue of `mercs2_ai::AiWorld`. Per-entity water
/// state ([`Swimmer`], [`Buoyancy`]) lives on ECS components in the `World`; this bundles what is
/// world-global and drives the per-entity swim update each fixed step.
///
/// Idles until a watermap is loaded and water-capable entities exist (the same data-driven idling the
/// AI/vehicle systems use — no watermap ⇒ [`tick`](Self::tick) is a no-op).
#[derive(Default)]
pub struct WaterWorld {
    /// The loaded static watermap (`None` until a world with water is streamed in).
    pub watermap: Option<Watermap>,
    /// Swim-FSM thresholds (gameplay-derived defaults; see [`swim`]).
    pub swim_config: SwimConfig,
    /// Animated surface waves ([`WaveModel`], WILDSTAR `CalcWaveOffsets` shape) — the displacement on
    /// top of the static watermap level. Shared with the water render so the drawn surface and the
    /// simulated one agree (swimmers bob on the waves the player sees).
    pub wave: WaveModel,
    /// Accumulated water time (s) driving the wave phase. Advanced by [`tick`](Self::tick); read by
    /// the render via [`time`](Self::time) so both sample the same field.
    time: f32,
}

impl WaterWorld {
    pub fn new() -> Self {
        WaterWorld::default()
    }

    /// Load (or replace) the static watermap.
    pub fn set_watermap(&mut self, watermap: Watermap) {
        self.watermap = Some(watermap);
    }

    /// Water time (s) driving the wave phase — the render passes this to the surface shader so the
    /// drawn wave matches the one [`sample`](Self::sample) reports.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// The full water query at a world XZ — `None` until a watermap is loaded, else the wet/height
    /// sample (the engine-owned half of `FUN_00480440`), with the animated wave displacement applied.
    pub fn sample(&self, x: f32, z: f32) -> Option<WaterSample> {
        self.watermap.as_ref().map(|w| {
            let mut s = w.sample(x, z);
            // Wave displacement applies to real water columns only; a dry cell keeps its sentinel.
            if s.is_water {
                s.surface_height += self.wave.height_offset(x, z, self.time);
            }
            s
        })
    }

    /// Is this world XZ over water? (`false` with no watermap.)
    pub fn is_water(&self, x: f32, z: f32) -> bool {
        self.watermap.as_ref().is_some_and(|w| w.is_water(x, z))
    }

    /// Water-surface height at this XZ where it is water (`None` over land / no watermap). Includes
    /// the animated [`WaveModel`] displacement, so this is the surface the player actually sees.
    pub fn water_surface_height(&self, x: f32, z: f32) -> Option<f32> {
        self.sample(x, z).filter(|s| s.is_water).map(|s| s.surface_height)
    }

    /// Per-fixed-step water update: advance the wave phase by `dt`, then advance every [`Swimmer`]'s
    /// FSM against the **wave-displaced** surface. No-op (beyond the clock) until a watermap is loaded.
    /// Returns the number of swimmers updated. Buoyancy/drag are applied by the physics silo using
    /// [`Buoyancy`]/[`WaterDragTunables`] against [`sample`](Self::sample); they are pure math, not a
    /// per-frame system here.
    pub fn tick(&mut self, world: &mut World, dt: f32) -> usize {
        self.time += dt;
        match &self.watermap {
            Some(wm) => update_swim_state(world, wm, &self.swim_config, &self.wave, self.time),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::glam::Vec3;
    use mercs2_core::Transform;

    /// Before a watermap is loaded, every query is empty and `tick` is a no-op (data-driven idle).
    #[test]
    fn idles_without_a_watermap() {
        let mut world = World::new();
        let e = world.spawn((Swimmer::new(), Transform::from_translation(Vec3::new(0.0, -5.0, 0.0))));
        let mut ww = WaterWorld::new();
        assert!(!ww.is_water(0.0, 0.0));
        assert_eq!(ww.sample(0.0, 0.0), None);
        assert_eq!(ww.water_surface_height(0.0, 0.0), None);
        assert_eq!(ww.tick(&mut world, 0.0), 0);
        assert_eq!(world.get::<&Swimmer>(e).unwrap().state, SwimState::OnLand);
    }

    /// End-to-end: load a wet watermap, and `tick` drives a submerged character into the Submerged
    /// state while the water query reports the surface height.
    #[test]
    fn loaded_watermap_drives_swim_and_query() {
        let mut world = World::new();
        let e = world.spawn((
            Swimmer::new(),
            Transform::from_translation(Vec3::new(0.0, -50.0, 0.0)),
        ));
        let mut ww = WaterWorld::new();
        ww.set_watermap(Watermap::uniform(GRID_DIM, CELL_SIZE_M, OPEN_WATER_SURFACE_M, true));

        // The character's feet at -50 sit 14 m under the -36 m open-water surface → deeply submerged.
        assert_eq!(ww.tick(&mut world, 0.0), 1);
        assert_eq!(world.get::<&Swimmer>(e).unwrap().state, SwimState::Submerged);
        assert!(ww.is_water(0.0, 0.0));
        assert_eq!(ww.water_surface_height(0.0, 0.0), Some(OPEN_WATER_SURFACE_M));
    }
}
