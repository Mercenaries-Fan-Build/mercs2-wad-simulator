//! The static watermap — the engine's "is this (x,z) over water and how high is the surface" data.
//!
//! Code map §4 (static waterline) + [`docs/watermap_format.md`](../../../../docs/watermap_format.md):
//! the watermap is a single reflection asset, type hash `0x4D7D30C4` = `pandemic_hash_m2("watermap")`,
//! stored as a `watr` UCFX chunk. It is a **height field + wet mask over a regular XZ grid** — the
//! static half of the water system (the *dynamic* wave displacement is rendered per-frame into the
//! ping-pong `pHeightS` RTs by `PgWaterHeightMapVP/FP`, code map §4; that layer is render-time and is
//! deliberately NOT modelled here — see the crate docs).
//!
//! Recovered layout (`watr`), verified byte-exact against retail `resident_P000_Q3`:
//!
//! ```text
//!   +0   u32  layer_count      = 5
//!   +4   u32  grid_width       = 257          \ the FINE grid
//!   +8   u32  grid_height      = 257          |
//!   +12  f32  cell_size_m      = 32.0         /
//!   +16  f32  height_min_m     = -50.0        \ exact range of Layer 0
//!   +20  f32  height_max_m     = 325.256073   /
//!   +24  f32  coarse_cell_m    = 64.0         \ the COARSE grid (Layer 4)
//!   +28  u32  coarse_width     = 129          |  128 × 64 m = the same 8192 m span
//!   +32  u32  coarse_height    = 129          |
//!   +36  f32  coarse_bias      = -49.831726   |  dequantize: h = bias + v * scale
//!   +40  f32  coarse_scale     = 0.0056970473 /
//!   +44        Layer 0  f32[w*h]  water-surface height, metres, game Y-up   (264,196 B)
//!   +264240    Layer 1  u8 [w*h]  wet mask: 0 = dry/land, 255 = water column ( 66,049 B)
//!   +330289    Layer 2  u8 [w*h]  coastal variant   (hypothesis, not modelled)
//!   +396338    Layer 3  u8 [w*h]  sparse override   (hypothesis, not modelled)
//!   +462387    Layer 4  u16[129²] coarse height     (hypothesis, not modelled)  (33,282 B)
//!   = 495,669 B — the retail chunk size, exactly.
//! ```
//!
//! ## The header is 44 bytes, not 36
//!
//! `docs/watermap_format.md` (and this parser, until it was corrected) called the header 36 bytes and
//! read `+36`/`+40` as "unknown trailing fields", starting Layer 0 at `+36`. That reads two header
//! floats as the first two height samples and **shifts the entire height field two cells against the
//! wet mask**. It is silent — the census still looks plausible — but on retail it put 4,681 cells
//! (7% of the map, all of them coastline) on the wrong side of the waterline: shore cells reported as
//! open water, and water cells reported as land.
//!
//! Four independent facts pin the 44-byte header, all asserted in the tests below:
//! 1. an offset sweep for Layer 1 bottoms out at `+264240` = `44 + 4·w·h`, and nowhere else;
//! 2. at that offset `mask == 255` ⟺ `height != height_min_m` for **all 66,049 cells** (at `+36` it
//!    disagreed on 4,681), and the mask goes cleanly binary — `{0: 27971, 255: 38078}`, no strays;
//! 3. Layer 0's min/max then match the header's `height_min_m`/`height_max_m` *exactly*;
//! 4. the size arithmetic closes with no slack: `44 + 4n + 3n + 2·129² = 495,669`, and the leftover is
//!    exactly a 129×129 `u16` grid — which is what `coarse_cell_m`/`coarse_width`/`coarse_height` at
//!    `+24`..`+32` describe.
//!
//! The `+36`/`+40` pair reads as the coarse layer's dequantization bias and scale: the coarse grid's
//! dominant value (2427, 12,097 of 16,641 cells) decodes to −36.005 m — the open-water plateau, to
//! 5 mm. Strong, but a **hypothesis**; Layer 4 is left unread either way.
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

