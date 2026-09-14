//! Watermap (`watr`) codec — decode the resident water-heightfield singleton.
//!
//! Port of `tools/watermap_decode.py` (Python legacy) — decode-only, no image export.
//!
//! # Layout (verified on retail PC `resident_P000_Q3`, type_hash `0x4D7D30C4`)
//!
//! The watermap lives in the always-loaded `resident` block as a single ASET entry with
//! `TYPE_HASH_WATERMAP = 0x4D7D30C4` / `TYPE_ID_WATERMAP = 25`. Its body is a UCFX container
//! wrapping a single `watr` chunk (~495 KiB on retail):
//!
//! * `watr` header (36 B): `u32 layer_count`, `u32 grid_w`, `u32 grid_h`, then six `f32`
//!   metadata floats (offsets 12..36).
//! * Layer 0: `f32` height field, `grid_w × grid_h` samples (257×257 on retail).
//! * Layers 1..3: `u8` masks, same resolution — wet mask (confirmed) + two hypothesis layers.
//! * Trailing ~33 KiB footer — does NOT divide the grid, so it is **not** a fourth full-grid
//!   raster; hypothesis is that `layer_count = 5` counts height + three u8 grids + this blob.
//!
//! Retail wet cells carry `~-36 m` (sea level); dry cells use a `-50 m` sentinel. Terrain
//! open-water tiles at `Y=0` are a separate system.

use core::convert::TryInto;

/// ASET `type_hash` for the resident watermap singleton.
pub const WATERMAP_TYPE_HASH: u32 = 0x4D7D30C4;

/// ASET `type_id` for the resident watermap singleton.
pub const TYPE_ID_WATERMAP: u32 = 25;

/// Size of the `watr` fixed header before the first raster layer.
pub const WATR_HEADER_SIZE: usize = 36;

/// UCFX chunk header stride.
const CHUNK_HDR: usize = 20;

/// Retail footer length window used by [`Watermap::footer_looks_valid`].
const FOOTER_MIN_SIZE: usize = 32_000;
const FOOTER_MAX_SIZE: usize = 34_000;

/// Retail sea-level value for wet cells (`mask == 255`), in game LH metres.
pub const SEA_LEVEL_M: f32 = -36.0;
/// Retail dry-cell sentinel value in the height field, in game LH metres.
pub const DRY_SENTINEL_M: f32 = -50.0;

/// Six metadata floats parsed from the `watr` header (offsets 12..36).
///
/// Naming follows the Python port; `field_c` and `field_d` sit at the same bytes as
/// [`Watermap::header_u32_at_28`] and may carry non-float packed data on some builds.
#[derive(Clone, Debug, Default)]
pub struct WatrHeaderFloats {
    /// Grid cell size in game LH metres (retail: 32 m).
    pub cell_size_m: f32,
    /// Header-clamp minimum height in metres.
    pub height_min_m: f32,
    /// Header-clamp maximum height in metres.
    pub height_max_m: f32,
    /// Unknown scalar (retail: 64.0). Hypothesis: legacy / non-metre.
    pub field_b: f32,
    /// Unknown; overlaps `header_u32_at_28[0]`.
    pub field_c: f32,
    /// Unknown; overlaps `header_u32_at_28[1]`.
    pub field_d: f32,
}

/// Parsed `watr` payload — the world height field plus per-cell masks.
#[derive(Clone, Debug)]
pub struct Watermap {
    /// `watr` header field. Retail = 5 (height + 3 u8 masks + trailing footer blob).
    pub layer_count: u32,
    /// Grid samples along X (retail: 257).
    pub grid_width: u32,
    /// Grid samples along Z (retail: 257).
    pub grid_height: u32,
    /// Six metadata floats at offsets 12..36.
    pub header_floats: WatrHeaderFloats,
    /// Two u32 words at offsets 28..36 (aliased with `field_c`/`field_d`).
    pub header_u32_at_28: [u32; 2],
    /// Layer 0 — `f32` height field, `grid_width * grid_height` samples in metres.
    pub heights: Vec<f32>,
    /// Layer 1 — wet mask. Confirmed: `0 = dry (-50 m sentinel)`, `255 = wet (~-36 m)`.
    pub wet_mask: Vec<u8>,
    /// Layer 2 — coastal variant (hypothesis; `255` = default, other = sparse shore codes).
    pub coastal_variant: Vec<u8>,
    /// Layer 3 — sparse per-cell override (hypothesis; `255` = default).
    pub override_sparse: Vec<u8>,
    /// Trailing bytes after the four full-grid layers (retail: ~33 KiB blob, does not
    /// divide `grid_cells()`).
    pub footer: Vec<u8>,
}

