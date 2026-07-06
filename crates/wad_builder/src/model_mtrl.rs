//! Fix multi-material MTRL count transposition in a converted model block.
//!
//! `ucfx_byteswap::convert_mtrl` byte-swaps material[0]'s `[u16 flags][u16 count]`
//! pair correctly but blanket-`swap_u32`s material[1..], which **transposes the two
//! u16 halves** (`[flags][count]` → `[count][flags]`). The engine reads `count` from
//! the second u16 and writes that many 12-byte `{hash,0xF011157A,0}` records into a
//! FIXED 10-slot array — a transposed count (e.g. 128) overruns it → `Mtrl_Parse` AV.
//!
//! The shared converter's full-array fix is intentionally parked (it changed
//! world-load behaviour). For on-demand assets (skins via `SetOutfit`) there is no
//! world-load, so we apply the fix here as a post-convert correction: walk the
//! material array and halfword-transpose every record whose `count` half is invalid,
//! then recompute the container CSUM. material[0] (already PC-form) is left as-is.

use mercs2_formats::crc32::crc32_mercs2;

const MTRL_PRE: usize = 104; // 26-dword param block before the [flags][count] pair

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Find a chunk body `(abs_offset, size)` by tag in a UCFX container.
fn find_chunk(container: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_base = rd_u32(container, 4) as usize;
    let n_desc = rd_u32(container, 16) as usize;
    for d in 0..n_desc {
        let off = 20 + d * 20;
        if off + 20 > container.len() {
            break;
        }
        if &container[off..off + 4] == tag {
            let row_u0 = rd_u32(container, off + 4) as usize;
            let body_size = rd_u32(container, off + 8) as usize;
            return Some((data_base + row_u0, body_size));
        }
    }
    None
}

/// Repoint texture hashes inside the MTRL material array per a `(old -> new)` map.
/// Walks each material record's `count`-entry texture slot array (the same layout
/// `fix_container_mtrl` uses) and replaces any slot whose hash is a key in `remap`.
/// Recomputes the trailing CSUM if anything changed. Returns the number of slots
/// repointed.
pub fn repoint_container_textures(
    container: &mut Vec<u8>,
    remap: &std::collections::HashMap<u32, u32>,
) -> Result<usize, String> {
    let (mabs, msize) = find_chunk(container, b"MTRL").ok_or("no MTRL chunk in container")?;
    if mabs + msize > container.len() {
        return Err("MTRL body out of range".into());
    }
    let mut changed = 0usize;
    let mut off = 0usize;
    loop {
        let cp = mabs + off + MTRL_PRE; // count-pair offset (absolute)
        if cp + 4 > mabs + msize {
            break;
        }
        let count = u16::from_le_bytes([container[cp + 2], container[cp + 3]]) as usize;
        if count == 0 || count > 10 {
            break; // not a valid PC-form material record — stop
        }
        // Texture-hash slots start right after the [flags][count] pair.
        for k in 0..count {
            let slot = cp + 4 + k * 4;
            if slot + 4 > mabs + msize {
                break;
            }
            let h = rd_u32(container, slot);
            if let Some(&new) = remap.get(&h) {
                container[slot..slot + 4].copy_from_slice(&new.to_le_bytes());
                changed += 1;
            }
        }
        off += 116 + count * 4;
    }
    if changed > 0 {
        let n = container.len();
        if n < 8 || &container[n - 8..n - 4] != b"CSUM" {
            return Err("container missing CSUM trailer".into());
        }
        let csum = crc32_mercs2(&container[..n - 8]);
        container[n - 4..n].copy_from_slice(&csum.to_le_bytes());
    }
    Ok(changed)
}

/// Fix material[1..] count-pair transposition inside a single UCFX container
/// (`[UCFX..CSUM]`), recomputing the trailing CSUM if anything changed.
/// Returns the number of records transposed.
pub fn fix_container_mtrl(container: &mut Vec<u8>) -> Result<usize, String> {
    let (mabs, msize) = find_chunk(container, b"MTRL").ok_or("no MTRL chunk in container")?;
    if mabs + msize > container.len() {
        return Err("MTRL body out of range".into());
    }
    let mut fixed = 0usize;
    let mut off = 0usize;
    loop {
        let cp = mabs + off + MTRL_PRE; // count-pair offset (absolute)
        if cp + 4 > mabs + msize {
            break;
        }
        let lo = u16::from_le_bytes([container[cp], container[cp + 1]]); // flags half
        let hi = u16::from_le_bytes([container[cp + 2], container[cp + 3]]); // count half
        let count = if (1..=10).contains(&hi) {
            hi // PC-form (material[0] etc.) — count already in the high half
        } else if (1..=10).contains(&lo) {
            // Transposed: swap the two u16 halves in place → [hi-bytes][lo-bytes].
            let b = [container[cp], container[cp + 1], container[cp + 2], container[cp + 3]];
            container[cp] = b[2];
            container[cp + 1] = b[3];
            container[cp + 2] = b[0];
            container[cp + 3] = b[1];
            fixed += 1;
            lo
        } else {
            break; // unrecognized record — stop, leave the rest untouched
        };
        off += 116 + count as usize * 4;
    }
    if fixed > 0 {
        // Recompute the trailing CSUM over [UCFX .. pre-CSUM].
        let n = container.len();
        if n < 8 || &container[n - 8..n - 4] != b"CSUM" {
            return Err("container missing CSUM trailer".into());
        }
        let csum = crc32_mercs2(&container[..n - 8]);
        container[n - 4..n].copy_from_slice(&csum.to_le_bytes());
    }
    Ok(fixed)
}
