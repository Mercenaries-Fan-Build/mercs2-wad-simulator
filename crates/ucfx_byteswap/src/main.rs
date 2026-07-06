use clap::Parser;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use ucfx_byteswap::{aset, convert, report, validate};

use report::SchemaCoverageReport;

#[derive(Parser)]
#[command(name = "ucfx_byteswap", about = "Convert Xbox 360 BE UCFX blocks to PC LE format")]
struct Cli {
    /// Input decompressed BE block file (omit when using --stdin)
    input: Option<PathBuf>,

    /// Output LE block file (omit when using --stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Read input from stdin instead of a file
    #[arg(long)]
    stdin: bool,

    /// Write output to stdout instead of a file
    #[arg(long)]
    stdout: bool,

    /// Dry-run: parse and report without writing output
    #[arg(long)]
    dry_run: bool,

    /// Skip validation checks on converted output
    #[arg(long)]
    no_validate: bool,

    /// Treat validation errors as fatal (non-zero exit, skip writing)
    #[arg(long)]
    strict: bool,

    /// Print a schema field coverage report after conversion
    #[arg(long)]
    report_schema_coverage: bool,

    /// Validate an existing PC LE block (no BE→LE conversion). For stage-2 / retail blobs.
    #[arg(long)]
    validate_only: bool,

    /// ASET sub-entry recompute mode (stdin→stdout). Input protocol:
    ///   [u32 n_entries] then n×[u32 hash][u32 u32_2][u8 primary][u8 in_base]
    ///   then the decompressed LE block bytes. Output: n×[u32 updated_u32_2].
    /// Faithful Rust replacement for dlc_port `_recompute_aset_sub_entries`.
    #[arg(long)]
    aset_recompute: bool,

    /// Untile a raw tiled Xbox DXT BODY file into the PC-linear mip chain.
    /// Requires --tex-w, --tex-h, --tex-fourcc, --tex-mips and --output.
    #[arg(long)]
    untile_tex: bool,

    #[arg(long)]
    tex_w: Option<usize>,
    #[arg(long)]
    tex_h: Option<usize>,
    /// FourCC, e.g. DXT1 or DXT5.
    #[arg(long)]
    tex_fourcc: Option<String>,
    #[arg(long)]
    tex_mips: Option<usize>,
}

/// Handle `--aset-recompute`: read the per-block protocol from `input`, run the
/// ported recompute, and write the updated `u32_2` values to stdout.
fn run_aset_recompute(input: &[u8]) -> i32 {
    if input.len() < 4 {
        eprintln!("aset-recompute: short input");
        return 1;
    }
    let n = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
    const REC: usize = 10; // u32 hash + u32 u32_2 + u8 primary + u8 in_base
    let header_end = 4 + REC * n;
    if input.len() < header_end {
        eprintln!("aset-recompute: truncated entry table");
        return 1;
    }
    let mut entries: Vec<aset::AsetEntry> = Vec::with_capacity(n);
    for i in 0..n {
        let o = 4 + i * REC;
        let hash = u32::from_le_bytes([input[o], input[o + 1], input[o + 2], input[o + 3]]);
        let u32_2 = u32::from_le_bytes([input[o + 4], input[o + 5], input[o + 6], input[o + 7]]);
        let primary = input[o + 8] != 0;
        let in_base = input[o + 9] != 0;
        entries.push(aset::AsetEntry { asset_hash: hash, u32_2, primary, in_base });
    }
    let block = &input[header_end..];
    aset::recompute_block_aset_subs(block, &mut entries);
    let mut out = Vec::with_capacity(4 * n);
    for e in &entries {
        out.extend_from_slice(&e.u32_2.to_le_bytes());
    }
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if let Err(e) = handle.write_all(&out) {
        eprintln!("aset-recompute: stdout write failed: {}", e);
        return 1;
    }
    0
}

