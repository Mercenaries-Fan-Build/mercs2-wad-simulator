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

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

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

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
