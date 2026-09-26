//! `load-request.json` in, `load-plan.json` out: the machine-readable contract between qm and the
//! tool that drives it (Modkit).
//!
//! This module holds the types of both files and their I/O; [`crate::compat::plan`] fills a plan.
//!
//! Two properties every reader relies on:
//!
//! * **Every key is always present.** A value that does not apply is `null`, never an omitted key,
//!   so a reader with `deny_unknown_fields` and no defaults can refuse anything malformed.
//! * **Ids, not paths.** A request item is referred to by its `id` everywhere in the plan. No
//!   request path and no absolute path appears in it, message text included.

use crate::manifest::PlaceIn;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `load-request.json`'s own format number. Unrelated to the manifest format.
pub const REQUEST_FORMAT: u32 = 1;
/// `load-plan.json`'s own format number. Unrelated to the manifest format.
pub const PLAN_FORMAT: u32 = 1;
/// The plan's file name inside `--out`.
pub const PLAN_FILE: &str = "load-plan.json";
/// The longest request id, in UTF-8 bytes.
pub const MAX_ID_BYTES: usize = 256;

/// `load-request.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadRequest {
    pub format: u32,
    pub items: Vec<RequestItem>,
}

/// One Shipment in a request. The ORDER of items is the tie-break for the load order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestItem {
    /// Issued by the caller and opaque to qm. The plan refers to the item by this alone.
    pub id: String,
    /// The Shipment directory. Never echoed into the plan.
    pub path: PathBuf,
}

/// Why a request could not be used. Every case is exit 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    Read { file: PathBuf, message: String },
    Parse { file: PathBuf, message: String },
    Format { found: u32 },
    BadId { index: usize, why: String },
    DuplicateId { id: String },
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Read { file, message } => {
                write!(f, "reading the request {}: {message}", file.display())
            }
            RequestError::Parse { file, message } => {
                write!(
                    f,
                    "the request {} is not a load request: {message}",
                    file.display()
                )
            }
            RequestError::Format { found } => write!(
                f,
                "the request declares format {found}; the only request format is {REQUEST_FORMAT}"
            ),
            RequestError::BadId { index, why } => write!(f, "items[{index}].id {why}"),
            RequestError::DuplicateId { id } => {
                write!(
                    f,
                    "the id {id:?} is used by more than one item; ids must be unique"
                )
            }
        }
    }
}

impl std::error::Error for RequestError {}

/// Read `load-request.json` and check it. A relative item path resolves against the request
/// file's directory.
pub fn read_request(file: &Path) -> Result<Vec<RequestItem>, RequestError> {
    let text = std::fs::read_to_string(file).map_err(|e| RequestError::Read {
        file: file.to_path_buf(),
        message: e.to_string(),
    })?;
    let request: LoadRequest = serde_json::from_str(&text).map_err(|e| RequestError::Parse {
        file: file.to_path_buf(),
        message: e.to_string(),
    })?;
    if request.format != REQUEST_FORMAT {
        return Err(RequestError::Format {
            found: request.format,
        });
    }
    check_ids(&request.items)?;
    let base = file.parent().unwrap_or_else(|| Path::new(""));
    Ok(request
        .items
        .into_iter()
        .map(|item| RequestItem {
            path: if item.path.is_absolute() {
                item.path
            } else {
                base.join(item.path)
            },
            id: item.id,
        })
        .collect())
}

/// The positional form: `arg:<n>` for each directory, `n` counting from 1 in argument order.
pub fn request_from_dirs(dirs: &[PathBuf]) -> Vec<RequestItem> {
    dirs.iter()
        .enumerate()
        .map(|(i, dir)| RequestItem {
            id: format!("arg:{}", i + 1),
            path: dir.clone(),
        })
        .collect()
}

/// 1–256 UTF-8 bytes, no control characters, no `/` or `\` (so a path passed by mistake fails
/// loudly), and unique by exact bytes.
fn check_ids(items: &[RequestItem]) -> Result<(), RequestError> {
    let mut seen = std::collections::BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let id = &item.id;
        let why = if id.is_empty() {
            Some("is empty".to_string())
        } else if id.len() > MAX_ID_BYTES {
            Some(format!(
                "is {} bytes; the limit is {MAX_ID_BYTES}",
                id.len()
            ))
        } else if id.chars().any(char::is_control) {
            Some("contains a control character".to_string())
        } else if id.contains('/') || id.contains('\\') {
            Some("contains `/` or `\\` — an id is not a path".to_string())
        } else {
            None
        };
        if let Some(why) = why {
            return Err(RequestError::BadId { index, why });
        }
        if !seen.insert(id.as_str()) {
            return Err(RequestError::DuplicateId { id: id.clone() });
        }
    }
    Ok(())
}

/// Which command wrote the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Producer {
    Preflight,
    Link,
}

