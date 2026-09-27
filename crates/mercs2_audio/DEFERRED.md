# mercs2_audio — deferred improvements

Non-blocking improvements intentionally left for a later system. Each is tagged `[faithful-blocker: no]`
— omitting it does **not** make the current behaviour less faithful to the exe oracle
(`docs/reverse_engineer/audio_code_map.md`); it is scope/quality, not correctness. Things the exe does
that we do not yet (parity gaps) belong in the code map's confirm-live list (see §10) and are marked
`// CONFIRM-LIVE:` in-source, **not** here.

## Backend / device output

- **EAX 2–5 hardware reverb** `[faithful-blocker: no]` — the exe probes and sets EAX2/3/4/5 on the
  DirectSound buffer's `IKsPropertySet` (`FUN_00832030`, `FUN_00832470/…`, 26-env reverb table
  `DAT_01176408`). cpal has no portable reverb; environmental reverb (`Sound.SetReverb*`) is accepted
  and stored but not rendered. A software reverb (per-env comb/allpass from the 26-env table) is the
  faithful-substitute upgrade.
- **Multichannel (4/6-ch) output** `[faithful-blocker: no]` — `CreateDevice` supports 1/2/4/6 ch with
  `WAVE_FORMAT_EXTENSIBLE`; the mixer here renders the config's channel count but the pan law and
  listener-gain table (`DAT_00fc34b0`) are implemented for stereo. Surround needs the full per-listener
  channel-gain matrix.
- ~~**Sample-rate conversion**~~ **DONE** — `PcmSource::with_rate` resamples per-voice (linear interp,
  clip rate→mixer rate) so a 22 050 Hz clip plays at correct pitch into a 44 100/48 000 Hz mix, matching
  the per-wave pitch step of `PalSoundWaveDX8`. The IMA-ADPCM/PCM decoder now lives in `wave.rs`
  (ported from the retail-verified tool decoder).
- **Doppler applied to the mix** `[faithful-blocker: no]` — `spatial::doppler_pitch` is implemented and
  matches `FUN_0083ade0`; the per-voice resample step now EXISTS (`PcmSource::with_rate`), so wiring
  Doppler is just folding the doppler ratio into that step — a small follow-up.

## Voices / mixer

