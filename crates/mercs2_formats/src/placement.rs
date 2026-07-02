//! Mercenaries 2 world-placement loader (`layers_static`, WAD block index 29).
//!
//! Ports `tools/placement_extractor.py` (the layers_static path only). See
//! `docs/placement_data_format.md` §2. Iterates all 173 UCFX sub-blocks, walks
//! each CHDR/COMP chunk table (generalising `terrain::read_lrterrain_object_records`),
//! and pulls every `Transform` + `Name` COMP, matching them by u32 entity key.
//!
//! Coordinates stay ENTIRELY in native game space (left-handed, +Y up). The
//! Python reference contains UE/glTF coordinate flips — those are NOT ported.
//!
//! Input: `layers_static_block` — decompressed `layers_static_P000_Q3` block:
//! 173 UCFX sub-blocks; each has a CHDR chunk table of COMP components whose
//! `info`/`data` child offsets are RELATIVE to `data_area_start`.

const CHUNK_HDR: usize = 20;

/// One world placement: entity key, optional name, world position + rotation
/// quaternion, and the sub-block it came from.
#[derive(Debug, Clone)]
pub struct Placement {
    pub key: u32,
    pub name: Option<String>,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub sub_block: u16,
}

fn read_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn read_f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn find_all(data: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= data.len() {
        if &data[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// One COMP component within a sub-block: its `info` type name and the absolute
/// byte span of its `data` child.
struct CompChild {
    info_name: Option<String>,
    data: Option<(usize, usize)>,
}

/// Public COMP inventory record for a single COMP in a sub-block: its `info`
/// type name, the payload stride declared by its `schm` child (bytes after the
/// entity key; total record stride = 4 + payload_stride), and the absolute byte
/// span of its `data` child inside the whole block buffer.
#[derive(Debug, Clone)]
pub struct CompInfo {
    pub sub_block: u16,
    pub info_name: Option<String>,
    pub payload_stride: Option<u32>,
    pub data_off: Option<usize>,
    pub data_size: Option<usize>,
}

/// Enumerate EVERY COMP across every UCFX sub-block of a decompressed block (both
/// `layers_static` and `vz_state`-style overlay blocks use the same UCFX/CHDR/COMP
/// layout with data-relative child offsets). Reports each COMP's info type-name,
/// `schm` payload stride, and data span. Used by the `--comp-probe`.
pub fn comp_inventory(block: &[u8]) -> Vec<CompInfo> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            out.push(CompInfo {
                sub_block: si as u16,
                info_name: c.info_name,
                payload_stride: c.payload_stride,
                data_off: c.data.map(|(o, _)| o),
                data_size: c.data.map(|(_, s)| s),
            });
        }
    }
    out
}

/// Like `walk_sub_block_comps` but also captures each COMP's `schm` payload
/// stride (`schm[4:8]` per docs §2.6).
struct CompChildFull {
    info_name: Option<String>,
    payload_stride: Option<u32>,
    data: Option<(usize, usize)>,
}

