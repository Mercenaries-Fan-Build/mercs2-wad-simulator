//! Trace the FULL cue -> wave routing for one SFX block, to find the layer `sounddb` alone misses.
//!
//! `sfx_extract` names a wave by the `sounddb` cue that routes to it, and that leaves most waves
//! anonymous — e.g. `wpn_pistol` has 10 waves but only 1 sounddb cue. Yet all 10 audibly play, so
//! `sounddb` is NOT the whole routing story.
//!
//! The engine's own `PgSoundDb` diagnostic dump names the missing layer:
//!
//! ```text
//!   Guid %x - Num sounds: %d
//!   Sound Groups (%d):
//!   Guid %x - Sounds: %x (%s) - [%d]
//!   Track %d - State: %s, Sounds: %d, ChildCue: %d
//!   Sound %d - ID: %x, Has Wave: %d
//! ```
//!
//! So a cue owns TRACKS, a track owns a SOUND GROUP, and a group owns N sounds — which is exactly
//! how one `wpn_pistol_fire` cue plays ten round-robin/random takes. That grouping lives in the
//! `soundbank` (0x9F8BCA10), the container `sfx_extract` never opened.
//!
//! This probe dumps every audio container in a block and cross-references each section of the
//! soundbank against the block's known wave hashes and cue guids, so the group table can be
//! located by evidence rather than guessed at.
//!
//! ```text
//! cargo run --release -p wad_simulator --bin sfx_route_probe -- --block wpn_pistol
//! ```

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::Wavebank;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

const TH_WAVEBANK: u32 = 0xF753_F6D0;
const TH_SOUNDDB: u32 = 0xE527_3C14;
const TH_SOUNDBANK: u32 = 0x9F8B_CA10;

#[derive(Parser)]
#[command(about = "Dump the full cue/group/wave routing of one SFX block")]
struct Cli {
    #[arg(long, default_value = "game-files/vz.wad")]
    wad: PathBuf,
    /// Block name substring, e.g. `wpn_pistol`.
    #[arg(long, default_value = "wpn_pistol")]
    block: String,
    /// Bytes of each soundbank section to hexdump as u32 words.
    #[arg(long, default_value_t = 64)]
    words: usize,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;

    let Some((blk, path)) = arch
        .paths
        .iter()
        .enumerate()
        .find(|(_, p)| p.contains(&cli.block))
        .map(|(i, p)| (i as u16, p.clone()))
    else {
        eprintln!("no block matching {:?}", cli.block);
        return Ok(());
    };
    println!("block {blk}: {path}\n");

    let dec = decompress_block(&mut f, &arch.indx, blk)?;
    let (parsed, _) = walk_decompressed_block(&dec, "route");

    // ── Inventory every container in the block ───────────────────────────────────────────────
    println!("containers:");
    for (i, ent) in parsed.entries.iter().enumerate() {
        let len = parsed
            .containers
            .get(i)
            .and_then(|c| extract_data_chunk(c))
            .map(|b| b.len())
            .unwrap_or(0);
        let kind = match ent.type_hash {
            TH_WAVEBANK => "wavebank",
            TH_SOUNDDB => "sounddb",
            TH_SOUNDBANK => "soundbank",
            _ => "",
        };
        if !kind.is_empty() {
            println!("  [{i:>3}] 0x{:08X} {kind:<10} {len:>8} B", ent.name_hash);
        }
    }

