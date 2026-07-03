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

/// Authentic game-start spawn: the PMC interior (off-map, high-Y; the game's `MrxUtil._TeleportHero`
/// drops the hero here on a new/continued game). This is GAME data — it belongs in the game layer.
const PMC_INTERIOR_SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

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
    println!(
        "[mercs2_game] rendering in-process: vz.wad={wadpath}  spawn=({:.1},{:.1},{:.1})  overlays={}",
        spawn[0], spawn[1], spawn[2], layers.len()
    );
    pollster::block_on(mercs2_engine::game_world::run_game_world(
        wadpath,
        Some(PMC_INTERIOR_SPAWN),
        layers,
    ));
}
