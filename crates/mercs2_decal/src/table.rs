//! The `decaltable` — the resident decal-material definition table (code map §1).
//!
//! Recovered as an ASET resident singleton, type-class hash **`0x3B0AABF8`** ("decaltable"). The
//! big ASET registrar `FUN_004bef00` registers it; its `GetTypeHash` vfn (`FUN_004cb1b0`) returns
//! `0x3b0aabf8`, and its instance resolver (`FUN_004cb1f0`) allocates a **`0x400`-byte** resident
//! block via `FUN_008242b0(0x400)` and stamps the resident flag `|0x4000` at `obj+0x16`. That block
//! **is** `PgDecalTable` (`.data @0x9288b8`): the array of decal-material definitions — bullet holes,
//! blood, scorch, tire tracks — carrying, per code map §1, a **texture handle / size / lifetime /
//! super flag** each.
//!
//! **Boundary (honest):** the table is read via computed offsets inside stripped functions, never by
//! name, so the *numeric* per-type values (exact texture handle, size, lifetime) are `confirm-live`
//! data, not statically recovered. This module therefore models the table as **loadable data +
//! lookup**: the recovered *layout* (the four field columns) and the recovered *category set* are
//! encoded; the numbers are fields a loader fills. `DecalTable::stock()` seeds the recovered
//! categories with neutral, clearly-marked placeholder parameters so the mechanism is exercisable —
//! it does **not** claim those numbers are the retail values.

use mercs2_formats::hash::pandemic_hash_m2;

/// `decaltable` ASET type-class hash — `FUN_004cb1b0` returns this (code map §1/§5).
pub const DECALTABLE_TYPE_HASH: u32 = 0x3B0A_ABF8;

/// Resident-block allocation size the resolver requests: `FUN_008242b0(0x400)` (code map §1).
pub const DECALTABLE_RESIDENT_ALLOC: usize = 0x400;

/// Resident flag OR'd into `obj+0x16` by `FUN_004cb1f0` marking the table a resident singleton.
pub const DECALTABLE_RESIDENT_FLAG: u16 = 0x4000;

/// `PgDecalTable` static-data address in the unpacked image (`.data @0x9288b8`) — for corpus x-ref.
pub const DECALTABLE_DATA_ADDR: u32 = 0x0092_88b8;

/// Recovered decal-material param bind-slot names (code map §2): the `decalNormal` (normal map) and
/// `decalParam` (param map) material slots the decal shader samples. Their `.data` string addresses
/// are recorded for corpus x-ref; the maps themselves are **data-only bind slots** (not code).
pub const DECAL_NORMAL_PARAM: &str = "decalNormal";
/// `.data` address of the `decalNormal` param string.
pub const DECAL_NORMAL_PARAM_ADDR: u32 = 0x00ba_c5d4;
/// The `decalParam` (param map) material bind slot.
pub const DECAL_PARAM_PARAM: &str = "decalParam";
/// `.data` address of the `decalParam` param string.
pub const DECAL_PARAM_PARAM_ADDR: u32 = 0x00ba_c5f0;

/// The recovered decal categories named in the `PgDecalTable` comment (code map §1) plus the
/// `DamageShadow` projected decal (§4 — grouped in the decal `.rdata` cluster, a scorch/damage
/// darkening projection, **not** a shadow-map pass).
///
/// The engine addresses table rows by hash, not by this enum; the enum is the reimpl's legible
/// handle onto the recovered set. Each variant's [`canonical_name`](DecalType::canonical_name)
/// hashes (via `pandemic_hash_m2`) to the row key — exactly how the engine keys a decal material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecalType {
    /// Bullet-hole impact decal (weapon hit on a surface).
    BulletHole,
    /// Blood splatter decal.
    Blood,
    /// Scorch / burn decal (explosion residue).
    Scorch,
    /// Tire-track decal laid by a vehicle.
    TireTrack,
    /// `DamageShadow` — projected scorch/damage darkening (code map §4).
    DamageShadow,
}

impl DecalType {
    /// The canonical lowercase name the engine hashes to key this decal material.
    pub fn canonical_name(self) -> &'static str {
        match self {
            DecalType::BulletHole => "bullethole",
            DecalType::Blood => "blood",
            DecalType::Scorch => "scorch",
            DecalType::TireTrack => "tiretrack",
            DecalType::DamageShadow => "damageshadow",
        }
    }

    /// The 32-bit row key — `pandemic_hash_m2` of the canonical name (how the engine addresses it).
    pub fn hash(self) -> u32 {
        pandemic_hash_m2(self.canonical_name())
    }

    /// The full recovered category set, in `PgDecalTable` order.
    pub fn all() -> [DecalType; 5] {
        [
            DecalType::BulletHole,
            DecalType::Blood,
            DecalType::Scorch,
            DecalType::TireTrack,
            DecalType::DamageShadow,
        ]
    }
}

