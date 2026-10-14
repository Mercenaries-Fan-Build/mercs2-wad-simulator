//! Big-endian Xbox 360 DLC input parsing (port of the parser half of
//! `tools/x360_dlc_io.py`): the BE FFCS/INDX/ASET/PTHS readers and the BE
//! `segs` (sges) decompressor. The STFS container reader + RAR extraction
//! live in `dlc_stfs.rs`.

pub const PAGE_SIZE: usize = 0x8000; // 32 KB (FFCS page)
pub const SCFF_MAGIC: &[u8; 4] = b"SCFF";
pub const SEGS_MAGIC: &[u8; 4] = b"segs";

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

/// Decompress the big-endian `segs` block that starts at `offset` in `data` and spans at most
/// `max_size` bytes, with [`crate::sges::decompress_sges`] (the segment-stream layout is documented
/// there).
pub fn decompress_be_sges(data: &[u8], offset: usize, max_size: usize) -> Result<Vec<u8>, String> {
    if data.len() < offset + 16 || &data[offset..offset + 4] != SEGS_MAGIC {
        return Err(format!("Expected segs magic at 0x{offset:X}"));
    }
    let end = (offset + max_size).min(data.len());
    crate::sges::decompress_sges(&data[offset..end])
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
        // segs header + one stored segment of 10 bytes at offset 32 (header + one table row,
        // 16-byte aligned), bit 0 of the offset clear.
        let body = b"0123456789";
        let mut d = vec![0u8; 16 + 8];
        d[0..4].copy_from_slice(SEGS_MAGIC);
        d[4..6].copy_from_slice(&4u16.to_be_bytes()); // version
        d[6..8].copy_from_slice(&1u16.to_be_bytes()); // seg_count
        d[8..12].copy_from_slice(&(body.len() as u32).to_be_bytes());
        d[12..16].copy_from_slice(&0u32.to_be_bytes());
        d[16..18].copy_from_slice(&(body.len() as u16).to_be_bytes()); // comp_size
        d[18..20].copy_from_slice(&(body.len() as u16).to_be_bytes()); // uncomp_size
        d[20..24].copy_from_slice(&32u32.to_be_bytes()); // offset 32, stored
        d.resize(32, 0);
        d.extend_from_slice(body);
        let out = decompress_be_sges(&d, 0, d.len()).unwrap();
        assert_eq!(out, body);
    }

    #[test]
    fn be_sges_full_size_stored_segment() {
        // A 64 KB stored segment records both sizes as 0 (= 65536), the case retail console
        // blocks carry (e.g. Xbox shell.wad block 16, segment 2).
        let body: Vec<u8> = (0..65536u32).map(|i| (i * 13) as u8).collect();
        let mut d = vec![0u8; 32];
        d[0..4].copy_from_slice(SEGS_MAGIC);
        d[4..6].copy_from_slice(&4u16.to_be_bytes());
        d[6..8].copy_from_slice(&1u16.to_be_bytes());
        d[8..12].copy_from_slice(&65536u32.to_be_bytes());
        d[20..24].copy_from_slice(&32u32.to_be_bytes()); // offset 32, stored; sizes 0/0
        d.extend_from_slice(&body);
        assert_eq!(decompress_be_sges(&d, 0, d.len()).unwrap(), body);
    }

    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        /// The retail Blow It Up Again Pack's `DLC01.doh`, read out of the STFS package named by the
        /// `x360_dlc_biua_stfs` key of `.mercs2-local.toml`, against values captured from
        /// `tools/x360_dlc_io.py` on the same file. A missing key or file fails the test naming it.
        #[test]
        fn real_doh_matches_python_golden() {
            let path =
                crate::game_paths::local_config_x360_dlc_biua_stfs(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or_else(|e| panic!("{e}"));
            let stfs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let doh = crate::dlc_stfs::StfsReader::new(stfs).read_doh().expect("read_doh");

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
        d[16..18].copy_from_slice(&(deflated.len() as u16).to_be_bytes());
        d[18..20].copy_from_slice(&(body.len() as u16).to_be_bytes());
        d[20..24].copy_from_slice(&33u32.to_be_bytes()); // offset 32, bit 0 set: deflate
        d.extend_from_slice(&deflated);
        let out = decompress_be_sges(&d, 0, d.len()).unwrap();
        assert_eq!(out, body);
    }
}
