//! The template author form: a world template declared field by field, checked against the
//! `worldentity` container's own schemas.
//!
//! ```yaml
//! name: qm_gate_c4
//! name_flag: 1
//! components:
//!   RedEffectComponent:
//!     name: global_explosion_c4        # a field by name; 0x1DE5C824 names the same field
//!     "0x4D7D459B": 1.0
//!     ...
//!   Label:                             # a class held more than once: a list, one map per record
//!     - { "0xFD084CCE": "0x3B5C3BAF" }
//!     - { "0xFD084CCE": "0x9A1C21F3" }
//! ```
//!
//! The same document reads from YAML, JSON or TOML ([`crate::Format`]).
//!
//! * A class is one of the container's component classes, and not `Name`: the name and its flag
//!   are `name` and `name_flag`.
//! * A field key is the field's name (hashed with `pandemic_hash_m2`) or its hash as `0xHHHHHHHH`.
//!   Every field of the class's `schm` is given exactly once; a missing, unknown or repeated field
//!   is an error.
//! * A value is checked against the field's schm type and bit field:
//!
//! | Field | Value |
//! |---|---|
//! | bit field, 1 bit wide | `true` / `false` |
//! | bit field, `w` bits wide | an integer below `2^w` |
//! | `Byte`, `U8` (codes 1, 2) | an integer 0–255 |
//! | `Short`, `U16` (codes 3, 4) | an integer 0–65535 |
//! | `Int` (code 5) | an integer in the `i32` range |
//! | `Hash`, `Enum` (codes 6, 9) | a name (hashed) or `0xHHHHHHHH` |
//! | `F32` (code 7) | a finite number |
//! | `Vec3` (code 10) | three finite numbers |
//! | `Blob32` (code 11) | eight finite numbers |
//! | `PointLocation`'s text, inline string | a string |
//!
//! There are no defaults: what is not declared is an error, and nothing is copied from another
//! template. [`TemplateForm::lower`] turns a form into a [`TemplateDecl`] for
//! [`WorldEntity::append_template`]; [`TemplateForm::express`] writes an existing template as a form.

use std::collections::BTreeMap;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::schema::{FieldValue, SchemaField, SchemaFieldType};
use mercs2_formats::worldentity::{
    Component, ComponentDecl, Layout, Payload, TemplateDecl, Value, WorldEntity,
};
use serde::{Deserialize, Serialize};

use crate::Format;

/// A template, as authored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateForm {
    /// The template name `Pg.Spawn` and `ObjectState.StartEmitter` look it up by.
    pub name: String,
    /// The `Name` record's flag byte.
    pub name_flag: u8,
    /// Every component, by class.
    pub components: BTreeMap<String, Records>,
}

/// One class's records: one map, or a list of maps for a class held more than once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Records {
    One(BTreeMap<String, FieldInput>),
    Many(Vec<BTreeMap<String, FieldInput>>),
}

/// An authored value, before it is checked against its field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldInput {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<f64>),
}

/// Parse a form from text.
pub fn from_str(text: &str, format: Format) -> Result<TemplateForm, String> {
    match format {
        Format::Yaml => serde_norway::from_str(text).map_err(|e| e.to_string()),
        Format::Json => serde_json::from_str(text).map_err(|e| e.to_string()),
        Format::Toml => toml::from_str(text).map_err(|e| e.to_string()),
    }
    .map_err(|e| format!("template form ({format:?}): {e}"))
}

/// Write a form as text.
pub fn to_string(form: &TemplateForm, format: Format) -> Result<String, String> {
    match format {
        Format::Yaml => serde_norway::to_string(form).map_err(|e| e.to_string()),
        Format::Json => serde_json::to_string_pretty(form).map_err(|e| e.to_string()),
        Format::Toml => toml::to_string(form).map_err(|e| e.to_string()),
    }
}

/// A field key's hash: `0xHHHHHHHH` is the hash, anything else is a name.
fn field_hash(key: &str) -> u32 {
    hex_hash(key).unwrap_or_else(|| pandemic_hash_m2(key))
}