impl Watermap {
    /// Total sample count (`grid_width * grid_height`).
    pub fn grid_cells(&self) -> usize {
        (self.grid_width as usize) * (self.grid_height as usize)
    }

    /// Confirmed cell size in metres, if the header carries a finite positive value.
    pub fn cell_size_m(&self) -> Option<f32> {
        let v = self.header_floats.cell_size_m;
        if v.is_finite() && v > 0.0 {
            Some(v)
        } else {
            None
        }
    }

    /// Header-declared height clamp `(min, max)` in metres, if both are finite.
    pub fn height_range_m(&self) -> Option<(f32, f32)> {
        let mn = self.header_floats.height_min_m;
        let mx = self.header_floats.height_max_m;
        if mn.is_finite() && mx.is_finite() {
            Some((mn, mx))
        } else {
            None
        }
    }

    /// World span `(x, z)` in metres derived from cell size × (grid_dim - 1).
    /// `None` when `cell_size_m` is missing or non-positive.
    pub fn world_span_m(&self) -> Option<(f32, f32)> {
        let cell = self.cell_size_m()?;
        let span_x = (self.grid_width as f32 - 1.0) * cell;
        let span_z = (self.grid_height as f32 - 1.0) * cell;
        Some((span_x, span_z))
    }

    /// True when the trailing blob length falls in the retail-observed window (~32–34 KB).
    pub fn footer_looks_valid(&self) -> bool {
        self.footer.len() >= FOOTER_MIN_SIZE && self.footer.len() <= FOOTER_MAX_SIZE
    }

    /// Expected bytes for header + four full-grid layers (height f32 + 3 × u8 masks).
    /// Actual payload length is this plus [`Self::footer`].
    pub fn expected_pre_footer_bytes(&self) -> usize {
        WATR_HEADER_SIZE + self.grid_cells() * (4 + 3)
    }

    /// Sample height at `(ix, iz)`; returns `None` on out-of-bounds.
    pub fn height_at(&self, ix: u32, iz: u32) -> Option<f32> {
        if ix >= self.grid_width || iz >= self.grid_height {
            return None;
        }
        let idx = (iz as usize) * (self.grid_width as usize) + (ix as usize);
        self.heights.get(idx).copied()
    }
}

// ---------------------------------------------------------------------------
// Container / UCFX helpers (mirror of Python `iter_block_entries`
// + `_find_watermap_body` + `_extract_watr_payload`)
// ---------------------------------------------------------------------------

/// Block-table entry: `(asset_hash, type_hash, body_offset, size)`.
type BlockEntry = (u32, u32, usize, usize);

fn read_u32_le(data: &[u8], off: usize) -> Result<u32, String> {
    let s = data
        .get(off..off + 4)
        .ok_or_else(|| format!("u32 read out of range at {}", off))?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

fn read_f32_le(data: &[u8], off: usize) -> Result<f32, String> {
    let s = data
        .get(off..off + 4)
        .ok_or_else(|| format!("f32 read out of range at {}", off))?;
    Ok(f32::from_le_bytes(s.try_into().unwrap()))
}

/// Walk the block-file header table and yield each entry with its absolute body offset.
fn iter_block_entries(data: &[u8]) -> Vec<BlockEntry> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap());
    if count < 1 || count > 50_000 {
        return Vec::new();
    }
    let count_us = count as usize;
    let header_end = 4 + count_us * 16;
    if header_end > data.len() {
        return Vec::new();
    }
    let mut out: Vec<BlockEntry> = Vec::with_capacity(count_us);
    let mut cumulative = header_end;
    for i in 0..count_us {
        let off = 4 + i * 16;
        if off + 16 > data.len() {
            break;
        }
        let asset_hash = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let type_hash = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        // data[off+8..off+12] reserved
        let size = u32::from_le_bytes(data[off + 12..off + 16].try_into().unwrap()) as usize;
        out.push((asset_hash, type_hash, cumulative, size));
        cumulative = cumulative.saturating_add(size);
    }
    out
}

