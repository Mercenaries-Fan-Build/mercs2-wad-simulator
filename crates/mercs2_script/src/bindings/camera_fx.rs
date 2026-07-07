//! `Camera` engine binding namespace — luaL_Reg table VA 0x00b9a7d8, 14 cfuncs.
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
pub const NAMESPACE: &str = "CameraFx";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Camera";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a7d8;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetYaw", corpus_calls: 5 },
    Required { name: "SetYaw", corpus_calls: 7 },
    Required { name: "GetPitch", corpus_calls: 0 },
    Required { name: "SetPitch", corpus_calls: 1 },
    Required { name: "StopBlending", corpus_calls: 2 },
    Required { name: "Shake", corpus_calls: 6 },
    Required { name: "SetFOV", corpus_calls: 0 },
    Required { name: "GetFOV", corpus_calls: 0 },
    Required { name: "SetPosition", corpus_calls: 5 },
    Required { name: "SetLookAt", corpus_calls: 6 },
    Required { name: "Hold", corpus_calls: 3 },
    Required { name: "Follow", corpus_calls: 0 },
    Required { name: "Blend", corpus_calls: 3 },
    Required { name: "SetShot", corpus_calls: 2 },
];

/// Cinematic-camera FX (shake, blend, hold, shot framing) plus a few pose getters. This table is
/// installed on the `Camera` global, shared with `camera.rs`, and `camera_fx` installs *after* it in
/// `install_all` — so we first wire our own bindings, then re-attach the sibling `camera.rs` entries
/// (SetNearFar/…) that would otherwise be clobbered by the global replace.
///
/// The reimpl camera is fixed-function, so the FX setters are faithful no-ops. `GetYaw` is the one
/// getter the gameplay Lua reads (e.g. `self.nHeading = Camera.GetYaw(cam)`); with no live camera
/// heading to report it returns a neutral 0° — a value the callers' arithmetic accepts. `GetPitch`/
/// `GetFOV` are not called by the corpus and stay no-ops.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    for name in [
        "SetYaw",
        "GetPitch",
        "SetPitch",
        "StopBlending",
        "Shake",
        "SetFOV",
        "GetFOV",
        "SetPosition",
        "SetLookAt",
        "Hold",
        "Follow",
        "Blend",
        "SetShot",
    ] {
        b.stub(name, lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;
    }
    // Read as a heading by gameplay Lua — neutral 0° default.
    b.real(
        "GetYaw",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0.0f64))?,
    )?;

    // Preserve the sibling `Camera` surface installed earlier by `camera.rs`; `install_global` below
    // replaces the global table, so copy those functions into ours first (no name overlap).
    if let Ok(existing) = lua.globals().get::<mlua::Table>(GLOBAL) {
        for pair in existing.pairs::<String, mlua::Function>() {
            let (k, f) = pair?;
            b.extra(&k, f)?;
        }
    }

    b.install_global(GLOBAL)
}
