//! Population / spawner reflection component families — the ECS components a population zone,
//! spawner, or living-world NPC carries.
//!
//! Code map §9 + `docs/mercs2-ecs/02_ai_perception_population.md`: every static-schema population
//! component registers via the Keystone-A descriptor/field-template pattern; the census gives the m2
//! hash, stride (`desc+0x24`), pool budget (`cdbsizes.ini`), and recovered field defaults. These are
//! the faithful engine-side component structs with those constants verbatim; the ambient/spawner
//! *runtime* that consumes them lives in [`crate::spawner`] / [`crate::density`] / [`crate::death`].
//!
//! Hashes cross-checked against [`mercs2_formats::hash::pandemic_hash_m2`] of the class name in the
//! `hashes_match_the_census` test below.

// ---------------------------------------------------------------------------------------------
// Enums (recovered from exe `.rdata`; member counts exact — code map §9 / ecs doc "Enum tables").
// ---------------------------------------------------------------------------------------------

/// `TrafficControlEnum` @`0xbc75fc` — 6 members. Gates what a [`PopulationDensity`] zone lets spawn.
/// Values in `.rdata` layout order; `Default` (0) = unrestricted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrafficControl {
    /// No restriction — crowds and traffic both spawn.
    #[default]
    Default = 0,
    /// Zone is quiet: nothing ambient spawns here.
    NoTraffic = 1,
    /// Peds only — vehicles are suppressed.
    NoVehicles = 2,
    /// Vehicles only — pedestrians are suppressed.
    NoPeds = 3,
    /// Everything except one banned faction may spawn.
    BanFaction = 4,
    /// Only one designated faction may spawn.
    SingleFaction = 5,
}

impl TrafficControl {
    /// Map the raw enum column (as streamed) to the variant, clamping unknowns to `Default`.
    pub fn from_raw(v: i32) -> Self {
        match v {
            1 => TrafficControl::NoTraffic,
            2 => TrafficControl::NoVehicles,
            3 => TrafficControl::NoPeds,
            4 => TrafficControl::BanFaction,
            5 => TrafficControl::SingleFaction,
            _ => TrafficControl::Default,
        }
    }
}

/// `DynamicRoadTypeEnum` @`0xbc675c` — 2 members. Default `Overpass` (code map ecs doc field schema).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DynamicRoadType {
    #[default]
    Overpass = 0,
    Wall = 1,
}

/// `FlowControlTypeEnum` @`0xbc6734` — 2 members. Default value 0 = `StopSign`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlowControlType {
    #[default]
    StopSign = 0,
    TrafficLight = 1,
}

/// `NeedTypeEnum` @`0xbc6118` — 5 members (the "need" a [`SocialUse`] point satisfies). Values in
/// the recovered `.rdata` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NeedType {
    #[default]
    Shade = 0,
    Contact = 1,
    Activity = 2,
    Trash = 3,
    Exit = 4,
}

// ---------------------------------------------------------------------------------------------
// Component m2 hashes + strides + pool budgets (code map §9 marriage table / ecs registry).
// ---------------------------------------------------------------------------------------------

