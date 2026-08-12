//! Extract EMBEDDED sound-effect waves (weapons, vehicles, ambience, explosions) to `.wav`.
//!
//! The counterpart to `vo_extract`. Speech is the awkward case — one wavebank of codec-`0x04`
//! records that merely index `vo_stream.<lang>.pws`, so extracting it needs the WAD *and* the
//! stream file. SFX are the easy case and were never wired up: every weapon/vehicle/ambience
//! bank ships its samples **inside its own block**, IMA-ADPCM or PCM16, nothing external.
//!
//!   block (e.g. `wpn_pistol_P000_Q3`)
//!     ├─ wavebank  0xF753F6D0 — the samples themselves (`Wavebank::parse` decodes them)
//!     └─ sounddb   0xE5273C14 — `{cue_guid, bank_hash, wave_index}` routing records
//!
//! A wave has no name of its own; it is named by whichever cue routes to it. So the sounddb in
//! the same block is read first to build `(bank_hash, wave_index) -> cue_guid`, and the guid is
//! reversed through the rainbow table (`pandemic_hash_m2(cue_name) == guid`). Unresolved guids
//! are emitted as `cue_<hash>` rather than dropped — the audio is still correct, only the label
//! is missing, and a later rainbow-table pass renames it.
//!
//! ```text
//! cargo run --release -p wad_simulator --bin sfx_extract -- \
//!     --wad game-files/vz.wad --filter wpn_ --out output/sfx_wav
//! cargo run --release -p wad_simulator --bin sfx_extract -- --wad game-files/vz.wad --list
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

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
    /// Build the cue catalog from every `sounddb` in the WAD rather than only the ones sharing a
    /// block with the bank. The engine merges all resident sounddbs into one catalog, and most of
    /// a weapon's cues do NOT live in that weapon's own block — so this names far more waves.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    global_cues: bool,
    /// Extra rainbow-table fragment(s) to load — e.g. the `--emit` output of `sfx_namecrack`.
    #[arg(long)]
    names: Vec<PathBuf>,
}

/// Harvest `(bank_hash, wave_index) -> cue_guid` from every `sounddb` the ASET table points at.
/// Only ~69 blocks carry one, so this is a cheap targeted pass, not a whole-WAD sweep.
fn global_cue_map(
    f: &mut File,
    arch: &mercs2_formats::ffcs::FfcsArchive,
) -> HashMap<(u32, u32), u32> {
    let mut blocks: Vec<u16> = arch
        .aset
        .iter()
        .filter(|e| e.type_id == ASET_TYPE_SOUNDDB)
        .map(|e| e.block_index())
        .collect();
    blocks.sort_unstable();
    blocks.dedup();

    let mut map: HashMap<(u32, u32), u32> = HashMap::new();
    for blk in blocks {
        let Ok(dec) = decompress_block(f, &arch.indx, blk) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "cues");
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_SOUNDDB {
                continue;
            }
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
                continue;
            };
            let Ok(db) = SoundDb::parse(&body) else { continue };
            for c in &db.cues {
                map.entry((c.bank_hash, c.wave_index)).or_insert(c.guid);
            }
        }
    }
    map
}

/// `soundbank` (`0x9F8BCA10`) — the SOUND-GROUP layer, and the reason a `sounddb`-only reading
/// leaves most waves anonymous. `wpn_pistol` has ten waves but exactly one `sounddb` cue, yet all
/// ten audibly play: the cue owns tracks, a track owns a group, and a group lists N waves with
/// selection weights (the engine's own `PgSoundDb` dump prints `Sound Groups (%d)` /
/// `Track %d - ... Sounds: %d`).
///
/// Layout, read off `wpn_pistol` and then confirmed against every shipped weapon bank:
/// ```text
/// +0x04 self_hash  +0x08 group_count(u16)  +0x0A cue_count(u16)
/// +0x10 data_start  +0x14 sec_b  +0x18 sec_c  +0x1C sec_end
///
/// section A [data_start..sec_b)  groups, VARIABLE length — located via section B
///     +0x00  u32  first wave's clip hash
///     +0x2D  u8   wave count for this group   (inside the known u8x4 field at +0x2C)
///     last count*12 bytes: { u32 bank_hash, u32 wave_index, f32 weight }
/// section B [sec_b..sec_c)       group_count u32 offsets, relative to data_start
/// section C [sec_c..sec_end)     cue_count records, stride = size/cue_count
///     +0x00  u32  cue guid; the record then carries { bank_hash, group_index } track refs
/// ```
struct SbGroup {
    waves: Vec<(u32, u32)>, // (bank_hash, wave_index)
}

