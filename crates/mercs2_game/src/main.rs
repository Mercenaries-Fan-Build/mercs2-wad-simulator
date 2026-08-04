//! `mercs2_game` — the Mercenaries 2 game exe.
//!
//! This is the *game* layer: it configures and boots the asset-agnostic engine from the player's real
//! save. No Mercenaries-specific data lives in the engine; it lives here.
//!
//! There is NO separate engine binary: `mercs2_game` IS the game exe. It drives the engine library
//! in-process, so `cargo run -p mercs2_game` always rebuilds a fresh engine and opens the window
//! itself.
//!
//! # Boot
//! Default (retail flow) = the SHELL MENU: enumerate `SaveGames\*.profile` (header-only parse,
//! [`menu::scan_slots`]) and open on the main menu; the player picks Continue / New Game / Load Game
//! (save browser) / Quit, and the chosen save drives the world boot in-loop. The boot itself is
//! [`world::Mercs2Game`] handed to `mercs2_engine::app::run` — the full third-person world (player
//! avatar + TPS/free camera, terrain, heightmap, clips, c3 cells, placements, PMC interior +
//! furniture, props, lights/FX, watermap, resident audio, hero spawn; `world::LOAD_PHASES` is the
//! loading bar's single source of truth).
//!
//! A positional `.profile` path boots that save directly with no menu; the save is parsed to header +
//! `SaveState` (active contract, mission flow, the `vz_state_*` world-state overlays to activate,
//! playtime — see `mercs2_formats::save`), printed as a boot banner, and rendered. Either way the
//! hero starts at the authentic **PMC-interior spawn** (`MrxUtil._TeleportHero` =
//! `(3794, 451, -3911)`, off-map high-Y — [`pmc::PMC_INTERIOR_SPAWN`]).
//!
//! `--stream` selects the alternate free-fly streaming world (`mercs2_engine::game_world::
//! run_game_world`) at the same spawn; `--interior-orbit` adds the debug orbit camera; `--plan`
//! prints the boot-state without rendering.
//!
//! # The two roots: install and saves
//! Both are resolvable from the command line; neither requires the Windows registry. See
//! [`mercs2_engine::paths`] for the full order and the reasoning.
//!
//! | Root | Flag | Environment | Last resort |
//! |---|---|---|---|
//! | install (assets) | `--game-dir <path>` (alias `--wad`) | `MERCS2_GAME_DIR`, then `VZ_WAD` | the EA Games registry key (Windows only) |
//! | saves (`*.profile`) | `--saves-dir <path>` (alias `--saves`) | `MERCS2_SAVES_DIR` | `$USERPROFILE`/`$HOME` `Documents/My Games/Mercenaries 2/SaveGames`, then `<install>/SaveGames` |
//!
//! The install flag takes the **install root**, its `data` folder, or `vz.wad` itself. The registry
//! key — `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames\Install Dir`, the only one
//! this project reads — cannot be the sole source: it is Windows-only and absent for a copied-off
//! `data` folder, a Wine/Proton prefix, or an install that never wrote it.
//!
//! # Modules
//! * [`world`] — the render/boot path: `Mercs2Game` (the engine's `Game` impl), `WorldData`, the
//!   staged `load_world_data`, camera/player/collision/audio wiring.
//! * [`pmc`] — PMC HQ interior: spawn + actor constants, `derive_interior_spawn`,
//!   `load_pmc_interior`, `RecruitUnlocks`, `Stockpile`.
//! * [`hero`] — the three playable heroes: templates, upgrade-tier looks, wardrobe outfits.
//! * [`menu`] — the shell menu + save browser (native `ChangeShellState` reimpl).
//!
//! The Lua host + fleet-sim cluster (script_host/runtime/gameplay/spawn) live INSIDE the engine — Lua
//! is a core engine pillar, married to the live engine systems — so the game reaches them (and every
//! other mechanism: physics/combat/ai/anim/audio/vehicle/decal/population/faction/water/ui) through
//! `mercs2_engine::…`.
//!
//! See `docs/modernization/pangea_engine_alignment.md` for the engine/game split this realizes.

