//! `ps3_dlc_crypt` — the PS3 "Blow It Up Again" DLC decrypt chain as a CLI.
//!
//! Rust promotion of the four reference scripts (`ps3_pkg_unpack.py`,
//! `edat_decrypt.py`, `unself_decrypt.py`, `klic_scan_*.py`). All logic lives in
//! `mercs2_formats::ps3_*`; this binary is the thin front end. It operates on
//! external `.pkg`/`.edat`/EBOOT files, so — unlike `mercs2_probe` — it does not
//! require a `vz.wad`.
//!
//! The end of the chain (decrypted inner WAD, big-endian `SCFF`) feeds straight
//! into the existing `dlc_input` + `ucfx_byteswap` + `patch_wad` pipeline that
//! `dlc_port` already uses for the Xbox 360 route.

use clap::{Parser, Subcommand};
use mercs2_formats::{ps3_edat, ps3_keys, ps3_klic, ps3_pkg, ps3_self};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "ps3_dlc_crypt",
    about = "PS3 DLC decrypt chain: PSN PKG -> NPDRM EDAT -> inner WAD, plus SELF->ELF and klicensee recovery"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse + AES-128-CTR-decrypt a retail PSN .pkg; list entries or extract them.
    PkgUnpack {
        /// Path to the .pkg file.
        #[arg(long)]
        pkg: PathBuf,
        /// Extract files to this directory (omit to only list the table).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Decrypt an NPDRM EDAT to its inner payload (the big-endian SCFF WAD).
    EdatDecrypt {
        /// Path to the .edat file.
        #[arg(long)]
        edat: PathBuf,
        /// Output path for the decrypted payload.
        #[arg(long)]
        out: PathBuf,
        /// Klicensee as 32 hex chars. Omit to use the recovered DLC01 klicensee.
        #[arg(long)]
        klic: Option<String>,
    },
    /// Decrypt a SCE SELF (APP or NPDRM) to a plain PPC64 ELF.
    Unself {
        /// Path to the SELF (e.g. EBOOT.BIN).
        #[arg(long = "self")]
        self_in: PathBuf,
        /// Output ELF path.
        #[arg(long)]
        out: PathBuf,
        /// Use the NPDRM keyset (v1.03 patch EBOOT). Default is the disc APP keyset.
        #[arg(long)]
        npdrm: bool,
    },
    /// Recover a title klicensee by sliding AES-CMAC over a binary (e.g. a
    /// decrypted EBOOT), validated against an EDAT header. With --check, just
    /// validate one candidate.
    KlicScan {
        /// EDAT file supplying the validation header (first 0x70 bytes).
        #[arg(long)]
        edat: PathBuf,
        /// Binary to scan for the klicensee.
        #[arg(long, required_unless_present = "check")]
        bin: Option<PathBuf>,
        /// Validate a single 32-hex-char candidate instead of scanning.
        #[arg(long)]
        check: Option<String>,
    },
}

fn parse_klic(s: &str) -> Result<[u8; 16], String> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() != 32 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("klic must be 32 hex chars, got {:?}", s));
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    Ok(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn read(p: &PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))
}

fn run() -> Result<(), String> {
    match Cli::parse().cmd {
        Cmd::PkgUnpack { pkg, out } => {
            let blob = read(&pkg)?;
            let pkg = ps3_pkg::parse_pkg(&blob)?;
            println!(
                "content_id={} rev=0x{:04x} type=0x{:04x} entries={}",
                pkg.content_id,
                pkg.revision,
                pkg.pkg_type,
                pkg.entries.len()
            );
            for e in &pkg.entries {
                println!("  [{:2}] {:>12}  {}", e.type_id(), e.size, e.name);
            }
            if let Some(dir) = out {
                for e in &pkg.entries {
                    let dst = dir.join(&e.name);
                    if e.is_dir() {
                        std::fs::create_dir_all(&dst).map_err(|x| x.to_string())?;
                        continue;
                    }
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).map_err(|x| x.to_string())?;
                    }
                    let bytes = pkg
                        .file_bytes(&e.name)
                        .ok_or_else(|| format!("no bytes for {}", e.name))?;
                    std::fs::write(&dst, bytes).map_err(|x| x.to_string())?;
                }
                println!("extracted to {}", dir.display());
            }
            Ok(())
        }
        Cmd::EdatDecrypt { edat, out, klic } => {
            let blob = read(&edat)?;
            let klic = match klic {
                Some(s) => parse_klic(&s)?,
                None => {
                    println!("using recovered DLC01 klicensee");
                    ps3_keys::dlc01_klicensee()
                }
            };
            let payload = ps3_edat::decrypt_edat(&blob, &klic)?;
            let magic = &payload[0..4.min(payload.len())];
            let named = match magic {
                b"SCFF" => "SCFF (big-endian FFCS WAD)",
                b"FFCS" => "FFCS (little-endian WAD)",
                b"segs" | b"sges" => "sges block",
                _ => "unknown",
            };
            std::fs::write(&out, &payload).map_err(|x| x.to_string())?;
            println!(
                "decrypted {} bytes -> {}  (magic {:02x?} = {named})",
                payload.len(),
                out.display(),
                magic
            );
            if magic != b"SCFF" && magic != b"FFCS" {
                return Err("payload magic is not a WAD — wrong klicensee?".into());
            }
            Ok(())
        }
        Cmd::Unself {
            self_in,
            out,
            npdrm,
        } => {
            let blob = read(&self_in)?;
            let kind = if npdrm {
                ps3_self::SelfKind::Npdrm
            } else {
                ps3_self::SelfKind::App
            };
            let elf = ps3_self::decrypt_self(&blob, kind)?;
            std::fs::write(&out, &elf).map_err(|x| x.to_string())?;
            let ok = ps3_self::is_ppc64_elf(&elf);
            println!(
                "decrypted SELF ({kind:?}) -> {} ({} bytes){}",
                out.display(),
                elf.len(),
                if ok {
                    ""
                } else {
                    "  [WARNING: not a PPC64 ELF64 — check keyset/kind]"
                }
            );
            if !ok {
                return Err("output is not a valid PPC64 ELF".into());
            }
            Ok(())
        }
        Cmd::KlicScan { edat, bin, check } => {
            let head = read(&edat)?;
            if let Some(cand) = check {
                let klic = parse_klic(&cand)?;
                let ok = ps3_klic::validate_klicensee(&head, &klic);
                println!("candidate {} : {}", hex(&klic), if ok { "VALID" } else { "no match" });
                return if ok {
                    Ok(())
                } else {
                    Err("candidate did not validate".into())
                };
            }
            let bin = bin.expect("clap requires --bin unless --check");
            let haystack = read(&bin)?;
            let hits = ps3_klic::scan_for_klicensee(&head, &haystack)?;
            if hits.is_empty() {
                return Err(format!(
                    "no klicensee found in {} ({} bytes scanned)",
                    bin.display(),
                    haystack.len()
                ));
            }
            for (off, klic) in &hits {
                println!("FOUND off=0x{off:x} klic={}", hex(klic));
            }
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ps3_dlc_crypt: error: {e}");
            ExitCode::FAILURE
        }
    }
}
