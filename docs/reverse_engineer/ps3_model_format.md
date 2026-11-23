# PS3 `model` chunk geometry

PS3 draws a group as segments for the SPUs, so its `model` chunks (type `0x5B724250`) hold geometry in a
form no other platform has. `mercs2_formats::be_to_le::ps3_model` converts it to PC form with no Xbox input.
All PS3 data is big-endian.

This was decoded black-box against the PC and Xbox twins of every retail model. The SPU code that reads it
was not disassembled. `mercs2_probe model-compare <ps3-VZ.WAD> <vz.wad> <out.tsv>` is the gate: it converts
every model and compares it with PC over the whole rung chain, because each platform splits rungs its own way.

## Tree

`INFO HIER MTRL [BSHP] [TINY] SEGM [PHY2] [STAM] [MIXR] GEOM`, as on PC. `GEOM` holds `INFO INDX` and
sub-objects `MESH|SKIN|TINY`, each `INFO PRMG…`. A PS3 `PRMG`:

```
INFO  STRM{info,decl,data}  SSPU  [BSHP × morphs]  IBUF  [TDTA] [BSHI]  PRMT
```

Everything outside `GEOM` is Xbox's bytes and takes the normal big-endian route. `INDX` equals PC's.

## Group `INFO` (u32 words)

| Bytes | Class | Words |
|---|---|---|
| 64 | `MESH` | draws, 0, `SSPU` bytes, `IBUF` bytes, vs, vs2, 10 floats: centre, radius, min, max |
| 32 | `SKIN` | draws, `morphs<<16 \| morph stride`, position bytes, skin bytes, morph bytes, `IBUF` bytes, vs, vs2 |
| 24 | `TINY` | draws, `SSPU` bytes, `IBUF` bytes, `TDTA` bytes, vs, vs2 |

`SKIN` and `TINY` carry no bounds. A PC `TINY` group needs them: min and max from the vertices, the centre
their midpoint, the radius the box's half-diagonal (within 0.28% of retail at p99, not bit exact).

## `PRMT`

`n` × 8 bytes, then `n` × 32 bytes, `n` being `INFO` word 0.

```
A: u16 material, u16 hole, u32 base_vertex
B: u16 sspu_bytes, u16 vertex_count, u32 sspu_offset, u16 format, u16 format, u16 index_count,
   u16 ibuf_bytes, u32 ibuf_offset, u16 e1, u16 e2, u32 e3, u32 flags
```

`flags`: bit 0 skin block, bit 1 `TINY`, bit 2 position has W. With a skin block, `e1` is the bone count,
`e2` the first bone, `e3` the offset into the block. On `TINY`, `e1` is the `TDTA` records, `e2` selects the
shader pair, `e3` the `TDTA` offset. `hole` is a function of format and index count and PC does not need it.
`base_vertex` indexes `STRM`.

## `IBUF`

One piece per draw at `ibuf_offset`, a triangle list:

```
u16 explicit_count   u16 delta_base   u16 flag_bytes   u8 delta_bits   u8 0
flag_bytes            one bit per fresh corner, MSB first: 0 next new vertex, 1 next explicit index
ceil(tris/4)          two-bit triangle codes, MSB first
ceil(n·bits/8)        explicit values, MSB first
```

```
explicit[i] = value[i] - delta_base + (i >= 8 ? explicit[i-8] : 0)
code 3: (a, b, c) = three fresh corners
code 0: (a, b, c) = (a, c, new)
code 1: (a, b, c) = (c, b, new)
code 2: (a, b, c) = (b, a, new)
```

Indices are draw-relative. A triangle's winding is PC's.

## `SSPU`

`format` and `flags` fix the record:

```
xyz      3 × half      always
w        half          flags bit 2
normal   3 bytes       format 1, 2, 3
tangent  3 bytes       format 2
uv       2 × half      format 3, which has no STRM data
```

Format 0 is `TINY` only, its normal in `STRM` as a big-endian `HEND3N`, 11:11:10 with `x` low. A vertex with
no stored W has W 1.0 on PC. A `TINY` vertex's W is its object number as a half float, from `TDTA`:
`e1` records of `{u16 first index, u16 last index, u16 object}`.

A unit vector is 3 bytes, `p:10 q:10 axis:2 hand:1 sign:1` from the top bit. `axis` is the component
recomputed as `sqrt(1-p²-q²)` with `sign` (1 positive), and `p`, `q` the other two in axis order. `hand` is a
tangent's W, 1 for +1. Each of `p` and `q` covers `±√½`. The bin's middle fits PC best,
`(v-511.5)·√2/1024`, with 1023 at `+√½`, 0 at `-√½` and 512 at 0, normalised afterwards.

