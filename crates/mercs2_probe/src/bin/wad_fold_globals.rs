//! wad_fold_globals — build a `dlc01-patch.wad` that supplies the base GLOBAL container blocks the
//! DLC arena needs but `dlc01.wad` lacks, so the arena boots self-contained on the retail engine.
//!
//! WHY (live-confirmed 2026-09-02, see memory `dlc-level-boot-and-replacement-architecture`): level
//! "dlc01" mounts `dlc01.wad` ALONE in the `<level>.wad` slot — `vz.wad` never co-mounts — so the
//! ~26k base assets that only live in `vz.wad` (`ui_hud`, `ui_shell`, `MUSIC`, `guilayouts`, `effects`,
//! fonts, `global_*`, scaleform HUD gfx) have no provider. GlobalEnter's HUD init then can't stream its
//! deps → a streaming node never reaches status 4 → the WAITFORSTREAMING spin
//! (STATUS_OBJECT_NAME_NOT_FOUND). This tool folds those base container blocks into a
//! `dlc01-patch.wad` (mounts one slot ABOVE `dlc01.wad`, last-wins), single-source from `vz.wad` so
//! `build_patch_wad_multi`'s LOD-rung remap has one index space (no two-WAD collision).
//!
//! DELIBERATELY EXCLUDED (would recreate the historic first-wins collision / spatial-hash AV, or is
//! Venezuela content the arena master script never requests): base `low_res_terrain`, `layers_static`,
//! `vz_state*`, `scripts_vz`, `vz_base`, `c3*` cells, `*contract*`, `*job*`, and (v1, to protect the
//! arena's own resident/contracts) `resident`/`resident2`. Any base block whose owned hash `dlc01.wad`
//! ALREADY owns is skipped, so nothing shadows arena content.
//!
//! Usage: wad_fold_globals <vz.wad> <dlc01.wad> <out dlc01-patch.wad>

use std::collections::HashSet;

use mercs2_formats::patch_wad::{build_patch_wad_multi, read_patch_wad, FFCS_CERT_BLOB};
use sha2::{Digest, Sha256};

/// Global-container families to CARRY from vz.wad (matched as a lowercased-path substring — picks up
/// every P-level of each family, so the LOD-rung closure comes along).
const KEEP: &[&str] = &[
    "ui_hud", "ui_shell", "guilayouts", "music", "effects", "global_", "font_16", "scaleform",
];

/// Families to EXCLUDE regardless (collision risk or Venezuela content the arena never requests).
const EXCLUDE: &[&str] = &[
    "low_res_terrain", "layers_static", "vz_state", "scripts_vz", "vz_base", "contract", "job",
    "resident", "dlctest", "\\c3", "/c3",
];

fn sha_full(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: wad_fold_globals <vz.wad> <dlc01.wad> <out dlc01-patch.wad>");
        std::process::exit(2);
    }
    let vz_bytes = std::fs::read(&args[0]).unwrap_or_else(|e| panic!("read {}: {e}", args[0]));
    let dlc_bytes = std::fs::read(&args[1]).unwrap_or_else(|e| panic!("read {}: {e}", args[1]));
    let out_path = &args[2];

    let vz = read_patch_wad(&vz_bytes).expect("parse vz.wad as FFCS");
    let dlc = read_patch_wad(&dlc_bytes).expect("parse dlc01.wad");
    println!("vz.wad: {} blocks; dlc01.wad: {} blocks", vz.blocks.len(), dlc.blocks.len());

    // Every asset hash dlc01.wad already owns — never shadow these (auto-excludes the arena's own
    // low_res_terrain and the 763 base-named blocks it overrides).
    let h_dlc: HashSet<u32> = dlc
        .blocks
        .iter()
        .flat_map(|b| b.aset_entries.iter().map(|a| a.asset_hash))
        .collect();
    println!("dlc01 owns {} distinct asset hashes (skip any base block that touches these)", h_dlc.len());

    let mut folded = Vec::new();
    let (mut n_excl, mut n_nomatch, mut n_conflict) = (0u32, 0u32, 0u32);
    for b in &vz.blocks {
        let p = b.path_string.to_lowercase();
        if EXCLUDE.iter().any(|e| p.contains(e)) {
            n_excl += 1;
            continue;
        }
        if !KEEP.iter().any(|k| p.contains(k)) {
            n_nomatch += 1;
            continue;
        }
        if b.aset_entries.iter().any(|a| h_dlc.contains(&a.asset_hash)) {
            n_conflict += 1;
            continue;
        }
        folded.push(b.clone());
    }
    println!(
        "selected {} base global-container blocks to fold ({} excluded-family, {} non-global, {} hash-conflict skipped)",
        folded.len(), n_excl, n_nomatch, n_conflict
    );
    if folded.is_empty() {
        eprintln!("ERROR: selected 0 blocks — check the vz.wad path or the KEEP families");
        std::process::exit(1);
    }
    // show the families we're carrying
    for b in folded.iter().take(30) {
        println!("  + {} ({} rows)", b.path_string, b.aset_entries.len());
    }
    if folded.len() > 30 {
        println!("  … and {} more", folded.len() - 30);
    }

    // Single-source build (all from vz.wad) so the rung remap has one source index space.
    let out = build_patch_wad_multi(&folded, vz.csum_value, None, &FFCS_CERT_BLOB)
        .expect("build dlc01-patch.wad");
    std::fs::write(out_path, &out).unwrap_or_else(|e| panic!("write {out_path}: {e}"));
    println!("\nwrote {out_path}: {} bytes, {} blocks, sha256 {}", out.len(), folded.len(), sha_full(&out));

    // Post-build self-check: re-parse.
    let rb = read_patch_wad(&out).expect("re-parse out");
    let total_rows: usize = rb.blocks.iter().map(|b| b.aset_entries.len()).sum();
    println!("POST-BUILD: {} blocks, {} ASET rows re-parse OK", rb.blocks.len(), total_rows);
    // confirm the confirmed-missing globals are present
    for want in ["ui_hud", "ui_shell", "music", "guilayouts", "effects", "font_16"] {
        let present = rb.blocks.iter().any(|b| b.path_string.to_lowercase().contains(want));
        println!("  global '{want}': {}", if present { "PRESENT" } else { "MISSING" });
    }
    println!("\nDeploy: {out_path} -> <game>\\data\\dlc01-patch.wad (dlc01.wad unchanged). Then aset_refcheck both.");
}
