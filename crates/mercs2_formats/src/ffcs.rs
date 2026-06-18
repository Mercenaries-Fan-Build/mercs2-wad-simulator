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