fn run_validation(data: &[u8], strict: bool, quiet: bool) -> bool {
    let issues = validate::validate_converted_block(data);
    if issues.is_empty() {
        if !quiet {
            println!("  Validation: OK (all checks passed)");
        }
        return false;
    }
    eprintln!("  Validation: {} issue(s) found:", issues.len());
    for issue in &issues {
        eprintln!("    WARN: {}", issue);
    }
    if strict {
        eprintln!("Strict mode: aborting due to validation errors");
        std::process::exit(2);
    }
    true
}

fn main() {
    let cli = Cli::parse();

    let pipe_mode = cli.stdin || cli.stdout;

    if !cli.stdin && cli.input.is_none() {
        eprintln!("Error: provide an input file or use --stdin");
        std::process::exit(1);
    }

    let input_data = if cli.stdin {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
            eprintln!("Error reading stdin: {}", e);
            std::process::exit(1);
        }
        buf
    } else {
        let path = cli.input.as_ref().unwrap();
        match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error reading {}: {}", path.display(), e);
                std::process::exit(1);
            }
        }
    };

    if cli.aset_recompute {
        std::process::exit(run_aset_recompute(&input_data));
    }

    if cli.untile_tex {
        let w = cli.tex_w.expect("--tex-w required");
        let h = cli.tex_h.expect("--tex-h required");
        let fcc_s = cli.tex_fourcc.expect("--tex-fourcc required");
        let mips = cli.tex_mips.expect("--tex-mips required");
        let fb = fcc_s.as_bytes();
        if fb.len() != 4 {
            eprintln!("--tex-fourcc must be 4 chars");
            std::process::exit(1);
        }
        let fourcc = [fb[0], fb[1], fb[2], fb[3]];
        match convert::untile_tiled_dxt_body(&input_data, w, h, &fourcc, mips) {
            Some(out) => {
                let out_path = cli.output.expect("--output required");
                if let Err(e) = std::fs::write(&out_path, &out) {
                    eprintln!("Error writing {}: {}", out_path.display(), e);
                    std::process::exit(1);
                }
                println!("untile-tex: wrote {} bytes to {}", out.len(), out_path.display());
                std::process::exit(0);
            }
            None => {
                eprintln!("untile-tex: conversion failed (body too short?)");
                std::process::exit(1);
            }
        }
    }

    if cli.validate_only {
        if cli.dry_run || cli.report_schema_coverage || cli.stdout {
            eprintln!("Error: --validate-only cannot be combined with --dry-run, --report-schema-coverage, or --stdout");
            std::process::exit(1);
        }
        if !pipe_mode {
            println!("ucfx_byteswap: validate-only ({} bytes)", input_data.len());
        }
        let failed = run_validation(&input_data, cli.strict, pipe_mode);
        std::process::exit(if failed { 1 } else { 0 });
    }

    if !pipe_mode {
        println!("ucfx_byteswap: processing ({} bytes)", input_data.len());
    }

    let mut report = if cli.report_schema_coverage {
        Some(SchemaCoverageReport::default())
    } else {
        None
    };

    match convert::convert_block(&input_data, cli.dry_run, report.as_mut()) {
        Ok(output) => {
            if let Some(rpt) = &report {
                rpt.print_report();
            }

            if cli.dry_run {
                if !pipe_mode {
                    println!("Dry run complete.");
                }
                return;
            }

            if !cli.no_validate {
                let failed = run_validation(&output, false, pipe_mode);
                if cli.strict && failed {
                    eprintln!("Strict mode: aborting due to validation errors");
                    std::process::exit(2);
                }
            }

            if cli.stdout {
                let stdout = io::stdout();
                let mut handle = stdout.lock();
                if let Err(e) = handle.write_all(&output) {
                    eprintln!("Error writing to stdout: {}", e);
                    std::process::exit(1);
                }
            } else if let Some(out_path) = cli.output {
                if let Err(e) = std::fs::write(&out_path, &output) {
                    eprintln!("Error writing {}: {}", out_path.display(), e);
                    std::process::exit(1);
                }
                if !pipe_mode {
                    println!("Wrote {} bytes to {}", output.len(), out_path.display());
                }
            } else {
                eprintln!("No output path specified (use --output, --stdout, or --dry-run)");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Conversion error: {}", e);
            std::process::exit(1);
        }
    }
}
