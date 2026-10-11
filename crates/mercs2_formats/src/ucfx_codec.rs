//! Helpers shared by the strict UCFX model codecs ([`crate::model`] and [`crate::tiny_model`]):
//! little-endian reads and writes over chunk bodies, and checked access to a parsed [`UcfxNode`]
//! tree that fails with the path being decoded.

use crate::ucfx::UcfxNode;

// ── little-endian reads and writes ──────────────────────────────────────────────────────────────

pub(crate) fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
pub(crate) fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}
pub(crate) fn f32s<const N: usize>(b: &[u8], o: usize) -> [f32; N] {
    std::array::from_fn(|k| f32_at(b, o + 4 * k))
}
pub(crate) fn put_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub(crate) fn put_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub(crate) fn put_f32s(v: &mut Vec<u8>, xs: &[f32]) {
    for x in xs {
        v.extend_from_slice(&x.to_bits().to_le_bytes());
    }
}

// ── tree helpers ────────────────────────────────────────────────────────────────────────────────

/// The body of a node that must be a childless leaf tagged `want`.
pub(crate) fn leaf<'a>(n: &'a UcfxNode, want: &[u8; 4], at: &str) -> Result<&'a [u8], String> {
    if &n.tag != want {
        return Err(format!("{at}: expected {} and found {}", String::from_utf8_lossy(want), n.tag_str()));
    }
    if !n.children.is_empty() {
        return Err(format!("{at}: {} has {} children; it is a leaf", n.tag_str(), n.children.len()));
    }
    n.body.as_deref().ok_or_else(|| format!("{at}: {} is a marker row; it carries a body", n.tag_str()))
}

/// The children of a node that must be a marker tagged `want`.
pub(crate) fn marker<'a>(n: &'a UcfxNode, want: &[u8; 4], at: &str) -> Result<&'a [UcfxNode], String> {
    if &n.tag != want {
        return Err(format!("{at}: expected {} and found {}", String::from_utf8_lossy(want), n.tag_str()));
    }
    if n.body.is_some() {
        return Err(format!("{at}: {} carries a body; it is a marker row", n.tag_str()));
    }
    Ok(&n.children)
}

/// Fails unless `body` is exactly `len` bytes; `what` names the body in the error.
pub(crate) fn exact_len(body: &[u8], len: usize, what: &str) -> Result<(), String> {
    if body.len() != len {
        return Err(format!("{what} is {} bytes; it is {len}", body.len()));
    }
    Ok(())
}

/// The count held by a body that must be exactly one u32.
pub(crate) fn count_word(body: &[u8], what: &str) -> Result<usize, String> {
    exact_len(body, 4, what)?;
    Ok(u32_at(body, 0) as usize)
}
