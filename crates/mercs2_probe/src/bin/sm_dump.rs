//! Disassemble a model's destruction state machine into something a human can read: every switch
//! node, every state (named from the cracked vocabulary), every command, with each node-hash
//! argument resolved to the HIER node it names and the geometry that node actually carries.
//!
//! This exists because our gate was reverse-engineered against a 4-wheeler and inverts on the tank
//! and the helicopter. Read the machine instead of guessing at it.
//!
//!   cargo run -p mercs2_probe --bin sm_dump -- ch_veh_tank_ztz98

use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;
use std::collections::HashMap;

fn state_name(h: u32) -> &'static str {
    match h {
        0x0ACE_072A => "InitState",
        0xACB5_1200 => "PristineState",
        0x1D55_75A1 => "DamagedState",
        0x5D30_8F4F => "InitDestroyedState",
        0x7687_DF41 => "DestroyedState",
        0x9279_1EBB => "StartDestroyedState",
        0xCA26_1E5B => "GoneState",
        0xC650_7EE1 => "DamageMsg",
        0x1ED7_AD78 => "DestroyMsg",
        0x3D0D_4C99 => "DestroyMsg2",
        _ => "",
    }
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");
    let (_, _, draws, _) = mesh::build_indexed_all(&c).expect("build");
    let hier = orch::parse_hier(&c);
    let sm = orch::parse_state_machine(&c).expect("state machine");

    // node hash -> (index, triangles, OR of lod masks)
    let mut geo: HashMap<usize, (u32, u8)> = HashMap::new();
    for d in draws.iter().filter(|d| d.node >= 0) {
        let e = geo.entry(d.node as usize).or_insert((0, 0));
        e.0 += d.index_count / 3;
        e.1 |= d.lod_mask;
    }
    let by_hash: HashMap<u32, usize> = hier.iter().enumerate().map(|(i, n)| (n.hash, i)).collect();

    let arg = |h: u32| -> String {
        match by_hash.get(&h) {
            None => format!("{h:#010x} <not a node>"),
            Some(&i) => match geo.get(&i) {
                None => format!("{h:#010x} n{i} (no mesh)"),
                Some((t, m)) => format!("{h:#010x} n{i} {t}tri mask{m:#04x}"),
            },
        }
    };

    println!("{name}: {} switch nodes, {} HIER nodes, {} draw groups\n", sm.nodes.len(), hier.len(), draws.len());

    // Children, to measure what a subtree-hide of each SWIT slot would actually cost.
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); hier.len()];
    for h in &hier {
        if let Some(p) = h.parent {
            if p < hier.len() {
                kids[p].push(h.index);
            }
        }
    }
    let subtree_tris = |root: usize| -> u32 {
        let (mut s, mut acc) = (vec![root], 0u32);
        while let Some(x) = s.pop() {
            if let Some((t, _)) = geo.get(&x) {
                acc += t;
            }
            s.extend_from_slice(&kids[x]);
        }
        acc
    };
    println!("SWIT chunk = {} slots (our seed hides each of these):", sm.switch_slots.len());
    for &slot in &sm.switch_slots {
        match by_hash.get(&slot) {
            None => println!("   {slot:#010x} <not a HIER node>"),
            Some(&i) => {
                let own = geo.get(&i).map(|(t, _)| *t).unwrap_or(0);
                let sub = subtree_tris(i);
                let flag = if sub > own { "   <-- SUBTREE-HIDE ALSO NUKES CHILDREN" } else { "" };
                println!("   {slot:#010x} n{i} own {own}tri / subtree {sub}tri{flag}");
            }
        }
    }
    println!();

    for (ni, node) in sm.nodes.iter().enumerate() {
        let def = orch::default_state_index(node);
        // only print switch nodes that actually touch geometry — the rest is noise
        let touches: bool = node.states.iter().any(|st| {
            st.enter.iter().any(|&t| by_hash.get(&t).and_then(|i| geo.get(i)).is_some())
        });
        if !touches {
            continue;
        }
        println!("SWITCH NODE {ni}  (we default to state #{def})");
        for (si, st) in node.states.iter().enumerate() {
            let sn = state_name(st.name_hash);
            let mark = if si == def { ">>" } else { "  " };
            println!("  {mark} state #{si} {:#010x} {sn}", st.name_hash);
            let (mut args, mut i) = (Vec::<u32>::new(), 0usize);
            while i < st.enter.len() {
                match st.enter[i] {
                    1 if i + 1 < st.enter.len() => {
                        args.push(st.enter[i + 1]);
                        i += 2;
                    }
                    2 if i + 1 < st.enter.len() => {
                        let cmd = st.enter[i + 1];
                        let cn = match cmd {
                            c if c == mercs2_formats::hash::pandemic_hash_m2("show") => "SHOW".into(),
                            c if c == mercs2_formats::hash::pandemic_hash_m2("hide") => "HIDE".into(),
                            c if c == mercs2_formats::hash::pandemic_hash_m2("setstate") => "SetState".into(),
                            c if c == mercs2_formats::hash::pandemic_hash_m2("setstateonmsg") => "SetStateOnMsg".into(),
                            c if c == mercs2_formats::hash::pandemic_hash_m2("startemitter") => "StartEmitter".into(),
                            c if c == mercs2_formats::hash::pandemic_hash_m2("createobject") => "CreateObject".into(),
                            c => format!("cmd{c:#010x}"),
                        };
                        let shown: Vec<String> = args
                            .iter()
                            .map(|&a| {
                                let s = state_name(a);
                                if !s.is_empty() { format!("{a:#010x} {s}") } else { arg(a) }
                            })
                            .collect();
                        println!("        {cn}({})", shown.join(", "));
                        args.clear();
                        i += 2;
                    }
                    3 => i += 1,
                    _ => {
                        args.push(st.enter[i]);
                        i += 1;
                    }
                }
            }
        }
        println!();
    }
}
