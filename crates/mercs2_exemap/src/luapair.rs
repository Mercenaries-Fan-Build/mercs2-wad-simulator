//! Pair the Lua functions of two corpora ([`crate::luaindex`] exports) by content.
//!
//! Scripts pair by name hash. Inside a pair of scripts, functions pair top-down:
//!
//! 1. the two `main` functions pair;
//! 2. a function whose (`code_hash`, `constants_hash`) is unique within its script on both sides
//!    pairs with its counterpart;
//! 3. inside every pair of functions, their nested functions are aligned in order by a longest
//!    common subsequence over `code_hash`; between two aligned children (and before the first and
//!    after the last), runs of unpaired children of equal length pair in order;
//! 4. step 3 repeats on new pairs until nothing changes.
//!
//! Every function ends up in a pair or in `only_a`/`only_b`. A pair's [`Class`] compares the two
//! functions' fingerprints and debug state.

use std::collections::{BTreeMap, BTreeSet};

use crate::luaindex::{FnRow, HEADER};
use crate::Error;

/// How two paired functions compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Same code, constants and debug state.
    Identical,
    /// Same code and constants; one side keeps debug information, the other does not.
    IdenticalDebugDiffers,
    /// Same code, different constants.
    ConstantsDiffer,
    /// Different code.
    CodeDiffers,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Identical => "identical",
            Class::IdenticalDebugDiffers => "identical-debug-differs",
            Class::ConstantsDiffer => "constants-differ",
            Class::CodeDiffers => "code-differs",
        }
    }
}

/// The outcome for one function or pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pairing {
    Pair { script: String, a: String, b: String, class: Class },
    OnlyA { script: String, path: String },
    OnlyB { script: String, path: String },
    ScriptOnlyA { script: String },
    ScriptOnlyB { script: String },
}

/// Parse an index export written by `exemap lua-index`.
pub fn parse_index(text: &str) -> Result<Vec<FnRow>, Error> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(Error::new("lua index: the first line is not the lua-index header"));
    }
    let mut out = Vec::new();
    for (i, l) in lines.enumerate() {
        let f: Vec<&str> = l.split('\t').collect();
        let bad = |what: &str| Error::new(format!("lua index line {}: {what}", i + 2));
        if f.len() != HEADER.split('\t').count() {
            return Err(bad(&format!("{} fields", f.len())));
        }
        let n = |s: &str, what: &str| s.parse::<usize>().map_err(|_| bad(&format!("{what} {s:?}")));
        let b = |s: &str, what: &str| s.parse::<u8>().map_err(|_| bad(&format!("{what} {s:?}")));
        let u = |s: &str, what: &str| s.parse::<u32>().map_err(|_| bad(&format!("{what} {s:?}")));
        out.push(FnRow {
            corpus: f[0].into(),
            wad: f[1].into(),
            script: f[2].into(),
            name_hash: u32::from_str_radix(f[3].trim_start_matches("0x"), 16).map_err(|_| bad("name_hash"))?,
            path: f[4].into(),
            line_defined: u(f[5], "line_defined")?,
            last_line_defined: u(f[6], "last_line_defined")?,
            num_params: b(f[7], "num_params")?,
            is_vararg: b(f[8], "is_vararg")?,
            nups: b(f[9], "nups")?,
            max_stack: b(f[10], "max_stack")?,
            code_len: n(f[11], "code_len")?,
            constants: n(f[12], "constants")?,
            functions: n(f[13], "functions")?,
            code_hash: f[14].into(),
            constants_hash: f[15].into(),
            debug: match f[16] {
                "yes" => true,
                "stripped" => false,
                v => return Err(bad(&format!("debug {v:?}"))),
            },
            lookups: if f[17].is_empty() { Vec::new() } else { f[17].split(';').map(str::to_string).collect() },
        });
    }
    Ok(out)
}

fn classify(a: &FnRow, b: &FnRow) -> Class {
    if a.code_hash != b.code_hash {
        Class::CodeDiffers
    } else if a.constants_hash != b.constants_hash {
        Class::ConstantsDiffer
    } else if a.debug != b.debug {
        Class::IdenticalDebugDiffers
    } else {
        Class::Identical
    }
}

/// The functions of one script, by path.
struct Script<'a> {
    by_path: BTreeMap<&'a str, &'a FnRow>,
}

impl<'a> Script<'a> {
    fn children(&self, path: &str) -> Vec<&'a FnRow> {
        let row = &self.by_path[path];
        (0..row.functions).map(|i| self.by_path[format!("{path}/{i}").as_str()]).collect()
    }
}

