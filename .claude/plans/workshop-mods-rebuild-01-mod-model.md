# Workshop "Mods" rebuild — Plan 01: the mod model + shared crate

**Status:** DESIGN (rev 2, 2026-07-25). Multi-session effort. No code written yet.
**Siblings:** `-02-navigation.md`, `-03-live-bridge.md`, `-04-manifest-format.md` (the format contract)
**Origin:** user wants to rebuild the Workshop's "Mods" page from the ground up. The page today is a
black-sheep single-recipe bench (inject a static mesh into a vehicle container's group N → emit a WAD).
See `app.rs` `Workbench::Mods` arm (~lines 1410, 2797–3056) and memory `mercs2-workshop-devtool`.

## The reframe that drives everything

A mod is **not** "an edit to an asset." A mod is **Modifications** — anything from a community member
patching the base game up to a full engine rewrite. The current tooling only encodes asset
edit/replace vectors because those are what we happened to build; the model must be open-ended.

### The layer model

The game is a stack; a mod contributes to one or more layers. Its "blast radius" = which layers +
which hashes/functions/symbols it touches.

```
Runtime   live process state ......... poke / spawn / hot-reload   (see Plan 03, the live bridge)
Code      exe / reimpl engine ........ native hooks, new systems, whole rewrites
Script    Lua behavior ............... patch, append, new modules
Data      WADs: mesh/tex/audio/tables  replace-by-hash, add-new-hash
```

- Texture reskin = 1 layer (Data).
- "Lower the landing-craft gate" = Data+Script, or Data+Code.
- Total conversion / engine rewrite = mostly Code layer.
- Same noun, different blast radius.

### A mod = a package of typed CONTRIBUTIONS with an OPEN lower bound

The `manifest.yaml` core list is `contributions`, NOT `edits`. Each contribution:
`{ target_layer, extension_point, payload }`.

**Critical design rule — the payload has an open lower bound.** Anything the tool doesn't have a
first-class recipe for is still expressible as a *raw* contribution: raw block bytes, raw Lua source,
raw native-hook descriptor, raw file drop — each with a declared blast radius. This is the SAME
principle `bundle.rs` already commits to ("nothing is discarded; keep the raw `raw/*.ucfx` bytes;
whatever we failed to understand can be put straight back"). We extend that principle from *export* to
the *entire mod format*. This is what lets the format carry engine rewrites BEFORE any UI exists for
them. Recipe UIs are just typed sugar over contributions the builder+linter already handle generically.

### Why generality is CHEAP here, not a boil-the-ocean

Every mod HAS a blast radius — **COMPUTED** by the Quartermaster from typed contributions (a `replace_texture`
on hash X ⇒ touches Data:X), and **DECLARED** only for `raw` contributions the tool can't infer. That
one object is the exact thing three features already need:
- **conflicts** (ClaimGroup, already built in modkit) = blast-radius overlap
- **the linter** (below) = rules keyed on what a mod touches
- **"reskin vs rewrite"** = the shape of the blast radius

So it pays for itself three times over; it isn't overhead. And because it's computed (not hand-authored)
for the common case, the author never maintains it — they just add contributions.

## Naming (re-settled 2026-07-25)

A mod is a **Shipment** — a documented package of changes; the user's framing is "an inventory of
modifications." It generalizes to ANY mod type — texture swap, mission, or engine rewrite are all
Shipments.

**Rejected, with reasons** (the bar: no collision with an in-game concept, a Rust keyword, or a
sibling tool):
- **Contract** — collides with Wally's `Ess.Contract` = an in-game mission, AND our own Missions
  domain authors real in-game contracts.
- **Dossier** — used through 2026-07-24, then dropped: the PDA carries a *Dossier* database of
  character bios (`WifBios.AddDossierEntry("BioChris")`, `PdaInterface.Database:AddDossierEntry`,
  `oPda.CustomData.tDataDossiers`). Same objection as Contract, and semantically inverted — the game's
  dossier catalogs *people*, ours catalogs *changes*.
- **crate** (Rust keyword + 8 Lua files) · **cache** (9 Lua files, overloaded) · **kit** (collides with
  Modkit) · **bundle** (our own `bundle.rs`) · **depot** (7 Lua files, 45 asset names) · **pallet**
  (7 asset names) · **dispatch** (40 files in our workspace) · Drop / Loadout.

Candidates are collision-checked against three surfaces before adoption: the decompiled Lua corpus,
the retail asset-name table, and our own Rust workspace. `shipment` / `manifest` / `quartermaster` are
clean on all three.

The **Quartermaster** is the counterpart concept: the role/engine that *works* a Shipment (reads, lints,
builds, ships it) — the role that inventories and issues materiel. Same concept at two layers — a Rust
crate AND its Workshop UI panel (Plan 02). A Shipment CONTAINS a `manifest.yaml`, which is both the
real-world relationship and the one Cargo already teaches every Rust developer:

- **Manifest file:** `manifest.yaml` — **YAML preferred**; `.json` (JS-tooling native) and `.toml` also
  accepted on read; one `serde` model parses all three (Plan 04). Quartermaster writes YAML.
- **Community template repo (its OWN standalone repo):** `mercs2-shipment-template`
- **Engine crate (format + linter + builder):** `mercs2_quartermaster` · CLI: `qm build ./my-shipment`
- **Workshop panel** (UI front-end to the crate): the **Quartermaster** (Plan 02)

## Two artifacts — keep them separate

1. **The template** (`mercs2-shipment-template`) — a STANDALONE git repo the community clicks "Use this
   template" on to start every mod. MUST be standalone: GitHub templates can't be a monorepo subdir, and
   a modder shouldn't clone our whole tree for a texture swap. This is the noun the community says aloud.
   Ships: the folder skeleton, a filled-in example `manifest.yaml`, a README, and CI that runs the
   linter on every push (so a fork is validated the moment it's pushed). ⚠ CI is **lint-only** — a
   public runner has no retail WADs, and a build needs them (Plan 04, host-provided game stack).
2. **The engine** (`mercs2_quartermaster`) — the Rust crate that reads/lints/builds a Shipment. It is a **new
   member of the `tools/wad_simulator` workspace** (`crates/mercs2_quartermaster`) that is ALSO crates.io-
   published, exactly like its siblings — the workspace already does both (path dep in-tree via
   `workspace = true`, published version consumed out-of-tree). So it does NOT need its own repo; only
   the template does. All three consumers depend on it cleanly: the template repo's CI + the in-tree
   Workshop + the out-of-tree Modkit (which consumes the published version).
   - ⚠ **`mercs2_formats` is at 3.0.0** (workspace note: a `^2.1` consumer like modkit will NOT pick 3.0.0
     up until edited). `mercs2_quartermaster` builds on 3.0.0; when Modkit is to consume `mercs2_quartermaster`, it
     must first migrate off `^2.1`. Not a phase-1 blocker for the crate, but a tracked follow-up.
   - ⚠ **`tools/wad_simulator` is its OWN nested git repo** (memory `nested-repo-wad-simulator`): the new
     crate + its commits land in the NESTED repo, not the outer one. `git -C tools/wad_simulator`.

## The engine crate: `mercs2_quartermaster`

Neither Workshop nor Modkit owns the Shipment format — `mercs2_quartermaster` does, and both apps are clients.

Responsibilities:
- `manifest.yaml` identity/version/author/load-order/deps + the `contributions` list (typed | raw).
- The contribution enum (known recipe kinds) + the raw escape hatch.
- **Lowering**: contribution → concrete artifact (patch block / scripts_vz / native descriptor).
- **The linter** (see next section) — the crown jewel.
- **The builder**: contributions → output WAD(s) + sha256 + build log. Headless (`qm build
  ./my-shipment`), gated on EXIT CODE, never a printed count (standing mandate). Builds must be
  DETERMINISTIC — byte-identical across runs, or verify-by-hash means nothing.
- Blast-radius computation + conflict resolution — modkit's ClaimGroup **generalized** from
  all-or-nothing overlap to `(write-set, read-set, merge-class)`; see the composition catalog below.
- **Path-in, never path-discovering.** The crate takes the game WAD stack as an argument (like
  `publish_in_background(wad_paths, …)` already does). Resolution is the HOST's job — Workshop
  Settings page, `qm --game <dir>`, or nothing at all in CI. `qm lint` must run with no game present;
  only `qm build` requires it. (Plan 04, and the Settings page is Plan 02.)

On-disk shape of a Shipment (git-diffable; what `mercs2-shipment-template` scaffolds). Full schema draft:
`workshop-mods-rebuild-04-manifest-format.md`.
```
my-shipment/
  manifest.yaml        # identity, version, load-order, deps, and contributions INLINE (v1)
  src/                # source .gltf, .png, .lua, raw payloads the contributions reference
  build/              # output wad + sha256 + build log (gitignored)
```
v1 declares contributions inline in `manifest.yaml`; an optional `contribs/*.yaml` include mechanism is a
future affordance for very large shipments, not a required directory.

### Known contribution kinds (the typed sugar; extend over time)

| kind | layer | lowers to |
|---|---|---|
| `replace_texture` | Data | fully-resident container under the same hash (NOT a body-splice) |
| `add_model` | Data | new-hash single-entry block + ASET row (ADDITIVE, own hash) |
| `patch_lua` | Script | a DECLARED MUTATION (target + payload + merge class), linked at deploy → `mercs2_luac` |
| `add_outfit` | **Data+Script** | additive model inject + a `patch_lua` on `_tOutfits` (proven wardrobe path) |
| `edit_state_machine` | Data | SWIT/STAT/CHDR/CEXE rewrite (`FUN_004cf340` format, decoded) |
| `native_hook` | Code | retail: a prebuilt **ASI** placed in `pmc_bb.dll`'s search path · reimpl: Rust/wasm/Lua plugin |
| `raw` | any | opaque payload + declared blast radius (the open lower bound) |

`retarget_rig` is NOT a standalone kind in v1 — it is an inline `retarget:` sub-block on `add_outfit`
/ `add_model` (Plan 04 Q6). `donor` is optional on every kind that accepts one.

Two engines make the Code layer real:
- **reimpl**: Code contribution = real Rust plugin / wasm / Lua module — hot-reloadable, first-class.
- **retail**: Code contribution = an **ASI plugin** loaded by our own `pmc_bb.dll` (v3.0.0 — SecuROM
  spoof + `pmc_blackbox.log` writer + ASI loader + MinHook), which the cracked exe imports directly.
  Plugins get `pmc_log`/`pmc_log_flush`. NOT ThirteenAG's Ultimate ASI Loader — that was evaluated,
  not shipped. Modkit installs and manages the loader; a Shipment never ships it. Detail: Plan 04
  "The Code layer". Hard path, but expressible (full symbol map;
  SecuROM decompiled per memory `securom-decompiled-not-a-blocker`).

## The linter — the strongest differentiator

Our memory index IS a rule set. Every entry is a trap a modder cannot discover on their own. Port each
into `mercs2_quartermaster` as a NUMBERED, documented, often auto-fixable diagnostic, gated on build. This is
where our reverse-engineering knowledge becomes something that scales past the two of us.

Seed rules (each → a `Mxxxx` code + doc link + optional `[Apply fix]`):
- Dangling `_P001/2/3` LOD rungs → 549 GB buffer request → open-world stream HANG
  (`patch-wad-dangling-lod-rungs`, gate with `aset_refcheck`).
- `packed_field` under-claims decompressed size → heap overrun (`modkit-merge-wardrobe-textures`).
- Short texture BODY vs `linear_mip_chain_size` → `BUFFER_TOO_SMALL` → world-load livelock.
- Two mods each shipping `scripts_vz` → silent mutual annihilation (per-hash last-wins whole block).
- Unnamed model used as outfit → unusable (`Player.SetOutfit` hashes a NAME string).
- Non-resident costume on the on-demand path → `STATE_WAITFORGAME` wedge
  (`dlc-skin-swap-via-pmc-wardrobe`).
- Overwriting a shipped asset instead of adding (`no-destructive-replacements` mandate).
- Name-hash collision → registry insert is FIRST-wins → your asset silently drops
  (`no-arbitrary-hashes` mandate).
- Never merge into `vz.wad`; ship via `vz-patch.wad` overlay (`never-merge-into-vz-wad` mandate).

**Corollary — make mandates UNREPRESENTABLE, not warned.** The UI should have no path that overwrites
a shipped asset, no free-text hash field (hash = `pandemic_hash_m2(name)`; the name is the identity),
and no way to write into `vz.wad`.

## The composition catalog — the linter's sibling (new 2026-07-25)

Same bet as the linter, aimed at a different question: not "is this Shipment valid?" but "do these two
Shipments compose?" Plan 04 defines the four merge classes (`Exclusive` / `KeyedSet` /
`OrderedList` / `LastWins`) and the write-set/read-set model. **The per-target RULES are crate content
here** — hand-curated, growing, and the reason a modder cannot answer this alone.

Each entry: target · mechanism · merge class · key · ordering constraint · derived companions ·
read-set · failure mode when violated. Seeded from the five mechanisms found in the base game
(evidence and line numbers in Plan 04):

| target | mechanism | class | note |
|---|---|---|---|
| PDA dossier entries | native additive API, ungated | `KeyedSet(sTitle)` | upsert; trivially merge-able |
| `tSupportData` | native additive API, DLC-gated | `KeyedSet(sKey)` | `AddSupportData` no-ops unless `g_bIsDlc` — but that global has exactly ONE reader in the whole corpus, so the QM just sets it (Plan 04 Q8). Retires "store items need a block-3185 rewrite" |
| `_tOutfits[hero]` | source-append of `table.insert` (it's a global — no AST edit) | `OrderedList`, keyed `(wearer, slug)` | append-only; index 2 reserved; save persists a POSITION; `_nAvailableCostumes` must be DERIVED from final length. Key is per-hero — retail reuses `Original`/`ChickenSuit` across all three |
| HQ starters | exclusive by construction | `Exclusive` | second claimant refused to `Debug.Printf` only — silent to the player |
| reward → support refs | cross-reference | (read-set) | missing key silently skipped; the lookup is also memoized, so late registration is invisible |
| texture by hash | WAD stack | `LastWins` | load order IS the user's answer |
| ASET rows | engine-enforced | `KeyedSet(hash)` | `validate_blocks` already rejects a duplicate primary and allows a repeated sub-entry (`patch_wad.rs:631`) |
| string DBs | `AddStringDb` | `LastWins` + **hard cap 8** | resolver walks DBs in REVERSE then falls back to base language, NULL on miss (`FUN_0046423e`); registration refuses past 8 slots (`FUN_00464540`) — a capped resource, so the 9th claimant silently gets nothing |
| ASI plugins | `pmc_bb.dll` loader (glob `*.asi` over game root + `scripts\`/`plugins\`/`update\`) | `Exclusive` keyed on **hooked address** | plugins coexist, but two hooking one address do not — and discovery is FILESYSTEM ORDER, so there is no load order that resolves it. Must be a hard error. Loader is Modkit-managed, never a contribution |

**Fail closed.** An unrecognized target ⇒ `Exclusive`. The open lower bound survives — an unknown
script edit stays expressible, it just cannot silently co-install.

Note the linter rule above ("two mods each shipping `scripts_vz` → silent mutual annihilation") is what
this catalog exists to FIX rather than merely diagnose: `patch_lua` ships a declared mutation, and the
Quartermaster links the block once across the installed set. Where a recipe implies a fixed edit (the
`add_outfit` availability-count lift), the Quartermaster owns it and emits it ONCE at link time.

## Testing — ship tests with every phase (user-set 2026-07-25)

Align with workspace norms rather than inventing: `mercs2_formats` already carries **276 tests** plus
`tests/fixtures/`, and game-dependent tests use the established gate — `#[ignore = "needs the retail
vz.wad"]` over an `Option`-returning opener that skips gracefully when the install is absent
(`mercs2_engine/tests/registry_wad_probe.rs`).

**The hermetic/gated split mirrors lint-vs-build exactly**, which is what makes template CI viable.

*Hermetic — no game required, runs in CI:*
- **Cross-format conformance.** The same logical document as YAML, JSON, and TOML must deserialize to
  an identical model. This is the direct test of the "one serde model" claim, and it surfaces the
  `toml` internally-tagged-enum gotcha on day one instead of mid-implementation.
- **Round-trip.** parse → emit → parse is stable; a manifest the Quartermaster WROTE re-reads identically.
- **Manifest errors are errors.** Two `manifest.*` in one root = ambiguity error; a NEWER `format:` =
  loud reject; an older one is accepted.
- **Name → hash vectors**, including `al_veh_boat_destroyer` = `0xE54047D5` vs
  `ch_veh_boat_destroyer` = `0x25FE00A7` — a regression test for the exact name/hash drift that was
  live in the Plan 04 draft, and the reason `touches` takes names.
- **One failing + one passing fixture per linter rule.** A rule without a test is a rule that will
  silently stop firing — which is the same failure class the linter exists to prevent.
- **Composition** (all pure logic, no game): two synthetic Shipments each appending an outfit merge
  with a DERIVED count and deterministic indices; two claiming one HQ starter hard-error; a read-set
  entry with no writer is flagged; an unrecognized target resolves to `Exclusive` (fail-closed).
- **Determinism.** Build twice, assert byte-identical output — otherwise verify-by-hash means nothing.

*Gated `#[ignore]` — needs the retail install:* donor resolution and auto-pick, `replace_texture`
dimension/resident-size checks, and a full end-to-end build of the template example verified by sha256.

The template repo's CI is itself a test: `qm lint` over the shipped example Shipment must pass, so the
example doubles as a conformance fixture the community can diff against.

## The fork (settled)

Split by ROLE; share the crates.
- **Modkit** (Tauri+Vue, `C:\Users\Shadow\Desktop\mercs2-modkit`) = **Tier 1** users. Install, load
  order, conflicts, deploy/undo, saves, updates, wardrobe, texture swap. Web UI is right for management.
- **Workshop** (native egui, `tools/wad_simulator/crates/mercs2_workshop`) = **Tier 2/3** users.
  Engine-accurate authoring, truthful preview — its entire reason to exist is the real renderer.
- Both consume `mercs2_quartermaster` (format + linter + builder) and, later, `mercs2_bridge` (Plan 03).

Tier 3 in the Workshop means **format-level** (ASET rows, host groups, chunk inventory, state hashes,
behind per-panel "Advanced" reveals) — NOT assembly-level. The x32dbg/Ghidra surface stays separate.
Modkit currently uses a *path* dep on these crates; publishing `mercs2_formats`/`mercs2_luac` (and
`mercs2_quartermaster`) to crates.io is an open modkit task (memory `modkit-merge-wardrobe-textures` §Open).

### Three-tier API (adopt from Wally's Ess)
Ess tiers its API `Raw` → Core → `Easy`, which maps 1:1 onto our Tier 3/2/1. Adopt the SAME idiom for the
Shipment recipe surface: `Raw` contributions (composable primitives, Workshop power user) → Core
(named-param recipes) → `Easy` (guard-railed presets, Modkit/beginner). One tiering idiom ecosystem-wide.

## Phasing — FORMAT-FIRST (ordering settled 2026-07-24)

Design the CONTRACT before the code. The `manifest.yaml` schema is what everything depends on.

0. **Format** *(current step)* — design + freeze the `manifest.yaml` schema and the template folder shape
   as a WRITTEN SPEC (`workshop-mods-rebuild-04-manifest-format.md`). No crate yet. This is the contract
   the crate implements and the template scaffolds. Validate the shape against 2-3 real mods on paper
   (an outfit, a texture reskin, a raw block) before freezing.
1. **Crate `mercs2_quartermaster`** *(STARTED 2026-07-25 — increment 1 landed)* — implement the frozen spec: parse the manifest (yaml/json/toml via one
   serde model), round-trip on disk,
   compute blast radius, headless build (`qm build`) gated on EXIT CODE. NO UI. Wrap the existing
   lowering building blocks (formats 3.0.0 publish/patch/texture/model_inject + workshop `publish.rs`
   + **`wad_builder build-skin`**, `wad_builder/src/main.rs:353` — an end-to-end wardrobe path that
   already exists), don't reimplement them. **Ships with:** the cross-format conformance suite over
   the Plan 04 fixtures, round-trip, manifest-error, and hash-vector tests.
2. **Port the linter rules** from memory, each with an ID + doc link. Start with the HANG-class rules
   (dangling LOD, packed_field, short texture body) — those are silent and catastrophic. **Ships with:**
   a failing AND a passing fixture per rule — no rule lands untested.
3. **Adopt + generalize ClaimGroup** — lift from modkit `models/claim.rs`, then extend it to
   `(write-set, read-set, merge-class)` and seed the composition catalog above. This is what makes the
   flagship recipe co-installable; without it, two outfit mods silently annihilate each other.
   **Ships with:** the synthetic two-Shipment merge tests (derived count, deterministic indices,
   exclusive collision, dangling read, fail-closed default) — all hermetic.
4. **Template repo `mercs2-shipment-template`** — the standalone repo: folder skeleton + a filled-in
   example `manifest.yaml` + README + CI. ⚠ CI runs **`qm lint` only** — a public runner has no retail
   WADs, so a headless *build* is not available there (Plan 04, host-provided game stack).
5. **First recipe end-to-end** — "Add an outfit" (fully proven path, and what the community wants most).
6. Everything else (navigation, live bridge) is Plans 02 and 03.

⚠ **Phase 0 is not something I can mark done.** Freezing is the user's call, and any freeze is
PROVISIONAL until a first Shipment actually BUILDS — writing plausible YAML is not proof.

### Phase 1 progress

**Increment 1 (2026-07-25) — the manifest model + cross-format read path. DONE, 13 tests green.**
`crates/mercs2_quartermaster`: `manifest.rs` (the one serde model), `lib.rs` (format dispatch,
path-in only — no game discovery), `tests/conformance.rs`.
Deliberately led with the schema-risk part rather than the builder, and it paid: **the TOML
internally-tagged-enum risk is closed** and the YAML crate choice is settled (`serde_norway`;
`serde_yml` turned out to be deprecated). See Plan 04 Serialization.

**Increment 2 (2026-07-25) — discovery + source resolution. DONE, 31 tests green.**
`discover.rs`: `find_manifest` (extension detection; **multiple `manifest.*` = ambiguity error**
naming the offenders), `open` (find → read → parse → validate), `source_refs` / `check_sources`.

Source checking reports four distinct facts rather than one blob, so the linter can grade them
separately: `Absolute` (not portable), `EscapesRoot`, `Missing`, `OutsideSrc` (convention, not
safety). Containment is checked **lexically** — so it works for files that do not exist yet — and
again by canonicalization when the file does exist, which is what catches a symlink pointing out of
the Shipment. It never short-circuits: an author fixing a manifest wants the whole list.

⚠ Residual: the lexical check is separator-naive on a Windows-style `src\foo.png` written into a
manifest read on Unix. Worth a rule when the linter lands.

**Increment 3 (2026-07-25) — blast radius + composition. DONE, 48 tests green.**
`blast.rs`: `Claim` / `Access` / `MergeClass` / `Intent`, `claims()` (computed for typed kinds,
declared for `raw`), `self_conflicts`, `conflicts`, `unsatisfied_reads`.

**The composition model survived contact with code, but three parts of it were wrong on paper and
only the tests found them:**

1. **The NAME is the identity; the hash is only the normalized comparison key.** Both spellings an
   author may write — `al_veh_boat_destroyer` and `0xE54047D5` — must land on one claim, or
   declaring the hash dodges conflict detection entirely. `pandemic_hash_m2` is one-way, so the hash
   is the only total function over both spellings and therefore what we compare on; but it must
   never be what a human reads or writes. Carrying the name *inside* the claim broke the first half
   (two spellings, two claims), so the name moved to `ClaimRecord.name`.
2. **Merge class depends on INTENT, not just target.** `raw` was inheriting ordinary
   replacement semantics (`LastWins`) because its declared target looked like any other asset. Opaque
   bytes must fail closed *before* the target-shaped rules run, or a `raw` block launders a target
   into permissive semantics.
3. **Intra-Shipment duplicates are not uniformly errors.** The draft said "any duplicate in one
   Shipment is a hard error." That rejects a perfectly ordinary multi-outfit pack, whose outfits all
   share the wardrobe script claim. Only `OrderedList` accumulates; the rule is now class-aware.

The headline test — `two_outfit_mods_for_the_same_hero_coexist` — is the whole point of the model,
and it passes. Also pinned: same `(wearer, slug)` collides, the same slug on *different* heroes does
not, minting one name twice is a hard error (registry is first-wins), two texture replacements are
load order rather than conflict, and a shared `donor` never conflicts because it is a READ.

⚠ `MERGEABLE_SCRIPTS` is currently a one-entry allow-list (`wifpmcinterior`). Deliberate: being
wrong there costs a false conflict (visible) instead of silent mutual annihilation (fatal). It grows
as we reverse more targets — this is the curated half of the composition catalog above.

**Increment 3b (2026-07-25) — hash → name resolution. 60 tests green.**
`names.rs`: `NameTable` over the committed 23,110-entry `data/production_names.json`, plus
`enrich()` and `bare_hash_suggestions()`.

Closes the other half of `no-arbitrary-hashes`. Unifying both spellings into one claim was only the
comparison half; a hand-written hash was still *displayed* as a hash and the author was never told
to write the name. Now: every claim we can reverse is named in diagnostics, and writing a hash we
have a name for is a flagged, **auto-fixable** finding (the message carries the replacement text).
A hash with no known name stays legal and silent — otherwise the documented escape is unusable.

Table is HOST-PROVIDED (path in), consistent with the game stack; `find_from` is a convenience that
mirrors what `mercs2_workshop` already does, and a missing table degrades diagnostics rather than
failing. `native_hook` touches are deliberately EXCLUDED from suggestion — they are code addresses,
and reversing one through the asset table would produce a confident, wrong rename.

**Increment 4 (2026-07-25) — the linter. 81 tests green.**
`lint.rs`: numbered `Mxxxx` rules with a title, doc link and — where the fix is mechanical — the
exact replacement text. `blocks_build()` is the gate (Hang/Error block, Warning/Info do not), gated
on EXIT CODE per the standing mandate.

13 hermetic rules implemented, covering the manifest surface: schema validity, the four source-path
outcomes, self-conflicts, bare hashes (auto-fixable), unknown wardrobe hero, unmergeable script
target, `raw` with an empty radius, ASI-on-reimpl, a hook that installs nothing, and malformed /
insecure external pins. Each has a test that FIRES and one that stays QUIET — a rule with only the
former eventually fires on everything and gets ignored, which is how linters die.

★ **The six HANG-class rules that need the WAD stack are REGISTERED in `lint::PENDING`, not
silently absent** — dangling `_P001` rungs, `packed_field` under-claim, short texture body, missing
ASET row, non-resident costume, shared-texture collateral. A linter that quietly omits its most
important rules reads as a clean bill of health, which is worse than no linter. They land with the
builder, where the WAD stack is in hand. A test pins that they stay registered.

Two judgment calls: an unknown wearer gets a suggested spelling only on a near-miss (`mattius` →
`mattias`) because a confident wrong suggestion is worse than none; and without a name table the
bare-hash rule stays silent rather than emitting a finding the author cannot act on.

**Increment 5 (2026-07-25) — the builder. 89 tests green.**
`game.rs` (`GameStack`, path-in, last-mounted-wins) and `build.rs` (lint-gate → lower → assemble →
emit). Adds `sha2` and `png`.

★ **A REAL SHIPMENT NOW BUILDS.** A `replace_texture` Shipment built against the retail 2.5 GB
`vz.wad`, emitted a 2,162,688-byte overlay, and **`wad_simulator` reports no violations** with every
asset type consuming `issues=0`. The encoded body is 174,760 bytes for 512² BC1 / 8 mips, which
matches the retail spec in `tex_build`'s header exactly. Two consecutive builds are byte-identical,
so the determinism requirement holds. This is the thing that was "not answerable on paper" for the
whole design — answered, for one recipe.

★★ **`wad_simulator` caught two structural bugs that NO digest check could.** The WAD hashed fine,
round-tripped fine, and was nonsense. Worth internalising: verify-by-hash proves an artifact is
unmodified, not that it is *correct*.

1. **A patch block is `[entry table][containers…]`, not a bare container.** I handed
   `ucfx_texture`'s output straight to `PatchBlock`, so the loader read the `UCFX` magic as an
   entry-table field. Fix: `texture::build_texture_block`, which wraps it as a single entry.
2. **The ASET row's low 16 bits must be `0xFFFF`.** I wrote `0x0000`, which is not "primary" — it
   names a `_P001` LOD block one level finer. **That is our own M0001, the dangling-LOD-rung HANG**,
   generated by the very tool meant to prevent it. Both are now pinned by assertions in the gated
   test.

⚠ **Blocker for the remaining kinds — an architectural one, not missing work.** Plan 01 says "wrap
the existing lowering building blocks … + workshop `publish.rs`". That is **not currently possible**:
`mercs2_workshop` and `wad_builder` are **binary-only crates with no `src/lib.rs`**, so
`publish.rs`'s donor resolution + model inject and `build-skin` are unreachable as libraries.
`mercs2_engine` is a lib but pulls winit + wgpu, which a headless crate must not take. `build.rs`
returns `Unsupported` with that reason rather than reimplementing — forking the one path proven to
work in-game would be worse than waiting.

**Next: extract the lowering into a library.** Either give `mercs2_workshop` a `lib.rs` exposing
`publish`/`retarget`, or move the donor-resolution + model-inject core into `mercs2_formats`
alongside the texture path that already works. Until then `add_model`/`add_outfit` cannot lower,
which also blocks Plan 01 phase 5 ("first recipe end-to-end" = an outfit).

Also still open: `patch_lua` lowers at LINK time across the installed set, and the linker is not
written; `lint::PENDING`'s six WAD-gated rules are now implementable and should land next to it.

## Open questions for the user
- The domain/nav question is Plan 02; the fork is settled here.
- Whether the first shipped recipe is outfits or texture reskins (leaning outfits).
- crates.io publish of `mercs2_formats`/`mercs2_luac` so both repos stop needing side-by-side paths.

## Grounding pointers (for a fresh session)
- `corpus_search` is stale-check first (`corpus_status`); it indexes docs+memory+tools+commits+ghidra.
- memory: `mercs2-workshop-devtool`, `modkit-merge-wardrobe-textures`, `asset-injection-playbook`,
  `mercs2-fixpack-project`, the mandate memories listed above.
- `docs/modernization/workshop_charter.md` — the existing (asset-centric) charter to supersede.
- `docs/modding/field_guide.md` — 17 traps, each already a linter rule.
- `bundle.rs` — the "keep raw, nothing discarded" precedent the mod format generalizes.
