//! Every Lua script container in a WAD, on every platform.
//!
//! A WAD (`FFCS` little-endian on PC, `SCFF` big-endian on Xbox 360 and PS3) is opened with
//! `mercs2_formats::ffcs::load_ffcs_archive`; every INDX block is inflated with
//! `mercs2_formats::sges::decompress_block` (which takes both byte orders). A decompressed block
//! starts with an entry table, all fields in the WAD's byte order:
//!
//! ```text
//! u32 entry_count
//! entry_count × { u32 name_hash, u32 type_hash, u32 field_c, u32 chunk_size }
//! the containers, back to back, chunk_size bytes each
//! ```
//!
//! A script is an entry whose `type_hash` is `mercs2_formats::types::TYPE_HASH_SCRIPT`; scripts are
//! found by that marker in every block, so no block is named in advance. Its container is a UCFX
//! container (magic `UCFX` on PC, `XFCU` on console, every field and tag in the WAD's byte order):
//!
//! ```text
//! +0x00 magic
//! +0x04 u32 data_area_off
//! +0x08 u32, +0x0c u32
//! +0x10 u32 n_desc
//! +0x14 n_desc × { tag[4], u32 rel_off, u32 size, u32, u32 }   body = data_area_off + rel_off
//! …     bodies
//! end   "CSUM" + u32 CRC-32/JAMCRC over every byte before the "CSUM" tag
//! ```
//!
//! The trailer is the same on every platform: the tag reads `CSUM` and the CRC is little-endian,
//! computed over the container bytes as stored. The one exception is the last container of a block
//! stored uncompressed (`IndxEntry::is_compressed` clear — every block of the Japanese PS3 disc
//! BLJM60101): its eight trailer bytes are zero, and it carries no checksum. On that disc every
//! last-in-block container has the zero trailer and every other container a valid `CSUM`.
//!
//! A descriptor whose `rel_off` is `0xFFFFFFFF` has no body. A script container's descriptors are
//! INFO, an optional DEPS and BINN; every one is returned raw, as stored.

use std::fs::File;
use std::path::Path;

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{load_ffcs_archive, Endian};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_SCRIPT;

use crate::Error;

/// One descriptor of a container: its tag (in reading order, `INFO` on every platform) and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub tag: [u8; 4],
    pub body: Vec<u8>,
}

/// One script container of a WAD.
#[derive(Clone, Debug)]
pub struct Script {
    /// The INDX block it sits in, and that block's path from PTHS.
    pub block: u16,
    pub block_path: String,
    /// Its position in the block's entry table.
    pub entry: usize,
    pub name_hash: u32,
    pub field_c: u32,
    /// The container's descriptors with a body, in table order.
    pub chunks: Vec<Chunk>,
}

impl Script {
    /// The body of the descriptor tagged `tag`.
    pub fn chunk(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.chunks.iter().find(|c| &c.tag == tag).map(|c| c.body.as_slice())
    }
}

/// The scripts of one WAD, and its byte order.
#[derive(Clone, Debug)]
pub struct WadScripts {
    pub endian: Endian,
    pub scripts: Vec<Script>,
}

fn u32_at(e: Endian, b: &[u8], off: usize, what: &str) -> Result<u32, Error> {
    if off + 4 > b.len() {
        return Err(Error::new(format!("{what}: u32 at 0x{off:x} runs past {} bytes", b.len())));
    }
    Ok(e.u32(b, off))
}

/// The trailer a container must end in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trailer {
    /// `CSUM` and the CRC of the container.
    Csum,
    /// Eight zero bytes: the last container of a block stored uncompressed.
    Zeroed,
}

