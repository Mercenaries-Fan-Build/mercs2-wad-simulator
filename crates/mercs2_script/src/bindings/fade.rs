//! `Fade` engine binding namespace — luaL_Reg table VA 0x00b9a778, 4 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Fade")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Fade";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Fade";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a778;

pub const REQUIRED: &[Required] = &[
    Required { name: "AmbientTop", corpus_calls: 0 },
    Required { name: "AmbientSides", corpus_calls: 0 },
    Required { name: "Terrain", corpus_calls: 0 },
    Required { name: "CameraFade", corpus_calls: 0 },
];

/// Screen/terrain fade cfuncs — presentation only. The reimpl's fixed-function renderer has no fade
/// compositor, so each is a faithful no-op (the fade is treated as instantly complete). None of these
/// return a value the game's Lua reads, so all are stubs.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Fade colors → the real `mercs2_core::FadeState` (the compositor lerps toward them). Each takes
    // `(r, g, b, a [, time])`; the target color is stored (the fade timing is a render-pass concern).
    macro_rules! fset {
        ($name:literal, |$fade:ident, $c:ident| $body:block) => {{
            let h = host.clone();
            b.real($name, lua.create_function(move |_, (r, g, bl, a, _t): (f32, f32, f32, Option<f32>, Option<f32>)| {
                let $c = [r, g, bl, a.unwrap_or(1.0)];
                if let Some(rs) = h.borrow_mut().render_state() { let $fade = &mut rs.fade; $body }
                Ok(())
            })?)?;
        }};
    }
    fset!("AmbientTop", |f, c| { f.ambient_top = c; });
    fset!("AmbientSides", |f, c| { f.ambient_sides = c; });
    fset!("Terrain", |f, c| { f.terrain = c; });
    fset!("CameraFade", |f, c| { f.camera_fade = c; });

    b.install_global(GLOBAL)
}
