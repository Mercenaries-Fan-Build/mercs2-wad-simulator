//! Lua C-function registration tables (`luaL_Reg`) read from an image's bytes.
//!
//! A table is a 4-byte-aligned array of `{const char* name, lua_CFunction func}` pairs ended by
//! `{NULL, NULL}`, where every name is a C identifier (at most 64 bytes) and every function is code
//! (on PS3, the address of an `.opd` descriptor, resolved to its code). The game's tables also nest
//! groups: `{name, 0xFFFFFFFF}` opens group `name` and `{name, 0xFFFFFFFE}` closes it, and an entry
//! inside a group is named `group.entry` (`Graphics` holds `Camera.GetYaw`). Markers must balance.
//!
//! A table's **namespace** is the identifier in the word before a pointer to the table — the
//! `name` of a `{const char* name, luaL_Reg* table, ...}` registry row. That word comes from the
//! image's bytes, or, in a PowerPC image (Xbox 360 or PS3), from a store instruction whose address
//! and value the code fixes. The PC, Xbox 360 Final and Xbox 360 retail game registries carry their
//! names as data. The PS3 images and the Xbox 360 Profile image leave each game row's name word zero
//! and fill it in a static initializer that copies the library's `const char*` name variable into
//! the row (PS3 `0x0050a8e8`, Profile `0x829eba98`), so on those images the name comes from that
//! store. Tables of at least two entries are kept, and a one-entry table when a registry row names
//! it. A table that two or more rows or stores name with different identifiers is an error.
//!
//! **Code stores.** Each function from [`crate::starts::function_starts`] is evaluated over its
//! control flow (to the next start) with the register values that are the same constant on every
//! path from the function's entry: `r1` is the entry stack pointer, `r2` on PS3 is the TOC of the
//! function's `.opd` descriptor, and every other register starts unknown. `li`/`lis`/`addi`/`addis`,
//! `ori`/`oris`, `mr`/`or`, `clrldi 32`, `lwz`/`lwzu` and `stw`/`stwu` are modelled; a word load
//! from a constant address reads the image's bytes there, and a word load from the stack reads the
//! value the function stored to that slot. Other loads make their target unknown, other stores
//! forget the stack slots they overlap (all of them when the address is unknown), and update forms
//! set their base register to the address. A call (`bl`, `bcl`, `bclrl`, `bcctrl`, `sc`) clobbers
//! `r0`, `r3`–`r12` and the stack slots. Compares, traps and condition-register logic change no
//! general register; any other instruction makes the registers in both of its register fields
//! unknown and clears the stack slots. A branch out of the function ends that path. Every `stw`
//! whose address and value are both constant is a code store. An x86 (PC) image has no code-store
//! pass; its names come from its data rows.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::mem::Mem;
use crate::starts::function_starts;
use crate::Error;

/// One registration table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub va: u32,
    /// The identifier a registry row (in the image's bytes or written by a code store) gives this
    /// table.
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

    let stores = if mem.big { code_stores(image, &mem)? } else { BTreeMap::new() };
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    for (va, w) in mem.data_words() {
        if !found.contains_key(&w) || va < 4 {
            continue;
        }
        let slot = va - 4;
        let written = stores.get(&slot).into_iter().flatten().copied();
        for ns in mem.word(slot).into_iter().chain(written).filter_map(|n| identifier(&mem, n)) {
            if let Some(prev) = names.get(&w) {
                if *prev != ns {
                    return Err(Error::new(format!(
                        "luaL_Reg table 0x{w:08x} is named both {prev} and {ns} by registry rows or code stores"
                    )));
                }
            }
            names.insert(w, ns);
        }
    }

    Ok(found
        .into_iter()
        .filter(|(va, entries)| entries.len() >= 2 || names.contains_key(va))
        .map(|(va, entries)| Table { va, namespace: names.get(&va).cloned(), entries })
        .collect())
}

/// A register or stack-slot value the evaluator knows: a constant, or the function's entry stack
/// pointer plus an offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Val {
    Const(u32),
    Stack(i32),
}

impl Val {
    fn add(self, d: i32) -> Val {
        match self {
            Val::Const(x) => Val::Const(x.wrapping_add(d as u32)),
            Val::Stack(k) => Val::Stack(k.wrapping_add(d)),
        }
    }
}

/// The known registers and 32-bit stack slots (keyed by entry-stack offset) at one point.
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    r: [Option<Val>; 32],
    stack: BTreeMap<i32, Val>,
}

