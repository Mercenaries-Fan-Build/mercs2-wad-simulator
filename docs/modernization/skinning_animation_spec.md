# Skinning + Animation Spec (native 64-bit Rust/wgpu reimplementation)

**Status:** spec draft for the modernization program (see `00_charter.md`).
**Scope:** (1) per-vertex linear-blend skinning (LBS) — vertex blend-data layout,
bone-palette resolution, the deform math; (2) the animation clip format — where it
lives, how it references bones, runtime sample/apply. Does **not** cover SEGM
static-accessory attachment (separate agent) beyond correcting what SEGM is.

This is a data/behavior spec. It does not touch engine code. Every CONFIRMED claim
is cross-checked against **two independent sources** per the double-blind mandate;
divergent/unverified items are collected in **§5 Open Questions**.

Evidence sources used:
- [`tools/wad_simulator/crates/mercs2_formats/src/skeleton.rs`](../../crates/mercs2_formats/src/skeleton.rs) (HIER decode; world-rest)
- `.../model_inject.rs` (`DECL64`, `encode_strm`, `collect_donor_skin_vertices`,
  `repose_part_cross_skeleton`), `.../mannequin.rs`, `.../retarget.rs`
- [`docs/skeleton_status.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/skeleton_status.md) (SKIN/vertex-buffer verified layout on `pmc_hum_mattias_v2`)
- `output/_scratch/cj/donor_geometry_format.md` (real donor `obama_faithful4` byte parse)
- [`tools/ucfx_mesh_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py) (`decode_prmg_skin_influences`, `parse_hier_inverse_bind_matrices`)
- [`docs/mercs2-pdb-analysis/animation-skeleton.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/animation-skeleton.md) (Xbox devkit symbol/RTTI evidence)
- [`docs/heli_rig_dissection.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/heli_rig_dissection.md) (animgroup = separate block, clips bound by node hash)
- [`docs/format_reference.md`](../format_reference.md) §11–12 + `tools/hk_anim/{interleaved,wavelet,delta}.py`
- `output/_ghidra/all_functions_decomp.txt` (`FUN_0067c780` ASTO/TRCK/VALU reader)

---

## 1. Per-vertex skinning (LBS)

### 1.1 Where skin data lives

Skeleton and skin data are **embedded inside the mesh model block** (WAD asset type
hash `0x5B724250` = "model"), not in separate files. A model block is a 20-byte WAD
wrapper + a `UCFX` container:

```
WAD wrapper (20B): +0 flags(=1)  +4 name_hash(m2)  +8 type_hash(0x5B724250)
                   +12 zero      +16 ucfx_payload_size    +20 UCFX…
UCFX header (20B): +0 "UCFX"  +4 data_area_off  +16 n_desc
Descriptor rows (20B each): tag[4] | u0(u32) | size(u32) | u2(u32) | u3(u32)
   u0 == 0xFFFFFFFF  => container marker (u3 = child/relative count, size = 0)
   real u0           => leaf body at (data_area_off + u0), length = size
Trailer: "CSUM" + crc32_mercs2(container[:-8])
```

Relevant chunks in a skinned model:
- **`HIER`** — the bone hierarchy (the skeleton). 176-byte nodes (§1.2).
- **`GEOM` → PRMG groups** — the render sub-meshes: each has `STRM`(info/decl/data),
  `IBUF`(info/data), `PRMT`. This is where the per-vertex blend data lives.
- **`SKIN`** containers — per-submesh **markers** (`u0 = 0xFFFFFFFF`). Each holds a
  4-byte `INFO` hash + a nested `PRMG` whose `STRM`/`IBUF` offsets point at the **same**
  buffers as the render mesh. **SKIN carries no separate weights blob** — bone indices
  and weights are interleaved in the vertex buffer (§1.3). SKIN's role is to *flag*
  which draw groups are skinned and name them.
- **`BSHP`/`BSHI`** — blend-shape / morph target data (out of scope here).

### 1.2 The skeleton: HIER chunk (CONFIRMED, 3 independent sources)

Per-node record, **176 bytes**, mirrored in `skeleton.rs` and `ucfx_mesh_codec.py` and
re-verified on real donors:

