//! loadprobe — world-load progress + forensic analyzer for `pmc_blackbox.log`.
//!
//! Scores how far a Mercenaries 2 world-load got against the milestone ladder,
//! classifies the end-state (REACHED-WORLD / CRASH@EIP / HANG / TRUNCATED), and
//! surfaces every non-routine ([lua]/[pool]) diagnostic line + high-signal Lua markers.

mod parse;
mod phases;
mod report;
mod sha256;
mod symbolize;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_LOG: &str = "C:/Users/Shadow/Desktop/Mercenaries 2 World in Flames/pmc_blackbox.log";
const DEFAULT_EXE_SYMBOLS: &str =
    "C:/Users/Shadow/Desktop/notes-on-the-released-game/scripts/mercs2_annotations.json";

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
    #[arg(long, default_value = DEFAULT_EXE_SYMBOLS)]
    exe_symbols: PathBuf,

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

    let path = cli.log.unwrap_or_else(|| PathBuf::from(DEFAULT_LOG));
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
            let mut sym = symbolize::Symbolizer::new(&cli.exe_symbols, dirs);
            if sym.has_any_source() {
                for line in crash.block.iter_mut() {
                    *line = sym.rewrite_line(line);
                }
            } else {
                eprintln!("loadprobe: --symbolize found no symbol sources \
                    (exe map {} and no .asi/.dll next to the log)", cli.exe_symbols.display());
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
