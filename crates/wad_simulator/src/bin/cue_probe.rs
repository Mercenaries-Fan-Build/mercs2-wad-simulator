//! Which wavebank do the VO cues actually route to, and how many waves do they expect?
//!
//! `vo_stream.english` (0xEADF9519) is the only streaming VO wavebank, yet it parses to just 29
//! clips addressing 1.4 MB — while `vo_stream.english.pws` is 798 MB (~2.5 h of PCM16). Something
//! is inconsistent. A sounddb cue is `{guid, bank_hash, wave_index}`, so the MAX `wave_index` any
//! cue uses for a bank is a lower bound on how many clips that bank really has. If cues reference
//! wave_index >> 29, then our record count is wrong and we are reading only the first 29 records.

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use mercs2_audio::sounddb::SoundDb;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

#[path = "../names.rs"]
mod names;
use names::RainbowTable;

const TH_SOUNDDB: u32 = 0xE527_3C14;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    wad: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let rb = {
        let t: Vec<PathBuf> = ["tools/rainbow_table.json", "docs/data/aset_block_strings.json"]
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();
        RainbowTable::load_many(&t).unwrap_or_default()
    };

    // bank_hash -> (cue count, max wave_index)
    let mut per_bank: HashMap<u32, (usize, u32)> = HashMap::new();

    for wad in &cli.wad {
        let mut f = File::open(wad)?;
        let size = f.metadata()?.len();
        let arch = load_ffcs_archive(&mut f, size)?;
        for blk in 0..arch.indx.len() {
            let Ok(dec) = decompress_block(&mut f, &arch.indx, blk as u16) else { continue };
            let (parsed, _) = walk_decompressed_block(&dec, "cue");
            for (i, ent) in parsed.entries.iter().enumerate() {
                if ent.type_hash != TH_SOUNDDB {
                    continue;
                }
                let Some(c) = parsed.containers.get(i) else { continue };
                let Some(body) = extract_data_chunk(c) else { continue };
                let Ok(db) = SoundDb::parse(&body) else { continue };
                for cue in &db.cues {
                    let e = per_bank.entry(cue.bank_hash).or_insert((0, 0));
                    e.0 += 1;
                    e.1 = e.1.max(cue.wave_index);
                }
            }
        }
    }

    let mut v: Vec<(u32, (usize, u32))> = per_bank.into_iter().collect();
    v.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));

    println!("\nbank_hash    cues   max_wave_index   name");
    for (bank, (cues, maxw)) in v.iter().take(25) {
        let name = rb.resolve(*bank).unwrap_or("");
        println!("0x{bank:08X}  {cues:>5}   {maxw:>14}   {name}");
    }
    Ok(())
}
