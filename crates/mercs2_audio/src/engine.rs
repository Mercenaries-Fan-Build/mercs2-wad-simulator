//! `AudioEngine` — the engine-side facade the script bindings drive.
//!
//! This is the union of the Pg high-level umbrella (`PgSound::Update` **`FUN_005fa950`**) and the Pal
//! low-level umbrella (`PgSoundPlayer::Update` **`FUN_006073c0`**): it owns the sound DB, the voice
//! pool + software mixer, the category mixer, the bank manager, the dual-deck music machine, the VO
//! manager, and the listener set, and it exposes **real bodies** for the `Sound.*` (88) and `VO.*`
//! (11) Lua surface (audio_code_map.md §5, luacd 08_audio_presentation).
//!
//! ## Binding-wiring seam (how the Lua tables reach these bodies)
//! The `Sound`/`VO` luaL_Reg tables live in **`mercs2_script::bindings::{sound,vo}`**,
//! whose `install(&Lua, &SharedHost)` closures call the engine through the `mercs2_script::EngineHost`
//! trait — the script host never touches the engine directly (crate `mercs2_script` lib docs). This
//! crate does **not** edit `mercs2_script`. Instead:
//!   1. `mercs2_engine`'s `EngineHost` impl holds an [`AudioEngine`] (e.g. `Rc<RefCell<AudioEngine>>`).
//!   2. `EngineHost` gains audio methods (`sound_cue`, `sound_transition_music`, `vo_cue`, …) that
//!      forward 1:1 to the [`AudioEngine`] methods below.
//!   3. `bindings/sound.rs` / `bindings/vo.rs` `install` fills each `REQUIRED` cfunc with
//!      `b.real("CueSound", lua.create_function(|_, args| host.borrow_mut().sound_cue(..))?)?`.
//! Every method here is named to match its Lua binding so that mapping is mechanical. The 9 retail
//! `return 0` stubs (`SetSourceEnterMusic`, `AddFadeCategory`, …) stay faithful no-ops.

use std::collections::HashMap;

use mercs2_core::glam::Vec3;
use mercs2_formats::hash::pandemic_hash_m2;

use crate::backend::{AudioSink, NullSink};
#[cfg(feature = "device")]
use crate::backend::CpalSink;
use crate::banks::{BankKind, BankManager, CallbackId};
use crate::categories::{category_id, Categories};
use crate::mixer::{Mixer, MixerConfig, PcmSource, SampleSource};
use crate::music::MusicStateMachine;
use crate::sounddb::{CueEntry, SoundDb};
use crate::select::{self, PalRng, STATE_INIT};
use crate::soundbank::{CueBody, GroupForm, Soundbank, SoundbankError};
use crate::spatial::{self, ListenerSet, Listener};
use crate::vo::{VoManager, VoPriority};
use crate::voice::{VoiceId, VoicePool, VoiceRequest};
use crate::wave::{DecodedClip, WaveError, Wavebank};

/// Why a cue did not resolve ([`AudioEngine::resolve_cue`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// The soundbank the entry (or a cue sound) names is not resident.
    SoundbankNotResident(u32),
    /// The entry's cue index is past the soundbank's cue table.
    CueIndexOutOfRange { soundbank: u32, index: u32, cues: usize },
    /// The soundbank cue at the entry's index carries a different guid than the entry.
    GuidMismatch { entry: u32, soundbank_cue: u32 },
    /// A group index is past the soundbank's group table.
    GroupIndexOutOfRange { soundbank: u32, index: u16, groups: usize },
    /// A multi-track sound lists no entries, or a group lists no waves: the engine would index past
    /// the list.
    EmptyChoice { soundbank: u32, what: &'static str },
    /// A selection mode the engine picks nothing with (it knows 0, 1 and 2).
    SelectionMode { soundbank: u32, what: &'static str, mode: u8 },
    /// The wavebank a group wave names is not resident.
    WavebankNotResident(u32),
    /// A group wave's index is past the wavebank's record table.
    WaveIndexOutOfRange { wavebank: u32, index: u32, waves: usize },
    /// The wave lives in a `.pws` stream; it has no resident samples.
    Streamed { clip_hash: u32 },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::SoundbankNotResident(h) => write!(f, "soundbank 0x{h:08X} is not resident"),
            ResolveError::CueIndexOutOfRange { soundbank, index, cues } => {
                write!(f, "cue index {index} is past soundbank 0x{soundbank:08X}'s {cues} cues")
            }
            ResolveError::GuidMismatch { entry, soundbank_cue } => write!(
                f,
                "sounddb entry 0x{entry:08X} lands on soundbank cue 0x{soundbank_cue:08X}"
            ),
            ResolveError::GroupIndexOutOfRange { soundbank, index, groups } => {
                write!(f, "group index {index} is past soundbank 0x{soundbank:08X}'s {groups} groups")
            }
            ResolveError::EmptyChoice { soundbank, what } => {
                write!(f, "a {what} in soundbank 0x{soundbank:08X} lists nothing to pick")
            }
            ResolveError::SelectionMode { soundbank, what, mode } => write!(
                f,
                "a {what} in soundbank 0x{soundbank:08X} has selection mode {mode}; the engine picks nothing"
            ),
            ResolveError::WavebankNotResident(h) => write!(f, "wavebank 0x{h:08X} is not resident"),
            ResolveError::WaveIndexOutOfRange { wavebank, index, waves } => {
                write!(f, "wave index {index} is past wavebank 0x{wavebank:08X}'s {waves} waves")
            }
            ResolveError::Streamed { clip_hash } => {
                write!(f, "clip 0x{clip_hash:08X} streams from a .pws; no resident samples")
            }
        }
    }
}

