//! The software mixer — the engine's source mix path, read from the disassembly of the unpacked PC
//! exe (the SecuROM splices in it resolved by emulating the runtime memory dump; see the audio code
//! map §10).
//!
//! **The mix pass** (`MixSources`, `FUN_00836610`, on the 45 ms mixer thread `FUN_00831ee0`):
//! `PrepareMix` (`FUN_0083c9c0`) zeroes the int32 accumulator; every **source** mixes its waves in
//! (`MixWavesToOutput`, `FUN_00838850`) — first the per-emitter (3D) sources in creation order, then
//! the shared 2D source for waves of up to two channels (engine `+0x1AC`, `FUN_0082f110`), then the
//! one for wider waves (`+0x1A8`, `FUN_0082f140`); `FUN_0083cbf0` saturates the accumulator to
//! int16 (`packssdw`) for the stream buffer. That buffer is always **6-channel** 16-bit PCM,
//! `WAVE_FORMAT_EXTENSIBLE` with channel mask `0x3F` — front left, front right, centre, LFE, back
//! left, back right (`FUN_0083f760`); DirectSound folds it to the speakers.
//!
//! **A source** (its mix object, vtable `0x00BE23CC`): `FUN_0083ade0` zeroes the source scratch
//! `DAT_00FC34B0` (six int32 per frame) and sets the source's six channel gains (1.0 for a 2D
//! source); for each of its waves, in the order they were added (`FUN_00838710`), `FUN_0083b120`
//! runs the wave mix (`FUN_00839ae0`) into the scratch; then `FUN_0083afc0` commits the scratch:
//! `acc = trunc((f32) scratch × gain + (f32) acc)` per sample and channel.
//!
//! **A wave** (`FUN_00839ae0`): its mix volumes (`FUN_0083e1d0`) are `master = clamp(d, 0, 2)` and
//! `gain[c] = clamp(ch[c] × d, 0, 2)` for the six outputs, `d` the volume the instance set (clamped
//! to `[0, 1]` there, `0x008373AA`) and `ch` the instance's output-channel multipliers (wave vtable
//! `+0x10C`); the kernels take `trunc(g × 32768)`. A wave whose master is `≤ 19 / 32768` only
//! advances (`FUN_0083a440`: position += step × frames, then any loop wraps). Otherwise the kernel for
//! its channel count (`FUN_00839fd0` mono, `FUN_0083a200` stereo) steps a 32.32 fixed-point read
//! position by `step = (freq << 32) / rate` (integer division) and, for each output frame, reads the
//! sample at the integer position — no interpolation — and adds `(sample × g) >> 15` to each of the
//! six scratch channels (`FUN_0083e970`; stereo `FUN_0083eb00` feeds left to outputs 0, 2, 4 and
//! right to 1, 3, 5, with its own shortcuts). At the end of the data `FUN_00839e90` wraps or stops
//! the wave ([`PcmSource::with_loops`]). After the kernel, a wave that carries a filter
//! ([`crate::filter::Biquad`]) runs it in place over the source scratch — `frames × 6` samples, every
//! wave of the source mixed so far this pass.
//!
//! **Output channels.** The engine's output is those six channels; the device format is Windows'
//! business (DirectSound folds 5.1 to the speakers). This mixer hands the six channels to a device
//! with six or more, and **front left / front right** to a stereo device (front left to a mono one) —
//! a substitute for DirectSound's fold-down, which is not engine code. Three to five output channels
//! are refused.
//!
//! **3D sources are not traced here yet:** the engine's per-emitter source gains come from
//! `FUN_0083d090` (five speaker directions against the listener) with the distance volume of
//! `FUN_0083d3a0`; this mixer gives an emitter source the left/right gains [`crate::spatial`]
//! computes (outputs 0 and 1, the other four 0), as before (`DEFERRED.md`).

use std::collections::HashMap;

use crate::filter::Biquad;
use crate::voice::{VoiceId, VoicePool};

/// Mixer thread cadence in milliseconds (`Sleep(0x2d)` = 45, audio_code_map.md §2).
pub const MIXER_TICK_MS: u32 = 45;

/// Channels of the engine's mix (`FUN_0083f760`: 5.1).
pub const ENGINE_CHANNELS: usize = 6;

