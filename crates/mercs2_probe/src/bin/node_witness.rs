//! Independent witness for the INDX->SEGM node binding: a mesh must physically sit at the node it
//! claims. HIER records carry a per-node bbox (+144/+160); compare each draw group's own bbox centre
//! against its assigned node's. A wrong node shows up as a mesh mounted metres from where it draws.
//!
//! This is the check that originally proved `SEGM[INDX[group]]` (barrels lifted from y~0 to turret
//! height). Re-run it per LOD rung — the amx30_elite binds tread to a node at tier 0 and the hull to
//! the SAME node at tier 1, which cannot both be right.
//!
//!   cargo run -p mercs2_probe --bin node_witness -- ch_veh_tank_ztz98

use mercs2_engine::{model::Model, wad};
use mercs2_formats::skeleton::Skeleton;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if args.is_empty() {
        vec!["ch_veh_tank_ztz98".into(), "vz_veh_tank_amx30_elite".into(), "vz_veh_tank_amx30_aa".into()]
    } else { args };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(m) = Model::load(&mut w, hash) else { continue };

        // Node world-rest positions, from the resident HIER (the only skeleton in the chain).
        let mut blk = vec![0u8; 20];
        blk[16..20].copy_from_slice(&(m.resident.len() as u32).to_le_bytes());
        blk.extend_from_slice(&m.resident);
        let Ok(skel) = Skeleton::from_block(&blk) else { continue };

        println!("\n{name}");
        for r in &m.rungs {
            let (mut ok, mut bad, mut worst) = (0u32, 0u32, 0.0f32);
            for d in r.draws.iter().filter(|d| d.node >= 0 && d.index_count > 0) {
                let n = d.node as usize;
                if n >= skel.bones.len() { continue; }
                // Mesh centroid in model space.
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                let s = d.index_start as usize;
                for i in s..(s + d.index_count as usize).min(r.indices.len()) {
                    let v = &r.vertices[r.indices[i] as usize];
                    for k in 0..3 { lo[k] = lo[k].min(v.pos[k]); hi[k] = hi[k].max(v.pos[k]); }
                }
                let c = [(lo[0]+hi[0])/2.0, (lo[1]+hi[1])/2.0, (lo[2]+hi[2])/2.0];
                // Node world-rest origin (row-vector: translation is row 3).
                let wm = skel.bones[n].world;
                let p = [wm[3][0], wm[3][1], wm[3][2]];
                let dist = ((c[0]-p[0]).powi(2) + (c[1]-p[1]).powi(2) + (c[2]-p[2]).powi(2)).sqrt();
                // A mesh mounted on a node should be near it. Vehicles are ~3-7 m; >3 m is a mismatch.
                if dist > 3.0 { bad += 1; worst = worst.max(dist); } else { ok += 1; }
            }
            println!("   P{:03}: {ok:3} meshes near their node, {bad:3} FAR (worst {worst:.1} m)", r.level);
        }
    }
}