/// One wave a group can play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedWave {
    /// The wavebank.
    pub wavebank: u32,
    /// The record index in it.
    pub index: u32,
    /// The wave's selection weight in its group.
    pub weight: f32,
    /// The resident clip's hash.
    pub clip_hash: u32,
}

/// One group a sound can pick.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedChoice {
    /// The entry's selection weight (1.0 for a single-track cue's one choice).
    pub weight: f32,
    /// The soundbank holding the group.
    pub soundbank: u32,
    /// The group's index there.
    pub group_index: u16,
    /// The group's wave selection mode; `None` for a single-wave group.
    pub selection: Option<u8>,
    /// Every wave the group can play, each resident and decoded.
    pub waves: Vec<ResolvedWave>,
}

/// One sound a cue fires.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSound {
    /// The track it belongs to (0 for a single-track cue).
    pub track: usize,
    /// When it fires, in seconds after the cue starts.
    pub start_s: f32,
    /// `(selection mode, state slot)` for a multi-track sound; `None` for a single-track cue's one
    /// sound, which plays its one group.
    pub selection: Option<(u8, u8)>,
    /// The groups it can pick, in entry order.
    pub choices: Vec<ResolvedChoice>,
}

/// Everything a cue can play, fully resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedCue {
    /// The soundbank holding the cue.
    pub soundbank: u32,
    /// The cue's index there.
    pub cue_index: u32,
    /// Every sound of every track.
    pub sounds: Vec<ResolvedSound>,
}

impl ResolvedCue {
    /// Every wave the cue can reach, in track / sound / choice / wave order (duplicates kept).
    pub fn waves(&self) -> impl Iterator<Item = &ResolvedWave> {
        self.sounds.iter().flat_map(|s| s.choices.iter().flat_map(|c| c.waves.iter()))
    }
}

/// One sound a cue start actually fired, after the engine's picks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickedSound {
    /// When it fires, in seconds after the cue starts.
    pub start_s: f32,
    /// The picked wave.
    pub wave: ResolvedWave,
}

impl std::error::Error for ResolveError {}

/// Library version reported by `Sound._GetLibVersion` (`FUN_005e4300` → `DAT_00dfdb4c` = 12.0).
pub const SOUND_LIB_VERSION: f32 = 12.0;

/// The audio engine facade.
pub struct AudioEngine {
    /// The parsed cue/wave catalog (`sounddb`, `FUN_00835b80`).
    pub sounddb: SoundDb,
    /// Software voice pool + priority-steal + 16-state FSM.
    pub pool: VoicePool,
    /// The software mixer (headless int16 renderer).
    pub mixer: Mixer,
    /// Per-category volume/pitch + master duck.
    pub categories: Categories,
    /// Sound/wave bank load state machine.
    pub banks: BankManager,
    /// Dual-deck dynamic-music machine (single region; see `DEFERRED.md` for multi-region).
    pub music: MusicStateMachine,
    /// VO arbitration.
    pub vo: VoManager,
    /// 3D listeners.
    pub listeners: ListenerSet,
    /// Resident soundbanks keyed by bank hash — the first hop of the cue chain (`sounddb` entry →
    /// soundbank cue → group).
    soundbanks: HashMap<u32, Soundbank>,
    /// Resident wavebanks keyed by bank hash — the last hop (group wave → decoded clip).
    wavebanks: HashMap<u32, Wavebank>,
    /// The sound random generator ([`crate::select`]).
    rng: PalRng,
    /// Per-group wave selection state, `(soundbank, group index)`, starting at `0xFFFFFFFF`.
    group_state: HashMap<(u32, u16), u32>,
    /// Per-cue sound selection state, `(soundbank, cue index, slot)`, starting at `0xFFFFFFFF`.
    slot_state: HashMap<(u32, u32, u8), u32>,
    /// Device sink (headless [`NullSink`] by default).
    sink: Box<dyn AudioSink>,
    /// True once a real output device is attached ([`attach_output_device`](Self::attach_output_device));
    /// gates the real-time [`pump`](Self::pump) so headless runs (tests/servers) never render for a
    /// discarding sink.
    has_device: bool,
    /// Real-time pump accumulator (fractional frames owed to the sink since the last render).
    pump_accum: f64,
    /// System pause (`Sound.SetSystemPause`) — freezes the runtime-sound submit pass.
    paused: bool,
    /// Survival-mode flag (`Sound.SetSurvivalMode`).
    survival: bool,
    /// Audio content directory (`Sound.GetAudioDir`).
    audio_dir: String,
}

