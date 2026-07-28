//! The player callback registry (`player_code_map.md` §7).
//!
//! **Retail stores a Lua *ref*, not a closure.** `SetBoundaryCallback` (`FUN_005DCD60`) writes
//! `{fn, ctx}` to `player+0x380` / `+0x384` (`0x005DCE44` / `0x005DCE4A`) — the engine holds a handle
//! into the Lua registry and a context value, never the function object itself. So this crate holds
//! opaque [`CallbackId`]s and the binding layer owns the `mlua::Function` table, which is both faithful
//! *and* what keeps `mercs2_player` free of an `mlua` dependency.
//!
//! **The bug this replaces.** All eight callback-registration verbs currently route through
//! `record_all`, whose `stringify_arg` maps `Value::Function` → `""`. The closure is destroyed at
//! registration and the resulting `("Player.X", [""])` tuple is pushed into a Vec nothing drains — so
//! none of these callbacks can ever fire. `Player.SetPDAMapModeCallback` alone has 7 shipped call sites.
//!
//! **`SetPlayerJoinedCallback` installs into three places.** `FUN_005DE860` writes the same handle to
//! `PTR_PTR_01176174+0x24`, `PTR_PTR_01175DB0+0x14` and `PTR_PTR_01176158+0x2C` — three subsystems each
//! keep their own copy. Modelled as a 3-element array so `RemovePlayerJoinedCallback` clearing all three
//! is visible rather than implicit.

/// The number of independent singletons `SetPlayerJoinedCallback` / `SetPlayerLeftCallback` install
/// into (`FUN_005DE860`).
pub const JOIN_LEAVE_SINK_COUNT: usize = 3;

/// A handle to a Lua callback the script layer retains, mirroring retail's Lua registry ref.
///
/// Opaque by design: this crate must not know what a `mlua::Function` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallbackId(pub u32);

/// Which player callback a [`CallbackId`] is registered against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackSlot {
    /// One of the three join sinks (`FUN_005DE860`).
    PlayerJoined(usize),
    /// One of the three leave sinks.
    PlayerLeft(usize),
    /// `player+0x380` / `+0x384` — the boundary `{fn, ctx}` pair, per player slot.
    Boundary(u8),
    /// `SetSurvivalModeCallback`, per player slot.
    SurvivalMode(u8),
    /// `SetPDAMapModeCallback`, per player slot.
    PdaMapMode(u8),
    /// `SetPDAMapModeCancelCallback`, per player slot.
    PdaMapModeCancel(u8),
    /// `SetSatelliteScanCallbacks`, per player slot.
    SatelliteScan(u8),
    /// `Player.VehicleDisguise`'s `Callback = ` key — per player slot, fired on every disguise change
    /// with `(playerGuid, nDisguiseState, uFaction)` (`wiftutorialvehicledisguise.lua:24`).
    DisguiseChanged(u8),
}

/// One argument in a queued callback invocation. Typed, unlike the `stringify_arg` path this replaces —
/// which is the whole point: a GUID that arrives as `"18446744073709551615"` is not a GUID.
#[derive(Clone, Debug, PartialEq)]
pub enum CallbackArg {
    Guid(u64),
    Number(f64),
    Bool(bool),
    Text(String),
    Nil,
}

/// One pending invocation, drained by the script layer each tick and dispatched onto the retained
/// `mlua::Function` together with the context arguments stored at registration.
#[derive(Clone, Debug, PartialEq)]
pub struct CallbackFire {
    pub id: CallbackId,
    pub args: Vec<CallbackArg>,
}

/// The registry: which id is bound to which slot, and the queue of invocations awaiting dispatch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallbackRegistry {
    /// Monotonic id source. Never reused, so a stale id from a removed callback can never be
    /// mistaken for a live one.
    next: u32,
    bound: Vec<(CallbackSlot, CallbackId)>,
    pending: Vec<CallbackFire>,
}

impl CallbackRegistry {
    /// Mint a fresh id for the script layer to associate with a retained `Function`.
    pub fn mint(&mut self) -> CallbackId {
        self.next += 1;
        CallbackId(self.next)
    }

    /// Register `id` against `slot`, replacing any previous binding there.
    ///
    /// Retail's setters overwrite — `SetBoundaryCallback` writes the pair unconditionally — so a second
    /// registration on the same slot must not leave two live callbacks.
    pub fn bind(&mut self, slot: CallbackSlot, id: CallbackId) {
        self.bound.retain(|(s, _)| *s != slot);
        self.bound.push((slot, id));
    }

    /// Clear whatever is bound to `slot`. Returns the id that was there, if any.
    pub fn unbind(&mut self, slot: CallbackSlot) -> Option<CallbackId> {
        let idx = self.bound.iter().position(|(s, _)| *s == slot)?;
        Some(self.bound.remove(idx).1)
    }