/// Find the watermap entry inside a block file and return `(body_offset, body_slice)`.
fn find_watermap_body(data: &[u8]) -> Result<(usize, &[u8]), String> {
    for (_ah, type_hash, body_off, size) in iter_block_entries(data) {
        if type_hash == WATERMAP_TYPE_HASH {
            let end = body_off
                .checked_add(size)
                .ok_or_else(|| "watermap entry size overflow".to_string())?;
            if end > data.len() {
                return Err(format!(
                    "watermap body truncated: {}..{} of {}",
                    body_off,
                    end,
                    data.len()
                ));
            }
            return Ok((body_off, &data[body_off..end]));
        }
    }
    Err("watermap entry not found".into())
}

/// Peel the UCFX outer container off the watermap body and return the raw `watr` payload.
///
/// Assumes the retail layout: one `watr` chunk immediately following the UCFX header, whose
/// data lives at `dao + rel_off` and spans `chunk_size` bytes.
fn extract_watr_payload(ucfx_body: &[u8]) -> Result<&[u8], String> {
    if ucfx_body.len() < CHUNK_HDR || &ucfx_body[..4] != b"UCFX" {
        return Err("watermap body is not UCFX".into());
    }
    // Outer UCFX header: 'UCFX' | dao u32 | u1 u32 | u2 u32 | n_chunks u32
    let dao = read_u32_le(ucfx_body, 4)? as usize;
    let first_chunk_pos = 20;
    if first_chunk_pos + CHUNK_HDR > ucfx_body.len() {
        return Err("truncated UCFX chunk table".into());
    }
    // Chunk record: tag[4] | u0(rel_off) u32 | u1(size) u32 | u2 u32 | u3 u32
    let rel_off = read_u32_le(ucfx_body, first_chunk_pos + 4)? as usize;
    let chunk_size = read_u32_le(ucfx_body, first_chunk_pos + 8)? as usize;
    let start = dao
        .checked_add(rel_off)
        .ok_or_else(|| "rel_off overflow".to_string())?;
    if start > ucfx_body.len() {
        return Err(format!(
            "watr payload start {} beyond body {}",
            start,
            ucfx_body.len()
        ));
    }
    let end = start.saturating_add(chunk_size).min(ucfx_body.len());
    Ok(&ucfx_body[start..end])
}

// ---------------------------------------------------------------------------
// Public decode API
// ---------------------------------------------------------------------------

