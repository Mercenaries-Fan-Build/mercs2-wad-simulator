# Workshop "Mods" rebuild — Plan 05: the remaining scope

**Status:** SCOPE (2026-07-28). Continues Plan 01's phasing after increment 6.
**Siblings:** `-01-mod-model.md`, `-02-navigation.md`, `-03-live-bridge.md`, `-04-manifest-format.md`

## Context

`mercs2_quartermaster` is built far enough to be useful and not far enough to ship. Two recipes —
`replace_texture` and `add_model` — build end-to-end against the retail `vz.wad` and pass
`wad_simulator` with zero violations. 99 tests, none `#[ignore]`d.

But the recipe the community actually wants (`add_outfit`) cannot lower at all, and the seven
*most dangerous* linter rules are registered-but-unimplemented. This document exists because that
remaining work has been discovered piecemeal, one blocker at a time, and the shape of it should be
visible before more of it is picked up.

Two structural facts dominate everything below, so they are stated once:

1. **Almost every proven implementation lives in a binary-only crate.** `wad_simulator`,
   `mercs2_probe`, `wad_builder` and `mercs2_workshop` all lack a `src/lib.rs`. Plan 01's "wrap the
   existing building blocks, don't reimplement them" is therefore *impossible* for most of them
   until they are extracted. This has now blocked three separate pieces of work (donor resolution,
   glTF import, and now two linter rules).
2. ~~**`mercs2_luac` has no automated parity test.**~~ **RESOLVED 2026-07-28** — see below. It is now
   verified against retail bytecode, and the verification surfaced a linking landmine that changes
   where the linker can live.

---

## Step 1 — DONE (2026-07-28): `mercs2_luac` is verified, and it cannot link beside mlua

`crates/mercs2_luac/tests/parity.rs` compiles the vendored corpus and diffs it against the bytecode
retail actually shipped, read out of `scripts_vz`.

**Result: 113 of 113 corpus scripts compile, every chunk is the exact length retail shipped, and
0 bytes differ outside line-number debug info.** Codegen is identical to the shipping toolchain's.
Two things had to be pinned down:

- The chunk name is stored verbatim, and retail used the **bare script name** — no `@`, no `.lua`.
  Passing `@name.lua` made all 113 differ by exactly 5 bytes, which is what identified it. **The
  linker must use the bare name** or every script it touches will differ from retail.
- The rest is line numbers: the corpus is decompiled, so `unluac`'s line breaks sit ~100 lines out
  from the original. A test *proves* this rather than assuming it, by mapping where line info lives
  (bytes that move when the source is shifted, unioned over eight shift sizes) and checking every
  retail difference falls inside that map. An exact byte match against a decompiled corpus is not
  achievable and is not the property the linker needs.

### ⚠ `mercs2_luac` and `mercs2_script` cannot be linked into the same binary

Found the hard way: the parity test, first placed in `mercs2_script`, **SIGSEGV'd partway through
the corpus** — while the script it died on compiled perfectly on its own, and all 114 compiled fine
in a process that linked only `mercs2_luac`.

`mercs2_luac` vendors a patched **Lua 5.1**; `mercs2_script` links mlua's vendored **Lua 5.4**. Both
static libs export the same unprefixed C symbols (`_lua_newstate`, `_lua_pcall`, `_lua_close`, …,
verified with `nm`). The linker silently picks one definition, so a `lua_State` allocated by one
runtime is parsed by the other. **It is not a link error — it is a segfault at runtime**, and only
in binaries that happen to pull in both.

Consequences to design around:
- The Lua linker uses `mercs2_luac`. Whatever process hosts it **must not** also link
  `mercs2_script`. `mercs2_quartermaster` does not today, and must not start.
- Any tool wanting to both *run* Lua (mlua) and *compile* it (`mercs2_luac`) — plausibly the
  Workshop, with a script editor beside publishing — is blocked until the symbols are namespaced.
- The real fix is to prefix `mercs2_luac`'s vendored symbols (a generated rename header, or
  `objcopy --prefix-symbols` on the archive). Bounded, and worth doing before the collision is
  discovered by someone else at runtime.
