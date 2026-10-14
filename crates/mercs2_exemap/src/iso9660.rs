//! Read one file out of an ISO-9660 disc image (the PS3 disc images) through its Joliet tree.
//!
//! The volume descriptor set starts at sector 16; each descriptor opens with its type byte and
//! `CD001`, and the set ends at type 255. The Joliet tree is the one a supplementary descriptor
//! (type 2) roots when its escape sequences at `+0x58` name UCS-2 (`%/@`, `%/C` or `%/E`); its names
//! are UCS-2 big-endian. An image without one is refused. The primary tree is not read. In the
//! descriptor, `+0x80` holds the logical block size (both-endian `u16`) and `+0x9c` the root directory record.
//! A directory record:
//!
//! ```text
//! +0x00 u8  record length (0 = no more records in this sector)
//! +0x02 u32 extent block (little-endian half of a both-endian pair)
//! +0x0a u32 data length  (little-endian half)
//! +0x19 u8  flags (0x02 = directory, 0x80 = not the file's final extent)
//! +0x20 u8  name length
//! +0x21     name (one byte `\0` = this directory, `\x01` = its parent; otherwise UCS-2BE,
//!           files may end in `;<version>`)
//! ```
//!
//! Records never cross a sector boundary. A file larger than one extent allows is a run of
//! consecutive records with the same name, exactly one of them without flag 0x80: that one is the
//! final extent, and the flagged ones are the extents before it, in table order. ECMA-119 puts the
//! final record last; the Joliet tree of the catalogue's PS3 disc image puts it first (`VZ.WAD`:
//! the final extent at block `0x1061b1`, then the flagged ones at `0x61b3` and `0x861b2`, which
//! are contiguous on the disc in that order). Names match case-insensitively, without the
//! `;<version>` suffix.

use std::io::{Read, Seek, SeekFrom};

use crate::Error;

const DESCRIPTORS_START: u64 = 16 * 0x800;
const FLAG_DIR: u8 = 0x02;
const FLAG_MORE: u8 = 0x80;

fn io(what: &str, e: std::io::Error) -> Error {
    Error::new(format!("ISO-9660 {what}: {e}"))
}

fn read_at<R: Read + Seek>(r: &mut R, off: u64, len: usize, what: &str) -> Result<Vec<u8>, Error> {
    r.seek(SeekFrom::Start(off)).map_err(|e| io(what, e))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).map_err(|e| io(what, e))?;
    Ok(buf)
}

/// One directory record.
struct Record {
    name: String,
    block: u32,
    size: u32,
    flags: u8,
}

fn record(rec: &[u8]) -> Result<Record, Error> {
    if rec.len() < 0x21 {
        return Err(Error::new(format!("ISO-9660 directory record of {} bytes is shorter than its fixed part", rec.len())));
    }
    let nlen = rec[0x20] as usize;
    let name = rec
        .get(0x21..0x21 + nlen)
        .ok_or_else(|| Error::new("ISO-9660 directory record name runs past the record"))?;
    let name = if nlen == 1 {
        String::from_utf8_lossy(name).into_owned()
    } else {
        if nlen % 2 != 0 {
            return Err(Error::new(format!("ISO-9660 Joliet name of {nlen} bytes is not UCS-2")));
        }
        let units: Vec<u16> = name.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).map_err(|e| Error::new(format!("ISO-9660 Joliet name: {e}")))?
    };
    let name = name.split_once(';').map_or(&*name, |(n, _)| n).to_string();
    Ok(Record {
        name,
        block: u32::from_le_bytes(rec[2..6].try_into().unwrap()),
        size: u32::from_le_bytes(rec[10..14].try_into().unwrap()),
        flags: rec[0x19],
    })
}

/// The records of one directory table, in table order, without `.` and `..`.
fn records(table: &[u8], block: usize) -> Result<Vec<Record>, Error> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < table.len() {
        let len = table[off] as usize;
        if len == 0 {
            off = (off / block + 1) * block;
            continue;
        }
        if off % block + len > block {
            return Err(Error::new(format!("ISO-9660 directory record at 0x{off:x} crosses a block boundary")));
        }
        let r = record(&table[off..off + len])?;
        if !(r.name == "\0" || r.name == "\x01") {
            out.push(r);
        }
        off += len;
    }
    Ok(out)
}

