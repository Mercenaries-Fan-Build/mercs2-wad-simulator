//! `mercs2_autoplay` CLI.
//!
//! ```text
//! mercs2_autoplay boot --trace <out.ndjson> [--hero mattias|jennifer|chris] [--vz-wad <path>]
//! ```
//!
//! `boot` boots a new game headless, records every call the Lua makes to `--trace` (see
//! `mercs2_autoplay::trace` for the record kinds), and prints the live task tree once the world load
//! has finished.
//! Without `--vz-wad` the archive comes from `MERCS2_VZ_WAD`, then the checkout's
//! `.mercs2-local.toml` (`mercs2_formats::game_paths::vz_wad`). No archive is an error: there is no
//! world-less mode.
//!
//! Exit codes: 0 booted, 1 the boot ran but did not finish, 2 could not run (bad arguments, no archive).

use std::path::PathBuf;
use std::process::ExitCode;

use mercs2_autoplay::boot::{self, BootError, Hero};
use mercs2_autoplay::tasks;

const USAGE: &str =
    "usage: mercs2_autoplay boot --trace <out.ndjson> [--hero mattias|jennifer|chris] [--vz-wad <path>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("boot") => cmd_boot(&args[1..]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn cmd_boot(args: &[String]) -> ExitCode {
    let mut hero = Hero::Chris;
    let mut vz_wad: Option<PathBuf> = None;
    let mut trace: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--hero" => match it.next().map(|s| Hero::parse(s)) {
                Some(Ok(h)) => hero = h,
                Some(Err(e)) => return usage_error(&e),
                None => return usage_error("--hero needs a value"),
            },
            "--vz-wad" => match it.next() {
                Some(p) => vz_wad = Some(PathBuf::from(p)),
                None => return usage_error("--vz-wad needs a value"),
            },
            "--trace" => match it.next() {
                Some(p) => trace = Some(PathBuf::from(p)),
                None => return usage_error("--trace needs a value"),
            },
            other => return usage_error(&format!("unknown argument '{other}'")),
        }
    }
    let Some(trace) = trace else { return usage_error("--trace is required") };

    let vz_wad = match vz_wad {
        Some(p) => p,
        None => {
            let cwd = match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => return usage_error(&format!("cannot read the working directory: {e}")),
            };
            match mercs2_formats::game_paths::vz_wad(&cwd) {
                Some(p) => p,
                None => {
                    return usage_error(
                        "no vz.wad: pass --vz-wad, set MERCS2_VZ_WAD, or run scripts/find-vz-wad.sh --write \
                         from tools/wad_simulator",
                    )
                }
            }
        }
    };

    let session = match boot::boot_new_game(&vz_wad, hero, &trace) {
        Ok(s) => s,
        Err(e @ BootError::DidNotSettle { .. }) => {
            eprintln!("[autoplay] {e}");
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("[autoplay] {e}");
            return ExitCode::from(2);
        }
    };

    let counts = session.trace.counts();
    println!(
        "[autoplay] trace {}: {} engine, {} lua, {} stub records",
        trace.display(),
        counts.engine,
        counts.lua,
        counts.stub
    );
    match tasks::snapshot(&session) {
        Ok(nodes) => {
            println!("[autoplay] booted as {} — task tree ({} nodes):", hero.as_str(), nodes.len());
            for n in &nodes {
                println!("[autoplay]   {:<9} {}", format!("{:?}", n.state), n.lineage);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[autoplay] task tree: {e}");
            ExitCode::from(1)
        }
    }
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("[autoplay] {msg}\n{USAGE}");
    ExitCode::from(2)
}
