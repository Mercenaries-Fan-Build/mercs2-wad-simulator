//! Parse a VO character soundbank and find the wave table into `vo_stream.<lang>.pws`.
//!
//! Established (see `vo_extract`, `cue_probe`, and the corpus):
//!   * A `sounddb` cue is `{guid, bank_hash, wave_index}`. The VO cues route to per-character
//!     banks: vo_Fiona (2577 waves), vo_Jen (570), vo_mattias (541), vo_Chris (524), plus ~500
//!     each for the soldier/pirate/civ banks — >10,000 waves in total.
//!   * Those banks are NOT wavebanks (English.wad has 43 wavebank containers / 1,142 clips and
//!     none of them match). They are SOUNDBANK containers living in the `vo_resident` block.
//!   * VO assets are language-suffixed at RUNTIME (`_GetLocalizedName` keys off the `vo_` prefix),
//!     so the cue stores `m2("vo_mattias")` = 0x88882912 but the shipped asset is
//!     `vo_mattias.english` = 0x4416CD1C. That mismatch is why a hash search for the bank failed.
//!   * The audio itself is the 798 MB `vo_stream.english.pws` (~2.5 h of PCM16 mono @44.1k) —
//!     which matches >10,000 waves at ~0.9 s each.
//!
//! Soundbank header (from `wad_simulator::audio::soundbank`, already reversed):
//!   +04 self_hash · +08 sub_count(u16) · +10 sub_count2(u16) · +16 data_start
//!   +20 section_off1 · +24 section_off2 · +28 section_off3   (32-byte header)
//! Section A = [data_start, section_off1), `sub_count` records, stride = size / sub_count.
//!
//! So: if Section A carries the stream table, each record should hold a (offset, size) pair that
//! lands inside the .pws. This probe finds the pair by testing every u32 field position in the
//! record against that constraint — no guessing.

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

const TH_SOUNDBANK: u32 = 0x9F8B_CA10;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\English.wad")]
    wad: PathBuf,
    /// The .pws the waves should live in.
    #[arg(long, default_value = r"C:\Users\Shadow\Desktop\Mercenaries 2 World in Flames\data\Audios\vo_stream.english.pws")]
    pws: PathBuf,
    /// Bank name hashes to inspect (default: the four main characters, localized).
    #[arg(long)]
    bank: Vec<String>,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let pws_len = std::fs::metadata(&cli.pws)?.len();
    eprintln!("pws {} = {} bytes", cli.pws.display(), pws_len);

    // localized character banks
    let wanted: HashMap<u32, &str> = if cli.bank.is_empty() {
        [
            (0x4416_CD1Cu32, "vo_mattias.english"),
            (0x1A0C_C278, "vo_Jen.english"),
            (0xCBA2_95EE, "vo_Chris.english"),
            (0xBB53_09D2, "vo_Fiona.english"),
        ]
        .into_iter()
        .collect()
    } else {
        HashMap::new()
    };

    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;

    for blk in 0..arch.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, blk as u16) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "sb");
        for (i, ent) in parsed.entries.iter().enumerate() {
            if ent.type_hash != TH_SOUNDBANK || !wanted.contains_key(&ent.name_hash) {
                continue;
            }
            let Some(c) = parsed.containers.get(i) else { continue };
            let Some(body) = extract_data_chunk(c) else { continue };
            let name = wanted[&ent.name_hash];

            let sub_count = u16::from_le_bytes([body[8], body[9]]) as usize;
            let sub_count2 = u16::from_le_bytes([body[10], body[11]]) as usize;
            let data_start = rd32(&body, 16) as usize;
            let off1 = rd32(&body, 20) as usize;
            let off2 = rd32(&body, 24) as usize;
            let off3 = rd32(&body, 28) as usize;

            println!(
                "\n=== {name} (0x{:08X}) body {} B",
                ent.name_hash,
                body.len()
            );
            println!(
                "  sub_count={sub_count} sub_count2={sub_count2} data_start={data_start} \
                 sec1={off1} sec2={off2} sec3={off3}"
            );

            // Section geometry: which section divides evenly by which count?
            for (tag, s, e) in [
                ("A", data_start, off1),
                ("B", off1, off2),
                ("C", off2, off3),
                ("D", off3, body.len()),
            ] {
                if e <= s {
                    continue;
                }
                let sz = e - s;
                let d1 = if sub_count > 0 && sz % sub_count == 0 {
                    format!("= {}×sub_count", sz / sub_count)
                } else {
                    format!("({:.2}/sub_count)", sz as f64 / sub_count.max(1) as f64)
                };
                let d2 = if sub_count2 > 0 && sz % sub_count2 == 0 {
                    format!("= {}×sub_count2", sz / sub_count2)
                } else {
                    String::new()
                };
                println!("  section {tag}: {sz:>7} B  {d1} {d2}");
            }

            let sec_a = off1.saturating_sub(data_start);
            // Section A's stride keys off whichever count divides it.
            let (n_rec, stride) = if sub_count2 > 0 && sec_a % sub_count2 == 0 {
                (sub_count2, sec_a / sub_count2)
            } else if sub_count > 0 && sec_a % sub_count == 0 {
                (sub_count, sec_a / sub_count)
            } else {
                println!("  section A divides by neither count — dumping rec0 region:");
                let words: Vec<String> = (0..16)
                    .map(|w| format!("+{:02}=0x{:08X}", w * 4, rd32(&body, data_start + w * 4)))
                    .collect();
                println!("    {}", words.join(" "));
                continue;
            };
            let sub_count = n_rec; // records in section A
            println!("  section A: {sec_a} B / {n_rec} records = stride {stride}");

            // Which u32 field positions in the record could be a .pws (offset, size) pair?
            // Constraint: offset+size <= pws_len for EVERY record, sizes plausible for speech,
            // and — the killer — sorting by offset must make consecutive deltas equal the size.
            let nfields = stride / 4;
            let mut best: Vec<(usize, usize, usize)> = Vec::new(); // (off_field, size_field, packed_hits)
            for of in 0..nfields {
                for sf in 0..nfields {
                    if of == sf {
                        continue;
                    }
                    let mut rows: Vec<(u64, u64)> = Vec::new();
                    let mut ok = true;
                    for r in 0..sub_count {
                        let base = data_start + r * stride;
                        let o = rd32(&body, base + of * 4) as u64;
                        let s = rd32(&body, base + sf * 4) as u64;
                        if s == 0 || s > 20_000_000 || o + s > pws_len {
                            ok = false;
                            break;
                        }
                        rows.push((o, s));
                    }
                    if !ok || rows.len() < 8 {
                        continue;
                    }
                    rows.sort();
                    let packed = rows
                        .windows(2)
                        .filter(|w| w[1].0.abs_diff(w[0].0 + w[0].1) <= 64)
                        .count();
                    best.push((of, sf, packed));
                }
            }
            best.sort_by_key(|(_, _, p)| std::cmp::Reverse(*p));
            if best.is_empty() {
                println!("  no (offset,size) field pair fits inside the .pws");
                // show record 0 so we can eyeball it
                let base = data_start;
                let words: Vec<String> = (0..nfields.min(16))
                    .map(|w| format!("+{:02}=0x{:08X}", w * 4, rd32(&body, base + w * 4)))
                    .collect();
                println!("  rec[0]: {}", words.join(" "));
                continue;
            }
            for (of, sf, packed) in best.iter().take(3) {
                println!(
                    "  candidate: offset@+{:<3} size@+{:<3} -> packed-deltas {packed}/{}",
                    of * 4,
                    sf * 4,
                    sub_count - 1
                );
            }
        }
    }
    Ok(())
}
