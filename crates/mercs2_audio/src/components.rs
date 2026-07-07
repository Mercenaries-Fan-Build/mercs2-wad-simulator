//! Audio ECS components — attached to `mercs2_core::World` entities.
//!
//! These are the audio-silo's own components (the plan lets silo 14 define its ECS comps here). They
//! mirror the runtime sound components the exe's Pg layer collects each frame
//! (`RuntimeSoundUpdates / Rt*Collect` **`FUN_005fa720`**, 3 ECS pools; audio_code_map.md §4.1) and
//! the listener the engine tracks for 3D (`UpdateListeners` `FUN_00608aa0`).

use mercs2_core::glam::Vec3;

use crate::voice::VoiceId;

/// Marks an entity as an **audio listener** (usually the camera/player). Its world position drives
/// 3D attenuation/pan for every positional emitter. Up to [`crate::spatial::MAX_LISTENERS`] may exist
/// (split-screen co-op); `slot` selects which engine listener index this entity owns.
#[derive(Clone, Copy, Debug)]
pub struct AudioListener {
    /// Engine listener index (0..4).
    pub slot: usize,
    /// Velocity for Doppler (world units/sec); the streaming/physics silo fills this.
    pub velocity: Vec3,
}

impl Default for AudioListener {
    fn default() -> Self {
        AudioListener {
            slot: 0,
            velocity: Vec3::ZERO,
        }
    }
}

/// A **3D sound emitter** on an entity — a positional cue the runtime-sound pass submits each frame.
/// Corresponds to the `RtSound*` runtime components the exe collects.
#[derive(Clone, Debug)]
pub struct SoundEmitter {
    /// Cue GUID to play (resolved via the sound DB).
    pub cue: u32,
    /// World position the sound emits from (usually the entity transform's translation).
    pub position: Vec3,
    /// Emitter velocity for Doppler.
    pub velocity: Vec3,
    /// Attenuation: full volume within this radius.
    pub min_dist: f32,
    /// Attenuation: silent beyond this radius.
    pub max_dist: f32,
    /// The live mixer voice, once started (`None` until the runtime pass allocates one).
    pub voice: Option<VoiceId>,
    /// Whether this emitter should be (re)started; cleared once a voice is allocated.
    pub retrigger: bool,
}

impl SoundEmitter {
    /// A one-shot emitter for `cue` at `position`.
    pub fn new(cue: u32, position: Vec3) -> SoundEmitter {
        SoundEmitter {
            cue,
            position,
            velocity: Vec3::ZERO,
            min_dist: 1.0,
            max_dist: 100.0,
            voice: None,
            retrigger: true,
        }
    }
}
