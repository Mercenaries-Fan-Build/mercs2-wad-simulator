//! A started cue, advanced frame by frame the way the engine advances a cue instance: the cue update
//! `FUN_00835060`, the track update `FUN_0083c070`, the loop restart `FUN_0083c3c0`, sound firing
//! `FUN_0083fee0`, sound instance start `FUN_008369e0` and instance update `FUN_00836c70`, all read
//! from the disassembly of the unpacked PC exe. [`crate::AudioEngine`] owns the playbacks and the
//! voices they fire; this module holds their state and the engine's arithmetic.
//!
//! **Blocks.** What passes from cue to track to instance is a [`Block`] of eight floats: a volume, a
//! pitch in semitones, and six output-channel multipliers.
//!
//! **Per frame, for a multi-track cue** (all single precision; `dt` the frame time):
//!
//! 1. The cue block is `{clamp01(cue gain × V), P, C}` with `V`, `P`, `C` the cue automation's
//!    output of the **previous** frame (1.0, 0.0 and 1.0 at start).
//! 2. The cue time advances by `dt`. If the cue's loop count is non-zero and the time reaches its loop
//!    end, every track first runs a **loop restart** ([`TrackPlayback`] below) against the cue's loop
//!    points, with the cue's raw previous-frame automation block (no gain, no clamp) and
//!    `loop end − old time`; the cue's automation rewinds to its loop start
//!    ([`crate::automation::AutomationState::rewind`]); the time becomes
//!    `loop start + ((old time + dt) − loop end)`, which is also the `dt` the tracks then advance by;
//!    and the count drops by one unless it is `0xFF`.
//! 3. The cue's event table is evaluated at the cue time.
//! 4. Each track advances (below). When every track is done: a cue whose loop count was non-zero at
//!    the start of the frame waits for its loop; otherwise it starts its child cue (kind 7) and waits
//!    for it, or is done.
//!
//! **A track** (`FUN_0083c070`) advances its time by `dt`; if its loop count is non-zero and the time
//! reaches its loop end it runs a loop restart against its own loop points, with the block
//! `{cue volume × previous track volume, cue pitch + previous track pitch, cue channels}` and
//! `loop end − old time`, then takes `loop start + ((dt + old time) − loop end)` as its time and as
//! its `dt`, and drops the count unless it is `0xFF`. It evaluates its automation, and fires and
//! updates its sounds with `{track volume × cue volume, track pitch + cue pitch, track channels × cue
//! channels}`. When its sounds are done and its loop count was 0 at the start of the frame, it starts
//! its child cue and waits for it, or is done.
//!
//! **A loop restart** (`FUN_0083c3c0`) fires the sounds that start by the loop end and updates the
//! instances with the given block and time, rewinds the automation to the loop start, rewinds the
//! sounds ([`SoundList::rewind`]) and marks the track playing.
//!
//! **Sounds** (`FUN_0083fee0`): while firing, every sound from the next unfired one whose start is at
//! or before the time fires. Then each instance that finished on an earlier update is dropped, and
//! every other one takes the override block if there is one and is updated with the block. The list
//! is fired once everything has fired and no instance is left, and done on the next update that finds
//! no instance.
//!
//! **An instance** (`FUN_00836c70`) multiplies its six channel multipliers by the block's — for good:
//! they start at 1.0 and are only reset by an override block — and plays at volume
//! `base volume × block volume` and pitch `base pitch + block pitch` semitones, at
//! [`crate::automation::pitched_rate`]; output channels 0 and 1 (left and right) take its channel
//! multipliers 0 and 1. Its base values and start delay are drawn at start, after its wave is picked:
//! base pitch `FUN_0083d700`, base volume `FUN_0083d770`, start delay `FUN_0083d7e0`
//! ([`InstanceParams::start`]). An override block (a non-zero-mode ramp: the cue's if active, else the
//! track's) replaces its base volume and pitch and resets its channel multipliers to 1.0.
//!
//! **A single-track cue** starts its one instance on its first frame and updates it with
//! `{clamp01(cue gain), 0, 1.0 × 6}`; it is done on the update after its instance finishes.
//!
//! **A multi-track cue** draws once at start (`FUN_008354e0`): it plays only if its `+0x18` value is
//! not below the draw.

use crate::automation::{AutomationOutput, AutomationState};
use crate::engine::CueHandle;
use crate::select::PalRng;
use crate::voice::VoiceId;

/// The eight floats a cue passes its tracks and a track its instances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    /// Volume multiplier.
    pub volume: f32,
    /// Pitch offset, semitones.
    pub pitch: f32,
    /// Output-channel multipliers.
    pub channels: [f32; 6],
}

