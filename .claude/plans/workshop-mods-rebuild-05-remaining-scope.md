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

## A. The Lua linker — ★ DONE for the single-Shipment case (2026-07-28)

`add_outfit` and `patch_lua` both lower. Verified against retail: model `consumed=2776 issues=0`,
script `consumed=644 issues=0`, no violations. **Plan 01 phase 5's recipe builds.**

The Quartermaster generates the `_tOutfits` row (append-only — index 2 is reserved and a saved
costume is a *position*) and emits the availability lift **once**, derived from the final list
length. That once-ness is structural rather than conventional: two Shipments each hard-coding
`shipped + 1` produce the same number, the later definition wins, and one outfit is in the WAD, in
the table, and invisible.

**The cross-Shipment relink is DONE too** — `build::link_installed` takes every installed Shipment,
collects their mutations, links once, and emits `zz-quartermaster-link.wad` to be mounted LAST.
Because it is built from all their mutations together it is a superset of each per-Shipment block,
so whichever it shadows, it shadows with something strictly more complete. Verified against retail:
`script consumed=644 issues=0`, no violations, and `two_installed_shipments_both_survive_the_deploy_
link` asserts each Shipment alone knows nothing of the other while the linked block carries both.

The enabler was making `script_mutations` derive from the **manifest alone** — no game stack, no
lowering. Otherwise the deploy path would have had to re-run model injection just to discover which
scripts are touched. `lower` went back to producing blocks only.

Deploy order does not change the bytes (`the_deploy_link_is_order_independent`), and a set with no
script mods emits no overlay at all rather than one that merely restates the base block.

### Original scope, for reference

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
| M0002 | `packed_field` under-claim → heap overrun | ✅ **DONE** — `patch_wad::validate_blocks_all` + `lint::artifact_checks` |
| M0001 | Dangling `_P001/2/3` rungs → 549 GB request, stream hang | ✅ **DONE** — same pair. Written fresh in `mercs2_formats` rather than lifted from `mercs2_probe`: the rung walk is trivial, and the hard part turned out to be *when* it may run, not how |
| M0003 | BODY < `linear_mip_chain_size` → BUFFER_TOO_SMALL livelock | ✅ **DONE** — `lint::artifact_checks` wraps `wad_simulator::texture::check_embedded_texture_buffers` |
| M0004 | New hash minted with no ASET row → silent wedge | ✅ **DONE** — same stage. The set difference, with the invariant behind it measured against retail rather than assumed |
| M0006 | `replace_texture` target shared by several materials | **No refcount anywhere.** Primitives exist (`parse_mtrl`, `mtrl_diffuse_hashes`); `simulate.rs:276` `xref_sources` keeps only the *first* referrer — making it a `Vec` yields the fan-in map nearly free. **M0009 already ships ~70% of this rule's real blast radius** |
| M0005 | Non-resident costume → `STATE_WAITFORGAME` wedge | **Nothing.** Captured only in a doc comment in `override_base_blocks.rs:1-13`. Needs a residency predicate and a costume classifier |
| M0008 | Small/non-square `page_count` livelock | **Nothing.** No code branches on `width != height`. The RE it rests on is `render_core_code_map.md` in the notes repo, which records the buffer-sizing livelock as a **known-open** converter fidelity issue — so this rule is blocked on research, not just implementation |

Suggested order: ~~M0002 → M0001 → M0003 → M0004~~ — all four done. Next is reassessing M0006
(M0009 may already suffice); M0005/M0008 stay research, not implementation.

**Blocker:** M0003 needs `wad_simulator`'s `[lib]` — which now exists, so this is unblocked.

### Step complete — M0001 + M0002 (2026-07-28)

Both landed, and the shape they landed in is worth recording because it generalises.

Neither rule is answerable from the manifest, and neither is answerable from the game stack
either — they are properties of *the WAD the builder emits*. That made a **third lint stage** the
right home rather than forcing them into `lint` (hermetic) or `game_checks`:

```
lint            manifest text only          → CI, no game
game_checks     + the retail WADs           → the author's machine
artifact_checks + the WAD we just assembled → after lowering, before the write
```

