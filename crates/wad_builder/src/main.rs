//! wad_builder — generic "raw asset → engine format → vz-patch.wad" builder.
//!
//! Jumping off from `dlc_port`/`cube_mod`, this crate adds the build-side
//! editing those lack. It started with Lua-script replacement inside a
//! multi-entry `scripts_vz` block (compile-from-source → re-wrap BINN/UCFX → fix
//! CSUM + chunk_size) and now covers model, texture and whole-WAD surgery too.
//!
//! `identity-test` is the correctness oracle: it proves the parse/serialize and
//! CSUM model reproduce a real block byte-for-byte before any edit is applied.
//! Every write path re-parses its own output and re-verifies all container CSUMs
//! before touching disk.
//!
//! Subcommands, by layer:
//!
//! * DECOMPRESSED BLOCK edits — `identity-test`, `extract-lua`, `replace-lua`,
//!   `fix-model-mtrl`, `fix-model-vertices`, `unwrap-mesh`, `reskin-eyes`,
//!   `set-tex-specular`, `rebuild-resident-tex`, `repoint-tex`, `build-atlas`.
//! * PATCH-WAD edits (via `mercs2_formats::patch_wad`) — `list-blocks`,
//!   `dump-block`, `replace-block`, `filter-keep`, `drop-blocks`, `merge-blocks`.
//!   The write paths recompute `packed_field` (INDX page count) from the
//!   DECOMPRESSED size; it sizes the engine's decompression dest buffer as
//!   `page_count << 15` (engine FUN_00875b00) and a stale value overruns the heap.
//! * END TO END — `build-skin`: patch a script inside an existing patch WAD's
//!   `scripts_vz` block, recompress (sges round-trip checked) and rebuild the WAD,
//!   then re-read the output and assert the edit landed.
//!
//! Module map:
//!
//! * [`scripts_block`] — `ScriptsBlock`/`Entry` container model (parse, serialize,
//!   CSUM verify, name lookup by `pandemic_hash_m2`, LuaQ extract/replace). The
//!   block parse/re-emit spine every other edit reuses.
//! * [`model_mtrl`] — MTRL material-array fixes: count transposition, texture-hash
//!   repointing.
//! * [`model_vertex`] — un-transpose FLOAT16 STRM vertex positions.
//! * [`model_unwrap`] — static-`MESH` slot surgery (strip `AREA` / drop the slot).
//! * [`model_reskin`] — convert a static `MESH` group into a skinned `SKIN` group.

