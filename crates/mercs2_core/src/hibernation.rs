//! The hibernation grid — retail's per-object wake/sleep decision (PC v1.0 `FUN_00735640` and its
//! registration `FUN_00516280`; `docs/reverse_engineer/hibernation_grid.md`).
//!
//! Every registered entity sits in a 32×32 grid of 256 m cells with its position quantised to about a
//! metre, a radius `r14` from its `HibernationControl.Distance`, three ring distances and a "base"
//! radius. Each frame the job recomputes every entry's state byte against the viewers (each local
//! player's camera target): the low nibble is the viewer mask (bit `v` = awake for viewer `v`), the high
//! nibble the ring tier 0x10..0x40. A change becomes a [`HibernationEvent`]; the host activates the
//! entity when its mask turns non-zero, deactivates it when the mask turns zero, and sets its model
//! streaming level from the tier. Pure data and arithmetic, bit-exact with retail's f32 operations.

use std::collections::{BTreeMap, HashMap};

/// Grid origin on X and Z (`[grid+0xC]` / `[grid+0x14]`).
pub const GRID_ORIGIN: f32 = -4096.0;
/// Cell edge in metres (`[grid+0x24]` = 2^8).
pub const CELL_SIZE: i32 = 256;
/// Cells per row and column (`[grid+6]` / `[grid+8]`).
pub const GRID_DIM: i32 = 32;
/// The quantisation scale of a position within its cell (`[0xBEAEAC]` = 0x3F7F0000).
pub const SUB_ENCODE: f32 = 0.996_093_75;
/// The inverse scale the job decodes with (`[0xBEAEA8]` = 0x3F808081).
pub const SUB_DECODE: f32 = f32::from_bits(0x3F80_8081);
/// The vertical range the y quantisation saturates at.
pub const Y_MIN: i32 = -512;
pub const Y_MAX: i32 = 512;
/// The wake/sleep band and the ring hysteresis, metres (`[grid+0x3C]`).
pub const MARGIN: f32 = 7.0;
/// The radius multiplier (`[0xBAAB38]` = 0x3F99999A).
pub const RADIUS_SCALE: f32 = f32::from_bits(0x3F99_999A);
/// `HibernationControl` defaults when neither the entity nor its template has one.
pub const DEFAULT_DISTANCE: u16 = 100;
pub const DEFAULT_RINGS: [u8; 3] = [0xA0, 0x3C, 0x14];
/// At most this many viewers (local and remote players).
pub const MAX_VIEWERS: usize = 4;

/// The hibernation scale (`FUN_0050C830`): `k = min(scale, 1)` with the config's scale and minimum
/// (single player 1.0 / 0, multiplayer 0.5 / 50), and the factors derived from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HibernationScale {
    /// `[grid+0x38]`.
    pub k: f32,
    /// `[0xD1BF34]` = 1/k.
    pub inv_k: f32,
    /// `[0x1175DC8]` = (1/k) · 1.2.
    pub radius_factor: f32,
    /// `[0x1175DC4]` = minimum hibernation / k.
    pub min_distance: f32,
}

impl HibernationScale {
    pub fn new(scale: f32, min_hibernation: f32) -> HibernationScale {
        let k = scale.min(1.0);
        let inv_k = 1.0 / k;
        HibernationScale { k, inv_k, radius_factor: inv_k * RADIUS_SCALE, min_distance: min_hibernation / k }
    }

    /// The single-player values (`[0xD1E3E0]` = 1.0, `[0x1175DA4]` = 0).
    pub fn single_player() -> HibernationScale {
        HibernationScale::new(1.0, 0.0)
    }

