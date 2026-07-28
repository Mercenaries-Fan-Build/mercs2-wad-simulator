//! The TPS character swim ECS layer — the per-entity [`Swimmer`] component and the per-fixed-step
//! system that drives it from the watermap.
//!
//! **The FSM itself lives in [`mercs2_core::swim`]** ([`SwimState`] / [`SwimConfig`]), because
//! `mercs2_player`'s on-foot controller switches locomotion mode on the same classification and no
//! leaf crate may depend on another. Both types are re-exported from this crate's root, so consumers
//! of `mercs2_water::{SwimState, SwimConfig}` are unaffected.
//!
//! What stays here is everything that needs water *data*: the component, and the query-the-watermap /
//! compute-feet-depth / advance-the-FSM tick. See the core module for the depth bands, the hysteresis
//! rule, and the honesty boundary on where the thresholds came from.

use mercs2_core::{SwimConfig, SwimState, World};

/// The per-character swim component: current FSM state + the last computed feet-depth under the
/// surface. Carried by any TPS actor that can enter water (player + swim-capable NPCs). Not a native
/// reflection component (the recovered water components are watermap/`Buoyancy`/`AiWaterZone`); this is
/// the reimpl's character-side swim state, driven each tick by [`update_swim_state`].
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Swimmer {
    /// Current swim FSM state.
    pub state: SwimState,
    /// Depth of the character's feet below the water surface (m); ≤ 0 when out of / above water.
    pub depth: f32,
    /// Vertical offset from the entity `Transform` translation to the character's feet. The watermap
    /// height compares against `feet_y = translation.y + feet_offset`. `0.0` = the transform origin is
    /// at the feet (the common rig convention).
    pub feet_offset: f32,
}

impl Swimmer {
    pub fn new() -> Self {
        Swimmer::default()
    }
}

/// Per-fixed-step swim update: for every entity carrying a [`Swimmer`] + `Transform`, query the
/// watermap under its XZ, compute feet-depth, and advance its FSM. Idle when no watermap is loaded or
/// no swimmers exist (the same data-driven idling the AI/vehicle systems use). Returns the number of
/// swimmers updated.
pub fn update_swim_state(
    world: &mut World,
    watermap: &crate::watermap::Watermap,
    cfg: &SwimConfig,
    wave: &crate::wave::WaveModel,
    time: f32,
) -> usize {
    use mercs2_core::Transform;
    let mut n = 0;
    for (_e, (sw, tf)) in world.query::<(&mut Swimmer, &Transform)>().iter() {
        let p = tf.translation;
        let sample = watermap.sample(p.x, p.z);
        let feet_y = p.y + sw.feet_offset;
        // Only water columns contribute depth; over land (or outside the grid) depth is negative.
        // Depth is measured against the WAVE-DISPLACED surface, so a swimmer bobs with the swell.
        sw.depth = if sample.is_water {
            sample.surface_height + wave.height_offset(p.x, p.z, time) - feet_y
        } else {
            -1.0
        };
        sw.state = cfg.advance(sw.state, sw.depth);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::glam::Vec3;
    use mercs2_core::{Transform, World};

    // The FSM's own tests (band classification, hysteresis, multi-band skips, the locomotion-mode
    // helpers) live with the FSM in `mercs2_core::swim` — deliberately not mirrored here, so the two
    // copies cannot drift. What follows tests only what this module still owns: driving `Swimmer`
    // from the watermap.

    #[test]
    fn system_drives_swimmer_from_watermap() {
        let mut world = World::new();
        // Water surface at 0 m over a wet map; character feet at -2 m => depth 2 => Submerged.
        let wm = crate::watermap::Watermap::uniform(5, 32.0, 0.0, true);
        let deep = world.spawn((
            Swimmer::new(),
            Transform::from_translation(Vec3::new(0.0, -2.0, 0.0)),
        ));
        // A character standing above the surface (feet at +1) => OnLand.
        let dry = world.spawn((
            Swimmer::new(),
            Transform::from_translation(Vec3::new(0.0, 1.0, 0.0)),
        ));
        let cfg = SwimConfig::default();
        // Flat wave field: these exercise the depth FSM in isolation, not the swell.
        let updated = update_swim_state(&mut world, &wm, &cfg, &crate::wave::WaveModel::flat(), 0.0);
        assert_eq!(updated, 2);
        assert_eq!(world.get::<&Swimmer>(deep).unwrap().state, SwimState::Submerged);
        assert_eq!(world.get::<&Swimmer>(dry).unwrap().state, SwimState::OnLand);
    }

    #[test]
    fn over_land_is_never_in_water_even_below_zero() {
        // A dry map: even a character far "below" reports OnLand because there is no water column.
        let mut world = World::new();
        let wm = crate::watermap::Watermap::uniform(5, 32.0, 100.0, false);
        let e = world.spawn((
            Swimmer::new(),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        ));
        update_swim_state(&mut world, &wm, &SwimConfig::default(), &crate::wave::WaveModel::flat(), 0.0);
        assert_eq!(world.get::<&Swimmer>(e).unwrap().state, SwimState::OnLand);
    }
}
