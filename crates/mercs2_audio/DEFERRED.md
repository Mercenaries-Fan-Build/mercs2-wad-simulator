# mercs2_audio — deferred improvements

Non-blocking improvements intentionally left for a later silo. Each is tagged `[faithful-blocker: no]`
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
- **Sample-rate conversion** `[faithful-blocker: no]` — `PcmSource` assumes waves already match the
  output rate; the exe's `PalSoundWaveDX8` mix kernels (`DAT_0198db60`, format axis PCM8/PCM16/ADPCM)
  resample per-wave. A resampler + an ADPCM decoder belong with the wave-bank decode silo.
- **Doppler applied to the mix** `[faithful-blocker: no]` — `spatial::doppler_pitch` is implemented and
  matches `FUN_0083ade0`, but the mixer does not yet vary a voice's read rate by pitch (no per-voice
  resampling). Wire it once SRC exists.

## Voices / mixer

- **Real wave-bind on cue** `[faithful-blocker: no]` — `cue_sound` allocates a voice and (given a
  `SampleSource`) mixes it, but resolving a cue's `bank_id`/`wave_index` into decoded PCM is the
  wave-bank/streaming silo's job (`FUN_00603110` load-completion, WAD streaming manager). Until then a
  cue with no supplied source is a silent (but correctly-arbitrated) voice.
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
  not yet auto-drive transitions from the faction/pursuit silo's action level. The crossfade mechanic
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
  physics/world silos to feed them.

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
