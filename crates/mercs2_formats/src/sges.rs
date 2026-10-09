//! sges block compression and decompression.
//!
//! A WAD block is a segment stream. The PC bake writes it little-endian under the magic `sges`;
//! the Xbox 360 and PS3 bakes write the same layout with every field big-endian, under the magic
//! `segs`:
//!
//! ```text
//! +0x00 magic       `sges` (little-endian fields) or `segs` (big-endian fields)
//! +0x04 u16 version
//! +0x06 u16 segment_count
//! +0x08 u32 total_uncompressed
//! +0x0c u32 total_compressed   (16-byte-aligned end of the last segment)
//! +0x10 segment_count × { u16 comp_size, u16 uncomp_size (0 = 65536), u32 offset_with_flag }
//! ```
//!
//! `offset_with_flag` is the segment's byte offset from the start of the stream with bit 0 set
//! when the segment is raw deflate; a segment with bit 0 clear is stored, `comp_size` bytes long
//! (0 = `uncomp_size`).

use flate2::write::DeflateEncoder;
use flate2::{Compression, Decompress};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::ffcs::{Endian, IndxEntry, PAGE_SIZE};

const DEFAULT_SEGMENT_SIZE: usize = 65536;
const DEFAULT_LEVEL: u32 = 6;

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u16(e: Endian, data: &[u8], offset: usize) -> u16 {
    match e {
        Endian::Little => read_u16_le(data, offset),
        Endian::Big => u16::from_be_bytes([data[offset], data[offset + 1]]),
    }
}

/// The byte order a segment stream's magic declares: `sges` little-endian, `segs` big-endian.
fn stream_endian(block_data: &[u8]) -> Result<Endian, String> {
    match block_data.get(0..4) {
        Some(b"sges") => Ok(Endian::Little),
        Some(b"segs") => Ok(Endian::Big),
        other => Err(format!(
            "segment stream starts with {:02x?}, neither `sges` nor `segs`",
            other.unwrap_or(block_data)
        )),
    }
}

/// Exact byte length of the `sges` stream at the start of `block_data`.
///
/// A block stored in a WAD is padded with zeroes out to a 32 KB page boundary. Recovering
/// the block therefore means knowing where its sges stream actually ends — and *guessing*
/// by trimming trailing zeroes truncates any stream whose final segment legitimately ends
/// in zero bytes (an all-zero payload compresses to a deflate stream ending in 0x00).
///
/// The header records the answer at `+12` (`total_c`, the 16-byte-aligned end of the last
/// segment). We trust it only when it is corroborated by an independent walk of the
/// segment table, so a corrupt or hostile header cannot make us over-read: for a stored
/// segment the walk ends at `data_offset + size`; for a deflate segment it ends wherever
/// the inflater actually stopped (`total_in`), since the per-segment u16 `compressed_size`
/// is unreliable for large/incompressible segments.
pub fn compressed_len(block_data: &[u8]) -> Result<usize, String> {
    let (endian, segments) = parse_segment_table(block_data)?;
    let end = block_data.len();
    let mut max_end = 16 + segments.len() * 8; // header + segment table

    for (i, seg) in segments.iter().enumerate() {
        let pos = seg.data_offset;
        if pos >= end {
            break;
        }
        let consumed = if seg.is_compressed {
            let next_off = if i + 1 < segments.len() {
                segments[i + 1].data_offset
            } else {
                end
            };
            let read_end = next_off.min(pos + 131072).min(end);
            if read_end <= pos {
                break;
            }
            let mut decompressor = Decompress::new(false);
            let mut buf = vec![0u8; seg.uncompressed_size];
            match decompressor.decompress(
                &block_data[pos..read_end],
                &mut buf,
                flate2::FlushDecompress::Finish,
            ) {
                Ok(_) => decompressor.total_in() as usize,
                Err(e) => return Err(format!("sges segment {i}: {e}")),
            }
        } else if seg.compressed_size > 0 {
            seg.compressed_size
        } else {
            seg.uncompressed_size
        };
        max_end = max_end.max((pos + consumed).min(end));
    }

    // Segments are laid out on 16-byte boundaries, and the writer rounds the final
    // segment's end up the same way — so the true stream length is align16 of the walk.
    let walked = align16(max_end).min(end);

    // `total_c` @ +12 is the writer's own record of that value. Prefer it, but only if it
    // agrees with the walk (>= it, and in bounds) — never let a bogus header over-read.
    let declared = endian.u32(block_data, 12) as usize;
    if declared >= walked && declared <= end {
        return Ok(declared);
    }
    Ok(walked)
}

