//! The `edit_world` schema — a placement LAYER's entities as editable YAML.
//!
//! Like `states`, the workflow is **extract, then edit**: [`extract`] dumps a layer's placements
//! (each entity's key, name, position, rotation, model), the author changes what they want, and
//! [`parse`] reads it back into a list of edits the lowering applies with `placement::patch_*`.
//! Only the fields present on an edit are written — a bare `pos:` moves without touching rotation or
//! model — so the file is a sparse patch over the extracted baseline, not a full re-authoring.
//!
//! An entity is addressed by its `key` (a quoted `"0xHHHHHHHH"`, the `Transform` record key the
//! extract prints) or by its `name` (matched against the layer's `Name` COMP). Key is unambiguous;
//! name is the convenience.
//!
//! [`parse_entities`] reads the other placement file: the NEW entities an `add_placement` or
//! `add_layer` places, closed to the fields a placement has.

use serde::{Deserialize, Serialize};

/// One entity's edit. All change fields are optional; absent means "leave it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityEdit {
    /// The target: a bare `0xKEY` (the Transform record key) or an entity name.
    pub entity: String,
    /// Move to this world position `[x, y, z]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<[f32; 3]>,
    /// Rotate to this unit quaternion `[x, y, z, w]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quat: Option<[f32; 4]>,
    /// Re-model: a new model asset name or bare `0xHASH` for this placement's `ModelName`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// The whole `edits:` file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDoc {
    pub edits: Vec<EntityEdit>,
}

/// Read an edited world file.
pub fn parse(yaml: &str) -> Result<WorldDoc, String> {
    serde_norway::from_str(yaml).map_err(|e| format!("edit_world file is not valid YAML: {e}"))
}

/// One entity record of an `add_placement` / `add_layer` entity file, as written: the key, the
/// model, the position, an optional orientation (`quat` or `yaw`, never both) and an optional name.
/// A field outside these is refused by name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityRecord {
    /// Quoted hex, `"0xHHHHHHHH"` ([`crate::manifest::entity_key`]).
    #[serde(with = "crate::manifest::entity_key")]
    pub key: u32,
    pub model: String,
    pub pos: [f32; 3],
    #[serde(default)]
    pub quat: Option<[f32; 4]>,
    /// Degrees about Y.
    #[serde(default)]
    pub yaw: Option<f32>,
    #[serde(default)]
    pub name: Option<String>,
}

/// An entity file as written: its records, and whether it is the one-record mapping form rather
/// than a list.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityFile {
    pub single: bool,
    pub records: Vec<EntityRecord>,
}

/// Read an `add_placement` / `add_layer` entity file: one [`EntityRecord`] mapping, or a list of
/// them. Each record is read on its own, so a refused field is named with the record it is in. A
/// record without an orientation is unrotated; one without a name is named `modent_<key>`.
pub fn parse_entities(yaml: &str) -> Result<Vec<mercs2_formats::placement_build::NewEntity>, String> {
    lower_entities(read_entities(yaml)?.records)
}

/// Read an entity file's records as written ([`parse_entities`] without the lowering).
pub fn read_entities(yaml: &str) -> Result<EntityFile, String> {
    let doc: serde_norway::Value =
        serde_norway::from_str(yaml).map_err(|e| format!("entity file is not valid YAML: {e}"))?;
    let (single, records): (bool, Vec<EntityRecord>) = match doc {
        serde_norway::Value::Sequence(items) => (
            false,
            items
                .into_iter()
                .enumerate()
                .map(|(i, v)| serde_norway::from_value(v).map_err(|e| format!("entity {i}: {e}")))
                .collect::<Result<_, _>>()?,
        ),
        serde_norway::Value::Mapping(_) => {
            (true, vec![serde_norway::from_value(doc).map_err(|e| format!("the entity: {e}"))?])
        }
        _ => {
            return Err("an entity file is one entity record (a mapping) or a list of them".into());
        }
    };
    if records.is_empty() {
        return Err("the entity file lists no entities".into());
    }
    Ok(EntityFile { single, records })
}

