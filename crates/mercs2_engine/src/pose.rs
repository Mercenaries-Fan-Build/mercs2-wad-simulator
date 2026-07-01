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
