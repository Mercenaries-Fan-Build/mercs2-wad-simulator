//! The build catalogue: `data/builds.tsv`, `data/layouts.tsv`, `data/overlays.tsv` and
//! `data/aliases.tsv`, parsed and cross-checked.
//!
//! Every table is tab-separated with one header line that must match exactly, rows sorted, and no
//! blank lines. Integers are `0x`-prefixed hex except `size`, `runs` and `diff_bytes`, which are
//! decimal. Any row that does not parse, any reference to an id that does not exist, and any
//! ordering violation is an [`Error`] naming the table and line.

use std::collections::{BTreeMap, BTreeSet};

use crate::Error;

const BUILDS_HEADER: &str =
    "build_id\tsha256\tmd5\tsize\tpe_timestamp\tlayout\tcode_version\trecipe_from\trecipe_step\toverlay_set";
const LAYOUTS_HEADER: &str = "layout_id\tpe_timestamp\treference_sha256\timage_base\tsection_alignment\t\
size_of_headers\tsection\trva\tvirtual_size\traw_offset\traw_size";
const OVERLAYS_HEADER: &str =
    "overlay_set\tsection\tspace\tstart\tend\tkind\truns\tdiff_bytes\tsha256\tdetail";
const ALIASES_HEADER: &str = "alias\tbuild_id\treason";

/// The game code a build runs. Every build of a layout runs the same code version: `U` is 1.0 and
/// `E` is 1.1, proven by the official v1.1 patcher turning `v10-unsigned` (U) into
/// `v11-official-securom` (E).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeVersion {
    /// The game as released, version 1.0.
    V1_0,
    /// The game after the official v1.1 patch.
    V1_1,
}

impl CodeVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            CodeVersion::V1_0 => "1.0",
            CodeVersion::V1_1 => "1.1",
        }
    }
    fn parse(s: &str) -> Option<CodeVersion> {
        match s {
            "1.0" => Some(CodeVersion::V1_0),
            "1.1" => Some(CodeVersion::V1_1),
            _ => None,
        }
    }
}

/// One way to produce a build from another catalogued build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// sha256 of the input build.
    pub from_sha256: String,
    /// The transform, named where it lives (a patch file, a crate function, or `manual`).
    pub step: String,
}

/// One row of `builds.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub id: String,
    pub sha256: String,
    pub md5: String,
    pub size: u64,
    pub pe_timestamp: u32,
    pub layout: String,
    /// The code version the build runs, the same for every build of its layout.
    pub code_version: CodeVersion,
    pub recipes: Vec<Recipe>,
    /// The overlay set holding this build's differences from its layout reference. `None` for a
    /// layout reference itself, which differs from itself nowhere.
    pub overlay_set: Option<String>,
}

/// One section of a layout's reference build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutSection {
    pub name: String,
    pub rva: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
}

/// A code layout: the PE timestamp it was compiled with and the build every member is compared
/// against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub id: String,
    pub pe_timestamp: u32,
    pub reference_sha256: String,
    pub image_base: u32,
    pub section_alignment: u32,
    pub size_of_headers: u32,
    pub sections: Vec<LayoutSection>,
}

/// The address space an overlay's `start`/`end` are in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Space {
    /// Absolute virtual addresses (`ImageBase + RVA`) in the mapped image.
    Va,
    /// File offsets, for bytes no header or section covers.
    File,
}

impl Space {
    pub fn as_str(self) -> &'static str {
        match self {
            Space::Va => "va",
            Space::File => "file",
        }
    }
    fn parse(s: &str) -> Option<Space> {
        match s {
            "va" => Some(Space::Va),
            "file" => Some(Space::File),
            _ => None,
        }
    }
}