/// The logical block size and the root directory record of the Joliet supplementary descriptor.
fn joliet<R: Read + Seek>(r: &mut R) -> Result<(u64, Record), Error> {
    let mut at = DESCRIPTORS_START;
    loop {
        let d = read_at(r, at, 0x800, "volume descriptor")?;
        if &d[1..6] != b"CD001" {
            return Err(Error::new(format!("ISO-9660: no volume descriptor at 0x{at:x}")));
        }
        match d[0] {
            2 if matches!(&d[0x58..0x5b], b"%/@" | b"%/C" | b"%/E") => {
                let block = u16::from_le_bytes([d[0x80], d[0x81]]) as u64;
                if block == 0 {
                    return Err(Error::new("ISO-9660: the Joliet descriptor's block size is 0"));
                }
                return Ok((block, record(&d[0x9c..0x9c + 34])?));
            }
            255 => return Err(Error::new("ISO-9660: the descriptor set ends without a Joliet descriptor")),
            _ => at += 0x800,
        }
    }
}

/// The extents of the file at `path` (components separated by `/`, matched case-insensitively) in
/// the disc image `r`, in order: (absolute byte offset, size).
pub fn locate<R: Read + Seek>(r: &mut R, path: &str) -> Result<Vec<(u64, u64)>, Error> {
    let (block, root) = joliet(r)?;
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err(Error::new("ISO-9660 path is empty"));
    }
    let mut dir = root;
    for (i, part) in parts.iter().enumerate() {
        let table = read_at(r, dir.block as u64 * block, dir.size as usize, "directory table")?;
        let recs = records(&table, block as usize)?;
        let first = recs
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(part))
            .ok_or_else(|| Error::new(format!("ISO-9660: {path}: no entry {part:?}")))?;
        let last = i + 1 == parts.len();
        let is_dir = recs[first].flags & FLAG_DIR != 0;
        if last && is_dir {
            return Err(Error::new(format!("ISO-9660: {path} is a directory")));
        }
        if !last && !is_dir {
            return Err(Error::new(format!("ISO-9660: {path}: {part:?} is a file, not a directory")));
        }
        if !last {
            let d = &recs[first];
            dir = Record { name: d.name.clone(), block: d.block, size: d.size, flags: d.flags };
            continue;
        }
        let run: Vec<&Record> = recs[first..].iter().take_while(|e| e.name.eq_ignore_ascii_case(part)).collect();
        if recs[first + run.len()..].iter().any(|e| e.name.eq_ignore_ascii_case(part)) {
            return Err(Error::new(format!("ISO-9660: {path}: the records named {part:?} are not consecutive")));
        }
        let finals: Vec<&&Record> = run.iter().filter(|e| e.flags & FLAG_MORE == 0).collect();
        if finals.len() != 1 {
            return Err(Error::new(format!(
                "ISO-9660: {path}: {} of its {} extent records are marked final, not exactly one",
                finals.len(),
                run.len()
            )));
        }
        let extent = |e: &Record| (e.block as u64 * block, e.size as u64);
        let mut extents: Vec<(u64, u64)> = run.iter().filter(|e| e.flags & FLAG_MORE != 0).map(|e| extent(e)).collect();
        extents.push(extent(finals[0]));
        return Ok(extents);
    }
    unreachable!("the loop returns on the last path component")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const B: usize = 0x800;

    fn rec(block: u32, size: u32, flags: u8, name: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; 0x21];
        r[2..6].copy_from_slice(&block.to_le_bytes());
        r[6..10].copy_from_slice(&block.to_be_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[14..18].copy_from_slice(&size.to_be_bytes());
        r[0x19] = flags;
        r[0x20] = name.len() as u8;
        r.extend_from_slice(name);
        if r.len() % 2 == 1 {
            r.push(0);
        }
        r[0] = r.len() as u8;
        r
    }

    /// `s` as a Joliet (UCS-2BE) name.
    fn j(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
    }

    fn descriptor(d: &mut [u8], sector: usize, kind: u8, escape: &[u8], root_block: u32) {
        let at = sector * B;
        d[at] = kind;
        d[at + 1..at + 6].copy_from_slice(b"CD001");
        d[at + 0x58..at + 0x58 + escape.len()].copy_from_slice(escape);
        d[at + 0x80..at + 0x82].copy_from_slice(&(B as u16).to_le_bytes());
        let root = rec(root_block, B as u32, FLAG_DIR, b"\0");
        d[at + 0x9c..at + 0x9c + root.len()].copy_from_slice(&root);
    }

    /// A primary descriptor (sector 16) whose root at block 19 holds a decoy `A.BIN;1`, and a
    /// Joliet descriptor (sector 17) whose root at block 20 holds `A.bin;1` (one extent) and dir
    /// `Sub`; `Sub` at block 21 holds `Big.wad;1` in two extents (blocks 22 and 23), with the
    /// records `sub` gives.
    fn disc_with(sub: &[Vec<u8>]) -> Vec<u8> {
        let mut d = vec![0u8; 26 * B];
        descriptor(&mut d, 16, 1, b"", 19);
        descriptor(&mut d, 17, 2, b"%/E", 20);
        d[18 * B] = 255;
        d[18 * B + 1..18 * B + 6].copy_from_slice(b"CD001");
        let p = rec(25, 5, 0, b"A.BIN;1");
        d[19 * B..19 * B + p.len()].copy_from_slice(&p);
        let mut t = Vec::new();
        t.extend(rec(20, B as u32, FLAG_DIR, b"\0"));
        t.extend(rec(20, B as u32, FLAG_DIR, b"\x01"));
        t.extend(rec(24, 3, 0, &j("A.bin;1")));
        t.extend(rec(21, B as u32, FLAG_DIR, &j("Sub")));
        d[20 * B..20 * B + t.len()].copy_from_slice(&t);
        let mut s = Vec::new();
        s.extend(rec(21, B as u32, FLAG_DIR, b"\0"));
        s.extend(rec(20, B as u32, FLAG_DIR, b"\x01"));
        for r in sub {
            s.extend_from_slice(r);
        }
        d[21 * B..21 * B + s.len()].copy_from_slice(&s);
        d[24 * B..24 * B + 3].copy_from_slice(b"abc");
        d
    }

    fn more() -> Vec<u8> {
        rec(22, B as u32, FLAG_MORE, &j("Big.wad;1"))
    }

    fn last() -> Vec<u8> {
        rec(23, 4, 0, &j("Big.wad;1"))
    }

    fn disc() -> Vec<u8> {
        disc_with(&[more(), last()])
    }

    #[test]
    fn finds_files_through_the_joliet_tree_and_joins_multi_extent_runs() {
        let mut c = Cursor::new(disc());
        assert_eq!(locate(&mut c, "a.bin").unwrap(), vec![(24 * B as u64, 3)]);
        let big = vec![(22 * B as u64, B as u64), (23 * B as u64, 4)];
        assert_eq!(locate(&mut c, "SUB/BIG.WAD").unwrap(), big);
        // The final extent's record listed first, as in the catalogue's PS3 disc image.
        assert_eq!(locate(&mut Cursor::new(disc_with(&[last(), more()])), "sub/big.wad").unwrap(), big);
    }

    #[test]
    fn missing_entries_kind_mismatches_and_broken_runs_are_errors() {
        let mut c = Cursor::new(disc());
        assert!(locate(&mut c, "nope").unwrap_err().to_string().contains("no entry"));
        assert!(locate(&mut c, "sub").unwrap_err().to_string().contains("is a directory"));
        assert!(locate(&mut c, "a.bin/x").unwrap_err().to_string().contains("not a directory"));
        let other = rec(25, 1, 0, &j("Other;1"));
        let e = locate(&mut Cursor::new(disc_with(&[more(), other, last()])), "sub/big.wad").unwrap_err().to_string();
        assert!(e.contains("not consecutive"), "{e}");
        let e = locate(&mut Cursor::new(disc_with(&[more(), more()])), "sub/big.wad").unwrap_err().to_string();
        assert!(e.contains("0 of its 2 extent records are marked final"), "{e}");
        let e = locate(&mut Cursor::new(disc_with(&[last(), last()])), "sub/big.wad").unwrap_err().to_string();
        assert!(e.contains("2 of its 2 extent records are marked final"), "{e}");
    }

    #[test]
    fn an_image_without_a_joliet_tree_is_refused() {
        let e = locate(&mut Cursor::new(vec![0u8; 20 * B]), "a").unwrap_err().to_string();
        assert!(e.contains("no volume descriptor"), "{e}");
        let mut d = disc();
        d[17 * B + 0x58..17 * B + 0x5b].copy_from_slice(b"\0\0\0");
        let e = locate(&mut Cursor::new(d), "a.bin").unwrap_err().to_string();
        assert!(e.contains("without a Joliet descriptor"), "{e}");
    }
}
