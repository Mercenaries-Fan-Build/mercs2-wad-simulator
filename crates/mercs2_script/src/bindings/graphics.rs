//! `Graphics` engine binding namespace — luaL_Reg table VA 0x00b9a4d0, 11 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Graphics")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Graphics";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Graphics";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a4d0;

pub const REQUIRED: &[Required] = &[
    Required { name: "ScreenShot", corpus_calls: 0 },
    Required { name: "SetNumFrameSync", corpus_calls: 0 },
    Required { name: "SetScreenRatio", corpus_calls: 0 },
    Required { name: "GetScreenRatio", corpus_calls: 0 },
    Required { name: "ReloadShaders", corpus_calls: 0 },
    Required { name: "SetGamma", corpus_calls: 2 },
    Required { name: "SetShadowBaseDistance", corpus_calls: 8 },
    Required { name: "GetShadowBaseDistance", corpus_calls: 4 },
    Required { name: "InitTinyGeometry", corpus_calls: 1 },
    Required { name: "ShowTinyGeometryObject", corpus_calls: 0 },
    Required { name: "SetBoundaryEffect", corpus_calls: 3 },
];

/// Graphics settings/quality cfuncs — presentation only. Screenshot, frame-sync, gamma, shader
/// reload, tiny-geometry and boundary-effect toggles are faithful no-ops on the fixed-function
/// renderer, and none of those return values the game reads.
///
/// `GetShadowBaseDistance` is the one getter the game reads: briefings do
/// `_nBaseShadowDistance = Graphics.GetShadowBaseDistance()`, temporarily lower it, then restore the
/// saved value. It doesn't gate control flow, but the return is consumed, so it's real and reports a
/// stable neutral base distance the save/restore round-trips cleanly. `GetScreenRatio` is never
/// called by the corpus and stays a no-op.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    for name in [
        "ScreenShot",
        "SetNumFrameSync",
        "SetScreenRatio",
        "GetScreenRatio",
        "ReloadShaders",
        "SetGamma",
        "SetShadowBaseDistance",
        "InitTinyGeometry",
        "ShowTinyGeometryObject",
        "SetBoundaryEffect",
    ] {
        b.stub(name, lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;
    }
    // Saved-and-restored by briefing scripts; report a stable neutral base distance.
    b.real(
        "GetShadowBaseDistance",
        lua.create_function(|_, _: mlua::MultiValue| Ok(100.0f64))?,
    )?;
    b.install_global(GLOBAL)
}
