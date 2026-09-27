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
use crate::automation::{pitched_rate, AutomationError, AutomationOutput};
use crate::mixer::{Mixer, MixerConfig, PcmSource, SampleSource};
use crate::multitrack::{Automation, MultiTrackCue};
use crate::playback::{clamp01, Instance, InstanceParams, TrackPlayback};
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
    /// What the group gives a sound instance at start (base volume, pitch, delay).
    pub instance: InstanceParams,
    /// The group's `+0x2C` byte when multi-wave (non-zero on groups whose cues carry length −1).
    pub loop_byte: u8,
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
    /// The cue's `+0x08` gain.
    pub gain: f32,
    /// The multi-track body, for its automation, loop counts, parameters and play probability.
    pub multitrack: Option<MultiTrackCue>,
    /// Every sound of every track.
    pub sounds: Vec<ResolvedSound>,
}

impl ResolvedCue {
    /// Every wave the cue can reach, in track / sound / choice / wave order (duplicates kept).
    pub fn waves(&self) -> impl Iterator<Item = &ResolvedWave> {
        self.sounds.iter().flat_map(|s| s.choices.iter().flat_map(|c| c.waves.iter()))
    }
}

/// A started cue ([`AudioEngine::cue_sound`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CueHandle(pub u32);

/// Why a cue did not start.
#[derive(Clone, Debug, PartialEq)]
pub enum CueError {
    /// No sounddb entry matches the cue id.
    Unknown(u32),
    /// The chain does not resolve.
    Resolve(ResolveError),
    /// The cue loops (a cue, track or group loop count is set); looping is not played here.
    Looping { what: &'static str },
    /// The cue's automation cannot be played ([`crate::automation`]).
    Automation(AutomationError),
    /// The voice pool refused the voice (every voice outranks it).
    Outranked,
}

impl std::fmt::Display for CueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CueError::Unknown(id) => write!(f, "cue 0x{id:08X} is not in the sound database"),
            CueError::Resolve(e) => write!(f, "{e}"),
            CueError::Looping { what } => write!(f, "the cue loops ({what}); looping is not played"),
            CueError::Automation(e) => write!(f, "{e}"),
            CueError::Outranked => write!(f, "the voice pool refused the voice"),
        }
    }
}

impl std::error::Error for CueError {}

