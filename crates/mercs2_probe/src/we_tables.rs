//! What the `worldentity` table exports share (`vehicle-tuning`, `weapon-configs`): reading the
//! container out of the archive named, and writing a field's value as table cells.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use mercs2_engine::registry::AssetRegistry;
use mercs2_engine::wad;
use mercs2_engine::worldutil::rainbow_names;
use mercs2_formats::schema::{FieldValue, SchemaFieldType};
use mercs2_formats::worldentity::{Value, WorldEntity, WORLDENTITY_TYPE_HASH};

/// The resident `worldentity` of the archive at `wadpath`, and of no overlay.
pub fn load(wadpath: &str) -> Result<WorldEntity, String> {
    let mut wads = vec![wad::open(wadpath).map_err(|e| format!("cannot open {wadpath}: {e}"))?];
    let mut registry = AssetRegistry::default();
    let (_, we_ref) =
        registry.resolve_singleton(&mut wads, WORLDENTITY_TYPE_HASH).ok_or("no worldentity chunk in the wad")?;
    let bytes = registry.slice(we_ref).ok_or("worldentity block evicted before read")?;
    WorldEntity::parse(bytes).map_err(|e| format!("worldentity: {e}"))
}

/// The names of schema fields, from the rainbow table the `rainbow_table` key of `.mercs2-local.toml`
/// names. A hash it has no name for is absent.
pub fn field_names(hashes: &BTreeSet<u32>) -> Result<HashMap<u32, String>, String> {
    let table = mercs2_formats::game_paths::local_config_rainbow_table(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    rainbow_names(&table, hashes)
}

/// A field's name, or its hash as `0xHHHHHHHH` when the table has none.
pub fn field_label(names: &HashMap<u32, String>, hash: u32) -> String {
    names.get(&hash).cloned().unwrap_or_else(|| format!("0x{hash:08X}"))
}

/// One field of a record as table cells: a `Vec3` is three, an eight-float blob eight. A `Hash` is
/// written `0xHHHHHHHH`; every other integer in decimal.
pub fn cells(field_type: SchemaFieldType, v: &Value) -> Vec<String> {
    match v {
        Value::Text(s) => vec![s.clone()],
        Value::Field(FieldValue::Bits(b)) => vec![b.to_string()],
        Value::Field(FieldValue::U8(b)) => vec![b.to_string()],
        Value::Field(FieldValue::U16(b)) => vec![b.to_string()],
        Value::Field(FieldValue::F32(f)) => vec![f.to_string()],
        Value::Field(FieldValue::U32(w)) => match field_type {
            SchemaFieldType::Hash => vec![format!("0x{w:08X}")],
            _ => vec![w.to_string()],
        },
        Value::Field(FieldValue::Vec3(a)) => a.iter().map(f32::to_string).collect(),
        Value::Field(FieldValue::Blob32(a)) => a.iter().map(f32::to_string).collect(),
    }
}

/// The column heads of one field: its name, or `name.x` `name.y` `name.z` for the cells of a vector.
pub fn heads(name: &str, n: usize) -> Vec<String> {
    match n {
        1 => vec![name.to_string()],
        3 => ["x", "y", "z"].iter().map(|a| format!("{name}.{a}")).collect(),
        _ => (0..n).map(|i| format!("{name}.{i}")).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_is_three_columns_and_a_hash_is_hex() {
        assert_eq!(heads("CenterOfMassOffset", 3), ["CenterOfMassOffset.x", "CenterOfMassOffset.y", "CenterOfMassOffset.z"]);
        assert_eq!(heads("Mass", 1), ["Mass"]);
        let v = Value::Field(FieldValue::Vec3([0.0, -0.25, 1.5]));
        assert_eq!(cells(SchemaFieldType::Vec3, &v), ["0", "-0.25", "1.5"]);
        let h = Value::Field(FieldValue::U32(0x024C_C491));
        assert_eq!(cells(SchemaFieldType::Hash, &h), ["0x024CC491"]);
        assert_eq!(cells(SchemaFieldType::Enum, &h), ["38585489"]);
    }

    #[test]
    fn a_field_the_table_does_not_name_is_headed_by_its_hash() {
        let names: HashMap<u32, String> = [(0xC3AE_B321, "Mass".to_string())].into();
        assert_eq!(field_label(&names, 0xC3AE_B321), "Mass");
        assert_eq!(field_label(&names, 0x44EF_C403), "0x44EFC403");
    }
}
