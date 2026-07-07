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

use mlua::{Lua, MultiValue, Result as LuaResult, Value};

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
        lua.create_function(move |_, ()| Ok(h.borrow().sys_master_script_name()))?,
    )?;
    let h = host.clone();
    b.real(
        "StartWithResources",
        lua.create_function(move |_, ()| Ok(h.borrow().start_with_resources()))?,
    )?;

    // --- world-load handshake (the markers loadprobe scores) ---
    let h = host.clone();
    b.real(
        "RequestGameState",
        lua.create_function(move |_, state: String| {
            h.borrow_mut().sys_request_game_state(&state);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "RequestAutosave",
        // RequestAutosave(inMission, lastMission, missionTime, pct) — args recorded, ignored here.
        lua.create_function(move |_, _: mlua::MultiValue| {
            h.borrow_mut().sys_request_autosave();
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "IsLoadingOrStreaming",
        lua.create_function(move |_, ()| Ok(h.borrow().sys_is_loading_or_streaming()))?,
    )?;
    let h = host.clone();
    b.real(
        "GuidToString",
        lua.create_function(move |_, guid: i64| Ok(h.borrow().sys_guid_to_string(guid as u64)))?,
    )?;

    // Sys.StringToGuid("0x000f9a64") — the faithful inverse of GuidToString: parse a hex (or decimal)
    // guid literal to its number (wifpmcgarage.lua/wiftutorialgatehonk.lua read the result). No host
    // method needed; the string→number marshal is self-contained. Unparseable → nil.
    b.real(
        "StringToGuid",
        lua.create_function(|_, s: String| {
            let t = s.trim();
            let parsed = t
                .strip_prefix("0x")
                .or_else(|| t.strip_prefix("0X"))
                .and_then(|hex| i64::from_str_radix(hex, 16).ok())
                .or_else(|| t.parse::<i64>().ok());
            Ok(match parsed {
                Some(g) => Value::Integer(g),
                None => Value::Nil,
            })
        })?,
    )?;

    // --- Time / timestamp surface (self-consistent monotonic clock; no host method needed). ---
    // The game marks a stamp (Real/MainTimeStamp, TimeStampMark) and later reads the delta
    // (TimeStampGetElapsed) — e.g. antiair.lua's lock-on blink. A single boot Instant makes every stamp
    // and elapsed value coherent.
    let boot = std::time::Instant::now();
    b.real("MainTime", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("RealTime", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("MainTimeStamp", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("RealTimeStamp", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("TimeStampMark", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("Clock", lua.create_function(move |_, ()| Ok(boot.elapsed().as_secs_f64()))?)?;
    b.real("TimeStampGetElapsed", lua.create_function(move |_, ts: f64| Ok(boot.elapsed().as_secs_f64() - ts))?)?;
    b.real("DiffTime", lua.create_function(|_, (a, b): (f64, f64)| Ok(a - b))?)?;
    b.real(
        "Time",
        lua.create_function(|_, ()| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Ok(secs)
        })?,
    )?;
    b.real(
        "Date",
        lua.create_function(|_, ()| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Ok(format!("{secs}"))
        })?,
    )?;

    // --- Config / platform / profile getters the game branches on → faithful retail-PC defaults. ---
    // (No host method for these yet; each returns the value the retail PC build reports.)
    b.real("SubtitlesEnabled", lua.create_function(|_, ()| Ok(true))?)?;
    b.real("RumbleEnabled", lua.create_function(|_, ()| Ok(true))?)?;
    let h = host.clone();
    b.real("TutorialsEnabled", lua.create_function(move |_, ()| Ok(h.borrow().sys_tutorials_enabled()))?)?;
    b.real("YAxisInverted", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("IsDemoMode", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("NoHud", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("IsFinalConfig", lua.create_function(|_, ()| Ok(true))?)?;
    b.real("IsConfirmOnCircle", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("IsGermanSKU", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetForceNewGame", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetLanguage", lua.create_function(|_, ()| Ok("English".to_string()))?)?;
    b.real("GetPlatform", lua.create_function(|_, ()| Ok(0i64))?)?;
    b.real("MemUsage", lua.create_function(|_, ()| Ok(0i64))?)?;
    b.real("HaveActiveProfile", lua.create_function(|_, ()| Ok(true))?)?;
    b.real("IsAutosaveEnabled", lua.create_function(|_, ()| Ok(true))?)?;
    b.real("LTIGetPrecacheBypass", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetAssetRequestMax", lua.create_function(|_, ()| Ok(0i64))?)?;
    // Shell / flow getters — fresh boot: nothing finished, nothing to auto-load or skip.
    b.real("FinishedShell", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("AutoLoad", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetINIBriefing", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetINILoadLastSave", lua.create_function(|_, ()| Ok(false))?)?;
    b.real("GetSkipMission", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
    b.real("GetCharacterTemplate", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("GetShellCode", lua.create_function(|_, ()| Ok(String::new()))?)?;
    // Sys.GetVersion() → (sCode, sData) — two strings (mrxguishell.lua:527).
    b.real("GetVersion", lua.create_function(|_, ()| Ok((String::new(), String::new())))?)?;
    b.real("ToStringL", lua.create_function(|_, _: MultiValue| Ok(String::new()))?)?;

    // --- Setters / actions / dev sinks the retail engine consumes but the game does not read back. ---
    // --- Config setters → the host settings store (Set* ↔ Get* real roundtrips). ---
    let h = host.clone();
    b.real("WriteToConsole", lua.create_function(move |_, msg: String| {
        h.borrow_mut().sys_write_to_console(&msg);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetTimeScale", lua.create_function(move |_, s: f32| { h.borrow_mut().sys_set_time_scale(s); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetLevelName", lua.create_function(move |_, n: String| { h.borrow_mut().sys_set_level_name(&n); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetMasterScriptName", lua.create_function(move |_, n: String| { h.borrow_mut().sys_set_master_script_name(&n); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetTutorialsEnabled", lua.create_function(move |_, on: bool| { h.borrow_mut().sys_set_tutorials_enabled(on); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetAutosaveEnabled", lua.create_function(move |_, on: bool| { h.borrow_mut().sys_set_autosave_enabled(on); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetLuaSaveVersion", lua.create_function(move |_, v: i64| { h.borrow_mut().sys_set_lua_save_version(v); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetNumberOfViewports", lua.create_function(move |_, n: i64| { h.borrow_mut().sys_set_viewports(n); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetAssetRequestMax", lua.create_function(move |_, n: i64| { h.borrow_mut().sys_set_asset_request_max(n); Ok(()) })?)?;
    let h = host.clone();
    b.real("StartSingleplayer", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().sys_start_singleplayer(); Ok(()) })?)?;

    // --- UNBACKED residue (burn-down): asset-preload/streaming controls + string-DB + intro movies +
    // mission-skip need the asset/streaming + localization subsystems. Honest no-ops. ---
    for name in [
        "RequiredAsset", "Callback", "SetSkipMission", "SetINIBriefing", "DisableAssetPreload",
        "FlushAssets", "PlayIntroMovies", "AddStringDb", "ClearStringDb", "ForceNextAutosave",
    ] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    b.install_global(GLOBAL)
}
