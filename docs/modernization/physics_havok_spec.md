# Physics Layer Spec — Havok in Mercenaries 2, and the reimplementation plan

**Status:** Research spec / recommendation (no engine code). Feeds [`docs/modernization/00_charter.md`](00_charter.md)
open question: *"Physics: reimplement Havok-equivalent vs adopt a modern physics lib (parity-gate either way)."*
**Date:** 2026-06-30

**Governing charter principle:** *implementation is free, behavior is gated by provable equivalence.*
Exact Havok numerics are **not** required — behavior must match within tolerance at an oracle boundary.

**Double-blind note:** every CONFIRMED claim below converges from **two independent sources** (e.g. the
LE reader `mercs2_formats/src/havok.rs` **and** the BE→LE converter `ucfx_byteswap/src/havok.rs`; or the
Xbox symbol/RTTI inventory **and** the PC Ghidra decomp). Single-source or inferred claims are marked **OPEN**.

---

## 1. Havok data as it exists in the WADs

### 1.1 Middleware version and provenance
- **Havok 5.5.0-r1, 32-bit.** CONFIRMED: build `VersionString` in the Xbox preview
  ([`docs/mercs2-pdb-analysis/havok-physics.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/havok-physics.md)) **and** the version string both converters search for
  (`HAVOK_VER = b"Havok-5.5.0-r1"`). Older `Havok-x.y.z` strings are the version-registry up-conversion
  targets, not the build version.
- The full Havok class set is linked in (`#include <Common/Compat/hkHavokAllClasses.h>`): dynamics,
  collision, constraints, ragdoll, vehicle SDK, animation (`hka*`), behavior graph (`hkb*`), and the
  `hkx*` scene/export layer. ~918 `hk*` reflection symbols; ~95 vtable-resolved classes bridge to the PC decomp.

### 1.2 Where collision data lives: the `PHY2` chunk
A `PHY2` chunk is a **three-part structure** (CONFIRMED — `ucfx_byteswap/src/havok.rs::convert_phy2_be_to_le`,
`format_reference.md §15.2`, memory `phy2-havok-chunk-not-u32`):

```
[ u32 header prefix ][ Havok 5.5 packfile ][ trailing engine collision-wrapper ]
   ~48B, u32 fields    magic-searched, NOT     quaternion + intra-chunk u32
   [count][asset-hash] at offset 0             self-offsets + 0xAA/0xBB fills
   [flags…][pf-size]                           (present on all 110 DLC PHY2 chunks)
```
- The **8-byte packfile magic** `57 E0 E0 57 10 C0 C0 10` is word-palindromic (survives a byte-swap),
  so the packfile is **searched**, not assumed at offset 0.
- **Animation** `data` chunks (type `0x18166555`) are the *same* Havok packfile format (magic at +0, no
  header) — the ragdoll/skeleton side shares the reader.

### 1.3 Packfile internal layout (HK550, 32-bit) — the extractor's structural spec
CONFIRMED by two independent parsers (LE reader `mercs2_formats/src/havok.rs`; BE reader in the converter):

- `__classnames__` marks a table of **three 48-byte section headers** = 20-byte name +
  7×u32 `[abs, lf, gf, vf, exp, imp, end]`. The three sections are `__classnames__`, `__types__`, `__data__`.
- **classnames body:** `{u32 sig, u8 flag, ASCII name\0}` records → `offset → class name` map. These ASCII
  strings are why a blanket u32 swap crashes the loader (`STATUS_OBJECT_NAME_NOT_FOUND` AV) — the loader
  resolves classes **by name**.
- **`__data__`** carries the class instances plus two fixup tables:
  - **local fixups** `{src, dst}` — relocate an object's pointer fields (e.g. `hkArray` data pointers).
  - **virtual fixups** `{src, sec, cnoff}` — bind an object offset to its class name.
- Cross-platform guards baked in: endian / pointer-size / padding / empty-base-class checks (packfile
  header `layoutRules`, 4×u8 at magic+16). This is exactly why the WAD→PC pipeline must fix those bytes.

### 1.4 What collision shapes are stored (and decodable today)
CONFIRMED shape taxonomy (recognised in `mercs2_formats/src/havok.rs::Shape` + present in the Xbox class inventory):

| Shape class | Decode status | Notes |
|---|---|---|
| `hkpConvexVerticesShape` | **Fully decoded** | Destructible **break-piece hulls**: `+64` rotatedVertices (FourVectors SoA, X[4]Y[4]Z[4] = 4 verts/48B), `+76` numVertices, `+80` planeEquations (`hkVector4` = `n.xyz, -support`), `+84` planeCount. Verified on the resident2 crate: 6 hulls `[19,24,35,12,36,10]` verts / `[12,15,22,8,22,7]` planes, real O(1)-metre coords. |
| `hkpBoxShape` | Half-extents (best-effort `+16`) | |
| `hkpMoppBvTreeShape` / `hkpMoppCode` | Recognised, not decoded | **Static non-convex mesh BV-tree** (Memory-Optimized Partial Polytope). Runtime layout pinned: `+0x34` = mopp code size, `+0x10` = mopp info ptr (from Xbox `TtrcMopp`/`MoppShape`). The mopp bytecode is a **u8 buffer** — must not be u32-swapped. |
| `WpMeshShape16` | Recognised, not decoded | **Pandemic-custom** 16-bit-indexed static collision mesh (two u16 index arrays at `+80`/`+88`). |
| Other `*Shape*` | Counted by name | The full HK set exists in-inventory: sphere, capsule, cylinder, triangle, convexTransform/Translate, list, convexList, multiSphere, height-fields, etc. |

**CONFIRMED important nuance:** convex hull vertices are **inset from their face planes by a uniform convex
radius** (Havok's shape `m_radius` shrink) — `havok_extract::hull_faces` selects verts at each plane's
*maximum* `n·v + w`, not at 0. Any reimplementation that rebuilds collision from these verts must preserve
(or re-apply) that radius or contacts will sit slightly inside the visual surface.

### 1.5 Rigid bodies / constraints / motions in the packfiles
- **Rigid bodies:** `hkpRigidBody` (+ `hkpEntity`, motions `hkpBoxMotion`/`hkpSphereMotion`/`hkpKeyframedRigidMotion`/
  `hkpFixedRigidMotion`/`hkpCharacterMotion`/stabilized/thin-box). CONFIRMED present (RTTI + PC vtable ctors
  `FUN_008d4be0` etc.). **OPEN:** per-body mass/inertia/friction/restitution field offsets are **not** decoded
  from the packfile yet — the ECS `PhysicsActor` component carries only a 4-byte handle; the real body is
  Havok-side, sourced from the packfile/asset ([`docs/mercs2-ecs/03_controllers_physics.md`](../../crates/mercs2_workshop/data/mercs2-ecs/03_controllers_physics.md)).
- **Constraints:** the full Havok constraint set is present — `hkpBallAndSocketConstraintData`,
  `hkpHingeConstraintData`, `hkpLimitedHingeConstraintData`, `hkpRagdollConstraintData`/`RagdollLimitsData`,
  `hkpPrismaticConstraintData`, `hkpWheelConstraintData`, `hkpStiffSpringConstraintData`,
  `hkpBreakableConstraintData`, `hkpGenericConstraintData`, plus motors and chains. The game exposes a matching
  `CONSTRAINT_TYPE_*` enum (BALLANDSOCKET, HINGE, LIMITEDHINGE, POWEREDHINGE, RAGDOLL, POWEREDRAGDOLL,
  STIFFSPRING, PRISMATIC, WHEEL, PULLEY, GENERIC, BREAKABLE, MALLEABLE, CONTACT, POINTTOPATH, POINTTOPLANE).
  **OPEN:** the constraint-data field layouts are not decoded (no HK550 physics classes in the class registry;
  only `hka*` animation + `hkpMoppCode`/`WpMeshShape16` are registered).

### 1.6 How collision binds to world objects / models — the binding chain (CONFIRMED, code-backed)
This is the single most important reimplementation fact. From `mercs2_formats/src/orchestrator.rs`
(`grounded_hulls`, `classify`, `parse_segm`) + `destruction_extract` + [`docs/heli_rig_dissection.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/heli_rig_dissection.md):

```
model container (a UCFX block)
├── HIER  : node tree (parent indices)          → world_matrices(hier) = per-node local→model transform
├── SEGM  : {u16 bone (LE), u8 seg_id, u8 state_mask}  → attachment bone + LOD-TIER MASK  [★CORRECTED]
├── PHY2  : embedded Havok packfile              → the convex hulls (in *node-local* space)
├── INDX  : sub-object → seg_id (into SEGM)      → node = SEGM[INDX[k]].bone   [★CORRECTED]
└── SWIT  : flat list of node hashes             → the destruction switch set (intact ⇆ break_piece)
```
- **Hull → node:** ⚠ *this hull-binding rule was derived from the old SEGM reading and has NOT been
  re-validated against the corrected layout — treat it as unconfirmed.* SEGM lists the collision nodes; sorted **descending**, `collision[i]` binds to
  PHY2 `hull[i]`. Each hull's verts are then transformed by that node's HIER **world matrix** →
  model-space collision (CONFIRMED on the crate: SEGM names nodes `{2,4,5,6,7,8}`, 6 hulls).
- **Everything is addressed by HIER node name-hash (Pandemic hash)** — vehicles wire blades/gear/weapons/
  hardpoints by node hash, not by geometry embedded in the spawn definition. A vehicle = model (HIER+meshes)
  + animgroup (Havok skeleton + clips) + ECS component set + a generic Lua class.
- **Destruction is a state switch, not a fracture solve at runtime:** the model *ships* both the intact
  mesh and its wreck; the engine switches which subtree is shown/collidable. The convex hulls in PHY2 are
  the break-piece colliders.

  > ★**CORRECTED 2026-07-12.** The old **intact / break_piece / static** classification was a
  > *heuristic* inferred from `SWIT` sibling structure, and it predates the recovery of the engine's
  > actual state machine. Destruction is driven by the per-model `NODE`/`STAT`/`CHDR`/`CEXE` machine:
  > named states (`PristineState 0xACB51200` → `DamagedState` → `StartDestroyedState` →
  > `DestroyedState 0x7687DF41`) whose enter-scripts `SHOW`/`HIDE` **whole HIER subtrees**.
  > `PristineState` `SHOW`s the intact body (`0x255EAB53`); `DestroyedState` `SHOW`s the wreck body
  > (`0x75F1F74D`), which IS geometry in the container. "static" nodes are not a category — nodes the
  > machine never names are still hidden as *children* of a governed parent.
  > See [`vehicle_model_spec.md`](vehicle_model_spec.md) §5 and
  > [`../destruction_orchestrator_format.md`](../destruction_orchestrator_format.md).

---

## 2. Runtime physics role (what Havok actually does in-game)

Sourced from `docs/mercs2-pdb-analysis/{havok-physics,physics-game,vehicles}.md`, ECS family 03, and the
`PgPhysicsActor*` bridge. Pangea wraps Havok behind `oPgHavokManager` / `PgHavokManager::Update` and a
polymorphic per-entity `PgPhysicsActor*` family (one stream-loaded ECS component family — CONFIRMED via the
shared `CopyFromStream` registrar).

| Role | Mechanism | Confidence |
|---|---|---|
| **Static world collision** | `hkpMoppBvTreeShape` (MOPP BV-tree) + `WpMeshShape16` (Pandemic mesh) for terrain/buildings/roads; `PgPhysicsActorTerrain(Mesh)/Building/Road/LowResTerrain`. | CONFIRMED |
| **Dynamic rigid bodies** | `hkpRigidBody` + motions; props/debris (`PgPhysicsActorProp/Debris`, ECS `RtDebris`). | CONFIRMED |
| **Character locomotion** | `hkpCharacterProxy` + `hkpCharacterState{InAir,Jumping,OnGround}`; game side `HumanPhysics`, `HumanIntegrateJob`, `HumanLinearCastJob` (a character-controller, swept-capsule model — NOT a full ragdoll while alive). | CONFIRMED |
| **Ragdoll (death/impact)** | `hkaRagdollInstance` + `hkpRagdollConstraintData`/`RagdollLimitsData`; `PgPhysicsActorRagdoll` + `RagdollController`; ragdoll↔animation skeleton mapper, bone-layer/frame fields. | CONFIRMED |
| **Vehicles** | **Havok Vehicle SDK is fully linked**: `hkpVehicleInstance`/`Data`, DefaultEngine/Transmission/Brake/Steering/Suspension/Aerodynamics, `hkpVehicleRaycastWheelCollide` (raycast wheels), friction descriptor, tyremarks. Game side: `PgPhysicsActorCar/Tank/Helicopter/Jet/Boat` + ECS `ControllerCar/Tank/Boat/Helicopter/Vehicle/LW/Ladder`. **Handling tuning IS in named reflection fields** (`vehicles.md`): `EngineTorque`, `GearRatio1–5`, `SteerMaxAngle`, `SpringStrength`/`SpringDamp`, `WheelRadius`, bike `BikeRoll*`, boat `WaterDrag*`/`HullFriction`, heli `MaxFwdSpeed`/`PitchAccel`/`RollAccel`. Heli rotor spin = `BoneCtrlLocalRotation` fed by `ControllerHelicopter` (no anim clip). | CONFIRMED (SDK + named tuning fields); **OPEN** (per-instance numeric defaults are stream-loaded, not baked constants) |
| **Destruction** | Ship-both-states + `SWIT` switch; break-piece convex hulls in PHY2; `hkpBreakableConstraintData` + `hkpConvexPieceStreamData` for streamed break pieces; ECS `BuildingDestruction`/`DestructionLink`(+`DestructionDelay`/`Radius`), `BuildingCollapseAnim`; prop-material variants `PropRock/BrickPlaster/Brick/Glass/Wood/Metal/Generic/Fragile`; `MassiveComponent`/`Crusher`/`RtDebris`. Not a runtime fracture solver — pieces are authored/streamed. | CONFIRMED |
| **Raycast/shapecast API** | `CastRay`, `CastRayShape{Box,Sphere,Capsule,Cylinder}`, `CastRayGroup`, delayed/queued casts, quota-limited (`Too many CastRay calls %i/%i`). Used by AI LOS, camera collision (5-camera record @ stride 0x620), weapons, floor probes. | CONFIRMED |
| **Constraints (gameplay)** | `ConstraintLink` layer + `Create/BreakAllConstraints`; grappling hook (`PgSysGrapplingHook`), winch/tow (`PgPhysicsActorWinch`, `AttachCargoToWinch`), anchors/tethers. | CONFIRMED |
| **Phantoms (triggers)** | `hkpAabbPhantom`/`hkpShapePhantom` overlap volumes → gameplay triggers, bounding phantoms. | CONFIRMED |
| **Fluid "droplets"** | `PgPhysicsSystemDroplet` linked-particle/blob system (fuel/liquid). | **OPEN** (only debug-counter strings; no decoded body) |
| **Threading** | `hkpMultiThreadedSimulation` + PIMP job system (`HavokIntegrateJob`, `HumanIntegrateJob`, `_CastRayWorkerMT`); simulation-islands with size alerts. | CONFIRMED |

**OPEN — the per-step integrator call graph.** Neither the Xbox nor PC decomp exposes a named, fully-decoded
`hkpWorld::stepWorld`/`integrate`. The actual VMX128/SSE integrator math does not decode. There is therefore
**no numeric oracle for the integrator internals** — only for its *observable outputs* (positions/velocities
at frame boundaries). This directly shapes the recommendation.

---

## 3. Recommendation: **(a) Adopt a modern Rust physics engine (rapier), driven by the game's collision data**

**Do NOT reimplement Havok 5.5 semantics.** Parse the collision geometry + body/constraint/vehicle
descriptors out of the WADs and feed them to `rapier3d` (already a Rust crate; aligns with D2 "Rust engine").

### Rationale
1. **Charter alignment (D1).** Reimplementing Havok's solver is the same trap as auto-lifting the binary:
   a faithful HK550 solver port is a multi-year effort producing an opaque, unmaintainable numeric clone.
   The charter explicitly says implementation is free; only behavior is gated.
2. **There is no clean numeric oracle to reimplement *against*.** The integrator math is VMX/SSE and
   undecoded in both builds (§2). We can't diff a reimplemented solver step-for-step against the original —
   we can only diff *observable behavior*. That removes the main advantage a faithful reimplementation would
   have had, and it is exactly the surface a modern engine can be gated on.
3. **The hard, game-specific data is already extractable.** The binding chain (PHY2 hulls → SEGM → HIER →
   model space), the convex-hull decode, MOPP/mesh shapes, and the ECS controller/property component set are
   understood or nearly so. Feeding convex hulls + trimesh colliders to rapier is a solved shape-mapping
   problem; rapier natively supports convex hulls, trimesh, compound, capsule (character), and has a
   built-in character controller and raycast/shapecast API that maps 1:1 onto `hkpCharacterProxy` and the
   `CastRay*` surface.
4. **Effort is bounded and front-loaded on extraction, which we must do anyway.** Even a Havok reimpl would
   need the same PHY2/SEGM/HIER extraction. Adopting rapier means the *only* net-new work vs. "just parse the
   data" is the tuning/parity loop, not writing a rigid-body solver, MOPP VM, TOI/CCD, constraint solver,
   and a vehicle SDK from scratch.
5. **Vehicles: NOT a Havok-vehicle problem at all** (CORRECTED 2026-07-06). Mercenaries 2 does **not**
   use the Havok vehicle SDK — there are **zero `hkpVehicle*`** on PC; the shipped drive model is
   **Pandemic's own custom raycast sim** (nine `hkpUnaryAction`-derived actors: `CarPhysicsV2` etc.),
   **now fully decoded** — per-axle raycast friction + linear torque falloff to MaxSpeed + donut LUT
   ([`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md) §4). So the reimpl mirrors **that decoded algorithm**,
   not `hkpVehicleRaycastWheelCollide`, which lowers the risk (we have the real math, not a black box).
   The tuning defaults are still data-driven (§2 OPEN). Gate on behavioral milestones for parity.

### Why NOT reimplement Havok (weighing parity vs effort)
- **Parity gain is illusory without an oracle.** A hand-written HK550 solver would *look* faithful but
  still couldn't be proven equal step-for-step (undecoded integrator), and would drift from real Havok
  anyway (we don't have Havok source). So it buys the same "behavior within tolerance" bar as rapier — at
  10–50× the effort and with a worse maintainability story (D1).
- **Determinism is the one real risk with either choice.** The charter's gold-standard gate is
  record/replay with identical RNG seeding. Physics determinism (cross-platform, frame-rate) must be
  designed in from day one regardless of engine. rapier is deterministic *within a build/platform* given
  fixed timestep + seed; exact cross-engine determinism vs the original is **not achievable** and is not
  required by the charter (behavior-within-tolerance, not bit-equality).

### Where "faithful" is actually gated (the tolerance boundaries)
- **Static collision & raycasts (Surface A-adjacent):** hull/mesh vertices decoded from PHY2 must match the
  extractor's exact output (already byte-verified). Raycast hit points/normals against static world within
  a small epsilon of the original (capture original hits via x32dbg on `CastRay`).
- **Character locomotion:** same reachable/blocked cells, same step-up/slope limits — behavioral gate, not
  numeric. Capture original via the binding tracer (Surface B) + loadprobe-style milestones.
- **Ragdoll & destruction:** perceptual/statistical (does the body settle in a plausible pose; do the same
  break pieces detach on the same hit) — render-golden with tolerance, not per-body numbers.
- **Vehicles:** behavioral milestones (traversal, top speed band, roll-over thresholds), not wheel forces.

---

## 4. Phased plan

Fits the charter's Phase 4 ("gameplay → AI → **physics** → audio → net"), but the *extraction* half can and
should land earlier (Phase 1–2) because it is pure asset→struct (Surface A) work.

- **P0 — Extraction hardening (do now; pure Surface-A, no physics engine yet).**
  1. Add HK550 **physics** class layouts to the converter registry so PHY2 `__data__` is class-aware (today
     it degenerates to a u32 sweep — memory OPEN). Decode `hkpRigidBody` mass/inertia/friction/restitution,
     `hkpMoppCode` mesh, `WpMeshShape16`, and the constraint-data field layouts. Gate: byte-parity vs retail.
  2. Extend `havok_extract`/`destruction_extract` to emit a **complete per-model physics manifest**:
     grounded hulls + MOPP/mesh colliders + body descriptors + constraint graph + SEGM/HIER binding, keyed
     by model hash. This is the engine's physics-asset input format.
  3. Capture oracle data via x32dbg: `CastRay` hit points/normals, `hkpWorld` body transforms at known
     frames, ragdoll settle poses — the golden set for P2–P4 gates.

- **P1 — Static world in rapier.** Load one level's MOPP/mesh + convex colliders, place by HIER world
  matrices, run rapier raycasts. Gate: raycast hits within epsilon of the captured `CastRay` oracle;
  render-golden of a collision-debug overlay.

- **P2 — Dynamic bodies + character controller.** Map `hkpRigidBody` descriptors → rapier rigid bodies;
  `hkpCharacterProxy` → rapier `KinematicCharacterController` (swept capsule). Gate: same reachable/blocked
  navigation, same step/slope limits (behavioral milestones).

- **P3 — Ragdoll + destruction.** Build rapier articulations from the ragdoll constraint graph
  (ball-socket/hinge/ragdoll limits → rapier joints); wire `SWIT` destruction state switching to swap
  colliders between intact/break-piece. Gate: perceptual/statistical parity (render-golden with tolerance).

- **P4 — Vehicles.** Port the game's **decoded custom raycast sim** ([`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md)
  §4 — `CarPhysicsV2`/`TankPhysics`/`BoatPhysics`/heli as `hkpUnaryAction`-style per-frame actors),
  NOT the Havok vehicle SDK (which the game doesn't use). Wheels = per-axle raycasts against the rapier
  world; suspension/friction/torque per the decoded algorithm. Gate: behavioral milestones (traversal,
  top-speed band, roll thresholds). Wire grappling hook / winch / anchors as rapier joints last.

- **Throughout:** fixed timestep + seeded RNG from day one (charter record/replay gate). Physics runs on the
  PIMP-equivalent job path; simulation-island size alerts preserved as diagnostics.

---

## 5. OPEN questions (surfaced for manual review — not guessed)

1. **Rigid-body / constraint field layouts in the packfile.** `hkpRigidBody` mass/inertia/friction/
   restitution and the `hkp*ConstraintData` layouts are **not decoded** (no HK550 physics classes in the
   converter registry; ECS `PhysicsActor` is only a 4-byte handle). Needed before P2/P3. Method: disassemble
   the `PTR_CopyFromStream_*` targets and/or add HK550 physics layouts (HavokLib sibling tool referenced in
   memory is a candidate source of these layouts — **verify against the on-disk packfile, don't trust blind**).
2. **The per-step integrator call graph.** Undecoded (VMX/SSE) in both builds — so **no numeric oracle for
   the solver internals exists.** Confirms "gate on observable outputs, not solver math," but flag it: if a
   future need for step-level parity arises, it requires live x32dbg capture, not static decomp.
3. **Vehicle handling numeric values.** The tuning **field set is known** — named reflection fields in
   [`docs/mercs2-pdb-analysis/vehicles.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/vehicles.md) (`EngineTorque`, `GearRatio1–5`, `SpringStrength`, `WheelRadius`,
   boat `WaterDrag*`, heli `MaxFwdSpeed`/`PitchAccel`, etc.). What is **OPEN** is the *numeric defaults per
   vehicle*: they are stream-loaded per-instance from vehicle-definition assets, not baked constants. Must be
   located/dumped before P4 (candidate: a vehicle-definition WAD reflection blob; the per-class stream
   sizes are known — [`../reverse_engineer/vehicle_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/vehicle_code_map.md) §2). **Note (2026-07-06):** `hkpVehicleData`
   is **dead SDK payload** on this title (the drive model is custom, not Havok), so it is not the source.
   These named fields map onto the decoded custom sim's actor fields, not a Havok controller.
4. **`PgPhysicsSystemDroplet` fluid model.** Single-source (debug counters only); the linked-particle reading
   is inference. Decide whether fuel/liquid physics needs faithful replication or can be a cosmetic modern
   particle system.
5. **Determinism policy vs the original.** Cross-engine bit-determinism vs Havok is **not achievable** and
   (per charter) not required — but the *record/replay* gate needs a defined tolerance band per system.
   Needs an explicit policy decision.
6. **MOPP vs re-tessellation.** MOPP is a Havok-internal BV-tree; rapier uses its own trimesh/BVH. Decode the
   **source triangles** MOPP was built from (or the `WpMeshShape16` mesh) and let rapier rebuild its own
   acceleration structure — do **not** port the MOPP VM. Confirm the source triangles are recoverable
   (WpMeshShape16 is; standalone MOPP-only shapes need their referenced child shape — verify).
