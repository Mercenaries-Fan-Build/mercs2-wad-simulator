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
pub mod water;
pub mod worldutil;

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
