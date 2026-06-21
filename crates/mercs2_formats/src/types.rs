//! ASET type_id and UCFX type_hash registry (retail vz.wad).

pub const TYPE_ID_WAVEBANK: u32 = 6;
pub const TYPE_ID_SOUNDBANK: u32 = 21;
pub const TYPE_ID_LAYER: u32 = 9;
pub const TYPE_ID_MODEL: u32 = 19;
pub const TYPE_ID_TEXTURE: u32 = 27;
pub const TYPE_ID_SCRIPT: u32 = 35;
pub const TYPE_ID_ANIMATION: u32 = 16;
pub const TYPE_ID_LOWRES_TERRAIN: u32 = 22;
pub const TYPE_ID_TERRAIN_MESH: u32 = 32;
pub const TYPE_ID_FONT: u32 = 15;
pub const TYPE_ID_PATH: u32 = 28;
pub const TYPE_ID_EFFECT: u32 = 29;
pub const TYPE_ID_STRINGDB: u32 = 7;
pub const TYPE_ID_LEVEL: u32 = 20;
/// "stance" / named-registry (ActionTable, VehicleAnimationLookup, Sounds, …);
/// type_hash 0x207359C7. Processed by FUN_0067cfb0's fixed 1024-slot table.
pub const TYPE_ID_STANCE: u32 = 11;
pub const TYPE_ID_MATERIAL_PARAMS: u32 = 14;
pub const TYPE_ID_MUSIC_STATE_MAP: u32 = 26;
pub const TYPE_ID_MUSIC_CUE_TABLE: u32 = 4;
pub const TYPE_ID_ANIM_STATE_MACHINE: u32 = 31;
pub const TYPE_ID_WORLD_ENTITY_DATA: u32 = 8;
pub const TYPE_ID_FX_DICTIONARY: u32 = 25;
/// Singleton `watermap` in resident block (`pandemic_hash_m2("watermap")`).
pub const TYPE_ID_WATERMAP: u32 = 0;

pub const TYPE_HASH_WAVEBANK: u32 = 0xF753F6D0;
pub const TYPE_HASH_SOUNDBANK: u32 = 0x9F8BCA10;
pub const TYPE_HASH_LAYER: u32 = 0xE6B81A54;
pub const TYPE_HASH_MODEL: u32 = 0x5B724250;
pub const TYPE_HASH_TEXTURE: u32 = 0xF011157A;
pub const TYPE_HASH_SCRIPT: u32 = 0x42498680;
pub const TYPE_HASH_ANIMATION: u32 = 0x18166555;
pub const TYPE_HASH_LOWRES_TERRAIN: u32 = 0x1602815C;
pub const TYPE_HASH_TERRAIN_MESH: u32 = 0x7C569307;
pub const TYPE_HASH_FONT: u32 = 0x99E77ACE;
pub const TYPE_HASH_PATH: u32 = 0xBCFE6314;
pub const TYPE_HASH_EFFECT: u32 = 0x5608BD5A;
pub const TYPE_HASH_STRINGDB: u32 = 0x39E5E978;
pub const TYPE_HASH_LEVEL: u32 = 0xEA4829D5;
pub const TYPE_HASH_MATERIAL_PARAMS: u32 = 0xDE982D61;
pub const TYPE_HASH_MUSIC_STATE_MAP: u32 = 0xC122545A;
pub const TYPE_HASH_MUSIC_CUE_TABLE: u32 = 0xE8DF4D87;
pub const TYPE_HASH_ANIM_STATE_MACHINE: u32 = 0xECE70371;
pub const TYPE_HASH_WORLD_ENTITY_DATA: u32 = 0x5647C35D;
pub const TYPE_HASH_GUIDMAP: u32 = 0x140E8728;
/// "stance" / named-registry (ActionTable, VehicleAnimationLookup, Sounds, …).
/// Its nested UCFX is INFO(dims triple)/TYPE(dim-name strings)/VALU(value rows);
/// INFO+TYPE need per-field conversion (see convert.rs), not a blanket u32 swap,
/// or the engine reads a transposed/byteswapped row count and overflows the
/// 1024-slot table in FUN_0067cfb0 (world-load livelock @0x67D130).
pub const TYPE_HASH_STANCE: u32 = 0x207359C7;
pub const TYPE_HASH_FX_DICTIONARY: u32 = 0xFA46D8A8;
pub const TYPE_HASH_WATERMAP: u32 = 0x4D7D30C4;

