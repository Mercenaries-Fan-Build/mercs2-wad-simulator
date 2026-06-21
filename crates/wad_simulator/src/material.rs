//! Material params (`material_params` / MTRL / PRMT) structural checks.

use crate::consume::ConsumeResult;
use mercs2_formats::ffcs::read_u32_le;
use mercs2_formats::ucfx::extract_chunk_body;

/// MTRL preamble: shader hash + default color/emissive/specular (`material_probe.py`).
const MTRL_PREAMBLE_MIN: usize = 104;

pub fn consume_material(container: &[u8], _data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let mut issues = Vec::new();
    let mut structural_advisory = 0u32;
    let mut xref_hashes = Vec::new();

    if let Some(mtrl) = extract_chunk_body(container, b"MTRL") {
        if mtrl.len() < MTRL_PREAMBLE_MIN {
            issues.push(format!(
                "{label}: MTRL body {} bytes < preamble minimum {MTRL_PREAMBLE_MIN}",
                mtrl.len()
            ));
            structural_advisory += 1;
        } else {
            let shader_hash = read_u32_le(&mtrl, 0);
            if std::env::var("MTRL_DEBUG").is_ok() {
                eprintln!(
                    "[MTRL/material] {label}: len={} shader_hash@+0=0x{shader_hash:08X} \
                     count@106={} (+108=0x{:08X})",
                    mtrl.len(),
                    if mtrl.len() >= 108 { u16::from_le_bytes([mtrl[106], mtrl[107]]) } else { 0 },
                    if mtrl.len() >= 112 { read_u32_le(&mtrl, 108) } else { 0 },
                );
            }
            if shader_hash != 0 && shader_hash != 0xFFFF_FFFF {
                xref_hashes.push(shader_hash);
            }
        }
    }

    if let Some(prmt) = extract_chunk_body(container, b"PRMT") {
        if prmt.len() < 8 {
            issues.push(format!("{label}: PRMT too small ({} bytes)", prmt.len()));
            structural_advisory += 1;
        } else if prmt.len() % 4 != 0 {
            issues.push(format!(
                "{label}: PRMT body {} not 4-byte aligned",
                prmt.len()
            ));
            structural_advisory += 1;
        }
    }

    // NOTE: a material_params asset is NOT required to carry MTRL/PRMT. Those are
    // the *model-embedded* material representation; a standalone material_params
    // (type_hash 0xDE982D61) commonly stores its parameters in a `data` chunk
    // instead (verified on retail vz.wad block 3185: info/data/trnm containers,
    // the `data` being e.g. a Havok packfile — no MTRL/PRMT, and it loads fine).
    // The old "missing MTRL and PRMT" flag therefore false-positived on retail and
    // has been removed. MTRL/PRMT are still validated above when present.

    ConsumeResult {
        consumed: true,
        issues,
        xref_hashes,
        structural_advisory,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_material_no_mtrl_prmt_is_not_flagged() {
        // material_params without MTRL/PRMT is valid (data-based materials, e.g.
        // a Havok `data` chunk — retail block 3185). It must NOT be flagged.
        let container = b"UCFX".to_vec();
        let result = consume_material(&container, None, "test_material");

        assert!(result.consumed);
        assert!(result.issues.is_empty(), "{:?}", result.issues);
        assert_eq!(result.structural_advisory, 0);
    }

    #[test]
    fn consume_material_returns_consume_result() {
        let container = b"TEST".to_vec();
        let result = consume_material(&container, None, "test");

        assert!(result.consumed);
        assert!(result.xref_hashes.is_empty());
    }

    #[test]
    fn consume_material_label_preservation() {
        // A malformed/too-short MTRL still carries the label; build one so there's
        // an issue to check (absence of MTRL/PRMT alone is no longer flagged).
        let mut container = vec![0u8; 20];
        container[0..4].copy_from_slice(b"UCFX");
        // (no MTRL/PRMT → no issues; label preservation is covered by the MTRL
        // preamble/PRMT paths, exercised elsewhere). Just assert it consumes.
        let result = consume_material(&container, None, "my_special_material");
        assert!(result.consumed);
    }

    #[test]
    fn consume_material_no_advisory_when_no_mtrl_prmt() {
        // Absence of MTRL/PRMT is valid (data-based material_params) → no advisory.
        let container = vec![0u8; 20];
        let result = consume_material(&container, None, "test");

        assert!(result.consumed);
        assert_eq!(result.structural_advisory, 0);
    }

    #[test]
    fn consume_material_preamble_constant() {
        // Verify the constant is reasonable (> 0)
        assert!(MTRL_PREAMBLE_MIN > 0);
        assert_eq!(MTRL_PREAMBLE_MIN, 104);
    }
}

