//! Regenerate an automap parity fixture from the CURRENT classifier. Use after a DELIBERATE change
//! to the automap (e.g. the forearm-twist fix), so the fixture records the corrected mapping rather
//! than the mesher's original — the divergence is intentional and documented in the automap.
//!
//!   regen_automap_fixture <rig.json> <out_expect.json>
use mercs2_formats::char_skin::automap::{automap, Rig};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let nodes = g["nodes"].as_array().unwrap();
    let n = nodes.len();
    let mut node_parent = vec![-1i32; n];
    let mut node_name = vec![String::new(); n];
    for (i, nd) in nodes.iter().enumerate() {
        node_name[i] = nd["name"].as_str().unwrap_or("").to_string();
        if let Some(ch) = nd["children"].as_array() {
            for c in ch { node_parent[c.as_u64().unwrap() as usize] = i as i32; }
        }
    }
    let joints: Vec<usize> = g["skins"][0]["joints"].as_array().unwrap().iter().map(|j| j.as_u64().unwrap() as usize).collect();
    let am = automap(&Rig { joint_nodes: &joints, node_parent: &node_parent, node_name: &node_name });
    let mut map = serde_json::Map::new();
    let mut inh = serde_json::Map::new();
    for (k, v) in &am.mapped { map.insert(k.to_string(), serde_json::json!(v)); }
    for (k, v) in &am.inherited { inh.insert(k.to_string(), serde_json::json!(v)); }
    let out = serde_json::json!({ "names": am.names, "map": map, "inherited": inh });
    std::fs::write(&a[2], serde_json::to_string_pretty(&out).unwrap()).unwrap();
    println!("wrote {} ({} mapped, {} inherited)", a[2], am.mapped.len(), am.inherited.len());
}
