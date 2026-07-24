//! Repair a patch WAD's dangling ASET LOD rungs by CARRYING the referenced base blocks in.
//!
//! `aset_refcheck` reports rungs (`_P001`/`_P002`/`_P003`) that point outside the patch's own block
//! table. Sentinelling them to 0xFFFF stops the stream wedging, but the asset then silently degrades
//! to its coarse tier — base-game content quietly lost. This does the non-lossy repair instead: pull
//! each referenced block out of the base WAD, append it to the patch, and re-point every rung at its
//! new index, so the whole LOD chain resolves inside the patch.
//!
//! Cheap in practice — `aset_closure` measures the pmcoutpost_fountain chain at 3 blocks / 1.5 MB
//! with nothing further pulled in.
//!
//! Runs on the FINAL artifact on purpose: a merge round-trip re-derives `source_block_index` from
//! position, so a block's true base index does not survive an intermediate WAD. Carried blocks are
//! added with NO ASET rows — a rung is resolved by block INDEX, so the payload is all the engine
//! needs, and re-registering the assets could collide with the base registration.
//!
//! Usage: `heal_lod_rungs <patch.wad> <base.wad> <out.wad>`

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::patch_wad::{build_patch_wad_multi, read_patch_wad, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::decompress_block;
use std::collections::BTreeSet;
use std::fs::File;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 3 {
        eprintln!("usage: heal_lod_rungs <patch.wad> <base.wad> <out.wad>");
        std::process::exit(2);
    }
    let (patch_path, base_path, out_path) = (&a[0], &a[1], &a[2]);

    let patch_bytes = std::fs::read(patch_path).expect("read patch");
    let mut contents = read_patch_wad(&patch_bytes).expect("parse patch");
    let n0 = contents.blocks.len();

    // every rung that cannot resolve inside this patch
    let mut dangling: BTreeSet<u16> = BTreeSet::new();
    for blk in &contents.blocks {
        for e in &blk.aset_entries {
            for r in [
                (e.u32_2 & 0xFFFF) as u16,
                (e.u32_1 >> 16) as u16,
                (e.u32_1 & 0xFFFF) as u16,
            ] {
                if r != 0xFFFF && r as usize >= n0 {
                    dangling.insert(r);
                }
            }
        }
    }
    if dangling.is_empty() {
        println!("{patch_path}: no dangling rungs — nothing to heal");
        std::fs::write(out_path, &patch_bytes).expect("write out");
        return;
    }
    println!("{n0} blocks, {} distinct dangling rung(s): {dangling:?}", dangling.len());

    let mut base_file = File::open(base_path).expect("open base");
    let base_size = base_file.metadata().map(|m| m.len()).unwrap_or(0);
    let base = load_ffcs_archive(&mut base_file, base_size).expect("parse base");

    // carry each referenced base block in, recording where it landed
    let mut map: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();
    for &r in &dangling {
        let raw = decompress_block(&mut base_file, &base.indx, r)
            .unwrap_or_else(|e| panic!("decompress base block {r}: {e}"));
        let path = base
            .paths
            .get(r as usize)
            .cloned()
            .unwrap_or_else(|| format!("blocks\\VZ\\carried_{r}.block"));
        let tier = base.indx.get(r as usize).map(|e| e.packed_field);
        let blk = PatchBlock::from_decompressed(&raw, path.clone(), Vec::new(), tier)
            .unwrap_or_else(|e| panic!("build carried block {r}: {e}"));
        let new_idx = contents.blocks.len() as u16;
        contents.blocks.push(blk);
        map.insert(r, new_idx);
        println!("  carried base block {r} -> patch index {new_idx}: {path} ({} bytes)", raw.len());
    }

    // re-point every rung, and drop source indices so the builder keeps what we just healed
    // (with no source space declared it only sentinels refs that cannot resolve at all).
    let mut healed = 0usize;
    for blk in &mut contents.blocks {
        blk.source_block_index = None;
        for e in &mut blk.aset_entries {
            let (mut p1, mut p2, mut p3) = (
                (e.u32_2 & 0xFFFF) as u16,
                (e.u32_1 >> 16) as u16,
                (e.u32_1 & 0xFFFF) as u16,
            );
            let before = (p1, p2, p3);
            for slot in [&mut p1, &mut p2, &mut p3] {
                if let Some(&to) = map.get(slot) {
                    *slot = to;
                }
            }
            if (p1, p2, p3) != before {
                healed += 1;
            }
            e.u32_2 = (e.u32_2 & 0xFFFF_0000) | p1 as u32;
            e.u32_1 = ((p2 as u32) << 16) | p3 as u32;
        }
    }

    let wad = build_patch_wad_multi(&contents.blocks, contents.csum_value, None, &FFCS_CERT_BLOB)
        .expect("rebuild patch");
    std::fs::write(out_path, &wad).expect("write out");
    println!(
        "healed {healed} row(s); {} -> {} blocks; wrote {out_path} ({} bytes)",
        n0,
        contents.blocks.len(),
        wad.len()
    );
}
