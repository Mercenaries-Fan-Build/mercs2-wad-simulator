//! `Camera` engine binding namespace — luaL_Reg table VA 0x00b9a530, 7 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Camera")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Camera";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Camera";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a530;

pub const REQUIRED: &[Required] = &[
    Required { name: "SetNearFar", corpus_calls: 7 },
    Required { name: "RestoreNearFar", corpus_calls: 7 },
    Required { name: "SetFovParams", corpus_calls: 3 },
    Required { name: "RestoreFovParams", corpus_calls: 6 },
    Required { name: "SetFocusParams", corpus_calls: 6 },
    Required { name: "RestoreFocusParams", corpus_calls: 8 },
    Required { name: "SetLodParams", corpus_calls: 0 },
];

/// Camera near/far/FOV/focus/LOD *override* setters (and their Restore pairs) — presentation tuning
/// on the active camera. The reimpl camera is fixed-function, so honoring these runtime overrides is a
/// faithful no-op; none return a value the game's Lua reads. (This table shares the `Camera` global
/// with `camera_fx.rs`, which installs later and preserves these entries — see that file.)
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    // Camera near/far/FOV/focus/LOD param setters → recorded Camera commands the camera system applies.
    super::record_all(&mut b, lua, host, "Camera", &[
        "SetNearFar",
        "RestoreNearFar",
        "SetFovParams",
        "RestoreFovParams",
        "SetFocusParams",
        "RestoreFocusParams",
        "SetLodParams",
    ])?;
    b.install_global(GLOBAL)
}
