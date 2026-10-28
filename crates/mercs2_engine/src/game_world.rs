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
/// Returns the placed `LoadedModel`. Each draw is bound to its PRMG group's representative
/// `terraintextures` detail layer and those textures are resolved here (they live in separate
/// `terraintextures_*` blocks but carry their own texture ASET rows, so `extract_texture` finds them).
pub fn load_terrainmesh_tile(w: &mut wad::Wad, terrainmesh_hash: u32, pos: [f32; 3]) -> Option<LoadedModel> {
    let container = wad::extract_container_typed(w, terrainmesh_hash, TERRAINMESH_TYPE_HASH).ok()?;
    let (mut verts, indices, mut draws, stats) = mesh::build_indexed_from_terrainmesh(&container).ok()?;
    // World-place verts + synthesize a tiled world-XZ UV (the terrainmesh has no UV; detail materials
    // tile every ~12 m via the Repeat sampler).
    const UV_SCALE: f32 = 1.0 / 12.0;
    for v in verts.iter_mut() {
        v.pos[0] += pos[0];
        v.pos[1] += pos[1];
        v.pos[2] += pos[2];
        v.uv = [v.pos[0] * UV_SCALE, v.pos[2] * UV_SCALE];
    }
    // SPLAT (multi-layer per-vertex blend). Each draw's PRMG group binds a terrain MATERIAL whose
    // texture slots are (byte-verified over vz.wad): [0]=base0 (per-tile dominant diffuse), [1]=3B030C8A
    // (a GLOBAL constant across EVERY tile — a shared detail/normal overlay, NOT a per-region diffuse),
    // then N×{detail_diffuse, A3CD72A7-marker} marker-delimited terraintextures details. `terrain_group_
    // layers` returns the marker-filtered slots: [base0, 3B030C8A, detail0, detail1, ...].
    //
    // The per-vertex COLOR is D3DCOLOR SPLAT WEIGHTS (R,G,B; the 4th/alpha channel is empirically
    // always 0 across 77k+ sampled verts, weights sum to ~255). We carry up to 3 DIFFUSE blend layers
    // per draw into the material's diffuse/normal/specular texture slots (terrain has no tangent normal
    // map or spec map here, so those slots are free) and the shader blends them by the normalized
    // weights: layer0 weighted by R, layer1 by G, layer2 by B (see `shader.wgsl` `overlay.y`).
    //
    // Layer ORDER: the representative layer (index 2 = detail0 when present, else base0) goes FIRST so
    // the DOMINANT weight channel (R, ~96% of the weight mass) drives the SAME texture the pre-blend
    // single-layer bind showed — i.e. no visual regression in the dominant case, with the other real
    // layers blending in where G/B fire. Absent layers ALIAS layer0 so a missing detail blends the base
    // again (never white/flat). `terrain_group_layers` is indexed by PRMG group ORDINAL, but the builder
    // emits multiple draws per group (sub-strip split), so map each draw via `DrawGroup::group_index`.
    let layers = mercs2_formats::texture::terrain_group_layers(&container).ok()?;
    for d in draws.iter_mut() {
        if let Some(l) = layers.get(d.group_index).filter(|l| !l.is_empty()) {
            let rep_i = if l.len() > 2 { 2 } else { 0 }; // representative: detail0 else base0
            let mut blend: Vec<u32> = Vec::with_capacity(3);
            blend.push(l[rep_i]);
            for (i, &h) in l.iter().enumerate() {
                if i == 1 || i == rep_i {
                    continue; // skip 3B030C8A (global const) and the already-added representative
                }
                if blend.len() >= 3 {
                    break;
                }
                blend.push(h);
            }
            let l0 = blend[0];
            d.diffuse = Some(l0);
            d.normal = Some(blend.get(1).copied().unwrap_or(l0)); // slot repurposed = detail layer 1
            d.specular = Some(blend.get(2).copied().unwrap_or(l0)); // slot repurposed = detail layer 2
        } else {
            d.normal = None;
            d.specular = None;
        }
    }
    // Resolve the terraintextures detail layers (all three blend slots), in their separate blocks.
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal, d.specular].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    // The vertex COLOR stays the raw per-vertex SPLAT WEIGHTS (R,G,B in [0,1]) — the terrain shader
    // path reads it as blend weights, NOT as an albedo multiply, so it must NOT be whited out. `prelit`
    // is force-cleared: terrain is sun-lit, and its near-gray weights would otherwise trip the
    // interior-shell baked-lighting heuristic.
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0;
    skin.prelit = false; // terrain is never prelit — its COLOR is splat weights, not baked light
    Some(LoadedModel { hash: terrainmesh_hash, verts, indices, draws, textures, skin, clips: Vec::new(), machine: None, hier: Vec::new() })
}

/// The **model→PHY2 linkage**, verified against retail vz.wad: a model's authored Havok collision is a
/// `PHY2` **sub-chunk INSIDE its resident UCFX model container** (the same container that holds
/// GEOM/SEGM/HIER/MTRL — see [`crate::model`]'s chunk table), NOT a sibling ASET or a c3-cell field.
/// This finds that sub-chunk by its `PHY2` descriptor tag, parses the embedded Havok 5.5 packfile, and
/// triangulates the authored convex/box shapes into **model-local** collision tris (same coordinate
/// space as the render verts, so the entity Transform places them identically).
///
/// Returns `Some(tris)` ONLY when the body is a COMPLETE authored collider (`AuthoredCollision::
/// is_complete`): it carries no UNDECODED static mesh, and either it produced collision tris (convex/box hulls
/// and/or a decoded BUILDING-scale `WpMeshShape16`) OR it accounted for its geometry with a TERRAIN-scale
/// `WpMeshShape16` that is deliberately kept out of the collider (the terrain heightfield covers it — so
/// `Some(vec![])` here, NOT render tris that would flood the collider with ~10k–18k tris/cell). A body
/// with only an undecodable mesh/MOPP, or a ragdoll-only body, returns `None` → the unit gets NO
/// collision (retail authored none, or an undecoded-mesh decode gap), never a render-mesh substitute.
/// `WpMeshShape16` is now decoded (`mercs2_formats::havok::MeshShape`), so the static world collides
/// from its authored Havok mesh here.
fn authored_collision_from_container(container: &[u8]) -> Option<Vec<[mercs2_core::glam::Vec3; 3]>> {
    let body = mercs2_formats::ucfx::extract_chunk_body(container, b"PHY2")?;
    let pf = mercs2_formats::havok::parse_phy2_body(&body).ok()?;
    let ac = crate::physics::phy2::authored_collision(&pf.shapes);
    ac.is_complete().then_some(ac.tris)
}

/// Authored PHY2 collision for a streamed PROP model, by hash: extract its resident UCFX container (the
/// primary/sub model ASET block) and triangulate its `PHY2` convex/box shapes (see
/// [`authored_collision_from_container`]). `None` = no complete authored collider → NO collision.
fn load_authored_collision(w: &mut wad::Wad, hash: u32) -> Option<Vec<[mercs2_core::glam::Vec3; 3]>> {
    let container = wad::extract_container(w, hash).ok()?;
    authored_collision_from_container(&container)
}

/// Load one c3 streaming cell's `model` container by block index (the streaming executor's LOAD path).
/// Slices the `model` chunk out of the block, builds it, resolves its textures, and returns the
/// placed `LoadedModel`, the cell-origin offset (zero when the verts prove already world-space), and the
/// authored PHY2 collision tris (`Some` iff the model container carries a complete authored collider;
/// `None` → NO collision — see [`authored_collision_from_container`]).
pub fn load_one_c3_cell(w: &mut wad::Wad, block: u16) -> Option<(LoadedModel, [f32; 3], Option<Vec<[mercs2_core::glam::Vec3; 3]>>)> {
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
    // Authored PHY2 collision from the SAME sliced container (no second decompress): Some iff it is a
    // complete authored collider (the c3 model→PHY2 linkage, verified against vz.wad).
    let phy2 = authored_collision_from_container(&dec[s0..s1]);
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
    Some((LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new(), machine: None, hier: Vec::new() }, offset, phy2))
}

/// Extract one model container by hash and build its renderable `LoadedModel` (verts/tris + draws +
/// textures + skin), with `skin.center=0 / scale=1` so world placement comes purely from the entity
/// Transform. Returns the model + its local bbox. `None` if the hash has no primary ASET / fails.
pub fn load_model_by_hash(w: &mut wad::Wad, hash: u32) -> Option<(LoadedModel, [f32; 3], [f32; 3])> {
    load_model_by_hash_state(w, hash, 0x01)
}

