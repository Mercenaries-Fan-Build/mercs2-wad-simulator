//! Compatibility across a set of Shipments: requirements, versions, capabilities, declared and
//! claimed conflicts, superseded legacy files — and the load order they imply.
//!
//! One function, [`plan`], answers all of it for a request and returns the `load-plan.json` value
//! ([`crate::plan::LoadPlan`]). `qm preflight` writes that plan; `qm link` computes the same plan
//! through the same function and refuses to link unless it is `ok`.
//!
//! Every problem is a [`Finding`], never a stop: the plan is complete even when it is not ok, so a
//! caller sees every problem at once. What cannot be answered at all — an input that cannot be
//! read, a game folder that is needed and cannot be found — is a [`CompatError`] instead, and no
//! plan exists.

use crate::blast::{self, Access, MergeClass};
use crate::build::{self, ASI_SUBDIR};
use crate::discover::LoadedShipment;
use crate::link::{self, OrderEdge};
use crate::manifest::{Contribution, Manifest, PlaceIn, Requirement, Superseded};
use crate::plan::{
    CapabilityRow, ClaimClass, ClaimConflictRow, ClaimantEntry, ConflictRow, ConflictSource,
    DeclaredConflictRow, DeclaredStatus, EdgeSource, Finding, FindingRef, FindingSeverity,
    LoadPlan, PlacedDestination, PlacedFileEntry, PlanEdge, PlanItem, PluginEntry, Producer, RuntimeDllEntry,
    RequirementKind, RequirementRow, RequirementStatus, Section, SupersededRow, PLAN_FORMAT,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// The running Quartermaster's version — what `shipment.quartermaster` ranges are checked against,
/// and the plan's `quartermaster` field.
pub const QUARTERMASTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One request item as [`plan`] sees it: the caller's id and the opened Shipment.
#[derive(Debug, Clone, Copy)]
pub struct PlanInput<'a> {
    pub id: &'a str,
    pub shipment: &'a LoadedShipment,
}

/// Why no plan could be computed. Every case is "could not run" (exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatError {
    /// One item's input could not be read or used. The message never names a request path.
    Item { id: String, message: String },
    /// Some item declares `supersedes`, and no game folder was given to probe.
    GameRootRequired { id: String },
    /// The superseded-file probe could not list a game-folder directory.
    Probe { id: String, message: String },
    /// The game folder could not be derived from the resolved `vz.wad`.
    GameRoot { message: String },
}

impl std::fmt::Display for CompatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompatError::Item { id, message } => write!(f, "{id}: -: {message}"),
            CompatError::GameRootRequired { id } => write!(
                f,
                "{id}: -: this Shipment declares `supersedes`, which is checked against the game \
                 folder, and no game folder was resolved — pass --game"
            ),
            CompatError::Probe { id, message } => write!(f, "{id}: -: {message}"),
            CompatError::GameRoot { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CompatError {}

/// Find `vz.wad`: from `--game` when given (the file itself, the install root, or its `data`
/// folder — `game_paths::wad_under`'s rule), otherwise by host discovery.
///
/// `qm build`, `qm preflight` and `qm link` all resolve the game through here, so `--game` means
/// the same thing to each.
pub fn resolve_vz_wad(explicit: Option<&Path>) -> Result<PathBuf, String> {
    match explicit {
        Some(path) => mercs2_formats::game_paths::wad_under(path, "vz.wad").ok_or_else(|| {
            format!(
                "--game {}: no vz.wad there — pass the vz.wad file, the install root, or its data \
                 folder",
                path.display()
            )
        }),
        None => crate::game::discover().map(|d| d.path).ok_or_else(|| {
            "no game install found. Pass --game <dir>, or run scripts/find-vz-wad.sh --write"
                .to_string()
        }),
    }
}

/// The languages whose WADs a set of Shipments reads: the language of every sound override that
/// declares one, and English for an `add_language`, whose voice-over tables fork `English.wad`'s.
pub fn declared_languages<'a>(
    manifests: impl IntoIterator<Item = &'a Manifest>,
) -> std::collections::BTreeSet<crate::manifest::Language> {
    let mut out = std::collections::BTreeSet::new();
    for c in manifests.into_iter().flat_map(|m| m.contributions.iter()) {
        match c {
            crate::manifest::Contribution::ReplaceSoundBank { language: Some(l), .. }
            | crate::manifest::Contribution::ReplaceSoundCue { language: Some(l), .. } => {
                out.insert(*l);
            }
            crate::manifest::Contribution::AddLanguage { .. } => {
                out.insert(crate::manifest::Language::English);
            }
            _ => {}
        }
    }
    out
}

