//! The decal **instance pool** + lifetime bookkeeping — the runtime the engine owns (code map §4).
//!
//! This is the recovered `CreateDecals` → `DecalsUpdate`/`DecalUnlock` runtime: spawn a projected
//! decal instance at a surface hit point, age it every frame, and free (`DecalUnlock`) it when its
//! lifetime elapses. The pool is **bounded** — decals are capped, so when full the spawn reuses the
//! **oldest** live instance (a projected-decal system never grows unbounded; `DecalUnlock` recycles).
//!
//! **Boundary (honest):**
//! - The exact **cap** is stripped/`confirm-live` (the `0x400`-byte alloc in §1 sizes the *table*
//!   object, not this pool). [`DEFAULT_POOL_CAP`] is a documented reimpl default, **not** a recovered
//!   number; the cap is a constructor argument so a loader/tuning can set the retail value.
//! - The exact **fade curve** is data-driven (`DecalsUpdate` is stripped). The reimpl fades linearly
//!   over the final [`FADE_FRACTION`] of each instance's lifetime — the standard aging discipline —
//!   and this is documented as a reimpl choice, not a recovered body.
//! - The projection **shader** (`PgDecalVP`/`PgDecal2FP` + `_pl`/`_sl`/… permutations, code map §2)
//!   is the render seam this crate hands off to — NOT implemented here. Each instance carries the
//!   projection *inputs* (position, surface normal, tangent, size, super flag) the draw pass consumes.

use mercs2_core::glam::Vec3;

use crate::table::DecalDef;

/// Documented reimpl default pool cap. **Not recovered** — the retail cap is `confirm-live`; supply it
/// via [`DecalPool::new`] when known. Chosen as a plausible projected-decal budget.
pub const DEFAULT_POOL_CAP: usize = 256;

/// Fraction of an instance's lifetime over which it fades out before expiry (the trailing window).
/// Reimpl choice — the exact `DecalsUpdate` fade curve is data/unrecovered.
pub const FADE_FRACTION: f32 = 0.25;

/// One live projected-decal instance — the projection inputs + its aging state.
///
/// Position/normal/tangent/size are the **render seam** inputs the (unimplemented here) decal draw
/// pass projects with; `age`/`lifetime`/`super_decal` are the runtime bookkeeping this pool owns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalInstance {
    /// Material row key (`DecalDef::key`) this instance draws with.
    pub def_key: u32,
    /// World-space projection centre (the surface hit point).
    pub position: Vec3,
    /// Surface normal — the projection axis (the decal projects along `-normal` onto the surface).
    pub normal: Vec3,
    /// Tangent — the decal's orientation (roll) about the normal.
    pub tangent: Vec3,
    /// Projection footprint size in world units (from the def, per-instance so it can be jittered).
    pub size: f32,
    /// Seconds this instance has been alive.
    pub age: f32,
    /// Total lifetime in seconds; `<= 0` = permanent (never ages out; only pool reuse evicts it).
    pub lifetime: f32,
    /// `EnableSuperDecal` higher-coverage variant (from the def) — selects the `_super` shader.
    pub super_decal: bool,
    /// Monotonic spawn sequence — the tie-breaker used to evict the oldest instance when the pool is
    /// full. Lower = older.
    seq: u64,
    /// Slot occupancy. A freed slot (`DecalUnlock`) is reused before evicting a live one.
    alive: bool,
}

impl DecalInstance {
    /// Alpha (opacity) `[0, 1]` for this instance from its aging — `1.0` until the trailing
    /// [`FADE_FRACTION`] of its lifetime, then linear to `0.0` at expiry. Permanent decals
    /// (`lifetime <= 0`) never fade. This is the value the render seam multiplies the decal by.
    pub fn alpha(&self) -> f32 {
        if self.lifetime <= 0.0 {
            return 1.0;
        }
        let remaining = (self.lifetime - self.age).max(0.0);
        let fade_window = self.lifetime * FADE_FRACTION;
        if fade_window <= 0.0 || remaining >= fade_window {
            1.0
        } else {
            remaining / fade_window
        }
    }

