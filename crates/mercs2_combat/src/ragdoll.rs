//! Ragdoll death reaction — **single-body stand-in** for the physics-silo constrained ragdoll.
//!
//! # Provenance & honesty boundary
//! WILDSTAR-sourced from `WSHumanRagdoll` + the explosion apply
//! (`docs/reverse_engineer/saboteur_mercs2_crossval_render_physics.md`): the recovered
//! `WSHumanRagdoll::SetBodyToRagdoll` snaps each rigid body onto its current animated bone pose and
//! then releases it to Havok, and `WSExplosion::Update` applies a **7-bone impulse spread** floored at
//! `damage::wildstar::FORCE_FLOOR` (200). The faithful engine target is that multi-body Havok ragdoll —
//! which needs the physics silo's constrained rigid bodies (`mercs2_physics` / `mercs2_anim` both mark
//! ragdoll DEFERRED). Until then this is a **clearly-marked single-body stand-in**: on a lethal blast a
//! character is released from animation into one rigid body launched by the blast impulse, integrated
//! under gravity, and settled on the ground. It closes the damage → death → visible-reaction loop with
//! the *shape* of the real system (handoff + impulse + settle); the per-bone articulation is the silo.

use glam::{Quat, Vec3};
use hecs::{Entity, World};

use mercs2_core::Transform;

/// Opt-in marker: entities the game flags as ragdoll-capable (humans with a skeleton). Props react
/// through the destruction FSM instead, so only `Ragdollable` entities are launched on a lethal blast.
pub struct Ragdollable;

/// Where a ragdolling body is in the alive→dynamic→settled lifecycle (mirrors `WSHumanRagdoll`'s
/// body state: `SetBodyToRagdoll`/`SetBodyDynamic` → motored fall → at rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RagdollState {
    /// Airborne / falling — integrating under gravity.
    Launched,
    /// At rest on the ground — no longer integrated.
    Settled,
}

/// Single-body death-physics state. Drives the entity's [`Transform`] once animation releases it (the
/// `SetBodyToRagdoll` handoff). Replaced wholesale when the physics-silo ragdoll lands.
pub struct Ragdoll {
    /// Linear velocity (m/s), seeded from the blast impulse.
    pub lin_vel: Vec3,
    /// Tumble axis (unit) and rate (rad/s) — the coarse stand-in for per-bone articulation.
    pub spin_axis: Vec3,
    pub spin_rate: f32,
    pub state: RagdollState,
}

/// Nominal body mass (kg) converting the WildStar **impulse** (N·s, `max(damage, FORCE_FLOOR)`) into a
/// launch velocity `v = J / m`. `// WILDSTAR/CONFIRM-LIVE:` the real ragdoll distributes the impulse
/// across 7 weighted bones; this lumps it into one 70 kg body.
const NOMINAL_MASS: f32 = 70.0;
/// Gravity (m/s²). `// CONFIRM-LIVE:` the exe uses the Havok world gravity vector.
const GRAVITY: f32 = -9.81;
/// Fraction of velocity retained on ground contact (skid/settle damping).
const GROUND_DAMP: f32 = 0.35;
/// Ground speed below which a grounded body is considered [`RagdollState::Settled`].
const SETTLE_SPEED: f32 = 0.4;

/// The `WSHumanRagdoll::SetBodyToRagdoll` handoff: release `victim` from animation into a launched
/// rigid body whose initial velocity is `impulse / NOMINAL_MASS`. No-op if the entity isn't
/// [`Ragdollable`] or is already ragdolling. The spin axis is derived from the impulse (a body thrown
/// sideways tumbles about the axis perpendicular to its travel and up).
pub fn trigger_ragdoll(world: &mut World, victim: Entity, impulse: Vec3) {
    if world.get::<&Ragdollable>(victim).is_err() || world.get::<&Ragdoll>(victim).is_ok() {
        return;
    }
    let lin_vel = impulse / NOMINAL_MASS;
    // Tumble about the horizontal axis perpendicular to travel; rate scales with launch speed.
    let horiz = Vec3::new(lin_vel.x, 0.0, lin_vel.z);
    let spin_axis = if horiz.length_squared() > 1e-6 {
        horiz.normalize().cross(Vec3::Y).normalize_or_zero()
    } else {
        Vec3::X
    };
    let spin_rate = (lin_vel.length() * 0.4).min(12.0); // rad/s, capped so it doesn't blur
    let _ = world.insert_one(
        victim,
        Ragdoll { lin_vel, spin_axis, spin_rate, state: RagdollState::Launched },
    );
}

