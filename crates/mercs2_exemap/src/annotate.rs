//! Per-function annotations of one image for a search corpus: every name a function carries, with
//! where it comes from, and its agreed translation to every other image of the match graph.
//!
//! **Input.** A functions file is JSON Lines written by the Ghidra exporter: an image record
//! (`{"kind":"image","image_id",…,"program_sha256",…,"functions":N}`), then `N` function records
//! (`{"kind":"function","va":"0x%08x","name","name_source","thunk",…}`), then an end record
//! (`{"kind":"end","functions":N,…}`). [`read_functions`] reads only the record kinds, the image
//! record's `image_id`, `program_sha256` and `functions`, each function's `va`, `name`,
//! `name_source`, `thunk` and `size`, and the end record's `functions`; every other field (the
//! decompiled text among them) is neither kept nor checked. It fails on an empty line, a line that is not a
//! JSON object, a missing or mistyped field it reads, a `va` that is not `0x` and eight lowercase
//! hex digits, a `va` that repeats, a first record that is not the image record, a record after the
//! end record, a missing end record (a truncated export), and a function count that differs from
//! the image record's or the end record's `functions`.
//!
//! [`annotate`] fails when the file's `image_id` is not the image annotated or its
//! `program_sha256` is not that image's `images.tsv` sha256, and refuses an image whose
//! `code_image` names another image (its code is annotated once, under that image, whose image
//! record lists it among its variants).
//!
//! **Names** ([`sidecar_names`]). Every name of a function is listed; none is chosen:
//!
//! | source | name | detail |
//! |---|---|---|
//! | `ghidra:<name_source>` | the functions file's `name` | `thunk` when the record's `thunk` is true, else null |
//! | `rtti` | the row's `class`, or its `raw_name` when the class column is empty (template and operator names) | `<that name> vtable 0x%08x slot <k>` |
//! | `luareg` | `<namespace>.<name>`, or `<name>` when the namespace column is empty | null |
//! | `import` | `<library>!<name>` | `<kind> ordinal <n>` (Xbox 360) or `function fnid <fnid>` (PS3) |
//!
//! The rtti and luareg rows come from `<exports>/<id>.rtti.tsv` and `<exports>/<id>.luareg.tsv`
//! ([`crate::matcher::Exports::load`]), required for every image; each row names the function
//! whose va is its `function`. The import rows come from `<exports>/<id>.imports.tsv`, written by
//! `exemap imports` for an image whose derivation chain has `xex:` steps and no `unself:` step
//! (header [`crate::xbox::IMPORTS_HEADER`]) or `unself:` steps and no `xex:` step (header
//! [`crate::prx::PRX_IMPORTS_HEADER`]); each row names the function whose va is its `address`.
//! For any other image (every PC image) the imports file is not produced, and one that exists is
//! an error, as is a chain with both steps. A required file that is missing is an error naming it.
//!
//! **Translations.** For every other image of the match graph ([`crate::xmap::Graph`]), the
//! function is translated along every path ([`crate::xmap::Graph::route`], computed once per
//! target image): an agreed translation is listed with whether the direct edge gives it and how
//! many of the paths answer; an untranslated one (no path answers, or no path) is not listed; a
//! contradicted one fails the whole annotation, listing every contradiction of every function as
//! [`crate::xmap::Graph::coverage`] reports them.
//!
//! **Output** (JSON Lines): an image record
//! `{"kind":"image","image_id","sha256","variants","graph_images","untranslatable_images"}` —
//! `variants` the images whose `code_image` is this one, `graph_images` every image of the match
//! graph but this one, `untranslatable_images` every `images.tsv` image (but this one and its
//! variants) the graph has no path to — then one record per function, in the functions file's
//! order, `{"va","names":[{"name","source","detail"}],"translations":[{"image","va","direct",
//! "answering_paths","paths"}]}`, then `{"kind":"end","functions":N}`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::images::{Images, MapImage, Step};
use crate::xmap::{parse_address, Graph, Verdict, Walk};
use crate::Error;

/// One function record of a functions file: the fields annotate reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub va: u32,
    pub name: String,
    pub name_source: String,
    pub thunk: bool,
    /// Ghidra's function body size in bytes (the span of its address set).
    pub size: u32,
}

/// A functions file, checked for completeness ([`read_functions`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionsFile {
    pub image_id: String,
    pub program_sha256: String,
    pub functions: Vec<Function>,
}

/// Every field read from any record; which ones a record must have depends on its kind. Fields
/// not listed are ignored without being kept.
#[derive(Deserialize)]
struct Record {
    kind: String,
    image_id: Option<String>,
    program_sha256: Option<String>,
    functions: Option<u64>,
    va: Option<String>,
    name: Option<String>,
    name_source: Option<String>,
    thunk: Option<bool>,
    size: Option<u32>,
}

