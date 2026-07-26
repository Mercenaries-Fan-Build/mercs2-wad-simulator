//! Engine wiring for [`mercs2_destruction`]: the asset seam, and the World→Scene sync that gets a
//! destruction result onto the draw gate.
//!
//! The split matters. `mercs2_destruction` is a leaf crate — it can compute a node-enable table but
//! it cannot reach [`crate::scene::Scene`], and it must not: the ECS **World is the source of truth**
//! and `Scene` is a render-side cache that mirrors it. So the flow each frame is:
//!
//! ```text
//! destruction_system(world, assets)        // leaf: Health -> messages -> chosen -> Destructible.node_enable
//!     -> sync_destruction_to_scene(...)    // engine: Destructible.node_enable -> Scene.entity_state[e]
//!     -> the draw gate reads RenderState   // clause 3
//! ```
//!
//! The intents the system returns (`CreateObject` debris, `StartEmitter` fire) are *recorded, not
//! performed* by the leaf crate; draining them is the engine's job too.

use std::collections::HashMap;

use mercs2_core::{Destructible, World};
use mercs2_destruction::{DamageBands, DestructionAssets, DestructionIntent};

/// Re-exported so consumers (the game crate, tools) can name the destruction vocabulary through
/// `mercs2_engine::destruction::*` without taking a direct dependency on the leaf crate — the same
/// courtesy the engine already extends for combat/vehicle.
pub use mercs2_destruction::{DamageBands as Bands, DestructionIntent as Intent, IntentKind};
use mercs2_formats::orchestrator::{HierNode, StateMachine};

/// Per-model destruction assets, keyed by model name-hash.
///
/// [`crate::model::Model`] already parses `machine` and `hier` when it loads, but a `Model` is
/// transient — it is flattened into GPU buffers and dropped. This keeps the two pieces the runtime
/// still needs, which is a few hundred bytes per model against re-parsing the resident block.
#[derive(Default)]
pub struct DestructionStore {
    entries: HashMap<u32, (Option<StateMachine>, Vec<HierNode>)>,
}

