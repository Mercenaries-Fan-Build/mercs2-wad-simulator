//! Rainbow table hash → name resolver.
//!
//! Loads rainbow-table JSON files (`{"pandemic_hash_m2": {"0xHEX": ["name", …]}}`) and provides
//! fast lookup from pandemic_hash_m2 values to human-readable asset names.

use std::collections::HashMap;
use std::path::Path;

/// A loaded rainbow table mapping hash values to candidate names.
#[derive(Default)]
pub struct RainbowTable {
    m2: HashMap<u32, Vec<String>>,
}

impl RainbowTable {
    /// Load and merge several rainbow-table files (later files add names for
    /// hashes the earlier ones missed; existing entries are never overwritten).
    pub fn load_many(paths: &[std::path::PathBuf]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut merged = Self::default();
        for p in paths {
            let t = Self::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
            for (hash, names) in t.m2 {
                merged.m2.entry(hash).or_insert(names);
            }
        }
        Ok(merged)
    }

    /// Load one rainbow-table file. A file without a `pandemic_hash_m2` object is an error.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        let root: serde_json::Value = serde_json::from_str(&text)?;

        let obj = root
            .get("pandemic_hash_m2")
            .and_then(|v| v.as_object())
            .ok_or_else(|| format!("{} has no `pandemic_hash_m2` object", path.display()))?;
        let mut m2 = HashMap::new();
        for (hex_key, names_val) in obj {
            let hash = u32::from_str_radix(hex_key.trim_start_matches("0x"), 16)
                .unwrap_or(0);
            if hash == 0 {
                continue;
            }
            let names: Vec<String> = match names_val.as_array() {
                Some(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                None => continue,
            };
            if !names.is_empty() {
                m2.insert(hash, names);
            }
        }

        Ok(Self { m2 })
    }

    /// Load a table of names HARVESTED from block bytes (`aset_block_strings.json`), keeping only
    /// the identifier-shaped ones ([`mercs2_formats::hash::is_identifier_shaped`]) and dropping
    /// every `(hash, name)` pair listed in `rejected` (`crates/mercs2_workshop/data/rejected_names.json`).
    /// A rejected entry whose name does not hash to its key is an error.
    pub fn load_harvested(path: &Path, rejected: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let rejected = Self::load(rejected).map_err(|e| format!("{}: {e}", rejected.display()))?;
        for (hash, names) in &rejected.m2 {
            for n in names {
                if mercs2_formats::hash::pandemic_hash_m2(n) != *hash {
                    return Err(format!("rejected name {n} does not hash to 0x{hash:08X}").into());
                }
            }
        }
        let mut t = Self::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
        t.m2.retain(|hash, names| {
            let bad = rejected.candidates(*hash);
            names.retain(|n| mercs2_formats::hash::is_identifier_shaped(n) && !bad.contains(n));
            !names.is_empty()
        });
        Ok(t)
    }

    /// Add `other`'s names for the hashes this table does not have yet (existing entries win, as in
    /// [`Self::load_many`]).
    pub fn merge(&mut self, other: RainbowTable) {
        for (hash, names) in other.m2 {
            self.m2.entry(hash).or_insert(names);
        }
    }

    /// Resolve a hash to the first (best) candidate name.
    pub fn resolve(&self, hash: u32) -> Option<&str> {
        self.m2.get(&hash).and_then(|v| v.first()).map(|s| s.as_str())
    }

    /// All candidate names for a hash (case variants / genuine collisions).
    /// Empty slice when the hash is unresolved.
    pub fn candidates(&self, hash: u32) -> &[String] {
        self.m2.get(&hash).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Format a hash as "0x{hash:08X} (name)" or just "0x{hash:08X}" if unknown.
    pub fn annotate(&self, hash: u32) -> String {
        match self.resolve(hash) {
            Some(name) => format!("0x{hash:08X} ({name})"),
            None => format!("0x{hash:08X}"),
        }
    }

    /// Number of entries loaded.
    pub fn len(&self) -> usize {
        self.m2.len()
    }

    pub fn is_empty(&self) -> bool {
        self.m2.is_empty()
    }
}
