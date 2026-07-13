//! lod_neuter — empty every drawing group in a model's FINER LOD RUNG.
//!
//! A vehicle is a LOD-BLOCK CHAIN (`_P000_Q3` resident -> `_P001_` -> `_P002_`), and the rungs
//! REFINE each other (finest wins per node+tier). Conform a novel model into the resident rung and
//! the finer rungs still hold the DONOR's original geometry — so the model looks right from a
//! distance and shreds itself the moment the camera is close enough to stream a finer rung in
//! (the donor's hull drawn straight through ours: cracks, holes, floating shards).
//!
//! This strips the geometry out of a finer rung so the resident rung is the only thing drawn at any
//! distance. Everything we don't touch is preserved byte-for-byte and the CSUM is recomputed.
//!
//! Usage:  lod_neuter <rung.ucfx> <out.ucfx> --name-hash 0xH
//!
//! See docs/modernization/vehicle_model_spec.md §1 (the LOD chain).

use mercs2_formats::model_inject::neutralise_lod_rung;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<&str> = Vec::new();
    let mut name_hash = 0u32;
    let mut it = a.iter();
    while let Some(s) = it.next() {
        match s.as_str() {
            "--name-hash" => {
                name_hash = it
                    .next()
                    .and_then(|v| {
                        u32::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
                    })
                    .unwrap_or(0)
            }
            other => pos.push(other),
        }
    }
    if pos.len() != 2 || name_hash == 0 {
        eprintln!("usage: lod_neuter <rung.ucfx> <out.ucfx> --name-hash 0xH");
        return 2;
    }
    let ucfx = match std::fs::read(pos[0]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read {}: {e}", pos[0]);
            return 1;
        }
    };
    match neutralise_lod_rung(&ucfx, name_hash) {
        Ok((out, n)) => {
            if let Err(e) = std::fs::write(pos[1], &out) {
                eprintln!("write {}: {e}", pos[1]);
                return 1;
            }
            println!("emptied {n} drawing group(s) -> {} ({} bytes)", pos[1], out.len());
            0
        }
        Err(e) => {
            eprintln!("lod_neuter: {e}");
            1
        }
    }
}