`artifact_checks` is the only stage that can catch a defect **the lowering introduced** rather than
one the author wrote — which is precisely the class of bug that has actually shipped here twice (a
bare container where an entry-table block was required; an ASET rung left at `0x0000` instead of the
`0xFFFF` sentinel). Neither was visible in the manifest. Both were plain in the bytes.

Two things fell out that were not anticipated:

1. **The rung check has a stage hazard.** `build_patch_wad_multi` validates its input *before* it
   remaps LOD rungs into the patch's index space. Run the check there and every rung still points
   into the 11,370-block `vz.wad`, so nearly every carried block reports as dangling — a confident
   wrong answer, not an error. `patch_wad::BlockStage` now makes the caller name the index space, so
   the wrong answer is unrepresentable rather than merely documented.
2. **The duplicate-primary case is a real diagnostic** (M0180), not a fatal one. The registry is
   first-writer-wins and retail ships the shape, so it must not block a build — but a duplicate a
   *mod* introduces means one contribution silently does nothing, which the author wants to know.
   It had been reaching an `eprintln!` nobody captured.

Also promoted out of the block validator while it was open: M0181 (header-region overflow) and
M0182 (an emitted block that will not inflate).

`verify_emitted` runs the stage on both emit paths — the per-Shipment overlay and the
cross-Shipment link WAD — and runs it **before the write**, so a WAD that would hang the game never
reaches the disk where its presence would read as success.

Tests: 122 in `mercs2_quartermaster`, 280 in `mercs2_formats`, all green, none ignored. Every new
rule ships the firing/quiet fixture pair this document requires.

### Step complete — M0003 + M0004 (2026-07-28)

Both landed in `artifact_checks` alongside M0001/M0002, which makes four of the seven HANG-class
rules answerable in one stage. That is not a coincidence: every one of them is a property of the
BYTES, and the manifest is silent about all four.

**M0003 is wrapped, not written.** `wad_simulator` got its `[lib]` in step C, and `[workspace.
dependencies]` got a `wad_simulator` row so the linter could actually name it — that row was the
only thing still missing. The linter calls `texture::check_embedded_texture_buffers`, which pairs
each `INFO` descriptor with the `BODY` after it and defers to `texture_buffer_too_small`. It runs
over **every** container rather than only entries typed `TYPE_HASH_TEXTURE`: a model or layer
container that embeds a texture never receives a texture dispatch, so type-gating would skip exactly
the case with no other check. A third fixture pins the **streamed-texture gate** itself — that gate
now sits behind a crate boundary, and losing it turns the rule into one that fires on nearly every
texture in the game.

**M0004's invariant was measured, not derived.** Before writing it, the whole retail `vz.wad` was
swept: 11,370 blocks, 55,429 entry-table rows, 30,006 distinct hashes, 30,645 ASET rows — and
**zero** entry hashes without a row. Sub-resources reached through their parent are not an
exception; they get a non-primary row. The concern that motivated the sweep was the M0003 shape,
where a naive predicate fires on 9,562 legitimate cases; here there are none, so the plain set
difference is the correct rule. `dlc_port.py`'s "ASET fix" pass (found via `corpus_search`, not by
reading code) is the same repair applied selectively, which is the third-party confirmation.

Three things worth recording:

1. **The rule is scoped to the emitted WAD, and that is a deliberate narrowing.** "New" is not
   answerable without the game stack — retail may already name the hash, in which case the overlay's
   copy resolves to retail's block rather than wedging. Scoping to "no row in *this* WAD" keeps the
   rule in the hermetic-of-the-artifact stage and costs nothing measurable: donor blocks turn out to
   be **single-entry** (`oc_veh_helicopter_md500` and `pmc_hum_mattias` both carry exactly one), and
   the linked scripts block already mints a row per entry.
2. **Reading a block as an entry table needs a guard.** `parse_block_entry_table` reads the first
   word as a count unconditionally, so an opaque payload yields a garbage count and, from there,
   confident nonsense. Requiring the walk to *complete* separates "this block has no unreachable
   hashes" from "this block is not an entry-table block". The pre-existing `b"payload"` fixtures are
   the second kind, which is why they stayed silent under both new rules without being touched.