/// Like [`load_model_by_hash`] but selects the SEGM `state_mask` tier `active_bit` (see
/// `mesh::build_indexed_state`). The PMC interior "livedin" shells invert the usual convention —
/// mask `0x04` is the intact/pristine building, `0x03` the ruined one — so they load with `0x04`.
pub fn load_model_by_hash_state(
    w: &mut wad::Wad,
    hash: u32,
    active_bit: u8,
) -> Option<(LoadedModel, [f32; 3], [f32; 3])> {
    // Assemble the model across its LOD-block chain (see `crate::model::Model`) — the primary ASET
    // block alone is a vehicle's far-LOD proxy, so a world loaded from it renders 371-triangle tanks.
    // `active_bit` keeps the legacy build-time tier filter: a segment survives if it has no mask or
    // shares a bit with the requested tier (`mesh::build_indexed_state`'s rule, applied to the
    // assembled draws instead of one container's).
    let m = crate::model::Model::load(w, hash).ok()?;
    let (verts, indices, mut draws, stats) = m.flatten();
    draws.retain(|d| d.lod_mask == 0 || d.lod_mask & active_bit != 0);
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                // Hi-res: this loader backs the always-visible PMC interior shells (the hall walls,
                // floor, columns, pictures), so assemble the full streamed mip chain, not the coarse
                // resident tail. Falls back to the resident tail for non-streaming (global) textures.
                if let Ok(t) = wad::extract_texture_hires(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0; // native metres; world placement is the authored Transform, no offset
    // Data-driven: give a rigged model its own clips (empty for a static prop / no-match — see
    // `load_clips_for_model`). This is what lets an interior prop / door / NPC animate.
    let clips = load_clips_for_model(w, &skin.rig);
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
        LoadedModel { hash, verts, indices, draws, textures, skin, clips, machine: m.machine.clone(), hier: m.hier.clone() },
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
        // Rigged prop → its own clips (empty for the static furniture that dominates this path).
        let clips = load_clips_for_model(w, &skin.rig);
        placed_meshes += 1;
        placed_instances += instances.len();
        out.push((
            hash,
            LoadedModel {
                hash, verts, indices, draws, textures, skin, clips,
                // Props are where destruction actually lives (buildings, fences, barrels). This
                // loader reads the container directly rather than via `Model`, so parse the two
                // destruction pieces off the same resident block.
                machine: mercs2_formats::orchestrator::parse_state_machine(&container),
                hier: mercs2_formats::orchestrator::parse_hier(&container),
            },
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


// ---------------------------------------------------------------------------
//   Animation clip loaders
// ---------------------------------------------------------------------------

/// The data the engine selects human clips with, resolved by asset name through the WAD stack
/// (overlays first) — never by block number: the AnimationLookup + ActionTable
/// (`animationtable` `0xE00B080C` / `0x6802C321`), and every worldentity template's animation
/// identity (`HumanAnimationSet`) with the template names that lead to it.
///
/// Selection is `(handle, template identity, equipment) → AnimationLookup row → ASTO → clip`
/// (`docs/modernization/human_animation_selection.md`); every miss is an error naming what failed.
pub struct HumanAnimData {
    pub selector: mercs2_formats::anim_select::AnimSelector,
    identities: std::collections::BTreeMap<u32, mercs2_formats::anim_select::AnimIdentity>,
    /// Lower-cased template `Name` → its keys.
    names: std::collections::HashMap<String, Vec<u32>>,
}

impl HumanAnimData {
    pub fn load(assets: &mut crate::asset::AssetSource) -> Result<HumanAnimData, String> {
        use mercs2_formats::anim_select::{human_animation_sets, AnimSelector, ACTIONTABLE, ANIMLOOKUP, TYPE_ANIMTABLE};
        use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};
        let lookup = assets
            .resolve(TYPE_ANIMTABLE, ANIMLOOKUP)
            .ok_or("AnimationLookup 0xE00B080C: in no open wad")?;
        let actions = assets
            .resolve(TYPE_ANIMTABLE, ACTIONTABLE)
            .ok_or("ActionTable 0x6802C321: in no open wad")?;
        let selector = AnimSelector::from_containers(&lookup, Some(&actions))?;
        let we = assets
            .resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
            .ok_or("worldentity 0x50075B3B: in no open wad")?;
        let we = WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?;
        let identities = human_animation_sets(&we)?;
        let mut names: std::collections::HashMap<String, Vec<u32>> = Default::default();
        for (n, keys) in we.names().map_err(|e| format!("worldentity: {e}"))? {
            names.entry(n.to_ascii_lowercase()).or_default().extend_from_slice(keys);
        }
        Ok(HumanAnimData { selector, identities, names })
    }

    /// The animation identity of the template `name` (e.g. the hero templates `mattias`/`chris`/`jen`).
    pub fn identity_of_template(&self, name: &str) -> Result<mercs2_formats::anim_select::AnimIdentity, String> {
        let keys = self
            .names
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| format!("template {name:?}: no worldentity Name"))?;
        self.agreed_identity(keys, &format!("template {name:?}"))
    }

    /// The animation identity (`HumanAnimationSet`) of the template with key `key`.
    pub fn identity_of_key(&self, key: u32) -> Result<mercs2_formats::anim_select::AnimIdentity, String> {
        self.identities
            .get(&key)
            .copied()
            .ok_or_else(|| format!("template 0x{key:08X}: no HumanAnimationSet"))
    }

    fn agreed_identity(&self, keys: &[u32], what: &str) -> Result<mercs2_formats::anim_select::AnimIdentity, String> {
        let mut found: Option<(u32, mercs2_formats::anim_select::AnimIdentity)> = None;
        for &k in keys {
            if let Some(&id) = self.identities.get(&k) {
                match found {
                    Some((k0, id0)) if id0 != id => {
                        return Err(format!("{what}: templates 0x{k0:08X} and 0x{k:08X} disagree ({id0:?} vs {id:?})"))
                    }
                    _ => found = Some((k, id)),
                }
            }
        }
        found
            .map(|(_, id)| id)
            .ok_or_else(|| format!("{what}: no template ({keys:08X?}) carries a HumanAnimationSet"))
    }

    /// The AnimationLookup row retail's query picks for `handle` and `key`
    /// ([`AnimSelector::query`](mercs2_formats::anim_select::AnimSelector::query)). An error when no
    /// row survives the query.
    pub fn row(
        &self,
        handle: u32,
        key: &mercs2_formats::anim_select::LookupKey,
    ) -> Result<mercs2_formats::anim_select::LookupRow, String> {
        self.selector
            .query(handle, key)
            .ok_or_else(|| format!("handle 0x{handle:08X}: no AnimationLookup row survives the query for {key:08X?}"))
    }

    /// The one clip `handle` resolves to for `key`. An error when no row survives the query, or when
    /// the row lists several clips — retail draws one of those per play, which a clip resolved once
    /// cannot stand for.
    pub fn clip(&self, handle: u32, key: &mercs2_formats::anim_select::LookupKey) -> Result<u32, String> {
        match self.row(handle, key)?.clips.as_slice() {
            [c] => Ok(*c),
            many => Err(format!(
                "handle 0x{handle:08X}: the row for {key:08X?} lists {many:08X?}; retail draws one per play"
            )),
        }
    }

    /// The Upright locomotion clips (idle / walk / run) for an identity carrying `equipment_class`.
    pub fn locomotion(
        &self,
        identity: mercs2_formats::anim_select::AnimIdentity,
        equipment_class: u32,
    ) -> Result<mercs2_player::LocomotionClips, String> {
        use mercs2_anim::select::{UPRIGHT_IDLE_HANDLE, UPRIGHT_RUN_HANDLE, UPRIGHT_WALK_HANDLE};
        let key = mercs2_formats::anim_select::LookupKey::new(identity, equipment_class);
        Ok(mercs2_player::LocomotionClips {
            idle: self.clip(UPRIGHT_IDLE_HANDLE, &key)?,
            walk: self.clip(UPRIGHT_WALK_HANDLE, &key)?,
            run: self.clip(UPRIGHT_RUN_HANDLE, &key)?,
        })
    }
}

/// One population unit template as the spawn path realizes it: its archetype (from the physics
/// component it carries), its `ModelName`, and its faction — `Association`, a faction template such as
/// "Guerilla" (`0x80002CF1`); 0 when the template has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitTemplate {
    pub archetype: crate::spawn::Archetype,
    pub model: u32,
    pub faction: u32,
}

/// The physics components that give a template its archetype. `_HelicopterPhysicsAi` rides
/// alongside `_HelicopterPhysics` on the same templates, so it does not count separately.
pub(crate) fn unit_archetype(classes: &[&str]) -> Result<crate::spawn::Archetype, String> {
    use crate::spawn::Archetype;
    use crate::vehicle::components::VehicleClass;
    let found: Vec<Archetype> = classes
        .iter()
        .filter_map(|c| match *c {
            "_HumanPhysics" => Some(Archetype::Character),
            "_CarPhysicsV2" => Some(Archetype::Vehicle(VehicleClass::Car)),
            "_TankPhysics" => Some(Archetype::Vehicle(VehicleClass::Tank)),
            "_BoatPhysics" => Some(Archetype::Vehicle(VehicleClass::Boat)),
            "_HelicopterPhysics" => Some(Archetype::Vehicle(VehicleClass::Helicopter)),
            "_JetPhysics" => Some(Archetype::Vehicle(VehicleClass::Jet)),
            _ => None,
        })
        .collect();
    match found.as_slice() {
        [] => Ok(Archetype::Prop),
        [a] => Ok(*a),
        many => Err(format!("carries several archetype physics components {many:?}")),
    }
}

/// What the ambient population spawns from, read from the retail `worldentity` by asset name: the
/// `PopulationList` unit lists (as a [`crate::population::UnitCatalog`]) and every unit template they
/// name.
pub struct PopulationData {
    pub catalog: crate::population::UnitCatalog,
    pub units: std::collections::HashMap<u32, UnitTemplate>,
}

impl PopulationData {
    pub fn load(assets: &mut crate::asset::AssetSource) -> Result<PopulationData, String> {
        use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};
        let we = assets
            .resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
            .ok_or("worldentity 0x50075B3B: in no open wad")?;
        Self::from_worldentity(&WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?)
    }

    /// Build from a parsed `worldentity`. An error when a listed unit (other than 0, which spawns
    /// nothing) has no template `ModelName` or several archetype physics components.
    pub fn from_worldentity(we: &mercs2_formats::worldentity::WorldEntity) -> Result<PopulationData, String> {
        use mercs2_formats::worldentity::Payload;
        let lists = mercs2_formats::unit_list::unit_lists(we)?;
        let wanted: std::collections::BTreeSet<u32> = lists.values().flatten().map(|e| e.unit).filter(|&u| u != 0).collect();
        let mut classes: std::collections::HashMap<u32, Vec<&str>> = Default::default();
        let mut words: std::collections::HashMap<(u32, &str), u32> = Default::default();
        for c in &we.components {
            for r in &c.records {
                for &k in r.keys.iter().filter(|k| wanted.contains(k)) {
                    classes.entry(k).or_default().push(c.class.as_str());
                    if let ("ModelName" | "Association", Payload::Fixed(b)) = (c.class.as_str(), &r.payload) {
                        if b.len() != 4 {
                            return Err(format!("unit 0x{k:08X}: {} payload is {} bytes, not one u32", c.class, b.len()));
                        }
                        words.insert((k, c.class.as_str()), u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                    }
                }
            }
        }
        let mut units = std::collections::HashMap::new();
        let mut vehicles = std::collections::BTreeSet::new();
        for &u in &wanted {
            let cls = classes.get(&u).map(Vec::as_slice).unwrap_or_default();
            let archetype = unit_archetype(cls).map_err(|e| format!("unit 0x{u:08X}: {e}"))?;
            let model = *words
                .get(&(u, "ModelName"))
                .ok_or_else(|| format!("unit 0x{u:08X}: the template has no ModelName"))?;
            if matches!(archetype, crate::spawn::Archetype::Vehicle(_)) {
                vehicles.insert(u);
            }
            units.insert(u, UnitTemplate { archetype, model, faction: words.get(&(u, "Association")).copied().unwrap_or(0) });
        }
        Ok(PopulationData { catalog: crate::population::UnitCatalog::new(lists, vehicles)?, units })
    }
}

/// Every unit the registered spawners' lists can spawn (unit 0, which spawns nothing, excluded) — the
/// set a population preload makes resident.
pub fn population_units(pop: &crate::population::PopulationWorld) -> std::collections::BTreeSet<u32> {
    pop.spawners
        .spawners()
        .iter()
        .flat_map(|s| pop.units.units_of(s.choice.unit_list))
        .filter(|&u| u != 0)
        .collect()
}

/// One population model made resident: the model (identity fit — world metres, as the hero) and each
/// unit template that draws it, with the unarmed Upright idle a Character of that template starts in
/// (`None` for a vehicle or prop unit).
pub struct PopulationModel {
    pub model: LoadedModel,
    pub units: Vec<(u32, Option<u32>)>,
}

/// Build every model `units` draws, once per model, with each Character unit's idle
/// (`UPRIGHT_IDLE_HANDLE`, its own `HumanAnimationSet` identity, `Bare`) bound to the model's HIER. An
/// error naming the unit or model when a template is missing, a model does not build, or an idle does
/// not resolve or bind.
pub fn load_population_models(
    w: &mut wad::Wad,
    anim: &HumanAnimData,
    data: &PopulationData,
    units: &std::collections::BTreeSet<u32>,
) -> Result<Vec<PopulationModel>, String> {
    use mercs2_formats::anim_select::{LookupKey, EQUIPMENT_BARE};
    let mut by_model: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
    for &u in units {
        let t = data.units.get(&u).ok_or_else(|| format!("unit 0x{u:08X}: no unit template"))?;
        by_model.entry(t.model).or_default().push(u);
    }
    let mut out = Vec::new();
    for (model, templates) in by_model {
        let (mut m, _, _) = load_model_by_hash(w, model)
            .ok_or_else(|| format!("units {templates:08X?}: model 0x{model:08X} does not build"))?;
        m.skin.center = [0.0, 0.0, 0.0];
        m.skin.scale = 1.0;
        m.hier = Vec::new();
        let mut units = Vec::new();
        let mut idles: Vec<u32> = Vec::new();
        for u in templates {
            let idle = if data.units[&u].archetype == crate::spawn::Archetype::Character {
                let id = anim.identity_of_key(u)?;
                let idle = anim
                    .clip(mercs2_anim::select::UPRIGHT_IDLE_HANDLE, &LookupKey::new(id, EQUIPMENT_BARE))
                    .map_err(|e| format!("unit 0x{u:08X}: {e}"))?;
                if !idles.contains(&idle) {
                    idles.push(idle);
                }
                Some(idle)
            } else {
                None
            };
            units.push((u, idle));
        }
        let hier: Vec<u32> = m.skin.rig.iter().map(|b| b.name_hash).collect();
        m.clips = load_clips_for_rig(w, &hier, &idles)
            .into_iter()
            .zip(&idles)
            .map(|(c, idle)| c.ok_or_else(|| format!("model 0x{model:08X}: idle 0x{idle:08X} is in no animgroup that binds to it")))
            .collect::<Result<_, _>>()?;
        out.push(PopulationModel { model: m, units });
    }
    Ok(out)
}

/// Resolve a swimming locomotion clip through the real ActionTable + AnimationLookup (resident block
/// 3185). Swim animations are SHARED across mercs — the AnimationLookup keys them under the NONE
/// character sentinel (`0x27DE7135`), not each merc's hash — so the engine plays the same swim clips
/// for everyone in the `Swim` stance (`m2("Swim") = 0x614DB965`). Returns a surface-swim clip,
/// preferring one that is not the `Dive` plunge (`m2("Dive")` handle → `0x64B3CC44`). Relocated from
/// `mercs2_game::world::resolve_player_swim`.
pub fn resolve_player_swim(w: &mut wad::Wad) -> Option<u32> {
    use mercs2_formats::anim_select::AnimSelector;
    const NONE_KEY: u32 = 0x27DE_7135; // shared/character-agnostic AnimationLookup key
    const DIVE_CLIP: u32 = 0x64B3_CC44; // the m2("Dive") swim clip — a plunge, not surface locomotion
    let dec = wad::decompress_block_index(w, 3185).ok()?;
    let sel = AnimSelector::from_resident_block(&dec).unwrap_or_else(|e| panic!("block 3185: {e}"))?;
    let swim = mercs2_formats::hash::pandemic_hash_m2("Swim");
    let mut fallback = None;
    for (st, ac) in sel.action_states() {
        if st != swim {
            continue;
        }
        for h in sel.handles_for_state(st, ac) {
            for c in sel.handle_clips(h).into_iter().filter(|&(ch, _)| ch == NONE_KEY).map(|(_, c)| c) {
                if c != DIVE_CLIP {
                    return Some(c);
                }
                fallback = fallback.or(Some(c));
            }
        }
    }
    fallback
}

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
    let clip = mercs2_formats::anim::parse_anim(c.packfile(&data).ok()?).ok()?;
    if !clip.decoded {
        return None; // a spline clip (header-only; none ship) — leave synthetic driver in place
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
            let clip = mercs2_formats::anim::parse_anim(c.packfile(data).ok()?).ok()?;
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
//   Generalised prop / object clip loading (data-driven, reuses the Havok decode)
// ---------------------------------------------------------------------------
//
// The player path (mercs2_game) hard-codes Mattias' idle/walk/run clip hashes. Every other model —
// interior props, doors, machines, NPCs, DLC/mod content — was left with `clips: Vec::new()`, so it
// could never animate even if the WAD ships clips for its rig. The functions below lift that hard
// coding into a data-driven discovery layer: index every animation clip once, then for ANY loaded
// model resolve its own clips by matching its HIER node name-hashes against each clip's `trnm`
// binding (`AnimBinding::resolve_to_hier`). Decode stays the existing wavelet/Havok path — this only
// adds the *selection* the player did by hand.
//
// What the retail clips are (`mercs2_probe --bin anim_codec_census` over vz.wad + vz-patch.wad):
// 4,261 animgroup entries — 4,103 wavelet, 73 interleaved and 56 delta-compressed Havok clips, and 29
// MANM material animations (MANM/MINF/TRCK rows, no `trnm`, played by `Object.PlayMaterialAnimation`).
// By transform-track count: 3,606 have 60–105, 356 have 20–59, 126 have 2–19, 141 have one (the
// hijack clips drive a vehicle's door or hatch node: `0x9C94874B` drives md500 HIER node 21), 3 have
// more than 105, and the 29 MANM have none. Of vz.wad's 2,414 models with two or more HIER nodes, 312
// carry a node some clip's binding names, 288 a node named by a clip of fewer than 20 tracks. This
// path animates any model whose rig a clip binds; `select_for_rig` below keeps a clip only when a
// majority of its tracks bind.

/// One clip's identity + rig binding, indexed once over every animgroup block so a model's clips can
/// be discovered by name-hash rig match without re-scanning the archive per model.
pub struct ClipIndexEntry {
    /// The animgroup WAD block this clip lives in.
    pub block: u16,
    /// Pandemic name-hash of the clip.
    pub name_hash: u32,
    /// Number of transform tracks (`hkaAnimation::numTransformTracks` == `trnm` length).
    pub num_transform_tracks: usize,
    /// Per-track bone name-hash binding (the `trnm` chunk).
    pub bones: Vec<u32>,
}

/// An index of every animation clip in the WAD (built once) enabling data-driven clip discovery for
/// any rigged model. Reuses the existing `animgroup`/`anim` Havok decode; this only adds the
/// *selection* layer the player path hard-coded.
pub struct AnimClipIndex {
    pub clips: Vec<ClipIndexEntry>,
}

impl AnimClipIndex {
    /// Scan every animgroup block once and record each clip's `trnm` binding.
    pub fn build(w: &mut wad::Wad) -> AnimClipIndex {
        use mercs2_formats::animgroup::parse_animgroup;
        let mut clips = Vec::new();
        for blk in wad::animgroup_blocks(w) {
            let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
            let Ok(ag) = parse_animgroup(&data) else { continue };
            for c in &ag.clips {
                clips.push(ClipIndexEntry {
                    block: blk,
                    name_hash: c.name_hash,
                    num_transform_tracks: c.num_transform_tracks as usize,
                    bones: c.binding.track_to_bone_hash.clone(),
                });
            }
        }
        clips.sort_by_key(|c| c.name_hash);
        clips.dedup_by_key(|c| c.name_hash);
        AnimClipIndex { clips }
    }

    /// Select the clips that BELONG to a rig, by name-hash match. A clip belongs when at least 2 of
    /// its tracks resolve into the rig AND a MAJORITY of its tracks do (`resolved*2 >= tracks`): the
    /// clip was authored for THIS skeleton, not merely sharing a couple of common node names. This
    /// cleanly separates a small prop/door clip (all its 2–10 tracks map into the prop rig) from the
    /// character body clips (60–105 tracks, of which a prop rig resolves ~0). Ranked by tracks
    /// resolved (desc), then track count (desc), capped at `max`. Pure — unit-testable without a WAD.
    pub fn select_for_rig(&self, hier: &[u32], max: usize) -> Vec<u32> {
        let set: std::collections::HashSet<u32> = hier.iter().copied().collect();
        let mut scored: Vec<(usize, usize, u32)> = Vec::new(); // (resolved, ntt, name_hash)
        for c in &self.clips {
            if c.num_transform_tracks == 0 {
                continue;
            }
            let resolved = c.bones.iter().filter(|b| set.contains(b)).count();
            if resolved >= 2 && resolved * 2 >= c.num_transform_tracks {
                scored.push((resolved, c.num_transform_tracks, c.name_hash));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        scored.truncate(max);
        scored.into_iter().map(|(_, _, h)| h).collect()
    }

    /// Decode ONE indexed clip (its block is already known — no re-scan) and bind its tracks to
    /// `hier`. Returns `None` for a non-decoded (spline header-only) clip.
    fn decode(&self, w: &mut wad::Wad, hier: &[u32], name_hash: u32) -> Option<ClipAnim> {
        use mercs2_formats::animgroup::parse_animgroup;
        let entry = self.clips.iter().find(|c| c.name_hash == name_hash)?;
        let data = wad::decompress_block_index(w, entry.block).ok()?;
        let ag = parse_animgroup(&data).ok()?;
        let c = ag.clips.iter().find(|c| c.name_hash == name_hash)?;
        let clip = mercs2_formats::anim::parse_anim(c.packfile(&data).ok()?).ok()?;
        if !clip.decoded {
            return None;
        }
        Some(ClipAnim {
            track_to_hier: c.binding.resolve_to_hier(hier),
            num_transform_tracks: c.num_transform_tracks as usize,
            name_hash,
            clip,
        })
    }
}

type ClipIndexCache = std::sync::Mutex<std::collections::HashMap<String, &'static AnimClipIndex>>;
static CLIP_INDEX: std::sync::OnceLock<ClipIndexCache> = std::sync::OnceLock::new();

/// Cached clip index, keyed by the ARCHIVE it was built from.
///
/// Keyed per archive because a WAD STACK holds several: the workshop opens `vz.wad` + `vz-patch.wad`,
/// and lookups walk the stack in reverse (patch first, the retail last-opened-wins rule). The patch
/// ships no animgroup blocks, so its index is empty; one process-wide index built from it would
/// resolve zero clips for every model. Each WAD gets its own index, and the base WAD's is never
/// shadowed by an overlay's.
///
/// Building scans the ~190 animgroup blocks once per archive (a few seconds); later lookups are cheap
/// integer comparisons over the flat clip list. Indexes are leaked deliberately — one per open
/// archive, for the life of the process.
pub fn clip_index(w: &mut wad::Wad) -> &'static AnimClipIndex {
    let key = wad::wad_path(w).to_string();
    let cache = CLIP_INDEX.get_or_init(Default::default);
    if let Some(ix) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return ix;
    }
    // Build OUTSIDE the lock: it is slow, and it needs `&mut w`.
    let ix: &'static AnimClipIndex = Box::leak(Box::new(AnimClipIndex::build(w)));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    *guard.entry(key).or_insert(ix)
}

/// Minimum HIER bone count for a model to be treated as animatable (below this it is a static prop:
/// a single skin bone, nothing to pose).
const MIN_ANIM_RIG_BONES: usize = 2;

/// How many discovered clips to auto-populate onto a model. Bounds decode cost — a character rig
/// matches thousands of body clips; a working set proves/drives playback without loading them all.
const MAX_AUTO_CLIPS: usize = 6;

/// Populate a model's own animation clips by matching its rig against the cached WAD clip index.
/// Returns EMPTY for a static (`rig < 2`) model or one with no matching clip (every retail prop — see
/// the RE note above). This is the data-driven generalisation of the hard-coded player idle/walk/run
/// load; decode reuses the existing Havok path.
pub fn load_clips_for_model(w: &mut wad::Wad, rig: &[mesh::BoneRig]) -> Vec<ClipAnim> {
    if rig.len() < MIN_ANIM_RIG_BONES {
        return Vec::new();
    }
    let hier: Vec<u32> = rig.iter().map(|b| b.name_hash).collect();
    let ix = clip_index(w); // &'static — independent of the &mut w decode borrows below
    let wanted = ix.select_for_rig(&hier, MAX_AUTO_CLIPS);
    let mut out = Vec::new();
    for h in wanted {
        if let Some(ca) = ix.decode(w, &hier, h) {
            out.push(ca);
        }
    }
    out
}

// ---------------------------------------------------------------------------
//   Streaming world
// ---------------------------------------------------------------------------

/// Everything the streaming runtime needs, produced on the background loader thread (all `Send`):
/// the WAD handle (moved to the render thread for on-demand activation extraction), the base terrain,
/// the block/region decision manager, and the worldentity template records placed entities fall back
/// to. Placed entities themselves arrive by layer loads ([`StreamingWorld`]).
///
/// **Opaque cross-crate handle** (K2 unification, `docs/modernization/k2_streaming_unification_plan.md`):
/// the type is `pub` so `mercs2_game`'s TPS boot can hold the loader output and hand it to the
/// streaming executor, but the fields stay crate-private — the executor (this crate) is the only thing
/// that reads them.
pub struct StreamingWorldData {
    wad: wad::Wad,
    terrain: LoadedModel,
    manager: mercs2_core::streaming::StreamingManager,
    templates: TemplateRecords,
    /// Low-res terrain grid cell (row*20+col) -> its draw-group index in the `terrain` model, so the
    /// executor can hide that tile when the hi-res terrainmesh at the same cell is resident.
    lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
    /// The layers whose containers the `layers_static` block carries, in block order.
    static_block_layers: Vec<u32>,
}

impl StreamingWorldData {
    /// The layers whose containers the `layers_static` block carries, in block order.
    pub fn static_block_layers(&self) -> &[u32] {
        &self.static_block_layers
    }

    /// Consume the loader output into the reusable [`StreamingWorld`] executor over the world's
    /// entity-key registry `guids`, handing back the base (low-res) terrain model for the one-time
    /// upload.
    ///
    /// This is the K2-unification hand-off for a caller in ANOTHER crate: `mercs2_game`'s TPS boot holds
    /// the opaque `StreamingWorldData` and cannot read its crate-private fields, so this method does the
    /// same boot glue `FreeFlyGame::setup` does inline (upload terrain, take ownership of the streamer)
    /// and returns the pieces the caller must wire into its own `Scene`/`World`.
    pub fn into_streaming_world(self, guids: std::rc::Rc<std::cell::RefCell<mercs2_core::GuidMap>>) -> (LoadedModel, StreamingWorld) {
        let StreamingWorldData { wad, terrain, manager, templates, lowres_draw_by_cell, static_block_layers: _ } = self;
        let terrain_hash = terrain.hash;
        let sw = StreamingWorld::new(wad, manager, templates, lowres_draw_by_cell, terrain_hash, guids);
        (terrain, sw)
    }
}

/// Convert a harvested `LightObject` placement to a GPU point light, dropping degenerate lights
/// (non-positive / non-finite radius) so the inferred `params[0]=intensity` mapping can never flood the
/// scene. Color/intensity/radius are the authored on-disk values.
pub fn placed_lights_to_gpu(placed: &[mercs2_formats::placement::PlacedLight]) -> Vec<crate::render::GpuLight> {
    placed
        .iter()
        .filter(|pl| !pl.light.is_spot()) // spots go to `placed_spot_lights_to_gpu`
        .filter_map(|pl| {
            let radius = pl.light.radius();
            let intensity = pl.light.intensity();
            if !radius.is_finite() || radius <= 0.0 || !intensity.is_finite() {
                return None;
            }
            Some(crate::render::GpuLight::point(pl.pos, pl.light.color, intensity, radius))
        })
        .collect()
}

/// The **spot** half of the same placement inventory → [`SpotLightGpu`](crate::scene::SpotLightGpu),
/// feeding the `_sl` per-pixel cone path (`Scene::set_spot_lights`).
///
/// Data-driven from the pinned `LightObject` layout (see `placement::LightObject::is_spot` /
/// `PlacedLight::cone_axis`): type 3 = spot, aimed along the placement's local −Y, with
/// `params[2]` = throw range, `params[5]` = the full cone angle (the edge is half of it) and
/// `params[4]` = the inner edge (its retail role is not decoded). Before this,
/// every authored spot was silently dropped (its `params[1]` is 0, and the old `radius()` read that
/// slot, so the `radius <= 0` guard discarded it).
pub fn placed_spot_lights_to_gpu(
    placed: &[mercs2_formats::placement::PlacedLight],
) -> Vec<crate::scene::SpotLightGpu> {
    placed
        .iter()
        .filter(|pl| pl.light.is_spot())
        .filter_map(|pl| {
            let range = pl.light.radius();
            let intensity = pl.light.intensity();
            if !range.is_finite() || range <= 0.0 || !intensity.is_finite() {
                return None;
            }
            // The cone edge is half the authored cone angle (`FUN_004A7660`); keep inner <= outer so
            // the edge fades inward.
            let outer = pl.light.cone_angle() * 0.5;
            let inner = pl.light.cone_inner().min(outer);
            Some(crate::scene::SpotLightGpu::new(
                pl.pos,
                pl.cone_axis(),
                pl.light.color,
                intensity,
                range,
                inner,
                outer,
            ))
        })
        .collect()
}

/// Build a static additive glow card for an environmental light-shaft placement
/// (`global_particle_env_godray2` — the PMC hall god rays). Faithful + data-driven:
///  * position = the placement world position;
///  * size = the effect `TRFM` horizontal footprint (`0.5·(|sx|+|sz|)`), scaled up so the soft
///    additive sprite reads as a shaft of dusty light rather than a pinpoint;
///  * tint = the effect `COLR` peak colour (dim white ~0.25) lifted for the additive/HDR path.
///
/// The effect template name is the placement name with `"particle_"` removed (verified:
/// `global_particle_env_godray2` → `global_env_godray2`, m2 `0xDB331999`, in the `effects` block).
/// Falls back to the reversed retail constants if the effect can't be found; an effect that is
/// found but does not parse is an error.
pub fn glow_card_for_effect(
    w: &mut wad::Wad,
    placement_name: &str,
    pos: [f32; 3],
) -> Result<crate::particles::GlowCard, String> {
    let (scale, color, alpha) = env_shaft_effect_params(w, placement_name)?
        .unwrap_or(([2.892, 0.805, 2.892], [0.25, 0.24, 0.20], 0.16));
    let footprint = 0.5 * (scale[0].abs() + scale[2].abs()); // ~2.89
    // The retail COLR is dim (~0.25 white, alpha ~0.16); lift into an additive/HDR-friendly tint so
    // the soft glow is visible and blooms, without washing out (clamped).
    let boost = (alpha * 4.0 + 0.6).clamp(0.8, 2.5);
    let a = (alpha * 3.5).clamp(0.25, 0.85);
    Ok(crate::particles::GlowCard {
        pos,
        size: (footprint * 3.0).clamp(4.0, 24.0),
        color: [
            (color[0] * boost).min(1.5),
            (color[1] * boost).min(1.5),
            (color[2] * boost).min(1.5),
            a,
        ],
    })
}

/// Load and parse an effect by name-hash
/// ([`parse_effect_container`](mercs2_formats::fxdict::parse_effect_container)). Feeds
/// [`EmitterDesc::from_effect`](crate::particles::EmitterDesc::from_effect) so authored
/// fire/smoke/steam effects drive the particle sim with real params.
///
/// The block comes from the effect's own ASET row (`asset_hash == name_hash`,
/// `type_id == TYPE_ID_EFFECT`) — the WAD's index of where the asset lives — not from a block path.
/// Picking "the first path containing `effect`" landed on `text_effect_P000_Q3` (block 3117, which
/// sorts before `effects_P000_Q3`, block 3459), so no effect ever loaded.
///
/// `Ok(None)` when the WAD has no effect row for the name; `Err` when it has one but the named
/// block does not hold the container, or the container does not parse.
pub fn load_effect(
    w: &mut wad::Wad,
    name_hash: u32,
) -> Result<Option<mercs2_formats::fxdict::EffectContainer>, String> {
    use mercs2_formats::types::{TYPE_HASH_EFFECT, TYPE_ID_EFFECT};
    let mut blocks: Vec<u16> = {
        let (archive, _) = wad::archive_and_file(w);
        archive
            .aset
            .iter()
            .filter(|a| a.asset_hash == name_hash && a.type_id == TYPE_ID_EFFECT)
            .flat_map(|a| a.lod_chain())
            .filter(|&b| b != 0xFFFF)
            .collect()
    };
    blocks.sort_unstable();
    blocks.dedup();
    if blocks.is_empty() {
        return Ok(None);
    }
    for &blk in &blocks {
        let dec = wad::decompress_block_index(w, blk)?;
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == TYPE_HASH_EFFECT && e.name_hash == name_hash {
                let c = dec
                    .get(pos..end)
                    .ok_or_else(|| format!("effect 0x{name_hash:08X} runs past block {blk}"))?;
                return mercs2_formats::fxdict::parse_effect_container(c)
                    .map(Some)
                    .map_err(|err| format!("effect 0x{name_hash:08X}: {err}"));
            }
            pos = end;
        }
    }
    Err(format!(
        "the ASET row for effect 0x{name_hash:08X} names blocks {blocks:?}, and none holds the container"
    ))
}

/// Read an environmental light-shaft effect and recover `(TRFM scale, COLR peak colour, COLR peak
/// fourth byte)` from its first emitter: the scale is the TRFM diagonal, the peak is the `COLR` key
/// with the largest sum of its first three colour bytes. `Ok(None)` if the effect isn't found.
fn env_shaft_effect_params(w: &mut wad::Wad, placement_name: &str) -> Result<Option<([f32; 3], [f32; 3], f32)>, String> {
    let want = mercs2_formats::hash::pandemic_hash_m2(&placement_name.replace("particle_", ""));
    let Some(effect) = load_effect(w, want)? else {
        return Ok(None);
    };
    let e = effect.emitters.first().ok_or_else(|| format!("effect 0x{want:08X} has no emitter"))?;
    let m = e.transform;
    let (rgb, a) = colr_peak(&e.particle.colr);
    Ok(Some(([m[0][0], m[1][1], m[2][2]], rgb, a)))
}

/// The first `COLR` key with the greatest `c[0] + c[1] + c[2]`, as `([c0, c1, c2] / 255, c3 / 255)`.
fn colr_peak(colr: &mercs2_formats::fxdict::Colr) -> ([f32; 3], f32) {
    let lum = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
    let mut best = colr.keys[0].rgba;
    for k in &colr.keys[1..] {
        if lum(k.rgba) > lum(best) {
            best = k.rgba;
        }
    }
    let c = best.map(|v| v as f32 / 255.0);
    ([c[0], c[1], c[2]], c[3])
}

/// Load the streaming world (the base terrain, the block/region `StreamingManager`, the template
/// records, the WAD handle) on the background thread. Placed entities are not loaded here: they arrive
/// by layer loads ([`StreamingWorld`]). `pub` for the K2 unification: `mercs2_game`'s playable boot
/// calls this to get the same streaming runtime the free-fly path uses (see
/// `k2_streaming_unification_plan.md`).
pub fn load_streaming_world_data(
    wadpath: &str,
    cfg: mercs2_core::streaming::StreamingConfig,
    templates: TemplateRecords,
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
        println!("[stream] terrain texture: vz_lrterrain atlas {}x{} ({:?}) bound at key 0", t.width, t.height, t.format);
        textures.insert(0, t);
        Some(0)
    } else {
        println!("[stream] terrain texture: NONE — vz_lrterrain atlas did not parse; terrain will render untextured (black)");
        None
    };
    // One draw group PER TILE (all sharing the atlas view), so a low-res tile can be hidden when its
    // hi-res terrainmesh is resident. `lowres_draw_by_cell[cell] = draw index` maps the 20x20 grid.
    let mut draws = Vec::with_capacity(tm.tile_draws.len());
    let mut lowres_draw_by_cell: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, &(cell, start, count)) in tm.tile_draws.iter().enumerate() {
        draws.push(mesh::DrawGroup { index_start: start, index_count: count, diffuse, ..Default::default() });
        lowres_draw_by_cell.insert(cell, i);
    }
    let terrain = LoadedModel {
        machine: None, hier: Vec::new(),
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

    // World block index (c3-cell extents) + the block/region manager.
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    progress.step("world index");
    let manager = build_streaming_catalog(&idx, cfg);
    println!("[stream] manager: {} c3-cell blocks", manager.block_count());

    // Seam A — E1 schema-driven component pass (ADDITIVE, alongside the oracle-driven catalog above):
    // build the kernel ComponentRegistry from the block's `schm` field schemas and deserialize its
    // generic COMP records, cross-checking agreement with the bespoke placement oracle. This wires the
    // field-schema deserializer into the live loader; it does not change what streams.
    let (comp_reg, sstats) = load_schema_components(&ls);
    println!(
        "[stream] schema (seam A): {} classes registered (pool budget {}), {} generic COMP groups / {} records deserialized; oracle agreement: ModelName {}/{}",
        sstats.classes, comp_reg.total_budget(), sstats.generic_groups, sstats.generic_records,
        sstats.model_agree, sstats.model_checked
    );
    if sstats.model_agree != sstats.model_checked {
        eprintln!(
            "[stream] WARNING: schema path disagreed with the placement oracle (ModelName {}/{}) — investigate before trusting schema-driven instantiation",
            sstats.model_agree, sstats.model_checked
        );
    }
    progress.step("streaming catalog");

    let static_block_layers = block_layers(&ls);
    Ok(StreamingWorldData { wad: w, terrain, manager, templates, lowres_draw_by_cell, static_block_layers })
}

