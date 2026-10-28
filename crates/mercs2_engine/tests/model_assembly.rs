//! The cross-block model assembly, locked down against the retail WAD.
//!
//! A vehicle's geometry is split across `<model>_P00N_Q(3-N)` blocks: only the resident (coarsest)
//! one ships HIER/SEGM/MTRL/the destruction machine, and the finer ones ship geometry that indexes
//! the resident block's SEGM. Reading each rung in isolation is what made every vehicle render as
//! its low-poly far-LOD proxy — a 371-triangle tank wearing a `_lod_dm` skin. These tests fail if we
//! ever go back to loading a single container.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.

use mercs2_engine::model::Model;
use mercs2_engine::render_state::RenderState;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;

/// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else. Panics with the
/// resolver's message when it is missing, and when the path is not UTF-8 (`wad::open` takes `&str`).
fn vz_wad_path() -> String {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    path.to_str()
        .unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display()))
        .to_string()
}

/// The retail `vz.wad`, opened. Panics when it cannot be found or opened.
fn open_vz_wad() -> wad::Wad {
    let path = vz_wad_path();
    wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"))
}

/// Triangles the full three-clause gate admits at a LOD rung, at a given health.
fn tris_at(m: &Model, rung: u8, health: f32) -> u32 {
    let node_enable = match &m.machine {
        Some(sm) => {
            let chosen = orch::node_states_for_health(sm, health, 0.99);
            orch::machine_node_enable(sm, &m.hier, &chosen).unwrap_or_else(|e| panic!("{e}"))
        }
        None => Vec::new(),
    };
    let rs = RenderState { lod: rung, view_state: 1u8 << rung.min(7), node_enable };
    m.visible_draws(&rs).iter().map(|(_, d)| d.index_count / 3).sum()
}

#[test]
fn tank_assembles_across_its_lod_block_chain() {
    let mut w = open_vz_wad();
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
    let mut w = open_vz_wad();
    // The machine SHOWs the intact body in PristineState and the wreck in DestroyedState. If both
    // draw at once we're piling a wreck on top of an intact hull — the original "MD500 drawn with
    // its wreck overlapping" bug.
    //
    // The invariant is that the two states draw DIFFERENT geometry, not that the wreck is smaller:
    // a tank's wreck is the shared `global_veh_tank_ruin` asset and legitimately has MORE triangles
    // than its clean hull. (An earlier version of this test asserted `intact > wrecked` — that was a
    // heuristic, and it was wrong.)
    for name in ["ch_veh_tank_ztz98", "vz_veh_tank_amx30_elite", "civ_veh_car_van_crappy"] {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let m = Model::load(&mut w, hash).expect("assembles");
        let nodes = |health: f32| -> std::collections::BTreeSet<i16> {
            let node_enable = match &m.machine {
                Some(sm) => {
                    let chosen = orch::node_states_for_health(sm, health, 0.99);
                    orch::machine_node_enable(sm, &m.hier, &chosen).unwrap_or_else(|e| panic!("{e}"))
                }
                None => Vec::new(),
            };
            let rs = RenderState { lod: 0, view_state: 0x01, node_enable };
            m.visible_draws(&rs).iter().map(|(_, d)| d.node).collect()
        };
        let intact = nodes(1.0);
        let wrecked = nodes(0.0);
        assert!(tris_at(&m, 0, 1.0) > 0 && tris_at(&m, 0, 0.0) > 0, "{name}: both states draw");
        assert!(
            !wrecked.is_superset(&intact),
            "{name}: the wreck draws everything the intact body does — it is being ADDED, not swapped"
        );
        assert!(
            wrecked.difference(&intact).next().is_some(),
            "{name}: destroying it must bring in geometry the intact body doesn't have"
        );
    }
}

