# mercs2_audio

The Mercenaries 2 audio stack, reimplemented: device backend, software mixer, dual-deck music state
machine, bank loader, voice-over arbitration and 3D positional audio.

## What it is

`mercs2_audio` owns
everything between a script-level `Sound.CueSound(...)` and int16 PCM leaving the process:

* **`sounddb` parsing** — the `'\x1d'`-tagged cue catalog that routes a cue GUID to
  `(soundbank hash, soundbank cue index)`.
* **`soundbank` parsing** — a bank's sound groups (which wave(s) a sound plays, with its category,
  distances, pitch and gain) and its cues (which group a cue plays).
* **`wavebank` decode** — turns a `LoadWaveBank` body into resident PCM clips (PCM16 + IMA-ADPCM
  decoders live here).
* **Bank encoding** — builds a bank's wavebank, soundbank and sounddb from named PCM16 cues; every
  one of the three codecs re-encodes all retail tables in `vz.wad` byte-identically.
* **Voice pool** — allocation, priority-steal when the pool is full, and a 16-state per-instance FSM.
* **Software mixer** — int32 accumulator → saturating clamp → interleaved int16, per-voice
  resampling from a clip's native rate to the mixer rate.
* **3D** — up to 4 listeners, closest-listener selection, distance attenuation, stereo pan, Doppler
  pitch, and the distance-derived start delay.
* **Categories** — per-category volume/pitch with timed fades, plus a ref-counted master duck.
* **Music** — a dual-deck crossfading state machine (states, transitions, bound cues).
* **Banks** — the 65-slot sound/wave bank load state machine with completion callbacks.
* **VO** — priority-arbitrated dialogue (`Cinematic` > `Briefing` > `Contract` > `Bounties` >
  `Freeplay`).

The whole crate runs **headless** — the mixer renders with no audio device at all — and
[`AudioEngine`] is the facade that exposes real bodies for the `Sound.*` (88 cfuncs) and `VO.*` (11
cfuncs) Lua surface.

## Where it comes from

Derived from `docs/reverse_engineer/audio_code_map.md` (+ `docs/data/audio_code_map.json`) and the
decompiled Lua corpus (`08_audio_presentation`). The original stack is two cooperating layers: **Pal**
(Pandemic Audio Library, the low-level engine) and **Pangea** (`Pg*`, the high-level message bus /
sound DB / music machine / banks). Anchors the source itself cites:

| Piece | Oracle |
| --- | --- |
| Device create | `FUN_00831b10` — DirectSound8 + EAX2–5 |
| Mixer thread | `FUN_00831ee0` (45 ms cadence) → `FUN_00836610` MixSources |
| Mix commit | `FUN_0083cbf0` — `packssdw`-saturate int32 accumulator → int16 |
| `sounddb` parser | `FUN_00835b80`; `FindCue` direct-index threshold `FUN_00835a70` |
| Voice steal | `FUN_00837830` GetLowestPrioritySound / `FUN_00837c50` StealWave |
| Instance FSM | `FUN_00836c70` (state byte at `+0x88`), Pg-level `FUN_006036c0` |
| Listeners / 3D | `FUN_00836280` GetClosestListener, `FUN_0083ade0` CalculateVolume |
| Music FSM | `FUN_0082d7a0` Transition (dual deck, states 5/4/2) |
| Banks | `FUN_00601dd0` UpdateLoads (`0x41` slots) |
| Categories | `FUN_00607960` (double-buffered pending list, ≤10 applies/frame) |

The PC build has **no hardware voice pool** — the whole Xbox `PalSoundXenonVoiceManager` family has no
PC counterpart — so voice contention is pure software priority-steal, which is what this crate models.

The three bank tables were **measured on every audio table in retail `vz.wad`** (95 wavebanks, 76
soundbanks, 77 sounddbs), and `tests/retail_banks.rs` re-encodes each one byte-identically from its
parsed records. The full byte layouts are in the module docs of `wave`, `soundbank` and `sounddb`; the
facts that matter most:

* A cue resolves through the chain the engine follows: `sounddb` entry `{guid, soundbank hash, cue
  index}` → that soundbank's cue → its one group (single-track) or, for every sound of every track, one
  of the sound's weighted groups (multi-track) → one of the group's weighted `{wavebank, wave index}`
  waves. The sounddb's third field is the **soundbank cue index**, not a wave index.
* Every table starts with the `u32` version `0x1D` (at the wavebank's `+0` too — it is not a record
  count), then the bank hash `m2(bank name)`.
* A wavebank record's data offset (`+0x20`) is **relative to the record's own start**. Blobs follow
  the record table in record order, each on a 16-byte boundary, and the body ends on one — the zero
  fill after the last blob is why 54 wavebanks run past their last blob. Embedded clips are PCM16
  (`+0x0C` = frames × channels × 2 on every one).
* Soundbank groups come in two forms (single-wave, 64 bytes; multi-wave, `0x68 + 12 × waves`), cues in
  two (single-track, 24 bytes; multi-track: tracks of timed sounds plus volume / pitch automation —
  `multitrack`).
