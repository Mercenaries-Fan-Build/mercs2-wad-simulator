//! RGBA → BC1/BC3 + the fully-resident UCFX texture container the engine reads inline.
//!
//! A faithful port of `tools/dds_to_ucfx_texture.py`, which is the encoder that has actually
//! produced working game textures. `mercs2_workshop::texenc` is NOT that encoder — its own header
//! calls it workbench-preview quality — so shipping assets are built here instead.
//!
//! **On `f32`.** The game reads no floating point: BC1 on disk is two RGB565 endpoints plus sixteen
//! 2-bit indices, all integer. The arithmetic below is `f32` solely to stay byte-comparable with the
//! reference, which pins `np.float32`. Precision matters at exactly one step — choosing each texel's
//! nearest palette entry. Where a texel sits near-equidistant between two entries, f32 and f64 round
//! the interpolated endpoints differently and the tie breaks the other way, flipping index bits. A
//! port that "looks right" but differs cannot be diffed against the known-good output, which is the
//! only cheap way to catch a mistake in a 175 KB blob of compressed blocks.

/// Mercenaries 2's CSUM: the raw CRC-32 register after processing, WITHOUT the usual final
/// inversion. The Python is `zlib.crc32(d, 0xFFFFFFFF) ^ 0xFFFFFFFF`, and zlib inverts its seed on
/// the way in and the register on the way out, so the two inversions cancel and what is stored is
/// the bare register seeded at zero. Getting this wrong writes a container the engine rejects.
pub fn crc32_mercs2(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut reg: u32 = 0;
    for &b in data {
        reg = table[((reg ^ b as u32) & 0xFF) as usize] ^ (reg >> 8);
    }
    reg
}

fn to565(c: [f32; 3]) -> u16 {
    let r = ((c[0].round() as i32) >> 3) & 0x1F;
    let g = ((c[1].round() as i32) >> 2) & 0x3F;
    let b = ((c[2].round() as i32) >> 3) & 0x1F;
    ((r << 11) | (g << 5) | b) as u16
}

fn from565(v: u16) -> [f32; 3] {
    let r = ((v >> 11) & 0x1F) as i32;
    let g = ((v >> 5) & 0x3F) as i32;
    let b = (v & 0x1F) as i32;
    [
        (((r << 3) | (r >> 2)) as f32),
        (((g << 2) | (g >> 4)) as f32),
        (((b << 3) | (b >> 2)) as f32),
    ]
}

/// 4x4 RGB (row-major, 16 texels) → 8-byte BC1 colour block, always 4-colour (opaque) mode.
pub fn bc1_block(px: &[[f32; 3]; 16]) -> [u8; 8] {
    let mut cmin = [f32::MAX; 3];
    let mut cmax = [f32::MIN; 3];
    for p in px.iter() {
        for k in 0..3 {
            cmin[k] = cmin[k].min(p[k]);
            cmax[k] = cmax[k].max(p[k]);
        }
    }
    let mut c0 = to565(cmax);
    let mut c1 = to565(cmin);
    if c0 == c1 {
        // Flat block: keep c0 > c1 so the decoder stays in 4-colour mode. c0 <= c1 would select
        // BC1's punch-through-alpha mode and make the block partly transparent.
        if c1 == 0 {
            c0 = 1;
        } else {
            c1 = c0 - 1;
        }
    }
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    let e0 = from565(c0);
    let e1 = from565(c1);
    let pal = [
        e0,
        e1,
        [
            (2.0 * e0[0] + e1[0]) / 3.0,
            (2.0 * e0[1] + e1[1]) / 3.0,
            (2.0 * e0[2] + e1[2]) / 3.0,
        ],
        [
            (e0[0] + 2.0 * e1[0]) / 3.0,
            (e0[1] + 2.0 * e1[1]) / 3.0,
            (e0[2] + 2.0 * e1[2]) / 3.0,
        ],
    ];
    let mut bits: u32 = 0;
    for (i, p) in px.iter().enumerate() {
        let mut best = (f32::MAX, 0usize);
        for (j, q) in pal.iter().enumerate() {
            let d = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
            // Strictly-less keeps the FIRST minimum, matching numpy argmin's tie policy.
            if d < best.0 {
                best = (d, j);
            }
        }
        bits |= (best.1 as u32) << (2 * i);
    }
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..8].copy_from_slice(&bits.to_le_bytes());
    out
}

