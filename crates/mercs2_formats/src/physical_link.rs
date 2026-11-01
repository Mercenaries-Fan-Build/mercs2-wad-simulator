//! The `PhysicalLink` component of the `worldentity` templates: a child template an entity is spawned
//! with, attached at one of its nodes. A jeep's mounted gun and an oil rig's modules are these; level
//! load instantiates a template's children from them (`FUN_00654940` `0x655705` → `FUN_00673070`), and
//! each child goes on the `RuntimePhysicalLink` container `0xDF9110`.
//!
//! Record, 44 bytes after its keys: `+0` child template, `+4` `HardPoint`, `+8` `NAME`, `+0x0C` the
//! `Position` blob of 32 bytes: x, y, z, a padding word, then the quaternion xyzw.

use crate::worldentity::{Payload, WorldEntity};
use std::collections::{BTreeMap, HashMap};

const CLASS: &str = "PhysicalLink";
const RECORD: usize = 44;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalLink {
    pub child: u32,
    /// The parent's node the child hangs from; 0 for the entity itself.
    pub hard_point: u32,
    pub name: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
}

fn parse(b: &[u8]) -> Result<PhysicalLink, String> {
    if b.len() != RECORD {
        return Err(format!("{CLASS}: a record of {} bytes, not {RECORD}", b.len()));
    }
    let word = |i: usize| [b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]];
    let float = |i: usize| f32::from_le_bytes(word(i));
    Ok(PhysicalLink {
        child: u32::from_le_bytes(word(0)),
        hard_point: u32::from_le_bytes(word(1)),
        name: u32::from_le_bytes(word(2)),
        pos: [float(3), float(4), float(5)],
        quat: [float(7), float(8), float(9), float(10)],
    })
}

/// Every template's links, in record order, by template key. An error for a record of another shape.
pub fn template_physical_links(we: &WorldEntity) -> Result<BTreeMap<u32, Vec<PhysicalLink>>, String> {
    let mut out: BTreeMap<u32, Vec<PhysicalLink>> = BTreeMap::new();
    for c in we.groups_of(CLASS) {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("{CLASS}: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            let link = parse(b).map_err(|e| format!("keys {:08X?}: {e}", r.keys))?;
            for &k in &r.keys {
                out.entry(k).or_default().push(link);
            }
        }
    }
    Ok(out)
}

/// Every placed entity's own links in a decompressed placement block, in record order, by entity key.
pub fn placed_physical_links(block: &[u8]) -> Result<HashMap<u32, Vec<PhysicalLink>>, String> {
    let mut out: HashMap<u32, Vec<PhysicalLink>> = HashMap::new();
    for (key, link) in placed_links_in_order(block)? {
        out.entry(key).or_default().push(link);
    }
    Ok(out)
}

/// Every placed link in a decompressed placement block with the key of the entity it is on, in record
/// order. A chunk that does not divide into `[u32 key][record]` is an error.
pub fn placed_links_in_order(block: &[u8]) -> Result<Vec<(u32, PhysicalLink)>, String> {
    let mut out = Vec::new();
    for c in crate::placement::comp_inventory(block) {
        if c.info_name.as_deref() != Some(CLASS) {
            continue;
        }
        let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
        let data = block.get(off..off + size).ok_or_else(|| format!("{CLASS}: {size} data bytes at {off} run past the block"))?;
        if data.len() % (4 + RECORD) != 0 {
            return Err(format!("{CLASS}: {} data bytes are not whole {}-byte records", data.len(), 4 + RECORD));
        }
        for r in data.chunks_exact(4 + RECORD) {
            out.push((u32::from_le_bytes([r[0], r[1], r[2], r[3]]), parse(&r[4..])?));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `M151 .50Cal (VZ) (DriverGunner)`'s record, verbatim from `mercs2_probe we-template`: its
    /// mounted gun `0x8000ACCC`, 0.3 m along Z, unrotated.
    #[test]
    fn a_record_reads_its_child_node_and_offset() {
        let words: [u32; 11] = [
            0x8000_ACCC, 0x59DB_5579, 0x4E2D_1534, 0, 0, 0x3E99_999A, 0, 0, 0, 0, 0x3F80_0000,
        ];
        let b: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let l = parse(&b).unwrap();
        assert_eq!((l.child, l.hard_point, l.name), (0x8000_ACCC, 0x59DB_5579, 0x4E2D_1534));
        assert_eq!(l.pos, [0.0, 0.0, 0.3]);
        assert_eq!(l.quat, [0.0, 0.0, 0.0, 1.0]);
        assert!(parse(&b[..40]).unwrap_err().contains("40 bytes"));
    }
}