/// What an overlay is. A closed set; each kind's meaning is stated on its variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Bytes inside the PE headers. `detail` names the fields.
    Header,
    /// An imported DLL's name string.
    ImportName,
    /// Reference bytes written by the bypass patch that the build does not carry.
    BypassStub,
    /// Slots inside the import address table range `0x705000+0x69c`.
    Iat,
    /// A difference between a SecuROM-wrapped retail file and its layout reference, an unwrapped file
    /// of the same code version, whose role in the code is not established.
    Lineage,
    /// A difference in a build taken from, or built from, a running process image.
    Runtime,
    /// A 5-byte `jmp rel32` written over the start of a `.text` function.
    HotPatch,
    /// Bytes changed inside the `reloaded` section.
    ReloadedPatch,
    /// A range the reference maps and the build does not. Carries no bytes.
    Unmapped,
    /// The Authenticode blob named by `DataDirectory[SECURITY]`.
    Certificate,
    /// Zero bytes after the last section's raw data.
    FilePadding,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Header => "header",
            Kind::ImportName => "import-name",
            Kind::BypassStub => "bypass-stub",
            Kind::Iat => "iat",
            Kind::Lineage => "lineage",
            Kind::Runtime => "runtime",
            Kind::HotPatch => "hot-patch",
            Kind::ReloadedPatch => "reloaded-patch",
            Kind::Unmapped => "unmapped",
            Kind::Certificate => "certificate",
            Kind::FilePadding => "file-padding",
        }
    }
    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "header" => Kind::Header,
            "import-name" => Kind::ImportName,
            "bypass-stub" => Kind::BypassStub,
            "iat" => Kind::Iat,
            "lineage" => Kind::Lineage,
            "runtime" => Kind::Runtime,
            "hot-patch" => Kind::HotPatch,
            "reloaded-patch" => Kind::ReloadedPatch,
            "unmapped" => Kind::Unmapped,
            "certificate" => Kind::Certificate,
            "file-padding" => Kind::FilePadding,
            _ => return None,
        })
    }
}

/// One row of `overlays.tsv`: a range of a build that differs from its layout reference.
///
/// `start`/`end` are tight: `start` is the first differing byte and `end` is one past the last.
/// `runs` counts the maximal runs of differing bytes inside the range and `diff_bytes` sums their
/// lengths. `sha256` is the digest of the build's bytes over the whole range — every byte, the
/// matching ones between runs included — and is `None` only for [`Kind::Unmapped`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub overlay_set: String,
    pub section: String,
    pub space: Space,
    pub start: u64,
    pub end: u64,
    pub kind: Kind,
    pub runs: u64,
    pub diff_bytes: u64,
    pub sha256: Option<String>,
    pub detail: String,
}

/// One row of `aliases.tsv`: another name for a build, accepted wherever a build id is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub alias: String,
    /// The canonical id the alias resolves to, a `builds.tsv` row.
    pub build_id: String,
    /// Why the alias is not the canonical id.
    pub reason: String,
}

/// The four tables, parsed and cross-checked.
#[derive(Clone, Debug)]
pub struct Catalogue {
    pub builds: Vec<Build>,
    pub layouts: Vec<Layout>,
    pub overlays: Vec<Overlay>,
    pub aliases: Vec<Alias>,
}

fn rows<'t>(table: &str, text: &'t str, header: &str) -> Result<Vec<(usize, Vec<&'t str>)>, Error> {
    let mut lines = text.lines();
    let first = lines.next().ok_or_else(|| Error::new(format!("{table} is empty")))?;
    if first != header {
        return Err(Error::new(format!("{table}: header line is {first:?}, expected {header:?}")));
    }
    let width = header.split('\t').count();
    let mut out = Vec::new();
    for (i, line) in lines.enumerate() {
        let n = i + 2;
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != width {
            return Err(Error::new(format!(
                "{table} line {n}: {} fields, expected {width}",
                fields.len()
            )));
        }
        out.push((n, fields));
    }
    Ok(out)
}

fn hex(table: &str, n: usize, col: &str, s: &str) -> Result<u64, Error> {
    let digits = s
        .strip_prefix("0x")
        .ok_or_else(|| Error::new(format!("{table} line {n}: {col} {s:?} is not 0x-prefixed hex")))?;
    u64::from_str_radix(digits, 16)
        .map_err(|e| Error::new(format!("{table} line {n}: {col} {s:?}: {e}")))
}

fn hex32(table: &str, n: usize, col: &str, s: &str) -> Result<u32, Error> {
    let v = hex(table, n, col, s)?;
    u32::try_from(v).map_err(|_| Error::new(format!("{table} line {n}: {col} {s} exceeds 32 bits")))
}

fn dec(table: &str, n: usize, col: &str, s: &str) -> Result<u64, Error> {
    s.parse::<u64>()
        .map_err(|e| Error::new(format!("{table} line {n}: {col} {s:?}: {e}")))
}

fn digest(table: &str, n: usize, col: &str, s: &str, len: usize) -> Result<String, Error> {
    if s.len() != len || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(Error::new(format!("{table} line {n}: {col} {s:?} is not {len} lowercase hex digits")));
    }
    Ok(s.to_string())
}

