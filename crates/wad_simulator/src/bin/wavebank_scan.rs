//! Find EVERY wavebank container in a WAD by walking blocks, not by trusting ASET.
//!
//! `vo_extract` enumerates wavebanks via ASET rows of type 6. That found 43 banks / 1,142 clips in
//! English.wad -- but the PC `vo_stream.english.pws` is 798 MB (~2.5 h of PCM16 mono @44.1k) and
//! those banks address only ~1.4 MB of it. So the bulk of the speech, very plausibly including the
//! Mattias / Jennifer player-character variants, is indexed by something we have not found.
//!
//! A container can exist in a block without a matching ASET row of that type (one asset hash often
//! carries wavebank+soundbank+sounddb rows, and the ASET table is a lookup index, not an inventory).
//! So scan the BLOCKS: decompress each, walk its UCFX entry table, and report every container whose
//! `type_hash` is `wavebank` (0xF753F6D0) -- with how many of its clips are STREAMING (fmt 0x04),
//! since a bank that indexes the .pws is exactly a bank full of streaming clips.

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use rayon::prelude::*;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

#[path = "../names.rs"]
mod names;
use names::RainbowTable;

const TH_WAVEBANK: u32 = 0xF753_F6D0;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    wad: PathBuf,
    /// Only report banks with at least this many streaming (fmt 0x04) clips.
    #[arg(long, default_value_t = 0)]
    min_streaming: usize,
    /// Container type_hash to scan for. Default = wavebank. Pass 0x9F8BCA10 for `soundbank`:
    /// the cues route to per-character banks (vo_mattias, vo_Jen, …) expecting 500+ waves each,
    /// but those are NOT wavebank containers — so their wave table must live in the soundbank.
    #[arg(long, default_value_t = 0xF753_F6D0)]
    type_hash: u32,
    /// Dump the header + first bytes of the container with this name hash.
    #[arg(long, default_value_t = 0)]
    dump: u32,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

struct BankInfo {
    name_hash: u32,
    block: u16,
    clips: usize,
    streaming: usize,
    embedded: usize,
    max_end: u64,
    stream_bytes: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let rb = {
        let tables: Vec<PathBuf> = [
            "tools/rainbow_table.json",
            "docs/data/aset_discovered_names.json",
            "docs/data/aset_block_strings.json",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
        RainbowTable::load_many(&tables).unwrap_or_default()
    };

    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;
    let n_blocks = arch.indx.len();
    eprintln!("{}: {n_blocks} blocks — scanning for wavebank containers…", cli.wad.display());

    let found: Vec<BankInfo> = (0..n_blocks)
        .into_par_iter()
        .flat_map_iter(|blk| {
            let mut out: Vec<BankInfo> = Vec::new();
            let Ok(mut fh) = File::open(&cli.wad) else { return out.into_iter() };
            let Ok(dec) = decompress_block(&mut fh, &arch.indx, blk as u16) else {
                return out.into_iter();
            };
            let (parsed, _) = walk_decompressed_block(&dec, "scan");
            for (i, ent) in parsed.entries.iter().enumerate() {
                if ent.type_hash != cli.type_hash {
                    continue;
                }
                let Some(container) = parsed.containers.get(i) else { continue };
                let Some(body) = extract_data_chunk(container) else { continue };
                if body.len() < 24 {
                    continue;
                }
                if cli.dump != 0 && ent.name_hash == cli.dump {
                    let hex: Vec<String> =
                        body[..96.min(body.len())].iter().map(|b| format!("{b:02X}")).collect();
                    let words: Vec<String> =
                        (0..12).map(|w| format!("+{:02}=0x{:08X}", w * 4, rd32(&body, w * 4))).collect();
                    eprintln!(
                        "\nDUMP 0x{:08X}: body {} B\n  {}\n  head: {}",
                        ent.name_hash,
                        body.len(),
                        words.join(" "),
                        hex.join(" ")
                    );
                }
                let capacity = rd32(&body, 0) as usize;
                let populated = u16::from_le_bytes([body[8], body[9]]) as usize;
                let roff = rd32(&body, 16) as usize;
                let n = if populated > 0 && populated <= capacity { populated } else { capacity };
                if n == 0 || n > 100_000 || roff >= body.len() {
                    continue;
                }
                let (mut streaming, mut embedded, mut max_end, mut sbytes) = (0, 0, 0u64, 0u64);
                for k in 0..n {
                    let r = roff + k * 36;
                    if r + 36 > body.len() {
                        break;
                    }
                    let fmt = body[r + 6];
                    let sz = rd32(&body, r + 12) as u64;
                    let off = rd32(&body, r + 32) as u64;
                    if sz == 0 {
                        continue;
                    }
                    if fmt == 0x04 || off + sz > body.len() as u64 {
                        streaming += 1;
                        sbytes += sz;
                        max_end = max_end.max(off + sz);
                    } else {
                        embedded += 1;
                    }
                }
                out.push(BankInfo {
                    name_hash: ent.name_hash,
                    block: blk as u16,
                    clips: n,
                    streaming,
                    embedded,
                    max_end,
                    stream_bytes: sbytes,
                });
            }
            out.into_iter()
        })
        .collect();

    let mut banks = found;
    banks.sort_by_key(|b| std::cmp::Reverse(b.streaming));

    let total_clips: usize = banks.iter().map(|b| b.clips).sum();
    let total_stream: usize = banks.iter().map(|b| b.streaming).sum();
    println!(
        "\n{} wavebank containers, {total_clips} clips ({total_stream} streaming)",
        banks.len()
    );

    println!("\nname                                   block  clips  strm  embed   stream_MB  furthest_MB");
    let mut shown = 0;
    for b in &banks {
        if b.streaming < cli.min_streaming {
            continue;
        }
        let path = arch.paths.get(b.block as usize).map(|s| s.as_str()).unwrap_or("");
        let label = rb
            .resolve(b.name_hash)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("0x{:08X}", b.name_hash));
        let short = path.rsplit(['\\', '/']).next().unwrap_or("").trim_end_matches(".block");
        println!(
            "{:<38} {:>5}  {:>5}  {:>4}  {:>5}  {:>9.1}  {:>10.1}   [{}]",
            label,
            b.block,
            b.clips,
            b.streaming,
            b.embedded,
            b.stream_bytes as f64 / 1e6,
            b.max_end as f64 / 1e6,
            short
        );
        shown += 1;
        if shown > 40 {
            println!("  … ({} more)", banks.len() - shown);
            break;
        }
    }
    Ok(())
}
