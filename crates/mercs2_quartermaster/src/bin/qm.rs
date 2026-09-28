//! `qm` — the Quartermaster CLI.
//!
//! Three audiences, and they do not overlap much:
//!
//! - **A modder's machine**, where the retail WADs exist and `qm build` can lower for real.
//! - **A template repo's CI**, where they never will. `qm lint` is hermetic on purpose — manifest
//!   text plus the Shipment directory, no game, no network — so a public runner can gate every push.
//! - **A deploy step**, which runs `qm preflight` over the whole installed set (requirements,
//!   versions, conflicts, superseded files and the load order, written to `load-plan.json`), then
//!   `qm link` across it, because Lua scripts load from a block rather than per-hash and two
//!   script-touching Shipments would otherwise silently annihilate each other.
//!
//! ## Exit codes
//!
//! The standing mandate is that a build is gated on **exit code, never on a printed count**. A
//! caller that ignores stdout must still be unable to ship a broken Shipment.
//!
//! ```text
//! 0  clean (warnings may have been printed)
//! 1  findings at Error or above — including every HANG-class rule
//! 2  the command could not run at all (no manifest, no game stack, bad usage)
//! ```
//!
//! 1 and 2 are distinct because CI wants to tell "this Shipment is wrong" from "this runner is
//! misconfigured", and a single nonzero code conflates a real finding with a missing game folder.

