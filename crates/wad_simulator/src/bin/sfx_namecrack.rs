//! Recover the plaintext names of SFX cues and waves by brute-forcing a grammar.
//!
//! `sfx_extract` decodes the audio fine, but names it `cue_8BC8ABF3` / `wave_3600C76F` — a
//! `wavebank` record carries only `pandemic_hash_m2(name)`, and the shipped WAD keeps almost no
//! plaintext sound identifiers (a whole-WAD string sweep yields just **two** cue names:
//! `wpn_covertpistol_fire_npc` and `wpn_tankgun_fire_npc`, plus `wpn_bomb_timer_01_armed`).
//!
//! Those three are enough, because they expose the naming grammar:
//!
//! ```text
//!   <bank>_<action>[_<nn>][_npc]        wpn_covertpistol_fire_npc
//!                                       wpn_bomb_timer_01_armed
//! ```
//!
//! So: take the bank stem from the block name (`wpn_pistol_P000_Q3` -> `wpn_pistol`), cross it
//! with an action vocabulary and the numeric/`_npc` variants, hash each candidate, and keep the
//! ones that land on a hash we are actually looking for. FNV-1a is ~1 ns per candidate, so the
//! search space can be widened freely — the cost is vocabulary, not CPU.
//!
//! Targets come from the same blocks `sfx_extract` reads: every `sounddb` cue guid and every
//! `wavebank` clip hash that the rainbow table cannot already resolve.
//!
//! ```text
//! cargo run --release -p wad_simulator --bin sfx_namecrack -- \
//!     --wad game-files/vz.wad --filter wpn_ --emit output/sfx_names.json
//! ```
//!
//! `--emit` writes a rainbow-table fragment (`{"pandemic_hash_m2": {"0x…": ["name"]}}`), the
//! format `RainbowTable::load_many` already consumes — drop it next to the other tables and
//! re-run `sfx_extract` to get named `.wav` files.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::Wavebank;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

// From the crate lib now; this used to be `#[path = "../names.rs"] mod names;`, the workaround
// for a crate that had no [lib] and so compiled this module once per binary.
use wad_simulator::names;
use names::RainbowTable;

const TH_WAVEBANK: u32 = 0xF753_F6D0;
const TH_SOUNDDB: u32 = 0xE527_3C14;
const TH_SOUNDBANK: u32 = 0x9F8B_CA10;

fn rd32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[derive(Parser)]
#[command(about = "Brute-force the plaintext names of SFX cue guids / wave hashes")]
struct Cli {
    #[arg(long, default_value = "game-files/vz.wad")]
    wad: PathBuf,
    /// Substring the block path must contain (same filter as `sfx_extract`).
    #[arg(long, default_value = "wpn_")]
    filter: String,
    /// Write the recovered names as a rainbow-table fragment.
    #[arg(long)]
    emit: Option<PathBuf>,
    /// Extra action tokens, one per line — widen the vocabulary without editing this file.
    #[arg(long)]
    actions: Option<PathBuf>,
}

