//! anim_patch — export a base-game player animation block and repack it into a
//! `vz-patch.wad` overlay for controlled Mercenaries 2 animation modding.
//!
//! Reuses the shipped modding pipeline verbatim
//! (`mercs2_formats::{ffcs, sges, animgroup, patch_wad}`) and matches the
//! `cube_mod` block-build pattern: source the target block via its ASET entry,
//! copy its exact `path_string` + ALL of its ASET entries, `compress_sges`, wrap
//! in a `PatchBlock`, and `build_patch_wad_multi`. The only new logic is (a)
//! locating the human-rig animgroup block that owns player-idle clip
//! `0x24F8C8E6`, and (b) the optional in-place clip perturbation passes in
//! [`perturb`].
//!
//! Modes:
//!   --roundtrip  decompress → recompress UNMODIFIED → verify byte-exact → pack.
//!   --freeze     zero every clip's dynamic wavelet coefficient data → pack.
//!
//! Output goes to `--out` (default a scratch path); the real game vz-patch.wad
//! is only touched when `--deploy` is passed.

mod perturb;

use std::fs::File;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use mercs2_formats::animgroup::parse_animgroup;
use mercs2_formats::ffcs::{find_chunk, load_ffcs_archive, AsetEntry as ArchiveAsetEntry, FfcsArchive};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_block, decompress_sges};
use mercs2_formats::types::TYPE_ID_ANIMATION;

/// The player idle clip we've been debugging — lives in a human-rig animgroup.
const PLAYER_IDLE_CLIP: u32 = 0x24F8_C8E6;

#[derive(Parser)]
#[command(
    name = "anim_patch",
    about = "Export the base-game player animation block and repack it into a vz-patch.wad overlay"
)]
struct Cli {
    /// Source vz.wad to read the animgroup block from. Defaults to the
    /// registry-discovered game vz.wad if present.
    #[arg(long)]
    wad: Option<PathBuf>,
    /// Output patch WAD path (default: crates/anim_patch/out/vz-patch.wad).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Select the animgroup block whose path_string contains this substring
    /// (e.g. "mattias" → characternameanimgroup_mattias_P000_Q3). This is the
    /// PRIMARY selector: the player (Mattias) loads a DIFFERENT block than the
    /// shared clip hash lands in (jennifer's copy is grabbed first by clip).
    /// Overridden by --block-index or --clip when either is given. Default is
    /// the player's character rig; a bare "mattias" also matches the chopper /
    /// briefing animgroups, so the default is scoped to characternameanimgroup.
    #[arg(long, default_value = "characternameanimgroup_mattias")]
    anim_name: String,
    /// Select the animgroup block by explicit index (overrides --anim-name/--clip).
    #[arg(long)]
    block_index: Option<u16>,
    /// Select the FIRST animgroup block containing this clip name-hash (legacy;
    /// overrides --anim-name). Note: shared clips like 0x24F8C8E6 exist in every
    /// character's rig, so this grabs whichever copy appears first.
    #[arg(long, value_parser = parse_hex_u32)]
    clip: Option<u32>,
    /// Identity pass: recompress the block UNMODIFIED and assert a byte-exact
    /// round-trip before packing. Run this first — nothing else is trusted until
    /// it passes.
    #[arg(long)]
    roundtrip: bool,
    /// Zero every clip's dynamic wavelet coefficient data (static/near-bind pose).
    #[arg(long)]
    freeze: bool,
    /// Write the result to the game's data/vz-patch.wad instead of --out.
    #[arg(long)]
    deploy: bool,
    /// Just report which block holds the clip (and its clips) and exit.
    #[arg(long)]
    list: bool,
    /// Write the selected block's DECOMPRESSED bytes to this path and exit.
    #[arg(long)]
    dump: Option<PathBuf>,
}

/// The `--clip` default, as an integer (kept in sync with the clap default_value).
fn cli_default_clip() -> u32 {
    parse_hex_u32("0x24F8C8E6").unwrap()
}

fn parse_hex_u32(s: &str) -> Result<u32, String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| format!("bad hex u32 '{s}': {e}"))
}

/// Discover the game's data/vz-patch.wad (sibling of the registry vz.wad).
#[cfg(windows)]
fn registry_vz_patch_wad() -> Option<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames")
        .ok()?;
    let dir: String = key.get_value("Install Dir").ok()?;
    let mut p = PathBuf::from(dir);
    p.push("data");
    p.push("vz-patch.wad");
    Some(p)
}

