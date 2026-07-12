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
pub mod mesh;
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