/// `load-plan.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadPlan {
    pub format: u32,
    pub producer: Producer,
    /// The qm that wrote it (`CARGO_PKG_VERSION`).
    pub quartermaster: String,
    /// `true` exactly when no finding has severity `error`.
    pub ok: bool,
    /// Request ids in load order. `None` exactly when a `requires` cycle (M0174) exists.
    pub order: Option<Vec<String>>,
    pub items: Vec<PlanItem>,
    pub edges: Vec<PlanEdge>,
    pub requirements: Vec<RequirementRow>,
    pub capabilities: Vec<CapabilityRow>,
    pub conflicts: Vec<ConflictRow>,
    pub supersedes: Vec<SupersededRow>,
    /// The PTHS path of every block `qm link` re-emits: the scripts blocks, then each merged string
    /// table's block. A deploy step drops the per-Shipment copies of exactly these.
    pub link_block_paths: Vec<String>,
    pub findings: Vec<Finding>,
}

/// One request item, in request order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    pub id: String,
    /// 0-based index in the request.
    pub requested: usize,
    /// Index in `order`; `None` exactly when `order` is.
    pub resolved: Option<usize>,
    /// Index into `edges` of the edge that last delayed this item, when an item with a larger
    /// request index loaded before it.
    pub held_back_by: Option<usize>,
    pub name: String,
    pub version: String,
    pub manifest_format: u32,
    pub quartermaster_range: Option<String>,
    pub provides: Vec<String>,
    pub plugins: Vec<PluginEntry>,
    pub runtime_dlls: Vec<RuntimeDllEntry>,
    pub placed_files: Vec<PlacedFileEntry>,
}

/// One `native_hook` that ships a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginEntry {
    pub contribution: usize,
    pub file_name: String,
    /// `src/`-relative, forward slashes.
    pub source: String,
    /// `scripts/<file_name>`.
    pub relative: String,
    /// Lowercase hex.
    pub sha256: String,
    /// The exe addresses this plugin patches (`0xHHHHHHHH`), carried from the manifest's `touches`
    /// so a loader knows what it hooks without disassembling the plugin. Empty when none declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<String>,
    /// Expected prologue bytes per hooked address (`touches` address → space-separated hex), so the
    /// plugin can verify the exe has not shifted under it before patching, and M0199 can check the
    /// declaration against a known build. Empty when the author declared no guards.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub signature_guard: std::collections::BTreeMap<String, String>,
}

/// One runtime DLL placed in the game root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeDllEntry {
    pub contribution: usize,
    pub file_name: String,
    pub source: String,
    pub relative: String,
    pub sha256: String,
}

/// Where a placed file goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacedDestination {
    GameFolder,
    DataWad,
}

/// One `place_file`, or one `add_language` WAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlacedFileEntry {
    pub contribution: usize,
    pub file_name: String,
    /// `src/`-relative; `None` for `add_language`, which generates its WAD.
    pub source: Option<String>,
    pub destination: PlacedDestination,
    /// `None` for `data_wad`.
    pub dest: Option<PlaceIn>,
    pub relative: String,
}

/// What produced an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSource {
    Requires,
    Capability,
}

/// One ordering edge: `first` (the provider) loads before `then` (the consumer, which declared it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanEdge {
    pub first: String,
    pub then: String,
    pub source: EdgeSource,
    /// Index into `requirements`.
    pub requirement: usize,
}

/// The kind of a `load.requires` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Shipment,
    Capability,
}

/// How a requirement resolved against the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Satisfied,
    Missing,
    VersionUnsatisfied,
    /// More than one item carries the name (M0203).
    Ambiguous,
}

/// One `load.requires` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequirementRow {
    pub consumer: String,
    /// Position in the consumer's `load.requires`.
    pub index: usize,
    pub kind: RequirementKind,
    /// A Shipment name or a capability token.
    pub target: String,
    /// `None` for a bare name and for a capability.
    pub range: Option<String>,
    pub providers: Vec<String>,
    /// Set when a Shipment requirement has exactly one provider.
    pub resolved_version: Option<String>,
    pub status: RequirementStatus,
}

/// One capability token some item requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRow {
    pub capability: String,
    pub consumers: Vec<String>,
    pub providers: Vec<String>,
}

/// Where a conflict row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSource {
    Declared,
    Claims,
}

/// A declared conflict's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredStatus {
    Conflict,
    OutsideRange,
    NotInstalled,
}

/// The merge class of a claim-graph conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimClass {
    Exclusive,
    KeyedSet,
}

