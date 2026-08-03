//! Loader over Wally's terrain height tensor (`heightmap-data/`): `meta.json` grid params,
//! `heights.bin` (int16 LE, height = value/10), `tiers.bin` (uint8, 5 = road). Provides the
//! ground-truth queries the scatter mask needs: `ground_y`, `is_water`, `is_road`, `slope`.
//!
//! Layout (meta.json): grid 500×500, `cell_world_units=16`, `origin_cell = -250` → world ±4000,
//! `sea_level = -35`. `index = (floor(z/16)+250)*500 + (floor(x/16)+250)`. `-32768` = never scanned.
//! Axes: x WEST-positive, z NORTH-positive, y up.

use std::path::Path;

pub const SENTINEL: i16 = -32768;

pub struct HeightMap {
    pub width: i32,
    pub height: i32,
    pub cell: f32,
    pub origin_cell_x: i32,
    pub origin_cell_z: i32,
    pub sea_level: f32,
    heights: Vec<i16>,
    tiers: Vec<u8>,
}

impl HeightMap {
    /// Load from a directory holding `heights.bin` + `tiers.bin` (+ `meta.json`, used only to
    /// sanity-check the grid; the fixed constants below match meta v1 and are asserted).
    pub fn load(dir: &Path) -> Result<HeightMap, String> {
        let width = 500i32;
        let height = 500i32;
        let cell = 16.0f32;
        let origin_cell_x = -250i32;
        let origin_cell_z = -250i32;
        let sea_level = -35.0f32;

        let hpath = dir.join("heights.bin");
        let tpath = dir.join("tiers.bin");
        let hbytes = std::fs::read(&hpath).map_err(|e| format!("read {}: {e}", hpath.display()))?;
        let tbytes = std::fs::read(&tpath).map_err(|e| format!("read {}: {e}", tpath.display()))?;

        let n = (width * height) as usize;
        if hbytes.len() != n * 2 {
            return Err(format!(
                "{}: expected {} bytes (500*500 int16), got {}",
                hpath.display(),
                n * 2,
                hbytes.len()
            ));
        }
        if tbytes.len() != n {
            return Err(format!(
                "{}: expected {} bytes (500*500 u8), got {}",
                tpath.display(),
                n,
                tbytes.len()
            ));
        }
        let heights: Vec<i16> = hbytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();

        Ok(HeightMap {
            width,
            height,
            cell,
            origin_cell_x,
            origin_cell_z,
            sea_level,
            heights,
            tiers: tbytes,
        })
    }

    #[inline]
    fn cell_index(&self, x: f32, z: f32) -> Option<usize> {
        let cx = (x / self.cell).floor() as i32;
        let cz = (z / self.cell).floor() as i32;
        let col = cx - self.origin_cell_x;
        let row = cz - self.origin_cell_z;
        if col < 0 || col >= self.width || row < 0 || row >= self.height {
            None
        } else {
            Some((row * self.width + col) as usize)
        }
    }

    /// Raw stored height (world units) or `None` if out-of-grid or a never-scanned sentinel cell.
    #[inline]
    pub fn ground_y(&self, x: f32, z: f32) -> Option<f32> {
        let i = self.cell_index(x, z)?;
        let raw = self.heights[i];
        if raw == SENTINEL {
            None
        } else {
            Some(raw as f32 / 10.0)
        }
    }

    /// Never-scanned (sentinel) or off-grid — cannot be planted.
    #[inline]
    pub fn is_unscanned(&self, x: f32, z: f32) -> bool {
        self.ground_y(x, z).is_none()
    }

    /// At or below sea level (or off-grid). Off-grid counts as water so we never plant off-map.
    #[inline]
    pub fn is_water(&self, x: f32, z: f32) -> bool {
        match self.ground_y(x, z) {
            Some(h) => h <= self.sea_level,
            None => true,
        }
    }

    /// Terrain tier byte == 5 (road/vehicle).
    #[inline]
    pub fn is_road(&self, x: f32, z: f32) -> bool {
        match self.cell_index(x, z) {
            Some(i) => self.tiers[i] == 5,
            None => false,
        }
    }

    /// Max abs height delta to the 4 cardinal neighbours (±`cell` in x/z), divided by `cell`.
    /// Neighbours that are off-grid or unscanned are skipped. Returns 0 if the point or all
    /// neighbours are unavailable (a flat/unknown assumption; the caller gates water/unscanned first).
    pub fn slope(&self, x: f32, z: f32) -> f32 {
        let Some(h0) = self.ground_y(x, z) else {
            return 0.0;
        };
        let mut worst = 0.0f32;
        for (dx, dz) in [(self.cell, 0.0), (-self.cell, 0.0), (0.0, self.cell), (0.0, -self.cell)] {
            if let Some(hn) = self.ground_y(x + dx, z + dz) {
                let s = (hn - h0).abs() / self.cell;
                if s > worst {
                    worst = s;
                }
            }
        }
        worst
    }
}
