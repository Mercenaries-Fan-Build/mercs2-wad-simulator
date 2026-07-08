//! The software mixer — the `FUN_00831ee0` / `FUN_00836610` analog.
//!
//! **Oracle (audio_code_map.md §2, §3.4):** the PC build has no hardware voices; a dedicated **mixer
//! thread** `FUN_00831ee0` runs on a **45 ms cadence** (`Sleep(0x2d)`), taking the engine CS and
//! calling `FUN_00836610` **MixSources**: PrepareMix zeroes a **`0x30000`-byte int32 accumulator**,
//! each source mixes its waves in (`MixWavesToOutput`), then MixWave/Commit (`FUN_0083cbf0`)
//! **`packssdw`-saturates the int32 accumulator to int16** and hands it to the DirectSound buffer.
//! Per-wave gain/pan/pitch is `PalSoundWaveDX8` (`FUN_00839ae0` kernels, `FUN_0083ade0` volume calc).
//!
//! This module reproduces that math **headlessly**: [`Mixer::mix`] renders any number of frames into
//! an interleaved int16 buffer using an int32 accumulator and saturating clamp — with no device at
//! all. A device sink ([`crate::backend`]) is an optional consumer of those buffers, not a
//! requirement. The 45 ms tick is [`Mixer::frames_per_tick`].

use std::collections::HashMap;

use crate::voice::{InstanceState, VoiceId, VoicePool};

/// Mixer thread cadence in milliseconds (`Sleep(0x2d)` = 45, audio_code_map.md §2).
pub const MIXER_TICK_MS: u32 = 45;

/// A source of int16 PCM samples for one voice (a decoded wave, a stream chunk, or a test tone).
///
/// `fill` writes up to `out.len()` interleaved samples and returns the number of **frames** written;
/// a short write (fewer frames than `out.len()/channels`) signals the source is exhausted.
pub trait SampleSource: Send {
    /// Fill `out` (interleaved, `channels`-wide) and return frames produced.
    fn fill(&mut self, out: &mut [i16], channels: usize) -> usize;
    /// True once the source has no more samples (and is not looping internally).
    fn is_finished(&self) -> bool;
    /// Rewind to the start (used when a looping voice wraps).
    fn reset(&mut self);
}

/// A resident PCM wave (decoded from a wave bank). Interleaved int16, `channels`-wide.
///
/// The clip's native sample rate rarely matches the mixer's, so [`fill`](SampleSource::fill) **resamples
/// on the fly** — a fractional source cursor advances by `src_rate / dst_rate` per output frame with
/// linear interpolation. This is the engine's per-voice pitch step (`PalSoundWaveDX8`, `FUN_00839ae0`):
/// a 22 050 Hz clip played into a 44 100 Hz mix advances the cursor at 0.5 frames/output-frame, so it
/// plays at the correct pitch and duration rather than an octave high.
#[derive(Clone, Debug)]
pub struct PcmSource {
    data: Vec<i16>,
    channels: usize,
    /// Fractional source-frame cursor (resampling position).
    pos: f64,
    /// Source frames advanced per output frame (`src_rate / dst_rate`).
    step: f64,
}

impl PcmSource {
    /// Build from interleaved int16 samples that are **already at the mixer rate** (no resampling).
    pub fn new(data: Vec<i16>, channels: usize) -> PcmSource {
        PcmSource {
            data,
            channels: channels.max(1),
            pos: 0.0,
            step: 1.0,
        }
    }

    /// Build a clip that plays at its native `src_rate` into a `dst_rate` mixer, resampling per frame.
    pub fn with_rate(data: Vec<i16>, channels: usize, src_rate: u32, dst_rate: u32) -> PcmSource {
        let step = if dst_rate == 0 {
            1.0
        } else {
            (src_rate.max(1) as f64) / (dst_rate as f64)
        };
        PcmSource {
            data,
            channels: channels.max(1),
            pos: 0.0,
            step,
        }
    }

    fn frames(&self) -> usize {
        self.data.len() / self.channels
    }

