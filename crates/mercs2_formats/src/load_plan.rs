//! `load-plan.json`: what the Quartermaster (`mercs2_quartermaster`) decided about a set of
//! Shipments — the load order, each Shipment's placed files, the conflicts, and the libraries its
//! link loads in the game, at which hook.
//!
//! qm writes it (`qm preflight`, `qm link`; `qm link` also places a copy in the game folder,
//! [`GAME_FOLDER_RELATIVE`]). The types live here, outside qm, so that a program reading a plan
//! parses the one definition qm writes.
//!
//! Three properties every reader relies on:
//!
//! * **Every key is always present**, except a plugin entry's `touches` and `signature_guard`, which
//!   are left out when empty and read as empty when absent. Any other value that does not apply is
//!   `null`, never an omitted key, so a reader can refuse anything malformed: every struct here denies
//!   unknown fields, and a reader calls [`LoadPlan::check`] after parsing.
//! * **Ids, not paths.** A request item is referred to by its `id` everywhere in the plan. No
//!   request path and no absolute path appears in it, message text included.
//! * **`format` is checked.** A plan of another format is refused ([`LoadPlan::check`]), never read as
//!   this one.
//!
//! The parse itself is the reader's (`serde_json::from_str::<LoadPlan>`).

use serde::{Deserialize, Serialize};

/// `load-plan.json`'s own format number. Unrelated to the manifest format. Format 3 added
/// [`LoadPlan::libraries`].
pub const PLAN_FORMAT: u32 = 3;
/// The plan's file name inside qm's `--out`.
pub const PLAN_FILE: &str = "load-plan.json";
/// Where `qm link` places its plan in the game folder (a `game_folder` placement), with forward
/// slashes: in `scripts/`, beside `lua_bridge.asi`, whose startup check reads it.
pub const GAME_FOLDER_RELATIVE: &str = "scripts/load-plan.json";

/// Deserialize an `Option` field whose key must be present: serde's derive reads a missing `Option`
/// key as `None`, which would let a plan with a key left out read as one that wrote `null`.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

/// Which command wrote the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Producer {
    Preflight,
    Link,
}

/// `load-plan.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadPlan {
    pub format: u32,
    pub producer: Producer,
    /// The qm that wrote it (`CARGO_PKG_VERSION`).
    pub quartermaster: String,
    /// `true` exactly when no finding has severity `error`.
    pub ok: bool,
    /// Request ids in load order. `None` exactly when a `requires` cycle (M0174) exists.
    #[serde(deserialize_with = "present")]
    pub order: Option<Vec<String>>,
    pub items: Vec<PlanItem>,
    pub edges: Vec<PlanEdge>,
    pub requirements: Vec<RequirementRow>,
    pub capabilities: Vec<CapabilityRow>,
    pub conflicts: Vec<ConflictRow>,
    pub supersedes: Vec<SupersededRow>,
    /// The PTHS path of every block `qm link` re-emits: the `vz.wad` scripts blocks, the `shell.wad`
    /// scripts block (`blocks\Shell\resident_P000_Q3.block`), then each merged string table's
    /// block, then each merged sound bank's block, then — when any Shipment has an `add_fx` or a
    /// `replace_fx` — the effects block. A deploy step drops the per-Shipment copies of exactly
    /// these.
    pub link_block_paths: Vec<String>,
    /// Every game data file `qm link` emits for the set (`data/shader3.bin`, `data/shader3Low.bin`):
    /// both stores when any Shipment has a shader kind. A deploy step drops the per-Shipment
    /// `data_file` placements of exactly these and deploys link's.
    pub link_file_paths: Vec<DataFile>,
    /// Every script module the link adds to the game, in load order within each session: what qm's
    /// loader loads at which hook, and what is only imported. Filled by `qm link` from the list it
    /// spliced; `None` from `qm preflight`, which links nothing, and from a plan that refused the link.
    #[serde(deserialize_with = "present")]
    pub libraries: Option<Vec<LibraryRow>>,
    pub findings: Vec<Finding>,
}