mod model_mtrl;
mod model_reskin;
mod model_unwrap;
mod model_vertex;
// `scripts_block` now lives in `mercs2_formats` so library consumers (the modkit GUI)
// can edit Lua without depending on this bin-only crate.
use mercs2_formats::scripts_block;

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
    /// Surgery on MESH-region (static-mesh) groups in a skinned model. By default
    /// strips only the AREA chunks; with --drop-slots removes the whole MESH eye
    /// slots (eyeless de-risk build). Fixes the zero-size vertex-buffer crash.
    UnwrapMesh {
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Remove the MESH slots entirely (drop GEOM count + trim INDX) instead of
        /// only stripping their AREA chunks.
        #[arg(long, default_value_t = false)]
        drop_slots: bool,
    },
    /// Re-rig the static MESH eye slots into SKINNED groups (decl 24→32 + blend
    /// weights to a head/eyeball bone, INFO PgMesh→PgSkin, strip MESH/AREA). Keeps
    /// all 17 slots. --bone is repeatable (one HIER bone index per MESH slot, order).
    ReskinEyes {
        #[arg(long)]
        block: PathBuf,
        /// HIER bone index to bind each MESH eye slot to (repeat per slot, in order).
        #[arg(long = "bone")]
        bones: Vec<u8>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Set the specular-map flag (INFO byte[8]=0x20) on a texture block. Working `_sm`
    /// (specular) textures carry this flag; a missing flag faults the model bind.
    /// Recomputes the block CSUM.
    SetTexSpecular {
        /// Decompressed texture block (.bin).
        #[arg(long)]
        block: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Rebuild a streamed texture's RESIDENT block body from its P-tier streaming page
    /// bodies (lossless descending-P mip-chain concat: P003=mip0 .. P001=tail), keeping
    /// the resident INFO/NAME and truncating to the resident body size. Recomputes CSUM.
    /// The streamed source block (P000) ships an empty/black body; this fills it with the
    /// real pixels from the streaming pages.
    RebuildResidentTex {
        /// The resident block (.bin, the P000_Q3 block with INFO + a body to overwrite).
        #[arg(long)]
        block: PathBuf,
        /// Streaming page bodies in DESCENDING mip order (P003 first, then P002, P001).
        /// Each is a decompressed P00x block (.bin); its BODY/data chunk is concatenated.
        #[arg(long = "page")]
        pages: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Repoint MTRL texture hashes in a model block per OLD:NEW hex pairs (e.g. to
    /// redirect dropped secondary maps to base-resident globals). Recomputes CSUM.
    RepointTex {
        #[arg(long)]
        block: PathBuf,
        /// Repeatable `oldhex:newhex` (8-hex-digit) remap pair.
        #[arg(long = "remap")]
        remaps: Vec<String>,
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
    /// Copy blocks (by path substring) from a SOURCE patch WAD into a TARGET patch WAD,
    /// preserving each copied block's compressed data + ASET entries. Replaces same-path blocks.
    MergeBlocks {
        /// Target patch WAD to add blocks into.
        #[arg(long)]
        patch_wad: PathBuf,
        /// Source patch WAD to copy blocks FROM.
        #[arg(long)]
        from: PathBuf,
        /// Path substring(s) identifying which source blocks to copy (repeatable).
        #[arg(long = "block")]
        blocks: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
    /// List every block (path, page_count, #aset) in a patch WAD.
    ListBlocks {
        #[arg(long)]
        patch_wad: PathBuf,
    },
    /// Drop blocks whose path contains any of --drop (inverse of filter-keep).
    DropBlocks {
        #[arg(long)]
        patch_wad: PathBuf,
        /// Path substring(s) to DROP (repeatable).
        #[arg(long = "drop")]
        drops: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Decompress one block (matched by path substring) from a patch WAD to a file.
    DumpBlock {
        #[arg(long)]
        patch_wad: PathBuf,
        #[arg(long)]
        path: String,
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

/// Obama's skinned eye-group INFO(56) byte template (PgSkinNoTangentVP /
/// PgSkinShadowVP). The trailing runtime-pointer dwords are zeroed by model_reskin.
const OBAMA_EYE_INFO56: [u8; 56] = [
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [0,1,2]
    0xe1, 0x3a, 0x8f, 0x4e, // [3] PgSkinNoTangentVP 0x4e8f3ae1
    0x93, 0xae, 0x8f, 0x0f, // [4] PgSkinShadowVP    0x0f8fae93
    0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x01, 0x00, // [5,6]
    0xbc, 0xc3, 0x6e, 0x10, 0x0c, 0x00, 0x00, 0x00, // [7,8]
    0x00, 0x00, 0x00, 0x00, 0xc8, 0xc3, 0x6e, 0x10, // [9,10]
    0x75, 0x04, 0x18, 0x78, 0x38, 0xc4, 0x6e, 0x10, // [11,12]
    0xac, 0xd0, 0x41, 0x04, // [13]
];

fn reskin_eyes(block: &PathBuf, bones: &[u8], out: &PathBuf) -> Result<(), String> {
    if bones.is_empty() {
        return Err("no --bone given (one HIER bone index per MESH eye slot)".into());
    }
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let mut sb = ScriptsBlock::parse(&raw)?;
    let mut total = 0usize;
    for (i, e) in sb.entries.iter_mut().enumerate() {
        match model_reskin::reskin_container_eyes(&mut e.bytes, bones, &OBAMA_EYE_INFO56) {
            Ok(n) => {
                if n > 0 {
                    println!("  entry #{i}: reskinned {n} MESH eye slot(s) → SKIN (bones {bones:?})");
                    total += n;
                }
            }
            Err(m) => println!("  entry #{i}: no reskin ({m})"),
        }
    }
    let outb = sb.serialize();
    ScriptsBlock::parse(&outb)?.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    println!("Reskinned {total} eye slot(s); CSUMs valid → {}", out.display());
    Ok(())
}

/// Extract a texture block's BODY/data chunk bytes (its largest data chunk).
fn tex_body_chunk(d: &[u8]) -> Result<(usize, usize), String> {
    if d.len() < 24 || &d[20..24] != b"UCFX" {
        return Err("not a UCFX texture block".into());
    }
    let n_desc = u32::from_le_bytes([d[36], d[37], d[38], d[39]]) as usize;
    let data_base = 40 + n_desc * 20;
    let mut best: Option<(usize, usize)> = None;
    for k in 0..n_desc {
        let o = 40 + k * 20;
        let tag = &d[o..o + 4];
        if tag == b"BODY" || tag == b"data" {
            let u0 = u32::from_le_bytes([d[o + 4], d[o + 5], d[o + 6], d[o + 7]]) as usize;
            let bs = u32::from_le_bytes([d[o + 8], d[o + 9], d[o + 10], d[o + 11]]) as usize;
            if bs > 0 && best.map_or(true, |(_, b)| bs > b) {
                best = Some((data_base + u0, bs));
            }
        }
    }
    best.ok_or("no BODY/data chunk".into())
}

fn rebuild_resident_tex(block: &PathBuf, pages: &[PathBuf], out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::crc32::crc32_mercs2;
    if pages.is_empty() {
        return Err("no --page given (P003,P002,P001 in descending order)".into());
    }
    let mut d = std::fs::read(block).map_err(|e| format!("read block: {e}"))?;
    let (body_off, body_size) = tex_body_chunk(&d)?;

    // Concatenate the page bodies in the given (descending mip) order.
    let mut concat: Vec<u8> = Vec::new();
    for p in pages {
        let pd = std::fs::read(p).map_err(|e| format!("read page {}: {e}", p.display()))?;
        let (po, ps) = tex_body_chunk(&pd)?;
        concat.extend_from_slice(&pd[po..po + ps]);
    }
    if concat.len() < body_size {
        return Err(format!(
            "page concat ({}) is smaller than the resident body ({}) — missing a tier?",
            concat.len(),
            body_size
        ));
    }
    concat.truncate(body_size);

    // Overwrite the resident BODY content in place (same size), recompute CSUM.
    d[body_off..body_off + body_size].copy_from_slice(&concat);
    let csum_pos = d
        .windows(4)
        .rposition(|w| w == b"CSUM")
        .ok_or("no CSUM trailer")?;
    let csum = crc32_mercs2(&d[20..csum_pos]);
    d[csum_pos + 4..csum_pos + 8].copy_from_slice(&csum.to_le_bytes());
    std::fs::write(out, &d).map_err(|e| format!("write: {e}"))?;
    println!(
        "Rebuilt resident body ({} bytes from {} page(s)); CSUM recomputed → {}",
        body_size,
        pages.len(),
        out.display()
    );
    Ok(())
}

fn set_tex_specular(block: &PathBuf, out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::crc32::crc32_mercs2;
    let mut d = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    // Texture block: count(4) + entry(16) + UCFX container @20 (NAME/INFO/BODY) + CSUM.
    if d.len() < 24 || &d[20..24] != b"UCFX" {
        return Err("not a UCFX texture block".into());
    }
    let n_desc = u32::from_le_bytes([d[36], d[37], d[38], d[39]]) as usize;
    let data_base = 40 + n_desc * 20;
    // Find the INFO chunk's body offset.
    let mut info_abs = None;
    for k in 0..n_desc {
        let o = 40 + k * 20;
        if &d[o..o + 4] == b"INFO" {
            let u0 = u32::from_le_bytes([d[o + 4], d[o + 5], d[o + 6], d[o + 7]]) as usize;
            info_abs = Some(data_base + u0);
            break;
        }
    }
    let info_abs = info_abs.ok_or("no INFO chunk")?;
    if d[info_abs + 8] == 0x20 {
        println!("  {} already has specular flag (byte[8]=0x20)", block.display());
        std::fs::write(out, &d).map_err(|e| format!("write: {e}"))?;
        return Ok(());
    }
    println!("  {} INFO byte[8] 0x{:02x} → 0x20 (specular flag)", block.display(), d[info_abs + 8]);
    d[info_abs + 8] = 0x20;
    // Recompute CSUM = crc32_mercs2 over the UCFX container [20 .. pre-CSUM].
    let csum_pos = d
        .windows(4)
        .rposition(|w| w == b"CSUM")
        .ok_or("no CSUM trailer")?;
    let csum = crc32_mercs2(&d[20..csum_pos]);
    d[csum_pos + 4..csum_pos + 8].copy_from_slice(&csum.to_le_bytes());
    std::fs::write(out, &d).map_err(|e| format!("write: {e}"))?;
    println!("Set specular flag; CSUM recomputed → {}", out.display());
    Ok(())
}

fn repoint_tex(block: &PathBuf, remaps: &[String], out: &PathBuf) -> Result<(), String> {
    use std::collections::HashMap;
    let mut map: HashMap<u32, u32> = HashMap::new();
    for r in remaps {
        let (o, n) = r.split_once(':').ok_or_else(|| format!("bad --remap '{r}' (want OLD:NEW)"))?;
        let oh = u32::from_str_radix(o.trim_start_matches("0x"), 16)
            .map_err(|e| format!("bad old hex '{o}': {e}"))?;
        let nh = u32::from_str_radix(n.trim_start_matches("0x"), 16)
            .map_err(|e| format!("bad new hex '{n}': {e}"))?;
        map.insert(oh, nh);
    }
    if map.is_empty() {
        return Err("no --remap pairs given".into());
    }
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let mut sb = ScriptsBlock::parse(&raw)?;
    let mut total = 0usize;
    for (i, e) in sb.entries.iter_mut().enumerate() {
        match model_mtrl::repoint_container_textures(&mut e.bytes, &map) {
            Ok(n) => {
                if n > 0 {
                    println!("  entry #{i}: repointed {n} texture slot(s)");
                    total += n;
                }
            }
            Err(m) => println!("  entry #{i}: no repoint ({m})"),
        }
    }
    let outb = sb.serialize();
    ScriptsBlock::parse(&outb)?.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    println!("Repointed {total} slot(s); CSUMs valid → {}", out.display());
    Ok(())
}

fn unwrap_mesh(block: &PathBuf, out: &PathBuf, drop_slots: bool) -> Result<(), String> {
    let raw = std::fs::read(block).map_err(|e| format!("read: {e}"))?;
    let mut sb = ScriptsBlock::parse(&raw)?;
    let mut total = 0usize;
    for (i, e) in sb.entries.iter_mut().enumerate() {
        let r = if drop_slots {
            model_unwrap::drop_container_mesh_slots(&mut e.bytes)
        } else {
            model_unwrap::unwrap_container_mesh(&mut e.bytes)
        };
        match r {
            Ok(n) => {
                if n > 0 {
                    if drop_slots {
                        println!("  entry #{i}: dropped {n} MESH slot(s) entirely");
                    } else {
                        println!("  entry #{i}: stripped {n} AREA chunk-group(s) from MESH regions");
                    }
                    total += n;
                }
            }
            Err(m) => println!("  entry #{i}: no change ({m})"),
        }
    }
    let outb = sb.serialize();
    // Self-verify: re-parse + CSUMs valid.
    ScriptsBlock::parse(&outb)?.verify_csums()?;
    std::fs::write(out, &outb).map_err(|e| format!("write: {e}"))?;
    let what = if drop_slots { "MESH slot(s)" } else { "AREA group(s)" };
    println!("Processed {total} {what}; CSUMs valid → {}", out.display());
    Ok(())
}

fn replace_block(patch_wad: &PathBuf, path: &str, data: &PathBuf, out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad, PAGE_SIZE};
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
    // CRITICAL: `packed_field` (INDX word1) sizes the engine's DECOMPRESSION dest
    // buffer = packed_field << 15 (page_count × 32 KB; see engine FUN_00875b00). It
    // must cover the DECOMPRESSED size, NOT be carried over from the old (smaller)
    // block. A stale value when the new data is larger → the decompressor overruns the
    // dest buffer into adjacent heap (the 0x6B6FDA render-singleton vtable crash).
    let needed_pages = ((decompressed.len() + PAGE_SIZE - 1) / PAGE_SIZE) as u32;
    // Set the page_count EXACTLY to what the decompressed size needs. Too-small
    // overruns the dest buffer (heap crash); too-large is harmless but leaves the
    // INDX page_count inconsistent with the body (the wad_simulator P2-8 flag). An
    // exact value is correct on both counts.
    if blk.packed_field != needed_pages {
        println!(
            "  page_count(packed_field) {} → {} (decompressed {} bytes needs {} × 0x8000)",
            blk.packed_field, needed_pages, decompressed.len(), needed_pages
        );
        blk.packed_field = needed_pages;
    }
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
    let new_wad = build_patch_wad_multi(&kept, contents.csum_value, None, &FFCS_CERT_BLOB)?;
    std::fs::write(out, &new_wad).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {} ({} bytes, {} blocks)", out.display(), new_wad.len(), kept.len());
    Ok(())
}

fn merge_blocks(patch_wad: &PathBuf, from: &PathBuf, blocks: &[String], out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad, PAGE_SIZE};
    use mercs2_formats::sges::decompress_sges;
    let target = std::fs::read(patch_wad).map_err(|e| format!("read target: {e}"))?;
    let src = std::fs::read(from).map_err(|e| format!("read from: {e}"))?;
    let src_contents = read_patch_wad(&src)?;
    let want: Vec<String> = blocks.iter().map(|s| s.to_lowercase()).collect();
    let mut picked: Vec<_> = src_contents
        .blocks
        .into_iter()
        .filter(|b| {
            let p = b.path_string.to_lowercase();
            want.iter().any(|k| p.contains(k))
        })
        .collect();
    if picked.is_empty() {
        return Err("no source blocks matched --block".into());
    }
    println!("Copying {} block(s) from {}:", picked.len(), from.display());
    for b in &mut picked {
        // Correct a stale `packed_field` (decompression dest-buffer page count) so it
        // covers the block's DECOMPRESSED size — the source WAD may carry an undersized
        // value (the same packaging bug that overran Sarah's model buffer).
        let mut fixed = false;
        if let Ok(dec) = decompress_sges(&b.compressed_data) {
            let needed = ((dec.len() + PAGE_SIZE - 1) / PAGE_SIZE) as u32;
            if b.packed_field < needed {
                println!(
                    "  {} ({} comp, {} aset) — page_count {} → {} (decompressed {} bytes)",
                    b.path_string, b.compressed_data.len(), b.aset_entries.len(),
                    b.packed_field, needed, dec.len()
                );
                b.packed_field = needed;
                fixed = true;
            }
        }
        if !fixed {
            println!("  {} ({} comp bytes, {} aset)", b.path_string, b.compressed_data.len(), b.aset_entries.len());
        }
    }
    let new_wad = merge_patch_wads(&target, picked, true)?;
    std::fs::write(out, &new_wad).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {} ({} bytes)", out.display(), new_wad.len());
    Ok(())
}

fn drop_blocks(patch_wad: &PathBuf, drops: &[String], out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::{build_patch_wad_multi, read_patch_wad, FFCS_CERT_BLOB};
    let wad = std::fs::read(patch_wad).map_err(|e| format!("read: {e}"))?;
    let contents = read_patch_wad(&wad)?;
    let drop_lc: Vec<String> = drops.iter().map(|s| s.to_lowercase()).collect();
    let before = contents.blocks.len();
    let kept: Vec<_> = contents
        .blocks
        .into_iter()
        .filter(|b| {
            let p = b.path_string.to_lowercase();
            !drop_lc.iter().any(|k| p.contains(k))
        })
        .collect();
    let dropped = before - kept.len();
    if dropped == 0 {
        return Err("no blocks matched --drop".into());
    }
    println!("Dropped {dropped} block(s); kept {}", kept.len());
    let new_wad = build_patch_wad_multi(&kept, contents.csum_value, None, &FFCS_CERT_BLOB)?;
    std::fs::write(out, &new_wad).map_err(|e| format!("write: {e}"))?;
    println!("Wrote {} ({} bytes, {} blocks)", out.display(), new_wad.len(), kept.len());
    Ok(())
}

fn list_blocks(patch_wad: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::read_patch_wad;
    let wad = std::fs::read(patch_wad).map_err(|e| format!("read: {e}"))?;
    let contents = read_patch_wad(&wad)?;
    println!("{} blocks (csum_value=0x{:08x}):", contents.blocks.len(), contents.csum_value);
    for b in &contents.blocks {
        println!("  {} ({} comp bytes, packed={}, {} aset)", b.path_string, b.compressed_data.len(), b.packed_field, b.aset_entries.len());
        for a in &b.aset_entries {
            println!("      aset hash=0x{:08x} u1=0x{:08x} u2=0x{:08x} u3=0x{:08x}", a.asset_hash, a.u32_1, a.u32_2, a.u32_3);
        }
    }
    Ok(())
}

fn dump_block(patch_wad: &PathBuf, path: &str, out: &PathBuf) -> Result<(), String> {
    use mercs2_formats::patch_wad::read_patch_wad;
    use mercs2_formats::sges::decompress_sges;
    let wad = std::fs::read(patch_wad).map_err(|e| format!("read: {e}"))?;
    let contents = read_patch_wad(&wad)?;
    let blk = contents
        .blocks
        .iter()
        .find(|b| b.path_string.to_lowercase().contains(&path.to_lowercase()))
        .ok_or_else(|| format!("no block matching '{path}'"))?;
    let dec = decompress_sges(&blk.compressed_data)?;
    std::fs::write(out, &dec).map_err(|e| format!("write: {e}"))?;
    println!("Decompressed '{}' → {} ({} bytes)", blk.path_string, out.display(), dec.len());
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
        Cmd::UnwrapMesh { block, out, drop_slots } => unwrap_mesh(block, out, *drop_slots),
        Cmd::ReskinEyes { block, bones, out } => reskin_eyes(block, bones, out),
        Cmd::SetTexSpecular { block, out } => set_tex_specular(block, out),
        Cmd::RebuildResidentTex { block, pages, out } => rebuild_resident_tex(block, pages, out),
        Cmd::RepointTex { block, remaps, out } => repoint_tex(block, remaps, out),
        Cmd::BuildAtlas { blocks, out } => build_atlas(blocks, out),
        Cmd::ReplaceBlock { patch_wad, path, data, out } => replace_block(patch_wad, path, data, out),
        Cmd::BuildSkin { patch_wad, script, luac, out } => build_skin(patch_wad, script, luac, out),
        Cmd::MergeBlocks { patch_wad, from, blocks, out } => merge_blocks(patch_wad, from, blocks, out),
        Cmd::ListBlocks { patch_wad } => list_blocks(patch_wad),
        Cmd::DropBlocks { patch_wad, drops, out } => drop_blocks(patch_wad, drops, out),
        Cmd::DumpBlock { patch_wad, path, out } => dump_block(patch_wad, path, out),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wad_builder: error: {e}");
            ExitCode::FAILURE
        }
    }
}
