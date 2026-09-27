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
- **The device fold-down** `[faithful-blocker: no]` — the mixer renders the engine's 6-channel
  stream (`FUN_0083f760` creates it with channel mask `0x3F`), emitter sources included (their speaker
  gains, distance volume and Doppler are the engine's, `spatial`). Folding that stream to the
  speakers is DirectSound's work, not engine code: a stereo device here takes channels 0 and 1 and a
  mono device channel 0, as a stand-in; a 3–5 channel device is refused.
- ~~**Sample-rate conversion**~~ **DONE** — `PcmSource` steps through a clip at the wave kernel's
  32.32 fixed-point step `(freq << 32) / rate`, taking the nearest (truncated) sample, as
  `PalSoundWaveDX8`'s mix (`FUN_00839fd0`) does. The IMA-ADPCM/PCM decoder now lives in `wave.rs`
  (ported from the retail-verified tool decoder).
- ~~**Doppler applied to the mix**~~ **DONE** — traced and applied where the engine applies it:
  `FUN_0083ade0` computes an emitter source's factor (source `+0x34`,
  `1 − (relative velocity · unit direction) × DAT_00BEB460`) against listener 0; `FUN_0083b120`
  scales it by each wave's Doppler scale (wave `+0x70`, the group's `+0x28`); `FUN_00839ae0` clamps it
  to `[0.1, 2]` into wave `+0xA8`; and the frequency getter `FUN_0083e170` multiplies the wave's
  frequency by it before the kernel's step. 2D sources pass 1.0. The earlier `spatial::doppler_pitch`
  (a musical ratio clamped to `[0.5, 2]`) was not the engine's and is gone.

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
