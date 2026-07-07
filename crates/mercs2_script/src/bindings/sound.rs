//! `Sound` engine binding namespace — luaL_Reg table VA 0x00b98c98, 88 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Sound")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Sound";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Sound";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98c98;

pub const REQUIRED: &[Required] = &[
    Required { name: "TestCueSound", corpus_calls: 0 },
    Required { name: "TestStopSound", corpus_calls: 0 },
    Required { name: "TestPauseSound", corpus_calls: 0 },
    Required { name: "CueSound", corpus_calls: 118 },
    Required { name: "StopSound", corpus_calls: 29 },
    Required { name: "PauseSound", corpus_calls: 0 },
    Required { name: "SetCategoryVolume", corpus_calls: 0 },
    Required { name: "SetCategoryPitch", corpus_calls: 0 },
    Required { name: "LockListenerPosition", corpus_calls: 8 },
    Required { name: "BindMusicCue", corpus_calls: 4 },
    Required { name: "ClearMusicCues", corpus_calls: 2 },
    Required { name: "TransitionMusic", corpus_calls: 45 },
    Required { name: "SetDynamicMusic", corpus_calls: 14 },
    Required { name: "IsDynamicMusic", corpus_calls: 2 },
    Required { name: "SetTimerUpdateMusic", corpus_calls: 4 },
    Required { name: "AddFactionMusic", corpus_calls: 4 },
    Required { name: "SetFactionMusic", corpus_calls: 4 },
    Required { name: "LockFactionMusic", corpus_calls: 8 },
    Required { name: "IsFactionLockedMusic", corpus_calls: 2 },
    Required { name: "SetActionLevelsMusic", corpus_calls: 8 },
    Required { name: "LockActionLevelMusic", corpus_calls: 15 },
    Required { name: "IsActionLevelLockedMusic", corpus_calls: 0 },
    Required { name: "SetHostilityDecayRateMusic", corpus_calls: 0 },
    Required { name: "SetSourceMusic", corpus_calls: 2 },
    Required { name: "SetSourceEnterMusic", corpus_calls: 0 },
    Required { name: "SetSourceExitMusic", corpus_calls: 0 },
    Required { name: "SetSourceMusicTransition", corpus_calls: 2 },
    Required { name: "ClearSourceMusicTransitions", corpus_calls: 2 },
    Required { name: "AddSourceMusicEntryState", corpus_calls: 2 },
    Required { name: "ClearSourceMusicEntryStates", corpus_calls: 2 },
    Required { name: "SetActionThresholdsMusic", corpus_calls: 8 },
    Required { name: "SetRootFactionRegionMusic", corpus_calls: 2 },
    Required { name: "SetHijackMusic", corpus_calls: 2 },
    Required { name: "ActivateFactionRegionMusic", corpus_calls: 2 },
    Required { name: "AddMusicState", corpus_calls: 58 },
    Required { name: "AddMusicTransition", corpus_calls: 66 },
    Required { name: "AddMusicSourcePlaylist", corpus_calls: 2 },
    Required { name: "ClearMusicSourcePlaylist", corpus_calls: 2 },
    Required { name: "RemoveMusicSourcePlaylist", corpus_calls: 0 },
    Required { name: "AddCueToMusicSourcePlaylist", corpus_calls: 2 },
    Required { name: "LoadBank", corpus_calls: 0 },
    Required { name: "UnloadBank", corpus_calls: 0 },
    Required { name: "LoadSoundBank", corpus_calls: 0 },
    Required { name: "LoadWaveBank", corpus_calls: 0 },
    Required { name: "UnloadSoundBank", corpus_calls: 0 },
    Required { name: "UnloadWaveBank", corpus_calls: 0 },
    Required { name: "LoadTempBank", corpus_calls: 2 },
    Required { name: "UnloadTempBank", corpus_calls: 2 },
    Required { name: "LoadBankWithCallback", corpus_calls: 2 },
    Required { name: "UnloadBankWithCallback", corpus_calls: 2 },
    Required { name: "RequestAmbienceBank", corpus_calls: 2 },
    Required { name: "SetStreamBlockDumping", corpus_calls: 0 },
    Required { name: "SilenceAmbience", corpus_calls: 0 },
    Required { name: "CueAmbience", corpus_calls: 2 },
    Required { name: "StopAmbience", corpus_calls: 2 },
    Required { name: "SetMessageFiltering", corpus_calls: 0 },
    Required { name: "DefineReverbPreset", corpus_calls: 2 },
    Required { name: "SetReverbPreset", corpus_calls: 4 },
    Required { name: "SetReverb", corpus_calls: 2 },
    Required { name: "SetLowPassFilterSettings", corpus_calls: 0 },
    Required { name: "SetLowPassFilter", corpus_calls: 0 },
    Required { name: "SetSurvivalMode", corpus_calls: 4 },
    Required { name: "SetVehicleEngineBoost", corpus_calls: 3 },
    Required { name: "SetCinematicMode", corpus_calls: 0 },
    Required { name: "ForceActionTransition", corpus_calls: 0 },
    Required { name: "GetMaxDuration", corpus_calls: 0 },
    Required { name: "ClearFadeCategories", corpus_calls: 0 },
    Required { name: "AddFadeCategory", corpus_calls: 0 },
    Required { name: "FadeCategoryDown", corpus_calls: 2 },
    Required { name: "FadeCategoryUp", corpus_calls: 2 },
    Required { name: "GetCategoryVolume", corpus_calls: 0 },
    Required { name: "ClearPitchCategories", corpus_calls: 0 },
    Required { name: "AddPitchCategory", corpus_calls: 0 },
    Required { name: "PitchCategoryActivate", corpus_calls: 2 },
    Required { name: "PitchCategoryDeactivate", corpus_calls: 2 },
    Required { name: "GetCategoryPitch", corpus_calls: 0 },
    Required { name: "SetSystemPause", corpus_calls: 0 },
    Required { name: "SetPauseFilter", corpus_calls: 0 },
    Required { name: "RegisterReadyCallback", corpus_calls: 2 },
    Required { name: "OpenStreamFile", corpus_calls: 4 },
    Required { name: "CloseStreamFile", corpus_calls: 2 },
    Required { name: "GetAudioDir", corpus_calls: 6 },
    Required { name: "OverrideUserMusic", corpus_calls: 1 },
    Required { name: "RestoreUserMusic", corpus_calls: 1 },
    Required { name: "StopAndFlushAllSounds", corpus_calls: 4 },
    Required { name: "SetMasterVolume", corpus_calls: 11 },
    Required { name: "_SummonEd", corpus_calls: 0 },
    Required { name: "_GetLibVersion", corpus_calls: 6 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