```
+0   u32   name_hash            (bone names are STRIPPED on disk — hash only)
+4   u16   index_a (=1)
+6   u16   first_child          (0xFFFF = leaf)
+8   u16   parent               (0xFFFF = root)     <- parent[i] < i guaranteed
+10  u16   sibling              (0xFFFF = none)
+12  u32   flags
+16  4x4 f32  LOCAL transform   (row-major, affine, translation in ROW 3)
+80  4x4 f32  INVERSE-BIND      (row-major, world-space inverse)  == InvBind_M
+144 vec3+pad tail_bbox_min
+160 vec3+pad tail_bbox_max
```

Convention: **row-vector / row-major**, so `world(bone) = local(bone) @ world(parent)`,
root world = its local. A single forward pass resolves world-rest (parent index < child
index is guaranteed by the exporter).

CONFIRMED by: (a) `skeleton.rs` computes symmetric world positions for `mattias_v2`
(root at origin, head ~1.66, hands ±0.46); (b) `skeleton_status.md` verifies
`world_transform * InverseBind == identity` for all 95/95 nodes; (c) donor byte parse
in `donor_geometry_format.md` (top INFO+32 = bone count = HIER_bytes/176 = 90 for obama).

Bone count is per-model (e.g. 90 obama, ~95 mattias, 86 for the Mi-26 vehicle rig).

### 1.3 Vertex blend-data layout — the DECL64 stream (CONFIRMED, 3 sources)

Meshes are D3D9-style. The vertex declaration is a `decl` leaf: an array of 8-byte
`D3DVERTEXELEMENT9`-shaped elements, terminated by `ff 00 00 00 11 00 00 00`:

