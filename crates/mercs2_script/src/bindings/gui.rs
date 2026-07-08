//! `Gui` engine binding namespace — luaL_Reg table VA 0x00b9a398, 38 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Gui")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Gui";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Gui";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a398;

pub const REQUIRED: &[Required] = &[
    Required { name: "AddObjective", corpus_calls: 0 },
    Required { name: "LoadTexture", corpus_calls: 27 },
    Required { name: "GetReticlePosition", corpus_calls: 5 },
    Required { name: "LoadFont", corpus_calls: 1 },
    Required { name: "IsPdaOnSelect", corpus_calls: 2 },
    Required { name: "IsXboxController", corpus_calls: 1 },
    Required { name: "ControllerInUse", corpus_calls: 3 },
    Required { name: "FindGuiLocation", corpus_calls: 2 },
    Required { name: "_MarkerAdd", corpus_calls: 0 },
    Required { name: "_MarkerAddTripwire", corpus_calls: 0 },
    Required { name: "_MarkerAddDisc", corpus_calls: 0 },
    Required { name: "_MarkerAdd3D", corpus_calls: 0 },
    Required { name: "_MarkerSetBlipLimit", corpus_calls: 0 },
    Required { name: "_MarkerAddOld", corpus_calls: 0 },
    Required { name: "_MarkerRemove", corpus_calls: 0 },
    Required { name: "_MarkerSetLocation", corpus_calls: 0 },
    Required { name: "_MarkerSetColor", corpus_calls: 0 },
    Required { name: "_MarkerSetFollowGuid", corpus_calls: 0 },
    Required { name: "_MarkerSetScale", corpus_calls: 0 },
    Required { name: "_MarkerPulse", corpus_calls: 0 },
    Required { name: "_MarkerHaltPulse", corpus_calls: 0 },
    Required { name: "SetFactionMarkerVisibleDistance", corpus_calls: 0 },
    Required { name: "EnableFactionMarkers", corpus_calls: 0 },
    Required { name: "SetFactionMarkerSize", corpus_calls: 0 },
    Required { name: "SetVehicleEntranceMarkerVisibleDistance", corpus_calls: 0 },
    Required { name: "EnableVehicleEntranceMarkers", corpus_calls: 0 },
    Required { name: "SetVehicleEntranceMarkerSize", corpus_calls: 0 },
    Required { name: "EnablePickupMarkers", corpus_calls: 0 },
    Required { name: "SetPickupMarkerSize", corpus_calls: 4 },
    Required { name: "SetPickupMarkerVisibleDistance", corpus_calls: 2 },
    Required { name: "EnablePlayerMarkers", corpus_calls: 10 },
    Required { name: "GetLanguageNum", corpus_calls: 0 },
    Required { name: "GetLanguageName", corpus_calls: 3 },
    Required { name: "DoSigninCheck", corpus_calls: 2 },
    Required { name: "OnShellLoaded", corpus_calls: 2 },
    Required { name: "OnGlobalExit", corpus_calls: 1 },
    Required { name: "ShowLoadingHints", corpus_calls: 4 },
    Required { name: "OutputToPIX", corpus_calls: 0 },
];

