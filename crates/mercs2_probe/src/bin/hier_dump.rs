//! Dump every HIER node: index, hash, parent, WORLD position. Tab-separated so the hashes can be
//! reversed against the name corpus offline — the point is to SEE the real skeleton (turret/barrel
//! mounts, hardpoints) rather than guess which node places a mesh.
//!
//!   cargo run -p mercs2_probe --bin hier_dump -- ch_veh_tank_ztz98
use mercs2_engine::wad;
use mercs2_formats::{orchestrator, skeleton::Skeleton};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = name
        .strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");

    let mut blk = vec![0u8; 20];
    blk[16..20].copy_from_slice(&(c.len() as u32).to_le_bytes());
    blk.extend_from_slice(&c);
    let skel = Skeleton::from_block(&blk).expect("skeleton");
    let hier = orchestrator::parse_hier(&c);

    for (i, h) in hier.iter().enumerate() {
        let p = skel
            .bones
            .get(i)
            .map(|b| [b.world[3][0], b.world[3][1], b.world[3][2]])
            .unwrap_or([0.0; 3]);
        println!(
            "{i}\t{:#010x}\t{}\t{:.2}\t{:.2}\t{:.2}",
            h.hash,
            h.parent.map(|x| x as i64).unwrap_or(-1),
            p[0],
            p[1],
            p[2]
        );
    }
}
