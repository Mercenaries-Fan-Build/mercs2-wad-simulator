//! `wrapper_reverse` — FULL byte-level reverse of the PHY2 trailing engine-wrapper (past the Havok
//! packfile) for a single collision container. Dumps the entire wrapper with body-absolute offset
//! annotation so the `0xAA/0xBB/0xCC/0xDD` descriptor-chain framing + the body-absolute pool pointers
//! can be proven. Reads vz.wad via the local config (no env needed).

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
        let pk_end = off + raw.size; // body-absolute start of wrapper
        let wrapper = &body[pk_end..];
        println!("\n=== container[{i}] 0x{nh:08X}  PHY2 {sz} B ===");
        println!("  packfile @+{off}  size {}  pkend(body-abs)@+{pk_end}", raw.size);
        println!("  wrapper: {} B", wrapper.len());
        for (mi, sh) in pf.shapes.iter().enumerate() {
            if let Shape::Mesh(m) = sh {
                if m.indices.is_empty() {
                    continue;
                }
                println!("  mesh(shape[{mi}]): {} tris {} verts  pool_bytes={}",
                    m.indices.len(), m.vertices.len(), m.vertices.len() * 6);
            }
        }

        // Full annotated hex of the wrapper. For each aligned u32, if its value V could be a
        // body-absolute pointer (pk_end <= V <= sz) annotate the target (V - pk_end = wrapper offset).
        println!("  --- FULL WRAPPER (body-abs offset | wrapper-rel | 16 bytes | u32 annotations) ---");
        let n = wrapper.len();
        let mut o = 0usize;
        while o < n {
            let row = &wrapper[o..(o + 16).min(n)];
            let hx: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
            // annotate any aligned u32 in the row that looks like a body-abs ptr
            let mut ann: Vec<String> = Vec::new();
            let mut k = 0;
            while k + 4 <= row.len() {
                if (o + k) % 4 == 0 {
                    let v = u32::from_le_bytes(row[k..k + 4].try_into().unwrap());
                    if v as usize >= pk_end && (v as usize) <= sz {
                        ann.push(format!("+{}:PTR->wrap+{}", o + k, v as usize - pk_end));
                    } else if matches!(v, 0xAAAA_AAAA | 0xBBBB_BBBB | 0xCCCC_CCCC | 0xDDDD_DDDD) {
                        ann.push(format!("+{}:MARK {:08X}", o + k, v));
                    }
                }
                k += 4;
            }
            println!(
                "  b+{:<6} w+{:<5} {:<48} {}",
                pk_end + o,
                o,
                hx.join(" "),
                ann.join("  ")
            );
            o += 16;
        }
    }
}