impl Default for AudioEngine {
    fn default() -> Self {
        AudioEngine::new(MixerConfig::default())
    }
}

impl AudioEngine {
    /// A fresh engine with the given mixer config and a default voice pool, running headless.
    pub fn new(cfg: MixerConfig) -> AudioEngine {
        AudioEngine {
            sounddb: SoundDb::default(),
            pool: VoicePool::new(VoicePool::DEFAULT_CAPACITY),
            mixer: Mixer::new(cfg),
            categories: Categories::default(),
            banks: BankManager::new(),
            music: MusicStateMachine::new(),
            vo: VoManager::new(),
            listeners: ListenerSet::default(),
            soundbanks: HashMap::new(),
            wavebanks: HashMap::new(),
            rng: PalRng::new(clock_seed()),
            group_state: HashMap::new(),
            slot_state: HashMap::new(),
            sink: Box::new(NullSink {
                sample_rate: cfg.sample_rate,
                channels: cfg.channels,
            }),
            has_device: false,
            pump_accum: 0.0,
            paused: false,
            survival: false,
            audio_dir: "audio/".to_string(),
        }
    }

    // ---- device output + real-time pump -----------------------------------------------------------

    /// Open the default output device and route the mixer to it (the DirectSound-secondary-buffer
    /// analog). The mixer is rebuilt to the **device's own rate/channels** so its int16 frames feed the
    /// stream with no format mismatch; per-voice resampling ([`PcmSource::with_rate`]) handles each
    /// clip's native rate → this mixer rate. Returns `false` (staying headless on [`NullSink`]) when no
    /// device is present — never a hard failure, never behind a build flag. Call once at startup, before
    /// any voice is attached.
    #[cfg(feature = "device")]
    pub fn attach_output_device(&mut self) -> bool {
        match CpalSink::try_default() {
            Ok(sink) => {
                let rate = sink.sample_rate();
                let channels = sink.channels().max(1);
                self.set_output_format(rate, channels);
                self.sink = Box::new(sink);
                self.has_device = true;
                true
            }
            Err(_) => false,
        }
    }

    /// Decode-only build (`device` off — the headless WAD CLIs, which never open a device). Same
    /// contract as the "no device present" path above: stay on [`NullSink`], report `false`, never a
    /// hard failure. The engine and the game always compile with `device`, so they never see this.
    #[cfg(not(feature = "device"))]
    pub fn attach_output_device(&mut self) -> bool {
        false
    }

    /// Rebuild the mixer at a new output format (drops any attached sources — call before playback).
    pub fn set_output_format(&mut self, sample_rate: u32, channels: usize) {
        self.mixer = Mixer::new(MixerConfig { sample_rate, channels: channels.max(1) });
    }

    /// Whether a real output device is attached.
    pub fn has_device(&self) -> bool {
        self.has_device
    }

    /// Real-time producer: render exactly the frames that elapsed in `dt` and submit them to the device
    /// sink, keeping its ring fed at wall-clock rate. No-op when headless (no device) so tests/servers
    /// never render into a discarding sink. A long stall is capped at 250 ms of catch-up so a hitch does
    /// not burst-render a huge block. This is the frame-loop analog of the exe's 45 ms mixer thread.
    pub fn pump(&mut self, dt: f32) {
        if !self.has_device {
            return;
        }
        let rate = self.mixer.config().sample_rate as f64;
        self.pump_accum += (dt as f64).max(0.0) * rate;
        let max_frames = (rate * 0.25) as usize; // 250 ms catch-up cap
        let mut frames = self.pump_accum as usize;
        if frames == 0 {
            return;
        }
        if frames > max_frames {
            frames = max_frames;
            self.pump_accum = 0.0;
        } else {
            self.pump_accum -= frames as f64;
        }
        let _ = self.render(frames); // render() mixes + submits to the sink
    }

    // ---- resident banks (LoadSoundBank / LoadWaveBank payloads) ----------------------------------

    /// Decode a `wavebank` body (`Sound.LoadWaveBank`) and hold it resident under its bank hash.
    /// Returns the number of clips that carry decoded samples (a streamed bank's clips carry none; the
    /// slots still exist so a group's wave index lands on them). A body outside the measured layout is
    /// refused whole.
    pub fn load_wavebank(&mut self, body: &[u8]) -> Result<usize, WaveError> {
        let bank = Wavebank::parse(body)?;
        let audible = bank.clips.iter().filter(|c| !c.samples.is_empty()).count();
        self.wavebanks.insert(bank.self_hash, bank);
        Ok(audible)
    }

