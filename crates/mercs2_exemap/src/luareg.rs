//! Lua C-function registration tables (`luaL_Reg`) read from an image's bytes.
//!
//! A table is a 4-byte-aligned array of `{const char* name, lua_CFunction func}` pairs ended by
//! `{NULL, NULL}`, where every name is a C identifier (at most 64 bytes) and every function is code
//! (on PS3, the address of an `.opd` descriptor, resolved to its code). The game's tables also nest
//! groups: `{name, 0xFFFFFFFF}` opens group `name` and `{name, 0xFFFFFFFE}` closes it, and an entry
//! inside a group is named `group.entry` (`Graphics` holds `Camera.GetYaw`). Markers must balance.
//!
//! A table's **namespace** is the identifier in a `{const char* name, luaL_Reg* table}` row that
//! points at the table, when the image holds one (the PC and Xbox 360 game registries do; the PS3
//! images name only Lua's own libraries this way). Tables of at least two entries are kept, and a
//! one-entry table when a registry row names it. A table that two or more rows name with different
//! identifiers is an error.

use std::collections::BTreeMap;

use crate::mem::Mem;
use crate::Error;

/// One registration table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub va: u32,
    /// The identifier a registry row gives this table.
    pub namespace: Option<String>,
    /// (name, function code address), in table order; a name inside a group is `group.entry`.
    pub entries: Vec<(String, u32)>,
}

fn identifier(mem: &Mem<'_>, va: u32) -> Option<String> {
    if va == 0 {
        return None;
    }
    let s = mem.cstr(va, 64)?;
    let ok = !s.is_empty()
        && (s[0].is_ascii_alphabetic() || s[0] == b'_')
        && s.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_');
    ok.then(|| String::from_utf8_lossy(s).into_owned())
}

