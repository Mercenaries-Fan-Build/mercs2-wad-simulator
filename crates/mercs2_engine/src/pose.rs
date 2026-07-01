//! Engine-side pose evaluation: recompose a skinning palette from a `BoneRig` hierarchy and a set
//! of per-bone LOCAL transforms (from animation, or the bind-pose local for un-animated bones).
//! At bind pose the palette is identity — matching the static build (the LBS gate).
//!
//! Row-major, row-vector convention throughout (`world = local · world_parent`), matching
//! `mercs2_formats::skeleton`. The palette is uploaded to the shader row-major (see shader.wgsl).
//!
//! The synthetic driver here is a PLACEHOLDER proof-of-life for the per-frame palette-upload +
//! LBS path; it is replaced by `mercs2_formats::anim` clip sampling once that lands.

use crate::mesh::BoneRig;
use mercs2_formats::anim::QsTransform;
use mercs2_formats::skeleton::mat4_mul;

// ---------------------------------------------------------------------------
// Faithful Havok hkQsTransform math (Havok Animation 5.5) — the actual pipeline the
// engine runs, not a matrix approximation:
//   sampleAndCombine: localPose starts as the reference (bind) pose; animated TRANSFORM
//     tracks overwrite their bone's full hkQsTransform. Undriven bones / float-track slots
//     keep the bind pose (so they don't collapse to (0,0,0)).
//   transformLocalPoseToModelPose: model[b] = model[parent] * local[b]  via hkQsTransform::setMul.
//   skin: skinMatrix[b] = InvBind[b] · model[b].
// ---------------------------------------------------------------------------

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

/// hkQsTransform::setMul — `out = a * b` (a = parent, b = child/local):
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

/// Decompose a row-vector affine matrix (as produced by `qs_to_local` / the HIER local) back into an
/// hkQsTransform: translation = row 3, scale = row lengths, rotation extracted from the normalized
/// R_row (inverse of `qs_to_local`'s quaternion→matrix).
pub fn mat_to_qs(m: &[[f32; 4]; 4]) -> QsTransform {
    let sx = (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt();
    let sy = (m[1][0] * m[1][0] + m[1][1] * m[1][1] + m[1][2] * m[1][2]).sqrt();
    let sz = (m[2][0] * m[2][0] + m[2][1] * m[2][1] + m[2][2] * m[2][2]).sqrt();
    let (ix, iy, iz) = (1.0 / sx.max(1e-8), 1.0 / sy.max(1e-8), 1.0 / sz.max(1e-8));
    // normalized R_row
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
    } else {
        // fallback: largest-diagonal branch (row-vector R_row convention)
        if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
            let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
            [0.25 * s, (r[1][0] + r[0][1]) / s, (r[2][0] + r[0][2]) / s, (r[1][2] - r[2][1]) / s]
        } else if r[1][1] > r[2][2] {
            let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
            [(r[1][0] + r[0][1]) / s, 0.25 * s, (r[2][1] + r[1][2]) / s, (r[2][0] - r[0][2]) / s]
        } else {
            let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
            [(r[2][0] + r[0][2]) / s, (r[2][1] + r[1][2]) / s, 0.25 * s, (r[0][1] - r[1][0]) / s]
        }
    };
    QsTransform { translation: [m[3][0], m[3][1], m[3][2]], rotation, scale: [sx, sy, sz] }
}

/// Bind local poses as hkQsTransforms (from the rig's bind-local matrices).
pub fn bind_qs(rig: &[BoneRig]) -> Vec<QsTransform> {
    rig.iter().map(|b| mat_to_qs(&b.local_bind)).collect()
}

/// `transformLocalPoseToModelPose`: model[b] = model[parent] * local[b] (hkQsTransform), root = local.
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

/// Skinning palette from model-space hkQsTransforms: Skin[b] = InvBind[b] · model[b] (row-vector).
pub fn skin_palette(rig: &[BoneRig], model: &[QsTransform]) -> Vec<[[f32; 4]; 4]> {
    (0..rig.len())
        .map(|b| mat4_mul(&rig[b].inv_bind, &qs_to_local(&model[b])))
        .collect()
}