/// Integrate every launched [`Ragdoll`] one step: gravity, tumble, and ground collision via
/// `ground_at(pos) -> ground height`. On contact the body damps and, once slow, settles. Call each
/// frame from the engine with its heightmap/collision sampler (tests pass a flat `|_| 0.0`).
pub fn ragdoll_system(world: &mut World, dt: f32, ground_at: impl Fn(Vec3) -> f32) {
    if dt <= 0.0 {
        return;
    }
    for (_e, (tf, rd)) in world.query::<(&mut Transform, &mut Ragdoll)>().iter() {
        if rd.state == RagdollState::Settled {
            continue;
        }
        rd.lin_vel.y += GRAVITY * dt;
        tf.translation += rd.lin_vel * dt;
        if rd.spin_rate.abs() > 1e-4 {
            let spin = Quat::from_axis_angle(rd.spin_axis, rd.spin_rate * dt);
            tf.rotation = (spin * tf.rotation).normalize();
        }
        let ground = ground_at(tf.translation);
        if tf.translation.y <= ground {
            tf.translation.y = ground;
            // Skid + damp on contact; never keep driving into the floor.
            rd.lin_vel *= GROUND_DAMP;
            if rd.lin_vel.y < 0.0 {
                rd.lin_vel.y = 0.0;
            }
            rd.spin_rate *= GROUND_DAMP;
            if rd.lin_vel.length() < SETTLE_SPEED {
                rd.state = RagdollState::Settled;
                rd.lin_vel = Vec3::ZERO;
                rd.spin_rate = 0.0;
            }
        }
    }
}

/// Compute the blast impulse to launch a body caught in an explosion: outward from `center`, lofted
/// upward (blasts throw bodies up + out), magnitude `max(damage, FORCE_FLOOR)` — the WildStar
/// `WSExplosion` ragdoll magnitude. Returns a zero vector if the victim sits exactly at the centre.
pub fn blast_impulse(center: Vec3, victim_pos: Vec3, damage: f32) -> Vec3 {
    let to = victim_pos - center;
    let dir = if to.length_squared() > 1e-6 {
        (to.normalize() + Vec3::Y * 0.6).normalize() // outward + upward loft
    } else {
        Vec3::Y
    };
    let mag = damage.max(crate::damage::wildstar::FORCE_FLOOR);
    dir * mag
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Health;

    fn spawn_body(world: &mut World, pos: Vec3, ragdollable: bool) -> Entity {
        let tf = Transform { translation: pos, ..Transform::IDENTITY };
        if ragdollable {
            world.spawn((tf, Health::new(50.0), Ragdollable))
        } else {
            world.spawn((tf, Health::new(50.0)))
        }
    }

    #[test]
    fn only_ragdollable_entities_launch() {
        let mut world = World::new();
        let human = spawn_body(&mut world, Vec3::new(2.0, 0.0, 0.0), true);
        let prop = spawn_body(&mut world, Vec3::new(2.0, 0.0, 0.0), false);
        let imp = blast_impulse(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), 300.0);
        trigger_ragdoll(&mut world, human, imp);
        trigger_ragdoll(&mut world, prop, imp);
        assert!(world.get::<&Ragdoll>(human).is_ok(), "human launches");
        assert!(world.get::<&Ragdoll>(prop).is_err(), "prop does not ragdoll");
    }

    #[test]
    fn blast_impulse_is_outward_and_lofted_and_floored() {
        // Small damage -> magnitude floored at FORCE_FLOOR (200).
        let imp = blast_impulse(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0), 10.0);
        assert!((imp.length() - crate::damage::wildstar::FORCE_FLOOR).abs() < 1e-3);
        assert!(imp.x > 0.0, "outward (+x)");
        assert!(imp.y > 0.0, "lofted (+y)");
    }

    #[test]
    fn launched_body_falls_and_settles_on_ground() {
        let mut world = World::new();
        let e = spawn_body(&mut world, Vec3::new(0.0, 3.0, 0.0), true);
        // Launch mostly upward/sideways.
        trigger_ragdoll(&mut world, e, Vec3::new(140.0, 700.0, 0.0));
        assert_eq!(world.get::<&Ragdoll>(e).unwrap().state, RagdollState::Launched);
        // Integrate a few seconds against a flat ground at y=0.
        for _ in 0..600 {
            ragdoll_system(&mut world, 1.0 / 60.0, |_| 0.0);
        }
        let tf = world.get::<&Transform>(e).unwrap();
        assert!((tf.translation.y).abs() < 1e-3, "rests on ground y=0, got {}", tf.translation.y);
        assert_eq!(world.get::<&Ragdoll>(e).unwrap().state, RagdollState::Settled);
    }
}
