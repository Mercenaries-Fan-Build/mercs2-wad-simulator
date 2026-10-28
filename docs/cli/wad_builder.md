# wad_builder

`wad_builder` is the build-side editor for Mercenaries 2 engine assets: it edits
**decompressed engine blocks** (Lua-script containers, character models, textures) and
edits **whole `vz-patch` WADs** (list / dump / replace / filter / merge blocks), then
re-emits engine-valid output. Every write path re-parses its own output and re-verifies
all container CSUMs (CRC-32/JAMCRC = `crc32_mercs2`) before touching disk, so a file that
lands on disk has already round-tripped through the parser once.

The crate began as a Lua-script replacer inside a multi-entry `scripts_vz` block
(compile-from-source → re-wrap BINN/UCFX → fix CSUM + `chunk_size`) and grew to cover
model, texture and whole-WAD surgery. `identity-test` is the correctness oracle: it proves
the parse/serialize/CSUM model reproduces a real block byte-for-byte before any edit runs.

## Synopsis

```
wad_builder <COMMAND>

wad_builder identity-test        --block <BIN> [--inspect <NAME>]
wad_builder extract-lua          --block <BIN> --script <NAME> --out <LUAC>
wad_builder replace-lua          --block <BIN> --script <NAME> --luac <LUAC> --out <BIN>
wad_builder fix-model-mtrl       --block <BIN> --out <BIN>
wad_builder fix-model-vertices   --block <BIN> --out <BIN>
wad_builder unwrap-mesh          --block <BIN> --out <BIN> [--drop-slots]
wad_builder reskin-eyes          --block <BIN> --bone <N> [--bone <N> ...] --out <BIN>
wad_builder set-tex-specular     --block <BIN> --out <BIN>
wad_builder rebuild-resident-tex --block <BIN> --page <BIN> [--page <BIN> ...] --out <BIN>
wad_builder repoint-tex          --block <BIN> --remap <OLD:NEW> [--remap ...] --out <BIN>
wad_builder build-atlas          --block <BIN> [--block <BIN> ...] --out <BIN>
wad_builder list-blocks          --patch-wad <WAD>
wad_builder dump-block           --patch-wad <WAD> --path <SUBSTR> --out <BIN>
wad_builder replace-block        --patch-wad <WAD> --path <SUBSTR> --data <BIN> --out <WAD>
wad_builder filter-keep          --patch-wad <WAD> --keep <SUBSTR> [--keep ...] --out <WAD>
wad_builder drop-blocks          --patch-wad <WAD> --drop <SUBSTR> [--drop ...] --out <WAD>
wad_builder merge-blocks         --patch-wad <WAD> --from <WAD> --block <SUBSTR> [--block ...] --out <WAD>
wad_builder build-skin           --patch-wad <WAD> [--script <NAME>] --luac <LUAC> --out <WAD>
```

The top-level binary takes no options other than `-h`/`--help`; all work is done through a
required subcommand. On success the process exits `0`; on any error it prints
`wad_builder: error: <message>` to stderr and exits with a non-zero (failure) code.

## Two data layers (read this before the option tables)

`wad_builder` works on two completely different kinds of file, and every subcommand belongs
to exactly one of them. Mixing them up is the most common usage error.

1. **Decompressed engine blocks** (`--block`, `.bin`). These are the *contents* of one WAD
   block after sges-decompression. There are two sub-shapes:
   * **Multi-entry container table** — `u32 entry_count` + `entry_count ×
     {name_hash, type_hash, field_c, chunk_size}` + the UCFX containers. This is the
     `scripts_vz` block shape and also how model blocks are laid out. `identity-test`,
     `extract-lua`, `replace-lua`, `fix-model-mtrl`, `fix-model-vertices`, `unwrap-mesh`,
     `reskin-eyes`, `repoint-tex`, and `build-atlas` parse this shape (via `ScriptsBlock`)
     and edit **every** container entry in it.
   * **Single UCFX texture block** — `count(4)` + one 16-byte table row + a UCFX container
     at offset 20 + `CSUM` trailer. `set-tex-specular` and `rebuild-resident-tex` read this
     shape directly at fixed offsets (they assume exactly one entry, UCFX at +20). Note this
     is the *same* layout as a one-entry container table, so a single-entry `.bin` can be
     both `build-atlas`-packed and `set-tex-specular`-flagged.