/// The WADs to open for `manifests`, in mount order: `vz_wad`, then the WAD of each declared
/// language ([`declared_languages`]) from the same folder, `<token>.wad` matched
/// case-insensitively. The engine mounts the language WAD above the level WAD
/// (`docs/fixpack/wad_duplicate_inventory.md` §B.2), and [`crate::game::GameStack`] resolves the
/// last-opened WAD first, so the stack reads as the game does. A declared language whose WAD is not
/// there is an error.
pub fn game_stack_paths<'a>(
    vz_wad: &Path,
    manifests: impl IntoIterator<Item = &'a Manifest>,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = vec![vz_wad.to_path_buf()];
    for language in declared_languages(manifests) {
        let file = format!("{}.wad", language.token());
        paths.push(crate::sound::sibling_wad(vz_wad, &file).map_err(|e| {
            format!("a contribution reads the {} WAD: {e}", language.token())
        })?);
    }
    Ok(paths)
}

/// The game folder: the parent of the `data` directory holding `vz.wad`.
///
/// The directory holding `vz.wad` must be named `data` (compared case-insensitively). Anything
/// else is refused rather than guessed at: a wrong root would make every superseded-file probe
/// look in the wrong place and report the legacy file absent.
pub fn game_root_of(vz_wad: &Path) -> Result<PathBuf, String> {
    let data = vz_wad
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", vz_wad.display()))?;
    let is_data = data
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("data"));
    if !is_data {
        return Err(format!(
            "cannot derive the game folder from the resolved vz.wad ({}): it is not in a `data` \
             directory; pass --game",
            vz_wad.display()
        ));
    }
    data.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{} has no parent directory", data.display()))
}

/// Whether any Shipment declares `supersedes`, and so needs the game folder probed.
pub fn needs_game_root<'a>(manifests: impl IntoIterator<Item = &'a Manifest>) -> bool {
    manifests.into_iter().any(|m| !m.supersedes.is_empty())
}