fn walk_sub_block_comps_full(
    data: &[u8],
    ucfx_pos: usize,
    block_end: usize,
) -> Vec<CompChildFull> {
    let mut out = Vec::new();
    if ucfx_pos + 8 > data.len() {
        return out;
    }
    let ucfx_size = read_u32_le(data, ucfx_pos + 4) as usize;
    let search_end = (ucfx_pos + ucfx_size + 200).min(data.len());
    let chdr_pos = match data[ucfx_pos..search_end]
        .windows(4)
        .position(|w| w == b"CHDR")
    {
        Some(p) => ucfx_pos + p,
        None => return out,
    };
    if chdr_pos + 20 > data.len() {
        return out;
    }
    let chdr_entries = read_u32_le(data, chdr_pos + 12) as usize;

    let mut pos = chdr_pos + 20;
    let mut chunks: Vec<(Vec<u8>, Vec<([u8; 4], usize, usize)>)> = Vec::new();
    for _ in 0..chdr_entries {
        if pos + CHUNK_HDR > block_end {
            break;
        }
        let tag = &data[pos..pos + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let num_children = read_u32_le(data, pos + 16) as usize;
        let mut children = Vec::with_capacity(num_children);
        let mut child_pos = pos + CHUNK_HDR;
        for _ in 0..num_children {
            if child_pos + CHUNK_HDR > block_end {
                break;
            }
            let mut ctag = [0u8; 4];
            ctag.copy_from_slice(&data[child_pos..child_pos + 4]);
            let coff = read_u32_le(data, child_pos + 4) as usize;
            let csz = read_u32_le(data, child_pos + 8) as usize;
            children.push((ctag, coff, csz));
            child_pos += CHUNK_HDR;
        }
        chunks.push((tag.to_vec(), children));
        pos = child_pos;
    }
    let data_area_start = pos;

    for (tag, children) in &chunks {
        if tag != b"COMP" {
            continue;
        }
        let mut info_name: Option<String> = None;
        let mut payload_stride: Option<u32> = None;
        let mut data_child: Option<(usize, usize)> = None;
        for (ctag, coff, csz) in children {
            let abs_off = data_area_start + coff;
            if ctag == b"info" && abs_off + csz <= data.len() {
                let raw = &data[abs_off..abs_off + csz];
                if let Some(nul) = raw.iter().position(|&b| b == 0) {
                    if nul > 0 {
                        info_name = Some(String::from_utf8_lossy(&raw[..nul]).into_owned());
                    }
                }
            } else if ctag == b"schm" && abs_off + 8 <= data.len() {
                payload_stride = Some(read_u32_le(data, abs_off + 4));
            } else if ctag == b"data" {
                data_child = Some((abs_off, *csz));
            }
        }
        out.push(CompChildFull { info_name, payload_stride, data: data_child });
    }
    out
}

/// Walk one UCFX sub-block's CHDR chunk table and return every COMP's info-name
/// + data span. Generalises `terrain::read_lrterrain_object_records`: instead of
/// stopping at `LowResTerrainObject`, it collects ALL COMPs. COMP child offsets
/// are relative to `data_area_start` (the end of the chunk descriptor table).
fn walk_sub_block_comps(
    data: &[u8],
    ucfx_pos: usize,
    block_end: usize,
) -> Vec<CompChild> {
    let mut out = Vec::new();
    if ucfx_pos + 8 > data.len() {
        return out;
    }
    let ucfx_size = read_u32_le(data, ucfx_pos + 4) as usize;

    // CHDR chunk within this sub-block.
    let search_end = (ucfx_pos + ucfx_size + 200).min(data.len());
    let chdr_pos = match data[ucfx_pos..search_end]
        .windows(4)
        .position(|w| w == b"CHDR")
    {
        Some(p) => ucfx_pos + p,
        None => return out,
    };
    if chdr_pos + 20 > data.len() {
        return out;
    }
    let chdr_entries = read_u32_le(data, chdr_pos + 12) as usize;

    // Walk the CHDR chunk table: COMP/enum/flgt/flgs rows, each with children.
    let mut pos = chdr_pos + 20;
    let mut chunks: Vec<(Vec<u8>, Vec<([u8; 4], usize, usize)>)> = Vec::new();
    for _ in 0..chdr_entries {
        if pos + CHUNK_HDR > block_end {
            break;
        }
        let tag = &data[pos..pos + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let num_children = read_u32_le(data, pos + 16) as usize;
        let mut children = Vec::with_capacity(num_children);
        let mut child_pos = pos + CHUNK_HDR;
        for _ in 0..num_children {
            if child_pos + CHUNK_HDR > block_end {
                break;
            }
            let mut ctag = [0u8; 4];
            ctag.copy_from_slice(&data[child_pos..child_pos + 4]);
            let coff = read_u32_le(data, child_pos + 4) as usize;
            let csz = read_u32_le(data, child_pos + 8) as usize;
            children.push((ctag, coff, csz));
            child_pos += CHUNK_HDR;
        }
        chunks.push((tag.to_vec(), children));
        pos = child_pos;
    }
    let data_area_start = pos;

    for (tag, children) in &chunks {
        if tag != b"COMP" {
            continue;
        }
        let mut info_name: Option<String> = None;
        let mut data_child: Option<(usize, usize)> = None;
        for (ctag, coff, csz) in children {
            let abs_off = data_area_start + coff;
            if ctag == b"info" && abs_off + csz <= data.len() {
                let raw = &data[abs_off..abs_off + csz];
                if let Some(nul) = raw.iter().position(|&b| b == 0) {
                    if nul > 0 {
                        info_name = Some(String::from_utf8_lossy(&raw[..nul]).into_owned());
                    }
                }
            } else if ctag == b"data" {
                data_child = Some((abs_off, *csz));
            }
        }
        out.push(CompChild { info_name, data: data_child });
    }
    out
}

/// Parse a `Transform` COMP `data` blob into (key -> (pos, quat)) records.
/// Each record is 42 bytes: u32 key + XYZ + pad + quat(xyzw) + 6-byte tail.
fn parse_transform_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, [f32; 3], [f32; 4])> {
    const STRIDE: usize = 42;
    let mut out = Vec::new();
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        let key = read_u32_le(data, r);
        let pos = [
            read_f32_le(data, r + 4),
            read_f32_le(data, r + 8),
            read_f32_le(data, r + 12),
        ];
        let quat = [
            read_f32_le(data, r + 20),
            read_f32_le(data, r + 24),
            read_f32_le(data, r + 28),
            read_f32_le(data, r + 32),
        ];
        out.push((key, pos, quat));
    }
    out
}

