//! Pairwise function matching between two images, from their exports.
//!
//! Every rule proposes candidate pairs `(a, b)` of function entry addresses, each tagged with the
//! evidence that proposed it, and every rule is a uniqueness or identity test (no similarity
//! scores):
//!
//! | evidence | rule |
//! |---|---|
//! | `vtable` | the same class (`Foo::Bar`, or the decorated name when the ABIs agree), the same vtable ordinal within the class, the same slot — used only when the class has as many vtables in both images, and each vtable pair either has as many slots, or is an MSVC vtable with a recognised destructor slot `k` against an Itanium vtable with one slot more: MSVC slot `i < k` pairs with Itanium slot `i`, `i > k` with `i + 1`, and the destructor itself (one MSVC slot for two Itanium ones) is not paired |
//! | `lua` | the same Lua binding: the same namespace and name when both tables are named, or a name that occurs once among all bindings of each image |
//! | `bytes` | the same masked-bytes hash (`mhash`), occurring once in each image (same Ghidra language only) |
//! | `string` | the same referenced string, referenced by exactly one function in each image; on its own it does not accept a pair (see **corroborated** below) |
//! | `callee` | the direct callees at the same position of an accepted pair whose call lists (in order) have the same length and agree at every position, where only two direct callees that are both function starts (keys of their image's fnsig export) form a candidate (same Ghidra language only: across compilers, inlining shifts call lists, so equal length is no alignment) |
//! | `anchor` | a `data/anchors.tsv` row for exactly this pair of the two images (in either orientation); it is added to a candidate another rule proposed and never proposes a candidate itself. It is always applied and is not chosen in the evidence list |
//!
//! Every function of a fnsig export has a string `mhash` and a `callees` and a `strs` array of
//! strings; a missing or mistyped one is an error naming the export file and the function. Call-list
//! entries are direct callees (`0x…`), imported functions (`ext:<library>!<name>`), and indirect
//! calls (`*0x…`, a call through a 32-bit memory slot; `*stack:<signed hex>`, a call through a
//! function pointer held in the stack variable at that offset; and `?`, a computed target); any
//! other entry is an error naming the export file and the function. Two aligned entries agree when
//! both are indirect, whatever their slot address or stack offset; when both are imports with the
//! same `<library>` and the same `<name>` (exact text: one ordinal name can be exported by two
//! libraries); or when both are direct and neither is half of an accepted pair whose other half is
//! not the aligned entry. Any other combination disagrees. An import is never a candidate, and
//! neither is a direct callee that is not a function start in its image (Ghidra writes an import
//! it cannot name as a small `0x…` number, which is no function start).
//!
//! A pair is **corroborated** when it has evidence other than `string`, or when two or more
//! distinct strings each give it `string` evidence. A pair is **accepted** when its `a` has no
//! other candidate, its `b` has no other candidate, and it is corroborated. An uncorroborated
//! candidate is still a candidate: it makes a **conflict** with every other candidate of its
//! functions, and when it is the only candidate of both its functions it is reported as
//! **uncorroborated**. Every conflict is reported with all its candidates; nothing is chosen among
//! them. `callee` candidates come from accepted pairs only (an uncorroborated pair neither proposes
//! callees nor constrains the call-list agreement), and the rules run to a fixpoint: the candidate
//! set and each candidate's evidence only grow, so acceptance is recomputed until it stops
//! changing; `callee` evidence that lands on an uncorroborated `string` candidate corroborates it.
//!
//! An `anchor` candidate is corroborated like any candidate with evidence other than `string`, and
//! it is accepted only when it has no rivals: an anchored candidate with another candidate on
//! either side stays a conflict.
//!
//! `data/anchors.tsv` lists pairs verified by reading both decompiles; [`check_anchors`] reports
//! every accepted pair that contradicts one.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Evidence {
    Vtable,
    Lua,
    Bytes,
    String,
    Callee,
    Anchor,
}

impl Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Evidence::Vtable => "vtable",
            Evidence::Lua => "lua",
            Evidence::Bytes => "bytes",
            Evidence::String => "string",
            Evidence::Callee => "callee",
            Evidence::Anchor => "anchor",
        }
    }
    pub fn parse(s: &str) -> Option<Evidence> {
        match s {
            "vtable" => Some(Evidence::Vtable),
            "lua" => Some(Evidence::Lua),
            "bytes" => Some(Evidence::Bytes),
            "string" => Some(Evidence::String),
            "callee" => Some(Evidence::Callee),
            "anchor" => Some(Evidence::Anchor),
            _ => None,
        }
    }
}

/// One `exemap rtti` row.
#[derive(Clone, Debug)]
pub struct VtableSlot {
    pub abi: crate::rtti::Abi,
    pub raw_name: String,
    pub class: String,
    pub vtable: u32,
    pub subobject_offset: i64,
    pub slot: usize,
    pub function: u32,
    /// The vtable's MSVC deleting-destructor slot, when recognised.
    pub deleting_destructor: Option<usize>,
}

/// One `exemap luareg` row.
#[derive(Clone, Debug)]
pub struct Binding {
    pub namespace: String,
    pub name: String,
    pub function: u32,
}

/// One entry of a `FnMnemonicSig` call list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Callee {
    /// `0x…`: the target of a direct call.
    Direct(u32),
    /// `ext:<library>!<name>`: an imported function.
    Import { library: String, name: String },
    /// `*0x…` (a call through a 32-bit memory slot), `*stack:<signed hex>` (a call through a
    /// function pointer held in a stack variable) or `?` (a computed target).
    Indirect,
}

