//! The retail shader registrations: every registration `FUN_0084f130` and its sub-registrars make.
//!
//! `data/registered_shaders.tsv` is written by `mercs2_quartermaster/tools/extract_shader_registry.py`
//! from the unpacked exe and the retail stores: one row per registration, with the configurations
//! (resident store pair × ShaderLevel) it is made in, plus `PgCompositeFP`, which the composite pass
//! constructor registers outside `FUN_0084f130` (site `0x0246A443`; its class argument comes from a
//! SecuROM call, so the row's class is `-`). A vertex shader's row also lists, per store holding its
//! record, the inputs and the CTAB constants the record declares.
//!
//! A material names its pixel shader by key (`pandemic_hash_m2` of the registration name); the row
//! gives the `.sho` file whose store record holds the bytecode.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::hash::pandemic_hash_m2;
use crate::shader_registry::Stage;

const REGISTERED_TSV: &str = include_str!("../data/registered_shaders.tsv");

/// A resident configuration of the registry: which extra store pair the caps word loads
/// (`DAT_01176288 + 0x5e4` bit 2: the VT pair; bit 2 clear and bit 3 set: the R2VB pair; neither:
/// none) and the ShaderLevel byte `DAT_00dfc345`, which gates the `_pl`/`_sl`/`_pl_sl`
/// registrations and selects the `_3.sho` or `_3l.sho` store records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Config {
    pub extra: ExtraStores,
    pub shader_level: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtraStores {
    None,
    R2vb,
    Vt,
}

impl Config {
    pub const ALL: [Config; 6] = [
        Config { extra: ExtraStores::None, shader_level: false },
        Config { extra: ExtraStores::None, shader_level: true },
        Config { extra: ExtraStores::R2vb, shader_level: false },
        Config { extra: ExtraStores::R2vb, shader_level: true },
        Config { extra: ExtraStores::Vt, shader_level: false },
        Config { extra: ExtraStores::Vt, shader_level: true },
    ];

    /// The `registered_shaders.tsv` spelling: `none0` … `vt1`.
    pub fn token(self) -> String {
        let e = match self.extra {
            ExtraStores::None => "none",
            ExtraStores::R2vb => "r2vb",
            ExtraStores::Vt => "vt",
        };
        format!("{e}{}", u8::from(self.shader_level))
    }

    fn parse(s: &str) -> Config {
        *Config::ALL
            .iter()
            .find(|c| c.token() == s)
            .unwrap_or_else(|| panic!("data/registered_shaders.tsv names configuration {s:?}"))
    }

    /// Whether a registration of light class `class` is made: class 0 always, classes 1-3 only with
    /// ShaderLevel on (`FUN_0084f130`'s `DAT_00dfc345 != 0` blocks).
    pub fn registers_class(self, class: u32) -> bool {
        class == 0 || self.shader_level
    }
}

/// One row of `data/registered_shaders.tsv`: a registration the retail registry makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub name: String,
    pub key: u32,
    pub sho: String,
    pub stage: Stage,
    pub family: String,
    /// The class argument of the registration call. `None` for `PgCompositeFP`, whose class a
    /// SecuROM call computes.
    pub class: Option<u32>,
    pub configs: BTreeSet<Config>,
    /// For a vertex shader: each store holding a record for its stem, with the `(usage, index)` of
    /// every input that record's bytecode declares.
    pub inputs: Vec<(String, Vec<(u8, u8)>)>,
    /// For a vertex shader: each store holding a record for its stem, with the constant names that
    /// record's CTAB declares.
    pub constants: Vec<(String, Vec<String>)>,
}

/// Every retail registration.
pub fn registered() -> &'static [Registered] {
    static R: OnceLock<Vec<Registered>> = OnceLock::new();
    R.get_or_init(|| {
        tsv_rows(REGISTERED_TSV, &["name", "key", "sho", "stage", "family", "class", "configs", "inputs", "constants"])
            .map(|f| Registered {
                name: f[0].to_string(),
                key: parse_hex(f[1]),
                sho: f[2].to_string(),
                stage: parse_stage(f[3]),
                family: f[4].to_string(),
                class: match f[5] {
                    "-" => None,
                    c => Some(c.parse().unwrap_or_else(|_| panic!("registered_shaders.tsv class {c:?}"))),
                },
                configs: f[6].split(',').map(Config::parse).collect(),
                inputs: parse_inputs(f[7]),
                constants: parse_constants(f[8]),
            })
            .collect()
    })
}

