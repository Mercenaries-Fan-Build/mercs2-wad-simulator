//! The regression invariant for moving visibility from BUILD time to DRAW time.
//!
//! There is nothing to snapshot: the wad is deterministic, versioned input, and a golden capture of
//! today's output would only enshrine the bug we are fixing (every model with a destruction machine is
//! *supposed* to change). What we can assert is a property.
//!
//! For a container with **no SWIT** (no destruction state machine) clause 3 of the draw gate is
//! vacuous — every node is enabled — so the new draw-time gate at `view_state = 0x01` must select
//! exactly the segments the legacy build-time filter `build_indexed_state(c, 0x01)` baked in.
//! All characters are in this class (`pmc_hum_mattias_v3` has no SWIT/NODE at all), as are most props.
//!
//! Models *with* SWIT are expected to differ — that is the fix, not a regression.
//!
//! `#[ignore]`d: needs the retail install. Run with
//! `cargo test -p mercs2_engine --test gate_invariant_probe -- --ignored --nocapture`.

use mercs2_engine::render_state::RenderState;
use mercs2_engine::{mesh, wad};

/// How many model hashes to sweep. The whole 3,007 would decompress thousands of multi-MB blocks.
const SAMPLE: usize = 200;

fn open_base() -> Option<wad::Wad> {
    wad::open(&wad::registry_vz_wad()?).ok()
}

