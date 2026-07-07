//! The per-entity perception update — the closest thing to an AI "think" step that is actually
//! recovered as native code (code map §2.4: `FUN_00600240`, per-entity perception-record maintenance).
//!
//! The full planner loop stays data/Lua (§5). What the engine *does* run per entity is perception-record
//! maintenance: for each observer with a [`Perception`] range, count which targetable entities it can
//! see, and classify observers of each target as total / aware / hostile using the
//! [`RelationMatrix`](crate::relation::RelationMatrix). Those counters ([`PerceptionRecord`]) are what
//! the debug overlay reads and what the data/Lua brain keys its goals off.
//!
//! Model (recovered constants; the exact falloff curve is data): an observer *sees* a target when the
//! target is within the observer's [`Perception::range`] (× its visual unit multiplier); it is *aware*
//! of it when the target is also within the target's own emitted [`Stimulus::radius`] (a stimulus that
//! actually reaches the observer). Hostile counts additionally require a negative relation. `attackers`
//! is fed by the combat/action-bus coupling (a later pass) and stays 0 here.

use mercs2_core::{Entity, Transform, World};

use crate::components::{dist_sq, AiFaction, Perception, PerceptionRecord, Stimulus, Target};
use crate::relation::RelationMatrix;

/// Recompute every entity's [`PerceptionRecord`] from the current world state + `relations`. Resets
/// each record first (records are derived, not accumulated across frames). An entity participates as an
/// *observer* if it carries `Perception + Transform + AiFaction`, and as a *target* if it carries
/// `PerceptionRecord + Target + Stimulus + Transform + AiFaction`.
pub fn update_perception(world: &mut World, relations: &RelationMatrix) {
    // Snapshot observers first (position, effective sight range, faction) so the target pass can borrow
    // PerceptionRecord mutably without aliasing the observer query.
    let observers: Vec<(Entity, mercs2_core::glam::Vec3, f32, u32)> = world
        .query::<(&Perception, &Transform, &AiFaction)>()
        .iter()
        .map(|(e, (p, t, f))| (e, t.translation, p.range * p.unit_mult[0], f.0))
        .collect();

    for (te, (rec, tgt, stim, tf, tfac)) in world
        .query::<(&mut PerceptionRecord, &Target, &Stimulus, &Transform, &AiFaction)>()
        .iter()
    {
        *rec = PerceptionRecord::default();
        if !tgt.0 {
            continue; // not a valid AI target → no observers recorded against it
        }
        let stim_r2 = stim.radius * stim.radius;
        for &(oe, opos, orange, ofac) in &observers {
            if oe == te {
                continue; // an entity does not observe itself
            }
            let d2 = dist_sq(opos, tf.translation);
            if d2 > orange * orange {
                continue; // out of this observer's sight range
            }
            rec.total_observers += 1;
            let aware = d2 <= stim_r2; // the target's stimulus reaches the observer
            if aware {
                rec.total_aware += 1;
            }
            if relations.is_hostile(ofac, tfac.0) {
                rec.hostile_observers += 1;
                if aware {
                    rec.hostile_aware += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::glam::Vec3;

    fn observer(world: &mut World, pos: Vec3, faction: u32) -> Entity {
        world.spawn((Perception::default(), Transform::from_translation(pos), AiFaction(faction)))
    }
    fn target(world: &mut World, pos: Vec3, faction: u32) -> Entity {
        world.spawn((
            PerceptionRecord::default(),
            Target::default(),
            Stimulus::default(),
            Transform::from_translation(pos),
            AiFaction(faction),
        ))
    }

    /// A hostile observer in range + inside the stimulus radius registers as a total AND hostile aware
    /// observer of the target.
    #[test]
    fn hostile_observer_in_range_is_counted_aware() {
        let mut world = World::new();
        let _o = observer(&mut world, Vec3::ZERO, 1);
        let t = target(&mut world, Vec3::new(50.0, 0.0, 0.0), 2); // within range 120 and radius 100
        let mut rel = RelationMatrix::new();
        rel.set(1, 2, -100); // faction 1 hostile to faction 2

        update_perception(&mut world, &rel);
        let rec = *world.get::<&PerceptionRecord>(t).unwrap();
        assert_eq!(rec.total_observers, 1);
        assert_eq!(rec.total_aware, 1);
        assert_eq!(rec.hostile_observers, 1);
        assert_eq!(rec.hostile_aware, 1);
    }

    /// A target inside sight range (120) but outside the stimulus radius (100) is observed but NOT
    /// aware — the observer can look that way but the target isn't emitting a reaching stimulus.
    #[test]
    fn in_sight_but_out_of_stimulus_is_observer_not_aware() {
        let mut world = World::new();
        observer(&mut world, Vec3::ZERO, 1);
        let t = target(&mut world, Vec3::new(110.0, 0.0, 0.0), 2); // 110: < range 120, > radius 100
        let rel = RelationMatrix::new(); // neutral

        update_perception(&mut world, &rel);
        let rec = *world.get::<&PerceptionRecord>(t).unwrap();
        assert_eq!(rec.total_observers, 1, "in sight range");
        assert_eq!(rec.total_aware, 0, "outside stimulus radius → not aware");
        assert_eq!(rec.hostile_observers, 0, "neutral relation → not hostile");
    }

    /// A non-targetable entity (Target(false)) records no observers regardless of who is looking.
    #[test]
    fn non_targetable_records_nothing() {
        let mut world = World::new();
        observer(&mut world, Vec3::ZERO, 1);
        let t = world.spawn((
            PerceptionRecord::default(),
            Target(false),
            Stimulus::default(),
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            AiFaction(2),
        ));
        let mut rel = RelationMatrix::new();
        rel.set(1, 2, -100);
        update_perception(&mut world, &rel);
        assert_eq!(world.get::<&PerceptionRecord>(t).unwrap().total_observers, 0);
    }

    /// Out of sight range entirely → nothing recorded.
    #[test]
    fn out_of_range_is_invisible() {
        let mut world = World::new();
        observer(&mut world, Vec3::ZERO, 1);
        let t = target(&mut world, Vec3::new(500.0, 0.0, 0.0), 2); // way past range 120
        let rel = RelationMatrix::new();
        update_perception(&mut world, &rel);
        assert_eq!(world.get::<&PerceptionRecord>(t).unwrap().total_observers, 0);
    }
}
