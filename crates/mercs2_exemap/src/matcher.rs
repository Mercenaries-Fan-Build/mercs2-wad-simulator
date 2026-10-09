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
//! | `string` | the same referenced string, referenced by exactly one function in each image |
//! | `callee` | the direct callees at the same position of an accepted pair whose call lists (direct and indirect, in order) have the same length, and where every position whose callees are both already accepted agrees with those pairs (same Ghidra language only: across compilers, inlining shifts call lists, so equal length is no alignment) |
//!
//! A pair is **accepted** when its `a` has no other candidate and its `b` has no other candidate.
//! Every other candidate is a **conflict** and is reported with all its candidates; nothing is
//! chosen among them. `callee` candidates come from accepted pairs, and the rules run to a
//! fixpoint: the candidate set only grows, so acceptance is recomputed until it stops changing.
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
}

impl Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Evidence::Vtable => "vtable",
            Evidence::Lua => "lua",
            Evidence::Bytes => "bytes",
            Evidence::String => "string",
            Evidence::Callee => "callee",
        }
    }
    pub fn parse(s: &str) -> Option<Evidence> {
        match s {
            "vtable" => Some(Evidence::Vtable),
            "lua" => Some(Evidence::Lua),
            "bytes" => Some(Evidence::Bytes),
            "string" => Some(Evidence::String),
            "callee" => Some(Evidence::Callee),
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

/// The parts of one `FnMnemonicSig` record the rules read.
#[derive(Clone, Debug, Default)]
pub struct Signature {
    pub mhash: String,
    /// Callees in call order: the target of a direct call, `None` for an indirect one.
    pub callees: Vec<Option<u32>>,
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
            let v: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
            let obj = v.as_object().ok_or_else(|| Error::new(format!("{}: not a JSON object", path.display())))?;
            let mut m = HashMap::with_capacity(obj.len());
            for (k, rec) in obj {
                let addr = hex32(k, "function")?;
                let strs = |key: &str| -> Vec<String> {
                    rec.get(key)
                        .and_then(|x| x.as_array())
                        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                        .unwrap_or_default()
                };
                let callees = strs("callees").into_iter().map(|c| if c.starts_with("0x") { hex32(&c, "callee").ok() } else { None }).collect();
                m.insert(
                    addr,
                    Signature {
                        mhash: rec.get("mhash").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                        callees,
                        strs: strs("strs"),
                    },
                );
            }
            Some(m)
        } else {
            None
        };
        Ok(Exports { vtables, bindings, signatures })
    }
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
}

type Candidates = BTreeMap<(u32, u32), BTreeSet<Evidence>>;

