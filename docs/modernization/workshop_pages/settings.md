# Settings — behaviour contract

Artboard: `Main.dc.html` ("Settings · Game archive") of the Workshop Polish Pass design.
Capture states: `crates/mercs2_workshop/ui_states/settings.json` (a saved path missing, `--wad` in
use) and `settings_unset.json` (no game archive set or found, and `MERCS2_WORKSHOP_DATA` pointing
at a folder without `manifest.json`: both sections red).

Settings is where every path the tool cannot discover for itself is set, and where the app opens
when one of them is unset or broken. Every row is an element of the page: what the user does, what
happens, where the data comes from, what it writes, and what proves it.

## The two chains, first match wins

**Game archive** (`vz.wad`):

1. `--wad <path>` — this launch only
2. this run's choice — "Use for this run only…" (new; this launch only)
3. saved setting — `settings.json` `wad_path`
4. beside the exe — `Mercenaries2.exe` next to the workshop (`colocated_vz_wad`)
5. `MERCS2_GAME_DIR` / `VZ_WAD`
6. the Windows registry key (`wad::registry_vz_wad`; "not on this platform" elsewhere)

**Reference data** (`workshop_data/`):

1. this run's choice — "Use for this run only…" (new)
2. `MERCS2_WORKSHOP_DATA`
3. saved setting — `settings.json` `data_dir`
4. `workshop_data/` beside the exe
5. `workshop_data/` in the working directory or a parent
6. the downloaded copy (`fetch::installed_dir`)

**An explicit source that is broken stops its chain.** `--wad`, this run's choice, the saved
setting and the environment variables are things a person set, so a broken one is never passed
over for the next source. Today `main.rs` and `index::data_home` both quietly move on to the next
source. A broken explicit source with nothing above it in use leaves the chain **unresolved**.
*Broken* means: the file or folder does not exist, a `vz.wad` does not open as a game archive
(`settings::check_wad`), or a folder holds no `manifest.json` (`settings::check_data_dir`).

The discovered steps (beside the exe, the working directory, the registry) are searched, not set,
so a step that finds nothing is passed over. The repo's own partial `workshop_data/` has no
manifest and is passed over too.

**Reference data with nothing set and nothing found** is downloaded (`index::require_data_home`,
as today). Download is offered only when no explicit source is set.

## Startup

| Situation | Effect | Proof |
|---|---|---|
| Both chains resolve | Boot screen, then the app as today, on the last page shown | test: resolution per chain |
| Either chain unresolved | The window opens on **Settings**, on the first unresolved section. That section's navigator row and its broken chain row are red. Rail pages other than Settings are disabled with the hover text "Set the game archive first" or "Set the reference data first". Nothing is loaded yet | test + capture `settings_unset.json` |
| An unresolved chain is fixed (Choose… or Use for this run only…) | When both chains resolve, the boot screen runs: catalog, then names and corpora. The app proceeds without a restart, landing on the page last shown, or Inspect | test: unresolved → resolved transition starts the boot |
| A headless run (any mode flag) with a chain unresolved | Exits 2 with the chain's rows and why each was not used, on stderr. No window, no picker | test |
| Tests (`MERCS2_WORKSHOP_DATA` pointing at an incomplete folder) | `data_home` returns the error naming the variable and the folder. It never falls back to the downloaded copy | test |

The first-run picker in `main.rs` is removed: Settings replaces it. That picker set up the macOS
app before winit's event loop existed, and winit then panicked. The event loop is now created
before anything else in the GUI launch.

