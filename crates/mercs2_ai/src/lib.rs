//! `mercs2_ai` — AI: the hash-addressed action bus + per-entity perception + component families.
//!
//! **Silo 11** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 23.
//! **Code map:** `docs/reverse_engineer/ai_code_map.md` (the recovered runtime spine + component
//! census), with `road_graph_ai_driving_code_map.md` for vehicle-AI actuation.
//! **Owned Lua namespace(s):** `Ai`.
//!
//! Per the code map's §8 reimpl disposition, this crate supplies the **mechanism** the engine owns —
//! it deliberately does NOT reimplement a compiled planner, because there isn't one: Mercs 2's AI
//! "brain" (goal selection, cover FSM, squad tactics) is a **data/Lua goal vocabulary dispatched over
//! a hash-addressed action bus**, so the faithful engine side is:
//!
//! - [`AiActionBus`] — the recovered 1024-slot action ring (`DirectAction` / "Ai 1024"), §2.2;
//! - [`perception::update_perception`] — the per-entity perception-record maintenance, §2.4;
//! - [`RelationMatrix`] — the `Ai.SetRelation` `[-100,100]` attitude matrix, §5 / faction map;
//! - [`components`] — the `Ai*`/`Perception`/`Stimulus`/`Squad` reflection components (§3/§4).
//!
//! The `Ai.*` Lua order surface (`Ai.Goal`, `Ai.SetRelation`, `Ai.SetState`, …) posts to this bus /
//! sets these components via the game's `EngineHost` seam; the goals themselves stay authored content.
//!
//! # Module map
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`bus`] | [`AiActionBus`] / [`AiAction`] / [`RING_CAP`] (`0x400`) / [`goal_action_hash`] — the 1024-slot `DirectAction` ring and verb hashing (`pandemic_hash_m2` of the lowercased verb). Over-cap posts are **dropped, not overwritten**. |
//! | [`relation`] | [`RelationMatrix`] — the *directed* attitude matrix, clamped [`RELATION_MIN`]`..=`[`RELATION_MAX`] (`-100..=100`); unset pairs read `0`. |
//! | [`perception`] | [`update_perception`] — recomputes every [`PerceptionRecord`] from observer positions, sight range and the relation matrix. Records are derived and reset each pass. |
//! | [`components`] | [`AiBehavior`] / [`AiSkill`] / [`Perception`] / [`Stimulus`] / [`Target`] / [`Squad`] / [`PerceptionRecord`] with their m2 class hashes and recovered defaults, plus the [`AiFaction`] key the perception pass needs. |
//!
//! [`AiWorld`] bundles the two world-global pieces (bus + relations). `mercs2_engine` re-exports this
//! crate as `mercs2_engine::ai`, holds an `AiWorld` on its runtime, ticks it each fixed step, and
//! forwards the `Ai.*` bindings into it from the script host.

pub mod bus;
pub mod components;
pub mod perception;
pub mod relation;

pub use bus::{goal_action_hash, AiAction, AiActionBus, RING_CAP};
pub use components::{
    AiBehavior, AiFaction, AiSkill, Perception, PerceptionRecord, Squad, Stimulus, Target,
};
pub use perception::update_perception;
pub use relation::{RelationMatrix, RELATION_MAX, RELATION_MIN};

use mercs2_core::World;

/// The host-owned AI mechanism: the action ring + the relation matrix, the two pieces the `Ai.*` Lua
/// surface drives directly. An AI actor's per-entity state (Perception/Stimulus/records) lives on ECS
/// components in the `World`; this bundles the world-global AI state the script host holds and ticks.
///
/// The game's `EngineHost` impl forwards `Ai.Goal`/`Ai.SetRelation`/… here; [`tick`](Self::tick) runs
/// the per-entity perception update over the world each fixed step (idle until AI entities exist, the
/// same data-driven way the vehicle/combat systems idle until their components are spawned).
#[derive(Default)]
pub struct AiWorld {
    /// The 1024-slot `DirectAction` ring — goals/orders posted for the (data/Lua) brain to consume.
    pub bus: AiActionBus,
    /// The `[-100,100]` directed attitude matrix behind `Ai.SetRelation`/`GetRelation`.
    pub relations: RelationMatrix,
}

