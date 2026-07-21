//! Does the INJECTED block decode back to the bones char_skin intended?
//!
//! A wrong bone binding is INVISIBLE at bind pose (every bone transform is identity there) and
//! catastrophic the moment animation moves bones — the reported "looks right standing, explodes
//! when animated". So compare what the proven reader (`model_cubeize::read_model_meshes`, which
//! expands the per-group INFO(56) palette to GLOBAL HIER indices exactly as the engine does)
//! recovers from the injected block against the mapping `char_skin` meant to write.
//!
//!   verify_skin_roundtrip <model.glb> <donor.block> <injected.bin> <group_ordinal>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: verify_skin_roundtrip <model.glb> <donor.block> <injected.bin> <group>");
        std::process::exit(2);
    }
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor = std::fs::read(&a[2]).expect("donor");
    let injected = std::fs::read(&a[3]).expect("injected");
    let want_group: usize = a[4].parse().expect("group ordinal");

    let skel = Skeleton::from_block(&donor).expect("donor skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");

    // INTENDED: per source vertex, the global HIER bone each influence should land on.
    let intended: Vec<[Option<u16>; 4]> = (0..cs.stats.verts)
        .map(|vi| {
            let mut o = [None; 4];
            for k in 0..4 {
                if glb.vweights[vi][k] > 0.0 {
                    o[k] = cs.full.get(&(glb.vjoints[vi][k] as usize)).map(|&h| h as u16);
                }
            }
            o
        })
        .collect();

    // ACTUAL: what the reader recovers from the injected container (palette already expanded).
    let ucfx_len = u32::from_le_bytes(injected[16..20].try_into().unwrap()) as usize;
    let container = &injected[20..20 + ucfx_len];
    let meshes = read_model_meshes(container).expect("read meshes");
    println!("injected container: {} meshes", meshes.len());
    for m in &meshes {
        println!(
            "  group {:2}  verts {:6}  tris {:6}  rigid {}  bone {}  joints {}",
            m.group_index,
            m.positions.len(),
            m.tris.len(),
            m.rigid,
            m.bone,
            if m.joints.is_empty() { "NONE" } else { "yes" }
        );
    }
    let Some(mesh) = meshes.iter().find(|m| m.group_index == want_group) else {
        println!("group {want_group} not found in injected container");
        return;
    };
    if mesh.joints.is_empty() {
        println!("group {want_group} decoded with NO BLENDINDICES — engine would skin it to bone 0");
        return;
    }

    println!(
        "\ncomparing {} source verts vs {} decoded verts in group {want_group}",
        intended.len(),
        mesh.joints.len()
    );
    let n = intended.len().min(mesh.joints.len());
    // SET comparison, not positional. `skin_bytes` is sorted by WEIGHT DESC and duplicate slots
    // are merged, so index k of the written stream is NOT source influence k. Comparing
    // positionally invents mismatches (the same trap that produced 625 phantom hits in char_diag).
    let (mut ok, mut bad) = (0usize, 0usize);
    let mut examples = Vec::new();
    let mut bone_swaps: HashMap<(String, String), usize> = HashMap::new();
    for vi in 0..n {
        let mut want: Vec<u16> = intended[vi].iter().filter_map(|o| *o).collect();
        want.sort_unstable();
        want.dedup();
        let mut got: Vec<u16> = (0..4)
            .filter(|&k| mesh.weights.get(vi).map_or(true, |w| w[k] > 0))
            .map(|k| mesh.joints[vi][k] as u16)
            .collect();
        got.sort_unstable();
        got.dedup();
        if want == got {
            ok += 1;
        } else {
            bad += 1;
            *bone_swaps.entry((format!("{want:?}"), format!("{got:?}"))).or_default() += 1;
            if examples.len() < 8 {
                examples.push(format!("  v{vi}: intended bones {want:?}, decoded {got:?}"));
            }
        }
    }
    // Which intended bones does the written palette actually cover?
    let covered: std::collections::HashSet<u16> = cs
        .ranges
        .iter()
        .flat_map(|&(b, c)| (b..b + c).collect::<Vec<u16>>())
        .collect();
    let mut want_bones: Vec<u16> = intended.iter().flatten().filter_map(|o| *o).collect();
    want_bones.sort_unstable();
    want_bones.dedup();
    let missing: Vec<u16> = want_bones.iter().copied().filter(|b| !covered.contains(b)).collect();
    println!("palette ranges      : {:?}", cs.ranges);
    println!("palette covers      : {} bones", covered.len());
    println!("intended bones used : {} distinct", want_bones.len());
    println!("UNCOVERED by palette: {} -> {:?}", missing.len(), missing);
    println!("vertices whose BONE SET matches : {ok}");
    println!("vertices with a WRONG bone set   : {bad}");
    if bad > 0 {
        for e in &examples {
            println!("{e}");
        }
        let mut sw: Vec<_> = bone_swaps.into_iter().collect();
        sw.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
        println!("  most common (intended -> decoded):");
        for ((w, g), c) in sw.into_iter().take(8) {
            println!("    {w} -> {g}   x{c}");
        }
        println!("\nVERDICT: palette expansion does NOT reproduce the intended bones.");
        println!("         Bind pose hides this; animation will tear the mesh apart.");
    } else {
        println!("\nVERDICT: every influence decodes to the intended bone. Skinning is faithful;");
        println!("         an animation blow-up must come from elsewhere (clip/skeleton binding).");
    }
}
