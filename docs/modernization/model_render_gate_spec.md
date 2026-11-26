# Model render gate + asset layer — how the engine decides what to draw

Status: **agreed target**, not yet implemented. Written 2026-07-09 from PC decomp.

Motivating symptom: the workshop previews `oc_veh_helicopter_md500` with the intact helicopter and
its wreck drawn on top of each other.

> **Revision note.** An earlier draft of this doc claimed SEGM byte@+3 was unrelated to distance-LOD
> and that the workshop's "States / LODs" panel was an artifact. **Both claims were wrong** and are
> corrected below. The error: a first pass checked `FUN_00490220` / `FUN_0066ee80` (which *are* a
> distance-LOD system — for streaming renderables in and out), found no link to byte@+3, and
> concluded there was none. The mesh-tier LOD selector is a *different* chain (`FUN_00470740` →
> `FUN_0047724e`). Absence of evidence in the wrong function is not evidence of absence.

---

## 1. The gate (ground truth)

Two sibling draw-dispatch loops, `FUN_004722a0 @0x004722a0` (line 57452) and
`FUN_00472a50 @0x00472a50` (line 57632), carry the identical test. Both fully decompiled.

```c
// per SEGM record, stride 4, array at M+0x50 where M = *(int*)(OBJ+0x1e0)
if ( renderable[ rec.seg_id ] != 0                              // (1) geometry exists
  && ( OBJ.view_state[view] & rec.lod_mask ) != 0               // (2) LOD rung membership
  && ( rec.node < 0 || OBJ.node_enable[ rec.node ] != 0 ) )     // (3) destruction SHOW/Hide
        draw(...);
```

Literal clause (2), from `FUN_004722a0`:
```c
((*(byte *)(param_1 + 0x352 + (uint)uVar2 * 0x24) & *(byte *)((int)psVar1 + 3)) != 0)
```

### SEGM record layout

| off | type | role |
|-----|------|------|
| +0 | **i16, signed** | HIER node index. Doubles as attachment bone AND key into the `OBJ+0x2a0` node-enable table. `< 0` ⇒ clause (3) always passes. |
| +2 | u8 | `seg_id` — indexes the renderable-pointer array `M+0x58`. **The** index field. |
| +3 | u8 | **LOD-rung membership bitmask.** ANDed with `view_state`, `!= 0`. Never used as an index. |

### The two axes

**Axis A — LOD rung (clause 2).** `OBJ+0x352` (per-view, stride `0x24`) is *recomputed every frame*
from camera distance:

```c
// FUN_00470740 — distance -> rung n
fVar4 = dist / (*(float*)(model+0x84) * k);
cVar2 = (char)((uint)fVar4 >> 0x17) + -0x7e;     // float exponent ≈ floor(log2)
if (cVar2 < model[0x80]) cVar2 = model[0x80];    // clamp minLOD
else if (model[0x7c]-1 < cVar2) cVar2 = model[0x7c]-1;  // clamp maxLOD-1

// FUN_0047724e — rung -> view_state
*(short*)(OBJ+0x34c) = 1 << (n     & 0x1f);
*(short*)(OBJ+0x350) = 1 << (n + 1 & 0x1f);
*(short*)(OBJ+0x34e) = (model[0x1e0]->0x80 < (int)(char)n) ? 1 << (n-1 & 0x1f) : 0;  // 0 at min rung

if ((OBJ[0x12] >> 9 & 1) == 0)  OBJ[0x352] = OBJ[0x34c];                       // single bit: 1<<n
else                            OBJ[0x352] = OBJ[0x34e]|OBJ[0x34c]|OBJ[0x350]; // 3-rung cross-fade
```

So a segment's `lod_mask` is the **set of rungs it is present at**. md500's masks decode cleanly:
`7` = rungs 0-2, `12` = rungs 2-3, `112` = rungs 4-6, `127` = all rungs. mattias's accessories at
`0x0F` = rungs 0-3. `mesh::state_tiers()` decomposing into seven bits is decomposing into the seven
rungs — that is correct. **`active_bit = 0x01` means "rung 0" (highest detail), which is a legitimate
preview choice, not an invented constant.**

