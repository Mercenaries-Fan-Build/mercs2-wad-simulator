//! Source-skeleton retarget: map a foreign rig — **ValveBiped** (Source engine), Mixamo, or the
//! Unreal mannequin — onto a Mercs2 HIER character skeleton by bone ROLE (anatomy), not by spatial
//! nearest-neighbour. The same role classifier runs over the source joint names and the target HIER
//! bone names; a source bone maps to whichever target bone shares its role.
//!
//! This is the workshop-side driver for the Skeleton workbench. The heavy lifting on the Mercs2
//! (donor) side — the per-vertex weight remap + injection — already exists in
//! `mercs2_formats::{retarget, mannequin, model_inject}`; this module supplies the detection + the
//! human-readable bone map the inspector shows, and the source→target joint table `apply` consumes.

/// The recognised source-rig conventions. Detection is by joint-name shape.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceRig {
    /// Valve Source engine — `ValveBiped.Bip01_*` (Z-up, inches).
    ValveBiped,
    /// Mixamo auto-rig — `mixamorig:*` (Y-up, cm).
    Mixamo,
    /// Unreal Engine mannequin — `pelvis`/`spine_0x`/`thigh_l` (Z-up, cm).
    Unreal,
    /// Unknown convention — mapped on generic anatomy keywords alone.
    Generic,
}

