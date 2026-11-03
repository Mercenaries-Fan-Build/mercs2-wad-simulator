//! c3_veg_forge — M2.2 of the density-upgrade reroute: inject a CLUSTER veg MODEL into existing
//! c3 streaming cells so the engine renders dense trees through its OWN per-cell foliage pipeline
//! (no SceneObject entities → no registry grow-storm, no veg spatial-hash pool exhaustion).
//!
//! Why this is the right route (see memory `veg-density-pool-12bit-ceiling`): the entity/placement
//! path hits a 4096-node, 12-bit-capped veg spatial-hash and livelocks. The engine's native trees
//! are MODELS (chunk 0x5B724250) bundled inside c3 cells; a cluster model (e.g. largecanopy01,
//! 0xFF7ABB3B — 14 mesh groups on multiple bones, ~44 m stand) renders many trees at one cell.
//! Density = place cluster models into MORE cells. Reuse an existing cluster (no invented hash;
//! textures resolve globally by MTRL name-hash, so the model body is self-contained across cells).
//!
//! A c3 cell block is `[u32 count][count × 16-B entry {name_hash,type_hash,field_c,chunk_size}]
//! [contiguous chunk bodies]` (entry[i] ↔ body[i] in order). Injection = append one entry + the
//! donor model's body, bump count. Ships ADDITIVELY via a patch-WAD overlay that shadows the base
//! cell blocks in place (last-wins); base vz.wad stays PRISTINE.
//!
//! Usage:
//!   c3_veg_forge --base-wad <vz.wad> --out <overlay.wad>
//!       [--x <world_x> --z <world_z> --radius <m>]   (target area; default PMC 2560,-926 r=200)
//!       [--donor-cell <id>] [--model <0xHASH>]       (default 30470 / largecanopy01)
//!       [--max-cells <n>] [--advertise]              (--advertise also adds a model ASET row)

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_ID_MODEL;
use mercs2_formats::ucfx::{parse_block_entry_table, walk_decompressed_block};
use mercs2_formats::world_index::{c3_cell_centre, BlockClass, WorldIndex};
use std::fs::File;

const MODEL_TYPE: u32 = 0x5B72_4250; // pandemic_hash_m2("model")

fn parse_hash(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("bad --model hash")
}

/// Append a bundled model chunk (`entry {name,type,field_c,size}` + `body`) to a decompressed
/// c3 cell block. Entry order == body order, so we splice a new entry at the end of the table and
/// its body at the end of the bodies. Bumps the leading count.
fn append_model_to_block(block: &[u8], name: u32, type_hash: u32, field_c: u32, body: &[u8]) -> Vec<u8> {
    let count = u32::from_le_bytes(block[0..4].try_into().unwrap());
    let table_end = 4 + count as usize * 16;
    let mut out = Vec::with_capacity(block.len() + 16 + body.len());
    out.extend_from_slice(&(count + 1).to_le_bytes()); // new count
    out.extend_from_slice(&block[4..table_end]); // original entries
    out.extend_from_slice(&name.to_le_bytes()); // new entry
    out.extend_from_slice(&type_hash.to_le_bytes());
    out.extend_from_slice(&field_c.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&block[table_end..]); // original bodies
    out.extend_from_slice(body); // new body (contiguous, after all originals)
    out
}

