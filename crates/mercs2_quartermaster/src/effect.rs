//! The effect author form: a whole particle effect declared node by node, lowered to the effect
//! container (`mercs2_formats::fxdict::EffectContainer`) and expressed back from one.
//!
//! ```yaml
//! shapes:                                   # the EMTR shape tables: each a list of 13-f32 records
//!   - - [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
//! emitters:
//!   - transform: [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]
//!     channels:                             # the nine TRFM channels, by name
//!       posx: { value: 0.0, curve: none, options: [] }
//!       ...
//!     geom: { shape: 0, word: 16 }          # or `none`
//!     particle:
//!       flags: 1
//!       attributes:                         # all 32 PTYP attributes, by name or 0xHHHHHHHH
//!         name: { value: "0x1B2C3D4E", curve: none, options: [] }
//!         size: { value: 0.5, curve: [[0, 0.5], [100, 2.0]], options: [] }
//!         ...
//!       colour:                             # exactly 100 keys over the particle's life
//!         - { rgba: [0, 255, 255, 255], half: "0x3C00" }
//!         ...
//!       frames: ["0x9BD846DE"]              # fxdict record names or 0xHHHHHHHH, at least one
//! forces:
//!   - params: { kind: gravity, magnitude: 9.8, direction: [0, -1, 0] }
//!     attributes: { ampl: { value: 1.0, curve: none, options: [] }, ... }
//! ```
//!
//! The same document reads from YAML, JSON or TOML ([`crate::Format`]). Nothing has a default:
//!
//! * every attribute position of its node is given exactly once, by its recovered name or its hash
//!   as `0xHHHHHHHH`, each with `value`, `curve` (`none` or a list of `[time, value]` keys) and
//!   `options` (a list of `bit7`, `resample`, `bit9`; empty for none);
//! * an f32 position takes a finite number; a u32 position takes an integer, `0xHHHHHHHH`, or a
//!   name (hashed with `pandemic_hash_m2`);
//! * `geom` is `{shape, word}` or `none`; `colour` has exactly 100 keys, each `rgba` and `half`
//!   (an integer or `0xHHHH`); `frames` names at least one sprite frame, a record of the game's
//!   `fxdict` (`crate::fx`).
//!
//! [`EffectForm::lower`] checks every value against its position and builds the container, which
//! the effect writer validates again ([`EffectContainer::validate`]); [`EffectForm::express`] writes
//! any container as a form. Every one of the 314 retail effects expresses, reads back from YAML and
//! re-encodes to its own bytes (`tests/fx_retail.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use mercs2_formats::fxdict::{
    atrb_flag, write_effect_container, AnimKey, Atrb, AtrbValue, AttrDef, Colr, ColrKey, EffectContainer,
    Emitter, EmitterGeom, EmitterShape, Force, ForceKind, ParticleType, Text, ValueKind, COLR_KEYS,
    FRCE_COMMON_ATTRIBUTES, PTYP_ATTRIBUTES_AFTER_COLR, PTYP_ATTRIBUTES_BEFORE_COLR, SHAPE_RECORD_FLOATS,
    TRFM_CHANNELS,
};
use mercs2_formats::hash::pandemic_hash_m2;
use serde::{Deserialize, Serialize};

use crate::Format;

/// One `EMTR/GEOM` shape table: its 13-f32 records.
pub type ShapeForm = Vec<[f32; SHAPE_RECORD_FLOATS]>;

/// A whole effect, as authored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectForm {
    /// The `EMTR` shape tables, in order; an emitter's `geom.shape` indexes this list.
    pub shapes: Vec<ShapeForm>,
    pub emitters: Vec<EmitterForm>,
    pub forces: Vec<ForceForm>,
}

/// One emitter: its `EMIT` (transform, channels, geom) and its `PTYP`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitterForm {
    /// The `TRFM` 4×4, rows as stored.
    pub transform: [[f32; 4]; 4],
    /// The nine `TRFM` channels.
    pub channels: BTreeMap<String, AttrForm>,
    pub geom: GeomForm,
    pub particle: ParticleForm,
}

/// An emitter's `GEOM`: a shape index and the second word, or `none` for an emitter without one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeomForm {
    Some(GeomRef),
    /// The keyword `none`.
    None(String),
}

