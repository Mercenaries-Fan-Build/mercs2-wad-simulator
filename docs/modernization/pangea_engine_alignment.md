# Pangea → native-engine alignment

**Purpose.** Map the original Mercenaries 2 engine ("Pangea", `Pg*`) as recovered in
[`docs/mercs2-pdb-analysis/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-pdb-analysis) onto our native Rust reimplementation's crate/module boundaries, so the
engine/game/middleware/tooling split follows the line the *original developers* drew instead of one
we invent. This is the analysis that precedes the crate split — not the split itself.

**Evidence discipline (read first).** Everything here derives from a symbol/string dump of a
recovered Xbox 360 devkit "Profile" build **plus** the PC-retail Ghidra decomp — *not* a real `.pdb`.
Per the source docs:

- **No `Pg*`/`Mrx*` class has RTTI.** Every Pangea class *name* is debug/string evidence; only Havok
  (`hk*`) has RTTI descriptors. So class layouts, field offsets, and inheritance are **unknown**.
- **`[FACT]`** = a symbol/string provably exists (and, where noted, a decompiled body was read).
  **`[INFERRED]`** = behavior/architecture the source doc itself labels inferred. This doc keeps that
  distinction; do not promote an inference to fact downstream.
- **VMX128 math does not decode — Xbox only; several PC cores DO decode.** The per-frame numeric
  cores (Havok physics step, damage solver, anim sample/blend/IK, shader math, audio mix/pan, water)
  are undecoded on **Xbox** (VMX128). **CORRECTION (2026-07-06):** on **PC** these are x86/SSE, and a
  Ghidra defect — a false `noreturn` on the x87 `sqrt`/`abs` helpers (`FUN_00401740`/`FUN_00401750`)
  that truncated every float-heavy function at its first sqrt — had made them *look* undecoded. With
  it cleared, the **vehicle drive model is fully decoded** (custom raycast sim, NOT the Havok vehicle
  kit — [`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md)). Other PC cores (`hkpWorld::step`, damage/anim)
  are worth re-checking under the same fix, not assumed undecoded. Gate on the exe where not yet read.
- **Address spaces don't mix:** PC VAs (`FUN_00…`) and Xbox VAs (`@82…`) are different builds.
- **Pool-count strings are budgets, not `sizeof`** (e.g. `HibernationControl 14080` = pool count +
  alignment, not bytes).

---

## 1. The two keystones (the real engine core)

These are the highest-value finding: **the same two mechanisms recur in every subsystem doc**, and
both were confirmed in decompiled bodies in *both* builds. They are what "engine registers the type,
game defines its meaning" is literally built on.

### Keystone A — the reflection / ECS component-descriptor registry `[FACT, code-confirmed]`
One registration template instantiated once per component class, seen in streaming, rendering,
animation, physics, camera, audio, AI, game-systems, vehicles, and weapons:

- a `CopyFromStream` deserialize vtable pointer — the **single shared** `&PTR_FUN_82030fa0` is wired
  into **232 component descriptors**;
- the golden-ratio hash seed **`0x9e3779b9`**;
- a class-name string as the registration key;
- a per-type **pool budget** (the `cdbsizes.ini` presize values — see
  `[[cdbsizes-component-pool-config]]`);
- a shared registrar `FUN_0064a770` (PC) / `FUN_824fd490`+`FUN_824fcac8(globals, SIZE)` (Xbox) and a
  central enum dispatcher `FUN_0064ac50`.

Every world-content type (terrain, regions, spawn-lists, physics actors, weapons, AI behaviors, cash,
faction values, sound effects, runtime-render components…) streams in through this one path. **This is
the engine's asset/component/serialization spine** and maps to `mercs2_core` (+ `mercs2_formats` for
the byte-level decode).

