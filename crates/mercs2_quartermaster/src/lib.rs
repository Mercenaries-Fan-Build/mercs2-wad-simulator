//! `mercs2_quartermaster` — the Shipment format: read, validate, (later) lint and build.
//!
//! A **Shipment** is a mod package; the **Quartermaster** is the engine that works it. Neither the
//! Workshop nor Modkit owns the format — this crate does, and both are clients.
//!
//! Spec: `.claude/plans/workshop-mods-rebuild-04-manifest-format.md` (rev 3, DRAFT — not frozen).
//!
//! ## What is here so far
//!
//! Phase 1, first increment: the manifest model plus the cross-format read path. This deliberately
//! leads with the part that de-risks the schema — if TOML cannot carry an internally-tagged enum,
//! that is a *format* change, and it is far cheaper to learn now than after a builder exists.
//!
//! ## What is deliberately NOT here
//!
//! * **Game-path discovery.** This crate is path-in, never path-discovering: the WAD stack arrives
//!   as an argument (exactly as `mercs2_workshop::publish::publish_in_background` already takes
//!   `wad_paths`). Resolution is the HOST's job — a Workshop Settings page, `qm --game`, or nothing
//!   at all in CI. Everything in [`manifest`] runs with no game present, which is what makes
//!   lint-only CI possible for the template repo.
//! * The linter, blast-radius computation, and the builder — later increments.

pub mod blast;
pub mod build;
pub mod discover;
pub mod game;
pub mod link;
pub mod lint;
pub mod manifest;
pub mod names;

pub use blast::{
    claims, conflicts, merge_class, self_conflicts, unsatisfied_reads, Access, Claim, ClaimRecord,
    Claimant, Conflict, MergeClass, SelfConflict, UnsatisfiedRead,
};
pub use build::{build, sha256_hex, BuildError, BuildReport, Destination, Placement};
pub use discover::{
    check_sources, find_manifest, open as open_shipment, source_refs, DiscoverError,
    LoadedShipment, OpenError, SourceIssue, SourceRef,
};
pub use game::{GameStack, GameStackError};
pub use lint::{blocks_build, lint, Diagnostic, Rule, Severity};
pub use manifest::{
    Contribution, Layer, Load, Manifest, PlaceIn, Requirement, Retarget, Shipment, Target,
    Textures, Touch, ValidateError, FORMAT_VERSION, MAX_NAME_LEN,
};
pub use names::{bare_hash_suggestions, BareHashSuggestion, NameTable};

/// Serialization formats a manifest may be written in. Detection is by EXTENSION; more than one
/// `manifest.*` in a Shipment root is an ambiguity error, never a silent pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The preferred form. What the template scaffolds and what the Quartermaster WRITES.
    Yaml,
    /// First-class on read — the natural format for the JS half of the ecosystem.
    Json,
    /// Accepted on read for Cargo-familiar authors.
    Toml,
}

impl Format {
    /// Detect from a file extension. Returns `None` for anything else.
    pub fn from_extension(ext: &str) -> Option<Format> {
        match ext.to_ascii_lowercase().as_str() {
            "yaml" | "yml" => Some(Format::Yaml),
            "json" => Some(Format::Json),
            "toml" => Some(Format::Toml),
            _ => None,
        }
    }
}

/// Failure reading a manifest. Parse errors keep the underlying message — an author needs the line
/// number, not "invalid manifest".
#[derive(Debug)]
pub enum ReadError {
    Parse { format: Format, message: String },
    Validate(ValidateError),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Parse { format, message } => {
                write!(f, "parsing manifest as {format:?}: {message}")
            }
            ReadError::Validate(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Parse a manifest from source text in a known format, then [`Manifest::validate`] it.
///
/// One `serde` model backs all three formats; this function is only the format dispatch.
pub fn from_str(text: &str, format: Format) -> Result<Manifest, ReadError> {
    let manifest: Manifest = match format {
        Format::Yaml => serde_norway::from_str(text).map_err(|e| ReadError::Parse {
            format,
            message: e.to_string(),
        })?,
        Format::Json => serde_json::from_str(text).map_err(|e| ReadError::Parse {
            format,
            message: e.to_string(),
        })?,
        Format::Toml => toml::from_str(text).map_err(|e| ReadError::Parse {
            format,
            message: e.to_string(),
        })?,
    };
    manifest.validate().map_err(ReadError::Validate)?;
    Ok(manifest)
}

/// Serialize a manifest as YAML — the one format the Quartermaster WRITES.
pub fn to_yaml(manifest: &Manifest) -> Result<String, String> {
    serde_norway::to_string(manifest).map_err(|e| e.to_string())
}

/// Serialize ONE contribution as the YAML block a manifest embeds — the exact text that landing this
/// recipe would write under `contributions:`. `Contribution` is internally tagged (`kind: …`), so a
/// single-item dump is a valid, self-describing block. This is the "show me what this does" the
/// Workshop shows for a recipe: the format is legible, so the UI can be honest about what it emits.
pub fn contribution_yaml(c: &manifest::Contribution) -> String {
    serde_norway::to_string(c).unwrap_or_else(|e| format!("# cannot serialize: {e}"))
}
