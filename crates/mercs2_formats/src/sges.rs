//! sges block compression and decompression.

use flate2::write::DeflateEncoder;
use flate2::{Compression, Decompress};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::ffcs::{read_u32_le, IndxEntry, PAGE_SIZE};

const DEFAULT_SEGMENT_SIZE: usize = 65536;
const DEFAULT_LEVEL: u32 = 6;

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

pub fn decompress_sges(block_data: &[u8]) -> Result<Vec<u8>, String> {
    if block_data.len() < 16 {
        return Err("Block too small for sges header".into());
    }
    if &block_data[0..4] != b"sges" {
        return Err(format!(
            "Bad sges magic: {:02X} {:02X} {:02X} {:02X}",
            block_data[0], block_data[1], block_data[2], block_data[3]
        ));
    }

    let segment_count = read_u16_le(block_data, 6) as usize;
    let total_uncompressed = read_u32_le(block_data, 8) as usize;
    let table_start = 16usize;
    let table_size = segment_count * 8;
    if block_data.len() < table_start + table_size {
        return Err("Block too small for segment table".into());
    }

    struct Segment {
        compressed_size: usize,
        uncompressed_size: usize,
        data_offset: usize,
        is_compressed: bool,
    }

    let mut segments = Vec::with_capacity(segment_count);
    for i in 0..segment_count {
        let base = table_start + i * 8;
        let compressed_size = read_u16_le(block_data, base) as usize;
        let raw_uncomp = read_u16_le(block_data, base + 2) as usize;
        let uncompressed_size = if raw_uncomp == 0 {
            DEFAULT_SEGMENT_SIZE
        } else {
            raw_uncomp
        };
        let offset_with_flag = read_u32_le(block_data, base + 4);
        let is_compressed = (offset_with_flag & 1) != 0;
        let data_offset = (offset_with_flag & 0xFFFFFFFE) as usize;
        segments.push(Segment {
            compressed_size,
            uncompressed_size,
            data_offset,
            is_compressed,
        });
    }

    // Mirror tools/sges_decompress.py: the per-segment u16 `compressed_size` is
    // unreliable for incompressible/large segments (it can wrap or be 0), so for
    // COMPRESSED segments we feed the inflater the byte span from this segment's
    // offset up to the next segment's offset (capped at 128 KB) and let it consume
    // exactly the deflate stream. Output is capped at the header's total_uncompressed.
    let target = total_uncompressed;
    let end = block_data.len();
    let mut output = Vec::with_capacity(total_uncompressed);
    for (i, seg) in segments.iter().enumerate() {
        if output.len() >= target {
            break;
        }
        let pos = seg.data_offset;
        if pos >= end {
            break;
        }
        if seg.is_compressed {
            let next_off = if i + 1 < segments.len() {
                segments[i + 1].data_offset
            } else {
                end
            };
            let read_end = next_off.min(pos + 131072).min(end);
            if read_end <= pos {
                break;
            }
            let chunk = &block_data[pos..read_end];
            let mut decompressor = Decompress::new(false);
            let mut buf = vec![0u8; seg.uncompressed_size];
            match decompressor.decompress(chunk, &mut buf, flate2::FlushDecompress::Finish) {
                Ok(_) => {
                    let written = decompressor.total_out() as usize;
                    buf.truncate(written);
                    output.extend_from_slice(&buf);
                }
                // Match the Python reference: stop on a corrupt stream rather than
                // failing the whole block (the page_count check flags truncation).
                Err(_) => break,
            }
        } else {
            let actual_sz = if seg.compressed_size > 0 {
                seg.compressed_size
            } else {
                seg.uncompressed_size
            };
            let remaining = target - output.len();
            let read_sz = actual_sz.min(remaining);
            let read_end = (pos + read_sz).min(end);
            if read_end > pos {
                output.extend_from_slice(&block_data[pos..read_end]);
            }
        }
    }
    if output.len() > target {
        output.truncate(target);
    }
    Ok(output)
}

pub fn decompress_block(
    file: &mut File,
    indx_entries: &[IndxEntry],
    block_index: u16,
) -> Result<Vec<u8>, String> {
    let idx = block_index as usize;
    if idx >= indx_entries.len() {
        return Err(format!(
            "block_index {idx} >= INDX count {}",
            indx_entries.len()
        ));
    }
    let indx = &indx_entries[idx];
    let file_offset = indx.page_index as u64 * PAGE_SIZE;
    let compressed_pages = indx.compressed_page_count();
    let compressed_size = compressed_pages as usize * PAGE_SIZE as usize;

    file.seek(SeekFrom::Start(file_offset))
        .map_err(|e| format!("seek error: {e}"))?;
    let mut compressed_data = vec![0u8; compressed_size];
    file.read_exact(&mut compressed_data)
        .map_err(|e| format!("read error: {e}"))?;

    if compressed_data.len() >= 4 && &compressed_data[0..4] == b"sges" {
        decompress_sges(&compressed_data)
    } else if compressed_data.len() >= 4 && &compressed_data[0..4] == b"UCFX" {
        let decomp_pages = indx.decompressed_page_count();
        let decomp_size = decomp_pages as usize * PAGE_SIZE as usize;
        compressed_data.truncate(decomp_size);
        Ok(compressed_data)
    } else {
        let decomp_pages = indx.decompressed_page_count();
        let decomp_size = decomp_pages as usize * PAGE_SIZE as usize;
        compressed_data.truncate(decomp_size);
        Ok(compressed_data)
    }
}