#[test]
fn rungs_refine_each_other_instead_of_double_drawing() {
    let mut w = open_vz_wad();
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
fn a_character_has_no_lod_chain() {
    let mut w = open_vz_wad();
    let hash = mercs2_formats::hash::pandemic_hash_m2("pmc_hum_mattias_v3");
    let m = Model::load(&mut w, hash).expect("mattias assembles");
    assert_eq!(m.rungs.len(), 1, "characters ship a single resident block, no chain");
    assert!(tris_at(&m, 0, 1.0) > 15_000, "and still draw at the near tier");
}

/// `INDX` is indexed by SUB-OBJECT ordinal (`MESH`/`SKIN` under `GEOM`), not by PRMG group.
///
/// Its length equals the sub-object count in every container in the game — never the PRMG count,
/// which is always larger because one sub-object can own several drawing groups. Keying on the group
/// index reads the WRONG SEGM row for every group past the first divergence, which hands the mesh
/// someone else's bone and someone else's LOD mask. That is what threw the amx30_elite's treads into
/// the air at three of its four tiers.
#[test]
fn indx_is_keyed_by_sub_object_not_by_prmg_group() {
    let mut w = open_vz_wad();
    for name in [
        "pmc_hum_mattias_v3",       // 24 INDX = 7 MESH + 17 SKIN  (29 PRMG)
        "ch_veh_tank_ztz98",
        "vz_veh_tank_amx30_elite",
        "civ_veh_car_van_crappy",
        "oc_veh_helicopter_md500",
    ] {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let m = Model::load(&mut w, hash).expect("assembles");
        for l in wad::extract_model_lods(&mut w, hash).expect("chain") {
            let indx = orch::parse_indx(&l.container);
            let subs = mercs2_formats::model_cubeize::sub_object_count(&l.container);
            assert_eq!(
                indx.len(),
                subs,
                "{name} P{:03}: INDX has {} rows but {subs} MESH/SKIN sub-objects — the two must \
                 agree, or the seg_id lookup is reading the wrong row",
                l.level,
                indx.len()
            );
        }
        // And every group must land on a real SEGM row.
        for r in &m.rungs {
            for d in &r.draws {
                assert!(
                    d.seg_id < m.segm.len(),
                    "{name}: group {} bound to seg_id {} outside the {}-row SEGM table",
                    d.group_index,
                    d.seg_id,
                    m.segm.len()
                );
            }
        }
    }
}

#[test]
fn lod_masks_partition_the_tiers_across_the_chain() {
    let mut w = open_vz_wad();
    let hash = mercs2_formats::hash::pandemic_hash_m2("ch_veh_tank_ztz98");
    let m = Model::load(&mut w, hash).expect("tank assembles");

    // Each block claims a band: resident owns the far tiers (4-6), the streamed ones the near tiers.
    // A rung whose segments resolved against the WRONG SEGM comes back all-zero — that was the bug.
    let by_level = |lv: u8| m.rungs.iter().find(|r| r.level == lv).expect("rung").lod_bits();
    assert_eq!(by_level(0) & 0x70, 0x70, "resident block serves the far tiers");
    assert!(by_level(2) & 0x03 != 0, "the finest block serves the near tiers");
    assert!(m.rungs.iter().all(|r| r.lod_bits() != 0), "no rung resolves to a zero mask");
}

/// Bounding-box centre of one draw's triangles, posed by its rung's REST palette (`stats.bones`).
fn rest_center(r: &mercs2_engine::model::Rung, d: &mercs2_engine::mesh::DrawGroup) -> [f32; 3] {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for k in d.index_start..d.index_start + d.index_count {
        let v = &r.vertices[r.indices[k as usize] as usize];
        let mut p = [0f32; 3];
        for j in 0..4 {
            let w = v.weights[j] as f32 / 255.0;
            if w == 0.0 {
                continue;
            }
            let s = &r.stats.bones[v.joints[j] as usize];
            for a in 0..3 {
                p[a] += w * (v.pos[0] * s[0][a] + v.pos[1] * s[1][a] + v.pos[2] * s[2][a] + s[3][a]);
            }
        }
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5]
}

#[test]
fn every_segm_row_draws_including_instance_rows() {
    // Retail's draw loop (`FUN_00472a50`) walks SEGM ROWS: row r draws `renderable[seg_id]` at its own
    // node. The M1A2's rows 74..99 re-draw its one road-wheel mesh (segs 72/73) on the other 13 hubs;
    // reaching SEGM only through INDX drew 74 of its 100 rows and left 13 wheels off.
    let mut w = open_vz_wad();
    let m = Model::load(&mut w, 0xCE77_D4CB).expect("al_veh_tank_m1a2 assembles");
    assert_eq!(m.segm.len(), 100);
    let drawn: std::collections::BTreeSet<usize> = m.rungs.iter().flat_map(|r| r.draws.iter().map(|d| d.segm_row)).collect();
    let missing: Vec<usize> = (0..m.segm.len()).filter(|i| !drawn.contains(i)).collect();
    assert!(missing.is_empty(), "SEGM rows with no draw: {missing:?}");
    let instances = m.rungs.iter().flat_map(|r| &r.draws).filter(|d| d.segm_row != d.seg_id).count();
    assert_eq!(instances, 26, "13 instanced road wheels x 2 LOD tiers");
}

#[test]
fn instanced_road_wheels_rest_on_their_own_hubs() {
    // Retail's live per-node matrices are the `+16` chain (see `mercs2_probe`
    // `retail_node_matrices_are_the_plus16_chain`); an instance node's `+80` is a COPY of its source's,
    // so an identity rest palette stacked all 14 near-tier wheels on one hub. Under the rest palette
    // each sits on its own: seven per side at x = +-1.45.
    let mut w = open_vz_wad();
    let m = Model::load(&mut w, 0xCE77_D4CB).expect("al_veh_tank_m1a2 assembles");
    let mut hubs: Vec<[i32; 3]> = Vec::new();
    for r in &m.rungs {
        for d in r.draws.iter().filter(|d| d.seg_id == 72) {
            let c = rest_center(r, d);
            assert!((c[0].abs() - 1.45).abs() < 0.01, "wheel row {} off the track line: {c:?}", d.segm_row);
            hubs.push([(c[0] * 100.0).round() as i32, (c[1] * 100.0).round() as i32, (c[2] * 100.0).round() as i32]);
        }
    }
    hubs.sort_unstable();
    hubs.dedup();
    assert_eq!(hubs.len(), 14, "14 road wheels on 14 distinct hubs: {hubs:?}");
}
