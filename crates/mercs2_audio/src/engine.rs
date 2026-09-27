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

use std::collections::{HashMap, HashSet};

use mercs2_core::glam::Vec3;
use mercs2_formats::hash::pandemic_hash_m2;

use crate::backend::{AudioSink, NullSink};
#[cfg(feature = "device")]
use crate::backend::CpalSink;
use crate::banks::{BankKind, BankManager, CallbackId};
use crate::categories::{category_id, Categories};
use crate::automation::{pitched_rate, AutomationError, AutomationOutput, AutomationState};
use crate::mixer::{Mixer, MixerConfig, PcmSource, SampleSource, SourceKey, Wave3d};
use crate::multitrack::{Automation, MultiTrackCue};
use crate::playback::{
    clamp01, Block, Instance, InstanceParams, InstanceWave, ListState, RunState, TrackPlayback,
};
use crate::music::MusicStateMachine;
use crate::sounddb::{CueEntry, SoundDb};
use crate::select::{self, PalRng, STATE_INIT};
use crate::soundbank::{CueBody, GroupForm, Soundbank, SoundbankError};
use crate::spatial::{ListenerSet, Listener};
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
    /// The group's `+0x2C` wave loop count when multi-wave (0 for a single-wave group).
    pub loop_byte: u8,
    /// The group's `+0x14` byte is set: with an emitter, its instances play from the emitter's own
    /// (3D) source; otherwise from the shared 2D source (`FUN_00837830`, `0x008378A9`).
    pub positional: bool,
    /// The group's 3D parameters, which a positional instance's wave takes (`0x00837C08`).
    pub wave3d: Wave3d,
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
    /// The cue's automation cannot be played ([`crate::automation`]).
    Automation(AutomationError),
    /// The cue has more curves than events: `FUN_00839db0`, looking for a kind-9 event to give each
    /// wave its filter, scans the first `curves` event slots, so it would read past the event table.
    FilterScan {
        /// The cue's event count.
        events: usize,
        /// The cue's curve count.
        curves: usize,
    },
    /// A child cue a kind-7 record names cannot be played.
    Child {
        /// The child cue's guid.
        cue: u32,
        /// Why.
        error: Box<CueError>,
    },
    /// The voice pool refused the voice (every voice outranks it).
    Outranked,
}

impl std::fmt::Display for CueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CueError::Unknown(id) => write!(f, "cue 0x{id:08X} is not in the sound database"),
            CueError::Resolve(e) => write!(f, "{e}"),
            CueError::Automation(e) => write!(f, "{e}"),
            CueError::FilterScan { events, curves } => write!(
                f,
                "the cue has {curves} curves but {events} events; FUN_00839db0's filter scan would read past the event table"
            ),
            CueError::Child { cue, error } => write!(f, "child cue 0x{cue:08X}: {error}"),
            CueError::Outranked => write!(f, "the voice pool refused the voice"),
        }
    }
}

impl std::error::Error for CueError {}