/// Action vocabulary. `fire` / `armed` / `finalstage` are the three observed in shipped plaintext;
/// the rest are the standard weapon-foley verbs, kept broad because candidates are nearly free.
const ACTIONS: &[&str] = &[
    // observed in the shipped data
    "fire", "armed", "finalstage",
    // firing
    "fire_lp", "fire_loop", "fire_start", "fire_stop", "fire_end", "fire_tail", "fire_single",
    "fire_burst", "fire_auto", "fire_semi", "shot", "shoot", "burst", "blast", "discharge",
    "launch", "report", "muzzle",
    // reloading / handling
    "reload", "reload_start", "reload_end", "reload_loop", "clipin", "clipout", "clip_in",
    "clip_out", "magin", "magout", "mag_in", "mag_out", "insert", "eject", "cock", "cocking",
    "bolt", "boltback", "boltforward", "pump", "lever", "slide", "slide_back", "slide_forward",
    "charge", "charge_up", "chargeup", "charge_lp", "handling", "foley", "rattle",
    // trigger / empty states
    "dryfire", "dry_fire", "empty", "click", "trigger", "misfire", "jam", "safety", "select",
    // equip / stow
    "equip", "unequip", "stow", "draw", "holster", "pickup", "drop", "raise", "lower",
    // spin-up weapons
    "spinup", "spindown", "spin_up", "spin_down", "spin_lp", "windup", "winddown", "whine",
    // projectile / impact tail
    "impact", "explode", "explosion", "detonate", "whizby", "flyby", "tail", "trail", "whoosh",
    "casing", "shell", "shells", "brass", "ricochet", "hit",
    // targeting / electronics
    "beep", "lock", "locked", "lockon", "unlock", "zoom", "scope", "scope_in", "scope_out",
    "targeting", "activate", "deactivate", "arm", "disarm", "timer", "warmup",
];

/// Perspective / distance / layer qualifiers. A cue is one name, but the waves under it are
/// usually per-perspective or per-layer takes, which is why cue guids crack far more readily than
/// wave hashes — the wave carries the extra token.
const VARIANTS: &[&str] = &[
    "", "npc", "player", "1p", "3p", "pov", "close", "near", "mid", "far", "distant", "dist",
    "int", "ext", "inside", "outside", "lp", "loop", "start", "stop", "end", "tail", "sub",
    "layer", "mech", "body", "low", "high", "a", "b", "c", "d",
];

