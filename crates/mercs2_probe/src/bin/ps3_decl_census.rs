//! Census of every `decl` body in a BE SCFF: count distinct byte sequences,
//! sort by frequency. Decides whether PS3 decl bodies are a small fixed set
//! (= opaque identifier/index) or truly variable (= a packed decl array).
//!
//!   cargo run --release -p mercs2_probe --bin ps3_decl_census -- \
//!       --scff <path> [--out <dir>]

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
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scff = arg_str(&args, "--scff").unwrap_or_else(|| {
        eprintln!("usage: ps3_decl_census --scff <path> [--out <dir>]");
        std::process::exit(2);
    });
    let out_dir: Option<PathBuf> = arg_str(&args, "--out").map(PathBuf::from);
    if let Some(d) = &out_dir { fs::create_dir_all(d).ok(); }

    let scff_path = PathBuf::from(&scff);
    let (doh, _) = load_stfs_or_doh(&scff_path).expect("load scff");
    let (_, rows) = parse_be_ffcs(&doh).expect("parse FFCS");
    let indx_row = rows.iter().find(|r| r.tag == "INDX").expect("INDX").clone();
    let indx = parse_be_indx(&doh, indx_row.offset as usize, indx_row.meta as usize);
    let pths = rows.iter().find(|r| r.tag == "PTHS").map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize)).unwrap_or_default();

    // key = (type_hash, body size, body hex) → list of (block_idx, path, descriptor-row-idx, parent, row_u3, row_u4)
    #[derive(Default)]
    struct Site {
        count: usize,
        samples: Vec<(usize, String, usize, String, u32, u32)>,
    }
    let mut census: BTreeMap<(u32, usize, String), Site> = BTreeMap::new();
    let mut total_decls = 0usize;

    for (blk_idx, e) in indx.iter().enumerate() {
        let path = pths.get(blk_idx).cloned().unwrap_or_default();
        let block_offset = e.file_offset();
        let block_size = e.page_count as usize * PAGE_SIZE;
        if block_offset + 4 > doh.len() { continue; }
        let slice = &doh[block_offset..(block_offset + block_size).min(doh.len())];
        let decompressed: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
            match decompress_be_sges(slice, 0, slice.len()) {
                Ok(d) => d,
                Err(_) => continue,
            }
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
        } else { continue; };

        // Walk the block entry table.
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
                let row_u3 = read_u32_be(container, row_start + 12);
                let row_u4 = read_u32_be(container, row_start + 16);
                if row_u0 == 0xFFFFFFFF { continue; }
                let abs = data_start + row_u0 as usize;
                let end = abs + body_size;
                if end > container.len() { continue; }
                let body = &container[abs..end];
                total_decls += 1;
                // Find parent tag.
                let mut parent_tag = String::from("?");
                for parent_di in (0..di).rev() {
                    let prs = 20 + parent_di * 20;
                    let mut pt = [0u8; 4];
                    pt.copy_from_slice(&container[prs..prs + 4]);
                    if is_be { pt.reverse(); }
                    let p_u0 = read_u32_be(container, prs + 4);
                    if p_u0 == 0xFFFFFFFF {
                        parent_tag = String::from_utf8_lossy(&pt).into_owned();
                        break;
                    }
                }

                let key = (type_hash, body_size, hex_of(body));
                let site = census.entry(key).or_default();
                site.count += 1;
                if site.samples.len() < 2 {
                    site.samples.push((blk_idx, path.clone(), di, parent_tag, row_u3, row_u4));
                }
            }
        }
    }

    let mut by_count: Vec<_> = census.iter().collect();
    by_count.sort_by_key(|((_,_,_), s)| std::cmp::Reverse(s.count));

    let mut out = String::new();
    out.push_str(&format!("scff: {scff}\ntotal decl descriptors: {total_decls}\ndistinct (type_hash, size, bytes) tuples: {}\n\n", census.len()));
    for ((type_hash, size, hex), site) in &by_count {
        out.push_str(&format!("count={:5}  type_hash=0x{type_hash:08X}  size={size}  bytes={hex}\n", site.count));
        for (bi, path, di, parent, u3, u4) in &site.samples {
            out.push_str(&format!("   block[{bi}] desc[{di}] parent={parent} u3=0x{u3:08X} u4=0x{u4:08X} {path}\n"));
        }
    }
    eprintln!("{out}");
    if let Some(dir) = &out_dir { fs::write(dir.join("decl_census.txt"), &out).ok(); }
}
