---
status: current
evidence: graded per claim (PROVEN / INFERRED / GAP)
---

> **Decisions (user, 2026-10-10):** Q1 exact match incl. load order; Q2 missing `modset.json` = refuse online (fatal, no fallback); Q5 sync adds + removes + reorders, with a snapshot of the previous set for one-click restore; Q7 hosts with local/imported mods may host, joiners are told exactly which mods are unshareable; Q3 unrecorded `.asi` files in the loader folders are hashed into the set as unshareable `local` entries; Q4 the PUBLIC browser hides mismatched games, but games reached through a matching room key or hosted by a friend stay listed, marked "mods differ — sync" (the intended modded co-op route: agree a room key, see the game, sync, join); Q6 one confirmation for the whole sync (natives highlighted); Q8 `<game>/modset.json` is a projection, Modkit's app-data lockfile stays the source of truth. Q9 (REASON letter for a mod-set mismatch) still open.

# Mod-set sync for co-op lobbies — plan (read-only research, 2026-10-10)

Goal: the host's installed mod set travels through our multiplayer server as **names + versions + digests only**. Each joiner compares it with its own and, on mismatch, Modkit fetches what is missing **from the joiner's own trusted sources**. No peer-to-peer bytes.

Grades: **P** = read in code this run, **I** = inferred, **GAP** = not found / undecided. Modkit paths are relative to `/Volumes/Projects/mercs2-modkit/src-tauri/src/`; qm paths to [`tools/wad_simulator/crates/mercs2_quartermaster/src/`](../../crates/mercs2_quartermaster/src/).

---

## 1. What exists today

### 1.1 Quartermaster
- Manifest identity is `shipment.name` (a slug, "unique; used by deps") plus `shipment.version` (semver, enforced) — `manifest.rs:65-87` **P**. `load.requires` is `{shipment, version-range}` or `{capability}` — `manifest.rs:100-134` **P**. `supersedes` lists legacy file names only — `manifest.rs:42-61` **P**.
- `load-request.json` = `{format, items:[{id, path}]}`. The item order is the Kahn tie-break — `plan.rs:26-42` **P**.
- `load-plan.json` (written by preflight and link) = `quartermaster` version, `ok`, `order` (request ids), and `items[]` with `name`, `version`, `plugins[].sha256`, `runtime_dlls`, `placed_files`, `data_files` — `plan.rs:165-238` **P**. It contains no paths ("ids, not paths", `plan.rs:10-11` **P**), and **no provenance**: qm cannot know where a Shipment came from.
- qm writes no record of an *installed* set anywhere. Its outputs go to whatever `--out` it is given — **P** (no such writer in `plan.rs` or `link.rs`). **Since plan format 3 (2026-11-08):** `qm link` also places its `load-plan.json` in the game folder at `scripts/load-plan.json` (a `game_folder` placement), with the libraries it loads; it still carries ids, not provenance (`mercs2_formats::load_plan`).

### 1.2 Modkit
- **Library rows (`ShipmentRef`)** carry `id` (folder-derived, "local only", never to be shown to anyone else), `slug`, `version`, `origin`, `install_reason` — `commands/shipment.rs:117-155` **P**. They are stored **only in webview `localStorage`** (`LIBRARY_KEY`), in `src/stores/project.ts:740-795` **P**. Nothing is on disk, and nothing is in the game folder.
- **`Origin`** = `{source: registry|catalog|local|imported|modkit, id, version}`. A registry id is mercs.ink's opaque `ModResource.id`; a catalog id is `"repo-url#slug"`. An id is comparable only within the same `source` — `models/origin.rs:14-91` **P**. `local` and `imported` entries have `id: None` by design (`origin.rs:93-110` **P**).
- **mercs.ink install** (`install_mercsink_shipment(slug, version: Option<String>, …)`) already installs a **chosen version** together with its dependencies. It verifies every zip against the **GitHub-published sha256 digest**, looked up by tag, and a missing digest is a hard failure — `commands/mercsink.rs:1228-1357`, `:916-959` **P**. The verified zip digest is **not persisted**: `ShipmentRef` has no digest field (`shipment.rs:121-155` **P**).
- **Catalog installs** (`install_catalog_mod`) always take the **latest** release and **do not verify a digest**. The version recorded is the tag — `commands/installer.rs:192-297` **P**. The catalog itself (`registry.rs:23-98`, root `registry.json` + per-repo `repository.json` `{slug,name,description,type,assets,version,incompatible}`) carries no hashes **P**. User custom sources go to `<app-data>/custom_sources.json` (`registry.rs:273-278` **P**).
- **Deploy records** live in app data, not in the game folder:
  - `deployed/deployed-wad.json` = `{installed_at, sha256, byte_size, deployed_at}`, written only after both halves of the deploy land (`commands/deploy_wad.rs:113-142, 526-542` **P**);
  - `deployed/placed-files.json` = per loose file `{abs_path, relative, sha256, shipment}` (`deploy_wad.rs:68-80, 230` **P**);
  - `managed/installed.json` covers components only: pmc_bb, dxwrapper, toolset (`managed/ledger.rs:60-121` **P**).
