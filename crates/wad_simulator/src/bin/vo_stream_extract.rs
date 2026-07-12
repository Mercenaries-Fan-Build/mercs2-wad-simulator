//! Extract the STREAMED voice-over — the ~12,988 waves in `vo_stream.<lang>.pws` that
//! `vo_extract` never saw, including every Mattias / Jennifer / Chris / Fiona line.
//!
//! ## The chain (all of it verified, none of it assumed)
//!
//! ```text
//!   sounddb cue {guid, bank_hash = vo_mattias, wave_index N}
//!     -> soundbank `vo_mattias.english`, section-A record N
//!          +52 = 0x421680B7  (= m2("vo_stream"))   <- the bank it routes to
//!          +56 = wave index into that bank
//!     -> wavebank `vo_stream.english`, record W  -> (data_offset, data_size)
//!     -> vo_stream.english.pws
//! ```
//!
//! Two things hid this:
//!
//! 1. **Localization.** VO assets get a `.<language>` suffix at RUNTIME (`_GetLocalizedName` keys
//!    off the `vo_` prefix). Cues store `m2("vo_mattias")` = 0x88882912, but the shipped asset is
//!    `vo_mattias.english` = 0x4416CD1C — so searching the WAD for the cue's hash finds nothing.
//!
//! 2. **A bad clamp.** The wavebank header's `+8` (`populated`) IS the record count; the word at
//!    `+0` is not a capacity. On `vo_stream.english`, `+0` reads 29 while `+8` reads **12,988**.
//!    Clamping to the smaller truncated the game's entire streamed VO to 29 clips. The header
//!    settles it arithmetically: records_offset(40) + 12,988 x 36 = 467,608 = the body length
//!    exactly. That bank's header even carries its own stream filename ("vo_stream.pws") at +24,
//!    which is why its records start at 40 instead of the usual 24.
//!
//! Corroboration: each character soundbank's `sub_count` equals the cue count for that character
//! exactly — Mattias 541, Jen 570, Chris 524, Fiona 2577.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use clap::Parser;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

#[path = "../names.rs"]
mod names;
use names::RainbowTable;

const TH_WAVEBANK: u32 = 0xF753_F6D0;
const TH_SOUNDBANK: u32 = 0x9F8B_CA10;
/// The section-A field that names the wavebank a soundbank record routes to.
const SB_BANK_FIELD: usize = 52;
/// …and the wave index within it.
const SB_WAVE_FIELD: usize = 56;

/// The VO banks, verbatim from `mrxsoundbootstrap.lua`. Each is language-suffixed at runtime, so
/// the shipped asset is `<name>.english` — the rainbow table does not hold those localized names,
/// so derive the hashes rather than looking them up.
const VO_BANKS: &[&str] = &[
    "vo_mattias", "vo_Jen", "vo_Chris", "vo_Fiona", "vo_carmona", "vo_Ewan", "vo_Misha", "vo_Misc",
    "vo_alliedSoldier_01", "vo_alliedSoldier_02", "vo_alliedSoldier_black_03",
    "vo_chinSoldier_01", "vo_chinSoldier_02", "vo_oc_merc_01", "vo_oc_merc_02",
    "vo_vzCiv_01", "vo_vzCiv_02", "vo_vzCiv_female_01", "vo_vzCiv_female_02",
    "vo_vzGurSoldier_01", "vo_vzGurSoldier_02", "vo_vzGurSoldier_female_01",
    "vo_vzSoldier_01", "vo_vzSoldier_02", "vo_pirate_01", "vo_pirate_02", "vo_pirate_female_01",
];

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\English.wad")]
    wad: PathBuf,
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\Audios\vo_stream.english.pws")]
    pws: PathBuf,
    #[arg(long, default_value = "output/vo_stream")]
    out: PathBuf,
    /// Report only; write nothing.
    #[arg(long)]
    dry_run: bool,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect()
}

struct Wave {
    clip_hash: u32,
    channels: u8,
    codec: u8,
    rate: u32,
    size: u32,
    offset: u32,
}