- ~~**Real wave-bind on cue**~~ **DONE** — the engine resolves a cue the way the tables route it:
  `sounddb` entry `{guid, soundbank hash, soundbank cue index}` → the resident soundbank's cue → every
  sound of every track → the groups they pick → the waves → the resident decoded clip
  (`AudioEngine::resolve_cue`), and a started cue picks among weighted groups and waves exactly as the
  engine does (`select`), following its volume / pitch automation (`automation`, `playback`). The three table layouts, multi-track cues included, were measured
  on all of retail `vz.wad` and re-encode byte-identically (`tests/retail_banks.rs`). Two earlier
  readings were wrong and are gone: the sounddb's third field was read as a wave index, and the
  wavebank record's data offset as body-relative (it is record-relative). Over all 1,198 retail cues
  1,012 resolve with every `vz.wad` bank resident (1,019 with `English.wad`'s wavebanks too); the rest
  reach `.pws`-streamed waves.
- **The cue filter (kind 9) needs per-source mixing** `[faithful-blocker: no]` — traced in full,
  not played. A cue whose event table carries kind 9 gives its waves a biquad low-pass filter
  (`FUN_00839db0` creates it at wave `+0x2C`; `FUN_0083f2d0`, vtable `0x00BE2678`). Every update the
  wave sets its cutoff and resonance from the cue parameter object at `+0x7C` (`FUN_0083e5c0`; that
  object's vtable `0x00BE1E60` returns cue `+0x84` / `+0x88`, the kind-9 outputs): `SetParam`
  (`0x0083F670`) maps 1 → Nyquist and 0 → 100 Hz for the cutoff, 1 → 2.0 and 0 → 0.7071 for the
  resonance term; `Process` (`FUN_0083f430`) recomputes `k = tan(π·cutoff/rate)`, `k²`, and the
  coefficients, and filters int32 samples in place. Where it runs: `MixWavesToOutput`
  (`FUN_00838850`; its SecuROM splice, emulated from the runtime dump, is only
  `mov ecx, [0x01176404]`) hands each wave to its source, whose `FUN_0083b120` mixes it into the
  source's 6-channel int32 scratch `DAT_00FC34B0` and then runs the filter over that whole buffer —
  every wave of the source mixed so far this pass, as one sequence per wave channel — before
  `FUN_0083afc0` adds the scratch into the accumulator. 2D waves share one source (`FUN_0082f110` /
  `FUN_0082f140`). This crate's mixer mixes each voice straight into one stereo accumulator, so the
  filter cannot be placed; the two `vz.wad` cues that carry kind 9 (`0xD8CE1427`, `0xF23B9836`) are
  refused. Playing them needs the source-level mix path (per-source scratch, the kernel gain math of
  `FUN_00839fd0` / `FUN_0083e970`, and the commit).
- **`.pws` stream voices** `[faithful-blocker: no]` — `OpenStreamFile`/`CloseStreamFile` record intent;
  the streamed-wave state machine (`PalSoundWaveDX8::Update` `FUN_00839870`, stream I/O mgr
  `DAT_011763f4`) that pumps `vo_stream.pws`/`music.pws`/`ambience.pws` chunks is not built here.

## Music

- **Per-faction / per-region machines** `[faithful-blocker: no]` — the exe holds one machine per
  region at `soundsys +0x48 + regionIdx*0x119C` (`Sound.ActivateFactionRegionMusic`,
  `SetRootFactionRegionMusic`). This crate models one `MusicStateMachine`; a `HashMap<region,
  MusicStateMachine>` + the active-region selector is a mechanical extension.
- **Action-level / faction-mood music drivers** `[faithful-blocker: no]` — `SetActionLevelsMusic`,
  `LockActionLevelMusic`, `SetHostilityDecayRateMusic`, faction music (`AddFactionMusic`,
  `SetFactionMusic`) and source-music playlists (`AddMusicSourcePlaylist`) are surfaced as state but do
  not yet auto-drive transitions from the faction/pursuit system's action level. The crossfade mechanic
  they feed is complete.
- **Music decks routed as mixer voices** `[faithful-blocker: no]` — deck cues + gains are exposed
  (`MusicStateMachine::decks`); binding each live deck to a `vo`/`music`-category mixer voice with its
  streamed source is the same wave-bind gap as above.

## Message bus / Pg pipeline

- **14-slot `PgSoundMessageTranslator` bus** `[faithful-blocker: no]` — `FUN_005fda10` drains 14 typed
  queues (`DAT_015386b0`). This crate calls engine methods directly (the observable result); the typed
  message bus + its event-bus tie-in (`FUN_005ed590`) is a structural nicety, and its singleton
  constructors are a confirm-live target (code map §4.2/§10.2).
- **Collision / ambience / group passes** `[faithful-blocker: no]` — `CollisionHandling` (`FUN_005fd5f0`),
  `SoundAmbience.Update` (`thunk_FUN_024f2850`), `GroupManager::Update` (`FUN_00607700`) and
  `CacheCharacters` (`FUN_00600240`) are named in the Pg tick order but not implemented; they need the
  physics/world systems to feed them.

## Category / pitch surface

- **Pitch categories & fade-category tables** `[faithful-blocker: no]` — per-category *pitch* fades are
  modelled (`Categories::set_category_pitch`), but the Lua-declared fade/pitch category *tables*
  (`MrxSoundCategories`, retail `AddFadeCategory`/`AddPitchCategory` are `return 0` stubs) are driven
  from Lua; the mode→category tables live script-side.

## Binding wiring (the seam, not a gap)

- **`mercs2_script` Sound/VO real bodies** — `bindings/sound.rs` + `bindings/vo.rs` still return
  `Installed::none()`. Filling them is the `mercs2_script`/`mercs2_engine` owner's edit (outside this
  crate's scope): add `AudioEngine` methods to `EngineHost` and forward each cfunc. See
  `engine.rs` module docs "Binding-wiring seam". Every `Sound.*`/`VO.*` engine body it needs exists
  here now.
