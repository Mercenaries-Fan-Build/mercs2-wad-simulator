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
use crate::lint::{self, Diagnostic, WARDROBE_HEROES};
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
use mercs2_formats::types::{
    TYPE_HASH_ANIMATION, TYPE_HASH_EFFECT, TYPE_HASH_LAYER, TYPE_HASH_MODEL, TYPE_HASH_STRINGDB,
    TYPE_HASH_TERRAIN_MESH, TYPE_ID_ANIMATION, TYPE_ID_CFX_PACK, TYPE_ID_EFFECT, TYPE_ID_LAYER,
    TYPE_ID_MODEL, TYPE_ID_SCRIPT, TYPE_ID_STRINGDB, TYPE_ID_TERRAIN_MESH, TYPE_ID_TEXTURE,
};
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

/// Load each of `blocks` (`(PTHS needle, PTHS path)`, [`link::SCRIPT_BLOCKS`] or
/// [`link::SHELL_SCRIPT_BLOCKS`]) from `stack`.
///
/// A block missing from the stack is **skipped, not fatal**. A synthetic or overlay-only stack may
/// carry `scripts_vz` and nothing else, and if a mutation actually needed the absent block the
/// linker already reports `UnknownScript` naming the target — which tells the author what to fix,
/// where "no resident block in the game stack" would not. A stack with none of them is an error.
fn load_script_blocks(
    stack: &mut GameStack,
    blocks: &[(&str, &str)],
    kind: &'static str,
) -> Result<Vec<LoadedScriptBlock>, BuildError> {
    let mut out = Vec::new();
    for (needle, path) in blocks {
        let Some((raw, rows)) = stack.block_and_rows_by_path(needle) else {
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
            message: format!(
                "none of the scripts blocks {:?} is in {}",
                blocks.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                stack.paths().iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
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

/// Link the front end's sound loader ([`link::link_front_end`]) into `shell.wad`'s scripts block,
/// read from the `shell.wad` beside `game`'s base WAD ([`GameStack::open_sibling`]), and return the
/// block to ship in the shell patch — every row copied from `shell.wad` ([`script_patch_blocks`]).
/// Empty when no registration loads a bank in the front end. A missing `shell.wad` is an error.
fn link_shell_loader(
    game: &GameStack,
    corpus: &Path,
    sound_regs: &[link::SoundBankRegistration],
    order: &[String],
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<Vec<PatchBlock>, BuildError> {
    if !sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::FrontEnd)) {
        return Ok(Vec::new());
    }
    let fail = |message: String| BuildError::Lower { index: 0, kind, message };
    let mut shell = game.open_sibling("shell.wad").map_err(fail)?;
    let mut loaded = load_script_blocks(&mut shell, link::SHELL_SCRIPT_BLOCKS, kind)?;
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|lb| link::TargetBlock { path: lb.path.clone(), block: &mut lb.block })
        .collect();
    let linked = link::link_front_end(&mut targets, corpus, sound_regs, order).map_err(|e| fail(e.to_string()))?;
    drop(targets);
    for l in &linked.scripts {
        log.push(format!(
            "linked {} in {} (shell.wad): {} → {} B source, {} B bytecode, from {:?}",
            l.target, loaded[l.block].path, l.base_source_bytes, l.linked_source_bytes, l.bytecode_bytes, l.contributors
        ));
    }
    script_patch_blocks(&loaded, &linked.scripts, kind)
}

/// The loader self-check ([`crate::sound::check_loader_banks`]): every bank the gameplay loader
/// loads has its wavebank in `overlay`, and every bank the front-end loader loads has its wavebank
/// in `shell`.
fn check_sound_loaders(
    sound_regs: &[link::SoundBankRegistration],
    overlay: &[&PatchBlock],
    shell: &[&PatchBlock],
    kind: &'static str,
) -> Result<(), BuildError> {
    use crate::manifest::LoadSession;
    for (session, blocks) in [(LoadSession::Gameplay, overlay), (LoadSession::FrontEnd, shell)] {
        let banks: Vec<&str> = sound_regs.iter().filter(|r| r.sessions.contains(&session)).map(|r| r.bank.as_str()).collect();
        crate::sound::check_loader_banks(link::Level::of(session), &banks, blocks)
            .map_err(|message| BuildError::Lower { index: 0, kind, message })?;
    }
    Ok(())
}

/// The CSUM row of the `shell.wad` beside `game`'s base WAD, which a shell patch is stamped with.
fn shell_csum(game: &GameStack, kind: &'static str) -> Result<(u32, Option<u32>), BuildError> {
    let fail = |message: String| BuildError::Lower { index: 0, kind, message };
    let base = game.paths().first().map(|p| p.to_path_buf()).ok_or_else(|| fail("the game stack is empty".into()))?;
    let shell = crate::sound::sibling_wad(&base, "shell.wad").map_err(fail)?;
    mercs2_formats::donor::base_csum(&shell).map_err(fail)
}

/// Where a built artifact has to end up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Inside the Shipment's overlay WAD.
    Overlay,
    /// A file placed in the game folder, at this path relative to it — an `.asi` in the loader's
    /// search path, or a companion beside it.
    GameFolder { relative: String },
    /// A NEW base WAD in `data/`, at this path relative to the game folder (`data/<name>.wad`). Only
    /// `add_language` produces one; it is ADDITIVE and collision-checked, never over a shipped WAD.
    /// `display` is the label a language selector shows for it.
    DataWad { relative: String, display: String },
    /// A patch WAD, at `relative` under the output directory, whose blocks a deploy step merges —
    /// with every other installed Shipment's for the same `language`, a link output last — into
    /// `data/<language>-patch.wad`, which the engine mounts directly above `data/<language>.wad`
    /// (`FUN_004BFEF0`: `%s\%s-patch.wad` with the language table's name).
    LanguagePatch { language: String, relative: String },
    /// A patch WAD, named by [`Placement::name`] in the output directory, whose blocks a deploy step
    /// merges — with every other installed Shipment's, a link output last — into
    /// `data/shell-patch.wad`, which the engine mounts directly above `shell.wad` in the front end
    /// (`FUN_004BFDA0`: `%s\%s-patch.wad` with the level name `FUN_004C1280` sets to `shell`).
    ShellPatch,
    /// A copy, made by the deploy step, of the game file at `from` to `to` (both relative to the game
    /// folder). No bytes are written to the output directory; [`Placement::sha256`] and
    /// [`Placement::bytes`] describe `from` as the build read it.
    StreamCopy { from: String, to: String },
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

/// The base-WAD basenames the game already ships, which `add_language` must never overwrite.
///
/// The engine opens `.\Data\<name>.wad` by name; a novel language is ADDITIVE, so it may only
/// introduce a name the game does not ship. These are the level/shell/loading WADs plus the six
/// shipped language WADs — placing over any of them would shadow base-game data.
pub const RESERVED_WAD_BASENAMES: &[&str] = &[
    "vz", "shell", "loading", "english", "french", "german", "italian", "spanish", "japanese",
    "russian",
];

/// Refuse an `add_language` name that could not be a novel language WAD.
///
/// Two failure modes, both about `name` becoming BOTH `.\Data\<name>.wad` AND the stringdb key: it
/// must be a lowercase `[a-z0-9_]` token (a single, filesystem-safe path component — the engine's own
/// language names are `english`, `english_uk`), and it must not be a WAD the game already ships, which
/// this kind would otherwise overwrite. This is the `data/` safety pivot in code: [`PlaceIn`] bans
/// `data/` outright, and `add_language` earns it back only for a builder-derived, collision-checked
/// name. Called by the linter (M0200, so template CI says so) AND the lowering (so the refusal
/// survives a suppressed rule).
pub fn language_name_refusal(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        return Some("it is empty".into());
    }
    if !n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Some(format!(
            "{n:?} is not a language token — expected lowercase a-z, digits and underscore only. It \
             becomes the filename `.\\Data\\{n}.wad` and hashes to the stringdb key, so a separator, \
             an uppercase letter or punctuation cannot appear in it"
        ));
    }
    if RESERVED_WAD_BASENAMES.contains(&n) {
        return Some(format!(
            "{n:?} is a WAD the game already ships (`.\\Data\\{n}.wad`). add_language only ADDS a \
             language it never shipped and must never overwrite a base WAD — pick a name that is not \
             one of: {}",
            RESERVED_WAD_BASENAMES.join(", ")
        ));
    }
    None
}

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
         emitted as `_build/<name>.wad` and mounted by the deploy step — it is never placed by an \
         author, and the format cannot express a write into the base WAD at all",
    ),
    (
        "exe",
        "`Mercenaries2.exe` is the game. An exe edit stays unrepresentable rather than merely \
         linted, and that is only true if no file placement can write one",
    ),
    (
        "dll",
        "a DLL in the game folder is either the game's own, the loader (`pmc_bb.dll`) or a runtime \
         other plugins import by name. A plugin ships as an `.asi` through `native_hook`, and a \
         runtime DLL ships through `add_runtime_dll`, which places `<shipment.name>.dll` in the \
         game root and refuses the loader's and the game's names",
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
    if let Some(why) = single_filename_refusal(name) {
        return Some(why);
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

/// Reject a name that is not ONE filename: empty, `.`/`..`, or carrying a separator or a drive
/// colon.
///
/// The component half of [`game_folder_name_refusal`], on its own because a name can need it without
/// being a file qm writes: a `supersedes` entry names a legacy file to DETECT in the game folder, so
/// the extension and reserved-name refusals, which exist to stop a write, do not apply to it.
pub fn single_filename_refusal(name: &str) -> Option<String> {
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
    None
}

/// Reject the file name of an `add_runtime_dll`, or `None` when it may be placed in the game root.
///
/// The single-filename rule is [`single_filename_refusal`], the same one every game-folder placement
/// uses. On top of it, and all compared lowercased because Windows file names are case-insensitive:
///
/// * the extension must be `.dll`;
/// * the stem must not be on [`crate::manifest::DENY_LISTED_DLL_STEMS`] — the loader, its sidecar
///   and the game-folder DLLs the loader reports;
/// * the name must be `<shipment_name>.dll`, so a runtime Shipment ships exactly one DLL, named
///   after itself.
///
/// One function, called by the linter (M0162), `qm preflight` / `qm link` (M0162 in the plan) and
/// the lowering (so the refusal survives a rule being suppressed).
pub fn runtime_dll_name_refusal(name: &str, shipment_name: &str) -> Option<String> {
    if let Some(why) = single_filename_refusal(name) {
        return Some(why);
    }
    let lowered = name.to_lowercase();
    let Some(stem) = lowered.strip_suffix(".dll") else {
        return Some(
            "it is not a `.dll`. `add_runtime_dll` places a runtime DLL that plugins import by \
             name; a plugin is an `.asi` shipped through `native_hook`, and any other file is a \
             `place_file`"
                .into(),
        );
    };
    if crate::manifest::DENY_LISTED_DLL_STEMS.contains(&stem) {
        return Some(format!(
            "`{stem}.dll` is a DLL no Shipment may ship ({}, compared case-insensitively): the \
             loader, its sidecar and the game-folder DLLs the loader reports",
            crate::manifest::DENY_LISTED_DLL_STEMS
                .iter()
                .map(|s| format!("{s}.dll"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let wanted = format!("{}.dll", shipment_name.to_lowercase());
    if lowered != wanted {
        return Some(format!(
            "a runtime DLL must be named after its Shipment, `{wanted}` (compared \
             case-insensitively), so that one runtime Shipment ships exactly one DLL and two \
             Shipments can never ship one name"
        ));
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
    /// Several blocks from one contribution. A skinned outfit with `textures:` emits the model
    /// block plus one `TYPE_ID_TEXTURE` block per supplied map, and they have to travel together —
    /// the model's MTRL repoints name hashes that only resolve if these ship alongside it.
    Blocks(Vec<PatchBlock>),
    /// A NEW base WAD placed in `data/`: `add_language` is the only producer. The engine mounts
    /// `.\Data\<language>.wad` in its language slot, above the level WAD (`FUN_004BFE20`), so its
    /// string table, fonts and voice-over tables resolve from there. `stream_copy` is the voice
    /// stream the language plays from, as `(from, to)` under the game folder.
    LanguageWad {
        language: String,
        display: String,
        blocks: Vec<PatchBlock>,
        stream_copy: (String, String),
    },
    /// A file placed in the game folder. Carries its bytes so the caller writes them exactly once,
    /// next to the digest it records for them.
    File {
        name: String,
        relative: String,
        bytes: Vec<u8>,
    },
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
    /// The load plan over the set is not ok, so nothing is linked. The plan carries the findings
    /// and has been written beside where the link output would go.
    Plan(Box<crate::plan::LoadPlan>),
    /// A file this Shipment supersedes is still in the game folder. qm never deletes it.
    Superseded {
        shipment: String,
        relative: String,
    },
    /// The plan or a superseded-file probe could not be computed at all.
    Compat(crate::compat::CompatError),
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
            BuildError::Plan(plan) => {
                let errors: Vec<&crate::plan::Finding> = plan
                    .findings
                    .iter()
                    .filter(|x| x.severity == crate::plan::FindingSeverity::Error)
                    .collect();
                write!(
                    f,
                    "the load plan is not ok — {} error finding(s), so nothing was linked:",
                    errors.len()
                )?;
                for x in errors {
                    write!(f, "\n  [{}] {}: {}", x.code, x.items.join(", "), x.message)?;
                }
                Ok(())
            }
            BuildError::Superseded { shipment, relative } => write!(
                f,
                "{shipment} supersedes {relative}, which is still in the game folder — remove it \
                 first (qm never deletes it)"
            ),
            BuildError::Compat(e) => write!(f, "{e}"),
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
            // add_ui's Script half is NOT a plain append. Its FlashWidget registration is baked into
            // the generated `qm_modloader` script and reached through a one-line trampoline the
            // linker synthesizes — so the resident never grows with mod count. Collected separately
            // by `ui_registrations`; the Data half (the cfx_pack movie) is in `lower`.
            Contribution::AddUi { .. } => {}
            // activate_layer contributes no script APPEND either — its marks are baked into
            // `qm_modloader` from `layer_registrations`, not concatenated onto a base script.
            Contribution::ActivateLayer { .. } => {}
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
            Contribution::AddShopItem {
                id,
                name,
                description,
                icon,
                shops,
                catalog,
                item_type,
                cash_cost,
                fuel_cost,
                max_stock,
                unlocked,
                behaviour,
                equipment_type,
            } => {
                use crate::manifest::ShopCatalog;
                // A NOVEL support behaviour (behaviour.script set) is NOT a load-time append: it
                // defers into `qm_modloader` via `support_registrations`, because its `module:Create()`
                // would run against a nil global at resident-load. Skip it here.
                if behaviour.as_ref().is_some_and(|b| b.script.is_some()) {
                    continue;
                }
                match catalog {
                    ShopCatalog::Support => {
                        let b = behaviour.as_ref().ok_or_else(|| BuildError::Lower {
                            index,
                            kind: "add_shop_item",
                            message: format!(
                                "support shop item {id:?} needs a `behaviour` (the oSupport module)"
                            ),
                        })?;
                        let itype = item_type.as_ref().map_or("Supply", |t| t.lua());
                        let unlock = link::shop_unlock_table(shops, *unlocked);
                        out.push(ScriptMutation {
                            shipment: shipment.clone(),
                            target: "mrxsupportdata".into(),
                            append: link::shop_support_row_append(
                                id,
                                name,
                                description,
                                icon,
                                itype,
                                *cash_cost,
                                *fuel_cost,
                                *max_stock,
                                &unlock,
                                &b.module,
                                b.cargo.as_deref(),
                                b.delivery_vehicle.as_deref(),
                            ),
                        });
                    }
                    ShopCatalog::Equipment => {
                        let et = equipment_type.as_ref().ok_or_else(|| BuildError::Lower {
                            index,
                            kind: "add_shop_item",
                            message: format!(
                                "equipment shop item {id:?} needs an `equipment_type` \
                                 (fuel_tank | grappling_hook)"
                            ),
                        })?;
                        out.push(ScriptMutation {
                            shipment: shipment.clone(),
                            target: "wifequipmentdata".into(),
                            append: link::shop_equipment_row_append(
                                id,
                                name,
                                description,
                                icon,
                                et.lua_const(),
                                *cash_cost,
                            ),
                        });
                    }
                }
                let field = match catalog {
                    ShopCatalog::Support => "tSupport",
                    ShopCatalog::Equipment => "tEquipment",
                };
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: "mrxrewarddata".into(),
                    append: link::shop_reward_append(id, field, shops),
                });
            }
            _ => {}
        }
    }
    Ok(out)
}

#[derive(serde::Deserialize)]
struct EntityYaml {
    key: u32,
    model: String,
    pos: [f32; 3],
    #[serde(default)]
    quat: Option<[f32; 4]>,
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum EntitiesDoc {
    List(Vec<EntityYaml>),
    Single(EntityYaml),
}

fn parse_entities_file(path: &Path) -> Result<Vec<mercs2_formats::placement_build::NewEntity>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading: {e}"))?;
    let doc: EntitiesDoc = serde_norway::from_str(&text).map_err(|e| format!("parse yaml: {e}"))?;
    let list = match doc {
        EntitiesDoc::List(v) => v,
        EntitiesDoc::Single(e) => vec![e],
    };
    if list.is_empty() { return Err("no entities".into()); }
    list.into_iter().map(|e| {
        let quat = if let Some(q) = e.quat { q }
        else if let Some(yaw) = e.yaw {
            let h = (yaw.to_radians() * 0.5); [0.0, h.sin(), 0.0, h.cos()]
        } else { [0.0, 0.0, 0.0, 1.0] };
        Ok(mercs2_formats::placement_build::NewEntity {
            key: e.key,
            model_hash: crate::manifest::asset_hash(&e.model),
            pos: e.pos,
            quat,
            name: e.name.unwrap_or_else(|| format!("modent_{:08x}", e.key)),
        })
    }).collect()
}

fn lower_layer_append(
    game: &mut crate::game::GameStack,
    template_layer: &str,
    new_layer_name: &str,
    ents: &[mercs2_formats::placement_build::NewEntity],
    index: usize,
    kind: &'static str,
) -> Result<Lowering, BuildError> {
    let inputs = game.layer_block_for_edit(template_layer).ok_or_else(|| BuildError::Lower {
        index, kind,
        message: format!("no layer matching {template_layer:?} in the game stack"),
    })?;
    let template_sub = find_template_sub(&inputs.block).ok_or_else(|| BuildError::Lower {
        index, kind,
        message: format!("no layer sub-block found in the carrier block for {template_layer:?}"),
    })?;
    let layer_hash = crate::manifest::asset_hash(new_layer_name);
    let new_block = mercs2_formats::placement_build::append_placements(
        &inputs.block, template_sub, ents, layer_hash,
    ).map_err(|m| BuildError::Lower { index, kind, message: m })?;

    // Every sub-block in the modified carrier needs its own ASET row: on WAD merge the base's
    // rows still point at BASE block indices, and our overlay's block gets re-indexed. Without
    // rows for the pre-existing sub-blocks, they resolve to a stale base index and M0004 fires.
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&new_block);
    let mut asets = Vec::with_capacity(count as usize);
    for e in &entries {
        let tid = mercs2_formats::aset_type_ids::type_id_for_type_hash(e.type_hash)
            .unwrap_or(TYPE_ID_LAYER);
        asets.push(AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, tid));
    }

    Ok(Lowering::Block(PatchBlock::from_decompressed(
        &new_block,
        inputs.path.clone(),
        asets,
        None,
    ).map_err(|m| BuildError::Lower { index, kind, message: m })?))
}

fn find_template_sub(block: &[u8]) -> Option<usize> {
    if block.len() < 4 { return None; }
    let count = u32::from_le_bytes(block[0..4].try_into().ok()?) as usize;
    for i in 0..count {
        let row = 4 + i * 16;
        if row + 8 > block.len() { break; }
        let type_hash = u32::from_le_bytes(block[row + 4..row + 8].try_into().ok()?);
        if type_hash == TYPE_HASH_LAYER
            && mercs2_formats::placement_build::container_has_scaffolding(block, i)
        {
            return Some(i);
        }
    }
    None
}

/// Shared helper for the passthrough kinds: read an author-supplied binary file and wrap it as a
/// single-entry mod block carrying one primary ASET row at `pandemic_hash_m2(<name_or_target>)`.
///
/// The manifest side (contribution parse, path sandbox, block emit, ASET wire) is real today;
/// the format side (an encoder that produces the bytes from an author-friendly source description)
/// is orthogonal future work per asset type. A modder with an external encoder can ship any of
/// these kinds NOW.
fn opaque_new_asset(
    root: &Path,
    payload: &Path,
    name_or_target: &str,
    type_hash: u32,
    type_id: u32,
    index: usize,
    kind: &'static str,
) -> Result<Lowering, BuildError> {
    let hash = crate::manifest::asset_hash(name_or_target);
    let bytes = std::fs::read(root.join(payload)).map_err(|e| BuildError::Lower {
        index,
        kind,
        message: format!("reading {}: {e}", root.join(payload).display()),
    })?;
    Ok(Lowering::Block(opaque_container_block(
        hash, type_hash, type_id, &bytes, index, kind,
    )?))
}

/// Wrap an already-produced container as a single-entry mod block. Split from
/// [`opaque_new_asset`] so the `replace_phy2` codepath (which produces `edited` in-Rust rather
/// than reading a file) can share the block-emit half.
fn opaque_container_block(
    hash: u32,
    type_hash: u32,
    type_id: u32,
    bytes: &[u8],
    index: usize,
    kind: &'static str,
) -> Result<PatchBlock, BuildError> {
    let mut block_data = Vec::new();
    block_data.extend_from_slice(&1u32.to_le_bytes());
    block_data.extend_from_slice(&hash.to_le_bytes());
    block_data.extend_from_slice(&type_hash.to_le_bytes());
    block_data.extend_from_slice(&0u32.to_le_bytes());
    block_data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    block_data.extend_from_slice(bytes);
    let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, type_id);
    PatchBlock::from_decompressed(
        &block_data,
        format!("blocks\\VZ\\mod_{hash:08x}.block"),
        vec![aset],
        None,
    )
    .map_err(|m| BuildError::Lower { index, kind, message: m })
}

/// `add_animation` / `replace_animation`: the retail Havok clip container, from its sources.
///
/// The container is `mercs2_formats::anim_container::build_clip_container` — `info` / `data` /
/// `trnm` / optional `evnt`, packed, with its CSUM — the shape all 4,232 retail clips take and that
/// the writer reproduces byte-for-byte over every one of them. The same pairing check M0213 runs
/// hermetically runs again here, because a build must not depend on somebody having linted first.
///
/// A REPLACE also reads the target out of the game stack: it must exist, and it must be a Havok
/// clip. The 29 retail `animation` assets that are `MANM` keyframe animations cannot be expressed by
/// these sources, and writing a clip over one would change what kind of asset it is.
fn lower_animation(
    root: &Path,
    name_or_target: &str,
    sources: AnimationSources<'_>,
    replace_in: Option<&mut GameStack>,
    index: usize,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<Lowering, BuildError> {
    use mercs2_formats::anim_container;
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let read = |p: &Path| {
        std::fs::read(root.join(p)).map_err(|e| err(format!("reading {}: {e}", p.display())))
    };
    let hash = crate::manifest::asset_hash(name_or_target);
    if let Some(game) = replace_in {
        let existing = game
            .container_for_asset(hash, TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION)
            .ok_or_else(|| {
                err(format!(
                    "{name_or_target:?} (0x{hash:08X}) is not an animation in the game stack — a \
                     replace needs a shipped clip to replace"
                ))
            })?;
        let chunks = anim_container::parse_container(&existing)
            .map_err(|m| err(format!("the shipped {name_or_target:?} does not read: {m}")))?;
        match anim_container::classify(&chunks).map_err(err)? {
            anim_container::AnimContainerKind::HavokClip { .. } => {}
            anim_container::AnimContainerKind::Keyframe => {
                return Err(err(format!(
                    "the shipped {name_or_target:?} (0x{hash:08X}) is a MANM keyframe animation, \
                     not a Havok clip. `clip`/`trnm`/`events` describe a Havok clip, so replacing \
                     it would change the asset's kind; no source this kind takes can express a \
                     MANM animation."
                )));
            }
        }
    }
    let clip_bytes = read(sources.clip)?;
    let trnm_bytes = read(sources.trnm)?;
    let evnt_bytes = sources.events.map(read).transpose()?;
    let container =
        anim_container::build_clip_container(&clip_bytes, &trnm_bytes, evnt_bytes.as_deref())
            .map_err(err)?;
    log.push(format!(
        "contributions[{index}] {kind} {name_or_target} 0x{hash:08X}: clip {} B, trnm {} B, {} \
         → container {} B",
        clip_bytes.len(),
        trnm_bytes.len(),
        match &evnt_bytes {
            Some(e) => format!("evnt {} B", e.len()),
            None => "no evnt".to_string(),
        },
        container.len()
    ));
    Ok(Lowering::Block(opaque_container_block(
        hash,
        TYPE_HASH_ANIMATION,
        TYPE_ID_ANIMATION,
        &container,
        index,
        kind,
    )?))
}

/// The three `src/` files an animation contribution names.
struct AnimationSources<'a> {
    clip: &'a Path,
    trnm: &'a Path,
    events: Option<&'a Path>,
}

/// Every `replace_lua`, ready for the linker to compile + swap in place.
pub fn script_replacements(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::ScriptReplacement>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::ReplaceLua { target, source } = c {
            let path = root.join(source);
            let src = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                index,
                kind: "replace_lua",
                message: format!("reading {}: {e}", path.display()),
            })?;
            out.push(link::ScriptReplacement {
                shipment: shipment.clone(),
                target: target.clone(),
                source: src,
            });
        }
    }
    Ok(out)
}