/// A source of int16 PCM samples at the mixer rate for one voice (a test tone, synthesized audio).
///
/// `fill` writes up to `out.len()` interleaved samples and returns the number of **frames** written;
/// a short write (fewer frames than `out.len()/channels`) signals the source is exhausted.
pub trait SampleSource: Send {
    /// Fill `out` (interleaved, `channels`-wide) and return frames produced.
    fn fill(&mut self, out: &mut [i16], channels: usize) -> usize;
    /// True once the source has no more samples.
    fn is_finished(&self) -> bool;
    /// Rewind to the start.
    fn reset(&mut self);
}

/// `cvttss2si`: truncation toward zero, `i32::MIN` when out of range or NaN.
fn trunc_i32(v: f32) -> i32 {
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

/// A resident PCM16 wave, 1 or 2 channels, played as the engine's wave object plays it.
#[derive(Clone, Debug)]
pub struct PcmSource {
    data: Vec<i16>,
    channels: usize,
    /// 32.32 fixed-point read position (`+0x120` fraction, `+0x124` integer frame).
    pos: i64,
    /// The playback frequency in Hz; `None` plays at the mixer rate (one frame per output frame).
    freq: Option<u32>,
    /// The mixer rate the frequency was given against (`None` for a source built at the mixer rate).
    dst_rate: Option<u32>,
    /// Wave loops left (the wave's `+0xBC`).
    loops: u32,
    /// The wave stopped at its end.
    finished: bool,
}

impl PcmSource {
    /// Samples **already at the mixer rate**.
    pub fn new(data: Vec<i16>, channels: usize) -> PcmSource {
        PcmSource { data, channels: channels.max(1), pos: 0, freq: None, dst_rate: None, loops: 0, finished: false }
    }

    /// A clip that plays at `src_rate` Hz into a `dst_rate` mixer.
    pub fn with_rate(data: Vec<i16>, channels: usize, src_rate: u32, dst_rate: u32) -> PcmSource {
        PcmSource {
            data,
            channels: channels.max(1),
            pos: 0,
            freq: Some(src_rate),
            dst_rate: Some(dst_rate),
            loops: 0,
            finished: false,
        }
    }

    /// Play the wave `1 + loops` times back to back, as the engine plays a wave whose loop count is
    /// `loops` (a multi-wave group's `+0x2C`, carried to the wave's `+0xBC` by `FUN_00837830`). At the
    /// end of the data `FUN_00839e90` reads the count (wave vtable `+0x110`, `0x0099C6C0`): 0 stops
    /// the wave; otherwise the read position drops by the data length — so playback resumes at the
    /// start, keeping the overshoot — and, the count being positive, it is set to count − 1 (vtable
    /// `+0x114`, `0x00839230`). `0xFF` is 255 like any other count: 256 plays.
    pub fn with_loops(mut self, loops: u32) -> PcmSource {
        self.loops = loops;
        self
    }

    /// Play on at `src_rate` Hz (a pitch change). Only a source built with
    /// [`with_rate`](Self::with_rate) knows its mixer rate.
    pub fn set_source_rate(&mut self, src_rate: u32) -> Result<(), MixerError> {
        match self.dst_rate {
            Some(dst) if dst > 0 => {
                self.freq = Some(src_rate);
                Ok(())
            }
            _ => Err(MixerError::NoMixerRate),
        }
    }

    /// The playback frequency, if it was given.
    pub fn source_rate(&self) -> Option<u32> {
        self.freq
    }

    fn frames(&self) -> i64 {
        (self.data.len() / self.channels) as i64
    }

    fn pos_int(&self) -> i64 {
        self.pos >> 32
    }

    /// `(freq << 32) / rate` (`FUN_00839fd0`: `__allshl` then `__alldiv`).
    fn step(&self, rate: u32) -> i64 {
        match self.freq {
            None => 1 << 32,
            Some(f) => (i64::from(f) << 32) / i64::from(rate.max(1)),
        }
    }

    /// `FUN_00839e90`: at the end of the data, wrap (true) or stop (false).
    fn end_of_data(&mut self) -> bool {
        let len = self.frames();
        if self.loops == 0 {
            self.pos = len << 32;
            self.finished = true;
            return false;
        }
        self.pos -= len << 32;
        self.loops -= 1;
        true
    }

    /// `FUN_0083a440`: advance `frames` output frames without mixing.
    fn advance_silent(&mut self, frames: usize, rate: u32) {
        self.pos = self.pos.wrapping_add(self.step(rate).wrapping_mul(frames as i64));
        while self.pos_int() >= self.frames() {
            if !self.end_of_data() {
                break;
            }
        }
    }

    /// `FUN_00839fd0` / `FUN_0083a200`: mix `frames` output frames into the 6-channel `scratch` with
    /// integer gains `g`.
    fn mix(&mut self, scratch: &mut [i32], frames: usize, rate: u32, g: [i32; 6]) {
        let step = self.step(rate);
        let len = self.frames();
        let mut left = frames as i64;
        let mut off = 0usize;
        loop {
            if self.pos_int() >= len && !self.end_of_data() {
                return;
            }
            // The one chunk of an embedded wave spans the whole data (FUN_00839480 / FUN_00839820).
            let mut n = ((len << 32) - self.pos) / step.max(1);
            if n == 0 {
                n = 1;
            }
            if left <= n {
                n = left;
            }
            for f in 0..n as usize {
                let i = self.pos_int() as usize;
                let o = &mut scratch[(off + f) * 6..(off + f) * 6 + 6];
                if self.channels == 1 {
                    let s = i32::from(self.data.get(i).copied().unwrap_or(0));
                    for c in 0..6 {
                        o[c] = o[c].wrapping_add(s.wrapping_mul(g[c]) >> 15);
                    }
                } else {
                    let l = i32::from(self.data.get(i * 2).copied().unwrap_or(0));
                    let r = i32::from(self.data.get(i * 2 + 1).copied().unwrap_or(0));
                    stereo_frame(o, l, r, g);
                }
                self.pos = self.pos.wrapping_add(step);
            }
            left -= n;
            off += n as usize;
            if left == 0 {
                return;
            }
        }
    }
}

/// One stereo frame into six outputs (`FUN_0083eb00`, `param_5 == 6`), shortcuts included: gains at
/// or above `0xFFEC` on exactly one output pair add the samples doubled there only; gains at or above
/// `0x14` on exactly one pair mix that pair only; otherwise left feeds 0, 2, 4 and right 1, 3, 5.
fn stereo_frame(o: &mut [i32], l: i32, r: i32, g: [i32; 6]) {
    let bits = |min: i32| g.iter().enumerate().fold(0u8, |b, (i, &v)| b | (u8::from(v >= min) << i));
    let near_two = bits(0xFFEC);
    let audible = bits(0x14);
    let pair = |o: &mut [i32], k: usize, a: i32, b: i32| {
        o[k] = o[k].wrapping_add(a);
        o[k + 1] = o[k + 1].wrapping_add(b);
    };
    match (near_two, audible) {
        (3, _) => pair(o, 0, l * 2, r * 2),
        (0xC, _) => pair(o, 2, l * 2, r * 2),
        (0x30, _) => pair(o, 4, l * 2, r * 2),
        (_, 3) => pair(o, 0, l.wrapping_mul(g[0]) >> 15, r.wrapping_mul(g[1]) >> 15),
        (_, 0xC) => pair(o, 2, l.wrapping_mul(g[2]) >> 15, r.wrapping_mul(g[3]) >> 15),
        (_, 0x30) => pair(o, 4, l.wrapping_mul(g[4]) >> 15, r.wrapping_mul(g[5]) >> 15),
        _ => {
            for k in (0..6).step_by(2) {
                pair(o, k, g[k].wrapping_mul(l) >> 15, g[k + 1].wrapping_mul(r) >> 15);
            }
        }
    }
}

impl SampleSource for PcmSource {
    /// The samples the wave's kernel reads (the integer read position, no interpolation), with its
    /// loop handling; mono data is repeated on every channel.
    fn fill(&mut self, out: &mut [i16], channels: usize) -> usize {
        let want = out.len() / channels.max(1);
        let rate = self.dst_rate.unwrap_or(1);
        let step = self.step(rate);
        let mut written = 0;
        while written < want {
            if self.pos_int() >= self.frames() && !self.end_of_data() {
                break;
            }
            let i = self.pos_int() as usize;
            for ch in 0..channels {
                let c = ch.min(self.channels - 1);
                out[written * channels + ch] = self.data[i * self.channels + c];
            }
            self.pos = self.pos.wrapping_add(step);
            written += 1;
        }
        written
    }
    fn is_finished(&self) -> bool {
        self.finished || (self.pos_int() >= self.frames() && self.loops == 0)
    }
    fn reset(&mut self) {
        self.pos = 0;
        self.finished = false;
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

/// Why a mixer voice could not be adjusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixerError {
    /// No source is attached to the voice.
    NotAttached(VoiceId),
    /// The voice's source is not a [`PcmSource`], so it has no playback rate to change.
    NotPcm(VoiceId),
    /// The PCM source was built without a mixer rate ([`PcmSource::new`]).
    NoMixerRate,
}

impl std::fmt::Display for MixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MixerError::NotAttached(id) => write!(f, "mixer: voice {} has no source", id.0),
            MixerError::NotPcm(id) => write!(f, "mixer: voice {}'s source is not PCM", id.0),
            MixerError::NoMixerRate => write!(f, "mixer: the PCM source was built without a mixer rate"),
        }
    }
}

