//! CPU collision over a raw world-space triangle soup — a capsule character controller + camera-boom
//! raycast operating directly on `&[[Vec3; 3]]`, no owning world object required.
//!
//! Folded here from `mercs2_game::collision` (the game owns *content*; the engine/physics owns the
//! *mechanism*). This is the BBOX-culled variant: the broad phase culls by each triangle's bounding box,
//! not the distance to one vertex — a large floor/wall triangle a player stands in the middle of is kept
//! (the "fell through the floor after moving" fix). It complements [`crate::StaticSoupPhysics`] (the
//! `PhysicsQuery` seam for the vehicle/combat/anim systems); this module is the lightweight direct-soup API
//! the on-foot player controller + camera boom use.
//!
//! The player is a vertical CAPSULE (a core segment from `feet+radius` to `feet+height-radius`, swept by
//! `radius`). Movement is **collide-and-slide**: attempt the move, then depenetrate the capsule out of
//! WALL triangles — pushing perpendicular to each contact preserves the tangential motion, i.e. the
//! capsule slides along walls. FLOORS are handled separately by a **downward ground probe** that places
//! the feet on the surface underneath (within a step tolerance), so stairs, ramps and thresholds all
//! work: a step shorter than the capsule radius is cleared by the bottom sphere with no special case,
//! and taller steps within `step` are climbed/descended by the ground probe. This mirrors how the retail
//! engine used Havok capsule-vs-geometry (`MatchCapsuleToPose`) rather than a heightmap.
//!
//! The camera boom uses the same soup via `raycast` (a thick spherecast margin), matching the exe's
//! `CameraCollisionCastRay` (a radius² probe that keeps the camera out of geometry).

use mercs2_core::glam::Vec3;

/// A triangle is a WALL if its normal is more horizontal than vertical (steep surface). Walls block +
/// slide; walkable surfaces (floors/ramps) are left to the ground probe.
fn is_wall(t: &[Vec3; 3]) -> bool {
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    let nl = n.length();
    nl > 1e-6 && (n.y / nl).abs() < 0.5
}

/// Axis-aligned bounding box of a triangle (min, max). The broad-phase MUST cull by the triangle's
/// bbox, not the distance to one vertex: a big floor/wall triangle's vertices can be far from a player
/// standing in its middle, so a vertex-distance cull wrongly drops the geometry the player is on/against
/// (the "fell through the floor after moving" bug).
#[inline]
fn tri_bbox(t: &[Vec3; 3]) -> (Vec3, Vec3) {
    (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]))
}

/// Does `pos.xz` fall within the triangle's XZ bbox expanded by `margin`? Broad-phase for the downward
/// ground probes (the exact ray-tri test still runs on survivors).
#[inline]
fn xz_in_tri_bbox(t: &[Vec3; 3], pos: Vec3, margin: f32) -> bool {
    let (b0, b1) = tri_bbox(t);
    pos.x >= b0.x - margin && pos.x <= b1.x + margin && pos.z >= b0.z - margin && pos.z <= b1.z + margin
}

// ---------------------------------------------------------------------------
//   Broadphase — a uniform XZ spatial grid over the triangle soup
// ---------------------------------------------------------------------------
//
// The retail engine broadphases collision with a `hkpMoppBvTreeShape` BVH, so a query touches
// `O(log n)` nodes instead of the whole soup. This is the faithful *acceleration* of that intent: a
// uniform grid over the world's XZ plane (the terrain/tile axis) buckets every triangle into the cells
// its bounding box overlaps. A ground probe then tests only the cell(s) under the feet, a ray only the
// cells it traverses (grid DDA), and a swept capsule only the cells it covers — `O(local)` instead of
// `O(all triangles)`. The exact per-triangle math (bbox cull + Möller–Trumbore / closest-point) is
// unchanged and still runs on the survivors, so the RESULT is bit-for-bit identical to the linear scan;
// only the *number of triangles visited* shrinks. See `DEFERRED.md` (Broadphase / MOPP BV-tree).

/// Nominal cell size (metres) — roughly the streamed terrain-tile detail scale, so a ground probe or a
/// swept-capsule step touches one or a handful of cells.
const TARGET_CELL: f32 = 12.0;
/// Hard cap on the cell count so a very large world stays memory-reasonable (the cell size is grown to
/// fit if needed). `1<<20` cells → the `cell_start` prefix array is ~4 MB worst case.
const MAX_CELLS: usize = 1 << 20;
/// A triangle whose XZ bbox spans more cells than this goes in the always-tested `oversized` bucket
/// (rather than being replicated into every cell) — bounds memory against a huge ground/wall triangle.
const OVERSIZE_CELLS: usize = 64;

/// A uniform XZ spatial hash over a triangle soup: `items` holds triangle indices grouped by cell
/// (CSR layout via `cell_start`), plus an `oversized` bucket of triangles too large to bucket. Built
/// once per distinct soup and reused across every query against it (see [`with_grid`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct Grid {
    min_x: f32,
    min_z: f32,
    cell: f32,
    inv_cell: f32,
    nx: usize,
    nz: usize,
    /// CSR offsets: cell `c`'s triangle indices are `items[cell_start[c]..cell_start[c+1]]`. Length
    /// `nx*nz + 1` (empty grid: empty).
    cell_start: Vec<u32>,
    items: Vec<u32>,
    /// Triangles spanning more than [`OVERSIZE_CELLS`] cells — always tested (never bucketed).
    oversized: Vec<u32>,
}

impl Grid {
    /// Bucket every triangle into the cells its XZ bounding box overlaps. `O(n)` once; amortised away
    /// by the per-soup cache. Rebuilt whenever the soup changes (a new slice identity — see [`SoupKey`]).
    pub(crate) fn build(tris: &[[Vec3; 3]]) -> Grid {
        if tris.is_empty() {
            return Grid::default();
        }
        let (mut min_x, mut min_z) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_z) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for t in tris {
            for v in t {
                min_x = min_x.min(v.x);
                max_x = max_x.max(v.x);
                min_z = min_z.min(v.z);
                max_z = max_z.max(v.z);
            }
        }
        if !(min_x.is_finite() && min_z.is_finite() && max_x.is_finite() && max_z.is_finite()) {
            // NaN/inf vertices: fall back to "everything is oversized" (correct, just unaccelerated).
            return Grid { oversized: (0..tris.len() as u32).collect(), ..Grid::default() };
        }
        let span_x = (max_x - min_x).max(1e-3);
        let span_z = (max_z - min_z).max(1e-3);
        // Grow the cell if the world is so large the grid would blow the cell-count cap.
        let mut cell = TARGET_CELL;
        loop {
            let nx = (span_x / cell) as usize + 1;
            let nz = (span_z / cell) as usize + 1;
            if nx.saturating_mul(nz) <= MAX_CELLS || !cell.is_finite() {
                break;
            }
            cell *= 2.0;
        }
        let inv_cell = 1.0 / cell;
        let nx = ((span_x / cell) as usize + 1).max(1);
        let nz = ((span_z / cell) as usize + 1).max(1);
        let ncells = nx * nz;

        let cx = |x: f32| (((x - min_x) * inv_cell) as isize).clamp(0, nx as isize - 1) as usize;
        let cz = |z: f32| (((z - min_z) * inv_cell) as isize).clamp(0, nz as isize - 1) as usize;
        let tri_range = |t: &[Vec3; 3]| {
            let (b0, b1) = tri_bbox(t);
            (cx(b0.x), cx(b1.x), cz(b0.z), cz(b1.z))
        };