    // ── The wave hashes we are trying to account for ─────────────────────────────────────────
    let mut wave_index: HashMap<u32, (u32, usize)> = HashMap::new(); // clip_hash -> (bank, idx)
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_WAVEBANK {
            continue;
        }
        let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
            continue;
        };
        let bank = Wavebank::parse(&body);
        println!("\nwavebank 0x{:08X}: {} clips", bank.self_hash, bank.clips.len());
        for (idx, c) in bank.clips.iter().enumerate() {
            println!(
                "  [{idx:>3}] clip 0x{:08X}  {} ch  {} Hz  {} frames",
                c.clip_hash,
                c.channels,
                c.sample_rate,
                c.frames()
            );
            wave_index.insert(c.clip_hash, (bank.self_hash, idx));
        }
    }

    let mut cue_guids: HashSet<u32> = HashSet::new();
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_SOUNDDB {
            continue;
        }
        let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
            continue;
        };
        let Ok(db) = SoundDb::parse(&body) else { continue };
        println!("\nsounddb 0x{:08X}: {} cues", db.self_hash, db.cues.len());
        for c in &db.cues {
            println!(
                "  cue 0x{:08X} -> bank 0x{:08X} wave_index {}",
                c.guid, c.bank_hash, c.wave_index
            );
            cue_guids.insert(c.guid);
        }
    }

    // ── The soundbank: where do the wave hashes actually appear? ─────────────────────────────
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_SOUNDBANK {
            continue;
        }
        let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
            continue;
        };
        let self_hash = rd32(&body, 4);
        let sub_count = u16::from_le_bytes([body[8], body[9]]);
        let sub_count2 = u16::from_le_bytes([body[10], body[11]]);
        let data_start = rd32(&body, 16) as usize;
        let s1 = rd32(&body, 20) as usize;
        let s2 = rd32(&body, 24) as usize;
        let s3 = rd32(&body, 28) as usize;
        println!(
            "\nsoundbank 0x{self_hash:08X}: {} B  sub_count={sub_count} sub_count2={sub_count2}\n  \
             data_start={data_start} A=[{data_start}..{s1}) B=[{s1}..{s2}) C=[{s2}..{s3}) tail=[{s3}..{})",
            body.len(),
            body.len()
        );
        if sub_count > 0 && s1 > data_start {
            println!("  section A stride = {}", (s1 - data_start) / sub_count as usize);
        }
        if sub_count2 > 0 && s3 > s2 {
            println!("  section C stride = {}", (s3 - s2) / sub_count2 as usize);
        }

        // Scan the WHOLE body for u32s that are wave hashes or cue guids — the decisive test.
        let mut wave_hits: Vec<(usize, u32, usize)> = Vec::new();
        let mut cue_hits: Vec<(usize, u32)> = Vec::new();
        let mut off = 0;
        while off + 4 <= body.len() {
            let v = rd32(&body, off);
            if let Some((_, idx)) = wave_index.get(&v) {
                wave_hits.push((off, v, *idx));
            }
            if cue_guids.contains(&v) {
                cue_hits.push((off, v));
            }
            off += 4;
        }
        let section = |o: usize| -> &'static str {
            if o < data_start {
                "header"
            } else if o < s1 {
                "A"
            } else if o < s2 {
                "B"
            } else if o < s3 {
                "C"
            } else {
                "tail"
            }
        };
        println!(
            "\n  wave-hash hits: {} of {} waves referenced",
            wave_hits.len(),
            wave_index.len()
        );
        for (o, v, idx) in &wave_hits {
            println!("    +0x{o:04X} [{}]  0x{v:08X}  = wave[{idx}]", section(*o));
        }
        println!("  cue-guid hits: {}", cue_hits.len());
        for (o, v) in &cue_hits {
            println!("    +0x{o:04X} [{}]  0x{v:08X}", section(*o));
        }

        // Walk the group table the way `sfx_extract` does, and show what each group claims.
        let mut offs: Vec<usize> = Vec::new();
        for g in 0..sub_count as usize {
            offs.push(data_start + rd32(&body, s1 + g * 4) as usize);
        }
        println!("\n  groups:");
        for (g, &start) in offs.iter().enumerate() {
            let end = offs.get(g + 1).copied().unwrap_or(s1);
            print!("    g{g}: @{start}..{end} ({} B) ->", end.saturating_sub(start));
            let mut o = start;
            let mut any = false;
            while o + 8 <= end {
                if rd32(&body, o) == self_hash {
                    print!(
                        " ({},{:.3})",
                        rd32(&body, o + 4),
                        f32::from_bits(rd32(&body, o + 8))
                    );
                    any = true;
                    o += 12;
                    continue;
                }
                o += 4;
            }
            if !any {
                print!("  <no wave refs>");
            }
            println!();
        }

        // Section C: one record per cue. Show which groups each cue's tracks reach.
        if sub_count2 > 0 && s3 > s2 {
            // The trailing section is section C's offset table, exactly as section B is
            // section A's — section C records are variable-length too, so a computed
            // stride is wrong (shotgun: 844/4 = 211, not even 4-byte aligned).
            let coffs: Vec<usize> = (0..sub_count2 as usize)
                .map(|c| s2 + rd32(&body, s3 + c * 4) as usize)
                .collect();
            println!("\n  cues (section C, offsets {coffs:?}):");
            for c in 0..sub_count2 as usize {
                let rec = coffs[c];
                let stride = coffs.get(c + 1).copied().unwrap_or(s3) - rec;
                print!("    cue[{c}] 0x{:08X} -> groups", rd32(&body, rec));
                let mut o = rec + 4;
                let mut gs: Vec<u32> = Vec::new();
                while o + 8 <= (rec + stride).min(body.len()) {
                    if rd32(&body, o) == self_hash {
                        let g = rd32(&body, o + 4);
                        if !gs.contains(&g) {
                            gs.push(g);
                        }
                    }
                    o += 4;
                }
                println!(" {gs:?}");
            }
        }

        // Head of each section as words, for layout eyeballing.
        for (name, start, end) in
            [("A", data_start, s1), ("B", s1, s2), ("C", s2, s3), ("tail", s3, body.len())]
        {
            if end <= start {
                continue;
            }
            println!("\n  section {name} @{start}..{end}:");
            let mut o = start;
            let mut printed = 0;
            while o + 4 <= end && printed < cli.words {
                if printed % 8 == 0 {
                    print!("\n    +0x{:04X}:", o);
                }
                print!(" {:08X}", rd32(&body, o));
                o += 4;
                printed += 1;
            }
            println!();
        }
    }
    Ok(())
}
