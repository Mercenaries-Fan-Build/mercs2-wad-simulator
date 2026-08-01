//! `mercs2_destruction` — entity destruction: the health→state-machine→node-enable pipeline.
//!
//! **Code map:** [`docs/reverse_engineer/state_machine_destruction_code_map.md`], with the
//! vehicle-side reading in [`docs/modernization/vehicle_model_spec.md`] §5.
//!
//! # What retail does
//!
//! ```text
//! Health ↓  →  DamageMsg (0xC6507EE1) / DestroyMsg (0x1ED7AD78)
//!           →  per-switch-node state machine  (SetStateOnMsg)
//!           →  SHOW / HIDE over HIER SUBTREES
//!           →  the node-enable table (OBJ+0x2a0) = draw-gate clause 3
//!           +  CreateObject (debris) / StartEmitter (fire) from the entered state's enter-script
//! ```
//!
//! The states are `Pristine → Damaged → StartDestroyed → Destroyed`, plus `DetachState` for
//! **break-parts** — which is how a car sheds a hood or a tank loses its turret as health falls.
//! `SHOW`/`HIDE` act on a whole subtree, so a governed parent takes its children with it.
//!
//! # Why this is a crate and not part of physics
//!
//! Destruction contains essentially no physics math — it is a state machine over model nodes. Physics
//! is a *consumer* of its output (debris get rigid bodies), not its host. Hosting it in
//! `mercs2_physics` would force that crate to depend on `mercs2_formats`'s orchestrator parser and to
//! write render state. Here, the natural dependencies are exactly `mercs2_core` + `mercs2_formats`,
//! which is the workspace carve rule for free.
//!
//! # The split
//!
//! - **Component** — [`mercs2_core::Destructible`] holds what survives between ticks (the delivered
//!   message set, the chosen state per switch node, the node-enable table).
//! - **Parsing / replay** — `mercs2_formats::orchestrator` owns the machine format and the pure
//!   functions ([`node_states_for_delivered`](mercs2_formats::orchestrator::node_states_for_delivered),
//!   [`machine_node_enable`](mercs2_formats::orchestrator::machine_node_enable)).
//! - **This crate** — the runtime: deliver messages as health falls, keep the delivered set
//!   **monotonic**, recompute only when it changes, and emit [`DestructionIntent`]s for the side
//!   effects a leaf crate cannot perform itself.
//!
//! # Faithfulness notes (read before trusting output)
//!
//! - The **node-enable seed is not ground truth.** `NodeSeed`'s two variants are a choice validated
//!   against real models, because the engine constructor's `memset` sits behind a register alias in
//!   the decomp (`model_render_gate_spec.md` §6). Everything here inherits that caveat.
//! - The **health→message band thresholds are ours**; the *states* reached are the engine's. Retail
//!   posts real damage messages with live HP math we have not recovered. [`DamageBands`] makes the
//!   approximation explicit and tunable rather than burying it.

/// Live destruction control — Lua generation for poking a running game's destructibles over the
/// bridge (the console consumes this). See the module for why it lives here.
pub mod live;

use std::collections::HashSet;

use mercs2_core::{Destructible, Entity, Health, ModelRef, World};
use mercs2_formats::orchestrator::{
    damage_messages, enter_commands, machine_node_enable, node_states_for_delivered, HierNode,
    StateMachine,
};

/// Where a model's parsed state machine + HIER come from. Mirrors `mercs2_anim`'s `AnimAssets` seam:
/// the systems take a `&dyn` so the asset store can live in the engine without a leaf→leaf edge.
pub trait DestructionAssets {
    /// The destruction state machine for `model`, or `None` if the model has none (most props).
    fn machine(&self, model: u32) -> Option<&StateMachine>;
    /// The model's HIER node table — `SHOW`/`HIDE` walk its parent links to flip whole subtrees.
    fn hier(&self, model: u32) -> Option<&[HierNode]>;
}