/// Discover the game's data/vz.wad from the EA Games registry key.
#[cfg(windows)]
fn registry_vz_wad() -> Option<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames")
        .ok()?;
    let dir: String = key.get_value("Install Dir").ok()?;
    let mut p = PathBuf::from(dir);
    p.push("data");
    p.push("vz.wad");
    p.exists().then_some(p)
}

#[cfg(not(windows))]
fn registry_vz_patch_wad() -> Option<PathBuf> {
    None
}
#[cfg(not(windows))]
fn registry_vz_wad() -> Option<PathBuf> {
    None
}

/// Candidate animgroup block indices, from the ASET table (no full decompress).
fn animgroup_blocks(archive: &FfcsArchive) -> Vec<u16> {
    let mut v: Vec<u16> = archive
        .aset
        .iter()
        .filter(|e| e.type_id == TYPE_ID_ANIMATION)
        .map(|e| e.block_index())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A selected animgroup block, decompressed and parsed, ready to pack/perturb.
struct Located {
    block_index: u16,
    path_string: String,
    decompressed: Vec<u8>,
    aset_entries: Vec<ArchiveAsetEntry>,
    clip_count: usize,
    clips: Vec<(u32, usize)>, // (name_hash, havok_offset) for every clip
}

/// How the target animgroup block is chosen (precedence: index > clip > name).
enum Selector {
    /// Explicit block index.
    BlockIndex(u16),
    /// First animgroup block containing this clip name-hash (shared-clip aware:
    /// grabs whichever character copy appears first).
    Clip(u32),
    /// Animgroup block whose `path_string` contains this substring (the intended
    /// selector — targets a SPECIFIC character's rig, e.g. "mattias").
    Name(String),
}

/// True if `bi` is an animation-type block: it owns an ASET entry with
/// `type_id == 16` AND decompresses to a parseable animgroup (record magic
/// 0x18166555 clips), so a stray path substring match on a non-anim block is
/// rejected.
fn is_animgroup_block(bi: u16, archive: &FfcsArchive) -> bool {
    archive
        .aset
        .iter()
        .any(|e| e.type_id == TYPE_ID_ANIMATION && e.block_index() == bi)
}

/// Resolve a [`Selector`] to a concrete block index.
fn select_block(file: &mut File, archive: &FfcsArchive, sel: &Selector) -> Result<u16, String> {
    match sel {
        Selector::BlockIndex(bi) => {
            if (*bi as usize) >= archive.indx.len() {
                return Err(format!("--block-index {bi} >= INDX count {}", archive.indx.len()));
            }
            if !is_animgroup_block(*bi, archive) {
                return Err(format!("block {bi} is not an animation-type block (no type_id=16 ASET)"));
            }
            Ok(*bi)
        }
        Selector::Name(substr) => {
            let needle = substr.to_lowercase();
            // Only animation-type blocks whose path contains the substring.
            let mut hits: Vec<u16> = archive
                .paths
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    p.to_lowercase().contains(&needle) && is_animgroup_block(*i as u16, archive)
                })
                .map(|(i, _)| i as u16)
                .collect();
            hits.sort_unstable();
            hits.dedup();
            match hits.as_slice() {
                [] => Err(format!(
                    "no animation-type block whose path contains '{substr}'"
                )),
                [one] => Ok(*one),
                many => {
                    // Ambiguous: list candidates so the caller can disambiguate.
                    let list: Vec<String> = many
                        .iter()
                        .map(|&b| {
                            format!(
                                "  [{b}] {}",
                                archive.paths.get(b as usize).map(|s| s.as_str()).unwrap_or("?")
                            )
                        })
                        .collect();
                    Err(format!(
                        "'{substr}' matches {} animation blocks (be more specific):\n{}",
                        many.len(),
                        list.join("\n")
                    ))
                }
            }
        }
        Selector::Clip(clip) => {
            for bi in animgroup_blocks(archive) {
                let dec = match decompress_block(file, &archive.indx, bi) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if let Ok(ag) = parse_animgroup(&dec) {
                    if ag.clips.iter().any(|c| c.name_hash == *clip) {
                        return Ok(bi);
                    }
                }
            }
            Err(format!("no animgroup block contains clip 0x{clip:08X}"))
        }
    }
}