/// One `PgDecalTable` row — a decal-material definition (code map §1 layout:
/// texture handle / size / lifetime / super flag), plus the two data-only bind slots (§2).
///
/// The numeric fields are **loadable data** (`confirm-live`): a table loader fills them from the
/// resident block. The struct encodes the recovered *columns*, not invented retail numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalDef {
    /// Row key — `pandemic_hash_m2` of the material name (the address the engine looks it up by).
    pub key: u32,
    /// Base colour/albedo texture handle (WAD hash). `0` = unbound (loader fills it).
    pub texture: u32,
    /// `decalNormal` normal-map handle bound to the `decalNormal` slot (§2). `0` = unbound.
    pub normal_map: u32,
    /// `decalParam` param-map handle bound to the `decalParam` slot (§2). `0` = unbound.
    pub param_map: u32,
    /// Projection footprint size in world units (the box the decal projects within).
    pub size: f32,
    /// Lifetime in seconds before the per-frame GC (`DecalsUpdate`/`DecalUnlock`) frees the instance.
    /// `<= 0` = permanent (persists until evicted by the pool's reuse-oldest policy).
    pub lifetime: f32,
    /// `EnableSuperDecal` higher-coverage variant flag (`_super`; the `global_decal_super_concrete`
    /// MTRL seen in the PMC hall). Selects the higher-coverage shader permutation at draw time.
    pub super_decal: bool,
}

impl DecalDef {
    /// A neutral, clearly-placeholder definition for `key`. All numeric params are data-driven
    /// defaults (size `1.0`, lifetime `30 s`), **not** recovered retail values — a loader overwrites
    /// them from the resident block. Exposed so the pool/lookup mechanism is exercisable.
    pub fn placeholder(key: u32) -> Self {
        DecalDef {
            key,
            texture: 0,
            normal_map: 0,
            param_map: 0,
            size: 1.0,
            lifetime: 30.0,
            super_decal: false,
        }
    }
}

/// The `decaltable` resident singleton — the array of [`DecalDef`] rows keyed by material hash.
///
/// Loadable data + lookup: the engine fills it from the `0x400`-byte resident block; the reimpl fills
/// it from a loader or from [`stock`](DecalTable::stock). Lookup is by row key (material hash) or by a
/// [`DecalType`] handle.
#[derive(Clone, Debug, Default)]
pub struct DecalTable {
    rows: Vec<DecalDef>,
}

impl DecalTable {
    /// An empty table (a loader appends rows).
    pub fn new() -> Self {
        DecalTable { rows: Vec::new() }
    }

    /// The recovered category set seeded with **placeholder** parameters (see [`DecalDef::placeholder`]).
    /// `DamageShadow` is seeded as a `super_decal` (it is the higher-coverage damage-darkening variant).
    /// The numbers are data-driven, not retail — this makes the pool/lookup mechanism exercisable and
    /// end-to-end testable without claiming the confirm-live values.
    pub fn stock() -> Self {
        let mut t = DecalTable::new();
        for ty in DecalType::all() {
            let mut def = DecalDef::placeholder(ty.hash());
            if ty == DecalType::DamageShadow {
                def.super_decal = true;
            }
            t.rows.push(def);
        }
        t
    }

    /// Append / register a row. If a row with the same key exists it is replaced (a re-load).
    pub fn insert(&mut self, def: DecalDef) {
        if let Some(slot) = self.rows.iter_mut().find(|d| d.key == def.key) {
            *slot = def;
        } else {
            self.rows.push(def);
        }
    }

    /// Look a row up by its material hash (the engine's addressing).
    pub fn get(&self, key: u32) -> Option<&DecalDef> {
        self.rows.iter().find(|d| d.key == key)
    }

    /// Look a row up by a recovered [`DecalType`] handle.
    pub fn get_type(&self, ty: DecalType) -> Option<&DecalDef> {
        self.get(ty.hash())
    }

    /// Number of registered rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Iterate the rows in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &DecalDef> {
        self.rows.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_type_hash_constant() {
        assert_eq!(DECALTABLE_TYPE_HASH, 0x3B0A_ABF8);
        assert_eq!(DECALTABLE_RESIDENT_ALLOC, 0x400);
        assert_eq!(DECALTABLE_RESIDENT_FLAG, 0x4000);
    }

    #[test]
    fn stock_table_has_the_recovered_category_set() {
        let t = DecalTable::stock();
        assert_eq!(t.len(), 5);
        for ty in DecalType::all() {
            assert!(t.get_type(ty).is_some(), "{ty:?} row must be present");
        }
    }

    #[test]
    fn lookup_by_material_hash_matches_type_hash() {
        let t = DecalTable::stock();
        // The engine addresses a row by pandemic_hash_m2 of the material name.
        let key = pandemic_hash_m2("scorch");
        assert_eq!(DecalType::Scorch.hash(), key);
        assert_eq!(t.get(key).unwrap().key, key);
    }

    #[test]
    fn distinct_categories_have_distinct_keys() {
        let keys: std::collections::HashSet<u32> = DecalType::all().iter().map(|t| t.hash()).collect();
        assert_eq!(keys.len(), 5, "each category hashes to a distinct row key");
    }

    #[test]
    fn damage_shadow_is_a_super_decal() {
        let t = DecalTable::stock();
        assert!(t.get_type(DecalType::DamageShadow).unwrap().super_decal);
        assert!(!t.get_type(DecalType::BulletHole).unwrap().super_decal);
    }

    #[test]
    fn insert_replaces_same_key() {
        let mut t = DecalTable::new();
        let key = DecalType::Blood.hash();
        t.insert(DecalDef { size: 2.0, ..DecalDef::placeholder(key) });
        t.insert(DecalDef { size: 5.0, ..DecalDef::placeholder(key) });
        assert_eq!(t.len(), 1, "same key replaces, not appends");
        assert_eq!(t.get(key).unwrap().size, 5.0);
    }
}
