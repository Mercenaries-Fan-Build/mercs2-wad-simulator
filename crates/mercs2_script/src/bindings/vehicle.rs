//! `Vehicle` engine binding namespace — luaL_Reg table VA 0x00b98918, 40 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Vehicle")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Vehicle";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Vehicle";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98918;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetRiders", corpus_calls: 22 },
    Required { name: "GetDriver", corpus_calls: 122 },
    Required { name: "GetFromSeat", corpus_calls: 0 },
    Required { name: "GetFromRider", corpus_calls: 30 },
    Required { name: "GetSeatFromRider", corpus_calls: 7 },
    Required { name: "GetRiderFromSeat", corpus_calls: 0 },
    Required { name: "GetSeatByType", corpus_calls: 7 },
    Required { name: "GetSeatParams", corpus_calls: 1 },
    Required { name: "SetCanPlayerUse", corpus_calls: 2 },
    Required { name: "GetSeatToSeat", corpus_calls: 2 },
    Required { name: "IsSeatBlocked", corpus_calls: 2 },
    Required { name: "IsSeatALadder", corpus_calls: 2 },
    Required { name: "TransferToSeat", corpus_calls: 2 },
    Required { name: "Enter", corpus_calls: 22 },
    Required { name: "EnterBySeatGuid", corpus_calls: 4 },
    Required { name: "Exit", corpus_calls: 27 },
    Required { name: "HijackComplete", corpus_calls: 1 },
    Required { name: "HijackAbort", corpus_calls: 1 },
    Required { name: "HijackAbortDone", corpus_calls: 2 },
    Required { name: "EnableTurret", corpus_calls: 6 },
    Required { name: "SetTurretPitch", corpus_calls: 1 },
    Required { name: "SetTurretYaw", corpus_calls: 0 },
    Required { name: "OpenDoor", corpus_calls: 2 },
    Required { name: "CloseDoor", corpus_calls: 3 },
    Required { name: "IsFlying", corpus_calls: 4 },
    Required { name: "IsFlipped", corpus_calls: 5 },
    Required { name: "SpinHeli", corpus_calls: 0 },
    Required { name: "StartTankHijackMotion", corpus_calls: 0 },
    Required { name: "StopTankHijackMotion", corpus_calls: 2 },
    Required { name: "IsHijackRemote", corpus_calls: 1 },
    Required { name: "HijackStart", corpus_calls: 1 },
    Required { name: "SetHijackState", corpus_calls: 1 },
    Required { name: "SetHijackSuccess", corpus_calls: 1 },
    Required { name: "CancelHijack", corpus_calls: 2 },
    Required { name: "Usable", corpus_calls: 13 },
    Required { name: "IsHijackBad", corpus_calls: 0 },
    Required { name: "RestoreHealth", corpus_calls: 0 },
    Required { name: "RestoreAmmo", corpus_calls: 1 },
    Required { name: "SetParts", corpus_calls: 27 },
    Required { name: "ClearControls", corpus_calls: 1 },
];

/// Boot slice: only `Vehicle.EnableTurret` is wired, as a no-op — the one `Vehicle.*` cfunc the
/// vehicle branch of `_SpawnActorComplete` touches. The other 39 (seats, hijack FSM, doors, turret
/// aim, flight state) are for later silos.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let _ = host;
    let mut b = NsBuilder::new(lua)?;
    b.stub("EnableTurret", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.install_global(GLOBAL)
}
