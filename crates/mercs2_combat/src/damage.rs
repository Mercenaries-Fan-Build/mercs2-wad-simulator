//! Damage / explosion applier — the **recovered sibling-engine solver** (code map §5).
//!
//! # Provenance & honesty boundary (read this)
//! The Mercs2 per-hit damage/explosion solver (`ApplyDamageToPrimaryHealth` / `ApplyDamageToNodeHealth`
//! / `ApplyExplosionToBodies` / `PhysicsCreateExplosion`) is SecuROM-thunked / string-only in retail, so
//! it was long the documented **wall**. The **algorithm** is now recovered first-hand from the
//! **fully-decompiled sibling engine** — The Saboteur / Pandemic "WildStar" 2008 Xbox-360 devkit, whose
//! `WSDamageable::ApplyDamage` / `WSExplosion::CreateExplosion` / `Update` / `AddVictim` /
//! `WSPhysicsObject::ApplyHitForce` decompile with **zero bad instructions** (24 fns;
//! `docs/reverse_engineer/saboteur_damage_solver_symbol_map.md`).
//!
//! The Mercs2 **Jul-08 Xenon prototype** (`Mercs2_Xenon_P`, un-DRM'd) **confirms this exact pipeline by
//! name** — its profiler-scope registration `@0x8237f400` enumerates
//! `GetExplosionCollector`/`ProcessExplosionCast`/`ProcessDamageShadowCast`/`ApplyExplosionToBodies`/
//! `ApplyExplosionToPrimary`/`AppendToForceList`/`PhysicsCreateExplosion` and, crucially,
//! **`ApplyDamageToPrimaryHealth` vs `ApplyDamageToNodeHealth` + `LookupNodeIdFromBodyId`** — the Mercs2
//! two-tier hull-vs-node health split (`RuntimeHealth` + `RuntimeNodeHealth`). So the *structure* here is
//! Mercs2-faithful; the *numeric constants* are the ones the WildStar `.rdata` yielded (`1/30` stagger,
//! `1.5 s` lifetime, force floor `200`, linear falloff, 32-victim cap). The Mercs2 numeric bodies
//! themselves are **not** statically readable (they are genuine VMX128 in the prototype, and a full BSim
//! cross-fork match was tried and fell below the noise floor — see the symbol map), so each such
//! constant is marked `// CONFIRM-LIVE:` and listed in this crate's report.
//!
//! `// WILDSTAR:` marks a shape or number taken from that recovery. Outputs are the ones the exe is
//! known to produce: it lowers health via the destruction bridge (`FUN_0066f220 → … → FUN_006696a0`,
//! `RuntimeHealth.cur` at `+0x04`, stride `0xc`) and posts `DamageMsg 0xC6507EE1` /
//! `DestroyMsg 0x1ED7AD78` into the destruction FSM (§5.3A).

use glam::Vec3;
use hecs::{Entity, World};

use mercs2_core::event::{Event, EventArg, EventBus};
use mercs2_core::PhysicsQuery;
use mercs2_core::Transform;

use crate::events::{DAMAGE_MSG, DESTROY_MSG};
use mercs2_core::Health;

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