- The link `load-plan.json` is written to `<app-data>/qm-work/link/`, which is **wiped before every build** (`shipment.rs:416-421, 714-730` **P**).
- **Planned but not built:**
  - the **lockfile** (P-e): per Shipment the resolved version, the selected asset and its verified digest, plus the qm version, as an app-data file; "a record, not a pin" (`.claude/plans/ws2-managed-m2-sdk.md:134, 637-658` **P**). A grep of `src-tauri/src` for `lockfile` found nothing **P**.
  - the **report-safe `PublicPlan`** projection (`ws2…md:533-567` **P**): `items:[{source, public_id|null, version|null, dependency}]` in resolved order, with local ids stripped. Not built **P** (no `report_projection.rs` in `commands/`).
  - `installed_shipments.json` ([`docs/modding/lua_engine_seam_hardening.md`](lua_engine_seam_hardening.md) "Modkit — the installed-provider registry", evidence: inferred). It **does not exist**: a grep of modkit `src-tauri/src`, `src`, `docs` found no hit **P**.
- **Deep links:** only `mercs2-modkit://ship?path=<local path>` is parsed (`shipment.rs:70-78` **P**), dispatched in `lib.rs:61-71, 108-118` **P**. The scheme is registered in `tauri.conf.json:27-29` **P**.
- **Plan-format drift (aside, I).** qm writes plan format 2, with `link_file_paths` and `items[].data_files` (`plan.rs:20, 187-215` **P**). Modkit mirrors format 1 with `deny_unknown_fields` and has neither field (`commands/load_plan.rs:34, 93-133` **P**). Whether this breaks depends on the qm version the toolchain installs. Worth checking before building on the plan. **Since 2026-11-08** qm writes format 3 (adds `libraries`), and the plan's types moved to `mercs2_formats::load_plan` with `Deserialize`, so Modkit can read that one definition instead of its mirror.

### 1.3 Multiplayer stack
- `CreateGame {name, max_players, version (B-version), joinable, attrs (B-U-*), privacy}` and `GameSummary {…, version, attrs, private}` — `mp_api/src/ws.rs:58-130` **P**. `JoinAccepted` and `Push::JoinDenied {reason: String}` — `ws.rs:233-258` **P**.
- `ClientInfo {build_sha256, asi_version}` is sent once per login on `POST /v1/sessions` (`mp_api/src/rest.rs:22-38` **P**) and stored per session (`mp_server/src/db.rs:106-119`, `api.rs:152-174` **P**). This is the natural home for a per-process fact like the mod set.
- `GamesJoin` on the server checks only the room key and friends (`mp_server/src/hub.rs:374-378` **P**). The `ErrorCode` set has no mismatch code (`mp_api/src/lib.rs:20-45` **P**).
- ASI: reads `<asi stem>.ini` beside itself via `GetModuleFileNameW`, and a failed read or parse is `fatal` (`client/src/win/mod.rs:268-284` **P**). EGAM goes to `games.join`; a refusal becomes `'jden'` `REASON=f`, and a pushed denial becomes EGEG `'jden'` + `REASON` (`client/src/theater.rs:22-25, 436-445, 728-735` **P**). The ASI registers a Lua namespace `Mp` (`GetRoomKey`/`SetRoomKey`) that `mp_menu.lua` uses (`client/src/lua/win.rs:102-107`, `src/mp_menu.lua:20-65` **P**). That is a channel for showing our own message.
- **B-version is not a mod fingerprint.** It is a baked/Region-derived constant, and a live sweep across mod stacks gave `-320369524` every time (memory `fesl-bversion-builder.md:30-31`; modkit `docs/mercs2_install_registry_contract.md:5-21` ties it to `Region`) **P (live, 2026-06-30)**. Leave B-version alone and carry the mod set separately.
- **GAP:** how the game presents each `REASON` letter (`c/f/h/q/r/t/w`, catalogue `multiplayer_txn_catalogue.md:92,100`). No string mapping was found in the corpus. Treat in-game text as unknown until it is read in x32dbg or the decomp.

