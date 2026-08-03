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
//!                        [--add-layer <layer_name_or_0xH> ...]
//!
//! `--add-layer H` appends a NEW layer ASET row (type_id 9 = layer, primary, single-block)
//! for hash `H` to the block named by the MOST RECENT preceding `--replace`. Use it when
//! that block gained a fresh layer sub-block (e.g. via `place_forge`, whose appended
//! sub-block's entry-table name is `H`): the retail engine loads a layer by name-hash
//! through the asset system, so a new sub-block is invisible until an ASET row advertises
//! its name exactly like the block's existing layer rows. This is the missing half — the
//! sub-block supplies the container, this row makes `H` resolvable/advertised.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::types::TYPE_ID_LAYER;
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
    // Each replace carries the new layer hashes (`--add-layer`) attached to it.
    let mut repl: Vec<(String, String, Vec<u32>)> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base-wad" => base = it.next(),
            "--out" => out = it.next(),
            "--replace" => {
                let v = it.next().ok_or("--replace needs NAME=FILE")?;
                let (n, f) = v.split_once('=').ok_or("--replace wants NAME=FILE")?;
                repl.push((n.to_string(), f.to_string(), Vec::new()));
            }
            "--add-layer" => {
                let v = it.next().ok_or("--add-layer needs a layer name or 0xHASH")?;
                let h = parse_hash(&v);
                repl.last_mut()
                    .ok_or("--add-layer must follow a --replace")?
                    .2
                    .push(h);
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
    for (name, file, add_layers) in &repl {
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
        let mut aset: Vec<AsetEntry> = ar
            .aset
            .iter()
            .filter(|e| e.block_index() as usize == bi)
            .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
            .collect();
        let carried = aset.len();

        // Advertise each freshly-injected layer sub-block. A layer that lives entirely in
        // this one block is a primary/single-block row: `u32_1 = 0xFFFFFFFF` (no _P002/_P003),
        // `u32_2` low16 = 0xFFFF (no _P001 — the sentinel that marks it primary; the builder
        // overwrites the high16 with this block's position), `type_id = 9` (layer). This
        // mirrors the exact shape of the block's 173 existing layer rows (verified via
        // `mercs2_probe aset_probe`: each is type=9, primary=1, single block).
        for &h in add_layers {
            if aset.iter().any(|e| e.asset_hash == h) {
                return Err(format!(
                    "--add-layer 0x{h:08X}: block {bi} already advertises this hash"
                ));
            }
            aset.push(AsetEntry::new(h, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_LAYER));
        }

        // Inherit the Xbox tier byte from the base INDX row; page count is recomputed.
        let tier = ar.indx.get(bi).map(|i| i.packed_field);
        let raw = std::fs::read(file).map_err(|e| format!("read {file}: {e}"))?;
        let blk = PatchBlock::from_decompressed(&raw, path.clone(), aset, tier)?;
        println!(
            "  base block {bi} '{path}' <- {file}\n     {} bytes decompressed, {} aset rows ({carried} carried + {} new layer), {} pages declared",
            raw.len(),
            blk.aset_entries.len(),
            add_layers.len(),
            blk.declared_pages()
        );
        blocks.push(blk);
    }

    let wad_bytes = build_patch_wad_multi(&blocks, 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad_bytes).map_err(|e| format!("write {out}: {e}"))?;
    println!("Wrote {out} ({} bytes, {} blocks)", wad_bytes.len(), blocks.len());
    Ok(())
}
