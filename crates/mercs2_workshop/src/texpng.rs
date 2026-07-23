//! CPU BC1/BC3 decode + PNG write for the `--tex-png` headless dump — visual inspection of WAD
//! textures without a GPU (the engine itself uploads the compressed blocks directly).

use mercs2_formats::texture::{TexFormat, TextureData};

/// Bytes one full mip surface occupies at this size/format.
fn surface_bytes(w: usize, h: usize, f: TexFormat) -> usize {
    let block_bytes = match f {
        TexFormat::Bc1 => 8,
        TexFormat::Bc3 => 16,
    };
    w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * block_bytes
}

/// The finest mip level `td.mip0` actually holds enough bytes for. `0` = the full `width x height`.
///
/// A STREAMED texture's resident container declares the full `width`/`height` but ships only the
/// coarse mip TAIL, so `mip0` can be a tiny fraction of a full surface — a 1024² BC3 normal map
/// arrives as 1,360 bytes. The first surface in that tail is level L, not level 0. This is the same
/// resolution the GPU path does in `make_bc_view`; without it a decoder writes a full-size image and
/// leaves ~99% of it zeroed.
fn usable_level(td: &TextureData) -> u32 {
    (0..12)
        .find(|&l| {
            let (w, h) = ((td.width >> l).max(1) as usize, (td.height >> l).max(1) as usize);
            surface_bytes(w, h, td.format) <= td.mip0.len()
        })
        .unwrap_or(11)
}

/// Decode the finest mip level the data actually covers, to straight-alpha RGBA8.
///
/// Returns the TRUE dimensions of what was decoded, which for a streamed texture that never had its
/// higher mips assembled is smaller than `td.width`/`td.height` — callers must write the PNG at the
/// returned size, not the declared one.
pub fn decode_bc(td: &TextureData) -> (u32, u32, Vec<u8>) {
    let level = usable_level(td);
    let (w, h) = ((td.width >> level).max(1) as usize, (td.height >> level).max(1) as usize);
    let mut out = vec![0u8; w * h * 4];
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    let block_bytes = match td.format {
        TexFormat::Bc1 => 8,
        TexFormat::Bc3 => 16,
    };
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by * bw + bx) * block_bytes;
            if off + block_bytes > td.mip0.len() {
                continue;
            }
            let block = &td.mip0[off..off + block_bytes];
            let texels = match td.format {
                TexFormat::Bc1 => decode_bc1_block(block, true),
                TexFormat::Bc3 => decode_bc3_block(block),
            };
            for ty in 0..4 {
                for tx in 0..4 {
                    let (px, py) = (bx * 4 + tx, by * 4 + ty);
                    if px < w && py < h {
                        let d = (py * w + px) * 4;
                        out[d..d + 4].copy_from_slice(&texels[ty * 4 + tx]);
                    }
                }
            }
        }
    }
    (w as u32, h as u32, out)
}

/// Mean of the blue channel of an RGBA8 buffer. A real tangent-space normal map is blue-dominant
/// (Z≈+1 → B≈255); the game's `_nm` textures are grayscale relief (B≈128). Used to tell them apart so
/// only the grayscale ones are converted.
pub fn mean_blue(rgba: &[u8]) -> f32 {
    if rgba.len() < 4 {
        return 0.0;
    }
    let n = rgba.len() / 4;
    let sum: u64 = (0..n).map(|i| rgba[i * 4 + 2] as u64).sum();
    sum as f32 / n as f32
}

