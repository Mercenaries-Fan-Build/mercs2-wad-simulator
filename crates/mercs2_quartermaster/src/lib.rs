//! `mercs2_quartermaster` — the Shipment format: read, validate, lint, and build.
//!
//! A **Shipment** is a mod package; the **Quartermaster** is the engine that works it. Neither the
//! Workshop nor Modkit owns the format — this crate does, and both are clients.
//!
//! Spec: `.claude/plans/workshop-mods-rebuild-04-dossier-format.md` (DRAFT — not frozen).
//!
//! ## What is here
//!
//! The full pipeline: the manifest model and cross-format read path ([`manifest`], [`discover`]),
//! [`lint`] rules, [`blast`]-radius (claim/conflict) computation, and the [`build`] path with its
//! [`link`]er. The read path leads on an internally-tagged enum across YAML/JSON/TOML — the schema
//! risk that shaped the format.
//!
//! ## What is deliberately NOT here
//!
//! * **Game-path discovery.** This crate is path-in, never path-discovering: the WAD stack arrives
//!   as an argument (exactly as `mercs2_workshop::publish::publish_in_background` already takes
//!   `wad_paths`). Resolution is the HOST's job — a Workshop Settings page, `qm --game`, or nothing
//!   at all in CI. Everything in [`manifest`] runs with no game present, which is what makes
//!   lint-only CI possible for the template repo.

pub mod blast;
pub mod build;
pub mod compat;
pub mod discover;
pub mod game;
pub mod link;
pub mod lint;
pub mod manifest;
pub mod names;
pub mod pe;
pub mod plan;
pub mod sound;
pub mod states;
pub mod world;

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
    CapabilityReq, CompatibleReq, ConflictDecl, Contribution, Layer, Load, Manifest, PlaceIn,
    Requirement, Retarget, Shipment, ShipmentReq, Superseded, Target, Textures, Touch,
    ValidateError, DENY_LISTED_DLL_STEMS, FORMAT_VERSION, MAX_NAME_LEN,
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
    /// `contributions[index]` names a kind the format no longer has
    /// ([`Contribution::REMOVED_KINDS`]).
    RemovedKind {
        index: usize,
        kind: &'static str,
        reason: &'static str,
    },
    Validate(ValidateError),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Parse { format, message } => {
                write!(f, "parsing manifest as {format:?}: {message}")
            }
            ReadError::RemovedKind {
                index,
                kind,
                reason,
            } => write!(
                f,
                "contributions[{index}]: kind `{kind}` has been removed from the format: {reason}"
            ),
            ReadError::Validate(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Parse a manifest from source text in a known format, then [`Manifest::validate`] it.
///
/// One `serde` model backs all three formats; this function is only the format dispatch.
pub fn from_str(text: &str, format: Format) -> Result<Manifest, ReadError> {
    let parsed: Result<Manifest, String> = match format {
        Format::Yaml => serde_norway::from_str(text).map_err(|e| e.to_string()),
        Format::Json => serde_json::from_str(text).map_err(|e| e.to_string()),
        Format::Toml => toml::from_str(text).map_err(|e| e.to_string()),
    };
    let manifest = match parsed {
        Ok(m) => m,
        Err(message) => {
            // A removed kind can only ever fail the typed parse, so it is looked for only then —
            // and reported in place of serde's "unknown variant", which reads like a typo.
            if let Some((index, kind, reason)) = removed_kind(text, format) {
                return Err(ReadError::RemovedKind {
                    index,
                    kind,
                    reason,
                });
            }
            return Err(ReadError::Parse { format, message });
        }
    };
    manifest.validate().map_err(ReadError::Validate)?;
    Ok(manifest)
}

/// The first contribution whose `kind` is in [`Contribution::REMOVED_KINDS`], read from the text
/// as an untyped document. `None` when the text does not parse even untyped, or names none.
fn removed_kind(text: &str, format: Format) -> Option<(usize, &'static str, &'static str)> {
    let doc: serde_json::Value = match format {
        Format::Yaml => serde_norway::from_str(text).ok()?,
        Format::Json => serde_json::from_str(text).ok()?,
        Format::Toml => toml::from_str(text).ok()?,
    };
    let contributions = doc.get("contributions")?.as_array()?;
    contributions.iter().enumerate().find_map(|(index, c)| {
        let kind = c.get("kind")?.as_str()?;
        Contribution::REMOVED_KINDS
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(k, reason)| (index, *k, *reason))
    })
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
