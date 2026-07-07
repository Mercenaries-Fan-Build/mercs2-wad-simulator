//! Damage / explosion applier — **the confirm-live stand-in** (code map §5).
//!
//! # Honesty boundary (read this)
//! The exe's exact per-hit ballistic/explosion solver is the one documented **wall**:
//! `ApplyDamageToPrimaryHealth` / `ApplyDamageToNodeHealth` / `UpdateExplosions` /
//! `PhysicsCreateExplosion` / `ApplyExplosionToBodies` are string-only on both builds and route through
//! SecuROM thunks — no readable code literal (code map §5.3B / §8.5). `DamagePerson FUN_005e0720` is
//! the **faction mood-report bridge, NOT the applier** (§5.2). So the exact damage curve and mitigation
//! math are **unread**, and this module does **not** claim them.
//!
//! What this module *is*: a faithful, clearly-marked **modern stand-in** built from the *authored*
//! inputs that ARE in the clear — the `Explosive`/`ProjectilePhysics` dropoff/radius/damage fields
//! (ecs-01) — using conventional radius falloff and the recovered `DamageKey` taxonomy. Its **outputs**
//! are the ones the exe's output is known to produce: it lowers a target's health and posts `DamageMsg
//! 0xC6507EE1` / `DestroyMsg 0x1ED7AD78` into the destruction FSM (§5.3A). Every `// CONFIRM-LIVE:`
//! comment marks a number/shape that is a modern choice pending a live capture.

use glam::Vec3;
use hecs::{Entity, World};

use mercs2_core::event::{Event, EventArg, EventBus};
use mercs2_core::PhysicsQuery;
use mercs2_core::Transform;

use crate::components::Health;
use crate::events::{DAMAGE_MSG, DESTROY_MSG};

/// The recovered damage taxonomy (`DamageKeyEnum`, code map §5.1 — the enum members are exact; the
/// per-key solver behaviour is confirm-live). Drives the destruction reaction a hit triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKey {
    /// Standard blast.
    Explosion,
    /// Large-calibre bullet (rifles/MGs).
    BulletLarge,
    /// Anti-materiel round.
    BulletAM,
    /// Rocket warhead.
    RocketLarge,
    /// Large blast (heavy ordnance).
    ExplosionLarge,
    /// Vehicle wheel burnout (contact).
    WheelBurnout,
    /// Bunker-buster.
    BunkerBuster,
}

impl DamageKey {
    /// The raw enum ordinal, for posting on the event bus (the exe keys the destruction reaction on
    /// this). Order matches the code map §5.1 listing.
    pub fn ordinal(self) -> u32 {
        match self {
            DamageKey::Explosion => 0,
            DamageKey::BulletLarge => 1,
            DamageKey::BulletAM => 2,
            DamageKey::RocketLarge => 3,
            DamageKey::ExplosionLarge => 4,
            DamageKey::WheelBurnout => 5,
            DamageKey::BunkerBuster => 6,
        }
    }
}

/// Explosion size taxonomy (Xbox debug menu, code map §5.1) — a coarse size band for FX/audio, derived
/// from the blast radius. Names are exact; the radius thresholds are `// CONFIRM-LIVE:` bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosionSize {
    Tiny,
    Small,
    Grenade,
    Vs,
    Large,
    Huge,
}

impl ExplosionSize {
    /// Classify by radius. `// CONFIRM-LIVE:` the exact band edges are the exe's, not captured.
    pub fn from_radius(radius: f32) -> Self {
        match radius {
            r if r < 1.0 => ExplosionSize::Tiny,
            r if r < 3.0 => ExplosionSize::Small,
            r if r < 6.0 => ExplosionSize::Grenade,
            r if r < 10.0 => ExplosionSize::Vs,
            r if r < 18.0 => ExplosionSize::Large,
            _ => ExplosionSize::Huge,
        }
    }
}