fn hex_hash(s: &str) -> Option<u32> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if h.is_empty() || h.len() > 8 {
        return None;
    }
    u32::from_str_radix(h, 16).ok()
}

fn hex(v: u32) -> String {
    format!("0x{v:08X}")
}

fn describe(f: &SchemaField) -> String {
    if f.bit_width > 0 {
        format!("{:?} bits {}+{}", f.field_type, f.bit_start, f.bit_width)
    } else {
        format!("{:?}", f.field_type)
    }
}

fn finite(class: &str, key: &str, x: f64) -> Result<f32, String> {
    let f = x as f32;
    if !f.is_finite() {
        return Err(format!("{class}.{key}: {x} is not a finite f32"));
    }
    Ok(f)
}

fn number(v: &FieldInput) -> Option<f64> {
    match v {
        FieldInput::Int(i) => Some(*i as f64),
        FieldInput::Float(x) => Some(*x),
        _ => None,
    }
}

fn floats<const N: usize>(class: &str, key: &str, v: &FieldInput) -> Result<[f32; N], String> {
    let FieldInput::List(xs) = v else {
        return Err(format!("{class}.{key}: takes a list of {N} numbers, got {v:?}"));
    };
    if xs.len() != N {
        return Err(format!("{class}.{key}: takes {N} numbers, got {}", xs.len()));
    }
    let mut a = [0f32; N];
    for (s, x) in a.iter_mut().zip(xs) {
        *s = finite(class, key, *x)?;
    }
    Ok(a)
}

/// Check one authored value against its field and type it.
fn typed(class: &str, key: &str, f: &SchemaField, text: bool, v: &FieldInput) -> Result<Value, String> {
    let wrong = || format!("{class}.{key} ({}): cannot take {v:?}", describe(f));
    if text {
        return match v {
            FieldInput::Text(s) => Ok(Value::Text(s.clone())),
            _ => Err(wrong()),
        };
    }
    let int = |max: i64| -> Result<i64, String> {
        match v {
            FieldInput::Int(i) if (0..=max).contains(i) => Ok(*i),
            FieldInput::Int(i) => Err(format!("{class}.{key} ({}): {i} is outside 0..={max}", describe(f))),
            _ => Err(wrong()),
        }
    };
    if f.bit_width > 0 {
        if f.bit_width == 1 {
            return match v {
                FieldInput::Bool(b) => Ok(Value::Field(FieldValue::Bits(*b as u32))),
                _ => Err(wrong()),
            };
        }
        return Ok(Value::Field(FieldValue::Bits(int((1i64 << f.bit_width) - 1)? as u32)));
    }
    use SchemaFieldType as T;
    Ok(Value::Field(match f.field_type {
        T::Byte | T::U8 => FieldValue::U8(int(0xFF)? as u8),
        T::Short | T::U16 => FieldValue::U16(int(0xFFFF)? as u16),
        T::Int => match v {
            FieldInput::Int(i) if (i32::MIN as i64..=i32::MAX as i64).contains(i) => {
                FieldValue::U32(*i as i32 as u32)
            }
            FieldInput::Int(i) => {
                return Err(format!("{class}.{key} (Int): {i} is outside the i32 range"))
            }
            _ => return Err(wrong()),
        },
        T::Hash | T::Enum => match v {
            FieldInput::Text(s) if !s.is_empty() => FieldValue::U32(field_hash(s)),
            _ => return Err(wrong()),
        },
        T::F32 => FieldValue::F32(finite(class, key, number(v).ok_or_else(wrong)?)?),
        T::Vec3 => FieldValue::Vec3(floats::<3>(class, key, v)?),
        T::Blob32 => FieldValue::Blob32(floats::<8>(class, key, v)?),
        T::StringRef => return Err(wrong()),
    }))
}

/// Which fields of a class's layout are inline strings.
fn text_fields(c: &Component) -> Vec<bool> {
    match c.layout {
        Layout::PointLocation => vec![false, true],
        Layout::Name => vec![true, false],
        Layout::Fixed | Layout::PackedU16 => vec![false; c.schema.fields.len()],
    }
}

