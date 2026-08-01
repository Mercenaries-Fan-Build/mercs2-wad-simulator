# Workshop "Mods" rebuild — Plan 04: the Shipment manifest format

**Status:** ★ LANDED (2026-08-01). Implemented in `mercs2_quartermaster`; the frozen contract lives at
`docs/modding/manifest_format.md`. Conformance fixtures (`yaml_json_and_toml_agree`,
`toml_carries_the_kind_tag_for_every_v1_kind`, `the_fixtures_exercise_every_kind_the_format_knows`)
hold it to the crate.
**Siblings:** `workshop-mods-rebuild-01-mod-model.md` (the model), `-02-navigation.md`, `-03-live-bridge.md`

> ## What landed (absorbs Plan 05 §G/§H)
>
> Every v1 kind lowers or refuses honestly, with a firing + staying-quiet lint fixture each:
> `add_outfit`, `add_model` (now with **`textures:`** and **`group:`**, and **donor auto-pick**),
> `add_texture`, `add_sound`, `add_movie`, **`add_ui`** (movie + Quartermaster-owned `qm_modloader`
> loader), `replace_texture`, `edit_stringdb`, **`edit_state_machine`** (full-family regenerator —
> add/remove states, `qm extract-states` baseline, M0193 vocabulary guard), `patch_lua`, `native_hook`,
> `place_file`, `raw`.
>
> - **Composition** (§Composition here) is the model the crate implements — five mechanisms, four merge
>   classes — and it **supersedes** Plan 01's ClaimGroup line. Load order (`after`/`before`) and
>   cross-Shipment conflicts are resolved (Plan 01 F1/F2).
> - **Novel assets**: the survey (Plan 05 §H) proved most "needs a builder" types are opaque `data`
>   wrappers; audio, stringdb and movies all land. Video is not an ASET type (loose `data/Movies`
>   files or a `cfx_pack`).
> - **Next format work, scoped not built**: the `vz_state` world overlay (permanent, world-scale
>   destruction) — `docs/modding/vz_state_world_overlay_scope.md`. And the `edit_state_machine`
>   **`states:` schema** is now real (`crate::states`, extract-then-edit).

## Naming (re-settled 2026-07-25 — supersedes the Dossier/Handler pair)

A mod package is a **Shipment**; the engine that reads/lints/builds/ships it is the **Quartermaster**.

