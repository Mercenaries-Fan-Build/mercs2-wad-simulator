//! The `Rotor` component of the `worldentity` templates: one record per rotor of a helicopter (and
//! per propeller of a boat), read by `0x006570A0` and turned by the bone-controller builder
//! `FUN_00683D70` into one `PgRotorBoneController` per record (vtable `0x00BCD5F8`).
//!
//! The controller swaps the rotor's solid blades (`RotorBlurOffBone`) for its motion-blur discs
//! (`RotorBlurOnBone`) by rotor speed and spins `RotorHubBoneName`; see
//! `docs/modernization/vehicle_model_spec.md`. `FirstRotorBladeBoneName`, `MinRotorAngle` and
//! `MaxRotorAngle` are read but the builder copies none of them into the controller.

use crate::hash::pandemic_hash_m2;
use crate::worldentity::{Payload, WorldEntity};

/// One `Rotor` record (28 bytes, version 82 in retail).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotorRecord {
    /// `+0x00` `RotorHubBoneName`: the node the controller spins.
    pub hub: u32,
    /// `+0x04` `SpinDir`: scales the spin rate (`+1` / `-1` in retail).
    pub spin_dir: f32,
    /// `+0x08` `FirstRotorBladeBoneName`.
    pub first_blade: u32,
    /// `+0x0C` `MinRotorAngle`.
    pub min_angle: f32,
    /// `+0x10` `MaxRotorAngle`.
    pub max_angle: f32,
    /// `+0x14` `RotorBlurOnBone`: shown while the rotor is fast (0 = none).
    pub blur_on: u32,
    /// `+0x18` `RotorBlurOffBone`: shown while the rotor is slow (0 = none).
    pub blur_off: u32,
}

/// The `Rotor` schema, field by field: name and offset.
const FIELDS: [(&str, u16); 7] = [
    ("RotorHubBoneName", 0x00),
    ("SpinDir", 0x04),
    ("FirstRotorBladeBoneName", 0x08),
    ("MinRotorAngle", 0x0C),
    ("MaxRotorAngle", 0x10),
    ("RotorBlurOnBone", 0x14),
    ("RotorBlurOffBone", 0x18),
];

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Every `Rotor` record of the container with the template keys that carry it, in container order.
/// A `Rotor` class whose schema is not the seven fields above is an error.
pub fn rotor_records(we: &WorldEntity) -> Result<Vec<(Vec<u32>, RotorRecord)>, String> {
    let mut out = Vec::new();
    for c in we.groups_of("Rotor") {
        let have: Vec<(u32, u16)> = c.schema.fields.iter().map(|f| (f.name_hash, f.byte_offset)).collect();
        let want: Vec<(u32, u16)> = FIELDS.iter().map(|&(n, o)| (pandemic_hash_m2(n), o)).collect();
        if have != want || c.schema.payload_stride != 28 {
            return Err(format!(
                "Rotor (version {}): schema {have:08X?} stride {} is not the 28-byte record {want:08X?}",
                c.version, c.schema.payload_stride
            ));
        }
        for r in &c.records {
            let b = match &r.payload {
                Payload::Fixed(b) if b.len() == 28 => b,
                p => return Err(format!("Rotor: payload {p:?} is not 28 bytes")),
            };
            out.push((
                r.keys.clone(),
                RotorRecord {
                    hub: u32_at(b, 0x00),
                    spin_dir: f32::from_bits(u32_at(b, 0x04)),
                    first_blade: u32_at(b, 0x08),
                    min_angle: f32::from_bits(u32_at(b, 0x0C)),
                    max_angle: f32::from_bits(u32_at(b, 0x10)),
                    blur_on: u32_at(b, 0x14),
                    blur_off: u32_at(b, 0x18),
                },
            ));
        }
    }
    Ok(out)
}

/// The keys of every template carrying `_HelicopterPhysics`: the physics whose `+0x3C` is the rotor
/// speed a helicopter's `PgRotorBoneController`s read.
pub fn helicopter_keys(we: &WorldEntity) -> std::collections::BTreeSet<u32> {
    we.groups_of("_HelicopterPhysics").flat_map(|c| c.records.iter().flat_map(|r| r.keys.iter().copied())).collect()
}

/// The rotors of `model`, seen as a model rather than as one template: every distinct `Rotor` record
/// of the templates whose `ModelName` is `model`, keeping those whose hub is one of the model's HIER
/// nodes (`node_hashes`) — a record's spin and its destruction latch both act on its hub. A template
/// may carry records for the hubs of a sibling model it also serves (`UH1 Transport` carries
/// `bone_rotor` / `bone_tailrotor` beside the template rig's hubs); those name no node here.
///
/// Two different records for one hub are an error naming both.
pub fn model_rotors(we: &WorldEntity, model: u32, node_hashes: &[u32]) -> Result<Vec<RotorRecord>, String> {
    model_rotors_in(&crate::mixr::kit_templates(we)?, &rotor_records(we)?, model, node_hashes)
}

/// [`model_rotors`] over already-read templates ([`crate::mixr::kit_templates`]) and records
/// ([`rotor_records`]).
pub fn model_rotors_in(
    templates: &[crate::mixr::KitTemplate],
    records: &[(Vec<u32>, RotorRecord)],
    model: u32,
    node_hashes: &[u32],
) -> Result<Vec<RotorRecord>, String> {
    let keys: std::collections::BTreeSet<u32> = templates.iter().filter(|t| t.model == model).map(|t| t.key).collect();
    let mut out: Vec<RotorRecord> = Vec::new();
    for &(ref rk, r) in records {
        if !rk.iter().any(|k| keys.contains(k)) || !node_hashes.contains(&r.hub) {
            continue;
        }
        match out.iter().find(|o| o.hub == r.hub) {
            Some(o) if *o == r => {}
            Some(o) => {
                return Err(format!("model 0x{model:08X}: hub 0x{:08X} has two Rotor records, {o:?} and {r:?}", r.hub));
            }
            None => out.push(r),
        }
    }
    Ok(out)
}