fn nonempty<'t>(table: &str, n: usize, col: &str, s: &'t str) -> Result<&'t str, Error> {
    if s.is_empty() {
        return Err(Error::new(format!("{table} line {n}: {col} is empty")));
    }
    Ok(s)
}

impl Catalogue {
    /// The tables compiled into this crate.
    pub fn embedded() -> Result<Catalogue, Error> {
        Catalogue::parse(
            include_str!("../data/builds.tsv"),
            include_str!("../data/layouts.tsv"),
            include_str!("../data/overlays.tsv"),
            include_str!("../data/aliases.tsv"),
        )
    }

    /// Parse and cross-check four table texts.
    pub fn parse(builds: &str, layouts: &str, overlays: &str, aliases: &str) -> Result<Catalogue, Error> {
        let builds = parse_builds(builds)?;
        let layouts = parse_layouts(layouts)?;
        let overlays = parse_overlays(overlays)?;
        let aliases = parse_aliases(aliases)?;
        let c = Catalogue { builds, layouts, overlays, aliases };
        c.cross_check()?;
        Ok(c)
    }

    /// The build with this sha256 (lowercase hex).
    pub fn build_by_sha(&self, sha256: &str) -> Option<&Build> {
        self.builds.iter().find(|b| b.sha256 == sha256)
    }

    /// The build with this id or alias.
    pub fn build(&self, id: &str) -> Option<&Build> {
        let canonical = self.canonical_id(id)?;
        self.builds.iter().find(|b| b.id == canonical)
    }

    /// The canonical build id that `id` names: `id` itself when it is a `builds.tsv` id, the
    /// alias's build id when it is an `aliases.tsv` alias, `None` otherwise.
    pub fn canonical_id<'a>(&'a self, id: &'a str) -> Option<&'a str> {
        if self.builds.iter().any(|b| b.id == id) {
            return Some(id);
        }
        self.aliases.iter().find(|a| a.alias == id).map(|a| a.build_id.as_str())
    }

    /// The layout with this id.
    pub fn layout(&self, id: &str) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.id == id)
    }

    /// The rows of one overlay set, in table order.
    pub fn overlay_set(&self, set: &str) -> Vec<&Overlay> {
        self.overlays.iter().filter(|o| o.overlay_set == set).collect()
    }

    fn cross_check(&self) -> Result<(), Error> {
        let shas: BTreeSet<&str> = self.builds.iter().map(|b| b.sha256.as_str()).collect();
        if shas.len() != self.builds.len() {
            return Err(Error::new("builds.tsv lists one sha256 under two build ids"));
        }
        let sets: BTreeSet<&str> = self.overlays.iter().map(|o| o.overlay_set.as_str()).collect();
        let mut referenced = BTreeSet::new();
        for b in &self.builds {
            let layout = self.layout(&b.layout).ok_or_else(|| {
                Error::new(format!("build {} names layout {}, which layouts.tsv does not define", b.id, b.layout))
            })?;
            if layout.pe_timestamp != b.pe_timestamp {
                return Err(Error::new(format!(
                    "build {} has PE timestamp 0x{:08x}, its layout {} has 0x{:08x}",
                    b.id, b.pe_timestamp, layout.id, layout.pe_timestamp
                )));
            }
            let is_reference = layout.reference_sha256 == b.sha256;
            match (&b.overlay_set, is_reference) {
                (None, true) => {}
                (None, false) => {
                    return Err(Error::new(format!(
                        "build {} is not its layout's reference and names no overlay set",
                        b.id
                    )))
                }
                (Some(_), true) => {
                    return Err(Error::new(format!("build {} is a layout reference and names an overlay set", b.id)))
                }
                (Some(s), false) => {
                    if !sets.contains(s.as_str()) {
                        return Err(Error::new(format!("build {} names overlay set {s}, which has no rows", b.id)));
                    }
                    if !referenced.insert(s.as_str()) {
                        return Err(Error::new(format!("overlay set {s} is named by two builds")));
                    }
                }
            }
            for r in &b.recipes {
                if !shas.contains(r.from_sha256.as_str()) {
                    return Err(Error::new(format!(
                        "build {} has a recipe from sha256 {}, which builds.tsv does not list",
                        b.id, r.from_sha256
                    )));
                }
            }
        }
        for l in &self.layouts {
            if !shas.contains(l.reference_sha256.as_str()) {
                return Err(Error::new(format!(
                    "layout {} names reference sha256 {}, which builds.tsv does not list",
                    l.id, l.reference_sha256
                )));
            }
        }
        if let Some(orphan) = sets.iter().find(|s| !referenced.contains(*s)) {
            return Err(Error::new(format!("overlays.tsv has rows for set {orphan}, which no build names")));
        }
        for b in &self.builds {
            let layout = self.layout(&b.layout).expect("checked above");
            let reference = self.build_by_sha(&layout.reference_sha256).expect("checked above");
            if b.code_version != reference.code_version {
                return Err(Error::new(format!(
                    "build {} has code version {}, its layout {} reference {} has {}",
                    b.id,
                    b.code_version.as_str(),
                    layout.id,
                    reference.id,
                    reference.code_version.as_str()
                )));
            }
        }
        for a in &self.aliases {
            if self.builds.iter().any(|b| b.id == a.alias) {
                return Err(Error::new(format!("aliases.tsv: alias {} is a builds.tsv build id", a.alias)));
            }
            if !self.builds.iter().any(|b| b.id == a.build_id) {
                return Err(Error::new(format!(
                    "aliases.tsv: alias {} names build {}, which builds.tsv does not list",
                    a.alias, a.build_id
                )));
            }
        }
        Ok(())
    }
}