/// The entities `records` place: a record without an orientation unrotated, `yaw` turned about Y,
/// one without a name named `modent_<key>`; `quat` and `yaw` together refused.
pub fn lower_entities(records: Vec<EntityRecord>) -> Result<Vec<mercs2_formats::placement_build::NewEntity>, String> {
    records
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let quat = match (e.quat, e.yaw) {
                (Some(_), Some(_)) => {
                    return Err(format!(
                        "entity {i} (key 0x{:08X}) gives both `quat` and `yaw`; give one — \
                         `quat: [x, y, z, w]` or `yaw: <degrees>`",
                        e.key
                    ))
                }
                (Some(q), None) => q,
                (None, Some(yaw)) => {
                    let h = yaw.to_radians() * 0.5;
                    [0.0, h.sin(), 0.0, h.cos()]
                }
                (None, None) => [0.0, 0.0, 0.0, 1.0],
            };
            Ok(mercs2_formats::placement_build::NewEntity {
                key: e.key,
                model_hash: crate::manifest::asset_hash(&e.model),
                pos: e.pos,
                quat,
                name: e.name.unwrap_or_else(|| format!("modent_{:08x}", e.key)),
            })
        })
        .collect()
}

/// Write an entity file: the one-record mapping form when `file.single`, else a list, each record's
/// fields in the order `key, model, pos, quat, yaw, name`, a field the record leaves out not written.
/// The text is checked to read back as `file` ([`read_entities`]); a value that cannot be written so
/// it does is refused.
pub fn write_entities(file: &EntityFile) -> Result<String, String> {
    if file.records.is_empty() {
        return Err("an entity file lists at least one entity".into());
    }
    if file.single && file.records.len() != 1 {
        return Err(format!("the one-record form holds one entity, not {}", file.records.len()));
    }
    let mut out = String::new();
    for (i, r) in file.records.iter().enumerate() {
        let ctx = |m: String| format!("entity {i} (key 0x{:08X}): {m}", r.key);
        let mut fields = vec![
            format!("key: \"0x{:08X}\"", r.key),
            format!("model: {}", Reader::Value.scalar(&r.model).map_err(ctx)?),
            format!("pos: {}", floats(&r.pos).map_err(ctx)?),
        ];
        if let Some(q) = &r.quat {
            fields.push(format!("quat: {}", floats(q).map_err(ctx)?));
        }
        if let Some(y) = r.yaw {
            fields.push(format!("yaw: {}", float(y).map_err(ctx)?));
        }
        if let Some(n) = &r.name {
            fields.push(format!("name: {}", Reader::Value.scalar(n).map_err(ctx)?));
        }
        for (j, f) in fields.iter().enumerate() {
            let lead = match (file.single, j) {
                (true, _) => "",
                (false, 0) => "- ",
                (false, _) => "  ",
            };
            out.push_str(lead);
            out.push_str(f);
            out.push('\n');
        }
    }
    match read_entities(&out) {
        Ok(back) if back == *file => Ok(out),
        Ok(back) => Err(format!("the written entity file reads back as {back:?}, not {file:?}")),
        Err(e) => Err(format!("the written entity file does not read back: {e}")),
    }
}

/// The comment line an `edit_world` file starts with.
pub const WORLD_HEADER: &str = "# edit_world baseline — edit any entity's pos / quat / model, drop the rest.";

/// Write an `edit_world` file: [`WORLD_HEADER`], then each edit's `entity`, `pos`, `quat` and `model`,
/// a field the edit leaves out not written. `note(entity)` is the comment written after an edit's
/// `entity` line (the extract writes the entity's name there). The text is checked to read back as
/// `doc` ([`parse`]); a value that cannot be written so it does is refused.
pub fn write_world(doc: &WorldDoc, note: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = format!("{WORLD_HEADER}\nedits:");
    if doc.edits.is_empty() {
        out.push_str(" []");
    }
    out.push('\n');
    for (i, e) in doc.edits.iter().enumerate() {
        let ctx = |m: String| format!("edit {i} ({}): {m}", e.entity);
        // A key is quoted, as the extract has always written it.
        let entity = match crate::manifest::bare_hash(&e.entity) {
            Some(_) => Reader::Field.quoted(&e.entity).map_err(ctx)?,
            None => Reader::Field.scalar(&e.entity).map_err(ctx)?,
        };
        out.push_str(&format!("  - entity: {entity}"));
        if let Some(n) = note(&e.entity) {
            if n.contains(['\n', '\r']) {
                return Err(ctx(format!("the note {n:?} holds a line break, which a comment cannot")));
            }
            out.push_str(&format!("   # {n}"));
        }
        out.push('\n');
        if let Some(p) = &e.pos {
            out.push_str(&format!("    pos: {}\n", floats(p).map_err(ctx)?));
        }
        if let Some(q) = &e.quat {
            out.push_str(&format!("    quat: {}\n", floats(q).map_err(ctx)?));
        }
        if let Some(m) = &e.model {
            // A hash is quoted, as a key is.
            let model = match crate::manifest::bare_hash(m) {
                Some(_) => Reader::Field.quoted(m).map_err(ctx)?,
                None => Reader::Field.scalar(m).map_err(ctx)?,
            };
            out.push_str(&format!("    model: {model}\n"));
        }
    }
    match parse(&out) {
        Ok(back) if back == *doc => Ok(out),
        Ok(back) => Err(format!("the written edit_world file reads back as {back:?}, not {doc:?}")),
        Err(e) => Err(format!("the written edit_world file does not read back: {e}")),
    }
}

