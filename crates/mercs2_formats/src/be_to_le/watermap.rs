//! A console `watermap` chunk in PC form.
//!
//! Its one `watr` row (`docs/watermap_format.md`) is three regions of three widths, measured against
//! the retail PC chunk byte for byte:
//!
//! ```text
//! u32 × (9 + width·height + 2)   the header, whose words 1 and 2 are the grid, the heights, two more words
//! u8 × 3·width·height            the three mask layers
//! u16 × rest                     the footer
//! ```
//!
//! A word swap of the whole body scrambles every mask byte that differs from its neighbours, and the
//! footer starts off the 4-byte grid.

use super::ps3_model::{be32, be_tree};
use crate::ucfx::{write_ucfx_tree, UcfxNode, UCFX_CSUM_BYTES};

/// Words of the header before the heights.
const HEADER_WORDS: usize = 9;
/// Words between the heights and the masks.
const AFTER_HEIGHTS: usize = 2;
/// Mask layers of one byte a cell.
const MASKS: usize = 3;

/// One `watr` row in PC form.
fn water(b: &[u8]) -> Result<Vec<u8>, String> {
    let cells = (be32(b, 4)? as usize).checked_mul(be32(b, 8)? as usize).ok_or("watermap: the grid overflows")?;
    let words = 4 * (HEADER_WORDS + cells + AFTER_HEIGHTS);
    let masks = words + MASKS * cells;
    if masks > b.len() || (b.len() - masks) % 2 != 0 {
        return Err(format!("watermap: a {}-byte body for {cells} cells", b.len()));
    }
    let mut out = Vec::with_capacity(b.len());
    out.extend(b[..words].chunks_exact(4).flat_map(|w| [w[3], w[2], w[1], w[0]]));
    out.extend_from_slice(&b[words..masks]);
    out.extend(b[masks..].chunks_exact(2).flat_map(|w| [w[1], w[0]]));
    Ok(out)
}

/// Convert a big-endian `watermap` chunk, without its `CSUM` trailer.
pub(super) fn convert(container: &[u8]) -> Result<Vec<u8>, String> {
    let roots = be_tree(container)?;
    let [root] = roots.as_slice() else {
        return Err(format!("watermap: {} root rows, expected 1", roots.len()));
    };
    if &root.tag != b"watr" {
        return Err(format!("watermap: a root row {}", root.tag_str()));
    }
    let body = root.body.as_deref().ok_or("watermap: watr has no body")?;
    let mut out = write_ucfx_tree(&[UcfxNode::leaf(*b"watr", water(body)?)]);
    out.truncate(out.len() - UCFX_CSUM_BYTES);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_region_swaps_by_its_width() {
        // A 1 × 2 grid: 9 + 2 + 2 words, 6 mask bytes, then a footer of two u16.
        let mut b = Vec::new();
        for w in [5u32, 1, 2, 0, 0, 0, 0, 0, 0, 0x0A0B_0C0D, 7, 8, 9] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        b.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        let out = water(&b).unwrap();
        assert_eq!(&out[36..40], &0x0A0B_0C0Du32.to_le_bytes());
        assert_eq!(&out[52..58], &[1, 2, 3, 4, 5, 6], "masks keep their order");
        assert_eq!(&out[58..], &[0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn a_body_short_of_its_grid_is_refused() {
        let mut b = Vec::new();
        for w in [5u32, 100, 100] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        assert!(water(&b).is_err());
    }
}
