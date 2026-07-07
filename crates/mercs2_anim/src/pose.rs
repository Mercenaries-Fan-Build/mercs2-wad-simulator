//! Pose evaluation — faithful `hkQsTransform` sample/compose/blend math.
//!
//! Ported verbatim from `mercs2_engine::pose` (the proven pipeline) so this crate is self-contained
//! and never depends on `mercs2_engine`. Row-major / row-vector convention throughout
//! (`world = local · world_parent`), matching `mercs2_formats::skeleton`.
//!
//! The math is the actual Havok Animation 5.5 pipeline the retail engine runs, not a matrix
//! approximation:
//!   * `sampleAndCombine`: localPose starts as the reference (bind) pose; animated TRANSFORM tracks
//!     overwrite their bone's full `hkQsTransform`. Undriven bones keep the bind pose.
//!   * `transformLocalPoseToModelPose`: `model[b] = model[parent] * local[b]` via `hkQsTransform::setMul`.
//!   * skin: `skinMatrix[b] = InvBind[b] · model[b]`.

use mercs2_formats::anim::QsTransform;
use mercs2_formats::skeleton::mat4_mul;

/// One bone's rig data — the decoupled analog of `mercs2_engine::mesh::BoneRig`. The engine builds
/// these from its `SkinData`; this crate only reads `parent`, `inv_bind`, `local_bind` for the
/// pose compose (plus `world_bind`/`name_hash` for IK anchoring).
#[derive(Debug, Clone)]
pub struct BoneRig {
    /// Parent bone index, or `-1` for a root.
    pub parent: i32,
    /// HIER node name hash (`m2(bone_name)`) — used to anchor IK on named leg bones.
    pub name_hash: u32,
    /// Bind-pose world-rest transform (row-major, translation in row 3).
    pub world_bind: [[f32; 4]; 4],
    /// Inverse of `world_bind` (the `InvBind` in `Skin[b] = InvBind[b] · Pose[b]`).
    pub inv_bind: [[f32; 4]; 4],
    /// Bind-pose LOCAL transform (relative to parent). Animation replaces this per driven bone.
    pub local_bind: [[f32; 4]; 4],
}

const QS_IDENTITY: QsTransform = QsTransform {
    translation: [0.0, 0.0, 0.0],
    rotation: [0.0, 0.0, 0.0, 1.0],
    scale: [1.0, 1.0, 1.0],
};

/// hkQuaternion multiply (Hamilton product), xyzw. `q = a * b`.
fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let ([ax, ay, az, aw], [bx, by, bz, bw]) = (a, b);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotate a vector by a quaternion (xyzw): `v' = v + 2w(u×v) + 2(u×(u×v))`, u = q.xyz.
fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let u = [x, y, z];
    let t = [
        2.0 * (u[1] * v[2] - u[2] * v[1]),
        2.0 * (u[2] * v[0] - u[0] * v[2]),
        2.0 * (u[0] * v[1] - u[1] * v[0]),
    ];
    [
        v[0] + w * t[0] + (u[1] * t[2] - u[2] * t[1]),
        v[1] + w * t[1] + (u[2] * t[0] - u[0] * t[2]),
        v[2] + w * t[2] + (u[0] * t[1] - u[1] * t[0]),
    ]
}

/// `hkQsTransform::setMul` — `out = a * b` (a = parent, b = child/local):
/// rotation = a.rot * b.rot; translation = a.t + a.rot·(a.scale ⊙ b.t); scale = a.scale ⊙ b.scale.
fn qs_mul(a: &QsTransform, b: &QsTransform) -> QsTransform {
    let rotation = quat_mul(a.rotation, b.rotation);
    let st = [
        a.scale[0] * b.translation[0],
        a.scale[1] * b.translation[1],
        a.scale[2] * b.translation[2],
    ];
    let r = quat_rotate(a.rotation, st);
    QsTransform {
        rotation,
        translation: [a.translation[0] + r[0], a.translation[1] + r[1], a.translation[2] + r[2]],
        scale: [a.scale[0] * b.scale[0], a.scale[1] * b.scale[1], a.scale[2] * b.scale[2]],
    }
}

