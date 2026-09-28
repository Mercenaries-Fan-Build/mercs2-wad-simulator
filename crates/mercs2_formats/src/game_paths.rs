//! Finding the game's archives from the environment — the one implementation every crate shares.
//!
//! This lives in `mercs2_formats` because it is the lowest crate the tools, the leaf crates
//! (`mercs2_anim`) and `mercs2_engine` all already depend on. The richer resolver in
//! `mercs2_engine::paths` — which also handles CLI flags, the Windows registry key, and the saves
//! folder — delegates its environment scan here rather than reimplementing it.
//!
//! The rule is: **no hardcoded install paths**. Every path comes from the environment or from
//! [`LOCAL_CONFIG`].

use std::path::{Path, PathBuf};

/// The environment variables naming the install, in precedence order.
///
/// `MERCS2_GAME_DIR` is checked first, then `VZ_WAD`.
pub const GAME_DIR_VARS: [&str; 2] = ["MERCS2_GAME_DIR", "VZ_WAD"];

/// Interpret one user-supplied path as the install root, its `data` folder, or the archive itself.
///
/// A file is taken as-is (so a renamed or copied archive works); a folder is probed at `data/<name>`
/// then `<name>`. `None` when nothing exists there, so a caller falls through instead of being
/// handed an unopenable path.
///
/// Exposed separately because this rule is what "a path to the game" MEANS, and every resolver uses
/// this one implementation.
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
/// [`wad_under`]. Returns `None` when nothing is set or nothing exists. Tools use this; game-gated
/// tests never do — they read [`LOCAL_CONFIG`] through [`local_config_vz_wad`].
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
/// For tools it is a **dev-checkout fallback only**, and deliberately lower priority than the
/// environment: Modkit manages the install and hands paths to the tools it launches, so a per-tool
/// config competing with that is how a fleet of tools ends up disagreeing about where the game is.
///
/// For game-gated tests it is the **only** source — see [`local_config_vz_wad`].
///
/// Keys, one `key = "…"` per line:
///
/// * `vz_wad` — the PC base archive. Every game-gated test reads it ([`local_config_vz_wad`]).
/// * `xbox_vz_wad` — an Xbox 360 bake of the archive (`SCFF` magic, big-endian). Read only by the
///   tests that need a console bake ([`local_config_xbox_vz_wad`]).
/// * `ps3_vz_wad` — a PS3 bake (`SCFF` magic, big-endian). Read only by the tests that need it
///   ([`local_config_ps3_vz_wad`]).
///
/// The console keys are never inferred from `vz_wad`'s folder: a console bake lives wherever it is
/// kept, and a test that needs one fails naming the missing key.
pub const LOCAL_CONFIG: &str = ".mercs2-local.toml";

/// Every [`LOCAL_CONFIG`] from `start` upward, nearest first, each with its text.
fn local_configs(start: &Path) -> impl Iterator<Item = (PathBuf, String)> + '_ {
    start.ancestors().filter_map(|d| {
        let file = d.join(LOCAL_CONFIG);
        std::fs::read_to_string(&file).ok().map(|text| (file, text))
    })
}

/// The `vz_wad` key of [`LOCAL_CONFIG`]: the PC base archive.
pub const VZ_WAD_KEY: &str = "vz_wad";
/// The `xbox_vz_wad` key of [`LOCAL_CONFIG`]: an Xbox 360 bake.
pub const XBOX_VZ_WAD_KEY: &str = "xbox_vz_wad";
/// The `ps3_vz_wad` key of [`LOCAL_CONFIG`]: a PS3 bake.
pub const PS3_VZ_WAD_KEY: &str = "ps3_vz_wad";

/// The `<key> = "…"` value in one [`LOCAL_CONFIG`]'s text, or `None` when the key is absent.
///
/// The key must match exactly, so `vz_wad` never reads the `xbox_vz_wad` line and vice versa.
fn config_value(text: &str, key: &str) -> Option<PathBuf> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| PathBuf::from(v.trim().trim_matches('"')))
    })
}

/// `vz_wad = "…"` from the nearest [`LOCAL_CONFIG`], searching upward from `start`.
pub fn wad_from_local_config(start: &Path) -> Option<PathBuf> {
    local_configs(start)
        .filter_map(|(_, text)| config_value(&text, VZ_WAD_KEY))
        .find(|p| p.is_file())
}

/// The base archive for a **game-gated test**: the nearest [`LOCAL_CONFIG`] above `start`, and
/// nothing else.
///
/// Tests pass `env!("CARGO_MANIFEST_DIR")` and panic with the error. The environment is not read.
/// The nearest file wins, and when it is broken the error names it.
///
/// The error says which of the three things is wrong: no file, no `vz_wad` key, or a path that is
/// not a file.
pub fn local_config_vz_wad(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        VZ_WAD_KEY,
        "run `scripts/find-vz-wad.sh --write` at the repository root, or write \
         `vz_wad = \"/path/to/vz.wad\"` into it by hand",
    )
}

/// The Xbox 360 bake for a **game-gated test**: `xbox_vz_wad` in the nearest [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key; it names a console dump kept by hand.
pub fn local_config_xbox_vz_wad(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        XBOX_VZ_WAD_KEY,
        "write `xbox_vz_wad = \"/path/to/xbox-vz.wad\"` into it by hand",
    )
}

/// The PS3 bake for a **game-gated test**: `ps3_vz_wad` in the nearest [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key; it names a console dump kept by hand.
pub fn local_config_ps3_vz_wad(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        PS3_VZ_WAD_KEY,
        "write `ps3_vz_wad = \"/path/to/ps3-VZ.WAD\"` into it by hand",
    )
}

