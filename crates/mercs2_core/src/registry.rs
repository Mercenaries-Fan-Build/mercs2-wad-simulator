//! Keystone A — the reflection / component-descriptor registry.
//!
//! The original engine registers every component *class* through one shared template: a per-class
//! descriptor carrying a `CopyFromStream` deserialize vtable, the class-name string, a serialized
//! record **stride**, a preallocation **pool budget** (`0x100` default, overridden per class by
//! `cdbsizes.ini`), and the `0x9e3779b9` golden-ratio seed used to key the class by its name-hash.
//! ~16 byte-identical registrars call the shared registrar `FUN_0064a770`; the field schema is a
//! separate per-class template. Evidence: `docs/mercs2-ecs/` (232 classes = 220 gameplay + 12
//! render/pipeline) and `docs/modernization/pangea_engine_alignment.md` §1 "Keystone A".
//!
//! This is the modern analog: the kernel's component/serialization spine. It stores each class's
//! descriptor keyed by its **type-hash** (the engine's `pandemic_hash_m2(name)`). The hashing itself
//! is NOT done here — this crate is asset-agnostic, and the name-hash lives at the asset/byte-decode
//! boundary (`mercs2_formats::hash`) so there is a single implementation and no drift. Callers pass
//! the precomputed `type_hash`.
//!
//! What consumes it: the world/streaming loader presizes component pools from `pool_budget` and,
//! once field-schema deserialization lands, instantiates a component from a stream by looking up its
//! `type_hash`. (Deserialization is the next brick — it needs the field-builder templates + a stream
//! reader, which live at the `mercs2_formats` boundary; this slice stands up the descriptor table
//! and the `cdbsizes.ini` budgets.)

use std::collections::HashMap;

/// The engine's default component preallocation budget (`0x100` = 256) — used when a class is not
/// listed in `cdbsizes.ini`. Matches the descriptor's pool field across every registrar body.
pub const DEFAULT_POOL_BUDGET: u32 = 0x100;

/// The kind of a reflected component field — the kernel-side mirror of the exe's `schm` field type
/// codes. The byte-level decode lives at the asset boundary (`mercs2_formats::schema::SchemaFieldType`
/// + its `CopyFromStream` analog); this crate stays asset-agnostic and re-declares only the small tag
/// it needs to describe a class's record layout. Codes match the on-disk `schm` type codes exactly
/// (1/2/4/5/6/7/8/9/10/11; there is no code 3), so a loader can translate one to the other by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Type 1 — one packed bit (see [`FieldLayout::bit_index`]).
    Bit,
    /// Type 2 — `u8`.
    U8,
    /// Type 4 — `u16`.
    U16,
    /// Type 5 — `f32`.
    F32,
    /// Type 6 — 32-bit hash/id.
    U32,
    /// Type 7 — 32-bit reference.
    Ref,
    /// Type 8 — 32-bit / inline string reference.
    StringRef,
    /// Type 9 — 32-bit flags word.
    Flags,
    /// Type 10 — `[f32; 3]`.
    Vec3,
    /// Type 11 — 32-byte composite (8×`f32`, e.g. a Transform pos+quat blob).
    Blob32,
}

impl FieldKind {
    /// Map an on-disk `schm` type code to a [`FieldKind`] (the same code set the exe uses).
    pub fn from_type_code(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::Bit,
            2 => Self::U8,
            4 => Self::U16,
            5 => Self::F32,
            6 => Self::U32,
            7 => Self::Ref,
            8 => Self::StringRef,
            9 => Self::Flags,
            10 => Self::Vec3,
            11 => Self::Blob32,
            _ => return None,
        })
    }

    /// Serialized width in bytes (a `Bit` occupies no bytes of its own — it shares a byte).
    pub fn byte_width(self) -> usize {
        match self {
            Self::Bit => 0,
            Self::U8 => 1,
            Self::U16 => 2,
            Self::F32 | Self::U32 | Self::Ref | Self::StringRef | Self::Flags => 4,
            Self::Vec3 => 12,
            Self::Blob32 => 32,
        }
    }
}

/// One reflected field within a component's serialized record: which named field
/// (`pandemic_hash_m2(field_name)`) lives at which byte offset, and its [`FieldKind`]. This is the
/// kernel-side view of a `schm` entry; a world/streaming loader fills it from
/// `mercs2_formats::schema::ComponentSchema` (offsets already resolved to the record-local LOW-16
/// byte offset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    /// `pandemic_hash_m2(field_name)` — the key sim code looks a field up by.
    pub name_hash: u32,
    /// Byte offset of the field inside the serialized payload record.
    pub byte_offset: u16,
    /// For [`FieldKind::Bit`], the bit position within `byte_offset`'s byte; 0 otherwise.
    pub bit_index: u8,
    pub kind: FieldKind,
}

