//! Per-asset-type consumption dispatch.

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
    /// Schema-driven ECS float-field corruption: NaN/Inf in any float field, or
    /// non-finite/out-of-bounds positions in non-Transform position-bearing
    /// components. Catches byte-swap defects the name-matched Transform heuristic
    /// misses (the spatial-hash garbage-cell-index source).
    pub ecs_float_violations: usize,
    /// FATAL — engine-accurate texture buffer-too-small messages (BODY shorter
    /// than the dimension-derived DXT mip chain). Aggregated into the report's
    /// headline `texture_buffer_too_small` count, NOT into `structural_violations`.
    pub texture_buffer_issues: Vec<String>,
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
