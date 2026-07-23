//! Faithful character-skinning writer — the exact inverse of the proven palette reader in
//! [`crate::model_cubeize`].
//!
//! Ported from the `mercs2-mesher` browser tool (byte-exact to the Python that produced two
//! in-game-confirmed characters). Where our existing injection path wrote **global**
//! BLENDINDICES and never authored a palette, this module produces the shipped-format SKIN
//! group data: **per-group palette-relative** BLENDINDICES + the `INFO(56)` range table
//! (`+20 u32 range_count`, then `{u16 hier_base, u16 count}×rc` from `+24`), plus a
//! direction-aligned re-pose onto the fixed game skeleton.
//!
//! Layers:
//!   * [`mat`]      — dependency-free f64 linear algebra (parity with `mat.js`).
//!   * [`automap`]  — source-rig → 84-bone auto-mapper (`automap.js`).
//!   * [`build`]    — palette + skin bytes + re-pose (`build.js`).
//!   * [`validate`] — the five-check battery (`validate.js`).
//!
//! `char_skin` is glTF-free: callers feed plain arrays (a glTF adapter lives in the CLI).

pub mod automap;
pub mod build;
pub mod mat;
pub mod transfer;
pub mod donor_transfer;
pub mod validate;

pub use build::{
    build_character, BuildInput, CharGlbData, CharSkin, MeshPart, Mode, TargetBone, TargetSkeleton,
};

use crate::skeleton::Skeleton;

/// Gram-Schmidt a row-major 3×3 (rows = mapped basis) into an orthonormal rotation, or `None`
/// if it is degenerate (zero/parallel rows). Removes any scale a bind matrix carried.
fn ortho3_colvec(m: [f64; 9]) -> Option<[f64; 9]> {
    let row = |i: usize| [m[i * 3], m[i * 3 + 1], m[i * 3 + 2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3], s: f64| [a[0] - b[0] * s, a[1] - b[1] * s, a[2] - b[2] * s];
    let normed = |a: [f64; 3]| {
        let n = dot(a, a).sqrt();
        if n < 1e-9 { None } else { Some([a[0] / n, a[1] / n, a[2] / n]) }
    };
    let u0 = normed(row(0))?;
    let mut r1 = row(1);
    r1 = sub(r1, u0, dot(r1, u0));
    let u1 = normed(r1)?;
    let mut r2 = row(2);
    r2 = sub(r2, u0, dot(r2, u0));
    r2 = sub(r2, u1, dot(r2, u1));
    let u2 = normed(r2)?;
    Some([u0[0], u0[1], u0[2], u1[0], u1[1], u1[2], u2[0], u2[1], u2[2]])
}

impl TargetSkeleton {
    /// Derive the target skeleton from a real donor's [`Skeleton`]. Bones are in HIER order,
    /// so `bone i` is global HIER index `i` — matching the mesher's baked `skeleton_npc84`.
    pub fn from_skeleton(sk: &Skeleton) -> TargetSkeleton {
        let bones: Vec<TargetBone> = sk
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                // BIND pose, not the default pose: a shipped HIER can hold a posed default (Chris's
                // arms are bent 76 deg there) while the vertices were authored against the
                // inverse-bind at HIER+80. Retargeting geometry onto the default pose reproduces
                // that pose in the import. See `Bone::bind_world`.
                let p = b.bind_pos();
                // ORIENTATION from the same matrix `bind_pos` reads (bind_world, else world). The
                // HIER 4x4 is row-vector (basis in the rows, translation in row 3); `apply3` is
                // column-vector, so the column-vector rotation is the TRANSPOSE of the upper-left
                // 3x3, then Gram-Schmidt'd to a clean rotation (bind matrices can carry scale).
                let m = b.bind_world.unwrap_or(b.world);
                let rot = ortho3_colvec([
                    m[0][0] as f64, m[1][0] as f64, m[2][0] as f64,
                    m[0][1] as f64, m[1][1] as f64, m[2][1] as f64,
                    m[0][2] as f64, m[1][2] as f64, m[2][2] as f64,
                ]);
                TargetBone {
                    i: i as u32,
                    pos: [p[0] as f64, p[1] as f64, p[2] as f64],
                    parent: b.parent,
                    name: format!("hash_{:08X}", b.name_hash),
                    name_hash: b.name_hash,
                    rot,
                }
            })
            .collect();
        let ys: Vec<f64> = bones.iter().map(|b| b.pos[1]).collect();
        let height = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - ys.iter().cloned().fold(f64::INFINITY, f64::min);
        TargetSkeleton { bones, height }
    }
}

