//! The `ModelName` component — one `Model` field, the model's asset hash — read by field name through
//! its `schm` from the worldentity templates. A placed entity with no `ModelName` of its own takes its
//! template's (`Transform.Parent`; retail's component lookup `FUN_00649A80` falls back to it).

use std::collections::{BTreeMap, HashMap};

use crate::hash::pandemic_hash_m2;
use crate::schema::FieldValue;
use crate::worldentity::{Payload, WorldEntity};

/// The `ModelName` component class.
pub const CLASS: &str = "ModelName";

/// Every placed entity's own model asset hash in a decompressed placement block or a layer container
/// ([`crate::ucfx::placement_containers`]), by entity key, read through each COMP group's `schm`. A
/// malformed block, a group whose data does not divide into records, a record without a `Model` word,
/// or a key with two records is an error.
pub fn placed_model_names(block: &[u8]) -> Result<HashMap<u32, u32>, String> {
    let field = pandemic_hash_m2("Model");
    let (containers, issues) = crate::ucfx::placement_containers(block, CLASS);
    if !issues.is_empty() {
        return Err(format!("{CLASS}: the UCFX walk reported {issues:?}"));
    }
    let mut out = HashMap::new();
    for c in &containers {
        for g in crate::schema::parse_comp_groups(c) {
            if g.name.as_deref() != Some(CLASS) {
                continue;
            }
            let schema = g.schema().ok_or_else(|| format!("{CLASS}: a COMP group has no readable schm"))?;
            let Some(data) = g.data.as_deref() else { continue };
            let recs = schema
                .deserialize_records(data)
                .ok_or_else(|| format!("{CLASS}: {} data bytes do not divide into {}-byte records", data.len(), schema.record_stride()))?;
            for r in recs {
                let Some(FieldValue::U32(model)) = r.get(field) else {
                    return Err(format!("{CLASS}: key 0x{:08X} has no Model word", r.entity_key));
                };
                if out.insert(r.entity_key, model).is_some() {
                    return Err(format!("{CLASS}: entity 0x{:08X} has two records", r.entity_key));
                }
            }
        }
    }
    Ok(out)
}

/// Every worldentity template's model asset hash, by template key. A key named by two records, or a
/// record without a `Model` word, is an error.
pub fn template_model_names(we: &WorldEntity) -> Result<BTreeMap<u32, u32>, String> {
    let field = pandemic_hash_m2("Model");
    let mut out = BTreeMap::new();
    for c in we.groups_of(CLASS) {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("{CLASS}: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            let model = match c.schema.read_payload(b).into_iter().find(|(k, _)| *k == field) {
                Some((_, FieldValue::U32(m))) => m,
                other => return Err(format!("{CLASS}: keys {:08X?}: Model is {other:?}, not a word", r.keys)),
            };
            for &k in &r.keys {
                if out.insert(k, model).is_some() {
                    return Err(format!("{CLASS}: template 0x{k:08X} has two records"));
                }
            }
        }
    }
    Ok(out)
}