/// Recovered explosion timing / force constants (`// WILDSTAR:` from `WSExplosion::Update`/`AddVictim`/
/// `CreateExplosion`, resolved from `.rdata`; the Mercs2 numeric bodies are VMX128/BSim-blocked so these
/// are the sibling-engine values — `// CONFIRM-LIVE:` vs a Mercs2 live capture). Named here so the
/// deferred-staggered blast system and the ragdoll impulse consume one source of truth.
pub mod wildstar {
    /// Per-victim apply delay = `dist * STAGGER_SECS_PER_METER`: the blast "travels" at 30 u/s, so
    /// nearer victims are hit first (`WSExplosion::Update` counts this per-victim delay down to 0).
    /// `// CONFIRM-LIVE:` the `1/30` constant is WildStar's.
    pub const STAGGER_SECS_PER_METER: f32 = 1.0 / 30.0;
    /// Explosion lifetime / defer window — it processes its victim list for this long, then frees itself
    /// (`WSExplosion::Update` `timer@0x1c >= 1.5`). `// CONFIRM-LIVE:` WildStar's `1.5 s`.
    pub const LIFETIME_SECS: f32 = 1.5;
    /// Ragdoll impulse magnitude floor: `mag = max(damage_amount, FORCE_FLOOR)` before the 7-bone
    /// spread. `// CONFIRM-LIVE:` WildStar's `200`.
    pub const FORCE_FLOOR: f32 = 200.0;
    /// Max victims one explosion tracks at once (`WSExplosion` `victimCount@0x2c < 0x20 = MAX_VICTIM`).
    pub const MAX_VICTIMS: usize = 32;
}

// ---------------------------------------------------------------------------
// Per-target mitigation & the two-tier health split (recovered structure)
// ---------------------------------------------------------------------------

/// `WSDamageableBlueprint::damageScale@0x8` — the **per-target vulnerability multiplier** the recovered
/// core formula multiplies into every hit: `health -= amount * damageScale`. Carried on the victim; a
/// victim without one takes damage at scale `1.0`. `// WILDSTAR:` `WSDamageable::ApplyDamage`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageScale(pub f32);

impl Default for DamageScale {
    fn default() -> Self {
        DamageScale(1.0)
    }
}

/// `WSDamageable::flags@0xC & 0x80` — the invincible / not-`AcceptsDamageOfThisType` gate. A victim
/// carrying this marker takes **no** primary-health damage (the recovered applier's first guard). Modelled
/// as an opt-in marker; most entities don't carry it. `// WILDSTAR:` `ApplyDamage` gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Invulnerable;

/// `RuntimeNodeHealth` — the **per-node health pool** of the Mercs2 two-tier split
/// (`ApplyDamageToNodeHealth`, `NodeHealth` ECS pool `11264`; confirmed by the Xenon prototype scope
/// names). A destructible object has a primary hull [`Health`] plus, optionally, this per-node pool for
/// parts that can be shot off independently (a turret, a wheel, a wing). `// WILDSTAR:` the recovered
/// solver splits `ApplyDamageToPrimaryHealth` (hull) from `ApplyDamageToNodeHealth` (parts, which
/// **tally** their hits rather than killing the whole object — `flags & 0x80` part nodes bump
/// `hits@0xE` instead of calling `Die`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeHealth {
    /// Current node HP.
    pub cur: f32,
    /// Max node HP.
    pub max: f32,
    /// Hit tally once the node reaches zero (part nodes tally instead of dying — `hits@0xE`).
    pub hits: u16,
}

impl NodeHealth {
    pub fn new(max: f32) -> Self {
        NodeHealth { cur: max, max, hits: 0 }
    }
    pub fn is_dead(&self) -> bool {
        self.cur <= 0.0
    }
}

/// An axis-aligned half-extent bound on a victim, so the explosion falloff can measure to the
/// **nearest point of the target's box** (the recovered curve) rather than centre-to-centre. Optional:
/// a victim without one is treated as a point. `// WILDSTAR:` `WSExplosion::CreateExplosion` box test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Half-extents (metres) around the [`Transform`] translation.
    pub half_extents: Vec3,
}

// ---------------------------------------------------------------------------
// Event posting (the destruction-FSM outputs — the exe's known outputs)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// The core applier — WSDamageable::ApplyDamage
// ---------------------------------------------------------------------------

