//! Read one file out of an Xbox / Xbox 360 disc image (XDVDFS).
//!
//! The volume descriptor (`MICROSOFT*XBOX*MEDIA`) sits at `partition + 0x10000`; the partition
//! starts at one of the offsets the disc formats use (`0`, `0x2080000`, `0xFD90000`,
//! `0x18300000`). After the 20-byte magic come the root directory's `u32` sector and `u32` size
//! (little-endian). A directory is a binary tree of 4-byte-aligned entries:
//!
//! ```text
//! +0x00 u16 left   (entry offset / 4; 0 or 0xFFFF = none)
//! +0x02 u16 right
//! +0x04 u32 start sector
//! +0x08 u32 size (bytes)
//! +0x0c u8  attributes (0x10 = directory)
//! +0x0d u8  name length
//! +0x0e     name (ASCII)
//! ```
//!
//! Sectors are 2048 bytes, relative to the partition start. Every entry the tree reaches must lie
//! inside its table; anything else is an error.

use std::io::{Read, Seek, SeekFrom};

use crate::Error;

const SECTOR: u64 = 0x800;
const MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
const PARTITIONS: [u64; 4] = [0x0, 0x208_0000, 0xFD9_0000, 0x1830_0000];
const ATTR_DIR: u8 = 0x10;

fn io(what: &str, e: std::io::Error) -> Error {
    Error::new(format!("XDVDFS {what}: {e}"))
}

fn read_at<R: Read + Seek>(r: &mut R, off: u64, len: usize, what: &str) -> Result<Vec<u8>, Error> {
    r.seek(SeekFrom::Start(off)).map_err(|e| io(what, e))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).map_err(|e| io(what, e))?;
    Ok(buf)
}

/// The partition start whose volume descriptor carries the XDVDFS magic.
fn partition<R: Read + Seek>(r: &mut R) -> Result<u64, Error> {
    for base in PARTITIONS {
        r.seek(SeekFrom::Start(base + 0x10000)).map_err(|e| io("seek", e))?;
        let mut magic = [0u8; 20];
        if r.read_exact(&mut magic).is_ok() && &magic == MAGIC {
            return Ok(base);
        }
    }
    Err(Error::new("no XDVDFS volume descriptor at any known partition offset"))
}

/// One directory entry.
struct Entry {
    name: String,
    sector: u32,
    size: u32,
    is_dir: bool,
}

/// Every entry reachable through one directory table's tree.
fn entries(table: &[u8]) -> Result<Vec<Entry>, Error> {
    let mut out = Vec::new();
    let mut stack = vec![0usize];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(off) = stack.pop() {
        if !seen.insert(off) {
            continue;
        }
        let head = table
            .get(off..off + 14)
            .ok_or_else(|| Error::new(format!("XDVDFS directory entry at 0x{off:x} runs past its {}-byte table", table.len())))?;
        let left = u16::from_le_bytes([head[0], head[1]]);
        let right = u16::from_le_bytes([head[2], head[3]]);
        let sector = u32::from_le_bytes(head[4..8].try_into().unwrap());
        let size = u32::from_le_bytes(head[8..12].try_into().unwrap());
        let attr = head[12];
        let nlen = head[13] as usize;
        let name = table
            .get(off + 14..off + 14 + nlen)
            .ok_or_else(|| Error::new(format!("XDVDFS entry name at 0x{off:x} runs past its table")))?;
        out.push(Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            sector,
            size,
            is_dir: attr & ATTR_DIR != 0,
        });
        for child in [left, right] {
            if child != 0 && child != 0xFFFF {
                stack.push(child as usize * 4);
            }
        }
    }
    Ok(out)
}

