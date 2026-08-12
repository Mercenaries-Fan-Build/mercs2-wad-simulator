//! Write a buildable Shipment — the artifact `mercs2_quartermaster` consumes.
//!
//! # Why this is here and not only on the CLI
//!
//! `mercs2_quartermaster::manifest` documents `retarget.bones` as *"the RESOLVED bone map, written
//! by the Workshop"*, justified by: *"three conventions need hand-verified correction tables, and
//! **any bone can be hand-adjusted**. Carrying only `from:` meant a rebuild silently differed from
//! what the author approved."*
//!
//! Hand adjustment is the operative half. The convention tables are derivable from the source file
//! — the Quartermaster derives them itself when no map is supplied — so a map that carries only the
//! table adds nothing a rebuild could not work out. What a rebuild genuinely **cannot** recover is
//! the author moving one bone in the Skeleton workbench, because that decision exists nowhere but
//! in the running UI.
//!
//! So the emitter takes a live [`Retarget`], and the GUI action passes the one the user has been
//! editing. A headless entry point that rebuilds a `Retarget` from the file (`--export-shipment`)
//! can only ever produce the derivable half — useful for scripting, not a substitute.

use crate::retarget::Retarget;
use std::path::PathBuf;

/// How much the recorded bone map is worth, for the caller to report.
pub struct BoneMapStats {
    /// Bone rows recorded in `bones:`.
    pub rows: usize,
    /// Of those, how many differ from what the convention table alone would give — the hand
    /// adjustments, which are the reason the map is worth writing at all.
    pub manual: usize,
    /// Rows written as a bare `0xHHHHHHHH` because no corpus names that target bone. Legal, and
    /// preferable to dropping them — but worth reporting, since a hash is one-way and undiffable.
    pub unnamed: usize,
}

/// `shipment.name` is a package slug (`^[a-z0-9]+(-[a-z0-9]+)*$`); a contribution's `name` is an
/// ASSET name (`pmc_hum_*`, underscores). Different grammars — reusing one for both fails the
/// Quartermaster's linter.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "my-outfit".into()
    } else {
        t
    }
}

/// A Lua identifier for the wardrobe table.
pub fn wardrobe_slug(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if up {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            up = false;
        } else {
            up = true;
        }
    }
    if out.is_empty() {
        "MyOutfit".into()
    } else {
        out
    }
}

/// Build the `add_outfit` contribution for the CURRENT retarget.
///
/// Returns a `Contribution` value rather than writing a file. Three things follow from that, all of
/// them fixes:
///
/// * **The Shipment is no longer clobbered.** This used to write a whole `manifest.yaml` over a
///   folder it had just asked the user to pick, so a bench could only ever produce a fresh
///   single-contribution Shipment — never add to the one already open. The caller now routes it
///   through `Panel::upsert_contribution`, so a retarget joins whatever is being assembled.
/// * **The YAML is serialized, not formatted.** The old emitter built the document with `format!`,
///   and its own `yaml_scalar` guard was dead code — so a target bone with no known name was
///   written as a BARE `0xE54047D5`, which YAML may read back as an integer. That is exactly the
///   coercion the guard existed to prevent. Serializing a real `Contribution` through
///   `mercs2_quartermaster::to_yaml` makes the bug unrepresentable.
/// * **`wearer` is a parameter that is actually passed.** The call site hard-coded `"mattias"`
///   while the Shipment page offered a live three-way choice, so the two halves could not agree.
///
/// `model` must already be `src/`-relative — see `quartermaster::Panel::import_source`, which
/// copies the bytes in. `rt` is the live retarget, hand adjustments included; that is the half a
/// headless rebuild cannot recover and the whole reason the map is written at all.
pub fn outfit_contribution(
    asset_name: &str,
    wearer: &str,
    donor_name: &str,
    model: PathBuf,
    rt: &Retarget,
) -> Result<(mercs2_quartermaster::manifest::Contribution, BoneMapStats), String> {
    use mercs2_quartermaster::manifest::{Contribution, Retarget as QmRetarget, Textures};

    if rt.target_bones.is_empty() {
        return Err(format!("target '{donor_name}' has no readable HIER skeleton"));
    }

    let overrides = rt.convention_overrides(rt.target_bones.len());
    let mut rows: std::collections::BTreeMap<String, Option<String>> = Default::default();
    let (mut unnamed, mut manual) = (0usize, 0usize);
    for (si, tgt) in &overrides {
        let Some(sname) = rt.source_bones.get(*si) else {
            continue;
        };
        if rt
            .map
            .iter()
            .any(|m| m.source_index == *si && m.confidence == crate::retarget::Confidence::Manual)
        {
            manual += 1;
        }
        match tgt {
            None => {
                rows.insert(sname.clone(), None);
            }
            Some(ti) => match rt.target_bones.get(*ti as usize) {
                None => unnamed += 1,
                Some(tn) => {
                    // A NAME is preferred, a bare hash is legal — Plan 04 decision 3, and the
                    // Quartermaster resolves `bones:` through `asset_hash`, which accepts either.
                    //
                    // So an unnameable target is written as `0xHHHHHHHH` rather than dropped. It
                    // has to be: 21 of pmc_hum_mattias's 116 bones have no name in ANY corpus this
                    // project holds, and dropping them left the map silently incomplete — which
                    // defeats the one thing it exists for. The two spellings a target can arrive
                    // in are the workbench's `0x…` and `TargetSkeleton`'s `hash_…`; both mean the
                    // same thing, and only the first is a legal reference.
                    let name = match tn.strip_prefix("hash_") {
                        Some(hex) => {
                            unnamed += 1;
                            format!("0x{hex}")
                        }
                        None if tn.starts_with("0x") || tn.starts_with("0X") => {
                            unnamed += 1;
                            tn.clone()
                        }
                        None => tn.clone(),
                    };
                    rows.insert(sname.clone(), Some(name));
                }
            },
        }
    }

    let stats = BoneMapStats { rows: rows.len(), manual, unnamed };
    let c = Contribution::AddOutfit {
        name: asset_name.to_string(),
        slug: wardrobe_slug(asset_name),
        display: asset_name.to_string(),
        wearer: wearer.to_string(),
        // This scaffolds an INJECTED outfit from a craft bench, so it always carries a model file.
        model: Some(model),
        donor: Some(donor_name.to_string()),
        textures: Textures::default(),
        retarget: Some(QmRetarget {
            from: rt.convention.slug().to_string(),
            bones: (!rows.is_empty()).then_some(rows),
        }),
        single_group: false,
    };
    Ok((c, stats))
}
