//! Show a HIER node's PARENT CHAIN with local translation vs our computed WORLD translation, so we
//! can see whether world matrices accumulate the parent chain correctly (or collapse to identity).
use mercs2_engine::wad;
use mercs2_formats::{orchestrator, skeleton::Skeleton};
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let want: Vec<usize> = std::env::args().skip(2).filter_map(|a| a.parse().ok()).collect();
    let want = if want.is_empty() { vec![18usize, 19] } else { want };
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).unwrap();
    let c = wad::extract_container(&mut w, hash).unwrap();
    let mut blk = vec![0u8; 20];
    blk[16..20].copy_from_slice(&(c.len() as u32).to_le_bytes());
    blk.extend_from_slice(&c);
    let skel = Skeleton::from_block(&blk).expect("skeleton");
    let hier = orchestrator::parse_hier(&c);
    println!("{name}: {} HIER nodes, {} skeleton bones\n", hier.len(), skel.bones.len());
    for &n in &want {
        println!("  chain for node {n}:");
        let mut cur = Some(n);
        let mut depth = 0;
        while let Some(i) = cur {
            let Some(h) = hier.get(i) else { break };
            let loc = [h.local[12], h.local[13], h.local[14]]; // row-vector: translation row 3
            let wpos = skel.bones.get(i).map(|b| [b.world[3][0], b.world[3][1], b.world[3][2]]);
            println!("    {:indent$}node {i:3} {:#010x} parent={:?}  local=({:6.2},{:6.2},{:6.2})  world={:?}",
                "", h.hash, h.parent, loc[0], loc[1], loc[2],
                wpos.map(|p| format!("({:6.2},{:6.2},{:6.2})", p[0], p[1], p[2])),
                indent = depth * 2);
            cur = h.parent;
            depth += 1;
            if depth > 12 { println!("    ...(deep)"); break }
        }
        println!();
    }
}
