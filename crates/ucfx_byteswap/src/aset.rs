//! ASET packed_block_ref (`u2`) sub-entry computation.
//!
//! Faithful Rust port of the Python logic in `tools/dlc_port.py`
//! (`_strip_xbox_sub_entry` + `_recompute_aset_sub_entries`) and the writer's
//! final compose in `tools/ffcs_patch_wad.py`
//! (`u2 = (block_index << 16) | (u32_2 & 0xFFFF)`).
//!
//! ## Field layout
//! The PC/LE ASET packed field is `{ block_index : hi16, sub : lo16 }`.
//! The Xbox/BE source field is the **inverse**: `{ sub : hi16, block : lo16 }`
//! (verified against the base BE↔LE pair). The Python pipeline mislabels these,
//! which is the root of the recurring sub-field bugs — this module keeps the
//! distinction explicit.
//!
//! ## Staging (per the porting plan)
//! STEP 1 (this file): reproduce the *current* Python behavior **exactly** so we
//! can prove byte-parity of the emitted WAD before changing anything. The known
//! bug (faithful BE-sub not preserved: primaries got the physical index; then
//! body-less stubs got force-`0xFFFF`, clobbering real sub-offsets) is **left
//! intact here on purpose** and fixed in STEP 3 once parity is confirmed.

/// Sentinel meaning "primary reference / resolve the body by hash".
pub const SUB_PRIMARY: u16 = 0xFFFF;

/// Strip the Xbox low16 from the packed field and set it to `0xFFFF`.
///
/// Faithful port of Python `_strip_xbox_sub_entry`:
/// ```py
/// return (u2 & 0xFFFF0000) | 0xFFFF
/// ```
#[inline]
pub fn strip_xbox_sub_entry(u2: u32) -> u32 {
    (u2 & 0xFFFF_0000) | 0xFFFF
}

/// One ASET row's mutable state during the sub-entry recompute.
#[derive(Debug, Clone)]
pub struct AsetEntry {
    pub asset_hash: u32,
    /// Working packed value. Only the low16 (`sub`) is meaningful to the
    /// recompute; the writer re-composes the block index into the high16.
    pub u32_2: u32,
    /// True for entries created on the "global / resolve-by-hash" path or as
    /// synthetic script/stringdb rows (Python sets `_primary = True`).
    pub primary: bool,
    /// True if this asset hash also exists in the base game WAD. A body-less
    /// stub whose hash is in base resolves its real body BY HASH (→ keep
    /// 0xFFFF); a DLC-new stub has no base fallback, so its BE sub-offset is
    /// the only locator we can preserve.
    pub in_base: bool,
}

impl AsetEntry {
    #[inline]
    pub fn sub(&self) -> u16 {
        (self.u32_2 & 0xFFFF) as u16
    }
    #[inline]
    pub fn set_sub(&mut self, sub: u16) {
        self.u32_2 = (self.u32_2 & 0xFFFF_0000) | sub as u32;
    }
}

/// Counts returned by [`recompute_block_aset_subs`], mirroring the Python log
/// line `"{preserved} primary kept-0xFFFF, {resolved} non-primary->index, …"`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecomputeCounts {
    pub preserved: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

/// Whether the preserve-primary behavior is enabled. Matches the Python module
/// constant `_PRESERVE_PRIMARY_ASET_SUB` (currently `True`).
pub const PRESERVE_PRIMARY_ASET_SUB: bool = true;

