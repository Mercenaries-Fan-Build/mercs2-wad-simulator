//! Rainbow table hash → name resolver.
//!
//! Loads `tools/rainbow_table.json` and provides fast lookup from
//! pandemic_hash_m2 values to human-readable asset names.

use std::collections::HashMap;
use std::path::Path;

/// A loaded rainbow table mapping hash values to candidate names.
#[derive(Default)]
pub struct RainbowTable {
    m2: HashMap<u32, Vec<String>>,
}

impl RainbowTable {
    /// Load from a rainbow_table.json file.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        let root: serde_json::Value = serde_json::from_str(&text)?;

        let mut m2 = HashMap::new();
        if let Some(obj) = root.get("pandemic_hash_m2").and_then(|v| v.as_object()) {
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
        }

        Ok(Self { m2 })
    }

    /// Resolve a hash to the first (best) candidate name.
    pub fn resolve(&self, hash: u32) -> Option<&str> {
        self.m2.get(&hash).and_then(|v| v.first()).map(|s| s.as_str())
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