/// The bytes of the file at `path` (components separated by `/`, matched case-insensitively as
/// XDVDFS does) in the disc image `r`.
pub fn read_file<R: Read + Seek>(r: &mut R, path: &str) -> Result<Vec<u8>, Error> {
    let base = partition(r)?;
    let root = read_at(r, base + 0x10000 + 0x14, 8, "root directory")?;
    let mut sector = u32::from_le_bytes(root[0..4].try_into().unwrap());
    let mut size = u32::from_le_bytes(root[4..8].try_into().unwrap());
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err(Error::new("XDVDFS path is empty"));
    }
    for (i, part) in parts.iter().enumerate() {
        let table = read_at(r, base + sector as u64 * SECTOR, size as usize, "directory table")?;
        let found = entries(&table)?
            .into_iter()
            .find(|e| e.name.eq_ignore_ascii_case(part))
            .ok_or_else(|| Error::new(format!("XDVDFS: {path}: no entry {part:?}")))?;
        let last = i + 1 == parts.len();
        if last && found.is_dir {
            return Err(Error::new(format!("XDVDFS: {path} is a directory")));
        }
        if !last && !found.is_dir {
            return Err(Error::new(format!("XDVDFS: {path}: {part:?} is a file, not a directory")));
        }
        sector = found.sector;
        size = found.size;
    }
    read_at(r, base + sector as u64 * SECTOR, size as usize, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn entry(left: u16, right: u16, sector: u32, size: u32, attr: u8, name: &str) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&left.to_le_bytes());
        e.extend_from_slice(&right.to_le_bytes());
        e.extend_from_slice(&sector.to_le_bytes());
        e.extend_from_slice(&size.to_le_bytes());
        e.push(attr);
        e.push(name.len() as u8);
        e.extend_from_slice(name.as_bytes());
        while e.len() % 4 != 0 {
            e.push(0xFF);
        }
        e
    }

    /// A disc with partition 0: root at sector 0x30 holding `a.xex` (left child) and dir `sub`
    /// (root), `sub` at sector 0x31 holding `b.bin`.
    fn disc() -> Vec<u8> {
        let mut d = vec![0u8; 0x40 * SECTOR as usize];
        d[0x10000..0x10014].copy_from_slice(MAGIC);
        let sub_entry = entry(0, 0, 0x31, 0, ATTR_DIR, "sub");
        let left_off = sub_entry.len();
        let mut root = entry((left_off / 4) as u16, 0, 0x31, 0, ATTR_DIR, "sub");
        root.extend(entry(0, 0, 0x32, 5, 0, "a.xex"));
        let sub = entry(0, 0, 0x33, 3, 0, "b.bin");
        let root_len = root.len() as u32;
        let sub_len = sub.len() as u32;
        // Patch the directory sizes now that the tables are laid out.
        root[8..12].copy_from_slice(&sub_len.to_le_bytes());
        d[0x10014..0x10018].copy_from_slice(&0x30u32.to_le_bytes());
        d[0x10018..0x1001c].copy_from_slice(&root_len.to_le_bytes());
        let at = |s: usize| s * SECTOR as usize;
        d[at(0x30)..at(0x30) + root.len()].copy_from_slice(&root);
        d[at(0x31)..at(0x31) + sub.len()].copy_from_slice(&sub);
        d[at(0x32)..at(0x32) + 5].copy_from_slice(b"XEX2!");
        d[at(0x33)..at(0x33) + 3].copy_from_slice(b"abc");
        d
    }

    #[test]
    fn reads_files_through_the_tree_and_subdirectories() {
        let mut c = Cursor::new(disc());
        assert_eq!(read_file(&mut c, "a.xex").unwrap(), b"XEX2!");
        assert_eq!(read_file(&mut c, "A.XEX").unwrap(), b"XEX2!");
        assert_eq!(read_file(&mut c, "sub/b.bin").unwrap(), b"abc");
    }

    #[test]
    fn missing_entries_and_kind_mismatches_are_errors() {
        let mut c = Cursor::new(disc());
        assert!(read_file(&mut c, "nope.xex").unwrap_err().to_string().contains("no entry"));
        assert!(read_file(&mut c, "sub").unwrap_err().to_string().contains("is a directory"));
        assert!(read_file(&mut c, "a.xex/x").unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn a_disc_without_the_magic_is_refused() {
        let mut c = Cursor::new(vec![0u8; 0x20000]);
        assert!(read_file(&mut c, "a.xex").unwrap_err().to_string().contains("no XDVDFS volume descriptor"));
    }
}