/// Identify the segments a build kept. NOT by `index_start`: that is an offset into the build's own
/// index buffer, and a build that keeps more geometry shifts every later offset. `(group, sub_object,
/// index_count)` names the same triangles in either build.
fn kept(draws: &[mesh::DrawGroup]) -> Vec<(usize, usize, u32)> {
    draws.iter().map(|d| (d.group_index, d.sub_object, d.index_count)).collect()
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn swit_less_models_gate_identically_at_rung_0() {
    let Some(mut w) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let hashes: Vec<u32> = wad::model_list(&w).into_iter().map(|(h, _)| h).take(SAMPLE).collect();

    let (mut checked, mut skipped_swit, mut skipped_zero_mask, mut load_fail) = (0, 0, 0, 0);
    for h in hashes {
        let Ok(c) = wad::extract_container(&mut w, h) else {
            load_fail += 1;
            continue;
        };
        if mercs2_formats::orchestrator::parse_state_machine(&c).is_some() {
            skipped_swit += 1; // clause 3 is live here; the two builders are SUPPOSED to differ
            continue;
        }
        let (Ok((_, _, legacy, _)), Ok((_, _, all, _))) =
            (mesh::build_indexed_state(&c, 0x01), mesh::build_indexed_all(&c))
        else {
            load_fail += 1;
            continue;
        };
        // The legacy builder treats mask == 0 as always-on; the engine's ANY-bit rule never draws it.
        // Those containers legitimately diverge — count them rather than pretend they match.
        if all.iter().any(|d| d.lod_mask == 0) {
            skipped_zero_mask += 1;
            continue;
        }

        // No SWIT ⇒ no node table ⇒ clause 3 passes for every segment.
        let rs = RenderState::rung0(0);
        let gated: Vec<_> =
            all.iter().filter(|d| rs.segment_visible(d.lod_mask, d.node)).cloned().collect();

        assert_eq!(
            kept(&gated),
            kept(&legacy),
            "0x{h:08X}: draw-time gate at rung 0 selected a different segment set than the \
             build-time filter did"
        );
        checked += 1;
    }

    println!(
        "gate invariant: {checked} SWIT-less models identical; \
         {skipped_swit} have a state machine (expected to differ); \
         {skipped_zero_mask} carry a mask==0 segment; {load_fail} failed to load"
    );
    assert!(checked > 20, "sample was too thin to mean anything (checked {checked})");
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn build_indexed_all_is_a_superset_of_every_rung() {
    // Whole-model upload must contain every segment any rung could ask for — otherwise moving the
    // filter to draw time silently loses geometry at some distance.
    let Some(mut w) = open_base() else { return eprintln!("no vz.wad; skipping") };
    for h in [0x9FCA_E910u32 /* md500 */, 0xA3C1_FABC /* mattias */, 0xE540_47D5 /* destroyer */] {
        let c = wad::extract_container(&mut w, h).expect("container");
        let (_, _, all, _) = mesh::build_indexed_all(&c).expect("build all");
        let total: usize = all.len();

        let mut union = std::collections::HashSet::new();
        for bit in 0..8u8 {
            let Ok((_, _, tier, _)) = mesh::build_indexed_state(&c, 1 << bit) else { continue };
            for d in &tier {
                union.insert((d.group_index, d.sub_object));
            }
        }
        let whole: std::collections::HashSet<_> =
            all.iter().map(|d| (d.group_index, d.sub_object)).collect();
        assert!(
            union.is_subset(&whole),
            "0x{h:08X}: whole-model build ({total} draws) is missing segments some rung selects: {:?}",
            union.difference(&whole).collect::<Vec<_>>()
        );
    }
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn no_view_state_can_hide_uh1s_mask_03_body_while_drawing_its_mask_01_groups() {
    // THE load-bearing claim. Field evidence: UH1 draws groups 20/24 (mask 0x01) at spawn but NOT
    // group 14 (mask 0x03). A memory concluded the rule must be all-bits `(mask & S) == mask`.
    //
    // Under the real ANY-bit rule, mask 0x03 is a SUPERSET of mask 0x01, so any `view_state` that
    // lights a 0x01 segment necessarily lights a 0x03 one. No `view_state` produces the observed
    // split. Therefore the mask never suppressed group 14 — clause 3 (the node-enable table) did.
    let Some(mut w) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let c = wad::extract_container(&mut w, 0x89D8_DE72).expect("uh1");
    let (_, _, all, _) = mesh::build_indexed_all(&c).expect("build all");

    let masks: std::collections::HashSet<u8> = all.iter().map(|d| d.lod_mask).collect();
    assert!(masks.contains(&0x01), "uh1 should carry mask-0x01 segments; got {masks:?}");
    assert!(masks.contains(&0x03), "uh1 should carry mask-0x03 segments; got {masks:?}");

    for s in 0..=u8::MAX {
        let rs = RenderState { lod: 0, view_state: s, node_enable: Vec::new() };
        if rs.segment_visible(0x01, -1) {
            assert!(
                rs.segment_visible(0x03, -1),
                "view_state 0x{s:02X} drew mask-0x01 but not mask-0x03 — the LOD mask WOULD then \
                 explain the UH1 observation, and the three-clause reading is wrong"
            );
        }
    }
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn clause_3_must_be_keyed_on_the_segm_node_not_on_indx() {
    // THE bug, pinned. Clause 3 indexes the node-enable table by the SEGM record's signed `node`
    // field. Keying it on `INDX[group_index]` instead — which is what the workshop used to do —
    // disagrees on md500 and leaves the wreck drawn next to the intact body.
    //
    // md500 draw groups 0 and 1 both sit on SEGM node 2, which the machine's DEFAULT state disables.
    // INDX maps group 0 to node 0 (enabled) and group 1 to node 2 (disabled). So the INDX keying
    // hides exactly one of the pair, and group 0 — the wreck — survives.
    let Some(mut w) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let c = wad::extract_container(&mut w, 0x9FCA_E910).expect("md500");
    let (_, _, all, _) = mesh::build_indexed_all(&c).expect("build all");

    let hier = mercs2_formats::orchestrator::parse_hier(&c);
    let indx = mercs2_formats::orchestrator::parse_indx(&c);
    let sm = mercs2_formats::orchestrator::parse_state_machine(&c).expect("md500 has a machine");
    let chosen: Vec<usize> =
        sm.nodes.iter().map(mercs2_formats::orchestrator::default_state_index).collect();
    let node_enable = mercs2_formats::orchestrator::machine_node_enable(&sm, &hier, &chosen);

    let rs = RenderState { lod: 0, view_state: 0x01, node_enable: node_enable.clone() };

    // What the LOD mask alone admits at rung 0, and what clause 3 then removes.
    let pass_mask: Vec<_> = all.iter().filter(|d| (rs.view_state & d.lod_mask) != 0).collect();
    let drawn: Vec<_> =
        pass_mask.iter().filter(|d| rs.segment_visible(d.lod_mask, d.node)).collect();
    assert_eq!(pass_mask.len(), 8, "rung 0 admits the same 8 groups the legacy builder baked in");
    assert_eq!(drawn.len(), 6, "clause 3 removes 2 of them (both on SEGM node 2)");

    // The INDX keying removes only one — that missing removal IS the wreck on screen.
    let indx_drawn = pass_mask
        .iter()
        .filter(|d| indx.get(d.group_index).is_none_or(|&n| node_enable.get(n).copied().unwrap_or(true)))
        .count();
    assert_eq!(indx_drawn, 7, "INDX keying leaves one extra group drawn");
    assert!(indx_drawn > drawn.len(), "the INDX keying is strictly more permissive — that is the bug");
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn disabling_a_node_removes_its_segments_at_every_lod_rung() {
    // Clause 3 is orthogonal to clause 2: a disabled node is gone at all rungs, near and far. This is
    // the mechanism that hides a wreck, and the one we do not implement today.
    let Some(mut w) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let c = wad::extract_container(&mut w, 0x9FCA_E910).expect("md500");
    let (_, _, all, _) = mesh::build_indexed_all(&c).expect("build all");

    let victim = all.iter().map(|d| d.node).find(|n| *n >= 0).expect("some noded segment");
    let n_nodes = all.iter().map(|d| d.node).max().unwrap_or(0).max(0) as usize + 1;
    let mut enable = vec![true; n_nodes];
    enable[victim as usize] = false;

    let mut suppressed = 0;
    for rung in 0..8u8 {
        let rs = RenderState { lod: rung, view_state: 1 << rung, node_enable: enable.clone() };
        for d in all.iter().filter(|d| d.node == victim) {
            assert!(!rs.segment_visible(d.lod_mask, d.node), "node {victim} drew at rung {rung}");
            suppressed += 1;
        }
        // …while a DIFFERENT node's segments still obey the LOD mask normally.
        for d in all.iter().filter(|d| d.node != victim && d.node >= 0) {
            assert_eq!(
                rs.segment_visible(d.lod_mask, d.node),
                (rs.view_state & d.lod_mask) != 0,
                "an enabled node's visibility must be decided by clause 2 alone"
            );
        }
    }
    assert!(suppressed > 0, "the sample node had no segments — test proves nothing");
}