/// Every `luaL_Reg` table in a PE or PS3 ELF image, by address.
pub fn tables(image: &[u8]) -> Result<Vec<Table>, Error> {
    let mem = Mem::of(image)?;
    const OPEN: u32 = 0xFFFF_FFFF;
    const CLOSE: u32 = 0xFFFF_FFFE;
    let mut found: BTreeMap<u32, Vec<(String, u32)>> = BTreeMap::new();
    for r in mem.ranges.iter().filter(|r| !r.exec) {
        let mut va = r.lo;
        while va + 8 <= r.hi {
            let mut entries = Vec::new();
            let mut groups: Vec<String> = Vec::new();
            let mut balanced = true;
            let mut p = va;
            while p + 8 <= r.hi {
                let (Some(n), Some(f)) = (mem.word(p), mem.word(p + 4)) else { break };
                let Some(name) = identifier(&mem, n) else { break };
                match f {
                    OPEN => groups.push(name),
                    CLOSE => {
                        if groups.pop().as_deref() != Some(name.as_str()) {
                            balanced = false;
                            break;
                        }
                    }
                    _ => match mem.function(f) {
                        Some(code) => {
                            let full = if groups.is_empty() { name } else { format!("{}.{name}", groups.join(".")) };
                            entries.push((full, code));
                        }
                        None => break,
                    },
                }
                p += 8;
            }
            let terminated = balanced
                && groups.is_empty()
                && p + 8 <= r.hi
                && mem.word(p) == Some(0)
                && mem.word(p + 4) == Some(0);
            if !entries.is_empty() && terminated {
                found.insert(va, entries);
                va = p + 8;
            } else {
                va += 4;
            }
        }
    }

    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    for (va, w) in mem.data_words() {
        if !found.contains_key(&w) || va < 4 {
            continue;
        }
        let Some(ns) = mem.word(va - 4).and_then(|n| identifier(&mem, n)) else { continue };
        if let Some(prev) = names.get(&w) {
            if *prev != ns {
                return Err(Error::new(format!(
                    "luaL_Reg table 0x{w:08x} is named both {prev} and {ns} by registry rows"
                )));
            }
        }
        names.insert(w, ns);
    }

    Ok(found
        .into_iter()
        .filter(|(va, entries)| entries.len() >= 2 || names.contains_key(va))
        .map(|(va, entries)| Table { va, namespace: names.get(&va).cloned(), entries })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starts::tests::pe_image;

    /// An x86 image: a two-entry table, a one-entry table (too short), and a registry row naming
    /// the first.
    fn image(conflict: bool) -> Vec<u8> {
        let base = 0x0040_0000u32;
        let le = |v: u32| v.to_le_bytes();
        let mut data = vec![0u8; 0x200];
        data[0x00..0x08].copy_from_slice(b"Player\0\0");
        data[0x08..0x18].copy_from_slice(b"GetName\0GetSeat\0");
        data[0x18..0x20].copy_from_slice(b"Other\0\0\0");
        let s = |o: u32| base + 0x1000 + o;
        // table at +0x40: {GetName, f1}, {GetSeat, f2}, {0, 0}
        data[0x40..0x44].copy_from_slice(&le(s(0x08)));
        data[0x44..0x48].copy_from_slice(&le(base + 0x2010));
        data[0x48..0x4c].copy_from_slice(&le(s(0x10)));
        data[0x4c..0x50].copy_from_slice(&le(base + 0x2020));
        // one-entry table at +0x60
        data[0x60..0x64].copy_from_slice(&le(s(0x08)));
        data[0x64..0x68].copy_from_slice(&le(base + 0x2030));
        // registry rows at +0x80
        data[0x80..0x84].copy_from_slice(&le(s(0x00)));
        data[0x84..0x88].copy_from_slice(&le(s(0x40)));
        if conflict {
            data[0x90..0x94].copy_from_slice(&le(s(0x18)));
            data[0x94..0x98].copy_from_slice(&le(s(0x40)));
        }
        pe_image(0x14c, base, &[(".rdata", 0x1000, 0x200, 0x4000_0040, &data), (".text", 0x2000, 0x100, 0x6000_0020, &[])])
    }

    #[test]
    fn tables_are_found_with_their_registry_namespace() {
        let t = tables(&image(false)).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].va, 0x0040_1040);
        assert_eq!(t[0].namespace.as_deref(), Some("Player"));
        assert_eq!(t[0].entries, vec![("GetName".to_string(), 0x0040_2010), ("GetSeat".to_string(), 0x0040_2020)]);
    }

    #[test]
    fn group_markers_qualify_the_entries_inside_them() {
        let base = 0x0040_0000u32;
        let le = |v: u32| v.to_le_bytes();
        let mut data = vec![0u8; 0x200];
        data[0x00..0x10].copy_from_slice(b"Camera\0\0GetYaw\0\0");
        data[0x10..0x18].copy_from_slice(b"Shot\0\0\0\0");
        let s = |o: u32| base + 0x1000 + o;
        let rows: [(u32, u32); 4] = [(s(0x10), base + 0x2010), (s(0x00), 0xFFFF_FFFF), (s(0x08), base + 0x2020), (s(0x00), 0xFFFF_FFFE)];
        for (i, (n, f)) in rows.iter().enumerate() {
            let o = 0x40 + i * 8;
            data[o..o + 4].copy_from_slice(&le(*n));
            data[o + 4..o + 8].copy_from_slice(&le(*f));
        }
        let img = pe_image(0x14c, base, &[(".rdata", 0x1000, 0x200, 0x4000_0040, &data), (".text", 0x2000, 0x100, 0x6000_0020, &[])]);
        let t = tables(&img).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].entries, vec![("Shot".to_string(), base + 0x2010), ("Camera.GetYaw".to_string(), base + 0x2020)]);
        // An unclosed group is not a table.
        let mut open = data.clone();
        open[0x58..0x60].fill(0);
        let img = pe_image(0x14c, base, &[(".rdata", 0x1000, 0x200, 0x4000_0040, &open), (".text", 0x2000, 0x100, 0x6000_0020, &[])]);
        assert!(tables(&img).unwrap().iter().all(|t| !t.entries.iter().any(|(n, _)| n.contains('.'))));
    }

    #[test]
    fn a_table_named_twice_differently_is_an_error() {
        let e = tables(&image(true)).unwrap_err().to_string();
        assert!(e.contains("named both"), "{e}");
    }
}