- The corpus is therefore reached **by path**, not by depending on `mercs2_script` — which
  independently confirms the design note below that the linker should take the corpus root as a
  parameter.

## A. The Lua linker — blocks `add_outfit`, `patch_lua`, and Plan 01 phase 5

The highest-value item. Scripts load from the **block**, not per-hash, so editing one means
re-emitting all 114 — and two mods each shipping their own `scripts_vz` silently annihilate each
other. The fix is to link once across the installed set: concatenate each Shipment's declared
source-append onto the base script, compile once, emit one block.

**Already in place** (more than expected):
- `mercs2_formats::scripts_block` — `parse` / `serialize` / `find_by_name` / `extract_lua` /
  `replace_lua` / `verify_csums`. A real library.
- `mercs2_luac::compile(source, chunk_name)` — game dialect (float `lua_Number`, 4-byte `size_t`),
  header-verified before return.
- **The decompiled corpus is vendored in-tree**: `crates/mercs2_script/corpus/mercs2-luacd/src/`,
  370 scripts, reached via `mercs2_script::corpus::root()`. `vz/<name>.lua` ↔ the `scripts_vz`
  entry `pandemic_hash_m2(<name>)`. Verified: 114 `vz/` scripts, `wifpmcinterior.lua` present.
  **The linker can work offline** — it never needs to decompile.
- The manifest kind, blast-radius claim, merge class and `M0141` lint rule are all already written.
- `wad_builder::build_skin` (`main.rs:353`) is a complete reference flow, including a re-read-and-
  verify step worth copying.

### Step 4 progress — the linker core WORKS (2026-07-28)

`mercs2_quartermaster::link` links N Shipments' appends into one `scripts_vz`, verified against the
retail block. **`two_script_mods_both_survive_the_link` passes** — two independent wardrobe mods,
base 58,828 B → 59,143 B linked source → 80,301 B bytecode, entry count unchanged, CSUMs verifying,
LuaQ header correct. That is the annihilation this whole design exists to prevent, now prevented.

Also pinned: mutations on different scripts stay independent; an unknown target is *named* rather
than skipped (a mod whose script vanished would otherwise install "successfully" and do nothing); a
syntax error in a mod's append surfaces the compiler's own message with a line number; and a link
with no mutations leaves the block byte-identical.

Ordering is **by Shipment name, not install order** — two installs of the same set must produce
identical bytes, or verify-by-hash means nothing. Load order decides who *wins* a conflict; it does
not get to decide the bytes of a merge that has no conflict. Each append is attributed with a
`-- [Quartermaster] appended by Shipment: <name>` comment so a decompiled linked block is readable.

**Still to write:**
1. ~~The linker module itself.~~ **DONE** — core linking, ordering, error surfacing.
2. **Base-block acquisition.** Nothing resolves `scripts_vz` from a `GameStack`; `build_skin` only
   handles an *existing patch WAD*. Add a raw-block accessor, or match the PTHS path substring.
3. Corpus lookup by script name (`"wifpmcinterior"` → `vz/wifpmcinterior.lua`, searching
   resident/vz/shell/stubs) — small, currently absent.
4. Deterministic concatenation order across Shipments, consuming `manifest::Load.after/before`,
   which nothing reads today.
5. **Page-count recomputation.** `build_skin` skips it on a "size unchanged" assumption that N-mod
   concatenation breaks; a stale `packed_field` overruns the engine heap. Use
   `PatchBlock::from_decompressed`.
6. The `add_outfit` composition on top: append the `_tOutfits` row, and have the Quartermaster —
   not each Shipment — emit the availability-count lift exactly once.

**Known hazards to design around:**
- ~~`replace_lua` hard-errors on a metadata-bearing `BINN`.~~ **CLEARED 2026-07-28** by
  `mercs2_formats/tests/scripts_block_survey.rs`: all **114 containers surveyed — 0 metadata-bearing,
  0 unparseable, 0 with a non-script `type_hash`**. The restriction is theoretical for `scripts_vz`.
  The same file also pins that splicing identical bytecode back in reproduces the block **byte for
  byte** with CSUMs intact, so the rebuild path loses nothing before a mod is even involved.