/// Every `add_script`, ready for the linker to compile + mint as a fresh scripts_vz entry.
///
/// Reads each source file up front, so a missing `.lua` is a build-time (deploy-time) error with
/// the shipment name and the offending path, not a mysterious silent skip at link time.
pub fn script_additions(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::ScriptAddition>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::AddScript { name, source } = c {
            let path = root.join(source);
            let src = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                index,
                kind: "add_script",
                message: format!("reading {}: {e}", path.display()),
            })?;
            out.push(link::ScriptAddition {
                shipment: shipment.clone(),
                name: name.clone(),
                source: src,
            });
        }
    }
    Ok(out)
}

/// Every `add_ui`'s FlashWidget registration, for the linker to bake into `qm_modloader`.
///
/// Kept separate from [`script_mutations`] because a UI mod does NOT append to a base script — its
/// registration lives in the Quartermaster-owned load space, reached by a trampoline the linker
/// synthesizes once. Returning an empty vec (no `add_ui`) means no loader is minted at all.
pub fn ui_registrations(manifest: &crate::manifest::Manifest) -> Vec<link::UiRegistration> {
    let shipment = manifest.shipment.name.clone();
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::AddUi { name, .. } => Some(link::UiRegistration {
                shipment: shipment.clone(),
                // `SetSwfFile` hashes this name to find the cfx_pack, so it must be the SAME `name`
                // the movie block is registered under (see the AddMovie/AddUi arm in `lower`, which
                // mints the pack at `asset_hash(name)`), not the source file's stem.
                movie: name.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Every `activate_layer`'s `MrxLayerManager` marks, for the linker to bake into `qm_modloader`.
///
/// Kept separate from [`script_mutations`] for the same reason as [`ui_registrations`]: it does not
/// append to a base script but lives in the Quartermaster-owned load space, reached by the one
/// synthesized trampoline. A UI mod and a layer mod both feed this loader, so either alone is enough
/// to mint it.
pub fn layer_registrations(manifest: &crate::manifest::Manifest) -> Vec<link::LayerRegistration> {
    let shipment = manifest.shipment.name.clone();
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::ActivateLayer { layer, replaces } => Some(link::LayerRegistration {
                shipment: shipment.clone(),
                add: layer.clone(),
                remove: replaces.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Every NOVEL-behaviour shop item's registration, for the linker to mint the `MrxSupport` subclass
/// and bake its DEFERRED catalog row into `qm_modloader`. A shop item is novel only when its
/// `behaviour.script` is set; a resident-module item stays on the load-time append path in
/// [`script_mutations`]. Support catalog only — equipment carries no behaviour.
pub fn support_registrations(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::SupportRegistration>, BuildError> {
    use crate::manifest::ShopCatalog;
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let Contribution::AddShopItem {
            id,
            name,
            description,
            icon,
            shops,
            catalog,
            item_type,
            cash_cost,
            fuel_cost,
            max_stock,
            unlocked,
            behaviour,
            ..
        } = c
        else {
            continue;
        };
        let Some(b) = behaviour else { continue };
        let Some(script) = &b.script else { continue };
        if !matches!(catalog, ShopCatalog::Support) {
            return Err(BuildError::Lower {
                index,
                kind: "add_shop_item",
                message: format!("shop item {id:?}: behaviour.script is support-catalog only"),
            });
        }
        let path = root.join(script);
        let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
            index,
            kind: "add_shop_item",
            message: format!("reading behaviour.script {}: {e}", path.display()),
        })?;
        out.push(link::SupportRegistration {
            shipment: shipment.clone(),
            module: b.module.clone(),
            source,
            id: id.clone(),
            name: name.clone(),
            description: description.clone(),
            icon: icon.clone(),
            item_type: item_type.as_ref().map_or("Supply", |t| t.lua()).to_string(),
            cash_cost: *cash_cost,
            fuel_cost: *fuel_cost,
            max_stock: *max_stock,
            unlock_table: link::shop_unlock_table(shops, *unlocked),
            cargo: b.cargo.clone(),
            delivery_vehicle: b.delivery_vehicle.clone(),
            shops: shops.iter().map(|v| v.faction_key().to_string()).collect(),
        });
    }
    Ok(out)
}

/// Mint a Scaleform movie as a `cfx_pack` patch block under `name`, from the file at `root/movie`.
///
/// Shared by `add_movie` and `add_ui` so the two ship byte-identical Data — the only difference is
/// that `add_ui` also bakes a FlashWidget registration into `qm_modloader`. Parses before wrapping
/// (a non-movie leaf still checksums and resolves — the loader is the first to notice, and it names
/// nothing), then emits the pack with its ADDITIVE, PRIMARY cfx_pack ASET row.
fn lower_movie(
    root: &Path,
    name: &str,
    movie: &Path,
    index: usize,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<PatchBlock, BuildError> {
    let path = root.join(movie);
    let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
        index,
        kind,
        message: format!("reading {}: {e}", path.display()),
    })?;

    let parsed = mercs2_formats::gfx::GfxMovie::parse(&bytes).map_err(|m| BuildError::Lower {
        index,
        kind,
        message: format!(
            "{} is not a Scaleform movie this build can read: {m}. Expected a `.gfx` beginning \
             with `GFX` or `CFX` (or the SWF spellings `FWS`/`CWS`) — retail ships 61 `CFX` and 3 \
             `GFX`, so either is fine, but a project file, an already-wrapped container or a \
             truncated export is not.",
            path.display()
        ),
    })?;

    let hash = crate::manifest::asset_hash(name);
    let block_bytes = mercs2_formats::gfx::build_cfx_pack_block(hash, &bytes);

    // The tag census is logged rather than merely counted: an emitter that silently dropped the
    // movie's content still produces a valid header, and "0 tags" in the log is the only place that
    // would show.
    let features = parsed.features();
    let [w, h] = parsed.stage_px();
    log.push(format!(
        "contributions[{index}] {kind} {name} 0x{hash:08X} ← {} \
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

    // ADDITIVE and PRIMARY. A movie has no LOD chain at all, so both rung halves stay at their
    // sentinels — `0x0000` in the low 16 is the dangling-rung HANG, not "no rung".
    let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_CFX_PACK);
    PatchBlock::from_decompressed(
        &block_bytes,
        format!("blocks\\VZ\\mod_{hash:08x}.block"),
        vec![aset],
        None,
    )
    .map_err(|m| BuildError::Lower {
        index,
        kind,
        message: m,
    })
}

/// Encode one authored image into a `TYPE_ID_TEXTURE` patch block under `asset_name`.
///
/// Returns `(block, to_hash)`. For a skin, the caller pairs `to_hash` with each donor hash the model
/// names at that slot to build the [`MtrlRepoint`]s; for a standalone `add_texture` the hash IS the
/// asset and nothing is repointed.
///
/// The name is a PARAMETER rather than being derived as `<outfit>_<slot>` here, which is what
/// previously made a novel texture inexpressible on its own: the encoder already did every hard
/// part (swizzle, format choice, full resident mip chain, primary ASET row) and simply had no way
/// to be told what to call its output.
///
/// Format policy, from `character_kit.md`: a NORMAL map is BC3/DXT5nm — this project's swizzle is
/// `R=1, G=ny, B=1, A=nx` (matching `mercs2_workshop::texenc::encode_normal_full_chain`, so preview
/// and shipped agree). Diffuse and specular are BC1 unless the source carries real alpha, which
/// needs BC3. Textures are fully resident: the whole mip chain ships, because a short BODY makes the
/// engine over-read and the world-load livelocks.
fn build_named_texture(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    path: &Path,
    is_normal: bool,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let img = read_png_rgba(path).map_err(err)?;
    build_texture_from_rgba(index, kind, asset_name, img, is_normal, None, &path.display().to_string())
}

/// Build a texture block from a GLB's embedded PNG bytes — the per-material skin path. `brighten`
/// gamma-lifts a too-dark imported diffuse; `label` names the material for any error.
fn build_named_texture_bytes(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    png_bytes: &[u8],
    is_normal: bool,
    brighten: Option<f32>,
    label: &str,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let img = read_png_rgba_bytes(png_bytes, label).map_err(err)?;
    build_texture_from_rgba(index, kind, asset_name, img, is_normal, brighten, label)
}

/// The encoder half: already-decoded RGBA → a resident texture block + its asset hash. Shared by the
/// file-path and embedded-bytes callers.
fn build_texture_from_rgba(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    img: Rgba,
    is_normal: bool,
    // Gamma < 1 brightens the RGB (alpha untouched). Imported diffuse maps are often authored much
    // darker than the game's dim interior lighting can show; a lift brings them into the range retail
    // albedos sit in. `None` for a hand-authored `textures:` map, which is presumed already correct.
    brighten: Option<f32>,
    label: &str,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower {
        index,
        kind,
        message: m,
    };
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || w % 4 != 0 || h % 4 != 0 {
        return Err(err(format!(
            "{label}: {w}x{h} — a block-compressed texture needs both dimensions to be multiples of 4"
        )));
    }

    // Swizzle first, then decide the format from what the pixels actually contain.
    let pixels: Vec<f32> = if is_normal {
        img.pixels
            .chunks_exact(4)
            .flat_map(|p| [255.0, p[1], 255.0, p[0]])
            .collect()
    } else if let Some(g) = brighten {
        let lift = |v: f32| ((v / 255.0).powf(g) * 255.0).clamp(0.0, 255.0);
        img.pixels
            .chunks_exact(4)
            .flat_map(|p| [lift(p[0]), lift(p[1]), lift(p[2]), p[3]])
            .collect()
    } else {
        img.pixels.clone()
    };
    let has_alpha = !is_normal && pixels.chunks_exact(4).any(|p| p[3] < 254.0);
    let format = if is_normal || has_alpha {
        TexFormat::Bc3
    } else {
        TexFormat::Bc1
    };
    let body = match format {
        TexFormat::Bc1 => {
            let rgb = drop_alpha(&pixels);
            texture_encode::mip_chain(w, h, 3, &rgb, texture_encode::encode_bc1)
        }
        TexFormat::Bc3 => texture_encode::mip_chain(w, h, 4, &pixels, texture_encode::encode_bc3),
    };

    let to = crate::manifest::asset_hash(asset_name);
    // Ship the FULLY-RESIDENT container shape the engine's texture binder actually reads —
    // NAME / INFO / BODY with the `0xFFFF` sentinel at INFO@32 and INFO[26..32]=0. The old
    // `build_texture_block` emitted only INFO/BODY (no NAME, no sentinel), so its texture LOADED but
    // never BOUND: the injected model sampled it black. This is the obama skin recipe — ship a texture
    // block shaped like a real resident texture. `build_resident_texture` enforces the exact mip-chain
    // length too (a short body hangs the world load), so it validates `body` on the way through.
    let container =
        mercs2_formats::texture::build_resident_texture(asset_name, w as u32, h as u32, format, &body)
            .map_err(|e| err(format!("{label}: {e}")))?;
    let mut block_bytes = Vec::with_capacity(20 + container.len());
    block_bytes.extend_from_slice(&1u32.to_le_bytes()); // flags/version
    block_bytes.extend_from_slice(&to.to_le_bytes()); // asset name hash
    block_bytes.extend_from_slice(&mercs2_formats::types::TYPE_HASH_TEXTURE.to_le_bytes());
    block_bytes.extend_from_slice(&0u32.to_le_bytes());
    block_bytes.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block_bytes.extend_from_slice(&container);
    let aset = AsetEntry::new(to, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_TEXTURE);
    let block = PatchBlock::from_decompressed(
        &block_bytes,
        format!("blocks\\VZ\\mod_{to:08x}.block"),
        vec![aset],
        None,
    )
    .map_err(err)?;
    Ok((block, to))
}

/// Bilinear-resample an [`Rgba`] to `(w, h)`. Straight (non-premultiplied) RGBA; a clone when the
/// dimensions already match. The build has no `image` crate, so the one resampler it needs for the
/// single-group atlas lives here.
fn resize_rgba(src: &Rgba, w: usize, h: usize) -> Rgba {
    if src.width == w && src.height == h {
        return Rgba { width: w, height: h, pixels: src.pixels.clone() };
    }
    let (sw, sh) = (src.width.max(1), src.height.max(1));
    let mut px = vec![0.0f32; w * h * 4];
    let sx = sw as f32 / w as f32;
    let sy = sh as f32 / h as f32;
    let at = |xx: usize, yy: usize, c: usize| src.pixels[(yy * sw + xx) * 4 + c];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f32;
            for c in 0..4 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                let bot = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                px[(y * w + x) * 4 + c] = top * (1.0 - ty) + bot * ty;
            }
        }
    }
    Rgba { width: w, height: h, pixels: px }
}

/// Bake the used materials' diffuse maps into ONE atlas texture, returning the atlas image and each
/// material's cell as `[u0, v0, su, sv]` (fractions of the atlas). This is what lets `single_group`
/// wear a MULTI-material import's whole correct skin from one draw group / one material: the caller
/// remaps each part's UVs into its cell (see `char_lower::LowerOpts::atlas_cells`).
///
/// A shelf packer into a fixed 2048-wide atlas, tallest-first, each cell capped so the pack fits in
/// 2048 tall (dropping the cap 1024→512→… on overflow). Deterministic given the same inputs, which
/// keeps the build reproducible.
fn bake_diffuse_atlas(mats: &[(usize, Rgba)]) -> (Rgba, std::collections::HashMap<usize, [f32; 4]>) {
    use std::collections::HashMap;
    const WIDTH: usize = 2048;
    const MAXH: usize = 2048;
    for &cap in &[1024usize, 512, 256, 128, 64] {
        // Cell size = native, capped to `cap` and to the atlas width.
        let mut cells: Vec<(usize, usize, usize)> = mats
            .iter()
            .map(|(m, img)| (*m, img.width.min(cap).min(WIDTH).max(4), img.height.min(cap).max(4)))
            .collect();
        cells.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        let mut placed: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new();
        let (mut x, mut y, mut row_h, mut used_h, mut ok) = (0usize, 0usize, 0usize, 0usize, true);
        for &(m, cw, ch) in &cells {
            if x + cw > WIDTH {
                x = 0;
                y += row_h;
                row_h = 0;
            }
            if y + ch > MAXH {
                ok = false;
                break;
            }
            placed.insert(m, (x, y, cw, ch));
            x += cw;
            row_h = row_h.max(ch);
            used_h = used_h.max(y + ch);
        }
        if !ok {
            continue;
        }
        let atlas_h = ((used_h + 3) / 4 * 4).max(4);
        let mut atlas = Rgba { width: WIDTH, height: atlas_h, pixels: vec![0.0; WIDTH * atlas_h * 4] };
        for i in 0..WIDTH * atlas_h {
            atlas.pixels[i * 4 + 3] = 255.0; // opaque
        }
        let mut cellmap = HashMap::new();
        for (m, img) in mats {
            let (px, py, cw, ch) = placed[m];
            let r = resize_rgba(img, cw, ch);
            for ry in 0..ch {
                for rx in 0..cw {
                    let s = (ry * cw + rx) * 4;
                    let dst = ((py + ry) * WIDTH + (px + rx)) * 4;
                    atlas.pixels[dst..dst + 4].copy_from_slice(&r.pixels[s..s + 4]);
                }
            }
            cellmap.insert(
                *m,
                [
                    px as f32 / WIDTH as f32,
                    py as f32 / atlas_h as f32,
                    cw as f32 / WIDTH as f32,
                    ch as f32 / atlas_h as f32,
                ],
            );
        }
        return (atlas, cellmap);
    }
    // Unreachable for any real input (cap 64 fits thousands of cells); a valid 4x4 keeps it total.
    let mut px = vec![0.0f32; 64];
    for i in 0..16 {
        px[i * 4 + 3] = 255.0;
    }
    (Rgba { width: 4, height: 4, pixels: px }, std::collections::HashMap::new())
}

/// Regenerate a rigid prop's static collision from its OWN injected mesh and splice it into the
/// model block, replacing the donor's `PHY2`. Backs `add_model collision: follow_geometry`.
///
/// `new_block` is the block returned by [`inject_static_into_donor_block`]
/// (`[20-byte block header][UCFX container incl. CSUM]`); `mesh` is the imported glTF geometry in
/// MODEL-LOCAL space (the rigid path injects it with `fit_to_template=false`, so its verts are the
/// same frame the render mesh occupies and the frame the engine queries collision in).
///
/// Only the `PHY2` chunk changes: [`build_phy2_multi`] emits one whole-mesh `WpMeshShape16`+MOPP
/// shape (N=1 is byte-identical to the proven single-mesh path), and the donor's 48-byte PHY2 prefix
/// (asset-hash + framing) is preserved except byte-32 (the authored packfile size). `SEGM`, `INDX`,
/// `HIER` and every render chunk stay byte-for-byte identical — collision is not `SEGM`-bound.
/// Partition a mesh into `MeshSoup` chunks of at most `max_tris` triangles each, so every chunk's
/// authored MOPP fits the encoder's 16-bit child-jump budget. Triangles are sorted along the longest
/// bbox axis first, so each chunk is spatially compact (tight AABB → real broadphase pruning). Every
/// chunk re-indexes only the vertices it uses. One chunk (small mesh) returns the whole mesh, so the
/// N=1 path is unchanged.
fn split_mesh_for_mopp(
    tris: &[[u32; 3]],
    positions: &[[f32; 3]],
    max_tris: usize,
) -> Vec<mercs2_formats::phy2_build::MeshSoup> {
    use std::collections::HashMap;
    let n = tris.len();
    if n == 0 {
        return vec![(Vec::new(), Vec::new())];
    }
    let n_chunks = n.div_ceil(max_tris.max(1)).max(1);
    // Longest bbox axis, from triangle centroids.
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let cen: Vec<[f32; 3]> = tris
        .iter()
        .map(|t| {
            let (a, b, c) = (
                positions[t[0] as usize],
                positions[t[1] as usize],
                positions[t[2] as usize],
            );
            let ct = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            for k in 0..3 {
                lo[k] = lo[k].min(ct[k]);
                hi[k] = hi[k].max(ct[k]);
            }
            ct
        })
        .collect();
    let axis = (0..3)
        .max_by(|&a, &b| (hi[a] - lo[a]).partial_cmp(&(hi[b] - lo[b])).unwrap())
        .unwrap();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| cen[a][axis].partial_cmp(&cen[b][axis]).unwrap());

    let chunk_len = n.div_ceil(n_chunks);
    let mut soups = Vec::with_capacity(n_chunks);
    for chunk in order.chunks(chunk_len) {
        let mut vmap: HashMap<u32, u32> = HashMap::new();
        let mut verts: Vec<[f32; 3]> = Vec::new();
        let mut ctris: Vec<[u32; 3]> = Vec::with_capacity(chunk.len());
        for &ti in chunk {
            let t = tris[ti];
            let mut nt = [0u32; 3];
            for k in 0..3 {
                nt[k] = *vmap.entry(t[k]).or_insert_with(|| {
                    verts.push(positions[t[k] as usize]);
                    (verts.len() - 1) as u32
                });
            }
            ctris.push(nt);
        }
        soups.push((ctris, verts));
    }
    soups
}

