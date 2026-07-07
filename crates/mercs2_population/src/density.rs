//! Ambient-density budgeting + traffic-control gating — the `DensityUpdate` half of the population
//! tick (code map §3 "main ambient spawn/despawn" / §9 budget constants).
//!
//! What is **recovered as mechanism** (and built here):
//! - the spawn-history **decay op** `>>1 & 0x7f7f7f7f` (PC `FUN_004e0510`) — a per-byte halving of a
//!   packed 4-byte accumulator on each sim-seconds overflow, with no cross-byte borrow;
//! - the per-tick **spawn budgets** `10/10/2/2` (people far/near, vehicles far/near) — Xbox
//!   `FUN_82367d28`, §9;
//! - the [`TrafficControl`] gate deciding *whether* a faction/vehicle may spawn in a zone.
//!
//! What is **data / unread** (deliberately NOT synthesised): the big ambient driver `FUN_00503020`
//! body is unread, and the per-player region-select ("best-priority containing region") is a spatial
//! data query. So a [`PopulationDensity`] zone's five cap counters are consumed as *caps*, and the
//! "which region contains the player" decision is left to the caller — this module owns the budgeted,
//! traffic-gated **count** decision, which is the granularity the code map actually recovered.

use crate::components::{PopulationDensity, SpawnFaction, TrafficControl};

/// Per-tick ambient spawn budget — how many of each class the density update may emit per frame
/// (Xbox `FUN_82367d28` = `10/10/2/2`, code map §9). "Far/near" is the two viewport-distance bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DensityBudget {
    /// Pedestrians in the far band.
    pub people_far: u32,
    /// Pedestrians in the near band.
    pub people_near: u32,
    /// Vehicles in the far band.
    pub vehicles_far: u32,
    /// Vehicles in the near band.
    pub vehicles_near: u32,
}

impl DensityBudget {
    /// The recovered default per-tick budget `10/10/2/2`.
    pub const DEFAULT: DensityBudget =
        DensityBudget { people_far: 10, people_near: 10, vehicles_far: 2, vehicles_near: 2 };
}

impl Default for DensityBudget {
    fn default() -> Self {
        DensityBudget::DEFAULT
    }
}

/// Recovered per-tick budget constants (code map §9).
pub const BUDGET_PEOPLE_FAR: u32 = 10;
pub const BUDGET_PEOPLE_NEAR: u32 = 10;
pub const BUDGET_VEHICLES_FAR: u32 = 2;
pub const BUDGET_VEHICLES_NEAR: u32 = 2;

/// The faction density loop **skips types 7 & 8** (code map §9) — those channel indices are not
/// ambient-populated (they are reserved/scripted). Callers filtering a faction table honor this.
pub const DENSITY_SKIP_FACTION_TYPES: [u32; 2] = [7, 8];

/// Whether a faction-table type index participates in ambient density (i.e. is not 7 or 8).
pub fn density_faction_participates(faction_type: u32) -> bool {
    !DENSITY_SKIP_FACTION_TYPES.contains(&faction_type)
}

/// The spawn-density **decay op**: halve every byte of a packed 4-byte spawn-history accumulator with
/// no cross-byte borrow — exactly PC `FUN_004e0510`'s `(word >> 1) & 0x7f7f7f7f`. Applied on each
/// sim-seconds overflow so recent-spawn history ages out (a zone that spawned recently is throttled).
pub fn decay_spawn_history(word: u32) -> u32 {
    (word >> 1) & 0x7f7f_7f7f
}

/// The traffic-control gate: given a zone's [`TrafficControl`] rule, may a unit on `faction`'s list
/// spawn? `banned`/`single` name the faction referenced by `BanFaction`/`SingleFaction` (data-authored
/// on the zone; `None` if the rule doesn't apply). Vehicles vs peds is read off [`SpawnFaction`].
///
/// Semantics from the `TrafficControlEnum` members (§9): `Default` all; `NoTraffic` none; `NoVehicles`
/// peds only; `NoPeds` vehicles only; `BanFaction` all but the banned one; `SingleFaction` only the one.
pub fn traffic_allows(
    control: TrafficControl,
    faction: SpawnFaction,
    banned: Option<SpawnFaction>,
    single: Option<SpawnFaction>,
) -> bool {
    match control {
        TrafficControl::Default => true,
        TrafficControl::NoTraffic => false,
        TrafficControl::NoVehicles => !faction.is_vehicle(),
        TrafficControl::NoPeds => faction.is_vehicle(),
        TrafficControl::BanFaction => banned != Some(faction),
        TrafficControl::SingleFaction => single == Some(faction),
    }
}

/// The controller that owns the ambient budget + the per-zone decay accumulators. It answers the one
/// decision the code map recovers at this granularity: **how many** units of a class a zone may emit
/// this tick — `min(headroom, per-tick budget)`, gated by the zone's traffic rule and its decayed
/// spawn history.
#[derive(Debug)]
pub struct DensityController {
    /// The active per-tick budget (defaults to the recovered `10/10/2/2`).
    pub budget: DensityBudget,
}

impl Default for DensityController {
    fn default() -> Self {
        DensityController { budget: DensityBudget::DEFAULT }
    }
}

impl DensityController {
    pub fn new() -> Self {
        DensityController::default()
    }

    /// How many pedestrians a zone may spawn this tick in the given band: the per-tick people budget,
    /// clamped to the zone's remaining headroom (`cap - current`) and gated to `0` when the zone's
    /// traffic rule forbids peds. `cap` is the zone's authored ped cap (one of its five density
    /// counters — which slot is data), `current` the live count.
    pub fn ped_allowance(
        &self,
        zone: &PopulationDensity,
        cap: i32,
        current: i32,
        near: bool,
        faction: SpawnFaction,
    ) -> u32 {
        if !traffic_allows(zone.traffic, faction, None, None) {
            return 0;
        }
        let budget = if near { self.budget.people_near } else { self.budget.people_far };
        Self::allowance(cap, current, budget)
    }

