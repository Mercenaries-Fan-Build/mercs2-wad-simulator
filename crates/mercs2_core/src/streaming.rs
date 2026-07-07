//! Layer-2 == Layer-3 — the control-driven world-streaming DECISION core.
//!
//! This is PURE data + logic: given the player position and a catalog of loadable units
//! (coarse blocks from the world index, and per-entity placements carrying their own
//! `HibernationControl` distances), it decides — per tick — which blocks are resident, and
//! per loaded entity whether it is awake or hibernated and which LOD tier it renders at.
//! The output is a [`StreamDiff`] the engine executes (load/unload blocks, wake/hibernate
//! entities, tier changes). There is NO wgpu, no file I/O and no asset knowledge here — the
//! engine adapts `WorldIndex`/`ModelPlacement` into the neutral inputs below and performs the
//! actual GPU work. See `docs/modernization/world_streaming_spec.md` §10.
//!
//! Coordinates are native game space (left-handed, +Y up); proximity is measured in world
//! metres. Distances default to the class hibernation/LOD distances **100 / 160 / 60 / 20**
//! (spec §10) when a placement carries no directive of its own.

use std::collections::{HashMap, HashSet};

/// Class-default hibernation/LOD distances (spec §10): `dist[0]` = stream-out (hibernation)
/// distance; `dist[1..4]` = the three LOD-tier distances. A placement with no
/// `HibernationControl` directive falls back to these.
pub const DEFAULT_DISTANCES: [u16; 4] = [100, 160, 60, 20];

/// An axis-aligned box on the ground plane (XZ), native game metres. Used for a block's spatial
/// extent; streaming proximity is horizontal (Y ignored — matches `WorldIndex::Aabb::overlaps_xz`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent2 {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Extent2 {
    pub fn new(minx: f32, minz: f32, maxx: f32, maxz: f32) -> Extent2 {
        Extent2 { min: [minx, minz], max: [maxx, maxz] }
    }

    pub fn from_center_half(cx: f32, cz: f32, half: f32) -> Extent2 {
        Extent2 { min: [cx - half, cz - half], max: [cx + half, cz + half] }
    }

    /// Horizontal distance from `(x, z)` to the box (0 when the point is inside/on it).
    pub fn dist_xz(&self, x: f32, z: f32) -> f32 {
        let dx = (self.min[0] - x).max(0.0).max(x - self.max[0]);
        let dz = (self.min[1] - z).max(0.0).max(z - self.max[1]);
        (dx * dx + dz * dz).sqrt()
    }
}

/// Tunables for the streaming decision (all metres unless noted).
#[derive(Debug, Clone, Copy)]
pub struct StreamingConfig {
    /// Hysteresis: a resident block is only released once it is beyond its `stream_out + margin`.
    pub block_unload_margin: f32,
    /// Max block LOADs emitted per update (I/O throttle). Unloads are never throttled.
    pub block_budget: usize,
    /// Max entity WAKEs emitted per update (I/O throttle). Hibernations are never throttled.
    pub entity_budget: usize,
    /// Wake/hibernate hysteresis band: an awake entity is only hibernated once the player is beyond
    /// `stream_out + entity_hysteresis`, and a hibernated entity only wakes within `stream_out`.
    pub entity_hysteresis: f32,
    /// Spatial-grid gather cap: entities whose stream-out distance is within this are bucketed into
    /// the grid and queried by proximity; the rare long-range outliers (stream-out > cap) are held
    /// in a separate always-tested list so correctness is preserved without a map-wide grid sweep.
    pub entity_scan_cap: f32,
    /// Spatial-grid cell size (bucket edge, metres).
    pub grid_cell: f32,
    /// Fallback hibernation/LOD distances for placements with no directive.
    pub default_distances: [u16; 4],
    /// Per-object geometry-block stream-out distance BY c3 chain tier `[c0, c1, c2, c3]`. The c3
    /// chain is a loose-quadtree spatial index keyed by object SIZE (verified: `tier` is index depth,
    /// NOT a LOD detail level — big landmarks bucket shallow at c3, small props deep at c1), so a
    /// block's tier is a size proxy: coarse-tier objects stream out far (visible from a distance),
    /// fine-tier objects stream out near. Used when a `BlockUnit` gives no explicit `stream_out`.
    pub tier_stream_out: [f32; 4],
}

impl Default for StreamingConfig {
    fn default() -> Self {
        StreamingConfig {
            block_unload_margin: 120.0,
            block_budget: 8,
            entity_budget: 24,
            entity_hysteresis: 15.0,
            entity_scan_cap: 650.0,
            grid_cell: 128.0,
            default_distances: DEFAULT_DISTANCES,
            tier_stream_out: [350.0, 350.0, 700.0, 1200.0],
        }
    }
}

/// A streamable geometry block — ONE baked object indexed in the c3 loose-quadtree. The engine's
/// c3/c2/c1 chain is a size-keyed spatial index (NOT LOD levels of a shared surface — verified: a
/// cell's blocks are distinct objects, tier == index depth ≈ object size), so each block loads/
/// unloads independently by proximity, with a per-object `stream_out` distance scaled to its size/
/// tier (big landmarks visible far, small props cull near). Buildings are baked into these blocks,
/// so loading the near ones == "render the city" (spec §2B). `always_resident` marks a base layer
/// that never streams out.
#[derive(Debug, Clone, Copy)]
pub struct BlockUnit {
    pub block: u16,
    pub extent: Extent2,
    /// Distance past which this object streams out (its extent-to-player horizontal distance). Size/
    /// tier-scaled by the catalog (see `StreamingConfig::tier_stream_out`).
    pub stream_out: f32,
    pub always_resident: bool,
}

/// A streamable per-entity placement (prop / furniture / light / gameplay object), keyed by its
/// `u32` entity key. `dist` is its `HibernationControl` directive (or the class defaults): `dist[0]`
/// stream-out, `dist[1..4]` LOD-tier distances (finest boundary is `dist[3]`).
#[derive(Debug, Clone, Copy)]
pub struct EntityUnit {
    pub key: u32,
    pub pos: [f32; 3],
    pub dist: [u16; 4],
}

impl EntityUnit {
    /// The stream-out (hibernation) distance — past this the object is cached out.
    pub fn stream_out(&self) -> f32 {
        self.dist[0] as f32
    }
}