* A multi-wave group picks its wave, and a multi-track sound its entry, by a selection mode — 0
  sequential, 1 weighted random, 2 weighted random without an immediate repeat — drawing from the
  engine's generator. `select` reproduces both from the disassembly, generator included; the engine
  seeds it from its clock at startup, so `AudioEngine::set_rng_seed` exists to make picks repeatable.
* A bank's soundbank, sounddb and wavebank ship as three entries of one block under one name hash, each
  wrapped exactly as `mercs2_formats::ucfx::build_wrapped_block` wraps a payload (one retail soundbank,
  `0xDCCF8AFA`, plays other blocks' waves and has no wavebank of its own).

Resolving every per-bank sounddb entry in `vz.wad` (1,198 cues) with every `vz.wad` bank resident:
**1,012** resolve through every path to decoded PCM (628 of them multi-track). The rest are named, never
guessed: **177** reach a wave streamed from `music.pws` / `ambience.pws`, and **9** reach two wavebanks
that live in `English.wad` — with its wavebanks resident too, 7 of those resolve and 2 reach
`vo_stream.pws`, for **1,019** resolved. The game's resident set (12 banks, 807 catalog cues) resolves
**605**. The earlier reading — third field as a wave index, `+0x20` body-relative — named a wave after a
cue that does not play it for 1,084 of the 1,198 cues.

Retail verification (game-gated on `MERCS2_GAME_DIR`; each test prints `SKIPPING` and returns when it is
unset): `tests/retail_banks.rs` here, and `mercs2_probe/tests/audio_wad_probe.rs` for the resident banks
mixed through the engine.

## Usage

Library crate — no binaries. Cue a positional sound through the real voice → mixer path:

```rust
use mercs2_audio::{AudioEngine, Listener, MixerConfig, SoundDb};
use mercs2_core::glam::Vec3;

let mut eng = AudioEngine::new(MixerConfig { sample_rate: 44100, channels: 2 });

// Route the mixer to the default output device. Returns false and stays headless
// (NullSink) if there is no device — never a hard failure.
eng.attach_output_device();

// Listener 0 at the origin (Listener::default() is inactive).
eng.set_listener(0, Listener { active: true, ..Listener::default() });

// Load a bank's cue catalog, soundbank and wavebank bodies pulled from the WAD. A body outside
// the measured layout is an error, never a partial load.
eng.set_sounddb(SoundDb::parse(&sounddb_body).expect("sounddb"));
eng.load_soundbank(&soundbank_body).expect("soundbank");
let audible = eng.load_wavebank(&wavebank_body).expect("wavebank"); // clips carrying samples

// Cue by name: hashed to a GUID, resolved sounddb → soundbank cue → group → wave, bound to
// the resident clip, 3D-panned against the closest listener.
if let Some(voice) = eng.cue_sound_by_name("sfx_explosion", Some(Vec3::new(3.0, 0.0, 0.0)), None) {
    eng.stop_sound(voice);
}

// Per frame: advance the FSMs/fades, then keep the device ring fed at wall-clock rate.
eng.tick(dt);
eng.pump(dt);

// Or render explicitly (headless — tests, servers): interleaved int16 frames.
let pcm: Vec<i16> = eng.render(2048);
```

Encode a bank of new sounds — each cue a named PCM16 clip plus explicit group and cue parameters
(fields whose meaning is not established are parameters, never invented):

```rust
use mercs2_audio::encode::{encode_bank, BankSpec, CueSpec, Pcm16, UI_PDA_OPEN_CUE, UI_PDA_OPEN_GROUP};
use mercs2_formats::hash::pandemic_hash_m2 as m2;

// One wave, one single-wave group and one single-track cue per cue. For multi-wave groups and
// multi-track cues, author waves, groups and cues separately with `encode::encode_general`.
let bank = encode_bank(&BankSpec {
    name: "mod_ui_sounds".into(),
    cues: vec![CueSpec {
        name: "mod_click".into(),
        category: "ui".into(), // must be one of the retail Mercs2Globals categories
        sound_id: m2("mod_click"),
        clip_hash: m2("mod_click"),
        pcm: Pcm16 { channels: 1, sample_rate: 44100, samples },
        group: UI_PDA_OPEN_GROUP, // retail ui_PDA_Open_01_st's group values
        cue: UI_PDA_OPEN_CUE,
    }],
})?;
// bank.wavebank / bank.soundbank / bank.sounddb: the three `data` bodies, all under bank.bank_hash.
```

Music and categories go through the same facade:

```rust
eng.add_music_state("explore", [30.0, 0.0, 0.0, 2.0, 0.0]); // params[3] = crossfade seconds
eng.add_music_state("action",  [0.0, 3.0, 0.0, 2.0, 0.0]);
eng.add_music_transition("explore", "action");
eng.bind_music_cue("action", 0, 0x2222);
eng.transition_music("action");

eng.fade_category_down("music", 0.2, 1.0);
eng.duck_master_volume(0.0); // ref-counted; unduck_master_volume releases
```

## Modules

* **`sounddb`** — the `'\x1d'`-tagged cue catalog: exact parse/serialize (cue entries, the global
  category tree and parameters), `find_cue` (direct-index below `0x401`, hashed GUID at/above),
  `find_cue_by_name`.
* **`soundbank`** — `Soundbank`: exact parse/serialize of groups and cues.
* **`multitrack`** — `MultiTrackCue`: tracks, timed sounds and their weighted entries, automation.
* **`select`** — `PalRng` and `pick`: the engine's wave / entry selection, exactly.
* **`route`** — `route`: which cues play which waves, from the tables alone (for tools).
* **`wave`** — `WavebankFile` (exact parse/serialize) + PCM16/IMA-ADPCM decoders → `DecodedClip` /
  `Wavebank`.
* **`encode`** — `encode_bank` (named PCM16 cues) and `encode_general` (waves, single- and multi-wave
  groups, single- and multi-track cues) → the three table bodies; the `UI_PDA_OPEN_*` presets and the
  retail category table.
* **`voice`** — `VoicePool`: acquire, priority-steal, the 16-state `InstanceState` FSM.
* **`mixer`** — `Mixer`: int32 accumulate → saturate int16; `SampleSource` trait, `PcmSource`
  (with resampling), `ToneSource`.
* **`spatial`** — `Listener`/`ListenerSet` (max 4), `distance_attenuation`, `stereo_pan`,
  `doppler_pitch`, `start_delay_secs`.
* **`categories`** — per-category volume/pitch fades + ref-counted master duck.
* **`music`** — `MusicStateMachine`: dual-deck crossfading, states/transitions/bound cues.
* **`banks`** — `BankManager`: the 65-slot bank load state machine with completion callbacks.
* **`vo`** — `VoManager` + `VoPriority` arbitration.
* **`backend`** — device sinks: `NullSink` (headless) and `CpalSink` (feature `device`).
* **`components`** — audio ECS components: `AudioListener`, `SoundEmitter`.
* **`engine`** — `AudioEngine`, the facade the `Sound.*`/`VO.*` bindings drive.

## Notes / gotchas

* **`device` is on by default** and the engine/game never opt out — no audio behaviour is gated
  behind a build flag. It is optional at *link* time only because `cpal` pulls `alsa-sys`, which
  needs a full i386 multiarch sysroot to cross-compile; the headless WAD CLIs (`wad_simulator`'s
  `vo_extract` / `cue_probe` / `wavebank_layout_probe`) use only the decode side (`sounddb`, `wave`)
  and build with `default-features = false`. The codecs and the encoder need no device either.
* **`CpalSink` is a faithful substitute, not a reimplementation.** The mixer reproduces the exe's
  *software* mix exactly; cpal only stands in for the DirectSound secondary buffer that the finished
  int16 frames are streamed into. **EAX 2–5 hardware reverb has no portable analog** — `Sound.SetReverb*`
  is accepted and stored but not rendered.
* **`AudioSink` is not `Send`.** The exe runs audio on one thread (the VM and mixer share the engine
  CS) and `cpal::Stream` is `!Send` everywhere; the engine is driven from one thread to match.
* **`cue_sound` fires one voice per sound** the engine's picks produce (`pick_cue`), each starting at
  its sound's start time. Multi-track **automation** (volume / pitch ramps, LFOs, parameter curves) is
  decoded but not yet applied to those voices — see `DEFERRED.md`.
* **A cue whose chain does not resolve still allocates a (silent) voice** — faithful to the exe
  allocating a voice before its wave streams in. `resolve_cue` says why (`ResolveError`): a bank not
  resident, a streamed wave, an empty choice list, an unknown selection mode, or a bad index.
* **The game must load soundbanks too.** The chain's first hop is the soundbank; a host that loads
  only wavebanks and sounddbs resolves nothing.
* **`pump()` is a no-op when headless**, so tests and dedicated servers never render into a
  discarding sink. Use `render(frames)` to pull PCM explicitly. A stall is capped at 250 ms of
  catch-up so a hitch cannot burst-render a huge block.
* **The 9 retail `return 0` stubs** (`SetSourceEnterMusic`, `AddFadeCategory`, …) stay faithful
  no-ops here.
* One `MusicStateMachine` models **one region**; the exe holds one per region. Streamed `.pws` voices
  (`OpenStreamFile`/`CloseStreamFile` record intent only), Doppler folded into the mix, and surround
  channel-gain matrices are tracked in `DEFERRED.md` — all tagged `[faithful-blocker: no]`.
* The `Sound`/`VO` Lua tables in `mercs2_script` still return `Installed::none()`; wiring them is the
  `mercs2_engine` owner's edit (see the "Binding-wiring seam" docs in `engine.rs`). Every engine body
  they need exists in this crate.
