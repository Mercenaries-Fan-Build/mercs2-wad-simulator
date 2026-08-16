//! `phy2_census` — reconnaissance for the AUTHORED-PHY2 in-game gate.
//!
//! For a target block, dump per-collision-container: SEGM record count, the parsed shape census
//! (per-mesh tri/vert counts), the raw packfile object graph (virtual-fixup class order + class
//! counts). This is what decides single-shape (`build_phy2` as-is) vs multi-shape authoring: the
//! shape count MUST match the SEGM record count, and the multi-shape topology must be reproduced.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_packfile_raw, parse_phy2_body, Shape, HAVOK_MAGIC};
use mercs2_formats::model_cubeize::parse_segm;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}

/// Locate the PHY2 chunk inside a UCFX container (mirrors mopp_overlay_forge).
fn phy2_span(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
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

    let vz = game_paths::vz_wad(Path::new(".")).expect("vz.wad not found");
    let mut f = std::fs::File::open(&vz).expect("open vz.wad");
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let dec = decompress_block(&mut f, &ar.indx, block).expect("decompress");
    let (parsed, _issues) = walk_decompressed_block(&dec, "census");

    println!("=== phy2_census block {block} ===");
    for (i, c) in parsed.containers.iter().enumerate() {
        let Some((s, sz)) = phy2_span(c) else { continue };
        let nh = parsed.entries[i].name_hash;
        let segm = parse_segm(c);
        let body = &c[s..s + sz];
        println!("\ncontainer[{i}] name 0x{nh:08X}  container {} B  PHY2 {sz} B", c.len());
        println!("  SEGM records: {}", segm.len());
        for (k, r) in segm.iter().enumerate() {
            println!(
                "    SEGM[{k}] bone={} seg_id={} mask=0x{:02X}",
                r.bone, r.seg_id, r.state_mask
            );
        }
        match parse_phy2_body(body) {
            Ok(pf) => {
                println!("  version {}  class_counts {:?}", pf.version, pf.class_counts);
                let mut mesh_i = 0;
                for (si, sh) in pf.shapes.iter().enumerate() {
                    match sh {
                        Shape::Mesh(m) => {
                            println!(
                                "    shape[{si}] MESH #{mesh_i}: {} tris, {} verts, decoded={}",
                                m.indices.len(),
                                m.vertices.len(),
                                !m.indices.is_empty()
                            );
                            mesh_i += 1;
                        }
                        Shape::Convex(h) => {
                            println!("    shape[{si}] CONVEX: {} verts", h.vertices.len())
                        }
                        other => println!("    shape[{si}] {other:?}"),
                    }
                }
                // Raw packfile virtual-fixup object graph (class order = topology).
                if let Some(off) = body.windows(8).position(|w| w == HAVOK_MAGIC) {
                    if let Ok(raw) = parse_packfile_raw(&body[off..]) {
                        let mut vf = raw.vfixups.clone();
                        vf.sort_by_key(|(s, _)| *s);
                        println!("  vfixup object graph ({} objects):", vf.len());
                        for (src, cn) in &vf {
                            println!("      @{src:>6}  {cn}");
                        }
                    }
                }
            }
            Err(e) => println!("  parse_phy2_body FAILED: {e}"),
        }
    }
}
