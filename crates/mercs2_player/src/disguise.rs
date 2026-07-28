//! Vehicle disguise — **two mechanisms wearing four similar names**
//! (`player_code_map.md` §7).
//!
//! This is the single easiest part of the `Player` surface to get wrong, because the four cfuncs read
//! like one feature and are not:
//!
//! | cfunc | VA | resolves | acts on |
//! |---|---|---|---|
//! | `SetVehicleDisguise(bEnable)` | `0x005E00B0` | **nothing** | the global byte `[0x01176106]` |
//! | `GetVehicleDisguise()` | `0x005E0130` | **nothing** | the global byte `[0x01176106]` |
//! | `VehicleDisguise({Player=…})` | `0x005E02A0` | `FUN_006CDB70` (**character**) | per-player `+0x430/+0x434/+0x438` |
//! | `GetVehicleDisguiseState({Player=…})` | `0x005E0470` | `FUN_006CDB70` (**character**) | per-player, based at `+0x3A8` |
//!
//! The first pair does **no lookup at all** — `0x005E0100 mov byte [0x1176106], dl` and
//! `0x005E0131 mov bl, byte [0x1176106]`. That byte is also read by `Object.IsDisguised`
//! (`FUN_005CEF20`), and the Lua guards on it: `if not Player.GetVehicleDisguise() then return end`
//! (`wiftutorialvehicledisguise.lua:26`). It is a **global feature gate for the whole disguise
//! system**, not a per-player setting. It lives on [`crate::PlayerWorld`], not here.
//!
//! ## Argument shape — an earlier revision got this wrong
//!
//! `VehicleDisguise` and `GetVehicleDisguiseState` take a **named table**, and its `Player = ` key holds
//! a **character** guid, not a player guid:
//!
//! ```lua
//! Player.VehicleDisguise({Player = uRider, Callback = DisguiseChangedCallback})  -- :18
//! local bState = Player.GetVehicleDisguiseState({Player = uRider})               -- :35
//! Player.VehicleDisguise({Player = uRider, Remove = true})                       -- :97
//! ```
//!
//! where `local uRider = Player.GetLocalCharacter()`. An earlier revision typed `VehicleDisguise` as
//! `Option<bool>` and drops the table entirely, which loses the callback *and* the handle.
//!
//! ## The state's type
//!
//! `GetVehicleDisguiseState` sums two sub-queries off `player+0x3A8`
//! (`FUN_006ABC30`/`FUN_006ABC50` → `FUN_004B86E0`/`FUN_004B29C0`) into what the map calls an integer.
//! Those sub-queries were **not read**, and the only shipped consumer `tostring()`s the result against
//! `"true"`/`"false"` (`wiftutorialvehicledisguise.lua:37,41`) — so a **boolean** is what actually
//! satisfies the contract. Pushing `0` (what an earlier revision did) stringifies to `"0"` and both
//! branches go dead.
// CONFIRM-LIVE: bp at 0x005E0527 to read the real return, which also closes FUN_006CDB70's M→H.

use crate::callbacks::{CallbackArg, CallbackRegistry, CallbackSlot};
use crate::object::PlayerObject;

/// The parsed `{Player = …, Callback = …, Remove = …}` table `Player.VehicleDisguise` takes.
///
/// `callback` is `true` when the table carried one — the `mlua::Function` itself is retained by the
/// binding layer and referenced through a [`crate::callbacks::CallbackId`], never held here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisguiseRequest {
    /// The `Player = ` key — ⚠ a **character** guid.
    pub character: u64,
    /// Whether `Remove = true` was set (the teardown call at `wiftutorialvehicledisguise.lua:97`).
    pub remove: bool,
}

/// Apply a [`DisguiseRequest`] to the player possessing `request.character`.
///
/// Returns whether a player was found — a miss must push nil and **not** raise, per the map's note that
/// `FUN_004B2A50` is `push nil; return 1` and shipped scripts rely on `if ... then`.
///
/// Fires [`CallbackSlot::DisguiseChanged`] on an actual state change, with the shipped contract
/// `(playerGuid, nDisguiseState, uFaction)`.
pub fn apply(
    roster: &mut crate::roster::PlayerRoster,
    request: DisguiseRequest,
    faction: u64,
    callbacks: &mut CallbackRegistry,
) -> bool {
    let Some(p) = roster.by_character_mut(request.character) else { return false };
    let before = p.disguise.flag_bit3;
    // `FUN_005E02A0` toggles bit 3 of `+0x438`: `^= (v << 3 ^ cur) & 8`.
    p.disguise.flag_bit3 = !request.remove;
    let (slot, guid, after) = (p.slot, p.guid, p.disguise.flag_bit3);

    if after != before {
        callbacks.fire(
            CallbackSlot::DisguiseChanged(slot),
            vec![CallbackArg::Guid(guid), CallbackArg::Bool(after), CallbackArg::Guid(faction)],
        );
    }
    true
}

