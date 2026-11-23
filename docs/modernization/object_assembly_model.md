# The Mercenaries 2 object-assembly model

How the Pandemic "Pangea" engine builds a live in-world object, so our Rust reimpl can *reconstruct*
one instead of parsing a model container flat and calling that the object. Written 2026-07-10 from a
three-front corpus investigation (class/instantiation, destructible behavior, cross-WAD assets) +
the tank's live destruction state machine + live x32dbg reads.

> Provenance note: every "clean separation" claim I made earlier this session (LOD vs destruction as
> two orthogonal axes; SEGM-mask as the destruction selector; intact/ruin interleaved under one node)
> was a flat-parse artifact that reality corrected. Treat the crisp parts below as load-bearing and
> the one open question in §7 as genuinely open.

## 0. The reframe (this is the important part)

**An in-world object is not a C++ class instance with an inheritance tree. It is an ECS entity —
a bag of components — acted on by shared native systems.** A tank does not *inherit from* Vehicle
which inherits from Destructible. A tank is an entity carrying `{Transform, Model, Vehicle, Health,
RuntimeHealth, ObjectScript, …}`, and the "vehicle destruction behavior" is a **shared native system**
that reads those components, parameterized per model by data inside the model's own container. The
only actual inheritance is a thin Lua prototype chain, and it only does radar/blip.

So the user's instinct — shared class-level behavior, parameterized per vehicle — is correct in
substance; the *mechanism* is composition + shared systems, not an inheritance hierarchy. That makes
reconstruction cleaner: `object = component set + the systems that read them + assets assembled by
hash`.

We have been reconstructing **only the geometry of the Model component** and treating it as the whole
object. That is the root of every rendering surprise this session.

## 1. Three distinct "class" systems (not one)

| system | what it is | where | evidence |
|---|---|---|---|
| **(A) Native reflection registry** | ~231 **component types** + their deserializers. Each = a 0x50-byte descriptor (dword0 = `CopyFromStream` vtable; `pandemic_hash_m2(name)` key; u16 stride @+0x24; name @+0x3c). | EXE `.data`, populated at static-init from the global-ctor table → `FUN_0064a770` (registry array `PTR_PTR_00edbec8[DAT_01176058++]`). **Not** in the WAD, **not** Havok RTTI. | [`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/); `FUN_006473d0` is the `Model`-component builder, not a general class builder |
| **(B) On-disk COMP records** | an entity's **component set** as placement data | world blocks (`layers_static` blk 29, `vz_state` overlays); `COMP.info = {name_hash, type_hash, count}` + `schm` + `data` | [`docs/ecs_components.md`](../ecs_components.md) |
| **(C) Lua prototype pseudo-classes** | `Inheritable→Blippable→OrientedBlippable→VehicleBlippable→tank/helicopter`; metatable `__index` chain; duck-typed engine↔script by function name (`Init/OnDeath/OnStateChange`) | [`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd); C++ link = `ObjectScript` = two name-hashes | radar/blip only; **zero destruction logic** |

## 2. The entity object

- Layout: `{ header, 256-bucket component slot table @+8, intrusive component list @+0x808 }`,
  vtable `0x00BD2100` (ctor `FUN_0071aa60`). Get-or-add-component = `FUN_00873f20` → probe
  `FUN_008242b0(class_hash, 0x100)` → slot `this+8+id*4`.
- A **system** finds entities with component C via C's own instance pool (each reflection class owns a
  `0x9e3779b9`-hashed pool; insert `FUN_0064a600 = memcpy(dst, rec, stride)`).
