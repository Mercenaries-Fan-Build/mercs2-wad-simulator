//! Decide the wavebank clip-record layout against EVERY shipped bank, not just the VO ones.
//!
//! `mercs2_audio::wave` reads `data_offset` @+12 and `data_size` @+16. Extracting VO showed that
//! is wrong there (`+12 == 2*+16` on every clip; sorting by `+32` tiles the body with no overlap).
//! But the engine's `audio_wad_probe` test decodes 226 SFX cues from vz.wad through the IMA path
//! and gets RMS > 0, so before changing the engine we must know whether the corrected layout holds
//! for the SFX banks too — or only for VO. Flipping it globally on a VO-only proof could break
//! working sound.
//!
//! Two models, scored on hard structural constraints:
//!   A (current wave.rs): offset=+12, size=+16
//!   B (from VO):         offset=+32, size=+12   (+16 = sample count)
//!
//! A correct layout must: keep every blob inside the body, produce NO overlaps when sorted by
//! offset, and account for most of the body. A wrong one cannot fake all three.
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin wavebank_layout_probe -- \
//!       --wad game-files/vz.wad --wad game-files/English.wad

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

const TYPE_WAVEBANK: u32 = 6;
const TH_WAVEBANK: u32 = 0xF753_F6D0;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    wad: Vec<PathBuf>,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[derive(Default)]
struct Score {
    banks: usize,
    clips: usize,
    in_body: usize,
    overlaps: usize,
    /// clips where size == 2 * samples-field (the PCM16 bytes<->samples relation)
    ratio_2: usize,
    coverage: f64, // sum(size) / body payload, averaged over banks
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    for wad_path in &cli.wad {
        let mut file = File::open(wad_path)?;
        let size = file.metadata()?.len();
        let arch = load_ffcs_archive(&mut file, size)?;

        let mut want: HashMap<u16, Vec<u32>> = HashMap::new();
        for e in &arch.aset {
            if e.type_id == TYPE_WAVEBANK {
                want.entry(e.block_index()).or_default().push(e.asset_hash);
            }
        }

        let (mut a, mut b) = (Score::default(), Score::default());
        let (mut cov_a, mut cov_b) = (Vec::new(), Vec::new());
        let mut codec_hist: HashMap<u8, usize> = HashMap::new();
        // [sorted-by-+12, sorted-by-+32] x [delta==+12, delta==+16, total]
        let mut delta_stats = [[0usize; 3]; 2];

        for (blk, hashes) in &want {
            let Ok(dec) = decompress_block(&mut file, &arch.indx, *blk) else { continue };
            let (parsed, _) = walk_decompressed_block(&dec, "wb");
            for (i, ent) in parsed.entries.iter().enumerate() {
                if ent.type_hash != TH_WAVEBANK || !hashes.contains(&ent.name_hash) {
                    continue;
                }
                let Some(container) = parsed.containers.get(i) else { continue };
                let Some(body) = extract_data_chunk(container) else { continue };
                if body.len() < 24 {
                    continue;
                }
                let capacity = rd32(&body, 0) as usize;
                let populated = u16::from_le_bytes([body[8], body[9]]) as usize;
                let roff = rd32(&body, 16) as usize;
                let n = if populated > 0 && populated <= capacity { populated } else { capacity };
                if n == 0 || n > 5000 || roff >= body.len() {
                    continue;
                }

                a.banks += 1;
                b.banks += 1;
                let mut rows_a: Vec<(u64, u64)> = Vec::new();
                let mut rows_b: Vec<(u64, u64)> = Vec::new();

                for k in 0..n {
                    let r = roff + k * 36;
                    if r + 36 > body.len() {
                        break;
                    }
                    let f12 = rd32(&body, r + 12) as u64;
                    let f16 = rd32(&body, r + 16) as u64;
                    let f32_ = rd32(&body, r + 32) as u64;
                    let codec = body[r + 6];
                    if f12 == 0 && f16 == 0 {
                        continue;
                    }
                    *codec_hist.entry(codec).or_default() += 1;

                    // Streaming clips (fmt 0x04) address the external .pws, not this body — they
                    // would pollute a tiling test that is only meaningful for embedded audio.
                    if codec == 0x04 {
                        continue;
                    }

                    a.clips += 1;
                    b.clips += 1;
                    if f12 == 2 * f16 {
                        b.ratio_2 += 1;
                    }
                    // A: offset=f12, size=f16 | B: offset=f32, size=f12
                    if f12 + f16 <= body.len() as u64 {
                        a.in_body += 1;
                    }
                    if f32_ + f12 <= body.len() as u64 {
                        b.in_body += 1;
                    }
                    rows_a.push((f12, f16));
                    rows_b.push((f32_, f12));
                }

                for (rows, s, cov) in [
                    (&mut rows_a, &mut a, &mut cov_a),
                    (&mut rows_b, &mut b, &mut cov_b),
                ] {
                    rows.sort();
                    let mut prev_end = 0u64;
                    let mut sum = 0u64;
                    for (o, sz) in rows.iter() {
                        if *o < prev_end {
                            s.overlaps += 1;
                        }
                        prev_end = o + sz;
                        sum += sz;
                    }
                    let payload = body.len().saturating_sub(roff + n * 36) as f64;
                    if payload > 0.0 {
                        cov.push((sum as f64 / payload).min(9.9));
                    }
                }

                // ★ The decisive test. If clips are PACKED, then sorted by the TRUE offset each
                // consecutive delta equals the TRUE size. Score both candidate size fields against
                // the deltas of each candidate offset field — only the real pair can agree.
                for (off_idx, slot) in [(0usize, 0usize), (1usize, 1usize)] {
                    let mut v: Vec<(u64, u64, u64)> = Vec::new(); // (offset, f12, f16)
                    for k in 0..n {
                        let r = roff + k * 36;
                        if r + 36 > body.len() || body[r + 6] == 0x04 {
                            continue;
                        }
                        let f12 = rd32(&body, r + 12) as u64;
                        let f16 = rd32(&body, r + 16) as u64;
                        let f32_ = rd32(&body, r + 32) as u64;
                        if f12 == 0 && f16 == 0 {
                            continue;
                        }
                        v.push((if off_idx == 0 { f12 } else { f32_ }, f12, f16));
                    }
                    v.sort();
                    for w in v.windows(2) {
                        let delta = w[1].0.saturating_sub(w[0].0);
                        let (_, f12, f16) = w[0];
                        delta_stats[slot][2] += 1;
                        if delta > 0 && delta.abs_diff(f12) <= 64 {
                            delta_stats[slot][0] += 1;
                        }
                        if delta > 0 && delta.abs_diff(f16) <= 64 {
                            delta_stats[slot][1] += 1;
                        }
                    }
                }
            }
        }
        a.coverage = cov_a.iter().sum::<f64>() / cov_a.len().max(1) as f64;
        b.coverage = cov_b.iter().sum::<f64>() / cov_b.len().max(1) as f64;

        let mut ch: Vec<(u8, usize)> = codec_hist.into_iter().collect();
        ch.sort_by_key(|(_, n)| std::cmp::Reverse(*n));

        println!("\n=== {} — {} banks, {} clips", wad_path.display(), a.banks, a.clips);
        println!(
            "  fmt byte @+6 histogram: {:?}",
            ch.iter().map(|(c, n)| format!("0x{c:02X}:{n}")).collect::<Vec<_>>()
        );
        println!(
            "  A (wave.rs: off=+12 size=+16): in_body {}/{}, overlaps {}, coverage {:.2}x",
            a.in_body, a.clips, a.overlaps, a.coverage
        );
        println!(
            "  B (corrected: off=+32 size=+12): in_body {}/{}, overlaps {}, coverage {:.2}x, \
             size==2*samples on {}/{}",
            b.in_body, b.clips, b.overlaps, b.coverage, b.ratio_2, b.clips
        );
        println!("  PACKED-DELTA TEST (sorted by candidate offset, does delta == candidate size?)");
        for (i, tag) in ["sorted by +12", "sorted by +32"].iter().enumerate() {
            let [d12, d16, tot] = delta_stats[i];
            println!(
                "    {tag:<14}: delta==+12 {d12}/{tot} ({:.0}%), delta==+16 {d16}/{tot} ({:.0}%)",
                100.0 * d12 as f64 / tot.max(1) as f64,
                100.0 * d16 as f64 / tot.max(1) as f64
            );
        }
    }
    Ok(())
}
