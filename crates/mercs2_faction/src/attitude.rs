//! Attitude classification + price / meter policy (code map §3, `mrxfactionmanager.lua`).
//!
//! A relation value in `[-100, 100]` maps to one of three **attitude levels** (Hostile / Neutral /
//! Friendly), each with a shop **price multiplier**, a HUD **colour**, and a display **meter**. This
//! is the authoritative reimpl policy from the Lua — reproduced verbatim, **not** re-derived.

/// Relation range (`_knRelationMin/Max`, `mrxfactionmanager.lua:11`).
pub const RELATION_MIN: i32 = -100;
pub const RELATION_MAX: i32 = 100;

/// Meter range (`_knAttitudeMeterMin/Max`, `:9`).
pub const METER_MIN: f32 = 0.0;
pub const METER_MAX: f32 = 100.0;

/// Attitude-band thresholds (`:20/:35/:50`): Hostile `[-100,-33)`, Neutral `[-33,33)`,
/// Friendly `[33,100]`.
pub const HOSTILE_UPPER: i32 = -33;
pub const FRIENDLY_LOWER: i32 = 33;

/// The pursuit-escalation threshold (`FinishedReporting`, §1/§5): a relation `≤ -100` toward the PMC
/// triggers `IncrementPursuit`. Equals [`RELATION_MIN`] (the relation floor), so a maxed-out hostile
/// relation escalates heat.
pub const PURSUIT_ESCALATE_AT: i32 = RELATION_MIN;

/// One of the three faction attitude levels (`mrxfactionmanager.lua`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attitude {
    /// rel `[-100, -33)` — will not sell to the PMC (`nPrices = nil`), red.
    Hostile,
    /// rel `[-33, 33)` — sells at **1.5×**, grey.
    Neutral,
    /// rel `[33, 100]` — sells at **1.0×**, blue.
    Friendly,
}

impl Attitude {
    /// Classify a relation value into its attitude band (§3 thresholds). Values are clamped into
    /// range first, so out-of-range inputs still classify sanely.
    pub fn classify(relation: i32) -> Attitude {
        let r = relation.clamp(RELATION_MIN, RELATION_MAX);
        if r < HOSTILE_UPPER {
            Attitude::Hostile
        } else if r < FRIENDLY_LOWER {
            Attitude::Neutral
        } else {
            Attitude::Friendly
        }
    }

    /// The shop **price multiplier** for this attitude (`nPrices`): `None` = will not sell
    /// (Hostile); `Some(1.5)` Neutral; `Some(1.0)` Friendly. Shops multiply catalogue prices by it.
    pub fn price_multiplier(self) -> Option<f32> {
        match self {
            Attitude::Hostile => None,
            Attitude::Neutral => Some(1.5),
            Attitude::Friendly => Some(1.0),
        }
    }

    /// The HUD attitude **colour** (RGB), §3: Hostile 255/0/0, Neutral 200/200/200, Friendly 0/127/255.
    pub fn color(self) -> [u8; 3] {
        match self {
            Attitude::Hostile => [255, 0, 0],
            Attitude::Neutral => [200, 200, 200],
            Attitude::Friendly => [0, 127, 255],
        }
    }

    /// The localized label token for this attitude (`[Generic.Attitudes.<…>]`, §6).
    pub fn label(self) -> &'static str {
        match self {
            Attitude::Hostile => "Hostile",
            Attitude::Neutral => "Neutral",
            Attitude::Friendly => "Friendly",
        }
    }

    /// The **median** relation of this band — used for the recovered initial relations
    /// (`median(Neutral)`, `median(Friendly)`, §3 initial-relations row). Integer-floored midpoint of
    /// the band: Hostile `(-100 + -34)/2 = -67`, Neutral `(-33 + 32)/2 = -1`… — see the note below.
    ///
    /// **Honesty:** the Lua says `median(Neutral)=0` and `median(Friendly)=median of [33,100]`. We
    /// return the exact documented `median(Neutral) = 0`; for Friendly we return the band midpoint
    /// `(33 + 100) / 2 = 66` (integer floor). The precise Lua rounding of `median(Friendly)` is not
    /// pinned in the map — treat the Friendly median as **confirm-live** (±1).
    pub fn median(self) -> i32 {
        match self {
            Attitude::Hostile => (RELATION_MIN + (HOSTILE_UPPER - 1)) / 2, // [-100,-34] midpoint
            Attitude::Neutral => 0, // documented: median(Neutral) = 0
            Attitude::Friendly => (FRIENDLY_LOWER + RELATION_MAX) / 2, // [33,100] midpoint = 66
        }
    }
}

/// `ConvertRelationToMeterValue` (`:632`): `meter = 100·(rel + 100) / 200`, i.e. the relation
/// `[-100,100]` linearly mapped onto the meter `[0,100]`.
pub fn relation_to_meter(relation: i32) -> f32 {
    let r = relation.clamp(RELATION_MIN, RELATION_MAX) as f32;
    METER_MAX * (r - RELATION_MIN as f32) / (RELATION_MAX - RELATION_MIN) as f32
}

/// HUD faction-meter level thresholds `_tLevels = {0, 25, 50, 75}` (§6, `mrxguihudfactiongauge.lua`).
pub const METER_LEVELS: [u32; 4] = [0, 25, 50, 75];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_band_boundaries() {
        assert_eq!(Attitude::classify(-100), Attitude::Hostile);
        assert_eq!(Attitude::classify(-34), Attitude::Hostile);
        assert_eq!(Attitude::classify(-33), Attitude::Neutral); // -33 is Neutral's lower bound
        assert_eq!(Attitude::classify(0), Attitude::Neutral);
        assert_eq!(Attitude::classify(32), Attitude::Neutral);
        assert_eq!(Attitude::classify(33), Attitude::Friendly); // 33 is Friendly's lower bound
        assert_eq!(Attitude::classify(100), Attitude::Friendly);
        // out-of-range clamps
        assert_eq!(Attitude::classify(9999), Attitude::Friendly);
        assert_eq!(Attitude::classify(-9999), Attitude::Hostile);
    }

    #[test]
    fn price_and_color_policy() {
        assert_eq!(Attitude::Hostile.price_multiplier(), None);
        assert_eq!(Attitude::Neutral.price_multiplier(), Some(1.5));
        assert_eq!(Attitude::Friendly.price_multiplier(), Some(1.0));
        assert_eq!(Attitude::Hostile.color(), [255, 0, 0]);
        assert_eq!(Attitude::Neutral.color(), [200, 200, 200]);
        assert_eq!(Attitude::Friendly.color(), [0, 127, 255]);
    }

    #[test]
    fn meter_conversion_endpoints_and_mid() {
        assert_eq!(relation_to_meter(-100), 0.0);
        assert_eq!(relation_to_meter(100), 100.0);
        assert_eq!(relation_to_meter(0), 50.0);
        assert_eq!(METER_LEVELS, [0, 25, 50, 75]);
    }

    #[test]
    fn medians_match_recovered_initials() {
        assert_eq!(Attitude::Neutral.median(), 0); // median(Neutral) = 0, exact
        assert_eq!(Attitude::Friendly.median(), 66); // (33+100)/2 band midpoint
    }
}
