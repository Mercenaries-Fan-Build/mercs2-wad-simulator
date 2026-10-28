//! Entity activation — retail's `FUN_004F8DA0` (begin, then complete now or later) and the pending
//! queue its deferred path fills, pumped once a frame by the second half of `FUN_004F8460`
//! (`0x4F85FC..0x4F8D92`; `docs/reverse_engineer/hibernation_grid.md` §6).
//!
//! The queue is pure bookkeeping; the host supplies the entity side through [`ActivationHost`]:
//! begin (state 0 → 1, asset requests), the readiness poll `CanActivate` (`FUN_004F7710`), complete
//! (state 1 → 2), deactivate (`FUN_004F6AB0`), the microsecond clock and whether resource I/O is busy.

use std::collections::HashMap;

/// The completion budget a pass starts from, microseconds (`[0xD6CB48]` reset value).
pub const BUDGET_US: u64 = 1000;
/// Added to the budget after a pass that deferred a completion (`0x4F8C88`).
pub const BUDGET_STEP_US: u64 = 300;
/// The stall counter at which the head blocker gets a forced poll (`0x4F8CF8`).
pub const STALL_FORCED_POLL: u32 = 0x200;
/// The stall counter at which the head blocker is dropped and deactivated (`0x4F8D2E`).
pub const STALL_DEACTIVATE: u32 = 0x800;

/// Pending-record flags (`0x2478860`).
pub mod flags {
    /// `CanActivate` has succeeded.
    pub const READY: u8 = 1;
    /// SceneObject `+0x18` bit14 at enqueue: may complete behind an unready entry.
    pub const PRIORITY: u8 = 2;
    /// The caller's second argument.
    pub const A2: u8 = 8;
    /// Allowed to complete: without it the pump polls the entry but never completes it.
    pub const ALLOWED: u8 = 0x10;
}

/// The entity side of activation.
pub trait ActivationHost {
    /// Begin (`FUN_004F722E`): only from state 0 (→ 1), requesting the entity's assets. `Ok(false)`
    /// when a listener cancelled it (the latch `0xED3B18` names the entity).
    fn begin(&mut self, key: u32) -> Result<bool, String>;
    /// `CanActivate`: every polling listener ready. `forced` is the stall breaker's poll.
    fn can_activate(&mut self, key: u32, forced: bool) -> Result<bool, String>;
    /// Complete (`FUN_004F7C6A`): only from state 1 (→ 2).
    fn complete(&mut self, key: u32) -> Result<(), String>;
    /// Deactivate (`FUN_004F6AB0`): back to state 0.
    fn deactivate(&mut self, key: u32);
    /// The clock, microseconds.
    fn now_us(&mut self) -> u64;
    /// Resource I/O is waiting or loading (`[res+0x4C38C] + [res+0x4C39C]` non-zero).
    fn io_busy(&mut self) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Record {
    key: u32,
    serial: i32,
    flags: u8,
}

/// The pending container (`0xDF9C10`): dense records, removal swap-with-last, ordered by serial when
/// pumped.
#[derive(Clone, Debug)]
pub struct ActivationQueue {
    records: Vec<Record>,
    index: HashMap<u32, usize>,
    /// `[0xD6CB44]`: the next serial; reset to 0 when a pump finds the queue empty.
    next_serial: i32,
    /// `[0xD6CB48]`.
    budget_us: u64,
    /// `[0x198D8E4]` / `[0x198D8E0]`.
    blocker: u32,
    stall: u32,
}

impl Default for ActivationQueue {
    fn default() -> Self {
        ActivationQueue { records: Vec::new(), index: HashMap::new(), next_serial: 0, budget_us: BUDGET_US, blocker: 0, stall: 0 }
    }
}

impl ActivationQueue {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn contains(&self, key: u32) -> bool {
        self.index.contains_key(&key)
    }

    /// The pass budget, microseconds.
    pub fn budget_us(&self) -> u64 {
        self.budget_us
    }

    /// Enqueue (`0x4F8380` → `0x2478860`): a new record with the next serial, not ready. A key
    /// already pending is rewritten in place — a new serial (to the back), its ready bit cleared.
    pub fn enqueue(&mut self, key: u32, priority: bool, a2: bool, allowed: bool) {
        let mut f = 0u8;
        if priority {
            f |= flags::PRIORITY;
        }
        if a2 {
            f |= flags::A2;
        }
        if allowed {
            f |= flags::ALLOWED;
        }
        let rec = Record { key, serial: self.next_serial, flags: f };
        self.next_serial = self.next_serial.wrapping_add(1);
        match self.index.get(&key) {
            Some(&i) => self.records[i] = rec,
            None => {
                self.index.insert(key, self.records.len());
                self.records.push(rec);
            }
        }
    }

