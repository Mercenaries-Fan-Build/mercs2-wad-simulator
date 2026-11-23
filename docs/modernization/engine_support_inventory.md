# mercs2_engine vs base game — support inventory

**Purpose.** One table of record: what [`tools/wad_simulator/crates/mercs2_engine`](../../crates/mercs2_engine) (plus its spine
crates `mercs2_core`, `mercs2_script`) actually implements today, against what the shipped game's
Pangea engine supports as documented in [`docs/mercs2-pdb-analysis/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-pdb-analysis) (20 system docs),
[`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/) (231-class component registry), and [`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd) (370-script Lua corpus
and the engine-binding surface it consumes).

**Compiled 2026-07-04** from a full read of all 12 `mercs2_engine` sources + the three doc corpora.
Sibling docs this consolidates (does not replace):
- [`pangea_engine_alignment.md`](pangea_engine_alignment.md) — subsystem→crate mapping + evidence discipline (its guard rails apply here too)
- [`rendering_fx_lighting_gap.md`](rendering_fx_lighting_gap.md) — the *detailed* rendering/FX/lighting checklist; this doc only summarizes it
- [`world_streaming_spec.md`](world_streaming_spec.md) — authoritative streaming spec
- **[§5 Reassembly cross-check](#5-reassembly-cross-check-2026-07-06)** — full-corpus function-count coverage per row + non-engine substrate (from `mercs2_reassemble`)

Status legend: ✅ **BUILT** (works on retail data) · 🟡 **PARTIAL** (exists, limited — the note says how) ·
⭕ **STUB** (placeholder/no-op) · ❌ **ABSENT** (no code at all).

---

## 1. Scoreboard

| # | Subsystem | Base game (per docs) | Our engine | Status |
|---|-----------|----------------------|------------|--------|
| 1 | Render core (geometry, materials, textures) | Pimp-job frame pipeline → **one shared per-viewport scene driver `FUN_00466d40`** (collect → z → 4× shadow cascade → reflection → color → water → mirror), `Mtrl_Parse` 10-slot multi-sub-material models, page-based texture streaming w/ mip swap, precompiled `.sho` registry (FP16 HDR + R32F/D24S8 RTs) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | Forward pass, per-PRMT multi-material, BC1/BC3 + hi-mip streaming, baked vertex lighting | ✅ core / 🟡 overall |
| 2 | Lighting | Placed `LightObject` (0x97e8ee92; 0x34 = type + rgb + 9 floats) point/spot + `_pl`/`_sl`/`_pl_sl` per-pixel-light shader permutations (light-class = 4th arg to `FUN_0085ac90`, gated by `ShaderLevel`/`DAT_00dfc345`), sun/day-night (atmosphere struct), `LightAnimation` + runtime `Rt{Light,Color,Scale,Alpha}Animation` (dispatcher `FUN_00675e50`) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | 32 forward point lights + fixed sun + Blinn-Phong; **Wave-1 silo 2** added `_pl`/`_sl`/`_pl_sl` light-class permutation (ShaderLevel-gated) + spot-light set + `Rt*Animation` light-tween (`scene.rs`). Spot/anim descriptors not yet harvested from `LightObject` data (empty on retail data until the placement decode lands) | 🟡→ |
| 3 | Shadows | `ShadowBuffer` depth maps + per-mesh-type shadow VPs + `BlobShadow` · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **4-cascade 1024×4096 depth atlas** + PCF + per-cascade VP + distance-LOD caster gate + **BlobShadow** fallback (Wave-1 silo 2, `scene.rs`); chars/props cast onto baked geometry via `set_shadow`. Per-mesh-type shadow VP still single | 🟡→ |
| 4 | Particles / FX | PgFX job-parallel, `fxdict` 630 effect params, EFCT/EMTR templates, ECS spawners, ribbons · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | CPU billboard sim + COLR gradients + glow cards; templates must be fed by game code (no fxdict auto-map) | 🟡 |
| 5 | Sky / post / HDR | PgSky/PgSun/PgCloud, adaptive-luminance tone-map, bloom, motion blur, rain/underwater · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | Scattering-gradient sky + HDR → bright-pass/blur → ACES/Reinhard + bloom; exposure approximated | 🟡 |
| 6 | Decals | Projected decals as parallel job, "super decal" · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_decal`** — decaltable loader (type `0x3B0AABF8`) + bounded instance pool (spawn/age/GC/evict + fade) + `DisableDecals`/`Disable3DDecals` tags; ticked in `GameRuntime`. Projection shader + draw = render seam. 20 tests | 🟡 |
| 7 | Water | `PgWater*` heightmap/wake/reflection shaders + buoyancy · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_water`** — static watermap (`0x4D7D30C4`) XZ→wet/height query + swim-state FSM + `Buoyancy`(`0xb9659f7b`)/`WaterDrag*` force math; swim FSM ticked in `GameRuntime`. Wave-sim RTs/reflection/shaders = render seam. 22 tests | 🟡 |
| 8 | World streaming | StreamingManager pre/post-load, per-object hibernation, 2-tier terrain LOD, overlays · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | Decision core (proximity, budgets, hysteresis, per-entity distances) + GPU unload + terrain LOD swap + vz_state overlays | ✅ |
| 9 | Load areas / region cache | `PgSysPopulation` CacheIn/Out/Required region cache (distinct from hibernation) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`RegionCache` decision layer** (Wave-0 S5, `mercs2_core::streaming`): CacheIn nearest-first lump drain + CacheOut/Clear (msg types 3/7 from `FUN_005017b0`; type 0xB = confirm-live) + bounded kept-ring + rect/density select; runtime GPU wiring pending | 🟡 |
| 10 | Prop LOD / imposters | MESH/TINY imposters, `RtGenericLOD` · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | 4-tier LOD computed but informational (retail props ship ~no alt meshes); terrain swap is real | 🟡 |
| 11 | Assets / formats | WAD/UCFX, MTRL, Havok packfiles, wavelet/delta/spline anim · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | WAD/ASET/container extraction, mesh de-strip + POFF + SEGM state-mask, MTRL, wavelet decode (168/168 tests) | ✅ |
| 12 | ECS / reflection registry (Keystone A) | 232 stream-deserialized component classes, `0x9e3779b9` seed, cdbsizes pools · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | hecs world + descriptor table w/ cdbsizes budgets; **field-schema deserialization BUILT** (Wave-0 E1): `schema::parse_comp_groups` (`FUN_00654940` info/schm/data arm) + endian-aware `from_schm_body` (retail LE byte-offset = low 16, **fixed** vs the `>>16` DLC-converter bug) + `ComponentSchema::deserialize_records` (CopyFromStream analog) + `registry::register_with_fields`; verified deserializing Transform/HibernationControl/Road/FactionMarker/ModelName from real vz.wad. Open: RuntimeHealth degenerate schema + Transform's CHDR-stride-gated data record + variable-length strings (confirm-live); 4 native hot-path components vs 231 classes | 🟡→ |
| 13 | Event/RPC bus (Keystone B) | name-hash + typed-TLV (≤7 args), shared GUI/Net/AI/audio; wire router unrecovered on Xbox · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | in-memory pub/sub, hash + ≤7 typed args, 2048 deferred cap | ✅ (in-memory; wire N/A) |
| 14 | Scheduler / tick (Keystone C) | `PgSys*` 32-slot registry + framerate policies; master tick order was unknown even in exe · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | fixed-timestep `Schedule`/`Time` + `run_fixed` + **`frame::LayerStack`** (5-slot app stack, 0→4 climb + enter transitions). **BOTH live boot loops now run the recovered RunFrame 9-stage order on the `LayerStack`**: `run_game_world` (`--stream`, layers loading→GAME) + `run_scene_world_loading` (default full-world, layers frontend/menu→loading→GAME). Sim drains the **decoupled fixed-sim clock** (`Time::advance_frame`/`run_fixed`, `timescale`). Remaining seam: the **layer-4 per-system order** is still host-inline (camera→streaming→animation), not a registered list mirroring `FUN_004c9740` | 🟡→ |
| 15 | Jobs / threading (Pimp) | worker pool, lock-free a64 queue, per-CPU timers, Havok MT glue · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_jobs`** — worker pool (`pimpGetNumCpus` affinity popcount) + 96-B Job (Jobtype hash + fork/join fence) + CS-guarded ring (caps `0x10`/`0x1000`/`0x400`) + asymmetric drain; CS/multi-producer collapse to `&mut` single-threaded. No in-loop consumer yet (awaits anim/streaming submitters). 9 tests | 🟡 |
| 16 | Scripting host | Lua 5.1.2 VM, `Sys.*` table, import/inherit module system, console/debugger · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | Lua 5.4 (mlua) + compat prelude + import/dynamic_import/inherit + EngineHost seam + auto-stub tracer | ✅ host |
| 17 | Script binding surface | ~53 namespaces / ~1216 fns (Surface-B trace); see §3 · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **82 real cfunc bodies** (was 11) across Debug/Sys/Pg/Object/Player/Event/Vehicle/Sound via the `EngineHost` seam: full script-side **Event system** (Create/Persistent/Delete/Post + ScriptEvent/Timer + ObjectDeath feed + `__pump`), Player economy + character getters, Object health/labels/velocity, Sys game-state handshake, Vehicle seats/doors, Sound cue + dual-deck music FSM → `mercs2_audio`. **`binding_coverage.json` = 1004 remaining / 506 called-missing** (was 1075/575); `tests/game_hooks.rs` TDD harness = **23/26** real corpus hooks green (reds = AI + proximity condition-feed) | 🟡→ |
| 18 | Input | XDK module-level only (no per-fn list recoverable) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | data-driven 25-action set from `Mercs2.ini`, KB/mouse + gilrs gamepads, analog; streaming loop still uses its own hardcoded free-fly keys | ✅ |
| 19 | Camera | `PgSysCamera`: per-vehicle modes, shake, FOV set, collision cast, ≤5 viewports, cinematic · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | free-fly + auto-orbit + `set_view` seam + game-crate 3P cam; **Wave-1 silo 9** added the vehicle camera modes (CameraCarPreset/Tank/Turret/Helicopter, `mercs2_vehicle::camera`) | 🟡 |
| 20 | Animation | Havok hka/hkb: MT sampling w/ CPU budget, bone controllers, IK, ragdoll blend, FaceFX, GPU skin · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | wavelet/Havok decode + `hkQsTransform` compose + GPU 4-influence LBS + root-motion strip (engine path). **`mercs2_anim` built** (Wave-1 silo 8): data-driven clip picker (`ActionTable→AnimationLookup→ASTO`, live-validated vs vz.wad), `AnimController`+crossfade system, two-bone foot-placement IK. Ragdoll (gated on physics bodies)/FaceFX deferred; migrating the engine loop onto `mercs2_anim` (AnimState→AnimController) pending | 🟡→ (sampling/skinning/selection/IK ✅; ragdoll/FaceFX ❌) |
| 21 | Audio (Pal) | voices w/ priority steal, 16-state instance FSM, ducking, dynamic faction/action music, banks, 3D · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_audio` built + FULLY WIRED, audible** (Wave-1 silo 14 + last-mile): software mixer (int32→`packssdw`→i16 = the exe's mix), voice pool w/ priority-steal + 16-state FSM, dual-deck music FSM, 3D atten/pan. **Last mile closed:** cpal output device attached at boot (headless `NullSink` fallback) + mixer **pumped each frame** (`audio.pump(dt)`, real-time producer for the 45 ms mix thread); **`wavebank` IMA-ADPCM/PCM decode** ported from the retail-verified tool decoder + per-voice linear resample (clip rate→mixer rate); **`sounddb` layout CALIBRATED against shipped blocks** (28-B header + 12-B `{cue_guid, bank_hash, wave_index}` records — the old 16-B guess read 0 cues). World-load loads the 12 always-resident banks from vz.wad and merges their per-bank sounddbs; `Sound.CueSound` routes cue→bank→wave and mixes real decoded PCM. **Verified vs vz.wad**: 319 resident clips, 853-cue merged catalog, 226 cues resolve to decoded waves, real cue → RMS 25 286 (`tests/audio_wad_probe.rs`). Remaining: global `Mercs2Globals` catalog (extended header) + VO wave-bind + streamed `.pws` + EAX reverb (no portable analog) | ✅→ (mixer/decode/route/device ✅; VO+stream+EAX ❌) |
| 22 | Physics | Havok 5.5 world/MOPP/constraints/character proxy + `PgPhysicsActor*` bridge family, grapple, winch · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_physics` built + wired** (Wave-1 silo 7): `StaticSoupPhysics` impl of the `mercs2_core::PhysicsQuery` seam — exact ray/tri + closest-point + **swept-capsule character controller** (5-state OnGround/InAir/Jumping machine, tunnel-free conservative advancement) + rigid-body step. Fed the streamed collision soup, ticked in the loop. Per-frame integrator = confirm-live (VMX). Havok MOPP-BVtree/constraints/grapple deferred | 🟡→ (queries+controller ✅; Havok sim ❌) |
| 23 | AI | percept→context→goal planner (~30 verbs), cover FSM, squads, pedestrians, AI driving; pathfinding algorithm unrecovered · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** (core AI + AI driving) | **`mercs2_ai` — the recovered MECHANISM built + wired**: `AiActionBus` (1024-slot `DirectAction` ring), `perception::update_perception` (per-entity records: observers/aware/hostile), `RelationMatrix` ([-100,100] `Ai.SetRelation`), component families (AiBehavior/AiSkill/Perception/Stimulus/Target/Squad, census defaults). `Ai.Goal`/`SetRelation`/`GetRelation`/`SetState` Lua surface real (host→`AiWorld`); `GameRuntime.tick` runs perception over the world. Planner/cover FSM stay data/Lua (no native body, §8); navmesh unrecovered. 13 crate tests | 🟡 |
| 24 | Population / spawners | `PgSysPopulation`: faction spawn lists, traffic/window/path/hardpoint spawners, cache-in/out. **FULLY CODE-MAPPED Xbox↔PC** · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** — Update = Xbox `FUN_82368008` ↔ PC `FUN_00502510` (~24-call fan-out); 4 simple-spawner families (`FUN_004e4100`→quartet, cap-128 queues, terminal state 5); death check/compute (`FUN_00500b40`/`ac0`); region cache msg-pump (types 3/7/0xB, `FUN_005017b0`); spawn drain `FUN_004b4590`; full Lua surface (`Ai.TweakAttachedSpawners`, `Set/GetSpawnList`, `Pg.StartHeliWaveSpawner`, skirmish) with cfunc VAs | **`mercs2_population`** — `PgSysPopulation::Update` fan-out (death check → 4 simple-spawner families over a 768-cap pool → queue drain), density `>>1&0x7f7f7f7f` decay + `TrafficControl` gate, census components; emits `SpawnRequest`s realized through the shared `SpawnResolver` (`GameRuntime::tick_population`). Terminal spawn worker (SecuROM-VM) = the resolver seam it feeds. 27 tests | 🟡 |
| 25 | Vehicles | **CUSTOM raycast drive model (NOT Havok Vehicle SDK — zero `hkpVehicle*` on PC)**, 6 command-broadcast rings, turrets, huge tuning surface. **CODE-MAPPED Xbox↔PC** ([`docs/reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md)): nine `hkpUnaryAction` actor classes (Car `FUN_0044db60`/Boat `FUN_00447260`/Heli `FUN_00453760`/Tank `FUN_00454d80` applyActions), drive math decoded (per-axle raycast friction + linear torque falloff to MaxSpeed + donut sine-LUT), control pump `FUN_00532f80`, per-class HandleCommand `FUN_00437300`/`435790`/`441900`. **Ghidra `noreturn` on sqrt helper had truncated all physics fns — fixed, global decomp exports need regen** · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_vehicle` built + wired + proven to act** (Wave-1 silo 9): custom raycast drive (Car/Tank — per-axle friction + linear torque falloff to MaxSpeed + donut sine-LUT, wheel rays on `PhysicsQuery`), command rings + `HandleCommand`, seats/doors, `Vehicle`/`Camera` Lua bodies. Ticked via `GameplaySystems`; a throttled car **drives end-to-end** through the wired tick (test-proven). Tuning defaults (MaxSpeed/suspension) = confirm-live (stripped). Runtime spawns entities via the Lua/population path (next phase) | 🟡→ (drive ✅; live spawns pending) |
| 26 | Weapons / combat | data-driven `Weapon*` ECS family, projectile lifecycle, homing FSM, damage/explosion taxonomy (solver undecoded) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_combat` built + ticked** (Wave-1 silo 10): projectile lifecycle (gravity + swept `PhysicsQuery` raycast) + homing FSM (lock→launch→guided cross-product steering + gravity + detonation) + `wpn_*` stat loader (live-validated vs `wpn_sniperrifle`) + `Weapon`/`Airstrike` Lua bodies. Damage/explosion **solver = confirm-live stand-in** (SecuROM/string-only both builds). Ticked via `GameplaySystems`; acts on spawned weapon entities (next phase) | 🟡→ (spine ✅; solver confirm-live) |
| 27 | GUI / HUD | Scaleform GFx + `_GuiInternal` widget backend + per-player `Gui*Update` events, PDA, minimap · **[Scaleform code map ↓](#1a-pc-reverse-engineering-code-maps)** | loading screen (shell.wad plate + spinner) + 2D UI pass (`ui.rs`/`ui.wgsl`: instanced quads + 8x8 bitmap-font text over the shell pass) + GAME shell menu (`mercs2_game::menu`: main menu w/ retail option set `autoContinue`/`newGame`/load/`quitGame` + save browser over `SaveGames\*.profile`, boots the picked save; KB + gamepad nav) | 🟡 (shell menu ✅ native; Scaleform/HUD/PDA ❌) |
| 28 | Networking | Keystone-B replication (`NetSubCat*`/`NetSafe*`/`NetClient*`), join-time module pull, LIVE/XLSP · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | **`mercs2_net`** — Layer-1 replication/session: max-2 slot session + host gate (`should_replicate`), `NetMessage` (name-hash + category nibble + typed args ≤7) marshal/unmarshal, local-vs-wire branch (`FUN_005a0cc0`), category registrar (seed `0x9e3779b9`), module-pull gate (`0x762c8f61`). No in-loop consumer yet (SP boot); Layer 2/3 transports = the online-restore mod. 25 tests | 🟡 |
| 29 | Save / serialize | versioned save w/ hash + corruption handling; `SetLuaSaveVersion` · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | read ✅ + **write ✅** (Wave-0 S6, `mercs2_formats::save_write`): `write_profile` re-stamps header fields + recomputes `data_size` + **ProfileHash DERIVED = CRC-32/BZIP2 (non-reflected) over `[4:]`**, byte-exact against all 8 retail saves; round-trips `write(parse(x))==x` incl. valid hash; `set_lua_payload` re-deflates the `return{…}` blob. Open (`DEFERRED.md`): from-scratch (no-template) assembly + `SaveState`→Lua-source serializer | ✅ (read ✅ / write ✅) |
| 30 | Entity state machines | `PgModelStateMachine` + ECS STAT/SWIT destruction graph (`CollapseState`) · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | DATA LAYER DONE (2026-07-05, [`docs/destruction_orchestrator_format.md`](../destruction_orchestrator_format.md)): `orchestrator::parse_state_machine` parses the engine's named-state machine (recovered from `FUN_004cf340`); `decode_script` reads the Enter/Exit command scripts (SHOW/Hide/SetState/SetStateOnMsg/StartEmitter/StopEmitter/CreateObject/KillObjectsLinkedToHP/DisableConstraint/SetRootNode/StartAnim/SetNodePhysicsModelKeyframed); `machine_group_visibility` EXECUTES SHOW/Hide → per-group visibility (workshop-proven, interactive). Missing: ECS component + tick system, message routing (SetStateOnMsg ← damage events), CreateObject/emitter/physics command execution | 🟡 (formats ✅ / runtime ❌) |
| 31 | Destruction | `BuildingDestruction`/`DestructionLink`, pristine/ruined vz_state, debris · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | SEGM state-mask tier selection + #30's default-visibility execution (per-node init states → SHOW/Hide). NOTE: the ENGINE world path still renders ALL SWIT subtrees — spawned vehicles show floating break pieces in-game; first wiring step = apply `machine_group_visibility` defaults wherever `build_indexed_*` models reach `Scene::load_model` (prop wake + game loaders), then the runtime graph (damage msgs → SetStateOnMsg transitions → StartEmitter via the existing particle system, CreateObject debris) | 🟡 |
| 32 | Diagnostics | ~250-item debug menu, profiler zones, per-system dumps · **[PC code map ↓](#1a-pc-reverse-engineering-code-maps)** | 24 headless diag fns (anim gates, stream probe, placement/hash hunting) + 2 env flags | ✅ (different shape, serves the RE workflow) |

**One-line read (updated 2026-07-07, post Wave-1 + connection layer):** the engine is a faithful
streaming-world renderer + spine (rows 1–18 green/yellow) **plus a now-built gameplay-simulation fleet**
— physics, animation, vehicles, weapons/combat, audio (rows 20/21/22/25/26) all migrated to tested
Rust crates and **wired into the game loop's fixed tick** (`mercs2_game::gameplay::GameplaySystems`),
with the vehicle system proven to act end-to-end. **~26 of 32 subsystems now have real
implementation** (≈8 ✅ built / ≈18 🟡 partial); the 6 genuinely-unstarted are **Decals (6), Water (7),
Jobs/Pimp (15), AI (23), Population (24), Networking (28)**. The dominant *remaining effort* is not
those 6 but (a) the **connection layer** — feeding the wired systems real game entities via the
spawn/population/mission-Lua path (the fleet is tested-driven, not yet game-data-driven), and (b)
hardening the 18 partials to ✅. AI (23) still needs its planner/navmesh (data/Lua-dispatched, no
native body); its spine is now code-mapped (`ai_code_map.md`).

---

## 1a. PC reverse-engineering code maps

Deep function-level maps of the shipped **PC** engine (from the unpacked SecuROM image
`output/_ghidra/securom_dump/mercs2_unpacked.exe`), one per subsystem. Each binds the Xbox-PDB
symbol names to concrete PC addresses and records what stays vtable/SecuROM-gated as explicit
`confirm-live` (x32dbg) items. These are the RE reference for a faithful reimpl; they do **not**
change the "Our engine" status above (which tracks what `mercs2_engine` implements).

| Row | Subsystem | Code map | Machine-readable | What it recovered |
|---|---|---|---|---|
| 1 | Render core | [`render_core_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_core_code_map.md) | [`render_core_lighting_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/render_core_lighting_code_map.json) | Frame = **Pimp-job pipeline** (`FUN_0046a440` registers 6 render/stream jobtypes; RenderFrame/SubmitFrame = render-view vtbl +0x14/+0x10); **one shared per-viewport scene driver `FUN_00466d40`** (collect → z → 4× shadow cascade → reflection → fading-trees → color → water → mirror → blob); PgLtiRenderer singleton `PTR_PTR_00dfc2fc` (vtbl 0xbd3940, device +0x2d28) + main-RT creator `FUN_00749f70` (MainColor 0x71 FP16, DepthBuffer 0x73 R32F, own D24S8); shader registry `FUN_0084f130`→`FUN_0085ac90` (name,`.sho`,variant) + `.sho` loader `FUN_0085b3f0` (device CreateVS +0x16c/PS +0x1a8); `Mtrl_Parse FUN_00858790` 10-slot lazy-handle struct; page-based texture streaming `FUN_00874fb0`/`00875760`/`00875b00`/`00875cc0`; Mercs2.ini `FUN_00753280`. Renderable vtbl +0x14/+0x18(RenderShadow)/+0x1c(draw)/+0x40 + phase-list producers `.securom`-thunked → confirm-live |
| 2 | Lighting | [`lighting_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/lighting_code_map.md) | [`render_core_lighting_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/render_core_lighting_code_map.json) | `LightObject` (0x97e8ee92, `FUN_006622e0`, 0x34 = int + rgb + 9 floats, layout proven) + `LightAnimation` (0xbd5349f7, `FUN_00662ee0`, 0x2c); the 4 `Rt{Light,Color,Scale,Alpha}Animation` descriptors + master per-frame Update dispatcher `FUN_00675e50`; **`_pl`/`_sl`/`_pl_sl` = the light-class 4th arg to `FUN_0085ac90` (0/1/2/3), gated by `DAT_00dfc345` ShaderLevel**; sun/ambient via the atmosphere struct (sky map). `RenderLights`/`RenderLightBounds` = Xbox-only strings; per-draw light-class select + light-constant accumulation + RtAmbience → confirm-live |
| 3 | Shadows | [`shadow_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/shadow_code_map.md) | [`particle_fx_shadow_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/particle_fx_shadow_code_map.json) | Shadow-map RT creator `FUN_00755d90` (4-tile 1024² atlas), per-mesh-type shadow-VP selection, caster collection + distance-LOD gate, `BlobShadow` fallback, `EnableShadows` cvar chain |
| 4 | Particles / FX | [`particle_fx_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/particle_fx_code_map.md) | [`particle_fx_shadow_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/particle_fx_shadow_code_map.json) | ECS FX component family + reflection registrars, `fxdict`/effect-template loaders (`FUN_00491320`/`FUN_00492af0`, chunk→offset map, the `0x493102` crash field), shader registry, Ribbon runtime; runtime emit/`Render3DParticles` vtable-gated |
| 5 | Sky / post / HDR | [`sky_post_hdr_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/sky_post_hdr_code_map.md) | [`sky_decal_water_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/sky_decal_water_code_map.json) | Atmosphere struct + exact scatter offsets, full `Graphics.Atmosphere.*` setter→C table (`@0xb9a570`), cloud RTs (`FUN_0047d710`, R32F `mCloudOutRT`), 7-stage HDR post driver `FUN_0074f8d0` + adaptive-exposure loop; **motion-blur/velocity ABSENT on PC** |
| 6 | Decals | [`decal_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/decal_code_map.md) | [`sky_decal_water_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/sky_decal_water_code_map.json) | `decaltable` ASET loader (`FUN_004cb1b0`/`FUN_004cb1f0`, type `0x3B0AABF8` → `PgDecalTable`), decal shader register, `DisableDecals`/`Disable3DDecals` components; create/`DecalJob`/`DamageShadow` strings stripped → confirm-live |
| 7 | Water | [`water_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/water_code_map.md) | [`sky_decal_water_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/sky_decal_water_code_map.json) | Shader sub-loader `FUN_00484380` (R2VB/NVT/plain tiers), pass driver `FUN_00466d40` (wake → occlusion → reflection mirror-matrix `FUN_004677d0` → surface), ping-pong sim RTs, waterline query `FUN_00480440`, `Buoyancy` comp `0xb9659f7b` |
| 8 | World streaming | [`world_streaming_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/world_streaming_code_map.md) | — (uses `../scripts/mercs2_annotations.json` load-path fns) | Streaming spine married Xbox↔PC: manager update `FUN_00872d30` (evict→flush→tick→retry), per-frame LOD-budget notify `FUN_0084ae70` (tier 0–3 hysteresis, from master update), completion gate `FUN_008739e0` (`==4`), **busy flag `mgr+0x4c35c` = `IsLoadingOrStreaming` = the `WaitForStreaming` gate**, pre/post-load prime `FUN_004bf8c0`; terrain hi→low GUID map `FUN_004a88a0` + patch Activate/Deactivate `FUN_0066cac0`/`FUN_0066d030` + descriptors/deserializers; HibernationControl descriptor `FUN_00640a40` (**field-size 6** ⇔ parsed on-disk schema) + `Object.{Set/Get/Is/Revert}Hibernation*` cfuncs (`0x5CF4F0/…`, binding-walk validated) + population hibernate-out gate `FUN_00500b40`→`FUN_005007d0`; `_PropPhysics`/`_DebrisPhysics` descriptors + region containment `FUN_004d60e0` + region/music cfuncs. **Correction:** Xbox `OpenStreamFile`/`UpdateStreamBlocks` are audio `.pws`, not world content. Height queries + PropPhysics::Activate runtime + >400 warn = confirm-live |
| 10 | Prop LOD / imposters | [`prop_lod_imposter_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/prop_lod_imposter_code_map.md) | — | LOD runtime PC-in-the-clear (Xbox string-stripped → PC-anchored): `RtGenericLOD` generator `FUN_0066ee80` (bakes a **hard 4-tier** pre-squared `{near²,far²,handle,proxy}` band array, count@+0x40) + per-frame **annular-shell** proxy consumer `FUN_00490220` (spawn/despawn a distinct proxy entity per tier), ticked from the layer-4 `Rt*` updater `FUN_00675e50`←`FUN_004c9740` (**corrects rendering-shaders.md**: `FUN_00675e50` is the shared `Rt*` updater, not just `RtLightAnimation::Update`). MESH/TINY dispatchers `0x471900`/`0x471a01` (0x10/0x18-B index-keyed renderables) + mesh assembler `FUN_00478120`→`FUN_00478270`. **SEGM reconciled**: `{u16 bone@0, u8 seg_id@2 (the registry "group"), u8 state_mask@3 (state/LOD draw gate vs model+0x352)}`. **Retail reality**: distance-LOD is a ~128-object special case (pools `GenericLOD 128` / `RtGenericLOD·Proxy 32` vs `HibernationControl 14080`) — the real imposters are the whole-region `_tiny` satellite layers (`vz_*_tiny`, `TinyGeometryObject 0x06468e56`), not per-prop swap |
| 11 | Assets / formats | [`asset_formats_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/asset_formats_code_map.md) | — (mirrors `mercs2_formats` + [`ghidra_knowledge_inventory.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/ghidra_knowledge_inventory.md) Part C.2) | Container pipeline pinned Xbox↔PC: FFCS (INDX `page<<15` / DATA / ASET / PTHS) → sges (validator `0x005148B0` + zlib-1.2.3 inflate `0x00794700`, `-15` deflate + 64KB sentinel) → entry table → UCFX → CHDR tree. **Six parser bodies read first-hand**: entry reader `FUN_00464780` (0x14 stride), CHDR dispatcher `FUN_00654940` (**the Transform stride gate `[0x01176078]`→42/40**), placement tree `FUN_004cf340` (CHDR/NODE/**CEXE** [not CECE]/INFO/STAT/SWIT), `Mtrl_Parse 0x858790` (u16 count@+0xa2, **fixed 10 slots** = the `0x84DD5B` overrun), texture INFO/BODY `0x750a30`, STRM `0x4a4770` / IBUF `0x4a48f0`. **Keystone marriage** = the shared stream-deserialize descriptor mechanism (`&PTR_FUN_82030fa0`+`0x9e3779b9` ⇔ PC `&PTR_CopyFromStream_*`+`0x9e3779b9`, cross-build-confirmed) + Havok packfile field-binders (`FUN_0089c0c0`/`FUN_00957240`). Type-hash + FourCC reference block. Xbox parser bodies unlocated by name (identical on-disk format carries the marriage) |
| 12 | ECS / reflection registry (Keystone A) | [`ecs_reflection_registry_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/ecs_reflection_registry_code_map.md) | — | The stream-deserialize descriptor mechanism read first-hand (Keystone A). One 159-B ctor/class fills a **0x50-B descriptor** (CopyFromStream vtable@+0, flags@+8, pool cap@+0xc, name-hash@+0x10, **stride@+0x24**, seed `0x9e3779b9`@+0x2c, name@+0x3c) then tail-calls the shared registrar **`FUN_0064a770`** (+ twin `FUN_0064a782`), which appends into **one of two arrays by `flags & 8`**: authored `PTR_PTR_00edbec8` / `Runtime*` `PTR_PTR_00edbac8` (proven: RuntimeWeapon flags=8, FactionMarker/Hibernation flags=0) — marries Xbox `&DAT_83808ae0`/`&DAT_83808ee0` field-for-field. Lifecycle: name-hash `FUN_0064a7e0` (= `pandemic_hash_m2`), sort `FUN_0064a8f0`, **type-hash→descriptor `FUN_0064aa10`**, pool alloc `FUN_00672e40` (cdbsizes budgets). Consumer: COMP arm `FUN_00654940` (`info`→resolve / `schm`→field-schema / `data`→CopyFromStream) → pool insert `FUN_0064a600` → spatial reg `FUN_00665590`; object→instance resolver `FUN_005857e0`. **Open (= the engine gap)**: the `schm` field-name→offset handler is the one mechanism read-live not static. Row 12 = 🟡: table + budgets + 231-class census exist; field-schema deser NOT implemented; 4 native vs 231 |
| 13 | Event/RPC bus | [`event_bus_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/event_bus_code_map.md) | [`keystone_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/keystone_code_map.json) | Subscriber registry + dispatch recovered on PC (18-bucket per-category hash, 11-tick deferred-delete GC, `(bucket<<16)\|id16` handles, registry `DAT_00edaf88`) + Lua `Event.*` path; local-vs-wire branch stays SecuROM-virtualized → confirm-live |
| 14 | Scheduler / tick | [`scheduler_tick_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/scheduler_tick_code_map.md) | [`keystone_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/keystone_code_map.json) | Master tick recovered: RunFrame `FUN_00630ef0` 9-stage order → `FUN_004c14f0`→`FUN_004c15e0` 5-layer stack ticked 0→4; QPC decoupled fixed-sim + variable-render; corrected the Xbox "update-fn list" to per-object |
| 15 | Jobs / threading (Pimp) | [`pimp_job_system_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/pimp_job_system_code_map.md) | [`keystone_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/keystone_code_map.json) | Per-CPU worker `FUN_00876400`, 96-B Jobtype+fence+params jobs, **a64 lock-free queue degraded to CRITICAL_SECTION-guarded ring on x86**, `pimpGetNumCpus FUN_008767b0`; profiler-zone timer tree strings stripped → confirm-live |
| 21 | Audio (Pal + Pangea) | [`audio_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md) | [`audio_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/audio_code_map.json) | Full stack: 24 self-named Pal anchors, **DirectSound8 + EAX2–5 backend `FUN_00831b10` (Xbox `*Xenon`→PC `*DX8`)**, software mixer thread `FUN_00831ee0` (45 ms), dual-deck music FSM `FUN_0082d7a0`, sounddb parser `FUN_00835b80` (`'\x1d'` tag cross-verified vs Xbox), 14-slot msg bus `FUN_005fda10`, 3 tick call sites in `FUN_004c9740`, full 88-fn `Sound.*`/11-fn `VO.*` Lua tables; SecuROM-thunked cue-dispatch/stream-open → confirm-live |
| 23 | AI (perception / behavior / planner) | [`ai_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/ai_code_map.md) | — | **Core `PgAi*` framework married Xbox↔PC — the row-23 gap §5.2 flagged.** Runtime spine PC-in-the-clear: AI **host** `FUN_00590380` ↔ `PgSysAi @8240b548` (three AI objects hung off `PgSystem`+0x3ce4/+0x3cdc/+0x3ce0; singleton `DAT_01175e54`; **names the two sub-systems Xbox couldn't** — `FUN_004b2370`/`FUN_004f4190`); **hash-addressed action bus** `DirectAction FUN_0056aa70` ↔ `@823daea0` → local-post `FUN_00423d10` (**1024-slot ring `DAT_012476f0` = the "Ai 1024" budget, NOT a component**) → MP-replicate `FUN_006bb960` gated by player-count `FUN_006cdac0`; `AimedScan FUN_0054c6e0` (hashes `0x3a054530`/`0x53ac2c09`); per-entity **perception record** `FUN_0058d520` (0x64-B, observer/threat counters) + update `FUN_00600240`. **25-component census** (hash+registrar+stride: `Perception`/`Stimulus`/`AiBehavior`/`AiSkill`/`Squad`…) + master enum registrar `FUN_0064ac50` (16,897 B). **Load-bearing finding**: the planner/cover-FSM/squad tactics have **no native body on either build** — verbs are 32-bit hashes, the vocabulary lives in Lua/data (`Ai.Goal`/`Role`/`Squad` surface). **Corrections**: `PathFind` = debug-colour registrar not nav; `Ai 1024` = the action ring. Open: navmesh (unrecovered), planner goal-selection body, sub-system roles → confirm-live |
| 23 | AI driving + road graph | [`road_graph_ai_driving_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/road_graph_ai_driving_code_map.md) | [`road_graph_ai_driving_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/road_graph_ai_driving_code_map.json) | Runtime road/intersection graph **rebuilder** `FUN_004fd9f0` (road pool `PTR_PTR_017bd910` + intersection pool `PTR_PTR_017be9f0` → index tables + adjacency `FUN_004fe750`, per-element lock `DAT_01174ffc`); **corrected `06_..roads..md`: `FUN_0063d4b0` is the RoadIntersection stream-loader, not the runtime consumer**; AI driving-state producer cluster `0x544xxx–0x561xxx` (`FUN_00546ce0` driver) feeds the vehicle command rings; graph-rebuild cadence + `CarDriveRoads` fn + lane-enum semantics → confirm-live |
| 9 / 24 | Population / spawners + region cache | [`population_spawner_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/population_spawner_code_map.md) | [`population_spawner_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/population_spawner_code_map.json) | **Whole `PgSysPopulation` married Xbox↔PC** (66 fns): Update = Xbox `FUN_82368008` ↔ PC `FUN_00502510` (~24-call fan-out) reached via the layer-4 per-system list `FUN_004c9740`; UpdateSimpleSpawners `FUN_004e4100`→4-family quartet (`FUN_004e1590/1ad0/2110/1d50`, cap-128 queues, terminal state 5, ≤8 groups); death check/compute `FUN_00500b40`/`FUN_00500ac0` (budget 20/frame); **region CacheIn/CacheOut = the msg-pump `FUN_005017b0`** (decisive `msg==3/7/0xB` switch matching Xbox `FUN_82365828`) + 8-slot lump ring `FUN_00502fc0`; spawn-queue drain `FUN_004b4590`; `PopulationSimpleSpawner` is a class-**manager** `@0xDF8510` (why it's absent from the 231 flat classes); full Lua surface via walked binding tables (`Ai.TweakAttachedSpawners`/`Set/GetSpawnList`/`ResetAllSpawnLists`, `Pg.StartHeliWaveSpawner`/skirmish) with PC↔Xbox cfunc VAs. **Corrected `world-streaming.md` §30 (`FUN_82364058` = OnEntityDeleted, not Update) and `scheduler_tick_code_map.md` §7 (population slot was mis-attributed to SceneObject stubs; real Update `FUN_00502510`, and the layer-4 intra-order is the literal `FUN_004c9740` call list).** Terminal spawn worker SecuROM-thunked (`0x24F3200`) + family↔queue order + kept-ring cap (Xbox 64 vs PC 8) → confirm-live |
| 25 | Vehicles | [`vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md) | [`vehicle_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/vehicle_code_map.json) | **Drive model is a CUSTOM raycast sim, NOT the Havok Vehicle Kit** (nine `hkpUnaryAction` actors, car math decoded: per-axle friction + linear torque falloff + donut LUT); 6 broadcast-ring control transport (`FUN_00538c90`/`d20`, pump `FUN_00532f80`) + per-class HandleCommand switches (command IDs `0x3483DBF1` Turn etc.); physics-class reflection registrars + stream sizes; **found + fixed a Ghidra `noreturn`-on-sqrt defect that had truncated every physics fn**; command-ID hash + drive-tuning defaults + SecuROM enqueue islands → confirm-live |
| 26 | Weapons / combat | [`weapons_combat_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/weapons_combat_code_map.md) | — | Weapons spine married Xbox↔PC (PC bodies read first-hand): **weapon-system driver `FUN_0051cff0`** (PgWeaponSystem::Update analog, in the layer-4 list `FUN_004c9740`@0x4c9a1d beside vehicle/population, pause-gated `[+0x62]`) → homing sub-update + ~12 pooled per-`Runtime*`-type passes. **Homing/lock-on FSM fully in the clear**: driver `FUN_0052e730` → lock FSM `FUN_0052dce0` (emits `HomingLockStart/Update/Clear` + Stinger reticle) → launch `FUN_0052d120` (`HomingLaunched` + spawns RuntimeHomingWeapon) → guided-flight `FUN_0052e1f0` (cross-product steering + gravity + detonation timer). Descriptors married by name+stride+Xbox RVA: RuntimeWeapon `FUN_0063dcf0` (0x34, `@0x829f3940`), RuntimeProjectile `FUN_0063dda0` (0xa0), RuntimeHomingWeapon `FUN_00645e30` (0x54), RuntimeExplosion `FUN_0066ae30` + full static gun-stat table (26 `wpn_*` blocks; `RateOfFire 120`/`iClipSize 30` defaults). **Two corrections**: the brief's "RuntimeWeapon updaters" `FUN_00666f00`/`006670e0` are instance **serializers**, not the firing solver; the Xbox "VMX128 ballistics" gap does **not** carry to PC (x86 decompiles). **The one wall**: the per-hit damage/explosion **solver** (`ApplyDamage*`/`UpdateExplosions`/`PhysicsCreateExplosion`) is string-only/SecuROM-thunked on both builds; `DamagePerson FUN_005e0720` is a notoriety-**scoring** serializer, not the applier → confirm-live. Ordnance surface = `Airstrike.SpawnOrdnance/ConeSpawn/Flyby` (not `Munitions`). Row 26 = ❌ (no combat layer); this map = the reimpl target |
| 27 | GUI / HUD (Scaleform) | [`scaleform_gfx_class_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/scaleform_gfx_class_map.md) | [`scaleform_gfx_function_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/scaleform_gfx_function_map.json) | Scaleform GFx **2.0.48** (Flash 8/AS2); 2,596-function lib map (`0x75F000–0x812000`) + engine PgScaleform HAL + FlashWidget bridge; SecuROM trampolines resolved; all 83 `.gfx` movies extracted to `output/gfx_movies/` |
| 29 | Save / serialize | [`save_serialize_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/save_serialize_code_map.md) | — | Save write-side spine married Xbox↔PC (PC bodies read first-hand): Lua `SaveSingleton` seam → **serialize driver `FUN_005a4520`** (`SaveData`/`InitialSaveData` handler; source = profile/economy singleton `[0x1176054]+0x470`; **big-endian** `ntohl` blob; payload is `return {…}` Lua **source**, not bytecode; under CS `DAT_01174ffc`) → **hash-dispatched per-object serializer `FUN_00874150`** (`HashTable_Probe(0x100)`→serializer vtable `+8`, the write-side mirror of `CopyFromStream`) → zlib codec `FUN_0075b070`. On-disk `.profile` = 13,404-B LE header (`ProfileHash`@0x00 / `version==4`@0x04 / `data_size`@0x08 / hero@0x4D / upgrade@0x4F) + zlib@0x468. Corruption/autosave dispatchers read (`FUN_00614080` raises `hasCorruptedSave 0x32ff679b`/`hasAutosave`; `FUN_00614540` case `0x5a06a0a6` gates autosave on `[0x1176054]+0x11/+0x25f`). **Honest correction**: audit §3.15's "CERTAIN" cfunc VAs (`SaveGame 0x7B8AC4`/`SetLuaSaveVersion 0x5E6120`/`saveProfile 0x7BC628`…) are registration/binding-table anchored, **not** decompiled bodies. **Write side landed 2026-08-07** (Wave-0 S6, `mercs2_formats::save_write`): `ProfileHash` DERIVED = **CRC-32/BZIP2 (non-reflected)** over `[4:]` ([[profile-hash-is-crc32-bzip2]]), `write_profile` round-trips byte-exact vs all 8 retail saves. The native `saveProfile` disk-write body remains unlocated in the dump but is structural context, not a blocker. Row 29 = ✅ read ✅ / write ✅ |
| 30 / 31 | Entity state machines + Destruction | [`state_machine_destruction_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/state_machine_destruction_code_map.md) | — | **One system, married Xbox↔PC** (destruction = the state machine's primary consumer). Parser `FUN_004cf340` (INFO/NODE/STAT/CHDR/CEXE/SWIT → named-state machine). **Net-new runtime cluster**: per-object update `FUN_006696a0` → health-producer/initializer `FUN_004cfed0` (opens the machine in **InitState `0x0ACE072A`** or **InitDestroyedState `0x5D308F4F`** from a spawn-dead flag — the `0x52628825` delta read *literally* from code), Activate `FUN_004b67b0`→`FUN_004d3f00` (0x2A8-B instance, pool `&PTR_PTR_00df9310`), force-destroy `FUN_004d05c0`, and **`SetState(machine,guid,state) FUN_004d3e10` — an indexed indirect jump through the `_DAT_024556d4` data jump-table** (case table in SecuROM data; Enter-script interpreter + `OnStateChange` Lua fire read live in the unpacked image — [[securom-decompiled-not-a-blocker]], not a wall); damage/destroy message consumer `FUN_0059be00` (matches **DestroyMsg `0x1ED7AD78`** + `0x3D0D4C99`). Descriptors: BuildingDestruction `FUN_00642590`, DamageKey `FUN_006616c0`, StateMachine `FUN_00641aa0`, ObjectScript `FUN_006424e0`, RuntimeHealth/NodeHealth producers = `FUN_004cfed0`. **Cracked 11/14 hashes**: state vocab `{Init,Start,∅}×{Pristine,Damaged,Destroyed}+State` + `GoneState`/`DetachState`; messages `DamageMsg 0xC6507EE1`/`DestroyMsg`. vz_state pristine↔ruined = a separate *persistent overlay* mechanism, not a per-frame transition. Open: 2 state + 1 msg + 1 cmd hash, `TickDamage`/`RuntimeSoundRuinKey` PC bodies → confirm-live |
| 32 | Diagnostics | [`diagnostics_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/diagnostics_code_map.md) | — | **Asymmetric by design** (that IS the finding): the Xbox **Profile** build ships a fully-wired ~250-item debug/cheat menu (registrar `FUN_82279978`, 27-toggle template flipping `DAT_836dba4x` bools, F-keys `EnableFunctionKeys`); **PC retail strips it** — the Lua-facing debug family (`Printf`/`DumpAssets`/`SetTraffic/Road/SidewalkSpawning` + ~60 more) collapses to a **shared `return-0` stub `0x006D5640`** (`33 C0 C3`, read-confirmed; `Debug.Printf` ptr `0x00B9882C`→stub). Survivors: the profiler-zone machinery — `Hash_String FUN_00824270` (= `pandemic_hash_m2`, the PC twin of Xbox FNV marker `FUN_8290ba80`) + `Hash_Probe FUN_008242b0` (256-bucket) + per-CPU timer tree (`FUN_008243a0`/`Time_NowMs FUN_008763c0`) — but **zone-name strings stripped** → confirm-live; and `Sys.WriteToConsole` (real, non-stub). **Cheats = Lua** (`_G.Cheat.DisplayOptions`; effects via native `Object.SetInvincible`/`SetInfiniteAmmo`) — closes the debug-cheat-menu open question. Engine row 32 = ✅ different-shape (24 headless RE diag fns + loadprobe/pmc_bb/lua_trace/workshop/mercs2_probe belt) |
| ★ cross-cutting | Faction / reputation | [`faction_reputation_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/faction_reputation_code_map.md) | — | **No scoreboard row — spans AI (23) / population spawn-lists (24) / HUD (27) / music (21)**; the whole loop married Xbox↔PC. Hostile act → `Ai.AddInfraction` → **combat→faction mood bridge `FUN_005e0720`** (read first-hand: serializes the 7 infraction keys `DamagePerson/DestroyPerson/DamageObject/DestroyObject/Hijack/Trespassing/SpecialEvent` = the **exact** set+order `mrxfactionmanager.lua:1212` weights ×3/×50/×1/×25/×10/×20, clamp ≥ −60) → Lua `ChangeRelation` → **`Ai.Set/GetRelation`** matrix `[-100,100]` → `Event.Post("Attitude")` → attitude levels (Hostile `[-100,-33)` no-buy / Neutral `[-33,33)` 1.5× price / Friendly `[33,100]` 1.0×) + pursuit "heat" (Level 0..3, `SetPursuitLevelTimes(120,300)`) + HUD meter + faction-locked music (`ActivateFactionRegionMusic 0x5E1CC0`). **Faction\* ECS descriptors married both builds**: FactionMarker `FUN_00641340`/`FUN_0065c0f0` (id), FactionValue `FUN_00641830`/`FUN_0065c7d0` (float), FactionZone `FUN_006414b0`/`FUN_0065c490`, RtFactionZone. §3 = the authoritative attitude/price/mood policy spec. The `Ai.*`/`Pg.*Pursuit*`/`Net*Faction*` cfunc bodies are binding-table-only (recover via the walk); `xPgSysNetFactionRelations`/`ApplyCachedFactionRelations` PC bodies = open. This map = the reimpl target |
| 16 / 17 | Scripting host + binding surface | [`scripting_host_binding_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/scripting_host_binding_code_map.md) | — | **Host (16, PC-anchored)**: stock **Lua 5.1.2 / float**; module-system bootstrap `FUN_005a2c40` registers the `_SYS` C-closures (table `@0x00B9A854` — new find) then compiles+runs `_G._MODULES={}` via `luaL_loadbuffer FUN_00860240` → `lua_pcall FUN_0085df50` (the pair every inline chunk uses); C-API VAs pinned; `luaU_undump`/`luaL_register` VAs unlocated by name (both builds). **Binding surface (17)**: the master index — **60 `.rdata` `luaL_Reg` tables** (53 game namespaces, ~1216 cfuncs) with table VA + count + namespace; ~120 cfunc bodies VA-recovered across the sibling maps (cited); the shared `return-0` stub `0x006D5640` swallows ~60+ dev bindings; `String.GetHash` = `Hash_String FUN_00824270`. **Honest**: Xbox host has 0 Lua strings → all PC-anchored (one Xbox fact = the `FUN_8241cbf0` 261-caller arg-fetch ABI); ~1100 binding-only bodies → forcing-script walk. Row 16 = ✅ (mlua Lua 5.4) / Row 17 = 🟡 thin slice; priority by count (Hud 114 / Player 107 / Net 92 / Object 87) |
| 18 | Input | [`input_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/input_code_map.md) | — | **Asymmetric (like row 32)**: Xbox = XDK XInput-over-XAM via `xam.xex` (module-level, no body). **PC = DirectInput8, recovered first-hand**: create/acquire `FUN_0082ba90` (`DirectInput8Create`→`CreateDevice`→`SetDataFormat`→`SetCooperativeLevel(6)`→`SetProperty(BUFFERSIZE=16)`→`Acquire`), poll `FUN_0082aae0`/`FUN_0082aa60` (`GetDeviceData`+`GetDeviceState`) with the **`DIERR_INPUTLOST 0x8007001E` re-acquire loop** (the fingerprint); per-frame tick `FUN_004fc0c0` (from the layer-4 pump `FUN_004c9740`) walks the action table + pressed/held/released FSM; input-code→is-down resolver `FUN_0082ab60` (KB `0x09–0x100` / mouse `0x180–0x600` / gamepad `0xf00–0xf1a`). Message pump `FUN_006315f0` = PeekMessage only (**no WM_INPUT**). **PC gamepad = DirectInput joystick, not XInput** (no XInput import in the decomp). Row 18 = ✅ (25-action set built); remaining seam = the streaming loop's hardcoded free-fly keys |
| 19 | Camera | [`camera_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/camera_code_map.md) | — | **Render seam fully in the clear**: render-view singleton `PTR_PTR_00dfc2f8` (obj `0x017CFAF0`) holds a **≤5-slot camera array, stride `0xE80`**, indexed by active-viewport `view+0x2B92`; per-viewport loop **`FUN_0085a9e0`** (render-view vtable +4) builds each view/proj from the camera **source transform `cam+0x10`** (SecuROM builder `FUN_00858f30`, read live) then drives the scene pass `FUN_00466d40`. **Two `PgSysCamera` sub-systems reconciled**: the gameplay side is a *second* ≤5-viewport array (**stride `0x620`, cap 5**, selector `FUN_0070f560` = byte-for-byte Xbox `CameraCollisionCastRay @825ea110`'s 5×0x620 loop); post-render notify `FUN_0062ef00` → two cam/shake writers `FUN_0060f6d0`/`FUN_0060fee0` (= Xbox sub-objs @owner+0x1ec0/+0x1ec4). **Full camera-struct offset map** (`+0x10` src xform / `+0x48` FOV / `+0xab0` view / `+0xaf0` proj / `+0xb20` world pos) — **live-confirmed from the freecam capture**. Modes = ECS components (CameraCarPreset/Tank/Turret/Helicopter/HumanCameraModifier/CameraShake); `Camera.*` cfuncs via pose accessor `FUN_0042ee10`. Open: the sim-side `cam+0x10` pose writer + the collision-cast caller → confirm-live. Row 19 = 🟡 |
| 20 | Animation | [`animation_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/animation_code_map.md) | [`animation_code_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/animation_code_map.json) | **PC keeps full RTTI on the Havok anim classes** (`0x0087xxxx–0x009fxxxx`) → runtime pinned first-hand, no SecuROM chase: `hkaAnimatedSkeleton` ctor `FUN_00884190`, `hkaRagdollInstance FUN_0088c000`, `hkaSkeletonMapper::createMapping FUN_0087c560`, **foot-IK `hkaFootPlacementIkSolver` ctor `FUN_009ef650`**, **FaceFX face-graph evaluator `FUN_00686ce0`** (src `…\mercs2\fxs`), catch-fall/getUp binder `FUN_008a7280`, referencePose builder `FUN_0089d280` (48-B hkQsTransform/bone), Human anim reflection registrars `FUN_0065ade0`/`FUN_0065af90`. **Consolidated the SOLVED anchors (cited)**: clip picker ActionTable `0x6802C321`→AnimationLookup `0xE00B080C`→`ASTO[idx]`→clip (e2e-validated); wavelet/delta/spline codec (168/168); BLENDINDICES per-group palette; HIER `trnm` track→bone. **Open**: the per-frame VMX/SSE sample-and-blend math (vtable-dispatched in layer 4), the MT `AnimCpu*Job` dispatch, the FaceFX curve solver → confirm-live. Row 20 = 🟡 (sampling/skinning ✅; controllers/IK/ragdoll/FaceFX ❌) |
| 22 | Physics | [`physics_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/physics_code_map.md) | — | **Highest-confidence map in the set** — `hk*` RTTI survives on BOTH builds, so `symbol-map.md`'s Xbox-RTTI→PC-ctor pairing is vtable-proven (204/287 classes). Full **Havok 5.5** stack embedded. World spine: `hkpWorld FUN_008d8340`, MT sim `FUN_008f75e0`, `addAction FUN_008dae30` (READ), deferred-op queue `FUN_008f8af0` (READ), broadphase `hkp3AxisSweep FUN_009347e0`. **Character controller fully decoded** (the biggest recovery): on-foot = a real `hkpCharacterProxy` (ctor `FUN_0094f2c0`) + **5-state machine** (OnGround `FUN_0094ce90` / InAir `FUN_0094d7b0` / Jumping `FUN_00951ef0`) via `hkpCharacterContext FUN_0094d2e0`; game builder `FUN_004255c0` (`HumanPhysics::Activate`, 7 capsules + phantom + proxy). Query layer `getClosestPoints FUN_008db880`/`getPenetrations FUN_008dba60` (READ). Shapes/MOPP/heightfield mapped (real terrain = `hkpSampledHeightFieldShape FUN_00a0e3d0`); ragdoll builder `FUN_009463a0` + `RagdollController FUN_00648130`; vehicle actors cited to the vehicle map. **Open**: the per-frame integrator (`hkpWorld::step`) is VMX128/SSE, does not decode on either build (outputs only); grapple/winch tow is constraint-driven but the 44-B `Winch` layout → confirm-live. Row 22 = ❌ sim (engine has a terrain-heightmap capsule only) — this map = the reimpl target |
| 28 | Networking | [`networking_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/networking_code_map.md) | — | **The final map — completes the 32-row scoreboard.** 3-layer stack married Xbox↔PC. **(1) Game-side `Net*` replication = the event-bus wire branch**: marshal core `FUN_005a0cc0` (READ) → SecuROM-VM encode/emit (`thunk_FUN_02935000`/`024f28e0`, read live) + remote islands `FUN_007002d0`→`_DAT_0244fb3c` / `FUN_0059ddb0`→`_DAT_0245dc0c`; **on-wire packet format recovered** from the Xbox receive decoder `NetEventCallback @825d3ce8` = `{u32 name-hash}{4 arg type-tags}{header byte: category<<4 \| argc}{argc words}` — **closes pangea_alignment's "wire protocol not known"**; category registrar `NetCategoryInfo FUN_00644510` (`NetPrimaryCategory` + 8 `NetSubCat*`); join-time module pull `SynchNetImportModule @825ce918` (module-hash `0x762c8f61` gate). **(2) Session/transport**: `CNetworkManager` `FUN_009e010a`/`FUN_009dff40`, matchmaking `FUN_009ba4b0`, Havok `hkBsdSock` Winsock-2.2 peer mesh (`FUN_009cf970`/`FUN_009cfa10`/WSAStartup `FUN_0089c710`) + NETAPI32 LAN. **(3) FESL** (the working online-restore reference): B-version `FUN_008445d0` (ver ^ `0x6B3C35EB`), CA key `.rdata 0x768378` (128B, patch-proven), tlsterm. **★TLS verdict (§7)**: the game's FESL client is a **statically-linked OpenSSL 0.9.8d** (SSLv3/RC4, ClientHello `03 00`) — **SSLv3/TLS-1.0-locked by the library** (no TLS 1.2/1.3 code to enable, no constant to flip); the game↔proxy leg is **loopback** so SSLv3 is harmless → correct architecture = keep SSLv3 on loopback + modern TLS on the proxy↔internet upstream (tlsterm can do this today). Boundaries: Massive ad-net + LIVE/XLSP = dead services (replace-don't-port); the `Net*` replication payloads = the Keystone-B reimpl target. Row 28 = ❌ in-engine; the online-restore mod is the separate working reference |

**Common method + caveats** (shared across these maps): the retail PC build strips most profiler-marker
symbol strings and dispatches per-frame render/tick through vtables, so *setup* code (shader/RT/
reflection registration, format parsers, Lua setters, cvars) is high/med confidence while the
*per-frame draw/dispatch* is often recorded as a vtable addr + slot and flagged confirm-live rather
than guessed — the same honesty model as the SecuROM gaps. Reconciliation note: `FUN_00466d40` was
mapped as both `Water::Render` (#7) and the shadow caster-collection driver (#3); the render-core map
(#1 §5) now **resolves this** — it is the single shared per-viewport scene-pass driver that both
render-actions (and the z/color/shadow-cascade/reflection sub-passes) route through.

---

## 2. The gameplay-simulation gap, quantified

What "❌ gameplay layer" means concretely, from the doc corpora:

- **ECS coverage:** 4 native components (`Transform`, `ModelRef`, `AnimState`, `SkinPalette`) vs
  **231 registry classes** documented in [`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/) (34 combat, 25 AI, 31 controllers/physics,
  23 player/vehicle, 34 presentation/audio/FX, 30 world/roads, 35 gameplay/health/mission, 7 misc,
  12 render-pipeline). The registry mechanism exists (descriptors + pool budgets) but cannot yet
  deserialize field schemas, so none of the 231 stream in.
- **Minimal-playable ECS core** (per the ecs survey): `SceneObject`+`Model`, `Health`/`RuntimeHealth`,
  `ObjectScript`+`StateMachine`, `WeaponProjectileBase`, `ControllerPlayer`+`PhysicsActor`,
  `Equipment`, `FactionMarker`, `Perception`/`AiSkill`/`AiBehavior`, regions + `RuntimeLayerId`.
- **Lua binding coverage:** the corpus depends on ~20 engine namespaces (§3); we implement real
  bodies in 4 (`Debug`, parts of `Sys`, `Pg`, `Object`) plus a fake `Event.Create`. Everything the
  game Lua does — missions, economy, HUD, AI orders, audio, support drops — blocks on the rest.
- **The load-gate seam:** `Sys.RequestGameState("WaitForStreaming"/"WaitForTether")` +
  `Event.GameStateChange` + `MrxLayerManager.Add` is how game Lua drives world-load (the exact
  markers loadprobe scores). Our streaming runtime can satisfy this — the binding glue is what's
  missing.

---

## 3. Lua → engine binding surface (what game scripts require vs what we provide)

Consolidated from [`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd) (all 8 docs; DLC adds no new namespaces). ✅ = real body in
`mercs2_script::register_engine`, ⭕ = no-op stub installed, ❌ = nothing (auto-stub layer logs it).

| Namespace | Game usage (top functions) | Ours |
|---|---|---|
| `Debug` | Printf/Print | ✅ |
| `Sys` | **RequestGameState**, RequestAutosave, GetLevelName, StartWithResources, GuidToString, SetAssetRequestMax | 🟡 GetLevelName/GetMasterScriptName/StartWithResources only |
| `Pg` | GetGuidByName, Spawn, FastCollect\*, Load/UnloadAsset, SaveGame/LoadGame, Contract\*, AddContextAction | 🟡 GetGuidByName/Spawn only |
| `Object` | GetHealth, IsAlive, Get/SetPosition/Yaw, HasLabel, Kill, FadeOut, ApplyImpulse, SetInfiniteAmmo, SetHibernationDistance, OpenGate | 🟡 SetName/SetPosition/SetYaw/GetPosition/GetYaw real; Attach/DisablePhysics ⭕ |
| `Event` | Create/CreatePersistent/Delete/Post + ~18 event types (GameStateChange, ObjectProximity/Death/InSeat, Boundary, Timer…) | ⭕ constants + handle counter, **no event loop** |
| `Ai` | Goal, Role, Deploy, Set/GetRelation, SetPriorityTarget, LivingWorld | ⭕ Enable only |
| `Vehicle` | GetDriver/GetRiders, Enter/Exit, SetParts, OpenDoor, GetSeatParams | ⭕ EnableTurret only |
| `ObjectState` | GetLinkGuid, SetState, SendDamage, **StartEmitter/StopEmitter** | ❌ (engine has `Scene::fx_start/fx_stop` ready to back the emitter half) |
| `Player` | Get/SetCash, Get/Set/AddFuel, FuelCapacity, GetPrimaryCharacter, VehicleDisguise | ❌ |
| `Hud` / `Pda` / `Gui` / `Marker` / `_GuiInternal` | full HUD/PDA widget + per-player `Gui*Update` event surface | ❌ |
| `Sound` / `VO` | CueSound, banks w/ callbacks, category fades, AddMusicState/Transition/BindMusicCue, VO priorities | ❌ |
| `Airstrike` / `Munitions` | Flyby, SpawnOrdnance, designators | ❌ |
| `Net` | IsServer/IsClient, SendCustomEvent, SendEvent_\* (support, revive, PDA, fanfare), SetLoadingScreen | ❌ |
| `Human.Inventory`, `Weapon`, `Camera`, `String.GetHash`, `Graphics`, `DangerousBuilding` | SetAllWeapons, SetReserveAmmo, GetYaw, GetHash, InitTinyGeometry, SetRarity | ❌ |

The auto-stub `_G` metatable (Surface-B tracer) means unimplemented namespaces log-and-continue
rather than crash — the corpus can already be *executed* for coverage measurement, just not *obeyed*.

---

## 4. Notes and traps carried forward

- **Evidence discipline** is inherited from `pangea_engine_alignment.md` §4: all `Pg*` behavior is
  string-inferred (no RTTI), the VMX128 numeric cores (hit solver, anim/audio/shader math) are
  undecoded on Xbox — behavior-gate against the exe, never claim the math. **EXCEPT (PC, 2026-07-06):**
  the **vehicle drive model IS decoded** (custom raycast sim — [`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md),
  row 25); the old "undecoded" was a Ghidra noreturn-on-sqrt truncation, not VMX128 — re-check other PC
  cores under that fix (see row 22 note) before calling them undecoded.
- **Rows that look done but have known seams:** streaming's c3 building-`Model` (0x5b724250)
  residency is deliberately disabled (authored placement unrecovered — `worldutil.rs:316-322`);
  SEGM byte-3 is treated as a state mask but the registry calls it `group` (reconcile vs the SEGM
  consumer decomp before building on it); the HDR path is gated on `sky_enabled` and falls back to
  direct present.
- **Loop unification (2026-07-06):** BOTH live boot loops now run on the recovered Keystone-C spine —
  `mercs2_core::frame::LayerStack` (the 5-slot 0→4 application stack, `FUN_004c15e0`) with the per-frame
  body in RunFrame's 9-stage order (`FUN_00630ef0` §2). `game_world::run_game_world` (`--stream`) maps
  its old `loading` bool onto layers loading→GAME and drains its streaming+animation sim on the shared
  `Time` accumulator (dropped the private `anim_dt`). `world::run_scene_world_loading` (the default
  full-world boot) maps its three phases — shell menu / loading / in-game — onto the frontend→loading
  →GAME layers (its `menu.is_some()` + `loading` bool gates are gone), keeping its `Schedule::run_fixed`
  animation tick. **Still open:** (1) the **layer-4 per-system order** (camera → streaming → animation)
  is host-inline, not yet a registered system list mirroring `FUN_004c9740`; (2) the free-fly cam in
  `run_game_world` still reads hardcoded keys, not the `input.rs` action layer (`world.rs` already
  uses it).
- **Retail-data realities:** props ship essentially no Havok clips (prop-anim layer is correct but
  idle on retail data) and ~no alternate LOD meshes (2/446) — so rows 10 and 20's "partial" is
  partly the *data's* fault, not the code's.

---

## 5. Reassembly cross-check (2026-07-06)

Generated by [`tools/wad_simulator/crates/mercs2_reassemble`](../../crates/mercs2_reassemble) (memory `mercs2-reassemble-tool`), which
joins every attribution source we have — the §1a code maps, Ghidra **RTTI** vtables/namespaces
(`ExportFuncClass.java`), **FID** byte-signatures (MSVC/openssl via `FidApplyExport.java`), the **Lua
binding surface** (`binding_map.json`), and [`scripts/mercs2_annotations.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/scripts/mercs2_annotations.json) — onto the
27,077-function decomp **by virtual address**, then propagates the residue by address-locality +
call-graph. Output: `output/engine_reassembled/` (gitignored) — per-subsystem `<module>.c`,
`MANIFEST.json/.csv`, `CLASSIFICATION.md/.csv`, `UTILITY_REPORT.md`, `HARDCORE_UNCLASSIFIED.csv`.

**Evidence is graded — never conflate a propagated guess with a decompiled fact:**

| Tier | Meaning | Count | % |
|---|---|---|---|
| **evidence** | code map / annotation / recovered symbol / RTTI / FID / Lua-binding | 7,036 | 26.0% |
| **signal** | decisive in-body string/import/class token | 217 | 0.8% |
| **inferred** | address-locality / call-graph / securom-region (**probabilistic scaffold**) | 18,405 | 68.0% |
| **artifact** | Ghidra bad-disassembly (data-as-code) — excluded | 85 | 0.3% |
| **unclassified** | real code, no signal — the deeper-RE core | 1,334 | 4.9% |

**4,229 functions carry a real recovered name** (was ~1,614 from headers alone). 94.8% of the corpus
carries a subsystem label; the honest hard core is the **1,334 unclassified** (mostly Havok
free-functions reached only through vtables — `HARDCORE_UNCLASSIFIED.csv` + a ready x32dbg coverage
script await a runtime trace). *Trust the `verified+signal` column below as the floor; `total`
includes probabilistic inference.*

### 5.1 Per-row function counts (`verified+signal / total`)

| Row | Module(s) | v+s / total | Row | Module(s) | v+s / total |
|---|---|---|---|---|---|
| 1 Render core | `render_core` | 118 / 1453 | 17 (w/16) | — | — |
| 2 Lighting | `lighting` | 16 / 71 | 18 Input | `input` | 7 / 31 |
| 3 Shadows | `shadow` | 5 / 20 | 19 Camera | `camera` | 25 / 231 |
| 4 Particles/FX | `particle_fx` | 46 / 163 | 20 Animation | `animation` | 115 / 508 |
| 5 Sky/post/HDR | `sky_post_hdr` | 47 / 186 | 21 Audio | `audio` | 117 / 787 |
| 6 Decals | `decal` | 7 / 139 | 22 Physics | `physics` | 947 / 2902 |
| 7 Water | `water` | 23 / 123 | 23 AI | `road_graph_ai_driving` | **14 / 76 ⚠** |
| 8 World streaming | `world_streaming` | 37 / 388 | 24 Population | `population_spawner` | 65 / 776 |
| 9 Region cache | `region_cache` | 5 / 23 | 25 Vehicles | `vehicles` | 122 / 872 |
| 10 Prop LOD | `prop_lod_imposter` | 9 / 38 | 26 Weapons | `weapons_combat` | 117 / 866 |
| 11 Assets/formats | `asset_formats`+`asset_load` | 77 / 578 | 27 GUI/HUD | `gui_hud_scaleform` | 2391 / 2618 |
| 12 ECS/reflection | `ecs_reflection_registry` | 10 / 61 | 28 Networking | `networking` | 133 / 1981 |
| 13 Event bus | `event_bus` | 13 / 165 | 29 Save | `save_serialize` | 15 / 392 |
| 14 Scheduler | `scheduler_tick` | 33 / 612 | 30/31 State/Destruction | `state_machine_destruction` | 23 / 360 |
| 15 Jobs/Pimp | `pimp_job_system` | 21 / 369 | 32 Diagnostics | — | 0 / 0 |
| 16/17 Scripting | `scripting_host_binding` | 692 / 1419 | ★ Faction (cross-cut) | `faction_reputation` | 7 / 146 |

### 5.2 AI (row 23): spine mapped, planner is data

**Now mapped** ([`ai_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/ai_code_map.md), 2026-07-07). The core `PgAi*`
framework is married Xbox↔PC: AI **host** (`FUN_00590380`↔`PgSysAi`), the **hash-addressed action bus**
(`DirectAction FUN_0056aa70` → 1024-slot ring → MP-replicate), per-entity **perception records**, and
the **25-component census** + master enum registrar `FUN_0064ac50`. The reassembly counter still shows
only the driving sub-graph (`road_graph_ai_driving`, 76 fns) clustered because AI carries **no RTTI
class family** and the spine functions attribute to generic sim/registry clusters — re-run
`mercs2_reassemble` to fold the new anchors in.

**The load-bearing finding is a negative one:** the **goal planner / cover FSM / squad tactics have no
native body on either build** — AI actions are 32-bit **hashes** dispatched over the bus, and the verb
vocabulary (`moveto`/`takecover`/…) + cover states live in **Lua/data**, not compiled AI code. So
row 23 is not "un-mapped code" but "the engine is a mechanism; the brain is authored content." The two
remaining true code gaps are **pathfinding/navmesh** (unrecovered — `PathFind` is a debug-colour
registrar, not nav) and the planner goal-selection body. *(Row 32 Diagnostics reads 0/0 but that is
**expected**, not a miss — PC retail stripped the debug menu to the shared `0x6D5640` return-0 stub,
per row 32.)*

### 5.3 Non-engine substrate (~7,300 fns ≈ 27% — not Pangea engine, absent from the scoreboard)

The reassembly surfaced the third-party/runtime layers beneath the 32 game subsystems. These are **not
missing engine work** (you'd link the real libraries), but they account for ~27% of the 27k, so "the
engine" is really **~20k functions**:

| Module | v+s / total | What it is |
|---|---|---|
| `crt` | 1176 / 2577 | MSVC C runtime |
| `cpp_runtime` | 481 / 2001 | C++ EH / RTTI / STL runtime |
| `havok` | 247 / 1200 | Havok **base library** (hk* math/mem/containers) — distinct from the physics *sim* (row 22, hkp*) |
| `securom_drm` | 0 / 1250 | SecuROM DRM / VM / crypto (address-region label ≥0x01000000) |
| `scaleform_gfx` | 92 / 276 | Scaleform GFx **base library** (G* classes) — distinct from the GUI/HUD *integration* (row 27) |

**Re-run** `cargo run -p mercs2_reassemble --release -- --corpus <notes checkout>` after any new code map, RTTI/FID export, or game
trace lands (it wipes+rebuilds `out_dir`); these counts refresh from `CLASSIFICATION.csv`.

---

## 6. Cross-system seam gaps (the connection backlog) — 2026-07-07

§1's scoreboard rates each subsystem **in isolation** (is the mechanism built?). This section rates the
**edges between subsystems** — the interconnected links an actual playthrough depends on. It is the
output of a systematic 5-cluster read-only audit (boot/persistence · world/streaming · actors ·
render/FX · infra), every claim grep-verified against the source, not the mechanism crates' self-reports.

**Governing finding.** The reimpl is a set of correct, individually-tested **mechanism islands**. The
*edges between them are mostly unwired*, and — decisively — the path the player actually runs
(`mercs2_game::world::run_scene_world_loading`, reached by the menu boot and the direct `.profile`
boot) is where the wiring is thinnest. The **decode/mechanism layer runs far ahead of the
consume/wire layer**: contract, missions, flow-state, vz_state overlays, spot-light cones, and swim are
all decoded/built and even printed in the boot banner, yet nothing consumes them. The archetype the
audit was seeded with — *"the save encodes the player's position/contract but the game boots to a
hardcoded spawn"* — is not a one-off; it is the **shape of ~30 edges**.

### 6.1 The three keystone gaps (each gates many others)

Most individual gaps below collapse into three missing connectors. Close these and a large fraction of
the table closes with them.

- **K1 — no persistent mission-Lua host in the game loop.** `run_scene_world_loading` calls
  `run_interior_boot()` **once** to harvest spawn intents, then drops the `GameScriptHost` and runs
  **zero script** thereafter. So in the running game there is no `Event.__pump`, no `Event.Post`, no
  `TimerRelative`, no mission Lua, no runtime `Pg.Spawn`, and the Lua `Sound.*` cues hit a throwaway
  audio engine instead of the audible one. *Gates:* boot seams (mission flow, spawns), infra seams
  (event pump, audible Lua sound), and the AI action ring's only real driver. The `take_new_spawns()` /
  `realize_spawns()` / `GameScriptHost::audio()` methods built this session are the **half-wired sockets
  waiting for exactly this** — make the host loop-resident and pump it each frame.
- **K2 — two disconnected world paths; the playable one bypasses streaming.** `run_game_world`
  (`--stream`) drives `StreamingManager` but is free-fly (no player/collision/gameplay). The default
  `run_scene_world_loading` has the player + fleet tick but does a **one-shot 400 m `load_world_data`
  bubble** and never instantiates `StreamingManager`. *Gates:* world streaming during play (walk out →
  empty world), vz_state overlays in the playable path, streamed collision, and terrain-height→physics.
- **K3 — the actor-realization archetype is missing.** `SpawnResolver` (`spawn.rs`) only builds
  `Prop` (bare `Transform`) or `Vehicle`; there is no `Character` archetype. A spawned NPC therefore
  carries **none** of `Perception`/`Stimulus`/`Target`/`AiFaction`/`AiBehavior`/`Health`/`Inventory`/
  `HumanAnimationSet`, and `SpawnRequest.faction` is silently dropped. *Gates:* AI perception, the
  faction loop, combat death, and animation for every population/`Pg.Spawn` actor at once.

Two cross-cutting **anti-patterns** also recur and are worth naming:
- **EventBus with no subscribers.** Combat posts `DestroyMsg`/`DamageMsg`; `GameplaySystems`
  `dispatch_all()` drains them; **zero subscribers are registered** in-game. Kills fire into the void →
  no body retire, no faction hit, no `PerceptionRecord.attackers` (hardcoded `0`), no Lua
  `Event.ObjectDeath` (the two buses are disjoint — `fire_object_death` is called only from the
  `Object.Kill` binding, never from the damage solver).
- **Mechanism ticks wired to nothing on both ends.** `runtime.decal.update` ages an always-empty pool
  (no combat producer, no render pass consumer); `runtime.water.tick` no-ops forever (`set_watermap`
  never called); `mercs2_jobs` and `mercs2_net` are **orphan crates** (not a dependency of anything).

### 6.2 Master seam-gap table (gap rows; WIRED edges omitted)

Status: **⛔ MISSING** (not connected) · **⚠ PARTIAL** (one side / dev-path only / starved). "K" = the
keystone (6.1) it collapses into.

| Cluster | Seam (Producer → Consumer) | Status | K | Gap / player-visible consequence |
|---|---|---|---|---|
| Boot | Save → player spawn **transform** | ⛔ | — | Player world pos+yaw **not even decoded** from the SaveSingleton Lua; `spawn_pos` is a constant. Every save boots to the same spot. |
| Boot | Save contract/missions → **mission flow** | ⛔ | K1 | Fully decoded + printed, **no consumer**; no mission system → no objectives/triggers. World is a diorama. |
| Boot | Save `vz_state` layers → **world-state** | ⚠ | K2 | Applied only on `--stream`; the playable boot takes no overlays arg (`add_layers` is a `{}` stub). No destruction/faction/act staging in-game. |
| Boot | In-game state → **autosave/quicksave OUT** | ⛔ | — | `save_write` can stamp a valid `.profile` but is **never called**. Persistence is read-only; progress never saved. |
| Boot | New Game → char-select → first mission | ⛔ | K1 | New Game hardcodes Mattias/tier-0, boots the diorama; no character-select, no opening contract. |
| Boot | Save costume byte → wardrobe look | ⚠ | — | Costume file byte unlocated → `costume_byte=0`; wardrobe override never fires (cosmetic). |
| World | Streaming manager → **playable world** | ⚠ | K2 | Streaming runs only in free-fly `--stream`; gameplay boot is a static bubble. |
| World | Placement → **gameplay components** | ⚠ | K3 | Placements realize render-only (`Transform+ModelRef`); `SpawnResolver.by_template` is empty. Nothing streamed becomes a vehicle/destructible/actor. |
| World | Terrain heightmap → **fleet physics** | ⚠ | — | `StaticSoupPhysics` gets `set_collision(tris)` but **never `set_heightmap`**; vehicles/raycasts have no terrain → cars on open ground fall through. |
| World | Streaming → **collision** → physics | ⚠ | K2 | `collision_tris` built once at boot, excludes terrain+props; two parallel collision reps (player `Vec` vs fleet `StaticSoupPhysics`) can desync. |
| World | Ground cover (`scrub`) | ⚠ | — | Drawn from `ScrubObject` placements with retail's cull, fade, blend and vertical shear ([spec §7](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/scrub_ground_cover_format.md)); `PgScrubLight*VP` wind is not applied; its clock and sway are decoded in spec §7. |
| World | LOD-budget governor → streaming | ⚠ | — | `GlobalLodGovernor` built but `set_lod_budget` **never called** → tier always 0, dead code. |
| World | Destruction state → **mesh swap** | ⛔ | — | SEGM `state_mask` selected with a hardcoded bit; no state→mesh swap. Nothing can be destroyed. |
| Actors | Population/`Pg.Spawn` → **actor archetype** | ⛔ | K3 | No `Character` archetype; NPCs spawn as inert factionless props (see K3). |
| Actors | Combat damage → **faction** (infraction→attitude→pursuit) | ⛔ | — | `mercs2_faction` is an empty scaffold. Shooting a faction never makes it hostile; no wanted/price/HUD/music. |
| Actors | Combat death → **population retire / perception** | ⛔ | — | EventBus has no subscribers → killed NPCs never enter `DeathQueue` (budget leak); `attackers` stays 0. |
| Actors | AI driving → **vehicle command rings** | ⛔ | — | No `AiDriving` component; `pump_car_ring`/`pump_boat_heli_ring` never called. No AI-driven traffic/enemies. |
| Actors | AI stance/action → **animation** | ⛔ | K3 | `animation_system` never ticked; `HumanAnimationSet` never attached. NPCs don't animate via the picker. |
| Actors | AI action ring → **actuation** | ⛔ | K1 | `AiActionBus::drain()` called only in tests. `Ai.Goal("Attack")` accumulates and is never actuated → AI does nothing. |
| Render | Combat impact → **decal spawn** → decal pass | ⛔ | — | Combat never calls `decal.spawn`; there is **no decal render pass**. No bullet holes/scorch/blood; the pool tick is dead weight. |
| Render | Combat/explosion → **particle emit** | ⛔ | — | `ParticleSystem`+`fx_start` built + drawn, but no combat/explosion path calls it. No muzzle flash / fireball / impact puff. |
| Render | Watermap → swim FSM; water → **render pass** | ⚠/⛔ | — | `set_watermap` never called (swim no-ops); no water surface drawn at all → the sea is absent on Maracaibo. |
| Render | Buoyancy → vehicle/prop physics | ⛔ | — | `Buoyancy`+`WaterDrag*` math exists; **no physics system applies it**. Boats sink. |
| Render | Placement → **spot/cone lights** | ⛔ | — | Cone data decoded then dropped; every light collapses to omni. Searchlights/headlights are blobs. |
| Render | Placement → **light animation** | ⛔ | — | `Rt*Animation` sub-records never decoded; torches/pulsing lights are static. |
| Render | Clock → **time-of-day** (sun/atmo) | ⛔ | — | `sun_dir` + atmosphere are hardcoded; sun never moves, no dawn/dusk. |
| Infra | Native combat death → **Lua `Event.ObjectDeath`/proximity** | ⛔ | K1 | The native and Lua event buses are disjoint; proximity is never fed (confirmed RED hook). Real kills/proximity can't trigger missions. |
| Infra | Per-frame **Lua event pump** → mission logic | ⛔ | K1 | No persistent host → no `__pump`, `Event.Post`, or `TimerRelative` in the running game. |
| Infra | Input → **vehicle controls** (enter/drive) | ⛔ | — | No enter-vehicle path; `VehicleControls` set only in tests. Player can't drive; spawned cars are scenery. |
| Infra | Gameplay state → **GFx/HUD** | ⛔ | — | No in-game HUD; `Hud`/`Gui` bindings auto-stubbed. No health/cash/fuel/minimap/objectives on screen. |
| Infra | Lua `Sound.*` → **audible engine** | ✅ | K1 | `GameScriptHost` forwards Lua cues to the loop's shared `AudioEngine`, which now has a cpal device sink + per-frame mixer pump + real decoded wavebanks + calibrated sounddb routing → scripted cues are audible (row 21). |
| Infra | Attitude → **dynamic music** | ⛔ | — | `transition_music` exists; nothing drives it from combat/faction. No stinger on contact. |
| Infra | `mercs2_net` / `mercs2_jobs` → consumers | ⛔ | — | Orphan crates (depended on by nothing). No replication; anim/streaming run inline (no job fork/join). |
| Infra | Schedule (layer-4 ordered spine) → systems | ⚠ | — | Only `"animation"` is registered on the ordered `Schedule`; the rest is inlined in `runtime.tick` — order asserted in comments, not enforced. |

**Cleanly WIRED edges (for contrast):** hero+upgrade → player look; recruits/cash → PMC state; input →
on-foot locomotion; point-light inventory → forward shader; environmental `global_particle_*`
placements → FX pass; static sky/HDR-post chain; one directional shadow cascade; and — verified as a
non-gap — the AI/faction relation matrix is **single-source** (`mercs2_ai::RelationMatrix`; `mercs2_faction`
does not duplicate it).

### 6.3 The connection backlog (ranked)

**Tier 0 — the keystone connectors (unblock the most):**
1. **K1** Make `GameScriptHost` loop-resident in `run_scene_world_loading`: pump `Event.__pump(dt)`,
   drain `take_new_spawns → runtime.realize_spawns` each frame, share its audio `Rc` with the fleet.
2. **K3** Add `Archetype::Character` to `SpawnResolver` — attach the full AI/faction/health/anim/inventory
   component set and map `SpawnFaction → AiFaction`.
3. **K2** Route `run_scene_world_loading` through `StreamingManager` (per-step `mgr.update(player.pos)` +
   incremental collision) instead of the one-shot bubble.

**Tier 1 — high player-visible impact, mostly independent of the keystones:**
4. Combat-death **EventBus subscribers**: `DestroyMsg → DeathQueue` + `DamageMsg → perception.attackers`
   + bridge to Lua `fire_object_death`.
5. Combat→**faction loop** (`mercs2_faction`: `AddInfraction` → mood → `SetRelation` → `Attitude` event).
6. **Particle emit** on explosions/weapon-fire; **decal spawn** on impact + a decal render pass.
7. **Water**: load `Watermap` (`set_watermap`) + a water-surface `RenderNode`; apply **buoyancy** in physics.
8. **Drive**: enter/exit-vehicle state routing `Input → VehicleControls`.
9. **HUD**: 2D pass reading live cash/health/fuel/objectives (reuse the menu bitmap-text overlay).

**Tier 2 — fidelity / lower urgency:**
10. Terrain `set_heightmap` → fleet physics; destruction state→mesh-swap; spot/cone + light-animation
    decode; time-of-day; `set_lod_budget` wiring; tick the anim picker + AI action-ring actuation +
    vehicle-ring pump; autosave-out; give `mercs2_jobs`/`mercs2_net` real consumers (or defer explicitly).

### 6.4 The archetype, generalized

The user's example — *a new save must spawn the player at the right place; a loaded save at its saved
position* — is **seam "Save → player spawn transform"** (6.2, row 1) and is the worst case: the data is
neither decoded **nor** consumed. But the same **producer-exists / consumer-missing** shape recurs ~30×
above. The reimpl's next phase is not more mechanisms — it is **the connection layer**: wire the edges,
starting with the three keystones, and most of this table closes behind them.
