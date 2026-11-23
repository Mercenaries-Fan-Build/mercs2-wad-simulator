# ECS family 03 — Controllers / Physics

Reverse-engineered from the Ghidra decompilation of the game EXE (Ghidra decomp of the
retail PC `Mercenaries2.exe`). Hashes from `tools/pandemic_hash.py --m2`.

## Key architectural finding (read first)

Unlike the data-driven reflection classes (e.g. `WeaponProjectileBase` = `FUN_0065ca70`,
which calls `FUN_00656210/00656320/00656720` to register named int/float/enum fields), **every
class in the controllers/physics family deserializes an opaque, fixed-size raw block** and
registers **no named reflected fields**.

The deserialize template for each class is uniformly:

```c
undefined4 FUN_xxxx(undefined4 param_1, int *param_2) {
  int iVar1;
  undefined1 local[STRIDE];
  (**(code **)(*param_2 + 0x14))(local, STRIDE, 0);   // read STRIDE bytes from stream
  iVar1 = DAT_<counter>;
  FUN_0064a600(param_1, &stack...);                   // finalize / hash-seed reflection record
  if ((iVar1 != DAT_<counter>) && (++_DAT_<counter+0x2c>, DAT_00cfb58a != '\0'))
    FUN_00665590(param_1, (int)DAT_<name>);           // emit name on schema change
  return 1;
}
```

Verified by grep: across the entire controller/physics deserializer region (decomp lines
~289100–293400 and ~297600–300900) there are **0** calls to `FUN_00656210` (int field),
`FUN_00656320` (float field), `FUN_00656720` (enum field) or `FUN_00656890`. So there are
**no per-field defaults (no named mass/force/gravity floats) baked into the reflection schema
for these classes** — the `STRIDE`-byte payload is an opaque runtime struct whose meaning is
defined by C++ `CopyFromStream` code, and the actual tuning (masses, forces, damping) is
supplied per-instance from the level/vehicle data assets, not as reflection defaults. This is
the expected shape for **runtime ("Rt…") / controller components**: lightweight per-entity
state holders, not data templates.

### Descriptor layout (50-byte block, per `FUN_0063f390` WeaponProjectileBase oracle)

Each class has a builder `FUN_006xxxxx` (single caller in the init table `0x00a7aXXX`) that
fills a 0x50-byte descriptor. Relative to the **name-string slot** (= manifest col2, the
`s_<Class>` address):

| offset from name slot | field | meaning |
|---|---|---|
| `-0x3c` | dword0 | `&PTR_CopyFromStream_xxxx` (deserialize fn ptr) |
| `-0x38` | u16/u16 | `0xffff,0xffff` (sentinel handles) |
| `-0x34` | `0x100` | category const |
| `-0x30` | `+8` | **runtime component size / alloc footprint** (col "rt-size") |
| `-0x2c` | `3` | type tag |
| `-0x20` | counter | reflection field-count accumulator (`DAT_<counter>`) |
| `-0x18` | **STRIDE** | **serialized block size** (col "stride") |
| `-0x16` | `8` | element align |
| `-0x14` | `0x100` | |
| `-0x10` | `0x9e3779b9` | golden-ratio hash seed |
| `+0x00` | name ptr | `s_<Class>` string (manifest col2) |

`FUN_0064a770` (called at end of every builder) registers the descriptor; `FUN_0064a600`
(in every deserialize template) finalizes the per-instance reflection record.

## Registry (all 31 classes)

`stride` = serialized block size (bytes the stream reader consumes). `rt-size` = runtime
component alloc footprint (descriptor +8 field). `purpose` = inferred role.