/// How a file's reader takes a string field: straight into the field ([`parse`]), or through a YAML
/// value first ([`read_entities`]), where `0x86D7CF92` is an integer and `true` a bool.
#[derive(Clone, Copy)]
enum Reader {
    Field,
    Value,
}

impl Reader {
    /// What `text`, as a YAML mapping value, reads as.
    fn reads(self, text: &str) -> Option<String> {
        let doc = format!("k: {text}\n");
        match self {
            Reader::Field => serde_norway::from_str::<std::collections::BTreeMap<String, String>>(&doc).ok()?.remove("k"),
            Reader::Value => match serde_norway::from_str::<std::collections::BTreeMap<String, serde_norway::Value>>(&doc).ok()?.remove("k")? {
                serde_norway::Value::String(s) => Some(s),
                _ => None,
            },
        }
    }

    /// A string as a YAML mapping value: plain when this reader reads it back as itself, else
    /// double-quoted.
    fn scalar(self, s: &str) -> Result<String, String> {
        if !s.is_empty() && !s.contains(['\n', '\r']) && self.reads(s).as_deref() == Some(s) {
            return Ok(s.to_string());
        }
        self.quoted(s)
    }

    /// A string double-quoted, checked to read back as itself.
    fn quoted(self, s: &str) -> Result<String, String> {
        let q = serde_json::to_string(s).map_err(|e| format!("{s:?}: {e}"))?;
        if self.reads(&q).as_deref() == Some(s) {
            Ok(q)
        } else {
            Err(format!("{s:?} cannot be written as a YAML string that reads back as itself"))
        }
    }
}

fn float(v: f32) -> Result<String, String> {
    if v.is_finite() {
        Ok(fmt_f(v))
    } else {
        Err(format!("{v} is not a finite number"))
    }
}

fn floats(v: &[f32]) -> Result<String, String> {
    Ok(format!("[{}]", v.iter().map(|&x| float(x)).collect::<Result<Vec<_>, _>>()?.join(", ")))
}

/// A placement as the extractor sees it — enough to write a baseline the author edits.
pub struct Placed {
    pub key: u32,
    pub name: Option<String>,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub model: Option<u32>,
}

/// Dump a layer's placements to the editable baseline ([`write_world`]). `model_name` reverses a
/// model hash to a name so the file reads in vocabulary, and a model it does not reverse is its
/// quoted hash; the entity `key` is always emitted as a quoted hash (it is the stable address), with
/// the entity name as a trailing comment.
pub fn extract(places: &[Placed], model_name: impl Fn(u32) -> Option<String>) -> Result<String, String> {
    let doc = WorldDoc {
        edits: places
            .iter()
            .map(|p| EntityEdit {
                entity: format!("0x{:08X}", p.key),
                pos: Some(p.pos),
                quat: Some(p.quat),
                model: p.model.map(|m| model_name(m).unwrap_or_else(|| format!("0x{m:08X}"))),
            })
            .collect(),
    };
    let names: std::collections::HashMap<u32, &str> =
        places.iter().filter_map(|p| p.name.as_deref().map(|n| (p.key, n))).collect();
    write_world(&doc, |entity| {
        crate::manifest::bare_hash(entity).and_then(|k| names.get(&k)).map(|n| n.to_string())
    })
}

