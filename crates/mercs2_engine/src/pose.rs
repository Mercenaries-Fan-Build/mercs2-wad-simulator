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

/// Full Havok pose->palette for one sampled frame: sampleAndCombine (bind base, overwrite the
/// rotation of bones driven by a real transform track) -> model-space compose -> skin palette.
/// `sample` is the clip's per-track local pose; `track_to_hier` maps track->bone; only the first
/// `num_transform_tracks` are transforms (the rest are float tracks).
pub fn havok_palette(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<[[f32; 4]; 4]> {
    let local = havok_locals(rig, sample, track_to_hier, num_transform_tracks);
    skin_palette(rig, &model_poses(rig, &local))
}

/// `havok_palette` with baked root locomotion stripped: after sampleAndCombine, ROOT bones keep
/// their BIND local translation (sampled rotation/scale stay), so a clip whose root track strides
/// (the walk clip's root translates up to ~1.55 m) animates in place and the entity `Transform`
/// alone moves the model. Separate entry point so `--animate` / diagnostics keep the raw clip.
pub fn havok_palette_in_place(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<[[f32; 4]; 4]> {
    let mut local = havok_locals(rig, sample, track_to_hier, num_transform_tracks);
    for b in 0..rig.len() {
        if rig[b].parent < 0 {
            let lb = rig[b].local_bind;
            local[b].translation = [lb[3][0], lb[3][1], lb[3][2]];
        }
    }
    skin_palette(rig, &model_poses(rig, &local))
}

/// CROSS-SKELETON RETARGET palette. Drive a foreign character's OWN skeleton (`target_rig` — the
/// imported mesh's bones, in its own bind pose and proportions) with a clip that was authored for the
/// game's skeleton (`source_rig`), so the imported mesh animates without being deformed or reshaped.
///
/// Copying the clip's LOCAL rotations straight onto the target bones is WRONG whenever the two rigs'
/// bind bone-orientations differ (Maya/IW joint-orient vs the HIER frame) — it throws the character
/// into a spread-eagle mess. But so is applying the delta in WORLD space (`anim·bind⁻¹` then target
/// bind): that only works if the two skeletons' bind poses FACE the same world direction, and a
/// foreign character authored facing another way then runs sideways (legs stride off-axis while the
/// torso faces forward). The robust transfer expresses each source bone's rotation as a delta in its
/// OWN bind-LOCAL frame (`Δ = bind_model⁻¹ · anim_model`) and re-applies it in the TARGET bone's bind
/// frame (`target_model = target_bind_model · Δ`). Because the delta rides each bone's own bind
/// orientation, a global facing difference — and per-bone joint-orient differences — cancel out: the
/// hip swings like a hip, the elbow flexes like an elbow, in the character's OWN forward direction.
/// Target bone lengths (local translations) and the mesh/weights are never touched, so proportions
/// and off-body gear are kept.
///
/// `target_to_source[j]` maps target bone `j` → the `source_rig` bone that drives it (from the bone
/// map); an out-of-range entry leaves bone `j` at its bind pose. `track_to_hier_source` maps clip
/// track → `source_rig` bone. Assumes both rigs list parents before children (as HIER / a normal
/// glTF skeleton export do).
pub fn havok_palette_retarget_cross(
    target_rig: &[BoneRig],
    source_rig: &[BoneRig],
    target_to_source: &[usize],
    sample: &[QsTransform],
    track_to_hier_source: &[Option<usize>],
    num_transform_tracks: usize,
) -> Vec<[[f32; 4]; 4]> {
    let conj = |q: [f32; 4]| [-q[0], -q[1], -q[2], q[3]];
    // Source (clip) skeleton: bind vs animated model-space rotations.
    let src_anim_local = havok_locals(source_rig, sample, track_to_hier_source, num_transform_tracks);
    let src_anim_model = model_poses(source_rig, &src_anim_local);
    let src_bind_model = model_poses(source_rig, &bind_qs(source_rig));
    // Target skeleton bind model-space rotations.
    let tgt_bind_model = model_poses(target_rig, &bind_qs(target_rig));

    // Target model-space rotation per bone = target bind ∘ (source bind-LOCAL delta). Applying the
    // delta in the bone's OWN bind frame (target_bind · bind⁻¹·anim), not in world (anim·bind⁻¹ ·
    // target_bind), is what makes a differently-facing character move in its own forward direction.
    let mut tgt_model_rot: Vec<[f32; 4]> = vec![[0.0, 0.0, 0.0, 1.0]; target_rig.len()];
    for j in 0..target_rig.len() {
        let bind_rot = tgt_bind_model[j].rotation;
        let s = target_to_source.get(j).copied().unwrap_or(usize::MAX);
        tgt_model_rot[j] = if s < source_rig.len() {
            let delta = quat_mul(conj(src_bind_model[s].rotation), src_anim_model[s].rotation);
            quat_mul(bind_rot, delta)
        } else {
            bind_rot
        };
    }

    // Model rotation → local rotation (local = parent_model⁻¹ · model); keep target bind translation
    // and scale so bone lengths — and thus proportions — are the target's, not the clip skeleton's.
    let mut local = bind_qs(target_rig);
    for j in 0..target_rig.len() {
        let p = target_rig[j].parent;
        local[j].rotation = if p >= 0 {
            quat_mul(conj(tgt_model_rot[p as usize]), tgt_model_rot[j])
        } else {
            tgt_model_rot[j]
        };
    }
    skin_palette(target_rig, &model_poses(target_rig, &local))
}

/// The model-space matrix of a single `bone` for one clip sample — the same sampleAndCombine →
/// root-in-place → compose chain as [`havok_palette_in_place`], but returning ONE bone's model
/// transform (not the whole skin palette). Used to attach a held object (a weapon in the hand) to a
/// bone each frame: the object's world matrix is `entity_world · bone_model · grip`. Returns the
/// identity for an out-of-range bone.
pub fn bone_model_matrix(
    rig: &[BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    num_transform_tracks: usize,
    bone: usize,
) -> [[f32; 4]; 4] {
    let mut local = havok_locals(rig, sample, track_to_hier, num_transform_tracks);
    for b in 0..rig.len() {
        if rig[b].parent < 0 {
            let lb = rig[b].local_bind;
            local[b].translation = [lb[3][0], lb[3][1], lb[3][2]];
        }
    }
    let model = model_poses(rig, &local);
    model.get(bone).map(qs_to_local).unwrap_or([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// The horizontal distance the ROOT bone travels between two clip samples (its BAKED locomotion),
/// divided by the elapsed time = the clip's authentic ground SPEED (m/s). The world movement code
/// uses this to advance the entity Transform exactly as fast as the feet stride — so nothing slides
/// and FOOT_SYNC doesn't have to squash the step cadence to hide a hardcoded-speed mismatch. Pass
/// samples at t≈0 and t≈duration; returns 0 for clips whose root doesn't translate (idle).
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
    let (dx, dz) = (b[root].translation[0] - a[root].translation[0],
                    b[root].translation[2] - a[root].translation[2]);
    (dx * dx + dz * dz).sqrt() / elapsed.max(1e-3)
}

/// Crossfade variant of [`havok_palette_in_place`]: sampleAndCombine BOTH clip samples into
/// per-bone locals, blend them per bone (hkaSkeletonUtils::blendPoses math — `w` = weight of
/// sample B), strip root locomotion, then compose. Used by the world scene during clip switches.
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
    for b in 0..rig.len() {
        if rig[b].parent < 0 {
            let m = rig[b].local_bind;
            local[b].translation = [m[3][0], m[3][1], m[3][2]];
        }
    }
    skin_palette(rig, &model_poses(rig, &local))
}

/// hkaSkeletonUtils::blendPoses per-bone math: translation/scale lerp, rotation nlerp with the
/// hemisphere fix (negate b's quaternion if dot(a,b) < 0, then lerp + renormalize). `w` = weight
/// of `b` (0 = pure a, 1 = pure b).
fn qs_blend(a: &QsTransform, b: &QsTransform, w: f32) -> QsTransform {
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

/// sampleAndCombine into per-bone local hkQsTransforms (shared by the palette entry points above).
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
                // Full sampled hkQsTransform overwrites the bind pose (the sample carries the real
                // bone offsets in T; only the quaternion needs renormalizing after interpolation).
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

#[cfg(test)]
mod weapon_attach_tests {
    use super::*;

    fn ident() -> [[f32; 4]; 4] {
        [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]]
    }
    fn translate(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
        [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [x, y, z, 1.]]
    }
    fn bone(parent: i32, local: [[f32; 4]; 4]) -> BoneRig {
        BoneRig { parent, name_hash: 0, world_bind: ident(), inv_bind: ident(), local_bind: local }
    }

    /// `bone_model_matrix` composes a child bone's model-space position from the parent chain: at bind
    /// pose (no clip tracks), a root→child rig with the child offset (1,2,3) puts the child's model
    /// matrix translation at (1,2,3) — the anchor a held weapon rides. This is the weapon-attach core.
    #[test]
    fn hand_bone_model_matrix_is_the_chain_position() {
        let rig = vec![bone(-1, ident()), bone(0, translate(1.0, 2.0, 3.0))];
        let m = bone_model_matrix(&rig, &[], &[], 0, 1);
        // Row-vector convention: translation is row 3.
        assert!((m[3][0] - 1.0).abs() < 1e-4, "x = {}", m[3][0]);
        assert!((m[3][1] - 2.0).abs() < 1e-4, "y = {}", m[3][1]);
        assert!((m[3][2] - 3.0).abs() < 1e-4, "z = {}", m[3][2]);
        // A grandchild stacks onto the parent: (1,2,3) + (0,1,0) = (1,3,3).
        let rig2 = vec![bone(-1, ident()), bone(0, translate(1.0, 2.0, 3.0)), bone(1, translate(0.0, 1.0, 0.0))];
        let g = bone_model_matrix(&rig2, &[], &[], 0, 2);
        assert!((g[3][0] - 1.0).abs() < 1e-4 && (g[3][1] - 3.0).abs() < 1e-4 && (g[3][2] - 3.0).abs() < 1e-4);
        // Out-of-range bone → identity (no translation).
        let oob = bone_model_matrix(&rig, &[], &[], 0, 99);
        assert_eq!(oob, ident());
    }
}
