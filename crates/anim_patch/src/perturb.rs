//! Animation-clip perturbation passes for controlled modding experiments.
//!
//! Each pass rewrites the DECOMPRESSED animgroup block bytes in place. They are
//! kept small and self-contained so new perturbation modes (retime, mask-zero,
//! bind-pose swap, …) can be added next to [`freeze_clip`] without touching the
//! WAD-build plumbing in `main.rs`.
//!
//! ## Wavelet on-disk layout (HK 5.5.0-r1, 32-bit LE)
//! The offsets below mirror the private constants in
//! `mercs2_formats::anim` (`W_OFF_*` / `WAVELET_STRUCT_SIZE`). They are the
//! on-disk `hkaWaveletSkeletalAnimation` field indices; the quantized DYNAMIC
//! coefficient stream lives in the data blob right after the 96-byte header,
//! at `quantDataIdx` for `quantDataSize` bytes. Zeroing exactly that region
//! removes all per-frame detail while leaving the header, masks, block index,
//! and static DOFs (the near-bind static pose) intact.

// On-disk wavelet struct field indices (relative to the struct start). These
// intentionally match `mercs2_formats::anim`'s private `W_OFF_*` constants.
const W_OFF_ANIM_TYPE: usize = 8;
const W_OFF_DURATION: usize = 12;
const W_OFF_NUM_TT: usize = 16;
const W_OFF_BLOCK_SIZE: usize = 40;
const W_OFF_QUANT_DATA_IDX: usize = 80;
const W_OFF_QUANT_DATA_SIZE: usize = 84;
const WAVELET_STRUCT_SIZE: usize = 96;

const HAVOK_MAGIC: [u8; 8] = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0
    }
}

#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_le(b, o))
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Locate the `hkaWaveletSkeletalAnimation` struct start within a Havok packfile
/// slice. Faithful mirror of `anim::find_wavelet_struct` (type==3 + plausible
/// duration / track-count / block-size). Returns the offset RELATIVE to `pk`.
fn find_wavelet_struct(pk: &[u8]) -> Option<usize> {
    if pk.len() < WAVELET_STRUCT_SIZE {
        return None;
    }
    // Scan the whole packfile slice (not the 4096-byte heuristic window
    // `anim::find_wavelet_struct` uses) so larger/differently-laid-out clips are
    // also frozen. The type==3 + duration/track/block-size gate is strict enough
    // to make a false positive vanishingly unlikely.
    let limit = pk.len() - WAVELET_STRUCT_SIZE;
    let mut off = 0usize;
    while off < limit {
        if u32_le(pk, off + W_OFF_ANIM_TYPE) != 3 {
            off += 4;
            continue;
        }
        let d = f32_le(pk, off + W_OFF_DURATION);
        if !(d.is_finite() && (0.001..=600.0).contains(&d)) {
            off += 4;
            continue;
        }
        let ntt = u32_le(pk, off + W_OFF_NUM_TT);
        if !(1..=500).contains(&ntt) {
            off += 4;
            continue;
        }
        let bs = u32_le(pk, off + W_OFF_BLOCK_SIZE);
        if !matches!(bs, 2 | 4 | 8 | 16 | 32 | 64) {
            off += 4;
            continue;
        }
        return Some(off);
    }
    None
}

/// The byte range (absolute, in the decompressed block) that a `--freeze` pass
/// zeroed for one clip.
#[derive(Debug, Clone, Copy)]
pub struct ZeroedRange {
    pub name_hash: u32,
    pub start: usize,
    pub len: usize,
}

/// Zero the DYNAMIC quantized wavelet coefficient stream of one clip, in place.
///
/// `block` is the whole decompressed animgroup block; `havok_offset` is the
/// clip's `ClipEntry::havok_offset` (absolute start of its Havok packfile). The
/// header, static/dynamic masks, block index, static DOFs, and quantization
/// descriptors are left untouched — only the `quantData` blob (the per-frame
/// detail) is set to zero, holding a static / near-bind pose.
///
/// Returns the absolute byte range zeroed, or `None` for a clip that is not a
/// locatable wavelet animation (interleaved/delta/spline are skipped).
pub fn freeze_clip(
    block: &mut [u8],
    name_hash: u32,
    havok_offset: usize,
    clip_end: usize,
) -> Option<ZeroedRange> {
    if havok_offset >= block.len() {
        return None;
    }
    let pk_start = havok_offset;
    // Bound the search to THIS clip's packfile (`clip_end` = the next clip's
    // havok_offset, or the block end) so a miss never reaches into the next
    // clip's data and zeroes the wrong region.
    let end = clip_end.min(block.len());
    if end <= pk_start {
        return None;
    }
    // The wavelet finder walks from the packfile start; scan from the Havok
    // magic if the offset points slightly before/after it (robust to the exact
    // container→packfile base like `anim::parse_anim`).
    let scan_base = find_sub(&block[pk_start..end], &HAVOK_MAGIC)
        .map(|rel| pk_start + rel)
        .unwrap_or(pk_start);
    let pk = &block[scan_base..end];
    let so = find_wavelet_struct(pk)?;

    let qd_idx = u32_le(pk, so + W_OFF_QUANT_DATA_IDX) as usize;
    let qd_size = u32_le(pk, so + W_OFF_QUANT_DATA_SIZE) as usize;
    if qd_size == 0 {
        return None;
    }
    // Data blob begins right after the 96-byte header; quantData is qd_idx into it.
    let db = so + WAVELET_STRUCT_SIZE;
    let start_abs = scan_base + db + qd_idx;
    let end_abs = start_abs.checked_add(qd_size)?;
    if end_abs > block.len() {
        return None;
    }
    for byte in &mut block[start_abs..end_abs] {
        *byte = 0;
    }
    Some(ZeroedRange {
        name_hash,
        start: start_abs,
        len: qd_size,
    })
}
