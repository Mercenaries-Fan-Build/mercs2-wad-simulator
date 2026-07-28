//! PDA map mode and satellite scan — the callback-driven modal UI
//! (`player_code_map.md` §7, §2.2).
//!
//! Ten cfuncs spanning `0x005DB1E0`–`0x005DBDE0`, over two sub-objects: `player+0x1A8` (PDA map mode)
//! and `player+0x1AC` (satellite scan). The widget half belongs to `hud_widget_code_map.md`; what lives
//! here is the player-side state and the callback protocol.
//!
//! ## The arity an earlier revision dropped
//!
//! `SetPDAMapMode` is **nine arguments**, not one bool
//! (`mrxsupportdesignatorsatellite.lua:77`):
//!
//! ```lua
//! Player.SetPDAMapMode(self.uOwner, true, nX, nY + self.nStartZoom, nZ, self.nRadius,
//!                      self.nStartZoom - self.nMinZoom, self.nMaxZoom - self.nStartZoom,
//!                      MrxGuiSatellite.UseMinigame())
//! ...
//! Player.SetPDAMapMode(oDesignator.uOwner, false)   -- teardown: TWO arguments
//! ```
//!
//! So the binding must accept both the 9-arg engage and the 2-arg teardown. An earlier revision's
//! `Option<bool>` signature reads argument 1 — the owner handle — as its boolean, drops the other seven,
//! and (because `mlua`'s `bool` conversion is Lua-truthiness) sets the flag to `true` on the teardown
//! call as well.
//!
//! ## The callback protocol
//!
//! ```lua
//! Player.SetPDAMapModeCallback(self.uOwner, true, SatelliteTargettingEnd, {self})   -- :78
//! Player.SetPDAMapModeCancelCallback(self.uOwner, SatelliteTargettingCancel, {self}) -- :80
//! Player.RequestPDAMapModeExit(owner, fn, {args})                                    -- mrxguisatellite.lua:609
//! Player.RequestPDAMapModeCancel(uPlayer)                                            -- mrxutil.lua:974
//! ```
//!
//! Note `SetPDAMapModeCallback` carries a **context table** (`{self}`) alongside the function — retail's
//! `{fn, ctx}` pair. The context is retained by the binding layer with the function; this crate only
//! holds the [`crate::callbacks::CallbackId`].

use crate::callbacks::{CallbackArg, CallbackRegistry, CallbackSlot};
use crate::object::{PdaMapMode, PlayerObject};

/// The nine arguments of an engaging `Player.SetPDAMapMode` call, named from the shipped call site —
/// which is the only thing that pins them, since the sub-object's field roles are confidence **M**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdaMapModeRequest {
    /// Args 3–5: the map-view centre, world space. Note the shipped site passes `nY + nStartZoom` for
    /// the Y component, i.e. the camera height already folded in.
    pub centre: [f32; 3],
    /// Arg 6: view radius.
    pub radius: f32,
    /// Arg 7: `nStartZoom - nMinZoom`.
    pub zoom_below: f32,
    /// Arg 8: `nMaxZoom - nStartZoom`.
    pub zoom_above: f32,
    /// Arg 9: whether the targeting minigame is in play.
    pub minigame: bool,
}

/// `Player.SetPDAMapMode(owner, true, …)` — engage map mode with the full spatial payload.
pub fn engage_map_mode(p: &mut PlayerObject, req: PdaMapModeRequest) {
    p.pda_map = PdaMapMode {
        active: true,
        centre: req.centre,
        radius: req.radius,
        zoom_below: req.zoom_below,
        zoom_above: req.zoom_above,
        minigame: req.minigame,
    };
}

/// `Player.SetPDAMapMode(owner, false)` — the two-argument teardown.
///
/// Clears `active` but **retains** the spatial payload: retail's teardown writes the mode flag, and the
/// sub-object's other fields are not zeroed, so a script that disengages and re-engages without
/// respecifying keeps its view.
pub fn disengage_map_mode(p: &mut PlayerObject) {
    p.pda_map.active = false;
}

/// `Player.RequestPDAMapModeExit(owner [, fn, ctx])` — ask map mode to close normally, firing the exit
/// callback registered by `SetPDAMapModeCallback`.
///
/// Returns whether a callback was reachable.
pub fn request_exit(p: &mut PlayerObject, callbacks: &mut CallbackRegistry) -> bool {
    p.pda_map.active = false;
    callbacks.fire(CallbackSlot::PdaMapMode(p.slot), vec![CallbackArg::Guid(p.guid)])
}

/// `Player.RequestPDAMapModeCancel(uPlayer)` — cancel, firing the *cancel* callback instead
/// (`0x005DB658`).
pub fn request_cancel(p: &mut PlayerObject, callbacks: &mut CallbackRegistry) -> bool {
    p.pda_map.active = false;
    callbacks.fire(CallbackSlot::PdaMapModeCancel(p.slot), vec![CallbackArg::Guid(p.guid)])
}