/// `GEOM`: `shape` indexes [`EffectForm::shapes`]; `word` is the second u16 (stored at `+0x00` of
/// the emitter record; its meaning is not established).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeomRef {
    pub shape: u16,
    pub word: u16,
}

/// The `PTYP` node and its children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleForm {
    /// Bit 0 and bit 1 are read by the loader; no other bit is.
    pub flags: u32,
    /// All 32 `PTYP` attributes.
    pub attributes: BTreeMap<String, AttrForm>,
    /// `COLR`: exactly 100 keys.
    pub colour: Vec<ColourKeyForm>,
    /// `TEXT`: the sprite frames, each the key of an `fxdict` record, by name or `0xHHHHHHHH`.
    pub frames: Vec<String>,
}

/// One `COLR` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColourKeyForm {
    /// The four colour bytes, in file order.
    pub rgba: [u8; 4],
    /// The binary16 bit pattern carried with the key.
    pub half: WordInput,
}

/// An integer written as a number or as `0xHHHH…`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WordInput {
    Int(u64),
    Text(String),
}

/// One `FRCE`: the typed force and its attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForceForm {
    pub params: ForceParamsForm,
    /// The seven common attributes and the kind's extras.
    pub attributes: BTreeMap<String, AttrForm>,
}

/// A force's kind and parameters, as `FUN_00491920`'s `FRCE` arm reads them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ForceParamsForm {
    Gravity { magnitude: f32, direction: [f32; 3] },
    Drag { magnitude: f32 },
    Wind { magnitude: f32, direction: [f32; 3] },
    Attractor { magnitude: f32, flag_13c: u32, param_130: f32, param_134: f32, vector_104: [f32; 3] },
    Vortex {
        magnitude: f32,
        flag_13c: u32,
        param_130: f32,
        param_134: f32,
        param_138: f32,
        vector_104: [f32; 3],
        vector_110: [f32; 3],
        vector_11c: [f32; 3],
    },
}

/// One attribute: its value, its curve and its option bits, every one declared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttrForm {
    pub value: ValueInput,
    pub curve: CurveForm,
    pub options: Vec<AtrbOption>,
}

/// An authored attribute value, before it is checked against its position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueInput {
    Int(i64),
    Float(f32),
    Text(String),
}

/// An attribute's `ANIM` curve: `[time, value]` keys, or the keyword `none`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CurveForm {
    Keys(Vec<[f32; 2]>),
    /// The keyword `none`.
    Keyword(String),
}

/// The authored `ATRB` option bits ([`atrb_flag::OPTIONS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtrbOption {
    /// Bit 7 (meaning not established).
    Bit7,
    /// Bit 8: `FUN_00493150` resamples a TRFM curve to 100 words.
    Resample,
    /// Bit 9 (meaning not established).
    Bit9,
}

impl AtrbOption {
    const ALL: [AtrbOption; 3] = [AtrbOption::Bit7, AtrbOption::Resample, AtrbOption::Bit9];

    fn bit(self) -> u32 {
        match self {
            AtrbOption::Bit7 => atrb_flag::BIT7,
            AtrbOption::Resample => atrb_flag::RESAMPLE,
            AtrbOption::Bit9 => atrb_flag::BIT9,
        }
    }
}

const NONE: &str = "none";

/// Parse a form from text.
pub fn from_str(text: &str, format: Format) -> Result<EffectForm, String> {
    match format {
        Format::Yaml => serde_norway::from_str(text).map_err(|e| e.to_string()),
        Format::Json => serde_json::from_str(text).map_err(|e| e.to_string()),
        Format::Toml => toml::from_str(text).map_err(|e| e.to_string()),
    }
    .map_err(|e| format!("effect form ({format:?}): {e}"))
}

/// Write a form as text.
pub fn to_string(form: &EffectForm, format: Format) -> Result<String, String> {
    match format {
        Format::Yaml => serde_norway::to_string(form).map_err(|e| e.to_string()),
        Format::Json => serde_json::to_string_pretty(form).map_err(|e| e.to_string()),
        Format::Toml => toml::to_string(form).map_err(|e| e.to_string()),
    }
}

/// The format of a form file, from its extension.
pub fn file_format(path: &Path) -> Result<Format, String> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension)
        .ok_or_else(|| format!("{}: the extension names no form format (yaml, yml, json, toml)", path.display()))
}

