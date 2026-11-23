# ECS Family 06 — World / Terrain / Roads / Streaming

Native ECS/reflection component classes for Mercenaries 2: World in Flames (PC, x86, the game EXE (uncracked v1.1), imagebase 0x400000). This family covers terrain, the road/lane graph, streaming layers/LOD, regions, scene placement, and traffic flow.

Source of truth:
- Builders/decomp: the Ghidra decompilation of the game EXE
- Descriptor structs resolved from the live builders (the `.data` descriptors are zero in the file image and are populated at load by per-class builder functions in the `0x0064xxxx` cluster, called from the global-ctor table at `0x00a7bxxx`).
- Hashes: `python tools/pandemic_hash.py --m2 "<ClassName>"`

> Cross-ref: `Model` m2 hash = **0x5b724250** — the same value the MEMORY note `chdr-dual-layout-mesh-vs-placement` calls the MESH `Model` CHDR class. The 4-byte **Model component** here is just a handle that references that mesh block class.

---

## Architecture (verified)

Each class owns a ~0x70-byte **pool descriptor** written by a builder (`FUN_0064xxxx`). Descriptor layout (offsets relative to the name pointer at `name_addr`):

| field | builder line | meaning |
|---|---|---|
| name_addr-0x6c (base) | `DAT_<base> = 0xffff; +2 = 0xffff` | free-list sentinels / pool head |
| base + … `= &PTR_CopyFromStream_xxxx` | CopyFromStream vtable | **deserialize** (stream → element); all resolve to generic dispatch `0x644ec0` + `0x640d00` driven by the field table |
| name_addr-0x18 | `DAT_<n-0x18> = <stride>` | **elemSize / record stride** (u16) |
| (0x100) | | initial pool capacity = 256 |
| (0x9e3779b9) | | golden-ratio multiplicative hash seed (keyed by instance hash) |
| base+… `= &PTR_FUN_00bc5ff8` | shared pool-class vtable |
| name_addr | `_DAT_<name> = s_<Class>_xxxx` (set last) | class name string |

**Field schema** (for the classes that have one) is emitted by a separate *schema-template* function (callers=`[]`, registered as data) that calls the field-builders **in stream order** then finalizes via `FUN_0064a600(inst, &recordbuf)`:

| builder | field type |
|---|---|
| `FUN_00656210(default)` | INT-family (i8/i16/i32 per descriptor field-type; arg = default) |
| `FUN_00656320(defaultBits)` | FLOAT (arg = hex float bits; `0x3f800000`=1.0) |
| `FUN_00656610(x)` | VECTOR3 (3 dwords) |
| `FUN_00656720(s_XxxEnum, default)` | ENUM (first arg = enum table) |
| `FUN_00656890(default)` | BOOL/byte |
| `FUN_00656440()` | STRING/hash |

Two finalize primitives exist and were sometimes conflated: `FUN_0064a600` = component-pool insert (`memcpy(dst,rec,elemSize)` into a 0x9e3779b9-hashed pool); `FUN_00649180(&PTR_PTR_00df8xxx,…)` = reflection/property-table register. Editor-reflected classes (e.g. the LaneFakeHP serializer `FUN_006601d0`) use the latter; the thin runtime pools use the former. Field-builder ordering is identical in both.

> **Converter implication:** Family-A classes below (those with a `FUN_00656xxx` schema template) carry a serialized field layout subject to Xbox→PC byte/field-order conversion. The **Rt\*** classes are **engine-generated at world-load** (computed by generator functions, not stream-parsed into named fields) → no field-order parity concern for them.

---

## Registry — all components