    /// Parse a `soundbank` body (`Sound.LoadSoundBank`) and hold it resident under its bank hash.
    /// Returns its cue count.
    pub fn load_soundbank(&mut self, body: &[u8]) -> Result<usize, SoundbankError> {
        let bank = Soundbank::parse(body)?;
        let cues = bank.cues.len();
        // A (re)loaded bank starts with fresh selection state, as the engine allocates it per load.
        self.group_state.retain(|(b, _), _| *b != bank.bank_hash);
        self.slot_state.retain(|(b, _, _), _| *b != bank.bank_hash);
        self.soundbanks.insert(bank.bank_hash, bank);
        Ok(cues)
    }

    /// Number of resident wave clips across every resident wavebank.
    pub fn resident_wave_count(&self) -> usize {
        self.wavebanks.values().map(|b| b.clips.len()).sum()
    }

    /// Reseed the sound random generator. The engine seeds it once from its tick counter, so its
    /// picks differ every run; a fixed seed makes this engine's picks reproducible.
    pub fn set_rng_seed(&mut self, seed: u32) {
        self.rng = PalRng::new(seed);
    }

    /// Everything a cue can play: `sounddb` entry → its soundbank's cue → every sound of every track
    /// (a single-track cue is one sound) → every group the sound can pick → every wave the group can
    /// play → the resident decoded clip. Fails, naming the reason, unless every path resolves.
    pub fn resolve_cue(&self, cue: &CueEntry) -> Result<ResolvedCue, ResolveError> {
        let bank = self
            .soundbanks
            .get(&cue.bank_hash)
            .ok_or(ResolveError::SoundbankNotResident(cue.bank_hash))?;
        let sb_cue = bank.cues.get(cue.cue_index as usize).ok_or(ResolveError::CueIndexOutOfRange {
            soundbank: cue.bank_hash,
            index: cue.cue_index,
            cues: bank.cues.len(),
        })?;
        if sb_cue.guid != cue.guid {
            return Err(ResolveError::GuidMismatch { entry: cue.guid, soundbank_cue: sb_cue.guid });
        }
        let sounds = match &sb_cue.body {
            CueBody::SingleTrack { soundbank, group_index, .. } => vec![ResolvedSound {
                track: 0,
                start_s: 0.0,
                selection: None,
                choices: vec![self.resolve_choice(*soundbank, *group_index, 1.0)?],
            }],
            CueBody::MultiTrack(m) => {
                let mut sounds = Vec::new();
                for (t, track) in m.tracks.iter().enumerate() {
                    for s in &track.sounds {
                        if s.entries.is_empty() {
                            return Err(ResolveError::EmptyChoice { soundbank: cue.bank_hash, what: "sound" });
                        }
                        if s.selection > 2 {
                            return Err(ResolveError::SelectionMode {
                                soundbank: cue.bank_hash,
                                what: "sound",
                                mode: s.selection,
                            });
                        }
                        let choices = s
                            .entries
                            .iter()
                            .map(|e| self.resolve_choice(e.soundbank, e.group_index, e.weight))
                            .collect::<Result<_, _>>()?;
                        sounds.push(ResolvedSound {
                            track: t,
                            start_s: s.start_s,
                            selection: Some((s.selection, s.slot)),
                            choices,
                        });
                    }
                }
                sounds
            }
        };
        Ok(ResolvedCue { soundbank: cue.bank_hash, cue_index: cue.cue_index, sounds })
    }

    fn resolve_choice(&self, soundbank: u32, group_index: u16, weight: f32) -> Result<ResolvedChoice, ResolveError> {
        let bank = self.soundbanks.get(&soundbank).ok_or(ResolveError::SoundbankNotResident(soundbank))?;
        let group = bank.groups.get(group_index as usize).ok_or(ResolveError::GroupIndexOutOfRange {
            soundbank,
            index: group_index,
            groups: bank.groups.len(),
        })?;
        let selection = match &group.form {
            GroupForm::Single { .. } => None,
            GroupForm::Multi(m) => {
                if m.selection > 2 {
                    return Err(ResolveError::SelectionMode { soundbank, what: "group", mode: m.selection });
                }
                Some(m.selection)
            }
        };
        if group.waves().is_empty() {
            return Err(ResolveError::EmptyChoice { soundbank, what: "group" });
        }
        let waves = group
            .waves()
            .iter()
            .map(|w| {
                let clip = self.clip(w.wavebank, w.index)?;
                if clip.streaming {
                    return Err(ResolveError::Streamed { clip_hash: clip.clip_hash });
                }
                Ok(ResolvedWave { wavebank: w.wavebank, index: w.index, weight: w.weight, clip_hash: clip.clip_hash })
            })
            .collect::<Result<_, _>>()?;
        Ok(ResolvedChoice { weight, soundbank, group_index, selection, waves })
    }