/// Decompress only the HEAD of a block (first ≤ `max_out` bytes) — enough to read the
/// block entry table without inflating a whole multi-MB block. Reads at most the header,
/// segment table and first segment; non-sges blocks return their leading bytes directly.
pub fn decompress_block_head(
    file: &mut File,
    indx_entries: &[IndxEntry],
    block_index: u16,
    max_out: usize,
) -> Result<Vec<u8>, String> {
    let idx = block_index as usize;
    if idx >= indx_entries.len() {
        return Err(format!(
            "block_index {idx} >= INDX count {}",
            indx_entries.len()
        ));
    }
    let indx = &indx_entries[idx];
    let file_offset = indx.page_index as u64 * PAGE_SIZE;
    let compressed_size = indx.compressed_page_count() as usize * PAGE_SIZE as usize;
    // Header (16) + segment table + first segment (compressed ≤ 64 KB) fit in 128 KB.
    let read_size = compressed_size.min(0x20000);
    file.seek(SeekFrom::Start(file_offset))
        .map_err(|e| format!("seek error: {e}"))?;
    let mut head = vec![0u8; read_size];
    file.read_exact(&mut head)
        .map_err(|e| format!("read error: {e}"))?;

    if head.len() < 24 || &head[0..4] != b"sges" {
        head.truncate(max_out);
        return Ok(head);
    }
    let segment_count = read_u16_le(&head, 6) as usize;
    if segment_count == 0 {
        return Err("sges head: no segments".into());
    }
    // First segment table row @0x10 (same decode as `decompress_sges`).
    let compressed_size0 = read_u16_le(&head, 16) as usize;
    let raw_uncomp = read_u16_le(&head, 18) as usize;
    let uncompressed_size = if raw_uncomp == 0 {
        DEFAULT_SEGMENT_SIZE
    } else {
        raw_uncomp
    };
    let offset_with_flag = read_u32_le(&head, 20);
    let is_compressed = (offset_with_flag & 1) != 0;
    let data_offset = (offset_with_flag & 0xFFFFFFFE) as usize;
    if data_offset >= head.len() {
        return Err("sges head: first segment offset out of range".into());
    }
    if !is_compressed {
        let actual_sz = if compressed_size0 > 0 {
            compressed_size0
        } else {
            uncompressed_size
        };
        let end = (data_offset + actual_sz.min(max_out)).min(head.len());
        return Ok(head[data_offset..end].to_vec());
    }
    let chunk = &head[data_offset..(data_offset + 131072).min(head.len())];
    let mut decompressor = Decompress::new(false);
    let mut buf = vec![0u8; uncompressed_size.min(max_out.max(1))];
    match decompressor.decompress(chunk, &mut buf, flate2::FlushDecompress::Finish) {
        Ok(_) => {
            buf.truncate(decompressor.total_out() as usize);
            Ok(buf)
        }
        Err(e) => Err(format!("sges head inflate: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Compression (inverse of `decompress_sges`).
//
// Faithful port of `tools/sges_compress.py`. Format (version 4):
//   - 16-byte header: `sges` + u16 major + u16 segment_count
//     + u32 total_uncompressed + u32 total_compressed
//   - Segment table @0x10: segment_count x (u16 comp_size, u16 uncomp_size,
//     u32 offset_with_flag). bit 0 of the offset = compressed flag; all real
//     offsets are 16-byte aligned (always even) so the flag is free.
//   - Payload: raw-deflate (or stored-raw) segments at 16-byte-aligned offsets.
//
// NOTE: the deflate *bitstream* is not byte-identical to Python's zlib — a
// different deflate implementation packs the same data differently. Correctness
// is therefore defined by round-trip (`decompress_sges(compress_sges(x)) == x`)
// and engine load, NOT by byte-matching the Python WAD. The container framing
// (header, table, offsets, flags, alignment) IS byte-faithful to the Python.
// ---------------------------------------------------------------------------

fn align16(x: usize) -> usize {
    (x + 15) / 16 * 16
}

/// Byte offset where the compressed payload starts (16-byte aligned).
fn sges_data_offset(num_segments: usize) -> usize {
    align16(16 + num_segments * 8)
}

/// Raw deflate (no zlib header/trailer), matching Python's `compressobj(level, DEFLATED, -15)`.
fn deflate_raw(data: &[u8], level: u32) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(level));
    enc.write_all(data).expect("in-memory deflate never fails");
    enc.finish().expect("in-memory deflate never fails")
}

/// Compress raw block data into an sges block with default options (level 6, 64 KB segments, major 4).
pub fn compress_sges(uncompressed: &[u8]) -> Result<Vec<u8>, String> {
    compress_sges_opts(uncompressed, DEFAULT_SEGMENT_SIZE, DEFAULT_LEVEL, 4)
}

/// Compress raw block data into an sges block. See module note on byte-fidelity.
pub fn compress_sges_opts(
    uncompressed: &[u8],
    segment_size: usize,
    level: u32,
    major: u16,
) -> Result<Vec<u8>, String> {
    if uncompressed.is_empty() {
        return Err("Cannot compress empty data".into());
    }

    struct Seg {
        stored: Vec<u8>,
        uncomp_size: usize,
        is_compressed: bool,
    }

    let total_u = uncompressed.len();
    let mut segments: Vec<Seg> = Vec::new();
    let mut offset = 0usize;
    while offset < total_u {
        let end = (offset + segment_size).min(total_u);
        let chunk = &uncompressed[offset..end];
        let compressed = deflate_raw(chunk, level);
        // Fall back to raw storage when deflate overflows the u16 comp_sz limit
        // or fails to shrink the data.
        if compressed.len() > 65535 || compressed.len() >= chunk.len() {
            segments.push(Seg {
                stored: chunk.to_vec(),
                uncomp_size: chunk.len(),
                is_compressed: false,
            });
        } else {
            segments.push(Seg {
                stored: compressed,
                uncomp_size: chunk.len(),
                is_compressed: true,
            });
        }
        offset = end;
    }

    let num_segments = segments.len();
    let data_start = sges_data_offset(num_segments);

    // Lay out segments at 16-byte-aligned offsets, tracking each start.
    let mut seg_offsets: Vec<usize> = Vec::with_capacity(num_segments);
    let mut payload: Vec<u8> = Vec::new();
    let mut pos = data_start;
    for seg in &segments {
        seg_offsets.push(pos);
        payload.extend_from_slice(&seg.stored);
        pos += seg.stored.len();
        let padding = align16(pos) - pos;
        payload.extend(std::iter::repeat(0u8).take(padding));
        pos += padding;
    }

    let last_seg_end = seg_offsets[num_segments - 1] + segments[num_segments - 1].stored.len();
    let total_c = align16(last_seg_end);

    let mut block: Vec<u8> = Vec::with_capacity(total_c);
    block.extend_from_slice(b"sges");
    block.extend_from_slice(&major.to_le_bytes());
    block.extend_from_slice(&(num_segments as u16).to_le_bytes());
    block.extend_from_slice(&(total_u as u32).to_le_bytes());
    block.extend_from_slice(&(total_c as u32).to_le_bytes());

    for (i, seg) in segments.iter().enumerate() {
        // comp_sz: full-size raw segments (>65535) overflow u16 -> store 0 (= default).
        let comp_sz: u16 = if !seg.is_compressed && seg.stored.len() > 65535 {
            0
        } else {
            seg.stored.len() as u16
        };
        // uncomp field: 0 for full-size segments, actual size for the short last one.
        let uncomp_field: u16 = if seg.uncomp_size == segment_size {
            0
        } else {
            seg.uncomp_size as u16
        };
        let mut abs_offset_flagged = seg_offsets[i] as u32;
        if seg.is_compressed {
            abs_offset_flagged |= 1;
        }
        block.extend_from_slice(&comp_sz.to_le_bytes());
        block.extend_from_slice(&uncomp_field.to_le_bytes());
        block.extend_from_slice(&abs_offset_flagged.to_le_bytes());
    }

    // Pad header + table up to the payload start.
    block.resize(data_start, 0);

    // Append payload (trim the trailing inter-segment pad after the last segment;
    // total_c covers the final extent).
    let actual_payload_needed = last_seg_end - data_start;
    block.extend_from_slice(&payload[..actual_payload_needed]);

    // Pad the whole block out to total_c.
    block.resize(total_c, 0);

    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) {
        let block = compress_sges(data).expect("compress");
        assert_eq!(&block[0..4], b"sges");
        let back = decompress_sges(&block).expect("decompress");
        assert_eq!(back, data, "round-trip mismatch for {} bytes", data.len());
    }

    #[test]
    fn roundtrip_small() {
        roundtrip(b"hello sges world, the quick brown fox jumps over the lazy dog");
    }

    #[test]
    fn roundtrip_highly_compressible() {
        roundtrip(&vec![0xABu8; 200_000]);
    }

    #[test]
    fn roundtrip_multi_segment_mixed() {
        // > 3 full 64 KB segments + a short tail, mixed content so some segments
        // compress and the layout exercises alignment + the last-segment math.
        let mut data = Vec::new();
        for i in 0..200_000usize {
            data.push((i.wrapping_mul(2654435761) >> 13) as u8);
        }
        data.extend_from_slice(&[0u8; 70_000]); // a compressible run spanning a segment
        roundtrip(&data);
    }

    #[test]
    fn roundtrip_incompressible_falls_back_to_raw() {
        // Pseudo-random, deflate can't shrink it -> stored raw; must still round-trip.
        let mut data = Vec::with_capacity(100_000);
        let mut x: u32 = 0x12345678;
        for _ in 0..100_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            data.push(x as u8);
        }
        roundtrip(&data);
    }
}