/// One started cue.
struct Playback {
    handle: CueHandle,
    resolved: ResolvedCue,
    /// Voice request template (priority, category, 3D).
    req: VoiceRequest,
    /// Spatial left/right gains, fixed at start.
    gains: (f32, f32),
    /// Cue-local parameter values supplied at start.
    params: HashMap<u32, f32>,
    /// An explicit-source cue: its one voice, left alone.
    direct: Option<VoiceId>,
    /// Single-track: whether its instance has started, and the instance.
    single: Option<Instance>,
    started: bool,
    /// A multi-track cue whose start draw exceeded its play probability: it plays nothing.
    dropped: bool,
    /// Multi-track state.
    cue_time: f32,
    cue_automation: crate::automation::AutomationState,
    cue_prev: (f32, f32),
    tracks: Vec<TrackPlayback>,
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
    /// Started cues, in start order.
    playbacks: Vec<Playback>,
    /// The next cue handle.
    next_handle: u32,
    /// Global parameter values (the Pal global table); an absent one reads −1.0, as in the engine.
    global_params: HashMap<u32, f32>,
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
            playbacks: Vec::new(),
            next_handle: 1,
            global_params: HashMap::new(),
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
        let multitrack = match &sb_cue.body {
            CueBody::MultiTrack(m) => Some(m.clone()),
            CueBody::SingleTrack { .. } => None,
        };
        Ok(ResolvedCue { soundbank: cue.bank_hash, cue_index: cue.cue_index, gain: sb_cue.gain, multitrack, sounds })
    }

    fn resolve_choice(&self, soundbank: u32, group_index: u16, weight: f32) -> Result<ResolvedChoice, ResolveError> {
        let bank = self.soundbanks.get(&soundbank).ok_or(ResolveError::SoundbankNotResident(soundbank))?;
        let group = bank.groups.get(group_index as usize).ok_or(ResolveError::GroupIndexOutOfRange {
            soundbank,
            index: group_index,
            groups: bank.groups.len(),
        })?;
        let (selection, instance, loop_byte) = match &group.form {
            GroupForm::Single { gain, unknown_30, .. } => {
                (None, InstanceParams::Single { volume: *gain, pitch: *unknown_30 }, 0)
            }
            GroupForm::Multi(m) => {
                if m.selection > 2 {
                    return Err(ResolveError::SelectionMode { soundbank, what: "group", mode: m.selection });
                }
                let f = m.floats_4c;
                (
                    Some(m.selection),
                    InstanceParams::Multi {
                        pitch_lo: f[4],
                        pitch_hi: f[5],
                        volume_lo: f[1],
                        volume_hi: f[2],
                        delay_a: m.unknown_3c,
                        delay_b: m.unknown_40,
                    },
                    m.byte_2c,
                )
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
        Ok(ResolvedChoice { weight, soundbank, group_index, selection, waves, instance, loop_byte })
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
    /// the observable result: resolve the cue through every path, refuse what cannot be played
    /// ([`CueError`]), and start a playback ([`crate::playback`]) whose sounds fire, pick their waves
    /// and follow their automation on each [`tick`](Self::tick). If `position` is given and the cue is
    /// positional, 3D channel gains and a distance start delay are computed against the closest
    /// listener at start. Cue-local curve parameters are given by [`cue_sound_with_params`].
    ///
    /// [`cue_sound_with_params`]: Self::cue_sound_with_params
    pub fn cue_sound(&mut self, cue_id: u32, position: Option<Vec3>) -> Result<CueHandle, CueError> {
        self.cue_sound_with_params(cue_id, position, &[])
    }

    /// [`cue_sound`](Self::cue_sound) with values for the cue's own curve parameters.
    pub fn cue_sound_with_params(
        &mut self,
        cue_id: u32,
        position: Option<Vec3>,
        params: &[(u32, f32)],
    ) -> Result<CueHandle, CueError> {
        let cue: CueEntry = *self.sounddb.find_cue(cue_id).ok_or(CueError::Unknown(cue_id))?;
        let resolved = self.resolve_cue(&cue).map_err(CueError::Resolve)?;
        let params: HashMap<u32, f32> = params.iter().copied().collect();
        self.check_playable(&resolved, &params)?;
        let (req, gains) = self.voice_template(&cue, position);
        let handle = self.new_handle();
        let tracks = resolved.multitrack.as_ref().map(|m| vec![TrackPlayback::new(); m.tracks.len()]).unwrap_or_default();
        let mut pb = Playback {
            handle,
            resolved,
            req,
            gains,
            params,
            direct: None,
            single: None,
            started: false,
            dropped: false,
            cue_time: 0.0,
            cue_automation: Default::default(),
            cue_prev: (1.0, 0.0),
            tracks,
        };
        // FUN_008354e0: a multi-track cue draws once and plays only if its +0x18 value is not below.
        if let Some(m) = &pb.resolved.multitrack {
            let r = self.rng.next_unit();
            pb.dropped = m.unknown_18 < r;
        }
        self.playbacks.push(pb);
        Ok(handle)
    }

    /// Play an explicit sample source for a cue (tests, synthesized audio): one voice, no automation.
    pub fn cue_sound_with_source(
        &mut self,
        cue_id: u32,
        position: Option<Vec3>,
        source: Box<dyn SampleSource>,
    ) -> Result<CueHandle, CueError> {
        let cue: CueEntry = *self.sounddb.find_cue(cue_id).ok_or(CueError::Unknown(cue_id))?;
        let (req, gains) = self.voice_template(&cue, position);
        let id = self.pool.acquire(&req).ok_or(CueError::Outranked)?;
        self.mixer.attach(id, source);
        self.mixer.set_channel_gains(id, gains.0, gains.1);
        let handle = self.new_handle();
        self.playbacks.push(Playback {
            handle,
            resolved: ResolvedCue { soundbank: 0, cue_index: 0, gain: 1.0, multitrack: None, sounds: Vec::new() },
            req,
            gains,
            params: HashMap::new(),
            direct: Some(id),
            single: None,
            started: true,
            dropped: false,
            cue_time: 0.0,
            cue_automation: Default::default(),
            cue_prev: (1.0, 0.0),
            tracks: Vec::new(),
        });
        Ok(handle)
    }

    /// `Sound.CueSound` by cue *name* (hashes then [`cue_sound`](Self::cue_sound)).
    pub fn cue_sound_by_name(&mut self, name: &str, position: Option<Vec3>) -> Result<CueHandle, CueError> {
        self.cue_sound(pandemic_hash_m2(name), position)
    }

    /// The sound instances a started cue has fired, with their current base values.
    pub fn cue_instances(&self, handle: CueHandle) -> Vec<Instance> {
        let Some(pb) = self.playbacks.iter().find(|p| p.handle == handle) else { return Vec::new() };
        pb.single.into_iter().chain(pb.tracks.iter().flat_map(|t| t.instances.iter().copied())).collect()
    }

    /// The voices a started cue currently owns.
    pub fn cue_voices(&self, handle: CueHandle) -> Vec<VoiceId> {
        let Some(pb) = self.playbacks.iter().find(|p| p.handle == handle) else { return Vec::new() };
        pb.direct
            .into_iter()
            .chain(pb.single.map(|i| i.voice))
            .chain(pb.tracks.iter().flat_map(|t| t.instances.iter().map(|i| i.voice)))
            .collect()
    }

    /// Set a global parameter (the Pal global table the curves fall back to). Refused when a playing
    /// cue has a curve over it that the value lies past.
    pub fn set_global_param(&mut self, param: u32, value: f32) -> Result<(), CueError> {
        for pb in &self.playbacks {
            if let Some(m) = &pb.resolved.multitrack {
                if !m.params.contains(&param) {
                    check_curves(m, param, value)?;
                }
            }
        }
        self.global_params.insert(param, value);
        Ok(())
    }

    fn new_handle(&mut self) -> CueHandle {
        let h = CueHandle(self.next_handle);
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        h
    }

    /// The voice request and spatial gains a cue's voices share.
    fn voice_template(&self, cue: &CueEntry, position: Option<Vec3>) -> (VoiceRequest, (f32, f32)) {
        let mut req = VoiceRequest {
            cue_guid: cue.guid,
            priority: cue.priority,
            category: cue.category,
            gain: 1.0,
            looping: cue.is_looping(),
            positional: cue.is_positional() && position.is_some(),
            start_delay: 0.0,
        };
        let mut gains = (1.0f32, 1.0f32);
        if req.positional {
            if let Some(pos) = position {
                if let Some((idx, dist)) = self.listeners.closest(pos) {
                    req.start_delay = spatial::start_delay_secs(dist);
                    let (min_d, max_d) = self.cue_distances(cue);
                    let atten = spatial::distance_attenuation(dist, min_d, max_d);
                    let listener = self.listeners.get(idx).copied().unwrap_or_default();
                    let (l, r) = spatial::stereo_pan(pos, &listener);
                    gains = (l * atten, r * atten);
                }
            }
        }
        (req, gains)
    }

    /// Refuse a cue this engine cannot play faithfully: loops, automation kinds with no counterpart,
    /// unset cue-local parameters, and parameters past a curve.
    fn check_playable(&self, resolved: &ResolvedCue, params: &HashMap<u32, f32>) -> Result<(), CueError> {
        if resolved.sounds.iter().any(|s| s.choices.iter().any(|c| c.loop_byte != 0)) {
            return Err(CueError::Looping { what: "a group's +0x2C loop byte" });
        }
        let Some(m) = &resolved.multitrack else { return Ok(()) };
        if m.byte_10 != 0 {
            return Err(CueError::Looping { what: "the cue's +0x10 loop count" });
        }
        if m.tracks.iter().any(|t| t.byte_00 != 0) {
            return Err(CueError::Looping { what: "a track's +0x00 loop count" });
        }
        for a in m.events.iter().chain(m.tracks.iter().flat_map(|t| t.automation.iter())) {
            let kind = match a {
                Automation::Kind4 { .. } => Some(4),
                Automation::Kind7 { .. } => Some(7),
                Automation::Kind9 { .. } => Some(9),
                Automation::Curve { kind: crate::multitrack::CurveKind::Cue, .. } => Some(8),
                _ => None,
            };
            if let Some(kind) = kind {
                return Err(CueError::Automation(AutomationError::Unsupported { kind }));
            }
            if let Automation::Curve { param, .. } = a {
                let value = param_value(m, params, &self.global_params, *param).map_err(CueError::Automation)?;
                check_curves(m, *param, value)?;
            }
        }
        Ok(())
    }

    /// Min/max attenuation distances for a cue (from the cue record, or emitter defaults if zero).
    fn cue_distances(&self, cue: &CueEntry) -> (f32, f32) {
        let min_d = if cue.min_dist > 0.0 { cue.min_dist } else { 1.0 };
        let max_d = if cue.max_dist > 0.0 { cue.max_dist } else { 100.0 };
        (min_d, max_d)
    }

    /// `Sound.StopSound(cue)` — stop a started cue: its voices fade out and nothing more fires.
    pub fn stop_sound(&mut self, handle: CueHandle) {
        for id in self.cue_voices(handle) {
            self.pool.stop(id, true);
        }
        self.playbacks.retain(|p| p.handle != handle);
    }

    /// `Sound.PauseSound(cue)` — pause a started cue's voices.
    pub fn pause_sound(&mut self, handle: CueHandle) {
        for id in self.cue_voices(handle) {
            if let Some(v) = self.pool.get_mut(id) {
                v.state = crate::voice::InstanceState::Paused;
            }
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
            self.advance_playbacks(dt);
            self.pool.tick(dt);
        }
    }

    // ---- cue playback (FUN_00835060 / FUN_0083c070 / FUN_0083fee0 / FUN_008369e0) ----------------

    fn advance_playbacks(&mut self, dt: f32) {
        let mut pbs = std::mem::take(&mut self.playbacks);
        for pb in &mut pbs {
            self.advance(pb, dt);
        }
        pbs.retain(|pb| !self.finished(pb));
        self.playbacks = pbs;
    }

    /// A playback is over once everything has fired and no voice of it is left.
    fn finished(&self, pb: &Playback) -> bool {
        if pb.dropped {
            return true;
        }
        let live = |id: VoiceId| self.pool.get(id).is_some_and(|v| !v.state.is_terminal()) && self.mixer.is_attached(id);
        if let Some(id) = pb.direct {
            return !live(id);
        }
        if pb.resolved.multitrack.is_none() {
            return pb.started && pb.single.is_none_or(|i| !live(i.voice));
        }
        let sound_counts: Vec<usize> = pb
            .resolved
            .multitrack
            .as_ref()
            .map(|m| m.tracks.iter().map(|t| t.sounds.len()).collect())
            .unwrap_or_default();
        pb.tracks.iter().enumerate().all(|(t, tr)| {
            tr.next_sound >= sound_counts.get(t).copied().unwrap_or(0) && tr.instances.iter().all(|i| !live(i.voice))
        })
    }

    fn advance(&mut self, pb: &mut Playback, dt: f32) {
        if pb.direct.is_some() || pb.dropped {
            return;
        }
        let Some(m) = pb.resolved.multitrack.clone() else {
            // Single-track: start the one instance on the first frame, then hold its parameters.
            let volume = clamp01(pb.resolved.gain * 1.0);
            if !pb.started {
                pb.started = true;
                pb.single = self.fire(pb, 0, 0, None);
            }
            if let Some(inst) = pb.single {
                self.apply(inst, volume, 0.0);
            }
            return;
        };
        pb.started = true;
        // 1. cue parameters from the previous frame's cue automation.
        let cue_volume = clamp01(pb.resolved.gain * pb.cue_prev.0);
        let cue_pitch = pb.cue_prev.1;
        // 2. cue time and cue automation.
        pb.cue_time += dt;
        let params = pb.params.clone();
        let globals = self.global_params.clone();
        let value = |p: u32| param_value(&m, &params, &globals, p);
        let cue_out = pb
            .cue_automation
            .step(&m.events, pb.cue_time, &value)
            .expect("automation was validated when the cue started");
        pb.cue_prev = (cue_out.volume, cue_out.pitch);
        let cue_override = override_of(&cue_out);
        // 3. tracks.
        let mut sound_base = 0usize;
        for (t, track) in m.tracks.iter().enumerate() {
            let tp = &mut pb.tracks[t];
            tp.time += dt;
            let out = tp
                .automation
                .step(&track.automation, tp.time, &value)
                .expect("automation was validated when the cue started");
            let volume = out.volume * cue_volume;
            let pitch = out.pitch + cue_pitch;
            let over = cue_override.or(override_of(&out));
            let time = tp.time;
            let mut fired = Vec::new();
            for (k, s) in track.sounds.iter().enumerate().skip(tp.next_sound) {
                if s.start_s <= time {
                    fired.push(k);
                    tp.next_sound = k + 1;
                }
            }
            for k in fired {
                if let Some(inst) = self.fire(pb, sound_base + k, t, over) {
                    pb.tracks[t].instances.push(inst);
                }
            }
            let tp = &mut pb.tracks[t];
            if let Some((ov, op)) = over {
                for inst in &mut tp.instances {
                    inst.volume = ov;
                    inst.pitch = op;
                }
            }
            let instances = tp.instances.clone();
            for inst in instances {
                self.apply(inst, volume, pitch);
            }
            sound_base += track.sounds.len();
        }
    }

    /// Fire sound `s` of the resolved cue: pick its entry and the entry group's wave, draw the
    /// instance's start values, and start a voice.
    fn fire(&mut self, pb: &Playback, s: usize, _track: usize, over: Option<(f32, f32)>) -> Option<Instance> {
        let sound = &pb.resolved.sounds[s];
        let choice = match sound.selection {
            None => Some(0),
            Some((mode, slot)) => {
                let weights: Vec<f32> = sound.choices.iter().map(|c| c.weight).collect();
                let st = self.slot_state.entry((pb.resolved.soundbank, pb.resolved.cue_index, slot)).or_insert(STATE_INIT);
                select::pick(mode, &weights, st, &mut self.rng)
            }
        };
        let choice = choice.and_then(|i| sound.choices.get(i))?;
        let wave = match choice.selection {
            None => Some(0),
            Some(mode) => {
                let weights: Vec<f32> = choice.waves.iter().map(|w| w.weight).collect();
                let st = self.group_state.entry((choice.soundbank, choice.group_index)).or_insert(STATE_INIT);
                select::pick(mode, &weights, st, &mut self.rng)
            }
        };
        let wave = *wave.and_then(|i| choice.waves.get(i))?;
        let start = choice.instance.start(&mut self.rng);
        let clip = self.clip(wave.wavebank, wave.index).expect("resolved when the cue started");
        let (samples, channels, clip_rate) = (clip.samples.clone(), clip.channels as usize, clip.sample_rate);
        let src = PcmSource::with_rate(samples, channels, clip_rate, self.mixer.config().sample_rate);
        let req = VoiceRequest { start_delay: pb.req.start_delay + start.delay_s, ..pb.req.clone() };
        let id = self.pool.acquire(&req)?;
        self.mixer.attach_pcm(id, src);
        self.mixer.set_channel_gains(id, pb.gains.0, pb.gains.1);
        let (volume, pitch) = over.unwrap_or((start.volume, start.pitch));
        Some(Instance { voice: id, volume, pitch, clip_rate, wavebank: wave.wavebank, wave_index: wave.index })
    }

    /// Give an instance's voice its volume and pitched rate.
    fn apply(&mut self, inst: Instance, volume: f32, pitch: f32) {
        if !self.mixer.is_attached(inst.voice) {
            return; // the voice finished or was stolen
        }
        if let Some(v) = self.pool.get_mut(inst.voice) {
            v.gain = inst.volume * volume;
        }
        let rate = pitched_rate(inst.clip_rate, inst.pitch + pitch);
        self.mixer
            .set_source_rate(inst.voice, rate)
            .expect("playback voices carry PCM sources built at the mixer rate");
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

/// The override block of an automation step, when a non-zero-mode ramp is active.
fn override_of(out: &AutomationOutput) -> Option<(f32, f32)> {
    out.override_active.then_some((out.override_volume, out.override_pitch))
}

/// Refuse a value that lies past the last point of a curve over `param` in `m`.
fn check_curves(m: &MultiTrackCue, param: u32, value: f32) -> Result<(), CueError> {
    for a in m.events.iter().chain(m.tracks.iter().flat_map(|t| t.automation.iter())) {
        if let Automation::Curve { param: p, points, .. } = a {
            if *p != param {
                continue;
            }
            crate::automation::curve(points, param, value).map_err(CueError::Automation)?;
        }
    }
    Ok(())
}

/// A curve parameter's value: a cue-local one (in the cue's parameter list) from the values given at
/// start, else the global table, −1.0 when absent (`FUN_008349d0`, `FUN_0082f170`).
fn param_value(
    m: &MultiTrackCue,
    local: &HashMap<u32, f32>,
    globals: &HashMap<u32, f32>,
    param: u32,
) -> Result<f32, AutomationError> {
    if m.params.contains(&param) {
        local.get(&param).copied().ok_or(AutomationError::ParameterUnset { param })
    } else {
        Ok(globals.get(&param).copied().unwrap_or(-1.0))
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;
    use crate::encode::{build_general, CueBodySpec, CueDef, GroupFormSpec, GroupHeadParams, GroupSpec, Pcm16, TablesSpec, WaveSpec};
    use crate::multitrack::{Sound, SoundEntry, Target, Track};
    use mercs2_formats::hash::pandemic_hash_m2 as m2;

    fn engine_with(tracks: Vec<Track>, events: Vec<Automation>, gain: f32) -> (AudioEngine, u32) {
        let bank = m2("mod_auto");
        let head = GroupHeadParams {
            unknown_10: 1.0,
            unknown_14: 0,
            min_distance: 10.0,
            max_distance: 100.0,
            unknown_20: 1.0,
            pitch: 1.0,
            unknown_28: 1.0,
        };
        let tracks = tracks
            .into_iter()
            .map(|mut t| {
                for s in &mut t.sounds {
                    for e in &mut s.entries {
                        e.soundbank = bank;
                    }
                }
                t
            })
            .collect();
        let spec = TablesSpec {
            name: "mod_auto".into(),
            waves: vec![WaveSpec { clip_hash: 1, pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![1000; 22050 * 4] } }],
            groups: vec![GroupSpec {
                sound_id: 1,
                category: "sfx".into(),
                head,
                form: GroupFormSpec::Single { wave: 0, gain: 0.5, unknown_30: 2.0, weight: 1.0 },
            }],
            cues: vec![CueDef {
                name: "mod_auto_cue".into(),
                byte_06: 0,
                gain,
                body: CueBodySpec::MultiTrack(MultiTrackCue {
                    byte_10: 0,
                    sound_slots: 1,
                    unknown_18: 1.0,
                    unknown_1c: -1.0,
                    unknown_20: -1.0,
                    unknown_24: 0.0,
                    events,
                    curves: vec![],
                    tracks,
                    params: vec![],
                }),
            }],
        };
        let enc = build_general(&spec).unwrap().to_bytes().unwrap();
        let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 2 });
        eng.set_rng_seed(7);
        eng.set_sounddb(SoundDb::parse(&enc.sounddb).unwrap());
        eng.load_soundbank(&enc.soundbank).unwrap();
        eng.load_wavebank(&enc.wavebank).unwrap();
        (eng, m2("mod_auto_cue"))
    }

    fn one_sound_track(automation: Vec<Automation>) -> Track {
        Track {
            byte_00: 0,
            unknown_04: -1.0,
            unknown_08: -1.0,
            automation,
            sounds: vec![Sound {
                slot: 0,
                byte_01: 2,
                byte_02: 2,
                selection: 1,
                start_s: 0.0,
                entries: vec![SoundEntry { soundbank: 0, group_index: 0, unknown_06: 0, weight: 1.0 }],
            }],
        }
    }

    /// A track volume fade and pitch ramp reach the voice: gain = base volume (group +0x2C) × ramp ×
    /// clamp01(cue gain); rate = pitched_rate(clip rate, base pitch (+0x30) + ramp).
    #[test]
    fn track_automation_drives_the_voice() {
        let fade = Automation::Ramp { target: Target::Volume, start_s: 0.0, mode: 0, unknown_0c: 0, duration_s: 1.0, from: 1.0, to: 0.0 };
        let bend = Automation::Ramp { target: Target::Pitch, start_s: 0.0, mode: 0, unknown_0c: 0, duration_s: 1.0, from: 0.0, to: 12.0 };
        let (mut eng, cue) = engine_with(vec![one_sound_track(vec![fade, bend])], vec![], 0.8);
        let h = eng.cue_sound(cue, None).unwrap();
        for _ in 0..5 {
            eng.tick(0.1);
        }
        let t = 0.1f32 + 0.1 + 0.1 + 0.1 + 0.1;
        let inst = eng.cue_instances(h)[0];
        let v = eng.pool.get(inst.voice).unwrap().gain;
        let ramp_v = (0.0f32 - 1.0) / ((1.0 + 0.0) - 0.0) * (t - 0.0) + 1.0;
        assert_eq!(v, 0.5 * (ramp_v * 0.8f32.min(1.0)));
        let ramp_p = (12.0f32 - 0.0) / ((1.0 + 0.0) - 0.0) * (t - 0.0);
        let rate = pitched_rate(22050, 2.0 + ((ramp_p + 0.0) + 0.0));
        assert_eq!(eng.mixer.source_step(inst.voice).unwrap(), rate as f64 / 44100.0);
    }

    /// The cue's event table acts one frame late, through the clamped cue volume.
    #[test]
    fn cue_events_lag_a_frame() {
        let fade = Automation::Ramp { target: Target::Volume, start_s: 0.0, mode: 0, unknown_0c: 0, duration_s: 1.0, from: 0.5, to: 0.5 };
        let (mut eng, cue) = engine_with(vec![one_sound_track(vec![])], vec![fade], 1.0);
        let h = eng.cue_sound(cue, None).unwrap();
        eng.tick(0.1);
        let inst = eng.cue_instances(h)[0];
        assert_eq!(eng.pool.get(inst.voice).unwrap().gain, 0.5 * 1.0, "frame 1 still uses the start value");
        eng.tick(0.1);
        assert_eq!(eng.pool.get(inst.voice).unwrap().gain, 0.5 * 0.5, "frame 2 sees frame 1's event");
    }

    #[test]
    fn unsupported_automation_and_loops_refuse_the_cue() {
        let (mut eng, cue) = engine_with(vec![one_sound_track(vec![Automation::Kind7 { unknown_04: 0, hash: 1 }])], vec![], 1.0);
        assert_eq!(eng.cue_sound(cue, None), Err(CueError::Automation(AutomationError::Unsupported { kind: 7 })));
        let mut looping = one_sound_track(vec![]);
        looping.byte_00 = 0xFF;
        let (mut eng, cue) = engine_with(vec![looping], vec![], 1.0);
        assert!(matches!(eng.cue_sound(cue, None), Err(CueError::Looping { .. })));
    }
}
