//! `mercs2_game` — the Mercenaries 2 game exe.
//!
//! This is the *game* layer: it configures and boots the asset-agnostic engine from the player's real
//! save. No Mercenaries-specific data lives in the engine; it lives here. The boot:
//! 1. find the newest `.profile` in the player's save folder,
//! 2. parse it → header + `SaveState` (active contract, mission flow, the `vz_state_*` world-state
//!    overlays to activate, playtime — see `mercs2_formats::save`),
//! 3. render the engine's streaming world IN-PROCESS at the authentic **PMC-interior spawn**
//!    (`MrxUtil._TeleportHero` = `(3794, 451, -3911)`, off-map high-Y).
//!
//! There is NO separate engine binary: `mercs2_game` IS the game exe. It calls the engine library's
//! public render entry point (`mercs2_engine::game_world::run_game_world`) directly, so
//! `cargo run -p mercs2_game` always rebuilds a fresh engine and opens the window itself.
//!
//! See `docs/modernization/pangea_engine_alignment.md` for the engine/game split this realizes.
//! Run `mercs2_game` to boot; `mercs2_game --plan` to print the boot-state without rendering.

use std::path::{Path, PathBuf};

use mercs2_formats::save;

mod pmc; // GAME-specific PMC interior assembly (constants + load_pmc_interior)
mod script_host; // GAME-specific Lua interior boot (EngineHost impl + run_interior_boot)
mod world; // GAME render/boot: full TPS/free world render path (player avatar, 10-stage load)

use pmc::PMC_INTERIOR_SPAWN;

/// The engine loads the PMC interior ROOM (shells + furniture, by PATH) as static geometry at the
/// spawn (`mercs2_engine::game_world`), because the room shells don't resolve via the streaming
/// name-hash overlay recipe. So the game does NOT fold `vz_state_pmcinterior` here (that would
/// double-load the furniture). Extra interior overlays (recruit variants) could be added later.
const INTERIOR_OVERLAYS: &[&str] = &[];

/// `%USERPROFILE%\Documents\My Games\Mercenaries 2\SaveGames`, if it exists.
fn save_games_dir() -> Option<PathBuf> {
    let up = std::env::var("USERPROFILE").ok()?;
    let p = Path::new(&up).join("Documents/My Games/Mercenaries 2/SaveGames");
    p.is_dir().then_some(p)
}

