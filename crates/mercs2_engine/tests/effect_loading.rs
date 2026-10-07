//! Effects load from the real `vz.wad` through `game_world::load_effect`.
//!
//! The loader used to pick the effects block as "the first block path containing `effect`", which is
//! `text_effect_P000_Q3` (block 3117), not `effects_P000_Q3` (block 3459). Every lookup came back
//! empty and every god-ray glow card silently used its fallback constants. It now finds the block
//! through the effect's own ASET row; these tests prove real effects arrive.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.

use mercs2_engine::game_world::{glow_card_for_effect, load_effect};
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;

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
