---
title: The Mercenaries 2 Character Authoring Kit
status: current
evidence: proven
date: 2026-07-27
---

# The Character Authoring Kit

Export a retail character, edit it in Blender, put it back in the game. This is the supported route
for new skins, faces and equipment.

**One skeleton serves the whole game.** All 127 humanoid models in `vz.wad` share a single bind pose
to the millimetre (leg 0.860 m, torso 0.483 m, head 1.659 m). There is no tall NPC and no short one.
So one base kit covers every character, and a donor is chosen for its bone count and texture layout —
**never** for its proportions, which carry no information.

---

## 1. Export the base

```bash
mercs2_workshop --wad <vz.wad> --no-auto-patch --export-bundle pmc_hum_mattias --out kit/
```

You get, under `kit/pmc_hum_mattias/`:

| file | what it is |
|---|---|
| `model.gltf` + `model.bin` | 116 named joints w/ inverse-bind matrices, skinned mesh, **100 hero clips** |
| `textures/*.png` | every texture the model binds, decoded |
| `raw/blockNNNN_P000.ucfx` | the ORIGINAL container bytes — nothing is lost |
| `manifest.json` | draw groups, LOD chain, hier tree, texture dimensions |

The glTF opens with the **close-up display tier as the default scene**, and Blender imports the other
tiers as separate collections bound to the same armature. The clips come with it, so you can scrub a
real walk cycle while you model.

---

## 2. The rules that will bite you

These are not style preferences. Each one has cost a session.

**Bone lengths are NOT yours to change.** Animation clips carry a per-bone translation track that is a
constant copy of the stock bind offsets, and the engine uses it. A re-proportioned skeleton is
overridden back to stock while your inverse-binds stay custom, so the mesh tears. **Proven in-game**
(`mercs2_probe --bin stretchrig`): a Mattias built with legs at 1.162 m rendered with stubby legs and
arm geometry snapping back to stock. Mesh **shape** — bulk, silhouette, build — is unlimited; mesh
**height** is not available.

**A host draw group must be in LOD tier `0x01`.** Geometry on a `0x02` group previews perfectly and
is invisible in game. That has shipped once already, as an invisible torso and arms.

**The bone palette cap is 48 per draw group, and it counts BONES, not triangles.** A part may span
several groups; a group must never span parts, because a draw group carries exactly one material.