- Health is native components: `Health` (`0x06be1abf`, float + 3 flags), `RuntimeHealth`
  (`0xf9b9b2a5`, **`{max, cur}`** — see below), `RuntimeNodeHealth` (`0x76927bf5`, **one `u16` per
  destructible node** — see below). Produced by `FUN_004cfed0`. That per-node health is how
  doors/barrels fall off independently.

  > **⚠ Corrected 2026-07-26 — this line previously read `{cur,max}` and "one float per node".**
  > Both were backwards/wrong, and both were disproved independently by two blind validation passes
  > and re-read here:
  > - **`RuntimeHealth` is `{ +0x00: f32 max, +0x04: f32 cur }`.** `Object.GetMaxHealth`
  >   (`0x005CC030`) loads `[rec + 0x00]`; `Object.GetHealth` (`0x005CBDB0`) loads `[rec + 0x04]`;
  >   `Object.SetHealth` (`0x005CBEE0`) clamps against `[esi]` and stores to `[esi+4]`. The container
  >   is `0x017BEF78`, which names itself `RuntimeHealth` via `[vtable+0x34]`, and its registered
  >   stride is **12 bytes** — so the record is 3 dwords and the third field is still unidentified by
  >   any document.
  > - **`GetNodeHealth` reads a `u16`, not a float** (`movzx` → `cvtsi2ss`, an integer widening).
  >
  > Anyone parsing a `RuntimeHealth` blob out of a WAD must use the engine order. Note that
  > `mercs2_core::Health` declares `{cur, max}` — that is *our* struct's field order and is
  > deliberately not a mirror of the engine layout; nothing memcpys between them.

## 3. Instantiation

`Pg.Spawn(name|hash)` → hash → **name registry @`0xDF6B88`** (open-addr, `bucket=(0x9E3779B9*hash)%n`;
value bit-31 = template handle) → **template = a COMP set on disk** → deserialize each COMP via its
class `CopyFromStream` → attach to the entity's component table. The terminal spawn worker is
SecuROM-thunked (`0x24F3200`, live-only).

## 4. Entity → render node (two objects)

The **entity** (vtable `0x00BD2100`) references a **render node** (vtable `0x00BAB0F0`) through its
**`Model` component** (a 4-byte handle, type `0x5B724250`). They are distinct objects — which is why
the camera-owner record has `+0x1e0 = null` and only the render node has `+0x1e0 = model resource M`.
The render node is where the draw gate runs: `M+0x4c` node count, `M+0x50` node records, `M+0x2a0`
node-enable, `M+0x352` view_state. (Exact entity-field offset holding the render-node pointer =
live-read TODO.)

## 5. Asset assembly across the WADs (there is no manifest)

- **Dependencies are declared inside the model container's `MTRL`** as texture *hashes* — 12-byte
  lazy handles `{asset_hash, type_hash=0xF011157A, resolved_ptr}`, ≤10 slots/material (0=diffuse,
  1=spec, 2=normal). No per-object asset list, no reflection manifest. `ModelName` COMP is *only*
  `{entity_key, model_hash}`.
- **Shared / `global_` assets are just hashes some other resident block supplies.** `global_` is an
  authoring convention; resolution is pure hash against the **first-wins** registry (`FUN_004cc130`;
  5120-cell texture pool). The tank's own MTRL points at `global_veh_tank_ruin_dm` (`0x78AE5A1E`) the
  same way it points at its private skin.
- **Residency is spatial** (c3 cells) + an always-resident block; there is no per-object block list.
  A model's own block is found via its type-19 ASET row; texture hashes resolve lazily against
  whatever is resident (miss → load-on-demand `FUN_006654b0`; absent → NULL sentinel → the `0x47AA5C`
  AV).
- **Sounds** = Lua-global banks (`veh_shared`, `destruction_shared`, `MrxSoundBootstrap.LoadBanks`).
  **Effects/debris** = behavior-spawned at runtime (`CreateObject`), separate containers.

Tank graph (`aset_probe`): model block **3565** (private skins `_dm` `0x8C4A7D9D`, `_lod_dm`
`0x2CB99555`, spec/normal co-resident); shared ruin `0x78AE5A1E` in c3-cell block **1569**; sounds
Lua-global; debris = `CreateObject` templates.

## 6. Rendering (the Model component)

Per-segment gate on the render node (`FUN_004722a0`/`FUN_00472a50`):
`renderable && (view_state & lod_mask) && node_enable[node]`.
- **LOD axis**: `view_state = 1<<n` (single) or `1<<(n-1)|1<<n|1<<(n+1)` (cross-fade window, bit 9 of
  `OBJ+0x12`); `n` from camera distance, clamped to per-model `[minLOD (M+0x80), maxLOD-1 (M+0x7c)]`
  read from the container's 72-byte model header (`+0x34` LOD count, `+0x38` distance; verified live).
- **Destruction axis**: per-HIER-node state FSMs from the container's orchestrator chunk (SWIT).

