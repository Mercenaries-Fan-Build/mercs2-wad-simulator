//! `mercs2_audio` — Audio backend, software mixer, dual-deck music FSM, banks, VO, 3D positional.
//!
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
//! * [`sounddb`] — the `'\x1d'`-tagged cue catalog (`FUN_00835b80`): 28-B header + 12-B
//!   `{guid, soundbank hash, soundbank cue index}` entries (+ the global catalog's category tree).
//! * [`soundbank`] — the `soundbank` table: sound groups (which wave(s) a sound plays) and cues.
//! * [`multitrack`] — the multi-track cue body: tracks of timed sounds and their automation.
//! * [`select`] — the engine's weighted wave / entry selection and its random generator.
//! * [`route`] — which cues can play which waves, from the tables alone (for tools).
//! * [`wave`] — the `wavebank` table + PCM16 / IMA-ADPCM decoders → resident [`DecodedClip`]s.
//! * [`automation`] — a track's or cue's automation evaluated as the engine does (`FUN_0083b4a0`):
//!   volume / pitch ramps, LFOs and parameter curves, output-channel multipliers (kind 4) and child
//!   cues (kind 7); its sine table, and pitch → playback rate.
//! * [`playback`] — a started cue's per-frame state: instance start draws, track and cue composition,
//!   track and cue loops, sound firing and instance update.
//! * [`duration`] — a cue's `+0x0C` length (what `Sound.GetMaxDuration` returns), as retail carries it.
//! * [`encode`] — builds a bank's wavebank + soundbank + sounddb: named PCM16 cues, or waves, groups
//!   (single- and multi-wave) and cues (single- and multi-track) authored table by table.
//! * [`voice`] — voice pool, priority-steal, the 16-state instance FSM (`FUN_00836c70`).
//! * [`mixer`] — the software mixer (`FUN_00836610`), headless: per source a 6-channel int32 scratch
//!   the waves mix into (the wave kernel's gains and 32.32 step, [`PcmSource`]), each wave's filter
//!   run over it, the commit into the accumulator, then saturation to int16.
//! * [`filter`] — the kind-9 biquad low-pass filter a wave carries (`FUN_0083f2d0`).
//! * [`spatial`] — 4 listeners; an emitter source's speaker gains, distance volume and Doppler as the
//!   mix computes them against listener 0; start delay.
//! * [`emitter`] — an emitter's source holder (position, velocity) and the per-frame update that
//!   moves it with its object (`FUN_006036C0`): a jitter drawn from the game's global random state
//!   and the finite-difference velocity the Doppler factor reads.
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
//! [`AudioEngine::set_sounddb`] installs the catalog; [`AudioEngine::load_soundbank`] and
//! [`AudioEngine::load_wavebank`] hold a bank's soundbank and decoded clips resident.
//! [`AudioEngine::resolve_cue`] follows a cue through everything it can play — sounddb entry →
//! soundbank cue → every track's sounds ([`multitrack`]) → every group they can pick → every wave →
//! the resident clip. [`AudioEngine::cue_sound`] refuses what it cannot play faithfully ([`CueError`]:
//! unset curve parameters, a filter scan past the event table, a refused child cue) and otherwise
//! starts a playback ([`playback`]). Each [`AudioEngine::tick`] advances it the way the engine
//! advances a cue: sounds fire at their start times, pick their groups and waves ([`select`]), draw
//! their base volume, pitch and start delay, follow the cue's and track's automation
//! ([`automation`]), loop their tracks and the cue, and start child cues when they finish — one voice
//! per sound instance (priority-steal if the pool is full), positional ones through their emitter's
//! source (speaker gains, distance volume and Doppler against listener 0).
//! [`AudioEngine::cue_sound_on_object`] is `Sound.CueSound(emitter, cue)`: the cue plays through its
//! object's emitter, which [`AudioEngine::update_object_emitters`] moves with the object every frame
//! ([`emitter`]), so a moving object's cues are Doppler-shifted.
//! [`AudioEngine::stop_sound`] releases a cue the engine's way (tail child cues start).
//! [`AudioEngine::tick`] also advances the FSMs/fades; [`AudioEngine::render`] mixes int16 frames, and
//! [`AudioEngine::pump`] feeds them to the device at wall-clock rate (a no-op when headless). Retail
//! coverage: `tests/retail_banks.rs` here and `mercs2_probe/tests/audio_wad_probe.rs`.
//!
//! ## Features
//! `device` (**on by default**) links `cpal` for [`backend::CpalSink`]. No audio *behaviour* is gated
//! behind it: with the feature off — or simply with no device present — the mixer runs fully headless
//! on [`backend::NullSink`]. It exists only so the decode-side consumers (the WAD CLIs, which use just
//! [`sounddb`] + [`wave`]) can build with `default-features = false` and avoid linking `alsa-sys`,
//! which breaks the 32-bit cross build.
//!
//! Parity gaps that are *not* faithfulness blockers (EAX reverb, `.pws` stream voices, per-region music
//! machines, the device fold-down) are enumerated in `DEFERRED.md`.

