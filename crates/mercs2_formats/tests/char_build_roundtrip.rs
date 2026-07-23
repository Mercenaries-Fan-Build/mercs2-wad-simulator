//! Palette round-trip + build invariants for `char_skin::build_character`.
//!
//! Primary RE proof (palette level): the writer's `INFO(56)` range table, expanded by the
//! SAME algorithm `model_cubeize` uses when reading, must reproduce EXACTLY the writer's
//! `slot → HIER` map. Writer = the documented inverse of the proven reader. The full
//! byte-level round-trip through `model_cubeize::read_model_meshes` lives with the injector.

use mercs2_formats::char_skin::automap::Rig;
use mercs2_formats::char_skin::build::{build_character, BuildInput, TargetBone, TargetSkeleton};
use mercs2_formats::char_skin::{expand_ranges, validate};
use std::collections::HashMap;

fn load_skeleton() -> TargetSkeleton {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/skeleton_npc84.json")).unwrap())
            .unwrap();
    let bones = j["bones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| TargetBone {
            i: b["i"].as_u64().unwrap() as u32,
            pos: {
                let p = b["pos"].as_array().unwrap();
                [
                    p[0].as_f64().unwrap(),
                    p[1].as_f64().unwrap(),
                    p[2].as_f64().unwrap(),
                ]
            },
            parent: b["parent"].as_i64().unwrap() as i32,
            name: b["name"].as_str().unwrap().to_string(),
            name_hash: mercs2_formats::hash::pandemic_hash_m2(b["name"].as_str().unwrap()),
            rot: None,
        })
        .collect();
    TargetSkeleton {
        bones,
        height: j["height"].as_f64().unwrap(),
    }
}

/// A simple 17-joint humanoid whose names automap resolves 1:1 to known HIER indices.
/// node index == joint index; parent chain given. Source bind = target position (so the
/// estimated similarity ≈ identity and the re-pose leaves the mesh roughly in place).
struct SynthRig {
    names: Vec<String>,
    parents: Vec<i32>,
    /// expected joint→HIER (for asserting automap on this rig).
    expect: HashMap<usize, u32>,
}

fn synth_rig() -> SynthRig {
    let rows: [(&str, i32, u32); 17] = [
        ("Hips", -1, 3),
        ("Spine", 0, 14),
        ("Chest", 1, 16),
        ("Neck", 2, 20),
        ("Head", 3, 21),
        ("LeftUpperArm", 2, 43),
        ("LeftForearm", 5, 44),
        ("LeftHand", 6, 46),
        ("RightUpperArm", 2, 64),
        ("RightForearm", 8, 65),
        ("RightHand", 9, 67),
        ("LeftThigh", 0, 6),
        ("LeftShin", 11, 7),
        ("LeftFoot", 12, 8),
        ("RightThigh", 0, 10),
        ("RightShin", 14, 11),
        ("RightFoot", 15, 12),
    ];
    let names = rows.iter().map(|r| r.0.to_string()).collect();
    let parents = rows.iter().map(|r| r.1).collect();
    let expect = rows.iter().enumerate().map(|(i, r)| (i, r.2)).collect();
    SynthRig { names, parents, expect }
}

/// Row-major translation matrix.
fn translate(p: [f64; 3]) -> [f64; 16] {
    [
        1.0, 0.0, 0.0, p[0], 0.0, 1.0, 0.0, p[1], 0.0, 0.0, 1.0, p[2], 0.0, 0.0, 0.0, 1.0,
    ]
}
/// Inverse of a pure translation.
fn inv_translate(p: [f64; 3]) -> [f64; 16] {
    translate([-p[0], -p[1], -p[2]])
}

struct Built {
    input_joints: Vec<[u16; 4]>,
    input_weights: Vec<[f64; 4]>,
    indices: Vec<u32>,
    positions: Vec<[f64; 3]>,
}

fn make_mesh() -> Built {
    // five vertices exercising single + multi influence across body regions.
    let input_joints = vec![
        [0u16, 0, 0, 0],   // hips
        [5, 6, 0, 0],      // L upper arm + forearm
        [4, 0, 0, 0],      // head
        [7, 10, 0, 0],     // L hand + R hand
        [11, 12, 13, 0],   // L thigh/shin/foot
    ];
    let input_weights = vec![
        [1.0f64, 0.0, 0.0, 0.0],
        [0.5, 0.5, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.6, 0.4, 0.0, 0.0],
        [0.4, 0.35, 0.25, 0.0],
    ];
    // positions are irrelevant to the palette/skin bytes; give them spread so triangle
    // area / height checks have something to chew on.
    let positions = vec![
        [0.0, 1.0, 0.0],
        [0.3, 1.4, 0.0],
        [0.0, 1.66, 0.0],
        [0.4, 1.2, 0.0],
        [0.1, 0.5, 0.0],
    ];
    let indices = vec![0u32, 1, 2, 2, 3, 4];
    Built {
        input_joints,
        input_weights,
        indices,
        positions,
    }
}

