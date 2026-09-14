//! veh_template_probe — decode what a retail BOAT VEHICLE TEMPLATE is on disk.
//!
//! `vehicles_boat_Piranha` (the drivable-boat definition) lives as an entry in the resident block
//! 3185 (`resident_P000_Q3.block`). This probe decompresses that block, lists its resident entries
//! (name_hash / type_hash / size), resolves the ones whose hash matches a candidate boat/vehicle
//! name, and dumps the structure of the boat-template body — is it a UCFX chunk tree, a COMP set, or
//! an opaque serialized blob? That tells us exactly what to author for a from-scratch boat vehicle.
//!
//! Usage: veh_template_probe <vz.wad> [block_index=3185] [name1 name2 ...]

use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use std::collections::HashMap;

fn ascii(b: &[u8]) -> String {
    b.iter()
        .map(|&c| if (32..127).contains(&c) { c as char } else { '.' })
        .collect()
}

/// Resident block = `[u32 count][count × (name_hash,type_hash,field_c,size)][bodies...]`.
struct ResidentBlock {
    entries: Vec<(u32, u32, u32, Vec<u8>)>, // name, type, field_c, body
}
impl ResidentBlock {
    fn parse(blk: &[u8]) -> ResidentBlock {
        let count = read_u32_le(blk, 0) as usize;
        let mut pos = 4 + count * 16;
        let mut entries = Vec::with_capacity(count);
        for ei in 0..count {
            let base = 4 + ei * 16;
            let nh = read_u32_le(blk, base);
            let th = read_u32_le(blk, base + 4);
            let fc = read_u32_le(blk, base + 8);
            let sz = read_u32_le(blk, base + 12) as usize;
            let end = (pos + sz).min(blk.len());
            entries.push((nh, th, fc, blk[pos..end].to_vec()));
            pos += sz;
        }
        ResidentBlock { entries }
    }
}

/// If the body is a UCFX container, list its descriptor tags. Otherwise report it opaque.
fn describe_body(body: &[u8]) -> String {
    if body.len() >= 20 && &body[0..4] == b"UCFX" {
        let ndesc = read_u32_le(body, 16) as usize;
        let mut tags: Vec<String> = Vec::new();
        for i in 0..ndesc.min(64) {
            let ro = 20 + i * 20;
            if ro + 4 > body.len() {
                break;
            }
            tags.push(ascii(&body[ro..ro + 4]));
        }
        format!("UCFX ndesc={ndesc} tags=[{}]", tags.join(","))
    } else {
        // opaque blob — show the first 32 bytes as hex + ascii
        let n = body.len().min(32);
        let hex: String = body[..n].iter().map(|b| format!("{b:02X} ")).collect();
        format!("OPAQUE first{n}=[{}] ascii=\"{}\"", hex.trim_end(), ascii(&body[..n]))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wad = args.first().cloned().unwrap_or_else(|| {
        eprintln!("usage: veh_template_probe <vz.wad> [block=3185] [name ...]");
        std::process::exit(1);
    });
    let block: u16 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3185);
    let names: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        vec![
            "vehicles_boat_Piranha".into(),
            "vehicles_boat_lcur".into(),
            "al_veh_boat_lcur".into(),
            "vehicles_boat_cutter".into(),
            "vehicles_boat_dinghy".into(),
        ]
    };
    let want: HashMap<u32, String> = names.iter().map(|n| (pandemic_hash_m2(n), n.clone())).collect();

    let mut f = std::fs::File::open(&wad).unwrap_or_else(|e| panic!("open {wad}: {e}"));
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let dec = decompress_block(&mut f, &ar.indx, block).expect("decompress block");
    let rb = ResidentBlock::parse(&dec);
    println!("block {block}: {} bytes, {} resident entries", dec.len(), rb.entries.len());

    // Type-hash histogram (what classes of resident entry exist).
    let mut type_hist: HashMap<u32, usize> = HashMap::new();
    for (_, th, _, _) in &rb.entries {
        *type_hist.entry(*th).or_default() += 1;
    }
    let mut hist: Vec<_> = type_hist.iter().collect();
    hist.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    println!("\n=== resident type_hash histogram (top 20) ===");
    for (th, c) in hist.iter().take(20) {
        println!("  type=0x{th:08X}  x{c}");
    }

    println!("\n=== matched vehicle/boat template entries ===");
    let mut found = 0;
    for (nh, th, fc, body) in &rb.entries {
        if let Some(name) = want.get(nh) {
            found += 1;
            println!(
                "\n[{name}] name=0x{nh:08X} type=0x{th:08X} field_c={fc} body={} B",
                body.len()
            );
            println!("  {}", describe_body(body));
        }
    }
    if found == 0 {
        println!("  (none of {:?} matched by hash in block {block})", names);
        // Fall back: show any entry whose body ascii mentions 'boat' or 'veh'
        println!("\n=== entries whose body text mentions boat/veh (scan) ===");
        for (nh, th, _, body) in &rb.entries {
            let txt = ascii(&body[..body.len().min(256)]);
            if txt.to_lowercase().contains("boat") || txt.to_lowercase().contains("veh_") {
                println!("  name=0x{nh:08X} type=0x{th:08X} {} B :: {}", body.len(), describe_body(body));
            }
        }
    }
}
