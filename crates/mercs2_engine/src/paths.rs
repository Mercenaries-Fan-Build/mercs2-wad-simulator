//! Where the game's two roots live: the **install folder** (assets) and the **saves folder**
//! (profiles). One module so there is exactly one answer per root, and one place to change it.
//!
//! # Why this exists
//!
//! Both roots were previously discovered by Windows-only mechanisms with no override:
//!
//! * the install, from `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames` →
//!   `Install Dir` — the **only** registry key this project reads, from four separate call sites
//!   using three different mechanisms (`winreg` in [`crate::wad`] and `anim_patch`, a shell-out to
//!   `reg.exe` in `mercs2_quartermaster`);
//! * the saves, from `%USERPROFILE%\Documents\My Games\Mercenaries 2\SaveGames` — not a registry key
//!   at all, just a hardcoded relative path under an environment variable that only Windows sets.
//!
//! Neither survives contact with the ways people actually have the game: a copied-off `data` folder,
//! a Wine/Proton prefix, a reinstall that did not write the key, a non-Windows host. Both roots take
//! an explicit path first, so the registry is a *convenience*, never a requirement.
//!
//! # Resolution order
//!
//! Install root — [`resolve_game_dir`]: `--game-dir` → `$MERCS2_GAME_DIR` → `$VZ_WAD` (its parent
//! chain) → the registry key.
//!
//! Saves root — [`resolve_saves_dir`]: `--saves-dir` → `$MERCS2_SAVES_DIR` → `%USERPROFILE%` →
//! `$HOME` → `<install>/SaveGames`.

use std::path::{Path, PathBuf};

/// Explicit install root — a folder holding `data/vz.wad`, or the `data` folder, or `vz.wad` itself.
/// The names actually scanned live in `mercs2_formats::game_paths::GAME_DIR_VARS`; this re-states the
/// primary one for callers that want to print it in a hint.
pub const GAME_DIR_ENV: &str = "MERCS2_GAME_DIR";

/// Explicit saves folder — the directory holding `*.profile`. Scanned by
/// `mercs2_formats::game_paths::saves_dir_from_env`, re-stated here for hint text.
pub const SAVES_DIR_ENV: &str = "MERCS2_SAVES_DIR";

/// The install folder, given whichever form the caller has in hand.
///
/// Accepts the install root, its `data` folder, or the `vz.wad` file — and normalises all three to the
/// **root**, so `<root>/SaveGames` and `<root>/data/*.wad` both resolve off one answer. Returns `None`
/// if the path holds no recognisable install.
pub fn game_dir_from(path: impl AsRef<Path>) -> Option<PathBuf> {
    let p = path.as_ref();
    // `vz.wad` itself → up through `data` to the root. `.parent()` twice, not a fixed slice, so a
    // renamed archive or a `data` folder living somewhere unusual still normalises correctly.
    if p.is_file() {
        return p.parent().and_then(|d| d.parent()).map(Path::to_path_buf).or_else(|| {
            p.parent().map(Path::to_path_buf)
        });
    }
    if !p.is_dir() {
        return None;
    }
    // The `data` folder was handed in directly: its parent is the root.
    if p.join("vz.wad").is_file() {
        return p.parent().map(Path::to_path_buf).or_else(|| Some(p.to_path_buf()));
    }
    // The root itself.
    if p.join("data").join("vz.wad").is_file() {
        return Some(p.to_path_buf());
    }
    None
}

/// Resolve the install root: explicit path → `$MERCS2_GAME_DIR` → `$VZ_WAD` → the EA Games registry
/// key. Every source accepts the three forms [`game_dir_from`] does.
pub fn resolve_game_dir(explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit.filter(|s| !s.is_empty()) {
        return game_dir_from(p);
    }
    // The environment scan itself lives in `mercs2_formats::game_paths` — the one implementation the
    // leaf crates and headless tools share. Here it only needs normalising to the install ROOT.
    if let Some(w) = mercs2_formats::game_paths::vz_wad_from_env() {
        if let Some(d) = game_dir_from(w) {
            return Some(d);
        }
    }
    // The registry yields `…/data/vz.wad`; normalise it to the root like every other source.
    crate::wad::registry_vz_wad().and_then(game_dir_from)
}