/// Parse one script container, which must end in `trailer`.
pub fn parse_container(c: &[u8], e: Endian, trailer: Trailer, what: &str) -> Result<Vec<Chunk>, Error> {
    let magic: &[u8] = if e == Endian::Big { b"XFCU" } else { b"UCFX" };
    if c.len() < 28 || &c[0..4] != magic {
        return Err(Error::new(format!(
            "{what}: container of {} bytes starts with {:02x?}, not {}",
            c.len(),
            &c[..c.len().min(4)],
            String::from_utf8_lossy(magic)
        )));
    }
    let tail = &c[c.len() - 8..];
    match trailer {
        Trailer::Csum => {
            if &tail[0..4] != b"CSUM" {
                return Err(Error::new(format!(
                    "{what}: container of {} bytes does not end in a CSUM trailer (last 8 bytes {tail:02x?})",
                    c.len()
                )));
            }
            let stored = Endian::Little.u32(tail, 4);
            let computed = crc32_mercs2(&c[..c.len() - 8]);
            if stored != computed {
                return Err(Error::new(format!("{what}: CSUM 0x{stored:08x}, computed 0x{computed:08x}")));
            }
        }
        Trailer::Zeroed => {
            if tail.iter().any(|&b| b != 0) {
                return Err(Error::new(format!(
                    "{what}: the last container of a stored block ends in {tail:02x?}, not eight zero bytes"
                )));
            }
        }
    }
    let data_area_off = u32_at(e, c, 4, what)? as usize;
    if data_area_off == 0 {
        return Err(Error::new(format!("{what}: data_area_off is 0")));
    }
    let n_desc = u32_at(e, c, 16, what)? as usize;
    if 20 + n_desc * 20 > c.len() {
        return Err(Error::new(format!("{what}: {n_desc} descriptors run past the container")));
    }
    let mut out = Vec::new();
    for i in 0..n_desc {
        let row = 20 + i * 20;
        let mut tag = [c[row], c[row + 1], c[row + 2], c[row + 3]];
        if e == Endian::Big {
            tag.reverse();
        }
        let rel = e.u32(c, row + 4);
        let size = e.u32(c, row + 8) as usize;
        if rel == 0xFFFF_FFFF {
            continue;
        }
        let start = data_area_off + rel as usize;
        if start + size > c.len() - 8 {
            return Err(Error::new(format!(
                "{what}: descriptor {i} {} body 0x{start:x}+{size} runs past the container",
                String::from_utf8_lossy(&tag)
            )));
        }
        out.push(Chunk { tag, body: c[start..start + size].to_vec() });
    }
    Ok(out)
}

