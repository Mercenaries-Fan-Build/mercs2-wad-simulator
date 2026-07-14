//! `mercs2_engine` — the native 64-bit engine of the Mercenaries 2 reimplementation (Rust/wgpu),
//! running on the retail game's own data (`docs/modernization/00_charter.md`).
//!
//! This is a **pure library**: no binary, no `main`, no argument parsing. The engine is asset-agnostic
//! machinery; the consumers (`mercs2_game` — the game exe — and `mercs2_probe` — the tooling) depend
//! on it and configure it. The streaming-world render lives HERE, in the library, so the game drives it
//! **in-process** via [`game_world::run_game_world`] instead of shelling out to a separate engine
//! binary. Boundary rule, per `docs/modernization/pangea_engine_alignment.md` §6: **mechanism → engine;
//! selection / content / tunables → game.**
//!
//! # Boot paths
//! - [`app::run`] — the single winit event loop. It owns the window, GPU, `Time`, the fixed step, raw
//!   input, the background-load poll and the loading screen; an [`app::Game`] implementor supplies only
//!   policy through the hooks (`config` / `spawn_loader` / `setup` / `update` / `fixed_update` /
//!   `render_prep` / `ui`). Both the dev free-fly boot and the TPS game boot run on it.
//! - [`game_world::run_game_world`] — the streaming world: background WAD loader plus the per-frame
//!   executor that loads/unloads c3 cells + terrain tiles and wakes/hibernates `ModelName` props by
//!   proximity, with a `populate` hook the GAME uses to spawn its own entities once base geometry lands.
//!
//! # Module map
//! Asset layer
//! - [`wad`] — FFCS / `vz.wad` access (open, block decompress, ASET/container extraction, textures).
//! - [`asset`] — `AssetSource`: base WAD + an ordered stack of patch/overlay WADs, resolved last-wins.
//! - [`registry`] — `AssetRegistry`: block residency + hash-keyed chunk tables (insert is FIRST-wins).
//! - [`mesh`] — UCFX container → indexed geometry (`Vertex`, `BoneRig`, `build_indexed_from_container`).
//! - [`model`] — cross-block model assembly over the `<model>_P00N_Q(3-N)` LOD chain.
//! - [`worldutil`] — render-agnostic helpers: `HeightMap`, the streaming decision catalog, reverse-hash.
//!
//! Render
//! - [`render`] — wgpu helper glue + shared types (`LoadedModel`, `ClipAnim`, `TexMap`, `LoadProgress`).
//! - [`render_graph`] — the named-node scene pass order recovered from `FUN_00466d40`.
//! - [`render_state`] — per-object state + the three-clause per-segment draw gate (`FUN_00472a50`).
//! - [`scene`] — the multi-entity `Scene` renderer over the `mercs2_core` ECS `World`.
//! - [`pose`] — skinning-palette recomposition (depends only on `mesh::BoneRig`).
//! - [`post`] — HDR target + bright-pass → bloom → tone-map (fallible; degrades to plain forward).
//! - [`water`] — `WaterNode`, the translucent water-surface render node.
//! - [`particles`] — CPU billboard particle system driven by `fxdict` effect templates.
//! - [`ui`] — 2D overlay pass: screen-space quads + monospace text (`ui.wgsl`).
//!
//! Simulation / game seam
//! - [`camera`] — mode-based camera rig (`CameraMode` → `CameraPreset`) + boom-collision math.
//! - [`player`] — `PlayerController`: third-person locomotion, collide-and-slide, ground snap, clip FSM.
//! - [`input`] — data-driven action/binding layer read from the retail `Mercs2.ini`; KB/mouse/gamepad.
//! - [`script_host`] / [`spawn`] / [`runtime`] / [`gameplay`] — the Lua host + simulation cluster (below).
//! - [`diag`] — headless, render-agnostic diagnostics/exports consumed by `mercs2_probe`.
pub mod app;
pub mod asset;
pub mod camera;
pub mod diag;
// The Lua host + simulation cluster. Lua is a core engine pillar, not just WAD content: the VM binding
// surface (`Pg.*`/`Object.*`/`Player.*`/`Ai.*`/…) marries the scripts to the live engine systems (World,
// physics, audio, AI). `script_host` implements `mercs2_script`'s `EngineHost`; `runtime`/`gameplay`/
// `spawn` are the ECS-driven fleet tick + spawn resolver it drives. The game supplies scripts (WADs) +
// policy (namespace/hero/economy/faction seed) via the constructor/setters.
pub mod gameplay;
pub mod runtime;
pub mod script_host;
pub mod spawn;
pub mod game_world;
pub mod input;
/// Model container → indexed triangle geometry.
///
/// Carved out into its own crate (`mercs2_mesh`) because it is pure decode — no GPU, no
/// windowing — and tools that are not the engine (the modkit GUI's 3D texture viewer) need
/// it without pulling in wgpu/winit. Re-exported here so every `mercs2_engine::mesh::…` and
/// `crate::mesh::…` path keeps working unchanged.
pub use mercs2_mesh as mesh;
pub mod model;
pub mod particles;
pub mod player;
pub mod pose;
pub mod post;
pub mod registry;
pub mod render;
pub mod render_graph;
pub mod render_state;
pub mod scene;
pub mod ui;
pub mod wad;
pub mod water;
pub mod worldutil;

