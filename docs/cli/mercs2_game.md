# mercs2_game

`mercs2_game` **is** the Mercenaries 2 game exe: it configures the asset-agnostic `mercs2_engine`
from the player's real `.profile` save and boots the full third-person world in-process (there is no
separate engine binary).

The tool hand-rolls its argument parsing with `std::env::args()` — there is **no `--help`, no `-h`,
and no `--version`**. Flags are matched literally, most in `main.rs` but a few directly in
`world.rs` during the world boot (`--interior`, `--markers`); anything unrecognised is silently
ignored (or, if it does not start with `--`, treated as a positional profile path).

## Synopsis

```
mercs2_game                                  # retail flow: main menu -> save browser -> world
mercs2_game <SAVE.profile>                   # boot that save directly, no menu
mercs2_game --plan [SAVE.profile]            # print boot-state banner, do not render
mercs2_game --stream [SAVE.profile]          # alternate free-fly streaming world
mercs2_game --interior-orbit                 # menu/world boot + debug orbit camera

mercs2_game --game-dir <INSTALL> ...         # any of the above, against that install
mercs2_game --saves-dir <FOLDER> ...         # any of the above, against that saves folder
```

When run from source: `cargo run -p mercs2_game -- <args>`.

Every mode needs `vz.wad`. The install resolves from `--game-dir`, then `MERCS2_GAME_DIR`, then
`VZ_WAD`, then (Windows only) the EA Games registry key (see [Failure modes](#failure-modes)).

## Options

`mercs2_game` takes at most one positional argument and a handful of `--` flags. No flag takes an
`=value` form; value-carrying flags read the **next** argument. No flag is repeatable in any
meaningful way (each is tested with `args.iter().any(...)` or `.position(...)`, so a second
occurrence is ignored). None is strictly required — with no arguments the tool opens the retail
shell menu.

### Positional

| Argument | Type | Default | Required | What it does |
| --- | --- | --- | --- | --- |
| `<SAVE.profile>` | path | newest `.profile` in the saves folder | no | The first argument that does **not** start with `--` (after argv[0]) and is not the value of `--game-dir`/`--wad`/`--saves-dir`/`--saves` is taken as an explicit save path. Its presence suppresses the shell menu and boots that save directly. If omitted (and not in menu mode), the newest `.profile` in the resolved saves folder (see [Root flags](#root-flags)) is used. |

### Root flags

Both take the **next** argument as their value; a next argument that starts with `--` is not taken.

| Flag | Value | Default | What it does |
| --- | --- | --- | --- |
| `--game-dir` (alias `--wad`) | install root, its `data` folder, or `vz.wad` itself | `MERCS2_GAME_DIR`, then `VZ_WAD`, then (Windows only) `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames\Install Dir` | The install the assets are read from. |
| `--saves-dir` (alias `--saves`) | folder | `MERCS2_SAVES_DIR`, then `Documents/My Games/Mercenaries 2/SaveGames` under `$USERPROFILE` / `$HOME`, then `<install>/SaveGames` | The folder `*.profile` saves are listed from. An explicit folder is used whether or not it exists; every other source must exist. |

### Boot-mode flags

| Flag | Value | Default | What it does |
| --- | --- | --- | --- |
| *(none)* | — | — | **Retail flow.** Scans `SaveGames\*.profile` (header-only) and opens the main menu (`menu::Menu`); the player picks Continue / New Game / Load Game / Quit and the chosen save drives the world boot in-loop. New Game uses Mattias, upgrade tier 0, default skin. Only reached when there is no explicit profile, no `--plan`, and no `--stream`. |
| `--plan` | — | off | Resolve the profile (explicit or newest), parse it to header + `SaveState`, print the boot-state banner (profile name, active contract, playtime/cash/fuel, flow chain, active missions, `vz_state` overlay layers, and the resume-branch spawn marker `Pmc_Entry1`), then **return without rendering**. No window opens. |
| `--stream` | — | off | Boot the alternate **free-fly streaming world** (`mercs2_engine::game_world::run_game_world`) at the PMC-interior spawn with the save's overlay layers, populating the PMC interior via `populate_pmc_interior`. Suppresses the shell menu. |
| `--interior-orbit` | — | off | Adds the engine's **debug orbit camera** to the interior. Read only in the shell-menu boot and the default full-world boot; **ignored under `--stream`** (that branch never consults it). |
| `--interior` | — | off | **Skips the streaming world** (base streaming terrain, overlay-layer lights and the streamer) during the world boot (checked in `world.rs`, not `main.rs`). The interior room sits above the SE terrain peak and the exterior would occlude it, so the boot stays fully static. Active in the default full-world boot and the shell-menu boot. |
| `--markers` | — | off | Spawns the placement-marker **debug glyphs** for the loaded block (checked in `world.rs`). Only has an effect when the boot has placement data (`data.placements`); otherwise silently does nothing. |

### Environment variables

The root variables stand in for the root flags when those are absent:

| Variable | Effect |
| --- | --- |
| `MERCS2_GAME_DIR` | The install, in any form `--game-dir` takes. |
| `VZ_WAD` | Same forms as `MERCS2_GAME_DIR`; read after it. |
| `MERCS2_SAVES_DIR` | The saves folder; used only if it exists. |
| `USERPROFILE` / `HOME` | Home directories probed for `Documents/My Games/Mercenaries 2/SaveGames`. |

These are read inside `load_pmc_interior`, so they affect any boot that assembles the PMC interior
(default full-world and `--stream`). They are diagnostic toggles — presence of the variable (any
value) enables them.

| Variable | Effect |
| --- | --- |
| `MERCS2_FURNDBG` | Prints a `[furn]` per-item floor-Y line for each furniture placement (`pos.y + mesh bmin.y`). |
| `MERCS2_ALLNAMES` | Prints a `[name]` line for every placement name in each interior state block. |
| `MERCS2_ALLBLOCKS` | Overrides recruit-driven block selection and loads **every** interior variant block `[667, 711, 461, 703, 291]` — shows every recruit's bay regardless of the save. |

## How the options combine

The single most important thing to understand is the **decision order in `main`**, because it is
strictly first-match-wins and several flags shadow each other:

1. **The roots are resolved first** — install (`--game-dir` → `MERCS2_GAME_DIR` → `VZ_WAD` →
   registry) and saves (`--saves-dir` → `MERCS2_SAVES_DIR` → home probes → `<install>/SaveGames`).
   No `vz.wad` ends the run here with exit 1, before any other flag is looked at (`--plan` included).

2. **Then** three things are computed:
   - `plan_only` = `--plan` present;
   - `explicit` = the first non-`--` argument that is not a root flag's value = the positional
     profile path (or none);
   - `--stream` presence.

3. **Shell-menu boot is taken only when all three are false**: no explicit profile **and** no
   `--plan` **and** no `--stream`. In other words, *any* of an explicit profile path, `--plan`, or
   `--stream` **suppresses the retail menu** and forces a direct/dev path. `--interior-orbit` alone
   does **not** suppress the menu (you still get the menu, now with the orbit camera).

4. **Otherwise the profile is resolved** (explicit path if given, else newest `.profile`), parsed,
   and the boot-state banner is printed. Then:
   - `--plan` **wins over `--stream`**: the banner is printed and the function returns *before* the
     `--stream` check, so `mercs2_game --plan --stream` never renders. `--plan` is effectively a
     terminal "print and stop."
   - If not `--plan`, `--stream` selects the free-fly streaming world; otherwise the default
     full-world TPS boot runs.

Flag-interaction summary:

- **`--plan` overrides `--stream`** (plan returns first) and overrides the default full-world render.
  A `--plan` run opens no window regardless of any other flag.
- **`--stream` overrides the default full-world boot** and suppresses the menu, but is itself
  overridden by `--plan`.
- **An explicit `<SAVE.profile>` overrides "newest save" selection** in every non-menu path, and its
  mere presence suppresses the menu. Under the menu path it is never reached (the menu picks the
  save at runtime).
- **`--interior-orbit` is only meaningful in the menu boot and the default full-world boot.** It is
  read but has no effect under `--plan` (nothing renders) and is not read at all under `--stream`.
- **The heroes' look differs by path.** The menu's New Game always boots Mattias / upgrade tier 0 /
  default skin (`player_model_candidates(1,0,0)`). A direct profile boot (default full-world path)
  instead uses **this profile's** `character_index` (header `@0x4D`, 1-based) and `upgrade_index`
  (`@0x4F`), so the on-screen character and skin change with the save. `--stream` does not select a
  player model this way.
- **The save drives the interior content** on both rendering paths: `unlocked_starters` →
  `RecruitUnlocks` picks which recruit-bay state blocks load, and `cash` seeds the stockpile so only
  the stockpile tiers the save has reached are shown. `MERCS2_ALLBLOCKS` overrides the recruit-driven
  block choice.
- **Root-flag values do not collide with the positional profile.** The argument after `--game-dir`,
  `--wad`, `--saves-dir` or `--saves` is skipped by the positional-profile scan (`VALUE_FLAGS` in
  `main.rs`), so `mercs2_game --game-dir <install>` never treats the install folder as a save.

## Examples

```sh
# Normal play: main menu, pick a save, world boots in-loop.
mercs2_game
```

```sh
# Boot one specific save directly, no menu — character/skin come from that save's header.
mercs2_game "C:\Users\me\Documents\My Games\Mercenaries 2\SaveGames\auto_634304EA.profile"
```

```sh
# Inspect what a save would boot into (contract, flow chain, active missions, overlay layers,
# spawn) without opening a window.
mercs2_game --plan
```

```sh
# Same, but for a specific save.
mercs2_game --plan "C:\...\SaveGames\slot1.profile"
```

```sh
# Free-fly the streaming world at the PMC spawn with the newest save's overlays.
mercs2_game --stream
```

```sh
# Point at an install and a saves folder explicitly (no registry, no environment).
mercs2_game --game-dir "C:\Program Files (x86)\EA Games\Mercenaries 2 World in Flames" --saves-dir "D:\m2saves"
```

```sh
# Boot a save and print a per-item floor-Y line for each PMC interior furniture placement.
MERCS2_FURNDBG=1 mercs2_game "C:\...\SaveGames\slot1.profile"
```

## Failure modes

- **No `vz.wad` found.** If no source (`--game-dir`, `MERCS2_GAME_DIR`, `VZ_WAD`, the registry key)
  yields an install holding `data/vz.wad`, the tool prints `mercs2_game: no vz.wad found. Point it at
  your install with any of: …` (listing every source) and **exits with code 1** (`resolve_roots`).
  This happens before every mode, `--plan` included.
- **No save found.** In a direct boot with no explicit path and no `.profile` in the SaveGames
  folder, the tool prints `mercs2_game: no .profile save found …` and **exits 1**. (The shell-menu
  path tolerates zero saves — it just reports "0 save(s) available".)
- **Read error on the profile.** `mercs2_game: read <path>: <io error>` then **exit 1**.
- **Parse error on the profile.** `mercs2_game: parse <path>: <error>` then **exit 1** (the header
  could not be decoded). A profile whose header parses but whose Lua `SaveState` does not still boots
  — the banner prints `(SaveSingleton Lua state unavailable - header only)` and the world loads with
  no overlays.
- **Unknown flags are silent.** A misspelled flag (e.g. `--Plan`, `--interior_orbit`) is neither an
  error nor a warning — it is ignored, and if it lacks the `--` prefix it may be mistaken for a
  profile path. There is no usage message.
