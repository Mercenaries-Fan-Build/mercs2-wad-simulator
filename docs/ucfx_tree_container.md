# The UCFX tree container

**Status:** specification. Every rule below was checked by re-encoding retail data byte for byte:
all 314 effect containers and the resident `fxdict` container in PC `vz.wad`
(`mercs2_formats/tests/effect_retail_roundtrip.rs`), and all 1,311 destruction families
(`mercs2_formats/tests/state_machine_roundtrip_survey.rs`, for the `x2` rule).

**Implementation:** `mercs2_formats::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode}`.

A UCFX container is a **tree** flattened in pre-order into a table of 20-byte rows, followed by the
row bodies and a checksum. This page specifies the flattening. What the tags mean is per asset type
(see [`effect_container_format.md`](effect_container_format.md) for effects).

---

## 1. Layout

All integers are little-endian u32 on PC.

```text
+0    "UCFX"
+4    data_area_off  = 20 + 20·n
+8    0
+12   0
+16   n               (row count)
+20   n × row         { tag, rel_off, size, x2, x3 }
      data area       the bodies, in row order, no gaps, no padding
      "CSUM" crc      8-byte trailer
```

### 1.1 Row

| Offset | Field | Meaning |
|---|---|---|
| +0 | `tag` | four bytes, compared as a u32 by the loaders |
| +4 | `rel_off` | body offset from `data_area_off`; `0xFFFFFFFF` for a marker row |
| +8 | `size` | body length in bytes; `0` for a marker row |
| +12 | `x2` | **reverse sibling ordinal**: how many siblings follow this row at its own level (the last child has `x2 = 0`) |
| +16 | `x3` | **descendant count**: rows in this row's subtree, not counting itself |

### 1.2 Tree rules

1. **Pre-order.** A row's children are the rows `i+1 ..= i+x3`. They are walked sibling to
   sibling with `next = i + x3 + 1`. This is the loop the effect loader runs
   (`FUN_00491920`, `mercs2_unpacked.exe`: `iVar9 = *(row + 0x10) + 1 + iVar6`), guarded by
   `x2 != 0` for "has a next sibling" and by `x3 != 0` for "has a first child" (`iVar6 + 1`).
2. **`x2` is derived:** the number of rows that follow at the same level. It is never stored
   independently of the tree.
3. **`x3` is derived:** the subtree row count.
4. **Marker rows** own no bytes: `rel_off = 0xFFFFFFFF`, `size = 0`. A marker is a pure grouping node
   (the effect `EMIT`, the model `GEOM`, ECS `COMP`). A marker may have children.
5. **A row with a body may also have children** (the effect `EFCT`, `PTYP`, `FRCE`, `ATRB` do).
6. **Bodies are contiguous in row order.** The first body is at `rel_off = 0`; each next body
   starts where the previous one ended. There is no alignment padding. A zero-length body is legal
   and occupies no bytes.
7. **The top level may be a forest.** The `fxdict` container has two top-level rows (`INFO`,
   `DICT` with `x2 = 1, 0`); an effect has one (`EFCT`).

### 1.3 Checksum

`CSUM` then a u32: CRC-32 with init 0, reflected polynomial `0xEDB88320`, no final XOR, over every
byte before the `CSUM` tag (`mercs2_formats::crc32::crc32_mercs2`). Any rewrite recomputes it.

---

## 2. Reading and writing

A **writer** emits the rows in pre-order, computes `x2`/`x3`, writes marker rows for body-less
nodes, lays the bodies out contiguously, and appends the checksum.

A **strict reader** rejects anything that writer would not produce: a wrong `data_area_off`, a
non-zero header word at `+8`/`+12`, an `x2` or `x3` that disagrees with the tree, a marker with a
size, a body that does not start where the previous one ended, trailing bytes, or a bad checksum.
With that rule a successful parse guarantees a lossless re-write.

## 3. Relation to older notes

[`ucfx_tag_registry.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ucfx_tag_registry.md) §2 already describes marker rows and the two
derived words. This page adds the rules the writer needs and that retail was measured against:
contiguous bodies without padding, the derivation of `x2` for every row (not only markers), and the
forest top level.