pub mod backend;
pub mod automation;
pub mod banks;
pub mod categories;
pub mod filter;
pub mod components;
pub mod duration;
pub mod emitter;
pub mod encode;
pub mod engine;
mod le;
pub mod mixer;
pub mod multitrack;
pub mod playback;
pub mod music;
pub mod route;
pub mod select;
pub mod soundbank;
pub mod sounddb;
pub mod spatial;
pub mod vo;
pub mod voice;
pub mod wave;

pub use components::{AudioListener, SoundEmitter};
pub use encode::{encode_bank, BankSpec, CueSpec, EncodedBank, EncodeError, Pcm16};
pub use emitter::Holder;
pub use engine::{AudioEngine, CueError, CueHandle, EmitterId, ResolveError, ResolvedCue, SOUND_LIB_VERSION};
pub use mixer::{Mixer, MixerConfig, PcmSource, SampleSource, ToneSource};
pub use music::{DeckState, MusicStateMachine};
pub use soundbank::{Soundbank, SoundbankError};
pub use sounddb::{CategoryEntry, CueEntry, SoundDb, SoundDbError, SOUNDDB_TAG};
pub use spatial::{Listener, ListenerSet, MAX_LISTENERS};
pub use vo::{VoManager, VoPriority};
pub use voice::{InstanceState, Voice, VoiceId, VoicePool, VoiceRequest};
pub use wave::{DecodedClip, WaveError, Wavebank, WavebankFile};

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::glam::Vec3;
    use mercs2_formats::hash::pandemic_hash_m2 as m2;

    /// Build a small synthetic sounddb: a direct-index cue plus a hashed cue.
    fn sample_db() -> SoundDb {
        let cues = vec![
            CueEntry {
                guid: 0x0000_0001,
                bank_hash: 0,
                cue_index: 0,
                priority: 100,
                category: 0,
                default_gain: 1.0,
                min_dist: 0.0,
                max_dist: 0.0,
            },
            CueEntry {
                guid: mercs2_formats::hash::pandemic_hash_m2("sfx_explosion"),
                bank_hash: 0,
                cue_index: 3,
                priority: 200,
                category: 1,
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
        let bytes = routed.to_bytes().expect("sorted table encodes");
        assert_eq!(bytes[0], SOUNDDB_TAG, "first byte is the 0x1D node tag");
        assert_eq!(SoundDb::parse(&bytes).expect("parse synthesized block"), routed);

        // FindCue on the in-memory sample DB.
        let db = sample_db();
        assert_eq!(db.find_cue(0).expect("cue index 0").guid, 0x0000_0001); // direct index (< 0x401)
        let hashed = db.find_cue_by_name("sfx_explosion").expect("hashed cue resolves"); // id >= 0x401
        assert_eq!(hashed.cue_index, 3);

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

    // ---- 4. a positional source takes its emitter's speaker gains ---------------------------------

    #[test]
    fn a_positional_source_takes_its_emitters_speaker_gains() {
        // Listener 0 at the origin with the identity basis; the source straight along +X. The speaker
        // gains (FUN_0083d090) are then clamp01(v.x): 0.7 for front left and back left (channels 0
        // and 4), 0 elsewhere; channel 3 (LFE) is the source constructor's 0.0.
        let render_at = |dist: f32| -> Vec<i16> {
            let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 6 });
            eng.set_listener(0, Listener { active: true, ..Listener::default() });
            eng.set_sounddb(sample_db());
            let src = Box::new(ToneSource::new(440.0, 44100, 12000, 8192));
            let handle = eng
                .cue_sound_with_source(m2("sfx_explosion"), Some(Vec3::new(dist, 0.0, 0.0)), src)
                .expect("cue allocates");
            let id = eng.cue_voices(handle)[0];
            for _ in 0..8 {
                eng.tick(0.05);
            }
            assert!(eng.pool.get(id).unwrap().state.is_audible());
            eng.render(2048)
        };
        let near = render_at(3.0);
        let channel = |buf: &[i16], c: usize| buf.iter().skip(c).step_by(6).copied().collect::<Vec<i16>>();
        for c in [1, 2, 3, 5] {
            assert!(channel(&near, c).iter().all(|&s| s == 0), "channel {c} is silent");
        }
        assert!(mixer::rms_i16(&channel(&near, 0)) > 0.0, "front left carries the source");
        assert_eq!(channel(&near, 0), channel(&near, 4), "back left takes the same gain");
        // An explicit source has no group, so no 3D parameters: no distance volume (FUN_00839ae0 calls
        // FUN_0083d3a0 only for a wave whose +0x5C is set).
        assert_eq!(render_at(60.0), near, "no distance volume without a group");
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

    // ---- 6. a synthetic bank's cue name resolves through the full chain to its PCM --------------

    #[test]
    fn synthetic_bank_cue_name_resolves_to_its_pcm_and_mixes_audible() {
        use crate::encode::{encode_bank, CueSpec, Pcm16, UI_PDA_OPEN_CUE, UI_PDA_OPEN_GROUP};
        use mercs2_formats::hash::pandemic_hash_m2 as m2;

        let tone: Vec<i16> = (0..6000).map(|i| if i % 50 < 25 { 8000 } else { -8000 }).collect();
        let other: Vec<i16> = vec![-3000; 2 * 400];
        let spec = BankSpec {
            name: "mod_chain_bank".to_string(),
            cues: vec![
                CueSpec {
                    name: "mod_other".to_string(),
                    category: "sfx".to_string(),
                    sound_id: m2("mod_other"),
                    clip_hash: m2("mod_other"),
                    pcm: Pcm16 { channels: 2, sample_rate: 44100, samples: other.clone() },
                    group: UI_PDA_OPEN_GROUP,
                    cue: UI_PDA_OPEN_CUE,
                },
                CueSpec {
                    name: "mod_tone".to_string(),
                    category: "ui".to_string(),
                    sound_id: m2("mod_tone"),
                    clip_hash: m2("mod_tone"),
                    pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: tone.clone() },
                    group: UI_PDA_OPEN_GROUP,
                    cue: UI_PDA_OPEN_CUE,
                },
            ],
        };
        let enc = encode_bank(&spec).expect("encodes");

        let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 2 });
        eng.set_sounddb(SoundDb::parse(&enc.sounddb).expect("sounddb parses"));
        assert_eq!(eng.load_soundbank(&enc.soundbank).expect("soundbank parses"), 2);
        assert_eq!(eng.load_wavebank(&enc.wavebank).expect("wavebank parses"), 2);

        // The name hash is all the caller supplies; the chain does the rest.
        for (name, pcm, ch) in [("mod_tone", &tone, 1u8), ("mod_other", &other, 2)] {
            let entry = *eng.sounddb.find_cue_by_name(name).expect("sounddb routes the name");
            let resolved = eng.resolve_cue(&entry).expect("chain resolves");
            let waves: Vec<_> = resolved.waves().collect();
            assert_eq!(waves.len(), 1, "{name}: one sound, one single-wave group");
            let clip = eng.clip(waves[0].wavebank, waves[0].index).expect("resident");
            assert_eq!(&clip.samples, pcm, "{name} resolves to its own PCM");
            assert_eq!(clip.channels, ch);
        }

        let handle = eng.cue_sound_by_name("mod_tone", None).expect("the cue starts");
        assert!(eng.cue_voices(handle).is_empty(), "a cue's sounds fire on the next frame");
        for _ in 0..8 {
            eng.tick(0.02);
        }
        let id = eng.cue_voices(handle)[0];
        assert!(eng.pool.get(id).unwrap().state.is_audible(), "voice reached a playing state");
        let buf = eng.render(2048);
        assert!(mixer::rms_i16(&buf) > 0.0, "the resolved clip mixed to audible PCM");

        // Without the soundbank resident the chain stops at its first hop, with the reason.
        let mut bare = AudioEngine::default();
        bare.set_sounddb(SoundDb::parse(&enc.sounddb).expect("sounddb parses"));
        bare.load_wavebank(&enc.wavebank).expect("wavebank parses");
        let entry = *bare.sounddb.find_cue_by_name("mod_tone").unwrap();
        assert_eq!(
            bare.resolve_cue(&entry).map(|_| ()),
            Err(ResolveError::SoundbankNotResident(m2("mod_chain_bank")))
        );
    }

    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