/// Recruit-unlock + stockpile state from the newest save (for dev tools without a `SaveState` in hand).
fn newest_save_interior() -> (pmc::RecruitUnlocks, pmc::Stockpile) {
    let prof = save_games_dir()
        .and_then(|d| newest_profile(&d))
        .and_then(|p| std::fs::read(&p).ok())
        .and_then(|b| save::parse(&b).ok());
    let recruits = prof
        .as_ref()
        .and_then(|p| p.save_state().ok())
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

    // GAME dev tool: headless PMC interior assembly + floor/furniture Y check (no window). It is fine
    // for the GAME to parse its own args for dev modes — the ENGINE never does. `MERCS2_FURNDBG=1`
    // adds per-item floor Ys.
    if args.iter().any(|a| a == "--interior-assemble") {
        let (recruits, stockpile) = newest_save_interior();
        match mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            Some(mut w) => {
                let _ = pmc::load_pmc_interior(&mut w, recruits, &stockpile);
            }
            None => eprintln!("--interior-assemble: no vz.wad found"),
        }
        return;
    }

    // GAME dev tool: search EVERY WAD in the install (vz/English/shell/Loading) for a mesh by name or
    // 0xHASH — to locate assets missing from vz.wad. `--find-mesh <name|0xHASH>`.
    if let Some(i) = args.iter().position(|a| a == "--find-mesh") {
        let arg = args.get(i + 1).cloned().unwrap_or_default();
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        println!("[find-mesh] '{arg}' -> 0x{hash:08X}");
        if let Some(vz) = mercs2_engine::wad::registry_vz_wad() {
            if let Some(dir) = Path::new(&vz).parent() {
                for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("wad")) {
                        let Some(ps) = p.to_str() else { continue };
                        match mercs2_engine::wad::open(ps) {
                            Ok(mut w) => {
                                let models = mercs2_engine::wad::model_list(&w);
                                let present = models.iter().any(|(h, _)| *h == hash);
                                let mesh = mercs2_engine::game_world::load_model_by_hash(&mut w, hash)
                                    .map(|(m, _, _)| format!("MESH {}v/{}t", m.verts.len(), m.indices.len() / 3))
                                    .unwrap_or_else(|| "no-mesh".into());
                                println!(
                                    "  {:<14} {} models, hash-present={present}, {mesh}",
                                    p.file_name().unwrap().to_string_lossy(), models.len()
                                );
                            }
                            Err(err) => println!("  {}: open failed: {err}", p.file_name().unwrap().to_string_lossy()),
                        }
                    }
                }
            }
        }
        return;
    }

    // GAME dev tool: dump the COMP-type inventory of a WAD block (default 667, the PMC interior
    // overlay) — to see which components each placement carries (Transform/Name/ModelName/Model/…).
    if let Some(i) = args.iter().position(|a| a == "--comps") {
        let blk: u16 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(667);
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            if let Ok(data) = mercs2_engine::wad::decompress_block_index(&mut w, blk) {
                println!("[comps] block {blk} COMP inventory:");
                for ci in mercs2_formats::placement::comp_inventory(&data) {
                    println!("  {ci:?}");
                }
            }
        }
        return;
    }

    // GAME dev tool: scan c3 models for flat, floor-sized meshes (PMC-floor candidates).
    if args.iter().any(|a| a == "--c3-flat") {
        if let Some(p) = mercs2_engine::wad::registry_vz_wad() {
            if let Err(e) = mercs2_engine::diag::c3_flat_report(&p) {
                eprintln!("--c3-flat: {e}");
            }
        }
        return;
    }

    let plan_only = args.iter().any(|a| a == "--plan");
    // Optional explicit profile path (positional); else newest in the save folder.
    let explicit = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);

    let profile_path = match explicit.or_else(|| save_games_dir().and_then(|d| newest_profile(&d))) {
        Some(p) => p,
        None => {
            eprintln!("mercs2_game: no .profile save found in %USERPROFILE%\\Documents\\My Games\\Mercenaries 2\\SaveGames. Pass a .profile path.");
            std::process::exit(1);
        }
    };

    let bytes = match std::fs::read(&profile_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("mercs2_game: read {}: {e}", profile_path.display());
            std::process::exit(1);
        }
    };
    let profile = match save::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("mercs2_game: parse {}: {e}", profile_path.display());
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
    println!(
        "  spawn     : PMC interior ({:.1}, {:.1}, {:.1})",
        spawn[0], spawn[1], spawn[2]
    );
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
    let wadpath = match mercs2_engine::wad::registry_vz_wad() {
        Some(p) => p,
        None => {
            eprintln!(
                "mercs2_game: no vz.wad found — install Mercenaries 2 so that the EA Games registry \
                 key resolves to a folder containing data\\vz.wad."
            );
            std::process::exit(1);
        }
    };
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
        pollster::block_on(world::run_scene_world_loading(
            wadpath, true, true, true, true, true, orbit, recruits, stockpile,
        ));
    }
}

/// GAME world population: once the engine's streaming world has loaded, spawn the PMC interior into
/// the engine's World/Scene. The interior spawns because the GAME asks for it — the engine has no
/// concept of a "PMC interior". Runs the authentic `MrxUtil.SpawnActor` path (`run_interior_boot`)
/// then realizes the resolved geometry (`load_pmc_interior`) as ECS entities.
///
/// (Phase-1 seam: `load_pmc_interior` / `run_interior_boot` still physically live in `mercs2_engine`
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

    let intents = script_host::run_interior_boot();
    for r in &intents {
        println!(
            "[lua] Pg.Spawn '{}' (name={}) at ({:.1},{:.1},{:.1}) -> guid 0x{:x}",
            r.template, r.name, r.pos[0], r.pos[1], r.pos[2], r.guid
        );
    }
    let want = intents
        .iter()
        .any(|r| r.template.eq_ignore_ascii_case(script_host::PMC_INTERIOR_TEMPLATE));
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
        Err(e) => eprintln!("[game] PMC interior load failed: {e}"),
    }
}
