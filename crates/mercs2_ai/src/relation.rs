//! The AI relation matrix — `Ai.SetRelation` / `Ai.GetRelation`.
//!
//! Code map §5 + faction map: the AI order surface sets a directed **relation matrix** in `[-100, 100]`
//! between two GUIDs (typically factions). `Ai.SetRelation(a, b, v)` = "a's attitude toward b is v";
//! `-100` = kill-on-sight, `+100` = allied. The combat→faction loop reads this to drive price scaling /
//! pursuit / HUD colour (see `faction_reputation_code_map.md`); here we own the matrix mechanism.

use std::collections::HashMap;

/// The clamped relation range recovered from the faction loop.
pub const RELATION_MIN: i32 = -100;
pub const RELATION_MAX: i32 = 100;

/// Directed attitude matrix keyed by `(from, to)` GUID → value in `[-100, 100]`. An unset pair reads
/// as `0` (neutral). Directed (not symmetric): the engine lets A hate B without B hating A.
#[derive(Default)]
pub struct RelationMatrix {
    by_pair: HashMap<(u32, u32), i32>,
}

impl RelationMatrix {
    pub fn new() -> Self {
        RelationMatrix::default()
    }

    /// `Ai.SetRelation(from, to, value)` — set `from`'s attitude toward `to`, clamped to `[-100, 100]`.
    pub fn set(&mut self, from: u32, to: u32, value: i32) {
        self.by_pair.insert((from, to), value.clamp(RELATION_MIN, RELATION_MAX));
    }

    /// `Ai.GetRelation(from, to)` — `from`'s attitude toward `to`; `0` (neutral) if never set.
    pub fn get(&self, from: u32, to: u32) -> i32 {
        self.by_pair.get(&(from, to)).copied().unwrap_or(0)
    }

    /// Whether `from` is hostile toward `to` (negative attitude) — the perception threat model uses
    /// this to decide if an observed entity counts as a hostile observer.
    pub fn is_hostile(&self, from: u32, to: u32) -> bool {
        self.get(from, to) < 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set/get roundtrips; unset pairs are neutral; the matrix is directed.
    #[test]
    fn set_get_roundtrip_and_directed() {
        let mut m = RelationMatrix::new();
        m.set(1, 2, -100);
        assert_eq!(m.get(1, 2), -100);
        assert_eq!(m.get(2, 1), 0, "relation is directed — the reverse pair is untouched");
        assert_eq!(m.get(3, 4), 0, "unset pair reads neutral");
        assert!(m.is_hostile(1, 2));
        assert!(!m.is_hostile(2, 1));
    }

    /// Values outside [-100, 100] are clamped (the faction loop's range).
    #[test]
    fn values_clamp_to_range() {
        let mut m = RelationMatrix::new();
        m.set(1, 2, 9999);
        m.set(3, 4, -9999);
        assert_eq!(m.get(1, 2), RELATION_MAX);
        assert_eq!(m.get(3, 4), RELATION_MIN);
    }
}