fn parse_builds(text: &str) -> Result<Vec<Build>, Error> {
    const T: &str = "builds.tsv";
    let mut out: Vec<Build> = Vec::new();
    for (n, f) in rows(T, text, BUILDS_HEADER)? {
        let id = nonempty(T, n, "build_id", f[0])?.to_string();
        if let Some(prev) = out.last() {
            if prev.id >= id {
                return Err(Error::new(format!("{T} line {n}: build_id {id} is not after {}", prev.id)));
            }
        }
        let code_version = CodeVersion::parse(f[6]).ok_or_else(|| {
            Error::new(format!("{T} line {n}: code_version {:?} is not 1.0 or 1.1", f[6]))
        })?;
        let froms: Vec<&str> = if f[7].is_empty() { Vec::new() } else { f[7].split(';').collect() };
        let steps: Vec<&str> = if f[8].is_empty() { Vec::new() } else { f[8].split(';').collect() };
        if froms.len() != steps.len() {
            return Err(Error::new(format!(
                "{T} line {n}: {} recipe_from entries but {} recipe_step entries",
                froms.len(),
                steps.len()
            )));
        }
        let mut recipes = Vec::new();
        for (from, step) in froms.into_iter().zip(steps) {
            recipes.push(Recipe {
                from_sha256: digest(T, n, "recipe_from", from, 64)?,
                step: nonempty(T, n, "recipe_step", step)?.to_string(),
            });
        }
        out.push(Build {
            id,
            sha256: digest(T, n, "sha256", f[1], 64)?,
            md5: digest(T, n, "md5", f[2], 32)?,
            size: dec(T, n, "size", f[3])?,
            pe_timestamp: hex32(T, n, "pe_timestamp", f[4])?,
            layout: nonempty(T, n, "layout", f[5])?.to_string(),
            code_version,
            recipes,
            overlay_set: if f[9].is_empty() { None } else { Some(f[9].to_string()) },
        });
    }
    Ok(out)
}

fn parse_layouts(text: &str) -> Result<Vec<Layout>, Error> {
    const T: &str = "layouts.tsv";
    let mut out: Vec<Layout> = Vec::new();
    for (n, f) in rows(T, text, LAYOUTS_HEADER)? {
        let id = nonempty(T, n, "layout_id", f[0])?;
        let head = Layout {
            id: id.to_string(),
            pe_timestamp: hex32(T, n, "pe_timestamp", f[1])?,
            reference_sha256: digest(T, n, "reference_sha256", f[2], 64)?,
            image_base: hex32(T, n, "image_base", f[3])?,
            section_alignment: hex32(T, n, "section_alignment", f[4])?,
            size_of_headers: hex32(T, n, "size_of_headers", f[5])?,
            sections: Vec::new(),
        };
        let section = LayoutSection {
            name: nonempty(T, n, "section", f[6])?.to_string(),
            rva: hex32(T, n, "rva", f[7])?,
            virtual_size: hex32(T, n, "virtual_size", f[8])?,
            raw_offset: hex32(T, n, "raw_offset", f[9])?,
            raw_size: hex32(T, n, "raw_size", f[10])?,
        };
        match out.last_mut() {
            Some(l) if l.id == id => {
                let mut cmp = head.clone();
                cmp.sections = l.sections.clone();
                if cmp != *l {
                    return Err(Error::new(format!("{T} line {n}: layout {id} fields differ from its earlier rows")));
                }
                let prev = l.sections.last().expect("a layout row carries a section");
                if prev.rva >= section.rva {
                    return Err(Error::new(format!("{T} line {n}: section rva 0x{:x} is not after 0x{:x}", section.rva, prev.rva)));
                }
                l.sections.push(section);
            }
            last => {
                if let Some(l) = last {
                    if l.id >= head.id {
                        return Err(Error::new(format!("{T} line {n}: layout {id} is not after {}", l.id)));
                    }
                }
                let mut l = head;
                l.sections.push(section);
                out.push(l);
            }
        }
    }
    Ok(out)
}