// Mechanism re-exports. The engine OWNS these subsystems; the game reaches them ONLY through the
// engine (`mercs2_engine::physics::…`) so it can depend on the engine alone. Each `pub use … as <name>`
// is a zero-cost alias to the same underlying type — `mercs2_engine::physics::X` *is* `mercs2_physics::X`
// — so a game `use` flip and the eventual direct-dep drop are decoupled and never mismatch mid-flight.
pub use mercs2_ai as ai;
pub use mercs2_anim as anim;
pub use mercs2_audio as audio;
pub use mercs2_combat as combat;
pub use mercs2_decal as decal;
pub use mercs2_faction as faction;
pub use mercs2_physics as physics;
pub use mercs2_population as population;
pub use mercs2_script as script;
pub use mercs2_vehicle as vehicle;
// `water_sim`/`widgets` avoid the name clash with the engine's own render modules `water` (the water
// surface `RenderNode`) and `ui` (the 2D overlay pass): `mercs2_water` is the watermap/swim DATA crate,
// `mercs2_ui` is the retained HUD widget tree the `Hud.*` bindings drive.
pub use mercs2_ui as widgets;
pub use mercs2_water as water_sim;

/// Wave-0 Tier-2 seam guard (seam F, `docs/modernization/wave0_seam_review.md`).
///
/// The `schm` type-code table exists as two **parallel enums** by architectural necessity —
/// `mercs2_formats::schema::SchemaFieldType` (the on-disk/asset side) and
/// `mercs2_core::registry::FieldKind` (the asset-agnostic kernel side, which cannot depend on
/// `mercs2_formats`). They are a hand-kept mirror; this test — living in the one crate that depends on
/// **both** — fails the moment a code or width diverges, so the mirror can never silently drift.
#[cfg(test)]
mod schema_type_code_mirror {
    use mercs2_core::registry::FieldKind;
    use mercs2_formats::schema::SchemaFieldType;

    #[test]
    fn field_kind_mirrors_schema_field_type_for_every_code() {
        // The full schm code space (0..=12 covers every valid code + the gaps 0/3/12).
        for code in 0u32..=12 {
            let asset = SchemaFieldType::from_code(code);
            let kernel = FieldKind::from_type_code(code);
            assert_eq!(
                asset.is_some(),
                kernel.is_some(),
                "code {code}: formats and core disagree on whether it is a valid schm type"
            );
            if let (Some(a), Some(k)) = (asset, kernel) {
                assert_eq!(
                    a.byte_width(),
                    k.byte_width(),
                    "code {code} ({a:?} vs {k:?}): byte-width mismatch between the two mirrors"
                );
            }
        }
    }
}
