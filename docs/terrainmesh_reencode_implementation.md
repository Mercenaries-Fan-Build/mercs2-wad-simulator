---
status: current
evidence: proven
verified_on: 2026-07-21
witness: "Re-measured end-to-end against retail bytes. Fixture provenance: `terrain_provenance` resolves 0xCA67E07B/0xD0E2F48D/0x81348F94 through ASET to blocks 756/757/758 (`blocks\\vz\\c30010..12_P000_Q3.block`) in BOTH game-files/vz.wad and game-files/xbox-vz.wad and matches all six `output/_scratch/terrain_A/*_{be,pc}.bin` fixtures byte-for-byte (common_prefix == full length). Claims re-derived with `cargo run --release -p ucfx_byteswap --bin terrain_audit -- output/_scratch/terrain_A`, which re-runs the CURRENT converter over the BE fixtures. Confirmed exactly: sizes 1248592→1419036 / 974880→1110508 / 911644→1021982; dao=6640 ndesc=331; stride rule 78/78; POSITION per-u16-swap 53142/53142; FLOAT16_2 10430/10430; chunkdiff info byte-eq 1→78, data size-eq 1→78, decl 78/78, POFF 48/48, INFO 129 size-eq / 48 byte-eq, MTRL first-diff 236→568, info 87.8% (1096/1248), INFO 87.7% (4947/5640); container +183034/+119990/+178040; NORMAL 11-11-10 mean 0.0463° max 0.1251° over 53142 pairs; byte-exact ≤1.22%; `--strict` on real BE block 756 exits 0; 400 terrainmesh entries in each WAD. Two prose defects corrected below (TANGENT bit split, STRM byte-exact %)."
supersedes: [docs/terrainmesh_reencode_investigation_A.md, docs/terrainmesh_reencode_investigation_B.md]
# NOTE: this file has CRLF endings, and knowledge.js::splitFrontMatter cuts `raw` at the
# `\n` of the closing `\r\n---`, so the LAST front-matter line keeps a trailing `\r` and
# fails the key regex. Keep a comment last so no real key is the one that gets dropped.
---

# Terrainmesh (mesh_A / `0x7C569307`) Xbox 360 → PC re-encode — implementation

Implements the converged plan from `terrainmesh_reencode_investigation_A.md` and
`…_B.md`. The transform lives in `tools/wad_simulator/crates/ucfx_byteswap/src/convert.rs`,
gated on `type_hash == TYPE_HASH_TERRAIN_MESH (0x7C569307)`, in a new post-pass
`apply_terrainmesh_reencode` (runs last in `convert_container`, after the generic
body sweep, `apply_decl_translate`, and `apply_strm_vertex_fix`).

PC retail terrainmesh is a **genuine re-encode** (+170 KB / +136 KB / +110 KB on the
three worked meshes — "~+170 KB" is the largest, not the typical), not a
byte-swap. The pass rebuilds the container's data area chunk-by-chunk and reframes
every descriptor offset (PC bodies are contiguous, zero-gap, no alignment pad —
verified across the worked corpus, so a simple concatenation reproduces the shape).

## The DEC3N normal — pinned empirically