use clap::{Parser, Subcommand};
use mercs2_quartermaster::compat::{self, PlanInput};
use mercs2_quartermaster::discover::{DiscoverError, OpenError};
use mercs2_quartermaster::plan::{self, FindingSeverity, LoadPlan, Producer, RequestItem};
use mercs2_quartermaster::{
    build, lint, open_shipment, BuildError, Diagnostic, GameStack, LoadedShipment, NameTable,
    ReadError, Severity,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Findings at Error or above.
const EXIT_FINDINGS: u8 = 1;
/// The command could not run.
const EXIT_UNUSABLE: u8 = 2;

#[derive(Parser)]
#[command(
    name = "qm",
    about = "Quartermaster — lint and build Mercenaries 2 Shipments",
    long_about = "Reads a Shipment (manifest.yaml/.json/.toml plus src/) and either checks it or \
                  builds it into an overlay WAD.\n\n\
                  `lint` is hermetic and needs no game install, which is what lets it run in CI. \
                  `build` and `link` need the retail WADs.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a Shipment without a game install. Hermetic — safe for CI.
    Lint {
        /// The Shipment directory (the one holding manifest.yaml).
        #[arg(default_value = ".")]
        shipment: PathBuf,
        /// Also run the rules that need the retail WADs, if a stack can be found.
        #[arg(long)]
        with_game: bool,
        /// Where the game is installed. Only meaningful with --with-game.
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// hash → name lookup, for M0130. Defaults to the workspace's data/production_names.json.
        #[arg(long, value_name = "FILE")]
        names: Option<PathBuf>,
        /// Also write the findings as JSON (lint-report.json) to this file. Any older file there is
        /// removed first; when lint cannot run (exit 2), no report is written.
        #[arg(long, value_name = "FILE")]
        report: Option<PathBuf>,
    },
    /// Check version ranges with the same semver grammar qm applies to manifest ranges (M0172).
    /// Hermetic: no Shipment, no game, no network.
    ///
    /// Put the ranges after `--`, so that no range can be read as an option. Exit 0: every range
    /// parses. Exit 1: at least one does not (the report has one M0172 finding per bad range).
    /// Exit 2: nothing was checked (no ranges, or the report could not be written), and no report
    /// is left behind.
    CheckRange {
        /// Where to write range-report.json. Any older file there is removed first.
        #[arg(long, value_name = "FILE")]
        report: PathBuf,
        /// The ranges, e.g. ">=0.7, <1" "^1.0.0".
        ranges: Vec<String>,
    },
    /// Print a manifest's Shipment name and version as one JSON object. Hermetic: parses and
    /// validates the manifest file alone — no source files, no game.
    ///
    /// Exit 0: stdout is exactly `{"name": "<shipment.name>", "version": "<shipment.version>"}`.
    /// Exit 2: the file cannot be read, has no manifest extension (yaml, yml, json, toml), does not
    /// parse, or fails validation; the reason goes to stderr and stdout is empty.
    ManifestInfo {
        /// The manifest file.
        manifest: PathBuf,
    },
    /// Compile Lua files with the game's Lua compiler and check each chunk's LuaQ header.
    ///
    /// Each file is compiled under a bare chunk name: --chunk-name, or the file stem. With --out-dir
    /// the bytecode is written to <dir>/<chunk>.luac, read back and its header checked. Prints
    /// `ok <file> <bytes> <sha256>` per file. Exit 0: all compiled and verified. Exit 1: a syntax
    /// error, or a name that is not a bare chunk name. Exit 2: could not run (a file missing,
    /// unreadable or not UTF-8, --chunk-name with several files, an unwritable output, or a header
    /// mismatch, which is a toolchain defect).
    CompileLua {
        /// The `.lua` files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// The chunk name, instead of the file stem. Only with exactly one file.
        #[arg(long, value_name = "NAME")]
        chunk_name: Option<String>,
        /// Write each chunk to <dir>/<chunk>.luac and verify it from disk.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Build a Shipment into an overlay WAD. Needs the retail WADs.
    Build {
        /// The Shipment directory.
        #[arg(default_value = ".")]
        shipment: PathBuf,
        /// Where the game is installed. Defaults to host discovery.
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// Output directory. Defaults to <shipment>/_build.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// The decompiled Lua corpus root, for script-touching contributions.
        #[arg(long, value_name = "DIR")]
        corpus: Option<PathBuf>,
        /// The reference bundle whose `lua/` subtree is the corpus (as `mercs2_workshop --pack-data`
        /// ships it). Used only when --corpus is absent. Env: MERCS2_WORKSHOP_DATA.
        #[arg(long, value_name = "DIR")]
        workshop_data: Option<PathBuf>,
        /// hash → name lookup, for M0130. Defaults to the workspace's data/production_names.json.
        #[arg(long, value_name = "FILE")]
        names: Option<PathBuf>,
    },
    /// Check a set of Shipments before building: requirements, versions, conflicts, superseded
    /// legacy files and the load order. Writes <out>/load-plan.json. No WAD is opened.
    ///
    /// Exit 0: the plan is ok. Exit 1: the plan has error findings (it is still written, with the
    /// load order unless the requirements form a cycle). Exit 2: nothing could be checked, and no
    /// plan is written.
    Preflight {
        /// Every Shipment directory, in request order (the tie-break). Each gets the id `arg:<n>`.
        /// Give either these or --request.
        shipments: Vec<PathBuf>,
        /// A load-request.json naming the Shipments and their ids, in request order.
        #[arg(long, value_name = "FILE")]
        request: Option<PathBuf>,
        /// Where to write load-plan.json.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// Where the game is installed: vz.wad, the install root or its data folder. Defaults to
        /// host discovery. Needed only when a Shipment declares `supersedes`; resolved whenever
        /// given.
        #[arg(long, value_name = "PATH")]
        game: Option<PathBuf>,
    },
    /// Link the Lua of several installed Shipments into one WAD, mounted last.
    ///
    /// Scripts load from the block, not per-hash, so a Shipment's own overlay is only valid
    /// standalone. This is what makes two script-touching Shipments coexist. The set's load plan
    /// is computed first and written beside the output; a plan that is not ok links nothing.
    Link {
        /// Every installed Shipment directory, in request order. Give either these or --request.
        shipments: Vec<PathBuf>,
        /// A load-request.json naming the Shipments and their ids, in request order.
        #[arg(long, value_name = "FILE")]
        request: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        game: Option<PathBuf>,
        /// Where to write the link WAD. Required — it is not any one Shipment's output.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        #[arg(long, value_name = "DIR")]
        corpus: Option<PathBuf>,
        /// The reference bundle whose `lua/` subtree is the corpus (as `mercs2_workshop --pack-data`
        /// ships it). Used only when --corpus is absent. Env: MERCS2_WORKSHOP_DATA.
        #[arg(long, value_name = "DIR")]
        workshop_data: Option<PathBuf>,
    },
    /// Extract a destructible's state machine as an editable `states:` file.
    ///
    /// The baseline for `edit_state_machine`: dump what the model already carries (states named where
    /// their hashes reverse, command scripts as token lists), redirect it to a file, edit that, and
    /// point `states:` at it. Authoring one by hand would be punishing; this is the "pull it
    /// automatically" half of the workflow.
    ExtractStates {
        /// The destructible model — a name or a bare `0xHASH`.
        target: String,
        /// Where the game is installed. Defaults to host discovery.
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// hash → name lookup, so the dump reads in names. Defaults to the workspace's names.
        #[arg(long, value_name = "FILE")]
        names: Option<PathBuf>,
    },
    /// Extract a placement layer's entities as an editable `edit_world` file.
    ///
    /// The baseline for `edit_world`: dump a `vz_state` overlay or `layers_static`'s placements (each
    /// entity's key, name, position, rotation, model), redirect it to a file, edit the ones you want,
    /// and point `edits:` at it.
    ExtractWorld {
        /// The layer — a PTHS-path needle (`vz_state_pmccon004`, `layers_static`).
        layer: String,
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// hash → name lookup, so a model reads by name. Defaults to the workspace's names.
        #[arg(long, value_name = "FILE")]
        names: Option<PathBuf>,
    },
    /// List every rule: what is checked, what is known-but-unchecked, and where each is documented.
    Rules,
    /// List every contribution kind this qm reads — the authoritative list. Hermetic: no Shipment,
    /// no game, no network.
    ///
    /// Prints one kind per line. With --json, prints `{"format":<manifest format>,"kinds":[...]}`
    /// instead. Exit 0.
    Kinds {
        /// Print one JSON object instead of one kind per line.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint {
            shipment,
            with_game,
            game,
            names,
            report,
        } => cmd_lint(
            &shipment,
            with_game,
            game.as_deref(),
            names.as_deref(),
            report.as_deref(),
        ),
        Command::CheckRange { report, ranges } => cmd_check_range(&report, &ranges),
        Command::ManifestInfo { manifest } => cmd_manifest_info(&manifest),
        Command::CompileLua {
            files,
            chunk_name,
            out_dir,
        } => cmd_compile_lua(&files, chunk_name.as_deref(), out_dir.as_deref()),
        Command::Build {
            shipment,
            game,
            out,
            corpus,
            workshop_data,
            names,
        } => cmd_build(
            &shipment,
            game.as_deref(),
            out.as_deref(),
            corpus.as_deref(),
            workshop_data.as_deref(),
            names.as_deref(),
        ),
        Command::Preflight {
            shipments,
            request,
            out,
            game,
        } => cmd_preflight(&shipments, request.as_deref(), &out, game.as_deref()),
        Command::Link {
            shipments,
            request,
            game,
            out,
            corpus,
            workshop_data,
        } => cmd_link(
            &shipments,
            request.as_deref(),
            game.as_deref(),
            &out,
            corpus.as_deref(),
            workshop_data.as_deref(),
        ),
        Command::ExtractStates {
            target,
            game,
            names,
        } => cmd_extract_states(&target, game.as_deref(), names.as_deref()),
        Command::ExtractWorld {
            layer,
            game,
            names,
        } => cmd_extract_world(&layer, game.as_deref(), names.as_deref()),
        Command::Rules => cmd_rules(),
        Command::Kinds { json } => cmd_kinds(json),
    }
}

