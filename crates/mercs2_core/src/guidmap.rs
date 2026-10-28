//! `GuidMap` — the entity-key world a script guid addresses
//! (`docs/reverse_engineer/object_entity_core_code_map.md` §4.0).
//!
//! A script guid is the entity's u32 key: a placed entity's WAD key, a template's bit-31 key, or a
//! runtime key (`0x4000xxxx`) for anything spawned. This registry holds what retail resolves a guid
//! against:
//! - the `Name` container's name index (`0xDF6B88` index 2): `pandemic_hash_m2(name)` → key, the
//!   earliest registration winning (`Pg.GetGuidByName`, `0x5D3220`);
//! - the SceneObject rows (`0x17C02D8`): each live key's transform, its template alias
//!   (`Transform.Parent`) and its activation state (`+0x18` bits 12..13);
//! - the ECS entity a key is realized as, when it has one;
//! - the runtime key counter.
//!
//! Asset-agnostic: names arrive as hashes (the caller hashes at the asset boundary).

use std::collections::HashMap;

use hecs::Entity;

/// The class every runtime key is in (`Object.Remove` tests `(g & 0xF0000000) == 0x40000000`,
/// `0x5CDCB6`), and the allocator's first key: the counter `[0xD8A42C]` starts at `0x40000000` in the
/// static image and every allocation post-increments it by one, with no free list (`0x6746D0`).
pub const RUNTIME_KEY_BASE: u32 = 0x4000_0000;

/// One SceneObject row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneRow {
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    /// `+0x14`: the template it was placed or spawned from, 0 for none.
    pub template: u32,
    /// `+0x18` bits 12..13: 0 asleep, 1 waking, 2 awake.
    pub state: u8,
}

/// A layer record (`docs/reverse_engineer/layer_load_code_map.md` §1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerRecord {
    /// `+0x14`: 0 unloaded, 2 loaded, 3 failed.
    pub status: u16,
    /// `+0x18` bit0: loaded with `Pg.LoadLayer(name, true)`.
    pub dynamic: bool,
    /// `+0x18` bit1: a reload is in flight.
    pub reloading: bool,
    /// `+0x18` bit2: loaded as a static layer.
    pub statik: bool,
    /// The reference count `Pg.LoadLayer` adds to and `Pg.UnloadLayer` releases.
    pub refs: u16,
    /// The keys its container placed.
    pub keys: Vec<u32>,
}

/// A layer operation the streaming executor performs (`Pg.LoadLayer` / `UnloadLayer` / `ReloadLayer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerOp {
    Load,
    Unload,
    Reload,
}

/// The entity-key registry. One instance per world, shared (`Rc<RefCell<GuidMap>>`) by the frame loop,
/// the streaming executor and the script host.
#[derive(Debug)]
pub struct GuidMap {
    names: HashMap<u32, u32>,
    names_of: HashMap<u32, Vec<u32>>,
    scene: HashMap<u32, SceneRow>,
    entity: HashMap<u32, Entity>,
    key_of: HashMap<Entity, u32>,
    next_runtime: u32,
    layers: HashMap<u32, LayerRecord>,
    /// RuntimeLayerId (`0x017BF478`): the dynamic layer that placed a key.
    runtime_layer: HashMap<u32, u32>,
    layer_ops: Vec<(LayerOp, u32)>,
    /// `Pg.UnloadingStaticLayers` (`[0x1175F58]`).
    pub unloading_static_layers: bool,
    /// The `LandingZone` rows: key → (zone, player slot).
    landing_zones: HashMap<u32, (u32, u32)>,
    /// Rows a SetTransform changed whose grid re-cell is pending.
    moved: Vec<u32>,
}

impl Default for GuidMap {
    fn default() -> Self {
        GuidMap::new()
    }
}

impl GuidMap {
    pub fn new() -> GuidMap {
        GuidMap {
            names: HashMap::new(),
            names_of: HashMap::new(),
            scene: HashMap::new(),
            entity: HashMap::new(),
            key_of: HashMap::new(),
            next_runtime: RUNTIME_KEY_BASE,
            layers: HashMap::new(),
            runtime_layer: HashMap::new(),
            layer_ops: Vec::new(),
            unloading_static_layers: false,
            landing_zones: HashMap::new(),
            moved: Vec::new(),
        }
    }

