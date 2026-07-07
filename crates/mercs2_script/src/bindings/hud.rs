//! `Hud` engine binding namespace — luaL_Reg table VA 0x00b99ff8, 114 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Hud")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Hud";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Hud";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99ff8;

pub const REQUIRED: &[Required] = &[
    Required { name: "CreateWidget", corpus_calls: 0 },
    Required { name: "DeleteWidget", corpus_calls: 0 },
    Required { name: "SetWidgetLocation", corpus_calls: 0 },
    Required { name: "GetWidgetLocation", corpus_calls: 0 },
    Required { name: "GetWidgetHighlightable", corpus_calls: 0 },
    Required { name: "SetWidgetHighlightable", corpus_calls: 0 },
    Required { name: "SetWidgetCorrectedLocation", corpus_calls: 0 },
    Required { name: "GetWidgetCorrectedLocation", corpus_calls: 0 },
    Required { name: "SetWidgetColor", corpus_calls: 0 },
    Required { name: "GetWidgetColor", corpus_calls: 0 },
    Required { name: "SetWidgetVisible", corpus_calls: 0 },
    Required { name: "GetWidgetVisible", corpus_calls: 0 },
    Required { name: "SetWidgetIgnoresPause", corpus_calls: 0 },
    Required { name: "GetWidgetIgnoresPause", corpus_calls: 0 },
    Required { name: "ActivateWidget", corpus_calls: 0 },
    Required { name: "SetWidgetSleep", corpus_calls: 0 },
    Required { name: "GetWidgetSleep", corpus_calls: 0 },
    Required { name: "PushWidgetToFront", corpus_calls: 0 },
    Required { name: "PushWidgetToBack", corpus_calls: 0 },
    Required { name: "SetWidgetAnchoring", corpus_calls: 0 },
    Required { name: "GetWidgetAnchoring", corpus_calls: 0 },
    Required { name: "InterpolateWidget", corpus_calls: 0 },
    Required { name: "SetWidgetUpdateCallback", corpus_calls: 0 },
    Required { name: "SetWidgetViewport", corpus_calls: 0 },
    Required { name: "GetWidgetViewport", corpus_calls: 0 },
    Required { name: "AddWidgetChild", corpus_calls: 0 },
    Required { name: "SetWidgetChild", corpus_calls: 0 },
    Required { name: "RemoveWidgetChild", corpus_calls: 0 },
    Required { name: "RemoveAllWidgetChildren", corpus_calls: 0 },
    Required { name: "GetWidgetChildren", corpus_calls: 0 },
    Required { name: "SetWidgetFullscreen", corpus_calls: 0 },
    Required { name: "CorrectWidgetForResolution", corpus_calls: 0 },
    Required { name: "SetWidgetUseResolutionCorrection", corpus_calls: 0 },
    Required { name: "SetWidgetUseNewRescale", corpus_calls: 0 },
    Required { name: "GetWidgetHighlightId", corpus_calls: 0 },
    Required { name: "GetWidgetDownId", corpus_calls: 0 },
    Required { name: "CreateImageWidget", corpus_calls: 0 },
    Required { name: "SetImageTexture", corpus_calls: 0 },
    Required { name: "SetImageRotation", corpus_calls: 0 },
    Required { name: "GetImageRotation", corpus_calls: 0 },
    Required { name: "SetImageTextureCoordinates", corpus_calls: 0 },
    Required { name: "GetImageTextureCoordinates", corpus_calls: 0 },
    Required { name: "SetImageTiling", corpus_calls: 0 },
    Required { name: "SetImageTextureTransience", corpus_calls: 0 },
    Required { name: "SetImageClockAnimation", corpus_calls: 0 },
    Required { name: "SetImageClockCallback", corpus_calls: 0 },
    Required { name: "GetImageClockElapsed", corpus_calls: 0 },
    Required { name: "SetImagePieSliceRender", corpus_calls: 0 },
    Required { name: "DisableImagePieSliceRender", corpus_calls: 0 },
    Required { name: "CreateTextWidget", corpus_calls: 0 },
    Required { name: "SetTextText", corpus_calls: 0 },
    Required { name: "GetTextText", corpus_calls: 0 },
    Required { name: "SetTextFont", corpus_calls: 0 },
    Required { name: "SetTextWrapping", corpus_calls: 0 },
    Required { name: "GetTextWrapping", corpus_calls: 0 },
    Required { name: "GetTextWidth", corpus_calls: 0 },
    Required { name: "GetTextHeight", corpus_calls: 0 },
    Required { name: "SetTextJustification", corpus_calls: 0 },
    Required { name: "GetTextJustification", corpus_calls: 0 },
    Required { name: "SetTextScale", corpus_calls: 0 },
    Required { name: "GetTextScale", corpus_calls: 0 },
    Required { name: "SplitText", corpus_calls: 0 },
    Required { name: "AnimateText", corpus_calls: 0 },
    Required { name: "HaltTextAnimation", corpus_calls: 0 },
    Required { name: "MinimapCreate", corpus_calls: 0 },
    Required { name: "MinimapUpdate", corpus_calls: 0 },
    Required { name: "MinimapSetPlayerLocation", corpus_calls: 0 },
    Required { name: "MinimapSetFocusLocation", corpus_calls: 0 },
    Required { name: "MinimapSetRotation", corpus_calls: 0 },
    Required { name: "MinimapSetRange", corpus_calls: 0 },
    Required { name: "SetMinimapOwner", corpus_calls: 0 },
    Required { name: "SetMinimapBorder", corpus_calls: 0 },
    Required { name: "SetMinimapRadius", corpus_calls: 0 },
    Required { name: "MinimapAddObjective", corpus_calls: 0 },
    Required { name: "MinimapAnimateObjectiveSize", corpus_calls: 0 },
    Required { name: "MinimapAnimateObjectiveAlpha", corpus_calls: 0 },
    Required { name: "MinimapAnimateObjectiveSonar", corpus_calls: 0 },
    Required { name: "MinimapUnanimateObjective", corpus_calls: 0 },
    Required { name: "MinimapRemoveObjective", corpus_calls: 0 },
    Required { name: "MinimapDelete", corpus_calls: 0 },
    Required { name: "SetPlayerPDAWidget", corpus_calls: 0 },
    Required { name: "CreateFlashWidget", corpus_calls: 0 },
    Required { name: "SetFlashSwfFile", corpus_calls: 0 },
    Required { name: "SetFlashPlaySpeed", corpus_calls: 0 },
    Required { name: "GetFlashPlaySpeed", corpus_calls: 0 },
    Required { name: "PauseFlash", corpus_calls: 0 },
    Required { name: "PlayFlash", corpus_calls: 0 },
    Required { name: "RestartFlash", corpus_calls: 0 },
    Required { name: "SendFlashInput", corpus_calls: 0 },
    Required { name: "SendFlashLeftAnalogInput", corpus_calls: 0 },
    Required { name: "SendFlashRightAnalogInput", corpus_calls: 0 },
    Required { name: "SetFlashCallback", corpus_calls: 0 },
    Required { name: "CallFlashScriptFunction", corpus_calls: 0 },
    Required { name: "SetFlashPauseMenu", corpus_calls: 0 },
    Required { name: "SetFlashTesselationAllowed", corpus_calls: 0 },
    Required { name: "RemoveFlashPauseMenu", corpus_calls: 0 },
    Required { name: "CreateSpriteWidget", corpus_calls: 0 },
    Required { name: "SetSpriteTexture", corpus_calls: 0 },
    Required { name: "SetSpriteTextureSize", corpus_calls: 0 },
    Required { name: "SetSpriteFrameSize", corpus_calls: 0 },
    Required { name: "AnimateSprite", corpus_calls: 0 },
    Required { name: "HaltSpriteAnimation", corpus_calls: 0 },
    Required { name: "SetSpriteFrame", corpus_calls: 0 },
    Required { name: "CreateMovieWidget", corpus_calls: 0 },
    Required { name: "SetMovieFile", corpus_calls: 0 },
    Required { name: "PlayMovie", corpus_calls: 0 },
    Required { name: "PauseMovie", corpus_calls: 0 },
    Required { name: "StopMovie", corpus_calls: 0 },
    Required { name: "GetMovieCurrentFrameNumber", corpus_calls: 0 },
    Required { name: "SetMovieEndCallback", corpus_calls: 0 },
    Required { name: "RegisterForPdaUpdate", corpus_calls: 0 },
    Required { name: "RemovePdaBlip", corpus_calls: 0 },
    Required { name: "UpdatePdaBlip", corpus_calls: 0 },
    Required { name: "AddPdaMapBlips", corpus_calls: 0 },
];