fn regenerate_prop_collision(
    new_block: &[u8],
    mesh: &mercs2_formats::model_inject::ExternalMesh,
    name: &str,
) -> Result<Vec<u8>, String> {
    use mercs2_formats::phy2_container::{phy2_span_in_container, replace_phy2_in_container};

    if mesh.tris.is_empty() || mesh.positions.is_empty() {
        return Err("injected mesh has no triangles to build collision from".into());
    }
    if new_block.len() < 20 {
        return Err("model block too small".into());
    }
    let ucfx_len = u32::from_le_bytes(new_block[16..20].try_into().unwrap()) as usize;
    let container = new_block
        .get(20..20 + ucfx_len)
        .ok_or("model block truncated before end of UCFX container")?;

    let (pstart, psize) = phy2_span_in_container(container)
        .ok_or("donor container has no PHY2 collision chunk to replace (follow_geometry needs one)")?;
    let base_body = &container[pstart..pstart + psize];
    if base_body.len() < 48 {
        return Err("donor PHY2 body shorter than the 48-byte prefix".into());
    }

    // Build a fresh whole-model PHY2 from the injected geometry as N `WpMeshShape16` shapes sharing
    // one common frame + vertex pool (the proven shared-pool multi-shape path). A single-shape MOPP's
    // child-jump offsets are only 16-bit (`mopp::encode`'s `0x23` split), so a tree over more than
    // ~11 k triangles overflows and only part of it decodes. Splitting into ≤ `MAX_MOPP_TRIS`-triangle
    // shapes keeps every shape's MOPP inside the 16-bit budget while collision still follows the FULL
    // mesh. Shapes bind via the packfile `WpArray`, not `SEGM`, so any shape count is legal on any
    // donor (census-proven). N=1 (a small mesh) stays byte-identical to the proven single-mesh path.
    const MAX_MOPP_TRIS: usize = 8000;
    let soups = split_mesh_for_mopp(&mesh.tris, &mesh.positions, MAX_MOPP_TRIS);
    let authored = mercs2_formats::phy2_build::build_phy2_multi(name, &soups)?;
    if authored.len() < 48 {
        return Err("authored PHY2 body shorter than the 48-byte prefix".into());
    }

    // Preserve the donor's 48-byte PHY2 prefix verbatim except byte-32 (the authored packfile size).
    let mut new_body = Vec::with_capacity(authored.len());
    new_body.extend_from_slice(&base_body[0..48]);
    new_body[32..36].copy_from_slice(&authored[32..36]);
    new_body.extend_from_slice(&authored[48..]);

    let new_container = replace_phy2_in_container(container, pstart, psize, &new_body)?;

    // Rewrap the block header, updating the UCFX length field (offset 16).
    let mut out = new_block[0..20].to_vec();
    out[16..20].copy_from_slice(&(new_container.len() as u32).to_le_bytes());
    out.extend_from_slice(&new_container);
    Ok(out)
}

/// A rigid render group indexes vertices with u16 and draws ONE triangle strip, so a dense mesh
/// whose strip would exceed 65 534 indices cannot be injected as its render geometry. This
/// cluster-decimates a RENDER copy just enough to fit. Collision (`follow_geometry`) is regenerated
/// from the FULL mesh separately, so the visible LOD drops while the collider stays geometry-tight.
///
/// Returns the mesh unchanged when its strip already fits (the common case), else the finest
/// decimation that fits — plus the (verts, tris) it landed on, for the build log.
fn fit_render_mesh_to_u16_strip(
    mesh: &mercs2_formats::model_inject::ExternalMesh,
) -> (mercs2_formats::model_inject::ExternalMesh, Option<(usize, usize)>) {
    use mercs2_formats::model_inject::to_strip_connected;
    const U16_STRIP_MAX: usize = 65534;
    if to_strip_connected(&mesh.tris).len() <= U16_STRIP_MAX {
        return (mesh.clone(), None);
    }
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &mesh.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let mut cell_lo = diag / 8192.0; // fine   -> many verts
    let mut cell_hi = diag / 2.0; // coarse -> few verts
    let mut best = cluster_decimate_ext(mesh, cell_hi);
    // Smallest cell (most detail) whose connected strip still fits.
    for _ in 0..48 {
        let mid = (cell_lo * cell_hi).sqrt();
        let d = cluster_decimate_ext(mesh, mid);
        if to_strip_connected(&d.tris).len() <= U16_STRIP_MAX {
            best = d;
            cell_hi = mid;
        } else {
            cell_lo = mid;
        }
        if cell_hi / cell_lo < 1.02 {
            break;
        }
    }
    let n = (best.positions.len(), best.tris.len());
    (best, Some(n))
}

/// Vertex-cluster decimation on an [`mercs2_formats::model_inject::ExternalMesh`]: quantise positions
/// to a `cell` grid, collapse each occupied cell to its centroid, drop degenerate / zero-area tris,
/// recompute area-weighted normals. UVs are zeroed (this render path serves an untextured prop) and
/// skin arrays dropped (rigid). Mirrors `mesh_prep::cluster_decimate`.
fn cluster_decimate_ext(
    m: &mercs2_formats::model_inject::ExternalMesh,
    cell: f32,
) -> mercs2_formats::model_inject::ExternalMesh {
    use std::collections::HashMap;
    let inv = 1.0 / cell;
    let key = |p: &[f32; 3]| {
        (
            (p[0] * inv).floor() as i64,
            (p[1] * inv).floor() as i64,
            (p[2] * inv).floor() as i64,
        )
    };
    let mut cells: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut sum: Vec<[f64; 3]> = Vec::new();
    let mut cnt: Vec<u32> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(m.positions.len());
    for p in &m.positions {
        let k = key(p);
        let idx = *cells.entry(k).or_insert_with(|| {
            sum.push([0.0; 3]);
            cnt.push(0);
            (sum.len() - 1) as u32
        });
        let i = idx as usize;
        sum[i] = [
            sum[i][0] + p[0] as f64,
            sum[i][1] + p[1] as f64,
            sum[i][2] + p[2] as f64,
        ];
        cnt[i] += 1;
        remap.push(idx);
    }
    let positions: Vec<[f32; 3]> = sum
        .iter()
        .zip(&cnt)
        .map(|(s, &c)| {
            let c = c.max(1) as f64;
            [(s[0] / c) as f32, (s[1] / c) as f32, (s[2] / c) as f32]
        })
        .collect();
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(m.tris.len());
    for t in &m.tris {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        if a == b || b == c || a == c {
            continue;
        }
        let n = cross(
            sub(positions[b as usize], positions[a as usize]),
            sub(positions[c as usize], positions[a as usize]),
        );
        if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] <= 1e-20 {
            continue;
        }
        tris.push([a, b, c]);
    }
    let mut normals = vec![[0.0f32; 3]; positions.len()];
    for t in &tris {
        let (ia, ib, ic) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = cross(sub(positions[ib], positions[ia]), sub(positions[ic], positions[ia]));
        for &i in &[ia, ib, ic] {
            for k in 0..3 {
                normals[i][k] += n[k];
            }
        }
    }
    for nrm in normals.iter_mut() {
        let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if l > 1e-12 {
            nrm[0] /= l;
            nrm[1] /= l;
            nrm[2] /= l;
        } else {
            *nrm = [0.0, 1.0, 0.0];
        }
    }
    let uvs = vec![[0.0f32; 2]; positions.len()];
    mercs2_formats::model_inject::ExternalMesh {
        positions,
        normals,
        uvs,
        tris,
        joints: Vec::new(),
        weights: Vec::new(),
    }
}

/// Lower a rigged `.glb` onto a donor — the SKINNED path, shared by `add_model` and `add_outfit`.
///
/// This used to be a flat refusal (`BuildError::Unsupported`), because everything it needs lived in
/// binary crates: the skinned glTF reader in the Workshop, a second copy in `mercs2_poc`, and the
/// bone mapper alongside them. All three are library code now, so a Shipment can finally ship a
/// character instead of being told the format supports one in principle.
/// The donor's UCFX container: a donor block is `[count][16-byte entry][container]`, and the entry's
/// last word is the container's length.
fn donor_container(donor_blk: &[u8]) -> &[u8] {
    let n = u32::from_le_bytes(donor_blk[16..20].try_into().unwrap_or([0; 4])) as usize;
    donor_blk.get(20..20 + n).unwrap_or(&[])
}

/// A model's own maps, as the manifest names them: the model's name (the texture assets are
/// `<name>_dm` / `_sm` / `_nm`), its `textures:` block, and the Shipment root those paths resolve in.
struct ModelSkin<'a> {
    name: &'a str,
    textures: &'a crate::manifest::Textures,
    root: &'a Path,
}

/// An author's `textures:` maps → one resident texture block each, plus the MTRL repoints that bind
/// them. Shared by the skinned and rigid model lowerings; they differ only in WHICH materials a map
/// replaces, which `froms(slot)` answers with the donor hashes currently at that MTRL slot (slot
/// order `0 = diffuse, 1 = specular, 2 = normal`). `scope` names that set for the error a map with
/// nothing to replace raises.
fn author_texture_repoints(
    index: usize,
    kind: &'static str,
    skin: ModelSkin<'_>,
    scope: &str,
    froms: impl Fn(usize) -> Vec<u32>,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<mercs2_formats::model_inject::MtrlRepoint>), BuildError> {
    let name = skin.name;
    let mut tex_blocks: Vec<PatchBlock> = Vec::new();
    let mut repoints: Vec<mercs2_formats::model_inject::MtrlRepoint> = Vec::new();
    for (slot, suffix, src, is_normal) in [
        (0usize, "dm", &skin.textures.diffuse, false),
        (1, "sm", &skin.textures.specular, false),
        (2, "nm", &skin.textures.normal, true),
    ] {
        let Some(rel) = src else { continue };
        // The historical naming for a model's own maps, spelled at the call site.
        let (block, to) = build_named_texture(
            index,
            kind,
            &format!("{name}_{suffix}"),
            &skin.root.join(rel),
            is_normal,
        )?;
        let slot_froms = froms(slot);
        if slot_froms.is_empty() {
            // Nothing to bind it to. Shipping the texture anyway would look like it worked.
            return Err(BuildError::Lower {
                index,
                kind,
                message: format!(
                    "`textures.{}` was supplied, but the donor names no texture at MTRL slot \
                     {slot} ({suffix}) for {scope} — there is nothing to repoint onto it.",
                    match slot {
                        0 => "diffuse",
                        1 => "specular",
                        _ => "normal",
                    },
                ),
            });
        }
        log.push(format!(
            "contributions[{index}] {kind} {name}: {name}_{suffix} 0x{to:08X} replaces {} donor \
             hash(es) at slot {slot}",
            slot_froms.len()
        ));
        for from in slot_froms {
            repoints.push(mercs2_formats::model_inject::MtrlRepoint { from, to });
        }
        tex_blocks.push(block);
    }
    Ok((tex_blocks, repoints))
}

/// The `MTRL` flags bit a rigid material must carry to sample a texture. Records without it (flags
/// `0x0000`) are flat-shaded and ignore whatever is bound (`docs/modding/field_guide.md`, the MTRL
/// record-count trap; the Workshop's publish path hosts only on such textured groups).
pub const MTRL_TEXTURED: u16 = 0x0080;

/// The rigid path's `textures:` → texture blocks + repoints, over the HOST group's materials.
///
/// Empty `textures:` returns nothing to ship: the prop wears the donor's materials.
fn rigid_texture_repoints(
    index: usize,
    kind: &'static str,
    skin: ModelSkin<'_>,
    donor_ucfx: &[u8],
    donor_name: &str,
    host_group: usize,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<mercs2_formats::model_inject::MtrlRepoint>), BuildError> {
    let t = skin.textures;
    let slots: Vec<usize> = [(0usize, &t.diffuse), (1, &t.specular), (2, &t.normal)]
        .iter()
        .filter(|(_, p)| p.is_some())
        .map(|(s, _)| *s)
        .collect();
    if slots.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let groups = mercs2_formats::texture::group_prmt_material_indices(donor_ucfx);
    let mats = mercs2_formats::texture::parse_mtrl(donor_ucfx);
    let froms = rigid_slot_froms(host_group, &groups, &mats, &slots).map_err(|m| {
        BuildError::Lower {
            index,
            kind,
            message: format!("donor {donor_name}: {m}"),
        }
    })?;
    author_texture_repoints(
        index,
        kind,
        skin,
        &format!("host group {host_group}'s materials in donor {donor_name}"),
        |slot| froms.get(&slot).cloned().unwrap_or_default(),
        log,
    )
}