/// Whether a superseded legacy file is in the game folder.
///
/// The directory is LISTED and names are compared case-insensitively, because the game runs on
/// Windows, where `1_ess.lua` and `1_Ess.lua` are one file, and the host may be case-sensitive. A
/// symlink counts as present and is not followed. A destination directory that does not exist
/// holds nothing; any other listing failure is an error, never "absent".
pub fn superseded_present(game_root: &Path, entry: &Superseded) -> Result<bool, String> {
    let dir = game_root.join(entry.dest.relative_dir());
    let listing = match std::fs::read_dir(&dir) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("listing {}: {e}", dir.display())),
    };
    let wanted = entry.file.to_lowercase();
    for dirent in listing {
        let dirent = dirent.map_err(|e| format!("listing {}: {e}", dir.display()))?;
        // A name that is not UTF-8 cannot equal the (UTF-8) name being looked for.
        if let Some(name) = dirent.file_name().to_str() {
            if name.to_lowercase() == wanted {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `path` relative to the Shipment's `src/`, with forward slashes, or `None` when it is not a
/// plain relative path under `src/`.
fn src_relative(path: &Path) -> Option<String> {
    let rest = path.strip_prefix("src").ok()?;
    let mut parts = Vec::new();
    for c in rest.components() {
        match c {
            Component::Normal(p) => parts.push(p.to_str()?.to_string()),
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn error(
    code: &'static str,
    message: String,
    items: Vec<String>,
    refs: Vec<FindingRef>,
) -> Finding {
    Finding {
        code,
        severity: FindingSeverity::Error,
        message,
        items,
        refs,
        fix: None,
    }
}

fn at(section: Section, index: usize) -> FindingRef {
    FindingRef { section, index }
}

/// A requirement's target, reduced to what resolution needs.
enum Wanted<'m> {
    Shipment {
        name: &'m str,
        range: Option<&'m str>,
    },
    Capability(&'m str),
}

/// Compute the load plan for a request.
///
/// `game_root` is required when any item declares `supersedes` ([`needs_game_root`]); the probe
/// is never skipped.
pub fn plan(
    inputs: &[PlanInput<'_>],
    producer: Producer,
    game_root: Option<&Path>,
) -> Result<LoadPlan, CompatError> {
    let mut findings: Vec<Finding> = Vec::new();
    let manifest = |i: usize| &inputs[i].shipment.manifest;
    let id = |i: usize| inputs[i].id.to_string();

    // ── items ─────────────────────────────────────────────────────────────────────────────────
    let mut items = Vec::with_capacity(inputs.len());
    for (requested, input) in inputs.iter().enumerate() {
        let m = &input.shipment.manifest;
        let (plugins, runtime_dlls, placed_files) = item_files(input, requested, &mut findings)?;
        if let Some(range) = &m.shipment.quartermaster {
            let req = semver::VersionReq::parse(range).map_err(|e| CompatError::Item {
                id: id(requested),
                message: format!(
                    "internal error: shipment.quartermaster {range:?} passed validation but does not \
                     parse: {e}"
                ),
            })?;
            let running =
                semver::Version::parse(QUARTERMASTER_VERSION).expect("CARGO_PKG_VERSION is semver");
            if !req.matches(&running) {
                findings.push(error(
                    "M0210",
                    format!(
                        "{} needs Quartermaster {range}; this is {QUARTERMASTER_VERSION}",
                        m.shipment.name
                    ),
                    vec![id(requested)],
                    vec![at(Section::Items, requested)],
                ));
            }
        }
        items.push(PlanItem {
            id: id(requested),
            requested,
            resolved: None,
            held_back_by: None,
            name: m.shipment.name.clone(),
            version: m.shipment.version.clone(),
            manifest_format: m.format,
            quartermaster_range: m.shipment.quartermaster.clone(),
            provides: m.load.provides.clone(),
            plugins,
            runtime_dlls,
            placed_files,
        });
    }

    // M0203: exactly one copy per name.
    let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for i in 0..inputs.len() {
        by_name
            .entry(manifest(i).shipment.name.as_str())
            .or_default()
            .push(i);
    }
    for (name, holders) in &by_name {
        if holders.len() > 1 {
            findings.push(error(
                "M0203",
                format!(
                    "{} items are named {name}; exactly one copy of a Shipment may be in the set",
                    holders.len()
                ),
                holders.iter().map(|&i| id(i)).collect(),
                holders.iter().map(|&i| at(Section::Items, i)).collect(),
            ));
        }
    }
    let named = |name: &str| by_name.get(name).cloned().unwrap_or_default();
    let providing = |token: &str| -> Vec<usize> {
        (0..inputs.len())
            .filter(|&i| manifest(i).load.provides.iter().any(|p| p == token))
            .collect()
    };

    // ── requirements and edges ────────────────────────────────────────────────────────────────
    let mut requirements = Vec::new();
    let mut edges: Vec<PlanEdge> = Vec::new();
    let mut order_edges: Vec<OrderEdge> = Vec::new();
    let mut capability_consumers: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for consumer in 0..inputs.len() {
        let m = manifest(consumer);
        for (index, req) in m.load.requires.iter().enumerate() {
            let wanted = match req {
                Requirement::Shipment(name) => Wanted::Shipment { name, range: None },
                Requirement::ShipmentRange(r) => Wanted::Shipment {
                    name: &r.shipment,
                    range: Some(&r.version),
                },
                Requirement::Capability(c) => Wanted::Capability(&c.capability),
                Requirement::Compatible(_) => {
                    return Err(CompatError::Item {
                        id: id(consumer),
                        message: format!(
                            "internal error: load.requires[{index}] is the `{{ name, version }}` \
                             form, which validation refuses — this manifest was not validated"
                        ),
                    })
                }
            };
            let row = requirements.len();
            let (row_value, edge_from, edge_source) = match wanted {
                Wanted::Shipment { name, range } => {
                    let providers = named(name);
                    let (status, resolved_version) = match providers.as_slice() {
                        [] => (RequirementStatus::Missing, None),
                        [only] => {
                            let installed = &manifest(*only).shipment.version;
                            let satisfied = match range {
                                None => true,
                                Some(r) => range_matches(r, installed, &id(consumer), index)?,
                            };
                            let status = if satisfied {
                                RequirementStatus::Satisfied
                            } else {
                                RequirementStatus::VersionUnsatisfied
                            };
                            (status, Some(installed.clone()))
                        }
                        _ => (RequirementStatus::Ambiguous, None),
                    };
                    match status {
                        RequirementStatus::Missing => findings.push(error(
                            "M0204",
                            format!(
                                "{} requires {name}{}; no Shipment named {name} is in the set",
                                m.shipment.name,
                                range.map(|r| format!(" {r}")).unwrap_or_default()
                            ),
                            vec![id(consumer)],
                            vec![at(Section::Requirements, row)],
                        )),
                        RequirementStatus::VersionUnsatisfied => {
                            let mut who = vec![id(consumer)];
                            who.extend(providers.iter().map(|&p| id(p)));
                            findings.push(error(
                                "M0204",
                                format!(
                                    "{} requires {name} {}; the installed {name} is {}",
                                    m.shipment.name,
                                    range.unwrap_or_default(),
                                    resolved_version.as_deref().unwrap_or_default()
                                ),
                                who,
                                vec![at(Section::Requirements, row)],
                            ))
                        }
                        RequirementStatus::Satisfied | RequirementStatus::Ambiguous => {}
                    }
                    let value = RequirementRow {
                        consumer: id(consumer),
                        index,
                        kind: RequirementKind::Shipment,
                        target: name.to_string(),
                        range: range.map(str::to_string),
                        providers: providers.iter().map(|&p| id(p)).collect(),
                        resolved_version,
                        status,
                    };
                    (value, providers, EdgeSource::Requires)
                }
                Wanted::Capability(token) => {
                    let providers = providing(token);
                    capability_consumers
                        .entry(token.to_string())
                        .or_default()
                        .push(consumer);
                    let status = if providers.is_empty() {
                        findings.push(error(
                            "M0204",
                            format!(
                                "{} requires the capability {token}; no Shipment in the set \
                                 provides it",
                                m.shipment.name
                            ),
                            vec![id(consumer)],
                            vec![at(Section::Requirements, row)],
                        ));
                        RequirementStatus::Missing
                    } else {
                        RequirementStatus::Satisfied
                    };
                    let value = RequirementRow {
                        consumer: id(consumer),
                        index,
                        kind: RequirementKind::Capability,
                        target: token.to_string(),
                        range: None,
                        providers: providers.iter().map(|&p| id(p)).collect(),
                        resolved_version: None,
                        status,
                    };
                    (value, providers, EdgeSource::Capability)
                }
            };
            requirements.push(row_value);
            // One edge from each provider. A requirement with no provider orders nothing.
            for first in edge_from {
                edges.push(PlanEdge {
                    first: id(first),
                    then: id(consumer),
                    source: edge_source,
                    requirement: row,
                });
                order_edges.push(OrderEdge {
                    first,
                    then: consumer,
                });
            }
        }
    }

    let capabilities: Vec<CapabilityRow> = capability_consumers
        .into_iter()
        .map(|(token, mut consumers)| {
            consumers.dedup();
            CapabilityRow {
                providers: providing(&token).into_iter().map(id).collect(),
                consumers: consumers.into_iter().map(id).collect(),
                capability: token,
            }
        })
        .collect();

    // ── order ─────────────────────────────────────────────────────────────────────────────────
    let order = match link::resolve_load_order(inputs.len(), &order_edges) {
        Ok(resolved) => {
            for (pos, &i) in resolved.order.iter().enumerate() {
                items[i].resolved = Some(pos);
            }
            for (i, held) in resolved.held_back_by.iter().enumerate() {
                items[i].held_back_by = *held;
            }
            Some(resolved.order.into_iter().map(id).collect())
        }
        Err(cycles) => {
            for cycle in cycles {
                let mut members: Vec<usize> = cycle
                    .iter()
                    .flat_map(|&e| [order_edges[e].first, order_edges[e].then])
                    .collect();
                members.sort_unstable();
                members.dedup();
                let described: Vec<String> = cycle
                    .iter()
                    .map(|&e| {
                        format!(
                            "{} before {}",
                            manifest(order_edges[e].first).shipment.name,
                            manifest(order_edges[e].then).shipment.name
                        )
                    })
                    .collect();
                findings.push(error(
                    "M0174",
                    format!(
                        "the requirements form a cycle, so no load order exists: {}",
                        described.join(", ")
                    ),
                    members.into_iter().map(id).collect(),
                    cycle.iter().map(|&e| at(Section::Edges, e)).collect(),
                ));
            }
            None
        }
    };

    // ── conflicts ─────────────────────────────────────────────────────────────────────────────
    let mut conflicts = Vec::new();
    for declarer in 0..inputs.len() {
        let m = manifest(declarer);
        for (index, decl) in m.load.conflicts.iter().enumerate() {
            let installed = named(decl.name());
            let status = if installed.is_empty() {
                DeclaredStatus::NotInstalled
            } else {
                match decl.range() {
                    None => DeclaredStatus::Conflict,
                    Some(r) => {
                        let mut any = false;
                        for &i in &installed {
                            any |= range_matches(
                                r,
                                &manifest(i).shipment.version,
                                &id(declarer),
                                index,
                            )?;
                        }
                        if any {
                            DeclaredStatus::Conflict
                        } else {
                            DeclaredStatus::OutsideRange
                        }
                    }
                }
            };
            let row = conflicts.len();
            if status == DeclaredStatus::Conflict {
                let mut who = vec![id(declarer)];
                who.extend(installed.iter().map(|&i| id(i)));
                let versions: Vec<String> = installed
                    .iter()
                    .map(|&i| {
                        format!(
                            "{} {}",
                            manifest(i).shipment.name,
                            manifest(i).shipment.version
                        )
                    })
                    .collect();
                findings.push(error(
                    "M0206",
                    format!(
                        "{} declares it cannot be installed with {}{}; installed: {}",
                        m.shipment.name,
                        decl.name(),
                        decl.range().map(|r| format!(" {r}")).unwrap_or_default(),
                        versions.join(", ")
                    ),
                    who,
                    vec![at(Section::Conflicts, row)],
                ));
            }
            conflicts.push(ConflictRow::Declared(DeclaredConflictRow {
                source: ConflictSource::Declared,
                declarer: id(declarer),
                index,
                named: decl.name().to_string(),
                range: decl.range().map(str::to_string),
                installed: installed.into_iter().map(id).collect(),
                status,
            }));
        }
    }

    let refs: Vec<(&str, &Manifest)> = (0..inputs.len())
        .map(|i| (manifest(i).shipment.name.as_str(), manifest(i)))
        .collect();
    for c in blast::conflicts(&refs) {
        let class = match c.class {
            MergeClass::Exclusive => ClaimClass::Exclusive,
            MergeClass::KeyedSet => ClaimClass::KeyedSet,
            MergeClass::OrderedList | MergeClass::LastWins => {
                return Err(CompatError::Item {
                    id: id(0),
                    message: format!(
                        "internal error: blast::conflicts reported a {:?} claim, which never \
                         collides",
                        c.class
                    ),
                })
            }
        };
        // `blast::conflicts` groups claimants by NAME; map each back to the item(s) with that
        // name that really make this claim at that contribution.
        let mut claimants: Vec<ClaimantEntry> = Vec::new();
        let mut who: BTreeSet<usize> = BTreeSet::new();
        for claimant in &c.claimants {
            for i in named(&claimant.shipment) {
                let makes_it = blast::claims(manifest(i)).into_iter().any(|r| {
                    r.access == Access::Write && r.claim == c.claim && r.index == claimant.index
                });
                let listed = claimants
                    .iter()
                    .any(|e| e.item == id(i) && e.contribution == claimant.index);
                if !makes_it || listed {
                    continue;
                }
                who.insert(i);
                claimants.push(ClaimantEntry {
                    item: id(i),
                    contribution: claimant.index,
                    kind: manifest(i).contributions[claimant.index].kind().to_string(),
                });
            }
        }
        if claimants.is_empty() {
            return Err(CompatError::Item {
                id: id(0),
                message: format!(
                    "internal error: no item makes the conflicting claim {}",
                    c.claim.label()
                ),
            });
        }
        let row = conflicts.len();
        findings.push(error(
            "M0207",
            c.to_string(),
            who.into_iter().map(id).collect(),
            vec![at(Section::Conflicts, row)],
        ));
        conflicts.push(ConflictRow::Claims(ClaimConflictRow {
            source: ConflictSource::Claims,
            claim: c.claim.describe(c.name.as_deref()),
            class,
            claimants,
        }));
    }

    // ── supersedes ────────────────────────────────────────────────────────────────────────────
    let mut supersedes = Vec::new();
    for declarer in 0..inputs.len() {
        let m = manifest(declarer);
        if m.supersedes.is_empty() {
            continue;
        }
        let root = game_root.ok_or_else(|| CompatError::GameRootRequired { id: id(declarer) })?;
        for (index, entry) in m.supersedes.iter().enumerate() {
            let present =
                superseded_present(root, entry).map_err(|message| CompatError::Probe {
                    id: id(declarer),
                    message,
                })?;
            let relative = build::place_path(entry.dest.relative_dir(), &entry.file);
            let row = supersedes.len();
            if present {
                findings.push(error(
                    "M0208",
                    format!(
                        "{} supersedes {relative}, which is still in the game folder — remove it \
                         first (qm never deletes it)",
                        m.shipment.name
                    ),
                    vec![id(declarer)],
                    vec![at(Section::Supersedes, row)],
                ));
            }
            supersedes.push(SupersededRow {
                declared_by: id(declarer),
                index,
                dest: entry.dest,
                file: entry.file.clone(),
                relative,
                present,
            });
        }
    }

    // Sorted by code, then by the first item's request index; stable otherwise.
    crate::plan::sort_findings(&mut findings, |item| {
        inputs.iter().position(|p| p.id == item)
    });
    let ok = !findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Error);

    Ok(LoadPlan {
        format: PLAN_FORMAT,
        producer,
        quartermaster: QUARTERMASTER_VERSION.to_string(),
        ok,
        order,
        items,
        edges,
        requirements,
        capabilities,
        conflicts,
        supersedes,
        link_block_paths: build::link_block_paths(inputs.iter().map(|i| &i.shipment.manifest)),
        findings,
    })
}

/// Whether `version` falls in `range`. Both passed validation, so a parse failure here is an
/// internal error, reported as such rather than read as "outside the range".
fn range_matches(range: &str, version: &str, id: &str, index: usize) -> Result<bool, CompatError> {
    let req = semver::VersionReq::parse(range).map_err(|e| CompatError::Item {
        id: id.to_string(),
        message: format!("internal error: range {range:?} (entry {index}) passed validation but does not parse: {e}"),
    })?;
    let v = semver::Version::parse(version).map_err(|e| CompatError::Item {
        id: id.to_string(),
        message: format!(
            "internal error: version {version:?} passed validation but does not parse: {e}"
        ),
    })?;
    Ok(req.matches(&v))
}

/// An item's `plugins[]`, `runtime_dlls[]` and `placed_files[]`, with their M0162 / M0178 findings.
///
/// Reads every plugin and runtime DLL (for its digest and PE header) and checks every placed file
/// exists. A file
/// that cannot be read, or a path that is not under `src/`, is a [`CompatError`]: the plan cannot
/// describe it.
fn item_files(
    input: &PlanInput<'_>,
    requested: usize,
    findings: &mut Vec<Finding>,
) -> Result<(Vec<PluginEntry>, Vec<RuntimeDllEntry>, Vec<PlacedFileEntry>), CompatError> {
    let m = &input.shipment.manifest;
    let root = &input.shipment.root;
    let fail = |message: String| CompatError::Item {
        id: input.id.to_string(),
        message,
    };
    let refused = |contribution: usize, name: &str, why: &str| {
        error(
            "M0162",
            format!(
                "contributions[{contribution}]: {name} cannot be placed in the game folder: {why}"
            ),
            vec![input.id.to_string()],
            vec![at(Section::Items, requested)],
        )
    };
    let not_loadable = |contribution: usize, name: &str, why: &str| {
        error(
            "M0178",
            format!("contributions[{contribution}]: {name} cannot be loaded by the game: {why}"),
            vec![input.id.to_string()],
            vec![at(Section::Items, requested)],
        )
    };
    let mut plugins = Vec::new();
    let mut runtime_dlls = Vec::new();
    let mut placed = Vec::new();
    for (contribution, c) in m.contributions.iter().enumerate() {
        match c {
            Contribution::NativeHook {
                plugin: Some(plugin),
                touches,
                signature_guard,
                ..
            } => {
                let source = src_relative(plugin).ok_or_else(|| {
                    fail(format!(
                        "contributions[{contribution}] (native_hook): the plugin path is not a \
                         relative path under src/"
                    ))
                })?;
                let bytes = std::fs::read(root.join(plugin)).map_err(|e| {
                    fail(format!(
                        "contributions[{contribution}] (native_hook): reading src/{source}: {e}"
                    ))
                })?;
                let file_name = file_name_of(plugin, contribution, "native_hook").map_err(fail)?;
                let why = if !file_name.to_ascii_lowercase().ends_with(".asi") {
                    Some(
                        "it is not an `.asi`, and the loader only globs `*.asi`, so it would never \
                         be loaded"
                            .to_string(),
                    )
                } else {
                    build::game_folder_name_refusal(&file_name)
                };
                if let Some(why) = why {
                    findings.push(refused(contribution, &file_name, &why));
                }
                if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, "native_hook") {
                    findings.push(not_loadable(contribution, &file_name, &why));
                }
                plugins.push(PluginEntry {
                    contribution,
                    relative: build::place_path(ASI_SUBDIR, &file_name),
                    sha256: build::sha256_hex(&bytes),
                    file_name,
                    source,
                    touches: touches.iter().map(|t| t.0.clone()).collect(),
                    signature_guard: signature_guard.clone(),
                });
            }
            Contribution::AddRuntimeDll { dll } => {
                let source = src_relative(dll).ok_or_else(|| {
                    fail(format!(
                        "contributions[{contribution}] (add_runtime_dll): the dll path is not a \
                         relative path under src/"
                    ))
                })?;
                let bytes = std::fs::read(root.join(dll)).map_err(|e| {
                    fail(format!(
                        "contributions[{contribution}] (add_runtime_dll): reading src/{source}: {e}"
                    ))
                })?;
                let file_name = file_name_of(dll, contribution, "add_runtime_dll").map_err(fail)?;
                if let Some(why) = build::runtime_dll_name_refusal(&file_name, &m.shipment.name) {
                    findings.push(refused(contribution, &file_name, &why));
                }
                if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, "add_runtime_dll") {
                    findings.push(not_loadable(contribution, &file_name, &why));
                }
                runtime_dlls.push(RuntimeDllEntry {
                    contribution,
                    relative: build::place_path(PlaceIn::GameRoot.relative_dir(), &file_name),
                    sha256: build::sha256_hex(&bytes),
                    file_name,
                    source,
                });
            }
            Contribution::PlaceFile { file, dest } => {
                let source = src_relative(file).ok_or_else(|| {
                    fail(format!(
                        "contributions[{contribution}] (place_file): the file path is not a \
                         relative path under src/"
                    ))
                })?;
                if !root.join(file).is_file() {
                    return Err(fail(format!(
                        "contributions[{contribution}] (place_file): src/{source} does not exist"
                    )));
                }
                let file_name = file_name_of(file, contribution, "place_file").map_err(fail)?;
                if let Some(why) = build::companion_name_refusal(&file_name) {
                    findings.push(refused(contribution, &file_name, &why));
                }
                placed.push(PlacedFileEntry {
                    contribution,
                    relative: build::place_path(dest.relative_dir(), &file_name),
                    file_name,
                    source: Some(source),
                    destination: PlacedDestination::GameFolder,
                    dest: Some(*dest),
                });
            }
            Contribution::AddLanguage { name, .. } => placed.push(PlacedFileEntry {
                contribution,
                file_name: format!("{name}.wad"),
                source: None,
                destination: PlacedDestination::DataWad,
                dest: None,
                relative: format!("data/{name}.wad"),
            }),
            _ => {}
        }
    }
    Ok((plugins, runtime_dlls, placed))
}

fn file_name_of(path: &Path, contribution: usize, kind: &str) -> Result<String, String> {
    path.file_name()
        .and_then(|f| f.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            format!("contributions[{contribution}] ({kind}): the path has no UTF-8 file name")
        })
}
