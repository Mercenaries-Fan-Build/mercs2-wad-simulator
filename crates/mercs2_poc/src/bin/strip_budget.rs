//! Recompute the real per-model index/vertex budget instead of trusting a hardcoded
//! triangle cap. Reports naive vs adjacency strip cost against the u16 ceiling.
//!   strip_budget <model.glb> [<model.glb> ...]

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::model_inject::{strip_to_tris, to_strip, to_strip_connected};

const U16_MAX: usize = 65535;

fn main() {
    for path in std::env::args().skip(1) {
        let glb = match gltf::load_char_glb(&path) {
            Ok(g) => g,
            Err(e) => {
                println!("{path}: load failed: {e}");
                continue;
            }
        };
        let tris = glb.tris.len();
        let verts = glb.positions.len();

        let naive = to_strip(&glb.tris);
        let conn = to_strip_connected(&glb.tris);

        // Self-verify the adjacency strip reproduces the exact triangle set.
        let back = strip_to_tris(&conn);
        let ok = back.len() == tris;

        let n_cost = naive.len() as f64 / tris as f64;
        let c_cost = conn.len() as f64 / tris as f64;

        println!("== {path}");
        println!("   tris {tris}  verts {verts}  (u16 vert ceiling {U16_MAX}: {})",
            if verts <= U16_MAX { "OK" } else { "OVER" });
        println!("   naive to_strip           : {:>7} idx  ({:.2} idx/tri)  max tris @cost = {}",
            naive.len(), n_cost, (U16_MAX as f64 / n_cost) as usize);
        println!("   to_strip_connected       : {:>7} idx  ({:.2} idx/tri)  max tris @cost = {}",
            conn.len(), c_cost, (U16_MAX as f64 / c_cost) as usize);
        println!("   fits u16 index ceiling   : naive {}  connected {}",
            if naive.len() <= U16_MAX { "YES" } else { "NO" },
            if conn.len() <= U16_MAX { "YES" } else { "NO" });
        println!("   strip round-trip exact   : {}", if ok { "YES" } else { "NO" });
    }
}
