//! Multi-track automation, evaluated exactly as the track update `FUN_0083b4a0` evaluates it (read
//! from its disassembly in the unpacked PC exe; the helpers are `FUN_0083f7e0` for curves and
//! `FUN_0083f860` for oscillators).
//!
//! One [`AutomationState`] runs one record list — a track's automation table, or a multi-track cue's
//! event table — against that list's elapsed time. Each [`step`](AutomationState::step):
//!
//! 1. starts from volume 1.0 and pitch 0.0 (semitones);
//! 2. applies every **active** record, in the order they became active: a ramp (kinds 0/1) keeps
//!    applying its straight line for ever, extrapolating past both ends; an oscillator (kinds 2/3)
//!    applies, then leaves the active list once `time >= start + duration`; a curve (kinds 5/6)
//!    applies at the current parameter value. After a volume ramp the volume is clamped to `[0, 1]`;
//! 3. **activates** records, from the first not yet activated to the end of the list, each whose
//!    start is below the elapsed time (`time > start`); the next activation resumes after the last one
//!    activated, so an earlier record still waiting is skipped for good. An activated record applies
//!    at once (a volume curve then clamps volume to `[0, 1]`; a volume ramp does not).
//!
//! A ramp whose `mode` word is non-zero does not scale the running value: it sets an **override**
//! (volume or pitch) that replaces the sound instances' own base values for as long as it is active
//! (`FUN_0083c070` passes the override block to `FUN_0083fee0`). The override block starts at volume
//! 1.0 and pitch 0.0 and keeps its values between steps.
//!
//! Oscillators index the engine's 8192-entry sine table (`DAT_00CE8F08`) at
//! `trunc((time - start) × (1 / period) × (8192 / 2π) × 2π + 0.5) & 0x1FFF`, all single precision,
//! and yield `table × depth + 1.0` (or 1.0 when the low byte of `mode` is non-zero). A pitch oscillator
//! adds that value, baseline 1.0 included, to the pitch.
//!
//! Curves are piecewise linear over the parameter: `x ≤ x₀` yields `y₀`; otherwise the first segment
//! with `xᵢ < x ≤ xᵢ₊₁` interpolates. Past the last point the engine reads the word after the
//! record's last point — memory that is not part of the curve — so that is an error here.
//!
//! Every step starts the six **output-channel multipliers** at 1.0. A kind-4 record, when it
//! activates, sets them for that step only (`FUN_0083f8e0`, [`channel_multipliers`]); it never
//! joins the active list. A kind-7 record, when it activates, names the **child cue** the track (or,
//! in a cue's event table, the cue) starts when it finishes (`+0x7C`, kept until the state is
//! rebuilt). Kinds 9 and 10 evaluate the cue's kind-8 curves into the state's `+0x68` / `+0x6C` /
//! `+0x74`, which for the event table are cue `+0x84` / `+0x88` / `+0x90`; a wave whose cue has a
//! kind-9 event carries a biquad filter (`FUN_00839db0` creates it, `FUN_0083f2d0`) that takes its
//! parameters from the cue object at `+0x7C` (INFERRED: that object hands on those fields). Which
//! samples the filter runs over is decided in `MixWavesToOutput` (`0x00838860`), whose body is reached
//! only through a SecuROM-protected pointer. Reaching kind 8, 9 or 10 is therefore an
//! [`AutomationError::Unsupported`].
//!
//! A cue or track that loops rebuilds its state at the loop point ([`AutomationState::rewind`],
//! `FUN_0083bdb0`).

use std::sync::OnceLock;

use crate::multitrack::{Automation, CurveKind, Target};
use crate::select::PalRng;

/// Entries in the engine's sine table.
pub const SINE_TABLE_LEN: usize = 8192;

/// The engine's sine table `DAT_00CE8F08`: entry `i` is `sin(f32(2π) × i / 8192)` rounded to `f32`,
/// except entry 4096, which holds the single-precision `sin(π)` the table was built with. Checked
/// against the table in the unpacked exe by its FNV-1a-64 fingerprint (see the test).
pub fn sine_table() -> &'static [f32; SINE_TABLE_LEN] {
    static TABLE: OnceLock<[f32; SINE_TABLE_LEN]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let two_pi = std::f32::consts::TAU;
        let mut t = [0f32; SINE_TABLE_LEN];
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((two_pi * i as f32) as f64 / SINE_TABLE_LEN as f64).sin() as f32;
        }
        t[4096] = f32::from_bits(0xB3BB_BD4D);
        t
    })
}

