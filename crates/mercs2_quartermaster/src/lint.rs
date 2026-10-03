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

use crate::blast;
use crate::discover::{self, SourceIssue};
use crate::game::GameStack;
use crate::manifest::{Contribution, Manifest, Target};
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
/// A plugin or runtime DLL the game could not load: not an i386 PE DLL
/// ([`crate::pe::pe_dll_load_blocker`]). Needs the file, so it runs only when lint has the Shipment
/// root.
pub const M0178_DLL_NOT_LOADABLE: Rule = Rule {
    code: "M0178",
    title: "a plugin or runtime DLL is not a loadable i386 PE DLL",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
/// Reported by validation (`ValidateError::BadRange`).
pub const M0172_BAD_VERSION_REQ: Rule = Rule {
    code: "M0172",
    title: "a version range is not a valid semver range",
    doc: "docs/modding/manifest_format.md#the-code-layer",
};
/// Reported by validation (`ValidateError::SelfReference`).
pub const M0173_SELF_REFERENCE: Rule = Rule {
    code: "M0173",
    title: "load.requires or load.conflicts names the Shipment itself",
    doc: "docs/modding/manifest_format.md#dependencies",
};
/// Reported by validation (`ValidateError::ReservedName`).
pub const M0211_RESERVED_NAME: Rule = Rule {
    code: "M0211",
    title: "shipment.name is a reserved name: the stem of a DLL no Shipment may ship",
    doc: "docs/modding/manifest_format.md#dependencies",
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

/// An `add_language` name that cannot be a novel language WAD — not a lowercase `[a-z0-9_]` token, or
/// one the game already ships (`.\Data\<name>.wad`), which the kind would otherwise overwrite. The
/// message comes from [`crate::build::language_name_refusal`], the same check the lowering enforces —
/// so the refusal exists at BOTH ends and cannot be reached past by suppressing the rule.
pub const M0200_LANGUAGE_NAME_UNUSABLE: Rule = Rule {
    code: "M0200",
    title: "an add_language name is not a usable language name, or collides with a shipped WAD",
    doc: "docs/modding/manifest_format.md#add_language",
};

/// `collision: follow_geometry` set alongside `retarget:` on the same `add_model`. `retarget` takes
/// the SKINNED lowering (character rig → ragdoll/capsule collision), where the rigid static-collision
/// regeneration never runs — so `follow_geometry` is silently ignored. It is a rigid-path-only option.
pub const M0202_COLLISION_ON_SKINNED: Rule = Rule {
    code: "M0202",
    title: "collision: follow_geometry is ignored on a skinned (retarget) add_model",
    doc: "docs/modding/manifest_format.md#add_model",
};

/// An `add_animation` / `replace_animation` whose sources do not belong together: the clip is not a
/// Havok 5.5 packfile with a readable `hkaAnimation`, the `trnm` is malformed or binds a different
/// number of tracks than the clip's `numTransformTracks`, or the `events` do not parse or are not
/// in time order. The rules are [`mercs2_formats::anim_container::clip_pairing_problems`] — the same
/// check the lowering makes, and one all 4,232 retail clips pass. Needs the files, so it runs only
/// when lint has the Shipment root.
pub const M0213_ANIMATION_PAIRING: Rule = Rule {
    code: "M0213",
    title: "an animation's clip, trnm and events do not belong together",
    doc: "docs/modding/manifest_format.md#add_animation",
};

/// A native_hook `signature_guard` is malformed — a guard for an address the hook does not
/// `touch`, or a value that is not hex prologue bytes — or, with the game exe in hand, names bytes
/// that do not match `Mercenaries2.exe` at that address. The guard is a plugin's load-time defence
/// against patching an exe that has shifted under it; a wrong or missing guard defeats it silently.
/// The malformed cases are hermetic errors; the exe mismatch is a game-gated warning (the local exe
/// may be a different build than the hook targets), and an unguarded touch is an advisory.
pub const M0199_SIGNATURE_GUARD: Rule = Rule {
    code: "M0199",
    title: "a native_hook signature guard is malformed, or does not match the exe it names",
    doc: "docs/modding/manifest_format.md#native_hook",
};

/// An `add_script` whose Lua registers itself as a mission (its `name` appears as an
/// `sModuleName = "…"` in a `tMissionData` row shipped by this Shipment, or a hook it emits) but
/// whose top-level does not `inherit("MrxTask…")`. The engine's `_ModuleLoaded`
/// ([mrxtask.lua:294](../mercs2-luacd/src/resident/mrxtask.lua)) replaces `oMission`'s metatable
/// with `{__index = <this module>}` at contract activation; missing inherit → `oMission:IsActive`,
/// `:Configure`, `:SaveInstance`, `:Cleanup` all resolve to nil, `RefreshAllPdaMissionDetails`
/// crashes on the first `:IsActive()`, the containing pcall unwinds, and the game degrades
/// silently (fuel deducted, delivery never fires — [[custom-mission-inherit-mrxtask-required]]).
pub const M0300_MISSION_ADD_SCRIPT_NO_INHERIT: Rule = Rule {
    code: "M0300",
    title: "an add_script that registers as a mission does not inherit an MrxTask subclass",
    doc: "docs/modding/lua_engine_seam_hardening.md",
};

/// A Shipment's Lua calls `Event.Create` / `Event.CreatePersistent` directly rather than through
/// `MrxTask`'s `self:_CreateEvent(…)` / `self:_CreatePersistentEvent(…)`. The direct handle is
/// not tracked by the task's `_tEvents` set, so `MrxTask.DestroyEvents(self)` cannot delete it —
/// the callback keeps firing after mission Cleanup, capturing `self` past the mission's lifetime.
/// `Event.CreatePersistent` is worse: it survives level transitions forever. See
/// `docs/modding/lua_engine_seam_hardening.md#f7--event--callback-handle-leaks`.
pub const M0301_BARE_EVENT_CREATE: Rule = Rule {
    code: "M0301",
    title: "Event.Create called directly instead of through self:_CreateEvent",
    doc: "docs/modding/lua_engine_seam_hardening.md",
};

/// A Shipment's Lua writes to `_G.<name>` or `_MODULES[<name>]` at file scope — either directly
/// pollutes the global environment or reaches into another module's namespace. Both surface as
/// silent breakage: the engine's own writes to `_G` (via `dynamic_import`) crash a `__newindex`
/// meta-hook at `0x0059C82A` (`docs/dlc_mission_loading.md`), and a module clobbered through
/// `_MODULES` continues to be `import`ed but with unpredictable state. See
/// `docs/modding/lua_engine_seam_hardening.md#qm--compile-time-correct-by-construction`.
pub const M0302_GLOBAL_SHADOWING: Rule = Rule {
    code: "M0302",
    title: "Shipment Lua writes to _G or _MODULES — engine-state clobber",
    doc: "docs/modding/lua_engine_seam_hardening.md",
};

/// A Shipment writes a row into `WifMissionData.tMissionData` whose key does not match the shipped
/// `<Faction3><Con|Job><NN+>` naming shape (e.g. `PmcCon001`, `OilJob004`). The instant the player
/// selects such a briefing from a starter root menu, `MrxUtil.ExplodeMissionName` returns
/// `nNumber=nil`, and `GetSpielFileName` at
/// [`mrxbriefing.lua:2849`](../../tools/wad_simulator/workshop_data/lua/resident/mrxbriefing.lua#L2849)
/// unconditionally does `string.format("%02d", nil)` → runtime error. The error propagates out of
/// the dialog-selection handler before `_End` reaches `_Fade(false, _EndBegin)`; the UI tears
/// down partially (`_ClientMenuBox` stays `true`, briefing camera stays locked). See F12 in
/// `docs/modding/lua_engine_seam_hardening.md`.
pub const M0303_MISSION_ID_UNPARSEABLE: Rule = Rule {
    code: "M0303",
    title: "tMissionData row key is not in the <Faction3><Con|Job><NN+> shape",
    doc: "docs/modding/lua_engine_seam_hardening.md",
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
    M0150_RAW_NO_TOUCHES,
    M0160_ASI_ON_REIMPL,
    M0161_HOOK_DOES_NOTHING,
    M0162_PLACED_FILE_REFUSED,
    M0163_COMPANION_NOT_BESIDE_PLUGIN,
    M0172_BAD_VERSION_REQ,
    M0173_SELF_REFERENCE,
    M0178_DLL_NOT_LOADABLE,
    M0190_MOVIE_CARRIES_AS3,
    M0191_SHARED_STRING_TABLE,
    M0199_SIGNATURE_GUARD,
    M0200_LANGUAGE_NAME_UNUSABLE,
    M0202_COLLISION_ON_SKINNED,
    M0211_RESERVED_NAME,
    M0213_ANIMATION_PAIRING,
    M0300_MISSION_ADD_SCRIPT_NO_INHERIT,
    M0301_BARE_EVENT_CREATE,
    M0302_GLOBAL_SHADOWING,
    M0303_MISSION_ID_UNPARSEABLE,
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

/// Parse a signature-guard value — space-separated hex byte pairs like `"55 8B EC"` — into bytes.
/// `None` if it is empty or any token is not a single hex byte. Shared by the hermetic M0199 check
/// (validity) and the game-gated one (the bytes to compare against the exe).
fn parse_prologue_bytes(s: &str) -> Option<Vec<u8>> {
    let bytes: Option<Vec<u8>> = s
        .split_whitespace()
        .map(|tok| (tok.len() <= 2).then(|| u8::from_str_radix(tok, 16).ok()).flatten())
        .collect();
    bytes.filter(|b| !b.is_empty())
}

/// Render bytes back as the guard's own `"55 8B EC"` spelling, for a diagnostic that quotes them.
fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// An absolute `0xHHHHHHHH` address (a native_hook `touches` / guard key) as a u32, or `None`.
fn parse_address(s: &str) -> Option<u32> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    u32::from_str_radix(hex, 16).ok()
}

/// Rules that need the retail WADs. Separate from [`lint`] on purpose: everything there runs in CI
/// with no game, and mixing the two would make the hermetic set impossible to run alone.
pub fn game_checks(manifest: &Manifest, game: &GameStack) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        // M0199 (game-gated half): compare each declared signature guard against the bytes actually
        // at that address in `Mercenaries2.exe`. Self-skips when the exe is not beside the install.
        if let Contribution::NativeHook { touches: _, signature_guard, .. } = c {
            if !signature_guard.is_empty() {
                if let Some(exe_path) = game.exe_path() {
                    if let Ok(exe) = std::fs::read(&exe_path) {
                        for (addr, sig) in signature_guard {
                            // Malformed addr/bytes are the hermetic half's job (M0199 error there);
                            // here we only verify the ones we can actually read.
                            let (Some(va), Some(expected)) =
                                (parse_address(addr), parse_prologue_bytes(sig))
                            else {
                                continue;
                            };
                            match crate::pe::read_at_va(&exe, va, expected.len()) {
                                Some(actual) if actual == expected => {}
                                Some(actual) => out.push(Diagnostic {
                                    rule: M0199_SIGNATURE_GUARD,
                                    severity: Severity::Warning,
                                    message: format!(
                                        "signature_guard for {addr} expects [{}] but {} has [{}] \
                                         there. The local exe may be a different build than this \
                                         hook targets — verify against the intended \
                                         Mercenaries2.exe before shipping.",
                                        hex_bytes(&expected),
                                        exe_path.display(),
                                        hex_bytes(&actual)
                                    ),
                                    at: Some(index),
                                    fix: None,
                                }),
                                None => out.push(Diagnostic {
                                    rule: M0199_SIGNATURE_GUARD,
                                    severity: Severity::Warning,
                                    message: format!(
                                        "signature_guard names {addr}, which maps into no readable \
                                         code of {} — that address is not part of this exe build.",
                                        exe_path.display()
                                    ),
                                    at: Some(index),
                                    fix: None,
                                }),
                            }
                        }
                    }
                }
            }
        }
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
    doc: "docs/modding/field_guide.md#trap-8--you-edited-a-block-and-now-the-heap-is-corrupt-the-packed_field-bug",
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
fn movie_checks(index: usize, name: &str, root: &Path, movie: &Path) -> Vec<Diagnostic> {
    // The message names `movie` as the manifest wrote it, never the joined path: a report must not
    // carry the local machine's absolute path.
    let Ok(bytes) = std::fs::read(root.join(movie)) else {
        // M0110 already reports a missing source; an unreadable one is not this rule's business.
        return Vec::new();
    };
    let Ok(parsed) = mercs2_formats::gfx::GfxMovie::parse(&bytes) else {
        return Vec::new();
    };
    let features = parsed.features();
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
            movie.display(),
            features.do_abc
        ),
        at: Some(index),
        fix: None,
    }]
}