/// Size of the `watr` fixed header, in bytes — where Layer 0 begins. **44, not 36**: the last two
/// fields (`+36`/`+40`) are header floats, not height samples. See the module docs for the four facts
/// that pin this; reading them as Layer 0 shifts the height field two cells against the wet mask.
pub const HEADER_LEN: usize = 44;

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

    /// Parse a raw `watr` chunk (the payload *after* the 4-byte tag). Reads the 44-byte header +
    /// Layer 0 (f32 heights) + Layer 1 (u8 wet mask); Layers 2–4 are left unread (hypothesis-only —
    /// see the module docs). Uses the centred-origin hypothesis.
    pub fn from_watr_bytes(buf: &[u8]) -> Result<Self, WatermapError> {
        if buf.len() < HEADER_LEN {
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
        let l0 = HEADER_LEN;
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

    /// How many cells carry a water column (Layer 1 == [`WET`]). Retail Maracaibo: 38,070 of 66,049.
    pub fn wet_cell_count(&self) -> usize {
        self.wet.iter().filter(|w| **w == WET).count()
    }

    /// `(min, max)` Layer-0 surface height over the **wet** cells only — the real waterline range,
    /// with the dry `HEIGHT_MIN_M` sentinel excluded. `None` on a map with no water.
    pub fn wet_height_range(&self) -> Option<(f32, f32)> {
        let mut range: Option<(f32, f32)> = None;
        for (h, w) in self.heights.iter().zip(&self.wet) {
            if *w != WET {
                continue;
            }
            range = Some(match range {
                None => (*h, *h),
                Some((lo, hi)) => (lo.min(*h), hi.max(*h)),
            });
        }
        range
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

    /// Whether a world XZ lies inside the grid footprint at all (outside → no water data). The
    /// footprint is the union of the sample *cells*, so it runs half a cell past the outermost sample
    /// on each side — the same extent [`surface_mesh`](Self::surface_mesh) draws.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        let half = self.cell_size * 0.5;
        let min_x = self.origin_x - half;
        let min_z = self.origin_z - half;
        let max_x = self.origin_x + (self.width as f32 - 1.0) * self.cell_size + half;
        let max_z = self.origin_z + (self.height as f32 - 1.0) * self.cell_size + half;
        x >= min_x && x <= max_x && z >= min_z && z <= max_z
    }

    fn idx(&self, ix: usize, iz: usize) -> usize {
        iz * self.width + ix
    }

    /// Build a renderable surface mesh over every **wet** cell: one flat quad per cell at that cell's
    /// Layer-0 surface height, in world space (game Y-up). Returns `(positions, indices)` for a
    /// translucent water pass. Empty when the map has no wet cells. Positions are `[x, y, z]`; indices
    /// are `u32` triangles (two per quad, CCW seen from above).
    ///
    /// Each quad is the sample's own footprint — `±cell_size/2` **centred on** the sample, matching
    /// [`cell_at`](Self::cell_at)'s nearest-sample rounding. Drawing `[ix*cs, (ix+1)*cs]` instead
    /// would put the visible surface half a cell (16 m on retail) off the water the swim/buoyancy
    /// query reports, so the shoreline you see and the shoreline you can swim in would disagree.
    pub fn surface_mesh(&self) -> (Vec<[f32; 3]>, Vec<u32>) {
        let mut pos = Vec::new();
        let mut idx = Vec::new();
        let cs = self.cell_size;
        let half = cs * 0.5;
        for iz in 0..self.height {
            for ix in 0..self.width {
                let i = self.idx(ix, iz);
                if self.wet[i] != WET {
                    continue;
                }
                let h = self.heights[i];
                let x0 = self.origin_x + ix as f32 * cs - half;
                let z0 = self.origin_z + iz as f32 * cs - half;
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
        // The sample at index (0,0) sits at world (0,0), so its quad is CENTRED there: [-5,5]².
        assert_eq!(pos[0], [-5.0, 3.0, -5.0]);
        assert_eq!(pos[2], [5.0, 3.0, 5.0]);
    }

    /// The drawn surface and the queried surface cover the same ground: every quad corner the mesh
    /// emits lies over a cell that [`Watermap::sample`] agrees is water, and each quad is centred on
    /// its sample. A half-cell slip here shows up in-game as a shoreline you can see but not swim in.
    #[test]
    fn surface_mesh_and_sample_agree_on_where_the_water_is() {
        // 3×3 grid, 10 m cells, centred → samples at -10, 0, +10 on each axis. Middle column wet.
        let heights = vec![-50.0, 2.0, -50.0, -50.0, 2.0, -50.0, -50.0, 2.0, -50.0];
        let wet = vec![DRY, WET, DRY, DRY, WET, DRY, DRY, WET, DRY];
        let wm = Watermap::from_parts(3, 3, 10.0, -10.0, -10.0, heights, wet);
        let (pos, _) = wm.surface_mesh();
        for p in &pos {
            // Nudge each corner a hair inward so it lands unambiguously inside its own cell.
            let (cx, cz) = (p[0] * 0.98, p[2] * 0.98);
            let s = wm.sample(cx, cz);
            assert!(s.is_water, "mesh corner ({cx}, {cz}) draws water the query calls dry");
            assert_eq!(s.surface_height, p[1], "drawn height differs from the queried height");
        }
        // And the converse at the cell centres: the dry columns are not drawn.
        for z in [-10.0f32, 0.0, 10.0] {
            assert!(!wm.sample(-10.0, z).is_water, "left column is dry");
            assert!(wm.sample(0.0, z).is_water, "middle column is wet");
        }
    }

    /// A dry map yields an empty mesh (the caller then skips registering the water node).
    #[test]
    fn surface_mesh_empty_when_no_water() {
        let wm = Watermap::from_parts(2, 2, 10.0, 0.0, 0.0, vec![-50.0; 4], vec![0u8; 4]);
        let (pos, idx) = wm.surface_mesh();
        assert!(pos.is_empty() && idx.is_empty());
    }

    /// A retail-shaped `watr` header: the full 44 bytes, in the recovered field order.
    fn watr_header(w: usize, h: usize, cell: f32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&5u32.to_le_bytes()); // +0  layer_count
        buf.extend_from_slice(&(w as u32).to_le_bytes()); // +4  grid_width
        buf.extend_from_slice(&(h as u32).to_le_bytes()); // +8  grid_height
        buf.extend_from_slice(&cell.to_le_bytes()); // +12 cell_size_m
        buf.extend_from_slice(&HEIGHT_MIN_M.to_le_bytes()); // +16 height_min_m
        buf.extend_from_slice(&325.256073f32.to_le_bytes()); // +20 height_max_m
        buf.extend_from_slice(&(cell * 2.0).to_le_bytes()); // +24 coarse_cell_m
        buf.extend_from_slice(&(w as u32 / 2 + 1).to_le_bytes()); // +28 coarse_width
        buf.extend_from_slice(&(h as u32 / 2 + 1).to_le_bytes()); // +32 coarse_height
        buf.extend_from_slice(&(-49.831726f32).to_le_bytes()); // +36 coarse dequant bias
        buf.extend_from_slice(&0.0056970473f32.to_le_bytes()); // +40 coarse dequant scale
        assert_eq!(buf.len(), HEADER_LEN);
        buf
    }

    /// A round-trip through the `watr` byte parser recovers dimensions, height and mask, and centres
    /// the grid on the origin (index 128 → world 0).
    #[test]
    fn parse_watr_bytes_roundtrips_header_and_layers() {
        let (w, h) = (3usize, 3usize);
        let mut buf = watr_header(w, h, 32.0);
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
        // A 36-byte buffer is now short of the header too — that is the whole point of the fix.
        assert_eq!(Watermap::from_watr_bytes(&[0u8; 36]), Err(WatermapError::TooShortForHeader));
        let hdr = watr_header(GRID_DIM, GRID_DIM, CELL_SIZE_M); // header, no layers
        assert_eq!(Watermap::from_watr_bytes(&hdr), Err(WatermapError::TooShortForLayers));
    }

    /// Layer 0 starts at `HEADER_LEN`, and reading it from the old 36-byte offset shifts every height
    /// two cells against the wet mask. This is the retail bug in miniature: build a chunk whose mask
    /// and heights agree exactly, and check the parser reproduces that agreement cell-for-cell.
    #[test]
    fn layer0_starts_after_the_full_44_byte_header() {
        let (w, h) = (5usize, 5usize);
        let mut buf = watr_header(w, h, 32.0);
        // A diagonal shoreline: wet where (ix + iz) is even. Heights follow the mask exactly, which is
        // the invariant retail satisfies for all 66,049 cells.
        let mut heights = Vec::new();
        let mut mask = Vec::new();
        for iz in 0..h {
            for ix in 0..w {
                let wet = (ix + iz) % 2 == 0;
                heights.push(if wet { OPEN_WATER_SURFACE_M } else { HEIGHT_MIN_M });
                mask.push(if wet { WET } else { DRY });
            }
        }
        for hgt in &heights {
            buf.extend_from_slice(&hgt.to_le_bytes());
        }
        buf.extend_from_slice(&mask);

        let wm = Watermap::from_watr_bytes(&buf).expect("parse");
        for i in 0..w * h {
            let (ix, iz) = (i % w, i / w);
            let x = wm.origin_x + ix as f32 * wm.cell_size();
            let z = wm.origin_z + iz as f32 * wm.cell_size();
            let s = wm.sample(x, z);
            assert_eq!(s.is_water, mask[i] == WET, "cell ({ix},{iz}) wet flag");
            assert_eq!(s.surface_height, heights[i], "cell ({ix},{iz}) height");
            // The recovered invariant: the mask and the sentinel never disagree.
            assert_eq!(s.is_water, s.surface_height != HEIGHT_MIN_M);
        }
    }

    /// The retail chunk's size closes exactly on the recovered layout — 44-byte header, three
    /// full-resolution layers plus the f32 height field, and a 129×129 `u16` coarse grid. No slack,
    /// which is the arithmetic that rules the 36-byte header out.
    #[test]
    fn retail_layout_accounts_for_every_byte() {
        const RETAIL_WATR_LEN: usize = 495_669;
        const COARSE_DIM: usize = 129;
        let n = GRID_DIM * GRID_DIM;
        let total = HEADER_LEN            // header
            + n * 4                       // Layer 0: f32 heights
            + n                           // Layer 1: u8 wet mask
            + n                           // Layer 2: u8 (coastal variant, hypothesis)
            + n                           // Layer 3: u8 (sparse override, hypothesis)
            + COARSE_DIM * COARSE_DIM * 2; // Layer 4: u16 coarse height (hypothesis)
        assert_eq!(total, RETAIL_WATR_LEN);
        // The coarse grid spans the same world extent as the fine one: 128 × 64 m = 256 × 32 m.
        assert_eq!(
            (COARSE_DIM - 1) as f32 * (CELL_SIZE_M * 2.0),
            (GRID_DIM - 1) as f32 * CELL_SIZE_M
        );
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