/// Apply a single direct hit of **base** `amount` to `victim`, following the recovered core formula:
///
/// ```text
/// if (!invulnerable && health > 0):                 // WSDamageable::ApplyDamage guards
///     health -= amount * damageScale                // ← THE damage line (blueprint.damageScale@0x8)
///     OnDamaged()                                    // post DamageMsg
///     if health <= 0: Die()                          // post DestroyMsg
/// ```
///
/// `amount` is the weapon/blast base; the victim's [`DamageScale`] (default `1.0`) is the per-target
/// vulnerability multiplier the formula applies here (so callers pass the *base*, not a pre-scaled
/// value). A victim carrying [`Invulnerable`] takes nothing. Returns the damage actually applied (0 if
/// no [`Health`], already dead, or invulnerable).
///
/// This is `ApplyDamageToPrimaryHealth` (the hull). Per-node parts go through [`apply_node_hit`].
pub fn apply_hit(
    world: &mut World,
    bus: &mut EventBus,
    victim: Entity,
    instigator: Option<Entity>,
    amount: f32,
    key: DamageKey,
) -> f32 {
    // Gate: the `flags & 0x80` invincible / not-accepted guard.
    if world.get::<&Invulnerable>(victim).is_ok() {
        return 0.0;
    }
    // The per-target vulnerability multiplier (blueprint.damageScale), default 1.0.
    let scale = world.get::<&DamageScale>(victim).map(|d| d.0).unwrap_or(1.0);
    let scaled = amount * scale;

    let (applied, now_dead) = {
        let Ok(mut h) = world.get::<&mut Health>(victim) else {
            return 0.0;
        };
        if h.is_dead() {
            return 0.0;
        }
        let before = h.cur;
        h.cur = (h.cur - scaled).max(0.0);
        (before - h.cur, h.cur <= 0.0)
    };
    if applied > 0.0 {
        post_damage_events(bus, victim, instigator, applied, key, now_dead);
    }
    applied
}

/// Apply a hit to a victim's **per-node** health pool ([`NodeHealth`]) — the recovered
/// `ApplyDamageToNodeHealth` half of the two-tier split. Base `amount` is scaled by the victim's
/// [`DamageScale`] exactly as [`apply_hit`]. When the node reaches zero it **tallies a hit** and posts a
/// `DamageMsg` (parts don't call `Die` on the whole object — `flags & 0x80` nodes bump `hits@0xE`); it
/// posts `DestroyMsg` only if `also_destroys` (a node whose loss is authored to kill the object). Returns
/// the damage applied (0 if the victim has no [`NodeHealth`] or the node is already gone).
pub fn apply_node_hit(
    world: &mut World,
    bus: &mut EventBus,
    victim: Entity,
    instigator: Option<Entity>,
    amount: f32,
    key: DamageKey,
    also_destroys: bool,
) -> f32 {
    if world.get::<&Invulnerable>(victim).is_ok() {
        return 0.0;
    }
    let scale = world.get::<&DamageScale>(victim).map(|d| d.0).unwrap_or(1.0);
    let scaled = amount * scale;

    let (applied, node_dead) = {
        let Ok(mut n) = world.get::<&mut NodeHealth>(victim) else {
            return 0.0;
        };
        if n.is_dead() {
            return 0.0;
        }
        let before = n.cur;
        n.cur = (n.cur - scaled).max(0.0);
        if n.cur <= 0.0 {
            n.hits = n.hits.saturating_add(1); // part node tallies, does not kill the hull
        }
        (before - n.cur, n.cur <= 0.0)
    };
    if applied > 0.0 {
        // A node death is a DestroyMsg only when authored to cascade to the object.
        post_damage_events(bus, victim, instigator, applied, key, node_dead && also_destroys);
    }
    applied
}

// ---------------------------------------------------------------------------
// The falloff curve — WSExplosion::CreateExplosion
// ---------------------------------------------------------------------------

