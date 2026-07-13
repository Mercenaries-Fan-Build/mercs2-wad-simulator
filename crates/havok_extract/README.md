# havok_extract

Stage-2 Havok collision extractor: an exact little-endian packfile decode that replaces the
heuristic `tools/havok_extractor.py`.

## What it is

A single binary (no library target). It takes an arbitrary blob — a decompressed WAD block or a
model container body — scans it for every embedded Havok packfile, and writes, per packfile:

* `havok_NNNN_Havok.bin` — the packfile's real byte range, sliced at its true size (taken from the
  section headers, not a fixed byte cap).
* `convex_hull_NNNN.obj` — one OBJ per `hkpConvexVerticesShape`, only with `--emit-convex-obj`.
  Vertices are the exact decoded hull vertices; faces are reconstructed from the plane equations
  (verts on each plane, ordered around the plane normal, fan-triangulated) and are advisory for
  viewing — the vertices are the authoritative decode.
* `manifest.json` — schema `mercs2_havok/2`, one `havok_slices[]` entry per packfile with its
  `offset`, `size_written`, Havok `version`, per-class instance census (`class_counts`), and a
  `shapes[]` array. Shapes are tagged `convex` (with `vertices` + `planes`), `box` (with
  `half_extents`), `mopp`, `mesh`, or `other` (with the class name).

All the actual parsing lives in [`mercs2_formats::havok`]; this crate is the CLI + OBJ/JSON
emission around it.

## Where it comes from

PC-retail `PHY2` collision data — Havok 5.5.0-r1 32-bit little-endian packfiles as they ship inside
the game's blocks. The decoder (`mercs2_formats::havok`) walks the packfile structure properly:
section headers → virtual fixups (object → class name) → local fixups (pointer relocation) → class
instances, and pulls `hkpConvexVerticesShape` vertices (FourVectors SoA) and plane equations out of
the resolved `hkArray`s.

The predecessor, `tools/havok_extractor.py`, sliced Havok regions at a fixed 256 KiB cap and guessed
hulls with a `longest_vec3_run` byte scan, producing denormal-garbage vertices. This binary is the
drop-in replacement in `scripts/stage2_parallel.sh` (`HAVOK_BIN`), with the Python tool kept only as
a fallback when the binary isn't built.

## Usage

```
cargo run --release -p havok_extract -- <blob> --out-dir <dir> [--emit-convex-obj]
```

Real invocation, as the stage-2 pipeline runs it:

```
./target/release/havok_extract block_1234.bin --out-dir out/havok_1234 --emit-convex-obj
```

prints e.g. `havok_extract: 1 packfile(s), 6 convex hull(s) from block_1234.bin` and fills
`out/havok_1234/` with the `.bin` slices, hull OBJs and `manifest.json`.

CLI:

| arg | meaning |
| --- | --- |
| `<blob>` (positional, required) | file to scan for Havok packfiles |
| `--out-dir <dir>` (required) | output directory, created if absent |
| `--emit-convex-obj` | also write `convex_hull_NNNN.obj` per hull |
| `--max-len <n>` | **accepted and ignored** — kept for CLI parity with the Python tool; the real packfile size from the section headers is always used |

Exit codes: `0` ok, `1` I/O error, `2` bad usage.

## Notes / gotchas

* `manifest.json` always names the first hull of a packfile in `convex_hull_filename` /
  `has_convex_hull`, even without `--emit-convex-obj` — in that case the per-shape `obj` field is
  `null` and no OBJ file is written.
* Hull vertices are inset from their planes by the shape's convex radius, so face selection picks
  the verts at each plane's *maximum* `n·v + w` (radius-agnostic) rather than at 0.
* The packfile magic is *searched for*, not assumed at offset 0 (there is a u32 prefix), and
  overlapping magics inside an already-parsed packfile are skipped.
* `publish = false` — this crate is excluded from the crates.io release set.
