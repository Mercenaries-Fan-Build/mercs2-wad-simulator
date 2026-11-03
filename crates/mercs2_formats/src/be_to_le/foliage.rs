//! A console `foliage` chunk in PC form.
//!
//! Its one `tree` row is a stream of fields, not of words: a species holds a lone flag byte, so everything
//! after it sits off the 4-byte grid, and a word swap of the body leaves its materials' u16 pairs
//! transposed, which `Mtrl_Parse` reads as a texture count and dereferences null. So each field is swapped
//! by its width, in the order `FUN_004a0c40` and `FUN_004a01e0` read them:
//!
//! ```text
//! u32 version, u32 species count, then each species:
//!   u32 template, u32 × 2 atlas, f32 range, u8 flag, f32 × 3, u32 × 25 basis, u32 × 2,
//!   u32 vertex count, u32 index count,
//!   [vertex count × f32 × 5, index count × u16, material]   when both counts are non-zero
//!   material
//!   u32 bit stream bytes, the bit stream as u32 words, u32 bit count, f32 × 6, u32 × 8, u32 × 63,
//!   u32 × 3, u32 × 21
//! material: u32 × 26, u16 flags, u16 texture count, u32 × textures, u32 × 2
//! ```

use super::ps3_model::{be32, be_tree};
use crate::ucfx::{write_ucfx_tree, UcfxNode, UCFX_CSUM_BYTES};

/// A field stream read big-endian and written little-endian.
struct Fields<'a> {
    b: &'a [u8],
    p: usize,
    out: Vec<u8>,
}

impl Fields<'_> {
    fn take(&mut self, n: usize, what: &str) -> Result<&[u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| format!("foliage: {what} at {} runs past {} bytes", self.p, self.b.len()))?;
        self.p += n;
        Ok(s)
    }

    fn u32s(&mut self, n: usize, what: &str) -> Result<u32, String> {
        let mut last = 0;
        for _ in 0..n {
            let w = self.take(4, what)?;
            last = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
            self.out.extend_from_slice(&last.to_le_bytes());
        }
        Ok(last)
    }

    fn u16s(&mut self, n: usize, what: &str) -> Result<u16, String> {
        let mut last = 0;
        for _ in 0..n {
            let w = self.take(2, what)?;
            last = u16::from_be_bytes([w[0], w[1]]);
            self.out.extend_from_slice(&last.to_le_bytes());
        }
        Ok(last)
    }

    fn material(&mut self, what: &str) -> Result<(), String> {
        self.u32s(26, what)?;
        self.u16s(1, what)?;
        let textures = self.u16s(1, what)? as usize;
        self.u32s(textures + 2, what)?;
        Ok(())
    }

    fn species(&mut self) -> Result<(), String> {
        self.u32s(4, "species head")?;
        let flag = self.take(1, "flag")?[0];
        self.out.push(flag);
        self.u32s(3 + 25 + 2, "species fields")?;
        let vertices = self.u32s(1, "vertex count")? as usize;
        let indices = self.u32s(1, "index count")? as usize;
        if vertices != 0 && indices != 0 {
            self.u32s(5 * vertices, "billboard vertices")?;
            self.u16s(indices, "billboard indices")?;
            self.material("modelled billboard material")?;
        }
        self.material("billboard material")?;
        let bytes = self.u32s(1, "bit stream length")? as usize;
        if bytes % 4 != 0 {
            return Err(format!("foliage: a bit stream of {bytes} bytes is not whole words"));
        }
        self.u32s(bytes / 4, "bit stream")?;
        self.u32s(1 + 6 + 8 + 63 + 3 + 21, "position tree")?;
        Ok(())
    }
}

/// One `tree` row in PC form.
fn tree(b: &[u8]) -> Result<Vec<u8>, String> {
    let mut f = Fields { b, p: 0, out: Vec::with_capacity(b.len()) };
    f.u32s(1, "version")?;
    let count = f.u32s(1, "species count")?;
    for i in 0..count {
        f.species().map_err(|e| format!("species {i}: {e}"))?;
    }
    if f.p != b.len() {
        return Err(format!("foliage: {} bytes after the last species", b.len() - f.p));
    }
    Ok(f.out)
}

/// Convert a big-endian `foliage` chunk, without its `CSUM` trailer.
pub(super) fn convert(container: &[u8]) -> Result<Vec<u8>, String> {
    be32(container, 16)?;
    let roots = be_tree(container)?;
    let [root] = roots.as_slice() else {
        return Err(format!("foliage: {} root rows, expected 1", roots.len()));
    };
    if &root.tag != b"tree" {
        return Err(format!("foliage: a root row {}", root.tag_str()));
    }
    let body = root.body.as_deref().ok_or("foliage: tree has no body")?;
    let mut out = write_ucfx_tree(&[UcfxNode::leaf(*b"tree", tree(body)?)]);
    out.truncate(out.len() - UCFX_CSUM_BYTES);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One species with no modelled billboard, big-endian, its flag byte putting the rest off the grid.
    fn species(textures: &[u32]) -> Vec<u8> {
        let mut b = Vec::new();
        for w in [1u32, 2, 3, 4] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b.push(1);
        for w in 0..30u32 {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b.extend_from_slice(&[0; 8]);
        b.extend_from_slice(&[0; 104]);
        b.extend_from_slice(&0x0102u16.to_be_bytes());
        b.extend_from_slice(&(textures.len() as u16).to_be_bytes());
        for t in textures {
            b.extend_from_slice(&t.to_be_bytes());
        }
        b.extend_from_slice(&[0; 8]);
        b.extend_from_slice(&4u32.to_be_bytes());
        b.extend_from_slice(&0xAABB_CCDDu32.to_be_bytes());
        b.extend_from_slice(&[0; 4 * 102]);
        b
    }

    #[test]
    fn a_material_after_the_flag_keeps_its_u16_fields() {
        let mut b = [2u32.to_be_bytes(), 1u32.to_be_bytes()].concat();
        b.extend(species(&[0x1122_3344]));
        let out = tree(&b).unwrap();
        assert_eq!(out.len(), b.len());
        let at = 8 + 16 + 1 + 4 * 32 + 104;
        assert_eq!(&out[at..at + 4], &[0x02, 0x01, 1, 0], "flags, then a count of 1");
        assert_eq!(&out[at + 4..at + 8], &0x1122_3344u32.to_le_bytes());
        let bits = at + 8 + 8 + 4;
        assert_eq!(&out[bits..bits + 4], &0xAABB_CCDDu32.to_le_bytes(), "bit stream words swap as u32");
    }

    #[test]
    fn a_short_or_long_body_is_refused() {
        let mut b = [2u32.to_be_bytes(), 1u32.to_be_bytes()].concat();
        b.extend(species(&[]));
        assert!(tree(&b[..b.len() - 1]).is_err());
        b.push(0);
        assert!(tree(&b).is_err());
    }
}
