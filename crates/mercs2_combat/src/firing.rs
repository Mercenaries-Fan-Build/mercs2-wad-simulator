//! Firing pipeline — the `RuntimeWeapon` per-tick leaf (code map §2/§9 step 2).
//!
//! A trigger-pull becomes a shot here: fire-rate cooldown, magazine/reload gating, and hitscan-vs-
//! projectile dispatch. This is the reimpl of the pooled RuntimeWeapon update inside `FUN_0051cff0`
//! (the exact SecuROM-virtualised leaf math is confirm-live, code map §8.1; the *structure* —
//! `RateOfFire` delay → `iBulletsPerShot` shots → magazine/`iClipSize`/reload — is faithful).
//!
//! Homing launchers defer the actual missile spawn to [`crate::homing`] (only when the lock FSM reports
//! `Locked`), mirroring the exe's split of firing (`FUN_0051cff0`) from launch (`FUN_0052d120`).

use glam::Vec3;
use hecs::{Entity, World};

use mercs2_core::event::{Event, EventArg, EventBus};
use mercs2_core::PhysicsQuery;

use crate::components::{HomingState, RuntimeProjectile, RuntimeWeapon};
use crate::events::WEAPON_EVENT;
use crate::stats::FireType;

/// Maximum hitscan range (m). `// CONFIRM-LIVE:` the exe derives this from the weapon/projectile range
/// fields; this is a conventional cap until pinned.
pub const HITSCAN_MAX_RANGE: f32 = 500.0;

/// Reload duration (s). `// CONFIRM-LIVE:` the exe's reload time is animation/`wpn_*`-driven; a fixed
/// stand-in until pinned. Not a fidelity blocker (`DEFERRED.md`).
pub const RELOAD_TIME: f32 = 2.0;

/// A deferred projectile spawn produced by the firing pass (applied after the weapon query is dropped).
struct SpawnProjectile(RuntimeProjectile);

/// Advance every [`RuntimeWeapon`] one fixed step: cool down the fire timer, run reloads, and — when
/// the trigger is down, the cooldown has elapsed, and the magazine is non-empty — fire. Hitscan shots
/// raycast through `physics` and damage the struck entity; projectile weapons (`muzzle_velocity > 0`)
/// spawn a [`RuntimeProjectile`]. Homing launchers only launch when their lock FSM is `Locked` (the
/// missile spawn is [`crate::homing::launch_missile`]).
///
/// Emits `WeaponEvent` per shot. Returns the number of shots fired this tick (for tests/telemetry).
pub fn weapon_firing_system(
    world: &mut World,
    dt: f32,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
) -> u32 {
    let mut spawns: Vec<SpawnProjectile> = Vec::new();
    let mut hitscans: Vec<(Entity, Vec3, Vec3, f32, crate::damage::DamageKey)> = Vec::new();
    let mut launches: Vec<Entity> = Vec::new(); // homing weapons that fired this tick
    let mut shots_fired = 0u32;

    for (we, w) in world.query::<&mut RuntimeWeapon>().iter() {
        // --- reload in progress ---
        if w.reloading {
            w.reload_timer -= dt;
            if w.reload_timer <= 0.0 {
                finish_reload(w);
            }
            continue;
        }
        // --- fire-rate cooldown ---
        if w.fire_cooldown > 0.0 {
            w.fire_cooldown -= dt;
        }
        // --- trigger release resets the semi/burst latch ---
        if !w.trigger_down {
            w.trigger_latched = false;
            // Auto-reload an empty magazine when the trigger is released (faithful convenience).
            if w.clip_ammo == 0 && w.can_reload() {
                begin_reload(w);
            }
            continue;
        }
        // --- empty magazine: auto-reload, no shot ---
        if w.clip_ammo <= 0 {
            if w.can_reload() {
                begin_reload(w);
            }
            continue;
        }
        // --- fire-mode gating ---
        let ready = w.fire_cooldown <= 0.0;
        let mode_ok = match w.stats.fire_type {
            FireType::Automatic => true,
            FireType::SemiAutomatic | FireType::Burst => !w.trigger_latched,
        };
        if !(ready && mode_ok) {
            continue;
        }

        // --- FIRE ---
        // Homing launcher: only launch when the lock FSM has acquired.
        if w.stats.homing.is_some() {
            if matches!(w.lock, HomingState::Locked { .. }) {
                launches.push(we);
                if !w.infinite_ammo {
                    w.clip_ammo -= 1;
                }
                w.fire_cooldown = w.stats.fire_interval();
                w.trigger_latched = true;
                shots_fired += 1;
                emit_weapon_event(bus, w.owner);
            }
            continue;
        }

        // Conventional gun: fire `iBulletsPerShot` shots.
        let pellets = w.stats.bullets_per_shot.max(1);
        for p in 0..pellets {
            let dir = spread_dir(w.aim_dir, w.stats.scatter_min, p, pellets);
            if w.stats.muzzle_velocity > 0.0 {
                // Projectile weapon.
                spawns.push(SpawnProjectile(RuntimeProjectile {
                    owner: w.owner,
                    pos: w.muzzle,
                    vel: dir * w.stats.muzzle_velocity,
                    gravity: w.stats.projectile_gravity,
                    life: w.stats.projectile_lifetime,
                    damage: w.stats.damage,
                    damage_key: w.stats.damage_key,
                    explosive: w.stats.explosive,
                }));
            } else {
                // Hitscan.
                hitscans.push((w.owner, w.muzzle, dir, w.stats.damage, w.stats.damage_key));
            }
        }
        if !w.infinite_ammo {
            w.clip_ammo -= 1;
        }
        w.fire_cooldown = w.stats.fire_interval();
        w.trigger_latched = true;
        shots_fired += 1;
        emit_weapon_event(bus, w.owner);
    }

    // --- apply deferred effects ---
    for SpawnProjectile(p) in spawns {
        world.spawn((p,));
    }
    for (owner, origin, dir, damage, key) in hitscans {
        if let Some(pq) = physics {
            if let Some(hit) = pq.raycast(origin, dir, HITSCAN_MAX_RANGE) {
                if let Some(victim) = hit.entity {
                    crate::damage::apply_hit(world, bus, victim, Some(owner), damage, key);
                }
            }
        }
    }
    // Homing launches (spawns the guided missile via the homing module).
    for we in launches {
        crate::homing::launch_missile(world, bus, we);
    }

    shots_fired
}