fn main() {
    if let Err(e) = run() {
        eprintln!("c3_veg_forge: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (mut base, mut out) = (None, None);
    let (mut px, mut pz, mut radius) = (2560.0_f32, -926.0_f32, 200.0_f32);
    let (mut donor_cell, mut model) = (30470_u32, 0xFF7A_BB3B_u32);
    let mut max_cells = 64_usize;
    let mut advertise = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut next = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--base-wad" => base = Some(next()?),
            "--out" => out = Some(next()?),
            "--x" => px = next()?.parse().map_err(|_| "bad --x")?,
            "--z" => pz = next()?.parse().map_err(|_| "bad --z")?,
            "--radius" => radius = next()?.parse().map_err(|_| "bad --radius")?,
            "--donor-cell" => donor_cell = next()?.parse().map_err(|_| "bad --donor-cell")?,
            "--model" => model = parse_hash(&next()?),
            "--max-cells" => max_cells = next()?.parse().map_err(|_| "bad --max-cells")?,
            "--advertise" => advertise = true,
            o => return Err(format!("unknown arg {o}")),
        }
    }
    let base = base.ok_or("--base-wad required")?;
    let out = out.ok_or("--out required")?;

    let mut f = File::open(&base).map_err(|e| format!("open {base}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("parse {base}: {e}"))?;
    let idx = WorldIndex::build(&ar, &mut f);

    // ── 1. donor: extract the cluster model body from whichever LOD tier of the donor cell has it.
    //     Track which tier (0=P000 finest .. 3=P003 coarsest) so we inject into the matching tier. ──
    let donor_chain = idx.lod_chain(donor_cell);
    let mut found = None;
    for (tier, slot) in donor_chain.iter().enumerate() {
        let Some(b) = slot else { continue };
        let dec = decompress_block(&mut f, &ar.indx, b.block_index)?;
        let (blk, _) = walk_decompressed_block(&dec, "donor");
        if let Some(mi) = blk
            .entries
            .iter()
            .position(|e| e.name_hash == model && e.type_hash == MODEL_TYPE)
        {
            found = Some((tier, blk.containers[mi].clone(), blk.entries[mi].field_c, b.block_index));
            break;
        }
    }
    let (donor_tier, model_body, model_field_c, donor_bi) = found.ok_or_else(|| {
        format!("model 0x{model:08X} not found in any LOD tier of donor cell {donor_cell}")
    })?;
    println!(
        "donor cell {donor_cell} tier P00{donor_tier} (block {donor_bi}): model 0x{model:08X} body = {} bytes (field_c=0x{model_field_c:08X})",
        model_body.len()
    );

    // ── 2. target cells: existing c3 cells whose centre is within `radius` of (px,pz) ──
    let mut target_cells: Vec<(u32, f32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for b in &idx.blocks {
        if b.class != BlockClass::C3Cell {
            continue;
        }
        let Some(cid) = b.lod.base_cell_id else { continue };
        if !seen.insert(cid) {
            continue;
        }
        let (cx, cz) = c3_cell_centre(cid);
        let d = ((cx - px).powi(2) + (cz - pz).powi(2)).sqrt();
        if d <= radius {
            target_cells.push((cid, d));
        }
    }
    target_cells.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    target_cells.truncate(max_cells);
    if target_cells.is_empty() {
        return Err(format!(
            "no existing c3 cells within {radius} m of ({px},{pz}) — pick a spot with cells"
        ));
    }
    println!(
        "{} target c3 cells within {radius} m of ({px},{pz}) [nearest {:.0} m, farthest {:.0} m]",
        target_cells.len(),
        target_cells.first().unwrap().1,
        target_cells.last().unwrap().1
    );

    // ── 3. per target cell: append the model to its P000 block, wrap as an override PatchBlock ──
    let mut blocks: Vec<PatchBlock> = Vec::new();
    for (cid, dist) in &target_cells {
        let Some(p0) = idx.lod_chain(*cid)[donor_tier] else {
            eprintln!("  cell {cid}: no P00{donor_tier} block — skip");
            continue;
        };
        let bi = p0.block_index;
        // skip if this cell already carries the model (idempotent)
        let dec = decompress_block(&mut f, &ar.indx, bi)?;
        let (count, entries) = parse_block_entry_table(&dec);
        if entries.iter().any(|e| e.name_hash == model && e.type_hash == MODEL_TYPE) {
            println!("  cell {cid} (block {bi}): already has model — skip");
            continue;
        }
        let modified = append_model_to_block(&dec, model, MODEL_TYPE, model_field_c, &model_body);

        let path = ar
            .paths
            .get(bi as usize)
            .cloned()
            .ok_or_else(|| format!("block {bi} has no path"))?;
        // carry EVERY existing ASET row that points at this block so its advertisement survives
        let mut aset: Vec<AsetEntry> = ar
            .aset
            .iter()
            .filter(|e| e.block_index() == bi)
            .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
            .collect();
        // Optionally advertise the bundled model as a single-block primary row (type_id 19 = model).
        // Off by default — the per-cell collect walks the block entry table to find bundled models,
        // so a fresh ASET row may be unnecessary (and 0xFF7ABB3B is already advertised elsewhere;
        // the registry is first-wins, so a duplicate could be dropped). Toggle if it doesn't render.
        if advertise && !aset.iter().any(|e| e.asset_hash == model) {
            aset.push(AsetEntry::new(model, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL));
        }
        let tier = ar.indx.get(bi as usize).map(|i| i.packed_field);
        let blk = PatchBlock::from_decompressed(&modified, path.clone(), aset, tier)?;
        println!(
            "  cell {cid} (block {bi}, {dist:.0} m) '{path}': {} -> {} bytes, count {}->{}, {} pages",
            dec.len(),
            modified.len(),
            count,
            count + 1,
            blk.declared_pages()
        );
        blocks.push(blk);
    }
    if blocks.is_empty() {
        return Err("no cells modified (all already had the model?)".into());
    }

    let wad_bytes = build_patch_wad_multi(&blocks, 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad_bytes)
        .map_err(|e| format!("write {out}: {e}"))?;
    println!(
        "Wrote {out} ({} bytes, {} cells modified). Mount as data/vz-patch.wad (overlay, last-wins).",
        wad_bytes.len(),
        blocks.len()
    );
    Ok(())
}
