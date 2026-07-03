//! The streaming-world render — the game's real run path, driven **in-process**.
//!
//! `run_game_world` is the public entry point the engine binary's default boot AND `mercs2_game`
//! (the game exe) both call. It owns the wgpu window, the background WAD loader, and the per-frame
//! streaming executor (load/unload c3 cells + wake/hibernate `ModelName` props by proximity). The
//! render-coupled WAD loaders it uses on WAKE (`load_one_c3_cell`, `load_terrainmesh_tile`,
//! `load_model_by_hash`, …) live here too, plus the shared prop/interior loaders the engine binary's
//! `--world` path consumes.
//!
//! Faithful relocation of the former `main.rs` streaming render — no behaviour change.

use std::sync::Arc;

use winit::{
    event::{Event, KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

use crate::mesh::{self, Vertex};
use crate::render::{ClipAnim, LoadProgress, LoadedModel, TexMap};
use crate::wad;
use crate::worldutil::*;

// ---------------------------------------------------------------------------
//   Terrain vertices
// ---------------------------------------------------------------------------

const WORLD_MIN_M: f32 = -4000.0;
const WORLD_SPAN_M: f32 = 8000.0;

/// Map a `TerrainMesh` into engine `Vertex`es. Positions are native game-space
/// world metres (no flips). Because the source vertex UVs are not a texture
/// atlas mapping (they carry normals), synthesize a planar XZ projection over the
/// 8 km continent so the shared `vz_lrterrain` atlas lands on the terrain
/// (mirrors `terrain_extractor.py::_world_xz_to_uv`, retail V-flip). normal =
/// [0,1,0], color = white, tangent = [1,0,0,1], joints = 0, weights = [255,0,0,0]
/// (binds every vertex to identity bone 0).
pub fn terrain_to_vertices(tm: &mercs2_formats::terrain::TerrainMesh, textured: bool) -> Vec<Vertex> {
    // Real per-vertex normals (decoded from the tile verts, verified unit-length) drive terrain
    // relief shading. Fall back to up if the normals vec is short (shouldn't happen).
    let up = [0.0f32, 1.0, 0.0];
    tm.positions
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let uv = if textured {
                let u = (p[0] - WORLD_MIN_M) / WORLD_SPAN_M;
                let v = 1.0 - (p[2] - WORLD_MIN_M) / WORLD_SPAN_M; // retail V-flip
                [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]
            } else {
                [0.0, 0.0]
            };
            Vertex {
                pos: p,
                color: [1.0, 1.0, 1.0],
                uv,
                normal: tm.normals.get(i).copied().unwrap_or(up),
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
//   Render-coupled WAD loaders
// ---------------------------------------------------------------------------

/// Load one hi-res terrainmesh by its `0x7C569307` asset hash, built with POFF (16 sub-tiles) and
/// translated to its world tile position `pos`. Y is world-absolute (pos.y is 0); XZ shifts by pos.
/// Returns the placed `LoadedModel`. Textures may be empty (terrain materials live in separate
/// `terraintextures` blocks — resolved later, the splat step).
pub fn load_terrainmesh_tile(w: &mut wad::Wad, terrainmesh_hash: u32, pos: [f32; 3]) -> Option<LoadedModel> {
    let container = wad::extract_container_typed(w, terrainmesh_hash, TERRAINMESH_TYPE_HASH).ok()?;
    let (mut verts, indices, mut draws, stats) = mesh::build_indexed_from_container(&container).ok()?;
    // World-place verts + synthesize a tiled world-XZ UV (the terrainmesh has no UV; detail materials
    // tile every ~12 m via the Repeat sampler).
    const UV_SCALE: f32 = 1.0 / 12.0;
    for v in verts.iter_mut() {
        v.pos[0] += pos[0];
        v.pos[1] += pos[1];
        v.pos[2] += pos[2];
        v.uv = [v.pos[0] * UV_SCALE, v.pos[2] * UV_SCALE];
    }
    // SPLAT (first pass): bind each draw's representative detail layer (the reversed per-draw
    // material -> terraintextures layers). Full per-vertex blend of all layers by the COLOR weights
    // is the next stage; this shows the real per-region surface material.
    let layers = mercs2_formats::texture::terrain_group_layers(&container);
    if layers.len() == draws.len() {
        for (d, l) in draws.iter_mut().zip(layers.iter()) {
            // First detail (slot 2) for per-region variety; fall back to the base (slot 0).
            if let Some(&h) = l.get(2).or_else(|| l.first()) {
                d.diffuse = Some(h);
            }
            d.normal = None;
        }
    }
    // Resolve the material textures (now the terraintextures detail layers, in separate blocks).
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0;
    Some(LoadedModel { hash: terrainmesh_hash, verts, indices, draws, textures, skin, clips: Vec::new() })
}

/// Load one c3 streaming cell's `model` container by block index (the streaming executor's LOAD path).
/// Slices the `model` chunk out of the block, builds it, resolves its textures, and returns the
/// placed `LoadedModel` + the cell-origin offset (zero when the verts prove already world-space).
pub fn load_one_c3_cell(w: &mut wad::Wad, block: u16) -> Option<(LoadedModel, [f32; 3])> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let path = wad::block_paths(w).get(block as usize)?.clone();
    let cell_id = c3_cell_id_from_path(&path)?;
    let (cx, cz) = c3_cell_centre(cell_id);
    let dec = wad::decompress_block_index(w, block).ok()?;
    let (count, entries) = parse_block_entry_table(&dec);
    let mut pos = 4 + count as usize * 16;
    let mut model: Option<(u32, usize, usize)> = None;
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
            model = Some((e.name_hash, pos, end));
            break;
        }
        pos = end;
    }
    let (hash, s0, s1) = model?;
    let (verts, indices, draws, stats) = mesh::build_indexed_from_container(&dec[s0..s1]).ok()?;
    // World-space check (identical to load_c3_cells): bbox centre already inside this cell's bounds
    // => verts are world-space (identity); else cell-local (offset to the cell centre).
    let bcx = (stats.bbox_min[0] + stats.bbox_max[0]) * 0.5;
    let bcz = (stats.bbox_min[2] + stats.bbox_max[2]) * 0.5;
    let half = C3_CELL_SIZE * 0.5;
    let world_space = (bcx - cx).abs() <= half && (bcz - cz).abs() <= half;
    let offset = if world_space { [0.0, 0.0, 0.0] } else { [cx, 0.0, cz] };
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0;
    Some((LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() }, offset))
}

