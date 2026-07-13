//! `mercs2_audio` — Audio backend, software mixer, dual-deck music FSM, banks, VO, 3D positional.
//!
//! **Silo 14** (`docs/modernization/reimplementation_parallelization_plan.md` §3), scoreboard **row 21**.
//! **Code map:** `docs/reverse_engineer/audio_code_map.md` (+ `docs/data/audio_code_map.json`).
//! **Owned Lua namespaces:** `Sound` (88 cfuncs @0xB98C98), `VO` (11 cfuncs @0xB988B0).
//!
//! The Mercenaries 2 audio stack is two cooperating layers: **Pal** (Pandemic Audio Library — the
//! low-level engine, self-naming profiler strings on PC) and **Pangea** (`Pg*` — high-level msg bus,
//! sound DB, music state machine, banks). The exe backend is **DirectSound8 + EAX2–5**
//! (`FUN_00831b10`) with a **software mixer thread** on a 45 ms cadence (`FUN_00831ee0` → `FUN_00836610`
//! MixSources); the PC build has **no hardware voice pool**, so contention is pure software
//! priority-steal (`FUN_00837830`/`FUN_00837c50`). This crate reimplements that structure faithfully
//! and headlessly; device output is a **portable cpal substitute** for the DirectSound secondary
//! buffer (see [`backend`]).
//!
//! ## Modules
//! * [`sounddb`] — the `'\x1d'`-tagged sound/cue catalog parser (`FUN_00835b80`). Layout CALIBRATED
//!   against shipped blocks: 28-B header + 12-B `{guid, bank_hash, wave_index}` entries.
//! * [`wave`] — `wavebank` record parser + PCM16 / IMA-ADPCM decoders → resident [`DecodedClip`]s.
//! * [`voice`] — voice pool, priority-steal, the 16-state instance FSM (`FUN_00836c70`).
//! * [`mixer`] — the software mixer (`FUN_00836610`): int32 accumulate → saturate int16, headless.
//!   Per-voice resampling (clip rate → mixer rate) via [`PcmSource`].
//! * [`spatial`] — 4 listeners, distance attenuation, stereo pan, Doppler, start-delay.
//! * [`categories`] — per-category volume/pitch fades + ref-counted master duck.
//! * [`music`] — the dual-deck crossfading music state machine (`FUN_0082d7a0`).
//! * [`banks`] — the 65-slot sound/wave bank load state machine.
//! * [`vo`] — priority-arbitrated voice-over.
//! * [`backend`] — device sinks: headless [`backend::NullSink`], device [`backend::CpalSink`].
//! * [`components`] — audio ECS components ([`AudioListener`], [`SoundEmitter`]).
//! * [`engine`] — [`AudioEngine`], the facade exposing the real `Sound.*`/`VO.*` bodies + the
//!   binding-wiring seam (see its module docs).
//!
//! ## Cue → audible PCM (the whole path)
//! [`AudioEngine::set_sounddb`] installs the catalog; [`AudioEngine::load_wavebank`] decodes a
//! `wavebank` body and holds its clips resident; [`AudioEngine::cue_sound`] resolves the cue, allocates
//! a voice (priority-steal if the pool is full), auto-binds the resident wave it routes to, and applies
//! 3D gains against the closest listener. [`AudioEngine::tick`] advances the FSMs/fades;
//! [`AudioEngine::render`] mixes int16 frames, and [`AudioEngine::pump`] feeds them to the device at
//! wall-clock rate (a no-op when headless). Verified end-to-end on retail data —
//! `mercs2_game/tests/audio_wad_probe.rs`.
//!
//! ## Features
//! `device` (**on by default**) links `cpal` for [`backend::CpalSink`]. No audio *behaviour* is gated
//! behind it: with the feature off — or simply with no device present — the mixer runs fully headless
//! on [`backend::NullSink`]. It exists only so the decode-side consumers (the WAD CLIs, which use just
//! [`sounddb`] + [`wave`]) can build with `default-features = false` and avoid linking `alsa-sys`,
//! which breaks the 32-bit cross build.
//!
//! Parity gaps that are *not* faithfulness blockers (EAX reverb, `.pws` stream voices, per-region music
//! machines, surround channel-gain matrices) are enumerated in `DEFERRED.md`.

