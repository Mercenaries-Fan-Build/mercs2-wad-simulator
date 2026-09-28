//! `add_model`'s shader import: the vertex shaders a host primitive group names (its `INFO` words
//! `+0x0C` main and `+0x10` shadow) and the pixel shader each of its materials names (the `MTRL`
//! word after the texture hashes), resolved by the convention retail follows and written into the
//! lowered block.
//!
//! The convention is measured, not assumed: `data/shader_import_rules.tsv` is the census of every
//! retail model (`census_container` over each `model` container of `vz.wad`; the game-gated test
//! `shader_import_census` recomputes it and compares). Each rule maps an input to the set of shaders
//! retail uses for it:
//!
//! * `pixel`: a material's texture count and which slots are non-zero (`3:111`) → its pixel shader.
//! * `vertex`: the group's sub-object kind (`MESH`, `SKIN`, `TINY`), its vertex declaration's
//!   `usage.index` elements in order, and its materials' one pixel shader → its main vertex shader.
//! * `shadow`: the kind, the main vertex shader, and whether the materials' flags carry `0x08`
//!   (`alpha`) or none do (`opaque`) → its shadow vertex shader.
//!
//! An input with one choice resolves to it. An input with several, or none, has no answer in retail,
//! so the author declares the shader in the glTF: `extras.pixel_shader` on a material,
//! `extras.vertex_shader` / `extras.shadow_vertex_shader` on a mesh or primitive. A declared name
//! must be a registered shader of the right stage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::ucfx::{read_ucfx_rows, UcfxRow};

use crate::shader::{self, Added, Stage};

const RULES_TSV: &str = include_str!("../data/shader_import_rules.tsv");

/// Which rule an input belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    Pixel,
    Vertex,
    Shadow,
}

impl Rule {
    pub fn token(self) -> &'static str {
        match self {
            Rule::Pixel => "pixel",
            Rule::Vertex => "vertex",
            Rule::Shadow => "shadow",
        }
    }

    /// The glTF extras key that declares this rule's shader.
    pub fn extras_key(self) -> &'static str {
        match self {
            Rule::Pixel => "pixel_shader",
            Rule::Vertex => "vertex_shader",
            Rule::Shadow => "shadow_vertex_shader",
        }
    }

    fn parse(s: &str) -> Rule {
        match s {
            "pixel" => Rule::Pixel,
            "vertex" => Rule::Vertex,
            "shadow" => Rule::Shadow,
            other => panic!("shader_import_rules.tsv rule {other:?}"),
        }
    }
}

/// `rule → input → the shaders retail uses for it`.
pub type Tables = BTreeMap<Rule, BTreeMap<String, BTreeSet<String>>>;

/// The committed census.
pub fn rules() -> &'static Tables {
    static R: OnceLock<Tables> = OnceLock::new();
    R.get_or_init(|| {
        let mut lines = RULES_TSV.lines();
        let head = lines.next().unwrap_or_default();
        if head != "rule\tinput\tchoices" {
            panic!("shader_import_rules.tsv header {head:?}");
        }
        let mut out: Tables = BTreeMap::new();
        for l in lines {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() != 3 {
                panic!("shader_import_rules.tsv row {l:?}");
            }
            out.entry(Rule::parse(f[0]))
                .or_default()
                .insert(f[1].to_string(), f[2].split(',').map(str::to_string).collect());
        }
        out
    })
}

/// The rule table as the committed TSV spells it.
pub fn to_tsv(tables: &Tables) -> String {
    let mut s = String::from("rule\tinput\tchoices\n");
    for (rule, rows) in tables {
        for (input, choices) in rows {
            s.push_str(&format!(
                "{}\t{input}\t{}\n",
                rule.token(),
                choices.iter().cloned().collect::<Vec<_>>().join(",")
            ));
        }
    }
    s
}

/// A material's pixel rule input: `<tex_count>:<slot mask>`, `1` for a non-zero hash.
pub fn pixel_input(textures: &[u32]) -> String {
    let mask: String = textures.iter().map(|&h| if h == 0 { '0' } else { '1' }).collect();
    format!("{}:{mask}", textures.len())
}

