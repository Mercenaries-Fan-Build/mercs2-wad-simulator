//! Compare PS3->PC vs Xbox-DOH->PC `convert_block` output byte-by-byte for
//! every pair of blocks that match by `path` across the two containers.
//!
//! Proof of the PS3 1-to-1 native re-interleave path (see
//! `be_to_le::ps3_native`): for every terrain / lowres block where PS3 ships
//! only 1-to-1 compact-decl STRM groups, the LE output of `convert_block` must
//! be byte-identical regardless of which platform's bytes the walker consumed.
//!
//!   cargo run --release -p mercs2_probe --bin ps3_native_converge -- \
//!       --ps3 <ps3.scff> --xbox <xbox.doh> [--only <path-substring>] [--out <dir>]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mercs2_formats::be_to_le::convert::convert_block;
use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;

fn arg_str(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1).cloned())
}

struct BlockBytes {
    path: String,
    raw: Vec<u8>,
}

fn load_blocks(scff_path: &str, only: Option<&str>) -> Vec<BlockBytes> {
    let p = PathBuf::from(scff_path);
    let (doh, _) = load_stfs_or_doh(&p).expect("load scff/doh");
    let (_, rows) = parse_be_ffcs(&doh).expect("parse FFCS");
    let indx_row = rows.iter().find(|r| r.tag == "INDX").expect("INDX").clone();
    let indx = parse_be_indx(&doh, indx_row.offset as usize, indx_row.meta as usize);
    let pths = rows
        .iter()
        .find(|r| r.tag == "PTHS")
        .map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize))
        .unwrap_or_default();

    let mut out = Vec::new();
    for (i, e) in indx.iter().enumerate() {
        let path = pths.get(i).cloned().unwrap_or_else(|| format!("block_{i:05}"));
        if let Some(sub) = only {
            if !path.contains(sub) {
                continue;
            }
        }
        let off = e.file_offset();
        let size = e.page_count as usize * PAGE_SIZE;
        if off + 4 > doh.len() {
            continue;
        }
        let slice = &doh[off..(off + size).min(doh.len())];
        let raw: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
            match decompress_be_sges(slice, 0, slice.len()) {
                Ok(d) => d,
                Err(_) => continue,
            }
        } else {
            // Trim trailing zero padding to the 4-byte boundary.
            let mut d = slice.to_vec();
            while d.len() > 4 && *d.last().unwrap() == 0 {
                d.pop();
            }
            while d.len() % 4 != 0 {
                d.push(0);
            }
            d
        };
        out.push(BlockBytes { path, raw });
    }
    out
}