/// The retail keys of `stage` registered in every configuration. One key can be registered by
/// several rows, each in some configurations (`PgMeshNoTangentAmbientWindVP` loads a different
/// `.sho` with the VT pair resident), so the configurations are united per key.
pub fn retail_keys_everywhere(stage: Stage) -> BTreeSet<u32> {
    let mut by_key: BTreeMap<u32, BTreeSet<Config>> = BTreeMap::new();
    for r in registered().iter().filter(|r| r.stage == stage) {
        by_key.entry(r.key).or_default().extend(r.configs.iter().copied());
    }
    by_key.into_iter().filter(|(_, c)| c.len() == Config::ALL.len()).map(|(k, _)| k).collect()
}

/// Whether the retail vertex shader `key` declares `constant` in the CTAB of its record in any
/// store, in any configuration.
pub fn retail_vertex_declares(key: u32, constant: &str) -> bool {
    registered()
        .iter()
        .filter(|r| r.key == key && r.stage == Stage::Vertex)
        .flat_map(|r| r.constants.iter())
        .any(|(_, names)| names.iter().any(|n| n == constant))
}

/// The retail registration name for `key`, when there is one.
pub fn retail_name(key: u32) -> Option<&'static str> {
    registered().iter().find(|r| r.key == key).map(|r| r.name.as_str())
}

/// The store stems the retail registry loads for `stem`'s registrations, case-folded like the
/// engine's hash.
fn stem_of(sho: &str) -> &str {
    &sho[..sho.len() - 4]
}

/// The families of every retail registration that loads `<stem>.sho`.
pub fn retail_families_of_stem(stem: &str) -> BTreeSet<&'static str> {
    let key = pandemic_hash_m2(stem);
    registered()
        .iter()
        .filter(|r| pandemic_hash_m2(stem_of(&r.sho)) == key)
        .map(|r| r.family.as_str())
        .collect()
}

/// Retail registrations made in `config`, per stage.
pub fn retail_count(config: Config, stage: Stage) -> usize {
    let keys: BTreeSet<u32> = registered()
        .iter()
        .filter(|r| r.stage == stage && r.configs.contains(&config))
        .map(|r| r.key)
        .collect();
    keys.len()
}

/// The samplers every registered pixel shader declares, read from a retail shader store: for each
/// pixel key, the `(stage, name)` of every sampler in its record's CTAB (e.g. `PgDiffEmisFP`:
/// `diffuseMap` s0, `emissiveMap` s1, `globalDirLightShadowMap` s15). Each pixel key is registered
/// with one `.sho`; its record is `store_id(stem, low)` in the store.
///
/// It also keeps, per key, what the program writes to `oC0.w` ([`OutputAlpha`]), which is the source
/// alpha of a blended draw.
#[derive(Debug, Clone)]
pub struct PixelSamplers {
    by_key: BTreeMap<u32, Vec<(u8, String)>>,
    /// Each carried key's output alpha, or why its `oC0.w` write is not one [`OutputAlpha`] names.
    alpha_by_key: BTreeMap<u32, Result<OutputAlpha, String>>,
    /// Registrations whose `.sho` has no record in this store, with the reason (e.g. `PgCompositeFP`,
    /// which the composite pass registers outside `FUN_0084f130`).
    absent: BTreeMap<u32, String>,
}

impl PixelSamplers {
    /// Read every pixel registration's record from `store` (`shader3.bin` with `low = false`,
    /// `shader3Low.bin` with `low = true`). A registration whose record the store does not carry is
    /// kept as absent, and looking it up is an error naming the record; a record whose CTAB does not
    /// parse is an error here.
    pub fn from_store(store: &crate::shader3::Store, low: bool) -> Result<PixelSamplers, String> {
        let (mut by_key, mut absent, mut alpha_by_key) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        for r in registered().iter().filter(|r| r.stage == Stage::Pixel) {
            if by_key.contains_key(&r.key) || absent.contains_key(&r.key) {
                continue;
            }
            let stem = stem_of(&r.sho);
            let id = crate::shader3::store_id(stem, low).map_err(|e| format!("{}: {e:?}", r.name))?;
            let Some(rec) = store.records.iter().find(|rec| rec.id == id) else {
                absent.insert(r.key, format!("{} is registered with {}, whose record 0x{id:08X} this store does not carry", r.name, r.sho));
                continue;
            };
            let (_, consts) =
                crate::shader3::parse_ctab(store.blob(rec)).map_err(|e| format!("{} ({}): CTAB: {e:?}", r.name, r.sho))?;
            let samplers =
                consts.into_iter().filter(|c| c.register_set == 3).map(|c| (c.register_index as u8, c.name)).collect();
            by_key.insert(r.key, samplers);
            let asm = crate::shader3::disassemble(store.blob(rec)).map_err(|e| format!("{} ({}): {e:?}", r.name, r.sho))?;
            alpha_by_key.insert(r.key, OutputAlpha::of_program(&asm).map_err(|e| format!("{} ({}): {e}", r.name, r.sho)));
        }
        Ok(PixelSamplers { by_key, alpha_by_key, absent })
    }

