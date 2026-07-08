//! The static watermap — the engine's "is this (x,z) over water and how high is the surface" data.
//!
//! Code map §4 (static waterline) + [`docs/watermap_format.md`](../../../../docs/watermap_format.md):
//! the watermap is a single reflection asset, type hash `0x4D7D30C4` = `pandemic_hash_m2("watermap")`,
//! stored as a `watr` UCFX chunk. It is a **height field + wet mask over a regular XZ grid** — the
//! static half of the water system (the *dynamic* wave displacement is rendered per-frame into the
//! ping-pong `pHeightS` RTs by `PgWaterHeightMapVP/FP`, code map §4; that layer is render-time and is
//! deliberately NOT modelled here — see the crate docs).
//!
//! Recovered layout (`watr`, all confirmed in the format doc unless noted):
//! - header: `layer_count`(5) · `grid_width`(257) · `grid_height`(257) · `cell_size_m`(32.0) ·
//!   `height_min_m`(−50.0) · `height_max_m`(~325.26) · 3 unknown trailing fields;
//! - **Layer 0** — `f32[w*h]` water-surface height in metres (game Y-up);
//! - **Layer 1** — `u8[w*h]` wet mask (`0` = dry/land, `255` = wet/water column);
//! - Layers 2–3 (coastal-variant / sparse-override, *hypothesis*) + a 33,290-B footer of unknown
//!   purpose are **not** modelled — the code map/format doc mark them unconfirmed.
//!
//! Sentinels the height field uses: dry cells are exactly `height_min_m` (−50.0); the open-water wet
//! plateau sits near **−36.0 m** (the sea surface in the retail Maracaibo asset — *not* Y=0).

/// Reflection type hash of the watermap asset: `pandemic_hash_m2("watermap")` (format doc). Asserted
/// against the live hasher in the tests.
pub const WATERMAP_HASH: u32 = 0x4D7D_30C4;

/// UCFX chunk tag carrying the watermap payload.
pub const WATR_TAG: [u8; 4] = *b"watr";

/// Confirmed retail grid dimension (square): 257×257 samples = 256 intervals.
pub const GRID_DIM: usize = 257;

/// Confirmed cell size in metres (header `cell_size_m`). 256 intervals × 32 m = an 8192 m span.
pub const CELL_SIZE_M: f32 = 32.0;

/// Confirmed dry-cell sentinel / header `height_min_m`: a cell reading exactly this is land.
pub const HEIGHT_MIN_M: f32 = -50.0;

/// Open-water wet-surface plateau (≈ −36 m) — the sea level *in the watermap asset*, per the format
/// doc. Not a hard header field; the reference value the reimpl calibrates the ocean plane against.
pub const OPEN_WATER_SURFACE_M: f32 = -36.0;

/// Wet-mask byte for a water column (Layer 1 `255`).
pub const WET: u8 = 255;
/// Wet-mask byte for dry land (Layer 1 `0`).
pub const DRY: u8 = 0;

/// The result of a watermap query at a world XZ position (the engine-owned half of `FUN_00480440`'s
/// job, code map §5 — the SecuROM-island thunk whose *exact* return packing, height-vs-boolean, is
/// confirm-live; here we return both facts and let the caller pick).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSample {
    /// Whether this position is over a water column (Layer 1 wet mask == `255`).
    pub is_water: bool,
    /// Water-surface height in metres at this position (Layer 0), regardless of `is_water` — a dry
    /// cell reports its sentinel (`HEIGHT_MIN_M`). Callers gate on `is_water`.
    pub surface_height: f32,
}

/// Errors from parsing a raw `watr` chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatermapError {
    /// Buffer shorter than the fixed header.
    TooShortForHeader,
    /// A grid dimension was zero / absurd.
    BadDimensions,
    /// Buffer shorter than header + Layer 0 (f32 heights) + Layer 1 (u8 mask).
    TooShortForLayers,
}