impl Callee {
    /// Parse one call-list entry; an entry of none of the forms `0x…`, `*0x…`, `*stack:-0x…`,
    /// `*stack:0x…`, `?` and `ext:<library>!<name>` is an error.
    pub fn parse(s: &str) -> Result<Callee, Error> {
        if let Some(rest) = s.strip_prefix("ext:") {
            let (library, name) =
                rest.split_once('!').ok_or_else(|| Error::new(format!("callee {s:?} is not ext:<library>!<name>")))?;
            if library.is_empty() || name.is_empty() {
                return Err(Error::new(format!("callee {s:?} has an empty library or name")));
            }
            Ok(Callee::Import { library: library.to_string(), name: name.to_string() })
        } else if s.starts_with("0x") {
            Ok(Callee::Direct(hex32(s, "callee")?))
        } else if let Some(offset) = s.strip_prefix("*stack:") {
            // A signed offset: an optional `-` and a 0x-prefixed magnitude of at most 32 bits.
            hex32(offset.strip_prefix('-').unwrap_or(offset), "callee stack offset")
                .map_err(|e| Error::new(format!("callee {s:?}: {e}")))?;
            Ok(Callee::Indirect)
        } else if let Some(slot) = s.strip_prefix('*') {
            hex32(slot, "callee slot").map_err(|e| Error::new(format!("callee {s:?}: {e}")))?;
            Ok(Callee::Indirect)
        } else if s == "?" {
            Ok(Callee::Indirect)
        } else {
            Err(Error::new(format!("callee {s:?} is none of 0x…, *0x…, *stack:<signed hex>, ? and ext:<library>!<name>")))
        }
    }
}

/// The parts of one `FnMnemonicSig` record the rules read.
#[derive(Clone, Debug, Default)]
pub struct Signature {
    pub mhash: String,
    /// Callees in call order.
    pub callees: Vec<Callee>,
    /// Referenced strings (raw bytes as hex).
    pub strs: Vec<String>,
}

/// One image's exports.
#[derive(Clone, Debug, Default)]
pub struct Exports {
    pub vtables: Vec<VtableSlot>,
    pub bindings: Vec<Binding>,
    pub signatures: Option<HashMap<u32, Signature>>,
}

fn hex32(s: &str, what: &str) -> Result<u32, Error> {
    let h = s.strip_prefix("0x").ok_or_else(|| Error::new(format!("{what} {s:?} is not 0x-prefixed")))?;
    u32::from_str_radix(h, 16).map_err(|e| Error::new(format!("{what} {s:?}: {e}")))
}

fn tsv(path: &Path, header: &str) -> Result<Vec<Vec<String>>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::new(format!("read {}: {e}", path.display())))?;
    let mut lines = text.lines();
    if lines.next() != Some(header) {
        return Err(Error::new(format!("{}: header is not {header:?}", path.display())));
    }
    let width = header.split('\t').count();
    lines
        .enumerate()
        .map(|(i, l)| {
            let f: Vec<String> = l.split('\t').map(str::to_string).collect();
            if f.len() != width {
                return Err(Error::new(format!("{} line {}: {} fields, expected {width}", path.display(), i + 2, f.len())));
            }
            Ok(f)
        })
        .collect()
}

impl Exports {
    /// Load `<dir>/<id>.rtti.tsv`, `<dir>/<id>.luareg.tsv` and, when `signatures` is set,
    /// `<dir>/<id>.fnsig.json`. A missing file is an error naming it.
    pub fn load(dir: &Path, id: &str, signatures: bool) -> Result<Exports, Error> {
        let mut vtables = Vec::new();
        for f in tsv(
            &dir.join(format!("{id}.rtti.tsv")),
            "abi\traw_name\tclass\tvtable\tsubobject_offset\tslot\tfunction\tdeleting_destructor",
        )? {
            let abi = match f[0].as_str() {
                "msvc" => crate::rtti::Abi::Msvc,
                "itanium" => crate::rtti::Abi::Itanium,
                other => return Err(Error::new(format!("rtti abi {other:?} is neither msvc nor itanium"))),
            };
            vtables.push(VtableSlot {
                abi,
                raw_name: f[1].clone(),
                class: f[2].clone(),
                vtable: hex32(&f[3], "vtable")?,
                subobject_offset: f[4].parse().map_err(|e| Error::new(format!("subobject_offset {:?}: {e}", f[4])))?,
                slot: f[5].parse().map_err(|e| Error::new(format!("slot {:?}: {e}", f[5])))?,
                function: hex32(&f[6], "function")?,
                deleting_destructor: if f[7].is_empty() {
                    None
                } else {
                    Some(f[7].parse().map_err(|e| Error::new(format!("deleting_destructor {:?}: {e}", f[7])))?)
                },
            });
        }
        let mut bindings = Vec::new();
        for f in tsv(&dir.join(format!("{id}.luareg.tsv")), "table\tnamespace\tindex\tname\tfunction")? {
            bindings.push(Binding { namespace: f[1].clone(), name: f[3].clone(), function: hex32(&f[4], "function")? });
        }
        let signatures = if signatures {
            let path = dir.join(format!("{id}.fnsig.json"));
            let text = std::fs::read_to_string(&path).map_err(|e| Error::new(format!("read {}: {e}", path.display())))?;
            Some(parse_signatures(&text, &path.display().to_string())?)
        } else {
            None
        };
        Ok(Exports { vtables, bindings, signatures })
    }
}

/// Parse a fnsig export (`file` names it in errors). Every function must have a string `mhash`
/// and a `callees` and a `strs` array whose entries are all strings, and every callee must parse;
/// anything else is an error naming the file and the function.
pub fn parse_signatures(text: &str, file: &str) -> Result<HashMap<u32, Signature>, Error> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| Error::new(format!("{file}: {e}")))?;
    let obj = v.as_object().ok_or_else(|| Error::new(format!("{file}: not a JSON object")))?;
    let mut m = HashMap::with_capacity(obj.len());
    for (k, rec) in obj {
        let addr = hex32(k, "function")?;
        let strings = |key: &str| -> Result<Vec<String>, Error> {
            let list = rec
                .get(key)
                .ok_or_else(|| Error::new(format!("{file} {k}: no {key:?} key")))?
                .as_array()
                .ok_or_else(|| Error::new(format!("{file} {k}: {key:?} is not an array")))?;
            list.iter()
                .map(|x| x.as_str().map(str::to_string).ok_or_else(|| Error::new(format!("{file} {k}: {key:?} entry {x} is not a string"))))
                .collect()
        };
        let callees = strings("callees")?
            .iter()
            .map(|c| Callee::parse(c).map_err(|e| Error::new(format!("{file} {k}: {e}"))))
            .collect::<Result<Vec<_>, _>>()?;
        m.insert(
            addr,
            Signature {
                mhash: rec
                    .get("mhash")
                    .ok_or_else(|| Error::new(format!("{file} {k}: no \"mhash\" key")))?
                    .as_str()
                    .ok_or_else(|| Error::new(format!("{file} {k}: \"mhash\" is not a string")))?
                    .to_string(),
                callees,
                strs: strings("strs")?,
            },
        );
    }
    Ok(m)
}

