//! Ragdoll death reaction — the leaf-side **death-reaction request** that hands a killed character off
//! to the constrained multi-body Havok ragdoll (`mercs2_physics::ragdoll`, driven engine-side).
//!
//! # Provenance & honesty boundary
//! WILDSTAR-sourced from `WSHumanRagdoll` + the explosion apply
//! (`docs/reverse_engineer/saboteur_mercs2_crossval_render_physics.md`): the recovered
//! `WSHumanRagdoll::SetBodyToRagdoll` snaps each rigid body onto its current animated bone pose and then
//! releases it to Havok, and `WSExplosion::Update` applies a **7-bone impulse spread** floored at
//! `damage::wildstar::FORCE_FLOOR` (200).
//!
//! # Why this module is now a thin handoff (superseded single-body stand-in)
//! `mercs2_combat` is a **leaf** crate (`mercs2_core` + `mercs2_formats` only; carve rule §4), so it
//! cannot reference the physics-system ragdoll. The faithful **constrained multi-body** ragdoll lives in
//! [`mercs2_physics::ragdoll`] (`RagdollDef::human` — the recovered 11-capsule body/bone map — plus the
//! XPBD `Ragdoll` sim) and is snapped onto the posed skeleton through the
//! [`mercs2_anim::ragdoll`] seam. This module therefore owns only the **decision + seed**: flag a killed
//! [`Ragdollable`] for ragdolling and record the killing blast's initial velocity. The engine layer
//! (`mercs2_engine`) reads that [`Ragdoll`] handoff, spawns the multi-body ragdoll seeded from the
//! victim's current posed skeleton, steps it against the world each fixed tick, and writes it back to the
//! skin. The former single-rigid-body integrator (`Ragdoll { lin_vel, spin_axis, .. }` + `ragdoll_system`)
//! that lived here is **superseded** by that sim and removed; `blast_impulse` (the recovered magnitude /
//! loft) is kept intact because `damage.rs` seeds the handoff through it.

use glam::Vec3;
use hecs::{Entity, World};

/// Opt-in marker: entities the game flags as ragdoll-capable (humans with a skeleton). Props react
/// through the destruction FSM instead, so only `Ragdollable` entities are handed to the ragdoll on a
/// lethal hit.
pub struct Ragdollable;

/// Where a ragdolling body is in its lifecycle (mirrors `WSHumanRagdoll`'s body state: released into the
/// dynamic ragdoll → at rest). The engine flips [`Ragdoll::state`] to `Settled` once the multi-body sim
/// comes to rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RagdollState {
    /// The multi-body ragdoll is active — articulating / falling under its constraints.
    Launched,
    /// It has come to rest.
    Settled,
}

/// The **death-reaction handoff** placed on a killed [`Ragdollable`]. It is the leaf→engine seam for the
/// constrained multi-body Havok ragdoll: the engine detects this on a dead entity, spawns
/// `mercs2_physics::ragdoll::Ragdoll::human()` snapped onto the victim's posed skeleton, and seeds every
/// ragdoll body's initial velocity from [`seed_velocity`](Ragdoll::seed_velocity).
///
/// Supersedes the old single-rigid-body stand-in that integrated on this component directly — the real
/// per-bone articulation now lives in `mercs2_physics::ragdoll`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ragdoll {
    /// Initial per-body velocity (m/s) the engine seeds every ragdoll body with — the killing blast
    /// impulse (`blast_impulse`, N·s) over the nominal body mass. `ZERO` for a non-blast kill (the body
    /// simply goes limp and falls).
    pub seed_velocity: Vec3,
    /// Lifecycle state; the engine flips this to [`RagdollState::Settled`] once the multi-body sim rests.
    pub state: RagdollState,
}

/// Nominal body mass (kg) converting the WildStar **impulse** (N·s, `max(damage, FORCE_FLOOR)`) into a
/// launch velocity `v = J / m`. `// WILDSTAR/CONFIRM-LIVE:` the real ragdoll distributes the impulse
/// across 7 weighted bones; this lumps it into one 70 kg body to seed the whole ragdoll's initial motion.
const NOMINAL_MASS: f32 = 70.0;

/// The `WSHumanRagdoll::SetBodyToRagdoll` handoff: flag `victim` for the constrained multi-body ragdoll,
/// recording the initial per-body velocity `impulse / NOMINAL_MASS`. No-op if the entity isn't
/// [`Ragdollable`] or is already flagged. The engine layer consumes this [`Ragdoll`] to spawn + step the
/// physics ragdoll seeded on the victim's posed skeleton.
pub fn trigger_ragdoll(world: &mut World, victim: Entity, impulse: Vec3) {
    if world.get::<&Ragdollable>(victim).is_err() || world.get::<&Ragdoll>(victim).is_ok() {
        return;
    }
    let seed_velocity = impulse / NOMINAL_MASS;
    let _ = world.insert_one(victim, Ragdoll { seed_velocity, state: RagdollState::Launched });
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
    use mercs2_core::{Health, Transform};

    fn spawn_body(world: &mut World, pos: Vec3, ragdollable: bool) -> Entity {
        let tf = Transform { translation: pos, ..Transform::IDENTITY };
        if ragdollable {
            world.spawn((tf, Health::new(50.0), Ragdollable))
        } else {
            world.spawn((tf, Health::new(50.0)))
        }
    }

    #[test]
    fn only_ragdollable_entities_are_flagged() {
        let mut world = World::new();
        let human = spawn_body(&mut world, Vec3::new(2.0, 0.0, 0.0), true);
        let prop = spawn_body(&mut world, Vec3::new(2.0, 0.0, 0.0), false);
        let imp = blast_impulse(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), 300.0);
        trigger_ragdoll(&mut world, human, imp);
        trigger_ragdoll(&mut world, prop, imp);
        assert!(world.get::<&Ragdoll>(human).is_ok(), "human is handed off to the ragdoll");
        assert!(world.get::<&Ragdoll>(prop).is_err(), "prop does not ragdoll");
        assert_eq!(world.get::<&Ragdoll>(human).unwrap().state, RagdollState::Launched);
    }

    #[test]
    fn trigger_seeds_per_body_velocity_from_impulse() {
        let mut world = World::new();
        let e = spawn_body(&mut world, Vec3::ZERO, true);
        let impulse = Vec3::new(140.0, 700.0, 0.0);
        trigger_ragdoll(&mut world, e, impulse);
        let rd = *world.get::<&Ragdoll>(e).unwrap();
        // The engine seeds every ragdoll body with impulse / nominal mass.
        assert!((rd.seed_velocity - impulse / NOMINAL_MASS).length() < 1e-4);
        // Re-triggering is a no-op (already handed off).
        trigger_ragdoll(&mut world, e, Vec3::ZERO);
        assert!((world.get::<&Ragdoll>(e).unwrap().seed_velocity - impulse / NOMINAL_MASS).length() < 1e-4);
    }

    #[test]
    fn blast_impulse_is_outward_and_lofted_and_floored() {
        // Small damage -> magnitude floored at FORCE_FLOOR (200).
        let imp = blast_impulse(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0), 10.0);
        assert!((imp.length() - crate::damage::wildstar::FORCE_FLOOR).abs() < 1e-3);
        assert!(imp.x > 0.0, "outward (+x)");
        assert!(imp.y > 0.0, "lofted (+y)");
    }
}