/// A loaded static watermap: the Layer-0 height field + Layer-1 wet mask over a regular XZ grid, plus
/// the grid→world mapping. This is the loadable data behind the waterline query.
///
/// **World mapping** (format doc §"World extent mapping"): the grid is centred on the world origin —
/// index `0` maps to world `-(dim-1)/2 * cell`. Origin alignment is a *hypothesis* in the format doc
/// (the exe hasn't been shown to pin it), so it is stored as [`origin_x`]/[`origin_z`] fields rather
/// than hard-coded, letting an exe-confirmed origin override the centred default.
///
/// [`origin_x`]: Watermap::origin_x
/// [`origin_z`]: Watermap::origin_z
#[derive(Clone, Debug, PartialEq)]
pub struct Watermap {
    width: usize,
    height: usize,
    cell_size: f32,
    /// World X of grid index `ix = 0`.
    pub origin_x: f32,
    /// World Z of grid index `iz = 0`.
    pub origin_z: f32,
    /// Layer 0 — water-surface height per cell, row-major (`iz*width + ix`), metres.
    heights: Vec<f32>,
    /// Layer 1 — wet mask per cell, row-major (`0` dry, `255` wet).
    wet: Vec<u8>,
}

impl Watermap {
    /// Build from decoded layers. `heights` and `wet` are row-major `width*height`. `origin_*` are the
    /// world coords of index `(0,0)`; use [`centred_origin`](Self::centred_origin) for the retail
    /// default.
    pub fn from_parts(
        width: usize,
        height: usize,
        cell_size: f32,
        origin_x: f32,
        origin_z: f32,
        heights: Vec<f32>,
        wet: Vec<u8>,
    ) -> Self {
        assert_eq!(heights.len(), width * height, "height layer size mismatch");
        assert_eq!(wet.len(), width * height, "wet-mask layer size mismatch");
        Watermap { width, height, cell_size, origin_x, origin_z, heights, wet }
    }

    /// The centred origin coordinate for a grid dimension `dim` at `cell_size` — `-(dim-1)/2 * cell`
    /// (format doc index→world hypothesis). For 257 @ 32 m this is −4096 m.
    pub fn centred_origin(dim: usize, cell_size: f32) -> f32 {
        -((dim as f32 - 1.0) * 0.5) * cell_size
    }

    /// Parse a raw `watr` chunk (the payload *after* the 4-byte tag). Reads the fixed header + Layer 0
    /// (f32 heights) + Layer 1 (u8 wet mask); Layers 2–3 and the footer are left unread (unconfirmed,
    /// per the format doc). Uses the centred-origin hypothesis.
    pub fn from_watr_bytes(buf: &[u8]) -> Result<Self, WatermapError> {
        const HEADER: usize = 36;
        if buf.len() < HEADER {
            return Err(WatermapError::TooShortForHeader);
        }
        let rd_u32 = |o: usize| u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        let rd_f32 = |o: usize| f32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        // +0 layer_count (ignored — logical layer count, not a raster count), +4 w, +8 h, +12 cell.
        let width = rd_u32(4) as usize;
        let height = rd_u32(8) as usize;
        let cell_size = rd_f32(12);
        if width == 0 || height == 0 || width > 1 << 16 || height > 1 << 16 {
            return Err(WatermapError::BadDimensions);
        }
        let n = width * height;
        let l0 = HEADER;
        let l1 = l0 + n * 4;
        if buf.len() < l1 + n {
            return Err(WatermapError::TooShortForLayers);
        }
        let heights: Vec<f32> = (0..n).map(|i| rd_f32(l0 + i * 4)).collect();
        let wet: Vec<u8> = buf[l1..l1 + n].to_vec();
        let origin_x = Self::centred_origin(width, cell_size);
        let origin_z = Self::centred_origin(height, cell_size);
        Ok(Watermap { width, height, cell_size, origin_x, origin_z, heights, wet })
    }

