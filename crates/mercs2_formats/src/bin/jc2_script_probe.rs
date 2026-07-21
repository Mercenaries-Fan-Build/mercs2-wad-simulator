//! jc2_script_probe — dump resident-script chunk structure (INFO/DEPS/BINN) from block 3185, so we
//! can author a new mrxjc2sportscar script chunk + wire mrxshop's DEPS.
//! Usage: jc2_script_probe [vz.wad] [name1 name2 ...]

use std::fs::File;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
    });
    let names: Vec<String> = std::env::args().skip(2).collect();
    let names = if names.is_empty() {
        vec!["mrxshop".into(), "mrxsupportdata".into(), "mrxrewarddata".into()]
    } else { names };

    // If arg1 is a .bin, treat it as a raw decompressed block; else load it as a vz.wad.
    let blk = if path.ends_with(".bin") {
        std::fs::read(&path).unwrap()
    } else {
        let mut f = File::open(&path).unwrap();
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).unwrap();
        // Print the base ASET row (block_index + sub_entry) for each requested script name.
        println!("=== base ASET rows for requested scripts ===");
        for n in &names {
            let h = pandemic_hash_m2(n);
            for a in arch.aset.iter().filter(|a| a.asset_hash == h) {
                println!("  {n} (0x{h:08X}): block {} sub 0x{:04X} type 0x{:08X} sec 0x{:08X}",
                    a.block_index(), a.sub_entry(), a.type_id, a.secondary_ref);
            }
        }
        let mut blk = None;
        for bi in 0..arch.indx.len() {
            let Ok(d) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
            if d.len() < 4 { continue; }
            let c = read_u32_le(&d, 0) as usize;
            if c == 0 || c > 100_000 { continue; }
            if (0..c).any(|e| 4+e*16+16 <= d.len() && read_u32_le(&d, 4+e*16+4) == 0x5647_C35D) { blk = Some(d); break; }
        }
        blk.unwrap()
    };
    let count = read_u32_le(&blk, 0) as usize;

    // Summary mode: scan every script-type entry's INFO metadata byte (byte after name+terminator).
    if names.first().map(|s| s == "--summary").unwrap_or(false) {
        let mut pos = 4 + count * 16;
        let mut hist: std::collections::BTreeMap<u8, usize> = Default::default();
        let mut zero_examples: Vec<String> = Vec::new();
        let mut n_scripts = 0;
        for ei in 0..count {
            let base = 4 + ei * 16;
            let th = read_u32_le(&blk, base + 4);
            let sz = read_u32_le(&blk, base + 12) as usize;
            let c = &blk[pos..pos + sz];
            pos += sz;
            if th != 0x4249_8680 { continue; } // script type
            if c.len() < 20 || &c[0..4] != b"UCFX" { continue; }
            // INFO body at data_area_off (first descriptor). INFO = [05][u16 len][name][00][meta][00 00]
            let data_off = read_u32_le(c, 4) as usize;
            let info = &c[data_off..];
            if info.len() < 4 || info[0] != 0x05 { continue; }
            let nlen = u16::from_le_bytes([info[1], info[2]]) as usize;
            let meta_off = 3 + nlen + 1; // marker + u16 len + name + terminator
            if meta_off >= info.len() { continue; }
            let meta = info[meta_off];
            let name: String = info[3..3 + nlen].iter().map(|&b| b as char).collect();
            *hist.entry(meta).or_default() += 1;
            n_scripts += 1;
            if meta == 0 && zero_examples.len() < 10 { zero_examples.push(name); }
        }
        println!("scanned {n_scripts} script entries");
        println!("metadata-byte histogram: {hist:?}");
        println!("scripts with metadata==0 ({}): {:?}", hist.get(&0).copied().unwrap_or(0), zero_examples);
        return;
    }

    let want: Vec<(String, u32)> = names.iter().map(|n| (n.clone(), pandemic_hash_m2(n))).collect();

    let mut pos = 4 + count * 16;
    for ei in 0..count {
        let base = 4 + ei * 16;
        let nh = read_u32_le(&blk, base);
        let sz = read_u32_le(&blk, base + 12) as usize;
        let c = &blk[pos..pos + sz];
        pos += sz;
        let Some((name, _)) = want.iter().find(|(_, h)| *h == nh) else { continue };
        println!("\n===== {name} (name_hash 0x{nh:08X}) container {sz} bytes =====");
        if c.len() < 20 || &c[0..4] != b"UCFX" { println!("  not UCFX"); continue; }
        let data_off = read_u32_le(c, 4) as usize;
        let ndesc = read_u32_le(c, 16) as usize;
        println!("  data_area_off={data_off} ndesc={ndesc}");
        let ds = if data_off > 0 { data_off } else { 8 };
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            if ro + 20 > c.len() { break; }
            let tag = std::str::from_utf8(&c[ro..ro+4]).unwrap_or("????");
            let u0 = read_u32_le(c, ro + 4);
            let bsz = read_u32_le(c, ro + 8) as usize;
            let w3 = read_u32_le(c, ro + 12);
            let w4 = read_u32_le(c, ro + 16);
            print!("  desc[{i}] {tag:?} u0={u0} sz={bsz} w3={w3} w4={w4}");
            if u0 != 0xFFFF_FFFF {
                let s = ds + u0 as usize;
                let e = (s + bsz).min(c.len());
                let body = &c[s..e];
                let show = &body[..48.min(body.len())];
                let asc: String = show.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                print!("  body[{s}..{e}]: {asc}");
                if tag == "INFO" {
                    print!("  INFO hex: {}", body.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
                }
                if tag == "DEPS" {
                    // [u8 count][count u32 hashes]
                    if !body.is_empty() {
                        let dn = body[0] as usize;
                        print!("  DEPS count={dn}: ");
                        for k in 0..dn { if 1+k*4+4 <= body.len() { print!("0x{:08X} ", read_u32_le(body, 1+k*4)); } }
                    }
                }
                if tag == "BINN" {
                    let luaq = body.windows(4).position(|w| w == b"\x1bLua");
                    print!("  [BINN body {} bytes, LuaQ@{:?}]", body.len(), luaq);
                    if let Some(o) = luaq {
                        let hdr = &body[o..(o+18).min(body.len())];
                        print!("  LuaQ hdr: {}", hdr.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
                    }
                }
            }
            println!();
        }
        // trailer + CSUM verify
        if c.len() >= 8 && &c[c.len()-8..c.len()-4] == b"CSUM" {
            let stored = read_u32_le(c, c.len()-4);
            let computed = mercs2_formats::crc32::crc32_mercs2(&c[..c.len()-8]);
            println!("  CSUM: stored 0x{stored:08X} computed 0x{computed:08X} {}",
                if stored == computed { "✓" } else { "✗ MISMATCH" });
        }
    }
}
