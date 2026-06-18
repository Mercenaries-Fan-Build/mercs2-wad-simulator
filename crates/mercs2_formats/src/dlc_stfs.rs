//! STFS (Xbox 360 secure container) reader + RAR extraction.
//!
//! Port of the STFS half of `tools/x360_dlc_io.py` (py360-derived block
//! mapping). Walks the interleaved hash-table/data-block layout to read logical
//! file data (here: the DLC `.doh`). RAR extraction shells out to `UnRAR.exe`,
//! mirroring the ffmpeg/java subprocess pattern elsewhere in this workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dlc_input::SCFF_MAGIC;

const STFS_BLOCK_SIZE: usize = 0x1000; // 4 KB
const STFS_DATA_OFFSET: i64 = 0xC000;
const STFS_HASH_ENTRY_SIZE: usize = 0x18;
const STFS_BLOCKS_PER_L0: u64 = 0xAA; // 170
const STFS_BLOCKS_PER_L1: u64 = 0x70E4; // 28,900
const STFS_BLOCKS_PER_L2: u64 = 0x4AF768; // 4,913,000
const STFS_HASH_BLOCK_SENTINEL: u32 = 0xFFFFFF;

pub const STFS_MAGIC_LIVE: &[u8; 4] = b"LIVE";
pub const STFS_MAGIC_PIRS: &[u8; 4] = b"PIRS";
pub const STFS_MAGIC_CON: &[u8; 4] = b"CON ";

fn read_u32_be(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}
fn u24le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16
}

#[derive(Clone, Debug)]
pub struct FileTableEntry {
    pub name: String,
    pub is_dir: bool,
    pub consecutive: bool,
    pub valid_blocks: u32,
    pub alloc_blocks: u32,
    pub first_block: u32,
    pub path_ind: u16,
    pub file_size: u32,
}

/// LIVE/PIRS use 1 block per hash table (shift=0); CON uses 2 (shift=1).
fn detect_table_size_shift(stfs: &[u8]) -> u32 {
    if stfs.len() >= 0x344 && &stfs[0..4] == STFS_MAGIC_CON {
        let entry_id = read_u32_be(stfs, 0x340);
        if ((entry_id.wrapping_add(0xFFF)) & 0xF000) >> 0xC == 0xB {
            return 0;
        }
        return 1;
    }
    0
}

