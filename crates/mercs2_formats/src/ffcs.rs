//! FFCS WAD header, INDX, ASET, PTHS parsing.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub const PAGE_SIZE: u64 = 0x8000;
pub const FFCS_HEADER_SIZE: usize = 0x100;

#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub tag: [u8; 4],
    pub offset: u32,
    pub meta: u32,
}

#[derive(Debug, Clone)]
pub struct IndxEntry {
    pub page_index: u32,
    pub packed_field: u32,
    pub flags_and_page_count: u32,
}

impl IndxEntry {
    pub fn decompressed_page_count(&self) -> u32 {
        self.packed_field & 0x00FFFFFF
    }

    pub fn compressed_page_count(&self) -> u32 {
        self.flags_and_page_count & 0xFFFF
    }
}

#[derive(Debug, Clone)]
pub struct AsetEntry {
    pub asset_hash: u32,
    pub secondary_ref: u32,
    pub packed_block_ref: u32,
    pub type_id: u32,
}

impl AsetEntry {
    pub fn block_index(&self) -> u16 {
        (self.packed_block_ref >> 16) as u16
    }

    pub fn sub_entry(&self) -> u16 {
        (self.packed_block_ref & 0xFFFF) as u16
    }

    pub fn is_primary(&self) -> bool {
        self.sub_entry() == 0xFFFF
    }
}

#[derive(Debug, Clone)]
pub struct FfcsArchive {
    pub chunks: Vec<ChunkRow>,
    pub indx: Vec<IndxEntry>,
    pub aset: Vec<AsetEntry>,
    pub paths: Vec<String>,
}

pub fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

pub fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

pub fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

pub fn read_u16_be(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

pub fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32_le(data, offset))
}

pub fn read_f32_be(data: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32_be(data, offset))
}

pub fn parse_ffcs_header(header: &[u8; FFCS_HEADER_SIZE]) -> Result<Vec<ChunkRow>, String> {
    if &header[0..4] != b"FFCS" {
        return Err(format!("Bad magic: {:?}", &header[0..4]));
    }
    let version = read_u32_le(header, 4);
    if version != 2 {
        return Err(format!("Unexpected version: {version}"));
    }
    let chunk_count = read_u32_le(header, 8);
    let row_count = chunk_count.min(5) as usize;
    let mut rows = Vec::new();
    for i in 0..row_count {
        let base = 0x0C + i * 12;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&header[base..base + 4]);
        rows.push(ChunkRow {
            tag,
            offset: read_u32_le(header, base + 4),
            meta: read_u32_le(header, base + 8),
        });
    }
    Ok(rows)
}

pub fn find_chunk<'a>(rows: &'a [ChunkRow], tag: &[u8; 4]) -> Option<&'a ChunkRow> {
    rows.iter().find(|r| &r.tag == tag)
}

pub fn parse_indx_entries(file: &mut File, row: &ChunkRow) -> io::Result<Vec<IndxEntry>> {
    let count = row.meta as usize;
    file.seek(SeekFrom::Start(row.offset as u64))?;
    let mut buf = vec![0u8; count * 12];
    file.read_exact(&mut buf)?;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let base = i * 12;
        entries.push(IndxEntry {
            page_index: read_u32_le(&buf, base),
            packed_field: read_u32_le(&buf, base + 4),
            flags_and_page_count: read_u32_le(&buf, base + 8),
        });
    }
    Ok(entries)
}

pub fn parse_aset_entries(file: &mut File, row: &ChunkRow) -> io::Result<Vec<AsetEntry>> {
    let count = row.meta as usize;
    file.seek(SeekFrom::Start(row.offset as u64))?;
    let mut buf = vec![0u8; count * 16];
    file.read_exact(&mut buf)?;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let base = i * 16;
        entries.push(AsetEntry {
            asset_hash: read_u32_le(&buf, base),
            secondary_ref: read_u32_le(&buf, base + 4),
            packed_block_ref: read_u32_le(&buf, base + 8),
            type_id: read_u32_le(&buf, base + 12),
        });
    }
    Ok(entries)
}

