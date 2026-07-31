//! The builder: lint-gate → lower → assemble one overlay WAD → emit, verified by hash.
//!
//! Two mandates shape this module.
//!
//! **Gated on EXIT CODE, never a printed count.** [`build`] returns `Err(BuildError::Blocked)` when
//! the linter finds anything at `Error` or above; a caller cannot accidentally ship by ignoring
//! stdout.
//!
//! **Verified by hash** (`verify-artifacts-by-hash-not-size-mtime`). Every artifact carries its
//! sha256 in the [`Placement`] record, which is also what makes a file drop reversible — a WAD
//! overlay is undone by deleting one file, but an `.asi` in the game folder is not backable-out
//! unless something wrote down what was placed and where.
//!
//! ## Lowering status
//!
//! Every kind lowers here except `edit_state_machine`.
//!
//! `raw` is the open lower bound: opaque bytes plus an author-DECLARED blast radius. It is the one
//! kind with no encoder behind it, so the lowering checks everything structural it can and refuses
//! rather than warns — and it requires the declared `touches` to match the payload's own entry
//! table exactly, because that declaration is the only thing that can mint the ASET rows.
//!
//! `add_outfit` is the composed case, and the reason [`Lowering`] has more than one outcome: a Data
//! half (the model, injected into a hero-rigged donor) that lowers immediately, and a Script half
//! that cannot, because Lua links across the installed set rather than per Shipment. Linking a
//! Shipment's own mutations here keeps its overlay valid **standalone**; the cross-Shipment relink
//! is deploy's job, and skipping it is what lets one script mod overwrite another's Lua.
//!
//! `add_movie` is the only kind that needs NO game stack: a Scaleform movie is self-contained, so
//! unlike a texture (whose dimensions come from the target) or a model (whose rig comes from a
//! donor) there is nothing to read out of retail. It therefore lowers in template CI as well.
//!
//! `native_hook` and `place_file` are the kinds that produce no WAD content at all — a file placed
//! in the game folder, plus the [`Placement`] record that makes the drop reversible. `native_hook`
//! places the `.asi` and chooses its directory outright; `place_file` places the companions that
//! `.asi` reads, and lets the author pick a destination NAME from a closed set
//! ([`crate::manifest::PlaceIn`]) rather than write a path. Neither can be pointed at the game
//! executable or a WAD, and neither needs a game stack, so both lower in template CI.
//!
//! `edit_state_machine` returns `Unsupported`, and is expected to keep doing so for a while: the
//! destruction machine can be read and cannot be written, and three of the four things blocking it
//! live outside this crate. The reason it returns says which, so the refusal is actionable rather
//! than a deferral — and it points the author at `raw`, which can carry a hand-built block today
//! with a declared blast radius. A kind that returns `Unsupported` with a reason is honest; one
//! that is quietly skipped produces a WAD that looks fine and does nothing.

use crate::discover::LoadedShipment;
use crate::game::{GameStack, Platform};
use crate::link::{self, ScriptMutation};
use crate::lint::{self, Diagnostic};
use crate::manifest::{Contribution, Layer};
use crate::names::NameTable;
use mercs2_formats::donor;
use mercs2_formats::mesh_import;
use mercs2_formats::model_inject::inject_static_into_donor_block;
use mercs2_formats::{char_import, char_lower};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::texture::{build_texture_block, TexFormat, TextureData};
use mercs2_formats::texture_encode::{self, encode_bc1, encode_bc3, mip_chain};
use mercs2_formats::types::{TYPE_ID_CFX_PACK, TYPE_ID_MODEL, TYPE_ID_SCRIPT, TYPE_ID_TEXTURE};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One scripts block read out of the game stack, with the ASET rows it publishes.
struct LoadedScriptBlock {
    /// The block's own PTHS path, carried through to the emitted `PatchBlock`.
    path: String,
    block: ScriptsBlock,
    /// `asset_hash -> (packed_block_ref, secondary_ref, type_id)`, as the base WAD had them.
    rows: std::collections::HashMap<u32, (u32, u32, u32)>,
}

/// Load every scripts block a `patch_lua` target could live in.
///
/// A block missing from the stack is **skipped, not fatal**. A synthetic or overlay-only stack may
/// carry `scripts_vz` and nothing else, and if a mutation actually needed the absent block the
/// linker already reports `UnknownScript` naming the target — which tells the author what to fix,
/// where "no resident block in the game stack" would not.
fn load_script_blocks(
    game: &mut GameStack,
    kind: &'static str,
) -> Result<Vec<LoadedScriptBlock>, BuildError> {
    let mut out = Vec::new();
    for (needle, path) in link::SCRIPT_BLOCKS {
        let Some((raw, rows)) = game.block_and_rows_by_path(needle) else {
            continue;
        };
        let block = ScriptsBlock::parse(&raw).map_err(|m| BuildError::Lower {
            index: 0,
            kind,
            message: format!("parsing {path}: {m}"),
        })?;
        out.push(LoadedScriptBlock {
            path: (*path).to_string(),
            block,
            rows,
        });
    }
    if out.is_empty() {
        return Err(BuildError::Lower {
            index: 0,
            kind,
            message: "no scripts block in the configured game stack".into(),
        });
    }
    Ok(out)
}

/// Emit a `PatchBlock` for each scripts block the link actually spliced.
///
/// **Only the touched blocks.** Re-emitting an untouched block would shadow the base with a
/// byte-identical copy — harmless in isolation, but it puts the whole ~7,000-entry resident block
/// into every overlay that patches one `vz` script, and makes the overlay's contents stop meaning
/// "what this Shipment changed".
///
/// ★ **A row for EVERY entry the block carries, taken from the base WAD.** Not just the scripts:
/// an asset present in a block with no ASET row naming it in the same WAD is the **M0004 HANG** —
/// nothing can resolve it by hash, and the world load stops completing with no error. Retail never
/// ships that shape; all 30,006 asset hashes in `vz.wad`'s blocks have a row.
///
/// The rows are **copied from the block's own rows in the base WAD** rather than synthesised,
/// because `type_id` selects which loader is dispatched and the type-hash→id tables are known wrong
/// for 12 of 36 ids. For `scripts_vz` this is a no-op restatement (114 script rows); for the
/// resident block it preserves ~6,800 rows this code has no business inventing — get one wrong and
/// the validator reports the container as unreadable by the loader it was handed to.
///
/// A hash with no row in the base falls back to a sentinel script row. That should not happen for a
/// block read out of the stack — retail gives every carried asset a row — and a row that exists
/// beats the M0004 hang of no row at all.
fn script_patch_blocks(
    loaded: &[LoadedScriptBlock],
    linked: &[link::LinkedScript],
    kind: &'static str,
) -> Result<Vec<PatchBlock>, BuildError> {
    let touched: std::collections::BTreeSet<usize> = linked.iter().map(|l| l.block).collect();
    let mut out = Vec::new();
    for bi in touched {
        let lb = &loaded[bi];
        let aset: Vec<AsetEntry> = lb
            .block
            .entries
            .iter()
            .map(|e| match lb.rows.get(&e.name_hash) {
                Some(&(packed, secondary, type_id)) => {
                    AsetEntry::new(e.name_hash, secondary, packed, type_id)
                }
                None => AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_SCRIPT),
            })
            .collect();
        out.push(
            PatchBlock::from_decompressed(&lb.block.serialize(), lb.path.clone(), aset, None)
                .map_err(|m| BuildError::Lower {
                    index: 0,
                    kind,
                    message: m,
                })?,
        );
    }
    Ok(out)
}

/// Where a built artifact has to end up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Inside the Shipment's overlay WAD.
    Overlay,
    /// A file placed in the game folder, at this path relative to it — an `.asi` in the loader's
    /// search path, or a companion beside it.
    GameFolder { relative: String },
}

/// One emitted artifact and its digest.
///
/// For a [`Destination::GameFolder`] artifact, `relative` names the file BOTH under the build
/// directory and under the game folder — the output mirrors the tree it will be copied into, so a
/// deploy step never has to reconstruct one from the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The bare filename, for messages. The path is on the [`Destination`].
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub destination: Destination,
}

/// Where the Quartermaster puts an `.asi`, relative to the game folder.
///
/// **The author never names this**, which is the whole point: `native_hook` has no `dest` field, so
/// there is no spelling of a Shipment that writes next to `Mercenaries2.exe` or into `data\vz.wad`.
/// Those stay unreachable by construction rather than by a lint rule somebody could suppress.
///
/// `pmc_bb.dll` (v3.0.0, read directly: the format strings `%s*.asi`, `%sscripts\`, `%splugins\`,
/// `%supdate\`) globs four roots — the game directory itself and these three subfolders. `scripts\`
/// is chosen because it is where the ecosystem already puts them (`cruise.asi`, `dlc_enable.asi`)
/// and because keeping mod files out of the game root makes an uninstall obvious.
///
/// A forward slash on purpose: the loader's own literal is `scripts\`, but this string is a
/// filesystem path a deploy tool joins, not an engine path like the backslashed PTHS entries.
pub const ASI_SUBDIR: &str = "scripts";