        // Pass 1: count per-cell occupancy (and collect oversized).
        let mut counts = vec![0u32; ncells];
        let mut oversized: Vec<u32> = Vec::new();
        for (i, t) in tris.iter().enumerate() {
            let (cx0, cx1, cz0, cz1) = tri_range(t);
            if (cx1 - cx0 + 1) * (cz1 - cz0 + 1) > OVERSIZE_CELLS {
                oversized.push(i as u32);
                continue;
            }
            for zc in cz0..=cz1 {
                let base = zc * nx;
                for xc in cx0..=cx1 {
                    counts[base + xc] += 1;
                }
            }
        }
        // Prefix-sum into CSR offsets.
        let mut cell_start = vec![0u32; ncells + 1];
        for c in 0..ncells {
            cell_start[c + 1] = cell_start[c] + counts[c];
        }
        // Pass 2: scatter triangle indices into their cells.
        let mut items = vec![0u32; cell_start[ncells] as usize];
        let mut cursor: Vec<u32> = cell_start[..ncells].to_vec();
        for (i, t) in tris.iter().enumerate() {
            let (cx0, cx1, cz0, cz1) = tri_range(t);
            if (cx1 - cx0 + 1) * (cz1 - cz0 + 1) > OVERSIZE_CELLS {
                continue;
            }
            for zc in cz0..=cz1 {
                let base = zc * nx;
                for xc in cx0..=cx1 {
                    let c = base + xc;
                    items[cursor[c] as usize] = i as u32;
                    cursor[c] += 1;
                }
            }
        }
        Grid { min_x, min_z, cell, inv_cell, nx, nz, cell_start, items, oversized }
    }

    #[inline]
    fn clamp_cx(&self, x: f32) -> usize {
        (((x - self.min_x) * self.inv_cell) as isize).clamp(0, self.nx as isize - 1) as usize
    }
    #[inline]
    fn clamp_cz(&self, z: f32) -> usize {
        (((z - self.min_z) * self.inv_cell) as isize).clamp(0, self.nz as isize - 1) as usize
    }

    /// Collect (sorted, de-duplicated) the indices of every triangle bucketed into any cell overlapping
    /// the XZ rectangle `[x0,x1]×[z0,z1]`, plus the oversized bucket. A *superset* of every triangle
    /// whose XZ bbox meets the rectangle, so a caller re-running the exact bbox test gets identical hits.
    /// De-dup matters: a triangle in two queried cells must be visited once (else a depenetration would
    /// double-count it).
    pub(crate) fn gather_rect(&self, out: &mut Vec<u32>, x0: f32, x1: f32, z0: f32, z1: f32) {
        out.clear();
        if self.nx > 0 && self.nz > 0 {
            let gxmax = self.min_x + self.nx as f32 * self.cell;
            let gzmax = self.min_z + self.nz as f32 * self.cell;
            if x1 >= self.min_x && x0 <= gxmax && z1 >= self.min_z && z0 <= gzmax {
                let (cx0, cx1) = (self.clamp_cx(x0), self.clamp_cx(x1));
                let (cz0, cz1) = (self.clamp_cz(z0), self.clamp_cz(z1));
                for zc in cz0..=cz1 {
                    let base = zc * self.nx;
                    for xc in cx0..=cx1 {
                        let c = base + xc;
                        out.extend_from_slice(&self.items[self.cell_start[c] as usize..self.cell_start[c + 1] as usize]);
                    }
                }
            }
        }
        out.extend_from_slice(&self.oversized);
        out.sort_unstable();
        out.dedup();
    }

    /// Collect (sorted, de-duplicated) the indices of every triangle in the cells the segment `[o,end]`
    /// traverses in XZ (via a grid DDA), plus the oversized bucket. A *superset* of every triangle the
    /// segment can actually intersect: any hit point lies in the triangle's XZ bbox, which sits in a
    /// cell the segment's XZ projection crosses — so a caller re-running the exact ray/tri test gets the
    /// identical nearest hit.
    pub(crate) fn gather_ray(&self, out: &mut Vec<u32>, o: Vec3, end: Vec3) {
        out.clear();
        if self.nx > 0 && self.nz > 0 {
            self.dda_xz(out, o.x, o.z, end.x, end.z);
        }
        out.extend_from_slice(&self.oversized);
        out.sort_unstable();
        out.dedup();
    }

    /// Amanatides–Woo grid traversal of the XZ segment, appending each visited cell's triangle indices
    /// (raw; the caller de-dups). The segment is first clipped to the grid's XZ bounds.
    fn dda_xz(&self, out: &mut Vec<u32>, ox: f32, oz: f32, ex: f32, ez: f32) {
        let gxmin = self.min_x;
        let gxmax = self.min_x + self.nx as f32 * self.cell;
        let gzmin = self.min_z;
        let gzmax = self.min_z + self.nz as f32 * self.cell;
        let (dx, dz) = (ex - ox, ez - oz);
        let mut t0 = 0.0f32;
        let mut t1 = 1.0f32;
        if !clip_slab(ox, dx, gxmin, gxmax, &mut t0, &mut t1) {
            return;
        }
        if !clip_slab(oz, dz, gzmin, gzmax, &mut t0, &mut t1) {
            return;
        }
        let nx = self.nx as isize;
        let nz = self.nz as isize;
        let mut cx = self.clamp_cx(ox + dx * t0) as isize;
        let mut cz = self.clamp_cz(oz + dz * t0) as isize;
        let ecx = self.clamp_cx(ox + dx * t1) as isize;
        let ecz = self.clamp_cz(oz + dz * t1) as isize;
        let step_x: isize = (dx > 0.0) as isize - (dx < 0.0) as isize;
        let step_z: isize = (dz > 0.0) as isize - (dz < 0.0) as isize;
        let (mut t_max_x, t_delta_x) = axis_step(ox, dx, cx, step_x, self.min_x, self.cell);
        let (mut t_max_z, t_delta_z) = axis_step(oz, dz, cz, step_z, self.min_z, self.cell);
        let mut guard = self.nx + self.nz + 4;
        loop {
            let c = (cz * nx + cx) as usize;
            out.extend_from_slice(&self.items[self.cell_start[c] as usize..self.cell_start[c + 1] as usize]);
            if (cx == ecx && cz == ecz) || guard == 0 {
                break;
            }
            guard -= 1;
            if t_max_x <= t_max_z {
                cx += step_x;
                t_max_x += t_delta_x;
                if cx < 0 || cx >= nx {
                    break;
                }
            } else {
                cz += step_z;
                t_max_z += t_delta_z;
                if cz < 0 || cz >= nz {
                    break;
                }
            }
        }
    }
}

/// Liang–Barsky slab clip of the ray `o + t·d` against `[lo,hi]`; narrows `[t0,t1]` to the overlap.
/// Returns `false` when the segment misses the slab entirely.
fn clip_slab(o: f32, d: f32, lo: f32, hi: f32, t0: &mut f32, t1: &mut f32) -> bool {
    if d.abs() < 1e-12 {
        return o >= lo && o <= hi;
    }
    let inv = 1.0 / d;
    let (mut ta, mut tb) = ((lo - o) * inv, (hi - o) * inv);
    if ta > tb {
        std::mem::swap(&mut ta, &mut tb);
    }
    *t0 = t0.max(ta);
    *t1 = t1.min(tb);
    *t0 <= *t1
}