/// Faithful port of the per-block body of `_recompute_aset_sub_entries`.
///
/// `decompressed` is the block's decompressed UCFX bytes; `entries` are the
/// ASET rows attached to that block. Updates each entry's `u32_2` low16 in
/// place and returns the (preserved, resolved, unresolved) counts.
///
/// Entry-table layout (matches the Python parse): `entry_count = u32@0`, then
/// `entry_count` rows of `{ u32 size, u32 name_hash, u32 type, u32 pad }` at
/// `i*16`, with the per-entry bodies concatenated starting at `entry_count*16`.
/// A body-less **stub** is detected by the absence of a `BODY` chunk or a
/// `BODY` length field equal to `0xFFFFFFFF`.
pub fn recompute_block_aset_subs(decompressed: &[u8], entries: &mut [AsetEntry]) -> RecomputeCounts {
    let mut counts = RecomputeCounts::default();
    if decompressed.len() < 4 || entries.is_empty() {
        return counts;
    }
    let entry_count = rd_u32(decompressed, 0) as usize;
    if entry_count < 1 || entry_count > 50_000 {
        return counts;
    }

    // Build hash -> first index, and hash -> is_stub, walking the entry table
    // and the concatenated bodies (cumulative by per-entry size).
    use std::collections::HashMap;
    let mut hash_to_index: HashMap<u32, usize> = HashMap::new();
    let mut hash_is_stub: HashMap<u32, bool> = HashMap::new();
    let mut body_off: usize = entry_count.saturating_mul(16);
    for i in 0..entry_count {
        let eoff = i * 16;
        if eoff + 16 > decompressed.len() {
            break;
        }
        let esize = rd_u32(decompressed, eoff) as usize;
        let name_hash = rd_u32(decompressed, eoff + 4);
        if !hash_to_index.contains_key(&name_hash) {
            hash_to_index.insert(name_hash, i);
            let stub = if esize != 0 && body_off + esize <= decompressed.len() {
                let body = &decompressed[body_off..body_off + esize];
                match find_subslice(body, b"BODY") {
                    None => true,
                    // Match Python exactly: `body[p+4:p+8] == b"\xff\xff\xff\xff"`.
                    // If fewer than 4 bytes follow `BODY`, Python's short slice
                    // compares unequal → NOT a stub. So out-of-range → false.
                    Some(p) => body
                        .get(p + 4..p + 8)
                        .map(|w| w == [0xFF, 0xFF, 0xFF, 0xFF])
                        .unwrap_or(false),
                }
            } else {
                true
            };
            hash_is_stub.insert(name_hash, stub);
        }
        body_off += esize;
    }

    if entry_count <= 1 {
        return counts;
    }

    for entry in entries.iter_mut() {
        let ah = entry.asset_hash;
        // The faithful BE sub survives as the HIGH16 of the post-`_strip` u32_2
        // (non-global: (BE_sub<<16)|0xFFFF; global: 0xFFFFFFFF).
        let be_sub = (entry.u32_2 >> 16) as u16;

        // Genuinely primary — synthetic script/stringdb rows, or a global entry
        // whose BE sub is the 0xFFFF "resolve-by-hash" sentinel.
        if PRESERVE_PRIMARY_ASET_SUB && (entry.primary || be_sub == SUB_PRIMARY) {
            entry.set_sub(SUB_PRIMARY);
            counts.preserved += 1;
            continue;
        }

        // STEP 3 FIX (base-aware), now extended to REAL bodies: if the asset hash
        // exists in the base game, resolve it BY HASH (0xFFFF) so the engine SHARES
        // base's already-resident copy — whether this block carries only a body-less
        // stub OR a redundant re-included body (the Xbox DLC bundles ~262 base
        // textures with bodies). A physical sub-entry index gives the entry a
        // position-derived key DISTINCT from base's hash-key, so the engine allocates
        // a SECOND texture-component cell (type 0xF011157A) for a texture base already
        // has resident. Those duplicate cells overflow the fixed 5120-slot pool
        // (FUN_004cc130/FUN_004cc030) → NULL pop @0x4CC064 / streaming livelock at the
        // mission-start spawn region. Sharing by hash is correct: the hash is the
        // name-derived key, so an in-base hash IS the same texture (identical content).
        if PRESERVE_PRIMARY_ASET_SUB && entry.in_base {
            entry.set_sub(SUB_PRIMARY);
            counts.preserved += 1;
            continue;
        }

        // DLC-new STUB (not in base): no by-hash fallback, so its local BE sub-offset
        // is the only locator we can preserve. [the genuine empty-texture set]
        if PRESERVE_PRIMARY_ASET_SUB && *hash_is_stub.get(&ah).unwrap_or(&false) {
            entry.set_sub(be_sub);
            counts.preserved += 1;
            continue;
        }

        // DLC-new entry with a real local body → physical sub-entry index
        // (unchanged faithful behavior).
        match hash_to_index.get(&ah) {
            Some(&idx) => {
                entry.set_sub(idx as u16);
                counts.resolved += 1;
            }
            None => {
                entry.set_sub(SUB_PRIMARY);
                counts.unresolved += 1;
            }
        }
    }
    counts
}