/// The one `.asi` name the loader refuses to load: it skips its own.
///
/// Read from the binary, not assumed. A plugin shipped under this name would be placed correctly,
/// hash correctly, and never load — with the loader logging nothing at all, because it never
/// considered the file.
pub const RESERVED_ASI: &str = "pmc_bb.asi";

/// Join a game-folder directory and a filename into the relative path a deploy tool writes.
///
/// One function so the PLACEMENT and the [`crate::blast::Claim::FileArtifact`] that guards it can
/// never disagree — a claim computed one way and a path emitted another is a conflict system that
/// quietly stops matching. Forward slashes, and the game root is the empty string so joining is
/// uniform.
pub fn place_path(dir: &str, file: &str) -> String {
    if dir.is_empty() {
        file.to_string()
    } else {
        format!("{dir}/{file}")
    }
}

/// Extensions no Shipment may write into the game folder, whatever destination it names.
///
/// This is the second half of "the exe and the WADs are unreachable". [`crate::manifest::PlaceIn`]
/// takes the DESTINATION out of the author's hands; this takes the parts of the FILENAME that could
/// still clobber something load-bearing. Both halves are needed: `dest: game_root` is a legitimate
/// destination the loader really globs, and it is also where `Mercenaries2.exe` lives.
const FORBIDDEN_PLACEMENT_EXT: &[(&str, &str)] = &[
    (
        "wad",
        "a WAD is the base game's data (`data\\vz.wad`) or a Shipment's own overlay. The overlay is \
         emitted as `build/<name>.wad` and mounted by the deploy step — it is never placed by an \
         author, and the format cannot express a write into the base WAD at all",
    ),
    (
        "exe",
        "`Mercenaries2.exe` is the game. An exe edit stays unrepresentable rather than merely \
         linted, and that is only true if no file placement can write one",
    ),
    (
        "dll",
        "the DLLs in the game folder are the game's own, and `pmc_bb.dll` is the LOADER — Modkit \
         installs and manages it, and a Shipment never ships it (N Shipments carrying their own \
         copies would collide on one filename with no arbitration). The sanctioned way to add \
         native code is an `.asi` through `native_hook`",
    ),
];

/// Reject a filename that must not be written into the game folder, for any destination.
///
/// Everything here is about the NAME, because the name is the only part of a placement an author
/// influences — it comes from the source file, so `src/../..` is already an M0111 error before this
/// runs. What is left is a filename that is not a single path component (a deploy tool joining
/// `scripts/` + `..\..\Mercenaries2.exe`, or + `C:\evil`, or + `\\host\share\x` escapes the game
/// folder on the Windows machine that consumes the record, even though every one of those is a
/// perfectly ordinary filename on the macOS machine that built it), and a name that would clobber
/// something load-bearing.
///
/// Applies to `native_hook` too. Its extension check is narrower — it REQUIRES `.asi` — but the
/// component and reserved-name rules are the same file-in-the-game-folder rules.
pub fn game_folder_name_refusal(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("it has no filename at all".into());
    }
    if name == "." || name == ".." {
        return Some(format!(
            "{name:?} names a directory, not a file. A placement is a single file written into the \
             game folder"
        ));
    }
    if let Some(bad) = name.chars().find(|c| matches!(c, '/' | '\\' | ':')) {
        return Some(format!(
            "it contains {bad:?}, so it is not a single filename. A deploy tool joins this onto a \
             game-folder directory ON WINDOWS, where a separator, a drive letter (`C:\\…`) or a UNC \
             prefix (`\\\\host\\share`) would leave the game folder entirely — while on the \
             machine that built the Shipment all three are ordinary characters in a filename"
        ));
    }
    if name.eq_ignore_ascii_case(RESERVED_ASI) {
        return Some(format!(
            "{RESERVED_ASI} is reserved: the loader skips its own name, so a file shipped under it \
             is never loaded and nothing is logged, because the file is never considered"
        ));
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    if let Some(ext) = ext {
        for (denied, why) in FORBIDDEN_PLACEMENT_EXT {
            if ext == *denied {
                return Some(format!("it is a `.{denied}`, and {why}"));
            }
        }
    }
    None
}

/// Reject a filename a `place_file` may not use: everything [`game_folder_name_refusal`] rejects,
/// plus an `.asi`.
///
/// An `.asi` is a plugin, not a companion. Refusing it here rather than quietly placing it is what
/// stops `place_file` being a way around `native_hook`: that kind reads the PE headers
/// `LoadLibrary` will reject, refuses the loader's reserved name, and records the addresses the
/// plugin hooks so two plugins fighting over one is a conflict rather than whichever the filesystem
/// enumerated first. A companion route that also produced a loadable `*.asi` would skip all three
/// and still yield a file the loader globs.
///
/// One function, called by both the linter (M0162, so template CI says so on the push) and the
/// lowering (so the refusal survives a rule being suppressed).
pub fn companion_name_refusal(name: &str) -> Option<String> {
    if name.to_ascii_lowercase().ends_with(".asi") {
        return Some(
            "it is an `.asi`, which the loader globs and loads as native code. Ship it as a \
             `native_hook` instead: that reads the PE headers `LoadLibrary` will reject, refuses \
             the loader's own reserved name, and records the addresses it hooks so two plugins \
             fighting over one is a conflict rather than whichever the filesystem enumerated first"
                .into(),
        );
    }
    game_folder_name_refusal(name)
}

/// `IMAGE_FILE_MACHINE_I386`. The game is a 32-bit process, so a 64-bit plugin cannot load into it.
const PE_MACHINE_I386: u16 = 0x014C;
/// `IMAGE_FILE_DLL`. The loader calls `LoadLibrary`, which will not run an executable image.
const PE_CHARACTERISTICS_DLL: u16 = 0x2000;

/// What lowering one contribution produced.
///
/// Three outcomes rather than `Option<PatchBlock>`, because the Code layer genuinely does not
/// produce a block: an `.asi` is a file in the game folder, and a format that could only express
/// WAD content could not describe our own live bridge.
enum Lowering {
    /// Nothing to emit here. The contribution's effect is realised elsewhere — a `patch_lua`
    /// declares a mutation that the linker applies later.
    Nothing,
    Block(PatchBlock),
    /// A file placed in the game folder. Carries its bytes so the caller writes them exactly once,
    /// next to the digest it records for them.
    File {
        name: String,
        relative: String,
        bytes: Vec<u8>,
    },
}

/// Reject a plugin the game's loader could not load, by reading its PE header.
///
/// Both failures are observable in `pmc_blackbox.log` as `[FAILED] … (error: …)` — which is rare
/// good news for this codebase — but only to a modder who knows to look. Neither is recoverable at
/// deploy time, and both are cheap to see here.
fn asi_load_blocker(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return Some("it is not a PE image at all (no `MZ` header)".into());
    }
    let pe_at = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    if pe_at + 24 > bytes.len() || &bytes[pe_at..pe_at + 4] != b"PE\0\0" {
        return Some("its `e_lfanew` does not point at a `PE\\0\\0` signature".into());
    }
    let coff = pe_at + 4;
    let machine = u16::from_le_bytes([bytes[coff], bytes[coff + 1]]);
    let characteristics = u16::from_le_bytes([bytes[coff + 18], bytes[coff + 19]]);
    if machine != PE_MACHINE_I386 {
        return Some(format!(
            "it is built for machine 0x{machine:04X}, not i386 (0x{PE_MACHINE_I386:04X}). \
             Mercenaries2.exe is a 32-bit process and `LoadLibrary` refuses a foreign architecture"
        ));
    }
    if characteristics & PE_CHARACTERISTICS_DLL == 0 {
        return Some(
            "its COFF characteristics do not set `IMAGE_FILE_DLL`, so it is an executable image \
             rather than a DLL. An `.asi` is a DLL with a different extension; the loader calls \
             `LoadLibrary` on it"
                .into(),
        );
    }
    None
}