/// Format an f32 without a trailing `.0`-less integer surprise, keeping YAML happy and diffs small.
fn fmt_f(v: f32) -> String {
    if v == v.trunc() && v.is_finite() {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sparse_edit_parses_with_only_the_fields_present() {
        let yaml = "edits:\n  - entity: \"0x000C740D\"\n    pos: [1.0, 2.0, 3.0]\n  - entity: guardpost\n    model: pmc_hum_mattias\n";
        let d = parse(yaml).unwrap();
        assert_eq!(d.edits.len(), 2);
        assert_eq!(d.edits[0].entity, "0x000C740D");
        assert_eq!(d.edits[0].pos, Some([1.0, 2.0, 3.0]));
        assert!(d.edits[0].quat.is_none() && d.edits[0].model.is_none());
        assert_eq!(d.edits[1].model.as_deref(), Some("pmc_hum_mattias"));
    }

    /// One record or a list; `yaw` turns about Y; a record with no orientation is unrotated and one
    /// with no name is named by its key.
    #[test]
    fn an_entity_file_is_one_record_or_a_list() {
        let one = parse_entities("key: \"0x0F00A001\"\nmodel: global_barrela\npos: [1, 2, 3]\n").unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].key, 0x0F00_A001);
        assert_eq!(one[0].model_hash, crate::manifest::asset_hash("global_barrela"));
        assert_eq!(one[0].pos, [1.0, 2.0, 3.0]);
        assert_eq!(one[0].quat, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(one[0].name, "modent_0f00a001");

        let list = parse_entities(
            "- { key: \"0x1\", model: a, pos: [0, 0, 0], yaw: 180, name: first }\n\
             - { key: \"0x2\", model: b, pos: [0, 0, 0], quat: [0, 0, 1, 0] }\n",
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "first");
        assert!((list[0].quat[1] - 1.0).abs() < 1e-6 && list[0].quat[3].abs() < 1e-6, "{:?}", list[0].quat);
        assert_eq!(list[1].quat, [0.0, 0.0, 1.0, 0.0]);
    }

    /// The schema is closed: a field a placement does not have is refused BY NAME, in a single
    /// record and in a list — `position` / `orientation` are not fields.
    #[test]
    fn an_unknown_entity_field_is_refused_by_name() {
        let e = parse_entities("key: \"0x1\"\nmodel: a\npos: [0, 0, 0]\nposition: [0, 0, 0]\n").unwrap_err();
        assert!(e.contains("unknown field `position`"), "{e}");
        let e = parse_entities("- { key: \"0x1\", model: a, pos: [0, 0, 0] }\n- { key: \"0x2\", model: a, pos: [0, 0, 0], orientation: 9 }\n")
            .unwrap_err();
        assert!(e.starts_with("entity 1:") && e.contains("unknown field `orientation`"), "{e}");
    }

    /// `quat` and `yaw` together are refused rather than one silently winning.
    #[test]
    fn quat_and_yaw_together_are_refused() {
        let e = parse_entities("key: \"0x7\"\nmodel: a\npos: [0, 0, 0]\nquat: [0, 0, 0, 1]\nyaw: 90\n").unwrap_err();
        assert!(e.contains("both `quat` and `yaw`") && e.contains("0x00000007"), "{e}");
    }

    /// A key is quoted hex: a number, hex or decimal, is refused with the quoted spelling of its
    /// value, and a string that is not hex is refused.
    #[test]
    fn a_key_is_quoted_hex() {
        for (text, spelled) in [("0x0F00A001", "\"0x0F00A001\""), ("251699201", "\"0x0F00A001\"")] {
            let e = parse_entities(&format!("key: {text}\nmodel: a\npos: [0, 0, 0]\n")).unwrap_err();
            assert!(e.contains("is a number") && e.contains(spelled), "{text}: {e}");
        }
        let e = parse_entities("key: 0x1F00000000\nmodel: a\npos: [0, 0, 0]\n").unwrap_err();
        assert!(e.contains("not a 32-bit key"), "{e}");
        let e = parse_entities("key: \"crate\"\nmodel: a\npos: [0, 0, 0]\n").unwrap_err();
        assert!(e.contains("\"crate\" is not \"0x\""), "{e}");
        assert_eq!(parse_entities("key: \"0x1f\"\nmodel: a\npos: [0, 0, 0]\n").unwrap()[0].key, 0x1F);
    }

    /// A file that is not YAML, an empty list, and a scalar are refused.
    #[test]
    fn an_unusable_entity_file_is_refused() {
        assert!(parse_entities("key: [\n").unwrap_err().contains("not valid YAML"));
        assert!(parse_entities("[]\n").unwrap_err().contains("no entities"));
        assert!(parse_entities("42\n").unwrap_err().contains("one entity record"));
        assert!(parse_entities("key: \"0x1\"\nmodel: a\n").unwrap_err().contains("missing field `pos`"));
    }

    #[test]
    fn extract_then_parse_round_trips_the_positions() {
        let places = vec![
            Placed { key: 0x000C_740D, name: Some("recruitjet 0x000c740d".into()), pos: [10.0, -3.5, 7.0], quat: [0.0, 0.0, 0.0, 1.0], model: Some(0x86D7_CF92) },
            Placed { key: 0x1111_2222, name: None, pos: [0.0, 0.0, 0.0], quat: [0.0, 0.0, 0.0, 1.0], model: None },
        ];
        let yaml = extract(&places, |h| (h == 0x86D7_CF92).then(|| "recruitjet".to_string())).unwrap();
        assert!(yaml.contains("0x000C740D") && yaml.contains("recruitjet"), "{yaml}");
        let d = parse(&yaml).unwrap();
        assert_eq!(d.edits.len(), 2);
        assert_eq!(d.edits[0].pos, Some([10.0, -3.5, 7.0]));
        assert_eq!(d.edits[0].model.as_deref(), Some("recruitjet"));
    }

    /// The extract's form: a key quoted, and a model the names do not reverse written as its quoted
    /// hash, which reads back as the text `0x…` the lowering hashes.
    #[test]
    fn an_unnamed_model_extracts_as_its_quoted_hash() {
        let places = vec![Placed { key: 1, name: None, pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 1.0], model: Some(0x86D7_CF92) }];
        let yaml = extract(&places, |_| None).unwrap();
        assert_eq!(yaml, format!("{WORLD_HEADER}\nedits:\n  - entity: \"0x00000001\"\n    pos: [0.0, 0.0, 0.0]\n    quat: [0.0, 0.0, 0.0, 1.0]\n    model: \"0x86D7CF92\"\n"));
        assert_eq!(parse(&yaml).unwrap().edits[0].model.as_deref(), Some("0x86D7CF92"));
    }

    /// The extract is what `write_world` writes: its own text, read and written again, is the same
    /// bytes, and a sparse edit writes only its fields.
    #[test]
    fn an_edit_world_file_writes_back_as_it_was() {
        let places = vec![
            Placed { key: 0x000C_740D, name: Some("recruitjet 0x000c740d".into()), pos: [10.0, -3.5, 7.25], quat: [0.0, 0.70710677, 0.0, 0.70710677], model: Some(0x86D7_CF92) },
            Placed { key: 2, name: None, pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 1.0], model: None },
        ];
        let names: std::collections::HashMap<String, String> =
            [("0x000C740D".to_string(), "recruitjet 0x000c740d".to_string())].into();
        let yaml = extract(&places, |h| (h == 0x86D7_CF92).then(|| "recruitjet".to_string())).unwrap();
        assert_eq!(write_world(&parse(&yaml).unwrap(), |e| names.get(e).cloned()).unwrap(), yaml);

        let sparse = WorldDoc {
            edits: vec![EntityEdit { entity: "guardpost".into(), pos: Some([1.0, 2.0, 3.0]), quat: None, model: None }],
        };
        let text = write_world(&sparse, |_| None).unwrap();
        assert_eq!(text, format!("{WORLD_HEADER}\nedits:\n  - entity: guardpost\n    pos: [1.0, 2.0, 3.0]\n"));
        assert_eq!(write_world(&WorldDoc::default(), |_| None).unwrap(), format!("{WORLD_HEADER}\nedits: []\n"));
        assert!(write_world(&sparse, |_| Some("a\nb".into())).unwrap_err().contains("line break"));
    }

    /// Both forms write back as themselves; a name or model YAML would read as something else is
    /// quoted; what cannot be written is refused.
    #[test]
    fn an_entity_file_writes_back_as_it_was() {
        let one = "key: \"0x0F00A001\"\nmodel: global_barrela\npos: [1.0, 2.0, 3.5]\nyaw: 90.0\nname: first\n";
        assert_eq!(write_entities(&read_entities(one).unwrap()).unwrap(), one);
        // qm reads an entity file through a YAML value, so a hash model is quoted to stay a string.
        let list = "- key: \"0x00000001\"\n  model: a\n  pos: [0.0, 0.0, 0.0]\n  quat: [0.0, 0.0, 1.0, 0.0]\n- key: \"0x00000002\"\n  model: \"0x86D7CF92\"\n  pos: [-1.5, 0.0, 2.0]\n  name: \"crate: east #2\"\n";
        let bare = read_entities("key: \"0x1\"\nmodel: 0x86D7CF92\npos: [0, 0, 0]\n").unwrap_err();
        assert!(bare.contains("invalid type: integer"), "{bare}");
        let f = read_entities(list).unwrap();
        assert!(!f.single && f.records[1].model == "0x86D7CF92" && f.records[1].name.as_deref() == Some("crate: east #2"));
        assert_eq!(write_entities(&f).unwrap(), list);

        let mut bad = read_entities(one).unwrap();
        bad.records[0].pos[1] = f32::NAN;
        assert!(write_entities(&bad).unwrap_err().contains("not a finite number"));
        bad.records[0].pos[1] = 0.0;
        bad.records.push(bad.records[0].clone());
        assert!(write_entities(&bad).unwrap_err().contains("one-record form"));
    }
}