/// Load a Shipment, or explain why not and give up.
fn load(root: &Path) -> Result<LoadedShipment, ExitCode> {
    open_shipment(root).map_err(|e| {
        eprintln!("error: {}: {e}", root.display());
        ExitCode::from(EXIT_UNUSABLE)
    })
}

/// Load the name lookup, or explain what is lost without it.
///
/// Optional on purpose. It powers M0130, which turns a bare hash in a manifest back into the name
/// it was minted from — genuinely useful, and not worth refusing to lint over. But a linter that
/// quietly runs one rule short is exactly the "clean bill of health" failure this crate is built to
/// avoid, so a missing table says so.
fn resolve_names(explicit: Option<&Path>) -> Option<NameTable> {
    if let Some(p) = explicit {
        return match NameTable::load(p) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("warning: {}: {e} — M0130 will not run", p.display());
                None
            }
        };
    }

    // Walk up from the EXECUTABLE first, then from the working directory.
    //
    // Emphatically NOT `CARGO_MANIFEST_DIR`: that bakes the path of whatever machine built the
    // binary into the binary, so a released `qm` would look for the table on a CI runner's
    // filesystem and never find it. This crate exists partly because that class of bug — a path
    // that worked on one dev machine — shipped before.
    //
    // The executable comes first because that is where a managed install puts its data; the working
    // directory covers running inside a checkout.
    let from_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(NameTable::find_from));
    let found = from_exe.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| NameTable::find_from(&cwd))
    });

    if found.is_none() {
        eprintln!(
            "note: no name table found next to qm or under the working directory; M0130 (bare hash \
             where a name is known) will not run. Pass --names <file> to enable it."
        );
    }
    found
}

