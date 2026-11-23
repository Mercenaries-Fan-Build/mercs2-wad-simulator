# The vehicle model — assembly, binding, and how to author a new one

**Status:** authoritative as of 2026-07-12. Derived entirely from retail containers + the decomp;
every rule below is backed by a probe you can re-run. Supersedes the "intact/ruin submesh
interleaving" story in `model_render_gate_spec.md` §2c and `object_assembly_model.md` §7 — see
[§9 Retracted](#9-retracted-claims).

Read this before authoring a new tank/car/heli, or before touching the model loader.

---

## 1. A vehicle is not one container

Its geometry is spread across a **chain of blocks**, coarsest first:

```
ch_veh_tank_ztz98_P000_Q3.block     4,435 tri   <- RESIDENT: the object lives here
ch_veh_tank_ztz98_P001_Q2.block    18,333 tri   <- streamed refinement
ch_veh_tank_ztz98_P002_Q1.block    28,620 tri   <- streamed refinement
```

`P00N_Q(3-N)` is the same c3 LOD-block scheme that carries a texture's higher mips (see
`wad::extract_texture_hires`). World-placed vehicles use **cell-named** blocks instead
(`c31188` → `c31188-c20055` → `c31188-c20055-c11083`); the chain is that cell's subtree. Both forms
are walked by `wad::extract_model_lods`.

**Only the RESIDENT block ships the object.** The finer blocks are pure geometry payloads:

| chunk | resident `P000` | streamed `P001` / `P002` |
|---|---|---|
| `HIER` — node tree (the skeleton) | ✅ | ❌ |
| `SEGM` — segment table (bone + LOD tier) | ✅ | ❌ |
| `MTRL` — material table | ✅ | ❌ |
| `PHY2` — Havok convex hulls | ✅ | ❌ |
| `SWIT` + `NODE`/`STAT`/`CHDR`/`CEXE` — destruction machine | ✅ | ❌ |
| `GEOM`/`MESH`/`SKIN`/`PRMG`/`PRMT`/`IBUF`/`STRM` — geometry | ✅ (coarse) | ✅ (finer) |
| `INDX` — sub-object → seg_id | ✅ | ✅ |

> **The object is** `geometry(rung) × INDX(rung) × SEGM/HIER/MTRL/PHY2/machine(RESIDENT)`.

The resident `SEGM` is the **master segment table for the whole chain** — the ztz98's 130 records
serve 12 + 35 + 63 groups across three blocks. A streamed rung's `INDX` names rows in *that* table.

**Characters ship a single block and no chain** (Mattias: 55,490 tri resident). If you're testing a
loader change, a character will not exercise any of this.

---

## 2. ★ The binding chain (get this wrong and nothing works)

```
PRMG drawing group
  └─ its parent MESH/SKIN sub-object under GEOM  → sub_object ordinal k
       └─ seg_id = INDX[k]                        ← INDEXED BY SUB-OBJECT, *NOT* BY GROUP
            └─ SEGM[seg_id] = { bone: u16, seg_id: u8, state_mask: u8 }   (4-byte records)
                 ├─ bone       → HIER node = the ATTACHMENT MOUNT (draw-gate clause 3)
                 └─ state_mask → the LOD TIER bitmask            (draw-gate clause 2)
```

**`INDX` has exactly one entry per top-level `MESH`/`SKIN` sub-object under `GEOM`** — never one per
PRMG group. A sub-object can own several drawing groups, so the two ordinals diverge:

| model | INDX rows | MESH + SKIN | PRMG groups |
|---|---|---|---|
| `pmc_hum_mattias_v3` | 24 | 7 + 17 = **24** ✅ | 29 |
| `civ_veh_car_van_crappy` | 65 | **65** ✅ | 77 |
| `ch_veh_tank_ztz98` (P002) | 62 | **62** ✅ | 63 |
| `vz_veh_tank_amx30_elite` (P002) | 31 | 29 + 2 = **31** ✅ | 32 |

Keying on the group index makes every group past the first divergence read the **wrong SEGM row**,
handing the mesh **someone else's bone and someone else's LOD tier**. Symptom, if you ever see it
again: geometry is *correct but in the wrong place* — `vz_veh_tank_amx30_elite` hung its treads in
mid-air at tiers 0/2/4 while tier 1 (where the ordinals coincidentally lined up) rendered perfectly.

