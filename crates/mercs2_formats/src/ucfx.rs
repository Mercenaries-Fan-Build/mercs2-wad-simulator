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

pub fn walk_decompressed_block(
    decompressed: &[u8],
    label: &str,
) -> (ParsedBlock, Vec<UcfxWalkIssue>) {
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

        if let Some(csum_issues) =
            verify_ucfx_container(&container, &format!("{label}/entry[{i}]"), entry.type_hash)
        {
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
pub fn verify_ucfx_container(
    container: &[u8],
    label: &str,
    type_hash: u32,
) -> Option<Vec<UcfxWalkIssue>> {
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
        return if issues.is_empty() {
            None
        } else {
            Some(issues)
        };
    }

    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return if issues.is_empty() {
            None
        } else {
            Some(issues)
        };
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
                context: format!(
                    "{label} desc[{i}] {:?}",
                    std::str::from_utf8(tag).unwrap_or("????")
                ),
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
    // Reverse walk: the engine's asset map overwrites a re-encountered name hash, so when a block
    // carries the same name hash more than once the LAST entry is the one that resolves in game.
    // (See `resolve_type_hash` — resident_P000_Q3 has both a BINN asset and the real fxdict at
    // 0x86BF6C5B, and only the later fxdict survives.)
    for (i, entry) in parsed.entries.iter().enumerate().rev() {
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
    let body = extract_data_chunk(bytes).ok_or_else(|| crate::safe_slice::AccessViolation {
        context: format!("{}:no data chunk", container.label()),
        offset: 0,
        size: 0,
        buffer_len: bytes.len(),
    })?;
    Ok(SafeSlice::new(body, format!("{}::data", container.label())))
}

/// Wrap opaque bytes as a single-`data`-leaf UCFX container inside a single-entry block.
///
/// This is `gfx::build_cfx_pack_block` with the type hash lifted into a parameter. Its doc records
/// how that one was written — *"The shape is not invented. All 64 `cfx_pack` containers in retail
/// `vz.wad` were measured and every one of them is byte-for-byte this layout"* — and
/// `tests/novel_asset_shape_survey.rs` since ran the same measurement across EVERY ASET type.
/// Seven more types turned out to be the identical shape, including the whole audio stack
/// (`soundbank` 98/98, `sounddb` 58/58, `wavebank` 92/93 bare `data`). So the builder that was
/// written for movies is the builder for all of them, and the only thing that ever varied was the
/// type hash in the entry table.
///
/// ```text
/// UCFX | data_area_off = 40 | 0 | 0 | ndesc = 1
/// desc[0]: "data", off 0, size = payload.len(), 0, 0
/// <payload bytes>
/// CSUM <crc32_mercs2 of everything above>
/// ```
///
/// The payload is copied VERBATIM. Nothing here inspects it — validate before calling, the way
/// `GfxMovie::parse` gates the movie path, because a container whose `data` leaf is not what its
/// type claims still checksums, still walks and still resolves; the loader is the first thing that
/// finds out, and it does not say which asset.
///
/// The one-entry case of [`build_wrapped_entries`].
pub fn build_wrapped_block(name_hash: u32, type_hash: u32, payload: &[u8]) -> Vec<u8> {
    build_wrapped_entries(&[WrappedEntry { name_hash, type_hash, payload }])
}

/// One entry of a block [`build_wrapped_entries`] assembles: the entry row's name and type hash, and
/// the payload its container wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedEntry<'a> {
    pub name_hash: u32,
    pub type_hash: u32,
    pub payload: &'a [u8],
}

/// Wrap each payload as a single-`data`-leaf UCFX container (the shape [`build_wrapped_block`]
/// documents) and lay them out as ONE block:
///
/// ```text
/// u32 count
/// count × { u32 name_hash, u32 type_hash, u32 0, u32 container size }
/// the containers, in row order, back to back
/// ```
///
/// This is the layout [`walk_decompressed_block`] reads. A retail sound bank is one such block of
/// three entries under one name hash (soundbank, sounddb, wavebank).
pub fn build_wrapped_entries(entries: &[WrappedEntry<'_>]) -> Vec<u8> {
    let containers: Vec<Vec<u8>> = entries.iter().map(|e| wrap_data_container(e.payload)).collect();
    let rows = 4 + 16 * entries.len();
    let mut block = Vec::with_capacity(rows + containers.iter().map(Vec::len).sum::<usize>());
    block.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (e, c) in entries.iter().zip(&containers) {
        block.extend_from_slice(&e.name_hash.to_le_bytes());
        block.extend_from_slice(&e.type_hash.to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&(c.len() as u32).to_le_bytes());
    }
    for c in &containers {
        block.extend_from_slice(c);
    }
    block
}

/// `UCFX | 40 | 0 | 0 | 1 | {"data", 0, len, 0, 0} | payload | "CSUM" | crc32_mercs2`.
fn wrap_data_container(payload: &[u8]) -> Vec<u8> {
    const HEADER: u32 = 20;
    const DESC_ROW: u32 = 20;
    let data_area_off = HEADER + DESC_ROW;

    let mut ucfx = Vec::with_capacity(data_area_off as usize + payload.len() + 8);
    ucfx.extend_from_slice(b"UCFX");
    ucfx.extend_from_slice(&data_area_off.to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(&1u32.to_le_bytes()); // one descriptor
    ucfx.extend_from_slice(b"data");
    ucfx.extend_from_slice(&0u32.to_le_bytes()); // body offset, relative to the data area
    ucfx.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(payload);
    let csum = crate::crc32::crc32_mercs2(&ucfx);
    ucfx.extend_from_slice(b"CSUM");
    ucfx.extend_from_slice(&csum.to_le_bytes());
    ucfx
}

// ------------------------------------------------------------------------------------------------
// The UCFX descriptor TREE.
// ------------------------------------------------------------------------------------------------
//
// A UCFX container is a pre-order flattening of a tree:
//
// ```text
// "UCFX" | data_area_off = 20 + 20·n | 0 | 0 | n
// n × row { tag, rel_off, size, x2, x3 }
// <bodies, contiguous in row order, no padding>
// "CSUM" | crc32_mercs2(everything above)
// ```
//
// * `x3` is the row's DESCENDANT count. The loader steps from a row to its next sibling with
//   `idx + x3 + 1` (effect loader, `mercs2_unpacked.exe` decomp around the EFCT child walk), so a
//   row's children are the rows `idx+1 ..= idx+x3`, walked the same way.
// * `x2` is the REVERSE sibling ordinal: the number of siblings that follow the row at its own
//   level (the last child has `x2 = 0`). Measured over every destruction family
//   (`tests/state_machine_roundtrip_survey.rs`) and every retail effect (`tests/effect_retail_roundtrip.rs`).
// * A MARKER row (a pure grouping node) has `rel_off = 0xFFFFFFFF` and `size = 0`.
// * `rel_off` is relative to `data_area_off`. Bodies follow row order with no gaps.
//
// Both halves are strict: [`parse_ucfx_tree`] rejects anything [`write_ucfx_tree`] would not
// reproduce byte-for-byte, naming the row and the rule it breaks, so a successful parse is a
// guarantee that re-writing is lossless.

/// `rel_off` of a marker row.
pub const UCFX_MARKER_OFFSET: u32 = 0xFFFF_FFFF;
/// Header bytes before the descriptor rows.
pub const UCFX_HEADER_BYTES: usize = 20;
/// Bytes per descriptor row.
pub const UCFX_ROW_BYTES: usize = 20;
/// Bytes in the `CSUM` trailer.
pub const UCFX_CSUM_BYTES: usize = 8;

/// One node of a UCFX descriptor tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UcfxNode {
    pub tag: [u8; 4],
    /// `None` for a marker row (`rel_off = 0xFFFFFFFF`, `size = 0`); `Some` for a row that owns
    /// bytes in the data area (which may be empty).
    pub body: Option<Vec<u8>>,
    pub children: Vec<UcfxNode>,
}

impl UcfxNode {
    /// A node that owns a body.
    pub fn leaf(tag: [u8; 4], body: Vec<u8>) -> Self {
        UcfxNode { tag, body: Some(body), children: Vec::new() }
    }
    /// A node that owns a body and has children.
    pub fn with_children(tag: [u8; 4], body: Vec<u8>, children: Vec<UcfxNode>) -> Self {
        UcfxNode { tag, body: Some(body), children }
    }
    /// A marker (grouping) node.
    pub fn marker(tag: [u8; 4], children: Vec<UcfxNode>) -> Self {
        UcfxNode { tag, body: None, children }
    }
    /// This node plus every descendant.
    pub fn row_count(&self) -> usize {
        1 + self.children.iter().map(UcfxNode::row_count).sum::<usize>()
    }
    /// The tag as text, for messages.
    pub fn tag_str(&self) -> String {
        String::from_utf8_lossy(&self.tag).into_owned()
    }
}

/// One descriptor row exactly as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UcfxRow {
    pub tag: [u8; 4],
    pub rel_off: u32,
    pub size: u32,
    /// Reverse sibling ordinal.
    pub x2: u32,
    /// Descendant count.
    pub x3: u32,
}

/// Read the descriptor rows of a UCFX container without interpreting them.
pub fn read_ucfx_rows(container: &[u8]) -> Result<Vec<UcfxRow>, String> {
    if container.len() < UCFX_HEADER_BYTES {
        return Err(format!("UCFX container too small ({} bytes)", container.len()));
    }
    if &container[0..4] != b"UCFX" {
        return Err(format!("bad UCFX magic {:02X?}", &container[0..4]));
    }
    let n = read_u32_le(container, 16) as usize;
    let rows_end = n
        .checked_mul(UCFX_ROW_BYTES)
        .and_then(|b| b.checked_add(UCFX_HEADER_BYTES))
        .ok_or_else(|| format!("UCFX row count {n} overflows"))?;
    if rows_end > container.len() {
        return Err(format!(
            "UCFX declares {n} rows ({rows_end} bytes) but the container is {} bytes",
            container.len()
        ));
    }
    Ok((0..n)
        .map(|i| {
            let o = UCFX_HEADER_BYTES + i * UCFX_ROW_BYTES;
            UcfxRow {
                tag: [container[o], container[o + 1], container[o + 2], container[o + 3]],
                rel_off: read_u32_le(container, o + 4),
                size: read_u32_le(container, o + 8),
                x2: read_u32_le(container, o + 12),
                x3: read_u32_le(container, o + 16),
            }
        })
        .collect())
}

/// Parse a UCFX container into its descriptor forest (the top-level rows and their subtrees).
///
/// Strict: the header words, `data_area_off`, every `x2`/`x3`, marker rows, body contiguity and the
/// `CSUM` trailer must all be exactly what [`write_ucfx_tree`] produces. Anything else is an error
/// naming the row and the rule, never a best-effort tree.
pub fn parse_ucfx_tree(container: &[u8]) -> Result<Vec<UcfxNode>, String> {
    let rows = read_ucfx_rows(container)?;
    let n = rows.len();
    let dao = read_u32_le(container, 4) as usize;
    let expect_dao = UCFX_HEADER_BYTES + n * UCFX_ROW_BYTES;
    if dao != expect_dao {
        return Err(format!("UCFX data_area_off {dao} != 20 + 20·{n} = {expect_dao}"));
    }
    for (o, name) in [(8usize, "+8"), (12, "+12")] {
        let w = read_u32_le(container, o);
        if w != 0 {
            return Err(format!("UCFX header word {name} is 0x{w:08X}, not 0"));
        }
    }
    if container.len() < dao + UCFX_CSUM_BYTES {
        return Err(format!("UCFX container {} bytes has no room for a CSUM trailer", container.len()));
    }
    let csum_at = container.len() - UCFX_CSUM_BYTES;
    if &container[csum_at..csum_at + 4] != b"CSUM" {
        return Err("UCFX container does not end in a CSUM trailer".into());
    }
    let stored = read_u32_le(container, csum_at + 4);
    let actual = crc32_mercs2(&container[..csum_at]);
    if stored != actual {
        return Err(format!("UCFX CSUM 0x{stored:08X} != computed 0x{actual:08X}"));
    }
    let data = &container[dao..csum_at];

    let mut cursor = 0usize; // next expected body offset (contiguity)
    let mut idx = 0usize;
    let mut roots = Vec::new();
    let top = count_siblings(&rows, 0, n)?;
    while idx < n {
        let node = parse_node(&rows, data, &mut idx, top - 1 - roots.len(), &mut cursor)?;
        roots.push(node);
    }
    if cursor != data.len() {
        return Err(format!(
            "UCFX data area is {} bytes but the bodies cover {cursor} (trailing bytes)",
            data.len()
        ));
    }
    Ok(roots)
}

/// Number of sibling rows in `[start, end)`, walked `idx + x3 + 1`.
fn count_siblings(rows: &[UcfxRow], start: usize, end: usize) -> Result<usize, String> {
    let mut i = start;
    let mut k = 0usize;
    while i < end {
        let step = rows[i].x3 as usize + 1;
        if i + step > end {
            return Err(format!(
                "UCFX row {i} '{}' claims {} descendants, past its parent's end (row {end})",
                String::from_utf8_lossy(&rows[i].tag),
                rows[i].x3
            ));
        }
        i += step;
        k += 1;
    }
    Ok(k)
}

fn parse_node(
    rows: &[UcfxRow],
    data: &[u8],
    idx: &mut usize,
    expect_x2: usize,
    cursor: &mut usize,
) -> Result<UcfxNode, String> {
    let i = *idx;
    let r = rows[i];
    let tag_s = String::from_utf8_lossy(&r.tag).into_owned();
    if r.x2 as usize != expect_x2 {
        return Err(format!(
            "UCFX row {i} '{tag_s}' x2 = {} but {expect_x2} siblings follow it",
            r.x2
        ));
    }
    let body = if r.rel_off == UCFX_MARKER_OFFSET {
        if r.size != 0 {
            return Err(format!("UCFX marker row {i} '{tag_s}' has size {} (must be 0)", r.size));
        }
        None
    } else {
        let off = r.rel_off as usize;
        let size = r.size as usize;
        if off != *cursor {
            return Err(format!(
                "UCFX row {i} '{tag_s}' body at +{off}, but the previous body ended at +{} \
                 (bodies must be contiguous in row order)",
                *cursor
            ));
        }
        let end = off
            .checked_add(size)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| {
                format!("UCFX row {i} '{tag_s}' body +{off}..+{size} exceeds data area {}", data.len())
            })?;
        *cursor = end;
        Some(data[off..end].to_vec())
    };
    let end = i + 1 + r.x3 as usize;
    let kids = count_siblings(rows, i + 1, end)?;
    *idx = i + 1;
    let mut children = Vec::with_capacity(kids);
    while *idx < end {
        let c = parse_node(rows, data, idx, kids - 1 - children.len(), cursor)?;
        children.push(c);
    }
    Ok(UcfxNode { tag: r.tag, body, children })
}

