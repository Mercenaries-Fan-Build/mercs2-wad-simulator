//! `qm` — the Quartermaster CLI.
//!
//! Three audiences, and they do not overlap much:
//!
//! - **A modder's machine**, where the retail WADs exist and `qm build` can lower for real.
//! - **A template repo's CI**, where they never will. `qm lint` is hermetic on purpose — manifest
//!   text plus the Shipment directory, no game, no network — so a public runner can gate every push.
//! - **A deploy step**, which needs `qm link` across the whole installed set, because Lua scripts
//!   load from a block rather than per-hash and two script-touching Shipments would otherwise
//!   silently annihilate each other.
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
use mercs2_quartermaster::{
    build, game, lint, open_shipment, BuildError, Diagnostic, GameStack, LoadedShipment, NameTable,
    Severity,
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
    },
    /// Build a Shipment into an overlay WAD. Needs the retail WADs.
    Build {
        /// The Shipment directory.
        #[arg(default_value = ".")]
        shipment: PathBuf,
        /// Where the game is installed. Defaults to host discovery.
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// Output directory. Defaults to <shipment>/build.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// The decompiled Lua corpus root, for script-touching contributions.
        #[arg(long, value_name = "DIR")]
        corpus: Option<PathBuf>,
        /// hash → name lookup, for M0130. Defaults to the workspace's data/production_names.json.
        #[arg(long, value_name = "FILE")]
        names: Option<PathBuf>,
    },
    /// Link the Lua of several installed Shipments into one WAD, mounted last.
    ///
    /// Scripts load from the block, not per-hash, so a Shipment's own overlay is only valid
    /// standalone. This is what makes two script-touching Shipments coexist.
    Link {
        /// Every installed Shipment directory.
        #[arg(required = true)]
        shipments: Vec<PathBuf>,
        #[arg(long, value_name = "DIR")]
        game: Option<PathBuf>,
        /// Where to write the link WAD. Required — it is not any one Shipment's output.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        #[arg(long, value_name = "DIR")]
        corpus: Option<PathBuf>,
    },
    /// List every rule: what is checked, what is known-but-unchecked, and where each is documented.
    Rules,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Lint {
            shipment,
            with_game,
            game,
            names,
        } => cmd_lint(&shipment, with_game, game.as_deref(), names.as_deref()),
        Command::Build {
            shipment,
            game,
            out,
            corpus,
            names,
        } => cmd_build(
            &shipment,
            game.as_deref(),
            out.as_deref(),
            corpus.as_deref(),
            names.as_deref(),
        ),
        Command::Link {
            shipments,
            game,
            out,
            corpus,
        } => cmd_link(&shipments, game.as_deref(), &out, corpus.as_deref()),
        Command::Rules => cmd_rules(),
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
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => {
            let p = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .map(|root| root.join(mercs2_quartermaster::names::PRODUCTION_NAMES))?;
            if !p.is_file() {
                eprintln!(
                    "note: no name table found; M0130 (bare hash where a name is known) will not run. \
                     Pass --names <file> to enable it."
                );
                return None;
            }
            p
        }
    };
    match NameTable::load(&path) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("warning: {}: {e} — M0130 will not run", path.display());
            None
        }
    }
}

/// Resolve the game stack: an explicit path wins, otherwise host discovery.
///
/// The manifest is never consulted. A Shipment that could name its own game folder would be a
/// Shipment that behaves differently on the author's machine than on anyone else's.
fn resolve_game(explicit: Option<&Path>) -> Result<GameStack, ExitCode> {
    let paths = match explicit {
        Some(dir) => vec![dir.to_path_buf()],
        None => match game::discover() {
            Some(found) => vec![found.path],
            None => {
                eprintln!(
                    "error: no game install found. Pass --game <dir>, or run \
                     scripts/find-vz-wad.sh --write.\n\
                     note: `qm lint` needs no game install and will still run."
                );
                return Err(ExitCode::from(EXIT_UNUSABLE));
            }
        },
    };
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
) -> ExitCode {
    let shipment = match load(root) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let names = resolve_names(names_path);
    let mut found = lint::lint(&shipment.manifest, Some(&shipment.root), names.as_ref());

    if with_game {
        match resolve_game(game_dir) {
            Ok(stack) => found.extend(lint::game_checks(&shipment.manifest, &stack)),
            Err(code) => return code,
        }
    }

    report(&shipment.manifest.shipment.name, &found);
    if lint::blocks_build(&found) {
        ExitCode::from(EXIT_FINDINGS)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_build(
    root: &Path,
    game_dir: Option<&Path>,
    out: Option<&Path>,
    corpus: Option<&Path>,
    names_path: Option<&Path>,
) -> ExitCode {
    let shipment = match load(root) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let mut stack = match resolve_game(game_dir) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let names = resolve_names(names_path);

    match build::build(&shipment, Some(&mut stack), names.as_ref(), out, corpus) {
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
                BuildError::Blocked(_) | BuildError::Artifact { .. } => EXIT_FINDINGS,
                _ => EXIT_UNUSABLE,
            };
            eprintln!("error: {e}");
            ExitCode::from(code)
        }
    }
}

fn cmd_link(
    roots: &[PathBuf],
    game_dir: Option<&Path>,
    out: &Path,
    corpus: Option<&Path>,
) -> ExitCode {
    let mut loaded = Vec::new();
    for root in roots {
        match load(root) {
            Ok(s) => loaded.push(s),
            Err(code) => return code,
        }
    }
    let mut stack = match resolve_game(game_dir) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let Some(corpus) = corpus.map(Path::to_path_buf).or_else(default_corpus) else {
        eprintln!(
            "error: the Lua corpus is required to link scripts. Pass --corpus <dir> \
             (the decompiled corpus under crates/mercs2_script/corpus/mercs2-luacd/src)."
        );
        return ExitCode::from(EXIT_UNUSABLE);
    };

    let refs: Vec<&LoadedShipment> = loaded.iter().collect();
    match build::link_installed(&refs, &mut stack, &corpus, out) {
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
                    refs.len()
                ),
            }
            ExitCode::SUCCESS
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
fn default_corpus() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join("mercs2_script/corpus/mercs2-luacd/src");
    p.is_dir().then_some(p)
}

fn cmd_rules() -> ExitCode {
    println!("Hermetic — run by `qm lint` with no game install:");
    for r in lint::RULES {
        println!("  {}  {}\n      {}", r.code, r.title, r.doc);
    }
    println!("\nNeed the retail WADs — `qm lint --with-game`, and always during `qm build`:");
    for r in [lint::M0007_MULTI_RUNG_REPLACE, lint::M0009_NO_PRIMARY_ROW] {
        println!("  {}  {}\n      {}", r.code, r.title, r.doc);
    }
    println!("\nChecked against the WAD the builder emits, before it reaches disk:");
    for r in lint::ARTIFACT_RULES {
        println!("  {}  {}\n      {}", r.code, r.title, r.doc);
    }
    // Printed on purpose. A linter that silently omits its most dangerous rules reads as a clean
    // bill of health, which is worse than no linter at all.
    println!("\nKNOWN AND NOT YET CHECKED — these can still hang the game:");
    for r in lint::PENDING {
        println!("  {}  {}\n      {}", r.code, r.title, r.doc);
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