/// Decompress + parse the chosen animgroup block into a [`Located`].
fn build_located(file: &mut File, archive: &FfcsArchive, bi: u16) -> Result<Located, String> {
    let dec = decompress_block(file, &archive.indx, bi)
        .map_err(|e| format!("decompress block {bi}: {e}"))?;
    let ag = parse_animgroup(&dec).map_err(|e| format!("parse animgroup block {bi}: {e}"))?;
    let path_string = archive
        .paths
        .get(bi as usize)
        .cloned()
        .unwrap_or_else(|| format!("block_{bi}"));
    // ALL ASET entries whose owning block is this one (preserve verbatim).
    let aset_entries: Vec<ArchiveAsetEntry> = archive
        .aset
        .iter()
        .filter(|e| e.block_index() == bi)
        .cloned()
        .collect();
    let clips = ag.clips.iter().map(|c| (c.name_hash, c.havok_offset)).collect();
    Ok(Located {
        block_index: bi,
        path_string,
        aset_entries,
        clip_count: ag.clips.len(),
        clips,
        decompressed: dec,
    })
}

/// Map an archive ASET entry into a patch-WAD ASET entry, preserving all fields.
/// `build_patch_wad_multi` re-derives the block index into `u32_2`'s high bits
/// from the block's position, so we pass the ORIGINAL sub-entry (low 16 bits of
/// `packed_block_ref`) into `u32_2`'s low bits and keep `secondary_ref`/`type_id`.
fn to_patch_aset(e: &ArchiveAsetEntry) -> AsetEntry {
    let sub_entry = (e.packed_block_ref & 0xFFFF) as u32;
    AsetEntry::new(e.asset_hash, e.secondary_ref, sub_entry, e.type_id)
}

/// Build the single override `PatchBlock` from a (possibly perturbed) decompressed
/// block, matching the cube_mod pattern (path_string + all ASET entries + packed
/// page count + defaults).
fn build_block(loc: &Located, decompressed: &[u8]) -> Result<PatchBlock, String> {
    let compressed = compress_sges(decompressed).map_err(|e| format!("sges compress: {e}"))?;
    let aset: Vec<AsetEntry> = loc.aset_entries.iter().map(to_patch_aset).collect();
    let decomp_pages = ((decompressed.len() + 0x7FFF) / 0x8000) as u32;
    let mut block = PatchBlock::new(compressed, loc.path_string.clone(), aset);
    block.packed_field = decomp_pages;
    Ok(block)
}