fn parse_overlays(text: &str) -> Result<Vec<Overlay>, Error> {
    const T: &str = "overlays.tsv";
    let mut out: Vec<Overlay> = Vec::new();
    let mut seen_sets: BTreeMap<String, usize> = BTreeMap::new();
    for (n, f) in rows(T, text, OVERLAYS_HEADER)? {
        let space = Space::parse(f[2]).ok_or_else(|| Error::new(format!("{T} line {n}: space {:?} is not va or file", f[2])))?;
        let kind = Kind::parse(f[5]).ok_or_else(|| Error::new(format!("{T} line {n}: unknown kind {:?}", f[5])))?;
        let o = Overlay {
            overlay_set: nonempty(T, n, "overlay_set", f[0])?.to_string(),
            section: nonempty(T, n, "section", f[1])?.to_string(),
            space,
            start: hex(T, n, "start", f[3])?,
            end: hex(T, n, "end", f[4])?,
            kind,
            runs: dec(T, n, "runs", f[6])?,
            diff_bytes: dec(T, n, "diff_bytes", f[7])?,
            sha256: if f[8] == "-" { None } else { Some(digest(T, n, "sha256", f[8], 64)?) },
            detail: f[9].to_string(),
        };
        if o.start >= o.end {
            return Err(Error::new(format!("{T} line {n}: empty range 0x{:x}..0x{:x}", o.start, o.end)));
        }
        if o.runs == 0 || o.diff_bytes < o.runs || o.diff_bytes > o.end - o.start {
            return Err(Error::new(format!("{T} line {n}: runs {} / diff_bytes {} do not fit the range", o.runs, o.diff_bytes)));
        }
        if (o.kind == Kind::Unmapped) != o.sha256.is_none() {
            return Err(Error::new(format!("{T} line {n}: sha256 is \"-\" exactly when kind is unmapped")));
        }
        if let Some(prev) = out.last() {
            let key = |x: &Overlay| (x.overlay_set.clone(), x.space, x.start);
            if key(prev) >= key(&o) {
                return Err(Error::new(format!("{T} line {n}: row is not after the previous one (set, space, start)")));
            }
            if prev.overlay_set == o.overlay_set && prev.space == o.space && prev.end > o.start {
                return Err(Error::new(format!("{T} line {n}: range overlaps the previous row")));
            }
            if prev.overlay_set != o.overlay_set && seen_sets.contains_key(&o.overlay_set) {
                return Err(Error::new(format!("{T} line {n}: set {} is split across the table", o.overlay_set)));
            }
        }
        seen_sets.entry(o.overlay_set.clone()).or_insert(n);
        out.push(o);
    }
    Ok(out)
}

