//! Voice-over (VO) — priority-arbitrated dialogue.
//!
//! **Oracle (audio_code_map.md §5, luacd 08_audio_presentation §2/§3):**
//! * VO table @`0x00B988B0` (11 cfuncs); update `VO/dialog` **`FUN_00515300`**; VO manager singleton
//!   `DAT_01175dbc`.
//! * `VO.Cue` shim `FUN_005e9de0` → **`thunk_FUN_028da000(speaker, cue, priority, …)`** —
//!   SecuROM-morphed, `// CONFIRM-LIVE:`.
//! * `VO.Cancel` shim `FUN_005ea0a0` → `FUN_005150d0`: scans the VO queue `DAT_01175dbc`, fires the
//!   cancel callback, net-replicates.
//! * `VO.PRIORITY_*` constants come from a postamble Lua chunk @`0xBBA910` (not C functions); the
//!   values used by `MrxVoSequence` (luacd §3) are reproduced in [`VoPriority`].
//!
//! Arbitration (from `MrxVoSequence.Start`): a **higher** priority pre-empts the current line; an
//! **equal or lower** priority is rejected. A cued line routes to a mixer voice in the `vo` category.

/// VO priority levels (`VO.PRIORITY_*`, postamble @`0xBBA910`). Higher pre-empts lower.
///
/// Ordering matches `MrxVoSequence` usage (cinematic is highest; freeplay lowest). The numeric values
/// are the arbitration order — a cue only plays if its priority is **strictly greater** than the
/// line currently playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum VoPriority {
    /// `VO.PRIORITY_SCRIPTED_FREEPLAY`.
    Freeplay = 1,
    /// `VO.PRIORITY_SCRIPTED_BOUNTIES`.
    Bounties = 2,
    /// `VO.PRIORITY_SCRIPTED_CONTRACT` (the `MrxVoSequence` default).
    Contract = 3,
    /// `VO.PRIORITY_SCRIPTED_BRIEFING`.
    Briefing = 4,
    /// `VO.PRIORITY_CINEMATIC` — highest, pre-empts everything.
    Cinematic = 5,
}

/// A VO line currently owned by the manager.
#[derive(Clone, Debug)]
pub struct VoLine {
    /// Speaker id (character/actor).
    pub speaker: u32,
    /// Cue GUID being spoken.
    pub cue: u32,
    /// Its arbitration priority.
    pub priority: VoPriority,
    /// The mixer voice carrying the samples (if started), for cancel/stop.
    pub voice: Option<crate::voice::VoiceId>,
    /// Whether subtitles are shown (`VO.Cue` vs `VO.CueWithoutSubtitles`).
    pub subtitles: bool,
    /// Paused (`VO.Pause`).
    pub paused: bool,
}

/// The VO manager (`DAT_01175dbc`): one arbitrated active line (plus a small history for CancelAll).
#[derive(Clone, Debug, Default)]
pub struct VoManager {
    active: Option<VoLine>,
    /// `VO.SetCinematicMode` — ducks non-cinematic categories while a cinematic plays.
    cinematic_mode: bool,
}

impl VoManager {
    /// Empty manager.
    pub fn new() -> VoManager {
        VoManager::default()
    }

    /// `VO.Cue(speaker, cue, priority)` (`FUN_005e9de0` → `thunk_FUN_028da000`).
    ///
    /// // CONFIRM-LIVE: the exe's cue dispatch is SecuROM-morphed (`thunk_FUN_028da000`); this models
    /// the observable arbitration (`MrxVoSequence.Start`): accept iff `priority` **strictly exceeds**
    /// the currently-playing line's, pre-empting it. Returns `true` if the line was accepted.
    pub fn cue(&mut self, speaker: u32, cue: u32, priority: VoPriority, subtitles: bool) -> bool {
        if let Some(cur) = &self.active {
            if !cur.paused && priority <= cur.priority {
                return false; // equal/lower loses — rejected
            }
        }
        self.active = Some(VoLine {
            speaker,
            cue,
            priority,
            voice: None,
            subtitles,
            paused: false,
        });
        true
    }

    /// Attach the mixer voice that was allocated for the active line (so cancel can stop it).
    pub fn set_active_voice(&mut self, voice: crate::voice::VoiceId) {
        if let Some(l) = &mut self.active {
            l.voice = Some(voice);
        }
    }

    /// `VO.Cancel(cue)` (`FUN_005150d0`): cancel the active line if it matches `cue`. Returns the
    /// mixer voice to stop, if any.
    pub fn cancel(&mut self, cue: u32) -> Option<crate::voice::VoiceId> {
        if self.active.as_ref().map(|l| l.cue) == Some(cue) {
            let v = self.active.take().and_then(|l| l.voice);
            return v;
        }
        None
    }

    /// `VO.CancelAll` — cancel whatever is playing. Returns its voice to stop.
    pub fn cancel_all(&mut self) -> Option<crate::voice::VoiceId> {
        self.active.take().and_then(|l| l.voice)
    }

    /// `VO.Pause` / `VO.Unpause` on the active line.
    pub fn set_paused(&mut self, paused: bool) {
        if let Some(l) = &mut self.active {
            l.paused = paused;
        }
    }

    /// `VO.SetCinematicMode(bEnable)`.
    pub fn set_cinematic_mode(&mut self, enable: bool) {
        self.cinematic_mode = enable;
    }
    /// Is cinematic mode active?
    pub fn cinematic_mode(&self) -> bool {
        self.cinematic_mode
    }

    /// The active line, if any.
    pub fn active(&self) -> Option<&VoLine> {
        self.active.as_ref()
    }

    /// True if a line is currently owned (playing or paused).
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Called when the active line's mixer voice finishes naturally.
    pub fn on_voice_finished(&mut self, voice: crate::voice::VoiceId) {
        if self.active.as_ref().and_then(|l| l.voice) == Some(voice) {
            self.active = None;
        }
    }
}