    /// An entity's quantised radius `r14` (`0x516C2C..0x516C5E`): static entities
    /// `cvtt(1.2 · min(dist0/k, max(dist0, min/k))) & 0x3FFF`; the others `min(0x3FFF, trunc(dist0 · 1.2/k))`.
    pub fn radius(&self, dist0: u16, is_static: bool) -> u16 {
        let d = f32::from(dist0);
        if is_static {
            let r = RADIUS_SCALE * (self.inv_k * d).min(d.max(self.min_distance));
            (cvtt(r) as u32 & 0x3FFF) as u16
        } else {
            (cvtt(d * self.radius_factor).clamp(0, 0x3FFF)) as u16
        }
    }
}

/// x86 `cvttss2si`: truncate toward zero; out of range → `i32::MIN` (the "integer indefinite").
fn cvtt(v: f32) -> i32 {
    if v.is_nan() || v >= 2_147_483_648.0 || v < -2_147_483_648.0 {
        i32::MIN
    } else {
        v as i32
    }
}

/// A position quantised into the grid (`FUN_00516B10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quantized {
    /// `32 · row + col` (no clamp).
    pub cell: i32,
    pub bx: u8,
    pub bz: u8,
    pub ylo: u8,
    pub yhi: u8,
}

/// Quantise a world position: per axis `d = f32(p + 4096)`, `c = cvtt(d) >> 8`,
/// `b = cvtt(f32((d − 256c) · 0.99609375))`; `y` saturated to ±512 and split into a high pair of bits
/// and a low byte.
pub fn quantize(pos: [f32; 3]) -> Quantized {
    let axis = |p: f32| {
        let d = p - GRID_ORIGIN;
        let c = cvtt(d) >> 8;
        let b = cvtt((d - (c * CELL_SIZE) as f32) * SUB_ENCODE);
        (c, b as u8)
    };
    let (col, bx) = axis(pos[0]);
    let (row, bz) = axis(pos[2]);
    let iy = cvtt(pos[1]);
    let (ylo, yhi) = if iy >= Y_MAX {
        (0xFF, 3)
    } else if iy <= Y_MIN {
        (0, 0)
    } else {
        let v = iy - Y_MIN;
        ((v & 0xFF) as u8, (v >> 8) as u8)
    };
    Quantized { cell: GRID_DIM * row + col, bx, bz, ylo, yhi }
}

impl Quantized {
    /// The cell's minimum corner on X and Z.
    pub fn cell_origin(&self) -> (f32, f32) {
        let col = self.cell.rem_euclid(GRID_DIM);
        let row = self.cell.div_euclid(GRID_DIM);
        (GRID_ORIGIN + (col * CELL_SIZE) as f32, GRID_ORIGIN + (row * CELL_SIZE) as f32)
    }

    /// The decoded XZ position the job measures (`cellX + b · 1.0039216`).
    pub fn decoded_xz(&self) -> (f32, f32) {
        let (cx, cz) = self.cell_origin();
        (f32::from(self.bx) * SUB_DECODE + cx, f32::from(self.bz) * SUB_DECODE + cz)
    }

    /// The decoded height (`hi · 256 + ylo − 512`).
    pub fn decoded_y(&self) -> f32 {
        (i32::from(self.yhi) * 256 + i32::from(self.ylo) + Y_MIN) as f32
    }
}

/// What registering an entity needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Registration {
    pub key: u32,
    /// The quantiser's input: the SceneObject position, or its render-node centre when it has one.
    pub position: [f32; 3],
    /// `HibernationControl.Distance` (own, else the template's, else 100).
    pub distance: u16,
    /// `Q2Distance`, `Q1Distance`, `Q0Distance`.
    pub rings: [u8; 3],
    /// The render node's radius, truncated (0 without a node; TerrainObject 10000 at first registration).
    pub base: u16,
    /// Not TerrainObject / LowResTerrainObject / SceneObject `+0x1A` bit2.
    pub is_static: bool,
}

/// One grid entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridEntry {
    pub key: u32,
    pub q: Quantized,
    pub r14: u16,
    pub rings: [u8; 3],
    pub base: u16,
    pub is_static: bool,
    /// Low nibble: viewer mask; high nibble: ring tier 0x10..0x40.
    pub state: u8,
}

