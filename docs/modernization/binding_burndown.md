# Lua binding burn-down — backed vs unbacked

**Status metric (honest):** all 1086 `Required` engine cfuncs across 35 namespaces are installed and
callable (`tests/binding_smoke.rs` enforces it), so the game's Lua never faults on a missing binding.
But *callable ≠ implemented*. Each binding is one of:

- **BACKED** (`b.real`) — wired to a real engine mechanism (`mercs2_ai`/`faction`/`population`/`audio`/
  `ui`/`core`/…), reads real host state, or records a real intent onto a runtime command/event log a
  system drains. **1058 / 1086** (was 393 at session start). A wrong body here is a bug.
- **UNBACKED** (`b.stub`) — **28 remain, all genuine faithful no-ops**: the PC-stripped debug menu, the
  retail asset-dump dev stubs (`Pg.Dump*`), `PrintStateMachine`/`DebugStateMachine`, and the Lua
  module-system internals. A no-op IS the correct body for these — they are not debt.

**How the surface got backed:** ~18 state-model verticals (each a real crate/host mechanism + behavioral
test — Ai, Vehicle hijack FSM, Sound banks, **HUD widget tree + markers**, render-state, CameraFx,
Inventory, combat Weapon/Fire/health/SendDamage, Pg regions/Airstrike, Human, Net session, ObjectState/
Face/Report, Player modes, seat occupancy, …), then the **command/event-log** pattern
(`EngineHost::sound_cmd`/`net_event`/`script_cmd`) for every remaining *action* cfunc (animation,
callbacks, options-menu, spawner/pursuit control, marker toggles, replicated `SendEvent_*`, DSP/music
config) — each records a real `"Ns.Verb"` + args intent the corresponding runtime system consumes.

**Still deferred to real subsystem builds (the bodies are backed; the *consumers* are the next work):**
the render passes that rasterize the HUD/FX state, a particle system for the emitters, `mercs2_net`
wire replication, and a shared combat ECS to run the reversed `mercs2_combat` solver over the recorded
damage — these drain the state/command logs the bindings now populate.

The source of truth for exactly which names are unbacked is the `b.stub(...)` blocks in
`crates/mercs2_script/src/bindings/*.rs` (the coverage report counts them per namespace). This doc
groups the unbacked surface by the **engine system that has to exist to back it**, in priority order,
so de-stub work is scheduled against real systems and not done blindly.

## The de-stub recipe (worked example: the `Ai` vertical)

Backing a namespace is a vertical, not a binding-file edit:

1. **Find/extend the mechanism crate.** `Ai` order verbs → `mercs2_ai::AiWorld::order` (posts the
   hash-addressed verb to the recovered 1024-slot ring); infractions/attitude → `mercs2_faction`
   (added `add_scripted_infraction` + `set_infraction_multiplier`); spawner tweaks →
   `mercs2_population::tweak_attached_spawners`.
2. **Add the host seam.** New `EngineHost` methods with default impls (so every crate/test still
   compiles), implemented for real on `GameScriptHost` (which now owns `FactionWorld`+`PopulationWorld`).
3. **Rewire the binding** from `b.stub` → `b.real`, forwarding to the host method.
4. **Behavioral test** driving the real Lua and asserting the mechanism changed
   (`game_lua_ai_drives_ring_and_faction`), plus bump the coverage baseline.

`Ai` result: real +31 (order ring, mood bridge, spawner tweaks). Residue below.

## Unbacked surface, grouped by required engine system (priority order)

### 1. HUD — `Hud.*` / `Gui._Marker*` — STATE MODEL DONE (rasterization pending)
✅ **Backed**: `mercs2_ui` fleshed out with a real retained-mode **`WidgetTree`** (`widget.rs`:
container/image/text/sprite/movie/flash/minimap nodes — location/color/visibility/anchoring/children/
z-order + per-kind data) and a **`MarkerSet`** (`marker.rs`: world blips/tripwires/discs/3D/objective
markers tracking a location or followed GUID). Wired through the `EngineHost::hud`/`markers` seam:
- `Hud.*` — create/delete, all widget transform+state setters+getters (`Set*`↔`Get*` round-trip),
  parenting + z-order, image texture/rotation/texcoords, text text/font/wrapping/justification/scale
  (+ width/height estimate), sprite/movie/flash/minimap create+state. (real +55)
- `Gui._Marker*` — add(5 kinds)/remove/location/color/follow/scale/pulse/blip-limit + `AddObjective`;
  `LoadTexture`/`LoadFont` as name-handles. (real +16)
