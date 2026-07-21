//! Build a patch WAD that overrides RESIDENT BASE-GAME blocks.
//!
//! Why this exists: `replace-block` edits blocks that already live in an existing patch WAD, and
//! the shipped `vz-patch.wad` carries only `dlc01` blocks. Hosting an injected character there is
//! WRONG — a DLC costume model is not resident, so selecting it takes the on-demand load path and
//! wedges on the unreleased `STATE_WAITFORGAME` refcount (hang, or the `0x0052A10E` AV seen when
//! an injected model sits resident-but-inactive). Base costumes work precisely BECAUSE they are
//! already resident. So an injected character must replace a RESIDENT BASE block, which means
//! emitting a patch WAD whose block paths match `vz.wad`'s own — WAD-overlay resolution is
//! last-wins, so the patch shadows the base block in place.
//!
//! `build_patch_wad_multi` re-numbers each ASET row's block index into the new WAD
//! (`(blk_idx << 16) | (u2 & 0xFFFF)`), so the base rows are carried through verbatim.
//!
//! Usage:
//!   override_base_blocks --base-wad <vz.wad> --out <patch.wad>
//!                        --replace <model_name_or_0xHASH>=<injected_block.bin> [--replace ...]

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use std::fs::File;

fn parse_hash(s: &str) -> u32 {
    s.strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| pandemic_hash_m2(s))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("override_base_blocks: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut base = None;
    let mut out = None;
    let mut repl: Vec<(String, String)> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base-wad" => base = it.next(),
            "--out" => out = it.next(),
            "--replace" => {
                let v = it.next().ok_or("--replace needs NAME=FILE")?;
                let (n, f) = v.split_once('=').ok_or("--replace wants NAME=FILE")?;
                repl.push((n.to_string(), f.to_string()));
            }
            o => return Err(format!("unknown arg {o}")),
        }
    }
    let base = base.ok_or("--base-wad required")?;
    let out = out.ok_or("--out required")?;
    if repl.is_empty() {
        return Err("at least one --replace required".into());
    }

    let mut f = File::open(&base).map_err(|e| format!("open {base}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("parse {base}: {e}"))?;

    let mut blocks: Vec<PatchBlock> = Vec::new();
    for (name, file) in &repl {
        let hash = parse_hash(name);
        let primary = ar
            .aset
            .iter()
            .find(|e| e.asset_hash == hash && e.is_primary())
            .ok_or_else(|| format!("{name} (0x{hash:08X}): no primary ASET row in {base}"))?;
        let bi = primary.block_index() as usize;
        let path = ar
            .paths
            .get(bi)
            .cloned()
            .ok_or_else(|| format!("{name}: block {bi} has no path"))?;

        // Carry EVERY row that points at this block, so the block's full advertisement survives.
        let aset: Vec<AsetEntry> = ar
            .aset
            .iter()
            .filter(|e| e.block_index() as usize == bi)
            .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
            .collect();

        // Inherit the Xbox tier byte from the base INDX row; page count is recomputed.
        let tier = ar.indx.get(bi).map(|i| i.packed_field);
        let raw = std::fs::read(file).map_err(|e| format!("read {file}: {e}"))?;
        let blk = PatchBlock::from_decompressed(&raw, path.clone(), aset, tier)?;
        println!(
            "  base block {bi} '{path}' <- {file}\n     {} bytes decompressed, {} aset rows, {} pages declared",
            raw.len(),
            blk.aset_entries.len(),
            blk.declared_pages()
        );
        blocks.push(blk);
    }

    let wad_bytes = build_patch_wad_multi(&blocks, 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad_bytes).map_err(|e| format!("write {out}: {e}"))?;
    println!("Wrote {out} ({} bytes, {} blocks)", wad_bytes.len(), blocks.len());
    Ok(())
}