    /// The resident clip at `(wavebank, index)`.
    pub fn clip(&self, wavebank: u32, index: u32) -> Result<&DecodedClip, ResolveError> {
        let bank = self.wavebanks.get(&wavebank).ok_or(ResolveError::WavebankNotResident(wavebank))?;
        bank.clips.get(index as usize).ok_or(ResolveError::WaveIndexOutOfRange {
            wavebank,
            index,
            waves: bank.clips.len(),
        })
    }

    /// Start a cue the way the engine does: for each sound, in track and sound order, pick an entry
    /// (a multi-track sound's selection mode and state slot) and then a wave of that entry's group (the
    /// group's selection mode and state), drawing from the engine's generator ([`crate::select`]).
    /// A sound the engine would pick nothing for is left out. The engine makes each pick when its
    /// sound fires; sounds that start together are picked in this same order.
    pub fn pick_cue(&mut self, cue: &CueEntry) -> Result<Vec<PickedSound>, ResolveError> {
        let resolved = self.resolve_cue(cue)?;
        let mut picked = Vec::new();
        for s in &resolved.sounds {
            let choice = match s.selection {
                None => Some(0),
                Some((mode, slot)) => {
                    let weights: Vec<f32> = s.choices.iter().map(|c| c.weight).collect();
                    let st = self.slot_state.entry((resolved.soundbank, resolved.cue_index, slot)).or_insert(STATE_INIT);
                    select::pick(mode, &weights, st, &mut self.rng)
                }
            };
            let Some(choice) = choice.and_then(|i| s.choices.get(i)) else { continue };
            let wave = match choice.selection {
                None => Some(0),
                Some(mode) => {
                    let weights: Vec<f32> = choice.waves.iter().map(|w| w.weight).collect();
                    let st = self.group_state.entry((choice.soundbank, choice.group_index)).or_insert(STATE_INIT);
                    select::pick(mode, &weights, st, &mut self.rng)
                }
            };
            if let Some(w) = wave.and_then(|i| choice.waves.get(i)) {
                picked.push(PickedSound { start_s: s.start_s, wave: *w });
            }
        }
        Ok(picked)
    }

    /// Install the parsed sound database (chain: `Sound.AddPgAsset("Mercs2Globals","sounddb")`).
    pub fn set_sounddb(&mut self, db: SoundDb) {
        self.sounddb = db;
    }

    /// Attach a device sink (e.g. `CpalSink::try_default()`), replacing the headless default. The
    /// mixer is unaffected — it always renders; the sink only decides where the frames go.
    pub fn set_sink(&mut self, sink: Box<dyn AudioSink>) {
        self.sink = sink;
    }

    // ---- listeners -------------------------------------------------------------------------------

    /// `UpdateListeners` (`FUN_00608aa0`): set listener `slot`'s pose/velocity.
    pub fn set_listener(&mut self, slot: usize, l: Listener) {
        self.listeners.set(slot, l);
    }

    // ---- Sound.* : playback ----------------------------------------------------------------------

