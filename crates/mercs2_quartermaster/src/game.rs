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
    Open {
        path: PathBuf,
        message: String,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    /// A stack mixing a PC bake with a console bake. Resolution walks the whole stack, so a mixed
    /// one would silently read structures of the wrong endianness.
    MixedPlatforms {
        paths: Vec<PathBuf>,
    },
    Empty,
}

impl std::fmt::Display for GameStackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameStackError::Open { path, message } => {
                write!(f, "opening {}: {message}", path.display())
            }
            GameStackError::Parse { path, message } => {
                write!(
                    f,
                    "parsing {} as an FFCS archive: {message}",
                    path.display()
                )
            }
            GameStackError::MixedPlatforms { paths } => {
                let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
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

/// Every environment variable [`discover_from`] consults, in precedence order.
///
/// `MERCS2_VZ_WAD` is this crate's own name; the other two are the workspace-wide ones
/// (`mercs2_formats::game_paths::GAME_DIR_VARS`). All are honoured so a user sets ONE variable and
/// every tool in the repo finds the same install.
///
/// This is a `const` rather than three literals at the use site because the tests below must skip when
/// **any** of them is set — a guard that named only one silently stopped testing the config walk-up the
/// moment another was exported.
pub const ENV_VARS: [&str; 3] = [VZ_WAD_ENV, "MERCS2_GAME_DIR", "VZ_WAD"];

/// True when any [`ENV_VARS`] override is active, i.e. when the lower-priority sources are unreachable.
pub fn env_override_active() -> bool {
    ENV_VARS
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

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
/// 1. [`ENV_VARS`] — `MERCS2_VZ_WAD`, `MERCS2_GAME_DIR`, `VZ_WAD`; each takes the install root, its
///    `data` folder, or the `vz.wad` file itself
/// 2. `.mercs2-local.toml` (`vz_wad = "…"`), searched upward from `start`
/// 3. co-located `Mercenaries2.exe` next to the running binary, then `data/vz.wad`
/// 4. the EA registry key (Windows only — the other arm returns `None`, which is why 2 and 3 exist)
pub fn discover_from(start: &Path) -> Option<Discovered> {
    // See [`ENV_VARS`]: the three names had forked, and quartermaster saw neither of the other two.
    for var in ENV_VARS {
        let Some(p) = std::env::var_os(var).filter(|s| !s.is_empty()) else {
            continue;
        };
        // A folder is accepted as well as a file: the install root or its `data` folder. Requiring the
        // full `…/data/vz.wad` is the papercut, and a folder is the form users actually have.
        // The dir-or-file rule is `game_paths`' to own — what "a path to the game" means should have
        // exactly one definition, and this was a verbatim copy of it.
        if let Some(path) = mercs2_formats::game_paths::wad_under(&PathBuf::from(p), "vz.wad") {
            return Some(Discovered {
                path,
                origin: Origin::Env,
            });
        }
    }
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(LOCAL_CONFIG);
        if candidate.is_file() {
            if let Some(path) = read_local_config(&candidate) {
                if path.is_file() {
                    return Some(Discovered {
                        path,
                        origin: Origin::LocalConfig,
                    });
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
                    return Some(Discovered {
                        path: candidate,
                        origin: Origin::CoLocated,
                    });
                }
            }
        }
    }
    mercs2_engine_registry_vz_wad().map(|path| Discovered {
        path,
        origin: Origin::Registry,
    })
}

/// Discover starting from the current directory.
pub fn discover() -> Option<Discovered> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    discover_from(&cwd)
}