3. A block that will not inflate is left to M0182 rather than being complained about twice.

Tests: 134 → **140** in `mercs2_quartermaster`, all green, none ignored. Verified past `cargo test`:
a real 512×512 `replace_texture` against retail `vz.wad` builds clean, and `wad_simulator` reports
`issues=0` on all 34 asset types with `VERDICT: Full consumption path completed without violations`.

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

## D. The `qm` CLI — ✅ DONE (2026-07-28)

Four commands: `lint` (hermetic), `build`, `link` (across the installed set), `rules`.

**Three exit codes, not two.** `0` clean, `1` findings at Error or above, `2` the command could not
run. Splitting 1 from 2 was not in the original scope and turned out to matter: collapsed into one
nonzero code, a CI runner with no game install is indistinguishable from a failing mod. The template
repo will hit exactly that.

`lint` never looks for a game at all — not "looks and tolerates absence". That is the property
section I depends on, and `lint_needs_no_game_install` asserts stderr stays free of the discovery
message rather than only checking the exit code.

`rules` prints the unimplemented HANG-class traps under their own `KNOWN AND NOT YET CHECKED`
heading. Same reasoning as `PENDING` existing at all: a linter that silently omits its most dangerous
checks reads as a clean bill of health.

The name table is optional (it powers M0130 only) and says so when missing, rather than quietly
running one rule short.

Tests are subprocess-level, because "gated on exit code, never a printed count" is a claim about the
process rather than about `BuildReport` — including the clean-exits-zero case, without which the
others prove nothing. One builds a real WAD against retail vz.wad and checks the digest, placement
record and log.

Fixed in passing: diagnostics printed the contribution index twice
(`contributions[0]: contributions[0] (replace_texture) …`), invisible until a CLI started showing
them to modders. `SourceIssue::detail()` now returns the location-free form while `Display` stays
self-contained.

### Original scope, for reference

`mercs2_quartermaster` is library-only: no `src/bin/`, no `[[bin]]`. Every plan references
`qm build ./my-shipment` and `qm lint`, and the template repo's CI depends on `qm lint` existing.
Needs: `lint` (hermetic, no game), `build` (game-stack from `game_paths`/flags), gated on exit code
per the standing mandate.

## E. Consumption and publishing — ✅ DONE (2026-07-28)

The stated architecture is "neither Workshop nor Modkit owns the format — the crate does, and both
are clients." That was unimplement**able**, not merely unimplemented: with no `[workspace.dependencies]`
entry, neither client could write `mercs2_quartermaster = { workspace = true }` at all. Registered at
`1.0.0` (first release, matching siblings at 1.x/3.x), with a `README.md`.

**Distribution is ours, not the community's.** `qm` joins the curated set in
`.github/workflows/release.yml`, so a modder gets a prebuilt binary from the release page and needs
no Rust toolchain — the same model the ASIs already use (release assets carrying their digests) and
which Modkit already auto-updates against. The alternative considered and rejected was having the
template repo's CI build `qm` from a git dependency, which pushes our build problems onto every
Shipment author.

Four 64-bit rows only. `qm` is authoring tooling and never runs inside the game's 32-bit process, so
unlike an injected tool it has no reason to match the game's bitness; putting it on the i686 rows
would also mean cross-compiling `mercs2_luac`'s vendored C Lua for i686 with nothing asking for it.
The workflow records that so the omission stays a decision rather than becoming an oversight —
which is what `GUARDRAIL 2` in that file exists to prevent.

Consequence for I: the template repo's CI installs a **pinned released `qm`** and verifies its
sha256, rather than building anything.

## F. Rule doc links — ✅ DONE (2026-07-28)

Diagnostics print a URL now (`Rule::url()` over `DOC_BASE`), and `qm rules` with them.

**Seven of the anchors were broken and nobody had noticed.** The field guide's headings read
`## Trap 7 — Your reskin makes the game hang…`, so GitHub generates
`trap-7--your-reskin-makes-the-game-hang-on-the-loading-screen-not-crash--hang` — `#trap-7` landed
every reader at the top of a 17-trap document. Fixed, and `tests/doc_links.rs` now derives anchors
the way GitHub does and checks each against the real headings, which is what makes anchors this ugly
maintainable at all: a reworded heading fails a test instead of silently breaking a link.