/// An accepted pair and the evidence behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub a: u32,
    pub b: u32,
    pub evidence: BTreeSet<Evidence>,
}

/// A function with more than one candidate partner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// `'a'` or `'b'`: the side the function is on.
    pub side: char,
    pub function: u32,
    pub candidates: Vec<(u32, BTreeSet<Evidence>)>,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub pairs: Vec<Pair>,
    pub conflicts: Vec<Conflict>,
    /// Candidates that are the only candidate of both their functions but are not corroborated.
    pub uncorroborated: Vec<Pair>,
}

/// The evidence behind one candidate pair, and how many distinct strings gave it `string`.
#[derive(Clone, Debug, Default)]
struct Support {
    evidence: BTreeSet<Evidence>,
    strings: usize,
}

impl Support {
    fn corroborated(&self) -> bool {
        self.evidence.iter().any(|e| *e != Evidence::String) || self.strings >= 2
    }
}

type Candidates = BTreeMap<(u32, u32), Support>;

/// Record `e` for `(a, b)`; true when the pair did not have `e` before.
fn add(c: &mut Candidates, a: u32, b: u32, e: Evidence) -> bool {
    c.entry((a, b)).or_default().evidence.insert(e)
}

/// Record one more distinct string that gives `(a, b)` `string` evidence.
fn add_string(c: &mut Candidates, a: u32, b: u32) {
    let s = c.entry((a, b)).or_default();
    s.evidence.insert(Evidence::String);
    s.strings += 1;
}

/// One vtable of a class: its ABI, MSVC destructor slot, and slot functions.
struct Vt {
    abi: crate::rtti::Abi,
    dtor: Option<usize>,
    slots: Vec<u32>,
}

/// Slot index pairs (MSVC or Itanium slot `i` with the other vtable's slot `j`) for two vtables of
/// the same class, or `None` when their shapes do not determine an alignment.
fn align(a: &Vt, b: &Vt) -> Option<Vec<(usize, usize)>> {
    use crate::rtti::Abi;
    if a.slots.len() == b.slots.len() && a.abi == b.abi {
        return Some((0..a.slots.len()).map(|i| (i, i)).collect());
    }
    let shifted = |m: &Vt, n: &Vt| -> Option<Vec<(usize, usize)>> {
        let k = m.dtor?;
        if m.abi != Abi::Msvc || n.abi != Abi::Itanium || n.slots.len() != m.slots.len() + 1 {
            return None;
        }
        Some((0..m.slots.len()).filter(|i| *i != k).map(|i| (i, if i < k { i } else { i + 1 })).collect())
    };
    if let Some(v) = shifted(a, b) {
        return Some(v);
    }
    shifted(b, a).map(|v| v.into_iter().map(|(i, j)| (j, i)).collect())
}

fn vtable_candidates(a: &Exports, b: &Exports, c: &mut Candidates) {
    // class key -> vtables (by subobject offset, then address)
    fn index(v: &[VtableSlot]) -> HashMap<String, Vec<Vt>> {
        let mut by_class: HashMap<String, BTreeMap<(i64, u32), (crate::rtti::Abi, Option<usize>, BTreeMap<usize, u32>)>> = HashMap::new();
        for s in v {
            let key = if s.class.is_empty() { s.raw_name.clone() } else { s.class.clone() };
            let e = by_class.entry(key).or_default().entry((s.subobject_offset, s.vtable)).or_insert((s.abi, s.deleting_destructor, BTreeMap::new()));
            e.2.insert(s.slot, s.function);
        }
        by_class
            .into_iter()
            .map(|(k, vts)| (k, vts.into_values().map(|(abi, dtor, slots)| Vt { abi, dtor, slots: slots.into_values().collect() }).collect()))
            .collect()
    }
    let ia = index(&a.vtables);
    let ib = index(&b.vtables);
    for (class, va) in &ia {
        let Some(vb) = ib.get(class) else { continue };
        if va.len() != vb.len() {
            continue;
        }
        for (x, y) in va.iter().zip(vb) {
            let Some(pairs) = align(x, y) else { continue };
            for (i, j) in pairs {
                add(c, x.slots[i], y.slots[j], Evidence::Vtable);
            }
        }
    }
}

fn lua_candidates(a: &Exports, b: &Exports, c: &mut Candidates) {
    fn named(v: &[Binding]) -> HashMap<(&str, &str), Vec<u32>> {
        let mut m: HashMap<(&str, &str), Vec<u32>> = HashMap::new();
        for x in v.iter().filter(|x| !x.namespace.is_empty()) {
            m.entry((x.namespace.as_str(), x.name.as_str())).or_default().push(x.function);
        }
        m
    }
    fn unique(v: &[Binding]) -> HashMap<&str, u32> {
        let mut n: HashMap<&str, Vec<u32>> = HashMap::new();
        for x in v {
            n.entry(x.name.as_str()).or_default().push(x.function);
        }
        n.into_iter().filter(|(_, f)| f.len() == 1).map(|(k, f)| (k, f[0])).collect()
    }
    let (na, nb) = (named(&a.bindings), named(&b.bindings));
    for (k, fa) in &na {
        if let Some(fb) = nb.get(k) {
            if fa.len() == 1 && fb.len() == 1 {
                add(c, fa[0], fb[0], Evidence::Lua);
            }
        }
    }
    let (ua, ub) = (unique(&a.bindings), unique(&b.bindings));
    for (name, fa) in &ua {
        if let Some(fb) = ub.get(name) {
            add(c, *fa, *fb, Evidence::Lua);
        }
    }
}