/// One conflict row: a `load.conflicts` entry, or a claim-graph collision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ConflictRow {
    Declared(DeclaredConflictRow),
    Claims(ClaimConflictRow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredConflictRow {
    /// Always [`ConflictSource::Declared`].
    pub source: ConflictSource,
    pub declarer: String,
    /// Position in the declarer's `load.conflicts`.
    pub index: usize,
    pub named: String,
    pub range: Option<String>,
    pub installed: Vec<String>,
    pub status: DeclaredStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaimConflictRow {
    /// Always [`ConflictSource::Claims`].
    pub source: ConflictSource,
    pub claim: String,
    pub class: ClaimClass,
    pub claimants: Vec<ClaimantEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaimantEntry {
    pub item: String,
    pub contribution: usize,
    pub kind: String,
}

/// One top-level `supersedes` entry and whether the file is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupersededRow {
    pub declared_by: String,
    pub index: usize,
    pub dest: PlaceIn,
    pub file: String,
    pub relative: String,
    pub present: bool,
}

/// A finding's severity.
///
/// A load plan and a range report use only `warning` and `error`. `lint-report.json` uses all four,
/// because it carries lint's own levels ([`crate::lint::Severity`]); it is one finding shape, not
/// three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
    Error,
    Hang,
}

impl FindingSeverity {
    /// The wire spelling, for text output.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingSeverity::Info => "info",
            FindingSeverity::Warning => "warning",
            FindingSeverity::Error => "error",
            FindingSeverity::Hang => "hang",
        }
    }
}

/// The section a finding points into. The first five are load-plan sections; `contributions` is the
/// manifest's list (`lint-report.json` only) and `ranges` the command's arguments
/// (`range-report.json` only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Items,
    Edges,
    Requirements,
    Conflicts,
    Supersedes,
    Contributions,
    Ranges,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingRef {
    pub section: Section,
    pub index: usize,
}

/// The one list of problems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: FindingSeverity,
    /// Never contains an absolute path.
    pub message: String,
    pub items: Vec<String>,
    pub refs: Vec<FindingRef>,
    /// Exact replacement text when the fix is mechanical. Only `lint` fills it; a plan and a range
    /// report always write `null`.
    pub fix: Option<String>,
}

/// The `format` of `lint-report.json` and `range-report.json`. Unrelated to the manifest format.
pub const REPORT_FORMAT: u32 = 1;

/// `lint-report.json`: what `qm lint --report` writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LintReport<'a> {
    pub format: u32,
    /// Always `"lint"`.
    pub producer: &'static str,
    pub quartermaster: &'static str,
    /// `true` exactly when lint exits 0: no finding at `error` or `hang`.
    pub ok: bool,
    /// The parsed manifest, as serde writes qm's model.
    pub manifest: &'a crate::manifest::Manifest,
    pub findings: Vec<Finding>,
}

/// `range-report.json`: what `qm check-range --report` writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RangeReport {
    pub format: u32,
    /// Always `"check-range"`.
    pub producer: &'static str,
    pub quartermaster: &'static str,
    /// `true` exactly when every range parses.
    pub ok: bool,
    pub findings: Vec<Finding>,
}

/// Sort findings the one way every report sorts them: by code, then by the first item's request
/// index (`requested_of`), stable otherwise.
pub fn sort_findings(findings: &mut [Finding], requested_of: impl Fn(&str) -> Option<usize>) {
    findings.sort_by(|a, b| {
        a.code.cmp(b.code).then_with(|| {
            let fa = a.items.first().and_then(|i| requested_of(i));
            let fb = b.items.first().and_then(|i| requested_of(i));
            fa.cmp(&fb)
        })
    });
}

/// Delete `<out>/load-plan.json` if it exists, so a failed run never leaves a stale plan that reads
/// as this run's.
pub fn remove_stale(out: &Path) -> Result<(), String> {
    remove_stale_file(&out.join(PLAN_FILE))
}

/// Delete `path` if it exists. Used before a report is produced, so a run that cannot write one
/// never leaves an older report that reads as this run's.
pub fn remove_stale_file(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("removing the stale {}: {e}", path.display())),
    }
}

/// Write `<out>/load-plan.json` through `<out>/load-plan.json.tmp` and a rename. On any failure the
/// tmp file is removed and nothing is left at the plan's path.
pub fn write_plan(out: &Path, plan: &LoadPlan) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    write_json(&out.join(PLAN_FILE), plan, "the load plan")
}

/// Write `value` as pretty JSON to `path` through `<path>.tmp` and a rename. On any failure the tmp
/// file is removed and nothing is left at `path`. `what` names the document in errors.
pub fn write_json<T: Serialize>(path: &Path, value: &T, what: &str) -> Result<(), String> {
    let text =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialising {what}: {e}"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("{}: not a file path", path.display()))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let written = std::fs::write(&tmp, text + "\n")
        .map_err(|e| format!("writing {}: {e}", tmp.display()))
        .and_then(|()| {
            std::fs::rename(&tmp, &path)
                .map_err(|e| format!("renaming {} to {}: {e}", tmp.display(), path.display()))
        });
    written.map_err(|err| match std::fs::remove_file(&tmp) {
        Ok(()) => err,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => err,
        Err(e) => format!("{err}; and removing {} failed too: {e}", tmp.display()),
    })
}
