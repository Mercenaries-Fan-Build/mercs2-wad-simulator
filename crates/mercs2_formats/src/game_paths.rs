//! Finding the game's archives from the environment — the one implementation every crate shares.
//!
//! This lives in `mercs2_formats` because it is the lowest crate the tools, the leaf crates
//! (`mercs2_anim`) and `mercs2_engine` all already depend on. The richer resolver in
//! `mercs2_engine::paths` — which also handles CLI flags, the Windows registry key, and the saves
//! folder — delegates its environment scan here rather than reimplementing it.
//!
//! The rule is: **no hardcoded install paths**. Several call sites previously fell back to a literal
//! `C:/Program Files (x86)/EA Games/...`, which could not resolve off Windows, so the live tests
//! guarded by it silently never ran there and reported green.

use std::path::{Path, PathBuf};

/// The environment variables naming the install, in precedence order.
///
/// `MERCS2_GAME_DIR` is the current name; `VZ_WAD` predates it and is still accepted so existing
/// shells and scripts keep working.
pub const GAME_DIR_VARS: [&str; 2] = ["MERCS2_GAME_DIR", "VZ_WAD"];

/// Interpret one user-supplied path as the install root, its `data` folder, or the archive itself.
///
/// A file is taken as-is (so a renamed or copied archive works); a folder is probed at `data/<name>`
/// then `<name>`. `None` when nothing exists there, so a caller falls through instead of being
/// handed an unopenable path.
///
/// Exposed separately because this rule is what "a path to the game" MEANS, and every resolver needs
/// it — `mercs2_quartermaster` had it copied verbatim, which is precisely the drift the rest of this
/// module exists to prevent.
pub fn wad_under(path: &Path, name: &str) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    [path.join("data").join(name), path.join(name)]
        .into_iter()
        .find(|c| c.is_file())
}

/// Resolve a WAD by filename from whichever of [`GAME_DIR_VARS`] is set.
///
/// Each variable may hold the install root, its `data` folder, or a WAD file directly — see
/// [`wad_under`]. Returns `None` when nothing is set or nothing exists; callers skip rather than fail.
pub fn wad_from_env(name: &str) -> Option<PathBuf> {
    GAME_DIR_VARS
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .find_map(|p| wad_under(&p, name))
}

/// [`wad_from_env`] for the base archive, `vz.wad`.
pub fn vz_wad_from_env() -> Option<PathBuf> {
    wad_from_env("vz.wad")
}

/// Machine-local, git-ignored config naming the install, written by `scripts/find-vz-wad.sh`.
///
/// A **dev-checkout fallback only**, and deliberately lower priority than the environment: Modkit
/// manages the install and hands paths to the tools it launches, so a per-tool config competing with
/// that is how a fleet of tools ends up disagreeing about where the game is. It exists so tests run
/// on a checkout without ceremony.
pub const LOCAL_CONFIG: &str = ".mercs2-local.toml";

/// `vz_wad = "…"` from the nearest [`LOCAL_CONFIG`], searching upward from `start`.
pub fn wad_from_local_config(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join(LOCAL_CONFIG)) {
            if let Some(raw) = text
                .lines()
                .find(|l| l.trim_start().starts_with("vz_wad"))
                .and_then(|l| l.split('=').nth(1))
            {
                let p = PathBuf::from(raw.trim().trim_matches('"'));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        dir = d.parent();
    }
    None
}

/// The base archive: environment first, then the dev-checkout config.
///
/// This is the resolution a **test** wants — enough to run on a developer's machine without setup,
/// and `None` on CI so it skips rather than fails. Hosts that also need co-location and the registry
/// use `mercs2_quartermaster::game::discover`, which layers those on top and reports where the
/// answer came from.
pub fn vz_wad(start: &Path) -> Option<PathBuf> {
    vz_wad_from_env().or_else(|| wad_from_local_config(start))
}

/// The environment variable naming the saves folder.
pub const SAVES_DIR_VAR: &str = "MERCS2_SAVES_DIR";

/// The saves folder's location under a user's home directory. Retail writes
/// `Documents\My Games\Mercenaries 2\SaveGames`; a Wine/Proton prefix reproduces it verbatim, and a
/// hand-copied save set almost always lands at the same relative spot.
pub const SAVES_UNDER_HOME: &str = "Documents/My Games/Mercenaries 2/SaveGames";