impl Block {
    /// The block of an automation output, as the engine stores it (`+0x20`..`+0x3C`).
    pub fn of(out: &AutomationOutput) -> Block {
        Block { volume: out.volume, pitch: out.pitch, channels: out.channels }
    }
}

/// A cue's or a track's run state (the cue's `+0x164`, the track's `+0xD8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    /// 1: playing.
    Playing,
    /// 2: done.
    Done,
    /// 3: waiting for its child cue (or, after a stop, for its instances).
    WaitingChild,
}

/// What a group contributes to a sound instance's start (`FUN_0083d700`/`d770`/`d7e0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstanceParams {
    /// A single-wave group: fixed base volume (`+0x2C`) and pitch (`+0x30`); its start delay is drawn
    /// against the engine's zeroed default block, so it is 0 (one draw is still made).
    Single {
        /// `+0x2C`.
        volume: f32,
        /// `+0x30`, semitones.
        pitch: f32,
    },
    /// A multi-wave group: base pitch drawn in `[+0x5C, +0x60]`, base volume in `[+0x50, +0x54]`, start
    /// delay in `[max(+0x3C − +0x40, 0), +0x40 + +0x3C]`.
    Multi {
        /// `+0x5C`.
        pitch_lo: f32,
        /// `+0x60`.
        pitch_hi: f32,
        /// `+0x50`.
        volume_lo: f32,
        /// `+0x54`.
        volume_hi: f32,
        /// `+0x3C`.
        delay_a: f32,
        /// `+0x40`.
        delay_b: f32,
    },
}

/// A sound instance's drawn start values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceStart {
    /// Base volume.
    pub volume: f32,
    /// Base pitch, semitones.
    pub pitch: f32,
    /// Start delay, seconds.
    pub delay_s: f32,
}

impl InstanceParams {
    /// Draw an instance's start values in the engine's order: pitch, volume (multi-wave only), then
    /// the delay (always).
    pub fn start(&self, rng: &mut PalRng) -> InstanceStart {
        match *self {
            InstanceParams::Single { volume, pitch } => {
                // FUN_0083d7e0 on the zeroed default block: lo = max(0 − 0, 0), span = (0 + 0) − lo.
                let r = rng.next_unit();
                InstanceStart { volume, pitch, delay_s: r * 0.0 + 0.0 }
            }
            InstanceParams::Multi { pitch_lo, pitch_hi, volume_lo, volume_hi, delay_a, delay_b } => {
                let pitch = rng.next_unit() * (pitch_hi - pitch_lo) + pitch_lo;
                let volume = rng.next_unit() * (volume_hi - volume_lo) + volume_lo;
                let d = delay_a - delay_b;
                let lo = if 0.0 > d { 0.0 } else { d };
                let hi = delay_b + delay_a;
                let delay_s = rng.next_unit() * (hi - lo) + lo;
                InstanceStart { volume, pitch, delay_s }
            }
        }
    }
}

/// `clamp01` as `FUN_00835060` clamps the cue volume: negative → 0, above 1 → 1.
pub fn clamp01(v: f32) -> f32 {
    if 0.0 <= v {
        if v > 1.0 {
            1.0
        } else {
            v
        }
    } else {
        0.0
    }
}

/// The wave a sound instance plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceWave {
    /// The wavebank.
    pub wavebank: u32,
    /// The record index there.
    pub index: u32,
    /// The clip's native sample rate.
    pub clip_rate: u32,
}

/// A sound instance: its voice and its base values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// The mixer voice; `None` when the voice pool refused one, or when no wave was picked.
    pub voice: Option<VoiceId>,
    /// The wave; `None` when the entry or wave pick found nothing (the engine starts the instance
    /// finished).
    pub wave: Option<InstanceWave>,
    /// Base volume (replaced by an override block).
    pub volume: f32,
    /// Base pitch in semitones (replaced by an override block).
    pub pitch: f32,
    /// Output-channel multipliers (`+0x48`..`+0x5C`): 1.0 at start, multiplied by every update's block.
    pub channels: [f32; 6],
    /// Start delay, seconds.
    pub delay_s: f32,
    /// Time since start, seconds (`+0x6C`).
    pub elapsed_s: f32,
    /// The group's wave loop count (`+0x80`, from group `+0x2C`); the wave plays `1 + count` times.
    pub loop_count: u8,
    /// Whether it plays from its emitter's (3D) source: its group's `+0x14` byte is set and the cue
    /// has a position.
    pub positional: bool,
    /// The emitter (the cue it belongs to) whose source a positional instance mixes through.
    pub emitter: u32,
    /// Its wave carries the cue's kind-9 filter (`FUN_00839db0`).
    pub filtered: bool,
    /// State 2: it is dropped at its sound list's next update.
    pub finished: bool,
}

