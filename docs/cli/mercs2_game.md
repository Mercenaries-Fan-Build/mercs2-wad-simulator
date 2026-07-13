# mercs2_game

`mercs2_game` **is** the Mercenaries 2 game exe: it configures the asset-agnostic `mercs2_engine`
from the player's real `.profile` save and boots the full third-person world in-process (there is no
separate engine binary). It also carries a set of headless dev tools that inspect `vz.wad` and the
interior/collision/texture pipelines without opening a window.

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

mercs2_game --time-load                      # dev: headless timed interior load
mercs2_game --interior-assemble              # dev: headless interior assembly + floor Ys
mercs2_game --find-mesh <name|0xHASH>        # dev: locate a mesh across every WAD
mercs2_game --dump-asets                     # dev: dump every ASET row in vz.wad
mercs2_game --comps [BLOCK]                  # dev: COMP inventory of a block
mercs2_game --interior-placements [BLOCK]    # dev: ModelName placements of a block
mercs2_game --hall-hunt                      # dev: find the interior hall shell mesh
mercs2_game --tex-audit <0xMODEL>            # dev: per-texture streaming audit for a model
mercs2_game --tex-locate <0xHASH>            # dev: find every block carrying a texture chunk
mercs2_game --coll-probe                     # dev: walk the player through the hall collision soup
mercs2_game --c3-flat                        # dev: scan c3 models for flat floor-sized meshes
```

When run from source: `cargo run -p mercs2_game -- <args>`.

Every boot mode requires the game to be installed so the EA Games registry key resolves to a folder
containing `data\vz.wad` (see [Failure modes](#failure-modes)).

## Options

`mercs2_game` takes at most one positional argument and a handful of `--` flags. No flag takes an
`=value` form; value-carrying flags read the **next** argument. No flag is repeatable in any
meaningful way (each is tested with `args.iter().any(...)` or `.position(...)`, so a second
occurrence is ignored). None is strictly required — with no arguments the tool opens the retail
shell menu.

### Positional

| Argument | Type | Default | Required | What it does |
| --- | --- | --- | --- | --- |
| `<SAVE.profile>` | path | newest `.profile` in the save folder | no | The first argument that does **not** start with `--` (after argv[0]) is taken as an explicit save path. Its presence suppresses the shell menu and boots that save directly. If omitted (and not in menu mode), the newest `.profile` in `%USERPROFILE%\Documents\My Games\Mercenaries 2\SaveGames` is used. |

### Boot-mode flags

| Flag | Value | Default | What it does |
| --- | --- | --- | --- |
| *(none)* | — | — | **Retail flow.** Scans `SaveGames\*.profile` (header-only) and opens the main menu (`menu::Menu`); the player picks Continue / New Game / Load Game / Quit and the chosen save drives the world boot in-loop. New Game uses Mattias, upgrade tier 0, default skin. Only reached when there is no explicit profile, no `--plan`, and no `--stream`. |
| `--plan` | — | off | Resolve the profile (explicit or newest), parse it to header + `SaveState`, print the boot-state banner (profile name, active contract, playtime/cash/fuel, flow chain, active missions, `vz_state` overlay layers, PMC-interior spawn), then **return without rendering**. No window opens. |
| `--stream` | — | off | Boot the alternate **free-fly streaming world** (`mercs2_engine::game_world::run_game_world`) at the PMC-interior spawn with the save's overlay layers, populating the PMC interior via `populate_pmc_interior`. Suppresses the shell menu. |
| `--interior-orbit` | — | off | Adds the engine's **debug orbit camera** to the interior. Read only in the shell-menu boot and the default full-world boot; **ignored under `--stream`** (that branch never consults it). |
| `--interior` | — | off | **Skips loading the terrain model** during the world boot (checked in `world.rs`, not `main.rs`). The terrain sits above the SE terrain peak and would occlude the interior room, so this hides it. Active in the default full-world boot and the shell-menu boot. |
| `--markers` | — | off | Spawns the placement-marker **debug glyphs** for the loaded block (checked in `world.rs`). Only has an effect when the boot has placement data (`data.placements`); otherwise silently does nothing. |

### Dev-tool flags (headless, no window)

Each of these short-circuits `main` — it runs, prints to stdout, and returns before any boot logic.
They are checked in the source order listed here; if you pass two, the **first in this list wins**.

| Flag | Value | What it does |
| --- | --- | --- |
| `--time-load` | — | Runs the full interior `load_world_data` (all core systems on) headless and prints the total wall-clock time; the loader itself prints `stage k/n` timings for the 14 `LOAD_STAGES`. A repeatable benchmark for the load path. |
| `--interior-assemble` | — | Assembles the PMC interior from the **newest** save (`load_pmc_interior`) and reports floor/furniture Y extents. No window. |
| `--find-mesh` | `<name\|0xHASH>` (next arg) | Resolves the arg to a hash (`0x…` parsed as hex, else `pandemic_hash_m2` of the name with a leading `_` stripped) and searches **every `.wad`** beside `vz.wad` (vz/English/shell/Loading) for that mesh, reporting per-WAD model count, presence, ASET types, and whether a mesh / typed-model builds. Missing next arg hashes the empty string. |
| `--dump-asets` | — | Dumps every ASET row in `vz.wad` as `name_hash type_id primary(1/0)`, one per line — for reverse-name hunts over non-primary assets. |
| `--comps` | `[BLOCK]` (next arg, default `667`) | Prints the COMP-type inventory of one WAD block (which components each placement carries). Default block 667 = the PMC interior overlay. |
| `--interior-placements` | `[BLOCK]` (next arg, default `667`) | Lists every `ModelName` placement in a block — hash, pos, quat-derived yaw, and offset from the interior spawn — sorted by X. Default block 667. |
| `--hall-hunt` | — | Scans every `vz.wad` model for a room-sized hollow mesh whose local bbox encloses the player-enter hardpoint, to positively ID the PMC interior hall shell. Prints up to 40 candidates, enclosing ones first. |
| `--tex-audit` | `<0xMODEL>` (next arg) | For a model hash, builds it, collects its distinct diffuse textures, and for each shows every ASET texture-row (block + primary), per-block dims/format/body size, whether that block holds the FULL mip0 or only the streamed resident tail, and the hi-res assembly across the cell subtree. Same hash-parsing rule as `--find-mesh`. |
| `--tex-locate` | `<0xHASH>` (next arg) | Scans **every block's entry table** (not just the ASET table) for a texture chunk (`type_hash 0xF011157A`) with that name hash, to find streaming copies that are not ASET-indexed. Reports `(block, chunk_size, path)` big-first and dumps the leading chunk bytes of each hit. Same hash-parsing rule as `--find-mesh`. |
| `--coll-probe` | — | Builds the hall collision soup (mesh `0x39AF17DC`, state `0x01`, at actor origin `(3750,450,-3840)`), then walks the player character in each cardinal direction from the spawn, reporting metres travelled before it sticks (`physics::soup::move_character`). |
| `--c3-flat` | — | Scans c3 models for flat, floor-sized meshes (PMC-floor candidates) via `mercs2_engine::diag::c3_flat_report`. |

### Environment variables

These are read inside `load_pmc_interior`, so they affect **both** `--interior-assemble` **and** any
real boot that assembles the PMC interior (default full-world and `--stream`). They are diagnostic
toggles — presence of the variable (any value) enables them.

| Variable | Effect |
| --- | --- |
| `MERCS2_FURNDBG` | Prints a `[furn]` per-item floor-Y line for each furniture placement (`pos.y + mesh bmin.y`). |
| `MERCS2_ALLNAMES` | Prints a `[name]` line for every placement name in each interior state block. |
| `MERCS2_ALLBLOCKS` | Overrides recruit-driven block selection and loads **every** interior variant block `[667, 711, 461, 703, 291]` — shows every recruit's bay regardless of the save. |
| `USERPROFILE` | Standard Windows var; used to locate the `SaveGames` folder. If unset, no save folder is found. |

## How the options combine

The single most important thing to understand is the **decision order in `main`**, because it is
strictly first-match-wins and several flags shadow each other:

1. **Dev-tool flags are checked first, in the fixed order** listed in the table above
   (`--time-load` → `--interior-assemble` → `--find-mesh` → `--dump-asets` → `--comps` →
   `--interior-placements` → `--hall-hunt` → `--tex-audit` → `--tex-locate` → `--coll-probe` →
   `--c3-flat`). The first one present runs and the process returns. **All boot flags and the
   positional profile are ignored** when any dev tool is present. Passing two dev tools runs only the
   earlier one.

2. **If no dev tool matched**, three things are computed:
   - `plan_only` = `--plan` present;
   - `explicit` = the first non-`--` argument = the positional profile path (or none);
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
- **Dev-tool value args do not collide with the positional profile.** `--find-mesh`, `--tex-audit`,
  `--tex-locate`, `--comps`, and `--interior-placements` read their value as the *next* arg, but
  those tools return before the positional-profile scan runs, so e.g. `mercs2_game --comps 667` never
  treats `667` as a save path.

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
# Profile the load path: prints per-stage timings for the 14 load stages plus a total.
mercs2_game --time-load
```

