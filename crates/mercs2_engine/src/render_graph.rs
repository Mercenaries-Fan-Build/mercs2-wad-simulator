//! Per-viewport scene render graph — the recovered `FUN_00466d40` pass order.
//!
//! The shipped Pandemic engine renders **every** viewport through ONE shared per-viewport scene-pass
//! driver, `FUN_00466d40` (2,679 B), reached per-frame from the `RenderFrameJob`
//! (`LAB_0046a260` hash `0x5f2080d1` → `FUN_0085a9e0` → `FUN_00466d40(view+0x2b94)`). The "Water::Render"
//! and "CollectShadowCasters" labels are two render-actions routed through this same driver — it is a
//! per-viewport render dispatcher, not exclusively either.
//!
//! Anchors (unpacked SecuROM image `mercs2_unpacked.exe`, base `0x400000`):
//! - `docs/reverse_engineer/render_core_code_map.md` §5 (body order table) + §11 (summary:
//!   collect → z/opaque → 4× shadow cascade → reflection → fading-trees → color → water-surface →
//!   mirror → blob-fallback).
//! - `docs/reverse_engineer/water_code_map.md` §2 (wake → occlusion → reflection → main → surface).
//! - `docs/reverse_engineer/shadow_code_map.md` §4 (the `while(i<4)` cascade emit around `FUN_00468ca0`).
//!
//! This module encodes that order as a list of **named nodes** so the engine's frame recording follows
//! the oracle, and so the Band-A silos (reflection / water / decals / sky-as-pass / particles-as-pass)
//! have documented seams to plug into (see [`RenderNode`]).
//!
//! ## Carve note (Wave-0 silo E2)
//! Only a subset of the canonical nodes is realized today; the rest are **seams that render nothing**
//! (NOT faked). The engine's current single-forward realization maps onto the graph like so:
//! - [`PassId::ShadowCascade`] — our single directional shadow-depth pass (one cascade, not four).
//! - [`PassId::Color`] — our combined forward pass: a fullscreen sky draw (engine approximation of the
//!   canonical sky-as-pass) then the opaque geometry, optionally through the HDR + bloom post chain.
//! - [`PassId::TransparentFx`] / [`PassId::Ui`] / [`PassId::Overlay`] — engine-added tail passes that
//!   run AFTER the world graph (they are not part of `FUN_00466d40`; the exe composites HUD via the
//!   Scaleform/GFx layer and particles through the canonical [`PassId::Particles`] seam).
//!
//! Because the not-yet-implemented canonical passes are no-op seams, executing the full list in
//! `SCENE_ORDER` reduces to exactly the engine's prior command sequence (shadow-depth → color → fx →
//! ui → overlay) — a behaviour-preserving carve.

