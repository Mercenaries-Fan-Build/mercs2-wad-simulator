//! loadprobe — world-load progress + forensic analyzer for `pmc_blackbox.log`.
//!
//! Scores how far a Mercenaries 2 world-load got against the milestone ladder,
//! classifies the end-state (REACHED-WORLD / CRASH@EIP / HANG / TRUNCATED), and
//! surfaces every non-routine ([lua]/[pool]) diagnostic line + high-signal Lua markers.
//!
//! The log is the one written by `tools/pmc_blackbox` (`[HH:MM:SS.mmm] [source] msg`,
//! with the Lua hook's `  @script:line` suffix and the crash handler's
//! `module+0xOFFSET` frames).
//!
//! The verdict is also the process exit code, so scripts can branch on it:
//! `0` REACHED-WORLD, `10` CRASH, `11` HANG, `12` TRUNCATED (`2` = could not read
//! the log / JSON error). A crash AFTER the world finished loading is reported as a
//! post-load crash and does not demote the verdict; a crash at a `teardown` EIP
//! (e.g. 0x874E7D) is a hard-close artifact and never becomes a CRASH headline.
//!
//! Beyond the verdict, the run also reports: the phase timeline with deltas,
//! WAITFORSTREAMING cycles, progression (acts / player / portals / faction jobs),
//! texture-component pool health from `[cc]`/`[pool]`, `[mtrl] OVERCOUNT` and
//! `[stall]` dumps, the largest time gaps, the end-of-log tail, and a BUILD/RUN
//! IDENTITY block (the log's own SHA-256 plus pmc_bb's `[blackbox] BUILD` artifact
//! hashes) that binds the metrics to the exact bytes that produced them.
//!
//! Module map:
//! * [`parse`] — log-line parser → ordered `LogLine`s (ts, source, msg, script:line).
//! * [`phases`] — the static tables: milestone `LADDER`, `KNOWN_SOURCES`,
//!   `KNOWN_EIPS` (+ teardown flags), job-module test. Extend HERE, not elsewhere.
//! * [`report`] — analysis → `Report`, the colored text dump, and the JSON form.
//! * [`symbolize`] — optional (`--symbolize`) COFF/exe-map naming of crash frames.
//! * [`sha256`] — dependency-free SHA-256 for the log's own fingerprint.
//!
//! Integration tests in `tests/fixtures.rs` lock the classifier against four real
//! captures in `storage/`.

mod parse;
mod phases;
mod report;
mod sha256;
mod symbolize;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

/// Environment variable naming the exe-symbol annotations JSON, since it lives in the sibling
/// research repo whose location varies per checkout and cannot be derived from this one.
const EXE_SYMBOLS_ENV: &str = "MERCS2_EXE_SYMBOLS";

/// The game's `pmc_blackbox.log`, which retail writes into the install folder.
///
/// Derived from the resolved install (`--game-dir`/`MERCS2_GAME_DIR`/`VZ_WAD`/registry) rather than a
/// literal `C:/Users/Shadow/Desktop/...`, which named one developer's machine and could not resolve
/// anywhere else.
/// Mirrors `mercs2_formats::game_paths::GAME_DIR_VARS`, reproduced locally rather than depended upon —
/// this is a small standalone log tool and pulling in the format crate for three lines of path probing
/// would be the tail wagging the dog. Keep the variable names in step.
fn default_log() -> Option<PathBuf> {
    ["MERCS2_GAME_DIR", "VZ_WAD"]
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .find_map(|p| {
            // Accept the install root, its `data` folder, or `data/vz.wad`; the log sits at the root.
            let root = if p.is_file() { p.parent()?.parent()?.to_path_buf() } else if p.join("data").is_dir() { p } else { p.parent()?.to_path_buf() };
            let log = root.join("pmc_blackbox.log");
            log.is_file().then_some(log)
        })
}

/// The annotations JSON from `$MERCS2_EXE_SYMBOLS`, if set and present.
fn default_exe_symbols() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os(EXE_SYMBOLS_ENV)?);
    p.is_file().then_some(p)
}

#[derive(Parser)]
#[command(name = "loadprobe", about = "Quantify world-load progress + forensic dump of pmc_blackbox.log")]
struct Cli {
    /// Log file to analyze (defaults to the deployed game's pmc_blackbox.log).
    log: Option<PathBuf>,

    /// Comma-separated sources treated as routine (suppressed from the line dump).
    #[arg(long, default_value = "lua,pool")]
    routine: String,

    /// Steady-pool duration (seconds) with no progress to classify a HANG.
    #[arg(long, default_value_t = 10)]
    hang_secs: u64,

