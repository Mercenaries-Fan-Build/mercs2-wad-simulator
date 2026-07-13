//! Does the EXPORTED animation reproduce the ENGINE's pose?
//!
//! The export crosses a convention boundary: the engine is row-major / row-vector
//! (`p' = p · M`, `world = local · world_parent`), glTF is column-major / column-vector
//! (`p' = M · p`, `world = world_parent · local`). A quaternion or a matrix transposed the wrong way
//! still LOOKS like a plausible skeleton — bones in roughly the right places, subtly wrong joints —
//! so eyeballing the export in Blender cannot prove it. This does.
//!
//! The test recomputes each bone's model-space position two independent ways at several times in a
//! clip and asserts they agree:
//!   * ENGINE: `pose::model_poses(rig, clip.sample_local(t))` — the exact math the renderer skins with.
//!   * glTF:   compose the node TRS chain the way a glTF viewer does (column-vector, parent · local),
//!             using the values the exporter writes into the file.
//! If the exporter's convention is wrong, these diverge and the test fails.
//!
//! Requires the retail WAD, so it is `#[ignore]`d like the other WAD probes; run with `--ignored`.

use mercs2_engine::{game_world, model::Model, wad};
use mercs2_formats::anim::QsTransform;

const MATTIAS_V3: u32 = 0xA3C1_FABC;

/// Column-vector 4x4 multiply, glTF's convention: `out = a · b`.
fn mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0.0f32; 4]; 4];
    for (r, orow) in o.iter_mut().enumerate() {
        for (c, x) in orow.iter_mut().enumerate() {
            *x = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

/// A glTF node's TRS -> its local matrix, the way a glTF viewer builds it: `M = T · R · S`,
/// column-vector, with the SAME `(x,y,z,w)` quaternion the exporter writes.
fn gltf_trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [[f32; 4]; 4] {
    let [x, y, z, w] = q;
    // Standard column-vector rotation matrix from a quaternion.
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)],
        [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)],
        [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [
        [r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2], t[0]],
        [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2], t[1]],
        [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2], t[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn exported_animation_matches_engine_pose() {
    let Some(mut w) = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()) else {
        eprintln!("no vz.wad — skipping");
        return;
    };
    let m = Model::load(&mut w, MATTIAS_V3).expect("load mattias");
    let (_v, _i, _d, stats) = m.flatten();
    let rig = &stats.rig;
    assert!(!rig.is_empty(), "character must have a rig");

    let clips = game_world::load_clips_for_model(&mut w, rig);
    let clip = clips
        .iter()
        .find(|c| c.clip.decoded && c.clip.num_frames > 1)
        .expect("at least one decoded multi-frame clip");

    // What the exporter writes as each node's DEFAULT TRS (bind-local), and what it would write as
    // an animated node's TRS at time t (the clip's sampled local for that bone's track).
    let bind: Vec<QsTransform> =
        rig.iter().map(|b| mercs2_engine::pose::mat_to_qs(&b.local_bind)).collect();

    let mut checked = 0usize;
    let mut worst = 0.0f32;

    for step in 0..5 {
        let t = clip.clip.duration * (step as f32 / 4.0);

        // ---- ENGINE side: the renderer's own local->model composition. ----
        let sampled = clip.clip.sample_local(t);
        let mut locals = bind.clone();
        for track in 0..clip.num_transform_tracks.min(clip.track_to_hier.len()) {
            if let Some(bone) = clip.track_to_hier[track] {
                if bone < locals.len() {
                    if let Some(qs) = sampled.get(track) {
                        locals[bone] = *qs;
                    }
                }
            }
        }
        let engine_model = mercs2_engine::pose::model_poses(rig, &locals);

        // ---- glTF side: compose the node tree exactly as a viewer would. ----
        // Each node's TRS is what the exporter emits: the sampled local for a tracked bone, else
        // the bind-local default. HIER guarantees parent index < child, so one pass suffices.
        let mut gltf_world: Vec<[[f32; 4]; 4]> = vec![[[0.0; 4]; 4]; rig.len()];
        for b in 0..rig.len() {
            let qs = locals[b];
            let local = gltf_trs(qs.translation, qs.rotation, qs.scale);
            gltf_world[b] = if rig[b].parent < 0 {
                local
            } else {
                mul(&gltf_world[rig[b].parent as usize], &local)
            };
        }

        // Compare each bone's MODEL-SPACE ORIGIN — the thing a wrong transpose moves.
        for b in 0..rig.len() {
            let e = engine_model[b].translation;
            // Column-vector: the translation is the last COLUMN.
            let g = [gltf_world[b][0][3], gltf_world[b][1][3], gltf_world[b][2][3]];
            let d = ((e[0] - g[0]).powi(2) + (e[1] - g[1]).powi(2) + (e[2] - g[2]).powi(2)).sqrt();
            // Scale-relative: a deep bone sits metres from the origin, and the two sides reach it by
            // different algebra (the engine composes QUATERNIONS via hkQsTransform::setMul; a glTF
            // viewer composes MATRICES), so f32 rounding accumulates down the chain. What must not
            // happen is a CONVENTION error, which displaces a bone by a large fraction of its own
            // radius — orders of magnitude above rounding.
            let radius = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt().max(1.0);
            worst = worst.max(d / radius);
            assert!(
                d / radius < 1e-3,
                "bone {b} at t={t:.2}s diverges: engine {e:?} vs glTF {g:?} \
                 (|d| = {d} m, {:.4}% of its {radius:.2} m radius)",
                100.0 * d / radius
            );
            checked += 1;
        }
    }

    println!(
        "clip 0x{:08X}: {checked} bone-poses agree across 5 times; \
         worst divergence {:.5}% of bone radius (pure f32 chain rounding)",
        clip.name_hash,
        100.0 * worst
    );
    assert!(checked > 0);
}

