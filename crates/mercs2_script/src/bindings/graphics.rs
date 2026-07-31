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

use mercs2_luac::rt::{Lua, Result as LuaResult};

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
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Graphics settings → the real `mercs2_core::GraphicsState` (Set*↔Get* round-trip).
    let h = host.clone();
    b.real("SetGamma", lua.create_function(move |_, v: f32| { if let Some(rs) = h.borrow_mut().render_state() { rs.graphics.gamma = v; } Ok(()) })?)?;
    let h = host.clone();
    b.real("SetShadowBaseDistance", lua.create_function(move |_, v: f32| { if let Some(rs) = h.borrow_mut().render_state() { rs.graphics.shadow_base_distance = v; } Ok(()) })?)?;
    let h = host.clone();
    b.real("GetShadowBaseDistance", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| {
        Ok(h.borrow().render_state_ref().map(|rs| rs.graphics.shadow_base_distance).unwrap_or(0.0))
    })?)?;
    let h = host.clone();
    b.real("SetScreenRatio", lua.create_function(move |_, v: f32| { if let Some(rs) = h.borrow_mut().render_state() { rs.graphics.screen_ratio = v; } Ok(()) })?)?;
    let h = host.clone();
    b.real("GetScreenRatio", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| {
        Ok(h.borrow().render_state_ref().map(|rs| rs.graphics.screen_ratio).unwrap_or(16.0 / 9.0))
    })?)?;
    let h = host.clone();
    b.real("SetBoundaryEffect", lua.create_function(move |_, v: f32| { if let Some(rs) = h.borrow_mut().render_state() { rs.graphics.boundary_effect = v; } Ok(()) })?)?;

    // Screenshot capture, frame-sync, shader reload, tiny-geometry debug viz → recorded Graphics
    // commands the render device drains.
    super::record_all(&mut b, lua, host, "Graphics", &[
        "ScreenShot", "SetNumFrameSync", "ReloadShaders", "InitTinyGeometry", "ShowTinyGeometryObject",
    ])?;

    // `Graphics.Camera` — a NESTED table, and a SEPARATE luaL_Reg from this one, so it is installed
    // via `value` (not coverage-tracked here; it needs its own namespace module once its table VA is
    // identified). The six members below are every one the corpus calls:
    //
    //     Graphics.Camera.SetNearFar / RestoreNearFar          (3 / 3 call sites)
    //     Graphics.Camera.SetFovParams / RestoreFovParams      (2 / 3)
    //     Graphics.Camera.SetFocusParams / RestoreFocusParams  (5 / 5)
    //
    // Each is a Set/Restore pair for a camera parameter the fixed-function renderer does not yet
    // model (near-far planes, FOV, depth-of-field focus), and none returns a value the game reads —
    // so they record like the rest of the presentation surface.
    //
    // The absence of this table was a hard stop, not a cosmetic gap: `WifPmcInterior._CompleteOnEnter`
    // ends with `Graphics.Camera.SetNearFar(0, 0.3, 500, 0)` (`vz/wifpmcinterior.lua:423`), so every
    // RESUME boot — which enters the PMC HQ interior — died there on "attempt to index a nil value
    // (field 'Camera')".
    let camera = lua.create_table()?;
    for name in [
        "SetNearFar", "RestoreNearFar",
        "SetFovParams", "RestoreFovParams",
        "SetFocusParams", "RestoreFocusParams",
    ] {
        let h = host.clone();
        let verb: std::rc::Rc<str> = std::rc::Rc::from(format!("Graphics.Camera.{name}").as_str());
        camera.set(
            name,
            lua.create_function(move |_, args: mercs2_luac::rt::MultiValue| {
                let sa: Vec<String> = args.iter().map(super::stringify_arg).collect();
                h.borrow_mut().script_cmd(&verb, sa);
                Ok(())
            })?,
        )?;
    }
    b.value("Camera", camera)?;

    b.install_global(GLOBAL)
}
