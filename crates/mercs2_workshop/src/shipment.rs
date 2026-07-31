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
use std::path::{Path, PathBuf};

/// What was written, for the caller to report.
pub struct Written {
    pub manifest: PathBuf,
    pub model: PathBuf,
    /// Bone rows recorded in `bones:`.
    pub rows: usize,
    /// Of those, how many differ from what the convention table alone would give — the hand
    /// adjustments, which are the reason the map is worth writing at all.
    pub manual: usize,
    /// Target bones dropped because no name in the corpus resolves them.
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

/// Write `manifest.yaml` plus `src/<model>` into `dir`.
///
/// `rt` is the live retarget — including any manual rows the author set. The map written is
/// [`Retarget::convention_overrides`], the same one the lowering will use, resolved to NAMES:
/// a Shipment is reviewed in a diff and rebuilt against whatever donor it names, not against HIER
/// indices that a different donor orders differently.
pub fn write(
    dir: &Path,
    asset_name: &str,
    wearer: &str,
    donor_name: &str,
    src_glb: &Path,
    rt: &Retarget,
) -> Result<Written, String> {
    if rt.target_bones.is_empty() {
        return Err(format!("target '{donor_name}' has no readable HIER skeleton"));
    }
    std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = src_glb
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model.glb".into());
    let model = dir.join("src").join(&stem);
    std::fs::copy(src_glb, &model).map_err(|e| format!("copy {}: {e}", src_glb.display()))?;

    let overrides = rt.convention_overrides(rt.target_bones.len());
    let mut rows: Vec<(String, Option<String>)> = Vec::new();
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
            None => rows.push((sname.clone(), None)),
            Some(ti) => match rt.target_bones.get(*ti as usize) {
                // A target with no name in the corpus is DROPPED, not written as `hash_XXXXXXXX`.
                // The Quartermaster resolves `bones:` by hashing the name, so a placeholder would
                // hash to a bone the donor does not have and fail the build. Dropping leaves that
                // joint to the automap, which is where it would have landed anyway.
                Some(tn) if !tn.starts_with("hash_") && !tn.starts_with("0x") => {
                    rows.push((sname.clone(), Some(tn.clone())))
                }
                _ => unnamed += 1,
            },
        }
    }
    rows.sort();

    let mut y = String::new();
    y.push_str("format: 1\n");
    y.push_str(&format!(
        "shipment: {{ name: {}, version: 1.0.0, target: retail }}\n",
        slugify(asset_name)
    ));
    y.push_str("contributions:\n");
    y.push_str("  - kind: add_outfit\n");
    y.push_str(&format!("    name: {asset_name}\n"));
    y.push_str(&format!("    slug: {}\n", wardrobe_slug(asset_name)));
    y.push_str(&format!("    display: {asset_name}\n"));
    y.push_str(&format!("    wearer: {wearer}\n"));
    y.push_str(&format!("    model: src/{stem}\n"));
    y.push_str(&format!("    donor: {donor_name}\n"));
    y.push_str("    retarget:\n");
    y.push_str(&format!("      from: {}\n", rt.convention.slug()));
    if !rows.is_empty() {
        y.push_str("      bones:\n");
        for (s_, t_) in &rows {
            match t_ {
                Some(t) => y.push_str(&format!("        {s_}: {t}\n")),
                None => y.push_str(&format!("        {s_}: ~\n")),
            }
        }
    }
    let manifest = dir.join("manifest.yaml");
    std::fs::write(&manifest, y).map_err(|e| format!("write {}: {e}", manifest.display()))?;

    Ok(Written {
        manifest,
        model,
        rows: rows.len(),
        manual,
        unnamed,
    })
}