/// A viewer: a player's camera target position; `local` = a joined local player (only those raise
/// the ring tier).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewer {
    pub position: [f32; 3],
    pub local: bool,
}

/// A state change the job emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HibernationEvent {
    pub key: u32,
    pub old: u8,
    pub new: u8,
}

impl HibernationEvent {
    /// Whether the viewer mask (the wake decision) changed.
    pub fn mask_changed(&self) -> bool {
        (self.old ^ self.new) & 0x0F != 0
    }
}

/// The grid: entries by cell, viewers and last frame's viewers, and the scale.
#[derive(Clone, Debug)]
pub struct HibernationGrid {
    scale: HibernationScale,
    cells: BTreeMap<i32, Vec<GridEntry>>,
    index: HashMap<u32, i32>,
    viewers: [Option<Viewer>; MAX_VIEWERS],
    previous: [Option<[f32; 3]>; MAX_VIEWERS],
}

impl HibernationGrid {
    pub fn new(scale: HibernationScale) -> HibernationGrid {
        HibernationGrid { scale, cells: BTreeMap::new(), index: HashMap::new(), viewers: [None; MAX_VIEWERS], previous: [None; MAX_VIEWERS] }
    }

    pub fn scale(&self) -> &HibernationScale {
        &self.scale
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains(&self, key: u32) -> bool {
        self.index.contains_key(&key)
    }

    /// The entry of `key`, if registered.
    pub fn entry(&self, key: u32) -> Option<&GridEntry> {
        let cell = self.index.get(&key)?;
        self.cells.get(cell)?.iter().find(|e| e.key == key)
    }

    /// Register an entity (`FUN_00516C00`) with state 0, or with its current viewer mask when
    /// `compute_mask` (a runtime spawn, `[0x1175DD4]`); returns the state it was given. A key already
    /// registered is re-celled ([`moved`](Self::moved)).
    pub fn insert(&mut self, r: &Registration, compute_mask: bool) -> u8 {
        if self.contains(r.key) {
            self.moved(r);
            return self.entry(r.key).map_or(0, |e| e.state);
        }
        let q = quantize(r.position);
        let r14 = self.scale.radius(r.distance, r.is_static);
        let mut e = GridEntry { key: r.key, q, r14, rings: r.rings, base: r.base, is_static: r.is_static, state: 0 };
        if compute_mask {
            e.state = self.spawn_mask(&e);
        }
        self.cells.entry(q.cell).or_default().push(e);
        self.index.insert(r.key, q.cell);
        e.state
    }

    /// `0x516C00`'s mask for a runtime spawn: bit `v` when the viewer is within the radius on XZ.
    fn spawn_mask(&self, e: &GridEntry) -> u8 {
        let r = f32::from(e.r14) * self.scale.k;
        let (x, z) = e.q.decoded_xz();
        self.viewers.iter().enumerate().fold(0u8, |m, (v, vw)| match vw {
            Some(vw) => {
                let (dx, dz) = (x - vw.position[0], z - vw.position[2]);
                if dx * dx + dz * dz < r * r {
                    m | (1 << v)
                } else {
                    m
                }
            }
            None => m,
        })
    }

    /// A registered entity moved or changed (`FUN_00516F00`): recompute its radius and position; same
    /// cell → rewritten in place; another cell → moved there keeping its state and rings.
    pub fn moved(&mut self, r: &Registration) {
        let Some(&old_cell) = self.index.get(&r.key) else { return };
        let q = quantize(r.position);
        let r14 = self.scale.radius(r.distance, r.is_static);
        let list = self.cells.get_mut(&old_cell).expect("indexed cell");
        let i = list.iter().position(|e| e.key == r.key).expect("indexed entry");
        if q.cell == old_cell {
            let e = &mut list[i];
            e.q = q;
            e.r14 = r14;
            e.base = r.base;
            e.is_static = r.is_static;
            return;
        }
        let mut e = list.swap_remove(i);
        if list.is_empty() {
            self.cells.remove(&old_cell);
        }
        e.q = q;
        e.r14 = r14;
        e.base = r.base;
        e.is_static = r.is_static;
        self.cells.entry(q.cell).or_default().push(e);
        self.index.insert(r.key, q.cell);
    }

    /// Remove an entity from the grid (`0x5181B0` / `0x517010`); returns its last state.
    pub fn remove(&mut self, key: u32) -> Option<u8> {
        let cell = self.index.remove(&key)?;
        let list = self.cells.get_mut(&cell)?;
        let i = list.iter().position(|e| e.key == key)?;
        let e = list.swap_remove(i);
        if list.is_empty() {
            self.cells.remove(&cell);
        }
        Some(e.state)
    }

    /// Set this frame's viewers (`FUN_0050C830`).
    pub fn set_viewers(&mut self, viewers: [Option<Viewer>; MAX_VIEWERS]) {
        self.viewers = viewers;
    }

    /// The minimum Chebyshev distance from a cell's box to any current or last-frame viewer
    /// (`FUN_00735580` / `FUN_00735480`); `f32::MAX` with none.
    fn cell_distance(&self, cell: i32) -> f32 {
        let col = cell.rem_euclid(GRID_DIM);
        let row = cell.div_euclid(GRID_DIM);
        let x0 = GRID_ORIGIN + (col * CELL_SIZE) as f32;
        let z0 = GRID_ORIGIN + (row * CELL_SIZE) as f32;
        let (x1, z1) = (x0 + CELL_SIZE as f32, z0 + CELL_SIZE as f32);
        let gap = |v: f32, lo: f32, hi: f32| if v <= lo { lo - v } else if v <= hi { 0.0 } else { v - hi };
        let cheb = |p: [f32; 3]| gap(p[0], x0, x1).max(gap(p[2], z0, z1));
        let mut best = f32::MAX;
        for v in 0..MAX_VIEWERS {
            let mut d = f32::MAX;
            if let Some(vw) = self.viewers[v] {
                d = cheb(vw.position);
            }
            if let Some(p) = self.previous[v] {
                d = d.min(cheb(p));
            }
            best = best.min(d);
        }
        best
    }

    /// One frame of the job (`FUN_00735640` over every cell): recompute every entry's state and return
    /// the changes, then remember this frame's viewers as last frame's (`FUN_00516020`).
    pub fn step(&mut self) -> Vec<HibernationEvent> {
        let mut events = Vec::new();
        let scale = self.scale;
        let viewers = self.viewers;
        let cells: Vec<i32> = self.cells.keys().copied().collect();
        for cell in cells {
            let t = cvtt_f64(f64::from(self.cell_distance(cell) - MARGIN));
            let list = self.cells.get_mut(&cell).expect("cell");
            for e in list.iter_mut() {
                let new = if i32::from(e.r14) < t { 0 } else { evaluate(e, &viewers, &scale) };
                if new != e.state {
                    events.push(HibernationEvent { key: e.key, old: e.state, new });
                    e.state = new;
                }
            }
        }
        for v in 0..MAX_VIEWERS {
            self.previous[v] = self.viewers[v].map(|vw| vw.position);
        }
        events
    }
}

/// `cvttsd2si`: truncate toward zero; out of range → `i32::MIN`.
fn cvtt_f64(v: f64) -> i32 {
    if v.is_nan() || v >= 2_147_483_648.0 || v < -2_147_483_648.0 {
        i32::MIN
    } else {
        v as i32
    }
}

/// One entry's new state byte from its current one (`FUN_00735640` `0x735793..0x735BBE`), for an entry
/// the cell prefilter did not cull.
pub fn evaluate(e: &GridEntry, viewers: &[Option<Viewer>; MAX_VIEWERS], scale: &HibernationScale) -> u8 {
    let r = f32::from(e.r14) * scale.k;
    let out = (r + MARGIN) * (r + MARGIN);
    let inner = r * r;
    let (x, z) = e.q.decoded_xz();
    let y = e.q.decoded_y();
    let ringed = e.state & 0xF0 != 0;
    let (mut mask, mut min_h, mut min_3, mut remote) = (0u8, f32::MAX, f32::MAX, false);
    for (v, vw) in viewers.iter().enumerate() {
        let Some(vw) = vw else { continue };
        let (dx, dz) = (x - vw.position[0], z - vw.position[2]);
        let d2 = dx * dx + dz * dz;
        if d2 < out && (d2 < inner || ringed) {
            mask |= 1 << v;
            if vw.local {
                min_h = min_h.min(d2);
                let dy = y - vw.position[1];
                min_3 = min_3.min(dy * dy + d2);
            } else {
                remote = true;
            }
        }
    }
    let mut tier = 0u8;
    if min_h < out {
        if inner <= min_h {
            if ringed {
                tier = 0x10;
            }
        } else {
            let base = f32::from(e.base);
            let old = e.state;
            let band = |t: f32, lo: u8, hi: u8, deeper: &dyn Fn() -> u8| -> u8 {
                if (t + MARGIN) * (t + MARGIN) <= min_3 {
                    lo
                } else if t * t <= min_3 {
                    if old < hi { lo } else { hi }
                } else {
                    deeper()
                }
            };
            let t3 = base + f32::from(e.rings[2]);
            let t2 = base + f32::from(e.rings[1]);
            let t1 = base + f32::from(e.rings[0]);
            tier = band(t1, 0x10, 0x20, &|| band(t2, 0x20, 0x30, &|| band(t3, 0x30, 0x40, &|| 0x40)));
        }
    }
    if tier == 0 && remote {
        tier = 0x10;
    }
    mask | tier
}

/// The SceneObject's quaternion as the Transform reader stores it (`0x63D7F0..0x63D837`): each
/// component × 32000, truncated to an i16.
pub fn scene_object_quat(q: [f32; 4]) -> [i16; 4] {
    let k = f32::from_bits(0x46FA_0000);
    q.map(|v| cvtt(v * k) as i16)
}

/// The quantiser's input for an entity with a render node (`0x515FD2..0x516010`): the SceneObject
/// position plus its i16 quaternion applied to the node's model-space centre. `0x434F80` scales the
/// i16s by `1/32000`; `0x435030` normalises (x87 sqrt, modelled in double); `0x824D90` runs
/// D3DXQuaternionNormalize (RSQRTSS plus one Newton step — the CPU's RSQRTSS table is not
/// reproduced, the correctly rounded reciprocal square root stands in), D3DXMatrixRotationQuaternion
/// (x87) and D3DXVec3TransformNormal (`(y·M1 + x·M0) + z·M2`); the sum is f32.
pub fn node_input(pos: [f32; 3], q: [i16; 4], centre: [f32; 3]) -> [f32; 3] {
    let k = f32::from_bits(0x3803_126F);
    let q0 = q.map(|v| f32::from(v) * k);
    let s = ((q0[0] * q0[0] + q0[1] * q0[1]) + q0[2] * q0[2]) + q0[3] * q0[3];
    let q1 = if s < f32::from_bits(0x38D1_B717) {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        let inv = 1.0f64 / f64::from(s).sqrt();
        q0.map(|v| (f64::from(v) * inv) as f32)
    };
    let s2 = (q1[0] * q1[0] + q1[1] * q1[1]) + (q1[2] * q1[2] + q1[3] * q1[3]);
    let r = (1.0f64 / f64::from(s2).sqrt()) as f32;
    let kk = ((3.0f32 - s2 * (r * r)) * 0.5) * r;
    let (x, y, z, w) = (q1[0] * kk, q1[1] * kk, q1[2] * kk, q1[3] * kk);
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let d = f64::from;
    let m = [
        [(1.0 - d(yy) - d(zz)) as f32, (d(xy) + d(wz)) as f32, (d(xz) - d(wy)) as f32],
        [(d(xy) - d(wz)) as f32, (1.0 - d(xx) - d(zz)) as f32, (d(yz) + d(wx)) as f32],
        [(d(xz) + d(wy)) as f32, (d(yz) - d(wx)) as f32, (1.0 - d(xx) - d(yy)) as f32],
    ];
    let t = |j: usize| (centre[1] * m[1][j] + centre[0] * m[0][j]) + centre[2] * m[2][j];
    [pos[0] + t(0), pos[1] + t(1), pos[2] + t(2)]
}

/// What the event consumer (`FUN_004F8460` `0x4F84C1..0x4F85F7`) makes of one frame's events, for the
/// entities it handles (those with a component signature, `0xDF6D08`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeDecisions {
    /// Mask turned 0: deactivated now, in event order (`FUN_004F6AB0`).
    pub deactivate: Vec<u32>,
    /// Mask changed to non-zero: activated deferred (`FUN_004F8DA0(e, 1, 0, 1)`), in this order —
    /// largest own `HibernationControl.Distance` first (100 without one).
    pub activate: Vec<u32>,
}

