//! UCFX `type_hash` → ASET `type_id` map (retail vz.wad registry).
//! Port of `tools/aset_type_ids.py` (source: docs/type_hash_registry.md).

pub const SCRIPT_TYPE_HASH: u32 = 0x42498680;
pub const STRINGDB_TYPE_HASH: u32 = 0x39E5E978;
pub const SCRIPT_ASET_TYPE_ID: u32 = 35;
pub const STRINGDB_ASET_TYPE_ID: u32 = 7;

/// ASET type_id for a UCFX type_hash, or `None` if unknown.
pub fn type_id_for_type_hash(type_hash: u32) -> Option<u32> {
    Some(match type_hash {
        0xF011157A => 27, // texture
        0xBCFE6314 => 28, // path
        0x5B724250 => 19, // model
        0x18166555 => 16, // animation
        0x600B904E => 12,
        0xE6B81A54 => 9, // layer
        0x42498680 => 35, // script
        0x6310807F => 30,
        0x7C569307 => 32, // terrainmesh
        0x1602815C => 22, // lowresterrain
        0x5608BD5A => 29, // effect
        0xF753F6D0 => 6,  // wavebank
        0x665EF13E => 5,
        0xE5273C14 => 13,
        0x9F8BCA10 => 21, // soundbank
        0xFE0E8320 => 23,
        0x1CF649BB => 34,
        0xFA0B8DBC => 18,
        0x207359C7 => 11,
        0x8F0A54E2 => 3, // binary
        0x99E77ACE => 15, // font
        0xDE982D61 => 14,
        0x39E5E978 => 7, // stringdb
        0x59B9DF6A => 0, // materialtable (singleton; id from ASET row)
        0x4D7D30C4 => 0,
        0x34612F86 => 0,
        0xACCE47F2 => 33,
        0xC122545A => 26,
        0xE8DF4D87 => 4,
        0xECE70371 => 31,
        0xEA4829D5 => 20, // level
        0x3B0AABF8 => 1,
        0x5647C35D => 8,
        0x140E8728 => 10,
        0xFA46D8A8 => 25,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown() {
        assert_eq!(type_id_for_type_hash(0xF011157A), Some(27)); // texture
        assert_eq!(type_id_for_type_hash(SCRIPT_TYPE_HASH), Some(35));
        assert_eq!(type_id_for_type_hash(STRINGDB_TYPE_HASH), Some(7));
        assert_eq!(type_id_for_type_hash(0xDEADBEEF), None);
    }
}
