//! Hunt `minLOD` on disk. The engine clamps the LOD rung to [minLOD (M+0x80), maxLOD-1 (M+0x7c)];
//! maxLOD is the header's lod_count (+0x34) but minLOD's on-disk source was never located. The
//! amx30_elite has an essentially EMPTY tier 0 (188 tri of tread where its hull should be) and its
//! real body at tier 1 — exactly what a minLOD of 1 would mean. Dump the 72-byte model header of a
//! model with an empty tier 0 beside ones without, and look for the field that differs.
//!
//!   cargo run -p mercs2_probe --bin header_diff

use mercs2_engine::{model::Model, wad};

const MODELS: &[&str] = &[
    "vz_veh_tank_amx30_elite", // empty tier 0 -> expect minLOD = 1
    "vz_veh_tank_amx30_aa",
    "ch_veh_tank_ztz98",
    "vz_veh_tank_scorpion90",
    "civ_veh_car_van_crappy",
    "pmc_hum_mattias_v3",
];

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    // The header is descriptor row 0's INFO leaf (72 bytes) — same one parse_model_header reads.
    println!("{:<26} {:>4} {:>4}  header words 0x20..0x48 (u32 / f32)", "model", "lodc", "t0tri");
    for name in MODELS {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let Ok(m) = Model::load(&mut w, hash) else { continue };
        let hdr = mercs2_formats::model_cubeize::model_header_bytes(&m.resident);
        let Some(h) = hdr else {
            println!("{name:<26} <no header>");
            continue;
        };
        // Triangles the gate admits at tier 0 (ignore destruction — just geometry presence).
        let t0: u32 = m
            .rungs
            .iter()
            .flat_map(|r| r.draws.iter())
            .filter(|d| d.lod_mask & 0x01 != 0)
            .map(|d| d.index_count / 3)
            .sum();
        let lodc = m.lod_count();
        print!("{name:<26} {lodc:>4} {t0:>5}  ");
        for off in (0x20..0x48).step_by(4) {
            if off + 4 > h.len() {
                break;
            }
            let u = u32::from_le_bytes([h[off], h[off + 1], h[off + 2], h[off + 3]]);
            let f = f32::from_bits(u);
            if u < 64 {
                print!(" [{off:#04x}]={u}");
            } else if f.is_finite() && f.abs() > 0.01 && f.abs() < 1e6 {
                print!(" [{off:#04x}]={f:.0}f");
            } else {
                print!(" [{off:#04x}]={u:#x}");
            }
        }
        println!();
    }
}
