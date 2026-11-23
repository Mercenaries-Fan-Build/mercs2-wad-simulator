# Wave-0 Tier-2 seam review

**Scope.** The inter-silo boundaries of the six merged Wave-0 silos (carve, E1, E2, E3, S5, S6),
reviewed against the actual code on `main` @ `ba7e884`. Tier-2's question is not "does each silo work
in isolation" (they do — build + full test suite green) but **"does data flow across the boundaries,
and do two silos' assumptions compose?"**

**Verdict: no correctness defects.** Every seam is one of three kinds:
- **LIVE** — data actually flows across it today.
- **DANGLING-by-design** — a correct mechanism with no runtime caller yet. Expected: Wave 0 was the
  *enabling* layer; enablers unblock Wave 1, they don't themselves change runtime behaviour. The risk
  is only if a dangling seam is mistaken for "done."
- **API-GAP / DRIFT-RISK** — the seam compiles but is under-specified for its Wave-1 consumer, or two
  copies of a fact can drift. These are the ones to close *before* Wave 1 fans out.

---

## Seam table

| # | Seam | Status | Finding | Wave-1 action |
|---|------|--------|---------|---------------|
| **A** | **E1 schema → world loader** | ⚠ DANGLING | `parse_comp_groups`/`deserialize_records` are called only by the old `ucfx_byteswap` converter + tests. The live `mercs2_engine` world-load path (`game_world`/`worldutil::build_streaming_catalog`) never invokes them, so **no ECS component streams in yet** — E1's whole value is latent. | **The keystone Wave-1 wiring.** In the world loader: per block → `parse_comp_groups(container)` → `g.schema()` → `register_with_fields` → `deserialize_records(g.data)` → fill component pools / spawn entities by type-hash. |
| **B** | **S5 RegionCache ← E1 schema** | ⚠ DANGLING + cross-dep | `worldutil.rs:390` documents it: the manager carries the decision layer (`add_region`/`update_regions`) but **no regions are registered** — the catalog needs `SphereRegion`/`CircleRegion`/`LineRegion`/`PopulationDensity` COMP parsing, i.e. **seam A**. RegionCache is inert until A lands. | Ride seam A: parse the region COMPs via E1's schema → `mgr.add_region(...)`. One task, unblocks row 9 at runtime. |
| **C** | **carve `PhysicsQuery` → impls** | ⚠ DANGLING-by-design | The trait is clean and grounded, but **zero `impl PhysicsQuery`** exist. Vehicle/combat/anim will `take &dyn PhysicsQuery` and have nothing to run against. | Cheap first impl: an adapter holding the existing `collision_tris` + terrain heightmap (`mercs2_game::collision::move_character` already matches `move_character`'s shape). Lets Band-C compile + run *now*, before full Havok. Physics silo (7) later swaps it. |
| **D** | **E2 `PassCtx` → Band-A** | 🔧 API-GAP | `PassCtx` exposes only `device/queue/encoder/color/depth/size`. Water/reflection need **camera view/proj + lights bind groups, the collected renderable/draw-item list, and extra RT views** (reflection / wake / clip / shadow atlas). All four Band-A silos must extend this one struct → **write-collision hazard**. | Make `PassCtx` extension a **mini-enabler owned by ONE silo** (render core, silo 1) at the start of Wave 1, before lighting/fx/water/sky fan out. Add the camera+lights bind groups + a shared `Collect` renderable list (the `PassId::Collect` seam) first. |
| **E** | **S6 save-write → consumers** | DANGLING-by-design | `write_profile`/`set_lua_payload` have no caller outside tests. Correct: writing a save is a gameplay action (a `Pg.SaveGame` Lua binding / shell "save" action) that doesn't exist yet. Row 29 write ✅ = the *format* is faithful. | Wire when the save-Lua namespace lands (rides `Pg`/`Sys` binding + a shell action). No urgency. |
| **F** | **E1 `FieldType` (formats) vs `FieldKind` (core)** | 🔧 DRIFT-RISK | Two parallel enums encode the **same schm type codes** (1=Bit, 2=U8, 4=U16, 5=F32, 6=U32, 7=Ref, 11=Blob32…). The mirror is *architecturally required* (core is asset-agnostic, can't depend on formats), but **no test pins them in agreement** — a code added to one silently diverges. | Add a cross-crate agreement test (in `mercs2_engine`, which depends on both): for every code, `FieldType::from_code(c)` and `FieldKind::from_type_code(c)` agree on presence + byte width. ~15 lines. |
| **G** | **E3 namespaces → Wave-1 owners** | 🔧 API-GAP | Cross-checking E3's 35 installed namespaces against the carve leaf-crates' declared owned-ns leaves **orphans** (see below). | Assign every namespace an owner in the plan §3 before the owning silo starts (else its coverage-gate stubs never get a home). |
| **H** | **leaf crates → layer-4 schedule** | DANGLING-by-design | The 9 leaf crates are empty scaffolds; none registers a system into the Keystone-C schedule yet, and the layer-4 order is still host-inline (the open item from the scheduler work). | As each sim silo lands, register its system into an ordered layer-4 list mirroring `FUN_004c9740`. |

---

## Seam G detail — namespace ownership map

E3 installs **35 engine namespaces**; the carve leaf crates + spine own most, but these are **orphaned
or ambiguous** and need an assigned owner before their silo starts:

| Namespace | Traffic | Proposed owner | Note |
|---|---|---|---|
| **`Player`** | **107 (2nd-highest)** | **`mercs2_player` (DECIDED)** | Cash/fuel/character/disguise spans economy + player-controller → its own crate (scaffolded 2026-07-07, silo 17), not folded into vehicle/faction. `Human.Inventory` (player loadout) is a candidate to co-own here vs `mercs2_combat` — decide when the silo starts. |
| `Graphics`, `Atmosphere`, `Bloom`, `Fade` | render | Band-A render silos (1/4), **in `mercs2_engine`** | The render silos have no *leaf crate* (they live in `mercs2_engine`), so these namespaces need to be owned in-place (a `render_graph`/bindings hook), not by a leaf crate. Document so they aren't orphaned. |
| `Face` | anim | silo 8 (`mercs2_anim`) | FaceFX facial anim — fold into anim's owned-ns (currently only `Human.*`). |
| `Inventory` | player/combat | silo 10 or the `Player` owner | `Human.Inventory` = equipment/ammo. |
| `Timer` | event/spine | spine (`Event`) | Timers are events; likely already spine. |
| `Fire` | combat/fx | silo 10 or 3 | Flamethrower/ignition — clarify combat vs particle FX. |
| `ObjectFilter`, `Report`, `Lti` | uncertain | spine / confirm-live | E3 flagged these low-usage tables' names as needing a `luaL_register` confirm-live read. |
| Split: `Ai` (ai + faction), `ObjectState` (faction + particles) | multi-owner | — | The harness supports two silos installing into one global, **but they must MERGE not overwrite** (E3's caveat). The coverage gate should assert no double-install clobbers. |

---

## The prioritized Wave-1 "do first" list (from the seams)

Before the Band-A/C silos fan out, close the enabling gaps in this order:

1. **Seam A — wire E1 into the world loader** (unblocks components streaming *and* seam B/region cache). The single highest-leverage connection; nothing in Band C is real until components instantiate.
2. **Seam C — a `PhysicsQuery` adapter over the existing collision/heightmap** so vehicle/combat/anim have a live impl to build against from day one.
3. **Seam D — extend `PassCtx`** (camera + lights + shared `Collect` list) as a render-core-owned mini-enabler, before lighting/fx/water/sky start.
4. **Seam G — assign the orphan namespaces** (decide `Player`'s owner first) in plan §3.
5. **Seam F — the `FieldType`/`FieldKind` agreement test** (cheap, prevents silent drift).

Seams E and H are genuinely later (no Wave-1 urgency).

**Bottom line:** the wave composes cleanly — the work now is *connection*, not *correction*. Seam A is
the keystone: it turns E1 from a tested-but-latent mechanism into the thing that makes the whole
gameplay layer possible.
