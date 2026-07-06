//! FOREIGN-RIG RETARGET driver (reusable; not cesium-specific).
//!
//! A rigged glb carries its own skeleton: per-vertex `JOINTS_0` indices into a
//! `skin.joints` node list, plus coherent `WEIGHTS_0`. Spatial nearest-neighbour
//! weight transfer DISCARDS that rig and reconstructs weights from donor-vertex
//! proximity — which scrambles anatomy (a head vertex can snap to a torso bone).
//! This module instead HONOURS the foreign rig: it maps each foreign joint to the
//! anatomically-correct donor bone BY NAME, then every vertex keeps its native
//! weights and only has its 4 joint indices remapped through that table.
//!
//! The pipeline is:
//!   1. classify each foreign joint NAME -> a [`BodyRole`] (anatomical slot),
//!   2. resolve each role -> the donor's global bone index (via [`mannequin::BodyMap`],
//!      which is itself resolved from the donor [`crate::skeleton::Skeleton`] by
//!      name-hash + hierarchy — no hard-coded indices),
//!   3. emit a `Vec<u8>` of length `n_joints` mapping foreign joint index -> donor
//!      global bone index (the "19->95 table" for CesiumMan).
//!
//! GENERALISES to any rigged glb on any donor: supply the joint names and the
//! resolved `BodyMap`; the classifier handles the common humanoid naming idioms
//! (torso/spine/pelvis, neck/head, arm/bicep/forearm/hand, leg/thigh/shin/foot,
//! L/R sidedness). Joints it cannot classify fall back to the pelvis (a safe,
//! near-root bone) and are reported so the caller can refine the classifier.

use crate::mannequin::BodyMap;

/// Anatomical slot a foreign joint maps onto. Each resolves to one donor bone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyRole {
    Pelvis,
    Chest,
    Neck,
    Head,
    UpperArmL,
    UpperArmR,
    ForearmL,
    ForearmR,
    HandL,
    HandR,
    ThighL,
    ThighR,
    ShinL,
    ShinR,
    FootL,
    FootR,
}

impl BodyRole {
    /// Resolve this role to the donor's global bone index via the resolved map.
    pub fn bone(self, map: &BodyMap) -> usize {
        match self {
            BodyRole::Pelvis => map.pelvis,
            BodyRole::Chest => map.chest,
            BodyRole::Neck => map.neck,
            BodyRole::Head => map.head,
            BodyRole::UpperArmL => map.upperarm_l,
            BodyRole::UpperArmR => map.upperarm_r,
            BodyRole::ForearmL => map.forearm_l,
            BodyRole::ForearmR => map.forearm_r,
            BodyRole::HandL => map.hand_l,
            BodyRole::HandR => map.hand_r,
            BodyRole::ThighL => map.thigh_l,
            BodyRole::ThighR => map.thigh_r,
            BodyRole::ShinL => map.shin_l,
            BodyRole::ShinR => map.shin_r,
            BodyRole::FootL => map.foot_l,
            BodyRole::FootR => map.foot_r,
        }
    }
}

/// Sidedness parsed from a joint name (`_L_`, `__R__`, `left`, `right`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    L,
    R,
    None,
}

fn side_of(name_lc: &str) -> Side {
    // Match common sidedness idioms. Cesium uses `_L_`/`_R` suffixes embedded in
    // joint names (e.g. `arm_joint_L__4_`, `leg_joint_R_2`); also accept left/right.
    let has = |needles: &[&str]| needles.iter().any(|n| name_lc.contains(n));
    if has(&["_l_", "_l ", "left", "joint_l", "arm_l", "leg_l", ".l", "_lt"]) {
        Side::L
    } else if has(&["_r_", "_r ", "right", "joint_r", "arm_r", "leg_r", ".r", "_rt"]) {
        Side::R
    } else if name_lc.ends_with("_l") || name_lc.ends_with("_l_") {
        Side::L
    } else if name_lc.ends_with("_r") || name_lc.ends_with("_r_") {
        Side::R
    } else {
        Side::None
    }
}

/// Trailing limb-segment ordinal (the `_N_`/`_N` number in a chain joint name),
/// used to order a multi-joint limb chain from root to tip. CesiumMan numbers
/// arms `_2_/_3_/_4_` and legs `_1/_2/_3/_5`; bigger trailing number on a limb is
/// generally further from the torso. Returns None when there is no ordinal.
/// (Alternate chain-rank for rigs without usable world positions; the cesium
/// driver ranks by world distance instead, which is unambiguous.)
#[allow(dead_code)]
fn trailing_ordinal(name_lc: &str) -> Option<u32> {
    let bytes = name_lc.as_bytes();
    let mut end = bytes.len();
    // skip trailing non-digits (e.g. the closing `_`)
    while end > 0 && !bytes[end - 1].is_ascii_digit() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start == end {
        None
    } else {
        name_lc[start..end].parse().ok()
    }
}

