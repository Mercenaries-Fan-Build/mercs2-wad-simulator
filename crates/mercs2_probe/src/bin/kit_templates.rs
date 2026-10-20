//! Which world entities wear a given model? Reads the resident `worldentity` (type `0x5647C35D`).
//!
//! The `ModelName` data is a sequence of `[u32 N][N × u32 entity_key][u32 model_hash]` runs (one
//! model shared by N entities, no header); the walk must end exactly at the data end and every model
//! must be a type-19 hash, or the run fails. For every entity whose model is queried, its `Name`
//! string is reported — found by locating the key as a LE u32 at any byte offset in the `Name` data
//! and reading the NUL-terminated string after it (`{u32 key}["EntityName\0"]`).
//!
//! Exit 0 = worldentity read and every queried model accounted for (hit or no ModelName record);
//! 1 = the container or a required group could not be read; 2 = usage.
//!
//! ```text
//! cargo run -p mercs2_probe --bin kit_templates -- 0x4D4F42B5 [0x...] [--wad path]
//! ```

use mercs2_engine::wad;
use mercs2_formats::schema::parse_comp_groups;
use std::collections::{BTreeMap, HashSet};

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;

fn fail(msg: String) -> ! {
    eprintln!("kit_templates: {msg}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wad_arg = args.iter().position(|a| a == "--wad").and_then(|i| args.get(i + 1)).cloned();
    let targets: Vec<u32> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && args[i - 1] == "--wad"))
        .map(|(_, a)| match a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16).unwrap_or_else(|e| {
                eprintln!("kit_templates: {a:?} is not a hex hash: {e}");
                std::process::exit(2);
            }),
            None => mercs2_formats::hash::pandemic_hash_m2(a),
        })
        .collect();
    if targets.is_empty() {
        eprintln!("usage: kit_templates <model name|0xHASH>... [--wad <path>]");
        std::process::exit(2);
    }
    let want: HashSet<u32> = targets.iter().copied().collect();

    let wadpath = wad::resolve_vz_wad(wad_arg.as_deref()).unwrap_or_else(|| {
        eprintln!("kit_templates: no vz.wad found (pass --wad, or write .mercs2-local.toml)");
        std::process::exit(2);
    });
    let mut w = wad::open(&wadpath).unwrap_or_else(|e| fail(format!("cannot open {wadpath}: {e}")));

    // Find the worldentity chunk by its type in every block's entry table.
    let nblocks = wad::block_paths(&w).len() as u16;
    let mut we: Option<(u16, Vec<u8>)> = None;
    'blocks: for b in 0..nblocks {
        let dec = wad::decompress_block_index(&mut w, b).unwrap_or_else(|e| fail(format!("block {b}: {e}")));
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        let mut off = 4 + count as usize * 16;
        for e in &entries {
            let end = off + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if e.type_hash == WORLDENTITY_TYPE {
                we = Some((b, dec[off..end].to_vec()));
                break 'blocks;
            }
            off = end;
        }
    }
    let (we_block, we) = we.unwrap_or_else(|| fail("no worldentity chunk in any block".into()));
    let model_hashes: HashSet<u32> = wad::model_list_all(&w).into_iter().map(|(h, _)| h).collect();
    let groups = parse_comp_groups(&we);
    eprintln!("[kit_templates] worldentity in block {we_block}: {} bytes, {} COMP groups", we.len(), groups.len());

    // model -> the entity keys whose ModelName is that model.
    let mut model_keys: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut saw_modelname = false;
    for g in &groups {
        let Some(data) = g.data.as_ref() else { continue };
        if g.name.as_deref() == Some("ModelName") {
            // The worldentity ModelName data is NOT fixed {key, model} records: it is a sequence of
            // [u32 N][N × u32 entity_key][u32 model_hash] — one model shared by N entities — with no
            // header. The walk must land exactly on the end, and every model must be a type-19 hash.
            let rd = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            let (mut o, mut runs, mut keys) = (0usize, 0usize, 0usize);
            let mut not_model = 0usize;
            while o < data.len() {
                if o + 4 > data.len() {
                    fail(format!("ModelName: truncated count at {o}"));
                }
                let n = rd(o) as usize;
                let end = o + 4 + n * 4 + 4;
                if n == 0 || end > data.len() {
                    fail(format!("ModelName: run at {o} claims {n} keys, runs past {} bytes", data.len()));
                }
                let model = rd(o + 4 + n * 4);
                if !model_hashes.contains(&model) {
                    not_model += 1;
                }
                for i in 0..n {
                    let key = rd(o + 4 + i * 4);
                    if want.contains(&model) {
                        model_keys.entry(model).or_default().push(key);
                    }
                }
                runs += 1;
                keys += n;
                o = end;
            }
            // A wrong layout lands on random words, almost none of them model hashes; the retail data
            // also references some models this WAD does not carry, so the miss count is reported.
            eprintln!(
                "[kit_templates] ModelName: {} bytes = {runs} runs over {keys} entity keys; {} runs name a type-19 model in this WAD, {not_model} do not",
                data.len(),
                runs - not_model
            );
            saw_modelname = true;
        }
    }
    if !saw_modelname {
        fail("no ModelName group with a fixed-stride schema".into());
    }
    let name_data = groups
        .iter()
        .find(|g| g.name.as_deref() == Some("Name"))
        .and_then(|g| g.data.clone())
        .unwrap_or_else(|| fail("no Name group data".into()));

    let name_for = |key: u32| -> Vec<String> {
        let needle = key.to_le_bytes();
        let mut out = Vec::new();
        // Name records are variable-length, so the key can sit at any byte offset. A handle key
        // (0x80xxxxxx) has a non-ASCII top byte, so it does not occur inside the name text itself.
        let mut i = 0;
        while i + 4 <= name_data.len() {
            if name_data[i..i + 4] == needle {
                let s = &name_data[i + 4..];
                let end = s.iter().position(|&c| c == 0).unwrap_or(0);
                let txt = &s[..end];
                if !txt.is_empty() && txt.iter().all(|c| (0x20..0x7f).contains(c)) {
                    out.push(String::from_utf8_lossy(txt).into_owned());
                }
            }
            i += 1;
        }
        out
    };

    // `--components`: every other component record each entity carries. `data` is grouped records
    // `[u32 n][n × u32 key][payload]` with the schm `payload_stride` (docs/worldentity_container_format.md
    // §3.3); the three classes whose native reader reads another layout are not walked.
    let profiles = args.iter().any(|a| a == "--profiles");
    let components = profiles || args.iter().any(|a| a == "--components");
    let mut comps_of: BTreeMap<u32, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    if components {
        let wanted_keys: HashSet<u32> = model_keys.values().flatten().copied().collect();
        for g in &groups {
            let name = g.name.clone().unwrap_or_else(|| format!("0x{:08X}", g.type_hash.unwrap_or(0)));
            if ["Name", "PointLocation", "NetCategoryInfo", "ModelName"].contains(&name.as_str()) {
                continue;
            }
            let (Some(schema), Some(data)) = (g.schema(), g.data.as_ref()) else { continue };
            let stride = schema.payload_stride as usize;
            let rd = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            let mut o = 0usize;
            while o < data.len() {
                if o + 4 > data.len() {
                    fail(format!("{name}: truncated count at {o}"));
                }
                let n = rd(o) as usize;
                let end = o + 4 + n * 4 + stride;
                if n == 0 || end > data.len() {
                    fail(format!("{name}: group at {o} claims {n} keys + {stride}-byte payload, past {} bytes", data.len()));
                }
                let payload = &data[o + 4 + n * 4..end];
                for i in 0..n {
                    let key = rd(o + 4 + i * 4);
                    if wanted_keys.contains(&key) {
                        comps_of.entry(key).or_default().push((name.clone(), payload.to_vec()));
                    }
                }
                o = end;
            }
        }
    }

    for t in &targets {
        let keys = model_keys.get(t).cloned().unwrap_or_default();
        println!("\n=== model 0x{t:08X}: {} ModelName record(s)", keys.len());
        for k in keys {
            println!("  entity 0x{k:08X}  name {:?}", name_for(k));
            for (c, p) in comps_of.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                let words: Vec<String> = p
                    .chunks(4)
                    .map(|w| w.iter().rev().map(|b| format!("{b:02X}")).collect::<String>())
                    .collect();
                println!("    {c:28} {}", words.join(" "));
            }
        }

        // `--profiles`: where inside the model container each template's ModelMixerProfile hash sits.
        // UCFX rows are `{tag, rel_off, size, x2, x3}` at 20 + 20·i, in pre-order with x3 = descendant
        // count, bodies at `data_area_off (u32 @4) + rel_off` (docs/ucfx_tree_container.md §1).
        if !profiles {
            continue;
        }
        let mut wanted: Vec<u32> = Vec::new();
        for k in model_keys.get(t).map(Vec::as_slice).unwrap_or(&[]) {
            for (c, p) in comps_of.get(k).map(Vec::as_slice).unwrap_or(&[]) {
                if c == "ModelMixerProfile" && p.len() == 4 {
                    let v = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                    if v != 0 && !wanted.contains(&v) {
                        wanted.push(v);
                    }
                }
            }
        }
        let c = wad::extract_container(&mut w, *t).unwrap_or_else(|e| fail(format!("model 0x{t:08X}: {e}")));
        let rd = |o: usize| u32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
        let (data_off, nrows) = (rd(4) as usize, rd(16) as usize);
        let tag = |i: usize| String::from_utf8_lossy(&c[20 + i * 20..24 + i * 20]).into_owned();
        // parent chain of row i: the nearest earlier row whose subtree covers i
        let parents = |i: usize| -> Vec<String> {
            let mut chain = Vec::new();
            let mut j = i;
            while let Some(p) = (0..j).rev().find(|&p| p + rd(20 + p * 20 + 16) as usize >= j) {
                chain.push(tag(p));
                j = p;
            }
            chain.reverse();
            chain
        };
        println!("  --- profiles inside model 0x{t:08X} ({} rows, data @{data_off})", nrows);
        // The whole subtree of every MIXR row, bodies in full.
        for m in (0..nrows).filter(|&i| tag(i) == "MIXR") {
            let x3 = rd(20 + m * 20 + 16) as usize;
            for i in m..=m + x3 {
                let (ro, sz) = (rd(20 + i * 20 + 4), rd(20 + i * 20 + 8) as usize);
                if ro == 0xFFFF_FFFF {
                    println!("  row {i:3} {} (marker, {} descendants)", tag(i), rd(20 + i * 20 + 16));
                    continue;
                }
                let body = &c[data_off + ro as usize..data_off + ro as usize + sz];
                let words: Vec<String> = body.chunks(4).map(|w| w.iter().rev().map(|b| format!("{b:02X}")).collect()).collect();
                println!("  row {i:3} {} {sz} bytes: {}", tag(i), words.join(" "));
            }
        }
        for v in &wanted {
            let needle = v.to_le_bytes();
            let hits: Vec<usize> = c.windows(4).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect();
            for at in hits {
                let row = (0..nrows).find(|&i| {
                    let (ro, sz) = (rd(20 + i * 20 + 4), rd(20 + i * 20 + 8) as usize);
                    ro != 0xFFFF_FFFF && at >= data_off + ro as usize && at < data_off + ro as usize + sz
                });
                match row {
                    None => println!("  profile 0x{v:08X} @{at}: not inside any row body"),
                    Some(i) => {
                        let (ro, sz) = (rd(20 + i * 20 + 4) as usize, rd(20 + i * 20 + 8) as usize);
                        let body = &c[data_off + ro..data_off + ro + sz];
                        let words: Vec<String> = body.chunks(4).map(|w| w.iter().rev().map(|b| format!("{b:02X}")).collect()).collect();
                        println!(
                            "  profile 0x{v:08X} @{at}: row {i} {} under {:?}, body {sz} bytes, +{}",
                            tag(i),
                            parents(i),
                            at - data_off - ro
                        );
                        println!("    {}", words.join(" "));
                    }
                }
            }
        }
    }
}
