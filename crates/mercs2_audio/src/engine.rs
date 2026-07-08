//! `AudioEngine` — the engine-side facade the script bindings drive.
//!
//! This is the union of the Pg high-level umbrella (`PgSound::Update` **`FUN_005fa950`**) and the Pal
//! low-level umbrella (`PgSoundPlayer::Update` **`FUN_006073c0`**): it owns the sound DB, the voice
//! pool + software mixer, the category mixer, the bank manager, the dual-deck music machine, the VO
//! manager, and the listener set, and it exposes **real bodies** for the `Sound.*` (88) and `VO.*`
//! (11) Lua surface (audio_code_map.md §5, luacd 08_audio_presentation).
//!
//! ## Binding-wiring seam (how the Lua tables reach these bodies)
//! The `Sound`/`VO` luaL_Reg tables live in **`mercs2_script::bindings::{sound,vo}`** (Wave-0 E3),
//! whose `install(&Lua, &SharedHost)` closures call the engine through the `mercs2_script::EngineHost`
//! trait — the script host never touches the engine directly (crate `mercs2_script` lib docs). Silo 14
//! does **not** edit that crate. Instead:
//!   1. `mercs2_engine`'s `EngineHost` impl holds an [`AudioEngine`] (e.g. `Rc<RefCell<AudioEngine>>`).
//!   2. `EngineHost` gains audio methods (`sound_cue`, `sound_transition_music`, `vo_cue`, …) that
//!      forward 1:1 to the [`AudioEngine`] methods below.
//!   3. `bindings/sound.rs` / `bindings/vo.rs` `install` fills each `REQUIRED` cfunc with
//!      `b.real("CueSound", lua.create_function(|_, args| host.borrow_mut().sound_cue(..))?)?`.
//! Every method here is named to match its Lua binding so that mapping is mechanical. The 9 retail
//! `return 0` stubs (`SetSourceEnterMusic`, `AddFadeCategory`, …) stay faithful no-ops.

use mercs2_core::glam::Vec3;
use mercs2_formats::hash::pandemic_hash_m2;

use crate::backend::{AudioSink, NullSink};
use crate::banks::{BankKind, BankManager, CallbackId};
use crate::categories::{category_id, Categories};
use crate::mixer::{Mixer, MixerConfig, SampleSource};
use crate::music::MusicStateMachine;
use crate::sounddb::{CueEntry, SoundDb};
use crate::spatial::{self, ListenerSet, Listener};
use crate::vo::{VoManager, VoPriority};
use crate::voice::{VoiceId, VoicePool, VoiceRequest};

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
    /// Device sink (headless [`NullSink`] by default).
    sink: Box<dyn AudioSink>,
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
            sink: Box::new(NullSink {
                sample_rate: cfg.sample_rate,
                channels: cfg.channels,
            }),
            paused: false,
            survival: false,
            audio_dir: "audio/".to_string(),
        }
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
    /// gains against the closest listener. `source` is the decoded wave (from the wave-bank silo); pass
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

        let id = self.pool.acquire(&req)?;
        if let Some(src) = source {
            self.mixer.attach(id, src);
        }
        self.mixer.set_channel_gains(id, gains.0, gains.1);
        Some(id)
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
    /// // CONFIRM-LIVE: stream open is SecuROM-thunked; here it records intent (the WAD-streaming silo
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