impl LoadPlan {
    /// Refuse a plan this definition does not describe: another `format`, or a conflict row whose
    /// `source` names the other shape. Every reader calls this after parsing.
    pub fn check(&self) -> Result<(), String> {
        if self.format != PLAN_FORMAT {
            return Err(format!(
                "the load plan declares format {}; this reader reads format {PLAN_FORMAT}",
                self.format
            ));
        }
        for (i, row) in self.conflicts.iter().enumerate() {
            let (want, found) = match row {
                ConflictRow::Declared(r) => (ConflictSource::Declared, r.source),
                ConflictRow::Claims(r) => (ConflictSource::Claims, r.source),
            };
            if want != found {
                return Err(format!(
                    "conflicts[{i}] has the {want:?} shape but its source is {found:?}"
                ));
            }
        }
        Ok(())
    }
}

/// One script module a link adds ([`LoadPlan::libraries`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryRow {
    /// The module name: what `import` and `dynamic_import` take.
    pub module: String,
    /// The request id of the Shipment that adds it; `None` for the Quartermaster's own loaders
    /// (`qm_modloader`, `qm_shell_modloader`).
    #[serde(deserialize_with = "present")]
    pub item: Option<String>,
    /// The session whose WAD carries it and whose Lua VM loads it.
    pub session: LoadSession,
    /// The hook qm's loader `dynamic_import`s it at; `None` when nothing loads it but an `import`.
    #[serde(deserialize_with = "present")]
    pub when: Option<String>,
    /// Whether the session's settle hook (`world.begin`, `shell.open`) checks it is ready: true
    /// exactly for a library its entry hook requests. Always false when `when` is `None`.
    pub settles: bool,
}

/// One request item, in request order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanItem {
    pub id: String,
    /// 0-based index in the request.
    pub requested: usize,
    /// Index in `order`; `None` exactly when `order` is.
    #[serde(deserialize_with = "present")]
    pub resolved: Option<usize>,
    /// Index into `edges` of the edge that last delayed this item, when an item with a larger
    /// request index loaded before it.
    #[serde(deserialize_with = "present")]
    pub held_back_by: Option<usize>,
    pub name: String,
    pub version: String,
    pub manifest_format: u32,
    #[serde(deserialize_with = "present")]
    pub quartermaster_range: Option<String>,
    pub provides: Vec<String>,
    pub plugins: Vec<PluginEntry>,
    pub runtime_dlls: Vec<RuntimeDllEntry>,
    pub placed_files: Vec<PlacedFileEntry>,
    /// The game data files this Shipment's contributions edit: both shader stores when it has a
    /// shader kind. Its build reads their originals from `--original-data`.
    pub data_files: Vec<DataFile>,
}

/// One `native_hook` that ships a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDllEntry {
    pub contribution: usize,
    pub file_name: String,
    pub source: String,
    pub relative: String,
    pub sha256: String,
}

/// Where a placed file goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacedDestination {
    GameFolder,
    DataWad,
}

/// One `place_file`, or one WAD of an `add_language` or `add_level`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedFileEntry {
    pub contribution: usize,
    pub file_name: String,
    /// `src/`-relative; `None` for `add_language` and `add_level`, which generate their WADs.
    #[serde(deserialize_with = "present")]
    pub source: Option<String>,
    pub destination: PlacedDestination,
    /// `None` for `data_wad`.
    #[serde(deserialize_with = "present")]
    pub dest: Option<PlaceIn>,
    pub relative: String,
}

/// What produced an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSource {
    Requires,
    Capability,
}

/// One ordering edge: `first` (the provider) loads before `then` (the consumer, which declared it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanEdge {
    pub first: String,
    pub then: String,
    pub source: EdgeSource,
    /// Index into `requirements`.
    pub requirement: usize,
}

/// The kind of a `load.requires` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Shipment,
    Capability,
}

/// How a requirement resolved against the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Satisfied,
    Missing,
    VersionUnsatisfied,
    /// More than one item carries the name (M0203).
    Ambiguous,
}

