//! What a model's textures look like RESIDENT vs HIRES — the workshop's export gap, measured.
//!
//! The workshop's model-load path fetches `extract_texture` (resident) and hands that `TextureData`
//! to the preview AND to both exporters. The resident ASET block ships only the coarse mip TAIL, so
//! `mip0` holds a few hundred bytes while `width`/`height` still say 512x512 — every consumer that
//! decodes `mip0` at `width x height` (e.g. `texpng::decode_bc`) therefore fills a fraction of the
//! image and leaves the rest zeroed. This prints both sides so the gap is a number, not a claim.
//!
//! usage: texcmp [0xMODELHASH]   (default: ch_veh_tank_ztz98)

use mercs2_engine::wad;
use mercs2_formats::texture::{TexFormat, TextureData};

/// Bytes one full mip level occupies at this size/format.
fn mip_bytes(w: u32, h: u32, f: TexFormat) -> usize {
    let bb = match f {
        TexFormat::Bc1 => 8,
        TexFormat::Bc3 => 16,
    };
    (w as usize).div_ceil(4).max(1) * (h as usize).div_ceil(4).max(1) * bb
}

/// The finest mip level actually covered by `mip0`'s bytes — i.e. the real resolution a decoder can
/// honestly produce. Level 0 = full `width x height`.
fn usable_level(td: &TextureData) -> u32 {
    for l in 0..12u32 {
        let (w, h) = ((td.width >> l).max(1), (td.height >> l).max(1));
        if mip_bytes(w, h, td.format) <= td.mip0.len() {
            return l;
        }
    }
    11
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhash = args
        .get(1)
        .and_then(|a| a.strip_prefix("0x"))
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or(0xF88147A1); // ch_veh_tank_ztz98
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    let container = wad::extract_container(&mut w, mhash).expect("extract container");
    let mats = mercs2_formats::texture::parse_mtrl(&container);
    let mut texs: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for m in &mats {
        for &h in &m.textures {
            if h != 0 && h != 0xFFFF_FFFF {
                texs.insert(h);
            }
        }
    }

    println!("model 0x{mhash:08X}: {} materials, {} distinct textures", mats.len(), texs.len());
    println!(
        "{:<12} {:>10} {:>12} {:>14} | {:>12} {:>14} {:>8}",
        "texture", "dims", "resident B", "resident real", "hires B", "hires real", "gain"
    );

    let (mut upgraded, mut same) = (0u32, 0u32);
    for &th in &texs {
        let Ok(res) = wad::extract_texture(&mut w, th) else { continue };
        let hi = wad::extract_texture_hires(&mut w, th).unwrap_or_else(|_| res.clone());

        let rl = usable_level(&res);
        let hl = usable_level(&hi);
        let (rw, rh) = ((res.width >> rl).max(1), (res.height >> rl).max(1));
        let (hw, hh) = ((hi.width >> hl).max(1), (hi.height >> hl).max(1));
        if hl < rl {
            upgraded += 1;
        } else {
            same += 1;
        }
        println!(
            "0x{th:08X}   {:>4}x{:<5} {:>12} {:>9}x{:<4} | {:>12} {:>9}x{:<4} {:>7}",
            res.width,
            res.height,
            res.mip0.len(),
            rw,
            rh,
            hi.mip0.len(),
            hw,
            hh,
            if hl < rl { format!("{}x", 1 << (rl - hl)) } else { "-".into() }
        );
    }
    println!("\n{upgraded} textures gain resolution from the hires chain; {same} unchanged.");
    println!(
        "'real' = the resolution mip0's bytes actually cover. Where resident real << dims, a decoder\n\
         that trusts width/height (texpng::decode_bc) writes a mostly-EMPTY image at the full size."
    );
}