    /// Remove a pending entry (`0x649530`, swap-with-last).
    pub fn remove(&mut self, key: u32) -> bool {
        let Some(i) = self.index.remove(&key) else { return false };
        self.records.swap_remove(i);
        if let Some(moved) = self.records.get(i) {
            self.index.insert(moved.key, i);
        }
        true
    }

    /// Clear a pending entry's ready bit so it is polled again (link add `0x5183D4`).
    pub fn unready(&mut self, key: u32) {
        if let Some(&i) = self.index.get(&key) {
            self.records[i].flags &= !flags::READY;
        }
    }

    /// `FUN_004F8DA0(e, deferred, a2, allowed)`: begin; a cancel deactivates; deferred → enqueue;
    /// otherwise complete now when ready (and leave the queue), else enqueue. True when it completed.
    pub fn activate(&mut self, host: &mut impl ActivationHost, key: u32, deferred: bool, a2: bool, allowed: bool) -> Result<bool, String> {
        if !host.begin(key)? {
            host.deactivate(key);
            self.remove(key);
            return Ok(false);
        }
        if !deferred && host.can_activate(key, false)? {
            host.complete(key)?;
            self.remove(key);
            return Ok(true);
        }
        self.enqueue(key, false, a2, allowed);
        Ok(false)
    }

    /// `FUN_004F6AB0` on a pending entity: it leaves the queue, then deactivates.
    pub fn deactivate(&mut self, host: &mut impl ActivationHost, key: u32) {
        self.remove(key);
        host.deactivate(key);
    }

    /// One pump pass (`0x4F85FC..0x4F8D92`).
    pub fn pump(&mut self, host: &mut impl ActivationHost) -> Result<(), String> {
        if self.records.is_empty() {
            self.next_serial = 0;
            return Ok(());
        }
        let mut order: Vec<(i32, u32)> = self.records.iter().map(|r| (r.serial, r.key)).collect();
        order.sort_by_key(|&(s, _)| s);
        let deadline = host.now_us() + self.budget_us;
        let (mut no_unready, mut blocker, mut deferred) = (true, 0u32, false);
        for (_, key) in order {
            let Some(&i) = self.index.get(&key) else { continue };
            if self.records[i].flags & flags::READY == 0 {
                if host.can_activate(key, false)? {
                    self.records[i].flags |= flags::READY;
                } else {
                    if no_unready {
                        blocker = key;
                    }
                    no_unready = false;
                }
            }
            let f = self.records[i].flags;
            let eligible = f & flags::READY != 0 && (no_unready || f & flags::PRIORITY != 0) && f & flags::ALLOWED != 0;
            if !eligible {
                continue;
            }
            if host.now_us() >= deadline {
                deferred = true;
                continue;
            }
            host.complete(key)?;
            self.remove(key);
        }
        self.budget_us = if deferred { self.budget_us + BUDGET_STEP_US } else { BUDGET_US };

        if no_unready || blocker != self.blocker {
            self.blocker = if no_unready { 0 } else { blocker };
            self.stall = 0;
        } else {
            self.stall = if host.io_busy() { self.stall + 1 } else { (self.stall + 0x40) & !0x3F };
            if self.stall == STALL_FORCED_POLL {
                host.can_activate(blocker, true)?;
            } else if self.stall == STALL_DEACTIVATE {
                self.remove(blocker);
                host.deactivate(blocker);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct Host {
        state: HashMap<u32, u8>,
        ready: HashSet<u32>,
        cancel: HashSet<u32>,
        clock: u64,
        tick: u64,
        busy: bool,
        completed: Vec<u32>,
        deactivated: Vec<u32>,
        forced: Vec<u32>,
    }

    impl ActivationHost for Host {
        fn begin(&mut self, key: u32) -> Result<bool, String> {
            if self.state.get(&key).copied().unwrap_or(0) == 0 {
                self.state.insert(key, 1);
            }
            Ok(!self.cancel.contains(&key))
        }
        fn can_activate(&mut self, key: u32, forced: bool) -> Result<bool, String> {
            if forced {
                self.forced.push(key);
            }
            Ok(self.ready.contains(&key))
        }
        fn complete(&mut self, key: u32) -> Result<(), String> {
            if self.state.get(&key) == Some(&1) {
                self.state.insert(key, 2);
                self.completed.push(key);
            }
            Ok(())
        }
        fn deactivate(&mut self, key: u32) {
            self.state.remove(&key);
            self.deactivated.push(key);
        }
        fn now_us(&mut self) -> u64 {
            self.clock += self.tick;
            self.clock
        }
        fn io_busy(&mut self) -> bool {
            self.busy
        }
    }

    /// Ready entries complete in serial order in the pass; one that becomes ready completes in the same
    /// pass; the queue empties and the serial resets.
    #[test]
    fn ready_entries_complete_in_serial_order() {
        let (mut q, mut h) = (ActivationQueue::default(), Host::default());
        for k in [5, 3, 9] {
            q.activate(&mut h, k, true, false, true).unwrap();
        }
        h.ready.extend([5, 3, 9]);
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![5, 3, 9]);
        assert!(q.is_empty());
        q.pump(&mut h).unwrap();
        q.activate(&mut h, 1, true, false, true).unwrap();
        assert_eq!(q.records[0].serial, 0, "an empty pump resets the serial");
    }

    /// Behind the first unready entry only priority entries complete; the unready one is polled again.
    #[test]
    fn an_unready_entry_blocks_later_non_priority_ones() {
        let (mut q, mut h) = (ActivationQueue::default(), Host::default());
        for k in [1, 2, 3] {
            q.activate(&mut h, k, true, false, true).unwrap();
        }
        q.enqueue(4, true, false, true);
        h.state.insert(4, 1);
        h.ready.extend([1, 3, 4]);
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![1, 4], "2 is unready: 3 waits, priority 4 completes");
        h.ready.insert(2);
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![1, 4, 2, 3]);
    }

