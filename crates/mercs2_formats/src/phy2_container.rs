//! Locate and splice the `PHY2` collision chunk inside a UCFX model container.
//!
//! A model container is `UCFX [ndesc × 20-byte descriptor rows] [data area] CSUM`; the collision
//! lives in one row tagged `PHY2` whose body is `[48-byte prefix][Havok packfile][engine wrapper]`
//! ([`crate::phy2_build`] / [`crate::phy2_moppswap`]). Replacing that body — with a fresh PHY2 built
//! from a model's OWN geometry ([`crate::phy2_build::build_phy2_multi`]) — is how "collision follows
//! new geometry" lands: the loader walks the self-contained `WpArray` of shapes (each model-local,
//! placed by the object/cell transform), so swapping the PHY2 body alone re-shapes collision. `SEGM`
//! is a pure RENDER draw table and is NOT touched here — collision is not SEGM-bound.
//!
//! Extracted verbatim from the `mopp_overlay_forge` probe so both the probe and
//! `mercs2_quartermaster`'s `add_model collision: follow_geometry` path share ONE splice
//! implementation (container repack / descriptor-offset fix / CSUM recompute).

use crate::crc32::crc32_mercs2;

/// Locate the `PHY2` chunk inside a UCFX container. Returns `(body_start, body_size)` where
/// `body_start` is an absolute offset into `container`. `None` when the container is not a UCFX
/// packet or carries no resolvable `PHY2` row.
pub fn phy2_span_in_container(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

/// Replace the `PHY2` body at absolute container offset `pstart` (old length `psize`) with `new_body`,
/// fixing the UCFX descriptor table (this `PHY2`'s `body_size`; any body located AFTER it shifts by the
/// size delta), splicing the data region, and recomputing the trailing `CSUM`. Returns the rebuilt
/// container. `delta == 0` (in-place-size swaps) produces a container with only the changed body bytes +
/// recomputed CSUM — byte-identical when the body itself is unchanged.
pub fn replace_phy2_in_container(
    container: &[u8],
    pstart: usize,
    psize: usize,
    new_body: &[u8],
) -> Result<Vec<u8>, String> {
    if container.len() < 28 || &container[0..4] != b"UCFX" {
        return Err("container is not a UCFX packet".into());
    }
    if container.len() < 8 || &container[container.len() - 8..container.len() - 4] != b"CSUM" {
        return Err("container has no CSUM trailer".into());
    }
    let csum_start = container.len() - 8;
    if pstart + psize > csum_start {
        return Err("PHY2 body overlaps the CSUM trailer".into());
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().unwrap()) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().unwrap()) as usize;
    let delta = new_body.len() as i64 - psize as i64;
    let phy2_rel = pstart
        .checked_sub(if data_area_off > 0 { data_area_off } else { 8 })
        .ok_or("PHY2 body starts before the data area")?;

    // Patch the descriptor table (rows sit before the data area, i.e. before `pstart`).
    let mut c = container.to_vec();
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > c.len() {
            break;
        }
        let ru0 = u32::from_le_bytes(c[r + 4..r + 8].try_into().unwrap());
        if ru0 == 0xFFFF_FFFF {
            continue;
        }
        let ru0u = ru0 as usize;
        if ru0u == phy2_rel && &c[r..r + 4] == b"PHY2" {
            c[r + 8..r + 12].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
        } else if ru0u > phy2_rel {
            let nv = (ru0 as i64 + delta) as u32;
            c[r + 4..r + 8].copy_from_slice(&nv.to_le_bytes());
        }
    }

    // Splice the data region and recompute the CSUM over everything before the trailer.
    let mut out = Vec::with_capacity((c.len() as i64 + delta).max(0) as usize);
    out.extend_from_slice(&c[..pstart]);
    out.extend_from_slice(new_body);
    out.extend_from_slice(&c[pstart + psize..csum_start]);
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}