    /// Whether this instance is occupied (live).
    pub fn is_alive(&self) -> bool {
        self.alive
    }
}

/// The bounded projected-decal instance pool — `CreateDecals`/`DecalsUpdate`/`DecalUnlock` runtime.
///
/// Fixed-capacity slot array. A spawn prefers a free slot; when all slots are live it evicts the
/// **oldest** (lowest spawn sequence) — the recovered "decals are capped, recycle" discipline.
#[derive(Clone, Debug)]
pub struct DecalPool {
    slots: Vec<DecalInstance>,
    cap: usize,
    /// Next spawn sequence number (monotonic).
    next_seq: u64,
}

impl Default for DecalPool {
    fn default() -> Self {
        DecalPool::new(DEFAULT_POOL_CAP)
    }
}

impl DecalPool {
    /// A pool bounded at `cap` instances. `cap` is the (data-driven) budget — the retail value is
    /// `confirm-live`; [`DEFAULT_POOL_CAP`] is the reimpl default.
    pub fn new(cap: usize) -> Self {
        DecalPool { slots: Vec::new(), cap: cap.max(1), next_seq: 0 }
    }

    /// `CreateDecals` — spawn a projected decal from a table `def` at a surface hit. `position` is the
    /// hit point, `normal` the surface normal (projection axis), `tangent` the roll orientation. Pulls
    /// size / lifetime / super flag from the def. Returns the slot index of the new instance.
    ///
    /// Reuse policy: fills a free slot if any; else, once at capacity, evicts the oldest live instance
    /// (the recovered bounded-pool recycle).
    pub fn spawn(&mut self, def: &DecalDef, position: Vec3, normal: Vec3, tangent: Vec3) -> usize {
        let seq = self.next_seq;
        self.next_seq += 1;
        let inst = DecalInstance {
            def_key: def.key,
            position,
            normal,
            tangent,
            size: def.size,
            age: 0.0,
            lifetime: def.lifetime,
            super_decal: def.super_decal,
            seq,
            alive: true,
        };

        // Prefer a free (previously unlocked) slot.
        if let Some(idx) = self.slots.iter().position(|s| !s.alive) {
            self.slots[idx] = inst;
            return idx;
        }
        // Grow until the cap.
        if self.slots.len() < self.cap {
            self.slots.push(inst);
            return self.slots.len() - 1;
        }
        // Full → evict the oldest live instance (lowest seq).
        let idx = self
            .slots
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| s.seq)
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.slots[idx] = inst;
        idx
    }

    /// `DecalsUpdate` — age every live instance by `dt` and `DecalUnlock` (free) any whose lifetime
    /// elapsed. Permanent instances (`lifetime <= 0`) never age out. Returns how many were freed.
    pub fn update(&mut self, dt: f32) -> usize {
        let mut freed = 0;
        for s in self.slots.iter_mut() {
            if !s.alive {
                continue;
            }
            s.age += dt;
            if s.lifetime > 0.0 && s.age >= s.lifetime {
                s.alive = false;
                freed += 1;
            }
        }
        freed
    }

    /// Number of live instances.
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|s| s.alive).count()
    }

    /// The configured capacity.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Iterate the live instances (the render seam draws these).
    pub fn iter_live(&self) -> impl Iterator<Item = &DecalInstance> {
        self.slots.iter().filter(|s| s.alive)
    }

    /// Free every instance (e.g. on world unload).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.next_seq = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(key: u32, lifetime: f32) -> DecalDef {
        DecalDef { lifetime, ..DecalDef::placeholder(key) }
    }

    #[test]
    fn spawn_records_projection_inputs_from_def() {
        let mut pool = DecalPool::new(8);
        let d = DecalDef { size: 2.5, super_decal: true, ..def(0xABCD, 10.0) };
        let idx = pool.spawn(&d, Vec3::new(1.0, 2.0, 3.0), Vec3::Y, Vec3::X);
        let inst = pool.iter_live().next().unwrap();
        assert_eq!(idx, 0);
        assert_eq!(inst.def_key, 0xABCD);
        assert_eq!(inst.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(inst.size, 2.5);
        assert_eq!(inst.lifetime, 10.0);
        assert!(inst.super_decal);
    }

    #[test]
    fn instances_age_out_and_are_freed() {
        let mut pool = DecalPool::new(8);
        pool.spawn(&def(1, 1.0), Vec3::ZERO, Vec3::Y, Vec3::X);
        assert_eq!(pool.live_count(), 1);
        assert_eq!(pool.update(0.5), 0, "still within lifetime");
        assert_eq!(pool.live_count(), 1);
        assert_eq!(pool.update(0.6), 1, "crossed lifetime → freed (DecalUnlock)");
        assert_eq!(pool.live_count(), 0);
    }

    #[test]
    fn permanent_decal_never_ages_out() {
        let mut pool = DecalPool::new(8);
        pool.spawn(&def(1, 0.0), Vec3::ZERO, Vec3::Y, Vec3::X); // lifetime <= 0 = permanent
        pool.update(1000.0);
        assert_eq!(pool.live_count(), 1);
        assert_eq!(pool.iter_live().next().unwrap().alpha(), 1.0);
    }

    #[test]
    fn freed_slot_is_reused_before_growing() {
        let mut pool = DecalPool::new(8);
        pool.spawn(&def(1, 1.0), Vec3::ZERO, Vec3::Y, Vec3::X);
        pool.update(2.0); // frees slot 0
        let idx = pool.spawn(&def(2, 1.0), Vec3::ONE, Vec3::Y, Vec3::X);
        assert_eq!(idx, 0, "reuses the freed slot, does not grow");
        assert_eq!(pool.live_count(), 1);
    }

    #[test]
    fn full_pool_evicts_oldest() {
        let mut pool = DecalPool::new(2);
        pool.spawn(&def(1, 100.0), Vec3::ZERO, Vec3::Y, Vec3::X); // seq 0 (oldest)
        pool.spawn(&def(2, 100.0), Vec3::ONE, Vec3::Y, Vec3::X); // seq 1
        assert_eq!(pool.live_count(), 2);
        // Pool full → the third spawn evicts the oldest (def_key 1).
        pool.spawn(&def(3, 100.0), Vec3::X, Vec3::Y, Vec3::X); // seq 2
        assert_eq!(pool.live_count(), 2, "cap holds at 2");
        let keys: Vec<u32> = pool.iter_live().map(|i| i.def_key).collect();
        assert!(!keys.contains(&1), "oldest (key 1) was recycled");
        assert!(keys.contains(&2) && keys.contains(&3));
    }

    #[test]
    fn alpha_fades_linearly_over_the_trailing_window() {
        let mut pool = DecalPool::new(4);
        pool.spawn(&def(1, 10.0), Vec3::ZERO, Vec3::Y, Vec3::X); // fade window = last 2.5s
        // t=7 (remaining 3.0 > 2.5 window) → full opacity.
        pool.update(7.0);
        assert_eq!(pool.iter_live().next().unwrap().alpha(), 1.0);
        // t=8.75 (remaining 1.25 = half the 2.5 window) → ~0.5.
        pool.update(1.75);
        let a = pool.iter_live().next().unwrap().alpha();
        assert!((a - 0.5).abs() < 1e-5, "half-faded, got {a}");
    }

    #[test]
    fn capacity_reported() {
        assert_eq!(DecalPool::new(64).capacity(), 64);
        assert_eq!(DecalPool::default().capacity(), DEFAULT_POOL_CAP);
    }
}