## Elements

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Header crumb | `SETTINGS  <section>` | settings section | — | test |
| Navigator | Game archive · Reference data · Shipments library · Shortcuts · About & diagnostics, each with its sub-line; click selects. Badge: **red** when that chain is unresolved, amber **CHECK** when a higher source is in use but an explicit one below it is broken, none otherwise | chain status | — | test: badge per chain state |
| Banner (Game archive) | Shown when the archive in use came from `--wad` or this run's choice and the saved setting is missing or broken: "SAVED PATH NOT FOUND — the saved vz.wad no longer exists, so this run used <source>. Nothing on disk was changed." **Save this run's path** writes the path in use as `wad_path`. **Clear saved** removes `wad_path`. The banner goes once neither applies | chain status | `settings.json` | test: banner conditions, both buttons write |
| Banner (Reference data) | The same, for `data_dir` with this run's choice in use | chain status | `settings.json` | test |
| WHERE VZ.WAD CAME FROM | One row per step of the chain, numbered, "first match wins". Status: **IN USE** (green) · **FILE MISSING** / **NOT AN ARCHIVE** (red when it left the chain unresolved, amber when a higher step is in use) · **not set** · **not reached** · **not on this platform**. Each row shows its path where it has one; hover gives the full path | the resolver, kept as rows instead of a bare `Option` | — | test: rows per scenario (saved missing + `--wad`; env broken; nothing set) |
| WHERE THE DATA CAME FROM | The same, for the data chain; statuses **IN USE** · **NO MANIFEST** / **MISSING** · **not set** · **not found** · **not reached** | the resolver | — | test |
| WHAT IT OPENED (Game archive) | base `vz.wad`; overlay `vz-patch.wad` "found beside it", or "none"; catalog `<models> models · <textures> textures`. Hidden while nothing is loaded | `WadStack`, `AssetIndex` | — | capture |
| WHAT IT OPENED (Reference data) | bundle folder, names loaded, Lua scripts (the manifest carries no release tag) | `AssetIndex::names`, the Lua corpus | — | capture |
| Choose vz.wad… / Choose folder… | Native picker. The choice is checked (`check_wad` / `check_data_dir`); a failure is shown under the buttons and nothing is written. A good choice is saved and **loaded in place**: see *Reloading* below | `settings` | `settings.json` | test: bad choice writes nothing |
| Use for this run only… | Native picker, same check. A good choice becomes step 2 (archive) or step 1 (data) for this launch; nothing is written. Loaded in place, as Choose… | process-wide run override | — | test: override wins over env and saved; nothing written |
| Clear saved | Enabled only when the setting is saved; removes it. The chain re-resolves; if that leaves it with no answer, what it fed is unloaded (a game archive takes the bundle with it), the window is on Settings and the section turns red | `settings` | `settings.json` | test |
| Download (Reference data) | Offered only when no explicit data source is set. Fetches `mercs2-workshop-data.zip` from the latest release into the download cache, with a spinner and stage text. On success the chain re-resolves to the downloaded copy (step 6) | `fetch::download` | download cache | test: offered only with no explicit source; capture of the download running |
| Reveal folder | Opens the folder of the source in use in the OS file browser. Disabled while unresolved | `quartermaster::open_in_os` | — | — |
| Footer | READY + live load summary, as today. While unresolved: "SETUP" + "Set <the unresolved chains> to load the game" | status | — | capture |

Shipments library, Shortcuts and About & diagnostics are unchanged by this board. About's
"Resolved stack" and "bundle" rows read the same resolver, so they agree with the chains above.

## Reloading

When the chain in use changes while the workshop is running (Choose…, Use for this run only…,
Clear saved, or a finished Download), the workshop reloads in place, with no relaunch:

- A changed **game archive** closes the open `WadStack` and re-runs the whole boot: catalog, kit
  templates, AnimationLookup, then names and corpora. State tied to the old archive is cleared:
  the loaded model and its preview, the clip loader, the open Audio bank, a bench's donor, and the
  Saves and Shipment pages' resolved stack. Each page shows its empty state until the user picks
  again.
- A changed **reference bundle** re-runs only the bundle half of the boot: names, Lua corpus,
  Missions reference, audio inventory. Names are re-applied to the existing catalog. Pages that
  read the corpus (Missions, the Lua references, Audio) are reset to their empty state.
- The loading screen shows while the reload runs, then Settings again, with the new source
  **IN USE**.
- A reload that fails (a bundle with a malformed file, say) leaves that chain red on Settings
  with the error. It does not show the boot error screen.

A Download into the folder already in use (the downloaded copy) reloads it too: the path is the same
but its files are new. A changed archive also unloads the preview's and the donor's meshes from the
scene, which keeps uploads by model hash.

Proof: test that a chain change starts the reload with the new source (`next_step`); captures after
a reload, through `--ui-state`'s `run_wad` / `run_data` (Settings' "Use for this run only…" applied
after the first load, captured once the reload lands): the run's choice IN USE with what it opened,
and a model loaded before the reload gone after it. Those states name local paths, so they are not
committed.

Building and installing a Shipment while the game is running is Modkit's concern. The workshop
does not check for a running game.
