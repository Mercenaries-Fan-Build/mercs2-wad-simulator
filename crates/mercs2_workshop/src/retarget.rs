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
    /// Call of Duty / IW-engine rig — `tag_origin`, `j_mainroot`, `j_spinelower`, `j_shoulder_le`,
    /// `j_elbow_le`, `j_hip_le`, `j_knee_le`, … (side markers `_le`/`_ri`). Its joint words don't
    /// line up with the generic anatomy keywords (`j_shoulder` is the UPPER ARM, `j_hip` the THIGH,
    /// `j_knee` the SHIN, `j_elbow` the FOREARM), so it gets an explicit role table — see [`classify_cod`].
    CallOfDuty,
    /// Unknown convention — mapped on generic anatomy keywords alone.
    Generic,
}

impl SourceRig {
    pub fn label(self) -> &'static str {
        match self {
            SourceRig::ValveBiped => "ValveBiped (Source)",
            SourceRig::Mixamo => "Mixamo",
            SourceRig::Unreal => "Unreal mannequin",
            SourceRig::CallOfDuty => "Call of Duty (IW-engine)",
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
        } else if any("tag_origin") || any("j_mainroot") || (any("j_spinelower") && any("j_shoulder")) {
            SourceRig::CallOfDuty
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
            // CoD assets arrive via a Y-up glTF export (Blender `export_yup`); the importer bakes
            // world transforms, so no extra fix is applied here.
            SourceRig::CallOfDuty => (UpAxis::Y, 1.0),
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
    /// Every target HIER bone name, in index order — the option list for a manual override dropdown.
    pub target_bones: Vec<String>,
    pub map: Vec<BoneMap>,
    pub up_axis: UpAxis,
    pub scale: f32,
    /// Source bind-pose bone positions (glTF skin space), index-aligned to `source_bones`. Empty if
    /// the import carried no skeleton positions. Used by [`Retarget::align_by_position`].
    pub source_pos: Vec<[f32; 3]>,
    /// Target bind-pose bone positions (Mercs2 world), index-aligned to `target_bones`.
    pub target_pos: Vec<[f32; 3]>,
    /// Source inverse-bind matrices (glTF column-major), index-aligned to `source_bones`. Consumed by
    /// the RetargetApply rebind (`newvert = Σ w · TargetBind[t] · SourceInvBind[s] · vert`).
    pub source_ibm: Vec<[[f32; 4]; 4]>,
    /// Parent index per source / target bone (`-1` = root) — the hierarchy the Skeleton workbench
    /// draws as side-by-side trees. Empty when unknown.
    pub source_parents: Vec<i32>,
    pub target_parents: Vec<i32>,
}

impl Retarget {
    /// Build a retarget from the source joint names and (optionally) the resolved target HIER bone
    /// names. With no target every row is `Unmapped` (detection + role classification still run, so
    /// the workbench shows the source rig before a target is picked). Position-free wrapper.
    pub fn build(source_bones: Vec<String>, target_bones: &[String]) -> Retarget {
        Self::build_with_pos(source_bones, Vec::new(), target_bones.to_vec(), Vec::new())
    }

    /// Full build with bind-pose positions on both sides, enabling [`align_by_position`].
    pub fn build_with_pos(
        source_bones: Vec<String>,
        source_pos: Vec<[f32; 3]>,
        target_bones: Vec<String>,
        target_pos: Vec<[f32; 3]>,
    ) -> Retarget {
        Self::build_full(source_bones, source_pos, Vec::new(), Vec::new(), target_bones, target_pos, Vec::new())
    }

