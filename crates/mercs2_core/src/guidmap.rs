//! `GuidMap` — the name-hash → `Entity` + guid ↔ `Entity` registry.
//!
//! Models the engine's global **guidmap singleton** (`0x385EA82C`) that `Pg.GetGuidByName` and the
//! `Object.*` bindings resolve against. In the shipped engine, entities register here as they stream in
//! (with their `Name` component) and script spawns mint a handle; a name/GUID lookup then yields a live
//! entity whose position/state is read from its live components — NOT from a side table parsed up front.
//!
//! This crate is deliberately **asset-agnostic** (see `registry.rs`): the map keys on `u32` name-hashes,
//! never hashing strings itself. The caller (`mercs2_game`/`mercs2_formats`) hashes names with
//! `pandemic_hash_m2` at the asset boundary before registering/looking up.

use std::collections::HashMap;

use hecs::Entity;

/// Fixed handle for the local hero character — the `Player.Get{Any,Local,Primary}Character` singleton.
/// Reserved below `FIRST_DYNAMIC_GUID` so a minted handle never collides with it.
pub const HERO_GUID: u64 = 1;
/// Fixed handle for the local player — the `Player.Get{Local,Primary}Player` singleton.
pub const LOCAL_PLAYER_GUID: u64 = 2;
/// First minted dynamic GUID. Matches the script host's historical spawn-guid space so logs/tests that
/// key off concrete handle values stay stable.
pub const FIRST_DYNAMIC_GUID: u64 = 0x1000_0000;

/// Bidirectional registry: `name-hash → Entity`, `guid ↔ Entity`. One instance per world (the engine's
/// resident guidmap). Held behind an `Rc<RefCell<GuidMap>>` shared by the frame loop and the script host.
#[derive(Debug, Default)]
pub struct GuidMap {
    by_name_hash: HashMap<u32, Entity>,
    by_guid: HashMap<u64, Entity>,
    guid_of: HashMap<Entity, u64>,
    next_guid: u64,
}

impl GuidMap {
    /// An empty registry; the next minted GUID is [`FIRST_DYNAMIC_GUID`].
    pub fn new() -> GuidMap {
        GuidMap {
            by_name_hash: HashMap::new(),
            by_guid: HashMap::new(),
            guid_of: HashMap::new(),
            next_guid: FIRST_DYNAMIC_GUID,
        }
    }

    /// Mint a fresh, never-reused GUID.
    pub fn mint(&mut self) -> u64 {
        let g = self.next_guid;
        self.next_guid += 1;
        g
    }

    /// Register `e` under an explicit `guid` (e.g. [`HERO_GUID`], or a `SpawnRequest`'s guid) and,
    /// optionally, a `name_hash`. Overwrites any prior mapping for that guid/name-hash.
    pub fn register(&mut self, e: Entity, name_hash: Option<u32>, guid: u64) {
        self.by_guid.insert(guid, e);
        self.guid_of.insert(e, guid);
        if let Some(h) = name_hash {
            self.by_name_hash.insert(h, e);
        }
    }

    /// Register a named entity, minting a fresh GUID; returns it. Used when the placement/streaming load
    /// creates an entity that carries a `Name` COMP.
    pub fn register_named(&mut self, e: Entity, name_hash: u32) -> u64 {
        let g = self.mint();
        self.register(e, Some(name_hash), g);
        g
    }

    /// The entity a GUID resolves to, if any.
    pub fn entity_by_guid(&self, guid: u64) -> Option<Entity> {
        self.by_guid.get(&guid).copied()
    }

    /// The entity a name-hash resolves to, if any.
    pub fn entity_by_name_hash(&self, name_hash: u32) -> Option<Entity> {
        self.by_name_hash.get(&name_hash).copied()
    }

    /// The GUID assigned to an entity, if it is registered.
    pub fn guid_of(&self, e: Entity) -> Option<u64> {
        self.guid_of.get(&e).copied()
    }

    /// `Pg.GetGuidByName`: the GUID of the entity a name-hash resolves to (name → entity → guid).
    pub fn guid_by_name_hash(&self, name_hash: u32) -> Option<u64> {
        self.by_name_hash
            .get(&name_hash)
            .and_then(|e| self.guid_of.get(e).copied())
    }

    /// Purge an entity from every index — call on despawn (`Object.Remove`, population retire, streaming
    /// hibernate/unload) so a stale `Entity` handle never misroutes a later lookup.
    pub fn unregister(&mut self, e: Entity) {
        if let Some(g) = self.guid_of.remove(&e) {
            self.by_guid.remove(&g);
        }
        self.by_name_hash.retain(|_, v| *v != e);
    }

    /// Number of GUID-registered entities (diagnostics).
    pub fn len(&self) -> usize {
        self.by_guid.len()
    }

    /// Whether the registry holds no entities.
    pub fn is_empty(&self) -> bool {
        self.by_guid.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecs::World;

    #[test]
    fn register_named_round_trips_name_guid_entity() {
        let mut w = World::new();
        let mut gm = GuidMap::new();
        let e = w.spawn((7u32,)); // any entity handle

        let guid = gm.register_named(e, 0x1B2C_8599);
        assert_eq!(guid, FIRST_DYNAMIC_GUID, "first mint is the reserved dynamic base");
        assert_eq!(gm.entity_by_name_hash(0x1B2C_8599), Some(e));
        assert_eq!(gm.entity_by_guid(guid), Some(e));
        assert_eq!(gm.guid_of(e), Some(guid));
        assert_eq!(gm.guid_by_name_hash(0x1B2C_8599), Some(guid), "Pg.GetGuidByName path");
        assert_eq!(gm.guid_by_name_hash(0xDEAD_BEEF), None, "unknown name → no guid");
    }

    #[test]
    fn mint_is_monotonic_and_reserved_handles_are_free() {
        let mut gm = GuidMap::new();
        let a = gm.mint();
        let b = gm.mint();
        assert_eq!((a, b), (FIRST_DYNAMIC_GUID, FIRST_DYNAMIC_GUID + 1));
        assert!(a > HERO_GUID && a > LOCAL_PLAYER_GUID, "dynamic space never hits reserved handles");
    }

    #[test]
    fn explicit_register_supports_reserved_hero_handle() {
        let mut w = World::new();
        let mut gm = GuidMap::new();
        let hero = w.spawn((1u8,));
        gm.register(hero, None, HERO_GUID);
        assert_eq!(gm.entity_by_guid(HERO_GUID), Some(hero));
        assert_eq!(gm.guid_of(hero), Some(HERO_GUID));
    }

    #[test]
    fn unregister_purges_every_index() {
        let mut w = World::new();
        let mut gm = GuidMap::new();
        let e = w.spawn((0u32,));
        let guid = gm.register_named(e, 0xAAAA_1111);
        gm.unregister(e);
        assert_eq!(gm.entity_by_guid(guid), None);
        assert_eq!(gm.entity_by_name_hash(0xAAAA_1111), None);
        assert_eq!(gm.guid_of(e), None);
        assert!(gm.is_empty());
    }
}
