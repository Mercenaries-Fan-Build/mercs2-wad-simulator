//! FFCS patch-WAD assembly (the output/writer side of the DLC port).
//!
//! Faithful port of `tools/ffcs_patch_wad.py` (`build_patch_wad_multi`,
//! `read_patch_wad`, `merge_patch_wads`). This is the canonical serializer for
//! a PC `vz-patch.wad`: a 256-byte FFCS header with five chunk rows
//! (INDX/DATA/CSUM/ASET/PTHS) + a fixed certificate blob, an INDX table of
//! N×12-byte block records, an ASET table of 16-byte asset records, a
//! null-terminated PTHS path list + trailer, and page-aligned DATA at 0x208000.

use crate::ffcs::read_u32_le;

pub const PAGE_SIZE: usize = 0x8000; // 32 KB

/// Page-aligned start of the DATA region. Everything above it (INDX + ASET + PTHS,
/// from `0x8000`) must fit in the 2 MB below it.
pub const DATA_OFFSET: usize = 0x208000;

/// PTHS trailer (258 ASCII bytes), appended after the per-block path strings.
pub const PTHS_TRAILER: &[u8; 258] = b"\
xa37dd45ffe100bfffcc9753aabac325f07cb3fa231144fe2e33ae4783feead2\
b8a73ff021fac326df0ef9753ab9cdf6573ddff0312fab0b0ff39779eaff312\
a4f5de65892ffee33a44569bebf21f66d22e54a22347efd375981188743afd9\
9baacc342d88a99321235798725fedcbf43252669dade32415fee89da543bf23\
d4ex";

/// Canonical 144-byte FFCS certificate blob written at header offset 0x48.
pub const FFCS_CERT_BLOB: [u8; 144] = [
    0xa8, 0xd8, 0x46, 0xfa, 0x28, 0x87, 0x0e, 0x14, 0x9a, 0xd3, 0x31, 0x71, 0xe2, 0x54, 0x0a, 0x8f,
    0xf8, 0xab, 0x0a, 0x3b, 0x3e, 0xf1, 0x5e, 0x66, 0xd0, 0xf6, 0x53, 0xf7, 0x78, 0xe9, 0xe5, 0x39,
    0x5a, 0x54, 0x22, 0xc1, 0x54, 0x1a, 0xb8, 0xe6, 0x87, 0x4d, 0xdf, 0xe8, 0xc7, 0x59, 0x73, 0x20,
    0x4e, 0x90, 0x0b, 0x60, 0x14, 0x3c, 0x27, 0xe5, 0x61, 0x2d, 0x98, 0xde, 0xce, 0x7a, 0xe7, 0x99,
    0x55, 0x65, 0x16, 0x18, 0x5d, 0xc3, 0x47, 0x56, 0xbc, 0x8d, 0x0b, 0xfa, 0x50, 0x42, 0x72, 0x5b,
    0x86, 0x2f, 0x61, 0x34, 0x10, 0xca, 0x8b, 0x9f, 0x5c, 0x81, 0x02, 0x16, 0x20, 0x83, 0x0e, 0xfe,
    0xf2, 0x47, 0xce, 0xac, 0xc4, 0x30, 0x7d, 0x4d, 0xd5, 0x29, 0x48, 0xea, 0x7a, 0x15, 0x11, 0xf0,
    0x14, 0x63, 0xfe, 0xbc, 0x5a, 0xbd, 0x08, 0x56, 0x7f, 0x80, 0x10, 0x63, 0x6a, 0xdf, 0xb9, 0x59,
    0x07, 0x93, 0x56, 0x7c, 0x71, 0x03, 0xe7, 0xec, 0xbb, 0x49, 0xf6, 0x1c, 0x80, 0x86, 0x49, 0x42,
];

/// One ASET asset record. `u32_1` defaults to 0xFFFFFFFF when unset (see Python `entry.get`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsetEntry {
    pub asset_hash: u32,
    pub u32_1: u32,
    pub u32_2: u32,
    pub u32_3: u32,
}