/// `Player.SetupSatelliteScan(...)` / `SetSatelliteScanMode(on)` — engage or clear scan mode.
///
/// Clearing drops the accumulated targets: a new scan starts empty, otherwise the previous
/// designation's targets would leak into it.
pub fn set_satellite_mode(p: &mut PlayerObject, on: bool) {
    p.satellite.active = on;
    if !on {
        p.satellite.targets.clear();
        p.satellite.paused = false;
    }
}

/// `Player.AddSatelliteScanTarget(uTarget)`.
pub fn add_satellite_target(p: &mut PlayerObject, target: u64) {
    p.satellite.targets.push(target);
}

/// `Player.SetSatelliteScanPaused(on)`.
pub fn set_satellite_paused(p: &mut PlayerObject, paused: bool) {
    p.satellite.paused = paused;
}

/// `Player.ClearGPS()` → `FUN_006A0FB0`, which reads the PDA widget id at `+0x390` and clears the GPS
/// slot at `+0x398`. The widget id is *read*, not cleared — clearing it would orphan the player's PDA.
pub fn clear_gps(p: &mut PlayerObject) {
    let _widget = p.pda_widget; // 0x005BA613 reads +0x390 before clearing +0x398.
    p.gps_slot = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> PdaMapModeRequest {
        PdaMapModeRequest {
            centre: [10.0, 25.0, -5.0],
            radius: 120.0,
            zoom_below: 3.0,
            zoom_above: 7.0,
            minigame: true,
        }
    }

    /// **All nine arguments land**, not just the mode flag — the regression an earlier one-bool
    /// binding would show.
    #[test]
    fn the_nine_argument_engage_lands_every_field() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        engage_map_mode(&mut p, req());
        assert!(p.pda_map.active);
        assert_eq!(p.pda_map.centre, [10.0, 25.0, -5.0], "args 3-5");
        assert_eq!(p.pda_map.radius, 120.0, "arg 6");
        assert_eq!(p.pda_map.zoom_below, 3.0, "arg 7");
        assert_eq!(p.pda_map.zoom_above, 7.0, "arg 8");
        assert!(p.pda_map.minigame, "arg 9");
    }

    /// The two-argument teardown clears only the flag, and re-engaging is idempotent on the payload.
    #[test]
    fn the_two_argument_teardown_retains_the_payload() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        engage_map_mode(&mut p, req());
        disengage_map_mode(&mut p);
        assert!(!p.pda_map.active);
        assert_eq!(p.pda_map.radius, 120.0, "the spatial payload is not zeroed by the teardown");
    }

    /// Exit and cancel fire **different** callbacks — a script registers separate handlers for them
    /// (`SatelliteTargettingEnd` vs `SatelliteTargettingCancel`) and must not receive the wrong one.
    #[test]
    fn exit_and_cancel_fire_distinct_callbacks() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        let mut cbs = CallbackRegistry::default();
        let end = cbs.mint();
        let cancel = cbs.mint();
        cbs.bind(CallbackSlot::PdaMapMode(0), end);
        cbs.bind(CallbackSlot::PdaMapModeCancel(0), cancel);

        engage_map_mode(&mut p, req());
        assert!(request_exit(&mut p, &mut cbs));
        let fires = cbs.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].id, end, "exit fires the end callback");
        assert!(!p.pda_map.active);

        engage_map_mode(&mut p, req());
        assert!(request_cancel(&mut p, &mut cbs));
        let fires = cbs.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].id, cancel, "cancel fires the cancel callback");
    }

    /// With no callback registered, exit still disengages and simply reports that nothing was reachable.
    #[test]
    fn exit_without_a_registered_callback_still_disengages() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        let mut cbs = CallbackRegistry::default();
        engage_map_mode(&mut p, req());
        assert!(!request_exit(&mut p, &mut cbs), "nothing bound");
        assert!(!p.pda_map.active, "but the mode still closed");
    }

    /// Clearing scan mode drops accumulated targets, so a second designation does not inherit the
    /// first's.
    #[test]
    fn clearing_satellite_mode_drops_the_targets() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        set_satellite_mode(&mut p, true);
        add_satellite_target(&mut p, 0xA);
        add_satellite_target(&mut p, 0xB);
        set_satellite_paused(&mut p, true);
        assert_eq!(p.satellite.targets, vec![0xA, 0xB]);

        set_satellite_mode(&mut p, false);
        assert!(p.satellite.targets.is_empty(), "a new scan must start empty");
        assert!(!p.satellite.paused, "and unpaused");
    }

    /// `ClearGPS` clears the GPS slot and leaves the PDA widget id alone.
    #[test]
    fn clear_gps_spares_the_pda_widget() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        p.pda_widget = 0x1D;
        p.gps_slot = 0x99;
        clear_gps(&mut p);
        assert_eq!(p.gps_slot, 0);
        assert_eq!(p.pda_widget, 0x1D, "+0x390 is read, not cleared");
    }
}