impl SourceRig {
    pub fn label(self) -> &'static str {
        match self {
            SourceRig::ValveBiped => "ValveBiped (Source)",
            SourceRig::Mixamo => "Mixamo",
            SourceRig::Unreal => "Unreal mannequin",
            SourceRig::Generic => "generic rig",
        }
    }

    /// Detect the convention from the joint-name set.
    pub fn detect(names: &[String]) -> SourceRig {
        let any = |needle: &str| names.iter().any(|n| n.to_ascii_lowercase().contains(needle));
        if any("valvebiped") || any("bip01") {
            SourceRig::ValveBiped
        } else if any("mixamorig") {
            SourceRig::Mixamo
        } else if any("spine_0") || (any("thigh_l") && any("pelvis")) {
            SourceRig::Unreal
        } else {
            SourceRig::Generic
        }
    }

    /// The up-axis + unit scale the convention ships in, applied on import to reach Mercs2 space
    /// (meters, Y-up). Heuristic defaults surfaced in the "Orientation fix" card.
    fn orientation(self) -> (UpAxis, f32) {
        match self {
            SourceRig::ValveBiped => (UpAxis::Z, 0.0254), // inches → m
            SourceRig::Unreal => (UpAxis::Z, 0.01),       // cm → m
            SourceRig::Mixamo => (UpAxis::Y, 0.01),       // cm → m
            SourceRig::Generic => (UpAxis::Y, 1.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UpAxis {
    Y,
    Z,
}

/// How a source bone was matched to its target.
#[allow(dead_code)] // `Manual` is reserved for user overrides (planned)
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Both sides classified by a primary anatomy keyword.
    Auto,
    /// Matched via a synonym (upperarm↔bicep, calf↔shin, spine↔chest) — worth a glance.
    Fuzzy,
    /// A user override (reserved).
    Manual,
    /// No target bone shares this source bone's role.
    Unmapped,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    L,
    R,
}

/// The anatomical role a bone plays — the currency both sides are classified into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Pelvis,
    Spine,
    Neck,
    Head,
    Clav(Side),
    UpperArm(Side),
    Forearm(Side),
    Hand(Side),
    Thigh(Side),
    Shin(Side),
    Foot(Side),
}

/// One row of the retarget: a source joint and the target bone it maps to.
pub struct BoneMap {
    pub source: String,
    pub source_index: usize,
    pub role: Option<Role>,
    pub target_name: Option<String>,
    pub target_index: Option<usize>,
    pub confidence: Confidence,
}

pub struct Retarget {
    pub convention: SourceRig,
    pub source_bones: Vec<String>,
    pub map: Vec<BoneMap>,
    pub up_axis: UpAxis,
    pub scale: f32,
}

impl Retarget {
    /// Build a retarget from the source joint names and (optionally) the resolved target HIER bone
    /// names. With no target every row is `Unmapped` (detection + role classification still run, so
    /// the workbench shows the source rig before a target is picked).
    pub fn build(source_bones: Vec<String>, target_bones: &[String]) -> Retarget {
        let convention = SourceRig::detect(&source_bones);
        let (up_axis, scale) = convention.orientation();

        // Target role → bone index (first match wins).
        let mut target_by_role: Vec<(Role, usize, bool)> = Vec::new();
        for (ti, tn) in target_bones.iter().enumerate() {
            if let Some((role, _fuzzy)) = classify(tn) {
                if !target_by_role.iter().any(|(r, _, _)| role_eq(*r, role)) {
                    target_by_role.push((role, ti, false));
                }
            }
        }
        let find_target = |role: Role| -> Option<usize> {
            target_by_role.iter().find(|(r, _, _)| role_eq(*r, role)).map(|(_, i, _)| *i)
        };

        let map = source_bones
            .iter()
            .enumerate()
            .map(|(si, name)| {
                let classified = classify(name);
                let (role, fuzzy) = match classified {
                    Some((r, f)) => (Some(r), f),
                    None => (None, false),
                };
                let target_index = role.and_then(find_target);
                let target_name = target_index.and_then(|i| target_bones.get(i).cloned());
                let confidence = match (role, target_index) {
                    (Some(_), Some(_)) if fuzzy => Confidence::Fuzzy,
                    (Some(_), Some(_)) => Confidence::Auto,
                    _ => Confidence::Unmapped,
                };
                BoneMap { source: name.clone(), source_index: si, role, target_name, target_index, confidence }
            })
            .collect();

        Retarget { convention, source_bones, map, up_axis, scale }
    }

    pub fn mapped_count(&self) -> usize {
        self.map.iter().filter(|m| m.target_index.is_some()).count()
    }

    pub fn up_axis_label(&self) -> &'static str {
        match self.up_axis {
            UpAxis::Y => "Y (native)",
            UpAxis::Z => "Z → Y",
        }
    }

    /// The source-joint-index → target-HIER-bone-index table `apply` consumes. Unmapped source
    /// joints fall back to the target's pelvis (or bone 0) so no vertex is left unbound.
    pub fn joint_table(&self, target_bone_count: usize) -> Vec<usize> {
        let fallback = self
            .map
            .iter()
            .find(|m| matches!(m.role, Some(Role::Pelvis)))
            .and_then(|m| m.target_index)
            .unwrap_or(0)
            .min(target_bone_count.saturating_sub(1));
        let mut table = vec![fallback; self.source_bones.len()];
        for m in &self.map {
            if let Some(ti) = m.target_index {
                if m.source_index < table.len() {
                    table[m.source_index] = ti.min(target_bone_count.saturating_sub(1));
                }
            }
        }
        table
    }
}

fn role_eq(a: Role, b: Role) -> bool {
    use Role::*;
    match (a, b) {
        (Pelvis, Pelvis) | (Spine, Spine) | (Neck, Neck) | (Head, Head) => true,
        (Clav(x), Clav(y))
        | (UpperArm(x), UpperArm(y))
        | (Forearm(x), Forearm(y))
        | (Hand(x), Hand(y))
        | (Thigh(x), Thigh(y))
        | (Shin(x), Shin(y))
        | (Foot(x), Foot(y)) => x == y,
        _ => false,
    }
}

/// Which body side a bone name denotes, if any. Conservative: only unambiguous markers
/// (`left`/`right`, and an `l`/`r` immediately after an underscore — which covers ValveBiped
/// `Bip01_L_*`, Unreal `thigh_l`, and Mercs2 `Bone_LBicep`).
fn side_of(n: &str) -> Option<Side> {
    if n.contains("left") {
        return Some(Side::L);
    }
    if n.contains("right") {
        return Some(Side::R);
    }
    if n.contains("_l") {
        return Some(Side::L);
    }
    if n.contains("_r") {
        return Some(Side::R);
    }
    None
}