/// Post `DamageMsg`/`DestroyMsg` for a health change on `victim`. Args: victim handle, instigator
/// handle, damage amount, damage-key ordinal — the shape the destruction FSM consumes (§5.3A). Emits
/// `DestroyMsg` additionally when the hit takes the target to zero.
fn post_damage_events(
    bus: &mut EventBus,
    victim: Entity,
    instigator: Option<Entity>,
    amount: f32,
    key: DamageKey,
    now_dead: bool,
) {
    let victim_h = victim.to_bits().get() as u32;
    let inst_h = instigator.map(|e| e.to_bits().get() as u32).unwrap_or(0);
    let mut dmg = Event::new(DAMAGE_MSG);
    let _ = dmg.try_push(EventArg::Handle(victim_h));
    let _ = dmg.try_push(EventArg::Handle(inst_h));
    let _ = dmg.try_push(EventArg::Float(amount as f64));
    let _ = dmg.try_push(EventArg::Int(key.ordinal() as i64));
    bus.emit(dmg);

    if now_dead {
        let mut des = Event::new(DESTROY_MSG);
        let _ = des.try_push(EventArg::Handle(victim_h));
        let _ = des.try_push(EventArg::Handle(inst_h));
        bus.emit(des);
    }
}

/// Apply a single direct hit of `amount` to `victim`, subtracting from its [`Health`] and posting the
/// damage/destroy events. Returns the damage actually applied (0 if the victim has no `Health` or is
/// already dead). This is the point-hit path (a bullet/rocket direct impact).
///
/// `// CONFIRM-LIVE:` the exe may apply per-key mitigation / armour before subtracting; unread. Here the
/// authored `amount` is subtracted directly.
pub fn apply_hit(
    world: &mut World,
    bus: &mut EventBus,
    victim: Entity,
    instigator: Option<Entity>,
    amount: f32,
    key: DamageKey,
) -> f32 {
    let (applied, now_dead) = {
        let Ok(mut h) = world.get::<&mut Health>(victim) else {
            return 0.0;
        };
        if h.is_dead() {
            return 0.0;
        }
        let before = h.cur;
        h.cur = (h.cur - amount).max(0.0);
        (before - h.cur, h.cur <= 0.0)
    };
    if applied > 0.0 {
        post_damage_events(bus, victim, instigator, applied, key, now_dead);
    }
    applied
}

/// Distance falloff for a blast: full `damage` at the centre, tapering to 0 at `radius`. `min_falloff`
/// biases the curve — `0` = linear, `>0` holds more damage toward the edge (a rough analog of the
/// authored `MinForceFalloff`).
///
/// `// CONFIRM-LIVE:` the exe's exact falloff curve is unread; this is a conventional
/// `(1 - d/r)` linear taper, softened toward the edge by `min_falloff`.
pub fn radius_falloff(dist: f32, radius: f32, damage: f32, min_falloff: f32) -> f32 {
    if radius <= 0.0 || dist >= radius {
        return 0.0;
    }
    let t = 1.0 - (dist / radius); // 1 at centre, 0 at edge
    let shaped = min_falloff + (1.0 - min_falloff) * t;
    damage * shaped.clamp(0.0, 1.0)
}