    /// Linear-interpolate source channel `ch` at fractional frame position `pos`.
    #[inline]
    fn sample_at(&self, pos: f64, ch: usize) -> i16 {
        let frames = self.frames();
        if frames == 0 {
            return 0;
        }
        let base = pos.floor() as usize;
        let frac = pos - base as f64;
        let c = ch.min(self.channels - 1);
        let s0 = self.data[base * self.channels + c] as f64;
        let s1 = if base + 1 < frames {
            self.data[(base + 1) * self.channels + c] as f64
        } else {
            s0
        };
        (s0 + (s1 - s0) * frac).round() as i16
    }
}

impl SampleSource for PcmSource {
    fn fill(&mut self, out: &mut [i16], channels: usize) -> usize {
        let want = out.len() / channels;
        let frames = self.frames();
        let mut written = 0;
        while written < want && (self.pos as usize) < frames {
            for ch in 0..channels {
                // mono→stereo duplicates; stereo→stereo passes through — with linear resampling.
                out[written * channels + ch] = self.sample_at(self.pos, ch);
            }
            self.pos += self.step;
            written += 1;
        }
        written
    }
    fn is_finished(&self) -> bool {
        (self.pos as usize) >= self.frames()
    }
    fn reset(&mut self) {
        self.pos = 0.0;
    }
}

/// A sine test tone — a deterministic source for tests (its RMS is measurable, so gain/attenuation is
/// observable end-to-end through the mixer).
#[derive(Clone, Debug)]
pub struct ToneSource {
    freq: f32,
    sample_rate: f32,
    amplitude: i16,
    phase: f32,
    remaining: usize, // frames left
}

impl ToneSource {
    /// A `freq`-Hz tone at `amplitude` for `frames` frames.
    pub fn new(freq: f32, sample_rate: u32, amplitude: i16, frames: usize) -> ToneSource {
        ToneSource {
            freq,
            sample_rate: sample_rate as f32,
            amplitude,
            phase: 0.0,
            remaining: frames,
        }
    }
}

impl SampleSource for ToneSource {
    fn fill(&mut self, out: &mut [i16], channels: usize) -> usize {
        let want = (out.len() / channels).min(self.remaining);
        let step = std::f32::consts::TAU * self.freq / self.sample_rate;
        for f in 0..want {
            let s = (self.phase.sin() * self.amplitude as f32) as i16;
            for ch in 0..channels {
                out[f * channels + ch] = s;
            }
            self.phase = (self.phase + step) % std::f32::consts::TAU;
        }
        self.remaining -= want;
        want
    }
    fn is_finished(&self) -> bool {
        self.remaining == 0
    }
    fn reset(&mut self) {
        self.phase = 0.0;
    }
}

/// Per-voice mixing parameters the mixer needs beyond the FSM state in [`VoicePool`].
struct MixVoice {
    source: Box<dyn SampleSource>,
    /// Left/right channel gains from [`crate::spatial`] (constant-power pan × distance attenuation).
    left_gain: f32,
    right_gain: f32,
}

/// Mixer configuration.
#[derive(Clone, Copy, Debug)]
pub struct MixerConfig {
    /// Output sample rate. 44100 with EAX/enabled, else 22050 (`GetOutputSampleRate` `FUN_008305d0`).
    pub sample_rate: u32,
    /// Output channels (1/2/4/6 supported by the exe; 2 here for the stereo substitute).
    pub channels: usize,
}

impl Default for MixerConfig {
    fn default() -> Self {
        MixerConfig {
            sample_rate: 44100,
            channels: 2,
        }
    }
}

/// The software mixer: owns per-voice sources and renders them into int16 buffers.
pub struct Mixer {
    cfg: MixerConfig,
    voices: HashMap<VoiceId, MixVoice>,
    /// int32 mix accumulator, reused across `mix` calls (`PrepareMix` zeroes it; §3.4).
    accum: Vec<i32>,
    /// Scratch per-source fill buffer.
    scratch: Vec<i16>,
}