/// One classified foreign limb joint, pre-sorted into its chain by `chain_rank`.
struct LimbJoint {
    j: usize,
}

/// Build the foreign-joint -> donor-bone index table ("19->95" for CesiumMan).
///
/// `joint_names[j]` is the glb node name of `skin.joints[j]`. The returned vector
/// has the same length; entry `j` is the donor global bone index that foreign
/// joint `j` retargets onto. `unclassified` collects any joint index whose name
/// the classifier could not place (defaulted to pelvis) so the caller can report.
///
/// Limb chains are resolved by ANATOMY, not by raw joint order: every arm joint is
/// collected per side and sorted by its trailing ordinal, then assigned
/// shoulder->elbow->hand (upperarm / forearm / hand). The same for legs
/// (thigh / shin / foot / foot for a 4th toe joint). This is what makes the
/// retarget robust to the foreign rig listing joints in a non-anatomical order
/// (CesiumMan lists L arm as j5/j7/j9 = shoulder/elbow/hand by ordinal _4_/_3_/_2_
/// — note the ordinal there DECREASES toward the hand, which the position-verified
/// sort below would get wrong; so we sort by WORLD-Y distance from the torso when
/// positions are supplied, else fall back to ordinal).
pub fn build_retarget_table(
    joint_names: &[String],
    map: &BodyMap,
    joint_chain_rank: &dyn Fn(usize) -> f32,
) -> (Vec<u8>, Vec<usize>) {
    let n = joint_names.len();
    let mut role: Vec<Option<BodyRole>> = vec![None; n];

    // collect limb joints per side for ordered assignment
    let mut arm_l: Vec<LimbJoint> = Vec::new();
    let mut arm_r: Vec<LimbJoint> = Vec::new();
    let mut leg_l: Vec<LimbJoint> = Vec::new();
    let mut leg_r: Vec<LimbJoint> = Vec::new();
    let mut neck_chain: Vec<usize> = Vec::new();
    let mut has_explicit_head = false;

    for j in 0..n {
        let lc = joint_names[j].to_lowercase();
        let side = side_of(&lc);
        let is_arm = lc.contains("arm") || lc.contains("bicep") || lc.contains("forearm")
            || lc.contains("hand") || lc.contains("clav") || lc.contains("shoulder");
        let is_leg = lc.contains("leg") || lc.contains("thigh") || lc.contains("shin")
            || lc.contains("calf") || lc.contains("foot") || lc.contains("toe")
            || lc.contains("knee") || lc.contains("ankle");
        let is_torso = lc.contains("torso") || lc.contains("spine") || lc.contains("pelvis")
            || lc.contains("hips") || lc.contains("chest") || lc.contains("root");
        let is_neck = lc.contains("neck");
        let is_head = lc.contains("head") || lc.contains("skull");

        if is_head {
            role[j] = Some(BodyRole::Head);
            has_explicit_head = true;
        } else if is_neck {
            // Resolved below: lowest neck joint -> Neck; topmost -> Head IF the rig
            // has NO explicit head joint (the head mesh then weights to the top
            // neck joint — exactly CesiumMan, whose head verts ride neck_joint_2).
            neck_chain.push(j);
        } else if is_arm {
            match side {
                Side::R => arm_r.push(LimbJoint { j }),
                _ => arm_l.push(LimbJoint { j }), // default left if ambiguous
            }
        } else if is_leg {
            match side {
                Side::R => leg_r.push(LimbJoint { j }),
                _ => leg_l.push(LimbJoint { j }),
            }
        } else if is_torso {
            // torso chain: first (lowest) -> pelvis, rest -> chest (resolved below
            // by chain rank so the lowest torso joint becomes the pelvis).
            role[j] = Some(BodyRole::Chest); // provisional; pelvis fixed below
        }
    }

    // ---- neck/head: order the neck chain root->tip. Lowest -> Neck. If the rig
    //      lacks an explicit head joint, the TOPMOST neck joint becomes the Head
    //      (its mesh region is the head). ----
    neck_chain.sort_by(|&a, &b| joint_chain_rank(a).partial_cmp(&joint_chain_rank(b)).unwrap());
    let nc = neck_chain.len();
    for (k, &jn) in neck_chain.iter().enumerate() {
        let is_top = k + 1 == nc;
        role[jn] = Some(if is_top && !has_explicit_head {
            BodyRole::Head
        } else {
            BodyRole::Neck
        });
    }

    // ---- torso: lowest-ranked torso joint becomes the pelvis ----
    let mut torso: Vec<usize> = (0..n)
        .filter(|&j| role[j] == Some(BodyRole::Chest))
        .collect();
    if !torso.is_empty() {
        torso.sort_by(|&a, &b| joint_chain_rank(a).partial_cmp(&joint_chain_rank(b)).unwrap());
        role[torso[0]] = Some(BodyRole::Pelvis);
        // remaining torso joints stay Chest
    }

    // ---- arms: sort root->tip by chain rank (distance from torso), assign
    //      shoulder/elbow/hand. A 4th+ joint collapses onto the hand. ----
    let assign_arm = |chain: &mut Vec<LimbJoint>,
                      role: &mut Vec<Option<BodyRole>>,
                      up: BodyRole,
                      fa: BodyRole,
                      hand: BodyRole| {
        chain.sort_by(|a, b| joint_chain_rank(a.j).partial_cmp(&joint_chain_rank(b.j)).unwrap());
        let m = chain.len();
        for (k, lj) in chain.iter().enumerate() {
            role[lj.j] = Some(if m == 0 {
                up
            } else if k == 0 {
                up
            } else if k == m - 1 {
                hand
            } else {
                fa
            });
        }
        let _ = m;
    };
    assign_arm(&mut arm_l, &mut role, BodyRole::UpperArmL, BodyRole::ForearmL, BodyRole::HandL);
    assign_arm(&mut arm_r, &mut role, BodyRole::UpperArmR, BodyRole::ForearmR, BodyRole::HandR);

    // ---- legs: sort root->tip, assign thigh/shin/foot; 4th joint -> foot ----
    let assign_leg = |chain: &mut Vec<LimbJoint>,
                      role: &mut Vec<Option<BodyRole>>,
                      thigh: BodyRole,
                      shin: BodyRole,
                      foot: BodyRole| {
        chain.sort_by(|a, b| joint_chain_rank(a.j).partial_cmp(&joint_chain_rank(b.j)).unwrap());
        let m = chain.len();
        for (k, lj) in chain.iter().enumerate() {
            role[lj.j] = Some(match (m, k) {
                (_, 0) => thigh,
                (3, 1) => shin,
                (3, _) => foot,
                // 4-joint chain (cesium): thigh, shin, foot, toe(->foot)
                (_, 1) => shin,
                _ => foot,
            });
        }
    };
    assign_leg(&mut leg_l, &mut role, BodyRole::ThighL, BodyRole::ShinL, BodyRole::FootL);
    assign_leg(&mut leg_r, &mut role, BodyRole::ThighR, BodyRole::ShinR, BodyRole::FootR);

    // ---- materialise the table; unclassified -> pelvis ----
    let mut table = vec![0u8; n];
    let mut unclassified = Vec::new();
    for j in 0..n {
        let r = role[j].unwrap_or_else(|| {
            unclassified.push(j);
            BodyRole::Pelvis
        });
        table[j] = r.bone(map) as u8;
    }
    (table, unclassified)
}