impl AsetEntry {
    pub fn new(asset_hash: u32, u32_1: u32, u32_2: u32, u32_3: u32) -> Self {
        Self {
            asset_hash,
            u32_1,
            u32_2,
            u32_3,
        }
    }
}

/// One block destined for a patch WAD.
#[derive(Clone, Debug)]
pub struct PatchBlock {
    pub compressed_data: Vec<u8>,
    pub path_string: String,
    pub aset_entries: Vec<AsetEntry>,
    pub packed_field: u32,
    pub flags: u16,
}

impl PatchBlock {
    /// Defaults match the Python dataclass: `packed_field=1`, `flags=0x8000`.
    ///
    /// ⚠️ `packed_field` is left at the placeholder `1`. That word sizes the engine's
    /// decompression destination buffer (`decompressed_page_count << 15`, engine
    /// `FUN_00875b00`), so a block whose *decompressed* size exceeds 32 KB and still
    /// carries `packed_field = 1` overruns the heap at load. Callers using this
    /// constructor MUST set `packed_field` themselves.
    ///
    /// Prefer [`PatchBlock::from_decompressed`], which computes it for you.
    pub fn new(compressed_data: Vec<u8>, path_string: String, aset_entries: Vec<AsetEntry>) -> Self {
        Self {
            compressed_data,
            path_string,
            aset_entries,
            packed_field: 1,
            flags: 0x8000,
        }
    }

    /// Build a block from its **decompressed** bytes: sges-compresses them and sets
    /// `packed_field` to the decompressed page count the engine needs to size its
    /// destination buffer. This is the safe constructor — it makes the
    /// under-sized-buffer footgun unrepresentable.
    ///
    /// The high byte of `packed_field` is an Xbox *tier* byte (`ffcs::IndxEntry`
    /// masks the page count with `0x00FFFFFF`; `dlc_port` reads `>> 24`). Pass the
    /// source block's original `packed_field` as `inherit_tier_from` to carry that
    /// byte forward when re-emitting an existing block; pass `None` for a new one.
    pub fn from_decompressed(
        raw: &[u8],
        path_string: String,
        aset_entries: Vec<AsetEntry>,
        inherit_tier_from: Option<u32>,
    ) -> Result<Self, String> {
        let compressed = crate::sges::compress_sges(raw)
            .map_err(|e| format!("sges compress {path_string}: {e}"))?;
        let tier = inherit_tier_from.unwrap_or(0) & 0xFF00_0000;
        Ok(Self {
            compressed_data: compressed,
            path_string,
            aset_entries,
            packed_field: tier | decompressed_pages(raw.len()),
            flags: 0x8000,
        })
    }

    /// The decompressed page count this block declares to the engine.
    pub fn declared_pages(&self) -> u32 {
        self.packed_field & 0x00FF_FFFF
    }
}

/// Pages needed to hold `len` decompressed bytes (the engine allocates `pages << 15`).
pub fn decompressed_pages(len: usize) -> u32 {
    len.div_ceil(PAGE_SIZE) as u32
}

