//! A started cue, advanced frame by frame the way the engine advances a cue instance: the cue-level
//! update `FUN_00835060`, the track update `FUN_0083c070`, sound firing `FUN_0083fee0`, and sound
//! instance start `FUN_008369e0`. [`crate::AudioEngine`] owns the playbacks and the voices they fire;
//! this module holds their state and the engine's arithmetic.
//!
//! **Per frame, for a multi-track cue** (all single precision):
//!
//! 1. The cue volume `clamp01(cue gain × cue-automation volume)` and pitch are taken from the
//!    **previous** frame's cue automation (`FUN_00835060` reads `+0x3C`/`+0x40` before updating
//!    them); both start at 1.0 and 0.0.
//! 2. The cue time advances by `dt` and the cue's event table is evaluated ([`crate::automation`]).
//! 3. Each track, in order: its time advances by `dt`; its automation is evaluated; its parameters
//!    are `track volume × cue volume` and `track pitch + cue pitch`; every sound from the next unfired
//!    one on whose start time is `<= track time` fires (in order; the next unfired index resumes after
//!    the last one fired); then every live sound instance of the track takes the parameters.
//! 4. A non-zero-mode ramp's override block — the cue's if active, else the track's — replaces the
//!    base volume and pitch of the track's instances, new and live.
//!
//! **A sound instance** takes `base volume × volume parameter` as its volume and
//! `base pitch + pitch parameter` semitones as its pitch, played at
//! [`crate::automation::pitched_rate`]. Its base values and start delay are drawn at start, after its
//! wave is picked: base pitch `FUN_0083d700`, base volume `FUN_0083d770`, start delay `FUN_0083d7e0`
//! ([`InstanceParams::start`]).
//!
//! **A single-track cue** starts its one instance on its first frame, with volume parameter
//! `clamp01(cue gain)` and pitch parameter 0.
//!
//! **A multi-track cue** draws once at start (`FUN_008354e0`): it plays only if its `+0x18` value is
//! not below the draw.

use crate::automation::AutomationState;
use crate::select::PalRng;
use crate::voice::VoiceId;

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

/// A live sound instance: its voice and its base values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// The mixer voice.
    pub voice: VoiceId,
    /// Base volume (replaced by an active override).
    pub volume: f32,
    /// Base pitch in semitones (replaced by an active override).
    pub pitch: f32,
    /// The clip's native sample rate.
    pub clip_rate: u32,
    /// The wavebank of the wave it plays.
    pub wavebank: u32,
    /// The wave's record index there.
    pub wave_index: u32,
}

/// One track of a playing multi-track cue.
#[derive(Clone, Debug)]
pub struct TrackPlayback {
    /// Track time.
    pub time: f32,
    /// The automation state.
    pub automation: AutomationState,
    /// The next sound index that may fire.
    pub next_sound: usize,
    /// Live instances.
    pub instances: Vec<Instance>,
}

impl TrackPlayback {
    /// A track at time 0.
    pub fn new() -> TrackPlayback {
        TrackPlayback { time: 0.0, automation: AutomationState::default(), next_sound: 0, instances: Vec::new() }
    }
}

impl Default for TrackPlayback {
    fn default() -> Self {
        TrackPlayback::new()
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
    fn clamp01_matches_the_engine() {
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.25), 0.25);
        assert_eq!(clamp01(3.0), 1.0);
    }
}