/// PTHS: null-separated path strings until mandatory trailer marker region.
pub fn parse_pths(file: &mut File, row: &ChunkRow, file_size: u64) -> io::Result<Vec<String>> {
    let start = row.offset as u64;
    let count = row.meta as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    // Read until next chunk or reasonable cap
    let _end = file_size;
    file.seek(SeekFrom::Start(start))?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..n]);
        if raw.len() > 16 * 1024 * 1024 {
            break;
        }
    }
    let mut paths = Vec::new();
    let mut pos = 0usize;
    for _ in 0..count {
        if pos >= raw.len() {
            break;
        }
        let nul = raw[pos..].iter().position(|&b| b == 0);
        match nul {
            Some(0) => {
                pos += 1;
            }
            Some(n) => {
                let s = String::from_utf8_lossy(&raw[pos..pos + n]).to_string();
                if !s.is_empty() && (s.contains('\\') || s.contains('/')) {
                    paths.push(s);
                }
                pos += n + 1;
            }
            None => break,
        }
    }
    Ok(paths)
}

pub fn load_ffcs_archive(file: &mut File, file_size: u64) -> Result<FfcsArchive, Box<dyn std::error::Error>> {
    let mut header = [0u8; FFCS_HEADER_SIZE];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut header)?;
    let chunks = parse_ffcs_header(&header)?;
    let indx_row = find_chunk(&chunks, b"INDX").ok_or("Missing INDX")?;
    let aset_row = find_chunk(&chunks, b"ASET").ok_or("Missing ASET")?;
    let indx = parse_indx_entries(file, indx_row)?;
    let aset = parse_aset_entries(file, aset_row)?;
    let paths = if let Some(pths) = find_chunk(&chunks, b"PTHS") {
        parse_pths(file, pths, file_size).unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(FfcsArchive {
        chunks,
        indx,
        aset,
        paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u32_le_basic() {
        let data = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(read_u32_le(&data, 0), 0x12345678);
    }

    #[test]
    fn read_u32_be_basic() {
        let data = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(read_u32_be(&data, 0), 0x12345678);
    }

    #[test]
    fn read_u16_le_basic() {
        let data = [0x34, 0x12];
        assert_eq!(read_u16_le(&data, 0), 0x1234);
    }

    #[test]
    fn read_u16_be_basic() {
        let data = [0x12, 0x34];
        assert_eq!(read_u16_be(&data, 0), 0x1234);
    }

    #[test]
    fn read_f32_le_from_zero() {
        let bits = 0u32;
        assert_eq!(read_f32_le(&bits.to_le_bytes(), 0), 0.0);
    }

    #[test]
    fn read_f32_le_ten() {
        let bits = 0x41200000u32; // 10.0 in IEEE 754
        let f = read_f32_le(&bits.to_le_bytes(), 0);
        assert!((f - 10.0).abs() < 0.001);
    }

    #[test]
    fn read_f32_be_ten() {
        let bits = 0x41200000u32; // 10.0 in IEEE 754
        let f = read_f32_be(&bits.to_be_bytes(), 0);
        assert!((f - 10.0).abs() < 0.001);
    }

    #[test]
    fn indx_entry_decompressed_page_count() {
        let entry = IndxEntry {
            page_index: 0,
            packed_field: 0x00FFFFFF,
            flags_and_page_count: 0,
        };
        assert_eq!(entry.decompressed_page_count(), 0x00FFFFFF);
    }

    #[test]
    fn indx_entry_compressed_page_count() {
        let entry = IndxEntry {
            page_index: 0,
            packed_field: 0,
            flags_and_page_count: 0xFFFF,
        };
        assert_eq!(entry.compressed_page_count(), 0xFFFF);
    }

    #[test]
    fn aset_entry_block_index() {
        let entry = AsetEntry {
            asset_hash: 0,
            secondary_ref: 0,
            packed_block_ref: 0x12340000,
            type_id: 0,
        };
        assert_eq!(entry.block_index(), 0x1234);
    }

    #[test]
    fn aset_entry_sub_entry() {
        let entry = AsetEntry {
            asset_hash: 0,
            secondary_ref: 0,
            packed_block_ref: 0x12345678,
            type_id: 0,
        };
        assert_eq!(entry.sub_entry(), 0x5678);
    }

    #[test]
    fn aset_entry_is_primary_yes() {
        let entry = AsetEntry {
            asset_hash: 0,
            secondary_ref: 0,
            packed_block_ref: 0x1234FFFF,
            type_id: 0,
        };
        assert!(entry.is_primary());
    }

    #[test]
    fn aset_entry_is_primary_no() {
        let entry = AsetEntry {
            asset_hash: 0,
            secondary_ref: 0,
            packed_block_ref: 0x12340000,
            type_id: 0,
        };
        assert!(!entry.is_primary());
    }

    #[test]
    fn parse_ffcs_header_bad_magic() {
        let mut header = [0u8; FFCS_HEADER_SIZE];
        header[0..4].copy_from_slice(b"XXXX");
        let result = parse_ffcs_header(&header);
        assert!(result.is_err());
    }

    #[test]
    fn parse_ffcs_header_bad_version() {
        let mut header = [0u8; FFCS_HEADER_SIZE];
        header[0..4].copy_from_slice(b"FFCS");
        // version at offset 4 stays 0 (wrong)
        let result = parse_ffcs_header(&header);
        assert!(result.is_err());
    }

    #[test]
    fn parse_ffcs_header_valid_zero_chunks() {
        let mut header = [0u8; FFCS_HEADER_SIZE];
        header[0..4].copy_from_slice(b"FFCS");
        // version = 2
        header[4..8].copy_from_slice(&2u32.to_le_bytes());
        // chunk_count = 0
        header[8..12].copy_from_slice(&0u32.to_le_bytes());
        let result = parse_ffcs_header(&header);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn parse_ffcs_header_valid_one_chunk() {
        let mut header = [0u8; FFCS_HEADER_SIZE];
        header[0..4].copy_from_slice(b"FFCS");
        // version = 2
        header[4..8].copy_from_slice(&2u32.to_le_bytes());
        // chunk_count = 1
        header[8..12].copy_from_slice(&1u32.to_le_bytes());
        // First chunk at offset 0x0C: tag, offset, meta
        header[0x0C..0x10].copy_from_slice(b"INDX");
        header[0x10..0x14].copy_from_slice(&0x100u32.to_le_bytes());
        header[0x14..0x18].copy_from_slice(&10u32.to_le_bytes());
        let result = parse_ffcs_header(&header).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].tag, *b"INDX");
        assert_eq!(result[0].offset, 0x100);
        assert_eq!(result[0].meta, 10);
    }

    #[test]
    fn find_chunk_found() {
        let rows = vec![
            ChunkRow { tag: *b"INDX", offset: 0x100, meta: 10 },
            ChunkRow { tag: *b"DATA", offset: 0x200, meta: 20 },
        ];
        let found = find_chunk(&rows, b"DATA");
        assert!(found.is_some());
        assert_eq!(found.unwrap().offset, 0x200);
    }

    #[test]
    fn find_chunk_not_found() {
        let rows = vec![
            ChunkRow { tag: *b"INDX", offset: 0x100, meta: 10 },
        ];
        let found = find_chunk(&rows, b"XXXX");
        assert!(found.is_none());
    }

    #[test]
    fn find_chunk_empty() {
        let rows: Vec<ChunkRow> = vec![];
        let found = find_chunk(&rows, b"INDX");
        assert!(found.is_none());
    }

    #[test]
    fn chunk_row_clone() {
        let row = ChunkRow {
            tag: *b"INDX",
            offset: 0x100,
            meta: 10,
        };
        let cloned = row.clone();
        assert_eq!(cloned.tag, row.tag);
        assert_eq!(cloned.offset, row.offset);
    }

    #[test]
    fn indx_entry_clone() {
        let entry = IndxEntry {
            page_index: 42,
            packed_field: 100,
            flags_and_page_count: 200,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.page_index, entry.page_index);
    }

    #[test]
    fn aset_entry_clone() {
        let entry = AsetEntry {
            asset_hash: 0x12345678,
            secondary_ref: 0x87654321,
            packed_block_ref: 0xDEADBEEF,
            type_id: 27,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.asset_hash, entry.asset_hash);
        assert_eq!(cloned.type_id, entry.type_id);
    }

    #[test]
    fn page_size_constant() {
        assert_eq!(PAGE_SIZE, 0x8000);
    }

    #[test]
    fn ffcs_header_size_constant() {
        assert_eq!(FFCS_HEADER_SIZE, 0x100);
    }
}
