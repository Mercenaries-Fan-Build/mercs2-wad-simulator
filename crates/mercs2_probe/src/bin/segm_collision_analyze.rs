//! `segm_collision_analyze` — second-pass analysis over the `segm_collision_census` JSON dump.
//! Tests the discriminating hypotheses that decide whether collision shapes are SEGM-bound at all.
//! Reproduce: cargo run -p mercs2_probe --bin segm_collision_analyze -- <census.json>

use serde_json::Value;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: segm_collision_analyze <census.json>");
    let txt = std::fs::read_to_string(&path).expect("read json");
    let models: Vec<Value> = serde_json::from_str(&txt).expect("parse json");

    let get_u = |m: &Value, k: &str| m[k].as_u64().unwrap_or(0) as usize;

    // Scope: containers carrying a static mesh collider.
    let mesh_models: Vec<&Value> = models.iter().filter(|m| get_u(m, "mesh_count") > 0).collect();
    let rich: Vec<&Value> = mesh_models.iter().copied().filter(|m| get_u(m, "segm_count") > 1).collect();
    println!("mesh-collider models: {}   rich(>1 segm): {}", mesh_models.len(), rich.len());

    // A. Distribution of segm_count for single-mesh (mesh_count==1) models — the "prop" generalization.
    let mut segm_hist_1mesh: BTreeMap<usize, usize> = BTreeMap::new();
    for m in &mesh_models {
        if get_u(m, "mesh_count") == 1 {
            *segm_hist_1mesh.entry(get_u(m, "segm_count")).or_insert(0) += 1;
        }
    }
    let single_mesh_total: usize = segm_hist_1mesh.values().sum();
    let single_mesh_1seg = *segm_hist_1mesh.get(&1).unwrap_or(&0);
    println!("\n[A] single-mesh (mesh_count==1) models: {single_mesh_total}");
    println!("    of those, segm_count==1: {single_mesh_1seg}  ({:.0}%)  -> segm_count>1: {}",
        100.0 * single_mesh_1seg as f64 / single_mesh_total.max(1) as f64,
        single_mesh_total - single_mesh_1seg);
    println!("    segm_count histogram (segm:count) for 1-mesh: {:?}",
        segm_hist_1mesh.iter().take(12).collect::<Vec<_>>());

    // D. Is mesh_count EVER > segm_count? (collision-independent-of-SEGM evidence)
    let mesh_gt_segm = mesh_models.iter().filter(|m| get_u(m, "mesh_count") > get_u(m, "segm_count")).count();
    let mesh_gt_segm_examples: Vec<(u64, usize, usize)> = mesh_models.iter()
        .filter(|m| get_u(m, "mesh_count") > get_u(m, "segm_count"))
        .take(10)
        .map(|m| (m["name_hash"].as_u64().unwrap_or(0), get_u(m, "mesh_count"), get_u(m, "segm_count")))
        .collect();
    println!("\n[D] models with mesh_count > segm_count: {mesh_gt_segm}");
    for (h, me, se) in &mesh_gt_segm_examples {
        println!("    0x{h:08X}  mesh={me} segm={se}");
    }

    // B. Does any SINGLE state_mask value select exactly mesh_count records?  (dedicated collision tier)
    //    Test both over ALL records and over UNREFERENCED records.
    let mut b_any = 0usize;      // some mask selects exactly mesh_count records (all)
    let mut b_unref = 0usize;    // some mask selects exactly mesh_count UNref records
    // C. distinct-bone tests.
    let mut c_all_distinct = 0usize;   // mesh_count == distinct bones over all segm
    let mut c_unref_distinct = 0usize; // mesh_count == distinct bones over unref segm
    // E. mask that appears ONLY in unreferenced (dedicated) and selects exactly mesh_count.
    let mut e_dedicated_tier = 0usize;

    for m in &rich {
        let mc = get_u(m, "mesh_count");
        let segm = m["segm"].as_array().unwrap();
        let mut mask_all: BTreeMap<u64, usize> = BTreeMap::new();
        let mut mask_unref: BTreeMap<u64, usize> = BTreeMap::new();
        let mut bones_all: std::collections::BTreeSet<u64> = Default::default();
        let mut bones_unref: std::collections::BTreeSet<u64> = Default::default();
        let mut ref_masks: std::collections::BTreeSet<u64> = Default::default();
        for r in segm {
            let mask = r["mask"].as_u64().unwrap();
            let bone = r["bone"].as_u64().unwrap();
            let referenced = r["referenced"].as_bool().unwrap();
            *mask_all.entry(mask).or_insert(0) += 1;
            bones_all.insert(bone);
            if referenced {
                ref_masks.insert(mask);
            } else {
                *mask_unref.entry(mask).or_insert(0) += 1;
                bones_unref.insert(bone);
            }
        }
        if mask_all.values().any(|&c| c == mc) { b_any += 1; }
        if mask_unref.values().any(|&c| c == mc) { b_unref += 1; }
        if bones_all.len() == mc { c_all_distinct += 1; }
        if bones_unref.len() == mc { c_unref_distinct += 1; }
        // dedicated tier: a mask present only in unref (not in ref) whose unref-count == mesh_count
        if mask_unref.iter().any(|(mask, &c)| c == mc && !ref_masks.contains(mask)) {
            e_dedicated_tier += 1;
        }
    }
    let nr = rich.len().max(1);
    println!("\n[B] rich models where SOME state_mask selects exactly mesh_count records:");
    println!("    over ALL segm records ....... {b_any}/{nr}  ({:.0}%)", 100.0*b_any as f64/nr as f64);
    println!("    over UNREFERENCED records ... {b_unref}/{nr}  ({:.0}%)", 100.0*b_unref as f64/nr as f64);
    println!("[C] rich models where mesh_count == distinct bones:");
    println!("    over ALL segm ............... {c_all_distinct}/{nr}  ({:.0}%)", 100.0*c_all_distinct as f64/nr as f64);
    println!("    over UNREFERENCED segm ...... {c_unref_distinct}/{nr}  ({:.0}%)", 100.0*c_unref_distinct as f64/nr as f64);
    println!("[E] rich models with a DEDICATED unref-only mask selecting exactly mesh_count: {e_dedicated_tier}/{nr}  ({:.0}%)",
        100.0*e_dedicated_tier as f64/nr as f64);

    // F. Global mask histograms, split referenced vs unreferenced (what tiers does render use vs the rest).
    let mut ref_mask_hist: BTreeMap<u64, usize> = BTreeMap::new();
    let mut unref_mask_hist: BTreeMap<u64, usize> = BTreeMap::new();
    for m in &mesh_models {
        for r in m["segm"].as_array().unwrap() {
            let mask = r["mask"].as_u64().unwrap();
            if r["referenced"].as_bool().unwrap() {
                *ref_mask_hist.entry(mask).or_insert(0) += 1;
            } else {
                *unref_mask_hist.entry(mask).or_insert(0) += 1;
            }
        }
    }
    println!("\n[F] global state_mask histogram (mesh-collider models):");
    println!("    REFERENCED (render-drawn here): {:?}", fmt_hist(&ref_mask_hist));
    println!("    UNREFERENCED (rest)           : {:?}", fmt_hist(&unref_mask_hist));

    // G. mesh_count vs render structure correlation (does collision track render sub-objects?)
    let (mut sum_mesh, mut sum_sub, mut sum_cvx, mut sum_segm) = (0usize, 0usize, 0usize, 0usize);
    for m in &rich {
        sum_mesh += get_u(m, "mesh_count");
        sum_sub += get_u(m, "sub_objects");
        sum_cvx += get_u(m, "convex_count");
        sum_segm += get_u(m, "segm_count");
    }
    println!("\n[G] rich totals: mesh={sum_mesh} sub_objects={sum_sub} convex={sum_cvx} segm={sum_segm}");
    println!("    mean mesh/model={:.2} sub/model={:.2} cvx/model={:.2} segm/model={:.2}",
        sum_mesh as f64/nr as f64, sum_sub as f64/nr as f64, sum_cvx as f64/nr as f64, sum_segm as f64/nr as f64);

    // H. Known-model spot checks.
    println!("\n[H] known models:");
    for &want in &[0x39AF17DCu64, 0x86D7CF92, 0xE8EB75D7, 0x09E169B3] {
        if let Some(m) = models.iter().find(|m| m["name_hash"].as_u64() == Some(want)) {
            let masks: Vec<(u64,u64,bool)> = m["segm"].as_array().unwrap().iter()
                .map(|r| (r["bone"].as_u64().unwrap(), r["mask"].as_u64().unwrap(), r["referenced"].as_bool().unwrap()))
                .collect();
            println!("    0x{want:08X} blk={} segm={} sub={} mesh={} mopp={} cvx={} compl={}",
                get_u(m,"block"), get_u(m,"segm_count"), get_u(m,"sub_objects"),
                get_u(m,"mesh_count"), get_u(m,"mopp_count"), get_u(m,"convex_count"),
                m["complement_seg_ids"].as_array().map(|a|a.len()).unwrap_or(0));
            println!("        (bone,mask,ref): {:?}", &masks[..masks.len().min(20)]);
        } else {
            println!("    0x{want:08X} not in census");
        }
    }
}

fn fmt_hist(h: &BTreeMap<u64, usize>) -> Vec<(String, usize)> {
    h.iter().map(|(k, v)| (format!("0x{k:02X}"), *v)).collect()
}