/// Names backed by a deliberate **faithful no-op** stub: every widget/image/text/minimap/flash/
/// sprite/movie/pda *mutator* (create/set/show/hide/update/animate/play/…). The retail bodies drive
/// the Scaleform GFx HUD overlay; this build renders no GFx HUD yet, so a silent no-op is the
/// faithful behavior — the game's Lua HUD managers (`mrxguiinterface`, `mrxguimanager`) run their
/// control flow unchanged, they simply produce no on-screen overlay. Getters are handled separately
/// in [`install`] (real bodies returning sane defaults so no Lua arithmetic hits `nil`).
const STUB_NAMES: &[&str] = &[
    // --- widget lifecycle / transform / state (mutators) ---
    "CreateWidget",
    "DeleteWidget",
    "SetWidgetLocation",
    "SetWidgetHighlightable",
    "SetWidgetCorrectedLocation",
    "SetWidgetColor",
    "SetWidgetVisible",
    "SetWidgetIgnoresPause",
    "ActivateWidget",
    "SetWidgetSleep",
    "PushWidgetToFront",
    "PushWidgetToBack",
    "SetWidgetAnchoring",
    "InterpolateWidget",
    "SetWidgetUpdateCallback",
    "SetWidgetViewport",
    "AddWidgetChild",
    "SetWidgetChild",
    "RemoveWidgetChild",
    "RemoveAllWidgetChildren",
    "SetWidgetFullscreen",
    "CorrectWidgetForResolution",
    "SetWidgetUseResolutionCorrection",
    "SetWidgetUseNewRescale",
    // --- image widget (mutators) ---
    "CreateImageWidget",
    "SetImageTexture",
    "SetImageRotation",
    "SetImageTextureCoordinates",
    "SetImageTiling",
    "SetImageTextureTransience",
    "SetImageClockAnimation",
    "SetImageClockCallback",
    "SetImagePieSliceRender",
    "DisableImagePieSliceRender",
    // --- text widget (mutators) ---
    "CreateTextWidget",
    "SetTextText",
    "SetTextFont",
    "SetTextWrapping",
    "SetTextJustification",
    "SetTextScale",
    "SplitText",
    "AnimateText",
    "HaltTextAnimation",
    // --- minimap (mutators) ---
    "MinimapCreate",
    "MinimapUpdate",
    "MinimapSetPlayerLocation",
    "MinimapSetFocusLocation",
    "MinimapSetRotation",
    "MinimapSetRange",
    "SetMinimapOwner",
    "SetMinimapBorder",
    "SetMinimapRadius",
    "MinimapAddObjective",
    "MinimapAnimateObjectiveSize",
    "MinimapAnimateObjectiveAlpha",
    "MinimapAnimateObjectiveSonar",
    "MinimapUnanimateObjective",
    "MinimapRemoveObjective",
    "MinimapDelete",
    "SetPlayerPDAWidget",
    // --- flash widget (mutators) ---
    "CreateFlashWidget",
    "SetFlashSwfFile",
    "SetFlashPlaySpeed",
    "PauseFlash",
    "PlayFlash",
    "RestartFlash",
    "SendFlashInput",
    "SendFlashLeftAnalogInput",
    "SendFlashRightAnalogInput",
    "SetFlashCallback",
    "CallFlashScriptFunction",
    "SetFlashPauseMenu",
    "SetFlashTesselationAllowed",
    "RemoveFlashPauseMenu",
    // --- sprite widget (mutators) ---
    "CreateSpriteWidget",
    "SetSpriteTexture",
    "SetSpriteTextureSize",
    "SetSpriteFrameSize",
    "AnimateSprite",
    "HaltSpriteAnimation",
    "SetSpriteFrame",
    // --- movie widget (mutators) ---
    "CreateMovieWidget",
    "SetMovieFile",
    "PlayMovie",
    "PauseMovie",
    "StopMovie",
    "SetMovieEndCallback",
    // --- PDA blips (mutators) ---
    "RegisterForPdaUpdate",
    "RemovePdaBlip",
    "UpdatePdaBlip",
    "AddPdaMapBlips",
];

