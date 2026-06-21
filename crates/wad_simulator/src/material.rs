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

    if extract_chunk_body(container, b"MTRL").is_none()
        && extract_chunk_body(container, b"PRMT").is_none()
    {
        issues.push(format!("{label}: material_params missing MTRL and PRMT"));
        structural_advisory += 1;
    }

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
    fn consume_material_no_chunks() {
        let container = b"UCFX".to_vec();
        let result = consume_material(&container, None, "test_material");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert!(result.issues[0].contains("missing MTRL and PRMT"));
        assert_eq!(result.structural_advisory, 1);
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
        let container = b"UCFX".to_vec();
        let result = consume_material(&container, None, "my_special_material");

        assert!(result.consumed);
        assert!(result.issues[0].contains("my_special_material"));
    }

    #[test]
    fn consume_material_default_advisory_on_missing() {
        let container = vec![0u8; 20];
        let result = consume_material(&container, None, "test");

        assert!(result.consumed);
        assert_eq!(result.structural_advisory, 1);
    }

    #[test]
    fn consume_material_preamble_constant() {
        // Verify the constant is reasonable (> 0)
        assert!(MTRL_PREAMBLE_MIN > 0);
        assert_eq!(MTRL_PREAMBLE_MIN, 104);
    }
}