    /// What pixel shader `key` writes to `oC0.w`. An error when the key is not carried
    /// ([`Self::samplers`]) or its program's output alpha is not one [`OutputAlpha`] names.
    pub fn output_alpha(&self, key: u32) -> Result<OutputAlpha, String> {
        self.samplers(key)?;
        self.alpha_by_key[&key].clone()
    }

    /// The registrations whose record this store does not carry, with the reason.
    pub fn absent(&self) -> impl Iterator<Item = &str> {
        self.absent.values().map(String::as_str)
    }

    /// The samplers pixel shader `key` declares, by stage. A key the retail registry does not
    /// register is an error (`Mtrl_Parse` would read through the registry's null entry: the
    /// `0x00858DB8` crash), and so is one whose record this store does not carry.
    pub fn samplers(&self, key: u32) -> Result<&[(u8, String)], String> {
        if let Some(why) = self.absent.get(&key) {
            return Err(why.clone());
        }
        self.by_key
            .get(&key)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("pixel shader key 0x{key:08X} is not a retail registration"))
    }

    /// The name of the sampler pixel shader `key` declares at `stage`, if any.
    pub fn sampler_at(&self, key: u32, stage: u8) -> Result<Option<&str>, String> {
        Ok(self.samplers(key)?.iter().find(|(s, _)| *s == stage).map(|(_, n)| n.as_str()))
    }
}

/// What a material pixel shader writes to `oC0.w`, read off its program. `d` is the diffuse texel's
/// alpha (`texld …, s0`), `k` = c98.w (`materialData.constant0.w`, the material's word 13), `v` the
/// COLOR0 input's alpha (`dcl_color`), `spec` the luminance (0.299, 0.587, 0.114) of the direct
/// specular light, before the reflection, vertex colour and fog terms (read by hand in
/// `PgDiffSpecMetalNormFP` and `PgDiffSpecNormAmbOccFP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputAlpha {
    /// `k` (`PgDiffRefract*`).
    Constant,
    /// `d · k` (`PgDiff*`, `PgDiffEmis*`, `PgFast*` and their rim/light variants).
    Diffuse,
    /// `d · k · v` (the non-specular `*AmbOcc*` shaders).
    DiffuseVertex,
    /// `saturate(max(d · k, spec))` (the `*Spec*` shaders).
    DiffuseOrSpecular,
    /// `saturate(max(d · k · v, spec))` (the `*Spec*AmbOcc*` shaders).
    DiffuseVertexOrSpecular,
}

/// One factor of an output-alpha product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AlphaFactor {
    DiffuseA,
    ConstantW,
    ColorA,
}

/// The value an `oC0.w` slice computes: a product of factors, maxed with the specular luminance
/// when `spec`.
#[derive(Debug, Default)]
struct AlphaTerm {
    factors: Vec<AlphaFactor>,
    spec: bool,
}

/// One instruction of a disassembled program: opcode (modifiers kept, e.g. `max_sat_pp`) and operands.
struct Instr<'a> {
    op: &'a str,
    operands: Vec<&'a str>,
}

/// A register operand split into name and swizzle/mask: `-r1_abs.x` → (`r1`, `x`), `oC0` → (`oC0`, ``).
fn register(operand: &str) -> (&str, &str) {
    let o = operand.trim_start_matches('-');
    let (reg, swz) = o.split_once('.').unwrap_or((o, ""));
    (reg.split('_').next().unwrap_or(reg), swz)
}

/// The component a source operand supplies to destination component `dest`.
fn source_component(swizzle: &str, dest: char) -> Result<char, String> {
    let s: Vec<char> = swizzle.chars().collect();
    match s.len() {
        0 => Ok(dest),
        1 => Ok(s[0]),
        4 => Ok(s["xyzw".find(dest).ok_or_else(|| format!("component {dest}"))?]),
        _ => Err(format!("swizzle .{swizzle} of a scalar slice")),
    }
}