/// Read the assembled WAD back and run [`crate::lint::artifact_checks`] on it.
///
/// Deliberately re-parses the bytes rather than checking the in-memory `Vec<PatchBlock>` that went
/// in. Those blocks have not been through `build_patch_wad_multi`'s LOD-rung remap, so their rungs
/// are still source-relative — checking them would answer the wrong question. More usefully, this
/// verifies what will actually be on disk, so a serializer bug is in scope too.
///
/// Fails the build on any blocking finding. That is the whole value: both structural bugs this
/// crate has shipped were invisible in the manifest and plain in the bytes.
fn verify_emitted(wad: &[u8]) -> Result<Vec<crate::lint::Diagnostic>, BuildError> {
    let contents =
        mercs2_formats::patch_wad::read_patch_wad(wad).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "verify",
            message: format!("the WAD we just wrote does not read back: {m}"),
        })?;
    let found = crate::lint::artifact_checks(&contents.blocks);
    if crate::lint::blocks_build(&found) {
        return Err(BuildError::Artifact { diagnostics: found });
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    /// Everything the linter said, including non-blocking warnings.
    pub diagnostics: Vec<Diagnostic>,
    /// The overlay WAD, when any contribution produced one.
    pub wad: Option<PathBuf>,
    /// Every artifact with its digest — the record deploy/undo consumes.
    pub placements: Vec<Placement>,
    pub log: Vec<String>,
}

#[derive(Debug)]
pub enum BuildError {
    /// The linter found something at `Error` or above.
    Blocked(Vec<Diagnostic>),
    /// A contribution needed the retail WADs and none were configured.
    GameRequired {
        index: usize,
        kind: &'static str,
    },
    /// The configured stack is a console bake and we cannot yet EMIT for one.
    ConsoleOutputUnsupported,
    /// A kind whose lowering is not implemented yet, with the reason.
    Unsupported {
        index: usize,
        kind: &'static str,
        reason: String,
    },
    Lower {
        index: usize,
        kind: &'static str,
        message: String,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    /// The WAD we just assembled failed its own self-check. Always a builder bug rather than an
    /// author one, and fatal on purpose: the whole point of a HANG-class rule is that the game will
    /// not tell anybody what went wrong.
    Artifact {
        diagnostics: Vec<crate::lint::Diagnostic>,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Blocked(d) => {
                writeln!(f, "build blocked by {} finding(s):", d.len())?;
                for x in d.iter().filter(|x| x.severity >= lint::Severity::Error) {
                    writeln!(f, "  {x}")?;
                }
                Ok(())
            }
            BuildError::GameRequired { index, kind } => write!(
                f,
                "contributions[{index}] ({kind}) needs the retail WADs — configure the game folder \
                 (Workshop Settings, or `qm --game <dir>`). `qm lint` runs without one."
            ),
            BuildError::ConsoleOutputUnsupported => write!(
                f,
                "the configured game stack is a CONSOLE bake (Xbox 360 / PS3, big-endian `SCFF`), \
                 and we cannot emit for one yet. Note this is not merely an endianness flip: \
                 `ucfx_byteswap` converts console → PC only, and the reverse needs GPU texture \
                 RE-tiling, XMA/Xbox-ADPCM audio encoding, big-endian Lua bytecode and Xbox vertex \
                 declarations. Reading a console WAD is supported; writing one is not."
            ),
            BuildError::Unsupported {
                index,
                kind,
                reason,
            } => {
                write!(
                    f,
                    "contributions[{index}] ({kind}) cannot be lowered yet: {reason}"
                )
            }
            BuildError::Lower {
                index,
                kind,
                message,
            } => {
                write!(f, "contributions[{index}] ({kind}): {message}")
            }
            BuildError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            BuildError::Artifact { diagnostics } => {
                write!(f, "the assembled WAD failed its self-check:")?;
                for d in diagnostics {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for BuildError {}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Which drawing group of the donor hosts the injected geometry.
///
/// ⚠ The manifest has no field for this — the Workshop's UI lets an author pick, but `add_model`
/// carries only name/model/donor. Group 0 is the common case for a simple prop; a donor whose
/// interesting geometry sits in a later group cannot currently be targeted. Recorded rather than
/// papered over: it likely wants a `group:` field, which is a format change.
const DEFAULT_TARGET_GROUP: usize = 0;

/// Every script mutation a Shipment declares, derived from its manifest alone.
///
/// **Hermetic on purpose — no game stack, no lowering.** That is what lets the same function serve
/// two callers that are otherwise very different: [`build`], linking one Shipment so its overlay is
/// valid standalone, and [`link_installed`], linking N Shipments at deploy so none of them
/// overwrites another. If mutations could only be obtained as a side effect of lowering, the deploy
/// path would have to re-run model injection just to find out which scripts are touched.
pub fn script_mutations(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<ScriptMutation>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::AddOutfit {
                name,
                slug,
                display,
                wearer,
                ..
            } => {
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: "wifpmcinterior".into(),
                    append: link::outfit_row_append(wearer, slug, name, display),
                });
            }
            Contribution::PatchLua { target, append } => {
                let path = root.join(append);
                let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                    index,
                    kind: "patch_lua",
                    message: format!("reading {}: {e}", path.display()),
                })?;
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: target.clone(),
                    append: source,
                });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Lower a rigged `.glb` onto a donor — the SKINNED path, shared by `add_model` and `add_outfit`.
///
/// This used to be a flat refusal (`BuildError::Unsupported`), because everything it needs lived in
/// binary crates: the skinned glTF reader in the Workshop, a second copy in `mercs2_poc`, and the
/// bone mapper alongside them. All three are library code now, so a Shipment can finally ship a
/// character instead of being told the format supports one in principle.
fn lower_skinned(
    index: usize,
    kind: &'static str,
    name: &str,
    model: &Path,
    donor_name: &str,
    retarget: &crate::manifest::Retarget,
    root: &Path,
    game: &mut GameStack,
    names: Option<&NameTable>,
    log: &mut Vec<String>,
) -> Result<Vec<u8>, BuildError> {
    let lower_err = |m: String| BuildError::Lower {
        index,
        kind,
        message: m,
    };

    let donor_hash = crate::manifest::asset_hash(donor_name);
    let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
    let donor_blk = donor::donor_block(&paths, donor_hash).map_err(lower_err)?;

    let glb = char_import::load_char_glb(&root.join(model)).map_err(lower_err)?;

    // Names of the source rig's joints, in palette order — what both the convention detector and
    // the `bones:` map key on.
    let source_names: Vec<String> = glb
        .joint_nodes
        .iter()
        .map(|&n| glb.node_name.get(n).cloned().unwrap_or_default())
        .collect();

    let detected = mercs2_formats::retarget::SourceRig::detect(&source_names);
    // Compare against the canonical slug, not a substring of the prose label. `from: cod` is the
    // documented spelling and `"call of duty (iw-engine)"` does not contain "cod", so this warned
    // on every correctly-authored CoD manifest — and, being a substring test, `from: a` matched
    // everything. A warning that fires on correct input teaches authors to ignore warnings.
    let declared = retarget.from.trim().to_ascii_lowercase();
    if !declared.is_empty() && declared != detected.slug() {
        log.push(format!(
            "contributions[{index}] {kind} {name}: manifest says `from: {}` but the bone names read \
             as `{}` ({}) — building from the names in the file",
            retarget.from,
            detected.slug(),
            detected.label()
        ));
    }

    // The RESOLVED map wins when the Shipment carries one. Resolve target bone NAMES onto this
    // donor's own HIER indices by name hash, so a map authored against one donor still lands
    // correctly on another that orders its bones differently.
    let skel = mercs2_formats::skeleton::Skeleton::from_block(&donor_blk)
        .map_err(|e| lower_err(format!("donor has no readable HIER skeleton: {e}")))?;
    let hier_of_name: std::collections::HashMap<u32, u32> = skel
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name_hash, i as u32))
        .collect();

    let mut overrides: std::collections::HashMap<usize, Option<u32>> =
        std::collections::HashMap::new();
    let mut unresolved: Vec<String> = Vec::new();
    if let Some(map) = &retarget.bones {
        for (src, tgt) in map {
            let Some(si) = source_names.iter().position(|n| n == src) else {
                unresolved.push(format!("{src} (not a bone in the model)"));
                continue;
            };
            match tgt {
                None => {
                    overrides.insert(si, None); // explicit drop
                }
                Some(t) => {
                    let h = mercs2_formats::hash::pandemic_hash_m2(t);
                    match hier_of_name.get(&h) {
                        Some(&hier) => {
                            overrides.insert(si, Some(hier));
                        }
                        None => unresolved.push(format!("{src} -> {t} (donor has no such bone)")),
                    }
                }
            }
        }
        if !unresolved.is_empty() {
            return Err(lower_err(format!(
                "`retarget.bones` does not fit this pairing: {}. The map is the reproducible record \
                 of a remap, so a stale entry is an error rather than something to skip.",
                unresolved.join("; ")
            )));
        }
        log.push(format!(
            "contributions[{index}] {kind} {name}: applied {} explicit bone mappings",
            overrides.len()
        ));
    } else {
        // No explicit map — so derive the SAME one the Workshop's preview derives.
        //
        // This used to leave `overrides` empty, which meant `build_character` fell through to the
        // generic `char_skin::automap` no matter what the source rig was. The convention tables had
        // been lifted into the library one commit earlier precisely so this call site could reach
        // them, and it never called them. Measured on a ValveBiped source against
        // `pmc_hum_mattias`: 21 of 45 source joints landed differently, including the whole spine
        // ladder and all 18 finger bones folded onto the hands. The author previewed the corrected
        // conform and shipped the uncorrected one, with only a warning to say so.
        //
        // Bone NAMES are load-bearing here: the tables resolve targets by name, and a HIER-derived
        // skeleton is hash-named, against which every table entry misses and `mapped_count()` is 0.
        // Hence `from_skeleton_with_names` over the curated table.
        let target = mercs2_formats::char_skin::TargetSkeleton::from_skeleton_with_names(
            &skel,
            |h| names.and_then(|n| n.reverse(h)).map(|s| s.to_string()),
        );
        let named = target
            .bones
            .iter()
            .filter(|b| !b.name.starts_with("hash_"))
            .count();
        let target_names: Vec<String> = target.bones.iter().map(|b| b.name.clone()).collect();
        let target_pos: Vec<[f32; 3]> = target
            .bones
            .iter()
            .map(|b| [b.pos[0] as f32, b.pos[1] as f32, b.pos[2] as f32])
            .collect();
        // Source bind positions, index-aligned to `source_names`. `Retarget::align_by_position`
        // needs them, and `node_world` is row-major with the translation at [3]/[7]/[11].
        let source_pos: Vec<[f32; 3]> = glb
            .joint_nodes
            .iter()
            .map(|&n| {
                glb.node_world
                    .get(n)
                    .map(|m| [m[3] as f32, m[7] as f32, m[11] as f32])
                    .unwrap_or([0.0, 0.0, 0.0])
            })
            .collect();
        let rt = mercs2_formats::retarget::Retarget::build_with_pos(
            source_names.clone(),
            source_pos,
            target_names,
            target_pos,
        );
        overrides = rt.convention_overrides(target.bones.len());
        log.push(format!(
            "contributions[{index}] {kind} {name}: {} bone map from the {} table \
             ({named}/{} donor bones named, {} source joints mapped)",
            if overrides.is_empty() { "generic automap" } else { "convention" },
            rt.convention.slug(),
            target.bones.len(),
            overrides.len()
        ));
        if named * 2 < target.bones.len() {
            log.push(format!(
                "contributions[{index}] {kind} {name}: WARNING — only {named} of {} donor bones \
                 could be named, so the convention tables have little to match against. The bone \
                 map will be closer to the generic automap than to the Workshop's preview.",
                target.bones.len()
            ));
        }
    }

    let opts = char_lower::LowerOpts {
        overrides,
        // A model already on the game's own rig keeps the author's weights: there is no fuzzy map
        // to repair, and resampling would discard everything painted on new geometry.
        native_rig: detected == mercs2_formats::retarget::SourceRig::Pandemic,
        repoints: Vec::new(),
    };

    let hash = crate::manifest::asset_hash(name);
    let out = char_lower::character_into_donor(&donor_blk, &glb, hash, &opts).map_err(lower_err)?;

    // Report every host, not `hosts[0]`. A two-host build logging "group 3" said nothing about
    // group 7 also being rewritten and the other 26 neutralised.
    log.push(format!(
        "contributions[{index}] {kind} {name} 0x{hash:08X} ← donor {donor_name} groups {:?}: \
         {} verts, {} tris, {} bones / {} palette slots | {}",
        out.hosts,
        out.stats.vertex_count,
        out.stats.triangle_count,
        out.skin.stats.bones,
        out.skin.palette_slots,
        out.transfer
    ));
    // `CharSkin::warnings` was populated all along and read by nothing on this path — including the
    // one that says an extremity will be stranded in space. A warning nobody prints was not issued.
    for w in &out.warnings {
        log.push(format!("contributions[{index}] {kind} {name}: WARNING — {w}"));
    }
    use mercs2_formats::char_skin::validate::Status;
    for c in out.report.checks.iter().filter(|c| c.status != Status::Ok) {
        log.push(format!(
            "contributions[{index}] {kind} {name}: {} = {:?}",
            c.title, c.status
        ));
    }

    Ok(out.block)
}