/// Which donor texture hashes the rigid host group's materials name at each supplied MTRL slot.
///
/// `groups` is `texture::group_prmt_material_indices` (every material each PRMG group's PRMT records
/// name) and `mats` is `texture::parse_mtrl`. A material the repoint touches — one naming a texture
/// at a supplied slot — must carry [`MTRL_TEXTURED`]; any that does not is an error, listed. A slot
/// no host material names maps to an empty list, which the caller refuses with the slot's name.
fn rigid_slot_froms(
    host_group: usize,
    groups: &[Vec<usize>],
    mats: &[mercs2_formats::texture::MtrlMaterial],
    slots: &[usize],
) -> Result<std::collections::BTreeMap<usize, Vec<u32>>, String> {
    let host = groups.get(host_group).ok_or_else(|| {
        format!(
            "host group {host_group} does not exist (the donor has {} PRMG groups)",
            groups.len()
        )
    })?;
    if host.is_empty() {
        return Err(format!("host group {host_group} binds no material (no PRMT record)"));
    }
    let mut out: std::collections::BTreeMap<usize, Vec<u32>> = std::collections::BTreeMap::new();
    let mut untextured: Vec<String> = Vec::new();
    for &m in host {
        let mat = mats.get(m).ok_or_else(|| {
            format!(
                "host group {host_group} names material {m}, but MTRL holds {} records",
                mats.len()
            )
        })?;
        let touched: Vec<(usize, u32)> = slots
            .iter()
            .filter_map(|&s| mat.textures.get(s).copied().filter(|&h| h != 0).map(|h| (s, h)))
            .collect();
        if touched.is_empty() {
            continue;
        }
        if mat.flags & MTRL_TEXTURED == 0 {
            untextured.push(format!("material {m} (flags 0x{:04X})", mat.flags));
            continue;
        }
        for (s, h) in touched {
            let list = out.entry(s).or_default();
            if !list.contains(&h) {
                list.push(h);
            }
        }
    }
    if !untextured.is_empty() {
        return Err(format!(
            "host group {host_group}'s {} lack the textured flag 0x{MTRL_TEXTURED:04X}, so they \
             are flat-shaded and would not sample the supplied map. Pick a `group:` whose \
             materials are textured.",
            untextured.join(", ")
        ));
    }
    Ok(out)
}

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
    // The author's own skin. Empty for `add_model`, which has no `textures:` field.
    textures: &crate::manifest::Textures,
    // Force one draw group + the source's own weights (see `AddOutfit::single_group`). `add_model`
    // passes `false`.
    single_group: bool,
    log: &mut Vec<String>,
) -> Result<(Vec<u8>, Vec<PatchBlock>), BuildError> {
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
                    // Through `asset_hash`, like every other reference in this format: a bare
                    // `0x…` IS the hash, anything else is a name. This called `pandemic_hash_m2`
                    // directly, which made a hash-spelled target unrepresentable — it would be
                    // hashed as the STRING "0x1C2E8837" and miss. That matters here more than
                    // most places: 21 of pmc_hum_mattias's 116 bones have no name in any corpus we
                    // have, so a name-only `bones:` map cannot address them at all and the
                    // Workshop was dropping those rows.
                    let h = crate::manifest::asset_hash(t);
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

    // ── The author's skin ────────────────────────────────────────────────────────────────────
    //
    // `textures:` was parsed and used by nothing: `lower_skinned` passed `repoints: Vec::new()`, so
    // an outfit shipped wearing the DONOR's materials whatever the author supplied. Each supplied
    // map becomes its own resident texture block, and every donor hash currently sitting at that
    // MTRL slot is repointed onto it.
    //
    // The `from` set comes from every material in the container rather than from the host groups:
    // hosts are chosen inside the lowering, after this has to run. Repointing all of them is also
    // the honest reading of one `textures:` block for one outfit, and non-hosts are neutralised.
    let donor_ucfx = donor_container(&donor_blk);
    let (mut tex_blocks, mut repoints) = author_texture_repoints(
        index,
        kind,
        ModelSkin { name, textures, root },
        &format!("any material of donor {donor_name}"),
        |slot| mercs2_formats::texture::material_slot_hashes(donor_ucfx, slot),
        log,
    )?;

    // PER-MATERIAL skins from the GLB's OWN embedded textures. When the author supplies no manual
    // `textures:`, a multi-material import wears each source material on the body region its triangles
    // host — separate head / torso / legs maps instead of one skin smeared over all of it (or the
    // donor's). The blocks + hashes are built here; the per-group repointing happens in
    // `character_into_donor`, where the host of each source part is known. Only materials a part
    // actually references are built.
    let mut part_material_textures: std::collections::HashMap<usize, [Option<u32>; 3]> =
        std::collections::HashMap::new();
    let author_supplied_skin =
        textures.diffuse.is_some() || textures.specular.is_some() || textures.normal.is_some();
    // Per-material skins need one host draw group PER source part, which is exactly what
    // `single_group` collapses away. When it is set the whole mesh shares one material, so building
    // per-material blocks would ship textures nothing repoints onto — skip them.
    if !author_supplied_skin && !single_group {
        let used: std::collections::BTreeSet<usize> =
            glb.parts.iter().filter_map(|p| p.material).collect();
        if !used.is_empty() {
            let mat_tex =
                mercs2_formats::char_import::load_char_material_textures(&root.join(model))
                    .map_err(lower_err)?;

            // A shared FLAT-MATTE specular. The donor's spec map lit the imported UVs with the wrong
            // (Mattias) highlights — the glitchy green blooms. Repointing every host material's spec
            // slot onto one near-black texture makes the import matte, which is right for tactical
            // gear (its glTF specularFactor is ~0.01) and kills the blooms. Built once, shared.
            let flat_spec: Option<u32> = {
                let px = vec![10.0f32; 4 * 4 * 4]; // 4x4 near-black RGBA
                let img = Rgba { width: 4, height: 4, pixels: px };
                match build_texture_from_rgba(index, kind, &format!("{name}_matte_sm"), img, false, None, "matte spec") {
                    Ok((block, to)) => { tex_blocks.push(block); Some(to) }
                    Err(_) => None,
                }
            };

            for &m in &used {
                let Some(tex) = mat_tex.get(m) else { continue };
                let mut slots: [Option<u32>; 3] = [None; 3];
                // DIFFUSE, brightened. Imported maps are authored far darker than the game's dim
                // interior lighting shows (measured ~18% average), so a gamma lift brings them into
                // the visible range. The NORMAL is still held back — shipping the GLB's own turned the
                // model black — so the donor's normal lights it; the flat spec below kills the wrong
                // highlights that produced.
                for (slot, suffix, png, is_normal, gamma) in
                    [(0usize, "dm", &tex.diffuse, false, Some(0.6f32))]
                {
                    let Some(bytes) = png else { continue };
                    let tex_name = format!("{name}_m{m}_{suffix}");
                    match build_named_texture_bytes(index, kind, &tex_name, bytes, is_normal, gamma, &tex_name)
                    {
                        Ok((block, to)) => {
                            slots[slot] = Some(to);
                            tex_blocks.push(block);
                        }
                        // A material whose image is not 4-aligned (or is a codec we can't read) is
                        // skipped, not fatal — that part just keeps the donor's skin.
                        Err(e) => log.push(format!(
                            "contributions[{index}] {kind} {name}: material {m} {suffix} skipped ({e})"
                        )),
                    }
                }
                // Every host material goes matte via the shared flat spec.
                slots[1] = flat_spec;
                if slots.iter().any(|s| s.is_some()) {
                    part_material_textures.insert(m, slots);
                }
            }
            if !part_material_textures.is_empty() {
                log.push(format!(
                    "contributions[{index}] {kind} {name}: shipping {} per-material skin(s) from the \
                     GLB's own embedded textures",
                    part_material_textures.len()
                ));
            }
        }
    }

    // SINGLE-GROUP DIFFUSE ATLAS. Per-material skins need one host group per material, which
    // `single_group` collapses away — so instead bake all the used materials' diffuse maps into ONE
    // atlas texture and let the lowering remap each part's UVs into its cell. That is what makes a
    // MULTI-material import wear its whole correct skin on one draw group, entirely from the GLB's own
    // embedded textures and the manifest — no hand-massaged source asset. Built here so the whole
    // Shipment stays reproducible from `single_group: true` + the original model.
    let mut atlas_cells: std::collections::HashMap<usize, [f32; 4]> = std::collections::HashMap::new();
    if !author_supplied_skin && single_group {
        let used: std::collections::BTreeSet<usize> =
            glb.parts.iter().filter_map(|p| p.material).collect();
        if !used.is_empty() {
            let mat_tex =
                mercs2_formats::char_import::load_char_material_textures(&root.join(model))
                    .map_err(lower_err)?;
            let mut decoded: Vec<(usize, Rgba)> = Vec::new();
            for &m in &used {
                let Some(mt) = mat_tex.get(m) else { continue };
                let Some(bytes) = &mt.diffuse else { continue };
                match read_png_rgba_bytes(bytes, &format!("{name} material {m} diffuse")) {
                    Ok(img) => decoded.push((m, img)),
                    Err(e) => log.push(format!(
                        "contributions[{index}] {kind} {name}: material {m} diffuse skipped ({e})"
                    )),
                }
            }
            if !decoded.is_empty() {
                let (atlas_img, cells) = bake_diffuse_atlas(&decoded);
                let (aw, ah) = (atlas_img.width, atlas_img.height);
                match build_texture_from_rgba(
                    index,
                    kind,
                    &format!("{name}_dm"),
                    atlas_img,
                    false,
                    Some(0.6),
                    &format!("{name}_dm atlas"),
                ) {
                    Ok((block, to)) => {
                        let froms = mercs2_formats::texture::material_slot_hashes(donor_ucfx, 0);
                        for from in froms {
                            repoints.push(mercs2_formats::model_inject::MtrlRepoint { from, to });
                        }
                        tex_blocks.push(block);
                        atlas_cells = cells;
                        // MATTE SPEC. Without this the donor's (Mattias's) specular map lights the
                        // atlas, so the import renders as shiny dark metal — the same wrong-highlight
                        // failure the per-material path kills with a flat spec. Repoint the donor's
                        // whole specular slot onto one near-black 4x4, which is right for tactical
                        // gear (its glTF specularFactor is ~0.01) and matte's out the metal sheen.
                        let matte: Vec<f32> = vec![10.0; 4 * 4 * 4];
                        let matte_img = Rgba { width: 4, height: 4, pixels: matte };
                        if let Ok((sblock, sto)) = build_texture_from_rgba(
                            index,
                            kind,
                            &format!("{name}_matte_sm"),
                            matte_img,
                            false,
                            None,
                            "matte spec",
                        ) {
                            for from in mercs2_formats::texture::material_slot_hashes(donor_ucfx, 1) {
                                repoints.push(mercs2_formats::model_inject::MtrlRepoint {
                                    from,
                                    to: sto,
                                });
                            }
                            tex_blocks.push(sblock);
                        }
                        // FLAT NORMAL. The mesh has ONE UV set, remapped into the atlas for the
                        // DIFFUSE — but the donor's normal map lives in the donor's UV layout, so
                        // sampling it with atlas UVs reads garbage normals and lights whole regions as
                        // if they faced away: the model goes black. Repoint the normal slot onto a flat
                        // tangent-space normal (0,0,1) so lighting falls back to the mesh's own
                        // (conformed, correct) geometry normals instead. `is_normal` swizzles the
                        // input, so a (128,128,255) texel encodes the neutral normal.
                        let flatn: Vec<f32> = [128.0f32, 128.0, 255.0, 255.0]
                            .iter()
                            .cycle()
                            .take(4 * 4 * 4)
                            .copied()
                            .collect();
                        let flatn_img = Rgba { width: 4, height: 4, pixels: flatn };
                        if let Ok((nblock, nto)) = build_texture_from_rgba(
                            index,
                            kind,
                            &format!("{name}_flat_nm"),
                            flatn_img,
                            true,
                            None,
                            "flat normal",
                        ) {
                            for from in mercs2_formats::texture::material_slot_hashes(donor_ucfx, 2) {
                                repoints.push(mercs2_formats::model_inject::MtrlRepoint {
                                    from,
                                    to: nto,
                                });
                            }
                            tex_blocks.push(nblock);
                        }
                        log.push(format!(
                            "contributions[{index}] {kind} {name}: baked a {}x{} diffuse atlas over \
                             {} materials for the single group (0x{to:08X}) + matte spec + flat normal",
                            aw,
                            ah,
                            decoded.len()
                        ));
                    }
                    Err(e) => log.push(format!(
                        "contributions[{index}] {kind} {name}: diffuse atlas skipped ({e:?}); wears \
                         the donor's skin"
                    )),
                }
            }
        }
    }

    // Say what a skin-less outfit actually ships. `wad_simulator` reports this after the fact —
    // "material[N] diffuse 0x… is base-resident but not shipped by the patch (fallback render) …
    // flags fallback-render risk in menu/wardrobe scenes" — and an author who never runs it has no
    // way to know. The materials are the DONOR's, resolved out of the base WAD at runtime, so the
    // outfit depends on those textures being resident wherever it is drawn. When the donor is also
    // the wearer that is nearly always true; when it is not, the wardrobe is where it shows.
    if repoints.is_empty() && part_material_textures.is_empty() {
        log.push(format!(
            "contributions[{index}] {kind} {name}: no `textures:` and no embedded GLB textures — \
             wears donor {donor_name}'s materials, which this patch does not ship. Fine in-world \
             where they are resident; the wardrobe/menu scene is where a fallback render would show. \
             Supply `textures:` or embed maps in the GLB to carry its own."
        ));
    }

    let opts = char_lower::LowerOpts {
        overrides,
        // A model already on the game's own rig keeps the author's weights: there is no fuzzy map
        // to repair, and resampling would discard everything painted on new geometry.
        native_rig: detected == mercs2_formats::retarget::SourceRig::Pandemic,
        repoints,
        part_material_textures,
        single_host: single_group,
        atlas_cells,
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

    // A repoint that matched nothing means the author's skin is in the WAD and nothing wears it.
    // xfer_apply warns here; a Shipment must not ship a silent substitution.
    let dead: Vec<String> = out
        .stats
        .mtrl_repoints
        .iter()
        .filter(|(_, _, n)| *n == 0)
        .map(|(f, t, _)| format!("0x{f:08X} -> 0x{t:08X}"))
        .collect();
    if !dead.is_empty() {
        return Err(lower_err(format!(
            "{} MTRL repoint(s) matched nothing in donor {donor_name}: {}. The texture blocks would              ship and nothing would reference them.",
            dead.len(),
            dead.join(", ")
        )));
    }

    Ok((out.block, tex_blocks))
}

/// Re-emit an edited placement LAYER block as an overlay that shadows the base by PTHS path.
///
/// A layer (vz_state / layers_static) is edited as a whole block — the placement records live in its
/// COMP sub-blocks, patched in place by `placement::patch_transform` / `patch_model`. The overlay
/// carries the edited block at the base's own path, its ASET rows restated verbatim, and the source
/// block index recorded so `build_patch_wad_multi` re-points any block refs. On a no-op the decoded
/// content is byte-identical to the base (the writer's proven property), which is what makes shadowing
/// the whole block safe.
pub fn emit_edited_layer(
    inputs: &crate::game::LayerEditInputs,
    edited_block: &[u8],
) -> Result<PatchBlock, String> {
    let aset: Vec<AsetEntry> = inputs
        .rows
        .iter()
        .map(|r| AsetEntry::new(r.asset_hash, r.secondary_ref, r.packed_block_ref, r.type_id))
        .collect();
    let mut block = PatchBlock::from_decompressed(edited_block, inputs.path.clone(), aset, None)?;
    block.source_block_index = Some(inputs.block_index);
    Ok(block)
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
    // `shipment.name`: an `add_runtime_dll` must be named after it.
    shipment_name: &str,
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
            group,
            textures,
            retarget,
            collision,
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
                // `textures:` is the model's OWN skin. Empty stays the old behaviour — a prop
                // wears the donor's materials, which is right for a prop and was wrong for a novel
                // mesh, the case the field was added for.
                let (new_block, tex_blocks) = lower_skinned(
                    index, kind, name, model, donor_name, rt, root, game, names, textures, false,
                    log,
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
                // The model's MTRL repoints name hashes that only resolve if the skin travels with
                // it, so they ship as one `Blocks` group and cannot be separated by accident.
                if tex_blocks.is_empty() {
                    return Ok(Lowering::Block(block));
                }
                let mut out = vec![block];
                out.extend(tex_blocks);
                return Ok(Lowering::Blocks(out));
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
            // The author's `group:` when given, else the default. The conform bench has always let
            // a host group be picked and had nowhere to record it, so a placement could be
            // previewed and then not expressed.
            let host_group = group.map(|g| g as usize).unwrap_or(DEFAULT_TARGET_GROUP);
            // The rigid render group is u16-indexed and draws one strip, so a dense mesh's strip can
            // exceed 65 534. Fit a RENDER copy to that budget; `follow_geometry` regenerates collision
            // from the FULL `mesh` below, so the collider stays geometry-tight while the visible LOD
            // drops. A mesh that already fits is returned unchanged (`render_decim` = None).
            let (render_mesh, render_decim) = fit_render_mesh_to_u16_strip(&mesh);

            // The model's OWN skin. On the rigid path the host group keeps the donor's material
            // records (the PRMT material index is preserved), so a supplied map replaces the
            // hashes the HOST group's materials name at that slot. A rigid material samples a
            // texture only when its flags carry 0x0080 — a 0x0000 record is flat-shaded and ignores
            // whatever is bound — so a repoint onto one would ship a texture nothing draws, and is
            // refused rather than shipped.
            let (tex_blocks, repoints) = rigid_texture_repoints(
                index,
                kind,
                ModelSkin {
                    name,
                    textures,
                    root,
                },
                donor_container(&donor_blk),
                donor_name,
                host_group,
                log,
            )?;

            // Flags mirror the workshop's proven call: auto-fit OFF (the mesh carries its own
            // transform), target the raw rendered group, neutralise the rest.
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &render_mesh,
                host_group,
                &repoints,
                hash,
                false,
                false,
                false,
                false,
                &[host_group],
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
                 group {host_group}: {} verts, {} tris",
                stats.vertex_count, stats.triangle_count
            ));
            // A repoint that matched nothing means the author's map is in the WAD and nothing
            // wears it — the same refusal the skinned path makes.
            let dead: Vec<String> = stats
                .mtrl_repoints
                .iter()
                .filter(|(_, _, n)| *n == 0)
                .map(|(f, t, _)| format!("0x{f:08X} -> 0x{t:08X}"))
                .collect();
            if !dead.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} MTRL repoint(s) matched nothing in donor {donor_name}: {}. The \
                         texture blocks would ship and nothing would reference them.",
                        dead.len(),
                        dead.join(", ")
                    ),
                });
            }
            if let Some((rv, rt)) = render_decim {
                log.push(format!(
                    "contributions[{index}] add_model {name} 0x{hash:08X}: render LOD decimated to \
                     {rt} tris / {rv} verts to fit the rigid group's u16 strip ({} tris source \
                     preserved for collision)",
                    mesh.tris.len()
                ));
            }

            // OPT-IN: regenerate static collision from the model's OWN injected geometry, replacing
            // the donor's PHY2. Safe on ANY donor — collision is not SEGM-bound (the loader walks the
            // self-contained WpArray of model-local shapes); SEGM/INDX/HIER stay byte-unchanged. The
            // default (`CollisionSource::Donor`) keeps the donor PHY2 verbatim (backward compatible).
            let new_block = match collision {
                crate::manifest::CollisionSource::Donor => new_block,
                crate::manifest::CollisionSource::FollowGeometry => {
                    let regen =
                        regenerate_prop_collision(&new_block, &mesh, name).map_err(|m| {
                            BuildError::Lower {
                                index,
                                kind,
                                message: format!(
                                    "follow_geometry collision for {name} (donor {donor_name}): {m}"
                                ),
                            }
                        })?;
                    log.push(format!(
                        "contributions[{index}] add_model {name} 0x{hash:08X}: collision \
                         follow_geometry — regenerated PHY2 from {} tris, {} verts (SEGM untouched)",
                        mesh.tris.len(),
                        mesh.positions.len()
                    ));
                    regen
                }
            };

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
            // As on the skinned path: the MTRL names hashes only these texture blocks resolve, so
            // they ship as one group.
            if tex_blocks.is_empty() {
                return Ok(Lowering::Block(block));
            }
            let mut out = vec![block];
            out.extend(tex_blocks);
            Ok(Lowering::Blocks(out))
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
            single_group,
            ..
        } => {
            // No model FILE → wear a model the game already ships (named by `name`): nothing to
            // inject here; the `_tOutfits` row is emitted by `script_mutations`. The injection-only
            // fields (donor / textures / retarget / single_group) do not apply.
            let Some(model) = model.as_ref() else {
                return Ok(Lowering::Nothing);
            };
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // `textures:` binds through the SKINNED lowering's MTRL repoints. The rigid path has
            // no repoint step, so an outfit built without `retarget:` wears the donor's materials —
            // which is what silently happened to every `textures:` block before this refusal
            // existed. Refuse rather than substitute.
            if (textures.diffuse.is_some()
                || textures.normal.is_some()
                || textures.specular.is_some())
                && retarget.is_none()
            {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "`textures:` needs the skinned lowering, which is selected by \
                             `retarget:`. The rigid path hosts geometry on the donor and performs \
                             no MTRL repoint, so the outfit would wear the DONOR's skin whatever \
                             you put here. Add `retarget:`, or drop `textures:`."
                        .into(),
                });
            }
            // Auto-pick when omitted (Plan 04 Q2): an outfit's `wearer` NAMES its valid host — the
            // hero whose wardrobe it joins and whose rig it must animate on. `pmc_hum_<wearer>` is
            // that hero's base model, and it is validated against the stack so a wrong pick fails
            // loudly here rather than silently producing an outfit rigged to nothing. `donor:` is
            // still an optional override for a variant host (`pmc_hum_mattias_v3`, …).
            let picked;
            let donor_name = match donor {
                Some(d) => d.as_str(),
                None => {
                    let host = auto_donor_for_wearer(wearer).ok_or_else(|| BuildError::Unsupported {
                        index,
                        kind,
                        reason: format!(
                            "no donor given and `wearer: {wearer}` is not a hero the auto-pick \
                             knows ({}). Name a `donor:` explicitly.",
                            WARDROBE_HEROES.join(", ")
                        ),
                    })?;
                    let hh = crate::manifest::asset_hash(host);
                    if !game.has_asset(hh, TYPE_ID_MODEL) {
                        return Err(BuildError::Lower {
                            index,
                            kind,
                            message: format!(
                                "auto-picked donor {host:?} (0x{hh:08X}) for wearer {wearer} is not \
                                 in the configured game stack — name a `donor:` explicitly"
                            ),
                        });
                    }
                    log.push(format!(
                        "contributions[{index}] add_outfit {name}: donor auto-picked {host} for \
                         wearer {wearer}"
                    ));
                    picked = host.to_string();
                    picked.as_str()
                }
            };

            // SKINNED path — an outfit that animates has to be re-posed onto the donor's rig.
            if let Some(rt) = retarget {
                let (new_block, tex_blocks) = lower_skinned(
                    index, kind, name, model, donor_name, rt, root, game, names, textures,
                    *single_group, log,
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
                log.push(format!(
                    "contributions[{index}] add_outfit {name}: wardrobe row {wearer}/{slug}"
                ));
                // Model block first, then its textures — they must ship together, since the
                // model's MTRL repoints name hashes only these blocks resolve.
                let mut out = vec![block];
                out.extend(tex_blocks);
                return Ok(Lowering::Blocks(out));
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
        // A standalone novel texture. Needs NO game stack: the dimensions and format come from the
        // author's own image rather than from a target the way `replace_texture` does, which makes
        // this (with `raw` and `add_movie`) one of the few kinds that exercises the whole emission
        // contract hermetically — the shape template CI runs in.
        Contribution::AddTexture {
            name,
            image,
            normal_map,
        } => {
            let (block, hash) =
                build_named_texture(index, kind, name, &root.join(image), *normal_map)?;
            log.push(format!(
                "contributions[{index}] add_texture {name} 0x{hash:08X} <- {} ({})",
                root.join(image).display(),
                if *normal_map { "DXT5nm normal" } else { "colour" }
            ));
            Ok(Lowering::Block(block))
        }

        // Encoded from the authored cues with the Shipment's other sound, after every contribution
        // (`sound::lower_shipment_sound`): the block ships to each `load_in` session's WAD, and that
        // session's loader loads it.
        Contribution::AddSound { .. } => Ok(Lowering::Nothing),
        // Lowered together, per bank, after every contribution (`sound::lower_overrides`): several
        // overrides of one bank share one forked soundbank and one override wavebank.
        Contribution::ReplaceSoundBank { .. } | Contribution::ReplaceSoundCue { .. } => {
            if game.is_none() {
                return Err(BuildError::GameRequired { index, kind });
            }
            Ok(Lowering::Nothing)
        }

        Contribution::AddMovie { name, movie } => {
            Ok(Lowering::Block(lower_movie(root, name, movie, index, kind, log)?))
        }

        // add_ui = the Data half of add_movie (this cfx_pack) + a Script half the linker bakes into
        // `qm_modloader` (collected by `ui_registrations`). Its block IS a movie block — same encoder,
        // same primary cfx_pack ASET row — so a hand-authored `add_movie` + trampoline and an `add_ui`
        // ship byte-identical Data. The composition, not a new format, is what makes the movie appear.
        Contribution::AddUi { name, movie } => {
            Ok(Lowering::Block(lower_movie(root, name, movie, index, kind, log)?))
        }

        // Contributes no block: its whole effect is a declared mutation, collected by
        // `script_mutations` and realised at link time.
        Contribution::PatchLua { .. } => Ok(Lowering::Nothing),
        // Realised at link time by `script_additions` + `link_into_blocks`. Nothing to pack here.
        Contribution::AddScript { .. } => Ok(Lowering::Nothing),
        // Realised at link time by `script_replacements` + `link_into_blocks`. Nothing to pack here.
        Contribution::ReplaceLua { .. } => Ok(Lowering::Nothing),

        // Swap a shipped model's PHY2 collision in place. Uses the proven container primitive
        // `phy2_container::replace_phy2_in_container`; the model asset hash is preserved.
        Contribution::ReplacePhy2 { target, phy2 } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);
            let container = game
                .container_for_asset(hash, TYPE_HASH_MODEL, TYPE_ID_MODEL)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{target:?} (0x{hash:08X}) is not a model in the game stack"),
                })?;
            let new_phy2 = std::fs::read(root.join(phy2)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(phy2).display()),
            })?;
            let (pstart, psize) = mercs2_formats::phy2_container::phy2_span_in_container(&container)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("model container for {target:?} has no PHY2 chunk to replace"),
                })?;
            let edited = mercs2_formats::phy2_container::replace_phy2_in_container(
                &container, pstart, psize, &new_phy2,
            )
            .map_err(|m| BuildError::Lower { index, kind, message: m })?;
            log.push(format!(
                "contributions[{index}] replace_phy2 {target} 0x{hash:08X}: {} -> {} bytes",
                container.len(),
                edited.len()
            ));
            Ok(Lowering::Block(opaque_container_block(
                hash,
                TYPE_HASH_MODEL,
                TYPE_ID_MODEL,
                &edited,
                index,
                kind,
            )?))
        }

        // The passthrough kinds below all follow the same shape: read author-supplied bytes,
        // wrap them as a single-entry mod block with the right (type_hash, type_id) ASET row.
        // For each, the format side that would produce those bytes from a source description is
        // future work; the manifest side is real today for anyone who has the encoder.
        Contribution::AddPlacement { layer, entity } => {
            let Some(game) = game else { return Err(BuildError::GameRequired { index, kind }); };
            let ents = parse_entities_file(&root.join(entity)).map_err(|m| BuildError::Lower {
                index, kind, message: format!("{}: {m}", root.join(entity).display()),
            })?;
            lower_layer_append(game, layer, layer, &ents, index, kind)
        }
        Contribution::AddLayer { name, template, entities } => {
            let Some(game) = game else { return Err(BuildError::GameRequired { index, kind }); };
            let ents = parse_entities_file(&root.join(entities)).map_err(|m| BuildError::Lower {
                index, kind, message: format!("{}: {m}", root.join(entities).display()),
            })?;
            lower_layer_append(game, template, name, &ents, index, kind)
        }
        // A new clip needs nothing from retail, so it lowers hermetically.
        Contribution::AddAnimation {
            name,
            clip,
            trnm,
            events,
        } => lower_animation(
            root,
            name,
            AnimationSources {
                clip,
                trnm,
                events: events.as_deref(),
            },
            None,
            index,
            kind,
            log,
        ),
        // A replace reads its target out of retail to check it is a Havok clip.
        Contribution::ReplaceAnimation {
            target,
            clip,
            trnm,
            events,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            lower_animation(
                root,
                target,
                AnimationSources {
                    clip,
                    trnm,
                    events: events.as_deref(),
                },
                Some(game),
                index,
                kind,
                log,
            )
        }
        Contribution::AddShader { name, blob } => opaque_new_asset(root, blob, name, TYPE_HASH_MODEL, TYPE_ID_MODEL, index, kind),
        Contribution::ReplaceShader { target, blob } => opaque_new_asset(root, blob, target, TYPE_HASH_MODEL, TYPE_ID_MODEL, index, kind),
        Contribution::AddFx { name, payload } => opaque_new_asset(root, payload, name, TYPE_HASH_EFFECT, TYPE_ID_EFFECT, index, kind),
        Contribution::ReplaceFx { target, payload } => opaque_new_asset(root, payload, target, TYPE_HASH_EFFECT, TYPE_ID_EFFECT, index, kind),
        Contribution::ReplaceTerrainCell { target, cell } => opaque_new_asset(root, cell, target, TYPE_HASH_TERRAIN_MESH, TYPE_ID_TERRAIN_MESH, index, kind),

        // No Data half: a shop item is pure Script-layer catalog + reward appends (see
        // `script_mutations`), composed by the linker. Nothing to pack into a block.
        Contribution::AddShopItem { .. } => Ok(Lowering::Nothing),

        // Contributes no block either: its effect is a layer registration baked into `qm_modloader`,
        // collected by `layer_registrations` and realised at link time — exactly like `add_ui`'s
        // Script half, but with no Data movie of its own.
        Contribution::ActivateLayer { .. } => Ok(Lowering::Nothing),

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
            // A record-only field: qm carries the sig map through so lint/report can compare it
            // against a live plugin's expected prologue bytes; it does not affect the packed WAD.
            signature_guard: _,
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
                         somebody else's, require the Shipment that ships it in `load.requires` \
                         rather than vendoring their binary.",
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
            if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, kind) {
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

        // A runtime DLL, placed in the game root. The same shape as `native_hook`'s placement —
        // bytes read once, the name refused before anything is written, the PE header checked —
        // with the destination fixed to the game root and the name fixed to `<shipment.name>.dll`.
        // Needs no game stack, so it lowers in template CI.
        Contribution::AddRuntimeDll { dll } => {
            let path = root.join(dll);
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
            // M0162 already blocks these as an Error; this is the belt to its braces.
            if let Some(why) = runtime_dll_name_refusal(&name, shipment_name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}."),
                });
            }
            if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, kind) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be loaded by the game: {why}."),
                });
            }
            let relative = place_path(crate::manifest::PlaceIn::GameRoot.relative_dir(), &name);
            log.push(format!(
                "contributions[{index}] add_runtime_dll {name} → {relative}: {} bytes, sha256 {} — \
                 UNRESTRICTED NATIVE CODE in the game process; the digest proves the bytes are \
                 unmodified, not that they are safe",
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
        // The destruction state machine, EDITED in place. The container is a leaf inside a model
        // block whose ASET row carries a LOD chain, but we do NOT shadow the whole base block: we
        // emit ONLY this model's edited container as a single-entry block, copy its ASET row, and
        // record the source block index. `build_patch_wad_multi` then re-points `_P000` at the new
        // block and remaps or SENTINELS the finer rungs — a sentinel degrades the model to its coarse
        // tier, it does not dangle — so no block-mate is carried and nothing hangs. `states:` is the
        // extracted-then-edited machine (see `crate::states`); `serialize_state_machine` REGENERATES
        // the whole family from it, so an edit may add or remove nodes and states, not only rewrite
        // them. What it cannot make safe is state IDENTITY: a state's hash is the engine's GLOBAL
        // `SetState` address, so an edit that names a state outside the known vocabulary (and outside
        // the model's own base states) is warned about — it ships, but the damage system will never
        // reach that state.
        Contribution::EditStateMachine { target, states } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);
            let inputs = game.model_container_for_edit(hash).ok_or_else(|| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "{target:?} (0x{hash:08X}) is not a model in the configured game stack, or its \
                     block carries no primary container — check the spelling; a name that does not \
                     exist hashes to a lookup that simply misses"
                ),
            })?;

            let base = mercs2_formats::orchestrator::parse_state_machine(&inputs.container)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{target:?} is a model but carries no destruction state machine — \
                         edit_state_machine only applies to destructibles (SWIT/NODE/STAT/CHDR/CEXE)"
                    ),
                })?;

            let text = std::fs::read_to_string(root.join(states)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(states).display()),
            })?;
            let edited_sm = crate::states::parse(&text).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("{}: {m}", root.join(states).display()),
            })?;

            // ── State IDENTITY guard (M0193) ────────────────────────────────────────────────────
            //
            // A state's `name_hash` is the engine's global `SetState` address, not a per-model label.
            // Warn on any edited state whose hash is neither one the base model already used nor a
            // member of the known global vocabulary — it is legal to ship, but the damage system's
            // transitions will never reach it, so the state is effectively dead.
            let base_states: std::collections::BTreeSet<u32> =
                base.nodes.iter().flat_map(|n| n.states.iter().map(|s| s.name_hash)).collect();
            for node in &edited_sm.nodes {
                for st in &node.states {
                    if !base_states.contains(&st.name_hash)
                        && !mercs2_formats::orchestrator::is_known_state(st.name_hash)
                    {
                        log.push(format!(
                            "contributions[{index}] WARNING ({}): state 0x{:08X} is not a known \
                             destruction state and was not in the base model — the engine's SetState \
                             will never transition into it, so it is unreachable. Edit a state's \
                             Enter/Exit scripts rather than renaming it, or use a vocabulary state \
                             (PristineState, DamagedState, DestroyedState, GoneState, …).",
                            crate::lint::M0193_STATE_OFF_VOCABULARY.code,
                            st.name_hash
                        ));
                    }
                }
            }

            let edited = mercs2_formats::orchestrator::serialize_state_machine(
                &inputs.container,
                &edited_sm,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "applying the edited states to {target:?}: {m}. The family is regenerated in \
                     full, so adding or removing nodes and states is fine — start from an extracted \
                     baseline so the shape and state identities are the model's own"
                ),
            })?;

            let n_nodes = edited_sm.nodes.len();
            let n_states: usize = edited_sm.nodes.iter().map(|n| n.states.len()).sum();
            let (b_nodes, b_states): (usize, usize) =
                (base.nodes.len(), base.nodes.iter().map(|n| n.states.len()).sum());
            log.push(format!(
                "contributions[{index}] edit_state_machine {target} 0x{hash:08X}: \
                 {b_nodes}\u{2192}{n_nodes} node(s), {b_states}\u{2192}{n_states} state(s); \
                 container {} -> {} bytes, emitted as a single-model block (finer LOD rungs sentinel \
                 to coarse tier)",
                inputs.container.len(),
                edited.len()
            ));

            // Splice the edited container straight into a single-entry block — it is a full UCFX
            // container (with its recomputed CSUM), not an opaque `data` leaf, so it is placed as-is
            // with the ORIGINAL field_c so the entry matches what the loader expects.
            let mut block_data = Vec::new();
            block_data.extend_from_slice(&1u32.to_le_bytes());
            block_data.extend_from_slice(&hash.to_le_bytes());
            block_data.extend_from_slice(&TYPE_HASH_MODEL.to_le_bytes());
            block_data.extend_from_slice(&inputs.field_c.to_le_bytes());
            block_data.extend_from_slice(&(edited.len() as u32).to_le_bytes());
            block_data.extend_from_slice(&edited);

            // Copy the base model's LOD-chain row verbatim; the builder re-points `_P000` and
            // remaps/sentinels the rest via `source_block_index`.
            let aset = AsetEntry::new(
                hash,
                inputs.secondary_ref,
                inputs.packed_block_ref,
                TYPE_ID_MODEL,
            );
            let mut block = PatchBlock::from_decompressed(
                &block_data,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower { index, kind, message: m })?;
            block.source_block_index = Some(inputs.source_block_index);
            Ok(Lowering::Block(block))
        }

        // Edit a placement LAYER (vz_state / layers_static): move / rotate / re-model its entities in
        // place. Loads the whole layer block from the stack, applies each edit with the proven
        // in-place `placement::patch_*` writer (matched by entity key, or by name via the layer's own
        // Name COMP), and re-emits the block as an overlay shadowing the base path.
        Contribution::EditWorld { layer, edits } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let inputs = game.layer_block_for_edit(layer).ok_or_else(|| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "no layer matching {layer:?} in the game stack — the target is a PTHS-path needle \
                     like \"vz_state_pmccon004\" or \"layers_static\""
                ),
            })?;

            let text = std::fs::read_to_string(root.join(edits)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(edits).display()),
            })?;
            let doc = crate::world::parse(&text).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("{}: {m}", root.join(edits).display()),
            })?;

            // Name → key, from the layer's own placements, so an author can target by entity name.
            let places = mercs2_formats::placement::load_placements(&inputs.block).unwrap_or_default();
            let key_by_name: std::collections::HashMap<&str, u32> = places
                .iter()
                .filter_map(|p| p.name.as_deref().map(|n| (n, p.key)))
                .collect();

            let mut edited = inputs.block.clone();
            let (mut moved, mut reskinned) = (0usize, 0usize);
            for e in &doc.edits {
                // Resolve the target to a Transform record key.
                let key = crate::manifest::bare_hash(&e.entity)
                    .or_else(|| key_by_name.get(e.entity.as_str()).copied())
                    .ok_or_else(|| BuildError::Lower {
                        index,
                        kind,
                        message: format!(
                            "edit_world: entity {:?} is neither a bare 0xKEY nor a name in {layer:?} \
                             — extract a baseline with `qm extract-world` and edit that",
                            e.entity
                        ),
                    })?;
                if e.pos.is_some() || e.quat.is_some() {
                    let n = mercs2_formats::placement::patch_transform(&mut edited, key, e.pos, e.quat);
                    if n == 0 {
                        return Err(BuildError::Lower {
                            index,
                            kind,
                            message: format!(
                                "edit_world: no Transform for entity {:?} (0x{key:08X}) in {layer:?}",
                                e.entity
                            ),
                        });
                    }
                    moved += n;
                }
                if let Some(m) = &e.model {
                    let mh = crate::manifest::asset_hash(m);
                    let n = mercs2_formats::placement::patch_model(&mut edited, key, mh);
                    if n == 0 {
                        return Err(BuildError::Lower {
                            index,
                            kind,
                            message: format!(
                                "edit_world: entity {:?} (0x{key:08X}) has no ModelName to re-model in \
                                 {layer:?}",
                                e.entity
                            ),
                        });
                    }
                    reskinned += n;
                }
            }
            if edited == inputs.block {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} declares no effective change to {layer:?} — an edit_world that moves nothing \
                         would ship an overlay that only restates the base layer",
                        root.join(edits).display()
                    ),
                });
            }
            log.push(format!(
                "contributions[{index}] edit_world {layer}: {} edit(s) \u{2192} {moved} moved, \
                 {reskinned} re-modelled; layer block {} B, shadowed at {}",
                doc.edits.len(),
                inputs.block.len(),
                inputs.path
            ));
            Ok(Lowering::Block(emit_edited_layer(&inputs, &edited).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?))
        }

        // String tables are lowered after the loop, all of a Shipment's writes to one table
        // together and in contribution order ([`merge_string_tables`], strict), so a later
        // contribution sees an earlier one's edits and each table ships as ONE block.
        Contribution::EditStringDb { .. }
        | Contribution::AddStringDbKeys { .. }
        | Contribution::ReplaceStringDbText { .. } => Ok(Lowering::Nothing),

        // A NEW language: `data/<name>.wad` carries its string table (the base table forked, the
        // translation applied, re-keyed to `m2(name)`), its fonts and atlases (forked from the
        // base's, `language::fork_fonts`) and English's voice-over tables re-keyed to
        // `<bank>.<name>` (`language::fork_vo_tables`). The voice stream is copied at deploy.
        Contribution::AddLanguage {
            name,
            display,
            strings,
            base,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // Belt to M0200's braces: never mint a WAD name that shadows a shipped one, even if the
            // rule was suppressed. The `data/` write is only safe because this holds.
            if let Some(why) = language_name_refusal(name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("add_language name {name:?} is not usable: {why}"),
                });
            }

            let base_name = base.as_deref().unwrap_or("english");
            let base_hash = crate::manifest::asset_hash(base_name);
            let container = game
                .container_for_asset(base_hash, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "base table {base_name:?} (0x{base_hash:08X}) is not a string table in the \
                         configured game stack — a new language forks a shipped table, so `base` must \
                         name one that exists (default `english`)"
                    ),
                })?;

            let text = std::fs::read_to_string(root.join(strings)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(strings).display()),
            })?;
            let edits = parse_string_edits(&text)
                .map_err(|m| BuildError::Lower { index, kind, message: m })?;
            if edits.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} declares no strings — a language identical to {base_name} is not a new \
                         language",
                        root.join(strings).display()
                    ),
                });
            }
            let edited = mercs2_formats::stringdb::edit_container(&container, &edits)
                .map_err(|m| BuildError::Lower { index, kind, message: m })?;
            let hash = crate::manifest::asset_hash(name);
            let fail = |message: String| BuildError::Lower { index, kind, message };

            let mut assets = vec![crate::language::Asset {
                name_hash: hash,
                type_hash: TYPE_HASH_STRINGDB,
                type_id: TYPE_ID_STRINGDB,
                container: edited,
            }];
            let fonts = crate::language::fork_fonts(game, base_name, name).map_err(fail)?;
            assets.extend(fonts);
            let main = crate::language::assets_block(format!("blocks\\{name}\\{name}.block"), &assets)
                .map_err(fail)?;

            // English's voice-over tables, one block per re-keyed entry name.
            let vo = crate::language::fork_vo_tables(game, name).map_err(fail)?;
            let mut by_name: std::collections::BTreeMap<u32, Vec<crate::language::Asset>> =
                std::collections::BTreeMap::new();
            for a in vo {
                by_name.entry(a.name_hash).or_default().push(a);
            }
            let mut blocks = vec![main];
            let vo_tables: usize = by_name.values().map(Vec::len).sum();
            for (entry, tables) in &by_name {
                blocks.push(
                    crate::language::assets_block(format!("blocks\\{name}\\vo_{entry:08x}.block"), tables)
                        .map_err(fail)?,
                );
            }
            log.push(format!(
                "contributions[{index}] add_language {name} 0x{hash:08X} ← fork {base_name} \
                 0x{base_hash:08X}: {} key(s) translated, fonts and atlases {}_18/_20, {vo_tables} \
                 voice-over table(s) under {} name(s) → .\\Data\\{name}.wad",
                edits.len(),
                name,
                by_name.len(),
            ));
            Ok(Lowering::LanguageWad {
                language: name.clone(),
                display: display.clone(),
                blocks,
                stream_copy: (
                    crate::language::VO_STREAM_FROM.to_string(),
                    crate::language::vo_stream_to(name),
                ),
            })
        }
    }
}

