//! Extract spoken VO from the game to named `.wav` files.
//!
//! ## How VO is stored
//!
//! Per `mrxsoundbootstrap.lua`, `vo_stream` is the streamed VO **wavebank**; the per-character
//! banks (`vo_mattias`, `vo_Chris`, `vo_Jen`, `vo_Fiona`, …) are **soundbanks** whose cues play
//! groups of its waves. Its records are format `0x04` (streamed): the samples are NOT in the WAD,
//! only a `(data_offset, data_size)` pair pointing into `data/Audios/vo_stream.<lang>.pws`. The
//! per-scene VO blocks in `English.wad` also carry small embedded wavebanks.
//!
//! A `.pws` is a HEADERLESS blob store (see `wad_simulator::pws`) — it carries no index and
//! no per-blob header, so it cannot be parsed standalone. The wavebank record is the index.
//! That is why extraction needs both halves:
//!
//!   WAD: wavebank record -> (clip_hash, channels, sample_rate, data_offset, data_size)
//!   PWS: bytes[data_offset .. data_offset+data_size]  -> decode -> WAV
//!
//! Names come from the cues that play each wave, found the way the engine finds them: sounddb
//! entry `{guid, soundbank, cue index}` → soundbank cue → (every track's sounds →) group → wave
//! (`mercs2_audio::route`). The guid is `pandemic_hash_m2(cue_name)`, reversed through the rainbow
//! table + the fragments cracked from the WAD, which is what turns `clip_0413.wav` into a named
//! line.
//!
//! `--list` is the recon mode: it dumps the clip records and hexdumps the head of a blob so
//! the payload encoding can be confirmed before committing to a decode.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::route::route;
use mercs2_audio::soundbank::Soundbank;
use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::{self, WaveData, WavebankFile};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

// From the crate lib now; this used to be `#[path = "../names.rs"] mod names;`, the workaround
// for a crate that had no [lib] and so compiled this module once per binary.
use wad_simulator::names;
use names::RainbowTable;

/// One asset pulled out of a WAD: its UCFX container's DATA body.
/// (Deliberately mercs2_formats-only — pulling in mercs2_engine would drag wgpu/winit
/// into a CLI that never opens a window.)
fn load_bodies(
    path: &str,
    want_type: u32,
) -> Result<Vec<(u32, String, Vec<u8>)>, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let arch = load_ffcs_archive(&mut file, size)?;

    // hash -> owning block, for the type we want
    let mut want: HashMap<u16, Vec<u32>> = HashMap::new();
    for e in &arch.aset {
        if e.type_id == want_type {
            want.entry(e.block_index()).or_default().push(e.asset_hash);
        }
    }

    let mut out = Vec::new();
    for (blk, hashes) in want {
        let Ok(data) = decompress_block(&mut file, &arch.indx, blk) else {
            continue;
        };
        let block_path = arch
            .paths
            .get(blk as usize)
            .cloned()
            .unwrap_or_default();
        let want_th = type_hash_for(want_type);
        let (parsed, _) = walk_decompressed_block(&data, "vo");
        for (i, ent) in parsed.entries.iter().enumerate() {
            if !hashes.contains(&ent.name_hash) || ent.type_hash != want_th {
                continue;
            }
            let Some(container) = parsed.containers.get(i) else {
                continue;
            };
            if let Some(body) = extract_data_chunk(container) {
                out.push((ent.name_hash, block_path.clone(), body));
            }
        }
    }
    Ok(out)
}

/// Is this asset SPEECH rather than a sound effect?
///
/// The game separates them for us: localized dialogue lives in English.wad, whose blocks are
/// all `blocks\English\vo_*` (per-mission: `vo_gurcon001.english`, `vo_resident`, …). SFX banks
/// (`wpn_shared`, `veh_shared`, `collision_shared`, `ambience`, `ui_hud`, …) live elsewhere.
/// So the owning BLOCK PATH is the discriminator — not the bank name, which is usually an
/// unresolved hash.
fn is_vo(block_path: &str, label: &str) -> bool {
    let p = block_path.to_lowercase().replace('/', "\\");
    p.contains("\\vo_") || label.to_lowercase().starts_with("vo_")
}

