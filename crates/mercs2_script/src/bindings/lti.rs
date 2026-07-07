//! `Lti` engine binding namespace — luaL_Reg table VA 0x00b99c78, 52 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Lti")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Lti";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Lti";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99c78;

pub const REQUIRED: &[Required] = &[
    Required { name: "LTIMovieStart", corpus_calls: 0 },
    Required { name: "LTIMovieStop", corpus_calls: 0 },
    Required { name: "LTIMoviePause", corpus_calls: 0 },
    Required { name: "LTIMovieResume", corpus_calls: 0 },
    Required { name: "LTIVideoEnter", corpus_calls: 0 },
    Required { name: "LTIVideoSwitchMode", corpus_calls: 0 },
    Required { name: "LTIVideoNextRes", corpus_calls: 0 },
    Required { name: "LTIVideoPrevRes", corpus_calls: 0 },
    Required { name: "LTIVideoNextRefresh", corpus_calls: 0 },
    Required { name: "LTIVideoPrevRefresh", corpus_calls: 0 },
    Required { name: "LTIVideoSetGamma", corpus_calls: 0 },
    Required { name: "LTIVideoGetViewDistance", corpus_calls: 0 },
    Required { name: "LTIVideoApplyChanges", corpus_calls: 0 },
    Required { name: "LTIVideoDefault", corpus_calls: 0 },
    Required { name: "LTIVideoCancel", corpus_calls: 0 },
    Required { name: "LTIVideoAdvanceEnter", corpus_calls: 0 },
    Required { name: "LTIVideoSwitchOpt1", corpus_calls: 0 },
    Required { name: "LTIVideoAdvanceDefault", corpus_calls: 0 },
    Required { name: "LTIInputGeneralEnter", corpus_calls: 0 },
    Required { name: "LTIInputGeneralOptions", corpus_calls: 0 },
    Required { name: "LTIInputGeneralInvertMouse", corpus_calls: 0 },
    Required { name: "LTIInputGeneralMouseSense", corpus_calls: 0 },
    Required { name: "LTIInputGeneralJoySense", corpus_calls: 0 },
    Required { name: "LTIInputGeneralRumble", corpus_calls: 0 },
    Required { name: "LTIInputKMEnter", corpus_calls: 0 },
    Required { name: "LTIInputKMChangeInput", corpus_calls: 0 },
    Required { name: "LTIInputKMApplyChanges", corpus_calls: 0 },
    Required { name: "LTIInputKMDefault", corpus_calls: 0 },
    Required { name: "LTIOverBoundResponse", corpus_calls: 0 },
    Required { name: "LTIInputKMCancelInput", corpus_calls: 0 },
    Required { name: "LTIInputKMExit", corpus_calls: 0 },
    Required { name: "LTIInputJoystickEnter", corpus_calls: 0 },
    Required { name: "LTIInputJoystickChangePrimary", corpus_calls: 0 },
    Required { name: "LTIInputJoystickChangeInput", corpus_calls: 0 },
    Required { name: "LTIInputJoystickCancel", corpus_calls: 0 },
    Required { name: "LTIInputJoystickApplyChanges", corpus_calls: 0 },
    Required { name: "LTIInputJoystickDefault", corpus_calls: 0 },
    Required { name: "LTIInputJoystickExit", corpus_calls: 0 },
    Required { name: "LTIInputJoystickReEnter", corpus_calls: 0 },
    Required { name: "LTIJoystickOverBoundResponse", corpus_calls: 0 },
    Required { name: "LTIGetStartButton", corpus_calls: 0 },
    Required { name: "ChangeShellState", corpus_calls: 0 },
    Required { name: "LTIProfileEnter", corpus_calls: 0 },
    Required { name: "LTIProfileExit", corpus_calls: 0 },
    Required { name: "LTIPauseItemChanged", corpus_calls: 0 },
    Required { name: "LTIPrecacheDone", corpus_calls: 0 },
    Required { name: "LTIPrecacheSmokeDone", corpus_calls: 0 },
    Required { name: "LTIChoseOnline", corpus_calls: 0 },
    Required { name: "LTIGetDateFormat", corpus_calls: 0 },
    Required { name: "LTICamera", corpus_calls: 0 },
    Required { name: "LTIupdateSupportQuickSlot", corpus_calls: 0 },
    Required { name: "FirstRun", corpus_calls: 0 },
];

/// LTI = the shell/front-end "library" the menu Flash calls back into: video/input/profile options,
/// precache handshakes, shell-state changes. These are UI-driver callbacks; the fixed-function shell
/// does not yet apply the option changes, and every observed call site discards the return, so the
/// whole surface is a faithful no-op.
///
/// The one exception is `FirstRun`, whose return the shell reads: `if FirstRun() == 1 then` (the
/// first-boot options prompt). Faithful default is a non-first-run session → `0`, so normal boot
/// proceeds.
///
/// The game reaches this surface exclusively through the `LTILibName` alias (`LTILibName.FirstRun()`,
/// `LTILibName.ChangeShellState(..)`, …) — never a bare `Lti.*`. The table installs as the `Lti`
/// global and is also bound to the `LTILibName` global below so those shell lookups resolve.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    for name in [
        "LTIMovieStart",
        "LTIMovieStop",
        "LTIMoviePause",
        "LTIMovieResume",
        "LTIVideoEnter",
        "LTIVideoSwitchMode",
        "LTIVideoNextRes",
        "LTIVideoPrevRes",
        "LTIVideoNextRefresh",
        "LTIVideoPrevRefresh",
        "LTIVideoSetGamma",
        "LTIVideoGetViewDistance",
        "LTIVideoApplyChanges",
        "LTIVideoDefault",
        "LTIVideoCancel",
        "LTIVideoAdvanceEnter",
        "LTIVideoSwitchOpt1",
        "LTIVideoAdvanceDefault",
        "LTIInputGeneralEnter",
        "LTIInputGeneralOptions",
        "LTIInputGeneralInvertMouse",
        "LTIInputGeneralMouseSense",
        "LTIInputGeneralJoySense",
        "LTIInputGeneralRumble",
        "LTIInputKMEnter",
        "LTIInputKMChangeInput",
        "LTIInputKMApplyChanges",
        "LTIInputKMDefault",
        "LTIOverBoundResponse",
        "LTIInputKMCancelInput",
        "LTIInputKMExit",
        "LTIInputJoystickEnter",
        "LTIInputJoystickChangePrimary",
        "LTIInputJoystickChangeInput",
        "LTIInputJoystickCancel",
        "LTIInputJoystickApplyChanges",
        "LTIInputJoystickDefault",
        "LTIInputJoystickExit",
        "LTIInputJoystickReEnter",
        "LTIJoystickOverBoundResponse",
        "LTIGetStartButton",
        "ChangeShellState",
        "LTIProfileEnter",
        "LTIProfileExit",
        "LTIPauseItemChanged",
        "LTIPrecacheDone",
        "LTIPrecacheSmokeDone",
        "LTIChoseOnline",
        "LTIGetDateFormat",
        "LTICamera",
        "LTIupdateSupportQuickSlot",
    ] {
        b.stub(name, lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;
    }
    // Shell first-boot gate: `if FirstRun() == 1` — report a normal (non-first) session.
    b.real(
        "FirstRun",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;

    let installed = b.install_global(GLOBAL)?;

    // The shell Flash callbacks address this surface as `LTILibName.*`; bind that alias to the table.
    if let Ok(lti) = lua.globals().get::<mlua::Table>(GLOBAL) {
        let _ = lua.globals().set("LTILibName", lti);
    }

    Ok(installed)
}
