//! wad_builder — generic "raw asset → engine format → vz-patch.wad" builder.
//!
//! Jumping off from `dlc_port`/`cube_mod`, this crate adds the build-side
//! editing those lack: replacing a Lua script inside a multi-entry `scripts_vz`
//! block (compile-from-source → re-wrap BINN/UCFX → fix CSUM + chunk_size).
//!
//! `identity-test` is the correctness oracle: it proves the parse/serialize and
//! CSUM model reproduce a real block byte-for-byte before any edit is applied.

mod model_mtrl;
mod model_vertex;
mod scripts_block;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use scripts_block::{parse_container, Entry, ScriptsBlock};

#[derive(Parser)]
#[command(name = "wad_builder", about = "Build engine WADs from raw/edited assets")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Verify the scripts-block model round-trips a real block byte-for-byte.
    IdentityTest {
        /// Decompressed scripts_vz block (.bin).
        #[arg(long)]
        block: PathBuf,
        /// Script name to inspect in detail (default: wifpmcinterior).
        #[arg(long, default_value = "wifpmcinterior")]
        inspect: String,
    },
    /// Extract one script's raw LuaQ bytecode.
    ExtractLua {
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        script: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Replace one script's LuaQ bytecode and re-emit the decompressed block.
    ReplaceLua {
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        script: String,
        /// New compiled LuaQ bytecode (.luac).
        #[arg(long)]
        luac: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Fix multi-material MTRL count transposition in a decompressed model block.
    FixModelMtrl {
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Fix transposed FLOAT16 vertex positions in a decompressed model block.
    FixModelVertices {
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Combine N decompressed single-entry texture blocks into one multi-entry
    /// atlas block (the resident-atlas layout: count + table + UCFX bodies).
    BuildAtlas {
        /// Decompressed single-entry block(s) to pack (repeat --block).
        #[arg(long = "block")]
        blocks: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Replace a decompressed block (matched by path substring) inside a patch WAD.
    ReplaceBlock {
        #[arg(long)]
        patch_wad: PathBuf,
        /// Path substring identifying the block to replace.
        #[arg(long)]
        path: String,
        /// New DECOMPRESSED block bytes (will be sges-compressed).
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Keep only blocks whose path contains one of --keep, rebuild the patch WAD.
    FilterKeep {
        #[arg(long)]
        patch_wad: PathBuf,
        /// Path substring(s) to KEEP (repeatable). Blocks matching none are dropped.
        #[arg(long)]
        keep: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Patch a script inside an existing patch WAD's scripts_vz block and rebuild the WAD.
    BuildSkin {
        /// Source patch WAD to edit (e.g. vz-patch-human.wad), preserved except scripts_vz.
        #[arg(long)]
        patch_wad: PathBuf,
        /// Script to replace (default wifpmcinterior).
        #[arg(long, default_value = "wifpmcinterior")]
        script: String,
        /// New compiled LuaQ bytecode (.luac).
        #[arg(long)]
        luac: PathBuf,
        /// Output patch WAD.
        #[arg(long)]
        out: PathBuf,
    },
}

fn identity_test(block_path: &PathBuf, inspect: &str) -> Result<(), String> {
    let raw = std::fs::read(block_path).map_err(|e| format!("read {}: {e}", block_path.display()))?;
    println!("Loaded {} ({} bytes)", block_path.display(), raw.len());

    let sb = ScriptsBlock::parse(&raw)?;
    println!("Parsed {} container entries", sb.entries.len());

    // (1) CSUM model: every trailing CSUM == JAMCRC over [UCFX..pre-CSUM].
    let n = sb.verify_csums()?;
    println!("✓ CSUM verified on all {n} containers (CRC-32/JAMCRC)");

    // (2) Round-trip identity: re-serialize must reproduce the input bytes.
    let reser = sb.serialize();
    if reser != raw {
        return Err(format!(
            "round-trip mismatch: in={} out={} (first diff at {:?})",
            raw.len(),
            reser.len(),
            reser.iter().zip(&raw).position(|(a, b)| a != b)
        ));
    }
    println!("✓ Round-trip identity: re-serialized block == input ({} bytes)", reser.len());

    // (3) Locate + dump the inspect target's container layout (from real bytes).
    let idx = sb
        .find_by_name(inspect)
        .ok_or_else(|| format!("'{inspect}' not found (pandemic_hash_m2 mismatch)"))?;
    let e = &sb.entries[idx];
    println!(
        "\n'{inspect}' = entry #{idx}  name_hash=0x{:08X} type_hash=0x{:08X} chunk_size={}",
        e.name_hash,
        e.type_hash,
        e.bytes.len()
    );
    let lay = parse_container(&e.bytes)?;
    println!(
        "  UCFX data_base={}  BINN desc@{} body_size={}  LuaQ@{} len={}  CSUM@{} stored=0x{:08X}",
        lay.data_base, lay.binn_desc_off, lay.binn_body_size, lay.luaq_off, lay.luaq_len, lay.csum_off, lay.stored_csum
    );
    // Cross-check: BINN descriptor body_size should equal the LuaQ length (§5.3).
    if lay.binn_body_size as usize == lay.luaq_len {
        println!("  ✓ BINN.body_size == LuaQ length ({})", lay.luaq_len);
    } else {
        println!(
            "  ⚠ BINN.body_size ({}) != LuaQ length ({}) — body_size is not pure-LuaQ; will locate metadata explicitly",
            lay.binn_body_size, lay.luaq_len
        );
    }
    // Show the BINN metadata prefix (between data_base bodies and LuaQ) head.
    let pre = &e.bytes[..lay.luaq_off];
    let tail = pre.len().saturating_sub(32);
    println!("  bytes just before LuaQ [{tail}..{}]: {}", pre.len(), hex(&pre[tail..]));
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn extract_lua(block: &PathBuf, script: &str, out: &PathBuf) -> Result<(), String> {
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let sb = ScriptsBlock::parse(&raw)?;
    let idx = sb.find_by_name(script).ok_or_else(|| format!("'{script}' not found"))?;
    let lua = sb.extract_lua(idx)?;
    std::fs::write(out, &lua).map_err(|e| format!("write: {e}"))?;
    println!("Extracted '{script}' LuaQ: {} bytes → {}", lua.len(), out.display());
    Ok(())
}

fn replace_lua(block: &PathBuf, script: &str, luac: &PathBuf, out: &PathBuf) -> Result<(), String> {
    let raw = std::fs::read(block).map_err(|e| format!("read block: {e}"))?;
    let new_luaq = std::fs::read(luac).map_err(|e| format!("read luac: {e}"))?;
    if new_luaq.get(..4) != Some(b"\x1bLua") {
        return Err("luac file is not LuaQ bytecode (missing \\x1bLua header)".into());
    }
    let mut sb = ScriptsBlock::parse(&raw)?;
    let idx = sb.find_by_name(script).ok_or_else(|| format!("'{script}' not found"))?;
    let old = sb.extract_lua(idx)?;
    sb.replace_lua(idx, &new_luaq)?;
    println!("Replaced '{script}' LuaQ: {} → {} bytes", old.len(), new_luaq.len());

    // Self-verify the rebuilt block: CSUMs valid, re-parse round-trips.
    let rebuilt = sb.serialize();
    let check = ScriptsBlock::parse(&rebuilt)?;
    let n = check.verify_csums()?;
    println!("✓ rebuilt block: {n} containers, all CSUMs valid, {} bytes", rebuilt.len());
    if new_luaq == old && rebuilt != raw {
        return Err("identical-LuaQ replace did NOT reproduce input block".into());
    }
    if new_luaq == old {
        println!("✓ identity check: identical-LuaQ replace reproduced input byte-for-byte");
    }
    std::fs::write(out, &rebuilt).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {}", out.display());
    Ok(())
}

fn build_skin(patch_wad: &PathBuf, script: &str, luac: &PathBuf, out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad};
    use mercs2_formats::sges::{compress_sges, decompress_sges};

    let wad = std::fs::read(patch_wad).map_err(|e| format!("read patch wad: {e}"))?;
    let new_luaq = std::fs::read(luac).map_err(|e| format!("read luac: {e}"))?;
    let contents = read_patch_wad(&wad)?;

    // Locate the scripts_vz block by path.
    let idx = contents
        .blocks
        .iter()
        .position(|b| b.path_string.to_lowercase().contains("scripts_vz"))
        .ok_or("no scripts_vz block in patch WAD")?;
    let mut blk = contents.blocks[idx].clone();
    println!("scripts_vz block: '{}' ({} compressed bytes)", blk.path_string, blk.compressed_data.len());

    // Decompress → edit the script → recompress.
    let decompressed = decompress_sges(&blk.compressed_data)?;
    let mut sb = ScriptsBlock::parse(&decompressed)?;
    let sidx = sb.find_by_name(script).ok_or_else(|| format!("'{script}' not in scripts_vz"))?;
    let old_len = sb.extract_lua(sidx)?.len();
    sb.replace_lua(sidx, &new_luaq)?;
    let edited = sb.serialize();
    println!("  '{script}' LuaQ {old_len} → {} bytes; block {} → {} bytes", new_luaq.len(), decompressed.len(), edited.len());

    // Self-verify the edited decompressed block before recompressing.
    ScriptsBlock::parse(&edited)?.verify_csums()?;
    let recompressed = compress_sges(&edited)?;
    if decompress_sges(&recompressed)? != edited {
        return Err("sges recompress round-trip mismatch".into());
    }
    println!("  recompressed: {} bytes (sges round-trip ok)", recompressed.len());

    // Swap compressed data (keep packed_field/flags/aset — decompressed page count
    // is unchanged) and rebuild the WAD, replacing the scripts_vz block by path.
    blk.compressed_data = recompressed;
    let new_wad = merge_patch_wads(&wad, vec![blk], true)?;
    std::fs::write(out, &new_wad).map_err(|e| format!("write out: {e}"))?;
    println!("Wrote {} ({} bytes)", out.display(), new_wad.len());

    // Validate the output WAD round-trips and carries the edit.
    let check = read_patch_wad(&new_wad)?;
    let cidx = check
        .blocks
        .iter()
        .position(|b| b.path_string.to_lowercase().contains("scripts_vz"))
        .ok_or("scripts_vz missing from output")?;
    let dec = decompress_sges(&check.blocks[cidx].compressed_data)?;
    let csb = ScriptsBlock::parse(&dec)?;
    csb.verify_csums()?;
    let back = csb.extract_lua(csb.find_by_name(script).ok_or("script missing in output")?)?;
    if back != new_luaq {
        return Err("output scripts_vz LuaQ != edited bytecode".into());
    }
    println!("✓ output WAD validated: {} blocks, scripts_vz CSUMs valid, '{script}' edit present", check.blocks.len());
    Ok(())
}

fn fix_model_mtrl(block: &PathBuf, out: &PathBuf) -> Result<(), String> {
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let mut sb = ScriptsBlock::parse(&raw)?;
    let mut total = 0usize;
    for (i, e) in sb.entries.iter_mut().enumerate() {
        match model_mtrl::fix_container_mtrl(&mut e.bytes) {
            Ok(n) => {
                if n > 0 {
                    println!("  entry #{i}: transposed {n} material count-pair(s)");
                    total += n;
                }
            }
            Err(m) => println!("  entry #{i}: no MTRL fix ({m})"),
        }
    }
    let outb = sb.serialize();
    // Self-verify: re-parse + CSUMs valid.
    ScriptsBlock::parse(&outb)?.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    println!("Fixed {total} material record(s); CSUMs valid → {}", out.display());
    Ok(())
}

fn build_atlas(blocks: &[PathBuf], out: &PathBuf) -> Result<(), String> {
    let mut entries: Vec<Entry> = Vec::new();
    for b in blocks {
        let raw = std::fs::read(b).map_err(|e| format!("read {}: {e}", b.display()))?;
        let sb = ScriptsBlock::parse(&raw)?;
        for e in sb.entries {
            println!("  + entry name_hash=0x{:08X} type=0x{:08X} ({} bytes)", e.name_hash, e.type_hash, e.bytes.len());
            entries.push(e);
        }
    }
    let atlas = ScriptsBlock { entries };
    let outb = atlas.serialize();
    let check = ScriptsBlock::parse(&outb)?;
    check.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    println!("atlas: {} entries, {} bytes, CSUMs valid -> {}", check.entries.len(), outb.len(), out.display());
    Ok(())
}

fn fix_model_vertices(block: &PathBuf, out: &PathBuf) -> Result<(), String> {
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let mut sb = ScriptsBlock::parse(&raw)?;
    let mut total = 0usize;
    for (i, e) in sb.entries.iter_mut().enumerate() {
        match model_vertex::fix_container_vertices(&mut e.bytes) {
            Ok(n) => {
                if n > 0 {
                    println!("  entry #{i}: un-transposed positions in {n} vertex buffer(s)");
                    total += n;
                }
            }
            Err(m) => println!("  entry #{i}: no vertex fix ({m})"),
        }
    }
    let outb = sb.serialize();
    ScriptsBlock::parse(&outb)?.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    println!("Fixed {total} vertex buffer(s); CSUMs valid → {}", out.display());
    Ok(())
}

fn replace_block(patch_wad: &PathBuf, path: &str, data: &PathBuf, out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad};
    use mercs2_formats::sges::compress_sges;

    let wad = std::fs::read(patch_wad).map_err(|e| format!("read wad: {e}"))?;
    let decompressed = std::fs::read(data).map_err(|e| format!("read data: {e}"))?;
    let contents = read_patch_wad(&wad)?;
    let idx = contents
        .blocks
        .iter()
        .position(|b| b.path_string.to_lowercase().contains(&path.to_lowercase()))
        .ok_or_else(|| format!("no block matching '{path}'"))?;
    let mut blk = contents.blocks[idx].clone();
    println!("replacing '{}' ({} → recompressing {} decompressed bytes)", blk.path_string, blk.compressed_data.len(), decompressed.len());
    blk.compressed_data = compress_sges(&decompressed)?;
    let new_wad = merge_patch_wads(&wad, vec![blk], true)?;
    std::fs::write(out, &new_wad).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {} ({} bytes)", out.display(), new_wad.len());
    Ok(())
}

fn filter_keep(patch_wad: &PathBuf, keep: &[String], out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{build_patch_wad_multi, read_patch_wad, FFCS_CERT_BLOB};
    let wad = std::fs::read(patch_wad).map_err(|e| format!("read: {e}"))?;
    let contents = read_patch_wad(&wad)?;
    let keep_lc: Vec<String> = keep.iter().map(|s| s.to_lowercase()).collect();
    let kept: Vec<_> = contents
        .blocks
        .into_iter()
        .filter(|b| {
            let p = b.path_string.to_lowercase();
            keep_lc.iter().any(|k| p.contains(k))
        })
        .collect();
    println!("Kept {} blocks:", kept.len());
    for b in &kept {
        println!("  {}", b.path_string);
    }
    if kept.is_empty() {
        return Err("no blocks matched --keep".into());
    }
    let new_wad = build_patch_wad_multi(&kept, contents.csum_value, None, &FFCS_CERT_BLOB);
    std::fs::write(out, &new_wad).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {} ({} bytes, {} blocks)", out.display(), new_wad.len(), kept.len());
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let r = match &cli.cmd {
        Cmd::IdentityTest { block, inspect } => identity_test(block, inspect),
        Cmd::FilterKeep { patch_wad, keep, out } => filter_keep(patch_wad, keep, out),
        Cmd::ExtractLua { block, script, out } => extract_lua(block, script, out),
        Cmd::ReplaceLua { block, script, luac, out } => replace_lua(block, script, luac, out),
        Cmd::FixModelMtrl { block, out } => fix_model_mtrl(block, out),
        Cmd::FixModelVertices { block, out } => fix_model_vertices(block, out),
        Cmd::BuildAtlas { blocks, out } => build_atlas(blocks, out),
        Cmd::ReplaceBlock { patch_wad, path, data, out } => replace_block(patch_wad, path, data, out),
        Cmd::BuildSkin { patch_wad, script, luac, out } => build_skin(patch_wad, script, luac, out),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wad_builder: error: {e}");
            ExitCode::FAILURE
        }
    }
}