/// The hero base model that hosts an outfit for `wearer`, or `None` for an unknown hero.
///
/// An outfit's `wearer` is which hero's wardrobe it joins, so that hero's own base model is its
/// only correct donor — the rig and materials it must animate on. This is the auto-pick Plan 04 Q2
/// resolved for `add_outfit`.
///
/// Both `jen` (the Workshop's pill label) and `jennifer` (the `_tOutfits` key the linter checks)
/// map to `pmc_hum_jen`, because the model name follows neither spelling and both reach the same
/// hero. `add_model` has no wearer and gets no auto-pick — a prop's host is geometry-dependent, and
/// the weight-transfer history shows guessing it is how a model ends up rigged to nothing.
fn auto_donor_for_wearer(wearer: &str) -> Option<&'static str> {
    match wearer.trim().to_ascii_lowercase().as_str() {
        "mattias" => Some("pmc_hum_mattias"),
        "chris" => Some("pmc_hum_chris"),
        "jen" | "jennifer" => Some("pmc_hum_jen"),
        _ => None,
    }
}

/// Parse a string-edits file: `[Bracket.Key] = New text` per line, or `[Bracket.Key]: New text`.
///
/// Deliberately line-oriented rather than YAML: the values are UTF-16 UI text that routinely
/// carries `:`, `%s`, quotes and colons, and a YAML parser would demand escaping the very
/// characters the strings are made of. `#` begins a comment; blank lines are skipped. The key is the
/// bracket key verbatim, hashed by the engine's own `pandemic_hash_m2`.
fn parse_string_edits(text: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut out = std::collections::BTreeMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        // The key is either a `[Bracket.Key]` token or a bare `0xHASH` — the same rule
        // `stringdb::edit_container` resolves them by, since a bracket key's reverse is not always
        // known and a dump gives only the hash.
        let (key, rest) = if t.starts_with('[') {
            let close = t
                .find(']')
                .ok_or_else(|| format!("line {}: unterminated `[` key", n + 1))?;
            (t[..=close].to_string(), t[close + 1..].trim_start())
        } else {
            // A `0xHASH key = value`: split on the first `=`/`:`, then the left side is the key.
            let sep = t
                .find(['=', ':'])
                .ok_or_else(|| format!("line {}: expected `=` or `:` after the key", n + 1))?;
            let k = t[..sep].trim();
            let is_hash = k
                .strip_prefix("0x")
                .or_else(|| k.strip_prefix("0X"))
                .is_some_and(|h| !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
            if !is_hash {
                return Err(format!(
                    "line {}: expected a `[Bracket.Key]` or a bare `0xHASH`, got {k:?}",
                    n + 1
                ));
            }
            (k.to_string(), &t[sep..])
        };
        let value = rest
            .trim_start()
            .strip_prefix('=')
            .or_else(|| rest.trim_start().strip_prefix(':'))
            .ok_or_else(|| format!("line {}: expected `=` or `:` after the key", n + 1))?
            .trim();
        out.insert(key, value.to_string());
    }
    Ok(out)
}

