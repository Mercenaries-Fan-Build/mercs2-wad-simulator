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
//! So a cue owns TRACKS, a track owns SOUNDS, each sound picks a SOUND GROUP, and a group owns N
//! waves — which is exactly how one `wpn_pistol_fire` cue plays ten round-robin/random takes. That
//! grouping lives in the `soundbank` (0x9F8BCA10).
//!
//! This probe dumps every audio container in a block and prints the full chain the engine follows:
//! each sounddb entry `{guid, soundbank, cue index}` → the soundbank cue → its tracks' sounds (or
//! its one group) → the group's waves, marking each wave with the block's clip it lands on.
//!
//! ```text
//! cargo run --release -p wad_simulator --bin sfx_route_probe -- --block wpn_pistol
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::soundbank::{CueBody, GroupForm, Soundbank};
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

    // ── The waves, by (wavebank, index) ──────────────────────────────────────────────────────
    let mut clip_at: HashMap<(u32, u32), u32> = HashMap::new(); // (bank, idx) -> clip hash
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_WAVEBANK {
            continue;
        }
        let body = parsed
            .containers
            .get(i)
            .and_then(|c| extract_data_chunk(c))
            .ok_or("wavebank container without a data chunk")?;
        let bank = Wavebank::parse(&body)?;
        println!("\nwavebank 0x{:08X}: {} clips", bank.self_hash, bank.clips.len());
        for (idx, c) in bank.clips.iter().enumerate() {
            println!(
                "  [{idx:>3}] clip 0x{:08X}  {} ch  {} Hz  {} frames",
                c.clip_hash,
                c.channels,
                c.sample_rate,
                c.frames()
            );
            clip_at.insert((bank.self_hash, idx as u32), c.clip_hash);
        }
    }

    let mut soundbanks: HashMap<u32, Soundbank> = HashMap::new();
    let mut dbs = Vec::new();
    for (i, ent) in parsed.entries.iter().enumerate() {
        if ent.type_hash != TH_SOUNDBANK && ent.type_hash != TH_SOUNDDB {
            continue;
        }
        let body = parsed
            .containers
            .get(i)
            .and_then(|c| extract_data_chunk(c))
            .ok_or("audio container without a data chunk")?;
        if ent.type_hash == TH_SOUNDBANK {
            let sb = Soundbank::parse(&body)?;
            soundbanks.insert(sb.bank_hash, sb);
        } else {
            dbs.push(SoundDb::parse(&body)?);
        }
    }

    let wave_label = |bank: u32, idx: u32| match clip_at.get(&(bank, idx)) {
        Some(h) => format!("0x{bank:08X}[{idx}] = clip 0x{h:08X}"),
        None => format!("0x{bank:08X}[{idx}] (not in this block)"),
    };
    let group_line = |bank: u32, g: u16| -> String {
        let Some(sb) = soundbanks.get(&bank) else {
            return format!("group {g} of soundbank 0x{bank:08X} (not in this block)");
        };
        let Some(group) = sb.groups.get(g as usize) else {
            return format!("group {g} of soundbank 0x{bank:08X}: PAST ITS {} GROUPS", sb.groups.len());
        };
        let mode = match &group.form {
            GroupForm::Single { .. } => "single".to_string(),
            GroupForm::Multi(m) => format!("selection {}", m.selection),
        };
        let waves: Vec<String> = group
            .waves()
            .iter()
            .map(|w| format!("{} w{:.3}", wave_label(w.wavebank, w.index), w.weight))
            .collect();
        format!("group {g} ({mode}): {}", waves.join(", "))
    };

    for db in &dbs {
        println!("\nsounddb 0x{:08X}: {} cues", db.self_hash, db.cues.len());
        for e in &db.cues {
            println!("  cue 0x{:08X} -> soundbank 0x{:08X} cue {}", e.guid, e.bank_hash, e.cue_index);
            let Some(sb) = soundbanks.get(&e.bank_hash) else {
                println!("    (soundbank not in this block)");
                continue;
            };
            let cue = sb
                .cues
                .get(e.cue_index as usize)
                .ok_or_else(|| format!("cue index {} past soundbank 0x{:08X}", e.cue_index, e.bank_hash))?;
            if cue.guid != e.guid {
                return Err(format!("entry 0x{:08X} lands on cue 0x{:08X}", e.guid, cue.guid).into());
            }
            match &cue.body {
                CueBody::SingleTrack { soundbank, group_index, .. } => {
                    println!("    single-track -> {}", group_line(*soundbank, *group_index));
                }
                CueBody::MultiTrack(m) => {
                    for (t, track) in m.tracks.iter().enumerate() {
                        for (k, snd) in track.sounds.iter().enumerate() {
                            println!(
                                "    track {t} sound {k} @ {:.3}s (selection {}, slot {}):",
                                snd.start_s, snd.selection, snd.slot
                            );
                            for en in &snd.entries {
                                println!("      w{:.3} -> {}", en.weight, group_line(en.soundbank, en.group_index));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
