//! Which wavebanks do the cues actually reach, and how many of each bank's waves do they need?
//!
//! `vo_stream.english` (0xEADF9519) is the streamed VO wavebank, and `vo_stream.english.pws` is
//! 798 MB (~2.5 h of speech). A cue reaches waves the way the engine resolves it: sounddb entry
//! `{guid, soundbank, cue index}` → soundbank cue → (every track's sounds →) group → wave
//! (`mercs2_audio::route`). So the highest wave index any group names in a wavebank is a lower bound
//! on how many records that bank must hold — the check that exposed the old record-count clamp
//! (the bank holds 12,988 records, not 29).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use mercs2_audio::route::route;
use mercs2_audio::soundbank::Soundbank;
use mercs2_audio::sounddb::SoundDb;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

// From the crate lib now; this used to be `#[path = "../names.rs"] mod names;`, the workaround
// for a crate that had no [lib] and so compiled this module once per binary.
use wad_simulator::names;
use names::RainbowTable;

const TH_SOUNDDB: u32 = 0xE527_3C14;
const TH_SOUNDBANK: u32 = 0x9F8B_CA10;
/// ASET type ids of the two tables routing needs.
const ASET_SOUNDDB: u32 = 13;
const ASET_SOUNDBANK: u32 = 21;

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

    // Every sounddb and soundbank of every WAD given: routing crosses archives (English.wad's VO
    // soundbanks play vo_stream's waves).
    let (mut dbs, mut sbs) = (Vec::new(), Vec::new());
    for wad in &cli.wad {
        let mut f = File::open(wad)?;
        let size = f.metadata()?.len();
        let arch = load_ffcs_archive(&mut f, size)?;
        let mut blocks: Vec<u16> = arch
            .aset
            .iter()
            .filter(|e| e.type_id == ASET_SOUNDDB || e.type_id == ASET_SOUNDBANK)
            .map(|e| e.block_index())
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        for blk in blocks {
            let dec = decompress_block(&mut f, &arch.indx, blk)?;
            let (parsed, _) = walk_decompressed_block(&dec, "cue");
            for (i, ent) in parsed.entries.iter().enumerate() {
                if ent.type_hash != TH_SOUNDDB && ent.type_hash != TH_SOUNDBANK {
                    continue;
                }
                let body = parsed
                    .containers
                    .get(i)
                    .and_then(|c| extract_data_chunk(c))
                    .ok_or_else(|| format!("block {blk}: 0x{:08X} has no data chunk", ent.name_hash))?;
                if ent.type_hash == TH_SOUNDDB {
                    dbs.push(SoundDb::parse(&body)?);
                } else {
                    sbs.push(Soundbank::parse(&body)?);
                }
            }
        }
    }
    let routing = route(&dbs, &sbs)?;
    println!(
        "{} sounddbs, {} soundbanks; {} references to soundbanks outside these WADs",
        dbs.len(),
        sbs.len(),
        routing.missing_soundbanks.len()
    );

    // wavebank -> (cues reaching it, highest wave index a group names)
    let mut per_bank: BTreeMap<u32, (BTreeSet<u32>, u32)> = BTreeMap::new();
    for (&(wavebank, index), uses) in &routing.waves {
        let e = per_bank.entry(wavebank).or_default();
        e.1 = e.1.max(index);
        for u in uses {
            e.0.extend(u.cues.iter().copied());
        }
    }
    let mut v: Vec<(u32, usize, u32)> = per_bank.into_iter().map(|(b, (c, m))| (b, c.len(), m)).collect();
    v.sort_by_key(|&(_, n, _)| std::cmp::Reverse(n));

    println!("\nwavebank     cues   max_wave_index   name");
    for (bank, cues, maxw) in v.iter().take(25) {
        let name = rb.resolve(*bank).unwrap_or("");
        println!("0x{bank:08X}  {cues:>5}   {maxw:>14}   {name}");
    }
    Ok(())
}