`SSPU` is `[position bytes][skin bytes]`. Skin records are at `position bytes + e3`, 8 bytes a vertex,
`(weight, index)` × 4, and the `HIER` bone is `e2 + index`. Weights are PC's bytes in PC's slot order.

## `STRM`

`decl` is 4-byte rows `(type, offset, usage, components)`. `data` holds UVs, then colour bytes `r g b a` with
0xFF stored as 0xFE, or the `TINY` normal. PC colour is `b g r a`.

## What PC needs built

| Row | From |
|---|---|
| Vertex stream | Interleaved per the decl: position, UV, colour, blend indices, blend weights, normal, tangent. Morph groups add 6 streams of `TEXCOORD1..6` |
| `IBUF` | Each draw's triangles as a strip (`terrainmesh::stripify`) |
| List A | One record per draw, in order. A material with flag `0x800` is in no list A |
| List B | One record per alpha-tested material (flag `0x08`) and one for all the rest, sharing a draw's range when it is one draw |
| Palette | PS3 has one contiguous bone range per draw; PC one list per group of at most 8 ranges and 64 bones. Built from the bones the vertices use, merging the closest runs |
| Morphs | `BSHP`: per draw, `vertex_count` × 3 half deltas, each draw padded to 16 bytes. PC adds W 1.0 |
| `AREA` | One half float per strip triangle. PS3 has none. The geometric area is an approximation: median ratio 1.004 to retail, p5 0.66, p95 5.6 |
| Vertex shaders | PS3 names one family; PC has one per vertex form, by the group's flags and morphs. The table is in `ps3_model.rs` |
| Chunk table word | The bytes of the rungs before it |

## Exactness

`model-compare` over all 3,007 retail models, 15,680,631 triangles:

| | |
|---|---|
| Equal to PC in triangles, materials, positions, UVs, winding, list B, weights and bones | 2,988 |
| Differ only in list B | 16 |
| Refused | 3 |

No model differs in a triangle of list A or in a vertex's skin. The 3 refused are `0x945C0657`, whose one group
is float32 on PC and half on PS3, and `0x98C5EE9B` and `0xD0C6D545`, each with a draw of bone base `0xFFFF`
(94 vertices): how that bone is chosen is not decoded. The DLC holds neither.

Not exact, and not recoverable from PS3:

- **Normals.** 10 bits over `±√½`, normalised: per-component error p99 0.0015, angle p99 0.067°. PS3 also
  dropped length, on 59,714 retail vertices with a non-unit PC normal.
- **Tangents.** PS3 normalised PC's 8-bit source: direction error p99 0.0013, length gone.
- **Palette ranges.** A valid palette exists for every group; retail's exact ranges are not reproducible.
- **List B** of a group of several alpha-tested materials, and `AREA`, are approximations.

Whether a converted model behaves the same in the game has not been tested. In the Workshop a converted tank and
a converted skinned character render as their Xbox builds do, with slightly busier shading on curved metal.

## `scrub`

Ground cover (type `0x600B904E`, `be_to_le::ps3_scrub`) uses the same `SSPU`, `IBUF` and unit vectors. Its tree:

```
INFO   SCRB × meshes {INFO MTRL STRM SSPU IBUF PRMT}   INST   PTCH   PTMS
```

- Root `INFO` is PC's 16 bytes and the rotation count. `SCRB` `INFO` is PC's 20 bytes and the `SSPU` and `IBUF`
  sizes. `PRMT` is one row B. `MTRL`, `PTCH`, `PTMS` and the shader keys are PC's, swapped.
- **`INST`**: `table` × `{x, y, z, w}` half quaternions, `instances` × `{tx, ty, tz, scale}` halves, `instances`
  × u16 index into the table (`FUN_0027c520`, `FUN_0027d198` in `ps3-v100`). A PC instance is `R(q)·scale` beside
  the translation, `q` normalised. Scale 0 has a NaN quaternion and a zero matrix.
- **Copies**: a PC mesh is 16 copies of the tuft, `TEXCOORD.z` the copy number, `TEXCOORD` `FLOAT16_4`, W 1.0 on
  position and normal, so a stride of 24, 28 with colour, 32 with tangent. `TEXCOORD.w` is not read by any of
  the 24 scrub vertex shaders and is written 0. The strip runs over the 16 copies with degenerate joins.

`mercs2_probe scrub-compare` over all 1,026 retail containers: every kept row, every triangle by copy with its
winding, and all 1,767,683 translations equal PC.

**Rotations are lossy on PS3 and cannot be recovered from it.** Instances reuse a table entry near their own
rotation: p50 5.13°, max 10.02° from PC. Xbox's `INST` is PC's, swapped. `PTCH` keeps PS3's bytes, which are PC's
bounds, so a vertex leaves its patch's culling sphere in 23 of 30,972 retail patches, by at most 5.5 cm.
