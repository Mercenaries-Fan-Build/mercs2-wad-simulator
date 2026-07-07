//! Per-entity animation runtime — the ECS components + the fixed-tick system.
//!
//! `HumanAnimationSet` names WHAT should play (the character + its current gameplay [`StateKey`]);
//! `AnimController` is the playback state machine (current/target clip, time, crossfade, speed).
//! [`animation_system`] runs each fixed tick: resolve the state to a clip via the data-driven
//! [`ClipPicker`], drive a crossfade on clip changes, advance time, sample+blend the pose, and write
//! the [`SkinPalette`] the render silo consumes.

use crate::pose::{
    havok_palette, havok_palette_blend_in_place, havok_palette_in_place, BoneRig,
};
use crate::select::{ClipPicker, StateKey};
use mercs2_core::{Entity, ModelRef, SkinPalette, World};
use mercs2_formats::anim::QsTransform;

/// The per-entity animation set: which character's clips to select and the current gameplay state.
/// One code path for every character (mercs, NPCs, DLC costumes) — only `character` + the loaded
/// animgroup differ, exactly as the retail engine does it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanAnimationSet {
    /// `pandemic_hash_m2(merc)` — the AnimationLookup `CharacterName` key.
    pub character: u32,
    /// Current gameplay state (Stance/Action/AimState/ActionDirection/…) driving selection.
    pub state: StateKey,
    /// Crossfade duration (seconds) applied when the resolved clip changes.
    pub crossfade: f32,
    /// Gameplay speed multiplier (locomotion speed-scale): 1.0 = authored rate.
    pub speed: f32,
    /// Foot-lock: strip baked root locomotion so the clip plays in place and the entity `Transform`
    /// carries the ground motion (the locomotion default). Set `false` for authored root motion.
    pub foot_lock: bool,
}

impl Default for HumanAnimationSet {
    fn default() -> Self {
        HumanAnimationSet {
            character: 0,
            state: StateKey::idle(),
            crossfade: ANIM_BLEND_SEC,
            speed: 1.0,
            foot_lock: true,
        }
    }
}

impl HumanAnimationSet {
    pub fn new(character: u32) -> Self {
        HumanAnimationSet { character, ..Self::default() }
    }
}

/// Default crossfade seconds between handles when the transition graph (`0xAB8FE34B`) has no
/// specific `TransitionDuration` — the fixed approximation the engine falls back to.
pub const ANIM_BLEND_SEC: f32 = 0.2;

/// Playback state for one entity's animation: current + previous (fading-out) clip, their playback
/// times, and the crossfade weight. Advanced by [`AnimController::advance`] on the fixed tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimController {
    /// Currently-playing (target) clip name-hash.
    pub clip: u32,
    /// Playback time within `clip`, seconds.
    pub time: f32,
    /// Playback-rate scalar applied on top of `HumanAnimationSet::speed` (the clip's clamped
    /// time-scale). 1.0 = authored rate.
    pub time_scale: f32,
    /// Whether `clip` loops (from the ActionTable `Looping` flag).
    pub looping: bool,
    /// The clip that was playing before the last switch (fading out while `blend < 1`).
    pub prev_clip: u32,
    /// Playback time within `prev_clip`.
    pub prev_time: f32,
    /// Crossfade weight of `clip` vs `prev_clip`, 0..1. 1.0 = no fade active.
    pub blend: f32,
    /// Per-second increase of `blend` (`1/crossfade_seconds`).
    pub blend_rate: f32,
    pub playing: bool,
}

impl Default for AnimController {
    fn default() -> Self {
        AnimController {
            clip: 0,
            time: 0.0,
            time_scale: 1.0,
            looping: true,
            prev_clip: 0,
            prev_time: 0.0,
            blend: 1.0,
            blend_rate: 0.0,
            playing: false,
        }
    }
}

