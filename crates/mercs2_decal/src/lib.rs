//! `mercs2_decal` — Decals (scoreboard row 6).
//!
//! **Code map:** `docs/reverse_engineer/decal_code_map.md` (the sky/decal/water PC code maps: the
//! `decaltable` loader + the decal pass driver).
//! **Depends only on** `mercs2_core` + `mercs2_formats`.
//!
//! Per the code map's §0 boundary, the decal system's *setup* half is statically recovered (the
//! `decaltable` ASET loader, the shader registration, the two ECS decal-toggle components) while the
//! *create / project / render / GC* half has its profiler-marker strings stripped from the retail PC
//! build and is data/vtable-driven. This crate supplies the **mechanism the engine owns** — the
//! bookkeeping, not the GPU draw:
//!
//! - [`table`] — the [`DecalTable`]/[`DecalDef`] `decaltable` loader (type `0x3B0AABF8`, §1) as
//!   loadable data + lookup;
//! - [`pool`] — the [`DecalPool`] bounded instance pool: `CreateDecals` spawn +
//!   `DecalsUpdate`/`DecalUnlock` aging/GC + reuse-oldest recycle + lifetime fade (§4);
//! - [`components`] — the [`DisableDecals`]/[`Disable3DDecals`] ECS toggle tags (§3).
//!
//! At the crate root, [`DecalWorld`] is the table+pool pair the host holds and ticks: `spawn` /
//! `spawn_by_key` / `spawn_on_entity` (`CreateDecals`, the last honouring the §3 suppression tags),
//! `update` (`DecalsUpdate`/`DecalUnlock`) once per fixed step, and `iter_live` for the render seam.
//! Library only — no binaries.
//!
//! The projection **shader** (`PgDecalVP`/`PgDecal2FP` + `_pl`/`_sl`/`_pl_sl`/`_li` light permutations
//! and the `decalNormal`/`decalParam` bind slots, §2) and the actual **draw** are the render seam this
//! crate hands off to `mercs2_engine`: each [`pool::DecalInstance`] carries the projection *inputs*
//! (position / surface normal / tangent / size / super flag / fade alpha) the draw pass consumes.
//!
//! **Deliberately NOT built** (code map marks data/unrecovered — represented as inputs, not invented
//! bodies): the retail per-type table numbers (texture handle / size / lifetime — `confirm-live`), the
//! retail pool cap (`confirm-live`), the exact `DecalsUpdate` fade curve (data), and the projection
//! shader + `DAT_00dfc345` light-permutation selection (render-side, vtable-driven).

pub mod components;
pub mod pool;
pub mod table;

pub use components::{
    suppresses_3d_decals, suppresses_all_decals, Disable3DDecals, DisableDecals,
    DISABLE_3D_DECALS_HASH, DISABLE_DECALS_HASH,
};
pub use pool::{DecalInstance, DecalPool, DEFAULT_POOL_CAP, FADE_FRACTION};
pub use table::{
    DecalDef, DecalTable, DecalType, DECALTABLE_RESIDENT_ALLOC, DECALTABLE_RESIDENT_FLAG,
    DECALTABLE_TYPE_HASH,
};

use mercs2_core::glam::Vec3;
use mercs2_core::{Entity, World};

/// The host-owned decal mechanism: the loaded [`DecalTable`] + the runtime [`DecalPool`]. This is the
/// pair the engine holds and ticks — the game requests a decal at a hit point, the pool books it and
/// ages it, and the render seam draws the live instances. World-global state (like the AI crate's
/// `AiWorld`); the per-entity decal-suppression tags live on ECS components in the `World`.
#[derive(Clone, Debug)]
pub struct DecalWorld {
    /// The `decaltable` — decal-material definitions keyed by material hash.
    pub table: DecalTable,
    /// The bounded projected-decal instance pool.
    pub pool: DecalPool,
}

impl Default for DecalWorld {
    fn default() -> Self {
        DecalWorld { table: DecalTable::stock(), pool: DecalPool::default() }
    }
}

impl DecalWorld {
    /// A decal world with the stock (recovered-category, placeholder-param) table and the default pool.
    pub fn new() -> Self {
        DecalWorld::default()
    }

    /// A decal world with a specific loaded table and pool capacity (a loader/tuning supplies both).
    pub fn with(table: DecalTable, pool_cap: usize) -> Self {
        DecalWorld { table, pool: DecalPool::new(pool_cap) }
    }

    /// `CreateDecals` — spawn a projected decal of a recovered [`DecalType`] at a surface hit point.
    /// Looks the material up in the table, then books an instance in the pool (size/lifetime/super from
    /// the def). Returns the pool slot index, or `None` if the type isn't registered in the table.
    pub fn spawn(
        &mut self,
        ty: DecalType,
        position: Vec3,
        normal: Vec3,
        tangent: Vec3,
    ) -> Option<usize> {
        let def = *self.table.get_type(ty)?;
        Some(self.pool.spawn(&def, position, normal, tangent))
    }

