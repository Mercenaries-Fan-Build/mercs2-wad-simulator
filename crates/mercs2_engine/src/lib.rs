//! `mercs2_engine` — the reimplementation's engine library surface.
//!
//! Exposes the render-agnostic, self-contained engine modules so sibling binaries (`mercs2_game`,
//! `mercs2_probe`) and the engine binary itself share ONE implementation instead of the modules
//! living privately inside `main.rs`:
//! - `wad` — FFCS / `vz.wad` asset access (open, block decompress, ASET/container extraction, texture).
//! - `mesh` — UCFX container → indexed geometry (`Vertex`, `BoneRig`, `build_indexed_from_container`).
//! - `pose` — skeletal palette sampling (depends only on `mesh::BoneRig`).
//!
//! The wgpu render path (`scene`, the frame loop, the render-helper glue like `make_bc_view`) stays
//! in the binary (`main.rs`) for now — it is coupled to bin-local render types. This split follows
//! `docs/modernization/pangea_engine_alignment.md` §6 and the staged crate split it drives (next:
//! peel the diagnostic/export functions into a `diag` module the `mercs2_probe` bin consumes).

pub mod diag;
pub mod mesh;
pub mod pose;
pub mod wad;
pub mod worldutil;