/// Begin a reload: latch the state and seed the timer. The rounds move on [`finish_reload`].
pub fn begin_reload(w: &mut RuntimeWeapon) {
    if w.can_reload() {
        w.reloading = true;
        w.reload_timer = RELOAD_TIME;
    }
}

/// Complete a reload: refill the magazine from reserve. `iRoundsPerReload == -1` ⇒ refill the whole
/// clip; otherwise add up to that many rounds (per-round reloads, e.g. shotguns).
fn finish_reload(w: &mut RuntimeWeapon) {
    let need = if w.stats.rounds_per_reload < 0 {
        w.stats.clip_size - w.clip_ammo
    } else {
        w.stats.rounds_per_reload.min(w.stats.clip_size - w.clip_ammo)
    };
    let moved = need.clamp(0, w.reserve_ammo);
    w.clip_ammo += moved;
    w.reserve_ammo -= moved;
    w.reloading = false;
    w.reload_timer = 0.0;
    // If per-round and still not full with rounds left, another reload can be issued next release.
}

/// Deterministic pellet spread: a symmetric fan of half-angle `scatter_deg` across `count` pellets.
/// For `count == 1` this is the aim direction unchanged (no jitter — the skill-weighted RNG spread is a
/// refinement, `DEFERRED.md`).
fn spread_dir(aim: Vec3, scatter_deg: f32, index: i32, count: i32) -> Vec3 {
    if count <= 1 || scatter_deg <= 0.0 {
        return aim.normalize_or_zero();
    }
    // Fan the pellets in the plane spanned by aim × up.
    let up = if aim.normalize_or_zero().dot(Vec3::Y).abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let side = aim.cross(up).normalize_or_zero();
    let frac = (index as f32) / ((count - 1) as f32) - 0.5; // -0.5..0.5
    let ang = frac * 2.0 * scatter_deg.to_radians();
    (aim.normalize_or_zero() + side * ang.tan()).normalize_or_zero()
}

