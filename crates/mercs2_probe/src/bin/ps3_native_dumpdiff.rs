//! Dump both PS3->PC and Xbox->PC converted bytes to files, side-by-side, for
//! one block (matched by `--only` substring). Lets you diff them structurally.
//!
//!   cargo run --release -p mercs2_probe --bin ps3_native_dumpdiff -- \
//!       --ps3 <ps3.scff> --xbox <xbox.doh> --only <path-substring> --out <dir>

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

fn load_block(scff_path: &str, substr: &str) -> Option<(String, Vec<u8>)> {
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

    for (i, e) in indx.iter().enumerate() {
        let path = pths.get(i).cloned().unwrap_or_else(|| format!("block_{i:05}"));
        if !path.contains(substr) {
            continue;
        }
        let off = e.file_offset();
        let size = e.page_count as usize * PAGE_SIZE;
        if off + 4 > doh.len() {
            continue;
        }
        let slice = &doh[off..(off + size).min(doh.len())];
        let raw: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
            decompress_be_sges(slice, 0, slice.len()).ok()?
        } else {
            let mut d = slice.to_vec();
            while d.len() > 4 && *d.last().unwrap() == 0 {
                d.pop();
            }
            while d.len() % 4 != 0 {
                d.push(0);
            }
            d
        };
        return Some((path, raw));
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ps3 = arg_str(&args, "--ps3").expect("--ps3");
    let xbox = arg_str(&args, "--xbox").expect("--xbox");
    let only = arg_str(&args, "--only").expect("--only <substr>");
    let out_dir: PathBuf = arg_str(&args, "--out").map(PathBuf::from).expect("--out");
    fs::create_dir_all(&out_dir).ok();

    let (p3_path, p3_raw) = load_block(&ps3, &only).expect("PS3 block not found");
    let (xb_path, xb_raw) = load_block(&xbox, &only).expect("Xbox block not found");
    eprintln!("PS3  : {p3_path} raw={} bytes", p3_raw.len());
    eprintln!("Xbox : {xb_path} raw={} bytes", xb_raw.len());

    fs::write(out_dir.join("ps3_be.bin"), &p3_raw).ok();
    fs::write(out_dir.join("xbox_be.bin"), &xb_raw).ok();

    let p3_out = convert_block(&p3_raw, false, None).expect("ps3 convert");
    let xb_out = convert_block(&xb_raw, false, None).expect("xbox convert");

    fs::write(out_dir.join("ps3_le.bin"), &p3_out).ok();
    fs::write(out_dir.join("xbox_le.bin"), &xb_out).ok();

    eprintln!("PS3  LE out: {} bytes", p3_out.len());
    eprintln!("Xbox LE out: {} bytes", xb_out.len());
    eprintln!("byte-equal: {}", p3_out == xb_out);
}
