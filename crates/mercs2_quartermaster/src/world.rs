//! The `edit_world` schema — a placement LAYER's entities as editable YAML.
//!
//! Like `states`, the workflow is **extract, then edit**: [`extract`] dumps a layer's placements
//! (each entity's key, name, position, rotation, model), the author changes what they want, and
//! [`parse`] reads it back into a list of edits the lowering applies with `placement::patch_*`.
//! Only the fields present on an edit are written — a bare `pos:` moves without touching rotation or
//! model — so the file is a sparse patch over the extracted baseline, not a full re-authoring.
//!
//! An entity is addressed by its `key` (a bare `0xHHHHHHHH`, the `Transform` record key the extract
//! prints) or by its `name` (matched against the layer's `Name` COMP). Key is unambiguous; name is
//! the convenience.
//!
//! [`parse_entities`] reads the other placement file: the NEW entities an `add_placement` or
//! `add_layer` places, closed to the fields a placement has.

use serde::{Deserialize, Serialize};

/// One entity's edit. All change fields are optional; absent means "leave it".
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDoc {
    pub edits: Vec<EntityEdit>,
}

/// Read an edited world file.
pub fn parse(yaml: &str) -> Result<WorldDoc, String> {
    serde_norway::from_str(yaml).map_err(|e| format!("edit_world file is not valid YAML: {e}"))
}

/// One entity record of an `add_placement` / `add_layer` entity file: the key, the model, the
/// position, an optional orientation (`quat` or `yaw`, never both) and an optional name. A field
/// outside these is refused by name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityRecord {
    key: u32,
    model: String,
    pos: [f32; 3],
    #[serde(default)]
    quat: Option<[f32; 4]>,
    /// Degrees about Y.
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    name: Option<String>,
}

/// Read an `add_placement` / `add_layer` entity file: one [`EntityRecord`] mapping, or a list of
/// them. Each record is read on its own, so a refused field is named with the record it is in. A
/// record without an orientation is unrotated; one without a name is named `modent_<key>`.
pub fn parse_entities(yaml: &str) -> Result<Vec<mercs2_formats::placement_build::NewEntity>, String> {
    let doc: serde_norway::Value =
        serde_norway::from_str(yaml).map_err(|e| format!("entity file is not valid YAML: {e}"))?;
    let records: Vec<EntityRecord> = match doc {
        serde_norway::Value::Sequence(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| serde_norway::from_value(v).map_err(|e| format!("entity {i}: {e}")))
            .collect::<Result<_, _>>()?,
        serde_norway::Value::Mapping(_) => {
            vec![serde_norway::from_value(doc).map_err(|e| format!("the entity: {e}"))?]
        }
        _ => {
            return Err("an entity file is one entity record (a mapping) or a list of them".into());
        }
    };
    if records.is_empty() {
        return Err("the entity file lists no entities".into());
    }
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

/// A placement as the extractor sees it — enough to write a baseline the author edits.
pub struct Placed {
    pub key: u32,
    pub name: Option<String>,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub model: Option<u32>,
}

/// Dump a layer's placements to the editable baseline. `model_name` reverses a model hash to a name
/// so the file reads in vocabulary; the entity `key` is always emitted as a bare hash (it is the
/// stable address), with the entity name as a trailing comment.
pub fn extract(places: &[Placed], model_name: impl Fn(u32) -> Option<String>) -> String {
    let mut out = String::from("# edit_world baseline — edit any entity's pos / quat / model, drop the rest.\nedits:\n");
    for p in places {
        out.push_str(&format!("  - entity: \"0x{:08X}\"", p.key));
        if let Some(n) = &p.name {
            out.push_str(&format!("   # {n}"));
        }
        out.push('\n');
        out.push_str(&format!(
            "    pos: [{}, {}, {}]\n",
            fmt_f(p.pos[0]),
            fmt_f(p.pos[1]),
            fmt_f(p.pos[2])
        ));
        out.push_str(&format!(
            "    quat: [{}, {}, {}, {}]\n",
            fmt_f(p.quat[0]),
            fmt_f(p.quat[1]),
            fmt_f(p.quat[2]),
            fmt_f(p.quat[3])
        ));
        if let Some(m) = p.model {
            let mv = model_name(m).unwrap_or_else(|| format!("0x{m:08X}"));
            out.push_str(&format!("    model: {mv}\n"));
        }
    }
    out
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
        let one = parse_entities("key: 0x0F00A001\nmodel: al_prop_barrel_red\npos: [1, 2, 3]\n").unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].key, 0x0F00_A001);
        assert_eq!(one[0].model_hash, crate::manifest::asset_hash("al_prop_barrel_red"));
        assert_eq!(one[0].pos, [1.0, 2.0, 3.0]);
        assert_eq!(one[0].quat, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(one[0].name, "modent_0f00a001");

        let list = parse_entities(
            "- { key: 1, model: a, pos: [0, 0, 0], yaw: 180, name: first }\n\
             - { key: 2, model: b, pos: [0, 0, 0], quat: [0, 0, 1, 0] }\n",
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
        let e = parse_entities("key: 1\nmodel: a\npos: [0, 0, 0]\nposition: [0, 0, 0]\n").unwrap_err();
        assert!(e.contains("unknown field `position`"), "{e}");
        let e = parse_entities("- { key: 1, model: a, pos: [0, 0, 0] }\n- { key: 2, model: a, pos: [0, 0, 0], orientation: 9 }\n")
            .unwrap_err();
        assert!(e.starts_with("entity 1:") && e.contains("unknown field `orientation`"), "{e}");
    }

    /// `quat` and `yaw` together are refused rather than one silently winning.
    #[test]
    fn quat_and_yaw_together_are_refused() {
        let e = parse_entities("key: 7\nmodel: a\npos: [0, 0, 0]\nquat: [0, 0, 0, 1]\nyaw: 90\n").unwrap_err();
        assert!(e.contains("both `quat` and `yaw`") && e.contains("0x00000007"), "{e}");
    }

    /// A file that is not YAML, an empty list, and a scalar are refused.
    #[test]
    fn an_unusable_entity_file_is_refused() {
        assert!(parse_entities("key: [\n").unwrap_err().contains("not valid YAML"));
        assert!(parse_entities("[]\n").unwrap_err().contains("no entities"));
        assert!(parse_entities("42\n").unwrap_err().contains("one entity record"));
        assert!(parse_entities("key: 1\nmodel: a\n").unwrap_err().contains("missing field `pos`"));
    }

    #[test]
    fn extract_then_parse_round_trips_the_positions() {
        let places = vec![
            Placed { key: 0x000C_740D, name: Some("recruitjet 0x000c740d".into()), pos: [10.0, -3.5, 7.0], quat: [0.0, 0.0, 0.0, 1.0], model: Some(0x86D7_CF92) },
            Placed { key: 0x1111_2222, name: None, pos: [0.0, 0.0, 0.0], quat: [0.0, 0.0, 0.0, 1.0], model: None },
        ];
        let yaml = extract(&places, |h| (h == 0x86D7_CF92).then(|| "recruitjet".to_string()));
        assert!(yaml.contains("0x000C740D") && yaml.contains("recruitjet"), "{yaml}");
        let d = parse(&yaml).unwrap();
        assert_eq!(d.edits.len(), 2);
        assert_eq!(d.edits[0].pos, Some([10.0, -3.5, 7.0]));
        assert_eq!(d.edits[0].model.as_deref(), Some("recruitjet"));
    }
}