/// A vertex declaration's elements as `usage.index`, in order, joined by `+`.
pub fn decl_usages(decl: &[u8]) -> String {
    let mut out = Vec::new();
    for e in decl.chunks_exact(8) {
        if u16::from_le_bytes([e[0], e[1]]) == 0xff {
            break;
        }
        out.push(format!("{}.{}", e[6], e[7]));
    }
    out.join("+")
}

pub fn vertex_input(kind: &str, usages: &str, pixel: &str) -> String {
    format!("{kind}|{usages}|{pixel}")
}

pub fn shadow_input(kind: &str, vertex: &str, alpha: bool) -> String {
    format!("{kind}|{vertex}|{}", if alpha { "alpha" } else { "opaque" })
}

/// Resolve one shader: the declared name when there is one, else the input's single retail choice.
/// An input with no choice or several is an error naming them, and naming the extras key that
/// declares one.
pub fn resolve(rule: Rule, input: &str, declared: Option<&str>) -> Result<String, String> {
    if let Some(d) = declared {
        return Ok(d.to_string());
    }
    match rules().get(&rule).and_then(|t| t.get(input)) {
        Some(c) if c.len() == 1 => Ok(c.iter().next().expect("one choice").clone()),
        Some(c) => Err(format!(
            "retail uses several {} shaders for {input}: {}. Declare one with `extras.{}` in the glTF",
            rule.token(),
            c.iter().cloned().collect::<Vec<_>>().join(", "),
            rule.extras_key()
        )),
        None => Err(format!(
            "no retail model has {} input {input}, so the convention names no shader for it. Declare \
             one with `extras.{}` in the glTF",
            rule.token(),
            rule.extras_key()
        )),
    }
}

// ── the model container ────────────────────────────────────────────────────────────────────────

/// A UCFX container's rows with the data area they index into.
struct Rows<'a> {
    buf: &'a [u8],
    rows: Vec<UcfxRow>,
    data_area: usize,
}

impl<'a> Rows<'a> {
    fn new(buf: &'a [u8]) -> Result<Rows<'a>, String> {
        let rows = read_ucfx_rows(buf)?;
        let data_area = u32::from_le_bytes(buf[4..8].try_into().expect("a UCFX header")) as usize;
        Ok(Rows { buf, rows, data_area })
    }

    fn is_marker(&self, i: usize) -> bool {
        self.rows[i].rel_off == 0xFFFF_FFFF
    }

    fn span(&self, i: usize) -> Result<(usize, usize), String> {
        let r = &self.rows[i];
        let s = self.data_area + r.rel_off as usize;
        let e = s + r.size as usize;
        if self.is_marker(i) || e > self.buf.len() {
            return Err(format!("row {i} ({}) has no body in the container", tag(&r.tag)));
        }
        Ok((s, e))
    }

    fn body(&self, i: usize) -> Result<&'a [u8], String> {
        let (s, e) = self.span(i)?;
        Ok(&self.buf[s..e])
    }

    /// Row `i`'s direct children.
    fn children(&self, i: usize) -> Vec<usize> {
        let end = i + 1 + self.rows[i].x3 as usize;
        let mut out = Vec::new();
        let mut j = i + 1;
        while j < end.min(self.rows.len()) {
            out.push(j);
            j += 1 + self.rows[j].x3 as usize;
        }
        out
    }

    /// The row whose subtree directly holds row `i`.
    fn parent(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&j| self.children(j).contains(&i))
    }

    fn child(&self, i: usize, t: &[u8; 4]) -> Option<usize> {
        self.children(i).into_iter().find(|&c| &self.rows[c].tag == t)
    }
}

fn tag(t: &[u8; 4]) -> String {
    String::from_utf8_lossy(t).into_owned()
}

/// One primitive group of a model container, as the import reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Ordinal among the container's `PRMG` rows.
    pub ordinal: usize,
    /// `MESH`, `SKIN` or `TINY`.
    pub kind: String,
    pub usages: String,
    /// Material indices of its `PRMT` records.
    pub materials: Vec<usize>,
    /// Absolute offset of the `INFO` body in the container.
    pub info_at: usize,
    pub vertex: u32,
    pub shadow: u32,
}

/// One material record, with the absolute offset of its pixel-shader key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub textures: Vec<u32>,
    pub flags: u16,
    pub key: u32,
    pub key_at: usize,
}