/// Lower a single contribution into a patch block.
fn lower(
    index: usize,
    contribution: &Contribution,
    root: &Path,
    game: Option<&mut GameStack>,
    // Host-provided, like the game stack — the crate never reaches into the filesystem for it.
    // Load-bearing for the skinned path: without bone NAMES the retarget correction tables have
    // nothing to match against and every build silently falls back to the generic automap.
    names: Option<&NameTable>,
    log: &mut Vec<String>,
) -> Result<Lowering, BuildError> {
    let kind = contribution.kind();
    match contribution {
        Contribution::ReplaceTexture { target, image } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);

            // The target's OWN dimensions and format are the spec: a replacement is same-hash and
            // fully resident, so it must match what the engine already expects to read.
            let existing = game.texture(hash).ok_or_else(|| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "{target:?} (0x{hash:08X}) is not in the configured game stack — check the \
                     spelling; a name that does not exist hashes to a lookup that simply misses"
                ),
            })?;

            let (w, h) = (existing.width as usize, existing.height as usize);
            let rgba = read_png_rgba(&root.join(image)).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            if rgba.width != w || rgba.height != h {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "image is {}x{} but {target} is {w}x{h}; a replacement is SAME-HASH and \
                         fully resident, so it must match the shipped dimensions exactly",
                        rgba.width, rgba.height
                    ),
                });
            }

            // Encode with the shipping encoder (`texture_encode`), not the workbench-preview one.
            let fourcc = *existing.format.fourcc();
            let body = match existing.format {
                TexFormat::Bc1 => {
                    let rgb = drop_alpha(&rgba.pixels);
                    mip_chain(w, h, 3, &rgb, encode_bc1)
                }
                TexFormat::Bc3 => mip_chain(w, h, 4, &rgba.pixels, encode_bc3),
            };
            // `build_texture_block` emits the UCFX container ALREADY WRAPPED in a single-entry
            // block table. A patch block is `[entry table][containers…]`, not a bare container —
            // handing over a raw container makes the loader read the `UCFX` magic as an entry-table
            // field. (Caught by `wad_simulator`, not by any digest check: the WAD hashed fine and
            // was structurally nonsense.)
            let mip_count = texture_encode::mip_count(w, h) as u32;
            let td = TextureData {
                width: existing.width,
                height: existing.height,
                format: existing.format,
                mip0: body[..mip0_len(w, h, existing.format).min(body.len())].to_vec(),
                all_mips: body,
                mip_count,
            };
            let block_bytes = build_texture_block(hash, &td);
            log.push(format!(
                "contributions[{index}] replace_texture {target} 0x{hash:08X} {w}x{h} {} \
                 {mip_count} mips → {} bytes",
                String::from_utf8_lossy(&fourcc),
                block_bytes.len()
            ));

            // Same hash, texture type, PRIMARY. The low 16 bits of `packed_block_ref` MUST be
            // 0xFFFF: that is what `is_primary()` tests, and any other value names a `_P001` LOD
            // block one level finer. A row pointing at a rung that does not exist is the
            // dangling-LOD-rung trap — a 549 GB buffer request and an open-world stream HANG.
            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_TEXTURE);
            // Path convention matches the proven publish pipeline
            // (`mercs2_workshop::publish`, docs/modernization/workshop_publish_pipeline.md):
            // `blocks\VZ\mod_<hash>.block`. It lands in PTHS; matching the shape that has actually
            // shipped working WADs costs nothing.
            let block = PatchBlock::from_decompressed(
                &block_bytes,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Lowering::Block(block))
        }

        Contribution::AddModel {
            name,
            model,
            donor,
            retarget,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // Resolved Q2 says `donor` may be omitted and auto-picked. Auto-pick is not written, so
            // this asks rather than guessing — a wrong host silently produces a prop with the wrong
            // rig and materials.
            let Some(donor_name) = donor else {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "donor auto-pick is not implemented yet — name a `donor:` explicitly. \
                             The donor supplies the rig, materials and state machine, so picking \
                             the wrong one fails quietly rather than loudly."
                        .into(),
                });
            };

            // SKINNED path. `retarget:` means the source carries a rig to be re-posed onto the
            // donor's, which needs char_skin's palette-relative BLENDINDICES and the matching
            // INFO(56) range table. Without it this stays the rigid lowering, which leaves joints
            // empty — correct for a prop, wrong for anything that animates.
            if let Some(rt) = retarget {
                let new_block = lower_skinned(
                    index, kind, name, model, donor_name, rt, root, game, names, log,
                )?;
                let hash = crate::manifest::asset_hash(name);
                let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
                let block = PatchBlock::from_decompressed(
                    &new_block,
                    format!("blocks\\VZ\\mod_{hash:08x}.block"),
                    vec![aset],
                    None,
                )
                .map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
                return Ok(Lowering::Block(block));
            }

            let donor_hash = crate::manifest::asset_hash(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;

            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = crate::manifest::asset_hash(name);
            // Flags mirror the workshop's proven call: auto-fit OFF (the mesh carries its own
            // transform), target the raw rendered group, neutralise the rest.
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &mesh,
                DEFAULT_TARGET_GROUP,
                &[],
                hash,
                false,
                false,
                false,
                false,
                &[DEFAULT_TARGET_GROUP],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            log.push(format!(
                "contributions[{index}] add_model {name} 0x{hash:08X} ← donor {donor_name} \
                 group {DEFAULT_TARGET_GROUP}: {} verts, {} tris",
                stats.vertex_count, stats.triangle_count
            ));

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Lowering::Block(block))
        }

        // `add_outfit` is a FIXED composition of add_model + a patch_lua on `_tOutfits`. The Data
        // half lowers here; the Script half is declared and linked later, because Lua is linked
        // across the installed set rather than per Shipment.
        // `slug`/`display` are the SCRIPT half's fields and are consumed by `script_mutations`;
        // only the Data half is lowered here.
        // `display` is the SCRIPT half's field and is consumed by `script_mutations`; only the Data
        // half is lowered here. `slug` is kept for the log line, which is how an author confirms the
        // wardrobe row they expected is the one that was generated.
        Contribution::AddOutfit {
            name,
            slug,
            wearer,
            model,
            donor,
            retarget,
            textures,
            ..
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // `textures:` is parsed and then used by nothing. It was originally dropped on the
            // floor by the `..` in this pattern; a first fix refused it only when `retarget` was
            // absent, on the stated grounds that "the skinned lowering performs the per-group MTRL
            // repoint" — which it does not. `lower_skinned` takes no textures argument and passes
            // `repoints: Vec::new()`. So the refusal steered authors toward `retarget:`, where the
            // silent drop was still waiting.
            //
            // Refuse on BOTH branches until the repoint path actually exists. An honest refusal is
            // worth more than a fix that only moves where the skin goes missing.
            if textures.diffuse.is_some()
                || textures.normal.is_some()
                || textures.specular.is_some()
            {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "`textures:` is not wired up yet, on either the rigid or the skinned \
                             path — the lowering passes no MTRL repoints, so an outfit built now \
                             wears the DONOR's materials whatever you put here. Refusing rather \
                             than shipping a silent substitution. Remove `textures:` to build the \
                             geometry alone."
                        .into(),
                });
            }
            let Some(donor_name) = donor else {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason:
                        "donor auto-pick is not implemented — name a `donor:` explicitly. For an \
                             outfit the donor must be a hero-rigged host, or the model will not \
                             animate."
                            .into(),
                });
            };

            // SKINNED path — an outfit that animates has to be re-posed onto the donor's rig.
            if let Some(rt) = retarget {
                let new_block =
                    lower_skinned(index, kind, name, model, donor_name, rt, root, game, names, log)?;
                let hash = crate::manifest::asset_hash(name);
                let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
                let block = PatchBlock::from_decompressed(
                    &new_block,
                    format!("blocks\\VZ\\mod_{hash:08x}.block"),
                    vec![aset],
                    None,
                )
                .map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
                log.push(format!(
                    "contributions[{index}] add_outfit {name}: wardrobe row {wearer}/{slug}"
                ));
                return Ok(Lowering::Block(block));
            }

            let donor_hash = crate::manifest::asset_hash(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = crate::manifest::asset_hash(name);
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &mesh,
                DEFAULT_TARGET_GROUP,
                &[],
                hash,
                false,
                false,
                false,
                false,
                &[DEFAULT_TARGET_GROUP],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;

            // The Script half. `Model` is the ASSET name SetOutfit receives; `Name` is the
            // unlock/tracking key; both are distinct from the display string.
            log.push(format!(
                "contributions[{index}] add_outfit {name} 0x{hash:08X} ← donor {donor_name}: \
                 {} verts, {} tris | wardrobe row {wearer}/{slug}",
                stats.vertex_count, stats.triangle_count
            ));
            Ok(Lowering::Block(block))
        }

        // A Scaleform GFx movie, added as a new `cfx_pack` asset.
        //
        // The one lowering here that needs NO game stack: `replace_texture` reads the target's
        // dimensions and `add_model` borrows a donor's rig, but a movie is self-contained — the
        // container holds the whole asset and nothing is conformed to anything. So this builds in
        // template CI, where the retail WADs will never exist.
        //
        // The movie is validated and then copied VERBATIM. `GfxMovie::parse` is the check that the
        // bytes are a movie at all; it is deliberately not followed by a re-encode, because retail
        // ships both compressed `CFX` (61 assets) and uncompressed `GFX` (3), so there is no
        // encoding to normalise TO, and swapping an author's verified bytes for ones nobody has run
        // is exactly the kind of helpfulness that produces a WAD that looks fine and does nothing.
        Contribution::AddMovie { name, movie } => {
            let path = root.join(movie);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            // Parse before wrapping. A container whose `data` leaf is not a movie still checksums,
            // still walks, and still resolves — the loader is the first thing that finds out, and it
            // reports `GFxLoader read failed` with no reference to which asset.
            let parsed =
                mercs2_formats::gfx::GfxMovie::parse(&bytes).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} is not a Scaleform movie this build can read: {m}. Expected a `.gfx` \
                         beginning with `GFX` or `CFX` (or the SWF spellings `FWS`/`CWS`) — retail \
                         ships 61 `CFX` and 3 `GFX`, so either is fine, but a project file, an \
                         already-wrapped container or a truncated export is not.",
                        path.display()
                    ),
                })?;

            let hash = crate::manifest::asset_hash(name);
            let block_bytes = mercs2_formats::gfx::build_cfx_pack_block(hash, &bytes);

            // The tag census is logged rather than merely counted: an emitter that silently dropped
            // the movie's content still produces a valid header, and "0 tags" in the log is the only
            // place that would show.
            let features = parsed.features();
            let [w, h] = parsed.stage_px();
            log.push(format!(
                "contributions[{index}] add_movie {name} 0x{hash:08X} ← {} \
                 {} v{} {}x{} px, {} tag(s): {} shape(s), {} sprite(s), {} button(s), \
                 {} edit-text, {} DoAction, {} import(s), {} GFx-ext → {} bytes",
                path.display(),
                String::from_utf8_lossy(&parsed.magic),
                parsed.version,
                w,
                h,
                parsed.tags.len(),
                features.shapes,
                features.sprites,
                features.buttons,
                features.edit_texts,
                features.do_action,
                features.imports,
                features.gfx_ext_tags,
                block_bytes.len()
            ));

            // ADDITIVE and PRIMARY. A movie has no LOD chain at all, so both rung halves stay at
            // their sentinels — `0x0000` in the low 16 is the dangling-rung HANG, not "no rung".
            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_CFX_PACK);
            let block = PatchBlock::from_decompressed(
                &block_bytes,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Lowering::Block(block))
        }

        // Contributes no block: its whole effect is a declared mutation, collected by
        // `script_mutations` and realised at link time.
        Contribution::PatchLua { .. } => Ok(Lowering::Nothing),

        // The OPEN LOWER BOUND: bytes we cannot interpret, plus a radius the author DECLARED.
        //
        // Every other kind has a second line of defence — an encoder that knows the shape, a donor
        // to conform to. `raw` has none, so everything structural that CAN be checked is checked
        // here, and a failure is a hard error rather than a warning. The declared `touches` is not
        // decoration either: it is the only thing that can mint the ASET rows, so it must agree
        // with the payload's own entry table exactly, in BOTH directions. A hash in `touches` that
        // the payload does not carry mints a row resolving to a block that does not contain it; a
        // hash the payload carries that `touches` omits is M0004's silent wedge, and it would also
        // mean the conflict system never saw the claim.
        Contribution::Raw {
            description,
            payload,
            target_layer,
            touches,
        } => {
            // The overlay is a WAD, and the Data layer is the only one a WAD holds. The other
            // three are refused by NAME rather than lowered into something plausible.
            match target_layer {
                Layer::Data => {}
                Layer::Script => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "a raw payload on the SCRIPT layer would ship a finished scripts_vz \
                             block. WAD resolution is last-mounted-wins, so it would silently \
                             delete every other installed Shipment's Lua — including the wardrobe \
                             rows `add_outfit` generates. That is the exact annihilation \
                             `patch_lua` exists to prevent by shipping a MUTATION instead of a \
                             block, and no declared blast radius can make it safe. Use `patch_lua`."
                                .into(),
                    });
                }
                Layer::Code => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "a raw payload on the CODE layer has nowhere to go: `raw` carries no \
                             destination field, and inventing one would hand the author a way to \
                             name Mercenaries2.exe or data/vz.wad — which is precisely what \
                             `native_hook` omitting `dest` keeps unreachable. Use `native_hook`, \
                             which places the file in the loader's search path for you."
                                .into(),
                    });
                }
                Layer::Runtime => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "the RUNTIME layer has no artifact. Nothing in the format says what a \
                             runtime payload is or where it would be placed, so there is no \
                             lowering to write — only a guess, and a guess here emits a WAD that \
                             looks fine and does nothing."
                                .into(),
                    });
                }
            }

            let path = root.join(payload);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            // Two shapes an author plausibly hands us that are NOT a block. Both are named
            // explicitly, because the generic "does not parse" message sends them looking in the
            // wrong place.
            if bytes.len() >= 4 && &bytes[0..4] == b"sges" {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} starts with the `sges` magic, so it is a COMPRESSED block. Supply the \
                         DECOMPRESSED bytes — the builder compresses and computes `packed_field` \
                         from their length, and a pre-compressed payload would be compressed twice \
                         while claiming the wrong decompressed page count.",
                        path.display()
                    ),
                });
            }
            if bytes.len() >= 4 && &bytes[0..4] == b"UCFX" {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} starts with `UCFX`, so it is a bare CONTAINER, not a block. A patch \
                         block is `[entry table][containers…]`: the loader reads the first word as \
                         an entry count, so it would read the `UCFX` magic as one. Prepend the \
                         table — `[u32 count][count × (name_hash, type_hash, field_c, chunk_size)]`.",
                        path.display()
                    ),
                });
            }

            // Coherence, in the same sense `lint::coherent_block` means it: the declared count must
            // be honoured and every container must fit. `parse_block_entry_table` reads the first
            // word as a count unconditionally, so anything else yields confident nonsense.
            let (parsed, issues) =
                mercs2_formats::ucfx::walk_decompressed_block(&bytes, "raw payload");
            if parsed.entry_count == 0
                || parsed.entries.len() != parsed.entry_count as usize
                || parsed.containers.len() != parsed.entries.len()
            {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} does not read as a patch block: its first word declares {} entr(ies) \
                         but only {} row(s) and {} container(s) fit in {} bytes. A block is \
                         `[u32 count][count × 16-byte rows][containers…]`.",
                        path.display(),
                        parsed.entry_count,
                        parsed.entries.len(),
                        parsed.containers.len(),
                        bytes.len()
                    ),
                });
            }
            if !issues.is_empty() {
                let detail: Vec<String> = issues
                    .iter()
                    .map(|i| format!("{}: {}", i.context, i.detail))
                    .collect();
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} is structurally invalid — {}. These are the checks the engine's own \
                         reader performs; a payload that fails them loads as garbage rather than \
                         failing loudly.",
                        path.display(),
                        detail.join("; ")
                    ),
                });
            }

            // `touches` are asset REFERENCES: a bare `0x…` is that hash, anything else is a name.
            let declared: std::collections::BTreeSet<u32> = touches
                .iter()
                .map(|t| crate::manifest::asset_hash(&t.0))
                .collect();
            let carried: std::collections::BTreeSet<u32> =
                parsed.entries.iter().map(|e| e.name_hash).collect();
            let hexes = |set: std::collections::BTreeSet<u32>| {
                set.iter()
                    .map(|h| format!("0x{h:08X}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let missing: std::collections::BTreeSet<u32> =
                declared.difference(&carried).copied().collect();
            if !missing.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "`touches` claims {} which the payload's entry table does not carry. The \
                         claim is what mints the ASET row, so this would publish a row pointing at \
                         a block that has no such asset in it — the lookup resolves, the block \
                         loads, and the asset is simply absent.",
                        hexes(missing)
                    ),
                });
            }
            let extra: std::collections::BTreeSet<u32> =
                carried.difference(&declared).copied().collect();
            if !extra.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "the payload carries {} which `touches` does not claim. Nothing else can \
                         infer a raw block's radius, so an unclaimed asset gets no ASET row (the \
                         M0004 silent wedge) and the conflict system never sees the claim at all — \
                         two Shipments could overwrite one asset without either being told.",
                        hexes(extra)
                    ),
                });
            }

            // The TYPE comes from the bytes, never from the author: the ASET row's type id decides
            // which loader the engine dispatches, and a guess there resolves the asset into the
            // wrong subsystem.
            let mut aset = Vec::new();
            for e in &parsed.entries {
                let type_id = mercs2_formats::types::type_id_for_type_hash(e.type_hash)
                    .ok_or_else(|| BuildError::Lower {
                        index,
                        kind,
                        message: format!(
                            "entry 0x{:08X} declares type hash 0x{:08X}, which is not one of the \
                             {} types the retail census found. The ASET row's type id is derived \
                             from it and decides which loader is dispatched, so there is nothing \
                             safe to guess.",
                            e.name_hash,
                            e.type_hash,
                            mercs2_formats::types::TYPE_HASH_REGISTRY.len()
                        ),
                    })?;
                // Sentinel rungs. A `0x0000` low-16 is the dangling-rung HANG, not "no rung".
                aset.push(AsetEntry::new(
                    e.name_hash,
                    0xFFFF_FFFF,
                    0x0000_FFFF,
                    type_id,
                ));
            }

            let first = parsed.entries[0].name_hash;
            log.push(format!(
                "contributions[{index}] raw {} {} bytes, {} entr(ies): {}",
                description.as_deref().unwrap_or("(no description)"),
                bytes.len(),
                parsed.entries.len(),
                parsed
                    .entries
                    .iter()
                    .map(|e| format!(
                        "0x{:08X} {}",
                        e.name_hash,
                        mercs2_formats::types::type_name_from_hash(e.type_hash)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));

            let block = PatchBlock::from_decompressed(
                &bytes,
                format!("blocks\\VZ\\mod_{first:08x}.block"),
                aset,
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Lowering::Block(block))
        }

        // The Code layer. This is the ONE kind that emits no WAD content: on retail a plugin is an
        // `.asi` — a plain Windows DLL under a different extension — dropped where `pmc_bb.dll`
        // globs for it. So it lowers to a placement, and the placement record is what makes it
        // reversible: an overlay is undone by deleting one file, but a file dropped into the game
        // folder cannot be backed out unless something wrote down what was put where.
        Contribution::NativeHook {
            target,
            plugin,
            symbol,
            touches,
        } => {
            let Some(plugin) = plugin else {
                // M0161 already blocks the both-absent case, so reaching here means a `symbol` with
                // no payload.
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: format!(
                        "this contribution names the symbol {} but ships no `plugin:`, and the \
                         Quartermaster does not compile native code — there is no binary for it to \
                         produce. Build the hook into an `.asi` and ship that, or, if the plugin is \
                         somebody else's, depend on it through `load.requires` with a pinned \
                         sha256 rather than vendoring their binary.",
                        symbol.as_deref().unwrap_or("(none)")
                    ),
                });
            };
            if *target != crate::manifest::Target::Retail {
                // M0160 already blocks reimpl+plugin as an Error, so this is the belt to its
                // braces: if that rule is ever relaxed, the lowering must still not place an ASI
                // into a runtime that has no loader for one.
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "an `.asi` is a RETAIL mechanism — `pmc_bb.dll` loads it into the \
                             retail exe. The reimpl Code layer is a Rust/wasm/Lua plugin and has no \
                             consumer yet, so there is nothing to place."
                        .into(),
                });
            }

            let path = root.join(plugin);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{} has no usable file name", path.display()),
                })?
                .to_string();

            // The loader globs `*.asi`. A plugin under any other extension is placed correctly and
            // never even considered — the quietest possible failure, and the file is right there
            // looking installed. The name is also the FileArtifact claim the conflict system keys
            // on, so renaming here would make the claim and the placement disagree.
            if !name.to_ascii_lowercase().ends_with(".asi") {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{name} is not an `.asi`. The loader globs `*.asi` across the game folder \
                         and scripts/plugins/update, so a file under any other extension is never \
                         considered — it would sit in the right place, hashing correctly, doing \
                         nothing. Rename the built DLL to `.asi`."
                    ),
                });
            }
            // The shared file-in-the-game-folder rules: a single path component, and not the
            // loader's own reserved name. `place_file` runs the same check, so the two kinds cannot
            // drift into disagreeing about what a placeable filename is.
            if let Some(why) = game_folder_name_refusal(&name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}. Rename it."),
                });
            }
            if let Some(why) = asi_load_blocker(&bytes) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{name} cannot be loaded by the game: {why}. The loader reports this as \
                         `[FAILED] … (error: …)` in pmc_blackbox.log rather than silently, but only \
                         to someone who reads it."
                    ),
                });
            }

            let relative = place_path(ASI_SUBDIR, &name);
            // The digest is recorded plainly and claims only INTEGRITY. A hash of malware is a
            // correct hash, and a Shipment recording its own payload's digest proves internal
            // consistency and nothing else — so the log says what an ASI is rather than letting a
            // green digest read as a safety check.
            log.push(format!(
                "contributions[{index}] native_hook {name} → {relative}: {} bytes, sha256 {} \
                 (hooks: {}) — UNRESTRICTED NATIVE CODE in the game process; the digest proves the \
                 bytes are unmodified, not that they are safe",
                bytes.len(),
                sha256_hex(&bytes),
                if touches.is_empty() {
                    "none declared".to_string()
                } else {
                    touches
                        .iter()
                        .map(|t| t.0.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ));

            Ok(Lowering::File {
                name,
                relative,
                bytes,
            })
        }

        // A companion file. Mechanically this is `native_hook` minus the PE checks and plus a
        // destination the author named — but the destination is a NAME out of a closed set, so the
        // property that matters is unchanged: no author input reaches the directory half of the
        // path, and the filename half comes from the source file rather than from a field.
        //
        // Needs no game stack, so it lowers in template CI.
        Contribution::PlaceFile { file, dest } => {
            let path = root.join(file);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{} has no usable file name", path.display()),
                })?
                .to_string();

            // M0162 already blocks every one of these as an Error, so this is the belt to its
            // braces — the same shape M0160/M0161 have with `native_hook`'s lowering. A refusal
            // that lives only in a lint rule is a refusal that stops existing the moment somebody
            // adds a way to suppress rules.
            if let Some(why) = companion_name_refusal(&name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}."),
                });
            }

            let relative = place_path(dest.relative_dir(), &name);
            log.push(format!(
                "contributions[{index}] place_file {name} → {relative}: {} bytes, sha256 {}",
                bytes.len(),
                sha256_hex(&bytes),
            ));

            Ok(Lowering::File {
                name,
                relative,
                bytes,
            })
        }

        // NOT implemented, and the reason is worth stating precisely rather than deferring: the
        // destruction machine can be READ and cannot be WRITTEN, and three of the four gaps are
        // outside this crate.
        //
        // 1. No serializer. `orchestrator::parse_state_machine` decodes the family (validated on
        //    retail: al_veh_boat_destroyer 0xE54047D5 parses to 59 switch slots and 47 nodes), but
        //    `StateMachine` is a VIEW — no descriptor indices, no data offsets, no container
        //    position — so it cannot even round-trip. Nothing in the workspace writes SWIT / NODE /
        //    STAT / CHDR / CEXE; `mercs2_workshop`'s bundler lists exactly these tags under
        //    `preserved_only_in_raw`, which is the ecosystem carrying them verbatim because it
        //    cannot author them either.
        // 2. The family is a NESTED container inside the model container, so writing one means
        //    rebuilding that container's descriptor table (tag / offset / size / descendant count
        //    per row), re-basing every following sibling's data offset, recomputing the CSUM, and
        //    re-emitting the whole model block. `model_inject` rewrites geometry groups, not an
        //    arbitrary sibling subtree.
        // 3. `states:` has no schema. Nothing in the manifest format says what that file contains,
        //    so defining one is a format change (Plan 04), not a lowering.
        // 4. There would be no way to check the result. The closest known destructible-model
        //    corruption — collapsing a group's PRMT records so the machine reads off the end — is
        //    an access violation at model instantiation that `wad_simulator` does NOT catch; it
        //    shows up only in-game. Every structural bug this crate has shipped was caught by that
        //    simulator, so a lowering it cannot see is a lowering with no safety net at all.
        Contribution::EditStateMachine { target, .. } => Err(BuildError::Unsupported {
            index,
            kind,
            reason: format!(
                "the destruction state machine can be READ but not WRITTEN. \
                 `orchestrator::parse_state_machine` decodes the SWIT/NODE/STAT/CHDR/CEXE family \
                 and is validated against retail, but it returns a decoded VIEW with no descriptor \
                 indices or data offsets, and no serializer for the family exists anywhere in the \
                 workspace — mercs2_workshop's bundler lists exactly these tags as preserved only \
                 in raw bytes. The family is also a nested container INSIDE the model container, so \
                 writing one means rebuilding that container's descriptor table and re-emitting the \
                 whole model block. On top of that, `states:` has no schema: nothing in the manifest \
                 format says what that file contains, so defining one is a format change rather \
                 than a lowering. Shipping a guess would be worse than refusing, because the \
                 closest known corruption of this kind faults at model instantiation and \
                 wad_simulator does not catch it — it only appears in-game. \
                 If you have already hand-built the block for {target:?}, ship it as `kind: raw` \
                 with `target_layer: data`: that at least carries a declared blast radius."
            ),
        }),
    }
}

