//! The combat→faction **mood bridge** — the 7-key infraction accumulator + its report weighting.
//!
//! Code map §2 (`FUN_005e0720`, read first-hand) + §3 mood weights (`mrxfactionmanager.lua:1212-1219`).
//!
//! Hostile acts accrue into a per-faction **7-slot accumulator**, each slot a `{id, score}` pair —
//! the exact `7×{id,score}` array the native serializer `FUN_005e0720` walks and emits under the
//! seven literal key strings (this is the *can't-coincide* H fingerprint of the code map). When a
//! faction NPC "reports" you, the accumulator is **weighted** into a single mood value which becomes
//! the relation change vs the PMC. The weighting is Lua policy (`FinishedReporting`), reproduced
//! here verbatim; the native side owns only the accumulate + serialize.

/// The seven infraction keys, in the **physical accumulator slot order** (`FUN_005e0720` reads pair
/// `puVar2[2*slot]=id`, `puVar2[2*slot+1]=score`). The enum discriminant == the slot index.
///
/// Note the serializer's *emit* order differs from slot order (see [`EMIT_ORDER`]); the mood
/// weighting is order-independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum InfractionKind {
    /// Hurting a person — weight ×3.
    DamagePerson = 0,
    /// Damaging an object/vehicle/building — weight ×1.
    DamageObject = 1,
    /// Killing a person — weight ×50.
    DestroyPerson = 2,
    /// Destroying an object — weight ×25.
    DestroyObject = 3,
    /// Hijacking a vehicle — weight ×10.
    Hijack = 4,
    /// Entering a faction-owned zone (fed by `FactionZone`) — weight ×20.
    Trespassing = 5,
    /// A scripted special infraction — weighted by its own `id` field (`id × score`), not a constant.
    SpecialEvent = 6,
}

/// Number of infraction keys (accumulator slots).
pub const INFRACTION_KINDS: usize = 7;

impl InfractionKind {
    /// All seven kinds in slot order.
    pub const ALL: [InfractionKind; INFRACTION_KINDS] = [
        InfractionKind::DamagePerson,
        InfractionKind::DamageObject,
        InfractionKind::DestroyPerson,
        InfractionKind::DestroyObject,
        InfractionKind::Hijack,
        InfractionKind::Trespassing,
        InfractionKind::SpecialEvent,
    ];

    /// The literal event-name key the native serializer emits for this slot (code map §2 string
    /// table `s_DamagePerson_00bb3d48` … `s_SpecialEvent_00bb994c`).
    pub fn key(self) -> &'static str {
        match self {
            InfractionKind::DamagePerson => "DamagePerson",
            InfractionKind::DamageObject => "DamageObject",
            InfractionKind::DestroyPerson => "DestroyPerson",
            InfractionKind::DestroyObject => "DestroyObject",
            InfractionKind::Hijack => "Hijack",
            InfractionKind::Trespassing => "Trespassing",
            InfractionKind::SpecialEvent => "SpecialEvent",
        }
    }

    /// The fixed mood weight for this key (`mrxfactionmanager.lua:1212-1219`). `SpecialEvent` has
    /// **no** constant weight — it multiplies its own `id × score` — so this returns `None` for it.
    pub fn weight(self) -> Option<i32> {
        match self {
            InfractionKind::DamagePerson => Some(3),
            InfractionKind::DamageObject => Some(1),
            InfractionKind::DestroyPerson => Some(50),
            InfractionKind::DestroyObject => Some(25),
            InfractionKind::Hijack => Some(10),
            InfractionKind::Trespassing => Some(20),
            InfractionKind::SpecialEvent => None,
        }
    }
}

/// The serializer's *write* order in `FUN_005e0720` (§2), which differs from the slot order above:
/// DamagePerson, DestroyPerson, DamageObject, DestroyObject, Hijack, Trespassing, SpecialEvent.
/// Recorded for faithfulness; the mood weighting does not depend on it.
pub const EMIT_ORDER: [InfractionKind; INFRACTION_KINDS] = [
    InfractionKind::DamagePerson,
    InfractionKind::DestroyPerson,
    InfractionKind::DamageObject,
    InfractionKind::DestroyObject,
    InfractionKind::Hijack,
    InfractionKind::Trespassing,
    InfractionKind::SpecialEvent,
];

/// One accumulator slot — the `{id, score}` pair the native `7×{id,score}` array stores per key.
/// `score` is the accumulated count/amount; `id` is the owner/faction id (and, for `SpecialEvent`,
/// the per-event multiplier).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfractionSlot {
    pub id: i32,
    pub score: i32,
}