struct SbRouting {
    groups: Vec<SbGroup>,
    /// (cue guid, the group indices its tracks reference)
    cues: Vec<(u32, Vec<usize>)>,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn parse_soundbank(body: &[u8]) -> Option<SbRouting> {
    if body.len() < 32 {
        return None;
    }
    let self_hash = rd32(body, 4);
    let group_count = u16::from_le_bytes([body[8], body[9]]) as usize;
    let cue_count = u16::from_le_bytes([body[10], body[11]]) as usize;
    let data_start = rd32(body, 16) as usize;
    let sec_b = rd32(body, 20) as usize;
    let sec_c = rd32(body, 24) as usize;
    let sec_end = rd32(body, 28) as usize;
    if !(data_start <= sec_b && sec_b <= sec_c && sec_c <= sec_end) || sec_end > body.len() {
        return None;
    }

    // Section B is the offset table that makes the variable-length section-A records walkable.
    let mut offs: Vec<usize> = Vec::with_capacity(group_count);
    for i in 0..group_count {
        let o = sec_b + i * 4;
        if o + 4 > sec_c {
            break;
        }
        offs.push(data_start + rd32(body, o) as usize);
    }

    let mut groups = Vec::with_capacity(offs.len());
    for (i, &start) in offs.iter().enumerate() {
        let end = offs.get(i + 1).copied().unwrap_or(sec_b);
        // Group records come in two shapes — a 104+12n "multi-take" record and a 64-byte
        // single-wave one — so a fixed offset or a count byte reads garbage on one of them.
        // Scanning the record for `{self_hash, wave_index, weight}` triples handles both, and
        // was checked to cover every wave index exactly once on the shipped weapon banks.
        let mut waves = Vec::new();
        let mut o = start;
        while o + 12 <= end.min(body.len()) {
            if rd32(body, o) == self_hash {
                waves.push((self_hash, rd32(body, o + 4)));
                o += 12;
                continue;
            }
            o += 4;
        }
        groups.push(SbGroup { waves });
    }

    // Section C: one record per cue; its track refs name the groups the cue plays. The trailing
    // section is section C's offset table, exactly as section B is section A's — these records
    // are variable-length too, so a computed stride is wrong (a 4-cue bank divides to 211, not
    // even 4-byte aligned, and misreads three of the four cue guids as zero).
    let mut cues = Vec::new();
    if cue_count > 0 && sec_end > sec_c {
        let coffs: Vec<usize> = (0..cue_count)
            .map(|c| sec_c + rd32(body, sec_end + c * 4) as usize)
            .collect();
        for c in 0..cue_count {
            let rec = coffs[c];
            let rec_end = coffs.get(c + 1).copied().unwrap_or(sec_end);
            if rec + 4 > body.len() {
                break;
            }
            let guid = rd32(body, rec);
            let mut gids = Vec::new();
            let mut o = rec + 4;
            while o + 8 <= rec_end.min(body.len()) {
                if rd32(body, o) == self_hash {
                    let idx = rd32(body, o + 4) as usize;
                    if idx < groups.len() && !gids.contains(&idx) {
                        gids.push(idx);
                    }
                }
                o += 4;
            }
            cues.push((guid, gids));
        }
    }
    Some(SbRouting { groups, cues })
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
        let m = global_cue_map(&mut f, &arch);
        eprintln!("global cue catalog: {} (bank, wave) routes", m.len());
        m
    } else {
        HashMap::new()
    };

    let (mut banks, mut written, mut named, mut skipped_empty) = (0usize, 0usize, 0usize, 0usize);
    let mut total_secs = 0.0f64;

    for (blk, short) in &blocks {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, *blk) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "sfx");

        // Pass 1 — every sounddb in this block, so waves can be named by the cue that plays them.
        // Keyed on (bank_hash, wave_index): a block's sounddb may route into more than one bank.
        // Seeded with the WAD-wide catalog; the block's own sounddb then takes precedence.
        let mut cue_of: HashMap<(u32, u32), u32> = global.clone();
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_SOUNDDB {
                continue;
            }
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else { continue };
            let Ok(db) = SoundDb::parse(&body) else { continue };
            for c in &db.cues {
                // Overwrites the global seed: a bank's own sounddb is the more specific route.
                cue_of.insert((c.bank_hash, c.wave_index), c.guid);
            }
        }

        // Pass 1b — the soundbank's group table. This is what accounts for the waves no sounddb
        // cue points at: they are the other takes/layers under a cue that `sounddb` names once.
        // Also records which group and slot a wave sits in, so siblings stay distinguishable.
        let mut group_of: HashMap<(u32, u32), (usize, usize)> = HashMap::new();
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_SOUNDBANK {
                continue;
            }
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
                continue;
            };
            let Some(mut sb) = parse_soundbank(&body) else { continue };
            // Groups are claimed by more than one cue (a `_fire` cue sweeps nearly every group,
            // while `_dryfire`/`_reload` name one each). Apply the narrowest cue first so the
            // specific name wins the wave rather than the catch-all.
            sb.cues.sort_by_key(|(_, gids)| gids.len());
            for (guid, gids) in &sb.cues {
                for &g in gids {
                    let Some(group) = sb.groups.get(g) else { continue };
                    for (slot, &(bank, idx)) in group.waves.iter().enumerate() {
                        cue_of.entry((bank, idx)).or_insert(*guid);
                        group_of.entry((bank, idx)).or_insert((g, slot));
                    }
                }
            }
            // A group no cue reached still names its waves as a group — better than nothing.
            for (g, group) in sb.groups.iter().enumerate() {
                for (slot, &(bank, idx)) in group.waves.iter().enumerate() {
                    group_of.entry((bank, idx)).or_insert((g, slot));
                }
            }
        }

        // Pass 2 — decode each wavebank's embedded clips.
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_WAVEBANK {
                continue;
            }
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else { continue };
            let bank = Wavebank::parse(&body);
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
                let label = cue_of
                    .get(&(bank.self_hash, idx as u32))
                    .and_then(|g| {
                        rb.resolve(*g).map(|s| s.to_string()).or(Some(format!("cue_{g:08X}")))
                    })
                    .or_else(|| rb.resolve(clip.clip_hash).map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("wave_{:08X}", clip.clip_hash));
                if !label.starts_with("cue_") && !label.starts_with("wave_") {
                    named += 1;
                }
                // Several waves share one cue name (layers + random takes), so qualify with the
                // group/slot the soundbank put this wave in — otherwise they collide on disk.
                let label = match group_of.get(&(bank.self_hash, idx as u32)) {
                    Some((g, slot)) => format!("{label}_g{g}_{slot}"),
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