/// `0x` and eight lowercase hex digits.
fn parse_va(s: &str) -> Result<u32, Error> {
    let v = parse_address(s)?;
    if format!("0x{v:08x}") != s {
        return Err(Error::new(format!("address {s:?} is not 0x and eight lowercase hex digits")));
    }
    Ok(v)
}

/// Read a functions file (`what` names it in errors); see the module documentation for every rule.
pub fn read_functions(text: &str, what: &str) -> Result<FunctionsFile, Error> {
    let mut header: Option<(String, String, u64)> = None;
    let mut end: Option<(usize, u64)> = None;
    let mut functions = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let at = |m: String| Error::new(format!("{what} line {n}: {m}"));
        if let Some((end_line, _)) = end {
            return Err(at(format!("a record after the end record on line {end_line}")));
        }
        if line.is_empty() {
            return Err(at("empty line".into()));
        }
        let r: Record = serde_json::from_str(line).map_err(|e| at(e.to_string()))?;
        let need = |v: Option<String>, field: &str| v.ok_or_else(|| at(format!("a {} record has no string {field:?}", r.kind)));
        let need_n = |v: Option<u64>| v.ok_or_else(|| at(format!("a {} record has no number \"functions\"", r.kind)));
        match (r.kind.as_str(), n) {
            ("image", 1) => {
                let image_id = need(r.image_id.clone(), "image_id")?;
                let sha = need(r.program_sha256.clone(), "program_sha256")?;
                header = Some((image_id, sha, need_n(r.functions)?));
            }
            (_, 1) => return Err(at(format!("the first record is a {:?} record, not the image record", r.kind))),
            ("image", _) => return Err(at("a second image record".into())),
            ("function", _) => {
                let va_text = need(r.va.clone(), "va")?;
                let va = parse_va(&va_text).map_err(|e| at(e.to_string()))?;
                if let Some(prev) = seen.insert(va, n) {
                    return Err(at(format!("function {va_text} repeats the one on line {prev}")));
                }
                let name = need(r.name.clone(), "name")?;
                let name_source = need(r.name_source.clone(), "name_source")?;
                let thunk = r.thunk.ok_or_else(|| at(format!("function {va_text} has no boolean \"thunk\"")))?;
                let size = r.size.ok_or_else(|| at(format!("function {va_text} has no number \"size\"")))?;
                functions.push(Function { va, name, name_source, thunk, size });
            }
            ("end", _) => end = Some((n, need_n(r.functions)?)),
            (other, _) => return Err(at(format!("unknown record kind {other:?}"))),
        }
    }
    let (image_id, program_sha256, declared) =
        header.ok_or_else(|| Error::new(format!("{what}: empty; the first record must be the image record")))?;
    let (_, ended) = end.ok_or_else(|| {
        Error::new(format!(
            "{what}: no end record after {} function records (the image record declares {declared}); the export is truncated",
            functions.len()
        ))
    })?;
    if functions.len() as u64 != declared || ended != declared {
        return Err(Error::new(format!(
            "{what}: {} function records, the image record declares {declared} and the end record {ended}",
            functions.len()
        )));
    }
    Ok(FunctionsFile { image_id, program_sha256, functions })
}

/// One name of a function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Name {
    pub name: String,
    pub source: String,
    pub detail: Option<String>,
}

/// One row of an `<id>.imports.tsv` export: the fields annotate names a function by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRow {
    pub address: u32,
    /// `function` or `variable` (Xbox 360); `function` (PS3).
    pub kind: String,
    pub library: String,
    /// The decimal ordinal (Xbox 360) or the 8-digit FNID (PS3).
    pub ordinal_or_fnid: String,
    pub name: String,
}

