//! CLI front-end for [`securom_unwrap`]: produce a SecuROM-free copy of a
//! cracked-but-decrypted Mercenaries 2 PC executable.

use clap::Parser;
use securom_unwrap::{unwrap, Options};
use std::path::PathBuf;
use std::process::ExitCode;

/// Remove SecuROM from a cracked-but-decrypted Mercenaries 2 PC executable by
/// restoring the original entry point and a clean import table. All sections
/// (including SecuROM's, which hold relocated game code) are kept verbatim.
#[derive(Parser)]
#[command(name = "securom_unwrap", version, about)]
struct Cli {
    /// Input PE (already-decrypted, e.g. a RELOADED-unpacked build).
    input: PathBuf,
    /// Output path for the SecuROM-free PE.
    output: PathBuf,
    /// Override the derived OEP, as a hex RVA (e.g. 0x5ee71c).
    #[arg(long, value_parser = parse_hex)]
    oep: Option<u32>,
    /// Extra section name(s) to treat as SecuROM's (repeatable).
    #[arg(long = "securom-section")]
    securom_section: Vec<String>,
}

fn parse_hex(s: &str) -> Result<u32, String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| format!("invalid hex '{s}': {e}"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let data = match std::fs::read(&cli.input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", cli.input.display());
            return ExitCode::FAILURE;
        }
    };

    let mut opts = Options::default();
    opts.oep = cli.oep;
    opts.securom_sections.extend(cli.securom_section);

    let (out, rep) = match unwrap(&data, &opts) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = std::fs::write(&cli.output, &out) {
        eprintln!("error: cannot write {}: {e}", cli.output.display());
        return ExitCode::FAILURE;
    }

    println!("OEP            : 0x{:08x}  (was 0x{:08x})", rep.oep, rep.original_entry);
    println!("IAT directory  : 0x{:08x} ..+0x{:x}", rep.iat_rva, rep.iat_size);
    println!("kept imports   : {}", rep.kept.len());
    for e in &rep.kept {
        println!("    {:<16} {} thunks", e.dll, e.thunk_count);
    }
    let dropped: Vec<&str> = rep.dropped.iter().map(|e| e.dll.as_str()).collect();
    println!("dropped SecuROM: {} [{}]", rep.dropped.len(), dropped.join(", "));
    println!("wrote          : {} ({} bytes)", cli.output.display(), out.len());
    ExitCode::SUCCESS
}