impl Mixer {
    /// A mixer with the given config.
    pub fn new(cfg: MixerConfig) -> Mixer {
        Mixer {
            cfg,
            voices: HashMap::new(),
            accum: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Output config.
    pub fn config(&self) -> MixerConfig {
        self.cfg
    }

    /// Frames rendered per 45 ms mixer tick at the current sample rate.
    pub fn frames_per_tick(&self) -> usize {
        (self.cfg.sample_rate as u64 * MIXER_TICK_MS as u64 / 1000) as usize
    }

    /// Attach a sample source to a voice (called when a cue is started).
    pub fn attach(&mut self, id: VoiceId, source: Box<dyn SampleSource>) {
        self.voices.insert(
            id,
            MixVoice {
                source,
                left_gain: 1.0,
                right_gain: 1.0,
            },
        );
    }

    /// Detach a voice's source (on stop/steal/finish).
    pub fn detach(&mut self, id: VoiceId) {
        self.voices.remove(&id);
    }

    /// Set a voice's spatial channel gains (left/right), from [`crate::spatial`].
    pub fn set_channel_gains(&mut self, id: VoiceId, left: f32, right: f32) {
        if let Some(v) = self.voices.get_mut(&id) {
            v.left_gain = left;
            v.right_gain = right;
        }
    }

    /// Number of voices with a live source.
    pub fn active_sources(&self) -> usize {
        self.voices.len()
    }

    /// **MixSources** (`FUN_00836610`): render `frames` frames of every audible voice into `out`
    /// (interleaved, `channels`-wide). Uses an int32 accumulator then saturates to int16 — exactly
    /// the exe's PrepareMix→MixWave→Commit pipeline (§3.4). Per-voice gain is
    /// `base_gain × fade × category_gain(cat) × channel_gain`. Voices whose source runs dry are
    /// finished via [`VoicePool::mark_finished`] (looping voices are rewound). Runs with **no device**.
    ///
    /// `category_gain(category_id) -> f32` supplies the [`crate::categories::Categories`] contribution;
    /// pass `|_| 1.0` for a flat mix.
    pub fn mix(
        &mut self,
        pool: &mut VoicePool,
        out: &mut [i16],
        category_gain: impl Fn(u32) -> f32,
    ) {
        let ch = self.cfg.channels;
        let frames = out.len() / ch;
        let n = frames * ch;

        self.accum.clear();
        self.accum.resize(n, 0); // PrepareMix: zero the int32 accumulator
        if self.scratch.len() < n {
            self.scratch.resize(n, 0);
        }

        // Snapshot the FSM state of voices we mix, so we can react after the immutable-borrow pass.
        let mut finished: Vec<VoiceId> = Vec::new();

        for (id, mv) in self.voices.iter_mut() {
            let voice = match pool.get(*id) {
                Some(v) if v.state.is_audible() => v,
                _ => continue, // not playing (start-delay, paused, stopping, …) — contributes silence
            };
            let cat_gain = category_gain(voice.category as u32);
            let base = (voice.gain * voice.fade * cat_gain).clamp(0.0, 4.0);
            if base <= 0.0 {
                continue;
            }

            let scratch = &mut self.scratch[..n];
            for s in scratch.iter_mut() {
                *s = 0;
            }
            let mut produced = mv.source.fill(scratch, ch);

            // Loop wrap or finish signalling.
            if produced < frames {
                if voice.looping {
                    mv.source.reset();
                    // top up the remainder of the buffer from the start of the wave
                    let rem = &mut self.scratch[produced * ch..n];
                    let more = mv.source.fill(rem, ch);
                    produced += more;
                } else if mv.source.is_finished() {
                    finished.push(*id);
                }
            }

            // Accumulate (int32) with per-channel gains.
            for f in 0..produced {
                for c in 0..ch {
                    let g = if c == 0 { mv.left_gain } else { mv.right_gain };
                    let sample = self.scratch[f * ch + c] as f32 * base * g;
                    self.accum[f * ch + c] += sample as i32;
                }
            }
        }

        // Commit (packssdw saturate int32 → int16).
        for (o, a) in out.iter_mut().zip(self.accum.iter()) {
            *o = (*a).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }

        for id in finished {
            pool.mark_finished(id);
            // If it did not loop, its source is spent; drop it once the voice leaves audible states.
            if pool.get(id).map(|v| v.state != InstanceState::Looping).unwrap_or(true) {
                self.voices.remove(&id);
            }
        }
    }
}

/// RMS of an interleaved int16 buffer — a small helper for tests/telemetry (measures how loud a mix
/// came out, so attenuation is observable end-to-end).
pub fn rms_i16(buf: &[i16]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}