/// The health fractions at which we deliver the machine's message bands.
///
/// **This is our approximation, not the engine's.** Retail drives transitions from real damage
/// messages carrying live HP math; we deliver the `minor` band once health drops below
/// [`damaged_below`](DamageBands::damaged_below) and the `terminal` band at zero. The *states* those
/// messages lead to are the engine's own, read out of the model's machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageBands {
    /// Health fraction under which minor damage (fire, dents, break-parts) shows.
    pub damaged_below: f32,
}

impl Default for DamageBands {
    fn default() -> Self {
        DamageBands { damaged_below: 0.5 }
    }
}

/// A side effect an entered state asked for, which this crate records rather than performs.
///
/// `CreateObject` and `StartEmitter` need to spawn entities and drive the FX system — both outside a
/// leaf crate's reach — so the engine drains these each tick. This is the same
/// record-an-intent-a-system-drains pattern the Lua binding layer already uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestructionIntent {
    /// The entity whose machine asked for this.
    pub entity: Entity,
    /// What to do.
    pub kind: IntentKind,
    /// **Every** pushed argument, in script order — not a single pre-picked field.
    ///
    /// Measured against retail machines (`mercs2_probe --bin seedcmp`, `SEEDCMP_ARGS=1`),
    /// `CreateObject` carries **five** arguments and the first is always the constant `0x1`:
    ///
    /// ```text
    /// ch_veh_tank_ztz98:
    ///   [0x1, 0x0,        0x31eb7ea2, 0x37a605ff, 0x243a5276]
    ///   [0x1, 0x510dcb96, 0x56c61f52, 0x37a605ff, 0xc851b695]
    /// ```
    ///
    /// Slot 3 is constant across a model's spawns (a type/class hash?) while the last slot varies
    /// per spawn, so the last is the best candidate for the spawned template — see
    /// [`template`](DestructionIntent::template). Which slot is authoritative is **not confirmed**,
    /// so the raw list is carried and the caller decides rather than the guess being baked in.
    pub args: Vec<u32>,
}