- The corpus has 12 gaps plus goto-heavy scripts `unluac` could not round-trip. Those are scripts
  the linker structurally cannot target, and it should say so loudly rather than fail obscurely.
- A brand-new script name needs a script-UCFX-container builder, which does not exist. Defer.
- `MERGEABLE_SCRIPTS` (`blast.rs:138`) is an allowlist of exactly one entry and should widen only
  as fast as the linker is actually proven per target.

**Prerequisite worth doing first:** a parity regression for `mercs2_luac` — compile the vendored
corpus and diff against the bytecode extracted from retail `scripts_vz`. Every ingredient is in-tree.
Without it the linker rests on an unverified compiler.

---

## B. The seven HANG-class rules (`lint::PENDING`)

Silent and catastrophic: a modder gets a frozen loading screen, no error. Three are wrappable, one
is a small addition, three need real work.

| Code | Rule | Status |
|---|---|---|
| M0002 | `packed_field` under-claim → heap overrun | **Library fn exists** — `patch_wad::validate_blocks:162`. Needs a `_all()` variant; today first-error-wins and can't yield a `Vec<Diagnostic>` |
| M0001 | Dangling `_P001/2/3` rungs → 549 GB request, stream hang | **Algorithm exists**, 7 lines in `mercs2_probe/src/bin/aset_refcheck.rs:53`, inside a `main()`. Lift to `mercs2_formats` on top of `AsetEntry::lod_chain` |
| M0003 | BODY < `linear_mip_chain_size` → BUFFER_TOO_SMALL livelock | **Most mature** — `wad_simulator::texture::texture_buffer_too_small:109`, with two retail-verified false-positive gates (9,562 legitimately-short streamed bodies). Do **not** reimplement |
| M0004 | New hash minted with no ASET row → silent wedge | **Inverse only.** `aset_validate` answers the opposite question. The forward direction is a set difference (`block_internal_hashes − aset_hashes`) whose two halves sit adjacent in `simulate.rs:618-628` and is never taken |
| M0006 | `replace_texture` target shared by several materials | **No refcount anywhere.** Primitives exist (`parse_mtrl`, `mtrl_diffuse_hashes`); `simulate.rs:276` `xref_sources` keeps only the *first* referrer — making it a `Vec` yields the fan-in map nearly free. **M0009 already ships ~70% of this rule's real blast radius** |
| M0005 | Non-resident costume → `STATE_WAITFORGAME` wedge | **Nothing.** Captured only in a doc comment in `override_base_blocks.rs:1-13`. Needs a residency predicate and a costume classifier |
| M0008 | Small/non-square `page_count` livelock | **Nothing.** No code branches on `width != height`. The RE it rests on is `render_core_code_map.md` in the notes repo, which records the buffer-sizing livelock as a **known-open** converter fidelity issue — so this rule is blocked on research, not just implementation |

Suggested order: M0002 → M0001 → M0003 → M0004, then reassess M0006 (M0009 may already suffice)
and treat M0005/M0008 as research, not implementation.

**Blocker:** M0001 and M0003 need `mercs2_probe` and `wad_simulator` to gain a `[lib]`.

---

## C. Library extraction — the recurring tax

Give a `src/lib.rs` to the crates whose logic keeps being needed elsewhere. Doing this once removes
a blocker that has already appeared three times.

- `wad_simulator` — `texture_buffer_too_small`, `validate_chunk_invariants`,
  `run_aset_hash_validation`. Its own doc comment (`simulate.rs:62`) shows a `use wad_simulator::…`
  example **that cannot compile today**.
- `mercs2_probe` — the LOD-rung walk.
- `wad_builder` — `build_skin`, still flagged in `build.rs:17-22`.

---

## D. The `qm` CLI — does not exist

`mercs2_quartermaster` is library-only: no `src/bin/`, no `[[bin]]`. Every plan references
`qm build ./my-shipment` and `qm lint`, and the template repo's CI depends on `qm lint` existing.
Needs: `lint` (hermetic, no game), `build` (game-stack from `game_paths`/flags), gated on exit code
per the standing mandate.