fn emit_weapon_event(bus: &mut EventBus, owner: Entity) {
    let mut ev = Event::new(WEAPON_EVENT);
    let _ = ev.try_push(EventArg::Handle(owner.to_bits().get() as u32));
    bus.emit(ev);
}

/// Request that `weapon` (a `RuntimeWeapon` entity) begin reloading now (the engine-side of a Lua/AI
/// reload command). No-op if it can't reload.
pub fn request_reload(world: &mut World, weapon: Entity) {
    if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(weapon) {
        begin_reload(&mut w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Health;
    use crate::stats::WeaponStats;
    use mercs2_core::physics_query::RayHit;

    /// A physics stub that reports a single entity straight ahead at a fixed distance.
    struct HitStub {
        victim: Entity,
        dist: f32,
    }
    impl PhysicsQuery for HitStub {
        fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
            if self.dist <= max {
                Some(RayHit {
                    point: origin + dir * self.dist,
                    normal: -dir,
                    distance: self.dist,
                    entity: Some(self.victim),
                })
            } else {
                None
            }
        }
        fn closest_point(&self, _p: Vec3, _m: f32) -> Option<mercs2_core::physics_query::ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            pos + delta
        }
    }

    #[test]
    fn rate_of_fire_gates_shots() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        let mut stats = WeaponStats::default(); // 120 rpm → 0.5s interval, hitscan
        stats.clip_size = 30;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.trigger_down = true;
        w.aim_dir = Vec3::Z;
        let we = world.spawn((w,));

        // First tick fires once (cooldown was 0); a small dt shouldn't allow a second within 0.5s.
        let n = weapon_firing_system(&mut world, 1.0 / 60.0, &mut bus, None);
        assert_eq!(n, 1);
        assert_eq!(world.get::<&RuntimeWeapon>(we).unwrap().clip_ammo, 29);
        // Immediately again: cooldown not elapsed → no shot.
        let n2 = weapon_firing_system(&mut world, 1.0 / 60.0, &mut bus, None);
        assert_eq!(n2, 0);
        // After 0.5s total the cooldown elapses → fires again.
        let n3 = weapon_firing_system(&mut world, 0.5, &mut bus, None);
        assert_eq!(n3, 1);
        assert_eq!(world.get::<&RuntimeWeapon>(we).unwrap().clip_ammo, 28);
    }

    #[test]
    fn clip_empties_then_auto_reloads() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        let mut stats = WeaponStats::default();
        stats.clip_size = 2;
        stats.max_ammo_reserve = 10;
        stats.rate_of_fire = 6000.0; // tiny interval so each tick can fire
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.trigger_down = true;
        let we = world.spawn((w,));

        // Fire until empty.
        weapon_firing_system(&mut world, 1.0, &mut bus, None);
        weapon_firing_system(&mut world, 1.0, &mut bus, None);
        assert_eq!(world.get::<&RuntimeWeapon>(we).unwrap().clip_ammo, 0);
        // Next tick with empty clip begins a reload.
        weapon_firing_system(&mut world, 1.0, &mut bus, None);
        assert!(world.get::<&RuntimeWeapon>(we).unwrap().reloading);
        // Advance past the reload; magazine refills from reserve.
        weapon_firing_system(&mut world, RELOAD_TIME + 0.01, &mut bus, None);
        let w2 = world.get::<&RuntimeWeapon>(we).unwrap();
        assert!(!w2.reloading);
        assert_eq!(w2.clip_ammo, 2);
        assert_eq!(w2.reserve_ammo, 8);
    }

    #[test]
    fn hitscan_damages_target_ahead() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let victim = world.spawn((Health::new(100.0),));
        let shooter = world.spawn(());
        let mut stats = WeaponStats::default();
        stats.damage = 25.0;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.trigger_down = true;
        w.muzzle = Vec3::ZERO;
        w.aim_dir = Vec3::Z;
        world.spawn((w,));

        let phys = HitStub { victim, dist: 10.0 };
        weapon_firing_system(&mut world, 1.0 / 60.0, &mut bus, Some(&phys));
        assert_eq!(world.get::<&Health>(victim).unwrap().cur, 75.0);
    }
}