**Why the rename.** "Dossier" collides with a real base-game concept: the PDA carries a *Dossier*
database of character bios — `WifBios.AddDossierEntry("BioChris")` (wifmissionflow.lua:51),
`PdaInterface.Database:AddDossierEntry` (mrxguiinterface.lua:1325), backed by
`oPda.CustomData.tDataDossiers` (mrxguipda.lua:1260). This is the SAME objection that killed
"Contract" (Wally's `Ess.Contract` = an in-game mission). Worse, the game's dossier is a catalog of
*people* while ours is a catalog of *changes* — the word does opposite work in the same UI, and our
own `patch_lua` contributions will sit in files next to `AddDossierEntry` calls.

**Why Shipment/Quartermaster.** A Quartermaster inventories and issues materiel, which is what a
Shipment is: an inventory of modifications. It also fixes a latent wart: `dossier.yaml` made the
package and its metadata file the same word. A Shipment CONTAINS a `manifest.yaml`, which is the
real-world relationship and the one Cargo already teaches every Rust developer.

Collision-checked against three surfaces before adoption — the decompiled Lua corpus
(`docs/mercs2-luacd/src/`), the retail asset-name table (`docs/data/aset_names.csv`), and our own Rust
workspace. `shipment`, `manifest`, and `quartermaster` are clean on all three. **Rejected on evidence:**
`crate` (Rust keyword + 8 Lua files), `cache` (9 Lua files, overloaded), `kit` (collides with Modkit),
`bundle` (our own `bundle.rs`), `depot` (7 Lua files, 45 asset names), `pallet` (7 asset names),
`dispatch` (40 files in our workspace).

| concept | was | now |
|---|---|---|
| the mod package | Dossier | **Shipment** |
| its manifest file | `dossier.yaml` | **`manifest.yaml`** |
| the crate | `mercs2_handler` | **`mercs2_quartermaster`** |
| the CLI | `handler build ./my-dossier` | **`qm build ./my-shipment`** |
| the template repo | `mercs2-dossier-template` | **`mercs2-shipment-template`** |
| the Workshop panel | the Handler | **the Quartermaster** |

## Serialization: YAML preferred, JSON + TOML also accepted

The manifest may be `manifest.yaml` / `.yml` / `.json` / `.toml`. **ONE `serde`-derived model**
deserializes all three, so support is nearly free.
- **YAML is the default/preferred:** the template scaffolds `manifest.yaml`, docs examples are YAML,
  and when the Quartermaster WRITES a manifest (`qm new`, the Workshop panel saving) it emits YAML.
- **JSON** is first-class on read — the natural format for the JS half of the ecosystem (Modkit's
  Tauri/Vue, Wally's web fleet + `EssBridge`), which can read/write `manifest.json` with zero deps.
- **TOML** accepted on read for Cargo-familiar authors.
- **Detection is by extension.** More than one `manifest.*` in a shipment root is an **ambiguity
  ERROR** (loud, not a silent pick) — the failure mode we most want to avoid.
- The Quartermaster is the CANONICAL parser; other tools go through it or a schema it emits.

✅ **Both Phase-1 gotchas are now SETTLED by running the code** (2026-07-25, `mercs2_quartermaster`):

- **(a) YAML crate — decided: `serde_norway` 0.9.42.** `serde_yaml` is archived, and `serde_yml` is
  **DEPRECATED** — its own crates.io blurb calls it "unmaintained… a thin compatibility shim," so the
  draft's "or evaluate `serde_yml`" is struck rather than left as a coin flip.
- **(b) TOML internally-tagged enums — RISK CLOSED.** The warning was written against `toml` 0.x;
  **`toml` is at 1.1.3** and carries `#[serde(tag = "kind")]` through an array-of-tables correctly for
  every v1 kind. The untagged `requires` dual form (bare name | `{url, sha256}`) also round-trips in
  all three. **No manual `kind` dispatch is needed.** Proven by
  `tests/conformance.rs::{yaml_json_and_toml_agree, toml_carries_the_kind_tag_for_every_v1_kind,
  requires_dual_form_agrees_across_formats}`.

⚠ **One real TOML authoring constraint** (a property of the format, not our schema): within a
`[[contributions]]` element, every scalar key must precede any sub-table (`[contributions.textures]`),
or TOML reports a value-after-table error. Worth stating in author docs; it does not affect the model.

## Design decisions baked into this draft (veto any)

1. **Contributions are a single INLINE `contributions:` list, internally tagged by `kind`** — NOT
   per-kind top-level arrays. The multi-format requirement settled it: a `kind`-tagged list serializes
   cleanly and identically across YAML/JSON/TOML AND preserves cross-kind apply order within a
   shipment. (An optional `contribs/` include is a future affordance for huge shipments.)
2. **Blast radius is COMPUTED, not authored** — for every typed kind the Quartermaster derives what it
   touches. Only `[[raw]]` carries an author-declared `touches`. (Plan 01.)
3. **A name is PREFERRED; a bare hash is legal.** ~~Identity is a NAME, never a hash… the author
   never types one.~~ **Corrected 2026-07-28.** Anywhere an existing asset is referenced, a bare
   `0xHHHHHHHH` resolves to that hash and anything else is hashed as a name
   (`manifest::asset_hash`). The linter offers the name when it can reverse one (M0130), as a
   warning that never blocks.

   The original wording mis-cited `no-arbitrary-hashes`. That mandate is about **not fabricating**
   names or hashes — compute them from real sources, never guess a name from a brute-force match
   ("false positives are *guaranteed* and would produce a confidently-wrong name, the exact failure
   [[no-arbitrary-hashes]] exists to prevent", `human_character_controller_code_map.md:1140`). It
   says nothing about which spelling an *author* may write, and reading it as "authors must write
   names" forbids referring to any asset the name table does not cover — a rule the base game's own
   data does not follow, since it ships hashes.

   This was not a harmless doc slip: the builder was written to match it, so `target: "0x6F84F6A3"`
   was hashed as a *string* to `0xC6B71C1F` and failed with "not in the configured game stack —
   check the spelling". What the linter actually guards is a wrong **pairing**, which is what the
   `ch_veh_boat_destroyer` / `0xE54047D5` example below really demonstrates.
4. **Novel assets are ADDITIVE** (own new hash); replacements are same-hash and must be FULLY
   RESIDENT. "Non-destructive" means **the base WAD is never modified** — the change ships as an
   overlay and is reversible by removing that WAD. It does NOT mean the asset's appearance is
   preserved; `replace_texture` legitimately changes how a shipped asset looks. The format cannot
   express a write into `vz.wad` (mandates `no-destructive-replacements`, `never-merge-into-vz-wad`).
5. **Two resolution rules run at once, in opposite directions** — see Composition below. This
   supersedes the old flat "load order is LAST-wins."
6. **Target is explicit:** `retail` | `reimpl`. `both` is REJECTED in v1 (see Open-Q4).
7. **Source files are repo-relative paths under `src/`.** Output is the overlay WAD **plus, for Code-
   layer contributions, file artifacts placed in the game folder** (see The Code layer). It is never a
   base-WAD edit and never an exe edit — those stay unrepresentable, not merely linted.
8. **The game stack is HOST-PROVIDED and never appears in `manifest.yaml`** (next section).
9. **A claim has a write-set AND a read-set** — the Quartermaster computes both (Composition below).

## The game stack is host-provided (settled 2026-07-25)

A build is **not hermetic**. Resolving a `donor`, reading a `replace_texture` target's dimensions, and
auto-picking a host all require the retail WADs — the existing builder already takes them as an
argument: `publish_in_background(wad_paths: Vec<String>, …)` (workshop `publish.rs:79`), documented as
"the live stack order `[base, overlays…]`; donors resolve last-wins."

The **manifest never names an install.** A Shipment must stay portable and copyright-clean, so paths
are host configuration. The Quartermaster crate takes paths in; the *host* decides where they come
from:

| host | resolution |
|---|---|
| Workshop | a **Settings page** (new — Plan 02) |
| `qm` CLI | `--game <dir>` / `--wad <path>`, else the same discovery order |
| template CI | none available ⇒ **lint-only mode**; `qm lint` must work with no game present |

**Discovery order** (first hit wins, and the resolved stack must be VISIBLE in the UI — "which install
was it actually reading" is behind half our trap reports):
1. Explicit user setting (Workshop Settings / CLI flag).
2. **Co-location** — `Mercenaries2.exe` next to the Workshop binary, then `./data/vz.wad`. Community
   members really do drop the Workshop into the game folder; this is a first-class path, and it is the
   only one that works cross-platform.
3. Registry — `registry_vz_wad()` (`mercs2_engine/src/wad.rs:33`):
   `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames` → `Install Dir` + `data\vz.wad`.
4. Nothing ⇒ hard error pointing at Settings. `qm lint` still runs.

⚠ **`registry_vz_wad` is `#[cfg(windows)]`; the other arm returns `None`** (wad.rs:47). On macOS/Linux
there is no discovery at all — the configurable folder is not a nicety, it is the only path on the
maintainer's own machine.

## The Code layer — ASI plugins (added 2026-07-25)

Not every mod is WAD content. A retail Code-layer mod is an **ASI plugin**: a plain Windows DLL with a
different extension, dropped in the game folder. The format must carry these — **our own ecosystem is
already made of them** (`cruise.asi`, `dlc_enable.asi`, and Wally's Lua bridge, Plan 03), so a v1 that
could not express one would be unable to describe its own live-bridge dependency.

**The loader is `pmc_bb.dll`, ours — NOT ThirteenAG's Ultimate ASI Loader.** (`asi_loader_setup.md`
documents the Ultimate loader as an evaluated alternative; that is not the shipped path.) Facts below
read from the binary itself (`dlls/pmc_bb.dll`, **v3.0.0**, 30,208 bytes) plus
`docs/modding_deep_dive.md`:

- **How it loads:** the de-DRM'd `Mercenaries2.cracked.exe` **imports** it (export `BlackboxEntry`,
  ordinal #1) — no proxy DLL, no import-table trickery at mod time. It is also the SecuROM event
  emulator and the `pmc_blackbox.log` writer (what `loadprobe` scores).
- **Plugin discovery:** glob `*.asi` across four roots — the game directory, then `scripts\`,
  `plugins\`, `update\`.
- **`pmc_bb.asi` is reserved** (the loader skips its own name).
- **Load failures are non-fatal and logged**, not silent: `[LOADED] …` / `[FAILED] … (error: …)`
  under an `[ASI Loader]` banner, with `(no .asi plugins found)` when the set is empty. Rare good news
  — this failure mode is observable, unlike most of the WAD ones.
- **Plugins get a logging API:** `pmc_log` / `pmc_log_flush` are exported for them.
- **Hooking substrate is MinHook** (`MH_Initialize` / `MH_CreateHook` / `MH_EnableHook`, with
  `Installed %d/%d hooks (mode=%s)` and a `[SKIP] %s @ 0x%08X` path). Wally's bridge also uses MinHook
  (Plan 03) — so two independent MinHook users can end up in one process.

**The loader is NOT a contribution.** Modkit installs and manages `pmc_bb.dll`. A Shipment
never ships it: N Shipments carrying their own copies would collide on one filename with no
arbitration, and it is not ours to redistribute per-mod. The Quartermaster treats it as a
**prerequisite** and errors with setup guidance when absent.

⚠ `dlls/pmc_bb.dll` and `output/dlls/pmc_bb.dll` in the notes repo **differ** — settle which is
canonical before the Quartermaster starts version-checking the loader.

## Annotated `manifest.yaml` (the preferred form)

```yaml
format: 1                          # schema version (unknown/NEWER = loud reject; older = accepted)

shipment:
  name: sean-devlin-outfit         # slug / id. ^[a-z0-9]+(-[a-z0-9]+)*$, <= 64 chars. Unique;
                                   #   used by deps AND as the output filename.
  title: Sean Devlin Outfit        # human display name
  version: 1.0.0                   # semver of THIS shipment
  authors: ["you <you@example.com>"]
  description: Adds Sean Devlin as a wearable outfit for Mattias.
  target: retail                   # retail | reimpl   ('both' reserved, rejected in v1)
  quartermaster: ">=0.1"           # min mercs2_quartermaster that can build this
  # license, homepage, tags[] — optional metadata

load:                              # all optional
  after: []                        # load-order hints (shipment names)
  before: []
  requires: []                     # hard deps — build fails if absent. Two forms:
                                   #   - a bare shipment name, or
                                   #   - an EXTERNAL artifact: { url, sha256 } for a third-party ASI
                                   #     published on a GitHub release. NEVER vendor someone else's
                                   #     binary; pin its digest instead. See The Code layer.
                                   #   NOTE: cross-shipment references are COMPUTED too (read-set);
                                   #   this field is for deps the Quartermaster cannot infer.
  conflicts: []                    # known-incompatible shipment names

# Contributions: one ordered list, each tagged by `kind`. Source paths are under src/.
# Blast radius is COMPUTED for every kind except `raw` (which DECLARES `touches`).
contributions:
  - kind: add_outfit               # Data(new model) + Script(_tOutfits entry)
    name: sean_devlin              # ASSET identity → pandemic_hash_m2 → _tOutfits.Model.
                                   #   This is what Player.SetOutfit receives (wifpmcinterior:1473).
    slug: SeanDevlin               # _tOutfits.Name — the unlock/tracking key (:1533).
                                   #   MERGE KEY is (wearer, slug), NOT slug alone: retail reuses
                                   #   "Original" and "ChickenSuit" across all three heroes.
    display: Sean Devlin           # _tOutfits.PlayerVisibleName (see Open-Q7 on localization)
    wearer: mattias                # _tOutfits key: chris | jennifer | mattias (wifpmcinterior:155)
    model: src/sean/sean.glb
    donor: pmc_hum_mattias         # host whose rig/materials are BORROWED. READ-only — the donor is
                                   #   never written; we emit a new hash. Omit to let the QM pick.
    textures:
      diffuse: src/sean/sean_d.png
      normal: src/sean/sean_n.png
      specular: src/sean/sean_s.png

  - kind: replace_texture          # Data, same-hash, FULLY RESIDENT
    target: al_hum_boss_ub         # NOTE: _ub is UPPER BODY only. A full character reskin needs
                                   #   _ub + _lb + _head — three contributions, not one.
    image: src/boss/new_boss_ub.png
    # LINTS: warns if target shared (used_by > 1); errors if body < resident mip-chain size
    #   (already enforced by mercs2_formats::texture::build_resident_texture, texture.rs:636)

  - kind: add_model                # Data, new-hash additive
    name: my_custom_helipad
    model: src/helipad/pad.glb
    donor: oc_veh_helicopter_md500 # a REAL model (type_id 19). ⚠ the earlier example here said
                                   #   `deliverycrate`, which has NO ASET row of any type in
                                   #   vz.wad — it is a name in the table, not a hostable asset.
                                   #   Optional in principle; auto-pick is not implemented, so the
                                   #   builder asks rather than guessing.

  - kind: patch_lua                # Script — a DECLARED MUTATION, not a finished block (Composition)
    target: wifpmcinterior         # base script to extend
    append: src/scripts/my_append.lua

  - kind: native_hook              # Code layer — a plugin I AUTHOR and distribute.
    target: retail                 #   To DEPEND on someone else's ASI, use `requires:` with a
                                   #   { url, sha256 } instead — never vendor a third-party binary.
    plugin: src/native/mybridge.asi     # prebuilt DLL. NOT compiled by the Quartermaster.
    # dest is NOT author-settable: the QM places it in the loader's search path. There is
    #   deliberately no way to name Mercenaries2.exe or data/vz.wad here.
    touches: ["0x004CF340"]        # hooked addresses — Exclusive per address (see Composition)

  - kind: raw                      # the OPEN LOWER BOUND — opaque payload
    description: hand-built destruction state machine block
    payload: src/raw/mystate.block
    target_layer: data             # data | script | code | runtime
    touches: ["al_veh_boat_destroyer"]   # DECLARED blast radius — NAMES (see correction below)
```

The identical document in **JSON** is a mechanical transcription — same keys, same `contributions`
list, same `kind` tags — because it's one `serde` model. TOML expresses the list as
`[[contributions]]` tables with a `kind = "…"` field.

## Known contribution kinds (v1 set — extend over time)

| kind | layer | required fields | lowers to (existing code to WRAP) |
|---|---|---|---|
| `add_outfit` | Data+Script | name, slug, display, wearer, model | additive model inject + `_tOutfits` append. An end-to-end builder ALREADY EXISTS to wrap: `wad_builder build-skin` (`wad_builder/src/main.rs:353`) |
| `add_model` | Data | name, model | new-hash single-entry block + ASET row (workshop `publish.rs`) |
| `replace_texture` | Data | target, image | fully-resident container, same hash (`texture::build_resident_texture`) |
| `patch_lua` | Script | target, append | declared mutation → linked at deploy → `mercs2_luac` |
| `edit_state_machine` | Data | target, states | SWIT/STAT/CHDR/CEXE rewrite (`FUN_004cf340`, decoded) |
| `native_hook` | Code | target + (`plugin` \| symbol/detour descriptor) + touches | retail: an **ASI file artifact** placed in the loader search path · reimpl: a Rust/wasm/Lua plugin |
| `raw` | any | payload, target_layer, touches | verbatim bytes into the overlay + declared blast radius |

`donor` is OPTIONAL on every kind that accepts one (resolved Q2) — omit and the Quartermaster
auto-picks a valid host. `retarget_rig` is NOT a standalone kind in v1; it exists only as an inline
`retarget:` sub-block on `add_outfit` / `add_model` (resolved Q6). It wraps workshop `retarget.rs`
(the only `retarget.rs` in the workspace as of `d609592`).

**⚠ `touches` takes a name OR a hash**, like every other asset reference — see the corrected
decision 3. The hazard the earlier wording pointed at is real but is about *mismatched pairs*, not
about hashes being illegitimate: this document's own draft paired `ch_veh_boat_destroyer` with
`0xE54047D5`, but that hash is `al_veh_boat_destroyer` (destruction_orchestrator_format.md:50;
`ch_veh_boat_destroyer` is `0x25FE00A7` — both computed and confirmed). Writing the name, **where
you have one**, is what makes that drift impossible; the Quartermaster warns and offers the name when
it can reverse one via the 23,110-entry `data/production_names.json`.

Each `Easy`/`Core`/`Raw` tier (Plan 01, from Ess) is the SAME kind at different guard-rail levels.

## ★ Composition — how two Shipments combine

The v1 question that most shapes the format. Load order alone does not answer it: the base game has at
least **five distinct composition mechanisms**, each with different merge semantics, and most fail
SILENTLY when violated. All five verified in the decompiled Lua corpus 2026-07-25.

### The engine's own rules come first

Two resolution rules run simultaneously, in opposite directions (field_guide.md:47-53;
fixpack/wad_duplicate_inventory.md §B.4):

| layer | question | rule | code |
|---|---|---|---|
| **WAD stack** | which archive supplies `(asset_hash, type_hash)` | **LAST mounted wins** | `FUN_00875E80` |
| **Runtime chunk registry** | which cell holds a chunk once resident | **FIRST writer wins** (get-or-create) | `FUN_004CC130` |
| **String databases** | which DB supplies a localized key | **LAST registered wins**, then base-language fallback, then NULL | `FUN_0046423e` |
| **ASI plugins** | which plugin gets a hooked address | **NO arbitration — first hook wins by filesystem order** | `pmc_bb.dll` MinHook |

The first two compose as retail intends: the overriding block is picked first at layer 1, so its
chunks reach layer 2 first and win there too. Our merge classes are **derived from** these, not
invented alongside them — where the engine already decides, the format's job is to say when that
decision is acceptable and to warn when it is not.

⚠ **String DBs have a hard cap of 8.** `AddStringDb` refuses past `DAT_011759b8 < 8`
(`FUN_00464540`), deduping by name hash and building the asset name as `<language>_<prefix>`. If every
Shipment that wants custom text registers its own DB, the ninth silently gets nothing. That makes
"register a string DB" a **capped** resource — a merge concern, not just a feature — and a lint rule
the moment `display:` localization is supported.

### The five mechanisms (evidence)

| mechanism | exemplar | merge behavior |
|---|---|---|
| **Native additive API, ungated** | `AddDossierEntry(oPda, sTitle, …)` mrxguipda.lua:1260 | Upsert keyed by `sTitle` into `tDataDossiersIndex` + `table.insert` into the parallel ordered list. Order-independent, idempotent. **Trivially merge-able.** |
| **Native additive API, DLC-gated** | `AddSupportData(tData, sKey)` mrxsupportdata.lua:2459 | Clean key-upsert into `tSupportData` — behind `if not g_bIsDlc then return nil`. This is WHY the heli experiment needed a full block-3185 replace. **The gate is cheap to satisfy** — see Q8. |
| **Source-append of `table.insert`, index-persisted** | `_tOutfits` wifpmcinterior.lua:155 | `_tOutfits` is a GLOBAL, so no literal rewrite is needed — append `table.insert(_tOutfits.mattias, {…})` after the base source. But: save stores a POSITION (`SetProfileCostume(iIndex - 1)`, :1472); index 2 is reserved for the unlock-code outfit (:1426, skipped :1430); the gate is a COUNT (`GetAvailableCostumes` :1518, filtered `nAvailableOutfits >= i` :1430). **Merge-able only with a domain-aware merger.** |
| **Exclusive by construction, silent refusal** | `MrxHq:AddStarter` mrxhq.lua:530 | `if self._tStarter then Debug.Printf("Failed to add …") return end`. One starter per HQ portal; the second is refused to a debug log no player sees. |
| **Cross-reference with silent drop** | `GetAllPotentialShopItems` mrxrewarddata.lua:1488 | `… and MrxSupportData.tSupportData[sId] then` (:1501) — a reward naming a missing support key is skipped in silence. Also MEMOIZED (`if not gtAllSupport[sFactionId]`, :1493), so anything registered after first call is invisible. |

The wardrobe row is the important one to internalize: **the "easy" case is not easy.** Two shipments
each appending one outfit and each hard-coding the availability count produce the same number —
last-wins, and one outfit is in the WAD, in the table, and *unreachable*. It is safe only if the merger
derives the count from the final list length and assigns indices deterministically.

### Merge classes

Every claim in a blast radius carries a class:

- **`Exclusive`** — one claimant; a second is a hard error at deploy. Raw blocks, function
  redefinitions, HQ starters, anything opaque.
- **`KeyedSet { key }`** — many claimants, union by key; duplicate key = hard error. `tSupportData`
  by `sKey`, PDA dossier entries by `sTitle`, `_tOutfits` by **`(wearer, slug)`** (retail reuses
  `Original`/`ChickenSuit` across heroes, so `slug` alone would false-positive), ASET rows by hash.
- **`OrderedList { append_only, derived: [...] }`** — many claimants, append-only, with
  Quartermaster-computed companions. `_tOutfits`: never insert, index 2 reserved,
  `_nAvailableCostumes` derived from final length, index assignment deterministic (sorted by shipment
  name, NOT load order — a saved costume is a position, so load-order churn silently re-dresses the
  player or nils `tOutfits[iIndex].Model` and wedges `STATE_WAITFORGAME`).
- **`LastWins`** — many claimants, later wins, load order IS the answer. Texture replacement.

ASI plugins are the sharpest `Exclusive` case and deserve calling out: plugins **coexist fine** at the
loader (it loads every `*.asi` it finds), but two hooking the SAME address do not — and unlike every
other layer there is **no ordering rule to fall back on**, because discovery is filesystem order
across four directories. So `native_hook` claims are `Exclusive` **keyed on the hooked address**, and
a collision must be a hard error rather than a load-order suggestion: there is no order that fixes it.

**Fail closed.** The Quartermaster only knows `_tOutfits` is an append-only list with a derived count
because WE wrote that down. An unrecognized target ⇒ `Exclusive`, always. That keeps the open lower
bound intact: an unknown script edit is still expressible, it just cannot silently co-install.

### Write-sets and read-sets

The fifth mechanism forces a model change. A claim is not just a write:

- **write ∩ write** ⇒ merge conflict, resolved by the class above.
- **read with no writer** ⇒ **dangling cross-shipment reference**, catchable at deploy.

Shipment A's reward can reference Shipment B's support key; disable B and A's entry evaporates with no
error. Today that is only expressible as a hand-authored `requires:`. It should be COMPUTED — the same
argument as decision 2 — and it generalizes past Lua: the missing-ASET-row wedge and the dangling
`_P001` LOD rungs are the same shape at the data layer, which is why `validate_blocks` already
distinguishes a rejected duplicate primary row from an allowed repeated sub-entry
(`mercs2_formats/src/patch_wad.rs:631`). `donor:` is a read-set entry too — it is borrowed, never
written.

### Consequence: `patch_lua` ships a mutation, not a block

Script entries load from the block, not per-hash, so editing one means shipping all of it —
`wifpmcinterior` is one of the 114 entries in the single `scripts_vz` block. Under one-WAD-per-shipment
+ last-wins, two shipments that each ship a finished `scripts_vz` block do not merge and do not error:
the later one wins and the earlier one's Lua vanishes, model and all.

So `patch_lua` **declares its mutation** (target script + payload + merge class) rather than shipping a
compiled block, and linking happens across the installed set at deploy. This splits **build** from
**link/deploy** — but stays honest: each shipment's `.wad` remains valid standalone, so a solo install
and verify-by-hash both work unchanged, and re-linking only fires when a block has more than one
claimant.

✅ **Corroborated by our own field guide, independently.** Trap 15
(`docs/modding/field_guide.md`) reaches the same conclusion from the modder's side: because
`_tOutfits` is a global, "a mod never needs an AST edit — append source text after the base script and
recompile once… **N mods union by plain text concatenation, compiled once. That is why exactly one
thing must own `scripts_vz`.**" Two independent derivations of link-at-deploy is the strongest
evidence we have that this is the right shape.

Where recipes imply a fixed edit (the `add_outfit` availability-count lift), the QUARTERMASTER owns it
and emits it once at link time — shipments never each ship their own copy. That is what makes
`add_outfit` genuinely composable.

**The domain rules table itself** (which targets are keyed sets, which are ordered lists, what is
derived, what the read-set is) lives in the crate — Plan 01 — as curated content that grows over time.
Plan 04 owns only the four classes, the read/write model, and the fail-closed default.

## Folder layout (what the template scaffolds)

```
my-shipment/
  manifest.yaml           # this file (or .json / .toml — one only; multiple = error)
  src/                    # .glb / .png / .lua / raw payloads referenced above
  build/                  # qm output: <name>.wad + .sha256 + build.log   (gitignored)
  README.md               # author's own description (template ships a stub)
```

## The build (headless — `qm build ./my-shipment`)

1. Locate the manifest (`manifest.{yaml,yml,json,toml}`; multiple = error), parse via the one serde
   model, schema-validate (`format` gate — NEWER than known = loud reject; older = accepted).
2. Resolve the game stack from the host (never from the manifest). Absent ⇒ `qm lint` still runs;
   `qm build` errors.
3. Resolve every `src/` path; hash every `name` via `pandemic_hash_m2`.
4. Compute the blast radius — **write-set and read-set** — from typed contributions; read `touches`
   for `raw`. Two contributions in ONE shipment claiming the same target is a hard error.
5. **Lint** (Plan 01 rules) → Problems. Gated: HANG-class findings FAIL the build (exit nonzero).
6. Lower each contribution (wrap existing builders); assemble ONE overlay WAD.
7. Emit `build/<name>.wad` + sha256 + log. A Shipment with Code-layer contributions ALSO emits its
   file artifacts plus a **placement record** — what goes where, **each entry carrying its SHA-256** —
   because a WAD overlay is reversible by deleting one file, but a file drop is not backable-out
   without knowing what was placed. Modkit's deploy/undo consumes that record, and the hashes make the
   deploy verifiable (integrity only — see The Code layer for what that does *not* mean).
8. Exit code = pass/fail.

⚠ **A prebuilt ASI is arbitrary native code.** The Quartermaster does not compile it and cannot
verify it. The format should record its provenance honestly, the linter should say so out loud, and
neither Workshop nor Modkit should ever execute one as a side effect of inspecting a Shipment. This is
a materially different trust proposition from a `.glb`, and pretending otherwise would be dishonest to
Tier-1 users who install by clicking.

### What we can actually offer today: integrity, not authenticity

**All v1 can do is SHA-256 the payload** (2026-07-25). That is not a new mechanism — it is the
standing `verify-artifacts-by-hash-not-size-mtime` mandate applied to a new artifact class, and
`sha256_hex` already exists (`mercs2_workshop/src/publish.rs:706`). Record the hash of every `plugin:`
payload in the build log and the placement record.

Be precise about what that buys, because the two are easy to conflate:

| ✅ integrity — what hashing gives us | ❌ authenticity/safety — what it does NOT |
|---|---|
| The `.asi` deployed is the `.asi` that was built | Any statement about what the code *does*. A hash of malware is a correct hash. |
| Tamper-evidence between build, distribution, and deploy | Trust in the author. A Shipment that records its own payload's hash proves internal consistency **and nothing else** — an attacker controlling the Shipment controls both the payload and the recorded hash. |
| Corruption and truncation detection on download | Protection from a *hostile* author — self-attestation is circular by construction. |
| Two Shipments shipping "the same" plugin become comparable | Any sandboxing. In-process native code cannot be contained; there is nothing to offer here. |

So a recorded hash is **provenance-of-bytes**, and it only becomes meaningful when attested from
OUTSIDE the Shipment.

### GitHub releases supply that outside attestation (2026-07-25)

ASI plugins are expected to be published on **GitHub release pages**, which carry the hash. That
breaks the circularity above: the digest is served by the plugin author's release, not asserted by
whoever wrote the Shipment. It also means **the Shipment should not vendor the blob at all.**

This splits a distinction the schema was blurring:

| case | how it is expressed | why |
|---|---|---|
| **I wrote this plugin** | `native_hook` with a `plugin:` payload | the Shipment IS the plugin's distribution |
| **I depend on someone else's plugin** (e.g. the Plan 03 bridge) | a `requires:` entry — **URL + pinned `sha256`** | never redistribute a third party's binary; pin the digest so the reference is tamper-evident |

Referencing beats vendoring for the dependency case on every axis: git repos stay small, we do not
redistribute other people's binaries, and the digest is externally attested. Same reasoning as
depending on crates.io instead of committing a vendored `.rlib`.

What it does *not* fix, and the doc should say so:
- **Availability.** Pinning a digest protects integrity, not access — a deleted release or a moved tag
  breaks the build. Some cache/mirror story is eventually needed.
- **Network at build time.** Interacts with the lint-only CI mode; resolving a `requires:` URL must not
  be on the `qm lint` path.
- **Trust simply moves.** "Do I trust this binary" becomes "do I trust this GitHub org." Better —
  it is at least a durable, public identity — but not solved.

⚠ Verify against the current GitHub API before building on it: release assets gained a `digest` field
(sha256) relatively recently, and many projects instead publish a `checksums.txt` asset. Support
reading a digest the author supplies regardless of which of those it came from.

### The step beyond integrity, for our own plugins

`cruise.asi`, `dlc_enable.asi`, and the Plan 03 bridge are built by US. For those, **GitHub Artifact
Attestations** (`actions/attest-build-provenance` + `gh attestation verify`) are worth adopting: they
prove a binary was built by a named workflow from a named commit, which is genuine provenance rather
than self-attestation. Combined with source-available deterministic builds, our own plugins need never
ship as opaque blobs — and that sets the norm we would want the community to copy, rather than us
being the first to hand out an unauditable binary.

The honest v1 posture: pin and verify digests, claim only integrity, and **tell the user plainly that
an ASI runs unrestricted native code in the game process.** Never let a hash check render as a safety
check — a green "verified" reads as "safe" to a Tier-1 user when it only means "unmodified."

**Determinism is required.** Two builds of the same shipment must be byte-identical — no timestamps,
stable iteration order — or verify-by-hash means nothing.

## Open questions

1. ~~Per-kind arrays vs one tagged list~~ — **RESOLVED**: single `contributions` list tagged by `kind`.
2. ~~`donor`/host auto-selection~~ — **RESOLVED: optional everywhere; the Quartermaster auto-picks.**
3. ~~Multi-WAD output~~ — **RESOLVED: one overlay WAD per shipment.**
4. **`target = "both"` — DEFERRED.** v1 accepts `retail` | `reimpl`; `both` is rejected.
5. ~~Format migrations~~ — **RESOLVED: `format: 1`; newer-than-known is a loud reject.**
6. ~~Inter-contribution references~~ — **RESOLVED for v1: inline `retarget:` sub-block**; a general
   contribution-`id` graph waits for a case needing a longer chain than "retarget → outfit".
10. **PLATFORM is a missing axis — OPEN (new 2026-07-25).** Shipments are expected to
    export to **every platform, not just PC**. `target: retail | reimpl` is the ENGINE axis; it says
    nothing about which bake. These are orthogonal, so v1's `target` cannot express "retail, Xbox
    360". The console WADs kept in `game-files/` are a deliberate corpus for this, not stray files.

    | platform | header | endian | blocks |
    |---|---|---|---|
    | PC | `FFCS` | little | `sges` |
    | Xbox 360 | `SCFF` | big | `segs` |
    | PS3 | `SCFF` | big | `segs` |

    ⚠ **Reading a console bake works; WRITING one is a much larger job than an endian flip.**
    `ucfx_byteswap` is console → PC *only*, and it is not a byte sweep — it untiles GPU DXT
    textures, transcodes Xbox-ADPCM/XMA audio to IMA, flips Lua `BINN` bytecode via
    disassemble/reassemble, rewrites Xbox 12-byte vertex elements to `D3DVERTEXELEMENT9`, and
    handles Havok section-aware. The inverse needs texture RE-tiling and XMA *encoding*, neither of
    which exists. So platform support is not "add a field"; the field is the cheap part.

    **PC is the present focus** (2026-07-25). This is recorded so the format does not get
    frozen in a shape that cannot grow the axis — it is not scheduled work.

    Until decided, `mercs2_quartermaster` OPENS console bakes and reports
    `Platform::BigEndianConsole`, and `build` refuses with `ConsoleOutputUnsupported` naming the
    real reason. Mixing platforms in one stack is an error, since resolution walks the whole stack.

7. **Localization of `display:` — STILL OPEN, but the resolver is now READ (decomp, 2026-07-25).**
   The lookup is `FUN_0046423e` (reached via `FUN_00464230` → `FUN_004dd6f1`). Established facts:
   - It takes a **key HASH**, walks the registered DBs **in REVERSE — last-registered wins** — then
     falls back to the base language DB, and **returns 0 (NULL) on a total miss.** There is no
     literal-passthrough at the lookup layer.
   - **Keys are hashed WITHOUT brackets.** Proven by the loading-tip picker (`FUN_00609600`), which
     `sprintf`s a bracket-free `Loading_Set1_%03d`, hashes it with `pandemic_hash_m2`, and uses a 0
     return to detect the end of the set.

   So our notes' "resolver keys on the leading-`[` prefix" (exe_analysis_agent_a.md:534) is **NOT
   confirmed at this layer** — the bracket is stripped or tested by a caller, and which caller does
   what for `PlayerVisibleName` is in the Scaleform/menu path above `FUN_0046423e`, still unread.
   A raw `Sean Devlin` is a perfectly well-formed key that simply MISSES; whether the GUI then draws
   the literal or draws nothing is the one unproven step.
   **Verdict unchanged, evidence much stronger:** do not rely on raw strings. Accept raw + lint
   "unlocalized" until one outfit is shipped and looked at.
   Written up in full: `docs/format_reference.md` §4.1 "Lookup semantics (runtime)".
8. ~~DLC context as a manifest concept~~ — **RESOLVED (docs, 2026-07-25): not a manifest concept at
   all.** `g_bIsDlc` is a plain Lua global, and it is read in **exactly one place in the entire
   decompiled corpus** (`mrxsupportdata.lua:2460`) — so setting it has no other side effects. Our own
   notes say so outright: "*it silently no-ops unless the global `g_bIsDlc` is true… **Set `g_bIsDlc`
   before calling.**"* (01_support_economy_delivery.md:423). A store contribution therefore gets the
   clean `KeyedSet` merge path for the cost of one emitted line of Lua, owned by the Quartermaster —
   no manifest field, no engine work. This also retires the assumption that store items are forced
   onto the block-3185 rewrite path.

**Freeze status:** NOT frozen — and rev 3 is why the question was worth asking. A freeze was proposed
on 2026-07-25 and **reopened the same day by the ASI case**, which broke design decision 7's
"output is always the overlay WAD" and revealed that Plan 01's stated lower bound (which includes
"raw file drop") was wider than what Plan 04 actually implemented.

Recommended shape when we do lock:

- **Freezable now — the envelope.** Identity/version/load/contributions shape, the three
  serializations, extension detection, folder layout, host-provided game stack, the kind set and their
  required fields, naming. `format: 1` covers exactly this.
- **Explicitly NOT frozen — the composition model.** Merge classes, write/read sets, the per-target
  rules. This is crate-internal *computed* behavior and appears nowhere on the manifest surface, so it
  can stay unstable without blocking a schema freeze. It is also the newest and least-exercised part.
- ~~**The one envelope risk that could still reopen it: TOML.**~~ **CLOSED 2026-07-25** — the
  conformance suite is written and green; `toml` 1.1.3 carries the `kind` tag for every v1 kind. The
  last identified envelope risk is gone, which is the strongest argument yet for locking `format: 1`.

Any freeze stays PROVISIONAL until a first Shipment actually builds. Two rev-3 findings argue for that
directly: the `slug` merge key was asserted globally-unique and is actually `(wearer, slug)`, and the
output model missed file artifacts entirely. Both were in parts derived rather than verified.

## Conformance fixtures

The three manifests below are not illustrations — they ship as **test inputs**. Each must parse to the
expected model in all three serializations, and A/B/C together are the seed of the cross-format
conformance suite (Plan 01, Testing). A change to this spec that does not update these fixtures is an
incomplete change.

## Validation-on-paper — CANDIDATE, NOT DONE

Hand-written to probe the schema SHAPE. Asset names below are now VERIFIED against
`docs/data/aset_names.csv` and the Lua corpus; what remains unverified is whether the schema expresses
real mods, which is not answerable until a real Shipment BUILDS.

**A. Texture reskin** — note this is upper-body only; an honest full reskin is three contributions:
```yaml
format: 1
shipment: { name: boss-reskin, title: "Boss Reskin", version: 1.0.0, target: retail }
contributions:
  - kind: replace_texture
    target: al_hum_boss_ub          # ✅ verified real (also _lb, _head, + _nm/_sm siblings)
    image: src/boss_ub.png
```

**B. Raw block** — opaque payload + declared blast radius is enough for the linter to reason without
understanding the bytes:
```yaml
contributions:
  - kind: raw
    description: hand-tuned destruction states for the destroyer
    payload: src/destroyer_states.block
    target_layer: data
    touches: ["al_veh_boat_destroyer"]   # ✅ this is 0xE54047D5 — NAMES, not hashes
```

**C. Outfit** — Sean Devlin, with the three-field identity split:
```yaml
contributions:
  - kind: add_outfit
    name: sean_devlin               # → _tOutfits.Model; what SetOutfit receives
    slug: SeanDevlin                # → _tOutfits.Name; the cross-shipment merge key
    display: Sean Devlin            # → PlayerVisibleName (Open-Q7)
    wearer: mattias                 # ✅ verified key (chris | jennifer | mattias)
    model: src/sean/sean.glb
    textures: { diffuse: src/sean/sean_d.png, normal: src/sean/sean_n.png }
```
The canonical Sean case dodges retargeting: Saboteur and Mercs2 SHARE the human rig (identical bone
hashes + bind pose, memory `pandemic-shared-human-rig-mercs2-saboteur`), so it is a name-hash join. A
general cross-rig source (CoD/Mixamo/GTA) adds `retarget: { from: mixamo }`.

**Still unverified and needing a real build:** donor auto-pick feasibility, `retarget: from: mixamo`
semantics, whether a raw `PlayerVisibleName` renders, and the whole composition model end-to-end.
