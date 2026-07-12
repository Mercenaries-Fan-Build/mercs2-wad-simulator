//! Is INDX ever the identity map? `accessory_bone_binding_B.md` claims "the i-th GEOM child attaches
//! to SEGM record i" — which is only true where INDX[i] == i. Print the raw rows.
//!
//!   cargo run -p mercs2_probe --bin indx_dump -- pmc_hum_mattias_v3

use mercs2_engine::wad;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if args.is_empty() {
        vec!["pmc_hum_mattias_v3".into(), "ch_veh_tank_ztz98".into(), "vz_veh_tank_amx30_elite".into()]
    } else { args };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(lods) = wad::extract_model_lods(&mut w, hash) else { continue };
        for l in &lods {
            let indx = mercs2_formats::orchestrator::parse_indx(&l.container);
            let ident = indx.iter().enumerate().all(|(i, &v)| i == v);
            println!("{name:26} P{:03}  identity={ident:5}  INDX={:?}", l.level,
                &indx[..indx.len().min(20)]);
        }
    }
}
