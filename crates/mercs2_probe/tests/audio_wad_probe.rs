//! Probe: verify the resident audio pipeline against the real installed `vz.wad` — the one
//! part of the audio last-mile that can't be proven headlessly. Confirms `extract_container_typed` by
//! `m2(name)` → `data` chunk → `AudioEngine::load_wavebank` / `load_soundbank` load real banks, and
//! that the per-bank `sounddb` catalog routes real cues through their soundbank cue and group to
//! those decoded waves and mixes them to audible PCM.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.
//!
//! ```text
//! cargo test -p mercs2_probe --features retail --test audio_wad_probe -- --nocapture
//! ```

use mercs2_engine::audio::{AudioEngine, CueError, SoundDb};
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK};

/// `sounddb` asset type (`0xE5273C14`, ASET type_id 13).
const SOUNDDB_TYPE: u32 = 0xE527_3C14;

/// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else. Panics with the
/// resolver's message when it is missing, and when the path is not UTF-8 (`wad::open` takes `&str`).
fn vz_wad_path() -> String {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    path.to_str()
        .unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display()))
        .to_string()
}

/// The always-resident gameplay/UI/ambience wavebanks (`MrxSoundBootstrap.LoadBanks`).
const RESIDENT_WAVEBANKS: &[&str] = &[
    "ui_hud", "ui_shell", "wpn_shared", "veh_shared", "veh_support", "ambience", "amb_birds",
    "amb_shared", "collision_shared", "destruction_shared", "fol_shared", "music",
];

/// Every ASET `(asset_hash, type_id)` pair in the archive at `path`: a bank is present exactly when
/// its row is.
fn aset_rows(path: &str) -> std::collections::HashSet<(u32, u32)> {
    let mut f = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let size = f.metadata().unwrap_or_else(|e| panic!("stat {path}: {e}")).len();
    let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size)
        .unwrap_or_else(|e| panic!("read the FFCS tables of {path}: {e}"));
    arch.aset.iter().map(|a| (a.asset_hash, a.type_id)).collect()
}

/// True when the archive's ASET has a row for `name` of `type_hash`.
fn has_row(rows: &std::collections::HashSet<(u32, u32)>, name: &str, type_hash: u32) -> bool {
    let type_id = mercs2_formats::aset_type_ids::type_id_for_type_hash(type_hash)
        .unwrap_or_else(|| panic!("type hash 0x{type_hash:08X} has no ASET type_id"));
    rows.contains(&(m2(name), type_id))
}

/// The `data`-chunk body for `name` of `type_hash` from the WAD (or the raw container for sounddb,
/// whose body may be the container itself). Panics naming the bank when it cannot be extracted.
fn bank_body(w: &mut wad::Wad, name: &str, type_hash: u32, raw_ok: bool) -> Vec<u8> {
    let c = wad::extract_container_typed(w, m2(name), type_hash)
        .unwrap_or_else(|e| panic!("{name} (type 0x{type_hash:08X}): {e}"));
    match mercs2_formats::ucfx::extract_chunk_body(&c, b"data") {
        Some(b) => b,
        None if raw_ok => c,
        None => panic!("{name} (type 0x{type_hash:08X}): container has no `data` chunk"),
    }
}

#[test]
fn resident_audio_extracts_decodes_and_routes_from_vz_wad() {
    let path = vz_wad_path();
    let mut w = wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let rows = aset_rows(&path);

    // Load every resident wavebank into one engine + merge every per-bank sounddb into one catalog —
    // exactly what the game does at world-load.
    let mut eng = AudioEngine::default();
    let mut catalog = SoundDb::default();
    let mut found_banks = 0usize;
    for name in RESIDENT_WAVEBANKS {
        assert!(
            has_row(&rows, name, TYPE_HASH_WAVEBANK),
            "resident wavebank {name} (0x{:08X}) has no ASET row in {path}",
            m2(name)
        );
        let body = bank_body(&mut w, name, TYPE_HASH_WAVEBANK, false);
        let audible = eng
            .load_wavebank(&body)
            .unwrap_or_else(|e| panic!("wavebank {name}: {e}"));
        found_banks += 1;
        println!("wavebank {name}: {} bytes -> {audible} audible clips", body.len());
        // A wavebank's soundbank and sounddb are separate assets; each is loaded when its ASET row
        // exists (`amb_shared` ships only the wavebank).
        if has_row(&rows, name, TYPE_HASH_SOUNDBANK) {
            let body = bank_body(&mut w, name, TYPE_HASH_SOUNDBANK, false);
            let cues = eng
                .load_soundbank(&body)
                .unwrap_or_else(|e| panic!("soundbank {name}: {e}"));
            println!("  soundbank {name}: {cues} cues");
        }
        if has_row(&rows, name, SOUNDDB_TYPE) {
            let body = bank_body(&mut w, name, SOUNDDB_TYPE, true);
            let db = SoundDb::parse(&body).unwrap_or_else(|e| panic!("sounddb {name}: {e}"));
            println!("  sounddb {name}: {} cues (self 0x{:08X})", db.cues.len(), db.self_hash);
            catalog.merge(&db);
        }
    }
    assert_eq!(found_banks, RESIDENT_WAVEBANKS.len(), "every resident wavebank loads");

    let resolvable = catalog.cues.iter().filter(|c| eng.resolve_cue(c).is_ok()).count();
    println!(
        "\nEND-TO-END: {} resident clips, {} cues, {resolvable} resolve through every path to decoded PCM",
        eng.resident_wave_count(),
        catalog.cues.len()
    );
    assert!(resolvable > 0, "no cue routed to a resident decoded wave");

    // Play the first resolvable cue the engine starts through the real mixer path; assert it produced
    // audible PCM. Only a cue the engine refuses to play (an automation record it cannot evaluate, a
    // filter scan reaching past the cue's events, or such a child) is passed over.
    eng.set_sounddb(catalog.clone());
    let mut started = None;
    for c in &catalog.cues {
        if eng.resolve_cue(c).is_err() {
            continue;
        }
        match eng.cue_sound(c.guid, None) {
            Ok(_) => {
                started = Some(c);
                break;
            }
            Err(CueError::Automation(_) | CueError::FilterScan { .. } | CueError::Child { .. }) => {}
            Err(e) => panic!("cue 0x{:08X}: {e}", c.guid),
        }
    }
    let cue = started.expect("a resolvable cue the engine starts");
    for _ in 0..8 {
        eng.tick(0.02);
    }
    let rms = mercs2_engine::audio::mixer::rms_i16(&eng.render(4096));
    println!("cue 0x{:08X} -> wave -> mix RMS {rms:.1}", cue.guid);
    assert!(rms > 0.0, "a real cue's resident wave mixed to audible PCM");
}