```sh
# Find where a mesh lives across every WAD in the install (by name or hash).
mercs2_game --find-mesh vehicle_tank_leopard
mercs2_game --find-mesh 0x39AF17DC
```

```sh
# Audit texture streaming for a model: which blocks hold full mip0 vs only the resident tail.
mercs2_game --tex-audit 0x39AF17DC
```

```sh
# List the authored furniture layout of the PMC interior overlay (block 667), with per-item floor Ys.
MERCS2_FURNDBG=1 mercs2_game --interior-assemble
```

```sh
# Diagnose why the player sticks in the interior: walk the collision soup in all 4 directions.
mercs2_game --coll-probe
```

## Failure modes

- **No `vz.wad` found.** Every rendering/boot mode and most dev tools resolve the install via the EA
  Games registry key. If it does not point at a folder containing `data\vz.wad`, the tool prints
  `mercs2_game: no vz.wad found — install Mercenaries 2 …` and **exits with code 1**
  (`require_vz_wad`). Some dev tools (`--interior-assemble`, `--comps`, `--interior-placements`,
  `--hall-hunt`, `--tex-audit`, `--tex-locate`, `--coll-probe`, `--c3-flat`) instead resolve `vz.wad`
  best-effort and simply **do nothing / print a "no vz.wad" line** rather than exiting.