/// Distance falloff for a blast — the **recovered linear curve** `(radius - dist) / radius`, full
/// (`1.0`) at/inside the target, zero at the edge (`WSExplosion::CreateExplosion`, `// WILDSTAR:`).
/// `min_falloff` biases toward the edge; `0` (the `ExplosiveStats` default) is the exact recovered
/// linear curve. `dist` is centre-to-target; use [`radius_falloff_box`] to measure to the nearest box
/// point (the recovered behaviour when the victim has [`Bounds`]).
pub fn radius_falloff(dist: f32, radius: f32, damage: f32, min_falloff: f32) -> f32 {
    if radius <= 0.0 || dist >= radius {
        return 0.0;
    }
    let t = 1.0 - (dist / radius); // 1 at centre, 0 at edge
    let shaped = min_falloff + (1.0 - min_falloff) * t;
    damage * shaped.clamp(0.0, 1.0)
}

/// The recovered box-aware falloff: measures `dist` to the **nearest point of the victim's AABB** (from
/// `center` to `[victim_pos ± half_extents]`), returning `1.0 × damage` when the blast centre is inside
/// the box (`box.Contains(center)`), then the linear curve. `// WILDSTAR:` `WSExplosion::CreateExplosion`
/// — point-blank/inside is full, else linear to the nearest box point.
pub fn radius_falloff_box(
    center: Vec3,
    victim_pos: Vec3,
    half_extents: Vec3,
    radius: f32,
    damage: f32,
    min_falloff: f32,
) -> f32 {
    // Nearest point on the AABB to the blast centre.
    let lo = victim_pos - half_extents;
    let hi = victim_pos + half_extents;
    let nearest = center.clamp(lo, hi);
    let inside = nearest == center; // box.Contains(center)
    if inside {
        // Point-blank / inside → full damage (curve = 1.0), still gated by min_falloff bias.
        return damage * (min_falloff + (1.0 - min_falloff)).clamp(0.0, 1.0);
    }
    let dist = (nearest - center).length();
    radius_falloff(dist, radius, damage, min_falloff)
}

// ---------------------------------------------------------------------------
// The explosion — CreateExplosion (gather) → Update (staggered apply)
// ---------------------------------------------------------------------------

/// One victim caught by a blast, enqueued at [`gather_explosion_victims`] time and applied by
/// [`update_explosion`] when its distance-staggered `countdown` reaches zero — the recovered
/// deferred/staggered apply (`WSExplosion` `UVictim{+0x4 force,+0x28 damage,+0x6c countdown}`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingBlastVictim {
    /// The struck entity.
    pub entity: Entity,
    /// Its world position at gather time (for the ragdoll blast direction).
    pub pos: Vec3,
    /// The falloff-scaled base damage to apply (`baseDamage * falloff`).
    pub damage: f32,
    /// Per-victim delay = `dist * STAGGER_SECS_PER_METER`; counts down, near victims fire first.
    pub countdown: f32,
    /// Set once this victim has been applied.
    pub applied: bool,
}

