//! `terrain-cell <0xH> [--at <x> <z>]` — one hi-res terrain cell as `mercs2_formats::terrainmesh` reads
//! it: its materials, and every draw group of the patch holding a world point, with what each group's
//! vertex stream carries and the `PRMT` records that draw it.

use mercs2_engine::asset::AssetSource;
use mercs2_formats::terrainmesh::{Prmg, TerrainCell};
use mercs2_formats::types::TYPE_HASH_TERRAIN_MESH;
use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};

fn ranges(p: &Prmg, poff: [f32; 3]) -> Result<String, String> {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let (mut uv_lo, mut uv_hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let mut has_uv = false;
    for i in 0..p.vertex_count() {
        let at = p.position(i);
        for k in 0..3 {
            lo[k] = lo[k].min(at[k] + poff[k]);
            hi[k] = hi[k].max(at[k] + poff[k]);
        }
        if let Some(uv) = p.texcoord(i)? {
            has_uv = true;
            for k in 0..2 {
                uv_lo[k] = uv_lo[k].min(uv[k]);
                uv_hi[k] = uv_hi[k].max(uv[k]);
            }
        }
    }
    let uv = if has_uv { format!("uv {uv_lo:.2?}..{uv_hi:.2?}") } else { "no uv".to_string() };
    Ok(format!("cell-local {lo:.1?}..{hi:.1?} {uv}"))
}

pub fn terrain_cell(wadpath: &str, hash: u32, centre: Option<[f32; 2]>, at: Option<[f32; 2]>) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let we = assets
        .ecs_container(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)?
        .ok_or("worldentity: in no open wad")?;
    let we = WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?;
    for g in we.groups_of("TerrainFade") {
        for r in &g.records {
            println!("TerrainFade keys {:08X?} {:?}", r.keys, g.decode(&r.payload).map_err(|e| format!("TerrainFade: {e}"))?);
        }
    }
    let c = assets.resolve(TYPE_HASH_TERRAIN_MESH, hash).ok_or_else(|| format!("0x{hash:08X}: no terrainmesh in {:?}", assets.paths()))?;
    let cell = TerrainCell::decode(&c)?;
    println!("cell 0x{hash:08X}: {} materials, {} patches", cell.mtrl.materials.len(), cell.geoms.len());
    for (i, m) in cell.mtrl.materials.iter().enumerate() {
        println!("  material {i}: word0 0x{:08X} flags 0x{:04X} tail {:08X?} textures {:08X?}", m.word_0, m.flags, m.tail, m.textures);
        println!("    params {:?}", m.params);
        for (r, row) in cell.mtrl.blocks[i].chunks(4).enumerate() {
            if row.iter().any(|&w| w != 0) {
                println!("    block[{r}] {:?}", row.iter().map(|&w| f32::from_bits(w)).collect::<Vec<_>>());
            }
        }
    }
    for (g, geom) in cell.geoms.iter().enumerate() {
        if let (Some(centre), Some(at)) = (centre, at) {
            let local = [at[0] - centre[0] - geom.poff[0], at[1] - centre[1] - geom.poff[2]];
            if local[0].abs() > 50.0 || local[1].abs() > 50.0 {
                continue;
            }
        }
        println!("patch {g} poff {:?}: {} draw groups", geom.poff, geom.prmgs.len());
        for (i, p) in geom.prmgs.iter().enumerate() {
            let usages: Vec<u8> = p.decl.iter().map(|e| e.usage).collect();
            println!("  group {i}: stride {} usages {usages:?} verts {} {}", p.stride, p.vertex_count(), ranges(p, geom.poff)?);
            println!("    pass groups {:?}", p.pass_groups.iter().map(|g| (g.draw_count, g.first_draw, g.terrain_end)).collect::<Vec<_>>());
            for d in p.draws.iter().chain(&p.alt_draws) {
                println!(
                    "    draw vertex shader 0x{:08X} material {} start {} prims {} indices {}..{} word5 {}",
                    d.vertex_shader, d.material, d.start_index, d.prim_count, d.min_index, d.max_index, d.field_18
                );
            }
        }
    }
    Ok(())
}