    /// [`build_with_pos`] plus source inverse-bind matrices (for the rebind) and both parent arrays
    /// (for the Skeleton workbench trees).
    #[allow(clippy::too_many_arguments)]
    pub fn build_full(
        source_bones: Vec<String>,
        source_pos: Vec<[f32; 3]>,
        source_ibm: Vec<[[f32; 4]; 4]>,
        source_parents: Vec<i32>,
        target_bones: Vec<String>,
        target_pos: Vec<[f32; 3]>,
        target_parents: Vec<i32>,
    ) -> Retarget {
        let convention = SourceRig::detect(&source_bones);
        let (up_axis, scale) = convention.orientation();

        let has_src_pos = source_pos.len() == source_bones.len() && !source_pos.is_empty();
        let has_tgt_pos = target_pos.len() == target_bones.len() && !target_pos.is_empty();

        // Classify both sides. The source uses its convention's table (Call of Duty) when the words
        // don't line up with the generic keywords; the target always classifies on Mercs2 names.
        let src_roles: Vec<Option<(Role, bool)>> = source_bones
            .iter()
            .map(|n| match convention {
                SourceRig::CallOfDuty => classify_cod(n),
                _ => classify(n),
            })
            .collect();
        let src_role_only: Vec<Option<Role>> = src_roles.iter().map(|o| o.map(|(r, _)| r)).collect();
        let tgt_roles: Vec<Option<Role>> = target_bones.iter().map(|n| classify(n).map(|(r, _)| r)).collect();

        // Proximal reference (pelvis) for ordering each role's bones proximal→distal.
        let src_root = role_root(&src_role_only, &source_pos, has_src_pos);
        let tgt_root = role_root(&tgt_roles, &target_pos, has_tgt_pos);

        // Gather PRIMARY (deform) bone indices per role. Auxiliary bones are excluded: the target's
        // attachment points (`bone_attach_*`) and twist/roll helpers sit at the body centre and would
        // suck arm/spine verts inward; the source's proc/twist/dummy helpers would eat the real chain
        // slots. So only primary target bones are assignable slots, and only primary source bones
        // consume them. A role can own several primary bones (the spine chain) — those spread across
        // the role's distinct targets proximal→distal (by distance from the pelvis, else HIER order).
        let group_primary =
            |names: &[String], roles: &[Option<Role>], is_aux: fn(&str) -> bool, pos: &[[f32; 3]], has_pos: bool, root: [f32; 3]| {
                let mut groups: Vec<(Role, Vec<usize>)> = Vec::new();
                for (i, r) in roles.iter().enumerate() {
                    if let Some(role) = r {
                        if is_aux(names[i].as_str()) {
                            continue;
                        }
                        match groups.iter_mut().find(|(rr, _)| role_eq(*rr, *role)) {
                            Some((_, v)) => v.push(i),
                            None => groups.push((*role, vec![i])),
                        }
                    }
                }
                if has_pos {
                    for (_, v) in groups.iter_mut() {
                        v.sort_by(|&a, &b| dist2(pos[a], root).total_cmp(&dist2(pos[b], root)));
                    }
                }
                groups
            };
        let role_targets = group_primary(&target_bones, &tgt_roles, is_aux_target, &target_pos, has_tgt_pos, tgt_root);
        let role_sources = group_primary(&source_bones, &src_role_only, is_aux_source, &source_pos, has_src_pos, src_root);

        let mut assign: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        // Primary source bones spread across the role's distinct primary targets; surplus folds onto the last.
        for (role, srcs) in &role_sources {
            if let Some((_, tgts)) = role_targets.iter().find(|(rr, _)| role_eq(*rr, *role)) {
                if tgts.is_empty() {
                    continue;
                }
                for (i, &si) in srcs.iter().enumerate() {
                    assign.insert(si, tgts[i.min(tgts.len() - 1)]);
                }
            }
        }
        // Auxiliary source bones fold onto the same target as the NEAREST primary source bone of their
        // role (a twist/proc bone follows its parent joint), or the role's first target as a fallback.
        for (si, r) in src_role_only.iter().enumerate() {
            if assign.contains_key(&si) || !is_aux_source(source_bones[si].as_str()) {
                continue;
            }
            let Some(role) = r else { continue };
            let prims = role_sources.iter().find(|(rr, _)| role_eq(*rr, *role));
            let via_nearest = if has_src_pos {
                prims.and_then(|(_, ps)| {
                    ps.iter()
                        .min_by(|&&a, &&b| {
                            dist2(source_pos[si], source_pos[a]).total_cmp(&dist2(source_pos[si], source_pos[b]))
                        })
                        .and_then(|&np| assign.get(&np).copied())
                })
            } else {
                None
            };
            let target = via_nearest
                .or_else(|| role_targets.iter().find(|(rr, _)| role_eq(*rr, *role)).and_then(|(_, t)| t.first().copied()));
            if let Some(t) = target {
                assign.insert(si, t);
            }
        }

        // EXPLICIT source→Mercs2 table (authoritative) for recognised conventions (CoD / ValveBiped /
        // Mixamo / Unreal). Overrides the heuristic for every deform + finger bone we've hand-matched,
        // so fingers bind 1:1 and near-duplicate targets can't be mis-picked. Bones absent from the
        // table (helpers, metacarpals) keep their heuristic/fold result; Generic rigs get no override.
        let explicit: std::collections::HashSet<usize> = {
            let by_name: std::collections::HashMap<String, usize> =
                target_bones.iter().enumerate().map(|(i, n)| (n.to_ascii_lowercase(), i)).collect();
            let mut set = std::collections::HashSet::new();
            for (si, name) in source_bones.iter().enumerate() {
                if let Some(ti) =
                    explicit_target_name(convention, name).and_then(|t| by_name.get(&t.to_ascii_lowercase()).copied())
                {
                    assign.insert(si, ti);
                    set.insert(si);
                }
            }
            set
        };

        // GEAR → ATTACH point. Holster/pouch/sling/cosmetic source bones have no anatomical match; they
        // fold to a THIGH by default, which drags the gear into the crotch. Map them instead to Jen's
        // nearest ATTACHMENT bone (`bone_attach_hipright/left/back*/chest`) — exactly where such gear
        // mounts — using the same anchor-fit transform (centroid + scale + yaw) as `align_by_position`.
        if has_src_pos && has_tgt_pos {
            let anchors: Vec<([f32; 3], [f32; 3])> =
                assign.iter().map(|(&si, &ti)| (source_pos[si], target_pos[ti])).collect();
            // Only TORSO-region attach points — gear mounts on the hips/back/chest, never the hands.
            let attach: Vec<usize> = (0..target_bones.len())
                .filter(|&i| {
                    let n = target_bones[i].to_ascii_lowercase();
                    n.contains("attach")
                        && (n.contains("hip") || n.contains("back") || n.contains("chest"))
                })
                .collect();
            if anchors.len() >= 3 && !attach.is_empty() {
                let cs = centroid(anchors.iter().map(|a| a.0));
                let ct = centroid(anchors.iter().map(|a| a.1));
                let ss = spread(anchors.iter().map(|a| a.0), cs);
                let st = spread(anchors.iter().map(|a| a.1), ct);
                let scl = if ss > 1e-6 { st / ss } else { 1.0 };
                let (mut num, mut den) = (0.0f32, 0.0f32);
                for (s, t) in &anchors {
                    let (sx, sz) = (s[0] - cs[0], s[2] - cs[2]);
                    let (tx, tz) = (t[0] - ct[0], t[2] - ct[2]);
                    num += sx * tz - sz * tx;
                    den += sx * tx + sz * tz;
                }
                let (sin, cos) = num.atan2(den).sin_cos();
                let align = |p: [f32; 3]| -> [f32; 3] {
                    let (dx, dy, dz) = (p[0] - cs[0], p[1] - cs[1], p[2] - cs[2]);
                    [ct[0] + scl * (cos * dx + sin * dz), ct[1] + scl * dy, ct[2] + scl * (-sin * dx + cos * dz)]
                };
                for (si, name) in source_bones.iter().enumerate() {
                    let ln = name.to_ascii_lowercase();
                    if !(ln.contains("holster") || ln.contains("pouch") || ln.contains("sling")
                        || ln.contains("cosmetic") || ln.contains("geist") || ln.contains("knife"))
                    {
                        continue;
                    }
                    let sp = align(source_pos[si]);
                    if let Some(&ti) = attach
                        .iter()
                        .min_by(|&&a, &&b| dist2(sp, target_pos[a]).total_cmp(&dist2(sp, target_pos[b])))
                    {
                        assign.insert(si, ti);
                    }
                }
            }
        }

        let map = source_bones
            .iter()
            .enumerate()
            .map(|(si, name)| {
                let (role, fuzzy) = match src_roles[si] {
                    Some((r, f)) => (Some(r), f),
                    None => (None, false),
                };
                let target_index = assign.get(&si).copied();
                let target_name = target_index.and_then(|i| target_bones.get(i).cloned());
                let confidence = match (role, target_index) {
                    (_, None) => Confidence::Unmapped,
                    _ if explicit.contains(&si) => Confidence::Auto, // hand-authored table
                    (Some(_), Some(_)) if fuzzy => Confidence::Fuzzy,
                    (Some(_), Some(_)) => Confidence::Auto,
                    (None, Some(_)) => Confidence::Fuzzy, // heuristic fold of a no-role bone
                };
                BoneMap { source: name.clone(), source_index: si, role, target_name, target_index, confidence }
            })
            .collect();

        let mut r = Retarget {
            convention,
            source_bones,
            target_bones,
            map,
            up_axis,
            scale,
            source_pos,
            target_pos,
            source_ibm,
            source_parents,
            target_parents,
        };
        // Route the auto-mapped rows through the AUTHORITATIVE `char_skin::automap` (Logan's proven
        // mapper) — the heuristic role classifier has no spine-root-is-pelvis rule and mis-detects
        // rigs (e.g. a `C1_*` game rip like 50 Cent that trips the loose Unreal keyword check). This
        // runs for EVERY convention except CoD (whose `j_shoulder` = upper arm naming the generic
        // keyword mapper genuinely can't read). Rows the convention's HAND-VERIFIED explicit table
        // already claimed (`explicit`) are preserved.
        if r.convention != SourceRig::CallOfDuty {
            r.remap_via_char_skin(&explicit);
        }
        r
    }

