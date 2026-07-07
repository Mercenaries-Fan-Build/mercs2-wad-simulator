//! `AiWaterZone` — the AI water-zone reflection component.
//!
//! Cross-ref [`ai_code_map.md`](../../../../docs/reverse_engineer/ai_code_map.md) §3/§4: `AiWaterZone`
//! is a real AI reflection component — m2 hash **`0xdf6533de`**, descriptor builder `FUN_0065c520` /
//! `FUN_00641560`, **stride `0x04`** (a single enum column). It tags a region of the world with an AI
//! water-type so navigation/behaviour can react to water (e.g. avoid, or allow amphibious traversal).
//!
//! **Honesty boundary — the enum vocabulary is data, not recovered here.** The code map lists the
//! type-name table `s_AiWaterZoneEnum` *exists* (§4) but, unlike the sibling enums (`AiPatrolModeEnum`
//! {Loop,Bounce}, `AiHintEnum` {Movement,…}), it **does not itemise `AiWaterZoneEnum`'s members**. So
//! this component is modelled faithfully as the recovered single-`u32` enum column — a raw value with
//! the recovered hash/stride — and the member vocabulary is left as data (not invented). This mirrors
//! how `mercs2_ai` keeps `Perception::mode` a raw column when the enum vocabulary is data.

/// `AiWaterZone` reflection hash (`ai_code_map.md` §3 census).
pub const AI_WATER_ZONE_HASH: u32 = 0xdf65_33de;

/// `AiWaterZone` stride in bytes (`0x04` — one enum dword).
pub const AI_WATER_ZONE_STRIDE: usize = 0x04;

/// The AI water-zone type tag on a region entity — a single enum column (`stride 0x04`). The concrete
/// `s_AiWaterZoneEnum` member names are **not recovered** (code map §4 lists the table but not its
/// members), so the value is carried raw exactly as the engine stores it; `0` is the default/first
/// member (the engine's enum tables are 0-based and default to member 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct AiWaterZone(pub u32);

impl AiWaterZone {
    /// The default zone (enum member `0`).
    pub const DEFAULT: AiWaterZone = AiWaterZone(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_hash_and_stride() {
        assert_eq!(AI_WATER_ZONE_HASH, 0xdf65_33de);
        assert_eq!(AI_WATER_ZONE_STRIDE, 4);
        assert_eq!(AiWaterZone::default(), AiWaterZone::DEFAULT);
    }
}