/// Read a form file, its format taken from its extension.
pub fn read(path: &Path) -> Result<EffectForm, String> {
    let format = file_format(path)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    from_str(&text, format).map_err(|e| format!("{}: {e}", path.display()))
}

/// `0xHHHHHHHH` as a hash; `None` for anything else.
pub(crate) fn hex_hash(s: &str) -> Option<u32> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if h.is_empty() || h.len() > 8 {
        return None;
    }
    u32::from_str_radix(h, 16).ok()
}

/// A name or `0xHHHHHHHH` as a hash. An empty string names nothing.
pub(crate) fn name_hash(s: &str) -> Result<u32, String> {
    if s.is_empty() {
        return Err("an empty name names nothing".into());
    }
    Ok(hex_hash(s).unwrap_or_else(|| pandemic_hash_m2(s)))
}

pub(crate) fn hex(v: u32) -> String {
    format!("0x{v:08X}")
}

/// The key a position is written under: its recovered name, or its hash.
fn label(d: &AttrDef) -> String {
    d.name.map(str::to_string).unwrap_or_else(|| hex(d.hash))
}

fn finite(what: &str, x: f32) -> Result<f32, String> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(format!("{what}: {x} is not a finite f32"))
    }
}

fn finite_all<const N: usize>(what: &str, v: &[f32; N]) -> Result<[f32; N], String> {
    for x in v {
        finite(what, *x)?;
    }
    Ok(*v)
}

/// Check an integer word against `max`.
pub(crate) fn word(what: &str, w: &WordInput, max: u64) -> Result<u64, String> {
    let v = match w {
        WordInput::Int(i) => *i,
        WordInput::Text(s) => {
            let h = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .filter(|h| !h.is_empty() && h.len() <= 16)
                .ok_or_else(|| format!("{what}: {s:?} is neither an integer nor 0xHHHH"))?;
            u64::from_str_radix(h, 16).map_err(|e| format!("{what}: {s:?}: {e}"))?
        }
    };
    if v > max {
        return Err(format!("{what}: {v} is above {max}"));
    }
    Ok(v)
}

/// A position's value, typed by the position.
pub(crate) fn lower_value(what: &str, d: &AttrDef, v: &ValueInput) -> Result<AtrbValue, String> {
    match (d.kind, v) {
        (ValueKind::F32, ValueInput::Float(x)) => Ok(AtrbValue::F32(finite(what, *x)?)),
        (ValueKind::F32, ValueInput::Int(i)) => {
            let x = *i as f32;
            if x as i64 != *i {
                return Err(format!("{what}: {i} is not exactly an f32"));
            }
            Ok(AtrbValue::F32(x))
        }
        (ValueKind::U32, ValueInput::Int(i)) if (0..=u32::MAX as i64).contains(i) => Ok(AtrbValue::U32(*i as u32)),
        (ValueKind::U32, ValueInput::Int(i)) => Err(format!("{what}: {i} is outside the u32 range")),
        (ValueKind::U32, ValueInput::Text(s)) => Ok(AtrbValue::U32(name_hash(s).map_err(|e| format!("{what}: {e}"))?)),
        (k, v) => Err(format!("{what}: an {k:?} position cannot take {v:?}")),
    }
}

/// A curve: `None` for the keyword `none`.
pub(crate) fn lower_curve(what: &str, c: &CurveForm) -> Result<Option<Vec<AnimKey>>, String> {
    match c {
        CurveForm::Keyword(k) if k == NONE => Ok(None),
        CurveForm::Keyword(k) => Err(format!("{what}: curve {k:?} is neither `none` nor a list of [time, value] keys")),
        CurveForm::Keys(keys) if keys.is_empty() => {
            Err(format!("{what}: a curve needs at least one key; write `none` for no curve"))
        }
        CurveForm::Keys(keys) => keys
            .iter()
            .map(|[t, v]| Ok(AnimKey { time: finite(what, *t)?, value: finite(what, *v)? }))
            .collect::<Result<Vec<_>, String>>()
            .map(Some),
    }
}

/// The option bits, each at most once.
pub(crate) fn lower_options(what: &str, options: &[AtrbOption]) -> Result<u32, String> {
    let mut bits = 0;
    for o in options {
        if bits & o.bit() != 0 {
            return Err(format!("{what}: option {o:?} is listed twice"));
        }
        bits |= o.bit();
    }
    Ok(bits)
}