    /// Spawn a decal by raw material row key (how the engine addresses a decal material internally).
    pub fn spawn_by_key(
        &mut self,
        key: u32,
        position: Vec3,
        normal: Vec3,
        tangent: Vec3,
    ) -> Option<usize> {
        let def = *self.table.get(key)?;
        Some(self.pool.spawn(&def, position, normal, tangent))
    }

    /// Spawn a decal projected onto a specific surface `entity`, honouring the ECS decal-toggle tags:
    /// if the entity carries an active [`DisableDecals`] (all decals) or [`Disable3DDecals`] (the
    /// projected/3D pass) tag, the request is dropped and `None` is returned — the recovered §3
    /// suppression. Otherwise identical to [`spawn`](Self::spawn).
    pub fn spawn_on_entity(
        &mut self,
        world: &World,
        entity: Entity,
        ty: DecalType,
        position: Vec3,
        normal: Vec3,
        tangent: Vec3,
    ) -> Option<usize> {
        if suppresses_all_decals(world, entity) || suppresses_3d_decals(world, entity) {
            return None;
        }
        self.spawn(ty, position, normal, tangent)
    }

    /// `DecalsUpdate` — age the pool by `dt` and free (`DecalUnlock`) expired instances. Returns how
    /// many were freed. Call once per fixed step; idle-cheap when the pool is empty.
    pub fn update(&mut self, dt: f32) -> usize {
        self.pool.update(dt)
    }

    /// Live projected decals — the render seam draws these (with per-instance `alpha()` fade).
    pub fn iter_live(&self) -> impl Iterator<Item = &DecalInstance> {
        self.pool.iter_live()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `decaltable` + pool roundtrip: spawn a bullet-hole decal, it becomes a live instance with
    /// the type's material key and the requested projection inputs.
    #[test]
    fn spawn_books_a_live_instance_from_the_table() {
        let mut dw = DecalWorld::new();
        let idx = dw.spawn(DecalType::BulletHole, Vec3::new(5.0, 0.0, 0.0), Vec3::Y, Vec3::X);
        assert_eq!(idx, Some(0));
        let inst = dw.iter_live().next().unwrap();
        assert_eq!(inst.def_key, DecalType::BulletHole.hash());
        assert_eq!(inst.position, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(dw.pool.live_count(), 1);
    }

    /// A type absent from the table cannot be spawned (loadable-data lookup, no invented row).
    #[test]
    fn unknown_type_does_not_spawn() {
        let mut dw = DecalWorld::with(DecalTable::new(), 16); // empty table
        assert_eq!(dw.spawn(DecalType::Blood, Vec3::ZERO, Vec3::Y, Vec3::X), None);
        assert_eq!(dw.pool.live_count(), 0);
    }

    /// End-to-end aging: a spawned decal ages out via `update` and frees its slot.
    #[test]
    fn update_ages_out_the_pool() {
        let mut table = DecalTable::new();
        table.insert(DecalDef { lifetime: 2.0, ..DecalDef::placeholder(DecalType::Scorch.hash()) });
        let mut dw = DecalWorld::with(table, 16);
        dw.spawn(DecalType::Scorch, Vec3::ZERO, Vec3::Y, Vec3::X);
        assert_eq!(dw.update(1.0), 0);
        assert_eq!(dw.pool.live_count(), 1);
        assert_eq!(dw.update(1.5), 1, "crossed 2.0s lifetime → freed");
        assert_eq!(dw.pool.live_count(), 0);
    }

    /// §3 suppression: an entity carrying `Disable3DDecals` drops projected-decal spawns against it.
    #[test]
    fn disable_tag_suppresses_spawn_on_entity() {
        let mut world = World::new();
        let plain = world.spawn((0u8,));
        let no_3d = world.spawn((Disable3DDecals::default(),));
        let no_all = world.spawn((DisableDecals::default(),));

        let mut dw = DecalWorld::new();
        assert!(dw
            .spawn_on_entity(&world, plain, DecalType::Blood, Vec3::ZERO, Vec3::Y, Vec3::X)
            .is_some());
        assert!(dw
            .spawn_on_entity(&world, no_3d, DecalType::Blood, Vec3::ZERO, Vec3::Y, Vec3::X)
            .is_none());
        assert!(dw
            .spawn_on_entity(&world, no_all, DecalType::Blood, Vec3::ZERO, Vec3::Y, Vec3::X)
            .is_none());
        assert_eq!(dw.pool.live_count(), 1, "only the un-suppressed spawn booked");
    }
}
