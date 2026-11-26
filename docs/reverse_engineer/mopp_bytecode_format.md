# Havok 5.5 MOPP bytecode format — decoded & proven

**Scope:** the on-disk `hkpMoppCode` MOPP (Memory-Optimized Partial Polytope) bytecode — the compressed
spatial BV-tree that accelerates static-world collision ([`terrain_collision_regeneration.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/terrain_collision_regeneration.md)).
This is the format a **native Rust MOPP compiler** must emit to regenerate collision for new geometry
without the HCT/DCC toolchain (walled — see memory `mopp-bake-oracle-hct-recipe`). **Status:** decoder
PROVEN; split-plane geometry PROVEN (see "Split geometry"); encoder is **spatially conservative** and
geometry-gated (`query_aabb` no-miss on synthetic + real meshes, FindAll + leaf-box gates on 43 real
`vz.wad` MOPPs). Upgrades the long-standing "recognised, not decoded" note in
[`physics_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/physics_code_map.md) §2.

## How it was proven
Decompiled the 14 functions in `hctFilterPhysics.dll` that reference the validator string *"Unknown
command – This mopp data has been corrupted…"* (`.\Collide\Mopp\Machine\hkpMoppObbVirtualMachine.cpp`).
Three independent VM variants — `FUN_10081890` (OBB query), `FUN_100a4630` (early-exit OBB), `FUN_100a59d0`
(KDop) — share **one identical instruction encoding**; that switch statement *is* the instruction set. A
decoder mirroring it (geometry tests stripped, FindAll semantics: visit every node, emit every leaf) walks
the real reference MOPP with **901/901 bytes consumed, 0 errors, 76 distinct contiguous shape-keys [0..76]**
(only key 32 absent). Deliverables archived at `Temp\hunt\mopp\{mopp_decode.rs,decode.py,phys_moppvm.txt,progress.md}`.

## `hkpMoppCode` struct + quantization (PROVEN)
```
hkpMoppCode : hkReferencedObject { CodeInfo m_info; hkArray<hkUint8> m_data; hkInt8 m_buildType; }
CodeInfo m_info = one hkVector4 m_offset = [offset.x, offset.y, offset.z, 1/scale]   // lane 3 = RECIPROCAL
```
- **On-disk layout (PC 32-bit, PROVEN):** for the serialized `hkpMoppCode` object, `m_info` sits at
  `obj+16` (offset.xyz @ `+16/+20/+24`, lane 3 @ `+28`); the `m_data` hkArray follows at `obj+32`
  (`ptr @ +32`, `count @ +36`, `capAndFlags @ +40`); `m_buildType` at `+44`.
- **Dequant:** `world_coord = int_coord * scale + offset`, where **`scale = 1.0 / m_info.lane3`**.
  Measured on retail `vz.wad`, lane 3 holds `~1e6`–`2e6` (e.g. `2 280 380` for a ~30 m cell) — far too
  large to be a world-per-unit step; its reciprocal (`~4.4e-7`) is the sane per-integer-unit scale for a
  ~24-bit frame, and matches the VM's `realCoord = intCoord * this[0x10]` multiply (`this[0x10]` = the
  cached `1/lane3`). *(Earlier note said lane 3 = scale directly — corrected: it is `1/scale`.)*
- Integer coords are hierarchical: `int = (operand << shift) + origin[axis]`. The formula is
  `shift = 0x10 - m_info.p3root` (VM `FUN_100a59d0` uses `0x10 - param_1[3]`; OBB `FUN_10081890` stores it
  directly in `param_1[5]`). **★ MEASURED 2026-08-16 — the ROOT shift is a FIXED `16` (`p3root = 0`).**
  Retail-oracle sweep (`mopp.rs::diag_retail_floor_root_shift_oracle`): each of floor `0x39AF17DC`'s 4 baked
  MOPPs, with FOUR DIFFERENT `m_info` scales (5.0e-7 … 2.1e-6), is no-miss over its OWN real triangles at
  shift 16 and ONLY 16; every other shift misses ~all triangles. So the shift is constant, and the *scale*
  is what varies: Havok bakes each mesh's `scale = ext_max / 0xFF0000` (a 24-bit frame — byte operand `0xFF`
  → int `0xFF0000`), so the far face maps to operand `0xFF` at shift 16. *(Corrects the earlier speculation
  that `p3root` varies with mesh size / that a ~30 m cell needs shift ~18 — it does not; it is 16 with a
  finer scale.)* Each REANCHOR (`0x01–0x04`) then does `origin[k] += code[+1+k] << shift; shift -= opcode` —
  refining a child cell BELOW the root's byte resolution. **Coordinate math is NOT needed to recover triangle
  indices** (the decoder ignores it) — but it IS needed for a spatial `query_aabb`/encoder; see below.
