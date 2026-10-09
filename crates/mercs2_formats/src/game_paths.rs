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
/// * `unpacked_exe` — the SecuROM-unpacked game executable (`mercs2_unpacked.exe`, a runtime dump).
///   Read only by the tests that disassemble the engine ([`local_config_unpacked_exe`]).
/// * `exe_corpus` — a **directory** of retail `Mercenaries2.exe` builds. Read only by the exe-map
///   tests ([`local_config_exe_corpus`]).
/// * `exe_dumps` — a **directory** of executables and process-image dumps derived from them. Read
///   only by the exe-map tests ([`local_config_exe_dumps`]).
/// * `drmfree_exe` — the DRM-free executable (`mercs2_nodrm_v3.exe`, v1.0 code). Read only by the
///   exe-map tests ([`local_config_drmfree_exe`]).
/// * `v11_patcher_exe` — the official v1.1 patcher (`mercenaries2_patch1_*.exe`). Read only by the
///   exe-map patcher test ([`local_config_v11_patcher_exe`]).
/// * `wine_binary` — the `wine64` binary that runs the patcher. Read only by the exe-map patcher
///   test ([`local_config_wine_binary`]).
/// * `x360_devkit_iso` — the Jul 11 2008 Xbox 360 preview disc image. Read only by the exe-map
///   console-image tests ([`local_config_x360_devkit_iso`]).
/// * `x360_retail_xex` — the Xbox 360 retail `default.xex` (NTSC-U/J). Read only by the exe-map
///   console-image tests ([`local_config_x360_retail_xex`]).
/// * `ps3_eboot_bin` — the PS3 disc's `EBOOT.BIN` (BLUS30056). Read only by the exe-map
///   console-image tests ([`local_config_ps3_eboot_bin`]).
/// * `ps3_update_pkgs` — a **directory** holding the PS3 v1.03 update `.pkg` files. Read only by
///   the exe-map console-image tests ([`local_config_ps3_update_pkgs`]).
/// * `pc_demo_zip` — the public PC demo, `Mercs2_Demo_Setup.zip`. Read only by the exe-map Lua
///   corpus tests ([`local_config_pc_demo_zip`]).
/// * `pc_official_install_dir` — a **directory**: the official PC release as EA's download manager
///   installs it. Read only by the exe-map Lua corpus tests ([`local_config_pc_official_install_dir`]).
/// * `pc_repack_install_dir` — a **directory**: the ElAmigos repack, installed once per language,
///   English last. Read only by the exe-map Lua corpus tests ([`local_config_pc_repack_install_dir`]).
///
/// The console keys and the executable keys are never inferred from `vz_wad`'s folder: each lives
/// wherever it is kept, and a test that needs one fails naming the missing key.
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
/// The `unpacked_exe` key of [`LOCAL_CONFIG`]: the SecuROM-unpacked game executable.
pub const UNPACKED_EXE_KEY: &str = "unpacked_exe";
/// The `exe_corpus` key of [`LOCAL_CONFIG`]: a directory of retail executables.
pub const EXE_CORPUS_KEY: &str = "exe_corpus";
/// The `exe_dumps` key of [`LOCAL_CONFIG`]: a directory of derived executables and image dumps.
pub const EXE_DUMPS_KEY: &str = "exe_dumps";
/// The `drmfree_exe` key of [`LOCAL_CONFIG`]: the DRM-free executable.
pub const DRMFREE_EXE_KEY: &str = "drmfree_exe";
/// The `v11_patcher_exe` key of [`LOCAL_CONFIG`]: the official v1.1 patcher.
pub const V11_PATCHER_EXE_KEY: &str = "v11_patcher_exe";
/// The `wine_binary` key of [`LOCAL_CONFIG`]: the `wine64` binary.
pub const WINE_BINARY_KEY: &str = "wine_binary";
/// The `x360_devkit_iso` key of [`LOCAL_CONFIG`]: the Xbox 360 preview disc image.
pub const X360_DEVKIT_ISO_KEY: &str = "x360_devkit_iso";
/// The `x360_retail_xex` key of [`LOCAL_CONFIG`]: the Xbox 360 retail `default.xex`.
pub const X360_RETAIL_XEX_KEY: &str = "x360_retail_xex";
/// The `ps3_eboot_bin` key of [`LOCAL_CONFIG`]: the PS3 disc `EBOOT.BIN`.
pub const PS3_EBOOT_BIN_KEY: &str = "ps3_eboot_bin";
/// The `ps3_update_pkgs` key of [`LOCAL_CONFIG`]: a directory of PS3 update packages.
pub const PS3_UPDATE_PKGS_KEY: &str = "ps3_update_pkgs";
/// The `pc_demo_zip` key of [`LOCAL_CONFIG`]: the public PC demo archive.
pub const PC_DEMO_ZIP_KEY: &str = "pc_demo_zip";
/// The `pc_official_install_dir` key of [`LOCAL_CONFIG`]: the official PC install.
pub const PC_OFFICIAL_INSTALL_DIR_KEY: &str = "pc_official_install_dir";
/// The `pc_repack_install_dir` key of [`LOCAL_CONFIG`]: the ElAmigos repack install.
pub const PC_REPACK_INSTALL_DIR_KEY: &str = "pc_repack_install_dir";

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

