//! Pursuit ("heat") state — the per-faction wanted level + dwell countdown (code map §5).
//!
//! A native singleton (per faction here) with a `Level` clamped `0..=3` and a per-level dwell timer.
//! `IncrementPursuit` bumps the level when a faction's relation vs the PMC hits `≤ -100`; the level
//! auto-decays after its dwell time elapses. The escalation *policy* is Lua; the native side owns the
//! **level state + countdown**, which is what this models. The cfunc VAs (`Pg.SetPursuit*`) are
//! binding-table-only in the exe — their bodies are not needed to reproduce the mechanism.

/// Max pursuit level (`min(Level+1, 3)`, §5).
pub const PURSUIT_LEVEL_MAX: u8 = 3;

/// Per-level dwell seconds from `Pg.SetPursuitLevelTimes(120, 300)` (`Setup :367`): level 1 = 120 s,
/// level 2 = 300 s.
pub const PURSUIT_DWELL_L1_SECS: f32 = 120.0;
pub const PURSUIT_DWELL_L2_SECS: f32 = 300.0;

/// The short re-arm applied by `Pg.SetPursuitSeconds(uFaction, 5, …)` on each fresh escalation (§5).
pub const PURSUIT_REARM_SECS: f32 = 5.0;

/// Dwell seconds for a level, or `None` when no auto-decay time is recovered.
///
/// **Honesty:** `SetPursuitLevelTimes` supplies only two values (L1, L2). The map does **not**
/// recover a level-3 dwell — so level 3 has **no auto-decay time** here (it holds until explicitly
/// cleared or de-escalated from below). This is a documented gap, not an invented number.
pub fn dwell_secs(level: u8) -> Option<f32> {
    match level {
        1 => Some(PURSUIT_DWELL_L1_SECS),
        2 => Some(PURSUIT_DWELL_L2_SECS),
        // Level 3 dwell is NOT in the recovered `SetPursuitLevelTimes(120,300)` call — confirm-live.
        _ => None,
    }
}

/// One faction's pursuit state: current level + the countdown until it decays a level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PursuitState {
    /// Current wanted level, `0..=3` (`0` = not pursued).
    pub level: u8,
    /// Seconds remaining before the level decays by one. Meaningless while `level == 0` or while the
    /// level has no recovered dwell (level 3).
    pub timer: f32,
    /// Whether the level is locked (`Pg.LockPursuit`) — a locked level neither decays nor is bumped.
    pub locked: bool,
}

impl PursuitState {
    pub fn new() -> Self {
        PursuitState::default()
    }

    /// `IncrementPursuit` (§5): raise the level by one (capped at 3) and (re)arm the countdown. On a
    /// bump the timer is set to the short re-arm (`SetPursuitSeconds(5)`); the full per-level dwell
    /// takes over once the fresh-escalation window passes (modelled by [`settle`]). A locked level is
    /// left unchanged. Returns whether the level actually changed.
    pub fn increment(&mut self) -> bool {
        if self.locked || self.level >= PURSUIT_LEVEL_MAX {
            // still refresh the timer on a re-report even at max level
            if !self.locked && self.level > 0 {
                self.timer = self.timer.max(PURSUIT_REARM_SECS);
            }
            return false;
        }
        self.level += 1;
        self.timer = PURSUIT_REARM_SECS;
        true
    }

    /// Promote the short re-arm window to the level's full dwell, if one is recovered. Called once the
    /// re-arm window elapses so the level then rides its real dwell (120 / 300 s).
    pub fn settle(&mut self) {
        if let Some(d) = dwell_secs(self.level) {
            self.timer = self.timer.max(d);
        }
    }

    /// `Pg.LockPursuit(level)` — pin the level (no decay, no bump).
    pub fn lock(&mut self, level: u8) {
        self.level = level.min(PURSUIT_LEVEL_MAX);
        self.locked = true;
    }

    /// `Pg.ClearPursuitLock` — unpin.
    pub fn clear_lock(&mut self) {
        self.locked = false;
    }

    /// Advance the dwell countdown by `dt` seconds. When it expires the level decays by one and the
    /// timer re-arms to the new level's dwell (or clears at level 0). A locked level, a level-0 state,
    /// and a level with no recovered dwell (level 3) do not decay.
    pub fn tick(&mut self, dt: f32) {
        if self.locked || self.level == 0 {
            return;
        }
        if dwell_secs(self.level).is_none() {
            // No recovered auto-decay for this level (level 3) — hold.
            return;
        }
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.level -= 1;
            self.timer = if self.level == 0 { 0.0 } else { dwell_secs(self.level).unwrap_or(0.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_dwell_times() {
        assert_eq!(dwell_secs(1), Some(120.0));
        assert_eq!(dwell_secs(2), Some(300.0));
        assert_eq!(dwell_secs(3), None, "level-3 dwell is not recovered (confirm-live)");
        assert_eq!(PURSUIT_LEVEL_MAX, 3);
    }

    #[test]
    fn increment_caps_at_three() {
        let mut p = PursuitState::new();
        assert!(p.increment());
        assert_eq!(p.level, 1);
        assert_eq!(p.timer, PURSUIT_REARM_SECS);
        p.increment();
        p.increment();
        assert_eq!(p.level, 3);
        assert!(!p.increment(), "cannot exceed 3");
        assert_eq!(p.level, 3);
    }

    #[test]
    fn decays_a_level_after_dwell() {
        let mut p = PursuitState::new();
        p.increment(); // level 1, timer 5 (re-arm)
        p.settle(); // timer now full L1 dwell = 120
        assert_eq!(p.timer, 120.0);
        p.tick(119.0);
        assert_eq!(p.level, 1);
        p.tick(2.0); // crosses 0
        assert_eq!(p.level, 0, "decayed back to unpursued");
    }

    #[test]
    fn locked_level_neither_decays_nor_bumps() {
        let mut p = PursuitState::new();
        p.lock(2);
        assert_eq!(p.level, 2);
        assert!(!p.increment());
        p.tick(1000.0);
        assert_eq!(p.level, 2, "locked level holds");
        p.clear_lock();
        assert!(p.increment());
        assert_eq!(p.level, 3);
    }

    #[test]
    fn level_three_holds_without_recovered_dwell() {
        let mut p = PursuitState::new();
        p.lock(3);
        p.clear_lock();
        p.tick(100000.0);
        assert_eq!(p.level, 3, "level 3 has no recovered auto-decay — holds");
    }
}