/// One `load.requires` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementRow {
    pub consumer: String,
    /// Position in the consumer's `load.requires`.
    pub index: usize,
    pub kind: RequirementKind,
    /// A Shipment name or a capability token.
    pub target: String,
    /// `None` for a bare name and for a capability.
    #[serde(deserialize_with = "present")]
    pub range: Option<String>,
    pub providers: Vec<String>,
    /// Set when a Shipment requirement has exactly one provider.
    #[serde(deserialize_with = "present")]
    pub resolved_version: Option<String>,
    pub status: RequirementStatus,
}

/// One capability token some item requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRow {
    pub capability: String,
    pub consumers: Vec<String>,
    pub providers: Vec<String>,
}

/// Where a conflict row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSource {
    Declared,
    Claims,
}

/// A declared conflict's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredStatus {
    Conflict,
    OutsideRange,
    NotInstalled,
}

/// The merge class of a claim-graph conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimClass {
    Exclusive,
    KeyedSet,
}

/// One conflict row: a `load.conflicts` entry, or a claim-graph collision. The two shapes share no
/// field but `source`, and each denies the other's fields, so a row parses as exactly one;
/// [`LoadPlan::check`] refuses a row whose `source` names the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConflictRow {
    Declared(DeclaredConflictRow),
    Claims(ClaimConflictRow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredConflictRow {
    /// Always [`ConflictSource::Declared`].
    pub source: ConflictSource,
    pub declarer: String,
    /// Position in the declarer's `load.conflicts`.
    pub index: usize,
    pub named: String,
    #[serde(deserialize_with = "present")]
    pub range: Option<String>,
    pub installed: Vec<String>,
    pub status: DeclaredStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimConflictRow {
    /// Always [`ConflictSource::Claims`].
    pub source: ConflictSource,
    pub claim: String,
    pub class: ClaimClass,
    pub claimants: Vec<ClaimantEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimantEntry {
    pub item: String,
    pub contribution: usize,
    pub kind: String,
}

/// One top-level `supersedes` entry and whether the file is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
/// because it carries lint's own levels; it is one finding shape, not three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingRef {
    pub section: Section,
    pub index: usize,
}

/// The one list of problems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub code: String,
    pub severity: FindingSeverity,
    /// Never contains an absolute path.
    pub message: String,
    pub items: Vec<String>,
    pub refs: Vec<FindingRef>,
    /// Exact replacement text when the fix is mechanical. Only `lint` fills it; a plan and a range
    /// report always write `null`.
    #[serde(deserialize_with = "present")]
    pub fix: Option<String>,
}

/// A session of the game that loads sound banks from Lua, and so a place a mod loader loads a bank.
///
/// Each session is one level WAD's Lua VM (the VM is closed and recreated on every level swap,
/// `scripting_host_binding_code_map.md`):
///
/// * `gameplay` — the `vz` level. Retail loads its banks in `MrxSoundBootstrap.LoadBanks`
///   (`resident/mrxsoundbootstrap.lua:192-246`) and unloads them in `ExitGame` (`:188-190`); the
///   mod loader loads at qm's `world.begin` hook and unloads at `world.sound_exit`, after `ExitGame`.
///   Its blocks ship in the Shipment overlay.
/// * `front_end` — the `shell` level (the main menu). Retail loads its banks in
///   `MrxSound.EnterShellState` (`shell/mrxsound.lua:5-15`) and unloads them in `ExitShellState`
///   (`:17-27`); the front-end loader runs after each, at qm's `shell.open` and `shell.sound_exit`
///   hooks. Its blocks ship in the shell patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadSession {
    Gameplay,
    FrontEnd,
}

impl LoadSession {
    /// Both sessions, in this order.
    pub const ALL: [LoadSession; 2] = [LoadSession::Gameplay, LoadSession::FrontEnd];

    /// The manifest spelling.
    pub const fn token(self) -> &'static str {
        match self {
            LoadSession::Gameplay => "gameplay",
            LoadSession::FrontEnd => "front_end",
        }
    }
}