struct Segment {
    compressed_size: usize,
    uncompressed_size: usize,
    data_offset: usize,
    is_compressed: bool,
}

/// Parse the header + segment table of an `sges` or `segs` stream (shared by `decompress_sges` /
/// `compressed_len`), with the byte order its magic declares.
fn parse_segment_table(block_data: &[u8]) -> Result<(Endian, Vec<Segment>), String> {
    if block_data.len() < 16 {
        return Err("Block too small for a segment-stream header".into());
    }
    let e = stream_endian(block_data)?;

    let segment_count = read_u16(e, block_data, 6) as usize;
    let table_start = 16usize;
    let table_size = segment_count * 8;
    if block_data.len() < table_start + table_size {
        return Err("Block too small for segment table".into());
    }

    let mut segments = Vec::with_capacity(segment_count);
    for i in 0..segment_count {
        let base = table_start + i * 8;
        let compressed_size = read_u16(e, block_data, base) as usize;
        let raw_uncomp = read_u16(e, block_data, base + 2) as usize;
        let uncompressed_size = if raw_uncomp == 0 {
            DEFAULT_SEGMENT_SIZE
        } else {
            raw_uncomp
        };
        let offset_with_flag = e.u32(block_data, base + 4);
        segments.push(Segment {
            compressed_size,
            uncompressed_size,
            data_offset: (offset_with_flag & 0xFFFFFFFE) as usize,
            is_compressed: (offset_with_flag & 1) != 0,
        });
    }
    Ok((e, segments))
}

/// Inflate a whole `sges` (little-endian) or `segs` (big-endian) segment stream.
///
/// Every segment must decode to exactly its `uncomp_size`, and the segments together to exactly
/// the header's `total_uncompressed`; anything else is an error naming the segment. The per-segment
/// u16 `comp_size` is unreliable for deflate segments (it can wrap or be 0), so a deflate segment is
/// fed the bytes from its offset up to the next segment's offset (capped at 128 KB) and the
/// inflater consumes exactly its stream.
pub fn decompress_sges(block_data: &[u8]) -> Result<Vec<u8>, String> {
    let (e, segments) = parse_segment_table(block_data)?;
    let target = e.u32(block_data, 8) as usize;
    let end = block_data.len();
    let mut output = Vec::with_capacity(target);
    for (i, seg) in segments.iter().enumerate() {
        let pos = seg.data_offset;
        if pos >= end {
            return Err(format!("segment {i}: offset 0x{pos:x} is past the {end}-byte stream"));
        }
        if seg.is_compressed {
            let next_off = if i + 1 < segments.len() {
                segments[i + 1].data_offset
            } else {
                end
            };
            let read_end = next_off.min(pos + 131072).min(end);
            if read_end <= pos {
                return Err(format!("segment {i}: no bytes between offset 0x{pos:x} and the next segment"));
            }
            let mut decompressor = Decompress::new(false);
            let mut buf = vec![0u8; seg.uncompressed_size];
            decompressor
                .decompress(&block_data[pos..read_end], &mut buf, flate2::FlushDecompress::Finish)
                .map_err(|err| format!("segment {i}: inflate at 0x{pos:x}: {err}"))?;
            let written = decompressor.total_out() as usize;
            if written != seg.uncompressed_size {
                return Err(format!(
                    "segment {i}: inflated {written} bytes, the table says {}",
                    seg.uncompressed_size
                ));
            }
            output.extend_from_slice(&buf);
        } else {
            let size = if seg.compressed_size > 0 {
                seg.compressed_size
            } else {
                seg.uncompressed_size
            };
            if size != seg.uncompressed_size {
                return Err(format!(
                    "segment {i}: stored segment of {size} bytes, the table says {} uncompressed",
                    seg.uncompressed_size
                ));
            }
            if pos + size > end {
                return Err(format!("segment {i}: stored bytes 0x{pos:x}+{size} run past the {end}-byte stream"));
            }
            output.extend_from_slice(&block_data[pos..pos + size]);
        }
    }
    if output.len() != target {
        return Err(format!(
            "segments decode to {} bytes, the header says {target}",
            output.len()
        ));
    }
    Ok(output)
}