fn hex_cap(bytes: &[u8], cap: usize) -> String {
    let n = bytes.len().min(cap);
    let mut s = String::new();
    for b in &bytes[..n] {
        s.push_str(&format!("{:02x}", b));
    }
    if bytes.len() > cap {
        s.push_str("...");
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ps3 = arg_str(&args, "--ps3").expect("--ps3 <scff>");
    let xbox = arg_str(&args, "--xbox").expect("--xbox <doh>");
    let only = arg_str(&args, "--only");
    let out_dir: Option<PathBuf> = arg_str(&args, "--out").map(PathBuf::from);
    if let Some(d) = &out_dir {
        fs::create_dir_all(d).ok();
    }

    eprintln!("[ps3_native_converge] loading PS3: {ps3}");
    let ps3_blocks = load_blocks(&ps3, only.as_deref());
    eprintln!("[ps3_native_converge] PS3 blocks loaded: {}", ps3_blocks.len());
    eprintln!("[ps3_native_converge] loading Xbox: {xbox}");
    let xbox_blocks = load_blocks(&xbox, only.as_deref());
    eprintln!("[ps3_native_converge] Xbox blocks loaded: {}", xbox_blocks.len());

    // Normalize paths to the shared "blocks\<dlc>\<name>" form (both platforms
    // ship identical paths for DLC01).
    let x_map: BTreeMap<String, &BlockBytes> =
        xbox_blocks.iter().map(|b| (b.path.clone(), b)).collect();

    let mut terrain_matched = 0usize;
    let mut terrain_byte_eq = 0usize;
    let mut terrain_bytes_diff = Vec::<(String, usize, usize, Vec<(usize, u8, u8)>)>::new();
    let mut lowres_matched = 0usize;
    let mut lowres_byte_eq = 0usize;
    let mut non_terrain_matched = 0usize;
    let mut non_terrain_byte_eq = 0usize;
    let mut ps3_fail = 0usize;
    let mut xbox_fail = 0usize;

    for p3 in &ps3_blocks {
        let Some(xb) = x_map.get(&p3.path) else {
            continue;
        };

        // Decide class by Xbox entry-table type_hash of the first entry.
        let th_xbox = if xb.raw.len() >= 20 {
            u32::from_be_bytes([xb.raw[8], xb.raw[9], xb.raw[10], xb.raw[11]])
        } else {
            0
        };

        let p3_out = match convert_block(&p3.raw, false, None) {
            Ok(b) => b,
            Err(_) => {
                ps3_fail += 1;
                continue;
            }
        };
        let xb_out = match convert_block(&xb.raw, false, None) {
            Ok(b) => b,
            Err(_) => {
                xbox_fail += 1;
                continue;
            }
        };

        let is_terrain = th_xbox == 0x7C569307;
        let is_lowres = th_xbox == 0x1602815C;

        if is_terrain {
            terrain_matched += 1;
        } else if is_lowres {
            lowres_matched += 1;
        } else {
            non_terrain_matched += 1;
        }

        if p3_out == xb_out {
            if is_terrain {
                terrain_byte_eq += 1;
            } else if is_lowres {
                lowres_byte_eq += 1;
            } else {
                non_terrain_byte_eq += 1;
            }
        } else if is_terrain || is_lowres {
            // Record a diff summary (up to 16 diffs).
            let n = p3_out.len().max(xb_out.len());
            let mut diffs = Vec::new();
            for i in 0..n.min(p3_out.len()).min(xb_out.len()) {
                if p3_out[i] != xb_out[i] {
                    diffs.push((i, p3_out[i], xb_out[i]));
                    if diffs.len() >= 16 {
                        break;
                    }
                }
            }
            terrain_bytes_diff.push((p3.path.clone(), p3_out.len(), xb_out.len(), diffs));
        }
    }

    eprintln!("--- summary -----------------------------------------------------");
    eprintln!(
        "terrain  (0x7C569307): matched {}  byte-eq {}  diff {}",
        terrain_matched, terrain_byte_eq, terrain_matched - terrain_byte_eq
    );
    eprintln!(
        "lowres   (0x1602815C): matched {}  byte-eq {}  diff {}",
        lowres_matched, lowres_byte_eq, lowres_matched - lowres_byte_eq
    );
    eprintln!(
        "other              : matched {}  byte-eq {}  diff {}",
        non_terrain_matched,
        non_terrain_byte_eq,
        non_terrain_matched - non_terrain_byte_eq
    );
    eprintln!("ps3 fail: {}  xbox fail: {}", ps3_fail, xbox_fail);

    for (p, p3_len, xb_len, diffs) in terrain_bytes_diff.iter().take(5) {
        eprintln!("DIFF {p} ps3_out={p3_len} xbox_out={xb_len}");
        for (off, a, b) in diffs {
            eprintln!("  off {off}: ps3={:02x} xbox={:02x}", a, b);
        }
    }

    if let Some(dir) = &out_dir {
        let mut tsv = String::from("path\tp3_len\txb_len\tdiff_count\n");
        for (p, p3l, xbl, d) in &terrain_bytes_diff {
            tsv.push_str(&format!("{p}\t{p3l}\t{xbl}\t{}\n", d.len()));
        }
        fs::write(dir.join("ps3_native_converge.tsv"), &tsv).ok();
    }
    let _ = hex_cap; // retained for future diff formatting
}
