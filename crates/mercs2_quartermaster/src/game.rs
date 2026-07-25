//! The game stack — the retail WADs a build reads from.
//!
//! **Path-in, never path-discovering.** This crate does not look for an install; the host decides
//! where the WADs are (a Workshop Settings page, `qm --game`, or nothing at all in CI) and hands
//! the resolved list here. That separation is what lets `qm lint` run in the template repo's CI,
//! where the retail WADs will never exist.
//!
//! Order is `[base, overlays…]` and resolution is a **reverse walk — last mounted wins** — matching
//! the engine (`FUN_00875E80`) and what `mercs2_workshop::publish` already does. Note this is the
//! opposite direction from the runtime chunk registry, which is first-writer-wins; the two rules
//! run simultaneously and getting them backwards is the classic error here.

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};
use mercs2_formats::texture::{extract_texture, TextureData};
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum GameStackError {
    Open { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    Empty,
}

impl std::fmt::Display for GameStackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameStackError::Open { path, message } => {
                write!(f, "opening {}: {message}", path.display())
            }
            GameStackError::Parse { path, message } => {
                write!(f, "parsing {} as an FFCS archive: {message}", path.display())
            }
            GameStackError::Empty => write!(
                f,
                "no WADs supplied — a build needs at least the base vz.wad. Configure the game \
                 folder (Workshop Settings, or `qm --game <dir>`); `qm lint` runs without one."
            ),
        }
    }
}

impl std::error::Error for GameStackError {}

struct OpenWad {
    path: PathBuf,
    file: File,
    archive: FfcsArchive,
}

/// An opened `[base, overlays…]` stack.
pub struct GameStack {
    wads: Vec<OpenWad>,
}

impl std::fmt::Debug for GameStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameStack").field("wads", &self.paths()).finish()
    }
}

impl GameStack {
    /// Open every WAD in stack order. Fails loudly on the first unreadable one — a half-open stack
    /// would silently resolve donors from the wrong place.
    pub fn open(paths: &[PathBuf]) -> Result<GameStack, GameStackError> {
        if paths.is_empty() {
            return Err(GameStackError::Empty);
        }
        let mut wads = Vec::with_capacity(paths.len());
        for path in paths {
            let mut file = File::open(path).map_err(|e| GameStackError::Open {
                path: path.clone(),
                message: e.to_string(),
            })?;
            let size = file
                .metadata()
                .map_err(|e| GameStackError::Open { path: path.clone(), message: e.to_string() })?
                .len();
            let archive =
                load_ffcs_archive(&mut file, size).map_err(|e| GameStackError::Parse {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
            wads.push(OpenWad { path: path.clone(), file, archive });
        }
        Ok(GameStack { wads })
    }

    /// The stack as configured, base first. Shown in the UI so "which install was it reading" is
    /// never a mystery.
    pub fn paths(&self) -> Vec<&Path> {
        self.wads.iter().map(|w| w.path.as_path()).collect()
    }

    pub fn len(&self) -> usize {
        self.wads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wads.is_empty()
    }

    /// Resolve a texture by name-hash, **last mounted wins**.
    pub fn texture(&mut self, name_hash: u32) -> Option<TextureData> {
        for wad in self.wads.iter_mut().rev() {
            if let Ok(td) = extract_texture(&mut wad.file, &wad.archive, name_hash) {
                return Some(td);
            }
        }
        None
    }

    /// Whether any WAD in the stack carries an ASET row for `(hash, type_id)`. Cheap existence
    /// check that does not decompress a block.
    pub fn has_asset(&self, name_hash: u32, type_id: u32) -> bool {
        self.wads
            .iter()
            .any(|w| w.archive.aset.iter().any(|e| e.asset_hash == name_hash && e.type_id == type_id))
    }
}