/// The PER-ENTITY LOD tier for a player distance `d` given a placement's `HibernationControl`
/// LOD-tier distances `dist` (0 = finest .. 3 = coarsest). Boundaries: `dist[3]` (0/1), `dist[2]`
/// (1/2), `dist[1]` (2/3).
///
/// NOTE (fidelity, world_streaming_code_map §2.1/§6): this is the `HibernationControl` distance
/// path (`FUN_00640a40` descriptor, class defaults 100/160/60/20 — verified, no drift). It is
/// **distinct** from the engine's *global* LOD-budget tier `FUN_0084ae70` (a memory-pressure
/// governor modeled by [`GlobalLodGovernor`]); the two compose (the global tier is a coarseness
/// FLOOR on this per-entity tier — see `StreamingManager::update`).
pub fn lod_tier(dist: &[u16; 4], d: f32) -> u8 {
    if d <= dist[3] as f32 {
        0
    } else if d <= dist[2] as f32 {
        1
    } else if d <= dist[1] as f32 {
        2
    } else {
        3
    }
}

/// LOD tier with a hysteresis dead-band around each boundary so an entity hovering on a boundary
/// does not thrash between tiers. `current` is the tier the entity holds now.
fn lod_tier_hyst(dist: &[u16; 4], d: f32, current: u8, h: f32) -> u8 {
    let raw = lod_tier(dist, d);
    if raw == current {
        return current;
    }
    // Boundary between tier t and t+1 (moving coarser as t grows): [0/1, 1/2, 2/3].
    let boundary = [dist[3] as f32, dist[2] as f32, dist[1] as f32];
    if raw > current {
        // Getting coarser: only commit once past the current boundary + h.
        if d > boundary[current as usize] + h {
            raw
        } else {
            current
        }
    } else {
        // Getting finer: only commit once below the target boundary - h.
        if d < boundary[raw as usize] - h {
            raw
        } else {
            current
        }
    }
}

/// The per-update decision the engine executes. Empty when nothing changed this tick.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StreamDiff {
    /// Blocks to decompress + instantiate (throttled to `block_budget`).
    pub load_blocks: Vec<u16>,
    /// Blocks to despawn + free (net-new GPU unload).
    pub unload_blocks: Vec<u16>,
    /// Entity keys to instantiate (throttled to `entity_budget`).
    pub wake: Vec<u32>,
    /// Entity keys to despawn + free.
    pub hibernate: Vec<u32>,
    /// Entity key -> new LOD tier (0 finest .. 3 coarsest), including the tier of a freshly-woken
    /// entity.
    pub tier_changes: Vec<(u32, u8)>,
}

impl StreamDiff {
    pub fn is_empty(&self) -> bool {
        self.load_blocks.is_empty()
            && self.unload_blocks.is_empty()
            && self.wake.is_empty()
            && self.hibernate.is_empty()
            && self.tier_changes.is_empty()
    }
}

/// The streaming decision manager: owns the static catalog (blocks + a spatially-bucketed entity
/// set) and the live residency/awake state, and turns a player position into a [`StreamDiff`] each
/// tick. Pure — no wgpu, no I/O.
pub struct StreamingManager {
    cfg: StreamingConfig,
    blocks: Vec<BlockUnit>,
    entities: Vec<EntityUnit>,
    /// Spatial grid: cell (i, j) -> entity indices with stream-out <= scan_cap.
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Entity indices whose stream-out exceeds `entity_scan_cap` (always tested; the ~222 outliers).
    long_range: Vec<u32>,
    key_to_idx: HashMap<u32, u32>,
    // ---- live state ----
    resident: HashSet<u16>,
    /// Awake entity key -> its current LOD tier.
    awake: HashMap<u32, u8>,
    /// Global memory-pressure LOD-budget governor (engine `FUN_0084ae70`) — a coarseness FLOOR on
    /// the per-entity `HibernationControl` tiers. Defaults to tier 0 (no clamp) until fed pressure.
    lod_gov: GlobalLodGovernor,
    /// Region cache (PgSysPopulation CacheIn/CacheOut, row 9) — a coarser decision layer than
    /// per-object hibernation: caches a whole region's population lump in/out by region containment.
    region_cache: RegionCache,
}

impl StreamingManager {
    pub fn new(cfg: StreamingConfig) -> StreamingManager {
        StreamingManager {
            cfg,
            blocks: Vec::new(),
            entities: Vec::new(),
            grid: HashMap::new(),
            long_range: Vec::new(),
            key_to_idx: HashMap::new(),
            resident: HashSet::new(),
            awake: HashMap::new(),
            lod_gov: GlobalLodGovernor::new(GlobalLodConfig::default()),
            region_cache: RegionCache::new(RegionCacheConfig::default()),
        }
    }

    pub fn config(&self) -> &StreamingConfig {
        &self.cfg
    }

    /// Register a streamable geometry block (one baked object in the loose quadtree).
    pub fn add_block(&mut self, unit: BlockUnit) {
        self.blocks.push(unit);
    }

    fn cell_of(&self, x: f32, z: f32) -> (i32, i32) {
        (
            (x / self.cfg.grid_cell).floor() as i32,
            (z / self.cfg.grid_cell).floor() as i32,
        )
    }

    /// Register a per-entity placement, bucketing it into the spatial grid (at index time, per
    /// spec §10's prop-granularity requirement — `layers_static` is one map-spanning block).
    pub fn add_entity(&mut self, unit: EntityUnit) {
        let idx = self.entities.len() as u32;
        self.key_to_idx.insert(unit.key, idx);
        if unit.stream_out() > self.cfg.entity_scan_cap {
            self.long_range.push(idx);
        } else {
            let c = self.cell_of(unit.pos[0], unit.pos[2]);
            self.grid.entry(c).or_default().push(idx);
        }
        self.entities.push(unit);
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
    /// Number of resident geometry blocks.
    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }
    pub fn awake_count(&self) -> usize {
        self.awake.len()
    }
    pub fn is_resident(&self, block: u16) -> bool {
        self.resident.contains(&block)
    }
    pub fn is_awake(&self, key: u32) -> bool {
        self.awake.contains_key(&key)
    }

    // ---- global LOD-budget governor (engine `FUN_0084ae70`) ----

    /// Feed the global streaming LOD-budget governor the current memory/budget `pressure` at
    /// fixed-sim `step` (called once per frame from the master update, before [`update`]). Returns
    /// `Some(new_tier)` when the global tier changed — the value the engine broadcasts to subscribers
    /// — else `None`. The tier acts as a coarseness FLOOR on every entity's per-entity LOD tier.
    pub fn set_lod_budget(&mut self, pressure: f32, step: u64) -> Option<u8> {
        self.lod_gov.update(pressure, step)
    }
    /// The current global LOD-budget tier (0 = full detail .. 3 = coarsest).
    pub fn global_lod_tier(&self) -> u8 {
        self.lod_gov.tier()
    }