/// Extract one model container by hash and build its renderable `LoadedModel` (verts/tris + draws +
/// textures + skin), with `skin.center=0 / scale=1` so world placement comes purely from the entity
/// Transform. Returns the model + its local bbox. `None` if the hash has no primary ASET / fails.
pub fn load_model_by_hash(w: &mut wad::Wad, hash: u32) -> Option<(LoadedModel, [f32; 3], [f32; 3])> {
    let container = wad::extract_container(w, hash).ok()?;
    let (verts, indices, draws, stats) = mesh::build_indexed_from_container(&container).ok()?;
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0; // native metres; world placement is the authored Transform, no offset
    if std::env::var("MERCS2_TEXDBG").is_ok() {
        let n_diff = draws.iter().filter(|d| d.diffuse.is_some()).count();
        let want: std::collections::HashSet<u32> =
            draws.iter().filter_map(|d| d.diffuse).collect();
        let got = want.iter().filter(|h| textures.contains_key(h)).count();
        eprintln!(
            "[texdbg] mesh 0x{hash:08X}: {} draws ({n_diff} w/ diffuse), {} distinct diffuse hashes, {got} extracted, {} textures total",
            draws.len(), want.len(), textures.len()
        );
    }
    Some((
        LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
        stats.bbox_min,
        stats.bbox_max,
    ))
}

/// One prop instance's world transform: position + full rotation quaternion (xyzw, native game
/// space — no coordinate flip). Full quat because ~16% of props carry pitch/roll, not just yaw.
pub type PropInstance = ([f32; 3], [f32; 4]);

/// Load discrete-prop geometry from a UCFX block via the proven `ModelName` COMP recipe
/// (`mercs2_formats::placement::load_model_placements`): each `{key, model_hash}` places the
/// model at the key's `Transform`. DEDUPES by `model_hash` — each distinct container (and its
/// textures) is extracted ONCE — and collects every placement instance for that model.
///
/// When `center` is `Some(c)`, only instances within `radius` metres of `c` (XZ) are kept
/// (exterior bounding); `None` loads all (interior). `cap` bounds the number of DISTINCT meshes
/// loaded (nearest-first when a centre is given). Returns `(model_hash, LoadedModel, instances)`
/// per distinct mesh; logs distinct/placed/skipped(out-of-range)/failed counts.
pub fn load_model_props(
    w: &mut wad::Wad,
    block: &[u8],
    center: Option<[f32; 3]>,
    radius: f32,
    cap: usize,
) -> Vec<(u32, LoadedModel, Vec<PropInstance>)> {
    let placements = mercs2_formats::placement::load_model_placements(block);
    let total = placements.len();

    // Group instances by distinct model_hash, applying the radius bound (XZ) per instance.
    let mut by_model: std::collections::HashMap<u32, Vec<PropInstance>> = std::collections::HashMap::new();
    let mut skipped_range = 0usize;
    for p in &placements {
        if let Some(c) = center {
            let dx = p.pos[0] - c[0];
            let dz = p.pos[2] - c[2];
            if (dx * dx + dz * dz).sqrt() > radius {
                skipped_range += 1;
                continue;
            }
        }
        by_model.entry(p.model_hash).or_default().push((p.pos, p.quat));
    }

    // Order distinct meshes nearest-first (by their closest instance to the centre) so `cap`
    // keeps the props around the player when bounded; arbitrary order when unbounded.
    let mut distinct: Vec<(u32, Vec<PropInstance>)> = by_model.into_iter().collect();
    if let Some(c) = center {
        let near2 = |insts: &[PropInstance]| {
            insts.iter().map(|(pos, _)| {
                let dx = pos[0] - c[0];
                let dz = pos[2] - c[2];
                dx * dx + dz * dz
            }).fold(f32::INFINITY, f32::min)
        };
        distinct.sort_by(|a, b| near2(&a.1).total_cmp(&near2(&b.1)));
    }
    let distinct_in_range = distinct.len();
    let mut capped_out = 0usize;
    if distinct.len() > cap {
        capped_out = distinct.len() - cap;
        distinct.truncate(cap);
    }

    let mut out: Vec<(u32, LoadedModel, Vec<PropInstance>)> = Vec::new();
    let (mut placed_meshes, mut placed_instances, mut failed) = (0usize, 0usize, 0usize);
    for (hash, instances) in distinct {
        let container = match wad::extract_container(w, hash) {
            Ok(c) => c,
            Err(_) => { failed += 1; continue; }
        };
        let (verts, indices, draws, stats) = match mesh::build_indexed_from_container(&container) {
            Ok(v) => v,
            Err(e) => { eprintln!("[props] model 0x{hash:08X}: container parse FAILED: {e}"); failed += 1; continue; }
        };
        let mut textures: TexMap = std::collections::HashMap::new();
        for d in &draws {
            for h in [d.diffuse, d.normal].into_iter().flatten() {
                if !textures.contains_key(&h) {
                    if let Ok(t) = wad::extract_texture(w, h) {
                        textures.insert(h, t);
                    }
                }
            }
        }
        let mut skin = stats.skin_data();
        skin.center = [0.0, 0.0, 0.0];
        skin.scale = 1.0; // native metres; world placement comes from each instance Transform
        placed_meshes += 1;
        placed_instances += instances.len();
        out.push((
            hash,
            LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
            instances,
        ));
    }
    println!(
        "[props] block ModelName: {total} placements, {distinct_in_range} distinct in range (radius {}), \
         {placed_meshes} meshes placed / {placed_instances} instances, {failed} resolve failures, \
         {capped_out} meshes over cap {cap}, {skipped_range} instances out of range",
        center.map(|_| format!("{radius:.0} m")).unwrap_or_else(|| "all".into())
    );
    out
}

