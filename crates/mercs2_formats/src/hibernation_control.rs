//! The `HibernationControl` component — schema version 83, 6 bytes — read by field name through its
//! `schm`, from a placed block's COMP or a worldentity template.
//!
//! Retail registers every placed entity in the hibernation grid with its own record, else its
//! template's (`Transform.Parent`), else the defaults (`docs/reverse_engineer/hibernation_grid.md`
//! §2, §7).

use std::collections::{BTreeMap, HashMap};

use crate::hash::pandemic_hash_m2;
use crate::schema::FieldValue;
use crate::worldentity::{Payload, WorldEntity};

/// The `HibernationControl` component class.
pub const CLASS: &str = "HibernationControl";

/// One authored `HibernationControl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HibernationControl {
    /// `Distance` (u16): the hibernation radius in metres before the radius rule.
    pub distance: u16,
    /// `Q2Distance` (u8).
    pub q2_distance: u8,
    /// `Q1Distance` (u8).
    pub q1_distance: u8,
    /// `Q0Distance` (u8).
    pub q0_distance: u8,
    /// `Flags` bit: read when spawning from a template (removed when spawned out of range).
    pub flags: bool,
    /// `NoDelOnHibernateIfPreplaced` bit.
    pub no_del_on_hibernate_if_preplaced: bool,
}

impl HibernationControl {
    /// The three ring distances in grid order `[Q2, Q1, Q0]`.
    pub fn rings(&self) -> [u8; 3] {
        [self.q2_distance, self.q1_distance, self.q0_distance]
    }

    /// Read a record from its `(field-name-hash, value)` pairs. An error naming the field when one is
    /// missing or of another type.
    pub fn from_fields(fields: &[(u32, FieldValue)]) -> Result<HibernationControl, String> {
        let get = |name: &str| {
            let h = pandemic_hash_m2(name);
            fields
                .iter()
                .find(|(k, _)| *k == h)
                .map(|(_, v)| *v)
                .ok_or_else(|| format!("{CLASS}: no {name} field"))
        };
        let byte = |name: &str| match get(name)? {
            FieldValue::U8(v) => Ok(v),
            other => Err(format!("{CLASS}: {name} is {other:?}, not a byte")),
        };
        let bit = |name: &str| match get(name)? {
            FieldValue::Bits(v) if v <= 1 => Ok(v == 1),
            other => Err(format!("{CLASS}: {name} is {other:?}, not a one-bit field")),
        };
        let FieldValue::U16(distance) = get("Distance")? else {
            return Err(format!("{CLASS}: Distance is not a u16"));
        };
        Ok(HibernationControl {
            distance,
            q2_distance: byte("Q2Distance")?,
            q1_distance: byte("Q1Distance")?,
            q0_distance: byte("Q0Distance")?,
            flags: bit("Flags")?,
            no_del_on_hibernate_if_preplaced: bit("NoDelOnHibernateIfPreplaced")?,
        })
    }
}

/// Every worldentity template's `HibernationControl`, by template key. A key named by two records is
/// an error.
pub fn template_hibernation(we: &WorldEntity) -> Result<BTreeMap<u32, HibernationControl>, String> {
    let mut out = BTreeMap::new();
    for c in we.groups_of(CLASS) {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("{CLASS}: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            let rec = HibernationControl::from_fields(&c.schema.read_payload(b))
                .map_err(|e| format!("keys {:08X?}: {e}", r.keys))?;
            for &k in &r.keys {
                if out.insert(k, rec).is_some() {
                    return Err(format!("{CLASS}: template 0x{k:08X} has two records"));
                }
            }
        }
    }
    Ok(out)
}

/// Every placed entity's `HibernationControl` in a decompressed placement block or a layer
/// container ([`crate::ucfx::placement_containers`]), by entity key, read through each COMP group's
/// `schm`. A malformed block, a group whose data does not divide into records, or a key with two
/// records is an error.
pub fn placed_hibernation(block: &[u8]) -> Result<HashMap<u32, HibernationControl>, String> {
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
                let rec = HibernationControl::from_fields(&r.fields).map_err(|e| format!("key 0x{:08X}: {e}", r.entity_key))?;
                if out.insert(r.entity_key, rec).is_some() {
                    return Err(format!("{CLASS}: entity 0x{:08X} has two records", r.entity_key));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(distance: u16, rings: [u8; 3], flags: u32, no_del: u32) -> Vec<(u32, FieldValue)> {
        vec![
            (pandemic_hash_m2("Distance"), FieldValue::U16(distance)),
            (pandemic_hash_m2("Q2Distance"), FieldValue::U8(rings[0])),
            (pandemic_hash_m2("Q1Distance"), FieldValue::U8(rings[1])),
            (pandemic_hash_m2("Q0Distance"), FieldValue::U8(rings[2])),
            (pandemic_hash_m2("Flags"), FieldValue::Bits(flags)),
            (pandemic_hash_m2("NoDelOnHibernateIfPreplaced"), FieldValue::Bits(no_del)),
        ]
    }

    /// The field-name hashes are the schema's (v83 `schm` of `HibernationControl`).
    #[test]
    fn field_names_hash_to_the_schema() {
        assert_eq!(pandemic_hash_m2("Distance"), 0xCBE8_ED58);
        assert_eq!(pandemic_hash_m2("Q2Distance"), 0x74E6_3261);
        assert_eq!(pandemic_hash_m2("Q1Distance"), 0xDEA8_88CE);
        assert_eq!(pandemic_hash_m2("Q0Distance"), 0x2332_033F);
        assert_eq!(pandemic_hash_m2("Flags"), 0x3CE5_1772);
        assert_eq!(pandemic_hash_m2("NoDelOnHibernateIfPreplaced"), 0x3F1D_A641);
    }

    #[test]
    fn reads_every_field_by_name() {
        let h = HibernationControl::from_fields(&fields(223, [0xA0, 0x3C, 0x14], 0, 1)).unwrap();
        assert_eq!(h.distance, 223);
        assert_eq!(h.rings(), [160, 60, 20]);
        assert!(!h.flags);
        assert!(h.no_del_on_hibernate_if_preplaced);
    }

    #[test]
    fn a_missing_or_mistyped_field_is_an_error() {
        let mut f = fields(100, [1, 2, 3], 0, 0);
        f.remove(2);
        assert!(HibernationControl::from_fields(&f).unwrap_err().contains("Q1Distance"));
        let mut f = fields(100, [1, 2, 3], 0, 0);
        f[0].1 = FieldValue::U8(100);
        assert!(HibernationControl::from_fields(&f).unwrap_err().contains("Distance"));
        let f = fields(100, [1, 2, 3], 2, 0);
        assert!(HibernationControl::from_fields(&f).unwrap_err().contains("Flags"));
    }
}
