//! The game stack — the retail WADs a build reads from.
//!
//! **The BUILD path is path-in, never path-discovering.** [`GameStack::open`] takes a resolved list
//! and nothing in `build`/`lint` ever goes looking, so a Shipment build cannot silently pick up an
//! install nobody chose — and `qm lint` runs in template CI where the retail WADs will never exist.
//!
//! [`discover`] is offered *alongside* that, for HOSTS to call: the Workshop Settings page, the
//! `qm` CLI, and the test suite all need the same resolution order (Plan 02), and having three
//! implementations of it would guarantee three behaviours. It is a separate, opt-in entry point —
//! the separation is about who decides, not about refusing to help.
//!
//! Order is `[base, overlays…]` and resolution is a **reverse walk — last mounted wins** — matching
//! the engine (`FUN_00875E80`) and what `mercs2_workshop::publish` already does. Note this is the
//! opposite direction from the runtime chunk registry, which is first-writer-wins; the two rules
//! run simultaneously and getting them backwards is the classic error here.

use mercs2_formats::ffcs::{load_ffcs_archive, Endian, FfcsArchive};
use mercs2_formats::texture::{extract_texture, TextureData};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Which bake a WAD belongs to.
///
/// Console WADs are a deliberate part of the corpus — Shipments are expected to export to every
/// platform, not just PC — so opening one is NOT an error. What a given operation can *do* with it
/// is a separate question, answered where that operation lives.
///
/// Xbox 360 and PS3 both present `SCFF`/big-endian and are indistinguishable from the header alone,
/// hence one variant rather than two guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Platform {
    /// `FFCS`, little-endian, `sges` blocks.
    Pc,
    /// `SCFF`, big-endian, `segs` blocks — Xbox 360 or PS3.
    BigEndianConsole,
}

impl Platform {
    fn of(endian: Endian) -> Platform {
        match endian {
            Endian::Little => Platform::Pc,
            Endian::Big => Platform::BigEndianConsole,
        }
    }
}

#[derive(Debug)]
pub enum GameStackError {
    Open { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    /// A stack mixing a PC bake with a console bake. Resolution walks the whole stack, so a mixed
    /// one would silently read structures of the wrong endianness.
    MixedPlatforms { paths: Vec<PathBuf> },
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
            GameStackError::MixedPlatforms { paths } => {
                let list: Vec<String> =
                    paths.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "the stack mixes PC and console bakes ({}) — resolution walks the whole stack, \
                     so this would read structures of the wrong endianness. Use one platform.",
                    list.join(", ")
                )
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

/// Env var holding an explicit `vz.wad` path — the highest-priority override.
pub const VZ_WAD_ENV: &str = "MERCS2_VZ_WAD";

/// Machine-local, git-ignored config naming the install. Written by `scripts/find-vz-wad.sh`.
pub const LOCAL_CONFIG: &str = ".mercs2-local.toml";

/// Where a discovered WAD came from — surfaced so the UI can show it. "Which install was it actually
/// reading" is behind a large share of our own trap reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Env,
    LocalConfig,
    CoLocated,
    Registry,
}

#[derive(Debug, Clone)]
pub struct Discovered {
    pub path: PathBuf,
    pub origin: Origin,
}

/// Locate a `vz.wad` **for a HOST to hand to [`GameStack::open`]**.
///
/// This is a convenience for hosts (the Workshop Settings page, the `qm` CLI, tests) and implements
/// the order Plan 02 specifies. **The build path never calls it** — the crate stays path-in, so a
/// Shipment build cannot silently pick up an install nobody chose.
///
/// Order, first hit wins:
/// 1. `MERCS2_VZ_WAD`
/// 2. `.mercs2-local.toml` (`vz_wad = "…"`), searched upward from `start`
/// 3. co-located `Mercenaries2.exe` next to the running binary, then `data/vz.wad`
/// 4. the EA registry key (Windows only — the other arm returns `None`, which is why 2 and 3 exist)
pub fn discover_from(start: &Path) -> Option<Discovered> {
    if let Some(p) = std::env::var_os(VZ_WAD_ENV) {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(Discovered { path, origin: Origin::Env });
        }
    }
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(LOCAL_CONFIG);
        if candidate.is_file() {
            if let Some(path) = read_local_config(&candidate) {
                if path.is_file() {
                    return Some(Discovered { path, origin: Origin::LocalConfig });
                }
            }
        }
        dir = d.parent();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            if exe_dir.join("Mercenaries2.exe").is_file() {
                let candidate = exe_dir.join("data").join("vz.wad");
                if candidate.is_file() {
                    return Some(Discovered { path: candidate, origin: Origin::CoLocated });
                }
            }
        }
    }
    mercs2_engine_registry_vz_wad()
        .map(|path| Discovered { path, origin: Origin::Registry })
}

