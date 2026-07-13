//! `mercs2_ui` — GUI / HUD / Scaleform GFx + input extensions.
//!
//! **Silo 15** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 27, 18.
//! **Code map:** `docs/reverse_engineer/scaleform_gfx_class_map.md (+ input_code_map.md)`.
//! **Owned Lua namespace(s):** `Hud`, `Pda`, `Gui`, `Marker`, `_GuiInternal`.
//!
//! This crate is the **owned GUI state model** — the scene-graph the HUD scripts drive. It does no
//! rendering and no rasterization: the GFx draw pass is separate, and consumes what lives here.
//!
//! # Modules
//! - [`widget`] — the retained-mode **widget tree** behind `Hud.*` (containers/image/text/sprite/movie/
//!   flash/minimap nodes with location/color/visibility/anchoring/children/z-order + per-kind data).
//!   The engine owns this scene-graph state; the GFx rasterization is a separate render pass.
//!   [`WidgetTree::draw_order`] yields the back-to-front handle list a renderer walks.
//! - [`marker`] — world-space **HUD markers** behind `Gui._Marker*` (blips/tripwires/discs/3D/objective
//!   markers tracking a world location or a followed GUID), plus the `_MarkerSetBlipLimit` cap.
//!
//! # Wiring
//! Both models are live, not inert scaffolding: `mercs2_script` binds the `Hud.*` and `Gui._Marker*`
//! Lua surfaces straight onto [`WidgetTree`] / [`MarkerSet`] through the `EngineHost::hud` /
//! `hud_ref` / `markers` / `markers_ref` seam, so `Set*`→`Get*` round-trips for real. `mercs2_engine`
//! owns the instances and re-exports this crate as `mercs2_engine::widgets`. Hosts that return `None`
//! from that seam (the smoke/test hosts) turn the `Hud.*` mutators into no-ops.
//!
//! Not implemented here: the GFx rasterizer, and the silo's input-extension half (row 18).

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