/// The **mood clamp floor** (`mrxfactionmanager.lua:1212`, "clamp ≥ −60"). The weighted mood is
/// clamped to `≥ -60` *before* it is negated into the relation delta, so a single favourable report
/// can raise a relation by at most `+60`; unfavourable reports are not floored here (the relation
/// itself clamps at `-100`).
pub const MOOD_CLAMP_MIN: i32 = -60;

/// The per-faction 7-key infraction accumulator — the `7×{id,score}` array `FUN_005e0720` serializes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfractionAccumulator {
    /// One slot per [`InfractionKind`], indexed by the enum discriminant (slot order).
    pub slots: [InfractionSlot; INFRACTION_KINDS],
}

impl InfractionAccumulator {
    pub fn new() -> Self {
        InfractionAccumulator::default()
    }

    /// `Ai.AddInfraction`-side accrual: add `amount` to a key's score (and stamp the owner `id` if it
    /// is still the default `0`). The native combat/destruction/hijack/trespass paths each route to
    /// the slot matching the act; `SpecialEvent` additionally carries a caller-supplied multiplier as
    /// its `id` (via [`add_special`]).
    ///
    /// [`add_special`]: InfractionAccumulator::add_special
    pub fn add(&mut self, kind: InfractionKind, id: i32, amount: i32) {
        let slot = &mut self.slots[kind as usize];
        slot.score += amount;
        if slot.id == 0 {
            slot.id = id;
        }
    }

    /// `SpecialEvent` accrual: its mood term is `multiplier × amount` (the Lua `SpecialEvent[1]×[2]`),
    /// so the multiplier is stored in the slot `id`. Sets the multiplier and accrues the amount.
    pub fn add_special(&mut self, multiplier: i32, amount: i32) {
        let slot = &mut self.slots[InfractionKind::SpecialEvent as usize];
        slot.id = multiplier;
        slot.score += amount;
    }

    /// The weighted mood, **before** the clamp/negation (`mrxfactionmanager.lua:1212-1219`):
    /// `DamageObject×1 + DestroyObject×25 + Trespassing×20 + Hijack×10 + SpecialEvent[1]×[2]
    ///  + DestroyPerson×50 + DamagePerson×3`.
    pub fn raw_mood(&self) -> i32 {
        let mut mood = 0i32;
        for kind in InfractionKind::ALL {
            let slot = self.slots[kind as usize];
            mood += match kind.weight() {
                Some(w) => slot.score.saturating_mul(w),
                // SpecialEvent: id (multiplier) × score.
                None => slot.id.saturating_mul(slot.score),
            };
        }
        mood
    }

    /// The **relation delta** a report of this accumulator produces vs the PMC
    /// (`ChangeRelation(faction,"Pmc", -nMood)` with the `nMood ≥ -60` clamp): the mood is clamped to
    /// `≥ MOOD_CLAMP_MIN` then negated. Negative = the faction likes the PMC less.
    pub fn relation_delta(&self) -> i32 {
        -(self.raw_mood().max(MOOD_CLAMP_MIN))
    }

    /// Whether any infraction has been accrued (an empty accumulator produces no report).
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| s.score == 0)
    }

    /// Clear the accumulator (a report consumes it).
    pub fn clear(&mut self) {
        *self = InfractionAccumulator::default();
    }
}

/// The **civilian-casualty** collateral penalty curve (`mrxfactionmanager.lua:815-823`): starts at
/// `-5000`, **doubles every 20 kills**, floored at `-1,000,000`. Returns the (negative) penalty for a
/// running civilian kill count. This rides the same relation-change path (plus a `CollateralDamage`
/// event + a cash penalty) as a report; the numbers are the recovered Lua policy.
pub const CIVILIAN_PENALTY_BASE: i64 = -5000;
/// Civilian kills per doubling of the collateral penalty.
pub const CIVILIAN_PENALTY_DOUBLE_EVERY: u32 = 20;
/// Floor of the collateral penalty (most negative it can get).
pub const CIVILIAN_PENALTY_FLOOR: i64 = -1_000_000;

