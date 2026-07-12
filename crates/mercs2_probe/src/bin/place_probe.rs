//! Disassemble a model's MESH ASSEMBLY: per drawing group, show the two candidate placement nodes
//! (SEGM record's node vs INDX[group]) with each node's WORLD position, the group's raw local bbox
//! centre, and where the group would land under each choice. Makes "which node places this chunk"
//! a thing we can SEE, instead of a rule someone asserts.
use mercs2_engine::wad;
use mercs2_formats::{model_cubeize, orchestrator, skeleton::Skeleton};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = name.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).unwrap();
    let c = wad::extract_container(&mut w, hash).unwrap();

    // Skeleton wants the 20-byte block wrapper + UCFX.
    let mut blk = vec![0u8; 20];
    blk[16..20].copy_from_slice(&(c.len() as u32).to_le_bytes());
    blk.extend_from_slice(&c);
    let skel = Skeleton::from_block(&blk).ok();
    let hier = orchestrator::parse_hier(&c);
    let indx = orchestrator::parse_indx(&c);
    let meshes = model_cubeize::read_model_meshes(&c).unwrap();

    let wpos = |n: usize| -> String {
        match skel.as_ref().and_then(|s| s.bones.get(n)) {
            // row-vector world matrix: translation is row 3
            Some(b) => format!("({:6.2},{:6.2},{:6.2})", b.world[3][0], b.world[3][1], b.world[3][2]),
            None => "     (no bone)     ".into(),
        }
    };
    let nhash = |n: usize| hier.get(n).map(|h| format!("{:#010x}", h.hash)).unwrap_or("-".into());

    println!("{name}: {} meshes, {} HIER nodes, {} INDX rows\n", meshes.len(), hier.len(), indx.len());
    println!("  {:>3} {:>4} {:>6} {:>5} | {:>4} {:>11} {:>21} | {:>4} {:>11} {:>21} | {}",
        "grp","sub","rigid","skin","SEGM","node hash","SEGM node world","INDX","node hash","INDX node world","local centre");
    for m in &meshes {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &m.positions { for k in 0..3 { lo[k]=lo[k].min(p[k]); hi[k]=hi[k].max(p[k]); } }
        let ctr = if m.positions.is_empty() { [0.0;3] } else { [(lo[0]+hi[0])*0.5,(lo[1]+hi[1])*0.5,(lo[2]+hi[2])*0.5] };
        let sn = m.bone as usize;
        let inode = indx.get(m.group_index).copied();
        println!("  {:>3} {:>4} {:>6} {:>5} | {:>4} {:>11} {} | {:>4} {:>11} {} | ({:6.2},{:6.2},{:6.2})",
            m.group_index, m.sub_object, m.rigid, !m.joints.is_empty(),
            m.bone, nhash(sn), wpos(sn),
            inode.map(|n| n.to_string()).unwrap_or("-".into()),
            inode.map(nhash).unwrap_or("-".into()),
            inode.map(wpos).unwrap_or("        -          ".into()),
            ctr[0], ctr[1], ctr[2]);
    }
}
