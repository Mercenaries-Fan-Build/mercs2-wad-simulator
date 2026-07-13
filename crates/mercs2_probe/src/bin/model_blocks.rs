//! Is a model's geometry spread across BLOCKS the way a texture's mips are?
//!
//! `extract_container` reads exactly one block (the primary model ASET) — but the tank's container
//! holds only far-LOD meshes (every mask is rung 4-6, skinned `*_lod_dm`). If the near LODs live in
//! finer c3-cell blocks, the same subtree walk that `extract_texture_hires` does for mips will find
//! them. Print every block carrying a chunk for this model hash, and what LOD masks its meshes have.
//!
//!   cargo run -p mercs2_probe --bin model_blocks -- ch_veh_tank_ztz98

use mercs2_engine::{mesh, wad};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    println!("{name} = {hash:#010x}\n");

    // Every ASET row for this hash, whatever its type.
    let rows: Vec<(u16, u32, bool)> = {
        let a = wad::archive_and_file(&mut w).0;
        a.aset
            .iter()
            .filter(|e| e.asset_hash == hash)
            .map(|e| (e.block_index(), e.type_id, e.is_primary()))
            .collect()
    };
    println!("ASET rows for this hash: {}", rows.len());
    for (b, t, p) in &rows {
        let path = wad::block_paths(&w).get(*b as usize).cloned().unwrap_or_default();
        let leaf = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        println!("   block {b:5}  type_id {t:3}  primary {p:5}  {leaf}");
    }

    // Now: which blocks actually contain a chunk named by this hash, and what geometry is in it?
    let nblocks = wad::block_paths(&w).len() as u16;
    println!("\nscanning {nblocks} blocks for chunks named {hash:#010x} ...");
    let mut hits = 0;
    for b in 0..nblocks {
        let Ok(dec) = wad::decompress_block_index(&mut w, b) else { continue };
        let Some(container) = wad::model_span_in(&dec, hash) else { continue };
        let (masks, tris): (std::collections::BTreeSet<u8>, u32) =
            match mesh::build_indexed_all(&container) {
                Ok((_, _, draws, _)) => (
                    draws.iter().map(|d| d.lod_mask).collect(),
                    draws.iter().map(|d| d.index_count / 3).sum(),
                ),
                Err(_) => (Default::default(), 0),
            };
        let path = wad::block_paths(&w).get(b as usize).cloned().unwrap_or_default();
        let leaf = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        println!(
            "   block {b:5}  {:8} bytes  {tris:6} tri  masks {masks:02X?}  {leaf}",
            container.len()
        );
        hits += 1;
    }
    println!("\n{hits} block(s) carry geometry for this model.");
}
