//! What a CHARACTER actually gives us for an animation export: skinning + clips.
//!
//! The workshop can PLAY clips but exports none of it, and before wiring glTF `skins`/`animations`
//! we need the ground truth on two things the exporter's correctness depends on:
//!   1. Is a character's geometry SKINNED (model space + joints/weights, deformed by the palette) or
//!      RIGID-MOUNTED (baked into a HIER node's space)? The bundle currently assumes the latter for
//!      every group with `node >= 0`, which would be wrong for skinned verts.
//!   2. Do the clips DECODE, and do their tracks bind to this model's HIER?
//!
//! usage: charanim [0xMODELHASH]   (default: pmc_hum_mattias_v3)

use mercs2_engine::{game_world, model::Model, wad};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhash = args
        .get(1)
        .and_then(|a| a.strip_prefix("0x"))
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or(0xA3C1_FABC); // pmc_hum_mattias_v3
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    let m = Model::load(&mut w, mhash).expect("load model");
    let (verts, _indices, draws, stats) = m.flatten();
    println!("model 0x{mhash:08X}: {} rung(s), {} draws, {} verts, {} bones in rig",
        m.rungs.len(), draws.len(), verts.len(), stats.rig.len());

    // (1) Skinned or rigid? A skinned vertex has a nonzero BLENDWEIGHT.
    let skinned = verts.iter().filter(|v| v.weights.iter().any(|&x| x > 0)).count();
    let max_joint = verts.iter().flat_map(|v| v.joints).max().unwrap_or(0);
    println!("\n-- skinning --");
    println!("verts with a nonzero BLENDWEIGHT: {skinned}/{} ({:.1}%)  max joint index: {max_joint}",
        verts.len(), 100.0 * skinned as f32 / verts.len().max(1) as f32);

    // The flag the exporter branches on: a group that kept its OWN blend data (a real deforming
    // skin) vs one this builder baked into its bone's space and bound 100% to it.
    let sk_groups = draws.iter().filter(|d| d.skinned).count();
    let sk_tris: u32 = draws.iter().filter(|d| d.skinned).map(|d| d.index_count / 3).sum();
    let all_tris: u32 = draws.iter().map(|d| d.index_count / 3).sum();
    println!(
        "DEFORMING draw groups: {sk_groups}/{} ({sk_tris}/{all_tris} tris = {:.1}%) -> exporter picks the {} path",
        draws.len(),
        100.0 * sk_tris as f32 / all_tris.max(1) as f32,
        if sk_groups > 0 { "SKIN" } else { "rigid node-parented" }
    );

    // Which draw groups are bound to a HIER node? The bundle bakes those into node-local space.
    let mut by_node: std::collections::BTreeMap<i16, (usize, usize)> = std::collections::BTreeMap::new();
    for d in &draws {
        let e = by_node.entry(d.node).or_insert((0, 0));
        e.0 += 1;
        e.1 += (d.index_count / 3) as usize;
    }
    println!("draw groups by SEGM node (node -1 = no mount):");
    for (node, (n, tris)) in by_node.iter().take(12) {
        println!("   node {node:>4}: {n:>3} groups, {tris:>6} tris");
    }
    if by_node.len() > 12 {
        println!("   ... {} more nodes", by_node.len() - 12);
    }

    // (2) Clips.
    println!("\n-- clips (game_world::load_clips_for_model, rig-matched) --");
    let clips = game_world::load_clips_for_model(&mut w, &stats.rig);
    println!("{} clip(s) resolve against this rig", clips.len());
    let (mut decoded, mut neutral) = (0u32, 0u32);
    for c in &clips {
        if c.clip.decoded { decoded += 1 } else { neutral += 1 }
    }
    println!("decoded: {decoded}   NOT decoded (would export as neutral pose): {neutral}");
    for c in clips.iter().take(8) {
        let bound = c.track_to_hier.iter().take(c.num_transform_tracks).filter(|x| x.is_some()).count();
        println!(
            "   0x{:08X}  {:>6.2}s  {:>4} frames  {:>3} tracks ({bound} bound to HIER)  decoded={}",
            c.name_hash, c.clip.duration, c.clip.num_frames, c.num_transform_tracks, c.clip.decoded
        );
    }
    if clips.len() > 8 {
        println!("   ... {} more", clips.len() - 8);
    }
}
