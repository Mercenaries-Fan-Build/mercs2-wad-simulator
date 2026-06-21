//! Post-conversion validation for LE block files.
//!
//! Checks entry table integrity, CSUM, descriptor bounds, float sanity
//! (NaN/Inf in STRM/BNDS), world envelope on BNDS, and IBUF index bounds.

use mercs2_formats::chunk_validate::{
    self, validate_deps_body, validate_fxdict_chunks, validate_watr_payload,
};
use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{read_f32_le, read_u16_le, read_u32_le};
use mercs2_formats::tags::ChunkTag;
use mercs2_formats::types;

#[derive(Debug)]
pub enum ValidationError {
    CsumMismatch {
        entry_idx: usize,
        expected: u32,
        actual: u32,
    },
    EntryTableOverflow {
        entry_idx: usize,
        detail: String,
    },
    DescriptorOutOfBounds {
        entry_idx: usize,
        desc_idx: usize,
    },
    FloatNanInf {
        entry_idx: usize,
        tag: String,
        offset: usize,
    },
    FloatOutOfWorld {
        entry_idx: usize,
        tag: String,
        offset: usize,
        value: f32,
    },
    IndexOutOfBounds {
        entry_idx: usize,
        index_value: u16,
        vertex_count: u16,
    },
    InvalidTag {
        entry_idx: usize,
        desc_idx: usize,
        tag_bytes: [u8; 4],
    },
    ChunkFormat {
        entry_idx: usize,
        tag: String,
        detail: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CsumMismatch { entry_idx, expected, actual } => {
                write!(f, "entry[{entry_idx}]: CSUM mismatch expected=0x{expected:08X} actual=0x{actual:08X}")
            }
            Self::EntryTableOverflow { entry_idx, detail } => {
                write!(f, "entry[{entry_idx}]: entry table overflow: {detail}")
            }
            Self::DescriptorOutOfBounds { entry_idx, desc_idx } => {
                write!(f, "entry[{entry_idx}] desc[{desc_idx}]: body extends past container")
            }
            Self::FloatNanInf { entry_idx, tag, offset } => {
                write!(f, "entry[{entry_idx}] {tag}: NaN/Inf float at offset 0x{offset:X}")
            }
            Self::FloatOutOfWorld { entry_idx, tag, offset, value } => {
                write!(f, "entry[{entry_idx}] {tag}: out-of-world float {value} at offset 0x{offset:X}")
            }
            Self::IndexOutOfBounds { entry_idx, index_value, vertex_count } => {
                write!(f, "entry[{entry_idx}] IBUF: index {index_value} >= vertex_count {vertex_count}")
            }
            Self::InvalidTag { entry_idx, desc_idx, tag_bytes } => {
                write!(f, "entry[{entry_idx}] desc[{desc_idx}]: invalid tag bytes {:?}", tag_bytes)
            }
            Self::ChunkFormat { entry_idx, tag, detail } => {
                write!(f, "entry[{entry_idx}] {tag}: {detail}")
            }
        }
    }
}

struct DescInfo {
    tag: ChunkTag,
    tag_bytes: [u8; 4],
    body_start: usize,
    body_size: usize,
}

/// Container types that don't have a standard descriptor table.
const SKIP_DESCRIPTOR_VALIDATION: &[u32] = &[
    types::TYPE_HASH_ANIMATION,
    types::TYPE_HASH_TEXTURE,
];

