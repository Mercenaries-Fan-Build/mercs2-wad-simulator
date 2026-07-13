//! Test the hypothesis: `INDX[i]` is a SEG_ID, so a mesh's segment record is `SEGM[INDX[i]]` —
//! NOT `SEGM[i]` (what we do today). Compare the node + LOD mask each rule yields, and check the
//! node against the HIER node whose own bbox matches the mesh (the independent witness).
//!
//!   cargo run -p mercs2_probe --bin segfix_probe -- ch_veh_tank_ztz98

use mercs2_engine::wad;
use mercs2_formats::{model_cubeize, orchestrator, skeleton::Skeleton};

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
    let segm = model_cubeize::parse_segm(&c);
    let meshes = model_cubeize::read_model_meshes(&c).expect("meshes");

    let wpos = |n: usize| -> [f32; 3] {
        skel.bones
            .get(n)
            .map(|b| [b.world[3][0], b.world[3][1], b.world[3][2]])
            .unwrap_or([0.0; 3])
    };

    println!("{name}: {} meshes, {} SEGM records, {} INDX rows\n", meshes.len(), segm.len(), indx.len());
    println!(
        "  {:>3} | {:>4} {:>5} | {:>4} {:>5} {:>4} | {:>22} | {:>4} {}",
        "grp", "OLD", "mask", "NEW", "mask", "seg", "NEW lands at", "bbox", "agree?"
    );

    let mut agree = 0;
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
        let msz = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let ctr = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];

        // independent witness: HIER node whose own bbox matches this mesh
        let mut best = (f32::MAX, usize::MAX);
        for (i, h) in hier.iter().enumerate() {
            let hs = [
                h.bbox_max[0] - h.bbox_min[0],
                h.bbox_max[1] - h.bbox_min[1],
                h.bbox_max[2] - h.bbox_min[2],
            ];
            if hs.iter().all(|&v| v <= 0.001) {
                continue;
            }
            let err: f32 = (0..3).map(|k| (hs[k] - msz[k]).abs()).sum();
            if err < best.0 {
                best = (err, i);
            }
        }

        // OLD rule: SEGM[sub_object]
        let old = segm.get(m.sub_object);
        // NEW rule: SEGM[INDX[group_index]]
        let seg_id = indx.get(m.group_index).copied();
        let new = seg_id.and_then(|s| segm.get(s));

        let nn = new.map(|r| r.bone as usize).unwrap_or(usize::MAX);
        let w = if nn == usize::MAX { [0.0; 3] } else { wpos(nn) };
        let ok = nn == best.1;
        if ok {
            agree += 1;
        }
        println!(
            "  {:>3} | {:>4} {:>#5x} | {:>4} {:>#5x} {:>4} | ({:6.2},{:6.2},{:6.2}) | {:>4} {}",
            m.group_index,
            old.map(|r| (r.bone as i16).to_string()).unwrap_or("-".into()),
            old.map(|r| r.state_mask).unwrap_or(0),
            new.map(|r| (r.bone as i16).to_string()).unwrap_or("-".into()),
            new.map(|r| r.state_mask).unwrap_or(0),
            seg_id.map(|s| s.to_string()).unwrap_or("-".into()),
            ctr[0] + w[0],
            ctr[1] + w[1],
            ctr[2] + w[2],
            best.1,
            if ok { "YES" } else { "no" },
        );
    }
    println!(
        "\n  NEW rule (SEGM[INDX[i]]) node agrees with the bbox-matched HIER node on {agree}/{} meshes.",
        meshes.len()
    );
}