    // ---- region cache (PgSysPopulation CacheIn/CacheOut, row 9) ----

    /// Register a cacheable population region.
    pub fn add_region(&mut self, r: RegionUnit) {
        self.region_cache.add_region(r);
    }
    /// Compute + apply the per-tick region-cache decision for `player` (a sibling phase to
    /// [`update`], mirroring the engine's distinct PgSysPopulation cache-in/out pump).
    pub fn update_regions(&mut self, player: [f32; 3]) -> RegionCacheDiff {
        self.region_cache.update(player)
    }
    /// Explicitly clear a region (engine `msg==7` reset).
    pub fn clear_region(&mut self, key: u32) -> RegionCacheDiff {
        self.region_cache.clear_region(key)
    }
    /// The density-driving region containing `player` (engine `FUN_004d60e0`).
    pub fn active_density_region(&self, player: [f32; 3]) -> Option<u32> {
        self.region_cache.active_density_region(player)
    }
    pub fn region_count(&self) -> usize {
        self.region_cache.region_count()
    }
    pub fn cached_region_count(&self) -> usize {
        self.region_cache.cached_count()
    }
    pub fn is_region_cached(&self, key: u32) -> bool {
        self.region_cache.is_cached(key)
    }
    /// Immutable access to the region cache (kept-ring inspection, etc.).
    pub fn region_cache(&self) -> &RegionCache {
        &self.region_cache
    }

    /// Histogram of awake entities by current LOD tier `[t0, t1, t2, t3]`.
    pub fn tier_histogram(&self) -> [usize; 4] {
        let mut h = [0usize; 4];
        for &t in self.awake.values() {
            if (t as usize) < 4 {
                h[t as usize] += 1;
            }
        }
        h
    }

    /// Compute the per-tick streaming decision for `player` (world XYZ) and apply it to the live
    /// state. The returned diff is what the engine should execute.
    pub fn update(&mut self, player: [f32; 3]) -> StreamDiff {
        let mut diff = StreamDiff::default();
        self.update_blocks(player, &mut diff);
        self.update_entities(player, &mut diff);
        diff
    }

    fn update_blocks(&mut self, player: [f32; 3], diff: &mut StreamDiff) {
        let (px, pz) = (player[0], player[2]);
        // Per-object proximity: each block loads within its own size/tier-scaled `stream_out` and
        // unloads past `stream_out + margin` (hysteresis). Big landmark objects (coarse tier, large
        // stream_out) stay visible from far; small props (fine tier) cull up close.
        let mut to_load: Vec<(f32, u16)> = Vec::new();
        for b in &self.blocks {
            let d = b.extent.dist_xz(px, pz);
            let resident = self.resident.contains(&b.block);
            if b.always_resident {
                if !resident {
                    to_load.push((d, b.block));
                }
                continue;
            }
            if resident {
                if d > b.stream_out + self.cfg.block_unload_margin {
                    diff.unload_blocks.push(b.block);
                }
            } else if d <= b.stream_out {
                to_load.push((d, b.block));
            }
        }
        for b in &diff.unload_blocks {
            self.resident.remove(b);
        }
        // Throttle loads: nearest first, up to the budget. The rest wait for later ticks.
        to_load.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_d, block) in to_load.into_iter().take(self.cfg.block_budget) {
            self.resident.insert(block);
            diff.load_blocks.push(block);
        }
    }

    fn update_entities(&mut self, player: [f32; 3], diff: &mut StreamDiff) {
        let h = self.cfg.entity_hysteresis;
        // Global LOD-budget FLOOR (engine `FUN_0084ae70`): under memory pressure the governor forces
        // a minimum coarseness on every entity, on top of its per-entity `HibernationControl` tier.
        // Default pressure keeps this at 0 (no clamp). `max` = never render finer than the budget.
        let floor = self.lod_gov.tier();
        // Gather candidate entity indices: grid cells within the scan cap + the long-range list.
        let mut candidates: Vec<u32> = Vec::new();
        let r = self.cfg.entity_scan_cap;
        let (cx0, cz0) = self.cell_of(player[0] - r, player[2] - r);
        let (cx1, cz1) = self.cell_of(player[0] + r, player[2] + r);
        for ci in cx0..=cx1 {
            for cj in cz0..=cz1 {
                if let Some(v) = self.grid.get(&(ci, cj)) {
                    candidates.extend_from_slice(v);
                }
            }
        }
        candidates.extend_from_slice(&self.long_range);

        // First pass: decide wake/hibernate/tier for every candidate; collect wake requests so the
        // budget can pick the nearest. `visited` keys let us hibernate awake entities that fell out
        // of the candidate set entirely.
        let mut visited: HashSet<u32> = HashSet::new();
        let mut wake_req: Vec<(f32, u32, u8)> = Vec::new(); // (dist, key, tier)
        for &idx in &candidates {
            let e = &self.entities[idx as usize];
            visited.insert(e.key);
            let d = dist3(player, e.pos);
            let out = e.stream_out();
            match self.awake.get(&e.key).copied() {
                Some(cur_tier) => {
                    if d > out + h {
                        diff.hibernate.push(e.key);
                    } else {
                        let nt = lod_tier_hyst(&e.dist, d, cur_tier, h).max(floor);
                        if nt != cur_tier {
                            diff.tier_changes.push((e.key, nt));
                            self.awake.insert(e.key, nt);
                        }
                    }
                }
                None => {
                    if d <= out {
                        wake_req.push((d, e.key, lod_tier(&e.dist, d).max(floor)));
                    }
                }
            }
        }
        // Any awake entity not seen this tick is beyond the scan cap -> hibernate it.
        let stale: Vec<u32> = self
            .awake
            .keys()
            .copied()
            .filter(|k| !visited.contains(k))
            .collect();
        for k in stale {
            diff.hibernate.push(k);
        }
        for k in &diff.hibernate {
            self.awake.remove(k);
        }
        // Throttle wakes: nearest first, up to the budget. Deferred entities stay hibernated and are
        // reconsidered next tick.
        wake_req.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_d, key, tier) in wake_req.into_iter().take(self.cfg.entity_budget) {
            self.awake.insert(key, tier);
            diff.wake.push(key);
            diff.tier_changes.push((key, tier));
        }
    }
}