/// Validate a fully-converted LE block file.
///
/// Returns a list of validation errors found. An empty list means the block
/// passed all checks.
pub fn validate_converted_block(data: &[u8]) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if data.len() < 4 {
        errors.push(ValidationError::EntryTableOverflow {
            entry_idx: 0,
            detail: format!("block too small ({} bytes)", data.len()),
        });
        return errors;
    }

    let entry_count = read_u32_le(data, 0) as usize;
    let header_size = 4 + entry_count * 16;

    if header_size > data.len() {
        errors.push(ValidationError::EntryTableOverflow {
            entry_idx: 0,
            detail: format!(
                "entry table ({} entries, {} bytes) exceeds block size {}",
                entry_count, header_size, data.len()
            ),
        });
        return errors;
    }

    // Parse entry table
    struct Entry {
        type_hash: u32,
        chunk_size: u32,
    }
    let mut entries = Vec::with_capacity(entry_count);
    for i in 0..entry_count {
        let base = 4 + i * 16;
        entries.push(Entry {
            type_hash: read_u32_le(data, base + 4),
            chunk_size: read_u32_le(data, base + 12),
        });
    }

    // Walk containers and validate each one
    let mut pos = header_size;
    for (ei, entry) in entries.iter().enumerate() {
        let chunk_size = entry.chunk_size as usize;

        if pos.saturating_add(chunk_size) > data.len() {
            errors.push(ValidationError::EntryTableOverflow {
                entry_idx: ei,
                detail: format!(
                    "chunk_size {} at offset 0x{:X} exceeds block len {}",
                    chunk_size, pos, data.len()
                ),
            });
            break;
        }

        let container = &data[pos..pos + chunk_size];
        pos += chunk_size;

        // Expect a CSUM trailer after the container
        if pos + 8 <= data.len() {
            let trailer_tag = &data[pos..pos + 4];
            if trailer_tag == b"CSUM" {
                let expected_crc = read_u32_le(data, pos + 4);
                let actual_crc = crc32_mercs2(container);
                if expected_crc != actual_crc {
                    errors.push(ValidationError::CsumMismatch {
                        entry_idx: ei,
                        expected: expected_crc,
                        actual: actual_crc,
                    });
                }
                pos += 8;
            }
        }

        // Validate container internals
        if container.len() >= 20 && &container[0..4] == b"UCFX" {
            if !SKIP_DESCRIPTOR_VALIDATION.contains(&entry.type_hash) {
                validate_container(container, ei, &mut errors);
            }
        }
    }

    errors
}