    /// A non-deferred activation completes at once when ready; a cancel deactivates and enqueues nothing;
    /// an entry not allowed to complete is polled but stays.
    #[test]
    fn immediate_cancel_and_not_allowed() {
        let (mut q, mut h) = (ActivationQueue::default(), Host::default());
        h.ready.insert(7);
        assert!(q.activate(&mut h, 7, false, false, true).unwrap());
        h.cancel.insert(8);
        assert!(!q.activate(&mut h, 8, true, false, true).unwrap());
        assert!(!q.contains(8) && h.deactivated == vec![8]);
        q.activate(&mut h, 9, true, true, false).unwrap();
        h.ready.insert(9);
        q.pump(&mut h).unwrap();
        assert!(q.contains(9) && !h.completed.contains(&9));
    }

    /// Past the deadline completions defer; the budget grows by 300 per deferring pass and resets after
    /// a pass that deferred nothing.
    #[test]
    fn budget_defers_and_grows_per_pass() {
        let (mut q, mut h) = (ActivationQueue::default(), Host { tick: 600, ..Host::default() });
        for k in 1..=4 {
            q.activate(&mut h, k, true, false, true).unwrap();
            h.ready.insert(k);
        }
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![1], "deadline 1000 after the first 600 µs step");
        assert_eq!(q.budget_us(), BUDGET_US + BUDGET_STEP_US);
        h.tick = 0;
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![1, 2, 3, 4]);
        assert_eq!(q.budget_us(), BUDGET_US);
    }

    /// Re-enqueueing a pending key moves it to the back and clears its ready bit.
    #[test]
    fn re_enqueue_moves_to_the_back() {
        let (mut q, mut h) = (ActivationQueue::default(), Host::default());
        for k in [1, 2] {
            q.activate(&mut h, k, true, false, true).unwrap();
        }
        q.enqueue(1, false, false, true);
        h.ready.extend([1, 2]);
        q.pump(&mut h).unwrap();
        assert_eq!(h.completed, vec![2, 1]);
    }

    /// The same head blocker counts frames: +1 while I/O is busy, else up to the next multiple of 64; a
    /// forced poll at 0x200, dropped and deactivated at 0x800.
    #[test]
    fn stall_breaker_counts_passes() {
        let (mut q, mut h) = (ActivationQueue::default(), Host::default());
        q.activate(&mut h, 1, true, false, true).unwrap();
        q.pump(&mut h).unwrap(); // first sight of the blocker: counter 0
        h.busy = true;
        q.pump(&mut h).unwrap();
        assert_eq!(q.stall, 1);
        h.busy = false;
        for _ in 0..8 {
            q.pump(&mut h).unwrap();
        }
        assert_eq!(q.stall, STALL_FORCED_POLL);
        assert_eq!(h.forced, vec![1]);
        for _ in 0..24 {
            q.pump(&mut h).unwrap();
        }
        assert_eq!(q.stall, STALL_DEACTIVATE);
        assert!(q.is_empty() && h.deactivated == vec![1]);
    }
}