    /// Number of largest inter-line time gaps to report.
    #[arg(long, default_value_t = 5)]
    top_gaps: usize,

    /// Comma-separated high-signal Lua message prefixes.
    #[arg(long, default_value = "###!,###,!!!,##@,@@@,***,=-=")]
    signals: String,

    /// Emit JSON instead of the text dump.
    #[arg(long)]
    json: bool,

    /// Disable ANSI colors.
    #[arg(long)]
    no_color: bool,

    /// Print the milestone ladder and exit.
    #[arg(long)]
    milestones: bool,

    /// Resolve `module+0xOFFSET` tokens in the [crash] block to `function+0xN`
    /// (reads the un-stripped .asi/.dll COFF symbols + the exe annotation map).
    #[arg(long, short = 'S')]
    symbolize: bool,

    /// Curated Mercenaries2.exe VA→name map for --symbolize.
    /// Defaults to `$MERCS2_EXE_SYMBOLS`; required when `--symbolize` is used.
    #[arg(long)]
    exe_symbols: Option<PathBuf>,

    /// Extra directory to search for .asi/.dll module files (repeatable). The
    /// log's directory and its scripts/ subdir are always searched.
    #[arg(long)]
    module_dir: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.milestones {
        println!("loadprobe milestone ladder (phase / name / match substrings):");
        for p in phases::LADDER {
            println!("  {:>2}  {:<26} {:?}", p.idx, p.name, p.matches);
        }
        println!("\nreached-world at phase >= {}", phases::REACHED_WORLD_IDX);
        return ExitCode::SUCCESS;
    }

    if cli.no_color {
        colored::control::set_override(false);
    }

    let Some(path) = cli.log.or_else(default_log) else {
        eprintln!(
            "loadprobe: no log given and none found in the install.\n  \
             Pass the log path, or set MERCS2_GAME_DIR (or VZ_WAD) so `<install>/pmc_blackbox.log` \
             resolves."
        );
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("loadprobe: cannot read {}: {}", path.display(), e);
            return ExitCode::from(2);
        }
    };

    let lines = parse::parse_log(&text);
    let log_sha256 = sha256::sha256_hex(text.as_bytes());
    let routine: Vec<String> = cli.routine.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let signals: Vec<String> = cli.signals.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

    let mut rep = report::analyze(&path.display().to_string(), log_sha256, &lines, &routine, &signals, cli.hang_secs, cli.top_gaps);

    // Optional: rewrite the [crash] block's `module+0xOFFSET` tokens into
    // `= function+0xN`. Post-analysis + out-of-crash-path by design (the game's
    // handler stays allocation-free; naming happens here where symbols are safe).
    if cli.symbolize {
        if let Some(crash) = rep.crash.as_mut() {
            let mut dirs = Vec::new();
            if let Some(parent) = path.parent() {
                dirs.push(parent.to_path_buf());
                dirs.push(parent.join("scripts"));
            }
            dirs.extend(cli.module_dir.iter().cloned());
            // The annotations JSON lives in the sibling research repo, whose location this repo cannot
            // derive — so it must be named explicitly. An absent map is not fatal: the .asi/.dll COFF
            // symbols beside the log are an independent source, and either alone is useful.
            let exe_symbols = cli
                .exe_symbols
                .clone()
                .or_else(default_exe_symbols)
                .unwrap_or_else(|| PathBuf::from("<none>"));
            let mut sym = symbolize::Symbolizer::new(&exe_symbols, dirs);
            if sym.has_any_source() {
                for line in crash.block.iter_mut() {
                    *line = sym.rewrite_line(line);
                }
            } else {
                eprintln!(
                    "loadprobe: --symbolize found no symbol sources (exe map {} and no .asi/.dll \
                     next to the log). Pass --exe-symbols <file> or set {EXE_SYMBOLS_ENV}.",
                    exe_symbols.display()
                );
            }
        }
    }

    if cli.json {
        match serde_json::to_string_pretty(&rep) {
            Ok(s) => println!("{}", s),
            Err(e) => { eprintln!("loadprobe: json error: {}", e); return ExitCode::from(2); }
        }
    } else {
        report::print_text(&rep);
    }

    // exit code reflects the verdict so scripts can branch on it
    match rep.verdict {
        report::Verdict::ReachedWorld { .. } => ExitCode::SUCCESS,
        report::Verdict::Crash { .. } => ExitCode::from(10),
        report::Verdict::Hang { .. } => ExitCode::from(11),
        report::Verdict::Truncated { .. } => ExitCode::from(12),
    }
}