impl State {
    /// Keep only what `other` agrees on; true when anything was dropped.
    fn meet(&mut self, other: &State) -> bool {
        let mut changed = false;
        for (a, b) in self.r.iter_mut().zip(other.r.iter()) {
            if a.is_some() && a != b {
                *a = None;
                changed = true;
            }
        }
        let before = self.stack.len();
        self.stack.retain(|k, v| other.stack.get(k) == Some(v));
        changed || self.stack.len() != before
    }

    fn clobber_call(&mut self) {
        self.r[0] = None;
        for r in &mut self.r[3..=12] {
            *r = None;
        }
        self.stack.clear();
    }

    /// Forget the stack slots that overlap `[k, k + len)`.
    fn forget_stack(&mut self, k: i32, len: i32) {
        self.stack.retain(|&s, _| s + 4 <= k || s >= k + len);
    }

    /// The effect of a store of `len` bytes at `ea` on the stack slots.
    fn store_effect(&mut self, ea: Option<Val>, len: i32) {
        match ea {
            Some(Val::Stack(k)) => self.forget_stack(k, len),
            Some(Val::Const(_)) => {}
            None => self.stack.clear(),
        }
    }
}

/// Where control goes after one instruction.
enum Flow {
    Next,
    /// A branch: its in-function target (when it has one), and whether it can fall through.
    Branch(Option<u32>, bool),
    End,
}

fn simm(w: u32) -> i32 {
    (w & 0xFFFF) as u16 as i16 as i32
}