**Corpus-verified spec (build from this, do not re-derive)** — [`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/) + the ghidra decomp
already pin the exact ~0x50-byte descriptor each per-class registrar (`FUN_0063xxxx`/`FUN_0064xxxx`,
~16 byte-identical bodies) fills: `CopyFromStream` deserialize vtable · type-tag `3` · **stride**
(serialized record size) · pool cap `0x100` (256) · seed `0x9e3779b9` · shared pool vtable
`&PTR_FUN_00bc5ff8` · class-name string written **last** → `FUN_0064a770` registers it. The **field
schema** is a *separate* per-class template calling the field-builders in stream order —
`FUN_00656210`(int) / `…320`(float) / `…720`(enum,default) / `…890`(bool) / `…610`(vec3) — then
finalizing with `FUN_0064a600` + `FUN_00665590` change-notify. Pool budgets = `cdbsizes.ini`
(`SceneObject 161,280` … see `[[cdbsizes-component-pool-config]]`). Ground truth:
`[[ecs-component-registry-corpus]]` — **232 classes = 220 gameplay + 12 render/pipeline**.

**Caveat (corpus, [`docs/mercs2-ecs/09_render_asset_pipeline.md`](../../crates/mercs2_workshop/data/mercs2-ecs/09_render_asset_pipeline.md)):** the D3D9 **GPU-precache** registry
(VB/IB/vertex-decl/VS+PS shaders) is a **separate** system — a distinct Precache resource vtable, a
per-resource size byte, **not** the `CopyFromStream + 0x9e3779b9` descriptor. Runtime-render
components (`RtLightAnimation`/`ParticleEmitter`/`ObjectMaterial`) *do* use Keystone A; GPU precache
resources do not. So "render streams through Keystone A" holds only for the former.

Corroborates the `ModelName`/world-content registrars (`[[world-placements-no-model-hash]]`).

### Keystone B — the serialized event / RPC bus `[FACT for the quartet + Net/GUI sharing; router internals are stubs]`
A single event bus, **shared by GUI, Networking, AI, and audio**:

- events are a **32-bit name hash + typed-TLV args** (4 arg types: string/guid, int, float, handle;
  argc capped at 7 on the wire);
- a frame **allocate → reserve N 8-byte slots (cap 2048) → build/dispatch → finalize** quartet
  (`FUN_8241d458` / `FUN_82878c50` / `FUN_82420690` / `FUN_8256eb28`);
- GUI (`ToggleHud`), Net (`NetEventCallback`), and AI (`DirectAction`) all dispatch through it; audio
  has its own filter→handler→translator variant.

Maps to a core engine service (`mercs2_core::event` / bus). **Caveat:** the *router* (`FUN_82420690`
local-vs-remote / on-the-wire decision) collapses to unrecovered stubs — the wire protocol is **not
known**.

### Keystone C — the `PgSys*` system registry + per-frame tick `[FACT for registration; master tick INFERRED/open]`
`PgGameSystem.cpp` + a family of `PgSys<Name>` systems. Each ctor claims a bit in a **32-slot
allocation bitmask + parallel name-string table** (per system family), and `PgSysPopulation` appends
its update fn to a bounded global update-list. Confirmed: `PgSysVehicle @82373600`,
`PgSysNetworking @82593618`, `PgSysPopulation @823641f0`. Engine systems (`PgSysRender`,
`PgSysCamera`, `PgSysAnimation`, `PgSysTransformController`, `PgSysDelete`) and gameplay systems
(`PgSysVehicle`, `PgWeaponSystem`, `PgSysAi`, `PgSysPopulation`) share the framework. **Open:** the
master `PgGameSystem::Update` that iterates the tables — and its order — was **not located**. Our
`mercs2_core` `Schedule` is the modern analog.

---

## 2. Subsystem → layer → crate cross-reference

Layer legend: **E**=engine service · **F**=engine framework/kernel · **G**=game content ·
**M**=middleware (replace/port) · **T**=tooling/reference. "Straddle" rows split explicitly.

| Pangea subsystem | Layer | Our crate/module | Notes (evidence-grounded) |
|---|---|---|---|
| pangea-engine-core (`PgGameSystem`, `PgSys*`, Update/RunState, asset registries, `PgModelStateMachine`, heaps, `Pal*`/`Pimp*`) | **F** | `mercs2_core` | Keystones A/C live here. `PgModelStateMachine`/`CreateStateTransTable` = per-entity state machine `[FACT: fn exists]`. Framerate-policy state = string-only. |
| rendering-shaders | **E** | `mercs2_engine::render` | Frame driver, multi-pass scene, renderables, materials/textures, shadows, post/HDR, particles (PgFX), lighting, Scaleform draw. Pass **ordering INFERRED**. Shader math undecoded. FaceFX here is **M** (misfiled by name). SSM/ucode compiler is XDK **T**. |
| world-streaming | **E**+G | `mercs2_core::streaming` + `mercs2_engine::stream` + `mercs2_formats` | Mechanism (StreamingManager, per-object `HibernationControl`, `TerrainObject`/`PropPhysics` activate, DMA load, terrain height query, 2-tier terrain LOD) = engine. Faction spawn-lists, `prop_*` catalog, `vz.wad` = **G**. Pre/post-load **pipeline ordering unconfirmed**. `UpdateStreamBlocks` is **audio**, not world. |
| animation-skeleton | **E**+M+G | `mercs2_engine::anim` + `mercs2_formats` (decode) + `mercs2_engine::render` (skinning) | Pose sample / bone controllers / skeleton-map / ragdoll orchestration = engine. Havok `hka`/`hkb` + FaceFX = **M**. Bone-name tables, anim-sets = **G**. Wavelet decode already ours (`[[wavelet-decode-solved-live-capture]]`). |
| audio-pal | **E**(M-flavored)+G | `mercs2_engine::audio` + `mercs2_core` (ECS sound comps) + `mercs2_game` (cues/music policy) | Pal = voice/mixer/3D/cue/bank-stream/16-state instance FSM/ducking/dynamic-music engine (replace XAudio backend). Cue/bank assets + faction/action music policy = **G**. Pal internals are **PC-build-only / string-inferred**. |
| gui-hud | **E**+M+G | `mercs2_engine::gui` + `mercs2_game::hud` | Scaleform/GFx render framework + movie player = engine (GFx itself **M**). `Gui*Update`, minimap, PDA blips, markers, reticle, screen-effects = **G**. Uses Keystone B. |
| networking | **E**+G+M | `mercs2_engine::net` + `mercs2_game::net` + `mercs2_game::session` | Bus/replication framework (Keystone B) = engine. `NetSubCat*`/`NetSafe*`/`NetClient*` payloads = **G**. Massive ads + Xbox LIVE/XLSP transport = **M** (replace, don't port). |
| lua-scripting | **E**+G | `mercs2_script` + `mercs2_game` (scripts) | VM host + `Sys.*` binding table + module loader + console/debugger = engine (VM itself is Lua, we target 5.4). Mercenaries Lua corpora (`[[decompiled-lua-corpus]]`) = **G**. No `PgScriptEventManager` in this doc (it's in core). |
| physics-game | **E**(seam)+G | `mercs2_engine::physics` (integration + backend trait) | `PgHavokManager` + the `PgPhysicsActor*` family = the **integration seam** (one stream-loaded component family; our backend slots behind it). Grapple/winch/droplet behaviors = **G**. |
| havok-physics | **M** | external (rapier/custom behind the seam); `mercs2_formats` reads packfiles | Havok **5.5.0-r1** — the capability contract our backend must satisfy (world/shapes/MOPP/heightfields/constraints/character/**vehicle SDK**/packfile serialization). |
| camera | **E** | `mercs2_engine::camera` | `PgSysCamera` → view/proj + `cameraPos`/`InvViewport` constants, collision cast (≤5 viewports = split-screen). Camera-mode blocks = ECS components (**F**), tuning = **G**. |
| jobs-threading | **E**(F) | `mercs2_core` (job/scheduler) + `mercs2_engine` (worker pool, GPU fence) | Pimp = worker pool / bounded `a64` queue / per-CPU timers / Jobtypes. **Entirely string-only** in decomp — internals inferred. `MassiveThread` ≠ Pimp. |
| vehicles | **G** on E | `mercs2_game::vehicles` (+ `mercs2_core` for the ring) | Bounded **command-ring** transport + per-class controller singletons + reflection stream-load = engine mechanism (Keystone A). Action classes, control verbs, tuning fields, physics actors, AI driving states, parts = **G**. Drive model **DECODED on PC** (2026-07-06): custom raycast sim (nine `hkpUnaryAction` actors), NOT the Havok vehicle kit — [`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md). |
| weapons-combat | **G** on E | `mercs2_game::weapons` (+ `mercs2_core`) | Same Keystone-A registry; homing lock = event-dispatch FSM; `DamagePerson` = event **authoring**, not the hit solver. All `Weapon*/Projectile*/Homing*/Damage*/Explosion*` + tunables + `wpn_*` = **G**. Ballistics/damage math **undecoded**. |
| ai | **E**(F)+G | `mercs2_core`/`mercs2_engine` (framework) + `mercs2_game`/`mercs2_script` | `PgSysAi` host + `DirectAction` bus + component registry = engine. Goal verbs, cover FSM, squad, pedestrian chatter, `Ai.Goal` Lua = **G**. **Pathfinding algorithm unnamed/unrecovered** (`PathFind` is a debug-color registrar). |
| game-systems | **G** (+E spine) | `mercs2_game::*` + `mercs2_core` (save/serialize mechanism) | Save versioning/hash/corruption/critical-section dispatch = engine mechanism; economy, missions/contracts, factions, achievements, profile content = **G**. Stats/leaderboards/entitlement = EA Blaze/Nucleus online back-end (**M/G-online**). |
| symbol-map / pdata-functions | **T** | reference (into `mercs2_probe` workflows) | RTTI↔PC-`FUN_` bridge (204/287) and 39,013-function inventory. Oracles, no portable logic. |
| data-defaults | **E-ref** | reference for `mercs2_formats`/`mercs2_core` | `.data` = the **Havok reflection schema** (1,287 classes) + format/math enums. Explicitly carries **no game tuning defaults** (those are WAD `wpn_*`/`wif*` blocks). Field offsets are all `0x0000` — no `name→default` chain. |
| imports-exports | **E (platform)** | `mercs2_engine` platform-abstraction + `mercs2_core` OS shim | The platform surface to abstract: D3D9/XGRAPHC→wgpu, XAUD/XMP→audio, XNET/XONLINE/XHV→net/voice, xboxkrnl/xam→OS/system. **Module-level only** — no per-function (incl. input) list recoverable. |
| debug-cheat-menu | **T** (+G) | `mercs2_probe` + `mercs2_game` (cheats) | ~250 items = a verified **subsystem checklist**. Cheats (God Mode/Infinite Ammo) are **Lua-bound, not engine-native** `[FACT correction]`. |