```
decl element (8B): [u16 Stream][u16 Offset][u8 Type][u8 Method][u8 Usage][u8 UsageIndex]
```
(NB: in the on-disk records the Type/Method/Usage/UsageIndex fields are stored as the
low bytes of two u16s; the injector's `DECL64` writes them as `[u16 type][u16 usage]`.)

The two skinned strides seen in shipped human/vehicle models:

**stride 32 (6-element, no tangent)** and **stride 40 (7-element, WITH tangent).**
Blend fields are identical in both:

| Field         | Offset | D3D type            | Notes |
|---------------|-------:|---------------------|-------|
| POSITION      |   +0   | 16 = FLOAT16_4      | xyz + w=1.0 (0x3c00) |
| TEXCOORD0     |   +8   | 15 = FLOAT16_2      | u,v |
| COLOR0        |  +12   | 4  = D3DCOLOR (BGRA8) | ~white |
| **BLENDINDICES** | **+16** | **5 = UBYTE4**   | **u8×4 bone indices** |
| **BLENDWEIGHT**  | **+20** | **8 = UBYTE4N**  | **u8×4, weights sum to 255** |
| NORMAL        |  +24   | 16 = FLOAT16_4      | unit, xyz + w |
| TANGENT (s40) |  +32   | 16 = FLOAT16_4      | tx,ty,tz + handedness sign |

CONFIRMED by: (a) `model_inject::DECL64` and `encode_strm` write exactly this;
(b) `skeleton_status.md` "BLENDINDICES @+16, BLENDWEIGHT @+20, weights sum to 255";
(c) real donor vertex 0 in `donor_geometry_format.md`: BLENDINDICES `0b 0a 00 00` =
bones [11,10,0,0], BLENDWEIGHT `e9 16 00 00` = [233,22,0,0] (233+22 = 255). Multi-UV
bodies add TEXCOORD1..6 on streams 1..6 (a 112-byte decl); the skin fields stay on
stream 0 unchanged.

Weight decode: `w_i = byte_i / 255.0`. Up to 4 influences/vertex. Unused influences
have weight byte 0 (their index byte is then don't-care; treat as bone 0).

### 1.4 Bone-palette resolution — CORRECTED 2026-07-01: PER-GROUP palette-relative

> **⚠️ SUPERSEDED.** The previous text of this section claimed BLENDINDICES are
> **global** HIER indices ("CONFIRMED, 2 independent sources"). **That was WRONG.**
> A screenshot-verified RCA on 2026-07-01 (memory `blendindices-per-group-palette`)
> proved BLENDINDICES are **per-draw-group palette-relative**. The old claim is
> preserved only as a caution at the bottom of this section.

**FINDING (CORRECTED, screenshot-verified 2026-07-01): BLENDINDICES are PER-DRAW-GROUP
palette-relative. Each PRMG group has its own bone palette = the concatenation of a
range table stored in the group's `INFO(56)` leaf. Vertex joints index that palette,
NOT the global HIER array.**

Per-group palette layout (`INFO(56)` leaf):
```
+20  u32  range_count
+24  range_count × { u16 hier_base, u16 count }
```
The group's palette is `concat over ranges of [hier_base .. hier_base+count)`. A vertex
joint `j` maps to a global HIER index by walking the ranges and accumulating:
```
// oracle: FUN_00479d90 uploads pose + hier_base*0x30 for count*3 rows per range,
// accumulating dest offset — i.e. it lays the ranges out contiguously into the palette.
global_hier_index = resolve_palette(group.ranges, j)
bone_matrix       = pose_matrix[ global_hier_index ]
```

**Why the old "global" reading looked right but was wrong:** it is invisible at bind
(identity palette), so obama-in-the-wardrobe rendered fine at rest. It only diverges
once a clip drives the bones — mis-indexed verts blend whatever bones the clip happens
to move. This single root cause produced **two** long-standing symptoms: the idle
(`0x24F8C8E6`) hand-claw and the walk (`0x53682784`) head-fan. One fix in
`model_cubeize.rs` (expand the INFO range table, remap joints→global at read; gated
1≤rc≤8, count>0, base+count≤4096, total≤256) killed both — fingers render correctly for
the first time. Evidence: Mattias `0xA3C1FABC` has no vertex index >45 yet rig=100 bones
(fingers 63–99 unreachable as-global); arm group `rc=4 [(4,1),(21,4),(58,26),(85,15)]`;
post-fix mean vertex→bone distance 0.04–0.22 m (was up to 6344 far-verts/group).

**SEGM is still not a bone palette.** SEGM is the 64-byte segment/LOD table (16 records
× 4B: `00 00 seg_id lod_or_count`) paired 1:1 with the `GEOM`→`INDX` segment→sub-mesh
map; it groups draw calls by LOD/segment. The bone palette lives in the per-group
`INFO(56)` range table above, not in SEGM.

**Kitbash-pipeline consequence (audit needed):** `mannequin.rs` /
`collect_donor_skin_vertices` and the `model_inject`/`retarget` code authored verts
assuming **global** indices. That assumption is invalid — audit the donor-injection
kitbash path (relates to the sarah/obama DLC skin ports).

<details><summary>Historical (DISPROVEN) claim, kept for context</summary>

The section originally read: *"BLENDINDICES are GLOBAL HIER node indices … There is NO
per-draw-group bone palette … `skeleton_status.md`: 'Joint indices = HIER node index' …
`mannequin.rs` uses raw global 95/90-bone indices … proven in-game (obama)."* This was
disproven by decomp oracle FUN_00479d90 + the screenshot-verified fix above.
</details>

### 1.5 The LBS math

Standard linear blend skinning, row-vector convention (matching `skeleton.rs`
`transform_point`: `p' = [p,1] @ M`).

Per bone `b`, per animated pose, the **skinning matrix** is:
```
Skin_b = InvBind_M[b] @ WorldPose_M[b]
```
- `InvBind_M[b]` = HIER node `+80` inverse-bind (already world-space inverse), OR
  compute `affine_inverse(world_rest[b])` — the two are the same up to precision
  (`world_rest * InvBind == identity`, verified 95/95). Prefer the on-disk `+80` matrix
  as the source of truth; fall back to computing the inverse only if it is absent/garbage.
- `WorldPose_M[b]` = the bone's animated world transform this frame. At **bind/rest**
  (static pose, Phase A) `WorldPose_M[b] == world_rest[b]`, so `Skin_b == Identity` and
  the mesh renders exactly as authored (the bind-pose sanity gate).

Per vertex (positions and normals):
```
p' = Σ_{k=0..3}  (w_k / Σw) · ( [p,1] @ Skin_{ BLENDINDICES[k] } )
n' = normalize( Σ_{k=0..3} (w_k/Σw) · ( n @ R3x3(Skin_{ BLENDINDICES[k] }) ) )
```
where `w_k = BLENDWEIGHT[k]/255`. Because weights already sum to 255, `Σw≈1`; still
normalize defensively (a stray 0-weight rig uses `[255,0,0,0]` → bone 0 rigid).

This is the same math the kitbash re-pose driver applies at build time
(`repose_part_cross_skeleton`: `v' = Σ w_b (World_M[map(b)] @ InvBind_S[b]) v`),
which is independent confirmation of the row-vector LBS composition order.

**GPU form (wgpu):** upload `Skin_b` as a bone-matrix palette (a storage buffer or a
per-draw uniform array — the original exposes `BoneMatrixArray` per draw, see the
`PgSkin*VP` shaders). The palette is indexed directly by BLENDINDICES (no remap). Skin
in the vertex shader. Original decl is FLOAT16 for pos/normal/tangent — decode f16→f32
(or feed as `Float16x4` vertex attributes, natively supported by wgpu). Winding note:
IBUF is a **triangle STRIP** with degenerate stitches (odd-index triangles reversed),
not a triangle list — de-strip (`strip_to_tris` logic) or draw as strip.

---

## 2. Animation clip format

### 2.1 Where clips live (CONFIRMED, 2 sources)

Animation clips do **NOT** live in the model block. They live in a **separate
"animgroup" WAD block** (asset "animation", magic `0x18166555` =
`pandemic_hash_m2("animation")`), which the ASET orchestration resolves alongside the
geometry block. Example (Mi-26 helicopter):
- geometry block `vz_veh_helicopter_mi26_wheels_P001_Q2` — meshes, INDX, no HIER
- **animgroup block `vehiclenameanimgroup_mi26_P000_Q3`** — **43 Havok packfiles**,
  `hkaSkeleton` (86 bones), **wavelet clips** (`opendoor`, `closedoor`, `ahj_gear_*`),
  annotation/event tracks.

CONFIRMED by: (a) `heli_rig_dissection.md` (block census above); (b) `format_reference.md`
§11 animgroup carve + `skeleton_status.md` "track counts are per-animgroup: 60 for
humans, 22 for vehicles/helis."

Animgroup block structure (`format_reference.md` §11): a leading **record table** —
16 bytes/record: `u32 checksum`, `u32 magic (0x18166555)`, `u32 reserved`,
`u32 record_size` — followed by concatenated `UCFX` wrappers. Inside each wrapper the
Havok slice begins at the ASCII token `Havok-5.5.0-r1`.

Inside a UCFX anim wrapper the descriptor tags are `ASTO`/`ITEM`/`SINF`/`MANM`/`TRCK`/
`VALU`/`TYPE`/`MINF` (see `tag_registry`): `ASTO` (`FUN_0067c780`) reads a u32 count then
`count*4` alloc; `INFO`/`TYPE`/`VALU`/`TRCK` are read as inner sub-chunks — this is the
Pandemic wrapper around the Havok payload, not the sample data itself.

### 2.2 Havok packfile + skeleton mapping

The payload is a **Havok 5.5.0-r1 32-bit packfile** (same family as PHY2 collision,
readable by the packfile walker in `havok.rs` / `hk_packfile.py`: three 48-byte section
headers `__classnames__`/`__types__`/`__data__`, hashed classname table, chained
local→global→virtual→finish fixups terminated by `0xFFFFFFFF`).

Runtime binding (from the Xbox symbol evidence, `animation-skeleton.md`):
- `hkaSkeleton` — bone list (`m_bones[].m_name`, `m_parentIndices`) + `referencePose`
  (one **48-byte `hkQsTransform`** per bone = translation vec3 + quat + scale vec3;
  confirmed by PC `FUN_0089d280` copying `poseCount * 0x30`).
- `hkaAnimationBinding` — maps animation **transform tracks → skeleton bone indices**
  (`transformTrackToBoneIndices` / `animationTrackToBoneIndices`), with separate
  mirrored/un-mirrored bindings. So a clip is authored in *track space*; the binding
  scatters sampled tracks onto skeleton bones.
- Skinning skeleton (mesh HIER) and animation skeleton are related by
  `hkaSkeletonMapper` (and a ragdoll skeleton via `animationTo/ragdollSkeletonMapper`).

> **Caveat (OPEN, see §5):** the mesh HIER bone order and the animgroup `hkaSkeleton`
> bone order are NOT proven identical. The vertex BLENDINDICES index the **mesh HIER**;
> clips index the **hkaSkeleton** via `hkaAnimationBinding`/`hkaSkeletonMapper`. The
> renderer must resolve each animated track onto the HIER bone it drives (by name-hash
> match is the safe path, since everything is addressed by Pandemic node hash).

### 2.3 Clip compression variants (implementation status known)

Three `hkaSkeletalAnimation` subclasses appear in the shipped data; decoders exist and
are the spec for the format:

| Class | Layout (HK550 32-bit, offsets from struct start) | Decoder |
|-------|--------------------------------------------------|---------|
| `hkaInterleavedUncompressedAnimation` | `+8 duration f32`, `+0x10 numTransformTracks u16`, `+0x12 numFloatTracks u16`, `+0x14 numFrames u32`, `+0x30` frames: `numFrames × numTransformTracks × 40-byte hkQsTransform` (T:vec3 @+0, quat @+12, S:vec3 @+28), then float tracks | [`tools/hk_anim/interleaved.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/interleaved.py) — **complete** |
| `hkaWaveletSkeletalAnimation` | 96-byte header: `+8 type(=3)`, `+12 duration f32`, `+16 numTransformTracks`, `+20 numFloatTracks`, `+36 numPoses`, `+40 blockSize`, `+44 QuantizationFormat(20B: numD, offsetIdx, scaleIdx, bitWidthIdx)`, `+64 staticMaskIdx`, `+68 staticDofsIdx`, `+72 blockIndexIdx`, `+76 blockIndexSize`, `+80 quantDataIdx`, `+84 quantDataSize`, `+92 numDataBuffer` | [`tools/hk_anim/wavelet.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/wavelet.py) — **complete** (static-mask classify → per-DOF quant offset/scale/bitWidth → per-block quantized coeffs → inverse-Haar lifting → per-frame TRS) |
| `hkaDeltaCompressedSkeletalAnimation` | header/meta (duration, track count, frame count, quant format byte, block-size hint) | [`tools/hk_anim/delta.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/delta.py) — **header only**; full staticMask+quantizedData bitstream NOT implemented (see HKLib `HKAnimationData.Delta`) |

Vehicles/humans in the shipped game predominantly use **wavelet** clips (the
fully-implemented path). Common shape: per-clip `duration`, `numTransformTracks`
(60 human / 22 vehicle), `numPoses` frames; each frame yields a TRS per track.

> **Authoritative decoder (2026-07-01):** the maintained wavelet decoder is the Rust
> port in [`tools/wad_simulator/crates/mercs2_formats/src/anim.rs`](../../crates/mercs2_formats/src/anim.rs) (the Python
> `hk_anim/wavelet.py` is legacy). It is **numerically validated against a live x32dbg
> capture** — 660/660 recompose floats ≤1e-4, 246/246 decompress coeffs ≤1e-3, 64/64
> oracle rotations. Full method, the 4 fixed bugs, and the fixture manifest are in
> [`docs/modernization/wavelet_decode_verification.md`](wavelet_decode_verification.md). Note the original Python decoder
> carried 4 subtle bugs (frame-vs-time, entropy-advance count, dequant bias, mult/addend
> swap) that only surfaced under live-capture diffing — do not treat the Python as ground
> truth.

### 2.4 Runtime sample + apply pipeline

From `animation-skeleton.md` (Pangea on top of Havok Animation `hka` + Behavior `hkb`).
The per-frame pipeline (ordering inferred from the "Animation Debug Mode" profiler label
sequence — see §5):

1. Select active clip(s) (`hkbClipGenerator`/`hkbBehaviorGraph` state machine; for a
   first pass, drive one clip by time).
2. **Sample** the clip at time `t` → a per-track local `hkQsTransform` (`hkAnimation::samplePose`
   / `Animation::HumanSamplePose`; decode via the variant decoder in §2.3). `t` wraps by
   `duration`; `fps = numFrames/duration`. Interpolate between adjacent frames (LERP
   translation/scale, NLERP/SLERP quaternion; `HACKRenormalizeQuats` marker in the
   binary implies quat renormalization after blending).
3. **Bind** each sampled track to its skeleton bone via `hkaAnimationBinding`
   (`transformTrackToBoneIndices`). Bones not driven by a track keep `referencePose`.
4. **Build model-space transforms** by chaining local TRS through parents
   (`BuildModelSpaceTransforms`) → `WorldPose_M[b]` per bone.
5. Apply post-sample **bone controllers** (`BoneCtrl*` — LookAt, LocalRotation for
   rotor spin, FakeWheel, StrapOn, PhysicsActor). Each is a per-bone procedural modifier
   bound to a u16 bone index (CONFIRMED: `BoneCtrl*` factories read node+4 = bone index).
   The Mi-26 rotor spin is `BoneCtrlLocalRotation`, NOT a clip.
6. Optional IK (`hkbFootIkModifier`, `hkaFootPlacementIkSolver`) and ragdoll blend.
7. **Skin:** `Skin_b = InvBind_M[b] @ WorldPose_M[b]` → `BoneMatrixArray` palette → LBS
   in the `PgSkin*VP` vertex shader (§1.5).

For the modernization, Phases A–B (below) implement steps 2→4→7 with a single clip and
no controllers/IK; controllers, blending, IK, ragdoll are later gated phases.

---

## 3. Phased implementation plan

**Phase A — static bind pose skinning (no animation).**
- Mesh reader: parse `STRM info/decl/data`, decode POSITION/NORMAL/TANGENT (f16),
  TEXCOORD, COLOR, **BLENDINDICES (+16 u8×4)**, **BLENDWEIGHT (+20 u8×4n)** by the decl.
- Skeleton: `Skeleton::from_block` (HIER) → `world_rest[b]` and `InvBind_M[b]` (from +80).
- Set `WorldPose_M[b] = world_rest[b]` ⇒ every `Skin_b = Identity`.
- Skin on GPU with the bone palette indexed directly by BLENDINDICES.
- **Gate:** rendered frame == the current static bind-pose renderer (render-golden hash).
  This proves the whole skin path (decl decode, palette indexing, LBS) with the identity
  transform, isolating it from animation. De-strip IBUF correctly (triangle strip).

**Phase B — single-clip playback.**
- Load the paired **animgroup** block; carve the Havok packfile(s) (record table →
  UCFX → `Havok-5.5.0-r1`); parse `hkaSkeleton` (`m_parentIndices`, name-hashes,
  `referencePose`) + `hkaAnimationBinding` (`transformTrackToBoneIndices`).
- Implement the **wavelet** decoder first (port `hk_anim/wavelet.py`; it covers the
  shipped human/vehicle clips), then interleaved; delta is a stretch.
- Sample one clip at time `t` (frame LERP + quat NLERP), scatter tracks onto bones via
  the binding, chain to model space, compose `Skin_b`, animate.
- Resolve animgroup-skeleton ↔ mesh-HIER bone correspondence **by name-hash** (§5).
- **Gate:** a chosen clip (e.g. a door-open or an idle) matches the original exe's pose
  at the same `t` (struct-dump Surface-A diff of `BoneMatrixArray`, or perceptual
  render-golden of the animated frame).

**Phase C — controllers + blending + IK + ragdoll (later, gated individually).**
- `BoneCtrl*` post-sample modifiers (LocalRotation for rotor spin is the smallest first
  win); `hkb*` blend graph; foot IK; powered-ragdoll blend. Each is its own oracle gate.

---

## 4. Concrete constants / cheat-sheet

- HIER node stride: **176 B**; parent @+8 u16 (0xFFFF=root); local @+16; InvBind @+80.
- Vertex blend: **BLENDINDICES @+16 (UBYTE4)**, **BLENDWEIGHT @+20 (UBYTE4N, Σ=255)**.
- Skinned strides: **32** (no tangent) / **40** (tangent); f16 pos/normal/tangent.
- BLENDINDICES = **per-group palette-relative** (index the group's `INFO(56)` range
  table `{u16 hier_base, u16 count}×rc` at +24, concatenated → global HIER). **NOT** a
  global HIER index — see §1.4 (corrected 2026-07-01).
- `Skin_b = InvBind_M[b] @ WorldPose_M[b]`, row-vector (`p' = [p,1] @ Skin`).
- IBUF = u16 **triangle strip** (degenerate-stitched; odd triangles reversed).
- Clips: separate **animgroup** block, Havok-5.5.0-r1 packfile, hkQsTransform = **48 B**
  (T vec3 + quat + S vec3). Interleaved data starts @+0x30; wavelet header = 96 B (type=3).
- Track counts: 60 human / 22 vehicle. Clips reference bones via `hkaAnimationBinding`.

---

## 5. OPEN QUESTIONS (need manual review / verification)

1. **[RESOLVED 2026-07-01] Mesh-HIER bone order vs animgroup hkaSkeleton bone order.**
   The track→bone binding (trnm resolve) was fully EXONERATED: 60/60 + 64/64 tracks
   resolve, and decoded track locals match bind-locals to <5e-5. The head-fan / hand-claw
   that were suspected here were actually the per-group BLENDINDICES bug (see §1.4). No
   runtime track→HIER remap divergence remains for the tested human rig (Mattias
   `0xA3C1FABC`, idle + walk). (Heli/vehicle rigs not separately re-verified.)

2. **[RESOLVED 2026-07-01] `hkaWavelet` decoder fidelity vs the live engine.** The decoder
   was re-ported from the decomp and validated numerically against a matched
   `(raw clip → coefficients → pose)` triple captured live from the running game via
   x32dbg (breakpoints at `LtSampleWave` FUN_009f5e40 entry and the `StRecomposeW` call
   site 0x9f6396). Results: Stage-2 StRecomposeW **660/660 pose floats ≤1e-4**; Stage-1
   decompress+dequant+wavelet+interp **246/246 coeffs ≤1e-3**; 64-track oracle clip
   **64/64 rotations**. Fixtures:
   [`tools/wad_simulator/crates/mercs2_formats/tests/fixtures/wavelet_capture_2p567s/`](../../crates/mercs2_formats/tests/fixtures/wavelet_capture_2p567s/);
   gates: `tests/wavelet_{decompress,recompose}.rs`. Four decode bugs were fixed (frame/
   frac is TIME not frame units; entropy-advance counts PRESENT not all codes; dequant
   `+0.5`/dc-sign hack removed; mult/addend arrays were swapped). See §2.3 and
   [`docs/modernization/wavelet_decode_verification.md`](wavelet_decode_verification.md).
   *Still UNVERIFIED:* the `blockSize≠8` and `preserved>0` decode paths (no fixture
   exercises them); delta-compressed clips (see #3 below).

3. **[MED] Delta-compressed clips.** `delta.py` is header-only (emits identity TRS). If
   any shipped human/vehicle clip is delta-compressed (not wavelet/interleaved), Phase B
   is incomplete for it. **Verify:** census the animation `type` byte across all animgroup
   Havok slices; if delta is present, port the full staticMask+quantizedData walk (HKLib
   `HKAnimationData.Delta`).

4. **[MED] hkQsTransform scale semantics + quaternion component order.** Interleaved uses
   T@+0, quat@+12 (x,y,z,w), S@+28. Whether scale is per-axis vs uniform in practice, and
   whether the packed quat uses a w-sentinel (`fix_quat_w_sentinel` exists in the shared
   code) needs a real-clip check before trusting blends. **Verify:** decode one known clip
   and confirm quats are unit and scales ~1 across frames.

5. **[MED] Pipeline ordering (sample → controllers → IK → ragdoll → skin).** This order
   is inferred from the "Animation Debug Mode" profiler *label* sequence, not from traced
   control flow (`animation-skeleton.md` explicitly marks it inferred). Fine for Phase A/B
   (which skip controllers/IK), but must be confirmed before Phase C blending.

6. **[LOW] >4 influences / weight normalization edge cases.** All observed rigs are ≤4
   influences with Σ=255. Confirm no shipped mesh uses a wider blend (would need a
   different decl) — the decl terminator + UBYTE4 field caps it at 4, so this is likely
   closed, but worth a corpus scan.

7. **[LOW] Reference/rest-pose source of truth.** Bones with no animation track fall back
   to `referencePose` (48-B hkQsTransform per bone in the hkaSkeleton). Confirm this
   equals the mesh HIER local transform (they should agree; if not, prefer the animgroup
   referencePose for animated frames and the HIER for the static bind-pose gate).
