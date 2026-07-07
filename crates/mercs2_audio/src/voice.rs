//! The voice pool: allocation, **priority-steal**, and the **16-state instance FSM**.
//!
//! **Oracle (audio_code_map.md §1, §3.1, §3.5, §8):** the PC build has **no hardware voice pool** —
//! the entire `PalSoundXenonVoiceManager::{CreateVoice,DeleteVoice,KillOldVoice,…}` family has *no PC
//! counterpart*. Waves are software-mixed into one streaming DirectSound buffer, and contention is
//! handled purely in shared code via:
//! * `PalSoundInstance::GetLowestPrioritySound` **`FUN_00837830`** — victim selection.
//! * `PalSoundInstance::StealWave` **`FUN_00837c50`** — wave→Stop(1), state→3/2, `FUN_008387b0` detach.
//! * `PalSoundInstance::GetWavePriority` **`~0x00837e30`** (Ghidra gap; string `0xbe2120`).
//!
//! The per-instance lifetime is `PalSoundInstance::Update` **`FUN_00836c70`** (a ~3 KB state machine
//! with 8 inlined scope-named sub-passes: CalcSubmitValues / CreateWave / WaveUpdate / MaxDistCheck /
//! StopCheck / WaveSubmitValues / CheckFinished) driving the instance state byte at **`+0x88`**
//! (`0 starting / 1 playing / 2 finished / 3 steal-pending`). The high-level Pg per-instance machine
//! is `FUN_006036c0` (1127 B). [`InstanceState`] is the union of both: a **16-state** lifecycle whose
//! variants each carry a `// oracle:` note back to a `+0x88` value or an inlined sub-pass.

/// The 16-state sound-instance FSM.
///
/// The exe stores a compact state byte at instance `+0x88` (4 values); the richer lifecycle here
/// unrolls the inlined sub-passes of `FUN_00836c70` and the Pg-level machine `FUN_006036c0` into
/// distinct, testable states. The `+0x88` mapping is noted per variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstanceState {
    /// Slot unused / available for allocation. oracle: not yet an instance.
    Free = 0,
    /// Allocated from the pool ring (`FUN_00834660`), parameters bound, not yet started.
    Allocated = 1,
    /// Waiting out the distance start-delay (`+0x70`, `FUN_008369e0`). oracle: `+0x88 == 0`.
    StartDelay = 2,
    /// Start latched; about to create its wave. oracle: `+0x88 == 0` (starting).
    Starting = 3,
    /// CreateWave sub-pass — wave object built/format-bound. oracle: inlined `CreateWave`.
    CreatingWave = 4,
    /// Actively playing (submitting samples each mix). oracle: `+0x88 == 1` (playing).
    Playing = 5,
    /// Playing and set to loop at end-of-wave rather than finish. oracle: `+0x88 == 1`, loop flag.
    Looping = 6,
    /// Volume being pulled down by a category duck/fade while still audible. oracle: WaveSubmitValues.
    Ducked = 7,
    /// Held (system pause / `Sound.PauseSound`); cursor frozen. oracle: Pg UpdatePause `FUN_006079c0`.
    Paused = 8,
    /// Fading toward stop (explicit `StopSound` with a fade). oracle: StopCheck sub-pass.
    FadingOut = 9,
    /// Marked as a steal victim; wave being stopped for a higher-priority cue.
    /// oracle: `StealWave` `FUN_00837c50`, `+0x88 == 3`.
    StealPending = 10,
    /// Stop requested, wave being torn down. oracle: `PalSoundInstance::Reset` `FUN_00836be0`, `+0x88 == 2`.
    Stopping = 11,
    /// Culled because it went past `max_dist`. oracle: `MaxDistCheck` inlined pass.
    MaxDistCulled = 12,
    /// Wave reached its natural end. oracle: `CheckFinished` sub-pass, `+0x88 == 2` (finished).
    Finished = 13,
    /// Wave detached from its source (`FUN_008387b0`); instance about to free.
    Detaching = 14,
    /// Reaped by the source reap loop (`FUN_00838670`); returns to [`Free`](Self::Free) next tick.
    Reaped = 15,
}

impl InstanceState {
    /// Is the voice producing samples this tick? (Playing/Looping/Ducked/FadingOut all submit.)
    pub fn is_audible(&self) -> bool {
        matches!(
            self,
            InstanceState::Playing
                | InstanceState::Looping
                | InstanceState::Ducked
                | InstanceState::FadingOut
        )
    }

    /// Has the instance reached a terminal state the pool can recycle?
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InstanceState::Finished
                | InstanceState::MaxDistCulled
                | InstanceState::Detaching
                | InstanceState::Reaped
        )
    }

    /// The compact `+0x88` state byte the exe would store for this lifecycle state.
    pub fn engine_state_byte(&self) -> u8 {
        match self {
            InstanceState::Playing | InstanceState::Looping | InstanceState::Ducked => 1,
            InstanceState::Stopping
            | InstanceState::Finished
            | InstanceState::MaxDistCulled
            | InstanceState::FadingOut => 2,
            InstanceState::StealPending => 3,
            _ => 0,
        }
    }
}