| Class | m2 hash | name str (col2) | descriptor base | stride | rt-size | deserialize fn | purpose |
|---|---|---|---|---|---|---|---|
| Anchor | 0xfa55f6ba | 0x00bb4a08 | 0x017bd268 | 0x10 | 0 | FUN_0063ab40 | Pins a body to a fixed world point (mooring/tether constraint). |
| BoneControllerRuntime | 0x09a0962d | 0x00bc5d40 | 0x017c00f8 | 4 | 8 | (factory) | Per-bone procedural controller runtime state. |
| BoneCtrlPhysicsActor | 0x1afaed2a | 0x00bc4abc | 0x017bbfa8 | 4 | 0 | FUN_00638b30 | Links a skeleton bone to a physics actor (bone-driven ragdoll/IK). |
| Buoyancy | 0xb9659f7b | 0x00bc4c90 | 0x017bc5e8 | 0x14 | 0 | FUN_006395e0 | Water flotation: applies up-force vs. waterline (boats/floating debris). |
| CenterOfMassInWorld | 0xe5276b5c | 0x00bc5cb8 | 0x017bffb8 | 0xc | 8 | (factory) | Stores/overrides the body COM as a world vec3 (12B = 3 floats). |
| ControllerBoat | 0x4f89a7c7 | 0x00bc4f28 | 0x017bcfe8 | 4 | 0 | FUN_0063a6c0 | Boat drive controller (steering/throttle → hull physics). |
| ControllerCar | 0xeeea744d | 0x00bc4f18 | 0x017bcf98 | 4 | 0 | FUN_0063a650 | Car/ground-vehicle drive controller. |
| ControllerHelicopter | 0x495a0cea | 0x00bc4fa0 | 0x017bd0d8 | 4 | 0 | FUN_0063a910 | Helicopter flight controller. |
| ControllerLW | 0x1bb0a5be | 0x00bc4f48 | 0x017bd088 | 4 | 0 | FUN_0063a7a0 | "Light Weapon"/turret-style controller (mounted weapon aim). |
| ControllerLadder | 0x964e010d | 0x00bc4fb8 | 0x017bd128 | 4 | 0 | FUN_0063a980 | Ladder-climb locomotion controller. |
| ControllerPlayer | 0x6ca511b2 | 0x00bc4ef0 | 0x017bcef8 | 0xc | 0 | FUN_0063a570 | Player character locomotion controller (12B = input/move state). |
| ControllerTank | 0x55bc62bd | 0x00bc4f38 | 0x017bd038 | 4 | 0 | FUN_0063a730 | Tank drive controller (tracks + turret). |
| ControllerVehicle | 0xbfb1aecb | 0x00bc4f04 | 0x017bcf48 | 4 | 0 | FUN_0063a5e0 | Generic vehicle controller base. |
| ControllerVelocity | 0xd61c71b4 | 0x00bc4d98 | 0x017bcb38 | 0x18 | 0 | FUN_00639de0 | Velocity-driven controller (24B = lin+ang velocity targets). |
| Crusher | 0x24463d8b | 0x00bc5628 | 0x017be988 | 4 | 0 | FUN_0063d3c0 | Marks an entity that crushes others on contact (instant-kill/squash). |
| MassiveComponent | 0xf482c286 | 0x00bc5614 | 0x017be938 | 4 | 0 | FUN_0063d350 | Flags a "massive" object (special collision/destruction handling). |
| PhysicsActor | 0xfe9497db | 0x00bc5b10 | 0x017bf888 | 4 | 8 | FUN_0066d370 (factory) | Core rigid-body physics actor (Havok body wrapper). |
| PhysicsActorRagdoll | 0xf365e0ec | 0x00bc5b34 | 0x017bf928 | 4 | 8 | (factory) | Ragdoll variant of PhysicsActor. |
| PhysicsActorWinch | 0x025b7ab6 | 0x00bc5b20 | 0x017bf8d8 | 4 | 8 | (factory) | Winch/cable physics actor variant. |
| PhysicsDefaultActivator | 0x2e2659f0 | 0x00bc4c3c | 0x017bc4f8 | 1 | 0 | FUN_00639350 | 1-byte flag: wake/activate physics by default. |
| PhysicsPropertyFakeContinuous | 0x639f9491 | 0x00bc4b00 | 0x017bc048 | 4 | 0 | FUN_00638c50 | Property: fake continuous-collision (anti-tunnelling hint). |
| PhysicsPropertyGravityScaler | 0x841ba027 | 0x00bc4b3c | 0x017bc0e8 | 4 | 0 | FUN_00638d30 | Property: per-body gravity multiplier (4B = 1 float scaler). |
| PhysicsPropertyUncrushable | 0xa61bd97b | 0x00bc4b20 | 0x017bc098 | 4 | 0 | FUN_00638cc0 | Property: immune to Crusher / crush damage. |
| RTHuman | 0x2c6e46b6 | 0x00bc5b54 | 0x017bf9c8 | 0x48 | 8 | (factory) | Runtime human/pedestrian state block (72B — largest payload). |
| RagdollController | 0x34ea185e | 0x00bc5eb0 | 0x017c0508 | 4 | 8 | (factory) | Drives ragdoll blend/activation. |
| RtDebris | 0x964bebaa | 0x00bc5b48 | 0x017bf978 | 0x1c | 0x68 | FUN_0063da60 | Runtime debris chunk (28B stream; 0x68 runtime struct). |
| RtDriverData | 0xe2636501 | 0x00bc5b8c | 0x017bfab8 | 0x10 | 0x18 | (factory) | Runtime per-driver (vehicle occupant) data. |
| RtJunction | 0x643b62af | 0x00bc5b9c | 0x017bfb08 | 0x10 | 8 | (factory) | Runtime traffic/road-junction node state. |
| RuntimeMassiveSubscriber | 0x4172e975 | 0x00bc5af4 | 0x017bf838 | 4 | 8 | (factory) | Subscribes an entity to MassiveComponent events. |
| Sticky | 0x97870d10 | 0x00bc54b0 | 0x017be5c8 | 4 | 0 | FUN_0063cbe0 | Marks objects that "stick" on contact (e.g. sticky explosives/attach). |
| Winch | 0x9c6b3368 | 0x00baa5f4 | 0x017bc548 | 0x2c | 0 | FUN_006393c0 | Winch/tow-cable component (44B — cable config: length/force/limits). |

