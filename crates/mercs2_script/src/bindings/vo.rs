//! `VO` engine binding namespace — luaL_Reg table VA 0x00b988b0, 11 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("VO")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

fn voice_opt(v: u64) -> Option<i64> {
    if v == 0 { None } else { Some(v as i64) }
}

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "VO";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "VO";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b988b0;

pub const REQUIRED: &[Required] = &[
    Required { name: "Cue", corpus_calls: 7 },
    Required { name: "CueWithoutSubtitles", corpus_calls: 5 },
    Required { name: "Cancel", corpus_calls: 11 },
    Required { name: "CancelAll", corpus_calls: 6 },
    Required { name: "Pause", corpus_calls: 0 },
    Required { name: "PauseAll", corpus_calls: 0 },
    Required { name: "Unpause", corpus_calls: 0 },
    Required { name: "UnpauseAll", corpus_calls: 0 },
    Required { name: "SetCinematicMode", corpus_calls: 8 },
    Required { name: "AddSequence", corpus_calls: 1 },
    Required { name: "RemoveSequence", corpus_calls: 1 },
];

/// Voice-over playback. `Cue`/`CueWithoutSubtitles` play a VO line through the audio host
/// ([`crate::EngineHost::vo_cue`]) and return the voice id (`0` → nil). Call shape is
/// `VO.Cue(speakerGuid, cueHandle, [onComplete, args, ...])`; we forward the cue handle and ignore
/// the trailing completion-callback args (a depth-pass VO-callback host seam — see report). The
/// pause/cancel/sequence controls drive VO state we don't own yet, so they are faithful no-ops.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real("Cue", lua.create_function(move |_, (_speaker, cue, _rest): (i64, String, MultiValue)| {
        Ok(voice_opt(h.borrow_mut().vo_cue(&cue)))
    })?)?;
    let h = host.clone();
    b.real("CueWithoutSubtitles", lua.create_function(move |_, (_speaker, cue, _rest): (i64, String, MultiValue)| {
        Ok(voice_opt(h.borrow_mut().vo_cue(&cue)))
    })?)?;

    // Cancel / pause / cinematic-mode → the real VoManager (via the host AudioEngine).
    let h = host.clone();
    b.real("Cancel", lua.create_function(move |_, (_speaker, cue): (Option<i64>, String)| {
        h.borrow_mut().vo_cancel(&cue);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("CancelAll", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().vo_cancel_all(); Ok(()) })?)?;
    let h = host.clone();
    b.real("Pause", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().vo_set_paused(true); Ok(()) })?)?;
    let h = host.clone();
    b.real("PauseAll", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().vo_set_paused(true); Ok(()) })?)?;
    let h = host.clone();
    b.real("Unpause", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().vo_set_paused(false); Ok(()) })?)?;
    let h = host.clone();
    b.real("UnpauseAll", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().vo_set_paused(false); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetCinematicMode", lua.create_function(move |_, on: Option<bool>| { h.borrow_mut().vo_set_cinematic_mode(on.unwrap_or(true)); Ok(()) })?)?;

    // UNBACKED residue (burn-down): VO sequence playlists need a sequence model (not in vo.rs yet).
    b.stub("AddSequence", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("RemoveSequence", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
