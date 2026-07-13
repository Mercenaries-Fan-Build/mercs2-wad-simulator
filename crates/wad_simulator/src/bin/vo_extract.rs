//! Extract spoken VO from the game to named `.wav` files.
//!
//! ## How VO is stored
//!
//! Per `mrxsoundbootstrap.lua`, `vo_stream` is the only VO **wavebank**; the per-character
//! banks (`vo_mattias`, `vo_Chris`, `vo_Jen`, `vo_Fiona`, …) are **soundbanks** that route a
//! cue to `(bank_hash, wave_index)`. So all spoken audio lives in ONE wavebank whose clips
//! are codec `0x04` (streamed): the samples are NOT in the WAD, only a `(data_offset,
//! data_size)` pair pointing into `data/Audios/vo_stream.<lang>.pws`.
//!
//! A `.pws` is a HEADERLESS blob store (see `wad_simulator::pws`) — it carries no index and
//! no per-blob header, so it cannot be parsed standalone. The wavebank record is the index.
//! That is why extraction needs both halves:
//!
//!   WAD: wavebank record -> (clip_hash, channels, sample_rate, data_offset, data_size)
//!   PWS: bytes[data_offset .. data_offset+data_size]  -> decode -> WAV
//!
//! Names come from the `sounddb` cue tables: a cue is `{guid, bank_hash, wave_index}`, so a
//! cue landing on our bank names the wave at `wave_index`. The guid is
//! `pandemic_hash_m2(cue_name)`, reversed through the rainbow table + the fragments cracked
//! from the WAD, which is what turns `clip_0413.wav` into a named line.
//!
//! `--list` is the recon mode: it dumps the clip records and hexdumps the head of a blob so
//! the payload encoding can be confirmed before committing to a decode.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

#[path = "../names.rs"]
mod names;
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

/// UCFX container type hashes. These matter: ONE asset hash commonly carries three ASET rows
/// (wavebank + soundbank + sounddb share a name), so the block holds three different containers
/// under the same name_hash. Selecting by name alone hands you the soundbank and you parse
/// float routing data as audio records (channels=219, rate=0x3F800000 = float 1.0). Match the
/// container's type_hash.
const TH_WAVEBANK: u32 = 0xF753_F6D0; // pandemic_hash_m2("wavebank")
const TH_SOUNDDB: u32 = 0xE527_3C14; // pandemic_hash_m2("sounddb")

fn type_hash_for(aset_type: u32) -> u32 {
    match aset_type {
        TYPE_SOUNDDB => TH_SOUNDDB,
        _ => TH_WAVEBANK,
    }
}

/// A wavebank clip record, kept RAW.
///
/// `mercs2_audio::wave::Wavebank` decodes clips and then discards `(data_offset, data_size,
/// codec)` — which is exactly what a streamed clip has instead of samples, so it is exactly
/// what we need. Parse the record layout ourselves (it is stable and documented in wave.rs)
/// and reuse that crate's decoders for the payload.
/// ## Record layout — corrected against the shipped data
///
/// `mercs2_audio::wave.rs` reads `data_offset` @+12 and `data_size` @+16. That is WRONG for the
/// VO banks, and provably so:
///
///   * `+12 == 2 * +16` on every clip in every bank.
///   * Sorting the clips by the field at **+32** yields ZERO overlaps and the blobs tile the
///     body exactly (`sum(+12)` == body length to within the header+record bytes). Sorting by
///     +12 does not.
///
/// So `+32` is the data offset, `+12` is the byte size, and `+16` is the sample count. Since
/// `samples == bytes / 2`, each sample is TWO bytes — i.e. 16-bit PCM, not 4-bit IMA (IMA packs
/// 2 samples *per* byte, which would make samples == 2 * bytes). Re-reading the format dword
/// `[00 01 02 00]` as `{_, channels, bytes_per_sample, _}` agrees: the "codec = 2" that wave.rs
/// sees is a sample WIDTH of 2 bytes.
#[derive(Clone, Copy)]
struct ClipRec {
    clip_hash: u32,
    channels: u8,
    /// Bytes per sample (2 = PCM16). Named for what it is, not what wave.rs calls it.
    bytes_per_sample: u8,
    sample_rate: u32,
    data_offset: u32,
    data_size: u32,
    sample_count: u32,
}