---

## 2. The canonical record — reuse the planned lockfile, deploy a copy into the game folder

No existing on-disk file has provenance, versions and digests together, and nothing is in the game folder. The closest designed artifact is the **P-e lockfile** plus the **`PublicPlan` projection**. Proposal: build the lockfile as designed, with one addition. At deploy, Modkit writes a **projection of it** into the game folder. The lockfile stays the single source of truth; the game-folder file is a derived copy, written in the same step as `deployed-wad.json` (after both halves of the deploy land, `deploy_wad.rs:526-542`).

**`<game>/modset.json`** (name is an open question), format 1:

```json
{ "format": 1,
  "quartermaster": "0.x.y",
  "shipments": [                                   // qm plan `order` (resolved load order)
    { "source": "registry", "public_id": "<mercs.ink ModResource.id>",
      "name": "lua-bridge", "version": "1.0.0",
      "asset": "lua-bridge-v1.0.0.zip", "asset_sha256": "<hex>",
      "dependency": true } ],
  "other": [                                       // non-Shipment load-order entries
    { "source": "catalog", "public_id": "https://github.com/o/r#slug", "version": "v1.2",
      "files": [ { "relative": "scripts/x.asi", "sha256": "<hex>" } ] },
    { "source": "imported", "public_id": null, "files": [ … ] } ],
  "digest": "<sha256 of the canonical JSON of {format, shipments, other} without `digest`>" }
```

- **Writer: Modkit only.** qm cannot know `source`/`public_id`, and its plan has no provenance (`plan.rs:194-216`). From qm it takes `order` and `version`. From the lockfile it takes `asset`/`asset_sha256`. From `Origin` it takes `source`/`public_id`.
- **Rows that need new data:**
  - `asset_sha256` must start being **persisted**: today `verify_asset_digest` returns it and it is dropped (`mercsink.rs:921-959`).
  - Catalog installs need a digest **computed at download**, because none is verified today (`installer.rs:265-273`).