| Class | m2 hash | descriptor base | name@ | stride (elemSize) | CopyFromStream vtable | builder | purpose |
|---|---|---|---|---|---|---|---|
| BoundaryData | 0x5a59763f | 0x017bdc88 | 0x017bdcf4 | 0x04 | 0xbc05f0 | FUN_00642380 | region: 1 bool-enum flag |
| CircleRegion | 0x6691b221 | 0x017bdbe8 | 0x017bdc54 | 0x04 | 0xbc0550 | FUN_00642220 | region: radius float |
| IntersectionToIntersection | 0xeb6de962 | 0x017bd918 | 0x017bd984 | 0x08 | 0xbc0190 | FUN_00641c00 | road graph: int pair (intxn link) |
| LaneData | 0x6a08e327 | 0x017bd968 | 0x017bd9d4 | 0x40 | 0xbc01e0 | FUN_00641cb0 | road: 4 lanes × {type,offset,f,f} |
| LaneZeroDirection | 0x7cf73564 | 0x017bd9b8 | 0x017bda24 | 0x04 | 0xbc0230 | FUN_00641d60 | road: lane direction enum |
| LineRegion | 0x6310807f | 0x017bdc38 | 0x017bdca4 | 0x04 | 0xbc05a0 | FUN_006422d0 | region: 1 int |
| LowResTerrainObject | 0x2d8d2435 | 0x017beb38 | 0x017beba4 | 0x08 | 0xbc1ef0 | FUN_00644470 | terrain: 2 ints (far-LOD handle+tier) |
| Model | 0x5b724250 | 0x017bff38 | 0x017bffa4 | 0x04 | 0xbc3a60 | FUN_006473d0 | 4-byte mesh-block (0x5b724250) handle |
| ObjectMaterial | 0xc1f1f72f | 0x017bde68 | 0x017bded4 | 0x04 | 0xbc07d0 | FUN_006427f0 | surface MaterialType enum |
| PathData | 0xaef6f7b4 | 0x017bdcd8 | 0x017bdd44 | 0x04 | 0xbc0640 | FUN_00642430 | 4-byte path/spline reference handle |
| PointLocation | 0x60b7abe0 | 0x017bda58 | 0x017bdac4 | 0x24 | 0xbc02d0 | FUN_00641ec0 | named point (string/hash) |
| RoadIntersection | 0x6fd048f4 | 0x017be9a8 | 0x017bea14 | 0x7c | 0xbc1d60 | FUN_00644100 | road graph intersection (7i+6vec3+6i) |
| RtFlowControl | 0xb6cb89de | 0x017bfd58 | 0x017bfdc4 | 0x5c | 0xbc3790 | FUN_00646e90 | runtime traffic flow-control state |
| RtFlowCycleTimer | 0xd4ca71da | 0x017bfda8 | 0x017bfe14 | 0x44 | 0xbc37e0 | FUN_00646f60 | runtime flow-cycle timer |
| RtGenericLOD | 0x0c51b633 | 0x017c0708 | 0x017c0774 | 0x44 | 0xbc4710 | FUN_00648680 | runtime LOD tier array (gen) |
| RtGenericLODProxy | 0xce91973d | 0x017c0758 | 0x017c07c4 | 0x04 ⚠ | 0xbc4760 | FUN_00648740 | runtime distance-LOD proxy handle |
| RtRoadIntersection | 0x5e137672 | 0x017bfdf8 | 0x017bfe64 | 0xc4 | 0xbc3830 | FUN_00647040 | runtime traffic-signal phase scheduler |
| RtSpeedLimit | 0xff142695 | 0x017c069c | 0x017c06d4 | 0x1c | 0xbc45f8 | FUN_006484f0 | runtime speed-limit (gen) |
| RtTerrainChildren | 0x0ff1c703 | 0x017c06b8 | 0x017c0724 | 0x40 | 0xbc4648 | FUN_006485b0 | runtime terrain child-node array (gen) |
| RtTickDamage | 0x27e19bf7 | 0x017bfbc8 | 0x017bfc34 | 0x10 | 0xbc3588 | FUN_00646aa0 | runtime damage-over-time |
| RuntimeLayerId | 0x2284fe19 | 0x017bf448 | 0x017bf4b4 | 0x04 | 0xbc2c28 | FUN_006458a0 | runtime streaming-layer id |
| SceneObject | 0xb6185886 | 0x017c02a8 | 0x017c0314 | 0x1c | 0xbc47b0 | FUN_00648850 | base placement/transform entity record |
| SpawnerAdjust | 0x1003413e | 0x017c0258 | 0x017c02c4 | 0x60 | 0xbc4028 | FUN_00647bb0 | spawner tuning (consumer FUN_004e2d80) |
| SpeedLimit | 0x9add960b | 0x017be8b8 | 0x017be924 | 0x0c | 0xbc1b08 | FUN_00643eb0 | road: 3 floats (speed) |
| SphereRegion | 0x4ca3fd52 | 0x017bda08 | 0x017bda74 | 0x04 | 0xbc0280 | FUN_00641e10 | region: radius float |
| SysPathIntersectionIndex | 0x2eef9dd2 | 0x017c0398 | 0x017c0404 | 0x04 | 0xbc4248 | FUN_00647e10 | road graph: intxn-id → table idx |
| SysPathRoadIndex | 0x805ad569 | 0x017c0348 | 0x017c03b4 | 0x04 | 0xbc41f8 | FUN_00647d50 | road graph: road-id → table idx |
| TerrainGuidMappingHighResToLowRes | 0x23b3d1e4 | 0x017c02f8 | 0x017c0364 | 0x04 | 0xbc41a8 | FUN_00647c90 | terrain hi-res GUID → low-res index |
| TerrainKey | 0x0868b0cd | 0x017bdf58 | 0x017bdfc4 | 0x04 | 0xbc0b18 | FUN_00642a50 | terrain surface-key enum |
| TerrainObject | 0x6c82ebe5 | 0x017bea48 | 0x017beab4 | 0x04 | 0xbc1e00 | FUN_00644260 | terrain cell/heightfield handle (1 int) |