/// M0213 — an animation's clip, `trnm` and `events` read and checked as the triple they ship as.
///
/// Error, because the lowering refuses the same triple: the rule only says so earlier, in the
/// hermetic stage template CI runs. A file that cannot be read is reported here too (the source
/// checks already passed, so the path exists and resolves inside the Shipment).
fn animation_checks(
    index: usize,
    root: &Path,
    clip: &Path,
    trnm: &Path,
    events: Option<&Path>,
) -> Vec<Diagnostic> {
    let finding = |message: String| Diagnostic {
        rule: M0213_ANIMATION_PAIRING,
        severity: Severity::Error,
        message,
        at: Some(index),
        fix: None,
    };
    let read = |p: &Path| {
        std::fs::read(root.join(p)).map_err(|e| finding(format!("{}: cannot be read: {e}", p.display())))
    };
    let (clip_bytes, trnm_bytes) = match (read(clip), read(trnm)) {
        (Ok(c), Ok(t)) => (c, t),
        (c, t) => return c.err().into_iter().chain(t.err()).collect(),
    };
    let evnt_bytes = match events.map(read).transpose() {
        Ok(e) => e,
        Err(d) => return vec![d],
    };
    mercs2_formats::anim_container::clip_pairing_problems(
        &clip_bytes,
        &trnm_bytes,
        evnt_bytes.as_deref(),
    )
    .into_iter()
    .map(|p| {
        finding(format!(
            "{} + {}{}: {p}",
            clip.display(),
            trnm.display(),
            events.map(|e| format!(" + {}", e.display())).unwrap_or_default()
        ))
    })
    .collect()
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

impl Diagnostic {
    /// This diagnostic as the shared finding element, for `lint-report.json`.
    ///
    /// `items` is always empty — a lint report covers one manifest and has no request ids — and
    /// `refs` points into `contributions` when the diagnostic belongs to one.
    pub fn to_finding(&self) -> crate::plan::Finding {
        use crate::plan::{Finding, FindingRef, FindingSeverity, Section};
        Finding {
            code: self.rule.code,
            severity: match self.severity {
                Severity::Info => FindingSeverity::Info,
                Severity::Warning => FindingSeverity::Warning,
                Severity::Error => FindingSeverity::Error,
                Severity::Hang => FindingSeverity::Hang,
            },
            message: self.message.clone(),
            items: Vec::new(),
            refs: self
                .at
                .map(|index| FindingRef {
                    section: Section::Contributions,
                    index,
                })
                .into_iter()
                .collect(),
            fix: self.fix.clone(),
        }
    }
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

/// M0162 and M0178 for an `add_runtime_dll`.
///
/// **M0162 (Error)** — the name is refused by [`crate::build::runtime_dll_name_refusal`], the same
/// function the lowering and the load plan call: not a single `.dll` file name, a deny-listed stem,
/// or not `<shipment.name>.dll`. Needs only the manifest, so it runs with or without a root.
///
/// **M0178 (Error)** — the bytes are not a loadable i386 PE DLL
/// ([`crate::pe::pe_dll_load_blocker`]). Needs the file, so it runs only with a `root`, and not for
/// a contribution whose source path is already an error (`source_issue_at`).
fn runtime_dll_checks(
    index: usize,
    dll: &Path,
    shipment_name: &str,
    root: Option<&Path>,
    source_issue_at: &[usize],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(name) = dll.file_name().and_then(|n| n.to_str()) else {
        out.push(Diagnostic {
            rule: M0162_PLACED_FILE_REFUSED,
            severity: Severity::Error,
            message: "the `dll` path has no UTF-8 file name, so it cannot be placed.".into(),
            at: Some(index),
            fix: None,
        });
        return out;
    };
    if let Some(why) = crate::build::runtime_dll_name_refusal(name, shipment_name) {
        out.push(Diagnostic {
            rule: M0162_PLACED_FILE_REFUSED,
            severity: Severity::Error,
            message: format!("{name} cannot be placed in the game folder: {why}."),
            at: Some(index),
            fix: None,
        });
    }
    let Some(root) = root else {
        return out;
    };
    if source_issue_at.contains(&index) {
        return out;
    }
    let why = match std::fs::read(root.join(dll)) {
        Ok(bytes) => crate::pe::pe_dll_load_blocker(&bytes, "add_runtime_dll"),
        Err(e) => Some(format!("it cannot be read: {e}")),
    };
    if let Some(why) = why {
        out.push(Diagnostic {
            rule: M0178_DLL_NOT_LOADABLE,
            severity: Severity::Error,
            message: format!("{name} cannot be loaded by the game: {why}."),
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
        // A validation failure with a code of its own is reported under it; the rest are M0100.
        let rule = match e.code() {
            None => M0100_MANIFEST_INVALID,
            Some(code) => *RULES.iter().find(|r| r.code == code).unwrap_or_else(|| {
                panic!("ValidateError reports {code}, which is not a registered rule in RULES")
            }),
        };
        out.push(Diagnostic {
            rule,
            severity: Severity::Error,
            message: e.to_string(),
            at: None,
            fix: None,
        });
    }

    // Contributions whose source path is already an error (missing, absolute, escaping, outside
    // `src/`). A rule that reads the file does not read those: the path is reported, and reading an
    // escaping path would read outside the Shipment.
    let mut source_issue_at: Vec<usize> = Vec::new();
    if let Some(root) = root {
        out.extend(lua_source_checks(manifest, root));

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
            source_issue_at.push(at);
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
                    out.extend(movie_checks(index, name, root, movie));
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
                            "`{target}` is served from BOTH shell.wad (front end) and vz.wad \
                             (gameplay). One overlay reaches one mount point, so a shared UI string \
                             edited here may show in only one. Deploy it to mount last in every \
                             session, or ship a shell copy too."
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
                touches,
                signature_guard,
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
                // M0199 (hermetic half). A guard defends a PATCHED address, so it must name one the
                // hook `touches` and carry real prologue bytes; a touched address with no guard is a
                // missed defence. The byte-vs-exe check is the game-gated half in `game_checks`.
                let touched: std::collections::BTreeSet<&str> =
                    touches.iter().map(|t| t.0.as_str()).collect();
                for (addr, sig) in signature_guard {
                    if !touched.contains(addr.as_str()) {
                        out.push(Diagnostic {
                            rule: M0199_SIGNATURE_GUARD,
                            severity: Severity::Error,
                            message: format!(
                                "signature_guard names {addr}, which is not in this hook's \
                                 `touches`. A guard protects a patched address; guarding one the \
                                 hook never touches is a mistake."
                            ),
                            at: Some(index),
                            fix: None,
                        });
                    }
                    if parse_prologue_bytes(sig).is_none() {
                        out.push(Diagnostic {
                            rule: M0199_SIGNATURE_GUARD,
                            severity: Severity::Error,
                            message: format!(
                                "signature_guard for {addr} is not hex prologue bytes ({sig:?}); \
                                 write space-separated byte pairs like \"55 8B EC\"."
                            ),
                            at: Some(index),
                            fix: None,
                        });
                    }
                }
                // Guards are opt-in: a hook that declares none is not flagged. But once SOME
                // addresses are guarded, a touched address left unguarded is almost certainly an
                // oversight — that partial-coverage gap is the advisory.
                if !signature_guard.is_empty() {
                    for t in touches {
                        if !signature_guard.contains_key(&t.0) {
                            out.push(Diagnostic {
                                rule: M0199_SIGNATURE_GUARD,
                                severity: Severity::Warning,
                                message: format!(
                                    "hook touches {} but guards other addresses and not this one, \
                                     so a plugin cannot tell whether the exe shifted under it here. \
                                     Record the expected prologue bytes for that address too.",
                                    t.0
                                ),
                                at: Some(index),
                                fix: None,
                            });
                        }
                    }
                }
            }
            Contribution::PlaceFile { file, dest } => {
                out.extend(placed_file_checks(index, file, *dest, &plugin_stems));
            }
            Contribution::AddRuntimeDll { dll } => {
                out.extend(runtime_dll_checks(
                    index,
                    dll,
                    &manifest.shipment.name,
                    root,
                    &source_issue_at,
                ));
            }
            Contribution::AddLanguage { name, .. } => {
                // The `data/` safety pivot: refuse a name that is not a usable language token or that
                // collides with a WAD the game already ships. Error, and the SAME refusal the lowering
                // enforces — surfaced here so template CI says so before a build is attempted.
                if let Some(why) = crate::build::language_name_refusal(name) {
                    out.push(Diagnostic {
                        rule: M0200_LANGUAGE_NAME_UNUSABLE,
                        severity: Severity::Error,
                        message: format!("add_language name {name:?}: {why}"),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            Contribution::AddAnimation {
                clip, trnm, events, ..
            }
            | Contribution::ReplaceAnimation {
                clip, trnm, events, ..
            } => {
                if let Some(root) = root {
                    if !source_issue_at.contains(&index) {
                        out.extend(animation_checks(index, root, clip, trnm, events.as_deref()));
                    }
                }
            }
            Contribution::AddModel {
                name,
                retarget,
                collision,
                ..
            } => {
                // `follow_geometry` regenerates STATIC (WpMeshShape16) collision on the rigid path.
                // `retarget` diverts to the skinned lowering, whose collision is ragdoll/capsule and
                // whose PHY2 is never re-authored — so the option silently does nothing there.
                if *collision == crate::manifest::CollisionSource::FollowGeometry
                    && retarget.is_some()
                {
                    out.push(Diagnostic {
                        rule: M0202_COLLISION_ON_SKINNED,
                        severity: Severity::Error,
                        message: format!(
                            "add_model {name:?} sets `collision: follow_geometry` together with \
                             `retarget:`. `retarget` is the SKINNED path (character rig → \
                             ragdoll/capsule collision); the rigid static-collision regeneration \
                             never runs there, so `follow_geometry` would be silently ignored. Drop \
                             one: `follow_geometry` is for rigid props, `retarget` for skinned \
                             characters."
                        ),
                        at: Some(index),
                        fix: None,
                    });
                }
            }
            _ => {}
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

/// The MrxTask class family. An `add_script` that registers as a mission (its name is an
/// `sModuleName` in this Shipment's Lua) must `inherit` one of these — every retail contract
/// and job script does, from `pmccon001` (`inherit("MrxTaskContract")`) through the outpost /
/// verify-set / destroy-set / destroy-type variants (`docs/mercs2-luacd/03_contracts_jobs.md`).
/// A missing inherit is the [[custom-mission-inherit-mrxtask-required]] failure.
const MRXTASK_BASES: &[&str] = &[
    "MrxTask",
    "MrxTaskMission",
    "MrxTaskJob",
    "MrxTaskContract",
    "MrxTaskContractOutpost",
    "MrxTaskJobVerifySet",
    "MrxTaskJobDestroySet",
    "MrxTaskJobDestroyType",
    "MrxTaskObjective",
    "MrxTaskObjectiveDeliver",
    "MrxTaskObjectiveDestroy",
    "MrxTaskObjectiveEnterVehicle",
    "MrxTaskObjectiveVerify",
    "MrxTaskObjectiveAction",
    "MrxTaskRace",
];

/// Return true if a Lua source contains `inherit("<any MrxTask class>")` — matched literally
/// because retail scripts spell it as one line, unambiguously (`inherit("MrxTaskContract")`).
/// Comments and string-inside-comments are not stripped: the rule is conservative — a shipped
/// script that lints as compliant must actually inherit; a comment mention is not enough. The
/// modder is 3 characters away from making it a real call.
fn inherits_mrxtask(source: &str) -> bool {
    for base in MRXTASK_BASES {
        let single = format!("inherit(\"{base}\"");
        let double = format!("inherit('{base}'");
        if source.contains(&single) || source.contains(&double) {
            return true;
        }
    }
    false
}

/// Every `sModuleName = "<name>"` in a Lua source. This is how retail's `tMissionData` rows point
/// at the script that implements a mission — a `patch_lua mrxmissionflow` append that adds a row
/// like `tMissionData["FioDef001"] = { sModuleName = "FioDef001", ... }` names the AddScript
/// that must inherit MrxTask.
///
/// Simple character-walking scan: no regex dependency, and the field name is distinctive enough
/// to have no false-positive class in shipped Lua (verified across `docs/mercs2-luacd/`).
fn scan_module_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let key = b"sModuleName";
    let mut i = 0;
    while i + key.len() < bytes.len() {
        if &bytes[i..i + key.len()] != key {
            i += 1;
            continue;
        }
        // Skip past the key.
        let mut j = i + key.len();
        // Skip whitespace.
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'=' {
            i = j;
            continue;
        }
        j += 1;
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        if j >= bytes.len() || !matches!(bytes[j], b'"' | b'\'') {
            i = j;
            continue;
        }
        let quote = bytes[j];
        j += 1;
        let start = j;
        while j < bytes.len() && bytes[j] != quote && bytes[j] != b'\n' {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == quote {
            if let Ok(name) = std::str::from_utf8(&bytes[start..j]) {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// One byte offset in a Lua source, translated to a `(line, col)` for a human-readable diagnostic.
/// Both 1-indexed, following the convention `qm build`'s Lua errors use.
fn line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Every occurrence of `Event.Create(` or `Event.CreatePersistent(` in a Lua source, returning
/// (line, col, byte-offset, kind) for each. Matches greedily on the literal form the shipped
/// scripts use — `Event . Create ( ... )` with arbitrary whitespace is uncommon in retail (0
/// hits across the decompiled corpus) but if a modder writes it that way we deliberately do
/// not catch it: the rule targets the readable form, and a workaround that goes out of its way
/// to hide from the linter is on the author, not us.
fn scan_event_create(source: &str) -> Vec<(usize, usize, usize, &'static str)> {
    let mut out = Vec::new();
    for needle in ["Event.CreatePersistent(", "Event.Create("] {
        let mut start = 0;
        while let Some(off) = source[start..].find(needle) {
            let abs = start + off;
            let (line, col) = line_col(source, abs);
            let kind = if needle.starts_with("Event.CreatePersistent") {
                "Event.CreatePersistent"
            } else {
                // Refuse to double-report the SAME byte offset the persistent scan already reported.
                if out.iter().any(|(l, c, _, _)| *l == line && *c == col) {
                    start = abs + needle.len();
                    continue;
                }
                "Event.Create"
            };
            out.push((line, col, abs, kind));
            start = abs + needle.len();
        }
    }
    out.sort();
    out
}

/// Is `at` inside a function whose signature makes `self` a valid identifier?
///
/// A colon-syntax method (`function X:Y(...)`) or an explicit-self first parameter (`function
/// X.Y(self, ...)`, `function X(self, ...)`) both make `self:_CreateEvent(...)` a fixable
/// suggestion. Anywhere else — top-level, a plain function without a self param, a helper
/// nested in a method that doesn't itself take self — the M0301 fix does not apply, so the
/// rule should not fire.
///
/// Walks backward line by line, stripping `--` line comments, and reads the nearest enclosing
/// `function` signature. A signature that spans lines gets classified as no-self (conservative).
fn enclosing_function_has_self(source: &str, at: usize) -> bool {
    let prefix = &source[..at];
    let mut line_starts: Vec<usize> = std::iter::once(0)
        .chain(prefix.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    while let Some(line_start) = line_starts.pop() {
        let line_end = source[line_start..]
            .find('\n')
            .map(|n| line_start + n)
            .unwrap_or(source.len());
        let line = &source[line_start..line_end];
        let code = line.split("--").next().unwrap_or("");
        let Some(fn_at) = code.find("function") else { continue };
        let ok_before = fn_at == 0 || {
            let c = code.as_bytes()[fn_at - 1];
            !(c.is_ascii_alphanumeric() || c == b'_')
        };
        if !ok_before {
            continue;
        }
        let after = code[fn_at + "function".len()..].trim_start();
        let Some(open) = after.find('(') else { return false };
        let Some(close_rel) = after[open + 1..].find(')') else { return false };
        let name_part = &after[..open];
        let params = &after[open + 1..open + 1 + close_rel];
        if name_part.contains(':') {
            return true;
        }
        let first = params.split(',').next().unwrap_or("").trim();
        return first == "self";
    }
    false
}

/// Every top-level write to `_G.<name>`, `_MODULES.<name>`, or `_MODULES[<expr>]` in a Lua
/// source. Reads the source line-by-line and looks for the pattern at any position — a write
/// nested inside a function is still a write. Returns (line, col, target) tuples.
///
/// `_G.<name> = …` writes to the global environment, competing with the engine's own globals
/// and, worse, tripping the `__newindex` crash at `0x0059C82A` if a mod later installs one
/// (`docs/dlc_mission_loading.md`). `_MODULES[<name>] = …` overwrites another module's table,
/// bypassing every `import()`er of that module.
fn scan_global_writes(source: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the start of a potential match.
        let rest = &bytes[i..];
        let (kind, base_len) = if rest.starts_with(b"_G.") {
            ("_G", 3usize)
        } else if rest.starts_with(b"_MODULES.") {
            ("_MODULES.", 9usize)
        } else if rest.starts_with(b"_MODULES[") {
            ("_MODULES[", 9usize)
        } else {
            i += 1;
            continue;
        };
        // The prior character must not be `[A-Za-z0-9_]` — otherwise we matched inside a longer
        // identifier like `MY_MODULES.foo` or a table field lookup.
        if i > 0 {
            let prev = bytes[i - 1];
            if prev == b'_' || prev.is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
        }
        // Skip past the base plus its name / bracketed expression.
        let mut j = i + base_len;
        if kind == "_MODULES[" {
            // Walk to the closing bracket.
            let mut depth = 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
        } else {
            // `_G.name` / `_MODULES.name` — walk over the identifier.
                while j < bytes.len()
                    && (bytes[j] == b'_'
                        || bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'.')
                {
                    j += 1;
                }
        }
        // Skip whitespace, then check for `=` (not `==`).
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'=' && bytes.get(j + 1) != Some(&b'=') {
            let (line, col) = line_col(source, i);
            let target = std::str::from_utf8(&bytes[i..j])
                .unwrap_or("(non-utf8)")
                .trim()
                .to_string();
            out.push((line, col, target));
        }
        i = j.max(i + 1);
    }
    out
}

/// Read a Lua source file. On error, produces no diagnostic — [`M0110_SOURCE_MISSING`] already
/// reports missing / unreadable sources; adding a second complaint would be noise.
fn read_lua(root: &Path, rel: &Path) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}

/// The three Lua-source rules — M0300 / M0301 / M0302. Read every `add_script` / `patch_lua` /
/// `replace_lua` source in the Shipment, scan for the danger patterns, emit diagnostics.
///
/// Runs only when [`lint`] has a `root` — the hermetic manifest-only pass has no files to read.
/// Every finding is anchored to a `(line, col)` in the emitted message so a modder can jump to
/// the exact site from their editor.
fn lua_source_checks(manifest: &Manifest, root: &Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // First pass: collect the set of `sModuleName = "…"` names referenced anywhere in this
    // Shipment's Lua. Used by the M0300 check below. Also builds the (index, name, path) list of
    // AddScripts.
    let mut referenced_module_names: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut add_scripts: Vec<(usize, &str, &Path)> = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::AddScript { name, source } => {
                add_scripts.push((index, name.as_str(), source.as_path()));
                if let Some(src) = read_lua(root, source) {
                    for m in scan_module_names(&src) {
                        referenced_module_names.insert(m);
                    }
                }
            }
            Contribution::PatchLua { append, .. } => {
                if let Some(src) = read_lua(root, append) {
                    for m in scan_module_names(&src) {
                        referenced_module_names.insert(m);
                    }
                }
            }
            Contribution::ReplaceLua { source, .. } => {
                if let Some(src) = read_lua(root, source) {
                    for m in scan_module_names(&src) {
                        referenced_module_names.insert(m);
                    }
                }
            }
            _ => {}
        }
    }

    // M0300: every AddScript whose name is referenced as an sModuleName in this Shipment's Lua
    // must inherit an MrxTask subclass at the top of its own source.
    for (index, name, source_path) in &add_scripts {
        if !referenced_module_names.contains(*name) {
            continue;
        }
        let Some(src) = read_lua(root, source_path) else {
            continue;
        };
        if !inherits_mrxtask(&src) {
            out.push(Diagnostic {
                rule: M0300_MISSION_ADD_SCRIPT_NO_INHERIT,
                severity: Severity::Error,
                message: format!(
                    "add_script {name:?} is referenced by a `sModuleName = \"{name}\"` \
                     registration in this Shipment, so at contract activation the engine's \
                     `_ModuleLoaded` will replace `oMission`'s metatable with `{{__index = <this \
                     module>}}`. Without an `inherit(\"MrxTaskContract\")` (or `MrxTaskMission` \
                     / `MrxTaskJob` / `MrxTaskContractOutpost` / …) at the top of {source}, \
                     `oMission:IsActive`, `:Configure`, `:SaveInstance`, `:Cleanup` all resolve \
                     to nil. `RefreshAllPdaMissionDetails` will call `:IsActive()` on every \
                     support drop, unwind the surrounding pcall, and the game degrades silently \
                     (fuel deducted, delivery never fires). Add one of these lines at the top of \
                     the module: {suggestions}.",
                    source = source_path.display(),
                    suggestions = MRXTASK_BASES
                        .iter()
                        .take(4)
                        .map(|b| format!("`inherit(\"{b}\")`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                at: Some(*index),
                fix: Some("inherit(\"MrxTaskContract\")".to_string()),
            });
        }
    }

    // M0301 and M0302: scan every Lua source in the Shipment.
    for (index, c) in manifest.contributions.iter().enumerate() {
        let (rel, kind_label): (&Path, &str) = match c {
            Contribution::AddScript { source, .. } => (source.as_path(), "add_script"),
            Contribution::PatchLua { append, .. } => (append.as_path(), "patch_lua"),
            Contribution::ReplaceLua { source, .. } => (source.as_path(), "replace_lua"),
            _ => continue,
        };
        let Some(src) = read_lua(root, rel) else {
            continue;
        };

        for (line, col, at, kind) in scan_event_create(&src) {
            if !enclosing_function_has_self(&src, at) {
                continue;
            }
            out.push(Diagnostic {
                rule: M0301_BARE_EVENT_CREATE,
                severity: Severity::Error,
                message: format!(
                    "{kind}( called directly at {rel}:{line}:{col} in this {kind_label} \
                     contribution. The handle returned by a direct `Event.Create` is not \
                     tracked by any `MrxTask._tEvents` set, so `DestroyEvents(self)` on mission \
                     Cleanup cannot delete it — the callback keeps firing against a torn-down \
                     mission, capturing `self` past the mission's lifetime. \
                     `Event.CreatePersistent` is worse: it survives level transitions. Call it \
                     through `self:_CreateEvent(nEventId, tArgs, fCallback, tCallbackArgs)` (or \
                     `self:_CreatePersistentEvent(…)`) instead — MrxTask's inherited helper \
                     table-inserts the handle so Cleanup can drain it.",
                    rel = rel.display(),
                ),
                at: Some(index),
                fix: Some("self:_CreateEvent(...)".to_string()),
            });
        }

        for (line, col, target) in scan_global_writes(&src) {
            let sink = if target.starts_with("_G") {
                "the global environment"
            } else {
                "another module's table"
            };
            out.push(Diagnostic {
                rule: M0302_GLOBAL_SHADOWING,
                severity: Severity::Error,
                message: format!(
                    "top-level write to `{target}` at {rel}:{line}:{col} in this {kind_label} \
                     contribution. Assigning to {sink} at file scope silently competes with the \
                     engine's own writes (`dynamic_import` targets `_G`, `_SYS._IMPORT` targets \
                     `_MODULES`) — a `__newindex` set later on either can crash at \
                     `0x0059C82A`. Keep module state module-scoped (a plain local or a table \
                     under this module's own name).",
                    rel = rel.display(),
                ),
                at: Some(index),
                fix: None,
            });
        }

        for (line, col, id) in scan_mission_data_assigns(&src) {
            if is_parseable_mission_id(&id) {
                continue;
            }
            out.push(Diagnostic {
                rule: M0303_MISSION_ID_UNPARSEABLE,
                severity: Severity::Error,
                message: format!(
                    "`WifMissionData.tMissionData` row with key `{id}` at {rel}:{line}:{col} in \
                     this {kind_label} contribution. The briefing dispatcher parses mission ids \
                     via `MrxUtil.ExplodeMissionName` as `<Faction3><Con|Job><NN+digits>` — a key \
                     that does not match this shape wedges the briefing-dialog teardown the \
                     instant the player selects it from a starter root menu (`GetSpielFileName` \
                     at `mrxbriefing.lua:2849` throws on `string.format(\"%02d\", nil)`). \
                     Rename the row to a parseable id (e.g. `AbtCon001`, `MyJob017`) and keep \
                     the human-readable label in `sTitle`.",
                    rel = rel.display(),
                ),
                at: Some(index),
                fix: None,
            });
        }
    }

    out
}

/// Find every statically-visible `WifMissionData.tMissionData[<literal>] = ` or
/// `WifMissionData.tMissionData.<ident> = ` assignment and return `(line, col, mission_id)` for
/// each. Dynamic keys (`tMissionData[e.name] = ...` inside a loop over a table of names) are
/// invisible to this scan — it is a best-effort catch of the common literal-key pattern.
fn scan_mission_data_assigns(source: &str) -> Vec<(usize, usize, String)> {
    const PREFIX: &[u8] = b"WifMissionData.tMissionData";
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + PREFIX.len() < bytes.len() {
        if &bytes[i..i + PREFIX.len()] != PREFIX {
            i += 1;
            continue;
        }
        // Prior byte must not continue an identifier.
        if i > 0 {
            let prev = bytes[i - 1];
            if prev == b'_' || prev.is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
        }
        let mut j = i + PREFIX.len();
        // Skip whitespace between `tMissionData` and the index/accessor.
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
            j += 1;
        }
        let (id_opt, key_end) = match bytes.get(j) {
            Some(b'[') => extract_literal_bracket_key(bytes, j + 1),
            Some(b'.') => extract_dot_ident_key(bytes, j + 1),
            _ => {
                i = j.max(i + 1);
                continue;
            }
        };
        let Some(id) = id_opt else {
            i = j.max(i + 1);
            continue;
        };
        // After the key, skip whitespace and require a bare `=` (not `==`).
        let mut k = key_end;
        while k < bytes.len() && matches!(bytes[k], b' ' | b'\t') {
            k += 1;
        }
        if k < bytes.len() && bytes[k] == b'=' && bytes.get(k + 1) != Some(&b'=') {
            let (line, col) = line_col(source, i);
            out.push((line, col, id));
        }
        i = k.max(i + 1);
    }
    out
}

/// After the opening `[`, pull a single quoted string literal (`"id"` or `'id'`) and return the
/// unquoted body plus the index just after the closing `]`. Returns `(None, idx)` if the bracket
/// contains anything that is not a plain quoted string.
fn extract_literal_bracket_key(bytes: &[u8], start: usize) -> (Option<String>, usize) {
    let mut j = start;
    while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
        j += 1;
    }
    let quote = match bytes.get(j) {
        Some(&q) if q == b'"' || q == b'\'' => q,
        _ => return (None, j),
    };
    j += 1;
    let id_start = j;
    while j < bytes.len() && bytes[j] != quote {
        if bytes[j] == b'\\' {
            // Any escape disqualifies — we are only interested in plain names.
            return (None, j);
        }
        j += 1;
    }
    if j >= bytes.len() {
        return (None, j);
    }
    let id = std::str::from_utf8(&bytes[id_start..j]).ok().map(String::from);
    j += 1; // past closing quote
    while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
        j += 1;
    }
    if bytes.get(j) != Some(&b']') {
        return (None, j);
    }
    (id, j + 1)
}

/// After the `.`, pull a Lua identifier and return it plus the index just after it.
fn extract_dot_ident_key(bytes: &[u8], start: usize) -> (Option<String>, usize) {
    let mut j = start;
    while j < bytes.len() && (bytes[j] == b'_' || bytes[j].is_ascii_alphanumeric()) {
        j += 1;
    }
    if j == start {
        return (None, j);
    }
    (
        std::str::from_utf8(&bytes[start..j]).ok().map(String::from),
        j,
    )
}

/// Does this mission id parse through `MrxUtil.ExplodeMissionName` into a numeric index? The
/// engine takes bytes 1–3 as the faction, 4–6 as `Con`|`Job`, 7–end as the index — which must
/// be convertible via `tonumber`. For the strict literal patterns this scanner surfaces, that is
/// 3 arbitrary chars + exactly `Con` or `Job` + 1+ ascii digits.
fn is_parseable_mission_id(id: &str) -> bool {
    let b = id.as_bytes();
    if b.len() < 7 {
        return false;
    }
    let kind = &b[3..6];
    if kind != b"Con" && kind != b"Job" {
        return false;
    }
    let tail = &b[6..];
    !tail.is_empty() && tail.iter().all(|c| c.is_ascii_digit())
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