/// One `ATRB` from its parts: the flag word is derived from the value, the curve and the options.
pub(crate) fn atrb(hash: u32, value: AtrbValue, curve: Option<Vec<AnimKey>>, options: u32) -> Atrb {
    let mut flags = options;
    if matches!(value, AtrbValue::F32(_)) {
        flags |= atrb_flag::FLOAT;
    }
    if curve.is_some() {
        flags |= atrb_flag::CURVE;
    }
    Atrb { hash, flags, value, curve }
}

fn lower_attr(what: &str, d: &AttrDef, a: &AttrForm) -> Result<Atrb, String> {
    Ok(atrb(
        d.hash,
        lower_value(what, d, &a.value)?,
        lower_curve(what, &a.curve)?,
        lower_options(what, &a.options)?,
    ))
}

/// The position a key names in `defs`: by name or by `0xHHHHHHHH`.
pub(crate) fn position_of(what: &str, defs: &[&AttrDef], key: &str) -> Result<usize, String> {
    let h = name_hash(key).map_err(|e| format!("{what}: {e}"))?;
    defs.iter().position(|d| d.hash == h).ok_or_else(|| {
        let known: Vec<String> = defs.iter().map(|d| label(d)).collect();
        format!("{what}: `{key}` is not one of its attributes ({})", known.join(", "))
    })
}

/// A run of attributes: every position of `defs` exactly once, in the table's order.
fn lower_attrs(what: &str, defs: &[&AttrDef], map: &BTreeMap<String, AttrForm>) -> Result<Vec<Atrb>, String> {
    let mut by_pos: Vec<Option<(&String, &AttrForm)>> = vec![None; defs.len()];
    for (k, a) in map {
        let p = position_of(what, defs, k)?;
        if let Some((prev, _)) = by_pos[p].replace((k, a)) {
            return Err(format!("{what}: `{prev}` and `{k}` name the same attribute"));
        }
    }
    defs.iter()
        .zip(by_pos)
        .map(|(d, slot)| {
            let (k, a) = slot.ok_or_else(|| {
                format!("{what}: attribute {} is not declared; every attribute of the node is", label(d))
            })?;
            lower_attr(&format!("{what}.{k}"), d, a)
        })
        .collect()
}

fn express_attr(d: &AttrDef, a: &Atrb) -> AttrForm {
    let value = match a.value {
        AtrbValue::F32(x) => ValueInput::Float(x),
        // The particle `name` holds a hash; every other u32 position holds a count or a mode.
        AtrbValue::U32(v) if d.hash == PTYP_ATTRIBUTES_BEFORE_COLR[0].hash => ValueInput::Text(hex(v)),
        AtrbValue::U32(v) => ValueInput::Int(v as i64),
    };
    let curve = match &a.curve {
        None => CurveForm::Keyword(NONE.into()),
        Some(keys) => CurveForm::Keys(keys.iter().map(|k| [k.time, k.value]).collect()),
    };
    let options = AtrbOption::ALL.into_iter().filter(|o| a.flags & o.bit() != 0).collect();
    AttrForm { value, curve, options }
}

fn express_attrs(defs: &[&AttrDef], attrs: &[Atrb]) -> BTreeMap<String, AttrForm> {
    defs.iter().zip(attrs).map(|(d, a)| (label(d), express_attr(d, a))).collect()
}

/// The `TRFM` channel positions.
pub fn channel_defs() -> Vec<&'static AttrDef> {
    TRFM_CHANNELS.iter().collect()
}

/// The 32 `PTYP` positions, in file order.
pub fn particle_defs() -> Vec<&'static AttrDef> {
    PTYP_ATTRIBUTES_BEFORE_COLR.iter().chain(PTYP_ATTRIBUTES_AFTER_COLR.iter()).collect()
}

/// A force kind's positions: the common seven, then its extras.
pub fn force_defs(kind: &ForceKind) -> Vec<&'static AttrDef> {
    FRCE_COMMON_ATTRIBUTES.iter().chain(kind.extra_attributes()).collect()
}

