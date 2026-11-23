//! `layer-scaffold` — how the layers of a `vz.wad` carry the `Name`, `ModelName` and `Transform`
//! COMPs a placement needs.
//!
//! For every layer sub-block it reads the container's tree and reports: how many layers carry each
//! of the three COMPs, and which carry none of their records; for each of the three, the distinct
//! `info` bodies (record count zeroed) and `schm` bodies; the distinct `CHDR`, `enum` and `flgt`
//! bodies; where each of the three sits among the container's COMPs, and the order pairs beside the
//! worldentity's COMP order; the `flgs` states of the entities that have only the three, named by
//! the worldentity `flgt` classes their bits index; and every layer `flgs` record against the rule
//! that bit `i` is set when the key has a record in class `flgt[i]`. It fails when a layer does not
//! read as a tree.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use mercs2_formats::ucfx::{parse_ucfx_tree, UcfxNode};

const COMPS: [&str; 3] = ["Name", "ModelName", "Transform"];

fn comp_name(n: &UcfxNode) -> Option<String> {
    if &n.tag != b"COMP" {
        return None;
    }
    let b = n.children.first()?.body.as_deref()?;
    let len = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    Some(String::from_utf8_lossy(&b[..len]).into_owned())
}

fn child<'a>(n: &'a UcfxNode, tag: &[u8; 4]) -> Option<&'a [u8]> {
    n.children.iter().find(|c| &c.tag == tag).and_then(|c| c.body.as_deref())
}

fn keys_of(name: &str, data: &[u8], stride: Option<usize>) -> Result<BTreeSet<u32>, String> {
    let mut out = BTreeSet::new();
    let mut o = 0;
    while o < data.len() {
        let key = data.get(o..o + 4).ok_or_else(|| format!("{name} data ends in a key"))?;
        out.insert(u32::from_le_bytes([key[0], key[1], key[2], key[3]]));
        o = match stride {
            Some(s) => o + s,
            None => o + 4 + data[o + 4..].iter().position(|&b| b == 0).ok_or("Name record with no NUL")? + 2,
        };
    }
    Ok(out)
}