⚠ `RtGenericLODProxy` stride: builder field (`name-0x18`) = **0x04**; one analysis pass read an alternate init block as 0x68. The builder-resolved value 0x04 (a single proxy handle) is taken as canonical; flagged for re-check.

---

## Per-component schemas

### Regions (BoundaryData / CircleRegion / SphereRegion / LineRegion / PointLocation)
Trivial single-field records (volume/area triggers used by mission and streaming systems to gate logic by world position).

- **BoundaryData** — `FUN_0065fc20`: `[0] ENUM(s_BoolEnum_00bc6084)=0`. One boolean (inside/outside or active flag).
- **CircleRegion** — `FUN_0065fee0`: `[0] FLOAT=0.0` (radius). 2D circle.
- **SphereRegion** — `FUN_0065fe40`: `[0] FLOAT=0.0` (radius). 3D sphere. Same shape as CircleRegion, distinct pool.
- **LineRegion** — `FUN_0065ff80`: `[0] INT=0`.
- **PointLocation** — `FUN_006608a0`: `[0] STRING/hash` (`FUN_00656440`). A named world point. (stride 0x24 ⇒ the record also carries the resolved position alongside the name.)

### Road / lane graph

**LaneData — `FUN_006606b0`** (stride 0x40 = 64B). A repeating 4-field group ×4 lanes:
```
per lane (×4):
  [0] ENUM s_LaneTypeEnum_00bc649c   = 0
  [1] ENUM s_LaneOffsetEnum_00bc64cc = 0
  [2] FLOAT                          = 0.0
  [3] FLOAT                          = 0.0
```
16 fields total. Defines up to 4 driving lanes per road segment (type, lateral offset, two widths/params). Core of the drivable road graph the traffic/AI systems walk.

**LaneZeroDirection — `FUN_00660620`**: `[0] ENUM(s_LaneDirectionEnum_00bc650c)=0`. Travel direction of lane 0 (orients the segment).

**IntersectionToIntersection — `FUN_00660580`**: `[0] INT=0, [1] INT=0`. A directed/undirected edge between two intersection indices (road-graph adjacency).

**RoadIntersection — `FUN_006602c0`** (stride 0x7c = 124B = 31 dwords; consumer `FUN_0063d4b0`):
```
[0..6]   INT   ×7  = 0   (intersection id + connected-road/lane indices/counts)
[7..12]  VEC3  ×6  = 0   (approach/corner world positions — intersection geometry)
[13..18] INT   ×6  = 0   (per-approach link indices)
```
The geometric + topological description of a road intersection. Ties directly to the project's road-graph RE: this is the node; `IntersectionToIntersection` + `SysPath*Index` are the edges/index tables.

**SpeedLimit — `FUN_00664f30`** (stride 0x0c; consumer `FUN_0063d2a0`): `[0..2] FLOAT ×3 = 0.0`. Speed value + 2 params (likely min/normal/max or value+direction). No lane enums.

