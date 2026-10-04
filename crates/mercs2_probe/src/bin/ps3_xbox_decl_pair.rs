//! Pair every PS3-side `decl` body with the Xbox-side `decl` body at the SAME
//! (block_index, descriptor_index) across the PS3 SCFF and the Xbox DOH. Emits
//! a TSV: (block, desc, type_hash, ps3_hex, xbox_hex, parent). Lets a reader
//! verify whether a PS3 compact pattern uniquely determines the Xbox decl
//! (= an oracle usable to drive the PS3 convert_decl path).
//!
//!   cargo run --release -p mercs2_probe --bin ps3_xbox_decl_pair -- \
//!       --ps3 <ps3.scff> --xbox <xbox.doh> --out <dir>

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::ffcs::read_u32_be;

fn arg_str(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1).cloned())
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes { s.push_str(&format!("{:02x}", b)); }
    s
}

/// (block_path, desc_idx, type_hash) -> (parent_tag, decl_body_bytes, info_stride_opt).
type DeclMap = BTreeMap<(String, usize, u32), (String, Vec<u8>, Option<u32>)>;

fn scan_decls(scff_path: &str) -> DeclMap {
    let p = PathBuf::from(scff_path);
    let (doh, _) = load_stfs_or_doh(&p).expect("load scff/doh");
    let (_, rows) = parse_be_ffcs(&doh).expect("parse FFCS");
    let indx_row = rows.iter().find(|r| r.tag == "INDX").expect("INDX").clone();
    let indx = parse_be_indx(&doh, indx_row.offset as usize, indx_row.meta as usize);
    let pths = rows.iter().find(|r| r.tag == "PTHS").map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize)).unwrap_or_default();

    let mut out: DeclMap = BTreeMap::new();
    for (blk_idx, e) in indx.iter().enumerate() {
        let path = pths.get(blk_idx).cloned().unwrap_or_else(|| format!("block_{blk_idx:05}"));
        let block_offset = e.file_offset();
        let block_size = e.page_count as usize * PAGE_SIZE;
        if block_offset + 4 > doh.len() { continue; }
        let slice = &doh[block_offset..(block_offset + block_size).min(doh.len())];
        let decompressed: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
            match decompress_be_sges(slice, 0, slice.len()) { Ok(d) => d, Err(_) => continue }
        } else if slice.len() >= 8 {
            let rec = read_u32_be(slice, 0) as usize;
            let header_end = 4 + rec * 16;
            let first_tag = slice.get(header_end..header_end + 4);
            if rec > 0 && rec < 5000 && first_tag == Some(b"XFCU") {
                let mut d = slice.to_vec();
                let mut z = d.len();
                while z > 4 && d[z - 1] == 0 { z -= 1; }
                z = (z + 3) & !3;
                d.truncate(z);
                d
            } else { continue; }
        } else { continue };

        if decompressed.len() < 4 { continue; }
        let entry_count = read_u32_be(&decompressed, 0) as usize;
        let header_size = 4 + entry_count * 16;
        if header_size > decompressed.len() { continue; }
        let mut offset = header_size;
        for ei in 0..entry_count {
            let eoff = 4 + ei * 16;
            let type_hash = read_u32_be(&decompressed, eoff + 4);
            let chunk_size = read_u32_be(&decompressed, eoff + 12) as usize;
            let container_end = offset + chunk_size;
            if container_end > decompressed.len() { break; }
            let mut container_end_eff = container_end;
            if container_end_eff - offset >= 8 {
                let tail = &decompressed[container_end_eff - 8..container_end_eff - 4];
                if tail == b"CSUM" || tail == b"MUSC" { container_end_eff -= 8; }
            }
            let container = &decompressed[offset..container_end_eff];
            offset = container_end;
            if container.len() < 20 { continue; }
            let magic = &container[0..4];
            let is_be = magic == b"XFCU";
            if !is_be && magic != b"UCFX" { continue; }
            let data_area_off = read_u32_be(container, 4) as usize;
            let n_desc = read_u32_be(container, 16) as usize;
            if n_desc > 10000 { continue; }
            let desc_table_end = 20 + n_desc * 20;
            if desc_table_end > container.len() { continue; }
            let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
            for di in 0..n_desc {
                let row_start = 20 + di * 20;
                let mut tag_bytes = [0u8; 4];
                tag_bytes.copy_from_slice(&container[row_start..row_start + 4]);
                if is_be { tag_bytes.reverse(); }
                if &tag_bytes != b"decl" && &tag_bytes != b"DECL" { continue; }
                let row_u0 = read_u32_be(container, row_start + 4);
                let body_size = read_u32_be(container, row_start + 8) as usize;
                if row_u0 == 0xFFFFFFFF { continue; }
                let abs = data_start + row_u0 as usize;
                let end = abs + body_size;
                if end > container.len() { continue; }
                let body = container[abs..end].to_vec();

                // Walk back from this decl, inside the same STRM group, to find
                // its sibling `info` chunk and read `info.stride` (bytes 4..8
                // of the info body — layout `[flag:u32][stride:u32][vcount:u32]`).
                let mut info_stride: Option<u32> = None;
                let mut parent_tag = String::from("?");
                for parent_di in (0..di).rev() {
                    let prs = 20 + parent_di * 20;
                    let mut pt = [0u8; 4];
                    pt.copy_from_slice(&container[prs..prs + 4]);
                    if is_be { pt.reverse(); }
                    let p_u0 = read_u32_be(container, prs + 4);
                    let p_sz = read_u32_be(container, prs + 8) as usize;
                    if p_u0 == 0xFFFFFFFF {
                        parent_tag = String::from_utf8_lossy(&pt).into_owned();
                        break;
                    }
                    if &pt == b"info" {
                        if p_sz >= 12 {
                            let iabs = data_start + p_u0 as usize;
                            if iabs + 12 <= container.len() {
                                info_stride = Some(read_u32_be(container, iabs + 4));
                            }
                        }
                    }
                }
                out.insert((path.clone(), di, type_hash), (parent_tag, body, info_stride));
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ps3 = arg_str(&args, "--ps3").expect("--ps3 required");
    let xbox = arg_str(&args, "--xbox").expect("--xbox required");
    let out_dir: Option<PathBuf> = arg_str(&args, "--out").map(PathBuf::from);
    if let Some(d) = &out_dir { fs::create_dir_all(d).ok(); }

    eprintln!("[pair] scanning PS3 decls from {ps3}...");
    let p = scan_decls(&ps3);
    eprintln!("[pair] PS3 decl descriptors: {}", p.len());
    eprintln!("[pair] scanning Xbox decls from {xbox}...");
    let x = scan_decls(&xbox);
    eprintln!("[pair] Xbox decl descriptors: {}", x.len());

    // Mapping of (type_hash, ps3_hex, info_stride) -> set of xbox hexes.
    let mut map: BTreeMap<(u32, String, Option<u32>), BTreeMap<String, usize>> = BTreeMap::new();
    let mut tsv = String::new();
    tsv.push_str("block\tdesc\ttype_hash\tparent\tps3_stride\tps3_hex\txbox_stride\txbox_hex\n");
    let mut matched = 0usize;
    let mut only_ps3 = 0usize;
    let mut only_xbox = 0usize;

    for ((path, di, th), (parent, ps3_body, ps3_stride)) in &p {
        let ps3_hex = hex_of(ps3_body);
        let ps3_stride_s = ps3_stride.map(|s| format!("{s}")).unwrap_or_default();
        match x.get(&(path.clone(), *di, *th)) {
            Some((_xp, xbox_body, xbox_stride)) => {
                let xbox_hex = hex_of(xbox_body);
                let xbox_stride_s = xbox_stride.map(|s| format!("{s}")).unwrap_or_default();
                tsv.push_str(&format!("{path}\t{di}\t0x{th:08X}\t{parent}\t{ps3_stride_s}\t{ps3_hex}\t{xbox_stride_s}\t{xbox_hex}\n"));
                let key = (*th, ps3_hex.clone(), *ps3_stride);
                *map.entry(key).or_default().entry(xbox_hex).or_insert(0) += 1;
                matched += 1;
            }
            None => {
                tsv.push_str(&format!("{path}\t{di}\t0x{th:08X}\t{parent}\t{ps3_stride_s}\t{ps3_hex}\t\t<no-xbox>\n"));
                only_ps3 += 1;
            }
        }
    }
    for ((path, di, th), _) in &x {
        if !p.contains_key(&(path.clone(), *di, *th)) { only_xbox += 1; }
    }

    eprintln!("[pair] matched (same block/desc/type_hash): {matched}");
    eprintln!("[pair] only in PS3: {only_ps3}");
    eprintln!("[pair] only in Xbox: {only_xbox}");

    // Summary: for each (type_hash, ps3_hex, stride), how many distinct xbox_hex and the counts.
    let mut summary = String::new();
    summary.push_str("# PS3 compact -> Xbox 12B+Nx12B decl mapping (empirical)\n\n");
    summary.push_str(&format!("## Pairs matched: {matched}  only_ps3: {only_ps3}  only_xbox: {only_xbox}\n\n"));
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by_key(|((_, _, _), xmap)| std::cmp::Reverse(xmap.values().sum::<usize>()));
    for ((th, ps3_hex, stride), xmap) in &rows {
        let total: usize = xmap.values().sum();
        let stride_s = stride.map(|s| format!("{s}")).unwrap_or_else(|| "?".into());
        summary.push_str(&format!(
            "\n## type_hash=0x{th:08X}  ps3=`{ps3_hex}`  stride={stride_s}  total={total}  distinct_xbox={}\n",
            xmap.len()
        ));
        let mut xs: Vec<_> = xmap.iter().collect();
        xs.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        for (xh, c) in xs {
            summary.push_str(&format!("  count={c}  xbox=`{xh}`\n"));
        }
    }

    eprintln!("{summary}");
    if let Some(dir) = &out_dir {
        fs::write(dir.join("ps3_xbox_decl_pair.tsv"), &tsv).ok();
        fs::write(dir.join("ps3_xbox_decl_mapping.md"), &summary).ok();
    }
}