impl OutputAlpha {
    /// Classify a pixel program's output alpha from its disassembly
    /// ([`crate::shader3::disassemble`]): the last write to `oC0.w`, traced back through `mov`, `mul`
    /// and `max` to texture reads, constants and inputs. Any other shape is an error naming it.
    pub fn of_program(asm: &[String]) -> Result<OutputAlpha, String> {
        let mut color0: Vec<&str> = Vec::new();
        // Each `def` constant's four components as written.
        let mut defs: BTreeMap<&str, [&str; 4]> = BTreeMap::new();
        let mut instrs: Vec<Instr> = Vec::new();
        for l in asm.iter().map(|l| l.trim()) {
            if l.is_empty() || l.starts_with('.') || l.starts_with("ps_") {
                continue;
            }
            let (op, rest) = l.split_once(' ').unwrap_or((l, ""));
            let operands: Vec<&str> = rest.split(',').map(str::trim).filter(|o| !o.is_empty()).collect();
            if op == "dcl_color" {
                color0.extend(operands.first().map(|o| register(o).0));
            } else if op == "def" {
                if let [reg, a, b, c, d] = operands[..] {
                    defs.insert(reg, [a, b, c, d]);
                }
            } else if !op.starts_with("dcl") {
                instrs.push(Instr { op, operands });
            }
        }
        let writes_w = |i: &Instr, reg: &str, comp: char| {
            i.operands.first().is_some_and(|d| {
                let (r, mask) = register(d);
                r == reg && (mask.is_empty() || mask.contains(comp))
            })
        };
        let last = instrs
            .iter()
            .rposition(|i| writes_w(i, "oC0", 'w'))
            .ok_or_else(|| "the program never writes oC0.w".to_string())?;
        // The operands of a `dp3` that read the luminance weights (0.299, 0.587, 0.114) from a `def`,
        // through their swizzle (`c4` = (0.299, 0.587, 0.114, 0) or `c4.yzw` of (30000, 0.299, …)).
        let luma = |operand: &str| -> bool {
            let (reg, swz) = register(operand);
            let Some(v) = defs.get(reg) else { return false };
            let pick: Vec<&str> = match swz.len() {
                0 => v[..3].to_vec(),
                3 | 4 => swz.chars().take(3).filter_map(|c| "xyzw".find(c).map(|k| v[k])).collect(),
                _ => return false,
            };
            pick == ["0.299", "0.587", "0.114"]
        };

        // The value instruction `at` gives component `comp` of its destination.
        fn trace(
            instrs: &[Instr],
            at: usize,
            comp: char,
            color0: &[&str],
            luma: &dyn Fn(&str) -> bool,
            writes: &dyn Fn(&Instr, &str, char) -> bool,
        ) -> Result<AlphaTerm, String> {
            let i = &instrs[at];
            let base = i.op.split('_').next().unwrap_or(i.op);
            let src = |k: usize| -> Result<AlphaTerm, String> {
                let operand = i.operands.get(k).ok_or_else(|| format!("{} has no operand {k}", i.op))?;
                let (reg, swz) = register(operand);
                let c = source_component(swz, comp)?;
                if reg == "c98" && c == 'w' {
                    return Ok(AlphaTerm { factors: vec![AlphaFactor::ConstantW], spec: false });
                }
                if reg.starts_with('v') && color0.contains(&reg) && c == 'w' {
                    return Ok(AlphaTerm { factors: vec![AlphaFactor::ColorA], spec: false });
                }
                if reg.starts_with('r') {
                    let d = instrs[..at]
                        .iter()
                        .rposition(|p| writes(p, reg, c))
                        .ok_or_else(|| format!("{reg}.{c} is read before it is written"))?;
                    return trace(instrs, d, c, color0, luma, writes);
                }
                Err(format!("{} reads {operand}", i.op))
            };
            match base {
                "texld" if i.operands.get(2) == Some(&"s0") && comp == 'w' => {
                    Ok(AlphaTerm { factors: vec![AlphaFactor::DiffuseA], spec: false })
                }
                "dp3" if i.operands[1..].iter().any(|o| luma(o)) => {
                    Ok(AlphaTerm { factors: Vec::new(), spec: true })
                }
                "mov" => src(1),
                "mul" => {
                    let (a, b) = (src(1)?, src(2)?);
                    if a.spec || b.spec {
                        return Err(format!("{} multiplies the specular luminance", i.op));
                    }
                    Ok(AlphaTerm { factors: a.factors.into_iter().chain(b.factors).collect(), spec: false })
                }
                "max" => {
                    let (a, b) = (src(1)?, src(2)?);
                    match (a.spec, b.spec) {
                        (true, false) if a.factors.is_empty() => Ok(AlphaTerm { factors: b.factors, spec: true }),
                        (false, true) if b.factors.is_empty() => Ok(AlphaTerm { factors: a.factors, spec: true }),
                        _ => Err(format!("{} is not the max of a product and the specular luminance", i.op)),
                    }
                }
                _ => Err(format!("{} {}", i.op, i.operands.join(", "))),
            }
        }
        let mut t = trace(&instrs, last, 'w', &color0, &luma, &writes_w)?;
        t.factors.sort();
        use AlphaFactor::*;
        match (t.factors.as_slice(), t.spec) {
            ([ConstantW], false) => Ok(OutputAlpha::Constant),
            ([DiffuseA, ConstantW], false) => Ok(OutputAlpha::Diffuse),
            ([DiffuseA, ConstantW, ColorA], false) => Ok(OutputAlpha::DiffuseVertex),
            ([DiffuseA, ConstantW], true) => Ok(OutputAlpha::DiffuseOrSpecular),
            ([DiffuseA, ConstantW, ColorA], true) => Ok(OutputAlpha::DiffuseVertexOrSpecular),
            (f, spec) => Err(format!("oC0.w is the product {f:?}{}", if spec { " maxed with the specular luminance" } else { "" })),
        }
    }