---

## 3. What is middleware (replace or port — never "our engine")

- **Havok 5.5.0-r1** — physics (`hkp*`), animation (`hka*`), behavior (`hkb*`), vehicle SDK, packfile
  serialization. Replace with a Rust backend behind `mercs2_engine::physics`; parse its packfiles in
  `mercs2_formats`.
- **Scaleform GFx** (Flash UI) + **FaceFX** (facial) — replace with our own UI/facial modules.
- **Pal** (audio) and **Pimp** (jobs) — Pandemic's *own* internal libraries, but still "not our
  engine": reimplement their capability, don't port their code (and their internals are largely
  undecoded anyway).
- **Massive** (in-game ads), **Xbox LIVE / XLSP** (sessions/matchmaking/voice), **EA Blaze/Nucleus**
  (stats/achievements/entitlement), **XDK** (D3D9/XAudio/XNet/XHV/xam/kernel), **Bink** (movies —
  see `[[ps3-movies-dropin-on-pc]]`). Replace with native equivalents.

---

## 4. Guard rails — what is NOT proven (anti-hallucination)

Do not let synthesis assert any of these:

1. **All Pangea behavior is inference** — no `Pg*` RTTI; class layouts/hierarchies unknown.
2. **VMX128 numeric cores are undecoded on Xbox** — physics step, damage/ballistics solver, anim
   sample/blend/IK, shader pixel/vertex math, audio mix/3D-pan, water. Behavior-gate against the exe;
   never state the math. **EXCEPT (PC, 2026-07-06):** the **vehicle drive model IS decoded** (custom
   raycast sim — [`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md)); the old "undecoded" was a Ghidra
   noreturn-on-sqrt truncation, not VMX128. Re-check other PC cores under that fix before asserting.
3. **Pipeline orderings are inferred from profiler labels, not traced control flow** — render pass
   order, anim pipeline, streaming pre/post-load, physics step order, master `PgGameSystem` tick.
4. **Pimp job system internals are string-only** (0 decomp hits) — worker pool, lock-free queue,
   Jobtype registry are inference.
5. **Pal audio bodies are PC-build-only**; Xbox methods not locatable; "Pandemic Audio Library" is a
   prefix inference.
6. **The event-bus router / wire protocol is unrecovered** (stubs) — we know senders, not routing.
7. **Naming ≠ implementation.** Confirmed misnamers: `PathFind`=debug-color registrar; `BoatStop`=AI
   state-table registrar; `DamagePerson`=damage-event authoring, not the hit solver;
   `UpdateStreamBlocks`=audio, not world streaming; `ClipGeneratorFlags`≈Havok anim, not a magazine.
   Treat a symbol name as "this fn references that string" until a body proves otherwise.
8. **`.data` has no game tuning defaults** — weapon/vehicle handling values live in WAD reflection
   blocks; `.data` is the Havok reflection schema (field offsets all zero).
9. **Cheats are Lua/game-side, not native.**
10. **Pool-count strings ≠ `sizeof`**; **PC VAs ≠ Xbox VAs**.

---

## 5. Engine capabilities Pangea has that we have *not* built

The cross-reference exposes real holes in "our engine" (vs. the game PoC we keep adding):

- **Formal component/asset registry** (Keystone A) as an engine module — today we `extract_container`
  ad-hoc.
- **The event/RPC bus** (Keystone B) — none.
- **The `PgSys` scheduler** as first-class (partly in `mercs2_core::Schedule`); the master tick order
  is unknown even in the exe.
- **Jobs/threading** (Pimp analog) — none (candidate: rayon).
- **Camera system** — only a fly-cam.
- **Audio** — none.
- **GUI/Scaleform-equivalent** — none.
- **Physics integration seam + backend** — none.
- **Net framework** — only the online-restore mod (`[[mercs2-online-restore-and-patch-architecture]]`).
- **Script host** (Lua 5.4 + `Sys.*` bindings + module loader + console) — planned.
- **Load areas / region cache** — `PgSysPopulation` `CacheIn`/`CacheOut`/`CacheRequired` is a
  region-scoped cache, **distinct** from per-object `HibernationControl`. This is the "load areas" the
  charter names, and we have only per-object hibernation so far.
- **Platform-abstraction layer** (input/IO, audio device, net, graphics device) from imports-exports.

---

## 6. Refined crate/module layout (proposal, driven by the above)

```
mercs2_formats (lib)   ENGINE — asset/format decode: WAD/UCFX, terrain, anim (wavelet/spline/delta),
                        texture/MTRL, .sho pairing, Havok packfile/reflection parse
mercs2_core   (lib)    ENGINE KERNEL — ECS World + Keystone A (component registry) + Keystone B
                        (event/RPC bus) + Keystone C (PgSys scheduler) + fixed-tick + job system +
                        GUID handles + save/serialize spine
mercs2_engine (lib)    ENGINE SERVICES — render · camera · anim · stream (+ load areas) · audio ·
                        gui · net (transport/replication) · physics (integration seam + backend
                        trait) · script (Lua host) · platform-abstraction (input/IO/device)
mercs2_game   (bin)    GAME (Mrx* + gameplay) — vehicles · weapons · ai · missions/economy/factions/
                        achievements · hud content · spawn lists · PMC interior + spawns · Lua corpora
                        · dynamic-music policy   → the exe
mercs2_probe  (bin)    TOOLING — the 44 diagnostic/export flags + reflection/enum/pool oracles
external backends       MIDDLEWARE — physics (rapier/custom), ui, etc. behind engine traits
```

The engine/game line holds cleanly; the only straddlers (gui, net, lua, audio, save) split the same
way every time — **framework/mechanism = engine, content/policy = game**.

---

## Provenance
Synthesized 2026-07-02 from all 20 [`docs/mercs2-pdb-analysis/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-pdb-analysis) docs (12 parallel evidence-grounded
readers), each required to tag FACT vs INFERRED and flag build/naming traps. Cross-refs:
`[[modernization-program-64bit-reimpl]]`, `[[ecs-component-registry-corpus]]`,
`[[mercs2-pdb-analysis-corpus]]`, `[[world-streaming-spec]]`, `[[cdbsizes-component-pool-config]]`.