Notes:
- "(factory)" deserialize fns (PhysicsActor `FUN_0066d370`, etc.) allocate the runtime C++
  object (set vtable, e.g. `PTR_FUN_00ba99b8`) and then call `FUN_0064a600`; they likewise
  register no named reflection fields. The stride field still records the serialized size.
- `descriptor base = name_str - 0x3c`; `stride field @ name_str - 0x18`; `counter @
  name_str - 0x20`; `rt-size @ name_str - 0x34`.

## Per-component notes (significant classes)

Because none of these classes expose a reflected field schema, there are **no named tuning
defaults to recover from the descriptor** (e.g. no `mass = 1500.0` constant in the binary's
reflection tables). What can be stated authoritatively is the **serialized payload size** and
the **role**, below. Where a size maps cleanly onto known data it is called out; otherwise the
field interpretation is flagged UNKNOWN (lives in C++ `CopyFromStream`, not reflection).

### Vehicle controllers (ControllerCar / Tank / Boat / Helicopter / Vehicle / LW / Ladder)
- All read a **4-byte** serialized block (`stride = 4`), rt-size 0. The 4 bytes are almost
  certainly a single handle/index or packed flag word, NOT physics tunables. `FUN_0063a650`
  (Car), `FUN_0063a730` (Tank), `FUN_0063a6c0` (Boat), `FUN_0063a910` (Helicopter),
  `FUN_0063a5e0` (Vehicle base), `FUN_0063a7a0` (LW), `FUN_0063a980` (Ladder) are byte-for-byte
  identical except for the descriptor counter/name they touch.