    /// The code the material shaders read (`MaterialParams.alpha.y`).
    pub fn code(self) -> u32 {
        match self {
            OutputAlpha::Constant => 0,
            OutputAlpha::Diffuse => 1,
            OutputAlpha::DiffuseVertex => 2,
            OutputAlpha::DiffuseOrSpecular => 3,
            OutputAlpha::DiffuseVertexOrSpecular => 4,
        }
    }
}

/// The rows of a tab-separated table whose first line must be exactly `header`; every row must have
/// as many fields.
pub fn tsv_rows<'a>(text: &'a str, header: &[&str]) -> impl Iterator<Item = Vec<&'a str>> + 'a {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    if head != header {
        panic!("TSV header {head:?} is not {header:?}");
    }
    let n = header.len();
    lines.map(move |l| {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != n {
            panic!("TSV row {l:?} has {} fields, not {n}", f.len());
        }
        f
    })
}

/// `-`, or `<store>=<usage>.<index>+…` per store, `;`-separated.
fn parse_inputs(s: &str) -> Vec<(String, Vec<(u8, u8)>)> {
    if s == "-" {
        return Vec::new();
    }
    s.split(';')
        .map(|entry| {
            let (store, list) = entry.split_once('=').unwrap_or_else(|| panic!("registered_shaders.tsv inputs {entry:?}"));
            let inputs = list
                .split('+')
                .filter(|x| !x.is_empty())
                .map(|ui| {
                    let (u, i) = ui.split_once('.').unwrap_or_else(|| panic!("registered_shaders.tsv input {ui:?}"));
                    (u.parse().expect("usage"), i.parse().expect("usage index"))
                })
                .collect();
            (store.to_string(), inputs)
        })
        .collect()
}

/// `-`, or `<store>=<name>+…` per store, `;`-separated.
fn parse_constants(s: &str) -> Vec<(String, Vec<String>)> {
    if s == "-" {
        return Vec::new();
    }
    s.split(';')
        .map(|entry| {
            let (store, list) = entry.split_once('=').unwrap_or_else(|| panic!("registered_shaders.tsv constants {entry:?}"));
            (store.to_string(), list.split('+').filter(|x| !x.is_empty()).map(str::to_string).collect())
        })
        .collect()
}

/// A TSV hex field (`0x…` or bare).
pub fn parse_hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or_else(|_| panic!("TSV hex field {s:?}"))
}

/// A TSV stage field: `vertex` or `pixel`.
pub fn parse_stage(s: &str) -> Stage {
    match s {
        "vertex" => Stage::Vertex,
        "pixel" => Stage::Pixel,
        other => panic!("TSV stage {other:?}"),
    }
}
