//! UCFX container parsing and CSUM verification.

use crate::crc32::crc32_mercs2;
use crate::ffcs::read_u32_le;
use crate::safe_slice::{AccessResult, SafeSlice};

#[derive(Debug, Clone)]
pub struct UcfxDescriptor {
    pub tag: [u8; 4],
    pub row_u0: u32,
    pub body_size: u32,
}

#[derive(Debug, Clone)]
pub struct BlockTableEntry {
    pub name_hash: u32,
    pub type_hash: u32,
    pub field_c: u32,
    pub chunk_size: u32,
}

#[derive(Debug)]
pub struct ParsedBlock {
    pub entry_count: u32,
    pub entries: Vec<BlockTableEntry>,
    pub containers: Vec<Vec<u8>>,
}

#[derive(Debug)]
pub struct UcfxWalkIssue {
    pub context: String,
    pub detail: String,
}

pub fn parse_block_entry_table(decompressed: &[u8]) -> (u32, Vec<BlockTableEntry>) {
    if decompressed.len() < 4 {
        return (0, Vec::new());
    }
    let count = read_u32_le(decompressed, 0);
    let mut entries = Vec::new();
    for i in 0..count as usize {
        let base = 4 + i * 16;
        if base + 16 > decompressed.len() {
            break;
        }
        entries.push(BlockTableEntry {
            name_hash: read_u32_le(decompressed, base),
            type_hash: read_u32_le(decompressed, base + 4),
            field_c: read_u32_le(decompressed, base + 8),
            chunk_size: read_u32_le(decompressed, base + 12),
        });
    }
    (count, entries)
}

pub fn walk_decompressed_block(decompressed: &[u8], label: &str) -> (ParsedBlock, Vec<UcfxWalkIssue>) {
    let mut issues = Vec::new();
    let (entry_count, entries) = parse_block_entry_table(decompressed);
    let header_end = 4 + (entry_count as usize) * 16;
    let mut containers = Vec::new();
    let mut pos = header_end;

    for (i, entry) in entries.iter().enumerate() {
        let chunk_size = entry.chunk_size as usize;
        if pos.saturating_add(chunk_size) > decompressed.len() {
            issues.push(UcfxWalkIssue {
                context: format!("{label} entry[{i}]"),
                detail: format!(
                    "chunk_size {chunk_size} at pos 0x{pos:X} exceeds block len {}",
                    decompressed.len()
                ),
            });
            break;
        }
        let container = decompressed[pos..pos + chunk_size].to_vec();
        pos += chunk_size;

        if let Some(csum_issues) = verify_ucfx_container(&container, &format!("{label}/entry[{i}]"), entry.type_hash) {
            issues.extend(csum_issues);
        }

        containers.push(container);
    }

    (
        ParsedBlock {
            entry_count,
            entries,
            containers,
        },
        issues,
    )
}

/// Container types whose internal layout at offset 16 is NOT a descriptor
/// count.  The standard UCFX descriptor table (tag+offset+size rows at +20)
/// does not apply to these; validating them as such produces false positives.
const SKIP_DESCRIPTOR_WALK: &[u32] = &[
    crate::types::TYPE_HASH_ANIMATION,
    crate::types::TYPE_HASH_TEXTURE,
];