/// Full 3-D distance between two world points.
fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// ===========================================================================================
//  Global LOD-budget governor — engine `FUN_0084ae70` (world_streaming_code_map §2.1)
// ===========================================================================================

/// Tunables for the global LOD-budget governor.
#[derive(Debug, Clone, Copy)]
pub struct GlobalLodConfig {
    /// Ascending memory/budget-pressure thresholds (climb points) for tiers 1/2/3. A pressure at or
    /// above `climb[k]` forces the governor to at least tier `k+1` (0 = full detail). The engine
    /// (`FUN_0084ae70`) compares the live budget pressure against `DAT_00ce8dd8` / `PTR_DAT_00ce8ddc`
    /// / `PTR_FUN_00ce8de0`.
    ///
    /// CONFIRM-LIVE: only the tier COUNT (0..3) and the compare ORDER are proven from the decompiled
    /// body; the concrete threshold values are not read out — these normalized defaults are a
    /// stand-in until the three `DAT_00ce8dd*` constants are read live (x32dbg, §5 item 2).
    pub climb: [f32; 3],
    /// Hysteresis band (`FUN_0084ae70` `DAT_00ce8dd4`): the governor DROPS a tier only once pressure
    /// falls below `climb[tier-1] - band`, and commits at most ONE drop per fixed-sim step
    /// (rate-limited via the step counter `DAT_011765cc`). Climbs apply immediately so memory
    /// pressure is relieved promptly.
    pub band: f32,
}

impl Default for GlobalLodConfig {
    fn default() -> Self {
        // Normalized pressure in [0,1] (e.g. resident-bytes / budget). CONFIRM-LIVE values.
        GlobalLodConfig { climb: [0.60, 0.80, 0.92], band: 0.08 }
    }
}

/// The global memory-pressure LOD-budget tier (engine `FUN_0084ae70`), 0 (full detail) .. 3
/// (coarsest). Ticked once per frame from the master update with the current budget pressure and the
/// fixed-sim step; a tier change is what the engine broadcasts (`vtable+4(newTier)`) to subscribers.
/// This is a *global governor* distinct from the per-entity `HibernationControl` distances; the two
/// compose as a FLOOR (see [`StreamingManager::update`]).
#[derive(Debug, Clone)]
pub struct GlobalLodGovernor {
    cfg: GlobalLodConfig,
    tier: u8,
    /// The fixed-sim step at which the last DROP committed (rate-limits drops to one/step).
    last_drop_step: u64,
}

impl GlobalLodGovernor {
    pub fn new(cfg: GlobalLodConfig) -> GlobalLodGovernor {
        GlobalLodGovernor { cfg, tier: 0, last_drop_step: 0 }
    }

    /// The tier `pressure` alone maps to (ignoring hysteresis/rate-limit).
    fn raw_tier(&self, pressure: f32) -> u8 {
        if pressure >= self.cfg.climb[2] {
            3
        } else if pressure >= self.cfg.climb[1] {
            2
        } else if pressure >= self.cfg.climb[0] {
            1
        } else {
            0
        }
    }

    /// Update from the current budget `pressure` at fixed-sim `step`. Climbs immediately (possibly
    /// several tiers on a spike); drops one tier at a time, only past the hysteresis band and at most
    /// once per fixed-sim step. Returns `Some(new_tier)` when the tier changed (the engine broadcasts
    /// it), else `None`.
    pub fn update(&mut self, pressure: f32, step: u64) -> Option<u8> {
        let raw = self.raw_tier(pressure);
        if raw > self.tier {
            self.tier = raw; // climb immediately
            return Some(self.tier);
        }
        if raw < self.tier {
            let cur = self.tier as usize; // dropping from `cur` needs pressure < climb[cur-1] - band
            if pressure < self.cfg.climb[cur - 1] - self.cfg.band && step != self.last_drop_step {
                self.tier -= 1;
                self.last_drop_step = step;
                return Some(self.tier);
            }
        }
        None
    }

    pub fn tier(&self) -> u8 {
        self.tier
    }
    pub fn config(&self) -> &GlobalLodConfig {
        &self.cfg
    }
}

// ===========================================================================================
//  Region cache (PgSysPopulation CacheIn / CacheOut) — row 9
//  population_spawner_code_map §5 (pump `FUN_005017b0`, drain `FUN_00502fc0`, region `FUN_004d60e0`)
// ===========================================================================================

/// One region-cache operation the executor applies. The `CacheOut`/`Clear` discriminants mirror the
/// engine's cache-message switch in the CacheOut pump `FUN_005017b0` (`msg==3` / `msg==7`); `CacheIn`
/// is the separate one-lump/tick kept-list drain `FUN_00502fc0`.
///
/// CONFIRM-LIVE: the pump also handles a third message type `0xB` (`msg != 0xB` is the skip guard),
/// but that body was not read from the decomp — it is a further cache-side event with no
/// proximity-derived trigger here, so this decision core does not emit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionCacheOp {
    /// Kept-list drain (`FUN_00502fc0`): (re-)instantiate a region's population lump. `warm` = the
    /// region was still in the kept ring, i.e. a cheap re-cache vs a cold first cache-in.
    CacheIn { region: u32, warm: bool },
    /// Pump `msg==3` — stream-out apply: despawn the region's lump and retain a kept-population
    /// record (the region key is pushed into the kept ring).
    CacheOut(u32),
    /// Pump `msg==7` — clear driver-data + faction cleanup: full region reset (dropped from the
    /// cache AND the kept ring). Emitted only by an explicit [`RegionCache::clear_region`].
    Clear(u32),
}

impl RegionCacheOp {
    /// The engine cache-message discriminant this op corresponds to in the `FUN_005017b0` switch.
    /// `CacheIn` runs through the separate `FUN_00502fc0` drain (no 3/7/0xB code) → `None`.
    pub fn msg_type(&self) -> Option<u8> {
        match self {
            RegionCacheOp::CacheOut(_) => Some(3),
            RegionCacheOp::Clear(_) => Some(7),
            RegionCacheOp::CacheIn { .. } => None,
        }
    }
}