/// Classify a bone name into an anatomical role. Returns `(role, fuzzy)` where `fuzzy` means the
/// match came through a synonym rather than the primary keyword. Order matters: specific keywords
/// are tested before the broad ones (`forearm` before `arm`, `upleg` before `leg`).
fn classify(name: &str) -> Option<(Role, bool)> {
    let n = name.to_ascii_lowercase();
    let side = side_of(&n);

    if n.contains("hips") || n.contains("pelvis") {
        return Some((Role::Pelvis, false));
    }
    if n.contains("head") {
        return Some((Role::Head, false));
    }
    if n.contains("neck") {
        return Some((Role::Neck, false));
    }
    if n.contains("clav") || n.contains("shoulder") {
        return side.map(|s| (Role::Clav(s), n.contains("shoulder")));
    }
    if n.contains("forearm") || n.contains("lowerarm") {
        return side.map(|s| (Role::Forearm(s), n.contains("lowerarm")));
    }
    if n.contains("upperarm") || n.contains("uparm") || n.contains("bicep") {
        return side.map(|s| (Role::UpperArm(s), n.contains("uparm")));
    }
    // Generic "arm" (Mixamo `LeftArm` = upper arm) — after fore/upper have been ruled out.
    if n.contains("arm") {
        return side.map(|s| (Role::UpperArm(s), true));
    }
    if n.contains("hand") || n.contains("wrist") {
        return side.map(|s| (Role::Hand(s), n.contains("wrist")));
    }
    if n.contains("thigh") || n.contains("upleg") || n.contains("upperleg") {
        return side.map(|s| (Role::Thigh(s), !n.contains("thigh")));
    }
    if n.contains("foot") || n.contains("ankle") {
        return side.map(|s| (Role::Foot(s), n.contains("ankle")));
    }
    if n.contains("calf") || n.contains("shin") || n.contains("lowerleg") || n.contains("leg") {
        return side.map(|s| (Role::Shin(s), !n.contains("shin")));
    }
    if n.contains("spine") || n.contains("chest") || n.contains("thorax") {
        // spine↔chest is many-to-one; flag it so the user can confirm which spine bone.
        return Some((Role::Spine, n.contains("spine")));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detect_valvebiped() {
        let src = names(&["ValveBiped.Bip01_Pelvis", "ValveBiped.Bip01_Spine"]);
        assert!(SourceRig::detect(&src) == SourceRig::ValveBiped);
    }

    #[test]
    fn valvebiped_maps_onto_mercs2() {
        let src = names(&[
            "ValveBiped.Bip01_Pelvis",
            "ValveBiped.Bip01_Spine1",
            "ValveBiped.Bip01_Head",
            "ValveBiped.Bip01_L_UpperArm",
            "ValveBiped.Bip01_L_Forearm",
            "ValveBiped.Bip01_L_Thigh",
            "ValveBiped.Bip01_L_Calf",
        ]);
        let tgt = names(&[
            "Bone_Hips", "Bone_Chest", "Bone_Head", "Bone_LBicep", "Bone_LForearm", "Bone_LThigh",
            "Bone_LShin",
        ]);
        let r = Retarget::build(src, &tgt);
        // pelvis, chest(spine), head, upperarm(bicep), forearm, thigh, shin(calf) all resolve.
        assert_eq!(r.mapped_count(), 7);
        // The left forearm must map to the left forearm, not the right.
        let fa = r.map.iter().find(|m| m.source.contains("Forearm")).unwrap();
        assert_eq!(fa.target_name.as_deref(), Some("Bone_LForearm"));
    }

    #[test]
    fn side_disambiguation() {
        assert!(side_of("bip01_l_upperarm") == Some(Side::L));
        assert!(side_of("bone_rbicep") == Some(Side::R));
        assert!(side_of("bone_hips").is_none());
    }
}
