//! Parity test: `char_skin::automap` must reproduce the `mercs2-mesher` auto-mapper
//! byte-for-byte on its five committed fixture rigs (19 / 83 / 94 / 119 / 266 joints).
//! The mesher's JS/Python is the specification; these `expect_automap_*.json` files are
//! its recorded output. A divergence here means the hand-ported classifier drifted.

use mercs2_formats::char_skin::automap::{automap, Rig};
use std::collections::HashMap;

/// Minimal glTF node graph reader — just enough to reproduce `glb.js`: skin joints, node
/// names, and node parents (from `children`). Mirrors the mesher's `automap()` input.
fn load_rig(path: &str) -> (Vec<usize>, Vec<i32>, Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap();
    let g: serde_json::Value = serde_json::from_str(&text).unwrap();
    let nodes = g["nodes"].as_array().unwrap();
    let n = nodes.len();
    let mut node_parent = vec![-1i32; n];
    let mut node_name = vec![String::new(); n];
    for (i, nd) in nodes.iter().enumerate() {
        node_name[i] = nd["name"].as_str().unwrap_or("").to_string();
        if let Some(children) = nd["children"].as_array() {
            for c in children {
                node_parent[c.as_u64().unwrap() as usize] = i as i32;
            }
        }
    }
    let joints: Vec<usize> = g["skins"][0]["joints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j.as_u64().unwrap() as usize)
        .collect();
    (joints, node_parent, node_name)
}

fn check(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let (joint_nodes, node_parent, node_name) = load_rig(&format!("{dir}/rig_{name}.json"));
    let rig = Rig {
        joint_nodes: &joint_nodes,
        node_parent: &node_parent,
        node_name: &node_name,
    };
    let am = automap(&rig);

    let expect: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/expect_automap_{name}.json")).unwrap())
            .unwrap();

    // names in joint order must match
    let exp_names: Vec<String> = expect["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(am.names, exp_names, "{name}: joint names/order differ");

    let to_map = |v: &serde_json::Value| -> HashMap<usize, u32> {
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, val)| (k.parse::<usize>().unwrap(), val.as_u64().unwrap() as u32))
            .collect()
    };
    let exp_mapped = to_map(&expect["map"]);
    let exp_inherited = to_map(&expect["inherited"]);

    assert_eq!(am.mapped, exp_mapped, "{name}: direct mapping differs");
    assert_eq!(am.inherited, exp_inherited, "{name}: inherited mapping differs");
}

/// The workshop's UI mapper (`Retarget::remap_via_char_skin`) feeds `char_skin::automap` a
/// JOINT-SPACE rig — `joint_nodes` identity, `node_parent` = each joint's nearest JOINT ancestor
/// (spacer nodes collapsed, as the importer's `skin_parents` provides). This asserts that collapse
/// still reproduces Logan's map, so the workshop UI matches the full-node-graph faithful path.
fn check_joint_space(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let (joint_nodes, node_parent, node_name) = load_rig(&format!("{dir}/rig_{name}.json"));
    let node_to_joint: HashMap<usize, usize> =
        joint_nodes.iter().enumerate().map(|(j, &n)| (n, j)).collect();
    // joint index -> nearest-JOINT-ancestor joint index (-1 = root); collapse spacer nodes.
    let joint_parent: Vec<i32> = joint_nodes
        .iter()
        .map(|&node| {
            let mut cur = node_parent[node];
            while cur >= 0 {
                if let Some(&j) = node_to_joint.get(&(cur as usize)) {
                    return j as i32;
                }
                cur = node_parent[cur as usize];
            }
            -1
        })
        .collect();
    let joint_names: Vec<String> = joint_nodes.iter().map(|&n| node_name[n].clone()).collect();
    let ident: Vec<usize> = (0..joint_nodes.len()).collect();
    let am = automap(&Rig {
        joint_nodes: &ident,
        node_parent: &joint_parent,
        node_name: &joint_names,
    });
    let expect: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/expect_automap_{name}.json")).unwrap())
            .unwrap();
    let to_map = |v: &serde_json::Value| -> HashMap<usize, u32> {
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, val)| (k.parse::<usize>().unwrap(), val.as_u64().unwrap() as u32))
            .collect()
    };
    assert_eq!(am.mapped, to_map(&expect["map"]), "{name}: joint-space direct mapping differs");
    assert_eq!(am.inherited, to_map(&expect["inherited"]), "{name}: joint-space inherited differs");
}

#[test]
fn automap_joint_space_riggedfigure() {
    // RiggedFigure: torso_joint_1 PARENTS the legs → HIPS (not spine1); torso_2/3 → spine1/spine2.
    check_joint_space("riggedfigure");
}
#[test]
fn automap_joint_space_50cent() {
    check_joint_space("50cent");
}
#[test]
fn automap_joint_space_vietnam() {
    check_joint_space("vietnam");
}

#[test]
fn automap_parity_riggedfigure() {
    check("riggedfigure");
}
#[test]
fn automap_parity_50cent() {
    check("50cent");
}
#[test]
fn automap_parity_crosby() {
    check("crosby");
}
#[test]
fn automap_parity_mechanicgirl() {
    check("mechanicgirl");
}
#[test]
fn automap_parity_vietnam() {
    check("vietnam");
}