/// Convert a grayscale HEIGHT/bump image into an OpenGL-convention tangent-space normal map
/// (R=X, G=+Y up, B=Z-out), which is what glTF — and so Blender's importer — expects. The game's
/// `_nm` slot is NOT a tangent normal map: all channels carry the same scalar relief, so bound
/// directly it yields near-sideways normals (Z≈0) and the mottled/banded mis-shading. Deriving the
/// normal from the height gradient fixes that while keeping the authored surface detail.
///
/// Height is read from the red channel (R==G==B for these). `strength` scales the bump (higher =
/// deeper relief). Edges clamp. Output alpha is opaque.
pub fn height_to_normal(rgba: &[u8], w: u32, h: u32, strength: f32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 {
        return rgba.to_vec();
    }
    let height_at = |x: usize, y: usize| -> f32 { rgba[(y * w + x) * 4] as f32 / 255.0 };
    let mut out = vec![0u8; w * h * 4];
    let enc = |v: f32, inv: f32| ((v * inv * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for y in 0..h {
        for x in 0..w {
            let xl = x.saturating_sub(1);
            let xr = (x + 1).min(w - 1);
            let yu = y.saturating_sub(1);
            let yd = (y + 1).min(h - 1);
            // Central-difference height gradient.
            let dx = (height_at(xr, y) - height_at(xl, y)) * 0.5;
            let dy = (height_at(x, yd) - height_at(x, yu)) * 0.5;
            // Slope opposes the gradient. glTF/OpenGL wants +Y (green) pointing UP the image, and the
            // image V axis runs downward, so the Y term is +dy (not -dy, which would be DirectX).
            let nx = -dx * strength;
            let ny = dy * strength;
            let nz = 1.0f32;
            let inv = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
            let d = (y * w + x) * 4;
            out[d] = enc(nx, inv);
            out[d + 1] = enc(ny, inv);
            out[d + 2] = enc(nz, inv);
            out[d + 3] = 255;
        }
    }
    out
}

fn rgb565(v: u16) -> [u8; 3] {
    let r = ((v >> 11) & 0x1F) as u32;
    let g = ((v >> 5) & 0x3F) as u32;
    let b = (v & 0x1F) as u32;
    [((r * 255 + 15) / 31) as u8, ((g * 255 + 31) / 63) as u8, ((b * 255 + 15) / 31) as u8]
}

/// 8-byte BC1 color block → 16 RGBA texels. `allow_1bit_alpha`: BC1's c0<=c1 punch-through mode
/// (BC3's embedded color block is always 4-color).
fn decode_bc1_block(b: &[u8], allow_1bit_alpha: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let p0 = rgb565(c0);
    let p1 = rgb565(c1);
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !allow_1bit_alpha {
        for k in 0..3 {
            pal[2][k] = ((2 * p0[k] as u32 + p1[k] as u32) / 3) as u8;
            pal[3][k] = ((p0[k] as u32 + 2 * p1[k] as u32) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = ((p0[k] as u32 + p1[k] as u32) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for t in 0..16 {
        out[t] = pal[((idx >> (t * 2)) & 3) as usize];
    }
    out
}

/// 16-byte BC3 block: 8 bytes interpolated alpha + an always-4-color BC1 color block.
fn decode_bc3_block(b: &[u8]) -> [[u8; 4]; 16] {
    let a0 = b[0] as u32;
    let a1 = b[1] as u32;
    let mut apal = [0u8; 8];
    apal[0] = a0 as u8;
    apal[1] = a1 as u8;
    if a0 > a1 {
        for k in 1..7u32 {
            apal[(k + 1) as usize] = (((7 - k) * a0 + k * a1) / 7) as u8;
        }
    } else {
        for k in 1..5u32 {
            apal[(k + 1) as usize] = (((5 - k) * a0 + k * a1) / 5) as u8;
        }
        apal[6] = 0;
        apal[7] = 255;
    }
    // 48-bit little-endian index stream, 3 bits per texel.
    let mut abits = 0u64;
    for (i, &byte) in b[2..8].iter().enumerate() {
        abits |= (byte as u64) << (8 * i);
    }
    let mut out = decode_bc1_block(&b[8..16], false);
    for (t, texel) in out.iter_mut().enumerate() {
        texel[3] = apal[((abits >> (t * 3)) & 7) as usize];
    }
    out
}

pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}