use std::path::{Path, PathBuf};

use mercs2_formats::save;

mod bridge_host; // Serve the live-bridge REPL protocol from the reimpl engine
mod hero; // GAME character identity: 3 heroes + wardrobe outfit lists (_tCharacterMap/_tOutfits)
mod menu; // GAME shell menu: main menu + save browser (native ChangeShellState reimpl)
mod pmc; // GAME-specific PMC interior assembly (constants + load_pmc_interior)
mod world; // GAME render/boot: full TPS/free world render path (player avatar, 10-stage load)
// The Lua host + fleet-sim cluster (script_host/runtime/gameplay/spawn) moved INTO the engine — Lua is a
// core engine pillar, married to the live engine systems. The game reaches it via `mercs2_engine::…`.

use pmc::PMC_INTERIOR_SPAWN;

/// The engine loads the PMC interior ROOM (shells + furniture, by PATH) as static geometry at the
/// spawn (`mercs2_engine::game_world`), because the room shells don't resolve via the streaming
/// name-hash overlay recipe. So the game does NOT fold `vz_state_pmcinterior` here (that would
/// double-load the furniture). Extra interior overlays (recruit variants) could be added later.
const INTERIOR_OVERLAYS: &[&str] = &[];

/// The two roots, resolved ONCE from the command line at startup.
///
/// A `OnceLock` rather than threading them through every call: `save_games_dir` is reached from
/// helpers several frames deep that take no arguments, and the roots are startup configuration that
/// cannot change while the process runs. Set by [`resolve_roots`] before anything reads them.
static SAVES_DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

/// Resolve both roots from the CLI and freeze them. Call once, first thing in `main`.
///
/// Returns the install root's `vz.wad`, exiting with a hint if no source yields one — the game cannot
/// run without assets, whereas an absent saves folder is merely "no saves yet".
fn resolve_roots(args: &[String]) -> String {
    let game_dir = mercs2_engine::paths::resolve_game_dir(flag_val(args, "--game-dir", "--wad"));
    let saves = mercs2_engine::paths::resolve_saves_dir(
        flag_val(args, "--saves-dir", "--saves"),
        game_dir.as_deref(),
    );
    match &saves {
        Some(p) => println!("[mercs2_game] saves:  {}", p.display()),
        None => println!(
            "[mercs2_game] saves:  none found — pass --saves-dir <folder> or set MERCS2_SAVES_DIR"
        ),
    }
    let _ = SAVES_DIR.set(saves);

    match game_dir.as_deref().and_then(|d| mercs2_engine::paths::wad_in_game_dir(d, "vz.wad")) {
        Some(p) => {
            println!("[mercs2_game] vz.wad: {}", p.display());
            p.to_string_lossy().into_owned()
        }
        None => {
            println!(
                "mercs2_game: no vz.wad found. Point it at your install with any of:\n  \
                 --game-dir <folder>      the install root, its data folder, or vz.wad itself\n  \
                 MERCS2_GAME_DIR=<folder> same forms, as an environment variable\n  \
                 VZ_WAD=<path>            legacy alias, same forms\n  \
                 (Windows only) HKLM\\SOFTWARE\\WOW6432Node\\EA Games\\Mercenaries 2 World in Flames\\Install Dir\n\n\
                 Saves are separate: --saves-dir <folder> or MERCS2_SAVES_DIR.\n\
                 Example:\n  \
                 mercs2_game --game-dir \"C:\\Program Files (x86)\\EA Games\\Mercenaries 2 World in Flames\""
            );
            std::process::exit(1);
        }
    }
}

/// The value after `name` (or its `alias`), if present and not itself a flag.
fn flag_val<'a>(args: &'a [String], name: &str, alias: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name || a == alias)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .filter(|s| !s.starts_with("--"))
}

