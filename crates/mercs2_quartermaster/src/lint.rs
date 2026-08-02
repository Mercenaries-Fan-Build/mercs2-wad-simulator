//! The linter — numbered, documented, gated diagnostics.
//!
//! Plan 01 calls this the crown jewel, and the reason is that **our memory index is a rule set**:
//! every entry is a trap a modder cannot discover on their own. Each rule here carries an `Mxxxx`
//! code, a doc link, and — where the fix is mechanical — the exact replacement text.
//!
//! ## What runs where
//!
//! [`lint`] is **hermetic**: manifest text plus, optionally, the Shipment directory. No game
//! install, no network. That is what lets template CI run `qm lint` on every push when the retail
//! WADs will never be available there.
//!
//! [`game_checks`] is the separate set that needs the retail WADs. Kept apart deliberately — folding
//! them together would make the hermetic set impossible to run on its own, and CI is the place the
//! linter matters most.
//!
//! [`artifact_checks`] is the third stage, and runs against the WAD the builder just emitted. It is
//! the only stage that can catch a defect the LOWERING introduced rather than one the author wrote,
//! which is the class of bug that has actually shipped here.
//!
//! Several of the worst traps still cannot be checked at all — the non-resident-costume wedge needs
//! a residency predicate that does not exist yet, and the non-square `page_count` livelock rests on
//! RE that is still open. Those are registered in [`PENDING`] rather than silently absent, so the
//! gap is visible instead of being mistaken for a clean bill of health.
//!
//! ## Gating
//!
//! [`blocks_build`] is the build gate. `Hang` and `Error` block; `Warning` and `Info` do not. The
//! standing mandate is that a build is gated on EXIT CODE, never on a printed count.

use crate::blast::{self, MergeClass};
use crate::discover::{self, SourceIssue};
use crate::game::GameStack;
use crate::manifest::{Contribution, Manifest, Requirement, Target};
use crate::names::{self, NameTable};
use std::path::Path;

/// How bad it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    /// The mod will not work, or will corrupt something.
    Error,
    /// The game will HANG or crash. These are the silent-and-catastrophic class the linter exists
    /// for — a modder gets no error message from the game, just a frozen loading screen.
    Hang,
}

/// Where the modding docs are published.
///
/// Diagnostics print a URL rather than a path. The paths in [`Rule::doc`] resolve against a checkout
/// of the notes repo, which a modder reading `qm lint` output in CI does not have and has no reason
/// to — so `— see docs/aset_format.md` was an instruction to go find a file that, for them, does not
/// exist anywhere.
pub const DOC_BASE: &str =
    "https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/";

/// A rule: stable code, one-line title, and where the trap is written up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub code: &'static str,
    pub title: &'static str,
    /// Path within the notes repo, with an optional `#anchor`. Use [`Rule::url`] to show it to
    /// anyone — the raw path is only meaningful to someone who has that repo checked out.
    pub doc: &'static str,
}

impl Rule {
    /// The published URL for this rule's write-up.
    pub fn url(&self) -> String {
        format!("{DOC_BASE}{}", self.doc)
    }
}

// --- Implemented, hermetic -------------------------------------------------