    /// A layer request (`docs/reverse_engineer/layer_load_code_map.md` §2–3). Returns false when it is
    /// refused (retail pushes nil and tracks nothing).
    /// - Load adds a reference, marks the record dynamic or static, and queues the load.
    /// - Unload is refused for a layer that is not dynamic unless static layers are being torn down
    ///   (`0x5D4F37`); it releases a reference and queues the unload when none remain.
    /// - Reload is refused for a layer that is not dynamic; a missing or unreferenced record becomes a
    ///   load (`0x5A2590`), otherwise the reload is marked in flight and queued.
    pub fn request_layer(&mut self, op: LayerOp, layer: u32, dynamic: bool) -> bool {
        match op {
            LayerOp::Load => {
                let r = self.layers.entry(layer).or_default();
                r.refs += 1;
                if dynamic {
                    r.dynamic = true;
                } else {
                    r.statik = true;
                }
                self.layer_ops.push((LayerOp::Load, layer));
            }
            LayerOp::Unload => {
                let dynamic = self.layers.get(&layer).is_some_and(|r| r.dynamic);
                if !dynamic && !self.unloading_static_layers {
                    return false;
                }
                if let Some(r) = self.layers.get_mut(&layer) {
                    r.refs = r.refs.saturating_sub(1);
                    if r.refs == 0 {
                        self.layer_ops.push((LayerOp::Unload, layer));
                    }
                }
            }
            LayerOp::Reload => {
                let Some(r) = self.layers.get_mut(&layer) else { return false };
                if !r.dynamic {
                    return false;
                }
                if r.refs == 0 {
                    return self.request_layer(LayerOp::Load, layer, true);
                }
                r.reloading = true;
                self.layer_ops.push((LayerOp::Reload, layer));
            }
        }
        true
    }

    /// Whether a request of `op` on `layer` has completed, and with what `bSuccess` (`FUN_005A28F0`);
    /// `None` while pending.
    /// - Load: at status 2 or 4 (true) or 3 (false).
    /// - Unload: true once the record is gone or unreferenced; false while another holder references it.
    /// - Reload: at status 2 or 4 with the reload no longer in flight (true), or 3 (false).
    pub fn layer_request_status(&self, op: LayerOp, layer: u32) -> Option<bool> {
        let r = self.layers.get(&layer);
        match op {
            LayerOp::Unload => Some(r.is_none_or(|r| r.refs == 0)),
            LayerOp::Load | LayerOp::Reload => {
                let r = r?;
                match r.status {
                    3 => Some(false),
                    2 | 4 if !(op == LayerOp::Reload && r.reloading) => Some(true),
                    _ => None,
                }
            }
        }
    }

    /// Record `key`'s `LandingZone` row.
    pub fn set_landing_zone(&mut self, key: u32, zone: u32, slot: u32) {
        self.landing_zones.insert(key, (zone, slot));
    }

    /// Every `LandingZone` row as `(zone, slot, key)`, sorted.
    pub fn landing_zones(&self) -> Vec<(u32, u32, u32)> {
        let mut v: Vec<(u32, u32, u32)> = self.landing_zones.iter().map(|(&k, &(z, s))| (z, s, k)).collect();
        v.sort_unstable();
        v
    }

    /// Drain the queued layer operations.
    pub fn take_layer_ops(&mut self) -> Vec<(LayerOp, u32)> {
        std::mem::take(&mut self.layer_ops)
    }

    pub fn layer(&self, layer: u32) -> Option<&LayerRecord> {
        self.layers.get(&layer)
    }

    pub fn layer_mut(&mut self, layer: u32) -> &mut LayerRecord {
        self.layers.entry(layer).or_default()
    }

    /// Record that a dynamic layer placed `key` (set-or-overwrite, `FUN_0064A600`).
    pub fn set_runtime_layer(&mut self, key: u32, layer: u32) {
        self.runtime_layer.insert(key, layer);
    }

    /// The dynamic layer that placed `key`, if any.
    pub fn runtime_layer(&self, key: u32) -> Option<u32> {
        self.runtime_layer.get(&key).copied()
    }

    /// Every key a dynamic layer placed.
    pub fn keys_of_runtime_layer(&self, layer: u32) -> Vec<u32> {
        self.runtime_layer.iter().filter(|(_, &l)| l == layer).map(|(&k, _)| k).collect()
    }

    /// A fresh runtime key (never reused): the counter's value, then the counter steps by one.
    pub fn mint_runtime(&mut self) -> u32 {
        let k = self.next_runtime;
        self.next_runtime += 1;
        k
    }

    /// Register `name_hash` → `key` in the name index. An existing registration of the hash stays (the
    /// probe returns the earliest).
    pub fn register_name(&mut self, name_hash: u32, key: u32) {
        if let std::collections::hash_map::Entry::Vacant(v) = self.names.entry(name_hash) {
            v.insert(key);
            self.names_of.entry(key).or_default().push(name_hash);
        }
    }

