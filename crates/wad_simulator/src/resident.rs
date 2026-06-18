//! Resident singleton consumers (watermap, fxdict).

use crate::consume::ConsumeResult;
use mercs2_formats::chunk_validate::{validate_fxdict_chunks, validate_watr_payload};
use mercs2_formats::ucfx::extract_chunk_body;

pub fn consume_watermap(container: &[u8], _data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let mut issues = Vec::new();
    let mut structural_violations = 0u32;

    let watr = match extract_chunk_body(container, b"watr") {
        Some(b) => b,
        None => {
            issues.push(format!("{label}: watermap UCFX missing watr chunk"));
            return ConsumeResult {
                consumed: true,
                issues,
                structural_violations: 1,
                ..Default::default()
            };
        }
    };

    if let Some(msg) = validate_watr_payload(&watr) {
        issues.push(format!("{label}: {msg}"));
        structural_violations += 1;
    }

    ConsumeResult {
        consumed: true,
        issues,
        structural_violations,
        ..Default::default()
    }
}

pub fn consume_fxdict(container: &[u8], _data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let mut issues = Vec::new();
    let mut structural_violations = 0u32;

    let info = extract_chunk_body(container, b"INFO")
        .or_else(|| extract_chunk_body(container, b"info"));
    let dict = extract_chunk_body(container, b"DICT");

    match (info, dict) {
        (Some(info_b), Some(dict_b)) => {
            if let Some(msg) = validate_fxdict_chunks(&info_b, &dict_b) {
                issues.push(format!("{label}: {msg}"));
                structural_violations += 1;
            }
        }
        _ => {
            issues.push(format!("{label}: fxdict missing INFO and/or DICT chunk"));
            structural_violations += 1;
        }
    }

    ConsumeResult {
        consumed: true,
        issues,
        structural_violations,
        ..Default::default()
    }
}
