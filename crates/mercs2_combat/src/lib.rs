//! `mercs2_combat` — Weapons/combat: `wpn_*` gun stats, the projectile lifecycle, the homing/lock-on
//! FSM, and a damage/explosion applier.
//!
//! **Silo 10** (`docs/modernization/reimplementation_parallelization_plan.md` §3), **scoreboard row
//! 26**, **code map** `docs/reverse_engineer/weapons_combat_code_map.md`. Owned Lua namespaces:
//! `Weapon`, `Airstrike` (+ `Human.Inventory`/`Object` ammo cfuncs). Hard edges: hit-tests consume
//! `mercs2_core::PhysicsQuery` (silo 7); firing/damage post on the `mercs2_core` event bus (Keystone
//! B). Depends only on `mercs2_core` + `mercs2_formats` — no leaf→leaf edge (carve rule §4).
//!
//! # What is decoded-faithful vs. confirm-live (the exe is the oracle)
//! - **Faithful ports** (read first-hand in the code map): the weapon-system tick order
//!   (`FUN_0051cff0`, [`WeaponSystem::update`]); the **homing/lock-on FSM** — lock `FUN_0052dce0`
//!   ([`homing::homing_lock_system`]), launch `FUN_0052d120` ([`homing::launch_missile`]), and the
//!   **guided-flight integration** `FUN_0052e1f0` — cross-product steering + gravity + detonation timer
//!   ([`homing::homing_flight_system`]); the projectile lifecycle (velocity + gravity + swept raycast,
//!   [`projectile::projectile_system`]); the firing pipeline (`RateOfFire`/`iClipSize`/reload,
//!   [`firing::weapon_firing_system`]); the `wpn_*` reflection-blob container parse
//!   ([`stats::parse_weapon_block`]); the `Weapon`/`Airstrike` Lua-cfunc bodies ([`lua_surface`]).
//! - **Confirm-live stand-in** (the one documented wall, code map §5): the **per-hit damage/explosion
//!   solver** ([`damage`]) — the exe's `ApplyDamage*`/`UpdateExplosions`/`PhysicsCreateExplosion` math
//!   is string-only/SecuROM on both builds and is **unread**. The applier here is a faithful modern
//!   stand-in from the *authored* dropoff/radius fields, with every stand-in choice marked
//!   `// CONFIRM-LIVE:`; its **outputs** (`DamageMsg`/`DestroyMsg` into the destruction FSM) are the
//!   exe's known outputs. Also confirm-live: the exact `wpn_*` byte offset → named-stat binding
//!   (`stats`), so per-weapon stats fall back to the recovered exe schema defaults. See `DEFERRED.md`.

use hecs::World;

use mercs2_core::event::EventBus;
use mercs2_core::PhysicsQuery;

pub mod components;
pub mod damage;
pub mod events;
pub mod firing;
pub mod homing;
pub mod lua_surface;
pub mod projectile;
pub mod stats;

pub use components::{
    Health, HomingState, Inventory, RuntimeExplosion, RuntimeHomingWeapon, RuntimeProjectile,
    RuntimeWeapon,
};
pub use damage::{DamageKey, ExplosionSize};
pub use stats::{ExplosiveStats, FireType, HomingStats, WeaponDefBlob, WeaponStats, WeaponSubObject};

/// The weapon-system per-frame driver — the reimpl of `FUN_0051cff0` (code map §2), an entry in the
/// layer-4 fixed-order gameplay-system list beside the vehicle pump and population update. Sequences
/// the combat passes in the exe's order: **homing lock FSM → firing → guided flight → projectiles →
/// explosions**. Runs at the fixed sim rate; gate it on world-present/not-paused at the call site (the
/// exe's `PTR_DAT_01175cdc[0x62]` gate, §1) — a paused world simply doesn't call this.
///
/// This is a zero-state sequencer (all state lives in the ECS components), so it's a unit struct; call
/// [`WeaponSystem::update`] once per fixed tick.
pub struct WeaponSystem;

impl WeaponSystem {
    /// Advance the whole weapon system one fixed step `dt`. `physics` is the [`PhysicsQuery`] seam for
    /// hitscan/projectile/explosion line tests (pass `None` before the physics silo lands — hitscans
    /// then simply miss, projectiles fly to lifetime, explosions still damage by ECS overlap).
    pub fn update(world: &mut World, dt: f32, bus: &mut EventBus, physics: Option<&dyn PhysicsQuery>) {
        // §4 homing sub-update (lock FSM) runs first (exe: FUN_0052e730 before the pooled passes).
        homing::homing_lock_system(world, dt, bus);
        // Pooled RuntimeWeapon leaf: trigger → shot / reload / rate-of-fire (may launch homing missiles).
        firing::weapon_firing_system(world, dt, bus, physics);
        // Guided-missile flight integration (FUN_0052e1f0).
        homing::homing_flight_system(world, dt, bus, physics);
        // Generic projectile flight (velocity + gravity + swept raycast).
        projectile::projectile_system(world, dt, bus, physics);
        // Live blasts apply their radial damage and age out.
        projectile::explosion_system(world, dt, bus, physics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use mercs2_core::physics_query::{ClosestPoint, RayHit};
    use mercs2_core::Transform;

    struct NoPhysics;
    impl PhysicsQuery for NoPhysics {
        fn raycast(&self, _o: Vec3, _d: Vec3, _m: f32) -> Option<RayHit> {
            None
        }
        fn closest_point(&self, _p: Vec3, _m: f32) -> Option<ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            pos + delta
        }
    }

    /// End-to-end integration: a locked rocket launcher fires through the full `WeaponSystem` tick, and
    /// the guided missile reaches and damages the target — exercising lock → launch → flight →
    /// detonation → explosion damage in one driver. Asserts the target takes heavy blast damage (the
    /// exact lethality depends on the confirm-live falloff curve, so we assert a substantial hit, not a
    /// one-shot kill).
    #[test]
    fn full_system_lock_fire_and_damage() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        let target = world.spawn((
            Transform::from_translation(Vec3::new(5.0, 0.0, 40.0)),
            Health::new(100.0),
        ));
        // Watch for the launch + damage events that prove the full chain fired.
        let launched = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        let l = launched.clone();
        bus.on(events::HOMING_LAUNCHED, move |_| *l.borrow_mut() += 1);
        let damaged = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        let d = damaged.clone();
        bus.on(events::DAMAGE_MSG, move |_| *d.borrow_mut() += 1);

        let mut stats = WeaponStats::rocket_launcher();
        stats.homing.as_mut().unwrap().lock_on_time = 0.05;
        stats.homing.as_mut().unwrap().detonation_distance = 3.0;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.aim_dir = Vec3::new(5.0, 0.0, 40.0).normalize();
        w.trigger_down = true;
        world.spawn((w,));

        let phys = NoPhysics;
        let start_hp = 100.0;
        let mut ticks = 0;
        while ticks < 600 && world.get::<&Health>(target).map(|h| h.cur == start_hp).unwrap_or(false) {
            WeaponSystem::update(&mut world, 1.0 / 60.0, &mut bus, Some(&phys));
            ticks += 1;
        }
        assert_eq!(*launched.borrow(), 1, "exactly one missile was launched");
        assert!(*damaged.borrow() >= 1, "a DamageMsg was posted on detonation");
        let hp = world.get::<&Health>(target).unwrap().cur;
        assert!(
            hp < start_hp * 0.5,
            "rocket locked, launched, tracked, and dealt heavy blast damage (hp {hp} < 50)"
        );
    }
}
