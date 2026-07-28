//! `watermap_probe` — resolve the static watermap singleton (`watr`, type `0x4D7D30C4`) through the
//! real asset layer and report what the engine gets: grid, world extent, wet/dry census, and the
//! waterline range. The check behind `[world] watermap loaded` at boot.
//!
//! ```text
//! cargo run --bin watermap_probe -- <path to vz.wad>
//! ```

use mercs2_engine::asset::AssetSource;
use mercs2_engine::water_sim::watermap::HEADER_LEN;
use mercs2_engine::water_sim::{Watermap, HEIGHT_MIN_M, OPEN_WATER_SURFACE_M};
use mercs2_formats::types::TYPE_HASH_WATERMAP;

fn main() {
    let path = std::env::args()
        .nth(1)
        .or_else(|| mercs2_engine::wad::resolve_vz_wad(None))
        .expect("usage: watermap_probe <vz.wad>");
    let mut assets = AssetSource::discover(&path, &[]).expect("mount wad stack");

    println!("type hash 0x{TYPE_HASH_WATERMAP:08X} = pandemic_hash_m2(\"watermap\")");
    assert_eq!(mercs2_formats::hash::pandemic_hash_m2("watermap"), TYPE_HASH_WATERMAP);

    let Some((name_hash, container)) = assets.extract_singleton(TYPE_HASH_WATERMAP) else {
        println!("NOT FOUND: no watermap chunk in any mounted archive");
        std::process::exit(1);
    };
    println!("resolved: chunk name 0x{name_hash:08X}, container {} B", container.len());

    let watr = mercs2_formats::ucfx::extract_chunk_body(&container, b"watr").expect("watr chunk");
    println!("watr body: {} B", watr.len());

    let wm = Watermap::from_watr_bytes(&watr).expect("parse watr");
    let (w, h) = (wm.width(), wm.height());
    let cs = wm.cell_size();
    println!("grid: {w}x{h} @ {cs} m  ({} cells)", w * h);
    println!(
        "world extent: X {:.0}..{:.0}  Z {:.0}..{:.0}  (span {:.0} m)",
        wm.origin_x - cs * 0.5,
        wm.origin_x + (w as f32 - 0.5) * cs,
        wm.origin_z - cs * 0.5,
        wm.origin_z + (h as f32 - 0.5) * cs,
        w as f32 * cs
    );

    let wet = wm.wet_cell_count();
    println!("census: {wet} wet / {} dry", w * h - wet);
    match wm.wet_height_range() {
        Some((lo, hi)) => println!("waterline over wet cells: {lo:.2} .. {hi:.2} m"),
        None => println!("waterline: no wet cells"),
    }

    // Spot samples: the origin, and the four corners of the playable box.
    for (x, z) in [(0.0, 0.0), (3000.0, 3000.0), (-3000.0, 3000.0), (3000.0, -3000.0), (-3000.0, -3000.0)] {
        let s = wm.sample(x, z);
        println!(
            "  sample ({x:>7.0}, {z:>7.0}) -> is_water={:<5} surface={:.2}{}",
            s.is_water,
            s.surface_height,
            if s.surface_height == HEIGHT_MIN_M { "  (dry sentinel)" } else { "" }
        );
    }
    println!("(reference open-water plateau: {OPEN_WATER_SURFACE_M} m)");

    let (pos, idx) = wm.surface_mesh();
    println!("surface mesh: {} verts / {} tris", pos.len(), idx.len() / 3);

    raw_layer_crosstab(&watr, w, h);
}

/// Cross-tab Layer 0 against Layer 1 straight out of the raw `watr` bytes — the check that the two
/// layers are actually aligned with each other. A wet cell must not carry the dry sentinel.
fn raw_layer_crosstab(watr: &[u8], w: usize, h: usize) {
    let n = w * h;
    let rd = |o: usize| f32::from_le_bytes([watr[o], watr[o + 1], watr[o + 2], watr[o + 3]]);
    let heights: Vec<f32> = (0..n).map(|i| rd(HEADER_LEN + i * 4)).collect();
    let mask = &watr[HEADER_LEN + n * 4..HEADER_LEN + n * 4 + n];

    let (mut wet_sentinel, mut wet_plateau, mut wet_other, mut dry_nonsentinel) = (0, 0, 0, 0);
    for i in 0..n {
        let sentinel = heights[i] == HEIGHT_MIN_M;
        match (mask[i], sentinel) {
            (255, true) => wet_sentinel += 1,
            (255, false) if (heights[i] - OPEN_WATER_SURFACE_M).abs() < 0.5 => wet_plateau += 1,
            (255, false) => wet_other += 1,
            (0, false) => dry_nonsentinel += 1,
            _ => {}
        }
    }
    println!("layer0 x layer1 cross-tab:");
    println!("  wet & open-water plateau (~{OPEN_WATER_SURFACE_M} m): {wet_plateau}");
    println!("  wet & other height (inland water):     {wet_other}");
    println!("  wet & DRY SENTINEL (misalignment?):    {wet_sentinel}");
    println!("  dry & non-sentinel height:             {dry_nonsentinel}");
}
