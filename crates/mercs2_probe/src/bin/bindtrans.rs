//! Do animation clips carry BONE LENGTHS, or only rotations?
//!
//! This decides whether a character can be a different SIZE. Every one of the game's 127 humanoid
//! models shares one bind pose to the millimetre, so retail never exercised a re-proportioned rig and
//! no shipped asset can tell us what the engine would do with one. Two readings of
//! `Skin[b] = InvBind[b] · WorldPose[b]` are indistinguishable on retail data:
//!
//!   (A) `WorldPose` composes the clip's ROTATION onto the model's OWN `local_bind` translation.
//!       Bone lengths come from the model -> a taller HIER animates correctly and IS taller.
//!       This is what `mercs2_engine::pose::animate_locals` implements (it overwrites `m[3]`).
//!   (B) `WorldPose` uses the clip's full 48-byte `hkQsTransform`, translation included.
//!       Bone lengths come from the CLIP -> a taller HIER is dragged back to stock proportions,
//!       and because `InvBind` still comes from the model, the mesh TEARS toward the stock skeleton.
//!
//! The discriminator is the clip's own translation track, measured against the rig's bind offsets:
//!
//!   * translations ~= the bind offsets  -> (A) and (B) agree on retail, and (B) would OVERRIDE a
//!     re-proportioned rig. Variable body size is then NOT safe without an in-game test.
//!   * translations ~= 0 while bind offsets are large -> (B) would collapse every character onto the
//!     root, which plainly does not happen, so the engine must be (A). Variable size is SAFE.
//!   * translations VARY over the clip -> they are real motion (squash/stretch/root slide) and
//!     overwriting them, as (A) does, silently discards animation.
//!
//! Reports per clip, in millimetres, plus a verdict. Read-only; touches no game process.
//!
//!   bindtrans [0xMODELHASH] [--clips N]

use mercs2_engine::{game_world, model::Model, wad};