/// Canonical NPC-84 bone name for each HIER index that [`automap`](automap::automap) can emit.
/// The auto-mapper works in the 84-bone convention, but a real donor may reorder/extend the HIER
/// (a HERO rig like `mattias_v2` inserts extra root/attach bones, so index 3 is `bone_root`, not
/// `Bone_Hips`). Resolve an automap index onto ANY target skeleton BY NAME via [`npc84_bone_name`]
/// rather than trusting the raw index. Unnamed NPC-84 slots (`hash_*`) are never emitted by automap.
pub const NPC84_NAMES: [&str; 84] = [
    "GlobalSRT", "Bone_Attach_Root", "bone_root", "Bone_Hips", "hash_1C2E8837", "hash_629B2990",
    "Bone_LThigh", "Bone_LShin", "Bone_LFootBone1", "Bone_LFootBone2", "Bone_RThigh", "Bone_RShin",
    "Bone_RFootBone1", "Bone_RFootBone2", "bone_spine1", "Bone_Spine2", "Bone_Chest",
    "hash_3846CB35", "hash_DF2D0826", "bone_attach_chest", "bone_neck", "Bone_Head",
    "bone_nose_right", "bone_nose_left", "bone_mouth_top_right", "bone_mouth_top_left",
    "bone_mouth_corner_right", "bone_mouth_corner_left", "bone_eyelid_top_right",
    "bone_eyelid_top_left", "bone_eyebrow_right", "bone_eyebrow_left", "bone_eyebrow_center",
    "bone_eyeball_right", "bone_eyeball_left", "bone_cheek_left", "bone_cheek_right",
    "bone_brow_center", "bone_jaw", "bone_tongue_tip", "bone_mouth_bottom_right",
    "bone_mouth_bottom_left", "bone_lshoulder", "Bone_LBicep", "Bone_LForearm", "bone_lforearmroll",
    "bone_lhand", "bone_attach_lhand", "bone_lindex1", "bone_lindex2", "bone_lindex3",
    "bone_lmiddle1", "bone_lmiddle2", "bone_lmiddle3", "bone_lpinky1", "bone_lpinky2", "bone_lpinky3",
    "bone_lring1", "bone_lring2", "bone_lring3", "bone_lthumb1", "bone_lthumb2", "bone_lthumb3",
    "bone_rshoulder", "Bone_RBicep", "Bone_RForearm", "bone_rforearmroll", "bone_rhand",
    "bone_attach_rhand", "bone_rindex1", "bone_rindex2", "bone_rindex3", "bone_rmiddle1",
    "bone_rmiddle2", "bone_rmiddle3", "bone_rpinky1", "bone_rpinky2", "bone_rpinky3", "bone_rring1",
    "bone_rring2", "bone_rring3", "bone_rthumb1", "bone_rthumb2", "bone_rthumb3",
];

/// The canonical NPC-84 bone name for an automap HIER index (see [`NPC84_NAMES`]).
pub fn npc84_bone_name(hier: u32) -> Option<&'static str> {
    NPC84_NAMES.get(hier as usize).copied()
}

/// The HIER name-hash of the canonical NPC-84 bone at an automap index — used to re-seat an
/// automap index onto any donor's HIER by identity ([`TargetSkeleton::index_by_canonical`]).
/// `None` for the four unnamed `hash_*` slots (never emitted by [`automap`](automap::automap)).
pub fn npc84_name_hash(hier: u32) -> Option<u32> {
    let name = npc84_bone_name(hier)?;
    if name.starts_with("hash_") {
        return None;
    }
    Some(crate::hash::pandemic_hash_m2(name))
}

/// Expand a palette range table to `slot → global HIER` — the SAME expansion
/// [`crate::model_cubeize`] applies when reading. Used to round-trip-verify the writer.
pub fn expand_ranges(ranges: &[(u16, u16)]) -> Vec<u16> {
    let mut palette = Vec::new();
    for &(base, count) in ranges {
        for h in base..base + count {
            palette.push(h);
        }
    }
    palette
}

/// Patch a SKIN group's `INFO(56)` leaf with a new palette range table, preserving the
/// game-required header bytes (`+0..20`). Overwrites `range_count@+20` and the
/// `{u16 base, u16 count}` pairs from `+24`, zeroing the remainder of the 56-byte leaf.
/// This is the write-side inverse of the read in `model_cubeize` (gated `1..=8` ranges).
pub fn patch_skin_info56(info: &mut [u8], ranges: &[(u16, u16)]) -> Result<(), String> {
    if info.len() < 56 {
        return Err(format!("INFO leaf is {} bytes, need >= 56", info.len()));
    }
    if ranges.is_empty() || ranges.len() > 8 {
        return Err(format!("range_count {} out of the reader's 1..=8 gate", ranges.len()));
    }
    info[20..24].copy_from_slice(&(ranges.len() as u32).to_le_bytes());
    let mut o = 24;
    for &(base, count) in ranges {
        info[o..o + 2].copy_from_slice(&base.to_le_bytes());
        info[o + 2..o + 4].copy_from_slice(&count.to_le_bytes());
        o += 4;
    }
    // zero the rest of the leaf so no stale pair bytes survive
    for b in info[o..56].iter_mut() {
        *b = 0;
    }
    Ok(())
}

/// Build a fresh, reader-valid 56-byte SKIN `INFO` from a group hash + ranges. Header
/// mirrors [`crate::model_build`]'s `build_skin_palette_info` (record/sub counts, group id);
/// used by the round-trip test and from-scratch SKIN authoring.
pub fn skin_info56(group_hash: u32, ranges: &[(u16, u16)]) -> Result<[u8; 56], String> {
    let mut info = [0u8; 56];
    info[0..4].copy_from_slice(&1u32.to_le_bytes()); // record count
    info[4..8].copy_from_slice(&1u32.to_le_bytes()); // sub count
    info[12..16].copy_from_slice(&group_hash.to_le_bytes());
    patch_skin_info56(&mut info, ranges)?;
    Ok(info)
}