impl AiWorld {
    pub fn new() -> Self {
        AiWorld::default()
    }

    /// `Ai.Goal(guid, goal)` — hash the goal verb and post it to the action ring (`DirectAction`).
    /// Returns whether the ring accepted it (false = the 1024-slot budget was full). The goal string
    /// is authored content; here we address it by hash exactly as the engine does.
    pub fn goal(&mut self, guid: u32, goal: &str) -> bool {
        self.bus.direct_action(guid, goal_action_hash(goal))
    }

    /// `Ai.DirectAction(guid, actionHash)` — post a pre-hashed action to the ring.
    pub fn direct_action(&mut self, guid: u32, action_hash: u32) -> bool {
        self.bus.direct_action(guid, action_hash)
    }

    /// The `Ai.*` **order surface** (`Role`/`Anchor`/`Squad`/`Deploy`/`SetHaste`/`RemoveGoal`/…): post
    /// the order verb, hash-addressed, to the same 1024-ring the (data/Lua) brain consumes. This is the
    /// mechanism the engine owns for every order directive — there is no compiled per-verb body (AI code
    /// map §5/§8); the verb *is* the identity and the parameters are brain-consumed content.
    pub fn order(&mut self, guid: u32, verb: &str) -> bool {
        self.bus.direct_action(guid, goal_action_hash(verb))
    }

    /// `Ai.SetRelation(from, to, value)` — set the directed attitude, clamped `[-100,100]`.
    pub fn set_relation(&mut self, from: u32, to: u32, value: i32) {
        self.relations.set(from, to, value);
    }

    /// `Ai.GetRelation(from, to)` — the directed attitude (`0` if unset).
    pub fn get_relation(&self, from: u32, to: u32) -> i32 {
        self.relations.get(from, to)
    }

    /// Per-fixed-step AI update: recompute every entity's perception record from the world + relations
    /// (§2.4). Idle when no AI entities carry perception components. The action ring is drained by the
    /// (data/Lua) brain consumer, not here.
    pub fn tick(&mut self, world: &mut World) {
        update_perception(world, &self.relations);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Ai.*` surface roundtrips through `AiWorld`: a goal posts to the ring; a relation set/gets.
    #[test]
    fn aiworld_goal_posts_and_relation_roundtrips() {
        let mut ai = AiWorld::new();
        assert!(ai.goal(0x1000, "Attack"));
        assert_eq!(ai.bus.len(), 1);
        assert_eq!(ai.bus.drain()[0].hash, goal_action_hash("attack"));

        ai.set_relation(1, 2, -100);
        assert_eq!(ai.get_relation(1, 2), -100);
        assert_eq!(ai.get_relation(2, 1), 0);
    }

    /// `AiWorld::tick` runs the perception update: a hostile observer becomes visible in the target's
    /// record — proving the world-global mechanism drives the per-entity records end to end.
    #[test]
    fn tick_updates_perception_records() {
        use mercs2_core::glam::Vec3;
        use mercs2_core::{Transform, World};

        let mut world = World::new();
        world.spawn((Perception::default(), Transform::from_translation(Vec3::ZERO), AiFaction(1)));
        let t = world.spawn((
            PerceptionRecord::default(),
            Target::default(),
            Stimulus::default(),
            Transform::from_translation(Vec3::new(40.0, 0.0, 0.0)),
            AiFaction(2),
        ));

        let mut ai = AiWorld::new();
        ai.set_relation(1, 2, -80);
        ai.tick(&mut world);
        assert_eq!(world.get::<&PerceptionRecord>(t).unwrap().hostile_aware, 1);
    }
}