impl TemplateForm {
    /// Check every component, field and value against the container's schemas and type them,
    /// for `key`. Errors name the class, the field and what it takes.
    pub fn lower(&self, we: &WorldEntity, key: u32) -> Result<TemplateDecl, String> {
        let mut components = Vec::new();
        for (class, records) in &self.components {
            if class == "Name" {
                return Err("Name: the template's name is `name` and `name_flag`, not a component".into());
            }
            let gi = we.append_group(class)?;
            let comp = &we.components[gi];
            let maps: Vec<&BTreeMap<String, FieldInput>> = match records {
                Records::One(m) => vec![m],
                Records::Many(v) if v.is_empty() => {
                    return Err(format!("{class}: an empty list declares no record"))
                }
                Records::Many(v) => v.iter().collect(),
            };
            let texts = text_fields(comp);
            for m in maps {
                let mut by_hash: BTreeMap<u32, (&String, &FieldInput)> = BTreeMap::new();
                for (k, v) in m {
                    if let Some((prev, _)) = by_hash.insert(field_hash(k), (k, v)) {
                        return Err(format!("{class}: `{prev}` and `{k}` name the same field"));
                    }
                }
                let mut values = Vec::with_capacity(comp.schema.fields.len());
                for (f, &text) in comp.schema.fields.iter().zip(&texts) {
                    let (k, v) = by_hash.remove(&f.name_hash).ok_or_else(|| {
                        format!(
                            "{class}: field {} ({}) is not declared; every field of the class is",
                            hex(f.name_hash),
                            describe(f)
                        )
                    })?;
                    values.push(typed(class, k, f, text, v)?);
                }
                if let Some((_, (k, _))) = by_hash.into_iter().next() {
                    let known: Vec<String> =
                        comp.schema.fields.iter().map(|f| hex(f.name_hash)).collect();
                    return Err(format!(
                        "{class}: `{k}` is not one of its fields ({})",
                        known.join(", ")
                    ));
                }
                comp.encode(&values).map_err(|e| format!("{class}: {e}"))?;
                components.push(ComponentDecl { class: class.clone(), values });
            }
        }
        Ok(TemplateDecl { name: self.name.clone(), key, name_flag: self.name_flag, components })
    }

    /// Write the template `key` of `we` as a form: every record of every class it has.
    pub fn express(we: &WorldEntity, key: u32) -> Result<TemplateForm, String> {
        let mut name = None;
        let mut components: BTreeMap<String, Vec<BTreeMap<String, FieldInput>>> = BTreeMap::new();
        for comp in &we.components {
            for r in comp.records.iter().filter(|r| r.keys.contains(&key)) {
                if let Payload::Name { name: n, flag } = &r.payload {
                    if name.replace((n.clone(), *flag)).is_some() {
                        return Err(format!("key {}: more than one Name record", hex(key)));
                    }
                    continue;
                }
                let values = comp.decode(&r.payload)?;
                let mut m = BTreeMap::new();
                for (f, v) in comp.schema.fields.iter().zip(values) {
                    m.insert(hex(f.name_hash), input_of(f, &v)?);
                }
                components.entry(comp.class.clone()).or_default().push(m);
            }
        }
        let (name, name_flag) = name.ok_or_else(|| format!("key {}: no Name record", hex(key)))?;
        Ok(TemplateForm {
            name,
            name_flag,
            components: components
                .into_iter()
                .map(|(c, mut v)| (c, if v.len() == 1 { Records::One(v.pop().unwrap()) } else { Records::Many(v) }))
                .collect(),
        })
    }
}