/// Where a `place_file` contribution puts its file — a **closed set of named destinations**, never
/// a path.
///
/// This enum IS the design of the manifest's `place_file`. `native_hook` has no destination field
/// at all, which is what makes `Mercenaries2.exe` and `data\vz.wad` unreachable by CONSTRUCTION
/// rather than by a lint somebody could suppress. A kind that places companion files had to keep
/// that property while admitting more than one destination, and the only way to hold both is to let
/// the author pick a NAME out of a fixed list instead of writing a path.
///
/// So there is no spelling of `dest:` that is a path. `..`, `/etc/passwd`, `C:\Windows`,
/// `\\host\share` and a symlink are not *rejected* — they do not parse, and serde says which
/// variants exist. The destination half of a placement carries no author bytes whatsoever; only the
/// filename does, and that comes from the source file (see qm's `build::game_folder_name_refusal`).
///
/// **The four ASI roots are measured, not assumed.** `pmc_bb.dll` v3.0.0 carries the format strings
/// `%s*.asi`, `%sscripts\`, `%splugins\` and `%supdate\`, so the loader globs the game directory
/// itself plus exactly those three subfolders. A destination outside that set would put a plugin's
/// companion where nothing looks for it.
///
/// ⚠ **The three `scripts/On*` rungs rest on weaker evidence, and are recorded as weaker.** They
/// come from Plan 03's write-up of Wally's Lua bridge — "script loader (`OnBoot/`/`OnLoad/`
/// (world-load-triggered)/`OnKey/`)" — which is prose about a repo this workspace does not vendor.
/// Nobody here has read the directory scan that consumes them, so the exact spelling and the parent
/// directory are inferred. They sit under `scripts/` because that is where the bridge `.asi` itself
/// goes, and because every companion path measured in this ecosystem so far resolves against the
/// loading module's OWN directory rather than the game root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceIn {
    /// The game directory itself — where `Mercenaries2.exe` lives. The loader globs `%s*.asi` here.
    GameRoot,
    /// `scripts\` — where the ecosystem already puts plugins and their companions.
    Scripts,
    /// `plugins\`.
    Plugins,
    /// `update\`.
    Update,
    /// The Lua bridge's boot-time script rung.
    OnBoot,
    /// The Lua bridge's world-load script rung.
    OnLoad,
    /// The Lua bridge's key-binding script rung.
    OnKey,
}

impl PlaceIn {
    /// The directory this destination names, relative to the game folder, with forward slashes.
    ///
    /// Forward slashes on purpose: the loader's own literals are backslashed (`%sscripts\`), but
    /// this is a filesystem path a deploy tool joins, not an engine path like a backslashed `PTHS`
    /// entry. The game root is the empty string, so joining is uniform.
    ///
    /// Every arm is a literal. Nothing an author writes reaches this string.
    pub const fn relative_dir(self) -> &'static str {
        match self {
            PlaceIn::GameRoot => "",
            PlaceIn::Scripts => "scripts",
            PlaceIn::Plugins => "plugins",
            PlaceIn::Update => "update",
            PlaceIn::OnBoot => "scripts/OnBoot",
            PlaceIn::OnLoad => "scripts/OnLoad",
            PlaceIn::OnKey => "scripts/OnKey",
        }
    }

    /// Every destination, so a test can assert a property of the whole set rather than of the
    /// arms somebody remembered to list.
    pub const ALL: [PlaceIn; 7] = [
        PlaceIn::GameRoot,
        PlaceIn::Scripts,
        PlaceIn::Plugins,
        PlaceIn::Update,
        PlaceIn::OnBoot,
        PlaceIn::OnLoad,
        PlaceIn::OnKey,
    ];
}

/// A game data file a Shipment edits, relative to the game folder. A closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DataFile {
    #[serde(rename = "data/shader3.bin")]
    Shader3,
    #[serde(rename = "data/shader3Low.bin")]
    Shader3Low,
}

