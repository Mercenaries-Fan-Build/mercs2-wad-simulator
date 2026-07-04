//! Dev bin: decode a model's diffuse textures (resident low-res AND hi-res streamed) to BMP so we can
//! eyeball whether the hi-res streaming assembly is correct (e.g. the PMC floor "shifted/transposed"
//! report). BMP is uncompressed → no extra deps. Separate binary so it isn't locked by a running game.
//!
//!   cargo run -p mercs2_engine --bin tex_dump -- 0x39AF17DC <out_dir>

use mercs2_engine::{game_world, wad};
use mercs2_formats::texture::{TexFormat, TextureData};

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1f) as u32;
    let g = ((c >> 5) & 0x3f) as u32;
    let b = (c & 0x1f) as u32;
    [((r * 255 + 15) / 31) as u8, ((g * 255 + 31) / 63) as u8, ((b * 255 + 15) / 31) as u8]
}

/// Decode BC1/BC3 mip0 → RGB (ignores alpha; colour endpoints are what we care about here).
fn decode_mip0(td: &TextureData) -> Option<(u32, u32, Vec<[u8; 3]>)> {
    let (w, h) = (td.width as usize, td.height as usize);
    let (block_bytes, color_off) = match td.format {
        TexFormat::Bc1 => (8usize, 0usize),
        TexFormat::Bc3 => (16usize, 8usize),
    };
    let bw = (w + 3) / 4;
    let bh = (h + 3) / 4;
    if td.mip0.len() < bw * bh * block_bytes {
        return None;
    }
    let mut px = vec![[0u8; 3]; w * h];
    for by in 0..bh {
        for bx in 0..bw {
            let blk = &td.mip0[(by * bw + bx) * block_bytes + color_off..];
            let c0 = u16::from_le_bytes([blk[0], blk[1]]);
            let c1 = u16::from_le_bytes([blk[2], blk[3]]);
            let (e0, e1) = (rgb565(c0), rgb565(c1));
            let lerp = |a: [u8; 3], b: [u8; 3], t: u32| {
                [((a[0] as u32 * (3 - t) + b[0] as u32 * t) / 3) as u8,
                 ((a[1] as u32 * (3 - t) + b[1] as u32 * t) / 3) as u8,
                 ((a[2] as u32 * (3 - t) + b[2] as u32 * t) / 3) as u8]
            };
            // BC3 colour is always 4-colour; BC1 uses the c0>c1 rule.
            let pal = if td.format == TexFormat::Bc1 && c0 <= c1 {
                [e0, e1, [((e0[0] as u16 + e1[0] as u16) / 2) as u8, ((e0[1] as u16 + e1[1] as u16) / 2) as u8, ((e0[2] as u16 + e1[2] as u16) / 2) as u8], [0, 0, 0]]
            } else {
                [e0, e1, lerp(e0, e1, 1), lerp(e0, e1, 2)]
            };
            let bits = u32::from_le_bytes([blk[4], blk[5], blk[6], blk[7]]);
            for py in 0..4 {
                for pxi in 0..4 {
                    let (x, y) = (bx * 4 + pxi, by * 4 + py);
                    if x < w && y < h {
                        let idx = ((bits >> (2 * (py * 4 + pxi))) & 3) as usize;
                        px[y * w + x] = pal[idx];
                    }
                }
            }
        }
    }
    Some((td.width, td.height, px))
}

fn write_bmp(path: &str, w: u32, h: u32, px: &[[u8; 3]]) {
    let row_pad = (4 - (w as usize * 3) % 4) % 4;
    let img_size = (w as usize * 3 + row_pad) * h as usize;
    let mut f = Vec::with_capacity(54 + img_size);
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&(54 + img_size as u32).to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    f.extend_from_slice(&54u32.to_le_bytes());
    f.extend_from_slice(&40u32.to_le_bytes());
    f.extend_from_slice(&(w as i32).to_le_bytes());
    f.extend_from_slice(&(h as i32).to_le_bytes()); // positive = bottom-up
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&24u16.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    f.extend_from_slice(&(img_size as u32).to_le_bytes());
    f.extend_from_slice(&[0u8; 16]);
    for y in (0..h as usize).rev() {
        for x in 0..w as usize {
            let p = px[y * w as usize + x];
            f.extend_from_slice(&[p[2], p[1], p[0]]); // BGR
        }
        f.extend(std::iter::repeat(0u8).take(row_pad));
    }
    std::fs::write(path, f).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhash = args.get(1).and_then(|a| a.strip_prefix("0x")).and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0x39AF17DC);
    let out = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    // Work at the MTRL level: every material carries a FULL texture list (all slots), not just the
    // diffuse(0)/normal(2) the mesh builder keeps. Dump EVERY texture in EVERY slot so nothing is hidden.
    let container = wad::extract_container(&mut w, mhash).expect("extract container");
    let mats = mercs2_formats::texture::parse_mtrl(&container);
    println!("model 0x{mhash:08X}: {} materials", mats.len());
    let mut texs: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for (i, mat) in mats.iter().enumerate() {
        let slots: Vec<String> = mat.textures.iter().map(|h| format!("0x{h:08X}")).collect();
        println!("  material {i:2}: {} tex slots [{}]", mat.textures.len(), slots.join(", "));
        for &h in &mat.textures {
            if h != 0 && h != 0xFFFFFFFF {
                texs.insert(h);
            }
        }
    }
    println!("== {} distinct textures across all slots; dumping (mip0) ==", texs.len());
    let (mut ok, mut fail) = (0u32, 0u32);
    for th in texs {
        match wad::extract_texture_hires(&mut w, th) {
            Ok(hi) => match decode_mip0(&hi) {
                Some((tw, tht, px)) => {
                    let p = format!("{out}/tex_{th:08X}_{tw}x{tht}.bmp");
                    write_bmp(&p, tw, tht, &px);
                    println!("  wrote tex_{th:08X}_{tw}x{tht}.bmp  ({:?} mips={})", hi.format, hi.mip_count);
                    ok += 1;
                }
                None => {
                    println!("  0x{th:08X}: decode failed ({}x{} {:?})", hi.width, hi.height, hi.format);
                    fail += 1;
                }
            },
            Err(e) => {
                println!("  0x{th:08X}: NOT a resolvable texture ({e})");
                fail += 1;
            }
        }
    }
    println!("== dumped {ok}, {fail} unresolved ==");
}