/// Whether INDX entry `idx` is consistent with the stored bytes it points at, which start with
/// `head`: a block flagged compressed ([`IndxEntry::is_compressed`]) is an `sges`/`segs` stream; a
/// block that is not is stored as-is in exactly its decompressed page count and is not a stream.
fn check_storage(indx: &IndxEntry, idx: usize, head: &[u8]) -> Result<(), String> {
    let stream = matches!(head.get(0..4), Some(b"sges") | Some(b"segs"));
    match (indx.is_compressed(), stream) {
        (true, true) => Ok(()),
        (true, false) => Err(format!(
            "block {idx} is flagged compressed but starts with {:02x?}, neither `sges` nor `segs`",
            &head[..head.len().min(4)]
        )),
        (false, true) => Err(format!(
            "block {idx} is flagged stored but starts with a segment-stream magic {:02x?}",
            &head[..4]
        )),
        (false, false) if indx.compressed_page_count() != indx.decompressed_page_count() => Err(format!(
            "block {idx} is flagged stored but occupies {} pages for {} decompressed pages",
            indx.compressed_page_count(),
            indx.decompressed_page_count()
        )),
        (false, false) => Ok(()),
    }
}

/// Read block `block_index` of an opened WAD. A block flagged compressed is inflated with
/// [`decompress_sges`] (a little-endian `sges` stream on PC, a big-endian `segs` stream on Xbox 360
/// and PS3); a block flagged stored is returned as stored, its whole page count. A block whose bytes
/// contradict its flag is an error.
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

    check_storage(indx, idx, &compressed_data)?;
    if !indx.is_compressed() {
        return Ok(compressed_data);
    }
    decompress_sges(&compressed_data).map_err(|e| format!("block {idx}: {e}"))
}