`docs/modding/manifest_format.md` is written — Plan 04's freeze deliverable, and the target of ten
rules that pointed at a file which did not exist. It lives in the **notes repo** (committed locally,
`4e6652b`), alongside the other modding docs.

⚠ **The URLs 404 until that commit is pushed.** The link test passes off the local checkout, so
nothing here catches it — the only signal is pushing.

### Original scope, for reference

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

## I. Template repo `mercs2-shipment-template` — ✅ DONE (2026-07-28)

<https://github.com/Mercenaries-Fan-Build/mercs2-shipment-template> — pushed, CI green on the first
run.

**The example had to be one that builds and loads as-is**, or the template teaches a modder to
ignore output on day one. `pmc_hum_mattias_v3_ub` was chosen because it is FULLY RESIDENT: its whole
mip chain lives in one block, so replacing it with one resident block changes nothing structural.
The obvious alternative — `al_hum_boss_ub`, which our own tests use — emits M0007 *and* M0009,
because it is a 4-rung streamed texture carried as a shared sub-entry. Verified by dogfooding
`qm lint --with-game` across candidates rather than by reasoning about it, then built and put
through `wad_simulator`: no violations, `issues=0` on every type.

**CI installs a pinned prebuilt `qm` and verifies its sha256 against GitHub's own record of the
release asset.** GitHub exposes a `digest` field per asset, which is the same value a hardcoded
digest would be copied from — so pinning one in the workflow would add a second thing to bump
alongside `QM_VERSION` while adding no assurance. Bumping the version is now a one-line edit.

The hermetic/game-stack split paid for itself here: the runner has no retail WADs and never will.

### Found by running the released binary rather than the debug build

`qm`'s name-table fallback used a `CARGO_MANIFEST_DIR`-relative path — the filesystem of the machine
that BUILT it. The released binary therefore looked for its data on a CI runner and never found it,
so M0130 silently never ran. Same class as the hardcoded asset paths that worked on exactly one dev
machine. Now walks up from the executable, then the working directory, with a regression test.

**Still open:** a bare downloaded `qm` has no name table anywhere, so template CI prints a one-line
note saying M0130 will not run. Correct behaviour — silently running one rule short is the failure
this crate exists to avoid — but it is noise on every run. The fix is to make `qm` carry the table:
`data/production_names.json` is 1.0 MB / 23,110 names against a 6.7 MB debug binary. Worth doing,
not blocking.

---

## Suggested sequencing

1. ✅ **`mercs2_luac` parity regression** — cheap, and everything in A rests on it.
2. ✅ **BINN metadata survey** — decides whether the linker approach holds at all.
3. ✅ **C (library extraction)** — unblocks A, B and future work in one pass.
4. ✅ **A (the linker)** → `add_outfit` lowers → Plan 01 phase 5. Includes the cross-Shipment
   relink, so two script-touching Shipments no longer annihilate each other.
5. **B** in the order given — M0001 and M0002 done; M0003 → M0004 next.
6. **D → I**, then E, F, G, H as they become relevant.

~~**Revised next step: D, ahead of the rest of B.**~~ Done — `qm` exists, so I is unblocked.

**A–F and I are done, and B is down to its research tail.** What is left:

- **B**: M0006 (reassess — M0009 may already suffice), then M0005 and M0008, both of which are
  research rather than implementation. M0001–M0004 are all implemented.
- **G**, the unimplemented lowering: `edit_state_machine`, `native_hook`, `raw`.
- **H**, format gaps (`add_model` `group:`, donor auto-pick, Open-Q7, Open-Q10).
- Making `qm` carry its own name table (see I).

**Next: M0006, and it should start as a measurement rather than an implementation.** The predicate is
cheap — `simulate.rs:276` `xref_sources` keeps only the first referrer, and making it a `Vec` yields
the fan-in map — but the question worth answering first is how much of the rule M0009 already covers
in practice. The M0003/M0004 step is the argument for doing it that way round: the retail sweep took
minutes and decided the shape of both rules before a line was written.

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
