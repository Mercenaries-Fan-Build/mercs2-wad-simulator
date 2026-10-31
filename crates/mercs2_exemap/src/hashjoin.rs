//! The cross-image reverse map of hash constants: which code consumes each one, which unknown
//! constants are hashes, and the names that the code around them proves.
//!
//! **Input.** One [`crate::hashsites`] output per image (`<image_id>.hashes.jsonl`) and that image's
//! [`crate::annotate`] output (`<image_id>.annotate.jsonl`) for its translations to the other images.
//! A hashes file must open with its image record and end with its end record; an annotate file
//! must open with an image record of the same image and end with its end record. A value is
//! **known** when its record lists pool names.
//!
//! **Canonical functions.** The agreed translations of every annotate file given are edges; a
//! function is labelled by the first member, in `images.tsv` order, of its connected component:
//! `<image_id>:0x%08x`. So every copy of a function the match graph links shares one label; copies
//! it does not link keep their own. A callee is found by its Ghidra name in the image's annotate
//! names.
//!
//! **Consumer groups.** Each site joins one group, keyed by role and canonical label:
//!
//! | role | group |
//! |---|---|
//! | `arg` (whole argument only) | `arg<index>` of the canonical callee |
//! | `cmp` | the canonical function and the operand's signature |
//! | `case` | the canonical function |
//! | `store` | the canonical function and the lvalue's signature |
//! | `return` | the canonical function |
//!
//! An operand's signature is its `param_<n>` names and numbers, in order (`param_1 0x24` for
//! `*(uint *)(param_1 + 0x24)`), taken from `lhs_def` when there is one; it is the same on every
//! platform, where type and local names are not. `bound`, `assign` and `other` sites join no group,
//! nor does an argument whose callee names no function (a Ghidra pseudo-operation like `CONCAT44`).
//!
//! **Hash consumers.** A group of `n` distinct values, `k` of them known, is a hash consumer when
//! `k >= 2` and `k` known values are improbable by chance: the Poisson tail `P(X >= k)` with
//! `λ = n · pool / 2³²` (the pool size from the hashes files' image records) is below
//! [`CONSUMER_P`]. A generic consumer of constants (a float or mask argument) holds pool hashes
//! only at that chance rate and is not one.
//!
//! **Hash candidates.** An unknown value is a candidate when (G) it is in a hash consumer, (T) it
//! is an unknown slot of a table, or (I) the same value is in the translated
//! function on another *platform* (not image: the PC images share addresses) at a grouped role,
//! and it is not [`crate::hashsites::structured`] (a mask, a float, a FourCC). Structure is checked
//! here only for (I), which has no hash-consumer evidence of its own; table slots were checked when
//! the tables were found.
//!
//! **Cracks.** For every value without pool names, each site's function's context is turned into
//! names and hashed: the function's own strings, then its translations' strings, then its
//! callers', then its callees' (a table slot: the functions that reference the table). A string
//! yields itself, each `[A-Za-z0-9_]` token of two or more characters, each token's prefixes up to
//! an `_` and its `_`-separated pieces, and each token's camel-case pieces joined by `_` and alone
//! (pieces and prefixes of two or more characters). For each group with an unknown value, every prefix up to an `_`
//! that two or more of its known names share is joined with every token of its known names and of
//! its sites' strings. A crack is a name whose `pandemic_hash_m2` is the value; the first source
//! that yields one is reported. Known values are checked the same way and reported with
//! `in_pool` = yes, which corroborates a pool name from the code that uses it.
//!
//! **Output.** `candidates` (TSV, one row per group or table that holds an uncracked candidate, and
//! one per canonical function for invariant-only candidates) and `cracks` (TSV, one row per cracked
//! value and name), both sorted.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Deserialize;

use mercs2_formats::hash::pandemic_hash_m2_bytes;

use crate::hashsites::{structured, Role};
use crate::images::{Images, Platform};
use crate::xmap::parse_address;
use crate::Error;

/// One function's context in a hashes file.
#[derive(Clone, Debug, Default)]
pub struct FnContext {
    pub name: String,
    pub strings: Vec<String>,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
}

/// One table of a hashes file.
#[derive(Clone, Debug)]
pub struct TableRec {
    pub va: u32,
    pub stride: u32,
    /// (slot va, value, pool names)
    pub slots: Vec<(u32, u32, Vec<String>)>,
    pub referenced_by: Vec<u32>,
}

/// One site of a value: the function and its role.
#[derive(Clone, Debug)]
pub struct SiteRec {
    pub function: u32,
    pub role: Role,
}

/// One image's hashes file.
#[derive(Clone, Debug)]
pub struct ImageHashes {
    pub image_id: String,
    /// Distinct hashes in the name pool the file was scanned with.
    pub pool_hashes: u64,
    /// value → (pool names, sites)
    pub values: BTreeMap<u32, (Vec<String>, Vec<SiteRec>)>,
    pub tables: Vec<TableRec>,
    pub functions: HashMap<u32, FnContext>,
}