/// WAD block index of the PMC interior asset block (`pmc_interior_P000_Q3.block`). VERIFIED this
/// session to contain NO geometry — only FaceFX (facefxanimationset 0x665EF13E ×4 / facefxactor
/// 0x1CF649BB ×4), Scaleform UI (0xFE0E8320 ×4) and one Havok animation (0x18166555). The interior
/// GEOMETRY is authored as placed instances (real `model` blocks referenced by name) in the
/// interior STATE overlay block below.
pub const PMC_INTERIOR_ASSET_BLOCK: u16 = 3490;
/// Authored game-start spawn (MrxUtil._TeleportHero). The interior placements are already in this
/// world space (their floor sits at Y≈450.8), so loaded geometry is placed at the authored world
/// position with NO synthetic offset (matches the interior state block verbatim).
pub const PMC_INTERIOR_SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

/// Load the PMC interior for `--interior`, ASSEMBLED FROM ITS KEYED ENTITIES.
///
/// STRUCTURE (Task-1 verified, `--entity-find`): the interior is the union of the keyed
/// `pmcoutpost_interior_recruit*` meshes, placed at their AUTHORED Transforms (native game space,
/// full quat, NO bbox-guess offset). Of the 6 documented keys (`wifpmcinterior.lua` `_tBuildings` +
/// recruit starters):
///  * `recruitjet` (0x000c740d) → Transform in block 711 (vz_state_pmcinterior_jet) @ (3750,450,-3840);
///    mesh 0x86D7CF92 (name-hash `pmcoutpost_interior_recruitjet`, block 2612), 8970 v / 10735 t,
///    local-bbox already in the interior world frame (x[48.8,72.1] z[-69.7,-40.6]).
///  * `recruitmechanic` (0x000c73ee) → Transform in block 461 (…_mec) @ (3750,450,-3840); mesh
///    0xE8EB75D7 (name-hash `pmcoutpost_interior_recruitmechanic`, block 2612), 19197 v / 31726 t.
///  * `recruitheli` (0x000c73ec) → Transform in block 703 (…_hel) @ (3750,450,-3840); GAP: no mesh
///    (no `recruitheli` model ASET in vz.wad; hash 0x634F1F65 absent) — placement kept, mesh skipped.
///  * The 3 `_tBuildings` (`hq_livedin` 0x000d3c77, `hqgarage_livedin` 0x000d3c78, `hqsuites`
///    0x000cf8c2) are the EXTERIOR base buildings — their Transforms live in blocks 329/226
///    (vz_state_pmc[_livedin]) at the main-map compound (~(2540..2647, -14, -951..-1015)), NOT the
///    off-map interior cell — and have NO discrete mesh (loaded as baked exterior geometry). They are
///    deliberately NOT placed here (they belong to the exterior, ~4 km from the interior spawn).
///
/// The block-667 `ModelName` furniture (the Custom Outfit Wardrobe) is placed SEPARATELY via the
/// `interior_props` prop-instancing path in `load_world_data`. Returns (model, world pos, world quat)
/// per instance — placed verbatim, no synthetic offset. The player spawns at `PMC_INTERIOR_SPAWN`.
pub fn load_pmc_interior(w: &mut wad::Wad) -> Result<Vec<(LoadedModel, [f32; 3], [f32; 4])>, String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_formats::placement::{load_model_placements, load_placements};
    use std::collections::HashMap;

    let mut out: Vec<(LoadedModel, [f32; 3], [f32; 4])> = Vec::new();
    let (mut tv, mut tt) = (0usize, 0usize);
    let mut distinct: HashMap<u32, usize> = HashMap::new();
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];

    // The game groups the interior into the vz_state_pmcinterior blocks it loads as a layer set (base
    // + starter variants). Follow that grouping: for EVERY entity in those blocks, resolve its mesh via
    // the proven recipe — the `ModelName` COMP hash if present, else the entity name hashed
    // (`pandemic_hash_m2`; asset names drop the leading `_`) — and place it at its authored Transform.
    // No manual mesh identification: we render the block the game renders. Locators/hardpoints (no
    // mesh) simply fail to resolve and are skipped.
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
                    // asset name = the entity name minus the leading `_` and the trailing ` 0xKEY`
                    // hex-id suffix that placement Name COMPs carry ("name 0x000c740d").
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
            // Flag large / floor-like meshes (big XZ footprint) — candidates for the hall/floor shell.
            let (dx, dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
            if dx > 18.0 || dz > 18.0 {
                println!(
                    "[interior]   LARGE mesh 0x{hash:08X} '{}' {}v dims=({:.1},{:.1},{:.1}) @ ({:.1},{:.1},{:.1})",
                    p.name.as_deref().unwrap_or("?"), m.verts.len(), dx, dy, dz, p.pos[0], p.pos[1], p.pos[2]
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

    // The interior SHELL buildings + starter bays are actor meshes anchored to the HqInterior origin
    // (wifpmcinterior.lua `_tBuildings` + mrxstarter SpawnActor, anchor "HqInterior" @ (3750,450,-3840)),
    // NOT vz_state placements — so add them explicitly at that origin. The `_livedin` HQ building is the
    // enclosing MAIN HALL the player stands in: mesh `pmcoutpost_bld_hq_livedin` (0x3E629E14), whose
    // local bbox contains the player hardpoint-local (44,0.8,-71) — verified via `--pmc-shell`.
    const ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];
    const IDENT_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    // (name, model-hash), hashes verified by `mercs2_engine --pmc-shell` (pandemic_hash_m2 of the
    // wifpmcinterior building name; recruit bays keep their known placement-name hashes).
    const INTERIOR_ACTOR_MESHES: &[(&str, u32)] = &[
        ("pmcoutpost_bld_hq_livedin", 0x3E629E14),       // MAIN HALL shell — the floor/walls
        ("pmcoutpost_bld_hqgarage_livedin", 0x33AC0183), // garage room
        ("pmcoutpost_bld_hqsuites", 0xD5D65249),         // suites room
        ("recruitjet", 0x86D7CF92),                      // starter bay
        ("recruitmechanic", 0xE8EB75D7),                 // starter bay (recruitheli mesh absent)
    ];
    for &(name, hash) in INTERIOR_ACTOR_MESHES {
        if let Some((m, bmin, bmax)) = load_model_by_hash(w, hash) {
            for c in 0..3 {
                wmin[c] = wmin[c].min(ACTOR_ORIGIN[c] + bmin[c]);
                wmax[c] = wmax[c].max(ACTOR_ORIGIN[c] + bmax[c]);
            }
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            *distinct.entry(hash).or_insert(0) += 1;
            println!(
                "[interior] actor mesh '{name}' 0x{hash:08X}: {} v / {} t @ actor-origin",
                m.verts.len(),
                m.indices.len() / 3
            );
            out.push((m, ACTOR_ORIGIN, IDENT_QUAT));
        } else {
            println!("[interior] actor mesh '{name}' 0x{hash:08X}: NOT FOUND in vz.wad");
        }
    }

    println!(
        "[interior] assembled {} instance(s) ({} distinct meshes), {tv} verts / {tt} tris; spawn @ ({:.1},{:.1},{:.1})",
        out.len(), distinct.len(), PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2]
    );
    if !out.is_empty() {
        println!(
            "[interior] WORLD BBOX min=({:.1},{:.1},{:.1}) max=({:.1},{:.1},{:.1}) center=({:.1},{:.1},{:.1}) dims=({:.1},{:.1},{:.1})",
            wmin[0], wmin[1], wmin[2], wmax[0], wmax[1], wmax[2],
            (wmin[0]+wmax[0])/2.0, (wmin[1]+wmax[1])/2.0, (wmin[2]+wmax[2])/2.0,
            wmax[0]-wmin[0], wmax[1]-wmin[1], wmax[2]-wmin[2]
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
//   Animation clip loaders
// ---------------------------------------------------------------------------

/// Find the animgroup whose binding best covers this model's HIER, decode a clip, and bind its
/// tracks to HIER bones. `want` selects a specific clip by name-hash; otherwise a normal fully-mapped
/// body clip is chosen (≤70 tracks — the 105-track full-body/reference clip is a special case that
/// over-poses a single body, so it's not the default).
pub fn load_clip_for_rig(w: &mut wad::Wad, hier: &[u32], want: Option<u32>) -> Option<ClipAnim> {
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best: Option<(u16, u32, usize, u32)> = None; // (block, clip_hash, resolved, tracks)
    for blk in wad::animgroup_blocks(w) {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            if let Some(h) = want {
                if c.name_hash != h {
                    continue;
                }
            }
            let resolved = c.binding.resolve_to_hier(hier).iter().filter(|r| r.is_some()).count();
            if resolved == 0 && want.is_none() {
                continue; // clip drives no bone of this model
            }
            let normal = c.num_transform_tracks <= 70; // exclude the 105-track special clip
            let better = match best {
                None => true,
                Some((_, _, r, _)) if want.is_some() => resolved > r,
                Some((_, _, r, t)) => {
                    let best_normal = t <= 70;
                    if normal != best_normal { normal } else { resolved > r }
                }
            };
            if better {
                best = Some((blk, c.name_hash, resolved, c.num_transform_tracks));
            }
        }
    }
    let (blk, clip_hash, _, _) = best?;
    // Pass 2: decode it.
    let data = wad::decompress_block_index(w, blk).ok()?;
    let ag = parse_animgroup(&data).ok()?;
    let c = ag.clips.iter().find(|c| c.name_hash == clip_hash)?;
    let clip = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok()?;
    if !clip.decoded {
        return None; // e.g. a delta clip (header-only) — leave synthetic driver in place
    }
    let track_to_hier = c.binding.resolve_to_hier(hier);
    Some(ClipAnim {
        clip,
        track_to_hier,
        num_transform_tracks: c.num_transform_tracks as usize,
        name_hash: clip_hash,
    })
}

/// Load SEVERAL clips by name-hash in ONE pass over the animgroup blocks (each block is
/// decompressed + parsed once, vs once per clip via `load_clip_for_rig` — the world load was
/// spending ~2/3 of its 20 s on the repeated scans). Same per-want selection rule as
/// `load_clip_for_rig` with `want = Some(h)`: best = most tracks resolved to this HIER.
pub fn load_clips_for_rig(w: &mut wad::Wad, hier: &[u32], wants: &[u32]) -> Vec<Option<ClipAnim>> {
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best: Vec<Option<(u16, usize)>> = vec![None; wants.len()]; // (block, resolved)
    for blk in wad::animgroup_blocks(w) {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            for (i, &h) in wants.iter().enumerate() {
                if c.name_hash != h {
                    continue;
                }
                let resolved = c.binding.resolve_to_hier(hier).iter().filter(|r| r.is_some()).count();
                if best[i].map_or(true, |(_, r)| resolved > r) {
                    best[i] = Some((blk, resolved));
                }
            }
        }
    }
    // Decode pass: only the chosen blocks (cached so a shared block decompresses once).
    let mut cache: std::collections::HashMap<u16, Vec<u8>> = std::collections::HashMap::new();
    wants
        .iter()
        .zip(best)
        .map(|(&h, b)| {
            let (blk, _) = b?;
            if !cache.contains_key(&blk) {
                cache.insert(blk, wad::decompress_block_index(w, blk).ok()?);
            }
            let data = cache.get(&blk)?;
            let ag = parse_animgroup(data).ok()?;
            let c = ag.clips.iter().find(|c| c.name_hash == h)?;
            let clip = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok()?;
            if !clip.decoded {
                return None;
            }
            Some(ClipAnim {
                clip,
                track_to_hier: c.binding.resolve_to_hier(hier),
                num_transform_tracks: c.num_transform_tracks as usize,
                name_hash: h,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
//   Streaming world
// ---------------------------------------------------------------------------

/// Everything the streaming runtime needs, produced on the background loader thread (all `Send`):
/// the WAD handle (moved to the render thread for on-demand wake extraction), the base terrain, its
/// heightmap, the pure streaming decision manager (blocks + per-entity props with hibernation), and
/// the key->spawn recipe map the executor uses on WAKE.
struct StreamingWorldData {
    wad: wad::Wad,
    terrain: LoadedModel,
    manager: mercs2_core::streaming::StreamingManager,
    props: std::collections::HashMap<u32, PropSpawn>,
    terrain_tiles: std::collections::HashMap<u32, (u32, [f32; 3])>,
    /// Low-res terrain grid cell (row*20+col) -> its draw-group index in the `terrain` model, so the
    /// executor can hide that tile when the hi-res terrainmesh at the same cell is resident.
    lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
}

/// Load the streaming world off-thread: open the WAD, merge the base terrain, build the world block
/// index + Layer-2 streaming catalog (c3-cell LOAD units + per-entity `ModelName` props with their
/// `HibernationControl` distances). Returns the data (incl. the WAD handle) for the render thread.
fn load_streaming_world_data(
    wadpath: &str,
    cfg: mercs2_core::streaming::StreamingConfig,
    overlays: &[String],
    progress: &LoadProgress,
) -> Result<StreamingWorldData, String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;
    progress.step("blocks");

    // Base terrain (the bottom LOD rung — one merged mesh, always present).
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
    let textured = tm.texture.is_some();
    let verts = terrain_to_vertices(&tm, textured);
    let mut textures: TexMap = std::collections::HashMap::new();
    let diffuse = if let Some(t) = tm.texture.clone() {
        textures.insert(0, t);
        Some(0)
    } else {
        None
    };
    // One draw group PER TILE (all sharing the atlas view), so a low-res tile can be hidden when its
    // hi-res terrainmesh is resident. `lowres_draw_by_cell[cell] = draw index` maps the 20x20 grid.
    let mut draws = Vec::with_capacity(tm.tile_draws.len());
    let mut lowres_draw_by_cell: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, &(cell, start, count)) in tm.tile_draws.iter().enumerate() {
        draws.push(mesh::DrawGroup { index_start: start, index_count: count, diffuse, normal: None });
        lowres_draw_by_cell.insert(cell, i);
    }
    let terrain = LoadedModel {
        hash: 0x7E44_A100,
        verts,
        indices: tm.indices.clone(),
        draws,
        textures,
        skin: mesh::SkinData::identity(),
        clips: Vec::new(),
    };
    println!("[stream] terrain: {} verts / {} tris / {} tiles", terrain.verts.len(), terrain.indices.len() / 3, tm.tiles_placed);
    progress.step("terrain");

    // World block index (c3-cell extents) + the streaming catalog.
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    progress.step("world index");
    let (mut manager, mut props, terrain_tiles) = build_streaming_catalog(&idx, &ls, cfg);
    let base_props = props.len();

    // Activate the save's vz_state OVERLAYS on top of the always-loaded base: resolve each active
    // layer name -> its WAD block, then fold its placements into the same streaming catalog so they
    // stream by proximity like the base. This is the game-state world (destruction/staging/faction +
    // the PMC interior) the save selects. Empty `overlays` = base world only.
    let (mut ov_resolved, mut ov_entities) = (0usize, 0usize);
    for layer in overlays {
        let Some(bi) = resolve_overlay_block(&w, layer) else { continue };
        let Ok(dec) = wad::decompress_block_index(&mut w, bi) else { continue };
        let (mn, nm) = add_overlay_to_catalog(&dec, cfg.default_distances, &mut manager, &mut props);
        ov_resolved += 1;
        ov_entities += mn + nm;
    }
    if !overlays.is_empty() {
        println!("[stream] overlays: {ov_resolved}/{} vz_state layers resolved, +{ov_entities} entities", overlays.len());
    }

    println!(
        "[stream] catalog: {} c3-cell blocks, {} per-entity props ({base_props} base + {} overlay), {} hi-res terrain tiles",
        manager.block_count(), props.len(), props.len() - base_props, terrain_tiles.len()
    );
    progress.step("streaming catalog");

    Ok(StreamingWorldData { wad: w, terrain, manager, props, terrain_tiles, lowres_draw_by_cell })
}

/// The control-driven streaming world with a free-fly camera (the no-arg default boot; also
/// `--stream`). Mirrors the original engine's ONE streaming system (spec §10): a background loader
/// builds the block index + Layer-2 decision catalog, then each frame the pure `StreamingManager`
/// turns the camera position into a load/unload/wake/hibernate diff, and this executor performs the
/// GPU work — LOAD c3-cell geometry + WAKE `ModelName` props (via the proven recipes), and the
/// net-new UNLOAD path (despawn + free GPU). Free-fly camera reuses the Shadow-PC dual-source mouse
/// input (CursorMoved+recentre fallback, never DeviceEvent on absolute-coordinate streams).
///
/// This is the public in-process render entry point: the engine binary's default boot and
/// `mercs2_game` (the game exe) both `pollster::block_on(run_game_world(..))`. `spawn` is the
/// authored start position (`mercs2_game` passes the authentic PMC-interior start); `overlays` are
/// the active vz_state layer names the save selects.
pub async fn run_game_world(wadpath: String, spawn: Option<[f32; 3]>, overlays: Vec<String>) {
    use crate::scene::Scene;
    use mercs2_core::glam::{Mat4, Quat, Vec3};
    use mercs2_core::streaming::StreamingConfig;
    use mercs2_core::{AnimState, Entity, ModelRef, SkinPalette, Transform, World};
    use std::collections::{HashMap, HashSet};
    use std::f32::consts::PI;
    use winit::event::{DeviceEvent, ElementState};
    use winit::window::CursorGrabMode;

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    // Runtime config: tighter per-frame budgets than the probe so wake/load disk I/O (container +
    // texture extraction) doesn't stall a frame; proximity radii are generous for an aerial cam.
    match spawn {
        Some(s) => eprintln!("[boot] spawn = ({:.1}, {:.1}, {:.1})  <- from --spawn (PMC interior via mercs2_game)", s[0], s[1], s[2]),
        None => eprintln!("[boot] spawn = DEFAULT exterior bird's-eye (NO --spawn received)"),
    }
    eprintln!("[boot] {} vz_state overlay layer(s) requested via --overlays", overlays.len());

    let cfg = StreamingConfig {
        block_unload_margin: 200.0,
        block_budget: 2,
        entity_budget: 6,
        entity_hysteresis: 15.0,
        entity_scan_cap: 700.0,
        grid_cell: 128.0,
        ..StreamingConfig::default()
    };

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Mercenaries 2 — streaming world (free-fly)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );
    if let Err(e) = window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
    {
        eprintln!("[stream] cursor grab unavailable ({e}); arrow keys still steer");
    }
    window.set_cursor_visible(false);
    let mut scene = Scene::new(window.clone()).await;
    scene.set_fog([0.55, 0.62, 0.70], 0.00016, 60.0);
    match wad::shell_loading_plate(&wadpath) {
        Ok(td) => scene.set_loading_art(&td),
        Err(e) => eprintln!("[stream] loading art unavailable ({e}); spinner only"),
    }

    // Background loader.
    let (tx, rx) = std::sync::mpsc::channel::<Result<StreamingWorldData, String>>();
    let progress = Arc::new(LoadProgress::new(4));
    let loader_progress = progress.clone();
    let loader_wadpath = wadpath.clone();
    let loader_overlays = overlays;
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = load_streaming_world_data(&loader_wadpath, cfg, &loader_overlays, &loader_progress);
        if r.is_ok() {
            println!("[stream] loaded in {:.1}s", t0.elapsed().as_secs_f64());
        }
        let _ = tx.send(r);
    });

    let mut world = World::new();
    // Streaming state, wired in on loader completion.
    let mut wad_opt: Option<wad::Wad> = None;
    let mut manager: Option<mercs2_core::streaming::StreamingManager> = None;
    let mut props: HashMap<u32, PropSpawn> = HashMap::new();
    let mut terrain_tiles: HashMap<u32, (u32, [f32; 3])> = HashMap::new(); // key -> (terrainmesh hash, pos)
    let mut lowres_draw_by_cell: HashMap<usize, usize> = HashMap::new(); // grid cell -> low-res draw idx
    let mut terrain_hash: u32 = 0; // the low-res terrain model hash (for the tile LOD swap)
    // Map a world XZ to the 20x20 low-res grid cell (row*20+col); tiles are 400 m from -3800.
    let pos_to_cell = |p: [f32; 3]| -> Option<usize> {
        let col = ((p[0] + 3800.0) / 400.0).round() as i32;
        let row = ((p[2] + 3800.0) / 400.0).round() as i32;
        (0..20).contains(&col).then(|| ())?;
        (0..20).contains(&row).then(|| ())?;
        Some(row as usize * 20 + col as usize)
    };
    // Live executor bookkeeping.
    let mut prop_ents: HashMap<u32, Entity> = HashMap::new(); // entity key -> ECS entity
    let mut block_ents: HashMap<u16, Entity> = HashMap::new(); // c3 block -> ECS entity
    let mut model_refs: HashMap<u32, u32> = HashMap::new(); // model hash -> live entity count
    let mut wake_failed: HashSet<u32> = HashSet::new(); // keys whose mesh wouldn't resolve (logged once)

    // Free-fly camera. Start over the PMC exterior spawn at a moderate height so nearby cells +
    // props stream in immediately; WASDQE + mouse-look fly around.
    // Free-fly camera start: the authored spawn if given (mercs2_game passes the authentic
    // PMC-interior start), else an elevated bird's-eye over the exterior pool for free exploration.
    let mut free_pos = match spawn {
        Some(s) => Vec3::new(s[0], s[1], s[2]),
        None => Vec3::new(EXTERIOR_SPAWN[0], 140.0, EXTERIOR_SPAWN[2]),
    };
    // Initial heading: at a provided spawn (the PMC interior) face +Z, level — the room extends +Z
    // from the spawn's near edge, so you look INTO it rather than at the wall behind. The exterior
    // default is a downward bird's-eye facing -Z over the pool.
    let (mut free_yaw, mut free_pitch): (f32, f32) = match spawn {
        Some(_) => (0.0, 0.0),
        None => (PI, -0.35),
    };
    let mut held: HashSet<KeyCode> = HashSet::new();
    let mut loading = true;
    let load_start = std::time::Instant::now();
    let mut bar_shown = 0.0f32;
    let mut bar_last_t = 0.0f32;
    let mut last = std::time::Instant::now();
    let mut mouse_acc: (f32, f32) = (0.0, 0.0);
    let mut mouse_raw_acc: (f32, f32) = (0.0, 0.0);
    let mut mouse_src: u8 = 0;
    let mut mouse_sane_events: u32 = 0;
    let mut stat_last = std::time::Instant::now();

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. },
                    ..
                } => match (code, state) {
                    (KeyCode::Escape, _) => elwt.exit(),
                    (c, ElementState::Pressed) => { held.insert(c); }
                    (c, ElementState::Released) => { held.remove(&c); }
                },
                WindowEvent::Resized(size) => scene.resize(size),
                WindowEvent::CursorMoved { position, .. } => {
                    let (cx, cy) = (scene.size.width as f64 / 2.0, scene.size.height as f64 / 2.0);
                    mouse_acc.0 += (position.x - cx) as f32;
                    mouse_acc.1 += (position.y - cy) as f32;
                    let _ = scene.window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                }
                WindowEvent::RedrawRequested => {
                    if loading {
                        match rx.try_recv() {
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                let t = load_start.elapsed().as_secs_f32();
                                let dt = (t - bar_last_t).max(0.0);
                                bar_last_t = t;
                                bar_shown += (progress.fraction() - bar_shown) * (1.0 - (-6.0 * dt).exp());
                                match scene.render_loading(t, bar_shown) {
                                    Ok(()) => {}
                                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                    Err(e) => eprintln!("surface error: {e:?}"),
                                }
                                return;
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                eprintln!("[stream] loader thread died"); elwt.exit(); return;
                            }
                            Ok(Err(e)) => { eprintln!("[stream] load failed: {e}"); elwt.exit(); return; }
                            Ok(Ok(mut data)) => {
                                // Base terrain: one static entity at identity (verts already world-space).
                                let terrain = data.terrain;
                                terrain_hash = terrain.hash;
                                scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
                                world.spawn((
                                    Transform::IDENTITY,
                                    ModelRef { model: terrain.hash },
                                    AnimState::default(),
                                    SkinPalette { mats: vec![IDENTITY] },
                                ));
                                // PMC interior, DRIVEN BY THE GAME'S LUA: run the authentic
                                // MrxUtil.SpawnActor body (Pg.Spawn + Object.*) through mercs2_script.
                                // The interior spawns because the script asked for it — not a hardcoded
                                // engine call. The engine then realizes each returned actor intent by
                                // resolving its template -> geometry.
                                let interior_intents = crate::script_host::run_interior_boot();
                                for r in &interior_intents {
                                    println!(
                                        "[lua] Pg.Spawn '{}' (name={}) at ({:.1},{:.1},{:.1}) -> guid 0x{:x}",
                                        r.template, r.name, r.pos[0], r.pos[1], r.pos[2], r.guid
                                    );
                                }
                                // Engine template resolver: PMC_INTERIOR_TEMPLATE -> the PMC interior
                                // geometry. Shells (`pmcoutpost_bld_*`) load by PATH (never via the
                                // streaming name-hash wake recipe), so load_pmc_interior IS the resolver
                                // body for this template. The enclosing hall SHELL mesh is the open
                                // sub-problem that lives right here, inside this resolution.
                                let want_interior = interior_intents.iter().any(|r| {
                                    r.template.eq_ignore_ascii_case(crate::script_host::PMC_INTERIOR_TEMPLATE)
                                });
                                if want_interior {
                                    match load_pmc_interior(&mut data.wad) {
                                        Ok(pieces) => {
                                            let n = pieces.len();
                                            for (m, pos, quat) in pieces {
                                                if !scene.has_model(m.hash) {
                                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                                }
                                                // Palette needs ONE identity per bone (like the WAKE path) — a
                                                // 1-entry palette on a multi-bone mesh under-runs the skin buffer
                                                // and flings the verts off-screen (why the room was invisible).
                                                let nbones = scene.model_bone_count(m.hash).max(1);
                                                world.spawn((
                                                    Transform {
                                                        translation: Vec3::new(pos[0], pos[1], pos[2]),
                                                        rotation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]),
                                                        scale: Vec3::ONE,
                                                    },
                                                    ModelRef { model: m.hash },
                                                    AnimState::default(),
                                                    SkinPalette { mats: vec![IDENTITY; nbones] },
                                                ));
                                            }
                                            println!(
                                                "[stream] PMC interior (Lua-driven, template '{}'): {n} pieces placed",
                                                crate::script_host::PMC_INTERIOR_TEMPLATE
                                            );
                                        }
                                        Err(e) => eprintln!("[stream] PMC interior load failed: {e}"),
                                    }
                                }
                                wad_opt = Some(data.wad);
                                manager = Some(data.manager);
                                props = data.props;
                                terrain_tiles = data.terrain_tiles;
                                lowres_draw_by_cell = data.lowres_draw_by_cell;
                                loading = false;
                            }
                        }
                    }

                    let now = std::time::Instant::now();
                    let dt = (now - last).as_secs_f32().min(0.1);
                    last = now;
                    let look = 1.6 * dt;

                    // --- mouse-look (dual-source; see run_scene_world_loading) ---
                    const MOUSE_SENS: f32 = 0.0008;
                    let src = if mouse_src == 1 { mouse_raw_acc } else { mouse_acc };
                    let mdx = src.0.clamp(-80.0, 80.0) * MOUSE_SENS;
                    let mdy = src.1.clamp(-80.0, 80.0) * MOUSE_SENS;
                    mouse_acc = (0.0, 0.0);
                    mouse_raw_acc = (0.0, 0.0);
                    free_yaw += mdx;
                    free_pitch = (free_pitch - mdy).clamp(-1.5, 1.5);

                    // --- free-fly movement ---
                    if held.contains(&KeyCode::ArrowUp) { free_pitch += look; }
                    if held.contains(&KeyCode::ArrowDown) { free_pitch -= look; }
                    if held.contains(&KeyCode::ArrowLeft) { free_yaw -= look; }
                    if held.contains(&KeyCode::ArrowRight) { free_yaw += look; }
                    free_pitch = free_pitch.clamp(-1.5, 1.5);
                    let fwd = Vec3::new(free_pitch.cos() * free_yaw.sin(), free_pitch.sin(), free_pitch.cos() * free_yaw.cos()).normalize();
                    let right = Vec3::Y.cross(fwd).normalize();
                    let mut mv = Vec3::ZERO;
                    if held.contains(&KeyCode::KeyW) { mv += fwd; }
                    if held.contains(&KeyCode::KeyS) { mv -= fwd; }
                    if held.contains(&KeyCode::KeyD) { mv += right; }
                    if held.contains(&KeyCode::KeyA) { mv -= right; }
                    if held.contains(&KeyCode::KeyE) { mv += Vec3::Y; }
                    if held.contains(&KeyCode::KeyQ) { mv -= Vec3::Y; }
                    let sp = if held.contains(&KeyCode::ShiftLeft) { 900.0 } else { 260.0 };
                    if mv != Vec3::ZERO { free_pos += mv.normalize() * sp * dt; }
                    let view = Mat4::look_to_lh(free_pos, fwd, Vec3::Y);

                    // --- streaming tick: decide, then execute the diff on the GPU/ECS ---
                    if let (Some(mgr), Some(w)) = (manager.as_mut(), wad_opt.as_mut()) {
                        let diff = mgr.update([free_pos.x, free_pos.y, free_pos.z]);

                        // UNLOAD first (free GPU): blocks that left the working radius.
                        for b in &diff.unload_blocks {
                            if let Some(e) = block_ents.remove(b) {
                                if let Ok(mr) = world.get::<&ModelRef>(e).map(|m| m.model) {
                                    dec_model_ref(&mut model_refs, mr, &mut scene);
                                }
                                let _ = world.despawn(e);
                                scene.forget_entity(e);
                            }
                        }
                        // HIBERNATE (free GPU): props beyond their stream-out distance.
                        for k in &diff.hibernate {
                            // If a hi-res terrain tile hibernates, un-hide its low-res tile again.
                            if let Some(&(_, pos)) = terrain_tiles.get(k) {
                                if let Some(di) = pos_to_cell(pos).and_then(|c| lowres_draw_by_cell.get(&c)) {
                                    scene.set_draw_hidden(terrain_hash, *di, false);
                                }
                            }
                            if let Some(e) = prop_ents.remove(k) {
                                if let Ok(mr) = world.get::<&ModelRef>(e).map(|m| m.model) {
                                    dec_model_ref(&mut model_refs, mr, &mut scene);
                                }
                                let _ = world.despawn(e);
                                scene.forget_entity(e);
                            }
                        }
                        // LOAD c3-cell blocks (throttled by the manager's block budget).
                        for b in &diff.load_blocks {
                            if block_ents.contains_key(b) { continue; }
                            if let Some((m, off)) = load_one_c3_cell(w, *b) {
                                if !scene.has_model(m.hash) {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                }
                                let e = world.spawn((
                                    Transform::from_translation(Vec3::new(off[0], off[1], off[2])),
                                    ModelRef { model: m.hash },
                                    AnimState::default(),
                                    SkinPalette { mats: vec![IDENTITY] },
                                ));
                                *model_refs.entry(m.hash).or_insert(0) += 1;
                                block_ents.insert(*b, e);
                            }
                        }
                        // WAKE props (throttled by the manager's entity budget): instantiate the
                        // ModelName mesh at the authored Transform (identity fit + bone-count palette).
                        for k in &diff.wake {
                            if prop_ents.contains_key(k) { continue; }
                            // Hi-res terrain tile? Load the terrainmesh (POFF-composed, world-placed via
                            // TerrainObject->Transform) and spawn at identity (verts already world-space).
                            if let Some(&(tm_hash, pos)) = terrain_tiles.get(k) {
                                if !scene.has_model(tm_hash) {
                                    match load_terrainmesh_tile(w, tm_hash, pos) {
                                        Some(m) => scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin),
                                        None => { wake_failed.insert(*k); continue; }
                                    }
                                }
                                let e = world.spawn((
                                    Transform::IDENTITY,
                                    ModelRef { model: tm_hash },
                                    AnimState::default(),
                                    SkinPalette { mats: vec![IDENTITY] },
                                ));
                                *model_refs.entry(tm_hash).or_insert(0) += 1;
                                prop_ents.insert(*k, e);
                                // Hide the low-res tile beneath this hi-res tile (the LOD swap).
                                if let Some(di) = pos_to_cell(pos).and_then(|c| lowres_draw_by_cell.get(&c)) {
                                    scene.set_draw_hidden(terrain_hash, *di, true);
                                }
                                continue;
                            }
                            let Some(spawn) = props.get(k).copied() else { continue };
                            if !scene.has_model(spawn.model_hash) {
                                match load_model_by_hash(w, spawn.model_hash) {
                                    Some((m, _, _)) => {
                                        scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    }
                                    None => {
                                        if wake_failed.insert(*k) {
                                            // Mesh hash has no primary model ASET (the documented ~10/465 gap).
                                        }
                                        continue;
                                    }
                                }
                            }
                            let nbones = scene.model_bone_count(spawn.model_hash).max(1);
                            let mut t = Transform::from_translation(Vec3::new(spawn.pos[0], spawn.pos[1], spawn.pos[2]));
                            t.rotation = Quat::from_xyzw(spawn.quat[0], spawn.quat[1], spawn.quat[2], spawn.quat[3]);
                            let e = world.spawn((
                                t,
                                ModelRef { model: spawn.model_hash },
                                AnimState::default(),
                                SkinPalette { mats: vec![IDENTITY; nbones] },
                            ));
                            *model_refs.entry(spawn.model_hash).or_insert(0) += 1;
                            prop_ents.insert(*k, e);
                        }
                        // Each geometry block streams independently by its own tier-scaled distance
                        // (per-object; the c3 chain is a size-keyed spatial index, not LOD levels).
                        // diff.tier_changes carries the per-PROP hibernation LOD tier — informational
                        // only; props don't ship alternate-LOD meshes (verified --lod-probe: 2/446).

                        // Periodic streaming stats to the console (proof the runtime is live).
                        if stat_last.elapsed().as_secs_f32() >= 1.0 {
                            stat_last = std::time::Instant::now();
                            println!(
                                "[stream] cam({:.0},{:.0},{:.0}) resident={} awake={} | live_blk_ents={} props={} models={}",
                                free_pos.x, free_pos.y, free_pos.z,
                                mgr.resident_count(), mgr.awake_count(),
                                block_ents.len(), prop_ents.len(), model_refs.len()
                            );
                        }
                    }

                    scene.set_view(view, 0.5, 30000.0);
                    match scene.render(&world) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                        Err(e) => eprintln!("surface error: {e:?}"),
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                let (dx, dy) = (delta.0 as f32, delta.1 as f32);
                if mouse_src != 2 {
                    if dx.abs() > 2000.0 || dy.abs() > 2000.0 {
                        mouse_src = 2;
                        eprintln!("[stream] absolute-coordinate raw input detected -> cursor-recentre mode");
                    } else {
                        mouse_raw_acc.0 += dx;
                        mouse_raw_acc.1 += dy;
                        if mouse_src == 0 && (dx != 0.0 || dy != 0.0) {
                            mouse_sane_events += 1;
                            if mouse_sane_events >= 10 { mouse_src = 1; }
                        }
                    }
                }
            }
            Event::AboutToWait => scene.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}

/// Decrement a model's live-reference count; free its GPU resources when it reaches zero (net-new
/// UNLOAD path — nothing freed GPU before the streaming runtime). Shared meshes stay resident until
/// the last referencing entity hibernates/unloads.
fn dec_model_ref(refs: &mut std::collections::HashMap<u32, u32>, hash: u32, scene: &mut crate::scene::Scene) {
    if let Some(c) = refs.get_mut(&hash) {
        *c = c.saturating_sub(1);
        if *c == 0 {
            refs.remove(&hash);
            scene.unload_model(hash);
        }
    }
}
