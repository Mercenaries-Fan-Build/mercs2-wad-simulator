//! The dynamic music engine — dual-deck crossfading state machine.
//!
//! **Oracle (audio_code_map.md §4.3, luacd 08_audio_presentation §2/§3):**
//! * `MusicStateMachine::Transition` **`FUN_0082d7a0`** — **dual-deck**: `this` and `this+0x28`, the
//!   *active* deck is the one whose `+0xc == 1`; deck states are **5 / 4 / 2**. A transition starts
//!   the *inactive* deck on the new cue and crossfades, then flips `active`.
//! * `FUN_0082d970` transition resolve — `FUN_0082df90` MusicMarkers eval, `FUN_0082e140`
//!   MusicTransitions record, `FUN_0082de20` match `(from, to)`.
//! * `MusicManager::Update` **`FUN_00600450`** — pause sync + music-index change drives
//!   `FUN_0082d920/d7a0/d6e0`.
//! * Lua surface: `Sound.AddMusicState` (`FUN_005fb460→FUN_00600d30`, 0x128-byte state record),
//!   `Sound.AddMusicTransition` (`FUN_005fb4b0→FUN_00600df0`, links from→to), `Sound.BindMusicCue`
//!   (`FUN_00600eb0`), `Sound.TransitionMusic` (`FUN_005e1600` → musicSM `+0x115C`).
//!
//! State parameters (`AddMusicState(name, p2..p6)`) and the concrete state table (`none`/`explore`/
//! `action`/`hijack`/…) come from `MrxMusic` (luacd §3). Deck states 5/4/2 map to
//! [`DeckState::{FadingIn, Playing, FadingOut}`]. The per-region machine lives at
//! `soundsys +0x48 + regionIdx*0x119C`; a single [`MusicStateMachine`] models one such region.

use std::collections::HashMap;

use mercs2_formats::hash::pandemic_hash_m2;

/// A declared music state (`Sound.AddMusicState`, 0x128-byte record `FUN_00600d30`). The `p2..p6`
/// positional args are stored verbatim (luacd §3 tabulates their meaning per state).
#[derive(Clone, Debug)]
pub struct MusicState {
    pub name: String,
    pub name_hash: u32,
    /// Positional native args p2..p6 (e.g. threshold / action-level / fade-in seconds / priority).
    /// `p5` is used here as the **crossfade length** in seconds (the "interval"/fade slot).
    pub params: [f32; 5],
    /// Bound cue GUIDs by index (`Sound.BindMusicCue(faction, state, index, cue)`; index ∈ (0,4)).
    pub cues: [u32; 4],
}

impl MusicState {
    /// The crossfade length this state requests when transitioned *to* (p5 slot). Non-negative.
    pub fn fade_len(&self) -> f32 {
        self.params[3].abs().max(0.0)
    }
}

/// A declared legal transition `from → to` (`Sound.AddMusicTransition`, `FUN_00600df0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicTransition {
    pub from: u32,
    pub to: u32,
}

/// Deck playback state (`FUN_0082d7a0` deck states 5/4/2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckState {
    /// No cue loaded / silent. (Deck `+0xc == 0`.)
    Idle,
    /// State 5 — ramping its cue up during a crossfade.
    FadingIn,
    /// State 4 — the live/active deck at full gain. (`+0xc == 1`.)
    Playing,
    /// State 2 — ramping down and about to go idle.
    FadingOut,
}

/// One music deck: a cue and its current mix gain.
#[derive(Clone, Copy, Debug)]
pub struct Deck {
    pub state: DeckState,
    pub cue: u32,
    pub gain: f32,
}

impl Default for Deck {
    fn default() -> Self {
        Deck {
            state: DeckState::Idle,
            cue: 0,
            gain: 0.0,
        }
    }
}

/// The dual-deck music state machine for one faction/region.
#[derive(Clone, Debug, Default)]
pub struct MusicStateMachine {
    decks: [Deck; 2],
    /// Index of the active deck (the one whose `+0xc == 1`).
    active: usize,
    states: HashMap<u32, MusicState>,
    transitions: Vec<MusicTransition>,
    /// The name-hash of the current logical state (what `TransitionMusic` last targeted).
    current: u32,
    /// Seconds and total-length of the crossfade in flight (`0.0..=len`), if any.
    fade_t: f32,
    fade_len: f32,
    fading: bool,
    /// Dynamic-music enable (Lua `Sound.SetDynamicMusic`); transitions are honored only when true.
    dynamic: bool,
}

impl MusicStateMachine {
    /// A fresh, dynamic-music-enabled machine with both decks idle.
    pub fn new() -> MusicStateMachine {
        MusicStateMachine {
            dynamic: true,
            ..Default::default()
        }
    }