const WB_HEADER: usize = 24;
const WB_RECORD: usize = 36;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn parse_records(body: &[u8]) -> Vec<ClipRec> {
    let mut out = Vec::new();
    if body.len() < WB_HEADER {
        return out;
    }
    let capacity = rd_u32(body, 0) as usize;
    let populated = u16::from_le_bytes([body[8], body[9]]) as usize;
    let records_off = rd_u32(body, 16) as usize;
    // `count` @0 is CAPACITY; `populated` @8 is how many records really exist (retail streaming
    // banks ship fewer than capacity). Take the smaller so we never run off the body.
    let n = if populated > 0 && populated <= capacity { populated } else { capacity };

    for i in 0..n {
        let roff = records_off + i * WB_RECORD;
        if roff + WB_RECORD > body.len() {
            break;
        }
        let rec = ClipRec {
            clip_hash: rd_u32(body, roff),
            channels: { let c = body[roff + 5]; if c == 0 { 1 } else { c } },
            bytes_per_sample: { let w = body[roff + 6]; if w == 0 { 2 } else { w } },
            sample_rate: rd_u32(body, roff + 8),
            data_size: rd_u32(body, roff + 12),
            sample_count: rd_u32(body, roff + 16),
            data_offset: rd_u32(body, roff + 32),
        };
        if rec.clip_hash == 0 && rec.sample_rate == 0 && rec.data_size == 0 {
            continue; // padding slot
        }
        out.push(rec);
    }
    out
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

    // ── 1. cue tables: guid -> (bank_hash, wave_index) ──────────────
    // Collected from every sounddb in every open wad; the VO soundbanks route into the
    // vo_stream wavebank, so these are what NAME the waves.
    let mut cues: Vec<(u32, u32, u32)> = Vec::new();
    for src in &sources {
        for (_, _, body) in load_bodies(src, TYPE_SOUNDDB)? {
            if let Ok(db) = SoundDb::parse(&body) {
                for c in &db.cues {
                    cues.push((c.guid, c.bank_hash, c.wave_index));
                }
            }
        }
    }
    eprintln!("cues: {}", cues.len());

    // ── 2. the VO wavebank(s) ───────────────────────────────────────
    let vo_hashes: HashMap<u32, &str> =
        VO_BANKS.iter().map(|n| (pandemic_hash_m2(n), *n)).collect();

    let mut found: Vec<(String, u32, u32, Vec<u8>, Vec<ClipRec>)> = Vec::new();
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
            let recs = parse_records(&body);
            let self_hash = rd_u32(&body, 4);
            found.push((label, hash, self_hash, body, recs));
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
    // A .pws has no index of its own, so only a wavebank record can address it. Report every
    // clip whose (offset,size) lands OUTSIDE its bank body — those are the stream references.
    if cli.streams {
        let mut n = 0usize;
        let mut max_end = 0u64;
        let mut bytes = 0u64;
        for (label, _, _, body, clips) in &found {
            for (i, c) in clips.iter().enumerate() {
                let end = c.data_offset as usize + c.data_size as usize;
                if end <= body.len() || c.data_size == 0 {
                    continue;
                }
                n += 1;
                bytes += c.data_size as u64;
                max_end = max_end.max(end as u64);
                if n <= 20 {
                    println!(
                        "  {label} [{i}] 0x{:08X} off={} size={} rate={} ch={}",
                        c.clip_hash, c.data_offset, c.data_size, c.sample_rate, c.channels
                    );
                }
            }
        }
        println!(
            "\n{n} stream-referencing clips, {:.1} MB addressed, furthest byte {} ({:.1} MB)",
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

    for (label, bank_hash, self_hash, body, clips) in &found {
        // wave_index -> cue name. A cue is {guid, bank_hash, wave_index}; guid is
        // pandemic_hash_m2(cue_name), so reversing it names the line. Cues reference the bank by
        // its SELF hash (body @+4), which is not always the ASET name hash — accept either.
        let mut name_of_index: HashMap<u32, String> = HashMap::new();
        for (guid, bh, wi) in &cues {
            if bh == bank_hash || bh == self_hash {
                if let Some(n) = rb.resolve(*guid) {
                    name_of_index.entry(*wi).or_insert_with(|| n.to_string());
                }
            }
        }

        if cli.list {
            println!(
                "\n=== {label} (0x{bank_hash:08X} self=0x{self_hash:08X}) — {} clips, body {} B, {} cue-named",
                clips.len(), body.len(), name_of_index.len()
            );
            // Layout proof. wave.rs reads offset@+12/size@+16, but +12 == 2*+16 on EVERY clip and
            // IMA yields exactly 2 samples/byte -- so +12 is a SAMPLE COUNT and +16 is the byte
            // size. The unread field at +32 is the real data offset. Verify by sorting on +32 and
            // checking the blobs tile the body without overlap.
            let records_off = rd_u32(body, 16) as usize;
            let capacity = rd_u32(body, 0);
            let mut rows: Vec<(u32, u32, u32)> = Vec::new(); // (off@32, size@16, samples@12)
            for i in 0..clips.len() {
                let roff = records_off + i * WB_RECORD;
                if roff + WB_RECORD > body.len() {
                    break;
                }
                rows.push((
                    rd_u32(body, roff + 32),
                    rd_u32(body, roff + 16),
                    rd_u32(body, roff + 12),
                ));
            }
            rows.sort_by_key(|r| r.0);
            let mut overlaps = 0usize;
            let mut covered = 0u64;
            let mut prev_end = 0u64;
            for (off, size, _) in &rows {
                let (o, s) = (*off as u64, *size as u64);
                if o < prev_end {
                    overlaps += 1;
                }
                covered += s;
                prev_end = o + s;
            }
            println!(
                "  capacity={capacity} records_off={records_off} body={} B",
                body.len()
            );
            println!(
                "  sorted by +32 with size=+16: overlaps={overlaps}, covered={covered} B, \
                 last_end={prev_end} (body {})",
                body.len()
            );
            for (off, size, samples) in rows.iter().take(4) {
                println!("    off={off:>9} size={size:>8} samples={samples:>9} (2*size={})", size * 2);
            }
            continue;
        }

        for (i, clip) in clips.iter().enumerate() {
            if cli.limit > 0 && written >= cli.limit {
                break;
            }
            if clip.data_size == 0 {
                continue;
            }
            let end = clip.data_offset as usize + clip.data_size as usize;

            // Embedded (codec 0x02 IMA / 0x00 PCM): the samples are in the bank body, inside the
            // WAD. Streamed (codec 0x04): only a reference — the bytes live in the .pws.
            let blob: Vec<u8> = if end <= body.len() {
                embedded_n += 1;
                body[clip.data_offset as usize..end].to_vec()
            } else if (clip.data_offset as u64 + clip.data_size as u64) <= pws_len {
                streamed_n += 1;
                let mut b = vec![0u8; clip.data_size as usize];
                pws.seek(SeekFrom::Start(clip.data_offset as u64))?;
                pws.read_exact(&mut b)?;
                b
            } else {
                continue; // reference fits neither the body nor the .pws
            };

            let ch = clip.channels.max(1) as u16;
            // 2 bytes/sample = interleaved PCM16 LE (the VO case — see ClipRec docs).
            // 1 byte/sample = 4-bit IMA ADPCM, decoded with the engine's tested decoder.
            let pcm: Vec<i16> = match clip.bytes_per_sample {
                2 => blob
                    .chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect(),
                _ if ch >= 2 => wave::decode_ima_stereo(&blob),
                _ => wave::decode_ima_mono(&blob),
            };
            if pcm.is_empty() {
                continue;
            }
            // The record states the expected sample count; a mismatch means the layout is wrong.
            if clip.bytes_per_sample == 2 && clip.sample_count > 0 {
                let got = pcm.len() as u32;
                if got.abs_diff(clip.sample_count) > 2 {
                    eprintln!(
                        "  WARN {label} clip {i}: decoded {got} samples, record says {} — layout suspect",
                        clip.sample_count
                    );
                }
            }

            let rate = if clip.sample_rate == 0 { 44100 } else { clip.sample_rate };
            let secs = pcm.len() as f64 / ch as f64 / rate as f64;
            total_secs += secs;

            // Name the line if a cue claims this wave; otherwise fall back to the scene block
            // name + index, which still tells you which mission the line belongs to.
            // Name the line, best source first:
            //   1. the clip's OWN hash — the engine's documented fallback is `clip_hash ==
            //      cue guid`, and a cue guid is pandemic_hash_m2(cue_name), so this reverses
            //      straight to the line's authored name when the rainbow table has it.
            //   2. a cue that claims this wave_index in this bank.
            //   3. scene block + index, which still identifies the mission.
            let base = if let Some(n) = rb.resolve(clip.clip_hash) {
                named += 1;
                format!("{}__{}", safe(label), safe(n))
            } else if let Some(n) = name_of_index.get(&(i as u32)) {
                named += 1;
                format!("{}__{}", safe(label), safe(n))
            } else {
                format!("{}__{:04}_0x{:08X}", safe(label), i, clip.clip_hash)
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
