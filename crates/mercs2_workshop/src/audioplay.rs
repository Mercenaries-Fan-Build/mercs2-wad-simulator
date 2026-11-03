//! Audio preview — play a decoded clip on the default output device, with pause, resume and the
//! position the device has reached.
//!
//! [`mercs2_audio::backend::CpalSink`] owns a `cpal::Stream`, which is `!Send`, so it lives on a
//! dedicated thread. The UI hands it messages across a channel. A new clip interrupts the last
//! (a preview should interrupt, not queue): it is resampled to the device rate, mapped to the
//! device's channel count, and fed to the device a short lead at a time ([`Playback`]), so a pause
//! can take back what the device has not played and the position is what it has. No device
//! (headless, or none installed) makes [`Player::start`] return `None`.

use mercs2_audio::backend::{AudioSink, CpalSink};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where the player is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status {
    /// The clip's id, as [`Player::play`] was given it.
    pub clip: u32,
    /// Seconds the device has played.
    pub position_s: f32,
    /// The clip's length in seconds.
    pub length_s: f32,
    /// Paused, holding `position_s`.
    pub paused: bool,
}

enum Msg {
    Play { clip: u32, samples: Vec<i16>, rate: u32, channels: u8, at_s: f32, paused: bool },
    Pause,
    Resume,
    Seek(f32),
    Stop,
}

/// Seconds of audio kept queued in the device ahead of what it plays.
const LEAD_S: f32 = 0.1;
/// Seconds submitted at a time.
const CHUNK_S: f32 = 0.02;
/// How far a line may sit from the timeline it follows before it is moved (the device's lead is
/// [`LEAD_S`]; a scrub moves further than this).
const DRIFT_S: f32 = 0.25;

/// The line a timeline (a FaceFX take) wants this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Want {
    pub clip: u32,
    /// Whether the timeline is running.
    pub playing: bool,
    /// The timeline's position in seconds.
    pub at_s: f32,
    /// The line's length in seconds.
    pub length_s: f32,
}

/// What [`follow`] asks of the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    Start { at_s: f32, paused: bool },
    Pause,
    Resume,
    Seek(f32),
    Stop,
}

/// What the player must do for the line to match the timeline this frame. `want` is the line wanted
/// (`None`: no line), `owned` the line this follower last wanted, `have` the player's state. Only
/// `owned`'s clip is ever stopped, and a clip the player was given by someone else is left to play:
/// the line comes back when that clip ends.
pub fn follow(want: Option<&Want>, owned: Option<u32>, have: Option<Status>) -> Vec<Cmd> {
    let mine = have.filter(|s| Some(s.clip) == owned);
    let Some(w) = want else {
        return mine.map(|_| Cmd::Stop).into_iter().collect();
    };
    if owned != Some(w.clip) {
        return if w.at_s < w.length_s {
            vec![Cmd::Start { at_s: w.at_s, paused: !w.playing }]
        } else {
            mine.map(|_| Cmd::Stop).into_iter().collect()
        };
    }
    match have {
        Some(s) if s.clip == w.clip => {
            let mut cmds = Vec::new();
            if (s.position_s - w.at_s).abs() > DRIFT_S {
                cmds.push(Cmd::Seek(w.at_s));
            }
            if s.paused && w.playing {
                cmds.push(Cmd::Resume);
            } else if !s.paused && !w.playing {
                cmds.push(Cmd::Pause);
            }
            cmds
        }
        Some(_) => Vec::new(),
        None if w.playing && w.at_s + DRIFT_S < w.length_s => vec![Cmd::Start { at_s: w.at_s, paused: false }],
        None => Vec::new(),
    }
}

/// One clip being fed to a sink, already at the sink's rate and channel count.
struct Playback {
    clip: u32,
    out: Vec<i16>,
    /// Samples submitted so far (interleaved).
    cursor: usize,
    ch: usize,
    rate: u32,
    paused: bool,
}