/// Split one frame's events into deactivations and the ordered activations. `handled` is "has a
/// component signature"; `removed` is SceneObject `+0x1A` bit3 (`Object.Remove`: never re-woken);
/// `own_distance` is the entity's own `HibernationControl.Distance`.
pub fn wake_decisions(
    events: &[HibernationEvent],
    handled: impl Fn(u32) -> bool,
    removed: impl Fn(u32) -> bool,
    own_distance: impl Fn(u32) -> Option<u16>,
) -> WakeDecisions {
    let mut out = WakeDecisions::default();
    let (mut dist, mut keys) = (Vec::new(), Vec::new());
    for ev in events.iter().filter(|e| e.mask_changed() && handled(e.key)) {
        if ev.new & 0x0F == 0 {
            out.deactivate.push(ev.key);
        } else if !removed(ev.key) {
            keys.push(ev.key);
            dist.push(f32::from(own_distance(ev.key).unwrap_or(DEFAULT_DISTANCE)));
        }
    }
    sort_by_distance(&mut dist, &mut keys);
    out.activate = keys.into_iter().rev().collect();
    out
}

/// `FUN_004E08F5`: sort `keys` ascending, carrying `values` along — an explicit-stack quicksort with a
/// middle pivot, ranges under 8 handed to the insertion sort `FUN_004E0A20`. Not stable; ported as is so
/// equal keys end in retail's order.
pub fn sort_by_distance(keys: &mut [f32], values: &mut [u32]) {
    assert_eq!(keys.len(), values.len());
    if keys.len() < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = vec![(0, keys.len() - 1)];
    while let Some((lo, hi)) = stack.pop() {
        if hi - lo + 1 < 8 {
            insertion_sort(&mut keys[lo..=hi], &mut values[lo..=hi]);
            continue;
        }
        let pivot = keys[(lo + hi) >> 1];
        let (mut i, mut j) = (lo as isize, hi as isize);
        loop {
            while pivot > keys[i as usize] {
                i += 1;
            }
            while keys[j as usize] > pivot {
                j -= 1;
            }
            if i > j {
                break;
            }
            keys.swap(i as usize, j as usize);
            values.swap(i as usize, j as usize);
            i += 1;
            j -= 1;
            if i > j {
                break;
            }
        }
        // `0x4E09D1..0x4E09F0`: the right part is pushed first, so the left part is sorted next.
        if (i as usize) < hi {
            stack.push((i as usize, hi));
        }
        if (lo as isize) < j {
            stack.push((lo, j as usize));
        }
    }
}