- **Digest.** sha256 over canonical JSON: sorted keys, no whitespace, `shipments` in resolved order. Computed by one shared Rust function (§5, M1), so Modkit, ASI and server cannot drift.
- **Which fields count for a match** (recommendation; the decision is the user's, Q1):
  - `(source, public_id, version, asset_sha256)` for every Shipment, and the per-file sha256 for `other`.
  - **Load order included.** The same set in a different request order can link to a different `vz-patch.wad`, because request order is qm's tie-break (`plan.rs:34`).
  - The `vz-patch.wad` sha256 is **excluded**: whether `qm build`/`link` is byte-reproducible across machines (game data, workshop corpus, qm version) is **GAP/unproven**.
  - The `quartermaster` version is excluded for the same reason, but displayed.
  - Placed-file bytes are excluded, because users edit `multiplayer_restored.ini` (room key), which is a `place_file` (`shipments/multiplayer-restored/manifest.yaml:92-95`). That is why the match is at **release-asset level**.

---

## 3. Data flow

1. **Install/build (Modkit).** The library is resolved, built and deployed, and the lockfile is written (P-e). Deploy writes `<game>/modset.json` last. The "no mods" case is an explicit file with empty arrays, never an absent file.
2. **ASI startup.** Read `<game root>/modset.json`. The game root is the exe directory (`GetModuleFileNameW(NULL)`), not the ASI directory. Parse strictly (`deny_unknown_fields`) and recompute the digest.
   - Missing, malformed, or digest mismatch → **fatal**, exactly like the ini (`win/mod.rs:281-284`), with a message naming the file and telling the player to redeploy from Modkit.
   - **Q2:** should a hand-installed game with no Modkit be able to play online? If yes, that would be a fallback, and that is the user's call.
   - **Optional check (Q3):** enumerate `*.asi` in the loader's scan folders (`.`, `scripts`, `plugins`, `update`; `deploy_wad.rs:53-55`) and refuse unrecorded ones. pmc_bb loads them all, so an untracked ASI is an undeclared mod.
3. **Login.** `ClientInfo` gains `modset: ModSet`. The server validates its shape and size limits (new `rules::MODSET_*`) and stores it on the session.
4. **games.create.** The server copies the **host session's** mod set onto the game. `CreateGame` stays unchanged, so the host cannot claim a set different from the one it logged in with.
5. **Listing.** `GameSummary` gains `modset_digest` and `modset: Vec<ModSetEntrySummary>` (name, version, source). Displaying it needs our own UI, because the GDAT fields the game renders are fixed (I). The ASI can put it behind `Mp.*` Lua for `mp_menu.lua`.
   - **Q4:** list only matching games, or list all and mark mismatches?
6. **Join.** The server compares the joiner session digest with the game digest in `GamesJoin` (`hub.rs:374`). On mismatch it returns new `ErrorCode::ModSetMismatch` and **does not** queue a request to the host. The server is authoritative here, because the ASI cannot be trusted to check its own set.
   - The ASI maps the error to `'jden'`. Which `REASON` letter is a **GAP** (see §1.3); `f` is what other refusals use today (`theater.rs:22`).
7. **Handoff to Modkit.** The ASI writes `<game>/modset-sync-request.json` = `{host_modset (verbatim from the server), host_name, game_id, written_at}`. It then fires `mercs2-modkit://sync` with **no payload**, and exposes the outcome to Lua (`Mp.GetLastSyncRequest()`) so the menu can say "Mod mismatch — Modkit opened to sync".
   - **GAP:** whether `ShellExecuteW` on a custom scheme reaches the host OS under Wine/Proton (I: Wine forwards unknown schemes through `winebrowser`; unverified). On macOS/Linux Modkit is the launcher (`launch.rs`), so it can also pick the request file up when the game exits.
8. **Modkit sync.** It reads the request file as **untrusted names** and diffs against the lockfile. For each missing or different Shipment:
   - `registry` → `install_mercsink_shipment(slug=name, version=Some(v))` (`mercsink.rs:1236-1264`) against the **player's configured** mercs.ink base URL. The downloaded zip's verified GitHub digest must equal the advertised `asset_sha256`, else hard-fail.
   - `catalog` → resolved only if `repo-url` is in the player's registry/custom sources. Needs **version-pinned** install + digest (new).
   - `local`/`imported` → **unresolvable**, named in the UI ("the host runs a locally built mod `X`; get it from them out-of-band").
   - Then the reorder/removal plan; preflight → build → link → deploy (existing path); rewrite lockfile and `modset.json`; show the diff; the player relaunches and retries. **Q5:** does sync also remove the joiner's extra mods and reorder, or only add? (An exact match needs both.)

---

## 4. Security model

- **The server and host can only name things.** Bytes come only from (a) mercs.ink metadata at the **player's** configured base URL + GitHub release assets whose sha256 GitHub publishes (`mercsink.rs:156-161, 916-959`), or (b) catalog repos the player already trusts (`registry.json` + `custom_sources.json`).
- **Hash pinning.** An advertised `asset_sha256` must match **both** GitHub's digest and the downloaded bytes. Any disagreement is a hard failure, and nothing gets installed (`mercsink.rs` already stages everything before committing, `:1299-1332`).
- **Unresolvable entries** (unknown `public_id`, a repo not in the player's sources, `local`/`imported`, a digest mismatch, a yanked release) refuse the sync and name the entry. No "install anyway".
- **Native code.** ASI/DLL Shipments (`plugins`, `runtime_dlls`, `plan.rs:218-250`) need an explicit per-Shipment confirmation listing the files and the repo, even when the source is trusted (Q6). Lua-only Shipments run inside the game's sandbox-less Lua, so they are effectively code too (I). Q6 decides whether they get the same prompt.
- **Input hardening.** The sync request is parsed with `deny_unknown_fields` and size limits:
  - names must match the qm slug regex (`manifest.rs:66`);
  - versions must parse as semver;
  - `public_id` is an opaque string, never used as a URL or path;
  - a catalog `public_id`'s repo half must match a configured source byte-for-byte.
- **Deep links never accept a path** for sync, unlike `ship?path=` (`shipment.rs:72`).
- **Privacy.** The advertised set uses the `PublicPlan` allowlist (no local ids, paths or folder names; `ws2…md:539-566`). `imported` entries expose only file names and hashes; whether that is acceptable is Q7.

---

## 5. Milestones (ordered; each gated on exit code)

| # | Component | Change | Tests |
|---|---|---|---|
| M0 | Modkit | Reconcile `load_plan.rs` with qm plan format 2 (aside §1.2) | parse a real `qm preflight` output fixture from the pinned qm |
| M1 | new shared crate (`mp_api::modset`, or a small published crate Modkit can depend on without `path =`, per modkit CLAUDE.md:108) | `ModSet` types, strict parse, canonical JSON, `digest()` | golden-vector digests; key-order/whitespace invariance; order sensitivity; unknown-field refusal; size limits |
| M2 | Modkit | Persist `asset_sha256` on `ShipmentRef`/lockfile; build the P-e lockfile and `PublicPlan` (sentinel no-leak test as designed, `ws2…md:556-567`) | lockfile written per build; digest survives restart; no-leak test |
| M3 | Modkit | Catalog installs: digest at download, version-pinned install by tag | tampered asset refused; pinned tag installed |
| M4 | Modkit | Write `<game>/modset.json` at deploy (after placements, before `deployed-wad.json`); remove it on uninstall | temp game root: file present, digest matches lockfile; "no mods" = explicit empty set; failed placement → no file |
| M5 | mp_api + mp_server | `ClientInfo.modset`; `GameSummary.modset_digest/modset`; `ErrorCode::ModSetMismatch`; join check in `hub.rs` | wire-shape tests (as `ws.rs:273-338`); join with mismatched digest → error, host receives no `JoinRequest`; matched → unchanged flow |
| M6 | ASI | Read/verify `modset.json` at startup (fatal on error); send in session; map mismatch → `'jden'`; write sync request; fire deep link; `Mp.*` getters | unit: parser + fatal paths; theater test like `theater.rs:1266-1275` for the mismatch frame; testserver e2e |
| M7 | Modkit | `mercs2-modkit://sync` handler + sync planner (diff, resolve, confirm natives, install, reorder/remove per Q5, rebuild, redeploy) | hermetic: fake mercs.ink + GitHub (existing `serve()` harness, `mercsink.rs:1800-1858`); unresolvable entry named; digest mismatch → nothing installed |
| M8 | live | Two machines (or Windows + Wine): host with set A, joiner with B → denied → sync → relaunch → join succeeds | sha256 of `vz-patch.wad` + `modset.json` before/after; ASI log; loadprobe on both |

Docs to update with each step: [`docs/modding/manifest_format.md`](manifest_format.md) (if any manifest field changes, none proposed), the `mp_api` module docs, the modkit `CLAUDE.md` deep-link section, and `multiplayer_txn_catalogue.md` (the REASON mapping once found).

---

## 6. Open questions for the user

1. **Match strictness:** exact `(public_id, version, asset_sha256)` with load order (recommended), or the same set at any order, or semver-compatible versions?
2. **No `modset.json`** (game set up without Modkit): refuse to go online (no fallback), or treat a missing file as "no mods"? The second is a fallback, so it is your call.
3. **Unrecorded `.asi` files** in the loader folders: refuse online play, or ignore them?
4. **Lobby browser:** hide mismatched games, or list them and mark them?
5. **Sync scope:** add only, or also remove extras and reorder to an exact match? Should the joiner's previous set be snapshotted for one-click restore (`deploy_wad.rs` already banks the old WAD)?
6. **Native-code consent:** a per-Shipment confirmation for ASI/DLL Shipments during sync? And for Lua-only Shipments?
7. **Local/imported mods on the host:** allow hosting (joiners then cannot sync), or refuse hosting with an unshareable set?
8. **File name and location** of the game-folder record (`modset.json` at the game root is proposed), and whether the lockfile should be that file (one file) or app data plus a projection (proposed; it keeps app data the source of truth).
9. **Which `REASON` letter** to use, and whether to read the game's REASON→text mapping live in x32dbg first (GAP §1.3).
