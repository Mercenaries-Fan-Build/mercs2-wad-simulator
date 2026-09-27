//! Extract EMBEDDED sound-effect waves (weapons, vehicles, ambience, explosions) to `.wav`.
//!
//! The counterpart to `vo_extract`. Speech is the awkward case — one wavebank of codec-`0x04`
//! records that merely index `vo_stream.<lang>.pws`, so extracting it needs the WAD *and* the
//! stream file. SFX are the easy case and were never wired up: every weapon/vehicle/ambience
//! bank ships its samples **inside its own block**, IMA-ADPCM or PCM16, nothing external.
//!
//!   block (e.g. `wpn_pistol_P000_Q3`)
//!     ├─ wavebank  0xF753F6D0 — the samples themselves (`Wavebank::parse` decodes them)
//!     ├─ soundbank 0x9F8BCA10 — cues and the groups of waves they play
//!     └─ sounddb   0xE5273C14 — `{cue_guid, soundbank, soundbank cue index}` routing records
//!
//! A wave has no name of its own; it is named by the cue that plays it, found the way the engine
//! finds it: sounddb entry → soundbank cue → (every track's sounds →) group → wave
//! (`mercs2_audio::route`). The guid is reversed through the rainbow table
//! (`pandemic_hash_m2(cue_name) == guid`). Unresolved guids are emitted as `cue_<hash>` rather than
//! dropped — the audio is still correct, only the label is missing, and a later rainbow-table pass
//! renames it. A group no cue reaches still qualifies its waves' names with its group and slot.
//!
//! ```text
//! cargo run --release -p wad_simulator --bin sfx_extract -- \
//!     --wad game-files/vz.wad --filter wpn_ --out output/sfx_wav
//! cargo run --release -p wad_simulator --bin sfx_extract -- --wad game-files/vz.wad --list
//! ```

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::route::{route, Routing};
use mercs2_audio::soundbank::Soundbank;
use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::Wavebank;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

// From the crate lib now; this used to be `#[path = "../names.rs"] mod names;`, the workaround
// for a crate that had no [lib] and so compiled this module once per binary.
use wad_simulator::names;
use names::RainbowTable;

const TH_WAVEBANK: u32 = 0xF753_F6D0;
const TH_SOUNDDB: u32 = 0xE527_3C14;
const TH_SOUNDBANK: u32 = 0x9F8B_CA10;
/// ASET rows key on a small `type_id`, NOT the type hash — filtering the table by `TH_SOUNDDB`
/// silently matches nothing.
const ASET_TYPE_SOUNDDB: u32 = 13;
const ASET_TYPE_SOUNDBANK: u32 = 21;

#[derive(Parser)]
#[command(about = "Extract embedded SFX wavebanks (weapons/vehicles/ambience) from a WAD to .wav")]
struct Cli {
    #[arg(long)]
    wad: PathBuf,
    /// Substring the block path must contain. `wpn_` = weapons, `veh_` = vehicles, `amb_` =
    /// ambience, `` (empty) = every block in the WAD.
    #[arg(long, default_value = "wpn_")]
    filter: String,
    /// Output root; one subdirectory per source block.
    #[arg(long, default_value = "output/sfx_wav")]
    out: PathBuf,
    /// Report what would be extracted without writing any file.
    #[arg(long)]
    list: bool,
    /// Route through every `sounddb` and `soundbank` in the WAD rather than only the ones sharing a
    /// block with the bank. The engine merges all resident sounddbs into one catalog, and a group may
    /// play another block's waves — so this names more waves.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    global_cues: bool,
    /// Extra rainbow-table fragment(s) to load — e.g. the `--emit` output of `sfx_namecrack`.
    #[arg(long)]
    names: Vec<PathBuf>,
}

/// The `sounddb` and `soundbank` tables of `blocks`. A table outside the measured layout is an error.
fn block_tables(
    f: &mut File,
    arch: &mercs2_formats::ffcs::FfcsArchive,
    blocks: &[u16],
) -> Result<(Vec<SoundDb>, Vec<Soundbank>), Box<dyn std::error::Error>> {
    let (mut dbs, mut sbs) = (Vec::new(), Vec::new());
    for &blk in blocks {
        let dec = decompress_block(f, &arch.indx, blk)?;
        let (parsed, _) = walk_decompressed_block(&dec, "cues");
        tables_of(&parsed, &mut dbs, &mut sbs)?;
    }
    Ok((dbs, sbs))
}

fn tables_of(
    parsed: &mercs2_formats::ucfx::ParsedBlock,
    dbs: &mut Vec<SoundDb>,
    sbs: &mut Vec<Soundbank>,
) -> Result<(), Box<dyn std::error::Error>> {
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_SOUNDDB && ent.type_hash != TH_SOUNDBANK {
            continue;
        }
        let body = parsed
            .containers
            .get(i)
            .and_then(|c| extract_data_chunk(c))
            .ok_or_else(|| format!("0x{:08X}: audio container without a data chunk", ent.name_hash))?;
        if ent.type_hash == TH_SOUNDDB {
            dbs.push(SoundDb::parse(&body)?);
        } else {
            sbs.push(Soundbank::parse(&body)?);
        }
    }
    Ok(())
}