/// A model container's groups and materials.
pub fn read_model(container: &[u8]) -> Result<(Vec<Group>, Vec<Material>), String> {
    let r = Rows::new(container)?;
    let mats = parse_mtrl(container, MtrlSource::Model).map_err(|e| format!("MTRL: {e}"))?;
    let mut materials = Vec::new();
    if !mats.is_empty() {
        let top: Vec<usize> = {
            let mut t = Vec::new();
            let mut j = 0;
            while j < r.rows.len() {
                t.push(j);
                j += 1 + r.rows[j].x3 as usize;
            }
            t
        };
        let leaf = top
            .into_iter()
            .find(|&i| &r.rows[i].tag == b"MTRL" && !r.is_marker(i))
            .ok_or("the model has materials and no top-level MTRL")?;
        let (mut at, _) = r.span(leaf)?;
        for m in mats {
            let key_at = at + 108 + 4 * m.textures.len();
            at = key_at + 8;
            materials.push(Material { textures: m.textures, flags: m.flags, key: m.shader_key, key_at });
        }
    }
    let mut groups = Vec::new();
    let prmgs: Vec<usize> = (0..r.rows.len()).filter(|&i| &r.rows[i].tag == b"PRMG" && r.is_marker(i)).collect();
    for (ordinal, &p) in prmgs.iter().enumerate() {
        let kind = r.parent(p).map(|q| tag(&r.rows[q].tag)).unwrap_or_default();
        if !matches!(kind.as_str(), "MESH" | "SKIN" | "TINY") {
            continue;
        }
        let info = r.child(p, b"INFO").ok_or_else(|| format!("PRMG {ordinal} has no INFO"))?;
        let (info_at, info_end) = r.span(info)?;
        if info_end - info_at < 0x14 {
            return Err(format!("PRMG {ordinal} INFO is {} bytes, short of the vertex-shader words", info_end - info_at));
        }
        let word = |o: usize| u32::from_le_bytes(container[info_at + o..info_at + o + 4].try_into().expect("4 bytes"));
        let strm = r.child(p, b"STRM").ok_or_else(|| format!("PRMG {ordinal} has no STRM"))?;
        let decl = r.child(strm, b"decl").ok_or_else(|| format!("PRMG {ordinal} STRM has no decl"))?;
        let usages = decl_usages(r.body(decl)?);
        let materials_of = match r.child(p, b"PRMT") {
            Some(t) => r
                .body(t)?
                .chunks_exact(16)
                .map(|rec| u32::from_le_bytes(rec[0..4].try_into().expect("4 bytes")) as usize)
                .collect(),
            None => Vec::new(),
        };
        groups.push(Group {
            ordinal,
            kind,
            usages,
            materials: materials_of,
            info_at,
            vertex: word(0x0C),
            shadow: word(0x10),
        });
    }
    Ok((groups, materials))
}

/// The census of one retail model container: `(rule, input, the shader retail names)` for every
/// material, and for every group whose materials are all in the container and agree on the input.
pub fn census_container(container: &[u8]) -> Result<Vec<(Rule, String, String)>, String> {
    let (groups, materials) = read_model(container)?;
    let name = |key: u32| -> Result<String, String> {
        shader::retail_name(key).map(str::to_string).ok_or_else(|| format!("key 0x{key:08X} is not a retail registration"))
    };
    let mut out = Vec::new();
    for m in &materials {
        out.push((Rule::Pixel, pixel_input(&m.textures), name(m.key)?));
    }
    for g in &groups {
        let mats: Option<Vec<&Material>> = g.materials.iter().map(|&i| materials.get(i)).collect();
        let Some(mats) = mats.filter(|m| !m.is_empty()) else { continue };
        let vs = name(g.vertex)?;
        let pixels: BTreeSet<u32> = mats.iter().map(|m| m.key).collect();
        if let [one] = pixels.into_iter().collect::<Vec<_>>()[..] {
            out.push((Rule::Vertex, vertex_input(&g.kind, &g.usages, &name(one)?), vs.clone()));
        }
        let alpha: BTreeSet<bool> = mats.iter().map(|m| m.flags & 0x08 != 0).collect();
        if let [one] = alpha.into_iter().collect::<Vec<_>>()[..] {
            out.push((Rule::Shadow, shadow_input(&g.kind, &vs, one), name(g.shadow)?));
        }
    }
    Ok(out)
}

