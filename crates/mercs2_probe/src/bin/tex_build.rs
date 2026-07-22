//! Build a Mercenaries 2 texture asset from a glTF's embedded atlas.
//!
//! An imported character wears the DONOR's textures until its own are packed and the material is
//! repointed, which is exactly what made `pmc_hum_fiddy` unrecognisable: correct geometry sampling
//! an arbitrary region of someone else's sheet.
//!
//! Retail character textures, measured with `--tex-check`, are the spec this matches:
//!
//!   diffuse   512^2 BC1  8 mips  174,760 B      (pmc_hum_mattias_v2_ub)
//!   specular  512^2 BC1  8 mips  174,760 B      (_ub_sm)
//!   normal    512^2 BC3  8 mips  349,520 B      (_ub_nm)
//!   eyes      128^2 BC1  6 mips   10,920 B      (pmc_hum_chris_eyes)
//!
//! all FULLY RESIDENT — a character ships its whole mip chain rather than streaming the fine levels
//! from finer c3-cell blocks the way world textures do.
//!
//!   tex_build <model.glb> <image-index> <name> <out.ucfx>
//!             [--size 512] [--kind diffuse|normal|specular] [--dump-dds <f.dds>]
//!
//! `--kind normal` applies the DXT5nm swizzle the engine's shader decodes (X in ALPHA, Y in GREEN,
//! Z reconstructed — see `shader.wgsl`), and encodes BC3 because that is where the 8-bit alpha
//! lives. `--kind specular` DERIVES a map from the image's luminance: the source glTF carries no
//! specular or metallic-roughness texture at all, and retail binds an `_sm` for every slot.

use mercs2_formats::texture_encode::{encode_bc1, encode_bc3, mip_chain, ucfx_texture};

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