2. **Patch WADs** (`--patch-wad` / `--from`, `.wad`). Whole `FFCS` `vz-patch` WADs. `list-blocks`,
   `dump-block`, `replace-block`, `filter-keep`, `drop-blocks`, and `merge-blocks` operate
   here. `build-skin` is the end-to-end bridge: it reaches *into* a patch WAD's `scripts_vz`
   block and does the `replace-lua` edit, then rebuilds the WAD.

The normal workflow chains the two layers: `dump-block` a block out of a WAD → edit the
decompressed `.bin` with a block-level command → `replace-block` it back into the WAD.

## Options

There are no global options besides `--help`. The table below is the union of every
subcommand flag; per-subcommand detail (required/repeatable/interaction) is in
[Subcommands](#subcommands).

| Flag | Value | Belongs to | Required | Repeatable | Default | Meaning |
|------|-------|-----------|----------|-----------|---------|---------|
| `--block` | path / substring | identity-test, extract-lua, replace-lua, fix-model-mtrl, fix-model-vertices, unwrap-mesh, reskin-eyes, set-tex-specular, rebuild-resident-tex, repoint-tex, build-atlas, **merge-blocks** | see note | in `build-atlas` and `merge-blocks` | — | In most subcommands: a single input decompressed block `.bin` path. In `build-atlas`: repeatable, one `.bin` path per source block. In `merge-blocks`: repeatable, a **case-insensitive path substring** selecting which blocks to copy (not a file path). **Required-ness:** where the subcommand cannot run without it, it is enforced at runtime, not by the parser — except `build-atlas`, where an empty `--block` is *not* rejected and silently writes an empty atlas block (see that subcommand). |
| `--inspect` | string | identity-test | no | no | `wifpmcinterior` | Script name whose container layout is dumped in detail. |
| `--script` | string | extract-lua, replace-lua, build-skin | yes for extract/replace-lua; optional for build-skin | no | build-skin: `wifpmcinterior` | Script selected by name (hashed with `pandemic_hash_m2`). |
| `--out` | path | every command except identity-test and list-blocks | yes | no | — | Output file (a `.bin` for block edits, a `.wad` for WAD edits). |
| `--luac` | path | replace-lua, build-skin | yes | no | — | New compiled LuaQ bytecode; must begin with the `\x1bLua` header. |
| `--drop-slots` | flag | unwrap-mesh | no | no | `false` | Switches `unwrap-mesh` from AREA-strip to full MESH-slot removal. |
| `--bone` | u8 | reskin-eyes | runtime¹ | yes | — | HIER bone index bound to each MESH eye slot, in descriptor order. |
| `--page` | path | rebuild-resident-tex | runtime¹ | yes | — | Streaming page `.bin` bodies in **descending** mip order (P003, P002, P001). |
| `--remap` | `OLD:NEW` hex | repoint-tex | runtime¹ | yes | — | 8-hex-digit texture-hash remap pair; `0x` prefix optional. |
| `--patch-wad` | path | list-blocks, dump-block, replace-block, filter-keep, drop-blocks, merge-blocks, build-skin | yes | no | — | Input/target patch WAD. In `merge-blocks` this is the **target** the blocks are added into. |
| `--from` | path | merge-blocks | yes | no | — | Source patch WAD to copy blocks *from*. |
| `--path` | string | dump-block, replace-block | yes | no | — | Case-insensitive path **substring** selecting one block. |
| `--data` | path | replace-block | yes | no | — | New **decompressed** block bytes (the tool sges-compresses them). |
| `--keep` | string | filter-keep | runtime¹ | yes | — | Case-insensitive substring(s) of blocks to KEEP. |
| `--drop` | string | drop-blocks | runtime¹ | yes | — | Case-insensitive substring(s) of blocks to DROP. |

> **¹ `runtime`** — these are repeatable `clap` `Vec` flags, so the argument parser does **not** mark
> them required; the subcommand instead fails at runtime with an explicit error if the vector is empty
> or matches nothing. (Exception: `build-atlas --block` has no such guard — see that subcommand.)

## Subcommands

### `identity-test` — correctness oracle (read-only)

Verifies the container model on a real block: it (1) parses the block, (2) re-verifies every
container's trailing CSUM, (3) re-serializes and asserts the bytes are **identical** to the
input, and (4) locates one script and dumps its container layout (UCFX `data_base`, BINN
descriptor offset + body size, LuaQ offset/length, CSUM offset + stored value) plus a
cross-check that `BINN.body_size == LuaQ length`.

| Flag | Required | Default | Notes |
|------|----------|---------|-------|
| `--block` | yes | — | Decompressed `scripts_vz` block `.bin`. |
| `--inspect` | no | `wifpmcinterior` | Script whose layout is printed. |

Writes nothing. Non-destructive; use it to confirm a block parses cleanly before editing.

```
wad_builder identity-test --block scripts_vz.bin --inspect wifpmcinterior
```

Prints the parse summary, `✓ CSUM verified on all N containers`, `✓ Round-trip identity`,
and the inspected script's field offsets.

### `extract-lua` — pull one script's LuaQ bytecode out

Finds the entry whose name hashes to `--script` and writes its raw LuaQ bytecode (the bytes
between the `\x1bLua` signature and the `CSUM` trailer) to `--out`.

| Flag | Required | Notes |
|------|----------|-------|
| `--block` | yes | Multi-entry block containing the script. |
| `--script` | yes | Script name. |
| `--out` | yes | Output `.luac`. |

```
wad_builder extract-lua --block scripts_vz.bin --script wifpmcinterior --out interior.luac
```

Produces `interior.luac` — the decompiler-ready LuaQ chunk for that one script.

### `replace-lua` — swap one script's bytecode and re-emit the block

Reads new LuaQ from `--luac` (rejected unless it starts with `\x1bLua`), replaces the named
script's bytecode by rebuilding its whole container (`ScriptsBlock::replace_lua`: `INFO` and
`DEPS` re-derived from the new bytecode by `mercs2_formats::script_meta`, keeping the script's
name; then `BINN`, the descriptor table and a fresh CSUM), and self-verifies: it re-parses the rebuilt block and re-checks every
CSUM. If the new bytecode is byte-identical to the old, it additionally asserts the whole
block reproduced the input byte-for-byte (an identity safety net).