impl Playback {
    fn new(clip: u32, samples: &[i16], rate: u32, channels: u8, dev_ch: usize, dev_rate: u32) -> Playback {
        let out = resample_map(samples, channels.max(1) as usize, rate, dev_ch.max(1), dev_rate);
        Playback { clip, out, cursor: 0, ch: dev_ch.max(1), rate: dev_rate.max(1), paused: false }
    }

    /// Move to `at_s` seconds, dropping what `sink` has queued; a pause holds.
    fn seek(&mut self, at_s: f32, sink: &mut dyn AudioSink) {
        sink.clear();
        let frame = (at_s.max(0.0) * self.rate as f32) as usize;
        self.cursor = (frame * self.ch).min(self.out.len());
    }

    fn samples_for(&self, secs: f32) -> usize {
        ((secs * self.rate as f32) as usize).max(1) * self.ch
    }

    /// Keep [`LEAD_S`] queued in `sink`, a [`CHUNK_S`] at a time; nothing while paused.
    fn feed(&mut self, sink: &mut dyn AudioSink) {
        if self.paused {
            return;
        }
        let (lead, chunk) = (self.samples_for(LEAD_S), self.samples_for(CHUNK_S));
        while sink.queued() < lead && self.cursor < self.out.len() {
            let end = (self.cursor + chunk).min(self.out.len());
            sink.submit(&self.out[self.cursor..end]);
            self.cursor = end;
        }
    }

    /// Take back what `sink` has not played, so the cursor is where the device stopped.
    fn pause(&mut self, sink: &mut dyn AudioSink) {
        if self.paused {
            return;
        }
        let q = sink.queued().min(self.cursor);
        sink.clear();
        self.cursor -= q;
        self.paused = true;
    }

    fn resume(&mut self) {
        self.paused = false;
    }

    fn status(&self, queued: usize) -> Status {
        let frames = |samples: usize| (samples / self.ch) as f32 / self.rate as f32;
        let played = if self.paused { self.cursor } else { self.cursor.saturating_sub(queued) };
        Status { clip: self.clip, position_s: frames(played), length_s: frames(self.out.len()), paused: self.paused }
    }

    fn done(&self, queued: usize) -> bool {
        !self.paused && self.cursor >= self.out.len() && queued == 0
    }
}

/// A handle to the audio-preview thread. Dropping it ends the thread and stops playback.
pub struct Player {
    tx: Sender<Msg>,
    status: Arc<Mutex<Option<Status>>>,
    _thread: std::thread::JoinHandle<()>,
}