/// Decompress only the HEAD of a block (first ≤ `max_out` bytes) — enough to read the
/// block entry table without inflating a whole multi-MB block. A compressed block reads at most
/// the header, segment table and first segment of its `sges`/`segs` stream; a stored block returns
/// its leading bytes. A block whose bytes contradict its INDX flag is an error.
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

    check_storage(indx, idx, &head)?;
    if !indx.is_compressed() {
        head.truncate(max_out);
        return Ok(head);
    }
    if head.len() < 24 {
        return Err(format!("block {idx}: {} bytes, too small for a segment stream", head.len()));
    }
    let e = stream_endian(&head).map_err(|m| format!("block {idx}: {m}"))?;
    let segment_count = read_u16(e, &head, 6) as usize;
    if segment_count == 0 {
        return Err("segment stream head: no segments".into());
    }
    // First segment table row @0x10 (same decode as `decompress_sges`).
    let compressed_size0 = read_u16(e, &head, 16) as usize;
    let raw_uncomp = read_u16(e, &head, 18) as usize;
    let uncompressed_size = if raw_uncomp == 0 {
        DEFAULT_SEGMENT_SIZE
    } else {
        raw_uncomp
    };
    let offset_with_flag = e.u32(&head, 20);
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

    /// A big-endian `segs` stream holding `payload` as one stored segment.
    fn segs_stored(payload: &[u8]) -> Vec<u8> {
        let n = payload.len() as u16;
        let mut b = Vec::new();
        b.extend_from_slice(b"segs");
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        b.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        b.extend_from_slice(&n.to_be_bytes());
        b.extend_from_slice(&n.to_be_bytes());
        // Offset 32 (header + one table row, 16-byte aligned), bit 0 clear: stored.
        b.extend_from_slice(&32u32.to_be_bytes());
        b.resize(32, 0);
        b.extend_from_slice(payload);
        b
    }

    #[test]
    fn decompress_block_follows_the_indx_compressed_flag() {
        let page = PAGE_SIZE as usize;
        let le = compress_sges(b"little-endian block").unwrap();
        let be = segs_stored(b"big-endian block");
        let mut file = vec![0u8; 4 * page];
        file[..le.len()].copy_from_slice(&le);
        file[page..page + be.len()].copy_from_slice(&be);
        file[2 * page..2 * page + 8].copy_from_slice(&[0, 0, 0, 1, b'U', b'C', b'F', b'X']);
        let path = std::env::temp_dir().join(format!("sges_decompress_block_{}.bin", std::process::id()));
        std::fs::write(&path, &file).unwrap();
        let compressed = |page_index: u32| IndxEntry { page_index, packed_field: 1, flags_and_page_count: 0x8000_0001 };
        let stored = |page_index: u32, pages: u32| IndxEntry { page_index, packed_field: 1, flags_and_page_count: pages };
        let mut f = File::open(&path).unwrap();
        let indx = [compressed(0), compressed(1), stored(2, 1)];
        assert_eq!(decompress_block(&mut f, &indx, 0).unwrap(), b"little-endian block");
        assert_eq!(decompress_block(&mut f, &indx, 1).unwrap(), b"big-endian block");
        // A stored block is returned as stored, its whole page.
        let raw = decompress_block(&mut f, &indx, 2).unwrap();
        assert_eq!(raw.len(), page);
        assert_eq!(&raw[..8], &[0, 0, 0, 1, b'U', b'C', b'F', b'X']);
        // Bytes that contradict the flag.
        let e = decompress_block(&mut f, &[compressed(2)], 0).unwrap_err();
        assert!(e.contains("flagged compressed but starts with"), "{e}");
        let e = decompress_block(&mut f, &[stored(0, 1)], 0).unwrap_err();
        assert!(e.contains("flagged stored but starts with a segment-stream magic"), "{e}");
        let e = decompress_block(&mut f, &[stored(2, 2)], 0).unwrap_err();
        assert!(e.contains("occupies 2 pages for 1 decompressed pages"), "{e}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn roundtrip_small() {
        roundtrip(b"hello sges world, the quick brown fox jumps over the lazy dog");
    }

    /// `compressed_len` must report the true end of the sges stream. A block in a WAD is
    /// zero-padded to a 32 KB page, and the old recovery path guessed the boundary by
    /// trimming trailing zeroes — which truncates any stream whose final compressed byte
    /// happens to be 0x00. Simulate the WAD's padding and demand exact recovery.
    #[test]
    fn compressed_len_recovers_a_page_padded_block_exactly() {
        for payload in [
            vec![0x00u8; 5000],                              // all zeroes
            (0..9000u32).map(|i| (i % 251) as u8).collect(), // incompressible-ish
            b"trailing nul bytes follow\0\0\0\0\0\0\0\0".to_vec(),
        ] {
            let block = compress_sges(&payload).expect("compress");
            let exact = compressed_len(&block).expect("compressed_len");
            assert_eq!(exact, block.len(), "must report the true stream length");

            // Now pad out to a page boundary, as a WAD does, and recover.
            let mut padded = block.clone();
            padded.resize(PAGE_SIZE as usize * 2, 0);
            let recovered = compressed_len(&padded).expect("compressed_len on padded");
            assert_eq!(recovered, block.len(), "must see through the page padding");
            assert_eq!(
                decompress_sges(&padded[..recovered]).expect("decompress"),
                payload,
                "the recovered slice must still inflate to the original payload"
            );
        }
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