/// Faithful HUD binding surface. We render no Scaleform GFx HUD yet, so every mutator is a silent
/// no-op and every query returns the sane engine default (a widget with default transform/state).
/// This keeps the game's Lua HUD managers running their exact control flow — the only observable
/// difference from retail is the absent overlay. No [`crate::EngineHost`] method is needed.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // --- mutators: faithful no-ops (ignore all args, return nil) ---
    for &name in STUB_NAMES {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    // --- queries: real bodies returning sane defaults so Lua never does arithmetic on nil ---
    // Boolean state getters — nothing is shown/awake in a HUD-less build.
    b.real("GetWidgetVisible", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetWidgetHighlightable", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetWidgetIgnoresPause", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetWidgetSleep", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetTextWrapping", lua.create_function(|_, _: MultiValue| Ok(false))?)?;

    // Location / viewport / color / texcoords — return neutral numeric tuples.
    b.real("GetWidgetLocation", lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32)))?)?;
    b.real("GetWidgetCorrectedLocation", lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32)))?)?;
    b.real("GetWidgetViewport", lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32, 0.0f32, 0.0f32)))?)?;
    b.real("GetWidgetColor", lua.create_function(|_, _: MultiValue| Ok((255.0f32, 255.0f32, 255.0f32, 255.0f32)))?)?;
    b.real("GetImageTextureCoordinates", lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32, 1.0f32, 1.0f32)))?)?;

    // Scalar numeric getters — 0 where a measurement/id, 1 where a multiplier/speed.
    b.real("GetWidgetAnchoring", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetWidgetHighlightId", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetWidgetDownId", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetImageRotation", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("GetImageClockElapsed", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("GetTextWidth", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("GetTextHeight", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("GetTextJustification", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetTextScale", lua.create_function(|_, _: MultiValue| Ok(1.0f32))?)?;
    b.real("GetFlashPlaySpeed", lua.create_function(|_, _: MultiValue| Ok(1.0f32))?)?;
    b.real("GetMovieCurrentFrameNumber", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;

    // String getter — empty text.
    b.real("GetTextText", lua.create_function(|_, _: MultiValue| Ok(String::new()))?)?;

    // Child list getter — empty table (iterating it is a no-op).
    b.real("GetWidgetChildren", lua.create_function(|lua, _: MultiValue| lua.create_table())?)?;

    b.install_global(GLOBAL)
}
