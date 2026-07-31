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

use mercs2_luac::rt::{Lua, Result as LuaResult};

use crate::{Guid, SharedHost};
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
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Pose/shake/blend → the real cinematic camera controller state on the host.
    let h = host.clone();
    b.real("SetYaw", lua.create_function(move |_, y: f32| { h.borrow_mut().camera_set_yaw(y); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetYaw", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| Ok(h.borrow().camera_yaw()))?)?;
    let h = host.clone();
    b.real("SetPitch", lua.create_function(move |_, p: f32| { h.borrow_mut().camera_set_pitch(p); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetPitch", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| Ok(h.borrow().camera_pitch()))?)?;
    let h = host.clone();
    b.real("SetFOV", lua.create_function(move |_, f: f32| { h.borrow_mut().camera_set_fov(f); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetFOV", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| Ok(h.borrow().camera_fov()))?)?;
    let h = host.clone();
    b.real("SetPosition", lua.create_function(move |_, (x, y, z): (f32, f32, f32)| { h.borrow_mut().camera_set_position([x, y, z]); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetLookAt", lua.create_function(move |_, (x, y, z): (f32, f32, f32)| { h.borrow_mut().camera_set_lookat([x, y, z]); Ok(()) })?)?;
    let h = host.clone();
    // ⚠ `Camera.Shake(uCamera, sShake, uTarget, nAmplitude, nTime)` — five arguments, the first and
    // third of them **handles**: `pmccon004.lua:117` is
    // `Camera.Shake(playerCamera, "ShakeCameraMedium", playerCharacter, 6, 5)` and `oilrig.lua:38`
    // is `Camera.Shake(StringToGuid("0x1"), "ShakeCameraConstantlyRandom", uiGuid, 0.5, 2000)`.
    // The earlier single-`Option<f32>` signature read the camera handle as the intensity; with
    // handles now arriving as lightuserdata that is not a mis-read but a raised conversion error, so
    // the whole shape has to be spelled out. Only the amplitude reaches the fixed-function camera
    // controller — the named shake profile and the target handle need a shake-curve model we do not
    // have (burn-down).
    b.real("Shake", lua.create_function(move |_, (_camera, _shake, _target, amplitude, _time): (Guid, Option<String>, Guid, Option<f32>, Option<f32>)| {
        h.borrow_mut().camera_shake(amplitude.unwrap_or(1.0));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("Blend", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| { h.borrow_mut().camera_set_blending(true); Ok(()) })?)?;
    let h = host.clone();
    b.real("StopBlending", lua.create_function(move |_, _: mercs2_luac::rt::MultiValue| { h.borrow_mut().camera_set_blending(false); Ok(()) })?)?;
    let h = host.clone();
    b.real("Follow", lua.create_function(move |_, guid: Guid| { h.borrow_mut().camera_follow(guid.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("Hold", lua.create_function(move |_, on: Option<bool>| { h.borrow_mut().camera_hold(on.unwrap_or(true)); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetShot", lua.create_function(move |_, shot: String| { h.borrow_mut().camera_set_shot(&shot); Ok(()) })?)?;

    // Preserve the sibling `Camera` surface installed earlier by `camera.rs`; `install_global` below
    // replaces the global table, so copy those functions into ours first (no name overlap).
    if let Ok(existing) = lua.globals().get::<mercs2_luac::rt::Table>(GLOBAL) {
        for pair in existing.pairs::<String, mercs2_luac::rt::Function>() {
            let (k, f) = pair?;
            b.extra(&k, f)?;
        }
    }

    b.install_global(GLOBAL)
}