    /// Overwrite the AUTO bone-map rows using [`mercs2_formats::char_skin::automap`] — the
    /// parity-verified port of Logan's `mercs2-mesher` mapper, which is the same mapper the
    /// faithful preview/export drive. This is ground truth: e.g. the spine-root-is-pelvis rule
    /// (a torso root that PARENTS the leg chains becomes HIPS, not spine1) and the ladder that
    /// spreads the remaining torso across spine1/spine2 — so `torso_joint_*` maps like Logan's,
    /// and no bone is forced onto chest when the rig has no chest joint. MANUAL rows (user
    /// dropdown overrides) are preserved. Emits NPC-84 HIER indices (hero-100 is a known gap on
    /// both sides), so target index == HIER index for an NPC donor.
    pub fn remap_via_char_skin(&mut self, preserve: &std::collections::HashSet<usize>) {
        use mercs2_formats::char_skin::automap::{automap, Rig};
        let n = self.source_bones.len();
        if n == 0 || self.source_parents.len() != n {
            return;
        }
        let joint_nodes: Vec<usize> = (0..n).collect();
        let am = automap(&Rig {
            joint_nodes: &joint_nodes,
            node_parent: &self.source_parents,
            node_name: &self.source_bones,
        });
        for m in self.map.iter_mut() {
            if m.confidence == Confidence::Manual || preserve.contains(&m.source_index) {
                continue; // never clobber a user override or a hand-verified explicit-table row
            }
            let j = m.source_index;
            let (hier, conf) = if let Some(&h) = am.mapped.get(&j) {
                (Some(h), Confidence::Auto)
            } else if let Some(&h) = am.inherited.get(&j) {
                (Some(h), Confidence::Fuzzy) // inherited from nearest mapped ancestor
            } else {
                (None, Confidence::Unmapped)
            };
            // automap emits CANONICAL NPC-84 indices; resolve each onto THIS target skeleton BY NAME
            // (a HERO donor like mattias_v2 reorders/extends the HIER, so the raw index lands on the
            // wrong bone — e.g. index 3 is `bone_root`, not `Bone_Hips`).
            let ti = hier
                .and_then(mercs2_formats::char_skin::npc84_bone_name)
                .and_then(|name| self.target_bones.iter().position(|t| t.eq_ignore_ascii_case(name)));
            m.target_index = ti;
            m.target_name = ti.and_then(|i| self.target_bones.get(i).cloned());
            m.confidence = if ti.is_some() { conf } else { Confidence::Unmapped };
        }
    }

    pub fn mapped_count(&self) -> usize {
        self.map.iter().filter(|m| m.target_index.is_some()).count()
    }

    /// Manually set (or clear, with `None`) a source bone's target. Marks the row `Manual`. This is
    /// the user override the dropdown in the bone-map grid drives.
    pub fn set_manual(&mut self, source_index: usize, target_index: Option<usize>) {
        let target_name = target_index.and_then(|i| self.target_bones.get(i).cloned());
        if let Some(m) = self.map.iter_mut().find(|m| m.source_index == source_index) {
            m.target_index = target_index;
            m.target_name = target_name;
            m.confidence =
                if target_index.is_some() { Confidence::Manual } else { Confidence::Unmapped };
        }
    }

    /// Fill still-UNMAPPED source bones by nearest target bone in a shared frame. The bones already
    /// matched by name are the anchors: they fix a similarity transform (centroid align + uniform
    /// scale + yaw rotation about the shared Y-up axis, which absorbs a facing-direction difference)
    /// from source space onto target space. Each unmapped source bone then takes the nearest target
    /// bone in that aligned frame. Name/manual maps are left untouched. Returns how many were filled.
    /// No-op (returns 0) without positions on both sides or with too few anchors to fit a transform.
    pub fn align_by_position(&mut self) -> usize {
        if self.source_pos.len() != self.source_bones.len()
            || self.target_pos.len() != self.target_bones.len()
            || self.source_pos.is_empty()
            || self.target_pos.is_empty()
        {
            return 0;
        }
        // Anchor pairs from the current (name/manual) mapping.
        let anchors: Vec<([f32; 3], [f32; 3])> = self
            .map
            .iter()
            .filter_map(|m| m.target_index.map(|ti| (self.source_pos[m.source_index], self.target_pos[ti])))
            .collect();
        if anchors.len() < 3 {
            return 0;
        }
        let cs = centroid(anchors.iter().map(|a| a.0));
        let ct = centroid(anchors.iter().map(|a| a.1));
        let ss = spread(anchors.iter().map(|a| a.0), cs);
        let st = spread(anchors.iter().map(|a| a.1), ct);
        let scale = if ss > 1e-6 { st / ss } else { 1.0 };
        // Best yaw (rotation about Y) aligning the centred anchor XZ clouds: 2D Procrustes.
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for (s, t) in &anchors {
            let (sx, sz) = (s[0] - cs[0], s[2] - cs[2]);
            let (tx, tz) = (t[0] - ct[0], t[2] - ct[2]);
            num += sx * tz - sz * tx;
            den += sx * tx + sz * tz;
        }
        let yaw = num.atan2(den);
        let (sin, cos) = yaw.sin_cos();
        // Aligned source position: ct + scale * R_y(yaw) * (p - cs).
        let align = |p: [f32; 3]| -> [f32; 3] {
            let (dx, dy, dz) = (p[0] - cs[0], p[1] - cs[1], p[2] - cs[2]);
            let rx = cos * dx + sin * dz;
            let rz = -sin * dx + cos * dz;
            [ct[0] + scale * rx, ct[1] + scale * dy, ct[2] + scale * rz]
        };

        let (source_pos, target_pos, target_bones) =
            (&self.source_pos, &self.target_pos, &self.target_bones);
        let mut filled = 0;
        for m in self.map.iter_mut() {
            if m.target_index.is_some() {
                continue; // keep name / manual maps
            }
            let sp = align(source_pos[m.source_index]);
            let mut best = None;
            let mut best_d = f32::MAX;
            for (ti, tp) in target_pos.iter().enumerate() {
                // Only PRIMARY deform bones are valid spatial targets — never the attachment points
                // (`bone_attach_*`) or twist helpers, or gear/cosmetic source bones scatter onto them.
                if is_aux_target(&target_bones[ti]) {
                    continue;
                }
                let d = dist2(sp, *tp);
                if d < best_d {
                    best_d = d;
                    best = Some(ti);
                }
            }
            if let Some(ti) = best {
                m.target_index = Some(ti);
                m.target_name = target_bones.get(ti).cloned();
                m.confidence = Confidence::Fuzzy; // spatial match = soft, worth a glance
                filled += 1;
            }
        }
        filled
    }

    pub fn up_axis_label(&self) -> &'static str {
        match self.up_axis {
            UpAxis::Y => "Y (native)",
            UpAxis::Z => "Z → Y",
        }
    }

    /// The source-joint-index → target-HIER-bone-index table `apply`/the rebind consumes. Directly
    /// mapped joints use their target; an UNMAPPED joint (e.g. a finger — there are no finger roles —
    /// a toe, or a gear bone) folds onto the target of the NEAREST MAPPED source bone by position, so
    /// it follows its neighbour (fingers→hand, toes→foot) instead of clumping the pelvis. With no
    /// positions it falls back to the target's pelvis (or bone 0). This is render-only: the bone map
    /// still shows those bones unmapped, so `align_by_position` can still refine them.
    pub fn joint_table(&self, target_bone_count: usize) -> Vec<usize> {
        let clamp = |i: usize| i.min(target_bone_count.saturating_sub(1));
        let pelvis = self
            .map
            .iter()
            .find(|m| matches!(m.role, Some(Role::Pelvis)))
            .and_then(|m| m.target_index)
            .map(clamp)
            .unwrap_or(0);
        // Direct maps first.
        let mut direct: Vec<Option<usize>> = vec![None; self.source_bones.len()];
        for m in &self.map {
            if let Some(ti) = m.target_index {
                if m.source_index < direct.len() {
                    direct[m.source_index] = Some(clamp(ti));
                }
            }
        }
        let has_pos = self.source_pos.len() == self.source_bones.len() && !self.source_pos.is_empty();
        let mapped: Vec<usize> = (0..direct.len()).filter(|&i| direct[i].is_some()).collect();
        (0..self.source_bones.len())
            .map(|i| match direct[i] {
                Some(t) => t,
                None if has_pos && !mapped.is_empty() => {
                    let np = *mapped
                        .iter()
                        .min_by(|&&a, &&b| {
                            dist2(self.source_pos[i], self.source_pos[a])
                                .total_cmp(&dist2(self.source_pos[i], self.source_pos[b]))
                        })
                        .unwrap();
                    direct[np].unwrap()
                }
                None => pelvis,
            })
            .collect()
    }

