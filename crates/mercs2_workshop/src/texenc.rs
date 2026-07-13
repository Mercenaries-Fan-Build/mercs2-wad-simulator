//! CPU BC1/BC3 ENCODE (min/max endpoint, no search) — turns imported RGBA8 images into the
//! compressed `TextureData` the engine's texture path uploads. Quality is fine for workbench
//! preview; a real encoder can slot in later without changing callers.

use mercs2_formats::texture::{TexFormat, TextureData};

/// Compress straight-alpha RGBA8 to BC1 (opaque) or BC3 (if any alpha < 250), as a 1-mip
/// `TextureData` ready for `Scene::load_model` / `set_loading_art`.
pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> TextureData {
    let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 250);
    let (bw, bh) = ((width as usize).div_ceil(4), (height as usize).div_ceil(4));
    let mut out = Vec::with_capacity(bw * bh * if has_alpha { 16 } else { 8 });
    let mut block = [[0u8; 4]; 16];
    for by in 0..bh {
        for bx in 0..bw {
            // Gather the 4x4 texel block (edge-clamped).
            for ty in 0..4 {
                for tx in 0..4 {
                    let px = (bx * 4 + tx).min(width as usize - 1);
                    let py = (by * 4 + ty).min(height as usize - 1);
                    let s = (py * width as usize + px) * 4;
                    block[ty * 4 + tx] = [rgba[s], rgba[s + 1], rgba[s + 2], rgba[s + 3]];
                }
            }
            if has_alpha {
                out.extend_from_slice(&encode_alpha_block(&block));
            }
            out.extend_from_slice(&encode_color_block(&block));
        }
    }
    let format = if has_alpha { TexFormat::Bc3 } else { TexFormat::Bc1 };
    TextureData { width, height, format, mip0: out.clone(), all_mips: out, mip_count: 1 }
}

fn rgb_to_565(p: [u8; 4]) -> u16 {
    ((p[0] as u16 >> 3) << 11) | ((p[1] as u16 >> 2) << 5) | (p[2] as u16 >> 3)
}

/// 8-byte BC1-style color block: min/max endpoints along the luminance-ish range, 4-color mode
/// (c0 > c1 forced by endpoint ordering; equal endpoints = flat block, all indices 0).
fn encode_color_block(block: &[[u8; 4]; 16]) -> [u8; 8] {
    // Endpoints: the texels with min/max sum(R,G,B) — crude but robust.
    let (mut lo, mut hi) = (block[0], block[0]);
    let lum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    for &p in block.iter() {
        if lum(p) < lum(lo) {
            lo = p;
        }
        if lum(p) > lum(hi) {
            hi = p;
        }
    }
    let (mut c0, mut c1) = (rgb_to_565(hi), rgb_to_565(lo));
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    // Palette (4-color mode) in RGB for index selection.
    let e = |v: u16| -> [i32; 3] {
        [
            (((v >> 11) & 31) * 255 / 31) as i32,
            (((v >> 5) & 63) * 255 / 63) as i32,
            ((v & 31) * 255 / 31) as i32,
        ]
    };
    let (p0, p1) = (e(c0), e(c1));
    let pal = [
        p0,
        p1,
        [(2 * p0[0] + p1[0]) / 3, (2 * p0[1] + p1[1]) / 3, (2 * p0[2] + p1[2]) / 3],
        [(p0[0] + 2 * p1[0]) / 3, (p0[1] + 2 * p1[1]) / 3, (p0[2] + 2 * p1[2]) / 3],
    ];
    let mut idx = 0u32;
    for (t, &p) in block.iter().enumerate() {
        let (mut best, mut bd) = (0usize, i32::MAX);
        for (k, q) in pal.iter().enumerate() {
            let d = (p[0] as i32 - q[0]).pow(2)
                + (p[1] as i32 - q[1]).pow(2)
                + (p[2] as i32 - q[2]).pow(2);
            if d < bd {
                bd = d;
                best = k;
            }
        }
        idx |= (best as u32) << (t * 2);
    }
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..8].copy_from_slice(&idx.to_le_bytes());
    out
}

/// 8-byte BC3 alpha block: min/max endpoints, 8-value interpolated mode, 3-bit indices.
fn encode_alpha_block(block: &[[u8; 4]; 16]) -> [u8; 8] {
    let (mut lo, mut hi) = (255u8, 0u8);
    for &p in block.iter() {
        lo = lo.min(p[3]);
        hi = hi.max(p[3]);
    }
    if lo == hi {
        hi = hi.saturating_add(1); // avoid a degenerate ramp; indices all map to a0 anyway
    }
    // a0 > a1 → 8-step interpolated palette.
    let (a0, a1) = (hi as u32, lo as u32);
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    for k in 1..7u32 {
        pal[(k + 1) as usize] = (((7 - k) * a0 + k * a1) / 7) as u8;
    }
    let mut bits = 0u64;
    for (t, &p) in block.iter().enumerate() {
        let (mut best, mut bd) = (0u64, i32::MAX);
        for (k, &q) in pal.iter().enumerate() {
            let d = (p[3] as i32 - q as i32).abs();
            if d < bd {
                bd = d;
                best = k as u64;
            }
        }
        bits |= best << (t * 3);
    }
    let mut out = [0u8; 8];
    out[0] = a0 as u8;
    out[1] = a1 as u8;
    out[2..8].copy_from_slice(&bits.to_le_bytes()[0..6]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode → decode (texpng) round trip stays close to the source for flat + gradient blocks.
    #[test]
    fn bc_round_trip_tolerance() {
        let (w, h) = (8u32, 8u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        // 1D gradient (R varies, G/B flat): representable on BC1's single palette line, so the
        // round trip only pays 565 quantization + 1/3-step interpolation error.
        for y in 0..h {
            for x in 0..w {
                let s = ((y * w + x) * 4) as usize;
                rgba[s] = (x * 30) as u8;
                rgba[s + 1] = 200;
                rgba[s + 2] = 50;
                rgba[s + 3] = 255;
            }
        }
        let td = encode_rgba(w, h, &rgba);
        assert_eq!(td.format, TexFormat::Bc1);
        let (_, _, back) = crate::texpng::decode_bc(&td);
        for (a, b) in rgba.chunks_exact(4).zip(back.chunks_exact(4)) {
            assert!((a[0] as i32 - b[0] as i32).abs() <= 24, "{a:?} vs {b:?}");
            assert!((a[1] as i32 - b[1] as i32).abs() <= 24);
            assert!((a[2] as i32 - b[2] as i32).abs() <= 24);
            assert_eq!(b[3], 255);
        }
    }

    #[test]
    fn bc3_when_alpha_present() {
        let rgba = vec![128u8, 128, 128, 100, 128, 128, 128, 100, 128, 128, 128, 100, 128, 128, 128, 100];
        let td = encode_rgba(2, 2, &rgba);
        assert_eq!(td.format, TexFormat::Bc3);
        let (_, _, back) = crate::texpng::decode_bc(&td);
        assert!((back[3] as i32 - 100).abs() <= 6);
    }
}
