//! `Hud` engine binding namespace — luaL_Reg table VA 0x00b99ff8, 114 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("_GuiInternal")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Value};

use crate::{Guid, SharedHost};
use super::{Installed, NsBuilder, Required};

/// The Lua side of the HUD's retained-callback registry: the `Function` the script registered plus the
/// context arguments it supplied, keyed by the opaque id `mercs2_ui` holds.
///
/// **Why this exists.** `SetMovieEndCallback` used to go through `record_all`, whose `stringify_arg`
/// maps `Value::Function` → `""`. The closure was destroyed at registration and the resulting tuple was
/// pushed into a Vec nothing drains, so a movie could never report completion — and since
/// `MrxGuiCinematic`'s end callback is what releases `STATE_WAITFORGAME`
/// (`wifmissionflow.lua:44 → MrxState.Exit(STATE_WAITFORGAME, _EndBlockingSequence)`), the whole
/// world-load state machine stalled behind the intro cinematic.
#[derive(Default)]
pub struct HudCallbacks {
    fns: BTreeMap<u32, (Function, Vec<Value>)>,
}

/// Shared handle, published into the Lua app-data exactly as `bindings::event` publishes its
/// `EventManager`.
pub type Cbs = Rc<RefCell<HudCallbacks>>;

/// Drain the widget tree's finished movies and invoke their retained Lua callbacks as
/// `fCallback(unpack(tData))`.
///
/// Called once per tick by the engine's resident pump, mirroring `ScriptHost::fire_*` for the event
/// bus. Errors propagate so a broken handler is visible rather than swallowed.
pub fn pump_hud_callbacks(lua: &Lua, host: &SharedHost, dt: f32) -> LuaResult<()> {
    let Some(cbs) = lua.app_data_ref::<Cbs>().map(|c| c.clone()) else { return Ok(()) };
    let fires = {
        let mut g = host.borrow_mut();
        match g.hud() {
            Some(tree) => {
                tree.tick_movies();
                tree.tick_animations(dt);
                // Animations first: a completed animation commonly *starts* the movie, and dispatching
                // in this order lets that happen a tick sooner.
                let mut f = tree.take_anim_completions();
                f.extend(tree.take_movie_end_fires());
                f
            }
            None => Vec::new(),
        }
    };
    for id in fires {
        let entry = cbs.borrow().fns.get(&id).cloned();
        if let Some((f, ctx)) = entry {
            f.call::<()>(mlua::MultiValue::from_vec(ctx))?;
        }
    }
    Ok(())
}

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
/// Registry row 12 (`0x00DFD508`) names this table **`_GuiInternal`**. It is NOT `Hud`: `Hud` is a
/// Lua global the game's own resident script assigns (`mrxguiinterface.lua:13`, `_G.Hud = HudInterface`),
/// so installing 114 cfuncs under that name squatted on it. Corrected 2026-07-26.
pub const NAMESPACE: &str = "_GuiInternal";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "_GuiInternal";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99ff8;