    /// Bind-pose Y-extent of a bone-position cloud (its height) — the numerator/denominator of the
    /// source→target unit scale.
    fn y_extent(pos: &[[f32; 3]]) -> f32 {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for p in pos {
            lo = lo.min(p[1]);
            hi = hi.max(p[1]);
        }
        (hi - lo).max(1e-3)
    }

    /// The source→target unit scale (bind-pose height ratio). Absorbs inches→metres for a CoD import
    /// and any size mismatch, so a rebound vertex offset lands at the target's scale. Falls back to 1.
    pub fn unit_scale(&self) -> f32 {
        if self.source_pos.is_empty() || self.target_pos.is_empty() {
            return 1.0;
        }
        Self::y_extent(&self.target_pos) / Self::y_extent(&self.source_pos)
    }

    /// The per-source-bone rebind matrices that re-anchor a foreign mesh onto the target skeleton.
    ///
    /// A **hybrid**, because the two bone classes want different treatment (established by rendering
    /// the shipped clips onto a retargeted Roze mesh and inspecting where each part lands):
    ///
    /// - **Deform bones** (spine, arms, legs, hands, fingers) use the classic bind-space change
    ///   `rebind[s] = TargetBind[t] · S(uscale) · SourceInvBind[s]`. The CoD/IW and Mercs2 HIER bone
    ///   frames line up well for these, so this conforms the mesh onto the target bone in position,
    ///   orientation AND proportion — the limbs and hands come out clean.
    ///
    /// - **Gear/cosmetic bones and anything mapped onto an ATTACHMENT point** (`bone_attach_*`) use
    ///   POSITION + SCALE only, keeping the source orientation: `rebind[s] = T(p_t) · S · T(−p_s)`.
    ///   An attach point's frame is unrelated to the gear bone's frame, so frame composition rotates
    ///   the (often long, off-centre) sling/pouch geometry by a garbage delta and flings it across the
    ///   hips — the exploded gear. Snapping by position parks it at the mount without that rotation.
    ///
    /// `S(uscale)` absorbs the source→target size difference (inches→metres for a CoD import). Returns
    /// one matrix per source bone (index-aligned to `source_bones` / the per-vertex joint indices), or
    /// an empty vec when neither source IBMs nor positions are available (the caller skips the rebind).
    pub fn rebind_matrices(&self, table: &[usize], tgt_bind: &[glam::Mat4]) -> Vec<glam::Mat4> {
        use glam::{Mat4, Vec3};
        let n = self.source_bones.len();
        let has_ibm = self.source_ibm.len() == n;
        let has_pos = self.source_pos.len() == n && !self.source_pos.is_empty();
        if tgt_bind.is_empty() || (!has_ibm && !has_pos) {
            return Vec::new();
        }
        let scale_m = Mat4::from_scale(Vec3::splat(self.unit_scale()));
        let is_attach = |t: usize| {
            self.target_bones.get(t).map(|nm| nm.to_ascii_lowercase().contains("attach")).unwrap_or(false)
        };
        // Position + scale, no rotation — for gear/attach bones.
        let snap = |s: usize, tb: &Mat4| -> Mat4 {
            let p_s = Vec3::from(self.source_pos.get(s).copied().unwrap_or([0.0; 3]));
            Mat4::from_translation(tb.w_axis.truncate()) * scale_m * Mat4::from_translation(-p_s)
        };
        (0..n)
            .map(|s| {
                let t = table.get(s).copied().unwrap_or(0).min(tgt_bind.len() - 1);
                let tb = tgt_bind[t];
                let gear = is_attach(t) || is_aux_source(&self.source_bones[s]);
                if gear && has_pos {
                    snap(s, &tb)
                } else if has_ibm {
                    tb * scale_m * Mat4::from_cols_array_2d(&self.source_ibm[s])
                } else {
                    snap(s, &tb)
                }
            })
            .collect()
    }

    /// Build the imported character's OWN skeleton as a `BoneRig` array, **relabeled** with the
    /// target's bone-name hashes so the target's animation clips bind to it — the non-destructive
    /// retarget. The mesh and its skin weights are left untouched; only the bone *identities* change,
    /// so the target's clips (which are keyed by bone-name hash) drive the imported bones while the
    /// character keeps its own bind pose, proportions and off-body gear. A source bone whose target is
    /// out of range keeps hash 0 (no clip binds it → it stays at bind).
    ///
    /// Built from the glTF inverse-bind matrices (`source_ibm`, column-major / column-vector) and the
    /// `source_parents` chain. `target_hashes[t]` is the target HIER bone-name hash for target index
    /// `t`. Returns an empty vec if the import carried no skeleton.
    pub fn animation_rig(&self, table: &[usize], target_hashes: &[u32]) -> Vec<mercs2_engine::mesh::BoneRig> {
        use glam::Mat4;
        use mercs2_engine::mesh::BoneRig;
        let n = self.source_bones.len();
        if self.source_ibm.len() != n || n == 0 {
            return Vec::new();
        }
        // Column-vector world/inv-bind per source joint. glTF IBM is already column-major, so
        // `from_cols_array_2d` reads it directly; its inverse is the bind world matrix.
        let inv_g: Vec<Mat4> = self.source_ibm.iter().map(Mat4::from_cols_array_2d).collect();
        let world_g: Vec<Mat4> = inv_g.iter().map(|m| m.inverse()).collect();
        (0..n)
            .map(|j| {
                let parent = self.source_parents.get(j).copied().unwrap_or(-1);
                // LOCAL (column-vector) = world_parent^-1 · world_bone; root local = world.
                let local_g = if parent >= 0 && (parent as usize) < n {
                    world_g[parent as usize].inverse() * world_g[j]
                } else {
                    world_g[j]
                };
                // The engine stores matrices as the row-vector form of the column-vector glam matrix,
                // which is exactly its `to_cols_array_2d()` (see the mesh loader's round-trip).
                let name_hash = table
                    .get(j)
                    .and_then(|&t| target_hashes.get(t))
                    .copied()
                    .unwrap_or(0);
                BoneRig {
                    parent,
                    name_hash,
                    world_bind: world_g[j].to_cols_array_2d(),
                    inv_bind: self.source_ibm[j],
                    local_bind: local_g.to_cols_array_2d(),
                }
            })
            .collect()
    }
}

/// A TARGET (Mercs2 HIER) bone that is NOT a deform bone: attachment points (`bone_attach_*`) and
/// twist/roll helpers. These must not be retarget slots — they sit at the body centre or twist axes
/// and would collapse arm/spine verts inward.
fn is_aux_target(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("attach") || n.contains("roll") || n.contains("twist") || n.contains("_ik")
}

/// A SOURCE bone that is a helper rather than a primary deform bone: proc/twist/dummy/cosmetic/
/// sling/roll/dq/lift/swivel/holster/teres. These fold onto their parent joint instead of consuming
/// a distinct target slot in the chain.
fn is_aux_source(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "proc", "twist", "dummy", "cosmetic", "sling", "roll", "dq", "lift", "swivel", "holster",
        "teres", "wristup",
    ]
    .iter()
    .any(|k| n.contains(k))
}

/// The proximal reference point for ordering a role's bones: the pelvis bone's position, else the
/// centroid of all positions, else the origin (no positions).
fn role_root(roles: &[Option<Role>], pos: &[[f32; 3]], has_pos: bool) -> [f32; 3] {
    if !has_pos {
        return [0.0; 3];
    }
    if let Some(i) = roles.iter().position(|r| matches!(r, Some(Role::Pelvis))) {
        if i < pos.len() {
            return pos[i];
        }
    }
    centroid(pos.iter().copied())
}

