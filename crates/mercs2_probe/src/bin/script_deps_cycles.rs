//! Probe: the DEPS cycles of a WAD's scripts, read from each container's stored DEPS chunk.
//!
//!   cargo run -p mercs2_probe --bin script_deps_cycles -- <wad> <pths-substring> [<pths-substring> ...]
//!
//! Reads every script entry of the blocks whose PTHS path contains one of the substrings (case
//! ignored), builds the graph script → each DEPS entry that names a script of those blocks, and
//! prints its strongly connected components with more than one member (or a self-dependency), one
//! per line: `scc <n> <size> <member> <member> …`, largest first. Then one line per DEPS entry that
//! names no script of those blocks: `unresolved <script> 0x<hash>`. Then one line per script with its
//! DEPS in list order, duplicates kept, each named: `deps <script> <child> <child> …`. Then
//! `scripts <n> edges <n> sccs <n>`.
//!
//! Exit 0 on success, 2 when the WAD or a block cannot be read or a substring matches no block.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;
use std::process::ExitCode;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::{container_deps, container_info, ScriptsBlock};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_SCRIPT;

/// Tarjan's strongly connected components of `edges` over `nodes`, iteratively.
fn sccs(nodes: &BTreeSet<u32>, edges: &BTreeMap<u32, Vec<u32>>) -> Vec<Vec<u32>> {
    let mut index: BTreeMap<u32, usize> = BTreeMap::new();
    let mut low: BTreeMap<u32, usize> = BTreeMap::new();
    let mut on: BTreeSet<u32> = BTreeSet::new();
    let mut stack: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    let mut next = 0usize;
    let empty = Vec::new();
    for &root in nodes {
        if index.contains_key(&root) {
            continue;
        }
        // (node, position in its edge list)
        let mut work: Vec<(u32, usize)> = vec![(root, 0)];
        index.insert(root, next);
        low.insert(root, next);
        next += 1;
        stack.push(root);
        on.insert(root);
        while let Some(&mut (v, ref mut at)) = work.last_mut() {
            let es = edges.get(&v).unwrap_or(&empty);
            if *at < es.len() {
                let w = es[*at];
                *at += 1;
                if !index.contains_key(&w) {
                    index.insert(w, next);
                    low.insert(w, next);
                    next += 1;
                    stack.push(w);
                    on.insert(w);
                    work.push((w, 0));
                } else if on.contains(&w) {
                    let lw = index[&w];
                    let lv = low.get_mut(&v).unwrap();
                    *lv = (*lv).min(lw);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                let lv = low[&v];
                let lp = low.get_mut(&parent).unwrap();
                *lp = (*lp).min(lv);
            }
            if low[&v] == index[&v] {
                let mut comp = Vec::new();
                loop {
                    let w = stack.pop().unwrap();
                    on.remove(&w);
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(comp);
            }
        }
    }
    out
}

fn run(wad: &Path, needles: &[String]) -> Result<(), String> {
    let mut f = File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| format!("{}: {e}", wad.display()))?.len();
    let arch = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e:?}", wad.display()))?;
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let mut deps: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for needle in needles {
        let lowered = needle.to_lowercase();
        let hits: Vec<usize> =
            arch.paths.iter().enumerate().filter(|(_, p)| p.to_lowercase().contains(&lowered)).map(|(i, _)| i).collect();
        if hits.is_empty() {
            return Err(format!("no block of {} has a path containing {needle:?}", wad.display()));
        }
        for bi in hits {
            let raw = decompress_block(&mut f, &arch.indx, bi as u16).map_err(|e| format!("{}: {e:?}", arch.paths[bi]))?;
            let block = ScriptsBlock::parse(&raw).map_err(|e| format!("{}: {e}", arch.paths[bi]))?;
            for e in block.entries.iter().filter(|e| e.type_hash == TYPE_HASH_SCRIPT) {
                let info = container_info(&e.bytes).map_err(|m| format!("{} 0x{:08x}: INFO: {m}", arch.paths[bi], e.name_hash))?;
                let d = container_deps(&e.bytes).map_err(|m| format!("{} {}: DEPS: {m}", arch.paths[bi], info.name))?;
                names.insert(e.name_hash, info.name);
                deps.insert(e.name_hash, d);
            }
        }
    }
    let nodes: BTreeSet<u32> = names.keys().copied().collect();
    let mut edges: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut unresolved = Vec::new();
    let mut edge_count = 0usize;
    for (&n, ds) in &deps {
        let mut seen = BTreeSet::new();
        for &d in ds {
            if !nodes.contains(&d) {
                unresolved.push((n, d));
            } else if seen.insert(d) {
                edges.entry(n).or_default().push(d);
                edge_count += 1;
            }
        }
    }
    let mut cyclic: Vec<Vec<u32>> = sccs(&nodes, &edges)
        .into_iter()
        .filter(|c| c.len() > 1 || edges.get(&c[0]).is_some_and(|e| e.contains(&c[0])))
        .collect();
    cyclic.sort_by_key(|c| std::cmp::Reverse(c.len()));
    for (i, c) in cyclic.iter().enumerate() {
        let mut members: Vec<&str> = c.iter().map(|h| names[h].as_str()).collect();
        members.sort();
        println!("scc\t{i}\t{}\t{}", c.len(), members.join(" "));
    }
    for (n, d) in &unresolved {
        println!("unresolved\t{}\t0x{d:08x}", names[n]);
    }
    for (n, ds) in &deps {
        let named: Vec<String> = ds.iter().map(|d| names.get(d).cloned().unwrap_or_else(|| format!("0x{d:08x}"))).collect();
        println!("deps\t{}\t{}", names[n], named.join(" "));
    }
    println!("scripts\t{}\tedges\t{edge_count}\tsccs\t{}", nodes.len(), cyclic.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: script_deps_cycles <wad> <pths-substring> [<pths-substring> ...]");
        return ExitCode::from(2);
    }
    match run(Path::new(&args[0]), &args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("script_deps_cycles: {e}");
            ExitCode::from(2)
        }
    }
}
