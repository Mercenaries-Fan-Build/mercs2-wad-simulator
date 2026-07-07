//! Device output backend — the DirectSound8 secondary-buffer analog.
//!
//! **Oracle (audio_code_map.md §3.3, §3.4):** the exe creates a DirectSound8 device
//! (`FUN_00831b10` `DirectSoundCreate8` → `SetCooperativeLevel(DSSCL_PRIORITY)` → primary buffer +
//! `SetFormat`), then the software mixer (`FUN_0083c880`) owns one **streaming secondary buffer**
//! (`CreateSoundBuffer(GETCURRENTPOSITION2 | GLOBALFOCUS)`) it Locks, fills with the saturated int16
//! mix, and Unlock/Plays. EAX2–5 reverb is set on the buffer's `IKsPropertySet`.
//!
//! ## Faithful-substitute note
//! We do **not** re-implement DirectSound8. The [`Mixer`](crate::mixer::Mixer) reproduces the exe's
//! *software* mix exactly (int32 accumulate → saturate int16, §3.4); this module just delivers those
//! finished int16 frames to the OS. [`CpalSink`] uses **cpal** (WASAPI/CoreAudio/ALSA) as a portable
//! stand-in for the DirectSound secondary buffer — same role (a ring the mix is streamed into), not
//! the exe's exact device. EAX hardware reverb has no portable analog and is a
//! `[faithful-blocker: no]` deferred item (see `DEFERRED.md`). The whole crate runs headless via
//! [`NullSink`] when no device is present, so tests and servers never need audio hardware.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A sink that finished int16 PCM frames are streamed to. The mixer is device-independent; a sink is
/// an optional consumer of its output.
///
/// Not `Send`: the exe runs its audio on one thread (the VM + the mixer share the engine CS), and
/// [`CpalSink`] holds a `cpal::Stream` which is `!Send` on every platform. The engine is driven from
/// one thread, matching that.
pub trait AudioSink {
    /// Submit interleaved int16 frames (channel count = [`channels`](Self::channels)).
    fn submit(&mut self, samples: &[i16]);
    /// Output sample rate.
    fn sample_rate(&self) -> u32;
    /// Output channel count.
    fn channels(&self) -> usize;
}

/// A headless sink — discards everything. The default; keeps the mixer fully functional with no
/// device (tests, dedicated servers).
#[derive(Clone, Copy, Debug)]
pub struct NullSink {
    pub sample_rate: u32,
    pub channels: usize,
}

impl Default for NullSink {
    fn default() -> Self {
        NullSink {
            sample_rate: 44100,
            channels: 2,
        }
    }
}

impl AudioSink for NullSink {
    fn submit(&mut self, _samples: &[i16]) {}
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn channels(&self) -> usize {
        self.channels
    }
}

/// A test/telemetry sink that retains everything submitted (for asserting the mixer reached the
/// device layer without needing hardware).
#[derive(Clone, Debug, Default)]
pub struct CaptureSink {
    pub sample_rate: u32,
    pub channels: usize,
    pub captured: Vec<i16>,
}

impl CaptureSink {
    pub fn new(sample_rate: u32, channels: usize) -> CaptureSink {
        CaptureSink {
            sample_rate,
            channels,
            captured: Vec::new(),
        }
    }
}

impl AudioSink for CaptureSink {
    fn submit(&mut self, samples: &[i16]) {
        self.captured.extend_from_slice(samples);
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn channels(&self) -> usize {
        self.channels
    }
}

/// A real device sink over **cpal** (the DirectSound-secondary-buffer substitute). Submitted frames
/// are pushed into a shared ring the audio callback drains; underruns produce silence (as the exe's
/// buffer does on a mix stall). Holds the cpal stream alive for the sink's lifetime.
pub struct CpalSink {
    sample_rate: u32,
    channels: usize,
    ring: Arc<Mutex<VecDeque<i16>>>,
    // The active output stream. Kept alive; playback stops when the sink is dropped.
    _stream: cpal::Stream,
}

impl CpalSink {
    /// Open the default output device and start a stream. Returns `Err` (so the engine can fall back
    /// to [`NullSink`]) if there is no device or the format cannot be built.
    pub fn try_default() -> Result<CpalSink, String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("default_output_config: {e}"))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;

        let ring: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
        let ring_cb = ring.clone();

        let err_fn = |e| eprintln!("mercs2_audio cpal stream error: {e}");

        // Pull one int16 sample from the ring (silence on underrun).
        let pull = move |ring: &Arc<Mutex<VecDeque<i16>>>| -> i16 {
            ring.lock().ok().and_then(|mut r| r.pop_front()).unwrap_or(0)
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |out: &mut [f32], _| {
                    for s in out.iter_mut() {
                        *s = pull(&ring_cb) as f32 / 32768.0;
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |out: &mut [i16], _| {
                    for s in out.iter_mut() {
                        *s = pull(&ring_cb);
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                move |out: &mut [u16], _| {
                    for s in out.iter_mut() {
                        *s = (pull(&ring_cb) as i32 + 32768) as u16;
                    }
                },
                err_fn,
                None,
            ),
            other => return Err(format!("unsupported sample format {other:?}")),
        }
        .map_err(|e| format!("build_output_stream: {e}"))?;

        stream.play().map_err(|e| format!("stream.play: {e}"))?;

        Ok(CpalSink {
            sample_rate,
            channels,
            ring,
            _stream: stream,
        })
    }

    /// Frames currently buffered in the ring (for underrun/backpressure telemetry).
    pub fn queued_samples(&self) -> usize {
        self.ring.lock().map(|r| r.len()).unwrap_or(0)
    }
}

impl AudioSink for CpalSink {
    fn submit(&mut self, samples: &[i16]) {
        if let Ok(mut r) = self.ring.lock() {
            r.extend(samples.iter().copied());
        }
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn channels(&self) -> usize {
        self.channels
    }
}