- **Implication for tuning:** vehicle handling (engine force, mass, grip, suspension, top
  speed) is **not** stored on these controller components. It must come from a separate
  vehicle-definition asset/component (look in family covering vehicle *stats*/*handling*, or a
  Havok vehicle setup) — these controllers only bind the entity to the drive system. Flag:
  UNKNOWN which component carries the numeric handling defaults; not in this family.

### ControllerPlayer (`FUN_0063a570`, stride 0xc = 12B)
- 12-byte payload = plausibly a vec3 (move-intent / velocity) or 3 packed dwords of input
  state. No named fields. UNKNOWN exact layout.

### ControllerVelocity (`FUN_00639de0`, stride 0x18 = 24B)
- 24 bytes = likely two vec3 (linear + angular velocity targets) the controller drives the
  body toward. UNKNOWN exact layout.

### PhysicsActor / PhysicsActorRagdoll / PhysicsActorWinch (factory fns, stride 4, rt-size 8)
- The serialized form is only 4 bytes (a handle/type selector); the real rigid body is a Havok
  object created by the factory (`FUN_0066d370` sets vtable `PTR_FUN_00ba99b8`). Mass, inertia,
  friction, restitution are Havok-side, sourced from the collision/physics asset (PHY2 Havok
  packfile — see project note `phy2-havok-chunk-not-u32`), **not** from this component's
  reflection. Flag: physics material/mass defaults are in the Havok packfile, out of scope here.

### PhysicsPropertyGravityScaler (`FUN_00638d30`, stride 4)
- 4-byte payload on a "GravityScaler" property ⇒ almost certainly a **single float gravity
  multiplier** applied to this body. Default value is supplied per-instance from the stream
  (no baked reflection default). This is the one component in the family with an obvious scalar
  physics tunable (gravity scale), but its default is data-driven, not a binary constant.

### PhysicsPropertyUncrushable / Crusher / PhysicsPropertyFakeContinuous / Sticky / MassiveComponent
- Tag/flag components (stride 1–4). `Crusher` (squash-kill on contact) and
  `PhysicsPropertyUncrushable` (immunity to it) are a matched pair. `MassiveComponent` +
  `RuntimeMassiveSubscriber` are a producer/subscriber pair for "massive object" events.
  `PhysicsDefaultActivator` (stride **1**) is a single bool: wake physics on spawn.

### Buoyancy (`FUN_006395e0`, stride 0x14 = 20B)
- 20-byte block = buoyancy config (e.g. waterline offset + up-force coefficient + damping;
  ~5 floats). Drives flotation for boats/floating debris. No named fields → defaults are
  per-instance. Field breakdown UNKNOWN (C++ `CopyFromStream_xxxx`).

### Winch (`FUN_006393c0`, stride 0x2c = 44B) / Anchor (stride 0x10) / RtDriverData / RtJunction
- `Winch` 44B = the richest opaque payload here: plausibly cable length, max force/tension,
  spring/damping, attach offsets (≈11 dwords). Paired with `PhysicsActorWinch`. UNKNOWN layout.
- `Anchor` 16B = a world anchor point (vec3 + flag/handle) — tether/mooring constraint.
- `RtDriverData`/`RtJunction` are runtime traffic/AI-driver state (vehicle occupant, road node).

### RtDebris (`FUN_0063da60`, stride 0x1c = 28B, rt-size 0x68) / RTHuman (stride 0x48 = 72B)
- `RtDebris`: 28-byte stream record but a 0x68 (104-byte) runtime struct — runtime physics
  debris chunk spawned on destruction. `RTHuman`: 72-byte block, the largest serialized payload
  in the family — runtime pedestrian/human state. Both opaque; layouts UNKNOWN (no reflection).

## Caveats / unknowns
- No numeric physics defaults (mass, force, gravity, damping, limits) are recoverable **from
  the reflection descriptors** for any class in this family — confirmed by the absence of
  `FUN_00656210/320/720` calls. The opaque stride blocks are decoded by C++ `CopyFromStream`
  routines (the `PTR_CopyFromStream_xxxx` targets) and filled from level/vehicle data assets.
- Where field meaning is given above (e.g. "44B Winch = length/force/damping") it is **inferred
  from size + class name**, not proven from a field-by-field schema, and is flagged UNKNOWN.
- To recover the actual byte layouts, the next step is to disassemble each
  `PTR_CopyFromStream_xxxx` target (the function pointer stored at descriptor +0) and read how
  it parses the STRIDE-byte block — that is where any real per-field semantics live.

Cited decomp lines (the Ghidra decompilation of the game EXE):
builders ~293243 (PhysicsActor `FUN_0063d910`), ~293307 (RtDebris `FUN_0063da60`), 294439
(WeaponProjectileBase oracle `FUN_0063f390`), 295296–295386 (Controller Car/Boat/Tank/LW),
295180–295255 (ControllerPlayer/Vehicle); deserialize templates 289157 (BoneCtrlPhysicsActor),
289217/289238/289259 (PhysicsProperty Fake/Uncrushable/GravityScaler), 289350
(PhysicsDefaultActivator), 289571 (Winch), 289711 (Buoyancy), 290098 (ControllerVelocity),
290524–290755 (Controller Player/Vehicle/Car/Boat/Tank/LW/Heli/Ladder), 290829 (Anchor), 292545
(Sticky), 292989/293010 (MassiveComponent/Crusher), 319457 (PhysicsActor factory `FUN_0066d370`).