#[inline]
fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_matches_python() {
        // (u2 & 0xFFFF0000) | 0xFFFF
        assert_eq!(strip_xbox_sub_entry(0x0123_4567), 0x0123_FFFF);
        assert_eq!(strip_xbox_sub_entry(0xFFFF_01CA), 0xFFFF_FFFF);
        assert_eq!(strip_xbox_sub_entry(0x047B_0091), 0x047B_FFFF);
    }

    /// Build a block matching the real on-disk layout the Python parser assumes:
    /// rows of `{u32 size, u32 hash, u32 type, u32 pad}` from offset 0, then the
    /// per-entry bodies concatenated starting at `n*16`. `entry_count` is read as
    /// the first u32 (= row0.size), so we make **entry 0 a header** whose size is
    /// exactly `n` and whose body is `n` filler bytes (keeps `decomp[0]==n` and
    /// the body cumulative aligned). Real entries under test live at index ≥ 1.
    /// A stub body carries `BODY` + `0xFFFFFFFF`; a real body carries `BODY` + len.
    fn make_block(real_entries: &[(u32, bool)]) -> Vec<u8> {
        let n = real_entries.len() + 1; // +1 header row at index 0
        let mut bodies: Vec<Vec<u8>> = Vec::with_capacity(n);
        bodies.push(vec![0u8; n]); // entry0 header body: exactly `n` bytes
        for &(_, stub) in real_entries {
            let mut b = Vec::new();
            b.extend_from_slice(b"BODY");
            if stub {
                b.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            } else {
                b.extend_from_slice(&64u32.to_le_bytes());
                b.extend_from_slice(&[0u8; 64]);
            }
            bodies.push(b);
        }
        let mut out = Vec::new();
        // Rows: {size, hash, type, pad}. row0.size = n (== entry_count).
        for i in 0..n {
            let size = bodies[i].len() as u32; // body0.len()==n by construction
            let hash = if i == 0 { 0xDEAD_BEEF } else { real_entries[i - 1].0 };
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&hash.to_le_bytes());
            out.extend_from_slice(&0xF011_157Au32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        for b in &bodies {
            out.extend_from_slice(b);
        }
        out
    }

    #[test]
    fn primary_kept_ffff() {
        // 0xBBBB at real index 0 (entry index 1), real body, but flagged primary.
        let blk = make_block(&[(0xBBBB, false), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xBBBB, u32_2: 0x0000_0000, primary: true, in_base: false }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 0xFFFF, "primary entry must keep 0xFFFF");
        assert_eq!(c, RecomputeCounts { preserved: 1, resolved: 0, unresolved: 0 });
    }

    #[test]
    fn nonprimary_realbody_gets_index() {
        // 0xCCCC is real index 1 → entry index 2; real body, non-primary → index 2.
        let blk = make_block(&[(0xBBBB, false), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xCCCC, u32_2: 0x0005_0000, primary: false, in_base: false }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 2, "real-body non-primary -> physical index 2");
        assert_eq!(c.resolved, 1);
    }

    #[test]
    fn dlcnew_stub_preserves_be_sub() {
        // DLC-new stub (NOT in base): no by-hash fallback, so keep its BE
        // sub-offset (the only locator).
        let blk = make_block(&[(0xBBBB, true), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xBBBB, u32_2: 0x047B_0000, primary: false, in_base: false }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 0x047B, "DLC-new stub preserves its BE sub-offset");
        assert_eq!(c.preserved, 1);
    }

    #[test]
    fn base_resident_stub_stays_ffff() {
        // Base-resident stub: by-hash resolves the base body, so keep 0xFFFF
        // (forcing the BE offset would point into the wrong DLC block — the
        // regression this base-aware split fixes).
        let blk = make_block(&[(0xBBBB, true), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xBBBB, u32_2: 0x047B_0000, primary: false, in_base: true }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 0xFFFF, "base-resident stub resolves by hash → 0xFFFF");
        assert_eq!(c.preserved, 1);
    }

    #[test]
    fn base_resident_realbody_shares_by_hash() {
        // A base texture re-included WITH a real body in the patch must resolve
        // by hash (0xFFFF) to SHARE base's resident texture-component cell, NOT a
        // physical index — which would duplicate the cell and overflow the 5120
        // pool (0x4CC064 livelock). 262 dlc01 textures hit this case.
        let blk = make_block(&[(0xBBBB, false), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xBBBB, u32_2: 0x0005_0000, primary: false, in_base: true }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 0xFFFF, "in-base real-body texture shares by hash");
        assert_eq!(c, RecomputeCounts { preserved: 1, resolved: 0, unresolved: 0 });
    }

    #[test]
    fn primary_stub_still_ffff() {
        // A stub whose BE sub IS the 0xFFFF primary sentinel stays 0xFFFF.
        let blk = make_block(&[(0xBBBB, true), (0xCCCC, false)]);
        let mut e = vec![AsetEntry { asset_hash: 0xBBBB, u32_2: 0xFFFF_0000, primary: false, in_base: false }];
        let c = recompute_block_aset_subs(&blk, &mut e);
        assert_eq!(e[0].sub(), 0xFFFF, "BE sub==0xFFFF stays primary");
        assert_eq!(c.preserved, 1);
    }
}
