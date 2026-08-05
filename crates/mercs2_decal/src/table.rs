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

    /// A per-category **faithful-look** definition — distinct size / lifetime / super-flag per decal
    /// category so the projected-decal pass reads correctly per type (a small tight bullet hole vs a
    /// broad scorch, a permanent damage shadow vs a fading tire track).
    ///
    /// `// CONFIRM-LIVE:` the exact retail size/lifetime numbers are stripped — the `decaltable`
    /// rows are read via computed offsets inside `FUN_004cb1f0`, never by name (decal_code_map §1/§6:
    /// bp `FUN_004cb1f0`, read the `0x400` resident block). These are **reimpl look-tuning**, chosen
    /// per-category to be plausible, NOT recovered retail values — [`DecalTable::load_resident_block`]
    /// overwrites them from a live capture. The category *set* and the size>tire, permanent-shadow
    /// *shape* are recovered (§1/§4); the scalars are the confirm-live remainder.
    pub fn for_category(ty: DecalType) -> Self {
        // (size_m, lifetime_s, super) per recovered category. Permanent = lifetime <= 0.
        let (size, lifetime, sup) = match ty {
            DecalType::BulletHole => (0.20, 45.0, false), // small, long-lived pockmark
            DecalType::Blood => (0.45, 25.0, false),      // broad splat, fades sooner
            DecalType::Scorch => (0.80, 60.0, false),     // large explosion burn
            DecalType::TireTrack => (0.35, 20.0, false),  // narrow, shortest-lived
            DecalType::DamageShadow => (1.20, 0.0, true), // permanent damage-darkening, super variant
        };
        DecalDef { size, lifetime, super_decal: sup, ..DecalDef::placeholder(ty.hash()) }
    }
}

/// The on-disk column layout of one `PgDecalTable` row in the `0x400` resident block. Because the
/// engine addresses the block by **computed offsets inside stripped functions** (never by name), the
/// exact stride and field offsets are `// CONFIRM-LIVE:` (bp `FUN_004cb1f0`, read the block).
/// [`DecalRowLayout::candidate`] is a documented decode hypothesis (the recovered 4-column shape:
/// texture handle / size / lifetime / super flag) a live capture confirms or corrects; a loader takes
/// it as a parameter rather than hard-coding it, so correcting the layout is a one-value change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalRowLayout {
    /// Bytes per row (the array stride within the block).
    pub stride: usize,
    /// Byte offset of the texture-handle `u32` within a row.
    pub texture_off: usize,
    /// Byte offset of the `f32` projection size.
    pub size_off: usize,
    /// Byte offset of the `f32` lifetime (seconds; `<= 0` = permanent).
    pub lifetime_off: usize,
    /// Byte offset of the `u32` super-decal flag (non-zero = `_super`).
    pub super_off: usize,
    /// Number of rows to read from the block.
    pub rows: usize,
}

