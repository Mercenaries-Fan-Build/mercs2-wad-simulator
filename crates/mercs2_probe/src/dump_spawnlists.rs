//! `dump-spawnlists` — the retail runtime unit lists a v1.0 capture holds: the table
//! `0x016E51F0` (stride `0x24`, count `DAT_01175D18`, built by `FUN_004E83F0` from the `PopulationList`
//! records of one list key). Each entry is `{u32 list key, u32 0, record, u32 head}` with the record
//! `{u32 -1, u32 n, u32 Σ max(weight, 1), u32* units, u32* weights, u32 next}` (`FUN_004E8130`), its
//! arrays already sorted by `FUN_004DC870`. Prints each list's units and weights in stored order.

use crate::dump::{Capture, Mem};

const TABLE: u32 = 0x016E_51F0;
const COUNT: u32 = 0x0117_5D18;
const STRIDE: u32 = 0x24;
/// `FUN_004E83F0` refuses to add past 0x100 lists.
const CAP: u32 = 0x100;

/// One runtime list as the capture holds it.
pub struct LiveUnitList {
    pub key: u32,
    pub units: Vec<u32>,
    pub weights: Vec<i32>,
    pub total: u32,
}

pub fn live_unit_lists(mem: &Mem) -> Result<Vec<LiveUnitList>, String> {
    let count = mem.u32(COUNT)?;
    if count > CAP {
        return Err(format!("list count {count} at 0x{COUNT:08X} exceeds the table's {CAP}"));
    }
    let mut out = Vec::new();
    for i in 0..count {
        let s = TABLE + i * STRIDE;
        let rec = s + 8;
        let n = mem.u32(rec + 4)?;
        let (units, weights) = (mem.u32(rec + 12)?, mem.u32(rec + 16)?);
        let mut l = LiveUnitList { key: mem.u32(s)?, units: Vec::new(), weights: Vec::new(), total: mem.u32(rec + 8)? };
        for k in 0..n {
            l.units.push(mem.u32(units + 4 * k)?);
            l.weights.push(mem.u32(weights + 4 * k)? as i32);
        }
        out.push(l);
    }
    Ok(out)
}

pub fn dump_spawnlists(capture: &str) -> Result<(), String> {
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    let lists = live_unit_lists(&mem)?;
    println!("[dump-spawnlists] {} (build {}): {} lists", cap.path.display(), cap.build_id, lists.len());
    for l in &lists {
        let row: Vec<String> = l.units.iter().zip(&l.weights).map(|(u, w)| format!("0x{u:08X}:{w}")).collect();
        println!("list 0x{:08X} n {} total {}: {}", l.key, l.units.len(), l.total, row.join(" "));
    }
    Ok(())
}

#[cfg(all(test, feature = "retail"))]
mod tests {
    use super::*;
    use mercs2_formats::unit_list::{unit_lists, RuntimeUnitList};
    use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};
    use std::path::Path;

    /// Every runtime unit list the game built in each `updated-chris-exports` capture equals the
    /// list `RuntimeUnitList::build` makes from the shipped `PopulationList` data: units and weights in
    /// the same order, the same total.
    #[test]
    fn live_unit_lists_match_the_built_lists() {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let mut assets = mercs2_engine::asset::AssetSource::discover(&wad.to_string_lossy(), &[]).unwrap_or_else(|e| panic!("{e}"));
        let we = assets.resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH).expect("worldentity");
        let lists = unit_lists(&WorldEntity::parse(&we).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
        let dir = mercs2_formats::game_paths::local_config_memory_dumps_dir(start)
            .unwrap_or_else(|e| panic!("{e}"))
            .join("updated-chris-exports");
        let mut checked = 0;
        for name in [
            "20261021_chris-jacobs-flying-winch.dmp",
            "20261021_chris-jacobs-mid-mission.dmp",
            "20261021_clean-load-chris-jacobs-7-percent.dmp",
            "20261021_mattias-nilson-jacket-skin.dmp",
        ] {
            let cap = Capture::open(&dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
            for live in live_unit_lists(&cap.mem().unwrap()).unwrap_or_else(|e| panic!("{e}")) {
                let data = lists.get(&live.key).unwrap_or_else(|| panic!("{name}: list 0x{:08X} is in no PopulationList", live.key));
                let built = RuntimeUnitList::build(data).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(built.units, live.units, "{name}: list 0x{:08X} units", live.key);
                assert_eq!(built.weights, live.weights, "{name}: list 0x{:08X} weights", live.key);
                assert_eq!(built.total, live.total, "{name}: list 0x{:08X} total", live.key);
                checked += 1;
            }
        }
        assert!(checked >= 30, "only {checked} live lists checked");
    }
}