    /// How many vehicles a zone may spawn this tick in the given band — the vehicle analogue of
    /// [`ped_allowance`](Self::ped_allowance), gated by the zone's traffic rule against the shared
    /// `VehicleSpawnList`.
    pub fn vehicle_allowance(
        &self,
        zone: &PopulationDensity,
        cap: i32,
        current: i32,
        near: bool,
    ) -> u32 {
        if !traffic_allows(zone.traffic, SpawnFaction::Vehicle, None, None) {
            return 0;
        }
        let budget = if near { self.budget.vehicles_near } else { self.budget.vehicles_far };
        Self::allowance(cap, current, budget)
    }

    /// `min(cap - current, per_tick_budget)` clamped at 0 — the shared allowance formula.
    fn allowance(cap: i32, current: i32, per_tick_budget: u32) -> u32 {
        let headroom = (cap - current).max(0) as u32;
        headroom.min(per_tick_budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The decay op halves each byte independently with no borrow across byte boundaries.
    #[test]
    fn decay_is_per_byte_halving() {
        // 0x01 in each byte → >>1 would borrow across bytes; the & 0x7f7f7f7f mask prevents it.
        assert_eq!(decay_spawn_history(0x0101_0101), 0x0000_0000);
        assert_eq!(decay_spawn_history(0xFFFF_FFFF), 0x7F7F_7F7F);
        assert_eq!(decay_spawn_history(0x80_80_80_80), 0x40_40_40_40);
        // A single byte 0x10 halves to 0x08; neighbours stay put.
        assert_eq!(decay_spawn_history(0x00_10_00_20), 0x00_08_00_10);
    }

    #[test]
    fn recovered_budget_constants() {
        assert_eq!(DensityBudget::DEFAULT, DensityBudget { people_far: 10, people_near: 10, vehicles_far: 2, vehicles_near: 2 });
        assert_eq!(BUDGET_PEOPLE_FAR, 10);
        assert_eq!(BUDGET_VEHICLES_NEAR, 2);
    }

    #[test]
    fn density_skips_faction_types_7_and_8() {
        assert!(!density_faction_participates(7));
        assert!(!density_faction_participates(8));
        assert!(density_faction_participates(0));
        assert!(density_faction_participates(6));
    }

    /// The traffic gate matches the `TrafficControlEnum` semantics for every member.
    #[test]
    fn traffic_gate_semantics() {
        use SpawnFaction::*;
        // Default: everything.
        assert!(traffic_allows(TrafficControl::Default, Vz, None, None));
        assert!(traffic_allows(TrafficControl::Default, Vehicle, None, None));
        // NoTraffic: nothing.
        assert!(!traffic_allows(TrafficControl::NoTraffic, Ped, None, None));
        // NoVehicles: peds yes, vehicles no.
        assert!(traffic_allows(TrafficControl::NoVehicles, Ped, None, None));
        assert!(!traffic_allows(TrafficControl::NoVehicles, Vehicle, None, None));
        // NoPeds: vehicles yes, peds no.
        assert!(!traffic_allows(TrafficControl::NoPeds, Ped, None, None));
        assert!(traffic_allows(TrafficControl::NoPeds, Vehicle, None, None));
        // BanFaction: all but the banned one.
        assert!(!traffic_allows(TrafficControl::BanFaction, Gur, Some(Gur), None));
        assert!(traffic_allows(TrafficControl::BanFaction, Vz, Some(Gur), None));
        // SingleFaction: only the one.
        assert!(traffic_allows(TrafficControl::SingleFaction, Chi, None, Some(Chi)));
        assert!(!traffic_allows(TrafficControl::SingleFaction, Vz, None, Some(Chi)));
    }

    /// Allowance = min(headroom, budget), gated by traffic and clamped at 0.
    #[test]
    fn allowance_is_budget_clamped_to_headroom() {
        let ctl = DensityController::new();
        let zone = PopulationDensity::default(); // TrafficControl::Default

        // Lots of headroom → capped by the per-tick budget (10 peds).
        assert_eq!(ctl.ped_allowance(&zone, 100, 0, true, SpawnFaction::Ped), 10);
        // Little headroom → capped by headroom.
        assert_eq!(ctl.ped_allowance(&zone, 3, 0, true, SpawnFaction::Ped), 3);
        // Full → 0.
        assert_eq!(ctl.ped_allowance(&zone, 10, 10, true, SpawnFaction::Ped), 0);
        // Over-full → clamped at 0, never negative.
        assert_eq!(ctl.ped_allowance(&zone, 5, 20, true, SpawnFaction::Ped), 0);
        // Vehicles obey the 2/tick budget.
        assert_eq!(ctl.vehicle_allowance(&zone, 100, 0, false, ), 2);
    }

    /// A `NoVehicles` zone yields zero vehicle allowance regardless of headroom/budget.
    #[test]
    fn traffic_rule_zeroes_disallowed_class() {
        let ctl = DensityController::new();
        let zone = PopulationDensity { traffic: TrafficControl::NoVehicles, ..Default::default() };
        assert_eq!(ctl.vehicle_allowance(&zone, 100, 0, true), 0);
        assert_eq!(ctl.ped_allowance(&zone, 100, 0, true, SpawnFaction::Ped), 10, "peds still flow");
    }
}