| Flag | Required | Notes |
|------|----------|-------|
| `--block` | yes | Source multi-entry block. |
| `--script` | yes | Script to replace. |
| `--luac` | yes | New LuaQ (`\x1bLua`-headed). |
| `--out` | yes | Rebuilt block `.bin`. |

**Constraint:** the new bytecode must still import every module the script's old `DEPS`
listed (a dropped load dependency can leave a module the script uses unloaded); otherwise the
command fails naming the missing hashes.

```
wad_builder replace-lua --block scripts_vz.bin --script wifpmcinterior --luac new.luac --out scripts_vz.new.bin
```

### `fix-model-mtrl` — repair transposed MTRL material counts

Walks **every** container entry in the block and fixes the multi-material MTRL count
transposition left by the shared UCFX byte-swap converter: material records `[1..]` have
their `[u16 flags][u16 count]` halves swapped, so the engine reads a bogus `count` and
overruns its fixed 10-slot texture array in `Mtrl_Parse`. Each record whose high (count)
half is out of the 1..=10 range but whose low half is valid gets its two u16 halves swapped
back; `material[0]` (already PC-form) is left as-is. Recomputes each edited container's CSUM,
then re-parses and re-verifies before writing.

| Flag | Required | Notes |
|------|----------|-------|
| `--block` | yes | Decompressed model block. |
| `--out` | yes | Fixed block. |

```
wad_builder fix-model-mtrl --block sarah_model.bin --out sarah_model.mtrlfix.bin
```

Prints per-entry `transposed N material count-pair(s)` and a total.

### `fix-model-vertices` — un-transpose FLOAT16 vertex positions

Walks each STRM group's vertex buffer and un-transposes character-mesh positions that the
generic u32 swap left swapped (`x↔y`, `z↔w`; the homogeneous `w=1.0` ends up in the z slot →
a flat/degenerate mesh). It is **self-detecting**: a position with f16 `1.0` (`0x3C00`) at
byte +6 is already correct and skipped; only the known transposed signature (`1.0` at +4,
not at +6) is rewritten. Recomputes CSUM and self-verifies.

| Flag | Required | Notes |
|------|----------|-------|
| `--block` | yes | Decompressed model block. |
| `--out` | yes | Fixed block. |

```
wad_builder fix-model-vertices --block sarah_model.bin --out sarah_model.vfix.bin
```

### `unwrap-mesh` — MESH-region eye-slot surgery

