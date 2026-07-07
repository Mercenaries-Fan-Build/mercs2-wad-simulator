//! Keystone C — the master frame spine.
//!
//! The modern analog of the shipped engine's per-frame driver, recovered in
//! `docs/reverse_engineer/scheduler_tick_code_map.md`:
//!
//! * **RunFrame `FUN_00630ef0`** — the 9-stage per-frame order (QPC dt → device-reinit gate →
//!   timestep compute → fixed accumulator drain → master update → render → frame-time end →
//!   vsync/cap → present). The host loop (winit) executes those stages in order; this module owns the
//!   two pieces that are engine policy rather than platform glue: the fixed-sim [`crate::Time`]
//!   accumulator (stages 3–4) and the [`LayerStack`] master update (stage 5).
//! * **The master tick `FUN_004c14f0 → FUN_004c15e0`** — a 5-slot application-layer stack ticked in
//!   fixed index order 0→4. `cur` boots at 0 and climbs to `target` (= 4, the in-game/ECS mode),
//!   firing an enter transition per slot; the top layer's `Update` ticks the gameplay systems.
//!
//! [`LayerStack`] is deliberately **index-only**: the per-layer `Update(dt)` bodies live in the host
//! loop (they need render / asset / streaming state that this renderer-agnostic crate does not own),
//! selected on [`LayerStack::active`]. This crate owns the *order and cadence*; the host owns the
//! *bodies* — exactly the split the exe has between RunFrame's C order and the vtable-dispatched
//! per-system work.

/// Size of the engine's application-layer stack (recovered `DAT_017bbcf4 = 5`, init `FUN_004c1170`).
pub const LAYER_COUNT: usize = 5;

/// The in-game / ECS mode — the stack's default climb target (`DAT_017bbcfc = 4`). Its `Update`
/// (`vtable +0xc`) ticks the gameplay systems (Camera / Animation / Vehicle / AI / Population …).
pub const LAYER_GAME: usize = LAYER_COUNT - 1;

/// A transition fired as the stack's `cur` moves toward `target` — the engine's vtable enter hooks
/// (`+4` ascending / `+8` descending). The payload is the layer index just entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerTransition {
    /// Climbed up into a higher layer (e.g. loading → game): the engine's `vtable +4`.
    Ascending(usize),
    /// Dropped back down to a lower layer (e.g. game → frontend/pause): the engine's `vtable +8`.
    Descending(usize),
}

/// The master tick's 5-slot application-layer stack (`FUN_004c15e0`).
///
/// `cur` moves one slot per [`advance`](LayerStack::advance) toward `target`, emitting the enter
/// transition for the slot just entered — faithful to the recovered init (`FUN_004c1170`: boots at 0,
/// target 4) and the hardcoded 0→4 climb (not registration order, not a sorted priority).
#[derive(Clone, Copy, Debug)]
pub struct LayerStack {
    cur: usize,
    target: usize,
}

impl LayerStack {
    /// Boot at layer 0 with the in-game layer (4) as the eventual target — the recovered init order.
    pub fn booting() -> Self {
        Self { cur: 0, target: LAYER_GAME }
    }

    /// Start settled at a specific layer (clamped into the stack). Used when the host boots straight
    /// into e.g. a loading layer that later raises its `target` to [`LAYER_GAME`].
    pub fn at(layer: usize) -> Self {
        let l = layer.min(LAYER_COUNT - 1);
        Self { cur: l, target: l }
    }

    /// The layer the stack is currently in — the host selects its per-frame `Update` body on this.
    pub fn active(&self) -> usize {
        self.cur
    }

    /// The layer the stack is climbing toward.
    pub fn target(&self) -> usize {
        self.target
    }

    /// `true` when `cur == target` (no pending transition).
    pub fn settled(&self) -> bool {
        self.cur == self.target
    }

    /// Raise / lower the layer the stack climbs toward (clamped to the 5-slot stack). The stack then
    /// reaches it one slot at a time via [`advance`](LayerStack::advance).
    pub fn set_target(&mut self, target: usize) {
        self.target = target.min(LAYER_COUNT - 1);
    }

    /// Move `cur` one slot toward `target`, returning the transition entered (`None` if already
    /// settled). Drive it in a `while let`/`while !settled()` loop to reach the target in one frame,
    /// or once per frame to climb gradually.
    pub fn advance(&mut self) -> Option<LayerTransition> {
        use std::cmp::Ordering::*;
        match self.cur.cmp(&self.target) {
            Less => {
                self.cur += 1;
                Some(LayerTransition::Ascending(self.cur))
            }
            Greater => {
                self.cur -= 1;
                Some(LayerTransition::Descending(self.cur))
            }
            Equal => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boots_at_zero_and_climbs_to_game_layer() {
        let mut stack = LayerStack::booting();
        assert_eq!(stack.active(), 0);
        assert_eq!(stack.target(), LAYER_GAME);
        // One Ascending transition per slot, in order, until settled at 4.
        let mut entered = Vec::new();
        while let Some(t) = stack.advance() {
            entered.push(t);
        }
        assert_eq!(
            entered,
            vec![
                LayerTransition::Ascending(1),
                LayerTransition::Ascending(2),
                LayerTransition::Ascending(3),
                LayerTransition::Ascending(LAYER_GAME),
            ]
        );
        assert!(stack.settled());
        assert_eq!(stack.active(), LAYER_GAME);
        // Settled: further advances are no-ops.
        assert_eq!(stack.advance(), None);
    }

    #[test]
    fn loading_layer_raises_target_and_enters_game_once() {
        // The streaming host's pattern: settle on a loading layer, then raise the target on loader
        // completion and enter GAME exactly once.
        let mut stack = LayerStack::at(3);
        assert!(stack.settled());
        assert_eq!(stack.advance(), None); // nothing to do while loading
        stack.set_target(LAYER_GAME);
        assert_eq!(stack.advance(), Some(LayerTransition::Ascending(LAYER_GAME)));
        assert_eq!(stack.advance(), None);
        assert_eq!(stack.active(), LAYER_GAME);
    }

    #[test]
    fn descending_fires_when_target_drops() {
        let mut stack = LayerStack::at(LAYER_GAME);
        stack.set_target(2);
        assert_eq!(stack.advance(), Some(LayerTransition::Descending(3)));
        assert_eq!(stack.advance(), Some(LayerTransition::Descending(2)));
        assert_eq!(stack.advance(), None);
    }

    #[test]
    fn set_target_is_clamped_to_the_stack() {
        let mut stack = LayerStack::at(0);
        stack.set_target(99);
        assert_eq!(stack.target(), LAYER_COUNT - 1);
    }
}