/// The refusal for `replace_stringdb_text` pairs whose old text matches no entry: it names the
/// Shipment, the table and every unmatched old text.
fn text_misses_message(shipment: &str, table: &str, misses: &[&String]) -> String {
    format!(
        "{shipment}: in the string table {table}, no entry's text is exactly {} — a replacement that \
         matches nothing changes nothing, so it is refused (check the spelling, and that the text is \
         the table's text at this point in the load order)",
        misses.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>().join(", ")
    )
}

/// Parse a `replace_stringdb_text` pairs file: one `old<TAB>new` pair per line, in file order.
///
/// The format is the kind's documented one (`manifest::Contribution::ReplaceStringDbText`): a line
/// starting with `#` is a comment and a blank line is skipped; every other line is exactly one tab
/// between the old text and the new. Nothing is trimmed or unescaped — the text is compared with the
/// table's text exactly, so a space or a `:` in it is part of it. A trailing `\r` (a CRLF file) is
/// not part of the text. A line with no tab, more than one tab, or an empty old text is refused,
/// naming `file` and the line.
pub(crate) fn parse_text_pairs(text: &str, file: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = format!("{file}:{}", n + 1);
        let mut parts = line.split('\t');
        let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
            return Err(format!("{at}: no tab — each line is `old<TAB>new`"));
        };
        if parts.next().is_some() {
            return Err(format!(
                "{at}: more than one tab — each line is exactly `old<TAB>new`"
            ));
        }
        if old.is_empty() {
            return Err(format!("{at}: the old text is empty — there is nothing to match"));
        }
        out.push((old.to_string(), new.to_string()));
    }
    Ok(out)
}

struct Rgba {
    width: usize,
    height: usize,
    /// Straight RGBA as `f32` in 0..=255, the shape `texture_encode` expects.
    pixels: Vec<f32>,
}

fn read_png_rgba(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_png_rgba_from(file, &path.display().to_string())
}

/// Decode a PNG from an in-memory buffer — the embedded-image path (a GLB carries its textures as
/// PNG bytes in a buffer view, so there is no file to open).
fn read_png_rgba_bytes(bytes: &[u8], label: &str) -> Result<Rgba, String> {
    read_png_rgba_from(std::io::Cursor::new(bytes), label)
}

/// Shared decoder over any reader, so a file path and an embedded buffer share one code path.
fn read_png_rgba_from<R: std::io::Read>(r: R, label: &str) -> Result<Rgba, String> {
    let decoder = png::Decoder::new(r);
    let mut reader = decoder.read_info().map_err(|e| format!("{label}: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{label}: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{label}: only 8-bit PNGs are supported"));
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
/// `out_dir` defaults to `<root>/_build`. Returns `Err(BuildError::Blocked)` rather than a report
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
    if let Some(g) = game.as_deref_mut() {
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

    // Superseded legacy files, before anything is lowered. The probe needs the game folder, so a
    // Shipment that declares `supersedes` cannot build without a game: the check is never skipped.
    if !manifest.supersedes.is_empty() {
        let name = &manifest.shipment.name;
        let Some(vz) = game.as_deref().and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
        else {
            return Err(BuildError::Compat(crate::compat::CompatError::GameRootRequired {
                id: name.clone(),
            }));
        };
        let root = crate::compat::game_root_of(&vz)
            .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?;
        for entry in &manifest.supersedes {
            let present = crate::compat::superseded_present(&root, entry).map_err(|message| {
                BuildError::Compat(crate::compat::CompatError::Probe {
                    id: name.clone(),
                    message,
                })
            })?;
            if present {
                return Err(BuildError::Superseded {
                    shipment: name.clone(),
                    relative: place_path(entry.dest.relative_dir(), &entry.file),
                });
            }
        }
        log.push(format!(
            "supersedes: none of {} legacy file(s) is in the game folder",
            manifest.supersedes.len()
        ));
    }

    // NOTE: self-conflicts are NOT re-checked here. `lint` already reports them as blocking M0120
    // findings, so a Shipment that claims one target twice never reaches this point. A second check
    // would be a redundant path with different formatting — and the first version of it returned on
    // the first conflict, hiding the rest.

    let mut blocks = Vec::new();
    let mut files = Vec::new();
    let mut lang_wads: Vec<(String, String, Vec<PatchBlock>, (String, String))> = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match lower(
            index,
            c,
            &shipment.root,
            game.as_deref_mut(),
            names,
            &manifest.shipment.name,
            &mut log,
        )? {
            Lowering::Nothing => {}
            Lowering::Block(b) => blocks.push(b),
            Lowering::Blocks(bs) => blocks.extend(bs),
            Lowering::LanguageWad {
                language,
                display,
                blocks: bs,
                stream_copy,
            } => lang_wads.push((language, display, bs, stream_copy)),
            Lowering::File {
                name,
                relative,
                bytes,
            } => files.push((name, relative, bytes)),
        }
    }
    // String tables: every edit_stringdb / add_stringdb_keys / replace_stringdb_text of this
    // Shipment, per table, in contribution order — the same code `qm link` merges a set with.
    if let Some((index, c)) = manifest.contributions.iter().enumerate().find(|(_, c)| {
        matches!(
            c,
            Contribution::EditStringDb { .. }
                | Contribution::AddStringDbKeys { .. }
                | Contribution::ReplaceStringDbText { .. }
        )
    }) {
        let Some(game) = game.as_deref_mut() else {
            return Err(BuildError::GameRequired {
                index,
                kind: c.kind(),
            });
        };
        blocks.extend(merge_string_tables(&[shipment], game, StringMerge::Strict, &mut log)?);
    }
    // Sound: every add_sound, replace_sound_bank and replace_sound_cue of this Shipment
    // (`sound::lower_shipment_sound`). The blocks go to the level of each session that loads them —
    // the overlay for gameplay, the shell patch for the front end — and a language's `vo_*` banks to
    // its patch; each session's loader loads the registrations.
    let lowered = crate::sound::lower_shipment_sound(shipment, game.as_deref_mut(), &mut log)
        .map_err(|(index, kind, message)| BuildError::Lower { index, kind, message })?;
    blocks.extend(lowered.overlay);
    let mut shell_blocks: Vec<PatchBlock> = lowered.shell;
    let language_blocks = lowered.language;
    let sound_regs = lowered.registrations;
    let gameplay_sounds = sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay));
    let mutations = script_mutations(manifest, &shipment.root)?;
    let ui_regs = ui_registrations(manifest);
    let layer_regs = layer_registrations(manifest);
    let support_regs = support_registrations(manifest, &shipment.root)?;
    let additions = script_additions(manifest, &shipment.root)?;
    let replacements = script_replacements(manifest, &shipment.root)?;

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
    //
    // `ui_regs` / `layer_regs` / `additions` count too: an add_ui / activate_layer / add_script with
    // no other script edit still needs the linker to run (respectively: mints the loader trampoline,
    // mints the loader trampoline, mints a fresh scripts_vz entry). So a non-empty of any of them
    // must trigger the link even when `mutations` is empty. A bank the front end loads links into
    // `shell.wad`'s scripts block instead (below).
    if !mutations.is_empty()
        || !ui_regs.is_empty()
        || !layer_regs.is_empty()
        || !support_regs.is_empty()
        || gameplay_sounds
        || !additions.is_empty()
        || !replacements.is_empty()
    {
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
                    "linking Lua needs the decompiled corpus (the base source to append to). It \
                          ships in the reference bundle as workshop_data/lua; for `qm`, pass \
                          --corpus <dir> or --workshop-data <dir> (env MERCS2_WORKSHOP_DATA)."
                        .into(),
            });
        };
        let mut loaded = load_script_blocks(game, link::SCRIPT_BLOCKS, "patch_lua")?;
        let mut targets: Vec<link::TargetBlock<'_>> = loaded
            .iter_mut()
            .map(|lb| link::TargetBlock {
                path: lb.path.clone(),
                block: &mut lb.block,
            })
            .collect();
        // A single Shipment has nothing to order against, so the resolved order is itself.
        // Cross-Shipment order is `link_installed`'s job.
        let solo_order = [manifest.shipment.name.clone()];
        let linked = link::link_into_blocks(
            &mut targets,
            corpus,
            &mutations,
            &ui_regs,
            &layer_regs,
            &support_regs,
            &sound_regs,
            &additions,
            &replacements,
            &solo_order,
        )
        .map_err(|e| BuildError::Lower {
            index: 0,
            kind: "patch_lua",
            message: e.to_string(),
        })?;
        drop(targets);
        // M0209 is a load-plan finding, and a single-Shipment build writes no plan; the warning goes
        // to the build log, where `qm link` over the installed set reports it again as a finding.
        for u in &linked.unresolved_imports {
            log.push(format!("warning [M0209]: {u}"));
        }
        let linked = linked.scripts;
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
        // Say what shipping a scripts block COSTS, because the number is not obvious and the
        // failure surfaces far away.
        //
        // A scripts block is claimed whole: the overlay carries every script asset in it, not just
        // the one that was patched. So a second installed mod that also touches scripts overlaps on
        // all of them, and neither contains the other — Modkit reports it as a half-applied mod,
        // and without that check it is the silent mutual annihilation the linter exists for.
        // `qm link` across the installed set is the fix; a standalone overlay cannot be.
        let script_blocks = script_patch_blocks(&loaded, &linked, "patch_lua")?;
        let carried: usize = script_blocks.iter().map(|b| b.aset_entries.len()).sum();
        if carried > 1 {
            log.push(format!(
                "scripts: this overlay claims {carried} script asset(s) — the whole block, not just \
                 the patched script. It is valid on its own, and cannot be installed beside another \
                 script-touching mod until `qm link` rebuilds them together."
            ));
        }
        blocks.extend(script_blocks);
    }

    // The front end's loader, linked into `shell.wad`'s scripts block and shipped in the shell patch.
    if sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::FrontEnd)) {
        let Some(game) = game.as_deref() else {
            return Err(BuildError::GameRequired { index: 0, kind: "front-end loader" });
        };
        let Some(corpus) = corpus_root else {
            return Err(BuildError::Lower {
                index: 0,
                kind: "front-end loader",
                message: "linking the front end's sound loader needs the decompiled corpus (the base \
                          source of `mrxsound`); for `qm`, pass --corpus <dir> or --workshop-data <dir>"
                    .into(),
            });
        };
        let solo_order = [manifest.shipment.name.clone()];
        shell_blocks.extend(link_shell_loader(game, corpus, &sound_regs, &solo_order, "front-end loader", &mut log)?);
    }
    check_sound_loaders(
        &sound_regs,
        &blocks.iter().collect::<Vec<_>>(),
        &shell_blocks.iter().collect::<Vec<_>>(),
        "sound loader",
    )?;

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
        .unwrap_or_else(|| shipment.root.join("_build"));
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

    // Language WADs. A NEW base WAD per `add_language`, placed in `data/` — NOT the Shipment overlay,
    // because the engine opens it by name. Assembled with the same machinery, self-checked before it
    // reaches disk, and recorded as a `Destination::DataWad` so deploy places it in `data/` — the one
    // place a Shipment writes a WAD, earned only by the collision-checked name (`language_name_refusal`).
    for (language, display, lang_blocks, (from, to)) in lang_wads {
        let wad =
            build_patch_wad_multi(&lang_blocks, csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
                BuildError::Lower {
                    index: 0,
                    kind: "add_language",
                    message: m,
                }
            })?;
        let found = verify_emitted(&wad)?;
        for d in &found {
            log.push(format!("self-check ({language}.wad): {d}"));
        }
        diagnostics.extend(found);

        let relative = format!("data/{language}.wad");
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&wad);
        log.push(format!(
            "wrote {relative}: {} bytes, sha256 {digest} → new base language WAD",
            wad.len()
        ));
        placements.push(Placement {
            name: format!("{language}.wad"),
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::DataWad { relative, display },
        });

        // The voice stream the language plays its voice-over from: a copy of English's, made at
        // deploy. Its record carries the digest of the source as read here.
        let vz = game
            .as_deref()
            .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
            .ok_or(BuildError::GameRequired { index: 0, kind: "add_language" })?;
        let game_root = crate::compat::game_root_of(&vz)
            .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?;
        let source = game_root.join(&from);
        let (bytes, sha256) = sha256_file(&source)?;
        log.push(format!(
            "{to} ← copy of {from} at deploy: {bytes} bytes, sha256 {sha256}"
        ));
        placements.push(Placement {
            name: to.rsplit('/').next().unwrap_or(&to).to_string(),
            bytes,
            sha256,
            destination: Destination::StreamCopy { from, to },
        });
    }

    // The shell patch and the language patches: patch WADs a deploy step merges into
    // `data/shell-patch.wad` and `data/<language>-patch.wad`. The shell patch mounts above
    // `shell.wad`, so it carries `shell.wad`'s CSUM row.
    if !shell_blocks.is_empty() {
        let name = format!("{}.shell-patch.wad", manifest.shipment.name);
        let game = game.as_deref().ok_or(BuildError::GameRequired { index: 0, kind: "shell patch" })?;
        placements.push(write_patch_wad(
            &out_dir,
            &name,
            &shell_blocks,
            shell_csum(game, "shell patch")?,
            Destination::ShellPatch,
            &mut log,
            &mut diagnostics,
        )?);
    }
    for (language, lblocks) in language_blocks {
        let relative = format!("language_patch/{}.wad", language.token());
        placements.push(write_patch_wad(
            &out_dir,
            &relative,
            &lblocks,
            csum,
            Destination::LanguagePatch {
                language: language.token().to_string(),
                relative: relative.clone(),
            },
            &mut log,
            &mut diagnostics,
        )?);
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
    write_placement_record(&out_dir, &placements)?;

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

/// The size and sha256 of the file at `path`, read in chunks.
fn sha256_file(path: &Path) -> Result<(usize, String), BuildError> {
    use std::io::Read;
    let io = |e: std::io::Error| BuildError::Io { path: path.to_path_buf(), message: e.to_string() };
    let mut f = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0usize;
    loop {
        let n = f.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n;
    }
    Ok((total, hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()))
}

/// Assemble `blocks` as a patch WAD at `relative` under `out_dir`, self-check it before it reaches
/// disk ([`verify_emitted`]), and record it with its digest under `destination`.
fn write_patch_wad(
    out_dir: &Path,
    relative: &str,
    blocks: &[PatchBlock],
    csum: (u32, Option<u32>),
    destination: Destination,
    log: &mut Vec<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Placement, BuildError> {
    let wad = build_patch_wad_multi(blocks, csum.0, csum.1, &FFCS_CERT_BLOB).map_err(|m| {
        BuildError::Lower {
            index: 0,
            kind: "assemble",
            message: format!("{relative}: {m}"),
        }
    })?;
    let found = verify_emitted(&wad)?;
    for d in &found {
        log.push(format!("self-check ({relative}): {d}"));
    }
    diagnostics.extend(found);
    let path = out_dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
            path: parent.to_path_buf(),
            message: e.to_string(),
        })?;
    }
    std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let digest = sha256_hex(&wad);
    log.push(format!("wrote {relative}: {} bytes, sha256 {digest}", wad.len()));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| BuildError::Io {
            path: path.clone(),
            message: "the output path has no file name".into(),
        })?;
    Ok(Placement {
        name,
        bytes: wad.len(),
        sha256: digest,
        destination,
    })
}