    /// `Sound.CueSound(cue [, position])` (shim `FUN_005e0ff0` → `thunk_FUN_024b65e0`).
    ///
    /// // CONFIRM-LIVE: the exe's cue queue-post is SecuROM-morphed (`thunk_FUN_024b65e0`). This models
    /// the observable result: resolve the cue in the sound DB, allocate a voice (priority-steal if the
    /// pool is full), and — if `position` is given and the cue is positional — compute 3D channel
    /// gains against the closest listener. `source` is the decoded wave (from the wave-bank system); pass
    /// `None` to allocate a silent voice (the wave-bind is the streaming seam). Returns the voice id,
    /// or `None` if the cue is unknown or the pool denied it (outranked).
    pub fn cue_sound(
        &mut self,
        cue_id: u32,
        position: Option<Vec3>,
        source: Option<Box<dyn SampleSource>>,
    ) -> Option<VoiceId> {
        let cue: CueEntry = *self.sounddb.find_cue(cue_id)?;
        let mut req = VoiceRequest {
            cue_guid: cue.guid,
            priority: cue.priority,
            category: cue.category,
            gain: if cue.default_gain > 0.0 { cue.default_gain } else { 1.0 },
            looping: cue.is_looping(),
            positional: cue.is_positional() && position.is_some(),
            start_delay: 0.0,
        };

        // 3D: start delay from distance to the closest listener (FUN_008369e0).
        let mut gains = (1.0f32, 1.0f32);
        if req.positional {
            if let Some(pos) = position {
                if let Some((idx, dist)) = self.listeners.closest(pos) {
                    req.start_delay = spatial::start_delay_secs(dist);
                    let (min_d, max_d) = self.cue_distances(&cue);
                    let atten = spatial::distance_attenuation(dist, min_d, max_d);
                    let listener = self.listeners.get(idx).copied().unwrap_or_default();
                    let (l, r) = spatial::stereo_pan(pos, &listener);
                    gains = (l * atten, r * atten);
                }
            }
        }

        // No explicit source: fire the cue's sounds the way the engine does (`pick_cue`), one voice
        // per fired sound, each starting at its sound's start time and resampled from the clip's
        // native rate to the mixer rate. A cue whose chain does not resolve (see [`ResolveError`]), or
        // for which the engine picks nothing, allocates one silent voice, as for a wave that has not
        // streamed in yet. Returns the first voice.
        if let Some(src) = source {
            let id = self.pool.acquire(&req)?;
            self.mixer.attach(id, src);
            self.mixer.set_channel_gains(id, gains.0, gains.1);
            return Some(id);
        }
        let dst_rate = self.mixer.config().sample_rate;
        let picked = self.pick_cue(&cue).unwrap_or_default();
        let mut first = None;
        for p in &picked {
            let Ok(clip) = self.clip(p.wave.wavebank, p.wave.index) else { continue };
            let src = Box::new(PcmSource::with_rate(
                clip.samples.clone(),
                clip.channels as usize,
                clip.sample_rate,
                dst_rate,
            ));
            let sound_req = VoiceRequest { start_delay: req.start_delay + p.start_s, ..req.clone() };
            let Some(id) = self.pool.acquire(&sound_req) else { continue };
            self.mixer.attach(id, src);
            self.mixer.set_channel_gains(id, gains.0, gains.1);
            first.get_or_insert(id);
        }
        if first.is_none() {
            let id = self.pool.acquire(&req)?;
            self.mixer.set_channel_gains(id, gains.0, gains.1);
            first = Some(id);
        }
        first
    }

    /// `Sound.CueSound` by cue *name* (hashes then [`cue_sound`](Self::cue_sound)).
    pub fn cue_sound_by_name(
        &mut self,
        name: &str,
        position: Option<Vec3>,
        source: Option<Box<dyn SampleSource>>,
    ) -> Option<VoiceId> {
        self.cue_sound(pandemic_hash_m2(name), position, source)
    }

    /// Min/max attenuation distances for a cue (from the cue record, or emitter defaults if zero).
    fn cue_distances(&self, cue: &CueEntry) -> (f32, f32) {
        let min_d = if cue.min_dist > 0.0 { cue.min_dist } else { 1.0 };
        let max_d = if cue.max_dist > 0.0 { cue.max_dist } else { 100.0 };
        (min_d, max_d)
    }

    /// `Sound.StopSound(voice)` — stop a voice (with a short fade).
    pub fn stop_sound(&mut self, id: VoiceId) {
        self.pool.stop(id, true);
    }

    /// `Sound.PauseSound(voice)` — pause a voice's playback.
    pub fn pause_sound(&mut self, id: VoiceId) {
        if let Some(v) = self.pool.get_mut(id) {
            v.state = crate::voice::InstanceState::Paused;
        }
    }

    /// `Sound.StopAndFlushAllSounds` — stop every voice.
    pub fn stop_and_flush_all_sounds(&mut self) {
        let ids: Vec<VoiceId> = self.pool.iter_active().map(|v| v.id).collect();
        for id in ids {
            self.pool.stop(id, false);
        }
    }

    // ---- Sound.* : categories --------------------------------------------------------------------

    /// `Sound.SetCategoryVolume(category, volume [, length])` (impl `FUN_00607960`).
    pub fn set_category_volume(&mut self, category: &str, volume: f32, length: f32) {
        self.categories
            .set_category_volume(category_id(category), volume, length);
    }
    /// `Sound.SetCategoryPitch(category, pitch [, length])`.
    pub fn set_category_pitch(&mut self, category: &str, pitch: f32, length: f32) {
        self.categories
            .set_category_pitch(category_id(category), pitch, length);
    }
    /// `Sound.GetCategoryVolume`.
    pub fn get_category_volume(&self, category: &str) -> f32 {
        self.categories.category_volume(category_id(category))
    }
    /// `Sound.GetCategoryPitch`.
    pub fn get_category_pitch(&self, category: &str) -> f32 {
        self.categories.category_pitch(category_id(category))
    }
    /// `Sound.FadeCategoryDown(category, level, length)`.
    pub fn fade_category_down(&mut self, category: &str, level: f32, length: f32) {
        self.categories
            .fade_category_down(category_id(category), level, length);
    }
    /// `Sound.FadeCategoryUp(category, level, length)`.
    pub fn fade_category_up(&mut self, category: &str, level: f32, length: f32) {
        self.categories
            .fade_category_up(category_id(category), level, length);
    }
    /// `Sound.SetMasterVolume(volume [, length])` (`FUN_0082f590` → StartMasterFade).
    pub fn set_master_volume(&mut self, volume: f32, length: f32) {
        self.categories.set_master_volume(volume, length);
    }
    /// `MrxSoundCategories.DuckMasterVolume(length)` — ref-counted master duck.
    pub fn duck_master_volume(&mut self, length: f32) {
        self.categories.duck_master(0.0, length);
    }
    /// `MrxSoundCategories.UnduckMasterVolume(length)`.
    pub fn unduck_master_volume(&mut self, length: f32) {
        self.categories.unduck_master(length);
    }

