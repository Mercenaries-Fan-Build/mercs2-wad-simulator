//! Effects load from the real `vz.wad` through `game_world::load_effect`.
//!
//! The loader used to pick the effects block as "the first block path containing `effect`", which is
//! `text_effect_P000_Q3` (block 3117), not `effects_P000_Q3` (block 3459). Every lookup came back
//! empty and every god-ray glow card silently used its fallback constants. It now finds the block
//! through the effect's own ASET row; these tests prove real effects arrive.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.

use glam::{Quat, Vec3};
use mercs2_engine::fx_sim::{EffectSim, Starter};
use mercs2_engine::game_world::{glow_card_for_effect, load_effect, load_fx_atlas};
use mercs2_engine::wad;
use mercs2_formats::fxdict::FxRect;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::types::TYPE_ID_EFFECT;
use std::collections::{BTreeSet, HashMap};

/// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else. Panics with the
/// resolver's message when it is missing, and when the path is not UTF-8 (`wad::open` takes `&str`).
fn vz_wad_path() -> String {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    path.to_str()
        .unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display()))
        .to_string()
}

/// The retail `vz.wad`, opened. Panics when it cannot be found or opened.
fn open_vz_wad() -> wad::Wad {
    let path = vz_wad_path();
    wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"))
}

#[test]
fn named_effects_load_and_a_non_effect_does_not() {
    let mut w = open_vz_wad();
    for name in ["global_env_godray2", "global_explosion_c4"] {
        let fx = load_effect(&mut w, pandemic_hash_m2(name))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name} did not load"));
        assert!(!fx.emitters.is_empty(), "{name} has emitters");
    }
    // A model hash is not an effect: no effect ASET row, so `None`, not an error.
    assert!(load_effect(&mut w, 0x9FCA_E910).expect("lookup").is_none());
}

/// The PMC god-ray card takes its tint from the effect's COLR. The retail COLR's brightest key is
/// three equal colour bytes (`3f 3f 3f`), so the card's RGB channels are equal; the fallback tint
/// (0.25, 0.24, 0.20) never is.
#[test]
fn the_godray_glow_card_uses_the_real_colr() {
    let mut w = open_vz_wad();
    let fx = load_effect(&mut w, pandemic_hash_m2("global_env_godray2")).expect("parse").expect("godray loads");
    let colr = &fx.emitters[0].particle.colr;
    let lum = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
    let mut peak = colr.keys[0].rgba;
    for k in &colr.keys[1..] {
        if lum(k.rgba) > lum(peak) {
            peak = k.rgba;
        }
    }
    assert_eq!(&peak[..3], &[0x3f, 0x3f, 0x3f], "retail god-ray peak colour");

    let card = glow_card_for_effect(&mut w, "global_particle_env_godray2", [0.0; 3]).expect("glow card");
    let alpha = peak[3] as f32 / 255.0;
    let boost = (alpha * 4.0 + 0.6).clamp(0.8, 2.5);
    let want = (0x3f as f32 / 255.0 * boost).min(1.5);
    for c in &card.color[..3] {
        assert!((c - want).abs() < 1e-6, "card rgb {:?} != COLR-derived {want}", &card.color[..3]);
    }
    assert!((card.color[3] - (alpha * 3.5).clamp(0.25, 0.85)).abs() < 1e-6);
}

/// Every retail effect runs: the sim takes each as it ships, its masses and curves included, and two
/// seconds of it give no non-finite sprite. Reports the emitter census.
#[test]
fn every_retail_effect_runs_in_the_sim() {
    let mut w = open_vz_wad();
    let (atlas, rects) = load_fx_atlas(&mut w).expect("the atlas");
    assert!(atlas.width >= 1024 && rects.len() > 500, "{} rects", rects.len());
    let rects: HashMap<u32, FxRect> = rects.into_iter().map(|r| (r.key, r)).collect();
    let hashes: BTreeSet<u32> = wad::all_asets(&w).into_iter().filter(|(_, ty, _)| *ty == TYPE_ID_EFFECT).map(|(h, _, _)| h).collect();
    let mut peak = 0usize;
    for h in &hashes {
        let fx = load_effect(&mut w, *h).expect("parse").expect("an effect");
        let mut sim = EffectSim::new(&fx, &rects, Starter::default(), Vec3::ZERO, Quat::IDENTITY, 7).unwrap_or_else(|e| panic!("0x{h:08X}: {e}"));
        for _ in 0..120 {
            sim.step(1.0 / 60.0);
            peak = peak.max(sim.live_particles());
            sim.sprites(Vec3::X, Vec3::Y, Vec3::Z, |s| {
                let finite = s.pos.iter().chain(s.color.iter()).chain(s.rect.iter()).all(|v| v.is_finite())
                    && s.size.is_finite() && s.sin.is_finite() && s.cos.is_finite() && s.intensity.is_finite();
                assert!(finite, "0x{h:08X}: {s:?}");
            });
        }
    }
    println!("[fx] {} effects ran; the most live particles in one: {peak}", hashes.len());
    assert_eq!(hashes.len(), 314);
}

/// `0x02D849BD` (`rate` 187.5, `life` 1.9 ± 0.2): the capture holds about `rate · life` of it per
/// instance, as the sim settles to.
#[test]
fn a_steady_effect_settles_where_the_captures_hold_it() {
    let mut w = open_vz_wad();
    let (_, rects) = load_fx_atlas(&mut w).expect("the atlas");
    let rects: HashMap<u32, FxRect> = rects.into_iter().map(|r| (r.key, r)).collect();
    let fx = load_effect(&mut w, 0x02D8_49BD).expect("parse").expect("the effect");
    let mut sim = EffectSim::new(&fx, &rects, Starter::default(), Vec3::ZERO, Quat::IDENTITY, 7).expect("runs");
    for _ in 0..600 {
        sim.step(1.0 / 60.0);
    }
    // Each emitter holds its `rate · life`.
    let held = |e: &mercs2_formats::fxdict::Emitter, h: u32| {
        let a = e.particle.attributes.iter().find(|a| a.hash == h).expect("the attribute");
        let last = a.curve.as_ref().and_then(|k| k.last()).map(|k| k.value);
        match a.value {
            mercs2_formats::fxdict::AtrbValue::F32(v) => last.unwrap_or(v),
            mercs2_formats::fxdict::AtrbValue::U32(v) => last.unwrap_or(v as f32),
        }
    };
    let (rate, life) = (0x062F_0D37, 0xD3AE_67AF);
    let parts: Vec<(f32, f32)> = fx.emitters.iter().map(|e| (held(e, rate), held(e, life))).collect();
    println!("[fx] 0x02D849BD emitters (rate, life): {parts:?}");
    let want: f32 = parts.iter().map(|(r, l)| r * l).sum();
    let got = sim.live_particles() as f32;
    assert!((got - want).abs() / want < 0.05, "{got} live, rate · life {want}");
}
