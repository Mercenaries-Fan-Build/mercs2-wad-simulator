# mercs2_audio

The Mercenaries 2 audio stack, reimplemented: device backend, software mixer, dual-deck music state
machine, bank loader, voice-over arbitration and 3D positional audio.

## What it is

`mercs2_audio` is **silo 14** (scoreboard **row 21**) of the Mercenaries 2 reimplementation. It owns
everything between a script-level `Sound.CueSound(...)` and int16 PCM leaving the process:

* **`sounddb` parsing** — the `'\x1d'`-tagged cue catalog that routes a cue GUID to
  `(wavebank hash, wave index)`.
* **`wavebank` decode** — turns a `LoadWaveBank` body into resident PCM clips (PCM16 + IMA-ADPCM
  decoders live here).
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

Two on-disk layouts here were **calibrated against shipped WAD blocks**, not guessed:

* The `sounddb` record is a **28-byte header + 12-byte entries** `{guid, bank_hash, wave_index}`,
  reversed from the `veh_support` block. The earlier 16-byte guess read **0 cues** from every real
  block. There is no priority/category/gain/distance on disk — the exe reads those from the wave
  descriptor at play time.
* The `wavebank` clip record is 36 bytes with `data_offset` at **+32** and `data_size` at **+12**
  (bytes). Sorting clips by +32 makes every consecutive delta equal +12 for **1174/1174** clips in
  `vz.wad` and **1071/1071** in `English.wad`. The format byte's `0x02` is a sample **width**, not an
  IMA codec id — embedded clips are PCM16.

End-to-end verification against retail data lives in `mercs2_game/tests/audio_wad_probe.rs` (226/853
resident cues in `vz.wad` decode to PCM with RMS > 0).

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

// Load the cue catalog and a wavebank body pulled from the WAD.
eng.set_sounddb(SoundDb::parse(&sounddb_body).expect("sounddb"));
let audible = eng.load_wavebank(&wavebank_body); // clips carrying decoded samples

// Cue by name: hashed to a GUID, resolved in the sounddb, bound to its resident
// wave, 3D-panned against the closest listener.
if let Some(voice) = eng.cue_sound_by_name("sfx_explosion", Some(Vec3::new(3.0, 0.0, 0.0)), None) {
    eng.stop_sound(voice);
}

// Per frame: advance the FSMs/fades, then keep the device ring fed at wall-clock rate.
eng.tick(dt);
eng.pump(dt);

// Or render explicitly (headless — tests, servers): interleaved int16 frames.
let pcm: Vec<i16> = eng.render(2048);
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

* **`sounddb`** — the `'\x1d'`-tagged cue catalog: parse/serialize, `find_cue` (direct-index below
  `0x401`, hashed GUID at/above), `find_cue_by_name`.
* **`wave`** — `wavebank` record parser + PCM16/IMA-ADPCM decoders → `DecodedClip` / `Wavebank`.
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
  and build with `default-features = false`.
* **`CpalSink` is a faithful substitute, not a reimplementation.** The mixer reproduces the exe's
  *software* mix exactly; cpal only stands in for the DirectSound secondary buffer that the finished
  int16 frames are streamed into. **EAX 2–5 hardware reverb has no portable analog** — `Sound.SetReverb*`
  is accepted and stored but not rendered.
* **`AudioSink` is not `Send`.** The exe runs audio on one thread (the VM and mixer share the engine
  CS) and `cpal::Stream` is `!Send` everywhere; the engine is driven from one thread to match.
* **A cue whose wave is not resident still allocates a (silent) voice** — faithful to the exe
  allocating a voice before its wave streams in.
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