/// A colour table: exactly 100 keys.
pub(crate) fn lower_colour(what: &str, keys: &[ColourKeyForm]) -> Result<Colr, String> {
    if keys.len() != COLR_KEYS {
        return Err(format!("{what}: {} keys; COLR has exactly {COLR_KEYS}", keys.len()));
    }
    let mut out = [ColrKey { colour: [0; 4], half_bits: 0 }; COLR_KEYS];
    for (i, (o, k)) in out.iter_mut().zip(keys).enumerate() {
        *o = ColrKey { colour: k.rgba, half_bits: word(&format!("{what}[{i}].half"), &k.half, u16::MAX as u64)? as u16 };
    }
    Ok(Colr { keys: out })
}

/// Sprite frames: at least one, each a name or `0xHHHHHHHH`.
pub(crate) fn lower_frames(what: &str, frames: &[String]) -> Result<Text, String> {
    if frames.is_empty() {
        return Err(format!("{what}: an emitter names at least one frame"));
    }
    let frames = frames
        .iter()
        .enumerate()
        .map(|(i, f)| name_hash(f).map_err(|e| format!("{what}[{i}]: {e}")))
        .collect::<Result<Vec<u32>, String>>()?;
    Ok(Text { frames })
}

pub(crate) fn lower_geom(what: &str, g: &GeomForm) -> Result<Option<EmitterGeom>, String> {
    match g {
        GeomForm::Some(r) => Ok(Some(EmitterGeom { shape_index: r.shape, word_00: r.word })),
        GeomForm::None(k) if k == NONE => Ok(None),
        GeomForm::None(k) => Err(format!("{what}: {k:?} is neither `none` nor {{shape, word}}")),
    }
}

pub(crate) fn lower_shape(what: &str, records: &ShapeForm) -> Result<EmitterShape, String> {
    for (i, r) in records.iter().enumerate() {
        finite_all(&format!("{what}[{i}]"), r)?;
    }
    Ok(EmitterShape { records: records.clone() })
}

pub(crate) fn lower_transform(what: &str, t: &[[f32; 4]; 4]) -> Result<[[f32; 4]; 4], String> {
    for (i, row) in t.iter().enumerate() {
        finite_all(&format!("{what}[{i}]"), row)?;
    }
    Ok(*t)
}

impl EmitterForm {
    /// The emitter, every node checked against its position table.
    pub fn lower(&self, what: &str) -> Result<Emitter, String> {
        let p = &self.particle;
        Ok(Emitter {
            transform: lower_transform(&format!("{what}.transform"), &self.transform)?,
            channels: lower_attrs(&format!("{what}.channels"), &channel_defs(), &self.channels)?,
            geom: lower_geom(&format!("{what}.geom"), &self.geom)?,
            particle: ParticleType {
                flags: p.flags,
                attributes: lower_attrs(&format!("{what}.particle.attributes"), &particle_defs(), &p.attributes)?,
                colr: lower_colour(&format!("{what}.particle.colour"), &p.colour)?,
                text: lower_frames(&format!("{what}.particle.frames"), &p.frames)?,
            },
        })
    }

    pub fn express(e: &Emitter) -> EmitterForm {
        EmitterForm {
            transform: e.transform,
            channels: express_attrs(&channel_defs(), &e.channels),
            geom: match e.geom {
                Some(g) => GeomForm::Some(GeomRef { shape: g.shape_index, word: g.word_00 }),
                None => GeomForm::None(NONE.into()),
            },
            particle: ParticleForm {
                flags: e.particle.flags,
                attributes: express_attrs(&particle_defs(), &e.particle.attributes),
                colour: e
                    .particle
                    .colr
                    .keys
                    .iter()
                    .map(|k| ColourKeyForm { rgba: k.colour, half: WordInput::Text(format!("0x{:04X}", k.half_bits)) })
                    .collect(),
                frames: e.particle.text.frames.iter().map(|&f| hex(f)).collect(),
            },
        }
    }
}

