# mercs2_game

The Mercenaries 2 game exe: it boots the asset-agnostic engine from the player's real save
(`.profile` → `SaveState` → world), and holds all the Mercenaries-specific config that drives it.

## What it is

A binary crate (`mercs2_game`), not a library. There is **no separate engine binary** — this crate
calls `mercs2_engine`'s public entry points in-process, so `cargo run -p mercs2_game` rebuilds a
fresh engine and opens the window itself.

Default boot is the retail shell flow:

1. enumerate `%USERPROFILE%\Documents\My Games\Mercenaries 2\SaveGames\*.profile` (header-only parse)
   and open on the native main menu / save browser (`menu.rs`);
2. the picked slot's profile is parsed to header + `SaveState` (active contract, mission flow chain,
   the `vz_state_*` world-state overlays to activate, playtime) via `mercs2_formats::save`;
3. the game hands `mercs2_engine::app::run` a `world::Mercs2Game` — the full third-person world:
   terrain, heightmap, player avatar + clips, c3 cells, placements, the PMC HQ interior and its
   furniture, exterior props, lights/FX, watermap, resident audio, hero spawn (14 load phases,
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

Assets come from the installed retail `vz.wad`, resolved through the EA Games registry key
(`mercs2_engine::wad::registry_vz_wad`); with no install the binary exits with that hint.

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

Dev tools (headless unless noted — all parse args in `main.rs`; the *engine* never parses args):

| flag | effect |
| --- | --- |
| `--time-load` | full interior load with no window, timing each of the 14 load stages |
| `--interior-assemble` | assemble the PMC interior from the newest save and report floor/furniture Ys (`MERCS2_FURNDBG=1` adds per-item floor Ys) |
| `--find-mesh <name\|0xHASH>` | search every WAD in the install (vz/English/shell/Loading) for a mesh |
| `--dump-asets` | dump every ASET row in vz.wad as `name_hash type_id primary` |
| `--comps [block]` | COMP-type inventory of a block (default 667, the PMC interior overlay) |
| `--interior-placements [block]` | every `ModelName` placement in a block: hash, pos, quat, yaw, offset from spawn |
| `--hall-hunt` | scan vz.wad models for the room-sized mesh whose local bbox encloses the player-enter hardpoint (IDs the interior hall shell) |
| `--tex-audit <0xMODEL>` | per-texture ASET rows for a model, per-block dims/body size, and whether a block carries the FULL mip0 or only the resident tail |
| `--tex-locate <0xHASH>` | scan every block's entry table for a texture chunk (finds streaming copies that aren't ASET-indexed) |
| `--coll-probe` | build the hall collision soup and walk the player in each cardinal direction, reporting distance travelled before sticking |
| `--c3-flat` | scan c3 models for flat, floor-sized meshes |

Ignored probe tests (need the real install):

```sh
cargo test -p mercs2_game --test hardpoint_probe    -- --ignored --nocapture
cargo test -p mercs2_game --test spawn_marker_probe -- --ignored --nocapture
cargo test -p mercs2_game --test audio_wad_probe    -- --ignored --nocapture
```

## Modules

Internal (binary crate — no public API surface):

* `world` — the render/boot path: `Mercs2Game` (the engine's `Game` impl), `WorldData`, the 14-phase
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
  (`pmc::load_pmc_interior`) lives in this crate. (`main.rs`'s `populate_pmc_interior` doc comment
  still describes both as engine-side — the `load_pmc_interior` half has since moved here.)
* Exterior props are bounded: only meshes within 400 m of the spawn, capped at 200 distinct meshes.