**Self-check:** a row reached THROUGH `INDX` names itself — `SEGM[INDX[k]].seg_id == INDX[k]`.
Assert this; it catches a mis-binding instantly. It is **not** true of every row: see §2a.

### 2a. ★ Instance rows — the table is walked by ROW, not by geometry

Retail's draw loop (`FUN_00472a50`) walks **every SEGM row**: row `r` draws `renderable[SEGM[r].seg_id]`
at `SEGM[r].node`, gated by `SEGM[r].state_mask`. The matrix slot comes from the row's node alone
(`FUN_004778b0` reads rec+0, never the seg_id). So a row whose `seg_id` names **another** row's
geometry draws that geometry **again, at its own node** — an instance. A loader that only reaches
SEGM through `INDX` never draws these rows.

| model | SEGM rows | instance rows | what they are |
|---|---|---|---|
| `al_veh_tank_m1a2` | 100 | 26 (rows 74–99 → segs 72/73) | 13 road wheels × 2 LOD tiers; one wheel mesh exists |
| `civ_veh_car_van_crappy` | 128 | 9 (→ segs 116–118) | 3 wheels × 3 tiers |
| `0x1FBFBB4B` (PMC resident) | 67 | 54 | repeated structures |

Census: **251 of 3,007 models, 3,739 instance rows** — every tank's road wheels, three of a car's
four wheels, helicopter rotor blades, repeated building parts. Every one has a rigid (node-placed)
source with geometry in some loaded rung.

**Placement: the REST pose is the `+16` chain, not the `+80` bind.** An instance node's `+80` is a
**copy of its source node's**, and an ordinary part node's `+80` is its **parent's** world (the M1A2
turret's `+80` is the hull origin, 1.63 m below its `+16` mount). Retail's live per-node matrices of
`0x1FBFBB4B` (capture `23fe7f38_cracked-cruise/pmc`, `OBJ+0x2B8`) equal the `+16` chain on 73/73
nodes and the `+80` bind on 8/73 — the geometry-owning nodes. Rigid geometry is placed by `+80` and
skinned by `invBind · pose`, so the rest palette must be `invBind · (+16 chain)`
(`mercs2_mesh` `SkinData::bones`); an identity rest sinks turrets and stacks every instance on its
source. Every spawned instance starts at it: `Scene::model_rest_palette` hands out
`SkinData::rest_palette` (the rest palette of a rigged model, identity for an unrigged one, sized to
every joint the verts reference) and a clip replaces it per frame. Tests: `model_assembly.rs`
`every_segm_row_draws_including_instance_rows`, `instanced_road_wheels_rest_on_their_own_hubs`,
`an_undriven_vehicle_spawns_at_its_rest_palette`; `mercs2_probe` `retail_node_matrices_are_the_plus16_chain`.

**Rigid vs skinned matters.** A `MESH` sub-object is **rigid**: authored in its bone's LOCAL space,
so it must be multiplied by that bone's world-rest matrix. A `SKIN` sub-object is already in model
space. Vehicles are almost entirely rigid (a tank is ~100% `MESH`), which is exactly why a wrong bone
throws parts across the map rather than merely mis-weighting them.

---

## 3. LOD: tiers, and how the rungs compose

`state_mask` is a **bitmask of LOD tiers** (bit 0 = nearest). The engine builds a `view_state` from
camera distance — `1<<n`, or a 3-tier cross-fade window `1<<(n-1) | 1<<n | 1<<(n+1)` — and a segment
passes clause 2 if `view_state & state_mask != 0`.

Once bound correctly, the masks **partition across the chain** (ztz98):

```
P000 (resident)  masks 0x70        -> tiers 4,5,6   (+ 0x7f = every tier: caps/trim)
P001             masks 0x04/0x08   -> tiers 2,3
P002             masks 0x01/0x02   -> tiers 0,1
```