/// A stable handle to a voice slot in the [`VoicePool`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId(pub u32);

/// Parameters to start one voice — the fields the mixer and the steal logic need. Populated from a
/// resolved [`crate::sounddb::CueEntry`] plus per-emitter overrides.
#[derive(Clone, Debug)]
pub struct VoiceRequest {
    /// Resolved cue GUID (for diagnostics / VO cancel-by-cue).
    pub cue_guid: u32,
    /// Steal priority (higher wins). oracle: `GetWavePriority`.
    pub priority: u8,
    /// Category id (indexes [`crate::categories::Categories`]).
    pub category: u8,
    /// Base linear gain before category/attenuation.
    pub gain: f32,
    /// True if the voice loops until stopped.
    pub looping: bool,
    /// True if 3D-positional (panned/attenuated).
    pub positional: bool,
    /// Start delay in seconds (distance / speed-of-sound); `StartDelay` until it elapses.
    pub start_delay: f32,
}

impl Default for VoiceRequest {
    fn default() -> Self {
        VoiceRequest {
            cue_guid: 0,
            priority: 128,
            category: 0,
            gain: 1.0,
            looping: false,
            positional: false,
            start_delay: 0.0,
        }
    }
}

/// A single voice slot. Mixing state (the sample source, cursor, computed pan/atten) lives in the
/// mixer; this carries the FSM-relevant fields.
#[derive(Clone, Debug)]
pub struct Voice {
    pub id: VoiceId,
    pub state: InstanceState,
    pub cue_guid: u32,
    pub priority: u8,
    pub category: u8,
    /// Base gain (0..1).
    pub gain: f32,
    /// Current fade/duck multiplier applied on top of `gain` (1.0 = none).
    pub fade: f32,
    pub looping: bool,
    pub positional: bool,
    /// Remaining start delay in seconds.
    pub start_delay: f32,
    /// Age in seconds since the voice entered [`InstanceState::Playing`] (tie-breaker for steal).
    pub age: f32,
}

impl Voice {
    fn free(id: VoiceId) -> Voice {
        Voice {
            id,
            state: InstanceState::Free,
            cue_guid: 0,
            priority: 0,
            category: 0,
            gain: 0.0,
            fade: 1.0,
            looping: false,
            positional: false,
            start_delay: 0.0,
            age: 0.0,
        }
    }
}

/// The fixed-capacity voice pool. Mirrors the PC instance pool (`FUN_00833e80` inits `0xA0`=160 pool
/// objects); the capacity is configurable here.
#[derive(Clone, Debug)]
pub struct VoicePool {
    voices: Vec<Voice>,
}

impl VoicePool {
    /// Instance-pool size in the PC build (`PalInstanceAllocator` inits 0xA0 objects).
    pub const DEFAULT_CAPACITY: usize = 160;

    /// A pool with `capacity` slots, all [`InstanceState::Free`].
    pub fn new(capacity: usize) -> VoicePool {
        let voices = (0..capacity)
            .map(|i| Voice::free(VoiceId(i as u32)))
            .collect();
        VoicePool { voices }
    }

    /// Total slot count.
    pub fn capacity(&self) -> usize {
        self.voices.len()
    }

