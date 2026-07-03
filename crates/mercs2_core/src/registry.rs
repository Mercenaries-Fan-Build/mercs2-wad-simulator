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
        let name = name.into();
        let pool_budget = self.budgets.get(&name).copied().unwrap_or(DEFAULT_POOL_BUDGET);
        self.by_hash.entry(type_hash).or_insert(ComponentDescriptor {
            name,
            type_hash,
            pool_budget,
            record_size,
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
}
