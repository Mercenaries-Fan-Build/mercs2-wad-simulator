//! Big-endian Xbox 360 DLC input parsing (port of the parser half of
//! `tools/x360_dlc_io.py`): the BE FFCS/INDX/ASET/PTHS readers and the BE
//! `segs` (sges) decompressor. The STFS container reader + RAR extraction
//! live in `dlc_stfs.rs`.

use flate2::{Decompress, FlushDecompress};

pub const PAGE_SIZE: usize = 0x8000; // 32 KB (FFCS page)
pub const SCFF_MAGIC: &[u8; 4] = b"SCFF";
pub const SEGS_MAGIC: &[u8; 4] = b"segs";

fn read_u16_be(data: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([data[off], data[off + 1]])
}
fn read_u32_be(data: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

/// One FFCS chunk-table row. `tag` is the LE-readable name (e.g. "INDX"); in the
/// BE container the four tag bytes are stored reversed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfcsChunkRow {
    pub tag: String,
    pub offset: u32,
    pub meta: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndxEntry {
    pub page_index: u32,
    pub packed_field: u32,
    pub flags: u16,
    pub page_count: u16,
}

impl IndxEntry {
    pub fn file_offset(&self) -> usize {
        self.page_index as usize * PAGE_SIZE
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsetEntry {
    pub asset_hash: u32,
    pub u1: u32,
    pub u2: u32,
    pub u3: u32,
}

impl AsetEntry {
    /// Block index is the high 16 bits of u2 (only valid when u2 is read BE).
    pub fn block_index(&self) -> u16 {
        ((self.u2 >> 16) & 0xFFFF) as u16
    }
}

/// Parse a big-endian FFCS header. Returns `(version, chunk_rows)` (up to 5 rows).
pub fn parse_be_ffcs(doh: &[u8]) -> Result<(u32, Vec<FfcsChunkRow>), String> {
    if doh.len() < 12 || &doh[0..4] != SCFF_MAGIC {
        return Err(format!(
            "Expected SCFF, got {:?}",
            &doh[0..4.min(doh.len())]
        ));
    }
    let version = read_u32_be(doh, 4);
    let chunk_count = read_u32_be(doh, 8) as usize;

    let mut rows = Vec::new();
    for i in 0..chunk_count.min(5) {
        let off = 0x0C + i * 12;
        // Tag bytes are stored reversed in the BE container.
        let tag_bytes = [doh[off + 3], doh[off + 2], doh[off + 1], doh[off]];
        let tag = String::from_utf8_lossy(&tag_bytes).into_owned();
        rows.push(FfcsChunkRow {
            tag,
            offset: read_u32_be(doh, off + 4),
            meta: read_u32_be(doh, off + 8),
        });
    }
    Ok((version, rows))
}

/// Parse big-endian INDX entries.
pub fn parse_be_indx(doh: &[u8], indx_offset: usize, count: usize) -> Vec<IndxEntry> {
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let off = indx_offset + i * 12;
        let page_idx = read_u32_be(doh, off);
        let packed = read_u32_be(doh, off + 4);
        let flags_pages = read_u32_be(doh, off + 8);
        entries.push(IndxEntry {
            page_index: page_idx,
            packed_field: packed,
            flags: ((flags_pages >> 16) & 0xFFFF) as u16,
            page_count: (flags_pages & 0xFFFF) as u16,
        });
    }
    entries
}

/// Parse big-endian ASET entries.
///
/// `u0`/`u1`/`u2` are big-endian; `u2`'s high 16 bits encode the block index.
/// `u3` (type_id) is stored **little-endian** even inside the BE container.
pub fn parse_be_aset(doh: &[u8], aset_offset: usize, count: usize) -> Vec<AsetEntry> {
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let off = aset_offset + i * 16;
        entries.push(AsetEntry {
            asset_hash: read_u32_be(doh, off),
            u1: read_u32_be(doh, off + 4),
            u2: read_u32_be(doh, off + 8),
            u3: crate::ffcs::read_u32_le(doh, off + 12), // LE field
        });
    }
    entries
}

/// Parse big-endian PTHS path strings (null-separated). Stops at the trailer.
pub fn parse_be_pths(doh: &[u8], pths_offset: usize, count: usize) -> Vec<String> {
    let mut paths = Vec::new();
    let mut pos = pths_offset;
    for _ in 0..count {
        let nul = match doh[pos..].iter().position(|&b| b == 0) {
            Some(rel) => pos + rel,
            None => break,
        };
        let s = String::from_utf8_lossy(&doh[pos..nul]).into_owned();
        if s.contains('\\') || s.contains('/') {
            paths.push(s);
        } else if s.starts_with("xa37dd45") {
            break;
        }
        pos = nul + 1;
    }
    paths
}

/// Decompress a big-endian Xbox 360 `segs` block.
///
/// Layout: `segs` magic, BE u16 version @4, BE u16 segment_count @6,
/// BE u32 total_decompressed @8, BE u32 total_compressed @12, then an N×8 segment
/// table (BE u16 comp_size, BE u16 decomp_size [0 = 64 KB], u32 offset unused).
/// Payload begins after the 16-byte-aligned header; segment starts are 16-byte aligned.
pub fn decompress_be_sges(data: &[u8], offset: usize, max_size: usize) -> Result<Vec<u8>, String> {
    if data.len() < offset + 16 || &data[offset..offset + 4] != SEGS_MAGIC {
        return Err(format!("Expected segs magic at 0x{offset:X}"));
    }
    let seg_count = read_u16_be(data, offset + 6) as usize;
    let decomp_total = read_u32_be(data, offset + 8) as usize;

    let mut seg_table: Vec<(usize, usize)> = Vec::with_capacity(seg_count);
    for si in 0..seg_count {
        let so = offset + 16 + si * 8;
        let csz = read_u16_be(data, so) as usize;
        let dsz = read_u16_be(data, so + 2) as usize;
        seg_table.push((csz, dsz));
    }

    let seg_table_bytes = seg_count * 8;
    let header_size = if seg_count > 0 {
        16 + ((seg_table_bytes + 15) & !15)
    } else {
        16
    };
    let payload_start = offset + header_size;
    let payload_end = (offset + max_size).min(data.len());
    if payload_start > payload_end {
        return Err("segs payload start beyond block".into());
    }
    let payload = &data[payload_start..payload_end];

    let mut result: Vec<u8> = Vec::with_capacity(decomp_total);
    let mut pos = 0usize;
    for (csz, dsz) in seg_table {
        let is_raw = csz > 0 && csz == dsz;
        if is_raw {
            let end = (pos + csz).min(payload.len());
            result.extend_from_slice(&payload[pos..end]);
            pos = end;
        } else {
            let out_cap = if dsz == 0 { 65536 } else { dsz };
            let mut buf = vec![0u8; out_cap];
            let mut dc = Decompress::new(false); // raw deflate
            let input = &payload[pos..];
            dc.decompress(input, &mut buf, FlushDecompress::Finish)
                .map_err(|e| format!("be sges inflate error: {e}"))?;
            let produced = dc.total_out() as usize;
            let consumed = dc.total_in() as usize;
            buf.truncate(produced);
            result.extend_from_slice(&buf);
            pos += consumed;
        }
        pos = (pos + 15) & !15;
    }

    if result.len() != decomp_total {
        return Err(format!(
            "Decompressed {} bytes but expected {}",
            result.len(),
            decomp_total
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn be_ffcs_fixture() -> Vec<u8> {
        // SCFF + version 2 + chunk_count 5, then 5 rows with reversed tags.
        let mut d = vec![0u8; 0x0C + 5 * 12];
        d[0..4].copy_from_slice(SCFF_MAGIC);
        d[4..8].copy_from_slice(&2u32.to_be_bytes());
        d[8..12].copy_from_slice(&5u32.to_be_bytes());
        let rows = [
            (b"INDX", 0x8000u32, 3u32),
            (b"ASET", 0x9000, 7),
            (b"PTHS", 0xA000, 3),
            (b"DATA", 0x208000, 36),
            (b"CSUM", 0x1234, 0),
        ];
        for (i, (tag, off, meta)) in rows.iter().enumerate() {
            let o = 0x0C + i * 12;
            // store reversed
            d[o] = tag[3];
            d[o + 1] = tag[2];
            d[o + 2] = tag[1];
            d[o + 3] = tag[0];
            d[o + 4..o + 8].copy_from_slice(&off.to_be_bytes());
            d[o + 8..o + 12].copy_from_slice(&meta.to_be_bytes());
        }
        d
    }

    #[test]
    fn parse_be_ffcs_reads_reversed_tags() {
        let (ver, rows) = parse_be_ffcs(&be_ffcs_fixture()).unwrap();
        assert_eq!(ver, 2);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].tag, "INDX");
        assert_eq!(rows[0].offset, 0x8000);
        assert_eq!(rows[1].tag, "ASET");
        assert_eq!(rows[3].tag, "DATA");
    }

    #[test]
    fn parse_be_aset_u3_is_le() {
        // One ASET entry: u0/u1/u2 BE, u3 LE.
        let mut d = vec![0u8; 16];
        d[0..4].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        d[4..8].copy_from_slice(&0x1111_2222u32.to_be_bytes());
        d[8..12].copy_from_slice(&0x0005_8800u32.to_be_bytes()); // block_index = 5
        d[12..16].copy_from_slice(&[0x1B, 0, 0, 0]); // u3 = 27 (LE)
        let e = &parse_be_aset(&d, 0, 1)[0];
        assert_eq!(e.asset_hash, 0xDEAD_BEEF);
        assert_eq!(e.block_index(), 5);
        assert_eq!(e.u3, 27, "u3 must be read little-endian");
    }

    #[test]
    fn be_sges_raw_segment() {
        // segs header + one raw segment (csz == dsz) of 10 bytes.
        let body = b"0123456789";
        let mut d = vec![0u8; 16 + 8];
        d[0..4].copy_from_slice(SEGS_MAGIC);
        d[4..6].copy_from_slice(&4u16.to_be_bytes()); // version
        d[6..8].copy_from_slice(&1u16.to_be_bytes()); // seg_count
        d[8..12].copy_from_slice(&(body.len() as u32).to_be_bytes());
        d[12..16].copy_from_slice(&0u32.to_be_bytes());
        d[16..18].copy_from_slice(&(body.len() as u16).to_be_bytes()); // csz
        d[18..20].copy_from_slice(&(body.len() as u16).to_be_bytes()); // dsz == csz -> raw
        // header_size = 16 + align16(8) = 32
        d.resize(32, 0);
        d.extend_from_slice(body);
        let out = decompress_be_sges(&d, 0, d.len()).unwrap();
        assert_eq!(out, body);
    }

    #[test]
    fn real_doh_matches_python_golden() {
        // Cross-check against tools/x360_dlc_io.py on the REAL DLC .doh.
        // Golden captured from Python (output/_scratch/dlc_be_golden.json).
        // Skips cleanly if the cached .doh isn't present on this machine.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../output/_scratch/dlc01.doh"
        );
        let doh = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("SKIP real_doh_matches_python_golden: {path} not found");
                return;
            }
        };

        let (ver, rows) = parse_be_ffcs(&doh).unwrap();
        assert_eq!(ver, 2);
        let chunk = |t: &str| rows.iter().find(|r| r.tag == t).unwrap();
        assert_eq!((chunk("INDX").offset, chunk("INDX").meta), (32768, 2196));
        assert_eq!((chunk("DATA").offset, chunk("DATA").meta), (294912, 36));
        assert_eq!((chunk("CSUM").offset, chunk("CSUM").meta), (3130166264, 933));
        assert_eq!((chunk("ASET").offset, chunk("ASET").meta), (59120, 5341));
        assert_eq!((chunk("PTHS").offset, chunk("PTHS").meta), (144576, 2196));

        let indx = parse_be_indx(&doh, chunk("INDX").offset as usize, chunk("INDX").meta as usize);
        assert_eq!(indx.len(), 2196);
        assert_eq!(
            (indx[0].page_index, indx[0].packed_field, indx[0].flags, indx[0].page_count),
            (9, 1, 32768, 1)
        );
        assert_eq!(
            (indx[2].page_index, indx[2].packed_field, indx[2].flags, indx[2].page_count),
            (11, 5, 32768, 2)
        );

        let aset = parse_be_aset(&doh, chunk("ASET").offset as usize, chunk("ASET").meta as usize);
        assert_eq!(aset.len(), 5341);
        assert_eq!(
            (aset[0].asset_hash, aset[0].u1, aset[0].u2, aset[0].u3),
            (1433122535, 126289364, 68616220, 27)
        );
        assert_eq!(
            (aset[1].asset_hash, aset[1].u1, aset[1].u2, aset[1].u3),
            (1843225593, 4294967295, 4294902224, 28)
        );

        let pths = parse_be_pths(&doh, chunk("PTHS").offset as usize, chunk("PTHS").meta as usize);
        assert_eq!(pths[0], "blocks\\dlc01\\dlc01_terrain_P000_Q3.block");
        assert_eq!(pths[1], "blocks\\dlc01\\dlc01_caicara_foliage_P000_Q3.block");
        assert_eq!(pths[2], "blocks\\dlc01\\dlc01_dlccon004_P000_Q3.block");
    }

    #[test]
    fn be_sges_compressed_segment() {
        let body: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(6));
        enc.write_all(&body).unwrap();
        let deflated = enc.finish().unwrap();

        let mut d = vec![0u8; 32];
        d[0..4].copy_from_slice(SEGS_MAGIC);
        d[4..6].copy_from_slice(&4u16.to_be_bytes());
        d[6..8].copy_from_slice(&1u16.to_be_bytes());
        d[8..12].copy_from_slice(&(body.len() as u32).to_be_bytes());
        d[12..16].copy_from_slice(&0u32.to_be_bytes());
        // csz != dsz -> compressed; dsz = decompressed size
        d[16..18].copy_from_slice(&(deflated.len() as u16).to_be_bytes());
        d[18..20].copy_from_slice(&(body.len() as u16).to_be_bytes());
        d.extend_from_slice(&deflated);
        let out = decompress_be_sges(&d, 0, d.len()).unwrap();
        assert_eq!(out, body);
    }
}