/// Decode a raw `watr` payload (post-UCFX-extraction).
///
/// Mirrors Python `decode_watr_payload` — parses the 36-byte header, one `f32` height
/// layer, and three `u8` mask layers, then stashes the trailing bytes verbatim in
/// [`Watermap::footer`].
pub fn parse_watr_payload(payload: &[u8]) -> Result<Watermap, String> {
    if payload.len() < WATR_HEADER_SIZE {
        return Err(format!("watr payload too short: {}", payload.len()));
    }
    let layer_count = read_u32_le(payload, 0)?;
    let grid_w = read_u32_le(payload, 4)?;
    let grid_h = read_u32_le(payload, 8)?;
    let header_floats = WatrHeaderFloats {
        cell_size_m: read_f32_le(payload, 12)?,
        height_min_m: read_f32_le(payload, 16)?,
        height_max_m: read_f32_le(payload, 20)?,
        field_b: read_f32_le(payload, 24)?,
        field_c: read_f32_le(payload, 28)?,
        field_d: read_f32_le(payload, 32)?,
    };
    let header_u32_at_28 = [read_u32_le(payload, 28)?, read_u32_le(payload, 32)?];

    let grid = (grid_w as usize)
        .checked_mul(grid_h as usize)
        .ok_or_else(|| format!("grid overflow: {}x{}", grid_w, grid_h))?;
    if grid == 0 {
        return Err("invalid grid size".into());
    }

    let mut off = WATR_HEADER_SIZE;

    // Layer 0 — f32 height field
    let need0 = grid
        .checked_mul(4)
        .ok_or_else(|| "height layer size overflow".to_string())?;
    let end0 = off
        .checked_add(need0)
        .ok_or_else(|| "height layer end overflow".to_string())?;
    if end0 > payload.len() {
        return Err("truncated f32 height layer".into());
    }
    let mut heights: Vec<f32> = Vec::with_capacity(grid);
    for i in 0..grid {
        let base = off + i * 4;
        heights.push(f32::from_le_bytes(
            payload[base..base + 4].try_into().unwrap(),
        ));
    }
    off = end0;

    // Layers 1..3 — u8 masks (same topology)
    let read_mask = |src_off: usize, name: &str| -> Result<Vec<u8>, String> {
        let end = src_off
            .checked_add(grid)
            .ok_or_else(|| format!("{} layer size overflow", name))?;
        if end > payload.len() {
            return Err(format!("truncated u8 layer '{}'", name));
        }
        Ok(payload[src_off..end].to_vec())
    };
    let wet_mask = read_mask(off, "wet_mask")?;
    off += grid;
    let coastal_variant = read_mask(off, "coastal_variant")?;
    off += grid;
    let override_sparse = read_mask(off, "override_sparse")?;
    off += grid;

    let footer = payload[off..].to_vec();

    Ok(Watermap {
        layer_count,
        grid_width: grid_w,
        grid_height: grid_h,
        header_floats,
        header_u32_at_28,
        heights,
        wet_mask,
        coastal_variant,
        override_sparse,
        footer,
    })
}

/// Decode a full container block: header table → find watermap entry → UCFX chunk → `watr`.
///
/// `container` is the raw bytes of a decompressed `.block.bin` (retail: `resident_P000_Q3`).
/// Mirrors Python `decode_block`.
pub fn parse(container: &[u8]) -> Result<Watermap, String> {
    let (_body_off, body) = find_watermap_body(container)?;
    let payload = extract_watr_payload(body)?;
    parse_watr_payload(payload)
}

// ---------------------------------------------------------------------------
// Tests (compile-only sanity for header helpers)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_payload_rejected() {
        let err = parse_watr_payload(&[0u8; 10]).unwrap_err();
        assert!(err.contains("too short"));
    }

    #[test]
    fn zero_grid_rejected() {
        let mut buf = vec![0u8; WATR_HEADER_SIZE];
        // layer_count=1, grid_w=0, grid_h=0 (all zeros already)
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        let err = parse_watr_payload(&buf).unwrap_err();
        assert!(err.contains("invalid grid"));
    }

    #[test]
    fn tiny_grid_roundtrip() {
        // 2x2 grid: header + 4*f32 heights + 4*3 u8 masks = 36 + 16 + 12 = 64 B
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(&5u32.to_le_bytes()); // layer_count
        buf.extend_from_slice(&2u32.to_le_bytes()); // grid_w
        buf.extend_from_slice(&2u32.to_le_bytes()); // grid_h
        buf.extend_from_slice(&32.0f32.to_le_bytes()); // cell_size_m
        buf.extend_from_slice(&(-50.0f32).to_le_bytes()); // hmin
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // hmax
        buf.extend_from_slice(&64.0f32.to_le_bytes()); // field_b
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // field_c
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // field_d
        for v in [-36.0f32, -50.0, -36.0, 0.0] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&[255u8, 0, 255, 0]); // wet_mask
        buf.extend_from_slice(&[255u8, 255, 255, 255]); // coastal_variant
        buf.extend_from_slice(&[255u8, 255, 255, 255]); // override_sparse
        let wm = parse_watr_payload(&buf).unwrap();
        assert_eq!(wm.grid_width, 2);
        assert_eq!(wm.grid_height, 2);
        assert_eq!(wm.grid_cells(), 4);
        assert_eq!(wm.heights.len(), 4);
        assert_eq!(wm.wet_mask, vec![255, 0, 255, 0]);
        assert_eq!(wm.cell_size_m(), Some(32.0));
        assert_eq!(wm.world_span_m(), Some((32.0, 32.0)));
        assert_eq!(wm.height_at(1, 1), Some(0.0));
        assert!(wm.footer.is_empty());
    }
}