/// Why automation could not be evaluated.
#[derive(Clone, Debug, PartialEq)]
pub enum AutomationError {
    /// A filter record (kind 8, reached through 9 or 10) was reached (see the module docs).
    Unsupported { kind: u32 },
    /// A curve's parameter has no value (a cue-local parameter nobody set).
    ParameterUnset { param: u32 },
    /// The parameter lies past the curve's last point, where the engine reads memory outside it.
    PastLastPoint { param: u32, value: f32 },
}

impl std::fmt::Display for AutomationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutomationError::Unsupported { kind } => write!(
                f,
                "automation kind {kind} drives the cue's biquad filter, whose input is chosen in \
                 MixWavesToOutput (0x00838860), reached only through a SecuROM-protected pointer; not traced"
            ),
            AutomationError::ParameterUnset { param } => {
                write!(f, "automation curve parameter 0x{param:08X} has no value")
            }
            AutomationError::PastLastPoint { param, value } => write!(
                f,
                "parameter 0x{param:08X} = {value} lies past the curve's last point (the engine reads outside it)"
            ),
        }
    }
}

impl std::error::Error for AutomationError {}

/// What one step produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutomationOutput {
    /// Volume multiplier.
    pub volume: f32,
    /// Pitch offset in semitones.
    pub pitch: f32,
    /// The six output-channel multipliers (1.0 unless a kind-4 record activated in this step).
    pub channels: [f32; 6],
    /// Whether a non-zero-mode ramp is active; the override values then replace the instances'
    /// base volume and pitch.
    pub override_active: bool,
    /// The override block's volume.
    pub override_volume: f32,
    /// The override block's pitch.
    pub override_pitch: f32,
}

impl Default for AutomationOutput {
    /// The values `FUN_0083b310` / `FUN_00834ad0` give the block before the first step.
    fn default() -> Self {
        AutomationOutput {
            volume: 1.0,
            pitch: 0.0,
            channels: [1.0; 6],
            override_active: false,
            override_volume: 1.0,
            override_pitch: 0.0,
        }
    }
}

/// One record list's evaluation state.
#[derive(Clone, Debug, PartialEq)]
pub struct AutomationState {
    /// The next record index that may activate.
    next: usize,
    /// Active record indices, in activation order.
    active: Vec<usize>,
    override_volume: f32,
    override_pitch: f32,
    /// The child cue a kind-7 record named (`+0x7C`; 0 = none).
    child_cue: u32,
}

impl Default for AutomationState {
    fn default() -> Self {
        AutomationState { next: 0, active: Vec::new(), override_volume: 1.0, override_pitch: 0.0, child_cue: 0 }
    }
}

/// The time field a record activates by (its `+0x04` word read as `f32`).
fn start_of(a: &Automation) -> f32 {
    match a {
        Automation::Ramp { start_s, .. } | Automation::Lfo { start_s, .. } => *start_s,
        Automation::Curve { unknown_04, .. } => f32::from_bits(*unknown_04),
        Automation::Kind4 { words } => f32::from_bits(words[0]),
        Automation::Kind7 { start_bits, .. } | Automation::Kind9 { start_bits, .. } => f32::from_bits(*start_bits),
    }
}

fn kind_code(a: &Automation) -> u32 {
    match a {
        Automation::Ramp { target: Target::Volume, .. } => 0,
        Automation::Ramp { target: Target::Pitch, .. } => 1,
        Automation::Lfo { target: Target::Volume, .. } => 2,
        Automation::Lfo { target: Target::Pitch, .. } => 3,
        Automation::Curve { kind: CurveKind::Volume, .. } => 5,
        Automation::Curve { kind: CurveKind::Pitch, .. } => 6,
        Automation::Curve { kind: CurveKind::Cue, .. } => 8,
        Automation::Kind4 { .. } => 4,
        Automation::Kind7 { .. } => 7,
        Automation::Kind9 { .. } => 9,
    }
}

