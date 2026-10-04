//! Dump every `decl` descriptor body of a (BE) UCFX block body, for inspecting
//! PS3-specific vertex-declaration layouts whose byte-size the Xbox walker
//! (`apply_decl_translate`) rejects. Pair-print the raw BE bytes, the first few
//! u32s as hex, and the enclosing descriptor row so the shape is immediately
//! obvious.
//!
//!   cargo run --release -p mercs2_probe --bin ps3_decl_dump -- \
//!       --block <fixture.bin> [--out <scratch_dir>] [--limit N]

use std::fs;
use std::path::PathBuf;

use mercs2_formats::ffcs::read_u32_be;

fn arg_str(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1).cloned())
}

fn arg_usize(args: &[String], key: &str) -> Option<usize> {
    arg_str(args, key).and_then(|s| s.parse().ok())
}

fn hex_dump(bytes: &[u8], prefix: &str, max: usize) -> String {
    let n = bytes.len().min(max);
    let mut s = String::new();
    for chunk in bytes[..n].chunks(16) {
        s.push_str(prefix);
        for b in chunk {
            s.push_str(&format!("{:02x} ", b));
        }
        s.push_str("  ");
        for b in chunk {
            s.push(if (0x20..=0x7E).contains(b) { *b as char } else { '.' });
        }
        s.push('\n');
    }
    if n < bytes.len() {
        s.push_str(&format!("{prefix}... ({} more bytes)\n", bytes.len() - n));
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block_path = arg_str(&args, "--block").unwrap_or_else(|| {
        eprintln!("usage: ps3_decl_dump --block <fixture.bin> [--out <dir>] [--limit N]");
        std::process::exit(2);
    });
    let out_dir: Option<PathBuf> = arg_str(&args, "--out").map(PathBuf::from);
    let limit = arg_usize(&args, "--limit").unwrap_or(5);

    if let Some(d) = &out_dir {
        fs::create_dir_all(d).expect("mkdir --out");
    }

    let data = fs::read(&block_path).expect("read block");
    eprintln!("[decl-dump] block={block_path} size={}", data.len());

    // Parse block entry table (same shape as convert_block).
    if data.len() < 4 {
        eprintln!("block too small");
        return;
    }
    let entry_count = read_u32_be(&data, 0) as usize;
    let header_size = 4 + entry_count * 16;
    if header_size > data.len() {
        eprintln!("entry table out of range");
        return;
    }
    eprintln!("entries: {entry_count}");

    let mut offset = header_size;
    let mut decl_count = 0usize;
    for ei in 0..entry_count {
        let eoff = 4 + ei * 16;
        let name_hash = read_u32_be(&data, eoff);
        let type_hash = read_u32_be(&data, eoff + 4);
        let _field_c = read_u32_be(&data, eoff + 8);
        let chunk_size = read_u32_be(&data, eoff + 12) as usize;
        let container_end = offset + chunk_size;
        if container_end > data.len() {
            eprintln!("entry {ei}: container exceeds block");
            break;
        }
        let mut container_end_eff = container_end;
        // Strip trailing CSUM.
        if container_end_eff - offset >= 8 {
            let tail = &data[container_end_eff - 8..container_end_eff - 4];
            if tail == b"CSUM" || tail == b"MUSC" {
                container_end_eff -= 8;
            }
        }
        let container = &data[offset..container_end_eff];
        offset = container_end;
        if container.len() < 20 {
            continue;
        }
        let magic = &container[0..4];
        let is_be = magic == b"XFCU";
        if !is_be && magic != b"UCFX" {
            continue;
        }
        let data_area_off = read_u32_be(container, 4) as usize;
        let n_desc = read_u32_be(container, 16) as usize;
        if n_desc > 10000 {
            continue;
        }
        let desc_table_end = 20 + n_desc * 20;
        if desc_table_end > container.len() {
            continue;
        }
        let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
        for di in 0..n_desc {
            let row_start = 20 + di * 20;
            let mut tag_bytes = [0u8; 4];
            tag_bytes.copy_from_slice(&container[row_start..row_start + 4]);
            if is_be {
                tag_bytes.reverse();
            }
            if &tag_bytes != b"decl" && &tag_bytes != b"DECL" {
                continue;
            }
            let row_u0 = read_u32_be(container, row_start + 4);
            let body_size = read_u32_be(container, row_start + 8);
            let row_u3 = read_u32_be(container, row_start + 12);
            let row_u4 = read_u32_be(container, row_start + 16);
            if row_u0 == 0xFFFFFFFF {
                continue;
            }
            let abs = data_start + row_u0 as usize;
            let end = abs + body_size as usize;
            if end > container.len() {
                continue;
            }
            let body = &container[abs..end];
            decl_count += 1;

            // Scan the enclosing group (STRM/GEOM sentinel immediately before) for context.
            let mut parent_tag = String::from("?");
            for parent_di in (0..di).rev() {
                let prs = 20 + parent_di * 20;
                let mut pt = [0u8; 4];
                pt.copy_from_slice(&container[prs..prs + 4]);
                if is_be {
                    pt.reverse();
                }
                let p_u0 = read_u32_be(container, prs + 4);
                if p_u0 == 0xFFFFFFFF {
                    parent_tag = String::from_utf8_lossy(&pt).into_owned();
                    break;
                }
            }

            eprintln!(
                "\n== decl #{decl_count} entry[{ei}] type_hash=0x{type_hash:08X} name=0x{name_hash:08X} desc[{di}] parent={parent_tag} \
                 row_u0=0x{row_u0:08X} body_size={body_size} row_u3=0x{row_u3:08X} row_u4=0x{row_u4:08X}"
            );
            eprintln!("abs=0x{abs:X}..0x{:X} within container 0x{:X} bytes", end, container.len());
            eprint!("{}", hex_dump(body, "  ", 128));

            if let Some(dir) = &out_dir {
                let fname = format!(
                    "decl_{:03}_entry{ei}_desc{di}_parent{parent_tag}_sz{body_size}.bin",
                    decl_count
                );
                fs::write(dir.join(&fname), body).ok();
            }
            if decl_count >= limit {
                return;
            }
        }
    }
    eprintln!("\n[decl-dump] total decls dumped: {decl_count}");
}