## 7. Destruction as a shared system — and the one open question

Health (native components) → damage **messages** (`0x1ED7AD78`/`0x3D0D4C99`) drained by `FUN_0059be00`
→ `SetStateOnMsg` rules → `SetState` (`FUN_004d3e10`) advances each node through a **global discrete
state vocabulary** (`0xACB51200` pristine → `0x1D5575A1` on-fire → `0x92791EBB` wreck → …). Each
state's enter-script (decoded live for the tank):
- **pristine** `SHOW`s intact HIER subtrees (`0x255EAB53` hull, `0x54C595F0` turret, `0x7893A99D`
  barrel), `Hide`s the break-caps;
- **on-fire** keeps the intact body, `StartEmitter`s fire on hardpoints (named HIER nodes like
  `hp_fx_exhaust_a`);
- **wreck** fires explosion emitters and `CreateObject`s `DebrisTemplate`/`PropDestructTemplate`
  pieces, then `SetState(DestroyedState)`, whose enter-script **`SHOW`s the wreck body**.

  > ★CORRECTION (2026-07-12): the wreck body **IS geometry in this container**, on its own HIER node
  > (`0x75F1F74D`), usually skinned with a shared global asset (`global_veh_tank_ruin_dm`). It is NOT
  > a separately-spawned object — `CreateObject` spawns the loose *debris*, which is a different
  > thing. See `vehicle_model_spec.md` §5.

Shared native machinery (`FUN_004cf340` parse, `FUN_004cfed0` init, `FUN_004d3e10` SetState,
`FUN_0059be00` messages) + global vocabulary = the class behavior; the per-model orchestrator chunk =
the variation. This matches the user's description exactly (100→0 health, increasing damage, fire near
0, parts shed via hardpoints, destroyed at 0).

**~~★OPEN — the `0x78AE5A1E` (`global_veh_tank_ruin_dm`) submeshes.~~ ★CLOSED (2026-07-12).**

They do **not** sit on the intact nodes. That reading came from a binding bug: `INDX` is indexed by
**sub-object ordinal**, not by PRMG group index, so every group past the first divergence resolved to
the **wrong SEGM row** — wrong node *and* wrong LOD mask. The ruin submeshes live on the **wreck node**
(`0x75F1F74D`), which only `DestroyedState` `SHOW`s.

Neither candidate explanation was right: there is **no decal layer** and **no submesh/material
selector**. The two-axis gate (LOD mask × node-enable) fully accounts for it. See
`vehicle_model_spec.md` §2 and §5, and the retraction in `model_render_gate_spec.md` §2c.

(The `texpng` BC1 alpha decoder is still broken, but it is no longer load-bearing for this question.)

## 8. What our reimpl currently gets wrong

We parse the **Model component's geometry flat** (SEGM groups, LOD × node-enable) and treat it as the
object. Missing, in order of leverage:
1. **The entity/component layer** — no entity, no component set, no `Health`. Destruction has no
   *input* (health/damage-state), so we can only ever show one static state.
2. **Correct HIER-subtree attribution** — our SEGM parse collapses distinct intact/wreck subtrees onto
   one node; the engine gates whole subtrees. (This is the "flatten" that produced the overlap.)
3. **Material blend/alpha state** — we render every submesh opaque; the engine alpha-tests/blends
   (the `0x78AE5A1E` decal, cross-fade LOD transitions).
4. **Separately-spawned sub-objects** — wreck bodies, debris, fire are `CreateObject`/`StartEmitter`,
   not geometry in the container.

**Faithful target:** represent the object as `entity + component set + system state`, drive the
Model component's render from (LOD rung from distance) × (node-enable from the destruction system at
the entity's current health) × (per-material blend state), and treat `CreateObject`/emitter spawns as
real child entities. The workshop's "damage" control should be a **health slider** that runs the real
orchestrator, not a mask picker.

## Live-read / follow-up TODO
- Entity→render-node pointer offset; spawn worker `0x24F3200`; render-node ctor for `0x00BAB0F0`.
- The §7 intact/ruin selector (the deciding tests above).
- State-name hash plaintext; per-hit HP math (`ApplyDamageToPrimaryHealth`, string-only).
- Fix `texpng` BC1 alpha decode (currently emits transparent for opaque BC1).