/// One node in the per-viewport scene graph, in `FUN_00466d40` body order (render_core §5).
///
/// The first 13 variants are the canonical driver passes; the trailing three are the engine's added
/// composite passes (transparent FX, 2D UI, external overlay) that run after the world graph.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PassId {
    /// Water wake/height generation into RTs — `FUN_00486390(DAT_00d6af80)` (Water::RenderWakeMap,
    /// Xbox 0x15ed0), gated on the water-visible flag `+0xca4`. First call in the driver.
    WakeMap,
    /// Scene begin / render-target setup — `(**(**(scene+4→+0x7e0))+4)(…)`; the device-side RT bind.
    SceneBegin,
    /// Caster/renderable collect — `FUN_004a6590(param_1+0x430)`: classify + frustum-cull into the
    /// `+0x1000` list (renderable vtbl `+0x40`). Our engine's CPU item snapshot stands in for this.
    Collect,
    /// Shadow-caster collect — the `CollectShadowCasters` gate: `thunk_FUN_024bbd20(4, …)` (phase-4
    /// list) → `FUN_00858150` distance/LOD classify → `FUN_00857c00` frustum cull, gated on
    /// `EnableShadows` (`DAT_00dfc360`) && shadow-suppress==0 (`DAT_0117507f`).
    ShadowCollect,
    /// Water occlusion / clip pass — `FUN_00482fa0(DAT_00d6af80+0xa30)` (Water::RenderOcclusion,
    /// Xbox 0x15ee8): builds the fmt-0x1a `WaterClip` texture.
    Occlusion,
    /// Z / opaque main-list draw — `FUN_00468e40`: LOD-gated renderable vtbl `+0x1c` (`Render`/
    /// `RenderZPass`). The exe runs a depth/opaque main-list pass here, before the shadow cascades.
    ZOpaque,
    /// 4× shadow-cascade emit — `while(i<4){ FUN_008596c0; FUN_00468ca0; FUN_00859790 }` around the
    /// per-object `RenderShadow` vtbl `+0x18`, one emit per cascade into the 1024×4096 atlas
    /// (shadow_code_map §1/§4). Our engine realizes ONE directional cascade.
    ShadowCascade,
    /// Water reflection — `FUN_00486fa0()` → `FUN_004677d0` (Water::RenderReflections, Xbox 0x15f00):
    /// builds mirrored view matrices vs the water plane (`+0xab0/+0xb70`) and re-renders renderables
    /// into the reflection RT.
    Reflection,
    /// Fading-trees / vegetation-fade draw — `FUN_00468bb0(param_1,…)` (RenderFadingTrees, Xbox
    /// 0x16a2c): tree-quality bit `+0x5e4>>4`, per-object fade accumulator `obj[0x74]`, vtbl `+0x14`.
    FadingTrees,
    /// Main color draw — `(**(*(scene+4))+4)(cam+0xae0)` (`PgScene::RenderColor`); underwater-ordered
    /// by `param_1+0x1c0`. Our engine's combined sky + opaque forward pass (+ optional HDR/bloom post).
    Color,
    /// Water surface passes — `FUN_00487540` (main surface) then `FUN_00487dd0` (pass 2:
    /// transparency/foam/underwater), ordered by the camera-underwater flag.
    WaterSurface,
    /// Mirror / sub-scene render — iterate `PTR_PTR_01175a10`, obj vtbl `+0x40` then `+0x14`.
    Mirror,
    /// Blob-shadow fallback — `if(DAT_0117507f!=0){ FUN_00853710(…) }` with the blob VB at
    /// `PTR_PTR_00dfc2fc+0x3e94` (emitted when the shadow atlas is suppressed).
    Blob,

    // --- engine-added composite tail (NOT in FUN_00466d40) ---
    /// Canonical particle system draw (PgFX). Seam for the Band-A particles-as-pass silo; the engine
    /// currently draws billboards through [`PassId::TransparentFx`] instead.
    Particles,
    /// Engine transparent-FX pass: billboard particles + additive glow cards, blended over the final
    /// image with a read-only depth test (our current realization of particle rendering).
    TransparentFx,
    /// Engine 2D UI overlay pass: `crate::ui` quads + bitmap text (tool panels / debug HUD). The exe
    /// composites the HUD through Scaleform/GFx, not this driver.
    Ui,
    /// External overlay hook — the workshop's egui inspector etc. draw here, last, before present.
    Overlay,
}

impl PassId {
    /// The Xbox↔PC anchor for this node (function address / doc section), for logs + doc cross-ref.
    pub fn anchor(self) -> &'static str {
        match self {
            PassId::WakeMap => "FUN_00486390 (Water::RenderWakeMap, Xbox 0x15ed0)",
            PassId::SceneBegin => "scene+0x7e0 vtbl+4 (RT setup) — render_core §5 step 2",
            PassId::Collect => "FUN_004a6590 (renderable collect) — render_core §5 step 3",
            PassId::ShadowCollect => "thunk_FUN_024bbd20(4)+FUN_00858150/FUN_00857c00 (CollectShadowCasters)",
            PassId::Occlusion => "FUN_00482fa0 (Water::RenderOcclusion, Xbox 0x15ee8)",
            PassId::ZOpaque => "FUN_00468e40 vtbl+0x1c (Render/RenderZPass) — render_core §5 step 8",
            PassId::ShadowCascade => "while(i<4) FUN_00468ca0 vtbl+0x18 (RenderShadow) — shadow §4",
            PassId::Reflection => "FUN_00486fa0→FUN_004677d0 (Water::RenderReflections, Xbox 0x15f00)",
            PassId::FadingTrees => "FUN_00468bb0 (RenderFadingTrees, Xbox 0x16a2c)",
            PassId::Color => "scene+4 vtbl+4 (PgScene::RenderColor) — render_core §5 step 12",
            PassId::WaterSurface => "FUN_00487540/FUN_00487dd0 (Water surface pass 1/2)",
            PassId::Mirror => "PTR_PTR_01175a10 iterate, obj vtbl+0x40/+0x14 — render_core §5 step 13",
            PassId::Blob => "FUN_00853710 (BlobShadow fallback) — render_core §5 step 15",
            PassId::Particles => "PgFX particle draw (particle_fx_code_map.md)",
            PassId::TransparentFx => "engine-added: billboard FX + glow cards",
            PassId::Ui => "engine-added: 2D UI overlay (HUD is Scaleform/GFx in the exe)",
            PassId::Overlay => "engine-added: external overlay hook (workshop egui)",
        }
    }

    /// Whether this node is a not-yet-implemented **seam** (renders nothing today). Band-A silos flip
    /// these to real passes; [`is_seam`] false means the engine records real GPU commands for it.
    pub fn is_seam(self) -> bool {
        matches!(
            self,
            PassId::WakeMap
                | PassId::SceneBegin
                | PassId::Collect
                | PassId::ShadowCollect
                | PassId::Occlusion
                | PassId::ZOpaque
                | PassId::Reflection
                | PassId::FadingTrees
                | PassId::WaterSurface
                | PassId::Mirror
                | PassId::Blob
                | PassId::Particles
        )
    }
}