/// Logical data-block index → physical on-disk block index (skips hash tables).
fn fix_blocknum(logical: u32, shift: u32) -> u32 {
    let lb = logical as u64;
    let mut adjust: u64 = 0;
    if lb >= STFS_BLOCKS_PER_L0 {
        adjust += ((lb / STFS_BLOCKS_PER_L0) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L1 {
        adjust += ((lb / STFS_BLOCKS_PER_L1) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L2 {
        adjust += ((lb / STFS_BLOCKS_PER_L2) + 1) << shift;
    }
    (lb + adjust) as u32
}

/// Read `length` bytes from a physical block (signed: the first L0 table is at -1 → 0xB000).
fn read_physical_block(stfs: &[u8], phys: i64, length: usize) -> &[u8] {
    let offset = STFS_DATA_OFFSET + phys * STFS_BLOCK_SIZE as i64;
    if offset < 0 {
        return &[];
    }
    let start = offset as usize;
    if start >= stfs.len() {
        return &[];
    }
    let end = (start + length).min(stfs.len());
    &stfs[start..end]
}

/// Physical block of the L0 hash table covering `logical_block` (py360 spacing).
fn get_l0_hash_table_phys_block(logical: u32, shift: u32, table_offset: i64) -> i64 {
    // (blocks_per_table, l1_spacing, l2_spacing) for shift 0 and 1.
    let spacing: [(i64, i64, i64); 2] = [(0xAB, 0x718F, 0xFE7DA), (0xAC, 0x723A, 0xFD00B)];
    let lb = logical as i64;
    let mut table_num = (lb / STFS_BLOCKS_PER_L0 as i64) * spacing[shift as usize].0;
    if lb >= STFS_BLOCKS_PER_L0 as i64 {
        table_num += ((lb / STFS_BLOCKS_PER_L1 as i64) + 1) << shift;
    }
    if lb >= STFS_BLOCKS_PER_L1 as i64 {
        table_num += 1i64 << shift;
    }
    table_num += table_offset - (1i64 << shift);
    table_num
}

/// Read the hash record for `logical_block`: returns `(next_block, info_byte)`.
fn get_block_hash_entry(stfs: &[u8], logical: u32, shift: u32, table_offset: i64) -> (u32, u8) {
    let record = (logical as u64 % STFS_BLOCKS_PER_L0) as usize;
    let table_phys = get_l0_hash_table_phys_block(logical, shift, table_offset);
    let table_data = read_physical_block(stfs, table_phys, STFS_BLOCK_SIZE);
    let entry_start = record * STFS_HASH_ENTRY_SIZE;
    if table_data.len() < entry_start + STFS_HASH_ENTRY_SIZE {
        return (STFS_HASH_BLOCK_SENTINEL, 0);
    }
    let entry = &table_data[entry_start..entry_start + STFS_HASH_ENTRY_SIZE];
    let info = entry[0x14];
    let next_block = u32::from_be_bytes([0, entry[0x15], entry[0x16], entry[0x17]]);
    (next_block, info)
}

/// Volume descriptor @0x379 → (file_table_first_block, file_table_block_count, total_alloc).
fn parse_volume_descriptor(stfs: &[u8]) -> (u32, u16, u32) {
    let d = 0x379;
    let ft_block_count = u16::from_le_bytes([stfs[d + 3], stfs[d + 4]]);
    let ft_first_block = u24le(stfs, d + 5);
    let total_alloc = read_u32_be(stfs, d + 28);
    (ft_first_block, ft_block_count, total_alloc)
}

/// Walk the file-table block chain and parse 0x40-byte entries.
fn parse_stfs_file_table(stfs: &[u8], shift: u32) -> Vec<FileTableEntry> {
    let (ft_first, ft_count, _) = parse_volume_descriptor(stfs);

    let mut ft_data: Vec<u8> = Vec::new();
    let mut block = ft_first;
    for _ in 0..ft_count {
        if block == STFS_HASH_BLOCK_SENTINEL {
            break;
        }
        let phys = fix_blocknum(block, shift);
        ft_data.extend_from_slice(read_physical_block(stfs, phys as i64, STFS_BLOCK_SIZE));
        let (mut next, mut info) = get_block_hash_entry(stfs, block, shift, 0);
        if shift > 0 && info < 0x80 {
            let r = get_block_hash_entry(stfs, block, shift, 1);
            next = r.0;
            info = r.1;
        }
        let _ = info;
        block = next;
    }

    let mut entries = Vec::new();
    for i in 0..ft_data.len() / 0x40 {
        let off = i * 0x40;
        let name_raw = &ft_data[off..off + 0x28];
        if name_raw[0] == 0 {
            break;
        }
        let name_end = name_raw.iter().position(|&b| b == 0).unwrap_or(name_raw.len());
        let name = String::from_utf8_lossy(&name_raw[..name_end]).into_owned();
        let flags = ft_data[off + 0x28];
        entries.push(FileTableEntry {
            name,
            is_dir: flags & 0x80 != 0,
            consecutive: flags & 0x40 != 0,
            valid_blocks: u24le(&ft_data, off + 0x29),
            alloc_blocks: u24le(&ft_data, off + 0x2C),
            first_block: u24le(&ft_data, off + 0x2F),
            path_ind: u16::from_be_bytes([ft_data[off + 0x32], ft_data[off + 0x33]]),
            file_size: read_u32_be(&ft_data, off + 0x34),
        });
    }
    entries
}

/// Hash-block-aware reader for file data inside an STFS container.
pub struct StfsReader {
    pub stfs_data: Vec<u8>,
    pub table_size_shift: u32,
    pub file_table: Vec<FileTableEntry>,
}

impl StfsReader {
    pub fn new(stfs_data: Vec<u8>) -> Self {
        let table_size_shift = detect_table_size_shift(&stfs_data);
        let file_table = parse_stfs_file_table(&stfs_data, table_size_shift);
        Self {
            stfs_data,
            table_size_shift,
            file_table,
        }
    }

    /// The DOH file-table entry (first whose name contains "doh", case-insensitive).
    pub fn doh_entry(&self) -> Option<&FileTableEntry> {
        self.file_table
            .iter()
            .find(|e| e.name.to_lowercase().contains("doh"))
    }

    /// Build the ordered list of logical block numbers for a file by walking its chain.
    fn build_chain(&self, first_block: u32, alloc_blocks: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut block = first_block;
        let max_blocks = alloc_blocks as u64 + 10; // small safety margin
        for _ in 0..max_blocks {
            if block == STFS_HASH_BLOCK_SENTINEL {
                break;
            }
            chain.push(block);
            let (mut next, mut info) = get_block_hash_entry(&self.stfs_data, block, self.table_size_shift, 0);
            if self.table_size_shift > 0 && info < 0x80 {
                let r = get_block_hash_entry(&self.stfs_data, block, self.table_size_shift, 1);
                next = r.0;
                info = r.1;
            }
            let _ = info;
            block = next;
        }
        chain
    }

    fn read_with_chain(&self, chain: &[u32], doh_offset: usize, length: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(length);
        let mut remaining = length;
        let mut chain_idx = doh_offset / STFS_BLOCK_SIZE;
        let mut skip = doh_offset % STFS_BLOCK_SIZE;

        while remaining > 0 {
            if chain_idx >= chain.len() {
                return Err(format!(
                    "DOH chain index {chain_idx} exceeds chain length ({})",
                    chain.len()
                ));
            }
            let logical = chain[chain_idx];
            let phys = fix_blocknum(logical, self.table_size_shift) as i64;
            let abs = STFS_DATA_OFFSET + phys * STFS_BLOCK_SIZE as i64 + skip as i64;
            let chunk = (STFS_BLOCK_SIZE - skip).min(remaining);
            let start = abs as usize;
            let end = start + chunk;
            if end > self.stfs_data.len() {
                return Err(format!(
                    "STFS read past EOF: chain[{chain_idx}] logical {logical} phys {phys} offset 0x{abs:X}"
                ));
            }
            out.extend_from_slice(&self.stfs_data[start..end]);
            remaining -= chunk;
            chain_idx += 1;
            skip = 0;
        }
        Ok(out)
    }

    /// Read the full DOH file (walks its hash chain). Equivalent to Python `reader.read(0, doh_size)`.
    pub fn read_doh(&self) -> Result<Vec<u8>, String> {
        let entry = self.doh_entry().ok_or("No DOH file found in STFS file table")?;
        let chain = self.build_chain(entry.first_block, entry.alloc_blocks);
        self.read_with_chain(&chain, 0, entry.file_size as usize)
    }
}

fn find_unrar() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("UNRAR") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    // tools/unrar/UnRAR.exe relative to this crate.
    let rel = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../unrar/UnRAR.exe");
    if rel.exists() {
        return Some(rel);
    }
    None
}

/// Extract an STFS container from a RAR archive and return a reader.
///
/// Uses `UnRAR.exe` when available (Windows), else `bsdtar`/`tar`. Picks the
/// largest extracted file whose magic is LIVE/PIRS/CON.
pub fn extract_stfs_from_rar(rar_path: &Path, work_dir: &Path) -> Result<StfsReader, String> {
    let stfs_dir = work_dir.join("stfs");
    std::fs::create_dir_all(&stfs_dir).map_err(|e| format!("mkdir: {e}"))?;

    let status = if let Some(unrar) = find_unrar() {
        Command::new(unrar)
            .arg("x")
            .arg("-o+")
            .arg("-y")
            .arg(rar_path)
            .arg(format!("{}\\", stfs_dir.display()))
            .output()
            .map_err(|e| format!("UnRAR spawn failed: {e}"))?
    } else {
        Command::new("bsdtar")
            .arg("-xf")
            .arg(rar_path)
            .arg("-C")
            .arg(&stfs_dir)
            .output()
            .map_err(|e| format!("bsdtar spawn failed: {e}"))?
    };
    if !status.status.success() {
        return Err(format!(
            "archive extraction failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    // Largest file with an STFS magic.
    let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
    collect_files(&stfs_dir, &mut candidates);
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (size, path) in candidates {
        if size < 1_000_000 {
            continue;
        }
        let mut magic = [0u8; 4];
        if let Ok(mut f) = std::fs::File::open(&path) {
            use std::io::Read;
            if f.read_exact(&mut magic).is_ok()
                && (&magic == STFS_MAGIC_CON || &magic == STFS_MAGIC_LIVE || &magic == STFS_MAGIC_PIRS)
            {
                let data = std::fs::read(&path).map_err(|e| format!("read stfs: {e}"))?;
                return Ok(StfsReader::new(data));
            }
        }
    }
    Err("Could not find STFS container in archive".into())
}

fn collect_files(dir: &Path, out: &mut Vec<(u64, PathBuf)>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_files(&p, out);
            } else if let Ok(m) = e.metadata() {
                out.push((m.len(), p));
            }
        }
    }
}

/// Load an STFS container or a raw DOH file → (doh_bytes, source_type).
pub fn load_stfs_or_doh(path: &Path) -> Result<(Vec<u8>, &'static str), String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let magic = &data[..4.min(data.len())];
    if magic == STFS_MAGIC_LIVE || magic == STFS_MAGIC_PIRS || magic == STFS_MAGIC_CON {
        let reader = StfsReader::new(data);
        Ok((reader.read_doh()?, "stfs"))
    } else if magic == SCFF_MAGIC {
        Ok((data, "doh"))
    } else {
        Err(format!("Unknown file format: {magic:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_blocknum_skips_hash_tables() {
        // No adjustment below the first L0 boundary.
        assert_eq!(fix_blocknum(0, 0), 0);
        assert_eq!(fix_blocknum(169, 0), 169);
        // At block 170 (0xAA) the formula inserts (170//170)+1 = 2 hash blocks.
        assert_eq!(fix_blocknum(170, 0), 172);
    }

    #[test]
    fn first_l0_table_is_block_minus_one() {
        // The L0 hash table for the first group sits at physical block -1 (offset 0xB000).
        assert_eq!(get_l0_hash_table_phys_block(0, 0, 0), -1);
        assert_eq!(get_l0_hash_table_phys_block(5, 0, 0), -1);
    }

    #[test]
    fn real_stfs_read_doh_matches_python() {
        // Cross-check the full STFS reader against Python's output on the real
        // container. Reads a cached raw STFS and asserts the recovered DOH equals
        // the cached Python-produced dlc01.doh. Skips if either is absent.
        let stfs_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../output/_scratch/dlc01.stfs");
        let doh_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../output/_scratch/dlc01.doh");
        let (stfs, doh_golden) = match (std::fs::read(stfs_path), std::fs::read(doh_path)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                eprintln!("SKIP real_stfs_read_doh_matches_python: cached STFS/DOH not present");
                return;
            }
        };
        let reader = StfsReader::new(stfs);
        let doh = reader.read_doh().expect("read_doh");
        assert_eq!(doh.len(), doh_golden.len(), "DOH length mismatch");
        assert!(doh == doh_golden, "DOH bytes differ from Python golden");
    }
}