/// Detonate an explosion at `center`: apply falloff damage to every [`Health`]-bearing entity within
/// `radius`, optionally gated by a line-of-sight raycast (cover blocks the blast), and post the
/// damage/destroy events. Returns the list of `(victim, damage_applied)`.
///
/// The target set is an **ECS spatial sweep** over entities with a [`Transform`] + [`Health`] within
/// the radius. `// CONFIRM-LIVE:` the exe's `PhysicsCreateExplosion`/`ApplyExplosionToBodies` queries
/// the Havok broadphase for `hkpRigidBody` overlap and applies an impulse; that body-set + impulse land
/// with the physics silo (`DEFERRED.md`). The gameplay-damage overlap here is faithful.
pub fn detonate_explosion(
    world: &mut World,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
    instigator: Option<Entity>,
    center: Vec3,
    stats: &crate::stats::ExplosiveStats,
    key: DamageKey,
) -> Vec<(Entity, f32)> {
    // 1) Gather candidate victims (Transform + Health) inside the radius, with LOS if physics given.
    let mut hits: Vec<(Entity, f32)> = Vec::new();
    {
        for (e, (tf, h)) in world.query::<(&Transform, &Health)>().iter() {
            if h.is_dead() {
                continue;
            }
            let to = tf.translation - center;
            let dist = to.length();
            if dist >= stats.radius {
                continue;
            }
            // Line-of-sight: if a solid surface sits between the blast and the target closer than the
            // target itself, cover absorbs the blast. Skip the ray for a target essentially at centre.
            if let Some(pq) = physics {
                if dist > 1e-3 {
                    let dir = to / dist;
                    if let Some(hit) = pq.raycast(center, dir, dist) {
                        // A hit strictly before the target (not the target's own surface) = blocked.
                        if hit.entity != Some(e) && hit.distance < dist - 0.05 {
                            continue;
                        }
                    }
                }
            }
            let dmg = radius_falloff(dist, stats.radius, stats.damage, stats.min_force_falloff);
            if dmg > 0.0 {
                hits.push((e, dmg));
            }
        }
    }
    // 2) Apply (mutable pass, after the immutable query is dropped).
    let mut applied = Vec::with_capacity(hits.len());
    for (e, dmg) in hits {
        let got = apply_hit(world, bus, e, instigator, dmg, key);
        if got > 0.0 {
            applied.push((e, got));
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_is_full_at_center_zero_at_edge() {
        assert!((radius_falloff(0.0, 10.0, 100.0, 0.0) - 100.0).abs() < 1e-4);
        assert_eq!(radius_falloff(10.0, 10.0, 100.0, 0.0), 0.0);
        assert_eq!(radius_falloff(11.0, 10.0, 100.0, 0.0), 0.0);
        // Monotonic decrease with distance.
        let a = radius_falloff(2.0, 10.0, 100.0, 0.0);
        let b = radius_falloff(5.0, 10.0, 100.0, 0.0);
        assert!(a > b && b > 0.0);
    }

    #[test]
    fn explosion_size_bands() {
        assert_eq!(ExplosionSize::from_radius(0.5), ExplosionSize::Tiny);
        assert_eq!(ExplosionSize::from_radius(8.0), ExplosionSize::Vs);
        assert_eq!(ExplosionSize::from_radius(50.0), ExplosionSize::Huge);
    }

    #[test]
    fn apply_hit_lowers_health_and_kills() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let e = world.spawn((Health::new(50.0),));
        let dmg_seen = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        let ds = dmg_seen.clone();
        bus.on(DAMAGE_MSG, move |_| *ds.borrow_mut() += 1);
        let dead_seen = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        let dd = dead_seen.clone();
        bus.on(DESTROY_MSG, move |_| *dd.borrow_mut() += 1);

        assert_eq!(apply_hit(&mut world, &mut bus, e, None, 20.0, DamageKey::BulletLarge), 20.0);
        assert_eq!(world.get::<&Health>(e).unwrap().cur, 30.0);
        // Overkill clamps at 0 and fires DestroyMsg.
        let got = apply_hit(&mut world, &mut bus, e, None, 100.0, DamageKey::BulletLarge);
        assert_eq!(got, 30.0);
        assert!(world.get::<&Health>(e).unwrap().is_dead());
        assert_eq!(*dmg_seen.borrow(), 2);
        assert_eq!(*dead_seen.borrow(), 1);
        // A dead target takes no further damage.
        assert_eq!(apply_hit(&mut world, &mut bus, e, None, 10.0, DamageKey::BulletLarge), 0.0);
    }
}
