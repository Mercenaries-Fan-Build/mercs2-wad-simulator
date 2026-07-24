//! Verify every ASET row's LOD block references resolve inside the WAD's OWN block table.
//!
//! A patch WAD carries ASET rows copied out of a source WAD. Each row packs up to four block
//! indices — `_P000` (`packed_block_ref` hi16) plus `_P001`/`_P002`/`_P003` — and those indices are
//! relative to the archive that owns the row. Copying a block out of an 11,370-block `vz.wad` into a
//! 29-block patch therefore leaves the finer LOD rungs pointing at indices that do not exist in the
//! patch. The engine reads INDX out of range, decodes whatever bytes sit there as a page index and
//! `packed_field`, and can end up sizing a streaming buffer at hundreds of gigabytes. That request
//! can never be satisfied, the node never reaches provider-status 4, `Stream_Manager_Tick`
//! (`FUN_008739e0`) only unlinks on 4, so the pending count never drains and the world-streaming
//! gate (`FUN_004b9af0`) never releases — a spinning hang with no crash and no error.
//!
//! Every WAD that loads has zero dangling refs (measured: retail base `vz.wad` 30645 rows; our
//! WORKING dlc01 port patch 5451 rows), while the wardrobe builds that hang carry 22 each.
//! NOTE: there is no Pandemic-shipped `vz-patch.wad` — every vz-patch is a build of ours.
//!
//! Usage: `aset_refcheck <wad> [<wad> ...]`  — exit code 1 if any WAD has a violation.

use mercs2_formats::ffcs::load_ffcs_archive;
use std::fs::File;

fn main() {
    let wads: Vec<String> = std::env::args().skip(1).collect();
    if wads.is_empty() {
        eprintln!("usage: aset_refcheck <wad> [<wad> ...]");
        std::process::exit(2);
    }

    let mut any_bad = false;
    for path in &wads {
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                println!("{path}: OPEN FAILED: {e}");
                any_bad = true;
                continue;
            }
        };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let archive = match load_ffcs_archive(&mut file, size) {
            Ok(a) => a,
            Err(e) => {
                println!("{path}: PARSE FAILED: {e}");
                any_bad = true;
                continue;
            }
        };

        let nblocks = archive.indx.len();
        // rung label by position in `lod_chain()`: index 0 is always _P000, then the non-sentinel
        // halves in _P001, _P002, _P003 order.
        let mut violations: Vec<(usize, u32, usize, u16)> = Vec::new();
        for (row, e) in archive.aset.iter().enumerate() {
            for (rung, blk) in e.lod_chain().iter().enumerate() {
                if (*blk as usize) >= nblocks {
                    violations.push((row, e.asset_hash, rung, *blk));
                }
            }
        }

        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        if violations.is_empty() {
            println!("OK   {name}: {} rows / {nblocks} blocks — all LOD refs resolve", archive.aset.len());
        } else {
            any_bad = true;
            println!(
                "FAIL {name}: {} rows / {nblocks} blocks — {} DANGLING LOD ref(s)",
                archive.aset.len(),
                violations.len()
            );
            // distinct offending block indices, with how far out of range they are
            let mut seen: Vec<u16> = violations.iter().map(|v| v.3).collect();
            seen.sort_unstable();
            seen.dedup();
            for b in &seen {
                let n = violations.iter().filter(|v| v.3 == *b).count();
                println!("       -> block {b} (table has {nblocks}) x{n} row(s)");
            }
            for (row, hash, rung, blk) in violations.iter().take(12) {
                println!("       row {row}: asset 0x{hash:08X} rung[{rung}] -> block {blk}");
            }
        }
    }

    if any_bad {
        std::process::exit(1);
    }
}