fn build(sr: &SynthRig, sk: &TargetSkeleton, mesh: &Built) -> mercs2_formats::char_skin::CharSkin {
    let n = sr.names.len();
    let joint_nodes: Vec<usize> = (0..n).collect();
    // per-joint bind position = its target HIER position (clean similarity fit)
    let node_world: Vec<[f64; 16]> = (0..n)
        .map(|j| {
            let h = sr.expect[&j];
            translate(sk.tgt(h).unwrap())
        })
        .collect();
    let ibm: Vec<Option<[f64; 16]>> = (0..n)
        .map(|j| {
            let h = sr.expect[&j];
            Some(inv_translate(sk.tgt(h).unwrap()))
        })
        .collect();
    // node children from parents
    let mut node_children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (c, &p) in sr.parents.iter().enumerate() {
        if p >= 0 {
            node_children[p as usize].push(c);
        }
    }
    let rig = Rig {
        joint_nodes: &joint_nodes,
        node_parent: &sr.parents,
        node_name: &sr.names,
    };
    let inp = BuildInput {
        rig,
        positions: &mesh.positions,
        // The round-trip checks positions and skin bytes; no normals to conform.
        normals: &[],
        vjoints: &mesh.input_joints,
        vweights: &mesh.input_weights,
        indices: &mesh.indices,
        node_world: &node_world,
        node_children: &node_children,
        ibm: &ibm,
        skeleton: sk,
        container_verts: None,
        overrides: HashMap::new(),
        shared_bind_anchor: false,
    };
    build_character(&inp).expect("build_character")
}

#[test]
fn index_by_canonical_resolves_onto_shifted_hero() {
    // A HERO donor reorders/extends the HIER. Build one by prepending two bones to the NPC-84
    // skeleton (every real bone shifts +2) and confirm a canonical automap index re-seats onto the
    // hero's OWN index by name — the resolution that keeps 50 Cent → mattias from scrambling, and
    // (composed with the NPC-84 finger-collapse) keeps its 58-bone map under the palette cap.
    let npc = load_skeleton();
    let mut bones = vec![
        TargetBone { i: 0, pos: [0.0; 3], parent: -1, name: "x_root_a".into(), name_hash: 0xA, rot: None },
        TargetBone { i: 1, pos: [0.0; 3], parent: 0, name: "x_root_b".into(), name_hash: 0xB, rot: None },
    ];
    for b in &npc.bones {
        bones.push(TargetBone {
            i: b.i + 2,
            pos: b.pos,
            parent: if b.parent < 0 { -1 } else { b.parent + 2 },
            name: b.name.clone(),
            name_hash: b.name_hash,
            rot: b.rot,
        });
    }
    let hero = TargetSkeleton { bones, height: npc.height };
    // NPC skeleton: canonical resolution is the identity.
    assert_eq!(npc.index_by_canonical(3), Some(3), "NPC Bone_Hips");
    // Hero skeleton: every bone re-seats +2 by NAME.
    assert_eq!(hero.index_by_canonical(3), Some(5), "hero Bone_Hips");
    assert_eq!(hero.index_by_canonical(46), Some(48), "hero bone_lhand");
    assert_eq!(hero.index_by_canonical(48), Some(50), "hero bone_lindex1 (finger)");
    assert_eq!(hero.index_by_canonical(67), Some(69), "hero bone_rhand");
}

#[test]
fn automap_resolves_synthetic_rig() {
    let sr = synth_rig();
    let sk = load_skeleton();
    let cs = build(&sr, &sk, &make_mesh());
    for (j, h) in &sr.expect {
        assert_eq!(cs.full.get(j), Some(h), "joint {j} mapped wrong");
    }
}

#[test]
fn palette_expand_is_exact_inverse_of_slot_map() {
    // The core RE proof: expanding the writer's ranges with the reader's algorithm must
    // reproduce the writer's slot→HIER map exactly, for EVERY slot.
    let sr = synth_rig();
    let sk = load_skeleton();
    let cs = build(&sr, &sk, &make_mesh());
    let palette = expand_ranges(&cs.ranges); // slot -> HIER (reader's expansion)
    assert_eq!(palette.len(), cs.palette_slots, "palette length mismatch");
    for (&hier, &slot) in &cs.slot_of {
        assert_eq!(
            palette[slot as usize] as u32, hier,
            "range table does not invert slot_of at slot {slot}"
        );
    }
    // ranges are within the reader's gate (1..=8 runs, total <= 256)
    assert!(!cs.ranges.is_empty() && cs.ranges.len() <= 8);
    assert!(cs.palette_slots <= 256);
}