impl DecalRowLayout {
    /// A documented decode **hypothesis** for the `0x400` `PgDecalTable` block: a `0x40`-byte row
    /// carrying `[texture:u32 @0][size:f32 @4][lifetime:f32 @8][super:u32 @0xc]`, 5 rows (the recovered
    /// category set). `// CONFIRM-LIVE:` this stride/offset set is unproven — the block is accessed by
    /// computed offset, so a live read of `FUN_004cb1f0`'s resident block is what pins it.
    pub fn candidate() -> Self {
        DecalRowLayout {
            stride: 0x40,
            texture_off: 0x0,
            size_off: 0x4,
            lifetime_off: 0x8,
            super_off: 0xc,
            rows: 5,
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

    /// The recovered category set seeded with per-category **faithful-look** parameters
    /// ([`DecalDef::for_category`]): distinct size/lifetime per type and the permanent super
    /// `DamageShadow`. `// CONFIRM-LIVE:` the scalars are reimpl look-tuning, not retail numbers — the
    /// category *set* and *shape* are recovered; [`load_resident_block`](Self::load_resident_block)
    /// overwrites the scalars from a live `0x400`-block capture. This makes the projection pass read
    /// correctly per type and stays end-to-end testable without claiming the confirm-live values.
    pub fn stock() -> Self {
        let mut t = DecalTable::new();
        for ty in DecalType::all() {
            t.rows.push(DecalDef::for_category(ty));
        }
        t
    }

    /// Load rows from a captured `0x400` resident `PgDecalTable` block, decoded per `layout`. This is
    /// the **mechanism** the engine uses (`FUN_004cb1f0` allocates the block; the table is read by
    /// computed offset) realized as a data loader: given a live-captured block + the confirmed
    /// [`DecalRowLayout`], it fills the recovered category rows in `PgDecalTable` order. Rows that
    /// would read past the block end are skipped. Returns how many rows were decoded.
    ///
    /// `// CONFIRM-LIVE:` `block` must be a real capture and `layout` must be the confirmed byte
    /// layout — neither is statically recoverable (decal_code_map §1/§6). With
    /// [`DecalRowLayout::candidate`] and a placeholder block this exercises the path; on retail data it
    /// yields the real per-type params.
    pub fn load_resident_block(&mut self, block: &[u8], layout: DecalRowLayout) -> usize {
        let rd_u32 = |o: usize| -> u32 {
            u32::from_le_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]])
        };
        let rd_f32 = |o: usize| -> f32 { f32::from_bits(rd_u32(o)) };
        let cats = DecalType::all();
        let mut n = 0;
        for (i, ty) in cats.iter().enumerate().take(layout.rows) {
            let base = i * layout.stride;
            // Bounds: every field the layout names must fit inside the block.
            let end = base
                + layout
                    .texture_off
                    .max(layout.size_off)
                    .max(layout.lifetime_off)
                    .max(layout.super_off)
                + 4;
            if end > block.len() {
                break;
            }
            self.insert(DecalDef {
                key: ty.hash(),
                texture: rd_u32(base + layout.texture_off),
                normal_map: 0,
                param_map: 0,
                size: rd_f32(base + layout.size_off),
                lifetime: rd_f32(base + layout.lifetime_off),
                super_decal: rd_u32(base + layout.super_off) != 0,
            });
            n += 1;
        }
        n
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
    fn for_category_has_distinct_faithful_shapes() {
        // Recovered *shape* invariants (not the confirm-live scalars): scorch is the broadest of the
        // fading decals, tire tracks the shortest-lived, and the damage shadow is permanent + super.
        let scorch = DecalDef::for_category(DecalType::Scorch);
        let tire = DecalDef::for_category(DecalType::TireTrack);
        let bullet = DecalDef::for_category(DecalType::BulletHole);
        let shadow = DecalDef::for_category(DecalType::DamageShadow);
        assert!(scorch.size > bullet.size, "scorch broader than a bullet hole");
        assert!(tire.lifetime < scorch.lifetime, "tire track shorter-lived than scorch");
        assert!(shadow.lifetime <= 0.0 && shadow.super_decal, "damage shadow permanent + super");
        assert!(!bullet.super_decal);
    }

    #[test]
    fn load_resident_block_decodes_rows_per_layout() {
        // Build a synthetic 0x400 block in the candidate layout and confirm the loader overwrites the
        // stock scalars from it (the mechanism; retail numbers stay confirm-live).
        let layout = DecalRowLayout::candidate();
        let mut block = vec![0u8; DECALTABLE_RESIDENT_ALLOC];
        for (i, _ty) in DecalType::all().iter().enumerate() {
            let base = i * layout.stride;
            let tex = 0x1000_0000u32 + i as u32;
            let size = 0.5 + i as f32; // distinct per row
            let life = 10.0 + i as f32;
            block[base..base + 4].copy_from_slice(&tex.to_le_bytes());
            block[base + 4..base + 8].copy_from_slice(&size.to_le_bytes());
            block[base + 8..base + 12].copy_from_slice(&life.to_le_bytes());
            block[base + 12..base + 16].copy_from_slice(&((i as u32) & 1).to_le_bytes());
        }
        let mut t = DecalTable::stock();
        let n = t.load_resident_block(&block, layout);
        assert_eq!(n, 5, "all recovered rows decoded");
        let bullet = t.get_type(DecalType::BulletHole).unwrap();
        assert_eq!(bullet.texture, 0x1000_0000);
        assert_eq!(bullet.size, 0.5);
        assert_eq!(bullet.lifetime, 10.0);
        // row 1 (Blood) had super flag bit set.
        assert!(t.get_type(DecalType::Blood).unwrap().super_decal);
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