/// A cacheable population region — a rectangular containment area (engine `FUN_004d60e0` is a
/// rect-containment test) that owns an ambient-population "lump" streamed in/out as a unit.
#[derive(Debug, Clone, Copy)]
pub struct RegionUnit {
    pub key: u32,
    /// Rectangular region extent on the ground plane (`FUN_004d60e0` min/max `[+0x10,+0x18]×
    /// [+0x14,+0x1c]`).
    pub extent: Extent2,
    /// Priority gate (`FUN_004d60e0` `+0x38`): on overlap the higher-priority region wins the
    /// density selection; ties break by the deeper edge-margin (best-fit).
    pub priority: i32,
    /// Horizontal distance at/under which the region caches IN (0 = player inside the rect).
    pub cache_in: f32,
    /// Horizontal distance past which the region caches OUT. `>= cache_in` gives a hysteresis band so
    /// a player loitering on the boundary does not thrash cache in/out.
    pub cache_out: f32,
}

/// Tunables for the region cache.
#[derive(Debug, Clone, Copy)]
pub struct RegionCacheConfig {
    /// CacheIn drain budget = region lumps admitted per tick. The engine drains **one lump/tick**
    /// (`FUN_00502fc0`); nearest-first, the rest wait for later ticks.
    pub cache_in_budget: usize,
    /// Kept-population ring capacity. CONFIRM-LIVE (population_spawner_code_map §5): the PC build
    /// drains an **8-slot** ring (`DAT_00ed55d4`, cursor `& 7`) while the Xbox build keeps **64** —
    /// the discrepancy is an open confirm-live item; this uses the PC value.
    pub kept_ring: usize,
}

impl Default for RegionCacheConfig {
    fn default() -> Self {
        RegionCacheConfig { cache_in_budget: 1, kept_ring: 8 }
    }
}

/// The per-tick region-cache decision.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RegionCacheDiff {
    /// Regions cached IN this tick (throttled to `cache_in_budget`): `(region_key, warm)` where
    /// `warm` = it was still in the kept ring.
    pub cache_in: Vec<(u32, bool)>,
    /// Regions cached OUT this tick (unthrottled — evict is never budgeted).
    pub cache_out: Vec<u32>,
    /// Regions explicitly CLEARED this tick (engine `msg==7`).
    pub cleared: Vec<u32>,
    /// The typed op stream mirroring the engine messages, in apply order: clears, then cache-outs,
    /// then the throttled cache-in drain. `cache_in`/`cache_out`/`cleared` are convenience
    /// projections of this.
    pub ops: Vec<RegionCacheOp>,
}

impl RegionCacheDiff {
    pub fn is_empty(&self) -> bool {
        self.cache_in.is_empty() && self.cache_out.is_empty() && self.cleared.is_empty()
    }
}

/// The region-cache decision layer. Owns the region catalog + live cache residency + the bounded
/// kept ring, and turns a player position into a [`RegionCacheDiff`] each tick. Pure — no I/O.
#[derive(Debug, Clone)]
pub struct RegionCache {
    cfg: RegionCacheConfig,
    regions: Vec<RegionUnit>,
    key_to_idx: HashMap<u32, usize>,
    /// Currently cached-in region keys.
    cached: HashSet<u32>,
    /// Bounded ring of recently cached-out region keys (engine `DAT_00ed55d4`, cursor `& (len-1)`):
    /// a re-entry that finds its key here is a cheap "warm" re-cache.
    kept: Vec<u32>,
}

impl RegionCache {
    pub fn new(cfg: RegionCacheConfig) -> RegionCache {
        RegionCache {
            cfg,
            regions: Vec::new(),
            key_to_idx: HashMap::new(),
            cached: HashSet::new(),
            kept: Vec::new(),
        }
    }

    pub fn add_region(&mut self, r: RegionUnit) {
        self.key_to_idx.insert(r.key, self.regions.len());
        self.regions.push(r);
    }

    pub fn config(&self) -> &RegionCacheConfig {
        &self.cfg
    }
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
    pub fn cached_count(&self) -> usize {
        self.cached.len()
    }
    pub fn is_cached(&self, key: u32) -> bool {
        self.cached.contains(&key)
    }
    /// Whether `key` is currently held in the kept ring (a warm re-cache candidate).
    pub fn in_kept_ring(&self, key: u32) -> bool {
        self.kept.contains(&key)
    }

    /// Push a region key into the bounded kept ring (dropping the oldest once full — the `& (len-1)`
    /// cursor overwrite). No-op when the ring capacity is 0.
    fn keep(&mut self, key: u32) {
        if self.cfg.kept_ring == 0 {
            return;
        }
        // Avoid duplicate entries so `in_kept_ring` stays a clean set membership.
        self.kept.retain(|&k| k != key);
        self.kept.push(key);
        while self.kept.len() > self.cfg.kept_ring {
            self.kept.remove(0);
        }
    }

