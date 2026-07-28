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
//! Several of the worst traps still cannot be checked at all — a short texture BODY needs the
//! target's resident mip-chain size, and the non-resident-costume wedge needs a residency
//! predicate that does not exist yet. Those are registered in [`PENDING`] rather than silently
//! absent, so the gap is visible instead of being mistaken for a clean bill of health.
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

/// A rule: stable code, one-line title, and where the trap is written up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub code: &'static str,
    pub title: &'static str,
    pub doc: &'static str,
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
    doc: "docs/modding/field_guide.md#trap-14",
};
pub const M0130_BARE_HASH: Rule = Rule {
    code: "M0130",
    title: "a hash was written where a name is known",
    doc: "docs/modding/field_guide.md#trap-1",
};
pub const M0140_UNKNOWN_WEARER: Rule = Rule {
    code: "M0140",
    title: "outfit targets a hero the wardrobe has no list for",
    doc: "docs/modding/field_guide.md#trap-15",
};
pub const M0141_UNMERGEABLE_SCRIPT: Rule = Rule {
    code: "M0141",
    title: "patching a script whose composition is not reversed makes the Shipment exclusive",
    doc: "docs/modding/field_guide.md#trap-15",
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
    M0170_BAD_DIGEST,
    M0171_INSECURE_URL,
];

// --- Known, NOT yet implemented -------------------------------------------

/// HANG-class traps that need the game stack or a built WAD, and so cannot run in a hermetic lint.
///
/// **Registered on purpose.** A linter that silently omits its most important rules reads as a
/// clean bill of health, which is worse than no linter. These land with the builder (increment 5),
/// where the WAD stack is in hand.
pub const PENDING: &[Rule] = &[
    // M0001 and M0002 have moved to `ARTIFACT_RULES` — both are answerable against the emitted WAD.
    Rule {
        code: "M0003",
        title: "texture BODY shorter than linear_mip_chain_size — BUFFER_TOO_SMALL, world-load livelock",
        doc: "docs/modding/field_guide.md#trap-7",
    },
    Rule {
        code: "M0004",
        title: "new asset hash minted without an ASET row — loader wedges silently at world-load",
        doc: "docs/modding/field_guide.md#trap-1",
    },
    Rule {
        code: "M0005",
        title: "non-resident costume on the on-demand path — STATE_WAITFORGAME wedge",
        doc: "docs/modding/field_guide.md#trap-12",
    },
    Rule {
        code: "M0006",
        title: "replace_texture target is shared by several materials — collateral reskin",
        doc: "docs/modding/field_guide.md#trap-6",
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
            let hash = mercs2_formats::hash::pandemic_hash_m2(target);
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
    }
    out
}

/// M0001, promoted out of [`PENDING`]. Answerable only against an emitted WAD.
pub const M0001_DANGLING_RUNG: Rule = Rule {
    code: "M0001",
    title: "dangling _P001/2/3 LOD rungs — 549 GB buffer request, open-world stream HANG",
    doc: "docs/modding/field_guide.md#trap-7",
};

/// M0002, promoted out of [`PENDING`]. Answerable only against an emitted WAD.
pub const M0002_PACKED_FIELD_UNDER_CLAIM: Rule = Rule {
    code: "M0002",
    title: "packed_field under-claims decompressed size — heap overrun",
    doc: "docs/modding/field_guide.md#trap-8",
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

    validate_blocks_all(blocks, BlockStage::Emitted)
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
        write!(f, " — see {}", self.rule.doc)
    }
}

/// The wardrobe's hero keys, verified against `wifpmcinterior.lua:155`. An outfit filed under any
/// other key sits in a table nothing ever reads.
pub const WARDROBE_HEROES: [&str; 3] = ["chris", "jennifer", "mattias"];

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
                message: issue.to_string(),
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
                message: s.to_string(),
                at: Some(s.index),
                fix: Some(s.name.clone()),
            });
        }
    }

    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::AddOutfit { wearer, .. } => {
                if !WARDROBE_HEROES.contains(&wearer.as_str()) {
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
        for code in ["M0001", "M0002", "M0180", "M0181", "M0182"] {
            assert!(
                ARTIFACT_RULES.iter().any(|r| r.code == code),
                "{code} unregistered"
            );
        }
        for code in ["M0001", "M0002"] {
            assert!(
                !PENDING.iter().any(|r| r.code == code),
                "{code} is implemented, not pending"
            );
        }
    }
}