pub const REQUIRED: &[Required] = &[
    Required { name: "CreateWidget", corpus_calls: 2 },
    Required { name: "DeleteWidget", corpus_calls: 4 },
    Required { name: "SetWidgetLocation", corpus_calls: 6 },
    Required { name: "GetWidgetLocation", corpus_calls: 4 },
    Required { name: "GetWidgetHighlightable", corpus_calls: 0 },
    Required { name: "SetWidgetHighlightable", corpus_calls: 2 },
    Required { name: "SetWidgetCorrectedLocation", corpus_calls: 4 },
    Required { name: "GetWidgetCorrectedLocation", corpus_calls: 4 },
    Required { name: "SetWidgetColor", corpus_calls: 4 },
    Required { name: "GetWidgetColor", corpus_calls: 4 },
    Required { name: "SetWidgetVisible", corpus_calls: 4 },
    Required { name: "GetWidgetVisible", corpus_calls: 2 },
    Required { name: "SetWidgetIgnoresPause", corpus_calls: 2 },
    Required { name: "GetWidgetIgnoresPause", corpus_calls: 2 },
    Required { name: "ActivateWidget", corpus_calls: 4 },
    Required { name: "SetWidgetSleep", corpus_calls: 4 },
    Required { name: "GetWidgetSleep", corpus_calls: 2 },
    Required { name: "PushWidgetToFront", corpus_calls: 4 },
    Required { name: "PushWidgetToBack", corpus_calls: 4 },
    Required { name: "SetWidgetAnchoring", corpus_calls: 2 },
    Required { name: "GetWidgetAnchoring", corpus_calls: 2 },
    Required { name: "InterpolateWidget", corpus_calls: 4 },
    Required { name: "SetWidgetUpdateCallback", corpus_calls: 2 },
    Required { name: "SetWidgetViewport", corpus_calls: 3 },
    Required { name: "GetWidgetViewport", corpus_calls: 1 },
    Required { name: "AddWidgetChild", corpus_calls: 2 },
    Required { name: "SetWidgetChild", corpus_calls: 2 },
    Required { name: "RemoveWidgetChild", corpus_calls: 2 },
    Required { name: "RemoveAllWidgetChildren", corpus_calls: 2 },
    Required { name: "GetWidgetChildren", corpus_calls: 2 },
    Required { name: "SetWidgetFullscreen", corpus_calls: 2 },
    Required { name: "CorrectWidgetForResolution", corpus_calls: 6 },
    Required { name: "SetWidgetUseResolutionCorrection", corpus_calls: 2 },
    Required { name: "SetWidgetUseNewRescale", corpus_calls: 6 },
    Required { name: "GetWidgetHighlightId", corpus_calls: 8 },
    Required { name: "GetWidgetDownId", corpus_calls: 6 },
    Required { name: "CreateImageWidget", corpus_calls: 2 },
    Required { name: "SetImageTexture", corpus_calls: 2 },
    Required { name: "SetImageRotation", corpus_calls: 2 },
    Required { name: "GetImageRotation", corpus_calls: 2 },
    Required { name: "SetImageTextureCoordinates", corpus_calls: 2 },
    Required { name: "GetImageTextureCoordinates", corpus_calls: 4 },
    Required { name: "SetImageTiling", corpus_calls: 4 },
    Required { name: "SetImageTextureTransience", corpus_calls: 6 },
    Required { name: "SetImageClockAnimation", corpus_calls: 2 },
    Required { name: "SetImageClockCallback", corpus_calls: 2 },
    Required { name: "GetImageClockElapsed", corpus_calls: 2 },
    Required { name: "SetImagePieSliceRender", corpus_calls: 4 },
    Required { name: "DisableImagePieSliceRender", corpus_calls: 4 },
    Required { name: "CreateTextWidget", corpus_calls: 2 },
    Required { name: "SetTextText", corpus_calls: 2 },
    Required { name: "GetTextText", corpus_calls: 2 },
    Required { name: "SetTextFont", corpus_calls: 2 },
    Required { name: "SetTextWrapping", corpus_calls: 4 },
    Required { name: "GetTextWrapping", corpus_calls: 0 },
    Required { name: "GetTextWidth", corpus_calls: 4 },
    Required { name: "GetTextHeight", corpus_calls: 2 },
    Required { name: "SetTextJustification", corpus_calls: 2 },
    Required { name: "GetTextJustification", corpus_calls: 2 },
    Required { name: "SetTextScale", corpus_calls: 2 },
    Required { name: "GetTextScale", corpus_calls: 2 },
    Required { name: "SplitText", corpus_calls: 4 },
    Required { name: "AnimateText", corpus_calls: 4 },
    Required { name: "HaltTextAnimation", corpus_calls: 4 },
    Required { name: "MinimapCreate", corpus_calls: 2 },
    Required { name: "MinimapUpdate", corpus_calls: 2 },
    Required { name: "MinimapSetPlayerLocation", corpus_calls: 2 },
    Required { name: "MinimapSetFocusLocation", corpus_calls: 2 },
    Required { name: "MinimapSetRotation", corpus_calls: 2 },
    Required { name: "MinimapSetRange", corpus_calls: 4 },
    Required { name: "SetMinimapOwner", corpus_calls: 4 },
    Required { name: "SetMinimapBorder", corpus_calls: 4 },
    Required { name: "SetMinimapRadius", corpus_calls: 0 },
    Required { name: "MinimapAddObjective", corpus_calls: 4 },
    Required { name: "MinimapAnimateObjectiveSize", corpus_calls: 4 },
    Required { name: "MinimapAnimateObjectiveAlpha", corpus_calls: 4 },
    Required { name: "MinimapAnimateObjectiveSonar", corpus_calls: 4 },
    Required { name: "MinimapUnanimateObjective", corpus_calls: 4 },
    Required { name: "MinimapRemoveObjective", corpus_calls: 2 },
    Required { name: "MinimapDelete", corpus_calls: 2 },
    Required { name: "SetPlayerPDAWidget", corpus_calls: 8 },
    Required { name: "CreateFlashWidget", corpus_calls: 4 },
    Required { name: "SetFlashSwfFile", corpus_calls: 2 },
    Required { name: "SetFlashPlaySpeed", corpus_calls: 4 },
    Required { name: "GetFlashPlaySpeed", corpus_calls: 4 },
    Required { name: "PauseFlash", corpus_calls: 4 },
    Required { name: "PlayFlash", corpus_calls: 4 },
    Required { name: "RestartFlash", corpus_calls: 4 },
    Required { name: "SendFlashInput", corpus_calls: 9 },
    Required { name: "SendFlashLeftAnalogInput", corpus_calls: 4 },
    Required { name: "SendFlashRightAnalogInput", corpus_calls: 4 },
    Required { name: "SetFlashCallback", corpus_calls: 4 },
    Required { name: "CallFlashScriptFunction", corpus_calls: 2 },
    Required { name: "SetFlashPauseMenu", corpus_calls: 2 },
    Required { name: "SetFlashTesselationAllowed", corpus_calls: 4 },
    Required { name: "RemoveFlashPauseMenu", corpus_calls: 2 },
    Required { name: "CreateSpriteWidget", corpus_calls: 2 },
    Required { name: "SetSpriteTexture", corpus_calls: 2 },
    Required { name: "SetSpriteTextureSize", corpus_calls: 2 },
    Required { name: "SetSpriteFrameSize", corpus_calls: 2 },
    Required { name: "AnimateSprite", corpus_calls: 2 },
    Required { name: "HaltSpriteAnimation", corpus_calls: 2 },
    Required { name: "SetSpriteFrame", corpus_calls: 2 },
    Required { name: "CreateMovieWidget", corpus_calls: 4 },
    Required { name: "SetMovieFile", corpus_calls: 2 },
    Required { name: "PlayMovie", corpus_calls: 2 },
    Required { name: "PauseMovie", corpus_calls: 2 },
    Required { name: "StopMovie", corpus_calls: 2 },
    Required { name: "GetMovieCurrentFrameNumber", corpus_calls: 4 },
    Required { name: "SetMovieEndCallback", corpus_calls: 2 },
    Required { name: "RegisterForPdaUpdate", corpus_calls: 4 },
    Required { name: "RemovePdaBlip", corpus_calls: 6 },
    Required { name: "UpdatePdaBlip", corpus_calls: 4 },
    Required { name: "AddPdaMapBlips", corpus_calls: 4 },
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
    // --- PDA blips (mutators) ---
    "RegisterForPdaUpdate",
    "RemovePdaBlip",
    "UpdatePdaBlip",
    "AddPdaMapBlips",
];