/// 16 alpha values → 8-byte BC3 alpha block (8-value interpolated mode).
fn bc3_alpha_block(a: &[f32; 16]) -> [u8; 8] {
    let mut amin = 255.0f32;
    let mut amax = 0.0f32;
    for &v in a.iter() {
        amin = amin.min(v);
        amax = amax.max(v);
    }
    let a0 = amax.round().clamp(0.0, 255.0) as u8;
    let a1 = amin.round().clamp(0.0, 255.0) as u8;
    // a0 > a1 selects the 8-value mode (no explicit 0/255 endpoints), which is what a normal map
    // wants: the swizzled X channel is a smooth gradient, not a mask.
    let (a0, a1) = if a0 > a1 { (a0, a1) } else { (a1, a0) };
    let mut pal = [0.0f32; 8];
    pal[0] = a0 as f32;
    pal[1] = a1 as f32;
    if a0 > a1 {
        for k in 1..7 {
            pal[k + 1] = ((7 - k) as f32 * a0 as f32 + k as f32 * a1 as f32) / 7.0;
        }
    } else {
        for k in 1..5 {
            pal[k + 1] = ((5 - k) as f32 * a0 as f32 + k as f32 * a1 as f32) / 5.0;
        }
        pal[6] = 0.0;
        pal[7] = 255.0;
    }
    let mut bits: u64 = 0;
    for (i, &v) in a.iter().enumerate() {
        let mut best = (f32::MAX, 0usize);
        for (j, &q) in pal.iter().enumerate() {
            let d = (v - q).abs();
            if d < best.0 {
                best = (d, j);
            }
        }
        bits |= (best.1 as u64) << (3 * i);
    }
    let mut out = [0u8; 8];
    out[0] = a0;
    out[1] = a1;
    for i in 0..6 {
        out[2 + i] = ((bits >> (8 * i)) & 0xFF) as u8;
    }
    out
}

fn gather<const N: usize>(
    w: usize,
    h: usize,
    px: &[f32],
    bx: usize,
    by: usize,
    ch: usize,
) -> [[f32; N]; 16] {
    let mut blk = [[0.0f32; N]; 16];
    for ty in 0..4 {
        for tx in 0..4 {
            // Edge-clamp partial blocks rather than zero-fill: zeros drag the endpoints toward
            // black and darken the last row/column of a non-multiple-of-4 texture.
            let sx = (bx * 4 + tx).min(w - 1);
            let sy = (by * 4 + ty).min(h - 1);
            let s = (sy * w + sx) * ch;
            for k in 0..N {
                blk[ty * 4 + tx][k] = px[s + k];
            }
        }
    }
    blk
}

/// Compress one RGB surface (`w*h*3` floats 0..255) to BC1.
pub fn encode_bc1(w: usize, h: usize, rgb: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(w.div_ceil(4) * h.div_ceil(4) * 8);
    for by in 0..h.div_ceil(4) {
        for bx in 0..w.div_ceil(4) {
            out.extend_from_slice(&bc1_block(&gather::<3>(w, h, rgb, bx, by, 3)));
        }
    }
    out
}

/// Compress one RGBA surface (`w*h*4` floats 0..255) to BC3: alpha block then colour block.
pub fn encode_bc3(w: usize, h: usize, rgba: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(w.div_ceil(4) * h.div_ceil(4) * 16);
    for by in 0..h.div_ceil(4) {
        for bx in 0..w.div_ceil(4) {
            let blk = gather::<4>(w, h, rgba, bx, by, 4);
            let mut a = [0.0f32; 16];
            let mut c = [[0.0f32; 3]; 16];
            for i in 0..16 {
                a[i] = blk[i][3];
                c[i] = [blk[i][0], blk[i][1], blk[i][2]];
            }
            out.extend_from_slice(&bc3_alpha_block(&a));
            out.extend_from_slice(&bc1_block(&c));
        }
    }
    out
}

/// Box-downsample by 2x, `ch` channels interleaved.
pub fn box_down(w: usize, h: usize, ch: usize, px: &[f32]) -> (usize, usize, Vec<f32>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0.0f32; nw * nh * ch];
    for y in 0..nh {
        for x in 0..nw {
            for k in 0..ch {
                let s = |xx: usize, yy: usize| px[(yy.min(h - 1) * w + xx.min(w - 1)) * ch + k];
                out[(y * nw + x) * ch + k] =
                    (s(2 * x, 2 * y) + s(2 * x + 1, 2 * y) + s(2 * x, 2 * y + 1) + s(2 * x + 1, 2 * y + 1))
                        * 0.25;
            }
        }
    }
    (nw, nh, out)
}

/// Mip count the container declares: `max(1, bit_length(min(w,h)) - 2)`.
///
/// Not `log2(n)+1`: the chain stops at 4x4, the smallest block-compressed level that is a whole
/// block. Measured against retail — 512² gives 8 and `pmc_hum_mattias_v2_ub` ships 8; 128² gives 6
/// and `pmc_hum_chris_eyes` ships 6.
pub fn mip_count(w: usize, h: usize) -> usize {
    let m = w.min(h);
    (usize::BITS - m.leading_zeros()) as usize - 2
}