/// A sound list's state (`FUN_0083fee0`, `+0x08`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListState {
    /// 0: sounds may still fire.
    Firing,
    /// 1: everything fired and no instance was left.
    Fired,
    /// 2: done.
    Done,
}

/// A track's sounds: the next one that may fire and the live instances.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundList {
    /// The list state.
    pub state: ListState,
    /// The next sound index that may fire (`+0x18`).
    pub next: usize,
    /// Live instances, in start order.
    pub instances: Vec<Instance>,
}

impl SoundList {
    /// A list with nothing fired.
    pub fn new() -> SoundList {
        SoundList { state: ListState::Firing, next: 0, instances: Vec::new() }
    }

    /// `FUN_00840230`, at a loop: resume firing at the first sound whose start is at or after
    /// `loop_start` — except that a match on sound 0 does not stop the search, so when sound 1
    /// matches too the list resumes at sound 1 and sound 0 does not fire again. Always back to
    /// firing.
    pub fn rewind(&mut self, starts: &[f32], loop_start: f32) {
        for (b, &start) in starts.iter().enumerate() {
            if start >= loop_start {
                self.next = b;
                if b != 0 {
                    break;
                }
            }
        }
        self.state = ListState::Firing;
    }
}

impl Default for SoundList {
    fn default() -> Self {
        SoundList::new()
    }
}

/// One track of a playing multi-track cue.
#[derive(Clone, Debug)]
pub struct TrackPlayback {
    /// The run state.
    pub state: RunState,
    /// The loop count left (`+0xD5`; `0xFF` loops for ever).
    pub loop_count: u8,
    /// Track time.
    pub time: f32,
    /// The automation state.
    pub automation: AutomationState,
    /// The automation output of the latest step (`+0x24`..`+0x5C`).
    pub last: AutomationOutput,
    /// The sounds.
    pub sounds: SoundList,
    /// The child cue it started (`+0xCC`; `None` = −1).
    pub child: Option<CueHandle>,
}

impl TrackPlayback {
    /// A track at time 0 with its loop count.
    pub fn new(loop_count: u8) -> TrackPlayback {
        TrackPlayback {
            state: RunState::Playing,
            loop_count,
            time: 0.0,
            automation: AutomationState::default(),
            last: AutomationOutput::default(),
            sounds: SoundList::new(),
            child: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_start_draws_in_engine_order() {
        let params = InstanceParams::Multi {
            pitch_lo: -1.0,
            pitch_hi: 1.0,
            volume_lo: 0.5,
            volume_hi: 1.0,
            delay_a: 0.2,
            delay_b: 0.5,
        };
        let mut a = PalRng::new(42);
        let got = params.start(&mut a);
        let mut b = PalRng::new(42);
        let (rp, rv, rd) = (b.next_unit(), b.next_unit(), b.next_unit());
        assert_eq!(got.pitch, rp * 2.0 + -1.0);
        assert_eq!(got.volume, rv * 0.5 + 0.5);
        assert_eq!(got.delay_s, rd * 0.7 + 0.0, "lo = max(0.2 − 0.5, 0) = 0, hi = 0.7");
        assert_eq!(a, b);

        let mut c = PalRng::new(42);
        let single = InstanceParams::Single { volume: 0.63, pitch: 0.0 }.start(&mut c);
        assert_eq!((single.volume, single.pitch, single.delay_s), (0.63, 0.0, 0.0));
        assert_ne!(c, PalRng::new(42), "the single-wave delay still draws once");
    }

    #[test]
    fn sound_rewind_skips_sound_zero_when_sound_one_also_starts_after_the_loop_start() {
        let mut l = SoundList { state: ListState::Done, next: 3, instances: Vec::new() };
        l.rewind(&[0.0, 0.5, 1.0], 0.0);
        assert_eq!((l.state, l.next), (ListState::Firing, 1), "FUN_00840230 stops only on a match past sound 0");
        l.rewind(&[0.0], 0.0);
        assert_eq!(l.next, 0, "a single sound resumes at 0");
        l.rewind(&[0.0, 0.5, 1.0], 0.75);
        assert_eq!(l.next, 2);
        l.next = 3;
        l.rewind(&[0.0, 0.5, 1.0], 2.0);
        assert_eq!(l.next, 3, "unchanged when every sound starts before the loop start");
    }

    #[test]
    fn clamp01_matches_the_engine() {
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.25), 0.25);
        assert_eq!(clamp01(3.0), 1.0);
    }
}
