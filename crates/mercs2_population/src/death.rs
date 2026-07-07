//! The population death system — `DeathCheck` / `DeathCompute` (code map §4, H confidence).
//!
//! PC `FUN_00500b40` (DeathCheck) walks the pending-death list, resolves each object, **decrements a
//! per-entry timer by `dt`**, distance-gates it, then either decays or removes it via `FUN_00500ac0`
//! (DeathCompute). The Xbox budget is **20 units/frame** (`li 0x14`), and Xbox `FUN_8235efc0` carries
//! the per-viewport **squared-distance table** below. This frees the population budget by retiring
//! dead/despawnable bodies that have aged out and are far from every viewport.
//!
//! **Confirm-live (§10.4):** the distance table is read from the **Xbox** `FUN_8235efc0`; the PC gate
//! uses the same distance-select logic but the numeric constants were not read out on PC. They are
//! encoded here as the Xbox-recovered values, flagged for a live PC confirm — not invented.

use mercs2_core::glam::Vec3;
use mercs2_core::Entity;

/// DeathCheck per-frame budget — at most this many pending-death entries are processed per tick
/// (Xbox `li 0x14` = 20; PC round-robin), so a mass-death event retires over several frames.
pub const DEATH_BUDGET_PER_FRAME: u32 = 20;

/// The per-viewport squared-distance gate table (Xbox `FUN_8235efc0`, code map §4): a body beyond the
/// selected radius² from the nearest viewport is eligible for removal. Values are meters² —
/// `50² / 250² / 200² / 400² / 100² / 150² / 160² / 80² / 140²`. **PC confirm-live** (§10.4).
pub const DEATH_DISTANCE_SQ_TABLE: [f32; 9] =
    [2500.0, 62500.0, 40000.0, 160000.0, 10000.0, 22500.0, 25600.0, 6400.0, 19600.0];

/// One entry on the pending-death list (`DAT_00dd12e8`, count `DAT_00dd13ec`). A dead/despawnable body
/// queued for retirement: an entity, a countdown timer (`DAT_017959d0[i*0xc]`, decremented by `dt`),
/// and which [`DEATH_DISTANCE_SQ_TABLE`] gate it uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingDeath {
    /// The world entity queued for removal.
    pub entity: Entity,
    /// Seconds until this body is eligible to retire (decremented by `dt` each check).
    pub timer: f32,
    /// Index into [`DEATH_DISTANCE_SQ_TABLE`] selecting the distance² gate for this body.
    pub gate: usize,
    /// The body's world position (for the distance gate).
    pub position: Vec3,
}

impl PendingDeath {
    /// The squared-distance gate this entry uses (clamped to the table).
    pub fn gate_sq(&self) -> f32 {
        DEATH_DISTANCE_SQ_TABLE[self.gate.min(DEATH_DISTANCE_SQ_TABLE.len() - 1)]
    }
}

/// The pending-death list + the budgeted per-frame retirement check.
#[derive(Default)]
pub struct DeathQueue {
    pending: Vec<PendingDeath>,
    /// Round-robin cursor so the 20/frame budget sweeps the whole list over successive frames rather
    /// than always re-processing the head (the PC round-robin, §4).
    cursor: usize,
}

impl DeathQueue {
    pub fn new() -> Self {
        DeathQueue::default()
    }