pub mod backend;
pub mod banks;
pub mod categories;
pub mod components;
pub mod engine;
pub mod mixer;
pub mod music;
pub mod sounddb;
pub mod spatial;
pub mod vo;
pub mod voice;
pub mod wave;

pub use components::{AudioListener, SoundEmitter};
pub use engine::{AudioEngine, SOUND_LIB_VERSION};
pub use mixer::{Mixer, MixerConfig, PcmSource, SampleSource, ToneSource};
pub use music::{DeckState, MusicStateMachine};
pub use sounddb::{CueEntry, SoundDb, SoundDbError, SOUNDDB_TAG};
pub use spatial::{Listener, ListenerSet, MAX_LISTENERS};
pub use vo::{VoManager, VoPriority};
pub use voice::{InstanceState, Voice, VoiceId, VoicePool, VoiceRequest};
pub use wave::{DecodedClip, Wavebank};

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::glam::Vec3;

    /// Build a small synthetic sounddb: a direct-index cue plus a hashed positional cue.
    fn sample_db() -> SoundDb {
        let cues = vec![
            CueEntry {
                guid: 0x0000_0001,
                bank_hash: 0,
                wave_index: 0,
                priority: 100,
                category: 0,
                flags: 0,
                default_gain: 1.0,
                min_dist: 0.0,
                max_dist: 0.0,
            },
            CueEntry {
                guid: mercs2_formats::hash::pandemic_hash_m2("sfx_explosion"),
                bank_hash: 0,
                wave_index: 3,
                priority: 200,
                category: 1,
                flags: 0x2, // positional
                default_gain: 0.75,
                min_dist: 0.0,
                max_dist: 0.0,
            },
        ];
        SoundDb::from_cues(SOUNDDB_TAG, cues)
    }

    // ---- 1. sounddb parse (vs a real bank if present; else synthetic round-trip) ----------------

    #[test]
    fn sounddb_parse_roundtrip_and_findcue() {
        // If a real sounddb block is bundled anywhere reachable, parse it; otherwise (the usual case
        // in this worktree) exercise the parser on a synthesized block — skip-green on absence.
        let real = [
            "assets/sounddb.bin",
            "../../assets/sounddb.bin",
            "test_data/sounddb.bin",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok());

        if let Some(bytes) = real {
            let db = SoundDb::parse(&bytes).expect("real sounddb must parse");
            assert_eq!(db.version, SOUNDDB_TAG, "real sounddb version tag is 0x1D");
        } else {
            eprintln!("sounddb_parse_roundtrip_and_findcue: no real bank bundled — synthetic block");
        }

        // Synthetic round-trip of the three on-disk routing fields (the play-time fields are not on
        // disk — the exe reads them from the wave descriptor).
        let routed = SoundDb::from_cues(
            SOUNDDB_TAG,
            vec![
                CueEntry::routed(0x0000_0001, 0xBEEF, 0),
                CueEntry::routed(0x00AA_BB01, 0xBEEF, 3),
            ],
        );
        let bytes = routed.to_bytes();
        assert_eq!(bytes[0], SOUNDDB_TAG, "first byte is the 0x1D node tag");
        assert_eq!(SoundDb::parse(&bytes).expect("parse synthesized block"), routed);

        // FindCue on the in-memory sample DB.
        let db = sample_db();
        assert_eq!(db.find_cue(0).expect("cue index 0").guid, 0x0000_0001); // direct index (< 0x401)
        let hashed = db.find_cue_by_name("sfx_explosion").expect("hashed cue resolves"); // id >= 0x401
        assert!(hashed.is_positional());
        assert_eq!(hashed.wave_index, 3);

        // A non-0x1D buffer is rejected.
        let mut bad = bytes.clone();
        bad[0] = 0x1C;
        assert!(matches!(SoundDb::parse(&bad), Err(SoundDbError::BadTag(0x1C))));
    }

    // ---- 2. voice steal picks the lowest-priority voice -----------------------------------------

    #[test]
    fn voice_steal_picks_lowest_priority() {
        let mut pool = VoicePool::new(3);
        // Fill the pool with three voices of distinct priorities.
        let _a = pool.acquire(&VoiceRequest { priority: 50, ..Default::default() }).unwrap();
        let low = pool
            .acquire(&VoiceRequest { priority: 20, ..Default::default() })
            .unwrap(); // lowest
        let _c = pool.acquire(&VoiceRequest { priority: 80, ..Default::default() }).unwrap();
        assert_eq!(pool.active_count(), 3, "pool full");

        // The stealable victim must be the priority-20 voice.
        assert_eq!(pool.get_lowest_priority(), Some(low));

        // A higher-priority request steals exactly that slot.
        let stolen = pool
            .acquire(&VoiceRequest { priority: 90, cue_guid: 0xABCD, ..Default::default() })
            .expect("higher priority steals");
        assert_eq!(stolen, low, "the lowest-priority voice was reused");
        assert_eq!(pool.get(stolen).unwrap().priority, 90);
        assert_eq!(pool.get(stolen).unwrap().cue_guid, 0xABCD);

        // An equal-or-lower request cannot steal (every remaining voice outranks it) → denied.
        let denied = pool.acquire(&VoiceRequest { priority: 10, ..Default::default() });
        assert!(denied.is_none(), "cannot outrank the field — cue dropped");
    }

    // ---- 3. music FSM crossfades on a state change ----------------------------------------------

    #[test]
    fn music_fsm_crossfades_on_transition() {
        let mut m = MusicStateMachine::new();
        // params[3] (the p5 slot) is the crossfade length in seconds.
        m.add_music_state("explore", [30.0, 0.0, 0.0, 2.0, 0.0]);
        m.add_music_state("action", [0.0, 3.0, 0.0, 2.0, 0.0]);
        m.add_music_transition("explore", "action");
        m.bind_music_cue("explore", 0, 0x1111);
        m.bind_music_cue("action", 0, 0x2222);

        // Establish "explore" as the live deck (instant, no prior deck).
        assert!(m.transition("explore"));
        for _ in 0..4 {
            m.tick(1.0);
        }
        assert_eq!(m.active_deck().state, DeckState::Playing);
        assert_eq!(m.active_deck().cue, 0x1111);

        // Transition to "action": both decks must be crossfading, old down / new up.
        assert!(m.transition_declared(
            mercs2_formats::hash::pandemic_hash_m2("explore"),
            mercs2_formats::hash::pandemic_hash_m2("action"),
        ));
        assert!(m.transition("action"));
        m.tick(1.0); // half of the 2.0s fade
        assert!(m.is_crossfading(), "mid-transition both decks are live");
        let old = m.active_deck().gain;
        let new = m.inactive_deck().gain;
        assert!(old > 0.0 && old < 1.0, "old deck fading out ({old})");
        assert!(new > 0.0 && new < 1.0, "new deck fading in ({new})");
        assert!((old + new - 1.0).abs() < 1e-3, "constant-sum crossfade");

        // Finish the fade: new cue becomes the sole active deck.
        m.tick(1.0);
        assert!(!m.is_crossfading());
        assert_eq!(m.active_deck().cue, 0x2222);
        assert_eq!(m.active_deck().gain, 1.0);
        assert_eq!(m.inactive_deck().gain, 0.0);
    }

    // ---- 4. 3D attenuation falls off with distance ----------------------------------------------

    #[test]
    fn attenuation_falls_off_with_distance() {
        // Unit-level: monotonic non-increasing, full inside min, silent past max.
        assert_eq!(spatial::distance_attenuation(0.5, 1.0, 10.0), 1.0);
        let near = spatial::distance_attenuation(2.0, 1.0, 10.0);
        let far = spatial::distance_attenuation(8.0, 1.0, 10.0);
        assert!(near > far, "closer is louder ({near} > {far})");
        assert!(far > 0.0);
        assert_eq!(spatial::distance_attenuation(20.0, 1.0, 10.0), 0.0, "past max = silent");

        // End-to-end: a positional cue at two distances must render quieter when farther, through the
        // real voice→mixer path.
        let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 2 });
        // An active listener at the origin, facing +Z (Listener::default() is inactive).
        eng.set_listener(0, Listener { active: true, ..Listener::default() });
        eng.set_sounddb(sample_db());

        let render_at = |eng: &mut AudioEngine, dist: f32| -> f32 {
            eng.pool = VoicePool::new(8); // fresh pool
            eng.mixer = Mixer::new(MixerConfig { sample_rate: 44100, channels: 2 });
            let src = Box::new(ToneSource::new(440.0, 44100, 12000, 8192));
            // place the source off to the +X side at `dist`
            let pos = Vec3::new(dist, 0.0, 0.0);
            let id = eng
                .cue_sound_by_name("sfx_explosion", Some(pos), Some(src))
                .expect("cue allocates");
            // advance the FSM out of start-delay/starting into Playing
            for _ in 0..8 {
                eng.tick(0.05);
            }
            assert!(eng.pool.get(id).unwrap().state.is_audible());
            let buf = eng.render(2048);
            mixer::rms_i16(&buf)
        };

        let close = render_at(&mut eng, 3.0);
        let distant = render_at(&mut eng, 60.0);
        assert!(close > 0.0, "a nearby cue produces sound ({close})");
        assert!(
            distant < close,
            "a distant cue is attenuated: distant {distant} < close {close}"
        );
    }

    // ---- 5. facade smoke: banks, categories, VO, lib version ------------------------------------

    #[test]
    fn facade_surface_smoke() {
        let mut eng = AudioEngine::default();
        assert_eq!(eng.get_lib_version(), 12.0);

        // Banks: request → tick → complete → resident, and the callback fires.
        assert!(eng.load_sound_bank("sfx_common", Some(7)));
        eng.tick(0.016);
        eng.banks.complete_load("sfx_common");
        assert!(eng.banks.is_loaded("sfx_common"));
        assert_eq!(eng.banks.drain_callbacks(), vec![7]);

        // Categories: fade a category down, tick, observe it dropping.
        eng.fade_category_down("music", 0.2, 1.0);
        eng.tick(0.5);
        let v = eng.get_category_volume("music");
        assert!(v < 1.0 && v > 0.2, "music category mid-fade ({v})");

        // VO arbitration: cinematic pre-empts contract; freeplay cannot pre-empt cinematic.
        assert!(eng.vo_cue(1, 0xAAAA, VoPriority::Contract, true, None));
        assert!(eng.vo_cue(2, 0xBBBB, VoPriority::Cinematic, true, None));
        assert_eq!(eng.vo.active().unwrap().cue, 0xBBBB);
        assert!(!eng.vo_cue(3, 0xCCCC, VoPriority::Freeplay, true, None));

        // Master duck is ref-counted.
        eng.duck_master_volume(0.0);
        eng.duck_master_volume(0.0);
        eng.tick(0.1);
        assert!(eng.categories.master_volume() < 1.0);
        eng.unduck_master_volume(0.0);
        eng.unduck_master_volume(0.0);
        eng.tick(0.1);
        assert_eq!(eng.categories.master_volume(), 1.0, "restored when last ref released");
    }

    // ---- 6. a cue binds its resident wave and mixes to audible PCM (the last-mile wire) ----------

    #[test]
    fn cue_binds_resident_wave_and_mixes_audible() {
        let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 2 });
        let hash = 0x5FBA_3915u32; // >= 0x401 → resolves via the hashed FindCue path

        // A resident, loud, constant mono clip under `hash` (as LoadWaveBank would leave it).
        eng.add_wave(DecodedClip {
            clip_hash: hash,
            channels: 1,
            sample_rate: 22050, // native rate ≠ mixer rate → exercises the resampler
            samples: vec![8000i16; 6000],
            streaming: false,
        });
        assert_eq!(eng.resident_wave_count(), 1);

        // A sounddb whose single cue's guid == the clip hash (the one-shot-SFX fallback the cue path
        // binds on when no bank routing resolves). No explicit source is passed — the engine must
        // auto-bind the resident wave.
        let cue = CueEntry::routed(hash, 0, 0);
        eng.set_sounddb(SoundDb::from_cues(SOUNDDB_TAG, vec![cue]));

        let id = eng.cue_sound(hash, None, None).expect("cue allocates a voice");
        // Advance the voice FSM out of start/ready into Playing.
        for _ in 0..8 {
            eng.tick(0.02);
        }
        assert!(eng.pool.get(id).unwrap().state.is_audible(), "voice reached a playing state");

        let buf = eng.render(2048);
        assert!(
            mixer::rms_i16(&buf) > 0.0,
            "a cue's resident wave produced audible PCM through the real mixer path"
        );

        // A cue whose wave is NOT resident allocates a (silent) voice but binds no source — faithful to
        // the exe allocating a voice before its wave streams in.
        let other = 0x1234_5678u32;
        eng.set_sounddb(SoundDb::from_cues(
            SOUNDDB_TAG,
            vec![CueEntry { guid: other, ..cue }],
        ));
        assert!(eng.cue_sound(other, None, None).is_some(), "still allocates a voice");
    }

    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