impl AnimController {
    /// A controller already playing `clip` from t=0 (no fade).
    pub fn playing(clip: u32) -> Self {
        AnimController { clip, playing: true, ..Self::default() }
    }

    /// Switch the target clip, starting a crossfade over `crossfade` seconds (0 = instant). No-op if
    /// `new_clip` is already the target. The outgoing clip becomes `prev_clip` and fades out.
    pub fn set_clip(&mut self, new_clip: u32, crossfade: f32) {
        if new_clip == self.clip && self.playing {
            return;
        }
        self.prev_clip = self.clip;
        self.prev_time = self.time;
        self.clip = new_clip;
        self.time = 0.0;
        self.playing = true;
        if crossfade > 1e-4 && self.prev_clip != 0 {
            self.blend = 0.0;
            self.blend_rate = 1.0 / crossfade;
        } else {
            self.blend = 1.0;
            self.blend_rate = 0.0;
            self.prev_clip = 0;
        }
    }

    /// Advance playback by `dt` seconds. `speed` is the gameplay speed-scale, `dur`/`prev_dur` the
    /// clip durations (0 = unknown → no wrap). Wraps looping clips, clamps one-shots, and advances
    /// the crossfade, dropping `prev_clip` when the fade completes.
    pub fn advance(&mut self, dt: f32, speed: f32, dur: f32, prev_dur: f32) {
        if !self.playing {
            return;
        }
        let step = dt * speed * self.time_scale;
        self.time = advance_time(self.time, step, dur, self.looping);
        if self.blend < 1.0 {
            self.prev_time = advance_time(self.prev_time, step, prev_dur, true);
            self.blend = (self.blend + self.blend_rate * dt).min(1.0);
            if self.blend >= 1.0 {
                self.prev_clip = 0;
            }
        }
    }

    /// True while a crossfade from `prev_clip` is in progress.
    pub fn fading(&self) -> bool {
        self.blend < 1.0 && self.prev_clip != 0
    }
}

/// Advance a time cursor by `step`, wrapping (loop) or clamping (one-shot) on `dur`.
fn advance_time(mut t: f32, step: f32, dur: f32, looping: bool) -> f32 {
    t += step;
    if dur > 1e-6 {
        if looping {
            t %= dur;
            if t < 0.0 {
                t += dur;
            }
        } else {
            t = t.clamp(0.0, dur);
        }
    }
    t
}

/// One clip sampled at a time — the seam between this crate and the loaded animgroup. The engine's
/// impl backs this with `mercs2_formats::anim::AnimClip::sample_local` + the clip's
/// `track_to_bone` binding.
pub struct SampledPose {
    /// Per-track local `hkQsTransform` at the requested time (length = clip's track count).
    pub locals: Vec<QsTransform>,
    /// Track index → rig bone index (`None` = unbound track), from the clip's `track_to_bone`.
    pub track_to_hier: Vec<Option<usize>>,
    /// Number of leading transform tracks (the rest are float tracks).
    pub num_transform_tracks: usize,
}

/// The asset seam the engine implements so this crate can drive an entity without depending on the
/// renderer/loader: fetch a model's bone rig, a clip's duration (for time-wrap), and a sampled pose.
pub trait AnimAssets {
    /// The bone rig (parent/inv_bind/local_bind per bone) for a model, or `None` if not resident.
    fn rig(&self, model: u32) -> Option<&[BoneRig]>;
    /// Clip length in seconds, or `None` if the clip isn't resolvable for this model.
    fn clip_duration(&self, model: u32, clip: u32) -> Option<f32>;
    /// Sample `clip` at `time` seconds into a per-track local pose, or `None` if unresolvable.
    fn sample(&self, model: u32, clip: u32, time: f32) -> Option<SampledPose>;
}