    /// Queue a body for retirement.
    pub fn push(&mut self, entry: PendingDeath) {
        self.pending.push(entry);
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// `DeathCheck` — process up to [`DEATH_BUDGET_PER_FRAME`] entries this tick, round-robin. Each
    /// processed entry's timer decrements by `dt`; when it reaches `0` **and** the body is beyond its
    /// distance gate from the nearest viewport, it retires (returned in the removal list and dropped
    /// from the pending list). An expired-but-still-close body stays queued (the "decay" branch —
    /// keep it alive near the player). Returns the entities to remove this frame.
    pub fn check(&mut self, dt: f32, viewports: &[Vec3]) -> Vec<Entity> {
        let mut removed = Vec::new();
        if self.pending.is_empty() {
            return removed;
        }
        let budget = (DEATH_BUDGET_PER_FRAME as usize).min(self.pending.len());
        // Collect indices to remove after the sweep (removing mid-iteration would shift the cursor).
        let mut retire_idx = Vec::new();
        for _ in 0..budget {
            if self.pending.is_empty() {
                break;
            }
            let i = self.cursor % self.pending.len();
            let entry = &mut self.pending[i];
            entry.timer -= dt;
            if entry.timer <= 0.0 && beyond_gate(entry.position, entry.gate_sq(), viewports) {
                retire_idx.push(i);
            }
            self.cursor = self.cursor.wrapping_add(1);
        }
        // Remove retired entries high-index-first so earlier indices stay valid.
        retire_idx.sort_unstable();
        retire_idx.dedup();
        for &i in retire_idx.iter().rev() {
            removed.push(self.pending.remove(i).entity);
        }
        if !self.pending.is_empty() {
            self.cursor %= self.pending.len();
        } else {
            self.cursor = 0;
        }
        removed
    }
}

/// Whether `pos` is beyond `gate_sq` (squared meters) from *every* viewport — i.e. far enough from all
/// cameras to retire. With no viewports the body is trivially "far" (offscreen world).
fn beyond_gate(pos: Vec3, gate_sq: f32, viewports: &[Vec3]) -> bool {
    if viewports.is_empty() {
        return true;
    }
    viewports.iter().all(|&v| (pos - v).length_squared() > gate_sq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::World;

    #[test]
    fn recovered_death_constants() {
        assert_eq!(DEATH_BUDGET_PER_FRAME, 20);
        assert_eq!(DEATH_DISTANCE_SQ_TABLE[0], 50.0 * 50.0);
        assert_eq!(DEATH_DISTANCE_SQ_TABLE[3], 400.0 * 400.0);
        assert_eq!(DEATH_DISTANCE_SQ_TABLE[6], 160.0 * 160.0);
        assert_eq!(DEATH_DISTANCE_SQ_TABLE.len(), 9);
    }

    /// A body whose timer has expired and is beyond its gate retires; one still close stays.
    #[test]
    fn expired_and_far_retires_expired_and_near_stays() {
        let mut w = World::new();
        let far = w.spawn(());
        let near = w.spawn(());
        let mut q = DeathQueue::new();
        // gate 0 = 50² = 2500. viewport at origin.
        q.push(PendingDeath { entity: far, timer: 0.1, gate: 0, position: Vec3::new(100.0, 0.0, 0.0) });
        q.push(PendingDeath { entity: near, timer: 0.1, gate: 0, position: Vec3::new(10.0, 0.0, 0.0) });

        let removed = q.check(0.2, &[Vec3::ZERO]); // dt drives both timers to <=0
        assert_eq!(removed, vec![far], "far+expired retires");
        assert_eq!(q.len(), 1, "near+expired stays queued (decay branch)");
    }

    /// A body still on its timer is not retired even when far.
    #[test]
    fn unexpired_stays_even_when_far() {
        let mut w = World::new();
        let e = w.spawn(());
        let mut q = DeathQueue::new();
        q.push(PendingDeath { entity: e, timer: 5.0, gate: 0, position: Vec3::new(1000.0, 0.0, 0.0) });
        assert!(q.check(0.1, &[Vec3::ZERO]).is_empty());
        assert_eq!(q.len(), 1);
    }

    /// The 20/frame budget bounds work per tick: 25 far, expired bodies retire at most 20 in one check.
    #[test]
    fn budget_caps_removals_per_frame() {
        let mut w = World::new();
        let mut q = DeathQueue::new();
        for _ in 0..25 {
            let e = w.spawn(());
            q.push(PendingDeath { entity: e, timer: 0.0, gate: 0, position: Vec3::new(1000.0, 0.0, 0.0) });
        }
        let first = q.check(0.1, &[Vec3::ZERO]);
        assert_eq!(first.len(), DEATH_BUDGET_PER_FRAME as usize, "at most 20 retired this frame");
        let second = q.check(0.1, &[Vec3::ZERO]);
        assert_eq!(second.len(), 5, "the remaining 5 retire next frame");
        assert!(q.is_empty());
    }
}
