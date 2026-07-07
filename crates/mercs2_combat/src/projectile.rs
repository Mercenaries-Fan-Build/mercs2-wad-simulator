//! Projectile lifecycle — the `RuntimeProjectile` per-tick integration (code map §3 / §9 step 3).
//!
//! Faithful to the Xbox sub-phase order `Update::Gravity` → `Update::Movement` → `Update::Raycast`
//! (code map §3): each tick applies gravity, moves by velocity, then raycasts the **swept segment**
//! for impact. On a direct hit the projectile applies its damage (and, if explosive, detonates a
//! [`crate::components::RuntimeExplosion`]); on lifetime expiry it despawns (an explosive projectile
//! detonates in place — a grenade/warhead fuze). The generic non-homing leaf's exact impact predicate
//! is confirm-live (SecuROM ring fetch, §3); the velocity+gravity+raycast structure is faithful.

use glam::Vec3;
use hecs::{Entity, World};

use mercs2_core::event::EventBus;
use mercs2_core::PhysicsQuery;

use crate::components::{Health, RuntimeExplosion, RuntimeProjectile};
use crate::impact::Impact;

/// One resolved projectile outcome, applied after the (immutable) integration query is dropped.
enum Outcome {
    /// Direct hit on `victim` (if any) at `point`; despawn the projectile. `normal` is the physics
    /// contact normal (may be degenerate → the impact channel falls back to reverse-travel); `dir` is
    /// the projectile's unit travel direction at impact (the reverse-travel fallback source).
    Hit {
        proj: Entity,
        victim: Option<Entity>,
        point: Vec3,
        normal: Vec3,
        dir: Vec3,
    },
    /// Lifetime expired at `point`; despawn (detonate in place if explosive).
    Expired { proj: Entity, point: Vec3 },
}

/// Advance every [`RuntimeProjectile`] one fixed step. Returns the number despawned this tick
/// (impact + expiry) for tests/telemetry. This is the legacy entry point that **discards** the impact
/// FX channel; call [`projectile_system_impacts`] to capture it.
pub fn projectile_system(
    world: &mut World,
    dt: f32,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
) -> u32 {
    let mut sink = Vec::new();
    projectile_system_impacts(world, dt, bus, physics, &mut sink)
}

/// Advance every [`RuntimeProjectile`] one fixed step, appending a bullet/blood [`Impact`] for each
/// **non-explosive** direct hit to `impacts` (explosive rounds emit their impact through the
/// [`RuntimeExplosion`] they spawn, handled by [`explosion_system_impacts`]). Returns the number
/// despawned this tick.
pub fn projectile_system_impacts(
    world: &mut World,
    dt: f32,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
    impacts: &mut Vec<Impact>,
) -> u32 {
    let mut outcomes: Vec<Outcome> = Vec::new();

    for (pe, p) in world.query::<&mut RuntimeProjectile>().iter() {
        // Update::Gravity — +Y up, so gravity subtracts from vy.
        p.vel.y -= p.gravity * dt;
        // Update::Movement — integrate.
        let from = p.pos;
        let to = p.pos + p.vel * dt;
        // Update::Raycast — sweep the segment for an impact.
        let seg = to - from;
        let len = seg.length();
        let mut impacted = false;
        if len > 1e-6 {
            if let Some(pq) = physics {
                let dir = seg / len;
                if let Some(hit) = pq.raycast(from, dir, len) {
                    outcomes.push(Outcome::Hit {
                        proj: pe,
                        victim: hit.entity,
                        point: hit.point,
                        normal: hit.normal,
                        dir,
                    });
                    impacted = true;
                }
            }
        }
        if impacted {
            continue;
        }
        p.pos = to;
        // Lifetime.
        p.life -= dt;
        if p.life <= 0.0 {
            outcomes.push(Outcome::Expired { proj: pe, point: p.pos });
        }
    }

    let mut despawned = 0u32;
    for outcome in outcomes {
        let (proj, victim, point, hit_facing) = match outcome {
            Outcome::Hit { proj, victim, point, normal, dir } => (proj, victim, point, Some((normal, dir))),
            Outcome::Expired { proj, point } => (proj, None, point, None),
        };
        // Read the projectile's payload before despawning it.
        let payload = world.get::<&RuntimeProjectile>(proj).ok().map(|p| {
            (
                p.owner,
                p.damage,
                p.damage_key,
                p.explosive,
            )
        });
        let Some((owner, damage, key, explosive)) = payload else {
            continue;
        };
        // Direct hit damage.
        if let Some(v) = victim {
            crate::damage::apply_hit(world, bus, v, Some(owner), damage, key);
        }
        // Explosive round: spawn a blast at the impact/expiry point (its Explosion impact is emitted by
        // the explosion pass). A non-explosive direct hit emits a bullet/blood impact here.
        if let Some(exp) = explosive {
            world.spawn((RuntimeExplosion {
                owner: Some(owner),
                pos: point,
                stats: exp,
                damage_key: key,
                applied: false,
                life: 0.25, // brief linger; the damage applies on its first tick
            },));
        } else if let Some((normal, dir)) = hit_facing {
            // A character hit sprays blood; a world/prop hit leaves a bullet hole.
            let is_character = victim.map(|v| world.get::<&Health>(v).is_ok()).unwrap_or(false);
            impacts.push(Impact::from_hit(point, normal, dir, is_character));
        }
        let _ = world.despawn(proj);
        despawned += 1;
    }
    despawned
}