**Rungs REFINE each other; they do not sum.** The resident block is a *complete, self-contained
low-detail model spanning every tier* — the fallback for an object whose finer blocks haven't
streamed — and each finer block **re-authors** some of its nodes at the near tiers. The car van's
body is on node 3 as **736 triangles in the resident block and 9,360 in P001**, both masked for
tier 0. Draw both and you get two detail levels of one panel fighting for the same pixels (that was
11,604 of 19,107 triangles double-drawn).

⇒ **For each `(node, tier)`, only the FINEST loaded rung survives.** `model::apply_supersede` bakes
this into `lod_mask` at load, so downstream consumers just run the ordinary gate. Same relationship
as the texture chain: a finer mip block *replaces* the top of the resident chain rather than adding
to it.

`lod_count` (= runtime `maxLOD`) is model-header `+0x34`; the LOD distance is `+0x38`. **`minLOD`
(runtime `M+0x80`) has no known on-disk source** — header word `0x30` is 0 for every vehicle (and 4
for Mattias, matching his `lod_count`). Open.

---

## 4. The draw gate (3 clauses, all must pass)

`FUN_004722a0` / `FUN_00472a50`:

```c
renderable[seg_id] != 0
&& (view_state@OBJ+0x352 & lod_mask)          != 0     // clause 2 — LOD tier
&& (node < 0 || node_enable@OBJ+0x2a0[node])           // clause 3 — destruction
```

LOD is one axis; destruction is the other. **A mesh draws only if BOTH pass.** `node < 0` means
"bound to no node" — always visible, never superseded.

---

## 5. Destruction

Health → damage **messages** → the per-model state machine → per-node `SHOW`/`HIDE` → clause 3.

**State vocabulary** (cracked; do not re-derive):

| hash | name |
|---|---|
| `0x0ACE072A` | `InitState` — spawn; immediately `SetState(PristineState)` |
| `0x5D308F4F` | `InitDestroyedState` — spawn already wrecked |
| `0xACB51200` | **`PristineState`** |
| `0x1D5575A1` | `DamagedState` — body still shown, fire emitters start |
| `0x92791EBB` | `StartDestroyedState` — explosion, `CreateObject` debris, `SetState(Destroyed)` |
| `0x7687DF41` | **`DestroyedState`** — `SHOW`s the wreck body |
| `0xCA261E5B` | `GoneState` — terminal/empty |

**Messages:** `0xC6507EE1` `DamageMsg`, `0x1ED7AD78` `DestroyMsg` (+ sibling `0x3D0D4C99`).

**Commands** in a state's enter-script: `SHOW`, `HIDE`, `SetState`, `SetStateOnMsg`, `StartEmitter`,
`CreateObject`. The last argument of `SetState`/`SetStateOnMsg` is the **switch node's own hash** (its
machine address), not geometry — don't treat it as a node to hide.

**★`SHOW`/`HIDE` act on the whole SUBTREE**, not just the named node. This is why a tank's chassis
and treads — which the machine never names — still vanish on destruction: they're children of a
governed parent. (Counting only directly-named nodes produces a bogus "40% ungoverned" figure.)

**Shared body slots.** Every vehicle carries two sibling nodes under its body switch node:

| hash | role |
|---|---|
| `0x255EAB53` | the node `PristineState` `SHOW`s — the **intact** body |
| `0x75F1F74D` | the node `DestroyedState` `SHOW`s — the **wreck** body |

The wreck is **geometry in the container**, on its own node — usually skinned with a **shared global
asset** (`global_veh_tank_ruin_dm`, `global_veh_civilianruin`, `global_veh_brokenglass`). It is *not*
a separately-spawned object. `CreateObject` spawns loose **debris** pieces, which is a different
thing.

**A wreck legitimately has MORE triangles than the clean body** (the elite: 11,956 vs 11,579) — a
crumpled hull is high-poly. Never assert `intact > wrecked`. The real invariant is that the two states
draw **different** geometry: the wreck is *swapped in*, not added. Verified across car/tank/heli/AA:
**0 triangles drawn identically intact vs destroyed.**

### 5a. ★ Template states — a variant's loadout on a shared model

