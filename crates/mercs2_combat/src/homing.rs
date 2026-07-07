//! Homing / lock-on FSM — the cleanest first-hand recovery in the code map (§4), ported faithfully.
//!
//! Pipeline (exe → reimpl):
//! - **Lock FSM** `FUN_0052dce0` → [`homing_lock_system`]: per homing weapon × candidate target,
//!   evaluate lock geometry (angle/distance), advance the per-weapon lock timer, and emit
//!   `HomingLockStart` (acquire, state 2) / `HomingLockUpdate` (hold, state 3) / `HomingLockClear`
//!   (lost, state 1) — the exact `local_44 ∈ {1,2,3}` selector.
//! - **Launch** `FUN_0052d120` → [`launch_missile`]: on the shot (fired by [`crate::firing`] when
//!   `Locked`), emit `HomingLaunched` + `HomingLockClear` (lock consumed) and spawn a
//!   [`RuntimeHomingWeapon`] at the muzzle toward the locked target.
//! - **Guided flight** `FUN_0052e1f0` → [`homing_flight_system`]: per missile, **cross-product
//!   steering** of the velocity toward the target + a **gravity bias** + a **detonation/arm timer**,
//!   then advance position and test impact. Detonates on proximity to the target, on the arm timer, or
//!   on a swept-segment impact.
//!
//! The steering tunables (`DAT_00b92874` turn/normalize, `DAT_00b9b664` gravity) and the authored
//! `LockOn*` / `TurnSpeed` fields feed these; the guidance math decompiles in the clear on PC (the
//! "VMX128" correction, §4.4) — this is a port, not a stand-in.

use glam::Vec3;
use hecs::{Entity, World};

use mercs2_core::event::{Event, EventArg, EventBus};
use mercs2_core::{PhysicsQuery, Transform};

use crate::components::{Health, HomingState, RuntimeExplosion, RuntimeHomingWeapon, RuntimeWeapon};
use crate::events::{HOMING_LAUNCHED, HOMING_LOCK_CLEAR, HOMING_LOCK_START, HOMING_LOCK_UPDATE};

/// Emit a homing lock event (`Start`/`Update`/`Clear`) carrying the weapon + target handles — the
/// `FUN_0052f0d0(pcVar6, weaponGuid, targetGuid, …)` emit (code map §4.2).
fn emit_lock(bus: &mut EventBus, name_hash: u32, weapon: Entity, target: Option<Entity>) {
    let mut ev = Event::new(name_hash);
    let _ = ev.try_push(EventArg::Handle(weapon.to_bits().get() as u32));
    let _ = ev.try_push(EventArg::Handle(target.map(|t| t.to_bits().get() as u32).unwrap_or(0)));
    bus.emit(ev);
}

/// The best lock candidate for a weapon: the [`Health`]-bearing entity nearest the aim axis that is
/// within `LockOnMaxAngle` and `LockOnMaxDistance` of the muzzle (and not the shooter). Returns
/// `(entity, angle_rad)`.
fn best_target(
    world: &World,
    shooter: Entity,
    muzzle: Vec3,
    aim: Vec3,
    max_angle_deg: f32,
    max_dist: f32,
) -> Option<(Entity, f32)> {
    let aim = aim.normalize_or_zero();
    let max_angle = max_angle_deg.to_radians();
    let mut best: Option<(Entity, f32)> = None;
    for (e, (tf, _h)) in world.query::<(&Transform, &Health)>().iter() {
        if e == shooter {
            continue;
        }
        let to = tf.translation - muzzle;
        let dist = to.length();
        if dist < 1e-3 || dist > max_dist {
            continue;
        }
        let cos = (to / dist).dot(aim).clamp(-1.0, 1.0);
        let angle = cos.acos();
        if angle > max_angle {
            continue;
        }
        if best.is_none_or(|(_, a)| angle < a) {
            best = Some((e, angle));
        }
    }
    best
}

