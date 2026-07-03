//! GAME-specific PMC HQ interior assembly.
//!
//! This is Mercenaries-specific content — where the PMC interior is, which blocks/meshes make it up,
//! the destruction-state tiers, the floor heights — so it lives in the GAME, not the engine. It calls
//! the engine's asset-agnostic public API (`mercs2_engine::wad` / `game_world::load_model_by_hash*` /
//! `render::LoadedModel`) to turn WAD data into renderable meshes, then the game spawns them.

use std::collections::HashMap;

use mercs2_engine::game_world::{load_model_by_hash, load_model_by_hash_state};
use mercs2_engine::render::LoadedModel;
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::placement::{load_model_placements, load_placements};

// (The PMC interior asset block `pmc_interior_P000_Q3.block` = 3490 is FaceFX/Scaleform only, no
// geometry — the renderable interior is placed instances in the state overlay blocks 667/711/461/703.)

/// Authored game-start spawn (`MrxUtil._TeleportHero`). The interior placements are already in this
/// world space (their floor sits at Y≈450.8), so loaded geometry is placed at the authored world
/// position with NO synthetic offset.
pub const PMC_INTERIOR_SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

/// Assemble the PMC HQ interior from its keyed entities: the block-{667,711,461,703} `ModelName` /
/// name-hash furniture placed at authored Transforms, plus the actor-anchored shell buildings + recruit
/// bays. Returns (model, world pos, world quat) per instance. See the memory `pmc-teleport-coords-and-
/// interior` for the full derivation (hall shell = `pmcoutpost_bld_hq_livedin` 0x3E629E14, intact tier
/// mask 0x04, floor seated at the hero-feet Y).
pub fn load_pmc_interior(w: &mut wad::Wad) -> Result<Vec<(LoadedModel, [f32; 3], [f32; 4])>, String> {
    let mut out: Vec<(LoadedModel, [f32; 3], [f32; 4])> = Vec::new();
    let (mut tv, mut tt) = (0usize, 0usize);
    let mut distinct: HashMap<u32, usize> = HashMap::new();
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];

    // Furniture: for EVERY entity in the interior state blocks, resolve its mesh via the ModelName COMP
    // hash if present, else the entity name hashed (asset names drop the leading `_` and ` 0xKEY`
    // suffix), and place it at its authored Transform. Locators/hardpoints (no mesh) simply skip.
    let (mut furn_ymin, mut furn_ymax) = (f32::MAX, f32::MIN);
    const INTERIOR_STATE_BLOCKS: &[u16] = &[667, 711, 461, 703]; // base + jet + mec + hel variants
    for &blk in INTERIOR_STATE_BLOCKS {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let model_by_key: HashMap<u32, u32> = load_model_placements(&data)
            .into_iter()
            .map(|mp| (mp.key, mp.model_hash))
            .collect();
        let placements = load_placements(&data).unwrap_or_default();
        let mut resolved = 0usize;
        for p in &placements {
            let hash = model_by_key.get(&p.key).copied().or_else(|| {
                p.name.as_deref().map(|n| {
                    let base = n.split(" 0x").next().unwrap_or(n).trim_start_matches('_');
                    pandemic_hash_m2(base)
                })
            });
            let Some(hash) = hash else { continue };
            let Some((m, bmin, bmax)) = load_model_by_hash(w, hash) else { continue };
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            for c in 0..3 {
                wmin[c] = wmin[c].min(p.pos[c] + bmin[c]);
                wmax[c] = wmax[c].max(p.pos[c] + bmax[c]);
            }
            furn_ymin = furn_ymin.min(p.pos[1] + bmin[1]);
            furn_ymax = furn_ymax.max(p.pos[1] + bmin[1]);
            if std::env::var("MERCS2_FURNDBG").is_ok() {
                println!(
                    "[furn] blk{blk} '{}' floor Y {:.2} (pos {:.2}, mesh bmin {:.2})",
                    p.name.as_deref().unwrap_or("?"), p.pos[1] + bmin[1], p.pos[1], bmin[1]
                );
            }
            let (dx, _dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
            if dx > 18.0 || dz > 18.0 {
                println!(
                    "[interior]   LARGE mesh 0x{hash:08X} '{}' {}v @ ({:.1},{:.1},{:.1})",
                    p.name.as_deref().unwrap_or("?"), m.verts.len(), p.pos[0], p.pos[1], p.pos[2]
                );
            }
            *distinct.entry(hash).or_insert(0) += 1;
            resolved += 1;
            out.push((m, p.pos, p.quat));
        }
        println!(
            "[interior] block {blk}: {} transforms, {} ModelName, {resolved} resolved to a mesh",
            placements.len(), model_by_key.len()
        );
    }

    // Actor-anchored shell buildings + recruit bays (wifpmcinterior.lua `_tBuildings` + mrxstarter
    // SpawnActor @ (3750,450,-3840)). The `_livedin` HQ building is the enclosing MAIN HALL; it renders
    // its INTACT destruction tier (mask 0x04; 0x03 = ruined). The hall + suites are seated at the hero
    // floor Y (450.75, where the block-667 furniture sits); the bays/garage keep the actor anchor.
    const ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];
    const IDENT_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    let hall_origin = [ACTOR_ORIGIN[0], PMC_INTERIOR_SPAWN[1], ACTOR_ORIGIN[2]]; // Y = 450.75
    let interior_actor_meshes: &[(&str, u32, u8, [f32; 3])] = &[
        ("pmcoutpost_bld_hq_livedin", 0x3E629E14, 0x04, hall_origin), // MAIN HALL — floor at hero level
        ("pmcoutpost_bld_hqsuites", 0xD5D65249, 0x04, hall_origin),   // suites room (hall level)
        ("pmcoutpost_bld_hqgarage_livedin", 0x33AC0183, 0x04, ACTOR_ORIGIN), // garage (own level)
        ("recruitjet", 0x86D7CF92, 0x01, ACTOR_ORIGIN),              // starter bay (own floor ≈458)
        ("recruitmechanic", 0xE8EB75D7, 0x01, ACTOR_ORIGIN),         // starter bay (recruitheli absent)
    ];
    for &(name, hash, state_bit, pos) in interior_actor_meshes {
        if let Some((m, bmin, bmax)) = load_model_by_hash_state(w, hash, state_bit) {
            for c in 0..3 {
                wmin[c] = wmin[c].min(pos[c] + bmin[c]);
                wmax[c] = wmax[c].max(pos[c] + bmax[c]);
            }
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            *distinct.entry(hash).or_insert(0) += 1;
            println!(
                "[interior] actor mesh '{name}' 0x{hash:08X}: {} v / {} t @ ({:.2},{:.2},{:.2}); FLOOR Y {:.2}",
                m.verts.len(), m.indices.len() / 3, pos[0], pos[1], pos[2], pos[1] + bmin[1]
            );
            out.push((m, pos, IDENT_QUAT));
        } else {
            println!("[interior] actor mesh '{name}' 0x{hash:08X}: NOT FOUND in vz.wad");
        }
    }

    println!(
        "[interior] assembled {} instance(s) ({} distinct meshes), {tv} verts / {tt} tris",
        out.len(), distinct.len()
    );
    if furn_ymin <= furn_ymax {
        println!(
            "[interior] FLOOR CHECK: furniture bottoms Y {:.2}..{:.2}; hero feet Y {:.2}",
            furn_ymin, furn_ymax, PMC_INTERIOR_SPAWN[1]
        );
    }
    if !out.is_empty() {
        println!(
            "[interior] WORLD BBOX min=({:.1},{:.1},{:.1}) max=({:.1},{:.1},{:.1})",
            wmin[0], wmin[1], wmin[2], wmax[0], wmax[1], wmax[2]
        );
    }
    Ok(out)
}