Most switch nodes' `InitState` runs `SetState(0xCDE0B80F, <node>)`. The `SetState` handler
`0x004D1C70` resolves `0xCDE0B80F` to the state the entity's **`OSMStateParameter`** records name for
that switch node (pool `0xDF8B90`; schema `+0` switch-node hash, `+4` state hash) — the entity's own
records first, then its template's (`SceneObject+0x14`), first match — else `PristineState`
(`0x004D1CBE`). The lookup `0x00649440` → `0x0050026C` is read on x360 (`FUN_8231A0E8` →
`FUN_824FC4A0`); the PC body is not read. `0x004D4010` enters any state: one the node does not list is
recorded with no script run.

This is how a variant template picks its loadout on one model: `UH1 Transport (GR)` (`0x80008205`) sends
all 14 of `global_veh_uh1huey`'s kit switches to `0x46F79D30`, a state with no enter script, so the
`SWIT`-controlled kits are never shown; `UH1 Attack` names 11 (six kits show), `UH1 Elite` 4 (the nose
kit shows). `al_veh_truck_hmmwv_armored`'s weapon variants send the other weapon's mount to a state
hashed as the switch node itself, which it does not list — hidden the same way. **Proven against a
live capture:** the template's records plus the rotors (§5b) reproduce both live Huey tables of
`20261021_chris-jacobs-flying-winch.dmp` node for node (`mercs2_engine` test `rotor_retail`;
`orchestrator::template_states` / `machine_run_in` / `default_state_index_in`). A model viewed with no
template takes `PristineState` for every such node and shows every kit.

### 5b. ★ Rotors — blades, blur discs, and the spin

A helicopter rotor ships **two versions on two nodes**: the solid blades and the motion-blur discs (the
discs are blended materials, see `material_shader_spec.md` §1d). Both are always in the container; a
**bone controller** shows one, the other, or both by rotor speed. Its node writes go into the **same
node-enable records** the destruction machine's do (clause 3), so the swap is not a separate hide pass.

**Data — the `Rotor` component of the `worldentity` templates** (version 82, 28 bytes, one record per
rotor; boats carry records for their propellers with no blur nodes). Read by `0x006570A0`
(`mercs2_formats::rotor`):

| off | field | used by the controller |
|---|---|---|
| `+0x00` | `RotorHubBoneName` | spun |
| `+0x04` | `SpinDir` | scales the spin (`±1` in retail) |
| `+0x08` | `FirstRotorBladeBoneName` | not copied |
| `+0x0C` | `MinRotorAngle` | not copied (0 in every retail record) |
| `+0x10` | `MaxRotorAngle` | not copied (0 in every retail record) |
| `+0x14` | `RotorBlurOnBone` | shown when fast |
| `+0x18` | `RotorBlurOffBone` | shown when slow |

