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

/// The LOD tier for a player distance `d` given LOD-tier distances `dist` (0 = finest .. 3 =
/// coarsest). Boundaries: `dist[3]` (0/1), `dist[2]` (1/2), `dist[1]` (2/3).
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
                        let nt = lod_tier_hyst(&e.dist, d, cur_tier, h);
                        if nt != cur_tier {
                            diff.tier_changes.push((e.key, nt));
                            self.awake.insert(e.key, nt);
                        }
                    }
                }
                None => {
                    if d <= out {
                        wake_req.push((d, e.key, lod_tier(&e.dist, d)));
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
}