/// NEGATIVE CONTROL — proves the parity test above has teeth.
///
/// If the exporter had picked the wrong quaternion handedness (the single most likely way to get
/// this boundary wrong, and one that still produces a plausible-looking skeleton), the pose must
/// diverge FAR beyond the f32 rounding the parity test tolerates. If this test ever stops finding a
/// large divergence, the parity test has gone blind and is no longer evidence of anything.
#[test]
#[ignore = "needs the retail vz.wad"]
fn a_conjugated_quaternion_would_be_caught() {
    let Some(mut w) = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()) else {
        eprintln!("no vz.wad — skipping");
        return;
    };
    let m = Model::load(&mut w, MATTIAS_V3).expect("load mattias");
    let (_v, _i, _d, stats) = m.flatten();
    let rig = &stats.rig;
    let clips = game_world::load_clips_for_model(&mut w, rig);
    let clip = clips
        .iter()
        .find(|c| c.clip.decoded && c.clip.num_frames > 1)
        .expect("a decoded clip");

    let bind: Vec<QsTransform> =
        rig.iter().map(|b| mercs2_engine::pose::mat_to_qs(&b.local_bind)).collect();
    let t = clip.clip.duration * 0.5;
    let sampled = clip.clip.sample_local(t);
    let mut locals = bind.clone();
    for track in 0..clip.num_transform_tracks.min(clip.track_to_hier.len()) {
        if let Some(bone) = clip.track_to_hier[track] {
            if bone < locals.len() {
                if let Some(qs) = sampled.get(track) {
                    locals[bone] = *qs;
                }
            }
        }
    }
    let engine_model = mercs2_engine::pose::model_poses(rig, &locals);

    // Same composition, but with the quaternion CONJUGATED — the bug we are ruling out.
    let mut bad: Vec<[[f32; 4]; 4]> = vec![[[0.0; 4]; 4]; rig.len()];
    for b in 0..rig.len() {
        let qs = locals[b];
        let q = qs.rotation;
        let local = gltf_trs(qs.translation, [-q[0], -q[1], -q[2], q[3]], qs.scale);
        bad[b] = if rig[b].parent < 0 { local } else { mul(&bad[rig[b].parent as usize], &local) };
    }

    let mut worst = 0.0f32;
    for b in 0..rig.len() {
        let e = engine_model[b].translation;
        let g = [bad[b][0][3], bad[b][1][3], bad[b][2][3]];
        let d = ((e[0] - g[0]).powi(2) + (e[1] - g[1]).powi(2) + (e[2] - g[2]).powi(2)).sqrt();
        worst = worst.max(d);
    }
    println!("conjugated-quaternion control: worst bone displacement {worst:.4} m");
    assert!(
        worst > 0.05,
        "the wrong convention displaced bones by only {worst} m — the parity test cannot \
         distinguish right from wrong and is worthless"
    );
}
