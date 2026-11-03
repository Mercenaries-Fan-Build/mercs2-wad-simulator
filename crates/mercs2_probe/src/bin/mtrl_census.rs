//! Census of every retail model material's `flags@104` word against its pixel-shader key.
//!
//! `Mtrl_Parse` (`FUN_00858790`) passes the flags through `FUN_0084ee70`, which turns the low three
//! bits into a one-hot (1..4 → 1, 2, 4, 8) and moves the rest. This counts, over every model of
//! the retail `vz.wad`, how often each low-3-bit value occurs, which other bits ride with it, and
//! which pixel shaders carry it, with one example material per (value, shader).
//!
//! `mtrl_census [names.txt]` — `names.txt` lists candidate shader names, one per line; each is
//! hashed with `pandemic_hash_m2` to label keys. Keys no name hashes to print as hex. Exits 1 when a
//! model fails to read or its MTRL fails to parse (each one is listed).

use std::collections::BTreeMap;

use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};

fn main() {
    let names: BTreeMap<u32, String> = match std::env::args().nth(1) {
        Some(p) => std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("read {p}: {e}"))
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| (pandemic_hash_m2(l), l.to_string()))
            .collect(),
        None => BTreeMap::new(),
    };
    let label = |k: u32| names.get(&k).cloned().unwrap_or_else(|| format!("0x{k:08X}"));

    let start = std::env::current_dir().expect("cwd");
    let vz = mercs2_formats::game_paths::local_config_vz_wad(&start).unwrap_or_else(|e| panic!("{e}"));
    let mut w = wad::open(vz.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));

    // low-3-bit value → (materials, models, the other flag bits seen → (count, example "model mat#"))
    let mut by_mode: BTreeMap<u16, (usize, std::collections::BTreeSet<u32>, BTreeMap<u16, (usize, String)>)> =
        BTreeMap::new();
    // (low-3-bit value, shader key) → (materials, example "model mat#")
    let mut by_shader: BTreeMap<(u16, u32), (usize, String)> = BTreeMap::new();
    // Alpha-tested materials (runtime bit 0x40): their word-13 alpha multiplier (`materialData` c98.w).
    let mut tested_alpha: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let (mut models, mut materials) = (0usize, 0usize);

    wad::for_each_model_hier(&mut w, |_, _| {}, |hash, read| {
        let (container, _) = match read {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("0x{hash:08X}: read: {e}"));
                return;
            }
        };
        let mats = match parse_mtrl(container, MtrlSource::Model) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("0x{hash:08X}: MTRL: {e:?}"));
                return;
            }
        };
        models += 1;
        for (i, m) in mats.iter().enumerate() {
            materials += 1;
            let mode = m.flags & 7;
            let e = by_mode.entry(mode).or_default();
            e.0 += 1;
            e.1.insert(hash);
            e.2.entry(m.flags & !7).or_insert_with(|| (0, format!("0x{hash:08X} mat{i}"))).0 += 1;
            let s = by_shader.entry((mode, m.shader_key)).or_insert_with(|| (0, format!("0x{hash:08X} mat{i}")));
            s.0 += 1;
            if m.runtime_flags() & 0x40 != 0 {
                *tested_alpha.entry(format!("{:.4}", m.preamble[13])).or_default() += 1;
            }
        }
    });

    println!("{models} models, {materials} materials");
    for (mode, (n, ms, rest)) in &by_mode {
        println!("\nflags&7 = {mode}: {n} materials in {} models", ms.len());
        let mut r: Vec<_> = rest.iter().collect();
        r.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (bits, (c, ex)) in r {
            println!("  other bits 0x{bits:04X}: {c}  e.g. {ex}");
        }
        let mut sh: Vec<_> = by_shader.iter().filter(|((m, _), _)| m == mode).collect();
        sh.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for ((_, key), (c, ex)) in sh {
            println!("  shader {}: {c}  e.g. {ex}", label(*key));
        }
    }
    println!("\nalpha-tested materials by word-13 alpha multiplier:");
    for (v, c) in &tested_alpha {
        println!("  {v}: {c}");
    }
    if !failures.is_empty() {
        println!("\n{} models failed:", failures.len());
        for f in &failures {
            println!("  {f}");
        }
        std::process::exit(1);
    }
}
