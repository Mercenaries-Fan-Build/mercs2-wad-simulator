//! **PHY2 MOPP swap** — replace the baked `hkpMoppCode` in a real Mercs2 collision chunk with a
//! freshly-compiled MOPP and rebuild a *structurally valid* Havok packfile, entirely offline.
//!
//! This is the bridge between the native MOPP compiler ([`crate::mopp`]) and an in-game collision
//! proof: given a cell's `PHY2` body we locate one `hkpMoppCode`, swap its `m_data` bytecode (and,
//! for the spatial mode, its `m_info` quantization frame), and re-emit the packfile so it re-parses
//! and re-decodes cleanly. The overlay / in-game step is deliberately NOT done here.
//!
//! ## PHY2 framing (pinned on retail `vz.wad` blocks 767 / 826)
//! ```text
//!  [ u32 prefix (48 B) ][ Havok packfile (magic 57E0E057…) ][ trailing engine wrapper ]
//!    prefix[8] @ byte 32 == Havok packfile size (bounds the packfile; the wrapper follows it)
//!    the WpMeshShape16 quantized vertex pool lives in the trailing wrapper (0xAAAAAAAA-filled)
//! ```
//! The packfile is always at `body+48`; the trailing wrapper begins at `48 + packfile_size`.
//!
//! ## `hkpMoppCode` on-disk layout (32-bit PC, PROVEN)
//! ```text
//!  obj+16  hkVector4 m_info  = [offset.x, offset.y, offset.z, 1/scale(lane3)]
//!  obj+32  hkArray m_data    : ptr (local fixup) / count u32 @+36 / capAndFlags u32 @+40
//!  obj+44  hkInt8  m_buildType (== 1 on every retail cell)
//! ```
//! Every retail `m_data` array is 16-byte aligned with `capAndFlags == 0xC0000000 | count`
//! (LOCKED | DONT_DEALLOCATE flags, capacity == count).
//!
//! ## Rewrite strategy (why append, not splice-in-place)
//! The `m_data` buffers are scattered *through* the `__data__` object region (verified: a cell holds
//! a dozen MOPPs interleaved with objects), NOT at the end — so growing one in place would shift every
//! following object and invalidate every fixup. Instead:
//! - **new ≤ old:** overwrite the bytes in place, shrink `count`/`capAndFlags`. Zero shift — the packfile
//!   size, the prefix, the trailing wrapper and the container CSUM are all untouched.
//! - **new > old:** *append* the new buffer at the end of the `__data__` body (before the fixup tables),
//!   repoint the single `m_data` pointer's local fixup, and grow the `__data__` section-header offsets +
//!   the packfile size + `prefix[8]`. The old bytes become dead padding (nothing references them). No
//!   existing object moves, so every other fixup stays valid. The trailing wrapper is preserved verbatim
//!   and simply relocated after the larger packfile (its own intra-wrapper offsets are byte-identical —
//!   see the caveat in the module tests re: whether the game rebases them; that is an in-game concern for
//!   the NEXT step, not this offline gate).