/// `PopulationDensity` (`0x6fa2f9d4`, stride 0x1c, pool 128/64) — the crowd/vehicle density + traffic
/// rules of a population zone. Schema: 5 int density/cap counters, a `TrafficControlEnum`, 1 int.
pub const POPULATION_DENSITY_HASH: u32 = 0x6fa2_f9d4;
pub const POPULATION_DENSITY_STRIDE: usize = 0x1c;
pub const POPULATION_DENSITY_POOL: (u32, u32) = (128, 64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopulationDensity {
    /// The five int density/cap counters (`i0..i4`). Exact per-slot meaning (people/veh caps + tuning)
    /// is **data** — the main ambient driver `FUN_00503020` body is unread (code map §3) — so they are
    /// carried as the streamed columns, consumed by [`crate::density`] as caps against the recovered
    /// per-tick budgets, not re-derived here.
    pub caps: [i32; 5],
    /// `TrafficControlEnum` (field 5) — the spawn gate (default `Default`).
    pub traffic: TrafficControl,
    /// Trailing int column `i6` (default 0).
    pub extra: i32,
}

/// `PopulationDynamicRoad` (`0xffc5baa5`, stride 0x0c, pool 32/32) — dynamic road type (overpass/wall)
/// + 2 int columns.
pub const POPULATION_DYNAMIC_ROAD_HASH: u32 = 0xffc5_baa5;
pub const POPULATION_DYNAMIC_ROAD_STRIDE: usize = 0x0c;
pub const POPULATION_DYNAMIC_ROAD_POOL: (u32, u32) = (32, 32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopulationDynamicRoad {
    pub road_type: DynamicRoadType,
    pub i1: i32,
    pub i2: i32,
}

/// `PopulationFlow` (`0x322750ec`, stride 0x0c, pool 192/64) — traffic-flow control (stop-sign/light)
/// + 2 int columns.
pub const POPULATION_FLOW_HASH: u32 = 0x3227_50ec;
pub const POPULATION_FLOW_STRIDE: usize = 0x0c;
pub const POPULATION_FLOW_POOL: (u32, u32) = (192, 64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopulationFlow {
    pub flow_type: FlowControlType,
    pub i1: i32,
    pub i2: i32,
}

/// `SkirmishZone` (`0xfc5923af`, stride 0x08) — a skirmish region: 1 float + 1 int (both default 0).
pub const SKIRMISH_ZONE_HASH: u32 = 0xfc59_23af;
pub const SKIRMISH_ZONE_STRIDE: usize = 0x08;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkirmishZone {
    pub f0: f32,
    pub i1: i32,
}

/// `SkirmishSpawnList` (`0xafba5846`, stride 0x18, pool 16/16) — six spawn-list slot ints
/// (faction/unit/count indices; all default 0). The primary content-authored spawn table a skirmish
/// or attached spawner draws templates from.
pub const SKIRMISH_SPAWN_LIST_HASH: u32 = 0xafba_5846;
pub const SKIRMISH_SPAWN_LIST_STRIDE: usize = 0x18;
pub const SKIRMISH_SPAWN_LIST_POOL: (u32, u32) = (16, 16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkirmishSpawnList {
    /// Six slot ints `i0..i5` (faction/unit/count indices). Meaning is data-authored; carried verbatim.
    pub slots: [i32; 6],
}

/// `SocialUse` (`0x7e6bf93d`, stride 0x10) — a prop/anchor a civilian uses to satisfy a "need".
/// Schema: `NeedTypeEnum` + 3 floats (range/duration 5 / 30 / 5).
pub const SOCIAL_USE_HASH: u32 = 0x7e6b_f93d;
pub const SOCIAL_USE_STRIDE: usize = 0x10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SocialUse {
    pub need: NeedType,
    pub f1: f32,
    pub f2: f32,
    pub f3: f32,
}
impl Default for SocialUse {
    fn default() -> Self {
        SocialUse { need: NeedType::Shade, f1: 5.0, f2: 30.0, f3: 5.0 }
    }
}

/// `RtPopMembership` (`0x8c8e5490`, stride 0x14) — **runtime** population-group membership of an NPC
/// (which living-world group it belongs to). No static field template — the 0x14-byte record is filled
/// at runtime by the living-world manager (ecs doc "Runtime / networked components"); carried as an
/// opaque group id + the raw record width so the reimpl can round-trip it without inventing a body.
pub const RT_POP_MEMBERSHIP_HASH: u32 = 0x8c8e_5490;
pub const RT_POP_MEMBERSHIP_STRIDE: usize = 0x14;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RtPopMembership {
    /// The runtime population group this NPC is a member of (server/living-world assigned).
    pub group: u32,
}

/// `RuntimeTravelGroup` (`0x5f187fa4`, stride 0x08) — **runtime** traveling-NPC group. Runtime-filled
/// (no static schema); carried as the group id it links an NPC into.
pub const RUNTIME_TRAVEL_GROUP_HASH: u32 = 0x5f18_7fa4;
pub const RUNTIME_TRAVEL_GROUP_STRIDE: usize = 0x08;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeTravelGroup {
    pub group: u32,
}

// ---------------------------------------------------------------------------------------------
// Faction spawn lists (code map §9: VZ / Pir / Oil / Gur / Chi / Ali / Ped + VehicleSpawnList).
// ---------------------------------------------------------------------------------------------

/// The eight spawn-list channels a population zone / attached spawner draws from — the seven factions
/// plus the shared vehicle list (code map §9 "Spawn-list faction abbreviations", both builds). The
/// `TweakAttachedSpawners{SpawnList=…}` script lever selects one of these per spawner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnFaction {
    /// Venezuelan army (VZ).
    Vz = 0,
    /// Pirates (Pir).
    Pir = 1,
    /// PMC / Oil (Oil).
    Oil = 2,
    /// Guerrilla (Gur).
    Gur = 3,
    /// Chinese (Chi).
    Chi = 4,
    /// Allied Nations (Ali).
    Ali = 5,
    /// Civilian pedestrians (Ped).
    Ped = 6,
    /// The shared `VehicleSpawnList`.
    Vehicle = 7,
}

impl SpawnFaction {
    /// Whether this channel spawns vehicles (only the shared `VehicleSpawnList`) vs on-foot units —
    /// used by the [`TrafficControl`] gate to tell peds from traffic.
    pub fn is_vehicle(self) -> bool {
        matches!(self, SpawnFaction::Vehicle)
    }

    /// The abbreviation used in the shipped data keys (e.g. `"Spawnlist (VZ Ground)"`).
    pub fn abbrev(self) -> &'static str {
        match self {
            SpawnFaction::Vz => "VZ",
            SpawnFaction::Pir => "Pir",
            SpawnFaction::Oil => "Oil",
            SpawnFaction::Gur => "Gur",
            SpawnFaction::Chi => "Chi",
            SpawnFaction::Ali => "Ali",
            SpawnFaction::Ped => "Ped",
            SpawnFaction::Vehicle => "Vehicle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    /// Every population component's `*_HASH` const equals `pandemic_hash_m2(ClassName)` — proving the
    /// census hashes are the real m2 name hashes, not transcription errors.
    #[test]
    fn hashes_match_the_census() {
        assert_eq!(POPULATION_DENSITY_HASH, pandemic_hash_m2("PopulationDensity"));
        assert_eq!(POPULATION_DYNAMIC_ROAD_HASH, pandemic_hash_m2("PopulationDynamicRoad"));
        assert_eq!(POPULATION_FLOW_HASH, pandemic_hash_m2("PopulationFlow"));
        assert_eq!(SKIRMISH_ZONE_HASH, pandemic_hash_m2("SkirmishZone"));
        assert_eq!(SKIRMISH_SPAWN_LIST_HASH, pandemic_hash_m2("SkirmishSpawnList"));
        assert_eq!(SOCIAL_USE_HASH, pandemic_hash_m2("SocialUse"));
        assert_eq!(RT_POP_MEMBERSHIP_HASH, pandemic_hash_m2("RtPopMembership"));
        assert_eq!(RUNTIME_TRAVEL_GROUP_HASH, pandemic_hash_m2("RuntimeTravelGroup"));
    }

    /// Recovered field defaults match the census schemas.
    #[test]
    fn recovered_defaults_match_the_census() {
        assert_eq!(PopulationDensity::default().traffic, TrafficControl::Default);
        assert_eq!(PopulationDensity::default().caps, [0; 5]);
        assert_eq!(PopulationDynamicRoad::default().road_type, DynamicRoadType::Overpass);
        assert_eq!(PopulationFlow::default().flow_type, FlowControlType::StopSign);
        assert_eq!(SkirmishSpawnList::default().slots, [0; 6]);
        let s = SocialUse::default();
        assert_eq!((s.need, s.f1, s.f2, s.f3), (NeedType::Shade, 5.0, 30.0, 5.0));
    }

    /// Strides match the streamed record widths (`desc+0x24`).
    #[test]
    fn strides_match_the_census() {
        assert_eq!(POPULATION_DENSITY_STRIDE, 0x1c);
        assert_eq!(POPULATION_DYNAMIC_ROAD_STRIDE, 0x0c);
        assert_eq!(POPULATION_FLOW_STRIDE, 0x0c);
        assert_eq!(SKIRMISH_ZONE_STRIDE, 0x08);
        assert_eq!(SKIRMISH_SPAWN_LIST_STRIDE, 0x18);
        assert_eq!(SOCIAL_USE_STRIDE, 0x10);
        assert_eq!(RT_POP_MEMBERSHIP_STRIDE, 0x14);
        assert_eq!(RUNTIME_TRAVEL_GROUP_STRIDE, 0x08);
    }

    #[test]
    fn vehicle_list_is_the_only_traffic_channel() {
        assert!(SpawnFaction::Vehicle.is_vehicle());
        for f in [SpawnFaction::Vz, SpawnFaction::Ped, SpawnFaction::Gur] {
            assert!(!f.is_vehicle());
        }
    }
}
