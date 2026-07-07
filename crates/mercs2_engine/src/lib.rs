//! `mercs2_engine` — the reimplementation's engine library surface.
//!
//! Exposes the self-contained engine modules so sibling binaries (`mercs2_game`, `mercs2_probe`)
//! and the engine binary itself share ONE implementation instead of the modules living privately
//! inside `main.rs`:
//! - `wad` — FFCS / `vz.wad` asset access (open, block decompress, ASET/container extraction, texture).
//! - `mesh` — UCFX container → indexed geometry (`Vertex`, `BoneRig`, `build_indexed_from_container`).
//! - `pose` — skeletal palette sampling (depends only on `mesh::BoneRig`).
//! - `render` — wgpu helper glue + shared render types (`LoadedModel`, `ClipAnim`, `TexMap`, …).
//! - `scene` — the multi-entity wgpu `Scene` renderer (ECS + streaming world).
//! - `game_world` — the streaming-world render entry point `run_game_world` + its WAD loaders.
//!
//! The streaming-world render now lives in the library so `mercs2_game` (the game exe) drives it
//! **in-process** via `game_world::run_game_world` instead of shelling out to a separate engine
//! binary. This follows `docs/modernization/pangea_engine_alignment.md` §6.

pub mod diag;
pub mod game_world;
pub mod input;
pub mod mesh;
pub mod particles;
pub mod pose;
pub mod post;
pub mod render;
pub mod render_graph;
pub mod scene;
pub mod ui;
pub mod wad;
pub mod worldutil;