struct Rgba {
    width: usize,
    height: usize,
    /// Straight RGBA as `f32` in 0..=255, the shape `texture_encode` expects.
    pixels: Vec<f32>,
}

fn read_png_rgba(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{}: only 8-bit PNGs are supported", path.display()));
    }
    let mut pixels = vec![0f32; w * h * 4];
    for i in 0..w * h {
        let src = i * channels;
        let (r, g, b, a) = match channels {
            1 => (buf[src], buf[src], buf[src], 255),
            2 => (buf[src], buf[src], buf[src], buf[src + 1]),
            3 => (buf[src], buf[src + 1], buf[src + 2], 255),
            _ => (buf[src], buf[src + 1], buf[src + 2], buf[src + 3]),
        };
        pixels[i * 4] = r as f32;
        pixels[i * 4 + 1] = g as f32;
        pixels[i * 4 + 2] = b as f32;
        pixels[i * 4 + 3] = a as f32;
    }
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

fn drop_alpha(rgba: &[f32]) -> Vec<f32> {
    rgba.chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

/// Byte length of mip level 0 for a BC-compressed surface: 4x4 blocks, 8 bytes for BC1, 16 for BC3.
fn mip0_len(w: usize, h: usize, format: TexFormat) -> usize {
    let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
    blocks
        * match format {
            TexFormat::Bc1 => 8,
            TexFormat::Bc3 => 16,
        }
}

/// Lint, lower, assemble, emit.
///
/// `out_dir` defaults to `<root>/build`. Returns `Err(BuildError::Blocked)` rather than a report
/// when the linter blocks — the gate is the return type, not a field a caller might not read.
pub fn build(
    shipment: &LoadedShipment,
    mut game: Option<&mut GameStack>,
    names: Option<&NameTable>,
    out_dir: Option<&Path>,
    corpus_root: Option<&Path>,
) -> Result<BuildReport, BuildError> {
    let mut log = Vec::new();
    let manifest = &shipment.manifest;

    let mut diagnostics = lint::lint(manifest, Some(&shipment.root), names);
    // Rules that need the retail WADs run only when a stack is configured. They are appended
    // BEFORE the gate so a game-aware Error would still block, even though M0007 is a warning.
    if let Some(g) = game.as_deref() {
        diagnostics.extend(lint::game_checks(manifest, g));
    }
    if lint::blocks_build(&diagnostics) {
        return Err(BuildError::Blocked(diagnostics));
    }
    // Reading a console bake is fine and supported; EMITTING for one is not. Checked before any
    // lowering so the failure names the real reason rather than surfacing as a texture-encode error.
    if game
        .as_deref()
        .is_some_and(|g| g.platform() != Platform::Pc)
    {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    log.push(format!(
        "lint: {} finding(s), none blocking",
        diagnostics.len()
    ));

    // NOTE: self-conflicts are NOT re-checked here. `lint` already reports them as blocking M0120
    // findings, so a Shipment that claims one target twice never reaches this point. A second check
    // would be a redundant path with different formatting — and the first version of it returned on
    // the first conflict, hiding the rest.

    let mut blocks = Vec::new();
    let mut files = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match lower(index, c, &shipment.root, game.as_deref_mut(), names, &mut log)? {
            Lowering::Nothing => {}
            Lowering::Block(b) => blocks.push(b),
            Lowering::File {
                name,
                relative,
                bytes,
            } => files.push((name, relative, bytes)),
        }
    }
    let mutations = script_mutations(manifest, &shipment.root)?;

    // ── Link the Script layer ──────────────────────────────────────────────────────────────────
    //
    // Linking this Shipment's own mutations produces a `scripts_vz` that is correct for a SOLO
    // install, which is what keeps each overlay valid standalone and verify-by-hash meaningful.
    //
    // ⚠ It is NOT the whole story. When several script-touching Shipments are installed together,
    // the deploy step must re-link all of their mutations into ONE block — otherwise the last WAD
    // mounted wins and the others' Lua disappears, which is the failure the linker exists to
    // prevent. That cross-Shipment relink belongs to deploy (Modkit), and this is deliberately only
    // its single-Shipment case.
    if !mutations.is_empty() {
        let Some(game) = game.as_deref_mut() else {
            return Err(BuildError::GameRequired {
                index: 0,
                kind: "patch_lua",
            });
        };
        let Some(corpus) = corpus_root else {
            return Err(BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message:
                    "linking Lua needs the decompiled corpus (the base source to append to) — \
                          pass its root; it is vendored at \
                          crates/mercs2_script/corpus/mercs2-luacd/src"
                        .into(),
            });
        };
        let mut loaded = load_script_blocks(game, "patch_lua")?;
        let mut targets: Vec<link::TargetBlock<'_>> = loaded
            .iter_mut()
            .map(|lb| link::TargetBlock {
                path: lb.path.clone(),
                block: &mut lb.block,
            })
            .collect();
        let linked =
            link::link_into_blocks(&mut targets, corpus, &mutations).map_err(|e| {
                BuildError::Lower {
                    index: 0,
                    kind: "patch_lua",
                    message: e.to_string(),
                }
            })?;
        drop(targets);
        for l in &linked {
            log.push(format!(
                "linked {} in {}: {} → {} B source, {} B bytecode, from {:?}",
                l.target,
                loaded[l.block].path,
                l.base_source_bytes,
                l.linked_source_bytes,
                l.bytecode_bytes,
                l.contributors
            ));
        }
        blocks.extend(script_patch_blocks(&loaded, &linked, "patch_lua")?);
    }

    // Mirror the base WAD's CSUM value/meta into the overlay, as the proven publish path does. I
    // previously passed 0/None here, which is a gratuitous divergence from output shapes that are
    // known to load — it costs one header read to match them.
    let csum = match game
        .as_deref()
        .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
    {
        Some(base) => mercs2_formats::donor::base_csum(&base).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "assemble",
            message: m,
        })?,
        None => (0, None),
    };

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| shipment.root.join("build"));
    std::fs::create_dir_all(&out_dir).map_err(|e| BuildError::Io {
        path: out_dir.clone(),
        message: e.to_string(),
    })?;

    let mut placements = Vec::new();
    let mut wad_path = None;
    if !blocks.is_empty() {
        let wad = build_patch_wad_multi(&blocks, csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "assemble",
                message: m,
            }
        })?;
        // Self-check BEFORE writing: a WAD that would hang the game should not reach the disk at
        // all, where a later step could mistake its presence for success.
        let found = verify_emitted(&wad)?;
        for d in &found {
            log.push(format!("self-check: {d}"));
        }
        diagnostics.extend(found);

        let name = format!("{}.wad", manifest.shipment.name);
        let path = out_dir.join(&name);
        std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&wad);
        std::fs::write(
            out_dir.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        log.push(format!(
            "wrote {name}: {} bytes, sha256 {digest}",
            wad.len()
        ));
        placements.push(Placement {
            name,
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::Overlay,
        });
        wad_path = Some(path);
    }

    // Code-layer artifacts. The build directory MIRRORS the tree these will be copied into, so
    // `destination.relative` names the file both here and in the game folder and a deploy step can
    // copy the tree wholesale. Writing them flat was fine while the only destination was `scripts/`
    // and stopped being fine the moment there were seven: two placements differing only in
    // destination — `scripts/OnBoot/init.lua` and `scripts/OnLoad/init.lua` — are not a conflict,
    // they are two files, and flattening them would have one silently overwrite the other in the
    // output while both records claimed the same digest.
    //
    // The digest is taken from the bytes that were WRITTEN, read back off the disk, rather than
    // from the buffer we happen to hold: the record's whole job is to describe what is actually
    // there, and a digest of the intended bytes would still verify after a truncated write.
    for (name, relative, bytes) in files {
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&path, &bytes).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let written = std::fs::read(&path).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&written);
        log.push(format!(
            "wrote {name}: {} bytes, sha256 {digest} → place at {relative}",
            written.len()
        ));
        placements.push(Placement {
            name,
            bytes: written.len(),
            sha256: digest,
            destination: Destination::GameFolder { relative },
        });
    }

    // The placement record: what goes where, each with its digest. Deploy/undo consumes this — a
    // file drop cannot be backed out without it.
    let record = placement_json(&placements);
    std::fs::write(out_dir.join("placement.json"), &record).map_err(|e| BuildError::Io {
        path: out_dir.join("placement.json"),
        message: e.to_string(),
    })?;

    let log_text = log.join("\n") + "\n";
    std::fs::write(out_dir.join("build.log"), &log_text).map_err(|e| BuildError::Io {
        path: out_dir.join("build.log"),
        message: e.to_string(),
    })?;

    Ok(BuildReport {
        diagnostics,
        wad: wad_path,
        placements,
        log,
    })
}

