//! The bitmap `font` asset (ASET type id 15, type hash `0x99E77ACE`): `INFO`, `CHAR`, `MTRL`.
//!
//! The PC loader is `FUN_004ac8e0`. It reads `INFO` as 16 bytes and stores `u32 @0` (the page /
//! material count), `u32 @4`, `u16 @8` and `u16 @10` on the font object; `CHAR` is `count` u16
//! code points followed by `count` 12-byte glyph records (`count` = `INFO u32 @12`), inserted
//! verbatim into a hash table keyed by code point (`FUN_004acc30`); `MTRL` is one material per
//! page, each naming the page's texture ([`crate::texture::parse_mtrl`] with `MtrlSource::Font`).
//!
//! A glyph record:
//!
//! | bytes | field | meaning |
//! |---|---|---|
//! | 0-1, 2-3 | `u0`, `u1` | left and right edge, UNORM16 over the page width |
//! | 4-5, 6-7 | `v0`, `v1` | top and bottom edge, UNORM16 over the page height |
//! | 8 | `width` | the box width in pixels |
//! | 9 | `height` | the box height in pixels |
//! | 10 | `byte10` | not established — the baseline glyphs of `english_18` end at `byte10 + height = 28`, which reads like an offset from the line top, but the code that lays text out has not been found |
//! | 11 | `page` | index into the `MTRL` pages |
//!
//! Measured on the retail fonts: every `english_18` and `english_20` glyph's box, `(u1-u0)·w` by
//! `(v1-v0)·h` on its page, matches `width` × `height` to half a pixel and lies inside the page; the
//! `u16 @8` / `u16 @10` of `INFO` are the largest `width` / `height`. 16 glyphs of other fonts
//! (punctuation, CJK, private use) have a box whose size disagrees with `width`/`height` or whose
//! `v1 < v0`. No record field holds an advance.

use crate::ucfx::extract_chunk_body;

/// One glyph of a font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// The code point the font maps to this glyph.
    pub code: u16,
    /// The box on its page: left, right, top, bottom edges as UNORM16 fractions of the page size.
    pub u0: u16,
    pub u1: u16,
    pub v0: u16,
    pub v1: u16,
    /// The box size in pixels.
    pub width: u8,
    pub height: u8,
    /// Byte 10 of the record; its meaning is not established (see the module docs).
    pub byte10: u8,
    /// The page (index into [`Font::pages`]).
    pub page: u8,
}

impl Glyph {
    /// The box in pixels on a `page_w` × `page_h` page: `(x0, y0, x1, y1)`.
    pub fn box_px(&self, page_w: u32, page_h: u32) -> (f32, f32, f32, f32) {
        let s = |v: u16, n: u32| v as f32 / 65535.0 * n as f32;
        (s(self.u0, page_w), s(self.v0, page_h), s(self.u1, page_w), s(self.v1, page_h))
    }
}

/// A font container, read.
#[derive(Debug, Clone)]
pub struct Font {
    /// `INFO u32 @4`: stored on the font object at `+0x20`; no reader of it has been found.
    pub info_u32_4: u32,
    /// `INFO u16 @8` / `@10`: the largest glyph width / height in pixels.
    pub max_width: u16,
    pub max_height: u16,
    pub glyphs: Vec<Glyph>,
    /// Each page's texture hash, in `MTRL` order (a material's first texture).
    pub pages: Vec<u32>,
}

/// Read a font container.
pub fn parse_font(container: &[u8]) -> Result<Font, String> {
    let info = extract_chunk_body(container, b"INFO").ok_or("font: no INFO chunk")?;
    if info.len() < 16 {
        return Err(format!("font: INFO is {} bytes, not 16", info.len()));
    }
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let page_count = u32_at(&info, 0) as usize;
    let count = u32_at(&info, 12) as usize;
    let chr = extract_chunk_body(container, b"CHAR").ok_or("font: no CHAR chunk")?;
    if chr.len() != count * 14 {
        return Err(format!("font: CHAR is {} bytes, not {count} glyphs × 14", chr.len()));
    }
    let recs = count * 2;
    let glyphs = (0..count)
        .map(|i| {
            let r = recs + i * 12;
            Glyph {
                code: u16_at(&chr, i * 2),
                u0: u16_at(&chr, r),
                u1: u16_at(&chr, r + 2),
                v0: u16_at(&chr, r + 4),
                v1: u16_at(&chr, r + 6),
                width: chr[r + 8],
                height: chr[r + 9],
                byte10: chr[r + 10],
                page: chr[r + 11],
            }
        })
        .collect();
    let mats = crate::texture::parse_mtrl(container, crate::texture::MtrlSource::Font).map_err(|e| format!("font: MTRL: {e:?}"))?;
    if mats.len() != page_count {
        return Err(format!("font: INFO names {page_count} pages, MTRL holds {}", mats.len()));
    }
    let pages = mats
        .iter()
        .enumerate()
        .map(|(i, m)| m.textures.first().copied().ok_or(format!("font: page {i}'s material names no texture")))
        .collect::<Result<Vec<u32>, String>>()?;
    Ok(Font { info_u32_4: u32_at(&info, 4), max_width: u16_at(&info, 8), max_height: u16_at(&info, 10), glyphs, pages })
}

#[cfg(test)]
mod tests {
    use super::Glyph;

    /// english_18's `A`, as the retail record holds it, on its 256×512 page.
    #[test]
    fn a_glyph_box_is_its_unorm_edges_over_the_page() {
        let a = Glyph { code: 65, u0: 48383, u1: 52735, v0: 10623, v1: 13183, width: 17, height: 20, byte10: 8, page: 0 };
        let (x0, y0, x1, y1) = a.box_px(256, 512);
        assert!((x1 - x0 - a.width as f32).abs() <= 0.5, "{x0} {x1}");
        assert!((y1 - y0 - a.height as f32).abs() <= 0.5, "{y0} {y1}");
        assert!(x0 >= 0.0 && x1 <= 256.0 && y0 >= 0.0 && y1 <= 512.0);
    }
}
