# Reimplementation parallelization plan — the 16 silos

**Purpose.** Answer one question concretely: *of the 32 documented subsystems, how many can we
reconstruct in parallel, and how do we carve the work so N owners (developers or agents) don't
collide?* This is the launch reference for the faithful-reimpl program — it turns the
[`engine_support_inventory.md`](engine_support_inventory.md) scoreboard (what's built) and the §1a
PC code maps (the RE reference) into **ownable, collision-free work units**.

**Non-negotiable fidelity bar (applies to every silo):**
1. **The exe is the oracle.** Behaviour-gate against the shipped game / the code map; never claim the
   VMX/SSE numeric cores you can't read (`pangea_engine_alignment.md` §4). Where a map says
   *confirm-live*, that item is closed by an x32dbg capture, not a guess.
2. **No stubbed Lua streams.** Every `luaL_Reg` binding the game's Lua actually calls
   ([`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd) + the ~1216-cfunc surface, row 17) gets a **real body**, not the auto-stub.
   Each silo owns its namespaces and its exit criterion includes *the binding-coverage tracer reads
   zero stubs for them.*
3. **No hiding behind flags.** Real behaviour is wired in unconditionally and data-driven; only
   diagnostics may be toggled (per the standing mandate).
4. **Improvements are deferred, not smuggled in.** Anything that changes shipped behaviour — an
   optimization, an upgrade, a *mod-only* capability — is **documented in the silo's `DEFERRED.md`
   backlog** (see §6), never baked into the faithful pass. Faithful first; enhanced second, behind a
   clean seam.

---

## 1. The two-tier model (as requested)

- **Tier 1 — the components.** The 16 silos below. Each reconstructs a subsystem (or a cohesive
  cluster) to the fidelity bar, in its own crate/module tree, against its code map, owning its Lua
  namespaces. Silos are sized so one owner holds one without write-colliding with another.
- **Tier 2 — the seams.** After (or as) silos land, a review pass over every *inter-silo boundary*:
  the ECS component hand-offs, the event-bus contracts, the tick-order dependencies, the render-graph
  pass ordering, the Lua-call fan-in. Tier 2 is where "16 correct components" becomes "one correct
  engine." It is **not** parallel the same way — it's a smaller set of integration owners reading
  across silos (§5.3).

---

## 2. The enabling layer (Tier 0) — 3 pieces that gate the fan-out

These are the shared substrate. Until they're stable, wide parallelism *causes* collisions rather
than avoiding them. They are small, mostly designed, and should be finished **before** Bands A and C
open to their full width. (The Keystone-C scheduler/tick spine, the event bus, and the Lua host are
**already built** — see the scoreboard rows 13/14/16 — so they're not listed here.)

| # | Enabler | Why it gates | Where |
|---|---|---|---|
| **E1** | **ECS field-schema deserialization** (row 12, the `schm` field-name→offset handler) | Until components stream in generically, every Band-C sim silo must hand-author component structs — divergent and un-faithful. E1 makes all 231 registry classes instantiate from the WAD by type-hash. **The single highest-leverage unblock.** | `mercs2_core::registry` + `mercs2_formats` |
| **E2** | **Render-graph carve** — split `mercs2_engine::scene` into an explicit pass/graph module (collect → z → shadow → reflection → color → water → mirror → post → ui, per `FUN_00466d40`) | The 4 rendering silos all edit `scene.rs` today → guaranteed collisions. A pass-registry lets each own its pass module. Also the faithful multi-pass structure (row 1). | `mercs2_engine::render_graph` (new) |
| **E3** | **Lua binding harness carve** — split `mercs2_script::register_engine` into per-namespace modules + wire the existing auto-stub tracer as a **coverage gate** (CI-checkable "N stubs remaining") | 53 namespaces in one register fn = a collision magnet, and "no stubs" is only enforceable if it's measured. Each silo then adds its namespace module independently. | `mercs2_script::bindings::*` (new) |

**Rule:** Band B (world/assets) can start **immediately** (its substrate is built). Bands A/C/D open
to full width **after** their respective enabler (E2 for A, E1 for C, E3 for the binding half of
every silo).

---

## 3. The 16 Tier-1 silos

Grouped into 4 bands by shared substrate. "Rows" = scoreboard rows covered. "Namespaces" = the Lua
surface this silo must implement for real (row 17). "Deps" = hard prerequisites. Every silo's code
map is in [`engine_support_inventory.md` §1a](engine_support_inventory.md#1a-pc-reverse-engineering-code-maps).

### Band A — Rendering (needs **E2**; 4 silos, parallel once the graph exists)

| # | Silo | Rows | Namespaces | Deps | Home |
|---|---|---|---|---|---|
| **1** | Render core + materials + texture streaming | 1 | `Graphics` (partial) | E2 | `render_graph::core`, `mercs2_formats::mtrl` |
| **2** | Lighting + shadows | 2, 3 | — | E2 | `render_graph::light`, `::shadow` |
| **3** | Particles/FX + decals | 4, 6 | `ObjectState.StartEmitter/StopEmitter` | E2 | `render_graph::fx`, `mercs2_engine::particles` |
| **4** | Sky/atmosphere/HDR-post + water | 5, 7 | `Graphics.Atmosphere.*` | E2 | `render_graph::sky`, `::water`, `mercs2_formats::atmosphere` |

### Band B — World & assets (substrate built; **starts immediately**; 2 silos)

| # | Silo | Rows | Namespaces | Deps | Home |
|---|---|---|---|---|---|
| **5** | Streaming + region cache + prop LOD/imposters | 8, 9, 10 | `Object.*Hibernation`, `Pg.Load/UnloadAsset` | — | `mercs2_core::streaming`, `mercs2_engine::worldutil` |
| **6** | Assets/formats hardening + save read/write | 11, 29 | `Pg.SaveGame/LoadGame`, `Sys.SetLuaSaveVersion` | (save-write: ProfileHash *confirm-live*) | `mercs2_formats` |

### Band C — Simulation / gameplay (needs **E1**; 7 silos, each ~its own crate)

| # | Silo | Rows | Namespaces | Deps | Home |
|---|---|---|---|---|---|
| **7** | Physics (Havok char controller, rigid bodies, queries, MOPP/heightfield) | 22 | — | E1 | `mercs2_physics` (new) |
| **8** | Animation runtime (controllers, IK, ragdoll, FaceFX; sampling ✅) | 20 | `Human.*` anim | E1, (ragdoll: 7) | `mercs2_engine::pose` → `mercs2_anim` (new) |
| **9** | Vehicles + camera modes | 25, 19 | `Vehicle`, `Camera` | E1, 7 (raycast) | `mercs2_vehicle` (new), `mercs2_engine::camera` |
| **10** | Weapons / combat (projectile, homing FSM, damage/explosion) | 26 | `Weapon`, `Airstrike`, `Munitions` | E1, 7, 13 | `mercs2_combat` (new) |
| **11** | AI (percept→goal planner, cover, squads, pedestrians, driving) | 23 | `Ai` | E1, 12, **AI code map (RE prelude)** | `mercs2_ai` (new) |
| **12** | Population / spawners | 24 | `Ai.*SpawnList`, `Pg.StartHeliWaveSpawner` | E1, 5, 11 | `mercs2_engine::population` (new) |
| **13** | Entity state machines + destruction + faction/reputation | 30, 31, ★faction | `ObjectState.SetState/SendDamage`, `Ai.Set/GetRelation` | E1, 13(events), 3(emitters) | `mercs2_engine::orchestrator` (data ✅), `mercs2_faction` (new) |

### Band D — Presentation / IO (needs **E3** for bindings; 3 silos, each ~its own crate)

| # | Silo | Rows | Namespaces | Deps | Home |
|---|---|---|---|---|---|
| **14** | Audio (DirectSound/EAX backend, mixer, dual-deck music FSM, banks, 3D) | 21 | `Sound`, `VO` | E3 | `mercs2_audio` (new) |
| **15** | GUI / HUD / Scaleform + input extensions | 27, 18 | `Hud`, `Pda`, `Gui`, `Marker`, `_GuiInternal` | E3 | `mercs2_ui` (new), `mercs2_engine::ui`/`input` |
| **16** | Networking (Keystone-B replication, session/transport, FESL) | 28 | `Net` | E3, 13(event bus) | `mercs2_net` (new); online-restore mod = reference |

**Also cross-cutting, distributed across the silos (not a separate silo):** the high-traffic Lua
namespaces every script touches — `Player`, `Object`, `Event`, `Sys`, `Pg`, `Debug`. `Event`/`Sys`/
`Pg`/`Debug`/`Object` core live with the spine (Tier 0-ish, mostly built); `Player` rides silo 9/13
(cash/fuel/character). These are prioritized because they unblock *executing* the corpus at all.

---

## 4. Crate carve for collision-free parallelism

Parallelism is only real if the code is physically partitioned. Today the sim + render live in two
big crates (`mercs2_engine`, `mercs2_game`) — N owners would fight over `scene.rs`/`world.rs`. The
carve (a Tier-0 task alongside E2/E3):

- **New leaf crates** (one per Band-C/D silo): `mercs2_physics`, `mercs2_anim`, `mercs2_vehicle`,
  `mercs2_combat`, `mercs2_ai`, `mercs2_faction`, `mercs2_audio`, `mercs2_ui`, `mercs2_net`. Each
  depends on `mercs2_core` (ECS/events/time) + `mercs2_formats`, exposes systems registered into the
  layer-4 schedule, and owns its Lua namespace module. **No leaf crate depends on another leaf
  crate** except the declared hard edges (anim→physics for ragdoll, vehicle/combat→physics for
  queries) — those go through a thin trait in `mercs2_core` (e.g. `PhysicsQuery`) so the depender
  compiles against the interface, not the impl.
- **`mercs2_engine` keeps** the window/device/scene-graph + `render_graph::*` pass modules (Band A
  owns one submodule each) + streaming (silo 5).
- **`mercs2_game`** stays the boot/glue + asset-specific (PMC interior) layer; shrinks as leaf crates
  absorb logic.

Result: silo *i* touches its own crate + its own `render_graph`/`bindings` submodule + adds one line
to the layer-4 schedule registration. The only shared write points are the schedule-registration list
and `Cargo.toml` — both trivial merges.

---

## 5. Schedule & recommended concurrency

### 5.1 Wave 0 (now → enablers)
E1 (ECS deser), E2 (render-graph carve), E3 (binding harness) + the leaf-crate carve. **Also runs in
parallel:** Band B (silos 5, 6) — no enabler dependency. So **~5 active** in wave 0 (3 enablers + 2 B
silos).

### 5.2 Wave 1+ (post-enablers)
All of Bands A, C, D open. Physics (7) is a mini-foundation for 8/9/10 — start it *first within its
wave* so the `PhysicsQuery` trait stabilizes (the dependents can start against the terrain-heightmap +
`collision_tris` raycast we already have, then swap to full Havok). AI (11) carries an **RE prelude**:
write the missing `ai_code_map.md` (the one under-mapped system, scoreboard §5.2) before its impl.

### 5.3 Recommended concurrent width: **6–8, not 16**
All 16 are dependency-independent post-enabler, but running 16 at once maximizes integration churn and
degrades both the developer experience (seam thrash, unreviewable diffs) and — since mod surfaces
harden per silo — the eventual modding experience. **Run in waves of 6–8**, each wave closing its
Tier-2 seams before the next opens. A sane ordering:
- **Wave 1 (6):** 7 Physics, 1 Render core, 2 Lighting/shadows, 14 Audio, 5 Streaming-hardening, 6 Assets/save.
- **Wave 2 (7):** 8 Anim, 9 Vehicle/camera, 10 Weapons, 3 FX/decals, 4 Sky/water, 13 State/destruction/faction, 15 GUI/HUD.
- **Wave 3 (3):** 11 AI (after its code map), 12 Population, 16 Networking.

### 5.4 Tier 2 — the seam pass (per wave)
A small set (2–3 owners) reads across the wave's silos for: ECS component-hand-off correctness,
event-bus contract match, tick-order (layer-4 registered list vs `FUN_004c9740`), render-graph pass
order, and Lua fan-in. This is where the scoreboard rows flip to ✅ and the binding tracer is driven
to zero. Uses the existing multi-agent review harness (`/code-review ultra`) per seam.

---

## 6. Cross-cutting: modding & the deferred-improvements backlog

Two of the user's explicit goals are **not faithful-reimpl work** but must be *designed for* during
it, or they cost 10× to retrofit:

**Modding architecture (a principle every silo follows, not a silo):**
- Everything data-driven from registries + asset-override layers (extend the proven `vz-patch.wad`
  overlay + the Lua auto-stub/import system). No hardcoded hashes/paths in a leaf crate.
- Lua stays hot-loadable; the `EngineHost` IoC seam already lets game Lua drive engine systems — keep
  that boundary clean so a mod is "just more Lua + assets."
- Each new leaf crate exposes its tunables as data (the way weapons/vehicles ship stat blocks), so a
  mod re-tunes without a recompile.

**The deferred backlog (`DEFERRED.md` per silo + one roll-up
`docs/modernization/enhancements_backlog.md`):** every idea that changes shipped behaviour is logged
where the code lives, marked **`[faithful-blocker: no]`**, so the faithful pass stays honest and the
enhancement pass has a ready queue. Seed entries mapped to silos:

| Enhancement (user-requested) | Silo | Note |
|---|---|---|
| **Mod support via the Rust engine** | cross-cutting | The architecture principle above; a `mercs2_mods` loader crate is a post-fidelity silo. |
| **Resolution upscaling** (FSR/DLSS-style / render-scale) | 1 Render core | Render-graph makes an upscale pass a clean insert. |
| **Configurable crowd sizes** | 12 Population | `cdbsizes.ini` + spawn-list caps are already data — expose as config. |
| **Pilot every vehicle / ship** | 9 Vehicles | The 9 actor classes are decoded; "pilot anything" = lifting the authored can-enter gate. |
| Ultrawide / high-FOV / uncapped framerate | 19 Camera / 14 Scheduler | Decoupled fixed-sim already supports variable render-rate. |
| Rebindable controls / gamepad-everywhere | 18 Input | The 25-action set is data-driven from `Mercs2.ini`. |

---

## 7. One-line answer

**16 parallel Tier-1 silos**, behind a **3-piece enabling layer** (ECS field-schema deser,
render-graph carve, Lua-binding harness) plus the leaf-crate carve; **~5 start immediately**, the rest
open per-enabler; run **6–8 at a time in 3 waves**, each closing its Tier-2 seams before the next. The
fidelity bar — exe-as-oracle, zero-stub Lua, no flags, improvements deferred to `DEFERRED.md` — is the
same in every silo.

---

## 8. Status log

**Wave 0 — LANDED (2026-07-06, all 6 merged to `main` @ `ba7e884`; integrated workspace builds + full
test suite green).** Six agents, isolated worktrees off `main`:
- **carve** — 9 leaf crates (`mercs2_{physics,anim,vehicle,combat,ai,faction,audio,ui,net}`) + the
  `mercs2_core::PhysicsQuery` seam. Wave-1 sim silos now have homes.
- **E1** — `schm` field-schema deserializer BUILT (`mercs2_formats::schema` + `registry::register_with_fields`);
  **fixed the retail-LE byte-offset bug** ([`docs/schm_type_codes.md`](../schm_type_codes.md) `>>16` was DLC-converter residue).
  Scoreboard row 12 field-schema deser ✅.
- **E2** — `render_graph` carve: `SCENE_ORDER` = the `FUN_00466d40` pass order, `RenderNode`/`PassCtx`
  seam for Band-A; byte-identical (unimplemented canonical nodes are no-op seams). Public `Scene` API held.
- **E3** — 35-namespace modular binding harness + the **zero-stub coverage gate** (`binding_coverage.json`,
  baseline **1075 remaining / 575 called-missing** — the number Band-C/D drive down).
- **S5** — `RegionCache` (row 9) + global LOD governor (`FUN_0084ae70`); c3-building residency left
  disabled with a precise confirm-live RCA (declined to fabricate Y).
- **S6** — save WRITE path + **ProfileHash DERIVED = CRC-32/BZIP2 over `[4:]`** (byte-exact vs all 8
  retail saves). Scoreboard row 29 → write ✅.

Env note: subagents authenticate as read-only `headless-rebase`, so they cannot push/PR — integration
is **local merges to `main`**; pushing `main` needs a write credential.

**Next:** Wave 1 (7 Physics [start first — stabilizes `PhysicsQuery`], 1 Render core, 2 Lighting/shadows,
14 Audio, + continued Band B), then its Tier-2 seam pass. AI (11) needs its `ai_code_map.md` RE prelude.