use crate::havok::{parse_packfile_raw, HAVOK_MAGIC};
use crate::mopp;

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Which MOPP to write into the target `hkpMoppCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapMode {
    /// Re-serialize the SAME decoded MOPP bytes (a pure plumbing check — output must round-trip to the
    /// identical key multiset, and, when size is preserved, be byte-identical to the source).
    Identity,
    /// `encode_return_all(N)` where `N` = the source MOPP's key (triangle) count. Index-only, no verts.
    ReturnAll,
    /// The **emit-nothing** MOPP: a single `0x00` (RETURN) at the root (`encode_return_all(0)`). Decodes
    /// to ZERO shape keys, so the broadphase returns no triangle candidates for any query and the shape
    /// contributes no collision — the player falls through. Structurally a valid `hkpMoppCode` (one
    /// RETURN opcode, count = 1); the co-located `WpMeshShape16` is untouched. Negative control for the
    /// in-game ladder: it isolates "our patched block drives this cell's collision".
    Empty,
    /// `encode(tris, verts)` over the co-located `WpMeshShape16` (matched by triangle count == N). Also
    /// rewrites `m_info`. Carries the known frame-fidelity caveat (our root shift = 8 vs the cell's
    /// baked frame); the offline gate still proves a clean self-consistent round-trip.
    Spatial,
}

/// What the swap did — the numbers the offline report prints.
#[derive(Debug, Clone)]
pub struct MoppSwapReport {
    pub mode: SwapMode,
    /// Index of the targeted `hkpMoppCode` among the body's MOPPs (virtual-fixup order).
    pub mopp_index: usize,
    /// Total `hkpMoppCode` objects in the body.
    pub mopp_count: usize,
    /// Source MOPP's decoded key (triangle) count `N`.
    pub source_key_count: usize,
    /// `true` when the source keys are a perfect contiguous `[0..N-1]` run (single-subpart cell).
    pub source_keys_contiguous: bool,
    pub old_buf_len: usize,
    pub new_buf_len: usize,
    /// `true` when the new buffer was APPENDED (grew the packfile); `false` = in-place overwrite.
    pub grew: bool,
    pub old_packfile_size: usize,
    pub new_packfile_size: usize,
    /// Whether `m_info` was rewritten (spatial only).
    pub info_rewritten: bool,
}

impl MoppSwapReport {
    pub fn size_delta(&self) -> i64 {
        self.new_packfile_size as i64 - self.old_packfile_size as i64
    }
}

/// Locate the `hkpMoppCode` objects in a PHY2 body, in virtual-fixup order. Returns
/// `(packfile_offset, RawPackfile, Vec<mopp_src>)`.
fn locate_mopps(body: &[u8]) -> Result<(usize, crate::havok::RawPackfile, Vec<usize>), String> {
    let off = find_sub(body, &HAVOK_MAGIC).ok_or("PHY2 body has no embedded Havok packfile")?;
    let raw = parse_packfile_raw(&body[off..])?;
    let mopps: Vec<usize> = raw
        .vfixups
        .iter()
        .filter(|(_, c)| c == "hkpMoppCode")
        .map(|(s, _)| *s)
        .collect();
    if mopps.is_empty() {
        return Err("PHY2 body contains no hkpMoppCode object".into());
    }
    Ok((off, raw, mopps))
}

/// Decode the `m_data` bytecode of the `hkpMoppCode` at `mopp_index` → its shape keys. Reusable for
/// both source inspection and post-swap verification.
pub fn decode_phy2_mopp_keys(body: &[u8], mopp_index: usize) -> Result<mopp::Decoded, String> {
    let (off, raw, mopps) = locate_mopps(body)?;
    let src = *mopps
        .get(mopp_index)
        .ok_or_else(|| format!("mopp_index {mopp_index} out of range (body has {})", mopps.len()))?;
    let pk = &body[off..];
    let ptr = raw
        .resolve_ptr(src, 32)
        .ok_or("hkpMoppCode m_data pointer is unrelocated (null array)")?;
    let count = u32_le(pk, raw.data_pk + src + 36) as usize;
    if ptr + count > pk.len() {
        return Err("hkpMoppCode m_data buffer runs past the packfile".into());
    }
    Ok(mopp::decode(&pk[ptr..ptr + count]))
}

/// The count of `hkpMoppCode` objects in a PHY2 body (0 if none / not a packfile PHY2).
pub fn count_phy2_mopps(body: &[u8]) -> usize {
    locate_mopps(body).map(|(_, _, m)| m.len()).unwrap_or(0)
}

/// Build the replacement `m_data` buffer (and, for spatial, the new `MoppInfo`) for one MOPP.
fn build_replacement(
    body: &[u8],
    off: usize,
    raw: &crate::havok::RawPackfile,
    src: usize,
    mode: SwapMode,
    source_keys: &[u32],
) -> Result<(Vec<u8>, Option<mopp::MoppInfo>), String> {
    let pk = &body[off..];
    let n = {
        let mut ks = source_keys.to_vec();
        ks.sort_unstable();
        ks.dedup();
        ks.len()
    };
    match mode {
        SwapMode::Identity => {
            let ptr = raw.resolve_ptr(src, 32).ok_or("null m_data")?;
            let count = u32_le(pk, raw.data_pk + src + 36) as usize;
            Ok((pk[ptr..ptr + count].to_vec(), None))
        }
        SwapMode::ReturnAll => Ok((mopp::encode_return_all(n as u32), None)),
        // Emit-nothing: a lone RETURN (`encode_return_all(0)` == `[0x00]`). No verts needed.
        SwapMode::Empty => Ok((mopp::encode_return_all(0), None)),
        SwapMode::Spatial => {
            // Pair the MOPP with the co-located WpMeshShape16 whose triangle count == N (single-subpart
            // cell). Decode the whole body's shapes and match by triangle count.
            let pfile = crate::havok::parse_packfile(pk)?;
            let mesh = pfile
                .shapes
                .iter()
                .filter_map(|s| match s {
                    crate::havok::Shape::Mesh(m) if !m.indices.is_empty() => Some(m),
                    _ => None,
                })
                .find(|m| m.indices.len() == n)
                .ok_or_else(|| {
                    format!("spatial: no WpMeshShape16 with exactly {n} triangles to pair with this MOPP")
                })?;
            let tris: Vec<[u32; 3]> = mesh
                .indices
                .iter()
                .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
                .collect();
            let (code, info) = mopp::encode(&tris, &mesh.vertices);
            Ok((code, Some(info)))
        }
    }
}

/// Replace the `hkpMoppCode` at `mopp_index` with a MOPP per `mode`, returning the rebuilt PHY2 body
/// and a report. The base body is never mutated.
pub fn swap_phy2_mopp(
    body: &[u8],
    mopp_index: usize,
    mode: SwapMode,
) -> Result<(Vec<u8>, MoppSwapReport), String> {
    let (off, raw, mopps) = locate_mopps(body)?;
    let mopp_count = mopps.len();
    let src = *mopps
        .get(mopp_index)
        .ok_or_else(|| format!("mopp_index {mopp_index} out of range (body has {mopp_count})"))?;

    // Source decode → N + contiguity.
    let src_dec = decode_phy2_mopp_keys(body, mopp_index)?;
    if src_dec.error.is_some() {
        return Err(format!("source MOPP does not decode clean: {:?}", src_dec.error));
    }
    let (src_keys, range, missing) = src_dec.key_summary();
    let source_key_count = src_keys.len();
    let source_keys_contiguous = missing.is_empty() && range == Some((0, source_key_count as u32 - 1));

    let old_packfile_size = raw.size;
    let old_ptr_rel = raw.resolve_ptr(src, 32).ok_or("null m_data")?; // relative to pk (== data_pk + dst)
    let old_dst = old_ptr_rel - raw.data_pk; // data-body-relative
    let old_count = u32_le(&body[off..], raw.data_pk + src + 36) as usize;

    // Build the replacement bytes (+ optional new frame).
    let (new_buf, new_info) = build_replacement(body, off, &raw, src, mode, &src_keys)?;
    let new_len = new_buf.len();

    // The packfile slice and the trailing wrapper (preserved verbatim).
    let pk_start = off;
    let pk_end = off + old_packfile_size;
    if pk_end > body.len() {
        return Err("packfile size exceeds body (corrupt prefix?)".into());
    }
    let prefix = &body[..off];
    let trailing = &body[pk_end..];

    let mut new_pk = body[pk_start..pk_end].to_vec();
    let cap_flags = u32_le(&new_pk, raw.data_pk + src + 40) & 0xC000_0000;

    // Always rewrite count + capAndFlags to the new length.
    new_pk[raw.data_pk + src + 36..raw.data_pk + src + 40].copy_from_slice(&(new_len as u32).to_le_bytes());
    new_pk[raw.data_pk + src + 40..raw.data_pk + src + 44]
        .copy_from_slice(&(cap_flags | (new_len as u32 & 0x3FFF_FFFF)).to_le_bytes());

    // Spatial: rewrite m_info (offset.xyz @ +16/+20/+24, lane3 = 1/scale @ +28).
    let mut info_rewritten = false;
    if let Some(info) = new_info {
        let base = raw.data_pk + src + 16;
        for (k, v) in info.offset.iter().enumerate() {
            new_pk[base + k * 4..base + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        let lane3 = if info.scale.abs() > 0.0 { 1.0 / info.scale } else { 0.0f32 };
        new_pk[base + 12..base + 16].copy_from_slice(&lane3.to_le_bytes());
        info_rewritten = true;
    }

    let grew;
    let new_packfile_size;
    if new_len <= old_count {
        // ── In-place overwrite: zero shift. ──
        grew = false;
        let dst_abs = raw.data_pk + old_dst; // relative to pk == new_pk
        new_pk[dst_abs..dst_abs + new_len].copy_from_slice(&new_buf);
        // (bytes in [dst+new_len, dst+old_count) stay as dead old-buffer tail — unreferenced.)
        new_packfile_size = old_packfile_size;
    } else {
        // ── Append at the end of the __data__ body, before the fixup tables. ──
        grew = true;
        // Section-header table lives just after the searched "__classnames__" tag.
        let sh = find_sub(&new_pk, b"__classnames__").ok_or("packfile lost __classnames__")?;
        let data_hdr = sh + 2 * 48; // __data__ is the 3rd (index 2) section header
        // 7 body-relative u32s at +20: [abs, lf, gf, vf, exp, imp, end].
        let field = |k: usize| data_hdr + 20 + k * 4;
        let d_lf = u32_le(&new_pk, field(1)) as usize; // == __data__ body length
        // New buffer goes at body-relative offset d_lf (already 16-aligned on retail). Pad to 16.
        let new_dst = d_lf;
        debug_assert_eq!(new_dst % 16, 0, "retail __data__ body length is 16-aligned");
        let mut insert = new_buf.clone();
        while insert.len() % 16 != 0 {
            insert.push(0);
        }
        let insert_len = insert.len();

        // Repoint the m_data pointer's LOCAL fixup dst (src field == obj_src+32) to new_dst. The local
        // fixup table starts at data_pk + d_lf; entries are {u32 src, u32 dst}, terminated by 0xFFFFFFFF.
        let lf_table = raw.data_pk + d_lf;
        let want_src = (src + 32) as u32;
        let mut k = lf_table;
        let mut patched = false;
        while k + 8 <= new_pk.len() {
            let s = u32_le(&new_pk, k);
            if s == 0xFFFF_FFFF {
                break;
            }
            if s == want_src {
                new_pk[k + 4..k + 8].copy_from_slice(&(new_dst as u32).to_le_bytes());
                patched = true;
                break;
            }
            k += 8;
        }
        if !patched {
            return Err("could not find the m_data local fixup to repoint".into());
        }

        // Grow every __data__ body-relative offset field after the body (lf,gf,vf,exp,imp,end) by the
        // inserted length; `abs` (field 0) is unchanged.
        for kf in 1..7 {
            let v = u32_le(&new_pk, field(kf)) as usize + insert_len;
            new_pk[field(kf)..field(kf) + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }

        // Splice the buffer in at the body end (== start of the local-fixup table, at pk offset lf_table).
        let mut spliced = Vec::with_capacity(new_pk.len() + insert_len);
        spliced.extend_from_slice(&new_pk[..lf_table]);
        spliced.extend_from_slice(&insert);
        spliced.extend_from_slice(&new_pk[lf_table..]);
        new_pk = spliced;
        new_packfile_size = old_packfile_size + insert_len;
    }

    // Reassemble the PHY2 body: prefix (with packfile-size @ byte 32 updated) + packfile + trailing.
    let mut new_body = Vec::with_capacity(prefix.len() + new_pk.len() + trailing.len());
    new_body.extend_from_slice(prefix);
    new_body.extend_from_slice(&new_pk);
    new_body.extend_from_slice(trailing);
    // prefix[8] @ byte 32 == Havok packfile size (verified on retail 767/826). Only present when the
    // prefix is the full 48-byte header (off >= 36); guard defensively.
    if off >= 36 && u32_le(&new_body, 32) as usize == old_packfile_size {
        new_body[32..36].copy_from_slice(&(new_packfile_size as u32).to_le_bytes());
    } else if grew && off >= 36 {
        return Err(format!(
            "prefix packfile-size field not at byte 32 (found {}); refusing to grow without updating it",
            u32_le(&new_body, 32)
        ));
    }

    Ok((
        new_body,
        MoppSwapReport {
            mode,
            mopp_index,
            mopp_count,
            source_key_count,
            source_keys_contiguous,
            old_buf_len: old_count,
            new_buf_len: new_len,
            grew,
            old_packfile_size,
            new_packfile_size,
            info_rewritten,
        },
    ))
}

/// Result of the offline gate over a swapped body.
#[derive(Debug, Clone)]
pub struct OfflineGate {
    pub reparse_ok: bool,
    pub reparse_err: Option<String>,
    pub mopp_present: bool,
    pub decode_clean: bool,
    pub decode_coverage_full: bool,
    /// Identity: same key multiset as source. ReturnAll: exactly `[0..N)`. Spatial: a clean superset
    /// whose distinct keys are `[0..N)` (encoder emits each triangle index once).
    pub keys_as_expected: bool,
    /// The mesh (WpMeshShape16) still decodes → the trailing wrapper / vertex pool survived the rewrite.
    pub mesh_still_decodes: bool,
    pub decoded_key_count: usize,
}

/// Re-parse a swapped body from scratch and check: the packfile is structurally valid, the target MOPP
/// is present and decodes cleanly with the expected keys, and the co-located mesh still decodes.
///
/// `expected_keys` is the source key multiset (for identity) or `None` to expect `[0..N)` (return-all
/// / spatial, where `N` = distinct decoded count).
pub fn validate_swapped_body(
    body: &[u8],
    mopp_index: usize,
    mode: SwapMode,
    expected_keys: Option<&[u32]>,
) -> OfflineGate {
    let mut g = OfflineGate {
        reparse_ok: false,
        reparse_err: None,
        mopp_present: false,
        decode_clean: false,
        decode_coverage_full: false,
        keys_as_expected: false,
        mesh_still_decodes: false,
        decoded_key_count: 0,
    };

    // 1. Structural re-parse (fixups resolve, sections walk).
    let pf = match crate::havok::parse_phy2_body(body) {
        Ok(pf) => {
            g.reparse_ok = true;
            pf
        }
        Err(e) => {
            g.reparse_err = Some(e);
            return g;
        }
    };
    g.mopp_present = pf.class_counts.get("hkpMoppCode").copied().unwrap_or(0) > 0;
    g.mesh_still_decodes = pf
        .shapes
        .iter()
        .any(|s| matches!(s, crate::havok::Shape::Mesh(m) if !m.indices.is_empty()));

    // 2. Re-decode the swapped MOPP.
    let dec = match decode_phy2_mopp_keys(body, mopp_index) {
        Ok(d) => d,
        Err(_) => return g,
    };
    g.decode_clean = dec.error.is_none();
    // coverage is checked against the freshly-extracted buffer length inside decode; re-extract to compare.
    if let Ok((off, raw, mopps)) = locate_mopps(body) {
        if let Some(&src) = mopps.get(mopp_index) {
            let pk = &body[off..];
            if raw.resolve_ptr(src, 32).is_some() {
                let count = u32_le(pk, raw.data_pk + src + 36) as usize;
                g.decode_coverage_full = dec.consumed == count;
            }
        }
    }

    let (ks, range, missing) = dec.key_summary();
    g.decoded_key_count = ks.len();
    g.keys_as_expected = match (mode, expected_keys) {
        (SwapMode::Identity, Some(exp)) => {
            let mut e = exp.to_vec();
            e.sort_unstable();
            e.dedup();
            ks == e
        }
        // Emit-nothing: the swapped MOPP must decode to EXACTLY zero keys (a lone RETURN).
        (SwapMode::Empty, _) => ks.is_empty(),
        _ => {
            // return-all / spatial: distinct keys must be a perfect [0..N-1] run.
            !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1))
        }
    };
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    // Load a real PHY2 body from a vz.wad block container that carries a MOPP.
    fn load_phy2_with_mopp(block: u16) -> Option<Vec<u8>> {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let path = crate::game_paths::vz_wad(std::path::Path::new("."))?;
        let mut f = std::fs::File::open(&path).ok()?;
        let size = f.metadata().ok()?.len();
        let arch = load_ffcs_archive(&mut f, size).ok()?;
        let dec = decompress_block(&mut f, &arch.indx, block).ok()?;
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if let Some(body) = extract_chunk_body(&dec[pos..end], b"PHY2") {
                if count_phy2_mopps(&body) > 0 {
                    return Some(body);
                }
            }
            pos = end;
        }
        None
    }

    /// Pick the first MOPP index whose source keys are a perfect contiguous [0..N-1] run (single-subpart
    /// cell) — the mission's "clean keys" target.
    fn first_contiguous_mopp(body: &[u8]) -> Option<usize> {
        for i in 0..count_phy2_mopps(body) {
            if let Ok(d) = decode_phy2_mopp_keys(body, i) {
                let (ks, range, missing) = d.key_summary();
                if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1)) {
                    return Some(i);
                }
            }
        }
        None
    }

    #[test]
    fn identity_swap_is_byte_identical_and_reparses_if_wad_present() {
        let Some(body) = load_phy2_with_mopp(767) else {
            return eprintln!("SKIPPING identity_swap: vz.wad not found / no MOPP PHY2 in block 767");
        };
        let idx = first_contiguous_mopp(&body).unwrap_or(0);
        let src = decode_phy2_mopp_keys(&body, idx).unwrap();
        let (src_keys, _, _) = src.key_summary();

        let (new_body, rep) = swap_phy2_mopp(&body, idx, SwapMode::Identity).expect("identity swap");
        // Identity is size-preserving → BYTE-IDENTICAL to the source (the strongest plumbing proof).
        assert_eq!(new_body, body, "identity re-serialize must be byte-identical");
        assert!(!rep.grew, "identity must not grow");
        assert_eq!(rep.old_packfile_size, rep.new_packfile_size);

        let g = validate_swapped_body(&new_body, idx, SwapMode::Identity, Some(&src_keys));
        assert!(g.reparse_ok && g.mopp_present, "identity must re-parse with the MOPP present: {:?}", g.reparse_err);
        assert!(g.decode_clean && g.decode_coverage_full, "identity MOPP must decode clean & fully covered");
        assert!(g.keys_as_expected, "identity keys must match the source multiset");
        eprintln!(
            "identity[block 767 mopp {idx}]: {} keys, contiguous={}, {} B packfile unchanged",
            rep.source_key_count, rep.source_keys_contiguous, rep.old_packfile_size
        );
    }

    #[test]
    fn return_all_swap_reparses_and_decodes_full_range_if_wad_present() {
        let Some(body) = load_phy2_with_mopp(767) else {
            return eprintln!("SKIPPING return_all_swap: vz.wad not found");
        };
        let idx = first_contiguous_mopp(&body).unwrap_or(0);
        let (new_body, rep) = swap_phy2_mopp(&body, idx, SwapMode::ReturnAll).expect("return-all swap");

        let g = validate_swapped_body(&new_body, idx, SwapMode::ReturnAll, None);
        assert!(g.reparse_ok && g.mopp_present, "return-all must re-parse: {:?}", g.reparse_err);
        assert!(g.decode_clean && g.decode_coverage_full, "return-all MOPP must decode clean & fully covered");
        assert!(g.keys_as_expected, "return-all must decode to a perfect [0..N-1] run");
        assert_eq!(g.decoded_key_count, rep.source_key_count, "same triangle count as the source");
        assert!(g.mesh_still_decodes, "the WpMeshShape16 vertex pool must survive the rewrite");
        // If it grew, the packfile size + prefix must have been recomputed (never a blind in-place swap).
        if rep.grew {
            assert!(rep.new_packfile_size > rep.old_packfile_size);
            assert_eq!(u32_le(&new_body, 32) as usize, rep.new_packfile_size, "prefix[8] must equal the new packfile size");
        }
        eprintln!(
            "return-all[block 767 mopp {idx}]: N={} keys, buf {}→{} B, packfile {}→{} B (grew={})",
            rep.source_key_count, rep.old_buf_len, rep.new_buf_len, rep.old_packfile_size, rep.new_packfile_size, rep.grew
        );
    }

    #[test]
    fn empty_swap_reparses_and_decodes_to_zero_keys_if_wad_present() {
        let Some(body) = load_phy2_with_mopp(767) else {
            return eprintln!("SKIPPING empty_swap: vz.wad not found");
        };
        let idx = first_contiguous_mopp(&body).unwrap_or(0);
        let (new_body, rep) = swap_phy2_mopp(&body, idx, SwapMode::Empty).expect("empty swap");
        // Emit-nothing is a single 0x00 byte (count = 1) → strictly ≤ any real m_data, so it is always
        // an in-place overwrite: zero shift, packfile/prefix/wrapper untouched.
        assert!(!rep.grew, "empty must never grow (1-byte buffer)");
        assert_eq!(rep.new_buf_len, 1, "emit-nothing m_data is a lone RETURN byte");
        assert_eq!(rep.old_packfile_size, rep.new_packfile_size, "empty must preserve packfile size");

        let g = validate_swapped_body(&new_body, idx, SwapMode::Empty, None);
        assert!(g.reparse_ok && g.mopp_present, "empty must re-parse with the MOPP present: {:?}", g.reparse_err);
        assert!(g.decode_clean && g.decode_coverage_full, "empty MOPP must decode clean & fully covered");
        assert!(g.keys_as_expected, "empty must decode to ZERO keys");
        assert_eq!(g.decoded_key_count, 0, "emit-nothing yields no shape keys");
        assert!(g.mesh_still_decodes, "the WpMeshShape16 vertex pool must survive the empty swap");
        eprintln!(
            "empty[block 767 mopp {idx}]: {} src keys → 0 keys, buf {}→1 B, packfile {} B unchanged",
            rep.source_key_count, rep.old_buf_len, rep.old_packfile_size
        );
    }

    #[test]
    fn spatial_swap_reparses_and_roundtrips_if_wad_present() {
        let Some(body) = load_phy2_with_mopp(767) else {
            return eprintln!("SKIPPING spatial_swap: vz.wad not found");
        };
        // Spatial needs a MOPP paired with a decodable WpMeshShape16 of the same triangle count.
        let mut done = false;
        for idx in 0..count_phy2_mopps(&body) {
            match swap_phy2_mopp(&body, idx, SwapMode::Spatial) {
                Ok((new_body, rep)) => {
                    let g = validate_swapped_body(&new_body, idx, SwapMode::Spatial, None);
                    assert!(g.reparse_ok && g.mopp_present, "spatial must re-parse: {:?}", g.reparse_err);
                    assert!(g.decode_clean && g.decode_coverage_full, "spatial MOPP must decode clean & covered");
                    assert!(g.keys_as_expected, "spatial must decode to [0..N-1]");
                    assert!(g.mesh_still_decodes, "the mesh pool must survive the spatial rewrite");
                    assert!(rep.info_rewritten, "spatial must rewrite m_info");
                    if rep.grew {
                        assert_eq!(u32_le(&new_body, 32) as usize, rep.new_packfile_size);
                    }
                    eprintln!(
                        "spatial[block 767 mopp {idx}]: N={}, buf {}→{} B, packfile {}→{} B (grew={}), m_info rewritten",
                        rep.source_key_count, rep.old_buf_len, rep.new_buf_len, rep.old_packfile_size, rep.new_packfile_size, rep.grew
                    );
                    done = true;
                    break;
                }
                Err(_) => continue, // this MOPP had no same-count mesh; try the next
            }
        }
        assert!(done, "no MOPP in block 767 could be paired with a same-count WpMeshShape16 for spatial");
    }
}
