//! Dev bin: how many LightObject dynamic lights do we actually harvest, and how many are near the PMC
//! villa spawn? Diagnoses "no interior lights showing".
//!   cargo run -p mercs2_engine --bin light_probe

use mercs2_engine::{wad, worldutil};
use mercs2_formats::placement::light_inventory;

const SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

fn d2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

fn dump(tag: &str, block: &[u8]) {
    let lights = light_inventory(block);
    let near = lights.iter().filter(|l| d2(l.pos, SPAWN) < 200.0 * 200.0).count();
    println!("[{tag}] {} LightObject lights ({near} within 200 m of the villa spawn)", lights.len());
    for l in lights.iter().filter(|l| d2(l.pos, SPAWN) < 200.0 * 200.0).take(10) {
        println!(
            "   {:<28} pos [{:8.1},{:7.1},{:8.1}] type {} color ({:.2},{:.2},{:.2}) params {:?}",
            l.name.clone().unwrap_or_default(),
            l.pos[0], l.pos[1], l.pos[2], l.light.light_type,
            l.light.color[0], l.light.color[1], l.light.color[2],
            &l.light.params[..4]
        );
    }
}

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let (_low, ls) = worldutil::find_terrain_blocks(&mut w).expect("layers_static");
    dump("layers_static", &ls);
    for blk in [667u16, 461, 291, 703, 711] {
        match wad::decompress_block_index(&mut w, blk) {
            Ok(dec) => dump(&format!("block {blk}"), &dec),
            Err(e) => println!("[block {blk}] decompress failed: {e}"),
        }
    }

    // Block-667 named furniture: does each name resolve to a mesh the way load_pmc_interior does
    // (pandemic_hash_m2(base name) -> extract_container)? A miss = a silently-skipped "missing" prop.
    if let Ok(dec) = wad::decompress_block_index(&mut w, 667) {
        let pls = mercs2_formats::placement::load_placements(&dec).unwrap_or_default();
        let mut seen: std::collections::BTreeSet<String> = Default::default();
        println!("== block 667: name -> pandemic_hash_m2 -> container resolution ==");
        for p in &pls {
            let Some(raw) = p.name.as_deref() else { continue };
            let base = raw.split(" 0x").next().unwrap_or(raw).trim_start_matches('_');
            if base.is_empty() || !seen.insert(base.to_string()) {
                continue;
            }
            let h = mercs2_formats::hash::pandemic_hash_m2(base);
            let status = match wad::extract_container(&mut w, h) {
                Ok(c) => match mercs2_engine::mesh::build_indexed_from_container(&c) {
                    Ok((v, _, d, _)) => format!("OK {} verts / {} draws", v.len(), d.len()),
                    Err(e) => format!("BUILD FAIL: {e}"),
                },
                Err(_) => "MISS (no container)".to_string(),
            };
            println!("   {base:<34} 0x{h:08X} -> {status}");
        }
    }

    // Baked-lighting check: does the PMC hall carry non-white vertex COLORS (static lightmap baked to
    // verts, the Pandemic-era interior lighting) — or flat white (no baked light)?
    for (name, hash) in [
        ("PMC hall", 0x39AF17DCu32),
        ("lamppostmilitary", 0xA6AD0346),
        ("wardrobe", 0x8AAA90D1),
        ("Mattias(player)", 0xA3C1FABC),
    ] {
        if let Some((m, _, _)) = mercs2_engine::game_world::load_model_by_hash_state(&mut w, hash, 0x01) {
            let n = m.verts.len().max(1) as f32;
            let mut cmn = [1.0f32; 3];
            let mut cmx = [0.0f32; 3];
            let mut sum = [0.0f32; 3];
            let mut nonwhite = 0usize;
            // NATIVE model-space bbox (positions are pre-fit): height/footprint in game units.
            let mut pmn = [f32::MAX; 3];
            let mut pmx = [f32::MIN; 3];
            for v in &m.verts {
                for c in 0..3 {
                    cmn[c] = cmn[c].min(v.color[c]);
                    cmx[c] = cmx[c].max(v.color[c]);
                    sum[c] += v.color[c];
                    pmn[c] = pmn[c].min(v.pos[c]);
                    pmx[c] = pmx[c].max(v.pos[c]);
                }
                if v.color.iter().any(|&c| c < 0.95) {
                    nonwhite += 1;
                }
            }
            println!(
                "[{name} 0x{hash:08X}] {} verts  bbox X {:.2} Y {:.2} Z {:.2} (native units)  color mean({:.2},{:.2},{:.2}) {} non-white ({:.0}%)",
                m.verts.len(), pmx[0] - pmn[0], pmx[1] - pmn[1], pmx[2] - pmn[2],
                sum[0] / n, sum[1] / n, sum[2] / n, nonwhite, 100.0 * nonwhite as f32 / n
            );
        }
    }

    // Locomotion: derive the player's walk/run GROUND SPEED from each clip's baked root stride
    // (what world.rs now uses instead of the hardcoded 2.2/6.5 m/s).
    if let Ok(container) = wad::extract_container(&mut w, 0xA3C1FABC) {
        if let Ok((_v, _i, _d, stats)) = mercs2_engine::mesh::build_indexed_from_container(&container) {
            let hier: Vec<u32> = stats.rig.iter().map(|b| b.name_hash).collect();
            let wanted = [0x24F8_C8E6u32, 0x5368_2784, 0x867B_166D];
            let names = ["idle", "walk", "run"];
            for (found, (h, nm)) in mercs2_engine::game_world::load_clips_for_rig(&mut w, &hier, &wanted)
                .into_iter()
                .zip(wanted.iter().zip(names))
            {
                if let Some(ca) = found {
                    let d = ca.clip.duration.max(1e-3);
                    let sp = mercs2_engine::pose::clip_root_speed(
                        &stats.rig,
                        &ca.clip.sample_local(0.0),
                        &ca.clip.sample_local(d * 0.999),
                        &ca.track_to_hier,
                        ca.num_transform_tracks,
                        d * 0.999,
                    );
                    println!("[locomotion] {nm} 0x{h:08X}: dur {d:.2}s -> {sp:.2} m/s (stride {:.2} m)", sp * d);
                }
            }
        }
    }
}