**SysPathRoadIndex** (4B, builder `FUN_00647d50`, populated in `FUN_004fe750`): road-segment-id → runtime road-table index. Each road edge gets a 0x14-stride runtime record; this pool is the hash index into it.

**SysPathIntersectionIndex** (4B, builder `FUN_00647e10`, populated in `FUN_004fd9f0`/`FUN_004fe750`): intersection-node-id → runtime intersection-table index. Dedups nodes and allocates a 0x20-stride record (node-id + xyz). The intersection counterpart to SysPathRoadIndex; together they form the **runtime road graph** consumed by pathfinding/traffic.

**PathData** (4B, builder `FUN_00642430`): path/spline reference handle, consumed by the SysPath builder (`FUN_004fd9f0`) walking the path-node pool at `&DAT_017bd960`.

### Traffic flow (Rt\*, engine-runtime)

- **RtRoadIntersection** (0xc4, builder `FUN_00647040`): traffic-signal **phase scheduler**. Per-frame tick (`FUN_00676xxx` ~line 324732): `timer += dt`, wraps on cycle period `[+4]`, advances phase `[+8]` through phase-time array `[+0x14..]` bounded by phase-count `[+0xc]`. Red/green cycling per intersection.
- **RtFlowControl** (0x5c, builder `FUN_00646e90`): runtime flow-control state {speed/limit floats, byte flag}. Producer `FUN_00596bd0` (token DAT_017bfd54); enum `s_FlowControlTypeEnum_00bc6734`. Ticked with the traffic loop.
- **RtFlowCycleTimer** (0x44, builder `FUN_00646f60`): periodic flow-cycle timer, ticked in the same per-frame traffic loop.

### Terrain

- **TerrainObject — `FUN_00662460`** (4B; consumer `FUN_0063d590`): `[0] INT=0`. A terrain cell/heightfield handle (or object-table index). `RtTerrainChildren`'s generator stamps a TerrainObject record onto each spawned child node (decomp ~line 319189).
- **LowResTerrainObject — `FUN_006621b0`** (8B; consumer `FUN_0063d6e0`): `[0] INT=0, [1] INT=0`. Far/low-LOD terrain mesh handle + LOD-tier/chunk index. Pairs with the hi-res via the GUID-mapping pool below.
- **TerrainKey — `FUN_00661750`** (4B): `[0] ENUM(s_TerrainKeyEnum_00bc72c4)=0`. Terrain surface key (material/footstep/physics class per cell). NOTE: `s_TerrainKeyEnum` belongs to *this* class, not to `TerrainObject`.
- **TerrainGuidMappingHighResToLowRes** (4B, builder `FUN_00647c90`, built in `FUN_004a88a0`): hi-res terrain-chunk GUID → low-res streaming-proxy index. Builder memsets the index table to -1, then walks the source GUID pool (`&DAT_017beb80`) inserting each mapping (gated by `DAT_01175a3a`). The bridge between hi-res terrain and its streamed low-res representation.

#### RtTerrainChildren — generator `FUN_0066cac0` (stride 0x40, builder `FUN_006485b0`)
**Engine-generated**, not stream-parsed. Loops `count = *(terrain+0x24)` (≤16, `aiStack_48[17]`); per child it spawns a node (`FUN_00673070`), computes a world offset from a stride-0xf (60-byte) sub-record array of tile transforms (`+0x30/+0x34/+0x38`), allocates a 0x240 node, and stamps a TerrainObject record. It finalizes three parent records: the child-handle array (token → RtTerrainChildren, the `int[16]`), an object pointer (token DAT_017bf8a4), and an AABB (6 floats from `puStack_b8[0xb..0xf]`). Payload = up to 16 terrain-child entity handles + parent bounding box. Directly relevant to terrain streaming/spatial culling.

### LOD (Rt\*, engine-runtime)

