//! `tex-rungs` — every texture chunk of a big-endian archive and of its PC twin, one row per block.
//!
//! A texture lives in several blocks: a resident one holding `NAME`/`INFO`/`BODY`, and the finer rungs
//! of its cell. Each row gives the side, the texture, the block's path, the tags the chunk carries, the
//! `INFO` bytes as stored, the size of `BODY` and its CRC raw and with 16-bit words swapped, so the two
//! bakes' splits and bytes line up by hash and path.

use std::fmt::Write as _;
use std::fs::File;
use std::path::Path;

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_TEXTURE;
use mercs2_formats::ucfx::block_chunk_spans;

/// One chunk's rows, tags as they read in a little-endian container, offsets relative to its data area.
fn rows(container: &[u8], be: bool) -> Result<Vec<([u8; 4], usize, usize)>, String> {
    let word = |o: usize| -> Result<u32, String> {
        let b: [u8; 4] = container.get(o..o + 4).and_then(|s| s.try_into().ok()).ok_or("container is short")?;
        Ok(if be { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    };
    let (data_area, n) = (word(4)? as usize, word(16)? as usize);
    let table_end = 20 + n * 20;
    let data = if data_area > 0 { data_area } else { table_end };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let r = 20 + i * 20;
        let mut tag: [u8; 4] = container.get(r..r + 4).and_then(|s| s.try_into().ok()).ok_or("row is short")?;
        if be {
            tag.reverse();
        }
        let (off, size) = (word(r + 4)?, word(r + 8)? as usize);
        // A row whose offset is the sentinel holds children, not bytes.
        out.push((tag, if off == u32::MAX { usize::MAX } else { data + off as usize }, size));
    }
    Ok(out)
}

/// Where `--raw` writes the `BODY` of each block holding one texture.
pub struct Raw<'a> {
    pub hash: u32,
    pub dir: &'a Path,
}

fn row(tsv: &mut String, side: &str, hash: u32, path: &str, container: &[u8], be: bool, raw: Option<&Raw>) -> Result<(), String> {
    let (mut tags, mut info, mut body, mut crc, mut swapped) = (String::new(), String::new(), 0usize, 0u32, 0u32);
    for (tag, off, size) in rows(container, be)? {
        tags.push_str(&String::from_utf8_lossy(&tag));
        tags.push(' ');
        if &tag == b"INFO" && off != usize::MAX {
            for b in container.get(off..off + size).ok_or("INFO runs past the container")? {
                write!(info, "{b:02x}").map_err(|e| e.to_string())?;
            }
        } else if &tag == b"BODY" {
            body = size;
            if off != usize::MAX {
                let bytes = container.get(off..off + size).ok_or("BODY runs past the container")?;
                crc = crc32_mercs2(bytes);
                let flipped: Vec<u8> = bytes.chunks(2).flat_map(|w| w.iter().rev().copied()).collect();
                swapped = crc32_mercs2(&flipped);
                if let Some(r) = raw.filter(|r| r.hash == hash) {
                    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
                    let to = r.dir.join(format!("{side}_{hash:08X}_{file}.bin"));
                    std::fs::write(&to, bytes).map_err(|e| format!("write {}: {e}", to.display()))?;
                }
            }
        }
    }
    writeln!(tsv, "{side}\t0x{hash:08X}\t{path}\t{}\t{info}\t{body}\t{crc:08x}\t{swapped:08x}", tags.trim_end()).map_err(|e| e.to_string())
}

pub fn tex_rungs(archive: &Path, vz_wad: &Path, out: &Path, raw: Option<&Raw>) -> Result<(), String> {
    let mut tsv = String::from("side\thash\tpath\ttags\tinfo\tbody\tcrc\tcrc_swap16\n");
    let mut f = File::open(vz_wad).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    for b in 0..pc.indx.len() {
        let dec = decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("pc block {b}: {e}"))?;
        let path = pc.paths.get(b).map_or("", String::as_str);
        for (e, range) in block_chunk_spans(&dec).map_err(|e| format!("pc block {b}: {e}"))? {
            if e.type_hash == TYPE_HASH_TEXTURE {
                row(&mut tsv, "pc", e.name_hash, path, &dec[range], false, raw).map_err(|err| format!("pc block {b}: {err}"))?;
            }
        }
    }
    let (doh, _) = load_stfs_or_doh(archive)?;
    let (_, tables) = parse_be_ffcs(&doh)?;
    let indx = tables.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = tables.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    for (bi, e) in entries.iter().enumerate() {
        let path = paths.get(bi).map_or("", String::as_str);
        let be = read_be_block(&doh, e).map_err(|err| format!("block {bi}: {err}"))?;
        let word = |o: usize| be.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or(format!("block {bi}: table is short"));
        let n = word(0)? as usize;
        // The table is `[count]` then 16-byte rows of name, type, offset and size.
        let mut off = 4 + n * 16;
        for i in 0..n {
            let r = 4 + i * 16;
            let (name, ty, size) = (word(r)?, word(r + 4)?, word(r + 12)? as usize);
            let chunk = be.get(off..off + size).ok_or(format!("block {bi}: chunk {i} runs past the block"))?;
            if ty == TYPE_HASH_TEXTURE {
                row(&mut tsv, "console", name, path, chunk, true, raw).map_err(|err| format!("block {bi}: {err}"))?;
            }
            off += size;
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}
