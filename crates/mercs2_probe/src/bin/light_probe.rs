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

    // Baked-lighting check: does the PMC hall carry non-white vertex COLORS (static lightmap baked to
    // verts, the Pandemic-era interior lighting) — or flat white (no baked light)?
    for (name, hash) in [("PMC hall", 0x39AF17DCu32), ("lamppostmilitary", 0xA6AD0346)] {
        if let Some((m, _, _)) = mercs2_engine::game_world::load_model_by_hash_state(&mut w, hash, 0x01) {
            let n = m.verts.len().max(1) as f32;
            let mut mn = [1.0f32; 3];
            let mut mx = [0.0f32; 3];
            let mut sum = [0.0f32; 3];
            let mut nonwhite = 0usize;
            for v in &m.verts {
                for c in 0..3 {
                    mn[c] = mn[c].min(v.color[c]);
                    mx[c] = mx[c].max(v.color[c]);
                    sum[c] += v.color[c];
                }
                if v.color.iter().any(|&c| c < 0.95) {
                    nonwhite += 1;
                }
            }
            println!(
                "[{name} 0x{hash:08X}] {} verts  color min({:.2},{:.2},{:.2}) max({:.2},{:.2},{:.2}) mean({:.2},{:.2},{:.2})  {} non-white ({:.0}%)",
                m.verts.len(), mn[0], mn[1], mn[2], mx[0], mx[1], mx[2],
                sum[0] / n, sum[1] / n, sum[2] / n, nonwhite, 100.0 * nonwhite as f32 / n
            );
        }
    }
}