/// The shaders a glTF declares in `extras`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub pixel: Option<String>,
    pub vertex: Option<String>,
    pub shadow: Option<String>,
}

/// Read the shader `extras` of a `.glb` or `.gltf`: `pixel_shader` on materials, `vertex_shader` and
/// `shadow_vertex_shader` on meshes and primitives. The import writes one of each, so two different
/// declarations of one key are an error.
pub fn read_declared(path: &Path) -> Result<Declared, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: serde_json::Value = if bytes.starts_with(b"glTF") {
        if bytes.len() < 20 || &bytes[16..20] != b"JSON" {
            return Err(format!("{}: a GLB whose first chunk is not JSON", path.display()));
        }
        let len = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
        let chunk = bytes.get(20..20 + len).ok_or_else(|| format!("{}: the GLB JSON chunk runs past the file", path.display()))?;
        serde_json::from_slice(chunk).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?
    };
    let mut found: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut take = |extras: Option<&serde_json::Value>, key: &'static str| -> Result<(), String> {
        if let Some(v) = extras.and_then(|e| e.get(key)) {
            let s = v.as_str().ok_or_else(|| format!("extras.{key} is not a string"))?;
            found.entry(key).or_default().insert(s.to_string());
        }
        Ok(())
    };
    for m in json.get("materials").and_then(|v| v.as_array()).into_iter().flatten() {
        take(m.get("extras"), "pixel_shader")?;
    }
    for mesh in json.get("meshes").and_then(|v| v.as_array()).into_iter().flatten() {
        for key in ["vertex_shader", "shadow_vertex_shader"] {
            take(mesh.get("extras"), key)?;
        }
        for p in mesh.get("primitives").and_then(|v| v.as_array()).into_iter().flatten() {
            for key in ["vertex_shader", "shadow_vertex_shader"] {
                take(p.get("extras"), key)?;
            }
        }
    }
    let one = |key: &str| -> Result<Option<String>, String> {
        match found.get(key) {
            None => Ok(None),
            Some(s) if s.len() == 1 => Ok(s.iter().next().cloned()),
            Some(s) => Err(format!(
                "{}: extras.{key} is declared as {}; the model's host group takes one",
                path.display(),
                s.iter().cloned().collect::<Vec<_>>().join(", ")
            )),
        }
    };
    Ok(Declared { pixel: one("pixel_shader")?, vertex: one("vertex_shader")?, shadow: one("shadow_vertex_shader")? })
}

/// The inputs each of a manifest's added vertex shaders declares, in `shader` and `shader_low`,
/// keyed by registration key.
pub fn added_vertex_inputs(manifest: &crate::manifest::Manifest, root: &Path) -> Result<BTreeMap<u32, Vec<Vec<(u8, u8)>>>, String> {
    let mut out = BTreeMap::new();
    for c in &manifest.contributions {
        if let crate::manifest::Contribution::AddShader { family, classes } = c {
            if family.stage() != Stage::Vertex {
                continue;
            }
            for class in classes {
                let mut per_store = Vec::new();
                for src in [&class.shader, &class.shader_low] {
                    let code = shader::load_source(root, src)?;
                    per_store.push(mercs2_formats::shader3::vertex_inputs(&code.blob).map_err(|e| e.to_string())?);
                }
                out.insert(pandemic_hash_m2(&class.name), per_store);
            }
        }
    }
    Ok(out)
}

/// What the import wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub group: usize,
    pub vertex: String,
    pub shadow: String,
    /// `(material index, pixel shader)` for each host material.
    pub pixels: Vec<(usize, String)>,
}

/// A failed import, with the rule it is reported under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub code: &'static str,
    pub message: String,
}

fn err(code: &'static str, message: String) -> ImportError {
    ImportError { code, message }
}

/// The inputs a vertex shader declares, per store holding it: retail's from the committed table,
/// an added one's from its source bytecode (`added_inputs`).
fn vertex_inputs_of(name: &str, added_inputs: &BTreeMap<u32, Vec<Vec<(u8, u8)>>>) -> Option<Vec<Vec<(u8, u8)>>> {
    let key = pandemic_hash_m2(name);
    if let Some(i) = added_inputs.get(&key) {
        return Some(i.clone());
    }
    shader::registered().iter().find(|r| r.key == key && r.stage == Stage::Vertex).map(|r| r.inputs.iter().map(|(_, v)| v.clone()).collect())
}