/// Parse an imports export written by `exemap imports` (`what` names it in errors). `prx` selects
/// the PS3 format ([`crate::prx::PRX_IMPORTS_HEADER`]) over the Xbox 360 one
/// ([`crate::xbox::IMPORTS_HEADER`]); the header line must be the selected one. Fails on a row
/// whose field count differs from the header's, an address that is not `0x` and eight lowercase
/// hex digits, a kind other than `function` or `variable` (`function` only for PS3), an ordinal
/// that is not decimal, an FNID that is not eight lowercase hex digits, a stub that is not
/// lowercase hex of whole bytes, an empty library or name, and an address that repeats.
pub fn parse_imports(text: &str, prx: bool, what: &str) -> Result<Vec<ImportRow>, Error> {
    let header = if prx { crate::prx::PRX_IMPORTS_HEADER } else { crate::xbox::IMPORTS_HEADER };
    let mut lines = text.lines();
    let first = lines.next().ok_or_else(|| Error::new(format!("{what}: empty")))?;
    if first != header {
        return Err(Error::new(format!("{what}: header {first:?} is not {header:?}")));
    }
    let width = header.split('\t').count();
    let lower_hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let mut rows = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (i, l) in lines.enumerate() {
        let n = i + 2;
        let at = |m: String| Error::new(format!("{what} line {n}: {m}"));
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != width {
            return Err(at(format!("{} fields, expected {width}", f.len())));
        }
        let address = parse_va(f[0]).map_err(|e| at(e.to_string()))?;
        match (f[1], prx) {
            ("function", _) | ("variable", false) => {}
            (k, _) => return Err(at(format!("kind {k:?} is not {}", if prx { "function" } else { "function or variable" }))),
        }
        if f[2].is_empty() || f[4].is_empty() {
            return Err(at("empty library or name".into()));
        }
        if prx {
            if f[3].len() != 8 || !lower_hex(f[3]) {
                return Err(at(format!("fnid {:?} is not eight lowercase hex digits", f[3])));
            }
            if f[5].len() % 2 != 0 || !lower_hex(f[5]) {
                return Err(at(format!("stub {:?} is not lowercase hex of whole bytes", f[5])));
            }
        } else if f[3].is_empty() || !f[3].bytes().all(|b| b.is_ascii_digit()) {
            return Err(at(format!("ordinal {:?} is not decimal", f[3])));
        }
        if let Some(prev) = seen.insert(address, n) {
            return Err(at(format!("address {} repeats the one on line {prev}", f[0])));
        }
        rows.push(ImportRow {
            address,
            kind: f[1].to_string(),
            library: f[2].to_string(),
            ordinal_or_fnid: f[3].to_string(),
            name: f[4].to_string(),
        });
    }
    Ok(rows)
}

/// The rtti, luareg and imports names of `image`'s functions from the exports directory `dir`,
/// by function va, each in file order (rtti rows, then luareg rows, then import rows).
pub fn sidecar_names(dir: &Path, image: &MapImage) -> Result<HashMap<u32, Vec<Name>>, Error> {
    let id = &image.id;
    let exports = crate::matcher::Exports::load(dir, id, false)?;
    let mut by_va: HashMap<u32, Vec<Name>> = HashMap::new();
    for v in &exports.vtables {
        let label = if v.class.is_empty() { v.raw_name.clone() } else { v.class.clone() };
        let detail = format!("{label} vtable 0x{:08x} slot {}", v.vtable, v.slot);
        by_va.entry(v.function).or_default().push(Name { name: label, source: "rtti".into(), detail: Some(detail) });
    }
    for b in &exports.bindings {
        let name = if b.namespace.is_empty() { b.name.clone() } else { format!("{}.{}", b.namespace, b.name) };
        by_va.entry(b.function).or_default().push(Name { name, source: "luareg".into(), detail: None });
    }
    let xex = image.chain.iter().filter(|l| matches!(l.step, Step::Xex(_))).count();
    let unself = image.chain.iter().filter(|l| matches!(l.step, Step::Unself(_))).count();
    let path = dir.join(format!("{id}.imports.tsv"));
    let prx = match (xex > 0, unself > 0) {
        (false, false) => {
            if path.exists() {
                return Err(Error::new(format!(
                    "{}: image {id}'s derivation chain has no xex or unself step, so exemap imports writes no imports for it",
                    path.display()
                )));
            }
            return Ok(by_va);
        }
        (true, true) => {
            return Err(Error::new(format!(
                "image {id}: its derivation chain has {xex} xex steps and {unself} unself steps; its imports format is undefined"
            )))
        }
        (false, true) => true,
        (true, false) => false,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| Error::new(format!("read {}: {e}", path.display())))?;
    for r in parse_imports(&text, prx, &path.display().to_string())? {
        let detail = if prx { format!("function fnid {}", r.ordinal_or_fnid) } else { format!("{} ordinal {}", r.kind, r.ordinal_or_fnid) };
        by_va.entry(r.address).or_default().push(Name {
            name: format!("{}!{}", r.library, r.name),
            source: "import".into(),
            detail: Some(detail),
        });
    }
    Ok(by_va)
}

#[derive(Serialize)]
struct ImageLine<'a> {
    kind: &'static str,
    image_id: &'a str,
    sha256: &'a str,
    variants: Vec<&'a str>,
    graph_images: Vec<&'a str>,
    untranslatable_images: Vec<&'a str>,
}

#[derive(Serialize)]
struct Translated<'a> {
    image: &'a str,
    va: String,
    direct: bool,
    answering_paths: usize,
    paths: usize,
}

#[derive(Serialize)]
struct FunctionLine<'a> {
    va: String,
    names: Vec<&'a Name>,
    translations: Vec<Translated<'a>>,
}

#[derive(Serialize)]
struct EndLine {
    kind: &'static str,
    functions: usize,
}

/// The annotation of one image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotation {
    /// The JSON Lines text.
    pub text: String,
    pub functions: usize,
    /// Per graph image: how many functions translate to it.
    pub translated: BTreeMap<String, usize>,
    pub untranslatable_images: Vec<String>,
    /// Per name source (`rtti`, `luareg`, `import`): the sidecar rows that name a function of the
    /// functions file, and the rows whose va is no function of it (and so name nothing).
    pub sidecar_rows: BTreeMap<String, (usize, usize)>,
}