/// Suffix decorations: `<stem>_<action>[_<variant>][_<nn>]` plus the reversed `..._01_armed`
/// ordering the bomb-timer cue uses. Cheap enough to emit every arrangement.
fn decorate(stem: &str, action: &str, out: &mut Vec<String>) {
    for v in VARIANTS {
        let base =
            if v.is_empty() { format!("{stem}_{action}") } else { format!("{stem}_{action}_{v}") };
        out.push(base.clone());
        for n in 1..=16 {
            out.push(format!("{base}_{n:02}"));
            out.push(format!("{base}_{n}"));
        }
        // the variant token ahead of the action, and the number ahead of it
        if !v.is_empty() {
            out.push(format!("{stem}_{v}_{action}"));
            for n in 1..=16 {
                out.push(format!("{stem}_{v}_{action}_{n:02}"));
            }
        }
        for n in 1..=16 {
            out.push(format!("{stem}_{n:02}_{action}"));
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let rb = {
        let tables: Vec<PathBuf> = [
            "tools/rainbow_table.json",
            "docs/data/aset_discovered_names.json",
            "docs/data/aset_block_strings.json",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
        RainbowTable::load_many(&tables).unwrap_or_default()
    };

    let mut actions: Vec<String> = ACTIONS.iter().map(|s| s.to_string()).collect();
    if let Some(p) = &cli.actions {
        for line in std::fs::read_to_string(p)?.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') {
                actions.push(t.to_string());
            }
        }
    }

    let mut f = File::open(&cli.wad)?;
    let size = f.metadata()?.len();
    let arch = load_ffcs_archive(&mut f, size)?;

    // ── Collect targets + the bank stems to build candidates from ────────────────────────────
    let mut targets: HashSet<u32> = HashSet::new();
    let mut stems: HashSet<String> = HashSet::new();
    let mut cue_targets = 0usize;
    let mut wave_targets = 0usize;

    let matching: Vec<(u16, String)> = arch
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.contains(&cli.filter))
        .map(|(i, p)| {
            let short = p.rsplit(['\\', '/']).next().unwrap_or(p).trim_end_matches(".block");
            // `wpn_pistol_P000_Q3` -> `wpn_pistol`: strip the streaming LOD/quality suffix.
            let stem = short.split("_P000").next().unwrap_or(short).to_string();
            (i as u16, stem)
        })
        .collect();

    for (blk, stem) in &matching {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, *blk) else { continue };
        let (parsed, _) = walk_decompressed_block(&dec, "namecrack");
        let mut hit = false;
        for (i, ent) in parsed.entries.iter().enumerate() {
            let Some(body) = parsed.containers.get(i).and_then(|c| extract_data_chunk(c)) else {
                continue;
            };
            if ent.type_hash == TH_SOUNDDB {
                if let Ok(db) = SoundDb::parse(&body) {
                    for c in &db.cues {
                        if rb.resolve(c.guid).is_none() && targets.insert(c.guid) {
                            cue_targets += 1;
                        }
                    }
                    hit = true;
                }
            } else if ent.type_hash == TH_SOUNDBANK {
                // Most cue guids are NOT in the sounddb — they live in the soundbank's section C,
                // located via the trailing offset table. Harvesting them roughly quadruples the
                // target set, and cue guids are the ones the grammar actually cracks.
                if body.len() >= 32 {
                    let cue_count = u16::from_le_bytes([body[10], body[11]]) as usize;
                    let sec_c = rd32(&body, 24) as usize;
                    let sec_end = rd32(&body, 28) as usize;
                    if sec_c <= sec_end && sec_end + cue_count * 4 <= body.len() {
                        for c in 0..cue_count {
                            let rec = sec_c + rd32(&body, sec_end + c * 4) as usize;
                            if rec + 4 > body.len() {
                                continue;
                            }
                            let guid = rd32(&body, rec);
                            if guid > 0x400 && rb.resolve(guid).is_none() && targets.insert(guid) {
                                cue_targets += 1;
                            }
                        }
                    }
                }
                hit = true;
            } else if ent.type_hash == TH_WAVEBANK {
                for clip in &Wavebank::parse(&body).clips {
                    if rb.resolve(clip.clip_hash).is_none() && targets.insert(clip.clip_hash) {
                        wave_targets += 1;
                    }
                }
                hit = true;
            }
        }
        if hit {
            stems.insert(stem.clone());
        }
    }

    eprintln!(
        "{} unresolved targets ({cue_targets} cue guids, {wave_targets} wave hashes) across {} bank stems",
        targets.len(),
        stems.len()
    );

    // ── Generate + hash ──────────────────────────────────────────────────────────────────────
    // Also try the bare stem and the stem minus its `wpn_`/`veh_`/`amb_` prefix, since a wave name
    // is not guaranteed to repeat the bank prefix.
    let mut all_stems: Vec<String> = Vec::new();
    for s in &stems {
        all_stems.push(s.clone());
        if let Some((_, rest)) = s.split_once('_') {
            all_stems.push(rest.to_string());
        }
    }
    all_stems.sort();
    all_stems.dedup();

    let mut found: BTreeMap<u32, String> = BTreeMap::new();
    let mut tried = 0u64;
    let mut cands: Vec<String> = Vec::new();
    for stem in &all_stems {
        for action in &actions {
            cands.clear();
            decorate(stem, action, &mut cands);
            for c in &cands {
                tried += 1;
                let h = pandemic_hash_m2(c);
                if targets.contains(&h) {
                    found.entry(h).or_insert_with(|| c.clone());
                }
            }
        }
    }

    eprintln!("tried {tried} candidates");
    println!("\nrecovered {} / {} names:\n", found.len(), targets.len());
    for (h, name) in &found {
        println!("  0x{h:08X}  {name}");
    }

    if let Some(path) = &cli.emit {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (h, name) in &found {
            map.insert(format!("0x{h:08X}"), vec![name.clone()]);
        }
        let mut wrapper: HashMap<&str, &BTreeMap<String, Vec<String>>> = HashMap::new();
        wrapper.insert("pandemic_hash_m2", &map);
        let mut out = File::create(path)?;
        out.write_all(serde_json::to_string_pretty(&wrapper)?.as_bytes())?;
        println!("\nemitted {} names -> {}", found.len(), path.display());
    }
    Ok(())
}