Operates on static-`MESH` sub-meshes inside a skinned character model (they use the static
pipeline, which the skinned-mesh consumer cannot load → zero-size vertex buffer → crash).
Two modes, chosen by `--drop-slots`:

* **default (AREA-strip):** removes only the `AREA` bounding-volume chunk-groups from the
  MESH regions. This is the AREA-removal primitive; on its own it is insufficient to make the
  model load (the MESH wrapper still hides the subtree), kept for reference.
* **`--drop-slots`:** removes the MESH slots *entirely* (wrapper + INFO + AREA + inner
  PRMG/STRM/IBUF/PRMT), decrements the GEOM mesh-slot count, trims the INDX slot-index list,
  trims the SEGM table, and fixes the reverse-sibling `u3` indices of the surviving GEOM
  children. Yields an "eyeless" de-risk build whose remaining skinned groups all load.

Both rebuild the descriptor table, sibling spans, body region + offsets, `data_base`,
`n_desc`, and recompute CSUM; then self-verify.

| Flag | Required | Default | Notes |
|------|----------|---------|-------|
| `--block` | yes | — | Skinned model block. |
| `--out` | yes | — | Output block. |
| `--drop-slots` | no | `false` | Full slot removal instead of AREA-strip. |

```
wad_builder unwrap-mesh --block sarah_model.bin --out sarah_eyeless.bin --drop-slots
```

### `reskin-eyes` — convert static MESH eye slots into skinned groups

Re-rigs the static `MESH` eye slots into `SKIN` groups so the skinned consumer loads them
like every other group, keeping all slots (no removal). Per slot it: retags `MESH`→`SKIN`,
drops the inner `AREA` chunk, rewrites each PRMG INFO(60 PgMesh)→INFO(56 PgSkin) using
Obama's skinned-eye template (selecting the shader pair by whether the source decl has a
TANGENT channel), re-encodes each STRM decl 24→32 (inserting BLENDINDICES + BLENDWEIGHT
before NORMAL) and re-packs the vertex data at the new stride, translates the eye positions
into model space by the eyeball bone's HIER world translation, binds every vertex to the head
bone (index 25), and fixes the `u3`/`u4` sibling bookkeeping. Rebuilds body/offsets/CSUM and
self-verifies.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--block` | yes | no | Skinned model block with static MESH eye slots. |
| `--bone` | runtime¹ | yes | HIER **eyeball**-bone index for each MESH slot, in descriptor order. Repeat once per slot. |
| `--out` | yes | no | Output block. |

Requires at least one `--bone`; if a container has more MESH slots than bones supplied, that
container is left unchanged with `entry #i: no reskin (need N bone(s), got M)` and the run
continues. Reads the model's `HIER` chunk to compute the
per-slot world-space translation, so the block must contain a HIER.

```
wad_builder reskin-eyes --block sarah_model.bin --bone 38 --bone 39 --out sarah_eyes_skinned.bin
```

### `set-tex-specular` — set the specular-map flag on a texture block

Sets `INFO byte[8] = 0x20` on a single UCFX texture block (working `_sm` specular textures
carry this flag; a missing flag faults the model bind), then recomputes the block CSUM
(`crc32_mercs2` over `[20 .. pre-CSUM]`). **Idempotent:** if the flag is already `0x20` it
writes an unchanged copy and reports so.

| Flag | Required | Notes |
|------|----------|-------|
| `--block` | yes | Single UCFX texture block (`.bin`, UCFX at offset 20). |
| `--out` | yes | Output block. |

```
wad_builder set-tex-specular --block gun_sm.bin --out gun_sm.spec.bin
```

### `rebuild-resident-tex` — fill a streamed texture's resident body from its pages

A streamed texture's resident block (P000_Q3) ships an empty/black body; the real pixels live
in the P-tier streaming pages. This concatenates the pages' BODY/data chunks in the given
order, truncates to the resident body size, overwrites the resident body in place, and
recomputes the CSUM. Pass the pages in **descending** mip order (P003 = mip0 first, then
P002, then P001).

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--block` | yes | no | Resident block (`.bin`) whose body is overwritten (INFO/NAME kept). |
| `--page` | runtime¹ | yes | A streaming page `.bin`; its largest BODY/data chunk is concatenated. Descending mip order. |
| `--out` | yes | no | Output resident block. |

The concatenated page bodies must be at least as large as the resident body, otherwise it
errors (`page concat (..) is smaller than the resident body (..) — missing a tier?`).

```
wad_builder rebuild-resident-tex --block road_dm_P000.bin --page road_dm_P003.bin --page road_dm_P002.bin --page road_dm_P001.bin --out road_dm.resident.bin
```

### `repoint-tex` — remap MTRL texture hashes in a model block

Walks every container's MTRL material array and replaces any texture-slot hash that matches
an `OLD` key with its `NEW` value (e.g. redirect a dropped secondary map to a base-resident
global). Recomputes CSUM per edited container and self-verifies.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--block` | yes | no | Decompressed model block. |
| `--remap` | runtime¹ | yes | `OLD:NEW` 8-hex-digit pair; `0x` prefix accepted. Duplicate OLD keys: the last one wins (map insert). |
| `--out` | yes | no | Output block. |