/// Validate a single LE UCFX container's descriptor table, float data,
/// and index buffers.
fn validate_container(container: &[u8], entry_idx: usize, errors: &mut Vec<ValidationError>) {
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;

    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return;
    }

    // First pass: collect descriptor info and validate bounds
    let mut descs = Vec::with_capacity(n_desc);
    let mut vertex_count: Option<u16> = None;

    for di in 0..n_desc {
        let row_off = 20 + di * 20;
        if row_off + 20 > container.len() {
            break;
        }

        let mut tag_bytes = [0u8; 4];
        tag_bytes.copy_from_slice(&container[row_off..row_off + 4]);
        let tag = ChunkTag::from_bytes(tag_bytes);

        // Validate tag bytes are printable ASCII or a known binary tag
        if matches!(tag, ChunkTag::Unknown(_)) && !is_plausible_tag(tag_bytes) {
            errors.push(ValidationError::InvalidTag {
                entry_idx,
                desc_idx: di,
                tag_bytes,
            });
        }

        let row_u0 = read_u32_le(container, row_off + 4);
        let body_size = read_u32_le(container, row_off + 8) as usize;

        if row_u0 == 0xFFFF_FFFF {
            descs.push(None);
            continue;
        }

        let body_start = if data_area_off > 0 {
            data_area_off + row_u0 as usize
        } else {
            8 + row_u0 as usize
        };

        if body_start.saturating_add(body_size) > container.len() {
            errors.push(ValidationError::DescriptorOutOfBounds {
                entry_idx,
                desc_idx: di,
            });
            descs.push(None);
            continue;
        }

        descs.push(Some(DescInfo {
            tag,
            tag_bytes,
            body_start,
            body_size,
        }));
    }

    // Second pass: float checks and IBUF/STRM analysis
    // Discover vertex count from STRM bodies for IBUF validation.
    // STRM groups: the first STRM after a STRM group sentinel often has the
    // position data. We try to infer vertex count from any STRM by looking at
    // the DECL that precedes it, but as a simpler heuristic, we use the first
    // STRM body size / 12 (3 floats per vertex) as a rough upper bound.
    for desc in &descs {
        let Some(d) = desc else { continue };
        if d.tag == ChunkTag::Strm && d.body_size >= 12 && vertex_count.is_none() {
            let candidate = (d.body_size / 12) as u16;
            if candidate > 0 {
                vertex_count = Some(candidate);
            }
        }
    }

    for desc in &descs {
        let Some(d) = desc else { continue };
        let body = &container[d.body_start..d.body_start + d.body_size];

        match d.tag {
            ChunkTag::Strm => {
                check_floats_nan_inf(body, entry_idx, &d.tag_bytes, d.body_start, errors);
            }
            ChunkTag::Bnds => {
                check_floats_nan_inf(body, entry_idx, &d.tag_bytes, d.body_start, errors);
                check_bnds_world_envelope(body, entry_idx, d.body_start, errors);
            }
            ChunkTag::Ibuf => {
                if let Some(vc) = vertex_count {
                    check_ibuf_bounds(body, entry_idx, vc, errors);
                }
            }
            ChunkTag::Deps => {
                if let Some(msg) = validate_deps_body(body) {
                    errors.push(ValidationError::ChunkFormat {
                        entry_idx,
                        tag: "DEPS".into(),
                        detail: msg,
                    });
                }
            }
            ChunkTag::Watr => {
                if let Some(msg) = validate_watr_payload(body) {
                    errors.push(ValidationError::ChunkFormat {
                        entry_idx,
                        tag: "watr".into(),
                        detail: msg,
                    });
                }
            }
            ChunkTag::Skin => {
                for msg in chunk_validate::validate_skin_containers(container) {
                    errors.push(ValidationError::ChunkFormat {
                        entry_idx,
                        tag: "SKIN".into(),
                        detail: msg,
                    });
                }
            }
            _ => {}
        }
    }

    // fxdict INFO+DICT (resident singleton)
    if let (Some(info), Some(dict)) = (
        find_chunk_body(container, &descs, ChunkTag::InfoUpper)
            .or_else(|| find_chunk_body(container, &descs, ChunkTag::Info)),
        find_chunk_body(container, &descs, ChunkTag::Dict),
    ) {
        if let Some(msg) = validate_fxdict_chunks(info, dict) {
            errors.push(ValidationError::ChunkFormat {
                entry_idx,
                tag: "fxdict".into(),
                detail: msg,
            });
        }
    }
}

fn find_chunk_body<'a>(
    container: &'a [u8],
    descs: &[Option<DescInfo>],
    want: ChunkTag,
) -> Option<&'a [u8]> {
    for desc in descs {
        let d = desc.as_ref()?;
        if d.tag == want {
            return Some(&container[d.body_start..d.body_start + d.body_size]);
        }
    }
    None
}

/// Check if a tag's bytes are plausible (all printable ASCII).
fn is_plausible_tag(tag: [u8; 4]) -> bool {
    tag.iter().all(|&b| b.is_ascii_graphic() || b == b' ')
}

/// Scan all aligned f32 values in a body for NaN/Inf.
fn check_floats_nan_inf(
    body: &[u8],
    entry_idx: usize,
    tag_bytes: &[u8; 4],
    body_abs_start: usize,
    errors: &mut Vec<ValidationError>,
) {
    let tag_str = tag_display(tag_bytes);
    let n_floats = body.len() / 4;
    for i in 0..n_floats {
        let off = i * 4;
        let v = read_f32_le(body, off);
        if v.is_nan() || v.is_infinite() {
            errors.push(ValidationError::FloatNanInf {
                entry_idx,
                tag: tag_str.clone(),
                offset: body_abs_start + off,
            });
        }
    }
}

