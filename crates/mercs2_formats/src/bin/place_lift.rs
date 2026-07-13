//! place_lift — raise (or lower) the Y of existing `Transform` records in a
//! decompressed placement block, selected by ENTITY KEY.
//!
//! Why: the PMC exit teleport (`MrxUtil._TeleportHero`) resolves its destination
//! from a named world object (`Pmc_B1`/`Pmc_B2`, keys 0x000C73C2/0x000C73C3) via
//! `Pg.GetGuidByName` -> `Object.GetPosition`, then does
//! `DisablePhysics -> SetPosition -> (wait for streaming) -> EnablePhysics`.
//! On a fast load the exterior terrain collision becomes resident *after* the
//! hero is placed at ground level (Y=-13.1779), sealing the player UNDER the
//! land. Lifting the anchor a few metres means that when collision arrives the
//! hero is above it and simply drops onto the surface.
//!
//! A `Transform` record is a fixed 42 bytes and Y is a plain f32 at offset 8, so
//! this is a pure in-place 4-byte rewrite: no size change, no child-offset
//! recomputation, no entry-table surgery.
//!
//!   [0..4]   u32 entity key
//!   [4..16]  f32 x, y, z
//!   [16..20] u32 pad (always 0)  <- used as a validity check
//!   [20..36] f32 quat x,y,z,w
//!   [36..42] 6-byte tail
//!
//! Usage:
//!   place_lift <in_block.bin> <out_block.bin> --key 0x000C73C2 [--key ...] --dy 3.0
//!   place_lift <in_block.bin> --list --key 0x000C73C2        (read-only report)

const REC: usize = 42;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Is `off` the start of a plausible Transform record for `key`?
/// Requires: key match, zero pad at [16..20], and finite, in-world-range xyz.
fn is_transform(block: &[u8], off: usize, key: u32) -> bool {
    if off + REC > block.len() || rd_u32(block, off) != key || rd_u32(block, off + 16) != 0 {
        return false;
    }
    let (x, y, z) = (rd_f32(block, off + 4), rd_f32(block, off + 8), rd_f32(block, off + 12));
    [x, y, z].iter().all(|v| v.is_finite()) && x.abs() < 1.0e5 && y.abs() < 1.0e4 && z.abs() < 1.0e5
}

/// Every Transform record in `block` whose entity key is `key`.
fn find_transforms(block: &[u8], key: u32) -> Vec<usize> {
    let k = key.to_le_bytes();
    let mut hits = Vec::new();
    let mut i = 0usize;
    while i + REC <= block.len() {
        if block[i] == k[0] && block[i..i + 4] == k && is_transform(block, i, key) {
            hits.push(i);
            i += REC;
        } else {
            i += 1;
        }
    }
    hits
}

fn parse_key(s: &str) -> Option<u32> {
    let t = s.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => t.parse().ok(),
    }
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<String> = Vec::new();
    let mut keys: Vec<u32> = Vec::new();
    let mut dy = 0.0f32;
    let mut set_y: Option<f32> = None;
    let mut list = false;

    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--key" => match it.next().and_then(|s| parse_key(s)) {
                Some(k) => keys.push(k),
                None => {
                    eprintln!("--key: expected 0xHEX or decimal");
                    return 2;
                }
            },
            "--dy" => dy = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            "--set-y" => set_y = it.next().and_then(|s| s.parse().ok()),
            "--list" => list = true,
            s => pos.push(s.to_string()),
        }
    }
    if pos.is_empty() || keys.is_empty() || (!list && pos.len() != 2) {
        eprintln!("usage: place_lift <in.bin> <out.bin> --key 0xKEY [--key ...] --dy <f32>");
        eprintln!("       place_lift <in.bin> --list --key 0xKEY [--key ...]");
        return 2;
    }
    let mut block = match std::fs::read(&pos[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {}: {e}", pos[0]);
            return 1;
        }
    };

    let mut patched = 0usize;
    for &key in &keys {
        let hits = find_transforms(&block, key);
        if hits.is_empty() {
            eprintln!("key 0x{key:08X}: NO Transform record found");
            return 1;
        }
        for off in hits {
            let (x, y0, z) = (rd_f32(&block, off + 4), rd_f32(&block, off + 8), rd_f32(&block, off + 12));
            if list {
                println!("key=0x{key:08X} @{off} pos=({x:.4}, {y0:.4}, {z:.4})");
                continue;
            }
            let y1 = match set_y {
                Some(v) => v,
                None => y0 + dy,
            };
            block[off + 8..off + 12].copy_from_slice(&y1.to_le_bytes());
            println!("key=0x{key:08X} @{off}  Y {y0:.4} -> {y1:.4}   (x={x:.4}, z={z:.4})");
            patched += 1;
        }
    }
    if list {
        return 0;
    }

    // Verify by re-reading what we just wrote, then emit.
    for &key in &keys {
        for off in find_transforms(&block, key) {
            let y = rd_f32(&block, off + 8);
            if !y.is_finite() {
                eprintln!("VERIFY FAILED: key 0x{key:08X} Y not finite");
                return 1;
            }
        }
    }
    if let Err(e) = std::fs::write(&pos[1], &block) {
        eprintln!("write {}: {e}", pos[1]);
        return 1;
    }
    println!("patched {patched} Transform record(s) -> {} ({} bytes, size unchanged)", pos[1], block.len());
    0
}
