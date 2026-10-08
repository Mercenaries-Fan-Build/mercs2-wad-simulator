//! `exemap` — identify a Mercenaries 2 executable and check it against its code layout.
//!
//! ```text
//! exemap identify <exe>
//! exemap check <exe> --reference <layout-reference-exe>
//! ```
//!
//! Exit status: 0 when the build is catalogued (and, for `check`, every difference from the layout
//! reference is a listed overlay); 1 on an unknown build or an unexpected difference; 2 when a file
//! cannot be read.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mercs2_exemap::{identify_in, membership, Catalogue};

#[derive(Parser)]
#[command(name = "exemap", version, about = "Identify a Mercenaries 2 executable and check its layout membership")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the build id of an executable, found by its sha256.
    Identify { exe: PathBuf },
    /// Print the build id, its layout, and every overlay that separates it from the layout reference.
    Check {
        exe: PathBuf,
        /// The layout's reference build (builds.tsv names it per layout in layouts.tsv).
        #[arg(long)]
        reference: PathBuf,
    },
}

fn read(path: &Path) -> Result<Vec<u8>, ExitCode> {
    std::fs::read(path).map_err(|e| {
        eprintln!("error: cannot read {}: {e}", path.display());
        ExitCode::from(2)
    })
}

fn run(cli: Cli) -> Result<(), ExitCode> {
    let catalogue = Catalogue::embedded().map_err(|e| {
        eprintln!("error: the embedded catalogue is invalid: {e}");
        ExitCode::from(2)
    })?;
    match cli.command {
        Command::Identify { exe } => {
            let bytes = read(&exe)?;
            let b = identify_in(&catalogue, &bytes).map_err(|e| {
                eprintln!("error: {}: {e}", exe.display());
                ExitCode::from(1)
            })?;
            println!("{}", b.id);
        }
        Command::Check { exe, reference } => {
            let bytes = read(&exe)?;
            let refb = read(&reference)?;
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {}: {e}", exe.display());
                ExitCode::from(1)
            };
            let b = identify_in(&catalogue, &bytes).map_err(fail)?;
            let rows = membership::check(&catalogue, &bytes, &refb).map_err(fail)?;
            println!("build   {}", b.id);
            println!("layout  {}", b.layout);
            println!("overlays {}", rows.len());
            for o in rows {
                println!(
                    "  {:<14} {:<10} {:<4} 0x{:08x}..0x{:08x}  runs {:<7} bytes {:<9} {}",
                    o.kind.as_str(),
                    o.section,
                    o.space.as_str(),
                    o.start,
                    o.end,
                    o.runs,
                    o.diff_bytes,
                    o.detail
                );
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}
