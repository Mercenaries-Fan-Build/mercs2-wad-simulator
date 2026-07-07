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
pub mod impact;
pub mod lua_surface;
pub mod projectile;
pub mod stats;

pub use components::{
    Health, HomingState, Inventory, RuntimeExplosion, RuntimeHomingWeapon, RuntimeProjectile,
    RuntimeWeapon,
};
pub use damage::{DamageKey, ExplosionSize};
pub use impact::{Impact, ImpactKind};
pub use stats::{ExplosiveStats, FireType, HomingStats, WeaponDefBlob, WeaponStats, WeaponSubObject};

/// The weapon-system per-frame driver — the reimpl of `FUN_0051cff0` (code map §2), an entry in the
/// layer-4 fixed-order gameplay-system list beside the vehicle pump and population update. Sequences
/// the combat passes in the exe's order: **homing lock FSM → firing → guided flight → projectiles →
/// explosions**. Runs at the fixed sim rate; gate it on world-present/not-paused at the call site (the
/// exe's `PTR_DAT_01175cdc[0x62]` gate, §1) — a paused world simply doesn't call this.
///
/// Almost all state lives in the ECS components; the one piece of frame-local system state is the
/// **impact-output buffer** — the [`Impact`] records produced by resolved hits this frame, drained by
/// the game layer to feed the decal/particle consumers. Construct with [`WeaponSystem::default`], call
/// [`WeaponSystem::tick`] once per fixed step, then [`WeaponSystem::take_impacts`] to hand the frame's
/// impacts to the consumers.
#[derive(Default)]
pub struct WeaponSystem {
    /// Impacts accumulated since the last drain (drain-then-clear, like the event bus's post-then-drain
    /// cadence). Filled by [`WeaponSystem::tick`]; emptied by [`WeaponSystem::take_impacts`].
    impacts: Vec<impact::Impact>,
}

impl WeaponSystem {
    /// Advance the whole weapon system one fixed step `dt`, **discarding impact FX records** — the
    /// legacy stateless entry point (kept for callers that don't consume the impact channel). `physics`
    /// is the [`PhysicsQuery`] seam for hitscan/projectile/explosion line tests (pass `None` before the
    /// physics silo lands — hitscans then simply miss, projectiles fly to lifetime, explosions still
    /// damage by ECS overlap).
    ///
    /// To capture the impact channel, construct a [`WeaponSystem`] and call [`WeaponSystem::tick`].
    pub fn update(world: &mut World, dt: f32, bus: &mut EventBus, physics: Option<&dyn PhysicsQuery>) {
        let mut sink = Vec::new();
        Self::run(world, dt, bus, physics, &mut sink);
    }

    /// Advance the whole weapon system one fixed step `dt`, **accumulating** the [`Impact`] records for
    /// every resolved hit (hitscan strikes, projectile direct hits, explosion detonations) into
    /// `self.impacts`. Drain them with [`WeaponSystem::take_impacts`] after the tick.
    pub fn tick(
        &mut self,
        world: &mut World,
        dt: f32,
        bus: &mut EventBus,
        physics: Option<&dyn PhysicsQuery>,
    ) {
        Self::run(world, dt, bus, physics, &mut self.impacts);
    }

    /// Drain and return this frame's accumulated impacts (drain-then-clear); the buffer is left empty
    /// for the next tick.
    pub fn take_impacts(&mut self) -> Vec<impact::Impact> {
        std::mem::take(&mut self.impacts)
    }

    /// Borrow the currently-accumulated impacts without draining (telemetry/inspection).
    pub fn impacts(&self) -> &[impact::Impact] {
        &self.impacts
    }

    /// The shared sequencer body: runs the combat passes in the exe's order and appends every resolved
    /// hit's [`Impact`] to `impacts`.
    fn run(
        world: &mut World,
        dt: f32,
        bus: &mut EventBus,
        physics: Option<&dyn PhysicsQuery>,
        impacts: &mut Vec<impact::Impact>,
    ) {
        // §4 homing sub-update (lock FSM) runs first (exe: FUN_0052e730 before the pooled passes).
        homing::homing_lock_system(world, dt, bus);
        // Pooled RuntimeWeapon leaf: trigger → shot / reload / rate-of-fire (may launch homing missiles).
        firing::weapon_firing_system_impacts(world, dt, bus, physics, impacts);
        // Guided-missile flight integration (FUN_0052e1f0) — detonations spawn RuntimeExplosions whose
        // impacts are emitted by the explosion pass below.
        homing::homing_flight_system(world, dt, bus, physics);
        // Generic projectile flight (velocity + gravity + swept raycast).
        projectile::projectile_system_impacts(world, dt, bus, physics, impacts);
        // Live blasts apply their radial damage, emit an explosion impact, and age out.
        projectile::explosion_system_impacts(world, dt, bus, physics, impacts);
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

    /// The stateful `tick` path captures the impact channel: a homing missile detonation flows through
    /// the explosion pass and lands one `Explosion` impact, drainable via `take_impacts`.
    #[test]
    fn tick_collects_and_drains_explosion_impact() {
        use crate::impact::ImpactKind;
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        let _target = world.spawn((
            Transform::from_translation(Vec3::new(5.0, 0.0, 40.0)),
            Health::new(100.0),
        ));
        let mut stats = WeaponStats::rocket_launcher();
        stats.homing.as_mut().unwrap().lock_on_time = 0.05;
        stats.homing.as_mut().unwrap().detonation_distance = 3.0;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.aim_dir = Vec3::new(5.0, 0.0, 40.0).normalize();
        w.trigger_down = true;
        world.spawn((w,));

        let phys = NoPhysics;
        let mut sys = WeaponSystem::default();
        let mut saw_explosion = false;
        for _ in 0..600 {
            sys.tick(&mut world, 1.0 / 60.0, &mut bus, Some(&phys));
            let drained = sys.take_impacts();
            // take_impacts drains: the buffer is empty right after.
            assert!(sys.impacts().is_empty());
            if drained.iter().any(|i| i.kind == ImpactKind::Explosion) {
                saw_explosion = true;
                break;
            }
        }
        assert!(saw_explosion, "the missile detonation produced a drainable Explosion impact");
    }
}
