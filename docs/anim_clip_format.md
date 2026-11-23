# Animation asset container (`animation`, type 16)

The on-disk shape of a Mercenaries 2 (PC) `animation` asset: ASET type id **16**, UCFX entry type
hash **`0x18166555`** (`pandemic_hash_m2("animation")`). This page is the open description of the
container and of its `evnt` (event) chunk. Everything here was measured over retail `vz.wad` and is
enforced by a byte-identical round trip; the section [What is not known](#what-is-not-known) lists
what was looked for and not found.

Code: [`mercs2_formats::anim_container`](../crates/mercs2_formats/src/anim_container.rs)
(reader/writer), proven by
[`tests/anim_container_roundtrip.rs`](../crates/mercs2_formats/tests/anim_container_roundtrip.rs).
The track binding (`trnm`) is covered in more depth in
[`reverse_engineer/animation_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/animation_code_map.md) §1 and
`mercs2_formats::animgroup`.

## Census (retail `vz.wad`)

| what | count |
|---|---|
| ASET type-16 rows | 4,261, every one `secondary_ref = 0xFFFFFFFF`, `packed_block_ref & 0xFFFF = 0xFFFF` (primary, no LOD rungs) |
| blocks carrying them | 191 |
| Havok clip containers, `info,data,trnm` | 1,969 |
| Havok clip containers, `info,data,trnm,evnt` | 2,263 |
| `MANM` keyframe containers | 29 |
| events across all `evnt` chunks | 9,645 |

All 4,261 containers parse with the strict reader below and rebuild **byte-identically** from their
own chunks; all 2,263 `evnt` chunks decode and re-encode byte-identically.

## The container

Every `animation` container is a **packed leaf UCFX container**:

```text
+0   "UCFX"
+4   u32 data_area_off = 20 + 20·n
+8   u32 0
+12  u32 0
+16  u32 n                       descriptor count
+20  n × 20-byte rows            [tag(4)] [u32 off] [u32 size] [u32 n-1-k] [u32 0]   (k = row index)
     data area                   bodies in row order, packed, no padding:
                                 row k's off = Σ size of rows 0..k-1
     "CSUM" u32                  CRC-32 over everything before "CSUM"
```

- `off` is relative to `data_area_off`. No row is a nested-container sentinel (`off = 0xFFFFFFFF`).
- The container is exactly `data_area_off + Σ size + 8` bytes.
- The checksum is the UCFX `CSUM`: CRC-32, reflected polynomial `0xEDB88320`, **init 0, no final
  XOR** ([`format_reference.md` §4.0](format_reference.md#40-csum-trailer-per-chunk-integrity)).
- The word at row `+12` counts the rows after this one (`n-1-k`); `+16` is always 0.

A writer that pads bodies to an alignment (as `fxdict::write_ucfx_container` does) does **not**
produce this shape.

In the block, the entry-table row is `(name_hash, 0x18166555, 0, container_size)`, and the asset's
ASET row is `(name_hash, 0xFFFFFFFF, 0x????FFFF, 16)` — the high half of the third word is the block
index.

## Havok clip: `info`, `data`, `trnm`, `evnt`

| chunk | body |
|---|---|
| `info` | two bytes, `01 00`, in all 4,232 clips |
| `data` | a Havok 5.5.0-r1 32-bit packfile (magic `57 E0 E0 57 10 C0 C0 10`) holding one `hkaAnimationContainer` and one `hka*SkeletalAnimation` |
| `trnm` | `[u16 count][u16 flags][u32 lead][count × u32 HIER bone name-hash]` — size is exactly `8 + 4·count` |
| `evnt` | optional; see below |

`trnm`'s `count` equals the packfile's `hkaAnimation::numTransformTracks` for **every** retail clip.
The `flags` half is `0x0000` on most clips and `0xFFFF` on a whole class of them (e.g. the 50-track
clips); read `count` as the low 16 bits only. The `lead` word is not a transform track's bone.

## `evnt` — animation events

```text
[u32 count]
count × {
    [f32 time]          seconds from the start of the clip
    [name]              ASCII, NUL-terminated (may be empty)
    [category]          ASCII, NUL-terminated (may be empty)
}
```

No padding, no alignment, nothing after the last event. Little-endian on PC; the console bake stores
`count` and `time` big-endian and the strings unchanged (the converter swaps exactly those two words
per event).

Example — one event at 0.2 s named `opendoor` with no category (18 bytes):

```text
01 00 00 00  CD CC 4C 3E  6F 70 65 6E 64 6F 6F 72 00  00
count = 1    0.2f         "opendoor\0"                  "\0"
```

### What retail puts in it

Names are sound cues (`ahj_footstep_solidmetal`, `fol_rustle_human`, `sfx_fol_punch_hard`), voice
lines (`vo_mat_chat_pain_heavy`), gameplay markers (`opendoor`, `ahj_foot_contact`) and animation
cues (`anim_laugh`). Categories across the 9,645 events:

| category | events |
|---|---|
| `sound` | 4,435 |
| *(empty)* | 2,795 |
| `sound_surface` | 843 |
| `vo` | 806 |
| `camera` | 642 |
| `sound_weapon` | 89 |
| `magazineA` | 31 |
| `sound_suface` | 4 (spelled so in retail) |

### Invariants retail holds

- Every time is a finite, non-negative `f32`.
- Events are listed in **non-decreasing time order**.
- Times are **not** bounded by the clip's duration: 35 events in 13 clips fire after the clip's
  `hkaAnimation::duration` (block 3272, clip `0x62991523`, is 2.17 s long and carries footsteps out
  to 3.97 s). A writer must not reject that.
- Every string is ASCII; none contains a NUL.

## `MANM` keyframe animations

29 of the type-16 assets are not Havok clips. Their containers are the same packed shape but carry a
`MANM` record followed by `MINF` / `TRCK` groups (`MANM,MINF,TRCK,…`, up to 37 chunks). Across all
29, `MANM` is always 16 bytes and `MINF` always 6; `TRCK` is 18 to 54 bytes (18 + 4·k for
k ∈ {0…7, 9}). These are the Pandemic keyframe (non-Havok) animation
records the tag registry lists — `MANM` a name→clip record, `MINF` `[u32 hash][u16]` records binding
meshes to clips, `TRCK` a 12-byte header plus parallel arrays
([`ucfx_tag_registry.md` §8](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ucfx_tag_registry.md#8-animation-cluster)). The reader and writer round
trip them byte-identically as containers; their field semantics beyond the registry are not decoded,
so nothing authors one.

## What is not known

- **The engine's `evnt` and `trnm` readers are not located.** Searched: the RE docs (`evnt` appears
  only in [`ucfx_tag_registry.md` §10](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ucfx_tag_registry.md#10-registry-gaps--follow-ups), as a
  converter-known tag whose handler is a follow-up) and `output/_ghidra/mercs2_unpacked.exe_decomp.txt`
  for the FourCC as a little- or big-endian immediate (`0x746E7665`, `0x65766E74`) and as text
  (`evnt`, `trnm`) — no hits. So what the engine *does* with an event (which system consumes a
  `category`, whether a name is hashed or matched as text, what happens to an event past the clip's
  end) is not established; the layout above is established by the byte-identical round trip only.
- **A novel clip's in-game behaviour** — that a container built this way plays, and that its events
  fire — has not been observed in the running game.