    /// Currently non-free voices.
    pub fn active_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state != InstanceState::Free)
            .count()
    }

    /// Immutable/mutable access to a voice.
    pub fn get(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.get(id.0 as usize)
    }
    pub fn get_mut(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.get_mut(id.0 as usize)
    }

    /// Iterate active voices.
    pub fn iter_active(&self) -> impl Iterator<Item = &Voice> {
        self.voices
            .iter()
            .filter(|v| v.state != InstanceState::Free)
    }

    /// `PalSoundInstance::GetLowestPrioritySound` (`FUN_00837830`): the *stealable* voice with the
    /// lowest priority. Ties broken by **oldest** (largest `age`) — the exe's KillOldVoice bias.
    /// Free/terminal/steal-pending slots are not victims. Returns `None` if nothing is stealable.
    pub fn get_lowest_priority(&self) -> Option<VoiceId> {
        self.voices
            .iter()
            .filter(|v| {
                v.state != InstanceState::Free
                    && v.state != InstanceState::StealPending
                    && !v.state.is_terminal()
            })
            .min_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then(b.age.total_cmp(&a.age)) // older loses ties
            })
            .map(|v| v.id)
    }

    /// Acquire a voice for `req`.
    ///
    /// 1. Reuse a [`Free`](InstanceState::Free) slot if one exists.
    /// 2. Otherwise pick the lowest-priority active voice (`get_lowest_priority`). If its priority is
    ///    **strictly less** than the request's, **steal** it (`StealWave` `FUN_00837c50`) and reuse
    ///    the slot; if not, the request is denied (a higher- or equal-priority voice already owns
    ///    every slot — the exe drops the new cue).
    pub fn acquire(&mut self, req: &VoiceRequest) -> Option<VoiceId> {
        // 1. free slot
        if let Some(v) = self
            .voices
            .iter_mut()
            .find(|v| v.state == InstanceState::Free)
        {
            let id = v.id;
            Self::start_voice(v, req);
            return Some(id);
        }
        // 2. steal the lowest-priority voice if we outrank it
        let victim = self.get_lowest_priority()?;
        let victim_prio = self.voices[victim.0 as usize].priority;
        if req.priority <= victim_prio {
            return None; // cannot outrank the field — cue is dropped, faithful to the exe
        }
        // StealWave: stop + detach the victim, then reuse the slot.
        {
            let v = &mut self.voices[victim.0 as usize];
            v.state = InstanceState::StealPending;
        }
        let v = &mut self.voices[victim.0 as usize];
        Self::start_voice(v, req);
        Some(victim)
    }

    fn start_voice(v: &mut Voice, req: &VoiceRequest) {
        v.cue_guid = req.cue_guid;
        v.priority = req.priority;
        v.category = req.category;
        v.gain = req.gain;
        v.fade = 1.0;
        v.looping = req.looping;
        v.positional = req.positional;
        v.start_delay = req.start_delay;
        v.age = 0.0;
        v.state = if req.start_delay > 0.0 {
            InstanceState::StartDelay
        } else {
            InstanceState::Starting
        };
    }

    /// Request a stop on a voice (`Sound.StopSound`). With `fade > 0` it enters [`FadingOut`];
    /// otherwise it goes straight to [`Stopping`]. (InstanceState `+0x88` → 2.)
    pub fn stop(&mut self, id: VoiceId, fade: bool) {
        if let Some(v) = self.get_mut(id) {
            if v.state == InstanceState::Free {
                return;
            }
            v.state = if fade {
                InstanceState::FadingOut
            } else {
                InstanceState::Stopping
            };
        }
    }

    /// Advance every voice's FSM by `dt` seconds. This is the shared-code half of
    /// `PalSoundInstance::Update` (`FUN_00836c70`): resolve start delays, promote Starting→Playing,
    /// tear down Stopping/Finished, and recycle terminal slots to Free. Sample production and the
    /// end-of-wave `CheckFinished` signal come from the mixer, which flips a voice to
    /// [`Finished`](InstanceState::Finished) via [`mark_finished`](Self::mark_finished).
    pub fn tick(&mut self, dt: f32) {
        for v in &mut self.voices {
            match v.state {
                InstanceState::StartDelay => {
                    v.start_delay -= dt;
                    if v.start_delay <= 0.0 {
                        v.state = InstanceState::Starting;
                    }
                }
                InstanceState::Starting => {
                    v.state = InstanceState::CreatingWave;
                }
                InstanceState::CreatingWave => {
                    v.state = if v.looping {
                        InstanceState::Looping
                    } else {
                        InstanceState::Playing
                    };
                }
                InstanceState::Playing | InstanceState::Looping | InstanceState::Ducked => {
                    v.age += dt;
                }
                InstanceState::FadingOut => {
                    v.age += dt;
                    v.fade -= dt * 4.0; // ~0.25 s stop fade
                    if v.fade <= 0.0 {
                        v.fade = 0.0;
                        v.state = InstanceState::Stopping;
                    }
                }
                InstanceState::StealPending | InstanceState::Stopping => {
                    v.state = InstanceState::Detaching;
                }
                InstanceState::Finished
                | InstanceState::MaxDistCulled
                | InstanceState::Detaching => {
                    v.state = InstanceState::Reaped;
                }
                InstanceState::Reaped => {
                    *v = Voice::free(v.id);
                }
                InstanceState::Free | InstanceState::Paused | InstanceState::Allocated => {}
            }
        }
    }

    /// The mixer signals a wave reached its end (`CheckFinished`): finish it, or restart if looping.
    pub fn mark_finished(&mut self, id: VoiceId) {
        if let Some(v) = self.get_mut(id) {
            if v.looping {
                v.age = 0.0; // loop: keep playing
            } else {
                v.state = InstanceState::Finished;
            }
        }
    }

    /// The mixer signals a positional voice went past `max_dist` (`MaxDistCheck`).
    pub fn mark_maxdist(&mut self, id: VoiceId) {
        if let Some(v) = self.get_mut(id) {
            v.state = InstanceState::MaxDistCulled;
        }
    }
}