/// The authored value that types back to `v` for field `f`.
fn input_of(f: &SchemaField, v: &Value) -> Result<FieldInput, String> {
    use SchemaFieldType as T;
    Ok(match v {
        Value::Text(s) => FieldInput::Text(s.clone()),
        Value::Field(FieldValue::Bits(x)) if f.bit_width == 1 => FieldInput::Bool(*x == 1),
        Value::Field(FieldValue::Bits(x)) => FieldInput::Int(*x as i64),
        Value::Field(FieldValue::U8(x)) => FieldInput::Int(*x as i64),
        Value::Field(FieldValue::U16(x)) => FieldInput::Int(*x as i64),
        Value::Field(FieldValue::U32(x)) if f.field_type == T::Int => FieldInput::Int(*x as i32 as i64),
        Value::Field(FieldValue::U32(x)) => FieldInput::Text(hex(*x)),
        Value::Field(FieldValue::F32(x)) => FieldInput::Float(*x as f64),
        Value::Field(FieldValue::Vec3(a)) => FieldInput::List(a.iter().map(|&x| x as f64).collect()),
        Value::Field(FieldValue::Blob32(a)) => FieldInput::List(a.iter().map(|&x| x as f64).collect()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::ucfx::{write_ucfx_tree, UcfxNode};

    fn field(code: u32, hash: u32, off: u16, start: u8, width: u8) -> Vec<u8> {
        let mut e = Vec::new();
        for w in [code, hash, 0] {
            e.extend_from_slice(&w.to_le_bytes());
        }
        e.extend_from_slice(&off.to_le_bytes());
        e.push(start);
        e.push(width);
        e
    }

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn comp(class: &str, version: u32, count: u32, fields: &[Vec<u8>], stride: u32, data: Vec<u8>) -> UcfxNode {
        let mut info = class.as_bytes().to_vec();
        info.push(0);
        info.extend_from_slice(&le(&[pandemic_hash_m2(class), version, count, 0]));
        let mut schm = le(&[fields.len() as u32, stride]);
        for f in fields {
            schm.extend_from_slice(f);
        }
        UcfxNode::marker(
            *b"COMP",
            vec![
                UcfxNode::leaf(*b"info", info),
                UcfxNode::leaf(*b"schm", schm),
                UcfxNode::leaf(*b"data", data),
            ],
        )
    }

    /// A container with a RedEffectComponent-shaped class (hash, f32, int, enum, vec3, a 1-bit
    /// and a 3-bit field), Name, and a flgt listing the first.
    fn container() -> WorldEntity {
        let fields = [
            field(6, 0x1DE5_C824, 0, 0, 0),
            field(7, 0x1111_0001, 4, 0, 0),
            field(5, 0x1111_0002, 8, 0, 0),
            field(9, 0x1111_0003, 12, 0, 0),
            field(10, 0x1111_0004, 16, 0, 0),
            field(5, 0x1111_0005, 28, 0, 1),
            field(5, 0x1111_0006, 28, 1, 3),
        ];
        let mut d = le(&[1, 0x8000_0002, 0x41B4_326E, 1f32.to_bits(), (-2i32) as u32, 0, 0, 0, 0, 0b1011]);
        assert_eq!(d.len(), 8 + 32);
        let mut names = le(&[1, 0x8000_0002]);
        names.extend_from_slice(b"box\0\0");
        let name_fields = [field(8, 0x1DE5_C824, 0, 0, 0), field(1, 0x12AF_A0B8, 4, 0, 0)];
        let mut flgt = le(&[1, pandemic_hash_m2("RedEffectComponent")]);
        flgt.extend_from_slice(b"RedEffectComponent\0");
        flgt.extend_from_slice(&le(&[0]));
        let mut flgs = le(&[1, 0x8000_0002, 1]);
        flgs.extend_from_slice(&[0; 28]);
        let mut chdr = vec![0, 0, 0x33, 0];
        chdr.extend_from_slice(&le(&[1]));
        d.truncate(40);
        let bytes = write_ucfx_tree(&[
            UcfxNode::leaf(*b"CHDR", chdr),
            UcfxNode::leaf(*b"enum", le(&[0])),
            UcfxNode::leaf(*b"UNIQ", le(&[1, 0x8000_0002])),
            comp("RedEffectComponent", 0x57, 1, &fields, 32, d),
            comp("Name", 1, 1, &name_fields, 5, names),
            UcfxNode::leaf(*b"flgt", flgt),
            UcfxNode::leaf(*b"flgs", flgs),
        ]);
        WorldEntity::parse(&bytes).expect("fixture parses")
    }

    const FORM: &str = r#"
name: qm_gate_c4
name_flag: 1
components:
  RedEffectComponent:
    name: global_explosion_c4
    "0x11110001": 1.5
    "0x11110002": -2
    "0x11110003": "0x00000001"
    "0x11110004": [1.0, 2.0, 3.0]
    "0x11110005": true
    "0x11110006": 5
"#;

    #[test]
    fn a_form_lowers_to_a_typed_declaration_and_appends() {
        let mut we = container();
        let form = from_str(FORM, Format::Yaml).unwrap();
        let t = form.lower(&we, 0x8000_0100).unwrap();
        assert_eq!(t.components.len(), 1);
        assert_eq!(t.components[0].values[0], Value::Field(FieldValue::U32(0x41B4_326E)));
        assert_eq!(t.components[0].values[2], Value::Field(FieldValue::U32(0xFFFF_FFFE)));
        assert_eq!(t.components[0].values[6], Value::Field(FieldValue::Bits(5)));
        we.append_template(&t).unwrap();
        let back = WorldEntity::parse(&we.write().unwrap()).unwrap();
        let again = TemplateForm::express(&back, 0x8000_0100).unwrap();
        assert_eq!(again.lower(&back, 0x8000_0100).unwrap().components, t.components);
        assert_eq!(again.name, "qm_gate_c4");
    }

    #[test]
    fn the_same_form_reads_from_json_and_toml() {
        let we = container();
        let yaml = from_str(FORM, Format::Yaml).unwrap();
        for f in [Format::Json, Format::Toml] {
            let text = to_string(&yaml, f).unwrap();
            let back = from_str(&text, f).unwrap();
            assert_eq!(back.lower(&we, 0x8000_0100).unwrap(), yaml.lower(&we, 0x8000_0100).unwrap(), "{f:?}");
        }
    }

    fn lower_err(edit: impl Fn(&mut TemplateForm)) -> String {
        let we = container();
        let mut form = from_str(FORM, Format::Yaml).unwrap();
        edit(&mut form);
        form.lower(&we, 0x8000_0100).unwrap_err()
    }

    fn red(form: &mut TemplateForm) -> &mut BTreeMap<String, FieldInput> {
        match form.components.get_mut("RedEffectComponent").unwrap() {
            Records::One(m) => m,
            Records::Many(_) => unreachable!(),
        }
    }

    #[test]
    fn every_field_once_and_typed() {
        let e = lower_err(|f| {
            red(f).remove("0x11110001");
        });
        assert!(e.contains("0x11110001") && e.contains("not declared"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x99999999".into(), FieldInput::Int(1));
        });
        assert!(e.contains("not one of its fields"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x1DE5C824".into(), FieldInput::Text("x".into()));
        });
        assert!(e.contains("name the same field"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110001".into(), FieldInput::Text("x".into()));
        });
        assert!(e.contains("cannot take"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110005".into(), FieldInput::Int(1));
        });
        assert!(e.contains("cannot take"), "a 1-bit field takes a bool: {e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110006".into(), FieldInput::Int(8));
        });
        assert!(e.contains("outside 0..=7"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110002".into(), FieldInput::Int(1 << 40));
        });
        assert!(e.contains("i32 range"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110004".into(), FieldInput::List(vec![1.0, 2.0]));
        });
        assert!(e.contains("takes 3 numbers"), "{e}");
        let e = lower_err(|f| {
            red(f).insert("0x11110001".into(), FieldInput::Float(f64::INFINITY));
        });
        assert!(e.contains("finite"), "{e}");
        let e = lower_err(|f| {
            let m = red(f).clone();
            f.components.insert("Missing".into(), Records::One(m));
        });
        assert!(e.contains("no component group"), "{e}");
        let e = lower_err(|f| {
            f.components.insert("Name".into(), Records::Many(vec![]));
        });
        assert!(e.contains("`name` and `name_flag`"), "{e}");
    }

    #[test]
    fn unknown_top_level_keys_are_refused() {
        let e = from_str("name: a\nname_flag: 0\ncomponents: {}\nhandle: 5\n", Format::Yaml).unwrap_err();
        assert!(e.contains("handle"), "{e}");
        assert!(from_str("name: a\ncomponents: {}\n", Format::Yaml).unwrap_err().contains("name_flag"));
    }
}