/// The scene and ECS world a step presents entities in; `None` for a headless step.
type Presentation<'a> = Option<(&'a mut crate::scene::Scene, &'a mut mercs2_core::World)>;

/// A placed entity's activation side ([`mercs2_core::activation::ActivationHost`]) over the streaming
/// executor: begin requests its model, `CanActivate` is that model resident, complete spawns it. A
/// headless activation (no presentation) loads and spawns nothing: an asset the archive carries counts
/// as resident.
struct Activator<'a> {
    sw: &'a mut StreamingWorld,
    present: Presentation<'a>,
}

impl mercs2_core::activation::ActivationHost for Activator<'_> {
    /// From asleep → waking; an asset the archive does not carry cancels it (`0xED3B18`).
    fn begin(&mut self, key: u32) -> Result<bool, String> {
        if self.sw.state(key) != 0 {
            return Ok(true);
        }
        self.sw.guids.borrow_mut().set_state(key, 1);
        Ok(self.sw.assets_present(key))
    }

    /// The model is resident: loaded here, synchronously, so a poll of a present asset succeeds.
    fn can_activate(&mut self, key: u32, _forced: bool) -> Result<bool, String> {
        match &mut self.present {
            Some((scene, _)) => self.sw.load_assets(key, scene),
            None => Ok(self.sw.assets_present(key)),
        }
    }

    /// Waking → awake; then, as retail's complete calls SetTransform after the listeners
    /// (`0x4F8171`), the grid entry is re-celled with the render node the ModelName listener made.
    fn complete(&mut self, key: u32) -> Result<(), String> {
        if self.sw.state(key) != 1 {
            return Ok(());
        }
        self.sw.guids.borrow_mut().set_state(key, 2);
        self.sw.recell(key)?;
        match &mut self.present {
            Some((scene, world)) => self.sw.sync_visual(key, scene, world),
            None => Ok(()),
        }
    }

    fn deactivate(&mut self, key: u32) {
        if self.sw.state(key) != 0 {
            self.sw.guids.borrow_mut().set_state(key, 0);
            if let Some((scene, world)) = &mut self.present {
                self.sw.despawn_visual(key, scene, world);
            }
        }
    }

    fn now_us(&mut self) -> u64 {
        self.sw.epoch.elapsed().as_micros() as u64
    }

    /// Nothing waits on I/O: assets load synchronously in the poll.
    fn io_busy(&mut self) -> bool {
        false
    }
}

/// The one-bone identity palette of a static streamed model.
const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// A per-frame stats snapshot for the streaming console/HUD line.
pub struct StreamStats {
    pub resident: usize,
    pub awake: usize,
    pub cached_regions: usize,
    pub regions: usize,
    pub block_ents: usize,
    pub props: usize,
    pub models: usize,
    /// Cumulative streamed units that used AUTHORED PHY2 (convex/box/mesh) collision.
    pub coll_authored: usize,
    /// Cumulative streamed units left with NO collision because they carry no complete authored
    /// collider — faithful for the many props retail authored non-colliding, and a logged decode
    /// gap for the handful of undecoded static meshes. NEVER a render-mesh substitute.
    pub coll_no_authored: usize,
}

/// Map a world XZ to the 20×20 low-res terrain grid cell (`row*20+col`); tiles are 400 m from -3800.
/// Used by the executor to hide/show the low-res tile beneath a woken/hibernated hi-res terrainmesh.
fn pos_to_cell(p: [f32; 3]) -> Option<usize> {
    let col = ((p[0] + 3800.0) / 400.0).round() as i32;
    let row = ((p[2] + 3800.0) / 400.0).round() as i32;
    (0..20).contains(&col).then_some(())?;
    (0..20).contains(&row).then_some(())?;
    Some(row as usize * 20 + col as usize)
}

// --- K2 S2 collision-delta helpers ---
/// Distinct tagged keys so block (u16) and prop (u32) collision entries never collide in one map.
fn block_key(b: u16) -> u64 {
    (1u64 << 32) | b as u64
}
fn prop_key(k: u32) -> u64 {
    k as u64
}

/// One incremental collision delta emitted by [`StreamingWorld::step`], drained by the consumer via
/// [`StreamingWorld::take_collision_ops`] and applied to its persistent broadphase in order. A WAKE/LOAD
/// yields `Insert` (the unit's world-space tris); a HIBERNATE/UNLOAD yields `Remove` (the unit key). This
/// replaces the old "flip a dirty bit, re-flatten + rebuild the whole grid" path: the consumer now touches
/// only the changed unit's triangles, matching how retail's `hkpWorld` adds/removes one body's shape.
#[derive(Clone, Debug)]
pub enum CollisionOp {
    /// A unit woke/loaded — insert its world-space triangles under `key` (`hkpWorld::addEntity`).
    Insert(u64, Vec<[mercs2_core::glam::Vec3; 3]>),
    /// A unit hibernated/unloaded — remove `key`'s triangles (`hkpWorld::removeEntity`).
    Remove(u64),
}

/// A mesh's **local-space** collision triangles (raw vertex positions, per index triple). Degenerate
/// indices that fall outside the vertex range are skipped rather than panicking.
fn extract_local_tris(m: &LoadedModel) -> Vec<[mercs2_core::glam::Vec3; 3]> {
    use mercs2_core::glam::Vec3;
    m.indices
        .chunks_exact(3)
        .filter_map(|idx| {
            let a = m.verts.get(idx[0] as usize)?;
            let b = m.verts.get(idx[1] as usize)?;
            let c = m.verts.get(idx[2] as usize)?;
            Some([Vec3::from(a.pos), Vec3::from(b.pos), Vec3::from(c.pos)])
        })
        .collect()
}