/// The resolved saves folder, or `None` if no source produced one. See [`resolve_roots`].
fn save_games_dir() -> Option<PathBuf> {
    SAVES_DIR.get().and_then(|o| o.clone())
}

/// Recruit-unlock + stockpile state from the newest save (for dev tools without a `SaveState` in hand).
fn newest_save_interior() -> (pmc::RecruitUnlocks, pmc::Stockpile) {
    let dir = save_games_dir();
    let path = dir.as_ref().and_then(|d| newest_profile(d));
    println!("[save] SaveGames dir = {dir:?}");
    println!("[save] newest .profile = {path:?}");
    let prof = path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| match save::parse(&b) {
            Ok(p) => Some(p),
            Err(e) => {
                println!("[save] parse FAILED: {e}");
                None
            }
        });
    let ss = prof.as_ref().and_then(|p| match p.save_state() {
        Ok(s) => Some(s),
        Err(e) => {
            println!("[save] save_state FAILED: {e}");
            None
        }
    });
    println!("[save] unlocked_starters = {:?}", ss.as_ref().map(|s| &s.unlocked_starters));
    let recruits = ss
        .map(|s| pmc::RecruitUnlocks::from_starters(&s.unlocked_starters))
        .unwrap_or_default();
    let stockpile = pmc::Stockpile {
        cash: prof.as_ref().map(|p| p.cash as i64).unwrap_or(0),
        ..Default::default()
    };
    (recruits, stockpile)
}

