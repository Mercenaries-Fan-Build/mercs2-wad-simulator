//! Dump ALL raw SEGM records {i16 node, u8 seg_id, u8 mask} — how many are there really, and do any
//! name the barrel mount nodes (3, 11)? We index SEGM[sub_object]; if there are far more records
//! than sub-objects, that index is wrong.
use mercs2_engine::wad;
use mercs2_formats::model_cubeize;
fn main(){
    let name=std::env::args().nth(1).unwrap_or_else(||"ch_veh_tank_ztz98".into());
    let hash=mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w=wad::registry_vz_wad().and_then(|p|wad::open(&p).ok()).unwrap();
    let c=wad::extract_container(&mut w,hash).unwrap();
    let recs=model_cubeize::parse_segm(&c);
    let meshes=model_cubeize::read_model_meshes(&c).unwrap();
    println!("{name}: {} SEGM records, {} meshes/sub-objects\n", recs.len(), meshes.len());
    println!("  {:>4} {:>6} {:>7} {:>6}", "rec", "node", "seg_id", "mask");
    for (i,r) in recs.iter().enumerate().take(24) {
        println!("  {:>4} {:>6} {:>7} {:>#6x}", i, r.bone as i16, r.seg_id, r.state_mask);
    }
    if recs.len()>24 { println!("  ... ({} more)", recs.len()-24); }
    // Do any records name the barrel mounts?
    for want in [3u16, 11] {
        let hits: Vec<usize> = recs.iter().enumerate()
            .filter(|(_,r)| r.bone == want).map(|(i,_)| i).collect();
        println!("\n  records naming node {want} (a barrel mount): {hits:?}");
        for &i in hits.iter().take(4) {
            let r=&recs[i];
            println!("     rec {i}: node={} seg_id={} mask={:#04x}", r.bone as i16, r.seg_id, r.state_mask);
        }
    }
}