Each pair must contain a `:`, and both halves must parse as hex, else the run errors.

```
wad_builder repoint-tex --block tank_model.bin --remap 1a2b3c4d:deadbeef --remap 0xAABBCCDD:0x11223344 --out tank_model.repointed.bin
```

### `build-atlas` — pack single-entry texture blocks into one multi-entry block

Reads N decompressed single-entry blocks, collects **all** their container entries, and emits
one multi-entry atlas block (the resident-atlas layout: count + table + UCFX bodies). Re-parses
and re-verifies all CSUMs before writing.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--block` | **no (unenforced)** | yes | Each source single-entry block. Repeat `--block` once per input. Unlike the other repeatable flags this has **no runtime guard**: passing zero `--block` succeeds and writes an atlas with no entries. |
| `--out` | yes | no | Output multi-entry atlas block. |

```
wad_builder build-atlas --block eye_l.bin --block eye_r.bin --block iris.bin --out eyes_atlas.bin
```

Produces `eyes_atlas.bin` with one entry per input container. Passing no `--block` produces a valid
but empty atlas block — the tool does **not** stop you.

### `list-blocks` — enumerate a patch WAD (read-only)

Prints every block's path, compressed size, `packed_field` (INDX page count) and ASET entry
count, plus each ASET record (asset hash + three u32s), and the WAD's `csum_value`.

| Flag | Required | Notes |
|------|----------|-------|
| `--patch-wad` | yes | WAD to inspect. |

```
wad_builder list-blocks --patch-wad vz-patch-human.wad
```

### `dump-block` — decompress one block out of a WAD

Finds the block whose path contains `--path` (case-insensitive substring), sges-decompresses
it, and writes the raw decompressed bytes to `--out`.

| Flag | Required | Notes |
|------|----------|-------|
| `--patch-wad` | yes | Source WAD. |
| `--path` | yes | Path substring selecting the block. |
| `--out` | yes | Output decompressed `.bin`. |

```
wad_builder dump-block --patch-wad vz-patch-human.wad --path scripts_vz --out scripts_vz.bin
```

### `replace-block` — put an edited decompressed block back into a WAD

sges-**compresses** the `--data` file, replaces the block matching `--path` (case-insensitive
substring), and rebuilds the WAD (same-path replace). **Critically**, it recomputes
`packed_field` (the INDX page count) from the *decompressed* size as `ceil(size / 0x8000)` and
sets it exactly — this word sizes the engine's decompression destination buffer
(`page_count << 15`, engine `FUN_00875b00`); a stale/too-small value overruns the heap.

| Flag | Required | Notes |
|------|----------|-------|
| `--patch-wad` | yes | Target WAD. |
| `--path` | yes | Substring selecting the block to replace. |
| `--data` | yes | **Decompressed** new block bytes. |
| `--out` | yes | Output WAD. |

```
wad_builder replace-block --patch-wad vz-patch-human.wad --path scripts_vz --data scripts_vz.new.bin --out vz-patch-human.new.wad
```

### `filter-keep` — rebuild a WAD keeping only matched blocks

Keeps only blocks whose path contains one of the `--keep` substrings (case-insensitive) and
rebuilds the WAD, preserving each kept block's compressed data + ASET entries and the WAD's
`csum_value` and cert blob. Errors if nothing matched.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--patch-wad` | yes | no | Source WAD. |
| `--keep` | runtime¹ | yes | Substrings to keep. |
| `--out` | yes | no | Output WAD. |

```
wad_builder filter-keep --patch-wad vz-patch.wad --keep scripts_vz --keep pmc_hum --out vz-patch.slim.wad
```