    /// A uniform test/stand-in map: every cell at `surface_height`, wet or dry, centred. Not a
    /// disk format — a convenience for driving the swim/buoyancy mechanism without a real asset.
    pub fn uniform(dim: usize, cell_size: f32, surface_height: f32, wet: bool) -> Self {
        let n = dim * dim;
        let origin = Self::centred_origin(dim, cell_size);
        Watermap {
            width: dim,
            height: dim,
            cell_size,
            origin_x: origin,
            origin_z: origin,
            heights: vec![surface_height; n],
            wet: vec![if wet { WET } else { DRY }; n],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// World XZ → nearest grid index, clamped to the grid. Nearest-cell (not bilinear): Layer 1 is a
    /// categorical mask and Layer 0 mixes a −50 dry sentinel with wet heights, so interpolating across
    /// a shoreline would smear both — the engine samples the discrete field, so we do too. Returns the
    /// clamped `(ix, iz)`.
    pub fn cell_at(&self, x: f32, z: f32) -> (usize, usize) {
        let fx = (x - self.origin_x) / self.cell_size;
        let fz = (z - self.origin_z) / self.cell_size;
        let ix = (fx.round() as i64).clamp(0, self.width as i64 - 1) as usize;
        let iz = (fz.round() as i64).clamp(0, self.height as i64 - 1) as usize;
        (ix, iz)
    }

    /// Whether a world XZ lies inside the grid footprint at all (outside → no water data).
    pub fn contains(&self, x: f32, z: f32) -> bool {
        let max_x = self.origin_x + (self.width as f32 - 1.0) * self.cell_size;
        let max_z = self.origin_z + (self.height as f32 - 1.0) * self.cell_size;
        x >= self.origin_x && x <= max_x && z >= self.origin_z && z <= max_z
    }

    fn idx(&self, ix: usize, iz: usize) -> usize {
        iz * self.width + ix
    }

    /// Build a renderable surface mesh over every **wet** cell: one flat quad per cell at that cell's
    /// Layer-0 surface height, in world space (game Y-up). Returns `(positions, indices)` for a
    /// translucent water pass. Empty when the map has no wet cells. Positions are `[x, y, z]`; indices
    /// are `u32` triangles (two per quad, CCW seen from above).
    pub fn surface_mesh(&self) -> (Vec<[f32; 3]>, Vec<u32>) {
        let mut pos = Vec::new();
        let mut idx = Vec::new();
        let cs = self.cell_size;
        for iz in 0..self.height {
            for ix in 0..self.width {
                let i = self.idx(ix, iz);
                if self.wet[i] != WET {
                    continue;
                }
                let h = self.heights[i];
                let x0 = self.origin_x + ix as f32 * cs;
                let z0 = self.origin_z + iz as f32 * cs;
                let (x1, z1) = (x0 + cs, z0 + cs);
                let base = pos.len() as u32;
                pos.push([x0, h, z0]);
                pos.push([x1, h, z0]);
                pos.push([x1, h, z1]);
                pos.push([x0, h, z1]);
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        (pos, idx)
    }

    /// The full query at a world XZ: wet flag + surface height. Positions outside the grid are dry with
    /// the dry sentinel height. This is the engine-owned waterline query used by swim/buoyancy (§5).
    pub fn sample(&self, x: f32, z: f32) -> WaterSample {
        if !self.contains(x, z) {
            return WaterSample { is_water: false, surface_height: HEIGHT_MIN_M };
        }
        let (ix, iz) = self.cell_at(x, z);
        let i = self.idx(ix, iz);
        WaterSample { is_water: self.wet[i] == WET, surface_height: self.heights[i] }
    }

    /// Convenience: is this world XZ over a water column?
    pub fn is_water(&self, x: f32, z: f32) -> bool {
        self.sample(x, z).is_water
    }

    /// Convenience: the water-surface height at this XZ **only where it is water** (`None` over land).
    pub fn water_surface_height(&self, x: f32, z: f32) -> Option<f32> {
        let s = self.sample(x, z);
        if s.is_water {
            Some(s.surface_height)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    /// The recovered type hash is exactly `pandemic_hash_m2("watermap")` (format doc claim, verified
    /// live).
    #[test]
    fn watermap_type_hash_is_pandemic_of_watermap() {
        assert_eq!(pandemic_hash_m2("watermap"), WATERMAP_HASH);
    }

    /// Centred origin for the retail 257 @ 32 m grid is −4096 m (the ±4096 half-span).
    #[test]
    fn centred_origin_matches_retail_extent() {
        assert_eq!(Watermap::centred_origin(GRID_DIM, CELL_SIZE_M), -4096.0);
    }

    /// `surface_mesh` emits one quad (4 verts, 6 indices) per WET cell at that cell's surface height,
    /// in world space — and nothing for dry cells.
    #[test]
    fn surface_mesh_covers_only_wet_cells() {
        // 2×2 grid, cell 10 m, origin (0,0). Two wet cells (heights 3.0), two dry.
        let heights = vec![3.0, 3.0, -50.0, -50.0];
        let wet = vec![WET, WET, 0, 0];
        let wm = Watermap::from_parts(2, 2, 10.0, 0.0, 0.0, heights, wet);
        let (pos, idx) = wm.surface_mesh();
        assert_eq!(pos.len(), 8, "2 wet cells → 8 verts");
        assert_eq!(idx.len(), 12, "2 wet cells → 12 indices (2 tris each)");
        // Every emitted vertex sits at a wet cell's surface height.
        assert!(pos.iter().all(|p| (p[1] - 3.0).abs() < 1e-6), "verts at the 3.0 m waterline");
        // Cell (0,0) quad spans [0,10]×[0,10] in world XZ.
        assert_eq!(pos[0], [0.0, 3.0, 0.0]);
        assert_eq!(pos[2], [10.0, 3.0, 10.0]);
    }

    /// A dry map yields an empty mesh (the caller then skips registering the water node).
    #[test]
    fn surface_mesh_empty_when_no_water() {
        let wm = Watermap::from_parts(2, 2, 10.0, 0.0, 0.0, vec![-50.0; 4], vec![0u8; 4]);
        let (pos, idx) = wm.surface_mesh();
        assert!(pos.is_empty() && idx.is_empty());
    }

    /// A round-trip through the `watr` byte parser recovers dimensions, height and mask, and centres
    /// the grid on the origin (index 128 → world 0).
    #[test]
    fn parse_watr_bytes_roundtrips_header_and_layers() {
        let (w, h) = (3usize, 3usize);
        let mut buf = Vec::new();
        buf.extend_from_slice(&5u32.to_le_bytes()); // layer_count
        buf.extend_from_slice(&(w as u32).to_le_bytes());
        buf.extend_from_slice(&(h as u32).to_le_bytes());
        buf.extend_from_slice(&32.0f32.to_le_bytes()); // cell
        buf.extend_from_slice(&HEIGHT_MIN_M.to_le_bytes()); // height_min
        buf.extend_from_slice(&325.26f32.to_le_bytes()); // height_max
        buf.extend_from_slice(&64.0f32.to_le_bytes()); // field_b
        buf.extend_from_slice(&0u32.to_le_bytes()); // field_c
        buf.extend_from_slice(&0u32.to_le_bytes()); // field_d
        // Layer 0: centre cell wet at -36, rest dry sentinel.
        let heights = [
            -50.0f32, -50.0, -50.0, -50.0, OPEN_WATER_SURFACE_M, -50.0, -50.0, -50.0, -50.0,
        ];
        for hgt in heights {
            buf.extend_from_slice(&hgt.to_le_bytes());
        }
        // Layer 1: centre cell wet.
        buf.extend_from_slice(&[DRY, DRY, DRY, DRY, WET, DRY, DRY, DRY, DRY]);

        let wm = Watermap::from_watr_bytes(&buf).expect("parse");
        assert_eq!((wm.width(), wm.height()), (3, 3));
        assert_eq!(wm.cell_size(), 32.0);
        // Centred: index 1 (middle of 3) → world 0.
        assert_eq!(wm.origin_x, -32.0);
        let mid = wm.sample(0.0, 0.0);
        assert!(mid.is_water);
        assert_eq!(mid.surface_height, OPEN_WATER_SURFACE_M);
        // A corner cell is dry.
        let corner = wm.sample(-32.0, -32.0);
        assert!(!corner.is_water);
        assert_eq!(corner.surface_height, HEIGHT_MIN_M);
    }

    /// Truncated buffers are rejected, not silently mis-read.
    #[test]
    fn parse_rejects_truncated() {
        assert_eq!(Watermap::from_watr_bytes(&[0u8; 8]), Err(WatermapError::TooShortForHeader));
        let mut hdr = Vec::new();
        hdr.extend_from_slice(&5u32.to_le_bytes());
        hdr.extend_from_slice(&257u32.to_le_bytes());
        hdr.extend_from_slice(&257u32.to_le_bytes());
        hdr.extend_from_slice(&32.0f32.to_le_bytes());
        hdr.extend_from_slice(&[0u8; 20]); // rest of header, no layers
        assert_eq!(Watermap::from_watr_bytes(&hdr), Err(WatermapError::TooShortForLayers));
    }

    /// Nearest-cell mapping + out-of-grid handling: inside the wet uniform map is water; far outside is
    /// dry with the sentinel height.
    #[test]
    fn sample_inside_and_outside_grid() {
        let wm = Watermap::uniform(5, 32.0, OPEN_WATER_SURFACE_M, true);
        assert!(wm.is_water(0.0, 0.0));
        assert_eq!(wm.water_surface_height(0.0, 0.0), Some(OPEN_WATER_SURFACE_M));
        // Way outside the ±64 m footprint of a 5-cell/32 m grid.
        let out = wm.sample(10_000.0, 0.0);
        assert!(!out.is_water);
        assert_eq!(out.surface_height, HEIGHT_MIN_M);
        assert_eq!(wm.water_surface_height(10_000.0, 0.0), None);
    }
}
