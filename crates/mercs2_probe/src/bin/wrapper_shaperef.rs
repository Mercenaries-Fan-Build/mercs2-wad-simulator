//! `wrapper_shaperef` — decode the PHY2 trailing wrapper's per-leaf records field-by-field and classify
//! EVERY field (marker / wrapper-ptr / packfile-shape ptr naming the object / scalar / FF). The original
//! `wrapper_reverse` only annotated u32s in `[pk_end, sz]` — BLIND to a field holding a body-absolute
//! offset `< pk_end` that relocates onto a packfile shape at load, which is the per-leaf "shape
//! back-pointer" the live A/B x32dbg proof showed our author wired all to shape[0].
//!
//! Runs on BOTH the retail floor container AND our authored `build_phy2_multi` output over the same 4
//! decoded meshes, so `record[i] -> shape[i]` (retail) vs `record[i] -> shape[0]` (ours) is visible.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_packfile_raw, parse_phy2_body, MeshShape, Shape, HAVOK_MAGIC};
use mercs2_formats::phy2_build::build_phy2_multi;
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

/// Body-absolute offsets of every named shape object, in address order.
fn shape_objs(body: &[u8], off: usize, raw: &mercs2_formats::havok::RawPackfile) -> Vec<(usize, String)> {
    let _ = body;
    let mut objs: Vec<(usize, String)> = Vec::new();
    let mut per_class: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (src, class) in &raw.vfixups {
        let body_abs = off + raw.obj_abs(*src);
        let idx = per_class.entry(class.clone()).or_insert(0);
        objs.push((body_abs, format!("{class}[{idx}]")));
        *idx += 1;
    }
    objs.sort_by_key(|(a, _)| *a);
    objs
}