/// Parse a `Name` COMP `data` blob into (key -> name) records. Each record is
/// `[u32 key][ascii "EntityName 0xKEY\0"]`; the stored string is the full
/// `"Name 0xHEXID"` — we keep the bare name portion (before the trailing hex id).
fn parse_name_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let blob = &data[off..off + size];
    let mut p = 0usize;
    while p + 4 < blob.len() {
        let key = u32::from_le_bytes([blob[p], blob[p + 1], blob[p + 2], blob[p + 3]]);
        p += 4;
        // The string runs to the next NUL.
        let start = p;
        while p < blob.len() && blob[p] != 0 {
            p += 1;
        }
        if p <= start {
            // Empty string; skip the NUL and continue.
            p += 1;
            continue;
        }
        let s = String::from_utf8_lossy(&blob[start..p]).into_owned();
        // Strip a trailing " 0xHEXID" if present (gameplay name is the head).
        let name = match s.rfind(" 0x") {
            Some(i) => s[..i].to_string(),
            None => s,
        };
        out.push((key, name));
        // Consume the NUL terminator (and any run of padding NULs).
        while p < blob.len() && blob[p] == 0 {
            p += 1;
        }
    }
    out
}

/// Load all world placements from a decompressed `layers_static` block.
///
/// Iterates every UCFX sub-block, walks its COMP table, matches `Name` COMP
/// records to `Transform` COMP records by u32 entity key (within the sub-block),
/// and emits one `Placement` per Transform record. No coordinate flips.
pub fn load_placements(layers_static_block: &[u8]) -> Result<Vec<Placement>, String> {
    let ucfx_positions = find_all(layers_static_block, b"UCFX");
    if ucfx_positions.is_empty() {
        return Err("no UCFX sub-blocks found in layers_static".into());
    }

    let mut out: Vec<Placement> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            layers_static_block.len()
        };
        let comps = walk_sub_block_comps(layers_static_block, ucfx_pos, block_end);

        // Collect this sub-block's Transform records and Name map.
        let mut transforms: Vec<(u32, [f32; 3], [f32; 4])> = Vec::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        for c in &comps {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    transforms.extend(parse_transform_records(layers_static_block, off, size));
                }
                Some("Name") => {
                    for (k, n) in parse_name_records(layers_static_block, off, size) {
                        names.entry(k).or_insert(n);
                    }
                }
                _ => {}
            }
        }

        for (key, pos, quat) in transforms {
            out.push(Placement {
                key,
                name: names.get(&key).cloned(),
                pos,
                quat,
                sub_block: si as u16,
            });
        }
    }

    if out.is_empty() {
        return Err("no Transform COMP records found in layers_static".into());
    }
    Ok(out)
}

/// Yaw (radians) around +Y from a placement quaternion: `2 * atan2(qy, qw)`.
pub fn yaw_from_quat(quat: &[f32; 4]) -> f32 {
    2.0 * quat[1].atan2(quat[3])
}

/// One prop placement: the entity key, the model asset hash it renders as (the `ModelName`
/// COMP record's second u32 = `pandemic_hash_m2(model-name)` = the model ASET `asset_hash`),
/// its world transform (pos + full quat, native game space — no flip), and the optional
/// gameplay name (from the `Name` COMP).
#[derive(Debug, Clone)]
pub struct ModelPlacement {
    pub key: u32,
    pub model_hash: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub name: Option<String>,
}

/// Parse a `ModelName` COMP `data` blob into (key -> model_hash) records. Each record is
/// `{u32 entity_key, u32 model_hash}` (8-byte stride); the model_hash is the model ASET
/// asset_hash (== `pandemic_hash_m2(model-name)`).
fn parse_model_name_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, u32)> {
    const STRIDE: usize = 8;
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        out.push((read_u32_le(data, r), read_u32_le(data, r + 4)));
    }
    out
}

