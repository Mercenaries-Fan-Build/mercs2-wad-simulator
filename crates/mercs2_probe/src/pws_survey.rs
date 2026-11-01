//! `pws-survey` — every wavebank of an Xbox DLC package, and each streamed record against the bytes
//! of the `.pws` it names.
//!
//! One row per record: the bank, its kind and `.pws` name, then the record's fields as stored. A
//! streamed record adds the first four bytes it addresses and whether its range fits the file, so the
//! codec reads off the stream rather than off the record's format byte. `--extract <dir>` writes every
//! `.pws` of the package.

use std::fmt::Write as _;
use std::path::Path;

use mercs2_formats::be_to_le::texture::container_rows;
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh_with_streams;

const TH_WAVEBANK: u32 = 0xF753_F6D0;

fn be32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("short read at +0x{o:X}"))
}

/// Every streamed record of a PC archive, one row per record, with the first bytes each range holds in
/// the `.pws` under `audios`.
pub fn pc_streams(wad: &Path, audios: &Path, out: &Path) -> Result<(), String> {
    let le32 = |b: &[u8], o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("short read at +0x{o:X}"));
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut tsv = String::from("bank\tpws\trec\tchannels\tformat\trate\tsize\tfield_10\tword_1c\toffset\tfile_len\tfirst4\n");
    for b in 0..pc.indx.len() {
        let dec = mercs2_formats::sges::decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("block {b}: {e}"))?;
        for (e, range) in mercs2_formats::ucfx::block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
            if e.type_hash != TH_WAVEBANK {
                continue;
            }
            let body = mercs2_formats::ucfx::extract_chunk_body(&dec[range], b"data").ok_or_else(|| format!("block {b}: bank 0x{:08X} has no data chunk", e.name_hash))?;
            if u16::from_le_bytes([body[10], body[11]]) == 0 {
                continue;
            }
            let count = u16::from_le_bytes([body[8], body[9]]) as usize;
            let roff = le32(&body, 16)? as usize;
            let pws = String::from_utf8_lossy(&body[24..roff]).trim_end_matches('\0').to_string();
            let stem = pws.trim_end_matches(".pws");
            let file = [format!("{stem}.pws"), format!("{stem}.english.pws")].into_iter().map(|n| audios.join(n)).find(|p| p.is_file());
            let bytes = match &file {
                Some(p) => std::fs::read(p).map_err(|err| format!("{}: {err}", p.display()))?,
                None => return Err(format!("no file for {pws} under {}", audios.display())),
            };
            for ri in 0..count {
                let rr = roff + ri * 36;
                let (rate, dsz, f10, w1c, offset) = (le32(&body, rr + 8)?, le32(&body, rr + 12)?, le32(&body, rr + 16)?, le32(&body, rr + 28)?, le32(&body, rr + 32)?);
                let first = bytes.get(offset as usize..offset as usize + 4).map_or(String::new(), |s| format!("{:02x}{:02x}{:02x}{:02x}", s[0], s[1], s[2], s[3]));
                writeln!(tsv, "0x{:08X}\t{pws}\t{ri}\t{}\t{}\t{rate}\t{dsz}\t{f10}\t0x{w1c:08X}\t{offset}\t{}\t{first}", e.name_hash, body[rr + 5], body[rr + 6], bytes.len()).map_err(|err| err.to_string())?;
            }
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}

pub fn pws_survey(package: &Path, out: &Path, extract: Option<&Path>) -> Result<(), String> {
    let (doh, _, files) = load_stfs_or_doh_with_streams(package)?;
    let mut streams = std::collections::HashMap::new();
    for (name, bytes) in files {
        if let Some(dir) = extract {
            let to = dir.join(&name);
            std::fs::write(&to, &bytes).map_err(|err| format!("write {}: {err}", to.display()))?;
        }
        streams.insert(name.to_ascii_lowercase(), bytes);
    }
    let (_, tables) = parse_be_ffcs(&doh)?;
    let indx = tables.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = tables.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    let mut tsv = String::from("block\tpath\tbank\tkind\tpws\trec\tclip\tbytes4_7\trate\tsize\tframes\tword_1c\toffset\tin_file\tfirst4\n");
    for (bi, e) in entries.iter().enumerate() {
        let path = paths.get(bi).map_or("", String::as_str);
        let be = read_be_block(&doh, e).map_err(|err| format!("block {bi}: {err}"))?;
        let n = be32(&be, 0)? as usize;
        let mut off = 4 + n * 16;
        for i in 0..n {
            let r = 4 + i * 16;
            let (name, ty, size) = (be32(&be, r)?, be32(&be, r + 4)?, be32(&be, r + 12)? as usize);
            let chunk = be.get(off..off + size).ok_or_else(|| format!("block {bi}: chunk {i} runs past the block"))?;
            off += size;
            if ty != TH_WAVEBANK {
                continue;
            }
            let (_, doff, dsize) = container_rows(chunk)?
                .into_iter()
                .find(|(t, ..)| t == b"data")
                .ok_or_else(|| format!("block {bi}: bank 0x{name:08X} has no data chunk"))?;
            let body = chunk.get(doff..doff + dsize).ok_or_else(|| format!("block {bi}: data runs past the chunk"))?;
            let count = u16::from_be_bytes([body[8], body[9]]) as usize;
            let kind = body[0x0A];
            let roff = be32(body, 16)? as usize;
            let pws = if kind != 0 {
                String::from_utf8_lossy(&body[24..roff]).trim_end_matches('\0').to_string()
            } else {
                String::new()
            };
            let lang_file = pws.to_ascii_lowercase();
            for ri in 0..count {
                let rr = roff + ri * 36;
                let (clip, rate, dsz, frames, w1c, doffset) =
                    (be32(body, rr)?, be32(body, rr + 8)?, be32(body, rr + 12)?, be32(body, rr + 16)?, be32(body, rr + 28)?, be32(body, rr + 32)?);
                let fmt = &body[rr + 4..rr + 8];
                let (mut fits, mut first) = ("-".to_string(), String::new());
                if kind != 0 {
                    // The name the bank gives and the file the package ships are read together here: the
                    // survey lists every `.pws` of the package whose stem starts with the bank's.
                    let stem = lang_file.trim_end_matches(".pws");
                    let mut hits: Vec<&String> = streams.keys().filter(|k| k.starts_with(stem)).collect();
                    hits.sort();
                    for k in hits {
                        let f = &streams[k];
                        let end = doffset as usize + dsz as usize;
                        write!(fits, "{}:{} ", k, end <= f.len()).map_err(|e| e.to_string())?;
                        if let Some(b) = f.get(doffset as usize..doffset as usize + 4) {
                            write!(first, "{:02x}{:02x}{:02x}{:02x} ", b[0], b[1], b[2], b[3]).map_err(|e| e.to_string())?;
                        }
                    }
                }
                writeln!(
                    tsv,
                    "{bi}\t{path}\t0x{name:08X}\t{kind}\t{pws}\t{ri}\t0x{clip:08X}\t{:02x}{:02x}{:02x}{:02x}\t{rate}\t{dsz}\t{frames}\t0x{w1c:08X}\t{doffset}\t{}\t{}",
                    fmt[0], fmt[1], fmt[2], fmt[3], fits.trim_end(), first.trim_end()
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}