- ⚠ `output/_scratch/old_mopp.bin` is only the 901-byte `m_data` buffer (no struct), AND it is the **stale
  u32-scrambled** dump (predates fix `b93e00` by ~4 min). Decode it with `--unreverse` (un-reverse each
  aligned 4-byte word). A MOPP extracted by the *current* `havok.rs` (stores raw u8) decodes with NO unreverse.

## Opcode table (byte → operands → action)
Context carried down each branch: an integer origin/AABB frame, a **shape-key base accumulator**, and a
scale exponent. **Operands are big-endian.** All PROVEN from the VM switch unless marked INFERRED.

| Opcode | Len | Action |
|---|---|---|
| `0x00` | 1 | **RETURN** — end this branch |
| `0x01–0x04` | 4 | REANCHOR + rescale child frame (unary, continue): `origin[k] += code[+1+k] << shift`, then `shift -= opcode`. *PROVEN — see "Re-anchors and cuts in the retail walkers"* |
| `0x05` | var | JUMP8: `pc += code[+1] + 2` |
| `0x06` | var | JUMP16: `pc += code[+1]*256 + code[+2] + 3` |
| `0x07` | var | JUMP24: `pc += BE16(+1)*256 + code[+3] + 4` |
| `0x08` | — | INVALID |
| `0x09` | 2 | key_base **+=** `code[+1]` |
| `0x0a` | 3 | key_base **+=** `BE16(+1)` |
| `0x0b` | 5 | key_base **=** `BE32(+1)` (absolute set) |
| `0x0c` | 3 | Jump to chunk `BE16(+1)`, at `BE16(+1) * 512`. Only PS3 codes hold it; 4 of the PC exe's 14 walkers follow it and 10 take it for an unknown command. See "PS3 chunked code" |
| `0x0d–0x0f` | — | INVALID |
| `0x10–0x12` | 4+ | **SPLIT** axis (op−0x10). `code[+1]`=LEFT.max plane, `code[+2]`=RIGHT.min plane. LEFT child @ `pc+4`, RIGHT @ `pc+4+code[+3]`. *plane geometry PROVEN* |
| `0x13–0x1c` | 4+ | **SPLIT** on 26-DOP diagonal planes. LEFT@+4, RIGHT@+4+code[+3]. *diagonal-plane geometry INFERRED* |
| `0x1d–0x1f` | — | INVALID |
| `0x20–0x22` | 3+ | **SPLIT** (compressed 1-value) axis (op−0x20). `v=code[+1]`: LEFT.max=`(v+1)<<shift`, RIGHT.min=`v<<shift`. LEFT @ `pc+3`, RIGHT @ `pc+3+code[+2]`. *PROVEN* |
| `0x23–0x25` | 7+ | **SPLIT** w/ 16-bit child offsets. `code[+1]`=LEFT.max, `code[+2]`=RIGHT.min. LEFT @ `pc+7+BE16(+3)`, RIGHT @ `pc+7+BE16(+5)`. *PROVEN* |
| `0x26–0x28` | 3 | unary **CUT** axis (op−0x26): box.min[axis]=`code[+1]` plane, box.max[axis]=`code[+2]` plane; continue. **(most frequent byte)** *PROVEN* |
| `0x29–0x2b` | 7 | unary **CUT** (BE24 absolute coords): box.min=`BE24(+1)`, box.max=`BE24(+4)`; continue. *PROVEN* |
| `0x2c–0x2f` | — | INVALID |
| `0x30–0x4f` | 1 | **TERMINAL**: emit key = `key_base + (op−0x30)`, RETURN |
| `0x50` | 2 | TERMINAL: emit `key_base + code[+1]`, RETURN |
| `0x51` | 3 | TERMINAL: emit `key_base + BE16(+1)`, RETURN |
| `0x52` | 4 | TERMINAL: emit `key_base + BE24(+1)`, RETURN |
| `0x53` | 5 | TERMINAL: emit `key_base + BE32(+1)`, RETURN |
| `0x54–0x5f` | — | INVALID |
| `0x60–0x63` | 2 | set property[op−0x60] = `code[+1]`, continue (per-terminal metadata) |
| `0x64–0x67` | 3 | set property = `BE16`, continue |
| `0x68–0x6b` | 5 | set property = `BE32`, continue |
| `0x6c–0xff` | — | INVALID |

