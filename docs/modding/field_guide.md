# Mercenaries 2 Modding — Field Guide (data & packaging half)

This guide covers models, skins, textures and WAD packaging. Lua scripting is covered
elsewhere. It is written as a list of **traps**, because that is what this engine actually is:
a machine that almost never tells you what you did wrong. It loads your broken asset, says
nothing, and then hangs, or crashes ten seconds later when you turn the camera, or silently
hands you the stock car instead of your tank.

Every claim here comes from the project corpus and cites its source. If something is not in
here, assume it is not known.

---

## Read this first: everything is a name hash

The engine has almost no strings at runtime. Assets, bones, events, materials, spawn
templates, script names — all of it is addressed by a 32-bit hash called
**`pandemic_hash_m2`** (an FNV-1a variant with a `|0x20` case-fold and a `^0x2A · prime`
step). The PC implementation is `Hash_String FUN_00824270`; `String.GetHash` in Lua funnels
into the same function.

Three consequences you must internalise before you touch a byte:

1. **A typo is not an error.** `pmc_hum_mattias_v2` and `pmc_hum_mattais_v2` are both perfectly
   valid inputs. One resolves to an asset; the other resolves to a hash that is in no table,
   and the engine simply does nothing — no log line, no assert. Your mod appears not to have
   loaded. This is the single most common beginner failure, and it looks identical to "my
   tool is broken".