    /// Clear all three sinks of a join/leave callback at once —
    /// `RemovePlayerJoinedCallback` / `RemovePlayerLeftCallback`.
    pub fn unbind_all_sinks(&mut self, joined: bool) {
        for i in 0..JOIN_LEAVE_SINK_COUNT {
            let slot =
                if joined { CallbackSlot::PlayerJoined(i) } else { CallbackSlot::PlayerLeft(i) };
            self.unbind(slot);
        }
    }

    /// The id bound to `slot`, if any.
    pub fn bound_to(&self, slot: CallbackSlot) -> Option<CallbackId> {
        self.bound.iter().find(|(s, _)| *s == slot).map(|(_, id)| *id)
    }

    /// Queue an invocation of whatever is bound to `slot`. A no-op when nothing is bound — which is
    /// how retail behaves and why an unregistered callback must not be an error.
    ///
    /// Returns whether anything was queued, so callers can assert a callback was actually reachable.
    pub fn fire(&mut self, slot: CallbackSlot, args: Vec<CallbackArg>) -> bool {
        match self.bound_to(slot) {
            Some(id) => {
                self.pending.push(CallbackFire { id, args });
                true
            }
            None => false,
        }
    }

    /// Drain the queued invocations for the script layer to dispatch.
    pub fn take_fires(&mut self) -> Vec<CallbackFire> {
        std::mem::take(&mut self.pending)
    }

    /// How many callbacks are currently bound — for tests and diagnostics.
    pub fn bound_count(&self) -> usize {
        self.bound.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids are never reused, so an id captured before a rebind cannot be confused with the new one.
    #[test]
    fn ids_are_unique_and_never_reused() {
        let mut r = CallbackRegistry::default();
        let a = r.mint();
        let b = r.mint();
        assert_ne!(a, b);
        r.bind(CallbackSlot::Boundary(0), a);
        r.unbind(CallbackSlot::Boundary(0));
        assert_ne!(r.mint(), a, "a freed slot must not hand back the old id");
    }

    /// Re-registering a slot replaces, matching retail's unconditional pair write — otherwise a script
    /// that re-registers on mission restart would fire two callbacks per event.
    #[test]
    fn rebinding_a_slot_replaces_rather_than_accumulates() {
        let mut r = CallbackRegistry::default();
        let first = r.mint();
        let second = r.mint();
        r.bind(CallbackSlot::PdaMapMode(0), first);
        r.bind(CallbackSlot::PdaMapMode(0), second);
        assert_eq!(r.bound_count(), 1);
        assert_eq!(r.bound_to(CallbackSlot::PdaMapMode(0)), Some(second));

        r.fire(CallbackSlot::PdaMapMode(0), vec![]);
        assert_eq!(r.take_fires().len(), 1, "exactly one dispatch, not two");
    }

    /// The join/leave callbacks occupy three sinks and `Remove*` clears all three.
    #[test]
    fn join_callbacks_occupy_three_sinks_and_clear_together() {
        let mut r = CallbackRegistry::default();
        for i in 0..JOIN_LEAVE_SINK_COUNT {
            let id = r.mint();
            r.bind(CallbackSlot::PlayerJoined(i), id);
        }
        assert_eq!(r.bound_count(), 3, "FUN_005DE860 installs the handle in three singletons");

        r.unbind_all_sinks(true);
        assert_eq!(r.bound_count(), 0);
    }

    /// Firing an unbound slot is a no-op that reports it, not an error — but firing a *bound* one
    /// queues typed args. This is the property the `stringify_arg` path could not have.
    #[test]
    fn firing_carries_typed_args_and_reports_reachability() {
        let mut r = CallbackRegistry::default();
        assert!(!r.fire(CallbackSlot::DisguiseChanged(0), vec![]), "nothing bound -> not reachable");
        assert!(r.take_fires().is_empty());

        let id = r.mint();
        r.bind(CallbackSlot::DisguiseChanged(0), id);
        // The shipped disguise callback contract: (playerGuid, nDisguiseState, uFaction).
        assert!(r.fire(
            CallbackSlot::DisguiseChanged(0),
            vec![CallbackArg::Guid(0x2), CallbackArg::Bool(true), CallbackArg::Guid(0x7)]
        ));
        let fires = r.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].id, id);
        assert_eq!(
            fires[0].args,
            vec![CallbackArg::Guid(0x2), CallbackArg::Bool(true), CallbackArg::Guid(0x7)],
            "a GUID stays a GUID — it is not stringified"
        );
        assert!(r.take_fires().is_empty(), "draining empties the queue");
    }

    /// Per-slot callbacks on different players are distinct bindings.
    #[test]
    fn per_player_slots_are_independent() {
        let mut r = CallbackRegistry::default();
        let a = r.mint();
        let b = r.mint();
        r.bind(CallbackSlot::Boundary(0), a);
        r.bind(CallbackSlot::Boundary(1), b);
        assert_eq!(r.bound_count(), 2);
        r.fire(CallbackSlot::Boundary(1), vec![]);
        let fires = r.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].id, b, "player 1's boundary callback, not player 0's");
    }
}
