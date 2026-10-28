//! The authored `PopulationSimpleSpawner` record — 112 bytes, schema version 88 — read by field name
//! through its `schm`, whether it comes from a placed block's COMP or a worldentity template.
//!
//! A record is inert data: retail turns it into a runtime spawner only when its owner goes live or a
//! script tweaks its group on (`docs/reverse_engineer/population_spawner_code_map.md` §6). Init
//! `FUN_004E2840` copies these fields into the runtime spawner; the byte fields keep the raw byte, and
//! each consumer applies retail's own sign handling.

use std::collections::BTreeMap;

use crate::hash::pandemic_hash_m2;
use crate::schema::{ComponentSchema, FieldValue};
use crate::worldentity::{Payload, WorldEntity};

/// The `PopulationSimpleSpawner` component class.
pub const CLASS: &str = "PopulationSimpleSpawner";

/// One authored spawner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnerRecord {
    /// `Position` (Blob32) words 0..3: the spawner's position local to its owner.
    pub position: [f32; 3],
    /// `Position` words 4..8: its local rotation quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub spawner_name: u32,
    pub hard_point: u32,
    pub door: u32,
    pub unit_list: u32,
    pub squad_name: u32,
    pub seconds_per_cycle: f32,
    pub escape_radius: f32,
    pub active_radius: f32,
    pub path_spacing: f32,
    pub wakeup_effect: u32,
    /// `SpawnerWeightTypeEnum`.
    pub unit_list_weighting: u32,
    /// `SpawnerRadiusTypeEnum`.
    pub radius_type: u32,
    /// `SimpleSpawnerStateEnum`.
    pub default_state: u32,
    /// `SimpleSpawnerTypeEnum`.
    pub spawner_type: u32,
    /// `SimpleSpawnerGroupEnum`.
    pub group: u32,
    pub skip_percent_chance: u8,
    pub max_skips: u8,
    pub max_no_skips: u8,
    pub chance_not_active: u8,
    pub chance_to_deactivate: u8,
    pub wakeup_fixed_countdown: u8,
    pub wakeup_random_countdown: u8,
    pub path_count: u8,
    pub ignore_model: u32,
    pub window_spawner: u32,
    pub traffic_spawner: u32,
}

impl SpawnerRecord {
    /// Read a record from its `(field-name-hash, value)` pairs ([`ComponentSchema::read_payload`] or a
    /// block's `ComponentRecord`). An error naming the field when one is missing or of another type.
    pub fn from_fields(fields: &[(u32, FieldValue)]) -> Result<SpawnerRecord, String> {
        let get = |name: &str| {
            let h = pandemic_hash_m2(name);
            fields
                .iter()
                .find(|(k, _)| *k == h)
                .map(|(_, v)| *v)
                .ok_or_else(|| format!("{CLASS}: no {name} field"))
        };
        let word = |name: &str| match get(name)? {
            FieldValue::U32(v) => Ok(v),
            other => Err(format!("{CLASS}: {name} is {other:?}, not a word")),
        };
        let float = |name: &str| match get(name)? {
            FieldValue::F32(v) => Ok(v),
            other => Err(format!("{CLASS}: {name} is {other:?}, not an f32")),
        };
        let byte = |name: &str| match get(name)? {
            FieldValue::U8(v) => Ok(v),
            other => Err(format!("{CLASS}: {name} is {other:?}, not a byte")),
        };
        let FieldValue::Blob32(p) = get("Position")? else {
            return Err(format!("{CLASS}: Position is not a Blob32"));
        };
        Ok(SpawnerRecord {
            position: [p[0], p[1], p[2]],
            rotation: [p[4], p[5], p[6], p[7]],
            spawner_name: word("SpawnerName")?,
            hard_point: word("HardPoint")?,
            door: word("Door")?,
            unit_list: word("UnitList")?,
            squad_name: word("SquadName")?,
            seconds_per_cycle: float("SecondsPerCycle")?,
            escape_radius: float("EscapeRadius")?,
            active_radius: float("ActiveRadius")?,
            path_spacing: float("PathSpacing")?,
            wakeup_effect: word("WakeupEffect")?,
            unit_list_weighting: word("UnitListWeighting")?,
            radius_type: word("SpawnerRadiusType")?,
            default_state: word("DefaultState")?,
            spawner_type: word("SpawnerType")?,
            group: word("SpawnerGroup")?,
            skip_percent_chance: byte("SkipPercentChance")?,
            max_skips: byte("MaxSkips")?,
            max_no_skips: byte("MaxNoSkips")?,
            chance_not_active: byte("ChanceNotActive")?,
            chance_to_deactivate: byte("ChanceToDeactivate")?,
            wakeup_fixed_countdown: byte("WakeupFixedCountdown")?,
            wakeup_random_countdown: byte("WakeupRandomCountdown")?,
            path_count: byte("PathCount")?,
            ignore_model: word("IgnoreModel")?,
            window_spawner: word("WindowSpawner")?,
            traffic_spawner: word("TrafficSpawner")?,
        })
    }

