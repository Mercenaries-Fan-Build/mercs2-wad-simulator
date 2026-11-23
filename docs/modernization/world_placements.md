# World-Content Pipeline — the `ModelName` COMP Keystone

**Status: KEYSTONE SOLVED, proven live vs retail (2026-07-02).** Source of record:
memory `world-placements-no-model-hash`.

This closes the last unknown in world content: how a placed world entity links to the
mesh it should render. There are now **two proven paths and no unknowns left.**

## The placement records

`mercs2_formats/src/placement.rs` `load_placements(block 29)` parses **62,624 placements
(exact)**: `{ entity_key, name, pos, quat }`, `yaw_from_quat = 2·atan2(qy, qw)`. Verified
via `--placements` markers.

## ★ The keystone: `ModelName` COMP

The entity → mesh link is the **`ModelName` COMP** — a sibling COMP keyed by the same
`entity_key` (it was simply never read before; only Transform + Name were parsed, which
is why it earlier looked like "no model hash existed").

- Record layout = **`{ u32 entity_key, u32 model_hash }`** (8-byte stride; schm
  payload_stride 4 + key).
- `model_hash` = `pandemic_hash_m2(model-name)` = the model ASET `asset_hash` **directly**
  → feed straight to `wad::extract_container`.
- **455 / 465 `ModelName` records resolve** to loadable containers.
- There is **no** raw hash field and **no** separate table.

Engine side: [`docs/mercs2-ecs`](../../crates/mercs2_workshop/data/mercs2-ecs) `Model` `0x5b724250` / `SceneObject` `0xb6185886` are the
runtime handles; `ModelName` is the on-disk serialized reflection field. Tooling:
`placement::comp_inventory` / `CompInfo` + `mercs2_engine --comp-probe`. Block 29 holds
**43 COMP types** (Transform ×166 st52, Name ×166 st5, ModelName st4, LightObject,
RoadIntersection, DestructionLink, Road, LaneData, DangerousBuilding, …).

> Cross-reference: `ecs_components.md` lists `ModelName | 4 (stride 8) | u32 model hash`
> but without the pipeline; `placement_data_format.md` documents Transform / Name /
> LowResTerrainObject but **not** ModelName. This doc is the authority for the
> entity→mesh pipeline; those two should point here.

## ★ Architecture verdict — two paths

**Exterior BUILDINGS are baked into c3 cell geometry (tier B), NOT placed per-entity** —
0 `_bld_` names appear in `ModelName`, and a test house resolved to its c3 cell. So:

1. **Buildings / static world → load c3 cells** covering the area.
   Loader exists (`--cells`, `load_c3_cells`; grid formula in `world_terrain_loader.md`).
   Expand from the 16-nearest to a radius.
2. **Props + interior furniture → the ModelName recipe.** Per sub-block, build
   `key → Transform` and `key → name`; for each `ModelName {key, hash}`:
   `extract_container(hash) → build_indexed_from_container → place at the key's Transform`
   (pos + yaw). Local verts → world via the transform.

## Interiors

(See memory `pmc-teleport-coords-and-interior`.) The interior **shell** is loaded by PATH
(`pmcoutpost_bld_hq` `0x50AACA22` / `pool` `0xC087777D` / `suites` `0xD5D65249`);
**furniture** is `ModelName` in block 667 (proven: the wardrobe `0x8AAA90D1` at the game
spawn). Game-start spawn is the PMC INTERIOR at `(3794.04, 450.75, -3911.03)` (off-map
high-Y); the exterior spawn pool is `(2560.26, -13.18, -926.25)`.

## Next

The RENDER brick is now mechanical: spawn `ModelName` props + expand c3 cells + assemble
the interior. See `world_terrain_loader.md`, `world-lod-and-destruction-scope`.
