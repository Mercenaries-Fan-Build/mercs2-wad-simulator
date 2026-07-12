//! DISASSEMBLE the mesh assembly. For each drawing group, show its raw local bbox and the three
//! candidate placement nodes — SEGM record's node, INDX[group], and the HIER node whose OWN bbox
//! best matches the mesh — with each node's world position and where the mesh would land.
//!
//! HIER records carry a per-node bbox (`+144`/`+160`). If a node's bbox matches a mesh's, that node
//! owns the mesh — which gives us the placement rule from DATA instead of a rule someone asserted.
//!
//!   cargo run -p mercs2_probe --bin assembly_probe -- ch_veh_tank_ztz98

use mercs2_engine::wad;
use mercs2_formats::{model_cubeize, orchestrator, skeleton::Skeleton};

fn size(mn: [f32; 3], mx: [f32; 3]) -> [f32; 3] {
    [mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]]
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = name
        .strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");

    let mut blk = vec![0u8; 20];
    blk[16..20].copy_from_slice(&(c.len() as u32).to_le_bytes());
    blk.extend_from_slice(&c);
    let skel = Skeleton::from_block(&blk).expect("skeleton");
    let hier = orchestrator::parse_hier(&c);
    let indx = orchestrator::parse_indx(&c);
    let meshes = model_cubeize::read_model_meshes(&c).expect("meshes");

    let wpos = |n: usize| -> [f32; 3] {
        skel.bones
            .get(n)
            .map(|b| [b.world[3][0], b.world[3][1], b.world[3][2]])
            .unwrap_or([0.0; 3])
    };

    println!("{name}: {} meshes, {} HIER nodes\n", meshes.len(), hier.len());
    println!(
        "  {:>3} {:>16} | {:>4} {:>22} | {:>4} {:>22} | {:>4} {:>22}",
        "grp", "mesh size", "SEGM", "-> lands at", "INDX", "-> lands at", "BBOX", "-> lands at"
    );

    for m in &meshes {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if m.positions.is_empty() {
            continue;
        }
        let msz = size(lo, hi);
        let ctr = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];

        // Which HIER node's OWN bbox matches this mesh's size best?
        let mut best = (f32::MAX, usize::MAX);
        for (i, h) in hier.iter().enumerate() {
            let hsz = size(h.bbox_min, h.bbox_max);
            if hsz.iter().all(|&v| v <= 0.001) {
                continue; // node carries no geometry
            }
            let err: f32 = (0..3).map(|k| (hsz[k] - msz[k]).abs()).sum();
            if err < best.0 {
                best = (err, i);
            }
        }

        let land = |n: usize| -> String {
            if n == usize::MAX {
                return "            -           ".into();
            }
            let w = wpos(n);
            format!("({:6.2},{:6.2},{:6.2})", ctr[0] + w[0], ctr[1] + w[1], ctr[2] + w[2])
        };
        let seg = m.bone as usize;
        let ind = indx.get(m.group_index).copied().unwrap_or(usize::MAX);
        println!(
            "  {:>3} {:>16} | {:>4} {} | {:>4} {} | {:>4} {}  (err {:.2})",
            m.group_index,
            format!("{:.1}x{:.1}x{:.1}", msz[0], msz[1], msz[2]),
            seg,
            land(seg),
            if ind == usize::MAX { "-".into() } else { ind.to_string() },
            land(ind),
            best.1,
            land(best.1),
            best.0,
        );
    }
    println!("\n  'lands at' = mesh local centre + that node's world translation.");
    println!("  A tank barrel belongs at y≈1.7-1.9 (turret height); the hull at y≈1.2.");
}
