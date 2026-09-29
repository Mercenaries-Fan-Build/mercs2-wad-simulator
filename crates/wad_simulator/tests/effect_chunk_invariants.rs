//! The chunk-invariant validator passes every retail effect.
//!
//! `chunk_invariants` checks effect chunks against their proven sizes: EFCT ≥ 18, PTYP ≥ 4 (a u32
//! flags word), COLR ≥ 800 (100 × 8-byte keys), ANIM ≥ 4, AKEY ≥ 8 (see
//! `docs/effect_container_format.md`). Raising PTYP from 1 and COLR from 0xC8 made those checks
//! stricter; this runs the validator over every effect container in retail `vz.wad` and requires
//! zero violations, so the stricter minimums are shown to hold on real data.
//!
//! Game-gated: without `vz.wad` it prints `SKIPPING` and returns.

use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_EFFECT, TYPE_ID_EFFECT};
use mercs2_formats::ucfx::parse_block_entry_table;
use wad_simulator::chunk_invariants::validate_chunk_invariants;

#[test]
fn every_retail_effect_passes_the_chunk_invariants() {
    let Some(wad) = mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))) else {
        eprintln!("SKIPPING: no vz.wad (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        return;
    };
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");

    let mut blocks: Vec<u16> = archive
        .aset
        .iter()
        .filter(|a| a.type_id == TYPE_ID_EFFECT)
        .flat_map(|a| a.lod_chain())
        .filter(|&b| b != 0xFFFF)
        .collect();
    blocks.sort_unstable();
    blocks.dedup();

    let mut effects = 0usize;
    let mut failures = Vec::new();
    for bi in blocks {
        let dec = decompress_block(&mut f, &archive.indx, bi).expect("decompress");
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == TYPE_HASH_EFFECT {
                effects += 1;
                let label = format!("effect 0x{:08X}", e.name_hash);
                let r = validate_chunk_invariants(&dec[pos..end], &label);
                if r.violations != 0 {
                    failures.push(format!("{label}: {} violations {:?}", r.violations, r.issues));
                }
            }
            pos = end;
        }
    }
    eprintln!("{effects} retail effects validated, {} with violations", failures.len());
    assert_eq!(effects, 314);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