    /// `Pg.GetGuidByName`: the key `name_hash` names, if live.
    pub fn key_by_name_hash(&self, name_hash: u32) -> Option<u32> {
        self.names.get(&name_hash).copied()
    }

    /// Add or replace `key`'s SceneObject row.
    pub fn set_scene(&mut self, key: u32, row: SceneRow) {
        self.scene.insert(key, row);
    }

    /// `key`'s SceneObject row, if live.
    pub fn scene(&self, key: u32) -> Option<&SceneRow> {
        self.scene.get(&key)
    }

    /// SetTransform (`0x6658B0`) on a live row: write the transform and queue the grid re-cell its
    /// change listener runs (`0x518140`), which the streaming executor performs at its next step.
    /// False without a row.
    pub fn set_transform(&mut self, key: u32, pos: [f32; 3], quat: [f32; 4]) -> bool {
        let Some(r) = self.scene.get_mut(&key) else { return false };
        r.pos = pos;
        r.quat = quat;
        if !self.moved.contains(&key) {
            self.moved.push(key);
        }
        true
    }

    /// Drain the keys [`set_transform`](Self::set_transform) moved, in first-move order.
    pub fn take_moved(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.moved)
    }

    /// Set `key`'s activation state (it must have a row).
    pub fn set_state(&mut self, key: u32, state: u8) {
        if let Some(r) = self.scene.get_mut(&key) {
            r.state = state;
        }
    }

    /// `key`'s activation state; `None` without a SceneObject row (every template key).
    pub fn state(&self, key: u32) -> Option<u8> {
        self.scene.get(&key).map(|r| r.state)
    }

    /// Realize `key` as ECS entity `e`.
    pub fn bind(&mut self, key: u32, e: Entity) {
        if let Some(old) = self.entity.insert(key, e) {
            self.key_of.remove(&old);
        }
        self.key_of.insert(e, key);
    }

    /// Drop `e`'s link to its key (the entity despawned; the key and its row stay).
    pub fn unbind(&mut self, e: Entity) {
        if let Some(k) = self.key_of.remove(&e) {
            self.entity.remove(&k);
        }
    }

    /// The ECS entity `key` is realized as, if any.
    pub fn entity_of(&self, key: u32) -> Option<Entity> {
        self.entity.get(&key).copied()
    }

    /// The key an ECS entity realizes, if any.
    pub fn key_of(&self, e: Entity) -> Option<u32> {
        self.key_of.get(&e).copied()
    }

    /// Remove `key` entirely: its row, its names, its layer link and its entity link.
    pub fn remove(&mut self, key: u32) {
        self.scene.remove(&key);
        self.moved.retain(|&k| k != key);
        self.runtime_layer.remove(&key);
        self.landing_zones.remove(&key);
        if let Some(hs) = self.names_of.remove(&key) {
            for h in hs {
                self.names.remove(&h);
            }
        }
        if let Some(e) = self.entity.remove(&key) {
            self.key_of.remove(&e);
        }
    }

    /// Live SceneObject rows (diagnostics).
    pub fn len(&self) -> usize {
        self.scene.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scene.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecs::World;

    fn row(state: u8) -> SceneRow {
        SceneRow { pos: [1.0, 2.0, 3.0], quat: [0.0, 0.0, 0.0, 1.0], template: 0x8000_4385, state }
    }

    /// The earliest registration of a name wins; a removed key frees its names.
    #[test]
    fn names_resolve_first_registration() {
        let mut gm = GuidMap::new();
        gm.register_name(0x1B2C_8599, 0x000D_39FD);
        gm.register_name(0x1B2C_8599, 0x000A_1B2C);
        assert_eq!(gm.key_by_name_hash(0x1B2C_8599), Some(0x000D_39FD));
        assert_eq!(gm.key_by_name_hash(0xDEAD_BEEF), None);
        gm.remove(0x000D_39FD);
        assert_eq!(gm.key_by_name_hash(0x1B2C_8599), None, "the later registration was never stored");
    }

    /// A key's state lives on its SceneObject row; a key without one (a template) has none.
    #[test]
    fn state_needs_a_scene_row() {
        let mut gm = GuidMap::new();
        gm.set_scene(0x000A_3B2E, row(0));
        gm.set_state(0x000A_3B2E, 2);
        assert_eq!(gm.state(0x000A_3B2E), Some(2));
        assert_eq!(gm.state(0x8000_4385), None);
    }

    /// A load adds a reference and marks the record; unloading a static layer is refused unless static
    /// layers are being torn down.
    #[test]
    fn layer_requests_and_the_static_guard() {
        let mut gm = GuidMap::new();
        assert!(gm.request_layer(LayerOp::Load, 7, false));
        assert!(gm.request_layer(LayerOp::Load, 9, true));
        assert_eq!((gm.layer(7).unwrap().refs, gm.layer(7).unwrap().statik), (1, true));
        assert!(!gm.request_layer(LayerOp::Unload, 7, false), "static: refused");
        assert!(!gm.request_layer(LayerOp::Reload, 7, false), "static: refused");
        assert!(gm.request_layer(LayerOp::Unload, 9, false), "dynamic: queued");
        gm.unloading_static_layers = true;
        assert!(gm.request_layer(LayerOp::Unload, 7, false));
        assert_eq!(gm.take_layer_ops(), vec![(LayerOp::Load, 7), (LayerOp::Load, 9), (LayerOp::Unload, 9), (LayerOp::Unload, 7)]);
    }

    /// Completion follows the record: a load waits for status 2/3/4, an unload holds false while
    /// another reference remains, a reload of an unreferenced layer is a load.
    #[test]
    fn layer_requests_complete_by_status() {
        let mut gm = GuidMap::new();
        gm.request_layer(LayerOp::Load, 5, true);
        gm.request_layer(LayerOp::Load, 5, true);
        assert_eq!(gm.layer_request_status(LayerOp::Load, 5), None, "not loaded yet");
        gm.layer_mut(5).status = 2;
        assert_eq!(gm.layer_request_status(LayerOp::Load, 5), Some(true));
        gm.layer_mut(6).status = 3;
        assert_eq!(gm.layer_request_status(LayerOp::Load, 6), Some(false), "failed");
        gm.take_layer_ops();
        assert!(gm.request_layer(LayerOp::Unload, 5, false));
        assert!(gm.take_layer_ops().is_empty(), "one reference remains: nothing to unload");
        assert_eq!(gm.layer_request_status(LayerOp::Unload, 5), Some(false));
        assert!(gm.request_layer(LayerOp::Reload, 5, false));
        assert_eq!(gm.layer_request_status(LayerOp::Reload, 5), None, "in flight");
        gm.layer_mut(5).reloading = false;
        assert_eq!(gm.layer_request_status(LayerOp::Reload, 5), Some(true));
        assert!(gm.request_layer(LayerOp::Unload, 5, false));
        assert_eq!(gm.layer_request_status(LayerOp::Unload, 5), Some(true));
        assert_eq!(gm.take_layer_ops(), vec![(LayerOp::Reload, 5), (LayerOp::Unload, 5)]);
        gm.layer_mut(5).status = 0;
        assert!(gm.request_layer(LayerOp::Reload, 5, false), "unreferenced: becomes a load");
        assert_eq!(gm.take_layer_ops(), vec![(LayerOp::Load, 5)]);
    }

    /// SetTransform writes a live row and queues its re-cell once per key; a key without a row, or one
    /// removed before the executor runs, queues nothing.
    #[test]
    fn set_transform_queues_one_recell_per_row() {
        let mut gm = GuidMap::new();
        gm.set_scene(0x4000_0002, row(0));
        let q = [0.0, 0.707, 0.0, 0.707];
        assert!(gm.set_transform(0x4000_0002, [4.0, 5.0, 6.0], q));
        assert!(gm.set_transform(0x4000_0002, [7.0, 8.0, 9.0], q));
        assert!(!gm.set_transform(0x4000_0003, [0.0; 3], q), "no row");
        assert_eq!((gm.scene(0x4000_0002).unwrap().pos, gm.scene(0x4000_0002).unwrap().quat), ([7.0, 8.0, 9.0], q));
        assert_eq!(gm.take_moved(), vec![0x4000_0002]);
        gm.set_transform(0x4000_0002, [0.0; 3], q);
        gm.remove(0x4000_0002);
        assert!(gm.take_moved().is_empty());
    }

    /// Runtime keys are in the 0x40000000 class and never repeat; an entity link is per key.
    #[test]
    fn runtime_keys_and_entity_links() {
        let mut w = World::new();
        let mut gm = GuidMap::new();
        let (a, b) = (gm.mint_runtime(), gm.mint_runtime());
        assert_eq!((a, b), (RUNTIME_KEY_BASE, RUNTIME_KEY_BASE + 1), "post-increment from the base");
        let e = w.spawn((1u8,));
        gm.bind(a, e);
        assert_eq!((gm.entity_of(a), gm.key_of(e)), (Some(e), Some(a)));
        gm.unbind(e);
        assert_eq!((gm.entity_of(a), gm.key_of(e)), (None, None));
    }
}