**Split child layout:** LEFT child immediately follows the header (inline); RIGHT child at the encoded byte
offset. A split visits both; a terminal / `0x00` ends the branch; CUT / REANCHOR / property ops are unary.

## PS3 chunked code

A PS3 `hkpMoppCode` has `m_buildType` 0 and holds its code in 512-byte chunks for the SPUs, each padded with
`0xCD`. A split's child that lies in another chunk is a 3-byte `0x0C` jump, and the walk's key base carries
across it. On one DLC cell, `dlc01_terrain_r07_c05` (6,024 triangles), the code is 228 chunks and 227 jumps, 222
of them backward, and it walks to the same 6,024 keys as the Xbox code over the same mesh. The Xbox code is
68,249 bytes where the PS3 code is 116,736.

The retail PC exe (pc-v10; pc-v11 is the same code at other addresses) has 14 MOPP walkers, each a
`cmp 0x6b; ja; movzx; jmp [table]` dispatch, named here by the Havok source file its unknown-command
message cites. Four have a case for `0x0C` that does
`pc = [code+0x20] + BE16(+1) * 512` (`code+0x20` is `m_data`'s pointer): AabbCast `0x928E00`, Obb `0xA48E10`
(case at `0xA491E8`), LongRay `0xA49B10` (case at `0xA4A74A`) and Modify `0xA4DB70`. The other ten send
`0x0C` to their "Unknown command" handler, which does not advance `pc`: the four float-AABB walkers
(`0xA446E0`, `0xA45900`, `0xA46DC0`, `0xA47DD0`), EarlyExitObb `0xA4AFE0`, Sphere `0xA4BAD0`, KDop `0xA4C380`
and the three FindAll walkers (`0xA4D130`, `0xA4D4D0`, `0xA4D830`). Those ten are reached only through
wrappers no direct or absolute reference in either image points at, which does not show they never run.
Jumps `0x05–0x07` add an unsigned operand in all 14, so on PC a code can only jump forward within itself.

A PS3 collider's MOPP is therefore recomputed from its mesh
(`mercs2_formats::phy2_moppswap::rebake_phy2_mopps`), which requires the recomputed keys to equal the
source's and the recomputed code to fit the source's bytes. `mopp::decode_chunked` walks the source;
`be_to_le::havok::convert_phy2_be_to_le_as_baked` converts a body and keeps its chunked codes, for
measuring them.

## Split geometry — the plane operands + child-visit rule (PROVEN)
Resolves the earlier "coordinate math INFERRED" note. Ground truth = the VM switch in
`Temp\hunt\mopp\phys_moppvm.txt` (`FUN_10081890` OBB, `FUN_100a59d0` KDop — both agree byte-for-byte).

A MOPP internal node is a **Bounding-Interval-Hierarchy (BIH) split**: two planes on ONE axis, an *inline
(LEFT)* child bounded ABOVE and an *offset (RIGHT)* child bounded BELOW.
- `0x10–0x12`: `Lmax = (code[+1] << shift) + origin[axis]`, `Rmin = (code[+2] << shift) + origin[axis]`
  (both then `* scale + offset` to world). The inline child occupies `[.., Lmax]` on the axis; the offset
  child occupies `[Rmin, ..]`. (KDop: LEFT recursion sets `bound[axis].max = code[+1]`; RIGHT sets
  `bound[axis].min = code[+2]`. OBB: `param_3[axis+4] = code[+1]` for LEFT, a stack temp `= code[+2]`
  becomes RIGHT's min. Identical.)
- `0x20–0x22` compressed: a single `v = code[+1]` → `Lmax = (v+1) << shift`, `Rmin = v << shift` (the two
  planes are one quantum apart — a 1-unit overlap).
- `0x23–0x25`: `code[+1]`/`code[+2]` = `Lmax`/`Rmin` exactly as `0x10–0x12`, with BE16 child offsets.
- CUT `0x26–0x28` sets the running node box **absolutely**: `box.min[axis] = code[+1]`-plane,
  `box.max[axis] = code[+2]`-plane. `0x29–0x2b` uses BE24 **absolute** ints (no `<<shift`, no origin).

**Child-visit rule (the VM's pruning, reduced to an axis-aligned query AABB `[qmin,qmax]`):**
visit the inline child iff `qmin[axis] ≤ Lmax`; visit the offset child iff `qmax[axis] ≥ Rmin`. (The OBB
VM's slab test `FUN_10081890` collapses to exactly this for an axis-aligned query; a CUT prunes the whole
branch when the query is disjoint from the tightened box.) Implemented as `mopp.rs::query_aabb`.

**Conservative encoding (`mopp.rs::encode`).** For each split the inline child's `code[+1]` = its true
max-on-axis rounded UP, and the offset child's `code[+2]` = its true min-on-axis rounded DOWN — so a query
can never miss a boundary triangle. **Root operand shift is `16`** (the measured engine value — see above)
and the frame divides the widest extent by `0xFF0000` (a 24-bit frame) so the AABB-max face maps to
`int = 0xFF0000 = 0xFF<<16`, representable as an upper bound. The frame is taken over the vertices the
triangles use, not the whole pool. *(Historically shift `8` / `÷0xFF00`, which was
internally self-consistent but off by 256× vs the engine — the root cause of the authored-floor spatial-MOPP
fall-through, fixed 2026-08-16.)*

A node of the encoder's is, in order: one re-anchor (`0x01–0x04`) where a frame at the node's own corner
holds it at a finer shift; a cut (`0x26–0x28`) on each axis where the node's box is slack against what its
ancestors' planes leave it; a key-base raise (`0x09`/`0x0a`/`0x0b`) where that shortens the node's terminals
by more than the raise costs; then a terminal or a split. The half of a split that is lower on its axis
always goes inline: `code[+1]` bounds the inline child above and `code[+2]` the offset child below, so the
other way round each plane would bound a half on the side its parent already bounds and the split would
prune nothing. `mopp::Tuning` holds the two thresholds (the fewest bits a re-anchor may gain, the least
share of a node's inherited extent a cut may remove); `phy2_moppswap` tries `Tuning::TO_FIT` in turn when a
code has to fit the bytes of the one it replaces.

## Re-anchors and cuts in the retail walkers (PROVEN, pc-v10)

Read from the Obb walker `0xA48E10` and the LongRay walker `0xA49B10` of the retail PC exe:

- **Query frame (Obb).** The walker holds the query in the code's 24-bit integer frame, one unit wider on
  each side: `qmin = round((q.min - offset) * (1/scale)) - 1`, `qmax = round((q.max - offset) * (1/scale)) + 1`.
  At a node it compares them shifted down to the node's frame.
- **Re-anchor `0x01–0x04`** (Obb at `0xA49203`, LongRay at `0xA4A765`): `origin[k] += code[+1+k] << shift` for
  the three axes, then `shift -= opcode`. No bound is checked. Obb keeps the origin in the node's own units
  and a shift that counts up from 0, so its form of the same move is `origin[k] = (origin[k] + code[+1+k])
  << opcode`; LongRay keeps the ray in floats and does `ray[k] = (ray[k] - code[+1+k]) * 2^opcode`.
- **Split `0x10–0x12`:** the inline child is visited iff `qmin < code[+1]`, the offset child iff
  `qmax > code[+2]`; the two operands are read independently of each other.
- **Cut `0x26–0x28`:** the branch is left when `qmax < code[+1]` or `qmin >= code[+2]`.

`mopp::query_aabb` walks with these rules. `mopp::query_aabb_steps` also returns how many opcodes the walk
read.

## What Havok's own codes hold

`mercs2_probe mopp-census <pc wad>` walks every code of every PHY2 body and counts opcodes, opcode pairs
along a branch, the shift each re-anchor, split and cut sits at, a split's two bounds against each other
and a cut's width; `--archive <big-endian archive>` does the same for a console archive. The census of
retail PC `vz.wad` (4,380 codes in 1,964 bodies) is [`docs/data/mopp_census_pc_retail.tsv`](../data/mopp_census_pc_retail.tsv).
Across those codes:

- No `0x52` terminal. A key more than 16 bits past the key base is a `0x53`.
- No re-anchor directly after a re-anchor.
- No frame below shift 7 (four re-anchors reach 7).
- No `0x26–0x28` cut whose two bounds are the same plane, in 7.5 million cuts.
- No `0x10–0x12` split whose inline bound is exactly one above its offset bound: that case is always the
  3-byte `0x20–0x22` form.
- No `0x00`, no `0x29–0x2b` cut and no `0x60–0x6b` property; every other opcode of the table above occurs.

The PS3 DLC package's 233 codes hold nothing retail PC's do not except the chunk jump and the pairs it
makes; their lowest shift is 8.

The encoder keeps to the first four (`mopp.rs::a_code_stays_inside_what_retail_codes_hold`). It differs in
two things, both listed by `mopp-census <built wad> --never-in docs/data/mopp_census_pc_retail.tsv`: it
writes a split whose bounds are one apart in the 4-byte form, and its split over an inline child past
64 KiB (below) puts the JUMP24 in the offset child of a `0x23–0x25`, where Havok's JUMP24s follow a
`0x10–0x12`.

## How selective a code is

`mercs2_probe mopp-selectivity <pc wad> (--blocks a,b,c | --match <path part>)` casts ray-like segments (a
2 m vertical probe, a 25 m level ray, a 60 m slanted ray) through each collider and reports, for the code
in the file and for the code the encoder computes over the same mesh, the triangles the walk hands back,
the count whose box the segment overlaps, and the opcodes read. `--archive <big-endian archive>` measures
a console archive's codes as its platform baked them against the code its conversion writes. Any triangle
a segment overlaps that a code does not hand back, and any collider that cannot be measured, is exit 1.

**Child offsets past 64 KiB (PROVEN from the retail engine, 2026-09-27).** The widest offset a split
carries is 16 bits. All 14 MOPP virtual machines in the unpacked retail `Mercenaries2.exe` (Ghidra decomp,
e.g. the OBB walker `FUN_00a48e10`) read `0x10–0x12` with a one-byte RIGHT offset (`pc + 4 + code[+3]`),
`0x23–0x25` with two BE16 offsets (`LEFT = pc + 7 + BE16(+3)`, `RIGHT = pc + 7 + BE16(+5)`), and `0x07`
(JUMP24) as `pc + 4 + BE24(+1)` — the only 24-bit offset in the instruction set. The encoder used to write
`(off >> 8) as u8` into the BE16 field with no bound, silently dropping the high bits once an inline
subtree passed 65,535 bytes: a 96,800-triangle grid baked a 974,137-byte tree whose walk reached only
121,782 bytes. A whole terrain cell's collider (up to 81,619 triangles in retail) passes that point. `encode` and
`encode_return_all` now emit, for an inline child over 0xFFFF bytes,
`[0x23+axis, Lmax, Rmin, 00 04, 00 00] [07 BE24(len(left))] [left…] [right…]` — LEFT at `pc + 11`, RIGHT at
`pc + 7`, which is a JUMP24 over the left subtree to the right one — and panic past the 24-bit range.
Gated by `mopp.rs::a_tree_past_64k_offsets_still_walks_every_key` (every byte reached, every key once,
no-miss queries) and by rebuilding all 400 terrain cells' colliders as one shape each.

**Gates (all green, `cargo test -p mercs2_formats mopp`):**
- NO-MISS: `query_aabb(encode(mesh))` ⊇ brute-force AABB-overlap set over thousands of random queries, on a
  quad, a box, a 2 k random-tri soup, and 5 real `WpMeshShape16` meshes from `vz.wad` — **zero misses**;
  mean over-inclusion 1.0–4.1× (candidates / true overlaps).
- FindAll equivalence: `query_aabb(all-space)` == `decode()` shape-key multiset on **43 real MOPPs** —
  proves the traversal (REANCHOR / JUMP / CUT / 26-DOP) is byte-correct on Havok's own bytecode.
- Leaf-box reachability: on **2 703 real leaf boxes** across 43 MOPPs, a query at each fully-bounded leaf's
  reconstructed box centre returns that leaf — proves the pruning is nesting-correct (no over-prune).

**Resolved 2026-08-16 (was "still open"):** the *absolute* `query_aabb(realMOPP) ⊇ brute-force-over-source-tris`
cross-check that was "BLOCKED by frame alignment" is now green — the blocker was the wrong root shift, not a
mesh-decode issue. At the MEASURED shift `16` (not the old `8`), each of retail floor `0x39AF17DC`'s 4 MOPPs
is no-miss over its own decoded triangles (`mopp.rs::diag_retail_floor_root_shift_oracle` +
`phy2_build.rs::spatial_mopp_no_miss_and_prunes_in_common_frame_if_present`). The 26-DOP diagonal split
plane geometry (`0x13–0x1c`) is still INFERRED; `query_aabb` visits both children unpruned for those
(conservative), and `encode` never emits them.

## Phase-3 encoder plan (we emit our OWN valid tree, not Havok's exact one)
1. Build any BVH over the triangles (median/SAH, axis-aligned).
2. Per node: quantize the split plane into the current integer frame; emit `0x10–0x12` (axis split, 1-byte
   right offset), or `0x23–0x25` (16-bit) when the left subtree exceeds 255 bytes, with a JUMP24 trampoline
   once it exceeds 65,535 (see "Child offsets past 64 KiB"). Recurse LEFT inline, place RIGHT at the
   recorded offset.
3. Leaves as terminals. **MVP-safe:** `0x0b <BE32 tri_index>` then `0x30` (delta 0) per leaf — trivially
   correct absolute keys; optimize to small deltas + `0x09/0x0a` base bumps later.
4. Optional: tighten node AABBs with CUT ops (`0x26–0x28`) for prune performance — not required for correctness.

### Invariants the emitter MUST honor
- Every byte reached at an instruction boundary must be a valid opcode (avoid all INVALID ranges) — else the
  "corrupted" assert fires and the load-time walk can read OOB.
- Child offsets must land exactly on a subtree's first instruction.
- 1-byte right offsets (`0x10–0x22`) cap the left subtree at 255 bytes; use `0x23–0x25` beyond that; its
  BE16 offsets cap it at 65,535, beyond which the RIGHT offset must go through a JUMP24 (`0x07`, 24-bit).
  Jumps `0x05`/`0x06`/`0x07` carry 8/16/24-bit offsets.
- Node AABBs must **conservatively enclose** all descendant triangles so runtime overlap queries never miss.
- Emit raw little-endian-on-disk u8 (**no u32 swap** — that scramble is what corrupted `old_mopp.bin`).

## Open (non-blocking for an MVP encoder)
- Exact plane geometry of `0x13–0x1c` (26-DOP diagonal splits) — the MVP avoids them (axis splits only).
- Property (`0x60–0x6b`) semantics — application-defined per-terminal metadata; irrelevant to triangle indices.
- Whether absent key 32 is a degenerate-tri cull vs a sibling-shape key — needs the source `WpMeshShape16` tri count.
- `m_info` scale/offset for the specific `old_mopp.bin` sample (the file is only `m_data`).