/// The `images.tsv` image `image_id`, when it can be annotated: it must exist and have no
/// `code_image` (a variant is annotated under its code image).
pub fn annotated_image<'i>(images: &'i Images, image_id: &str) -> Result<&'i MapImage, Error> {
    let image = images.image(image_id).ok_or_else(|| Error::new(format!("images.tsv has no image {image_id}")))?;
    if let Some(c) = &image.code_image {
        return Err(Error::new(format!(
            "image {image_id} shares its code with {c} (images.tsv code_image); annotate {c}, whose image record lists {image_id} among its variants"
        )));
    }
    Ok(image)
}

/// [`annotated_image`] `image_id`, when `file` is its functions file: the file's `image_id` must be
/// `image_id` and its `program_sha256` the image's `images.tsv` sha256.
pub fn checked_image<'i>(images: &'i Images, image_id: &str, file: &FunctionsFile) -> Result<&'i MapImage, Error> {
    let image = annotated_image(images, image_id)?;
    if file.image_id != image_id {
        return Err(Error::new(format!("the functions file is of image {}, not {image_id}", file.image_id)));
    }
    if file.program_sha256 != image.sha256 {
        return Err(Error::new(format!(
            "the functions file's program_sha256 is {}; images.tsv says image {image_id} is {}",
            file.program_sha256, image.sha256
        )));
    }
    Ok(image)
}