/// Sample a controller's pose into a skinning palette, crossfading `prev_clip`→`clip` while a fade is
/// active. Returns `None` if the current clip can't be sampled. Honors `foot_lock` (in-place) vs raw
/// root motion.
pub fn sample_controller_palette(
    rig: &[BoneRig],
    ctrl: &AnimController,
    model: u32,
    assets: &dyn AnimAssets,
    foot_lock: bool,
) -> Option<Vec<[[f32; 4]; 4]>> {
    let cur = assets.sample(model, ctrl.clip, ctrl.time)?;
    if ctrl.fading() {
        if let Some(prev) = assets.sample(model, ctrl.prev_clip, ctrl.prev_time) {
            // Blend weight = weight of the CURRENT clip (sample B); it grows 0→1.
            return Some(havok_palette_blend_in_place(
                rig,
                &prev.locals,
                &prev.track_to_hier,
                prev.num_transform_tracks,
                &cur.locals,
                &cur.track_to_hier,
                cur.num_transform_tracks,
                ctrl.blend,
            ));
        }
    }
    Some(if foot_lock {
        havok_palette_in_place(rig, &cur.locals, &cur.track_to_hier, cur.num_transform_tracks)
    } else {
        havok_palette(rig, &cur.locals, &cur.track_to_hier, cur.num_transform_tracks)
    })
}

/// The fixed-tick animation system. For every entity carrying `(ModelRef, HumanAnimationSet,
/// AnimController)` it:
///   1. resolves the entity's `state` to a clip via the data-driven `picker` (skipped if `None`),
///      driving a crossfade through `AnimController::set_clip` when the clip changes;
///   2. advances playback time + the crossfade on the fixed `dt`;
///   3. samples + blends the pose and writes/updates the entity's [`SkinPalette`].
///
/// This is the single entry point the engine loop calls each tick (`schedule.add_system`). `picker`
/// is `&ClipPicker` (read-only) so precompute the merc characters in its constructor; entities whose
/// character wasn't precomputed keep their current clip (no lazy mutation on the hot path).
pub fn animation_system(
    world: &mut World,
    picker: Option<&ClipPicker>,
    assets: &dyn AnimAssets,
    dt: f32,
) {
    // Collect palettes during the query, then insert after it releases the component borrows.
    let mut palettes: Vec<(Entity, Vec<[[f32; 4]; 4]>)> = Vec::new();

    for (entity, (model, set, ctrl)) in world
        .query::<(&ModelRef, &mut HumanAnimationSet, &mut AnimController)>()
        .iter()
    {
        // 1. Selection → crossfade.
        if let Some(p) = picker {
            if let Some(res) = p.resolve_indexed(set.character, set.state) {
                if res.clip != ctrl.clip || !ctrl.playing {
                    ctrl.set_clip(res.clip, set.crossfade);
                    ctrl.looping = res.looping;
                    ctrl.time_scale = clamp_time_scale(res.min_time_scale, res.max_time_scale);
                }
            }
        }

        // 2. Advance.
        let dur = assets.clip_duration(model.model, ctrl.clip).unwrap_or(0.0);
        let prev_dur = if ctrl.prev_clip != 0 {
            assets.clip_duration(model.model, ctrl.prev_clip).unwrap_or(0.0)
        } else {
            0.0
        };
        ctrl.advance(dt, set.speed, dur, prev_dur);

        // 3. Sample + blend → palette.
        if let Some(rig) = assets.rig(model.model) {
            if let Some(mats) = sample_controller_palette(rig, ctrl, model.model, assets, set.foot_lock) {
                palettes.push((entity, mats));
            }
        }
    }

    for (entity, mats) in palettes {
        // `insert_one` replaces the component if the entity already has one, else attaches it.
        let _ = world.insert_one(entity, SkinPalette { mats });
    }
}