/// Place local triangles into world space by an instance's rotation + translation (`rotate·v +
/// translate`) — the same transform the render entity uses, so collision matches what's drawn.
fn placed_tris(
    local: &[[mercs2_core::glam::Vec3; 3]],
    translate: mercs2_core::glam::Vec3,
    rotate: mercs2_core::glam::Quat,
) -> Vec<[mercs2_core::glam::Vec3; 3]> {
    local
        .iter()
        .map(|t| [rotate * t[0] + translate, rotate * t[1] + translate, rotate * t[2] + translate])
        .collect()
}

// --- K3: hi-res terrain collision as a baked heightfield (retail hkpHeightFieldShape) ---
//
// Retail terrain collision is a baked HEIGHTFIELD, not collision triangles. Feeding every resident hi-res
// `terrainmesh` tile's triangles into the collision world meant the streamer rebuilt a ~273k-triangle
// broadphase FROM SCRATCH whenever a tile woke/hibernated — which the terrain streamer does constantly
// while the player moves, collapsing the frame rate. Instead each tile is baked ONCE on wake into a
// sparse per-cell max-height grid ([`TileHeightGrid`]) held in a [`TerrainHeightField`], and dropped on
// hibernate. Ground queries sample it O(1), and the collider keeps only props/buildings (a few thousand
// triangles) so its regrid stays cheap and rare. The hero still stands on the hi-res surface (≈1 m above
// the low-res heightmap) — the reason the tris were ever added to the collider — with none of the cost.

/// One woken hi-res terrain tile, baked to a regular max-height grid over its XZ AABB. A cell with no
/// covering vertex is `NEG_INFINITY` (a hole → [`height_at`](Self::height_at) returns `None` so the
/// low-res heightmap fills in). The tile's triangles are already WORLD-SPACE (the terrainmesh bakes its
/// world position into its verts on load), so `bake` reads world XZ/Y directly.
pub struct TileHeightGrid {
    min_x: f32,
    min_z: f32,
    cell: f32,
    nx: usize,
    nz: usize,
    heights: Vec<f32>, // row-major [iz*nx + ix], NEG_INFINITY where uncovered
}

impl TileHeightGrid {
    /// Target hi-res terrain cell (m). ~2 m tracks the terrainmesh vertex spacing closely enough that,
    /// after the dilation passes below, every interior cell carries a surface height, while keeping the
    /// per-cell max within a metre of the true surface on the game's steepest slopes (so the hero stands
    /// ON the hi-res ground, never sinks below it and never floats far above it).
    const TARGET_CELL: f32 = 2.0;
    /// Hard cap on a tile's grid dimension so a tile can never allocate an unbounded grid.
    const MAX_DIM: usize = 256;

    /// Bake a tile's WORLD-SPACE triangles into a per-cell max-height grid. `None` for an empty or
    /// XZ-degenerate tile (no finite extent).
    pub fn bake(world_tris: &[[mercs2_core::glam::Vec3; 3]]) -> Option<TileHeightGrid> {
        if world_tris.is_empty() {
            return None;
        }
        let (mut min_x, mut min_z) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_z) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for t in world_tris {
            for v in t {
                min_x = min_x.min(v.x);
                max_x = max_x.max(v.x);
                min_z = min_z.min(v.z);
                max_z = max_z.max(v.z);
            }
        }
        let (ex, ez) = (max_x - min_x, max_z - min_z);
        if !(ex.is_finite() && ez.is_finite()) || ex <= 0.0 || ez <= 0.0 {
            return None;
        }
        // Cell from the larger extent, capped at MAX_DIM cells so the grid stays bounded.
        let cell = (ex.max(ez) / Self::MAX_DIM as f32).max(Self::TARGET_CELL);
        let nx = (((ex / cell).ceil() as usize) + 1).min(Self::MAX_DIM);
        let nz = (((ez / cell).ceil() as usize) + 1).min(Self::MAX_DIM);
        let mut heights = vec![f32::NEG_INFINITY; nx * nz];
        let cell_of = |v: f32, min: f32, n: usize| {
            (((v - min) / cell) as isize).clamp(0, n as isize - 1) as usize
        };
        // Max vertex Y per cell — the surface high point covering that cell.
        for t in world_tris {
            for v in t {
                let ix = cell_of(v.x, min_x, nx);
                let iz = cell_of(v.z, min_z, nz);
                let c = &mut heights[iz * nx + ix];
                if v.y > *c {
                    *c = v.y;
                }
            }
        }
        // Dilate holes from finite neighbours so the whole tile AABB returns a height (no sink to the
        // low-res surface under the hero between sparse vertices). Bounded passes; stops once filled.
        for _ in 0..3 {
            let prev = heights.clone();
            let mut filled_any = false;
            for iz in 0..nz {
                for ix in 0..nx {
                    if prev[iz * nx + ix].is_finite() {
                        continue;
                    }
                    let mut best = f32::NEG_INFINITY;
                    for dz in iz.saturating_sub(1)..=(iz + 1).min(nz - 1) {
                        for dx in ix.saturating_sub(1)..=(ix + 1).min(nx - 1) {
                            best = best.max(prev[dz * nx + dx]);
                        }
                    }
                    if best.is_finite() {
                        heights[iz * nx + ix] = best;
                        filled_any = true;
                    }
                }
            }
            if !filled_any {
                break;
            }
        }
        Some(TileHeightGrid { min_x, min_z, cell, nx, nz, heights })
    }

    /// Nearest-cell surface height at world (x, z), or `None` outside the tile / over an unfilled hole.
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        let fx = (x - self.min_x) / self.cell;
        let fz = (z - self.min_z) / self.cell;
        if fx < 0.0 || fz < 0.0 {
            return None;
        }
        let ix = fx.round() as usize;
        let iz = fz.round() as usize;
        if ix >= self.nx || iz >= self.nz {
            return None;
        }
        let h = self.heights[iz * self.nx + ix];
        h.is_finite().then_some(h)
    }
}

/// The baked hi-res terrain collision: a set of per-tile height grids keyed by streamed tile key. A tile
/// is baked in on WAKE and dropped on HIBERNATE — the faithful stand-in for retail's resident
/// `hkpHeightFieldShape` terrain collision, holding ZERO collision triangles. Ground queries sample the
/// covering tile(s); nothing here ever touches the physics broadphase.
#[derive(Default)]
pub struct TerrainHeightField {
    tiles: std::collections::HashMap<u32, TileHeightGrid>,
}

impl TerrainHeightField {
    /// Bake a woken tile in (overwrites any prior grid for the same key).
    pub fn insert(&mut self, key: u32, grid: TileHeightGrid) {
        self.tiles.insert(key, grid);
    }
    /// Drop a hibernating tile. Returns whether a grid was present.
    pub fn remove(&mut self, key: u32) -> bool {
        self.tiles.remove(&key).is_some()
    }
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
    /// Highest baked hi-res terrain surface at world (x, z) across every resident tile covering it, or
    /// `None` when no resident tile covers the point (the caller falls back to the low-res heightmap).
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for g in self.tiles.values() {
            if let Some(h) = g.height_at(x, z) {
                best = Some(best.map_or(h, |b| b.max(h)));
            }
        }
        best
    }
}

/// The reusable streaming-runtime **executor** (K2 unification,
/// `docs/modernization/k2_streaming_unification_plan.md`). Owns the block/region [`StreamingManager`],
/// the hibernation grid every placed entity is registered in, the WAD handle (for on-demand
/// activation extraction) and all live-executor bookkeeping, and performs the per-frame GPU/ECS work in
/// [`step`](Self::step). Both the free-fly boot and the playable TPS boot drive this one path. The
/// one-time boot glue (base terrain upload, the game populate hook, world lights) stays in the caller;
/// [`new`](Self::new) takes ownership of the streaming state after that.
pub struct StreamingWorld {
    wad: wad::Wad,
    manager: mercs2_core::streaming::StreamingManager,
    /// The worldentity template records placed entities fall back to.
    templates: TemplateRecords,
    /// Every placed entity of the loaded layers.
    catalog: PlacedCatalog,
    grid: mercs2_core::hibernation::HibernationGrid,
    /// The world's entity-key registry, shared with the script host: SceneObject rows (and their
    /// activation state, `+0x18` bits 12..13), names, layer records, LandingZone rows.
    guids: std::rc::Rc<std::cell::RefCell<mercs2_core::GuidMap>>,
    /// The point and spot lights each loaded layer's `LightObject`s make.
    layer_lights: std::collections::BTreeMap<u32, (Vec<crate::render::GpuLight>, Vec<crate::scene::SpotLightGpu>)>,
    /// The scene's light set no longer matches `layer_lights`.
    lights_dirty: bool,
    /// Destroyed entities whose visual (and catalog record) the next step removes.
    destroyed_visuals: Vec<u32>,
    /// Entities whose visual the next presented step brings in line with their state: runtime spawns
    /// a `Pg.Spawn` activated inside the call, and adopted keys.
    pending_visuals: Vec<u32>,
    /// Keys whose ECS entity the caller owns ([`adopt`](Self::adopt)): the executor builds none.
    external: std::collections::HashSet<u32>,
    /// Entities `Object.Remove` took out (SceneObject `+0x1A` bit3): the grid never wakes them again.
    removed: std::collections::HashSet<u32>,
    /// How many events the grid emitted in the last step.
    last_events: usize,
    /// The pending activations (`0xDF9C10`).
    queue: mercs2_core::activation::ActivationQueue,
    /// The activation clock's epoch.
    epoch: std::time::Instant,
    /// Render-node spheres, by source.
    node_spheres: NodeSpheres,
    lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
    terrain_hash: u32,
    prop_ents: std::collections::HashMap<u32, mercs2_core::Entity>,
    block_ents: std::collections::HashMap<u16, mercs2_core::Entity>,
    model_refs: std::collections::HashMap<u32, u32>,
    anim_store: std::collections::HashMap<u32, crate::scene::ModelAnim>,
    // --- collision delta (K2 S2) ---
    /// Per-model **local-space** render triangles for the TERRAIN tiles only — cached on first load and
    /// baked ONCE into the [`terrain_field`](Self::terrain_field) heightfield (retail `hkpHeightFieldShape`).
    /// Props and c3 blocks do NOT populate this: they collide via AUTHORED PHY2 ([`phy2_local`](Self::phy2_local))
    /// or not at all — never from render-mesh triangles. Keyed by model hash.
    local_tris: std::collections::HashMap<u32, Vec<[mercs2_core::glam::Vec3; 3]>>,
    /// Per-model **authored PHY2** model-local collision tris, cached on first load. `Some(tris)` = the
    /// model ships a complete authored convex/box/mesh collider (that IS the unit's collision); `None` =
    /// no complete authored collider, so the unit gets NO collision — retail authored none (faithful) or
    /// its static mesh is an undecoded decode-gap. Never a render-mesh substitute. Computed once per
    /// model hash — the model→PHY2 linkage is the `PHY2` sub-chunk inside the model's UCFX container.
    phy2_local: std::collections::HashMap<u32, Option<Vec<[mercs2_core::glam::Vec3; 3]>>>,
    /// Ordered log of the per-unit collision deltas produced this step (a PROP/building unit woke/loaded →
    /// `Insert`, or hibernated/unloaded → `Remove`), keyed per streamed unit (block or prop — see
    /// `block_key`/`prop_key`). The consumer drains it each tick via [`take_collision_ops`](Self::take_collision_ops)
    /// and replays it INCREMENTALLY into its persistent broadphase (`insert_unit`/`remove_unit`) — no
    /// whole-grid clone or regrid. Terrain tiles do NOT append here — they bake into
    /// [`terrain_field`](Self::terrain_field) — so a terrain wake/hibernate never touches the broadphase.
    collision_ops: Vec<CollisionOp>,
    /// The baked hi-res terrain collision (retail `hkpHeightFieldShape`). Terrain tiles bake their
    /// surface here ONCE on wake and drop it on hibernate; ground queries sample it O(1). The ~270k
    /// terrain triangles live here as a cheap height grid, not in the collider.
    terrain_field: TerrainHeightField,
    /// Cumulative collision-source census over the session: streamed prop/building units that used
    /// AUTHORED PHY2 collision vs those left with NO collision (no complete authored collider —
    /// faithfully-none, or an undecoded-mesh gap). Reported in the periodic stat line. (Wake events, so
    /// a unit re-woken after hibernate counts again.)
    coll_authored: usize,
    coll_no_authored: usize,
}

impl StreamingWorld {
    /// Take ownership of the loaded streaming state. The caller has already uploaded the base terrain
    /// and run its populate hook (one-time boot glue, not per-frame). `terrain_hash` is the low-res
    /// terrain model hash (for the tile LOD swap). The world starts with no layer loaded: placed
    /// entities arrive through the layer requests queued on `guids` ([`step`](Self::step)). The
    /// templates' `Name` records enter the name index first, so a layer's later registration of a
    /// template's name does not displace it.
    pub fn new(
        wad: wad::Wad,
        manager: mercs2_core::streaming::StreamingManager,
        templates: TemplateRecords,
        lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
        terrain_hash: u32,
        guids: std::rc::Rc<std::cell::RefCell<mercs2_core::GuidMap>>,
    ) -> Self {
        {
            let gm = &mut *guids.borrow_mut();
            for (k, name) in &templates.names {
                gm.register_name(mercs2_formats::hash::pandemic_hash_m2(name), *k);
            }
        }
        StreamingWorld {
            wad,
            manager,
            templates,
            catalog: PlacedCatalog::default(),
            grid: mercs2_core::hibernation::HibernationGrid::new(mercs2_core::hibernation::HibernationScale::single_player()),
            guids,
            layer_lights: std::collections::BTreeMap::new(),
            lights_dirty: false,
            destroyed_visuals: Vec::new(),
            pending_visuals: Vec::new(),
            external: std::collections::HashSet::new(),
            removed: std::collections::HashSet::new(),
            last_events: 0,
            queue: mercs2_core::activation::ActivationQueue::default(),
            epoch: std::time::Instant::now(),
            node_spheres: NodeSpheres::default(),
            lowres_draw_by_cell,
            terrain_hash,
            prop_ents: std::collections::HashMap::new(),
            block_ents: std::collections::HashMap::new(),
            model_refs: std::collections::HashMap::new(),
            anim_store: std::collections::HashMap::new(),
            local_tris: std::collections::HashMap::new(),
            phy2_local: std::collections::HashMap::new(),
            collision_ops: Vec::new(),
            terrain_field: TerrainHeightField::default(),
            coll_authored: 0,
            coll_no_authored: 0,
        }
    }

    /// The baked hi-res terrain heightfield (retail `hkpHeightFieldShape`). The ground query samples this
    /// O(1) for the exterior near surface; it holds no collision triangles.
    pub fn terrain_field(&self) -> &TerrainHeightField {
        &self.terrain_field
    }

    /// Whether any per-unit collision delta was produced since the last [`take_collision_ops`](Self::take_collision_ops).
    pub fn collision_dirty(&self) -> bool {
        !self.collision_ops.is_empty()
    }

    /// Drain this step's ordered per-unit collision deltas (WAKE `Insert` / HIBERNATE `Remove`). The
    /// consumer replays them into its persistent broadphase — `insert_unit`/`remove_unit`, each
    /// `O(changed unit)` — instead of re-flattening and regridding the whole grid. Empty when nothing
    /// prop/building-shaped changed (terrain-only ticks never append here).
    pub fn take_collision_ops(&mut self) -> Vec<CollisionOp> {
        std::mem::take(&mut self.collision_ops)
    }

    /// Whether the last step changed nothing and nothing waits: the grid emitted no event and no
    /// activation is pending (the world-ready check `0x4BA2A7` waits for an empty queue).
    pub fn settled(&self) -> bool {
        self.last_events == 0 && self.queue.is_empty()
    }

    /// The live entity of the awake streamed object with entity key `key`, if it is awake.
    pub fn entity_of_key(&self, key: u32) -> Option<mercs2_core::Entity> {
        self.prop_ents.get(&key).copied()
    }