impl DataFile {
    pub const ALL: [DataFile; 2] = [DataFile::Shader3, DataFile::Shader3Low];

    pub fn relative(self) -> &'static str {
        match self {
            DataFile::Shader3 => "data/shader3.bin",
            DataFile::Shader3Low => "data/shader3Low.bin",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            DataFile::Shader3 => "shader3.bin",
            DataFile::Shader3Low => "shader3Low.bin",
        }
    }

    /// Whether this is the store the engine reads with ShaderLevel off (`_3l.sho` ids).
    pub fn is_low(self) -> bool {
        self == DataFile::Shader3Low
    }

    pub fn parse(relative: &str) -> Option<DataFile> {
        DataFile::ALL.into_iter().find(|d| d.relative() == relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> LoadPlan {
        LoadPlan {
            format: PLAN_FORMAT,
            producer: Producer::Link,
            quartermaster: "9.9.9".into(),
            ok: true,
            order: Some(vec!["arg:1".into()]),
            items: Vec::new(),
            edges: Vec::new(),
            requirements: Vec::new(),
            capabilities: Vec::new(),
            conflicts: vec![
                ConflictRow::Declared(DeclaredConflictRow {
                    source: ConflictSource::Declared,
                    declarer: "arg:1".into(),
                    index: 0,
                    named: "other".into(),
                    range: None,
                    installed: Vec::new(),
                    status: DeclaredStatus::NotInstalled,
                }),
                ConflictRow::Claims(ClaimConflictRow {
                    source: ConflictSource::Claims,
                    claim: "c".into(),
                    class: ClaimClass::Exclusive,
                    claimants: Vec::new(),
                }),
            ],
            supersedes: Vec::new(),
            link_block_paths: Vec::new(),
            link_file_paths: vec![DataFile::Shader3],
            libraries: Some(vec![
                LibraryRow {
                    module: "qm_modloader".into(),
                    item: None,
                    session: LoadSession::Gameplay,
                    when: Some("world.scripts_loaded".into()),
                    settles: true,
                },
                LibraryRow {
                    module: "helper".into(),
                    item: Some("arg:1".into()),
                    session: LoadSession::Gameplay,
                    when: None,
                    settles: false,
                },
            ]),
            findings: Vec::new(),
        }
    }

    #[test]
    fn a_plan_reads_back_as_written() {
        let p = plan();
        let text = serde_json::to_string_pretty(&p).unwrap();
        let back: LoadPlan = serde_json::from_str(&text).unwrap();
        assert_eq!(back, p);
        back.check().unwrap();
        // A value that does not apply is written as null, never left out.
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let rows = v["libraries"].as_array().unwrap();
        assert!(rows[0].as_object().unwrap().get("item").is_some_and(|x| x.is_null()));
        assert!(rows[1].as_object().unwrap().get("when").is_some_and(|x| x.is_null()));
        v["libraries"] = serde_json::Value::Null;
        let none: LoadPlan = serde_json::from_value(v).unwrap();
        assert_eq!(none.libraries, None);
    }

    #[test]
    fn a_plan_with_an_unknown_or_missing_key_or_another_format_is_refused() {
        let text = serde_json::to_string(&plan()).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["libraries"][0]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<LoadPlan>(v).is_err(), "an unknown key in a row is refused");

        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v.as_object_mut().unwrap().remove("libraries");
        assert!(serde_json::from_value::<LoadPlan>(v).is_err(), "a missing key is refused, not defaulted");

        let mut p = plan();
        p.format = 2;
        let err = p.check().unwrap_err();
        assert!(err.contains("format 2") && err.contains("format 3"), "{err}");
    }

    #[test]
    fn a_conflict_row_whose_source_names_the_other_shape_is_refused() {
        let mut p = plan();
        if let ConflictRow::Claims(r) = &mut p.conflicts[1] {
            r.source = ConflictSource::Declared;
        }
        let back: LoadPlan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        let err = back.check().unwrap_err();
        assert!(err.contains("conflicts[1]"), "{err}");
    }
}