    /// Read a record from a raw payload through its component schema.
    pub fn from_payload(schema: &ComponentSchema, payload: &[u8]) -> Result<SpawnerRecord, String> {
        Self::from_fields(&schema.read_payload(payload))
    }
}

/// Every worldentity template's spawner records, by template key, in data order. A template owns
/// every record whose key list names it (one record can serve several templates).
pub fn template_spawner_records(we: &WorldEntity) -> Result<BTreeMap<u32, Vec<SpawnerRecord>>, String> {
    let mut out: BTreeMap<u32, Vec<SpawnerRecord>> = BTreeMap::new();
    for c in we.groups_of(CLASS) {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("{CLASS}: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            let rec = SpawnerRecord::from_payload(&c.schema, b).map_err(|e| format!("keys {:08X?}: {e}", r.keys))?;
            for &k in &r.keys {
                out.entry(k).or_default().push(rec);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(r: &SpawnerRecord) -> Vec<(u32, FieldValue)> {
        let h = pandemic_hash_m2;
        let mut p = [0f32; 8];
        p[..3].copy_from_slice(&r.position);
        p[4..].copy_from_slice(&r.rotation);
        vec![
            (h("Position"), FieldValue::Blob32(p)),
            (h("SpawnerName"), FieldValue::U32(r.spawner_name)),
            (h("HardPoint"), FieldValue::U32(r.hard_point)),
            (h("Door"), FieldValue::U32(r.door)),
            (h("UnitList"), FieldValue::U32(r.unit_list)),
            (h("SquadName"), FieldValue::U32(r.squad_name)),
            (h("SecondsPerCycle"), FieldValue::F32(r.seconds_per_cycle)),
            (h("EscapeRadius"), FieldValue::F32(r.escape_radius)),
            (h("ActiveRadius"), FieldValue::F32(r.active_radius)),
            (h("PathSpacing"), FieldValue::F32(r.path_spacing)),
            (h("WakeupEffect"), FieldValue::U32(r.wakeup_effect)),
            (h("UnitListWeighting"), FieldValue::U32(r.unit_list_weighting)),
            (h("SpawnerRadiusType"), FieldValue::U32(r.radius_type)),
            (h("DefaultState"), FieldValue::U32(r.default_state)),
            (h("SpawnerType"), FieldValue::U32(r.spawner_type)),
            (h("SpawnerGroup"), FieldValue::U32(r.group)),
            (h("SkipPercentChance"), FieldValue::U8(r.skip_percent_chance)),
            (h("MaxSkips"), FieldValue::U8(r.max_skips)),
            (h("MaxNoSkips"), FieldValue::U8(r.max_no_skips)),
            (h("ChanceNotActive"), FieldValue::U8(r.chance_not_active)),
            (h("ChanceToDeactivate"), FieldValue::U8(r.chance_to_deactivate)),
            (h("WakeupFixedCountdown"), FieldValue::U8(r.wakeup_fixed_countdown)),
            (h("WakeupRandomCountdown"), FieldValue::U8(r.wakeup_random_countdown)),
            (h("PathCount"), FieldValue::U8(r.path_count)),
            (h("IgnoreModel"), FieldValue::U32(r.ignore_model)),
            (h("WindowSpawner"), FieldValue::U32(r.window_spawner)),
            (h("TrafficSpawner"), FieldValue::U32(r.traffic_spawner)),
        ]
    }

    const SAMPLE: SpawnerRecord = SpawnerRecord {
        position: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        spawner_name: 0x24CE_BC21,
        hard_point: 0xEBBF_5043,
        door: 0,
        unit_list: 0x8000_B00D,
        squad_name: 0,
        seconds_per_cycle: 10.0,
        escape_radius: 40.0,
        active_radius: 100.0,
        path_spacing: 0.0,
        wakeup_effect: 0,
        unit_list_weighting: 1,
        radius_type: 1,
        default_state: 1,
        spawner_type: 2,
        group: 3,
        skip_percent_chance: 0,
        max_skips: 0,
        max_no_skips: 0,
        chance_not_active: 0,
        chance_to_deactivate: 0,
        wakeup_fixed_countdown: 0,
        wakeup_random_countdown: 0,
        path_count: 0,
        ignore_model: 0,
        window_spawner: 0,
        traffic_spawner: 0,
    };

    #[test]
    fn reads_every_field_by_name() {
        assert_eq!(SpawnerRecord::from_fields(&fields(&SAMPLE)), Ok(SAMPLE));
    }

    #[test]
    fn a_missing_or_mistyped_field_is_an_error() {
        let mut f = fields(&SAMPLE);
        f.retain(|(k, _)| *k != pandemic_hash_m2("SpawnerGroup"));
        assert!(SpawnerRecord::from_fields(&f).unwrap_err().contains("SpawnerGroup"));
        let mut f = fields(&SAMPLE);
        f[1].1 = FieldValue::F32(0.0);
        assert!(SpawnerRecord::from_fields(&f).unwrap_err().contains("SpawnerName"));
    }
}