#[derive(Deserialize)]
struct HashesLine {
    kind: String,
    image_id: Option<String>,
    pool_hashes: Option<u64>,
    value: Option<String>,
    names: Option<Vec<String>>,
    sites: Option<Vec<SiteLine>>,
    va: Option<String>,
    stride: Option<u32>,
    slots: Option<Vec<SlotLine>>,
    referenced_by: Option<Vec<String>>,
    name: Option<String>,
    strings: Option<Vec<String>>,
    callers: Option<Vec<String>>,
    callees: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SiteLine {
    function: String,
    #[serde(flatten)]
    role: Role,
}

#[derive(Deserialize)]
struct SlotLine {
    va: String,
    value: String,
    names: Vec<String>,
}

fn need<T>(v: Option<T>, what: &str, n: usize, field: &str) -> Result<T, Error> {
    v.ok_or_else(|| Error::new(format!("{what} line {n}: no {field:?}")))
}

/// Read one hashes file (`what` names it in errors).
pub fn read_hashes(text: &str, what: &str) -> Result<ImageHashes, Error> {
    let mut image_id = None;
    let mut ended = false;
    let mut out =
        ImageHashes { image_id: String::new(), pool_hashes: 0, values: BTreeMap::new(), tables: Vec::new(), functions: HashMap::new() };
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if ended {
            return Err(Error::new(format!("{what} line {n}: a record after the end record")));
        }
        let r: HashesLine = serde_json::from_str(line).map_err(|e| Error::new(format!("{what} line {n}: {e}")))?;
        match (r.kind.as_str(), n) {
            ("image", 1) => {
                image_id = Some(need(r.image_id, what, n, "image_id")?);
                out.pool_hashes = need(r.pool_hashes, what, n, "pool_hashes")?;
            }
            (_, 1) => return Err(Error::new(format!("{what} line 1: a {:?} record, not the image record", r.kind))),
            ("value", _) => {
                let v = parse_address(&need(r.value, what, n, "value")?)?;
                let sites = need(r.sites, what, n, "sites")?
                    .into_iter()
                    .map(|s| Ok(SiteRec { function: parse_address(&s.function)?, role: s.role }))
                    .collect::<Result<Vec<_>, Error>>()?;
                out.values.insert(v, (need(r.names, what, n, "names")?, sites));
            }
            ("table", _) => {
                let slots = need(r.slots, what, n, "slots")?
                    .into_iter()
                    .map(|s| Ok((parse_address(&s.va)?, parse_address(&s.value)?, s.names)))
                    .collect::<Result<Vec<_>, Error>>()?;
                let referenced_by =
                    need(r.referenced_by, what, n, "referenced_by")?.iter().map(|s| parse_address(s)).collect::<Result<Vec<_>, Error>>()?;
                out.tables.push(TableRec {
                    va: parse_address(&need(r.va, what, n, "va")?)?,
                    stride: need(r.stride, what, n, "stride")?,
                    slots,
                    referenced_by,
                });
            }
            ("function", _) => {
                let va = parse_address(&need(r.va, what, n, "va")?)?;
                out.functions.insert(
                    va,
                    FnContext {
                        name: need(r.name, what, n, "name")?,
                        strings: need(r.strings, what, n, "strings")?,
                        callers: need(r.callers, what, n, "callers")?,
                        callees: need(r.callees, what, n, "callees")?,
                    },
                );
            }
            ("end", _) => ended = true,
            (k, _) => return Err(Error::new(format!("{what} line {n}: unknown record kind {k:?}"))),
        }
    }
    if !ended {
        return Err(Error::new(format!("{what}: no end record; the file is truncated")));
    }
    out.image_id = image_id.ok_or_else(|| Error::new(format!("{what}: empty")))?;
    Ok(out)
}

/// One image's annotations: translations by function, and function addresses by Ghidra name.
#[derive(Clone, Debug, Default)]
pub struct Annotations {
    pub translations: HashMap<u32, Vec<(String, u32)>>,
    pub by_name: HashMap<String, u32>,
}

#[derive(Deserialize)]
struct AnnotateLine {
    kind: Option<String>,
    image_id: Option<String>,
    va: Option<String>,
    names: Option<Vec<AnnotateName>>,
    translations: Option<Vec<AnnotateTranslation>>,
}

#[derive(Deserialize)]
struct AnnotateName {
    name: String,
    source: String,
}

#[derive(Deserialize)]
struct AnnotateTranslation {
    image: String,
    va: String,
}

