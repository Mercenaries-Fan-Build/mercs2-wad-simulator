//! Per-asset-type consumption dispatch and result aggregation.
//!
//! This module provides the trait-based asset consumer interface used to validate and simulate
//! loading of individual asset types (Models, Textures, Animations, etc.). Each consumer inspects
//! a UCFX container and its data chunks, checking for structural, schema, and data-integrity issues.
//!
//! # Consumer Results
//!
//! Each consumer returns a [`ConsumeResult`] with:
//! - `consumed`: Whether the asset was recognized and processed
//! - `issues`: Human-readable diagnostic messages
//! - `xref_hashes`: Cross-reference asset hashes referenced by this asset
//! - Type-specific validation counters (meshes, textures, placements, etc.)
//! - Categorized violations: structural, ECS float corruption, buffer overflows
//! - Advisory counters: heuristic checks excluded from the verdict
//!
//! # Asset Types Supported
//!
//! - **Model** (TYPE_ID_MODEL): Mesh hierarchies with skinning/rigging
//! - **Texture** (TYPE_ID_TEXTURE): DXT-compressed image data with mip chains
//! - **Animation** (TYPE_ID_ANIMATION): Skeletal animation keyframe data
//! - **Layer** (TYPE_ID_LAYER): Placement and instance data for level geometry
//! - **Script** (TYPE_ID_SCRIPT): Lua/game logic bytecode
//! - **Material** (TYPE_ID_MATERIAL_PARAMS): Shader parameter tables
//! - **Action Table** (TYPE_ID_ACTION_TABLE): Gameplay action dispatcher metadata
//! - **Terrain Mesh** (TYPE_ID_TERRAIN_MESH): Landscape geometry
//! - **Stance** (TYPE_ID_STANCE): Character pose/animation stance data
//! - **FX Dictionary** (TYPE_ID_FX_DICTIONARY): Particle/effect definitions
//! - **Watermap** (TYPE_ID_WATERMAP): Water surface simulation data
//! - **Wavebank** (TYPE_ID_WAVEBANK): Audio waveform container
//! - **Soundbank** (TYPE_ID_SOUNDBANK): Audio event dispatcher
//! - **Wavebank** (TYPE_ID_WAVEBANK): Audio waveform container
//!
//! # Violation Categories
//!
//! - **Structural**: Type mismatch, invalid header, entry count vs. index buffer size mismatch
//! - **ECS Float**: NaN/Inf in float fields or out-of-bounds positions in spatial components
//! - **Texture Buffer**: DXT mip chain exceeds BODY chunk size
//! - **Advisory**: Heuristic stride/offset guesses (STRM, HIER, PRMG, flgs)

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ConsumeResult {
    pub consumed: bool,
    pub issues: Vec<String>,
    pub xref_hashes: Vec<u32>,
    pub placements_validated: usize,
    pub flgs_placements_validated: usize,
    pub meshes_validated: usize,
    pub textures_validated: usize,
    pub vertex_violations: usize,
    pub bounds_violations: usize,
    pub structural_violations: u32,
    /// DIFFERENTIAL-ONLY count of NaN/Inf in schema-typed component float fields.
    /// NOT an absolute defect signal: per the decompilation only an object's
    /// transform position reaches the spatial-hash cell index, and retail itself
    /// carries component-field NaN (e.g. Road ref-data), so this is meaningful only
    /// relative to the retail oracle (tools/diff_ecs_violations.py). Non-fatal.
    pub ecs_float_violations: usize,
    /// The per-record NaN/Inf strings behind `ecs_float_violations`. Routed to the
    /// JSON report for the oracle diff, but kept OUT of the human "UCFX/FORMAT"
    /// display (they false-positive on retail and are differential-only).
    pub ecs_diff_issues: Vec<String>,
    /// FATAL — engine-accurate texture buffer-too-small messages (BODY shorter
    /// than the dimension-derived DXT mip chain). Aggregated into the report's
    /// headline `texture_buffer_too_small` count, NOT into `structural_violations`.
    pub texture_buffer_issues: Vec<String>,
    /// Per-material DIFFUSE texture hash (the first texture hash of each MTRL
    /// material), in material order. Populated by `consume_model` for the DLC
    /// material texture-provenance render-correctness check (see
    /// `simulate.rs`): a patch-origin model whose material diffuse resolves in
    /// the BASE wad's ASET but is NOT shipped by the PATCH wad falls back to wrong
    /// content in menu/wardrobe scenes. Advisory only — not a fatal counter.
    pub material_diffuse_hashes: Vec<u32>,
    // --- Advisory (NON-fatal) counters ---
    // These come from HEURISTIC checks whose offset/stride interpretations are not
    // verified against engine behavior; they fire heavily on WADs that load fine
    // in-game (false positives). Reported for differential analysis but EXCLUDED
    // from the verdict, mirroring `ecs_float_violations`.
    /// STRM vertex NaN/Inf (decl-stride guessing, not engine-verified).
    pub vertex_advisory: usize,
    /// HIER 176-byte node + PRMG 60-byte INFO bbox checks (unverified offsets).
    pub bounds_advisory: usize,
    /// IBUF-vs-heuristic-vertex-count + BNDS-envelope-vs-sampled-vertices.
    pub structural_advisory: u32,
    /// flgs placement records (42-byte stride guess).
    pub position_advisory: usize,
    /// Tags found in this container that the engine dispatches on but which we
    /// have NOT yet validated as WAD chunks (registered-but-unvalidated UCFX tags,
    /// or non-UCFX subsystems like the entity/network/Lua dispatchers). These are
    /// converted with a generic u32 sweep that may be wrong — surfaced (not fatal)
    /// so unverified engine features get a deeper look. See
    /// `mercs2_formats::tag_registry`.
    pub needs_investigation: Vec<String>,
}

