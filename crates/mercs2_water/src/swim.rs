//! The TPS character swim-state FSM — the third-person player/NPC water state driven by how deep the
//! character is under the water surface.
//!
//! **Scope note / honesty boundary.** This is the *water-and-swimming scope* mechanism
//! ([[water-and-swimming-scope]]): "the waterline is tracked, we just have to match the swimming."
//! The water **code map does not string-pin swim depth thresholds** — `FUN_00480440` (the waterline
//! query) supplies the surface height, and the swim state / swim clips are the data-and-animation side
//! of the scope (the exact `FUN_00480440` return packing is itself confirm-live, code map §5). So the
//! *state vocabulary* and the *depth-drives-state* shape here are faithful to the scope; the numeric
//! thresholds live on [`SwimConfig`] and are **gameplay-derived from character height, NOT recovered
//! from the exe** — they are labelled as such and kept as tunable config so an exe-confirmed value can
//! replace them without touching the FSM.
//!
//! Model: let `depth = surface_height − feet_y` = how far the character's feet sit below the water
//! surface (negative when the feet are above the water). The FSM is a monotone classification of
//! `depth`:
//!
//! | depth | state | meaning |
//! |---|---|---|
//! | `depth ≤ 0` | [`OnLand`](SwimState::OnLand) | feet above the surface — normal locomotion |
//! | `0 < depth < wade_depth` | [`Wading`](SwimState::Wading) | in the shallows, still ground-supported |
//! | `wade_depth ≤ depth < swim_depth` | [`Swimming`](SwimState::Swimming) | buoyant, swim locomotion, no ground snap |
//! | `depth ≥ swim_depth` | [`Submerged`](SwimState::Submerged) | head under — underwater/diving |
//!
//! A small hysteresis band ([`SwimConfig::hysteresis_m`]) keeps the state from chattering when the
//! character bobs across a boundary — the FSM is advanced from its *previous* state so a boundary must
//! be crossed by the full band before the state flips.

use mercs2_core::World;

/// The third-person swim state. Ordered land→deep so comparisons read naturally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SwimState {
    /// Feet above water — normal ground locomotion.
    #[default]
    OnLand,
    /// Standing in shallow water, still ground-supported (wade animations, minor drag).
    Wading,
    /// Buoyant at the surface — swim locomotion, no ground snap.
    Swimming,
    /// Head below the surface — underwater / diving.
    Submerged,
}

impl SwimState {
    /// Whether the character is in the water at all (anything past [`OnLand`](Self::OnLand)).
    pub fn in_water(self) -> bool {
        self != SwimState::OnLand
    }

    /// Whether the character is off the ground and swimming (Swimming or Submerged) — the locomotion
    /// switch: no ground snap, swim clips, reduced control authority.
    pub fn is_swimming(self) -> bool {
        matches!(self, SwimState::Swimming | SwimState::Submerged)
    }
}

/// Depth thresholds for the swim FSM. **Defaults are gameplay-derived from a ~1.8 m human, NOT
/// exe-recovered** (see the module docs / the water-and-swimming scope). `wade_depth` ≈ waist height,
/// `swim_depth` ≈ where the head submerges. Tunable so an exe-confirmed value can override.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwimConfig {
    /// Feet-depth at which the character transitions from wading to swimming (≈ waist, ~1.0 m).
    pub wade_depth: f32,
    /// Feet-depth at which the head submerges → underwater (≈ standing height, ~1.7 m).
    pub swim_depth: f32,
    /// Hysteresis half-band (m) applied at every boundary to prevent state chatter while bobbing.
    pub hysteresis_m: f32,
}

impl Default for SwimConfig {
    fn default() -> Self {
        SwimConfig { wade_depth: 1.0, swim_depth: 1.7, hysteresis_m: 0.1 }
    }
}

impl SwimConfig {
    /// Classify `depth` (= surface − feet_y) into a state *without* hysteresis (the raw bands). Used by
    /// [`advance`](Self::advance) and directly where no previous state exists.
    pub fn classify(&self, depth: f32) -> SwimState {
        if depth <= 0.0 {
            SwimState::OnLand
        } else if depth < self.wade_depth {
            SwimState::Wading
        } else if depth < self.swim_depth {
            SwimState::Swimming
        } else {
            SwimState::Submerged
        }
    }