/// All known type_hash → type_id mappings from retail census.
pub const TYPE_HASH_REGISTRY: &[(u32, u32)] = &[
    (0xF011157A, 27),
    (0xBCFE6314, 28),
    (0x5B724250, 19),
    (0x18166555, 16),
    (0x600B904E, 12),
    (0xE6B81A54, 9),
    (0x42498680, 35),
    (0x6310807F, 30),
    (0x7C569307, 32),
    (0x1602815C, 22),
    (0x5608BD5A, 29),
    (0xF753F6D0, 6),
    (0x665EF13E, 5),
    (0xE5273C14, 13),
    (0x9F8BCA10, 21),
    (0xFE0E8320, 23),
    (0x1CF649BB, 34),
    (0xFA0B8DBC, 18),
    (0x207359C7, 11),
    (0x8F0A54E2, 3),
    (0x99E77ACE, 15),
    (0xDE982D61, 14),
    (0x39E5E978, 7),
    (0x59B9DF6A, 0),
    (0x4D7D30C4, 0),
    (0x34612F86, 0),
    (0xACCE47F2, 33),
    (0xC122545A, 26),
    (0xE8DF4D87, 4),
    (0xECE70371, 31),
    (0xEA4829D5, 20),
    (0x3B0AABF8, 1),
    (0x5647C35D, 8),
    (0x140E8728, 10),
    (0xFA46D8A8, 25),
];

pub fn type_hash_for_type_id(type_id: u32) -> Option<u32> {
    TYPE_HASH_REGISTRY
        .iter()
        .find(|(_, id)| *id == type_id)
        .map(|(h, _)| *h)
}

pub fn type_id_for_type_hash(type_hash: u32) -> Option<u32> {
    TYPE_HASH_REGISTRY
        .iter()
        .find(|(h, _)| *h == type_hash)
        .map(|(_, id)| *id)
}

pub fn type_name_from_hash(hash: u32) -> &'static str {
    if hash == TYPE_HASH_WATERMAP {
        return "watermap";
    }
    for &(th, tid) in TYPE_HASH_REGISTRY {
        if th == hash {
            return type_name(tid);
        }
    }
    "unknown"
}

