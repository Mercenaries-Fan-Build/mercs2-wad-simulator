//! `lua-convert-check` — every Lua chunk of a big-endian archive, converted by
//! `be_to_le::lua::luaq_be_to_le`, against unluac's disassemble/assemble round-trip of the same chunk.
//!
//! The conversion decodes a chunk whole and encodes it little-endian. It replaced the unluac
//! round-trip, which this check keeps as the oracle: for every `BINN` body of every UCFX chunk in
//! every block, the LuaQ bytes from the converter's own [`luaq_offset`] are converted both ways and
//! compared byte for byte. java and unluac are the `java_binary` and `unluac_jar` keys of
//! `.mercs2-local.toml`.
//!
//! Exit 0 only when at least one Lua chunk was checked and every one matched.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use mercs2_formats::be_to_le::lua::{luaq_be_to_le, luaq_offset};
use mercs2_formats::be_to_le::texture::{block_chunks, container_rows};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::game_paths::{local_config_java_binary, local_config_unluac_jar};

/// unluac's round-trip of one big-endian LuaQ chunk: `--disassemble`, the listing's
/// `.endianness BIG` set to `LITTLE`, `--assemble`. The oracle, not the conversion.
fn unluac_round_trip(java: &Path, jar: &Path, be: &[u8]) -> Result<Vec<u8>, String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mercs2_lua_convert_check_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let result = (|| {
        let be_path = dir.join("be.luac");
        let listing_path = dir.join("chunk.luas");
        let le_path = dir.join("le.luac");
        std::fs::write(&be_path, be).map_err(|e| e.to_string())?;
        let dis = Command::new(java)
            .arg("-jar")
            .arg(jar)
            .arg("--disassemble")
            .arg(&be_path)
            .output()
            .map_err(|e| format!("running {}: {e}", java.display()))?;
        if !dis.status.success() {
            return Err(format!("unluac --disassemble: {}", String::from_utf8_lossy(&dis.stderr).trim()));
        }
        let text = String::from_utf8(dis.stdout).map_err(|_| "unluac's listing is not UTF-8".to_string())?;
        let flipped = [".endianness\tBIG", ".endianness BIG"]
            .iter()
            .find(|from| text.contains(*from))
            .map(|from| text.replacen(from, &from.replace("BIG", "LITTLE"), 1))
            .ok_or("unluac's listing has no `.endianness BIG` directive")?;
        std::fs::write(&listing_path, flipped).map_err(|e| e.to_string())?;
        let asm = Command::new(java)
            .arg("-jar")
            .arg(jar)
            .arg("--assemble")
            .arg(&listing_path)
            .arg("--output")
            .arg(&le_path)
            .output()
            .map_err(|e| format!("running {}: {e}", java.display()))?;
        if !asm.status.success() {
            return Err(format!("unluac --assemble: {}", String::from_utf8_lossy(&asm.stderr).trim()));
        }
        std::fs::read(&le_path).map_err(|e| format!("{}: {e}", le_path.display()))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Check every Lua chunk of `archive`. `Ok(true)` when at least one was checked and all matched.
pub fn check(archive: &Path) -> Result<bool, String> {
    let start = Path::new(env!("CARGO_MANIFEST_DIR"));
    let java: PathBuf = local_config_java_binary(start)?;
    let jar: PathBuf = local_config_unluac_jar(start)?;
    let (doh, source) = load_stfs_or_doh(archive)?;
    let (_, rows) = parse_be_ffcs(&doh)?;
    let indx_row = rows.iter().find(|r| r.tag == "INDX").ok_or("the archive has no INDX")?;
    let indx = parse_be_indx(&doh, indx_row.offset as usize, indx_row.meta as usize);
    println!("{} ({source}): {} blocks", archive.display(), indx.len());

    let (mut checked, mut same, mut not_ucfx) = (0usize, 0usize, 0usize);
    let mut distinct = std::collections::BTreeSet::new();
    let mut mismatches = Vec::new();
    for (bi, entry) in indx.iter().enumerate() {
        let block = read_be_block(&doh, entry).map_err(|e| format!("block {bi}: {e}"))?;
        for (hash, ty, chunk) in block_chunks(&block).map_err(|e| format!("block {bi}: {e}"))? {
            if chunk.get(..4) != Some(b"XFCU") {
                not_ucfx += 1;
                continue;
            }
            for (tag, off, size) in container_rows(chunk).map_err(|e| format!("block {bi} chunk 0x{hash:08x}: {e}"))? {
                if &tag != b"BINN" || size == 0 {
                    continue;
                }
                let body = chunk
                    .get(off..off + size)
                    .ok_or_else(|| format!("block {bi} chunk 0x{hash:08x}: BINN runs past the chunk"))?;
                let Some(at) = luaq_offset(body) else { continue };
                let be = &body[at..];
                let what = format!("block {bi} chunk 0x{hash:08x} type 0x{ty:08x}");
                let ours = luaq_be_to_le(be).map_err(|e| format!("{what}: the conversion refused it: {e}"))?;
                let oracle = unluac_round_trip(&java, &jar, be).map_err(|e| format!("{what}: {e}"))?;
                checked += 1;
                distinct.insert(be.to_vec());
                if ours == oracle {
                    same += 1;
                } else {
                    let at = ours.iter().zip(&oracle).position(|(a, b)| a != b).unwrap_or(ours.len().min(oracle.len()));
                    mismatches.push(format!(
                        "{what}: {} bytes, unluac {} bytes, first difference at 0x{at:x}",
                        ours.len(),
                        oracle.len()
                    ));
                }
            }
        }
    }
    for m in &mismatches {
        println!("DIFFER {m}");
    }
    println!(
        "Lua chunks checked {checked} ({} distinct), identical to unluac {same}, different {}; {not_ucfx} chunks are not UCFX containers",
        distinct.len(),
        mismatches.len()
    );
    if checked == 0 {
        println!("no Lua chunk was found, so nothing was checked");
    }
    Ok(checked > 0 && mismatches.is_empty())
}