/// Read one annotate file of image `image_id` (`what` names it in errors).
pub fn read_annotations(text: &str, what: &str, image_id: &str) -> Result<Annotations, Error> {
    let mut out = Annotations::default();
    let mut ended = false;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if ended {
            return Err(Error::new(format!("{what} line {n}: a record after the end record")));
        }
        let r: AnnotateLine = serde_json::from_str(line).map_err(|e| Error::new(format!("{what} line {n}: {e}")))?;
        match (r.kind.as_deref(), n) {
            (Some("image"), 1) => {
                let id = need(r.image_id, what, n, "image_id")?;
                if id != image_id {
                    return Err(Error::new(format!("{what}: the annotations of image {id}, not {image_id}")));
                }
            }
            (_, 1) => return Err(Error::new(format!("{what} line 1: not the image record"))),
            (Some("end"), _) => ended = true,
            (None, _) => {
                let va = parse_address(&need(r.va, what, n, "va")?)?;
                for nm in need(r.names, what, n, "names")? {
                    if nm.source.starts_with("ghidra:") {
                        out.by_name.insert(nm.name, va);
                    }
                }
                let t = need(r.translations, what, n, "translations")?
                    .into_iter()
                    .map(|t| Ok((t.image, parse_address(&t.va)?)))
                    .collect::<Result<Vec<_>, Error>>()?;
                if !t.is_empty() {
                    out.translations.insert(va, t);
                }
            }
            (Some(k), _) => return Err(Error::new(format!("{what} line {n}: unknown record kind {k:?}"))),
        }
    }
    if !ended {
        return Err(Error::new(format!("{what}: no end record; the file is truncated")));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Names from text

/// Split `s` at lower→upper and letter↔digit-run boundaries kept as camel pieces.
fn camel_pieces(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..b.len() {
        if b[i - 1].is_ascii_lowercase() && b[i].is_ascii_uppercase() {
            out.push(s[start..i].to_string());
            start = i;
        }
    }
    out.push(s[start..].to_string());
    out
}

/// Every name one string yields (see the module documentation).
pub fn names_from_string(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.insert(s.to_string());
    for tok in s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if tok.len() < 2 {
            continue;
        }
        out.insert(tok.to_string());
        for (i, c) in tok.char_indices() {
            if c == '_' && i >= 2 {
                out.insert(tok[..i].to_string());
            }
        }
        out.extend(tok.split('_').filter(|p| p.len() >= 2).map(str::to_string));
        let pieces = camel_pieces(tok);
        if pieces.len() > 1 {
            out.insert(pieces.join("_"));
            for p in pieces {
                if p.len() >= 2 {
                    out.insert(p);
                }
            }
        }
    }
    out
}

fn hash_names(names: &BTreeSet<String>) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for n in names {
        out.entry(pandemic_hash_m2_bytes(n.as_bytes())).or_insert_with(|| n.clone());
    }
    out
}

/// The chance-tail bound below which a group is a hash consumer.
pub const CONSUMER_P: f64 = 1e-4;

/// `P(X >= k)` for `X ~ Poisson(λ)`, summed from `k` upward (accurate for tiny tails).
pub fn poisson_tail(lambda: f64, k: u64) -> f64 {
    if k == 0 {
        return 1.0;
    }
    // log of the k-th term, then the terms after it by ratio
    let mut log_term = -lambda + k as f64 * lambda.ln();
    for i in 1..=k {
        log_term -= (i as f64).ln();
    }
    let mut term = log_term.exp();
    let mut sum = 0.0;
    let mut i = k;
    while term > sum * 1e-17 && i < k + 100_000 {
        sum += term;
        i += 1;
        term *= lambda / i as f64;
    }
    sum.min(1.0)
}

/// An operand's signature: its `param_<n>` names and numbers, in order.
fn signature(text: &str) -> String {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| t.starts_with("param_") || t.starts_with("0x") || (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------------
// The join

#[derive(Default)]
struct Group {
    known: BTreeMap<u32, Vec<String>>,
    unknown: BTreeSet<u32>,
    sites: BTreeSet<String>,
    images: BTreeSet<String>,
    site_fns: BTreeSet<(String, u32)>,
}

/// One cracked value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Crack {
    pub value: u32,
    pub name: String,
    pub in_pool: bool,
    pub source: String,
    pub image: String,
    pub function: String,
    pub string: String,
}

/// The join's outputs and counts.
pub struct Join {
    pub candidates_tsv: String,
    pub cracks_tsv: String,
    pub cracks: Vec<Crack>,
    /// Groups with two or more known values, each tested as a hash consumer.
    pub groups_tested: usize,
    /// Hash consumers among them.
    pub consumers: usize,
    /// The sum of the consumers' chance tails: the expected number of them that are chance.
    pub consumer_tail_sum: f64,
    /// Consumers holding an unknown value.
    pub groups: usize,
    pub candidates_g: usize,
    pub candidates_t: usize,
    pub candidates_i: usize,
    pub uncracked: usize,
    /// (value, name) comparisons made while cracking, for the expected chance-match count.
    pub comparisons: u64,
}

struct Ctx<'a> {
    order: HashMap<&'a str, usize>,
    platform: HashMap<&'a str, Platform>,
    hashes: HashMap<&'a str, &'a ImageHashes>,
    ann: &'a HashMap<String, Annotations>,
    /// The canonical label of every function some translation names.
    labels: HashMap<(String, u32), String>,
}