fn parse_aliases(text: &str) -> Result<Vec<Alias>, Error> {
    const T: &str = "aliases.tsv";
    let mut out: Vec<Alias> = Vec::new();
    for (n, f) in rows(T, text, ALIASES_HEADER)? {
        let a = Alias {
            alias: nonempty(T, n, "alias", f[0])?.to_string(),
            build_id: nonempty(T, n, "build_id", f[1])?.to_string(),
            reason: nonempty(T, n, "reason", f[2])?.to_string(),
        };
        if let Some(prev) = out.last() {
            if prev.alias >= a.alias {
                return Err(Error::new(format!("{T} line {n}: alias {} is not after {}", a.alias, prev.alias)));
            }
        }
        out.push(a);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_tables_parse_and_cross_check() {
        let c = Catalogue::embedded().unwrap();
        assert!(!c.builds.is_empty());
        assert_eq!(c.layouts.len(), 2);
        for l in &c.layouts {
            let r = c.build_by_sha(&l.reference_sha256).unwrap();
            assert_eq!(r.overlay_set, None);
        }
    }

    #[test]
    fn every_alias_resolves_to_its_build_and_no_alias_is_a_build_id() {
        let c = Catalogue::embedded().unwrap();
        assert!(!c.aliases.is_empty());
        for a in &c.aliases {
            assert!(c.builds.iter().all(|b| b.id != a.alias), "alias {} is a build id", a.alias);
            assert_eq!(c.canonical_id(&a.alias), Some(a.build_id.as_str()), "{}", a.alias);
            assert_eq!(c.build(&a.alias).map(|b| b.id.as_str()), Some(a.build_id.as_str()), "{}", a.alias);
        }
        for b in &c.builds {
            assert_eq!(c.canonical_id(&b.id), Some(b.id.as_str()));
        }
        assert_eq!(c.canonical_id("not-a-build"), None);
    }

    #[test]
    fn code_version_agrees_with_layout() {
        let c = Catalogue::embedded().unwrap();
        for b in &c.builds {
            let want = match b.layout.as_str() {
                "U" => CodeVersion::V1_0,
                "E" => CodeVersion::V1_1,
                other => panic!("build {} has layout {other}, which this test does not map to a code version", b.id),
            };
            assert_eq!(b.code_version, want, "{}", b.id);
        }
    }

    fn one_build_tables(code_version: &str) -> (String, String) {
        let sha = "a".repeat(64);
        let builds = format!("{BUILDS_HEADER}\nb\t{sha}\t{}\t1\t0x00000011\tL\t{code_version}\t\t\t\n", "b".repeat(32));
        let layouts = format!(
            "{LAYOUTS_HEADER}\nL\t0x00000011\t{sha}\t0x00400000\t0x1000\t0x200\t.text\t0x00001000\t0x00000100\t0x00000200\t0x00000100\n"
        );
        (builds, layouts)
    }

    #[test]
    fn an_alias_that_is_a_build_id_or_names_no_build_is_rejected() {
        let (builds, layouts) = one_build_tables("1.0");
        let ok = Catalogue::parse(&builds, &layouts, &format!("{OVERLAYS_HEADER}\n"), &format!("{ALIASES_HEADER}\nold\tb\twhy\n"))
            .unwrap();
        assert_eq!(ok.build("old").map(|b| b.id.as_str()), Some("b"));
        let e = Catalogue::parse(&builds, &layouts, &format!("{OVERLAYS_HEADER}\n"), &format!("{ALIASES_HEADER}\nb\tb\twhy\n"))
            .unwrap_err();
        assert!(e.to_string().contains("is a builds.tsv build id"), "{e}");
        let e = Catalogue::parse(&builds, &layouts, &format!("{OVERLAYS_HEADER}\n"), &format!("{ALIASES_HEADER}\nold\tnone\twhy\n"))
            .unwrap_err();
        assert!(e.to_string().contains("does not list"), "{e}");
    }

    #[test]
    fn an_unknown_code_version_is_rejected() {
        let (builds, layouts) = one_build_tables("1.2");
        let e = Catalogue::parse(&builds, &layouts, &format!("{OVERLAYS_HEADER}\n"), &format!("{ALIASES_HEADER}\n")).unwrap_err();
        assert!(e.to_string().contains("code_version \"1.2\""), "{e}");
    }

    #[test]
    fn a_wrong_header_or_width_is_rejected() {
        let e = parse_builds("build_id\tsha256\n").unwrap_err();
        assert!(e.to_string().contains("header line"), "{e}");
        let e = parse_builds(&format!("{BUILDS_HEADER}\nonly\ttwo\n")).unwrap_err();
        assert!(e.to_string().contains("2 fields"), "{e}");
    }

    #[test]
    fn unknown_kinds_and_unsorted_rows_are_rejected() {
        let row = |set: &str, start: &str, kind: &str| {
            format!("{set}\t.text\tva\t{start}\t0x00001010\t{kind}\t1\t1\t{}\t\n", "a".repeat(64))
        };
        let e = parse_overlays(&format!("{OVERLAYS_HEADER}\n{}", row("s", "0x00001000", "mystery"))).unwrap_err();
        assert!(e.to_string().contains("unknown kind"), "{e}");
        let e = parse_overlays(&format!(
            "{OVERLAYS_HEADER}\n{}{}",
            row("s", "0x00001008", "lineage"),
            row("s", "0x00001000", "lineage")
        ))
        .unwrap_err();
        assert!(e.to_string().contains("not after"), "{e}");
    }
}
