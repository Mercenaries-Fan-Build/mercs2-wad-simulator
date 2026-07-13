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
- ~~**Sample-rate conversion**~~ **DONE** — `PcmSource::with_rate` resamples per-voice (linear interp,
  clip rate→mixer rate) so a 22 050 Hz clip plays at correct pitch into a 44 100/48 000 Hz mix, matching
  the per-wave pitch step of `PalSoundWaveDX8`. The IMA-ADPCM/PCM decoder now lives in `wave.rs`
  (ported from the retail-verified tool decoder).
- **Doppler applied to the mix** `[faithful-blocker: no]` — `spatial::doppler_pitch` is implemented and
  matches `FUN_0083ade0`; the per-voice resample step now EXISTS (`PcmSource::with_rate`), so wiring
  Doppler is just folding the doppler ratio into that step — a small follow-up.

## Voices / mixer

- ~~**Real wave-bind on cue**~~ **DONE** — `AudioEngine::load_wavebank` decodes a `wavebank` body into
  resident clips (keyed by bank self-hash + clip hash); `cue_sound` auto-binds the resident wave a cue
  routes to (`resolve_wave`: `cue.bank_hash` → resident `Wavebank`, `cue.wave_index` → its clip; fallback
  clip-hash == cue-guid). The `sounddb` layout was CALIBRATED against shipped blocks (real 12-B
  `{guid, bank_hash, wave_index}` record — the old 16-B guess read 0 cues). Verified end-to-end on
  vz.wad (`mercs2_game/tests/audio_wad_probe.rs`): 226/853 resident cues → decoded PCM, RMS > 0.
  Still deferred: the global `Mercs2Globals` catalog (extended header, not yet decoded) + streamed
  `.pws` cues (below).
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