    /// The per-tick region-cache decision for `player`: evict regions the player left (CacheOut →
    /// kept ring), then admit up to `cache_in_budget` newly-entered regions nearest-first (CacheIn).
    pub fn update(&mut self, player: [f32; 3]) -> RegionCacheDiff {
        let (px, pz) = (player[0], player[2]);
        let mut diff = RegionCacheDiff::default();

        // CacheOut pass (unthrottled): any cached region the player is now beyond `cache_out`.
        let mut cache_out: Vec<u32> = Vec::new();
        for r in &self.regions {
            if self.cached.contains(&r.key) && r.extent.dist_xz(px, pz) > r.cache_out {
                cache_out.push(r.key);
            }
        }
        for key in cache_out {
            self.cached.remove(&key);
            self.keep(key);
            diff.cache_out.push(key);
            diff.ops.push(RegionCacheOp::CacheOut(key));
        }

        // CacheIn pass (throttled to `cache_in_budget`, nearest-first): regions the player entered.
        let mut want: Vec<(f32, u32)> = Vec::new();
        for r in &self.regions {
            if !self.cached.contains(&r.key) {
                let d = r.extent.dist_xz(px, pz);
                if d <= r.cache_in {
                    want.push((d, r.key));
                }
            }
        }
        want.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_d, key) in want.into_iter().take(self.cfg.cache_in_budget) {
            let warm = self.in_kept_ring(key);
            self.kept.retain(|&k| k != key); // consumed from the kept store on re-cache
            self.cached.insert(key);
            diff.cache_in.push((key, warm));
            diff.ops.push(RegionCacheOp::CacheIn { region: key, warm });
        }
        diff
    }

    /// Explicitly clear a region (engine `msg==7`: driver-data clear + faction cleanup): drop it from
    /// the cache AND the kept ring. Returns the diff (`cleared`/`ops` carry it) so the executor posts
    /// the reset. This is an event, not a proximity derivation.
    pub fn clear_region(&mut self, key: u32) -> RegionCacheDiff {
        let mut diff = RegionCacheDiff::default();
        if self.key_to_idx.contains_key(&key) {
            self.cached.remove(&key);
            self.kept.retain(|&k| k != key);
            diff.cleared.push(key);
            diff.ops.push(RegionCacheOp::Clear(key));
        }
        diff
    }

    /// The single density-driving region containing `player` (engine `FUN_004d60e0`): among regions
    /// whose rect contains the point, the highest `priority` wins; ties break by the deeper
    /// edge-margin (best-fit). `None` when the player is inside no region.
    pub fn active_density_region(&self, player: [f32; 3]) -> Option<u32> {
        let (px, pz) = (player[0], player[2]);
        let mut best: Option<(i32, f32, u32)> = None; // (priority, edge-margin, key)
        for r in &self.regions {
            if r.extent.dist_xz(px, pz) != 0.0 {
                continue; // not contained
            }
            let margin = (px - r.extent.min[0])
                .min(r.extent.max[0] - px)
                .min(pz - r.extent.min[1])
                .min(r.extent.max[1] - pz);
            let better = match best {
                None => true,
                Some((bp, bm, _)) => r.priority > bp || (r.priority == bp && margin > bm),
            };
            if better {
                best = Some((r.priority, margin, r.key));
            }
        }
        best.map(|(_, _, k)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(key: u32, x: f32, z: f32, dist: [u16; 4]) -> EntityUnit {
        EntityUnit { key, pos: [x, 0.0, z], dist }
    }

    #[test]
    fn extent_distance() {
        let e = Extent2::from_center_half(100.0, 200.0, 40.0); // [60..140] x [160..240]
        assert_eq!(e.dist_xz(100.0, 200.0), 0.0); // inside
        assert_eq!(e.dist_xz(150.0, 200.0), 10.0); // 10 past the +X face
        assert!((e.dist_xz(150.0, 250.0) - (100f32 + 100.0).sqrt()).abs() < 1e-4);
    }

    #[test]
    fn lod_tier_boundaries() {
        let d = DEFAULT_DISTANCES; // [100,160,60,20]
        assert_eq!(lod_tier(&d, 10.0), 0); // <=20 finest
        assert_eq!(lod_tier(&d, 20.0), 0);
        assert_eq!(lod_tier(&d, 40.0), 1); // <=60
        assert_eq!(lod_tier(&d, 100.0), 2); // <=160
        assert_eq!(lod_tier(&d, 200.0), 3); // coarsest
    }

    #[test]
    fn entity_wakes_within_stream_out_and_gets_initial_tier() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_entity(ent(1, 0.0, 0.0, DEFAULT_DISTANCES)); // stream-out 100, finest <=20
        // Player 50 m away: within stream-out (100) -> wake, tier 1 (<=60).
        let d = m.update([50.0, 0.0, 0.0]);
        assert_eq!(d.wake, vec![1]);
        assert_eq!(d.tier_changes, vec![(1, 1)]);
        assert!(m.is_awake(1));
        assert_eq!(m.tier_histogram(), [0, 1, 0, 0]);
    }

    #[test]
    fn entity_hibernates_beyond_stream_out_plus_hysteresis() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_entity(ent(1, 0.0, 0.0, DEFAULT_DISTANCES));
        m.update([50.0, 0.0, 0.0]); // wake
        assert!(m.is_awake(1));
        // 110 m: past stream-out (100) but within +hysteresis (15) -> stays awake.
        let d = m.update([110.0, 0.0, 0.0]);
        assert!(d.hibernate.is_empty());
        assert!(m.is_awake(1));
        // 130 m: past 100 + 15 -> hibernate.
        let d = m.update([130.0, 0.0, 0.0]);
        assert_eq!(d.hibernate, vec![1]);
        assert!(!m.is_awake(1));
    }

    #[test]
    fn default_fallback_vs_custom_directive() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_entity(ent(1, 0.0, 0.0, DEFAULT_DISTANCES)); // out 100
        m.add_entity(ent(2, 0.0, 0.0, [300, 160, 60, 20])); // custom out 300
        // 200 m away: default entity stays hibernated, custom one wakes.
        let d = m.update([200.0, 0.0, 0.0]);
        assert_eq!(d.wake, vec![2]);
        assert!(!m.is_awake(1));
        assert!(m.is_awake(2));
    }

    #[test]
    fn tier_hysteresis_prevents_thrash() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_entity(ent(1, 0.0, 0.0, DEFAULT_DISTANCES)); // boundary 0/1 at 20
        // Wake at 18 -> tier 0.
        let d = m.update([18.0, 0.0, 0.0]);
        assert_eq!(d.tier_changes, vec![(1, 0)]);
        // 24 m: past the 20 boundary but within +15 dead-band -> tier stays 0 (no change emitted).
        let d = m.update([24.0, 0.0, 0.0]);
        assert!(d.tier_changes.is_empty());
        // 40 m: clearly past 20 + 15 -> tier 1.
        let d = m.update([40.0, 0.0, 0.0]);
        assert_eq!(d.tier_changes, vec![(1, 1)]);
    }

    fn blk(id: u16, cx: f32, cz: f32, stream_out: f32) -> BlockUnit {
        BlockUnit { block: id, extent: Extent2::from_center_half(cx, cz, 5.0), stream_out, always_resident: false }
    }

    #[test]
    fn block_streams_by_its_own_size_scaled_distance() {
        // A big landmark (large stream_out) and a small prop (small stream_out) at the same spot.
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_block(blk(1, 0.0, 0.0, 1200.0)); // coarse/big -> visible far
        m.add_block(blk(2, 0.0, 0.0, 350.0)); //  fine/small -> near only
        // At 500 m from the origin: the big object is resident, the small one is not.
        let d = m.update([505.0, 0.0, 0.0]); // extent half 5 -> ~500 m to the box
        assert_eq!(d.load_blocks, vec![1]);
        assert!(m.is_resident(1) && !m.is_resident(2));
        // Move within 350: the small one now loads too.
        let d = m.update([300.0, 0.0, 0.0]);
        assert_eq!(d.load_blocks, vec![2]);
        assert!(m.is_resident(1) && m.is_resident(2));
    }

    #[test]
    fn block_unloads_past_stream_out_plus_margin() {
        let mut cfg = StreamingConfig::default();
        cfg.block_unload_margin = 50.0;
        let mut m = StreamingManager::new(cfg);
        m.add_block(blk(1, 0.0, 0.0, 100.0));
        m.update([0.0, 0.0, 0.0]); // resident
        assert!(m.is_resident(1));
        // 130 m (extent-dist ~125): past 100 but within +50 margin -> stays resident.
        let d = m.update([130.0, 0.0, 0.0]);
        assert!(d.unload_blocks.is_empty() && m.is_resident(1));
        // 170 m (~165): past 100 + 50 -> unloads.
        let d = m.update([170.0, 0.0, 0.0]);
        assert_eq!(d.unload_blocks, vec![1]);
        assert!(!m.is_resident(1));
    }

    #[test]
    fn block_load_budget_throttles_nearest_first() {
        let mut cfg = StreamingConfig::default();
        cfg.block_budget = 1;
        let mut m = StreamingManager::new(cfg);
        m.add_block(blk(10, 10.0, 0.0, 600.0)); // nearest
        m.add_block(blk(11, 30.0, 0.0, 600.0)); // farther
        let d = m.update([0.0, 0.0, 0.0]);
        assert_eq!(d.load_blocks, vec![10]);
        let d = m.update([0.0, 0.0, 0.0]);
        assert_eq!(d.load_blocks, vec![11]);
    }

    #[test]
    fn always_resident_block_loads_and_never_unloads() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_block(BlockUnit { block: 29, extent: Extent2::from_center_half(0.0, 0.0, 5.0), stream_out: 0.0, always_resident: true });
        let d = m.update([0.0, 0.0, 0.0]);
        assert_eq!(d.load_blocks, vec![29]);
        // Far away — still resident, no unload.
        let d = m.update([9000.0, 0.0, 9000.0]);
        assert!(d.unload_blocks.is_empty());
        assert!(m.is_resident(29));
    }

    #[test]
    fn wake_budget_defers_far_entities() {
        let mut cfg = StreamingConfig::default();
        cfg.entity_budget = 1;
        let mut m = StreamingManager::new(cfg);
        m.add_entity(ent(1, 10.0, 0.0, [400, 160, 60, 20]));
        m.add_entity(ent(2, 20.0, 0.0, [400, 160, 60, 20]));
        // Both within stream-out but budget 1 -> nearest (key 1) wakes first.
        let d = m.update([0.0, 0.0, 0.0]);
        assert_eq!(d.wake, vec![1]);
        let d = m.update([0.0, 0.0, 0.0]);
        assert_eq!(d.wake, vec![2]);
    }

    #[test]
    fn long_range_entity_is_tracked_and_wakes() {
        // stream-out beyond the scan cap must still be considered (the 222 >400 outliers).
        let mut cfg = StreamingConfig::default();
        cfg.entity_scan_cap = 200.0;
        let mut m = StreamingManager::new(cfg);
        m.add_entity(ent(7, 0.0, 0.0, [800, 160, 60, 20]));
        let d = m.update([500.0, 0.0, 0.0]); // 500 m: past scan cap but within stream-out 800
        assert_eq!(d.wake, vec![7]);
        assert!(m.is_awake(7));
    }

    // ---- global LOD-budget governor (FUN_0084ae70) ----

    #[test]
    fn global_lod_governor_climbs_bands_and_broadcasts_on_change() {
        let mut g = GlobalLodGovernor::new(GlobalLodConfig::default()); // climb [0.60,0.80,0.92]
        assert_eq!(g.tier(), 0);
        assert_eq!(g.update(0.10, 0), None); // low pressure: stays tier 0, no broadcast
        assert_eq!(g.update(0.65, 1), Some(1)); // crosses climb[0] -> tier 1, broadcast
        assert_eq!(g.update(0.65, 2), None); // unchanged -> no broadcast
        assert_eq!(g.update(0.85, 3), Some(2)); // crosses climb[1]
        assert_eq!(g.update(0.99, 4), Some(3)); // crosses climb[2] (coarsest)
    }

    #[test]
    fn global_lod_governor_climbs_immediately_but_drops_are_banded_and_rate_limited() {
        let mut g = GlobalLodGovernor::new(GlobalLodConfig::default());
        assert_eq!(g.update(0.99, 0), Some(3)); // spike straight to tier 3 (multi-tier climb)
        // Pressure collapses to 0, but drops are ONE tier per fixed-sim step.
        assert_eq!(g.update(0.0, 1), Some(2));
        assert_eq!(g.update(0.0, 1), None); // same step -> no further drop (rate-limited)
        assert_eq!(g.update(0.0, 2), Some(1));
        assert_eq!(g.update(0.0, 3), Some(0));
        assert_eq!(g.update(0.0, 4), None); // already at floor
    }

    #[test]
    fn global_lod_governor_hysteresis_holds_tier_inside_the_band() {
        let mut g = GlobalLodGovernor::new(GlobalLodConfig::default()); // band 0.08
        assert_eq!(g.update(0.65, 0), Some(1)); // climb to tier 1 at climb[0]=0.60
        // 0.55 is below climb[0] but within the band (0.60 - 0.08 = 0.52): do NOT drop.
        assert_eq!(g.update(0.55, 1), None);
        assert!(g.tier() == 1);
        // 0.50 is below the band -> drop.
        assert_eq!(g.update(0.50, 2), Some(0));
    }

    #[test]
    fn global_budget_tier_floors_per_entity_lod_tier() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_entity(ent(1, 0.0, 0.0, [400, 160, 60, 20])); // finest <=20
        // Force the global governor to tier 2 (memory pressure); it becomes a coarseness floor.
        assert_eq!(m.set_lod_budget(0.85, 0), Some(2));
        assert_eq!(m.global_lod_tier(), 2);
        // Player 5 m away: per-entity tier would be 0, but the floor clamps it to 2.
        let d = m.update([5.0, 0.0, 0.0]);
        assert_eq!(d.wake, vec![1]);
        assert_eq!(d.tier_changes, vec![(1, 2)]);
        assert_eq!(m.tier_histogram(), [0, 0, 1, 0]);
    }

    // ---- region cache (PgSysPopulation CacheIn/CacheOut, row 9) ----

    fn region(key: u32, cx: f32, cz: f32, half: f32, prio: i32) -> RegionUnit {
        RegionUnit {
            key,
            extent: Extent2::from_center_half(cx, cz, half),
            priority: prio,
            cache_in: 50.0,
            cache_out: 80.0,
        }
    }

    #[test]
    fn region_caches_in_on_entry_and_out_on_exit_with_hysteresis() {
        let mut rc = RegionCache::new(RegionCacheConfig::default());
        rc.add_region(region(1, 0.0, 0.0, 10.0, 0)); // rect [-10..10]²
        // 40 m from the rect edge (player at 50): within cache_in 50 -> cache in (cold).
        let d = rc.update([50.0, 0.0, 0.0]);
        assert_eq!(d.cache_in, vec![(1, false)]);
        assert!(d.cache_out.is_empty());
        assert!(rc.is_cached(1));
        assert_eq!(d.ops, vec![RegionCacheOp::CacheIn { region: 1, warm: false }]);
        // Player at 100 (dist 90 from edge): past cache_out 80 -> cache out, key kept.
        let d = rc.update([100.0, 0.0, 0.0]);
        assert_eq!(d.cache_out, vec![1]);
        assert_eq!(d.ops, vec![RegionCacheOp::CacheOut(1)]);
        assert!(!rc.is_cached(1));
        assert!(rc.in_kept_ring(1));
    }

    #[test]
    fn region_boundary_loiter_does_not_thrash() {
        let mut rc = RegionCache::new(RegionCacheConfig::default());
        rc.add_region(region(1, 0.0, 0.0, 10.0, 0));
        rc.update([50.0, 0.0, 0.0]); // cache in
        assert!(rc.is_cached(1));
        // dist-from-edge 60 (player 70): past cache_in 50 but within cache_out 80 -> stays cached.
        let d = rc.update([70.0, 0.0, 0.0]);
        assert!(d.is_empty());
        assert!(rc.is_cached(1));
    }

    #[test]
    fn region_re_entry_from_kept_ring_is_warm() {
        let mut rc = RegionCache::new(RegionCacheConfig::default());
        rc.add_region(region(1, 0.0, 0.0, 10.0, 0));
        rc.update([50.0, 0.0, 0.0]); // cold cache in
        rc.update([100.0, 0.0, 0.0]); // cache out -> kept ring
        assert!(rc.in_kept_ring(1));
        // Re-enter: still in the kept ring -> warm re-cache, and consumed from the ring.
        let d = rc.update([50.0, 0.0, 0.0]);
        assert_eq!(d.cache_in, vec![(1, true)]);
        assert!(rc.is_cached(1));
        assert!(!rc.in_kept_ring(1));
    }

    #[test]
    fn region_cache_in_drains_one_lump_per_tick_nearest_first() {
        let cfg = RegionCacheConfig { cache_in_budget: 1, kept_ring: 8 };
        let mut rc = RegionCache::new(cfg);
        rc.add_region(region(1, 10.0, 0.0, 1.0, 0)); // nearer to player at origin
        rc.add_region(region(2, 30.0, 0.0, 1.0, 0)); // farther
        // Both within cache_in but budget 1 -> nearest (key 1) caches this tick.
        let d = rc.update([0.0, 0.0, 0.0]);
        assert_eq!(d.cache_in, vec![(1, false)]);
        assert!(rc.is_cached(1) && !rc.is_cached(2));
        // Next tick admits the second lump.
        let d = rc.update([0.0, 0.0, 0.0]);
        assert_eq!(d.cache_in, vec![(2, false)]);
    }

    #[test]
    fn kept_ring_is_bounded_and_evicts_oldest() {
        let cfg = RegionCacheConfig { cache_in_budget: 8, kept_ring: 2 };
        let mut rc = RegionCache::new(cfg);
        for k in 1..=3u32 {
            rc.add_region(region(k, (k as f32) * 1000.0, 0.0, 1.0, 0));
        }
        // Cache each region in near its own centre, then cache it out by moving far away, so the kept
        // ring fills in order 1 -> 2 -> 3.
        rc.update([1000.0, 0.0, 0.0]); // cache in 1
        rc.update([9000.0, 0.0, 0.0]); // 1 out -> kept {1}
        rc.update([2000.0, 0.0, 0.0]); // cache in 2
        rc.update([9000.0, 0.0, 0.0]); // 2 out -> kept {1,2}
        rc.update([3000.0, 0.0, 0.0]); // cache in 3
        rc.update([9000.0, 0.0, 0.0]); // 3 out -> kept {2,3} (1 evicted, cap 2)
        assert!(!rc.in_kept_ring(1));
        assert!(rc.in_kept_ring(2));
        assert!(rc.in_kept_ring(3));
    }

    #[test]
    fn region_clear_resets_cache_and_kept_ring() {
        let mut rc = RegionCache::new(RegionCacheConfig::default());
        rc.add_region(region(1, 0.0, 0.0, 10.0, 0));
        rc.update([50.0, 0.0, 0.0]); // cache in
        assert!(rc.is_cached(1));
        let d = rc.clear_region(1);
        assert_eq!(d.cleared, vec![1]);
        assert_eq!(d.ops, vec![RegionCacheOp::Clear(1)]);
        assert!(!rc.is_cached(1) && !rc.in_kept_ring(1));
    }

    #[test]
    fn region_cache_op_msg_types_match_engine_switch() {
        assert_eq!(RegionCacheOp::CacheOut(1).msg_type(), Some(3));
        assert_eq!(RegionCacheOp::Clear(1).msg_type(), Some(7));
        assert_eq!(RegionCacheOp::CacheIn { region: 1, warm: false }.msg_type(), None);
    }

    #[test]
    fn active_density_region_picks_highest_priority_container() {
        let mut rc = RegionCache::new(RegionCacheConfig::default());
        rc.add_region(region(1, 0.0, 0.0, 100.0, 1)); // big low-priority
        rc.add_region(region(2, 0.0, 0.0, 20.0, 5)); // small high-priority, overlapping
        // Inside both -> the higher priority (key 2) drives density.
        assert_eq!(rc.active_density_region([0.0, 0.0, 0.0]), Some(2));
        // Inside only the big one.
        assert_eq!(rc.active_density_region([50.0, 0.0, 0.0]), Some(1));
        // Outside both.
        assert_eq!(rc.active_density_region([500.0, 0.0, 0.0]), None);
    }

    #[test]
    fn streaming_manager_delegates_region_cache() {
        let mut m = StreamingManager::new(StreamingConfig::default());
        m.add_region(region(1, 0.0, 0.0, 10.0, 0));
        assert_eq!(m.region_count(), 1);
        let d = m.update_regions([50.0, 0.0, 0.0]);
        assert_eq!(d.cache_in, vec![(1, false)]);
        assert_eq!(m.cached_region_count(), 1);
        assert!(m.is_region_cached(1));
        assert_eq!(m.active_density_region([0.0, 0.0, 0.0]), Some(1));
    }
}