/// The collateral penalty for `kills` civilian casualties: `-5000 · 2^(kills/20)`, floored at `-1M`.
pub fn civilian_casualty_penalty(kills: u32) -> i64 {
    let doublings = kills / CIVILIAN_PENALTY_DOUBLE_EVERY;
    // Saturate the shift well before it can overflow i64 (2^63); the floor clamps it anyway.
    let scaled = if doublings >= 63 {
        CIVILIAN_PENALTY_FLOOR
    } else {
        CIVILIAN_PENALTY_BASE.saturating_mul(1i64 << doublings)
    };
    scaled.max(CIVILIAN_PENALTY_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seven keys, their slot order, and the emit-order permutation all match the code map.
    #[test]
    fn seven_keys_slot_and_emit_order() {
        assert_eq!(InfractionKind::ALL.len(), 7);
        let keys: Vec<&str> = InfractionKind::ALL.iter().map(|k| k.key()).collect();
        assert_eq!(
            keys,
            vec![
                "DamagePerson",
                "DamageObject",
                "DestroyPerson",
                "DestroyObject",
                "Hijack",
                "Trespassing",
                "SpecialEvent"
            ]
        );
        // slot index == enum discriminant
        assert_eq!(InfractionKind::DamagePerson as usize, 0);
        assert_eq!(InfractionKind::SpecialEvent as usize, 6);
        // emit order is the FUN_005e0720 write sequence (Destroy/DamagePerson swapped vs slots)
        let emit: Vec<&str> = EMIT_ORDER.iter().map(|k| k.key()).collect();
        assert_eq!(emit[1], "DestroyPerson");
        assert_eq!(emit[2], "DamageObject");
    }

    /// The recovered mood weights, verbatim.
    #[test]
    fn mood_weights_verbatim() {
        assert_eq!(InfractionKind::DamagePerson.weight(), Some(3));
        assert_eq!(InfractionKind::DamageObject.weight(), Some(1));
        assert_eq!(InfractionKind::DestroyPerson.weight(), Some(50));
        assert_eq!(InfractionKind::DestroyObject.weight(), Some(25));
        assert_eq!(InfractionKind::Hijack.weight(), Some(10));
        assert_eq!(InfractionKind::Trespassing.weight(), Some(20));
        assert_eq!(InfractionKind::SpecialEvent.weight(), None);
    }

    /// A worked mood computation: 2 kills + 1 hijack + 3 trespass = 2·50 + 1·10 + 3·20 = 170.
    #[test]
    fn raw_mood_weighted_sum() {
        let mut acc = InfractionAccumulator::new();
        acc.add(InfractionKind::DestroyPerson, 7, 2);
        acc.add(InfractionKind::Hijack, 7, 1);
        acc.add(InfractionKind::Trespassing, 7, 3);
        assert_eq!(acc.raw_mood(), 2 * 50 + 1 * 10 + 3 * 20);
        // relation delta is the negated mood (mood > 0 so the clamp is inert).
        assert_eq!(acc.relation_delta(), -170);
    }

    /// SpecialEvent is `id × score` (its own multiplier), not a fixed weight.
    #[test]
    fn special_event_uses_multiplier() {
        let mut acc = InfractionAccumulator::new();
        acc.add_special(4, 5); // 4 × 5 = 20
        assert_eq!(acc.raw_mood(), 20);
    }

    /// The `≥ -60` mood clamp caps a favourable report's relation bonus at `+60`.
    #[test]
    fn favourable_mood_clamps_bonus_at_60() {
        let mut acc = InfractionAccumulator::new();
        // A large negative mood (a "favour"): SpecialEvent multiplier -1 × score 500 = -500.
        acc.add_special(-1, 500);
        assert_eq!(acc.raw_mood(), -500);
        // clamped to -60, negated → +60 max bonus.
        assert_eq!(acc.relation_delta(), 60);
    }

    #[test]
    fn accumulator_clear_and_empty() {
        let mut acc = InfractionAccumulator::new();
        assert!(acc.is_empty());
        acc.add(InfractionKind::DamagePerson, 1, 4);
        assert!(!acc.is_empty());
        acc.clear();
        assert!(acc.is_empty());
    }

    /// Civilian penalty: base −5000; doubles every 20 kills; floored at −1M.
    #[test]
    fn civilian_penalty_curve() {
        assert_eq!(civilian_casualty_penalty(0), -5000);
        assert_eq!(civilian_casualty_penalty(19), -5000);
        assert_eq!(civilian_casualty_penalty(20), -10_000); // one doubling
        assert_eq!(civilian_casualty_penalty(40), -20_000); // two doublings
        // deep into the curve it saturates at the floor and never overflows
        assert_eq!(civilian_casualty_penalty(100_000), CIVILIAN_PENALTY_FLOOR);
    }
}
