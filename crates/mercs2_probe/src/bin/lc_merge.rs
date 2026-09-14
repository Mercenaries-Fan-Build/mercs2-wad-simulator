//! `lc_merge` — merge a Quartermaster overlay's blocks into the live `vz-patch.wad` (last-wins),
//! producing a drop-in `vz-patch.wad`. The base `vz.wad` and the live overlay input are read-only.
//!
//!   lc_merge --overlay <qm.wad> --into <live-vz-patch.back> --out <merged.wad>

use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad};
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let overlay = arg(&a, "--overlay").expect("--overlay <qm.wad>");
    let into = arg(&a, "--into").expect("--into <live overlay wad>");
    let out = arg(&a, "--out").expect("--out <merged.wad>");

    let ov_bytes = std::fs::read(Path::new(&overlay)).expect("read overlay");
    let live_bytes = std::fs::read(Path::new(&into)).expect("read live overlay");

    let ov = read_patch_wad(&ov_bytes).expect("parse qm overlay");
    println!("qm overlay blocks: {}", ov.blocks.len());
    for b in &ov.blocks {
        println!("  + {}", b.path_string);
    }
    let live = read_patch_wad(&live_bytes).expect("parse live overlay");
    println!("live overlay blocks: {}", live.blocks.len());

    // replace=true → last-wins on a path collision; our new blocks otherwise append.
    let merged = merge_patch_wads(&live_bytes, ov.blocks, true).expect("merge");
    std::fs::write(Path::new(&out), &merged).expect("write merged");
    let re = read_patch_wad(&merged).expect("re-parse merged");
    println!("MERGED {} bytes, {} blocks -> {out}", merged.len(), re.blocks.len());
}