fn dump(tag: &str, body: &[u8]) {
    let off = body.windows(8).position(|w| w == HAVOK_MAGIC).unwrap();
    let raw = parse_packfile_raw(&body[off..]).unwrap();
    let pk_end = off + raw.size;
    let sz = body.len();
    let objs = shape_objs(body, off, &raw);
    let name_of = |v: usize| -> Option<String> { objs.iter().find(|(a, _)| *a == v).map(|(_, c)| c.clone()) };

    println!("\n######## {tag}  PHY2 {sz} B  packfile {} B  pkend@{pk_end}  wrapper {} B ########", raw.size, sz - pk_end);
    print!("  shapes: ");
    for (a, c) in &objs {
        if c.starts_with("hkpMoppBvTreeShape") {
            print!("{c}@{a}  ");
        }
    }
    println!();

    let rd = |abs: usize| u32::from_le_bytes(body[abs..abs + 4].try_into().unwrap());
    let classify = |v: u32| -> String {
        let vu = v as usize;
        if v == 0xFFFF_FFFF {
            "FF".into()
        } else if matches!(v, 0xAAAA_AAAA | 0xBBBB_BBBB | 0xCCCC_CCCC | 0xDDDD_DDDD | 0xEEEE_EEEE) {
            format!("MARK{v:08X}")
        } else if vu >= pk_end && vu <= sz {
            format!("wrap+{}", vu - pk_end)
        } else if vu >= off && vu < pk_end {
            match name_of(vu) {
                Some(nm) => format!("PKSHAPE->{nm}"),
                None => format!("pk@{vu}"),
            }
        } else {
            format!("{v}")
        }
    };

    // Walk the wrapper, decode every EE and every leaf AA record fully.
    let wrapper_len = sz - pk_end;
    let mut o = 0usize;
    let mut ee_idx = 0;
    let mut ee_shape_field: Vec<(usize, String)> = Vec::new();
    while o + 4 <= wrapper_len {
        let marker = rd(pk_end + o);
        let len = match marker {
            0xAAAA_AAAA => 68usize,
            0xCCCC_CCCC | 0xEEEE_EEEE => 108,
            _ => {
                o += 4;
                continue;
            }
        };
        if marker == 0xEEEE_EEEE {
            print!("  EE[{ee_idx}] @wrap+{o}:");
            let mut fo = 4;
            let mut shape_hit = String::new();
            while fo + 4 <= len && o + fo + 4 <= wrapper_len {
                let v = rd(pk_end + o + fo);
                let c = classify(v);
                if c.starts_with("PKSHAPE") {
                    print!(" +{fo}:[{c}]");
                    shape_hit = c.clone();
                } else if !c.starts_with("FF") && c != "0" && !c.chars().all(|ch| ch.is_ascii_digit()) {
                    print!(" +{fo}:{c}");
                }
                fo += 4;
            }
            println!();
            ee_shape_field.push((o, shape_hit));
            ee_idx += 1;
        } else if marker == 0xAAAA_AAAA {
            // leaf AA if it carries a +44 -> EE ptr; internal if +40 -> CC.
            let f40 = rd(pk_end + o + 40);
            let f44 = rd(pk_end + o + 44);
            let is_leaf = classify(f44).starts_with("wrap") && {
                let t = f44 as usize;
                t < sz && rd(t) == 0xEEEE_EEEE
            };
            let is_internal = classify(f40).starts_with("wrap") && {
                let t = f40 as usize;
                t < sz && rd(t) == 0xCCCC_CCCC
            };
            let kind = if is_leaf { "AA-leaf" } else if is_internal { "AA-internal" } else { "AA" };
            print!("  {kind} @wrap+{o}:");
            for foff in [32usize, 36, 40, 44, 48, 52, 56, 60] {
                let v = rd(pk_end + o + foff);
                print!(" +{foff}:{}", classify(v));
            }
            println!();
        }
        o += len;
    }
    println!("  --- shape-ref VERDICT (per EE, the field pointing to a packfile shape) ---");
    for (k, (rel, hit)) in ee_shape_field.iter().enumerate() {
        println!("    EE[{k}]@wrap+{rel}: {}", if hit.is_empty() { "NONE".into() } else { hit.clone() });
    }

    // GLOBAL-FIXUP audit: parse the packfile's global-fixup table (which the reader IGNORES) and print
    // where the WpArray element pointers (elem[i] @ elem_off + i*4) actually resolve. This is the ONE
    // place the WpArray->bvtree[i] binding lives; if all elements resolve to bvtree[0] the loader builds
    // N runtime records all pointing at shape[0] (the live-observed bug) and nothing else offline sees it.
    {
        let pk = &body[off..];
        // locate section-header table: 0x40 header + 3 × 48-byte section headers @ 0x40.
        let sec = |s: usize, k: usize| u32::from_le_bytes(pk[0x40 + s * 48 + 20 + k * 4..0x40 + s * 48 + 20 + k * 4 + 4].try_into().unwrap()) as usize;
        let body0 = 0x40 + 3 * 48;
        let data_pk_local = body0 + sec(0, 6) + sec(1, 6); // __data__ body start (in pk)
        let (d_gf, d_vf) = (sec(2, 2), sec(2, 3));
        let elems: Vec<(usize, usize)> = {
            // Find WpArray object + its element storage via vfixups + local fixup on WpArray+8.
            let wparray_src = raw.vfixups.iter().find(|(_, c)| c == "WpArray").map(|(s, _)| *s).unwrap();
            let elem_data = *raw.lf.get(&(wparray_src + 8)).unwrap(); // data-rel offset of element array
            let n = u32::from_le_bytes(pk[data_pk_local + wparray_src + 12..data_pk_local + wparray_src + 16].try_into().unwrap()) as usize;
            (0..n).map(|i| (i, elem_data + i * 4)).collect()
        };
        println!("  --- WpArray element -> bvtree binding (from the GLOBAL-FIXUP table) ---");
        let mut g = data_pk_local + d_gf;
        let mut gf: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        while g + 12 <= data_pk_local + d_vf {
            let src = u32::from_le_bytes(pk[g..g + 4].try_into().unwrap()) as usize;
            let dst = u32::from_le_bytes(pk[g + 8..g + 12].try_into().unwrap()) as usize;
            if src == 0xFFFF_FFFF {
                break;
            }
            gf.insert(src, dst);
            g += 12;
        }
        for (i, elem_src) in &elems {
            match gf.get(elem_src) {
                Some(dst) => {
                    let body_abs = off + data_pk_local + dst;
                    let nm = name_of(body_abs).unwrap_or_else(|| format!("data+{dst}"));
                    println!("    elem[{i}] (data+{elem_src}) -> {nm}  (body[{body_abs}])");
                }
                None => println!("    elem[{i}] (data+{elem_src}) -> NO GLOBAL FIXUP (null!)"),
            }
        }
        // Every bvtree's +16 (m_code) and +52 (child mesh) global fixup.
        for (bsrc, _) in raw.vfixups.iter().filter(|(_, c)| c == "hkpMoppBvTreeShape") {
            for (fo, lbl) in [(16usize, "m_code"), (52, "child")] {
                if let Some(dst) = gf.get(&(bsrc + fo)) {
                    let nm = name_of(off + data_pk_local + dst).unwrap_or_else(|| format!("data+{dst}"));
                    println!("    bvtree@{}+{fo}({lbl}) -> {nm}", off + data_pk_local + bsrc);
                }
            }
        }
    }

    // WHOLE-BODY scan: for each bvtree shape object, find EVERY aligned u32 in the body that equals its
    // body-absolute offset (a potential pointer to it), with the region (packfile / wrapper).
    println!("  --- whole-body scan: pointers TO each bvtree shape (value == its body-abs offset) ---");
    for (a, c) in objs.iter().filter(|(_, c)| c.starts_with("hkpMoppBvTreeShape")) {
        let mut locs: Vec<String> = Vec::new();
        let mut i = 0usize;
        while i + 4 <= sz {
            if u32::from_le_bytes(body[i..i + 4].try_into().unwrap()) as usize == *a {
                let region = if i < pk_end { "packfile" } else { "wrapper" };
                locs.push(format!("@{i}({region})"));
            }
            i += 4;
        }
        println!("    {c}@{a}: {} ref(s): {}", locs.len(), locs.join(" "));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block: u16 = arg(&args, "--block").and_then(|s| s.parse().ok()).unwrap_or(2612);
    let want = arg(&args, "--model")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x39AF_17DC);

    let vz = game_paths::vz_wad(Path::new(".")).expect("vz.wad");
    let mut f = std::fs::File::open(&vz).expect("open");
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let dec = decompress_block(&mut f, &ar.indx, block).expect("decompress");
    let (parsed, _i) = walk_decompressed_block(&dec, "wd");

    for (i, c) in parsed.containers.iter().enumerate() {
        if parsed.entries[i].name_hash != want {
            continue;
        }
        let Some((s, sz)) = phy2_span(c) else { continue };
        let body = c[s..s + sz].to_vec();
        let pf = parse_phy2_body(&body).expect("parse floor");
        let meshes: Vec<MeshShape> = pf
            .shapes
            .iter()
            .filter_map(|sh| match sh {
                Shape::Mesh(m) if !m.indices.is_empty() => Some(m.clone()),
                _ => None,
            })
            .collect();
        dump(&format!("RETAIL 0x{want:08X}"), &body);

        // Author from the same 4 decoded meshes and dump identically.
        let soup: Vec<(Vec<[u32; 3]>, Vec<[f32; 3]>)> = meshes
            .iter()
            .map(|m| {
                (
                    m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect(),
                    m.vertices.clone(),
                )
            })
            .collect();
        let authored = build_phy2_multi("floor", &soup).expect("build_phy2_multi");
        dump("AUTHORED build_phy2_multi", &authored);
        return;
    }
    eprintln!("container 0x{want:08X} not found in block {block}");
}