/// Write a descriptor forest as a UCFX container: pre-order rows with computed `x2`/`x3`, marker
/// rows for body-less nodes, bodies contiguous in row order, `CSUM` trailer.
pub fn write_ucfx_tree(roots: &[UcfxNode]) -> Vec<u8> {
    let n: usize = roots.iter().map(UcfxNode::row_count).sum();
    let dao = UCFX_HEADER_BYTES + n * UCFX_ROW_BYTES;
    let mut rows = Vec::with_capacity(n * UCFX_ROW_BYTES);
    let mut data = Vec::new();
    emit_level(roots, &mut rows, &mut data);

    let mut c = Vec::with_capacity(dao + data.len() + UCFX_CSUM_BYTES);
    c.extend_from_slice(b"UCFX");
    for v in [dao as u32, 0, 0, n as u32] {
        c.extend_from_slice(&v.to_le_bytes());
    }
    c.extend_from_slice(&rows);
    c.extend_from_slice(&data);
    let sum = crc32_mercs2(&c);
    c.extend_from_slice(b"CSUM");
    c.extend_from_slice(&sum.to_le_bytes());
    c
}

fn emit_level(level: &[UcfxNode], rows: &mut Vec<u8>, data: &mut Vec<u8>) {
    for (k, node) in level.iter().enumerate() {
        let x2 = (level.len() - 1 - k) as u32;
        let x3 = (node.row_count() - 1) as u32;
        let (rel_off, size) = match &node.body {
            None => (UCFX_MARKER_OFFSET, 0u32),
            Some(b) => {
                let off = data.len() as u32;
                data.extend_from_slice(b);
                (off, b.len() as u32)
            }
        };
        rows.extend_from_slice(&node.tag);
        for v in [rel_off, size, x2, x3] {
            rows.extend_from_slice(&v.to_le_bytes());
        }
        emit_level(&node.children, rows, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_sample() -> Vec<UcfxNode> {
        vec![UcfxNode::with_children(
            *b"ROOT",
            vec![1, 2, 3],
            vec![
                UcfxNode::leaf(*b"LEAF", vec![4]),
                UcfxNode::marker(
                    *b"MARK",
                    vec![UcfxNode::leaf(*b"AAAA", vec![5, 6]), UcfxNode::leaf(*b"BBBB", vec![])],
                ),
                UcfxNode::leaf(*b"LAST", vec![7, 8, 9, 10, 11]),
            ],
        )]
    }

    #[test]
    fn tree_rows_carry_reverse_ordinals_descendant_counts_and_markers() {
        let c = write_ucfx_tree(&tree_sample());
        let rows = read_ucfx_rows(&c).unwrap();
        let got: Vec<(&[u8; 4], u32, u32, u32, u32)> =
            rows.iter().map(|r| (&r.tag, r.rel_off, r.size, r.x2, r.x3)).collect();
        assert_eq!(
            got,
            vec![
                (b"ROOT", 0, 3, 0, 5),
                (b"LEAF", 3, 1, 2, 0),
                (b"MARK", UCFX_MARKER_OFFSET, 0, 1, 2),
                (b"AAAA", 4, 2, 1, 0),
                (b"BBBB", 6, 0, 0, 0),
                (b"LAST", 6, 5, 0, 0),
            ]
        );
        assert_eq!(read_u32_le(&c, 4) as usize, 20 + 20 * 6);
        assert_eq!(read_u32_le(&c, 8), 0);
        assert_eq!(read_u32_le(&c, 12), 0);
        // Bodies are contiguous: 3 + 1 + 2 + 0 + 5 = 11 bytes, then the 8-byte CSUM.
        assert_eq!(c.len(), 20 + 20 * 6 + 11 + 8);
        assert!(verify_ucfx_container(&c, "tree", 0).is_none());
    }

    #[test]
    fn tree_round_trips() {
        let t = tree_sample();
        let c = write_ucfx_tree(&t);
        assert_eq!(parse_ucfx_tree(&c).unwrap(), t);
        assert_eq!(write_ucfx_tree(&parse_ucfx_tree(&c).unwrap()), c);
    }

    #[test]
    fn a_forest_of_top_level_rows_round_trips() {
        let t = vec![UcfxNode::leaf(*b"INFO", vec![1, 0, 0, 0]), UcfxNode::leaf(*b"DICT", vec![9; 20])];
        let c = write_ucfx_tree(&t);
        let rows = read_ucfx_rows(&c).unwrap();
        assert_eq!((rows[0].x2, rows[0].x3, rows[1].x2, rows[1].x3), (1, 0, 0, 0));
        assert_eq!(parse_ucfx_tree(&c).unwrap(), t);
    }

    fn reseal(c: &mut Vec<u8>) {
        let at = c.len() - 8;
        let sum = crc32_mercs2(&c[..at]);
        c[at + 4..].copy_from_slice(&sum.to_le_bytes());
    }

    #[test]
    fn parse_rejects_what_the_writer_would_not_produce() {
        let good = write_ucfx_tree(&tree_sample());
        // Wrong x2 on LEAF (row 1, +12).
        let mut c = good.clone();
        c[20 + 20 + 12] = 0;
        reseal(&mut c);
        assert!(parse_ucfx_tree(&c).unwrap_err().contains("x2"));
        // A gap between bodies: LAST (row 5) moved one byte on.
        let mut c = good.clone();
        c[20 + 5 * 20 + 4] = 7;
        reseal(&mut c);
        assert!(parse_ucfx_tree(&c).unwrap_err().contains("contiguous"));
        // A marker with a size.
        let mut c = good.clone();
        c[20 + 2 * 20 + 8] = 1;
        reseal(&mut c);
        assert!(parse_ucfx_tree(&c).unwrap_err().contains("marker"));
        // A descendant count running past the parent.
        let mut c = good.clone();
        c[20 + 2 * 20 + 16] = 9;
        reseal(&mut c);
        assert!(parse_ucfx_tree(&c).is_err());
        // A bad checksum.
        let mut c = good.clone();
        let last = c.len() - 1;
        c[last] ^= 1;
        assert!(parse_ucfx_tree(&c).unwrap_err().contains("CSUM"));
    }

    fn entry(name_hash: u32, type_hash: u32) -> BlockTableEntry {
        BlockTableEntry { name_hash, type_hash, field_c: 0, chunk_size: 0 }
    }

    /// The engine overwrites a re-encountered name hash, so a lookup must resolve the LAST entry
    /// carrying that hash. resident_P000_Q3 holds a BINN asset and the real fxdict both at
    /// 0x86BF6C5B; the fxdict is loaded second and is the one the game keeps. Resolving the first
    /// handed `consume_fxdict` the BINN asset and falsely flagged the stock fxdict.
    #[test]
    fn get_container_resolves_the_last_entry_for_a_colliding_name_hash() {
        let parsed = ParsedBlock {
            entry_count: 2,
            entries: vec![entry(0x86BF6C5B, 0x424E_4E00), entry(0x86BF6C5B, 0xFA46_D8A8)],
            containers: vec![b"binn".to_vec(), b"fxdict".to_vec()],
        };
        assert_eq!(
            get_container_by_type_hash(&parsed, 0xFA46_D8A8, Some(0x86BF6C5B)).as_deref(),
            Some(&b"fxdict"[..]),
        );
    }

    /// Two entries sharing BOTH name and type: last-wins is what the reverse walk guarantees
    /// (a forward walk would return the earlier one).
    #[test]
    fn get_container_last_wins_when_name_and_type_both_repeat() {
        let parsed = ParsedBlock {
            entry_count: 2,
            entries: vec![entry(0xAAAA_AAAA, 0x1111_1111), entry(0xAAAA_AAAA, 0x1111_1111)],
            containers: vec![b"first".to_vec(), b"last".to_vec()],
        };
        assert_eq!(
            get_container_by_type_hash(&parsed, 0x1111_1111, Some(0xAAAA_AAAA)).as_deref(),
            Some(&b"last"[..]),
        );
    }

    /// Three entries: the rows come first, then the containers in row order; the walker reads each
    /// back under its own row with a clean CSUM, and each container is the one-entry wrapping of its
    /// payload.
    #[test]
    fn wrapped_entries_lay_out_rows_then_containers() {
        let payloads: [&[u8]; 3] = [b"soundbank body", b"sounddb", b"wavebank body bytes"];
        let types = [0x9F8B_CA10, 0xE527_3C14, 0xF753_F6D0];
        let entries: Vec<WrappedEntry<'_>> = payloads
            .iter()
            .zip(types)
            .map(|(p, t)| WrappedEntry { name_hash: 0x1234_5678, type_hash: t, payload: p })
            .collect();
        let block = build_wrapped_entries(&entries);

        let (parsed, issues) = walk_decompressed_block(&block, "three");
        assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.detail).collect::<Vec<_>>());
        assert_eq!(parsed.entry_count, 3);
        let mut expected_len = 4 + 16 * 3;
        for (i, e) in parsed.entries.iter().enumerate() {
            assert_eq!(e.name_hash, 0x1234_5678);
            assert_eq!(e.type_hash, types[i]);
            assert_eq!(e.field_c, 0);
            let single = build_wrapped_block(0x1234_5678, types[i], payloads[i]);
            assert_eq!(parsed.containers[i], single[20..], "entry {i} container");
            assert_eq!(extract_data_chunk(&parsed.containers[i]).as_deref(), Some(payloads[i]));
            expected_len += e.chunk_size as usize;
        }
        assert_eq!(block.len(), expected_len, "nothing follows the last container");
    }

    /// The one-entry block is exactly `build_wrapped_block`'s bytes.
    #[test]
    fn a_one_entry_block_is_the_wrapped_block() {
        let one = build_wrapped_entries(&[WrappedEntry { name_hash: 7, type_hash: 9, payload: b"abc" }]);
        assert_eq!(one, build_wrapped_block(7, 9, b"abc"));
        assert_eq!(read_u32_le(&one, 0), 1);
        assert_eq!(read_u32_le(&one, 16) as usize, one.len() - 20);
    }
}