    // ---- Sound.* : music -------------------------------------------------------------------------

    /// `Sound.AddMusicState(name, p2..p6)` (`FUN_005fb460 → FUN_00600d30`).
    pub fn add_music_state(&mut self, name: &str, params: [f32; 5]) {
        self.music.add_music_state(name, params);
    }
    /// `Sound.AddMusicTransition(from, to)` (`FUN_005fb4b0 → FUN_00600df0`).
    pub fn add_music_transition(&mut self, from: &str, to: &str) {
        self.music.add_music_transition(from, to);
    }
    /// `Sound.BindMusicCue(state, index, cue)` (`FUN_00600eb0`).
    pub fn bind_music_cue(&mut self, state: &str, index: usize, cue: u32) {
        self.music.bind_music_cue(state, index, cue);
    }
    /// `Sound.TransitionMusic(state)` (`FUN_005e1600` → `FUN_0082d7a0`): start a crossfade.
    pub fn transition_music(&mut self, state: &str) -> bool {
        self.music.transition(state)
    }
    /// `Sound.SetDynamicMusic(enable)`.
    pub fn set_dynamic_music(&mut self, enable: bool) {
        self.music.set_dynamic(enable);
    }
    /// `Sound.IsDynamicMusic`.
    pub fn is_dynamic_music(&self) -> bool {
        self.music.is_dynamic()
    }

    // ---- Sound.* : banks -------------------------------------------------------------------------

    /// `Sound.LoadSoundBank(name [, callback])`.
    pub fn load_sound_bank(&mut self, name: &str, cb: Option<CallbackId>) -> bool {
        self.banks.load(name, BankKind::Sound, cb)
    }
    /// `Sound.LoadWaveBank(name [, callback])`.
    pub fn load_wave_bank(&mut self, name: &str, cb: Option<CallbackId>) -> bool {
        self.banks.load(name, BankKind::Wave, cb)
    }
    /// `Sound.LoadBankWithCallback(name, callback)` — generic; kind inferred by caller (defaults Sound).
    pub fn load_bank_with_callback(&mut self, name: &str, cb: CallbackId) -> bool {
        self.banks.load(name, BankKind::Sound, Some(cb))
    }
    /// `Sound.RequestAmbienceBank(name)`.
    pub fn request_ambience_bank(&mut self, name: &str) -> bool {
        self.banks.load(name, BankKind::Ambience, None)
    }
    /// `Sound.UnloadSoundBank` / `UnloadWaveBank` / `UnloadBankWithCallback`.
    pub fn unload_bank(&mut self, name: &str, cb: Option<CallbackId>) -> bool {
        self.banks.unload(name, cb)
    }
    /// Whether a bank is currently resident (`BankManager` slot).
    pub fn bank_is_loaded(&self, name: &str) -> bool {
        self.banks.is_loaded(name)
    }

    // ---- Sound.* : stream files & misc -----------------------------------------------------------

    /// `Sound.GetAudioDir` (`FUN_005e...`) — the audio content directory.
    pub fn get_audio_dir(&self) -> &str {
        &self.audio_dir
    }
    /// `Sound.OpenStreamFile(name)` (`FUN_005e4020` → `thunk_FUN_035f0000`).
    /// // CONFIRM-LIVE: stream open is SecuROM-thunked; here it records intent (the WAD-streaming system
    /// binds the actual `.pws` stream). Returns a stream handle id.
    pub fn open_stream_file(&mut self, _name: &str) -> u32 {
        0 // CONFIRM-LIVE: real handle comes from the stream I/O mgr (DAT_011763f4)
    }
    /// `Sound.CloseStreamFile(handle)` (`FUN_005e40d0` → `FUN_00606c00`).
    pub fn close_stream_file(&mut self, _handle: u32) {}

    /// `Sound.SetSurvivalMode(enable)`.
    pub fn set_survival_mode(&mut self, enable: bool) {
        self.survival = enable;
    }
    /// `Sound.SetSystemPause(paused)`.
    pub fn set_system_pause(&mut self, paused: bool) {
        self.paused = paused;
    }
    /// `Sound._GetLibVersion` → 12.0.
    pub fn get_lib_version(&self) -> f32 {
        SOUND_LIB_VERSION
    }