fn write_wav(path: &std::path::Path, pcm: &[i16], ch: u16, rate: u32) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    let dl = (pcm.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + dl).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&ch.to_le_bytes())?;
    f.write_all(&rate.to_le_bytes())?;
    f.write_all(&(rate * ch as u32 * 2).to_le_bytes())?;
    f.write_all(&(ch * 2).to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&dl.to_le_bytes())?;
    for s in pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let vo_stream_hash = pandemic_hash_m2("vo_stream");

    let rb = {
        let t: Vec<PathBuf> = [
            "tools/rainbow_table.json",
            "docs/data/aset_discovered_names.json",
            "docs/data/aset_block_strings.json",
            "docs/data/aset_expanded_names.json",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
        RainbowTable::load_many(&t).unwrap_or_default()
    };
    eprintln!("rainbow: {} names", rb.len());

    // ── collect every wavebank + soundbank body in the WAD ──────────
    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;

    let mut waves: Vec<Wave> = Vec::new();
    let mut soundbanks: Vec<(u32, Vec<u8>)> = Vec::new();

    for blk in 0..arch.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, blk as u16) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "vo");
        for (i, ent) in parsed.entries.iter().enumerate() {
            let Some(c) = parsed.containers.get(i) else { continue };
            let Some(body) = extract_data_chunk(c) else { continue };

            if ent.type_hash == TH_SOUNDBANK {
                soundbanks.push((ent.name_hash, body));
                continue;
            }
            if ent.type_hash != TH_WAVEBANK || ent.name_hash != pandemic_hash_m2("vo_stream.english")
            {
                continue;
            }
            // ★ trust `populated` @+8; do NOT clamp to the word at +0 (it reads 29 here).
            let populated = u16::from_le_bytes([body[8], body[9]]) as usize;
            let roff = rd32(&body, 16) as usize;
            let max_fit = body.len().saturating_sub(roff) / 36;
            let n = populated.min(max_fit);
            println!(
                "vo_stream.english: body {} B, populated {populated}, records_off {roff} -> {n} waves \
                 (roff + n*36 = {}, body = {})",
                body.len(),
                roff + n * 36,
                body.len()
            );
            for k in 0..n {
                let r = roff + k * 36;
                waves.push(Wave {
                    clip_hash: rd32(&body, r),
                    channels: { let c = body[r + 5]; if c == 0 { 1 } else { c } },
                    codec: body[r + 6],
                    rate: rd32(&body, r + 8),
                    size: rd32(&body, r + 12),
                    offset: rd32(&body, r + 32),
                });
            }
        }
    }
    if waves.is_empty() {
        eprintln!("vo_stream.english wavebank not found");
        return Ok(());
    }

    // ── which character owns each wave? ─────────────────────────────
    // Section-A stride is not (sec1-data_start)/sub_count for these banks, so find it from the
    // data: the stride is whatever spacing makes every record's +52 equal m2("vo_stream").
    let localized: HashMap<u32, &str> = VO_BANKS
        .iter()
        .map(|n| (pandemic_hash_m2(&format!("{n}.english")), *n))
        .collect();

    let mut owner: HashMap<u32, String> = HashMap::new(); // wave index -> bank name
    for (hash, body) in &soundbanks {
        if body.len() < 32 {
            continue;
        }
        let name = match localized.get(hash) {
            Some(n) => (*n).to_string(),
            None => continue,
        };
        let sub_count = u16::from_le_bytes([body[8], body[9]]) as usize;
        let data_start = rd32(body, 16) as usize;
        if sub_count == 0 {
            continue;
        }
        // Section-A stride is 64: the routing fields sit at +52 (bank) and +56 (wave index) with a
        // float at +60, so the record cannot be smaller, and 64 is what the bank-hash signature
        // detects on every VO bank.
        //
        // Do NOT "search for the stride that matches most records" — smaller strides ALIAS. A
        // stride of 32 samples each 64-byte record twice and so scores ~2x the hits, which is how
        // that search produced a nonsense 32 for Fiona and 56 for Jen.
        const SB_STRIDE: usize = 64;
        let stride = SB_STRIDE;
        let mut n_routed = 0usize;
        for r in 0..sub_count {
            let o = data_start + r * stride;
            if o + SB_WAVE_FIELD + 4 > body.len() {
                break;
            }
            if rd32(body, o + SB_BANK_FIELD) != vo_stream_hash {
                continue;
            }
            let w = rd32(body, o + SB_WAVE_FIELD);
            owner.entry(w).or_insert_with(|| name.clone());
            n_routed += 1;
        }
        println!("  {name:<28} sub_count {sub_count:>5}, stride {stride:>3} -> {n_routed} waves routed");
    }
    println!("\n{} waves in vo_stream; {} attributed to a character bank", waves.len(), owner.len());

    if cli.dry_run {
        return Ok(());
    }

    // ── carve ───────────────────────────────────────────────────────
    let mut pws = File::open(&cli.pws)?;
    let pws_len = pws.metadata()?.len();
    let (mut n, mut oob, mut named, mut mp3, mut pcm_n) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut secs = 0.0f64;

    for (i, w) in waves.iter().enumerate() {
        if w.size == 0 || w.offset as u64 + w.size as u64 > pws_len {
            oob += 1;
            continue;
        }
        let mut blob = vec![0u8; w.size as usize];
        pws.seek(SeekFrom::Start(w.offset as u64))?;
        pws.read_exact(&mut blob)?;

        let bank = owner.get(&(i as u32)).cloned().unwrap_or_else(|| "vo_misc".into());
        let dir = cli.out.join(safe(&bank));
        std::fs::create_dir_all(&dir)?;

        let line = rb.resolve(w.clip_hash);
        if line.is_some() {
            named += 1;
        }
        let base = match line {
            Some(l) => format!("{:05}__{}", i, safe(l)),
            None => format!("{:05}__0x{:08X}", i, w.clip_hash),
        };

        // The stream is MP3 (as on console), zero-padded ahead of the first frame. Skip the pad
        // and REQUIRE a frame sync there; anything else is treated as raw PCM16.
        let pad = blob.iter().position(|&b| b != 0).unwrap_or(blob.len());
        let is_mp3 = pad + 2 <= blob.len() && blob[pad] == 0xFF && (blob[pad + 1] & 0xE0) == 0xE0;
        let rate = if w.rate == 0 { 44100 } else { w.rate };

        if is_mp3 {
            File::create(dir.join(format!("{base}.mp3")))?.write_all(&blob[pad..])?;
            mp3 += 1;
            // ~90 kbps mono
            secs += (w.size - pad as u32) as f64 * 8.0 / 90_000.0;
        } else {
            let s: Vec<i16> = blob
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            write_wav(&dir.join(format!("{base}.wav")), &s, w.channels.max(1) as u16, rate)?;
            pcm_n += 1;
            secs += s.len() as f64 / w.channels.max(1) as f64 / rate as f64;
        }
        n += 1;
        let _ = w.codec;
    }

    println!(
        "\nwrote {n} waves ({mp3} mp3, {pcm_n} pcm-wav), {named} with a resolved line name, \
         {oob} out-of-range = {:.1} min -> {}",
        secs / 60.0,
        cli.out.display()
    );
    Ok(())
}