/// GUI/HUD frontend cfuncs. Marker placement, objective/texture/font loading, faction/vehicle/pickup
/// marker toggles, sign-in checks, loading hints and PIX output are presentation/side-effect calls the
/// fixed-function HUD does not yet render — faithful no-ops whose returns the game never reads.
///
/// Six getters ARE read by the game and are backed with faithful neutral defaults:
/// - `IsXboxController` / `ControllerInUse` / `IsPdaOnSelect` gate KB/M-vs-controller branches → `false`
///   (PC keyboard/mouse is the faithful default).
/// - `GetReticlePosition` (→ two coords) and `FindGuiLocation` (→ four coords) feed widget arithmetic
///   → neutral zeros (nil would fault the `local x,y = ...` math).
/// - `GetLanguageName` supplies the localized-VO asset suffix → `"english"` (the base locale).
///
/// `GetLanguageNum` is not called by the corpus and stays a no-op.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    use mercs2_ui::MarkerKind;

    // --- world-space HUD markers → the real `mercs2_ui::MarkerSet` on the host. ---
    for (name, kind) in [
        ("_MarkerAdd", MarkerKind::Blip),
        ("_MarkerAddOld", MarkerKind::Blip),
        ("_MarkerAddTripwire", MarkerKind::Tripwire),
        ("_MarkerAddDisc", MarkerKind::Disc),
        ("_MarkerAdd3D", MarkerKind::ThreeD),
        ("AddObjective", MarkerKind::Objective),
    ] {
        let h = host.clone();
        b.real(name, lua.create_function(move |_, _: mlua::MultiValue| {
            Ok(h.borrow_mut().markers().map(|m| m.add(kind) as i64).unwrap_or(0))
        })?)?;
    }
    let h = host.clone();
    b.real("_MarkerRemove", lua.create_function(move |_, id: i64| {
        if let Some(m) = h.borrow_mut().markers() { m.remove(id as u64); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerSetLocation", lua.create_function(move |_, (id, x, y, z): (i64, f32, f32, Option<f32>)| {
        if let Some(m) = h.borrow_mut().markers() { m.set_location(id as u64, [x, y, z.unwrap_or(0.0)]); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerSetColor", lua.create_function(move |_, (id, r, g, bl, a): (i64, f32, f32, f32, Option<f32>)| {
        if let Some(m) = h.borrow_mut().markers() { m.set_color(id as u64, [r, g, bl, a.unwrap_or(255.0)]); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerSetFollowGuid", lua.create_function(move |_, (id, guid): (i64, i64)| {
        if let Some(m) = h.borrow_mut().markers() { m.set_follow(id as u64, guid as u64); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerSetScale", lua.create_function(move |_, (id, s): (i64, f32)| {
        if let Some(m) = h.borrow_mut().markers() { m.set_scale(id as u64, s); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerPulse", lua.create_function(move |_, id: i64| {
        if let Some(m) = h.borrow_mut().markers() { m.set_pulsing(id as u64, true); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerHaltPulse", lua.create_function(move |_, id: i64| {
        if let Some(m) = h.borrow_mut().markers() { m.set_pulsing(id as u64, false); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("_MarkerSetBlipLimit", lua.create_function(move |_, n: i64| {
        if let Some(m) = h.borrow_mut().markers() { m.blip_limit = n.max(0) as u32; }
        Ok(())
    })?)?;

    // --- texture/font handles: addressed by name (identity handle) so `Hud.SetImageTexture(w, tex)` gets
    // a usable value. The actual GPU upload is a render-pass concern. ---
    b.real("LoadTexture", lua.create_function(|_, name: String| Ok(name))?)?;
    b.real("LoadFont", lua.create_function(|_, name: String| Ok(name))?)?;

    // --- UNBACKED residue (burn-down): faction/vehicle/pickup/player marker CATEGORY toggles (need a
    // marker-category config + the render pass), sign-in/shell lifecycle, loading hints, PIX. No-ops. ---
    super::record_all(&mut b, lua, host, "Gui", &[
        "SetFactionMarkerVisibleDistance", "EnableFactionMarkers", "SetFactionMarkerSize",
        "SetVehicleEntranceMarkerVisibleDistance", "EnableVehicleEntranceMarkers",
        "SetVehicleEntranceMarkerSize", "EnablePickupMarkers", "SetPickupMarkerSize",
        "SetPickupMarkerVisibleDistance", "EnablePlayerMarkers", "GetLanguageNum", "DoSigninCheck",
        "OnShellLoaded", "OnGlobalExit", "ShowLoadingHints", "OutputToPIX",
    ])?;

    // Input-mode queries gate KB/M-vs-controller branches — PC default = keyboard/mouse.
    b.real(
        "IsXboxController",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;
    b.real(
        "ControllerInUse",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;
    b.real(
        "IsPdaOnSelect",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;
    // Screen-space queries feeding widget arithmetic — neutral coordinates.
    b.real(
        "GetReticlePosition",
        lua.create_function(|_, _: mlua::MultiValue| Ok((0.0f64, 0.0f64)))?,
    )?;
    b.real(
        "FindGuiLocation",
        lua.create_function(|_, _: mlua::MultiValue| Ok((0.0f64, 0.0f64, 0.0f64, 0.0f64)))?,
    )?;
    // Localized-VO asset suffix — faithful base locale.
    b.real(
        "GetLanguageName",
        lua.create_function(|_, _: mlua::MultiValue| Ok(String::from("english")))?,
    )?;

    b.install_global(GLOBAL)
}