/// The SecuROM-unpacked executable for a **game-gated test**: `unpacked_exe` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key; it names a runtime dump kept by hand.
pub fn local_config_unpacked_exe(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        UNPACKED_EXE_KEY,
        "write `unpacked_exe = \"/path/to/mercs2_unpacked.exe\"` into it by hand",
    )
}

/// The retail executable directory for a **game-gated test**: `exe_corpus` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`], except that the path
/// must be a directory. No script writes this key.
pub fn local_config_exe_corpus(start: &Path) -> Result<PathBuf, String> {
    local_config_path(
        start,
        EXE_CORPUS_KEY,
        Want::Dir,
        "write `exe_corpus = \"/path/to/exe_sha256\"` into it by hand",
    )
}

/// The derived-executable directory for a **game-gated test**: `exe_dumps` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`], except that the path
/// must be a directory. No script writes this key.
pub fn local_config_exe_dumps(start: &Path) -> Result<PathBuf, String> {
    local_config_path(
        start,
        EXE_DUMPS_KEY,
        Want::Dir,
        "write `exe_dumps = \"/path/to/securom_dump\"` into it by hand",
    )
}

/// The DRM-free executable for a **game-gated test**: `drmfree_exe` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_drmfree_exe(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        DRMFREE_EXE_KEY,
        "write `drmfree_exe = \"/path/to/mercs2_nodrm_v3.exe\"` into it by hand",
    )
}

/// The official v1.1 patcher for a **game-gated test**: `v11_patcher_exe` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_v11_patcher_exe(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        V11_PATCHER_EXE_KEY,
        "write `v11_patcher_exe = \"/path/to/mercenaries2_patch1_na.exe\"` into it by hand",
    )
}

/// The `wine64` binary for a **game-gated test**: `wine_binary` in the nearest [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_wine_binary(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        WINE_BINARY_KEY,
        "write `wine_binary = \"/path/to/wine/bin/wine64\"` into it by hand",
    )
}

/// The Xbox 360 preview disc image for a **game-gated test**: `x360_devkit_iso` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_x360_devkit_iso(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        X360_DEVKIT_ISO_KEY,
        "write `x360_devkit_iso = \"/path/to/Mercenaries 2 Preview X360 (Jul 11 2008).iso\"` into it by hand",
    )
}

/// The Xbox 360 retail `default.xex` for a **game-gated test**: `x360_retail_xex` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_x360_retail_xex(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        X360_RETAIL_XEX_KEY,
        "write `x360_retail_xex = \"/path/to/default.xex\"` into it by hand",
    )
}