/// Quantise a foreign vertex's 4 native float weights to u8x4 that sum EXACTLY to
/// 255 (the donor's UBYTE4N convention). Rounds each, then adds the rounding
/// residual to the largest-weight slot so the four bytes total 255. A fully-zero
/// input (no skin) returns the rigid `[255,0,0,0]` fallback.
pub fn quantise_weights(w: [f32; 4]) -> [u8; 4] {
    let sum: f32 = w.iter().sum();
    if sum <= 1e-6 {
        return [0xff, 0, 0, 0];
    }
    let norm = [w[0] / sum, w[1] / sum, w[2] / sum, w[3] / sum];
    let mut q = [0u8; 4];
    let mut total = 0i32;
    for i in 0..4 {
        let v = (norm[i] * 255.0).round() as i32;
        let v = v.clamp(0, 255);
        q[i] = v as u8;
        total += v;
    }
    let residual = 255 - total;
    if residual != 0 {
        // apply the residual to the largest-weight slot (keeps the dominant bone)
        let big = (0..4).max_by(|&a, &b| norm[a].partial_cmp(&norm[b]).unwrap()).unwrap();
        let nv = (q[big] as i32 + residual).clamp(0, 255);
        q[big] = nv as u8;
    }
    q
}

/// Remap a foreign vertex's 4 `JOINTS_0` indices through the retarget table to 4
/// donor global bone indices.
pub fn remap_joints(j0: [u16; 4], table: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for i in 0..4 {
        let idx = j0[i] as usize;
        out[i] = if idx < table.len() { table[idx] } else { 0 };
    }
    out
}