- **RtGenericLOD — generator `FUN_0066ee80`** (0x44, builder `FUN_00648680`): zero-inits a 0x44 record then iterates a spatial-hash bucket writing per-tier entries `{dist0², dist1², handle, 0}` — LOD switch radii pre-squared for cheap distance compares. Payload = array of LOD tiers.
- **RtGenericLODProxy** (0x04 ⚠, builder `FUN_00648740`): runtime distance-LOD proxy handle. Consumer `FUN_00490220` computes camera-distance² and, per LOD band (stride 0x10: near²/far²/handle), spawns a proxy and pool-inserts when in range, despawns (`FUN_004f30d0`) when out. The 4-byte payload = the spawned proxy entity handle. Distinct from `RtGenericLOD`.
- **RtSpeedLimit — generator `FUN_0066e9b0`** (0x1c, builder `FUN_006484f0`): reads source via `FUN_005857e0`, copies 3 dwords, writes the runtime speed-limit record. The runtime-baked form of the serialized `SpeedLimit` component.

### Scene / placement / misc

- **SceneObject** (0x1c, builder `FUN_00648850`; serializer `FUN_0063b800` reads 0x28, stores 0x1c): the base placement/transform-bearing entity record that Model/PathData/etc. attach to. `FUN@320200` reads a SceneObject then stacks the Model class hash (0x5b724250) beside it to bind geometry.
- **Model** (0x04, builder `FUN_006473d0`): single u32 = asset hash of a MESH-block "Model" CHDR class. Resolved via the global class registry (`FUN_00873f20`, 0x5b724250 lookup ~line 320224) to bind geometry to a SceneObject.
- **ObjectMaterial — `FUN_00661320`** (4B): `[0] ENUM(s_MaterialTypeEnum_00bc69d0)=0`. Per-object surface/material override.
- **RuntimeLayerId** (4B, builder `FUN_006458a0`): runtime id of a streaming layer (objects loaded/unloaded together). Producer `FUN_00535590` registers it gated by a distance-derived fade/alpha; callers `FUN_005402b0/00540370` = the layer streaming/fade system. Ties to the project's streaming-livelock work (layer load/unload sequencing).
- **SpawnerAdjust** (0x60, consumer `FUN_004e2d80`, callers `FUN_005a4c40/005a4d10`): spawner tuning/override record (population/spawn density). Field-named schema not recovered.
- **RtTickDamage** (0x10, builder `FUN_00646aa0`): damage-over-time component, ticked each frame in `FUN_00676xxx` (~line 324939). ~{timer, interval, damage, target/flags}. Included in this family by manifest; not world-streaming per se.

---

## Enum tables referenced

| enum string | used by | meaning |
|---|---|---|
| s_BoolEnum_00bc6084 | BoundaryData, LaneFakeHP flags | false/true |
| s_LaneTypeEnum_00bc649c | LaneData (×4) | lane type |
| s_LaneOffsetEnum_00bc64cc | LaneData (×4) | lateral lane offset |
| s_LaneDirectionEnum_00bc650c | LaneZeroDirection | lane travel direction |
| s_LaneIdentifierEnum_00bc6414 | LaneFakeHP | lane identifier |
| s_LaneFakeHPEnum_00bc6470 | LaneFakeHP | lane "fake HP" state |
| s_MaterialTypeEnum_00bc69d0 | ObjectMaterial | surface/material type |
| s_TerrainKeyEnum_00bc72c4 | TerrainKey | terrain surface key |
| s_FlowControlTypeEnum_00bc6734 | RtFlowControl | traffic flow-control type |
| s_DynamicRoadTypeEnum_00bc675c | (population dynamic-road, adjacent family) | dynamic road type |

---

## Unknowns / flags (not fabricated)

- `RtGenericLODProxy` stride 0x04 vs 0x68 discrepancy (see ⚠ above) — builder field says 0x04; re-verify against init `FUN_00648740`.
- Field-named schemas for **SceneObject (0x1c)**, **SpawnerAdjust (0x60)**, **RtFlowControl/RtFlowCycleTimer/RtRoadIntersection/RtTickDamage** are not recovered as named lists — these are thin/runtime pools whose `CopyFromStream` is a flat `memcpy(elemSize)`; intra-record offsets quoted (esp. RtRoadIntersection phase scheduler) are derived from consumer/tick code, not from a `FUN_00656xxx` field list.
- The `CopyFromStream` method bodies live in `.rdata` vtables (`PTR_CopyFromStream_00bcXXXX`) not expanded in the decomp dump; flat-copy inferred from matching serializer + elemSize.