Note: this is *not* the same LOD system as `FUN_00490220` / `FUN_0066ee80`, which bracket squared
camera distance against near/far pairs to **stream renderables in and out**. Two distinct mechanisms:
asset-residency LOD vs mesh-segment LOD.

**Axis B — destruction state (clause 3).** `OBJ+0x2a0` is a per-node byte table sized
`OBJ+0x284` = model node count (`M+0x5c`). Three parallel tables:

| addr | role |
|---|---|
| `+0x2a0` | base / authoritative node-enable, written by the SWIT state machine's SHOW/Hide |
| `+0x2a4` | sibling table, same size |
| `+0x2a8` | per-frame working copy |

Refresh on dirty bit 12 of flags `@+0x10` (`FUN_004737f0`, `FUN_00474f70`):
```c
if ((OBJ[0x10] >> 0xc & 1) != 0) {
    memcpy(OBJ[0x2a8], OBJ[0x2a0], OBJ[0x284]);
    OBJ[0x10] &= 0xefff;
}
```
Teardown: `FUN_004725b0` nulls `0x2a0/0x2a4/0x2a8/0x284`.

**This is the pristine-vs-wreck selector, and it is orthogonal to LOD** — matching the
`LOD tier × destruction state` matrix in `world-lod-and-destruction-scope`.

### On-disk → runtime SEGM mapping (mirror this)

With `M = *(int*)(OBJ+0x1e0)`:

| addr | contents |
|---|---|
| `M+0x4c` | segment count |
| `M+0x50` | SEGM record array, stride 4 (layout above) |
| `M+0x58` | per-seg renderable pointer array, indexed by `seg_id` |
| `M+0x5c` | node count (= size of the `0x2a0/0x2a4/0x2a8` tables) |
| `M+0x60` | node/bone matrix array, stride `0xb0` |
| `M+0x7c`, `M+0x80` | maxLOD, minLOD clamps |

---

## 2. What we do today

| | where | what |
|---|---|---|
| Clause (2) | `mercs2_engine` `mesh.rs::build_indexed_state` | applied at **build time**, `active_bit = 0x01`; survivors **baked into the vertex buffer** |
| Clause (3) | `mercs2_workshop` `app.rs` only | `orchestrator::machine_group_visibility` → `set_draw_hidden`, at **draw-group** granularity, failing **open** (`unwrap_or(false)`) |
| Clause (1) | — | implicit |

Consequences:

- **The wreck renders.** Clause (3) fails open. md500: 19 PRMG groups → 8 survive the build-time
  mask → 1 hidden by the machine. The wreck is in the other 7.
- **The game is worse than the workshop.** `machine_group_visibility` / `parse_state_machine` appear
  *only* in `mercs2_workshop`. `game_world.rs::load_model_by_hash_state` goes straight to
  `build_indexed_state(container, 0x01)`. Every destructible vehicle in-game renders its wreck
  welded to its body.
- **Visibility is keyed by model hash, not entity.** `scene.rs`:
  `hidden_draws: HashMap<u32, HashSet<usize>>`. Two md500s cannot be in different damage states —
  hiding a group on one hides it on both. Destruction is *structurally impossible*, not merely wrong.
  Same root cause as F11's tier switch mutating every placed copy.
- **LOD is frozen.** We bake rung 0 into the buffer, so no distance LOD, ever.

### Independent confirmation from the UH1 field evidence
UH1 (`0x89D8DE72`): group 14 mask `0x03`, groups 20/24 mask `0x01`; only 20/24 drew at spawn. Under
ANY-bit there is **no** `view_state` producing that split — any state containing bit 0 draws all
three. Therefore clause (3) suppressed group 14. (Also: a mask-`0x03` group is present at rungs 0-1,
i.e. it is a *lower-detail* variant of the mask-`0x01` body — consistent with `uh1huey_lod_dm`.)