/// HUD binding surface, wired to the retained-mode `mercs2_ui::WidgetTree` on the host (via the
/// `EngineHost::hud`/`hud_ref` seam). Every widget/image/text/sprite/movie/flash/minimap node is real
/// scene-graph state: create mints a handle, the mutators write the node's fields, and the getters read
/// them back (`Set*`→`Get*` round-trip). The GFx rasterization of the tree is a separate render pass;
/// the render/callback/animation-only cfuncs (callbacks, interpolation, pie-slice, PDA blips) stay
/// faithful no-ops until that pass + the input/anim seams exist (see burn-down).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    use mercs2_ui::WidgetKind;

    // The retained-callback table, published as app-data so `pump_hud_callbacks` can find it — the same
    // mechanism `bindings::event` uses for `EventManager`.
    let cbs: Cbs = Rc::new(RefCell::new(HudCallbacks::default()));
    lua.set_app_data(cbs.clone());

    // # Why a widget id is `i64` here and not a [`Guid`]
    //
    // Every *other* handle on this surface converted to lightuserdata (see [`crate::guid`]), but the
    // widget id did not, because two shipped call sites say it is a number in retail:
    // `mrxguidialogbox.lua:331-333` does `local nId = _GuiInternal.GetWidgetHighlightId()` and then
    // branches on `if nId ~= 0 then` before comparing `nId` against `oOption.BasicData.uId`; and
    // `mrxguipda.lua:111,151` clears the PDA widget with the literal `_GuiInternal.SetPlayerPDAWidget
    // (oPda:GetOwner(), 0)`. A lightuserdata is truthy and never equal to `0`, so both of those would
    // be wrong. Nothing in the corpus type-checks a widget id as `"userdata"` either — the 114
    // `"userdata"` comparisons are on player/object/viewport handles and texture names.
    //
    // The ids are still used as table keys (`WidgetIdIndex[uId]`, `mrxguibase.lua:407/891/1443`),
    // which integers satisfy. **`GetWidgetChildren` must keep returning the same type it takes**, or
    // that index lookup misses.
    //
    // The **viewport** id likewise stays `i64`, matching `Player.GetViewportId` (`player.rs`), whose
    // `-1` = "not joined" is a value the shipped code reads rather than a miss. `mrxguibase.lua:137`
    // does check `"userdata" ~= type(uViewportId)`, but only past an `if not Net.IsMultiplayer() then
    // return true` early-out, so it is unreachable on a single-player boot — not enough to overrule
    // the `-1` contract. Confirm-live if a multiplayer session is ever brought up.
    //
    // create(kind) → handle; single-value setter on a widget field; getter reading a widget field.
    macro_rules! create {
        ($name:literal, $kind:expr) => {{
            let hh = host.clone();
            b.real($name, lua.create_function(move |_, _: MultiValue| {
                Ok(hh.borrow_mut().hud().map(|t| t.create($kind) as i64).unwrap_or(0))
            })?)?;
        }};
    }
    macro_rules! wset {
        ($name:literal, $t:ty, |$wd:ident, $v:ident| $body:block) => {{
            let hh = host.clone();
            b.real($name, lua.create_function(move |_, (wid, $v): (i64, $t)| {
                if let Some(tree) = hh.borrow_mut().hud() {
                    if let Some($wd) = tree.get_mut(wid as u64) { $body }
                }
                Ok(())
            })?)?;
        }};
    }
    macro_rules! wget {
        ($name:literal, |$wd:ident| $body:expr, $default:expr) => {{
            let hh = host.clone();
            b.real($name, lua.create_function(move |_, wid: Option<i64>| {
                Ok(hh.borrow().hud_ref().and_then(|t| t.get(wid.unwrap_or(0) as u64)).map(|$wd| $body).unwrap_or($default))
            })?)?;
        }};
    }

    // --- widget lifecycle ---
    create!("CreateWidget", WidgetKind::Container);
    create!("CreateImageWidget", WidgetKind::Image);
    create!("CreateTextWidget", WidgetKind::Text);
    create!("CreateSpriteWidget", WidgetKind::Sprite);
    create!("CreateMovieWidget", WidgetKind::Movie);
    create!("CreateFlashWidget", WidgetKind::Flash);
    create!("MinimapCreate", WidgetKind::Minimap);
    for name in ["DeleteWidget", "MinimapDelete"] {
        let hh = host.clone();
        b.real(name, lua.create_function(move |_, wid: Option<i64>| {
            if let Some(t) = hh.borrow_mut().hud() { t.delete(wid.unwrap_or(0) as u64); }
            Ok(())
        })?)?;
    }

    // --- widget transform / state ---
    let hh = host.clone();
    // A widget location is a RECT `[x1, y1, x2, y2]`, and `Widget:GetLocation()` destructures four
    // values back out (`mrxguibase.lua:759`).
    //
    // The bottom-right corner is **optional**: `Widget:SetLocation(x, y, x1, y1)` forwards whatever
    // it was given, and auto-sizing widgets are placed by top-left alone — e.g.
    // `MrxGuiLoadScreen.InitSaveIcon` does `oText:SetLocation(128, 68)`, so args #4/#5 arrive nil.
    // A trailing "resize children" flag is also optional (`mrxguibase.lua:747/756` pass it,
    // `TextWidget:SetLocation` at :949 does not).
    //
    // When the far corner is omitted we collapse it onto the near corner (a zero-size rect at the
    // requested position) rather than leaving zeros, so that `OffsetLocation`'s `nX2 + x` arithmetic
    // stays meaningful. The retail cfunc's own nil handling is unverified — confirm-live.
    fn rect(x1: f32, y1: f32, x2: Option<f32>, y2: Option<f32>) -> [f32; 4] {
        [x1, y1, x2.unwrap_or(x1), y2.unwrap_or(y1)]
    }
    b.real("SetWidgetLocation", lua.create_function(move |_, (wid, x1, y1, x2, y2, _resize): (i64, f32, f32, Option<f32>, Option<f32>, Option<bool>)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid as u64) { w.location = rect(x1, y1, x2, y2); } }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("SetWidgetCorrectedLocation", lua.create_function(move |_, (wid, x1, y1, x2, y2): (i64, f32, f32, Option<f32>, Option<f32>)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid as u64) { w.corrected_location = rect(x1, y1, x2, y2); } }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("SetWidgetColor", lua.create_function(move |_, (wid, r, g, bl, a): (Option<i64>, f32, f32, f32, Option<f32>)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { w.color = [r, g, bl, a.unwrap_or(255.0)]; } }
        Ok(())
    })?)?;
    wset!("SetWidgetVisible", bool, |w, v| { w.visible = v; });
    wset!("SetWidgetIgnoresPause", bool, |w, v| { w.ignores_pause = v; });
    wset!("SetWidgetSleep", bool, |w, v| { w.sleep = v; });
    wset!("SetWidgetHighlightable", bool, |w, v| { w.highlightable = v; });
    wset!("SetWidgetAnchoring", i64, |w, v| { w.anchoring = v as u32; });
    wset!("SetWidgetViewport", i64, |w, v| { w.viewport = v as i32; });
    wset!("SetWidgetFullscreen", bool, |w, v| { w.fullscreen = v; });

    // --- tree / z-order ---
    for (name, front) in [("PushWidgetToFront", true), ("PushWidgetToBack", false)] {
        let hh = host.clone();
        b.real(name, lua.create_function(move |_, wid: Option<i64>| {
            if let Some(t) = hh.borrow_mut().hud() { if front { t.push_to_front(wid.unwrap_or(0) as u64) } else { t.push_to_back(wid.unwrap_or(0) as u64) } }
            Ok(())
        })?)?;
    }
    for name in ["AddWidgetChild", "SetWidgetChild"] {
        let hh = host.clone();
        b.real(name, lua.create_function(move |_, (parent, child): (Option<i64>, i64)| {
            if let Some(t) = hh.borrow_mut().hud() { t.add_child(parent.unwrap_or(0) as u64, child as u64); }
            Ok(())
        })?)?;
    }
    let hh = host.clone();
    b.real("RemoveWidgetChild", lua.create_function(move |_, (parent, child): (Option<i64>, i64)| {
        if let Some(t) = hh.borrow_mut().hud() { t.remove_child(parent.unwrap_or(0) as u64, child as u64); }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("RemoveAllWidgetChildren", lua.create_function(move |_, parent: Option<i64>| {
        if let Some(t) = hh.borrow_mut().hud() { t.remove_all_children(parent.unwrap_or(0) as u64); }
        Ok(())
    })?)?;

    // --- image widget ---
    // Texture name is nil-able: `ImageWidget:SetTexture(TextureName)` (MrxGuiBase) forwards whatever
    // the layout data held, and a layout widget with no texture passes nil. Typing this as a bare
    // `String` made a legitimate "no texture" a hard Lua error and killed the GUI bootstrap that
    // every `MrxUtil` importer drags in — i.e. the whole task framework.
    wset!("SetImageTexture", Option<String>, |w, v| {
        if let Some(i) = w.image.as_mut() { i.texture = v.unwrap_or_default(); }
    });
    wset!("SetImageRotation", f32, |w, v| { if let Some(i) = w.image.as_mut() { i.rotation = v; } });
    wset!("SetImageTiling", bool, |w, v| { if let Some(i) = w.image.as_mut() { i.tiling = v; } });
    let hh = host.clone();
    b.real("SetImageTextureCoordinates", lua.create_function(move |_, (wid, u0, v0, u1, v1): (i64, f32, f32, f32, f32)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid as u64) { if let Some(i) = w.image.as_mut() { i.tex_coords = [u0, v0, u1, v1]; } } }
        Ok(())
    })?)?;

    // --- text widget ---
    wset!("SetTextText", String, |w, v| { if let Some(x) = w.text.as_mut() { x.text = v; } });
    wset!("SetTextFont", String, |w, v| { if let Some(x) = w.text.as_mut() { x.font = v; } });
    wset!("SetTextWrapping", bool, |w, v| { if let Some(x) = w.text.as_mut() { x.wrapping = v; } });
    // Justification arrives as a STRING from the layout data — `TextWidget:SetJustification`
    // (MrxGuiBase) names its parameter `sJustification`, and the Hungarian `s` is the engine telling
    // us the type. Typing it `i64` hard-errored on every text widget in a layout file. Accept either:
    // a number passes straight through, a name maps to its slot.
    wset!("SetTextJustification", mlua::Value, |w, v| {
        if let Some(x) = w.text.as_mut() {
            x.justification = match &v {
                mlua::Value::Integer(n) => *n as u8,
                mlua::Value::Number(n) => *n as u8,
                mlua::Value::String(s) => match s.to_string_lossy().to_ascii_lowercase().as_str() {
                    "center" | "centre" | "middle" => 1,
                    "right" => 2,
                    _ => 0, // "left" and anything unrecognised
                },
                _ => 0,
            };
        }
    });
    wset!("SetTextScale", f32, |w, v| { if let Some(x) = w.text.as_mut() { x.scale = v; } });

    // --- sprite widget ---
    wset!("SetSpriteTexture", String, |w, v| { if let Some(s) = w.sprite.as_mut() { s.texture = v; } });
    wset!("SetSpriteFrame", i64, |w, v| { if let Some(s) = w.sprite.as_mut() { s.frame = v as u32; } });
    let hh = host.clone();
    b.real("SetSpriteTextureSize", lua.create_function(move |_, (wid, x, y): (Option<i64>, f32, f32)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(s) = w.sprite.as_mut() { s.texture_size = [x, y]; } } }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("SetSpriteFrameSize", lua.create_function(move |_, (wid, x, y): (Option<i64>, f32, f32)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(s) = w.sprite.as_mut() { s.frame_size = [x, y]; } } }
        Ok(())
    })?)?;

    // --- movie widget ---
    // `nil` clears the movie — `MrxGuiAttractMode:77` closes the attract screen with
    // `oMovie:SetMovie(nil)` (via `MovieWidget:SetMovie`, `mrxguibase.lua:1387`), so this setter
    // must accept a nil filename and reset to "no movie" rather than erroring.
    wset!("SetMovieFile", Option<String>, |w, v| {
        if let Some(m) = w.movie.as_mut() { m.file = v.unwrap_or_default(); }
    });
    // `MovieWidget:Play(bLoop)` (`mrxguibase.lua:1391`). Starting playback re-arms the end latch, so a
    // widget replayed after its callback already fired can complete again.
    wset!("PlayMovie", Option<bool>, |w, _v| {
        if let Some(m) = w.movie.as_mut() { m.playing = true; m.end_fired = false; }
    });
    wset!("PauseMovie", Option<bool>, |w, _v| { if let Some(m) = w.movie.as_mut() { m.playing = false; } });
    // Stop is an explicit cancel: it must NOT fire the end callback, or a script that stops a movie
    // early would get the same completion signal as one that watched it through.
    wset!("StopMovie", Option<bool>, |w, _v| {
        if let Some(m) = w.movie.as_mut() { m.playing = false; m.frame = 0; m.end_fired = true; }
    });

    // `_GuiInternal.InterpolateWidget(uId, nTime, x1, y1, x2, y2, r, g, b, a, fComplete, tData,
    //  u1, v1, u2, v2, nRotation, nRotationDirection, nElapsedTime)` — `mrxguibase.lua:715/717`.
    //
    // **The single most load-bearing callback in the GUI.** `MrxGuiBase`'s animation queue advances by
    // handing `_HandleAnimationComplete` in as `fComplete` and continuing the chain when the engine
    // calls it back (`mrxguibase.lua:635-720`). Routed through `record_all` — as this was — the closure
    // is stringified to `""` and the queue stalls at its first entry, taking with it every fade, every
    // menu transition, and the cinematic fade-in that starts the intro movie.
    //
    // The UV / rotation / elapsed tail (args 13-19) is accepted and not yet modelled: the widget tree
    // has no rotation-animation channel. Those are inert rather than dropped-and-forgotten, and the
    // geometry + colour + completion contract — everything the scripts actually observe — is real.
    let hh = host.clone();
    let cbs_interp = cbs.clone();
    #[allow(clippy::type_complexity)]
    b.real("InterpolateWidget", lua.create_function(move |_, args: MultiValue| {
        let a: Vec<Value> = args.into_vec();
        let num = |i: usize| -> Option<f32> {
            match a.get(i) {
                Some(Value::Integer(n)) => Some(*n as f32),
                Some(Value::Number(n)) => Some(*n as f32),
                _ => None,
            }
        };
        let Some(wid) = num(0).map(|v| v as u64) else { return Ok(()) };
        let duration = num(1).unwrap_or(0.0);
        let to_location = [num(2), num(3), num(4), num(5)];
        // Channels the caller did not mean to touch arrive as the -4096 sentinel.
        let to_color = [
            num(6).unwrap_or(mercs2_ui::COLOR_UNCHANGED),
            num(7).unwrap_or(mercs2_ui::COLOR_UNCHANGED),
            num(8).unwrap_or(mercs2_ui::COLOR_UNCHANGED),
            num(9).unwrap_or(mercs2_ui::COLOR_UNCHANGED),
        ];
        let f = match a.get(10) {
            Some(Value::Function(f)) => Some(f.clone()),
            _ => None,
        };
        let ctx = super::unpack_ctx(a.get(11).cloned());

        let id = {
            let mut g = hh.borrow_mut();
            match g.hud() {
                Some(tree) => {
                    let id = f.is_some().then(|| tree.mint_callback());
                    tree.interpolate(wid, duration, to_location, to_color, id);
                    id
                }
                None => None,
            }
        };
        if let (Some(id), Some(f)) = (id, f) {
            cbs_interp.borrow_mut().fns.insert(id, (f, ctx));
        }
        Ok(())
    })?)?;

    // `_GuiInternal.SetMovieEndCallback(uId, fCallback, tData)` `0x005BC640` — `mrxguibase.lua:1404`,
    // reached as `oMovie:SetEndCallback(HideSlow, {oWidget})` from `MrxGuiCinematic.ShowMovie:139`.
    //
    // The `Function` and its context table are retained here; `mercs2_ui` holds only the opaque id, the
    // way retail holds a Lua ref rather than a closure. `pump_hud_callbacks` dispatches on completion.
    let hh = host.clone();
    let cbs_movie = cbs.clone();
    b.real("SetMovieEndCallback", lua.create_function(move |_, (wid, f, ctx): (Option<i64>, Option<Function>, Option<Value>)| {
        let id = match (&f, hh.borrow_mut().hud()) {
            (Some(_), Some(tree)) => {
                let id = tree.mint_callback();
                tree.set_movie_end_callback(wid.unwrap_or(0) as u64, Some(id)).then_some(id)
            }
            // A nil callback clears the registration; a host with no widget tree is a no-op.
            (None, Some(tree)) => {
                tree.set_movie_end_callback(wid.unwrap_or(0) as u64, None);
                None
            }
            _ => None,
        };
        if let (Some(id), Some(f)) = (id, f) {
            cbs_movie.borrow_mut().fns.insert(id, (f, super::unpack_ctx(ctx)));
        }
        Ok(())
    })?)?;

    // --- flash widget ---
    wset!("SetFlashSwfFile", String, |w, v| { if let Some(f) = w.flash.as_mut() { f.swf = v; } });
    wset!("SetFlashPlaySpeed", f32, |w, v| { if let Some(f) = w.flash.as_mut() { f.play_speed = v; } });
    wset!("PlayFlash", Option<bool>, |w, _v| { if let Some(f) = w.flash.as_mut() { f.playing = true; } });
    wset!("PauseFlash", Option<bool>, |w, _v| { if let Some(f) = w.flash.as_mut() { f.playing = false; } });
    wset!("RestartFlash", Option<bool>, |w, _v| { if let Some(f) = w.flash.as_mut() { f.playing = true; } });

    // --- minimap ---
    wset!("MinimapSetRotation", f32, |w, v| { if let Some(m) = w.minimap.as_mut() { m.rotation = v; } });
    wset!("MinimapSetRange", f32, |w, v| { if let Some(m) = w.minimap.as_mut() { m.range = v; } });
    wset!("SetMinimapRadius", f32, |w, v| { if let Some(m) = w.minimap.as_mut() { m.radius = v; } });
    // The minimap owner is a **player handle**, not a widget id: `mrxguibase.lua:1419` is
    // `_GuiInternal.SetMinimapOwner(self.BasicData.uId, uGuid)` reached from `Widget:SetOwner(uGuid)`,
    // which gates on `"userdata" ~= type(uGuid)` at :853. An `i64` here raised on every owner set.
    wset!("SetMinimapOwner", Guid, |w, v| { if let Some(m) = w.minimap.as_mut() { m.owner = v.raw(); } });
    let hh = host.clone();
    b.real("MinimapSetPlayerLocation", lua.create_function(move |_, (wid, x, y): (Option<i64>, f32, f32)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(m) = w.minimap.as_mut() { m.player_location = [x, y]; } } }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("MinimapSetFocusLocation", lua.create_function(move |_, (wid, x, y): (Option<i64>, f32, f32)| {
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(m) = w.minimap.as_mut() { m.focus_location = [x, y]; } } }
        Ok(())
    })?)?;
    // `_GuiInternal.MinimapAddObjective(uId, sName, nX, nY, nZ, nR, nG, nB, uGuid, nWidth, nHeight,
    //  sTexture, bSticky, bRotate, bOriented, nSortOrder)` — `mrxguibase.lua:1485` and the identical
    // `:1516`, reached from `MinimapWidget:AddObjective` (:1484) and `:AddObjectiveWithGuid` (:1512),
    // the latter gating on `"userdata" ~= type(uGuid)` at :1513 before it calls.
    //
    // Argument **2 is the objective's name**, and the handle is argument **9**. An earlier signature
    // read arg 2 as the handle, so every objective landed under the same key and the colour channels
    // were read as the position. Objectives are therefore keyed by the engine hash of their name —
    // the same key `MinimapRemoveObjective(uId, sName)` (:1524) removes by.
    let hh = host.clone();
    #[allow(clippy::type_complexity)]
    b.real("MinimapAddObjective", lua.create_function(move |_, (wid, name, x, y, z, _r, _g, _b, _guid, _rest): (Option<i64>, String, f32, f32, Option<f32>, Option<f32>, Option<f32>, Option<f32>, Guid, MultiValue)| {
        let key = mercs2_formats::hash::pandemic_hash_m2(&name) as u64;
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(m) = w.minimap.as_mut() { m.objectives.insert(key, [x, y, z.unwrap_or(0.0)]); } } }
        Ok(())
    })?)?;
    let hh = host.clone();
    b.real("MinimapRemoveObjective", lua.create_function(move |_, (wid, name): (Option<i64>, String)| {
        let key = mercs2_formats::hash::pandemic_hash_m2(&name) as u64;
        if let Some(t) = hh.borrow_mut().hud() { if let Some(w) = t.get_mut(wid.unwrap_or(0) as u64) { if let Some(m) = w.minimap.as_mut() { m.objectives.remove(&key); } } }
        Ok(())
    })?)?;

    // --- getters: read the real widget state (was fixed defaults) ---
    wget!("GetWidgetVisible", |w| w.visible, true);
    wget!("GetWidgetHighlightable", |w| w.highlightable, false);
    wget!("GetWidgetIgnoresPause", |w| w.ignores_pause, false);
    wget!("GetWidgetSleep", |w| w.sleep, false);
    wget!("GetWidgetAnchoring", |w| w.anchoring as i64, 0i64);
    // Four returns, not two — see the `SetWidgetLocation` note above.
    wget!("GetWidgetLocation",
        |w| (w.location[0], w.location[1], w.location[2], w.location[3]),
        (0.0f32, 0.0f32, 0.0f32, 0.0f32));
    wget!("GetWidgetCorrectedLocation",
        |w| (w.corrected_location[0], w.corrected_location[1], w.corrected_location[2], w.corrected_location[3]),
        (0.0f32, 0.0f32, 0.0f32, 0.0f32));
    wget!("GetWidgetColor", |w| (w.color[0], w.color[1], w.color[2], w.color[3]), (255.0f32, 255.0f32, 255.0f32, 255.0f32));
    wget!("GetImageRotation", |w| w.image.as_ref().map(|i| i.rotation).unwrap_or(0.0), 0.0f32);
    wget!("GetImageTextureCoordinates", |w| { let c = w.image.as_ref().map(|i| i.tex_coords).unwrap_or([0.0, 0.0, 1.0, 1.0]); (c[0], c[1], c[2], c[3]) }, (0.0f32, 0.0f32, 1.0f32, 1.0f32));
    wget!("GetTextText", |w| w.text.as_ref().map(|x| x.text.clone()).unwrap_or_default(), String::new());
    wget!("GetTextWrapping", |w| w.text.as_ref().map(|x| x.wrapping).unwrap_or(false), false);
    wget!("GetTextJustification", |w| w.text.as_ref().map(|x| x.justification as i64).unwrap_or(0), 0i64);
    wget!("GetTextScale", |w| w.text.as_ref().map(|x| x.scale).unwrap_or(1.0), 1.0f32);
    // Text metrics: a rough monospace estimate off the real string + scale (renderer refines later).
    wget!("GetTextWidth", |w| w.text.as_ref().map(|x| x.text.chars().count() as f32 * 8.0 * x.scale).unwrap_or(0.0), 0.0f32);
    wget!("GetTextHeight", |w| w.text.as_ref().map(|x| 16.0 * x.scale).unwrap_or(0.0), 0.0f32);
    wget!("GetFlashPlaySpeed", |w| w.flash.as_ref().map(|f| f.play_speed).unwrap_or(1.0), 1.0f32);
    wget!("GetMovieCurrentFrameNumber", |w| w.movie.as_ref().map(|m| m.frame as i64).unwrap_or(0), 0i64);
    let hh = host.clone();
    // Returns (idList, size) — the game destructures `local tIds, nSize = GetWidgetChildren(uId)`.
    b.real("GetWidgetChildren", lua.create_function(move |lua, wid: i64| {
        let kids = hh.borrow().hud_ref().map(|t| t.children(wid as u64)).unwrap_or_default();
        let n = kids.len() as i64;
        let list = lua.create_sequence_from(kids.into_iter().map(|k| k as i64))?;
        Ok((list, n))
    })?)?;
    // Input-picking / viewport-rect getters — no picker/rect model yet → neutral.
    b.real("GetWidgetViewport", lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32, 0.0f32, 0.0f32)))?)?;
    b.real("GetWidgetHighlightId", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetWidgetDownId", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetImageClockElapsed", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;

    // Names newly backed above; everything else in STUB_NAMES stays a faithful no-op (render/callback/
    // animation-only cfuncs with no state to hold — see the module burn-down note).
    const BACKED: &[&str] = &[
        "CreateWidget", "CreateImageWidget", "CreateTextWidget", "CreateSpriteWidget", "CreateMovieWidget",
        "CreateFlashWidget", "MinimapCreate", "DeleteWidget", "MinimapDelete", "SetWidgetLocation",
        "SetWidgetCorrectedLocation", "SetWidgetColor", "SetWidgetVisible", "SetWidgetIgnoresPause",
        "SetWidgetSleep", "SetWidgetHighlightable", "SetWidgetAnchoring", "SetWidgetViewport",
        "SetWidgetFullscreen", "PushWidgetToFront", "PushWidgetToBack", "AddWidgetChild", "SetWidgetChild",
        "RemoveWidgetChild", "RemoveAllWidgetChildren", "SetImageTexture", "SetImageRotation",
        "SetImageTiling", "SetImageTextureCoordinates", "SetTextText", "SetTextFont", "SetTextWrapping",
        "SetTextJustification", "SetTextScale", "SetSpriteTexture", "SetSpriteFrame", "SetSpriteTextureSize",
        "SetSpriteFrameSize", "SetMovieFile", "PlayMovie", "PauseMovie", "StopMovie", "SetFlashSwfFile",
        "SetFlashPlaySpeed", "PlayFlash", "PauseFlash", "RestartFlash", "MinimapSetRotation",
        "MinimapSetRange", "SetMinimapRadius", "SetMinimapOwner", "MinimapSetPlayerLocation",
        "MinimapSetFocusLocation", "MinimapAddObjective", "MinimapRemoveObjective",
    ];
    // The non-backed widget residue (callbacks / interpolation / pie-slice / clock / text+sprite
    // animation / flash VM input / PDA blips) → recorded HUD commands the widget runtime drains.
    let residue: Vec<&'static str> = STUB_NAMES.iter().copied().filter(|n| !BACKED.contains(n)).collect();
    super::record_all(&mut b, lua, host, "Hud", &residue)?;

    let installed = b.install_global(GLOBAL)?;
    // `_GuiInternal` is the internal alias for this same widget table (`MrxGuiBase` drives the HUD
    // through it — identical method set). Bind the alias to the installed table. `nVersion` marks the
    // newer engine that handles widget-tree recursion (child visibility etc.) natively, so `MrxGuiBase`
    // skips its Lua child-walk fallbacks (the final PC build sets it).
    if let Ok(hud) = lua.globals().get::<mlua::Table>(GLOBAL) {
        let _ = hud.set("nVersion", 1i64);
        let _ = lua.globals().set("_GuiInternal", hud);
    }
    Ok(installed)
}