    /// Advance the FSM from `prev` given the new `depth`, applying the hysteresis band so a boundary
    /// must be over-crossed by [`hysteresis_m`](Self::hysteresis_m) before the state changes. Moving
    /// *into* deeper water requires the depth to exceed the boundary by `+band`; moving *out* requires
    /// it to fall below by `−band`. This mirrors how the engine avoids single-frame swim/wade flicker
    /// at the shoreline; the band value itself is a tuning default, not an exe constant.
    pub fn advance(&self, prev: SwimState, depth: f32) -> SwimState {
        let band = self.hysteresis_m;
        // Bias each boundary in the direction that resists leaving `prev`.
        let raw = self.classify(depth);
        if raw == prev {
            return prev;
        }
        // Deepening: only accept the deeper state once past boundary + band.
        if raw > prev {
            let boundary = match prev {
                SwimState::OnLand => 0.0,
                SwimState::Wading => self.wade_depth,
                SwimState::Swimming => self.swim_depth,
                SwimState::Submerged => return prev,
            };
            if depth >= boundary + band {
                // Re-classify the far side so we can skip multiple bands at once, but never step
                // shallower than one state deeper than `prev`.
                return self.classify(depth).max(next_up(prev));
            }
            return prev;
        }
        // Shallowing: only accept the shallower state once below boundary − band.
        let boundary = match prev {
            SwimState::Submerged => self.swim_depth,
            SwimState::Swimming => self.wade_depth,
            SwimState::Wading => 0.0,
            SwimState::OnLand => return prev,
        };
        if depth <= boundary - band {
            self.classify(depth).min(next_down(prev))
        } else {
            prev
        }
    }
}

/// The next deeper state (saturating at Submerged).
fn next_up(s: SwimState) -> SwimState {
    match s {
        SwimState::OnLand => SwimState::Wading,
        SwimState::Wading => SwimState::Swimming,
        SwimState::Swimming | SwimState::Submerged => SwimState::Submerged,
    }
}

/// The next shallower state (saturating at OnLand).
fn next_down(s: SwimState) -> SwimState {
    match s {
        SwimState::Submerged => SwimState::Swimming,
        SwimState::Swimming => SwimState::Wading,
        SwimState::Wading | SwimState::OnLand => SwimState::OnLand,
    }
}

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
) -> usize {
    use mercs2_core::Transform;
    let mut n = 0;
    for (_e, (sw, tf)) in world.query::<(&mut Swimmer, &Transform)>().iter() {
        let p = tf.translation;
        let sample = watermap.sample(p.x, p.z);
        let feet_y = p.y + sw.feet_offset;
        // Only water columns contribute depth; over land (or outside the grid) depth is negative.
        sw.depth = if sample.is_water { sample.surface_height - feet_y } else { -1.0 };
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

    #[test]
    fn classify_bands_are_monotone() {
        let c = SwimConfig::default();
        assert_eq!(c.classify(-0.5), SwimState::OnLand);
        assert_eq!(c.classify(0.5), SwimState::Wading);
        assert_eq!(c.classify(1.2), SwimState::Swimming);
        assert_eq!(c.classify(2.0), SwimState::Submerged);
        // Boundaries are half-open at the lower edge (>= goes to the deeper state).
        assert_eq!(c.classify(1.0), SwimState::Swimming);
        assert_eq!(c.classify(1.7), SwimState::Submerged);
    }

    #[test]
    fn hysteresis_resists_chatter_at_a_boundary() {
        let c = SwimConfig::default(); // band 0.1, wade_depth 1.0
        // Sitting just below the wade→swim boundary as Wading: a nudge to 1.05 (< 1.0+0.1) stays.
        assert_eq!(c.advance(SwimState::Wading, 1.05), SwimState::Wading);
        // Past the band → swims.
        assert_eq!(c.advance(SwimState::Wading, 1.15), SwimState::Swimming);
        // Coming back down from Swimming: 0.95 (> 1.0-0.1) still swims (hasn't cleared the band).
        assert_eq!(c.advance(SwimState::Swimming, 0.95), SwimState::Swimming);
        // Below the band → back to wading.
        assert_eq!(c.advance(SwimState::Swimming, 0.85), SwimState::Wading);
    }

    #[test]
    fn advance_can_skip_multiple_bands_in_one_step() {
        let c = SwimConfig::default();
        // Falling straight into deep water from land in one step lands on Submerged.
        assert_eq!(c.advance(SwimState::OnLand, 3.0), SwimState::Submerged);
        // Climbing straight out lands OnLand.
        assert_eq!(c.advance(SwimState::Submerged, -0.5), SwimState::OnLand);
    }

    #[test]
    fn helpers_report_locomotion_mode() {
        assert!(!SwimState::OnLand.in_water());
        assert!(SwimState::Wading.in_water());
        assert!(!SwimState::Wading.is_swimming());
        assert!(SwimState::Swimming.is_swimming());
        assert!(SwimState::Submerged.is_swimming());
    }

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
        let updated = update_swim_state(&mut world, &wm, &cfg);
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
        update_swim_state(&mut world, &wm, &SwimConfig::default());
        assert_eq!(world.get::<&Swimmer>(e).unwrap().state, SwimState::OnLand);
    }
}