/// The **vendored retail saves**, in-tree at `<crate>/fixtures/saves`.
///
/// These are the eight real `.profile` files the save reader and writer are reversed against, committed
/// so every assertion about them runs everywhere. They used to be read from one developer's
/// `C:/Users/Shadow/Documents/...`: the read-side tests PANICKED on any other machine, and the
/// write-side tests skipped silently and reported green while asserting nothing. A test that can only
/// pass on one computer is not a test.
///
/// Derived from `CARGO_MANIFEST_DIR` — **never a hardcoded path** — so it resolves from any checkout
/// location, under any CI layout, and from the extracted `.crate` when consumed from a registry. Same
/// discipline as the vendored Lua corpus in `mercs2_script`.
///
/// At 13,404 bytes each the whole set is 128 KiB, which is why vendoring is viable here and is not for
/// `vz.wad` (2.5 GiB).
pub fn save_fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("saves")
}

/// Every vendored save, by filename — the single authoritative list.
///
/// Both `save.rs` and `save_write.rs` iterate this. They previously kept private copies, which is how
/// the write-side list silently stayed at six after the set grew; a duplicated fixture list is a list
/// that drifts. `save::tests::the_fixture_set_is_complete_and_fully_covered` asserts this matches the
/// directory exactly in both directions, so a file cannot be added-but-unexercised or deleted-but-listed.
///
/// The set is deliberately varied — all three heroes, upgrade tiers 0 and 3, flow chains from 2 to 63
/// entries, and one non-ASCII slot name. See `fixtures/saves/README.md`.
pub const SAVE_FIXTURES: [&str; 8] = [
    "Mattias Nilsson_63430745.profile",
    "Mattias Nilsson_6A0E523C.profile",
    "Chris Jacobs_6A499ED6.profile",
    "_______ ________48EFABFB.profile",
    "auto_634304EA.profile",
    "auto_6A0BE454.profile",
    "auto_6A447BF8.profile",
    "auto_6A499D08.profile",
];

/// Resolve the folder holding a *player's live* `*.profile` saves: `$MERCS2_SAVES_DIR`, then
/// `Documents/My Games/…` under `$USERPROFILE` or `$HOME`.
///
/// This is for the running game, not for tests — tests use [`save_fixtures`], so they neither depend on
/// nor are perturbed by whatever saves the host happens to have.
///
/// Both home variables are tried on every platform: a Wine/Proton prefix sets `USERPROFILE` on Linux,
/// and a shell can set `HOME` on Windows. Returns `None` when none of them exists — callers should
/// SKIP rather than panic, because a developer's own save folder is not something CI can be expected
/// to have. `mercs2_engine::paths::resolve_saves_dir` adds the CLI flag and an install-relative
/// fallback on top of this.
pub fn saves_dir_from_env() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(SAVES_DIR_VAR).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(v);
        if p.is_dir() {
            return Some(p);
        }
    }
    ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
        .map(|home| PathBuf::from(home).join(SAVES_UNDER_HOME))
        .find(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder and a file both resolve, and a directory that holds no archive yields `None` so the
    /// caller falls through instead of being handed an unopenable path.
    #[test]
    fn folder_and_file_forms() {
        let root = std::env::temp_dir().join("mercs2_formats_game_paths");
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data").join("vz.wad"), b"x").unwrap();

        // `wad_from_env` reads the process environment, which tests must not mutate (it is shared and
        // unsafe to set concurrently in Rust 2024). Exercise the path logic directly instead.
        let probe = |p: PathBuf| -> Option<PathBuf> {
            if p.is_file() {
                return Some(p);
            }
            [p.join("data").join("vz.wad"), p.join("vz.wad")]
                .into_iter()
                .find(|c| c.is_file())
        };
        let want = root.join("data").join("vz.wad");
        assert_eq!(probe(root.clone()), Some(want.clone()), "install root");
        assert_eq!(probe(root.join("data")), Some(want.clone()), "data folder");
        assert_eq!(probe(want.clone()), Some(want), "the file itself");
        assert_eq!(probe(root.join("absent")), None, "no archive under it");

        std::fs::remove_dir_all(&root).ok();
    }
}