/// The PTHS path of the single-entry block a string table ships in: `blocks\VZ\mod_<hash>.block`.
///
/// One function for both producers of one — a Shipment's own build and the link, both through
/// [`merge_string_tables`] — so the link's merged table shadows the per-Shipment copies at exactly
/// their path.
pub fn stringdb_block_path(table: u32) -> String {
    format!("blocks\\VZ\\mod_{table:08x}.block")
}

/// Wrap a string-table container as its own single-entry block: `[count][name][type][field_c]
/// [size][container]`, one PRIMARY stringdb ASET row. A string table has no LOD chain, so the low 16
/// bits carry the sentinel — anything else would dangle (M0001).
fn stringdb_block(table: u32, container: &[u8]) -> Result<PatchBlock, String> {
    let mut block_data = Vec::with_capacity(20 + container.len());
    block_data.extend_from_slice(&1u32.to_le_bytes()); // entry count
    block_data.extend_from_slice(&table.to_le_bytes());
    block_data.extend_from_slice(&TYPE_HASH_STRINGDB.to_le_bytes());
    block_data.extend_from_slice(&0u32.to_le_bytes());
    block_data.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block_data.extend_from_slice(container);
    let aset = AsetEntry::new(table, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_STRINGDB);
    PatchBlock::from_decompressed(&block_data, stringdb_block_path(table), vec![aset], None)
}

/// Every string table `qm link` merges across a set: the targets of every `edit_stringdb`,
/// `add_stringdb_keys` and `replace_stringdb_text` in it, by asset hash.
pub fn merged_string_tables<'a>(
    manifests: impl IntoIterator<Item = &'a crate::manifest::Manifest>,
) -> std::collections::BTreeSet<u32> {
    manifests
        .into_iter()
        .flat_map(|m| m.contributions.iter())
        .filter_map(|c| match c {
            Contribution::EditStringDb { target, .. }
            | Contribution::AddStringDbKeys { target, .. }
            | Contribution::ReplaceStringDbText { target, .. } => {
                Some(crate::manifest::asset_hash(target))
            }
            _ => None,
        })
        .collect()
}

/// Every block `qm link` re-emits for a set, as the load plan's `link_block_paths` states it: the
/// `vz.wad` scripts blocks ([`link::SCRIPT_BLOCKS`]), the `shell.wad` scripts block
/// ([`link::SHELL_SCRIPT_BLOCKS`]), then each merged string table's block in hash order, then each
/// merged sound bank's block in entry-hash order ([`crate::sound::linked_sound_entries`]; one path
/// for the bank in every WAD that carries it).
/// A deploy step drops the per-Shipment copies of exactly these blocks, because the link WAD carries
/// the set-wide version of each.
pub fn link_block_paths<'a>(
    manifests: impl IntoIterator<Item = &'a crate::manifest::Manifest>,
) -> Vec<String> {
    let manifests: Vec<&crate::manifest::Manifest> = manifests.into_iter().collect();
    link::SCRIPT_BLOCKS
        .iter()
        .chain(link::SHELL_SCRIPT_BLOCKS)
        .map(|(_, p)| p.to_string())
        .chain(merged_string_tables(manifests.iter().copied()).into_iter().map(stringdb_block_path))
        .chain(
            crate::sound::linked_sound_entries(manifests.iter().copied())
                .into_iter()
                .map(crate::sound::block_path),
        )
        .collect()
}

/// How [`merge_string_tables`] treats key writes: the one difference between a Shipment's own
/// build and the set-wide link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringMerge {
    /// `qm build` of ONE Shipment: an `edit_stringdb` key must exist in the table as edited so far,
    /// an `add_stringdb_keys` key must not, and a file with no rows is refused. These are the rules
    /// each kind has always had, now checked in contribution order.
    Strict,
    /// `qm link` of the set: every key write is an upsert, the later winning, with
    /// no existence check — each Shipment's own build already applied [`StringMerge::Strict`].
    Upsert,
}

/// Which kind of key write a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyWrite {
    Edit,
    Add,
}

/// One write to a string table, in order.
enum TableWrite {
    /// `edit_stringdb` / `add_stringdb_keys`: set the key's text. `label` is the key as written.
    Key {
        key: u32,
        label: String,
        text: String,
        write: KeyWrite,
        who: String,
    },
    /// `replace_stringdb_text`: every entry whose CURRENT text is exactly `old` — in the table as
    /// edited so far — gets `new`. Matching nothing is an error naming `who`.
    Text { old: String, new: String, who: String },
}

/// Apply string-table writes, in order, into one table per target. The ONE implementation of
/// string-table lowering: `qm build` calls it for a single Shipment ([`StringMerge::Strict`]) and
/// `qm link` for the whole set ([`StringMerge::Upsert`]).
///
/// Each `edit_stringdb`, `add_stringdb_keys` and `replace_stringdb_text` of each Shipment in
/// `shipments` (for the link, the plan's order), in contribution order, is applied to the base table
/// as edited so far:
///
/// * a key write goes by key hash ([`mercs2_formats::stringdb::key_hash`]); how a missing or an
///   existing key is treated is `mode`'s;
/// * a text replacement changes every entry whose current text is exactly `old`
///   ([`mercs2_formats::stringdb::StringDb::replace_exact_text`]), and matching nothing is an error
///   naming the Shipment, the table and the text, in both modes.
///
/// A later write wins. One block per table comes back ([`stringdb_block`]).
fn merge_string_tables(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    mode: StringMerge,
    log: &mut Vec<String>,
) -> Result<Vec<PatchBlock>, BuildError> {
    let stage: &'static str = match mode {
        StringMerge::Strict => "string tables",
        StringMerge::Upsert => "link",
    };
    let fail = |message: String| BuildError::Lower {
        index: 0,
        kind: stage,
        message,
    };
    let mut by_table: std::collections::BTreeMap<u32, (String, Vec<TableWrite>, Vec<String>)> =
        std::collections::BTreeMap::new();
    for s in shipments {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let (target, file) = match c {
                Contribution::EditStringDb { target, strings }
                | Contribution::AddStringDbKeys { target, strings } => (target, strings),
                Contribution::ReplaceStringDbText { target, pairs } => (target, pairs),
                _ => continue,
            };
            let name = &s.manifest.shipment.name;
            let who = format!("{name} contributions[{index}] ({})", c.kind());
            let path = s.root.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| fail(format!("{who}: reading {}: {e}", path.display())))?;
            // A pairs file for text replacements, a strings file for the key kinds.
            let rows: Vec<(String, String)> = match c {
                Contribution::ReplaceStringDbText { .. } => {
                    parse_text_pairs(&text, &file.display().to_string())
                }
                _ => parse_string_edits(&text).map(|m| m.into_iter().collect()),
            }
            .map_err(|m| fail(format!("{who}: {m}")))?;
            if mode == StringMerge::Strict && rows.is_empty() {
                return Err(fail(format!(
                    "{who}: {} declares nothing — a contribution that changes nothing would ship a \
                     same-hash overlay that only restates the base table",
                    file.display()
                )));
            }
            let entry = by_table
                .entry(crate::manifest::asset_hash(target))
                .or_insert_with(|| (target.clone(), Vec::new(), Vec::new()));
            for (left, right) in rows {
                entry.1.push(match c {
                    Contribution::ReplaceStringDbText { .. } => TableWrite::Text {
                        old: left,
                        new: right,
                        who: who.clone(),
                    },
                    _ => TableWrite::Key {
                        key: mercs2_formats::stringdb::key_hash(&left),
                        label: left,
                        text: right,
                        write: if matches!(c, Contribution::AddStringDbKeys { .. }) {
                            KeyWrite::Add
                        } else {
                            KeyWrite::Edit
                        },
                        who: who.clone(),
                    },
                });
            }
            if !entry.2.contains(name) {
                entry.2.push(name.clone());
            }
        }
    }
    let mut blocks = Vec::with_capacity(by_table.len());
    for (table, (target, writes, contributors)) in by_table {
        let container = game
            .container_for_asset(table, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
            .ok_or_else(|| {
                fail(format!(
                    "{target:?} (0x{table:08X}) is not a string table in the game stack — check the \
                     spelling; there is no base table for {} to edit",
                    contributors.join(", ")
                ))
            })?;
        let merged = mercs2_formats::stringdb::apply_container(&container, |db| {
            for w in &writes {
                match w {
                    TableWrite::Key { key, label, text, write, who } => {
                        let exists = db.set_by_hash(*key, text);
                        match (mode, write, exists) {
                            (StringMerge::Strict, KeyWrite::Edit, false) => {
                                return Err(format!(
                                    "{who}: {label} is not a key in {target} — check the spelling; \
                                     the engine hashes the bracket key verbatim, so an unknown key \
                                     is a lookup that simply misses. To add a key, use \
                                     add_stringdb_keys"
                                ))
                            }
                            (StringMerge::Strict, KeyWrite::Add, true) => {
                                return Err(format!(
                                    "{who}: {label} already exists in {target} — use \
                                     edit_stringdb to overwrite, not add_stringdb_keys"
                                ))
                            }
                            (_, _, true) => {}
                            (_, _, false) => {
                                db.add_by_hash(*key, text);
                            }
                        }
                    }
                    TableWrite::Text { old, new, who } => {
                        if db.replace_exact_text(old, new) == 0 {
                            return Err(text_misses_message(who, &target, &[old]));
                        }
                    }
                }
            }
            Ok(())
        })
        .map_err(|m| fail(format!("{target:?} (0x{table:08X}): {m}")))?;
        log.push(format!(
            "string table {target} 0x{table:08X}: {} write(s) from {} applied in order, container \
             {} -> {} bytes",
            writes.len(),
            contributors.join(", "),
            container.len(),
            merged.len()
        ));
        blocks.push(stringdb_block(table, &merged).map_err(fail)?);
    }
    Ok(blocks)
}

/// The filename of the deploy-time link overlay. Named to sort and read as "last".
pub const LINK_WAD_NAME: &str = "zz-quartermaster-link.wad";

/// The filename of the link's shell patch: the merged sound banks `shell.wad` carries and the
/// front end's scripts block with the set's front-end loader.
pub const LINK_SHELL_PATCH_NAME: &str = "zz-quartermaster-link.shell-patch.wad";