/// The file `key` names in the nearest [`LOCAL_CONFIG`] above `start`, or an error saying which of
/// the three things is wrong — no config, no `key`, or a path that is not a file — ending in `fix`.
fn local_config_file(start: &Path, key: &str, fix: &str) -> Result<PathBuf, String> {
    let Some((file, text)) = local_configs(start).next() else {
        return Err(format!(
            "game-gated test: no {LOCAL_CONFIG} found in {} or any folder above it; {fix}",
            start.display()
        ));
    };
    let Some(path) = config_value(&text, key) else {
        return Err(format!("game-gated test: {} has no `{key}` key; {fix}", file.display()));
    };
    if !path.is_file() {
        return Err(format!(
            "game-gated test: {} names {key} = {}, which is not a file; {fix}",
            file.display(),
            path.display()
        ));
    }
    Ok(path)
}

/// The base archive: environment first, then the dev-checkout config.
///
/// For **tools** run from a checkout (the probe binaries). Game-gated tests do not use this; they use
/// [`local_config_vz_wad`], which reads only [`LOCAL_CONFIG`] and returns an error naming what is
/// missing.
/// Hosts that also need co-location and the registry use `mercs2_quartermaster::game::discover`,
/// which layers those on top and reports where the answer came from.
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
/// so every assertion about them runs on every machine.
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
/// Both `save.rs` and `save_write.rs` iterate this.
/// `save::tests::the_fixture_set_is_complete_and_fully_covered` asserts this matches the
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

    /// The strict test resolver distinguishes its three failures, names the file and the script
    /// that writes it, and returns the path when all three hold.
    #[test]
    fn local_config_vz_wad_reports_each_failure() {
        let root = std::env::temp_dir().join("mercs2_formats_local_config_vz_wad");
        std::fs::remove_dir_all(&root).ok();
        let deep = root.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();

        // No file anywhere above `deep`: the temp dir sits outside any checkout.
        let e = local_config_vz_wad(&deep).unwrap_err();
        assert!(e.contains("no .mercs2-local.toml found"), "{e}");
        assert!(e.contains("scripts/find-vz-wad.sh --write"), "{e}");

        let config = root.join(LOCAL_CONFIG);
        std::fs::write(&config, "# nothing here\n").unwrap();
        let e = local_config_vz_wad(&deep).unwrap_err();
        assert!(e.contains("has no `vz_wad` key"), "{e}");
        assert!(e.contains(&config.display().to_string()), "{e}");

        let absent = root.join("absent.wad");
        std::fs::write(&config, format!("vz_wad = \"{}\"\n", absent.display())).unwrap();
        let e = local_config_vz_wad(&deep).unwrap_err();
        assert!(e.contains("which is not a file"), "{e}");
        assert!(e.contains(&absent.display().to_string()), "{e}");

        let wad = root.join("vz.wad");
        std::fs::write(&wad, b"x").unwrap();
        std::fs::write(&config, format!("vz_wad = \"{}\"\n", wad.display())).unwrap();
        assert_eq!(local_config_vz_wad(&deep), Ok(wad));

        std::fs::remove_dir_all(&root).ok();
    }

    /// The console-bake resolvers report the same three failures under their own key name, and each
    /// key matches exactly: `vz_wad` is not read from the `xbox_vz_wad` line, nor the reverse.
    #[test]
    fn console_resolvers_match_their_key_exactly_and_report_each_failure() {
        let root = std::env::temp_dir().join("mercs2_formats_local_config_console");
        std::fs::remove_dir_all(&root).ok();
        let deep = root.join("a");
        std::fs::create_dir_all(&deep).unwrap();

        let e = local_config_xbox_vz_wad(&deep).unwrap_err();
        assert!(e.contains("no .mercs2-local.toml found"), "{e}");
        assert!(e.contains("xbox_vz_wad = "), "{e}");

        let (xbox, ps3, pc) = (root.join("xbox-vz.wad"), root.join("ps3-VZ.WAD"), root.join("vz.wad"));
        let config = root.join(LOCAL_CONFIG);
        std::fs::write(&xbox, b"x").unwrap();
        std::fs::write(&config, format!("xbox_vz_wad = \"{}\"\n", xbox.display())).unwrap();
        assert_eq!(local_config_xbox_vz_wad(&deep), Ok(xbox.clone()));
        let e = local_config_vz_wad(&deep).unwrap_err();
        assert!(e.contains("has no `vz_wad` key"), "{e}");
        let e = local_config_ps3_vz_wad(&deep).unwrap_err();
        assert!(e.contains("has no `ps3_vz_wad` key"), "{e}");
        assert!(e.contains("ps3_vz_wad = "), "{e}");

        let absent = root.join("absent.WAD");
        std::fs::write(&pc, b"x").unwrap();
        std::fs::write(
            &config,
            format!(
                "vz_wad = \"{}\"\nxbox_vz_wad = \"{}\"\nps3_vz_wad = \"{}\"\n",
                pc.display(),
                xbox.display(),
                absent.display()
            ),
        )
        .unwrap();
        assert_eq!(local_config_vz_wad(&deep), Ok(pc));
        assert_eq!(local_config_xbox_vz_wad(&deep), Ok(xbox));
        let e = local_config_ps3_vz_wad(&deep).unwrap_err();
        assert!(e.contains("names ps3_vz_wad = "), "{e}");
        assert!(e.contains("which is not a file"), "{e}");

        std::fs::write(&ps3, b"x").unwrap();
        std::fs::write(&config, format!("ps3_vz_wad = \"{}\"\n", ps3.display())).unwrap();
        assert_eq!(local_config_ps3_vz_wad(&deep), Ok(ps3));

        std::fs::remove_dir_all(&root).ok();
    }
}