2. **The hash is one-way and only 32 bits.** You cannot recover a name from a hash. The repo
   ships a **733,594-entry rainbow table** (`tools/rainbow_table.json`, built by
   [`tools/build_rainbow_table.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/build_rainbow_table.py)) that inverts most of the hashes the game actually uses, but
   a hash match *alone is not evidence a name is real* — with ~13,000 unresolved target hashes,
   any candidate list past ~6.7 million strings is *expected* to fabricate plausible names by
   collision. Names in the corpus carry a second, independent witness (a block path, a texture
   stem); guesses are marked as guesses.
3. **For bones the hash is case-insensitive.** `Bone_RBicep` == `bone_rbicep`, because of the
   `|0x20` fold. Casing is not recoverable and is not evidence of anything.

Everything below is a variation on "you handed the engine a number it could not use, and it
did not tell you."

Sources: [`docs/reverse_engineer/scripting_host_binding_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/scripting_host_binding_code_map.md) §4 ·
`memory/rainbow-table-pipeline-and-sibling-tooling.md` · `memory/aset-name-export.md` ·
`memory/model-hier-bone-naming.md` · [`docs/type_hash_registry.md`](../type_hash_registry.md)

### The resolution rules, running at once, in different directions

This trips people constantly. All three are true simultaneously:

| Layer | Rule | What it means for you |
|---|---|---|
| **WAD file stack** (`vz.wad`, then `vz-patch.wad`) | **last-opened wins** | Your patch WAD shadows the base by hash. You never edit `vz.wad`. |
| **Chunk registry** (once a block is resident) | **first-wins** | Within the resident set, the first registration of a hash keeps the slot. |
| **String databases** (`AddStringDb`) | **last-registered wins** | A later DB shadows an earlier one per key, then the base language DB, then NULL. Capped at **8 DBs total** — the ninth silently gets nothing. |

Sources: [`docs/comprehensive_engine_understanding.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/comprehensive_engine_understanding.md) §3.2 · [`docs/patch_wad_format.md`](../patch_wad_format.md) ·
[`tools/wad_simulator/crates/mercs2_engine/src/asset.rs`](../../crates/mercs2_engine/src/asset.rs) ·
[`format_reference.md` §4.1 "Lookup semantics"](../format_reference.md) (`FUN_0046423e`)

---

## Trap 1 — Your mod "didn't load" and there is no error

**The symptom.** You built a patch WAD, dropped it next to `vz.wad`, booted, and the game is
completely unchanged. No crash, no log line, nothing.

**Why.** Almost always one of:
- You hashed a name that does not exist (see above). The lookup misses and the engine moves on.
- You minted a **new** asset hash but did not give it an **ASET row**. The loader wedges
  *silently* at world-load — proven in the heli experiment: without the ASET entry (block index
  + sub `0xFFFF` + type id) the load simply never completes.
- You minted a new hash and expected it to appear in the world. **New asset hashes do not
  resolve into `layers_static` placements.** Five novel static models were invisible *before*
  any crash for exactly this reason. This is why skins ship as *wardrobe overrides*, not new
  hashes.

**The fix.** Prefer **overriding an existing, already-referenced hash** — that is the proven,
last-wins path and needs no placement work. If you genuinely need a new asset, it needs its own
ASET row *and* something that asks for it (a `Pg.Spawn` call, a wardrobe entry, a store item).
`Pg.Spawn` accepts a **raw m2 hash as a number** natively — no patch needed.

**Source.** [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §1 Stage C/D ·
[`docs/heli_store_injection_experiment.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/heli_store_injection_experiment.md) ·
`memory/from-scratch-models-rejected-conform-to-template.md` ·
`memory/name-registry-spawn-by-hash.md`

---

## Trap 2 — The model loads fine, then the game crashes when you *look* at it

**The symptom.** World loads. Everything is fine. You walk around the corner, the custom model
comes into view, and the game dies at **`EIP = 0x00855691`**, access violation reading address
`0x182`, `EAX = 0`.

**Why.** You **appended** a material to the model's `MTRL` chunk. A donor container ships N
material records; the engine sizes its per-model shader-registry entries from that count, and
record N (the 9th of an 8-material donor) never gets a slot. The chunk parses, the model loads —
and then the draw driver does:

```c
// FUN_00855420, per 0x58-byte draw item
shader = *(void **)(DAT_00ff46f4 + *(u16 *)(item + 0x3c) * 4);  // NULL for the appended material
perm   = *(i16 *)(shader + 0x182) + light_class;                // AV READ [null + 0x182]
```

A *clean* NULL (not a wild pointer) is the tell: the index is in range, the slot is simply empty.

**The fix.** **Never grow the MTRL record count.** Convert an unused record **in place**,
keeping the count: `inject_parts --replace-mtrl <dst>:<src>:<0xTEX>`. Untextured records
(flags `0x0000`) are exactly the ones to convert — only records with flags `0x0080+` sample a
texture at all.

Note: giving the appended record a *fresh unique hash* was tried and **did not help** — growing
the count is the trigger, full stop. (An MTRL record's first u32 *is* a unique name hash and is
distinct across records in every stock container; keep it unique when you replace, but that is
not what causes this crash.)

**Do not chase the loader.** The failure is draw-gated, not load-gated.

The same `+0x182` word is the material's pixel-shader registry index: a material whose pixel-shader
key is not registered crashes earlier, at `0x00858DB8` in `Mtrl_Parse`. `qm` checks every material
key and primitive-group vertex-shader word it emits against the registry (**M0235**, **M0236**), and
`add_model` writes them by the retail convention
([`manifest_format.md` — the shader import](manifest_format.md#the-shader-import)).

**Source.** `memory/mtrl-name-hash-shader-registry.md`

---

## Trap 3 — Your matte black tank renders as shiny pale-lavender chrome (or pure white)

**The symptom.** The geometry is right. The paint is catastrophically wrong: mirror-bright
lavender, or blown-out white, or the surface looks like crumpled, creased foil.

**Why.** The MTRL texture slot order is **`0 = diffuse, 1 = SPECULAR, 2 = NORMAL`**. It is *not*
diffuse/normal/spec. Get it backwards and the engine reads your normal map as a
specular-intensity map — a pale lavender (127,127,252) sheet means "uniformly high gloss"
everywhere. Garbage in slot 2 gives per-pixel normal garbage: "crumpled metal".

And the normal map in slot 2 is **DXT5nm, not an RGB normal**: `normal.x = ALPHA`,
`normal.y = the greyscale colour` (R==G==B), `z = sqrt(1 - x² - y²)`. Ship an RGB normal as
DXT1 and DXT1's implicit `alpha = 255` forces `normal.x = 1.0` for every texel — every normal
points along the tangent, lighting explodes, and the tank renders **white**.

**The fix.**
- Slot 1 (specular) = DXT1, and it must carry `INFO byte[8] = 0x20` (the specular flag), which
  `wad_builder set-tex-specular --block <sm>.bin --out <sm>.spec.bin` sets. A missing flag faults
  the model bind.
- Slot 2 (normal) = DXT5, packed DXT5nm. Encoder: [`tools/nm_to_ucfx_dxt5nm.py <nm.png> <name> <out.bin>`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/nm_to_ucfx_dxt5nm.py).
- Write slots explicitly: `--set-tex <mtrl>:<slot>:<hash>`. `--set-mtrl` only writes slot 0.
- A specular sheet for a matte vehicle wants to be **dark** (~26/255); 76/255 already reads as satin.

**Source.** `memory/model-injection-full-checklist.md` §2

---

## Trap 4 — Everything renders, but the geometry culls or shades wrongly (stale AREA)

**The symptom.** Subtle and hard to pin: parts vanish at angles, or the container simply fails
validation. You replaced STRM/IBUF/PRMT and thought you were done.

**Why.** The `AREA` chunk is **one f16 per strip triangle, `count == ibuf_index_count - 2`**.
Each value is that triangle's world-space surface area as a half-float, and exactly `0.0` for
the degenerate stitch triangles. (Proven: reinterpreted as f16, correlation with recomputed area
is **0.995**, ratio median 1.00. As raw ints it only correlates 0.88 with sqrt(area) — do not be
fooled into thinking it is an integer length.)

Replace a group's geometry and this array still describes the **donor's** mesh and is the wrong
length — the ztz98 hull ships 402 entries while the replacement hull has 62,995 triangles.

**The fix.** Use `inject_parts`, which rebuilds AREA per group. If you are hand-rolling, rebuild
it or you are shipping a lie about your mesh.

**Source.** `memory/model-injection-full-checklist.md` §1

---

## Trap 5 — Looks like a clean custom vehicle from a distance, shreds into holes up close

**The symptom.** You screenshot your new tank from across the courtyard. Perfect. You walk up to
it and it dissolves into cracks, holes and floating shards.

**Why.** **A model is not one container.** Its geometry is split across a chain of blocks named
`<model>_P00N_Q(3-N)`, coarsest first:

| chunk | resident `P000` (coarsest) | streamed `P001` / `P002` |
|---|---|---|
| `HIER`, `SEGM`, `MTRL`, `PHY2`, `SWIT`/`CEXE` (destruction machine) | **yes** | **absent** |
| `GEOM`/`MESH`/`PRMG`/`PRMT`/`IBUF` + `INDX` | yes (coarse) | yes (finer) |

The object = `geometry(rung) × INDX(rung) × SEGM/HIER/MTRL/machine(resident block)`. The rungs
**refine** — they never sum, and there is no "pick a rung" step.

Conform your model into the **resident** rung alone and the donor's own geometry is still sitting
in the finer rungs. Your injected parts get `lod_mask 0x7F` (every tier), so they draw at all
distances — but the donor's `P001` (tiers 2–3) and `P002` (tiers 0–1) keep drawing **their**
geometry at close range, straight through yours. That is two interpenetrating vehicles, not a
faceting bug. The ztz98 hid **32 + 61 = 93** donor drawing groups this way.

**The fix.** Override the finer rungs too:
`lod_neuter <rung.ucfx> <out.ucfx> --name-hash 0xH` empties every drawing group in a rung, then
`smuggler --exact-block --block-index <rung>` + `wad_builder merge-blocks` folds them into the
patch. `--exact-block` is the **only** way to reach a vehicle's finer rungs — by default
`--inject-container` redirects to the ASET-primary block.

Some rungs are **sub-entry** models with no ASET row of their own; those need
`block_neuter <block.bin> <out.bin>` (byte-size preserving, CSUM recomputed).

Two things that will mislead you:
- `<model>_P003_Q0` and `resident2-<model>_tracks_*` are **texture** blocks (type `0xF011157A` =
  high mips), not geometry.
- **Characters ship ONE block, no chain** (Mattias: 55,490 tri resident). That is why every
  character looked right while every vehicle looked broken.

**Any "it renders fine" verdict taken from a distant screenshot is not evidence. Get close.**

**Source.** `memory/model-lod-block-chain.md` · [`docs/modernization/vehicle_model_spec.md`](../modernization/vehicle_model_spec.md) ·
[`tools/wad_simulator/crates/mercs2_smuggler/src/main.rs`](../../crates/mercs2_smuggler/src/main.rs)

---

## Trap 6 — A surface renders the *wrong texture*, or props look "missing"

**The symptom.** The floor wears a neighbouring wall's texture. Paintings and crates in a room
are "missing" — actually they are there, just painted with the wrong material.

**Why.** A `PRMG` drawing group is frequently **multi-material**: it concatenates several `PRMT`
records in one IBUF, each an independent sub-strip with its **own** material. A PRMT record is 16
bytes: `material_index @0, index_start @4, index_count @8`.

The PMC hall shell (`0x39AF17DC`) has **10 of 15 groups multi-material** — group 1 alone packs 23
materials (floor + walls + trim, one group).

If your tool keeps only the *first* material per group and binds it to the whole group, every
later sub-strip renders with the wrong texture. Geometry and material were always present; the
binding was wrong. Fixing this took the hall from 15 draw groups / 15 textures to **80 / 44**.

**The fix.** Render (and reason) **per sub-strip**, not one material per group. Before you go
hunting the material table or the SWIT/STAT state, check whether the offending surface is a
non-first sub-strip.

Verify with `cargo run -p mercs2_probe --bin mesh_probe -- 0x<hash>` — it dumps the full per-group
PRMT material list and the post-fix draw count / distinct diffuse names.

**Source.** `memory/multi-material-draw-groups.md`

---

## Trap 7 — Your reskin makes the game hang on the loading screen (not crash — hang)

**The symptom.** You replaced a texture's `BODY` in place. World load never finishes. It is a
**hang**, not a crash, so there is no faulting address to chase.

**Why.** **9,562 of 13,339** retail textures are **streamed**. The inline BODY is only a small
resident *tail* — a 1024² map can ship as a 32×32 stub — and the real mips live in *other blocks*
entirely (the finer c3-cell LOD blocks of the texture's own subtree; each carries a lone `BODY`
UCFX chunk = exactly one finer mip level, with no INFO/NAME).

So you cannot reskin by overwriting the donor's body in place. A short BODY makes the engine
over-read → `BUFFER_TOO_SMALL` → world-load livelock.

**The fix.** Publish a **fully-resident** container under the same hash:
`INFO[26..32] = 0`, `0xFFFF` sentinel at offset 32, and `BODY` = the exact `linear_mip_chain_size`.
Fully-resident means read inline: no mip streaming, no livelock. This is what the shipped
mattias_v5 / Obama skins do. Encode with [`tools/dds_to_ucfx_texture.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/dds_to_ucfx_texture.py) (or
`mercs2_formats::texture::build_resident_texture`).

Downscale sources to power-of-two, 1024² is the working default (2048² max). **Upscaling is not
supported** — the dimensions are baked into the container's INFO.

**Two adjacent traps in the same family:**
- **Your exported PNG comes out ~99% empty, not merely blurry.** A resident `TextureData` reports
  the texture's **full** width/height while `mip0` holds only the tail. Any decoder that trusts
  `width`/`height` and walks blocks off `mip0` fills a few rows and leaves the rest zeroed.
  Always resolve the finest level the bytes actually cover. Rule: `extract_texture` is for a fast
  preview only — anything a human will *see* or *export* must use `extract_texture_hires`.
  Check with `cargo run -p mercs2_probe --bin texcmp -- [0xMODEL]` (prints resident-vs-hires real
  dims per texture).
- **A texture's inline BODY has two shapes.** Streamed (body = the tail of the chain) *or*
  prefix/single-level (body simply starts at mip 0 — **all HUD/UI art is like this**: `HUD_HQ_AN`
  is 64×64 DXT5, `mips=1`, body exactly 4096 B). Decide by size: `body >= level_bytes(0)` ⇒ starts
  at mip 0. Assuming only the tail case makes every `HUD_*` texture decode to nothing.

**Bonus trap: shared textures.** `pmc_hum_chris_ub` is used by **both** `pmc_hum_chris` and
`pmc_hum_mattias_v2` — reskinning it silently changes Mattias's Vacation outfit. Likewise
`al_veh_tank_m1a1_dm` is used by the **m1a2** model. Check what else uses a texture before you
touch it.

**Source.** `memory/modkit-merge-wardrobe-textures.md` · `memory/texture-high-mip-streaming.md` ·
[`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §1 Stage B

---

## Trap 8 — You edited a block and now the heap is corrupt (the `packed_field` bug)

**The symptom.** Anything. Heap corruption is a shapeshifter: random crashes far from the edit,
a world-load livelock, garbage in unrelated systems.

**Why.** Every WAD block carries an **INDX page count** (`packed_field`). The engine sizes its
decompression destination buffer as **`page_count << 15`** — a u24 page count × 32 KB — in
`FUN_00875b00`, and then issues an async read into it. The gate only checks free-list capacity;
it does **not** check that the read fits.

So if `packed_field` is stale, too small, or byte-swapped:
- **Under-claimed** → the read span exceeds the allocation → `STATUS_BUFFER_TOO_SMALL`
  (`0xC0000023`) → the streaming node never reaches ready state 4 → **livelock**.
- **Hardcoded to 1** → a 32 KB buffer for any block → every block over 32 KB **overruns the heap**.
  This was a real, shipped library bug: `PatchBlock::new` left `packed_field = 1` while every
  *binary* recomputed it. The wardrobe's `scripts_vz` block is **49 pages** — it would have been
  dead on arrival.
- **Over-claimed** (an unswapped Xbox-BE u16: `0x0003` read as `0x0300`) → the engine streams
  24 MB for a 66 KB block → source over-read → same livelock.

**The fix.** The page count must be recomputed from the **decompressed** size, and the high byte kept:

```
packed_field = (tier << 24) | ceil(decompressed_size / 0x8000)
```

`tier` is the block's LOD rung `N` (`_P00N`), and the engine uses it as the block's eviction class
([`reverse_engineer/render_core_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_core_code_map.md)
§8). A block re-emitted from an existing one copies the source
block's tier (`PatchBlock::from_decompressed(.., Some(source_packed_field))`); a new block that is an
asset's `_P000` is tier 0. Writing tier 0 on a finer rung makes it unevictable by coarse requests.

Route block edits through a tool that does this: `wad_builder replace-block` sets it **exactly**
from the decompressed size; `wad_builder merge-blocks` **bumps it up** if the source undersized it.
`filter-keep` / `drop-blocks` copy blocks verbatim and never re-examine page counts (safe — they
only remove blocks). Use `PatchBlock::from_decompressed(...)` in library code, never
`PatchBlock::new`.

**Source.** `memory/engine-streaming-buffer-sizing-chain.md` ·
`memory/modkit-merge-wardrobe-textures.md` · [`tools/wad_simulator/docs/cli/wad_builder.md`](../cli/wad_builder.md)

---

## Trap 9 — You changed one byte and the container is now invalid (CSUM)

**The symptom.** Depends on the consumer; do not rely on getting a message.

**Why.** Every UCFX container ends with a `CSUM` trailer: **CRC-32/JAMCRC** (init `0`, poly
`0xEDB88320`, no final XOR) — in this repo `crc32_mercs2`:

```python
crc32_mercs2(data) == (zlib.crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF
```

Verified across **53,765 chunks at 100% match**. Any byte edit requires a recompute.

**A caveat you should know about:** whether the engine *enforces* CSUM at runtime is still an open
question in the corpus (Mercs1's source had no such check, so it may be build-only). The project's
standing rule is **emit a correct JAMCRC anyway** — never rely on the engine not checking.

**Coverage note:** the corpus records two readings of the span — "everything **before** the trailing
`CSUM` tag" and "everything **through** the tag inclusive". Both appear in verified notes. The
practical answer is: don't hand-roll it. Every `wad_builder` write path recomputes and re-verifies
CSUMs against its own output before touching disk, and `wad_builder identity-test --block <bin>`
is the oracle — it proves the parse/serialize/CSUM model reproduces a real block byte-for-byte
*before* you edit anything.

**Source.** [`docs/modding_deep_dive.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/modding_deep_dive.md) · [`docs/reverse_engineer/valid_model_structure_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/valid_model_structure_map.md) ·
`memory/model-injection-full-checklist.md` §3 · [`tools/wad_simulator/docs/cli/wad_builder.md`](../cli/wad_builder.md) ·
[`docs/ui/shell_menu_lua_anatomy.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ui/shell_menu_lua_anatomy.md)

---

## Trap 10 — You authored a clean UCFX model from scratch and the engine rejects it

**The symptom.** Crash at **`0x004CC064`** (`FUN_004cc030` pool pop → NULL). Or, for skinned
models, `0x00858DB8` in `Mtrl_Parse`.

**Why.** **The engine rejects hand-authored UCFX model containers.** This was proven the hard way:
overriding the Phoenix car (a vehicle slot) *and* the hqsuites building (a static slot) with a
from-scratch container both crashed at `0x004CC064`. Even grafting a *known-good* material (Phoenix
mat0 — valid shader, valid textures, CSUM fixed) onto from-scratch geometry **still crashed**. So it
is not the texture hash and not the material content — it is the whole hand-authored container
(decl / shader / geometry binding). **Pure from-scratch has never rendered.**

**The fix — CONFORM, don't author.** Inject your novel geometry into a **real template container**
the engine already accepts. Preserve the donor's decl, material binding, shader, chunk layout and
HIER; swap only geometry (STRM/IBUF/PRMG/PRMT) + texture hashes, and recompute bounds + CSUM. Your
appearance stays 100% novel; only the scaffolding is inherited.

- Rigid single mesh → `inject_static <template.ucfx> <mesh.blob> <out.ucfx> --name-hash 0xHASH`
- Multi-part vehicle → `inject_parts <template.ucfx> <out.ucfx> --name-hash 0xH --part <mesh>:<group>:<node>:<mtrl_idx>[:spin] ...`

**A census invariant that saves you time:** every vehicle **and** every building carries the full
destruction + physics suite (`SEGM/SWIT/STAT/STAM/CHDR/CEXE/PHY2`). There is **no "simple
non-destructible" template**. Conforming preserves all of it, which is exactly why conforming is safe.

**Related shader trap (the material's shader key).** The first word of a material's 104-byte
preamble is **not** a shader. That word is `0x406b230e` in the skinned template and `0x0A164785` in
the static one; `Mtrl_Parse` stores it and looks nothing up with it. The pixel shader is the word
**after the texture hashes**: `pandemic_hash_m2` of a registered logical name. Examples:
`0xCAEFE1FE` = `PgDiffSpecNormFP` (static template), `0x322FCD56` = `PgDiffSpecReflNormAmbOccRimFP`
(skinned template, material flag `0x98`; the static template uses `0x80`).

`0x00858DB8` happens when that key was never registered. The key may be made up, or `tex_count` may
disagree with the hashes written, so a hash or a float lands in the key slot. The lookup then misses
and `Mtrl_Parse` reads `+8` of a null entry. See
[shader_store_format.md](../shader_store_format.md) §6–7.

**Related destructible trap.** Most buildings and all vehicles are destructibles — a `SWIT` node
swap where each mesh group carries **twin/multi PRMT records** (a state/LOD pair). A converter that
**collapses PRMT to one record** makes the state machine read off the end → AV into `CSUM` at
`0x00478E43` **at model instantiation**. `gltf_to_ucfx_model.py` now preserves the donor's PRMT
record count. `wad_simulator` does **not** catch this — it only shows up in-game.

**Source.** `memory/from-scratch-models-rejected-conform-to-template.md` ·
[`docs/reverse_engineer/valid_model_structure_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/valid_model_structure_map.md) · `memory/asset-injection-playbook.md` ·
[`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §1 Stage A

---

## Trap 11 — Your model has a group with a zero-size vertex buffer

**The symptom.** AV at **`0x0085C8D0`** (null surface).

**Why.** The engine **binds every drawing group's vertex buffer even when its draw-count is 0**.
A zero-size buffer is a null surface. So "just empty the buffers" is not a way to hide a group.

**The fix.** Hide a group by **collapsing its vertex positions to the origin** (degenerate
triangles) and zeroing its PRMT draw-counts. Never resize the buffers to zero.
`wad_builder unwrap-mesh --block <bin> --out <bin> [--drop-slots]` exists specifically to fix the
zero-size vertex-buffer crash on static-`MESH` sub-meshes inside a skinned character.

**Source.** `memory/model-injection-full-checklist.md` §3 ·
[`tools/wad_simulator/docs/cli/wad_builder.md`](../cli/wad_builder.md)

---

## Trap 12 — Your character skin hangs the wardrobe preview (a count field, not a crash)

**The symptom.** Textures resident, model loads, and then the wardrobe preview **hangs**. Live
trace showed the main thread spinning forever in `FUN_005A0B60` — an open-addressing hash-table
probe over a **1024-slot table that is 100% full**, so the linear probe visits every slot and never
finds an empty terminator.

**Why.** A novel model structure that the converter **mis-formatted** (bad byte-swap, wrong-assumed
field size) yields a **bogus COUNT**, the engine over-inserts, and a fixed resource hash table
overflows. In the Sarah case the material counts were sane (3,3,3,1…) so it was *not* MTRL — the
suspect was the geometry/group chunks (PRMG/STRM/IBUF/SEGM counts) of structures novel to that
model (glasses, eyelashes, hair).

**The generalisable rule:** **any count field that reads huge is a hang, not a crash.** When the
game hangs rather than faults, go looking for a count/size field the converter got wrong — not for
a faulting pointer.

The known concrete instance of this class: the `MTRL` `u16` texture-count at on-wire offset **106**,
transposed by a blanket u32 swap. `Mtrl_Parse FUN_00858790` reads it as a u16 and writes `count`
12-byte `{hash, 0xF011157A, 0}` records into a **fixed 10-slot array** at `material + 0xac` — so a
valid count (≤10) becoming `0x0400` overruns into the pool arena → AV `0x0084DD5B`. Fix with
`wad_builder fix-model-mtrl --block <bin> --out <bin>`.

MTRL on-wire layout, for reference:
`[u32/f32 × 26 = 104B][u16 flags @104][u16 count @106][u32 hash × count @108][u32 × 2 tail]`

**Source.** `memory/sarah-dlc-port-and-skin-pipeline.md` · [`docs/streaming_livelock_analysis.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/streaming_livelock_analysis.md) §1 ·
[`docs/reverse_engineer/asset_formats_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/asset_formats_code_map.md) §4.1

---

## Trap 13 — The tank is in the WAD, it validates, and the game hands you Fiona's stock car

**The symptom.** Your patch WAD builds. `wad_simulator` says clean. The model is definitely in
there. In-game, nothing you did shows up — you get the stock content.

**Why.** **A patch WAD's BLOCK SET is the contract, not its block count.** A vehicle swap is never
just the model. The custom tank at Fiona's spawn is **8 blocks**:

| block | what it does |
|---|---|
| `ch_veh_tank_ztz98_P000_Q3` | the conformed model container (`inject_parts` output) |
| `inject_<hash>` × 6 | the six skins |
| `scripts_vz_P000_Q3` | the `wifpmcgarage` bytecode redirect — makes the garage spawn `ZTZ98` instead of `_ksFionaCar = "Phoenix (Racing)"` |

Drop `scripts_vz` and the model is still in the WAD, still correct, still validates — and the game
hands you Fiona's stock car, **because nothing ever asks for the tank**. The spawn redirect and the
model are two independent halves of one feature.

**The fix.** Always **diff the block PATHS** after a rebuild, never just the count — a dropped
script block and an added texture both land on "8 blocks":

```bash
wad_builder list-blocks --patch-wad <new>.wad     # compare paths against the last good WAD
```

**Source.** [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §5.1

---

## Trap 14 — Mods fight each other and produce a chimera

**The symptom.** Two mods installed. You get mod A's model wearing mod B's textures. Neither mod is
"wrong".

**Why.** Naive per-hash last-wins resolution splits a mod apart. A real mod is a **set** of blocks
(see Trap 13) and it only means anything as a whole.

**The fix (modkit's model, worth copying).** Each operation emits one **`ClaimGroup`**, resolved
**all-or-nothing**: identical claim sets → clean override; strict subset → allowed only if the
overrider is `atomic: false`; **proper partial overlap → hard build error** (there is no correct
automatic answer). Load order is **LAST-wins** — the engine's rule, and MO2/Vortex's.

**Source.** `memory/modkit-merge-wardrobe-textures.md`

---

## Trap 15 — Wardrobe / skins: it is pure Lua, and only *named* models work

**The symptom.** You put a beautiful new skin in the WAD and it never appears in the PMC wardrobe.
Or: two skin mods are installed and only one of them exists.

**Why.**
- The wardrobe list is a **global Lua table**, `_tOutfits` (`wifpmcinterior.lua:155` — declared
  without `local`), and `GetAvailableCostumes` is a global function (line 1518). Nothing about
  outfits lives in a binary format. If you don't add an entry, the game never asks for your model.
- `Player.SetOutfit` takes a **name string**, which it runs through `pandemic_hash_m2` → ASET. So
  **only named models work.** A perfectly good, well-rigged, unnamed model is *unusable* as a
  wardrobe skin — 16 such models exist in retail.
- Two mods each shipping their own compiled `scripts_vz` block **silently annihilate each other**
  (per-hash last-wins: one whole block replaces the other).

- An appended row is not enough. The wardrobe saves row `i` as costume byte `i - 1`, and the join
  restores the byte through the player script's own map (`mrxplayer.lua:20`, a `local`), whose entry
  `k` is the model of row `k + 1`. Retail Mattias has 5 rows and an 11-model map, so a sixth row
  appended by hand restores as `gr_hum_starter_1` on the next load.

**The fix.** Declare the outfit with `add_outfit` and let `qm link` generate the wardrobe lists,
the character map, the always-listed rule and the saved outfit name for the whole installed set
([`docs/modding/manifest_format.md`](manifest_format.md), "Outfits are generated by the link"). A `patch_lua` that
appends its own `_tOutfits` row is refused once the set has outfits.

When compiling Lua for the game by hand, use the game's Lua dialect: **Lua 5.1 with `lua_Number = float` and 32-bit
`size_t`** (header `1b4c7561 51 00 01 04 04 04 04 00`) — `tools/lua51-mercs2/luac.exe`, or the
`mercs2_luac` crate (verified byte-for-byte identical output).

**What counts as a wearable skin:** a model **rigged to the heroes' skeleton** — that is what makes
hero animations play on it. Detect it by comparing HIER bone-hash sets against the heroes
(`parse_hier`), not by guessing names. Retail heroes = **116 / 94 / 92 bones, 85 shared** — use the
*intersection*, or one hero's private extras become fake requirements. Retail has **127 models ≥50%
rig, 25 at 100%**.

Deploy the edit with `wad_builder build-skin --patch-wad <wad> [--script wifpmcinterior] --luac <new.luac> --out <wad>`.

**Source.** `memory/modkit-merge-wardrobe-textures.md` ·
[`docs/mercs2-luacd/src/vz/wifpmcinterior.lua`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifpmcinterior.lua) · [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §1 Stage D

---

## Trap 16 — Bone names: you cannot brute-force them, and some "known" names are guesses

**The symptom.** You need to bind a part to a bone and you are trying to guess the bone's name.

**Why.** A bone is a **HIER node** (176 bytes; `+0` = `pandemic_hash_m2(name)`). The same node type
is used for deform bones, hardpoints, attach points and destruction pieces — there is no separate
ragdoll/Havok skeleton. The name is a one-way 32-bit hash, and brute force is **proven futile**: no
affix+wildcard grammar is enriched above the collision null. Do not re-run it.

**The fix — the census already exists. Don't re-derive it.** All **1,314 distinct HIER node hashes**
across every model in `vz.wad` + `vz-patch.wad` are enumerated in [`docs/data/bone_census.csv`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/bone_census.csv)
(**338 named**). Report: [`docs/reverse_engineer/bone_census.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/bone_census.md).

```bash
cargo run -p mercs2_probe -- bone-census [names.txt] [--csv <path>] [--wads a.wad,b.wad]
cargo run -p mercs2_probe -- hier --model 0xHASH [candidates.txt] [--csv <path>]
```

The confirmed grammar:

| pattern | meaning |
|---|---|
| `Bone_<Part>` | biped deform bone |
| `bone_<feature>_<side>` | face |
| `bone_attach_<part><side>` | attach point |
| `hp_<name>` | hardpoint |
| `bone_<part>_<axis>` | vehicle |
| `pristine` / `ruin` | destruction roots |

**Two corrections you need:**
- Most **static props have a two-root HIER**: `pristine` (`0x86DE6639`) + `ruin` (`0xB5D7712F`).
  "Root node hash = m2(model name)" is only true for rigged/destructible models.
- The destroyer `hp_seat_*` leaves and `bone_propeller_roll` / `bone_rudder_yaw` have **no second
  witness anywhere** — they are hash-only guesses. The *families* are real; the leaf words are not.
  **Do not cite them as fact.**

**Source.** `memory/model-hier-bone-naming.md` · [`docs/reverse_engineer/bone_census.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/bone_census.md) ·
[`docs/data/bone_census.csv`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/bone_census.csv)

---

## Trap 17 — You re-rigged your geometry onto the donor's nodes instead of the other way round

**The symptom.** The turret sits off the hull; parts land on wrong origins.

**Why.** Sliding your geometry sideways to meet the donor's node ring displaces it. Also: fitting
X/Z to the donor's **AABB centre** is wrong — the AABB centre is *not* the centreline. One
protruding fitting skews it (the ztz98's box centre is `+0.168` while every rig node sits at `X = 0`).

**The fix.** **Move the NODES to your model, not your model to the nodes.**
`inject_parts --node-at <node>:<x>,<y>,<z>` rewrites a HIER node's local matrix (the subtree
follows). Fit X/Z to the donor's **ORIGIN**.

And do not waste time on f16 vertex precision: max round-trip error at tank scale is **1.3 mm**
(mean 0.22 mm). It is not your problem.

**Source.** `memory/model-injection-full-checklist.md` §5, §6

---

## Trap 18 — You added an asset to vz's resident block and Play never reaches the world

**The symptom.** Play from the menu sits on the loading spinner forever. The shell exits, no level
Lua ever runs (`MrxSoundBootstrap.Init` never prints), the game keeps drawing, nothing crashes.

**Why.** The FFCS header's CSUM row `meta` (file offset `0x2C`) sizes the engine's block-entry install
ring, and the ring only ever grows to the largest `meta` of the WADs mounted. Retail vz.wad's is
7018 — exactly its resident block's entry count. A patch WAD whose resident holds one entry more
(any type: a chatter set, a texture, a 268-byte animation) and whose own `meta` is not raised is
retried every frame and never installed, so the level's scripts never exist
([`docs/format_reference.md`](../format_reference.md), CSUM row).

**The fix.** Write `meta` = the largest entry count of any block in the WAD. `mercs2_formats`'
`build_patch_wad_multi` does this itself (`patch_wad::csum_meta_for`); `wad_simulator` reports any
block the mounted stack's ring cannot hold (`NEVER INSTALLED`).

**The related trap.** Re-emitting a base block whole (qm does, for every Lua link into vz's resident)
must ship the blocks its rows name as LOD rungs too, or the writer drops those rungs and the assets
render at their coarsest tier — the PMC courtyard paving smears.

---

## The tools (these exist — use them, don't reinvent them)

All under `tools/wad_simulator` (a Rust cargo workspace). Run with `cargo run -p <crate> [--bin <b>] -- <args>`.

### Inspect / browse

```bash
# The native asset workshop — engine-rendered browser for every model and texture in the WAD.
# Same renderer mercs2_game boots, so materials/skinning/lighting are the faithful path.
cargo run -p mercs2_workshop
cargo run -p mercs2_workshop -- --overlay my-mod.wad          # stack a patch WAD on top (repeatable)
cargo run -p mercs2_workshop -- --check                       # headless: load every model end-to-end
cargo run -p mercs2_workshop -- --import-check my_mesh.glb    # will this foreign mesh parse?
cargo run -p mercs2_workshop -- --hash pmc_hum_mattias_v2     # m2-hash a name
cargo run -p mercs2_workshop -- --tex-png <name|0xHASH> out.png
```

⚠️ **Workshop overlay gotcha:** the texture modes (`--tex-check`, `--tex-png`, `--tex-png-block`,
`--tex-scan`, `--tex-scan-blocks`) open the **base WAD only** and ignore `--overlay` entirely. To
inspect a texture that lives in a patch WAD, pass that patch as `--wad`, not `--overlay`.

```bash
# Probes
cargo run -p mercs2_probe -- wad-list                        # blocks
cargo run -p mercs2_probe -- hier --model 0xHASH             # bone tree, names resolved
cargo run -p mercs2_probe -- bone-census --csv out.csv
cargo run -p mercs2_probe -- extract --model 0xHASH out.bin
cargo run -p mercs2_probe -- dump-block <index> out.bin
cargo run -p mercs2_probe --bin mesh_probe -- 0xHASH         # per-group PRMT material lists (Trap 6)
cargo run -p mercs2_probe --bin texcmp -- 0xMODEL            # resident-vs-hires real dims (Trap 7)
```

⚠️ **Probe gotcha:** `mercs2_probe block-grep` greps block **PATH NAMES** only — never contents. It
will confidently tell you "0 matches" for a string that is definitely in the data. The content-side
counterpart is `cargo run -p mercs2_probe --bin block_content_grep -- <needle> [--swap]`.

### Build a model container (conform — see Trap 10)

```bash
# Rigid single mesh into a static/vehicle template
inject_static barrel_template.ucfx my_crate.blob crate_out.ucfx --name-hash 0xDEADBEEF

# Multi-part vehicle: each part gets its own PRMG group, HIER node and material
inject_parts tank_template.ucfx out.ucfx --name-hash 0xH \
    --part hull.blob:0:12:3 --part turret.blob:1:14:5:spin \
    --node-at 14:0,1.8,0.4 \
    --set-tex 3:2:0xE1F66E9B \
    --replace-mtrl 7:3:0xAABBCCDD

# Silence a finer LOD rung so it can't out-draw your resident mesh (Trap 5)
lod_neuter uh1_P001.ucfx uh1_P001_muted.ucfx --name-hash 0x1A2B3C4D
block_neuter <block.bin> <out.bin>          # for SUB-ENTRY rungs with no ASET row
```

(`inject_static`, `inject_parts`, `lod_neuter`, `block_neuter` are bins of the `mercs2_formats`
crate: `cargo run -p mercs2_formats --bin inject_parts -- ...`.)

### Package it into a patch WAD

```bash
# Override an existing model by hash (the easy, proven path)
smuggler --source-wad vz.wad --block-index 3565 \
  --inject-container custom.ucfx --output output/parts/tank.wad

# --exact-block is the ONLY way to reach a vehicle's finer LOD rungs
smuggler --source-wad vz.wad --exact-block --block-index 3565 \
  --inject-container ct.ucfx \
  --inject-block "scripts_vz:scripts_vz_ztz.bin" \
  --inject-extra "0x21A2AFD1:27:skin.bin" \
  --output output/parts/ztz98.wad

# A from-scratch asset that overrides nothing (needs its own ASET row — see Trap 1)
smuggler --source-wad vz.wad --extra-only \
  --inject-extra "0xDEADBEEF:19:model.bin" --output output/parts/new_model.wad
```

Type ids for `--inject-extra`: **19 = model, 27 = texture, 35 = script**.

`smuggler` was formerly called `cube_mod` ([`docs/cube_mod_poc.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/cube_mod_poc.md) is the enabling RE writeup; same
CLI). It reads the source WAD, rebuilds the block, sges-compresses and emits a patch WAD with only
the overridden/added blocks + their ASET entries — **nothing is ever injected into `vz.wad` itself.
Reverting a mod = delete `vz-patch.wad`.**

### Edit blocks and whole WADs

```bash
wad_builder identity-test  --block scripts_vz.bin                        # ORACLE: run this first
wad_builder list-blocks    --patch-wad vz-patch.wad                      # paths + packed_field + ASET
wad_builder dump-block     --patch-wad vz-patch.wad --path scripts_vz --out scripts_vz.bin
wad_builder replace-block  --patch-wad vz-patch.wad --path scripts_vz --data new.bin --out out.wad
wad_builder merge-blocks   --patch-wad target.wad --from part.wad --block pmc_hum_sarah --out merged.wad
wad_builder fix-model-mtrl --block model.bin --out model.fixed.bin       # Trap 12
wad_builder set-tex-specular --block gun_sm.bin --out gun_sm.spec.bin    # Trap 3
wad_builder repoint-tex    --block model.bin --remap 0xOLD:0xNEW --out model.rp.bin
wad_builder build-skin     --patch-wad w.wad --luac interior.luac --out w.new.wad   # Trap 15
```

`--path` / `--keep` / `--drop` / `--block` are **case-insensitive substrings** and can match more
than one block. A too-broad substring silently sweeps in neighbours (`--drop tex` drops every path
containing "tex").

### Also available

- **`mercs2-modkit`** — a Tauri/Vue GUI (separate repo, `C:\Users\Shadow\Desktop\mercs2-modkit`) that
  wraps this: mod merging with ClaimGroups, the wardrobe, texture swap/browse, a 3D "where does this
  texture land" viewer with UV overlay, and WAD deploy/undo. `memory/modkit-merge-wardrobe-textures.md`.

---

## The verification loop (do this, don't eyeball it)

```bash
# 1. Offline: does it violate anything the simulator can see?
wad_simulator --wad output/data/vz-patch.wad --base-wad vz.wad --skip-audio
#    → expect VERDICT clean, 0 violations, model issues=0, ASET hash ownership verified

# 2. Confirm the artifact you tested is the artifact you shipped — BY HASH, not size/mtime.
sha256sum output/data/vz-patch.wad

# 3. Boot, then score the log with loadprobe — never by eye.
#    (0x874E7D is a hard-close, not a crash; loadprobe knows this and you don't.)
loadprobe pmc_blackbox.log
```

Three standing rules:
1. **`wad_simulator` clean ≠ correct.** It is ASET/consumption-level. It does **not** catch the
   collapsed-PRMT destructible crash, and it does **not** catch the multi-material draw-group bug.
   Those only show up in-game.
2. **Verify by DECODING THE SHIPPED CONTAINER** — strip → triangles, winding vs vertex normals,
   index range, AREA count. Every wrong theory in this corpus died the moment the container was
   actually read back.
3. **Get close.** A distant screenshot is not evidence (Trap 5).

**Source.** [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §5 ·
`memory/verify-artifacts-by-hash-not-size-mtime.md` ·
`memory/loadprobe-tool-and-0x874e7d-hardclose.md` · `memory/model-injection-full-checklist.md`

---

## Known-crash quick reference

| Address / status | Meaning | Trap |
|---|---|---|
| `0x00855691` | NULL shader slot at draw time — you grew the MTRL record count | 2 |
| `0x00858DB8` | `Mtrl_Parse` pixel-shader key not registered — made-up key, or `tex_count` disagrees with the hashes | 10 |
| `0x0084DD5B` | MTRL texture-count overrun of the fixed 10-slot array | 12 |
| `0x004CC064` | Engine rejected a hand-authored (from-scratch) UCFX container | 10 |
| `0x00478E43` | PRMT records collapsed to one on a destructible → state machine reads off the end | 10 |
| `0x0085C8D0` | Zero-size vertex buffer (a group whose buffers you emptied) | 11 |
| `STATUS_BUFFER_TOO_SMALL` (`0xC0000023`) → **livelock/hang** | `packed_field` page count wrong, or a streamed texture body too short | 7, 8 |
| **Hang** (no fault) in `FUN_005A0B60` | A bogus count field overflowed a fixed 1024-slot hash table | 12 |
| `0x874E7D` | **Not a crash** — a hard-close / force-quit | — |

---

## Adjacent knobs (not traps, but you will ask)

- **`data/cdbsizes.ini`** presizes the engine's ECS component pools. A copy is at
  [`docs/game_config/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/game_config). Doubling the actor/population/vehicle pools has been applied to a live
  install alongside `LARGE_ADDRESS_AWARE` on the three exes — but that was **not launch-tested**,
  and the per-frame CPU cost and the crowd *density* question are both still open.
  Sources: `memory/cdbsizes-component-pool-config.md`, `memory/entity-density-config-plus-laa-patch.md`.
- **`ViewDistance` is fog only.** Its sole reader is the fog function `FUN_007140b0`; the menu
  getter is a `return 1` stub. If you are trying to see further, that setting is not the lever.
  Source: `memory/viewdistance-is-fog-only.md`, [`docs/reverse_engineer/render_distance_and_density_levers.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_distance_and_density_levers.md).

---

## What this guide deliberately does not claim

- **Whether CSUM is enforced at runtime** is still an open question in the corpus (Trap 9). Emit a
  correct one regardless.
- **The exact CSUM byte span** (before the `CSUM` tag vs. through it inclusive) is recorded both
  ways in verified notes. Use the tools; don't hand-roll it.
- **Skinned custom-model import** (weight transfer) — **now solved for characters** (2026-08-02); the
  old "renders rigid, bound to bone 0 (A-pose)" limitation is lifted. A foreign skinned character
  retargets onto a hero donor (`retarget:` on `add_outfit`) and renders **deformed + animated**. The
  last holdout was **dense foreign rigs (>48 distinct bones)**: they are forced onto the multi-group
  balanced-split injector, which neuters donor host groups and renders **unstably** (culls/teleports on
  camera rotation). Fix — `add_outfit single_group: true` — skips donor transfer and forces the whole
  mesh into **one** host group on its own retargeted weights (RuMerc1: 53 bones/5 groups → 40 bones/45
  slots/1 group, shipped + persisted + rendered in-game; commit `b02a605`). Custom skins on the
  injected character bind once shipped **fully-resident** (Trap 7 / `build_resident_texture`, commit
  `4691fbf`) — a block missing its NAME chunk/`0xFFFF` sentinel loads but never binds → samples black.
  Still true structurally: **BLENDINDICES are per-group palette-relative** — they index the group's
  `INFO(56)` range-table concat, *not* the global HIER. Residual costs of `single_group`: per-material
  textures collapse to one **baked diffuse atlas**, and donor-resampled limb polish is skipped. **Not
  proven beyond characters** — `retarget` is character-only (humanoid Role → hero); arbitrary skinned
  props/vehicles still take the template-conform geometry path. Sources:
  [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §Stage A′, `memory/dense-foreign-rig-needs-single-group.md`,
  `memory/injected-texture-missing-name-chunk.md`, `memory/blendindices-per-group-palette.md`,
  `memory/cj-foreign-model-import.md`.
- **Texture upscaling is unsupported** — dimensions are baked into the container INFO.
- **Injecting a new placement into retail `layers_static`** is **untested**, and there is **no
  COMP-block writer** (`placement.rs` is read-only). Prefer Lua `Pg.Spawn` for new objects.
  Source: [`docs/asset_injection_playbook.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/asset_injection_playbook.md) §2.5.
- Retail **strips asset names** on the PC bake; **565 model hashes remain unnamed** and naming them
  needs a dev/debug build, not a cleverer search. Source: `memory/aset-name-export.md`.
