//! `Sys` engine binding namespace — luaL_Reg table VA 0x00b98a78, 64 cfuncs.
//!
//! `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! To back this namespace: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Sys")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mercs2_luac::rt::{Lua, MultiValue, Result as LuaResult, Value};

use super::{Installed, NsBuilder, Required};
use crate::{Guid, SharedHost};

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
/// currently returns the level name (same as `GetLevelName`) as a stand-in. The other ~61
/// `Sys.*` cfuncs (console, asset/layer load, guid marshalling, save-version) are not yet backed.
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
        lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| {
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
        lua.create_function(move |_, guid: Guid| Ok(h.borrow().sys_guid_to_string(guid.raw())))?,
    )?;

    // Sys.StringToGuid("0x000f9a64") — the faithful inverse of GuidToString: parse a hex (or decimal)
    // guid literal to a **handle** (wifpmcgarage.lua:243 assigns it to `uVehicle`,
    // wiftutorialgatehonk.lua:10 to `uGateGuid`, and oilrig.lua:38 feeds the result straight into
    // `Camera.Shake`'s camera-handle slot). It therefore returns `Guid`, not a number: the results go
    // into handle slots and are compared against handles minted elsewhere, and only lightuserdata
    // makes those comparisons and the corpus's `type(u) == "userdata"` gates work.
    // No host method needed; the string→number marshal is self-contained. Unparseable → nil.
    b.real(
        "StringToGuid",
        lua.create_function(|_, s: String| {
            let t = s.trim();
            let parsed = t
                .strip_prefix("0x")
                .or_else(|| t.strip_prefix("0X"))
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                .or_else(|| t.parse::<u64>().ok());
            Ok(Guid(parsed.unwrap_or(0)))
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
    // `Option<f64>` in AND out, deliberately: the shipped binding tolerates a nil stamp, and the
    // shipped Lua proves it. `MrxPlayState.GetTotalTimeElapsed` (`mrxplaystate.lua:100-107`) does
    //
    //     local nThisSession = Sys.TimeStampGetElapsed(_uSessionStartTimestamp)
    //     if type(nPriorSessions) == "number" and type(nThisSession) == "number" then ...
    //     else return Sys.MainTime() end
    //
    // — a type test on the RESULT plus a fallback, which is only reachable if this function can
    // return a non-number. It is reached on every real boot: `xQ!L._StartPlayerVisibleGameplay`
    // calls `WifMissionFlow.LoadSingleton` at `:861`, whose `UnlockMission` → `_fPreContractSave`
    // → `GenerateSaveData` chain reads `GetTotalTimeElapsed` while `_uSessionStartTimestamp` is
    // still nil — `StartSessionTimer()` is eight lines later, at `:869`. Declaring `ts: f64` made
    // that a hard `error converting Lua nil to f64` and stranded the resume boot mid-transition.
    b.real(
        "TimeStampGetElapsed",
        lua.create_function(move |_, ts: Option<f64>| {
            Ok(ts.map(|t| boot.elapsed().as_secs_f64() - t))
        })?,
    )?;
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
    // `Sys.ToStringL(v)` — the engine's own `tostring`, and it **must actually stringify**.
    //
    // ⚠ This was a stub returning `""`, which was harmless until the recovered bootstrap glue
    // (`crate::BOOTSTRAP_GLUE`) started running `tostring = Sys.ToStringL`. That replaces Lua's global
    // `tostring` with this function, so *every* `tostring(x)` in the game returned an empty string.
    // Mostly that shows up as gutted log lines ("MrxState.Enter: state STATE_WAITFORGAME (refcount=)"),
    // but it is not merely cosmetic: `wiftutorialvehicledisguise.lua:37,41` **branches** on
    // `tostring(bState) == "true"`, so the disguise tutorial silently took the wrong arm.
    //
    // Delegating to Lua's own `tostring` is the honest implementation, and the glue's own first
    // statement — `_tostring = tostring` — is the evidence: it preserves the original precisely because
    // `Sys.ToStringL` is meant to *be* a tostring, not to replace it with nothing.
    // Resolve at CALL time, not install time: the glue runs after `install_all`, so `_tostring` does
    // not exist yet while this closure is being built.
    //
    // Order of preference matters and is not interchangeable. `_tostring` is the pristine Lua
    // `tostring` the glue stashed; prefer it. Only if the glue has *not* run is the global `tostring`
    // still Lua's own and safe to call — after the glue it IS this function, so reaching for it first
    // would recurse until the C stack overflows.
    b.real(
        "ToStringL",
        lua.create_function(|lua, v: Value| {
            let g = lua.globals();
            let f: mercs2_luac::rt::Function = match g.get::<Option<mercs2_luac::rt::Function>>("_tostring")? {
                Some(f) => f,
                None => g.get("tostring")?,
            };
            f.call::<String>(v)
        })?,
    )?;

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
    super::record_all(&mut b, lua, host, "Sys", &[
        "RequiredAsset", "Callback", "SetSkipMission", "SetINIBriefing", "DisableAssetPreload",
        "FlushAssets", "PlayIntroMovies", "AddStringDb", "ClearStringDb", "ForceNextAutosave",
    ])?;

    b.install_global(GLOBAL)
}