/// Pull image `idx`'s raw bytes out of a GLB's JSON + BIN chunks.
fn glb_image(path: &str, idx: usize) -> Result<Vec<u8>, String> {
    let d = std::fs::read(path).map_err(|e| e.to_string())?;
    if d.len() < 12 || &d[0..4] != b"glTF" {
        return Err("not a GLB".into());
    }
    let (mut json, mut bin) = (None, None);
    let mut off = 12usize;
    while off + 8 <= d.len() {
        let ln = u32::from_le_bytes(d[off..off + 4].try_into().unwrap()) as usize;
        let ty = u32::from_le_bytes(d[off + 4..off + 8].try_into().unwrap());
        let body = &d[off + 8..(off + 8 + ln).min(d.len())];
        match ty {
            0x4E4F_534A => json = Some(body.to_vec()),
            0x004E_4942 => bin = Some(body.to_vec()),
            _ => {}
        }
        off += 8 + ln;
    }
    let json = String::from_utf8(json.ok_or("no JSON chunk")?).map_err(|e| e.to_string())?;
    let bin = bin.ok_or("no BIN chunk")?;

    // Minimal scan rather than a JSON dependency: find images[idx].bufferView, then that view's
    // byteOffset/byteLength. The probe crate has no serde_json and this is the only field needed.
    let imgs = json.find("\"images\"").ok_or("no images")?;
    let seg = &json[imgs..];
    let mut views = Vec::new();
    let mut p = 0usize;
    let end = seg.find("],").map(|e| e + 1).unwrap_or(seg.len());
    while let Some(k) = seg[p..end].find("\"bufferView\"") {
        let s = p + k + "\"bufferView\"".len();
        let rest = &seg[s..end];
        let digits: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
        views.push(digits.parse::<usize>().map_err(|e| e.to_string())?);
        p = s;
    }
    let bv = *views.get(idx).ok_or_else(|| format!("image {idx} not found ({} images)", views.len()))?;

    let bvs = json.find("\"bufferViews\"").ok_or("no bufferViews")?;
    let bseg = &json[bvs..];
    let mut entries: Vec<(usize, usize)> = Vec::new();
    let mut q = 0usize;
    while let Some(k) = bseg[q..].find("{") {
        let s = q + k;
        let e = bseg[s..].find('}').map(|e| s + e).unwrap_or(bseg.len());
        let obj = &bseg[s..e];
        let num = |key: &str| -> usize {
            obj.find(key)
                .map(|i| {
                    obj[i + key.len()..]
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse()
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        };
        entries.push((num("\"byteOffset\""), num("\"byteLength\"")));
        q = e + 1;
        if entries.len() > bv {
            break;
        }
    }
    let (o, l) = *entries.get(bv).ok_or("bufferView missing")?;
    Ok(bin[o..o + l].to_vec())
}

/// Decode a PNG to RGBA8.
fn decode_png(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let dec = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut rd = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; rd.output_buffer_size()];
    let info = rd.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let src = &buf[..info.buffer_size()];
    let mut rgba = vec![255u8; w * h * 4];
    match info.color_type {
        png::ColorType::Rgba => rgba.copy_from_slice(src),
        png::ColorType::Rgb => {
            for i in 0..w * h {
                rgba[i * 4..i * 4 + 3].copy_from_slice(&src[i * 3..i * 3 + 3]);
            }
        }
        png::ColorType::Grayscale => {
            for i in 0..w * h {
                rgba[i * 4] = src[i];
                rgba[i * 4 + 1] = src[i];
                rgba[i * 4 + 2] = src[i];
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for i in 0..w * h {
                rgba[i * 4] = src[i * 2];
                rgba[i * 4 + 1] = src[i * 2];
                rgba[i * 4 + 2] = src[i * 2];
                rgba[i * 4 + 3] = src[i * 2 + 1];
            }
        }
        c => return Err(format!("unsupported PNG colour type {c:?}")),
    }
    Ok((w, h, rgba))
}

/// Box-average resize to `(tw, th)`. Averaging the whole source footprint (not point-sampling)
/// is what keeps a 1024 -> 512 halving free of the shimmer a nearest-neighbour pick introduces.
fn resize(w: usize, h: usize, rgba: &[u8], tw: usize, th: usize) -> Vec<u8> {
    if w == tw && h == th {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; tw * th * 4];
    for y in 0..th {
        let (y0, y1) = (y * h / th, (((y + 1) * h).div_ceil(th)).min(h).max(y * h / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, (((x + 1) * w).div_ceil(tw)).min(w).max(x * w / tw + 1));
            let mut acc = [0.0f32; 4];
            let mut n = 0.0f32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let s = (sy * w + sx) * 4;
                    for k in 0..4 {
                        acc[k] += rgba[s + k] as f32;
                    }
                    n += 1.0;
                }
            }
            for k in 0..4 {
                out[(y * tw + x) * 4 + k] = (acc[k] / n.max(1.0)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: tex_build <model.glb> <image-index> <name> <out.ucfx> [--size N] [--kind diffuse|normal|specular] [--dump-dds f.dds]");
        std::process::exit(2);
    }
    let (glb, idx, name, out) = (&a[1], a[2].parse::<usize>().expect("image index"), &a[3], &a[4]);
    let kind = flag(&a, "--kind").unwrap_or("diffuse").to_string();
    let size: usize = flag(&a, "--size").and_then(|s| s.parse().ok()).unwrap_or(512);

    let png_bytes = glb_image(glb, idx).expect("glb image");
    let (w, h, rgba) = decode_png(&png_bytes).expect("decode png");
    let rgba = resize(w, h, &rgba, size, size);
    println!("{name}: image {idx} {w}x{h} -> {size}x{size}  kind={kind}");

    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let (body, fourcc) = match kind.as_str() {
        "normal" => {
            // DXT5nm: the shader reads X from ALPHA and Y from GREEN, then rebuilds Z. Storing X in
            // the 8-bit interpolated alpha is the whole point of BC3 here -- it gives X far more
            // precision than a 5-bit BC1 red channel, which is why the format is used for normals.
            let mut px = vec![0.0f32; size * size * 4];
            for i in 0..size * size {
                let (r, g) = (rgba[i * 4] as f32, rgba[i * 4 + 1] as f32);
                px[i * 4] = 0.0;       // R unused
                px[i * 4 + 1] = g;     // G = Y
                px[i * 4 + 2] = 0.0;   // B unused
                px[i * 4 + 3] = r;     // A = X
            }
            (mip_chain(size, size, 4, &px, |w, h, p| encode_bc3(w, h, p)), *b"DXT5")
        }
        "specular" => {
            // DERIVED, not authored: the source glTF has no specular or metallic-roughness map.
            // Luminance is the honest proxy -- darker cloth reflects less than skin or a cap -- and
            // it at least varies with the material instead of making every surface equally shiny.
            let mut px = vec![0.0f32; size * size * 3];
            for i in 0..size * size {
                let l = 0.2126 * rgba[i * 4] as f32
                    + 0.7152 * rgba[i * 4 + 1] as f32
                    + 0.0722 * rgba[i * 4 + 2] as f32;
                let v = (l * 0.75).clamp(0.0, 255.0);
                for k in 0..3 {
                    px[i * 3 + k] = v;
                }
            }
            (mip_chain(size, size, 3, &px, |w, h, p| encode_bc1(w, h, p)), *b"DXT1")
        }
        _ => {
            let mut px = vec![0.0f32; size * size * 3];
            for i in 0..size * size {
                for k in 0..3 {
                    px[i * 3 + k] = rgba[i * 4 + k] as f32;
                }
            }
            (mip_chain(size, size, 3, &px, |w, h, p| encode_bc1(w, h, p)), *b"DXT1")
        }
    };

    // Optional uncompressed BGRA DDS of exactly what was fed to the encoder, so the Rust port can be
    // byte-diffed against tools/dds_to_ucfx_texture.py rather than trusted.
    if let Some(p) = flag(&a, "--dump-dds") {
        let mut hdr = vec![0u8; 128];
        hdr[0..4].copy_from_slice(b"DDS ");
        let put = |h: &mut Vec<u8>, o: usize, v: u32| h[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut hdr, 4, 124);
        put(&mut hdr, 8, 0x1 | 0x2 | 0x4 | 0x1000);
        put(&mut hdr, 12, size as u32);
        put(&mut hdr, 16, size as u32);
        put(&mut hdr, 76, 32);
        put(&mut hdr, 80, 0x1 | 0x40);
        put(&mut hdr, 88, 32);
        put(&mut hdr, 92, 0x00FF_0000);
        put(&mut hdr, 96, 0x0000_FF00);
        put(&mut hdr, 100, 0x0000_00FF);
        put(&mut hdr, 104, 0xFF00_0000);
        put(&mut hdr, 108, 0x1000);
        let mut d = hdr;
        for i in 0..size * size {
            d.extend_from_slice(&[rgba[i * 4 + 2], rgba[i * 4 + 1], rgba[i * 4], rgba[i * 4 + 3]]);
        }
        std::fs::write(p, &d).expect("write dds");
        println!("  dumped {p} ({} bytes)", d.len());
    }

    let container = ucfx_texture(name, size, size, &fourcc, &body);
    std::fs::write(out, &container).expect("write");
    println!(
        "  {name} = 0x{hash:08X}  {} mips  BODY {} B  container {} B -> {out}",
        mercs2_formats::texture_encode::mip_count(size, size),
        body.len(),
        container.len()
    );
}