/// Full mip chain for a surface, encoded with `f`, concatenated coarse-to-fine as the engine expects
/// (level 0 first).
pub fn mip_chain(
    w: usize,
    h: usize,
    ch: usize,
    px: &[f32],
    f: impl Fn(usize, usize, &[f32]) -> Vec<u8>,
) -> Vec<u8> {
    let mips = mip_count(w, h).max(1);
    let mut body = Vec::new();
    let (mut cw, mut chh, mut cur) = (w, h, px.to_vec());
    for m in 0..mips {
        body.extend_from_slice(&f(cw, chh, &cur));
        if m + 1 < mips {
            let (nw, nh, n) = box_down(cw, chh, ch, &cur);
            cw = nw;
            chh = nh;
            cur = n;
        }
    }
    body
}

/// Wrap an encoded mip chain in a fully-resident UCFX texture container (NAME/INFO/BODY + CSUM).
///
/// `INFO[26..32] = 0` plus the `0xFFFF` sentinel is what marks it resident: the engine then reads
/// BODY inline instead of expecting the higher mips to stream from finer c3-cell blocks. A streamed
/// layout here would over-read and trip the BUFFER_TOO_SMALL path.
pub fn ucfx_texture(name: &str, w: usize, h: usize, fourcc: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut name_b: Vec<u8> = name.as_bytes().to_vec();
    name_b.push(0);
    while name_b.len() % 2 != 0 {
        name_b.push(0);
    }
    let mips = mip_count(w, h).max(1);
    let mut info = vec![0u8; 34];
    for (i, v) in [w as u16, h as u16, 1, mips as u16, 0, 1, 1].iter().enumerate() {
        info[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    info[14..18].copy_from_slice(fourcc);
    info[22..26].copy_from_slice(&(body.len() as u32).to_le_bytes());
    info[32..34].copy_from_slice(&0xFFFFu16.to_le_bytes());

    let rows: [(&[u8; 4], &[u8], u32); 3] =
        [(b"NAME", &name_b[..], 2), (b"INFO", &info[..], 1), (b"BODY", body, 0)];
    let data_off = 20u32 + 3 * 20;
    let mut blob: Vec<u8> = Vec::new();
    let mut placed: Vec<(&[u8; 4], u32, u32, u32)> = Vec::new();
    for (tag, bytes, u2) in rows.iter() {
        while blob.len() % 4 != 0 {
            blob.push(0);
        }
        placed.push((tag, blob.len() as u32, bytes.len() as u32, *u2));
        blob.extend_from_slice(bytes);
    }
    let mut c: Vec<u8> = Vec::new();
    c.extend_from_slice(b"UCFX");
    for v in [data_off, 0, 0, 3] {
        c.extend_from_slice(&v.to_le_bytes());
    }
    for (tag, off, sz, u2) in placed {
        c.extend_from_slice(tag);
        for v in [off, sz, u2, 0] {
            c.extend_from_slice(&v.to_le_bytes());
        }
    }
    c.extend_from_slice(&blob);
    let sum = crc32_mercs2(&c);
    c.extend_from_slice(b"CSUM");
    c.extend_from_slice(&sum.to_le_bytes());
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_counts_match_retail() {
        // pmc_hum_mattias_v2_ub ships 8 mips at 512^2; pmc_hum_chris_eyes ships 6 at 128^2.
        assert_eq!(mip_count(512, 512), 8);
        assert_eq!(mip_count(128, 128), 6);
        assert_eq!(mip_count(1024, 1024), 9);
    }

    #[test]
    fn bc1_chain_is_the_retail_size() {
        // 512^2 BC1, 8 mips, fully resident = the 174,760 bytes --tex-check reports for retail.
        let px = vec![128.0f32; 512 * 512 * 3];
        let body = mip_chain(512, 512, 3, &px, |w, h, p| encode_bc1(w, h, p));
        assert_eq!(body.len(), 174_760);
    }

    #[test]
    fn bc3_chain_is_the_retail_size() {
        // 512^2 BC3 is exactly twice BC1 — retail _ub_nm is 349,520 bytes.
        let px = vec![128.0f32; 512 * 512 * 4];
        let body = mip_chain(512, 512, 4, &px, |w, h, p| encode_bc3(w, h, p));
        assert_eq!(body.len(), 349_520);
    }

    #[test]
    fn flat_block_stays_in_opaque_mode() {
        // c0 <= c1 would select BC1's punch-through alpha and hole the surface.
        let blk = [[17.0f32, 34.0, 51.0]; 16];
        let out = bc1_block(&blk);
        let c0 = u16::from_le_bytes([out[0], out[1]]);
        let c1 = u16::from_le_bytes([out[2], out[3]]);
        assert!(c0 > c1, "flat block fell into 1-bit-alpha mode: c0={c0} c1={c1}");
    }
}