pub trait AssetConsumer {
    fn consume(&self, container: &[u8], data_body: Option<&[u8]>, label: &str) -> ConsumeResult;
}

/// Structural-only validation: UCFX already verified in walk; data chunk bounds.
pub struct StructuralConsumer;

impl AssetConsumer for StructuralConsumer {
    fn consume(&self, container: &[u8], data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
        let mut issues = Vec::new();
        if container.len() < 20 || &container[0..4] != b"UCFX" {
            issues.push(format!("{label}: not a UCFX container"));
        }
        if let Some(body) = data_body {
            if body.is_empty() {
                issues.push(format!("{label}: empty data chunk"));
            }
        }
        ConsumeResult {
            consumed: true,
            issues,
            ..Default::default()
        }
    }
}

pub fn consume_structural(container: &[u8], data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    StructuralConsumer.consume(container, data_body, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_result_default_is_all_zeros() {
        let result = ConsumeResult::default();
        assert!(!result.consumed);
        assert!(result.issues.is_empty());
        assert!(result.xref_hashes.is_empty());
        assert_eq!(result.placements_validated, 0);
        assert_eq!(result.meshes_validated, 0);
        assert_eq!(result.structural_violations, 0);
        assert_eq!(result.ecs_float_violations, 0);
    }

    #[test]
    fn structural_consumer_valid_ucfx_container() {
        let mut container = vec![0u8; 100];
        container[0..4].copy_from_slice(b"UCFX");
        let body = vec![1, 2, 3, 4];

        let result = consume_structural(&container, Some(&body), "test_asset");

        assert!(result.consumed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn structural_consumer_invalid_magic() {
        let container = vec![0u8; 100];
        let body = vec![1, 2, 3, 4];

        let result = consume_structural(&container, Some(&body), "test_asset");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert!(result.issues[0].contains("not a UCFX container"));
    }

    #[test]
    fn structural_consumer_empty_data_body() {
        let mut container = vec![0u8; 100];
        container[0..4].copy_from_slice(b"UCFX");
        let body = vec![];

        let result = consume_structural(&container, Some(&body), "test_asset");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert!(result.issues[0].contains("empty data chunk"));
    }

    #[test]
    fn structural_consumer_no_body() {
        let mut container = vec![0u8; 100];
        container[0..4].copy_from_slice(b"UCFX");

        let result = consume_structural(&container, None, "test_asset");

        assert!(result.consumed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn structural_consumer_too_short_container() {
        let container = vec![0u8; 10];
        let body = vec![1, 2, 3];

        let result = consume_structural(&container, Some(&body), "test_asset");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert!(result.issues[0].contains("not a UCFX container"));
    }

    #[test]
    fn structural_consumer_preserves_label() {
        let container = vec![0u8; 20];
        let body = vec![1, 2, 3];

        let result = consume_structural(&container, Some(&body), "my_special_asset");

        assert!(result.consumed);
        assert!(result.issues[0].contains("my_special_asset"));
    }

    #[test]
    fn consume_result_serializes() {
        let result = ConsumeResult {
            consumed: true,
            issues: vec!["issue1".to_string()],
            xref_hashes: vec![0x12345678],
            placements_validated: 5,
            meshes_validated: 3,
            structural_violations: 1,
            ecs_float_violations: 2,
            texture_buffer_issues: vec!["buffer too small".to_string()],
            ..Default::default()
        };

        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("true"));
        assert!(json_str.contains("issue1"));
    }

    #[test]
    fn structural_consumer_both_issues() {
        let container = vec![0u8; 10];
        let body = vec![];

        let result = consume_structural(&container, Some(&body), "bad_asset");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 2);
        assert!(result.issues[0].contains("not a UCFX container"));
        assert!(result.issues[1].contains("empty data chunk"));
    }
}