/// Validate BNDS floats fall within the world envelope.
/// BNDS is always world-space: X/Z in ±4000, Y in -200..+500.
fn check_bnds_world_envelope(
    body: &[u8],
    entry_idx: usize,
    body_abs_start: usize,
    errors: &mut Vec<ValidationError>,
) {
    if body.len() < 24 {
        return;
    }
    // BNDS typically contains min(x,y,z) then max(x,y,z) as 6 floats
    let coords = [
        ("min_x", 0),
        ("min_y", 4),
        ("min_z", 8),
        ("max_x", 12),
        ("max_y", 16),
        ("max_z", 20),
    ];

    for &(_label, off) in &coords {
        if off + 4 > body.len() {
            break;
        }
        let v = read_f32_le(body, off);
        if v.is_nan() || v.is_infinite() {
            continue; // already caught by NaN/Inf check
        }
        let is_y = off == 4 || off == 16;
        let in_range = if is_y {
            (-200.0..=500.0).contains(&v)
        } else {
            (-4000.0..=4000.0).contains(&v)
        };
        if !in_range {
            errors.push(ValidationError::FloatOutOfWorld {
                entry_idx,
                tag: "BNDS".to_string(),
                offset: body_abs_start + off,
                value: v,
            });
        }
    }
}

/// Validate that all u16 index values in an IBUF body are < vertex_count.
fn check_ibuf_bounds(
    body: &[u8],
    entry_idx: usize,
    vertex_count: u16,
    errors: &mut Vec<ValidationError>,
) {
    let n_indices = body.len() / 2;
    for i in 0..n_indices {
        let idx = read_u16_le(body, i * 2);
        if idx >= vertex_count {
            errors.push(ValidationError::IndexOutOfBounds {
                entry_idx,
                index_value: idx,
                vertex_count,
            });
            // Cap to avoid flooding on a badly converted buffer
            if errors.len() > 1000 {
                return;
            }
        }
    }
}

fn tag_display(tag_bytes: &[u8; 4]) -> String {
    tag_bytes.iter().map(|&b| {
        if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::CsumMismatch {
            entry_idx: 5,
            expected: 0x12345678,
            actual: 0x9ABCDEF0,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("entry[5]"));
        assert!(msg.contains("CSUM"));
    }

    #[test]
    fn test_entry_table_overflow_error() {
        let err = ValidationError::EntryTableOverflow {
            entry_idx: 0,
            detail: "block too small (10 bytes)".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("entry table overflow"));
    }

    #[test]
    fn test_float_nan_inf_error() {
        let err = ValidationError::FloatNanInf {
            entry_idx: 3,
            tag: "STRM".to_string(),
            offset: 0x100,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("NaN/Inf"));
        assert!(msg.contains("entry[3]"));
    }

    #[test]
    fn test_float_out_of_world_error() {
        let err = ValidationError::FloatOutOfWorld {
            entry_idx: 2,
            tag: "BNDS".to_string(),
            offset: 0x24,
            value: 9999.0,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("out-of-world"));
        assert!(msg.contains("9999"));
    }

    #[test]
    fn test_index_out_of_bounds_error() {
        let err = ValidationError::IndexOutOfBounds {
            entry_idx: 1,
            index_value: 1000,
            vertex_count: 500,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("IBUF"));
        assert!(msg.contains("1000"));
        assert!(msg.contains("500"));
    }

    #[test]
    fn test_validate_tiny_block() {
        let tiny = vec![0u8; 2];
        let errors = validate_converted_block(&tiny);
        assert!(!errors.is_empty());
        assert!(matches!(errors[0], ValidationError::EntryTableOverflow { .. }));
    }

    #[test]
    fn test_validate_empty_block_zero_entries() {
        // 4 bytes: entry_count = 0
        let data = vec![0u8; 4];
        // No entries, no data
        let errors = validate_converted_block(&data);
        assert!(errors.is_empty(), "Empty block should be valid");
    }

    #[test]
    fn test_tag_display() {
        let tag = tag_display(&[0x41, 0x42, 0x43, 0xFF]);
        assert_eq!(tag, "ABC?");
    }
}
