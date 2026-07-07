//! `Sys` engine binding namespace — luaL_Reg table VA 0x00b98a78, 64 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Sys")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Sys";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Sys";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98a78;

pub const REQUIRED: &[Required] = &[
    Required { name: "WriteToConsole", corpus_calls: 0 },
    Required { name: "ToStringL", corpus_calls: 0 },
    Required { name: "MemUsage", corpus_calls: 0 },
    Required { name: "StringToGuid", corpus_calls: 2 },
    Required { name: "GuidToString", corpus_calls: 25 },
    Required { name: "RequestGameState", corpus_calls: 48 },
    Required { name: "IsLoadingOrStreaming", corpus_calls: 1 },
    Required { name: "SetNumberOfViewports", corpus_calls: 2 },
    Required { name: "SetTimeScale", corpus_calls: 2 },
    Required { name: "LTIGetPrecacheBypass", corpus_calls: 1 },
    Required { name: "GetLevelName", corpus_calls: 11 },
    Required { name: "SetLevelName", corpus_calls: 1 },
    Required { name: "GetMasterScriptName", corpus_calls: 8 },
    Required { name: "SetMasterScriptName", corpus_calls: 1 },
    Required { name: "GetCharacterTemplate", corpus_calls: 2 },
    Required { name: "RequiredAsset", corpus_calls: 2 },
    Required { name: "SetAssetRequestMax", corpus_calls: 2 },
    Required { name: "GetAssetRequestMax", corpus_calls: 2 },
    Required { name: "Callback", corpus_calls: 0 },
    Required { name: "FinishedShell", corpus_calls: 1 },
    Required { name: "AutoLoad", corpus_calls: 4 },
    Required { name: "GetSkipMission", corpus_calls: 2 },
    Required { name: "GetINIBriefing", corpus_calls: 6 },
    Required { name: "SetSkipMission", corpus_calls: 10 },
    Required { name: "SetINIBriefing", corpus_calls: 4 },
    Required { name: "GetINILoadLastSave", corpus_calls: 0 },
    Required { name: "NoHud", corpus_calls: 2 },
    Required { name: "IsDemoMode", corpus_calls: 3 },
    Required { name: "DisableAssetPreload", corpus_calls: 0 },
    Required { name: "FlushAssets", corpus_calls: 0 },
    Required { name: "Clock", corpus_calls: 0 },
    Required { name: "Date", corpus_calls: 0 },
    Required { name: "Time", corpus_calls: 0 },
    Required { name: "DiffTime", corpus_calls: 0 },
    Required { name: "MainTime", corpus_calls: 1 },
    Required { name: "RealTime", corpus_calls: 0 },
    Required { name: "MainTimeStamp", corpus_calls: 6 },
    Required { name: "RealTimeStamp", corpus_calls: 1 },
    Required { name: "TimeStampMark", corpus_calls: 6 },
    Required { name: "TimeStampGetElapsed", corpus_calls: 7 },
    Required { name: "PlayIntroMovies", corpus_calls: 2 },
    Required { name: "StartWithResources", corpus_calls: 1 },
    Required { name: "SubtitlesEnabled", corpus_calls: 4 },
    Required { name: "RumbleEnabled", corpus_calls: 2 },
    Required { name: "TutorialsEnabled", corpus_calls: 8 },
    Required { name: "SetTutorialsEnabled", corpus_calls: 1 },
    Required { name: "YAxisInverted", corpus_calls: 2 },
    Required { name: "SetLuaSaveVersion", corpus_calls: 2 },
    Required { name: "AddStringDb", corpus_calls: 3 },
    Required { name: "ClearStringDb", corpus_calls: 1 },
    Required { name: "StartSingleplayer", corpus_calls: 2 },
    Required { name: "RequestAutosave", corpus_calls: 1 },
    Required { name: "IsFinalConfig", corpus_calls: 2 },
    Required { name: "IsConfirmOnCircle", corpus_calls: 7 },
    Required { name: "GetPlatform", corpus_calls: 6 },
    Required { name: "GetLanguage", corpus_calls: 2 },
    Required { name: "IsGermanSKU", corpus_calls: 3 },
    Required { name: "HaveActiveProfile", corpus_calls: 1 },
    Required { name: "IsAutosaveEnabled", corpus_calls: 0 },
    Required { name: "SetAutosaveEnabled", corpus_calls: 0 },
    Required { name: "ForceNextAutosave", corpus_calls: 1 },
    Required { name: "GetVersion", corpus_calls: 2 },
    Required { name: "GetShellCode", corpus_calls: 2 },
    Required { name: "GetForceNewGame", corpus_calls: 1 },
];

/// Boot slice: the level/master-script queries the bring-up path needs. `GetMasterScriptName`
/// currently returns the level name (same as `GetLevelName`) per the Phase-1 host. The other ~61
/// `Sys.*` cfuncs (console, asset/layer load, guid marshalling, save-version) are for later silos.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real(
        "GetLevelName",
        lua.create_function(move |_, ()| Ok(h.borrow().get_level_name()))?,
    )?;
    let h = host.clone();
    b.real(
        "GetMasterScriptName",
        lua.create_function(move |_, ()| Ok(h.borrow().get_level_name()))?,
    )?;
    let h = host.clone();
    b.real(
        "StartWithResources",
        lua.create_function(move |_, ()| Ok(h.borrow().start_with_resources()))?,
    )?;

    b.install_global(GLOBAL)
}