### `drop-blocks` — rebuild a WAD dropping matched blocks (inverse of filter-keep)

Drops every block whose path contains one of the `--drop` substrings (case-insensitive) and
rebuilds the WAD from the survivors, preserving compressed data + ASET + `csum_value` + cert
blob. Errors if nothing matched.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--patch-wad` | yes | no | Source WAD. |
| `--drop` | runtime¹ | yes | Substrings to drop. |
| `--out` | yes | no | Output WAD. |

```
wad_builder drop-blocks --patch-wad vz-patch.wad --drop scripts_vz --out vz-patch.noscripts.wad
```

### `merge-blocks` — copy blocks from one WAD into another

Copies blocks whose path contains a `--block` substring (case-insensitive) from `--from`
(source) into `--patch-wad` (target), preserving each copied block's compressed data + ASET
entries and replacing any same-path block already in the target. For each copied block it
decompresses to check `packed_field` and **bumps it up** to `ceil(decompressed / 0x8000)` if
the source carried an undersized value (it never shrinks it). Errors if no source block
matched.

| Flag | Required | Repeatable | Notes |
|------|----------|-----------|-------|
| `--patch-wad` | yes | no | **Target** WAD (blocks are added into this). |
| `--from` | yes | no | **Source** WAD to copy from. |
| `--block` | runtime¹ | yes | Substrings selecting source blocks. |
| `--out` | yes | no | Output WAD. |

```
wad_builder merge-blocks --patch-wad vz-patch.wad --from vz-patch-human.wad --block pmc_hum_sarah --out vz-patch.merged.wad
```

### `build-skin` — end-to-end: patch a script inside a WAD and rebuild it

The one-shot bridge across both layers. It finds the `scripts_vz` block in the target WAD,
sges-decompresses it, does the `replace-lua` edit on the named script, self-verifies the
edited block's CSUMs, sges-**recompresses** it (asserting a decompress round-trip), swaps the
compressed data back into the block (keeping `packed_field`/flags/ASET, since the decompressed
page count is unchanged), rebuilds the WAD, then re-reads the output and asserts the edit is
present.

| Flag | Required | Default | Notes |
|------|----------|---------|-------|
| `--patch-wad` | yes | — | Source WAD to edit (everything but `scripts_vz` is preserved). |
| `--script` | no | `wifpmcinterior` | Script to replace inside `scripts_vz`. |
| `--luac` | yes | — | New LuaQ (`\x1bLua`-headed). |
| `--out` | yes | — | Output WAD. |

```
wad_builder build-skin --patch-wad vz-patch-human.wad --luac interior.luac --out vz-patch-human.skinned.wad
```

## How the options combine

This section is the load-bearing one: how flags interact and what changes in the **output**.

**The two layers never mix in one command.** `--block` commands read/write decompressed
`.bin` files; `--patch-wad` commands read/write `.wad` files. There is no flag to make a
block command open a WAD or vice-versa. The realistic pipeline is a chain:
`dump-block` (WAD→bin) → a block edit (bin→bin) → `replace-block` (bin→WAD). `build-skin`
is the fused shortcut for exactly the `scripts_vz`/`replace-lua` case of that chain.

**`--block` vs `--out` are always distinct files;** every mutating command writes a fresh
`--out` and never edits in place. Nothing is written until the tool has re-parsed its own
output and re-verified CSUMs, so a failed self-check aborts *before* `--out` is created.

**Block-shape must match the command.** `set-tex-specular` and `rebuild-resident-tex` read a
*single* UCFX texture block at fixed offsets (UCFX at +20). The container-table commands
(`fix-model-*`, `unwrap-mesh`, `reskin-eyes`, `repoint-tex`, `build-atlas`, the `-lua`
commands) iterate the multi-entry table and touch **every** entry. A single-entry block is
valid input to both families; a genuine multi-entry model block fed to `set-tex-specular`
would be misread (it only looks at the first UCFX at +20).

**`unwrap-mesh --drop-slots` changes the operation, not just a parameter.** Without it the
command strips only AREA chunks (a no-op for loadability on its own). With it the command
performs the full slot deletion with GEOM/INDX/SEGM/`u3` rebookkeeping. The two produce
structurally different output blocks from the same input; they are mutually exclusive modes.

**`reskin-eyes --bone` is positional-by-repetition.** The i-th `--bone` binds the i-th MESH
slot (descriptor order). Order matters, and you must supply at least as many bones as any
container has MESH slots or that container errors out. This is the only command where the
*count* of a repeated flag is coupled to the input's structure.

**`rebuild-resident-tex --page` order is semantic.** Pages are concatenated in the order
given, which must be descending mip (P003, P002, P001). Reordering them produces a different
(wrong) mip chain. The concat is then truncated to the resident body size, so supplying more
tail data than needed is harmless but supplying too little errors.

**`repoint-tex --remap` accumulates into a map.** Multiple `--remap` pairs build one
old→new table; if two pairs share the same OLD key the later pair wins (HashMap insert). Only
slots whose current hash is a key are touched; unlisted hashes pass through.

**`build-atlas --block` order sets entry order.** Inputs are concatenated in the order given;
the atlas's entry table follows that order. All entries from every input are kept (no dedup).

**Path/keep/drop/from matching is always a case-insensitive substring, and can match more
than one block.** `--path` (dump-block, replace-block) acts on the *first* match; `--keep`,
`--drop`, and `--block` (merge-blocks) act on *all* matches. A too-broad substring silently
sweeps in neighbours — e.g. `--drop tex` drops every path containing "tex".

**`filter-keep`/`drop-blocks` vs `replace-block`/`merge-blocks` take different rebuild
paths, which matters for `packed_field`.**
* `filter-keep` and `drop-blocks` rebuild with `build_patch_wad_multi`, copying the surviving
  blocks **verbatim** (compressed data, ASET, and existing `packed_field` untouched). They
  never re-examine page counts — they only ever *remove* blocks, so no block grows.
* `replace-block` recompresses new decompressed data and sets `packed_field` **exactly** to
  what that decompressed size needs.
* `merge-blocks` copies compressed data verbatim but **bumps `packed_field` up** if the source
  WAD undersized it (it never shrinks it).
So if you need to *correct* a stale page count, route through `replace-block` (or
`merge-blocks`), not `filter-keep`.

**`replace-block` vs `merge-blocks` — where the new bytes come from.** `replace-block` takes
a **decompressed** `--data` file and compresses it (so it changes a block's contents).
`merge-blocks` takes an already-built block from another WAD and copies its **compressed**
data (so it moves a block between WADs unchanged apart from the page-count bump). Both use a
same-path replace, so both overwrite an existing same-path block in the target.

**`build-skin` == `dump-block` + `replace-lua` + `replace-block`, fused and validated.** It
keeps `packed_field`/flags/ASET because a bytecode swap does not change the decompressed page
count; if your edit *did* change the decompressed size across a 32 KB boundary you would
instead go the `dump-block`/`replace-lua`/`replace-block` route (which recomputes the page
count). `--script` on `build-skin` selects which script inside `scripts_vz` is patched and
defaults to `wifpmcinterior`.

## Examples

```
# 1. Prove a block parses and round-trips before you touch it.
wad_builder identity-test --block scripts_vz.bin
#  → CSUMs verified, re-serialized bytes == input; safe to edit.