- **No save found.** In a direct boot with no explicit path and no `.profile` in the SaveGames
  folder, the tool prints `mercs2_game: no .profile save found …` and **exits 1**. (The shell-menu
  path tolerates zero saves — it just reports "0 save(s) available".)
- **Read error on the profile.** `mercs2_game: read <path>: <io error>` then **exit 1**.
- **Parse error on the profile.** `mercs2_game: parse <path>: <error>` then **exit 1** (the header
  could not be decoded). A profile whose header parses but whose Lua `SaveState` does not still boots
  — the banner prints `(SaveSingleton Lua state unavailable - header only)` and the world loads with
  no overlays.
- **Missing dev-tool value.** `--find-mesh` / `--tex-audit` / `--tex-locate` with no following arg
  hash the empty string (a benign but useless lookup). `--comps` / `--interior-placements` with a
  non-numeric or missing next arg fall back to block **667**.
- **`--interior-assemble` reads the newest save**, not any explicit path; if that save fails to parse
  it prints `[save] parse FAILED` / `[save] save_state FAILED` and proceeds with default
  recruit/stockpile state.
- **Unknown flags are silent.** A misspelled flag (e.g. `--Plan`, `--interior_orbit`) is neither an
  error nor a warning — it is ignored, and if it lacks the `--` prefix it may be mistaken for a
  profile path. There is no usage message.