/// The PS3 disc `EBOOT.BIN` for a **game-gated test**: `ps3_eboot_bin` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_ps3_eboot_bin(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        PS3_EBOOT_BIN_KEY,
        "write `ps3_eboot_bin = \"/path/to/EBOOT.BIN\"` into it by hand",
    )
}

/// The public PC demo archive for a **game-gated test**: `pc_demo_zip` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`]. No script writes this
/// key.
pub fn local_config_pc_demo_zip(start: &Path) -> Result<PathBuf, String> {
    local_config_file(
        start,
        PC_DEMO_ZIP_KEY,
        "write `pc_demo_zip = \"/path/to/Mercs2_Demo_Setup.zip\"` into it by hand",
    )
}

/// The official PC install for a **game-gated test**: `pc_official_install_dir` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`], except that the path
/// must be a directory. No script writes this key.
pub fn local_config_pc_official_install_dir(start: &Path) -> Result<PathBuf, String> {
    local_config_path(
        start,
        PC_OFFICIAL_INSTALL_DIR_KEY,
        Want::Dir,
        "write `pc_official_install_dir = \"/path/to/Mercenaries 2 World in Flames\"` into it by hand",
    )
}

/// The ElAmigos repack install for a **game-gated test**: `pc_repack_install_dir` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`], except that the path
/// must be a directory. No script writes this key.
pub fn local_config_pc_repack_install_dir(start: &Path) -> Result<PathBuf, String> {
    local_config_path(
        start,
        PC_REPACK_INSTALL_DIR_KEY,
        Want::Dir,
        "write `pc_repack_install_dir = \"/path/to/Mercenaries 2 World in Flames\"` into it by hand",
    )
}

/// The PS3 update package directory for a **game-gated test**: `ps3_update_pkgs` in the nearest
/// [`LOCAL_CONFIG`].
///
/// Same rules and the same three distinct errors as [`local_config_vz_wad`], except that the path
/// must be a directory. No script writes this key.
pub fn local_config_ps3_update_pkgs(start: &Path) -> Result<PathBuf, String> {
    local_config_path(
        start,
        PS3_UPDATE_PKGS_KEY,
        Want::Dir,
        "write `ps3_update_pkgs = \"/path/to/updates\"` into it by hand",
    )
}

/// What a [`LOCAL_CONFIG`] key must name.
#[derive(Clone, Copy)]
enum Want {
    File,
    Dir,
}

/// The file `key` names in the nearest [`LOCAL_CONFIG`] above `start`, or an error saying which of
/// the three things is wrong — no config, no `key`, or a path that is not a file — ending in `fix`.
fn local_config_file(start: &Path, key: &str, fix: &str) -> Result<PathBuf, String> {
    local_config_path(start, key, Want::File, fix)
}