## E. Consumption and publishing — the crate has no consumers wired

The stated architecture is "neither Workshop nor Modkit owns the format — the crate does, and both
are clients." Neither can consume it today:
- not in `[workspace.dependencies]`, so no `workspace = true` dep is possible
- at `0.1.0` while siblings are 1.x/3.x
- no `README.md`, which siblings have for crates.io

## F. Rule doc links are repo-relative to the *notes* repo

The linter's `Rule.doc` paths resolve against `~/src/mercenaries-game`, not against this repo —
`docs/modding/field_guide.md`, `docs/aset_format.md`,
`docs/reverse_engineer/render_core_code_map.md` and
`docs/modernization/texture_extraction_notes.md` all exist there. The content is fine; the *form*
is the problem, because `Diagnostic::Display` prints `— see {doc}` to a modder who has neither repo
checked out. When the crate is published and the template repo's CI starts emitting diagnostics,
these want to be URLs.

One genuine gap: **`docs/modding/manifest_format.md` is not written yet.** Plan 04 already names it
as the freeze deliverable ("when frozen, it graduates to the template repo README +
`docs/modding/manifest_format.md`"), so this is scheduled work, not a loose end.

## G. Remaining lowering

`edit_state_machine`, `native_hook` (place the `.asi` + the placement record), `raw`. Each currently
returns `Unsupported`.

## H. Format gaps recorded but unresolved

- `add_model` cannot say **which donor group** hosts the geometry — likely wants a `group:` field.
- Donor auto-pick is unimplemented; the builder asks rather than guessing.
- **Open-Q7** raw `PlayerVisibleName` — needs one in-game test; the corpus has no answer.
- **Open-Q10** platform axis — recorded, deliberately parked, PC only.

## I. Template repo `mercs2-shipment-template`

Standalone repo: folder skeleton, filled-in `manifest.yaml`, README, CI running **`qm lint` only**
(a public runner has no retail WADs). Depends on D.

---

## Suggested sequencing

1. **`mercs2_luac` parity regression** — cheap, and everything in A rests on it.
2. **BINN metadata survey** — decides whether the linker approach holds at all.
3. **C (library extraction)** — unblocks A, B and future work in one pass.
4. **A (the linker)** → `add_outfit` lowers → Plan 01 phase 5.
5. **B** in the order given.
6. **D → I**, then E, F, G, H as they become relevant.

**A note on the sibling repo.** The reversed knowledge this crate encodes lives in
`~/src/mercenaries-game` — the decompiled Lua, the ASET/texture format docs, the Ghidra corpus, and
the `corpus_search` index over all of it. Several things in this document were found there and not
by reading code, and two bugs shipped in this crate because I derived a format instead of searching
it first. **`corpus_status` then `corpus_search` before deriving anything about a format** is the
cheapest rule on this list.

## Verification

- `cargo test -p mercs2_quartermaster` — 99 tests today, none ignored; game-dependent ones discover
  the WAD via `game::discover` and skip loudly without one.
- Every new linter rule ships a **firing** and a **staying-quiet** fixture; a rule with only the
  former eventually fires on everything and gets ignored.
- Any new lowering path must, against the real `vz.wad`: build, re-read with `read_patch_wad`,
  assert the ASET row is primary (low-16 `0xFFFF`) and the block starts with a single-entry table,
  and produce byte-identical output across two builds.
- Then `cargo run --bin wad_simulator -- --wad <out> --base-wad <vz.wad> --skip-audio` — expect no
  `UCFX / FORMAT` findings and a clean verdict. **This has caught every structural bug so far and
  no digest check has caught any of them.**
- For the linker specifically: install two synthetic outfit Shipments and assert both outfits
  survive into the linked block — the failure this whole design exists to prevent.

## Out of scope

Plan 02 (Workshop navigation, the Quartermaster panel, the Settings page) and Plan 03 (the live
bridge). Also Modkit-side deploy/undo, which consumes the placement record this crate emits.