/// `Player.GetVehicleDisguiseState({Player = uChar})` — the per-player disguise state.
///
/// Returns a **boolean**, not the integer the map's prose suggests; see the module docs.
pub fn state(p: &PlayerObject) -> bool {
    p.disguise.flag_bit3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::possession::{attach_to_character, CheatFlags};
    use crate::roster::PlayerRoster;

    fn possessed(character: u64) -> PlayerRoster {
        let mut r = PlayerRoster::single_player();
        attach_to_character(&mut r, 0, character, CheatFlags::default());
        r
    }

    /// The request resolves by **character**, not by player guid — the whole point of `FUN_006CDB70`.
    /// Passing the player's own guid must miss.
    #[test]
    fn the_request_resolves_through_a_character_handle() {
        let mut r = possessed(0xC0FFEE);
        let mut cbs = CallbackRegistry::default();
        let player_guid = r.get(0).unwrap().guid;

        assert!(
            apply(&mut r, DisguiseRequest { character: 0xC0FFEE, remove: false }, 0, &mut cbs),
            "a character handle resolves"
        );
        assert!(
            !apply(&mut r, DisguiseRequest { character: player_guid, remove: false }, 0, &mut cbs),
            "a PLAYER handle must not resolve — typing these as player handles fails silently"
        );
    }

    /// A miss is `false`, never a panic, so the binding can push nil and keep `if ... then` working.
    #[test]
    fn an_unresolvable_character_is_a_miss_not_a_panic() {
        let mut r = PlayerRoster::single_player();
        let mut cbs = CallbackRegistry::default();
        assert!(!apply(&mut r, DisguiseRequest { character: 0xBAD, remove: false }, 0, &mut cbs));
    }

    /// Apply then `Remove = true` round-trips, and the state reads back as a **boolean** — the shape
    /// `tostring(...) == "true"` at `wiftutorialvehicledisguise.lua:37` actually needs.
    #[test]
    fn disguise_round_trips_and_reads_back_as_a_boolean() {
        let mut r = possessed(0xC0FFEE);
        let mut cbs = CallbackRegistry::default();

        assert!(!state(r.get(0).unwrap()), "starts undisguised");
        apply(&mut r, DisguiseRequest { character: 0xC0FFEE, remove: false }, 0, &mut cbs);
        assert!(state(r.get(0).unwrap()), "the setter is observable by the getter");

        apply(&mut r, DisguiseRequest { character: 0xC0FFEE, remove: true }, 0, &mut cbs);
        assert!(!state(r.get(0).unwrap()), "Remove = true clears it");
    }

    /// The callback fires on a change, carrying the shipped three-argument contract, and does **not**
    /// fire when the state is unchanged.
    #[test]
    fn the_disguise_callback_carries_the_shipped_contract() {
        let mut r = possessed(0xC0FFEE);
        let mut cbs = CallbackRegistry::default();
        let id = cbs.mint();
        cbs.bind(CallbackSlot::DisguiseChanged(0), id);
        let player_guid = r.get(0).unwrap().guid;

        apply(&mut r, DisguiseRequest { character: 0xC0FFEE, remove: false }, 0x7, &mut cbs);
        let fires = cbs.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(
            fires[0].args,
            vec![CallbackArg::Guid(player_guid), CallbackArg::Bool(true), CallbackArg::Guid(0x7)],
            "DisguiseChangedCallback(playerGuid, nDisguiseState, uFaction)"
        );

        // Re-applying the same state is not a change.
        apply(&mut r, DisguiseRequest { character: 0xC0FFEE, remove: false }, 0x7, &mut cbs);
        assert!(cbs.take_fires().is_empty(), "no change, no callback");
    }
}