/// Advance every [`RuntimeExplosion`]: apply its radial damage once (on its first tick) via the
/// confirm-live applier, then age it out. Returns the number of blasts that applied damage this tick.
/// Legacy entry point that **discards** the impact FX channel; call [`explosion_system_impacts`] to
/// capture it.
pub fn explosion_system(
    world: &mut World,
    dt: f32,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
) -> u32 {
    let mut sink = Vec::new();
    explosion_system_impacts(world, dt, bus, physics, &mut sink)
}

/// Advance every [`RuntimeExplosion`], emitting one [`ImpactKind::Explosion`](crate::ImpactKind)
/// [`Impact`] per detonation (at the blast centre, facing world-up) into `impacts`. This is the single
/// choke point for explosion FX — grenade/mine blasts, explosive-projectile detonations, and homing
/// warheads all spawn a [`RuntimeExplosion`] that lands here. Returns the number of blasts that applied
/// damage this tick.
pub fn explosion_system_impacts(
    world: &mut World,
    dt: f32,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
    impacts: &mut Vec<Impact>,
) -> u32 {
    // Gather blasts to detonate this tick (those not yet applied), plus age/despawn bookkeeping.
    let mut to_detonate: Vec<(Entity, Option<Entity>, Vec3, crate::stats::ExplosiveStats, crate::damage::DamageKey)> =
        Vec::new();
    let mut expired: Vec<Entity> = Vec::new();
    for (ee, ex) in world.query::<&mut RuntimeExplosion>().iter() {
        if !ex.applied {
            to_detonate.push((ee, ex.owner, ex.pos, ex.stats, ex.damage_key));
            ex.applied = true;
        }
        ex.life -= dt;
        if ex.life <= 0.0 {
            expired.push(ee);
        }
    }
    let mut applied_count = 0u32;
    for (_ee, owner, pos, stats, key) in to_detonate {
        // Emit the explosion-mark/scorch FX at the blast centre (once, on the detonation tick),
        // regardless of whether the blast happened to catch any bodies.
        impacts.push(Impact::explosion(pos));
        let hits = crate::damage::detonate_explosion(world, bus, physics, owner, pos, &stats, key);
        if !hits.is_empty() {
            applied_count += 1;
        }
    }
    for ee in expired {
        let _ = world.despawn(ee);
    }
    applied_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Health;
    use crate::damage::DamageKey;
    use crate::stats::ExplosiveStats;
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

    #[test]
    fn projectile_falls_under_gravity_and_expires() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        let pe = world.spawn((RuntimeProjectile {
            owner,
            pos: Vec3::ZERO,
            vel: Vec3::new(0.0, 0.0, 20.0),
            gravity: 10.0,
            life: 0.05,
            damage: 5.0,
            damage_key: DamageKey::BulletLarge,
            explosive: None,
        },));
        // One 1/60 tick: gravity pulls vy negative, position advances +Z.
        projectile_system(&mut world, 1.0 / 60.0, &mut bus, Some(&NoPhysics));
        let p = world.get::<&RuntimeProjectile>(pe).unwrap();
        assert!(p.vel.y < 0.0, "gravity applied");
        assert!(p.pos.z > 0.0, "moved forward");
        drop(p);
        // Run past its short life → despawn.
        let n = projectile_system(&mut world, 0.05, &mut bus, Some(&NoPhysics));
        assert_eq!(n, 1);
        assert!(world.get::<&RuntimeProjectile>(pe).is_err());
    }

    #[test]
    fn explosive_projectile_detonates_on_expiry() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        // A victim sitting where the projectile expires.
        let victim = world.spawn((Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)), Health::new(100.0)));
        world.spawn((RuntimeProjectile {
            owner,
            pos: Vec3::new(0.0, 0.0, 1.0),
            vel: Vec3::ZERO,
            gravity: 0.0,
            life: 0.01,
            damage: 0.0,
            damage_key: DamageKey::Explosion,
            explosive: Some(ExplosiveStats { radius: 5.0, max_force: 10.0, damage: 60.0, min_force_falloff: 0.0 }),
        },));
        // Expire the projectile → spawns a RuntimeExplosion.
        projectile_system(&mut world, 0.02, &mut bus, Some(&NoPhysics));
        // Run the explosion system → damages the nearby victim (centre → full damage).
        let applied = explosion_system(&mut world, 1.0 / 60.0, &mut bus, Some(&NoPhysics));
        assert_eq!(applied, 1);
        assert!(world.get::<&Health>(victim).unwrap().cur < 100.0);
    }

    /// A physics stub that reports a hit at a fixed distance ahead with an optional victim.
    struct HitAhead {
        victim: Option<Entity>,
        dist: f32,
    }
    impl PhysicsQuery for HitAhead {
        fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
            if self.dist <= max {
                Some(RayHit {
                    point: origin + dir * self.dist,
                    normal: -dir,
                    distance: self.dist,
                    entity: self.victim,
                })
            } else {
                None
            }
        }
        fn closest_point(&self, _p: Vec3, _m: f32) -> Option<ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            pos + delta
        }
    }

    #[test]
    fn non_explosive_projectile_hit_emits_bullet_or_blood() {
        use crate::impact::ImpactKind;
        // World hit (no victim) → Bullet.
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        world.spawn((RuntimeProjectile {
            owner,
            pos: Vec3::ZERO,
            vel: Vec3::new(0.0, 0.0, 30.0),
            gravity: 0.0,
            life: 5.0,
            damage: 10.0,
            damage_key: DamageKey::BulletLarge,
            explosive: None,
        },));
        let mut impacts = Vec::new();
        projectile_system_impacts(&mut world, 1.0 / 60.0, &mut bus, Some(&HitAhead { victim: None, dist: 0.1 }), &mut impacts);
        assert_eq!(impacts.len(), 1);
        assert_eq!(impacts[0].kind, ImpactKind::Bullet);

        // Character hit (Health victim) → Blood.
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        let victim = world.spawn((Health::new(100.0),));
        world.spawn((RuntimeProjectile {
            owner,
            pos: Vec3::ZERO,
            vel: Vec3::new(0.0, 0.0, 30.0),
            gravity: 0.0,
            life: 5.0,
            damage: 10.0,
            damage_key: DamageKey::BulletLarge,
            explosive: None,
        },));
        let mut impacts = Vec::new();
        projectile_system_impacts(&mut world, 1.0 / 60.0, &mut bus, Some(&HitAhead { victim: Some(victim), dist: 0.1 }), &mut impacts);
        assert_eq!(impacts.len(), 1);
        assert_eq!(impacts[0].kind, ImpactKind::Blood);
    }

    #[test]
    fn explosive_projectile_emits_explosion_not_bullet() {
        use crate::impact::ImpactKind;
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        world.spawn((RuntimeProjectile {
            owner,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            gravity: 0.0,
            life: 0.01,
            damage: 0.0,
            damage_key: DamageKey::Explosion,
            explosive: Some(ExplosiveStats { radius: 5.0, max_force: 10.0, damage: 60.0, min_force_falloff: 0.0 }),
        },));
        let mut impacts = Vec::new();
        // Expiry spawns a RuntimeExplosion; the projectile pass itself emits NO impact for explosives.
        projectile_system_impacts(&mut world, 0.02, &mut bus, Some(&NoPhysics), &mut impacts);
        assert!(impacts.is_empty(), "explosive round defers its impact to the explosion pass");
        // The explosion pass emits exactly one Explosion impact (facing world-up).
        explosion_system_impacts(&mut world, 1.0 / 60.0, &mut bus, Some(&NoPhysics), &mut impacts);
        assert_eq!(impacts.len(), 1);
        assert_eq!(impacts[0].kind, ImpactKind::Explosion);
        assert_eq!(impacts[0].normal, Vec3::Y);
    }
}
