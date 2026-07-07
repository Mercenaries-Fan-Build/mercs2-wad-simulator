//! Faction reflection components — the faction state an entity carries.
//!
//! Code map §4 (`faction_reputation_code_map.md`) + the AI census (`ai_code_map.md` `Suspect`):
//! faction state *on an entity* is three single-field descriptors + one runtime blob + the
//! per-faction suspicion component. Each is registered by the standard two-role registrar/consumer
//! pair; the census gives the m2 class hash, stride, and payload. These are the faithful engine-side
//! component structs — hashes/strides verbatim from the code map, defaults documented per field.
//!
//! Every hash here is exactly `mercs2_formats::hash::pandemic_hash_m2(class_name)` (verified in the
//! module tests) — i.e. the engine keys these components by the m2 hash of the reflection class name.

/// `FactionMarker` (`0x9b98cb09`, registrar `FUN_00641340`, consumer `FUN_0065c0f0`, stride 4,
/// Xbox pool `FactionMarker 1280`) — 1 int32 = the **faction id** the entity belongs to. The floating
/// faction blip / friend-foe classification reads this.
pub const FACTION_MARKER_HASH: u32 = 0x9b98_cb09;
/// Stride of `FactionMarker` (bytes) — 1 int32.
pub const FACTION_MARKER_STRIDE: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactionMarker {
    /// The faction id (GUID) this entity belongs to. `0` = unassigned.
    pub faction_id: i32,
}

/// `FactionValue` (`0x8bfc69d6`, registrar `FUN_00641830`, consumer `FUN_0065c7d0`, stride 4,
/// Xbox pool `FactionValue 64 64`) — 1 float = a per-entity faction **scalar**
/// (rep / influence / contribution). Default `0.0`.
pub const FACTION_VALUE_HASH: u32 = 0x8bfc_69d6;
/// Stride of `FactionValue` (bytes) — 1 float.
pub const FACTION_VALUE_STRIDE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FactionValue {
    /// Per-entity faction scalar (influence / contribution / rep). Recovered default `0.0`.
    pub value: f32,
}
impl Default for FactionValue {
    fn default() -> Self {
        FactionValue { value: 0.0 }
    }
}

/// `FactionZone` (`0x67267cc1`, registrar `FUN_006414b0`, consumer `FUN_0065c490`, stride 4,
/// Xbox pool `FactionZone 16 16`) — 1 int32 = the **faction-owned zone id**. This is the world-authored
/// trespass trigger the Lua `FactionZone.Init{TresspasserCallback=…}` consumes, feeding the
/// `Trespassing` infraction key (see [`crate::mood`]). Default `0`.
pub const FACTION_ZONE_HASH: u32 = 0x6726_7cc1;
/// Stride of `FactionZone` (bytes) — 1 int32.
pub const FACTION_ZONE_STRIDE: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FactionZone {
    /// The faction id that owns this zone. `0` = unowned.
    pub zone_faction_id: i32,
}

/// `RtFactionZone` (`0xa67114c7`, desc base `0x017c05f8`, **stride 0x1c = 28 B**, raw-copy,
/// Xbox pool `RtFactionZone 16 16`) — the live runtime counterpart of [`FactionZone`]. The code map
/// records it only as a 28-byte raw-copy blob (`raw-copy 0x1c`); the internal field layout is
/// **not recovered** (confirm-live), so we carry it faithfully as an opaque 28-byte record rather
/// than invent fields. Default = zeroed.
pub const RT_FACTION_ZONE_HASH: u32 = 0xa671_14c7;
/// Stride of `RtFactionZone` (bytes) — 0x1c raw-copy record.
pub const RT_FACTION_ZONE_STRIDE: usize = 0x1c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtFactionZone {
    /// Opaque 28-byte runtime faction-zone state — internal layout unrecovered (confirm-live).
    pub raw: [u8; RT_FACTION_ZONE_STRIDE],
}
impl Default for RtFactionZone {
    fn default() -> Self {
        RtFactionZone { raw: [0u8; RT_FACTION_ZONE_STRIDE] }
    }
}

/// `Suspect` (`0x1afc276c`, runtime `FUN_006482b0`, **stride 0x20 = 32 B**) — the per-faction
/// **suspicion / wanted** state on an entity: 8 factions × 1 dword (the AI census, `ai_code_map.md`
/// row "Suspect"). One `i32` suspicion counter per faction, indexed by the 8-faction order in
/// [`crate::factions::FACTION_TEMPLATES`]. Default = all zero (unsuspected).
pub const SUSPECT_HASH: u32 = 0x1afc_276c;
/// Stride of `Suspect` (bytes) — 0x20 = 8 factions × i32.
pub const SUSPECT_STRIDE: usize = 0x20;
/// Number of per-faction suspicion slots in [`Suspect`] (8 factions × 1 dword = 0x20 bytes).
pub const SUSPECT_FACTIONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Suspect {
    /// Per-faction suspicion / wanted value; index = faction slot (see `factions::FACTION_TEMPLATES`).
    pub per_faction: [i32; SUSPECT_FACTIONS],
}
impl Default for Suspect {
    fn default() -> Self {
        Suspect { per_faction: [0; SUSPECT_FACTIONS] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    /// Every recovered component hash is exactly `pandemic_hash_m2(class_name)` — the engine keys
    /// each reflection component by the m2 hash of its class name. This ties the recovered ecs-07
    /// hashes to the shared hashing primitive.
    #[test]
    fn component_hashes_are_m2_of_class_name() {
        assert_eq!(pandemic_hash_m2("FactionMarker"), FACTION_MARKER_HASH);
        assert_eq!(pandemic_hash_m2("FactionValue"), FACTION_VALUE_HASH);
        assert_eq!(pandemic_hash_m2("FactionZone"), FACTION_ZONE_HASH);
        assert_eq!(pandemic_hash_m2("RtFactionZone"), RT_FACTION_ZONE_HASH);
        assert_eq!(pandemic_hash_m2("Suspect"), SUSPECT_HASH);
    }

    /// Strides + defaults match the recovered census.
    #[test]
    fn strides_and_defaults_match_census() {
        assert_eq!(FACTION_MARKER_STRIDE, 4);
        assert_eq!(FACTION_VALUE_STRIDE, 4);
        assert_eq!(FACTION_ZONE_STRIDE, 4);
        assert_eq!(RT_FACTION_ZONE_STRIDE, 28);
        assert_eq!(SUSPECT_STRIDE, 32);
        assert_eq!(SUSPECT_STRIDE, SUSPECT_FACTIONS * 4);

        assert_eq!(FactionMarker::default().faction_id, 0);
        assert_eq!(FactionValue::default().value, 0.0);
        assert_eq!(FactionZone::default().zone_faction_id, 0);
        assert_eq!(RtFactionZone::default().raw, [0u8; 28]);
        assert_eq!(Suspect::default().per_faction, [0i32; 8]);
    }
}