/// The most-recently-modified `.profile` in `dir` (the game's autosave/continue slot).
fn newest_profile(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("profile"))
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Resolve the install + saves roots FIRST: everything below reads them, and a bad install path
    // should fail with a path hint rather than somewhere deeper with a confusing one.
    let wadpath = resolve_roots(&args);

    let plan_only = args.iter().any(|a| a == "--plan");
    // Optional explicit profile path (positional); else newest in the save folder. Skip the values of
    // the value-taking flags — see [`VALUE_FLAGS`].
    let value_idxs: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| VALUE_FLAGS.contains(&a.as_str()))
        .map(|(i, _)| i + 1)
        .collect();
    let explicit = args
        .iter()
        .enumerate()
        .skip(1)
        .find(|(i, a)| !a.starts_with("--") && !value_idxs.contains(i))
        .map(|(_, a)| PathBuf::from(a));

    // ── Default boot = the SHELL MENU (retail flow) ──────────────────────────
    // No explicit .profile and no dev mode: open on the main menu; the player picks
    // Continue / New Game / Load Game (save browser) / Quit and the selected save drives the
    // world boot in-loop. An explicit .profile arg, `--plan` and `--stream` keep the direct
    // no-menu boots (dev workflows).
    if explicit.is_none() && !plan_only && !args.iter().any(|a| a == "--stream") {
        let slots = menu::scan_slots(save_games_dir());
        println!("[shell] main menu: {} save(s) available", slots.len());
        let wadpath = wadpath.clone();
        let orbit = args.iter().any(|a| a == "--interior-orbit");
        pollster::block_on(mercs2_engine::app::run(world::Mercs2Game::new(
            wadpath,
            true,
            true,
            true,
            true,
            true,
            orbit,
            pmc::RecruitUnlocks::default(),
            pmc::Stockpile::default(),
            hero::player_model_candidates(1, 0, 0), // retail new game: Mattias, tier 0, default skin
            Some(menu::Menu::new(slots)),
        )));
        return;
    }

    let profile_path = match explicit.or_else(|| save_games_dir().and_then(|d| newest_profile(&d))) {
        Some(p) => p,
        None => {
            println!(
                "mercs2_game: no .profile save found{}.\n  \
                 Point at your saves with --saves-dir <folder> or MERCS2_SAVES_DIR,\n  \
                 or pass a .profile path directly as a positional argument.",
                match save_games_dir() {
                    Some(d) => format!(" in {}", d.display()),
                    None => " (no saves folder resolved)".into(),
                }
            );
            std::process::exit(1);
        }
    };

    let bytes = match std::fs::read(&profile_path) {
        Ok(b) => b,
        Err(e) => {
            println!("mercs2_game: read {}: {e}", profile_path.display());
            std::process::exit(1);
        }
    };
    let profile = match save::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            println!("mercs2_game: parse {}: {e}", profile_path.display());
            std::process::exit(1);
        }
    };
    let state = profile.save_state().ok();
    let spawn = PMC_INTERIOR_SPAWN; // authentic game-start = PMC interior

    // ── Boot banner (the game's start-state, from the save) ──────────────────
    let line = "=".repeat(66);
    println!("{line}");
    println!("  MERCENARIES 2 - booting from save");
    println!("  profile   : {}", profile.save_name());
    println!(
        "  file      : {}",
        profile_path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
    );
    println!("  contract  : {}", profile.active_contract());
    println!(
        "  playtime  : {}s   cash: {}   fuel: {}",
        profile.play_time_seconds, profile.cash, profile.fuel
    );
    if let Some(s) = &state {
        if !s.flow_chain.is_empty() {
            println!("  flow      : {}", s.flow_chain.join(" -> "));
        }
        println!(
            "  missions  : {} active ({})",
            s.active_missions.len(),
            s.active_missions
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "  overlays  : {} vz_state world-state layers to activate",
            s.layers.len()
        );
        for l in s.layers.iter().take(4) {
            println!("                {l}");
        }
        if s.layers.len() > 4 {
            println!("                ... +{} more", s.layers.len() - 4);
        }
    } else {
        println!("  (SaveSingleton Lua state unavailable - header only)");
    }
    // Not a coordinate: booting from a save takes the master script's RESUME branch, which starts the
    // hero at the `Pmc_Entry1` MARKER (`vz/xQ!L.lua:661`) resolved against the live world. The old
    // banner printed `PMC_INTERIOR_SPAWN` here, which stopped being the answer once the boot Lua flow
    // became authoritative — `spawn` below only seeds the `--stream` free-fly camera.
    println!("  spawn     : resume branch -> marker Pmc_Entry1 (PMC HQ entrance)");
    println!("{line}");

    if plan_only {
        println!("[mercs2_game] --plan: boot-state only; not rendering the world.");
        return;
    }

    // ── Render the engine's streaming world IN-PROCESS at the spawn ──────────
    // No separate engine binary: mercs2_game calls the engine library's public render entry point
    // directly. The active vz_state overlays are the save's world-state layers PLUS the PMC interior
    // overlay (the game-start spawn IS the interior); the engine resolves each -> its WAD block and
    // folds it into the streaming manager.
    let mut layers: Vec<String> = state.as_ref().map(|s| s.layers.clone()).unwrap_or_default();
    for l in INTERIOR_OVERLAYS {
        if !layers.iter().any(|x| x == *l) {
            layers.push((*l).to_string());
        }
    }

    // The engine consumes the original game's assets from vz.wad (EA Games registry install dir).
    let wadpath = wadpath.clone();
    // Default boot = the FULL game world: player + third-person camera + PMC interior + c3 cells +
    // placements + props — all core components ON. `--stream` selects the alternate streaming free-fly
    // world; `--interior-orbit` adds the debug orbit camera.
    // The save drives the interior: which recruits are unlocked (state layers + bays) and how much
    // cash/supplies the player has (stockpile piles).
    let recruits = state
        .as_ref()
        .map(|s| pmc::RecruitUnlocks::from_starters(&s.unlocked_starters))
        .unwrap_or_default();
    let stockpile = pmc::Stockpile { cash: profile.cash as i64, ..Default::default() };
    if args.iter().any(|a| a == "--stream") {
        println!(
            "[mercs2_game] streaming world (free-fly): spawn=({:.1},{:.1},{:.1}) overlays={}",
            spawn[0], spawn[1], spawn[2], layers.len()
        );
        let sp = stockpile.clone();
        pollster::block_on(mercs2_engine::game_world::run_game_world(
            wadpath,
            Some(PMC_INTERIOR_SPAWN),
            layers,
            move |world, scene, wad| populate_pmc_interior(world, scene, wad, recruits, &sp),
        ));
    } else {
        println!("[mercs2_game] full world: TPS + PMC interior + c3 cells + placements + props");
        let orbit = args.iter().any(|a| a == "--interior-orbit");
        // Direct boot: the player model comes from THIS profile's hero + upgrade tier, like a
        // menu boot (costume file byte not yet located; 0 = wardrobe unused in all known saves).
        let hero_idx = profile.character_index; // header @0x4D, 1-based
        let models = hero::player_model_candidates(hero_idx, profile.upgrade_index, 0);
        println!(
            "[mercs2_game] character: {} [{}]",
            hero::hero(hero_idx).display,
            hero::look_label(hero_idx, profile.upgrade_index, 0)
        );
        let mut game = world::Mercs2Game::new(
            wadpath, true, true, true, true, true, orbit, recruits, stockpile, models, None,
        );
        // Resolve the boot the SAME way a menu pick does, so the direct path also hands the save to the
        // script host. Without this the master script would see no save and take its new-game branch —
        // booting the opening contract instead of resuming the profile that was just parsed.
        game.apply_boot(Some(profile_path.clone()));
        pollster::block_on(mercs2_engine::app::run(game));
    }
}