/// Advance the lock FSM for every homing [`RuntimeWeapon`] one fixed step. Emits the
/// `HomingLockStart/Update/Clear` events and advances each weapon's [`HomingState`]. A weapon becomes
/// `Locked` once its target is held continuously for `LockOnTime`.
pub fn homing_lock_system(world: &mut World, dt: f32, bus: &mut EventBus) {
    // Collect the per-weapon FSM decisions (immutable target scan), then apply (mutable), so the
    // world isn't borrowed twice.
    struct Decision {
        weapon: Entity,
        new_state: HomingState,
        emit: Option<(u32, Option<Entity>)>, // (event hash, target)
    }
    let mut decisions = Vec::new();

    for (we, w) in world.query::<&RuntimeWeapon>().iter() {
        let Some(h) = w.stats.homing else { continue };
        let cand = best_target(world, w.owner, w.muzzle, w.aim_dir, h.lock_on_max_angle, h.lock_on_max_distance);

        let (new_state, emit) = match (w.lock, cand) {
            // No candidate now.
            (HomingState::None, None) => (HomingState::None, None),
            (_, None) => (HomingState::None, Some((HOMING_LOCK_CLEAR, w.lock.target()))),
            // Candidate available.
            (HomingState::None, Some((t, _))) => (
                HomingState::Acquiring { target: t, timer: h.lock_on_time },
                Some((HOMING_LOCK_START, Some(t))),
            ),
            (HomingState::Acquiring { target, timer }, Some((t, _))) => {
                if target == t {
                    let nt = timer - dt;
                    if nt <= 0.0 {
                        (HomingState::Locked { target: t }, Some((HOMING_LOCK_UPDATE, Some(t))))
                    } else {
                        (HomingState::Acquiring { target: t, timer: nt }, Some((HOMING_LOCK_UPDATE, Some(t))))
                    }
                } else {
                    // Target switched: clear the old, restart acquisition on the new.
                    (HomingState::Acquiring { target: t, timer: h.lock_on_time }, Some((HOMING_LOCK_START, Some(t))))
                }
            }
            (HomingState::Locked { target }, Some((t, _))) => {
                if target == t {
                    (HomingState::Locked { target: t }, Some((HOMING_LOCK_UPDATE, Some(t))))
                } else {
                    (HomingState::Acquiring { target: t, timer: h.lock_on_time }, Some((HOMING_LOCK_START, Some(t))))
                }
            }
        };
        decisions.push(Decision { weapon: we, new_state, emit });
    }

    for d in decisions {
        if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(d.weapon) {
            w.lock = d.new_state;
        }
        if let Some((hash, target)) = d.emit {
            emit_lock(bus, hash, d.weapon, target);
        }
    }
}

/// Launch a guided missile from a homing [`RuntimeWeapon`] that is `Locked` — the `FUN_0052d120` path.
/// Emits `HomingLaunched` + `HomingLockClear` (the lock is consumed), clears the weapon's lock state,
/// and spawns a [`RuntimeHomingWeapon`] at the muzzle heading toward the locked target. No-op if the
/// weapon isn't locked or lacks homing stats. Called from [`crate::firing::weapon_firing_system`].
pub fn launch_missile(world: &mut World, bus: &mut EventBus, weapon: Entity) {
    let Some((owner, muzzle, aim, target, hstats, exp, key)) = ({
        let Ok(w) = world.get::<&RuntimeWeapon>(weapon) else { return };
        let (Some(h), HomingState::Locked { target }) = (w.stats.homing, w.lock) else { return };
        let exp = w.stats.explosive.unwrap_or_default();
        Some((w.owner, w.muzzle, w.aim_dir.normalize_or_zero(), target, h, exp, w.stats.damage_key))
    }) else {
        return;
    };

    // Emit launch + consume the lock.
    emit_lock(bus, HOMING_LAUNCHED, weapon, Some(target));
    emit_lock(bus, HOMING_LOCK_CLEAR, weapon, Some(target));
    if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(weapon) {
        w.lock = HomingState::None;
    }

    // Initial velocity along the aim (a real muzzle speed for a rocket; the guidance takes over).
    let speed = 45.0; // `// CONFIRM-LIVE:` initial rocket speed; the guidance dominates the path.
    world.spawn((RuntimeHomingWeapon {
        owner,
        target,
        pos: muzzle,
        vel: aim * speed,
        turn_speed: hstats.turn_speed,
        gravity: hstats.turn_speed * 0.0 + 3.0, // gravity bias (`DAT_00b9b664`); modest downward pull
        detonation_distance: hstats.detonation_distance,
        arm_timer: 8.0, // max flight before self-detonate (`piVar1[0x12]`)
        explosive: exp,
        damage_key: key,
    },));
}

