//! Ignored probe: verify the resident audio pipeline against the real installed `vz.wad` — the one
//! part of the audio last-mile that can't be proven headlessly. Confirms `extract_container_typed` by
//! `m2(name)` → `data` chunk → `AudioEngine::load_wavebank` decodes real clips, and that the per-bank
//! `sounddb` catalog routes real cues to those decoded waves and mixes them to audible PCM.
//!
//! ```text
//! cargo test -p mercs2_game --test audio_wad_probe -- --ignored --nocapture
//! ```

use mercs2_engine::audio::{AudioEngine, SoundDb};
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::types::TYPE_HASH_WAVEBANK;

/// `sounddb` asset type (`0xE5273C14`, ASET type_id 13).
const SOUNDDB_TYPE: u32 = 0xE527_3C14;

/// The always-resident gameplay/UI/ambience wavebanks (`MrxSoundBootstrap.LoadBanks`).
const RESIDENT_WAVEBANKS: &[&str] = &[
    "ui_hud", "ui_shell", "wpn_shared", "veh_shared", "veh_support", "ambience", "amb_birds",
    "amb_shared", "collision_shared", "destruction_shared", "fol_shared", "music",
];

/// Pull a `data`-chunk body for `name` of `type_hash` from the WAD (or the raw container for sounddb,
/// whose body may be the container itself).
fn bank_body(w: &mut wad::Wad, name: &str, type_hash: u32, raw_ok: bool) -> Option<Vec<u8>> {
    let c = wad::extract_container_typed(w, m2(name), type_hash).ok()?;
    match mercs2_formats::ucfx::extract_chunk_body(&c, b"data") {
        Some(b) => Some(b),
        None if raw_ok => Some(c),
        None => None,
    }
}

#[test]
#[ignore]
fn resident_audio_extracts_decodes_and_routes_from_vz_wad() {
    let path = wad::registry_vz_wad().expect("vz.wad resolvable via EA registry");
    let mut w = wad::open(&path).expect("open vz.wad");

    // Load every resident wavebank into one engine + merge every per-bank sounddb into one catalog —
    // exactly what the game does at world-load.
    let mut eng = AudioEngine::default();
    let mut catalog = SoundDb::default();
    let mut found_banks = 0usize;
    for name in RESIDENT_WAVEBANKS {
        if let Some(body) = bank_body(&mut w, name, TYPE_HASH_WAVEBANK, false) {
            let audible = eng.load_wavebank(&body);
            found_banks += 1;
            println!("wavebank {name}: {} bytes -> {audible} audible clips", body.len());
        } else {
            println!("wavebank {name}: NOT FOUND");
        }
        if let Some(body) = bank_body(&mut w, name, SOUNDDB_TYPE, true) {
            if let Ok(db) = SoundDb::parse(&body) {
                println!("  sounddb {name}: {} cues (self 0x{:08X})", db.cues.len(), db.self_hash);
                catalog.merge(&db);
            }
        }
    }
    assert!(found_banks > 0, "no resident wavebank resolved from the WAD by name");

    let resolvable = catalog.cues.iter().filter(|c| eng.resolve_wave(c).is_some()).count();
    println!(
        "\nEND-TO-END: {} resident clips, {} cues, {resolvable} resolve to a decoded wave",
        eng.resident_wave_count(),
        catalog.cues.len()
    );
    assert!(resolvable > 0, "no cue routed to a resident decoded wave");

    // Play the first resolvable cue through the real mixer path; assert it produced audible PCM.
    eng.set_sounddb(catalog.clone());
    let cue = catalog
        .cues
        .iter()
        .find(|c| eng.resolve_wave(c).is_some())
        .expect("a resolvable cue");
    eng.cue_sound(cue.guid, None, None).expect("cue allocates a voice");
    for _ in 0..8 {
        eng.tick(0.02);
    }
    let rms = mercs2_engine::audio::mixer::rms_i16(&eng.render(4096));
    println!("cue 0x{:08X} -> wave -> mix RMS {rms:.1}", cue.guid);
    assert!(rms > 0.0, "a real cue's resident wave mixed to audible PCM");
}