impl Player {
    /// Start the preview thread, opening the default output device. Returns `None` if there is no
    /// device or the stream cannot be built.
    pub fn start() -> Option<Player> {
        let (tx, rx) = channel::<Msg>();
        let status: Arc<Mutex<Option<Status>>> = Arc::new(Mutex::new(None));
        let shared = status.clone();
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
            let (dev_rate, dev_ch) = (sink.sample_rate(), sink.channels().max(1));
            let mut playing: Option<Playback> = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(Msg::Play { clip, samples, rate, channels, at_s, paused }) => {
                        sink.clear();
                        let mut p = Playback::new(clip, &samples, rate, channels, dev_ch, dev_rate);
                        p.seek(at_s, &mut sink);
                        p.paused = paused;
                        playing = Some(p);
                    }
                    Ok(Msg::Pause) => {
                        if let Some(p) = &mut playing {
                            p.pause(&mut sink);
                        }
                    }
                    Ok(Msg::Resume) => {
                        if let Some(p) = &mut playing {
                            p.resume();
                        }
                    }
                    Ok(Msg::Seek(at_s)) => {
                        if let Some(p) = &mut playing {
                            p.seek(at_s, &mut sink);
                        }
                    }
                    Ok(Msg::Stop) => {
                        sink.clear();
                        playing = None;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                if let Some(p) = &mut playing {
                    p.feed(&mut sink);
                    let q = sink.queued();
                    if p.done(q) {
                        playing = None;
                    }
                }
                if let Ok(mut s) = shared.lock() {
                    *s = playing.as_ref().map(|p| p.status(sink.queued()));
                }
            }
        });
        match ready_rx.recv() {
            Ok(true) => Some(Player { tx, status, _thread: thread }),
            _ => None,
        }
    }

    /// Play a decoded clip now, interrupting whatever was playing. `clip` is the caller's id for
    /// it, reported back in [`Status`].
    pub fn play(&self, clip: u32, samples: Vec<i16>, rate: u32, channels: u8) {
        self.play_from(clip, samples, rate, channels, 0.0, false);
    }

    /// [`Player::play`] from `at_s` seconds in, held there when `paused`.
    pub fn play_from(&self, clip: u32, samples: Vec<i16>, rate: u32, channels: u8, at_s: f32, paused: bool) {
        let _ = self.tx.send(Msg::Play { clip, samples, rate, channels, at_s, paused });
    }

    /// Move to `at_s` seconds; a pause holds.
    pub fn seek(&self, at_s: f32) {
        let _ = self.tx.send(Msg::Seek(at_s));
    }

    /// Stop now; [`Player::status`] goes back to `None`.
    pub fn stop(&self) {
        let _ = self.tx.send(Msg::Stop);
    }

    /// Pause, holding the position.
    pub fn pause(&self) {
        let _ = self.tx.send(Msg::Pause);
    }

    /// Resume where the pause held.
    pub fn resume(&self) {
        let _ = self.tx.send(Msg::Resume);
    }

    /// What is playing or paused, or `None` once the clip has played out.
    pub fn status(&self) -> Option<Status> {
        self.status.lock().ok().and_then(|s| *s)
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

    /// A sink with a queue a test plays out by hand, as the device callback would.
    struct Device {
        q: std::collections::VecDeque<i16>,
        played: Vec<i16>,
    }

    impl Device {
        fn play(&mut self, n: usize) {
            for _ in 0..n.min(self.q.len()) {
                let s = self.q.pop_front().expect("queued");
                self.played.push(s);
            }
        }
    }

    impl AudioSink for Device {
        fn submit(&mut self, samples: &[i16]) {
            self.q.extend(samples.iter().copied());
        }
        fn sample_rate(&self) -> u32 {
            1000
        }
        fn channels(&self) -> usize {
            1
        }
        fn clear(&mut self) {
            self.q.clear();
        }
        fn queued(&self) -> usize {
            self.q.len()
        }
    }

    /// The feed keeps a short lead queued; the position is what the device has played; a pause
    /// takes back the unplayed lead and holds the position; resuming continues from the next sample
    /// the device had not played, so nothing is skipped or repeated; the clip ends when all of it
    /// has played.
    #[test]
    fn pause_holds_the_position_and_resume_continues_from_it() {
        let samples: Vec<i16> = (0..2000).map(|i| i as i16).collect();
        let mut dev = Device { q: Default::default(), played: Vec::new() };
        let mut p = Playback::new(7, &samples, 1000, 1, 1, 1000);
        assert_eq!(p.status(0).length_s, 2.0);
        p.feed(&mut dev);
        assert!(dev.queued() >= 100 && dev.queued() < 140, "a 0.1 s lead, got {}", dev.queued());
        for _ in 0..53 {
            p.feed(&mut dev);
            dev.play(10);
        }
        assert_eq!(dev.played.len(), 530);
        let at = p.status(dev.queued()).position_s;
        assert!((at - 0.53).abs() < 1e-6, "position {at}");

        p.pause(&mut dev);
        assert_eq!(dev.queued(), 0, "the unplayed lead is taken back");
        let held = p.status(dev.queued());
        assert!(held.paused && (held.position_s - at).abs() < 1e-6, "{held:?}");
        p.feed(&mut dev);
        assert_eq!(dev.queued(), 0, "nothing is fed while paused");

        p.resume();
        while !p.done(dev.queued()) {
            p.feed(&mut dev);
            dev.play(37);
        }
        assert_eq!(dev.played, samples, "every sample plays once, in order, across the pause");
    }

    #[test]
    fn a_seek_moves_the_cursor_and_a_pause_holds_through_it() {
        let samples: Vec<i16> = (0..2000).map(|i| i as i16).collect();
        let mut dev = Device { q: Default::default(), played: Vec::new() };
        let mut p = Playback::new(7, &samples, 1000, 1, 1, 1000);
        p.feed(&mut dev);
        p.pause(&mut dev);
        p.seek(1.5, &mut dev);
        assert_eq!(dev.queued(), 0);
        let held = p.status(0);
        assert!(held.paused && (held.position_s - 1.5).abs() < 1e-6, "{held:?}");
        p.resume();
        p.feed(&mut dev);
        assert_eq!(dev.q.front(), Some(&1500), "the next sample is the seek's");
    }

    fn at(clip: u32, position_s: f32, paused: bool) -> Option<Status> {
        Some(Status { clip, position_s, length_s: 4.0, paused })
    }

    const LINE: Want = Want { clip: 7, playing: true, at_s: 1.0, length_s: 4.0 };

    #[test]
    fn a_new_line_starts_where_the_timeline_is() {
        let want = Want { playing: false, ..LINE };
        assert_eq!(follow(Some(&LINE), None, None), [Cmd::Start { at_s: 1.0, paused: false }]);
        assert_eq!(follow(Some(&want), Some(9), at(9, 0.5, false)), [Cmd::Start { at_s: 1.0, paused: true }]);
    }

    #[test]
    fn the_line_pauses_and_resumes_with_the_timeline() {
        let paused = Want { playing: false, ..LINE };
        assert_eq!(follow(Some(&paused), Some(7), at(7, 1.0, false)), [Cmd::Pause]);
        assert_eq!(follow(Some(&LINE), Some(7), at(7, 1.0, true)), [Cmd::Resume]);
        assert!(follow(Some(&LINE), Some(7), at(7, 1.1, false)).is_empty(), "in step within the drift");
    }

    #[test]
    fn a_scrub_moves_the_line_even_while_paused() {
        let scrubbed = Want { playing: false, at_s: 3.0, ..LINE };
        assert_eq!(follow(Some(&scrubbed), Some(7), at(7, 1.0, true)), [Cmd::Seek(3.0)]);
    }

    #[test]
    fn the_line_stops_when_the_take_goes_and_plays_out_at_the_timelines_end() {
        assert_eq!(follow(None, Some(7), at(7, 1.0, false)), [Cmd::Stop]);
        let end = Want { at_s: 4.0, ..LINE };
        assert!(follow(Some(&end), Some(7), at(7, 3.9, false)).is_empty(), "the tail is not cut");
    }

    #[test]
    fn a_line_that_played_out_starts_again_only_when_the_timeline_is_back_inside_it() {
        assert_eq!(follow(Some(&LINE), Some(7), None), [Cmd::Start { at_s: 1.0, paused: false }]);
        let tail = Want { at_s: 3.9, ..LINE };
        assert!(follow(Some(&tail), Some(7), None).is_empty(), "the face runs out the end of its line");
        let paused = Want { playing: false, ..LINE };
        assert!(follow(Some(&paused), Some(7), None).is_empty(), "nothing starts while paused");
    }

    #[test]
    fn a_clip_someone_else_gave_the_player_is_left_alone() {
        assert!(follow(None, None, at(3, 1.0, false)).is_empty());
        assert!(follow(None, Some(7), at(3, 1.0, false)).is_empty());
        assert!(follow(Some(&LINE), Some(7), at(3, 1.0, false)).is_empty());
    }
}
