# Havok Wavelet Skeletal-Animation Decode — Live-Capture Verification

**Status: SOLVED + NUMERICALLY VALIDATED (2026-07-01).**
Source of record: memory `wavelet-decode-solved-live-capture`.

The wavelet skeletal-animation decode was the long-standing blocker that made the
engine's `--animate` path explode into stretched-poly fans (formerly only ~25/64
rotations matched). It is now re-ported from the decomp and validated numerically
against a matched triple captured from the **running game**.

## Why it needed a live oracle

The Xbox sample/blend math is VMX128 and does not decompile cleanly, so the on-disk
decomp alone under-constrains the pipeline. A single output pose also under-constrains
it (many decoder bugs cancel at one frame). The fix was to capture the **intermediate
coefficient buffer** as well, splitting the pipeline into two independently-checkable
stages.

## Capture method (x32dbg on the live, unpacked process)

Break-pointed and dumped a matched `(raw clip bytes → coefficients → pose)` triple:

- `LtSampleWave` = `FUN_009f5e40` **entry** — raw quantized clip block + runtime state.
- `StRecomposeW` call site — breakpoint at **`0x9f6396`**, just after
  `call FUN_009fb870` — the intermediate coefficient buffer (`iVar5`) and the recomposed
  pose.

Capture is the 2.5673-second clip. Fixtures live at
[`tools/wad_simulator/crates/mercs2_formats/tests/fixtures/wavelet_capture_2p567s/`](../../crates/mercs2_formats/tests/fixtures/wavelet_capture_2p567s/)
(`meta.json` + `clip_data` / `coeffs_in` / `pose_out` / `header` / `mask` `.hex`).
`meta.json` records the runtime state (numPoses=78, DOFcount=246, blockSize=8, …).

## Validation results (real numbers)

| Stage | Gate | Result |
|-------|------|--------|
| Stage 2 — `StRecomposeW` | `tests/wavelet_recompose.rs` | **660/660** pose floats ≤ 1e-4 |
| Stage 1 — decompress + dequant + wavelet + interp | `tests/wavelet_decompress.rs` | **246/246** coeffs ≤ 1e-3 |
| 64-track oracle clip | rotations | **64/64** |
| Whole crate | `cargo test -p mercs2_formats` | 168/168 |

## The four decode bugs (all fixed in `mercs2_formats/src/anim.rs`)

1. **frame/frac split treated `framepos` as frame units — it is TIME in seconds.**
   `g = (numPoses-1) · time / duration; frame = ROUND(g)`. (Ghidra mistyped the duration
   f32 as an int.)
2. **`wv_entropy_advance` used `bw·n`, not `bw·present`.** `present` = count of
   stream-read / non-fill codes only; using the full `n` drifts the per-DOF quant pointer.
   The re-port makes `wv_entropy_unpack` return `(codes, fill_bitmap)` so advance counts
   only present codes.
3. **`wv_dequant` had a spurious `+0.5` rounding bias and a `dc_mode` sign-flip hack.**
   Live `_DAT_00bea940 = 0.0`. Correct: `code·(2^(-bw)·mult) + off`, no hack.
4. **`mult` / `addend` arrays were swapped.** On-disk `scale_idx` (QFMT+12) = the
   multiplier (runtime obj+0x38); `offset_idx` (QFMT+8) = the additive offset (obj+0x34).
   The swap took the oracle clip from 19/64 → 64/64.

W-sentinel quats reconstruct via `sqrt(1 - x² - y² - z²)` with `_DAT_00b6b6b8 = 2.0`.
`StRecomposeW` leaves the translation-w lane (component 3) untouched in the non-identity
path (prior `StStaticW` value).

## What is NOT yet verified

- **`blockSize ≠ 8`** and **`preserved > 0`** decode paths — no fixture exercises them.
- **Delta-compressed clips** — the decoder is header-only (see
  `skinning_animation_spec.md` §5 open item #3). Census the animation `type` byte across
  all animgroup Havok slices before assuming everything is wavelet.

## Relationship to the "warped hands" that persisted after this fix

The residual hand/finger deformation seen after the decode was fixed was **not** a decode
error — it was the per-group BLENDINDICES bug, since fixed. See
`skinning_animation_spec.md` §1.4 and memory `blendindices-per-group-palette`. The
compose side (sampleAndCombine / hkQsTransform) was verified separately via the
`--poseoracle` isolation test.