/// Resolve the game stack: an explicit path wins, otherwise host discovery.
///
/// `--game` may name `vz.wad`, the install root or its `data` folder, the same as for
/// `qm preflight` ([`compat::resolve_vz_wad`]). No manifest names a path: a Shipment that could
/// name its own game folder would be a Shipment that behaves differently on the author's machine
/// than on anyone else's. The manifests decide only which language WADs beside `vz.wad` join the
/// stack ([`compat::game_stack_paths`]).
fn resolve_game<'a>(
    explicit: Option<&Path>,
    manifests: impl IntoIterator<Item = &'a mercs2_quartermaster::Manifest>,
) -> Result<GameStack, ExitCode> {
    let vz = compat::resolve_vz_wad(explicit).map_err(|e| {
        eprintln!("error: {e}\nnote: `qm lint` needs no game install and will still run.");
        ExitCode::from(EXIT_UNUSABLE)
    })?;
    let paths = compat::game_stack_paths(&vz, manifests).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(EXIT_UNUSABLE)
    })?;
    GameStack::open(&paths).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(EXIT_UNUSABLE)
    })
}

fn cmd_lint(
    root: &Path,
    with_game: bool,
    game_dir: Option<&Path>,
    names_path: Option<&Path>,
    report_file: Option<&Path>,
) -> ExitCode {
    // The stale report goes first, so every exit-2 path below leaves no report behind.
    if let Some(file) = report_file {
        if let Err(e) = plan::remove_stale_file(file) {
            eprintln!("error: {e}");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    }
    let shipment = match load(root) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let names = resolve_names(names_path);
    let mut found = lint::lint(&shipment.manifest, Some(&shipment.root), names.as_ref());

    if with_game {
        match resolve_game(game_dir, [&shipment.manifest]) {
            Ok(mut stack) => found.extend(lint::game_checks(&shipment.manifest, &mut stack)),
            Err(code) => return code,
        }
    }

    let blocked = lint::blocks_build(&found);
    if let Some(file) = report_file {
        let mut findings: Vec<plan::Finding> = found.iter().map(Diagnostic::to_finding).collect();
        plan::sort_findings(&mut findings, |_| None);
        let lint_report = plan::LintReport {
            format: plan::REPORT_FORMAT,
            producer: "lint",
            quartermaster: compat::QUARTERMASTER_VERSION,
            ok: !blocked,
            manifest: &shipment.manifest,
            findings,
        };
        if let Err(e) = plan::write_json(file, &lint_report, "the lint report") {
            eprintln!("error: {e}");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    }

    report(&shipment.manifest.shipment.name, &found);
    if blocked {
        ExitCode::from(EXIT_FINDINGS)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_manifest_info(path: &Path) -> ExitCode {
    let unusable = |message: String| {
        eprintln!("error: {}: {message}", path.display());
        ExitCode::from(EXIT_UNUSABLE)
    };
    let Some(format) = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(mercs2_quartermaster::Format::from_extension)
    else {
        return unusable("-: not a manifest file — expected a .yaml, .yml, .json or .toml".into());
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return unusable(format!("-: reading the manifest: {e}")),
    };
    let manifest = match mercs2_quartermaster::from_str(&text, format) {
        Ok(m) => m,
        Err(ReadError::Validate(v)) => return unusable(format!("{}: {v}", v.code().unwrap_or("-"))),
        Err(e @ (ReadError::Parse { .. } | ReadError::RemovedKind { .. })) => {
            return unusable(format!("-: {e}"))
        }
    };
    println!(
        "{}",
        serde_json::json!({
            "name": manifest.shipment.name,
            "version": manifest.shipment.version,
        })
    );
    ExitCode::SUCCESS
}

/// Compile each file; the exit code is the worst outcome across them, so every file is reported.
fn cmd_compile_lua(files: &[PathBuf], chunk_name: Option<&str>, out_dir: Option<&Path>) -> ExitCode {
    if chunk_name.is_some() && files.len() != 1 {
        eprintln!(
            "error: --chunk-name names one chunk, but {} files were given",
            files.len()
        );
        return ExitCode::from(EXIT_UNUSABLE);
    }
    let mut worst: u8 = 0;
    for file in files {
        let outcome = compile_one(file, chunk_name, out_dir);
        match outcome {
            Ok(line) => println!("{line}"),
            Err((code, message)) => {
                eprintln!("error: {}: {message}", file.display());
                worst = worst.max(code);
            }
        }
    }
    ExitCode::from(worst)
}

/// One file: `Ok(the stdout line)` or `Err((exit code, message))`.
fn compile_one(
    file: &Path,
    chunk_name: Option<&str>,
    out_dir: Option<&Path>,
) -> Result<String, (u8, String)> {
    let bytes = std::fs::read(file).map_err(|e| (EXIT_UNUSABLE, format!("reading: {e}")))?;
    let source = String::from_utf8(bytes)
        .map_err(|e| (EXIT_UNUSABLE, format!("the file is not UTF-8: {e}")))?;
    let chunk = match chunk_name {
        Some(name) => name.to_string(),
        None => file
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .ok_or_else(|| {
                (
                    EXIT_FINDINGS,
                    "the file stem is not UTF-8, so it cannot be a chunk name".to_string(),
                )
            })?,
    };
    if let Some(why) = mercs2_quartermaster::link::chunk_name_refusal(&chunk) {
        return Err((
            EXIT_FINDINGS,
            format!("{chunk:?} is not a bare chunk name: {why}"),
        ));
    }
    let bytecode = mercs2_luac::compile(&source, &chunk).map_err(|e| match e {
        mercs2_luac::CompileError::Syntax(m) => (EXIT_FINDINGS, m),
        other => (EXIT_UNUSABLE, other.to_string()),
    })?;
    if let Some(dir) = out_dir {
        std::fs::create_dir_all(dir)
            .map_err(|e| (EXIT_UNUSABLE, format!("creating {}: {e}", dir.display())))?;
        let path = dir.join(format!("{chunk}.luac"));
        std::fs::write(&path, &bytecode)
            .map_err(|e| (EXIT_UNUSABLE, format!("writing {}: {e}", path.display())))?;
        let on_disk = std::fs::read(&path)
            .map_err(|e| (EXIT_UNUSABLE, format!("reading back {}: {e}", path.display())))?;
        mercs2_luac::check_header(&on_disk)
            .map_err(|e| (EXIT_UNUSABLE, format!("{}: {e}", path.display())))?;
        if on_disk != bytecode {
            return Err((
                EXIT_UNUSABLE,
                format!("{} does not read back as the bytes written", path.display()),
            ));
        }
    }
    Ok(format!(
        "ok {} {} {}",
        file.display(),
        bytecode.len(),
        build::sha256_hex(&bytecode)
    ))
}

fn cmd_check_range(report_file: &Path, ranges: &[String]) -> ExitCode {
    // The stale report goes first, so every exit-2 path below leaves no report behind.
    if let Err(e) = plan::remove_stale_file(report_file) {
        eprintln!("error: {e}");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    if ranges.is_empty() {
        eprintln!("error: give at least one range to check, after `--`");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    let findings: Vec<plan::Finding> = ranges
        .iter()
        .enumerate()
        .filter_map(|(index, range)| {
            mercs2_quartermaster::manifest::parse_range(range)
                .err()
                .map(|message| plan::Finding {
                    code: lint::M0172_BAD_VERSION_REQ.code,
                    severity: FindingSeverity::Error,
                    message,
                    items: Vec::new(),
                    refs: vec![plan::FindingRef {
                        section: plan::Section::Ranges,
                        index,
                    }],
                    fix: None,
                })
        })
        .collect();
    for f in &findings {
        let index = f.refs[0].index;
        eprintln!("[{}] error: ranges[{index}] {:?}: {}", f.code, ranges[index], f.message);
    }
    let ok = findings.is_empty();
    let range_report = plan::RangeReport {
        format: plan::REPORT_FORMAT,
        producer: "check-range",
        quartermaster: compat::QUARTERMASTER_VERSION,
        ok,
        findings,
    };
    if let Err(e) = plan::write_json(report_file, &range_report, "the range report") {
        eprintln!("error: {e}");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    if ok {
        eprintln!("{} range(s): all valid", ranges.len());
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_FINDINGS)
    }
}

fn cmd_build(
    root: &Path,
    game_dir: Option<&Path>,
    out: Option<&Path>,
    corpus: Option<&Path>,
    workshop_data: Option<&Path>,
    names_path: Option<&Path>,
) -> ExitCode {
    let shipment = match load(root) {
        Ok(s) => s,
        Err(code) => return code,
    };
    // A corpus is only REQUIRED for a script-touching Shipment, so `None` is passed through and
    // `build` decides. But a corpus path that WAS given and is unusable is a loud error now, not at
    // the point some patch_lua happens to need it.
    let corpus = match resolve_corpus(corpus, workshop_data) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    };
    let mut stack = match resolve_game(game_dir, [&shipment.manifest]) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let names = resolve_names(names_path);

    match build::build(
        &shipment,
        Some(&mut stack),
        names.as_ref(),
        out,
        corpus.as_deref(),
    ) {
        Ok(report_) => {
            for line in &report_.log {
                println!("{line}");
            }
            report(&shipment.manifest.shipment.name, &report_.diagnostics);
            if let Some(wad) = &report_.wad {
                println!("built {}", wad.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            // `Blocked` is a finding, not a misconfiguration; everything else means we could not
            // run. CI wants to tell those apart.
            let code = match e {
                BuildError::Blocked(_)
                | BuildError::Artifact { .. }
                | BuildError::Superseded { .. } => EXIT_FINDINGS,
                _ => EXIT_UNUSABLE,
            };
            eprintln!("error: {e}");
            ExitCode::from(code)
        }
    }
}

fn cmd_extract_states(target: &str, game_dir: Option<&Path>, names_path: Option<&Path>) -> ExitCode {
    let mut stack = match resolve_game(game_dir, []) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let hash = mercs2_quartermaster::manifest::asset_hash(target);
    let Some(inputs) = stack.model_container_for_edit(hash) else {
        eprintln!(
            "error: {target:?} (0x{hash:08X}) is not a model in the game stack (or its block has no \
             primary container)"
        );
        return ExitCode::from(EXIT_UNUSABLE);
    };
    let Some(sm) = mercs2_formats::orchestrator::parse_state_machine(&inputs.container) else {
        eprintln!("error: {target:?} is a model but carries no destruction state machine to extract");
        return ExitCode::from(EXIT_UNUSABLE);
    };
    let names = resolve_names(names_path);
    let name_of = |h: u32| names.as_ref().and_then(|n| n.reverse(h)).map(|s| s.to_string());
    print!("{}", mercs2_quartermaster::states::extract(&sm, name_of));
    ExitCode::SUCCESS
}

fn cmd_extract_world(layer: &str, game_dir: Option<&Path>, names_path: Option<&Path>) -> ExitCode {
    let mut stack = match resolve_game(game_dir, []) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let Some(inputs) = stack.layer_block_for_edit(layer) else {
        eprintln!(
            "error: no layer matching {layer:?} in the game stack — try a needle like \
             \"vz_state_pmccon004\" or \"layers_static\""
        );
        return ExitCode::from(EXIT_UNUSABLE);
    };
    let places = match mercs2_formats::placement::load_placements(&inputs.block) {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("error: {layer:?} carries no readable placements");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    };
    // Merge in each entity's model hash (only some placements carry a ModelName).
    let model_by_key: std::collections::HashMap<u32, u32> =
        mercs2_formats::placement::load_model_placements(&inputs.block)
            .into_iter()
            .map(|m| (m.key, m.model_hash))
            .collect();
    let dumped: Vec<mercs2_quartermaster::world::Placed> = places
        .into_iter()
        .map(|p| mercs2_quartermaster::world::Placed {
            key: p.key,
            name: p.name,
            pos: p.pos,
            quat: p.quat,
            model: model_by_key.get(&p.key).copied(),
        })
        .collect();
    let names = resolve_names(names_path);
    let model_name = |h: u32| names.as_ref().and_then(|n| n.reverse(h)).map(|s| s.to_string());
    print!("{}", mercs2_quartermaster::world::extract(&dumped, model_name));
    ExitCode::SUCCESS
}

/// The request items: from `--request`, or `arg:<n>` for each positional directory. Exactly one of
/// the two must be given.
fn request_items(dirs: &[PathBuf], request: Option<&Path>) -> Result<Vec<RequestItem>, ExitCode> {
    match (request, dirs.is_empty()) {
        (Some(file), true) => plan::read_request(file).map_err(|e| {
            eprintln!("error: {e}");
            ExitCode::from(EXIT_UNUSABLE)
        }),
        (None, false) => Ok(plan::request_from_dirs(dirs)),
        (Some(_), false) => {
            eprintln!("error: give either --request or Shipment directories, not both");
            Err(ExitCode::from(EXIT_UNUSABLE))
        }
        (None, true) => {
            eprintln!("error: give --request <load-request.json> or at least one Shipment directory");
            Err(ExitCode::from(EXIT_UNUSABLE))
        }
    }
}

/// Why an item's manifest could not be opened, as `<code or ->: <message>` — never naming the
/// item's path, which the caller maps back through the id.
fn open_failure(e: &OpenError) -> String {
    match e {
        OpenError::Discover(DiscoverError::NotADirectory(_)) => "-: the path is not a directory".into(),
        OpenError::Discover(DiscoverError::NoManifest { .. }) => {
            "-: no manifest in the directory — expected one of manifest.yaml, manifest.yml, \
             manifest.json, manifest.toml"
                .into()
        }
        OpenError::Discover(DiscoverError::Ambiguous { found, .. }) => {
            let names: Vec<String> = found
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            format!(
                "-: the directory holds {} manifests ({}) — keep exactly one",
                found.len(),
                names.join(", ")
            )
        }
        OpenError::Discover(DiscoverError::Io { message, .. }) => {
            format!("-: reading the manifest: {message}")
        }
        OpenError::Read(ReadError::Validate(v)) => format!("{}: {v}", v.code().unwrap_or("-")),
        OpenError::Read(r @ (ReadError::Parse { .. } | ReadError::RemovedKind { .. })) => {
            format!("-: {r}")
        }
    }
}

/// Open every item's Shipment, reporting EVERY failure (one line each) before giving up.
fn open_items(items: &[RequestItem]) -> Result<Vec<LoadedShipment>, ExitCode> {
    let mut opened = Vec::with_capacity(items.len());
    let mut failed = false;
    for item in items {
        match open_shipment(&item.path) {
            Ok(s) => opened.push(s),
            Err(e) => {
                eprintln!("error: {}: {}", item.id, open_failure(&e));
                failed = true;
            }
        }
    }
    if failed {
        Err(ExitCode::from(EXIT_UNUSABLE))
    } else {
        Ok(opened)
    }
}

/// Print a plan's findings, then a one-line verdict naming the file.
fn report_plan(plan_: &LoadPlan, out: &Path) {
    for f in &plan_.findings {
        eprintln!(
            "[{}] {}: {}: {}",
            f.code,
            f.severity.as_str(),
            f.items.join(", "),
            f.message
        );
    }
    eprintln!(
        "load plan {}: {} finding(s) → {}",
        if plan_.ok { "ok" } else { "NOT ok" },
        plan_.findings.len(),
        out.join(plan::PLAN_FILE).display()
    );
}

fn cmd_preflight(
    dirs: &[PathBuf],
    request: Option<&Path>,
    out: &Path,
    game_path: Option<&Path>,
) -> ExitCode {
    // The stale plan goes first, so every exit-2 path below leaves no plan behind.
    if let Err(e) = plan::remove_stale(out) {
        eprintln!("error: {e}");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    let items = match request_items(dirs, request) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let opened = match open_items(&items) {
        Ok(o) => o,
        Err(code) => return code,
    };
    // The game folder is derived only when something needs probing — but a --game that was given
    // is always resolved, so a wrong one never passes silently.
    let root = if game_path.is_some() || compat::needs_game_root(opened.iter().map(|s| &s.manifest)) {
        let derived = compat::resolve_vz_wad(game_path).and_then(|vz| compat::game_root_of(&vz));
        match derived {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(EXIT_UNUSABLE);
            }
        }
    } else {
        None
    };
    let inputs: Vec<PlanInput<'_>> = items
        .iter()
        .zip(&opened)
        .map(|(item, shipment)| PlanInput {
            id: &item.id,
            shipment,
        })
        .collect();
    let plan_ = match compat::plan(&inputs, Producer::Preflight, root.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    };
    if let Err(e) = plan::write_plan(out, &plan_) {
        eprintln!("error: {e}");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    report_plan(&plan_, out);
    if plan_.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_FINDINGS)
    }
}

fn cmd_link(
    dirs: &[PathBuf],
    request: Option<&Path>,
    game_dir: Option<&Path>,
    out: &Path,
    corpus: Option<&Path>,
    workshop_data: Option<&Path>,
) -> ExitCode {
    // The stale plan goes first, so every exit-2 path below leaves no plan behind.
    if let Err(e) = plan::remove_stale(out) {
        eprintln!("error: {e}");
        return ExitCode::from(EXIT_UNUSABLE);
    }
    let items = match request_items(dirs, request) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let opened = match open_items(&items) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let mut stack = match resolve_game(game_dir, opened.iter().map(|s| &s.manifest)) {
        Ok(s) => s,
        Err(code) => return code,
    };
    // Linking IS the script step, so a corpus is always required here.
    let corpus = match resolve_corpus(corpus, workshop_data) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!(
                "error: linking Lua needs the decompiled corpus. Pass --corpus <dir>, or \
                 --workshop-data <dir> / set MERCS2_WORKSHOP_DATA to your reference bundle — its \
                 lua/ subtree is the corpus, shipped by `mercs2_workshop --pack-data`."
            );
            return ExitCode::from(EXIT_UNUSABLE);
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(EXIT_UNUSABLE);
        }
    };

    let inputs: Vec<PlanInput<'_>> = items
        .iter()
        .zip(&opened)
        .map(|(item, shipment)| PlanInput {
            id: &item.id,
            shipment,
        })
        .collect();
    match build::link_installed(&inputs, &mut stack, &corpus, out) {
        Ok(report_) => {
            for line in &report_.log {
                println!("{line}");
            }
            match &report_.wad {
                Some(w) => println!("linked {}", w.display()),
                // Not a failure: a set with no script-touching Shipment needs no link WAD, and
                // emitting an empty one would be a file deploy has to reason about for nothing.
                None => println!(
                    "no script mutations across {} Shipment(s); nothing to link",
                    inputs.len()
                ),
            }
            report_plan(&report_.plan, out);
            ExitCode::SUCCESS
        }
        Err(BuildError::Plan(plan_)) => {
            // The plan was written; it is the explanation. Nothing was linked.
            report_plan(&plan_, out);
            eprintln!("error: the load plan is not ok, so nothing was linked");
            ExitCode::from(EXIT_FINDINGS)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(match e {
                BuildError::Artifact { .. } => EXIT_FINDINGS,
                _ => EXIT_UNUSABLE,
            })
        }
    }
}

/// The corpus vendored in this workspace, when `qm` is run from inside it.
/// The decompiled Lua corpus root the linker appends to. Resolution, highest first — and with NO
/// silent fallback: the corpus is either named explicitly or found in a named bundle, and anything
/// else fails loudly. The old `CARGO_MANIFEST_DIR` guess resolved to the AUTHOR's checkout and
/// nowhere else, so it "worked" in dev and left every released `qm` unable to link — the exact trap
/// a fallback becomes.
///
/// 1. `--corpus <dir>` — an explicit corpus root
/// 2. `--workshop-data <dir>` or `$MERCS2_WORKSHOP_DATA` — the reference bundle; the corpus is its
///    `lua/` subtree, exactly what `mercs2_workshop --pack-data` ships and the Workshop reads
///
/// `Ok(None)` means nothing was named (the caller decides whether that is fatal — `build` only needs
/// a corpus for a script-touching Shipment). `Err` means a path WAS named but is not a usable
/// corpus, which is always an error worth stopping for.
fn resolve_corpus(
    corpus: Option<&Path>,
    workshop_data: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    if let Some(c) = corpus {
        if !c.is_dir() {
            return Err(format!("--corpus {}: not a directory", c.display()));
        }
        return Ok(Some(c.to_path_buf()));
    }
    let bundle = workshop_data.map(Path::to_path_buf).or_else(|| {
        std::env::var_os("MERCS2_WORKSHOP_DATA")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    });
    let Some(bundle) = bundle else {
        return Ok(None);
    };
    let lua = bundle.join("lua");
    if !lua.is_dir() {
        return Err(format!(
            "{}: no lua/ corpus in this bundle — build it with `mercs2_workshop --pack-data`",
            bundle.display()
        ));
    }
    Ok(Some(lua))
}

fn cmd_rules() -> ExitCode {
    println!("Hermetic — run by `qm lint` with no game install:");
    for r in lint::RULES {
        println!("  {}  {}\n      {}", r.code, r.title, r.url());
    }
    println!("\nNeed the retail WADs — `qm lint --with-game`, and always during `qm build`:");
    for r in lint::GAME_RULES {
        println!("  {}  {}\n      {}", r.code, r.title, r.url());
    }
    println!("\nChecked against the WAD the builder emits, before it reaches disk:");
    for r in lint::ARTIFACT_RULES {
        println!("  {}  {}\n      {}", r.code, r.title, r.url());
    }
    // Printed on purpose. A linter that silently omits its most dangerous rules reads as a clean
    // bill of health, which is worse than no linter at all.
    println!("\nKNOWN AND NOT YET CHECKED — these can still hang the game:");
    for r in lint::PENDING {
        println!("  {}  {}\n      {}", r.code, r.title, r.url());
    }
    ExitCode::SUCCESS
}

/// Every kind in `Contribution::ALL_KINDS`, in that order, to stdout.
fn cmd_kinds(json: bool) -> ExitCode {
    let kinds = mercs2_quartermaster::Contribution::ALL_KINDS;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "format": mercs2_quartermaster::FORMAT_VERSION,
                "kinds": kinds,
            })
        );
    } else {
        for k in kinds {
            println!("{k}");
        }
    }
    ExitCode::SUCCESS
}

/// Print findings worst-first, then a one-line verdict.
fn report(name: &str, found: &[Diagnostic]) {
    let mut sorted: Vec<&Diagnostic> = found.iter().collect();
    sorted.sort_by(|a, b| b.severity.cmp(&a.severity));
    for d in &sorted {
        // Findings go to stderr so `qm build` stdout stays pipeable.
        eprintln!("{d}");
    }
    let hangs = found
        .iter()
        .filter(|d| d.severity == Severity::Hang)
        .count();
    let errors = found
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = found
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    if hangs + errors + warnings == 0 {
        eprintln!("{name}: clean");
    } else {
        eprintln!("{name}: {hangs} HANG, {errors} error(s), {warnings} warning(s)");
    }
}