> **CORRECTION (2026-07-21) — the bit split is per-USAGE, not one layout.**
> This section says "the Xbox 4-byte packed NORMAL/TANGENT is 11-11-10". That is
> right for **NORMAL** and **wrong for TANGENT/BINORMAL**, which are **10-10-10**
> (`D3DDECLTYPE_DEC3N` proper: X=sx(bits[0:10])/511, Y=sx(bits[10:20])/511,
> Z=sx(bits[20:30])/511, top 2 bits = W, ignored). Measured over the whole worked
> corpus (78 STRM groups, 63,572 packed→FLOAT16_4 pairs):
>
> | decl usage | n | 11-11-10 mean / max | 10-10-10 mean / max |
> |---|---|---|---|
> | 3 = NORMAL  | 53,142 | **0.0463° / 0.1251°** | 105.10° / 180.0° |
> | 6 = TANGENT | 10,430 | 93.19° / 179.46° | **0.0685° / 0.1545°** |
>
> Using 11-11-10 on a tangent is not a rounding error, it is garbage. **The shipped
> code is already correct** — `dec3n_to_half4_le(u, ten_ten_ten)` in `convert.rs`
> selects 10-10-10 for `usage != 3`; only this prose is stale. Note also that this
> section's own worked example proves it: `0x613ffb79 → (−0.2649, −0.00392,
> −0.9644)` (quoted in investigation A §4.2) is the *tangent* slot of the stride-32
> vertex, and only the 10-10-10 split reproduces it (−0.264 / −0.0039 / −0.9648).
> Reproduce: `cargo run --release -p ucfx_byteswap --bin terrain_audit --
> output/_scratch/terrain_A` → "per-usage angular error, 11-11-10 vs 10-10-10".

The Xbox 4-byte packed NORMAL is **11-11-10 signed-normalized** (the Xbox
360 `D3DDECLTYPE_DEC3N` / "HEND3N" variant), read from the raw big-endian u32:

```
X = sign_extend(bits[0:11])  / 1023        (11-bit signed)
Y = sign_extend(bits[11:22]) / 1023        (11-bit signed)
Z = sign_extend(bits[22:32]) / 511         (10-bit signed)
W = 1.0
```

then the vector is normalized and each component re-encoded to IEEE-754 binary16.

**Evidence.** Extracted 24,282 clean `(raw-Xbox-BE-u32 ↔ PC-FLOAT16_4)` NORMAL
pairs from the three worked terrainmeshes (`output/_scratch/terrain_A/`, scripts
`dec3n_extract.py` / `dec3n_solve3.py` / `dec3n_verify.py`). A per-axis brute force
over `(start, width, divisor, sign)` found this layout with per-axis RMS
≈ 0.0005. Worked decodes (raw BE u32 → unit normal):

| Xbox u32 | decoded normal | PC normal (f16) |
|---|---|---|
| `0x001ff007` | (0.0068, 1.000, 0.002) | (0.0076, 1.000, 0.0004) |
| `0x055ff007` | (0.0068, 0.999, 0.041) | (0.0077, 0.999, 0.0421) |
| `0x1c5c4659` | (−0.413, 0.883, 0.221) | (−0.4131, 0.8833, 0.2216) |

The decode is **geometry-correct, not byte-exact**: PC's FLOAT16_4 normals were
quantized from the original high-precision source mesh, so they differ from this
lossy-Xbox decode by ≤1–2 half-float ULP. Across all 42,712 NORMAL/TANGENT samples
the **mean angular error is 0.05°, max 0.124°** — the direction is reproduced to
<!-- CORRECTION (2026-07-21): see the note below on what 42,712 actually counts. -->
well within lighting tolerance, but the exact half-float bytes are not invertible
from the lossy 4-byte Xbox value (confirmed: best byte-exact rate by any decode +
normalization combo is ≤2%). There was **no pre-existing DEC3N decoder** in the
repo (`grep -rn DEC3N tools/` → only a passing comment in `convert.rs`).

> **CORRECTION (2026-07-21) — the sample counts, and why the tangent bug hid here.**
> The corpus contains **63,572** packed→FLOAT16_4 pairs (53,142 NORMAL + 10,430
> TANGENT) and **27,734 distinct** `(BE u32 → PC f16x4)` NORMAL pairs — not 24,282.
> **42,712 is exactly the number of stride-16→20 ("Type A") vertices**, i.e. the
> class that carries a NORMAL and *no* tangent. So the survey that produced
> "0.05° / 0.124°" sampled **zero tangents**, which is precisely why the prose above
> generalised the NORMAL layout to TANGENT. Re-measured on the true NORMAL
> population (53,142): mean **0.0463°**, max **0.1251°** — the quoted figures hold.
> Confirmed: "best byte-exact rate by any decode is ≤2%" → measured **1.18%**
> (11-11-10 everywhere) / **1.22%** (correct per-usage split), 751 resp. 773 of 63,572.

Implemented in `dec3n_to_half4_le` + `f32_to_f16_bits` (round-to-nearest-even,
matches `f32 as f16`). Unit tests: `dec3n_decodes_known_normals`,
`f16_encode_roundtrip_matches_std`.

## Per-chunk transforms

| chunk | transform | result |
|---|---|---|
| `decl`, `POFF`, GEOM `INFO` (clean words) | unchanged (already byte-exact) | **byte-exact** |
| STRM `info` (12B) | rewrite stride field: `pc_stride = be_stride + 4×(#FLOAT16_4 in decl)` | **byte-exact** |
| STRM `data` — POSITION (FLOAT16_4 @0) | undo the generic u32-swap = swap each u16 half | **byte-exact** |
| STRM `data` — FLOAT16_2 | same u16-half undo | **byte-exact** |
| STRM `data` — D3DCOLOR/UBYTE4 | copy the generic-swapped bytes verbatim (the generic u32-reverse already equals PC component order) | **byte-exact** |
| STRM `data` — NORMAL/TANGENT (FLOAT16_4) | DEC3N 4B → 8B widen (above) | **geometry-correct** (~0.05°), not byte-exact |
| IBUF `data` | de-strip: Xbox tri-strip w/ `0xFFFF` restart → tri-list (drop degenerates, alternating winding) | **geometry-correct**, not byte-exact |
| IBUF `info` (4B) | rewrite to the new (de-stripped) index count | follows the de-strip |
| MTRL | per-record count-pair walker (fix transposed `[count][0]`→`[0][count]`) | **engine-correct**, not byte-exact (PC also shrinks) |

`rebuild_terrain_vertices` drives the vertex widening off the (already
PC-translated) `decl`: position prefix (8B) + each declared element, emitting the
wider PC stride. `destrip_indices` does the strip→list expansion.
`mtrl_fix_transposed_counts` walks material records (stride `116 + count*4`, count
pair at `record+104`) and restores the count pair the generic `convert_mtrl` only
fixed for record 0.

### Verified per-offset agreement (worked asset `0xCA67E07B`, STRM #1, stride 20)

All 1073 vertices: offsets 0–7 (POSITION) and 8–11 (D3DCOLOR) are **100% byte-exact**;
**every** differing byte is confined to offsets 12–19 (the FLOAT16_4 normal), and
even the W-half (18–19) matches. STRM vertex buffers are 84% byte-exact overall
(the 16% is exactly the lossy normal field).

> **CORRECTION (2026-07-21) — 84% is stale; measured 87.2%, and the per-offset claim
> generalises.** Re-measured at HEAD over **all 42,712** stride-20 vertices in the
> three assets (not just 1073): diff counts per byte offset are
> `[0..11] = 0`, `[12]=37650 [13]=3275 [14]=14374 [15]=102 [16]=37668 [17]=3276`,
> `[18]=[19]=0`. So POSITION (0–7), D3DCOLOR (8–11) and the normal's W-half (18–19)
> are byte-exact across the whole corpus, and every diff is in bytes 12–17 — the
> stronger form of the claim above. Overall STRM `data` byte agreement vs PC is
> **87.18%** (1,035,723 / 1,188,000), not 84%: 88.7% for the stride-20 class and
> 83.2% for the stride-32 class. The 84–84.5% figures predate the TANGENT
> 10-10-10 fix (a garbled tangent differs in more bytes than a 1-ULP one).
> Reproduce: `terrain_audit` → "NEWCONV STRM data byte agreement vs PC".

## Oracle results (before → after)

Whole-asset SHA (the oracle's pass/fail gate):

```
mesh_A:  400 PC-only / 0 compared   →   400 compared, 400 MISMATCH, 0 crashes/skips
```

The whole-asset count stays at 400 **mismatch** because three independent
proprietary codecs are inherently non-byte-exact (normals, index re-strip, MTRL
shrink), and the oracle compares the full payload SHA — so a single lossy byte
fails the whole asset. The real, measurable progress is **per-chunk**, on the three
worked assets (`output/_scratch/terrain_A/chunkdiff.py`):

| tag | n | size-eq (before→after) | byte-eq (before→after) | byte-agreement on size-eq |
|---|---|---|---|---|
| `decl` | 78 | 78 → 78 | 78 → 78 | 100% |
| `POFF` | 48 | 48 → 48 | 48 → 48 | 100% |
| `info` (STRM 12B + IBUF 4B) | 156 | 156 → 156 | 1 → **78** | 87.8% |
| `data` (STRM + IBUF) | 156 | **1 → 78** | 0 → 0 | STRM data **84.5%** byte-exact (rest = lossy normal) |
| `MTRL` | 3 | 0 → 0 | 0 → 0 | first-diff 236 → **568** (records 0–3 counts fixed) |
| `INFO` (32/44B) | 129 | 129 → 129 | 48 → 48 | 87.7% |

> **Re-verified 2026-07-21 (all rows above reproduce exactly).** Independently
> re-measured by parsing the descriptor tables of `{be,pc}` and of a *fresh* run of
> the current converter over the BE fixtures: `decl` 78→78 byte-eq, `POFF` 48→48,
> `info` byte-eq **1 → 78**, `data` size-eq **1 → 78**, `INFO` 129 size-eq / 48
> byte-eq, MTRL first-diff **236 → 568** (CA67E07B), and the byte-agreement
> percentages land on the same numbers: `info` **87.8%** (1096/1248), `INFO`
> **87.7%** (4947/5640). The "PC bodies are contiguous, zero-gap, no alignment pad"
> premise also holds — 0 gaps / 0 overlap / 0 tail bytes on all three PC containers
> (and on the BE ones, so it is not a PC-only property).
> Two footnotes: MTRL first-diff is **0**, not 236, for `0x81348F94` (its very first
> u32 is a different hash), so the 236→568 progression is a CA67E07B fact; and the
> INFO follow-up below is exact — the differing u32 words are `{0,2,3,4,6,9,10}`,
> word 7 never differs in this corpus, and the root INFO's word 6 really is
> **304 → 267** (`0x130` → `0x10b`).

Container size (conv vs PC): `+183 KB` / `+120 KB` / `+178 KB` for the three assets —
larger than PC because the de-stripped triangle **list** carries more indices than
PC's single degenerate-stitched strip (see below). The output is structurally valid
(`--strict` validation passes, exit 0) and engine-loadable.

> **Re-verified 2026-07-21.** Container deltas measured exactly: **+183,034 /
> +119,990 / +178,040** bytes. `--strict` reproduced end-to-end on the *real* Xbox
> block (not a fixture): decompress base block 756 from `game-files/xbox-vz.wad`
> (BE `segs`, 1,015,808 → 1,866,534 B) and run
> `ucfx_byteswap be_block_756.bin -o le_block_756.bin --strict` → "Validation: OK
> (all checks passed)", 2,198,172 B written, **exit 0**. The oracle's population of
> **400** is also confirmed: both WADs contain exactly 400 entries with
> `type_hash == 0x7C569307` (`terrain_count`). The "400 MISMATCH" verdict itself was
> not re-run here, but it is forced: no asset can pass a whole-payload SHA while the
> normal codec is ≤1.22% byte-exact, the index stream is structurally different, and
> MTRL is a different size. **"engine-loadable" is untested in this audit** — nothing
> here booted the game.

## What is byte-exact vs geometry-correct vs unsolved

- **Byte-exact:** `decl`, `POFF`, STRM `info` (stride), and the POSITION /
  FLOAT16_2 / D3DCOLOR vertex elements of STRM `data`.
- **Geometry-correct (not byte-exact):**
  - **NORMAL/TANGENT** — DEC3N decode is direction-correct to 0.05°; PC's exact
    half-floats come from the lost source-mesh precision (not invertible).
  - **IBUF indices** — a correct triangle list of the de-stripped geometry. PC
    instead emits a **single degenerate-triangle-stitched strip with a
    vertex-cache re-index** (the triangle *sets* don't even match: PC has fewer,
    re-indexed triangles). Reproducing PC byte-for-byte needs replicating its
    mesh-compiler stitch + cache optimizer (e.g. Forsyth / D3DXOptimizeFaces).

> **CORRECTION (2026-07-21) — "stitched strip" confirmed, "vertex-cache re-index"
> is WRONG.** PC really is a strip, not a list: 41 of the 78 index buffers in the
> worked corpus have an index count **not divisible by 3** (e.g. 10,723 / 10,813 /
> 8,761), and ~70% of chunk-aligned triples are degenerate — impossible for a
> triangle list. But there is **no vertex re-index**: the set of index *values* used
> by PC equals the Xbox set in **78/78** buffers, and the vertex buffers themselves
> are in the same order (POSITION bytes match 1:1 for all 53,142 vertices). In
> **32/78** buffers the PC stream, with consecutive duplicates collapsed, is
> *literally* the Xbox strip's non-restart index sequence — e.g. `81348F94` IBUF#12:
> Xbox `0,1,2 |FFFF| 3,4,5,1 |FFFF| 6,7,8` → PC `0,1,2, 2,3,3, 3,4,5,1, 1,6,6, 7,8`.
> PC replaces the `0xFFFF` primitive-restart (unavailable on D3D9 PC) with repeated
> boundary indices. What is left to reverse is the **stitch/parity rule and the
> triangle ordering**, not a cache optimizer over re-numbered vertices.
> Reproduce: `terrain_audit` → "IBUF collapse-dupes seq match / index-value-SET match".
- **Engine-correct but not byte-exact:**
  - **MTRL** — the per-record count-pair transposition is fixed (records walk via
    stride `116 + count*4`); but PC additionally **drops sub-records / shrinks the
    body ~14.7 KB**, which is not reproducible from the Xbox data.

## Follow-ups / not done

- **PRMG/GEOM `INFO` (44B) count words** — words 4, 10 (and sometimes 7) are
  transposed `[u16][u16]` pairs (a real generic-u32-swap bug, value-preserved and
  field-fixable); words 0, 2, 3, 6, 9 are **geometry-recomputed counts** (e.g. the
  root INFO's 304→267) that depend on the non-byte-exact index re-encode and so
  can't be made byte-exact. Left as the generic u32 swap (engine-loadable) — fixing
  only the two clean u16-pair words yields no whole-chunk byte-exactness and adds
  risk, so it was deferred.
- **PRMT** — 20-byte draw-call records; PC drops/merges records (shrinks ~5 KB).
  Left as the existing u16 swap.
- **Index re-strip to match PC byte-for-byte** — would require the PC mesh
  compiler's stitch + vertex-cache re-index; out of scope (geometry-correct list
  is emitted instead).

## Reproduction

```
# build
cd tools/wad_simulator && cargo build --release -p ucfx_byteswap && cargo test -p ucfx_byteswap

# chunk-level diff on the worked assets (fast)
cd output/_scratch/terrain_A && python chunkdiff.py

# DEC3N pinning evidence
python dec3n_extract.py CA67E07B   # extract (BE u32 ↔ PC f16x4) normal pairs
python dec3n_solve3.py             # per-axis bit-layout brute force
python dec3n_verify.py             # byte-exact / angular-error survey

# 2026-07-21 audit: re-derive every number above from the retail bytes, in Rust.
# (`terrain_audit` re-runs the CURRENT converter over the BE fixtures, so it also
#  re-checks the "after" column of the chunkdiff table.)
cd tools/wad_simulator
cargo run --release -p ucfx_byteswap --bin terrain_audit -- ../../output/_scratch/terrain_A
# fixture provenance — resolve the assets through ASET in BOTH WADs and prove the
# fixtures are the retail entry bodies byte-for-byte:
cargo run --release -p ucfx_byteswap --bin terrain_provenance -- \
    ../../game-files/vz.wad      ../../output/_scratch/terrain_A _pc.bin CA67E07B D0E2F48D 81348F94
cargo run --release -p ucfx_byteswap --bin terrain_provenance -- \
    ../../game-files/xbox-vz.wad ../../output/_scratch/terrain_A _be.bin CA67E07B D0E2F48D 81348F94

# full oracle (whole WAD; uses the main repo's game-files)
python tools/wad_be_le_oracle.py --converter rust --type 0x7C569307 --jobs 8 \
    --xbox-wad game-files/xbox-vz.wad --pc-wad game-files/vz.wad \
    --out-dir output/_scratch/rosetta_terrain_impl
```
