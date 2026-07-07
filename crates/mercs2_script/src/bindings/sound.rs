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

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

fn voice_opt(v: u64) -> Option<i64> {
    if v == 0 { None } else { Some(v as i64) }
}

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

/// Cue playback + the dual-deck music FSM + category volumes, forwarded to the audio system through
/// [`crate::EngineHost`] (the real host backs it with `mercs2_audio::AudioEngine`). A `0` voice id
/// maps to Lua `nil`. The ~50 remaining faction/action/source-music + bank cfuncs are a later pass.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // --- cue playback ---
    let h = host.clone();
    b.real("CueSound", lua.create_function(move |_, cue: String| Ok(voice_opt(h.borrow_mut().sound_cue(&cue))))?)?;
    let h = host.clone();
    b.real("StopSound", lua.create_function(move |_, v: i64| { h.borrow_mut().sound_stop(v as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("PauseSound", lua.create_function(move |_, v: i64| { h.borrow_mut().sound_pause(v as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("StopAndFlushAllSounds", lua.create_function(move |_, ()| { h.borrow_mut().sound_stop_all(); Ok(()) })?)?;
    let h = host.clone();
    b.real("CueAmbience", lua.create_function(move |_, cue: String| Ok(voice_opt(h.borrow_mut().sound_cue_ambience(&cue))))?)?;
    let h = host.clone();
    b.real("StopAmbience", lua.create_function(move |_, ()| { h.borrow_mut().sound_stop_ambience(); Ok(()) })?)?;

    // --- category volumes / fades ---
    let h = host.clone();
    b.real("SetCategoryVolume", lua.create_function(move |_, (cat, vol): (String, f32)| { h.borrow_mut().sound_set_category_volume(&cat, vol); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetMasterVolume", lua.create_function(move |_, vol: f32| { h.borrow_mut().sound_set_master_volume(vol); Ok(()) })?)?;
    let h = host.clone();
    b.real("FadeCategoryDown", lua.create_function(move |_, cat: String| { h.borrow_mut().sound_fade_category(&cat, true); Ok(()) })?)?;
    let h = host.clone();
    b.real("FadeCategoryUp", lua.create_function(move |_, cat: String| { h.borrow_mut().sound_fade_category(&cat, false); Ok(()) })?)?;

    // --- dual-deck music FSM ---
    let h = host.clone();
    b.real("TransitionMusic", lua.create_function(move |_, state: String| Ok(h.borrow_mut().sound_transition_music(&state)))?)?;
    let h = host.clone();
    b.real("AddMusicState", lua.create_function(move |_, name: String| { h.borrow_mut().sound_add_music_state(&name); Ok(()) })?)?;
    let h = host.clone();
    b.real("AddMusicTransition", lua.create_function(move |_, (from, to): (String, String)| { h.borrow_mut().sound_add_music_transition(&from, &to); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetDynamicMusic", lua.create_function(move |_, on: bool| { h.borrow_mut().sound_set_dynamic_music(on); Ok(()) })?)?;
    let h = host.clone();
    b.real("IsDynamicMusic", lua.create_function(move |_, ()| Ok(h.borrow().sound_is_dynamic_music()))?)?;
    let h = host.clone();
    b.real("BindMusicCue", lua.create_function(move |_, (state, cue): (String, String)| { h.borrow_mut().sound_bind_music_cue(&state, &cue); Ok(()) })?)?;
    let h = host.clone();
    b.real("ClearMusicCues", lua.create_function(move |_, ()| { h.borrow_mut().sound_clear_music_cues(); Ok(()) })?)?;
    let h = host.clone();
    b.real("LockActionLevelMusic", lua.create_function(move |_, level: i64| { h.borrow_mut().sound_lock_action_level_music(level); Ok(()) })?)?;

    // --- info ---
    let h = host.clone();
    b.real("GetAudioDir", lua.create_function(move |_, ()| Ok(h.borrow().sound_audio_dir()))?)?;
    let h = host.clone();
    b.real("_GetLibVersion", lua.create_function(move |_, ()| Ok(h.borrow().sound_lib_version()))?)?;

    // --- test cue variants (route through the same cue playback path) ---
    let h = host.clone();
    b.real("TestCueSound", lua.create_function(move |_, cue: String| Ok(voice_opt(h.borrow_mut().sound_cue(&cue))))?)?;
    let h = host.clone();
    b.real("TestStopSound", lua.create_function(move |_, v: i64| { h.borrow_mut().sound_stop(v as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("TestPauseSound", lua.create_function(move |_, v: i64| { h.borrow_mut().sound_pause(v as u64); Ok(()) })?)?;

    // --- faithful-default GETTERS (game reads the return; no host state modelled yet) ---
    // `is-locked` music queries → never locked in a fresh session.
    b.real("IsFactionLockedMusic", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsActionLevelLockedMusic", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    // cue length lookup → 0 duration (no bank metadata loaded).
    b.real("GetMaxDuration", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    // category volume/pitch reads → neutral (unity gain / unity pitch).
    b.real("GetCategoryVolume", lua.create_function(|_, _: MultiValue| Ok(1.0f32))?)?;
    b.real("GetCategoryPitch", lua.create_function(|_, _: MultiValue| Ok(1.0f32))?)?;
    // stream file handle → nil (no async stream file opened).
    b.real("OpenStreamFile", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // --- faithful no-op SETTERS / actions the retail audio driver consumes but we don't model yet ---
    // (category pitch + listener + all faction/action/source dynamic-music tuning; bank load/unload;
    //  reverb/low-pass DSP; misc mode toggles; stream close + user-music override; dev summon.)
    for name in [
        "SetCategoryPitch",
        "LockListenerPosition",
        "SetTimerUpdateMusic",
        "AddFactionMusic",
        "SetFactionMusic",
        "LockFactionMusic",
        "SetActionLevelsMusic",
        "SetHostilityDecayRateMusic",
        "SetSourceMusic",
        "SetSourceEnterMusic",
        "SetSourceExitMusic",
        "SetSourceMusicTransition",
        "ClearSourceMusicTransitions",
        "AddSourceMusicEntryState",
        "ClearSourceMusicEntryStates",
        "SetActionThresholdsMusic",
        "SetRootFactionRegionMusic",
        "SetHijackMusic",
        "ActivateFactionRegionMusic",
        "AddMusicSourcePlaylist",
        "ClearMusicSourcePlaylist",
        "RemoveMusicSourcePlaylist",
        "AddCueToMusicSourcePlaylist",
        "LoadBank",
        "UnloadBank",
        "LoadSoundBank",
        "LoadWaveBank",
        "UnloadSoundBank",
        "UnloadWaveBank",
        "LoadTempBank",
        "UnloadTempBank",
        "LoadBankWithCallback",
        "UnloadBankWithCallback",
        "RequestAmbienceBank",
        "SetStreamBlockDumping",
        "SilenceAmbience",
        "SetMessageFiltering",
        "DefineReverbPreset",
        "SetReverbPreset",
        "SetReverb",
        "SetLowPassFilterSettings",
        "SetLowPassFilter",
        "SetSurvivalMode",
        "SetVehicleEngineBoost",
        "SetCinematicMode",
        "ForceActionTransition",
        "ClearFadeCategories",
        "AddFadeCategory",
        "ClearPitchCategories",
        "AddPitchCategory",
        "PitchCategoryActivate",
        "PitchCategoryDeactivate",
        "SetSystemPause",
        "SetPauseFilter",
        "RegisterReadyCallback",
        "CloseStreamFile",
        "OverrideUserMusic",
        "RestoreUserMusic",
        "_SummonEd",
    ] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    b.install_global(GLOBAL)
}
