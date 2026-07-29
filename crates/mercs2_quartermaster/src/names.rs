//! Reverse hash → name, so humans never have to read a hash we could have named for them.
//!
//! **Names are the identity; the hash is only the normalized comparison key.** `pandemic_hash_m2`
//! is one-way, so comparison has to happen on the hash — it is the only total function over both
//! spellings an author may write. But everything a human READS should be a name, and everything a
//! human WRITES should be a name too (mandate `no-arbitrary-hashes`).
//!
//! This module closes the gap between those: it takes the curated
//! `data/production_names.json` (23,110 hash-verified entries) and gives back the name for a hash,
//! so that
//!
//! * a `touches: ["0xE54047D5"]` shows up in diagnostics as `al_veh_boat_destroyer`, and
//! * the author gets told to write the name instead — see [`bare_hash_suggestions`].
//!
//! The table is HOST-PROVIDED, like the game stack: the crate takes a path rather than reaching
//! into the filesystem on its own. [`NameTable::find_from`] exists as a convenience for callers
//! that want the workspace copy, and mirrors what `mercs2_workshop` already does.

use crate::blast::{Claim, ClaimRecord};
use crate::manifest::{Contribution, Manifest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Curated `hash → name` lookup.
#[derive(Debug, Clone, Default)]
pub struct NameTable {
    by_hash: HashMap<u32, String>,
}

#[derive(Debug)]
pub enum NameTableError {
    Io { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
}

impl std::fmt::Display for NameTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameTableError::Io { path, message } => {
                write!(f, "reading {}: {message}", path.display())
            }
            NameTableError::Parse { path, message } => {
                write!(f, "parsing {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for NameTableError {}

/// The committed lookup, relative to a repo root.
pub const PRODUCTION_NAMES: &str = "data/production_names.json";

impl NameTable {
    /// Build from `hash → name` pairs. Mostly for tests and for callers with their own source.
    pub fn from_pairs<I, S>(pairs: I) -> NameTable
    where
        I: IntoIterator<Item = (u32, S)>,
        S: Into<String>,
    {
        NameTable {
            by_hash: pairs.into_iter().map(|(h, n)| (h, n.into())).collect(),
        }
    }

    /// Load `data/production_names.json`. Shape: `{ "pandemic_hash_m2": { "0xHHHHHHHH": "name" } }`.
    pub fn load(path: &Path) -> Result<NameTable, NameTableError> {
        let text = std::fs::read_to_string(path).map_err(|e| NameTableError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let doc: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| NameTableError::Parse {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?;
        let map = doc
            .get("pandemic_hash_m2")
            .and_then(|v| v.as_object())
            .ok_or_else(|| NameTableError::Parse {
                path: path.to_path_buf(),
                message: "missing a `pandemic_hash_m2` object".into(),
            })?;
        let mut by_hash = HashMap::with_capacity(map.len());
        for (key, value) in map {
            let hex = key.trim().trim_start_matches("0x").trim_start_matches("0X");
            let (Ok(hash), Some(name)) = (u32::from_str_radix(hex, 16), value.as_str()) else {
                // A malformed row is skipped rather than fatal: a partial table still helps, and
                // this file is generated, so a hard failure here would block work on an unrelated
                // regeneration bug.
                continue;
            };
            by_hash.insert(hash, name.to_string());
        }
        Ok(NameTable { by_hash })
    }

    /// Walk up from `start` looking for `data/production_names.json`, as `mercs2_workshop` does.
    /// Returns `None` rather than erroring — a missing table degrades diagnostics, it is never fatal.
    pub fn find_from(start: &Path) -> Option<NameTable> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let candidate = d.join(PRODUCTION_NAMES);
            if candidate.is_file() {
                return NameTable::load(&candidate).ok();
            }
            dir = d.parent();
        }
        None
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// The name for a hash, when we know one.
    pub fn reverse(&self, hash: u32) -> Option<&str> {
        self.by_hash.get(&hash).map(|s| s.as_str())
    }

    /// Fill in `name` on any claim record that lacks one but whose hash we can reverse — so a
    /// conflict over `0xE54047D5` reports `al_veh_boat_destroyer` instead.
    pub fn enrich(&self, records: &mut [ClaimRecord]) {
        for r in records.iter_mut() {
            if r.name.is_some() {
                continue;
            }
            if let Claim::Asset { hash } = r.claim {
                if let Some(name) = self.reverse(hash) {
                    r.name = Some(name.to_string());
                }
            }
        }
    }
}

/// An author wrote a bare hash for something we have a name for.
///
/// Per the mandate a bare hash is legal ONLY when no name is known. This is the diagnostic that
/// makes that enforceable rather than aspirational — and it is auto-fixable, since the replacement
/// text is exactly `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BareHashSuggestion {
    pub index: usize,
    pub kind: &'static str,
    /// What the author wrote, e.g. `0xE54047D5`.
    pub written: String,
    /// What they should have written.
    pub name: String,
}

impl BareHashSuggestion {
    /// The suggestion WITHOUT the `contributions[i]` prefix, for a [`crate::lint::Diagnostic`] that
    /// already prints the index from its `at` field.
    pub fn detail(&self) -> String {
        format!(
            "({}) refers to `{}`, which is `{}` — a name reads and diffs better, and cannot drift \
             from the asset it was copied for. The hash works; this is a suggestion, not a defect.",
            self.kind, self.written, self.name
        )
    }
}

impl std::fmt::Display for BareHashSuggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "contributions[{}] {}", self.index, self.detail())
    }
}

/// Every `touches:` entry written as a bare hash whose name we actually know.
pub fn bare_hash_suggestions(manifest: &Manifest, names: &NameTable) -> Vec<BareHashSuggestion> {
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let kind = c.kind();

        // References to assets that ALREADY EXIST. `name:` on add_model/add_outfit is deliberately
        // absent: that field mints a new identity, so a hash there is not an unnamed reference to
        // something we could name — and if it collided with a retail asset that would be a
        // different and more serious finding than "you could have written a name".
        let mut refs: Vec<&str> = Vec::new();
        match c {
            Contribution::ReplaceTexture { target, .. } => refs.push(target),
            Contribution::EditStateMachine { target, .. } => refs.push(target),
            Contribution::AddModel { donor, .. } | Contribution::AddOutfit { donor, .. } => {
                if let Some(d) = donor {
                    refs.push(d);
                }
            }
            _ => {}
        }
        // `raw` declares its blast radius by hand, so its `touches` are asset references too. A
        // native hook's `touches` are CODE ADDRESSES — reversing one through the ASSET name table
        // would be nonsense, so those are skipped rather than mis-suggested.
        if let Contribution::Raw { touches, .. } = c {
            refs.extend(touches.iter().map(|t| t.0.as_str()));
        }

        for r in refs {
            let Some(hash) = crate::manifest::bare_hash(r) else {
                continue; // already a name
            };
            if let Some(name) = names.reverse(hash) {
                out.push(BareHashSuggestion {
                    index,
                    kind,
                    written: r.trim().to_string(),
                    name: name.to_string(),
                });
            }
        }
    }
    out
}