    /// The 9 retail `return 0` stubs (`FUN_006d5640`): `SetSourceEnterMusic`, `SetSourceExitMusic`,
    /// `SetSourceMusicTransition`-adjacent v11 remnants, `AddFadeCategory`, `ClearPitchCategories`,
    /// `AddPitchCategory`, `SetCinematicMode`, `_SummonEd`. Faithful no-ops (not "unimplemented").
    pub fn stub_return_zero(&self) -> i32 {
        0
    }

    // ---- VO.* ------------------------------------------------------------------------------------

    /// `VO.Cue(speaker, cue, priority)` (`FUN_005e9de0` → `thunk_FUN_028da000`).
    /// Arbitrates by priority; on accept, allocates a `vo`-category voice.
    pub fn vo_cue(
        &mut self,
        speaker: u32,
        cue: u32,
        priority: VoPriority,
        subtitles: bool,
        source: Option<Box<dyn SampleSource>>,
    ) -> bool {
        if !self.vo.cue(speaker, cue, priority, subtitles) {
            return false;
        }
        // Route the VO line through the voice pool in the `vo` category (high priority).
        let req = VoiceRequest {
            cue_guid: cue,
            priority: 200 + priority as u8,
            category: category_id("vo") as u8,
            gain: 1.0,
            looping: false,
            positional: false,
            start_delay: 0.0,
        };
        if let Some(id) = self.pool.acquire(&req) {
            self.vo.set_active_voice(id);
            if let Some(src) = source {
                self.mixer.attach(id, src);
            }
            self.mixer.set_channel_gains(id, 1.0, 1.0);
        }
        true
    }
    /// `VO.CueWithoutSubtitles`.
    pub fn vo_cue_without_subtitles(
        &mut self,
        speaker: u32,
        cue: u32,
        priority: VoPriority,
        source: Option<Box<dyn SampleSource>>,
    ) -> bool {
        self.vo_cue(speaker, cue, priority, false, source)
    }
    /// `VO.Cancel(cue)` (`FUN_005150d0`).
    pub fn vo_cancel(&mut self, cue: u32) {
        if let Some(v) = self.vo.cancel(cue) {
            self.pool.stop(v, false);
            self.mixer.detach(v);
        }
    }
    /// `VO.CancelAll`.
    pub fn vo_cancel_all(&mut self) {
        if let Some(v) = self.vo.cancel_all() {
            self.pool.stop(v, false);
            self.mixer.detach(v);
        }
    }
    /// `VO.Pause` / `VO.Unpause`.
    pub fn vo_set_paused(&mut self, paused: bool) {
        self.vo.set_paused(paused);
    }
    /// `VO.SetCinematicMode(enable)`.
    pub fn vo_set_cinematic_mode(&mut self, enable: bool) {
        self.vo.set_cinematic_mode(enable);
    }
    /// Whether a VO line is currently active (test/introspection seam).
    pub fn vo_is_active(&self) -> bool {
        self.vo.is_active()
    }
    /// The current VO cinematic-mode flag.
    pub fn vo_cinematic_mode(&self) -> bool {
        self.vo.cinematic_mode()
    }

    // ---- frame + mix -----------------------------------------------------------------------------

    /// The Pg update umbrella (`FUN_005fa950` + `FUN_006073c0`) once per sim tick: advance category
    /// fades, the music crossfade, the bank load machine, and the voice FSMs. Sample mixing runs on
    /// its own cadence via [`render_tick`](Self::render_tick).
    pub fn tick(&mut self, dt: f32) {
        self.categories.tick(dt);
        self.music.tick(dt);
        self.banks.tick();
        if !self.paused {
            self.pool.tick(dt);
        }
    }

    /// One mixer-thread tick (`FUN_00831ee0` / `FUN_00836610`): render one 45 ms block, submit it to
    /// the sink, and return it. Runs headless (sink = [`NullSink`]) or to a device.
    pub fn render_tick(&mut self) -> Vec<i16> {
        let frames = self.mixer.frames_per_tick();
        self.render(frames)
    }

    /// Render exactly `frames` frames into a fresh buffer (interleaved int16), submit to the sink, and
    /// return it. The category mixer supplies per-category gain (`master × category`).
    pub fn render(&mut self, frames: usize) -> Vec<i16> {
        let ch = self.mixer.config().channels;
        let mut out = vec![0i16; frames * ch];
        let cats = &self.categories;
        self.mixer
            .mix(&mut self.pool, &mut out, |cat| cats.effective_gain(cat));
        self.sink.submit(&out);
        out
    }
}

/// The engine seeds its generator from its tick counter at init; this engine seeds from the wall
/// clock, the same kind of per-run value. [`AudioEngine::set_rng_seed`] fixes it.
fn clock_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock reads before 1970")
        .as_nanos() as u32
}