/// Every script container of the WAD at `path`, in block then entry order.
pub fn scripts(path: &Path) -> Result<WadScripts, Error> {
    let io = |e: std::io::Error| Error::new(format!("{}: {e}", path.display()));
    let mut f = File::open(path).map_err(io)?;
    let size = f.metadata().map_err(io)?.len();
    let arch = load_ffcs_archive(&mut f, size).map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
    if arch.paths.len() != arch.indx.len() {
        return Err(Error::new(format!(
            "{}: {} PTHS block paths for {} INDX blocks",
            path.display(),
            arch.paths.len(),
            arch.indx.len()
        )));
    }
    let e = arch.endian;
    let mut out = Vec::new();
    for bi in 0..arch.indx.len() {
        let what = format!("{} block {bi} ({})", path.display(), arch.paths[bi]);
        let block = decompress_block(&mut f, &arch.indx, bi as u16).map_err(|m| Error::new(format!("{what}: {m}")))?;
        let count = u32_at(e, &block, 0, &what)? as usize;
        let table_end = 4 + count * 16;
        if table_end > block.len() {
            return Err(Error::new(format!("{what}: {count} entries run past {} bytes", block.len())));
        }
        let mut pos = table_end;
        for i in 0..count {
            let row = 4 + i * 16;
            let name_hash = e.u32(&block, row);
            let type_hash = e.u32(&block, row + 4);
            let field_c = e.u32(&block, row + 8);
            let chunk_size = e.u32(&block, row + 12) as usize;
            if pos + chunk_size > block.len() {
                return Err(Error::new(format!("{what}: entry {i} runs past the block")));
            }
            if type_hash == TYPE_HASH_SCRIPT {
                let ewhat = format!("{what} entry {i} (0x{name_hash:08x})");
                let trailer = if !arch.indx[bi].is_compressed() && i + 1 == count {
                    Trailer::Zeroed
                } else {
                    Trailer::Csum
                };
                out.push(Script {
                    block: bi as u16,
                    block_path: arch.paths[bi].clone(),
                    entry: i,
                    name_hash,
                    field_c,
                    chunks: parse_container(&block[pos..pos + chunk_size], e, trailer, &ewhat)?,
                });
            }
            pos += chunk_size;
        }
    }
    Ok(WadScripts { endian: e, scripts: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(e: Endian, v: u32) -> [u8; 4] {
        if e == Endian::Big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    }

    /// A container in byte order `e` with an INFO body, an absent DEPS row and a BINN body, ending
    /// in `trailer`.
    fn container(e: Endian, trailer: Trailer) -> Vec<u8> {
        let rows: [(&[u8; 4], u32, u32); 3] = [(b"INFO", 0, 3), (b"DEPS", 0xFFFF_FFFF, 0), (b"BINN", 3, 4)];
        let data_area_off = 20 + rows.len() * 20;
        let mut c = Vec::new();
        c.extend_from_slice(if e == Endian::Big { b"XFCU" } else { b"UCFX" });
        c.extend_from_slice(&put(e, data_area_off as u32));
        c.extend_from_slice(&put(e, 0));
        c.extend_from_slice(&put(e, 0));
        c.extend_from_slice(&put(e, rows.len() as u32));
        for (tag, rel, size) in rows {
            let mut t = *tag;
            if e == Endian::Big {
                t.reverse();
            }
            c.extend_from_slice(&t);
            c.extend_from_slice(&put(e, rel));
            c.extend_from_slice(&put(e, size));
            c.extend_from_slice(&[0; 8]);
        }
        c.extend_from_slice(b"abc");
        c.extend_from_slice(b"\x1bLua");
        match trailer {
            Trailer::Csum => {
                let crc = crc32_mercs2(&c);
                c.extend_from_slice(b"CSUM");
                c.extend_from_slice(&crc.to_le_bytes());
            }
            Trailer::Zeroed => c.extend_from_slice(&[0; 8]),
        }
        c
    }

    #[test]
    fn containers_parse_in_both_byte_orders_with_either_trailer() {
        for e in [Endian::Little, Endian::Big] {
            for t in [Trailer::Csum, Trailer::Zeroed] {
                let chunks = parse_container(&container(e, t), e, t, "test").unwrap();
                assert_eq!(
                    chunks,
                    vec![
                        Chunk { tag: *b"INFO", body: b"abc".to_vec() },
                        Chunk { tag: *b"BINN", body: b"\x1bLua".to_vec() },
                    ],
                    "{e:?} {t:?}"
                );
            }
        }
    }

    #[test]
    fn a_bad_magic_trailer_or_checksum_is_an_error() {
        let le = container(Endian::Little, Trailer::Csum);
        let e = parse_container(&le, Endian::Big, Trailer::Csum, "t").unwrap_err().to_string();
        assert!(e.contains("not XFCU"), "{e}");
        let mut bad = le.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        let e = parse_container(&bad, Endian::Little, Trailer::Csum, "t").unwrap_err().to_string();
        assert!(e.contains("CSUM 0x"), "{e}");
        let e = parse_container(&container(Endian::Little, Trailer::Zeroed), Endian::Little, Trailer::Csum, "t")
            .unwrap_err()
            .to_string();
        assert!(e.contains("does not end in a CSUM trailer"), "{e}");
        let e = parse_container(&le, Endian::Little, Trailer::Zeroed, "t").unwrap_err().to_string();
        assert!(e.contains("not eight zero bytes"), "{e}");
    }
}
