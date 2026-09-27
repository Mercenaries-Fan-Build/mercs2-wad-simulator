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
//! Kinds 4, 7, 9 and 10 do not act on volume or pitch: kind 4 sets six clamped, optionally jittered
//! multipliers for the instance parameter channels 2–7 (`FUN_0083f8e0`), kind 7 names a child cue the
//! track starts when it finishes (`+0x7C` → `FUN_0082e930` in `FUN_0083c070`), and kinds 9/10
//! evaluate cue curves (kind 8) into track fields `+0x68`/`+0x6C`/`+0x74` whose consumers are not
//! established. This mixer has none of those channels or child cues, so reaching one while playing is
//! an [`AutomationError::Unsupported`].

use std::sync::OnceLock;

use crate::multitrack::{Automation, CurveKind, Target};

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
    /// A record of a kind that does not act on volume or pitch was reached (see the module docs).
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
                "automation kind {kind} does not act on volume or pitch and this mixer has no counterpart"
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
    /// Whether a non-zero-mode ramp is active; the override values then replace the instances'
    /// base volume and pitch.
    pub override_active: bool,
    /// The override block's volume.
    pub override_volume: f32,
    /// The override block's pitch.
    pub override_pitch: f32,
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
}

impl Default for AutomationState {
    fn default() -> Self {
        AutomationState { next: 0, active: Vec::new(), override_volume: 1.0, override_pitch: 0.0 }
    }
}

/// The time field a record activates by (its `+0x04` word read as `f32`).
fn start_of(a: &Automation) -> f32 {
    match a {
        Automation::Ramp { start_s, .. } | Automation::Lfo { start_s, .. } => *start_s,
        Automation::Curve { unknown_04, .. } => f32::from_bits(*unknown_04),
        Automation::Kind4 { words } => f32::from_bits(words[0]),
        Automation::Kind7 { unknown_04, .. } | Automation::Kind9 { unknown_04, .. } => f32::from_bits(*unknown_04),
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
    /// Evaluate `records` at elapsed time `t`. `param` gives a curve parameter's value by hash.
    pub fn step(
        &mut self,
        records: &[Automation],
        t: f32,
        param: &dyn Fn(u32) -> Result<f32, AutomationError>,
    ) -> Result<AutomationOutput, AutomationError> {
        let mut vol = 1.0f32;
        let mut pitch = 0.0f32;
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
                other => return Err(AutomationError::Unsupported { kind: kind_code(other) }),
            }
            self.active.push(i);
            self.next = i + 1;
        }

        Ok(AutomationOutput {
            volume: vol,
            pitch,
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

    #[test]
    fn a_fade_out_ramp_activates_after_its_start_clamps_and_extrapolates() {
        let recs = [ramp(Target::Volume, 1.0, 2.0, 1.0, 0.0, 0)];
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.5, &no_params).unwrap().volume, 1.0, "not yet active");
        assert_eq!(st.step(&recs, 2.0, &no_params).unwrap().volume, 0.5, "activates at t > start");
        assert_eq!(st.step(&recs, 5.0, &no_params).unwrap().volume, 0.0, "past the end: clamped at 0");
        let mut st = AutomationState::default();
        st.step(&recs, 1.5, &no_params).unwrap();
        assert_eq!(st.step(&recs, 0.0, &no_params).unwrap().volume, 1.0, "before the start: clamped at 1");
    }

    #[test]
    fn pitch_ramps_add_and_overrides_persist() {
        let recs = [ramp(Target::Pitch, 0.0, 1.0, 0.0, 12.0, 0), ramp(Target::Pitch, 0.0, 1.0, -2.0, -2.0, 1)];
        let mut st = AutomationState::default();
        let out = st.step(&recs, 0.5, &no_params).unwrap();
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
        assert_eq!(st.step(&recs, 0.25, &no_params).unwrap().pitch, 3.0);
        assert_eq!(st.step(&recs, 1.0, &no_params).unwrap().pitch, 1.0, "applies on its last step");
        assert_eq!(st.step(&recs, 1.5, &no_params).unwrap().pitch, 0.0, "then leaves");
    }

    #[test]
    fn curves_interpolate_and_refuse_past_the_last_point() {
        let c = Automation::Curve { kind: CurveKind::Volume, unknown_04: 0, param: 7, points: vec![(0.0, 0.0), (1.0, 0.5)] };
        let recs = [c];
        let mut st = AutomationState::default();
        let half = |_: u32| Ok(0.5f32);
        assert_eq!(st.step(&recs, 0.1, &half).unwrap().volume, 0.25);
        let beyond = |_: u32| Ok(2.0f32);
        assert_eq!(
            st.step(&recs, 0.2, &beyond),
            Err(AutomationError::PastLastPoint { param: 7, value: 2.0 })
        );
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.1, &no_params), Err(AutomationError::ParameterUnset { param: 7 }));
    }

    #[test]
    fn unsupported_kinds_stop_playback_when_reached() {
        let recs = [Automation::Kind7 { unknown_04: 0, hash: 1 }];
        let mut st = AutomationState::default();
        assert_eq!(st.step(&recs, 0.1, &no_params), Err(AutomationError::Unsupported { kind: 7 }));
    }

    #[test]
    fn pitch_to_rate() {
        assert_eq!(pitched_rate(44100, 0.0), 44100);
        assert_eq!(pitched_rate(44100, 12.0), 88200);
        assert_eq!(pitched_rate(44100, -24.0), 11025);
        assert_eq!(pitched_rate(44100, 40.0), 176400, "clamped at two octaves");
    }
}