fn unique_by<'a, F: Fn(&'a Signature) -> Vec<&'a str>>(s: &'a HashMap<u32, Signature>, f: F) -> HashMap<&'a str, u32> {
    let mut m: HashMap<&str, Vec<u32>> = HashMap::new();
    for (addr, sig) in s {
        for k in f(sig).into_iter().collect::<BTreeSet<_>>() {
            if !k.is_empty() {
                m.entry(k).or_default().push(*addr);
            }
        }
    }
    m.into_iter().filter(|(_, v)| v.len() == 1).map(|(k, v)| (k, v[0])).collect()
}

/// `bytes` and `string` candidates; `bytes` is set only when it is chosen and the images share a
/// Ghidra language.
fn signature_candidates(sa: &HashMap<u32, Signature>, sb: &HashMap<u32, Signature>, bytes: bool, string: bool, c: &mut Candidates) {
    if bytes {
        let (ua, ub) = (unique_by(sa, |s| vec![s.mhash.as_str()]), unique_by(sb, |s| vec![s.mhash.as_str()]));
        for (h, fa) in &ua {
            if let Some(fb) = ub.get(h) {
                add(c, *fa, *fb, Evidence::Bytes);
            }
        }
    }
    if !string {
        return;
    }
    let strs = |s: &HashMap<u32, Signature>| -> HashMap<String, u32> {
        let mut m: HashMap<&str, BTreeSet<u32>> = HashMap::new();
        for (addr, sig) in s {
            for x in &sig.strs {
                m.entry(x.as_str()).or_default().insert(*addr);
            }
        }
        m.into_iter().filter(|(_, v)| v.len() == 1).map(|(k, v)| (k.to_string(), *v.iter().next().unwrap())).collect()
    };
    let (ua, ub) = (strs(sa), strs(sb));
    // Each key of `ua` is a distinct string, so each call counts one more string for its pair.
    for (k, fa) in &ua {
        if let Some(fb) = ub.get(k) {
            add_string(c, *fa, *fb);
        }
    }
}

fn resolve(c: &Candidates) -> Outcome {
    let mut by_a: BTreeMap<u32, Vec<(u32, BTreeSet<Evidence>)>> = BTreeMap::new();
    let mut by_b: BTreeMap<u32, Vec<(u32, BTreeSet<Evidence>)>> = BTreeMap::new();
    for ((a, b), s) in c {
        by_a.entry(*a).or_default().push((*b, s.evidence.clone()));
        by_b.entry(*b).or_default().push((*a, s.evidence.clone()));
    }
    let mut out = Outcome::default();
    for ((a, b), s) in c {
        if by_a[a].len() == 1 && by_b[b].len() == 1 {
            let p = Pair { a: *a, b: *b, evidence: s.evidence.clone() };
            if s.corroborated() {
                out.pairs.push(p);
            } else {
                out.uncorroborated.push(p);
            }
        }
    }
    for (f, cands) in by_a.into_iter().filter(|(_, v)| v.len() > 1) {
        out.conflicts.push(Conflict { side: 'a', function: f, candidates: cands });
    }
    for (f, cands) in by_b.into_iter().filter(|(_, v)| v.len() > 1) {
        out.conflicts.push(Conflict { side: 'b', function: f, candidates: cands });
    }
    out
}

/// Add `anchor` evidence to every candidate that is one of `anchors`; no candidate is created.
fn add_anchors(c: &mut Candidates, anchors: &[(u32, u32)]) {
    for k in anchors {
        if let Some(s) = c.get_mut(k) {
            s.evidence.insert(Evidence::Anchor);
        }
    }
}

/// Match the functions of image `a` against image `b` with the chosen evidence.
/// `same_language` says whether both images are analysed with the same Ghidra language, i.e. built
/// by the same compiler for the same processor; `bytes` and `callee` evidence apply only then.
/// `anchors` are the verified pairs (image `a` function, image `b` function) of the two images
/// (see [`anchor_pairs`]); each adds `anchor` evidence to the candidate it names. `evidence` must
/// not contain `anchor`, which is always applied.
pub fn match_images(
    a: &Exports,
    b: &Exports,
    same_language: bool,
    evidence: &BTreeSet<Evidence>,
    anchors: &[(u32, u32)],
) -> Result<Outcome, Error> {
    if evidence.contains(&Evidence::Anchor) {
        return Err(Error::new("anchor evidence comes from data/anchors.tsv and is always applied; it is not chosen in the evidence list"));
    }
    let mut c = Candidates::new();
    if evidence.contains(&Evidence::Vtable) {
        vtable_candidates(a, b, &mut c);
    }
    if evidence.contains(&Evidence::Lua) {
        lua_candidates(a, b, &mut c);
    }
    let sigs = match (&a.signatures, &b.signatures) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    let wants_sigs = evidence.iter().any(|e| matches!(e, Evidence::Bytes | Evidence::String | Evidence::Callee));
    if wants_sigs && sigs.is_none() {
        return Err(Error::new("bytes, string and callee evidence need the fnsig export of both images"));
    }
    if let Some((sa, sb)) = sigs {
        let bytes = same_language && evidence.contains(&Evidence::Bytes);
        signature_candidates(sa, sb, bytes, evidence.contains(&Evidence::String), &mut c);
    }
    add_anchors(&mut c, anchors);
    let mut out = resolve(&c);
    if let (Some((sa, sb)), true, true) = (sigs, evidence.contains(&Evidence::Callee), same_language) {
        loop {
            let mut grew = false;
            let fwd: HashMap<u32, u32> = out.pairs.iter().map(|p| (p.a, p.b)).collect();
            let back: HashMap<u32, u32> = out.pairs.iter().map(|p| (p.b, p.a)).collect();
            for p in &out.pairs {
                let (Some(x), Some(y)) = (sa.get(&p.a), sb.get(&p.b)) else { continue };
                if x.callees.len() != y.callees.len() {
                    continue;
                }
                let consistent = x.callees.iter().zip(&y.callees).all(|(ca, cb)| match (ca, cb) {
                    (Callee::Direct(ca), Callee::Direct(cb)) => {
                        fwd.get(ca).is_none_or(|v| v == cb) && back.get(cb).is_none_or(|v| v == ca)
                    }
                    (Callee::Import { library: la, name: na }, Callee::Import { library: lb, name: nb }) => {
                        la == lb && na == nb
                    }
                    (Callee::Indirect, Callee::Indirect) => true,
                    _ => false,
                });
                if !consistent {
                    continue;
                }
                for (ca, cb) in x.callees.iter().zip(&y.callees) {
                    if let (Callee::Direct(ca), Callee::Direct(cb)) = (ca, cb) {
                        if sa.contains_key(ca) && sb.contains_key(cb) {
                            grew |= add(&mut c, *ca, *cb, Evidence::Callee);
                        }
                    }
                }
            }
            if !grew {
                break;
            }
            add_anchors(&mut c, anchors);
            out = resolve(&c);
        }
    }
    Ok(out)
}

