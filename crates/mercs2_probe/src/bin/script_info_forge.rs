//! Probe: set the INFO globals count (the u16 after the name) of named scripts in a patch WAD to a
//! chosen value, for live tests of what the engine does with that field.
//!
//!   cargo run -p mercs2_probe --bin script_info_forge -- <in.wad> <out.wad> <script>=<u16> [...]
//!
//! Only those two bytes change in each named script's container; its CSUM is re-stamped, its block
//! keeps its ASET rows and tier byte, and the WAD is rebuilt the way `merge_patch_wads` rebuilds one.
//! Every named script must be found exactly once. Exit 0 on success, 1 when a name is not found
//! exactly once, 2 on bad arguments or an unreadable WAD.

use std::process::ExitCode;

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::patch_wad::{build_patch_wad_multi, read_patch_wad, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::scripts_block::{container_chunk, container_info, ScriptsBlock};
use mercs2_formats::sges::decompress_sges;

fn run(args: &[String]) -> Result<(), (u8, String)> {
    if args.len() < 3 {
        return Err((2, "usage: script_info_forge <in.wad> <out.wad> <script>=<u16> [...]".into()));
    }
    let mut targets: Vec<(String, u16, usize)> = Vec::new();
    for a in &args[2..] {
        let (name, v) = a.split_once('=').ok_or((2, format!("{a:?} is not <script>=<u16>")))?;
        let v: u16 = v.parse().map_err(|_| (2, format!("{a:?}: {v:?} is not a u16")))?;
        targets.push((name.to_string(), v, 0));
    }
    let raw = std::fs::read(&args[0]).map_err(|e| (2, format!("{}: {e}", args[0])))?;
    let contents = read_patch_wad(&raw).map_err(|e| (2, format!("{}: {e}", args[0])))?;
    let mut blocks: Vec<PatchBlock> = Vec::with_capacity(contents.blocks.len());
    for blk in contents.blocks {
        let dec = decompress_sges(&blk.compressed_data).map_err(|e| (2, format!("{}: {e}", blk.path_string)))?;
        let Ok(mut sb) = ScriptsBlock::parse(&dec) else {
            blocks.push(blk);
            continue;
        };
        let mut changed = false;
        for e in sb.entries.iter_mut() {
            let Ok(info) = container_info(&e.bytes) else { continue };
            let Some(t) = targets.iter_mut().find(|t| t.0 == info.name) else { continue };
            let body = container_chunk(&e.bytes, b"INFO").map_err(|m| (2, m))?.ok_or((2, "no INFO".to_string()))?;
            let at = body.as_ptr() as usize - e.bytes.as_ptr() as usize + 4 + info.name.len();
            e.bytes[at..at + 2].copy_from_slice(&t.1.to_le_bytes());
            let n = e.bytes.len();
            let csum = crc32_mercs2(&e.bytes[..n - 8]);
            e.bytes[n - 4..].copy_from_slice(&csum.to_le_bytes());
            println!("{} in {}: INFO globals {} -> {}", t.0, blk.path_string, info.tail_u16, t.1);
            t.2 += 1;
            changed = true;
        }
        if changed {
            blocks.push(
                PatchBlock::from_decompressed(&sb.serialize(), blk.path_string.clone(), blk.aset_entries.clone(), Some(blk.packed_field))
                    .map_err(|m| (2, format!("{}: {m}", blk.path_string)))?,
            );
        } else {
            blocks.push(blk);
        }
    }
    let bad: Vec<String> = targets.iter().filter(|t| t.2 != 1).map(|t| format!("{} found {} times", t.0, t.2)).collect();
    if !bad.is_empty() {
        return Err((1, format!("each named script must be found exactly once: {}", bad.join(", "))));
    }
    let wad = build_patch_wad_multi(&blocks, contents.csum_value, None, &FFCS_CERT_BLOB).map_err(|m| (2, m))?;
    std::fs::write(&args[1], &wad).map_err(|e| (2, format!("{}: {e}", args[1])))?;
    println!("wrote {} ({} bytes)", args[1], wad.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(code)
        }
    }
}