/// Decompose a row-vector affine matrix (HIER local) into an `hkQsTransform`.
pub fn mat_to_qs(m: &[[f32; 4]; 4]) -> QsTransform {
    let sx = (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt();
    let sy = (m[1][0] * m[1][0] + m[1][1] * m[1][1] + m[1][2] * m[1][2]).sqrt();
    let sz = (m[2][0] * m[2][0] + m[2][1] * m[2][1] + m[2][2] * m[2][2]).sqrt();
    let (ix, iy, iz) = (1.0 / sx.max(1e-8), 1.0 / sy.max(1e-8), 1.0 / sz.max(1e-8));
    let r = [
        [m[0][0] * ix, m[0][1] * ix, m[0][2] * ix],
        [m[1][0] * iy, m[1][1] * iy, m[1][2] * iy],
        [m[2][0] * iz, m[2][1] * iz, m[2][2] * iz],
    ];
    let trace = r[0][0] + r[1][1] + r[2][2];
    let rotation = if trace > 0.0 {
        let w = (1.0 + trace).sqrt() * 0.5;
        let f = 1.0 / (4.0 * w);
        [(r[1][2] - r[2][1]) * f, (r[2][0] - r[0][2]) * f, (r[0][1] - r[1][0]) * f, w]
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
        [0.25 * s, (r[1][0] + r[0][1]) / s, (r[2][0] + r[0][2]) / s, (r[1][2] - r[2][1]) / s]
    } else if r[1][1] > r[2][2] {
        let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
        [(r[1][0] + r[0][1]) / s, 0.25 * s, (r[2][1] + r[1][2]) / s, (r[2][0] - r[0][2]) / s]
    } else {
        let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
        [(r[2][0] + r[0][2]) / s, (r[2][1] + r[1][2]) / s, 0.25 * s, (r[0][1] - r[1][0]) / s]
    };
    QsTransform { translation: [m[3][0], m[3][1], m[3][2]], rotation, scale: [sx, sy, sz] }
}

/// Compose a `QsTransform` into a row-major, row-vector LOCAL matrix (`p' = p · M`).
pub fn qs_to_local(qs: &QsTransform) -> [[f32; 4]; 4] {
    let [x, y, z, w] = qs.rotation;
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + w * z), 2.0 * (x * z - w * y)],
        [2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x)],
        [2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let s = qs.scale;
    let t = qs.translation;
    [
        [s[0] * r[0][0], s[0] * r[0][1], s[0] * r[0][2], 0.0],
        [s[1] * r[1][0], s[1] * r[1][1], s[1] * r[1][2], 0.0],
        [s[2] * r[2][0], s[2] * r[2][1], s[2] * r[2][2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

/// Bind local poses as `hkQsTransform`s (from the rig's bind-local matrices).
pub fn bind_qs(rig: &[BoneRig]) -> Vec<QsTransform> {
    rig.iter().map(|b| mat_to_qs(&b.local_bind)).collect()
}

/// `transformLocalPoseToModelPose`: `model[b] = model[parent] * local[b]`, root = local.
/// (HIER guarantees parent index < child.)
pub fn model_poses(rig: &[BoneRig], local: &[QsTransform]) -> Vec<QsTransform> {
    let mut model = vec![QS_IDENTITY; rig.len()];
    for b in 0..rig.len() {
        model[b] = if rig[b].parent < 0 {
            local[b]
        } else {
            qs_mul(&model[rig[b].parent as usize], &local[b])
        };
    }
    model
}

/// Skinning palette from model-space `hkQsTransform`s: `Skin[b] = InvBind[b] · model[b]`.
pub fn skin_palette(rig: &[BoneRig], model: &[QsTransform]) -> Vec<[[f32; 4]; 4]> {
    (0..rig.len())
        .map(|b| mat4_mul(&rig[b].inv_bind, &qs_to_local(&model[b])))
        .collect()
}

/// Full Havok pose->palette for one sampled frame (raw clip, root motion kept).
pub fn havok_palette(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<[[f32; 4]; 4]> {
    let local = havok_locals(rig, sample, track_to_hier, num_transform_tracks);
    skin_palette(rig, &model_poses(rig, &local))
}

/// `havok_palette` with baked root locomotion stripped (root keeps its BIND local translation), so a
/// striding clip animates in place and the entity `Transform` carries the ground motion.
pub fn havok_palette_in_place(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<[[f32; 4]; 4]> {
    let mut local = havok_locals(rig, sample, track_to_hier, num_transform_tracks);
    strip_root(rig, &mut local);
    skin_palette(rig, &model_poses(rig, &local))
}

/// Crossfade variant of [`havok_palette_in_place`]: sampleAndCombine BOTH clip samples into per-bone
/// locals, blend per bone (`hkaSkeletonUtils::blendPoses` math, `w` = weight of sample B), strip root
/// locomotion, then compose. Used during clip switches.
#[allow(clippy::too_many_arguments)]
pub fn havok_palette_blend_in_place(
    rig: &[BoneRig],
    sample_a: &[QsTransform],
    track_to_hier_a: &[Option<usize>],
    num_transform_tracks_a: usize,
    sample_b: &[QsTransform],
    track_to_hier_b: &[Option<usize>],
    num_transform_tracks_b: usize,
    w: f32,
) -> Vec<[[f32; 4]; 4]> {
    let la = havok_locals(rig, sample_a, track_to_hier_a, num_transform_tracks_a);
    let lb = havok_locals(rig, sample_b, track_to_hier_b, num_transform_tracks_b);
    let mut local: Vec<QsTransform> = la.iter().zip(&lb).map(|(a, b)| qs_blend(a, b, w)).collect();
    strip_root(rig, &mut local);
    skin_palette(rig, &model_poses(rig, &local))
}

/// Root bones keep their BIND local translation (foot-lock / in-place).
fn strip_root(rig: &[BoneRig], local: &mut [QsTransform]) {
    for b in 0..rig.len() {
        if rig[b].parent < 0 {
            let m = rig[b].local_bind;
            local[b].translation = [m[3][0], m[3][1], m[3][2]];
        }
    }
}

/// The horizontal distance the ROOT bone travels between two clip samples (its BAKED locomotion),
/// divided by elapsed time = the clip's authentic ground SPEED (m/s). Returns 0 for clips whose root
/// doesn't translate (idle). The locomotion system advances the entity `Transform` at this rate so
/// nothing foot-slides.
pub fn clip_root_speed(
    rig: &[BoneRig],
    start_sample: &[QsTransform],
    end_sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
    elapsed: f32,
) -> f32 {
    let Some(root) = rig.iter().position(|b| b.parent < 0) else { return 0.0 };
    let a = havok_locals(rig, start_sample, track_to_hier, num_transform_tracks);
    let b = havok_locals(rig, end_sample, track_to_hier, num_transform_tracks);
    if root >= a.len() || root >= b.len() {
        return 0.0;
    }
    let (dx, dz) = (
        b[root].translation[0] - a[root].translation[0],
        b[root].translation[2] - a[root].translation[2],
    );
    (dx * dx + dz * dz).sqrt() / elapsed.max(1e-3)
}

/// `hkaSkeletonUtils::blendPoses` per-bone math: translation/scale lerp, rotation nlerp with the
/// hemisphere fix. `w` = weight of `b` (0 = pure a, 1 = pure b).
pub fn qs_blend(a: &QsTransform, b: &QsTransform, w: f32) -> QsTransform {
    let l3 = |x: [f32; 3], y: [f32; 3]| {
        [x[0] + (y[0] - x[0]) * w, x[1] + (y[1] - x[1]) * w, x[2] + (y[2] - x[2]) * w]
    };
    let (qa, mut qb) = (a.rotation, b.rotation);
    if qa[0] * qb[0] + qa[1] * qb[1] + qa[2] * qb[2] + qa[3] * qb[3] < 0.0 {
        for c in qb.iter_mut() {
            *c = -*c;
        }
    }
    let mut r = [0.0f32; 4];
    for i in 0..4 {
        r[i] = qa[i] + (qb[i] - qa[i]) * w;
    }
    let n = (r.iter().map(|c| c * c).sum::<f32>()).sqrt();
    if n > 1e-6 {
        for c in r.iter_mut() {
            *c /= n;
        }
    } else {
        r = qa;
    }
    QsTransform { translation: l3(a.translation, b.translation), rotation: r, scale: l3(a.scale, b.scale) }
}

/// `sampleAndCombine` into per-bone local `hkQsTransform`s.
fn havok_locals(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<QsTransform> {
    let mut local = bind_qs(rig);
    for (track, bone) in track_to_hier.iter().enumerate() {
        if track >= num_transform_tracks {
            break;
        }
        if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
            if b < local.len() {
                let mut q = *qs;
                let qn = (q.rotation.iter().map(|c| c * c).sum::<f32>()).sqrt();
                if qn > 1e-6 {
                    for c in q.rotation.iter_mut() {
                        *c /= qn;
                    }
                }
                local[b] = q;
            }
        }
    }
    local
}

/// Flatten a palette to row-major f32s for a storage buffer.
pub fn flatten(palette: &[[[f32; 4]; 4]]) -> Vec<f32> {
    let mut out = Vec::with_capacity(palette.len() * 16);
    for m in palette {
        for row in m {
            out.extend_from_slice(row);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single-root, single-child rig with identity bind (root) + a translated child.
    fn two_bone_rig() -> Vec<BoneRig> {
        let id = |t: [f32; 3]| {
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [t[0], t[1], t[2], 1.0]]
        };
        vec![
            BoneRig { parent: -1, name_hash: 1, world_bind: id([0.0; 3]), inv_bind: id([0.0; 3]), local_bind: id([0.0; 3]) },
            BoneRig { parent: 0, name_hash: 2, world_bind: id([0.0, 1.0, 0.0]), inv_bind: id([0.0, -1.0, 0.0]), local_bind: id([0.0, 1.0, 0.0]) },
        ]
    }

    #[test]
    fn bind_pose_palette_is_identity() {
        let rig = two_bone_rig();
        let local = bind_qs(&rig);
        let pal = skin_palette(&rig, &model_poses(&rig, &local));
        // At bind pose Skin = InvBind · WorldBind = identity for every bone.
        for m in &pal {
            for r in 0..4 {
                for c in 0..4 {
                    let want = if r == c { 1.0 } else { 0.0 };
                    assert!((m[r][c] - want).abs() < 1e-4, "bind palette must be identity");
                }
            }
        }
    }

    #[test]
    fn crossfade_endpoints_and_midpoint() {
        // A: child rotated +90° about X; B: identity. Blend must land on A at w=0, B at w=1.
        let a = QsTransform { translation: [0.0, 1.0, 0.0], rotation: [0.70710677, 0.0, 0.0, 0.70710677], scale: [1.0; 3] };
        let b = QsTransform { translation: [0.0, 1.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] };
        let at_0 = qs_blend(&a, &b, 0.0);
        let at_1 = qs_blend(&a, &b, 1.0);
        for i in 0..4 {
            assert!((at_0.rotation[i] - a.rotation[i]).abs() < 1e-5, "w=0 == A");
            assert!((at_1.rotation[i] - b.rotation[i]).abs() < 1e-5, "w=1 == B");
        }
        // Midpoint quaternion is normalized and strictly between (w component grows from .707 to 1).
        let mid = qs_blend(&a, &b, 0.5);
        let n = (mid.rotation.iter().map(|c| c * c).sum::<f32>()).sqrt();
        assert!((n - 1.0).abs() < 1e-5, "blended quat stays unit");
        assert!(mid.rotation[3] > a.rotation[3] && mid.rotation[3] < b.rotation[3], "w between endpoints");
    }
}
