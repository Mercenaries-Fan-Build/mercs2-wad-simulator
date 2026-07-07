//! `VO` engine binding namespace — luaL_Reg table VA 0x00b988b0, 11 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("VO")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "VO";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "VO";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b988b0;

pub const REQUIRED: &[Required] = &[
    Required { name: "Cue", corpus_calls: 7 },
    Required { name: "CueWithoutSubtitles", corpus_calls: 5 },
    Required { name: "Cancel", corpus_calls: 11 },
    Required { name: "CancelAll", corpus_calls: 6 },
    Required { name: "Pause", corpus_calls: 0 },
    Required { name: "PauseAll", corpus_calls: 0 },
    Required { name: "Unpause", corpus_calls: 0 },
    Required { name: "UnpauseAll", corpus_calls: 0 },
    Required { name: "SetCinematicMode", corpus_calls: 8 },
    Required { name: "AddSequence", corpus_calls: 1 },
    Required { name: "RemoveSequence", corpus_calls: 1 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
