//! Link the Lua functions of a corpus to the native functions of an image.
//!
//! Every `Namespace.Name` a function reads ([`crate::luaindex`]'s `lookups`) is looked up in the
//! image's Lua registration tables (`exemap luareg` export: `table`, `namespace`, `index`, `name`,
//! `function`), by namespace and entry name. A lookup is [`Link::Resolved`] to the one native function
//! registered under that name, [`Link::Ambiguous`] when several distinct functions are,
//! [`Link::UnregisteredInNativeNamespace`] when the image registers the namespace but not that name,
//! and [`Link::LuaNamespace`] when the image registers no table under that namespace at all (a table
//! the scripts build themselves). An export that names no namespace is an error, and the command
//! reports how many it names.

use std::collections::{BTreeMap, BTreeSet};

use crate::luaindex::FnRow;
use crate::Error;

const LUAREG_HEADER: &str = "table\tnamespace\tindex\tname\tfunction";

/// The native functions registered under each (namespace, name).
pub fn parse_luareg(text: &str) -> Result<BTreeMap<(String, String), BTreeSet<u64>>, Error> {
    let mut lines = text.lines();
    if lines.next() != Some(LUAREG_HEADER) {
        return Err(Error::new("luareg export: the first line is not the luareg header"));
    }
    let mut out: BTreeMap<(String, String), BTreeSet<u64>> = BTreeMap::new();
    for (i, l) in lines.enumerate() {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != 5 {
            return Err(Error::new(format!("luareg export line {}: {} fields", i + 2, f.len())));
        }
        let addr = u64::from_str_radix(f[4].trim_start_matches("0x"), 16)
            .map_err(|_| Error::new(format!("luareg export line {}: function {:?}", i + 2, f[4])))?;
        out.entry((f[1].to_string(), f[3].to_string())).or_default().insert(addr);
    }
    Ok(out)
}

/// The outcome of one lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Resolved(u64),
    Ambiguous(Vec<u64>),
    UnregisteredInNativeNamespace,
    LuaNamespace,
}

/// One lookup of one Lua function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookupLink {
    pub script: String,
    pub path: String,
    pub lookup: String,
    pub link: Link,
}

/// Link every lookup of every row through `reg`.
pub fn link(rows: &[FnRow], reg: &BTreeMap<(String, String), BTreeSet<u64>>) -> Result<Vec<LookupLink>, Error> {
    let native_namespaces: BTreeSet<&str> = reg.keys().map(|(ns, _)| ns.as_str()).filter(|ns| !ns.is_empty()).collect();
    if native_namespaces.is_empty() {
        return Err(Error::new(
            "the luareg export names no namespace, so no lookup can be told apart from a Lua-built table",
        ));
    }
    let mut out = Vec::new();
    let mut seen: BTreeSet<(String, u32, String)> = BTreeSet::new();
    for r in rows {
        // A script that several WADs of a corpus carry is linked once.
        if !seen.insert((r.script.clone(), r.name_hash, r.path.clone())) {
            continue;
        }
        for lookup in &r.lookups {
            let (ns, name) = lookup
                .split_once('.')
                .ok_or_else(|| Error::new(format!("{} {}: lookup {lookup:?} is not Namespace.Name", r.script, r.path)))?;
            let link = match reg.get(&(ns.to_string(), name.to_string())) {
                None if native_namespaces.contains(ns) => Link::UnregisteredInNativeNamespace,
                None => Link::LuaNamespace,
                Some(set) if set.len() == 1 => Link::Resolved(*set.iter().next().unwrap()),
                Some(set) => Link::Ambiguous(set.iter().copied().collect()),
            };
            out.push(LookupLink { script: r.script.clone(), path: r.path.clone(), lookup: lookup.clone(), link });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_resolve_by_namespace_and_name() {
        let reg = parse_luareg(&format!(
            "{LUAREG_HEADER}\n0x1\tGame\t0\tSpawn\t0x100\n0x1\tGame\t1\tTwice\t0x200\n0x2\tGame\t0\tTwice\t0x300\n0x3\tGame\t0\tSame\t0x400\n0x4\tGame\t0\tSame\t0x400\n"
        ))
        .unwrap();
        let mut row = crate::luapair::parse_index(&format!(
            "{}\nc\tw\ts\t0x00000001\tmain\t0\t0\t0\t2\t0\t2\t1\t0\t0\th\tk\tyes\tGame.Spawn;Game.Twice;Game.Same;Game.Missing;MrxGui.Show\n",
            crate::luaindex::HEADER
        ))
        .unwrap();
        row.push(row[0].clone());
        let unnamed = parse_luareg(&format!("{LUAREG_HEADER}\n0x1\t\t0\tSpawn\t0x100\n")).unwrap();
        assert!(link(&row, &unnamed).unwrap_err().to_string().contains("names no namespace"));
        let links: Vec<Link> = link(&row, &reg).unwrap().into_iter().map(|l| l.link).collect();
        assert_eq!(
            links,
            [
                Link::Resolved(0x100),
                Link::Ambiguous(vec![0x200, 0x300]),
                Link::Resolved(0x400),
                Link::UnregisteredInNativeNamespace,
                Link::LuaNamespace
            ]
        );
    }
}