/// What a cross-Shipment link produced.
#[derive(Debug, Clone)]
pub struct LinkReport {
    pub wad: Option<PathBuf>,
    pub placements: Vec<Placement>,
    pub linked: Vec<crate::link::LinkedScript>,
    /// The load plan the link followed — always `ok`, since a plan that is not ok refuses the link
    /// ([`BuildError::Plan`]). Also written to `load-plan.json` beside `placement.json`.
    pub plan: crate::plan::LoadPlan,
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
///
/// **Plan first.** The set's load plan ([`crate::compat::plan`]) is computed before anything is
/// linked: a missing or out-of-range requirement, a declared or claimed conflict, a superseded file
/// still present, or a requirement cycle refuses the link with [`BuildError::Plan`], after writing
/// the plan to `out_dir`. Everything the link orders — append concatenation, `replace_lua`, the
/// `add_script` mint order and the `qm_modloader` bake — follows the plan's `order`, in which a
/// Shipment always comes after the Shipments it requires and the request order breaks ties.
pub fn link_installed(
    inputs: &[crate::compat::PlanInput<'_>],
    game: &mut GameStack,
    corpus_root: &Path,
    out_dir: &Path,
) -> Result<LinkReport, BuildError> {
    // A stale plan must never read as this run's, whatever happens below.
    crate::plan::remove_stale(out_dir).map_err(|message| BuildError::Io {
        path: out_dir.join(crate::plan::PLAN_FILE),
        message,
    })?;
    if game.platform() != Platform::Pc {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    let mut log = Vec::new();
    let mut mutations = Vec::new();

    let root = if crate::compat::needs_game_root(inputs.iter().map(|i| &i.shipment.manifest)) {
        let vz = game.paths()[0].to_path_buf();
        Some(
            crate::compat::game_root_of(&vz)
                .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?,
        )
    } else {
        None
    };
    let mut plan = crate::compat::plan(inputs, crate::plan::Producer::Link, root.as_deref())
        .map_err(BuildError::Compat)?;
    let write_plan = |plan: &crate::plan::LoadPlan| {
        crate::plan::write_plan(out_dir, plan).map_err(|message| BuildError::Io {
            path: out_dir.join(crate::plan::PLAN_FILE),
            message,
        })
    };
    if !plan.ok {
        write_plan(&plan)?;
        return Err(BuildError::Plan(Box::new(plan)));
    }
    // `ok` means no M0174, so the order exists; and no M0203, so each name is one item.
    let order_ids = plan.order.clone().ok_or_else(|| BuildError::Lower {
        index: 0,
        kind: "link",
        message: "internal error: an ok load plan has no order".into(),
    })?;
    let mut shipments: Vec<&LoadedShipment> = Vec::with_capacity(inputs.len());
    for oid in &order_ids {
        let input = inputs.iter().find(|i| i.id == oid.as_str()).ok_or_else(|| BuildError::Lower {
            index: 0,
            kind: "link",
            message: format!("internal error: the load order names {oid}, which is not in the request"),
        })?;
        shipments.push(input.shipment);
    }
    let order: Vec<String> = shipments
        .iter()
        .map(|s| s.manifest.shipment.name.clone())
        .collect();
    if shipments.len() > 1 {
        log.push(format!(
            "load order (requires first, then request order): {}",
            order.join(", ")
        ));
    }

    let mut ui_regs: Vec<link::UiRegistration> = Vec::new();
    let mut layer_regs: Vec<link::LayerRegistration> = Vec::new();
    let mut support_regs: Vec<link::SupportRegistration> = Vec::new();
    let mut additions: Vec<link::ScriptAddition> = Vec::new();
    let mut replacements: Vec<link::ScriptReplacement> = Vec::new();
    let mut sound_regs: Vec<link::SoundBankRegistration> = Vec::new();
    // What each Shipment's own build ships for sound, by level: the loaders' self-check below reads
    // it. Lowered through the one function `qm build` ships with (`sound::lower_shipment_sound`).
    let mut shipped_overlay: Vec<PatchBlock> = Vec::new();
    let mut shipped_shell: Vec<PatchBlock> = Vec::new();
    for s in &shipments {
        let mut sound_log = Vec::new();
        let sound = crate::sound::lower_shipment_sound(s, Some(&mut *game), &mut sound_log).map_err(|(index, kind, message)| {
            BuildError::Lower {
                index,
                kind,
                message: format!("{}: {message}", s.manifest.shipment.name),
            }
        })?;
        sound_regs.extend(sound.registrations);
        shipped_overlay.extend(sound.overlay);
        shipped_shell.extend(sound.shell);
        mutations.extend(script_mutations(&s.manifest, &s.root)?);
        ui_regs.extend(ui_registrations(&s.manifest));
        layer_regs.extend(layer_registrations(&s.manifest));
        support_regs.extend(support_registrations(&s.manifest, &s.root)?);
        additions.extend(script_additions(&s.manifest, &s.root)?);
        replacements.extend(script_replacements(&s.manifest, &s.root)?);
    }
    // A UI, layer, sound-bank, add_script, or replace_lua mod touches the Script layer too — UI,
    // layer and sound-bank registrations mint `qm_modloader` and the trampoline; add_script mints
    // its own fresh scripts_vz entry; replace_lua swaps a shipped script's bytecode. Any of them
    // needs the script link to run.
    let gameplay_sounds = sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay));
    let front_end_sounds = sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::FrontEnd));
    let touches_scripts = !(mutations.is_empty()
        && ui_regs.is_empty()
        && layer_regs.is_empty()
        && support_regs.is_empty()
        && !gameplay_sounds
        && additions.is_empty()
        && replacements.is_empty());
    // Every string table any Shipment edits or adds keys to is merged into one link-owned copy.
    let tables = merged_string_tables(shipments.iter().map(|s| &s.manifest));
    // Every sound bank any Shipment's replace_sound_cue targets, likewise.
    let sound_entries = crate::sound::linked_sound_entries(shipments.iter().map(|s| &s.manifest));
    if !touches_scripts && !front_end_sounds && tables.is_empty() && sound_entries.is_empty() {
        log.push(
            "no installed Shipment touches a script, a string table or a sound bank — nothing to link"
                .into(),
        );
        // Still write the (empty) placement record. Emitting no link WAD is the right call — an
        // overlay that merely restates the base block is a file deploy has to reason about for
        // nothing — but emitting no RECORD makes that indistinguishable from "link was never run",
        // and deploy has to tell those apart. An empty `placements` array says which one it is.
        write_plan(&plan)?;
        write_placement_record(out_dir, &[])?;
        log.push(format!(
            "wrote {}, {PLACEMENT_RECORD}: 0 placement(s) — nothing to mount from the link step",
            crate::plan::PLAN_FILE
        ));
        return Ok(LinkReport {
            wad: None,
            placements: Vec::new(),
            linked: Vec::new(),
            plan,
            log,
        });
    }
    log.push(format!(
        "linking {} mutation(s), {} UI, {} layer and {} sound-bank registration(s), {} string \
         table(s) and {} sound bank(s) from {} Shipment(s)",
        mutations.len(),
        ui_regs.len(),
        layer_regs.len(),
        sound_regs.len(),
        tables.len(),
        sound_entries.len(),
        shipments.len()
    ));

    let mut patches: Vec<PatchBlock> = Vec::new();
    let mut linked: Vec<link::LinkedScript> = Vec::new();
    if touches_scripts {
        let mut loaded = load_script_blocks(game, link::SCRIPT_BLOCKS, "link")?;
        let mut targets: Vec<link::TargetBlock<'_>> = loaded
            .iter_mut()
            .map(|lb| link::TargetBlock {
                path: lb.path.clone(),
                block: &mut lb.block,
            })
            .collect();
        let linked_out = link::link_into_blocks(
            &mut targets,
            corpus_root,
            &mutations,
            &ui_regs,
            &layer_regs,
            &support_regs,
            &sound_regs,
            &additions,
            &replacements,
            &order,
        )
        .map_err(|e| BuildError::Lower {
            index: 0,
            kind: "link",
            message: e.to_string(),
        })?;
        drop(targets);
        // M0209: literal imports nothing in the link provides. Warnings — `ok` does not change —
        // placed on the item whose source carries them. An ok plan has one item per name (no M0203).
        for u in &linked_out.unresolved_imports {
            let requested = inputs
                .iter()
                .position(|i| i.shipment.manifest.shipment.name == u.shipment)
                .ok_or_else(|| BuildError::Lower {
                    index: 0,
                    kind: "link",
                    message: format!(
                        "internal error: {} carries an import but is not in the request",
                        u.shipment
                    ),
                })?;
            plan.findings.push(crate::plan::Finding {
                code: "M0209",
                severity: crate::plan::FindingSeverity::Warning,
                message: u.to_string(),
                items: vec![inputs[requested].id.to_string()],
                refs: vec![crate::plan::FindingRef {
                    section: crate::plan::Section::Items,
                    index: requested,
                }],
                fix: None,
            });
            log.push(format!("warning [M0209]: {u}"));
        }
        crate::plan::sort_findings(&mut plan.findings, |item| {
            inputs.iter().position(|p| p.id == item)
        });
        linked = linked_out.scripts;
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
        patches.extend(script_patch_blocks(&loaded, &linked, "link")?);
    }

    // The merged string tables. The plan's `link_block_paths` promised exactly these, and a deploy
    // step drops the per-Shipment copies of each on that promise, so a mismatch is an internal error.
    let table_blocks = merge_string_tables(&shipments, game, StringMerge::Upsert, &mut log)?;
    let sound_paths: std::collections::BTreeSet<String> =
        sound_entries.iter().map(|&e| crate::sound::block_path(e)).collect();
    let promised: Vec<String> = plan
        .link_block_paths
        .iter()
        .filter(|p| {
            !link::SCRIPT_BLOCKS.iter().chain(link::SHELL_SCRIPT_BLOCKS).any(|(_, s)| s == p) && !sound_paths.contains(*p)
        })
        .cloned()
        .collect();
    let emitted: Vec<String> = table_blocks.iter().map(|b| b.path_string.clone()).collect();
    if promised != emitted {
        return Err(BuildError::Lower {
            index: 0,
            kind: "link",
            message: format!(
                "internal error: the plan promised the string-table blocks {promised:?}, and the \
                 link merged {emitted:?}"
            ),
        });
    }
    patches.extend(table_blocks);

    // The merged sound banks: one soundbank per bank a replace_sound_cue targets, carrying every
    // Shipment's cue overrides, in each WAD that carries the bank.
    let mut shell_blocks: Vec<PatchBlock> = Vec::new();
    let mut language_blocks: std::collections::BTreeMap<crate::manifest::Language, Vec<PatchBlock>> =
        std::collections::BTreeMap::new();
    if !sound_entries.is_empty() {
        let lowered = crate::sound::lower_overrides(
            &shipments,
            game,
            crate::sound::OverrideScope::Link,
            &mut log,
        )
        .map_err(|message| BuildError::Lower { index: 0, kind: "link", message })?;
        let merged: std::collections::BTreeSet<String> = lowered
            .overlay
            .iter()
            .chain(&lowered.shell)
            .chain(lowered.language.values().flatten())
            .map(|b| b.path_string.clone())
            .collect();
        if merged != sound_paths {
            return Err(BuildError::Lower {
                index: 0,
                kind: "link",
                message: format!(
                    "internal error: the plan promised the sound-bank blocks {sound_paths:?}, and \
                     the link merged {merged:?}"
                ),
            });
        }
        patches.extend(lowered.overlay);
        shell_blocks = lowered.shell;
        language_blocks = lowered.language;
    }

    // The front end's loader: every Shipment's front-end banks in one `qm_shell_modloader`, linked
    // into `shell.wad`'s scripts block and shipped in the link's shell patch.
    shell_blocks.extend(link_shell_loader(game, corpus_root, &sound_regs, &order, "link", &mut log)?);
    // The loaders load only what the set's builds ship, level by level.
    check_sound_loaders(
        &sound_regs,
        &shipped_overlay.iter().collect::<Vec<_>>(),
        &shipped_shell.iter().collect::<Vec<_>>(),
        "link",
    )?;

    let csum =
        mercs2_formats::donor::base_csum(game.paths()[0]).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "link",
            message: m,
        })?;
    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let mut placements = Vec::new();
    let mut diagnostics = Vec::new();
    if !shell_blocks.is_empty() {
        placements.push(write_patch_wad(
            out_dir,
            LINK_SHELL_PATCH_NAME,
            &shell_blocks,
            shell_csum(game, "link")?,
            Destination::ShellPatch,
            &mut log,
            &mut diagnostics,
        )?);
    }
    for (language, lblocks) in language_blocks {
        let relative = format!("language_patch/{}.wad", language.token());
        placements.push(write_patch_wad(
            out_dir,
            &relative,
            &lblocks,
            csum,
            Destination::LanguagePatch {
                language: language.token().to_string(),
                relative: relative.clone(),
            },
            &mut log,
            &mut diagnostics,
        )?);
    }
    if patches.is_empty() {
        write_plan(&plan)?;
        write_placement_record(out_dir, &placements)?;
        log.push(format!(
            "wrote {}, {PLACEMENT_RECORD}: {} placement(s), no link overlay",
            crate::plan::PLAN_FILE,
            placements.len()
        ));
        return Ok(LinkReport {
            wad: None,
            placements,
            linked,
            plan,
            log,
        });
    }
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

    // The same record `build` writes, for the same reason: this WAD has to be MOUNTED, and where
    // it must sit in the mount order is not recoverable from the file itself. The name encodes the
    // intent ("sorts last") but a deploy step reading a directory should not have to infer a
    // contract from a filename — `destination: overlay` in the record is the contract.
    placements.insert(
        0,
        Placement {
            name: LINK_WAD_NAME.to_string(),
            bytes: wad_bytes.len(),
            sha256: digest,
            destination: Destination::Overlay,
        },
    );
    write_plan(&plan)?;
    write_placement_record(out_dir, &placements)?;
    log.push(format!(
        "wrote {}, {PLACEMENT_RECORD}: {} placement(s)",
        crate::plan::PLAN_FILE,
        placements.len()
    ));

    Ok(LinkReport {
        wad: Some(path),
        placements,
        linked,
        plan,
        log,
    })
}

/// The file name every `qm` output directory carries. There is exactly one.
pub const PLACEMENT_RECORD: &str = "placement.json";

/// The `placement.json` format this build writes. Its destination kinds are `overlay`,
/// `game_folder`, `data_wad`, `language_patch`, `shell_patch` and `stream_copy` ([`Destination`]).
pub const PLACEMENT_FORMAT: u32 = 2;

/// Write `placement.json` into `out_dir`, creating it if needed.
///
/// # Why every emitting path calls this
///
/// `build` wrote a placement record and `link_installed` did not, so a deploy tool had **two**
/// contracts for the same question: read the record for one output directory, and special-case
/// `zz-quartermaster-link.wad` by name for the other. A second output path nobody documented is
/// how a deploy step ends up guessing, and a guess about which files to mount is not a guess that
/// fails loudly — it mounts the wrong set and the game boots.
///
/// It is also written when `placements` is EMPTY, which is the case that actually needed
/// deciding. An absent file is ambiguous between "this step produced nothing" and "this step
/// never ran", and those want opposite responses from a deploy tool. `{"placements": []}` says
/// the first one outright.
fn write_placement_record(out_dir: &Path, placements: &[Placement]) -> Result<(), BuildError> {
    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let path = out_dir.join(PLACEMENT_RECORD);
    std::fs::write(&path, placement_json(placements)).map_err(|e| BuildError::Io {
        path,
        message: e.to_string(),
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
                Destination::DataWad { relative, display } => {
                    serde_json::json!({ "kind": "data_wad", "relative": relative, "display": display })
                }
                Destination::LanguagePatch { language, relative } => {
                    serde_json::json!({ "kind": "language_patch", "language": language, "relative": relative })
                }
                Destination::ShellPatch => serde_json::json!({ "kind": "shell_patch" }),
                Destination::StreamCopy { from, to } => {
                    serde_json::json!({ "kind": "stream_copy", "from": from, "to": to })
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
        "format": PLACEMENT_FORMAT,
        "placements": entries,
    }))
    .unwrap_or_else(|_| "{}".into())
        + "\n"
}

#[cfg(test)]
mod existing_model_outfit {
    use crate::manifest::Contribution;

    /// An `add_outfit` with no `model` file is the "wear an existing in-game model" form: it parses
    /// with `model: None`, injects nothing, yet still emits the `_tOutfits` wardrobe row — the
    /// residency-safe Script mutation the linker reconciles.
    #[test]
    fn add_outfit_without_a_model_file_still_appends_the_wardrobe_row() {
        let yaml = r#"
format: 2
shipment:
  name: wear-solano
  version: "1.0.0"
  target: retail
contributions:
  - kind: add_outfit
    name: vz_hum_solano
    slug: Solano
    display: Solano
    wearer: chris
"#;
        let m = crate::from_str(yaml, crate::Format::Yaml).expect("parses");
        match &m.contributions[0] {
            Contribution::AddOutfit { model, name, .. } => {
                assert!(model.is_none(), "no model file -> existing-model outfit");
                assert_eq!(name, "vz_hum_solano");
            }
            other => panic!("wrong kind: {}", other.kind()),
        }

        // The Script half is unchanged: a wardrobe row on wifpmcinterior for the existing model.
        let muts = super::script_mutations(&m, std::path::Path::new(".")).unwrap();
        assert_eq!(muts.len(), 1);
        assert_eq!(muts[0].target, "wifpmcinterior");
        assert!(muts[0].append.contains("_tOutfits.chris"));
        assert!(muts[0].append.contains("vz_hum_solano"));
    }
}

#[cfg(test)]
mod text_pairs_tests {
    use super::parse_text_pairs;

    /// Free text on both sides, exactly as written: spaces, `:`, `=`, `[` and `%s` are text.
    #[test]
    fn pairs_are_old_tab_new_verbatim() {
        let text = "# a comment\n\nPress Start: to begin\tPress [A] = %s\r\n  \n Leading space\t\n";
        assert_eq!(
            parse_text_pairs(text, "src/p.txt"),
            Ok(vec![
                ("Press Start: to begin".to_string(), "Press [A] = %s".to_string()),
                (" Leading space".to_string(), String::new()),
            ])
        );
    }

    #[test]
    fn a_malformed_line_names_the_file_and_line() {
        for (text, line, why) in [
            ("ok\tfine\nno tab here\n", 2, "no tab"),
            ("a\tb\tc\n", 1, "more than one tab"),
            ("# c\n\tnew\n", 2, "old text is empty"),
        ] {
            let e = parse_text_pairs(text, "src/p.txt").unwrap_err();
            assert!(e.starts_with(&format!("src/p.txt:{line}: ")), "{e}");
            assert!(e.contains(why), "{e}");
        }
    }
}

#[cfg(test)]
mod donor_tests {
    use super::auto_donor_for_wearer;

    #[test]
    fn each_hero_maps_to_its_own_base_model() {
        assert_eq!(auto_donor_for_wearer("mattias"), Some("pmc_hum_mattias"));
        assert_eq!(auto_donor_for_wearer("chris"), Some("pmc_hum_chris"));
        // Both the wardrobe key and the UI pill reach the same hero model.
        assert_eq!(auto_donor_for_wearer("jennifer"), Some("pmc_hum_jen"));
        assert_eq!(auto_donor_for_wearer("jen"), Some("pmc_hum_jen"));
        // Case- and space-insensitive, since it runs on author input.
        assert_eq!(auto_donor_for_wearer("  Mattias "), Some("pmc_hum_mattias"));
        assert_eq!(auto_donor_for_wearer("bulldog"), None);
    }
}

#[cfg(test)]
mod rigid_texture_tests {
    use super::{rigid_slot_froms, MTRL_TEXTURED};
    use mercs2_formats::texture::MtrlMaterial;

    fn mat(flags: u16, textures: &[u32]) -> MtrlMaterial {
        MtrlMaterial {
            textures: textures.to_vec(),
            flags,
            preamble: Vec::new(),
        }
    }

    /// The host group's materials, and only theirs, supply the hashes a map replaces — per slot,
    /// deduplicated. Material 2 belongs to another group and must not contribute.
    #[test]
    fn froms_come_from_the_host_groups_materials_only() {
        let mats = [
            mat(MTRL_TEXTURED, &[0xA0, 0xA1, 0xA2]),
            mat(MTRL_TEXTURED | 0x0008, &[0xB0, 0xA1, 0xB2]),
            mat(MTRL_TEXTURED, &[0xC0, 0xC1, 0xC2]),
        ];
        let groups = vec![vec![2], vec![0, 1]];
        let froms = rigid_slot_froms(1, &groups, &mats, &[0, 1]).unwrap();
        assert_eq!(froms.get(&0), Some(&vec![0xA0, 0xB0]));
        assert_eq!(froms.get(&1), Some(&vec![0xA1]), "a shared hash is listed once");
        assert_eq!(froms.get(&2), None, "an unsupplied slot is not repointed");
    }

    /// A touched host material without the textured flag is flat-shaded: the map would ship and
    /// never draw. That is a hard error naming the material, not a skipped material.
    #[test]
    fn an_untextured_host_material_is_a_hard_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0]), mat(0x0000, &[0xB0])];
        let groups = vec![vec![0, 1]];
        let e = rigid_slot_froms(0, &groups, &mats, &[0]).unwrap_err();
        assert!(e.contains("material 1") && e.contains("0x0080"), "{e}");
    }

    /// An untextured material the repoint does NOT touch (nothing at the supplied slot) is fine.
    #[test]
    fn an_untouched_untextured_material_is_not_an_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0, 0xA1, 0xA2]), mat(0x0000, &[0xB0])];
        let groups = vec![vec![0, 1]];
        let froms = rigid_slot_froms(0, &groups, &mats, &[2]).unwrap();
        assert_eq!(froms.get(&2), Some(&vec![0xA2]));
    }

    #[test]
    fn a_missing_host_group_or_material_is_an_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0])];
        assert!(rigid_slot_froms(3, &[vec![0]], &mats, &[0]).unwrap_err().contains("does not exist"));
        assert!(rigid_slot_froms(0, &[vec![]], &mats, &[0]).unwrap_err().contains("no material"));
        assert!(rigid_slot_froms(0, &[vec![5]], &mats, &[0]).unwrap_err().contains("material 5"));
    }
}