/// The filename of the deploy-time link overlay. Named to sort and read as "last".
pub const LINK_WAD_NAME: &str = "zz-quartermaster-link.wad";

/// What a cross-Shipment link produced.
#[derive(Debug, Clone)]
pub struct LinkReport {
    pub wad: Option<PathBuf>,
    pub placements: Vec<Placement>,
    pub linked: Vec<crate::link::LinkedScript>,
    pub log: Vec<String>,
}

/// Link **every installed Shipment's** script mutations into ONE overlay.
///
/// This is the half `build` cannot do. Each Shipment's own overlay carries a `scripts_vz` linked
/// from its own mutations, which makes it valid standalone — but WAD resolution is last-mounted-
/// wins, so installing two of them means one Shipment's Lua silently disappears. That is the exact
/// failure the mutation-not-a-block design exists to prevent, and preventing it requires a step
/// that sees all of them at once.
///
/// The result **must be mounted LAST**, after every Shipment overlay. Because it is built from all
/// their mutations together it is a superset of each, so whichever per-Shipment block it shadows, it
/// shadows with something strictly more complete.
///
/// Returns `wad: None` when no installed Shipment touches a script — there is nothing to shadow, and
/// emitting an overlay that merely restates the base block would be noise a user has to reason about.
pub fn link_installed(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    corpus_root: &Path,
    out_dir: &Path,
) -> Result<LinkReport, BuildError> {
    if game.platform() != Platform::Pc {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    let mut log = Vec::new();
    let mut mutations = Vec::new();
    for s in shipments {
        mutations.extend(script_mutations(&s.manifest, &s.root)?);
    }
    if mutations.is_empty() {
        log.push("no installed Shipment touches a script — nothing to link".into());
        return Ok(LinkReport {
            wad: None,
            placements: Vec::new(),
            linked: Vec::new(),
            log,
        });
    }
    log.push(format!(
        "linking {} mutation(s) from {} Shipment(s)",
        mutations.len(),
        shipments.len()
    ));

    let mut loaded = load_script_blocks(game, "link")?;
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|lb| link::TargetBlock {
            path: lb.path.clone(),
            block: &mut lb.block,
        })
        .collect();
    let linked = link::link_into_blocks(&mut targets, corpus_root, &mutations).map_err(|e| {
        BuildError::Lower {
            index: 0,
            kind: "link",
            message: e.to_string(),
        }
    })?;
    drop(targets);
    for l in &linked {
        log.push(format!(
            "linked {} in {}: {} → {} B source, {} B bytecode, from {:?}",
            l.target,
            loaded[l.block].path,
            l.base_source_bytes,
            l.linked_source_bytes,
            l.bytecode_bytes,
            l.contributors
        ));
    }
    let patches = script_patch_blocks(&loaded, &linked, "link")?;

    let csum =
        mercs2_formats::donor::base_csum(game.paths()[0]).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "link",
            message: m,
        })?;
    let wad_bytes =
        build_patch_wad_multi(&patches, csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "link",
                message: m,
            }
        })?;

    // The link WAD is mounted LAST and so wins outright. It gets the same self-check as any other,
    // and for the same reason: nothing downstream would notice a defect here.
    let self_check = verify_emitted(&wad_bytes)?;

    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let path = out_dir.join(LINK_WAD_NAME);
    std::fs::write(&path, &wad_bytes).map_err(|e| BuildError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let digest = sha256_hex(&wad_bytes);
    log.push(format!(
        "wrote {LINK_WAD_NAME}: {} bytes, sha256 {digest}",
        wad_bytes.len()
    ));
    for d in &self_check {
        log.push(format!("self-check: {d}"));
    }

    Ok(LinkReport {
        wad: Some(path),
        placements: vec![Placement {
            name: LINK_WAD_NAME.to_string(),
            bytes: wad_bytes.len(),
            sha256: digest,
            destination: Destination::Overlay,
        }],
        linked,
        log,
    })
}

fn placement_json(placements: &[Placement]) -> String {
    let entries: Vec<serde_json::Value> = placements
        .iter()
        .map(|p| {
            let dest = match &p.destination {
                Destination::Overlay => serde_json::json!({ "kind": "overlay" }),
                Destination::GameFolder { relative } => {
                    serde_json::json!({ "kind": "game_folder", "relative": relative })
                }
            };
            serde_json::json!({
                "name": p.name,
                "bytes": p.bytes,
                "sha256": p.sha256,
                "destination": dest,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "placements": entries,
    }))
    .unwrap_or_else(|_| "{}".into())
        + "\n"
}
