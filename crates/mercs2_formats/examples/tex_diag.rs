//! Throwaway diagnostic: dump ASET rows + INFO + block layout for a texture hash.
use mercs2_formats::ffcs::{load_ffcs_archive, read_u16_le, read_u32_le};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::parse_block_entry_table;
use mercs2_formats::types::{TYPE_HASH_TEXTURE, TYPE_ID_TEXTURE};
use std::fs::File;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wad = &args[1];
    let mut file = File::open(wad).expect("open wad");
    let size = file.metadata().unwrap().len();
    let archive = load_ffcs_archive(&mut file, size).expect("load");

    let hashes: Vec<u32> = args[2..]
        .iter()
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect();

    for &h in &hashes {
        println!("\n########## texture 0x{h:08X} ##########");
        // All ASET rows for this hash.
        for e in &archive.aset {
            if e.asset_hash == h {
                println!(
                    "  ASET: type_id={} sec_ref=0x{:08X} block={} sub_entry=0x{:04X} primary={}",
                    e.type_id,
                    e.secondary_ref,
                    e.block_index(),
                    e.sub_entry(),
                    e.is_primary()
                );
            }
        }
        // Candidate blocks (mirror extract_container).
        let mut blocks: Vec<u16> = Vec::new();
        for e in &archive.aset {
            if e.asset_hash == h && e.type_id == TYPE_ID_TEXTURE && e.is_primary() {
                blocks.push(e.block_index());
            }
        }
        for e in &archive.aset {
            if e.asset_hash == h && e.type_id == TYPE_ID_TEXTURE && !e.is_primary() {
                let b = e.block_index();
                if !blocks.contains(&b) { blocks.push(b); }
            }
        }
        for block in blocks {
            let dec = match decompress_block(&mut file, &archive.indx, block) {
                Ok(d) => d, Err(err) => { println!("  block {block}: decompress err {err}"); continue }
            };
            let indx = &archive.indx[block as usize];
            println!(
                "  block {block}: dec_len={} (decomp_pages={} => {} bytes) comp_pages={}",
                dec.len(),
                indx.decompressed_page_count(),
                indx.decompressed_page_count() as usize * 0x8000,
                indx.compressed_page_count(),
            );
            let (count, entries) = parse_block_entry_table(&dec);
            let header_end = 4 + count as usize * 16;
            let mut off = header_end;
            for e in &entries {
                let end = off + e.chunk_size as usize;
                let is_tex = e.type_hash == TYPE_HASH_TEXTURE && e.name_hash == h;
                if is_tex || (e.type_hash == TYPE_HASH_TEXTURE && entries.iter().filter(|x| x.name_hash==h).count()==0) {
                    println!(
                        "    entry name=0x{:08X} type=0x{:08X} field_c=0x{:08X} chunk_size={} [{off}..{end}] {}",
                        e.name_hash, e.type_hash, e.field_c, e.chunk_size, if is_tex {"<== target"} else {""}
                    );
                    // Dump INFO from the container.
                    if end <= dec.len() {
                        dump_info(&dec[off..end]);
                    }
                }
                off = end;
            }
        }
    }
}

fn dump_info(container: &[u8]) {
    if container.len() < 20 || &container[0..4] != b"UCFX" { println!("      (not UCFX)"); return; }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    for i in 0..n_desc {
        let ro = 20 + i * 20;
        if ro + 20 > container.len() { break; }
        let tag = &container[ro..ro+4];
        let u0 = read_u32_le(container, ro+4);
        let sz = read_u32_le(container, ro+8) as usize;
        let f3 = read_u32_le(container, ro+12);
        let f4 = read_u32_le(container, ro+16);
        let tags = std::str::from_utf8(tag).unwrap_or("????");
        if u0 == 0xFFFF_FFFF {
            println!("      row {tags} MARKER sz={sz} f3=0x{f3:08X} f4=0x{f4:08X}");
            continue;
        }
        let start = if data_area_off>0 { data_area_off + u0 as usize } else { 8 + u0 as usize };
        let end = start + sz;
        println!("      row {tags} u0={u0} sz={sz} f3=0x{f3:08X} f4=0x{f4:08X} span[{start}..{end}]");
        if tag == b"INFO" && end <= container.len() {
            let info = &container[start..end];
            print!("        INFO bytes ({}):", info.len());
            for b in info { print!(" {b:02X}"); }
            println!();
            if info.len() >= 18 {
                println!("        -> w={} h={} f4field={} mip@6={} fourcc={:?}",
                    read_u16_le(info,0), read_u16_le(info,2), read_u16_le(info,4),
                    read_u16_le(info,6), std::str::from_utf8(&info[14..18]).unwrap_or("?"));
            }
        }
    }
}