/// Read `vz_wad` out of a local config.
///
/// # Why this is not just `toml::from_str`
///
/// It was, and on Windows that silently lost the file. A native path written into a TOML **basic**
/// string carries backslashes — `vz_wad = "C:\Users\me\…\vz.wad"` — and `\U`, `\m` and friends are
/// not valid TOML escapes, so the whole document fails to parse. `read_local_config` returned
/// `None`, discovery fell through to the registry, and the config the user wrote did nothing. No
/// error was reported anywhere, because a missing config is a legitimate state.
///
/// `scripts/find-vz-wad.sh` avoids it by writing forward slashes, which is why the generated file
/// always worked and only hand-written ones broke.
///
/// So: parse as TOML first — that is the format, and it handles quoting, comments and literal
/// strings (`'C:\path'`, which needs no escaping) correctly. Fall back to a tolerant `key = value`
/// scan only when the document does not parse, which is exactly the backslash case. The fallback is
/// the same shape the other in-tree readers of this file already use.
fn read_local_config(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    if let Ok(doc) = toml::from_str::<toml::Value>(&text) {
        if let Some(raw) = doc.get("vz_wad").and_then(|v| v.as_str()) {
            return Some(PathBuf::from(shellexpand_home(raw)));
        }
    }
    // Not valid TOML (or no `vz_wad` key). Recover the value by hand rather than discarding a
    // config the user plainly meant.
    let line = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find(|l| l.starts_with("vz_wad"))?;
    let raw = line.split_once('=')?.1.trim();
    let raw = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(raw);
    (!raw.is_empty()).then(|| PathBuf::from(shellexpand_home(raw)))
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

/// One ASET registration row, restated verbatim when re-emitting a block as an overlay.
#[derive(Debug, Clone)]
pub struct AsetRow {
    pub asset_hash: u32,
    pub packed_block_ref: u32,
    pub secondary_ref: u32,
    pub type_id: u32,
}

/// What [`GameStack::layer_block_for_edit`] hands back: the whole placement layer block, the PTHS
/// path to shadow, its archive index, and its ASET rows to restate.
#[derive(Debug, Clone)]
pub struct LayerEditInputs {
    pub block: Vec<u8>,
    pub path: String,
    pub block_index: u32,
    pub rows: Vec<AsetRow>,
}

/// What [`GameStack::model_container_for_edit`] hands back: the model's container plus the exact
/// bytes an overlay copy must reproduce (its `field_c` and its ASET LOD-chain refs) and the source
/// block index the rung remap needs.
#[derive(Debug, Clone)]
pub struct ModelEditInputs {
    /// The model's primary UCFX container, verbatim (with its CSUM) — the bytes to edit.
    pub container: Vec<u8>,
    /// The original block entry's third word, carried through so the re-emitted entry matches.
    pub field_c: u32,
    /// The ASET row's `secondary_ref` (`_P002`/`_P003` rungs) — copied so the chain is preserved.
    pub secondary_ref: u32,
    /// The ASET row's `packed_block_ref` (`_P000`/`_P001`) — copied; `_P000` is re-pointed at emit.
    pub packed_block_ref: u32,
    /// The block this container came from, so the rung remap can re-point or sentinel each rung.
    pub source_block_index: u32,
}

/// An opened `[base, overlays…]` stack.
pub struct GameStack {
    wads: Vec<OpenWad>,
}

impl std::fmt::Debug for GameStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameStack")
            .field("wads", &self.paths())
            .finish()
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
                .map_err(|e| GameStackError::Open {
                    path: path.clone(),
                    message: e.to_string(),
                })?
                .len();
            let archive =
                load_ffcs_archive(&mut file, size).map_err(|e| GameStackError::Parse {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
            wads.push(OpenWad {
                path: path.clone(),
                file,
                archive,
            });
        }
        // A console bake is fine on its own; MIXING is not, because resolution walks the stack.
        let platforms: std::collections::BTreeSet<Platform> = wads
            .iter()
            .map(|w| Platform::of(w.archive.endian))
            .collect();
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
        self.wads.iter().any(|w| {
            w.archive
                .aset
                .iter()
                .any(|e| e.asset_hash == name_hash && e.type_id == type_id)
        })
    }

    /// Every asset hash of a given type across the stack, deduplicated — for callers that must scan
    /// (a domain lens, the destructible finder) rather than name one asset up front.
    pub fn asset_hashes(&self, type_id: u32) -> Vec<u32> {
        let mut seen = std::collections::BTreeSet::new();
        for w in &self.wads {
            for e in &w.archive.aset {
                if e.type_id == type_id {
                    seen.insert(e.asset_hash);
                }
            }
        }
        seen.into_iter().collect()
    }

    /// The raw `(packed_block_ref, secondary_ref)` of an asset's PRIMARY row, last-mounted-wins.
    ///
    /// Exposed raw because those two words encode the asset's whole **LOD chain** — up to four
    /// rungs, not one block (`docs/aset_format.md`, proven 2026-07-21). Callers that need to know
    /// whether an asset is single-block must inspect both halves; see
    /// [`crate::lint::aset_row_is_single_block`].
    /// A whole decompressed block, located by a substring of its PTHS path — the way
    /// `wad_builder build-skin` finds `scripts_vz`, because there is no index constant to rely on
    /// and the path string is what actually identifies it.
    ///
    /// Reverse walk, so an overlay's block shadows the base's.
    pub fn block_by_path(&mut self, needle: &str) -> Option<Vec<u8>> {
        let needle = needle.to_lowercase();
        for wad in self.wads.iter_mut().rev() {
            let Some(idx) = wad
                .archive
                .paths
                .iter()
                .position(|p| p.to_lowercase().contains(&needle))
            else {
                continue;
            };
            if let Ok(dec) =
                mercs2_formats::sges::decompress_block(&mut wad.file, &wad.archive.indx, idx as u16)
            {
                return Some(dec);
            }
        }
        None
    }

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

    /// A block located by PTHS substring, plus **the rows that block itself publishes**, keyed by
    /// asset hash: `hash -> (packed_block_ref, secondary_ref, type_id)`.
    ///
    /// For republishing a block whole. Its assets have to keep the rows they already had, and both
    /// halves of "which row" matter:
    ///
    /// - **`type_id` decides which loader is dispatched**, so it must come from the WAD, not from a
    ///   type-hash lookup table — `docs/type_hash_registry.md` and `aset_type_ids` are wrong for 12
    ///   of 36 ids (`0xC122545A` is id 8, not 26).
    /// - The row must be **the one naming THIS block**. An asset hash can appear in rows belonging
    ///   to several blocks, so picking any row for the hash can hand back another block's type
    ///   entirely. ⚠ Nor can it be `AsetEntry::is_primary()`: that means "has no `_P001` rung", so
    ///   it silently skips every asset that *does* stream — which is precisely the set whose rows
    ///   most need preserving.
    ///
    /// Rows come from the same WAD the block came from, so an overlay cannot mix its block with a
    /// different WAD's rows.
    pub fn block_and_rows_by_path(
        &mut self,
        needle: &str,
    ) -> Option<(Vec<u8>, std::collections::HashMap<u32, (u32, u32, u32)>)> {
        let needle = needle.to_lowercase();
        for wad in self.wads.iter_mut().rev() {
            let Some(idx) = wad
                .archive
                .paths
                .iter()
                .position(|p| p.to_lowercase().contains(&needle))
            else {
                continue;
            };
            let Ok(dec) =
                mercs2_formats::sges::decompress_block(&mut wad.file, &wad.archive.indx, idx as u16)
            else {
                continue;
            };
            let rows = wad
                .archive
                .aset
                .iter()
                .filter(|e| e.block_index() as usize == idx)
                .map(|e| {
                    (
                        e.asset_hash,
                        (e.packed_block_ref, e.secondary_ref, e.type_id),
                    )
                })
                .collect();
            return Some((dec, rows));
        }
        None
    }

    /// Everything an in-place LAYER edit (`edit_world` — a vz_state / layers_static placement patch)
    /// needs to re-emit the whole layer block as an overlay that shadows the base by PTHS path.
    ///
    /// Unlike a model, a placement layer is edited AS A WHOLE block (one block holds the layer's COMP
    /// sub-blocks), so there is no minimal-container trick — the overlay carries the edited block at
    /// the base's own path, with its ASET rows restated. Returns the decompressed block, its PTHS
    /// path, its archive block index (for the rung remap), and its ASET rows.
    pub fn layer_block_for_edit(&mut self, needle: &str) -> Option<LayerEditInputs> {
        let needle = needle.to_lowercase();
        for wad in self.wads.iter_mut().rev() {
            let Some(idx) = wad.archive.paths.iter().position(|p| p.to_lowercase().contains(&needle))
            else {
                continue;
            };
            let Ok(dec) =
                mercs2_formats::sges::decompress_block(&mut wad.file, &wad.archive.indx, idx as u16)
            else {
                continue;
            };
            let rows = wad
                .archive
                .aset
                .iter()
                .filter(|e| e.block_index() as usize == idx)
                .map(|e| AsetRow {
                    asset_hash: e.asset_hash,
                    packed_block_ref: e.packed_block_ref,
                    secondary_ref: e.secondary_ref,
                    type_id: e.type_id,
                })
                .collect();
            return Some(LayerEditInputs {
                block: dec,
                path: wad.archive.paths[idx].clone(),
                block_index: idx as u32,
                rows,
            });
        }
        None
    }

    /// Everything an in-place MODEL-container edit (`edit_state_machine`) needs to re-emit the model
    /// as its OWN minimal block without shadowing its block-mates.
    ///
    /// A model container is a leaf in a block that usually holds many other models, and its ASET row
    /// packs a LOD chain of block indices. To edit just this one model we emit a single-entry block
    /// carrying only its (edited) container, copy its ASET row verbatim, and record the source block
    /// index. [`build_patch_wad_multi`](mercs2_formats::patch_wad::build_patch_wad_multi) then
    /// rewrites `_P000` to the new block and remaps or sentinels the finer rungs — a sentinel
    /// degrades that model to its coarse tier, it does not dangle — so no block-mate is carried and
    /// the chain never hangs. `field_c` from the original entry is preserved because the loader reads
    /// it and a guessed value is a needless risk.
    pub fn model_container_for_edit(&mut self, name_hash: u32) -> Option<ModelEditInputs> {
        use mercs2_formats::types::{TYPE_HASH_MODEL, TYPE_ID_MODEL};
        for wad in self.wads.iter_mut().rev() {
            let Some(row) = wad
                .archive
                .aset
                .iter()
                .find(|e| e.asset_hash == name_hash && e.type_id == TYPE_ID_MODEL && e.is_primary())
            else {
                continue;
            };
            let (secondary_ref, packed_block_ref) = (row.secondary_ref, row.packed_block_ref);
            let block_index = row.block_index();
            let Ok(dec) = mercs2_formats::sges::decompress_block(
                &mut wad.file,
                &wad.archive.indx,
                block_index,
            ) else {
                continue;
            };
            // Walk the raw entry table so `field_c` and the exact container bytes come through — the
            // container is spliced back verbatim, so anything the higher-level walker normalises away
            // would corrupt it.
            let (_n, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
            let mut pos = 4 + entries.len() * 16;
            for e in &entries {
                let end = (pos + e.chunk_size as usize).min(dec.len());
                if e.name_hash == name_hash && e.type_hash == TYPE_HASH_MODEL && pos < end {
                    return Some(ModelEditInputs {
                        container: dec[pos..end].to_vec(),
                        field_c: e.field_c,
                        secondary_ref,
                        packed_block_ref,
                        source_block_index: block_index as u32,
                    });
                }
                pos = end;
            }
        }
        None
    }

    /// The decompressed `UCFX` container for `(name_hash, type_id)`, last-mounted-wins.
    ///
    /// Locates the asset's block by its ASET row (block index = high 16 of `packed_block_ref`),
    /// decompresses it, and pulls the matching container out of the block's entry table. This is
    /// how `edit_stringdb` reads the base string table it edits — the string equivalent of
    /// [`Self::texture`], which reads a texture's dims for `replace_texture`.
    pub fn container_for_asset(&mut self, name_hash: u32, type_hash: u32, type_id: u32) -> Option<Vec<u8>> {
        for wad in self.wads.iter_mut().rev() {
            let Some(row) = wad
                .archive
                .aset
                .iter()
                .find(|e| e.asset_hash == name_hash && e.type_id == type_id)
            else {
                continue;
            };
            let block_idx = row.block_index();
            let Ok(dec) = mercs2_formats::sges::decompress_block(
                &mut wad.file,
                &wad.archive.indx,
                block_idx,
            ) else {
                continue;
            };
            let parsed = mercs2_formats::ucfx::walk_decompressed_block(&dec, "stringdb").0;
            if let Some(c) =
                mercs2_formats::ucfx::get_container_by_type_hash(&parsed, type_hash, Some(name_hash))
            {
                return Some(c);
            }
        }
        None
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
        std::fs::write(
            dir.join(LOCAL_CONFIG),
            format!("vz_wad = \"{}\"\n", wad.display()),
        )
        .unwrap();
        assert_eq!(read_local_config(&dir.join(LOCAL_CONFIG)), Some(wad));

        // `~/` is expanded so the file can be written by hand.
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(shellexpand_home("~/a/b"), format!("{home}/a/b"));
        assert_eq!(shellexpand_home("/abs/path"), "/abs/path");
    }

    /// A NATIVE Windows path in a TOML basic string is not valid TOML — `\U`, `\m`, `\v` are not
    /// escapes — so strict parsing dropped the whole document and the config silently did nothing.
    /// Both spellings must resolve, because both are things a user actually writes.
    #[test]
    fn a_windows_path_with_backslashes_still_resolves() {
        let dir = scratch("winpath");
        let cfg = dir.join(LOCAL_CONFIG);

        // The form `wad.display()` produces on Windows, and that a user typing a path produces.
        std::fs::write(&cfg, "vz_wad = \"C:\\Users\\me\\Mercenaries 2\\data\\vz.wad\"\n").unwrap();
        assert_eq!(
            read_local_config(&cfg),
            Some(PathBuf::from("C:\\Users\\me\\Mercenaries 2\\data\\vz.wad")),
            "a backslashed path must survive, not be dropped as a TOML parse failure"
        );

        // A TOML literal string needs no escaping and must keep working through the strict path.
        std::fs::write(&cfg, "vz_wad = 'C:\\Users\\me\\data\\vz.wad'\n").unwrap();
        assert_eq!(
            read_local_config(&cfg),
            Some(PathBuf::from("C:\\Users\\me\\data\\vz.wad"))
        );

        // Forward slashes — what `scripts/find-vz-wad.sh` writes — parse strictly as before.
        std::fs::write(&cfg, "# comment\nvz_wad = \"C:/Games/Mercs2/data/vz.wad\"\n").unwrap();
        assert_eq!(
            read_local_config(&cfg),
            Some(PathBuf::from("C:/Games/Mercs2/data/vz.wad"))
        );

        // A config with no key at all stays `None` — absence is still absence.
        std::fs::write(&cfg, "# nothing here\n").unwrap();
        assert_eq!(read_local_config(&cfg), None);
    }

    /// The config is found by walking UP, so a test running from a subdirectory still sees the
    /// repo-root file.
    #[test]
    fn the_config_is_found_from_a_subdirectory() {
        let dir = scratch("walkup");
        let wad = dir.join("vz.wad");
        std::fs::write(&wad, b"x").unwrap();
        std::fs::write(
            dir.join(LOCAL_CONFIG),
            format!("vz_wad = \"{}\"\n", wad.display()),
        )
        .unwrap();
        let deep = dir.join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();

        // Only meaningful when NO env override is set — every one of them outranks the config by design.
        if !env_override_active() {
            let found = discover_from(&deep).expect("should walk up to the config");
            assert_eq!(found.origin, Origin::LocalConfig);
            assert_eq!(found.path, wad);
        }
    }

    #[test]
    fn a_config_pointing_at_a_missing_file_is_ignored_rather_than_trusted() {
        let dir = scratch("missing");
        std::fs::write(dir.join(LOCAL_CONFIG), "vz_wad = \"/nope/vz.wad\"\n").unwrap();
        assert_eq!(
            read_local_config(&dir.join(LOCAL_CONFIG)),
            Some(PathBuf::from("/nope/vz.wad"))
        );
        if !env_override_active() {
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
        let dir = found
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
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
        let dir = found
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
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