impl ForceParamsForm {
    /// The typed force, every number finite.
    pub fn lower(&self, what: &str) -> Result<ForceKind, String> {
        let f = |x: f32| finite(what, x);
        let v = |x: &[f32; 3]| finite_all(what, x);
        Ok(match self {
            ForceParamsForm::Gravity { magnitude, direction } => {
                ForceKind::Gravity { magnitude: f(*magnitude)?, direction: v(direction)? }
            }
            ForceParamsForm::Drag { magnitude } => ForceKind::Drag { magnitude: f(*magnitude)? },
            ForceParamsForm::Wind { magnitude, direction } => {
                ForceKind::Wind { magnitude: f(*magnitude)?, direction: v(direction)? }
            }
            ForceParamsForm::Attractor { magnitude, flag_13c, param_130, param_134, vector_104 } => {
                ForceKind::Attractor {
                    magnitude: f(*magnitude)?,
                    flag_13c: *flag_13c,
                    param_130: f(*param_130)?,
                    param_134: f(*param_134)?,
                    vector_104: v(vector_104)?,
                }
            }
            ForceParamsForm::Vortex {
                magnitude,
                flag_13c,
                param_130,
                param_134,
                param_138,
                vector_104,
                vector_110,
                vector_11c,
            } => ForceKind::Vortex {
                magnitude: f(*magnitude)?,
                flag_13c: *flag_13c,
                param_130: f(*param_130)?,
                param_134: f(*param_134)?,
                param_138: f(*param_138)?,
                vector_104: v(vector_104)?,
                vector_110: v(vector_110)?,
                vector_11c: v(vector_11c)?,
            },
        })
    }

    pub fn express(k: &ForceKind) -> ForceParamsForm {
        match *k {
            ForceKind::Gravity { magnitude, direction } => ForceParamsForm::Gravity { magnitude, direction },
            ForceKind::Drag { magnitude } => ForceParamsForm::Drag { magnitude },
            ForceKind::Wind { magnitude, direction } => ForceParamsForm::Wind { magnitude, direction },
            ForceKind::Attractor { magnitude, flag_13c, param_130, param_134, vector_104 } => {
                ForceParamsForm::Attractor { magnitude, flag_13c, param_130, param_134, vector_104 }
            }
            ForceKind::Vortex {
                magnitude,
                flag_13c,
                param_130,
                param_134,
                param_138,
                vector_104,
                vector_110,
                vector_11c,
            } => ForceParamsForm::Vortex {
                magnitude,
                flag_13c,
                param_130,
                param_134,
                param_138,
                vector_104,
                vector_110,
                vector_11c,
            },
        }
    }
}

impl ForceForm {
    pub fn lower(&self, what: &str) -> Result<Force, String> {
        let kind = self.params.lower(&format!("{what}.params"))?;
        let attributes = lower_attrs(&format!("{what}.attributes"), &force_defs(&kind), &self.attributes)?;
        Ok(Force { kind, attributes })
    }

    pub fn express(f: &Force) -> ForceForm {
        ForceForm { params: ForceParamsForm::express(&f.kind), attributes: express_attrs(&force_defs(&f.kind), &f.attributes) }
    }
}

/// An attribute edited in place: `value`, `curve` and `options` each replace the existing one when
/// given and keep it when absent. The result is checked against the position.
pub(crate) fn edit_attr(
    what: &str,
    d: &AttrDef,
    a: &Atrb,
    value: Option<&ValueInput>,
    curve: Option<&CurveForm>,
    options: Option<&[AtrbOption]>,
) -> Result<Atrb, String> {
    if value.is_none() && curve.is_none() && options.is_none() {
        return Err(format!("{what}: the edit gives none of value, curve, options"));
    }
    let value = match value {
        Some(v) => lower_value(what, d, v)?,
        None => a.value,
    };
    let curve = match curve {
        Some(c) => lower_curve(what, c)?,
        None => a.curve.clone(),
    };
    let options = match options {
        Some(o) => lower_options(what, o)?,
        None => a.flags & atrb_flag::OPTIONS,
    };
    Ok(atrb(d.hash, value, curve, options))
}

impl EffectForm {
    /// The effect container, every node checked against its position tables and then against the
    /// writer's own rules ([`EffectContainer::validate`]).
    pub fn lower(&self) -> Result<EffectContainer, String> {
        let shapes = self
            .shapes
            .iter()
            .enumerate()
            .map(|(i, s)| lower_shape(&format!("shapes[{i}]"), s))
            .collect::<Result<Vec<_>, String>>()?;
        let emitters = self
            .emitters
            .iter()
            .enumerate()
            .map(|(i, e)| e.lower(&format!("emitters[{i}]")))
            .collect::<Result<Vec<_>, String>>()?;
        let forces = self
            .forces
            .iter()
            .enumerate()
            .map(|(i, f)| f.lower(&format!("forces[{i}]")))
            .collect::<Result<Vec<_>, String>>()?;
        let fx = EffectContainer { shapes, emitters, forces };
        fx.validate()?;
        Ok(fx)
    }