/// The full ordered pass list the engine records each world frame, in `FUN_00466d40` body order
/// followed by the engine's composite tail. Seam nodes (see [`PassId::is_seam`]) record nothing yet,
/// so iterating this list is behaviour-identical to the engine's prior hand-ordered passes.
pub const SCENE_ORDER: &[PassId] = &[
    // --- FUN_00466d40 per-viewport driver body (render_core §5) ---
    PassId::WakeMap,
    PassId::SceneBegin,
    PassId::Collect,
    PassId::ShadowCollect,
    PassId::Occlusion,
    PassId::ZOpaque,
    PassId::ShadowCascade,
    PassId::Reflection,
    PassId::FadingTrees,
    PassId::Color,
    PassId::WaterSurface,
    PassId::Mirror,
    PassId::Blob,
    // --- engine composite tail ---
    PassId::Particles,
    PassId::TransparentFx,
    PassId::Ui,
    PassId::Overlay,
];

/// Frame context handed to a [`RenderNode`] when it records — the shared per-frame GPU handles a
/// Band-A pass needs. This is the seam Band-A silos (reflection / water / decals / sky) plug into:
/// implement [`RenderNode`] and register it against its [`PassId`] slot, recording into `encoder`
/// against the `color` / `depth` targets. Extend this struct (add bind groups, item lists, RT views)
/// as the silos need — it is intentionally minimal today.
pub struct PassCtx<'a> {
    /// The wgpu device (create transient resources against it).
    pub device: &'a wgpu::Device,
    /// The frame queue (stage uniform / instance writes).
    pub queue: &'a wgpu::Queue,
    /// The frame command encoder — begin render passes here.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The color target this node composites into (HDR scene target or swapchain).
    pub color: &'a wgpu::TextureView,
    /// The shared scene depth buffer (load, don't clear, in tail passes).
    pub depth: &'a wgpu::TextureView,
    /// Surface size in pixels `[w, h]`.
    pub size: [u32; 2],
}

/// A pluggable scene pass. **Band-A seam:** the reflection / water / decal / sky / particle silos
/// implement this and register the node against its canonical [`PassId`] slot; the engine executes it
/// in [`SCENE_ORDER`] position. Wave-0 (silo E2) ships the ordering + the seams only — no external
/// `RenderNode` is registered yet, so the trait exists purely as the documented plug-in point.
pub trait RenderNode {
    /// The canonical slot this node fills.
    fn id(&self) -> PassId;
    /// Record this pass's GPU commands into `ctx.encoder`, in `SCENE_ORDER` position.
    fn record(&self, ctx: &mut PassCtx<'_>);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SCENE_ORDER` lists the canonical driver body in the recovered `FUN_00466d40` sequence, before
    /// the engine composite tail — the load-bearing invariant Band-A silos rely on for correct pass
    /// ordering. Guard the two anchor transitions the fidelity bar cares about.
    #[test]
    fn scene_order_follows_fun_00466d40() {
        let pos = |p: PassId| SCENE_ORDER.iter().position(|&x| x == p).unwrap();
        // render_core §5: z/opaque (8) → shadow cascade (9) → reflection (10) → fading-trees (11) →
        // color (12) → water-surface (12b).
        assert!(pos(PassId::ZOpaque) < pos(PassId::ShadowCascade));
        assert!(pos(PassId::ShadowCascade) < pos(PassId::Reflection));
        assert!(pos(PassId::Reflection) < pos(PassId::FadingTrees));
        assert!(pos(PassId::FadingTrees) < pos(PassId::Color));
        assert!(pos(PassId::Color) < pos(PassId::WaterSurface));
        assert!(pos(PassId::WaterSurface) < pos(PassId::Mirror));
        assert!(pos(PassId::Mirror) < pos(PassId::Blob));
        // Engine tail runs after the whole world graph.
        assert!(pos(PassId::Blob) < pos(PassId::TransparentFx));
        assert!(pos(PassId::TransparentFx) < pos(PassId::Ui));
        assert!(pos(PassId::Ui) < pos(PassId::Overlay));
    }

    /// The two currently-realized world passes (shadow depth, color) execute in the same relative
    /// order the engine recorded them by hand — shadow before color — so the carve is byte-identical.
    #[test]
    fn realized_world_passes_keep_prior_order() {
        let pos = |p: PassId| SCENE_ORDER.iter().position(|&x| x == p).unwrap();
        assert!(pos(PassId::ShadowCascade) < pos(PassId::Color));
        assert!(!PassId::ShadowCascade.is_seam());
        assert!(!PassId::Color.is_seam());
        assert!(!PassId::TransparentFx.is_seam());
        assert!(!PassId::Ui.is_seam());
    }
}