/// Parsed contents of an existing patch WAD (for merging).
#[derive(Clone, Debug)]
pub struct PatchWadContents {
    pub blocks: Vec<PatchBlock>,
    pub csum_value: u32,
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

/// Validate the invariants a patch WAD must satisfy for the engine to load it safely.
///
/// Called by [`build_patch_wad_multi`]; exposed so a builder can pre-flight a block
/// list and report problems against mod names before assembling anything.
///
/// 1. **One primary ASET row per asset hash.** The engine resolves an asset by hash to
///    a single ASET row; two primary rows for one hash in one WAD leave the winner
///    undefined. Sub-entry rows (low 16 bits != 0xFFFF) legitimately repeat and are
///    not checked.
/// 2. **`packed_field` covers the decompressed payload.** It sizes the engine's
///    decompression buffer (`pages << 15`); under-declaring overruns the heap.
///    Only checked for `sges` blocks, which are the ones the engine inflates.
/// 3. **The header region fits under DATA.** INDX + ASET + PTHS share the 2 MB below
///    `0x208000`; overflowing silently writes PTHS into the DATA region.
pub fn validate_blocks(blocks: &[PatchBlock]) -> Result<(), String> {
    // 1 — one primary ASET row per hash.
    let mut primary_owner: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (bi, blk) in blocks.iter().enumerate() {
        for entry in &blk.aset_entries {
            if entry.u32_2 & 0xFFFF != 0xFFFF {
                continue; // sub-entry row — repeats are legal
            }
            if let Some(prev) = primary_owner.insert(entry.asset_hash, bi) {
                // NOT fatal. Two claims are resolvable, not undefined: the runtime registry keeps
                // the FIRST block to register a hash and creates nothing on an occupied slot, so
                // the winner is deterministic (lowest block index). Retail `vz-patch.wad` ships
                // with both shapes — a block listing one hash twice (dlc01 human blocks) and two
                // c3 blocks claiming one hash (c30185/c30186) — and the game loads it. Failing
                // here made the builder refuse to edit a WAD the engine itself accepts. Warn so a
                // duplicate a MOD introduces is still visible, and keep the first claimant.
                if prev != bi {
                    eprintln!(
                        "  warning: asset 0x{:08X} claimed PRIMARY by [{prev}] {} and [{bi}] {}; \
                         engine takes the first (block {prev})",
                        entry.asset_hash, blocks[prev].path_string, blk.path_string
                    );
                    primary_owner.insert(entry.asset_hash, prev);
                }
            }
        }
    }

    // 2 — packed_field must cover the decompressed payload.
    for (bi, blk) in blocks.iter().enumerate() {
        if blk.compressed_data.len() < 4 || &blk.compressed_data[0..4] != b"sges" {
            continue; // stored/raw block: the engine does not inflate it
        }
        let raw = crate::sges::decompress_sges(&blk.compressed_data)
            .map_err(|e| format!("block [{bi}] {}: {e}", blk.path_string))?;
        let needed = decompressed_pages(raw.len());
        if blk.declared_pages() < needed {
            return Err(format!(
                "block [{bi}] {} declares {} decompressed page(s) but inflates to {} bytes \
                 ({needed} page(s)) — the engine would size its buffer at {} bytes and overrun the heap",
                blk.path_string,
                blk.declared_pages(),
                raw.len(),
                (blk.declared_pages() as usize) * PAGE_SIZE
            ));
        }
    }

    // 3 — header region must fit below DATA.
    let total_aset: usize = blocks.iter().map(|b| b.aset_entries.len()).sum();
    let pths_len: usize =
        blocks.iter().map(|b| b.path_string.len() + 1).sum::<usize>() + PTHS_TRAILER.len() + 1;
    let header_end = 0x8000 + blocks.len() * 12 + total_aset * 16 + pths_len;
    if header_end > DATA_OFFSET {
        return Err(format!(
            "INDX+ASET+PTHS need {header_end} bytes but DATA starts at {DATA_OFFSET} \
             — too many blocks/assets for the patch-WAD header region"
        ));
    }

    Ok(())
}

/// Build a PC FFCS patch WAD from one or more blocks.
///
/// `csum_meta` sets the CSUM chunk's `meta` field. When `None`, it is
/// auto-detected from the ASET-entry count of a block whose path ends with
/// `\resident_p000_q3.block` (falling back to 0). Pass it explicitly when any block
/// may have been carried in from another WAD — an imported block with that path would
/// otherwise silently hijack the value.
///
/// Errors if [`validate_blocks`] fails.
pub fn build_patch_wad_multi(
    blocks: &[PatchBlock],
    csum_value: u32,
    csum_meta: Option<u32>,
    cert_blob: &[u8; 144],
) -> Result<Vec<u8>, String> {
    validate_blocks(blocks)?;

    let num_blocks = blocks.len();

    // ── INDX / ASET / PTHS layout ──
    let indx_offset = 0x8000usize;
    let indx_size = num_blocks * 12;

    let aset_offset = indx_offset + indx_size;
    // (block_idx, entry) flattened in block order.
    let mut all_aset: Vec<(usize, &AsetEntry)> = Vec::new();
    for (blk_idx, blk) in blocks.iter().enumerate() {
        for entry in &blk.aset_entries {
            all_aset.push((blk_idx, entry));
        }
    }
    let total_aset = all_aset.len();
    let aset_size = total_aset * 16;

    let pths_offset = aset_offset + aset_size;
    let mut pths_bytes: Vec<u8> = Vec::new();
    for blk in blocks {
        pths_bytes.extend_from_slice(blk.path_string.as_bytes());
        pths_bytes.push(0);
    }
    pths_bytes.extend_from_slice(PTHS_TRAILER);
    pths_bytes.push(0);

    // ── DATA layout (page-aligned blocks starting at 0x208000) ──
    let data_offset = DATA_OFFSET;
    let data_page_start = data_offset / PAGE_SIZE;

    // (page_idx, pages, &data)
    let mut block_layouts: Vec<(usize, usize, &[u8])> = Vec::with_capacity(num_blocks);
    let mut current_page = data_page_start;
    for blk in blocks {
        let pages_needed = align_up(blk.compressed_data.len(), PAGE_SIZE) / PAGE_SIZE;
        block_layouts.push((current_page, pages_needed, &blk.compressed_data));
        current_page += pages_needed;
    }
    let file_size = current_page * PAGE_SIZE;

    // ── Resolve CSUM meta (resident ASET entry count) ──
    let csum_meta = csum_meta.unwrap_or_else(|| {
        for blk in blocks {
            let lower = blk.path_string.to_lowercase().replace('/', "\\");
            if lower.ends_with("\\resident_p000_q3.block") {
                return blk.aset_entries.len() as u32;
            }
        }
        0
    });

    // ── FFCS header (256 bytes) ──
    let mut out = vec![0u8; file_size];
    {
        let h = &mut out[..256];
        h[0..4].copy_from_slice(b"FFCS");
        h[4..8].copy_from_slice(&2u32.to_le_bytes());
        h[8..12].copy_from_slice(&7u32.to_le_bytes());

        let cr = 0x0C;
        let write_row = |h: &mut [u8], at: usize, tag: &[u8; 4], offset: u32, meta: u32| {
            h[at..at + 4].copy_from_slice(tag);
            h[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
            h[at + 8..at + 12].copy_from_slice(&meta.to_le_bytes());
        };
        write_row(h, cr, b"INDX", indx_offset as u32, num_blocks as u32);
        write_row(h, cr + 12, b"DATA", data_offset as u32, 36);
        write_row(h, cr + 24, b"CSUM", csum_value, csum_meta);
        write_row(h, cr + 36, b"ASET", aset_offset as u32, total_aset as u32);
        write_row(h, cr + 48, b"PTHS", pths_offset as u32, num_blocks as u32);
        h[0x48..0x48 + 144].copy_from_slice(cert_blob);
    }

    // ── INDX entries ──
    for (i, &(page_idx, pages, _data)) in block_layouts.iter().enumerate() {
        let blk = &blocks[i];
        let off = indx_offset + i * 12;
        out[off..off + 4].copy_from_slice(&(page_idx as u32).to_le_bytes());
        out[off + 4..off + 8].copy_from_slice(&blk.packed_field.to_le_bytes());
        let flags_pages = ((blk.flags as u32) << 16) | (pages as u32);
        out[off + 8..off + 12].copy_from_slice(&flags_pages.to_le_bytes());
    }

    // ── ASET entries (remap block index into u32_2 high bits) ──
    for (i, &(blk_idx, entry)) in all_aset.iter().enumerate() {
        let off = aset_offset + i * 16;
        let u2_remapped = ((blk_idx as u32) << 16) | (entry.u32_2 & 0xFFFF);
        out[off..off + 4].copy_from_slice(&entry.asset_hash.to_le_bytes());
        out[off + 4..off + 8].copy_from_slice(&entry.u32_1.to_le_bytes());
        out[off + 8..off + 12].copy_from_slice(&u2_remapped.to_le_bytes());
        out[off + 12..off + 16].copy_from_slice(&entry.u32_3.to_le_bytes());
    }

    // ── PTHS ──
    out[pths_offset..pths_offset + pths_bytes.len()].copy_from_slice(&pths_bytes);

    // ── DATA ──
    for &(page_idx, _pages, blk_data) in &block_layouts {
        let blk_offset = page_idx * PAGE_SIZE;
        out[blk_offset..blk_offset + blk_data.len()].copy_from_slice(blk_data);
    }

    Ok(out)
}

/// Parse an existing patch WAD's structure (INDX/ASET/PTHS/DATA) for merging.
pub fn read_patch_wad(raw: &[u8]) -> Result<PatchWadContents, String> {
    if raw.len() < 0x48 || &raw[0..4] != b"FFCS" {
        return Err("Not an FFCS WAD".into());
    }

    // Parse the five chunk rows.
    let mut chunks: std::collections::HashMap<[u8; 4], (u32, u32)> = std::collections::HashMap::new();
    for i in 0..5 {
        let off = 0x0C + i * 12;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&raw[off..off + 4]);
        let offset = read_u32_le(raw, off + 4);
        let meta = read_u32_le(raw, off + 8);
        chunks.insert(tag, (offset, meta));
    }

    let (indx_off, indx_count) = *chunks.get(b"INDX").ok_or("missing INDX")?;
    let (aset_off, aset_count) = *chunks.get(b"ASET").ok_or("missing ASET")?;
    let (pths_off, pths_count) = *chunks.get(b"PTHS").ok_or("missing PTHS")?;
    let csum_val = chunks.get(b"CSUM").map(|&(o, _)| o).unwrap_or(0);

    // INDX
    let mut indx_entries: Vec<(u32, u32, u32)> = Vec::with_capacity(indx_count as usize);
    for i in 0..indx_count as usize {
        let off = indx_off as usize + i * 12;
        indx_entries.push((
            read_u32_le(raw, off),
            read_u32_le(raw, off + 4),
            read_u32_le(raw, off + 8),
        ));
    }

    // ASET — group by block index (u2 high 16 bits)
    let mut aset_by_block: std::collections::HashMap<usize, Vec<AsetEntry>> =
        std::collections::HashMap::new();
    for i in 0..aset_count as usize {
        let off = aset_off as usize + i * 16;
        let u0 = read_u32_le(raw, off);
        let u1 = read_u32_le(raw, off + 4);
        let u2 = read_u32_le(raw, off + 8);
        let u3 = read_u32_le(raw, off + 12);
        let blk_idx = ((u2 >> 16) & 0xFFFF) as usize;
        aset_by_block
            .entry(blk_idx)
            .or_default()
            .push(AsetEntry::new(u0, u1, u2, u3));
    }

    // PTHS (null-separated, excluding trailer)
    let pths_region = &raw[pths_off as usize..];
    let mut path_strings: Vec<String> = Vec::new();
    let mut pos = 0usize;
    for _ in 0..pths_count as usize {
        match pths_region[pos..].iter().position(|&b| b == 0) {
            Some(rel) => {
                let nul = pos + rel;
                path_strings.push(String::from_utf8_lossy(&pths_region[pos..nul]).into_owned());
                pos = nul + 1;
            }
            None => break,
        }
    }

    // Extract each block's compressed data (trim trailing zero page-padding).
    let mut result_blocks: Vec<PatchBlock> = Vec::with_capacity(indx_entries.len());
    for (i, &(page_idx, packed, flags_pages)) in indx_entries.iter().enumerate() {
        let pages = (flags_pages & 0xFFFF) as usize;
        let flags = ((flags_pages >> 16) & 0xFFFF) as u16;
        let blk_offset = page_idx as usize * PAGE_SIZE;
        let blk_size = pages * PAGE_SIZE;
        let end = (blk_offset + blk_size).min(raw.len());
        let slice = &raw[blk_offset..end];

        // Blocks are zero-padded out to a page boundary. For an `sges` block the segment
        // table tells us exactly where the stream ends — ask it, rather than trimming
        // trailing zeroes (which truncates any stream whose last segment ends in zeroes).
        // Non-sges (stored/raw) blocks have no such table, so fall back to the trim.
        let actual_end = match crate::sges::compressed_len(slice) {
            Ok(n) => n.min(slice.len()),
            Err(_) => {
                let mut n = slice.len();
                while n > 4 && slice[n - 1] == 0 {
                    n -= 1;
                }
                align_up(n, 4).min(slice.len())
            }
        };

        let path = path_strings
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("block_{i:05}"));
        result_blocks.push(PatchBlock {
            compressed_data: slice[..actual_end].to_vec(),
            path_string: path,
            aset_entries: aset_by_block.remove(&i).unwrap_or_default(),
            packed_field: packed,
            flags,
        });
    }