/// Route every sounddb and soundbank the ASET table points at. Only the ~70 blocks that carry one
/// are read, so this is a cheap targeted pass, not a whole-WAD sweep.
fn global_routing(
    f: &mut File,
    arch: &mercs2_formats::ffcs::FfcsArchive,
) -> Result<Routing, Box<dyn std::error::Error>> {
    let mut blocks: Vec<u16> = arch
        .aset
        .iter()
        .filter(|e| e.type_id == ASET_TYPE_SOUNDDB || e.type_id == ASET_TYPE_SOUNDBANK)
        .map(|e| e.block_index())
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    let (dbs, sbs) = block_tables(f, arch, &blocks)?;
    Ok(route(&dbs, &sbs)?)
}

/// Sanitize a cue name into a filename.
fn safe(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect()
}

fn write_wav(path: &std::path::Path, pcm: &[i16], channels: u16, rate: u32) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    let data_len = (pcm.len() * 2) as u32;
    let byte_rate = rate * channels as u32 * 2;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&(channels * 2).to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

fn rainbow(extra: &[PathBuf]) -> RainbowTable {
    let mut tables: Vec<PathBuf> = [
        "tools/rainbow_table.json",
        "docs/data/aset_discovered_names.json",
        "docs/data/aset_block_strings.json",
    ]
    .iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect();
    tables.extend(extra.iter().filter(|p| p.exists()).cloned());
    RainbowTable::load_many(&tables).unwrap_or_default()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let rb = rainbow(&cli.names);
    eprintln!("rainbow: {} names", rb.len());

    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;

    // Blocks whose path matches the filter, in WAD order.
    let blocks: Vec<(u16, String)> = arch
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.contains(&cli.filter))
        .map(|(i, p)| {
            let short = p
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(p)
                .trim_end_matches(".block")
                .to_string();
            (i as u16, short)
        })
        .collect();
    eprintln!(
        "{}: {} of {} blocks match {:?}",
        cli.wad.display(),
        blocks.len(),
        arch.paths.len(),
        cli.filter
    );

    let global = if cli.global_cues {
        let r = global_routing(&mut f, &arch)?;
        eprintln!(
            "global routing: {} waves in groups, {} references to soundbanks not in this WAD",
            r.waves.len(),
            r.missing_soundbanks.len()
        );
        Some(r)
    } else {
        None
    };

    let (mut banks, mut written, mut named, mut skipped_empty) = (0usize, 0usize, 0usize, 0usize);
    let mut total_secs = 0.0f64;

    for (blk, short) in &blocks {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, *blk) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "sfx");

        // Pass 1 — the routing that names each wave: WAD-wide, or this block's tables alone.
        let local;
        let routing = match &global {
            Some(r) => r,
            None => {
                let (mut dbs, mut sbs) = (Vec::new(), Vec::new());
                tables_of(&parsed, &mut dbs, &mut sbs)?;
                local = route(&dbs, &sbs)?;
                &local
            }
        };

        // Pass 2 — decode each wavebank's embedded clips.
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_WAVEBANK {
                continue;
            }
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else { continue };
            let bank = Wavebank::parse(&body)?;
            if bank.clips.is_empty() {
                continue;
            }
            banks += 1;
            let dir = cli.out.join(safe(short));
            let mut wrote_here = 0usize;

            for (idx, clip) in bank.clips.iter().enumerate() {
                if clip.streaming || clip.samples.is_empty() {
                    skipped_empty += 1;
                    continue;
                }
                // Name priority: the cue that routes here > the clip's own hash > bare index.
                let label = routing
                    .first_cue(bank.self_hash, idx as u32)
                    .and_then(|g| {
                        rb.resolve(g).map(|s| s.to_string()).or(Some(format!("cue_{g:08X}")))
                    })
                    .or_else(|| rb.resolve(clip.clip_hash).map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("wave_{:08X}", clip.clip_hash));
                if !label.starts_with("cue_") && !label.starts_with("wave_") {
                    named += 1;
                }
                // Several waves share one cue name (layers + random takes), so qualify with the
                // group/slot the soundbank put this wave in — otherwise they collide on disk.
                let label = match routing.first_use(bank.self_hash, idx as u32) {
                    Some(u) => format!("{label}_g{}_{}", u.group_index, u.slot),
                    None => label,
                };

                let rate = if clip.sample_rate == 0 { 44100 } else { clip.sample_rate };
                let ch = clip.channels.max(1) as u16;
                total_secs += clip.frames() as f64 / rate as f64;

                if !cli.list {
                    std::fs::create_dir_all(&dir)?;
                    let path = dir.join(format!("{idx:03}_{}.wav", safe(&label)));
                    write_wav(&path, &clip.samples, ch, rate)?;
                }
                written += 1;
                wrote_here += 1;
            }

            println!(
                "{short:<34} bank 0x{:08X}  {:>3} clips  {:>3} decoded",
                bank.self_hash,
                bank.clips.len(),
                wrote_here
            );
        }
    }

    println!(
        "\n{banks} banks, {written} waves {} ({named} cue-named, {skipped_empty} streaming/empty), {:.1} s of audio",
        if cli.list { "found" } else { "written" },
        total_secs
    );
    if !cli.list {
        println!("out: {}", cli.out.display());
    }
    Ok(())
}
