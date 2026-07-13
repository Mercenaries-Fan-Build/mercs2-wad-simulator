//! block_neuter — hide the donor geometry in EVERY UCFX container inside a raw decompressed block.
//!
//! A vehicle's LOD chain is bigger than the three rungs `--dump-container` can reach. The ztz98 also
//! ships:
//!   * `ch_veh_tank_ztz98_P003_Q0`            — a FOURTH, finest rung (1.7 MB), and
//!   * `resident2-ch_veh_tank_ztz98_tracks_*` — a SEPARATE tracks model chain,
//! and both are SUB-ENTRY models with no model ASET row, so the container tools cannot see them at
//! all. Left alone they keep streaming the DONOR's hull/tracks in at close range, straight through
//! the conformed model.
//!
//! Hiding a group means COLLAPSING its vertex POSITIONS to the origin (degenerate triangles) and
//! zeroing its PRMT draw-counts — NOT emptying the buffers. The engine binds every drawing group's
//! vertex buffer even when its draw-count is 0 and faults on a zero-size one (AV 0x0085C8D0).
//! Collapsing is byte-size-preserving, so the block is patched in place and each container's CSUM
//! recomputed; everything else stays verbatim.
//!
//! Usage:  block_neuter <block.bin> <out.bin>

use mercs2_formats::model_inject::collapse_drawing_groups_in_place;
use mercs2_formats::ucfx::parse_block_entry_table;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 2 {
        eprintln!("usage: block_neuter <block.bin> <out.bin>");
        std::process::exit(2);
    }
    let mut blk = std::fs::read(&a[0]).expect("read block");
    let (count, entries) = parse_block_entry_table(&blk);
    let mut off = 4 + count as usize * 16;
    let mut total = 0usize;
    for (i, e) in entries.iter().enumerate() {
        let end = off + e.chunk_size as usize;
        if end > blk.len() {
            break;
        }
        if blk[off..].starts_with(b"UCFX") {
            match collapse_drawing_groups_in_place(&mut blk[off..end]) {
                Ok(n) if n > 0 => {
                    println!(
                        "  entry {i}: name=0x{:08X} type=0x{:08X} — collapsed {n} drawing group(s)",
                        e.name_hash, e.type_hash
                    );
                    total += n;
                }
                Ok(_) => {}
                Err(err) => eprintln!("  entry {i}: {err}"),
            }
        }
        off = end;
    }
    std::fs::write(&a[1], &blk).expect("write out");
    println!("collapsed {total} drawing group(s) -> {} ({} bytes)", a[1], blk.len());
}