/// Resolve the clip's clamped playback rate from its `Min/Max` time-scale (`-1` = default/unclamped).
/// With no clamp we play at the authored rate (1.0); otherwise clamp 1.0 into `[min, max]`.
fn clamp_time_scale(min: f32, max: f32) -> f32 {
    let lo = if min > 0.0 { min } else { f32::MIN };
    let hi = if max > 0.0 { max } else { f32::MAX };
    1.0f32.clamp(lo.min(hi), lo.max(hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_clip_starts_crossfade_and_advances_to_completion() {
        let mut c = AnimController::playing(0xAAAA);
        assert!(!c.fading());
        c.set_clip(0xBBBB, 0.2); // 0.2 s crossfade
        assert_eq!(c.clip, 0xBBBB);
        assert_eq!(c.prev_clip, 0xAAAA);
        assert!(c.fading());
        assert_eq!(c.blend, 0.0);
        // ~0.2 s of ticks at 1/60 s completes the fade (a couple extra for float headroom).
        for _ in 0..14 {
            c.advance(1.0 / 60.0, 1.0, 1.0, 1.0);
        }
        assert!(!c.fading(), "fade completes after ~0.2 s");
        assert_eq!(c.prev_clip, 0, "prev clip dropped when fade completes");
        assert!((c.blend - 1.0).abs() < 1e-6);
    }

    #[test]
    fn set_clip_same_target_is_noop() {
        let mut c = AnimController::playing(0xAAAA);
        c.time = 0.5;
        c.set_clip(0xAAAA, 0.2);
        assert_eq!(c.time, 0.5, "re-selecting the playing clip must not restart it");
        assert!(!c.fading());
    }

    /// A mock asset store: one bone rig, and every clip samples to the neutral (identity) pose.
    struct MockAssets {
        rig: Vec<BoneRig>,
    }
    impl MockAssets {
        fn one_bone() -> Self {
            let id = [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            MockAssets {
                rig: vec![BoneRig { parent: -1, name_hash: 1, world_bind: id, inv_bind: id, local_bind: id }],
            }
        }
    }
    impl AnimAssets for MockAssets {
        fn rig(&self, _model: u32) -> Option<&[BoneRig]> {
            Some(&self.rig)
        }
        fn clip_duration(&self, _model: u32, _clip: u32) -> Option<f32> {
            Some(1.0)
        }
        fn sample(&self, _model: u32, _clip: u32, _time: f32) -> Option<SampledPose> {
            Some(SampledPose {
                locals: vec![QsTransform::IDENTITY],
                track_to_hier: vec![Some(0)],
                num_transform_tracks: 1,
            })
        }
    }

    #[test]
    fn system_advances_and_writes_palette() {
        let mut world = World::new();
        let e = world.spawn((
            ModelRef { model: 0x1234 },
            HumanAnimationSet::new(0xD64B_B122),
            AnimController::playing(0xAAAA),
        ));
        let assets = MockAssets::one_bone();

        // No picker: the controller keeps its preset clip; the system advances time + writes palette.
        animation_system(&mut world, None, &assets, 0.1);

        let ctrl = world.get::<&AnimController>(e).unwrap();
        assert!((ctrl.time - 0.1).abs() < 1e-6, "system advanced playback time");
        drop(ctrl);
        let pal = world.get::<&SkinPalette>(e).expect("system attached a SkinPalette");
        assert_eq!(pal.mats.len(), 1, "one bone → one palette matrix");
    }

    #[test]
    fn looping_time_wraps_oneshot_clamps() {
        let mut c = AnimController::playing(0xAAAA);
        c.looping = true;
        c.advance(0.75, 1.0, 1.0, 0.0); // dur 1.0
        assert!((c.time - 0.75).abs() < 1e-6);
        c.advance(0.5, 1.0, 1.0, 0.0);
        assert!((c.time - 0.25).abs() < 1e-5, "wrapped past 1.0");

        let mut o = AnimController::playing(0xCCCC);
        o.looping = false;
        o.advance(2.0, 1.0, 1.0, 0.0);
        assert!((o.time - 1.0).abs() < 1e-6, "one-shot clamps at duration");
    }
}
