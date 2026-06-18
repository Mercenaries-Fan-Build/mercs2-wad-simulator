//! Validators for documented UCFX chunk layouts (retail PC, verified in Python tools).

use crate::ffcs::{read_f32_le, read_u32_le};

pub const CONTAINER_SENTINEL: u32 = 0xFFFF_FFFF;

/// Retail `watr` payload size (`watermap_decode.py` / `docs/watermap_format.md`).
pub const WATR_EXPECTED_SIZE: usize = 495_669;
pub const WATR_GRID_DIM: u32 = 257;
pub const WATR_LAYER_COUNT: u32 = 5;
pub const WATR_HEADER_BYTES: usize = 36;
pub const WATR_FOOTER_BYTES: usize = 33_290;

pub const FXDICT_DICT_RECORD_BYTES: usize = 20;

/// DEPS body: `[u8 count][u32 asset_hash × count]`.
pub fn validate_deps_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return Some("DEPS body empty".into());
    }
    let count = body[0] as usize;
    let expected = 1 + count * 4;
    if body.len() < expected {
        return Some(format!(
            "DEPS body {} bytes < expected {} (count={count})",
            body.len(),
            expected
        ));
    }
    if body.len() > expected {
        return Some(format!(
            "DEPS body {} bytes > expected {} (count={count}, {} trailing)",
            body.len(),
            expected,
            body.len() - expected
        ));
    }
    None
}

/// Compute expected `watr` size from header grid dimensions and layer count.
pub fn watr_payload_size_for_grid(grid_w: u32, grid_h: u32, layer_count: u32) -> Option<usize> {
    if grid_w == 0 || grid_h == 0 {
        return None;
    }
    let grid = (grid_w as usize).saturating_mul(grid_h as usize);
    // Retail: layer 0 f32 height + layers 1..3 u8 masks + footer blob.
    if layer_count != WATR_LAYER_COUNT {
        return None;
    }
    Some(
        WATR_HEADER_BYTES
            + grid * 4
            + grid * 3
            + WATR_FOOTER_BYTES,
    )
}

/// Validate `watr` header and total payload size (retail 257×257 → 495_669 B).
pub fn validate_watr_payload(payload: &[u8]) -> Option<String> {
    if payload.len() < WATR_HEADER_BYTES {
        return Some(format!(
            "watr payload too short: {} < header {}",
            payload.len(),
            WATR_HEADER_BYTES
        ));
    }
    let layer_count = read_u32_le(payload, 0);
    let grid_w = read_u32_le(payload, 4);
    let grid_h = read_u32_le(payload, 8);
    let cell_size = read_f32_le(payload, 12);

    if layer_count != WATR_LAYER_COUNT {
        return Some(format!(
            "watr layer_count {layer_count} != expected {WATR_LAYER_COUNT}"
        ));
    }
    if grid_w != WATR_GRID_DIM || grid_h != WATR_GRID_DIM {
        return Some(format!(
            "watr grid {grid_w}×{grid_h} != retail {WATR_GRID_DIM}×{WATR_GRID_DIM}"
        ));
    }
    if (cell_size - 32.0).abs() > 0.01 {
        return Some(format!("watr cell_size_m {cell_size} != 32.0"));
    }

    let expected = watr_payload_size_for_grid(grid_w, grid_h, layer_count)
        .unwrap_or(WATR_EXPECTED_SIZE);
    if payload.len() != expected {
        return Some(format!(
            "watr payload size {} != formula {}",
            payload.len(),
            expected
        ));
    }
    if payload.len() != WATR_EXPECTED_SIZE {
        return Some(format!(
            "watr payload size {} != retail constant {}",
            payload.len(),
            WATR_EXPECTED_SIZE
        ));
    }
    None
}

/// fxdict INFO (`u32 entry_count`) + DICT (`20 × count` bytes).
pub fn validate_fxdict_chunks(info: &[u8], dict: &[u8]) -> Option<String> {
    if info.len() < 4 {
        return Some(format!("fxdict INFO too short: {} bytes", info.len()));
    }
    let entry_count = read_u32_le(info, 0) as usize;
    let expected = entry_count * FXDICT_DICT_RECORD_BYTES;
    if dict.len() < expected {
        return Some(format!(
            "fxdict DICT {} bytes < expected {expected} ({entry_count} × {})",
            dict.len(),
            FXDICT_DICT_RECORD_BYTES
        ));
    }
    if dict.len() != expected {
        return Some(format!(
            "fxdict DICT {} bytes != expected {expected} (trailing {})",
            dict.len(),
            dict.len() - expected
        ));
    }
    None
}

/// SKIN container rows: `u0 == CONTAINER_SENTINEL`, child INFO (4 B hash) + PRMG container.
pub fn validate_skin_containers(container: &[u8]) -> Vec<String> {
    let mut issues = Vec::new();
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return issues;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return issues;
    }

    for di in 0..n_desc {
        let row_off = 20 + di * 20;
        if row_off + 20 > container.len() {
            break;
        }
        if &container[row_off..row_off + 4] != b"SKIN" {
            continue;
        }
        let row_u0 = read_u32_le(container, row_off + 4);
        let n_child = read_u32_le(container, row_off + 16) as usize;
        if row_u0 != CONTAINER_SENTINEL {
            issues.push(format!("SKIN[{di}]: row_u0 0x{row_u0:08X} != container sentinel"));
            continue;
        }
        if n_child < 1 {
            issues.push(format!("SKIN[{di}]: n_children={n_child} < 1"));
            continue;
        }
        if row_off + 20 + n_child * 20 > container.len() {
            issues.push(format!("SKIN[{di}]: child table overflows container"));
            continue;
        }

        let mut has_info = false;
        let mut has_prmg = false;
        for ci in 0..n_child {
            let cpos = row_off + 20 + ci * 20;
            let ctag = &container[cpos..cpos + 4];
            let cu0 = read_u32_le(container, cpos + 4);
            let cu1 = read_u32_le(container, cpos + 8) as usize;
            if ctag == b"INFO" && cu0 != CONTAINER_SENTINEL && cu1 == 4 {
                let body_start = data_area_off + cu0 as usize;
                if body_start + 4 <= container.len() {
                    has_info = true;
                }
            }
            if ctag == b"PRMG" && cu0 == CONTAINER_SENTINEL {
                has_prmg = true;
            }
        }
        if !has_info {
            issues.push(format!("SKIN[{di}]: missing 4-byte INFO hash child"));
        }
        if !has_prmg {
            issues.push(format!("SKIN[{di}]: missing PRMG container child"));
        }
    }
    issues
}