    /// `Sound.AddMusicState` — declare a state (or replace one of the same name).
    pub fn add_music_state(&mut self, name: &str, params: [f32; 5]) {
        let h = pandemic_hash_m2(name);
        self.states.insert(
            h,
            MusicState {
                name: name.to_string(),
                name_hash: h,
                params,
                cues: [0; 4],
            },
        );
    }

    /// `Sound.AddMusicTransition` — declare a legal `from → to` edge.
    pub fn add_music_transition(&mut self, from: &str, to: &str) {
        self.transitions.push(MusicTransition {
            from: pandemic_hash_m2(from),
            to: pandemic_hash_m2(to),
        });
    }

    /// `Sound.BindMusicCue(state, index, cue)` — bind cue GUID at index ∈ (0,4) of a state.
    pub fn bind_music_cue(&mut self, state: &str, index: usize, cue: u32) {
        if let Some(s) = self.states.get_mut(&pandemic_hash_m2(state)) {
            if index < s.cues.len() {
                s.cues[index] = cue;
            }
        }
    }

    /// `Sound.SetDynamicMusic(bEnable)`.
    pub fn set_dynamic(&mut self, enable: bool) {
        self.dynamic = enable;
    }
    /// `Sound.IsDynamicMusic`.
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }

    /// Whether a `from → to` edge is declared (the exe honours only declared transitions; an
    /// undeclared target still switches but is flagged so the caller can log it, matching
    /// `FUN_0082de20` match behaviour).
    pub fn transition_declared(&self, from: u32, to: u32) -> bool {
        self.transitions
            .iter()
            .any(|t| t.from == from && t.to == to)
    }

    /// `Sound.TransitionMusic(state)` (`FUN_005e1600` → `FUN_0082d7a0`): start a crossfade to `state`.
    ///
    /// Loads the target state's cue 0 onto the **inactive** deck (state 5 / FadingIn), sets the active
    /// deck to state 2 / FadingOut, and begins a crossfade of length = target state's fade slot.
    /// A transition to the state already playing is a no-op. Returns `true` if a crossfade started.
    pub fn transition(&mut self, state: &str) -> bool {
        let to = pandemic_hash_m2(state);
        self.transition_hash(to)
    }

    /// Same as [`transition`](Self::transition) but by pre-hashed state id.
    pub fn transition_hash(&mut self, to: u32) -> bool {
        if !self.dynamic {
            return false;
        }
        let Some(target) = self.states.get(&to) else {
            return false;
        };
        if to == self.current && self.decks[self.active].state == DeckState::Playing {
            return false; // already there
        }
        let cue = target.cues[0];
        let len = target.fade_len();

        let inactive = 1 - self.active;
        // Inactive deck picks up the new cue and fades in.
        self.decks[inactive] = Deck {
            state: DeckState::FadingIn,
            cue,
            gain: 0.0,
        };
        // Active deck (if live) fades out.
        if self.decks[self.active].state != DeckState::Idle {
            self.decks[self.active].state = DeckState::FadingOut;
        }
        self.current = to;
        self.fade_t = 0.0;
        self.fade_len = len;
        self.fading = true;
        if len <= 0.0 {
            // Instant swap.
            self.finish_fade();
        }
        true
    }

    fn finish_fade(&mut self) {
        let inactive = 1 - self.active;
        self.decks[inactive].state = DeckState::Playing;
        self.decks[inactive].gain = 1.0;
        self.decks[self.active] = Deck::default(); // old active goes idle/silent
        self.active = inactive;
        self.fading = false;
        self.fade_t = 0.0;
        self.fade_len = 0.0;
    }

    /// `MusicManager::Update` (`FUN_00600450`) crossfade tick: advance an in-flight crossfade by `dt`.
    pub fn tick(&mut self, dt: f32) {
        if !self.fading {
            return;
        }
        if self.fade_len <= 0.0 {
            self.finish_fade();
            return;
        }
        self.fade_t = (self.fade_t + dt).min(self.fade_len);
        let t = self.fade_t / self.fade_len;
        let inactive = 1 - self.active;
        self.decks[inactive].gain = t; // fade in
        self.decks[self.active].gain = 1.0 - t; // fade out
        if self.fade_t >= self.fade_len {
            self.finish_fade();
        }
    }

    /// The active deck (the one whose `+0xc == 1`).
    pub fn active_deck(&self) -> &Deck {
        &self.decks[self.active]
    }
    /// The inactive deck.
    pub fn inactive_deck(&self) -> &Deck {
        &self.decks[1 - self.active]
    }
    /// Both decks (for the mixer to sum).
    pub fn decks(&self) -> &[Deck; 2] {
        &self.decks
    }
    /// True while a crossfade is in progress (both decks contributing).
    pub fn is_crossfading(&self) -> bool {
        self.fading
    }
    /// Current logical state name-hash.
    pub fn current_state(&self) -> u32 {
        self.current
    }
    /// Look up a declared state by name.
    pub fn state(&self, name: &str) -> Option<&MusicState> {
        self.states.get(&pandemic_hash_m2(name))
    }
}
