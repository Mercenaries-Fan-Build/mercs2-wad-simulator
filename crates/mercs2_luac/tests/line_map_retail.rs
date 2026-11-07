//! Every shipped script maps, instruction for instruction, onto the decompiled corpus's lines.
//!
//! The workshop's Missions debugger lays a run over a graph lifted from the decompiled text. A retail
//! run reports the original author's lines, so it is translated through
//! `mercs2_formats::lua_linemap::LineMap`, which holds only where our compile of the corpus text and
//! retail's chunk differ in nothing but line info. This pairs every corpus file with the script it was
//! decompiled from (`lua_linemap::pair_modules`), builds its map, and fails naming every file that
//! pairs with no script and every function that does not map, with why.
//!
//! Game-gated, built by the `retail` feature: reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml` (and the `shell.wad` beside it) and fails if either is absent. Run with
//! `cargo xtask retail-test`.

mod common;

use std::path::Path;

use mercs2_formats::lua_linemap::{pair_modules, shipped_for, shipped_scripts, LineMap, CORPUS_SOURCES};
use mercs2_formats::luachunk::{decode, Proto};

#[test]
fn every_shipped_script_maps_onto_the_corpus_lines() {
    let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
    let corpus = common::corpus_root().expect("the vendored Lua corpus (crates/mercs2_script/corpus) is missing");

    let (mut mapped, mut functions, mut stripped, mut renamed) = (0usize, 0usize, 0usize, Vec::new());
    let mut failed: Vec<String> = Vec::new();
    for (wad_name, path, bank) in CORPUS_SOURCES {
        // Retail's scripts by INFO name, the name each was shipped under.
        let scripts = shipped_scripts(&vz, bank).unwrap_or_else(|e| panic!("{e}"));

        let dir = corpus.join(bank);
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "lua"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut files: Vec<(String, Proto)> = Vec::new();
        for name in names {
            let source = std::fs::read_to_string(dir.join(format!("{name}.lua"))).expect("read corpus script");
            match mercs2_luac::compile(&source, &name).map_err(|e| e.to_string()).and_then(|b| decode(&b).map_err(|e| e.to_string())) {
                Ok(c) => files.push((name, c.main)),
                Err(e) => failed.push(format!("{bank}/{name}: our compile fails: {}", e.lines().next().unwrap_or(""))),
            }
        }

        let s: Vec<(&str, &Proto)> = scripts.iter().map(|s| (s.name.as_str(), &s.main)).collect();
        let f: Vec<(&str, &Proto)> = files.iter().map(|(n, p)| (n.as_str(), p)).collect();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        let pairing = pair_modules(&s, &f).unwrap_or_else(|e| panic!("{wad_name} `{path}`: {e}"));
        for &fi in &pairing.unpaired_files {
            failed.push(format!("{bank}/{}: no script in {wad_name} `{path}` has its name or its code", files[fi].0));
        }
        if !pairing.unpaired_scripts.is_empty() {
            let mut left: Vec<&str> = pairing.unpaired_scripts.iter().map(|&i| scripts[i].name.as_str()).collect();
            left.sort();
            eprintln!("   no corpus file: {wad_name} `{path}`: {} of {} scripts: {left:?}", left.len(), scripts.len());
        }
        for (si, fi) in pairing.pairs {
            let (script, file) = (&scripts[si], &files[fi]);
            let what = format!("{bank}/{} ({wad_name} `{}`)", file.0, script.name);
            // The Workshop finds one file's script alone (`shipped_for`); it must be the bank-wide pairing's.
            match shipped_for(&scripts, &file.0, &file.1, &names) {
                Ok(one) if one.name == script.name => {}
                Ok(one) => failed.push(format!("{what}: shipped_for finds `{}`", one.name)),
                Err(e) => failed.push(format!("{what}: shipped_for: {e}")),
            }
            let module = match script.module() {
                Ok(m) => m,
                Err(e) => {
                    failed.push(format!("{what}: {e}"));
                    continue;
                }
            };
            if !script.name.eq_ignore_ascii_case(&file.0) || module != script.name {
                renamed.push(format!("{what}, running as `{module}`"));
            }
            match LineMap::build(&script.main, &file.1) {
                Ok(m) => {
                    mapped += 1;
                    functions += m.functions.len();
                    if script.main.line_info.is_empty() {
                        stripped += 1;
                    }
                    for u in m.unmapped() {
                        failed.push(format!("{what}: {}", u.unmapped.as_deref().unwrap_or("")));
                    }
                }
                Err(e) => failed.push(format!("{what}: {e}")),
            }
        }
    }
    eprintln!("mapped {mapped} scripts ({functions} functions, {stripped} debug-stripped), {} failed", failed.len());
    for r in &renamed {
        eprintln!("   named apart: {r}");
    }
    for f in &failed {
        eprintln!("   FAILED {f}");
    }
    assert!(mapped > 0, "no script mapped: the corpus or the blocks were not found");
    assert!(failed.is_empty(), "{} corpus file(s) or function(s) do not map onto the corpus lines:\n{}", failed.len(), failed.join("\n"));
}