/// Translation of a row-major, row-vector 4x4 (the convention `skeleton.rs::transform_point` uses).
fn trans(m: &[[f32; 4]; 4]) -> [f32; 3] {
    [m[3][0], m[3][1], m[3][2]]
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn len(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhash = args
        .get(1)
        .and_then(|a| a.strip_prefix("0x"))
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or(0x0BBA_3066); // pmc_hum_mattias
    let max_clips: usize = args
        .iter()
        .position(|a| a == "--clips")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);

    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let m = Model::load(&mut w, mhash).expect("load model");
    let (_v, _i, _d, stats) = m.flatten();
    let rig = &stats.rig;
    println!("model 0x{mhash:08X}: {} bones in rig", rig.len());

    // How big ARE the bind offsets? If the clip translations were zero, this is the scale of the
    // error (B) would introduce -- i.e. what "collapse onto the root" would cost.
    let mut bind_len: Vec<f32> = rig.iter().filter(|b| b.parent >= 0).map(|b| len(trans(&b.local_bind))).collect();
    bind_len.sort_by(|a, b| a.total_cmp(b));
    let med_bind = bind_len.get(bind_len.len() / 2).copied().unwrap_or(0.0);
    println!(
        "bind LOCAL offsets: median {:.1} mm, max {:.1} mm over {} non-root bones",
        med_bind * 1000.0,
        bind_len.last().copied().unwrap_or(0.0) * 1000.0,
        bind_len.len()
    );

    let clips = game_world::load_clips_for_model(&mut w, rig);
    println!("{} clip(s) bind this rig\n", clips.len());

    println!("{:<12} {:>7} {:>12} {:>12} {:>12} {:>10}", "clip", "tracks", "|clip-bind|", "|clip| mean", "over time", "verdict");
    println!("{:-<70}", "");

    let (mut agree, mut zero, mut other, mut moving) = (0usize, 0usize, 0usize, 0usize);
    for c in clips.iter().filter(|c| c.clip.decoded).take(max_clips) {
        // Sample across the clip so a translation that MOVES is distinguishable from a constant one.
        let ts = [0.0f32, 0.25, 0.5, 0.75];
        let samples: Vec<Vec<mercs2_formats::anim::QsTransform>> =
            ts.iter().map(|f| c.clip.sample_local(c.clip.duration * f)).collect();

        let (mut d_sum, mut t_sum, mut n) = (0.0f32, 0.0f32, 0usize);
        let mut drift_max = 0.0f32;
        // Per-bone drift, not just the max: a locomotion clip legitimately translates the HIPS, and
        // one such bone would otherwise masquerade as "every bone changes length".
        let mut movers: Vec<(f32, u32)> = Vec::new();
        for (track, bone) in c.track_to_hier.iter().enumerate().take(c.num_transform_tracks) {
            let Some(&b) = bone.as_ref() else { continue };
            if b >= rig.len() || rig[b].parent < 0 {
                continue; // the root's translation IS world motion, not a bone length
            }
            let bind = trans(&rig[b].local_bind);
            let Some(q0) = samples[0].get(track) else { continue };
            d_sum += dist(q0.translation, bind);
            t_sum += len(q0.translation);
            n += 1;
            let mut d_bone = 0.0f32;
            for s in &samples[1..] {
                if let Some(q) = s.get(track) {
                    d_bone = d_bone.max(dist(q.translation, q0.translation));
                }
            }
            drift_max = drift_max.max(d_bone);
            movers.push((d_bone, rig[b].name_hash));
        }
        let n_moving = movers.iter().filter(|(d, _)| *d > 0.001).count();
        movers.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top: Vec<String> =
            movers.iter().take(3).map(|(d, h)| format!("0x{h:08X}:{:.0}mm", d * 1000.0)).collect();
        println!(
            "   bones translating >1mm during clip: {n_moving}/{n}   top: {}",
            top.join("  ")
        );
        if n == 0 {
            continue;
        }
        let (d_mean, t_mean) = (d_sum / n as f32, t_sum / n as f32);
        // Thresholds in metres: 1 mm is well under any authored bone length here.
        let verdict = if d_mean < 0.001 {
            agree += 1;
            "== bind"
        } else if t_mean < 0.001 {
            zero += 1;
            "~= zero"
        } else {
            other += 1;
            "OTHER"
        };
        if drift_max > 0.001 {
            moving += 1;
        }
        println!(
            "0x{:08X} {:>7} {:>9.2} mm {:>9.2} mm {:>9.2} mm {:>10}",
            c.name_hash,
            n,
            d_mean * 1000.0,
            t_mean * 1000.0,
            drift_max * 1000.0,
            verdict
        );
    }

    println!("\n-- verdict --");
    println!("clips whose translations EQUAL the bind offsets : {agree}");
    println!("clips whose translations are ~ZERO             : {zero}");
    println!("clips with some OTHER translation              : {other}");
    println!("clips whose translations MOVE during the clip  : {moving}");
    if zero > 0 && agree == 0 {
        println!("\n=> Clips carry NO bone lengths. Reading (B) would collapse every bone onto its");
        println!("   parent, which the shipping game plainly does not do, so the engine must take");
        println!("   lengths from the model's own HIER. A re-proportioned bind is SAFE.");
    } else if agree > 0 && moving == 0 {
        println!("\n=> Clips carry CONSTANT translations equal to the stock bind offsets. (A) and (B)");
        println!("   are indistinguishable on retail data, and under (B) a re-proportioned rig would");
        println!("   be dragged back to stock while its InvBind stayed custom -- i.e. a TEAR.");
        println!("   Variable body size is NOT proven safe here; settle it in-game before shipping.");
    } else if moving > 0 {
        println!("\n=> Some translations MOVE during the clip: that is real authored motion, and");
        println!("   `animate_locals` overwriting m[3] DISCARDS it. Worth a second look regardless");
        println!("   of the sizing question.");
    }
}