/// Resolve and write the shaders of primitive group `host` of the model container inside `block`
/// (a single-entry block: `[count][name][type][field][size]` then the container): the group's
/// `INFO` vertex-shader words and each of its materials' pixel-shader key, then the container's
/// `CSUM`.
///
/// `added` are the Shipment's `add_shader` registrations and `added_inputs` the declared inputs of
/// its vertex shaders, one list per store ([`added_vertex_inputs`]). A declared or derived name must be registered in every configuration (M0235 for
/// a pixel shader, M0236 for a vertex shader); the main vertex shader's inputs must all be in the
/// group's declaration (M0238).
pub fn import_into_block(
    block: &mut [u8],
    host: usize,
    declared: &Declared,
    added: &[Added],
    added_inputs: &BTreeMap<u32, Vec<Vec<(u8, u8)>>>,
) -> Result<Imported, ImportError> {
    if block.len() < 20 {
        return Err(err("M0236", "the lowered block is shorter than its entry header".into()));
    }
    let size = u32::from_le_bytes(block[16..20].try_into().expect("4 bytes")) as usize;
    let container_end = 20 + size;
    if container_end > block.len() {
        return Err(err("M0236", "the lowered block's container runs past it".into()));
    }
    let (groups, materials) = read_model(&block[20..container_end]).map_err(|m| err("M0236", m))?;
    let g = groups
        .iter()
        .find(|g| g.ordinal == host)
        .ok_or_else(|| err("M0236", format!("primitive group {host} is not a MESH, SKIN or TINY group of the model")))?
        .clone();
    let keys = shader::ShaderKeys::with(added);
    let check = |rule: Rule, name: &str| -> Result<(), ImportError> {
        let (set, code, stage) = match rule {
            Rule::Pixel => (&keys.pixel, "M0235", "pixel"),
            Rule::Vertex | Rule::Shadow => (&keys.vertex, "M0236", "vertex"),
        };
        if set.contains(&pandemic_hash_m2(name)) {
            Ok(())
        } else {
            Err(err(code, format!("{name:?} is not a {stage} shader registered in every configuration")))
        }
    };

    let mut pixels = Vec::new();
    for &mi in &g.materials {
        let m = materials
            .get(mi)
            .ok_or_else(|| err("M0235", format!("group {host} names material {mi}; the model has {}", materials.len())))?;
        let ps = resolve(Rule::Pixel, &pixel_input(&m.textures), declared.pixel.as_deref())
            .map_err(|e| err("M0235", format!("material {mi}: {e}")))?;
        check(Rule::Pixel, &ps)?;
        if !pixels.iter().any(|(i, _)| *i == mi) {
            pixels.push((mi, ps));
        }
    }
    let distinct: BTreeSet<&str> = pixels.iter().map(|(_, p)| p.as_str()).collect();
    let pixel_for_vs = match distinct.into_iter().collect::<Vec<_>>()[..] {
        [one] => one.to_string(),
        ref several => {
            if declared.vertex.is_none() {
                return Err(err(
                    "M0236",
                    format!(
                        "group {host}'s materials resolve to several pixel shaders ({}), so no retail \
                         convention names its vertex shader. Declare one with `extras.vertex_shader`",
                        several.join(", ")
                    ),
                ));
            }
            String::new()
        }
    };
    let vertex = resolve(Rule::Vertex, &vertex_input(&g.kind, &g.usages, &pixel_for_vs), declared.vertex.as_deref())
        .map_err(|e| err("M0236", format!("group {host}: {e}")))?;
    check(Rule::Vertex, &vertex)?;
    let alpha: BTreeSet<bool> = g.materials.iter().filter_map(|&i| materials.get(i)).map(|m| m.flags & 0x08 != 0).collect();
    let shadow = match alpha.into_iter().collect::<Vec<_>>()[..] {
        [one] => resolve(Rule::Shadow, &shadow_input(&g.kind, &vertex, one), declared.shadow.as_deref()),
        _ => match &declared.shadow {
            Some(d) => Ok(d.clone()),
            None => Err(format!(
                "group {host}'s materials disagree on flag 0x08 (alpha), so no retail convention names \
                 its shadow vertex shader. Declare one with `extras.shadow_vertex_shader`"
            )),
        },
    }
    .map_err(|e| err("M0236", format!("group {host}: {e}")))?;
    check(Rule::Shadow, &shadow)?;

    // M0238: every input the main vertex shader declares, in every store holding it, is an element
    // of the group's declaration.
    let supplied: BTreeSet<&str> = g.usages.split('+').collect();
    let inputs = vertex_inputs_of(&vertex, added_inputs)
        .ok_or_else(|| err("M0238", format!("{vertex:?} has no bytecode to read its inputs from")))?;
    for per_store in inputs {
        let missing: Vec<String> = per_store
            .iter()
            .map(|(u, i)| format!("{u}.{i}"))
            .filter(|ui| !supplied.contains(ui.as_str()))
            .collect();
        if !missing.is_empty() {
            return Err(err(
                "M0238",
                format!(
                    "{vertex:?} reads input(s) {} (usage.index) that group {host}'s vertex declaration \
                     ({}) does not supply",
                    missing.join(", "),
                    g.usages
                ),
            ));
        }
    }

    let put = |b: &mut [u8], at: usize, v: u32| b[20 + at..20 + at + 4].copy_from_slice(&v.to_le_bytes());
    put(block, g.info_at + 0x0C, pandemic_hash_m2(&vertex));
    put(block, g.info_at + 0x10, pandemic_hash_m2(&shadow));
    for (mi, ps) in &pixels {
        put(block, materials[*mi].key_at, pandemic_hash_m2(ps));
    }
    let container = &mut block[20..container_end];
    let csum_at = container.len() - 8;
    if &container[csum_at..csum_at + 4] != b"CSUM" {
        return Err(err("M0236", "the lowered container does not end in a CSUM trailer".into()));
    }
    let crc = mercs2_formats::crc32::crc32_mercs2(&container[..csum_at]);
    container[csum_at + 4..csum_at + 8].copy_from_slice(&crc.to_le_bytes());
    Ok(Imported { group: host, vertex, shadow, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_spelled_as_the_table_keys_them() {
        assert_eq!(pixel_input(&[1, 0, 3]), "3:101");
        let decl = [
            0u8, 0, 0, 0, 16, 0, 0, 0, // POSITION
            0, 0, 8, 0, 15, 0, 5, 0, // TEXCOORD0
            0, 0, 12, 0, 16, 0, 3, 0, // NORMAL
            0xff, 0, 0, 0, 17, 0, 0, 0,
        ];
        assert_eq!(decl_usages(&decl), "0.0+5.0+3.0");
        assert_eq!(vertex_input("MESH", "0.0+5.0+3.0", "PgDiffFP"), "MESH|0.0+5.0+3.0|PgDiffFP");
        assert_eq!(shadow_input("MESH", "PgMeshVP", true), "MESH|PgMeshVP|alpha");
    }

    #[test]
    fn a_single_choice_resolves_and_several_ask_for_a_declaration() {
        assert_eq!(resolve(Rule::Pixel, "3:011", None).unwrap(), "PgDiffRefractNormFP");
        let several = resolve(Rule::Pixel, "3:111", None).unwrap_err();
        assert!(several.contains("PgDiffSpecNormFP") && several.contains("extras.pixel_shader"), "{several}");
        let none = resolve(Rule::Pixel, "7:1111111", None).unwrap_err();
        assert!(none.contains("no retail model"), "{none}");
        assert_eq!(resolve(Rule::Pixel, "3:111", Some("PgDiffSpecNormFP")).unwrap(), "PgDiffSpecNormFP");
    }

    #[test]
    fn the_vertex_convention_follows_the_declaration_and_the_pixel_shader() {
        assert_eq!(
            resolve(Rule::Vertex, &vertex_input("MESH", "0.0+5.0+3.0+6.0", "PgDiffSpecNormFP"), None).unwrap(),
            "PgMeshNoColorVP"
        );
        assert_eq!(
            resolve(Rule::Shadow, &shadow_input("MESH", "PgMeshNoColorVP", true), None).unwrap(),
            "PgMeshTexShadowVP"
        );
    }
}
