//! `mercs2_ui` — GUI / HUD / Scaleform GFx + input extensions.
//!
//! **Silo 15** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 27, 18.
//! **Code map:** `docs/reverse_engineer/scaleform_gfx_class_map.md (+ input_code_map.md)`.
//! **Owned Lua namespace(s):** `Hud`, `Pda`, `Gui`, `Marker`, `_GuiInternal`.
//!
//! Implemented so far:
//! - [`widget`] — the retained-mode **widget tree** behind `Hud.*` (containers/image/text/sprite/movie/
//!   flash/minimap nodes with location/color/visibility/anchoring/children/z-order + per-kind data).
//!   The engine owns this scene-graph state; the GFx rasterization is a separate render pass.
//! - [`marker`] — world-space **HUD markers** behind `Gui._Marker*` (blips/tripwires/discs/3D/objective
//!   markers tracking a world location or a followed GUID).

pub mod marker;
pub mod widget;

pub use marker::{Marker, MarkerKind, MarkerSet};
pub use widget::{
    FlashData, ImageData, MinimapData, MovieData, SpriteData, TextData, Widget, WidgetKind, WidgetTree,
};

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
        assert_eq!(2 + 2, 4);
    }
}
