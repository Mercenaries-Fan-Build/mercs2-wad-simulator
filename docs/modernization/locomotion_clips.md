# Mattias Locomotion Clips — Identification via Footstep Annotations

**Status: walk + run clips identified (2026-07-01).** Source of record: memory
`mattias-locomotion-clips-identified`.

## Source block

Mattias animgroup = WAD block **3154** `characternameanimgroup_mattias_P000_Q3`, **97
clips**, hash-only names (**0 / 97** resolve in `tools/rainbow_table.json` under key
`pandemic_hash_m2` — animation names were never in any harvested corpus).

## The method that worked

The decompressed block embeds Havok **ANNOTATION TRACK** event strings
(`footstep_walk` / `footstep_run`). Mapping each annotation byte-offset to the clip whose
`havok_offset` span contains it identifies the locomotion clips:

1. Find `footstep_*` byte offsets in the decompressed block.
2. Bucket them by clip `havok_offset` spans.

Tooling: `anim_patch --list` prints per-clip `hash + havok_offset`; `anim_patch --dump
<path>` writes the decompressed block. (`anim_patch` crate lives in the nested
`tools/wad_simulator` workspace.)

## Identified clips

**WALK (12, ~1.0 s 60-track cycles):**
`0x53682784`, `0xDDDCE94C`, `0x444E70F7` (0.87 s), `0x1857A825`, `0x8B48D9F6`,
`0xBBA47DC4`, `0x6E1569F6`, `0x11341769`, `0xB12F1F4F`, `0x82AE7811`, `0x68A09C91`,
`0xF66E9F67`.

**RUN (21):**
`0x867B166D`, `0xAFB2929A`, `0x5100FC54`, `0x3CCE1607`, `0x68D7ECBF`, `0x0EB9B669`,
`0x193879AD`, `0x17E5EA12`, `0x57207A06`, `0x3CB55D06`, `0xD23346BC`, `0xA72CFBEF`,
`0x35E07B11`, `0x10912665`, `0xC41DBDD1`, `0xC044FA3E`, `0x389EC5A1`, `0x4215FC79`,
`0x49478852`, `0xC49B7F23`, `0xE76E2740`, `0xFE34478E`.

Variants = directions × stances, **not** distinguishable from annotations alone; that
needs visual / RE follow-up.

## Currently wired in the engine

The TPS mode uses idle `0x24F8C8E6` + walk `0x53682784` + run `0x867B166D` (see
`world_terrain_loader.md`). Walk/run clips carry baked locomotion in the root track
(GlobalSRT, up to 1.55 m) — the engine strips it via `pose::havok_palette_in_place` so the
entity `Transform` drives movement instead.

## Historical note

The "head-fan" defect originally observed when rendering the walk clips was **not** a
clip / track-binding problem — it was the per-group BLENDINDICES bug, since fixed (see
`skinning_animation_spec.md` §1.4). The clips themselves decode correctly.
