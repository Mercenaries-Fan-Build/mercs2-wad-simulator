//! Ragdoll skeleton seam — the animation-side glue between a posed character skeleton and the
//! physics-system ragdoll (`mercs2_physics::ragdoll`).
//!
//! # Why the seam lives here (and stays physics-free)
//!
//! The constrained multi-body sim + the recovered body/constraint data live in `mercs2_physics`
//! (`RagdollDef::human`, `Ragdoll`). This crate must **not** take a leaf→leaf edge to the physics
//! system (the carve rule; see the crate docs). So this module speaks only in plain skeleton terms —
//! bone name-hashes and `(translation, rotation)` pairs — which the combat/death integrator hands to
//! `mercs2_physics::Ragdoll::spawn_with` and reads back from `Ragdoll::bone_transforms`.
//!
//! Two directions, matching the retail `WSHumanRagdoll` handoff:
//! - [`body_seeds`] — the **alive → ragdoll** snap (`SetBodyToRagdoll`): read each ragdoll bone's
//!   current animated MODEL-space transform so the physics body spawns exactly on the posed skeleton.
//! - [`write_back_model_pose`] — the **ragdoll → skin** read-back: overwrite the driven bones' model
//!   matrices with the simulated transforms each frame.
//!
//! Retail's `hkaSkeletonMapper` (animation↔ragdoll) is a no-op mapping here because each ragdoll body
//! *is* a named skeleton bone: the map is by name-hash identity (confirmed — the 11 ragdoll bone
//! hashes are present in every human HIER; see `mercs2_physics::ragdoll` docs). The retail WAD ships
//! **no** serialized mapper (a full-WAD census found zero `hkaSkeletonMapper` instances), consistent
//! with an identity/by-name mapping built at runtime.

use mercs2_core::glam::{Mat3, Quat, Vec3};

use crate::pose::BoneRig;

/// Decompose a row-major / row-vector model matrix (`world = local · world_parent`, translation in
/// row 3) into its world `(translation, rotation)`. Scale is dropped (ragdoll bodies are rigid).
///
/// In the row-vector convention each ROW of the linear 3×3 is a local basis axis expressed in world
/// space, so those rows are the COLUMNS of the equivalent column-vector rotation — hence
/// `Mat3::from_cols(row0, row1, row2)`.
pub fn model_matrix_to_pos_rot(m: &[[f32; 4]; 4]) -> (Vec3, Quat) {
    let x = Vec3::new(m[0][0], m[0][1], m[0][2]);
    let y = Vec3::new(m[1][0], m[1][1], m[1][2]);
    let z = Vec3::new(m[2][0], m[2][1], m[2][2]);
    let rot = Quat::from_mat3(&Mat3::from_cols(
        x.normalize_or(Vec3::X),
        y.normalize_or(Vec3::Y),
        z.normalize_or(Vec3::Z),
    ))
    .normalize();
    let pos = Vec3::new(m[3][0], m[3][1], m[3][2]);
    (pos, rot)
}

/// Compose a world `(translation, rotation)` back into a row-major / row-vector model matrix (the
/// inverse of [`model_matrix_to_pos_rot`]).
pub fn pos_rot_to_model_matrix(pos: Vec3, rot: Quat) -> [[f32; 4]; 4] {
    let r = Mat3::from_quat(rot.normalize());
    let (ax, ay, az) = (r.x_axis, r.y_axis, r.z_axis);
    [
        [ax.x, ax.y, ax.z, 0.0],
        [ay.x, ay.y, ay.z, 0.0],
        [az.x, az.y, az.z, 0.0],
        [pos.x, pos.y, pos.z, 1.0],
    ]
}

/// **Alive → ragdoll snap.** For each ragdoll body bone-hash (in the order the physics `RagdollDef`
/// lists its bodies), read that bone's current animated MODEL-space transform and return its world
/// `(position, orientation)`. The combat/death path feeds these to
/// `mercs2_physics::Ragdoll::spawn_with` (or builds `BodySeed`s from them).
///
/// `model_pose[b]` is bone `b`'s model-space 4×4 (the `model[b]` the pose pipeline computes, before
/// the inverse-bind is applied for skinning). `rig` supplies the bone→index lookup by `name_hash`.
/// Returns `None` for any bone-hash not in this character's rig (so the caller can decide whether the
/// ragdoll is applicable).
pub fn body_seeds(
    rig: &[BoneRig],
    model_pose: &[[[f32; 4]; 4]],
    bone_hashes: &[u32],
) -> Vec<Option<(Vec3, Quat)>> {
    bone_hashes
        .iter()
        .map(|&h| {
            rig.iter()
                .position(|b| b.name_hash == h)
                .filter(|&i| i < model_pose.len())
                .map(|i| model_matrix_to_pos_rot(&model_pose[i]))
        })
        .collect()
}