/// ASET type ids (docs/type_hash_registry.md).
const TYPE_WAVEBANK: u32 = 6;
const TYPE_SOUNDDB: u32 = 13;
const TYPE_SOUNDBANK: u32 = 21;

/// UCFX container type hashes. These matter: ONE asset hash commonly carries three ASET rows
/// (wavebank + soundbank + sounddb share a name), so the block holds three different containers
/// under the same name_hash. Selecting by name alone hands you the soundbank and you parse
/// float routing data as audio records (channels=219, rate=0x3F800000 = float 1.0). Match the
/// container's type_hash.
const TH_WAVEBANK: u32 = 0xF753_F6D0; // pandemic_hash_m2("wavebank")
const TH_SOUNDDB: u32 = 0xE527_3C14; // pandemic_hash_m2("sounddb")
const TH_SOUNDBANK: u32 = 0x9F8B_CA10; // pandemic_hash_m2("soundbank")

fn type_hash_for(aset_type: u32) -> u32 {
    match aset_type {
        TYPE_SOUNDDB => TH_SOUNDDB,
        TYPE_SOUNDBANK => TH_SOUNDBANK,
        _ => TH_WAVEBANK,
    }
}

/// The VO banks, from `mrxsoundbootstrap.lua`. `vo_stream` is the wavebank (the audio);
/// the rest are soundbanks (the routing) and are mined for cue names.
const VO_BANKS: &[&str] = &[
    "vo_stream", "vo_mattias", "vo_Chris", "vo_carmona", "vo_Jen", "vo_Fiona", "vo_Ewan",
    "vo_Misha", "vo_Misc", "vo_alliedSoldier_01", "vo_alliedSoldier_02",
    "vo_alliedSoldier_black_03", "vo_chinSoldier_01", "vo_chinSoldier_02", "vo_oc_merc_01",
    "vo_oc_merc_02", "vo_vzCiv_01", "vo_vzCiv_02", "vo_vzCiv_female_01", "vo_vzCiv_female_02",
    "vo_vzGurSoldier_01", "vo_vzGurSoldier_02", "vo_vzGurSoldier_female_01", "vo_vzSoldier_01",
    "vo_vzSoldier_02", "vo_pirate_01", "vo_pirate_02", "vo_pirate_female_01",
];

#[derive(Parser)]
#[command(about = "Extract spoken VO to named .wav files")]
struct Cli {
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\vz.wad")]
    wad: String,

    /// English.wad also carries VO blocks (vo_*.english).
    #[arg(long)]
    extra_wad: Vec<String>,

    /// Directory holding the .pws streams.
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\Audios")]
    audios: PathBuf,

    #[arg(long, default_value = "vo_stream.english.pws")]
    pws: String,

    #[arg(long, default_value = "output/vo_wav")]
    out: PathBuf,

    /// Recon: print clip records + hexdump a blob head, write nothing.
    #[arg(long)]
    list: bool,

    /// Report every clip that is NOT embedded (i.e. references the external .pws), so we can
    /// see what actually indexes the stream files.
    #[arg(long)]
    streams: bool,

    /// Cap the number of clips extracted (0 = all).
    #[arg(long, default_value_t = 0)]
    limit: usize,
}

