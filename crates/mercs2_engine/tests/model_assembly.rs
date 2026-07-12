//! The cross-block model assembly, locked down against the retail WAD.
//!
//! A vehicle's geometry is split across `<model>_P00N_Q(3-N)` blocks: only the resident (coarsest)
//! one ships HIER/SEGM/MTRL/the destruction machine, and the finer ones ship geometry that indexes
//! the resident block's SEGM. Reading each rung in isolation is what made every vehicle render as
//! its low-poly far-LOD proxy — a 371-triangle tank wearing a `_lod_dm` skin. These tests fail if we
//! ever go back to loading a single container.
//!
//! Skipped when vz.wad isn't installed.

use mercs2_engine::model::Model;
use mercs2_engine::render_state::RenderState;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;

fn open_wad() -> Option<wad::Wad> {
    wad::registry_vz_wad().and_then(|p| wad::open(&p).ok())
}

/// Triangles the full three-clause gate admits at a LOD rung, at a given health.
fn tris_at(m: &Model, rung: u8, health: f32) -> u32 {
    let node_enable = match &m.machine {
        Some(sm) => {
            let chosen = orch::node_states_for_health(sm, health, 0.99);
            orch::machine_node_enable(sm, &m.hier, &chosen)
        }
        None => Vec::new(),
    };
    let rs = RenderState { lod: rung, view_state: 1u8 << rung.min(7), node_enable };
    m.visible_draws(&rs).iter().map(|(_, d)| d.index_count / 3).sum()
}

#[test]
fn tank_assembles_across_its_lod_block_chain() {
    let Some(mut w) = open_wad() else { return };
    let hash = mercs2_formats::hash::pandemic_hash_m2("ch_veh_tank_ztz98");
    let m = Model::load(&mut w, hash).expect("tank assembles");

    // Three blocks: resident 4,435 tri + P001 18,333 + P002 28,620.
    assert_eq!(m.rungs.len(), 3, "tank ships a 3-block LOD chain");
    assert!(m.triangles() > 50_000, "whole chain, not one block: {}", m.triangles());

    // The resident SEGM is the master table for every rung — 130 records for 12+35+63 groups.
    assert_eq!(m.segm.len(), 130);
    assert!(m.rungs.iter().all(|r| !r.draws.is_empty()), "every rung binds to the resident SEGM");

    // Up close we draw the real hull, not the 371-triangle proxy that only serves rungs 4-6.
    let near = tris_at(&m, 0, 1.0);
    let far = tris_at(&m, 4, 1.0);
    assert!(near > 5_000, "near rung draws the detailed hull, got {near}");
    assert!(far < 1_000, "far rung draws the low-poly proxy, got {far}");
    assert!(near > far * 5, "near/far must differ by an order of magnitude: {near} vs {far}");
}

#[test]
fn wrecking_a_vehicle_swaps_geometry_rather_than_adding_it() {
    let Some(mut w) = open_wad() else { return };
    // The destruction machine SHOWs the intact body in PristineState and the wreck in
    // DestroyedState. If both draw at once we're piling a wreck on top of an intact hull — the
    // original "MD500 drawn with its wreck overlapping" bug.
    for name in ["ch_veh_tank_ztz98", "global_veh_klr650", "civ_veh_car_van_crappy"] {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let m = Model::load(&mut w, hash).expect("assembles");
        let intact = tris_at(&m, 0, 1.0);
        let wrecked = tris_at(&m, 0, 0.0);
        assert!(intact > wrecked, "{name}: intact {intact} should exceed wrecked {wrecked}");
        assert!(wrecked > 0, "{name}: a wreck still has geometry");
    }
}

#[test]
fn rungs_refine_each_other_instead_of_double_drawing() {
    let Some(mut w) = open_wad() else { return };
    // The resident block is a COMPLETE low-detail model spanning every tier; the finer blocks
    // re-author some of its nodes. Pooling them draws the same part twice at two detail levels — on
    // the car van that was 11,604 of 19,107 triangles. `apply_supersede` clears the coarser block's
    // bit for any (node, tier) a finer block covers, so no node may be drawn by two rungs at once.
    for name in ["civ_veh_car_van_crappy", "ch_veh_tank_ztz98", "vz_veh_tank_scorpion90"] {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let m = Model::load(&mut w, hash).expect("assembles");
        for tier in 0..8u8 {
            let bit = 1u8 << tier;
            let mut owner: std::collections::HashMap<i16, u8> = Default::default();
            for r in &m.rungs {
                for d in r.draws.iter().filter(|d| d.node >= 0 && d.lod_mask & bit != 0) {
                    if let Some(&prev) = owner.get(&d.node) {
                        assert_eq!(
                            prev, r.level,
                            "{name}: node {} at tier {tier} is drawn by rung {prev} AND rung {} \
                             — two detail levels of one part in the same space",
                            d.node, r.level
                        );
                    }
                    owner.insert(d.node, r.level);
                }
            }
        }
    }
}

#[test]
fn a_character_has_no_lod_chain_and_is_unaffected() {
    let Some(mut w) = open_wad() else { return };
    let hash = mercs2_formats::hash::pandemic_hash_m2("pmc_hum_mattias_v3");
    let m = Model::load(&mut w, hash).expect("mattias assembles");
    assert_eq!(m.rungs.len(), 1, "characters ship a single resident block, no chain");
    assert_eq!(tris_at(&m, 0, 1.0), 22_151, "unchanged by the cross-block work");
}

#[test]
fn lod_masks_partition_the_tiers_across_the_chain() {
    let Some(mut w) = open_wad() else { return };
    let hash = mercs2_formats::hash::pandemic_hash_m2("ch_veh_tank_ztz98");
    let m = Model::load(&mut w, hash).expect("tank assembles");

    // Each block claims a band: resident owns the far tiers (4-6), the streamed ones the near tiers.
    // A rung whose segments resolved against the WRONG SEGM comes back all-zero — that was the bug.
    let by_level = |lv: u8| m.rungs.iter().find(|r| r.level == lv).expect("rung").lod_bits();
    assert_eq!(by_level(0) & 0x70, 0x70, "resident block serves the far tiers");
    assert!(by_level(2) & 0x03 != 0, "the finest block serves the near tiers");
    assert!(m.rungs.iter().all(|r| r.lod_bits() != 0), "no rung resolves to a zero mask");
}