/// **Ragdoll → skin read-back.** Overwrite the driven bones' model matrices with the simulated
/// transforms (`driven` = `(name_hash, position, orientation)`, e.g. from
/// `mercs2_physics::Ragdoll::bone_transforms`). Non-ragdoll bones are left untouched; the engine's
/// usual local→model propagation carries child/accessory bones (fingers, feet, hair) along with their
/// nearest driven ancestor. Returns the number of bones written.
pub fn write_back_model_pose(
    rig: &[BoneRig],
    model_pose: &mut [[[f32; 4]; 4]],
    driven: &[(u32, Vec3, Quat)],
) -> usize {
    let mut n = 0;
    for &(h, pos, rot) in driven {
        if let Some(i) = rig.iter().position(|b| b.name_hash == h) {
            if i < model_pose.len() {
                model_pose[i] = pos_rot_to_model_matrix(pos, rot);
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig_with(hashes: &[u32]) -> Vec<BoneRig> {
        hashes
            .iter()
            .map(|&h| BoneRig {
                parent: -1,
                name_hash: h,
                world_bind: identity4(),
                inv_bind: identity4(),
                local_bind: identity4(),
            })
            .collect()
    }
    fn identity4() -> [[f32; 4]; 4] {
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
    }

    #[test]
    fn matrix_round_trips_through_pos_rot() {
        let rot = Quat::from_axis_angle(Vec3::new(0.3, 1.0, -0.2).normalize(), 0.9);
        let pos = Vec3::new(1.5, -2.0, 0.75);
        let m = pos_rot_to_model_matrix(pos, rot);
        let (p2, r2) = model_matrix_to_pos_rot(&m);
        assert!((p2 - pos).length() < 1e-5, "pos {p2:?} vs {pos:?}");
        // Quaternion sign may flip; compare the rotation action on a probe vector.
        let v = Vec3::new(0.4, 0.2, -0.9);
        assert!((r2 * v - rot * v).length() < 1e-5, "rotation mismatch");
    }

    #[test]
    fn body_seeds_reads_driven_bones_and_skips_absent() {
        let hashes = [0x24C5_009Cu32, 0x4C77_33ED, 0x705C_4508];
        let rig = rig_with(&hashes);
        // Give each bone a distinct model translation.
        let mut pose = vec![identity4(); 3];
        pose[0][3] = [0.0, 1.0, 0.0, 1.0];
        pose[1][3] = [0.0, 1.35, 0.0, 1.0];
        pose[2][3] = [0.0, 1.65, 0.0, 1.0];
        // Ask for two present + one absent bone.
        let seeds = body_seeds(&rig, &pose, &[0x24C5_009C, 0xDEAD_BEEF, 0x705C_4508]);
        assert_eq!(seeds.len(), 3);
        assert!((seeds[0].unwrap().0 - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
        assert!(seeds[1].is_none(), "absent bone -> None");
        assert!((seeds[2].unwrap().0 - Vec3::new(0.0, 1.65, 0.0)).length() < 1e-6);
    }

    #[test]
    fn write_back_overwrites_only_driven_bones() {
        let hashes = [0x24C5_009Cu32, 0x4C77_33ED, 0x705C_4508];
        let rig = rig_with(&hashes);
        let mut pose = vec![identity4(); 3];
        let driven = [
            (0x24C5_009Cu32, Vec3::new(2.0, 3.0, 4.0), Quat::IDENTITY),
            (0x705C_4508u32, Vec3::new(-1.0, 0.5, 0.0), Quat::IDENTITY),
        ];
        let n = write_back_model_pose(&rig, &mut pose, &driven);
        assert_eq!(n, 2);
        assert_eq!(pose[0][3], [2.0, 3.0, 4.0, 1.0]);
        assert_eq!(pose[1][3], [0.0, 0.0, 0.0, 1.0], "undriven bone untouched");
        assert_eq!(pose[2][3], [-1.0, 0.5, 0.0, 1.0]);
    }
}