fn write_wad(archive: &FfcsArchive, block: PatchBlock, out: &PathBuf) -> Result<usize, String> {
    let csum_value = find_chunk(&archive.chunks, b"CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = find_chunk(&archive.chunks, b"CSUM").map(|r| r.meta);
    let wad = build_patch_wad_multi(&[block], csum_value, csum_meta, &FFCS_CERT_BLOB);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(out, &wad).map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(wad.len())
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    debug_assert_eq!(cli_default_clip(), PLAYER_IDLE_CLIP);

    if [cli.roundtrip, cli.freeze].iter().filter(|b| **b).count() > 1 {
        return Err("choose only one of --roundtrip / --freeze".into());
    }

    let wad_path = cli
        .wad
        .clone()
        .or_else(registry_vz_wad)
        .ok_or("no --wad given and vz.wad not found via registry")?;
    let mut file = File::open(&wad_path).map_err(|e| format!("open {}: {e}", wad_path.display()))?;
    let file_size = file.metadata().map_err(|e| format!("metadata: {e}"))?.len();
    let archive = load_ffcs_archive(&mut file, file_size).map_err(|e| format!("FFCS: {e}"))?;

    // Selector precedence: --block-index > --clip > --anim-name (default mattias).
    let selector = if let Some(bi) = cli.block_index {
        Selector::BlockIndex(bi)
    } else if let Some(clip) = cli.clip {
        Selector::Clip(clip)
    } else {
        Selector::Name(cli.anim_name.clone())
    };
    let bi = select_block(&mut file, &archive, &selector)?;
    let loc = build_located(&mut file, &archive, bi)?;

    let idle = cli_default_clip();
    let has_idle = loc.clips.iter().any(|(h, _)| *h == idle);
    println!(
        "block {} | path '{}' | {} ASET entries | {} clips | clip 0x{:08X} present: {}",
        loc.block_index,
        loc.path_string,
        loc.aset_entries.len(),
        loc.clip_count,
        idle,
        has_idle
    );

    if let Some(path) = &cli.dump {
        std::fs::write(path, &loc.decompressed).map_err(|e| format!("write {path:?}: {e}"))?;
        println!("dumped {} decompressed bytes -> {path:?}", loc.decompressed.len());
        return Ok(());
    }
    if cli.list {
        for (i, &(name_hash, havok_off)) in loc.clips.iter().enumerate() {
            println!("  clip[{i:3}] 0x{name_hash:08X} @0x{havok_off:X}");
        }
        return Ok(());
    }
    if !cli.roundtrip && !cli.freeze {
        return Err("specify a mode: --roundtrip, --freeze (or --list)".into());
    }

    // Work on a copy of the decompressed block; apply the perturbation, if any.
    let original = loc.decompressed.clone();
    let mut work = original.clone();

    if cli.freeze {
        let mut total = 0usize;
        let mut zeroed = 0usize;
        println!("--freeze: zeroing dynamic wavelet coefficient data per clip:");
        for (i, &(name_hash, havok_off)) in loc.clips.iter().enumerate() {
            // Bound each clip's freeze to the next clip's packfile start (or the
            // block end) so the search can't spill into an adjacent clip.
            let clip_end = loc
                .clips
                .iter()
                .skip(i + 1)
                .map(|&(_, o)| o)
                .find(|&o| o > havok_off)
                .unwrap_or(work.len());
            match perturb::freeze_clip(&mut work, name_hash, havok_off, clip_end) {
                Some(r) => {
                    println!(
                        "  clip 0x{:08X}: zeroed [0x{:X}..0x{:X}) ({} bytes)",
                        r.name_hash,
                        r.start,
                        r.start + r.len,
                        r.len
                    );
                    total += r.len;
                    zeroed += 1;
                }
                None => println!(
                    "  clip 0x{name_hash:08X}: skipped (no locatable wavelet quantData)"
                ),
            }
        }
        println!("--freeze: {zeroed}/{} clips frozen, {total} bytes zeroed total", loc.clips.len());
        if zeroed == 0 {
            return Err("--freeze: no clip could be frozen (nothing to pack)".into());
        }
    }

    // Build the override block from the (possibly perturbed) bytes.
    let block = build_block(&loc, &work)?;

    // CRITICAL VERIFY: recompress must round-trip byte-exact to `work` (sges is
    // lossless). For --roundtrip, `work == original`, so this also proves the
    // block survived the pipeline unchanged.
    let re = decompress_sges(&block.compressed_data)
        .map_err(|e| format!("verify decompress: {e}"))?;
    let exact = re == work;
    if cli.roundtrip {
        // Additionally confirm we did not perturb anything.
        let unchanged = work == original;
        let ok = exact && unchanged;
        println!(
            "ROUNDTRIP {}: re-decompressed block {} original ({} bytes), unmodified={}",
            if ok { "OK" } else { "FAIL" },
            if exact { "==" } else { "!=" },
            work.len(),
            unchanged
        );
        if !ok {
            return Err("roundtrip verification failed — block not byte-identical".into());
        }
    } else if !exact {
        return Err("sges recompress did not round-trip byte-exact to the frozen block".into());
    } else {
        println!("verify: recompressed block re-decompresses byte-exact ({} bytes)", work.len());
    }

    // Resolve output path (scratch by default; game dir only with --deploy).
    let out = if cli.deploy {
        cli.out.clone().or_else(registry_vz_patch_wad).ok_or(
            "--deploy given but game vz-patch.wad path not found via registry (pass --out)",
        )?
    } else {
        cli.out.clone().unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out").join("vz-patch.wad")
        })
    };

    let len = write_wad(&archive, block, &out)?;
    println!(
        "Wrote {} ({} bytes / {:.2} MB){}",
        out.display(),
        len,
        len as f64 / 1024.0 / 1024.0,
        if cli.deploy { " [DEPLOYED to game data dir]" } else { "" }
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("anim_patch error: {e}");
            ExitCode::FAILURE
        }
    }
}