/// Compose a Havok `QsTransform` (translation, quat xyzw, scale) into a row-major, row-vector
/// LOCAL matrix (`p' = p · M`): upper 3×3 = `diag(scale) · R_row`, translation in row 3 — the same
/// layout as the HIER local transform (`skeleton.rs`). Retail ships NO referencePose, so clip locals
/// are authored in the same frame as the HIER bind locals (verified by `--animcheck`: the animated
/// per-bone translation equals the bind-local translation, since bones rotate but don't stretch).
pub fn qs_to_local(qs: &QsTransform) -> [[f32; 4]; 4] {
    let [x, y, z, w] = qs.rotation;
    // Row-vector rotation matrix (transpose of the column-vector quaternion matrix).
    let r = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + w * z),
            2.0 * (x * z - w * y),
        ],
        [
            2.0 * (x * y - w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + w * x),
        ],
        [
            2.0 * (x * z + w * y),
            2.0 * (y * z - w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
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

/// Build the per-bone local transforms for a sampled animation frame. Mercs2 (like most humanoid
/// skeletal animation) is ROTATION-DRIVEN: bones rotate but their offset from the parent is rigid,
/// so each driven bone takes the clip's rotation (+scale) with its BIND local translation preserved.
/// The root's world-space translation (locomotion) is handled separately, so the root stays at its
/// bind local here — animating in place. Un-driven bones keep their HIER bind local.
///
/// This was verified by measurement (`--animdiag`): applying the clip's per-bone TRANSLATION blows
/// the mesh apart (frame-0 bbox extent 3.3 vs bind 1.9, since small offset errors on near-root bones
/// compound down the chain), while rotation-only + bind offsets reproduces the bind extent (1.98) as
/// a correctly posed figure. The rotation convention (`qs_to_local`, quat xyzw, row-vector) is
/// confirmed correct by the same test.
pub fn animate_locals(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
) -> Vec<[[f32; 4]; 4]> {
    let mut locals = bind_locals(rig);
    for (track, bone) in track_to_hier.iter().enumerate() {
        if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
            if b >= locals.len() || rig[b].parent < 0 {
                continue; // root: keep bind (its motion is world-space, applied elsewhere)
            }
            let mut m = qs_to_local(qs);
            let lb = rig[b].local_bind; // keep the rigid bind bone-offset; animate rotation/scale only
            m[3] = [lb[3][0], lb[3][1], lb[3][2], 1.0];
            locals[b] = m;
        }
    }
    locals
}

/// Compute `WorldPose[b]` for every bone from a set of local transforms (one per bone), then
/// `Skin[b] = InvBind[b] · WorldPose[b]`. `locals[b]` is the bone's animated local transform;
/// pass `rig[b].local_bind` for un-animated bones. Bones are assumed in parent-before-child order
/// (HIER guarantees parent index < child index).
pub fn palette(rig: &[BoneRig], locals: &[[[f32; 4]; 4]]) -> Vec<[[f32; 4]; 4]> {
    let n = rig.len();
    let mut world = vec![[[0.0f32; 4]; 4]; n];
    for b in 0..n {
        let l = locals[b];
        world[b] = if rig[b].parent < 0 {
            l
        } else {
            mat4_mul(&l, &world[rig[b].parent as usize])
        };
    }
    (0..n).map(|b| mat4_mul(&rig[b].inv_bind, &world[b])).collect()
}

/// Bind-pose locals (the identity gate): every bone keeps its bind-pose local transform.
pub fn bind_locals(rig: &[BoneRig]) -> Vec<[[f32; 4]; 4]> {
    rig.iter().map(|b| b.local_bind).collect()
}

/// Flatten a palette to row-major f32s for the storage buffer (WGSL reads column-major → the
/// transpose, so `bones[b] * v` computes the row-vector product `v · Skin[b]`).
pub fn flatten(palette: &[[[f32; 4]; 4]]) -> Vec<f32> {
    let mut out = Vec::with_capacity(palette.len() * 16);
    for m in palette {
        for row in m {
            out.extend_from_slice(row);
        }
    }
    out
}

/// SYNTHETIC animation proof (placeholder): oscillate one mid-hierarchy bone about its local X axis
/// so its descendants visibly swing relative to the body — exercising the per-frame palette upload
/// and LBS without a real clip. Bounded rotation, so it cannot diverge.
pub fn synthetic_palette(rig: &[BoneRig], t: f32) -> Vec<[[f32; 4]; 4]> {
    let mut locals = bind_locals(rig);
    if !rig.is_empty() {
        let b = rig.len() / 2; // arbitrary but deterministic joint
        let ang = 0.6 * (t * 1.5).sin(); // ±~34°
        // Rotate in the bone's own local frame (pre-multiply, row-vector: p · R · local).
        locals[b] = mat4_mul(&rot_x(ang), &rig[b].local_bind);
    }
    palette(rig, &locals)
}

/// Row-vector rotation about the local X axis (`p' = p · Rx`).
fn rot_x(a: f32) -> [[f32; 4]; 4] {
    let (s, c) = a.sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, c, s, 0.0],
        [0.0, -s, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