# 2. Full script-swap pipeline, layer by layer.
wad_builder dump-block   --patch-wad vz-patch-human.wad --path scripts_vz --out scripts_vz.bin
wad_builder replace-lua  --block scripts_vz.bin --script wifpmcinterior --luac new.luac --out scripts_vz.new.bin
wad_builder replace-block --patch-wad vz-patch-human.wad --path scripts_vz --data scripts_vz.new.bin --out vz-patch-human.new.wad
#  → vz-patch-human.new.wad with the patched script and a corrected page count.

# 3. Same result in one command.
wad_builder build-skin --patch-wad vz-patch-human.wad --luac new.luac --out vz-patch-human.new.wad
#  → identical outcome to example 2 for a same-size (page-count-preserving) edit.

# 4. Repair a converted character model, then re-inject.
wad_builder fix-model-mtrl     --block sarah.bin       --out sarah.m.bin
wad_builder fix-model-vertices --block sarah.m.bin     --out sarah.mv.bin
wad_builder reskin-eyes        --block sarah.mv.bin --bone 38 --bone 39 --out sarah.fixed.bin
wad_builder replace-block --patch-wad vz-patch.wad --path pmc_hum_sarah --data sarah.fixed.bin --out vz-patch.sarah.wad
#  → material counts un-transposed, positions un-flipped, eyes reskinned; block back in the WAD.