The bone names are per template: most helicopters use `bone_bluron`/`bone_bluroff` (tail:
`bone_tailbluron`/`bone_tailbluroff`); the Ka-29B's coaxial rotors name their own nodes
(`0x39A44BCE`/`0xD22DF6A4` upper, `0x9BABA1D9`/`0xEC2BE0FB` lower). A template can carry records for
hubs of a sibling model it also serves (`UH1 Transport` carries `bone_rotor`/`bone_tailrotor` beside the
template rig's hubs); over the 28 rotor models in `vz.wad`, every record whose hub a model has also
names blur nodes it has (`mercs2_engine` test `rotor_retail`).

**The controller — `PgRotorBoneController`** (class name from PS3 v1.01-eu RTTI). The bone-controller
builder `FUN_00683D70` makes one 0x2C-byte controller per record (vtable `0x00BCD5F8`, priority 10):
`+0x14` angle, `+0x18` kill latch, `+0x1C` hub, `+0x20` SpinDir, `+0x24` blur-on, `+0x28` blur-off.
Update, slot 2 `0x00638000` (x360 `0x824CDB70`, PS3 `0x001E8B28`):

```text
speed = HelicopterPhysics+0x3C                     // 0..1, via PhysicsActor vfunc +0x64 (0x00439250)
kill  = actor.u8[+0x15] | ctl.u8[+0x18]
angle = wrap_pi(angle + SpinDir*speed*dt*30.0)     // 30.0 @0x00B977D0
set_node(obj, node(blur-on),  speed > 0.3 && !kill)    // 0.3 @0x00B9B688
set_node(obj, node(blur-off), speed < 0.9 && !kill)    // 0.9 @0x00BB3E44
```

Strict compares: **between 0.3 and 0.9 both show**; a NaN speed shows neither; no hysteresis, no fade.
`set_node` is `0x00472960` → `0x004B7630`: it replaces the node's record and the table rebuild
`FUN_004727B0` runs (a node with no record under a recorded parent copies its parent, so the Ka-29B's
blades, children of its blur-off nodes, follow). Slot 5 `0x006381F0` turns the hub's local transform
by `angle` about its own +Y (the multiply order is INFERRED). **Proven against a live capture:** the two
Hueys of `20261021_chris-jacobs-flying-winch.dmp` (speed 1.0 and 0.0) have node-enable tables that differ
in exactly the four blur nodes, and the controllers over the pristine machine reproduce that difference.

**Kill latch.** The state machine's command `0xBA20F82C(event, node)` reaches `FUN_004B16B0`; on event
`0xB4A26AA7` (main rotor, issued from the Huey's rotor switch when `DestroyMsg` arrives) or `0x6CCBB80A`
(tail) it sets `+0x18` on every controller whose hub is that node, its ancestor or its descendant. Both
meshes then hide; the broken blades are the machine's own `CreateObject` debris. Nothing clears it.

**Rotor speed — `HelicopterPhysics+0x3C`**, stepped by `FUN_004517B0` inside applyAction `0x00453760`
(`mercs2_vehicle::rotor::HeliRotor`):

- vertical control `+0x28` (verb `0xE6FA6B1B`, the player's every frame, -1..1) above 0 → `+0.25/s`
  (full speed in 4 s); exactly `5.0` (AI) → `0.95`;
- otherwise the rotor runs down **only after 0.5 s of contact** (`+0x4C`, the contact listener at
  `+0xD8`); airborne with no input the speed **holds** (the flying Huey above: speed 1.0, input 0, no
  contacts);
- run-down is the decay `0x00453B30` at 0.975/tick pushing down, 0.99 idle above 0.1, else 0.95; a lost
  main rotor (`+0x48`) runs down at 0.925 regardless;
- created at 0, or at 1.0 when the vehicle is created crewed (`+0x68`, a seat/rider test, partly read).

**Open:** what sets `+0x44 = 3` (an object looked up by key `0x2001`), who sends event `0x9CC8AF58`,
the `+0xA0` pool behind the actor's `+0x15` latch, whether the AI's `5.0` is `MaintainRotorSpeed`, and
what a boat's propeller reads for speed (its actor's vfunc `+0x64`).

---

## 6. Materials

`MTRL` (resident only) is the material table. **Each `PRMT` sub-strip inside a PRMG group carries its
own material** — render per sub-strip, not one material per group. A single group routinely mixes the
body skin and a shared ruin/glass skin; that is normal and is *not* evidence of a hidden selector.

---

## 7. Authoring a NEW vehicle — the checklist

The governing principle from `valid_model_structure_map.md` still holds: **conform, don't invent.**
Start from a real vehicle container of the same class and rewrite it.

**Must-haves in the RESIDENT block:**

1. `HIER` — the node tree. Node name-hash = `pandemic_hash_m2(bone_name)` (`bone_<part>_<axis>`).
   Include the two shared body slots: `0x255EAB53` (intact) and `0x75F1F74D` (wreck), as **siblings
   under a common parent** which is the body switch node.
2. `SEGM` — one 4-byte record per segment `{bone u16, seg_id u8, state_mask u8}`, sized for the
   **whole chain** (all rungs). A row that owns geometry has `SEGM[i].seg_id == i`; an instance row
   (§2a) names another row's `seg_id` to re-draw that geometry at its own node.
3. `INDX` — **one u16 per `MESH`/`SKIN` sub-object**, in sub-object order, each naming a SEGM row.
   `INDX.len() == sub_object_count`. This is the single easiest thing to get wrong.
4. `MTRL`, `PHY2`, and the `SWIT` + `NODE`/`STAT`/`CHDR`/`CEXE` destruction machine.
5. Model header (descriptor row 0's `INFO`): AABB `+0x04`/`+0x10`, node count `+0x20`,
   `lod_count` `+0x34`, LOD distance `+0x38`.

**Per streamed rung (`_P001_`, `_P002_`):** geometry + its own `INDX` only. No HIER/SEGM/MTRL.
Its `INDX` rows index the **resident** SEGM. Give each rung a contiguous tier band and make sure the
bands don't overlap on the same node — or `apply_supersede` will (correctly) drop the coarser copy.

**Class shapes to respect** (parts differ, base destruction doesn't):
- **car** — body + 4 wheels; ~96–98% of near-tier geometry under the machine.
- **tank** — hull + turret + barrel + treads; 7 switch nodes; wreck from `global_veh_tank_ruin`.
- **heli** — fuselage + rotor + blades; the md500 has 33 switch nodes.
- **bike** — frame + 2 wheels; tiny (a 7,149-tri motorcycle).
- **boat** — no shared body slots at all; a different scheme. Don't assume.

**Family sharing is normal.** `ch_veh_truck_sx2150_{tanker,cargo,mlrs,semi}` are four models sharing a
`ch_veh_truck_sx2150_P000_Q3` block that holds **only textures** (cab/wheel/crane) and **no model of
its own**. Cross-block texture refs resolve through the asset registry; nothing special is needed.

---

## 8. Validate with these (all re-runnable)

```
mercs2_probe --bin assembly_check          # chain, tiers, intact/wrecked triangle table
mercs2_probe --bin lod_chunks   -- <model> # chunk inventory per rung (INDX vs MESH/SKIN vs PRMG)
mercs2_probe --bin segm_join    -- <model> # each rung's INDX resolved against the resident SEGM
mercs2_probe --bin overlap_probe            # nodes double-drawn by two rungs at one tier (must be 0)
mercs2_probe --bin govern_probe -- <model> # per-node intact vs wrecked (--tier N)
mercs2_probe --bin sm_dump      -- <model> # the destruction machine, disassembled
mercs2_probe --bin rigid_probe  -- <model> # rigid/skinned per LOD mask; INDX-row shortfall
```

Regression tests: `crates/mercs2_engine/tests/model_assembly.rs`.

**Invariants a valid model must satisfy:**
- `INDX.len() == sub_object_count` for every rung.
- Every group's `seg_id < SEGM.len()`, and `SEGM[seg_id].seg_id == seg_id`.
- Every SEGM row whose `seg_id` has geometry in a loaded rung yields a draw — instance rows included.
- No `(node, tier)` drawn by two rungs (0 double-drawn triangles).
- Intact and wrecked draw **different** node sets; the wreck brings in ≥1 node the intact body lacks.

---

## 9. Retracted claims

Struck from earlier docs — every one was an artifact of the mis-binding, and they will send you down
a rabbit hole:

- ❌ *"Intact and ruin submeshes share a node and a LOD mask, so a third selection axis must exist"*
  (`model_render_gate_spec.md` §2c). **No.** The wreck has its own node (`0x75F1F74D`), `SHOW`n by
  `DestroyedState`. The two-axis model (LOD × node-enable) is complete. The apparent sharing was the
  `INDX[group]` bug handing meshes the wrong SEGM row.
- ❌ *"The wreck body is `CreateObject`'d separately, not geometry in this container"*
  (`object_assembly_model.md` §7). **No.** The wreck body IS in the container. `CreateObject` spawns
  debris.
- ❌ *"Tanks leave ~40% of their geometry ungoverned by the destruction machine."* Measurement
  artifact — `SHOW`/`HIDE` act on subtrees.
- ❌ *"A wreck must have fewer triangles than the intact body."* A heuristic, and false.
- ❌ `mesh::near_view_state` / `best_near_view_state` (both deleted) and the "cross-fade window
  `0x0E`" story — both existed only to cope with vehicles whose near geometry we had never loaded.

---

## 10. Still open

- **`minLOD`** (`M+0x80`) — no located on-disk source.
- **`texpng` BC1 alpha decode** is broken (reports opaque skins as ~99% transparent), so alpha
  questions can't currently be settled by eye.
- **`CreateObject` debris and `StartEmitter` fire** are not yet modelled as real child entities.