fn add(c: &mut Candidates, a: u32, b: u32, e: Evidence) -> bool {
    c.entry((a, b)).or_default().insert(e)
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

fn signature_candidates(sa: &HashMap<u32, Signature>, sb: &HashMap<u32, Signature>, same_isa: bool, c: &mut Candidates) {
    if same_isa {
        let (ua, ub) = (unique_by(sa, |s| vec![s.mhash.as_str()]), unique_by(sb, |s| vec![s.mhash.as_str()]));
        for (h, fa) in &ua {
            if let Some(fb) = ub.get(h) {
                add(c, *fa, *fb, Evidence::Bytes);
            }
        }
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
    for (k, fa) in &ua {
        if let Some(fb) = ub.get(k) {
            add(c, *fa, *fb, Evidence::String);
        }
    }
}

fn resolve(c: &Candidates) -> Outcome {
    let mut by_a: BTreeMap<u32, Vec<(u32, BTreeSet<Evidence>)>> = BTreeMap::new();
    let mut by_b: BTreeMap<u32, Vec<(u32, BTreeSet<Evidence>)>> = BTreeMap::new();
    for ((a, b), e) in c {
        by_a.entry(*a).or_default().push((*b, e.clone()));
        by_b.entry(*b).or_default().push((*a, e.clone()));
    }
    let mut out = Outcome::default();
    for ((a, b), e) in c {
        if by_a[a].len() == 1 && by_b[b].len() == 1 {
            out.pairs.push(Pair { a: *a, b: *b, evidence: e.clone() });
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

/// Match the functions of image `a` against image `b` with the chosen evidence.
/// `same_language` says whether both images are analysed with the same Ghidra language, i.e. built
/// by the same compiler for the same processor; `bytes` and `callee` evidence apply only then.
pub fn match_images(a: &Exports, b: &Exports, same_language: bool, evidence: &BTreeSet<Evidence>) -> Result<Outcome, Error> {
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
        if evidence.contains(&Evidence::Bytes) || evidence.contains(&Evidence::String) {
            let mut sc = Candidates::new();
            signature_candidates(sa, sb, same_language, &mut sc);
            for ((x, y), es) in sc {
                for e in es.into_iter().filter(|e| evidence.contains(e)) {
                    add(&mut c, x, y, e);
                }
            }
        }
    }
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
                    (Some(ca), Some(cb)) => {
                        fwd.get(ca).is_none_or(|v| v == cb) && back.get(cb).is_none_or(|v| v == ca)
                    }
                    (None, None) => true,
                    _ => false,
                });
                if !consistent {
                    continue;
                }
                for (ca, cb) in x.callees.iter().zip(&y.callees) {
                    if let (Some(ca), Some(cb)) = (ca, cb) {
                        grew |= add(&mut c, *ca, *cb, Evidence::Callee);
                    }
                }
            }
            if !grew {
                break;
            }
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
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != 5 || f[4].is_empty() {
            return Err(Error::new(format!("anchors.tsv line {}: expected 5 fields with verified_by set", i + 2)));
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

/// Check `outcome` (pairs of image `a` to image `b`) against every anchor between the two images,
/// in either orientation.
pub fn check_anchors(a: &str, b: &str, outcome: &Outcome, anchors: &[Anchor]) -> AnchorCheck {
    let mut r = AnchorCheck::default();
    for an in anchors {
        let (fa, fb) = if an.image_a == a && an.image_b == b {
            (an.va_a, an.va_b)
        } else if an.image_a == b && an.image_b == a {
            (an.va_b, an.va_a)
        } else {
            continue;
        };
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
        let o = match_images(&a, &b, false, &ev(&[Evidence::Vtable])).unwrap();
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
        let o = match_images(&a, &b, false, &ev(&[Evidence::Vtable])).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(10, 20), (12, 23)]);
        // The same pair the other way round.
        let o = match_images(&b, &a, false, &ev(&[Evidence::Vtable])).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(20, 10), (23, 12)]);
        // Without a recognised destructor there is no alignment.
        let a = Exports { vtables: vec![slot("C", 1, 0, 0, 10), slot("C", 1, 0, 1, 11), slot("C", 1, 0, 2, 12)], ..Default::default() };
        assert!(match_images(&a, &b, false, &ev(&[Evidence::Vtable])).unwrap().pairs.is_empty());
    }

    #[test]
    fn a_function_with_two_candidates_is_a_conflict_not_a_pair() {
        let bind = |ns: &str, n: &str, f: u32| Binding { namespace: ns.into(), name: n.into(), function: f };
        let a = Exports { bindings: vec![bind("P", "X", 1), bind("P", "Y", 1)], ..Default::default() };
        let b = Exports { bindings: vec![bind("P", "X", 2), bind("P", "Y", 3)], ..Default::default() };
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua])).unwrap();
        assert!(o.pairs.is_empty());
        assert_eq!(o.conflicts.len(), 1);
        assert_eq!(o.conflicts[0].side, 'a');
        assert_eq!(o.conflicts[0].function, 1);
    }

    #[test]
    fn callees_propagate_from_accepted_pairs_to_a_fixpoint() {
        let sig = |callees: Vec<u32>| Signature { mhash: String::new(), callees: callees.into_iter().map(Some).collect(), strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        let sa: HashMap<u32, Signature> = [(1, sig(vec![2])), (2, sig(vec![3])), (3, sig(vec![]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![12])), (12, sig(vec![13])), (13, sig(vec![]))].into();
        let a = Exports { bindings: vec![bind("Root", 1)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee])).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(1, 11), (2, 12), (3, 13)]);
        // Across languages, callee positions are not evidence.
        let o = match_images(&a, &b, false, &ev(&[Evidence::Lua, Evidence::Callee])).unwrap();
        assert_eq!(o.pairs.len(), 1);
    }

    #[test]
    fn a_call_list_that_contradicts_accepted_pairs_does_not_propagate() {
        let sig = |callees: Vec<Option<u32>>| Signature { mhash: String::new(), callees, strs: vec![] };
        let bind = |n: &str, f: u32| Binding { namespace: String::new(), name: n.into(), function: f };
        // Root calls [X, Y]; in b the order is [Y', X'], and X<->X' is accepted by its own binding.
        let sa: HashMap<u32, Signature> = [(1, sig(vec![Some(2), Some(3)])), (2, sig(vec![])), (3, sig(vec![]))].into();
        let sb: HashMap<u32, Signature> = [(11, sig(vec![Some(13), Some(12)])), (12, sig(vec![])), (13, sig(vec![]))].into();
        let a = Exports { bindings: vec![bind("Root", 1), bind("X", 2)], signatures: Some(sa), ..Default::default() };
        let b = Exports { bindings: vec![bind("Root", 11), bind("X", 12)], signatures: Some(sb), ..Default::default() };
        let o = match_images(&a, &b, true, &ev(&[Evidence::Lua, Evidence::Callee])).unwrap();
        let pairs: Vec<(u32, u32)> = o.pairs.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(pairs, vec![(1, 11), (2, 12)], "position 0 maps 2 to 13, contradicting the accepted 2<->12");
    }

    #[test]
    fn anchors_report_agreement_absence_and_contradiction() {
        let an = |a: u32, b: u32| Anchor { image_a: "x".into(), va_a: a, image_b: "y".into(), va_b: b, verified_by: "read".into() };
        let pair = |a: u32, b: u32| Pair { a, b, evidence: BTreeSet::new() };
        let o = Outcome { pairs: vec![pair(1, 11), pair(2, 99)], conflicts: vec![] };
        let r = check_anchors("x", "y", &o, &[an(1, 11), an(2, 12), an(3, 13)]);
        assert_eq!((r.agreeing, r.unmatched, r.contradicted.len()), (1, 1, 1));
        // The same anchors read in the other orientation.
        let o = Outcome { pairs: vec![pair(11, 1)], conflicts: vec![] };
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
        let e = match_images(&Exports::default(), &Exports::default(), true, &ev(&[Evidence::String])).unwrap_err();
        assert!(e.to_string().contains("fnsig"), "{e}");
    }
}