/// Annotate image `image_id` from its functions file, its sidecar names ([`sidecar_names`]) and
/// the match graph; see the module documentation for every rule. Fails on a contradicted
/// translation, listing every one.
pub fn annotate(
    images: &Images,
    image_id: &str,
    file: &FunctionsFile,
    names: &HashMap<u32, Vec<Name>>,
    graph: &Graph,
) -> Result<Annotation, Error> {
    let image = checked_image(images, image_id, file)?;
    let variants: Vec<&str> =
        images.images.iter().filter(|m| m.code_image.as_deref() == Some(image_id)).map(|m| m.id.as_str()).collect();
    let graph_images: Vec<&str> = graph.images().filter(|t| *t != image_id).collect();
    let untranslatable: Vec<&str> = images
        .images
        .iter()
        .map(|m| m.id.as_str())
        .filter(|id| *id != image_id && !variants.contains(id) && graph.paths(image_id, id).is_empty())
        .collect();
    let routes: Vec<(&str, crate::xmap::Route)> = graph_images.iter().map(|t| (*t, graph.route(image_id, t))).collect();

    let mut text = line(&ImageLine {
        kind: "image",
        image_id,
        sha256: &image.sha256,
        variants: variants.clone(),
        graph_images: graph_images.clone(),
        untranslatable_images: untranslatable.clone(),
    })?;
    let mut translated: BTreeMap<String, usize> = graph_images.iter().map(|t| (t.to_string(), 0)).collect();
    let mut contradictions = Vec::new();
    let functions: HashSet<u32> = file.functions.iter().map(|f| f.va).collect();
    for f in &file.functions {
        let ghidra = Name {
            name: f.name.clone(),
            source: format!("ghidra:{}", f.name_source),
            detail: f.thunk.then(|| "thunk".to_string()),
        };
        let mut fnames = vec![&ghidra];
        fnames.extend(names.get(&f.va).into_iter().flatten());
        let mut translations = Vec::new();
        for (to, route) in &routes {
            let w = route.walks(f.va);
            match w.verdict() {
                Verdict::NoPath | Verdict::Untranslated => {}
                Verdict::Agreed(g) => {
                    *translated.get_mut(*to).unwrap() += 1;
                    translations.push(Translated {
                        image: to,
                        va: format!("0x{g:08x}"),
                        direct: matches!(route.direct(&w), Some(Walk::Answer(_))),
                        answering_paths: w.answering(),
                        paths: route.paths.len(),
                    });
                }
                Verdict::Contradicted(answers) => contradictions.push(route.contradiction(image_id, f.va, to, &answers)),
            }
        }
        text.push_str(&line(&FunctionLine { va: format!("0x{:08x}", f.va), names: fnames, translations })?);
    }
    if !contradictions.is_empty() {
        let list: Vec<String> = contradictions.iter().map(|c| c.tsv()).collect();
        return Err(Error::new(format!(
            "{} translations are contradicted by the match graph:\n{}",
            contradictions.len(),
            list.join("\n")
        )));
    }
    text.push_str(&line(&EndLine { kind: "end", functions: file.functions.len() })?);
    let mut sidecar_rows: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (va, ns) in names {
        for n in ns {
            let e = sidecar_rows.entry(n.source.clone()).or_default();
            if functions.contains(va) {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    Ok(Annotation {
        text,
        functions: file.functions.len(),
        translated,
        untranslatable_images: untranslatable.iter().map(|s| s.to_string()).collect(),
        sidecar_rows,
    })
}

/// The seed starts of an image: every va an `rtti` or `luareg` sidecar row names (a vtable slot or
/// a registered Lua C function — code the program calls through a table) that is not a function of
/// the functions file, ascending, each with the names that point at it. Import rows are not seeds:
/// they name import stubs and data slots, not game functions.
///
/// `starts` is the `CreateFunctions.java` start list (one hex VA per line, no `0x`); `names` is a
/// TSV (`va`, `source`, `name`, `detail`; `-` for no detail), one row per name, for review.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seeds {
    pub starts: String,
    pub names: String,
    pub count: usize,
}

pub fn seeds(images: &Images, image_id: &str, file: &FunctionsFile, names: &HashMap<u32, Vec<Name>>) -> Result<Seeds, Error> {
    checked_image(images, image_id, file)?;
    let functions: HashSet<u32> = file.functions.iter().map(|f| f.va).collect();
    let mut vas: Vec<u32> = names
        .iter()
        .filter(|(va, ns)| !functions.contains(va) && ns.iter().any(|n| n.source == "rtti" || n.source == "luareg"))
        .map(|(va, _)| *va)
        .collect();
    vas.sort_unstable();
    let mut out = Seeds { names: "va\tsource\tname\tdetail\n".into(), count: vas.len(), ..Default::default() };
    for va in vas {
        out.starts.push_str(&format!("{va:08x}\n"));
        for n in names[&va].iter().filter(|n| n.source == "rtti" || n.source == "luareg") {
            out.names.push_str(&format!("0x{va:08x}\t{}\t{}\t{}\n", n.source, n.name, n.detail.as_deref().unwrap_or(crate::NO_VALUE)));
        }
    }
    Ok(out)
}

/// One output record as a JSON line.
fn line<T: Serialize>(record: &T) -> Result<String, Error> {
    let mut s = serde_json::to_string(record).map_err(|e| Error::new(format!("serialise an output record: {e}")))?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmap::Pairs;
    use crate::Catalogue;

    fn images() -> Images {
        Images::embedded(&Catalogue::embedded().unwrap()).unwrap()
    }

    fn sha(id: &str) -> String {
        images().image(id).unwrap().sha256.clone()
    }

    /// A functions file of `id` with `program_sha256` `sha`, one record per `(va, name)`.
    fn functions_text(id: &str, sha: &str, fns: &[(u32, &str)]) -> String {
        let mut s = format!(
            "{{\"kind\":\"image\",\"image_id\":\"{id}\",\"program\":\"p\",\"program_sha256\":\"{sha}\",\"ghidra_version\":\"12.1\",\"language\":\"L\",\"compiler_spec\":\"c\",\"functions\":{}}}\n",
            fns.len()
        );
        for (va, name) in fns {
            s.push_str(&format!(
                "{{\"kind\":\"function\",\"va\":\"0x{va:08x}\",\"name\":\"{name}\",\"name_source\":\"DEFAULT\",\"block\":\".text\",\"size\":4,\"thunk\":false,\"thunk_target\":null,\"callers\":[],\"callees\":[],\"decomp\":\"void {name}(void) {{}}\",\"decomp_error\":null}}\n"
            ));
        }
        s.push_str(&format!("{{\"kind\":\"end\",\"functions\":{},\"decompiled\":{},\"failed\":0}}\n", fns.len(), fns.len()));
        s
    }

    fn pairs(a: &str, b: &str, p: &[(u32, u32)]) -> Pairs {
        Pairs { image_a: a.into(), image_b: b.into(), pairs: p.to_vec() }
    }

    fn records(text: &str) -> Vec<serde_json::Value> {
        text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// A fresh exports directory holding `files`.
    fn exports(test: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mercs2_exemap_annotate_{test}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        dir
    }

    const RTTI: &str = "abi\traw_name\tclass\tvtable\tsubobject_offset\tslot\tfunction\tdeleting_destructor\n\
        msvc\t.?AVFoo@@\tFoo\t0x00b00000\t0\t3\t0x00401000\t0\n\
        msvc\t.?AV?$vector@H@std@@\t\t0x00b00100\t0\t0\t0x00401000\t\n\
        msvc\t.?AVBar@@\tBar\t0x00b00200\t0\t0\t0x00409999\t\n";
    const LUAREG: &str = "table\tnamespace\tindex\tname\tfunction\n\
        0x00b10000\tPg\t0\tSpawn\t0x00401000\n\
        0x00b10100\t\t0\tprint\t0x00402000\n";

    #[test]
    fn every_name_of_a_function_is_listed_with_its_source() {
        let dir = exports("names", &[("pc-v10.rtti.tsv", RTTI), ("pc-v10.luareg.tsv", LUAREG)]);
        let ims = images();
        let names = sidecar_names(&dir, ims.image("pc-v10").unwrap()).unwrap();
        let file = read_functions(&functions_text("pc-v10", &sha("pc-v10"), &[(0x401000, "FUN_00401000"), (0x402000, "print_impl")]), "f").unwrap();
        let g = Graph::new(&[]).unwrap();
        let a = annotate(&ims, "pc-v10", &file, &names, &g).unwrap();
        let r = records(&a.text);
        assert_eq!(r.len(), 4);
        assert_eq!(
            r[1]["names"],
            serde_json::json!([
                {"name": "FUN_00401000", "source": "ghidra:DEFAULT", "detail": null},
                {"name": "Foo", "source": "rtti", "detail": "Foo vtable 0x00b00000 slot 3"},
                {"name": ".?AV?$vector@H@std@@", "source": "rtti", "detail": ".?AV?$vector@H@std@@ vtable 0x00b00100 slot 0"},
                {"name": "Pg.Spawn", "source": "luareg", "detail": null},
            ])
        );
        assert_eq!(
            r[2]["names"],
            serde_json::json!([
                {"name": "print_impl", "source": "ghidra:DEFAULT", "detail": null},
                {"name": "print", "source": "luareg", "detail": null},
            ])
        );
        // Bar's slot names 0x00409999, which is no function of the file.
        assert_eq!(a.sidecar_rows["rtti"], (2, 1));
        assert_eq!(a.sidecar_rows["luareg"], (2, 0));
        assert_eq!(r[3], serde_json::json!({"kind": "end", "functions": 2}));
    }

    #[test]
    fn seeds_are_the_rtti_and_luareg_targets_that_are_no_function() {
        let dir = exports("seeds", &[("pc-v10.rtti.tsv", RTTI), ("pc-v10.luareg.tsv", LUAREG)]);
        let ims = images();
        let names = sidecar_names(&dir, ims.image("pc-v10").unwrap()).unwrap();
        let file = read_functions(&functions_text("pc-v10", &sha("pc-v10"), &[(0x401000, "FUN_00401000")]), "f").unwrap();
        let s = seeds(&ims, "pc-v10", &file, &names).unwrap();
        // 0x00401000 is a function; Bar's slot 0x00409999 and print's 0x00402000 are not.
        assert_eq!(s.count, 2);
        assert_eq!(s.starts, "00402000\n00409999\n");
        assert_eq!(
            s.names,
            "va\tsource\tname\tdetail\n0x00402000\tluareg\tprint\t-\n0x00409999\trtti\tBar\tBar vtable 0x00b00200 slot 0\n"
        );
        // An import address is never a seed.
        let xbox = "address\tkind\tlibrary\tordinal\tname\n0x82000600\tvariable\txam.xex\t651\tXNotifyGetNext\n";
        let dir = exports("seeds_x360", &[("x360-retail.rtti.tsv", RTTI), ("x360-retail.luareg.tsv", LUAREG), ("x360-retail.imports.tsv", xbox)]);
        let names = sidecar_names(&dir, ims.image("x360-retail").unwrap()).unwrap();
        let file = read_functions(&functions_text("x360-retail", &sha("x360-retail"), &[(0x401000, "FUN_00401000"), (0x402000, "p"), (0x409999, "b")]), "f").unwrap();
        assert_eq!(seeds(&ims, "x360-retail", &file, &names).unwrap().count, 0);
        // The functions file must be the image's.
        let e = seeds(&ims, "pc-v11", &file, &names).unwrap_err().to_string();
        assert!(e.contains("the functions file is of image x360-retail, not pc-v11"), "{e}");
    }

    #[test]
    fn imports_follow_the_derivation_chain() {
        let ims = images();
        let xbox = "address\tkind\tlibrary\tordinal\tname\n0x82000600\tvariable\txam.xex\t651\tXNotifyGetNext\n0x82a3cc54\tfunction\txam.xex\t651\tXNotifyGetNext\n";
        let dir = exports(
            "imports_x360",
            &[("x360-retail.rtti.tsv", RTTI), ("x360-retail.luareg.tsv", LUAREG), ("x360-retail.imports.tsv", xbox)],
        );
        let names = sidecar_names(&dir, ims.image("x360-retail").unwrap()).unwrap();
        assert_eq!(
            names[&0x82a3cc54],
            vec![Name {
                name: "xam.xex!XNotifyGetNext".into(),
                source: "import".into(),
                detail: Some("function ordinal 651".into())
            }]
        );
        // A PS3 image reads the PRX format, and a file in the other format is refused.
        let dir = exports("imports_ps3", &[("ps3-v100.rtti.tsv", RTTI), ("ps3-v100.luareg.tsv", LUAREG), ("ps3-v100.imports.tsv", xbox)]);
        let e = sidecar_names(&dir, ims.image("ps3-v100").unwrap()).unwrap_err().to_string();
        assert!(e.contains("is not \"address\\tkind\\tlibrary\\tfnid\\tname\\tstub\""), "{e}");
        // A required imports file that is missing is an error naming it.
        std::fs::remove_file(dir.join("ps3-v100.imports.tsv")).unwrap();
        let e = sidecar_names(&dir, ims.image("ps3-v100").unwrap()).unwrap_err().to_string();
        assert!(e.contains("ps3-v100.imports.tsv"), "{e}");
        // A PC image has no imports file; one that exists is an error.
        let dir = exports("imports_pc", &[("pc-v10.rtti.tsv", RTTI), ("pc-v10.luareg.tsv", LUAREG), ("pc-v10.imports.tsv", xbox)]);
        let e = sidecar_names(&dir, ims.image("pc-v10").unwrap()).unwrap_err().to_string();
        assert!(e.contains("writes no imports for it"), "{e}");
        // A missing rtti export is an error naming it.
        let dir = exports("no_rtti", &[("pc-v10.luareg.tsv", LUAREG)]);
        let e = sidecar_names(&dir, ims.image("pc-v10").unwrap()).unwrap_err().to_string();
        assert!(e.contains("pc-v10.rtti.tsv"), "{e}");
    }

    #[test]
    fn prx_imports_parse_and_bad_rows_are_refused() {
        let ok = "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tfunction\tcellSysmodule\t32267a31\tcellSysmoduleLoadModule\t3980\n";
        assert_eq!(parse_imports(ok, true, "t").unwrap()[0].address, 0xdb40a4);
        for bad in [
            "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tvariable\tl\t32267a31\tn\t3980\n",
            "address\tkind\tlibrary\tfnid\tname\tstub\n0xdb40a4\tfunction\tl\t32267a31\tn\t3980\n",
            "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tfunction\tl\t3226\tn\t3980\n",
            "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tfunction\tl\t32267a31\tn\t398\n",
            "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tfunction\t\t32267a31\tn\t3980\n",
            "address\tkind\tlibrary\tfnid\tname\tstub\n0x00db40a4\tfunction\tl\t32267a31\tn\t3980\n0x00db40a4\tfunction\tl\t32267a32\tm\t3980\n",
        ] {
            assert!(parse_imports(bad, true, "t").is_err(), "{bad:?}");
        }
        assert!(parse_imports("address\tkind\tlibrary\tordinal\tname\n0x82000600\tvariable\tx\t6a\tn\n", false, "t").is_err());
    }

    #[test]
    fn translations_list_agreed_answers_and_a_contradiction_fails() {
        let ims = images();
        let file = read_functions(&functions_text("pc-v10", &sha("pc-v10"), &[(1, "a"), (2, "b"), (3, "c")]), "f").unwrap();
        // 1 -> 11 directly and via pc-v11; 2 -> 12 only via pc-v11; 3 has no pair.
        let g = Graph::new(&[
            pairs("pc-v10", "pc-v10-drmfree", &[(1, 11)]),
            pairs("pc-v10", "pc-v11", &[(1, 21), (2, 22)]),
            pairs("pc-v11", "pc-v10-drmfree", &[(21, 11), (22, 12)]),
        ])
        .unwrap();
        let a = annotate(&ims, "pc-v10", &file, &HashMap::new(), &g).unwrap();
        let r = records(&a.text);
        assert_eq!(r[0]["graph_images"], serde_json::json!(["pc-v10-drmfree", "pc-v11"]));
        assert!(!r[0]["untranslatable_images"].as_array().unwrap().iter().any(|v| v == "pc-v11" || v == "pc-v10"));
        assert!(r[0]["untranslatable_images"].as_array().unwrap().iter().any(|v| v == "x360-retail"));
        assert_eq!(
            r[1]["translations"],
            serde_json::json!([
                {"image": "pc-v10-drmfree", "va": "0x0000000b", "direct": true, "answering_paths": 2, "paths": 2},
                {"image": "pc-v11", "va": "0x00000015", "direct": true, "answering_paths": 2, "paths": 2},
            ])
        );
        assert_eq!(
            r[2]["translations"],
            serde_json::json!([
                {"image": "pc-v10-drmfree", "va": "0x0000000c", "direct": false, "answering_paths": 1, "paths": 2},
                {"image": "pc-v11", "va": "0x00000016", "direct": true, "answering_paths": 1, "paths": 2},
            ])
        );
        assert_eq!(r[3]["translations"], serde_json::json!([]));
        assert_eq!(a.translated["pc-v10-drmfree"], 2);
        // 1 -> 13 via pc-v11 contradicts the direct 11.
        let g = Graph::new(&[
            pairs("pc-v10", "pc-v10-drmfree", &[(1, 11)]),
            pairs("pc-v10", "pc-v11", &[(1, 21), (2, 22)]),
            pairs("pc-v11", "pc-v10-drmfree", &[(21, 13), (22, 12)]),
        ])
        .unwrap();
        let e = annotate(&ims, "pc-v10", &file, &HashMap::new(), &g).unwrap_err().to_string();
        assert!(e.contains("1 translations are contradicted"), "{e}");
        assert!(e.contains("pc-v10\t0x00000001\tpc-v10-drmfree\t0x0000000b(pc-v10>pc-v10-drmfree) 0x0000000d(pc-v10>pc-v11>pc-v10-drmfree)"), "{e}");
    }

    #[test]
    fn a_truncated_or_inconsistent_functions_file_is_refused() {
        let full = functions_text("pc-v10", &sha("pc-v10"), &[(1, "a"), (2, "b")]);
        assert_eq!(read_functions(&full, "f").unwrap().functions.len(), 2);
        let lines: Vec<&str> = full.lines().collect();
        // No end record: the export stopped early.
        let truncated = lines[..3].join("\n");
        let e = read_functions(&truncated, "f").unwrap_err().to_string();
        assert!(e.contains("no end record after 2 function records") && e.contains("truncated"), "{e}");
        let e = read_functions(&lines[..2].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("truncated"), "{e}");
        // One function fewer than declared, with the end record.
        let e = read_functions(&[lines[0], lines[1], lines[3]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("1 function records, the image record declares 2 and the end record 2"), "{e}");
        // A repeated va.
        let e = read_functions(&[lines[0], lines[1], lines[1], lines[3]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("line 3: function 0x00000001 repeats the one on line 2"), "{e}");
        // A record after the end, a first record that is not the image, a missing field.
        let e = read_functions(&[lines[0], lines[1], lines[2], lines[3], lines[1]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("after the end record"), "{e}");
        let e = read_functions(&lines[1..].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("not the image record"), "{e}");
        let no_thunk = lines[1].replace("\"thunk\":false,", "");
        let e = read_functions(&[lines[0], &no_thunk, lines[2], lines[3]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("no boolean \"thunk\""), "{e}");
        let no_size = lines[1].replace("\"size\":4,", "");
        let e = read_functions(&[lines[0], &no_size, lines[2], lines[3]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("no number \"size\""), "{e}");
        assert_eq!(read_functions(&full, "f").unwrap().functions[0].size, 4);
        let short_va = lines[1].replace("0x00000001", "0x1");
        let e = read_functions(&[lines[0], &short_va, lines[2], lines[3]].join("\n"), "f").unwrap_err().to_string();
        assert!(e.contains("eight lowercase hex digits"), "{e}");
        let e = read_functions("", "f").unwrap_err().to_string();
        assert!(e.contains("empty"), "{e}");
    }

    #[test]
    fn the_file_must_be_of_the_image_annotated() {
        let ims = images();
        let g = Graph::new(&[]).unwrap();
        let wrong_sha = read_functions(&functions_text("pc-v10", &sha("pc-v11"), &[(1, "a")]), "f").unwrap();
        let e = annotate(&ims, "pc-v10", &wrong_sha, &HashMap::new(), &g).unwrap_err().to_string();
        assert!(e.contains(&format!("program_sha256 is {}; images.tsv says image pc-v10 is {}", sha("pc-v11"), sha("pc-v10"))), "{e}");
        let wrong_id = read_functions(&functions_text("pc-v11", &sha("pc-v10"), &[(1, "a")]), "f").unwrap();
        let e = annotate(&ims, "pc-v10", &wrong_id, &HashMap::new(), &g).unwrap_err().to_string();
        assert!(e.contains("of image pc-v11, not pc-v10"), "{e}");
    }

    #[test]
    fn variants_are_listed_under_their_code_image_and_not_annotated_themselves() {
        let ims = images();
        let g = Graph::new(&[pairs("ps3-v100", "ps3-v103-na", &[(1, 2)])]).unwrap();
        let file = read_functions(&functions_text("ps3-v103-na", &sha("ps3-v103-na"), &[(2, "b")]), "f").unwrap();
        let a = annotate(&ims, "ps3-v103-na", &file, &HashMap::new(), &g).unwrap();
        let r = records(&a.text);
        assert_eq!(r[0]["variants"], serde_json::json!(["ps3-v103-de", "ps3-v103-eu"]));
        assert_eq!(r[0]["graph_images"], serde_json::json!(["ps3-v100"]));
        let untranslatable = r[0]["untranslatable_images"].as_array().unwrap();
        assert!(!untranslatable.iter().any(|v| v == "ps3-v103-de" || v == "ps3-v103-eu" || v == "ps3-v100"), "{untranslatable:?}");
        assert!(untranslatable.iter().any(|v| v == "ps3-v101-de"), "{untranslatable:?}");
        assert_eq!(r[1]["translations"][0]["va"], "0x00000001");
        let file = read_functions(&functions_text("ps3-v103-de", &sha("ps3-v103-de"), &[(2, "b")]), "f").unwrap();
        let e = annotate(&ims, "ps3-v103-de", &file, &HashMap::new(), &g).unwrap_err().to_string();
        assert!(e.contains("shares its code with ps3-v103-na"), "{e}");
    }
}