fn rainbow() -> RainbowTable {
    let tables: Vec<PathBuf> = [
        "tools/rainbow_table.json",
        "docs/data/aset_discovered_names.json",
        "docs/data/aset_block_strings.json",
        "docs/data/aset_expanded_names.json",
    ]
    .iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect();
    RainbowTable::load_many(&tables).unwrap_or_default()
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

fn rms(pcm: &[i16]) -> f64 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum: f64 = pcm.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / pcm.len() as f64).sqrt()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut sources = vec![cli.wad.clone()];
    sources.extend(cli.extra_wad.iter().cloned());

    let rb = rainbow();
    eprintln!("rainbow: {} names", rb.len());

    // ── 1. routing: (wavebank, wave index) -> the cues that play it ─
    // Every sounddb and soundbank in every open wad; the VO soundbanks' groups play the VO
    // wavebanks' waves, so these are what NAME the waves.
    let (mut dbs, mut sbs) = (Vec::new(), Vec::new());
    for src in &sources {
        for (_, _, body) in load_bodies(src, TYPE_SOUNDDB)? {
            dbs.push(SoundDb::parse(&body)?);
        }
        for (_, _, body) in load_bodies(src, TYPE_SOUNDBANK)? {
            sbs.push(Soundbank::parse(&body)?);
        }
    }
    let routing = route(&dbs, &sbs)?;
    eprintln!(
        "routing: {} sounddbs, {} soundbanks, {} waves in groups",
        dbs.len(),
        sbs.len(),
        routing.waves.len()
    );

    // ── 2. the VO wavebank(s) ───────────────────────────────────────
    let vo_hashes: HashMap<u32, &str> =
        VO_BANKS.iter().map(|n| (pandemic_hash_m2(n), *n)).collect();

    let mut found: Vec<(String, u32, WavebankFile)> = Vec::new();
    let mut skipped_sfx = 0usize;
    for src in &sources {
        for (hash, block_path, body) in load_bodies(src, TYPE_WAVEBANK)? {
            let label = vo_hashes
                .get(&hash)
                .map(|s| s.to_string())
                .or_else(|| rb.resolve(hash).map(|s| s.to_string()))
                .unwrap_or_else(|| {
                    // Fall back to the block's own name — `blocks\English\vo_gurcon001.english`
                    // tells us exactly which scene's dialogue this is.
                    block_path
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .trim_end_matches(".block")
                        .trim_end_matches("_P000_Q3")
                        .to_string()
                });
            // Speech only — the user asked for things SAID, not sound effects.
            if !is_vo(&block_path, &label) {
                skipped_sfx += 1;
                continue;
            }
            found.push((label, hash, WavebankFile::parse(&body)?));
        }
    }
    eprintln!("VO wavebanks: {} (skipped {skipped_sfx} SFX banks)", found.len());

    if found.is_empty() {
        eprintln!("no VO wavebank found — is vz.wad/English.wad correct?");
        return Ok(());
    }

    let mut pws = File::open(cli.audios.join(&cli.pws))?;
    let pws_len = pws.metadata()?.len();
    eprintln!("pws {} = {} bytes", cli.pws, pws_len);

    std::fs::create_dir_all(&cli.out)?;
    let mut written = 0usize;

    // ── what indexes the .pws? ──────────────────────────────────────
    // A .pws has no index of its own; a streamed bank's records are the index.
    if cli.streams {
        let mut n = 0usize;
        let mut max_end = 0u64;
        let mut bytes = 0u64;
        for (label, _, file) in &found {
            for (i, r) in file.records.iter().enumerate() {
                let WaveData::Streamed { offset, size, .. } = r.data else { continue };
                n += 1;
                bytes += size as u64;
                max_end = max_end.max(offset as u64 + size as u64);
                if n <= 20 {
                    println!(
                        "  {label} [{i}] 0x{:08X} off={offset} size={size} rate={} ch={}",
                        r.clip_hash, r.sample_rate, r.channels
                    );
                }
            }
        }
        println!(
            "\n{n} streamed records, {:.1} MB addressed, furthest byte {} ({:.1} MB)",
            bytes as f64 / 1e6,
            max_end,
            max_end as f64 / 1e6
        );
        println!("pws on disk: {} ({:.1} MB)", pws_len, pws_len as f64 / 1e6);
        return Ok(());
    }

    let mut embedded_n = 0usize;
    let mut streamed_n = 0usize;
    let mut named = 0usize;
    let mut total_secs = 0.0f64;

    for (label, bank_hash, file) in &found {
        if cli.list {
            let routed = (0..file.records.len() as u32)
                .filter(|&i| routing.first_cue(file.bank_hash, i).is_some())
                .count();
            println!(
                "\n=== {label} (0x{bank_hash:08X} table 0x{:08X}{}) — {} records, {routed} reached by a cue",
                file.bank_hash,
                file.stream_name.as_deref().map(|n| format!(", streams from {n}")).unwrap_or_default(),
                file.records.len(),
            );
            for (i, r) in file.records.iter().enumerate().take(8) {
                let cue = routing
                    .first_cue(file.bank_hash, i as u32)
                    .map(|g| rb.resolve(g).map(str::to_string).unwrap_or(format!("cue_{g:08X}")))
                    .unwrap_or_default();
                println!(
                    "  [{i:>5}] clip 0x{:08X} {} ch {} Hz {} frames  {cue}",
                    r.clip_hash, r.channels, r.sample_rate, r.frames
                );
            }
            continue;
        }

        for (i, rec) in file.records.iter().enumerate() {
            if cli.limit > 0 && written >= cli.limit {
                break;
            }
            let ch = rec.channels as u16;
            // Embedded: interleaved PCM16 in the bank body. Streamed: the bytes live in the .pws and
            // are IMA ADPCM there (mercs2_workshop::vostream decodes the same stream the same way).
            let pcm: Vec<i16> = match &rec.data {
                WaveData::Embedded(bytes) => {
                    embedded_n += 1;
                    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
                }
                WaveData::Streamed { offset, size, .. } => {
                    if *offset as u64 + *size as u64 > pws_len {
                        return Err(format!(
                            "{label} record {i}: {size} bytes at {offset} run past the {pws_len}-byte {}",
                            cli.pws
                        )
                        .into());
                    }
                    streamed_n += 1;
                    let mut b = vec![0u8; *size as usize];
                    pws.seek(SeekFrom::Start(*offset as u64))?;
                    pws.read_exact(&mut b)?;
                    if ch >= 2 { wave::decode_ima_stereo(&b) } else { wave::decode_ima_mono(&b) }
                }
            };
            if pcm.is_empty() {
                continue;
            }
            let rate = rec.sample_rate;
            let secs = pcm.len() as f64 / ch as f64 / rate as f64;
            total_secs += secs;

            // Name the line, best source first:
            //   1. the clip's OWN hash, reversed through the rainbow table;
            //   2. a cue that plays this wave (sounddb → soundbank cue → group → wave);
            //   3. scene block + index, which still identifies the mission.
            let cue_name = routing.first_cue(file.bank_hash, i as u32).and_then(|g| rb.resolve(g));
            let base = if let Some(n) = rb.resolve(rec.clip_hash) {
                named += 1;
                format!("{}__{}", safe(label), safe(n))
            } else if let Some(n) = cue_name {
                named += 1;
                format!("{}__{}", safe(label), safe(n))
            } else {
                format!("{}__{:04}_0x{:08X}", safe(label), i, rec.clip_hash)
            };
            let path = cli.out.join(format!("{base}.wav"));
            write_wav(&path, &pcm, ch, rate)?;
            written += 1;
            if written <= 6 {
                println!(
                    "  {} — {:.1}s, {} Hz, rms {:.0}",
                    path.file_name().unwrap().to_string_lossy(),
                    secs, rate, rms(&pcm)
                );
            }
        }
    }

    if !cli.list {
        println!(
            "\nwrote {written} wav files ({embedded_n} embedded, {streamed_n} streamed), \
             {named} with a resolved line name = {:.1} min of speech -> {}",
            total_secs / 60.0,
            cli.out.display()
        );
    }
    Ok(())
}