impl DestructionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a model's machine + HIER. Call this wherever a model is loaded; idempotent per hash.
    pub fn insert(&mut self, model: u32, machine: Option<StateMachine>, hier: Vec<HierNode>) {
        self.entries.entry(model).or_insert((machine, hier));
    }

    /// Take both halves straight off a loaded [`crate::model::Model`].
    pub fn insert_model(&mut self, m: &crate::model::Model) {
        self.insert(m.name_hash, m.machine.clone(), m.hier.clone());
    }

    /// Assemble `model` from the WAD and record its machine + HIER, if not already present.
    ///
    /// The engine's model loaders (`game_world::load_model_by_hash*`) flatten a [`crate::model::Model`]
    /// into GPU buffers and drop it, so the machine is parsed and discarded. Rather than thread an
    /// out-parameter through every call site, a caller that spawns destructible entities calls this
    /// once per model hash. Returns whether the model turned out to have a machine at all.
    pub fn load_from_wad(&mut self, wad: &mut crate::wad::Wad, model: u32) -> bool {
        if let std::collections::hash_map::Entry::Vacant(v) = self.entries.entry(model) {
            match crate::model::Model::load(wad, model) {
                Ok(m) => {
                    v.insert((m.machine, m.hier));
                }
                // A model that will not assemble is not destructible; record the miss so we do not
                // retry the parse every frame.
                Err(_) => {
                    v.insert((None, Vec::new()));
                }
            }
        }
        self.is_destructible(model)
    }

    /// Whether this model is governed by a destruction machine at all (most props are not).
    pub fn is_destructible(&self, model: u32) -> bool {
        self.entries.get(&model).is_some_and(|(m, _)| m.is_some())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl DestructionAssets for DestructionStore {
    fn machine(&self, model: u32) -> Option<&StateMachine> {
        self.entries.get(&model).and_then(|(m, _)| m.as_ref())
    }
    fn hier(&self, model: u32) -> Option<&[HierNode]> {
        self.entries.get(&model).map(|(_, h)| h.as_slice())
    }
}

/// The slice of the render side the destruction sync needs: per-entity render state, get and set.
///
/// Narrow on purpose. [`crate::scene::Scene`] owns a GPU device, so a function taking `&mut Scene`
/// cannot be tested without one; taking this instead makes the sync a pure data operation with a
/// two-line fake in tests, and `Scene` satisfies it with the accessors it already had.
pub trait EntityRenderStates {
    fn render_state(&self, e: mercs2_core::Entity) -> Option<&crate::render_state::RenderState>;
    fn set_render_state(&mut self, e: mercs2_core::Entity, rs: crate::render_state::RenderState);
}

impl EntityRenderStates for crate::scene::Scene {
    fn render_state(&self, e: mercs2_core::Entity) -> Option<&crate::render_state::RenderState> {
        self.entity_render_state(e)
    }
    fn set_render_state(&mut self, e: mercs2_core::Entity, rs: crate::render_state::RenderState) {
        self.set_entity_render_state(e, rs);
    }
}

/// Copy every entity's destruction node-enable table into its render state, so the draw gate
/// (clause 3) sees it.
///
/// One-directional by design: the World is authoritative, the render side mirrors. Entities whose
/// machine has not run yet carry an empty table, which the gate already reads as "everything draws" —
/// so a pristine object needs no special case. Returns how many entities were synced.
pub fn sync_destruction_to_scene(world: &World, scene: &mut dyn EntityRenderStates) -> usize {
    let mut n = 0;
    for (e, d) in world.query::<&Destructible>().iter() {
        if d.node_enable.is_empty() {
            continue;
        }
        let mut rs = scene.render_state(e).cloned().unwrap_or_default();
        if rs.node_enable != d.node_enable {
            rs.node_enable = d.node_enable.clone();
            scene.set_render_state(e, rs);
        }
        n += 1;
    }
    n
}

/// Run destruction for the frame and mirror the result onto the scene, returning the side-effect
/// intents for the caller to drain.
pub fn destruction_frame(
    world: &mut World,
    scene: &mut dyn EntityRenderStates,
    store: &DestructionStore,
    bands: DamageBands,
) -> Vec<DestructionIntent> {
    let intents = mercs2_destruction::destruction_system(world, store, bands);
    sync_destruction_to_scene(world, scene);
    intents
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::{Health, ModelRef};
    use mercs2_formats::orchestrator::{StateDef, SwitchNodeDef, STATE_PRISTINE, STATE_WRECK};

    /// `1 <imm>` push · `2 <cmd>` invoke · `3` end.
    fn set_state_on_msg(target: u32, msg: u32) -> Vec<u32> {
        let cmd = mercs2_formats::hash::pandemic_hash_m2("setstateonmsg");
        vec![1, target, 1, msg, 2, cmd, 3]
    }

    /// A one-switch-node machine: pristine routes `DamageMsg` to the wreck state, and the wreck state
    /// HIDEs the switch node's own subtree (which is what makes the enable table change).
    fn store_with_machine(model: u32) -> DestructionStore {
        let hide = mercs2_formats::hash::pandemic_hash_m2("hide");
        let enter = set_state_on_msg(STATE_WRECK, 0xC650_7EE1);
        let wreck_enter = vec![1u32, 0xAAAA_0001, 2, hide, 3];
        let sm = StateMachine {
            switch_slots: vec![0],
            nodes: vec![SwitchNodeDef {
                name_hash: 0xAAAA_0001,
                states: vec![
                    StateDef { name_hash: STATE_PRISTINE, enter, exit: vec![] },
                    StateDef { name_hash: STATE_WRECK, enter: wreck_enter, exit: vec![] },
                ],
            }],
        };
        let hier = vec![
            HierNode { index: 0, hash: 0xAAAA_0001, parent: None, local: [0.0; 16],
                       bbox_min: [0.0; 3], bbox_max: [0.0; 3] },
        ];
        let mut s = DestructionStore::new();
        s.insert(model, Some(sm), hier);
        s
    }


    /// A two-line stand-in for the render side, so the sync is testable without a GPU.
    #[derive(Default)]
    struct FakeStates(std::collections::HashMap<mercs2_core::Entity, crate::render_state::RenderState>);
    impl EntityRenderStates for FakeStates {
        fn render_state(&self, e: mercs2_core::Entity) -> Option<&crate::render_state::RenderState> {
            self.0.get(&e)
        }
        fn set_render_state(&mut self, e: mercs2_core::Entity, rs: crate::render_state::RenderState) {
            self.0.insert(e, rs);
        }
    }

    /// The sync copies the World's table onto the render side, leaves the other render fields alone,
    /// skips entities whose machine has not run, and is idempotent.
    #[test]
    fn sync_mirrors_the_world_table_without_clobbering_lod() {
        let mut w = World::new();
        let ran = w.spawn((Destructible {
            delivered: vec![1],
            chosen: vec![1],
            node_enable: vec![true, false],
        },));
        let untouched = w.spawn((Destructible::default(),)); // machine never ran: empty table

        let mut st = FakeStates::default();
        // Pre-existing LOD state the sync must preserve.
        st.set_render_state(
            ran,
            crate::render_state::RenderState { lod: 2, view_state: 0x04, node_enable: vec![] },
        );

        assert_eq!(sync_destruction_to_scene(&w, &mut st), 1, "only the entity with a table syncs");
        let rs = st.render_state(ran).unwrap();
        assert_eq!(rs.node_enable, vec![true, false], "the table is mirrored");
        assert_eq!((rs.lod, rs.view_state), (2, 0x04), "LOD/view_state must survive the sync");
        assert!(st.render_state(untouched).is_none(), "an un-run machine writes nothing");

        // Idempotent: running again changes nothing and still reports the same count.
        assert_eq!(sync_destruction_to_scene(&w, &mut st), 1);
        assert_eq!(st.render_state(ran).unwrap().node_enable, vec![true, false]);
    }

    #[test]
    fn a_model_with_no_machine_is_not_destructible() {
        let mut s = DestructionStore::new();
        s.insert(9, None, Vec::new());
        assert!(!s.is_destructible(9));
        assert!(s.machine(9).is_none());
        assert!(s.hier(9).is_some(), "hier is still served even without a machine");
    }

    /// **End to end.** A health-bearing entity with a governed model starts fully visible; once it
    /// dies the machine hides the governed subtree, and that reaches the *draw gate* — not just the
    /// component. This is the test that proves the wiring, since it asserts on
    /// `RenderState::node_visible`, the same predicate the renderer calls.
    #[test]
    fn killing_an_entity_hides_its_geometry_at_the_draw_gate() {
        let store = store_with_machine(42);
        let mut w = World::new();
        let e = w.spawn((Health::new(100.0), ModelRef { model: 42 }, Destructible::default()));
        let bands = DamageBands::default();

        // Pristine: the machine has not moved, so nothing is hidden.
        mercs2_destruction::destruction_system(&mut w, &store, bands);
        let d = w.get::<&Destructible>(e).unwrap();
        assert!(d.draws(0), "a pristine object must draw");
        drop(d);

        // Dead: the wreck state's HIDE runs over the HIER.
        w.get::<&mut Health>(e).unwrap().cur = 0.0;
        mercs2_destruction::destruction_system(&mut w, &store, bands);
        let d = w.get::<&Destructible>(e).unwrap();
        assert!(!d.node_enable.is_empty(), "the machine must have produced a table");
        assert!(!d.draws(0), "the governed subtree must be hidden once destroyed");

        // And the same table, read through the engine's gate predicate, agrees.
        let rs = crate::render_state::RenderState {
            lod: 0,
            view_state: 0x01,
            node_enable: d.node_enable.clone(),
        };
        assert!(!rs.node_visible(0), "the draw gate must hide it too");
        assert!(rs.node_visible(-1), "an ungoverned segment still draws");
    }

    #[test]
    fn the_store_is_idempotent_per_hash() {
        let mut s = store_with_machine(4);
        assert_eq!(s.len(), 1);
        s.insert(4, None, Vec::new());
        assert!(s.is_destructible(4), "re-inserting must not clobber the machine");
        assert_eq!(s.len(), 1);
    }
}
