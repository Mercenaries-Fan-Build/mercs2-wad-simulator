//! `wrapper_dump` — analyze the PHY2 trailing engine-wrapper (past the Havok packfile) that holds the
//! WpMeshShape16 quantized vertex pools. Reports packfile size, wrapper size, and for each mesh the pool
//! base the reader's content-scan locks onto (offset-from-packfile-end), plus a hex window of the wrapper
//! head. This is what decides how to author a MULTI-pool wrapper for the floor.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_packfile_raw, parse_phy2_body, Shape, HAVOK_MAGIC};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn phy2_span(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block: u16 = arg(&args, "--block").and_then(|s| s.parse().ok()).unwrap_or(2612);
    let want: Option<u32> = arg(&args, "--model")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());

    let vz = game_paths::vz_wad(Path::new(".")).expect("vz.wad");
    let mut f = std::fs::File::open(&vz).expect("open");
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let dec = decompress_block(&mut f, &ar.indx, block).expect("decompress");
    let (parsed, _i) = walk_decompressed_block(&dec, "wd");

    for (i, c) in parsed.containers.iter().enumerate() {
        let nh = parsed.entries[i].name_hash;
        if let Some(w) = want {
            if nh != w {
                continue;
            }
        }
        let Some((s, sz)) = phy2_span(c) else { continue };
        let body = &c[s..s + sz];
        let Ok(pf) = parse_phy2_body(body) else { continue };
        let n_mesh = pf.shapes.iter().filter(|s| matches!(s, Shape::Mesh(m) if !m.indices.is_empty())).count();
        if n_mesh == 0 {
            continue;
        }
        let off = body.windows(8).position(|w| w == HAVOK_MAGIC).unwrap();
        let raw = parse_packfile_raw(&body[off..]).unwrap();
        let pk_end = off + raw.size; // absolute in `body`
        let wrapper = &body[pk_end..];
        println!("\n=== container[{i}] 0x{nh:08X}  PHY2 {sz} B ===");
        println!("  packfile @+{off}  size {}  packfile-end@+{pk_end}", raw.size);
        println!("  wrapper: {} B (body {} - pkend {})", wrapper.len(), sz, pk_end);
        // meshes: report tris/verts + brute-force pool base (offset-from-packfile-end) using each mesh's
        // decoded verts to re-quantize is not available; instead report the reader's decoded verts bbox.
        for (mi, sh) in pf.shapes.iter().enumerate() {
            if let Shape::Mesh(m) = sh {
                if m.indices.is_empty() {
                    continue;
                }
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &m.vertices {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
                println!(
                    "  mesh(shape[{mi}]): {} tris {} verts  bbox min[{:.1},{:.1},{:.1}] max[{:.1},{:.1},{:.1}]",
                    m.indices.len(), m.vertices.len(), lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
                );
            }
        }
        // hex of wrapper head + tail
        let hexdump = |label: &str, data: &[u8], base: usize| {
            println!("  {label} ({} B shown):", data.len());
            for (r, ch) in data.chunks(16).enumerate() {
                let hx: Vec<String> = ch.iter().map(|b| format!("{b:02x}")).collect();
                println!("    +{:<6} {}", base + r * 16, hx.join(" "));
            }
        };
        let head = &wrapper[..wrapper.len().min(64)];
        hexdump("wrapper HEAD", head, 0);
        // scan for u32 markers 0xAAAAAAAA / 0xBBBBBBBB / 0xCCCCCCCC / 0xDDDDDDDD in wrapper
        let mut markers = Vec::new();
        let mut o = 0;
        while o + 4 <= wrapper.len() {
            let v = u32::from_le_bytes(wrapper[o..o + 4].try_into().unwrap());
            if matches!(v, 0xAAAA_AAAA | 0xBBBB_BBBB | 0xCCCC_CCCC | 0xDDDD_DDDD | 0x39) {
                markers.push((o, v));
            }
            o += 4;
        }
        println!("  aligned u32 markers in wrapper: {}", markers.len());
        for (o, v) in markers.iter().take(40) {
            println!("    wrapper+{o:<7} 0x{v:08X}");
        }
    }
}