/// The canonical label of every function of every translation edge: the connected components of
/// all the annotations' translations, each labelled by its first member in `images.tsv` order
/// (then by address).
fn component_labels(ann: &HashMap<String, Annotations>, order: &HashMap<&str, usize>) -> HashMap<(String, u32), String> {
    let mut index: HashMap<(String, u32), usize> = HashMap::new();
    let mut nodes: Vec<(String, u32)> = Vec::new();
    let mut parent: Vec<usize> = Vec::new();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut id = |n: (String, u32), nodes: &mut Vec<(String, u32)>, parent: &mut Vec<usize>| -> usize {
        *index.entry(n.clone()).or_insert_with(|| {
            nodes.push(n);
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    for (image, a) in ann {
        for (va, ts) in &a.translations {
            let x = id((image.clone(), *va), &mut nodes, &mut parent);
            for (ti, tva) in ts {
                let y = id((ti.clone(), *tva), &mut nodes, &mut parent);
                let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
                if rx != ry {
                    parent[rx] = ry;
                }
            }
        }
    }
    let rank = |n: &(String, u32)| (order.get(n.0.as_str()).copied().unwrap_or(usize::MAX), n.0.clone(), n.1);
    let mut best: HashMap<usize, usize> = HashMap::new();
    for i in 0..nodes.len() {
        let r = find(&mut parent, i);
        let b = best.entry(r).or_insert(i);
        if rank(&nodes[i]) < rank(&nodes[*b]) {
            *b = i;
        }
    }
    let mut out = HashMap::new();
    for i in 0..nodes.len() {
        let r = find(&mut parent, i);
        let (img, va) = &nodes[best[&r]];
        out.insert(nodes[i].clone(), format!("{img}:0x{va:08x}"));
    }
    out
}

impl Ctx<'_> {
    fn canon(&self, image: &str, va: u32) -> String {
        match self.labels.get(&(image.to_string(), va)) {
            Some(l) => l.clone(),
            None => format!("{image}:0x{va:08x}"),
        }
    }

    /// The canonical label of a callee, when its name is a function's (Ghidra's pseudo-operations,
    /// such as `CONCAT44`, and indirect calls name none).
    fn callee_label(&self, image: &str, callee: &str) -> Option<String> {
        self.ann.get(image).and_then(|a| a.by_name.get(callee)).map(|va| self.canon(image, *va))
    }

    fn group_key(&self, image: &str, function: u32, role: &Role) -> Option<String> {
        Some(match role {
            Role::Arg { callee, index, whole: true } => format!("arg{index} {}", self.callee_label(image, callee)?),
            Role::Cmp { lhs, lhs_def, .. } => {
                format!("cmp {} [{}]", self.canon(image, function), signature(lhs_def.as_deref().unwrap_or(lhs)))
            }
            Role::Case => format!("case {}", self.canon(image, function)),
            Role::Store { lhs } => format!("store {} [{}]", self.canon(image, function), signature(lhs)),
            Role::Return => format!("return {}", self.canon(image, function)),
            _ => return None,
        })
    }

    fn translations(&self, image: &str, va: u32) -> &[(String, u32)] {
        self.ann.get(image).and_then(|a| a.translations.get(&va)).map(Vec::as_slice).unwrap_or(&[])
    }
}

fn hex(v: u32) -> String {
    format!("0x{v:08x}")
}

fn first_names(names: &BTreeMap<u32, Vec<String>>, max: usize) -> String {
    let mut v: Vec<&str> = names.values().filter_map(|n| n.first().map(String::as_str)).collect();
    v.sort_unstable();
    v.dedup();
    let more = v.len().saturating_sub(max);
    let mut s = v.into_iter().take(max).collect::<Vec<_>>().join(" ");
    if more > 0 {
        s.push_str(&format!(" (+{more})"));
    }
    s
}

/// Join every image's hashes with the annotations (see the module documentation).
pub fn join(images: &Images, hashes: &[ImageHashes], ann: &HashMap<String, Annotations>) -> Result<Join, Error> {
    let mut ctx = Ctx { order: HashMap::new(), platform: HashMap::new(), hashes: HashMap::new(), ann, labels: HashMap::new() };
    for (i, im) in images.images.iter().enumerate() {
        ctx.order.insert(im.id.as_str(), i);
        ctx.platform.insert(im.id.as_str(), im.platform);
    }
    ctx.labels = component_labels(ann, &ctx.order);
    for h in hashes {
        if !ctx.platform.contains_key(h.image_id.as_str()) {
            return Err(Error::new(format!("images.tsv has no image {}", h.image_id)));
        }
        if !ann.contains_key(&h.image_id) {
            return Err(Error::new(format!("no annotations for image {}", h.image_id)));
        }
        if ctx.hashes.insert(h.image_id.as_str(), h).is_some() {
            return Err(Error::new(format!("image {} is given twice", h.image_id)));
        }
    }

    // value sets per (image, function), and known names across every image
    let mut fn_values: HashMap<(&str, u32), BTreeSet<u32>> = HashMap::new();
    let mut known: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for h in hashes {
        for (v, (names, sites)) in &h.values {
            if !names.is_empty() {
                known.entry(*v).or_insert_with(|| names.clone());
            }
            for s in sites {
                fn_values.entry((h.image_id.as_str(), s.function)).or_default().insert(*v);
            }
        }
        for t in &h.tables {
            for (_, v, names) in &t.slots {
                if !names.is_empty() {
                    known.entry(*v).or_insert_with(|| names.clone());
                }
            }
        }
    }

    // groups and invariance
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut invariant: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new(); // value → canonical fns
    for h in hashes {
        let me = ctx.platform[h.image_id.as_str()];
        for (v, (_, sites)) in &h.values {
            for s in sites {
                let key = ctx.group_key(&h.image_id, s.function, &s.role);
                if let Some(key) = &key {
                    let g = groups.entry(key.clone()).or_default();
                    match known.get(v) {
                        Some(n) => {
                            g.known.insert(*v, n.clone());
                        }
                        None => {
                            g.unknown.insert(*v);
                        }
                    }
                    g.sites.insert(ctx.canon(&h.image_id, s.function));
                    g.images.insert(h.image_id.clone());
                    g.site_fns.insert((h.image_id.clone(), s.function));
                }
                if key.is_some() && !known.contains_key(v) && !structured(*v) {
                    let other_platform = ctx.translations(&h.image_id, s.function).iter().any(|(img, va)| {
                        ctx.platform.get(img.as_str()).is_some_and(|p| *p != me)
                            && fn_values.get(&(img.as_str(), *va)).is_some_and(|set| set.contains(v))
                    });
                    if other_platform {
                        invariant.entry(*v).or_default().insert(ctx.canon(&h.image_id, s.function));
                    }
                }
            }
        }
    }

    // hash consumers
    let pool = hashes.first().map(|h| h.pool_hashes).unwrap_or(0);
    if let Some(h) = hashes.iter().find(|h| h.pool_hashes != pool) {
        return Err(Error::new(format!(
            "image {} was scanned with a pool of {} hashes, image {} with {pool}; scan every image with the same names",
            h.image_id, h.pool_hashes, hashes[0].image_id
        )));
    }
    let rate = pool as f64 / 4294967296.0;
    let mut groups_tested = 0usize;
    let mut consumer_tail_sum = 0.0f64;
    let mut consumers: BTreeSet<String> = BTreeSet::new();
    for (key, g) in &groups {
        if g.known.len() < 2 {
            continue;
        }
        groups_tested += 1;
        let n = (g.known.len() + g.unknown.len()) as f64;
        let tail = poisson_tail(n * rate, g.known.len() as u64);
        if tail < CONSUMER_P {
            consumers.insert(key.clone());
            consumer_tail_sum += tail;
        }
    }

    // cracking
    let mut cache: HashMap<(String, u32), HashMap<u32, String>> = HashMap::new();
    let mut comparisons = 0u64;
    let fn_names = |image: &str, va: u32, cache: &mut HashMap<(String, u32), HashMap<u32, String>>| -> usize {
        let key = (image.to_string(), va);
        if !cache.contains_key(&key) {
            let mut all = BTreeSet::new();
            if let Some(f) = ctx.hashes.get(image).and_then(|h| h.functions.get(&va)) {
                for s in &f.strings {
                    all.extend(names_from_string(s));
                }
            }
            cache.insert(key.clone(), hash_names(&all));
        }
        cache[&key].len()
    };
    let lookup = |image: &str, va: u32, v: u32, cache: &HashMap<(String, u32), HashMap<u32, String>>| -> Option<(String, String)> {
        let name = cache.get(&(image.to_string(), va))?.get(&v)?.clone();
        let f = ctx.hashes.get(image)?.functions.get(&va)?;
        let string = f.strings.iter().find(|s| names_from_string(s).contains(&name)).cloned().unwrap_or_default();
        Some((name, string))
    };
    let mut cracks: BTreeSet<Crack> = BTreeSet::new();
    let mut try_sources = |v: u32, sources: Vec<(String, String, u32)>, cache: &mut HashMap<(String, u32), HashMap<u32, String>>, comparisons: &mut u64| -> bool {
        for (label, img, va) in sources {
            *comparisons += fn_names(&img, va, cache) as u64;
            if let Some((name, string)) = lookup(&img, va, v, cache) {
                cracks.insert(Crack {
                    value: v,
                    name,
                    in_pool: known.contains_key(&v),
                    source: label,
                    image: img.clone(),
                    function: hex(va),
                    string,
                });
                return true;
            }
        }
        false
    };
    let mut cracked: BTreeSet<u32> = BTreeSet::new();
    for h in hashes {
        let img = h.image_id.as_str();
        for (v, (_, sites)) in &h.values {
            if cracked.contains(v) {
                continue;
            }
            for s in sites {
                let mut sources = vec![("function".to_string(), img.to_string(), s.function)];
                for (ti, tva) in ctx.translations(img, s.function) {
                    sources.push((format!("translation {ti}"), ti.clone(), *tva));
                }
                if let Some(f) = h.functions.get(&s.function) {
                    for c in f.callers.iter().chain(f.callees.iter()) {
                        if let Ok(cva) = parse_address(c) {
                            let rel = if f.callers.contains(c) { "caller" } else { "callee" };
                            sources.push((rel.to_string(), img.to_string(), cva));
                        }
                    }
                }
                if try_sources(*v, sources, &mut cache, &mut comparisons) {
                    cracked.insert(*v);
                    break;
                }
            }
        }
        for t in &h.tables {
            for (_, v, names) in &t.slots {
                if !names.is_empty() || cracked.contains(v) {
                    continue;
                }
                let sources = t.referenced_by.iter().map(|f| ("table reference".to_string(), img.to_string(), *f)).collect();
                if try_sources(*v, sources, &mut cache, &mut comparisons) {
                    cracked.insert(*v);
                }
            }
        }
    }
    // sibling prefixes
    for (key, g) in &groups {
        let open: Vec<u32> = g.unknown.iter().copied().filter(|v| !cracked.contains(v)).collect();
        if open.is_empty() || !consumers.contains(key) {
            continue;
        }
        let names: Vec<&String> = g.known.values().flatten().collect();
        let mut prefix_count: BTreeMap<String, usize> = BTreeMap::new();
        for n in &names {
            let lower = n.to_ascii_lowercase();
            let mut seen = BTreeSet::new();
            for (i, c) in lower.char_indices() {
                if c == '_' && seen.insert(i) {
                    *prefix_count.entry(lower[..=i].to_string()).or_default() += 1;
                }
            }
        }
        let prefixes: Vec<&String> = prefix_count.iter().filter(|(_, c)| **c >= 2).map(|(p, _)| p).collect();
        if prefixes.is_empty() {
            continue;
        }
        let mut tokens: BTreeSet<String> = BTreeSet::new();
        for n in &names {
            tokens.extend(n.to_ascii_lowercase().split('_').filter(|t| !t.is_empty()).map(str::to_string));
        }
        for (img, va) in &g.site_fns {
            if let Some(f) = ctx.hashes.get(img.as_str()).and_then(|h| h.functions.get(va)) {
                for s in &f.strings {
                    tokens.extend(names_from_string(s).into_iter().map(|t| t.to_ascii_lowercase()));
                }
            }
        }
        let open_set: BTreeSet<u32> = open.iter().copied().collect();
        for p in &prefixes {
            for t in &tokens {
                let name = format!("{p}{t}");
                comparisons += open_set.len() as u64;
                let h = pandemic_hash_m2_bytes(name.as_bytes());
                if open_set.contains(&h) && !cracked.contains(&h) {
                    cracked.insert(h);
                    cracks.insert(Crack {
                        value: h,
                        name: name.clone(),
                        in_pool: false,
                        source: format!("sibling prefix {p:?} in {key}"),
                        image: String::new(),
                        function: String::new(),
                        string: String::new(),
                    });
                }
            }
        }
    }

    // candidates
    let crack_name: BTreeMap<u32, &str> = cracks.iter().filter(|c| !c.in_pool).map(|c| (c.value, c.name.as_str())).collect();
    let mut rows: Vec<String> = Vec::new();
    let mut cand_g = BTreeSet::new();
    let mut cand_t = BTreeSet::new();
    let mut n_groups = 0usize;
    for (key, g) in &groups {
        if !consumers.contains(key) || g.unknown.is_empty() {
            continue;
        }
        n_groups += 1;
        cand_g.extend(g.unknown.iter().copied());
        let uncracked: Vec<String> = g.unknown.iter().filter(|v| !crack_name.contains_key(v)).map(|v| hex(*v)).collect();
        let cracked_here: Vec<String> = g.unknown.iter().filter_map(|v| crack_name.get(v).map(|n| format!("{}={n}", hex(*v)))).collect();
        rows.push(format!(
            "group\t{key}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            g.images.iter().cloned().collect::<Vec<_>>().join(","),
            g.known.len(),
            first_names(&g.known, 12),
            uncracked.len(),
            if uncracked.is_empty() { "-".into() } else { uncracked.join(" ") },
            if cracked_here.is_empty() { "-".into() } else { cracked_here.join(" ") },
            g.sites.iter().take(8).cloned().collect::<Vec<_>>().join(" ")
        ));
    }
    for h in hashes {
        for t in &h.tables {
            let k: BTreeMap<u32, Vec<String>> = t.slots.iter().filter(|(_, _, n)| !n.is_empty()).map(|(_, v, n)| (*v, n.clone())).collect();
            let unknown: BTreeSet<u32> = t.slots.iter().filter(|(_, _, n)| n.is_empty()).map(|(_, v, _)| *v).collect();
            if unknown.is_empty() {
                continue;
            }
            cand_t.extend(unknown.iter().copied());
            let uncracked: Vec<String> = unknown.iter().filter(|v| !crack_name.contains_key(v)).map(|v| hex(*v)).collect();
            let cracked_here: Vec<String> = unknown.iter().filter_map(|v| crack_name.get(v).map(|n| format!("{}={n}", hex(*v)))).collect();
            rows.push(format!(
                "table\t{}:{} stride {}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                h.image_id,
                hex(t.va),
                t.stride,
                h.image_id,
                k.len(),
                first_names(&k, 12),
                uncracked.len(),
                if uncracked.is_empty() { "-".into() } else { uncracked.join(" ") },
                if cracked_here.is_empty() { "-".into() } else { cracked_here.join(" ") },
                if t.referenced_by.is_empty() {
                    "-".into()
                } else {
                    t.referenced_by.iter().take(8).map(|f| ctx.canon(&h.image_id, *f)).collect::<Vec<_>>().join(" ")
                }
            ));
        }
    }
    let mut cand_i: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for (v, fns) in &invariant {
        if cand_g.contains(v) || cand_t.contains(v) {
            continue;
        }
        for f in fns {
            cand_i.entry(f.clone()).or_default().insert(*v);
        }
    }
    let n_i: BTreeSet<u32> = cand_i.values().flatten().copied().collect();
    for (f, vs) in &cand_i {
        let uncracked: Vec<String> = vs.iter().filter(|v| !crack_name.contains_key(v)).map(|v| hex(*v)).collect();
        let cracked_here: Vec<String> = vs.iter().filter_map(|v| crack_name.get(v).map(|n| format!("{}={n}", hex(*v)))).collect();
        rows.push(format!(
            "invariant\t{f}\t-\t0\t-\t{}\t{}\t{}\t{f}",
            uncracked.len(),
            if uncracked.is_empty() { "-".into() } else { uncracked.join(" ") },
            if cracked_here.is_empty() { "-".into() } else { cracked_here.join(" ") },
        ));
    }
    rows.sort();
    let mut candidates_tsv =
        String::from("kind\tconsumer\timages\tknown\tknown_names\tuncracked\tuncracked_values\tcracked_by_context\tsites\n");
    for r in &rows {
        candidates_tsv.push_str(r);
        candidates_tsv.push('\n');
    }
    let all_candidates: BTreeSet<u32> = cand_g.iter().chain(cand_t.iter()).chain(n_i.iter()).copied().collect();
    let uncracked = all_candidates.iter().filter(|v| !crack_name.contains_key(v)).count();

    let mut cracks_tsv = String::from("value\tname\tin_pool\tsource\timage\tfunction\tstring\n");
    for c in &cracks {
        cracks_tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            hex(c.value),
            c.name,
            if c.in_pool { "yes" } else { "no" },
            c.source,
            if c.image.is_empty() { "-" } else { &c.image },
            if c.function.is_empty() { "-" } else { &c.function },
            if c.string.is_empty() { "-".to_string() } else { c.string.replace(['\t', '\n'], " ") },
        ));
    }
    Ok(Join {
        candidates_tsv,
        cracks_tsv,
        cracks: cracks.into_iter().collect(),
        groups_tested,
        consumers: consumers.len(),
        consumer_tail_sum,
        groups: n_groups,
        candidates_g: cand_g.len(),
        candidates_t: cand_t.len(),
        candidates_i: n_i.len(),
        uncracked,
        comparisons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_yield_tokens_and_camel_pieces() {
        let n = names_from_string("SupportMenu %s: Enter_up");
        for want in ["SupportMenu %s: Enter_up", "SupportMenu", "Support_Menu", "Support", "Menu", "Enter_up", "Enter", "up"] {
            assert!(n.contains(want), "{want} missing from {n:?}");
        }
        assert!(!n.contains("s"), "one-character tokens are not names: {n:?}");
    }

    #[test]
    fn structured_values_are_masks_floats_and_fourccs() {
        assert!(structured(0x8000_0000));
        assert!(structured(0xffff_fffe));
        assert!(structured(1.0f32.to_bits()));
        assert!(structured(0.1f32.to_bits()), "0x3dcccccd");
        assert!(structured(2.75f32.to_bits()));
        assert!(structured(u32::from_le_bytes(*b"CHDR")));
        for name in ["enter", "exit", "pause", "shell", "loading"] {
            assert!(!structured(pandemic_hash_m2_bytes(name.as_bytes())), "{name}");
        }
    }

    #[test]
    fn a_signature_keeps_params_and_numbers() {
        assert_eq!(signature("* (uint *)(param_1 + 0x24)"), "param_1 0x24");
        assert_eq!(signature("param_3 [2]"), "param_3 2");
        assert_eq!(signature("FUN_0085d7f0()"), "");
    }

    #[test]
    fn labels_follow_translations_transitively() {
        // pc-v11 ↔ x360-retail and x360-retail ↔ ps3-v100; the PS3 copy has no edge to the PC one
        let mut ann = HashMap::new();
        let mut a = Annotations::default();
        a.translations.insert(0x005f06c0, vec![("x360-retail".into(), 0x8234fd98)]);
        ann.insert("pc-v11".to_string(), a);
        let mut b = Annotations::default();
        b.translations.insert(0x8234fd98, vec![("ps3-v100".into(), 0x00587390)]);
        ann.insert("x360-retail".to_string(), b);
        let order: HashMap<&str, usize> = [("pc-v11", 2), ("ps3-v100", 3), ("x360-retail", 11)].into_iter().collect();
        let l = component_labels(&ann, &order);
        for n in [("ps3-v100", 0x00587390), ("x360-retail", 0x8234fd98), ("pc-v11", 0x005f06c0)] {
            assert_eq!(l[&(n.0.to_string(), n.1)], "pc-v11:0x005f06c0", "{n:?}");
        }
    }

    #[test]
    fn chance_tails_separate_generic_consumers_from_hash_consumers() {
        let rate = 734_360.0 / 4294967296.0;
        // 8 pool hits among 29013 constants is what chance gives
        assert!(poisson_tail(29013.0 * rate, 8) > 0.1);
        // 2 among 46 is not
        let t = poisson_tail(46.0 * rate, 2);
        assert!(t < CONSUMER_P && t > 1e-6, "{t}");
        assert_eq!(poisson_tail(1.0, 0), 1.0);
        assert!((poisson_tail(1.0, 1) - (1.0 - (-1.0f64).exp())).abs() < 1e-12);
    }

    fn hashes(image: &str, lines: &[&str]) -> ImageHashes {
        let mut t = format!("{{\"kind\":\"image\",\"image_id\":\"{image}\",\"pool_hashes\":734360}}\n");
        for l in lines {
            t.push_str(l);
            t.push('\n');
        }
        t.push_str("{\"kind\":\"end\"}\n");
        read_hashes(&t, image).unwrap()
    }

    fn annotations(image: &str, lines: &[&str]) -> Annotations {
        let mut t = format!("{{\"kind\":\"image\",\"image_id\":\"{image}\"}}\n");
        for l in lines {
            t.push_str(l);
            t.push('\n');
        }
        t.push_str("{\"kind\":\"end\",\"functions\":0}\n");
        read_annotations(&t, image, image).unwrap()
    }

    #[test]
    fn context_strings_crack_an_unknown_in_a_known_group_across_images() {
        let catalogue = crate::Catalogue::embedded().unwrap();
        let images = Images::embedded(&catalogue).unwrap();
        let enter = pandemic_hash_m2_bytes(b"enter");
        let exit = pandemic_hash_m2_bytes(b"exit");
        let shell = pandemic_hash_m2_bytes(b"shell");
        let unknown = 0x5305_6c27u32; // not "Attract"-derived; stays uncracked
        let cmp = r#""role":"cmp","op":"==","lhs":"uVar1","lhs_def":"* (uint *)(param_1 + 0x24)""#;
        let pc = hashes(
            "pc-v11",
            &[
                &format!(r#"{{"kind":"value","value":"0x{shell:08x}","names":["shell"],"sites":[{{"function":"0x005f06c0",{cmp}}}]}}"#),
                &format!(r#"{{"kind":"value","value":"0x{exit:08x}","names":["exit"],"sites":[{{"function":"0x005f06c0",{cmp}}}]}}"#),
                &format!(r#"{{"kind":"value","value":"0x{unknown:08x}","names":[],"sites":[{{"function":"0x005f06c0",{cmp}}}]}}"#),
                r#"{"kind":"function","va":"0x005f06c0","name":"FUN_005f06c0","strings":["Shell"],"callers":[],"callees":[]}"#,
            ],
        );
        // the x360 function holds an unknown the PC one lacks; its translation's strings crack it
        let x = hashes(
            "x360-retail",
            &[
                &format!(r#"{{"kind":"value","value":"0x{enter:08x}","names":[],"sites":[{{"function":"0x8234fd98",{cmp}}}]}}"#),
                r#"{"kind":"function","va":"0x8234fd98","name":"FUN_8234fd98","strings":[],"callers":[],"callees":[]}"#,
            ],
        );
        let pc_fn = hashes(
            "pc-v11",
            &[r#"{"kind":"function","va":"0x005f06c0","name":"FUN_005f06c0","strings":["Enter_up"],"callers":[],"callees":[]}"#],
        );
        let mut pc = pc;
        pc.functions = pc_fn.functions;
        let mut ann = HashMap::new();
        ann.insert(
            "pc-v11".to_string(),
            annotations("pc-v11", &[r#"{"va":"0x005f06c0","names":[{"name":"FUN_005f06c0","source":"ghidra:DEFAULT","detail":null}],"translations":[{"image":"x360-retail","va":"0x8234fd98"}]}"#]),
        );
        ann.insert(
            "x360-retail".to_string(),
            annotations("x360-retail", &[r#"{"va":"0x8234fd98","names":[],"translations":[{"image":"pc-v11","va":"0x005f06c0"}]}"#]),
        );
        let j = join(&images, &[pc, x], &ann).unwrap();
        let c: Vec<(&str, &str)> = j.cracks.iter().filter(|c| !c.in_pool).map(|c| (c.name.as_str(), c.source.as_str())).collect();
        assert_eq!(c, vec![("Enter", "translation pc-v11")], "{:?}", j.cracks);
        // one group, the PC and x360 sites merged under the PC label; the unknown stays uncracked
        assert!(j.candidates_tsv.contains("group\tcmp pc-v11:0x005f06c0 [param_1 0x24]\tpc-v11,x360-retail\t2\texit shell\t1\t0x53056c27\t"), "{}", j.candidates_tsv);
        assert_eq!(j.uncracked, 1);
    }
}