/// Execute one instruction on `s`, pushing every constant-address, constant-value `stw` onto
/// `stores` when it is given. `inside` says whether a branch target lies in the function.
fn step(
    mem: &Mem<'_>,
    s: &mut State,
    va: u32,
    w: u32,
    inside: &dyn Fn(u32) -> bool,
    stores: Option<&mut Vec<(u32, u32)>>,
) -> Flow {
    let op = w >> 26;
    let d = ((w >> 21) & 31) as usize;
    let a = ((w >> 16) & 31) as usize;
    let b = ((w >> 11) & 31) as usize;
    let base = |s: &State| if a == 0 { Some(Val::Const(0)) } else { s.r[a] };
    let ea = |s: &State| base(s).map(|v| v.add(simm(w)));
    let load = |s: &State, at: Option<Val>| match at {
        Some(Val::Const(x)) => mem.word(x).map(Val::Const),
        Some(Val::Stack(k)) => s.stack.get(&k).copied(),
        None => None,
    };
    let unknown = |s: &mut State| {
        s.r[d] = None;
        s.r[a] = None;
        s.stack.clear();
    };
    match op {
        // addi / addis
        14 => s.r[d] = base(s).map(|v| v.add(simm(w))),
        15 => {
            s.r[d] = match base(s) {
                Some(Val::Const(x)) => Some(Val::Const(x.wrapping_add((simm(w) as u32) << 16))),
                _ => None,
            }
        }
        // ori / oris
        24 | 25 => {
            let imm = if op == 24 { w & 0xFFFF } else { (w & 0xFFFF) << 16 };
            s.r[a] = match s.r[d] {
                v if imm == 0 => v,
                Some(Val::Const(x)) => Some(Val::Const(x | imm)),
                _ => None,
            };
        }
        // compares and traps write no general register.
        2 | 3 | 10 | 11 => {}
        // rldicl: only `clrldi rA, rS, 32` keeps a 32-bit value.
        30 if (w >> 2) & 7 == 0 => {
            let sh = ((w >> 11) & 31) | ((w >> 1) & 1) << 5;
            let mb = ((w >> 6) & 31) | ((w >> 5) & 1) << 5;
            s.r[a] = if sh == 0 && mb == 32 { s.r[d] } else { None };
        }
        // lwz / lwzu
        32 | 33 => {
            let at = ea(s);
            s.r[d] = load(s, at);
            if op == 33 {
                s.r[a] = at;
            }
        }
        // lbz, lbzu, lhz, lhzu, lha, lhau
        34 | 35 | 40..=43 => {
            let at = ea(s);
            s.r[d] = None;
            if op % 2 == 1 {
                s.r[a] = at;
            }
        }
        // stw / stwu
        36 | 37 => {
            let at = ea(s);
            let v = s.r[d];
            match at {
                Some(Val::Stack(k)) => {
                    s.forget_stack(k, 4);
                    if let Some(v) = v {
                        s.stack.insert(k, v);
                    }
                }
                Some(Val::Const(x)) => {
                    if let (Some(out), Some(Val::Const(v))) = (stores, v) {
                        out.push((x, v));
                    }
                }
                None => s.stack.clear(),
            }
            if op == 37 {
                s.r[a] = at;
            }
        }
        // stb, stbu, sth, sthu, stfs, stfsu, stfd, stfdu
        38 | 39 | 44 | 45 | 52..=55 => {
            let at = ea(s);
            let len = match op {
                38 | 39 => 1,
                44 | 45 => 2,
                52 | 53 => 4,
                _ => 8,
            };
            s.store_effect(at, len);
            if op % 2 == 1 {
                s.r[a] = at;
            }
        }
        // lmw writes rD..r31; stmw stores them.
        46 => {
            for r in &mut s.r[d..] {
                *r = None;
            }
        }
        47 => {
            let at = ea(s);
            s.store_effect(at, 4 * (32 - d as i32));
        }
        // lfs, lfsu, lfd, lfdu
        48..=51 => {
            if op % 2 == 1 {
                s.r[a] = ea(s);
            }
        }
        // ld, ldu, lwa (DS form)
        58 => {
            let at = base(s).map(|v| v.add(simm(w & !3)));
            s.r[d] = None;
            if w & 3 == 1 {
                s.r[a] = at;
            }
        }
        // std, stdu (DS form)
        62 => {
            let at = base(s).map(|v| v.add(simm(w & !3)));
            s.store_effect(at, 8);
            if w & 3 == 1 {
                s.r[a] = at;
            }
        }
        31 => match (w >> 1) & 0x3FF {
            // cmp, cmpl
            0 | 32 => {}
            // or (mr when rS == rB)
            444 => {
                s.r[a] = match (s.r[d], s.r[b]) {
                    (v, _) if d == b => v,
                    (Some(Val::Const(x)), Some(Val::Const(y))) => Some(Val::Const(x | y)),
                    _ => None,
                }
            }
            // lswi, lswx write a run of registers that wraps through r0.
            533 | 597 => s.r = [None; 32],
            _ => unknown(s),
        },
        // sc
        17 => s.clobber_call(),
        // b, ba, bl, bla
        18 => {
            let li = ((w & 0x03FF_FFFC) << 6) as i32 >> 6;
            let target = if w & 2 != 0 { li as u32 } else { va.wrapping_add(li as u32) };
            if w & 1 != 0 {
                s.clobber_call();
                return Flow::Next;
            }
            return if inside(target) { Flow::Branch(Some(target), false) } else { Flow::End };
        }
        // bc
        16 => {
            let bd = simm(w & !3);
            let target = if w & 2 != 0 { bd as u32 } else { va.wrapping_add(bd as u32) };
            if w & 1 != 0 {
                s.clobber_call();
                return Flow::Next;
            }
            let always = (w >> 21) & 0b10100 == 0b10100;
            return Flow::Branch(inside(target).then_some(target), !always);
        }
        19 => {
            let xo = (w >> 1) & 0x3FF;
            if xo == 16 || xo == 528 {
                if w & 1 != 0 {
                    s.clobber_call();
                    return Flow::Next;
                }
                let always = (w >> 21) & 0b10100 == 0b10100;
                return if always { Flow::End } else { Flow::Branch(None, true) };
            }
            // condition-register logic, mcrf, isync: no general register.
        }
        _ => unknown(s),
    }
    Flow::Next
}

/// Every constant-address word store a PowerPC image's code makes with a constant value, as
/// address -> the values stored there.
fn code_stores(image: &[u8], mem: &Mem<'_>) -> Result<BTreeMap<u32, BTreeSet<u32>>, Error> {
    let (starts, _) = function_starts(image)?;
    let mut toc: BTreeMap<u32, u32> = BTreeMap::new();
    if let Some((lo, hi)) = mem.opd {
        for p in (lo..hi).step_by(8) {
            let (Some(code), Some(t)) = (mem.word(p), mem.word(p + 4)) else {
                return Err(Error::new(format!(".opd descriptor 0x{p:08x} is not mapped")));
            };
            if let Some(prev) = toc.insert(code, t) {
                if prev != t {
                    return Err(Error::new(format!(
                        "function 0x{code:08x} has .opd descriptors with TOC 0x{prev:08x} and 0x{t:08x}"
                    )));
                }
            }
        }
    }
    let mut out: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (i, &start) in starts.iter().enumerate() {
        let range = mem
            .ranges
            .iter()
            .find(|r| r.exec && start >= r.lo && start < r.hi)
            .ok_or_else(|| Error::new(format!("function start 0x{start:08x} is not in a mapped code section")))?;
        let end = starts.get(i + 1).map_or(range.hi, |&n| n.min(range.hi));
        let mut entry = State { r: [None; 32], stack: BTreeMap::new() };
        entry.r[1] = Some(Val::Stack(0));
        if mem.opd.is_some() {
            let t = toc
                .get(&start)
                .ok_or_else(|| Error::new(format!("function start 0x{start:08x} has no .opd descriptor")))?;
            entry.r[2] = Some(Val::Const(*t));
        }
        for (ea, v) in function_stores(mem, start, end, entry)? {
            out.entry(ea).or_default().insert(v);
        }
    }
    Ok(out)
}