    /// The container's bytes.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        write_effect_container(&self.lower()?)
    }

    /// Write any effect as a form.
    pub fn express(fx: &EffectContainer) -> EffectForm {
        EffectForm {
            shapes: fx.shapes.iter().map(|s| s.records.clone()).collect(),
            emitters: fx.emitters.iter().map(EmitterForm::express).collect(),
            forces: fx.forces.iter().map(ForceForm::express).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::fxdict::parse_effect_container;

    fn attr(value: ValueInput) -> AttrForm {
        AttrForm { value, curve: CurveForm::Keyword(NONE.into()), options: vec![] }
    }

    /// A one-emitter effect with every position declared, and a gravity and a vortex force.
    fn form() -> EffectForm {
        let channels = channel_defs()
            .iter()
            .enumerate()
            .map(|(i, d)| (label(d), attr(ValueInput::Float(if i >= 6 { 1.0 } else { 0.0 }))))
            .collect();
        let attributes = particle_defs()
            .iter()
            .map(|d| {
                let v = match (d.name, d.kind) {
                    (Some("name"), _) => ValueInput::Text("qm_test_burst".into()),
                    (_, ValueKind::U32) => ValueInput::Int(1),
                    (Some("size"), _) => ValueInput::Float(0.5),
                    _ => ValueInput::Float(0.0),
                };
                (label(d), attr(v))
            })
            .collect();
        let force = |params: ForceParamsForm| {
            let kind = params.lower("p").unwrap();
            let attributes = force_defs(&kind)
                .iter()
                .map(|d| {
                    let v = match d.kind {
                        ValueKind::F32 => ValueInput::Float(0.0),
                        ValueKind::U32 => ValueInput::Int(0),
                    };
                    (label(d), attr(v))
                })
                .collect();
            ForceForm { params, attributes }
        };
        let mut f = EffectForm {
            shapes: vec![vec![[0.0; SHAPE_RECORD_FLOATS]]],
            emitters: vec![EmitterForm {
                transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
                channels,
                geom: GeomForm::Some(GeomRef { shape: 0, word: 16 }),
                particle: ParticleForm {
                    flags: 1,
                    attributes,
                    colour: (0..COLR_KEYS)
                        .map(|i| ColourKeyForm { rgba: [0, 255, 255, (255 - i * 2) as u8], half: WordInput::Text("0x3C00".into()) })
                        .collect(),
                    frames: vec!["qm_cyan_disc".into(), "0x00000001".into()],
                },
            }],
            forces: vec![
                force(ForceParamsForm::Gravity { magnitude: 9.8, direction: [0.0, -1.0, 0.0] }),
                force(ForceParamsForm::Vortex {
                    magnitude: 1.0,
                    flag_13c: 1,
                    param_130: 2.0,
                    param_134: 3.0,
                    param_138: 4.0,
                    vector_104: [0.0, 0.0, 0.0],
                    vector_110: [0.0, 1.0, 0.0],
                    vector_11c: [1.0, 0.0, 0.0],
                }),
            ],
        };
        // A resampled curve on `size` and a linear one with an option on the force's `ampl`.
        let size = f.emitters[0].particle.attributes.get_mut("size").unwrap();
        size.curve = CurveForm::Keys(vec![[0.0, 0.5], [100.0, 2.0]]);
        let ampl = f.forces[0].attributes.get_mut("ampl").unwrap();
        ampl.curve = CurveForm::Keys(vec![[0.0, 1.0], [100.0, 0.0]]);
        f
    }

    #[test]
    fn a_form_lowers_writes_and_expresses_back() {
        let f = form();
        let bytes = f.encode().unwrap();
        let fx = parse_effect_container(&bytes).unwrap();
        assert_eq!(fx.emitters[0].particle.text.frames, vec![pandemic_hash_m2("qm_cyan_disc"), 1]);
        assert_eq!(fx.emitters[0].particle.attributes[0].value, AtrbValue::U32(pandemic_hash_m2("qm_test_burst")));
        assert_eq!(fx.forces.len(), 2);
        let back = EffectForm::express(&fx);
        assert_eq!(back.encode().unwrap(), bytes);
    }

    #[test]
    fn the_form_round_trips_through_yaml_json_and_toml() {
        let fx = form().lower().unwrap();
        let expressed = EffectForm::express(&fx);
        for format in [Format::Yaml, Format::Json, Format::Toml] {
            let text = to_string(&expressed, format).unwrap();
            let back = from_str(&text, format).unwrap_or_else(|e| panic!("{format:?}: {e}\n{text}"));
            assert_eq!(back, expressed, "{format:?}");
            assert_eq!(back.lower().unwrap(), fx, "{format:?}");
        }
    }

    fn lower_err(edit: impl Fn(&mut EffectForm)) -> String {
        let mut f = form();
        edit(&mut f);
        f.lower().unwrap_err()
    }

    #[test]
    fn each_refusal_names_what_is_wrong() {
        let e = lower_err(|f| {
            f.emitters[0].channels.remove("posx");
        });
        assert!(e.contains("posx") && e.contains("not declared"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].channels.insert("0x7D1117EB".into(), attr(ValueInput::Float(0.0)));
        });
        assert!(e.contains("name the same attribute"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.insert("nope".into(), attr(ValueInput::Float(0.0)));
        });
        assert!(e.contains("not one of its attributes"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.insert("size".into(), attr(ValueInput::Text("x".into())));
        });
        assert!(e.contains("cannot take"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.insert("scale".into(), attr(ValueInput::Int(-1)));
        });
        assert!(e.contains("u32 range"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.insert("size".into(), attr(ValueInput::Float(f32::NAN)));
        });
        assert!(e.contains("finite"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.get_mut("mass").unwrap().curve = CurveForm::Keys(vec![[0.0, 1.0]]);
        });
        assert!(e.contains("not decoded"), "a curve where the loader's handling is unknown: {e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.get_mut("size").unwrap().curve = CurveForm::Keys(vec![]);
        });
        assert!(e.contains("at least one key"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.get_mut("size").unwrap().curve = CurveForm::Keyword("nil".into());
        });
        assert!(e.contains("neither `none`"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.get_mut("size").unwrap().options = vec![AtrbOption::Bit7, AtrbOption::Bit7];
        });
        assert!(e.contains("listed twice"), "{e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.attributes.get_mut("scale").unwrap().options = vec![AtrbOption::Bit9];
        });
        assert!(e.contains("u32 attribute"), "options on a u32 attribute: {e}");
        let e = lower_err(|f| {
            f.emitters[0].particle.colour.pop();
        });
        assert!(e.contains("99 keys"), "{e}");
        let e = lower_err(|f| f.emitters[0].particle.colour[3].half = WordInput::Int(70000));
        assert!(e.contains("above 65535"), "{e}");
        let e = lower_err(|f| f.emitters[0].particle.frames.clear());
        assert!(e.contains("at least one frame"), "{e}");
        let e = lower_err(|f| f.emitters[0].geom = GeomForm::Some(GeomRef { shape: 1, word: 0 }));
        assert!(e.contains("shape index"), "{e}");
        let e = lower_err(|f| f.emitters[0].geom = GeomForm::None("nothing".into()));
        assert!(e.contains("neither `none`"), "{e}");
        let e = lower_err(|f| f.emitters[0].particle.flags = 4);
        assert!(e.contains("never reads"), "{e}");
        let e = lower_err(|f| f.emitters.clear());
        assert!(e.contains("at least one emitter"), "{e}");
        let e = lower_err(|f| f.shapes.clear());
        assert!(e.contains("EMTR shape"), "{e}");
        let e = lower_err(|f| f.forces[1].attributes.remove("radial").map(|_| ()).unwrap());
        assert!(e.contains("radial") && e.contains("not declared"), "{e}");
        let e = lower_err(|f| f.emitters[0].transform[0][0] = f32::INFINITY);
        assert!(e.contains("finite"), "{e}");
    }

    #[test]
    fn unknown_keys_and_missing_fields_are_refused() {
        let text = to_string(&form(), Format::Yaml).unwrap();
        let e = from_str(&format!("{text}handle: 1\n"), Format::Yaml).unwrap_err();
        assert!(e.contains("handle"), "{e}");
        assert!(text.contains("geom:"), "{text}");
        let e = from_str(&text.replacen("geom:", "gem:", 1), Format::Yaml).unwrap_err();
        assert!(e.contains("gem") || e.contains("geom"), "{e}");
    }
}