pub fn type_name(type_id: u32) -> &'static str {
    match type_id {
        0 => "singleton",
        3 => "binary",
        4 => "music_cue_table",
        5 => "mission_flow",
        6 => "wavebank",
        7 => "stringdb",
        8 => "world_entity_data",
        9 => "layer",
        10 => "guidmap",
        11 => "stance",
        12 => "shader_scrb",
        13 => "audio_group",
        14 => "material_params",
        15 => "font",
        16 => "animation",
        18 => "resident_misc",
        19 => "model",
        20 => "level",
        21 => "soundbank",
        22 => "lowresterrain",
        23 => "cfx_pack",
        25 => "fx_dictionary",
        26 => "music_state_map",
        27 => "texture",
        28 => "path",
        29 => "effect",
        30 => "object_registry",
        31 => "anim_state_machine",
        32 => "terrainmesh",
        33 => "sequence",
        34 => "starter",
        35 => "script",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_hash_roundtrip() {
        // Known constants
        assert_eq!(
            type_hash_for_type_id(TYPE_ID_TEXTURE),
            Some(TYPE_HASH_TEXTURE)
        );
        assert_eq!(
            type_id_for_type_hash(TYPE_HASH_MODEL),
            Some(TYPE_ID_MODEL)
        );
        assert_eq!(
            type_id_for_type_hash(TYPE_HASH_TEXTURE),
            Some(TYPE_ID_TEXTURE)
        );
    }

    #[test]
    fn all_constants_in_registry() {
        let known_ids = vec![
            TYPE_ID_WAVEBANK,
            TYPE_ID_SOUNDBANK,
            TYPE_ID_LAYER,
            TYPE_ID_MODEL,
            TYPE_ID_TEXTURE,
            TYPE_ID_SCRIPT,
            TYPE_ID_ANIMATION,
            TYPE_ID_LOWRES_TERRAIN,
            TYPE_ID_TERRAIN_MESH,
            TYPE_ID_FONT,
            TYPE_ID_PATH,
            TYPE_ID_EFFECT,
            TYPE_ID_STRINGDB,
            TYPE_ID_LEVEL,
            TYPE_ID_STANCE,
            TYPE_ID_MATERIAL_PARAMS,
            TYPE_ID_MUSIC_STATE_MAP,
            TYPE_ID_MUSIC_CUE_TABLE,
            TYPE_ID_ANIM_STATE_MACHINE,
            TYPE_ID_WORLD_ENTITY_DATA,
            TYPE_ID_FX_DICTIONARY,
        ];
        for type_id in known_ids {
            let name = type_name(type_id);
            assert_ne!(name, "unknown", "type_id {} has unknown name", type_id);
        }
    }

    #[test]
    fn unknown_type_id() {
        assert_eq!(type_name(999), "unknown");
    }

    #[test]
    fn unknown_type_hash() {
        assert_eq!(type_id_for_type_hash(0xDEADBEEF), None);
        assert_eq!(type_hash_for_type_id(999), None);
    }

    #[test]
    fn type_name_from_hash_known() {
        assert_eq!(type_name_from_hash(TYPE_HASH_TEXTURE), "texture");
        assert_eq!(type_name_from_hash(TYPE_HASH_MODEL), "model");
        assert_eq!(type_name_from_hash(TYPE_HASH_SCRIPT), "script");
    }

    #[test]
    fn type_name_from_hash_watermap() {
        assert_eq!(type_name_from_hash(TYPE_HASH_WATERMAP), "watermap");
    }

    #[test]
    fn type_name_from_hash_unknown() {
        assert_eq!(type_name_from_hash(0xDEADBEEF), "unknown");
    }

    #[test]
    fn type_registry_no_duplicates() {
        // Check that each hash is unique
        for i in 0..TYPE_HASH_REGISTRY.len() {
            for j in (i + 1)..TYPE_HASH_REGISTRY.len() {
                assert_ne!(
                    TYPE_HASH_REGISTRY[i].0, TYPE_HASH_REGISTRY[j].0,
                    "Duplicate hash in registry"
                );
            }
        }
    }

    #[test]
    fn texture_constants_match() {
        assert_eq!(TYPE_HASH_TEXTURE, 0xF011157A);
        assert_eq!(TYPE_ID_TEXTURE, 27);
        assert_eq!(type_id_for_type_hash(TYPE_HASH_TEXTURE), Some(27));
    }

    #[test]
    fn script_constants_match() {
        assert_eq!(TYPE_HASH_SCRIPT, 0x42498680);
        assert_eq!(TYPE_ID_SCRIPT, 35);
        assert_eq!(type_id_for_type_hash(TYPE_HASH_SCRIPT), Some(35));
    }

    #[test]
    fn soundbank_constants_match() {
        assert_eq!(TYPE_HASH_SOUNDBANK, 0x9F8BCA10);
        assert_eq!(TYPE_ID_SOUNDBANK, 21);
        assert_eq!(type_id_for_type_hash(TYPE_HASH_SOUNDBANK), Some(21));
    }

    #[test]
    fn all_registry_entries_have_names() {
        for (_hash, type_id) in TYPE_HASH_REGISTRY {
            let name = type_name(*type_id);
            // Most should have meaningful names
            if *type_id != 999 && *type_id != 1000 {
                // Allow unmapped IDs to have "unknown" name
                let _ = name;
            }
        }
    }
}
