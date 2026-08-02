//! One-shot audio preview — play a decoded clip on the default output device.
//!
//! [`mercs2_audio::backend::CpalSink`] owns a `cpal::Stream`, which is `!Send`, so it lives on a
//! dedicated thread. The UI hands PCM jobs across a channel; the thread clears whatever was playing
//! (a preview should interrupt, not queue), resamples the clip to the device rate, maps it to the
//! device's channel count, and submits it. No device (headless, or none installed) is a silent no-op,
//! never a failure.

use mercs2_audio::backend::{AudioSink, CpalSink};
use std::sync::mpsc::{channel, Sender};

struct Job {
    samples: Vec<i16>,
    rate: u32,
    channels: u8,
}

/// A handle to the audio-preview thread. Dropping it ends the thread and stops playback.
pub struct Player {
    tx: Sender<Job>,
    _thread: std::thread::JoinHandle<()>,
}

impl Player {
    /// Start the preview thread, opening the default output device. Returns `None` if there is no
    /// device or the stream cannot be built — the app simply plays nothing.
    pub fn start() -> Option<Player> {
        let (tx, rx) = channel::<Job>();
        // Probe the device on THIS thread so a failure returns `None` synchronously; then hand the
        // sink to the worker. `CpalSink` is `!Send`, so it cannot cross — build it in the worker and
        // report readiness back.
        let (ready_tx, ready_rx) = channel::<bool>();
        let thread = std::thread::spawn(move || {
            let mut sink = match CpalSink::try_default() {
                Ok(s) => {
                    let _ = ready_tx.send(true);
                    s
                }
                Err(e) => {
                    eprintln!("audio preview: no output device ({e})");
                    let _ = ready_tx.send(false);
                    return;
                }
            };
            let dev_rate = sink.sample_rate();
            let dev_ch = sink.channels().max(1);
            while let Ok(job) = rx.recv() {
                sink.clear();
                let out = resample_map(&job.samples, job.channels.max(1) as usize, job.rate, dev_ch, dev_rate);
                sink.submit(&out);
            }
        });
        match ready_rx.recv() {
            Ok(true) => Some(Player { tx, _thread: thread }),
            _ => None,
        }
    }

    /// Play a decoded clip now, interrupting whatever was playing.
    pub fn play(&self, samples: Vec<i16>, rate: u32, channels: u8) {
        let _ = self.tx.send(Job { samples, rate, channels });
    }
}

/// Linear-resample interleaved int16 from `src_rate` to `dev_rate` and map `src_ch` → `dev_ch`
/// (mono fans out to every channel; stereo down-mixes to mono or fills the front pair).
fn resample_map(src: &[i16], src_ch: usize, src_rate: u32, dev_ch: usize, dev_rate: u32) -> Vec<i16> {
    if src.is_empty() || src_ch == 0 || dev_ch == 0 || dev_rate == 0 {
        return Vec::new();
    }
    let frames = src.len() / src_ch;
    if frames == 0 {
        return Vec::new();
    }
    let ratio = src_rate as f64 / dev_rate as f64;
    let out_frames = ((frames as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_frames * dev_ch);
    let sample = |frame: usize, ch: usize| -> f64 {
        src.get(frame * src_ch + ch.min(src_ch - 1)).copied().unwrap_or(0) as f64
    };
    for i in 0..out_frames {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = pos - idx as f64;
        let lerp = |ch: usize| {
            let a = sample(idx, ch);
            let b = sample((idx + 1).min(frames - 1), ch);
            a + (b - a) * frac
        };
        if src_ch == 1 {
            let m = lerp(0) as i16;
            for _ in 0..dev_ch {
                out.push(m);
            }
        } else {
            let l = lerp(0);
            let r = lerp(1);
            if dev_ch == 1 {
                out.push(((l + r) * 0.5) as i16);
            } else {
                out.push(l as i16);
                out.push(r as i16);
                for _ in 2..dev_ch {
                    out.push(0);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_maps_channels_and_length() {
        // Mono 8-frame at 8 kHz → stereo at 16 kHz ≈ doubles the frames, fans out to both channels.
        let src: Vec<i16> = (0..8).collect();
        let out = resample_map(&src, 1, 8000, 2, 16000);
        assert_eq!(out.len() % 2, 0, "stereo output is frame-aligned");
        let out_frames = out.len() / 2;
        assert!((out_frames as i32 - 16).abs() <= 1, "≈2x frames, got {out_frames}");
        // Each output frame has its two channels equal (mono fanned out).
        for f in out.chunks_exact(2) {
            assert_eq!(f[0], f[1]);
        }
        // Downmix path and empty input are well-behaved.
        assert!(resample_map(&[], 1, 44100, 2, 48000).is_empty());
        assert!(!resample_map(&[1, 2, 3, 4], 2, 44100, 1, 44100).is_empty());
    }
}
