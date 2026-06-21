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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_deps_body_empty() {
        assert!(validate_deps_body(&[]).is_some());
    }

    #[test]
    fn validate_deps_body_zero_count() {
        let body = [0u8]; // count = 0
        assert!(validate_deps_body(&body).is_none());
    }

    #[test]
    fn validate_deps_body_single_hash() {
        let mut body = vec![1u8]; // count = 1
        body.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]); // one hash
        assert!(validate_deps_body(&body).is_none());
    }

    #[test]
    fn validate_deps_body_multiple_hashes() {
        let mut body = vec![3u8]; // count = 3
        body.extend_from_slice(&[0, 0, 0, 0]); // hash 1
        body.extend_from_slice(&[1, 1, 1, 1]); // hash 2
        body.extend_from_slice(&[2, 2, 2, 2]); // hash 3
        assert!(validate_deps_body(&body).is_none());
    }

    #[test]
    fn validate_deps_body_truncated() {
        let mut body = vec![2u8]; // count = 2
        body.extend_from_slice(&[0, 0, 0, 0]); // hash 1
        // Missing hash 2 — should error
        assert!(validate_deps_body(&body).is_some());
    }

    #[test]
    fn validate_deps_body_trailing_data() {
        let mut body = vec![1u8]; // count = 1
        body.extend_from_slice(&[0, 0, 0, 0]); // hash 1
        body.push(0xFF); // trailing byte
        assert!(validate_deps_body(&body).is_some());
    }

    #[test]
    fn watr_payload_size_for_grid_zero_dims() {
        assert!(watr_payload_size_for_grid(0, 257, 5).is_none());
        assert!(watr_payload_size_for_grid(257, 0, 5).is_none());
    }

    #[test]
    fn watr_payload_size_for_grid_wrong_layers() {
        assert!(watr_payload_size_for_grid(257, 257, 4).is_none());
        assert!(watr_payload_size_for_grid(257, 257, 6).is_none());
    }

    #[test]
    fn watr_payload_size_for_grid_retail() {
        let size = watr_payload_size_for_grid(WATR_GRID_DIM, WATR_GRID_DIM, WATR_LAYER_COUNT);
        assert_eq!(size, Some(WATR_EXPECTED_SIZE));
    }

    #[test]
    fn watr_payload_size_for_grid_calculation() {
        // 257x257 grid: 36 header + 257*257*4 (heights) + 257*257*3 (masks) + 33290 footer
        let expected = 36 + 257 * 257 * 4 + 257 * 257 * 3 + 33290;
        assert_eq!(watr_payload_size_for_grid(257, 257, 5), Some(expected));
    }

    #[test]
    fn validate_watr_payload_too_short() {
        let payload = [0u8; 20];
        assert!(validate_watr_payload(&payload).is_some());
    }

    #[test]
    fn validate_watr_payload_wrong_layers() {
        let mut payload = vec![0u8; WATR_HEADER_BYTES];
        // layer_count at offset 0: set to 4 instead of 5
        payload[0] = 4;
        payload[1] = 0;
        payload[2] = 0;
        payload[3] = 0;
        assert!(validate_watr_payload(&payload).is_some());
    }

    #[test]
    fn validate_watr_payload_wrong_grid_dims() {
        let mut payload = vec![0u8; WATR_EXPECTED_SIZE];
        // layer_count = 5
        payload[0] = 5;
        // grid_w = 256 (wrong)
        payload[4] = 0;
        payload[5] = 1;
        payload[6] = 0;
        payload[7] = 0;
        // grid_h = 257
        payload[8] = 1;
        payload[9] = 1;
        payload[10] = 0;
        payload[11] = 0;
        // cell_size = 32.0
        payload[12] = 0;
        payload[13] = 0;
        payload[14] = 0;
        payload[15] = 0x42;
        assert!(validate_watr_payload(&payload).is_some());
    }

    #[test]
    fn validate_fxdict_chunks_info_too_short() {
        assert!(validate_fxdict_chunks(&[], &[]).is_some());
        assert!(validate_fxdict_chunks(&[0, 0], &[]).is_some());
    }

    #[test]
    fn validate_fxdict_chunks_zero_entries() {
        let info = [0, 0, 0, 0]; // entry_count = 0
        let dict = [];
        assert!(validate_fxdict_chunks(&info, &dict).is_none());
    }

    #[test]
    fn validate_fxdict_chunks_single_entry() {
        let info = [1, 0, 0, 0]; // entry_count = 1
        let dict = [0u8; 20]; // exactly 1 * 20 bytes
        assert!(validate_fxdict_chunks(&info, &dict).is_none());
    }

    #[test]
    fn validate_fxdict_chunks_multiple_entries() {
        let info = [3, 0, 0, 0]; // entry_count = 3
        let dict = vec![0u8; 3 * 20]; // exactly 3 * 20 bytes
        assert!(validate_fxdict_chunks(&info, &dict).is_none());
    }

    #[test]
    fn validate_fxdict_chunks_dict_too_short() {
        let info = [3, 0, 0, 0]; // entry_count = 3
        let dict = vec![0u8; 50]; // less than 3 * 20 = 60
        assert!(validate_fxdict_chunks(&info, &dict).is_some());
    }

    #[test]
    fn validate_fxdict_chunks_dict_trailing() {
        let info = [2, 0, 0, 0]; // entry_count = 2
        let dict = vec![0u8; 2 * 20 + 5]; // 2 * 20 + 5 trailing
        assert!(validate_fxdict_chunks(&info, &dict).is_some());
    }

    #[test]
    fn validate_skin_containers_empty() {
        let result = validate_skin_containers(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_skin_containers_too_small() {
        let data = vec![0u8; 10];
        let result = validate_skin_containers(&data);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_skin_containers_bad_magic() {
        let data = vec![0u8; 20];
        let result = validate_skin_containers(&data);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_skin_containers_zero_descriptors() {
        let mut data = vec![0u8; 20];
        data[0..4].copy_from_slice(b"UCFX");
        // n_desc at offset 16 = 0
        let result = validate_skin_containers(&data);
        assert!(result.is_empty());
    }
}