pub const M0100_MANIFEST_INVALID: Rule = Rule {
    code: "M0100",
    title: "manifest fails schema validation",
    doc: "docs/modding/manifest_format.md",
};
pub const M0110_SOURCE_MISSING: Rule = Rule {
    code: "M0110",
    title: "a referenced source file does not exist",
    doc: "docs/modding/manifest_format.md#folder-layout",
};
pub const M0111_SOURCE_ESCAPES: Rule = Rule {
    code: "M0111",
    title: "a source path leaves the Shipment root",
    doc: "docs/modding/manifest_format.md#folder-layout",
};
pub const M0112_SOURCE_OUTSIDE_SRC: Rule = Rule {
    code: "M0112",
    title: "a source file is not under src/",
    doc: "docs/modding/manifest_format.md#folder-layout",
};
pub const M0120_SELF_CONFLICT: Rule = Rule {
    code: "M0120",
    title: "two contributions in one Shipment claim the same target",
    doc: "docs/modding/field_guide.md#trap-14--mods-fight-each-other-and-produce-a-chimera",
};
pub const M0130_BARE_HASH: Rule = Rule {
    code: "M0130",
    title: "a hash was written where a name is known",
    doc: "docs/modding/field_guide.md#trap-1--your-mod-didnt-load-and-there-is-no-error",
};
pub const M0140_UNKNOWN_WEARER: Rule = Rule {
    code: "M0140",
    title: "outfit targets a hero the wardrobe has no list for",
    doc: "docs/modding/field_guide.md#trap-15--wardrobe--skins-it-is-pure-lua-and-only-named-models-work",
};
pub const M0141_UNMERGEABLE_SCRIPT: Rule = Rule {
    code: "M0141",
    title: "patching a script whose composition is not reversed makes the Shipment exclusive",
    doc: "docs/modding/field_guide.md#trap-15--wardrobe--skins-it-is-pure-lua-and-only-named-models-work",
};
pub const M0150_RAW_NO_TOUCHES: Rule = Rule {
    code: "M0150",
    title: "a raw contribution declares no blast radius",
    doc: "docs/modding/manifest_format.md#composition",
};
pub const M0160_ASI_ON_REIMPL: Rule = Rule {
    code: "M0160",
    title: "an ASI plugin was attached to a reimpl target",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0161_HOOK_DOES_NOTHING: Rule = Rule {
    code: "M0161",
    title: "a native_hook supplies neither a plugin nor a symbol",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0162_PLACED_FILE_REFUSED: Rule = Rule {
    code: "M0162",
    title: "a placed file's name is one no Shipment may write into the game folder",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0163_COMPANION_NOT_BESIDE_PLUGIN: Rule = Rule {
    code: "M0163",
    title: "a companion file is not in the directory the plugin will look for it in",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0170_BAD_DIGEST: Rule = Rule {
    code: "M0170",
    title: "an external requirement's sha256 is not a 64-character hex digest",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0171_INSECURE_URL: Rule = Rule {
    code: "M0171",
    title: "an external requirement is fetched over an untrusted transport",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
pub const M0190_MOVIE_CARRIES_AS3: Rule = Rule {
    code: "M0190",
    title: "an added movie carries AS3 bytecode, which the GFx 2.0.48 runtime cannot execute",
    doc: "docs/reverse_engineer/scaleform_gfx_class_map.md#1-sdk-version--settled",
};

/// String tables retail serves from BOTH `shell.wad` and `vz.wad`, per
/// `docs/fixpack/wad_duplicate_inventory.md` §C. The language tables carry the shared UI chrome
/// (button prompts, options, PDA), so editing one copy is a half-fix. A DLC module table
/// (`english_dlc01`) lives in one place and is not on this list.
pub const SHARED_STRING_TABLES: &[&str] = &[
    "english", "french", "german", "italian", "spanish", "japanese",
];

pub const M0191_SHARED_STRING_TABLE: Rule = Rule {
    code: "M0191",
    title: "editing one copy of a string table shared between shell.wad and vz.wad is a half-fix",
    doc: "docs/modding/manifest_format.md#edit_stringdb",
};
pub const M0192_MOVIE_UNREFERENCED: Rule = Rule {
    code: "M0192",
    title: "an added movie's name matches no shipped movie, so nothing references it",
    doc: "docs/modding/manifest_format.md#add_movie",
};

/// An `edit_state_machine` names a state whose hash is neither one the base model used nor a member
/// of the cracked global vocabulary. The engine's `SetState`/`SetStateOnMsg` key on that global
/// hash, so a novel one is unreachable — the state ships but the damage system never enters it.
/// Surfaced from the lowering (it needs both the game stack and the states file), not `game_checks`.
pub const M0193_STATE_OFF_VOCABULARY: Rule = Rule {
    code: "M0193",
    title: "an edited destruction state is outside the global SetState vocabulary — unreachable",
    doc: "docs/modding/manifest_format.md#edit_state_machine",
};

/// An `activate_layer` names a layer with no `type_id 9` (layer) ASET row in the game stack, so the
/// runtime `MrxLayerManager.MarkForAddition`/`MarkForRemoval` reaches nothing — the activation ships
/// but is a no-op. Advisory, like M0192: a companion `edit_world`/`raw` in the same install MAY ship
/// the layer, which this cannot see. Needs the game stack, so it lives in [`game_checks`].
pub const M0194_LAYER_UNKNOWN: Rule = Rule {
    code: "M0194",
    title: "an activated world layer is not in the game stack — MarkForAddition reaches nothing",
    doc: "docs/modding/manifest_format.md#activate_layer",
};

/// Needs the game stack — see [`game_checks`], not [`lint`].
pub const M0007_MULTI_RUNG_REPLACE: Rule = Rule {
    code: "M0007",
    title: "fully-resident replacement of a MULTI-RUNG texture stops it streaming",
    doc: "docs/aset_format.md",
};

/// Needs the game stack. The shared-texture case: retail carries the asset only as a sub-entry.
pub const M0009_NO_PRIMARY_ROW: Rule = Rule {
    code: "M0009",
    title: "replacing a texture that has no primary ASET row mints one, capturing every sharer",
    doc: "docs/modernization/texture_extraction_notes.md",
};

/// Every hermetic rule this build implements.
pub const RULES: &[Rule] = &[
    M0100_MANIFEST_INVALID,
    M0110_SOURCE_MISSING,
    M0111_SOURCE_ESCAPES,
    M0112_SOURCE_OUTSIDE_SRC,
    M0120_SELF_CONFLICT,
    M0130_BARE_HASH,
    M0140_UNKNOWN_WEARER,
    M0141_UNMERGEABLE_SCRIPT,
    M0150_RAW_NO_TOUCHES,
    M0160_ASI_ON_REIMPL,
    M0161_HOOK_DOES_NOTHING,
    M0162_PLACED_FILE_REFUSED,
    M0163_COMPANION_NOT_BESIDE_PLUGIN,
    M0170_BAD_DIGEST,
    M0171_INSECURE_URL,
    M0190_MOVIE_CARRIES_AS3,
    M0191_SHARED_STRING_TABLE,
];

// --- Known, NOT yet implemented -------------------------------------------

/// HANG-class traps that need the game stack or a built WAD, and so cannot run in a hermetic lint.
///
/// **Registered on purpose.** A linter that silently omits its most important rules reads as a
/// clean bill of health, which is worse than no linter. These land with the builder (increment 5),
/// where the WAD stack is in hand.
pub const PENDING: &[Rule] = &[
    // M0001, M0002, M0003 and M0004 have moved to `ARTIFACT_RULES` — all four are answerable
    // against the emitted WAD.
    Rule {
        code: "M0005",
        title: "non-resident costume on the on-demand path — STATE_WAITFORGAME wedge",
        doc: "docs/modding/field_guide.md#trap-12--your-character-skin-hangs-the-wardrobe-preview-a-count-field-not-a-crash",
    },
    // MEASURED (F7, 2026-08-01, retail vz.wad via the simulator's fan-in map): 678 referenced
    // hashes are shared by more than one asset, one by 400. So this is NOT redundant with M0009 —
    // that fires on ASET-row STRUCTURE (no primary row), while collateral reskin is about material
    // FAN-IN, and the two do not coincide. The measurement mechanism now exists (`SimulateReport::
    // xref_fan_in`); the rule itself is the remaining delta — an artifact check that flags a
    // replace_texture whose base target has fan-in > 1.
    Rule {
        code: "M0006",
        title: "replace_texture target is shared by several materials — collateral reskin",
        doc: "docs/modding/field_guide.md#trap-6--a-surface-renders-the-wrong-texture-or-props-look-missing",
    },
    // Found via corpus_search 2026-07-25, not from first principles.
    Rule {
        code: "M0008",
        title: "small / non-square texture may hit the open page_count buffer-sizing livelock",
        doc: "docs/reverse_engineer/render_core_code_map.md",
    },
];

/// A texture's ASET row is single-block only when BOTH LOD halves are sentinel — the row names up
/// to four rungs, not one (`docs/aset_format.md`, proven 2026-07-21):
///
/// ```text
/// _P000 -> packed_block_ref hi16 (always present)
/// _P001 -> packed_block_ref lo16   sentinel 0xFFFF
/// _P002 -> secondary_ref    hi16   sentinel 0xFFFF
/// _P003 -> secondary_ref    lo16   sentinel 0xFFFF
/// ```
///
/// This is the predicate M0007 needs: a world texture keeps its finer mips as lone `BODY` chunks in
/// finer c3-cell blocks (`externalTextures`), so replacing it with ONE fully-resident block shadows
/// the row and orphans those rungs. Character textures are already fully resident and are unaffected
/// — which is why the first end-to-end test (`al_hum_boss_ub`) passed without tripping this.
pub fn aset_row_is_single_block(packed_block_ref: u32, secondary_ref: u32) -> bool {
    packed_block_ref & 0xFFFF == 0xFFFF && secondary_ref == 0xFFFF_FFFF
}

/// Rules that need the retail WADs. Separate from [`lint`] on purpose: everything there runs in CI
/// with no game, and mixing the two would make the hermetic set impossible to run alone.
pub fn game_checks(manifest: &Manifest, game: &GameStack) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::ReplaceTexture { target, .. } = c {
            let hash = crate::manifest::asset_hash(target);
            // Use EVERY row, not just the primary one: a shared texture may have no primary row at
            // all, and looking only for one would silently skip exactly those assets.
            let rows = game.aset_rows(hash, mercs2_formats::types::TYPE_ID_TEXTURE);
            let Some(&(packed, secondary, _)) = rows.first() else {
                continue;
            };

            if !aset_row_is_single_block(packed, secondary) {
                let rungs = [packed & 0xFFFF, secondary >> 16, secondary & 0xFFFF]
                    .iter()
                    .filter(|r| **r != 0xFFFF)
                    .count();
                out.push(Diagnostic {
                    rule: M0007_MULTI_RUNG_REPLACE,
                    severity: Severity::Warning,
                    message: format!(
                        "{target} is a STREAMED texture whose row names {rungs} finer rung(s) \
                         besides the resident one (packed 0x{packed:08X}, secondary \
                         0x{secondary:08X}). Retail keeps those mips as separate BODY chunks in \
                         finer c3-cell blocks. This replacement is one fully-resident block, so \
                         those rungs stop being named: the texture no longer streams and ships its \
                         whole chain inline. That is how HERO textures already work \
                         (pmc_hum_* rows are single-block), but here it changes residency and \
                         resident size. Structurally valid — verify in-game before shipping."
                    ),
                    at: Some(index),
                    fix: None,
                });
            }

            if !rows.iter().any(|(_, _, primary)| *primary) {
                out.push(Diagnostic {
                    rule: M0009_NO_PRIMARY_ROW,
                    severity: Severity::Warning,
                    message: format!(
                        "{target} has NO primary ASET row — retail carries it as a shared \
                         sub-entry inside another asset's block, and the engine resolves it by \
                         falling back to any type_id 27 row. This replacement mints a primary row, \
                         which then wins the lookup. That is what makes the replacement take \
                         effect, but it also means every asset that shares this texture now gets \
                         your version."
                    ),
                    at: Some(index),
                    fix: None,
                });
            }
        }

        // A movie whose name matches a shipped cfx_pack is a REPLACEMENT — the proven UI-mod path:
        // same name -> same hash -> last-wins, and every Lua site that already names it now serves
        // yours. A NOVEL name mints a movie the game references from nowhere, so it will sit in the
        // WAD and never display. Advisory, because a Shipment MAY add a movie it also wires up with
        // its own patch_lua — this cannot see that the reference was added, only that retail has none.
        if let Contribution::AddMovie { name, .. } = c {
            let hash = crate::manifest::asset_hash(name);
            if !game.has_asset(hash, mercs2_formats::types::TYPE_ID_CFX_PACK) {
                out.push(Diagnostic {
                    rule: M0192_MOVIE_UNREFERENCED,
                    severity: Severity::Warning,
                    message: format!(
                        "no shipped movie is named {name:?}, so nothing in the game references it \
                         yet — the engine binds a movie to a `FlashWidget` by NAME. To REPLACE a \
                         shipped movie use its exact name; to ADD a new one that appears on screen, \
                         use `add_ui` instead of `add_movie` — it ships this same movie AND bakes \
                         the `FlashWidget` that plays it into the mod loader for you. (By hand it is \
                         an `add_movie` plus a `patch_lua` doing `w = FlashWidget:new(); \
                         w:SetSwfFile(<name>); w:Play(); w:SetVisible(true)`, the way `mrxgui.lua` \
                         loads `loadingscreen_standalone`.) Without one of these the movie sits in \
                         the WAD unshown."
                    ),
                    at: Some(index),
                    fix: None,
                });
            }
        }

        // activate_layer marks a layer at runtime; if no layer-typed (type_id 9) ASET row carries
        // that name, the mark reaches nothing and the activation is a silent no-op. Same advisory
        // shape as M0192: a companion edit_world/raw MAY ship the layer, which this cannot see.
        if let Contribution::ActivateLayer { layer, replaces } = c {
            for name in std::iter::once(layer).chain(replaces.iter()) {
                let hash = crate::manifest::asset_hash(name);
                if !game.has_asset(hash, mercs2_formats::types::TYPE_ID_LAYER) {
                    out.push(Diagnostic {
                        rule: M0194_LAYER_UNKNOWN,
                        severity: Severity::Warning,
                        message: format!(
                            "no layer named {name:?} is in the game stack (no type_id 9 ASET row), \
                             so `MrxLayerManager.MarkForAddition`/`MarkForRemoval` reaches nothing \
                             at runtime — the activation ships but does nothing. Layer names are \
                             CASE-SENSITIVE; a `vz_state_*` name must match retail exactly. If a \
                             companion `edit_world` or `raw` in this install ships this layer, \
                             ignore this."
                        ),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
        }
    }
    out
}

/// M0001, promoted out of [`PENDING`]. Answerable only against an emitted WAD.
pub const M0001_DANGLING_RUNG: Rule = Rule {
    code: "M0001",
    title: "dangling _P001/2/3 LOD rungs — 549 GB buffer request, open-world stream HANG",
    doc: "docs/modding/field_guide.md#trap-7--your-reskin-makes-the-game-hang-on-the-loading-screen-not-crash--hang",
};

/// M0002, promoted out of [`PENDING`]. Answerable only against an emitted WAD.
pub const M0002_PACKED_FIELD_UNDER_CLAIM: Rule = Rule {
    code: "M0002",
    title: "packed_field under-claims decompressed size — heap overrun",
    doc: "docs/modding/field_guide.md#trap-8--you-edited-a-block-and-now-the-heap-is-corrupt-the-packedfield-bug",
};

/// M0003, promoted out of [`PENDING`]. Answerable only against an emitted WAD: the INFO/BODY pair
/// this rule compares does not exist until lowering has encoded one.
pub const M0003_TEXTURE_BODY_SHORT: Rule = Rule {
    code: "M0003",
    title: "texture BODY shorter than linear_mip_chain_size — BUFFER_TOO_SMALL, world-load livelock",
    doc: "docs/modding/field_guide.md#trap-7--your-reskin-makes-the-game-hang-on-the-loading-screen-not-crash--hang",
};

/// M0004, promoted out of [`PENDING`]. Answerable only against an emitted WAD: it is a set
/// difference between two tables that only both exist once the WAD is assembled.
pub const M0004_NO_ASET_ROW: Rule = Rule {
    code: "M0004",
    title: "new asset hash minted without an ASET row — loader wedges silently at world-load",
    doc: "docs/modding/field_guide.md#trap-1--your-mod-didnt-load-and-there-is-no-error",
};

/// M0180: a hash claimed by two blocks. Not HANG-class — the registry is first-writer-wins, so the
/// outcome is defined — but one of the two contributions silently does nothing.
pub const M0180_DUPLICATE_PRIMARY: Rule = Rule {
    code: "M0180",
    title: "two blocks claim one asset hash — the later one is silently dropped",
    doc: "docs/modding/manifest_format.md#composition",
};

/// M0181: the WAD's header region outgrew the 2 MB below DATA.
pub const M0181_HEADER_OVERFLOW: Rule = Rule {
    code: "M0181",
    title: "INDX+ASET+PTHS overflow the patch-WAD header region",
    doc: "docs/modding/manifest_format.md#limits",
};

/// M0182: a block the builder emitted will not inflate. Always a builder bug, never an author one.
pub const M0182_BLOCK_UNREADABLE: Rule = Rule {
    code: "M0182",
    title: "an emitted block does not decompress",
    doc: "docs/modding/manifest_format.md#limits",
};

/// Every rule answerable only against an emitted WAD — see [`artifact_checks`].
pub const ARTIFACT_RULES: &[Rule] = &[
    M0001_DANGLING_RUNG,
    M0002_PACKED_FIELD_UNDER_CLAIM,
    M0003_TEXTURE_BODY_SHORT,
    M0004_NO_ASET_ROW,
    M0180_DUPLICATE_PRIMARY,
    M0181_HEADER_OVERFLOW,
    M0182_BLOCK_UNREADABLE,
];

/// Rules that can only be answered against the WAD the builder just emitted.
///
/// A third stage, after [`lint`] (hermetic) and [`game_checks`] (needs the retail stack). These are
/// the checks that catch a defect the LOWERING introduced rather than one the author wrote — the
/// class of bug that has actually shipped here twice (a bare container emitted where an entry-table
/// block was required, and an ASET rung left at `0x0000` instead of the `0xFFFF` sentinel). Neither
/// was visible in the manifest; both were visible in the bytes.
///
/// Pass the blocks as read back by `read_patch_wad`, whose rungs are in the emitted WAD's own index
/// space — that is what makes the M0001 answer meaningful. See `patch_wad::BlockStage`.
pub fn artifact_checks(blocks: &[mercs2_formats::patch_wad::PatchBlock]) -> Vec<Diagnostic> {
    use mercs2_formats::patch_wad::{validate_blocks_all, BlockFinding, BlockStage};

    let mut out: Vec<Diagnostic> = validate_blocks_all(blocks, BlockStage::Emitted)
        .into_iter()
        .map(|finding| {
            let (rule, severity) = match &finding {
                BlockFinding::DanglingLodRung { .. } => (M0001_DANGLING_RUNG, Severity::Hang),
                BlockFinding::PackedFieldUnderClaim { .. } => {
                    (M0002_PACKED_FIELD_UNDER_CLAIM, Severity::Hang)
                }
                BlockFinding::DuplicatePrimary { .. } => {
                    (M0180_DUPLICATE_PRIMARY, Severity::Warning)
                }
                BlockFinding::HeaderOverflow { .. } => (M0181_HEADER_OVERFLOW, Severity::Error),
                BlockFinding::Sges { .. } => (M0182_BLOCK_UNREADABLE, Severity::Error),
            };
            Diagnostic {
                rule,
                severity,
                message: finding.to_string(),
                // A block cannot be traced back to the contribution that produced it: lowering may
                // merge several into one (the linked scripts block) or split one into several.
                at: None,
                fix: None,
            }
        })
        .collect();

    out.extend(texture_body_checks(blocks));
    out.extend(unreachable_hash_checks(blocks));
    out
}

/// A block's payload as the engine sees it after inflation.
///
/// `None` when it will not inflate — that is M0182's finding, already reported by
/// [`mercs2_formats::patch_wad::validate_blocks_all`], so the entry-table rules stay quiet on it
/// rather than adding a second, less informative complaint about the same block.
fn inflated(blk: &mercs2_formats::patch_wad::PatchBlock) -> Option<Vec<u8>> {
    if blk.compressed_data.len() >= 4 && &blk.compressed_data[0..4] == b"sges" {
        mercs2_formats::sges::decompress_sges(&blk.compressed_data).ok()
    } else {
        // A stored block: the engine does not inflate it, so its bytes are the payload.
        Some(blk.compressed_data.clone())
    }
}

/// Walk a block as `[entry table][containers…]`, but only when it actually IS one.
///
/// Every block this crate emits and every block it carries out of retail has that shape, and
/// `parse_block_entry_table` reads the first word as a count unconditionally — so handing it an
/// opaque payload yields a garbage count and, from there, confidently wrong findings. Requiring the
/// walk to complete (every declared entry parsed, every container in bounds) is what makes the
/// difference between "this block has no unreachable hashes" and "this block is not an entry-table
/// block", and only the first is something to report on.
fn coherent_block(raw: &[u8], label: &str) -> Option<mercs2_formats::ucfx::ParsedBlock> {
    let (parsed, _issues) = mercs2_formats::ucfx::walk_decompressed_block(raw, label);
    let complete = parsed.entries.len() == parsed.entry_count as usize
        && parsed.containers.len() == parsed.entries.len();
    complete.then_some(parsed)
}

/// M0003 — a texture BODY shorter than the mip chain the engine will read out of it.
///
/// The predicate is [`wad_simulator::texture::check_embedded_texture_buffers`], which pairs each
/// `INFO` descriptor with the `BODY`/`DXT1` that follows it and defers to
/// `texture_buffer_too_small`. It is **wrapped, not reimplemented**: that function carries two
/// gates verified against retail — streamed textures legitimately ship a short resident tail
/// (9,562 of them in `vz.wad`), and the chain is sized from the CLAIMED mip count rather than the
/// full dimension chain — and a second copy of the predicate is a second copy of those gates to
/// keep in step. Without them the rule fires on almost every texture in the game.
///
/// The artifact stage is the only one that can answer this. The hermetic stage has a PNG and a
/// target name; `INFO` and `BODY` do not exist until lowering has encoded them, and the defect this
/// catches is one the ENCODER introduces — a claimed mip count the body does not cover.
fn texture_body_checks(blocks: &[mercs2_formats::patch_wad::PatchBlock]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for blk in blocks {
        let Some(raw) = inflated(blk) else { continue };
        let Some(parsed) = coherent_block(&raw, &blk.path_string) else {
            continue;
        };
        for (i, container) in parsed.containers.iter().enumerate() {
            let label = format!("{} entry[{i}]", blk.path_string);
            // Runs over EVERY container, not just `type_hash == TEXTURE`: a model or layer
            // container that embeds a texture never gets a texture dispatch of its own, so gating
            // on the entry's type would skip exactly the case that has no other check.
            let (issues, _violations) =
                wad_simulator::texture::check_embedded_texture_buffers(container, &label);
            for message in issues {
                out.push(Diagnostic {
                    rule: M0003_TEXTURE_BODY_SHORT,
                    severity: Severity::Hang,
                    message,
                    at: None,
                    fix: None,
                });
            }
        }
    }
    out
}

/// M0004 — an asset in a block that no ASET row names, so nothing can ask for it.
///
/// The forward direction of the question `aset_validate` answers backwards: that one asks whether
/// every ASET row has a block behind it, this one asks whether every asset in a block has a row in
/// front of it. `block_internal_hashes − aset_hashes`.
///
/// **The invariant is retail-verified, not assumed.** Across `vz.wad`'s 11,370 blocks there are
/// 55,429 entry-table rows covering 30,006 distinct hashes, and **every one of them has an ASET
/// row** — 30,645 rows, zero orphans. Sub-resources meant to be reached through their parent are
/// not the exception: they get a row too, a non-primary one. So an entry with no row anywhere is a
/// shape the shipping game never takes.
///
/// It is HANG-class because of what happens next: the field guide's Trap 1 records the heli
/// experiment, where a minted asset without its row (block index + sub `0xFFFF` + type id) made the
/// world load simply never complete — no crash, no log line.
///
/// Scoped to the emitted WAD on purpose. An overlay's rows shadow retail's per hash, so "no row in
/// this WAD" is the answerable question; whether retail happens to carry a row for the same hash
/// would need the game stack, and a carried-donor hash that retail still names resolves to retail's
/// copy rather than wedging. In practice this does not weaken the rule: the blocks this crate
/// carries out of retail are single-entry, and the linked scripts block mints a row per entry.
fn unreachable_hash_checks(blocks: &[mercs2_formats::patch_wad::PatchBlock]) -> Vec<Diagnostic> {
    let aset_hashes: std::collections::HashSet<u32> = blocks
        .iter()
        .flat_map(|b| b.aset_entries.iter().map(|e| e.asset_hash))
        .collect();

    let mut out = Vec::new();
    let mut reported = std::collections::HashSet::new();
    for blk in blocks {
        let Some(raw) = inflated(blk) else { continue };
        let Some(parsed) = coherent_block(&raw, &blk.path_string) else {
            continue;
        };
        for entry in &parsed.entries {
            // A zero name_hash is padding, never an asset.
            if entry.name_hash == 0 || aset_hashes.contains(&entry.name_hash) {
                continue;
            }
            if !reported.insert(entry.name_hash) {
                continue;
            }
            out.push(Diagnostic {
                rule: M0004_NO_ASET_ROW,
                severity: Severity::Hang,
                message: format!(
                    "block {} carries asset 0x{:08X} (type 0x{:08X}) but no ASET row in this WAD \
                     names it, so nothing can resolve it by hash. Retail never ships this shape — \
                     all 30,006 asset hashes in vz.wad's blocks have a row. An asset minted without \
                     one does not fail loudly: the world load stops completing and the game sits on \
                     the loading screen.",
                    blk.path_string, entry.name_hash, entry.type_hash
                ),
                at: None,
                fix: None,
            });
        }
    }
    out
}

/// M0190 — an `add_movie` payload carrying ActionScript 3.
///
/// **The runtime is AVM1 only.** The embedded middleware is Scaleform GFx **2.0.48**, targeting
/// Flash 8 / AS2, proven three ways in the unpacked exe: the `gfxVersion` property returns the
/// literal `"2.0.48"`, the loader carries `incompatible GFX file, version 2.x expected`, and the
/// builtin class registrar installs the AS2 class table with no AVM2 anywhere. GFx 2.x has no
/// `DoABC` tag loader at all.
///
/// So an AS3 movie does not fail — it **loads**. The tag is unknown, so it is skipped; the shapes,
/// text and timeline all render, and not one line of the movie's logic ever runs. Nothing is logged,
/// because from the loader's point of view nothing went wrong. That is the exact silent-no-op class
/// this linter exists for, which is why it blocks rather than warns.
///
/// Retail corroborates the direction: across all 64 `cfx_pack` assets in `vz.wad`, `DoABC` appears
/// zero times.
///
/// A movie that cannot be read at all stays silent HERE on purpose. The lowering refuses it with a
/// message about what a `.gfx` is supposed to look like, and that is a better place to say so than a
/// rule about AS3 — a rule that reported "no AS3 found" for a file that is not a movie would be
/// answering a question nobody asked.
fn movie_checks(index: usize, name: &str, path: &Path) -> Vec<Diagnostic> {
    let Ok(bytes) = std::fs::read(path) else {
        // M0110 already reports a missing source; an unreadable one is not this rule's business.
        return Vec::new();
    };
    let Ok(movie) = mercs2_formats::gfx::GfxMovie::parse(&bytes) else {
        return Vec::new();
    };
    let features = movie.features();
    if features.do_abc == 0 {
        return Vec::new();
    }
    vec![Diagnostic {
        rule: M0190_MOVIE_CARRIES_AS3,
        severity: Severity::Error,
        message: format!(
            "{} carries {} DoABC tag(s) — ActionScript 3. The game embeds Scaleform GFx 2.0.48, \
             which is AVM1/AS2 only and has no DoABC loader, so the tag is skipped as unknown: the \
             movie loads, {name} renders, and none of its script ever runs. Nothing is logged, \
             because as far as the loader is concerned nothing failed. None of the 64 movies retail \
             ships carries AS3. Re-author the logic as AS2 (AVM1).",
            path.display(),
            features.do_abc
        ),
        at: Some(index),
        fix: None,
    }]
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: Rule,
    pub severity: Severity,
    pub message: String,
    /// Index into `contributions`, when the finding belongs to one.
    pub at: Option<usize>,
    /// Exact replacement text, when the fix is mechanical.
    pub fix: Option<String>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Hang => "HANG",
        };
        write!(f, "[{}] {sev}: ", self.rule.code)?;
        if let Some(i) = self.at {
            write!(f, "contributions[{i}]: ")?;
        }
        write!(f, "{}", self.message)?;
        if let Some(fix) = &self.fix {
            write!(f, " (fix: {fix})")?;
        }
        write!(f, " — see {}", self.rule.url())
    }
}

/// The wardrobe's hero keys, verified against `wifpmcinterior.lua:155`. An outfit filed under any
/// other key sits in a table nothing ever reads.
/// The wearer spellings the linter SUGGESTS — the preferred set (`jen`, not `jennifer`). Validity
/// is a separate question: `manifest::wearer_table_key` accepts either spelling, since `jennifer`
/// is the literal runtime key and is not wrong to write. Kept as a re-export of the manifest's
/// vocabulary so the two cannot drift.
pub use crate::manifest::WEARERS as WARDROBE_HEROES;

/// M0162 and M0163 — the two things that can be wrong with a `place_file`.
///
/// **M0162 (Error) — a name no Shipment may write.** The lowering refuses these too, and
/// deliberately: this is the same belt-and-braces shape M0160/M0161 already have with
/// `native_hook`'s lowering. The reason to say it HERE as well is that `qm lint` is what template CI
/// runs, and a Shipment that will not build is worth hearing about on the push rather than on
/// somebody's machine. [`crate::build::companion_name_refusal`] is called rather than
/// reimplemented, because two copies of "which filenames are dangerous" is one copy that will
/// eventually be shorter than the other.
///
/// **M0163 (Warning) — a companion the plugin will not find.** This one encodes a MEASURED fact
/// about how these mods read their config, not a guess. In the community QoL mods the pattern is
/// `m2_module_path(g_hModule, "quiet_freeplay_vo.ini", …)`, and `m2_module_path` is
/// `GetModuleFileNameA(module)` truncated at the last separator — so the file is looked up beside
/// the LOADED MODULE, which is wherever the `.asi` was placed, and nowhere else. Since the
/// Quartermaster puts every `.asi` in [`crate::build::ASI_SUBDIR`], a companion sent to any other
/// destination is simply not found: the plugin falls back to its defaults and logs, at most, "no
/// such .ini — using defaults", to a file nobody reads.
///
/// It is a WARNING rather than an error because the stem match is a heuristic and the plugin's
/// source is not ours to inspect. A plugin may legitimately read something from the game root, and
/// a rule that blocked the build over a filename coincidence would be worse than the trap. It fires
/// only when the two stems match, which is exactly the naming convention every measured example
/// follows.
fn placed_file_checks(
    index: usize,
    file: &Path,
    dest: crate::manifest::PlaceIn,
    plugin_stems: &[String],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
        return out;
    };

    if let Some(why) = crate::build::companion_name_refusal(name) {
        out.push(Diagnostic {
            rule: M0162_PLACED_FILE_REFUSED,
            severity: Severity::Error,
            message: format!("{name} cannot be placed in the game folder: {why}."),
            at: Some(index),
            fix: None,
        });
    }

    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    let is_companion = stem.is_some_and(|s| plugin_stems.contains(&s));
    if is_companion && dest.relative_dir() != crate::build::ASI_SUBDIR {
        out.push(Diagnostic {
            rule: M0163_COMPANION_NOT_BESIDE_PLUGIN,
            severity: Severity::Warning,
            message: format!(
                "{name} shares its name with a plugin this Shipment ships, but it is placed in \
                 {:?} while the plugin goes in {:?}. These plugins resolve their config against \
                 their OWN module directory (`GetModuleFileNameA` truncated at the last \
                 separator), so a companion anywhere else is never opened — the plugin silently \
                 falls back to its defaults with the file sitting there looking installed.",
                display_dest(dest),
                crate::build::ASI_SUBDIR
            ),
            at: Some(index),
            fix: None,
        });
    }
    out
}

/// The game root prints as `<game folder>`; an empty string in a diagnostic reads as a bug.
fn display_dest(dest: crate::manifest::PlaceIn) -> String {
    match dest.relative_dir() {
        "" => "<game folder>".to_string(),
        d => d.to_string(),
    }
}

/// Run every hermetic rule.
///
/// `root` enables the source-file checks; pass `None` to lint manifest text alone. `names` enables
/// hash→name suggestions; without it those simply do not fire, because suggesting a name we cannot
/// look up is impossible rather than merely unhelpful.
pub fn lint(
    manifest: &Manifest,
    root: Option<&Path>,
    names: Option<&NameTable>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    if let Err(e) = manifest.validate() {
        out.push(Diagnostic {
            rule: M0100_MANIFEST_INVALID,
            severity: Severity::Error,
            message: e.to_string(),
            at: None,
            fix: None,
        });
    }

    if let Some(root) = root {
        for issue in discover::check_sources(manifest, root) {
            let (rule, severity, at) = match &issue {
                SourceIssue::Missing { index, .. } => {
                    (M0110_SOURCE_MISSING, Severity::Error, *index)
                }
                // Absolute and escaping are the same rule from the author's point of view: the path
                // does not resolve inside the Shipment.
                SourceIssue::Absolute { index, .. } | SourceIssue::EscapesRoot { index, .. } => {
                    (M0111_SOURCE_ESCAPES, Severity::Error, *index)
                }
                SourceIssue::OutsideSrc { index, .. } => {
                    (M0112_SOURCE_OUTSIDE_SRC, Severity::Warning, *index)
                }
            };
            out.push(Diagnostic {
                rule,
                severity,
                // `detail()`, not `to_string()` — `at` already carries the index and `Diagnostic`
                // prints it, so the full Display would name the contribution twice.
                message: issue.detail(),
                at: Some(at),
                fix: None,
            });
        }
    }

    for sc in blast::self_conflicts(manifest) {
        out.push(Diagnostic {
            rule: M0120_SELF_CONFLICT,
            severity: Severity::Error,
            message: sc.to_string(),
            at: sc.indices.first().copied(),
            fix: None,
        });
    }

    if let Some(names) = names {
        for s in names::bare_hash_suggestions(manifest, names) {
            out.push(Diagnostic {
                rule: M0130_BARE_HASH,
                severity: Severity::Warning,
                message: s.detail(),
                at: Some(s.index),
                fix: Some(s.name.clone()),
            });
        }
    }

    // Every plugin filename stem this Shipment ships, for M0163. Collected up front because the
    // rule is about a RELATIONSHIP between two contributions, and the companion may be listed
    // before the plugin it belongs to.
    let plugin_stems: Vec<String> = manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::NativeHook { plugin, .. } => plugin.as_ref(),
            _ => None,
        })
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_ascii_lowercase())
        .collect();

    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::AddOutfit { wearer, .. } => {
                // Valid = resolves to a real `_tOutfits` key (either `jen` or `jennifer` does);
                // the suggestion, when it does not, is the preferred spelling.
                if crate::manifest::wearer_table_key(wearer).is_none() {
                    let suggestion = closest(wearer, &WARDROBE_HEROES);
                    out.push(Diagnostic {
                        rule: M0140_UNKNOWN_WEARER,
                        severity: Severity::Error,
                        message: format!(
                            "wearer {wearer:?} is not a wardrobe hero; `_tOutfits` has lists only \
                             for {}. The outfit would be appended to a table nothing reads, so it \
                             would never appear in the wardrobe and the game would report nothing.",
                            WARDROBE_HEROES.join(", ")
                        ),
                        at: Some(index),
                        fix: suggestion.map(|s| s.to_string()),
                    });
                }
            }
            Contribution::AddMovie { name, movie } => {
                if let Some(root) = root {
                    out.extend(movie_checks(index, name, &root.join(movie)));
                }
            }
            Contribution::PatchLua { target, .. } => {
                let claim = blast::Claim::Script {
                    name: target.clone(),
                };
                let class =
                    blast::merge_class(&claim, blast::Access::Write, blast::Intent::Additive);
                if class == MergeClass::Exclusive {
                    out.push(Diagnostic {
                        rule: M0141_UNMERGEABLE_SCRIPT,
                        severity: Severity::Warning,
                        message: format!(
                            "we have not reversed how {target:?} composes, so this claim is treated \
                             as exclusive: your Shipment will refuse to install alongside any other \
                             that patches the same script. This is deliberate — the alternative is \
                             the two silently annihilating each other."
                        ),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            Contribution::EditStringDb { target, .. } => {
                // The shared UI tables are served from BOTH shell.wad (front end) and vz.wad
                // (gameplay); an overlay reaches only one mount point, so a shared-string edit that
                // ships as a single Shipment overlay is a half-fix. Advisory — the fix is a deploy
                // question (mount last in every session, or ship a shell copy too), not a defect in
                // the manifest — so amber, never red.
                let t = target.to_ascii_lowercase();
                if SHARED_STRING_TABLES.iter().any(|s| t == *s) {
                    out.push(Diagnostic {
                        rule: M0191_SHARED_STRING_TABLE,
                        severity: Severity::Warning,
                        message: format!(
                            "`{target}` is served from BOTH shell.wad (front end) and vz.wad                              (gameplay). One overlay reaches one mount point, so a shared UI string                              edited here may show in only one. Deploy it to mount last in every                              session, or ship a shell copy too."
                        ),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            Contribution::Raw { touches, .. } => {
                if touches.is_empty() {
                    out.push(Diagnostic {
                        rule: M0150_RAW_NO_TOUCHES,
                        severity: Severity::Error,
                        message:
                            "a raw contribution with an empty `touches` claims nothing, so the \
                             conflict system cannot see it — it would overwrite other Shipments \
                             silently. The declared blast radius IS what makes opaque payloads safe."
                                .into(),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            Contribution::NativeHook {
                target,
                plugin,
                symbol,
                ..
            } => {
                if *target == Target::Reimpl && plugin.is_some() {
                    out.push(Diagnostic {
                        rule: M0160_ASI_ON_REIMPL,
                        severity: Severity::Error,
                        message:
                            "an .asi is a RETAIL mechanism — pmc_bb.dll loads it into the retail \
                             exe. A reimpl Code contribution is a Rust/wasm/Lua plugin, and this \
                             file would never be loaded."
                                .into(),
                        at: Some(index),
                        fix: None,
                    });
                }
                if plugin.is_none() && symbol.is_none() {
                    out.push(Diagnostic {
                        rule: M0161_HOOK_DOES_NOTHING,
                        severity: Severity::Error,
                        message: "native_hook supplies neither `plugin` nor `symbol`, so it \
                                  contributes nothing."
                            .into(),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            Contribution::PlaceFile { file, dest } => {
                out.extend(placed_file_checks(index, file, *dest, &plugin_stems));
            }
            _ => {}
        }
    }

    for req in &manifest.load.requires {
        if let Requirement::External { url, sha256 } = req {
            let looks_like_digest =
                sha256.len() == 64 && sha256.chars().all(|c| c.is_ascii_hexdigit());
            if !looks_like_digest {
                out.push(Diagnostic {
                    rule: M0170_BAD_DIGEST,
                    severity: Severity::Error,
                    message: format!(
                        "external requirement {url} pins {sha256:?}, which is not a 64-character \
                         hex sha256. An unusable pin is worse than none: it reads as verified."
                    ),
                    at: None,
                    fix: None,
                });
            }
            if !url.starts_with("https://") {
                out.push(Diagnostic {
                    rule: M0171_INSECURE_URL,
                    severity: Severity::Warning,
                    message: format!(
                        "external requirement {url} is not https. The pinned digest still protects \
                         INTEGRITY, so this is not fatal — but the fetch itself is interceptable."
                    ),
                    at: None,
                    fix: None,
                });
            }
        }
    }

    out
}

/// The build gate. `Hang` and `Error` block; warnings do not.
///
/// Gate on this, never on a printed count (standing mandate).
pub fn blocks_build(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity >= Severity::Error)
}

/// Cheap edit-distance-1-ish suggestion for a misspelled key. Deliberately conservative: it only
/// fires on a near-miss, because a confident wrong suggestion is worse than none.
fn closest<'a>(input: &str, options: &[&'a str]) -> Option<&'a str> {
    let lower = input.to_ascii_lowercase();
    options
        .iter()
        .find(|o| {
            let o = o.to_ascii_lowercase();
            if o == lower {
                return true;
            }
            // Same first three characters and similar length reads as a typo.
            let prefix = lower.chars().take(3).collect::<String>();
            o.starts_with(&prefix) && (o.len() as i32 - lower.len() as i32).abs() <= 2
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_codes_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for r in RULES
            .iter()
            .chain(PENDING.iter())
            .chain(ARTIFACT_RULES.iter())
        {
            assert!(seen.insert(r.code), "duplicate rule code {}", r.code);
        }
    }

    #[test]
    fn closest_suggests_a_typo_but_not_a_stranger() {
        assert_eq!(closest("mattius", &WARDROBE_HEROES), Some("mattias"));
        assert_eq!(closest("Mattias", &WARDROBE_HEROES), Some("mattias"));
        assert_eq!(closest("bulldog", &WARDROBE_HEROES), None);
    }
}

#[cfg(test)]
mod artifact_check_tests {
    use super::*;
    use mercs2_formats::patch_wad::{AsetEntry, PatchBlock};

    fn block(path: &str, rows: Vec<AsetEntry>) -> PatchBlock {
        PatchBlock::from_decompressed(b"payload", path.into(), rows, None).unwrap()
    }

    /// M0001 fires. The rung names block 9 in a one-block WAD; the streamer sizes a buffer from
    /// that index and the open-world load hangs — silently, which is why this rule exists.
    #[test]
    fn m0001_fires_on_a_dangling_rung() {
        let blocks = [block(
            "blocks\\a.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_0009, 19)],
        )];
        let d = artifact_checks(&blocks);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule.code, "M0001");
        assert_eq!(
            d[0].severity,
            Severity::Hang,
            "a hang must outrank an error"
        );
    }

    /// M0001 stays quiet on a fully-sentinel row — the shape every fully-resident character
    /// texture has, and the one our own lowering emits. A rule that fired here would fire on
    /// everything we build.
    #[test]
    fn m0001_is_quiet_on_a_sentinel_row() {
        let blocks = [block(
            "blocks\\a.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 19)],
        )];
        assert_eq!(artifact_checks(&blocks), vec![]);
    }

    /// M0002 fires when `packed_field` under-claims. Built by hand because
    /// `PatchBlock::from_decompressed` makes this state unrepresentable — which is the point of
    /// that constructor, and why the rule is a backstop for the paths that do not use it.
    #[test]
    fn m0002_fires_when_packed_field_under_claims() {
        let raw = vec![0xABu8; mercs2_formats::patch_wad::PAGE_SIZE * 3];
        let mut blk = block(
            "blocks\\a.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 19)],
        );
        blk.compressed_data = mercs2_formats::sges::compress_sges(&raw).unwrap();
        blk.packed_field = 1; // claims one page; needs three
        let d = artifact_checks(&[blk]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule.code, "M0002");
        assert_eq!(d[0].severity, Severity::Hang);
        assert!(
            d[0].message.contains("overrun the heap"),
            "{}",
            d[0].message
        );
    }

    /// M0002 stays quiet on a multi-page block whose count was computed honestly.
    #[test]
    fn m0002_is_quiet_when_packed_field_is_honest() {
        let raw = vec![0xABu8; mercs2_formats::patch_wad::PAGE_SIZE * 3];
        let blk = PatchBlock::from_decompressed(
            &raw,
            "blocks\\a.block".into(),
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 19)],
            None,
        )
        .unwrap();
        assert_eq!(artifact_checks(&[blk]), vec![]);
    }

    // --- M0003 / M0004 fixtures ------------------------------------------------
    //
    // Both rules read a block as `[entry table][containers…]`, so their fixtures are REAL texture
    // blocks built by `build_texture_block` rather than the `b"payload"` stand-in above. That
    // stand-in is not an entry-table block at all, which is exactly why the existing fixtures stay
    // silent under the new rules.

    /// A fully-resident DXT1 texture block: `claimed_mips` in INFO, `written_mips` levels of body.
    /// Equal counts is what the lowering emits; a claim larger than the body is the defect.
    fn texture_block(
        name_hash: u32,
        dim: usize,
        claimed_mips: u32,
        written_mips: usize,
    ) -> Vec<u8> {
        let body_len =
            mercs2_formats::texsize::linear_mip_chain_size(dim, dim, b"DXT1", written_mips);
        let td = mercs2_formats::texture::TextureData {
            width: dim as u32,
            height: dim as u32,
            format: mercs2_formats::texture::TexFormat::Bc1,
            mip0: Vec::new(),
            all_mips: vec![0u8; body_len],
            mip_count: claimed_mips,
        };
        mercs2_formats::texture::build_texture_block(name_hash, &td)
    }

    fn block_from(raw: &[u8], path: &str, rows: Vec<AsetEntry>) -> PatchBlock {
        PatchBlock::from_decompressed(raw, path.into(), rows, None).unwrap()
    }

    /// M0003 fires when INFO claims more mip levels than BODY carries. The engine sizes its read
    /// from the CLAIM, over-reads the surface array, and `STATUS_BUFFER_TOO_SMALL` leaves the page
    /// short of ready state — the world load then never completes.
    #[test]
    fn m0003_fires_when_the_body_is_short_for_the_claimed_chain() {
        // 64x64 DXT1: retail's convention is 5 levels (2,728 B). Claim all 5, write only mip 0.
        let raw = texture_block(0xBEEF, 64, 5, 1);
        let blk = block_from(
            &raw,
            "blocks\\VZ\\mod_short.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        let d = artifact_checks(&[blk]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule.code, "M0003");
        assert_eq!(d[0].severity, Severity::Hang);
        assert!(d[0].message.contains("2728"), "{}", d[0].message);
    }

    /// M0003 stays quiet on the shape our own lowering emits — a claim the body covers exactly.
    /// A rule that fired here would fire on every texture this crate builds.
    #[test]
    fn m0003_is_quiet_on_a_complete_mip_chain() {
        let raw = texture_block(0xBEEF, 64, 5, 5);
        let blk = block_from(
            &raw,
            "blocks\\VZ\\mod_full.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        assert_eq!(artifact_checks(&[blk]), vec![]);
    }

    /// The gate that makes M0003 usable at all: a STREAMED texture ships a short resident tail by
    /// design, and retail has 9,562 of them. Pinned here because the gate lives in `wad_simulator`
    /// and this crate now depends on it — if that predicate ever loses the residency check, the
    /// rule starts firing on almost every texture in the game and this test says so.
    #[test]
    fn m0003_is_quiet_on_a_streamed_texture_with_a_short_tail() {
        let mut raw = texture_block(0xBEEF, 64, 5, 1);
        // INFO is the first leaf of the single container: [4 count][16 entry][20 UCFX hdr]
        // [2 x 20 descriptors] = 80 bytes in. Bytes 26..32 of INFO are the partial-residency
        // descriptor; a non-zero value there is what marks the body a streamed tail.
        let info_at = 4 + 16 + 20 + 2 * 20;
        raw[info_at + 26..info_at + 32].copy_from_slice(&[0x01, 0x00, 0x0e, 0x00, 0x10, 0x00]);
        let blk = block_from(
            &raw,
            "blocks\\VZ\\mod_streamed.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        assert_eq!(artifact_checks(&[blk]), vec![]);
    }

    /// M0004 fires on an asset no ASET row names. The block is well-formed and the payload is
    /// intact — it is simply unreachable, which is the whole reason this failure is silent.
    #[test]
    fn m0004_fires_when_a_block_asset_has_no_aset_row() {
        let raw = texture_block(0xC0FFEE, 64, 5, 5);
        // The row names a DIFFERENT hash, so the block's own asset is unnamed.
        let blk = block_from(
            &raw,
            "blocks\\VZ\\mod_orphan.block",
            vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        let d = artifact_checks(&[blk]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule.code, "M0004");
        assert_eq!(d[0].severity, Severity::Hang);
        assert!(d[0].message.contains("0x00C0FFEE"), "{}", d[0].message);
    }

    /// M0004 stays quiet when the row names the asset — the shape every lowering path emits.
    #[test]
    fn m0004_is_quiet_when_every_block_asset_is_named() {
        let raw = texture_block(0xC0FFEE, 64, 5, 5);
        let blk = block_from(
            &raw,
            "blocks\\VZ\\mod_named.block",
            vec![AsetEntry::new(0xC0FFEE, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        assert_eq!(artifact_checks(&[blk]), vec![]);
    }

    /// A row may live in ANOTHER block of the same WAD and still name the asset — the ASET table is
    /// per-archive, not per-block, and the linked-scripts path relies on that. Checking rows
    /// block-locally would report a WAD the engine loads fine as a hang.
    #[test]
    fn m0004_accepts_a_row_carried_by_a_sibling_block() {
        let raw = texture_block(0xC0FFEE, 64, 5, 5);
        let carrier = block_from(&raw, "blocks\\VZ\\mod_a.block", vec![]);
        let rows = block_from(
            b"payload",
            "blocks\\VZ\\mod_b.block",
            vec![AsetEntry::new(0xC0FFEE, 0xFFFF_FFFF, 0x0000_FFFF, 27)],
        );
        assert_eq!(artifact_checks(&[carrier, rows]), vec![]);
    }

    /// M0180 fires but does not block: the registry is first-writer-wins, so this is a defined
    /// outcome, not a hang. It matters because one contribution silently does nothing.
    #[test]
    fn m0180_warns_on_a_duplicate_claim_without_blocking() {
        let row = || vec![AsetEntry::new(0xBEEF, 0xFFFF_FFFF, 0x0000_FFFF, 19)];
        let d = artifact_checks(&[
            block("blocks\\a.block", row()),
            block("blocks\\b.block", row()),
        ]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule.code, "M0180");
        assert!(
            !blocks_build(&d),
            "retail ships this shape; it must not fail a build"
        );
    }

    /// Every artifact rule is registered, so `qm` can list what it checks.
    #[test]
    fn artifact_rules_are_registered() {
        for code in [
            "M0001", "M0002", "M0003", "M0004", "M0180", "M0181", "M0182",
        ] {
            assert!(
                ARTIFACT_RULES.iter().any(|r| r.code == code),
                "{code} unregistered"
            );
        }
        for code in ["M0001", "M0002", "M0003", "M0004"] {
            assert!(
                !PENDING.iter().any(|r| r.code == code),
                "{code} is implemented, not pending"
            );
        }
    }
}