/// **`WSExplosion::CreateExplosion`** — gather stage. Sweep every [`Health`]-bearing entity with a
/// [`Transform`] inside `radius`, compute box-aware linear falloff, gate on line-of-sight (the recovered
/// `IsVictimShieldedFromExplosion` / `ProcessDamageShadowCast`), and enqueue each surviving victim with
/// its distance-staggered countdown (`dist * `[`STAGGER_SECS_PER_METER`](wildstar::STAGGER_SECS_PER_METER)).
/// Bounded to [`MAX_VICTIMS`](wildstar::MAX_VICTIMS). `// WILDSTAR:` faithful to the recovered gather.
pub fn gather_explosion_victims(
    world: &World,
    physics: Option<&dyn PhysicsQuery>,
    center: Vec3,
    stats: &crate::stats::ExplosiveStats,
    _key: DamageKey,
) -> Vec<PendingBlastVictim> {
    let mut victims: Vec<PendingBlastVictim> = Vec::new();
    for (e, (tf, h)) in world.query::<(&Transform, &Health)>().iter() {
        if victims.len() >= wildstar::MAX_VICTIMS {
            break; // MAX_VICTIM cap
        }
        if h.is_dead() {
            continue;
        }
        // Box-aware distance/falloff if the victim has Bounds, else point.
        let (dist, dmg) = if let Ok(b) = world.get::<&Bounds>(e) {
            let lo = tf.translation - b.half_extents;
            let hi = tf.translation + b.half_extents;
            let nearest = center.clamp(lo, hi);
            let d = (nearest - center).length();
            (
                d,
                radius_falloff_box(
                    center,
                    tf.translation,
                    b.half_extents,
                    stats.radius,
                    stats.damage,
                    stats.min_force_falloff,
                ),
            )
        } else {
            let d = (tf.translation - center).length();
            (d, radius_falloff(d, stats.radius, stats.damage, stats.min_force_falloff))
        };
        if dist >= stats.radius || dmg <= 0.0 {
            continue;
        }
        // Line-of-sight / damage-shadow: a solid surface strictly before the target absorbs the blast.
        if let Some(pq) = physics {
            if dist > 1e-3 {
                let dir = (tf.translation - center) / (tf.translation - center).length().max(1e-6);
                if let Some(hit) = pq.raycast(center, dir, dist) {
                    if hit.entity != Some(e) && hit.distance < dist - 0.05 {
                        continue; // shielded
                    }
                }
            }
        }
        victims.push(PendingBlastVictim {
            entity: e,
            pos: tf.translation,
            damage: dmg,
            countdown: dist * wildstar::STAGGER_SECS_PER_METER,
            applied: false,
        });
    }
    victims
}

/// Apply one ready blast victim: the recovered `WSExplosion::Update` per-victim body — force first (a
/// ragdoll blast-impulse spread on a `Ragdollable` that this hit kills, floored at
/// [`FORCE_FLOOR`](wildstar::FORCE_FLOOR)), then the health damage via [`apply_hit`]. Returns the damage
/// applied.
fn apply_blast_victim(
    world: &mut World,
    bus: &mut EventBus,
    instigator: Option<Entity>,
    center: Vec3,
    v: &PendingBlastVictim,
    key: DamageKey,
) -> f32 {
    let got = apply_hit(world, bus, v.entity, instigator, v.damage, key);
    if got > 0.0 {
        let now_dead = world.get::<&Health>(v.entity).map(|h| h.is_dead()).unwrap_or(false);
        if now_dead {
            // Force: the 7-bone ragdoll impulse spread (floor 200) — WSExplosion applies it on the
            // lethal frame. `blast_impulse` carries the recovered magnitude/loft.
            let impulse = crate::ragdoll::blast_impulse(center, v.pos, got);
            crate::ragdoll::trigger_ragdoll(world, v.entity, impulse);
        }
    }
    got
}

/// Detonate an explosion at `center` **immediately** (gather + apply all victims now). This is the
/// all-at-once convenience/path used by simple callers and tests — the *total* damage is identical to
/// the staggered path; only the timing differs. Returns `(victim, damage_applied)` for each hit.
///
/// For the faithful **deferred + distance-staggered** blast (the recovered `CreateExplosion` → `Update`
/// cadence, near victims first, over [`LIFETIME_SECS`](wildstar::LIFETIME_SECS)), a
/// [`crate::components::RuntimeExplosion`] gathers victims once and [`update_explosion`] applies them as
/// their countdowns expire — driven by [`crate::projectile::explosion_system`].
pub fn detonate_explosion(
    world: &mut World,
    bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
    instigator: Option<Entity>,
    center: Vec3,
    stats: &crate::stats::ExplosiveStats,
    key: DamageKey,
) -> Vec<(Entity, f32)> {
    let victims = gather_explosion_victims(world, physics, center, stats, key);
    let mut applied = Vec::with_capacity(victims.len());
    for v in victims {
        let got = apply_blast_victim(world, bus, instigator, center, &v, key);
        if got > 0.0 {
            applied.push((v.entity, got));
        }
    }
    applied
}