/// Discover starting from the current directory.
pub fn discover() -> Option<Discovered> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    discover_from(&cwd)
}

fn read_local_config(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    let doc: toml::Value = toml::from_str(&text).ok()?;
    let raw = doc.get("vz_wad")?.as_str()?;
    Some(PathBuf::from(shellexpand_home(raw)))
}

/// Expand a leading `~/` so the config can be written by hand without an absolute path.
fn shellexpand_home(raw: &str) -> String {
    match raw.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => format!("{}/{rest}", home.to_string_lossy()),
            None => raw.to_string(),
        },
        None => raw.to_string(),
    }
}

/// The registry lookup lives in `mercs2_engine`, which pulls winit + wgpu — far too heavy for a
/// headless crate to depend on. It is a dozen lines and Windows-only, so it is reproduced rather
/// than depended upon; the non-Windows arm matches (`None`).
#[cfg(windows)]
fn mercs2_engine_registry_vz_wad() -> Option<PathBuf> {
    use std::process::Command;
    // Avoid a winreg dependency for one key: ask the OS.
    let out = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames",
            "/v",
            "Install Dir",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("Install Dir"))?;
    let dir = line.split("REG_SZ").nth(1)?.trim();
    let candidate = Path::new(dir).join("data").join("vz.wad");
    candidate.is_file().then_some(candidate)
}