---

## 2b. The deeper problem: we have no asset layer

**A model container is not the asset.** (User: "think of the wad more like a raid array … the names
are misleading if you treat them as the whole picture.")

Measured with `mercs2_probe --bin aset_probe`:
- **No intra-hash striping.** All 3,007 type-19 model hashes have exactly one ASET row in one block.
- **1,236 of 3,007 have no primary row** (`sub_entry != 0xFFFF`) — sub-entries inside another block's
  chunk list. This is the catalog's "parts in the model list".
- **Sharing is the cross-block asset graph.** md500's model chunk is in block **3350**; its textures
  `0x22101D86` / `0xFB385BF0` are in blocks **2976** / **2977**. A hash can own rows of several types
  (`0x9FCAE910` is both type-19 model and type-27 texture).

`type_id ↔ type_hash` is 1:1 and already fully enumerated in [`docs/type_hash_registry.md`](../type_hash_registry.md)
(19 = model `0x5B724250`, 27 = texture `0xF011157A`, 12 = scrub `0x600B904E`, +20 more). Consume it.

### What the retail engine does

1. **Block load → per-chunk dispatch.** Two dispatch stages. Stream jobs dispatch through a fnptr
   table `PTR_LAB_019f904c` indexed by an integer job-type id (`FUN_00876400`). Inside a container,
   the entry reader `FUN_00464780` (count `this+8`, entries `this+0x10` stride `0x14`) is walked and
   branched on the entry's **FourCC `type_hash`** — see the CHDR gate `FUN_00654940`'s
   `enum/COMP/UNIQ/CHDR/flgs/flgt` chain (Ghidra: "could not recover jumptable at 0x00654ed0").
2. **Global registries**, all open-addressing. Probe primitive `FUN_008242b0(key, size)` (8-way
   unrolled linear probe, `slot = key % size`). Textures: 5120-cell paged pool (`%0x1400`, stride
   `0x54`), key = texture hash, type-gated on `record+4 == 0xF011157A`. Render-resource handles:
   256-slot (`%0x100`), keys `DAT_0197de48`, values `DAT_0197da48`, null sentinel `DAT_0197da44`.
   Materials: per-renderable `0xc`-stride table (`FUN_004acc30`).
3. **Insert is get-or-create — FIRST-wins, not last-wins.** `FUN_004cc130`:
   ```c
   iVar2 = FUN_008242b0(0x1400);
   piVar3 = (iVar2 < 0) ? param_1 + 2 : param_1 + iVar2 + 3;
   iVar2 = *piVar3;
   if (iVar2 == 0) { iVar2 = (**(code **)(*param_1 + 0x1c))(param_2); ... return iVar2; }
   return iVar2;   // OCCUPIED -> return existing cell, create nothing
   ```
   A second block carrying the same texture hash is **ignored**. Consistent with high-mip streaming:
   higher mips are separate BODY chunks assembled into one cell's chain, not competing copies.
   ⇒ `wad.rs:115`'s "the game's global registry keeps whichever is loaded last" is **wrong**.
   (`asset.rs`'s three "last wins" comments are **right** — they describe the *WAD overlay stack*,
   base + `vz-patch.wad`, which is a file-resolution rule. Two different layers, same phrase.)
4. **No registry eviction.** `FUN_008738f0` frees finished stream nodes and decrements resident bytes
   `mgr+0x4c368`; render objects refcount (`FUN_004c0730`: `piVar8[2]++`, gate
   `want = (obj[3]==0 && obj[2]>0)`). But no removal-on-unload was found and registry records carry
   **no owning-block id**. A stale reference resolves to the null sentinel and is dereferenced
   unchecked → the known AV at `0x47AA5C`.
5. **Resolution.** `ModelName` COMP `{u32 entity_key, u32 model_hash}` → handle-table probe
   (`FUN_008731f0`). Resident ⇒ handle. Not resident but streamable ⇒ load-on-demand
   (`FUN_006654b0`, `FUN_004bf8c0` force-load by type-hash). Genuinely absent ⇒ NULL sentinel.

### What `mercs2_engine` does
`asset.rs::AssetSource::extract_container(hash)` → find one ASET row → decompress that block → slice
one span → parse in isolation. **Blocks are a lookup mechanism, not a residency unit. No registry.
Nothing shared.** Grep for block residency or asset registry in `mercs2_engine/src` returns nothing.

⇒ **The workshop is faithfully driving `mercs2_engine`; `mercs2_engine` is what diverges from retail.**
Anything an object needs that is not physically inside its own container is re-extracted ad hoc
(textures, resident mip only) or **absent**: sibling LOD/damage assets (`uh1huey_lod_dm`), hardpoint
weapon variants, destruction-machine `CreateObject` (`0xC6E8AFA8`) spawns.

**Ordering:** the gate is a property of an *instantiated object* — `view_state` and `node_enable` live
on the object. We cannot model per-object state while we have only parsed containers. **The asset
layer is the prerequisite, not a follow-on.**

---

## 2c. ~~Correction: destruction is NOT purely node-level (intact/ruin submesh interleaving)~~ — ★RETRACTED (2026-07-12)

**This entire section was WRONG.** It described an artifact of a binding bug, not the engine.

It claimed the tank's intact and ruin submeshes "share the same SEGM node (18) and the same LOD mask",
and concluded that a **third selection axis** must exist below the node table. Both the observation and
the conclusion are false:

- The apparent sharing came from resolving a mesh's segment record with **`INDX[group_index]`**.
  `INDX` is indexed by **sub-object ordinal** (`MESH`/`SKIN` under `GEOM`), *not* by PRMG group — so
  every group past the first divergence read the **wrong SEGM row**, i.e. the wrong node AND the wrong
  LOD mask. Intact and ruin never shared a node; we just looked them both up incorrectly.
- The wreck body has **its own HIER node** (`0x75F1F74D`), `SHOW`n by `DestroyedState`, while
  `PristineState` `SHOW`s the intact body (`0x255EAB53`). Node-granular gating separates them exactly
  as designed.

⇒ **The two-axis model (LOD mask × node-enable) is COMPLETE.** There is no third axis. Do not build a
material/submesh selector, and do not add the "hide `*_ruin_dm` in the pristine preview" stopgap — it
would paper over a correct engine with a heuristic.

Verified after the fix: **0 triangles drawn identically intact vs destroyed**, across the car van, the
ztz98, the amx30_aa and the amx30_elite (`mercs2_probe --bin govern_probe`).

**See `vehicle_model_spec.md`** for the corrected binding chain, the LOD-block chain, and the
destruction model. The lesson stands, just aimed at the right target: treat structural claims as
hypotheses until a real model refutes them — including the ones in this document.

## 3. Target architecture

### 3.1 Asset layer (`mercs2_engine`)
```rust
struct AssetRegistry {
    resident: HashMap<u16, ResidentBlock>,          // refcounted; block = residency unit
    models:    HashMap<u32, Rc<ModelResource>>,     // get-or-create, FIRST-wins
    textures:  HashMap<u32, Rc<TextureResource>>,
    machines:  HashMap<u32, Rc<StateMachine>>,
}
impl AssetRegistry {
    fn make_resident(&mut self, block: u16);        // decompress -> entry walk -> dispatch on type_hash
    fn resolve_model(&mut self, hash: u32) -> Option<Rc<ModelResource>>;   // miss -> load-on-demand via ASET
    fn instantiate(&mut self, model_hash: u32) -> Option<Object>;
}
```
Preserve retail semantics: first-wins insert; `resolve` falls back to ASET-driven load-on-demand;
`Option` where retail returns a null sentinel (we return `None` instead of crashing).
The WAD overlay stack (last-wins) sits *below* this, unchanged.

### 3.2 Per-object render state
```rust
struct RenderState {
    lod: u8,                  // rung n, recomputed per frame from distance + model min/max clamps
    view_state: u8,           // 1<<n, or n-1|n|n+1 when cross-fading
    node_enable: Vec<bool>,   // len = model node count; written by the state machine
    dirty: bool,
}
```
An ECS component on the **entity**, not a `HashMap` keyed by model hash. `Scene::hidden_draws` dies.

### 3.3 Draw
`build_indexed_state` stops filtering — models upload **whole**, every group, every segment, once,
each draw range tagged with its `{node, seg_id, lod_mask}`. The draw loop evaluates all three
clauses per segment against the entity's `RenderState`. LOD becomes free and real; destruction
becomes possible; two instances can differ.

### 3.4 UI
Two independent controls, because they are two independent axes:
- **LOD rung** (0..maxLOD) — currently mislabelled "States / LODs". It *is* LOD; label it that.
- **Destruction node states** — the existing per-node state-machine selector.

Default preview: rung 0, plus the state machine's default state (pristine).

---

## 4. Catalog taxonomy

`Models (3007)` is ~80% not a previewable standalone model:

| Category | Count |
|---|---|
| Unresolved raw hashes | 1,607 |
| `vz_*_tinygeometry_*` region imposters | 783 |
| `global_weapon_*` (bone/hardpoint attachments) | 77 |
| `global_veh_*` **shared base assets** (`global_veh_van` ↔ `civ_veh_car_van_*`; `global_veh_turbosquid` ↔ `gr_/oc_veh_boat_turbosquid`) | 10 |
| `global_*` props (legitimately standalone) | 136 |
| other named | ~393 |

Presentation: **faceted tabs** — Models / Props / Weapons / Shared / Imposters / Unnamed.

- Row `0xE25098E4  "Y8n\"` is a **rainbow-table preimage collision**, not a name. Suppress
  implausible preimages; the workshop shows game-data labels only.
- `global_veh_*` as a *prefix rule* is a heuristic. The durable classifier is the **reference graph**:
  a model is standalone iff some entity references it (`ModelName` COMPs across world blocks ∪ Lua
  `Pg.Spawn`/template hashes). It also labels the 1,607 unnamed.

---

## 5. Corrections to existing docs

| Doc / memory | Claim | Status |
|---|---|---|
| `accessory_bone_binding_A.md` | gate lives in `FUN_00477e20`/`FUN_0047a6c0` | **wrong**. Those are a different pass (`FUN_0047a6c0` gates on bit 10 of a status word, stride `0x1c4`/`0x10`). Gate = `FUN_004722a0`/`FUN_00472a50`. |
| `accessory_bone_binding_A.md` | SEGM +0 = `u16` bone index | **refine**: signed `i16` HIER node; negative = always visible. Bone and enable-table key are the same field. |
| `ucfx_tag_registry.md` | SEGM byte@+3 = "group" | **misleading** if read as an index. It is a bitmask, never indexed. `lod_mask` is the accurate name. |
| `rendering_fx_lighting_gap.md` | byte@+3 semantics unreconciled | **resolved** — LOD-rung membership bitmask, ANY-bit. |
| `vehicle-group-render-all-bits-mask` memory | real rule is ALL-bits `(mask & S) == mask` | **wrong mechanism**. Gate is ANY-bit; UH1 observation caused by clause (3). |
| `mercs2-workshop-devtool` memory | "Destruction states + LOD chains are the same mechanism" | **wrong**. Two orthogonal axes: `lod_mask@+3` vs `node_enable@0x2a0`. |
| `wad.rs:115` | runtime registry "keeps whichever is loaded last" | **wrong** — `FUN_004cc130` is get-or-create, first-wins. (`asset.rs`'s "last wins" = WAD overlay stack, correct.) |
| *this doc, earlier draft* | byte@+3 unrelated to LOD; "States/LODs" is fiction | **wrong**, retracted — see revision note. |

---

## 6. What is NOT in the wad (live-read items)

There is **nothing to snapshot**: the wad is deterministic, versioned input, and a golden capture of
today's output would enshrine the bug. Regression protection is an **invariant**, not an artifact:
for every SWIT-less container (all characters — `mattias_v3` has no SWIT/NODE), clause 3 is vacuous,
so the new gate at `view_state = 0x01`, all nodes enabled, must be **bit-identical** to today's
`build_indexed_state(c, 0x01)`. Pure property over wad data; cannot rot.

`view_state` is **not** a spawn constant — it is recomputed per frame from camera distance
(`OBJ+0x354 + view*0x24` holds `n`). Nothing to capture; compute it the way `FUN_00470740` does.

### 6a-2. The selector's arguments and the main view's factor (2026-11-25)

Read from the pc-v10 instructions at the selector's two call sites and from retail memory captures.

- **`FUN_0047724e` → `FUN_00470740` (site `0x004773bb`).** `k` is the float at `view + 0xe70`
  (`0x0047739d FLD [EDI+0xe70]`, stored to the pushed slot at `0x004773b4`), `view = owner + idx·0xe80
  + 0x10`, `idx` the word at `owner + 0x2b92`. The distance in XMM0 is the object's cached
  `OBJ + 0x36c + idx·0x24`; the model in EDX is `*(OBJ + 0x1e0)`. `OBJ + 0x368 + idx·0x24` caches the
  `k` the rung was computed with, and the rung is recomputed when the view's `k` differs from it.
- **`FUN_0042f920` (site `0x0042fa36`)** passes `k = 1.0` (`FLD1`).
- **The selector holds `x = dist / (scale · k)` to at least `0.2`** (`DAT_00b9854c` = `0x3E4CCCCD`)
  before taking the exponent.
- **`owner` is the static `0x017CFAF0`** (`*0x00DFC2F8`). Two views. In five v1.0 captures (the
  `23fe7f38` PMC one at 640×480; Chris mid-mission, flying-winch and new-game, and Mattias jacket, at
  1600×900) the float the selector reads is **`1.0` for `idx` 0** (`0x017D0970`) and **`0x3F2C092D`
  ≈ 0.67202 for `idx` 1** (`0x017D17F0`).
- **The main camera is `idx` 1.** In the mid-mission capture viewport 1 holds the live eye
  (`dump-render`), and the render object of an entity 6.7 m from it (`0x1E74FBC0`) has, in its
  `idx` 1 block, rung 0, `view_state` 3, cached `k` 0.67202 and cached distance 6.728; its `idx` 0
  block was never computed (cached `k` −1.0).
- **So for the main camera, rung `n ≥ 1` starts at `header+0x38 × 0.67202 × 2^(n-1)` metres.** A house
  with the reference 100 leaves rung 0 at 67.2 m and enters rung 2 at 134.4 m.
- No store to `view + 0xe70` was found statically (the view/projection builder `FUN_00858f30` is a
  SecuROM indirect stub), so what changes the factor is not known.

### 6a. Live x32dbg confirmations (2026-07-10)

Captured render objects via a single-shot BP at `FUN_00472a50` (`OBJ` in `ECX`; delete the BP the
instant it hits — a *conditional* BP on this per-frame gate wedges the session). Confirmed against
two real props:

- **Header → runtime `M` chain is correct.** `M = *(OBJ+0x1e0)`; `M+0x08 = 0x5B724250` ("model").
  `M+0x7c` (maxLOD) read **5** and **4** — matches container header `+0x34`. `M+0x84` read **60.0**
  and **30.0** — matches header `+0x38`. So `parse_model_header` reads exactly the runtime's fields.
- **`M+0x80` (minLOD) = 0** on both props.
- **Cross-fade = bit 9 of `OBJ+0x12`**, confirmed from the live `FUN_0047724e` body (NOT bit 13 —
  the `>>0xd` branch there is a distance-cache refresh):
  ```c
  cVar4 = FUN_00470740(...);                    // rung n, clamped to [M+0x80, M+0x7c-1]
  ... 1<<(n) , 1<<(n+1), (M+0x80 < n ? 1<<(n-1) : 0) ...
  if ((*(ushort*)(OBJ+0x12) >> 9 & 1) == 0) view_state = 1<<n;                        // single
  else                                      view_state = 1<<(n-1)|1<<n|1<<(n+1);      // window
  ```
  Both sampled world objects have bit 9 **set** → they render the 3-rung window.

### 6b. The near-view_state model (derived; minLOD source still open)

At the camera, `n → minLOD` (the clamp floor). So the **near** view_state is:
- character (bit 9 clear): `1<<minLOD` — for minLOD 0, `0x01`, a clean full body (what the workshop
  already shows). A window here would draw two hair LODs at once = the overdraw we must avoid, which
  is *why* characters cannot cross-fade.
- world object (bit 9 set): `window(minLOD)` — `0x03` for a minLOD-0 prop, and **`0x0E` for the
  tank**, which is the only view_state that assembles its hull+turret+barrel (bits 1/2/3). Since
  retail renders whole tanks and the tank's geometry needs `0x0E = window(2)`, **the tank's minLOD is
  ~2** — minLOD is genuinely per-model.

**OPEN — the one thing neither the decomp agent nor the live reads settled:** where `M+0x80`
(minLOD) and the bit-9 cross-fade flag come from. The flag is set per render-instance by the
scene-graph attach path (`FUN_008eb500`/`FUN_008e0f20`), i.e. plausibly per object *class*, not in
the container. minLOD's container/derivation source is unknown (it is NOT an obvious model-header
field — header `+0x30` is `{tank:0, mattias:4, md500:0}`, not minLOD-shaped). Live capture gives the
runtime *value* but not the disk source, so this is offline work: either find the `M+0x80` writer
(a HW write-watchpoint on `M+0x80` for a freshly-loaded model → its constructor), or derive minLOD
from the container's LOD/SEGM data and validate against a live vehicle read.

---

## 7. Plan

**Asset layer before gate** (§2b).

- [x] **1. Asset layer** — `mercs2_engine::registry::AssetRegistry`: block residency, a global
  `(type_hash, name_hash)` chunk registry, **first-wins** insert, last-wins WAD-overlay block
  selection, ASET-driven load-on-demand, and coherent eviction (retail has none — it leaks entries and
  faults on the stale handle at `0x47AA5C`). `AssetSource::extract_container` routes through it, as
  does `mercs2_workshop`'s `WadStack` — which until now was a *verbatim copy* of `AssetSource` that
  never touched the engine's asset layer at all. `type_id ↔ type_hash` consumed from
  `mercs2_formats::types::TYPE_HASH_REGISTRY`.
  *Verified against the real wad*: md500's model resolves from block 3350; `0x9FCAE910` registers as
  both model and texture; its textures stream from blocks 2976/2977 (3 resident blocks for one
  helicopter); resolved bytes byte-identical to the legacy `wad::extract_container`; eviction
  unregisters and re-streams. Resolving md500 registers 5 chunks, `boat_destroyer` 18, mattias 1.
- [x] **2. Whole-model build + the gate as a pure function** — `mesh::build_indexed_all` keeps every
  segment; `DrawGroup` now carries `{lod_mask, node, sub_object}`. `render_state::RenderState`
  implements clauses (2) and (3); `lod_rung` / `view_state` port `FUN_00470740` / `FUN_0047724e`
  including the bit-9 cross-fade and the zeroed `n-1` term at the bottom rung.
- [x] **3. Invariant test** (§6), against the real archive. Measured: **160 SWIT-less models select an
  identical segment set** under the draw-time gate at rung 0 as under the legacy build-time filter;
  40 have a state machine and are expected to differ; **0 models carry a `mask == 0` segment**, so the
  legacy `mask==0 → always keep` quirk never fires on real data and the ANY-bit rule is safe.
  Also proven on UH1's real masks: **no `view_state` draws a mask-`0x01` segment while hiding a
  mask-`0x03` one** (0x03 ⊇ 0x01 under ANY-bit) — so the mask cannot explain the UH1 observation, and
  clause 3 must. The all-bits reading is dead.
- [x] **4. Per-entity `RenderState`.** `Scene::entity_state: HashMap<Entity, RenderState>`; the draw
  loop gates every draw call against it. `hidden_draws` survives, correctly scoped: it is a
  MODEL-level authoring/streaming override (terrain LOD tile swap, workshop isolate-a-group), never
  object state. Every spawn site installs a state; `forget_entity` drops it.
- [x] **5. The gate is in the draw loop.** `DrawCall` carries `{lod_mask, node}`; the workshop builds
  models whole (`build_indexed_all`) and switching LOD rung is now free — it writes `view_state` on
  the entity, with no rebuild, no re-upload, and no effect on other instances of the same hash.
- [x] **6. The destruction machine writes `node_enable`.** `orchestrator::machine_node_enable` returns
  the per-HIER-node table (`machine_group_visibility` is now a thin wrapper over it, and has no
  callers). ★**Clause 3 must be keyed on the SEGM record's `node`, NOT on `INDX[group]`** — they
  disagree on 5 of md500's 19 groups. Groups 0 and 1 both sit on SEGM node 2, which the default state
  disables; INDX maps group 0 to node 0 (enabled), so the old INDX keying hid group 1 and left
  **group 0 — the wreck — drawn**. That was the bug on screen. Regression test:
  `clause_3_must_be_keyed_on_the_segm_node_not_on_indx`.
- [x] **7. The `node_enable` seed is DATA, not a debugger read.** `machine_node_enable` at the default
  state gives **98 of 107 nodes enabled** on md500: every `SWIT` participant subtree starts *hidden*,
  everything else starts *visible*, and the chosen state's `SHOW`/`Hide` scripts flip subtrees from
  there. The 28-variant experiment collapses to one variant. At rung 0 the LOD mask admits 8 groups
  and clause 3 removes exactly 2. (The x32dbg route cost a 25-minute world load and would have
  settled one byte — see the `x32dbg-mcp-pitfalls` memory.)
- [ ] **8. `instantiate(hash) -> Object`** — fold model resolve + machine + `RenderState` into one
  engine seam, so the game's streaming spawn gets this too. Today only the workshop wires it.
- [ ] **9. Per-frame LOD**: drive `view_state` from `lod_rung(camera_distance, …)` instead of a fixed
  rung. `render_state::lod_rung` exists and is tested; nothing calls it yet.
- [ ] **10. Model fit/bbox** should be computed over *visible* geometry, not all 19 groups — break
  pieces authored at ejected anchors inflate the orbit radius (destroyer reads r=90m).
- [ ] **11. Relabel the UI**: the chips are LOD rungs, not "LOD chains + destruction states". Two axes.
- [ ] **12. Facet the catalog.** Follow-on: reference-graph classifier.

Diagnostic added: `cargo run -p mercs2_probe --bin gate_probe -- <model>` — per draw group, the SEGM
node, the INDX node, the LOD mask, and whether the machine's default state enables each. This is what
exposed the SEGM-vs-INDX disagreement.

Diagnostics:
- `cargo run -p mercs2_probe --bin aset_probe -- 0xHASH…` — every ASET row a hash owns + striping census.
- `cargo test -p mercs2_engine --test registry_wad_probe -- --ignored --nocapture`
- `cargo test -p mercs2_engine --test gate_invariant_probe -- --ignored --nocapture`