/// One registered component class — the Rust analog of the exe's ~0x50-byte descriptor. We carry the
/// fields the kernel needs: the class name, its type-hash key, the preallocation budget, and the
/// serialized record size when the field schema is known (the descriptor's stride).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentDescriptor {
    pub name: String,
    /// `pandemic_hash_m2(name)` — the registry key (the exe hashes the class name to a type id).
    pub type_hash: u32,
    /// Preallocation budget = the `cdbsizes.ini` count for this class, or [`DEFAULT_POOL_BUDGET`].
    pub pool_budget: u32,
    /// Serialized record stride in bytes, when the field schema is known (the descriptor's stride).
    pub record_size: Option<u32>,
    /// The class's reflected field layout (from its `schm`), when known. Empty for classes
    /// registered without a schema (e.g. opaque `Runtime*`/Controller/Physics blocks, or a
    /// pool-only presize). Ordered as they appear in the schema (= stream order).
    pub fields: Vec<FieldLayout>,
}

impl ComponentDescriptor {
    /// Look up a reflected field by its name-hash (`pandemic_hash_m2(field_name)`).
    pub fn field(&self, name_hash: u32) -> Option<&FieldLayout> {
        self.fields.iter().find(|f| f.name_hash == name_hash)
    }
}

/// The component-class registry: `type_hash → descriptor`. Registration order is irrelevant (unlike
/// the [`crate::Schedule`], which is Keystone C's ordered tick); lookup is by the class name-hash the
/// stream carries.
#[derive(Default)]
pub struct ComponentRegistry {
    by_hash: HashMap<u32, ComponentDescriptor>,
    /// Preallocation counts keyed by class name, loaded from `cdbsizes.ini`; consulted at register.
    budgets: HashMap<String, u32>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load preallocation budgets from a `cdbsizes.ini`-format string. Only the `[presize]` section
    /// is read; each line is `<ClassName> <count> [extra]` (whitespace/tab separated — the optional
    /// trailing number is a secondary/growth field we don't need for the budget). Returns how many
    /// budgets were parsed. Call before `register` so descriptors pick up their real pool size.
    pub fn load_budgets(&mut self, cdbsizes_ini: &str) -> usize {
        let mut in_presize = false;
        let mut n = 0;
        for raw in cdbsizes_ini.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_presize = line.eq_ignore_ascii_case("[presize]");
                continue;
            }
            if !in_presize {
                continue;
            }
            let mut it = line.split_whitespace();
            if let (Some(name), Some(count)) = (it.next(), it.next()) {
                if let Ok(c) = count.parse::<u32>() {
                    self.budgets.insert(name.to_string(), c);
                    n += 1;
                }
            }
        }
        n
    }

    /// Register a component class by `name` + its precomputed `type_hash` (hash the name with the
    /// engine name-hash at the call site — see module docs on why it isn't done here). The pool
    /// budget is taken from any loaded `cdbsizes.ini` entry for `name`, else [`DEFAULT_POOL_BUDGET`].
    /// Idempotent per `type_hash`: re-registering the same class returns the existing descriptor.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        type_hash: u32,
        record_size: Option<u32>,
    ) -> &ComponentDescriptor {
        self.register_with_fields(name, type_hash, record_size, Vec::new())
    }

    /// Register a component class together with its reflected [`FieldLayout`] (the class's `schm`,
    /// already decoded at the asset boundary). This is the faithful wire from the on-disk field
    /// schema to the kernel registry: `record_size` is the schema's payload stride and `fields`
    /// carry each field's name-hash → byte offset / kind, so sim code can resolve a component's
    /// fields by name without re-touching bytes. Idempotent per `type_hash`.
    pub fn register_with_fields(
        &mut self,
        name: impl Into<String>,
        type_hash: u32,
        record_size: Option<u32>,
        fields: Vec<FieldLayout>,
    ) -> &ComponentDescriptor {
        let name = name.into();
        let pool_budget = self.budgets.get(&name).copied().unwrap_or(DEFAULT_POOL_BUDGET);
        self.by_hash.entry(type_hash).or_insert(ComponentDescriptor {
            name,
            type_hash,
            pool_budget,
            record_size,
            fields,
        })
    }

    /// Look up a class descriptor by its type-hash (the key the stream carries).
    pub fn get(&self, type_hash: u32) -> Option<&ComponentDescriptor> {
        self.by_hash.get(&type_hash)
    }

    /// Look up a class descriptor by name (linear — for tooling/debug, not the hot path).
    pub fn get_by_name(&self, name: &str) -> Option<&ComponentDescriptor> {
        self.by_hash.values().find(|d| d.name == name)
    }

    /// The `cdbsizes.ini` budget for a class name, if one was loaded (before/without registration).
    pub fn budget_for(&self, name: &str) -> Option<u32> {
        self.budgets.get(name).copied()
    }

    /// Number of registered classes.
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// Total preallocation across all registered classes — the component pool footprint the world
    /// will need (the sum the exe reserves up front from `cdbsizes.ini`).
    pub fn total_budget(&self) -> u64 {
        self.by_hash.values().map(|d| d.pool_budget as u64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A trimmed cdbsizes.ini slice (real format: `[presize]` then `<Class> <count> [extra]`).
    const CDB: &str = "\
[presize]
SceneObject 161280
HibernationControl 14080
ModelName 4608
AiBehavior 512
_CarPhysicsV2 768
";

    #[test]
    fn loads_presize_budgets_and_ignores_other_sections() {
        let mut reg = ComponentRegistry::new();
        let n = reg.load_budgets(CDB);
        assert_eq!(n, 5);
        assert_eq!(reg.budget_for("SceneObject"), Some(161_280));
        assert_eq!(reg.budget_for("HibernationControl"), Some(14_080));
        assert_eq!(reg.budget_for("_CarPhysicsV2"), Some(768));
        assert_eq!(reg.budget_for("NotAThing"), None);
    }

    #[test]
    fn register_picks_up_budget_else_defaults() {
        let mut reg = ComponentRegistry::new();
        reg.load_budgets(CDB);
        // Known class → cdbsizes budget; hash is caller-supplied (stand-in constants here).
        let d = reg.register("ModelName", 0xE18A_0001, Some(8)).clone();
        assert_eq!(d.pool_budget, 4608);
        assert_eq!(d.record_size, Some(8));
        // Unlisted class → default 0x100 budget.
        let d2 = reg.register("SomeRuntimeThing", 0xDEAD_BEEF, None).clone();
        assert_eq!(d2.pool_budget, DEFAULT_POOL_BUDGET);
        // Lookup by hash and by name resolve to the same descriptor.
        assert_eq!(reg.get(0xE18A_0001), Some(&d));
        assert_eq!(reg.get_by_name("ModelName"), Some(&d));
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn register_is_idempotent_per_type_hash() {
        let mut reg = ComponentRegistry::new();
        reg.register("Health", 0x1111_2222, Some(8));
        reg.register("Health", 0x1111_2222, Some(8));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.total_budget(), DEFAULT_POOL_BUDGET as u64);
    }

    #[test]
    fn field_kind_matches_schm_type_codes() {
        assert_eq!(FieldKind::from_type_code(1), Some(FieldKind::Bit));
        assert_eq!(FieldKind::from_type_code(4), Some(FieldKind::U16));
        assert_eq!(FieldKind::from_type_code(11), Some(FieldKind::Blob32));
        assert_eq!(FieldKind::from_type_code(3), None); // there is no code 3
        assert_eq!(FieldKind::from_type_code(0), None);
        assert_eq!(FieldKind::Bit.byte_width(), 0);
        assert_eq!(FieldKind::U16.byte_width(), 2);
        assert_eq!(FieldKind::Vec3.byte_width(), 12);
        assert_eq!(FieldKind::Blob32.byte_width(), 32);
    }

    /// Register HibernationControl with its **real** retail field layout (name-hashes + offsets
    /// captured from vz.wad) and resolve fields by name-hash. This is the kernel end of the
    /// schm→registry seam that `mercs2_formats::schema` feeds.
    #[test]
    fn register_with_real_hibernation_field_layout() {
        // Ground-truth from retail vz.wad HibernationControl schm (payload stride 6):
        //   0xcbe8ed58 u16@0 · 0x74e63261 u8@2 · 0xdea888ce u8@3 · 0x2332033f u8@4
        //   0x3ce51772 bit@5(idx0) · 0x3f1da641 bit@5(idx1)
        let fields = vec![
            FieldLayout { name_hash: 0xcbe8_ed58, byte_offset: 0, bit_index: 0, kind: FieldKind::U16 },
            FieldLayout { name_hash: 0x74e6_3261, byte_offset: 2, bit_index: 0, kind: FieldKind::U8 },
            FieldLayout { name_hash: 0xdea8_88ce, byte_offset: 3, bit_index: 0, kind: FieldKind::U8 },
            FieldLayout { name_hash: 0x2332_033f, byte_offset: 4, bit_index: 0, kind: FieldKind::U8 },
            FieldLayout { name_hash: 0x3ce5_1772, byte_offset: 5, bit_index: 0, kind: FieldKind::Bit },
            FieldLayout { name_hash: 0x3f1d_a641, byte_offset: 5, bit_index: 1, kind: FieldKind::Bit },
        ];
        let mut reg = ComponentRegistry::new();
        reg.load_budgets("[presize]\nHibernationControl 14080\n");
        let d = reg
            .register_with_fields("HibernationControl", 0x1234_5678, Some(6), fields)
            .clone();
        assert_eq!(d.pool_budget, 14080);
        assert_eq!(d.record_size, Some(6));
        assert_eq!(d.fields.len(), 6);
        // Resolve a field by name-hash (the sim-code access pattern).
        let dist0 = d.field(0xcbe8_ed58).expect("dist0 field");
        assert_eq!(dist0.byte_offset, 0);
        assert_eq!(dist0.kind, FieldKind::U16);
        // The two bit fields share byte 5 but differ by bit index.
        assert_eq!(d.field(0x3ce5_1772).unwrap().bit_index, 0);
        assert_eq!(d.field(0x3f1d_a641).unwrap().bit_index, 1);
        assert!(d.field(0xDEAD_BEEF).is_none());
        // Lookup by hash returns the same descriptor (fields included in equality).
        assert_eq!(reg.get(0x1234_5678), Some(&d));
    }
}