/// Load every `ModelName` prop placement from a decompressed UCFX block (exterior
/// `layers_static` block 29, or an interior `vz_state_*` block such as 667), joined to its
/// `Transform` (pos + full quat) and `Name` COMP by u32 entity key within each sub-block.
///
/// The `ModelName` COMP is the entity->mesh link the engine uses for discrete props/furniture
/// (buildings are baked into c3 cells). One `ModelPlacement` is emitted per `ModelName` record
/// that has a matching `Transform` in the same sub-block; records without a Transform are
/// skipped (their world position is unknown). Coordinates stay entirely in native game space
/// (left-handed, +Y up) — no flips. The full quaternion is preserved (16% of props carry
/// pitch/roll, not just yaw).
pub fn load_model_placements(block: &[u8]) -> Vec<ModelPlacement> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out: Vec<ModelPlacement> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };

        let mut xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])> =
            std::collections::HashMap::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        let mut models: Vec<(u32, u32)> = Vec::new();
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    for (k, p, q) in parse_transform_records(block, off, size) {
                        xform.entry(k).or_insert((p, q));
                    }
                }
                Some("Name") => {
                    for (k, n) in parse_name_records(block, off, size) {
                        names.entry(k).or_insert(n);
                    }
                }
                Some("ModelName") => {
                    models.extend(parse_model_name_records(block, off, size));
                }
                _ => {}
            }
        }

        for (key, model_hash) in models {
            let Some(&(pos, quat)) = xform.get(&key) else { continue };
            out.push(ModelPlacement {
                key,
                model_hash,
                pos,
                quat,
                name: names.get(&key).cloned(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Transform record is exactly 42 bytes: key + XYZ + pad + quat + 6-tail.
    #[test]
    fn transform_record_stride_42() {
        // Two back-to-back records; second key must be read at +42.
        let mut d = vec![0u8; 84];
        d[0..4].copy_from_slice(&0x1111_1111u32.to_le_bytes());
        d[4..8].copy_from_slice(&12.5f32.to_le_bytes()); // x
        d[8..12].copy_from_slice(&(-3.0f32).to_le_bytes()); // y
        d[12..16].copy_from_slice(&7.0f32.to_le_bytes()); // z
        d[32..36].copy_from_slice(&1.0f32.to_le_bytes()); // qw
        d[42..46].copy_from_slice(&0x2222_2222u32.to_le_bytes());
        let recs = parse_transform_records(&d, 0, 84);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].0, 0x1111_1111);
        assert_eq!(recs[0].1, [12.5, -3.0, 7.0]);
        assert_eq!(recs[0].2, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(recs[1].0, 0x2222_2222);
    }

    /// Name records are `[u32 key][ascii name 0xHEXID\0]`; the bare name is kept.
    #[test]
    fn name_record_strips_hex_id() {
        let mut d = Vec::new();
        d.extend_from_slice(&0x00AB_CDEFu32.to_le_bytes());
        d.extend_from_slice(b"pmc_interior 0xABCDEF\0");
        let recs = parse_name_records(&d, 0, d.len());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].0, 0x00AB_CDEF);
        assert_eq!(recs[0].1, "pmc_interior");
    }

    /// A ModelName record is `{u32 key, u32 model_hash}` at 8-byte stride.
    #[test]
    fn model_name_record_stride_8() {
        let mut d = vec![0u8; 16];
        d[0..4].copy_from_slice(&0xAAAA_0001u32.to_le_bytes());
        d[4..8].copy_from_slice(&0x5B72_4250u32.to_le_bytes());
        d[8..12].copy_from_slice(&0xAAAA_0002u32.to_le_bytes());
        d[12..16].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let recs = parse_model_name_records(&d, 0, 16);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0], (0xAAAA_0001, 0x5B72_4250));
        assert_eq!(recs[1], (0xAAAA_0002, 0xDEAD_BEEF));
    }

    /// A pure-yaw quaternion sample is unit length, and yaw round-trips.
    #[test]
    fn quat_unit_length_and_yaw() {
        let yaw = 1.2345f32;
        let q = [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()];
        let mag = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((mag - 1.0).abs() < 1e-5, "quat mag {mag}");
        assert!((yaw_from_quat(&q) - yaw).abs() < 1e-4);
    }
}
