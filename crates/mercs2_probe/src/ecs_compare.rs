//! `ecs-compare` — one archive's ECS containers, converted, against another's, record by record.
//!
//! Either archive may be big-endian (converted here) or a PC `vz.wad`. A container pair is joined by
//! name and type, each group by class hash, each record by key, so a compact console container meets
//! its PC twin: a payload compares byte for byte, and a `Name` hash compares with the hash of the
//! other side's string. Writes one row per class; `--diffs <file>` lists every differing record and
//! row.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::Ordering;

use mercs2_formats::be_to_le::convert::QUIET;
use mercs2_formats::be_to_le::ecs::{ecs_containers, read_records, ComponentSchemas};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::types::TYPE_HASH_GUIDMAP;

#[derive(Default)]
struct Tally {
    class: String,
    both: usize,
    equal: usize,
    differ: usize,
    a_only: usize,
    b_only: usize,
    bytes: BTreeMap<usize, usize>,
}

pub fn ecs_compare(a: &Path, b: &Path, vz_wad: &Path, out: &Path, diffs: Option<&Path>) -> Result<(), String> {
    QUIET.store(true, Ordering::Relaxed);
    let schemas = ComponentSchemas::from_vz_wad(vz_wad)?;
    println!("{} classes from {}", schemas.len(), vz_wad.display());
    let left = ecs_containers(a, &schemas)?;
    let right = ecs_containers(b, &schemas)?;
    println!("{} containers, {} containers", left.len(), right.len());
    let mut tallies: BTreeMap<u32, Tally> = BTreeMap::new();
    let mut rows: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut diff_lines = String::new();
    let mut paired = 0usize;
    let mut chdrs: BTreeMap<(u32, i16, u32, i16), usize> = BTreeMap::new();
    let mut raw_equal = 0usize;
    for (key, lc) in &left {
        let Some(rc) = right.get(key) else { continue };
        paired += 1;
        raw_equal += usize::from(lc == rc);
        if key.1 == TYPE_HASH_GUIDMAP {
            let same = lc == rc;
            let e = rows.entry("guidmap".into()).or_default();
            if same { e.0 += 1 } else { e.1 += 1 }
            if !same && diffs.is_some() {
                diff_lines.push_str(&format!("0x{:08X}\tguidmap\t-\t{}\t{}\n", key.0, hex(lc), hex(rc)));
            }
            continue;
        }
        let at = |e: String| format!("0x{:08X}: {e}", key.0);
        let (l, r) = (read_records(lc, &schemas).map_err(at)?, read_records(rc, &schemas).map_err(at)?);
        *chdrs.entry((l.flags, l.gate, r.flags, r.gate)).or_default() += 1;
        for (tag, body) in &l.rows {
            if tag == "CHDR" || tag == "enum" {
                continue;
            }
            let e = rows.entry(tag.clone()).or_default();
            let same = r.rows.get(tag) == Some(body);
            if same { e.0 += 1 } else { e.1 += 1 }
            if let (false, true, Some(w)) = (same, diffs.is_some(), r.rows.get(tag)) {
                diff_lines.push_str(&format!("0x{:08X}\t{tag}\t-\t{}\t{}\n", key.0, hex(body), hex(w)));
            }
        }
        let classes: BTreeSet<u32> = l.classes.keys().chain(r.classes.keys()).copied().collect();
        for class in classes {
            let (lg, rg) = (l.classes.get(&class), r.classes.get(&class));
            let t = tallies.entry(class).or_default();
            if let Some(g) = lg.or(rg) {
                t.class = g.class.clone();
            }
            let empty = Default::default();
            let (lm, rm) = (lg.map_or(&empty, |g| &g.by_key), rg.map_or(&empty, |g| &g.by_key));
            for (k, v) in lm {
                let Some(w) = rm.get(k) else {
                    t.a_only += 1;
                    continue;
                };
                t.both += 1;
                if v == w {
                    t.equal += 1;
                    continue;
                }
                t.differ += 1;
                for (i, (x, y)) in v.iter().zip(w).enumerate() {
                    if x != y {
                        *t.bytes.entry(i).or_default() += 1;
                    }
                }
                if diffs.is_some() {
                    diff_lines.push_str(&format!("0x{:08X}\t{}\t0x{k:08X}\t{}\t{}\n", key.0, t.class, hex(v), hex(w)));
                }
            }
            t.b_only += rm.keys().filter(|k| !lm.contains_key(k)).count();
        }
    }
    println!("{paired} pairs, {raw_equal} byte-identical");
    for ((lf, lg, rf, rg), n) in &chdrs {
        println!("CHDR a flags {lf} gate {lg}, b flags {rf} gate {rg}: {n} containers");
    }
    for (tag, (same, differ)) in &rows {
        println!("{tag}: {same} equal, {differ} differ");
    }
    let mut tsv = String::from("class\thash\tboth\tequal\tdiffer\ta_only\tb_only\tdiffering_bytes\n");
    let (mut both, mut equal) = (0usize, 0usize);
    for (h, t) in &tallies {
        both += t.both;
        equal += t.equal;
        let bytes: Vec<String> = t.bytes.iter().map(|(o, n)| format!("{o}:{n}")).collect();
        tsv.push_str(&format!(
            "{}\t0x{h:08X}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            t.class,
            t.both,
            t.equal,
            t.differ,
            t.a_only,
            t.b_only,
            bytes.join(",")
        ));
    }
    println!("records on both sides: {both}, equal {equal}");
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    if let Some(d) = diffs {
        std::fs::write(d, diff_lines).map_err(|e| format!("write {}: {e}", d.display()))?;
    }
    Ok(())
}

/// Every name string of an archive's named containers, with the hash it stores under and the
/// container it came from.
pub fn ecs_names(archive: &Path, vz_wad: &Path, out: &Path) -> Result<(), String> {
    QUIET.store(true, Ordering::Relaxed);
    let schemas = ComponentSchemas::from_vz_wad(vz_wad)?;
    let mut tsv = String::from("hash\tname\tkey\tcontainer\n");
    let mut n = 0usize;
    for (key, c) in ecs_containers(archive, &schemas)? {
        if key.1 == TYPE_HASH_GUIDMAP {
            continue;
        }
        let r = read_records(&c, &schemas).map_err(|e| format!("0x{:08X}: {e}", key.0))?;
        for (k, s) in &r.names {
            tsv.push_str(&format!("0x{:08X}\t{s}\t0x{k:08X}\t0x{:08X}\n", pandemic_hash_m2(s), key.0));
            n += 1;
        }
    }
    println!("{n} names");
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
