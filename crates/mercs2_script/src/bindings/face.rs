//! `Face` engine binding namespace — luaL_Reg table VA 0x00b9a88c, 6 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Face")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Face";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Face";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a88c;

pub const REQUIRED: &[Required] = &[
    Required { name: "BindFaceAnimSet", corpus_calls: 0 },
    Required { name: "UnbindFaceAnimSet", corpus_calls: 0 },
    Required { name: "PlayFaceAnim", corpus_calls: 0 },
    Required { name: "PlayFacialExpression", corpus_calls: 0 },
    Required { name: "GetTranslationForStanceAndAction", corpus_calls: 0 },
    Required { name: "SetUseBriefingLOD", corpus_calls: 0 },
];

/// Facial animation driver. The engine doesn't drive face anim in the reimpl, so every cfunc is a
/// faithful no-op (`GetTranslationForStanceAndAction` returns nil — no translation). None of these are
/// called by the game Lua corpus.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    b.stub("BindFaceAnimSet", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("UnbindFaceAnimSet", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("PlayFaceAnim", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("PlayFacialExpression", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("GetTranslationForStanceAndAction", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.stub("SetUseBriefingLOD", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
