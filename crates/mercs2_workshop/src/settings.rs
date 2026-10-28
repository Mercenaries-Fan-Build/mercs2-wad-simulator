//! Persistent user settings: where the game is, and where the reference bundle is.
//!
//! Both were previously DISCOVERED ONLY, with no way for a user to say what the discovery got
//! wrong — and on every platform but Windows the discovery does not exist. `wad::registry_vz_wad`
//! reads an EA Games registry key, so it is `#[cfg(not(windows))] -> None`: a macOS or Linux user
//! had exactly one way to open the tool, `--wad <path>`, retyped on every launch. The reference
//! bundle was the same story in reverse — `MERCS2_WORKSHOP_DATA` or a `workshop_data/` sitting in
//! the right place, with no in-app way to point at one.
//!
//! So both paths are settable and remembered here. Resolution is `crate::chains`, most explicit
//! first, first match wins:
//!
//!   WAD   `--wad <path>` → this run's choice → this file → CO-LOCATION →
//!         `$MERCS2_GAME_DIR`/`$VZ_WAD` → the Windows registry key
//!   data  this run's choice → `MERCS2_WORKSHOP_DATA` → this file → next to the exe → a CWD
//!         walk-up → the downloaded copy
//!
//! A command-line flag still wins over a saved setting (a one-off run must not silently rewrite
//! the user's configuration), and the env var still wins for the data dir (it is how a dev points
//! a build at a scratch bundle). A source that is set but broken stops its chain, and the window
//! opens on Settings with it in red. Saving is explicit — nothing here writes the file on its own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The on-disk settings document. Every field is optional: absent means "fall back to discovery",
/// which is what a fresh install has and what "Clear" restores.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Full path to `vz.wad`. The game directory is derived from it (the overlay `vz-patch.wad`,
    /// `Loading.wad` and the shell plate are all resolved as siblings), so storing the file rather
    /// than the folder keeps one unambiguous anchor instead of two that can disagree.
    pub wad_path: Option<PathBuf>,
    /// The `workshop_data/` reference bundle — `names.bin` and the corpora that ride with it.
    pub data_dir: Option<PathBuf>,
    /// The hero: the model Inspect's Anims and FaceFX tabs play on, kept across tabs and launches.
    pub hero: Option<Hero>,
}

/// A model chosen as the hero, by its catalog row: the model's hash and the row's label (a
/// kit-template row shares its kit's hash; the label picks it out).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hero {
    pub hash: u32,
    pub label: String,
}

/// `<config>/mercs2-workshop/settings.json`, per-OS. Resolved from environment variables rather
/// than a crate: the three rules below are the whole of what a `dirs` dependency would give us.
pub fn config_path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) if !x.is_empty() => PathBuf::from(x),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        }
    };
    Some(dir.join("mercs2-workshop").join("settings.json"))
}

/// Read the saved settings. Any failure — no file, unreadable, malformed — is an EMPTY settings
/// document, never an error: a corrupt config must not stop the tool from starting, because the
/// UI that repairs it is inside the tool.
pub fn load() -> Settings {
    match config_path() {
        Some(p) => load_from(&p),
        None => Settings::default(),
    }
}

/// [`load`] from `p` rather than the per-OS config file.
pub fn load_from(p: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(p) else { return Settings::default() };
    match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[settings] {} is malformed ({e}) — ignoring it", p.display());
            Settings::default()
        }
    }
}

impl Settings {
    /// Write the settings, creating the config directory. Returns the path written, for the UI to
    /// show — "saved" with no location is not a verifiable claim.
    pub fn save(&self) -> Result<PathBuf, String> {
        let p = config_path().ok_or("no config directory on this platform")?;
        self.save_to(&p)?;
        Ok(p)
    }

    /// [`Settings::save`] to `p` rather than the per-OS config file.
    pub fn save_to(&self, p: &Path) -> Result<(), String> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// Does this path look like a game archive we can actually open? Checked by READING it, not by
/// trusting the file name: a settings page whose only feedback is "saved" would happily store a
/// path that fails at the next launch, when the error is far from the cause.
pub fn check_wad(path: &Path) -> Result<String, String> {
    let p = path.to_str().ok_or("path is not valid UTF-8")?;
    let w = mercs2_engine::wad::open(p).map_err(|e| e.to_string())?;
    let models = mercs2_engine::wad::model_list_all(&w).len();
    let patch = path.with_file_name("vz-patch.wad");
    let overlay = if patch.is_file() { ", vz-patch.wad alongside" } else { "" };
    Ok(format!("{models} models{overlay}"))
}

/// Does this directory hold a usable reference bundle? It must be a complete bundle (it holds
/// [`crate::index::BUNDLE_MANIFEST`], the only kind of folder [`crate::index::data_home`] accepts)
/// with a readable `names.bin`. Reports the name count.
pub fn check_data_dir(path: &Path) -> Result<String, String> {
    if !path.join(crate::index::BUNDLE_MANIFEST).is_file() {
        return Err(format!(
            "{} is not a complete reference bundle (no {})",
            path.display(),
            crate::index::BUNDLE_MANIFEST
        ));
    }
    let pack = path.join("names.bin");
    if !pack.is_file() {
        return Err(format!("no names.bin in {}", path.display()));
    }
    let n = crate::index::names_pack_count(&pack)
        .ok_or_else(|| format!("{} is not a names.bin", pack.display()))?;
    Ok(format!("{n} names"))
}

/// Native "where is vz.wad?" picker. `None` if the user cancelled.
pub fn pick_wad() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select the game's vz.wad")
        .add_filter("Game archive", &["wad"])
        .pick_file()
}

/// Native "where is workshop_data?" picker. `None` if the user cancelled.
pub fn pick_data_dir() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("Select the workshop_data folder").pick_folder()
}