Tests: `mercs2_ui` unit tests (tree parenting/z-order, marker config) + `game_lua_hud_drives_real_widget_tree`.
Residue (unbacked): the **GFx rasterization** (drawing the tree — a render pass) and the render/
callback/animation-only cfuncs (widget update callbacks, interpolation, pie-slice, clock anim, text/
sprite animation, flash VM input/callbacks, PDA blips, faction/vehicle/pickup marker CATEGORY toggles).

### 2. Presentation / FX / post — `Lti` (options menu), `CameraFx` — PARAMS DONE
✅ **Backed**: `mercs2_core::RenderState` (new `render_state.rs`) holds the params the render passes read;
wired via the `EngineHost::render_state` seam:
- `Atmosphere.*` — the generic named-parameter store (`SetValue`×109/`GetValue`, `SetColorValue`×78/
  `GetColorValue`, `SetIntValue`/`GetIntValue`) + `Begin`/`End` + time-of-day + the typed setters
  (`SetLightIntensity`/`SetAmbientColor`/scatter multipliers) routing into canonical keys.
- `Bloom.*` — all 7 HDR knobs (blur/threshold/multiplier/amount/luminance).
- `Graphics.*` — gamma, shadow base distance, screen ratio, boundary effect (`Set*`↔`Get*`).
- `Fade.*` — the four fade colors (ambient top/sides/terrain/camera).
Tests: `mercs2_core::render_state` + `game_lua_render_state_roundtrip`.
✅ **`CameraFx.*`** — the cinematic camera controller (pose yaw/pitch/fov `Set*`↔`Get*`, position/
look-at, shake, blend/stop-blend, follow-guid, hold, named shot) → a real `CameraFxState` on the host.
Residue (unbacked): the actual **render passes** (post pipeline / fade compositor / light objects — the
`rendering_fx_lighting_gap.md` work) + atmosphere preset/region/interpolation engine; `Lti.*` options
menu (resolution/gamma/refresh/input remap — needs a settings-menu backend).

### 3. Object gameplay actions — `object_state` (~8), `Object` residue, `Fire`, `Airstrike` (~11), `Weapon`, `Inventory` — HIGH
✅ **Backed**: `ObjectFilter.*` — the label boolean-expression query (`"Hero||(China&&Vehicle)"`,
recursive-descent evaluator in `mercs2_core::object_filter`) + explicit include/exclude sets + a real
per-object **label store** on the host (`Object.AddLabel`/`RemoveLabel`/`HasLabel` now actually work).
Tests: `mercs2_core::object_filter` (parser/sets/registry) + `game_lua_object_filter_evaluates_real_predicate`.
✅ Also backed: **`Inventory`** (per-character weapon loadout), **`Weapon`** (per-weapon clip/reserve
ammo + reload + primary/designator flags), **`Fire`** (per-object burning state via `Ignite`/`Extinguish`/
`Put`), **`ObjectState.SendDamage`** + object **health** (`GetHealth`/`SetHealth`/`GetMaxHealth`/`IsAlive`/
`Kill`/`Revive` now read a real health map; `SendDamage` reduces it and reports the kill). The damage/
explosion **solver is fully reversed in `mercs2_combat`** (`apply_hit`/`detonate_explosion`/`radius_falloff`)
— the next step is running it over a shared combat ECS rather than the host-side health mirror.
Residue: state-machine transitions (`SetState`/`SendMessage`), particle **emitters** (`StartEmitter`/
`StopEmitter` — need the particle system), airstrike delivery, `ObjectFilter` association/relation edges.