/// `FUN_004E0A20`: insertion sort, an element moving left past every strictly greater one.
fn insertion_sort(keys: &mut [f32], values: &mut [u32]) {
    for n in 1..keys.len() {
        let (k, v) = (keys[n], values[n]);
        let mut i = n;
        while i > 0 && keys[i - 1] > k {
            keys[i] = keys[i - 1];
            values[i] = values[i - 1];
            i -= 1;
        }
        keys[i] = k;
        values[i] = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(key: u32, pos: [f32; 3], distance: u16) -> Registration {
        Registration { key, position: pos, distance, rings: DEFAULT_RINGS, base: 0, is_static: true }
    }

    fn one_viewer(pos: [f32; 3]) -> [Option<Viewer>; MAX_VIEWERS] {
        [Some(Viewer { position: pos, local: true }), None, None, None]
    }

    #[test]
    fn radius_formulas() {
        let sp = HibernationScale::single_player();
        assert_eq!(sp.radius(100, true), 120);
        assert_eq!(sp.radius(100, false), 120);
        assert_eq!(sp.radius(65535, false), 0x3FFF, "non-static clamps");
        assert_eq!(u32::from(sp.radius(65535, true)), (78642u32) & 0x3FFF, "static masks, no clamp");
        let mp = HibernationScale::new(0.5, 50.0);
        // static: 1.2·min(2·dist0, max(dist0, 100)) — dist0 40 → 1.2·min(80, 100) = 96; dist0 200 → 1.2·200 = 240.
        assert_eq!((mp.radius(40, true), mp.radius(200, true)), (96, 240));
        assert_eq!(mp.radius(200, false), 480, "non-static 2.4·dist0");
    }

    #[test]
    fn quantize_round_trips_within_a_metre() {
        let q = quantize([2817.0105, 11.593, 348.67]);
        let (x, z) = q.decoded_xz();
        assert!((x - 2817.0105).abs() < 1.1 && (z - 348.67).abs() < 1.1);
        assert_eq!(q.decoded_y(), 11.0);
        assert_eq!(q.cell, GRID_DIM * ((348.67f32 + 4096.0) as i32 >> 8) + ((2817.0105f32 + 4096.0) as i32 >> 8));
        assert_eq!((quantize([0.0, 900.0, 0.0]).ylo, quantize([0.0, 900.0, 0.0]).yhi), (0xFF, 3));
        assert_eq!((quantize([0.0, -900.0, 0.0]).ylo, quantize([0.0, -900.0, 0.0]).yhi), (0, 0));
    }

    /// Wake inside R (120 m for dist0 100), stay awake to R + 7, sleep beyond.
    #[test]
    fn wake_band_and_hysteresis() {
        let mut g = HibernationGrid::new(HibernationScale::single_player());
        g.insert(&reg(1, [0.0, 0.0, 0.0], 100), false);
        g.set_viewers(one_viewer([125.0, 0.0, 0.0]));
        assert!(g.step().is_empty(), "125 m: outside R, not yet awake");
        g.set_viewers(one_viewer([110.0, 0.0, 0.0]));
        let ev = g.step();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].mask_changed() && ev[0].new & 1 == 1, "110 m wakes");
        g.set_viewers(one_viewer([125.0, 0.0, 0.0]));
        let ev = g.step();
        assert!(ev.iter().all(|e| !e.mask_changed()), "125 m: inside R+7, stays awake");
        assert_eq!(g.entry(1).unwrap().state & 0x0F, 1);
        g.set_viewers(one_viewer([130.0, 0.0, 0.0]));
        let ev = g.step();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].new & 0x0F, 0, "beyond R+7 sleeps");
    }

    /// The ring tier climbs to 0x40 near the viewer (3-D, base + rings, 7 m bands).
    #[test]
    fn ring_tier_by_distance() {
        let mut g = HibernationGrid::new(HibernationScale::single_player());
        g.insert(&reg(1, [0.0, 0.0, 0.0], 400), false);
        g.set_viewers(one_viewer([10.0, 0.0, 0.0]));
        g.step();
        assert_eq!(g.entry(1).unwrap().state, 0x41, "within Q0Distance 20: full detail");
        g.set_viewers(one_viewer([100.0, 0.0, 0.0]));
        g.step();
        assert_eq!(g.entry(1).unwrap().state, 0x21, "between Q1 60 and Q2 160");
        g.set_viewers(one_viewer([300.0, 0.0, 0.0]));
        g.step();
        assert_eq!(g.entry(1).unwrap().state, 0x11, "beyond Q2 + 7");
    }

    /// The Chebyshev prefilter zeroes far entries (with an event when they had a state).
    #[test]
    fn far_cells_are_culled() {
        let mut g = HibernationGrid::new(HibernationScale::single_player());
        g.insert(&reg(1, [0.0, 0.0, 0.0], 100), false);
        g.set_viewers(one_viewer([10.0, 0.0, 0.0]));
        g.step();
        g.set_viewers(one_viewer([3000.0, 0.0, 3000.0]));
        g.step(); // last frame's viewer still near: entry measured, falls asleep by distance
        let st = g.entry(1).unwrap().state;
        assert_eq!(st & 0x0F, 0);
        g.step();
        assert_eq!(g.entry(1).unwrap().state, 0);
    }

    /// The quicksort leaves keys ascending with each value beside its key, below and above the
    /// insertion-sort cut-over of 8.
    #[test]
    fn sort_by_distance_orders_keys_and_carries_values() {
        for n in [0usize, 1, 2, 7, 8, 9, 64, 300] {
            let mut keys: Vec<f32> = (0..n).map(|i| ((i * 7919) % 101) as f32).collect();
            let mut values: Vec<u32> = (0..n as u32).collect();
            let before: Vec<(u32, f32)> = values.iter().copied().zip(keys.iter().copied()).collect();
            sort_by_distance(&mut keys, &mut values);
            assert!(keys.windows(2).all(|w| w[0] <= w[1]), "n {n}: {keys:?}");
            for (v, k) in values.iter().zip(&keys) {
                assert_eq!(before[*v as usize].1, *k, "n {n}: value {v} lost its key");
            }
        }
    }

    /// Mask → 0 deactivates in event order; mask → non-zero activates largest own distance first (100
    /// without one), skipping removed entities and unhandled ones; tier-only changes do nothing.
    #[test]
    fn wake_decisions_split_and_order() {
        let ev = |key, old, new| HibernationEvent { key, old, new };
        let events = [
            ev(1, 0x00, 0x11),
            ev(2, 0x11, 0x00),
            ev(3, 0x00, 0x21),
            ev(4, 0x00, 0x11),
            ev(5, 0x11, 0x21),
            ev(6, 0x00, 0x11),
            ev(7, 0x00, 0x11),
            ev(8, 0x21, 0x00),
        ];
        let d = wake_decisions(
            &events,
            |k| k != 7,
            |k| k == 6,
            |k| match k {
                1 => Some(50),
                3 => Some(400),
                _ => None,
            },
        );
        assert_eq!(d.deactivate, vec![2, 8]);
        assert_eq!(d.activate, vec![3, 4, 1]);
    }

    #[test]
    fn spawn_mask_and_recell() {
        let mut g = HibernationGrid::new(HibernationScale::single_player());
        g.set_viewers(one_viewer([5.0, 0.0, 5.0]));
        assert_eq!(g.insert(&reg(9, [0.0, 0.0, 0.0], 100), true), 1, "a spawn in range starts masked");
        assert_eq!(g.insert(&reg(10, [500.0, 0.0, 0.0], 100), true), 0);
        g.moved(&reg(9, [600.0, 0.0, 600.0], 100));
        assert_eq!(g.entry(9).unwrap().q, quantize([600.0, 0.0, 600.0]));
        assert_eq!(g.entry(9).unwrap().state, 1, "a move keeps the state");
        assert_eq!(g.remove(9), Some(1));
        assert!(!g.contains(9));
    }
}