/// Resolve the saves folder: explicit path → `$MERCS2_SAVES_DIR` → `%USERPROFILE%` → `$HOME` →
/// `<install>/SaveGames`.
///
/// `game_dir` is consulted last and only as a fallback: a portable/copied install often carries its
/// own `SaveGames` beside the executable, but retail's real location is under the user's Documents,
/// so the home-directory probes must win when both exist.
///
/// An explicit path is returned **whether or not it exists yet** — the caller may be about to create
/// it, and silently substituting a different folder for the one the user named would write saves
/// somewhere they did not ask for. Every non-explicit source must exist to be chosen.
pub fn resolve_saves_dir(explicit: Option<&str>, game_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(p));
    }
    // `$MERCS2_SAVES_DIR` then the `Documents/My Games/…` probes under `$USERPROFILE`/`$HOME` — the
    // one implementation, shared with the headless crates.
    if let Some(p) = mercs2_formats::game_paths::saves_dir_from_env() {
        return Some(p);
    }
    game_dir.map(|d| d.join("SaveGames")).filter(|p| p.is_dir())
}

/// A named WAD inside a resolved install (`<root>/data/<name>`), if present.
///
/// `anim_patch` needs `vz-patch.wad` from the same install as `vz.wad`; deriving both from one
/// resolved root keeps them from disagreeing about which install is in play.
pub fn wad_in_game_dir(game_dir: &Path, name: &str) -> Option<PathBuf> {
    let p = game_dir.join("data").join(name);
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mercs2_paths_{tag}"));
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data").join("vz.wad"), b"x").unwrap();
        root
    }

    /// All three forms normalise to the SAME install root — that is what lets `<root>/SaveGames` and
    /// `<root>/data/vz-patch.wad` be derived from one answer instead of three ad-hoc joins.
    #[test]
    fn every_form_normalises_to_the_root() {
        let root = fixture("norm");
        let data = root.join("data");
        let wad = data.join("vz.wad");

        assert_eq!(game_dir_from(&root).as_deref(), Some(root.as_path()), "the root itself");
        assert_eq!(game_dir_from(&data).as_deref(), Some(root.as_path()), "the data folder");
        assert_eq!(game_dir_from(&wad).as_deref(), Some(root.as_path()), "the wad file");

        // A sibling WAD is then reachable off that one root.
        std::fs::write(data.join("vz-patch.wad"), b"x").unwrap();
        assert_eq!(
            wad_in_game_dir(&root, "vz-patch.wad"),
            Some(data.join("vz-patch.wad")),
            "the patch WAD comes from the same install as the base WAD"
        );
        assert!(wad_in_game_dir(&root, "absent.wad").is_none());

        // A folder with no install under it yields nothing, so callers fall through to the next source
        // rather than being handed a root that has no assets in it.
        assert!(game_dir_from(root.join("data").join("nope")).is_none());
        assert_eq!(resolve_game_dir(Some(&root.to_string_lossy())).as_deref(), Some(root.as_path()));

        std::fs::remove_dir_all(&root).ok();
    }

    /// An explicit saves path is honoured even when it does not exist yet (the caller may create it),
    /// while a co-located `SaveGames` is only chosen when it is really there.
    #[test]
    fn saves_dir_precedence() {
        let root = fixture("saves");
        let explicit = root.join("not_created_yet");
        assert_eq!(
            resolve_saves_dir(Some(&explicit.to_string_lossy()), Some(&root)).as_deref(),
            Some(explicit.as_path()),
            "an explicit path wins and is not required to exist"
        );

        // Without an explicit path the home probes run first; this test cannot depend on whether the
        // host has a real save folder, so it only asserts the property that holds either way: whatever
        // comes back is never the co-located folder while that folder is absent.
        let co_located = root.join("SaveGames");
        assert!(!co_located.exists());
        assert_ne!(resolve_saves_dir(None, Some(&root)).as_deref(), Some(co_located.as_path()));

        std::fs::create_dir_all(&co_located).unwrap();
        // Once it exists it is a legal answer — but still only if no home-directory folder outranked it.
        let got = resolve_saves_dir(None, Some(&root));
        assert!(got.is_some(), "a co-located SaveGames is found when nothing outranks it");

        std::fs::remove_dir_all(&root).ok();
    }
}
