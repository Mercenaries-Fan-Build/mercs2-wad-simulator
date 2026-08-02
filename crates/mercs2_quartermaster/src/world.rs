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