/// Verify CSUM and descriptor bounds; return issues.
///
/// `type_hash` from the block entry table controls whether the descriptor
/// walk is performed -- container types with non-standard internal layouts
/// skip it entirely to avoid false positives.
pub fn verify_ucfx_container(container: &[u8], label: &str, type_hash: u32) -> Option<Vec<UcfxWalkIssue>> {
    let mut issues = Vec::new();
    if container.len() < 20 {
        issues.push(UcfxWalkIssue {
            context: label.to_string(),
            detail: format!("container too small ({})", container.len()),
        });
        return Some(issues);
    }
    if &container[0..4] != b"UCFX" {
        issues.push(UcfxWalkIssue {
            context: label.to_string(),
            detail: format!("bad magic {:?}", &container[0..4]),
        });
        return Some(issues);
    }

    // CSUM trailer at end of chunk
    if container.len() >= 8 {
        let tail = &container[container.len() - 8..];
        if &tail[0..4] == b"CSUM" {
            let expected = read_u32_le(tail, 4);
            let body_for_crc = &container[..container.len() - 8];
            let actual = crc32_mercs2(body_for_crc);
            if actual != expected {
                issues.push(UcfxWalkIssue {
                    context: label.to_string(),
                    detail: format!(
                        "CSUM mismatch: expected 0x{expected:08X}, computed 0x{actual:08X}"
                    ),
                });
            }
        }
    }

    if SKIP_DESCRIPTOR_WALK.contains(&type_hash) {
        return if issues.is_empty() { None } else { Some(issues) };
    }

    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return if issues.is_empty() { None } else { Some(issues) };
    }

    for i in 0..n_desc {
        let row_off = 20 + i * 20;
        if row_off + 20 > container.len() {
            issues.push(UcfxWalkIssue {
                context: label.to_string(),
                detail: format!("descriptor[{i}] past container end"),
            });
            break;
        }
        let row_u0 = read_u32_le(container, row_off + 4);
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let row_u0 = row_u0 as usize;
        let body_size = read_u32_le(container, row_off + 8) as usize;
        let body_start = if data_area_off > 0 {
            data_area_off + row_u0
        } else {
            8 + row_u0
        };
        if body_start.saturating_add(body_size) > container.len() {
            let tag = &container[row_off..row_off + 4];
            issues.push(UcfxWalkIssue {
                context: format!("{label} desc[{i}] {:?}", std::str::from_utf8(tag).unwrap_or("????")),
                detail: format!(
                    "body [{body_start:#X}..+{body_size}] exceeds container {}",
                    container.len()
                ),
            });
        }
    }

    if issues.is_empty() {
        None
    } else {
        Some(issues)
    }
}

/// Extract inner chunk body by 4-byte descriptor tag (e.g. GEOM, INFO, BODY).
pub fn extract_chunk_body(container: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let row_off = 20 + i * 20;
        if row_off + 20 > container.len() {
            break;
        }
        if &container[row_off..row_off + 4] != tag {
            continue;
        }
        let row_u0 = read_u32_le(container, row_off + 4) as usize;
        let body_size = read_u32_le(container, row_off + 8) as usize;
        if row_u0 == 0xFFFF_FFFF_usize {
            continue;
        }
        let body_start = if data_area_off > 0 {
            data_area_off + row_u0
        } else {
            8 + row_u0
        };
        let body_end = body_start + body_size;
        if body_end > container.len() {
            return None;
        }
        return Some(container[body_start..body_end].to_vec());
    }
    None
}

pub fn extract_data_chunk(container: &[u8]) -> Option<Vec<u8>> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let row_off = 20 + i * 20;
        if row_off + 20 > container.len() {
            break;
        }
        let tag = &container[row_off..row_off + 4];
        let row_u0 = read_u32_le(container, row_off + 4) as usize;
        let body_size = read_u32_le(container, row_off + 8) as usize;
        if tag == b"data" && row_u0 != 0xFFFF_FFFF_usize {
            let body_start = if data_area_off > 0 {
                data_area_off + row_u0
            } else {
                8 + row_u0
            };
            let body_end = body_start + body_size;
            if body_end > container.len() {
                return None;
            }
            return Some(container[body_start..body_end].to_vec());
        }
    }
    None
}

pub fn get_container_by_type_hash(
    parsed: &ParsedBlock,
    type_hash: u32,
    name_hash: Option<u32>,
) -> Option<Vec<u8>> {
    for (i, entry) in parsed.entries.iter().enumerate() {
        if entry.type_hash != type_hash {
            continue;
        }
        if let Some(nh) = name_hash {
            if entry.name_hash != nh
                && parsed
                    .entries
                    .iter()
                    .any(|e| e.name_hash == nh && e.type_hash == type_hash)
            {
                continue;
            }
        }
        return parsed.containers.get(i).cloned();
    }
    None
}

pub fn extract_data_chunk_safe(container: &SafeSlice) -> AccessResult<SafeSlice> {
    let bytes = container.as_bytes();
    let body = extract_data_chunk(bytes).ok_or_else(|| {
        crate::safe_slice::AccessViolation {
            context: format!("{}:no data chunk", container.label()),
            offset: 0,
            size: 0,
            buffer_len: bytes.len(),
        }
    })?;
    Ok(SafeSlice::new(body, format!("{}::data", container.label())))
}