/// `(to - from) / ((duration + start) - start) × (t - start)` in the engine's order.
fn ramp_slope_term(t: f32, start: f32, duration: f32, from: f32, to: f32) -> f32 {
    let span = (duration + start) - start;
    (to - from) / span * (t - start)
}

/// `cvttss2si`: truncation toward zero, `0x80000000` when out of range or NaN.
fn cvttss2si(v: f32) -> i32 {
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

/// `FUN_0083f860`: the oscillator value at time `t`.
fn oscillator(t: f32, start: f32, mode: u32, period: f32, depth: f32) -> f32 {
    if mode & 0xFF != 0 {
        return 1.0;
    }
    let scale = 8192.0f32 / std::f32::consts::TAU;
    let phase = (t - start) * (1.0f32 / period) * scale * std::f32::consts::TAU + 0.5;
    let idx = (cvttss2si(phase) & 0x1FFF) as usize;
    sine_table()[idx] * depth + 1.0
}

/// `FUN_0083f8e0`: a kind-4 record's six output-channel multipliers. Record channel `k` has a flag
/// byte at `+0x08 + k`, an offset at `+0x10 + 4k` and a base at `+0x30 + 4k`; the engine writes
/// record channels 0, 1, 3, 4, 2, 5 to output channels 0–5, in that order. A channel whose flag is 1
/// draws `r` and takes `((r − 0.5) × 2 + offset) + base`, otherwise `base`; each is clamped to
/// `[0, 1]`.
pub fn channel_multipliers(words: &[u32; 17], rng: &mut PalRng) -> [f32; 6] {
    const ORDER: [usize; 6] = [0, 1, 3, 4, 2, 5];
    let flag = |k: usize| ((words[1 + k / 4] >> (8 * (k % 4))) & 0xFF) as u8;
    let mut out = [0f32; 6];
    for (o, &k) in ORDER.iter().enumerate() {
        let base = f32::from_bits(words[11 + k]);
        let v = if flag(k) == 1 {
            let offset = f32::from_bits(words[3 + k]);
            (rng.next_unit() - 0.5) * 2.0 + offset + base
        } else {
            base
        };
        out[o] = v.clamp(0.0, 1.0);
    }
    out
}

/// `FUN_0083f7e0`: a curve at `x`.
pub(crate) fn curve(points: &[(f32, f32)], param: u32, x: f32) -> Result<f32, AutomationError> {
    let Some(&(x0, y0)) = points.first() else {
        return Err(AutomationError::PastLastPoint { param, value: x });
    };
    if x0 >= x {
        return Ok(y0);
    }
    let n = points.len() & 0xFF;
    for i in 0..n.saturating_sub(1) {
        let (xi, yi) = points[i];
        let (xj, yj) = points[i + 1];
        if x > xi && xj >= x {
            return Ok((yj - yi) / (xj - xi) * (x - xi) + yi);
        }
    }
    Err(AutomationError::PastLastPoint { param, value: x })
}

impl AutomationState {
    /// The child cue a kind-7 record named, if one has activated.
    pub fn child_cue(&self) -> Option<u32> {
        (self.child_cue != 0).then_some(self.child_cue)
    }

    /// `FUN_0083bdb0`, at a loop: empty the active list and resume activation at the first record
    /// whose start is at or after `loop_start` (unchanged when there is none).
    pub fn rewind(&mut self, records: &[Automation], loop_start: f32) {
        self.active.clear();
        if let Some(i) = records.iter().position(|r| start_of(r) >= loop_start) {
            self.next = i;
        }
    }

    /// Evaluate `records` at elapsed time `t`. `param` gives a curve parameter's value by hash; `rng`
    /// is the engine's generator (kind-4 jitter draws from it).
    pub fn step(
        &mut self,
        records: &[Automation],
        t: f32,
        param: &dyn Fn(u32) -> Result<f32, AutomationError>,
        rng: &mut PalRng,
    ) -> Result<AutomationOutput, AutomationError> {
        let mut vol = 1.0f32;
        let mut pitch = 0.0f32;
        let mut channels = [1.0f32; 6];
        let mut override_active = false;

        // Phase 1: the active records.
        let mut still_active = Vec::with_capacity(self.active.len());
        for &i in &self.active {
            let mut keep = true;
            match &records[i] {
                Automation::Ramp { target, start_s, mode, duration_s, from, to, .. } => {
                    let term = ramp_slope_term(t, *start_s, *duration_s, *from, *to);
                    match target {
                        Target::Volume => {
                            let v = term + from;
                            if *mode != 0 {
                                self.override_volume = v;
                                override_active = true;
                            } else {
                                vol *= v;
                            }
                            vol = vol.clamp(0.0, 1.0);
                        }
                        Target::Pitch => {
                            if *mode != 0 {
                                self.override_pitch = term + from;
                                override_active = true;
                            } else {
                                pitch = (term + pitch) + from;
                            }
                        }
                    }
                }
                Automation::Lfo { target, start_s, mode, duration_s, period_s, depth, .. } => {
                    let v = oscillator(t, *start_s, *mode, *period_s, *depth);
                    match target {
                        Target::Volume => vol *= v,
                        Target::Pitch => pitch += v,
                    }
                    if t >= duration_s + start_s {
                        keep = false;
                    }
                }
                Automation::Curve { kind, param: p, points, .. } => {
                    let v = curve(points, *p, param(*p)?)?;
                    match kind {
                        CurveKind::Volume => vol *= v,
                        CurveKind::Pitch => pitch += v,
                        CurveKind::Cue => return Err(AutomationError::Unsupported { kind: 8 }),
                    }
                }
                other => return Err(AutomationError::Unsupported { kind: kind_code(other) }),
            }
            if keep {
                still_active.push(i);
            }
        }
        self.active = still_active;

        // Phase 2: activation.
        for (i, rec) in records.iter().enumerate().skip(self.next) {
            if t.partial_cmp(&start_of(rec)) != Some(std::cmp::Ordering::Greater) {
                continue;
            }
            match rec {
                Automation::Ramp { target, start_s, mode, duration_s, from, to, .. } => {
                    let term = ramp_slope_term(t, *start_s, *duration_s, *from, *to);
                    match target {
                        Target::Volume => {
                            if *mode != 0 {
                                self.override_volume = term + from;
                                override_active = true;
                            } else {
                                vol *= term + from;
                            }
                        }
                        Target::Pitch => {
                            if *mode != 0 {
                                self.override_pitch = term + from;
                                override_active = true;
                            } else {
                                pitch = (term + pitch) + from;
                            }
                        }
                    }
                }
                Automation::Lfo { target, start_s, mode, period_s, depth, .. } => {
                    let v = oscillator(t, *start_s, *mode, *period_s, *depth);
                    match target {
                        Target::Volume => vol *= v,
                        Target::Pitch => pitch += v,
                    }
                }
                Automation::Curve { kind, param: p, points, .. } => {
                    let v = curve(points, *p, param(*p)?)?;
                    match kind {
                        CurveKind::Volume => {
                            vol *= v;
                            vol = vol.clamp(0.0, 1.0);
                        }
                        CurveKind::Pitch => pitch += v,
                        CurveKind::Cue => return Err(AutomationError::Unsupported { kind: 8 }),
                    }
                }
                Automation::Kind4 { words } => {
                    channels = channel_multipliers(words, rng);
                    self.next = i + 1;
                    continue;
                }
                Automation::Kind7 { cue, .. } => {
                    self.child_cue = *cue;
                    self.next = i + 1;
                    continue;
                }
                other => return Err(AutomationError::Unsupported { kind: kind_code(other) }),
            }
            self.active.push(i);
            self.next = i + 1;
        }

        Ok(AutomationOutput {
            volume: vol,
            pitch,
            channels,
            override_active,
            override_volume: self.override_volume,
            override_pitch: self.override_pitch,
        })
    }
}

/// The playback rate the engine gives a wave of `base_rate` Hz at `pitch` semitones: the pitch is
/// scaled to `trunc(pitch × (1/24) × 8192)` (`FUN_00836c70` → wave `SetPitch`), kept to its low 16
/// bits and clamped to ±8192, then `FUN_0083df00` computes `trunc(2^(p / 4096) × base_rate)` (×0.25 and
/// ×4 at the clamps).
pub fn pitched_rate(base_rate: u32, pitch: f32) -> u32 {
    let raw = cvttss2si(pitch * (1.0f32 / 24.0) * 8192.0) as i16;
    let p = raw.clamp(-0x2000, 0x2000);
    let factor = if p <= -0x2000 {
        0.25
    } else if p >= 0x2000 {
        4.0
    } else {
        2f64.powf(f64::from(p) * (1.0 / 4096.0))
    };
    (factor * f64::from(base_rate)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv64(bytes: impl Iterator<Item = u8>) -> u64 {
        bytes.fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3))
    }

    /// The fingerprint of the 32 KiB table at `0x00CE8F08` in the unpacked exe.
    #[test]
    fn sine_table_matches_the_exe() {
        let bytes = sine_table().iter().flat_map(|v| v.to_le_bytes());
        assert_eq!(fnv64(bytes), 0x6342_1FAD_6B05_0CCA);
    }

    fn ramp(target: Target, start: f32, dur: f32, from: f32, to: f32, mode: u32) -> Automation {
        Automation::Ramp { target, start_s: start, mode, unknown_0c: 0, duration_s: dur, from, to }
    }
    fn no_params(p: u32) -> Result<f32, AutomationError> {
        Err(AutomationError::ParameterUnset { param: p })
    }
    fn rng() -> PalRng {
        PalRng::new(7)
    }

    #[test]
    fn a_fade_out_ramp_activates_after_its_start_clamps_and_extrapolates() {
        let recs = [ramp(Target::Volume, 1.0, 2.0, 1.0, 0.0, 0)];
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.5, &no_params, &mut rng()).unwrap().volume, 1.0, "not yet active");
        assert_eq!(st.step(&recs, 2.0, &no_params, &mut rng()).unwrap().volume, 0.5, "activates at t > start");
        assert_eq!(st.step(&recs, 5.0, &no_params, &mut rng()).unwrap().volume, 0.0, "past the end: clamped at 0");
        let mut st = AutomationState::default();
        st.step(&recs, 1.5, &no_params, &mut rng()).unwrap();
        assert_eq!(st.step(&recs, 0.0, &no_params, &mut rng()).unwrap().volume, 1.0, "before the start: clamped at 1");
    }

    #[test]
    fn pitch_ramps_add_and_overrides_persist() {
        let recs = [ramp(Target::Pitch, 0.0, 1.0, 0.0, 12.0, 0), ramp(Target::Pitch, 0.0, 1.0, -2.0, -2.0, 1)];
        let mut st = AutomationState::default();
        let out = st.step(&recs, 0.5, &no_params, &mut rng()).unwrap();
        assert_eq!(out.pitch, 6.0);
        assert!(out.override_active);
        assert_eq!(out.override_pitch, -2.0);
        assert_eq!(out.override_volume, 1.0, "the override block's volume stays at its initial 1.0");
    }

    #[test]
    fn oscillators_expire_and_carry_their_baseline() {
        let lfo = Automation::Lfo {
            target: Target::Pitch,
            start_s: 0.0,
            mode: 0,
            unknown_0c: 0,
            duration_s: 1.0,
            period_s: 1.0,
            depth: 2.0,
        };
        let recs = [lfo];
        let mut st = AutomationState::default();
        // phase index = trunc(0.25 × 8192 + 0.5) = 2048 → sin = 1.0 → 1 + 2 = 3 semitones.
        assert_eq!(st.step(&recs, 0.25, &no_params, &mut rng()).unwrap().pitch, 3.0);
        assert_eq!(st.step(&recs, 1.0, &no_params, &mut rng()).unwrap().pitch, 1.0, "applies on its last step");
        assert_eq!(st.step(&recs, 1.5, &no_params, &mut rng()).unwrap().pitch, 0.0, "then leaves");
    }

    #[test]
    fn curves_interpolate_and_refuse_past_the_last_point() {
        let c = Automation::Curve { kind: CurveKind::Volume, unknown_04: 0, param: 7, points: vec![(0.0, 0.0), (1.0, 0.5)] };
        let recs = [c];
        let mut st = AutomationState::default();
        let half = |_: u32| Ok(0.5f32);
        assert_eq!(st.step(&recs, 0.1, &half, &mut rng()).unwrap().volume, 0.25);
        let beyond = |_: u32| Ok(2.0f32);
        assert_eq!(
            st.step(&recs, 0.2, &beyond, &mut rng()),
            Err(AutomationError::PastLastPoint { param: 7, value: 2.0 })
        );
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.1, &no_params, &mut rng()), Err(AutomationError::ParameterUnset { param: 7 }));
    }

    #[test]
    fn filter_curves_stop_playback_when_reached() {
        let recs = [Automation::Kind9 { start_bits: 0, curve_a: 0, curve_b: u32::MAX }];
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.1, &no_params, &mut rng()), Err(AutomationError::Unsupported { kind: 9 }));
    }

    /// Retail `ambience` cue 0 (guid 0x0CA03B08), track 1: every record channel jittered.
    const RETAIL_KIND4: [u32; 17] = [
        0x4028_0000, 0x0101_0101, 0x0000_0001, 0x3D81_3855, 0x3D81_3855, 0x3D81_3855, 0x3D81_3855,
        0x3D81_3855, 0x0000_0000, 0x0000_0000, 0x4102_673C, 0x3F80_0000, 0x3F80_0000, 0x3F80_0000,
        0x3F80_0000, 0x3F80_0000, 0x3F80_0000,
    ];

    #[test]
    fn kind4_sets_the_channels_for_its_activation_step_only() {
        // Record channels 0..4 jittered (flags 1), channel 5 not: out[5] is its base.
        let mut words = RETAIL_KIND4;
        words[16] = 0.25f32.to_bits(); // record channel 5's base
        words[13] = 0.5f32.to_bits(); // record channel 2's base, written to output channel 4
        let recs = [Automation::Kind4 { words }];
        let mut st = AutomationState::default();
        let mut r = rng();
        let out = st.step(&recs, 3.0, &no_params, &mut r).unwrap();
        let mut expect_rng = rng();
        let off = f32::from_bits(0x3D81_3855);
        let jit = |r: &mut PalRng, base: f32| ((r.next_unit() - 0.5) * 2.0 + off + base).clamp(0.0, 1.0);
        let c0 = jit(&mut expect_rng, 1.0);
        let c1 = jit(&mut expect_rng, 1.0);
        let c3 = jit(&mut expect_rng, 1.0);
        let c4 = jit(&mut expect_rng, 1.0);
        let c2 = jit(&mut expect_rng, 0.5);
        assert_eq!(out.channels, [c0, c1, c3, c4, c2, 0.25], "record channels 0, 1, 3, 4, 2, 5");
        assert_eq!(r, expect_rng, "one draw per flagged channel, nothing else");
        assert_eq!(st.step(&recs, 3.1, &no_params, &mut r).unwrap().channels, [1.0; 6], "one step only");
    }

    #[test]
    fn kind7_names_the_child_and_rewind_resumes_activation() {
        let recs = [
            ramp(Target::Volume, 0.0, 1.0, 1.0, 0.0, 0),
            Automation::Kind7 { start_bits: 0.5f32.to_bits(), cue: 0xC0FFEE },
            ramp(Target::Volume, 2.0, 1.0, 1.0, 0.0, 0),
        ];
        let mut st = AutomationState::default();
        assert_eq!(st.child_cue(), None);
        st.step(&recs, 0.6, &no_params, &mut rng()).unwrap();
        assert_eq!(st.child_cue(), Some(0xC0FFEE));
        st.rewind(&recs, 0.25);
        assert_eq!(st.next, 1, "the first record starting at or after 0.25");
        assert!(st.active.is_empty());
        st.rewind(&recs, 5.0);
        assert_eq!(st.next, 1, "unchanged when every record starts below the loop start");
        assert_eq!(st.child_cue(), Some(0xC0FFEE), "the child survives a rewind");
    }

    #[test]
    fn pitch_to_rate() {
        assert_eq!(pitched_rate(44100, 0.0), 44100);
        assert_eq!(pitched_rate(44100, 12.0), 88200);
        assert_eq!(pitched_rate(44100, -24.0), 11025);
        assert_eq!(pitched_rate(44100, 40.0), 176400, "clamped at two octaves");
    }
}