impl std::error::Error for MixerError {}

/// Which source a voice's wave mixes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKey {
    /// The shared 2D source for waves of up to two channels (engine `+0x1AC`).
    Flat,
    /// The shared 2D source for wider waves (engine `+0x1A8`).
    FlatWide,
    /// A per-emitter (3D) source.
    Emitter(u32),
}

/// A voice's source: a decoded clip, or any other source at the mixer rate.
enum VoiceSource {
    Pcm(PcmSource),
    Other(Box<dyn SampleSource>),
}

/// Per-voice mixing state beyond the FSM state in [`VoicePool`].
struct MixVoice {
    source: VoiceSource,
    /// Left/right gains from [`crate::spatial`], used as an emitter source's gains.
    left_gain: f32,
    right_gain: f32,
    /// The instance's six output-channel multipliers (wave `+0xD8`..`+0xEC`).
    channels: [f32; 6],
    /// Which source it mixes through.
    key: SourceKey,
    /// The kind-9 filter, when the voice's cue has one.
    filter: Option<Biquad>,
    /// Attach order (the order `FUN_00838710` adds waves to a source).
    order: u64,
}

/// Mixer configuration.
#[derive(Clone, Copy, Debug)]
pub struct MixerConfig {
    /// Output sample rate. 44100 with EAX/enabled, else 22050 (`GetOutputSampleRate` `FUN_008305d0`).
    pub sample_rate: u32,
    /// Device output channels: 1, 2, or 6 and more (see the module docs).
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
    /// Emitter sources in creation order.
    emitters: Vec<u32>,
    next_order: u64,
    /// The int32 accumulator (six per frame).
    accum: Vec<i32>,
    /// The source scratch `DAT_00FC34B0` (six per frame); what lies past the mixed frames persists,
    /// as in the engine's static buffer.
    scratch: Vec<i32>,
    /// A mono fill buffer for non-PCM sources.
    fill: Vec<i16>,
}