/// For an Amanatides–Woo step: the segment parameter `t` at which the ray leaves cell `c` along `step`,
/// and the per-cell `t` increment. `step == 0` (axis-aligned) → never crosses (`+∞`).
fn axis_step(o: f32, d: f32, c: isize, step: isize, min: f32, cell: f32) -> (f32, f32) {
    if step == 0 {
        return (f32::INFINITY, f32::INFINITY);
    }
    let boundary = min + (c + (step > 0) as isize) as f32 * cell;
    ((boundary - o) / d, (cell / d.abs()).abs())
}

/// Identity of a soup slice: base pointer + length + a cheap content fingerprint. Two calls with the
/// same `SoupKey` are treated as the same soup, so the [`Grid`] is built once and reused; a
/// streaming block load/unload replaces the soup `Vec` (new pointer/length/content → new key → rebuild).
/// The fingerprint guards the rare case of a reused allocation at the same address and length.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SoupKey {
    ptr: usize,
    len: usize,
    fp: u64,
}

impl SoupKey {
    fn of(tris: &[[Vec3; 3]]) -> SoupKey {
        let len = tris.len();
        let mut fp = len as u64;
        if len > 0 {
            for v in [tris[0][0], tris[0][2], tris[len - 1][1], tris[len / 2][0]] {
                fp = fp.rotate_left(17)
                    ^ (v.x.to_bits() as u64)
                    ^ ((v.y.to_bits() as u64) << 21)
                    ^ ((v.z.to_bits() as u64) << 42);
            }
        }
        SoupKey { ptr: tris.as_ptr() as usize, len, fp }
    }
}

/// Number of distinct soups whose grids are cached per thread. The game queries a single soup, so one
/// slot suffices; a few slots absorb incidental multi-soup use (e.g. tests) without thrashing.
const CACHE_SLOTS: usize = 4;

thread_local! {
    static GRID_CACHE: std::cell::RefCell<GridCache> = std::cell::RefCell::new(GridCache::default());
}

#[derive(Default)]
struct GridCache {
    /// `(key, grid)` slots, most-recently-used last.
    slots: Vec<(SoupKey, Grid)>,
    /// Reusable candidate-index scratch (avoids a per-query allocation on the hot path).
    scratch: Vec<u32>,
}

/// Run `f` with the broadphase [`Grid`] for `tris` and a scratch buffer. The grid is built on first use
/// of a given soup and reused for every subsequent query against it (across frames), so the per-frame
/// queries pay only the `O(local)` traversal, not an `O(n)` rebuild. The grid is discarded and rebuilt
/// when the soup slice's identity changes ([`SoupKey`]) — i.e. when world streaming swaps the soup.
fn with_grid<R>(tris: &[[Vec3; 3]], f: impl FnOnce(&Grid, &mut Vec<u32>) -> R) -> R {
    GRID_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let key = SoupKey::of(tris);
        match c.slots.iter().position(|(k, _)| *k == key) {
            Some(i) => {
                // Promote to most-recently-used.
                let entry = c.slots.remove(i);
                c.slots.push(entry);
            }
            None => {
                let grid = Grid::build(tris);
                if c.slots.len() >= CACHE_SLOTS {
                    c.slots.remove(0);
                }
                c.slots.push((key, grid));
            }
        }
        let last = c.slots.len() - 1;
        let GridCache { slots, scratch } = &mut *c;
        f(&slots[last].1, scratch)
    })
}

// ---------------------------------------------------------------------------
//   Incremental broadphase — a MUTABLE spatial hash over a per-unit triangle soup
// ---------------------------------------------------------------------------
//
// The [`Grid`] above is immutable/CSR: built once per soup and thrown away when the soup changes. That
// is the wrong shape for STREAMING, where a prop/building wakes or hibernates every few frames: rebuilding
// the whole grid (and re-cloning the whole soup) each delta is `O(all tris)` per streaming event.
//
// [`IncrementalGrid`] is the persistent, mutable equivalent — the faithful stand-in for how retail's
// `hkpWorld` adds/removes each body's pre-baked shape from a persistent broadphase
// (`hkpWorld::addEntity` / `removeEntity`), never rebuilding. Triangles are owned in a compact `tris`
// buffer and tagged by a streamed-**unit** key (a block or prop id). [`insert_unit`](IncrementalGrid::insert_unit)
// appends a unit's tris and buckets ONLY those into the cells they overlap; [`remove_unit`](IncrementalGrid::remove_unit)
// deletes exactly that unit's tris (compacting via swap-remove) and touches ONLY the cells they occupied.
// So a wake/hibernate costs `O(that unit's tris)`, not `O(all tris)`.
//
// The broadphase is an **unbounded spatial hash** (integer cell coords → bucket) rather than the CSR
// grid's fixed extent, because the resident set grows/shrinks and moves as the player streams across the
// world — there is no single up-front bbox. The exact per-triangle math run on the gathered survivors is
// the SAME bbox-culled test the immutable path uses, so query results are identical to a linear scan of
// the currently-resident tris (proved by `incremental_matches_bruteforce_*`).

/// A persistent, mutable uniform spatial hash over a per-unit world-space triangle soup. Supports
/// `O(changed-unit)` [`insert_unit`](Self::insert_unit) / [`remove_unit`](Self::remove_unit) and the same
/// `gather_rect` / `gather_ray` broadphase the immutable [`Grid`] exposes. Owns its triangles in a compact
/// [`tris`](Self::tris) buffer (kept dense by swap-remove) so it also serves the raw `&[[Vec3;3]]` the
/// free-function soup consumers still take.
#[derive(Clone, Debug)]
pub struct IncrementalGrid {
    cell: f32,
    inv_cell: f32,
    /// Occupied cells only: integer cell coord `(cx,cz)` → the indices (into `tris`) bucketed there.
    buckets: std::collections::HashMap<(i32, i32), Vec<u32>>,
    /// Triangles whose XZ bbox spans more than [`OVERSIZE_CELLS`] cells — always tested (never bucketed).
    oversized: Vec<u32>,
    /// Compact triangle storage (append on insert, swap-remove on remove). `tris()` hands this slice to the
    /// free-function consumers unchanged.
    tris: Vec<[Vec3; 3]>,
    /// Parallel to `tris`: the unit key that owns each triangle (needed to fix up the moved tri on a
    /// swap-remove).
    unit_of: Vec<u64>,
    /// Per-unit list of the indices (into `tris`) a unit currently owns — the removal set for `remove_unit`.
    units: std::collections::HashMap<u64, Vec<u32>>,
}

impl Default for IncrementalGrid {
    fn default() -> Self {
        IncrementalGrid {
            cell: TARGET_CELL,
            inv_cell: 1.0 / TARGET_CELL,
            buckets: std::collections::HashMap::new(),
            oversized: Vec::new(),
            tris: Vec::new(),
            unit_of: Vec::new(),
            units: std::collections::HashMap::new(),
        }
    }
}

impl IncrementalGrid {
    /// An empty grid with the default [`TARGET_CELL`] cell size.
    pub fn new() -> Self {
        Self::default()
    }

    /// The compact resident triangle buffer (world space). Handed to the free-function soup consumers
    /// (`soup::raycast` / `move_character` / `ground_below`) that still take a raw slice.
    #[inline]
    pub fn tris(&self) -> &[[Vec3; 3]] {
        &self.tris
    }