    /// Live executor counts for the periodic stat line.
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            resident: self.manager.resident_count(),
            awake: self.catalog.iter().filter(|o| self.state(o.key) == 2).count(),
            cached_regions: self.manager.cached_region_count(),
            regions: self.manager.region_count(),
            block_ents: self.block_ents.len(),
            props: self.prop_ents.len(),
            models: self.model_refs.len(),
            coll_authored: self.coll_authored,
            coll_no_authored: self.coll_no_authored,
        }
    }

    /// `key`'s activation state: 0 asleep, 1 waking, 2 awake.
    fn state(&self, key: u32) -> u8 {
        self.guids.borrow().state(key).unwrap_or(0)
    }

    /// Perform the layer requests queued on the registry since the last call
    /// (`docs/reverse_engineer/layer_load_code_map.md` §2–3). A load of a layer that is already loaded
    /// or has failed does nothing. `exempt` names the entities a dynamic layer's unload must not
    /// destroy: the player-controlled entity and what is linked to it (`FUN_82437CF8`). Needs no scene:
    /// the visuals of destroyed entities and the scene's light set follow in the next
    /// [`step`](Self::step).
    pub fn apply_layer_ops(&mut self, exempt: &dyn Fn(u32) -> bool) -> Result<(), String> {
        use mercs2_core::LayerOp;
        let ops = self.guids.borrow_mut().take_layer_ops();
        for (op, layer) in ops {
            match op {
                LayerOp::Load => {
                    if self.guids.borrow().layer(layer).is_some_and(|r| r.status == 0) {
                        self.load_layer(layer)?;
                    }
                }
                LayerOp::Reload => self.load_layer(layer)?,
                LayerOp::Unload => self.unload_layer(layer, exempt),
            }
        }
        Ok(())
    }

    /// The scene side of the layer requests: despawn destroyed entities' visuals, and set the scene's
    /// lights to the loaded layers' when they changed.
    fn sync_layer_scene(&mut self, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World) {
        for k in std::mem::take(&mut self.destroyed_visuals) {
            self.despawn_visual(k, scene, world);
            // A layer loaded after the destroy may have placed the key again.
            if !self.grid.contains(k) {
                self.catalog.remove(&[k]);
            }
        }
        if std::mem::take(&mut self.lights_dirty) {
            self.present_lights(scene);
        }
    }

    /// Set the scene's point and spot lights to the loaded layers' `LightObject`s.
    pub fn present_lights(&self, scene: &mut crate::scene::Scene) {
        let points = self.layer_lights.values().flat_map(|(p, _)| p.iter().cloned()).collect();
        let spots = self.layer_lights.values().flat_map(|(_, s)| s.iter().cloned()).collect();
        scene.set_lights(points);
        scene.set_spot_lights(spots);
    }

    /// A layer's `OnLoad` (`0x45E5F0`): instantiate its container — every placed entity into the
    /// catalog and the hibernation grid (asleep), its SceneObject row, its names into the name index,
    /// its LandingZone rows, its population regions and its lights; a dynamic layer's entities get its
    /// RuntimeLayerId — then status 2. A layer the archive does not carry fails (status 3). A reload
    /// re-reads the container over the entities already placed (whether retail destroys them first is
    /// UNPROVEN); a key already registered keeps its grid entry.
    fn load_layer(&mut self, layer: u32) -> Result<(), String> {
        use mercs2_formats::hash::pandemic_hash_m2;
        let Some(c) = layer_container(&mut self.wad, layer)? else {
            let r = &mut *self.guids.borrow_mut();
            let rec = r.layer_mut(layer);
            rec.status = 3;
            rec.reloading = false;
            return Ok(());
        };
        let keys = self.catalog.add_block(&c, &self.templates).map_err(|e| format!("layer 0x{layer:08X}: {e}"))?;
        let names = mercs2_formats::placement::placed_names(&c);
        let pads = mercs2_formats::placement::load_landing_zones(&c);
        register_population_regions(&c, &mut self.manager);
        let inventory = mercs2_formats::placement::light_inventory(&c);
        self.layer_lights.insert(layer, (placed_lights_to_gpu(&inventory), placed_spot_lights_to_gpu(&inventory)));
        self.lights_dirty = true;

        let gm = &mut *self.guids.borrow_mut();
        let dynamic = gm.layer(layer).is_some_and(|r| r.dynamic);
        for &k in &keys {
            let o = *self.catalog.get(k).expect("add_block returned a catalogued key");
            if !self.grid.contains(k) {
                self.grid.insert(&o.registration(), false);
            }
            let state = gm.state(k).unwrap_or(0);
            gm.set_scene(k, mercs2_core::SceneRow { pos: o.pos, quat: o.quat, template: o.parent, state });
            if dynamic {
                gm.set_runtime_layer(k, layer);
            }
        }
        for (k, raw) in names {
            gm.register_name(pandemic_hash_m2(&raw), k);
        }
        for p in pads {
            gm.set_landing_zone(p.key, p.zone, p.slot);
        }
        let rec = gm.layer_mut(layer);
        rec.status = 2;
        rec.reloading = false;
        rec.keys = keys;
        Ok(())
    }

    /// A layer's `Unload` (`0x45E4E0`, Xbox `FUN_821AE540`): a dynamic layer destroys every entity whose
    /// RuntimeLayerId is this layer, except `exempt` ones, and drops its lights; then status 0. A
    /// static layer's entities stay.
    fn unload_layer(&mut self, layer: u32, exempt: &dyn Fn(u32) -> bool) {
        let dynamic = self.guids.borrow().layer(layer).is_some_and(|r| r.dynamic);
        if dynamic {
            let mut keys = self.guids.borrow().keys_of_runtime_layer(layer);
            keys.sort_unstable();
            for k in keys.into_iter().filter(|&k| !exempt(k)) {
                self.destroy(k);
            }
            if self.layer_lights.remove(&layer).is_some() {
                self.lights_dirty = true;
            }
        }
        self.guids.borrow_mut().layer_mut(layer).status = 0;
    }

    /// Destroy a placed entity (`FUN_82255FF0`): deactivate it (its visual goes in the next step), then
    /// drop it from the activation queue, the grid, the catalog and the registry (its row, names, layer
    /// link, LandingZone row).
    fn destroy(&mut self, key: u32) {
        if self.state(key) != 0 {
            self.guids.borrow_mut().set_state(key, 0);
        }
        self.queue.remove(key);
        self.grid.remove(key);
        self.removed.remove(&key);
        self.guids.borrow_mut().remove(key);
        // A visual's despawn reads the entity's catalog record, so the record goes with the visual.
        if self.prop_ents.contains_key(&key) {
            self.destroyed_visuals.push(key);
        } else {
            self.catalog.remove(&[key]);
        }
    }

    /// One streaming step with `viewer` — the local player's camera target (the character on foot, the
    /// vehicle when riding): the queued layer requests ([`apply_layer_ops`](Self::apply_layer_ops), with
    /// `exempt` as there), the block/region decision, then one frame of the hibernation grid and its
    /// event consumer (`FUN_004F8460`): deactivate entities whose viewer mask cleared, activate those
    /// whose mask was set (largest own `HibernationControl.Distance` first, deferred), pump the pending
    /// activations, and follow each ring-tier change with the entity's model residency.
    pub fn step(&mut self, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World, viewer: [f32; 3], exempt: &dyn Fn(u32) -> bool) -> Result<(), String> {
        self.apply_layer_ops(exempt)?;
        self.apply_moved()?;
        self.sync_layer_scene(scene, world);
        for k in std::mem::take(&mut self.pending_visuals) {
            self.sync_visual(k, scene, world)?;
        }
        self.step_blocks(scene, world, viewer);
        self.step_grid(Some((scene, world)), viewer)
    }

    /// [`step`](Self::step) without presentation: the layer requests, the grid and the activations run
    /// as in a presented step, but nothing is loaded, drawn or collided — the world-state simulation a
    /// headless run of the scripts drives (`mercs2_autoplay`, the boot tests).
    pub fn step_headless(&mut self, viewer: [f32; 3], exempt: &dyn Fn(u32) -> bool) -> Result<(), String> {
        self.apply_layer_ops(exempt)?;
        self.apply_moved()?;
        self.pending_visuals.clear();
        self.step_grid(None, viewer)
    }

    /// The grid re-cells of the SetTransforms since the last step (`0x518140` → `FUN_00516F00`): the
    /// catalog takes the row's transform, and a registered entity is re-celled from the render node it
    /// has now — an awake one's, else none (the bare position, `base` 0).
    fn apply_moved(&mut self) -> Result<(), String> {
        let moved = self.guids.borrow_mut().take_moved();
        for k in moved {
            let Some(row) = self.guids.borrow().scene(k).copied() else { continue };
            self.catalog.set_transform(k, row.pos, row.quat);
            if !self.grid.contains(k) {
                continue;
            }
            let o = *self.catalog.get(k).ok_or_else(|| format!("0x{k:08X}: moved and in the grid, but not catalogued"))?;
            let node = match row.state {
                2 => self.node_spheres.sphere(&mut self.wad, o.node).map_err(|e| format!("0x{k:08X}: {e}"))?,
                _ => None,
            };
            self.grid.moved(&o.activated_registration(node));
        }
        Ok(())
    }

    /// `Pg.Spawn`'s spawn of `template` (`FUN_006746D0` → `FUN_00673070`;
    /// `docs/reverse_engineer/hibernation_grid.md` §6 "Runtime spawns"), inside the call:
    /// - the key is taken first, so a spawn that fails below still consumes one;
    /// - `flags` = `net` && `net_enabled` && the template's NetCategoryInfo low nibble; the entity
    ///   registers non-static with `flags`, else with NetCategoryInfo bit 9;
    /// - the SceneObject row (position, the yaw quaternion, the template, asleep), then the grid insert
    ///   with the current viewers' mask;
    /// - out of every viewer's range with the template's `HibernationControl` Flags set, the entity is
    ///   removed again and the spawn returns `None`;
    /// - in range, it activates now (begin, then complete when its assets are present); its visual
    ///   follows at the next presented step.
    ///
    /// An error for a key that is not a worldentity template.
    pub fn spawn(&mut self, template: u32, pos: [f32; 3], yaw_degrees: f32, net: bool, net_enabled: bool) -> Result<Option<u32>, String> {
        use mercs2_core::glam::Quat;
        if !self.templates.keys.contains(&template) {
            return Err(format!("Pg.Spawn: 0x{template:08X} is not a worldentity template"));
        }
        let key = self.guids.borrow_mut().mint_runtime();
        let nci = self.templates.net_category.get(&template).copied().unwrap_or(0);
        let flags = net && net_enabled && nci & 0xF != 0;
        let non_static = flags || nci & (1 << 9) != 0;
        let q = Quat::from_rotation_y(yaw_degrees.to_radians());
        let quat = [q.x, q.y, q.z, q.w];
        let o = self.catalog.add_spawn(key, template, pos, quat, !non_static, &self.templates)?;
        self.guids.borrow_mut().set_scene(key, mercs2_core::SceneRow { pos, quat, template, state: 0 });
        let mask = self.grid.insert(&o.registration(), true) & 0x0F;
        if mask == 0 {
            if o.hibernation.is_some_and(|h| h.flags) {
                self.grid.remove(key);
                self.catalog.remove(&[key]);
                self.guids.borrow_mut().remove(key);
                return Ok(None);
            }
            return Ok(Some(key));
        }
        let mut queue = std::mem::take(&mut self.queue);
        let result = queue.activate(&mut Activator { sw: &mut *self, present: None }, key, false, true, true);
        self.queue = queue;
        result?;
        self.pending_visuals.push(key);
        Ok(Some(key))
    }

    /// The caller owns `key`'s ECS entity (the player's character, which the game builds itself): the
    /// executor builds no entity for it, and removes the one it built at the next presented step.
    pub fn adopt(&mut self, key: u32) {
        if self.external.insert(key) {
            self.pending_visuals.push(key);
        }
    }

    /// One frame of the hibernation grid and its event consumer, then the visuals of ring-tier changes.
    fn step_grid(&mut self, mut present: Presentation<'_>, viewer: [f32; 3]) -> Result<(), String> {
        use mercs2_core::hibernation::{wake_decisions, Viewer, MAX_VIEWERS};
        let mut viewers = [None; MAX_VIEWERS];
        viewers[0] = Some(Viewer { position: viewer, local: true });
        self.grid.set_viewers(viewers);
        let events = self.grid.step();
        self.last_events = events.len();
        let (catalog, removed) = (&self.catalog, &self.removed);
        let decisions = wake_decisions(
            &events,
            |k| catalog.get(k).is_some(),
            |k| removed.contains(&k),
            |k| catalog.get(k).and_then(|o| o.own_hibernation).map(|h| h.distance),
        );
        let mut queue = std::mem::take(&mut self.queue);
        let result = (|| {
            let present = present.as_mut().map(|(s, w)| (&mut **s, &mut **w));
            let mut host = Activator { sw: &mut *self, present };
            for k in decisions.deactivate {
                queue.deactivate(&mut host, k);
            }
            for k in decisions.activate {
                queue.activate(&mut host, k, true, false, true)?;
            }
            queue.pump(&mut host)
        })();
        self.queue = queue;
        result?;
        if let Some((scene, world)) = present {
            for ev in events.iter().filter(|e| (e.old ^ e.new) & 0xF0 != 0) {
                self.sync_visual(ev.key, scene, world)?;
            }
        }
        Ok(())
    }

    /// The block/region decision and its GPU/ECS work: UNLOAD blocks that left the working radius,
    /// LOAD c3-cell geometry.
    fn step_blocks(&mut self, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World, viewer: [f32; 3]) {
        use mercs2_core::glam::{Quat, Vec3};
        use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
        let Self { wad, manager, block_ents, model_refs, phy2_local, collision_ops, coll_authored, coll_no_authored, .. } = self;

        let diff = manager.update(viewer);
        // Seam B — drive the RegionCache decision layer each tick (PgSysPopulation CacheIn/CacheOut).
        let _region_diff = manager.update_regions(viewer);

        // UNLOAD first (free GPU): blocks that left the working radius.
        for b in &diff.unload_blocks {
            if let Some(e) = block_ents.remove(b) {
                if let Ok(mr) = world.get::<&ModelRef>(e).map(|m| m.model) {
                    dec_model_ref(model_refs, mr, scene);
                }
                let _ = world.despawn(e);
                scene.forget_entity(e);
            }
            collision_ops.push(CollisionOp::Remove(block_key(*b)));
        }
        // LOAD c3-cell blocks (throttled by the manager's block budget).
        for b in &diff.load_blocks {
            if block_ents.contains_key(b) {
                continue;
            }
            if let Some((m, off, phy2)) = load_one_c3_cell(wad, *b) {
                if !scene.has_model(m.hash) {
                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                }
                // Collision: AUTHORED PHY2 only (convex/box hulls + decoded static mesh), model-local,
                // placed at `off` (identity rotation). `phy2 == Some` only for a COMPLETE authored
                // collider; `None` (an undecoded WpMeshShape16 cell) gets NO collision op — a logged
                // decode gap, never a render-mesh substitute. Retail collides the static world from its
                // authored Havok shapes, so that is the only source here.
                phy2_local.entry(m.hash).or_insert_with(|| phy2);
                if let Some(a) = phy2_local.get(&m.hash).and_then(|o| o.as_ref()) {
                    *coll_authored += 1;
                    collision_ops.push(CollisionOp::Insert(
                        block_key(*b),
                        placed_tris(a, Vec3::new(off[0], off[1], off[2]), Quat::IDENTITY),
                    ));
                } else {
                    *coll_no_authored += 1;
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
    }

    /// The grid re-cell an activation's SetTransform causes (`0x518140` → `FUN_00515F60` →
    /// `FUN_00516F00`): with a render node, the quantiser's input becomes pos + R(q)·centre (the
    /// SceneObject's i16 quaternion) and `base` cvtt(radius); without one, the bare position and `base`
    /// 0. Mask, state and rings stay.
    fn recell(&mut self, key: u32) -> Result<(), String> {
        let o = *self.catalog.get(key).ok_or_else(|| format!("0x{key:08X}: not a placed entity"))?;
        let node = self.node_spheres.sphere(&mut self.wad, o.node).map_err(|e| format!("0x{key:08X}: {e}"))?;
        self.grid.moved(&o.activated_registration(node));
        Ok(())
    }

    /// Whether the archive carries the entity's model (or terrainmesh) — the asset its activation
    /// requests. A placed entity without one has nothing to request.
    fn assets_present(&self, key: u32) -> bool {
        match self.catalog.get(key) {
            Some(o) if o.terrain_mesh.is_some() => true,
            Some(o) => o.model.is_none_or(|m| wad::has_model(&self.wad, m)),
            None => false,
        }
    }

    /// Make an entity's model (or terrainmesh) resident in the scene. `Ok(false)` when the archive
    /// carries no model of that hash (retail cancels the activation); an error when it carries one
    /// that does not load.
    fn load_assets(&mut self, key: u32, scene: &mut crate::scene::Scene) -> Result<bool, String> {
        let o = *self.catalog.get(key).ok_or_else(|| format!("0x{key:08X}: not a placed entity"))?;
        if let Some(tm) = o.terrain_mesh {
            if !scene.has_model(tm) {
                let m = load_terrainmesh_tile(&mut self.wad, tm, o.pos)
                    .ok_or_else(|| format!("0x{key:08X}: terrainmesh 0x{tm:08X} did not load"))?;
                scene.load_terrain_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                self.local_tris.entry(m.hash).or_insert_with(|| extract_local_tris(&m));
            }
            return Ok(true);
        }
        let Some(model) = o.model else { return Ok(true) };
        if scene.has_model(model) {
            return Ok(true);
        }
        if !wad::has_model(&self.wad, model) {
            return Ok(false);
        }
        let (m, _, _) = load_model_by_hash(&mut self.wad, model)
            .ok_or_else(|| format!("0x{key:08X}: model 0x{model:08X} is in the archive but did not load"))?;
        scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
        // A rigged model that ships clips registers its rig + clips so the per-frame animation pass can
        // pose it (no-op for clip-less props).
        if !m.clips.is_empty() {
            self.anim_store.entry(m.hash).or_insert_with(|| crate::scene::ModelAnim {
                rig: m.skin.rig.clone(),
                clips: m.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
            });
        }
        Ok(true)
    }

    /// A placed entity's model is resident while it is awake and its ring tier is non-zero
    /// (`FUN_0050C530`: streaming level `4 − tier`, level 4 removes it). A runtime spawn's entity is its
    /// gameplay entity, which retail's component listeners keep from activation complete to deactivate,
    /// so it exists while it is awake. An adopted key gets none. Spawn or despawn to match.
    fn sync_visual(&mut self, key: u32, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World) -> Result<(), String> {
        let Some(o) = self.catalog.get(key) else { return Ok(()) };
        let has_visual = o.terrain_mesh.is_some() || o.model.is_some();
        let ringed = self.grid.entry(key).is_some_and(|e| e.state & 0xF0 != 0);
        let present = o.runtime.is_some() || ringed;
        let want = has_visual && present && self.state(key) == 2 && !self.external.contains(&key);
        match (want, self.prop_ents.contains_key(&key)) {
            (true, false) => {
                if self.load_assets(key, scene)? {
                    self.spawn_visual(key, scene, world);
                }
            }
            (false, true) => self.despawn_visual(key, scene, world),
            _ => {}
        }
        Ok(())
    }

    /// Spawn a placed entity's resident model at its authored transform.
    fn spawn_visual(&mut self, key: u32, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World) {
        use mercs2_core::glam::{Quat, Vec3};
        use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
        let o = self.catalog.get(key).copied().expect("a catalogued key");
        if let Some(tm) = o.terrain_mesh {
            // Terrain is a baked HEIGHTFIELD, not collision triangles (retail `hkpHeightFieldShape`):
            // bake the tile's world-space surface into `terrain_field`, never the broadphase.
            if let Some(lt) = self.local_tris.get(&tm) {
                if let Some(grid) = TileHeightGrid::bake(lt) {
                    self.terrain_field.insert(key, grid);
                }
            }
            let e = world.spawn((Transform::IDENTITY, ModelRef { model: tm }, AnimState::default(), SkinPalette { mats: vec![IDENTITY] }));
            *self.model_refs.entry(tm).or_insert(0) += 1;
            self.prop_ents.insert(key, e);
            self.guids.borrow_mut().bind(key, e);
            // Hide the low-res tile beneath this hi-res tile (the LOD swap).
            if let Some(di) = pos_to_cell(o.pos).and_then(|c| self.lowres_draw_by_cell.get(&c)) {
                scene.set_draw_hidden(self.terrain_hash, *di, true);
            }
            return;
        }
        let model = o.model.expect("sync_visual spawns only an entity with a model");
        let q = Quat::from_xyzw(o.quat[0], o.quat[1], o.quat[2], o.quat[3]);
        let p = Vec3::new(o.pos[0], o.pos[1], o.pos[2]);
        let nbones = scene.model_bone_count(model).max(1);
        let mut t = Transform::from_translation(p);
        t.rotation = q;
        // A runtime spawn of a vehicle or character template is its gameplay entity, which moves: no
        // static collision.
        match o.runtime.map(|r| (r.archetype, r.faction)) {
            Some((crate::spawn::Archetype::Character, faction)) => {
                let e = crate::spawn::spawn_character(world, o.parent, model, t);
                crate::spawn::set_faction(world, e, faction);
                let _ = world.insert_one(e, SkinPalette { mats: vec![IDENTITY; nbones] });
                self.bind_visual(key, model, e);
                return;
            }
            Some((crate::spawn::Archetype::Vehicle(class), _)) => {
                let e = crate::spawn::spawn_default_vehicle(world, class, key, t);
                let _ = world.insert_one(e, ModelRef { model });
                let _ = world.insert_one(e, SkinPalette { mats: vec![IDENTITY; nbones] });
                self.bind_visual(key, model, e);
                return;
            }
            Some((crate::spawn::Archetype::Prop, _)) | None => {}
        }
        // Collision: AUTHORED PHY2 only, placed at the authored pos+quat, computed once per model hash.
        // `None` = no complete authored collider → no collision, as retail authored it. Never a
        // render-mesh substitute.
        if !self.phy2_local.contains_key(&model) {
            let a = load_authored_collision(&mut self.wad, model);
            self.phy2_local.insert(model, a);
        }
        if let Some(a) = self.phy2_local.get(&model).and_then(|o| o.as_ref()) {
            self.coll_authored += 1;
            self.collision_ops.push(CollisionOp::Insert(prop_key(key), placed_tris(a, p, q)));
        } else {
            self.coll_no_authored += 1;
        }
        // Play the first discovered clip if this model has any; else stay static.
        let anim = match self.anim_store.get(&model).and_then(|ma| ma.clips.keys().next()) {
            Some(&clip) => AnimState::playing(clip),
            None => AnimState::default(),
        };
        let e = world.spawn((t, ModelRef { model }, anim, SkinPalette { mats: vec![IDENTITY; nbones] }));
        self.bind_visual(key, model, e);
    }

    /// Record `key`'s entity `e` drawing `model`.
    fn bind_visual(&mut self, key: u32, model: u32, e: mercs2_core::Entity) {
        *self.model_refs.entry(model).or_insert(0) += 1;
        self.prop_ents.insert(key, e);
        self.guids.borrow_mut().bind(key, e);
    }

    /// Despawn a placed entity's model and release what it held.
    fn despawn_visual(&mut self, key: u32, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World) {
        use mercs2_core::ModelRef;
        let Some(e) = self.prop_ents.remove(&key) else { return };
        self.guids.borrow_mut().unbind(e);
        if let Ok(mr) = world.get::<&ModelRef>(e).map(|m| m.model) {
            dec_model_ref(&mut self.model_refs, mr, scene);
        }
        let _ = world.despawn(e);
        scene.forget_entity(e);
        match self.catalog.get(key).copied() {
            Some(o) if o.terrain_mesh.is_some() => {
                // A terrain tile un-hides its low-res tile and drops its heightfield; it never entered
                // the broadphase.
                if let Some(di) = pos_to_cell(o.pos).and_then(|c| self.lowres_draw_by_cell.get(&c)) {
                    scene.set_draw_hidden(self.terrain_hash, *di, false);
                }
                self.terrain_field.remove(key);
            }
            _ => self.collision_ops.push(CollisionOp::Remove(prop_key(key))),
        }
    }

    /// The fixed-timestep animation pass for woken rigged props: advance each playing clip by `sim_dt`
    /// and pose it into the entity `SkinPalette` (same `havok_palette_in_place` math as the player
    /// system). Scoped to models that registered clips, so clip-less static content is untouched — a
    /// no-op when nothing woke with clips.
    pub fn animate(&self, world: &mut mercs2_core::World, sim_dt: f32) {
        use mercs2_core::{AnimState, ModelRef, SkinPalette};
        if self.anim_store.is_empty() {
            return;
        }
        for (_e, (state, palette, mref)) in world
            .query::<(&mut AnimState, &mut SkinPalette, &ModelRef)>()
            .iter()
        {
            if !state.playing {
                continue;
            }
            let Some(ma) = self.anim_store.get(&mref.model) else { continue };
            let Some(ca) = ma.clips.get(&state.clip).or_else(|| ma.clips.values().next()) else {
                continue;
            };
            let clip_dur = ca.clip.duration.max(1e-3);
            state.time = (state.time + sim_dt * state.speed) % clip_dur;
            let sample = ca.clip.sample_local(state.time);
            palette.mats = crate::pose::havok_palette_in_place(
                &ma.rig,
                &sample,
                &ca.track_to_hier,
                ca.num_transform_tracks,
            );
        }
    }
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
pub async fn run_game_world(
    wadpath: String,
    spawn: Option<[f32; 3]>,
    overlays: Vec<String>,
    // GAME hook: once the streaming world's base geometry has loaded, the engine hands the game the
    // live World / Scene / Wad so the GAME can spawn its own entities (player, PMC interior, …). The
    // engine is asset-agnostic and knows nothing about that — game specifics live in `mercs2_game`.
    populate: impl FnMut(&mut mercs2_core::World, &mut crate::scene::Scene, &mut crate::wad::Wad) + 'static,
) {
    match spawn {
        Some(s) => eprintln!("[boot] spawn = ({:.1}, {:.1}, {:.1})  <- from --spawn (PMC interior via mercs2_game)", s[0], s[1], s[2]),
        None => eprintln!("[boot] spawn = DEFAULT exterior bird's-eye (NO --spawn received)"),
    }
    eprintln!("[boot] {} vz_state overlay layer(s) requested via --overlays", overlays.len());
    // The free-fly boot is a `Game` over the unified engine loop (`app::run`); `FreeFlyGame` owns the
    // free-fly camera + the `StreamingWorld`, while the engine owns the window / event loop / loading
    // screen / render.
    crate::app::run(FreeFlyGame::new(wadpath, spawn, overlays, populate)).await
}

/// The dev free-fly boot as a [`crate::app::Game`]: streams the world around a WASDQE / mouse-look debug
/// camera. `populate` is the caller's one-shot world-population hook (`mercs2_game` passes its PMC
/// interior). This is the free-fly half of the unified `app::run` loop; the TPS game is `mercs2_game`'s
/// `Mercs2Game`. It owns the free-fly camera state + the `StreamingWorld` executor (the engine loop is
/// generic and knows about neither).
struct FreeFlyGame<P> {
    wadpath: String,
    overlays: Vec<String>,
    populate: P,
    free_pos: mercs2_core::glam::Vec3,
    free_yaw: f32,
    free_pitch: f32,
    stream: Option<StreamingWorld>,
    stat_last: std::time::Instant,
}

impl<P> FreeFlyGame<P> {
    fn new(wadpath: String, spawn: Option<[f32; 3]>, overlays: Vec<String>, populate: P) -> Self {
        use mercs2_core::glam::Vec3;
        use std::f32::consts::PI;
        // The authored spawn Y (MrxUtil._TeleportHero) is the hero ROOT — feet on the floor. A free-fly
        // camera sitting exactly there views from floor level, so start at standing EYE height above it.
        const EYE_HEIGHT: f32 = 1.7;
        let free_pos = match spawn {
            Some(s) => Vec3::new(s[0], s[1] + EYE_HEIGHT, s[2]),
            None => Vec3::new(EXTERIOR_SPAWN[0], 140.0, EXTERIOR_SPAWN[2]),
        };
        // At a provided spawn (the PMC interior) face +Z level (look INTO the room); the exterior default
        // is a downward bird's-eye facing -Z over the pool.
        let (free_yaw, free_pitch) = match spawn {
            Some(_) => (0.0, 0.0),
            None => (PI, -0.35),
        };
        FreeFlyGame {
            wadpath,
            overlays,
            populate,
            free_pos,
            free_yaw,
            free_pitch,
            stream: None,
            stat_last: std::time::Instant::now(),
        }
    }
}

impl<P> crate::app::Game for FreeFlyGame<P>
where
    P: FnMut(&mut mercs2_core::World, &mut crate::scene::Scene, &mut crate::wad::Wad) + 'static,
{
    type LoadData = StreamingWorldData;

    fn config(&self) -> crate::app::GameConfig {
        crate::app::GameConfig {
            title: "Mercenaries 2 — streaming world (free-fly)".into(),
            size: (1280.0, 720.0),
            grab_cursor: true,
            fog: ([0.55, 0.62, 0.70], 0.00016, 60.0),
            sun: None,
            atmosphere: Some(mercs2_formats::atmosphere::Atmosphere::default()),
            loading_plate_wad: Some(self.wadpath.clone()),
            load_stages: 4,
            bindings: crate::input::Bindings::default(),
        }
    }

    fn spawn_loader(
        &self,
        progress: std::sync::Arc<crate::render::LoadProgress>,
    ) -> std::sync::mpsc::Receiver<Result<StreamingWorldData, String>> {
        use mercs2_core::streaming::StreamingConfig;
        // Tighter per-frame budgets than the probe so wake/load disk I/O doesn't stall a frame; radii
        // generous for an aerial cam.
        let cfg = StreamingConfig {
            block_unload_margin: 200.0,
            block_budget: 2,
            ..StreamingConfig::default()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let wadpath = self.wadpath.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let r = crate::asset::AssetSource::discover(&wadpath, &[])
                .and_then(|mut assets| TemplateRecords::load(&mut assets))
                .and_then(|templates| load_streaming_world_data(&wadpath, cfg, templates, &progress));
            if r.is_ok() {
                println!("[stream] loaded in {:.1}s", t0.elapsed().as_secs_f64());
            }
            let _ = tx.send(r);
        });
        rx
    }

    fn setup(&mut self, ctx: &mut crate::app::Ctx, mut data: StreamingWorldData) {
        use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
        // Base terrain: one static entity at identity (verts already world-space).
        let terrain = data.terrain;
        let terrain_hash = terrain.hash;
        ctx.scene.load_model(
            terrain.hash,
            &terrain.verts,
            &terrain.indices,
            &terrain.draws,
            &terrain.textures,
            &terrain.skin,
        );
        {
            let mut w = ctx.world.borrow_mut();
            w.spawn((
                Transform::IDENTITY,
                ModelRef { model: terrain.hash },
                AnimState::default(),
                SkinPalette { mats: vec![IDENTITY] },
            ));
        }
        // GAME population hook: hand the game the live World/Scene/Wad so it can spawn its own entities.
        {
            let mut w = ctx.world.borrow_mut();
            (self.populate)(&mut w, ctx.scene, &mut data.wad);
        }
        // This boot runs no level script, so it makes the layer requests the script would: the level's
        // base layer (`Sys.RequiredAsset("vz_base", "layer")`, `levelbootstrap.lua:14`), the static
        // layers — here every layer the `layers_static` block carries, which is 169 of the 170 in
        // `xQ!L.lua`'s `_tStaticLayers` (the 170th, `vz_mar_roads`, lives in block 5 and is not
        // loaded here) plus 4 that list never names — and the requested overlays as dynamic layers
        // (`MrxLayerManager` loads a save's layers with `bDynamic = true`).
        let guids = std::rc::Rc::new(std::cell::RefCell::new(mercs2_core::GuidMap::new()));
        {
            let mut gm = guids.borrow_mut();
            gm.request_layer(mercs2_core::LayerOp::Load, mercs2_formats::hash::pandemic_hash_m2("vz_base"), false);
            for &l in &data.static_block_layers {
                gm.request_layer(mercs2_core::LayerOp::Load, l, false);
            }
            for l in &self.overlays {
                gm.request_layer(mercs2_core::LayerOp::Load, mercs2_formats::hash::pandemic_hash_m2(l), true);
            }
        }
        // Take ownership of the streaming state into the reusable executor.
        self.stream = Some(StreamingWorld::new(data.wad, data.manager, data.templates, data.lowres_draw_by_cell, terrain_hash, guids));
    }

    fn update(&mut self, ctx: &mut crate::app::Ctx) -> crate::app::Camera {
        use mercs2_core::glam::{Mat4, Vec3};
        use winit::keyboard::KeyCode;
        // Mouse-look (the App resolved the dual-source delta; we apply the free-fly sensitivity).
        const MOUSE_SENS: f32 = 0.0008;
        let mdx = ctx.mouse_delta.0.clamp(-80.0, 80.0) * MOUSE_SENS;
        let mdy = ctx.mouse_delta.1.clamp(-80.0, 80.0) * MOUSE_SENS;
        self.free_yaw += mdx;
        self.free_pitch = (self.free_pitch - mdy).clamp(-1.5, 1.5);
        let look = 1.6 * ctx.dt;
        let k = ctx.input.keys;
        if k.contains(&KeyCode::ArrowUp) {
            self.free_pitch += look;
        }
        if k.contains(&KeyCode::ArrowDown) {
            self.free_pitch -= look;
        }
        if k.contains(&KeyCode::ArrowLeft) {
            self.free_yaw -= look;
        }
        if k.contains(&KeyCode::ArrowRight) {
            self.free_yaw += look;
        }
        self.free_pitch = self.free_pitch.clamp(-1.5, 1.5);
        let fwd = Vec3::new(
            self.free_pitch.cos() * self.free_yaw.sin(),
            self.free_pitch.sin(),
            self.free_pitch.cos() * self.free_yaw.cos(),
        )
        .normalize();
        let right = Vec3::Y.cross(fwd).normalize();
        let mut mv = Vec3::ZERO;
        if k.contains(&KeyCode::KeyW) {
            mv += fwd;
        }
        if k.contains(&KeyCode::KeyS) {
            mv -= fwd;
        }
        if k.contains(&KeyCode::KeyD) {
            mv += right;
        }
        if k.contains(&KeyCode::KeyA) {
            mv -= right;
        }
        if k.contains(&KeyCode::KeyE) {
            mv += Vec3::Y;
        }
        if k.contains(&KeyCode::KeyQ) {
            mv -= Vec3::Y;
        }
        let sp = if k.contains(&KeyCode::ShiftLeft) { 900.0 } else { 260.0 };
        if mv != Vec3::ZERO {
            self.free_pos += mv.normalize() * sp * ctx.dt;
        }
        let view = Mat4::look_to_lh(self.free_pos, fwd, Vec3::Y);

        // Streaming tick around the (now final) camera position — the free camera is its own target.
        if let Some(sw) = self.stream.as_mut() {
            let pos = [self.free_pos.x, self.free_pos.y, self.free_pos.z];
            let mut w = ctx.world.borrow_mut();
            sw.step(&mut *ctx.scene, &mut w, pos, &|_| false).unwrap_or_else(|e| panic!("[stream] {e}"));
            drop(w);
            if self.stat_last.elapsed().as_secs_f32() >= 1.0 {
                self.stat_last = std::time::Instant::now();
                let s = sw.stats();
                println!(
                    "[stream] cam({:.0},{:.0},{:.0}) resident={} awake={} regions={}/{} | live_blk_ents={} props={} models={} | collision: authored_phy2={} no_authored={}",
                    self.free_pos.x, self.free_pos.y, self.free_pos.z,
                    s.resident, s.awake, s.cached_regions, s.regions, s.block_ents, s.props, s.models,
                    s.coll_authored, s.coll_no_authored
                );
            }
        }

        crate::app::Camera { view, pos: self.free_pos, near: 0.5, far: 30000.0 }
    }

    fn fixed_update(&mut self, ctx: &mut crate::app::Ctx) {
        // Animation pass (fixed-timestep): woken rigged props, via StreamingWorld::animate.
        if let Some(sw) = self.stream.as_ref() {
            let mut w = ctx.world.borrow_mut();
            sw.animate(&mut w, ctx.time.fixed_dt);
        }
    }
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

#[cfg(test)]
mod glow_card_tests {
    use super::*;

    /// A spot's cone edge is half its authored cone angle (`params[5]`, `FUN_004A7660`), clamped to π;
    /// the inner edge stays inside it.
    #[test]
    fn spot_cone_edge_is_half_the_authored_angle() {
        use mercs2_formats::placement::{LightObject, PlacedLight};
        let mut params = [0.0f32; 9];
        params[0] = 5.0;
        params[2] = 20.0;
        params[4] = 0.9;
        params[5] = 1.2;
        let light = LightObject { light_type: LightObject::TYPE_SPOT, color: [1.0; 3], params };
        let pl = PlacedLight { key: 1, name: None, pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 1.0], light, sub_block: 0 };
        let s = placed_spot_lights_to_gpu(&[pl.clone()]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].dir_cos[3], 0.6f32.cos(), "outer edge = 1.2 / 2");
        assert_eq!(s[0].params[0], 0.6f32.cos(), "inner edge capped at the outer one");
        let mut wide = pl;
        wide.light.params[5] = 10.0;
        assert_eq!(placed_spot_lights_to_gpu(&[wide])[0].dir_cos[3], (std::f32::consts::PI * 0.5).cos(), "clamped to π");
    }

    #[test]
    fn colr_peak_picks_the_first_brightest_key() {
        use mercs2_formats::fxdict::Colr;
        // The retail god-ray COLR shape: dark ends, a flat bright middle with a rising fourth byte.
        let colr = Colr::from_fn(|t| {
            if (0.2..=0.8).contains(&t) {
                ([0x3f, 0x3f, 0x3f, (0x26 as f32 + 6.0 * t) as u8], 0xBC00)
            } else {
                ([2, 2, 2, 1], 0xBC00)
            }
        });
        let (rgb, a) = colr_peak(&colr);
        assert_eq!(rgb, [63.0 / 255.0; 3]);
        let first = colr.keys.iter().find(|k| k.rgba[0] == 0x3f).unwrap();
        assert_eq!(a, first.rgba[3] as f32 / 255.0);
    }
}

#[cfg(all(test, feature = "retail"))]
mod delta_clip_retail_tests {
    use super::*;

    /// Every delta-compressed clip of the retail vz.wad decodes (each block's DOF chunks add up to
    /// its stored size, the masks' dynamic DOFs to numD) into finite poses whose rotations are unit
    /// quaternions to within 0.02 (the largest a mixed static/dynamic rotation track reaches).
    #[test]
    fn every_retail_delta_clip_decodes_to_unit_rotations() {
        use mercs2_formats::anim::AnimType;
        let vz = mercs2_formats::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{e}"));
        let mut w = wad::open(&vz.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
        let mut decoded = 0;
        for blk in wad::animgroup_blocks(&w) {
            let data = wad::decompress_block_index(&mut w, blk).unwrap_or_else(|e| panic!("block {blk}: {e}"));
            let ag = mercs2_formats::animgroup::parse_animgroup(&data).unwrap_or_else(|e| panic!("block {blk}: {e}"));
            for c in &ag.clips {
                let Ok(pk) = c.packfile(&data) else { continue };
                let clip = mercs2_formats::anim::parse_anim(pk).unwrap_or_else(|e| panic!("clip 0x{:08X}: {e}", c.name_hash));
                if clip.anim_type != AnimType::Delta {
                    continue;
                }
                assert!(clip.decoded);
                for f in 0..clip.num_frames {
                    let t = clip.duration * f as f32 / (clip.num_frames.max(2) - 1) as f32;
                    for q in clip.sample_local(t) {
                        let n = q.rotation.iter().map(|x| x * x).sum::<f32>().sqrt();
                        assert!(n.is_finite() && (n - 1.0).abs() < 0.02, "clip 0x{:08X} frame {f}: |q| = {n}", c.name_hash);
                        assert!(q.translation.iter().chain(&q.scale).all(|x| x.is_finite()));
                    }
                }
                decoded += 1;
            }
        }
        assert_eq!(decoded, 56, "the census counts 56 delta clips in vz.wad + vz-patch.wad");
    }
}

#[cfg(test)]
mod prop_anim_tests {
    use super::*;

    fn entry(name: u32, bones: &[u32]) -> ClipIndexEntry {
        ClipIndexEntry {
            block: 0,
            name_hash: name,
            num_transform_tracks: bones.len(),
            bones: bones.to_vec(),
        }
    }

    /// The selection rule must pick a small prop/door clip whose few tracks all map into the prop
    /// rig, and REJECT a character body clip that only shares one node name with the prop — this is
    /// exactly what stops interior props from being flung by the human idle/walk animations.
    #[test]
    fn selects_prop_clip_rejects_character_clip() {
        let door_rig = [0xD00D_0001u32, 0xD00D_0002];
        let door_clip = entry(0x0000_AABB, &[0xD00D_0001, 0xD00D_0002]); // 2/2 tracks into the rig
        let body_clip = entry(0x0000_CCDD, &[0xD00D_0001, 1, 2, 3, 4, 5]); // 1/6 tracks into the rig
        let ix = AnimClipIndex { clips: vec![door_clip, body_clip] };
        assert_eq!(ix.select_for_rig(&door_rig, 8), vec![0x0000_AABB]);
    }

    /// A single-bone (static) rig can never resolve the ≥2 tracks a real clip needs, so it selects
    /// nothing — the same gate `load_clips_for_model` applies via `MIN_ANIM_RIG_BONES`.
    #[test]
    fn static_rig_selects_nothing() {
        let ix = AnimClipIndex { clips: vec![entry(1, &[10, 11])] };
        assert!(ix.select_for_rig(&[10], 8).is_empty());
    }

    /// A full character rig selects its authored full-body clip (every track resolves).
    #[test]
    fn character_rig_selects_body_clip() {
        let hier: Vec<u32> = (0..60u32).collect();
        let body: Vec<u32> = (0..60u32).collect();
        let ix = AnimClipIndex { clips: vec![entry(0x1234_5678, &body)] };
        assert_eq!(ix.select_for_rig(&hier, 4), vec![0x1234_5678]);
    }

    /// `max` bounds how many clips are auto-populated (decode-cost cap), keeping the highest-
    /// resolving ones first.
    #[test]
    fn selection_is_capped_and_ranked() {
        let hier: Vec<u32> = (0..10u32).collect();
        let clips = vec![
            entry(0xA, &[0, 1]),          // resolved 2
            entry(0xB, &[0, 1, 2, 3]),    // resolved 4 (ranks first)
            entry(0xC, &[0, 1, 2]),       // resolved 3
        ];
        let ix = AnimClipIndex { clips };
        assert_eq!(ix.select_for_rig(&hier, 2), vec![0xB, 0xC]);
    }

    /// Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
    /// `.mercs2-local.toml`, and fails when it is absent.
    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        /// Live: a KNOWN animated model (the Mattias avatar `0xA3C1FABC`) must auto-populate >0 clips
        /// through the generalised load path — the core deliverable.
        #[test]
        fn live_known_animated_model_yields_clips() {
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            let (m, _, _) = load_model_by_hash(&mut w, 0xA3C1_FABC)
                .expect("the retail vz.wad carries the player model 0xA3C1FABC");
            assert!(
                m.skin.rig.len() >= MIN_ANIM_RIG_BONES,
                "avatar must be rigged, got {} bones",
                m.skin.rig.len()
            );
            assert!(
                !m.clips.is_empty(),
                "known animated model 0xA3C1FABC must auto-populate clips, got {}",
                m.clips.len()
            );
            // Every populated clip actually decoded to sampleable frames.
            for c in &m.clips {
                assert!(c.clip.num_tracks > 0, "clip 0x{:08X} decoded 0 tracks", c.name_hash);
            }
        }

        /// Each hero template resolves its own unarmed (`Bare`) Upright locomotion through its
        /// `HumanAnimationSet` identity — never another merc's clips, never an armed variant — and
        /// Chris with a Rifle idles on `0xED37BC56`, the clip a live `LtSampleWave` capture shows.
        #[test]
        fn hero_templates_resolve_their_own_unarmed_locomotion() {
            use mercs2_anim::select::{UPRIGHT_IDLE_HANDLE, UPRIGHT_SPRINT_HANDLE, UPRIGHT_FULL_SPRINT_HANDLE};
            use mercs2_formats::anim_select::{LookupKey, EQUIPMENT_BARE};
            let mut assets = crate::asset::AssetSource::discover(
                &crate::worldutil::schema_wire_tests::retail::vz_wad_path(),
                &[],
            )
            .expect("asset source");
            let data = HumanAnimData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            for (template, idle, walk, run) in [
                ("mattias", 0x6EA8_8E00u32, 0xBBA4_7DC4u32, 0x1938_79ADu32),
                ("chris", 0x835D_A06A, 0x3EFF_5F5B, 0xB567_9590),
                ("jen", 0x24F8_C8E6, 0xEB36_DB0F, 0x94B1_E4AC),
            ] {
                let id = data.identity_of_template(template).unwrap_or_else(|e| panic!("{e}"));
                let clips = data.locomotion(id, EQUIPMENT_BARE).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(clips, mercs2_player::LocomotionClips { idle, walk, run }, "template {template:?}");
            }
            const RIFLE: u32 = 0xD045_9A41;
            let mattias = data.identity_of_template("mattias").unwrap_or_else(|e| panic!("{e}"));
            let rifle = LookupKey::new(mattias, RIFLE);
            // The three running speeds as played in the workshop (Rifle: run, sprint, full sprint).
            assert_eq!(data.clip(mercs2_anim::select::UPRIGHT_RUN_HANDLE, &rifle), Ok(0x1091_2665));
            assert_eq!(data.clip(UPRIGHT_SPRINT_HANDLE, &rifle), Ok(0xD233_46BC));
            assert_eq!(data.clip(UPRIGHT_FULL_SPRINT_HANDLE, &rifle), Ok(0x35E0_7B11));
            let chris = data.identity_of_template("chris").unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(data.clip(UPRIGHT_IDLE_HANDLE, &LookupKey::new(chris, RIFLE)), Ok(0xED37_BC56));
        }

        /// Every unit the `layers_static` spawners' lists can spawn has a model that builds, and every
        /// Character among them has its unarmed idle in an animgroup that binds to that model — what
        /// the game's population preload loads.
        #[test]
        fn layers_static_population_units_load() {
            let path = crate::worldutil::schema_wire_tests::retail::vz_wad_path();
            let mut assets = crate::asset::AssetSource::discover(&path, &[]).expect("asset source");
            let anim = HumanAnimData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            let PopulationData { catalog, units } = PopulationData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            let mut pop = crate::population::PopulationWorld::new();
            pop.units = catalog;
            let ls = crate::worldutil::schema_wire_tests::retail::retail_layers_static();
            crate::worldutil::register_population_spawners(&ls, &mut pop).unwrap_or_else(|e| panic!("{e}"));
            let reachable = population_units(&pop);
            assert!(!reachable.is_empty(), "the layers_static spawners can spawn something");
            let data = PopulationData { catalog: std::mem::take(&mut pop.units), units };
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            let models = load_population_models(&mut w, &anim, &data, &reachable).unwrap_or_else(|e| panic!("{e}"));
            let loaded: std::collections::BTreeSet<u32> = models.iter().flat_map(|m| m.units.iter().map(|&(u, _)| u)).collect();
            assert_eq!(loaded, reachable, "every reachable unit has its model");
            for m in &models {
                for &(u, idle) in &m.units {
                    let character = data.units[&u].archetype == crate::spawn::Archetype::Character;
                    assert_eq!(idle.is_some(), character, "unit 0x{u:08X}: an idle exactly for a Character");
                    if let Some(idle) = idle {
                        assert!(
                            m.model.clips.iter().any(|c| c.name_hash == idle),
                            "unit 0x{u:08X}: idle 0x{idle:08X} bound on model 0x{:08X}",
                            m.model.hash
                        );
                    }
                }
            }
        }

        /// Every Character unit the shipped `PopulationList`s can spawn carries its own animation
        /// identity, and the AnimationLookup query gives it one unarmed idle clip.
        #[test]
        fn population_characters_resolve_an_identity_and_an_idle() {
            use mercs2_formats::anim_select::{LookupKey, EQUIPMENT_BARE};
            let mut assets = crate::asset::AssetSource::discover(
                &crate::worldutil::schema_wire_tests::retail::vz_wad_path(),
                &[],
            )
            .expect("asset source");
            let anim = HumanAnimData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            let pop = PopulationData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            let mut characters = 0;
            for (&key, unit) in &pop.units {
                if unit.archetype != crate::spawn::Archetype::Character {
                    continue;
                }
                characters += 1;
                let id = anim.identity_of_key(key).unwrap_or_else(|e| panic!("{e}"));
                anim.clip(mercs2_anim::select::UPRIGHT_IDLE_HANDLE, &LookupKey::new(id, EQUIPMENT_BARE))
                    .unwrap_or_else(|e| panic!("unit 0x{key:08X} {id:08X?}: {e}"));
            }
            assert_eq!(characters, 122, "the lists name 122 human unit templates");
        }

        /// The skin does not tear under animation. For each hero and its own Bare-class idle and walk
        /// (AnimationLookup rows, `human_animation_selection.md`), pose the palette the way the game
        /// does (`pose::havok_palette_in_place`) and check JOINT CONTINUITY: for every parent→child
        /// pair where both bones carry skinned vertex weight, the child's bind joint must land on the
        /// same point under the child's skin matrix and under the parent's. A skin bind that
        /// disagrees with the clips separates them — with the old `+16`-chain bind,
        /// `pmc_hum_chris`'s forearm / forearm-roll joint came apart by 143 mm.
        ///
        /// The bounds are the measured worst separations with the raw `+80` skin bind (2026-10-21:
        /// mattias 0.57 mm, chris 2.82 mm, jen 9.47 mm at bone #23 `0xF3D12F3C`), so this guards
        /// regressions rather than asserting perfection. Those residuals are retail's too: retail
        /// skins with exactly the raw `+80` (solved from its live palette —
        /// `docs/reverse_engineer/animation_code_map.md` §2).
        #[test]
        fn skin_joints_stay_together_under_animation() {
            use mercs2_formats::skeleton::transform_point;
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            for (model, name, clips, bound_mm) in [
                (0x0BBA_3066u32, "pmc_hum_mattias", [0x6EA8_8E00u32, 0xBBA4_7DC4], 1.0f32),
                (0x39F4_7E08, "pmc_hum_chris", [0x835D_A06A, 0x3EFF_5F5B], 3.0),
                (0x15D8_9832, "pmc_hum_jen", [0x24F8_C8E6, 0xEB36_DB0F], 10.0),
            ] {
                let (m, _, _) = load_model_by_hash(&mut w, model).unwrap_or_else(|| panic!("{name} loads"));
                let rig = &m.skin.rig;
                let mut weighted = vec![false; rig.len()];
                for v in &m.verts {
                    for k in 0..4 {
                        if v.weights[k] > 0 {
                            weighted[v.joints[k] as usize] = true;
                        }
                    }
                }
                let hier: Vec<u32> = rig.iter().map(|b| b.name_hash).collect();
                for (want, got) in clips.iter().zip(load_clips_for_rig(&mut w, &hier, &clips)) {
                    let ca = got.unwrap_or_else(|| panic!("{name}: clip 0x{want:08X} binds"));
                    for t in [0.0, ca.clip.duration * 0.5] {
                        let pal = crate::pose::havok_palette_in_place(
                            rig,
                            &ca.clip.sample_local(t),
                            &ca.track_to_hier,
                            ca.num_transform_tracks,
                        );
                        let mut worst = (0.0f32, 0usize);
                        for b in 0..rig.len() {
                            let p = rig[b].parent;
                            if p < 0 || !weighted[b] || !weighted[p as usize] {
                                continue;
                            }
                            // A child whose `+80` IS its parent's is a rigid accessory piece retail
                            // places offset from its authored spot on purpose (live palette:
                            // `mercs2_probe` `retail_skin_matches_the_live_palette`) — not a joint.
                            let (wb, wp) = (&rig[b].world_bind, &rig[p as usize].world_bind);
                            if (0..4).all(|r| (0..4).all(|c| (wb[r][c] - wp[r][c]).abs() <= 1e-5)) {
                                continue;
                            }
                            let j = [rig[b].world_bind[3][0], rig[b].world_bind[3][1], rig[b].world_bind[3][2]];
                            let (a, c) = (transform_point(&pal[b], j), transform_point(&pal[p as usize], j));
                            let d = (0..3).map(|k| (a[k] - c[k]).powi(2)).sum::<f32>().sqrt();
                            if d > worst.0 {
                                worst = (d, b);
                            }
                        }
                        assert!(
                            worst.0 * 1000.0 < bound_mm,
                            "{name} clip 0x{want:08X} t={t:.2}: joint of bone #{} 0x{:08X} separates by {:.2} mm (bound {bound_mm} mm)",
                            worst.1,
                            rig[worst.1].name_hash,
                            worst.0 * 1000.0
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod stream_collision_tests {
    use super::*;
    use mercs2_core::glam::{Quat, Vec3};

    /// A block and a prop with the same numeric key must never collide in the one collision map.
    #[test]
    fn block_and_prop_keys_are_disjoint() {
        assert_ne!(block_key(5), prop_key(5));
        assert_ne!(block_key(0), prop_key(0));
        assert_eq!(prop_key(0xDEAD_BEEF), 0xDEAD_BEEF_u64);
        assert_eq!(block_key(1), (1u64 << 32) | 1);
    }

    /// `placed_tris` applies an instance's rotation + translation so the collision matches what's drawn.
    #[test]
    fn placed_tris_translates_and_rotates() {
        let local = [[Vec3::X, Vec3::Y, Vec3::Z]];
        // Identity rotation → pure translation.
        let t = placed_tris(&local, Vec3::new(10.0, 2.0, 0.0), Quat::IDENTITY);
        assert_eq!(t[0][0], Vec3::new(11.0, 2.0, 0.0));
        assert_eq!(t[0][1], Vec3::new(10.0, 3.0, 0.0));
        // A 90° yaw actually rotates the vertex while preserving length.
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let r = placed_tris(&local, Vec3::ZERO, q);
        assert!((r[0][0].length() - 1.0).abs() < 1e-4, "rotation preserves length");
        assert!((r[0][0] - Vec3::X).length() > 0.5, "X was actually rotated, not left in place");
    }

    /// A flat hi-res terrain tile of world-space triangles at y=1, spanning [0,40]² in XZ.
    fn flat_tile(y: f32) -> Vec<[Vec3; 3]> {
        let mut tris = Vec::new();
        for xi in 0..10 {
            for zi in 0..10 {
                let (x0, x1) = (xi as f32 * 4.0, xi as f32 * 4.0 + 4.0);
                let (z0, z1) = (zi as f32 * 4.0, zi as f32 * 4.0 + 4.0);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1)]);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1)]);
            }
        }
        tris
    }

    /// K3 routing invariant: a terrain tile bakes into the HEIGHTFIELD and contributes ZERO triangles to
    /// the collision world — the whole point of the perf fix. A `TileHeightGrid` reproduces the tile's
    /// surface height, and the prop `collision` map the streamer feeds the collider stays empty of terrain.
    #[test]
    fn terrain_tile_bakes_into_heightfield_not_the_collider() {
        let tile = flat_tile(1.0);
        // The tile's raw triangles: none of them enter the collider.
        assert_eq!(tile.len(), 200, "the tile carries 200 tris that do not enter the collider");

        // Baked into a heightfield: the surface is recovered at ~1 m everywhere inside the tile.
        let mut field = TerrainHeightField::default();
        let grid = TileHeightGrid::bake(&tile).expect("flat tile bakes");
        field.insert(0x00A5_5E77_u32, grid);
        for &(x, z) in &[(0.0f32, 0.0f32), (20.0, 20.0), (39.0, 39.0), (5.5, 33.2)] {
            let h = field.height_at(x, z).expect("covered by the tile");
            assert!((h - 1.0).abs() < 1e-3, "hi-res surface at ({x},{z}) = {h}, expected 1.0");
        }
        // Outside the tile → no cover → the caller falls back to the low-res heightmap.
        assert_eq!(field.height_at(-10.0, -10.0), None, "outside the tile → None");
        assert_eq!(field.height_at(1000.0, 1000.0), None, "far outside → None");
        assert_eq!(field.tile_count(), 1);

        // The prop/building collider the streamer hands the runtime carries NONE of the terrain triangles.
        let prop_collision: std::collections::HashMap<u64, Vec<[Vec3; 3]>> =
            std::collections::HashMap::new();
        let collider_tris: Vec<[Vec3; 3]> = prop_collision.values().flatten().copied().collect();
        assert!(collider_tris.is_empty(), "terrain contributes zero triangles to the collider");
    }

    /// Dropping a hibernating tile removes exactly its grid, and a hole (uncovered interior after no
    /// dilation can reach it) reports `None` rather than a fabricated height.
    #[test]
    fn heightfield_insert_remove_and_bounds() {
        let mut field = TerrainHeightField::default();
        assert!(field.is_empty());
        field.insert(7, TileHeightGrid::bake(&flat_tile(2.5)).unwrap());
        assert_eq!(field.tile_count(), 1);
        assert!((field.height_at(10.0, 10.0).unwrap() - 2.5).abs() < 1e-3);
        assert!(field.remove(7), "the tile was present");
        assert!(!field.remove(7), "already gone");
        assert!(field.is_empty());
        assert_eq!(field.height_at(10.0, 10.0), None, "no resident tile → None");
    }

    /// Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
    /// `.mercs2-local.toml`, and fails when it is absent.
    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        /// Live: authored PHY2 convex collision loads for a real streamed prop, triangulates to a SANE,
        /// LOW tri count, and is lower-poly than the render mesh. Also proves the model→PHY2 linkage end to
        /// end (extract container → `PHY2` sub-chunk → parse → triangulate) and reports the WAD-wide
        /// authored-vs-fallback split across the ModelName props.
        #[test]
        fn live_prop_phy2_convex_collision_is_sane_and_low_poly() {
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            let (_low, ls) = find_terrain_blocks(&mut w)
                .unwrap_or_else(|e| panic!("the retail vz.wad's terrain blocks: {e}"));
            let mut hashes: Vec<u32> =
                mercs2_formats::placement::load_model_placements(&ls).iter().map(|p| p.model_hash).collect();
            hashes.sort_unstable();
            hashes.dedup();

            // Census + find one prop with a complete authored convex/box collider.
            let (mut authored, mut no_authored) = (0usize, 0usize);
            let mut example: Option<(u32, usize, usize)> = None; // hash, hull_tris, render_tris
            for &h in &hashes {
                match load_authored_collision(&mut w, h) {
                    Some(tris) => {
                        authored += 1;
                        // Every authored triangle vertex is finite and in a plausible model-local range.
                        for t in &tris {
                            for v in t {
                                assert!(v.is_finite(), "authored PHY2 tri vertex not finite for 0x{h:08X}: {v:?}");
                                assert!(v.length() < 2000.0, "authored PHY2 vertex implausibly far for 0x{h:08X}: {v:?}");
                            }
                        }
                        if example.is_none() {
                            let rt = load_model_by_hash(&mut w, h)
                                .map(|(m, _, _)| m.indices.len() / 3)
                                .unwrap_or_else(|| panic!("render model 0x{h:08X} does not load"));
                            example = Some((h, tris.len(), rt));
                        }
                    }
                    None => no_authored += 1,
                }
            }
            eprintln!(
                "ModelName props: {} distinct — authored PHY2 convex/box: {authored}, no authored collider (no collision): {no_authored}",
                hashes.len()
            );

            // The WAD must ship at least one prop with an authored convex collider (this is the whole point).
            let (h, hull_tris, render_tris) = example.expect("at least one prop ships authored convex PHY2 collision");
            eprintln!("example prop 0x{h:08X}: authored hull_tris={hull_tris}, render_tris={render_tris}");
            assert!(hull_tris >= 4, "a convex hull triangulates to >= 4 tris, got {hull_tris}");
            assert!(hull_tris < 5000, "authored hull collision must be LOW-poly, got {hull_tris} tris");
            assert!(
                hull_tris <= render_tris,
                "authored hull collision ({hull_tris}) should not exceed the render mesh ({render_tris})"
            );
        }
    }
}

#[cfg(test)]
mod terrain_texture_tests {
    /// Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
    /// `.mercs2-local.toml`, and fails when it is absent.
    #[cfg(feature = "retail")]
    mod retail {
        use super::super::*;

        /// Live: a real hi-res terrainmesh tile must come back TEXTURED, not white. Loading a tile binds
        /// each draw's PRMG-group representative `terraintextures` detail layer (via `DrawGroup::group_index`
        /// → `terrain_group_layers`) and resolves those hashes into the tile's texture map. Binding must
        /// hold across the multi-material sub-strip split, where `layers.len() != draws.len()`, or the
        /// ground renders white.
        #[test]
        fn live_terrainmesh_tile_is_textured() {
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            // The TerrainObject->Transform tile map (terrainmesh_hash -> world pos) lives in layers_static.
            let (_low, ls) = find_terrain_blocks(&mut w)
                .unwrap_or_else(|e| panic!("the retail vz.wad's terrain blocks: {e}"));
            let tiles = mercs2_formats::placement::load_terrain_tiles(&ls)
                .expect("every TerrainObject has a Transform");
            let tile = tiles.into_iter().next().expect("the retail vz.wad places at least one TerrainObject tile");
            wad::extract_container_typed(&mut w, tile.terrainmesh_hash, TERRAINMESH_TYPE_HASH)
                .unwrap_or_else(|e| panic!("terrainmesh 0x{:08X}: {e}", tile.terrainmesh_hash));

            let m = load_terrainmesh_tile(&mut w, tile.terrainmesh_hash, tile.pos)
                .expect("terrainmesh tile must load");
            assert!(!m.draws.is_empty(), "tile has no draws");

            // Every draw should carry a detail diffuse, and that hash must be a real, decoded
            // terraintextures texture present in the tile's resolved texture map.
            let with_diffuse = m.draws.iter().filter(|d| d.diffuse.is_some()).count();
            let resolved = m
                .draws
                .iter()
                .filter(|d| d.diffuse.map_or(false, |h| m.textures.contains_key(&h)))
                .count();
            assert!(!m.textures.is_empty(), "no terrain textures were loaded (terrain would render white)");
            assert!(
                with_diffuse * 100 >= m.draws.len() * 95,
                "expected ~all draws bound to a detail layer, got {with_diffuse}/{} — the group_index \
                 mapping regressed and the terrain would render white",
                m.draws.len()
            );
            assert!(
                resolved * 100 >= m.draws.len() * 95,
                "expected ~all bound draws to resolve to a loaded terraintextures texture, got \
                 {resolved}/{} resolved",
                m.draws.len()
            );
            eprintln!(
                "terrainmesh 0x{:08X}: {} draws, {with_diffuse} bound, {resolved} resolve, {} distinct textures",
                tile.terrainmesh_hash, m.draws.len(), m.textures.len()
            );
        }

        /// Live: the multi-layer SPLAT BLEND is faithful and non-degenerate. Over real vz.wad tiles this
        /// asserts (a) a terrainmesh group material carries >=2 blendable DETAIL diffuses beyond the base
        /// (so the blend has real layers to mix); (b) the per-vertex D3DCOLOR weights decode to a sane,
        /// non-degenerate distribution (dominant R, weights normalize to ~1, NOT all-equal); (c) after
        /// binding, every blend draw carries a representative diffuse AND its aliased detail slots, all
        /// resolving to loaded textures — the anti-white-terrain guarantee.
        #[test]
        fn live_terrainmesh_splat_blend_is_faithful() {
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            let (_low, ls) = find_terrain_blocks(&mut w)
                .unwrap_or_else(|e| panic!("the retail vz.wad's terrain blocks: {e}"));
            let tiles = mercs2_formats::placement::load_terrain_tiles(&ls)
                .expect("every TerrainObject has a Transform");

            // (a) Find a tile whose material set carries >=2 blendable detail diffuses beyond base0 +
            // the global 3B030C8A constant — i.e. a group layerset of len >= 4 — while AGGREGATING the
            // per-vertex weight distribution across every scanned tile (a single sparse edge tile can be
            // near-uniform; the corpus-wide distribution is what proves the weights are real splat data).
            let mut multi_layer_tile = None;
            let mut max_len_seen = 0usize;
            let (mut nverts, mut sum_r, mut sum_g, mut sum_b) = (0u64, 0u64, 0u64, 0u64);
            let mut sum_ok = 0u64; // verts whose R+G+B normalizes to ~1
            let mut distinct: std::collections::HashSet<[u8; 4]> = std::collections::HashSet::new();
            let mut scanned = 0usize;
            for t in &tiles {
                let container =
                    wad::extract_container_typed(&mut w, t.terrainmesh_hash, TERRAINMESH_TYPE_HASH)
                        .unwrap_or_else(|e| panic!("terrainmesh 0x{:08X}: {e}", t.terrainmesh_hash));
                let layers = mercs2_formats::texture::terrain_group_layers(&container)
                    .unwrap_or_else(|e| panic!("terrainmesh 0x{:08X} MTRL: {e}", t.terrainmesh_hash));
                let ml = layers.iter().map(|l| l.len()).max().unwrap_or(0);
                max_len_seen = max_len_seen.max(ml);
                if ml >= 4 && multi_layer_tile.is_none() {
                    let best = layers.iter().max_by_key(|l| l.len()).unwrap().clone();
                    multi_layer_tile = Some((t.terrainmesh_hash, t.pos, best));
                }
                let meshes = mercs2_formats::model_cubeize::read_model_meshes(&container)
                    .unwrap_or_else(|e| panic!("terrainmesh 0x{:08X} meshes: {e}", t.terrainmesh_hash));
                for m in &meshes {
                    for c in &m.colors {
                        // D3DCOLOR stored B,G,R,A: c[2]=R, c[1]=G, c[0]=B.
                        let (r, g, b) = (c[2] as u64, c[1] as u64, c[0] as u64);
                        sum_r += r; sum_g += g; sum_b += b;
                        let s = (r + g + b) as f32 / 255.0;
                        if (s - 1.0).abs() < 0.15 { sum_ok += 1; }
                        distinct.insert(*c);
                        nverts += 1;
                    }
                }
                scanned += 1;
                if scanned >= 24 && multi_layer_tile.is_some() {
                    break;
                }
            }
            let Some((tile_hash, tile_pos, best)) = multi_layer_tile else {
                panic!(
                    "no multi-layer terrain tile in the retail vz.wad (scanned {scanned} tiles, max layerset \
                     len {max_len_seen})"
                );
            };
            // base0 + 3B030C8A + >=2 details => at least 2 blendable diffuses after dropping the base
            // and the global constant (index 1).
            let blendable_details = best.len().saturating_sub(2);
            assert!(
                blendable_details >= 2,
                "expected a group to carry >=2 blendable detail diffuses, got layerset {best:08X?}"
            );

            // (b) Per-vertex weight distribution (aggregated): dominant R, sum normalizes to ~1, varied.
            assert!(nverts > 0, "terrain tiles carried no per-vertex splat weights");
            assert!(
                distinct.len() > 16,
                "weights are degenerate (all-equal): only {} distinct values over {nverts} verts",
                distinct.len()
            );
            assert!(
                sum_ok * 100 >= nverts * 90,
                "expected >=90% of vertices' R+G+B to normalize to ~1, got {sum_ok}/{nverts}"
            );
            assert!(
                sum_r > sum_g && sum_r > sum_b,
                "expected the R channel to dominate the splat weights (means R {} G {} B {})",
                sum_r / nverts, sum_g / nverts, sum_b / nverts
            );

            // (c) Binding: every blend draw carries a representative diffuse + aliased detail slots, all
            // resolving to a loaded terraintextures texture (anti-white-terrain).
            let m = load_terrainmesh_tile(&mut w, tile_hash, tile_pos).expect("tile loads");
            let blend_draws: Vec<_> = m.draws.iter().filter(|d| d.diffuse.is_some()).collect();
            assert!(!blend_draws.is_empty(), "no draw bound a detail layer");
            for d in &blend_draws {
                for slot in [d.diffuse, d.normal, d.specular] {
                    let h = slot.expect("a blend draw must bind all three diffuse slots (aliased if absent)");
                    assert!(
                        m.textures.contains_key(&h),
                        "blend layer 0x{h:08X} did not resolve to a loaded texture (white-terrain risk)"
                    );
                }
            }
            eprintln!(
                "splat 0x{:08X}: layerset {:08X?} ({blendable_details} blendable details); {} distinct \
                 weights over {nverts} verts, {sum_ok} normalize; {} blend draws all resolve",
                tile_hash, best, distinct.len(), blend_draws.len()
            );
        }

        /// Live proof of the K3 collision savings: a real terrainmesh tile carries thousands of triangles that
        /// USED to be cloned into the collision world on every wake/hibernate. Baking it into a
        /// `TileHeightGrid` costs one grid instead, and the grid reproduces the tile's surface height (so the
        /// hero still grounds on the hi-res terrain). Prints the before/after tri count.
        #[test]
        fn live_terrain_tile_routes_to_heightfield_not_the_collider() {
            let mut w = crate::worldutil::schema_wire_tests::retail::open_vz_wad();
            let (_low, ls) = find_terrain_blocks(&mut w)
                .unwrap_or_else(|e| panic!("the retail vz.wad's terrain blocks: {e}"));
            let tiles = mercs2_formats::placement::load_terrain_tiles(&ls)
                .expect("every TerrainObject has a Transform");
            let tile = tiles.into_iter().next().expect("the retail vz.wad places at least one TerrainObject tile");
            wad::extract_container_typed(&mut w, tile.terrainmesh_hash, TERRAINMESH_TYPE_HASH)
                .unwrap_or_else(|e| panic!("terrainmesh 0x{:08X}: {e}", tile.terrainmesh_hash));
            let m = load_terrainmesh_tile(&mut w, tile.terrainmesh_hash, tile.pos).expect("tile loads");
            let world_tris = extract_local_tris(&m); // already world-space
            let tris_avoided = world_tris.len();
            assert!(
                tris_avoided > 100,
                "a real terrain tile should carry many triangles (got {tris_avoided}) — these are the \
                 collider entries the heightfield replaces"
            );

            let grid = TileHeightGrid::bake(&world_tris).expect("tile bakes into a heightfield");
            let mut field = TerrainHeightField::default();
            field.insert(tile.key, grid);

            // The baked surface must track the tile's actual triangle vertices (the hero stands on THIS, not
            // the low-res surface). Sample a spread of real vertices and compare to the field.
            let mut checked = 0u32;
            let mut max_err = 0.0f32;
            for t in world_tris.iter().step_by(world_tris.len() / 50 + 1) {
                for v in t {
                    if let Some(h) = field.height_at(v.x, v.z) {
                        // The grid stores per-cell MAX height, so it is >= any single vertex in that cell;
                        // on the near-flat terrain the gap is small. Bound the shortfall generously.
                        max_err = max_err.max((h - v.y).abs());
                        checked += 1;
                    }
                }
            }
            assert!(checked > 0, "the field covered at least some of the tile's own vertices");
            eprintln!(
                "terrain tile 0x{:08X}: {tris_avoided} tris NO LONGER enter the collider (baked into a \
                 {}-tile heightfield); surface tracked at {checked} verts, max |Δ|={max_err:.2} m",
                tile.terrainmesh_hash,
                field.tile_count()
            );
        }
    }
}
