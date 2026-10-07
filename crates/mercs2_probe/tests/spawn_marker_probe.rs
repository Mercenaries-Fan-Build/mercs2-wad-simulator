//! Probe: what spawn markers actually exist in the shipped `vz.wad`, and which of the hero-
//! spawn candidates resolve. Verifies (against real data) why the hero lands where it does. Mirrors the
//! game's resolution: `load_placements(layers_static)` → lowercased name→pos → the candidate lookup the
//! world loop + boot Lua flow use.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.
//!
//! ```text
//! cargo test -p mercs2_probe --features retail --test spawn_marker_probe -- --nocapture
//! ```

use mercs2_engine::wad;
use mercs2_engine::worldutil::find_terrain_blocks;

/// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else. Panics with the
/// resolver's message when it is missing, and when the path is not UTF-8 (`wad::open` takes `&str`).
fn vz_wad_path() -> String {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    path.to_str()
        .unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display()))
        .to_string()
}

#[test]
fn spawn_markers_present_in_vz_wad() {
    let path = vz_wad_path();
    let mut w = wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let (_low, ls) = find_terrain_blocks(&mut w).expect("find layers_static");
    let placements = mercs2_formats::placement::load_placements(&ls).expect("load placements");

    // The named-marker table the game builds (lowercased name → world pos).
    let named: std::collections::HashMap<String, [f32; 3]> = placements
        .iter()
        .filter_map(|p| p.name.as_ref().map(|n| (n.to_ascii_lowercase(), p.pos)))
        .collect();
    println!(
        "{} placements, {} named markers",
        placements.len(),
        named.len()
    );

    // The exact candidates the engine shortcut + boot Lua flow try.
    for cand in ["pmccon001_start1", "pmc_entry1", "pmc_start1"] {
        match named.get(cand) {
            Some(p) => println!("  CANDIDATE '{cand}' -> ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]),
            None => println!("  CANDIDATE '{cand}' -> MISSING"),
        }
    }

    // Regression guard for the `parse_name_records` flag-byte fix: the contract/HQ start locators live
    // in layers_static (block 29) but were dropped by the per-record `0x01` flag mis-parse. They must
    // resolve now — this is what the spawn resolution binds to (data-driven, no hardcoded coords).
    assert!(
        named.contains_key("pmccon001_start1"),
        "PmcCon001_Start1 must resolve from block-29 data (parse_name_records flag-byte fix)"
    );
    assert!(named.contains_key("pmc_entry1"), "Pmc_Entry1 (HQ entrance) must resolve from block-29 data");

    // All spawn-ish marker names, so we can see what the real names look like.
    let mut spawnish: Vec<(&String, &[f32; 3])> = named
        .iter()
        .filter(|(k, _)| {
            k.contains("start") || k.contains("entry") || k.contains("spawn") || k.starts_with("pmc")
        })
        .map(|(k, v)| (k, v))
        .collect();
    spawnish.sort_by(|a, b| a.0.cmp(b.0));
    println!("\nspawn-ish markers ({}):", spawnish.len());
    for (name, p) in spawnish.iter().take(60) {
        println!("  {name:40} ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]);
    }
}