    /// Number of resident triangles.
    #[inline]
    pub fn len(&self) -> usize {
        self.tris.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Number of resident units (streamed blocks/props + any batch unit).
    #[inline]
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    /// Drop every unit and triangle (used by the batch `set_tris` reset path).
    pub fn clear(&mut self) {
        self.buckets.clear();
        self.oversized.clear();
        self.tris.clear();
        self.unit_of.clear();
        self.units.clear();
    }

    #[inline]
    fn cx(&self, x: f32) -> i32 {
        (x * self.inv_cell).floor() as i32
    }
    #[inline]
    fn cz(&self, z: f32) -> i32 {
        (z * self.inv_cell).floor() as i32
    }

    /// The inclusive cell range `(cx0,cx1,cz0,cz1)` a triangle's XZ bbox overlaps.
    #[inline]
    fn tri_cells(&self, t: &[Vec3; 3]) -> (i32, i32, i32, i32) {
        let (b0, b1) = tri_bbox(t);
        (self.cx(b0.x), self.cx(b1.x), self.cz(b0.z), self.cz(b1.z))
    }

    /// Add index `i` (whose geometry is `tris[i]`) to every cell its bbox overlaps, or to `oversized`.
    fn bucket_index(&mut self, i: u32) {
        let (cx0, cx1, cz0, cz1) = self.tri_cells(&self.tris[i as usize]);
        let span = (cx1 - cx0 + 1) as i64 * (cz1 - cz0 + 1) as i64;
        if span > OVERSIZE_CELLS as i64 {
            self.oversized.push(i);
            return;
        }
        for zc in cz0..=cz1 {
            for xc in cx0..=cx1 {
                self.buckets.entry((xc, zc)).or_default().push(i);
            }
        }
    }

    /// Remove index `i` (whose geometry is still `tris[i]`) from every cell it was bucketed into.
    fn unbucket_index(&mut self, i: u32) {
        let (cx0, cx1, cz0, cz1) = self.tri_cells(&self.tris[i as usize]);
        let span = (cx1 - cx0 + 1) as i64 * (cz1 - cz0 + 1) as i64;
        if span > OVERSIZE_CELLS as i64 {
            if let Some(p) = self.oversized.iter().position(|&x| x == i) {
                self.oversized.swap_remove(p);
            }
            return;
        }
        for zc in cz0..=cz1 {
            for xc in cx0..=cx1 {
                if let Some(v) = self.buckets.get_mut(&(xc, zc)) {
                    if let Some(p) = v.iter().position(|&x| x == i) {
                        v.swap_remove(p);
                    }
                    if v.is_empty() {
                        self.buckets.remove(&(xc, zc));
                    }
                }
            }
        }
    }

    /// Insert (or replace) a streamed unit's world-space triangles, keyed by `key`. Only the cells the new
    /// triangles overlap are touched — `O(tris.len())`, independent of the resident soup size. Re-inserting
    /// an existing key first removes its old triangles (idempotent WAKE).
    pub fn insert_unit(&mut self, key: u64, tris: &[[Vec3; 3]]) {
        if self.units.contains_key(&key) {
            self.remove_unit(key);
        }
        if tris.is_empty() {
            self.units.insert(key, Vec::new());
            return;
        }
        let mut idxs = Vec::with_capacity(tris.len());
        for t in tris {
            let i = self.tris.len() as u32;
            self.tris.push(*t);
            self.unit_of.push(key);
            self.bucket_index(i);
            idxs.push(i);
        }
        self.units.insert(key, idxs);
    }

    /// Remove a streamed unit's triangles by key (HIBERNATE / UNLOAD). Touches only the cells that unit's
    /// triangles occupied plus the one swap-moved survivor per removed triangle — `O(that unit's tris)`.
    /// A no-op for an absent key.
    pub fn remove_unit(&mut self, key: u64) {
        let Some(mut idxs) = self.units.remove(&key) else {
            return;
        };
        // Descending: swap-removing the highest index first guarantees the element swapped in from the
        // tail is always a survivor (never another index still queued for removal), so its bookkeeping fix
        // is a single rewrite.
        idxs.sort_unstable_by(|a, b| b.cmp(a));
        for i in idxs {
            self.remove_index(i);
        }
    }

    /// Swap-remove one triangle index, keeping `tris` dense and the grid/units bookkeeping consistent.
    fn remove_index(&mut self, i: u32) {
        let iu = i as usize;
        let last = self.tris.len() - 1;
        // Drop the removed triangle's own cell references.
        self.unbucket_index(i);
        if iu != last {
            // The tail survivor moves into slot `i`: pull its refs at `last`, move it, re-bucket at `i`.
            self.unbucket_index(last as u32);
            self.tris.swap(iu, last);
            self.unit_of.swap(iu, last);
            let owner = self.unit_of[iu];
            if let Some(v) = self.units.get_mut(&owner) {
                for e in v.iter_mut() {
                    if *e == last as u32 {
                        *e = i;
                    }
                }
            }
            self.bucket_index(i);
        }
        self.tris.pop();
        self.unit_of.pop();
    }

    /// Broadphase: gather (sorted, de-duplicated) every resident triangle index whose cell overlaps the XZ
    /// rectangle `[x0,x1]×[z0,z1]`, plus the oversized bucket. A superset of the immutable [`Grid::gather_rect`]
    /// result (identical exact-test survivors), so callers get identical hits. Falls back to a full scan when
    /// the rectangle spans more cells than there are triangles (cheaper, still a correct superset).
    pub(crate) fn gather_rect(&self, out: &mut Vec<u32>, x0: f32, x1: f32, z0: f32, z1: f32) {
        out.clear();
        let (cx0, cx1) = (self.cx(x0), self.cx(x1));
        let (cz0, cz1) = (self.cz(z0), self.cz(z1));
        let cells = (cx1 - cx0 + 1).max(0) as i64 * (cz1 - cz0 + 1).max(0) as i64;
        if cells > (self.tris.len() as i64).max(64) {
            out.extend(0..self.tris.len() as u32);
            return; // already sorted + unique + covers oversized
        }
        for zc in cz0..=cz1 {
            for xc in cx0..=cx1 {
                if let Some(v) = self.buckets.get(&(xc, zc)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.extend_from_slice(&self.oversized);
        out.sort_unstable();
        out.dedup();
    }

    /// Broadphase: gather (sorted, de-duplicated) every resident triangle index in the cells the segment
    /// `[o,end]` traverses in XZ (grid DDA), plus the oversized bucket — the mutable equivalent of
    /// [`Grid::gather_ray`].
    pub(crate) fn gather_ray(&self, out: &mut Vec<u32>, o: Vec3, end: Vec3) {
        out.clear();
        self.dda_xz(out, o.x, o.z, end.x, end.z);
        out.extend_from_slice(&self.oversized);
        out.sort_unstable();
        out.dedup();
    }

    /// Amanatides–Woo traversal of the XZ segment over the (originless) hash cells, appending each visited
    /// cell's triangle indices. The Manhattan distance to the end cell strictly decreases each step, so the
    /// guard is exact.
    fn dda_xz(&self, out: &mut Vec<u32>, ox: f32, oz: f32, ex: f32, ez: f32) {
        let (dx, dz) = (ex - ox, ez - oz);
        let mut cx = self.cx(ox);
        let mut cz = self.cz(oz);
        let ecx = self.cx(ex);
        let ecz = self.cz(ez);
        let step_x: i32 = (dx > 0.0) as i32 - (dx < 0.0) as i32;
        let step_z: i32 = (dz > 0.0) as i32 - (dz < 0.0) as i32;
        let (mut t_max_x, t_delta_x) = hash_axis_step(ox, dx, cx, step_x, self.cell);
        let (mut t_max_z, t_delta_z) = hash_axis_step(oz, dz, cz, step_z, self.cell);
        let mut guard = (ecx - cx).abs() + (ecz - cz).abs() + 4;
        loop {
            if let Some(v) = self.buckets.get(&(cx, cz)) {
                out.extend_from_slice(v);
            }
            if (cx == ecx && cz == ecz) || guard == 0 {
                break;
            }
            guard -= 1;
            if t_max_x <= t_max_z {
                cx += step_x;
                t_max_x += t_delta_x;
            } else {
                cz += step_z;
                t_max_z += t_delta_z;
            }
        }
    }

    // --- query methods (identical per-triangle math to the free functions, over the gathered survivors) ---

    /// Nearest triangle hit along `[o, o+dir*max_t]` — the [`raycast`] equivalent over the resident soup.
    pub fn raycast(&self, o: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let end = o + dir * max_t;
        let (smin, smax) = (o.min(end), o.max(end));
        let mut cand = Vec::new();
        self.gather_ray(&mut cand, o, end);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &self.tris[i as usize];
            let (b0, b1) = tri_bbox(t);
            if b1.x < smin.x || b0.x > smax.x || b1.y < smin.y || b0.y > smax.y || b1.z < smin.z || b0.z > smax.z {
                continue;
            }
            if let Some(d) = ray_tri(o, dir, t[0], t[1], t[2]) {
                if d <= max_t && best.map_or(true, |b| d < b) {
                    best = Some(d);
                }
            }
        }
        best
    }

    /// Highest walkable surface at or below `pos.y` within `max_drop` — the [`ground_below`] equivalent.
    pub fn ground_below(&self, pos: Vec3, radius: f32, max_drop: f32) -> Option<f32> {
        let origin = pos + Vec3::Y * 0.1;
        let max_t = max_drop + 0.1;
        let mut cand = Vec::new();
        self.gather_rect(&mut cand, pos.x - radius, pos.x + radius, pos.z - radius, pos.z + radius);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &self.tris[i as usize];
            if is_wall(t) || !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    }

    /// Swept player move + optional ground snap — the [`move_character`] equivalent over the resident soup.
    pub fn move_character(&self, feet: Vec3, horiz_move: Vec3, radius: f32, height: f32, step: f32, follow_ground: bool) -> Vec3 {
        let mut pos = feet + Vec3::new(horiz_move.x, 0.0, horiz_move.z);
        pos = self.depenetrate(pos, radius, height);
        if follow_ground {
            if let Some(gy) = self.ground_y(pos, radius, step) {
                pos.y = gy;
            } else {
                pos.y = feet.y;
            }
        }
        pos
    }

    fn ground_y(&self, pos: Vec3, radius: f32, step: f32) -> Option<f32> {
        let origin = pos + Vec3::Y * step;
        let max_t = step * 2.0;
        let mut cand = Vec::new();
        self.gather_rect(&mut cand, pos.x - radius, pos.x + radius, pos.z - radius, pos.z + radius);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &self.tris[i as usize];
            if is_wall(t) || !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    }

    fn depenetrate(&self, mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
        const DRIFT: f32 = 2.0;
        let mut cand = Vec::new();
        self.gather_rect(&mut cand, pos.x - radius - DRIFT, pos.x + radius + DRIFT, pos.z - radius - DRIFT, pos.z + radius + DRIFT);
        for _ in 0..4 {
            let mut moved = false;
            for &i in cand.iter() {
                let t = &self.tris[i as usize];
                if !is_wall(t) {
                    continue;
                }
                let (b0, b1) = tri_bbox(t);
                if b1.x < pos.x - radius
                    || b0.x > pos.x + radius
                    || b1.z < pos.z - radius
                    || b0.z > pos.z + radius
                    || b1.y < pos.y
                    || b0.y > pos.y + height
                {
                    continue;
                }
                let a = pos + Vec3::Y * radius;
                let b = pos + Vec3::Y * (height - radius);
                let (sp, tp) = seg_tri_closest(a, b, t[0], t[1], t[2]);
                let d = sp - tp;
                let dist = d.length();
                if dist < radius {
                    if dist > 1e-4 {
                        pos += d / dist * (radius - dist);
                    } else {
                        let n = (t[1] - t[0]).cross(t[2] - t[0]);
                        if n.length() > 1e-6 {
                            pos += n.normalize() * radius;
                        }
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        pos
    }
}

/// For an Amanatides–Woo step over the originless hash grid: the segment parameter `t` at which the ray
/// leaves cell `c` along `step`, plus the per-cell `t` increment. `step == 0` → never crosses (`+∞`).
fn hash_axis_step(o: f32, d: f32, c: i32, step: i32, cell: f32) -> (f32, f32) {
    if step == 0 {
        return (f32::INFINITY, f32::INFINITY);
    }
    let boundary = (c + (step > 0) as i32) as f32 * cell;
    ((boundary - o) / d, (cell / d.abs()).abs())
}

// ---------------------------------------------------------------------------
//   Ray / spherecast (camera boom)
// ---------------------------------------------------------------------------

/// Ray/triangle intersection (Möller–Trumbore). Returns hit distance `t ≥ 0` along `dir`, or `None`.
pub fn ray_tri(o: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-7 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = o - a;
    let u = tvec.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = tvec.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4).then_some(t)
}

/// Nearest triangle hit along `[o, o + dir*max_t]` (double-sided). The XZ broadphase grid restricts the
/// per-triangle test to the cells the ray traverses (`O(local)`); the exact segment-AABB cull +
/// Möller–Trumbore then run on those survivors, so the nearest hit is identical to a full linear scan.
pub fn raycast(tris: &[[Vec3; 3]], o: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
    // Broad-phase: the ray SEGMENT's AABB vs each triangle AABB (bbox-based, so a large triangle whose
    // first vertex is far from `o` is still tested).
    let end = o + dir * max_t;
    let (smin, smax) = (o.min(end), o.max(end));
    with_grid(tris, |grid, cand| {
        grid.gather_ray(cand, o, end);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &tris[i as usize];
            let (b0, b1) = tri_bbox(t);
            if b1.x < smin.x || b0.x > smax.x || b1.y < smin.y || b0.y > smax.y || b1.z < smin.z || b0.z > smax.z {
                continue;
            }
            if let Some(d) = ray_tri(o, dir, t[0], t[1], t[2]) {
                if d <= max_t && best.map_or(true, |b| d < b) {
                    best = Some(d);
                }
            }
        }
        best
    })
}

// ---------------------------------------------------------------------------
//   Closest-point primitives
// ---------------------------------------------------------------------------

/// Closest point on triangle `abc` to `p` (Ericson, *Real-Time Collision Detection* §5.1.5).
fn closest_on_tri(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Closest points between segments `[p1,q1]` and `[p2,q2]` (Ericson §5.1.9).
fn closest_seg_seg(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    const EPS: f32 = 1e-8;
    let (s, t);
    if a <= EPS && e <= EPS {
        return (p1, p2);
    }
    if a <= EPS {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let s0 = if denom.abs() > EPS { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            } else {
                t = t0;
                s = s0;
            }
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

/// Closest points between segment `[a,b]` and triangle `t0 t1 t2` (segment point, triangle point).
fn seg_tri_closest(a: Vec3, b: Vec3, t0: Vec3, t1: Vec3, t2: Vec3) -> (Vec3, Vec3) {
    // If the segment crosses the triangle's face, the distance is zero there.
    let n = (t1 - t0).cross(t2 - t0);
    let denom = n.dot(b - a);
    if denom.abs() > 1e-8 {
        let s = n.dot(t0 - a) / denom;
        if (0.0..=1.0).contains(&s) {
            let hit = a + (b - a) * s;
            if (hit - closest_on_tri(hit, t0, t1, t2)).length_squared() < 1e-6 {
                return (hit, hit);
            }
        }
    }
    // Otherwise the closest pair is on the boundary: segment vs each edge, and each endpoint vs face.
    let mut best = (a, closest_on_tri(a, t0, t1, t2));
    let mut best_d = (best.0 - best.1).length_squared();
    let consider = |sp: Vec3, tp: Vec3, best: &mut (Vec3, Vec3), best_d: &mut f32| {
        let d = (sp - tp).length_squared();
        if d < *best_d {
            *best_d = d;
            *best = (sp, tp);
        }
    };
    let qb = closest_on_tri(b, t0, t1, t2);
    consider(b, qb, &mut best, &mut best_d);
    for (e0, e1) in [(t0, t1), (t1, t2), (t2, t0)] {
        let (sp, tp) = closest_seg_seg(a, b, e0, e1);
        consider(sp, tp, &mut best, &mut best_d);
    }
    best
}

// ---------------------------------------------------------------------------
//   Capsule character controller
// ---------------------------------------------------------------------------

/// Push the capsule (feet `pos`, `radius`, `height`) out of every WALL triangle it penetrates. Pushing
/// perpendicular to each contact preserves tangential motion → the capsule slides along walls. A few
/// relaxation passes resolve inside corners. Floors are excluded (the ground probe owns Y).
fn depenetrate(tris: &[[Vec3; 3]], mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
    // The capsule's XZ footprint is `[pos.x±radius]×[pos.z±radius]`; the relaxation passes nudge `pos`
    // by at most a few `radius` in total. Gather candidates over that footprint plus a safe drift margin
    // (a superset of every triangle the per-pass bbox test can accept), then run the unchanged loop over
    // them — the wall push-out, and thus the result, is identical to the linear scan. `DRIFT` covers the
    // small position movement across passes so a wall that comes into range mid-relaxation is included.
    const DRIFT: f32 = 2.0;
    with_grid(tris, |grid, cand| {
        grid.gather_rect(cand, pos.x - radius - DRIFT, pos.x + radius + DRIFT, pos.z - radius - DRIFT, pos.z + radius + DRIFT);
        for _ in 0..4 {
            let mut moved = false;
            for &i in cand.iter() {
                let t = &tris[i as usize];
                if !is_wall(t) {
                    continue;
                }
                // Broad-phase: the capsule's AABB (feet `pos`, up `height`, `radius` around) vs the
                // triangle AABB. Bbox-based — a large wall triangle's first vertex can be far from it.
                let (b0, b1) = tri_bbox(t);
                if b1.x < pos.x - radius
                    || b0.x > pos.x + radius
                    || b1.z < pos.z - radius
                    || b0.z > pos.z + radius
                    || b1.y < pos.y
                    || b0.y > pos.y + height
                {
                    continue;
                }
                let a = pos + Vec3::Y * radius;
                let b = pos + Vec3::Y * (height - radius);
                let (sp, tp) = seg_tri_closest(a, b, t[0], t[1], t[2]);
                let d = sp - tp;
                let dist = d.length();
                if dist < radius {
                    if dist > 1e-4 {
                        pos += d / dist * (radius - dist);
                    } else {
                        let n = (t[1] - t[0]).cross(t[2] - t[0]);
                        if n.length() > 1e-6 {
                            pos += n.normalize() * radius;
                        }
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        pos
    })
}

/// Downward ground probe: the highest WALKABLE surface under `pos` within `[pos.y - step, pos.y + step]`.
/// This is what makes the feet follow stairs/ramps and clears low thresholds without any height hack.
fn ground_y(tris: &[[Vec3; 3]], pos: Vec3, radius: f32, step: f32) -> Option<f32> {
    let origin = pos + Vec3::Y * step;
    let max_t = step * 2.0;
    // Feet footprint is `[pos.x±radius]×[pos.z±radius]`; gather those cells and run the unchanged
    // walkable-surface probe over the survivors — the highest surface found is identical to a full scan.
    with_grid(tris, |grid, cand| {
        grid.gather_rect(cand, pos.x - radius, pos.x + radius, pos.z - radius, pos.z + radius);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &tris[i as usize];
            // Only walkable (near-horizontal) surfaces are ground; skip walls.
            if is_wall(t) {
                continue;
            }
            if !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    })
}

/// Highest walkable surface at or below `pos.y` within `max_drop` metres (a downward probe from
/// slightly above the feet). Unlike [`ground_y`] (a short step probe), this reaches far enough down to
/// catch a **landing** after a jump/fall. `None` = no ground within `max_drop` (a real drop / gap).
pub fn ground_below(tris: &[[Vec3; 3]], pos: Vec3, radius: f32, max_drop: f32) -> Option<f32> {
    let origin = pos + Vec3::Y * 0.1;
    let max_t = max_drop + 0.1;
    // Same feet footprint as `ground_y`, deeper vertical reach: gather the cells under the feet and run
    // the unchanged probe over the survivors — the landing surface found is identical to a full scan.
    with_grid(tris, |grid, cand| {
        grid.gather_rect(cand, pos.x - radius, pos.x + radius, pos.z - radius, pos.z + radius);
        let mut best: Option<f32> = None;
        for &i in cand.iter() {
            let t = &tris[i as usize];
            if is_wall(t) {
                continue;
            }
            if !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    })
}

/// Move the player capsule by a horizontal displacement with collide-and-slide against walls, then
/// (when `follow_ground`) place the feet on the surface underneath within `step`. Returns the new feet
/// position. `follow_ground=false` leaves Y to the caller (e.g. the exterior terrain heightmap).
pub fn move_character(
    tris: &[[Vec3; 3]],
    feet: Vec3,
    horiz_move: Vec3,
    radius: f32,
    height: f32,
    step: f32,
    follow_ground: bool,
) -> Vec3 {
    // Attempt the move, then depenetrate out of walls — perpendicular push-out is the slide.
    let mut pos = feet + Vec3::new(horiz_move.x, 0.0, horiz_move.z);
    pos = depenetrate(tris, pos, radius, height);
    if follow_ground {
        if let Some(gy) = ground_y(tris, pos, radius, step) {
            pos.y = gy;
        } else {
            pos.y = feet.y; // no ground within step (edge/gap): hold Y (no fall yet)
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact bug the player hit: a LARGE floor triangle whose vertices are far from a player standing
    /// in its MIDDLE. The old vertex-distance cull dropped it → `ground_below` returned `None` →
    /// fall-through after moving. The bbox cull keeps it; a point off the floor still reads a real gap.
    #[test]
    fn ground_below_finds_a_large_floor_from_its_middle() {
        let floor = [
            [Vec3::new(-20.0, 0.0, -20.0), Vec3::new(20.0, 0.0, -20.0), Vec3::new(20.0, 0.0, 20.0)],
            [Vec3::new(-20.0, 0.0, -20.0), Vec3::new(20.0, 0.0, 20.0), Vec3::new(-20.0, 0.0, 20.0)],
        ];
        // Dead centre, 20 m from the nearest vertex, 2 m up.
        assert_eq!(
            ground_below(&floor, Vec3::new(0.0, 2.0, 0.0), 0.4, 4.0),
            Some(0.0),
            "large floor must resolve from its middle (bbox cull, not vertex-distance)"
        );
        // Off the floor → a real gap.
        assert_eq!(ground_below(&floor, Vec3::new(100.0, 2.0, 0.0), 0.4, 4.0), None);
    }

    /// A LARGE wall blocks the capsule even when approached far from its vertices.
    #[test]
    fn depenetrate_pushes_out_of_a_large_wall() {
        let wall = [
            [Vec3::new(0.0, 0.0, -20.0), Vec3::new(0.0, 40.0, -20.0), Vec3::new(0.0, 40.0, 20.0)],
            [Vec3::new(0.0, 0.0, -20.0), Vec3::new(0.0, 40.0, 20.0), Vec3::new(0.0, 0.0, 20.0)],
        ];
        // Capsule slightly inside the wall from +X, 5 m up, far from any vertex.
        let out = depenetrate(&wall, Vec3::new(0.2, 5.0, 0.0), 0.4, 1.8);
        assert!(out.x >= 0.4 - 1e-3, "capsule pushed clear of the large wall (x={})", out.x);
    }

    // -----------------------------------------------------------------------
    //   Broadphase correctness: grid-accelerated == brute-force linear scan
    // -----------------------------------------------------------------------

    // Reference implementations = the ORIGINAL linear scans (pre-broadphase). The grid path must return
    // bit-identical results; the perf win follows from only touching local cells.

    fn raycast_brute(tris: &[[Vec3; 3]], o: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let end = o + dir * max_t;
        let (smin, smax) = (o.min(end), o.max(end));
        let mut best: Option<f32> = None;
        for t in tris {
            let (b0, b1) = tri_bbox(t);
            if b1.x < smin.x || b0.x > smax.x || b1.y < smin.y || b0.y > smax.y || b1.z < smin.z || b0.z > smax.z {
                continue;
            }
            if let Some(d) = ray_tri(o, dir, t[0], t[1], t[2]) {
                if d <= max_t && best.map_or(true, |b| d < b) {
                    best = Some(d);
                }
            }
        }
        best
    }

    fn ground_below_brute(tris: &[[Vec3; 3]], pos: Vec3, radius: f32, max_drop: f32) -> Option<f32> {
        let origin = pos + Vec3::Y * 0.1;
        let max_t = max_drop + 0.1;
        let mut best: Option<f32> = None;
        for t in tris {
            if is_wall(t) || !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    }

    fn ground_y_brute(tris: &[[Vec3; 3]], pos: Vec3, radius: f32, step: f32) -> Option<f32> {
        let origin = pos + Vec3::Y * step;
        let max_t = step * 2.0;
        let mut best: Option<f32> = None;
        for t in tris {
            if is_wall(t) || !xz_in_tri_bbox(t, pos, radius) {
                continue;
            }
            if let Some(d) = ray_tri(origin, -Vec3::Y, t[0], t[1], t[2]) {
                if d <= max_t {
                    let y = origin.y - d;
                    if best.map_or(true, |b| y > b) {
                        best = Some(y);
                    }
                }
            }
        }
        best
    }

    fn depenetrate_brute(tris: &[[Vec3; 3]], mut pos: Vec3, radius: f32, height: f32) -> Vec3 {
        for _ in 0..4 {
            let mut moved = false;
            for t in tris {
                if !is_wall(t) {
                    continue;
                }
                let (b0, b1) = tri_bbox(t);
                if b1.x < pos.x - radius
                    || b0.x > pos.x + radius
                    || b1.z < pos.z - radius
                    || b0.z > pos.z + radius
                    || b1.y < pos.y
                    || b0.y > pos.y + height
                {
                    continue;
                }
                let a = pos + Vec3::Y * radius;
                let b = pos + Vec3::Y * (height - radius);
                let (sp, tp) = seg_tri_closest(a, b, t[0], t[1], t[2]);
                let d = sp - tp;
                let dist = d.length();
                if dist < radius {
                    if dist > 1e-4 {
                        pos += d / dist * (radius - dist);
                    } else {
                        let n = (t[1] - t[0]).cross(t[2] - t[0]);
                        if n.length() > 1e-6 {
                            pos += n.normalize() * radius;
                        }
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        pos
    }

    fn move_character_brute(tris: &[[Vec3; 3]], feet: Vec3, horiz: Vec3, radius: f32, height: f32, step: f32, follow: bool) -> Vec3 {
        let mut pos = feet + Vec3::new(horiz.x, 0.0, horiz.z);
        pos = depenetrate_brute(tris, pos, radius, height);
        if follow {
            if let Some(gy) = ground_y_brute(tris, pos, radius, step) {
                pos.y = gy;
            } else {
                pos.y = feet.y;
            }
        }
        pos
    }

    // A tiny deterministic PRNG so the fuzz is reproducible without a `rand` dependency.
    struct Lcg(u64);
    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn f(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (self.next_u32() as f32 / u32::MAX as f32) * (hi - lo)
        }
    }

    // A ~51k-triangle soup: a 160×160 heightfield of small (2 m) triangles, a row of vertical walls, and
    // one map-spanning floor triangle that lands in the grid's OVERSIZED bucket. Exercises every path.
    fn big_terrain() -> Vec<[Vec3; 3]> {
        let n = 160usize;
        let s = 2.0f32;
        let h = |x: f32, z: f32| (x * 0.05).sin() * 1.5 + (z * 0.07).cos() * 1.2;
        let mut tris: Vec<[Vec3; 3]> = Vec::with_capacity(n * n * 2 + 64);
        for xi in 0..n {
            for zi in 0..n {
                let x0 = (xi as f32 - 80.0) * s;
                let x1 = x0 + s;
                let z0 = (zi as f32 - 80.0) * s;
                let z1 = z0 + s;
                let a = Vec3::new(x0, h(x0, z0), z0);
                let b = Vec3::new(x1, h(x1, z0), z0);
                let c = Vec3::new(x1, h(x1, z1), z1);
                let d = Vec3::new(x0, h(x0, z1), z1);
                tris.push([a, c, b]);
                tris.push([a, d, c]);
            }
        }
        for k in 0..20 {
            let xw = (k as f32 - 10.0) * 13.0;
            let a = Vec3::new(xw, -5.0, -30.0);
            let b = Vec3::new(xw, -5.0, 30.0);
            let c = Vec3::new(xw, 5.0, 30.0);
            let d = Vec3::new(xw, 5.0, -30.0);
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
        // Map-spanning floor far below → its bbox covers far more than OVERSIZE_CELLS cells.
        tris.push([Vec3::new(-200.0, -50.0, -200.0), Vec3::new(200.0, -50.0, -200.0), Vec3::new(200.0, -50.0, 200.0)]);
        tris
    }

    /// The whole point: over a 50k+ triangle soup, the grid-accelerated `ground_below` / `raycast` /
    /// `move_character` return results BIT-IDENTICAL to the brute-force linear scan (correctness); the
    /// speedup follows from only visiting local cells, not from any change in what is computed.
    #[test]
    fn grid_matches_bruteforce_over_a_large_soup() {
        let tris = big_terrain();
        assert!(tris.len() > 50_000, "soup should be large ({} tris)", tris.len());
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..600 {
            let x = rng.f(-172.0, 172.0);
            let z = rng.f(-172.0, 172.0);

            // ground_below (feet probe) — the SceneLocomotion ground_height hot path.
            let pos = Vec3::new(x, 10.0, z);
            assert_eq!(
                ground_below(&tris, pos, 0.4, 60.0),
                ground_below_brute(&tris, pos, 0.4, 60.0),
                "ground_below mismatch at ({x}, {z})"
            );

            // raycast (camera boom / weapon ray) — random origin, direction and length.
            let o = Vec3::new(x, rng.f(-2.0, 12.0), z);
            let d = Vec3::new(rng.f(-1.0, 1.0), rng.f(-1.0, 1.0), rng.f(-1.0, 1.0));
            if d.length_squared() > 1e-4 {
                let dir = d.normalize();
                let max = rng.f(1.0, 300.0);
                assert_eq!(
                    raycast(&tris, o, dir, max),
                    raycast_brute(&tris, o, dir, max),
                    "raycast mismatch o={o:?} dir={dir:?} max={max}"
                );
            }

            // move_character (swept player move + ground snap).
            let feet = Vec3::new(x, 8.0, z);
            let mv = Vec3::new(rng.f(-1.0, 1.0), 0.0, rng.f(-1.0, 1.0));
            assert_eq!(
                move_character(&tris, feet, mv, 0.4, 1.8, 0.5, true),
                move_character_brute(&tris, feet, mv, 0.4, 1.8, 0.5, true),
                "move_character mismatch feet={feet:?} mv={mv:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    //   Incremental broadphase correctness: after ANY sequence of insert/remove
    //   ops, IncrementalGrid queries == a brute-force scan of the resident tris.
    // -----------------------------------------------------------------------

    /// Build unit `u`'s triangles: a small cluster (a floor quad + a short wall) at a per-unit world
    /// offset, so different units land in different (and some SHARED) grid cells and the swap-remove
    /// bookkeeping is exercised across cell boundaries.
    fn unit_tris(u: u32) -> Vec<[Vec3; 3]> {
        let ox = ((u % 7) as f32 - 3.0) * 9.0;
        let oz = ((u / 7) as f32 - 3.0) * 9.0;
        let y = (u % 3) as f32 * 0.5;
        let s = 6.0;
        let mut v = vec![
            // floor quad (walkable)
            [Vec3::new(ox, y, oz), Vec3::new(ox + s, y, oz), Vec3::new(ox + s, y, oz + s)],
            [Vec3::new(ox, y, oz), Vec3::new(ox + s, y, oz + s), Vec3::new(ox, y, oz + s)],
            // a wall along +X
            [Vec3::new(ox, y, oz), Vec3::new(ox, y + 4.0, oz), Vec3::new(ox + s, y + 4.0, oz)],
        ];
        // Every 5th unit also carries a MAP-SPANNING floor → lands in the oversized bucket, so insert/
        // remove of oversized triangles is covered too.
        if u % 5 == 0 {
            v.push([Vec3::new(-300.0, -20.0 - y, -300.0), Vec3::new(300.0, -20.0 - y, -300.0), Vec3::new(300.0, -20.0 - y, 300.0)]);
        }
        v
    }

    /// The whole point of the incremental path: after a RANDOM sequence of `insert_unit`/`remove_unit`
    /// ops, the grid's `raycast` / `ground_below` / `move_character` are BIT-IDENTICAL to a brute-force
    /// linear scan of the currently-resident triangles (`grid.tris()`), and its `tris()` buffer holds
    /// exactly the resident units' triangles. Proves add/remove keep the broadphase consistent — the
    /// per-delta work is `O(changed unit)`, but the RESULT matches a full rebuild.
    #[test]
    fn incremental_matches_bruteforce_after_random_ops() {
        let mut rng = Lcg(0xdead_beef_0000_1234);
        let mut grid = IncrementalGrid::new();
        // Ground truth: which units are resident, so we can flatten a reference soup on demand.
        let mut resident: std::collections::BTreeMap<u64, Vec<[Vec3; 3]>> = std::collections::BTreeMap::new();
        const POOL: u32 = 24;

        for op in 0..400 {
            let u = rng.next_u32() % POOL;
            let key = u as u64;
            // Bias toward insert early (fill up), then a mix.
            let insert = resident.is_empty() || (rng.next_u32() % 100) < 55;
            if insert {
                let t = unit_tris(u);
                grid.insert_unit(key, &t);
                resident.insert(key, t);
            } else {
                grid.remove_unit(key);
                resident.remove(&key);
            }

            // Reference = the resident tris in the SAME order the grid stores them, so the order-sensitive
            // move_character/depenetrate accumulation matches bit-for-bit (raycast/ground pick a min/max
            // and are order-independent regardless).
            let flat: Vec<[Vec3; 3]> = grid.tris().to_vec();
            // The grid's compact buffer must hold exactly the resident triangle count.
            let want: usize = resident.values().map(|v| v.len()).sum();
            assert_eq!(flat.len(), want, "op {op}: grid tris count {} != resident {}", flat.len(), want);

            // A handful of queries per op keeps the test fast but broadly covering.
            for _ in 0..6 {
                let x = rng.f(-40.0, 40.0);
                let z = rng.f(-40.0, 40.0);

                let pos = Vec3::new(x, 8.0, z);
                assert_eq!(
                    grid.ground_below(pos, 0.4, 40.0),
                    ground_below_brute(&flat, pos, 0.4, 40.0),
                    "op {op}: ground_below mismatch at ({x},{z})"
                );

                let o = Vec3::new(x, rng.f(-2.0, 10.0), z);
                let d = Vec3::new(rng.f(-1.0, 1.0), rng.f(-1.0, 1.0), rng.f(-1.0, 1.0));
                if d.length_squared() > 1e-4 {
                    let dir = d.normalize();
                    let max = rng.f(1.0, 120.0);
                    assert_eq!(
                        grid.raycast(o, dir, max),
                        raycast_brute(&flat, o, dir, max),
                        "op {op}: raycast mismatch o={o:?} dir={dir:?} max={max}"
                    );
                }

                let feet = Vec3::new(x, 6.0, z);
                let mv = Vec3::new(rng.f(-1.0, 1.0), 0.0, rng.f(-1.0, 1.0));
                assert_eq!(
                    grid.move_character(feet, mv, 0.4, 1.8, 0.5, true),
                    move_character_brute(&flat, feet, mv, 0.4, 1.8, 0.5, true),
                    "op {op}: move_character mismatch feet={feet:?} mv={mv:?}"
                );
            }
        }
        assert!(grid.unit_count() <= POOL as usize);
    }

    /// Re-inserting an existing key REPLACES that unit (idempotent WAKE) rather than duplicating it, and a
    /// full drain leaves an empty, consistent grid.
    #[test]
    fn incremental_reinsert_replaces_and_full_drain_empties() {
        let mut grid = IncrementalGrid::new();
        let a = unit_tris(1);
        grid.insert_unit(1, &a);
        grid.insert_unit(1, &a); // re-wake same key
        assert_eq!(grid.tris().len(), a.len(), "re-insert must not duplicate");
        assert_eq!(grid.unit_count(), 1);
        grid.insert_unit(2, &unit_tris(2));
        grid.remove_unit(1);
        grid.remove_unit(2);
        grid.remove_unit(999); // absent key → no-op
        assert!(grid.is_empty());
        assert_eq!(grid.unit_count(), 0);
        assert!(grid.buckets.is_empty(), "no dangling buckets after full drain");
        assert!(grid.oversized.is_empty(), "no dangling oversized refs after full drain");
    }
}
