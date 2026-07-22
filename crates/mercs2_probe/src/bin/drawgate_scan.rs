//! How many retail model groups would a PRMT draw-count filter drop?
//!
//! Before making the shared mesh builder honour PRMT+8 it has to be shown that retail geometry does
//! not rely on drawing groups whose draw count is zero. If a real model ships zero-count groups it
//! still draws, the field does not mean what the injector assumes and the filter would delete
//! geometry across the whole game.
//!
//!   drawgate_scan [limit]
use mercs2_engine::wad;
use mercs2_formats::model_inject::group_draw_report;

fn main() {
    let limit: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(400);
    let path = wad::registry_vz_wad().expect("vz.wad");
    let mut w = wad::open(&path).expect("open");
    let hashes: Vec<u32> = wad::model_list_all(&w).into_iter().map(|(h, _)| h).collect();
    println!("scanning {} model assets (limit {limit})", hashes.len());

    let (mut models, mut groups, mut zero, mut zero_with_ic) = (0usize, 0usize, 0usize, 0usize);
    let mut worst: Vec<(u32, usize, usize)> = Vec::new();
    for h in hashes.into_iter().take(limit) {
        let Ok(c) = wad::extract_container(&mut w, h) else { continue };
        let Ok(rep) = group_draw_report(&c) else { continue };
        models += 1;
        let mut z = 0usize;
        for (_, ic, mx) in &rep {
            groups += 1;
            if *mx == 0 {
                zero += 1;
                if *ic > 0 {
                    zero_with_ic += 1;
                    z += 1;
                }
            }
        }
        if z > 0 {
            worst.push((h, z, rep.len()));
        }
    }
    worst.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("  {models} models, {groups} groups");
    println!("  draw-count 0            : {zero}");
    println!("  draw-count 0 AND ic > 0 : {zero_with_ic}   <- these are what a filter would DROP");
    for (h, z, n) in worst.iter().take(10) {
        println!("    0x{h:08X}: {z} of {n} groups");
    }
}