fn all_empty_check(roots: &[UcfxNode]) -> bool {
    !roots.iter().any(|n| &n.tag == b"COMP")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn layer_scaffold(wadpath: &str) -> Result<(), String> {
    let mut game = mercs2_quartermaster::GameStack::open(&[PathBuf::from(wadpath)]).map_err(|e| e.to_string())?;
    let layers = game.layer_containers()?;
    let mut with: BTreeMap<&str, usize> = BTreeMap::new();
    let mut variants: BTreeMap<(&str, &str), BTreeMap<String, usize>> = BTreeMap::new();
    let mut shared: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    let mut places: BTreeMap<String, usize> = BTreeMap::new();
    let mut lacking_model: Vec<(u32, usize, String)> = Vec::new();
    let mut empty = 0usize;
    let mut states: BTreeMap<String, usize> = BTreeMap::new();
    let mut sorted_by_name = 0usize;
    let mut precedes: BTreeSet<(String, String)> = BTreeSet::new();
    let mut repeats: BTreeMap<String, usize> = BTreeMap::new();
    let mut top_tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut empty_shapes: Vec<String> = Vec::new();
    // (layer, key, 32-byte state, the COMPs that hold a record of the key).
    type FlagRecord = (u32, u32, Vec<u8>, Option<BTreeSet<String>>);
    let mut flag_records: Vec<FlagRecord> = Vec::new();
    for (&h, c) in &layers {
        let roots = parse_ucfx_tree(c).map_err(|e| format!("layer 0x{h:08X}: {e}"))?;
        let names: Vec<String> = roots.iter().filter_map(comp_name).collect();
        let tags: BTreeSet<String> = roots.iter().map(|n| String::from_utf8_lossy(&n.tag).into_owned()).collect();
        for t in tags {
            *top_tags.entry(t).or_default() += 1;
        }
        if all_empty_check(&roots) {
            empty_shapes.push(format!("0x{h:08X} [{}]", roots.iter().map(|n| String::from_utf8_lossy(&n.tag).into_owned()).collect::<Vec<_>>().join(",")));
        }
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                if a == b {
                    *repeats.entry(a.clone()).or_default() += 1;
                } else {
                    precedes.insert((a.clone(), b.clone()));
                }
            }
        }
        let mut s = names.clone();
        s.sort();
        if s == names {
            sorted_by_name += 1;
        }
        for (tag, label) in [(b"CHDR", "CHDR"), (b"enum", "enum"), (b"flgt", "flgt")] {
            let body = roots.iter().find(|n| &n.tag == tag).and_then(|n| n.body.as_deref());
            *shared.entry(label).or_default().entry(body.map_or("absent".into(), hex)).or_default() += 1;
        }
        let mut keys: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
        let mut all_keys: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
        for n in &roots {
            let Some(name) = comp_name(n) else { continue };
            let data = child(n, b"data").unwrap_or(&[]);
            let stride = match name.as_str() {
                "Name" => None,
                "Transform" => Some(42),
                _ => {
                    let schm = child(n, b"schm").unwrap_or(&[]);
                    (schm.len() >= 8).then(|| 4 + u32::from_le_bytes([schm[4], schm[5], schm[6], schm[7]]) as usize)
                }
            };
            if let Some(Ok(ks)) = stride.map(|s| keys_of(&name, data, Some(s))).or_else(|| (name == "Name").then(|| keys_of(&name, data, None))) {
                for k in &ks {
                    all_keys.entry(*k).or_default().insert(name.clone());
                }
                if let Some(c) = COMPS.iter().find(|c| **c == name) {
                    keys.insert(c, ks);
                }
            }
            if let Some(c) = COMPS.iter().find(|c| **c == name) {
                *with.entry(c).or_default() += 1;
                let mut info = n.children.first().and_then(|i| i.body.clone()).unwrap_or_default();
                let o = c.len() + 1 + 8;
                if info.len() >= o + 4 {
                    info[o..o + 4].fill(0);
                }
                *variants.entry((c, "info")).or_default().entry(hex(&info)).or_default() += 1;
                *variants.entry((c, "schm")).or_default().entry(hex(child(n, b"schm").unwrap_or(&[]))).or_default() += 1;
                let before: Vec<&str> = names.iter().take_while(|x| x.as_str() != *c).map(String::as_str).collect();
                *places.entry(format!("{c} after {}", before.last().copied().unwrap_or("(first)"))).or_default() += 1;
            }
        }
        if all_keys.is_empty() {
            empty += 1;
        }
        if !keys.contains_key("ModelName") {
            lacking_model.push((h, all_keys.len(), names.join(",")));
        }
        if let Some(fl) = roots.iter().find(|n| &n.tag == b"flgs").and_then(|n| n.body.as_deref()) {
            let count = if fl.len() >= 4 { u32::from_le_bytes([fl[0], fl[1], fl[2], fl[3]]) as usize } else { 0 };
            if fl.len() == 4 + 36 * count {
                for r in fl[4..].as_chunks::<36>().0 {
                    let k = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
                    let only: BTreeSet<String> = COMPS.iter().map(|c| c.to_string()).collect();
                    if all_keys.get(&k) == Some(&only) {
                        *states.entry(hex(&r[4..])).or_default() += 1;
                    }
                    flag_records.push((h, k, r[4..].to_vec(), all_keys.get(&k).cloned()));
                }
            }
        }
    }
    println!("layers: {}; empty (no keyed records): {empty}; COMPs in name order: {sorted_by_name}", layers.len());
    println!("top-level rows, by how many layers carry each: {top_tags:?}");
    println!("layers with no COMP: {empty_shapes:?}");
    for c in COMPS {
        println!("{c}: in {} layers", with.get(c).copied().unwrap_or(0));
        for part in ["info", "schm"] {
            let v = variants.get(&(c, part)).cloned().unwrap_or_default();
            println!("  {part}: {} distinct", v.len());
            for (body, n) in &v {
                println!("    {n:>5} × {body}");
            }
        }
    }
    for (label, v) in &shared {
        println!("{label}: {} distinct", v.len());
        let mut top: Vec<_> = v.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        for (body, n) in top.iter().take(8) {
            println!("    {n:>5} × {}", if body.len() > 96 { format!("{}… ({} bytes)", &body[..96], body.len() / 2) } else { body.to_string() });
        }
    }
    println!("positions:");
    for (p, n) in &places {
        println!("  {n:>5} × {p}");
    }
    println!("flgs states of entities with only Name, ModelName and Transform: {} distinct", states.len());
    for (s, n) in &states {
        println!("    {n:>5} × {s}");
    }
    let both: Vec<&(String, String)> = precedes.iter().filter(|(a, b)| precedes.contains(&(b.clone(), a.clone()))).collect();
    println!("COMP pairs seen in both orders: {}", both.len() / 2);
    for (a, b) in both.iter().filter(|(a, b)| a < b) {
        println!("    {a} / {b}");
    }
    println!("COMPs repeated within a layer: {repeats:?}");
    // The game's worldentity COMP order, beside which the layers' orders are compared.
    let mut assets = mercs2_engine::asset::AssetSource::discover(wadpath, &[])?;
    let we = assets
        .ecs_container(mercs2_formats::worldentity::WORLDENTITY_TYPE_HASH, mercs2_formats::worldentity::RETAIL_WORLDENTITY_NAME_HASH)?
        .ok_or("worldentity: in no open wad")?;
    let we_roots = parse_ucfx_tree(&we)?;
    let we_order: Vec<String> = we_roots.iter().filter_map(comp_name).collect();
    // The worldentity's flgt classes, by bit, beside the bits the layers' model-only states set.
    let flgt = we_roots.iter().find(|n| &n.tag == b"flgt").and_then(|n| n.body.as_deref()).ok_or("worldentity: no flgt")?;
    let n = u32::from_le_bytes([flgt[0], flgt[1], flgt[2], flgt[3]]) as usize;
    let mut classes = Vec::with_capacity(n);
    let mut o = 4;
    for _ in 0..n {
        let s = &flgt[o + 4..];
        let len = s.iter().position(|&b| b == 0).ok_or("worldentity flgt: a class with no NUL")?;
        classes.push(String::from_utf8_lossy(&s[..len]).into_owned());
        o += 4 + len + 1;
    }
    for (s, count) in &states {
        let bytes: Vec<u8> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect();
        let bits: Vec<String> = (0..256)
            .filter(|b| bytes.get(b / 8).is_some_and(|x| x & (1 << (b % 8)) != 0))
            .map(|b| format!("{b} = {}", classes.get(b).map_or("(past flgt)", String::as_str)))
            .collect();
        println!("model-only state × {count}: bits {bits:?}");
    }
    for c in COMPS {
        println!("worldentity flgt bit of {c}: {:?}", classes.iter().position(|x| x == c));
    }
    // Every layer flgs record against the worldentity rule: bit i set exactly when the key has a
    // record in the class flgt[i].
    let (mut agree, mut against, mut unkeyed) = (0usize, Vec::new(), 0usize);
    for (h, k, state, comps) in &flag_records {
        let Some(comps) = comps else {
            unkeyed += 1;
            continue;
        };
        let mut want = [0u8; 32];
        for c in comps {
            if let Some(b) = classes.iter().position(|x| x == c) {
                want[b / 8] |= 1 << (b % 8);
            }
        }
        if want.as_slice() == state.as_slice() {
            agree += 1;
        } else {
            let extra: Vec<&str> = (0..256)
                .filter(|b| state[b / 8] & (1 << (b % 8)) != 0 && want[b / 8] & (1 << (b % 8)) == 0)
                .map(|b| classes.get(b).map_or("(past flgt)", String::as_str))
                .collect();
            against.push(format!(
                "layer 0x{h:08X} key 0x{k:08X} [{}]: state {} want {}; set beyond the rule: {extra:?}",
                comps.iter().cloned().collect::<Vec<_>>().join(","),
                hex(state),
                hex(&want)
            ));
        }
    }
    println!("layer flgs records against the flgt-class rule: {agree} agree, {} against, {unkeyed} with no COMP record read", against.len());
    let mut beyond: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &against {
        *beyond.entry(a.rsplit_once("set beyond the rule: ").map_or("", |x| x.1)).or_default() += 1;
    }
    println!("    the bits set beyond the rule, by how many records: {beyond:?}");
    for a in against.iter().take(12) {
        println!("    {a}");
    }
    let rank: BTreeMap<&str, usize> = we_order.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let (mut agree, mut against, mut unranked) = (0usize, 0usize, BTreeSet::new());
    for (a, b) in &precedes {
        match (rank.get(a.as_str()), rank.get(b.as_str())) {
            (Some(x), Some(y)) if x < y => agree += 1,
            (Some(_), Some(_)) => against += 1,
            (x, y) => {
                if x.is_none() {
                    unranked.insert(a.clone());
                }
                if y.is_none() {
                    unranked.insert(b.clone());
                }
            }
        }
    }
    println!("layer COMP pairs in the worldentity's COMP order: {agree} agree, {against} against; not in the worldentity: {unranked:?}");
    let near: Vec<&str> = we_order.iter().map(String::as_str).skip_while(|n| *n != "Name").take(12).collect();
    println!("worldentity order from Name: {near:?}");
    println!("layers with no ModelName COMP: {}", lacking_model.len());
    for (h, n, comps) in &lacking_model {
        println!("  0x{h:08X}  {n:>4} keys  [{comps}]");
    }
    Ok(())
}