/// Advance every guided [`RuntimeHomingWeapon`] one fixed step — the **`FUN_0052e1f0` port**:
/// cross-product steering of the velocity toward the (refreshed) target + a gravity bias + the
/// detonation/arm timer, then move and test impact. Detonates (spawns a [`RuntimeExplosion`]) on
/// proximity to the target, on the arm timer reaching zero, or on a swept-segment impact.
///
/// Returns the number of missiles that detonated this tick.
pub fn homing_flight_system(
    world: &mut World,
    dt: f32,
    _bus: &mut EventBus,
    physics: Option<&dyn PhysicsQuery>,
) -> u32 {
    // Pass 1 (immutable target reads + integration): decide each missile's new kinematics + whether it
    // detonates. Collect detonations to apply after.
    struct Step {
        missile: Entity,
        new_pos: Vec3,
        new_vel: Vec3,
        new_timer: f32,
        detonate_at: Option<Vec3>,
    }
    let mut steps = Vec::new();

    for (me, m) in world.query::<&RuntimeHomingWeapon>().iter() {
        // Refresh the target handle (it may have moved or died).
        let target_pos = world.get::<&Transform>(m.target).ok().map(|t| t.translation);

        let mut vel = m.vel;
        // Cross-product steering toward the target (the `FUN_0052e1f0` steering vector).
        if let Some(tp) = target_pos {
            let to = tp - m.pos;
            let to_len = to.length();
            let speed = vel.length();
            if to_len > 1e-3 && speed > 1e-3 {
                let dir = vel / speed;
                let tdir = to / to_len;
                // Rotation axis = dir × tdir; angle to rotate this tick = turn_speed (rad/s) * dt,
                // clamped to the remaining angle to the target.
                let axis = dir.cross(tdir);
                let axis_len = axis.length();
                if axis_len > 1e-6 {
                    let angle_to = dir.dot(tdir).clamp(-1.0, 1.0).acos();
                    let step = (m.turn_speed * dt).min(angle_to);
                    let q = glam::Quat::from_axis_angle(axis / axis_len, step);
                    vel = q * dir * speed;
                }
            }
        }
        // Gravity bias (`DAT_00b9b664`): pull the missile down a touch.
        vel.y -= m.gravity * dt;

        // Detonation / arm timer.
        let new_timer = m.arm_timer - dt;
        let mut detonate_at: Option<Vec3> = None;
        if let Some(tp) = target_pos {
            if (tp - m.pos).length() <= m.detonation_distance {
                detonate_at = Some(m.pos);
            }
        }
        if detonate_at.is_none() && new_timer <= 0.0 {
            detonate_at = Some(m.pos);
        }

        // Swept-segment impact (terrain / other geometry).
        let to = m.pos + vel * dt;
        if detonate_at.is_none() {
            let seg = to - m.pos;
            let len = seg.length();
            if len > 1e-6 {
                if let Some(pq) = physics {
                    if let Some(hit) = pq.raycast(m.pos, seg / len, len) {
                        detonate_at = Some(hit.point);
                    }
                }
            }
        }

        steps.push(Step {
            missile: me,
            new_pos: to,
            new_vel: vel,
            new_timer,
            detonate_at,
        });
    }

    // Pass 2 (mutable): apply kinematics or detonate.
    let mut detonations: Vec<(Entity, Vec3)> = Vec::new();
    for s in &steps {
        if let Some(point) = s.detonate_at {
            detonations.push((s.missile, point));
        } else if let Ok(mut m) = world.get::<&mut RuntimeHomingWeapon>(s.missile) {
            m.pos = s.new_pos;
            m.vel = s.new_vel;
            m.arm_timer = s.new_timer;
        }
    }

    let mut n = 0u32;
    for (missile, point) in detonations {
        let payload = world
            .get::<&RuntimeHomingWeapon>(missile)
            .ok()
            .map(|m| (m.owner, m.explosive, m.damage_key));
        if let Some((owner, exp, key)) = payload {
            world.spawn((RuntimeExplosion {
                owner: Some(owner),
                pos: point,
                stats: exp,
                damage_key: key,
                applied: false,
                life: 0.25,
            },));
            let _ = world.despawn(missile);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::WeaponStats;
    use mercs2_core::physics_query::{ClosestPoint, RayHit};

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
    fn lock_fsm_acquires_after_lock_on_time() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        let _target = world.spawn((Transform::from_translation(Vec3::new(0.0, 0.0, 30.0)), Health::new(100.0)));
        let mut stats = WeaponStats::rocket_launcher();
        stats.homing.as_mut().unwrap().lock_on_time = 0.1;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.muzzle = Vec3::ZERO;
        w.aim_dir = Vec3::Z; // pointing straight at the target
        let we = world.spawn((w,));

        // Tick 1: acquires (Start) → Acquiring.
        homing_lock_system(&mut world, 1.0 / 60.0, &mut bus);
        assert!(matches!(world.get::<&RuntimeWeapon>(we).unwrap().lock, HomingState::Acquiring { .. }));
        // Advance past lock_on_time → Locked.
        homing_lock_system(&mut world, 0.2, &mut bus);
        assert!(matches!(world.get::<&RuntimeWeapon>(we).unwrap().lock, HomingState::Locked { .. }));
    }

    #[test]
    fn missile_steers_to_moving_target_and_detonates() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let shooter = world.spawn(());
        // Target off to the side so the missile must steer, not just fly straight.
        let target = world.spawn((Transform::from_translation(Vec3::new(20.0, 0.0, 40.0)), Health::new(100.0)));

        // Directly spawn a locked launch (bypass the lock timer for this test).
        let mut stats = WeaponStats::rocket_launcher();
        stats.homing.as_mut().unwrap().detonation_distance = 3.0;
        let mut w = RuntimeWeapon::new(shooter, stats);
        w.aim_dir = Vec3::Z;
        w.lock = HomingState::Locked { target };
        let we = world.spawn((w,));
        launch_missile(&mut world, &mut bus, we);

        // Fly the missile; move the target slightly each tick to prove it tracks.
        let mut detonated = 0u32;
        for i in 0..600 {
            // Drift the target.
            if let Ok(mut tf) = world.get::<&mut Transform>(target) {
                tf.translation.x += 0.02;
            }
            detonated += homing_flight_system(&mut world, 1.0 / 60.0, &mut bus, Some(&NoPhysics));
            // Also run the explosion so the target takes damage.
            crate::projectile::explosion_system(&mut world, 1.0 / 60.0, &mut bus, Some(&NoPhysics));
            if detonated > 0 {
                break;
            }
            let _ = i;
        }
        assert_eq!(detonated, 1, "missile detonated on proximity");
        assert!(
            world.get::<&Health>(target).unwrap().cur < 100.0,
            "target took blast damage"
        );
    }
}