/// One row of `data/anchors.tsv`: a pair verified by reading both functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub image_a: String,
    pub va_a: u32,
    pub image_b: String,
    pub va_b: u32,
    pub verified_by: String,
}

/// The anchors compiled into this crate, with every image id checked against `images`.
pub fn anchors(images: &crate::images::Images) -> Result<Vec<Anchor>, Error> {
    parse_anchors(include_str!("../data/anchors.tsv"), images)
}

pub fn parse_anchors(text: &str, images: &crate::images::Images) -> Result<Vec<Anchor>, Error> {
    const HEADER: &str = "image_a\tva_a\timage_b\tva_b\tverified_by";
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(Error::new(format!("anchors.tsv: header is not {HEADER:?}")));
    }
    let mut out = Vec::new();
    for (i, l) in lines.enumerate() {
        let f = crate::table_row("anchors.tsv", HEADER, i + 2, l)?;
        if crate::field_value(f[4]).is_none() {
            return Err(Error::new(format!("anchors.tsv line {}: verified_by holds no value", i + 2)));
        }
        for id in [f[0], f[2]] {
            if images.image(id).is_none() {
                return Err(Error::new(format!("anchors.tsv line {}: image {id} is not in images.tsv", i + 2)));
            }
        }
        out.push(Anchor {
            image_a: f[0].to_string(),
            va_a: hex32(f[1], "va_a")?,
            image_b: f[2].to_string(),
            va_b: hex32(f[3], "va_b")?,
            verified_by: f[4].to_string(),
        });
    }
    Ok(out)
}

/// How the anchors that concern images `a` and `b` fared in `outcome`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnchorCheck {
    pub agreeing: usize,
    /// Anchors neither of whose functions the outcome pairs.
    pub unmatched: usize,
    /// Anchors the outcome contradicts, with the pair it accepted instead.
    pub contradicted: Vec<(Anchor, Pair)>,
}

/// The anchor's pair as (image `a` function, image `b` function), when it is between images `a`
/// and `b` in either orientation.
fn anchor_pair(a: &str, b: &str, an: &Anchor) -> Option<(u32, u32)> {
    if an.image_a == a && an.image_b == b {
        Some((an.va_a, an.va_b))
    } else if an.image_a == b && an.image_b == a {
        Some((an.va_b, an.va_a))
    } else {
        None
    }
}

/// Every anchor between images `a` and `b`, in either orientation, as (image `a` function, image
/// `b` function): the `anchors` argument of [`match_images`].
pub fn anchor_pairs(a: &str, b: &str, anchors: &[Anchor]) -> Vec<(u32, u32)> {
    anchors.iter().filter_map(|an| anchor_pair(a, b, an)).collect()
}