impl DestructionIntent {
    /// Best current candidate for the spawned template / emitter name-hash: the **last** argument,
    /// which is the only slot observed to vary per spawn. Unconfirmed — see [`args`](Self::args).
    /// Returns `None` for a command that carried no arguments.
    pub fn template(&self) -> Option<u32> {
        self.args.last().copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentKind {
    /// `CreateObject` — spawn debris / a wreck prop. **This is what a shed part becomes**; the wreck
    /// *body* itself is geometry in the container, shown by `DestroyedState` (a retracted claim in
    /// older docs said otherwise — see `vehicle_model_spec.md` §9).
    CreateObject,
    /// `StartEmitter` — fire/smoke on a damaged or destroyed state.
    StartEmitter,
}

fn cmd_create_object() -> u32 {
    mercs2_formats::hash::pandemic_hash_m2("createobject")
}
fn cmd_start_emitter() -> u32 {
    mercs2_formats::hash::pandemic_hash_m2("startemitter")
}

/// Advance every destructible entity from its current [`Health`], returning the side-effect intents
/// produced this tick.
///
/// Entities whose model has no state machine are skipped (they are simply indestructible), as are
/// those whose delivered set did not change — recomputation is not free and the machine only moves
/// when a new message lands.
pub fn destruction_system(
    world: &mut World,
    assets: &dyn DestructionAssets,
    bands: DamageBands,
) -> Vec<DestructionIntent> {
    let mut intents = Vec::new();

    for (entity, (health, model, d)) in
        world.query::<(&Health, &ModelRef, &mut Destructible)>().iter()
    {
        let Some(sm) = assets.machine(model.model) else { continue };
        let (minor, terminal) = damage_messages(sm);

        // Deliver by band. `deliver` dedupes, so the set only ever grows.
        let frac = health.fraction();
        let mut changed = false;
        if frac < bands.damaged_below {
            for m in &minor {
                changed |= d.deliver(*m);
            }
        }
        if health.is_dead() {
            for m in &terminal {
                changed |= d.deliver(*m);
            }
        }
        if !changed && !d.chosen.is_empty() {
            continue;
        }

        let before = d.chosen.clone();
        let delivered: HashSet<u32> = d.delivered.iter().copied().collect();
        d.chosen = node_states_for_delivered(sm, &delivered);

        if let Some(hier) = assets.hier(model.model) {
            d.node_enable = machine_node_enable(sm, hier, &d.chosen);
        }

        // Side effects fire once, on the tick a switch node ENTERS a new state.
        let (co, se) = (cmd_create_object(), cmd_start_emitter());
        for (i, &state_idx) in d.chosen.iter().enumerate() {
            if before.get(i) == Some(&state_idx) {
                continue; // this node did not move
            }
            let Some(state) = sm.nodes.get(i).and_then(|n| n.states.get(state_idx)) else { continue };
            for (cmd, args) in enter_commands(state) {
                let kind = if cmd == co {
                    IntentKind::CreateObject
                } else if cmd == se {
                    IntentKind::StartEmitter
                } else {
                    continue;
                };
                intents.push(DestructionIntent { entity, kind, args });
            }
        }
    }
    intents
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::ModelRef;
    use mercs2_formats::orchestrator::{StateDef, SwitchNodeDef};

    /// The packed-script encoding is `1 <imm>` = push an arg, `2 <cmd>` = invoke with the pushed
    /// args, `3` = end. So `SetStateOnMsg(target, msg)` is `[1 target, 1 msg, 2 cmd, 3]` — the
    /// command hash follows the `2`, it is **not** pushed as an argument.
    fn set_state_on_msg(target: u32, msg: u32) -> Vec<u32> {
        let cmd = mercs2_formats::hash::pandemic_hash_m2("setstateonmsg");
        vec![1, target, 1, msg, 2, cmd, 3]
    }

    struct OneNode(StateMachine);
    impl DestructionAssets for OneNode {
        fn machine(&self, _m: u32) -> Option<&StateMachine> {
            Some(&self.0)
        }
        fn hier(&self, _m: u32) -> Option<&[HierNode]> {
            None // node-enable needs a real HIER; these tests exercise the FSM half
        }
    }

    /// A machine whose pristine state routes a message straight to the wreck state.
    fn wreck_on(msg: u32) -> OneNode {
        use mercs2_formats::orchestrator::{STATE_PRISTINE, STATE_WRECK};
        OneNode(StateMachine {
            switch_slots: vec![0],
            nodes: vec![SwitchNodeDef {
                name_hash: 0xDEAD_BEEF,
                states: vec![
                    StateDef {
                        name_hash: STATE_PRISTINE,
                        enter: set_state_on_msg(STATE_WRECK, msg),
                        exit: vec![],
                    },
                    StateDef { name_hash: STATE_WRECK, enter: vec![], exit: vec![] },
                ],
            }],
        })
    }

    fn spawn(world: &mut World, hp: f32) -> Entity {
        world.spawn((Health::new(hp), ModelRef { model: 7 }, Destructible::default()))
    }

    /// **The invariant this crate exists for.** Damage moves the machine; healing does NOT move it
    /// back. Deriving state from the current health fraction each tick would reattach a shed part.
    #[test]
    fn the_machine_never_walks_backwards_when_health_is_restored() {
        let assets = wreck_on(0xC650_7EE1);
        let mut w = World::new();
        let e = spawn(&mut w, 100.0);
        let bands = DamageBands::default();

        destruction_system(&mut w, &assets, bands);
        let pristine = w.get::<&Destructible>(e).unwrap().chosen.clone();

        w.get::<&mut Health>(e).unwrap().cur = 0.0; // destroyed
        destruction_system(&mut w, &assets, bands);
        let wrecked = w.get::<&Destructible>(e).unwrap().chosen.clone();
        assert_ne!(pristine, wrecked, "zero health must move the machine");

        w.get::<&mut Health>(e).unwrap().cur = 100.0; // fully repaired
        destruction_system(&mut w, &assets, bands);
        let after = w.get::<&Destructible>(e).unwrap();
        assert_eq!(after.chosen, wrecked, "restoring health must NOT un-wreck the machine");
        assert!(after.delivered.contains(&0xC650_7EE1), "delivered set is monotonic");
    }

    /// An undamaged entity settles in its pristine state and stays there.
    #[test]
    fn a_healthy_entity_stays_pristine() {
        let assets = wreck_on(0xC650_7EE1);
        let mut w = World::new();
        let e = spawn(&mut w, 100.0);
        destruction_system(&mut w, &assets, DamageBands::default());
        let d = w.get::<&Destructible>(e).unwrap();
        assert_eq!(d.chosen, vec![0], "pristine is state index 0");
        assert!(d.delivered.is_empty(), "no message delivered at full health");
    }

    /// Clause 3 of the draw gate: an ungoverned node (-1) always draws, and so does everything while
    /// the machine has not run — a pristine object renders before it is ever damaged.
    #[test]
    fn draw_gate_defaults_to_visible() {
        let mut d = Destructible::default();
        assert!(d.draws(-1));
        assert!(d.draws(5), "empty table = machine has not run = draw");
        d.node_enable = vec![true, false, true];
        assert!(d.draws(0));
        assert!(!d.draws(1), "a HIDDEN node must not draw");
        assert!(d.draws(99), "out of range falls back to visible, never panics");
    }

    /// An entered state's `CreateObject` surfaces as an intent carrying **all** its arguments, and
    /// `template()` picks the last. Pinned against the real shape measured on `ch_veh_tank_ztz98`
    /// (`[0x1, 0x0, 0x31eb7ea2, 0x37a605ff, 0x243a5276]`) — the first argument is a constant `0x1`,
    /// so a regression back to `args.first()` would resolve every debris template to 1.
    #[test]
    fn create_object_carries_every_argument_not_just_the_first() {
        use mercs2_formats::orchestrator::{STATE_PRISTINE, STATE_WRECK};
        const MSG: u32 = 0xC650_7EE1;
        const TPL: u32 = 0x243A_5276;
        let co = mercs2_formats::hash::pandemic_hash_m2("createobject");
        // Wreck state spawns debris with the five-argument shape retail uses.
        let wreck_enter = vec![
            1, 0x1u32, 1, 0x0, 1, 0x31EB_7EA2, 1, 0x37A6_05FF, 1, TPL, 2, co, 3,
        ];
        let assets = OneNode(StateMachine {
            switch_slots: vec![0],
            nodes: vec![SwitchNodeDef {
                name_hash: 0xBEEF,
                states: vec![
                    StateDef {
                        name_hash: STATE_PRISTINE,
                        enter: set_state_on_msg(STATE_WRECK, MSG),
                        exit: vec![],
                    },
                    StateDef { name_hash: STATE_WRECK, enter: wreck_enter, exit: vec![] },
                ],
            }],
        });

        let mut w = World::new();
        let e = spawn(&mut w, 0.0); // dead on arrival
        let intents = destruction_system(&mut w, &assets, DamageBands::default());

        let co_intents: Vec<_> =
            intents.iter().filter(|i| i.kind == IntentKind::CreateObject).collect();
        assert_eq!(co_intents.len(), 1, "the wreck state's CreateObject must surface once");
        let it = co_intents[0];
        assert_eq!(it.entity, e);
        assert_eq!(it.args, vec![0x1, 0x0, 0x31EB_7EA2, 0x37A6_05FF, TPL], "all five args carried");
        assert_eq!(it.template(), Some(TPL), "template is the LAST arg, not the leading 0x1");
        assert_ne!(it.template(), Some(1), "regression guard: args.first() is the constant 1");
    }

    /// Entities whose model has no machine are simply indestructible, not a panic or a stall.
    #[test]
    fn a_model_without_a_machine_is_skipped() {
        struct NoMachine;
        impl DestructionAssets for NoMachine {
            fn machine(&self, _m: u32) -> Option<&StateMachine> {
                None
            }
            fn hier(&self, _m: u32) -> Option<&[HierNode]> {
                None
            }
        }
        let mut w = World::new();
        let e = spawn(&mut w, 0.0);
        let intents = destruction_system(&mut w, &NoMachine, DamageBands::default());
        assert!(intents.is_empty());
        assert!(w.get::<&Destructible>(e).unwrap().chosen.is_empty());
    }
}