# 5. Rebuild a streamed texture's resident body and flag it specular.
wad_builder rebuild-resident-tex --block t_P000.bin --page t_P003.bin --page t_P002.bin --page t_P001.bin --out t.res.bin
wad_builder set-tex-specular     --block t.res.bin  --out t.res.spec.bin
#  → resident body filled from the streaming pages, INFO byte[8]=0x20, CSUMs recomputed.

# 6. Slim a WAD down to just the blocks you ship.
wad_builder list-blocks  --patch-wad vz-patch.wad
wad_builder filter-keep  --patch-wad vz-patch.wad --keep scripts_vz --keep pmc_hum_sarah --out vz-patch.slim.wad
#  → a WAD containing only the two named block families.

# 7. Move one block between WADs.
wad_builder merge-blocks --patch-wad vz-patch.wad --from vz-patch-human.wad --block pmc_hum_sarah --out vz-patch.merged.wad
#  → Sarah's block copied into vz-patch.wad, page count bumped if the source undersized it.
```

## Failure modes

Every message below is printed as `wad_builder: error: <message>` and exits non-zero. Any file
I/O error (`read: <e>` / `write: <e>`) also lands here.

**Parse / container-model errors (block commands)**
* `not a UCFX container` / `not a UCFX texture block` — the input isn't the expected block
  shape (wrong file, or a multi-entry block fed to a single-UCFX command).
* `no BINN descriptor`, `missing CSUM trailer`, `descriptor table overruns container` — the
  container is malformed or truncated.
* `walk issue: …` / `entry/container count mismatch` — the multi-entry table header and its
  containers disagree; the block is corrupt or not a `scripts_vz`-shaped block.
* `round-trip mismatch: in=… out=… (first diff at …)` (`identity-test`) — the serialize model
  does not reproduce the input; the block uses a layout the model doesn't cover.
* `CSUM mismatch: stored 0x… computed 0x…` — a container's stored checksum is wrong (input
  already corrupt, or an edit path failed to recompute it).

**Script / Lua errors**
* `'<name>' not found` / `'<inspect>' not found (pandemic_hash_m2 mismatch)` — no entry hashes
  to that name.
* `luac file is not LuaQ bytecode (missing \x1bLua header)` — `--luac` isn't compiled LuaQ.
* `script <name>: the new bytecode no longer imports N of its retail DEPS (0x…); keep every
  import the script had` — `replace-lua`/`build-skin` refuse bytecode that drops a `DEPS` entry.
* `identical-LuaQ replace did NOT reproduce input block` — the identity safety net tripped
  (indicates a bug in the rebuild path, not user error).

**Model-surgery errors**
* `no --remap pairs given` / `bad --remap '<r>' (want OLD:NEW)` / `bad old hex …` / `bad new
  hex …` (`repoint-tex`).
* `no --bone given …` (`reskin-eyes`).
* Per-entry model errors are **not** fatal: `fix-model-mtrl`, `fix-model-vertices`,
  `unwrap-mesh`, `repoint-tex` and `reskin-eyes` print `entry #i: no … (<message>)` for a
  container they cannot edit, leave it unchanged, and go on (exit `0` once the output
  self-verifies). The messages include `no MTRL chunk in container` / `MTRL body out of range`
  (`fix-model-mtrl`, `repoint-tex`) and `need N bone(s), got M` / `no HIER chunk` / `STRM
  already has blend channels` / `STRM decl has no NORMAL element` (`reskin-eyes`).

**Texture errors**
* `no INFO chunk` / `no CSUM trailer` (`set-tex-specular`).
* `no --page given …` / `no BODY/data chunk` / `page concat (…) is smaller than the resident
  body (…) — missing a tier?` (`rebuild-resident-tex`).

**Patch-WAD errors**
* `Not an FFCS WAD` — `--patch-wad`/`--from` isn't a patch WAD.
* `no block matching '<path>'` (`dump-block`, `replace-block`) — the substring matched nothing.
* `no blocks matched --keep` / `no blocks matched --drop` / `no source blocks matched --block`
  — a filter/merge selected nothing (would produce an empty or unchanged WAD, so it aborts).
* `no scripts_vz block in patch WAD` / `'<script>' not in scripts_vz` / `sges recompress
  round-trip mismatch` / `output scripts_vz LuaQ != edited bytecode` (`build-skin`) — the
  target lacks a `scripts_vz` block, the script isn't in it, or a self-check on the rebuilt
  WAD failed.