> **★MEASURED 2026-07-31** — this line was a bare assertion and is now backed by a census of the
> whole shipped game: `skin_census --wad --per-group` over **3,007 model assets / 2,052 skinned
> drawing groups** ([`../data/palette_census.csv`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/palette_census.csv)). Distinct weighted
> bones per group stop dead at 48 — the tail is 44:41 groups, 45:47, 46:32, 47:13, 48:8, then
> nothing. No shipped group weights a 49th bone.
>
> **But BONES and palette SLOTS are different numbers, and conflating them was a live bug.** The
> `INFO(56)` table stores runs, and the packer bridges gaps between them to stay inside the 8-range
> field, so a group ships more slots than it has bones. Retail's own `pmc_hum_mattias` group 3 is
> `range_count=4, slots=49, bones=48`; the corpus maximum is **62 slots** (`gr_hum_elite` group 3 —
> one run of 62 covering just 13 weighted bones, i.e. retail declares a wide contiguous window and
> leaves most of it unweighted). Slot ceilings: `range_count <= 8` (field width) and `total <= 256`
> (the engine reader's own gate, [`skinning_animation_spec.md`](../modernization/skinning_animation_spec.md) §150-155).
>
> `mercs2_formats` gated SLOTS against this 48 until 2026-07-31 — so our own writer would have
> refused the donor it was conforming to. Now `MAX_GROUP_BONES = 48` (measured), `MAX_PALETTE_SLOTS
> = 256` (engine gate), `RETAIL_MAX_SLOTS = 62` (warn above; legal but unattested).

**Textures are fully resident, and the slot order is `0 = diffuse, 1 = SPECULAR, 2 = NORMAL`** — not
the intuitive d/n/s. Characters ship the whole mip chain; a short BODY makes the engine over-read and
the world-load livelocks. Normal maps are **BC3 / DXT5nm with GREEN FLIPPED** (glTF is +Y up, this
engine samples +Y down); getting that wrong turns every bump into a dent and is invisible in a
diffuse-only preview.

| slot | size | format | mips |
|---|---|---|---|
| diffuse, specular | 512² | BC1 | 8 |
| normal | 512² | BC3 | 8 |
| eyes | 128² | BC1 | 6 |

**Name your assets, never invent a hash.** Every hash is `pandemic_hash_m2(<real name>)` and the
registry insert is FIRST-wins, so a collision silently drops *your* asset. Check a candidate before
you commit to it:

```bash
mercs2_workshop --hash pmc_hum_yourchar_ub          # name -> hash
mercs2_workshop --wad <vz.wad> --check    0xHASH    # must say "no model chunk"
mercs2_workshop --wad <vz.wad> --tex-check 0xHASH   # must say "no ASET"
```

**Ship additively, over the top.** New content goes in `vz-patch.wad` as an overlay. `vz.wad` stays
pristine — always. If a patch WAD is already deployed, **merge** into it rather than replacing it:

```bash
wad_builder merge-blocks --patch-wad <live vz-patch.wad> --from <yours.wad> \
                         --block <your_block> --out merged.wad
aset_refcheck merged.wad     # MUST print OK before you deploy
```

---

## 3. Equipment slots

The rig ships eight mount points. These are real, retail-authored attachment bones — measured world
positions on the shared skeleton:

| bone | position (x, y, z) | mounts |
|---|---|---|
| `bone_attach_hipleft` / `hipright` | ±0.175, 1.090, −0.025 | holsters, hip pouches |
| `bone_attach_backleft` / `backright` | ±0.084, 1.396, −0.186 | packs, slung weapons |
| `bone_attach_chest` | 0.000, 1.250, 0.139 | webbing, chest rig |
| `bone_attach_lhand` / `rhand` | ±0.456, 0.894, 0.036 | held weapons, tools |
| `Bone_Attach_Root` | 0.000, 0.000, 0.000 | whole-character props |

**How retail actually does it.** An equipment part is a **rigid mesh parented to a bone**, authored in
that bone's LOCAL space — not a skinned part. `pmc_hum_mattias` carries six of them:

| part | mounted on |
|---|---|
| `pmc_att_packs_a_s18` / `_s19` | `bone6` / `bone5` (hip slots under `Bone_Hips`) |
| `pmc_hum_fiona_eyes_s20` / `_s21` | `bone_eyeball_right` / `_left` |
| `reflection_s20` / `_s21` | the same eyeball bones |

Note `pmc_hum_fiona_eyes` mounted on **Mattias** — parts are already shared across characters by
name. The modular system is Pandemic's, not ours.

**Because bone positions are fixed by the clips, you reposition an attachment by editing its
local-space vertices — not by moving its bone.** The import path carries rigid parts through as
single-bone 100 %-weighted skins, so they survive the round trip and animate correctly.

---

## 4. Faces

The head is its own draw group with its own material, and 89 face bones hang under `Bone_Head`
(`bone_eyeball_left/right`, `bone_jaw`, the brow/mouth cluster). A face variant is therefore a
**part swap**: replace the head group's geometry and its diffuse/spec/normal set, leave the body
alone. That is how retail ships its own variants.

---

## 5. Put it back

```bash
# preview the edit exactly as the engine will draw it — textured, skinned, posed by a real clip
mercs2_workshop --wad <vz.wad> --no-auto-patch --overlay <candidate.wad> \
                --render pmc_hum_yourchar --render-clip 0x1CA0475D --render-t 0.5 --render-out out
```

`--no-auto-patch` is **not optional** while iterating: without it the workshop silently loads the
deployed `vz-patch.wad` and you review yesterday's build. Always render a retail character beside
yours as a control.

**The render is trustworthy.** Its clip path (`havok_locals`) was confirmed against the running game
during the bone-length test — offline preview and in-game matched. Prefer it to screenshots, which
come from inconsistent camera angles and cannot be compared to each other.

Then publish the model as a wardrobe outfit. `Player.SetOutfit` takes a **name string**, so only
*named* models work, and `_tOutfits` in `wifpmcinterior.lua` is a plain global — mods compose by
appending Lua source and compiling once. Exactly one thing must own `scripts_vz`.

---

## 6. Known gaps

Honest about what is not done:

- **The round trip is vertex- and triangle-exact but not yet byte-exact**: a re-imported
  `pmc_hum_mattias` returns 29,023 verts / 35,344 tris (matching retail exactly) with the bbox top
  2.9 cm low — the conform still nudges the mohawk tip. A native import ideally bypasses the conform
  entirely.
- **Body height is blocked** (§2). Only mesh shape varies.
- **Texture upscaling is unsupported** — dimensions are baked into the container INFO.
- `tex_build` takes a single square `--size`; non-square source maps (e.g. 1024×512) need `--size WxH`,
  which is not implemented yet.

## Sources

`memory/pandemic-shared-human-rig-mercs2-saboteur.md` (one skeleton, census) ·
`memory/clips-carry-bone-lengths-body-size-blocked.md` (in-game proof) ·
`memory/native-rig-roundtrip-and-modular-parts.md` (round trip, modular parts) ·
`memory/character-texture-injection-solved.md` (MTRL slots, DXT5nm) ·
`memory/workshop-render-is-the-iteration-loop.md` (draw gate, LOD tier) ·
[`docs/modding/field_guide.md`](field_guide.md) (17 traps) · [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md)