/// Indices pairing `a` and `b` along a longest common subsequence of their code hashes.
fn lcs(a: &[&FnRow], b: &[&FnRow]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i].code_hash == b[j].code_hash { t[i + 1][j + 1] + 1 } else { t[i + 1][j].max(t[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i].code_hash == b[j].code_hash {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn pair_script(name: &str, a: &Script, b: &Script, out: &mut Vec<Pairing>) {
    let mut pairs: BTreeMap<String, String> = BTreeMap::new();
    let mut used_b: BTreeSet<String> = BTreeSet::new();
    let add = |pa: &str, pb: &str, pairs: &mut BTreeMap<String, String>, used_b: &mut BTreeSet<String>| {
        if pairs.contains_key(pa) || used_b.contains(pb) {
            return false;
        }
        pairs.insert(pa.to_string(), pb.to_string());
        used_b.insert(pb.to_string());
        true
    };
    add("main", "main", &mut pairs, &mut used_b);
    // Unique (code, constants) fingerprints on both sides.
    let key = |r: &FnRow| (r.code_hash.clone(), r.constants_hash.clone());
    let mut count_a: BTreeMap<_, Vec<&str>> = BTreeMap::new();
    for (p, r) in &a.by_path {
        count_a.entry(key(r)).or_default().push(p);
    }
    let mut count_b: BTreeMap<_, Vec<&str>> = BTreeMap::new();
    for (p, r) in &b.by_path {
        count_b.entry(key(r)).or_default().push(p);
    }
    for (k, pa) in &count_a {
        if let (1, Some(pb)) = (pa.len(), count_b.get(k)) {
            if pb.len() == 1 {
                add(pa[0], pb[0], &mut pairs, &mut used_b);
            }
        }
    }
    // Align the children of every pair until nothing changes.
    loop {
        let mut changed = false;
        let snapshot: Vec<(String, String)> = pairs.iter().map(|(x, y)| (x.clone(), y.clone())).collect();
        for (pa, pb) in snapshot {
            let ca = a.children(&pa);
            let cb = b.children(&pb);
            let anchors = lcs(&ca, &cb);
            let mut bounds = vec![(0usize, 0usize)];
            for &(i, j) in &anchors {
                changed |= add(&ca[i].path, &cb[j].path, &mut pairs, &mut used_b);
                bounds.push((i + 1, j + 1));
            }
            let mut ends: Vec<(usize, usize)> = anchors.clone();
            ends.push((ca.len(), cb.len()));
            for ((sa, sb), (ea, eb)) in bounds.into_iter().zip(ends) {
                let ga: Vec<&FnRow> = ca[sa..ea].iter().copied().filter(|r| !pairs.contains_key(&r.path)).collect();
                let gb: Vec<&FnRow> = cb[sb..eb].iter().copied().filter(|r| !used_b.contains(&r.path)).collect();
                if ga.len() == gb.len() {
                    for (x, y) in ga.iter().zip(gb.iter()) {
                        changed |= add(&x.path, &y.path, &mut pairs, &mut used_b);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for (pa, pb) in &pairs {
        out.push(Pairing::Pair {
            script: name.to_string(),
            a: pa.clone(),
            b: pb.clone(),
            class: classify(a.by_path[pa.as_str()], b.by_path[pb.as_str()]),
        });
    }
    for p in a.by_path.keys().filter(|p| !pairs.contains_key(**p)) {
        out.push(Pairing::OnlyA { script: name.to_string(), path: p.to_string() });
    }
    for p in b.by_path.keys().filter(|p| !used_b.contains(**p)) {
        out.push(Pairing::OnlyB { script: name.to_string(), path: p.to_string() });
    }
}

/// Group rows into scripts keyed by (name hash, WAD-independent script name). A script that occurs
/// in several WADs of one corpus with the same code keeps one copy; different code under one name is
/// an error.
fn scripts(rows: &[FnRow]) -> Result<BTreeMap<(u32, String), Script<'_>>, Error> {
    let mut by_wad: BTreeMap<(u32, String, String), Script> = BTreeMap::new();
    for r in rows {
        let s = by_wad
            .entry((r.name_hash, r.script.clone(), r.wad.clone()))
            .or_insert_with(|| Script { by_path: BTreeMap::new() });
        if s.by_path.insert(r.path.as_str(), r).is_some() {
            return Err(Error::new(format!("{} {}: path {} occurs twice", r.wad, r.script, r.path)));
        }
    }
    let mut out: BTreeMap<(u32, String), Script> = BTreeMap::new();
    for ((h, name, wad), s) in by_wad {
        match out.get(&(h, name.clone())) {
            None => {
                out.insert((h, name), s);
            }
            Some(prev) => {
                let same = prev.by_path.len() == s.by_path.len()
                    && prev.by_path.iter().all(|(p, r)| {
                        s.by_path.get(p).is_some_and(|q| q.code_hash == r.code_hash && q.constants_hash == r.constants_hash)
                    });
                if !same {
                    return Err(Error::new(format!("script {name} differs between WADs of one corpus (seen again in {wad})")));
                }
            }
        }
    }
    Ok(out)
}

/// Pair every function of corpus index `a` with corpus index `b`.
pub fn pair(a: &[FnRow], b: &[FnRow]) -> Result<Vec<Pairing>, Error> {
    let sa = scripts(a)?;
    let sb = scripts(b)?;
    let mut out = Vec::new();
    for ((h, name), x) in &sa {
        match sb.get(&(*h, name.clone())) {
            Some(y) => pair_script(name, x, y, &mut out),
            None => out.push(Pairing::ScriptOnlyA { script: name.clone() }),
        }
    }
    for (h, name) in sb.keys() {
        if !sa.contains_key(&(*h, name.clone())) {
            out.push(Pairing::ScriptOnlyB { script: name.clone() });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(script: &str, path: &str, functions: usize, code: &str, consts: &str, debug: bool) -> FnRow {
        FnRow {
            corpus: "c".into(),
            wad: "w".into(),
            script: script.into(),
            name_hash: script.len() as u32,
            path: path.into(),
            line_defined: 0,
            last_line_defined: 0,
            num_params: 0,
            is_vararg: 0,
            nups: 0,
            max_stack: 0,
            code_len: 0,
            constants: 0,
            functions,
            code_hash: code.into(),
            constants_hash: consts.into(),
            debug,
            lookups: vec!["Ns.Fn".into(), "Ns.Other".into()],
        }
    }

    #[test]
    fn an_inserted_function_does_not_shift_its_siblings() {
        // A: main { f1, f2, f3 }; B: main { f1, new, f2', f3 } with f2' changed code.
        let a = vec![
            row("s", "main", 3, "m", "k", true),
            row("s", "main/0", 0, "f1", "k", true),
            row("s", "main/1", 0, "f2", "k", true),
            row("s", "main/2", 0, "f3", "k", true),
        ];
        let b = vec![
            row("s", "main", 4, "m2", "k", false),
            row("s", "main/0", 0, "f1", "k", false),
            row("s", "main/1", 0, "new", "k", false),
            row("s", "main/2", 0, "f2x", "k", false),
            row("s", "main/3", 0, "f3", "k", false),
        ];
        let out = pair(&a, &b).unwrap();
        let p = |x: &str, y: &str, c: Class| Pairing::Pair { script: "s".into(), a: x.into(), b: y.into(), class: c };
        assert!(out.contains(&p("main", "main", Class::CodeDiffers)));
        assert!(out.contains(&p("main/0", "main/0", Class::IdenticalDebugDiffers)));
        assert!(out.contains(&p("main/2", "main/3", Class::IdenticalDebugDiffers)));
        // f2 vs {new, f2x}: unequal gap, so neither pairs.
        assert!(out.contains(&Pairing::OnlyA { script: "s".into(), path: "main/1".into() }));
        assert!(out.contains(&Pairing::OnlyB { script: "s".into(), path: "main/1".into() }));
        assert!(out.contains(&Pairing::OnlyB { script: "s".into(), path: "main/2".into() }));
    }

    #[test]
    fn scripts_missing_on_one_side_are_reported() {
        let a = vec![row("only_a", "main", 0, "x", "k", true)];
        let b = vec![row("bb", "main", 0, "x", "k", true)];
        let out = pair(&a, &b).unwrap();
        assert!(out.contains(&Pairing::ScriptOnlyA { script: "only_a".into() }));
        assert!(out.contains(&Pairing::ScriptOnlyB { script: "bb".into() }));
    }

    #[test]
    fn index_exports_round_trip_through_the_parser() {
        let r = row("s", "main/0", 2, "abc", "def", false);
        let text = format!("{HEADER}\n{}\n", r.tsv());
        assert_eq!(parse_index(&text).unwrap(), vec![r]);
        assert!(parse_index("nope\n").is_err());
    }
}