/// Mean of a point cloud.
fn centroid(pts: impl Iterator<Item = [f32; 3]>) -> [f32; 3] {
    let (mut c, mut n) = ([0.0f32; 3], 0.0f32);
    for p in pts {
        c[0] += p[0];
        c[1] += p[1];
        c[2] += p[2];
        n += 1.0;
    }
    if n > 0.0 {
        [c[0] / n, c[1] / n, c[2] / n]
    } else {
        [0.0; 3]
    }
}

/// RMS distance of a point cloud from `c` — the cloud's scale.
fn spread(pts: impl Iterator<Item = [f32; 3]>, c: [f32; 3]) -> f32 {
    let (mut s, mut n) = (0.0f32, 0.0f32);
    for p in pts {
        s += dist2(p, c);
        n += 1.0;
    }
    if n > 0.0 {
        (s / n).sqrt()
    } else {
        0.0
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz
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
    if n.contains("spine")
        || n.contains("chest")
        || n.contains("thorax")
        || n.contains("torso")
        || n.contains("abdomen")
        || n.contains("waist")
    {
        // The whole torso column is one role: spine / chest / thorax / TORSO / abdomen / waist all
        // mean the same region (RiggedFigure names it `torso_joint_*`, Blender rigs `chest`/`spine`).
        // spine↔chest is many-to-one; the generic terms (spine/torso) flag as fuzzy so the user can
        // confirm which rung, while the specific ones (chest/thorax) are treated as exact.
        return Some((Role::Spine, n.contains("spine") || n.contains("torso")));
    }
    None
}

/// EXPLICIT Call of Duty `j_*` → Mercs2 HIER bone-name map, authored by inspecting both skeletons
/// (Roze's `rook_body.qc_skeleton` and `pmc_hum_jen_v2`). Covers the deform chain AND fingers 1:1 —
/// the heuristic role classifier can't do fingers (no finger roles) and mis-picks near-duplicate
/// targets. Helper bones (proc/twist/dq/teres/sling/cosmetic/metacarpals) are intentionally absent:
/// they fold onto their nearest mapped neighbour via `joint_table`. Returns the target bone NAME;
/// the caller resolves it to an index (case-insensitively) against the actual target skeleton.
fn cod_target_name(src: &str) -> Option<String> {
    match src {
        "tag_origin" => return Some("bone_root".into()),
        "j_mainroot" => return Some("Bone_Hips".into()),
        "j_spinelower" => return Some("bone_spine1".into()),
        "j_spineupper" => return Some("Bone_Spine2".into()),
        "j_spine4" => return Some("Bone_Chest".into()),
        "j_neck" => return Some("bone_neck".into()),
        "j_head" => return Some("Bone_Head".into()),
        _ => {}
    }
    // Sided bones: strip the `_le`/`_ri` marker (it sits mid-name for fingers, e.g. j_thumb_le_1).
    let (marker, lc, uc) = if src.contains("_le") {
        ("_le", "l", "L")
    } else if src.contains("_ri") {
        ("_ri", "r", "R")
    } else {
        return None;
    };
    let base = src.replacen(marker, "", 1);
    let t = match base.as_str() {
        "j_clavicle" => format!("bone_{lc}shoulder"),
        "j_shoulder" => format!("Bone_{uc}Bicep"),
        "j_elbow" => format!("Bone_{uc}Forearm"),
        "j_wrist" => format!("bone_{lc}hand"),
        "j_hip" => format!("Bone_{uc}Thigh"),
        "j_knee" => format!("Bone_{uc}Shin"),
        "j_ankle" => format!("Bone_{uc}FootBone1"),
        "j_ball" => format!("Bone_{uc}FootBone2"),
        "j_thumb_1" => format!("bone_{lc}thumb1"),
        "j_thumb_2" => format!("bone_{lc}thumb2"),
        "j_thumb_3" => format!("bone_{lc}thumb3"),
        "j_index_1" => format!("bone_{lc}index1"),
        "j_index_2" => format!("bone_{lc}index2"),
        "j_index_3" => format!("bone_{lc}index3"),
        "j_mid_1" => format!("bone_{lc}middle1"),
        "j_mid_2" => format!("bone_{lc}middle2"),
        "j_mid_3" => format!("bone_{lc}middle3"),
        "j_ring_1" => format!("bone_{lc}ring1"),
        "j_ring_2" => format!("bone_{lc}ring2"),
        "j_ring_3" => format!("bone_{lc}ring3"),
        "j_pinky_1" => format!("bone_{lc}pinky1"),
        "j_pinky_2" => format!("bone_{lc}pinky2"),
        "j_pinky_3" => format!("bone_{lc}pinky3"),
        _ => return None,
    };
    Some(t)
}

/// Explicit source→Mercs2 HIER bone-name map for a recognised rig convention. Authored by inspecting
/// each skeleton; covers the deform chain AND fingers 1:1 (the role heuristic can't do fingers). CoD is
/// verified against a real asset (Roze); ValveBiped/Mixamo/Unreal use their standardised bone names.
/// Returns the target NAME; the caller resolves it against the actual target skeleton (case-insensitive).
fn explicit_target_name(conv: SourceRig, src: &str) -> Option<String> {
    match conv {
        SourceRig::CallOfDuty => cod_target_name(src),
        SourceRig::ValveBiped => valvebiped_target_name(src),
        SourceRig::Mixamo => mixamo_target_name(src),
        SourceRig::Unreal => unreal_target_name(src),
        SourceRig::Generic => None,
    }
}

/// ValveBiped (Source engine): `ValveBiped.Bip01_L_UpperArm`, `ValveBiped.Bip01_Spine1`,
/// `ValveBiped.Bip01_L_Finger0` (thumb) … `Finger4` (pinky), each with `01`/`02` segments.
fn valvebiped_target_name(src: &str) -> Option<String> {
    let tail = src.rsplit(|c| c == '.' || c == ':').next().unwrap_or(src);
    let n = tail.strip_prefix("Bip01_").unwrap_or(tail);
    match n {
        "Pelvis" => return Some("Bone_Hips".into()),
        "Spine" => return Some("bone_spine1".into()),
        "Spine1" => return Some("Bone_Spine2".into()),
        "Spine2" | "Spine4" => return Some("Bone_Chest".into()),
        "Neck" | "Neck1" => return Some("bone_neck".into()),
        "Head" | "Head1" => return Some("Bone_Head".into()),
        _ => {}
    }
    let (lc, uc, rest) = if let Some(r) = n.strip_prefix("L_") {
        ("l", "L", r)
    } else if let Some(r) = n.strip_prefix("R_") {
        ("r", "R", r)
    } else {
        return None;
    };
    let t = match rest {
        "Clavicle" => format!("bone_{lc}shoulder"),
        "UpperArm" => format!("Bone_{uc}Bicep"),
        "Forearm" => format!("Bone_{uc}Forearm"),
        "Hand" => format!("bone_{lc}hand"),
        "Thigh" => format!("Bone_{uc}Thigh"),
        "Calf" => format!("Bone_{uc}Shin"),
        "Foot" => format!("Bone_{uc}FootBone1"),
        "Toe0" => format!("Bone_{uc}FootBone2"),
        "Finger0" => format!("bone_{lc}thumb1"),
        "Finger01" => format!("bone_{lc}thumb2"),
        "Finger02" => format!("bone_{lc}thumb3"),
        "Finger1" => format!("bone_{lc}index1"),
        "Finger11" => format!("bone_{lc}index2"),
        "Finger12" => format!("bone_{lc}index3"),
        "Finger2" => format!("bone_{lc}middle1"),
        "Finger21" => format!("bone_{lc}middle2"),
        "Finger22" => format!("bone_{lc}middle3"),
        "Finger3" => format!("bone_{lc}ring1"),
        "Finger31" => format!("bone_{lc}ring2"),
        "Finger32" => format!("bone_{lc}ring3"),
        "Finger4" => format!("bone_{lc}pinky1"),
        "Finger41" => format!("bone_{lc}pinky2"),
        "Finger42" => format!("bone_{lc}pinky3"),
        _ => return None,
    };
    Some(t)
}

/// Mixamo auto-rig: `mixamorig:LeftForeArm`, `mixamorig:LeftHandIndex1`, `mixamorig:Spine1` …
fn mixamo_target_name(src: &str) -> Option<String> {
    let n = src.trim_start_matches("mixamorig").trim_start_matches(':');
    match n {
        "Hips" => return Some("Bone_Hips".into()),
        "Spine" => return Some("bone_spine1".into()),
        "Spine1" => return Some("Bone_Spine2".into()),
        "Spine2" => return Some("Bone_Chest".into()),
        "Neck" => return Some("bone_neck".into()),
        "Head" => return Some("Bone_Head".into()),
        _ => {}
    }
    let (lc, uc, rest) = if let Some(r) = n.strip_prefix("Left") {
        ("l", "L", r)
    } else if let Some(r) = n.strip_prefix("Right") {
        ("r", "R", r)
    } else {
        return None;
    };
    let t = match rest {
        "Shoulder" => format!("bone_{lc}shoulder"),
        "Arm" => format!("Bone_{uc}Bicep"),
        "ForeArm" => format!("Bone_{uc}Forearm"),
        "Hand" => format!("bone_{lc}hand"),
        "UpLeg" => format!("Bone_{uc}Thigh"),
        "Leg" => format!("Bone_{uc}Shin"),
        "Foot" => format!("Bone_{uc}FootBone1"),
        "ToeBase" => format!("Bone_{uc}FootBone2"),
        "HandThumb1" => format!("bone_{lc}thumb1"),
        "HandThumb2" => format!("bone_{lc}thumb2"),
        "HandThumb3" => format!("bone_{lc}thumb3"),
        "HandIndex1" => format!("bone_{lc}index1"),
        "HandIndex2" => format!("bone_{lc}index2"),
        "HandIndex3" => format!("bone_{lc}index3"),
        "HandMiddle1" => format!("bone_{lc}middle1"),
        "HandMiddle2" => format!("bone_{lc}middle2"),
        "HandMiddle3" => format!("bone_{lc}middle3"),
        "HandRing1" => format!("bone_{lc}ring1"),
        "HandRing2" => format!("bone_{lc}ring2"),
        "HandRing3" => format!("bone_{lc}ring3"),
        "HandPinky1" => format!("bone_{lc}pinky1"),
        "HandPinky2" => format!("bone_{lc}pinky2"),
        "HandPinky3" => format!("bone_{lc}pinky3"),
        _ => return None,
    };
    Some(t)
}

/// Unreal Engine mannequin: `upperarm_l`, `lowerarm_l`, `hand_l`, `index_01_l`, `spine_02`, `thigh_l` …
fn unreal_target_name(src: &str) -> Option<String> {
    let n = src.to_ascii_lowercase();
    match n.as_str() {
        "pelvis" => return Some("Bone_Hips".into()),
        "spine_01" => return Some("bone_spine1".into()),
        "spine_02" => return Some("Bone_Spine2".into()),
        "spine_03" => return Some("Bone_Chest".into()),
        "neck_01" | "neck" => return Some("bone_neck".into()),
        "head" => return Some("Bone_Head".into()),
        _ => {}
    }
    let (lc, uc, base) = if let Some(b) = n.strip_suffix("_l") {
        ("l", "L", b)
    } else if let Some(b) = n.strip_suffix("_r") {
        ("r", "R", b)
    } else {
        return None;
    };
    let t = match base {
        "clavicle" => format!("bone_{lc}shoulder"),
        "upperarm" => format!("Bone_{uc}Bicep"),
        "lowerarm" => format!("Bone_{uc}Forearm"),
        "hand" => format!("bone_{lc}hand"),
        "thigh" => format!("Bone_{uc}Thigh"),
        "calf" => format!("Bone_{uc}Shin"),
        "foot" => format!("Bone_{uc}FootBone1"),
        "ball" => format!("Bone_{uc}FootBone2"),
        "thumb_01" => format!("bone_{lc}thumb1"),
        "thumb_02" => format!("bone_{lc}thumb2"),
        "thumb_03" => format!("bone_{lc}thumb3"),
        "index_01" => format!("bone_{lc}index1"),
        "index_02" => format!("bone_{lc}index2"),
        "index_03" => format!("bone_{lc}index3"),
        "middle_01" => format!("bone_{lc}middle1"),
        "middle_02" => format!("bone_{lc}middle2"),
        "middle_03" => format!("bone_{lc}middle3"),
        "ring_01" => format!("bone_{lc}ring1"),
        "ring_02" => format!("bone_{lc}ring2"),
        "ring_03" => format!("bone_{lc}ring3"),
        "pinky_01" => format!("bone_{lc}pinky1"),
        "pinky_02" => format!("bone_{lc}pinky2"),
        "pinky_03" => format!("bone_{lc}pinky3"),
        _ => return None,
    };
    Some(t)
}

/// Classify a Call of Duty / IW-engine joint name into an anatomical role. CoD's joint words do
/// NOT line up with the generic anatomy keywords, so they get an explicit table:
///   `j_mainroot`/`tag_origin` → pelvis, `j_shoulder` → UPPER ARM (not clavicle), `j_elbow` →
///   FOREARM, `j_wrist` → hand, `j_hip` → THIGH, `j_knee` → SHIN, `j_ankle` → foot.
/// `j_clavicle` stays the clavicle. Auxiliary bones (proc/twist/dq/cosmetic/sling/ball) fall through
/// to their nearest major role by keyword, or to `None` (→ pelvis fallback in `joint_table`). Side
/// comes from the `_le`/`_ri` markers via [`side_of`] (`_l`/`_r`). Returns `(role, fuzzy)`.
fn classify_cod(name: &str) -> Option<(Role, bool)> {
    let n = name.to_ascii_lowercase();
    let side = side_of(&n);

    if n.contains("tag_origin") || n.contains("mainroot") {
        return Some((Role::Pelvis, false));
    }
    if n.contains("head") {
        return Some((Role::Head, false));
    }
    if n.contains("neck") {
        return Some((Role::Neck, false));
    }
    if n.contains("clavicle") {
        return side.map(|s| (Role::Clav(s), false));
    }
    // CoD `j_shoulder` is the deltoid/UPPER ARM, distinct from `j_clavicle` above.
    if n.contains("shoulder") {
        return side.map(|s| (Role::UpperArm(s), false));
    }
    if n.contains("elbow") {
        return side.map(|s| (Role::Forearm(s), false));
    }
    if n.contains("wrist") {
        return side.map(|s| (Role::Hand(s), false));
    }
    if n.contains("hip") {
        return side.map(|s| (Role::Thigh(s), false));
    }
    if n.contains("knee") {
        return side.map(|s| (Role::Shin(s), false));
    }
    if n.contains("ankle") {
        return side.map(|s| (Role::Foot(s), false));
    }
    if n.contains("spine") {
        return Some((Role::Spine, false));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// A GENERIC rig retargeted onto a HERO target must resolve char_skin::automap's canonical
    /// NPC-84 indices onto the hero's ACTUAL bones BY NAME — not by raw index. Here the hero target
    /// prepends two root bones, so `Bone_Hips` sits at index 5, not 3. Regression for the
    /// mattias_v2 scramble (torso→foot, arm→pinky) caused by index-space mismatch.
    #[test]
    fn remap_resolves_onto_hero_target_by_name() {
        let source_bones = names(&[
            "torso_joint_1", "torso_joint_2", "torso_joint_3", "leg_joint_L_1", "leg_joint_R_1",
            "arm_joint_L_1",
        ]);
        // torso_1 is root and PARENTS the legs (→ HIPS, not spine1); torso chain 1→2→3; arm under torso_3.
        let source_parents = vec![-1i32, 0, 1, 0, 0, 2];
        // HERO-like target: NPC-84 names shifted by two extra root bones → Bone_Hips at index 5.
        let mut target_bones = names(&["extra_root_a", "extra_root_b"]);
        target_bones.extend(mercs2_formats::char_skin::NPC84_NAMES.iter().map(|s| s.to_string()));
        let map = source_bones
            .iter()
            .enumerate()
            .map(|(i, s)| BoneMap {
                source: s.clone(),
                source_index: i,
                role: None,
                target_name: None,
                target_index: None,
                confidence: Confidence::Auto,
            })
            .collect();
        let mut rt = Retarget {
            convention: SourceRig::Generic,
            source_bones,
            target_bones: target_bones.clone(),
            map,
            up_axis: UpAxis::Y,
            scale: 1.0,
            source_pos: Vec::new(),
            target_pos: Vec::new(),
            source_ibm: Vec::new(),
            source_parents,
            target_parents: Vec::new(),
        };
        rt.remap_via_char_skin(&std::collections::HashSet::new());
        let ti = |src: usize| rt.map.iter().find(|m| m.source_index == src).unwrap().target_index;
        let idx = |name: &str| target_bones.iter().position(|t| t.eq_ignore_ascii_case(name)).unwrap();
        assert_eq!(ti(0), Some(idx("Bone_Hips")), "torso root -> hips (shifted)");
        assert_eq!(ti(0), Some(5), "hips is at the SHIFTED hero index, not 3");
        assert_eq!(ti(1), Some(idx("bone_spine1")));
        assert_eq!(ti(2), Some(idx("Bone_Spine2")));
        assert_eq!(ti(3), Some(idx("Bone_LThigh")));
        assert_eq!(ti(4), Some(idx("Bone_RThigh")));
        assert_eq!(ti(5), Some(idx("Bone_LBicep")), "arm -> upper arm, not a finger");
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
    fn detect_call_of_duty() {
        let src = names(&["tag_origin", "j_mainroot", "j_spinelower", "j_shoulder_le"]);
        assert!(SourceRig::detect(&src) == SourceRig::CallOfDuty);
    }

    #[test]
    fn call_of_duty_maps_onto_mercs2() {
        // The Roze rig's core deform bones (CoD `j_*`) onto the Mercs2 human HIER names.
        let src = names(&[
            "tag_origin",
            "j_mainroot",
            "j_spinelower",
            "j_neck",
            "j_head",
            "j_clavicle_le",
            "j_shoulder_le", // upper arm in CoD
            "j_elbow_le",    // forearm
            "j_wrist_le",
            "j_hip_le", // thigh
            "j_knee_le", // shin
            "j_ankle_le",
        ]);
        let tgt = names(&[
            "Bone_Hips", "bone_spine1", "bone_neck", "Bone_Head", "bone_lshoulder", "Bone_LBicep",
            "Bone_LForearm", "bone_lhand", "Bone_LThigh", "Bone_LShin", "bone_lfoot",
        ]);
        let r = Retarget::build(src, &tgt);
        let tgt_of = |src_sub: &str| -> Option<String> {
            r.map.iter().find(|m| m.source.contains(src_sub)).and_then(|m| m.target_name.clone())
        };
        // The semantic traps must land right: shoulder→bicep (upper arm), elbow→forearm,
        // hip→thigh, knee→shin — NOT the pelvis fallback.
        assert_eq!(tgt_of("j_shoulder_le").as_deref(), Some("Bone_LBicep"));
        assert_eq!(tgt_of("j_elbow_le").as_deref(), Some("Bone_LForearm"));
        assert_eq!(tgt_of("j_hip_le").as_deref(), Some("Bone_LThigh"));
        assert_eq!(tgt_of("j_knee_le").as_deref(), Some("Bone_LShin"));
        assert_eq!(tgt_of("j_clavicle_le").as_deref(), Some("bone_lshoulder"));
        assert_eq!(tgt_of("j_wrist_le").as_deref(), Some("bone_lhand"));
        assert_eq!(tgt_of("j_head").as_deref(), Some("Bone_Head"));
        assert_eq!(tgt_of("j_mainroot").as_deref(), Some("Bone_Hips"));
    }

    #[test]
    fn cod_explicit_table_maps_fingers_and_core() {
        let src = names(&[
            "j_mainroot", "j_spine4", "j_wrist_le", "j_thumb_le_1", "j_index_ri_3", "j_ankle_le",
            "j_ball_le",
        ]);
        let tgt = names(&[
            "Bone_Hips", "Bone_Chest", "bone_lhand", "bone_lthumb1", "bone_rindex3",
            "Bone_LFootBone1", "Bone_LFootBone2",
        ]);
        let r = Retarget::build(src, &tgt);
        let t = |sub: &str| r.map.iter().find(|m| m.source == sub).and_then(|m| m.target_name.clone());
        assert_eq!(t("j_mainroot").as_deref(), Some("Bone_Hips"));
        assert_eq!(t("j_spine4").as_deref(), Some("Bone_Chest"));
        assert_eq!(t("j_wrist_le").as_deref(), Some("bone_lhand"));
        assert_eq!(t("j_thumb_le_1").as_deref(), Some("bone_lthumb1"));
        assert_eq!(t("j_index_ri_3").as_deref(), Some("bone_rindex3")); // right-hand finger
        assert_eq!(t("j_ankle_le").as_deref(), Some("Bone_LFootBone1"));
        assert_eq!(t("j_ball_le").as_deref(), Some("Bone_LFootBone2"));
    }

    #[test]
    fn popular_format_explicit_names() {
        // ValveBiped (Source)
        assert_eq!(valvebiped_target_name("ValveBiped.Bip01_Pelvis").as_deref(), Some("Bone_Hips"));
        assert_eq!(valvebiped_target_name("ValveBiped.Bip01_L_UpperArm").as_deref(), Some("Bone_LBicep"));
        assert_eq!(valvebiped_target_name("ValveBiped.Bip01_L_Finger0").as_deref(), Some("bone_lthumb1"));
        assert_eq!(valvebiped_target_name("ValveBiped.Bip01_R_Finger12").as_deref(), Some("bone_rindex3"));
        assert_eq!(valvebiped_target_name("ValveBiped.Bip01_L_Toe0").as_deref(), Some("Bone_LFootBone2"));
        // Mixamo
        assert_eq!(mixamo_target_name("mixamorig:Hips").as_deref(), Some("Bone_Hips"));
        assert_eq!(mixamo_target_name("mixamorig:LeftForeArm").as_deref(), Some("Bone_LForearm"));
        assert_eq!(mixamo_target_name("mixamorig:LeftHandPinky2").as_deref(), Some("bone_lpinky2"));
        assert_eq!(mixamo_target_name("mixamorig:RightUpLeg").as_deref(), Some("Bone_RThigh"));
        // Unreal mannequin
        assert_eq!(unreal_target_name("pelvis").as_deref(), Some("Bone_Hips"));
        assert_eq!(unreal_target_name("lowerarm_l").as_deref(), Some("Bone_LForearm"));
        assert_eq!(unreal_target_name("middle_02_l").as_deref(), Some("bone_lmiddle2"));
        assert_eq!(unreal_target_name("ball_r").as_deref(), Some("Bone_RFootBone2"));
    }

    #[test]
    fn spine_chain_spreads_across_distinct_targets() {
        // The 3 spine bones must map to 3 DISTINCT target spine bones (proximal→distal), not collapse
        // onto the first. A pelvis anchors the proximal end for ordering (as in a real skeleton).
        let src = names(&["hips", "j_spinelower", "j_spineupper", "j_spine4"]);
        let spos = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 3.0, 0.0]];
        let tgt = names(&["Bone_Hips", "bone_spine1", "Bone_Spine2", "Bone_Chest"]);
        let tpos = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 3.0, 0.0]];
        let r = Retarget::build_with_pos(src, spos, tgt, tpos);
        assert_eq!(r.map[1].target_index, Some(1)); // spinelower → spine1
        assert_eq!(r.map[2].target_index, Some(2)); // spineupper → Spine2
        assert_eq!(r.map[3].target_index, Some(3)); // spine4    → Chest
    }

    #[test]
    fn manual_override_sets_and_clears() {
        let mut r = Retarget::build(names(&["a", "b"]), &names(&["Bone_Hips", "Bone_Head"]));
        r.set_manual(0, Some(1));
        let m = &r.map[0];
        assert_eq!(m.target_index, Some(1));
        assert_eq!(m.target_name.as_deref(), Some("Bone_Head"));
        assert!(matches!(m.confidence, Confidence::Manual));
        r.set_manual(0, None);
        assert_eq!(r.map[0].target_index, None);
        assert!(matches!(r.map[0].confidence, Confidence::Unmapped));
    }

    #[test]
    fn align_by_position_fills_unmapped() {
        // pelvis/head/hand_l map by name (anchors); "extra" has no role and must be filled by the
        // nearest target bone in the aligned frame.
        let src = names(&["pelvis", "head", "hand_l", "extra"]);
        let spos = vec![[0.0, 0.0, 0.0], [0.0, 10.0, 0.0], [5.0, 5.0, 0.0], [0.0, 2.5, 0.0]];
        let tgt = names(&["Bone_Hips", "Bone_Head", "bone_lhand", "Bone_Spine1"]);
        let tpos = vec![[0.0, 0.0, 0.0], [0.0, 10.0, 0.0], [5.0, 5.0, 0.0], [0.0, 3.0, 0.0]];
        let mut r = Retarget::build_with_pos(src, spos, tgt, tpos);
        assert_eq!(r.map[3].target_index, None); // unmapped by name
        assert!(matches!(r.map[0].confidence, Confidence::Auto)); // pelvis anchor
        assert_eq!(r.align_by_position(), 1);
        assert_eq!(r.map[3].target_name.as_deref(), Some("Bone_Spine1")); // nearest to [0,2.5,0]
        assert!(matches!(r.map[3].confidence, Confidence::Fuzzy));
        assert!(matches!(r.map[0].confidence, Confidence::Auto)); // anchor untouched
    }

    #[test]
    fn rebind_deform_bone_conforms_via_frame_composition() {
        // A deform bone (well-aligned source/target frames) uses `TargetBind · S · SourceInvBind`. A
        // vertex 1 unit above the source arm bone must land 1 (scaled) unit above the target arm bone,
        // relocated to the target bone's position — the bind-space change working end to end.
        let src = names(&["hips", "arm"]);
        let spos = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]; // y-extent 1
        let sparents = vec![-1i32, 0];
        // Source arm bind = translate(0,1,0), identity rotation → inverse-bind = translate(0,-1,0).
        let ident = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let arm_sib = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, -1.0, 0.0, 1.0]];
        let sibm = vec![ident, arm_sib];
        let tgt = names(&["Bone_Hips", "Bone_LBicep"]);
        let tpos = vec![[0.0, 0.0, 0.0], [0.5, 1.0, 0.0]]; // y-extent 1 → uscale 1
        let tparents = vec![-1i32, 0];
        let r = Retarget::build_full(src, spos, sibm, sparents, tgt, tpos, tparents);
        let table = vec![0usize, 1];
        let tgt_bind = vec![
            glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 0.0)),
            glam::Mat4::from_translation(glam::Vec3::new(0.5, 1.0, 0.0)),
        ];
        let rebind = r.rebind_matrices(&table, &tgt_bind);
        // Vertex 1 unit above the source arm bone (0,1,0) → (0,2,0), weighted to the arm.
        let landed = rebind[1].transform_point3(glam::Vec3::new(0.0, 2.0, 0.0));
        // Target arm bone at (0.5,1,0); 1 unit above → (0.5,2,0).
        let expected = glam::Vec3::new(0.5, 2.0, 0.0);
        assert!(
            (landed - expected).length() < 1e-4,
            "deform vertex landed at {landed:?}, expected {expected:?}"
        );
    }

    #[test]
    fn rebind_gear_bone_snaps_to_attach_without_frame_flip() {
        // A gear bone with a WILD source frame (90° rotation) mapped onto an attachment point must be
        // placed by position + scale, NOT frame composition — otherwise the attach point's unrelated
        // frame rotates the sling geometry across the hips (the exploded-gear failure). The vertex must
        // land near the attach point regardless of the gear bone's crazy frame.
        let src = names(&["hips", "j_sling_target"]);
        let spos = vec![[0.0, 0.0, 0.0], [5.0, 1.0, 0.0]]; // gear bone far out to one side
        let sparents = vec![-1i32, 0];
        // Gear source inverse-bind carries a 90°-about-Z rotation (+ translation) — a frame utterly
        // unlike the attach point's. If the rebind used it, the vertex would be flung.
        let ident = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        // inverse-bind = R(90°,Z) then translate — column-major: rows are basis, last row translation.
        let gear_sib = [[0.0, 1.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [-5.0, -1.0, 0.0, 1.0]];
        let sibm = vec![ident, gear_sib];
        let tgt = names(&["Bone_Hips", "bone_attach_hipleft"]);
        let tpos = vec![[0.0, 0.0, 0.0], [0.2, 1.0, 0.0]]; // y-extent 1 → uscale 1
        let tparents = vec![-1i32, 0];
        let r = Retarget::build_full(src, spos, sibm, sparents, tgt, tpos, tparents);
        let table = vec![0usize, 1];
        let tgt_bind = vec![
            glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 0.0)),
            glam::Mat4::from_translation(glam::Vec3::new(0.2, 1.0, 0.0)),
        ];
        let rebind = r.rebind_matrices(&table, &tgt_bind);
        // A vertex 0.1 out from the gear bone → should land ~0.1 from the attach point (position-snap),
        // NOT metres away (which frame composition with the 90° twist would produce).
        let landed = rebind[1].transform_point3(glam::Vec3::new(5.1, 1.0, 0.0));
        let attach = glam::Vec3::new(0.2, 1.0, 0.0);
        assert!(
            (landed - attach).length() < 0.15,
            "gear vertex landed at {landed:?}, should be near attach {attach:?}"
        );
    }

    #[test]
    fn side_disambiguation() {
        assert!(side_of("bip01_l_upperarm") == Some(Side::L));
        assert!(side_of("bone_rbicep") == Some(Side::R));
        assert!(side_of("bone_hips").is_none());
    }
}
