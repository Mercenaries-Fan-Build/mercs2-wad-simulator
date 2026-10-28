# mercs2_game

The Mercenaries 2 game exe: it boots the asset-agnostic engine from the player's real save
(`.profile` → `SaveState` → world), and holds all the Mercenaries-specific config that drives it.

## What it is

A binary crate (`mercs2_game`), not a library. There is **no separate engine binary** — this crate
calls `mercs2_engine`'s public entry points in-process, so `cargo run -p mercs2_game` rebuilds a
fresh engine and opens the window itself.

Default boot is the retail shell flow:

1. enumerate `*.profile` in the saves folder (`--saves-dir`, else `MERCS2_SAVES_DIR`, else
   `Documents/My Games/Mercenaries 2/SaveGames` under `$USERPROFILE`/`$HOME`, else
   `<install>/SaveGames`; header-only parse) and open on the native main menu / save browser
   (`menu.rs`);
2. the picked slot's profile is parsed to header + `SaveState` (active contract, mission flow chain,
   the `vz_state_*` world-state overlays to activate, playtime) via `mercs2_formats::save`;
3. the game hands `mercs2_engine::app::run` a `world::Mercs2Game` — the full third-person world:
   terrain, heightmap, player avatar + clips, c3 cells, placements, the PMC HQ interior and its
   furniture, exterior props, lights/FX, watermap, resident audio, hero spawn (19 load phases,
   `LOAD_PHASES` in `world.rs` is the loading bar's single source of truth).

The spawn is the authentic game-start **PMC interior**, `(3794.0427, 450.7505, -3911.0322)`
(`MrxUtil._TeleportHero`), which `pmc::derive_interior_spawn` can also re-derive from data: the
`HqInterior` actor position `{3750, 450, -3840}` (`mrxhq.lua:657 SpawnActor`) plus the
`hp_playerA_enter` hardpoint node in the hall mesh's HIER.

Everything Mercenaries-specific lives here — hero identity, the PMC interior layout, the shell menu,
the boot/render path — while the engine stays asset-agnostic. The game reaches every mechanism
(physics/combat/ai/anim/audio/vehicle/decal/population/faction/water/ui + the Lua script host)
through `mercs2_engine::…`, which owns and re-exports them.

## Where it comes from

* **Save format** — `mercs2_formats::save`: profile header (hero byte `@0x4D`, upgrade tier `@0x4F`,
  timestamp `@0x24`, cash, fuel, playtime) + the zlib Lua `SaveSingleton` payload.
* **Hero identity** (`hero.rs`) — the retail spawn path `mrxplayer.lua:155-179`
  (`GetTemplateAndModelName`): spawn template = `_tCharacterMap.templates[iUpgrade]`, wardrobe
  costume (`_tOutfits`) overrides the template model via `_tCharacterMap.models[iCostume]`. Hero byte
  values 1 mattias / 2 chris / 3 jen are engine-coded (`FUN_00634810` → `SHELL.SelectCharacter.*`);
  the engine-object offsets are `+0x61` (character) / `+0x62` (upgrade) / `+0x63` (costume).
* **Shell menu** (`menu.rs`) — the retail Lua shell state machine driving `shell.gfx`
  (`ChangeShellState("newGame"|…)`), reimplemented natively with the same option identity set
  (`autoContinue` / `newGame` / load / `quitGame`); save enumeration mirrors the retail profile
  manager (`getListProfiles` / `addSaveGame`). Docs: `docs/ui/main_menu_structure.md`,
  `docs/ui/shell_menu_lua_anatomy.md`.
* **PMC interior** (`pmc.rs`) — actor origin from `mrxhq.lua`; the renderable interior is placed
  instances in the `vz_state` overlay blocks 667/711/461/703 (the `pmc_interior_P000_Q3.block` = 3490
  asset block is FaceFX/Scaleform only, no geometry).
* **Resident audio** (`world.rs`) — the always-resident bank list is `MrxSoundBootstrap.LoadBanks`
  (`ui_hud`, `ui_shell`, `wpn_shared`, `veh_shared`, `veh_support`, `ambience`, `amb_birds`,
  `amb_shared`, `collision_shared`, `destruction_shared`, `fol_shared`, `music`), loaded as a
  `wavebank` (PCM) + a `sounddb` (cue→wave routing) under the same asset name.
* **Engine/game split** — `docs/modernization/pangea_engine_alignment.md`.

Assets come from the installed retail `vz.wad`, resolved from `--game-dir` (alias `--wad`), else
`MERCS2_GAME_DIR`, else `VZ_WAD`, else (Windows only) the EA Games registry key
(`mercs2_engine::paths::resolve_game_dir`); with no install the binary exits 1 with a hint listing
those sources.

## Usage

```sh
# Retail boot: main menu → save browser → world.
cargo run -p mercs2_game

# Boot a specific save directly (no menu).
cargo run -p mercs2_game -- "C:\Users\<you>\Documents\My Games\Mercenaries 2\SaveGames\auto_634304EA.profile"

# Print the boot-state (profile, contract, flow chain, active missions, vz_state overlays, spawn)
# without opening a window.
cargo run -p mercs2_game -- --plan
```

Other boot modes:

| flag | effect |
| --- | --- |
| `--stream` | alternate free-fly streaming world (`mercs2_engine::game_world::run_game_world`) at the PMC spawn, with the save's overlays |
| `--interior-orbit` | adds the debug orbit camera to the interior |
| `--game-dir <path>` (alias `--wad`) | the install root, its `data` folder, or `vz.wad` itself |
| `--saves-dir <path>` (alias `--saves`) | the folder `*.profile` saves are listed from |

Full flag reference: `docs/cli/mercs2_game.md`.

## Modules

Internal (binary crate — no public API surface):

* `world` — the render/boot path: `Mercs2Game` (the engine's `Game` impl), `WorldData`, the 19-phase
  `load_world_data`, TPS/free camera toggle, player controller wiring, collision, audio hand-off.
* `pmc` — PMC HQ interior: spawn/actor constants, `derive_interior_spawn`, `load_pmc_interior`,
  `RecruitUnlocks`, `Stockpile`.
* `hero` — the three playable heroes: `HEROES`, `hero()`, `look_label()`, `player_model_candidates()`.
* `menu` — shell menu + save browser: `SaveSlot`, `scan_slots`, `Menu`, `MenuAction`, `Nav`.

## Notes / gotchas

* **The save's `vz_state` layers are folded in, but the PMC interior overlay is not.** The engine
  loads the interior ROOM (shells + furniture, by path) as static geometry at the spawn, because the
  room shells don't resolve via the streaming name-hash overlay recipe. Adding `vz_state_pmcinterior`
  to the layer list would double-load the furniture — hence `INTERIOR_OVERLAYS` is empty.
* **One World.** The ECS `World` is owned by the engine (`app::run`) and lent to the game via `Ctx`.
  The game must not keep its own — models spawned into a second World never render.
* **Hero look follows the upgrade tier**, not a costume: tier 0 = the base model; Mattias tier 3 =
  `pmc_hum_mattias_v3` ("MetalHead"). Tier 1/2 and the Chris/Jen tier models are not yet extracted
  from their templates and fall back to the base model. Every observed save stores costume `0`
  (wardrobe unused), so the costume byte's FILE offset is not located — the engine object offset
  (`+0x63`) is proven, the file byte is not.
* The interior boot still runs `mercs2_engine::script_host::run_interior_boot` (the authentic
  `MrxUtil.SpawnActor` path) through the engine's public API; only the geometry realization
  (`pmc::load_pmc_interior`) lives in this crate.
* Exterior props are bounded: only meshes within 400 m of the spawn, capped at 200 distinct meshes.