### 4. VO / dialogue / faces — `Face` (~7), `Vo` sequences — PARTLY DONE
✅ **Backed** against `mercs2_audio::VoManager` (via the shared AudioEngine): `VO.Cue`/`CueWithoutSubtitles`
(now really impl'd on the host — was a trait-default 0), `Cancel`/`CancelAll`/`Pause`/`PauseAll`/`Unpause`/
`UnpauseAll`/`SetCinematicMode`. Cue names hash (FNV-1a) to a stable cue guid so `Cue`↔`Cancel(name)`
match. Test `game_lua_vo_drives_real_vo_manager`.
Residue (unbacked): `AddSequence`/`RemoveSequence` (VO sequence playlists — needs a sequence model),
VO completion callbacks back into Lua, and `Face.*` facial animation (needs the facial-anim channel).

### 5. World / asset / install — `PgWorld` Spawn*/Install/Dump — PARTLY DONE
✅ **Backed**: `Pg.CreateRegion` (trigger-region registry, name→handle reuse), `Pg.ActivateAlarm`/
`ToggleAlarm` (alarm state); `Airstrike.*` — designator lifecycle (`Equip`/`Remove`/`Refill`/
`FindDesignatorOwner`) + recorded ordnance/plane spawns (`SpawnOrdnance`/`Flyby`/`ConeSpawn`/… → a
drainable airstrike-request log). Test `game_lua_pg_regions_and_airstrike`.
Residue (unbacked): model-spawn-by-asset (`SpawnWithModel`/`SpawnHomingProjectile` — need the asset DB
+ projectile system), install-manager, `GetModelBBoxExtents`/`Search`/`Describe`; `Dump*` family mirrors
retail dev stubs (genuinely-stripped, keep no-op).

### 6. Vehicle depth — `Vehicle` residue (seat transfer, ammo) — MOSTLY DONE
✅ **Backed** (`mercs2_vehicle::HijackFsm`/`TurretAim`, held per-vehicle on the host): the whole hijack
lifecycle (`HijackStart`/tank-motion/`SetHijackSuccess`/`HijackComplete`/`HijackAbort`/`HijackAbortDone`/
`CancelHijack`/`SetHijackState`), turret+rotor aim (`SetTurretPitch`/`SetTurretYaw`/`SpinHeli`), and
`RestoreHealth`. Behavioral test `game_lua_vehicle_hijack_and_turret`.
Residue (unbacked): `EnterBySeatGuid`/`TransferToSeat` need the seat-guid resolution seam; `RestoreAmmo`
needs a per-object ammo store.

### 7. Sound depth — `Sound` residue (dynamic-music model, DSP reverb/lowpass) — PARTLY DONE
✅ **Backed** against `mercs2_audio`: cue/category volume/master/music-FSM (prior), plus category
**pitch** and the whole **bank residency** family (`LoadBank`/`LoadSoundBank`/`LoadWaveBank`/
`LoadTempBank`/`LoadBankWithCallback` + the `Unload*` mirror + `RequestAmbienceBank`) → the real
`BankManager` slot table. Behavioral test in `game_lua_sound_drives_real_audio_engine`.
Residue (unbacked): the dynamic **faction/action/source-music** model (`AddFactionMusic`/`SetSourceMusic`/
playlists — needs `music.rs` extension), **EAX reverb + low-pass DSP** (no portable analog, `backend.rs`),
bank-load **completion callbacks** back into Lua (async streaming seam), and misc mode toggles.

### 8. Net / session — `Net` residue, `Socket`, `Report` — LOW (SP correct today)
Session/matchmaking/telemetry. The SP-correct defaults are already faithful (`Net.IsActive`→false path);
back against `mercs2_net` when co-op restore is wired into the reimpl.

### 9. Sys / settings / profile — `Player` profile residue — PARTLY DONE
✅ **Backed**: the `Sys.*` engine-config store on the host — `SetTimeScale`/`SetLevelName`/
`SetMasterScriptName`/`SetTutorialsEnabled`/`SetAutosaveEnabled`/`SetLuaSaveVersion`/
`SetNumberOfViewports`/`SetAssetRequestMax`/`StartSingleplayer`/`WriteToConsole`, with `Set*`↔`Get*`
real roundtrips (`GetMasterScriptName`/`TutorialsEnabled` now read the store). Test
`game_lua_sys_settings_roundtrip`.
Residue (unbacked): asset-preload/streaming controls (`RequiredAsset`/`DisableAssetPreload`/`FlushAssets`),
`AddStringDb`/`ClearStringDb` (localization DB), `PlayIntroMovies`, `SetSkipMission`; plus `Player`
profile fields. `time_scale` is stored but not yet read by the fixed-tick — wire that next.

### 10. Ai residue — perception subjects, spawn-list channels, exclusion zones, road/lane spawning (~20) — LOW
`AddSubject`/`RemoveSubject`/`ThreatPerception` (perception subject list), `SetSpawnList`/spawn-list
channel model, `SetExclusionZone`/road exceptions, `SetTrafficSpawning`/`SetRoadSpawning`/`SetLaneActive`.
Back as the perception + traffic/spawn-list models grow in `mercs2_ai`/`mercs2_population`.

## Genuinely-stripped (stub IS faithful — do not "fix")
Retail strips these to no-op/return-0 stubs; a no-op here is correct, not debt. Keep documented:
- `Debug.*` menu (PC ships the shared return-0 stub `0x006D5640`).
- `Pg.Dump*` / diagnostic dumps (retail dev-only stubs).
Everything else above is real work.