#[test]
fn blendindices_decode_to_intended_bones() {
    // Every non-zero-weight BLENDINDICES byte must be a valid palette slot whose expansion
    // is one of the HIER bones the source vertex was actually weighted to.
    let sr = synth_rig();
    let sk = load_skeleton();
    let mesh = make_mesh();
    let cs = build(&sr, &sk, &mesh);
    let palette = expand_ranges(&cs.ranges);
    for vi in 0..mesh.positions.len() {
        // intended HIER set for this vertex
        let intended: std::collections::HashSet<u32> = (0..4)
            .filter(|&k| mesh.input_weights[vi][k] > 0.0)
            .filter_map(|k| cs.full.get(&(mesh.input_joints[vi][k] as usize)).copied())
            .collect();
        for k in 0..4 {
            let slot = cs.skin_bytes[vi * 8 + k];
            let w = cs.skin_bytes[vi * 8 + 4 + k];
            if w == 0 {
                continue;
            }
            let hier = palette[slot as usize] as u32;
            assert!(
                intended.contains(&hier),
                "vertex {vi}: slot {slot} -> HIER {hier} not in intended {intended:?}"
            );
        }
        // weights sum to exactly 255
        let sum: u32 = (0..4).map(|k| cs.skin_bytes[vi * 8 + 4 + k] as u32).sum();
        assert_eq!(sum, 255, "vertex {vi} weight sum");
    }
}

#[test]
fn validate_battery_passes_on_clean_build() {
    let sr = synth_rig();
    let sk = load_skeleton();
    let mesh = make_mesh();
    let cs = build(&sr, &sk, &mesh);
    let report = validate::validate(&cs, &mesh.input_joints, &mesh.input_weights, &mesh.indices);
    // static limits all satisfied on a clean small build
    for l in &report.limits {
        assert!(l.ok, "limit {} failed: {}", l.id, l.text);
    }
}

#[test]
fn hips_collapse_paradox_is_caught() {
    // Force every joint onto the hips: bone-distance RATES this catastrophe well (every
    // vertex genuinely is near its single bone), but influence + palette must expose it.
    // Pinned so the battery can never be reduced to bone-distance alone.
    let sr = synth_rig();
    let sk = load_skeleton();
    let mesh = make_mesh();
    let n = sr.names.len();
    let joint_nodes: Vec<usize> = (0..n).collect();
    let node_world: Vec<[f64; 16]> = (0..n)
        .map(|j| translate(sk.tgt(sr.expect[&j]).unwrap()))
        .collect();
    let ibm: Vec<Option<[f64; 16]>> = (0..n)
        .map(|j| Some(inv_translate(sk.tgt(sr.expect[&j]).unwrap())))
        .collect();
    let mut node_children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (c, &p) in sr.parents.iter().enumerate() {
        if p >= 0 {
            node_children[p as usize].push(c);
        }
    }
    // override: every joint -> hips (HIER 3)
    let overrides: HashMap<usize, Option<u32>> = (0..n).map(|j| (j, Some(3u32))).collect();
    let rig = Rig {
        joint_nodes: &joint_nodes,
        node_parent: &sr.parents,
        node_name: &sr.names,
    };
    let inp = BuildInput {
        rig,
        positions: &mesh.positions,
        // The round-trip checks positions and skin bytes; no normals to conform.
        normals: &[],
        vjoints: &mesh.input_joints,
        vweights: &mesh.input_weights,
        indices: &mesh.indices,
        node_world: &node_world,
        node_children: &node_children,
        ibm: &ibm,
        skeleton: &sk,
        container_verts: None,
        overrides,
        shared_bind_anchor: false,
    };
    let cs = build_character(&inp).expect("build");
    // palette collapses to a single bone
    assert_eq!(cs.palette_slots, 1, "all-hips collapse must yield 1 palette slot");
    let report = validate::validate(&cs, &mesh.input_joints, &mesh.input_weights, &mesh.indices);
    // multi-bone influence is destroyed → the influence limit must fail.
    let infl = report.limits.iter().find(|l| l.id == "influence").unwrap();
    assert!(!infl.ok, "influence check must catch the single-bone collapse");
}