#[cfg(not(windows))]
fn mercs2_engine_registry_vz_wad() -> Option<PathBuf> {
    None
}

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
        // A console bake is fine on its own; MIXING is not, because resolution walks the stack.
        let platforms: std::collections::BTreeSet<Platform> =
            wads.iter().map(|w| Platform::of(w.archive.endian)).collect();
        if platforms.len() > 1 {
            return Err(GameStackError::MixedPlatforms {
                paths: wads.iter().map(|w| w.path.clone()).collect(),
            });
        }
        Ok(GameStack { wads })
    }

    /// Which bake this stack is. Opening a console WAD is allowed; whether a given operation
    /// supports it is decided by that operation — see `build`.
    pub fn platform(&self) -> Platform {
        self.wads
            .first()
            .map(|w| Platform::of(w.archive.endian))
            .unwrap_or(Platform::Pc)
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

    /// The raw `(packed_block_ref, secondary_ref)` of an asset's PRIMARY row, last-mounted-wins.
    ///
    /// Exposed raw because those two words encode the asset's whole **LOD chain** — up to four
    /// rungs, not one block (`docs/aset_format.md`, proven 2026-07-21). Callers that need to know
    /// whether an asset is single-block must inspect both halves; see
    /// [`crate::lint::aset_row_is_single_block`].
    /// Every ASET row for `(hash, type_id)` across the stack as
    /// `(packed_block_ref, secondary_ref, is_primary)`.
    ///
    /// A texture may have **no primary row at all** — shared/aliased assets are carried as
    /// sub-entries inside another asset's block, and `extract_texture` resolves them by falling
    /// back to any `type_id 27` row (`docs/modernization/texture_extraction_notes.md`). Callers that
    /// only look for a primary row will silently see nothing for exactly those assets.
    pub fn aset_rows(&self, name_hash: u32, type_id: u32) -> Vec<(u32, u32, bool)> {
        let mut out = Vec::new();
        for wad in self.wads.iter().rev() {
            for e in wad
                .archive
                .aset
                .iter()
                .filter(|e| e.asset_hash == name_hash && e.type_id == type_id)
            {
                out.push((e.packed_block_ref, e.secondary_ref, e.is_primary()));
            }
        }
        out
    }

    pub fn primary_lod_chain(&self, name_hash: u32, type_id: u32) -> Option<(u32, u32)> {
        for wad in self.wads.iter().rev() {
            if let Some(e) = wad
                .archive
                .aset
                .iter()
                .find(|e| e.asset_hash == name_hash && e.type_id == type_id && e.is_primary())
            {
                return Some((e.packed_block_ref, e.secondary_ref));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qm_game_{}_{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn local_config_is_read_and_tilde_expanded() {
        let dir = scratch("cfg");
        let wad = dir.join("vz.wad");
        std::fs::write(&wad, b"x").unwrap();
        std::fs::write(dir.join(LOCAL_CONFIG), format!("vz_wad = \"{}\"\n", wad.display()))
            .unwrap();
        assert_eq!(read_local_config(&dir.join(LOCAL_CONFIG)), Some(wad));

        // `~/` is expanded so the file can be written by hand.
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(shellexpand_home("~/a/b"), format!("{home}/a/b"));
        assert_eq!(shellexpand_home("/abs/path"), "/abs/path");
    }

    /// The config is found by walking UP, so a test running from a subdirectory still sees the
    /// repo-root file.
    #[test]
    fn the_config_is_found_from_a_subdirectory() {
        let dir = scratch("walkup");
        let wad = dir.join("vz.wad");
        std::fs::write(&wad, b"x").unwrap();
        std::fs::write(dir.join(LOCAL_CONFIG), format!("vz_wad = \"{}\"\n", wad.display()))
            .unwrap();
        let deep = dir.join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();

        // Only meaningful when the env override is absent — it outranks the config by design.
        if std::env::var_os(VZ_WAD_ENV).is_none() {
            let found = discover_from(&deep).expect("should walk up to the config");
            assert_eq!(found.origin, Origin::LocalConfig);
            assert_eq!(found.path, wad);
        }
    }

    #[test]
    fn a_config_pointing_at_a_missing_file_is_ignored_rather_than_trusted() {
        let dir = scratch("missing");
        std::fs::write(dir.join(LOCAL_CONFIG), "vz_wad = \"/nope/vz.wad\"\n").unwrap();
        assert_eq!(read_local_config(&dir.join(LOCAL_CONFIG)), Some(PathBuf::from("/nope/vz.wad")));
        if std::env::var_os(VZ_WAD_ENV).is_none() {
            // discover_from must not return a path that does not exist.
            assert!(discover_from(&dir).is_none_or(|d| d.path.is_file()));
        }
    }

    /// A console bake OPENS fine — Shipments are expected to export to every platform, so refusing
    /// to read one would be wrong. Only EMITTING for it is unsupported, and that is the builder's
    /// call, not this layer's.
    #[test]
    fn a_console_bake_opens_and_reports_its_platform() {
        let Some(found) = discover() else { return };
        let dir = found.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for name in ["xbox-vz.wad", "ps3-VZ.WAD"] {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            let stack = GameStack::open(std::slice::from_ref(&candidate))
                .unwrap_or_else(|e| panic!("a console bake must open, not error: {e}"));
            assert_eq!(stack.platform(), Platform::BigEndianConsole, "{name}");
        }
    }

    /// Mixing platforms in one stack IS an error: resolution walks the whole stack, so it would read
    /// structures of the wrong endianness.
    #[test]
    fn a_mixed_platform_stack_is_rejected() {
        let Some(found) = discover() else { return };
        let dir = found.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let console = dir.join("xbox-vz.wad");
        if !console.is_file() {
            return;
        }
        let err = GameStack::open(&[found.path.clone(), console]).unwrap_err();
        assert!(err.to_string().contains("mixes PC and console"), "{err}");
    }

    #[test]
    fn an_empty_stack_explains_that_lint_still_works() {
        let err = GameStack::open(&[]).unwrap_err();
        assert!(err.to_string().contains("qm lint"), "{err}");
    }
}