/// The constant stores of the function `[start, end)`, evaluated to a fixpoint over its blocks.
fn function_stores(mem: &Mem<'_>, start: u32, end: u32, entry: State) -> Result<Vec<(u32, u32)>, Error> {
    let inside = |t: u32| t >= start && t < end && t.is_multiple_of(4);
    let word = |va: u32| mem.word(va).ok_or_else(|| Error::new(format!("code word 0x{va:08x} is not mapped")));
    // Block leaders: the entry, every in-function branch target, and the instruction after every
    // branch.
    let mut leaders: BTreeSet<u32> = BTreeSet::from([start]);
    let mut scratch = State { r: [None; 32], stack: BTreeMap::new() };
    let mut va = start;
    while va < end {
        let w = word(va)?;
        if let Flow::Branch(Some(target), _) = step(mem, &mut scratch, va, w, &inside, None) {
            leaders.insert(target);
        }
        if matches!(w >> 26, 16 | 18 | 19) {
            leaders.insert(va + 4);
        }
        va += 4;
    }
    let mut heads: BTreeMap<u32, State> = BTreeMap::from([(start, entry)]);
    let mut work: VecDeque<u32> = VecDeque::from([start]);
    let run = |h: u32, s: &mut State, mut stores: Option<&mut Vec<(u32, u32)>>| -> Result<Vec<u32>, Error> {
        let mut va = h;
        loop {
            match step(mem, s, va, word(va)?, &inside, stores.as_deref_mut()) {
                Flow::End => return Ok(Vec::new()),
                Flow::Branch(target, falls) => {
                    let mut next: Vec<u32> = target.into_iter().collect();
                    if falls && va + 4 < end {
                        next.push(va + 4);
                    }
                    return Ok(next);
                }
                Flow::Next => {
                    va += 4;
                    if va >= end {
                        return Ok(Vec::new());
                    }
                    if leaders.contains(&va) {
                        return Ok(vec![va]);
                    }
                }
            }
        }
    };
    while let Some(h) = work.pop_front() {
        let mut s = heads[&h].clone();
        for n in run(h, &mut s, None)? {
            match heads.get_mut(&n) {
                Some(prev) => {
                    if prev.meet(&s) {
                        work.push_back(n);
                    }
                }
                None => {
                    heads.insert(n, s.clone());
                    work.push_back(n);
                }
            }
        }
    }
    let mut stores = Vec::new();
    for (&h, s) in &heads {
        run(h, &mut s.clone(), Some(&mut stores))?;
    }
    Ok(stores)
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

    const XBASE: u32 = 0x8200_0000;
    const R11: u32 = 11;
    const R9: u32 = 9;
    const R10: u32 = 10;
    const R1: u32 = 1;
    const BLR: u32 = 0x4E80_0020;

    fn lis(d: u32, imm: u32) -> u32 {
        15 << 26 | d << 21 | imm
    }
    fn addi(d: u32, a: u32, imm: i16) -> u32 {
        14 << 26 | d << 21 | a << 16 | imm as u16 as u32
    }
    fn lwz(d: u32, a: u32, imm: i16) -> u32 {
        32 << 26 | d << 21 | a << 16 | imm as u16 as u32
    }
    fn stw(s: u32, a: u32, imm: i16) -> u32 {
        36 << 26 | s << 21 | a << 16 | imm as u16 as u32
    }
    fn stwu(s: u32, a: u32, imm: i16) -> u32 {
        37 << 26 | s << 21 | a << 16 | imm as u16 as u32
    }
    /// `beq cr0, +off`.
    fn beq(off: i16) -> u32 {
        16 << 26 | 12 << 21 | 2 << 16 | (off as u16 as u32 & 0xFFFC)
    }
    /// `bl +off`.
    fn bl(off: i32) -> u32 {
        18 << 26 | (off as u32 & 0x03FF_FFFC) | 1
    }

    /// An Xbox 360 image with one function (`code`, at `.text` 0x82001000) and `.rdata` holding
    /// the strings "Player" (0x82000600), "GetName", "GetSeat", "Other" (0x82000618), a name
    /// variable at 0x82000620 holding the "Player" pointer, a two-entry table at 0x82000640, and a
    /// registry row `{name, table, 0}` at 0x82000680 whose name word is `row_name`.
    fn xbox(code: &[u32], row_name: u32) -> Vec<u8> {
        let be = |v: u32| v.to_be_bytes();
        let mut rdata = vec![0u8; 0x200];
        rdata[0x00..0x08].copy_from_slice(b"Player\0\0");
        rdata[0x08..0x18].copy_from_slice(b"GetName\0GetSeat\0");
        rdata[0x18..0x20].copy_from_slice(b"Other\0\0\0");
        let s = |o: u32| XBASE + 0x600 + o;
        rdata[0x20..0x24].copy_from_slice(&be(s(0x00)));
        rdata[0x40..0x44].copy_from_slice(&be(s(0x08)));
        rdata[0x44..0x48].copy_from_slice(&be(XBASE + 0x1080));
        rdata[0x48..0x4c].copy_from_slice(&be(s(0x10)));
        rdata[0x4c..0x50].copy_from_slice(&be(XBASE + 0x1084));
        rdata[0x80..0x84].copy_from_slice(&be(row_name));
        rdata[0x84..0x88].copy_from_slice(&be(s(0x40)));
        let mut pdata = vec![0u8; 0x40];
        pdata[0..4].copy_from_slice(&be(XBASE + 0x1000));
        pdata[4..8].copy_from_slice(&be(1));
        let text: Vec<u8> = code.iter().flat_map(|w| w.to_be_bytes()).collect();
        pe_image(
            crate::xbox::MACHINE_XENON,
            XBASE,
            &[
                (".pdata", 0x400, 0x40, 0x4000_0040, &pdata),
                (".rdata", 0x600, 0x200, 0x4000_0040, &rdata),
                (".text", 0x1000, 0x100, 0x6000_0020, &text),
            ],
        )
    }

    /// `r9 = &row.name`.
    const ROW: [u32; 2] = [15 << 26 | R9 << 21 | 0x8200, 14 << 26 | R9 << 21 | R9 << 16 | 0x680];

    fn namespace_of(img: &[u8]) -> Option<String> {
        let t = tables(img).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].va, XBASE + 0x640);
        t[0].namespace.clone()
    }

    #[test]
    fn a_powerpc_code_store_names_the_row_its_data_leaves_empty() {
        // r11 = *(0x82000620) = "Player"; row.name = r11
        let code = [lis(R11, 0x8200), lwz(R11, R11, 0x620), ROW[0], ROW[1], stw(R11, R9, 0), BLR];
        assert_eq!(namespace_of(&xbox(&code, 0)).as_deref(), Some("Player"));
        // Without the store the row is unnamed.
        assert_eq!(namespace_of(&xbox(&[BLR], 0)), None);
    }

    #[test]
    fn a_value_spilled_to_the_stack_is_read_back() {
        let code = [
            stwu(R1, R1, -32),
            lis(R11, 0x8200),
            lwz(R11, R11, 0x620),
            stw(R11, R1, 8),
            lis(R11, 0),
            lwz(R10, R1, 8),
            ROW[0],
            ROW[1],
            stw(R10, R9, 0),
            BLR,
        ];
        assert_eq!(namespace_of(&xbox(&code, 0)).as_deref(), Some("Player"));
        // A call between the spill and the reload forgets the slot.
        let mut called = code;
        called[4] = bl(0x40);
        assert_eq!(namespace_of(&xbox(&called, 0)), None);
    }

    #[test]
    fn a_value_two_paths_disagree_on_is_not_a_store() {
        let join = |other: u32| {
            [
                lis(R11, 0x8200),
                addi(R11, R11, 0x600),
                beq(12),
                lis(R11, 0x8200),
                addi(R11, R11, other as i16),
                ROW[0],
                ROW[1],
                stw(R11, R9, 0),
                BLR,
            ]
        };
        assert_eq!(namespace_of(&xbox(&join(0x618), 0)), None);
        assert_eq!(namespace_of(&xbox(&join(0x600), 0)).as_deref(), Some("Player"));
    }

    #[test]
    fn a_code_store_that_contradicts_the_data_row_is_an_error() {
        let code = [lis(R11, 0x8200), lwz(R11, R11, 0x620), ROW[0], ROW[1], stw(R11, R9, 0), BLR];
        let e = tables(&xbox(&code, XBASE + 0x618)).unwrap_err().to_string();
        assert!(e.contains("named both"), "{e}");
    }
}