impl Mixer {
    /// A mixer with the given config. Panics on 3–5 output channels, which have no mapping from the
    /// engine's 5.1 mix here.
    pub fn new(cfg: MixerConfig) -> Mixer {
        assert!(
            matches!(cfg.channels, 1 | 2) || cfg.channels >= ENGINE_CHANNELS,
            "mixer: {} output channels; the engine mixes 5.1 and this mixer maps it to 1, 2 or 6+",
            cfg.channels
        );
        Mixer {
            cfg,
            voices: HashMap::new(),
            emitters: Vec::new(),
            next_order: 0,
            accum: Vec::new(),
            scratch: Vec::new(),
            fill: Vec::new(),
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

    fn insert(&mut self, id: VoiceId, source: VoiceSource) {
        let order = self.next_order;
        self.next_order += 1;
        self.voices.insert(
            id,
            MixVoice {
                source,
                left_gain: 1.0,
                right_gain: 1.0,
                channels: [1.0; 6],
                key: SourceKey::Flat,
                filter: None,
                order,
            },
        );
    }

    /// Attach a sample source to a voice (it mixes through the 2D source until told otherwise).
    pub fn attach(&mut self, id: VoiceId, source: Box<dyn SampleSource>) {
        self.insert(id, VoiceSource::Other(source));
    }

    /// Attach a decoded clip ([`set_source_rate`](Self::set_source_rate) changes its frequency).
    pub fn attach_pcm(&mut self, id: VoiceId, source: PcmSource) {
        let key = if source.channels > 2 { SourceKey::FlatWide } else { SourceKey::Flat };
        self.insert(id, VoiceSource::Pcm(source));
        if let Some(v) = self.voices.get_mut(&id) {
            v.key = key;
        }
    }

    /// A PCM voice's playback frequency.
    pub fn source_rate(&self, id: VoiceId) -> Result<Option<u32>, MixerError> {
        match self.voices.get(&id) {
            None => Err(MixerError::NotAttached(id)),
            Some(MixVoice { source: VoiceSource::Pcm(p), .. }) => Ok(p.source_rate()),
            Some(_) => Err(MixerError::NotPcm(id)),
        }
    }

    /// Whether a voice has a source attached.
    pub fn is_attached(&self, id: VoiceId) -> bool {
        self.voices.contains_key(&id)
    }

    /// Change a PCM voice's playback frequency to `src_rate` Hz.
    pub fn set_source_rate(&mut self, id: VoiceId, src_rate: u32) -> Result<(), MixerError> {
        match self.voices.get_mut(&id) {
            None => Err(MixerError::NotAttached(id)),
            Some(MixVoice { source: VoiceSource::Pcm(p), .. }) => p.set_source_rate(src_rate),
            Some(_) => Err(MixerError::NotPcm(id)),
        }
    }

    /// Detach a voice's source (on stop/steal/finish).
    pub fn detach(&mut self, id: VoiceId) {
        self.voices.remove(&id);
    }

    /// Set a voice's left/right spatial gains (an emitter source's gains).
    pub fn set_channel_gains(&mut self, id: VoiceId, left: f32, right: f32) {
        if let Some(v) = self.voices.get_mut(&id) {
            v.left_gain = left;
            v.right_gain = right;
        }
    }

    /// Set a voice's six output-channel multipliers (wave vtable `+0x10C`).
    pub fn set_output_channels(&mut self, id: VoiceId, channels: [f32; 6]) {
        if let Some(v) = self.voices.get_mut(&id) {
            v.channels = channels;
        }
    }

    /// Route a voice through a source.
    pub fn set_source(&mut self, id: VoiceId, key: SourceKey) {
        if let SourceKey::Emitter(e) = key {
            if !self.emitters.contains(&e) {
                self.emitters.push(e);
            }
        }
        if let Some(v) = self.voices.get_mut(&id) {
            v.key = key;
        }
    }

    /// Give a voice the kind-9 filter (`FUN_00839db0`).
    pub fn add_filter(&mut self, id: VoiceId) {
        if let Some(v) = self.voices.get_mut(&id) {
            v.filter = Some(Biquad::new());
        }
    }

    /// Hand a voice's filter its two parameters (`FUN_0083e5c0` → `SetParam` 0 and 1).
    pub fn set_filter_params(&mut self, id: VoiceId, a: f32, b: f32) {
        if let Some(f) = self.voices.get_mut(&id).and_then(|v| v.filter.as_mut()) {
            f.set_param(0, a);
            f.set_param(1, b);
        }
    }

    /// A voice's filter, if it has one.
    pub fn filter(&self, id: VoiceId) -> Option<&Biquad> {
        self.voices.get(&id).and_then(|v| v.filter.as_ref())
    }

    /// Number of voices with a live source.
    pub fn active_sources(&self) -> usize {
        self.voices.len()
    }

    /// **MixSources** (`FUN_00836610`): render `frames` frames of every audible voice into `out`
    /// (interleaved, device-channel-wide) through the engine's source path (module docs).
    /// `category_gain(category_id)` supplies the category volume the instance multiplies in before
    /// its `[0, 1]` clamp.
    pub fn mix(&mut self, pool: &mut VoicePool, out: &mut [i16], category_gain: impl Fn(u32) -> f32) {
        let dev = self.cfg.channels;
        let frames = out.len() / dev;
        let rate = self.cfg.sample_rate;
        let n = frames * ENGINE_CHANNELS;
        self.accum.clear();
        self.accum.resize(n, 0);
        if self.scratch.len() < n {
            self.scratch.resize(n, 0);
        }
        let mut finished: Vec<VoiceId> = Vec::new();

        let mut keys: Vec<SourceKey> = self.emitters.iter().map(|&e| SourceKey::Emitter(e)).collect();
        keys.push(SourceKey::Flat);
        keys.push(SourceKey::FlatWide);
        for key in keys {
            let mut ids: Vec<(u64, VoiceId)> =
                self.voices.iter().filter(|(_, v)| v.key == key).map(|(id, v)| (v.order, *id)).collect();
            if ids.is_empty() {
                continue;
            }
            ids.sort_unstable_by_key(|(order, _)| *order);
            // FUN_0083ade0: zero the scratch; a 2D source's gains are 1.0.
            for s in &mut self.scratch[..n] {
                *s = 0;
            }
            let mut gains = [1.0f32; 6];
            if let SourceKey::Emitter(_) = key {
                let v = &self.voices[&ids[0].1];
                gains = [v.left_gain, v.right_gain, 0.0, 0.0, 0.0, 0.0];
            }
            for (_, id) in ids {
                let Some(voice) = pool.get(id).filter(|v| v.state.is_audible()) else { continue };
                // FUN_00836c70 clamps the instance volume (0x008373AA); FUN_0083e1d0 clamps to [0, 2].
                let d = (voice.gain * voice.fade * category_gain(u32::from(voice.category))).clamp(0.0, 1.0);
                let mv = self.voices.get_mut(&id).expect("listed");
                let master = trunc_i32(d.clamp(0.0, 2.0) * 32768.0);
                let mut g = [0i32; 6];
                for (c, gc) in g.iter_mut().enumerate() {
                    *gc = trunc_i32((mv.channels[c] * d).clamp(0.0, 2.0) * 32768.0);
                }
                match &mut mv.source {
                    VoiceSource::Pcm(p) => {
                        if master > 0x13 {
                            p.mix(&mut self.scratch, frames, rate, g);
                        } else {
                            p.advance_silent(frames, rate);
                        }
                        if p.is_finished() {
                            finished.push(id);
                        }
                    }
                    VoiceSource::Other(src) => {
                        self.fill.clear();
                        self.fill.resize(frames, 0);
                        let got = src.fill(&mut self.fill, 1);
                        if master > 0x13 {
                            for (frame, &s) in self.scratch.chunks_exact_mut(6).zip(&self.fill[..got]) {
                                let s = i32::from(s);
                                for (o, gc) in frame.iter_mut().zip(g) {
                                    *o = o.wrapping_add(s.wrapping_mul(gc) >> 15);
                                }
                            }
                        }
                        if got < frames && src.is_finished() {
                            finished.push(id);
                        }
                    }
                }
                if let Some(filter) = mv.filter.as_mut() {
                    let wave_channels = match &mv.source {
                        VoiceSource::Pcm(p) => p.channels,
                        VoiceSource::Other(_) => 1,
                    };
                    filter.process(&mut self.scratch, n, wave_channels, rate);
                }
            }
            // FUN_0083afc0.
            for (acc, src) in self.accum.chunks_exact_mut(6).zip(self.scratch[..n].chunks_exact(6)) {
                for ((a, &s), gc) in acc.iter_mut().zip(src).zip(gains) {
                    *a = trunc_i32(s as f32 * gc + *a as f32);
                }
            }
        }

        // FUN_0083cbf0 (packssdw), then the device's channels.
        for f in 0..frames {
            for d in 0..dev {
                let s = if d < ENGINE_CHANNELS { self.accum[f * 6 + d] } else { 0 };
                out[f * dev + d] = s.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
        }

        for id in finished {
            pool.mark_finished(id);
            self.voices.remove(&id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::VoiceRequest;

    fn drain(src: &mut PcmSource, frames: usize) -> Vec<i16> {
        let mut out = vec![0i16; frames];
        let n = src.fill(&mut out, 1);
        out.truncate(n);
        out
    }

    /// A loop count of 1 plays the wave twice; the wrap drops the position by the data length, so
    /// an overshoot carries into the repeat (`FUN_00839e90`).
    #[test]
    fn a_loop_count_plays_the_wave_count_plus_one_times() {
        let mut once = PcmSource::new(vec![10, 20, 30], 1);
        assert_eq!(drain(&mut once, 8), vec![10, 20, 30]);
        assert!(once.is_finished());
        let mut twice = PcmSource::new(vec![10, 20, 30], 1).with_loops(1);
        assert_eq!(drain(&mut twice, 8), vec![10, 20, 30, 10, 20, 30]);
        assert!(twice.is_finished());
        // Step 2 over 3 frames: positions 0, 2, then 4 wraps to 1, then 3 ends it.
        let mut stepped = PcmSource::with_rate(vec![10, 20, 30], 1, 2, 1).with_loops(1);
        assert_eq!(drain(&mut stepped, 8), vec![10, 30, 20]);
    }

    fn one_voice(gain: f32, channels: usize, data: Vec<i16>) -> (VoicePool, Mixer, VoiceId) {
        let mut pool = VoicePool::new(4);
        let mut mixer = Mixer::new(MixerConfig { sample_rate: 44100, channels });
        let id = pool.acquire(&VoiceRequest::default()).unwrap();
        pool.get_mut(id).unwrap().gain = gain;
        mixer.attach_pcm(id, PcmSource::new(data, 1));
        pool.tick(0.0); // Starting → CreatingWave
        pool.tick(0.0); // → Playing
        (pool, mixer, id)
    }

    /// A mono 2D wave feeds all six engine channels with `(s × trunc(g × 32768)) >> 15`; the final
    /// volume is clamped to [0, 1] first (`0x008373AA`).
    #[test]
    fn a_mono_wave_feeds_six_channels_and_the_volume_is_clamped() {
        let render = |gain: f32| {
            let (mut pool, mut mixer, _) = one_voice(gain, 6, vec![1000; 16]);
            let mut out = vec![0i16; 6 * 8];
            mixer.mix(&mut pool, &mut out, |_| 1.0);
            out[..6].to_vec()
        };
        assert_eq!(render(1.0), vec![1000; 6]);
        assert_eq!(render(2.5), vec![1000; 6], "clamped to 1");
        assert_eq!(render(0.3), vec![((1000 * trunc_i32(0.3 * 32768.0)) >> 15) as i16; 6]);
        let (mut pool, mut mixer, _) = one_voice(1.0, 2, vec![1000; 16]);
        let mut out = vec![0i16; 2 * 8];
        mixer.mix(&mut pool, &mut out, |_| 1.0);
        assert_eq!(&out[..2], &[1000, 1000], "a stereo device takes front left and right");
    }

    /// Two voices in one 2D source, the second with the filter: the filter runs over the whole
    /// source buffer after the second wave is mixed — so the first voice, mixed before it, is
    /// filtered too, and a third source-less mix would not be (FUN_0083b120 → FUN_00839ae0).
    #[test]
    fn the_filter_runs_over_the_shared_source_buffer() {
        let mut pool = VoicePool::new(4);
        let mut mixer = Mixer::new(MixerConfig { sample_rate: 44100, channels: 6 });
        let a = pool.acquire(&VoiceRequest::default()).unwrap();
        let b = pool.acquire(&VoiceRequest::default()).unwrap();
        let wave_a: Vec<i16> = (0..32).map(|i| if i % 2 == 0 { 8000 } else { -8000 }).collect();
        mixer.attach_pcm(a, PcmSource::new(wave_a.clone(), 1));
        mixer.attach_pcm(b, PcmSource::new(vec![0; 32], 1));
        mixer.add_filter(b);
        mixer.set_filter_params(b, 0.2, 1.0);
        pool.tick(0.0);
        pool.tick(0.0);
        let frames = 8;
        let mut out = vec![0i16; 6 * frames];
        mixer.mix(&mut pool, &mut out, |_| 1.0);

        // Reference: voice a into the scratch, voice b (silent) adds nothing, then b's filter over
        // frames × 6 samples of the scratch as one run.
        let mut scratch: Vec<i32> = (0..frames)
            .flat_map(|f| std::iter::repeat_n(i32::from(wave_a[f]), 6))
            .collect();
        let mut f = Biquad::new();
        f.set_param(0, 0.2);
        f.set_param(1, 1.0);
        f.process(&mut scratch, frames * 6, 1, 44100);
        let want: Vec<i16> = scratch.iter().map(|&s| s.clamp(-32768, 32767) as i16).collect();
        assert_eq!(out, want);
        assert_ne!(out[0..6], [8000i16; 6], "voice a was filtered by voice b's filter");
    }
}