/// Check `outcome` (pairs of image `a` to image `b`) against every anchor between the two images,
/// in either orientation.
pub fn check_anchors(a: &str, b: &str, outcome: &Outcome, anchors: &[Anchor]) -> AnchorCheck {
    let mut r = AnchorCheck::default();
    for an in anchors {
        let Some((fa, fb)) = anchor_pair(a, b, an) else { continue };
        let hit = outcome.pairs.iter().find(|p| p.a == fa || p.b == fb);
        match hit {
            Some(p) if p.a == fa && p.b == fb => r.agreeing += 1,
            Some(p) => r.contradicted.push((an.clone(), p.clone())),
            None => r.unmatched += 1,
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(list: &[Evidence]) -> BTreeSet<Evidence> {
        list.iter().copied().collect()
    }

    fn slot(class: &str, vt: u32, off: i64, slot: usize, f: u32) -> VtableSlot {
        VtableSlot {
            abi: crate::rtti::Abi::Msvc,
            raw_name: class.into(),
            class: class.into(),
            vtable: vt,
            subobject_offset: off,
            slot,
            function: f,
            deleting_destructor: None,
        }
    }

    #[test]
    fn vtables_pair_by_class_ordinal_and_slot_only_when_shapes_agree() {
        let a = Exports { vtables: vec![slot("C", 1, 0, 0, 10), slot("C", 1, 0, 1, 11), slot("D", 2, 0, 0, 12)], ..Default::default() };
        let b = Exports {
            vtables: vec![slot("C", 5, 0, 0, 20), slot("C", 5, 0, 1, 21), slot("D", 6, 0, 0, 22), slot("D", 6, 0, 1, 23)],
            ..Default::default()
        };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Vtable]), &[]).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(10, 20), (11, 21)], "D has 1 slot in a and 2 in b, so it is not used");
    }

    #[test]
    fn msvc_and_itanium_vtables_align_around_the_destructor() {
        use crate::rtti::Abi;
        let m = |i: usize, f: u32| VtableSlot { deleting_destructor: Some(1), ..slot("C", 1, 0, i, f) };
        let it = |i: usize, f: u32| VtableSlot { abi: Abi::Itanium, ..slot("C", 9, 0, i, f) };
        // MSVC [m0, dtor, m2] against Itanium [i0, D1, D0, i2].
        let a = Exports { vtables: vec![m(0, 10), m(1, 11), m(2, 12)], ..Default::default() };
        let b = Exports { vtables: vec![it(0, 20), it(1, 21), it(2, 22), it(3, 23)], ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Vtable]), &[]).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(10, 20), (12, 23)]);
        // The same pair the other way round.
        let o = match_images(&b, &a, false, &ev(&[Evidence::Vtable]), &[]).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(20, 10), (23, 12)]);
        // Without a recognised destructor there is no alignment.
        let a = Exports { vtables: vec![slot("C", 1, 0, 0, 10), slot("C", 1, 0, 1, 11), slot("C", 1, 0, 2, 12)], ..Default::default() };
        assert!(match_images(&a, &b, false, &ev(&[Evidence::Vtable]), &[]).unwrap().pairs.is_empty());
    }

    #[test]
    fn a_function_with_two_candidates_is_a_conflict_not_a_pair() {
        let bind = |ns: &str, n: &str, f: u32| Binding { namespace: ns.into(), name: n.into(), function: f };
        let a = Exports { bindings: vec![bind("P", "X", 1), bind("P", "Y", 1)], ..Default::default() };
        let b = Exports { bindings: vec![bind("P", "X", 2), bind("P", "Y", 3)], ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua]), &[]).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.conflicts.len(), 1);
        assert_eq!(o.conflicts[0].side, 'a');
        assert_eq!(o.conflicts[0].function, 1);
    }

    #[test]
    fn callees_propagate_from_accepted_pairs_to_a_fixpoint() {
        let sig = |callees: Vec<u32>| Signature { mhash: String::new(), callees: callees.into_iter().map(Callee::Direct).collect(), strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let sa: HashMap<u32, Signature> = [(1, sig(vec![2])), (2, sig(vec![3])), (3, sig(vec![]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![12])), (12, sig(vec![13])), (13, sig(vec![]))].into();
        let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(1, 11), (2, 12), (3, 13)]);
        // Across languages, callee positions are not evidence.
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap();
        assert_eq!(o.pairs.len(), 1);
    }

    #[test]
    fn a_call_list_that_contradicts_accepted_pairs_does_not_propagate() {
        let sig = |callees: Vec<Callee>| Signature { mhash: String::new(), callees, strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        // Root calls [X, Y]; in b the order is [Y', X'], and X<->X' is accepted by its own binding.
        let d = Callee::Direct;
        let sa: HashMap<u32, Signature> = [(1, sig(vec![d(2), d(3)])), (2, sig(vec![])), (3, sig(vec![]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![d(13), d(12)])), (12, sig(vec![])), (13, sig(vec![]))].into();
        let a = Exports { bindings: vec![bind("Root", 1), bind("X", 2)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11), bind("X", 12)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(1, 11), (2, 12)], "position 0 maps 2 to 13, contradicting the accepted 2<->12");
    }

    fn strs_sig(strs: &[&str]) -> Signature {
        Signature { mhash: String::new(), callees: vec![], strs: strs.iter().map(|s| s.to_string()).collect() }
    }

    fn pairs_of(o: &Outcome) -> Vec<(u32, u32)> {
        o.pairs.iter().map(|p| (p.a, p.b)).collect()
    }

    #[test]
    fn a_pair_whose_only_evidence_is_one_string_is_not_accepted() {
        let a = Exports { signatures: Some([(1, strs_sig(&["7374617274"]))].into()), ..Default::default() };
        let b = Exports { signatures: Some([(11, strs_sig(&["7374617274"]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[]).unwrap();
        assert!(o.pairs.is_empty());
        assert!(o.conflicts.is_empty());
        assert_eq!(o.uncorroborated, vec![Pair { a: 1, b: 11, evidence: ev(&[Evidence::String]) }]);
    }

    #[test]
    fn a_second_distinct_unique_string_corroborates_a_string_pair() {
        let a = Exports { signatures: Some([(1, strs_sig(&["aa", "bb"]))].into()), ..Default::default() };
        let b = Exports { signatures: Some([(11, strs_sig(&["aa", "bb"]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[]).unwrap();
        assert_eq!(pairs_of(&o), vec![(1, 11)]);
        assert!(o.uncorroborated.is_empty());
        // A second string that another function of `b` also references is not unique, so it does
        // not count.
        let b = Exports { signatures: Some([(11, strs_sig(&["aa", "bb"])), (12, strs_sig(&["bb"]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[]).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.uncorroborated.len(), 1);
    }

    #[test]
    fn a_string_pair_with_other_evidence_is_accepted() {
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let a = Exports { bindings: vec![bind("F", 1)], signatures: Some([(1, strs_sig(&["aa"]))].into()), ..Default::default() };
        let b = Exports { bindings: vec![bind("F", 11)], signatures: Some([(11, strs_sig(&["aa"]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua, Evidence::String]), &[]).unwrap();
        assert_eq!(o.pairs, vec![Pair { a: 1, b: 11, evidence: ev(&[Evidence::Lua, Evidence::String]) }]);
    }

    #[test]
    fn callee_evidence_corroborates_a_string_pair_at_the_fixpoint() {
        let sig = |callees: Vec<Callee>, strs: &[&str]| Signature { callees, ..strs_sig(strs) };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let sa: HashMap<u32, Signature> = [(1, sig(vec![Callee::Direct(2)], &[])), (2, sig(vec![], &["aa"]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![Callee::Direct(12)], &[])), (12, sig(vec![], &["aa"]))].into();
        let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::String, Evidence::Callee]), &[]).unwrap();
        assert_eq!(o.pairs[1], Pair { a: 2, b: 12, evidence: ev(&[Evidence::String, Evidence::Callee]) });
        assert!(o.uncorroborated.is_empty());
    }

    #[test]
    fn an_uncorroborated_string_candidate_still_conflicts() {
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let a = Exports { bindings: vec![bind("F", 1)], signatures: Some([(1, strs_sig(&["aa"]))].into()), ..Default::default() };
        let b = Exports {
            bindings: vec![bind("F", 11)],
            signatures: Some([(11, strs_sig(&[])), (12, strs_sig(&["aa"]))].into()),
            ..Default::default()
        };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua, Evidence::String]), &[]).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.conflicts.len(), 1);
        assert_eq!((o.conflicts[0].side, o.conflicts[0].function), ('a', 1));
    }

    #[test]
    fn a_callee_that_is_not_a_function_start_is_never_paired() {
        let sig = |callees: Vec<Callee>| Signature { mhash: String::new(), callees, strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let d = Callee::Direct;
        // 0x6c is an import written as an EXTERNAL-space index; 500 and 600 are in-image targets
        // that are no function starts; 2 and 12 are function starts.
        let sa: HashMap<u32, Signature> = [(1, sig(vec![d(0x6c), d(500), d(2)])), (2, sig(vec![]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![d(0x6c), d(600), d(12)])), (12, sig(vec![]))].into();
        let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap();
        assert_eq!(pairs_of(&o), vec![(1, 11), (2, 12)]);
        assert!(o.conflicts.is_empty() && o.uncorroborated.is_empty());
    }

    #[test]
    fn aligned_imports_agree_only_when_their_libraries_and_names_are_equal() {
        let sig = |callees: Vec<Callee>| Signature { mhash: String::new(), callees, strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let imp = |n: &str| Callee::Import { library: "KERNEL32.DLL".into(), name: n.into() };
        let lib = |l: &str, n: &str| Callee::Import { library: l.into(), name: n.into() };
        let run = |ia: Callee, ib: Callee| {
            let sa: HashMap<u32, Signature> = [(1, sig(vec![ia, Callee::Direct(2)])), (2, sig(vec![]))].into();
            let sb: HashMap<u32, Signature> = [(11, sig(vec![ib, Callee::Direct(12)])), (12, sig(vec![]))].into();
            let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
            let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
            pairs_of(&match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap())
        };
        assert_eq!(run(imp("Sleep"), imp("Sleep")), vec![(1, 11), (2, 12)]);
        assert_eq!(run(imp("Sleep"), imp("ExitProcess")), vec![(1, 11)], "different imports at position 0");
        assert_eq!(run(imp("Sleep"), Callee::Indirect), vec![(1, 11)], "an import against an indirect call");
        assert_eq!(run(Callee::Indirect, Callee::Indirect), vec![(1, 11), (2, 12)]);
        // The same ordinal from two libraries is two imports.
        assert_eq!(run(lib("xboxkrnl.exe", "ord_293"), lib("xboxkrnl.exe", "ord_293")), vec![(1, 11), (2, 12)]);
        assert_eq!(run(lib("xboxkrnl.exe", "ord_293"), lib("xam.xex", "ord_293")), vec![(1, 11)], "same name, other library");
        assert_eq!(run(lib("xboxkrnl.exe", "ord_293"), lib("XBOXKRNL.EXE", "ord_293")), vec![(1, 11)], "library text is exact");
    }

    #[test]
    fn stack_and_slot_calls_align_as_indirect_calls_whatever_their_operand() {
        let sig = |callees: Vec<Callee>| Signature { mhash: String::new(), callees, strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let run = |ea: &str, eb: &str| {
            let (ia, ib) = (Callee::parse(ea).unwrap(), Callee::parse(eb).unwrap());
            let sa: HashMap<u32, Signature> = [(1, sig(vec![ia, Callee::Direct(2)])), (2, sig(vec![]))].into();
            let sb: HashMap<u32, Signature> = [(11, sig(vec![ib, Callee::Direct(12)])), (12, sig(vec![]))].into();
            let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
            let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
            pairs_of(&match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee]), &[]).unwrap())
        };
        let both = vec![(1, 11), (2, 12)];
        assert_eq!(run("*stack:-0x68", "*stack:-0x68"), both);
        assert_eq!(run("*stack:-0x68", "*stack:0x10"), both, "stack offsets are not compared");
        assert_eq!(run("*stack:-0x68", "*0x00a0c010"), both);
        assert_eq!(run("*stack:-0x68", "?"), both);
        assert_eq!(run("*0x00a0c010", "*0x00b00000"), both, "slot addresses are not compared");
        assert_eq!(run("*stack:-0x68", "ext:KERNEL32.DLL!Sleep"), vec![(1, 11)], "an indirect call against an import");
        assert_eq!(run("*stack:-0x68", "0x00000002"), vec![(1, 11)], "an indirect call against a direct one");
    }

    #[test]
    fn callee_entries_parse_by_form() {
        assert_eq!(Callee::parse("0x00401fb0").unwrap(), Callee::Direct(0x00401fb0));
        assert_eq!(
            Callee::parse("ext:KERNEL32.DLL!Sleep").unwrap(),
            Callee::Import { library: "KERNEL32.DLL".into(), name: "Sleep".into() }
        );
        assert_eq!(Callee::parse("*0x00000008").unwrap(), Callee::Indirect);
        assert_eq!(Callee::parse("*stack:-0x68").unwrap(), Callee::Indirect);
        assert_eq!(Callee::parse("*stack:0x10").unwrap(), Callee::Indirect);
        assert_eq!(Callee::parse("?").unwrap(), Callee::Indirect);
        assert_eq!(
            Callee::parse("ext:xboxkrnl.exe!ord_293").unwrap(),
            Callee::Import { library: "xboxkrnl.exe".into(), name: "ord_293".into() }
        );
        assert!(Callee::parse("ext:Sleep").is_err());
        assert!(Callee::parse("ext:KERNEL32.DLL!").is_err());
        assert!(Callee::parse("0xzz").is_err());
        // A memory slot is 32 bits; a stack reference is written `*stack:`.
        assert!(Callee::parse("*0xffffffffffffff98").is_err());
        // Any other form is an error, not an indirect call.
        for bad in [
            "", "??", "*", "*0xzz", "*8", "401fb0", "Sleep", "ext", "*stack:", "*stack:-68", "*stack:+0x10", "*stack:--0x68",
            "*stack:-0x", "*stack:0x1ffffffff", "*stack:esp", "*stack-0x68",
        ] {
            assert!(Callee::parse(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn a_fnsig_export_is_read_strictly() {
        let ok = r#"{"0x00401000": {"mhash": "h", "callees": ["0x00401fb0", "*0x00000008", "?", "ext:KERNEL32.DLL!Sleep"], "strs": ["6162"]}}"#;
        let m = parse_signatures(ok, "x.fnsig.json").unwrap();
        let s = &m[&0x00401000];
        assert_eq!(s.callees.len(), 4);
        assert_eq!(s.strs, vec!["6162".to_string()]);
        let err = |text: &str, want: &str| {
            let e = parse_signatures(text, "x.fnsig.json").unwrap_err().to_string();
            assert!(e.contains("x.fnsig.json") && e.contains("0x00401000") && e.contains(want), "{e}");
        };
        err(r#"{"0x00401000": {"mhash": "h", "callees": ["call_me"], "strs": []}}"#, "call_me");
        err(r#"{"0x00401000": {"mhash": "h", "callees": [7], "strs": []}}"#, "\"callees\" entry 7 is not a string");
        err(r#"{"0x00401000": {"mhash": "h", "callees": [], "strs": [null]}}"#, "\"strs\" entry null is not a string");
        err(r#"{"0x00401000": {"mhash": "h", "strs": []}}"#, "no \"callees\" key");
        err(r#"{"0x00401000": {"mhash": "h", "callees": []}}"#, "no \"strs\" key");
        err(r#"{"0x00401000": {"mhash": "h", "callees": "0x1", "strs": []}}"#, "\"callees\" is not an array");
        err(r#"{"0x00401000": {"mhash": "h", "callees": [], "strs": {}}}"#, "\"strs\" is not an array");
        err(r#"{"0x00401000": {"callees": [], "strs": []}}"#, "no \"mhash\" key");
        err(r#"{"0x00401000": {"mhash": 5, "callees": [], "strs": []}}"#, "\"mhash\" is not a string");
    }

    #[test]
    fn an_anchor_corroborates_a_string_candidate() {
        let a = Exports { signatures: Some([(1, strs_sig(&["aa"]))].into()), ..Default::default() };
        let b = Exports { signatures: Some([(11, strs_sig(&["aa"]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[(1, 11)]).unwrap();
        assert_eq!(o.pairs, vec![Pair { a: 1, b: 11, evidence: ev(&[Evidence::String, Evidence::Anchor]) }]);
        assert!(o.uncorroborated.is_empty() && o.conflicts.is_empty());
        // An anchor for another pair of the same functions adds nothing.
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[(1, 12)]).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.uncorroborated, vec![Pair { a: 1, b: 11, evidence: ev(&[Evidence::String]) }]);
    }

    #[test]
    fn an_anchor_creates_no_candidate_and_overrides_no_conflict() {
        // No rule proposes (1, 11), so the anchor makes nothing.
        let a = Exports { signatures: Some([(1, strs_sig(&[]))].into()), ..Default::default() };
        let b = Exports { signatures: Some([(11, strs_sig(&[]))].into()), ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::String]), &[(1, 11)]).unwrap();
        assert!(o.pairs.is_empty() && o.uncorroborated.is_empty() && o.conflicts.is_empty());
        // (1, 11) and (1, 12) are both candidates; anchoring one leaves the conflict.
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let a = Exports { bindings: vec![bind("F", 1)], signatures: Some([(1, strs_sig(&["aa"]))].into()), ..Default::default() };
        let b = Exports {
            bindings: vec![bind("F", 11)],
            signatures: Some([(11, strs_sig(&[])), (12, strs_sig(&["aa"]))].into()),
            ..Default::default()
        };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua, Evidence::String]), &[(1, 12)]).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.conflicts.len(), 1);
        assert_eq!(
            o.conflicts[0].candidates,
            vec![(11, ev(&[Evidence::Lua])), (12, ev(&[Evidence::String, Evidence::Anchor]))]
        );
    }

    #[test]
    fn anchor_evidence_is_not_chosen_in_the_evidence_list() {
        let e = match_images(&Exports::default(), &Exports::default(), true, &ev(&[Evidence::Anchor]), &[]).unwrap_err();
        assert!(e.to_string().contains("anchors.tsv"), "{e}");
        assert_eq!(Evidence::parse("anchor"), Some(Evidence::Anchor));
        assert_eq!(Evidence::Anchor.as_str(), "anchor");
    }

    #[test]
    fn anchor_pairs_orient_to_the_images_asked_for() {
        let an = |ia: &str, a: u32, ib: &str, b: u32| Anchor { image_a: ia.into(), va_a: a, image_b: ib.into(), va_b: b, verified_by: "read".into() };
        let all = [an("x", 1, "y", 11), an("y", 12, "x", 2), an("x", 3, "z", 13)];
        assert_eq!(anchor_pairs("x", "y", &all), vec![(1, 11), (2, 12)]);
        assert_eq!(anchor_pairs("y", "x", &all), vec![(11, 1), (12, 2)]);
    }

    #[test]
    fn anchors_report_agreement_absence_and_contradiction() {
        let an = |a: u32, b: u32| Anchor { image_a: "x".into(), va_a: a, image_b: "y".into(), va_b: b, verified_by: "read".into() };
        let pair = |a: u32, b: u32| Pair { a, b, evidence: BTreeSet::new() };
        let o = Outcome { pairs: vec![pair(1, 11), pair(2, 99)], ..Default::default() };
        let r = check_anchors("x", "y", &o, &[an(1, 11), an(2, 12), an(3, 13)]);
        assert_eq!((r.agreeing, r.unmatched, r.contradicted.len()), (1, 1, 1));
        // The same anchors read in the other orientation.
        let o = Outcome { pairs: vec![pair(11, 1)], ..Default::default() };
        assert_eq!(check_anchors("y", "x", &o, &[an(1, 11)]).agreeing, 1);
    }

    #[test]
    fn the_embedded_anchors_name_known_images() {
        let c = crate::Catalogue::embedded().unwrap();
        let images = crate::images::Images::embedded(&c).unwrap();
        assert!(!anchors(&images).unwrap().is_empty());
    }

    #[test]
    fn signature_evidence_without_signatures_is_an_error() {
        let e = match_images(&Exports::default(), &Exports::default(), true, &ev(&[Evidence::String]), &[]).unwrap_err();
        assert!(e.to_string().contains("fnsig"), "{e}");
    }
}
