# mercs2_engine

Phase-1 skeleton of the native 64-bit Mercenaries 2 reimplementation
(see `docs/modernization/00_charter.md`).

This is the **render shell**: a `wgpu` (DX12 / Vulkan / Metal) window with a working pipeline. It
currently draws one placeholder triangle to prove the device/surface/pipeline/present loop end to end.

## Run

```bash
cd tools/wad_simulator
cargo run -p mercs2_engine
```

A 1280×720 window opens on a dark background with a tri-colored triangle. `Esc` or close to quit.

## Where this is going (Phase 1 milestones)

- [x] **1a — shell**: wgpu device + surface + render loop + one triangle. *(this)*
- [ ] **1b — first triangle from real WAD data**: pull vertices from a real model block via
  `mercs2_formats`, upload, render. Validated against the original (render-golden, Surface A).
- [ ] **1c — textured mesh**: decode a DXT texture, bind it, draw a real textured model.
- [ ] **1d — camera + depth**: MVP matrix, depth buffer, orbit camera.

The engine consumes the **original game's data** (WADs) through `mercs2_formats` — the exe + decomp
remain the oracle/spec, not the shipping artifact.

## Stack

- `wgpu` 0.20 — DX12/Vulkan/Metal (exceeds the "DX11" target; portable).
- `winit` 0.29 — windowing/input.
- `mercs2_formats` — the asset layer (WAD/model/texture parsers), shared with the modding tools.
