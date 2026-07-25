//! The linter — numbered, documented, gated diagnostics.
//!
//! Plan 01 calls this the crown jewel, and the reason is that **our memory index is a rule set**:
//! every entry is a trap a modder cannot discover on their own. Each rule here carries an `Mxxxx`
//! code, a doc link, and — where the fix is mechanical — the exact replacement text.
//!
//! ## What runs where
//!
//! Everything in this module is **hermetic**: manifest text plus, optionally, the Shipment
//! directory. No game install, no network. That is what lets template CI run `qm lint` on every
//! push when the retail WADs will never be available there.
//!
//! Several of the worst traps *cannot* be checked hermetically — a short texture BODY needs the
//! target's resident mip-chain size from the base WAD, and a dangling LOD rung needs the built
//! block. Those are registered in [`PENDING`] rather than silently absent, so the gap is visible
//! instead of being mistaken for a clean bill of health.
//!
//! ## Gating
//!
//! [`blocks_build`] is the build gate. `Hang` and `Error` block; `Warning` and `Info` do not. The
//! standing mandate is that a build is gated on EXIT CODE, never on a printed count.

use crate::blast::{self, MergeClass};
use crate::discover::{self, SourceIssue};
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
    Rule {
        code: "M0001",
        title: "dangling _P001/2/3 LOD rungs — 549 GB buffer request, open-world stream HANG",
        doc: "docs/modding/field_guide.md#trap-7",
    },
    Rule {
        code: "M0002",
        title: "packed_field under-claims decompressed size — heap overrun",
        doc: "docs/modding/field_guide.md#trap-8",
    },
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
        code: "M0007",
        title: "fully-resident replacement of a MULTI-RUNG texture drops its finer external mips",
        doc: "docs/aset_format.md",
    },
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
pub fn lint(manifest: &Manifest, root: Option<&Path>, names: Option<&NameTable>) -> Vec<Diagnostic> {
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
                let claim = blast::Claim::Script { name: target.clone() };
                let class = blast::merge_class(&claim, blast::Access::Write, blast::Intent::Additive);
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
            Contribution::NativeHook { target, plugin, symbol, .. } => {
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
    diagnostics
        .iter()
        .any(|d| d.severity >= Severity::Error)
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
        for r in RULES.iter().chain(PENDING.iter()) {
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