/// The flags that take a VALUE. Their values must not be mistaken for the positional `.profile` path
/// — without this, `mercs2_game --game-dir <install>` would try to load the install folder as a save.
const VALUE_FLAGS: &[&str] = &["--game-dir", "--wad", "--saves-dir", "--saves"];

/// GAME world population: once the engine's streaming world has loaded, spawn the PMC interior into
/// the engine's World/Scene. The interior spawns because the GAME asks for it — the engine has no
/// concept of a "PMC interior". Runs the authentic `MrxUtil.SpawnActor` path (`run_interior_boot`)
/// then realizes the resolved geometry (`load_pmc_interior`) as ECS entities.
///
/// (Seam: `load_pmc_interior` / `run_interior_boot` still physically live in `mercs2_engine`
/// and are called here through its public API; a follow-up moves those bodies into this crate so the
/// engine holds none of it.)
fn populate_pmc_interior(
    world: &mut mercs2_core::World,
    scene: &mut mercs2_engine::scene::Scene,
    wad: &mut mercs2_engine::wad::Wad,
    recruits: pmc::RecruitUnlocks,
    stockpile: &pmc::Stockpile,
) {
    use mercs2_core::glam::{Quat, Vec3};
    use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let intents = mercs2_engine::script_host::run_interior_boot();
    for r in &intents {
        println!(
            "[lua] Pg.Spawn '{}' (name={}) at ({:.1},{:.1},{:.1}) -> guid 0x{:x}",
            r.template, r.name, r.pos[0], r.pos[1], r.pos[2], r.guid
        );
    }
    let want = intents
        .iter()
        .any(|r| r.template.eq_ignore_ascii_case(mercs2_engine::script_host::PMC_INTERIOR_TEMPLATE));
    if !want {
        return;
    }
    match pmc::load_pmc_interior(wad, recruits, stockpile) {
        Ok(pieces) => {
            let n = pieces.len();
            for (m, pos, quat) in pieces {
                if !scene.has_model(m.hash) {
                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                }
                let nbones = scene.model_bone_count(m.hash).max(1);
                world.spawn((
                    Transform {
                        translation: Vec3::new(pos[0], pos[1], pos[2]),
                        rotation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]),
                        scale: Vec3::ONE,
                    },
                    ModelRef { model: m.hash },
                    AnimState::default(),
                    SkinPalette { mats: vec![IDENTITY; nbones] },
                ));
            }
            println!("[game] PMC interior: {n} pieces placed");
        }
        Err(e) => println!("[game] PMC interior load failed: {e}"),
    }
}