/// One started cue (the engine's cue instance).
struct Playback {
    handle: CueHandle,
    resolved: ResolvedCue,
    /// Voice request template (priority, category, 3D).
    req: VoiceRequest,
    /// Where it was started (a child cue starts at its parent's position).
    position: Option<Vec3>,
    /// Cue-local parameter values supplied at start.
    params: HashMap<u32, f32>,
    /// An explicit-source cue: its one voice, left alone.
    direct: Option<VoiceId>,
    /// The run state (`+0x164`).
    state: RunState,
    /// Single-track: the instance slot state (`+0x08`: 0 start, 1 releasing, 2 done) and instance.
    single_state: u8,
    single: Option<Instance>,
    /// Multi-track: loop count left (`+0x15D`), time (`+0x154`), event-table automation and its
    /// latest output (`+0x3C`..`+0x9D`), tracks, and the child cue (`+0x150`).
    loop_count: u8,
    cue_time: f32,
    cue_automation: AutomationState,
    cue_last: AutomationOutput,
    tracks: Vec<TrackPlayback>,
    child: Option<CueHandle>,
    /// The cue (and track, for a track's child) that started it.
    parent: Option<(CueHandle, Option<usize>)>,
    /// The emitter its positional instances mix through: its own for a cue the game starts, its
    /// parent's for a child cue (`FUN_0082e930` passes the parent's `+0x118` / `+0xC8`).
    emitter: u32,
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
        let positional = group.head.unknown_14 & 0xFF != 0;
        let wave3d = Wave3d {
            min_distance: group.head.min_distance,
            max_distance: group.head.max_distance,
            exponent: group.head.distance_exponent,
            doppler_scale: group.head.doppler_scale,
        };
        Ok(ResolvedChoice { weight, soundbank, group_index, selection, waves, instance, loop_byte, positional, wave3d })
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
        // FUN_00835b80: every parameter the global catalog declares gets an entry, 0.0 until set
        // (the entry constructor at 0x008335A0); an undeclared one reads −1.0 (FUN_0082f170).
        for p in &db.params {
            self.global_params.entry(*p).or_insert(0.0);
        }
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
    /// and follow their automation on each [`tick`](Self::tick). If `position` is given, the cue's
    /// emitter sits there and its positional instances mix through the emitter's source (speaker
    /// gains, distance volume and Doppler against listener 0, [`crate::spatial`]); the cue API
    /// carries no velocity, so the emitter is at rest. Cue-local curve parameters are given by
    /// [`cue_sound_with_params`].
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
        self.check_playable(&resolved, &params, &mut HashSet::from([cue_id]))?;
        Ok(self.start_playback(&cue, resolved, position, params, None))
    }

    /// Allocate and play a resolved, playable cue (`FUN_00834ad0` then `FUN_008354e0`). A multi-track
    /// cue draws once and, if its `+0x18` value is below the draw, is done at once.
    fn start_playback(
        &mut self,
        cue: &CueEntry,
        resolved: ResolvedCue,
        position: Option<Vec3>,
        params: HashMap<u32, f32>,
        parent: Option<(CueHandle, Option<usize>, u32)>,
    ) -> CueHandle {
        let req = self.voice_template(cue, position);
        let handle = self.new_handle();
        let emitter = parent.map_or(handle.0, |p| p.2);
        let parent = parent.map(|(h, t, _)| (h, t));
        let (tracks, loop_count) = match &resolved.multitrack {
            Some(m) => (m.tracks.iter().map(|t| TrackPlayback::new(t.byte_00)).collect(), m.byte_10),
            None => (Vec::new(), 0),
        };
        let mut pb = Playback {
            handle,
            resolved,
            req,
            position,
            params,
            direct: None,
            state: RunState::Playing,
            single_state: 0,
            single: None,
            loop_count,
            cue_time: 0.0,
            cue_automation: AutomationState::default(),
            cue_last: AutomationOutput::default(),
            tracks,
            child: None,
            parent,
            emitter,
        };
        if let Some(m) = &pb.resolved.multitrack {
            let r = self.rng.next_unit();
            if m.unknown_18 < r {
                pb.state = RunState::Done;
            }
        }
        self.playbacks.push(pb);
        handle
    }

    /// Start a kind-7 child cue (`FUN_0082e930`): `None` when the engine's start fails — the cue is
    /// not in the sound database or its soundbank is not loaded (`FUN_00834ad0` returns 0) — which
    /// the parent treats as a finished child. The child plays at its parent's position, through its
    /// parent's emitter (`emitter`, the parent's [`Playback::emitter`]).
    ///
    /// `FUN_0082e930` calls the Pal cue start `FUN_0082e960` (Ghidra's `thunk_FUN_024b9220`: its first
    /// instruction is a SecuROM splice, which emulating the runtime dump shows to be
    /// `mov ecx, [0x01176400]`; the rest is plain `.text`) with the parent's emitter (`+0x118` /
    /// `+0xC8`, which is the parent's own start argument 3) and `0, 0` for the instance flags `+0x82` /
    /// `+0x83`. The allocation and play it does are the ones [`cue_sound`](Self::cue_sound) models.
    fn start_child(
        &mut self,
        guid: u32,
        position: Option<Vec3>,
        parent: (CueHandle, Option<usize>),
        emitter: u32,
    ) -> Option<CueHandle> {
        let cue = *self.sounddb.find_cue(guid)?;
        let resolved = match self.resolve_cue(&cue) {
            Ok(r) => r,
            Err(ResolveError::SoundbankNotResident(_)) => return None,
            Err(e) => panic!("child cue 0x{guid:08X} was checked playable when its parent started: {e}"),
        };
        Some(self.start_playback(&cue, resolved, position, HashMap::new(), Some((parent.0, parent.1, emitter))))
    }

    /// Play an explicit sample source for a cue (tests, synthesized audio): one voice, no automation.
    pub fn cue_sound_with_source(
        &mut self,
        cue_id: u32,
        position: Option<Vec3>,
        source: Box<dyn SampleSource>,
    ) -> Result<CueHandle, CueError> {
        let cue: CueEntry = *self.sounddb.find_cue(cue_id).ok_or(CueError::Unknown(cue_id))?;
        let req = self.voice_template(&cue, position);
        let id = self.pool.acquire(&req).ok_or(CueError::Outranked)?;
        self.mixer.attach(id, source);
        let handle = self.new_handle();
        if let Some(pos) = position {
            // A positional cue's voices are mixed through its emitter's source. An explicit source
            // has no group, so no 3D parameters: speaker gains apply, no distance volume or Doppler.
            self.mixer.set_emitter(handle.0, pos, Vec3::ZERO);
            self.mixer.set_source(id, SourceKey::Emitter(handle.0));
        }
        self.playbacks.push(Playback {
            handle,
            resolved: ResolvedCue { soundbank: 0, cue_index: 0, gain: 1.0, multitrack: None, sounds: Vec::new() },
            req,
            position,
            params: HashMap::new(),
            direct: Some(id),
            state: RunState::Playing,
            single_state: 0,
            single: None,
            loop_count: 0,
            cue_time: 0.0,
            cue_automation: AutomationState::default(),
            cue_last: AutomationOutput::default(),
            tracks: Vec::new(),
            child: None,
            parent: None,
            emitter: handle.0,
        });
        Ok(handle)
    }

    /// `Sound.CueSound` by cue *name* (hashes then [`cue_sound`](Self::cue_sound)).
    pub fn cue_sound_by_name(&mut self, name: &str, position: Option<Vec3>) -> Result<CueHandle, CueError> {
        self.cue_sound(pandemic_hash_m2(name), position)
    }

    /// The live sound instances a started cue holds, with their current base values.
    pub fn cue_instances(&self, handle: CueHandle) -> Vec<Instance> {
        let Some(pb) = self.playbacks.iter().find(|p| p.handle == handle) else { return Vec::new() };
        pb.single.into_iter().chain(pb.tracks.iter().flat_map(|t| t.sounds.instances.iter().copied())).collect()
    }

    /// The voices a started cue currently owns.
    pub fn cue_voices(&self, handle: CueHandle) -> Vec<VoiceId> {
        let Some(pb) = self.playbacks.iter().find(|p| p.handle == handle) else { return Vec::new() };
        pb.direct
            .into_iter()
            .chain(pb.single.and_then(|i| i.voice))
            .chain(pb.tracks.iter().flat_map(|t| t.sounds.instances.iter().filter_map(|i| i.voice)))
            .collect()
    }

    /// Whether a started cue is still playing (it has not reached its done state).
    pub fn cue_is_playing(&self, handle: CueHandle) -> bool {
        self.playbacks.iter().any(|p| p.handle == handle && p.state != RunState::Done)
    }

    /// The child cues a started cue has running: its own (kind 7 in its event table) and its tracks'.
    pub fn cue_children(&self, handle: CueHandle) -> Vec<CueHandle> {
        let Some(pb) = self.playbacks.iter().find(|p| p.handle == handle) else { return Vec::new() };
        pb.child.into_iter().chain(pb.tracks.iter().filter_map(|t| t.child)).collect()
    }

    /// Set a global parameter (the Pal global table the curves fall back to). Refused when a playing
    /// cue has a curve over it that the value lies past.
    pub fn set_global_param(&mut self, param: u32, value: f32) -> Result<(), CueError> {
        let mut bodies = Vec::new();
        let mut visited = HashSet::new();
        for pb in &self.playbacks {
            if let Some(m) = &pb.resolved.multitrack {
                self.with_children(m, &mut visited, &mut bodies);
            }
        }
        for m in &bodies {
            if !m.params.contains(&param) {
                check_curves(m, param, value)?;
            }
        }
        self.global_params.insert(param, value);
        Ok(())
    }

    /// `m` and every multi-track body its kind-7 children can reach (children a playing cue may still
    /// start read the global parameters too).
    fn with_children(&self, m: &MultiTrackCue, visited: &mut HashSet<u32>, out: &mut Vec<MultiTrackCue>) {
        out.push(m.clone());
        for a in m.events.iter().chain(m.tracks.iter().flat_map(|t| t.automation.iter())) {
            let Automation::Kind7 { cue, .. } = a else { continue };
            if !visited.insert(*cue) {
                continue;
            }
            let Some(entry) = self.sounddb.find_cue(*cue) else { continue };
            if let Ok(ResolvedCue { multitrack: Some(child), .. }) = self.resolve_cue(entry) {
                self.with_children(&child, visited, out);
            }
        }
    }

    fn new_handle(&mut self) -> CueHandle {
        let h = CueHandle(self.next_handle);
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        h
    }

    /// The voice request a cue's voices share. Whether an instance is positional is its group's
    /// (`ResolvedChoice::positional`); a wave loops by its own count, not the voice's. No distance
    /// start delay: `FUN_008369e0` adds one only for a multi-wave group whose `+0x44` byte is set
    /// (`0x00836ABF`), and that byte is 0 in every retail group (the soundbank reader requires it).
    fn voice_template(&self, cue: &CueEntry, position: Option<Vec3>) -> VoiceRequest {
        VoiceRequest {
            cue_guid: cue.guid,
            priority: cue.priority,
            category: cue.category,
            gain: 1.0,
            looping: false,
            positional: position.is_some(),
            start_delay: 0.0,
        }
    }

    /// Refuse a cue this engine cannot play faithfully: a curve (kinds 5 and 6, or a kind-8 curve a
    /// kind-9 record reads) whose parameter is unset or lies past its last point, a kind-9 curve index
    /// past the curve table, a filter scan past the event table, and a child cue that is refused
    /// itself. `visited` holds the cues already checked on this chain (a child chain may lead back to
    /// its start).
    fn check_playable(
        &self,
        resolved: &ResolvedCue,
        params: &HashMap<u32, f32>,
        visited: &mut HashSet<u32>,
    ) -> Result<(), CueError> {
        let Some(m) = &resolved.multitrack else { return Ok(()) };
        if m.curves.len() > m.events.len() {
            return Err(CueError::FilterScan { events: m.events.len(), curves: m.curves.len() });
        }
        for a in m.events.iter().chain(m.tracks.iter().flat_map(|t| t.automation.iter())) {
            match a {
                Automation::Kind9 { curve_a, curve_b, .. } => {
                    for index in [*curve_a, *curve_b].into_iter().filter(|&i| i != u32::MAX) {
                        let i = (index & 0xFF) as usize;
                        let Some(Automation::Curve { param, .. }) = m.curves.get(i) else {
                            return Err(CueError::Automation(AutomationError::CurveIndex {
                                index: index & 0xFF,
                                curves: m.curves.len(),
                            }));
                        };
                        let value = param_value(m, params, &self.global_params, *param).map_err(CueError::Automation)?;
                        check_curves(m, *param, value)?;
                    }
                }
                Automation::Curve { kind: crate::multitrack::CurveKind::Cue, .. } => {}
                Automation::Curve { param, .. } => {
                    let value = param_value(m, params, &self.global_params, *param).map_err(CueError::Automation)?;
                    check_curves(m, *param, value)?;
                }
                Automation::Kind7 { cue, .. } => self.check_child(*cue, visited)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// A kind-7 child is playable when the engine's start would fail (not in the sound database, or
    /// its soundbank not loaded — the parent then treats it as finished) or when it is playable
    /// itself, with no cue-local parameter values (nothing supplies them to a child).
    fn check_child(&self, guid: u32, visited: &mut HashSet<u32>) -> Result<(), CueError> {
        if !visited.insert(guid) {
            return Ok(());
        }
        let Some(cue) = self.sounddb.find_cue(guid) else { return Ok(()) };
        let refused = |error: CueError| CueError::Child { cue: guid, error: Box::new(error) };
        match self.resolve_cue(cue) {
            Err(ResolveError::SoundbankNotResident(_)) => Ok(()),
            Err(e) => Err(refused(CueError::Resolve(e))),
            Ok(r) => self.check_playable(&r, &HashMap::new(), visited).map_err(refused),
        }
    }


    /// `Sound.StopSound(cue)` — release a started cue the way the cue stop `FUN_00835720` does: its
    /// voices fade out, its tracks stop looping, nothing more fires, and every track (and the cue)
    /// whose child cue is not running starts it, or stops the one that is. The cue is done once its
    /// instances and children have finished.
    pub fn stop_sound(&mut self, handle: CueHandle) {
        let Some(i) = self.playbacks.iter().position(|p| p.handle == handle) else { return };
        let mut pb = self.take_playback(i);
        self.release(&mut pb);
        self.playbacks[i] = pb;
    }

    /// `FUN_00835720`.
    fn release(&mut self, pb: &mut Playback) {
        if matches!(pb.state, RunState::Done | RunState::WaitingChild) {
            return;
        }
        if let Some(id) = pb.direct {
            self.pool.stop(id, true);
            pb.state = RunState::WaitingChild;
            return;
        }
        let position = pb.position;
        for t in 0..pb.tracks.len() {
            // FUN_0083c470.
            let tr = &mut pb.tracks[t];
            tr.loop_count = 0;
            for inst in &tr.sounds.instances {
                if let Some(id) = inst.voice {
                    self.pool.stop(id, true);
                }
            }
            tr.sounds.state = ListState::Fired;
            match tr.child {
                None => {
                    if let Some(guid) = tr.automation.child_cue() {
                        let child = self.start_child(guid, position, (pb.handle, Some(t)), pb.emitter);
                        pb.tracks[t].child = child;
                        if self.finished_at_start(child) {
                            // The engine's completion callback (LAB_0083c550) ran inside the start.
                            pb.tracks[t].state = RunState::Done;
                        }
                    }
                }
                Some(child) => self.release_handle(child),
            }
            let tr = &mut pb.tracks[t];
            if tr.state != RunState::Done {
                tr.state = RunState::WaitingChild;
            }
        }
        match pb.child {
            None => {
                if let Some(guid) = pb.cue_automation.child_cue() {
                    pb.child = self.start_child(guid, position, (pb.handle, None), pb.emitter);
                    if self.finished_at_start(pb.child) {
                        // LAB_008359a0 ran inside the start: the cue finishes (its own state is set
                        // to 3 below regardless, as FUN_00835720 does).
                        self.finish(pb);
                    }
                }
            }
            Some(child) => self.release_handle(child),
        }
        if let Some(id) = pb.single.and_then(|i| i.voice) {
            self.pool.stop(id, true);
        }
        pb.single_state = 1;
        pb.state = RunState::WaitingChild;
    }

    fn release_handle(&mut self, handle: CueHandle) {
        if let Some(i) = self.playbacks.iter().position(|p| p.handle == handle) {
            let mut pb = self.take_playback(i);
            self.release(&mut pb);
            self.playbacks[i] = pb;
        }
    }

    /// `Sound.PauseSound(cue)` — pause a started cue's voices.
    pub fn pause_sound(&mut self, handle: CueHandle) {
        for id in self.cue_voices(handle) {
            if let Some(v) = self.pool.get_mut(id) {
                v.state = crate::voice::InstanceState::Paused;
            }
        }
    }

    /// `Sound.StopAndFlushAllSounds` — stop every voice and drop every started cue (no release: no
    /// child cue starts).
    pub fn stop_and_flush_all_sounds(&mut self) {
        let ids: Vec<VoiceId> = self.pool.iter_active().map(|v| v.id).collect();
        for id in ids {
            self.pool.stop(id, false);
        }
        self.playbacks.clear();
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

    /// The cue list walk of `FUN_0082ee60`: every started cue in start order, including cues started
    /// during the walk (a child is appended and advances in the same frame); done cues are dropped.
    fn advance_playbacks(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.playbacks.len() {
            let mut pb = self.take_playback(i);
            self.advance(&mut pb, dt);
            self.playbacks[i] = pb;
            i += 1;
        }
        self.playbacks.retain(|pb| pb.state != RunState::Done);
    }

    /// Take playback `i` out for mutation, leaving an inert placeholder (handle 0 is never issued).
    fn take_playback(&mut self, i: usize) -> Playback {
        let placeholder = Playback {
            handle: CueHandle(0),
            resolved: ResolvedCue { soundbank: 0, cue_index: 0, gain: 0.0, multitrack: None, sounds: Vec::new() },
            req: VoiceRequest::default(),
            position: None,
            params: HashMap::new(),
            direct: None,
            state: RunState::Done,
            single_state: 2,
            single: None,
            loop_count: 0,
            cue_time: 0.0,
            cue_automation: AutomationState::default(),
            cue_last: AutomationOutput::default(),
            tracks: Vec::new(),
            child: None,
            parent: None,
            emitter: 0,
        };
        std::mem::replace(&mut self.playbacks[i], placeholder)
    }

    /// Whether a child start ended with the child already finished: the start failed, or the child's
    /// play-probability draw dropped it — either way the engine ran the parent's completion callback
    /// inside the start.
    fn finished_at_start(&self, child: Option<CueHandle>) -> bool {
        child.is_none_or(|h| !self.cue_is_playing(h))
    }

    /// `FUN_00835850`: a cue finishes — its parent's completion callback runs.
    fn finish(&mut self, pb: &Playback) {
        if let Some(parent) = pb.parent {
            self.child_finished(parent);
        }
    }

    /// A child cue finished: `LAB_0083c550` (a track's child: the track is done, its child −1) or
    /// `LAB_008359a0` (the cue's child: the cue is done and finishes).
    fn child_finished(&mut self, (handle, track): (CueHandle, Option<usize>)) {
        let Some(i) = self.playbacks.iter().position(|p| p.handle == handle) else { return };
        match track {
            Some(t) => {
                let tr = &mut self.playbacks[i].tracks[t];
                tr.state = RunState::Done;
                tr.child = None;
            }
            None => {
                let mut pb = self.take_playback(i);
                pb.state = RunState::Done;
                self.finish(&pb);
                self.playbacks[i] = pb;
            }
        }
    }

    /// Whether an instance's voice has ended (its wave finished, or it was stolen).
    fn voice_live(&self, id: VoiceId) -> bool {
        self.pool.get(id).is_some_and(|v| !v.state.is_terminal()) && self.mixer.is_attached(id)
    }

    /// `FUN_00835060`: one frame of a started cue.
    fn advance(&mut self, pb: &mut Playback, dt: f32) {
        if pb.state == RunState::Done {
            return;
        }
        if let Some(id) = pb.direct {
            if !self.voice_live(id) {
                pb.state = RunState::Done;
            }
            return;
        }
        let Some(m) = pb.resolved.multitrack.clone() else {
            self.advance_single(pb, dt);
            return;
        };
        let params = pb.params.clone();
        let globals = self.global_params.clone();
        let cx = CueContext { m: &m, params: &params, globals: &globals };
        // The cue block, from the previous frame's cue automation.
        let block = Block {
            volume: clamp01(pb.resolved.gain * pb.cue_last.volume),
            pitch: pb.cue_last.pitch,
            channels: pb.cue_last.channels,
        };
        if pb.state == RunState::WaitingChild {
            let over = override_of(&pb.cue_last);
            if self.advance_tracks(pb, &cx, dt, Drive { block, over }) && pb.child.is_none() {
                pb.state = RunState::Done;
                self.finish(pb);
            }
            return;
        }
        let looped = pb.loop_count != 0;
        let mut dt_tracks = dt;
        if looped && pb.cue_time + dt >= m.unknown_20 {
            let (start, end) = (m.unknown_1c, m.unknown_20);
            pb.cue_automation.rewind(&m.events, start);
            let raw = Block::of(&pb.cue_last);
            let over = override_of(&pb.cue_last);
            let rem = end - pb.cue_time;
            for t in 0..m.tracks.len() {
                self.loop_restart(pb, &m, t, (start, end), rem, Drive { block: raw, over });
            }
            let d = (pb.cue_time + dt) - end;
            pb.cue_time = start + d;
            dt_tracks = d;
            if pb.loop_count != 0xFF {
                pb.loop_count -= 1;
            }
        } else {
            pb.cue_time += dt;
        }
        let value = |p: u32| param_value(&m, &params, &globals, p);
        pb.cue_last = pb
            .cue_automation
            .step(&m.events, &m.curves, pb.cue_time, &value, &mut self.rng)
            .expect("automation was validated when the cue started");
        let over = override_of(&pb.cue_last);
        if !self.advance_tracks(pb, &cx, dt_tracks, Drive { block, over }) || looped {
            return;
        }
        if let Some(guid) = pb.cue_automation.child_cue() {
            let child = self.start_child(guid, pb.position, (pb.handle, None), pb.emitter);
            if self.finished_at_start(child) {
                // LAB_008359a0 ran inside the start; FUN_00835060 then sets state 3 regardless.
                self.finish(pb);
            }
            pb.child = child;
            pb.state = RunState::WaitingChild;
            return;
        }
        pb.state = RunState::Done;
        self.finish(pb);
    }

    /// The single-track path of `FUN_00835060` (`FUN_0083bec0`): start the one instance, update it
    /// with `{clamp01(cue gain), 0, 1.0 × 6}`, and be done on the update after it finishes.
    fn advance_single(&mut self, pb: &mut Playback, dt: f32) {
        let block = Block { volume: clamp01(pb.resolved.gain * 1.0), pitch: 0.0, channels: [1.0; 6] };
        if pb.single_state == 0 && pb.single.is_none() {
            pb.single = Some(self.start_instance(pb, 0, None));
        }
        match pb.single {
            Some(mut inst) if !inst.finished => {
                self.update_instance(&mut inst, block, &pb.req.clone(), None, dt);
                pb.single = Some(inst);
            }
            _ => {
                pb.single = None;
                pb.single_state = 2;
            }
        }
        if pb.single_state == 2 {
            pb.state = RunState::Done;
            self.finish(pb);
        }
    }

    /// Advance every track in order; whether all of them are done.
    fn advance_tracks(&mut self, pb: &mut Playback, cx: &CueContext, dt: f32, cue: Drive) -> bool {
        let mut all_done = true;
        for t in 0..cx.m.tracks.len() {
            self.advance_track(pb, cx, t, dt, cue);
            all_done &= pb.tracks[t].state == RunState::Done;
        }
        all_done
    }

    /// `FUN_0083c070`: one frame of track `t`, under the cue's block and override.
    fn advance_track(&mut self, pb: &mut Playback, cx: &CueContext, t: usize, dt: f32, cue: Drive) {
        let (m, block, cue_over) = (cx.m, cue.block, cue.over);
        let track = &m.tracks[t];
        let looped = pb.tracks[t].loop_count != 0;
        let old = pb.tracks[t].time;
        let mut dt = dt;
        if old + dt >= track.unknown_08 && looped {
            let last = pb.tracks[t].last;
            let lb = Block { volume: last.volume * block.volume, pitch: last.pitch + block.pitch, channels: block.channels };
            let over = cue_over.or(override_of(&last));
            let (start, end) = (track.unknown_04, track.unknown_08);
            self.loop_restart(pb, m, t, (start, end), end - old, Drive { block: lb, over });
            let d = (dt + old) - end;
            let tr = &mut pb.tracks[t];
            tr.time = start + d;
            dt = d;
            if tr.loop_count != 0xFF {
                tr.loop_count -= 1;
            }
        } else {
            pb.tracks[t].time = old + dt;
        }
        let value = |p: u32| param_value(m, cx.params, cx.globals, p);
        let time = pb.tracks[t].time;
        let out = pb.tracks[t]
            .automation
            .step(&track.automation, &m.curves, time, &value, &mut self.rng)
            .expect("automation was validated when the cue started");
        pb.tracks[t].last = out;
        let mut channels = block.channels;
        for (c, o) in channels.iter_mut().zip(out.channels) {
            *c *= o;
        }
        let b = Block { volume: out.volume * block.volume, pitch: out.pitch + block.pitch, channels };
        let over = cue_over.or(override_of(&out));
        self.fire_and_update(pb, m, t, (time, dt), Drive { block: b, over });

        let tr = &pb.tracks[t];
        let sounds_done = tr.sounds.state == ListState::Done;
        let ready = if !looped && sounds_done {
            if tr.state == RunState::Playing {
                match tr.automation.child_cue() {
                    Some(guid) => {
                        let child = self.start_child(guid, pb.position, (pb.handle, Some(t)), pb.emitter);
                        let fired = self.finished_at_start(child);
                        let tr = &mut pb.tracks[t];
                        tr.child = child;
                        // A start that finished at once already ran LAB_0083c550 (state 2).
                        tr.state = if fired { RunState::Done } else { RunState::WaitingChild };
                    }
                    None => pb.tracks[t].state = RunState::Done,
                }
                return;
            }
            tr.state == RunState::WaitingChild
        } else {
            if tr.state != RunState::WaitingChild {
                return;
            }
            sounds_done
        };
        if ready && pb.tracks[t].child.is_none() {
            pb.tracks[t].state = RunState::Done;
        }
    }

    /// `FUN_0083c3c0`: a loop restart of track `t` — fire what starts by the loop end and update the
    /// instances, rewind the automation and the sounds to the loop start, and mark the track playing.
    fn loop_restart(
        &mut self,
        pb: &mut Playback,
        m: &MultiTrackCue,
        t: usize,
        (start, end): (f32, f32),
        dt: f32,
        drive: Drive,
    ) {
        self.fire_and_update(pb, m, t, (end, dt), drive);
        let track = &m.tracks[t];
        let starts: Vec<f32> = track.sounds.iter().map(|s| s.start_s).collect();
        let tr = &mut pb.tracks[t];
        tr.automation.rewind(&track.automation, start);
        tr.sounds.rewind(&starts, start);
        tr.state = RunState::Playing;
    }

    /// `FUN_0083fee0`: fire track `t`'s sounds that start by `time`, then drop the instances that
    /// finished on an earlier update and update the rest with the block (after the override block, if
    /// any) and `dt`, and move the list state on.
    fn fire_and_update(&mut self, pb: &mut Playback, m: &MultiTrackCue, t: usize, (time, dt): (f32, f32), drive: Drive) {
        let (block, over) = (drive.block, drive.over);
        let base: usize = m.tracks[..t].iter().map(|x| x.sounds.len()).sum();
        let n = m.tracks[t].sounds.len();
        if pb.tracks[t].sounds.state == ListState::Firing {
            for k in pb.tracks[t].sounds.next..n {
                if m.tracks[t].sounds[k].start_s <= time {
                    let inst = self.start_instance(pb, base + k, over);
                    let list = &mut pb.tracks[t].sounds;
                    list.instances.push(inst);
                    list.next = k + 1;
                }
            }
        }
        let req = pb.req.clone();
        let filter = Some(pb.cue_automation.filter_params());
        let mut kept = Vec::with_capacity(pb.tracks[t].sounds.instances.len());
        for mut inst in std::mem::take(&mut pb.tracks[t].sounds.instances) {
            if inst.finished {
                continue;
            }
            if let Some((v, p)) = over {
                inst.volume = v;
                inst.pitch = p;
                inst.channels = [1.0; 6];
            }
            self.update_instance(&mut inst, block, &req, filter, dt);
            kept.push(inst);
        }
        let list = &mut pb.tracks[t].sounds;
        list.instances = kept;
        match list.state {
            ListState::Firing if list.instances.is_empty() && list.next == n => list.state = ListState::Fired,
            ListState::Fired if list.instances.is_empty() => list.state = ListState::Done,
            _ => {}
        }
    }

    /// `FUN_00840280` / `FUN_008369e0`: start an instance of sound `s` — pick its entry and the
    /// entry group's wave, draw the base pitch, volume and start delay, and take a voice. An instance
    /// whose pick finds nothing starts finished.
    fn start_instance(&mut self, pb: &Playback, s: usize, over: Option<(f32, f32)>) -> Instance {
        let mut inst = Instance {
            voice: None,
            wave: None,
            volume: 1.0,
            pitch: 0.0,
            channels: [1.0; 6],
            delay_s: 0.0,
            elapsed_s: 0.0,
            loop_count: 0,
            positional: false,
            wave3d: None,
            emitter: pb.emitter,
            filtered: false,
            finished: true,
        };
        let sound = &pb.resolved.sounds[s];
        let choice = match sound.selection {
            None => Some(0),
            Some((mode, slot)) => {
                let weights: Vec<f32> = sound.choices.iter().map(|c| c.weight).collect();
                let st = self.slot_state.entry((pb.resolved.soundbank, pb.resolved.cue_index, slot)).or_insert(STATE_INIT);
                select::pick(mode, &weights, st, &mut self.rng)
            }
        };
        let Some(choice) = choice.and_then(|i| sound.choices.get(i)) else { return inst };
        let wave = match choice.selection {
            None => Some(0),
            Some(mode) => {
                let weights: Vec<f32> = choice.waves.iter().map(|w| w.weight).collect();
                let st = self.group_state.entry((choice.soundbank, choice.group_index)).or_insert(STATE_INIT);
                select::pick(mode, &weights, st, &mut self.rng)
            }
        };
        let Some(wave) = wave.and_then(|i| choice.waves.get(i)).copied() else { return inst };
        let start = choice.instance.start(&mut self.rng);
        let clip = self.clip(wave.wavebank, wave.index).expect("resolved when the cue started");
        let (samples, channels, clip_rate) = (clip.samples.clone(), clip.channels as usize, clip.sample_rate);
        let delay_s = pb.req.start_delay + start.delay_s;
        inst.positional = choice.positional && pb.req.positional;
        if inst.positional {
            // The instance's emitter source (FUN_00837830 creates it at the cue's position); the cue
            // API carries no velocity, so it is at rest. Its wave takes the group's 3D parameters.
            let pos = pb.position.expect("a positional cue has a position");
            self.mixer.set_emitter(inst.emitter, pos, Vec3::ZERO);
            inst.wave3d = Some(choice.wave3d);
        }
        inst.loop_count = choice.loop_byte;
        // FUN_00839db0 (wave vtable +0x7C, at CreateWave): a filter when one of the cue's first C event
        // records (C = its curve count) is kind 9.
        inst.filtered = pb
            .resolved
            .multitrack
            .as_ref()
            .is_some_and(|m| m.events.iter().take(m.curves.len()).any(|e| matches!(e, Automation::Kind9 { .. })));
        let req = VoiceRequest { start_delay: delay_s, positional: inst.positional, ..pb.req.clone() };
        inst.voice = self.pool.acquire(&req);
        if let Some(id) = inst.voice {
            let src = PcmSource::with_rate(samples, channels, clip_rate, self.mixer.config().sample_rate)
                .with_loops(u32::from(inst.loop_count));
            self.mixer.attach_pcm(id, src);
            self.wire_voice(id, &inst);
        }
        inst.wave = Some(InstanceWave { wavebank: wave.wavebank, index: wave.index, clip_rate });
        inst.volume = start.volume;
        inst.pitch = start.pitch;
        inst.delay_s = delay_s;
        inst.finished = false;
        if let Some((v, p)) = over {
            inst.volume = v;
            inst.pitch = p;
        }
        inst
    }

    /// `FUN_00836c70`: multiply the instance's channel multipliers by the block's, give its voice
    /// `base volume × block volume`, `base pitch + block pitch` and its six channel multipliers, and
    /// mark it finished once its voice has ended — or, with no voice, once its start delay has passed.
    fn update_instance(
        &mut self,
        inst: &mut Instance,
        block: Block,
        req: &VoiceRequest,
        filter: Option<(f32, f32)>,
        dt: f32,
    ) {
        for (c, b) in inst.channels.iter_mut().zip(block.channels) {
            *c *= b;
        }
        inst.elapsed_s += dt;
        if inst.voice.is_none() && inst.wave.is_some() && inst.elapsed_s >= inst.delay_s {
            if inst.loop_count == 0xFF {
                // FUN_00836c70: an instance with no wave whose group loop count is 0xFF stays in state 0
                // and tries to create its wave again on the next update; any other count finishes it.
                self.retry_voice(inst, req);
            } else {
                inst.finished = true;
                return;
            }
        }
        let (Some(id), Some(wave)) = (inst.voice, inst.wave) else {
            if inst.wave.is_none() && inst.elapsed_s >= inst.delay_s {
                inst.finished = true;
            }
            return;
        };
        if self.mixer.is_attached(id) {
            if let Some(v) = self.pool.get_mut(id) {
                v.gain = inst.volume * block.volume;
            }
            let rate = pitched_rate(wave.clip_rate, inst.pitch + block.pitch);
            self.mixer
                .set_source_rate(id, rate)
                .expect("playback voices carry PCM sources built at the mixer rate");
            // Wave vtable +0x10C for the six outputs.
            self.mixer.set_output_channels(id, inst.channels);
            // FUN_0083e5c0: the filter takes the cue's kind-9 outputs.
            if let (true, Some((a, b))) = (inst.filtered, filter) {
                self.mixer.set_filter_params(id, a, b);
            }
        }
        if !self.voice_live(id) {
            inst.finished = true;
        }
    }

    /// Try again to give a waveless instance its voice (the `CreateWave` retry of `FUN_00836c70`).
    fn retry_voice(&mut self, inst: &mut Instance, req: &VoiceRequest) {
        let Some(wave) = inst.wave else { return };
        let req = VoiceRequest { start_delay: 0.0, positional: inst.positional, ..req.clone() };
        let Some(id) = self.pool.acquire(&req) else { return };
        let clip = self.clip(wave.wavebank, wave.index).expect("resolved when the cue started");
        let (samples, channels) = (clip.samples.clone(), clip.channels as usize);
        let src = PcmSource::with_rate(samples, channels, wave.clip_rate, self.mixer.config().sample_rate)
            .with_loops(u32::from(inst.loop_count));
        self.mixer.attach_pcm(id, src);
        self.wire_voice(id, inst);
        inst.voice = Some(id);
    }

    /// Route a new wave through its source (its emitter's when positional, else the shared 2D one) and
    /// give it the cue's filter when it carries one.
    fn wire_voice(&mut self, id: VoiceId, inst: &Instance) {
        if inst.positional {
            self.mixer.set_source(id, SourceKey::Emitter(inst.emitter));
            self.mixer.set_wave_3d(id, inst.wave3d.expect("a positional instance has its group's 3D parameters"));
        }
        if inst.filtered {
            self.mixer.add_filter(id);
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
        self.mixer.set_listener(*self.listeners.mix_listener());
        self.mixer
            .mix(&mut self.pool, &mut out, |cat| cats.effective_gain(cat));
        self.sink.submit(&out);
        out
    }
}

/// The engine seeds its generator from the low 32 bits of `QueryPerformanceCounter` at init (see
/// [`crate::select`]); this engine seeds from the wall clock, the same kind of per-run value (the
/// counter's own value and frequency are the machine's). [`AudioEngine::set_rng_seed`] fixes it.
fn clock_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock reads before 1970")
        .as_nanos() as u32
}

/// What a multi-track cue's frame reads: its body and its parameter values.
struct CueContext<'a> {
    m: &'a MultiTrackCue,
    params: &'a HashMap<u32, f32>,
    globals: &'a HashMap<u32, f32>,
}

/// A block and the override block (volume, pitch) that goes with it, if any.
#[derive(Clone, Copy)]
struct Drive {
    block: Block,
    over: Option<(f32, f32)>,
}

/// The override block of an automation step, when a non-zero-mode ramp is active.
fn override_of(out: &AutomationOutput) -> Option<(f32, f32)> {
    out.override_active.then_some((out.override_volume, out.override_pitch))
}

/// Refuse a value that lies past the last point of a curve over `param` in `m`.
fn check_curves(m: &MultiTrackCue, param: u32, value: f32) -> Result<(), CueError> {
    for a in m.events.iter().chain(m.curves.iter()).chain(m.tracks.iter().flat_map(|t| t.automation.iter())) {
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
    use crate::encode::{
        build_general, CueBodySpec, CueDef, GroupFormSpec, GroupHeadParams, GroupSpec, MultiGroupParams, Pcm16,
        TablesSpec, WaveSpec,
    };
    use crate::multitrack::{Sound, SoundEntry, Target, Track};
    use mercs2_formats::hash::pandemic_hash_m2 as m2;

    const BANK: &str = "mod_auto";

    fn head() -> GroupHeadParams {
        GroupHeadParams {
            unknown_10: 1.0,
            unknown_14: 0,
            min_distance: 10.0,
            max_distance: 100.0,
            unknown_20: 1.0,
            distance_exponent: 1.0,
            doppler_scale: 1.0,
        }
    }

    fn multi(tracks: Vec<Track>, events: Vec<Automation>, gain: f32) -> MultiTrackCue {
        let bank = m2(BANK);
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
        let _ = gain;
        MultiTrackCue {
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
        }
    }

    /// A bank `mod_auto`: wave 0 is `frames` mono samples at 22,050 Hz; group 0 plays it (base volume
    /// 0.5, base pitch 2.0), group 1 plays it from a multi-wave group whose `+0x2C` loop count is 3.
    /// Each `(name, gain, body)` is a cue.
    fn engine(cues: Vec<(&str, f32, CueBodySpec)>, frames: usize, channels: usize) -> AudioEngine {
        engine_with(cues, frames, channels, head())
    }

    /// [`engine`] with both groups given `group_head`.
    fn engine_with(
        cues: Vec<(&str, f32, CueBodySpec)>,
        frames: usize,
        channels: usize,
        group_head: GroupHeadParams,
    ) -> AudioEngine {
        let spec = TablesSpec {
            name: BANK.into(),
            waves: vec![WaveSpec { clip_hash: 1, pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![1000; frames] } }],
            groups: vec![
                GroupSpec {
                    sound_id: 1,
                    category: "sfx".into(),
                    head: group_head,
                    form: GroupFormSpec::Single { wave: 0, gain: 0.5, unknown_30: 2.0, weight: 1.0 },
                },
                GroupSpec {
                    sound_id: 2,
                    category: "sfx".into(),
                    head: group_head,
                    form: GroupFormSpec::Multi {
                        params: MultiGroupParams {
                            byte_2c: 3,
                            selection: 0,
                            byte_2f: 0,
                            unknown_30: 0.0,
                            unknown_34: 0.0,
                            unknown_3c: 0.0,
                            unknown_40: 0.0,
                            word_48: 0,
                            floats_4c: [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
                            unknown_64: 0.0,
                        },
                        waves: vec![(0, 1.0)],
                    },
                },
            ],
            cues: cues
                .into_iter()
                .map(|(name, gain, body)| CueDef { name: name.into(), byte_06: 0, gain, body })
                .collect(),
        };
        let enc = build_general(&spec).unwrap().to_bytes().unwrap();
        let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels });
        eng.set_rng_seed(7);
        eng.set_sounddb(SoundDb::parse(&enc.sounddb).unwrap());
        eng.load_soundbank(&enc.soundbank).unwrap();
        eng.load_wavebank(&enc.wavebank).unwrap();
        eng
    }

    fn sound(start_s: f32, group_index: u16) -> Sound {
        Sound {
            slot: 0,
            byte_01: 2,
            byte_02: 2,
            selection: 1,
            start_s,
            entries: vec![SoundEntry { soundbank: 0, group_index, unknown_06: 0, weight: 1.0 }],
        }
    }

    fn track(automation: Vec<Automation>, sounds: Vec<Sound>) -> Track {
        Track { byte_00: 0, unknown_04: -1.0, unknown_08: -1.0, automation, sounds }
    }

    fn one_cue(tracks: Vec<Track>, events: Vec<Automation>, gain: f32, frames: usize) -> (AudioEngine, u32) {
        let body = CueBodySpec::MultiTrack(multi(tracks, events, gain));
        (engine(vec![("mod_auto_cue", gain, body)], frames, 2), m2("mod_auto_cue"))
    }

    const LONG: usize = 22050 * 4;
    const SHORT: usize = 1100; // 0.05 s

    fn ramp(target: Target, from: f32, to: f32) -> Automation {
        Automation::Ramp { target, start_s: 0.0, mode: 0, unknown_0c: 0, duration_s: 1.0, from, to }
    }

    /// Run `n` frames of `dt`, rendering after each so finished waves end their voices.
    fn run(eng: &mut AudioEngine, n: usize, dt: f32) {
        for _ in 0..n {
            eng.tick(dt);
            eng.render((44100.0 * dt) as usize);
        }
    }

    /// A track volume fade and pitch ramp reach the voice: gain = base volume (group +0x2C) × ramp ×
    /// clamp01(cue gain); rate = pitched_rate(clip rate, base pitch (+0x30) + ramp).
    #[test]
    fn track_automation_drives_the_voice() {
        let tr = track(vec![ramp(Target::Volume, 1.0, 0.0), ramp(Target::Pitch, 0.0, 12.0)], vec![sound(0.0, 0)]);
        let (mut eng, cue) = one_cue(vec![tr], vec![], 0.8, LONG);
        let h = eng.cue_sound(cue, None).unwrap();
        for _ in 0..5 {
            eng.tick(0.1);
        }
        let t = 0.1f32 + 0.1 + 0.1 + 0.1 + 0.1;
        let voice = eng.cue_instances(h)[0].voice.unwrap();
        let ramp_v = (0.0f32 - 1.0) / ((1.0 + 0.0) - 0.0) * (t - 0.0) + 1.0;
        assert_eq!(eng.pool.get(voice).unwrap().gain, 0.5 * (ramp_v * 0.8f32.min(1.0)));
        let ramp_p = (12.0f32 - 0.0) / ((1.0 + 0.0) - 0.0) * (t - 0.0);
        let rate = pitched_rate(22050, 2.0 + ((ramp_p + 0.0) + 0.0));
        assert_eq!(eng.mixer.source_rate(voice).unwrap(), Some(rate));
    }

    /// The cue's event table acts one frame late, through the clamped cue volume.
    #[test]
    fn cue_events_lag_a_frame() {
        let (mut eng, cue) = one_cue(vec![track(vec![], vec![sound(0.0, 0)])], vec![ramp(Target::Volume, 0.5, 0.5)], 1.0, LONG);
        let h = eng.cue_sound(cue, None).unwrap();
        eng.tick(0.1);
        let voice = eng.cue_instances(h)[0].voice.unwrap();
        assert_eq!(eng.pool.get(voice).unwrap().gain, 0.5 * 1.0, "frame 1 still uses the start value");
        eng.tick(0.1);
        assert_eq!(eng.pool.get(voice).unwrap().gain, 0.5 * 0.5, "frame 2 sees frame 1's event");
    }

    /// A track loop re-fires its sounds from the loop start — except sound 0 when sound 1 also starts
    /// at or after the loop start (`FUN_00840230`) — and stops once its count runs out.
    #[test]
    fn a_track_loop_refires_from_the_loop_start() {
        let mut tr = track(vec![], vec![sound(0.0, 0), sound(0.05, 0)]);
        tr.byte_00 = 1;
        tr.unknown_04 = 0.0;
        tr.unknown_08 = 0.2;
        let (mut eng, cue) = one_cue(vec![tr], vec![], 1.0, LONG);
        let h = eng.cue_sound(cue, None).unwrap();
        eng.tick(0.1); // t 0.1: sounds 0 and 1 fire
        assert_eq!(eng.cue_instances(h).len(), 2);
        eng.tick(0.1); // t 0.2 reaches the loop end: rewind to sound 1, time 0.0
        assert_eq!(eng.cue_instances(h).len(), 2, "time 0.0: sound 1 (start 0.05) not yet");
        eng.tick(0.1); // t 0.1: sound 1 again; sound 0 is skipped
        assert_eq!(eng.cue_instances(h).len(), 3);
        eng.tick(0.1);
        eng.tick(0.1); // t 0.3: the count is spent, no second loop
        assert_eq!(eng.cue_instances(h).len(), 3);
    }

    /// A cue loop restarts every track's sounds against the cue's loop points.
    #[test]
    fn a_cue_loop_restarts_its_tracks() {
        let mut m = multi(vec![track(vec![], vec![sound(0.0, 0)])], vec![], 1.0);
        m.byte_10 = 1;
        m.unknown_1c = 0.0;
        m.unknown_20 = 0.2;
        let mut eng = engine(vec![("mod_auto_cue", 1.0, CueBodySpec::MultiTrack(m))], LONG, 2);
        let h = eng.cue_sound(m2("mod_auto_cue"), None).unwrap();
        eng.tick(0.1);
        assert_eq!(eng.cue_instances(h).len(), 1);
        eng.tick(0.1); // cue time reaches 0.2: the track's sounds rewind to 0 and fire again
        assert_eq!(eng.cue_instances(h).len(), 2);
        eng.tick(0.1);
        eng.tick(0.1);
        assert_eq!(eng.cue_instances(h).len(), 2, "one loop only");
    }

    /// A kind-7 record starts its child cue when the track's sounds are done, the track waits for it,
    /// and the cue is done when the child is.
    #[test]
    fn a_track_starts_its_child_cue_when_it_finishes() {
        let child = Automation::Kind7 { start_bits: 0, cue: m2("mod_child") };
        let parent = multi(vec![track(vec![child], vec![sound(0.0, 0)])], vec![], 1.0);
        let mut eng = engine(
            vec![
                ("mod_parent", 1.0, CueBodySpec::MultiTrack(parent)),
                ("mod_child", 1.0, CueBodySpec::SingleTrack { group: 0, unknown_16: 0 }),
            ],
            SHORT,
            2,
        );
        let h = eng.cue_sound(m2("mod_parent"), None).unwrap();
        run(&mut eng, 1, 0.02);
        assert!(eng.cue_children(h).is_empty());
        let mut started = None;
        for _ in 0..40 {
            run(&mut eng, 1, 0.02);
            if let Some(&c) = eng.cue_children(h).first() {
                started = Some(c);
                break;
            }
        }
        let c = started.expect("the child starts once the parent's sound has finished");
        assert!(eng.cue_is_playing(h), "the parent waits for its child");
        for _ in 0..40 {
            run(&mut eng, 1, 0.02);
        }
        assert!(!eng.cue_is_playing(c));
        assert!(!eng.cue_is_playing(h), "done once its child is");
    }

    /// A child cue plays through its parent's emitter (`FUN_0082e930` hands `FUN_0082e960` the
    /// parent's `+0x118` / `+0xC8`), not one of its own: its positional instances name the parent's.
    #[test]
    fn a_child_cue_plays_through_its_parents_emitter() {
        let mut h3d = head();
        h3d.unknown_14 = 1;
        let child = Automation::Kind7 { start_bits: 0, cue: m2("mod_child") };
        let parent = multi(vec![track(vec![child], vec![sound(0.0, 0)])], vec![], 1.0);
        let mut eng = engine_with(
            vec![
                ("mod_parent", 1.0, CueBodySpec::MultiTrack(parent)),
                ("mod_child", 1.0, CueBodySpec::SingleTrack { group: 0, unknown_16: 0 }),
            ],
            SHORT,
            2,
            h3d,
        );
        let h = eng.cue_sound(m2("mod_parent"), Some(Vec3::new(4.0, 0.0, 0.0))).unwrap();
        let mut started = None;
        for _ in 0..40 {
            run(&mut eng, 1, 0.02);
            if let Some(&c) = eng.cue_children(h).first() {
                started = Some(c);
                break;
            }
        }
        let c = started.expect("the child starts");
        run(&mut eng, 1, 0.02);
        let insts = eng.cue_instances(c);
        assert!(!insts.is_empty(), "the child has an instance");
        for i in insts {
            assert!(i.positional, "the child plays positionally at its parent's position");
            assert_eq!(i.emitter, h.0, "through its parent's emitter");
            assert_ne!(i.emitter, c.0);
        }
    }

    /// Stopping a cue starts the child cue of every track that has one (`FUN_00835720`).
    #[test]
    fn stopping_a_cue_starts_its_child() {
        let child = Automation::Kind7 { start_bits: 0, cue: m2("mod_tail") };
        let parent = multi(vec![track(vec![child], vec![sound(0.0, 0)])], vec![], 1.0);
        let mut eng = engine(
            vec![
                ("mod_loop", 1.0, CueBodySpec::MultiTrack(parent)),
                ("mod_tail", 1.0, CueBodySpec::SingleTrack { group: 0, unknown_16: 0 }),
            ],
            LONG,
            2,
        );
        let h = eng.cue_sound(m2("mod_loop"), None).unwrap();
        eng.tick(0.1);
        assert!(eng.cue_children(h).is_empty());
        eng.stop_sound(h);
        assert_eq!(eng.cue_children(h).len(), 1, "the tail cue starts on release");
    }

    /// A child the engine cannot start (not in the sound database) counts as finished.
    #[test]
    fn an_unknown_child_counts_as_finished() {
        let child = Automation::Kind7 { start_bits: 0, cue: 0x1234_5678 };
        let (mut eng, cue) = one_cue(vec![track(vec![child], vec![sound(0.0, 0)])], vec![], 1.0, SHORT);
        let h = eng.cue_sound(cue, None).unwrap();
        run(&mut eng, 20, 0.02);
        assert!(!eng.cue_is_playing(h));
    }

    /// A kind-4 record's output channels scale the voice's six engine outputs (left and right on a
    /// stereo device), and stay on the instance after the step that set them.
    #[test]
    fn kind4_channels_reach_left_and_right() {
        let mut words = [0u32; 17];
        words[11] = 0.5f32.to_bits(); // record channel 0 → output 0
        words[12] = 0.25f32.to_bits(); // record channel 1 → output 1
        for w in &mut words[13..17] {
            *w = 1.0f32.to_bits();
        }
        let (mut eng, cue) = one_cue(vec![track(vec![Automation::Kind4 { words }], vec![sound(0.0, 0)])], vec![], 1.0, LONG);
        let h = eng.cue_sound(cue, None).unwrap();
        eng.tick(0.1);
        eng.tick(0.1);
        assert_eq!(eng.cue_instances(h)[0].channels, [0.5, 0.25, 1.0, 1.0, 1.0, 1.0]);
        let pcm = eng.render(64);
        assert_eq!((pcm[0], pcm[1]), ((1000.0f32 * 0.5 * 0.5) as i16, (1000.0f32 * 0.5 * 0.25) as i16));
        let mut eng6 = engine(
            vec![("mod_auto_cue", 1.0, CueBodySpec::MultiTrack(multi(vec![track(vec![Automation::Kind4 { words }], vec![sound(0.0, 0)])], vec![], 1.0)))],
            LONG,
            6,
        );
        eng6.cue_sound(cue, None).unwrap();
        eng6.tick(0.1);
        eng6.tick(0.1);
        assert_eq!(&eng6.render(8)[..6], &[250, 125, 500, 500, 500, 500], "(1000 × trunc(0.5 × ch × 32768)) >> 15");
    }

    /// A group with loop count 3 plays its wave 4 times back to back (`FUN_00839e90`), and the
    /// instance lasts that long.
    #[test]
    fn a_looping_group_plays_its_wave_count_plus_one_times() {
        let (mut eng, cue) = one_cue(vec![track(vec![], vec![sound(0.0, 1)])], vec![], 1.0, SHORT);
        let h = eng.cue_sound(cue, None).unwrap();
        run(&mut eng, 1, 0.02);
        assert_eq!(eng.cue_instances(h)[0].loop_count, 3);
        run(&mut eng, 8, 0.02); // 0.18 s of a 0.05 s wave played 4 times
        assert_eq!(eng.cue_instances(h).len(), 1, "still playing inside 0.2 s");
        run(&mut eng, 8, 0.02);
        assert!(eng.cue_instances(h).is_empty(), "done after four plays");
    }

    fn filter_cue(curves: usize) -> MultiTrackCue {
        let mut m = multi(
            vec![track(vec![], vec![sound(0.0, 0)])],
            vec![Automation::Kind9 { start_bits: 0, curve_a: 0, curve_b: u32::MAX }],
            1.0,
        );
        m.curves = (0..curves)
            .map(|_| Automation::Curve {
                kind: crate::multitrack::CurveKind::Cue,
                unknown_04: 0,
                param: 0xD913_464B,
                points: vec![(0.0, 0.4), (1.0, 0.4)],
            })
            .collect();
        m
    }

    /// A cue whose first event is kind 9 gives its waves the filter, and every update hands the filter
    /// the kind-9 outputs (curve 0 at the global parameter's −1.0 → 0.4; the second output keeps 1.0).
    #[test]
    fn a_kind9_cue_filters_its_waves() {
        let mut eng = engine(vec![("mod_filtered", 1.0, CueBodySpec::MultiTrack(filter_cue(1)))], LONG, 2);
        let h = eng.cue_sound(m2("mod_filtered"), None).expect("kind 9 plays");
        eng.tick(0.02);
        let voice = eng.cue_instances(h)[0].voice.unwrap();
        let mut want = crate::filter::Biquad::new();
        want.set_param(0, 0.4);
        want.set_param(1, 1.0);
        assert_eq!(eng.mixer.filter(voice).map(|f| f.params()), Some(want.params()));
    }

    /// More curves than events would make FUN_00839db0 read past the event table; a refused child
    /// refuses its parent.
    #[test]
    fn a_filter_scan_past_the_events_is_refused() {
        let mut eng = engine(vec![("mod_scan", 1.0, CueBodySpec::MultiTrack(filter_cue(2)))], LONG, 2);
        assert_eq!(eng.cue_sound(m2("mod_scan"), None), Err(CueError::FilterScan { events: 1, curves: 2 }));
        let child = Automation::Kind7 { start_bits: 0, cue: m2("mod_bad") };
        let mut eng = engine(
            vec![
                ("mod_good", 1.0, CueBodySpec::MultiTrack(multi(vec![track(vec![child], vec![sound(0.0, 0)])], vec![], 1.0))),
                ("mod_bad", 1.0, CueBodySpec::MultiTrack(filter_cue(2))),
            ],
            LONG,
            2,
        );
        assert!(matches!(eng.cue_sound(m2("mod_good"), None), Err(CueError::Child { .. })), "a refused child refuses its parent");
    }

    /// A group whose `+0x14` byte is set plays through its cue's emitter: its wave takes the group's
    /// 3D parameters, and the mix gives it the distance volume of FUN_0083d3a0 at the emitter's
    /// distance to listener 0 — full to `+0x18`, silent from `+0x1C`, `1 − t^exponent` between. Without
    /// a position the same group plays 2D, at full volume.
    #[test]
    fn a_positional_group_takes_its_distance_volume() {
        let mut h = head();
        h.unknown_14 = 1;
        h.distance_exponent = 2.0;
        let body = || CueBodySpec::MultiTrack(multi(vec![track(vec![], vec![sound(0.0, 0)])], vec![], 1.0));
        let volume_at = |pos: Option<Vec3>| {
            let mut eng = engine_with(vec![("mod_3d", 1.0, body())], LONG, 6, h);
            let cue = eng.cue_sound(m2("mod_3d"), pos).unwrap();
            run(&mut eng, 2, 0.02);
            let v = eng.cue_instances(cue)[0].voice.expect("a voice");
            eng.mixer.distance_volume(v).unwrap()
        };
        assert_eq!(volume_at(Some(Vec3::new(5.0, 0.0, 0.0))), 1.0, "inside the minimum");
        let t = (40.0f32 - 10.0) / (100.0 - 10.0);
        assert_eq!(volume_at(Some(Vec3::new(0.0, 0.0, 40.0))), 1.0 - f64::from(t).powf(2.0) as f32);
        assert_eq!(volume_at(Some(Vec3::new(150.0, 0.0, 0.0))), 0.0, "past the maximum");
        assert_eq!(volume_at(None), 1.0, "2D without a position");
    }
}