/// The path `key` names in the nearest [`LOCAL_CONFIG`] above `start`, which must be `want`.
fn local_config_path(start: &Path, key: &str, want: Want, fix: &str) -> Result<PathBuf, String> {
    let Some((file, text)) = local_configs(start).next() else {
        return Err(format!(
            "game-gated test: no {LOCAL_CONFIG} found in {} or any folder above it; {fix}",
            start.display()
        ));
    };
    let Some(path) = config_value(&text, key) else {
        return Err(format!("game-gated test: {} has no `{key}` key; {fix}", file.display()));
    };
    let (ok, noun) = match want {
        Want::File => (path.is_file(), "a file"),
        Want::Dir => (path.is_dir(), "a directory"),
    };
    if !ok {
        return Err(format!(
            "game-gated test: {} names {key} = {}, which is not {noun}; {fix}",
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

    /// `unpacked_exe` reports the three failures under its own key and matches it exactly.
    #[test]
    fn the_unpacked_exe_resolver_matches_its_key_exactly_and_reports_each_failure() {
        let root = std::env::temp_dir().join("mercs2_formats_local_config_unpacked_exe");
        std::fs::remove_dir_all(&root).ok();
        let deep = root.join("a");
        std::fs::create_dir_all(&deep).unwrap();

        let e = local_config_unpacked_exe(&deep).unwrap_err();
        assert!(e.contains("no .mercs2-local.toml found"), "{e}");
        assert!(e.contains("unpacked_exe = "), "{e}");

        let (exe, pc) = (root.join("mercs2_unpacked.exe"), root.join("vz.wad"));
        let config = root.join(LOCAL_CONFIG);
        std::fs::write(&pc, b"x").unwrap();
        std::fs::write(&config, format!("vz_wad = \"{}\"\n", pc.display())).unwrap();
        let e = local_config_unpacked_exe(&deep).unwrap_err();
        assert!(e.contains("has no `unpacked_exe` key"), "{e}");

        std::fs::write(&config, format!("vz_wad = \"{}\"\nunpacked_exe = \"{}\"\n", pc.display(), exe.display())).unwrap();
        let e = local_config_unpacked_exe(&deep).unwrap_err();
        assert!(e.contains("names unpacked_exe = ") && e.contains("which is not a file"), "{e}");

        std::fs::write(&exe, b"MZ").unwrap();
        assert_eq!(local_config_unpacked_exe(&deep), Ok(exe));
        assert_eq!(local_config_vz_wad(&deep), Ok(pc));

        std::fs::remove_dir_all(&root).ok();
    }

    /// The exe-map keys: the two directory keys reject a file, the file keys reject a directory, and
    /// each matches its key exactly.
    #[test]
    fn the_exe_map_resolvers_require_their_kind_of_path() {
        let root = std::env::temp_dir().join("mercs2_formats_local_config_exe_map");
        std::fs::remove_dir_all(&root).ok();
        let deep = root.join("a");
        std::fs::create_dir_all(&deep).unwrap();

        let e = local_config_exe_corpus(&deep).unwrap_err();
        assert!(e.contains("no .mercs2-local.toml found") && e.contains("exe_corpus = "), "{e}");

        let (corpus, dumps, exe) = (root.join("corpus"), root.join("dumps"), root.join("nodrm.exe"));
        std::fs::create_dir_all(&corpus).unwrap();
        std::fs::create_dir_all(&dumps).unwrap();
        std::fs::write(&exe, b"MZ").unwrap();
        let config = root.join(LOCAL_CONFIG);
        std::fs::write(&config, format!("exe_corpus = \"{}\"\n", exe.display())).unwrap();
        let e = local_config_exe_corpus(&deep).unwrap_err();
        assert!(e.contains("which is not a directory"), "{e}");
        let e = local_config_exe_dumps(&deep).unwrap_err();
        assert!(e.contains("has no `exe_dumps` key"), "{e}");

        std::fs::write(
            &config,
            format!(
                "exe_corpus = \"{}\"\nexe_dumps = \"{}\"\ndrmfree_exe = \"{}\"\n",
                corpus.display(),
                dumps.display(),
                dumps.display()
            ),
        )
        .unwrap();
        assert_eq!(local_config_exe_corpus(&deep), Ok(corpus.clone()));
        assert_eq!(local_config_exe_dumps(&deep), Ok(dumps.clone()));
        let e = local_config_drmfree_exe(&deep).unwrap_err();
        assert!(e.contains("which is not a file"), "{e}");

        std::fs::write(
            &config,
            format!("exe_corpus = \"{}\"\ndrmfree_exe = \"{}\"\n", corpus.display(), exe.display()),
        )
        .unwrap();
        assert_eq!(local_config_drmfree_exe(&deep), Ok(exe.clone()));
        let e = local_config_v11_patcher_exe(&deep).unwrap_err();
        assert!(e.contains("has no `v11_patcher_exe` key"), "{e}");
        let e = local_config_wine_binary(&deep).unwrap_err();
        assert!(e.contains("has no `wine_binary` key"), "{e}");

        std::fs::write(
            &config,
            format!("v11_patcher_exe = \"{}\"\nwine_binary = \"{}\"\n", exe.display(), corpus.display()),
        )
        .unwrap();
        assert_eq!(local_config_v11_patcher_exe(&deep), Ok(exe));
        let e = local_config_wine_binary(&deep).unwrap_err();
        assert!(e.contains("which is not a file"), "{e}");

        std::fs::remove_dir_all(&root).ok();
    }

    /// The console-image keys: the disc image and EBOOT are files, the update packages a directory.
    #[test]
    fn the_console_image_resolvers_require_their_kind_of_path() {
        let root = std::env::temp_dir().join("mercs2_formats_local_config_console_images");
        std::fs::remove_dir_all(&root).ok();
        let deep = root.join("a");
        std::fs::create_dir_all(&deep).unwrap();
        let (iso, eboot, pkgs) = (root.join("preview.iso"), root.join("EBOOT.BIN"), root.join("updates"));
        std::fs::write(&iso, b"iso").unwrap();
        std::fs::write(&eboot, b"SCE").unwrap();
        std::fs::create_dir_all(&pkgs).unwrap();
        let config = root.join(LOCAL_CONFIG);

        std::fs::write(&config, format!("ps3_update_pkgs = \"{}\"\n", eboot.display())).unwrap();
        let e = local_config_ps3_update_pkgs(&deep).unwrap_err();
        assert!(e.contains("which is not a directory"), "{e}");
        let e = local_config_x360_devkit_iso(&deep).unwrap_err();
        assert!(e.contains("has no `x360_devkit_iso` key"), "{e}");

        std::fs::write(
            &config,
            format!(
                "x360_devkit_iso = \"{}\"\nps3_eboot_bin = \"{}\"\nps3_update_pkgs = \"{}\"\n",
                iso.display(),
                pkgs.display(),
                pkgs.display()
            ),
        )
        .unwrap();
        assert_eq!(local_config_x360_devkit_iso(&deep), Ok(iso));
        assert_eq!(local_config_ps3_update_pkgs(&deep), Ok(pkgs.clone()));
        let e = local_config_ps3_eboot_bin(&deep).unwrap_err();
        assert!(e.contains("which is not a file"), "{e}");

        std::fs::write(&config, format!("ps3_eboot_bin = \"{}\"\nx360_retail_xex = \"{}\"\n", eboot.display(), pkgs.display())).unwrap();
        assert_eq!(local_config_ps3_eboot_bin(&deep), Ok(eboot.clone()));
        let e = local_config_x360_retail_xex(&deep).unwrap_err();
        assert!(e.contains("which is not a file"), "{e}");
        std::fs::write(&config, format!("x360_retail_xex = \"{}\"\n", eboot.display())).unwrap();
        assert_eq!(local_config_x360_retail_xex(&deep), Ok(eboot.clone()));
        let e = local_config_pc_demo_zip(&deep).unwrap_err();
        assert!(e.contains("has no `pc_demo_zip` key"), "{e}");
        std::fs::write(&config, format!("pc_demo_zip = \"{}\"\npc_official_install_dir = \"{}\"\n", eboot.display(), eboot.display())).unwrap();
        assert_eq!(local_config_pc_demo_zip(&deep), Ok(eboot));
        let e = local_config_pc_official_install_dir(&deep).unwrap_err();
        assert!(e.contains("which is not a directory"), "{e}");
        let e = local_config_pc_repack_install_dir(&deep).unwrap_err();
        assert!(e.contains("has no `pc_repack_install_dir` key"), "{e}");
        std::fs::write(&config, format!("pc_official_install_dir = \"{}\"\npc_repack_install_dir = \"{}\"\n", pkgs.display(), pkgs.display())).unwrap();
        assert_eq!(local_config_pc_official_install_dir(&deep), Ok(pkgs.clone()));
        assert_eq!(local_config_pc_repack_install_dir(&deep), Ok(pkgs));

        std::fs::remove_dir_all(&root).ok();
    }
}