/// **`WSExplosion::Update`** — advance a gathered blast one tick: age its `timer`, count each victim's
/// stagger `countdown` down, and apply (force + damage) every victim whose countdown has reached zero.
/// Returns the damage applied to each victim that fired **this** tick. Drive it from a
/// [`crate::components::RuntimeExplosion`] once `gather_explosion_victims` has filled its list; the blast
/// is done when every victim is applied or `timer >= `[`LIFETIME_SECS`](wildstar::LIFETIME_SECS).
pub fn update_explosion(
    world: &mut World,
    bus: &mut EventBus,
    instigator: Option<Entity>,
    center: Vec3,
    victims: &mut [PendingBlastVictim],
    key: DamageKey,
    dt: f32,
) -> Vec<(Entity, f32)> {
    let mut applied = Vec::new();
    for i in 0..victims.len() {
        if victims[i].applied {
            continue;
        }
        victims[i].countdown -= dt;
        if victims[i].countdown > 0.0 {
            continue; // near victims (small dist*K) fire first
        }
        let v = victims[i];
        let got = apply_blast_victim(world, bus, instigator, center, &v, key);
        victims[i].applied = true;
        if got > 0.0 {
            applied.push((v.entity, got));
        }
    }
    applied
}

/// Whether every victim of a gathered blast has been applied (the `Update` list-empty completion test).
pub fn blast_fully_applied(victims: &[PendingBlastVictim]) -> bool {
    victims.iter().all(|v| v.applied)
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

    /// The box-aware curve: full inside the AABB, linear to the nearest box point outside, matching the
    /// recovered `WSExplosion::CreateExplosion` box test against the plain centre curve.
    #[test]
    fn box_falloff_is_full_inside_and_linear_to_nearest_point() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        // Victim box centred 5 m away, half-extent 2 m along Z → nearest face at 3 m.
        let vpos = Vec3::new(0.0, 0.0, 5.0);
        let half = Vec3::new(2.0, 2.0, 2.0);
        let box_dmg = radius_falloff_box(center, vpos, half, 10.0, 100.0, 0.0);
        // Nearest point at dist 3 → falloff (10-3)/10 = 0.7 → 70.
        assert!((box_dmg - 70.0).abs() < 1e-3);
        // Centre-to-centre (dist 5) would be only 50 — the box measure hits harder, as recovered.
        assert!((radius_falloff(5.0, 10.0, 100.0, 0.0) - 50.0).abs() < 1e-3);
        // Blast centre inside the box → full.
        let inside = radius_falloff_box(Vec3::new(0.0, 0.0, 5.0), vpos, half, 10.0, 100.0, 0.0);
        assert!((inside - 100.0).abs() < 1e-3);
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

    /// The recovered `amount * damageScale` line: a per-target [`DamageScale`] multiplies the base.
    #[test]
    fn damage_scale_multiplies_the_base() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        // A frail target (scale 2.0) and a tough one (scale 0.25).
        let frail = world.spawn((Health::new(100.0), DamageScale(2.0)));
        let tough = world.spawn((Health::new(100.0), DamageScale(0.25)));
        assert_eq!(apply_hit(&mut world, &mut bus, frail, None, 10.0, DamageKey::BulletLarge), 20.0);
        assert_eq!(apply_hit(&mut world, &mut bus, tough, None, 10.0, DamageKey::BulletLarge), 2.5);
    }

    /// The `flags & 0x80` gate: an [`Invulnerable`] victim takes nothing and posts no events.
    #[test]
    fn invulnerable_takes_no_damage() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let e = world.spawn((Health::new(100.0), Invulnerable));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(0u32));
        let s = seen.clone();
        bus.on(DAMAGE_MSG, move |_| *s.borrow_mut() += 1);
        assert_eq!(apply_hit(&mut world, &mut bus, e, None, 50.0, DamageKey::Explosion), 0.0);
        assert_eq!(world.get::<&Health>(e).unwrap().cur, 100.0);
        assert_eq!(*seen.borrow(), 0);
    }

    /// The two-tier split: [`apply_node_hit`] drains a part's [`NodeHealth`] and **tallies** rather than
    /// killing the hull; the primary [`Health`] is untouched by a node hit.
    #[test]
    fn node_health_tallies_and_leaves_hull_intact() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let e = world.spawn((Health::new(500.0), NodeHealth::new(40.0)));
        // Two node hits kill the node (tally 1) but not the object.
        assert_eq!(apply_node_hit(&mut world, &mut bus, e, None, 25.0, DamageKey::BulletLarge, false), 25.0);
        assert_eq!(apply_node_hit(&mut world, &mut bus, e, None, 25.0, DamageKey::BulletLarge, false), 15.0);
        let n = *world.get::<&NodeHealth>(e).unwrap();
        assert!(n.is_dead());
        assert_eq!(n.hits, 1, "part node tallies, does not kill the hull");
        // The hull (primary Health) is untouched by node damage.
        assert_eq!(world.get::<&Health>(e).unwrap().cur, 500.0);
    }

    /// Distance-staggered apply: two victims at different ranges from the same blast fire on different
    /// ticks — the nearer first — over the recovered stagger (`dist * 1/30`).
    #[test]
    fn blast_is_staggered_by_distance() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let center = Vec3::ZERO;
        // Near victim at 3 m (countdown 0.1 s), far victim at 15 m (countdown 0.5 s).
        let near = world.spawn((Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)), Health::new(100.0)));
        let far = world.spawn((Transform::from_translation(Vec3::new(0.0, 0.0, 15.0)), Health::new(100.0)));
        let stats = crate::stats::ExplosiveStats { radius: 30.0, max_force: 1.0, damage: 60.0, min_force_falloff: 0.0 };
        let mut victims = gather_explosion_victims(&world, None, center, &stats, DamageKey::Explosion);
        assert_eq!(victims.len(), 2);
        // Tick 0.15 s: only the near victim's countdown (0.1) has elapsed.
        let hit = update_explosion(&mut world, &mut bus, None, center, &mut victims, DamageKey::Explosion, 0.15);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].0, near);
        assert!(world.get::<&Health>(near).unwrap().cur < 100.0);
        assert_eq!(world.get::<&Health>(far).unwrap().cur, 100.0, "far victim not yet reached");
        // Another 0.4 s (0.55 total) reaches the far victim.
        let hit2 = update_explosion(&mut world, &mut bus, None, center, &mut victims, DamageKey::Explosion, 0.4);
        assert_eq!(hit2.len(), 1);
        assert_eq!(hit2[0].0, far);
        assert!(blast_fully_applied(&victims));
    }

    /// LOS/damage-shadow: a wall strictly between the blast and a victim shields them (no victim gathered).
    #[test]
    fn line_of_sight_shields_the_victim() {
        use mercs2_core::physics_query::{ClosestPoint, RayHit};
        struct Wall;
        impl PhysicsQuery for Wall {
            fn raycast(&self, origin: Vec3, dir: Vec3, _m: f32) -> Option<RayHit> {
                // A wall 1 m from the blast centre, before any victim.
                Some(RayHit { point: origin + dir * 1.0, normal: -dir, distance: 1.0, entity: None })
            }
            fn closest_point(&self, _p: Vec3, _m: f32) -> Option<ClosestPoint> {
                None
            }
            fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
                pos + delta
            }
        }
        let mut world = World::new();
        let victim = world.spawn((Transform::from_translation(Vec3::new(0.0, 0.0, 8.0)), Health::new(100.0)));
        let stats = crate::stats::ExplosiveStats { radius: 20.0, max_force: 1.0, damage: 60.0, min_force_falloff: 0.0 };
        let victims = gather_explosion_victims(&world, Some(&Wall), Vec3::ZERO, &stats, DamageKey::Explosion);
        assert!(victims.is_empty(), "the wall shields the victim from the blast");
        let _ = victim;
    }
}
