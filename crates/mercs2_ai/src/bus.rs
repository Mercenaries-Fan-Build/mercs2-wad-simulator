//! The AI action bus — the hash-addressed "do this action now" ring recovered from the exe.
//!
//! Code map §2.2: AI commands are a single **hash-addressed message bus** with a bounded ring. The
//! local-enqueue primitive `FUN_00423d10` is the home of the famous "Ai 1024" budget:
//!
//! ```c
//! EnterCriticalSection(&DAT_0124aef8);
//! if (DAT_012476a8 < 0x400) {                        // 1024-slot cap  ← "Ai 1024"
//!     *(&DAT_012476f0 + DAT_012476a8*0xc) = {guid,hash,0};  // 0xc-byte entry
//!     DAT_012476a8++;
//! }
//! ```
//!
//! `DirectAction(guid, actionHash)` (`FUN_0056aa70`) local-posts here, then replicates to clients if
//! hosting. The "Ai 1024" number is **this ring**, NOT a per-entity component pool (code map §2.2
//! correction). The planner *brain* that decides which action to post is data/Lua (§5) — the bus is
//! the mechanism the engine supplies.

use mercs2_formats::hash::pandemic_hash_m2;

/// The recovered ring capacity — `0x400` = 1024 slots (`DAT_012476a8 < 0x400`).
pub const RING_CAP: usize = 0x400;

/// One `{guid, hash, 0}` action-ring entry (the 0xc-byte record the enqueue primitive writes). The
/// trailing dword is always 0 in the recovered code (a reserved/flags slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiAction {
    /// Target entity GUID the action is addressed to.
    pub guid: u32,
    /// 32-bit action verb hash (e.g. `pandemic_hash_m2("attack")`).
    pub hash: u32,
}

/// The 1024-slot AI action ring — `DirectAction`'s local enqueue. Bounded exactly as the exe: once
/// full, further posts are dropped (the recovered `if (count < 0x400)` gate), not overwritten.
#[derive(Default)]
pub struct AiActionBus {
    ring: Vec<AiAction>,
    /// Count of posts refused because the ring was at capacity (the exe silently drops; we count so a
    /// caller can observe the budget being hit).
    dropped: u64,
}

impl AiActionBus {
    pub fn new() -> Self {
        AiActionBus::default()
    }

    /// `DirectAction(guid, actionHash)` local-post: enqueue `{guid, hash}` if the ring is below the
    /// 1024-slot cap, else drop it (faithful to `if (DAT_012476a8 < 0x400)`). Returns whether it was
    /// accepted. (MP replication to clients — `FUN_006bb960` — is the host-gated next stage; not here.)
    pub fn direct_action(&mut self, guid: u32, action_hash: u32) -> bool {
        if self.ring.len() < RING_CAP {
            self.ring.push(AiAction { guid, hash: action_hash });
            true
        } else {
            self.dropped += 1;
            false
        }
    }

    /// Number of queued actions.
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// Posts refused because the ring was full since construction.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Drain every queued action in FIFO order (the tick consumer empties the ring each frame).
    pub fn drain(&mut self) -> Vec<AiAction> {
        std::mem::take(&mut self.ring)
    }
}

/// Hash an `Ai.Goal` verb string to its 32-bit action hash the way the engine addresses actions
/// (`pandemic_hash_m2` of the lowercased verb — matching the planner verb vocabulary `moveto`,
/// `attack`, `takecover`, … in code map §5). Case-insensitive so Lua's `"Attack"` and the native
/// `"attack"` verb agree.
pub fn goal_action_hash(goal: &str) -> u32 {
    pandemic_hash_m2(&goal.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A posted action is queued and drains FIFO with the guid + verb hash intact.
    #[test]
    fn direct_action_enqueues_and_drains_fifo() {
        let mut bus = AiActionBus::new();
        assert!(bus.direct_action(1, goal_action_hash("attack")));
        assert!(bus.direct_action(2, goal_action_hash("moveto")));
        assert_eq!(bus.len(), 2);
        let drained = bus.drain();
        assert_eq!(drained[0], AiAction { guid: 1, hash: goal_action_hash("attack") });
        assert_eq!(drained[1].guid, 2);
        assert!(bus.is_empty(), "drain empties the ring");
    }

    /// The ring is bounded at exactly 1024 (the recovered "Ai 1024" cap): the 1025th post is dropped,
    /// not stored.
    #[test]
    fn ring_caps_at_1024() {
        let mut bus = AiActionBus::new();
        for i in 0..RING_CAP {
            assert!(bus.direct_action(i as u32, 0), "post {i} within cap must be accepted");
        }
        assert_eq!(bus.len(), RING_CAP);
        assert!(!bus.direct_action(9999, 0), "post over cap must be dropped");
        assert_eq!(bus.len(), RING_CAP, "ring must not grow past the cap");
        assert_eq!(bus.dropped(), 1);
    }

    /// Goal hashing is case-insensitive (Lua "Attack" == native "attack") and distinguishes verbs.
    #[test]
    fn goal_hash_is_case_insensitive_and_distinct() {
        assert_eq!(goal_action_hash("Attack"), goal_action_hash("attack"));
        assert_ne!(goal_action_hash("attack"), goal_action_hash("moveto"));
    }
}