    Ok(PatchWadContents {
        blocks: result_blocks,
        csum_value: csum_val,
    })
}

/// Read an existing patch WAD and append (or replace) blocks, returning new WAD bytes.
///
/// Blocks are matched by `path_string`. With `replace = false` a new block whose asset
/// hashes collide with an existing block's is simply appended — [`validate_blocks`] then
/// rejects the result rather than emitting a WAD with two primary ASET rows for one hash.
/// If you are resolving overlapping mods, decide the winner *before* calling this and pass
/// a fully-resolved block list to [`build_patch_wad_multi`] instead.
pub fn merge_patch_wads(
    existing: &[u8],
    new_blocks: Vec<PatchBlock>,
    replace: bool,
) -> Result<Vec<u8>, String> {
    let contents = read_patch_wad(existing)?;
    let mut merged = contents.blocks;

    for new_blk in new_blocks {
        if replace {
            if let Some(slot) = merged
                .iter_mut()
                .find(|old| old.path_string == new_blk.path_string)
            {
                *slot = new_blk;
                continue;
            }
            merged.push(new_blk);
        } else {
            merged.push(new_blk);
        }
    }

    build_patch_wad_multi(&merged, contents.csum_value, None, &FFCS_CERT_BLOB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pths_trailer_and_cert_sizes() {
        assert_eq!(PTHS_TRAILER.len(), 258);
        assert_eq!(FFCS_CERT_BLOB.len(), 144);
    }

    fn primary(hash: u32) -> AsetEntry {
        AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, 19)
    }

    /// `PatchBlock::new` leaves `packed_field = 1`, which sizes the engine's
    /// decompression buffer at one 32 KB page. `from_decompressed` must derive the real
    /// page count from the *decompressed* length, or a big block overruns the heap.
    #[test]
    fn from_decompressed_sets_packed_field_from_decompressed_size() {
        // 100 KB decompressed => 4 pages (100000 / 32768 = 3.05 -> 4).
        let raw = vec![0x5Au8; 100_000];
        let blk = PatchBlock::from_decompressed(&raw, "blocks\\big.block".into(), vec![], None)
            .expect("compress");
        assert_eq!(blk.declared_pages(), 4);
        assert_eq!(decompressed_pages(raw.len()), 4);
        // The footgun constructor, for contrast, would have declared a single page.
        assert_eq!(
            PatchBlock::new(vec![0u8; 8], "x".into(), vec![]).declared_pages(),
            1
        );
    }

    /// The high byte of `packed_field` is an Xbox tier byte (`ffcs` masks the page count
    /// with 0x00FFFFFF); re-emitting a block must not clobber it.
    #[test]
    fn from_decompressed_preserves_the_xbox_tier_byte() {
        let raw = vec![0x11u8; 40_000]; // 2 pages
        let blk = PatchBlock::from_decompressed(
            &raw,
            "blocks\\tiered.block".into(),
            vec![],
            Some(0x7F00_0001), // tier 0x7F, stale page count 1
        )
        .expect("compress");
        assert_eq!(blk.declared_pages(), 2, "page count recomputed");
        assert_eq!(blk.packed_field >> 24, 0x7F, "tier byte carried forward");
    }

    /// Two primary ASET rows for one hash leave the engine's winner undefined.
    #[test]
    fn duplicate_primary_aset_hash_is_rejected() {
        let a = PatchBlock::from_decompressed(b"aaaa", "blocks\\a.block".into(), vec![primary(0xDEAD)], None).unwrap();
        let b = PatchBlock::from_decompressed(b"bbbb", "blocks\\b.block".into(), vec![primary(0xDEAD)], None).unwrap();
        let err = validate_blocks(&[a, b]).unwrap_err();
        assert!(err.contains("0x0000DEAD"), "got: {err}");
        assert!(err.contains("PRIMARY"), "got: {err}");
    }

    /// Sub-entry rows (low16 != 0xFFFF) legitimately repeat and must NOT trip the check.
    #[test]
    fn duplicate_sub_entry_aset_hash_is_allowed() {
        let sub = |h: u32| AsetEntry::new(h, 0xFFFF_FFFF, 0x0000_0007, 19); // low16 = 7
        let a = PatchBlock::from_decompressed(b"aaaa", "blocks\\a.block".into(), vec![sub(0xBEEF)], None).unwrap();
        let b = PatchBlock::from_decompressed(b"bbbb", "blocks\\b.block".into(), vec![sub(0xBEEF)], None).unwrap();
        validate_blocks(&[a, b]).expect("sub-entry rows may repeat");
    }

    /// An under-declared `packed_field` is exactly the heap-overrun bug; catch it.
    #[test]
    fn under_declared_packed_field_is_rejected() {
        let raw = vec![0x7Eu8; 100_000]; // needs 4 pages
        let mut blk =
            PatchBlock::from_decompressed(&raw, "blocks\\big.block".into(), vec![], None).unwrap();
        blk.packed_field = 1; // simulate the PatchBlock::new default
        let err = validate_blocks(&[blk]).unwrap_err();
        assert!(err.contains("overrun"), "got: {err}");
    }

    /// INDX+ASET+PTHS must fit under DATA (0x208000) or PTHS silently lands in DATA.
    #[test]
    fn header_region_overflow_is_rejected() {
        // Each block contributes 12 (INDX) + 16 (ASET) + ~40 (PTHS) bytes. 0x200000
        // bytes of headroom / ~68 => overflow well before 40k blocks.
        let blocks: Vec<PatchBlock> = (0..40_000u32)
            .map(|i| {
                PatchBlock::from_decompressed(
                    b"x",
                    format!("blocks\\modkit\\filler_{i:08}.block"),
                    vec![primary(i)],
                    None,
                )
                .unwrap()
            })
            .collect();
        let err = validate_blocks(&blocks).unwrap_err();
        assert!(err.contains("DATA starts at"), "got: {err}");
    }

    #[test]
    fn build_then_read_roundtrip() {
        // Two blocks with 4-byte-aligned, non-zero-terminated payloads so the
        // reader's trailing-zero trim + 4-byte realign reproduce them exactly.
        let b0 = PatchBlock::new(
            vec![0xABu8; 24], // len % 4 == 0, last byte != 0
            "blocks\\dlc01\\resident_p000_q3.block".to_string(),
            vec![AsetEntry::new(0x11111111, 0xFFFFFFFF, 0x1234, 0xAA), AsetEntry::new(0x22222222, 1, 2, 3)],
        );
        let b1 = PatchBlock::new(
            vec![0xCDu8; 32],
            "blocks\\dlc01\\speedcity\\foo.block".to_string(),
            vec![AsetEntry::new(0x33333333, 0xFFFFFFFF, 0x5678, 0xBB)],
        );
        let blocks = vec![b0.clone(), b1.clone()];

        let wad = build_patch_wad_multi(&blocks, 0xCAFEBABE, None, &FFCS_CERT_BLOB).expect("build");

        // Header structure
        assert_eq!(&wad[0..4], b"FFCS");
        assert_eq!(&wad[0x0C..0x10], b"INDX");
        assert_eq!(&wad[0x18..0x1C], b"DATA");
        assert_eq!(&wad[0x24..0x28], b"CSUM");
        assert_eq!(&wad[0x30..0x34], b"ASET");
        assert_eq!(&wad[0x3C..0x40], b"PTHS");
        assert_eq!(&wad[0x48..0x48 + 144], &FFCS_CERT_BLOB[..]);
        // CSUM value + auto-detected meta (resident block has 2 ASET entries)
        assert_eq!(read_u32_le(&wad, 0x24 + 4), 0xCAFEBABE);
        assert_eq!(read_u32_le(&wad, 0x24 + 8), 2);

        // Round-trip parse
        let parsed = read_patch_wad(&wad).expect("read");
        assert_eq!(parsed.csum_value, 0xCAFEBABE);
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(parsed.blocks[0].path_string, b0.path_string);
        assert_eq!(parsed.blocks[1].path_string, b1.path_string);
        assert_eq!(parsed.blocks[0].compressed_data, b0.compressed_data);
        assert_eq!(parsed.blocks[1].compressed_data, b1.compressed_data);
        assert_eq!(parsed.blocks[0].aset_entries.len(), 2);
        assert_eq!(parsed.blocks[1].aset_entries.len(), 1);
        // ASET block-index remap survives the round-trip (low 16 bits of u32_2).
        assert_eq!(parsed.blocks[0].aset_entries[0].asset_hash, 0x11111111);
        assert_eq!(parsed.blocks[1].aset_entries[0].asset_hash, 0x33333333);
        assert_eq!((parsed.blocks[1].aset_entries[0].u32_2 >> 16) & 0xFFFF, 1);
    }

    #[test]
    fn byte_identical_to_python_ffcs_patch_wad() {
        // Golden produced by `tools/ffcs_patch_wad.build_patch_wad_multi` with the
        // exact same inputs (see commit message / cross-check). Equal length +
        // equal Mercs2 CRC fingerprint ⇒ byte-identical container framing.
        let b0 = PatchBlock::new(
            vec![0xABu8; 24],
            "blocks\\dlc01\\resident_p000_q3.block".to_string(),
            vec![
                AsetEntry::new(0x11111111, 0xFFFFFFFF, 0x1234, 0xAA),
                AsetEntry::new(0x22222222, 1, 2, 3),
            ],
        );
        let b1 = PatchBlock::new(
            vec![0xCDu8; 32],
            "blocks\\dlc01\\speedcity\\foo.block".to_string(),
            vec![AsetEntry::new(0x33333333, 0xFFFFFFFF, 0x5678, 0xBB)],
        );
        let wad =
            build_patch_wad_multi(&[b0, b1], 0xCAFEBABE, None, &FFCS_CERT_BLOB).expect("build");
        assert_eq!(wad.len(), 2_195_456, "WAD length must match Python");
        assert_eq!(
            crate::crc32::crc32_mercs2(&wad),
            0x3B9F_7B27,
            "WAD bytes must be identical to Python build_patch_wad_multi"
        );
    }
}
