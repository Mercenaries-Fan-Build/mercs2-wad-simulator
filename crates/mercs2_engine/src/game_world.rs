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
    let container = wad::extract_container(w, hash).ok()?;
    let (verts, indices, draws, stats) = mesh::build_indexed_state(&container, active_bit).ok()?;
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
        LoadedModel { hash, verts, indices, draws, textures, skin, clips },
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
            LoadedModel { hash, verts, indices, draws, textures, skin, clips },
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
// RE NOTE (verified against retail vz.wad, 2026-07): retail *props* ship no clip of their own. A full
// census — 0/335 rigged `ModelName` placement props, and 0 of 1248 rigged models across all 1771
// model ASETs — has a clip whose `trnm` resolves into its rig; all 4261 animgroup clips are the
// character skeleton family (60–105 tracks). The native `sequ/SINF/TRCK/MINF/MANM/VALU` chunk cluster
// (concentrated in blocks 3183/3185, alongside the known Mattias clip 0x547C0A27) is the *character*
// animation binding/registry, not a prop-clip system; retail interior props/doors are moved by script
// or procedural bone controllers, not shipped clips. So this path is the correct, faithful mechanism
// and animates any model that DOES carry clips (characters/NPCs/DLC/mods); it is a deliberate no-op
// for clip-less retail props. See docs/modernization/rendering_fx_lighting_gap.md §H.

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
    /// `hier`. Returns `None` for a non-decoded (delta header-only) clip.
    fn decode(&self, w: &mut wad::Wad, hier: &[u32], name_hash: u32) -> Option<ClipAnim> {
        use mercs2_formats::animgroup::parse_animgroup;
        let entry = self.clips.iter().find(|c| c.name_hash == name_hash)?;
        let data = wad::decompress_block_index(w, entry.block).ok()?;
        let ag = parse_animgroup(&data).ok()?;
        let c = ag.clips.iter().find(|c| c.name_hash == name_hash)?;
        let clip = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok()?;
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

static CLIP_INDEX: std::sync::OnceLock<AnimClipIndex> = std::sync::OnceLock::new();

/// Process-wide cached clip index, built from the first WAD handle to ask for it (the WAD is fixed
/// per process). Building scans the ~190 animgroup blocks once (a few seconds); every later lookup is
/// cheap integer comparison over the flat clip list.
pub fn clip_index(w: &mut wad::Wad) -> &'static AnimClipIndex {
    if let Some(ix) = CLIP_INDEX.get() {
        return ix;
    }
    let ix = AnimClipIndex::build(w);
    CLIP_INDEX.get_or_init(|| ix)
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
/// the WAD handle (moved to the render thread for on-demand wake extraction), the base terrain, its
/// heightmap, the pure streaming decision manager (blocks + per-entity props with hibernation), and
/// the key->spawn recipe map the executor uses on WAKE.
///
/// **Opaque cross-crate handle** (K2 unification, `docs/modernization/k2_streaming_unification_plan.md`):
/// the type is `pub` so `mercs2_game`'s TPS boot can hold the loader output and hand it to the
/// streaming executor, but the fields stay crate-private — the executor (this crate) is the only thing
/// that reads them.
pub struct StreamingWorldData {
    wad: wad::Wad,
    terrain: LoadedModel,
    manager: mercs2_core::streaming::StreamingManager,
    props: std::collections::HashMap<u32, PropSpawn>,
    terrain_tiles: std::collections::HashMap<u32, (u32, [f32; 3])>,
    /// Low-res terrain grid cell (row*20+col) -> its draw-group index in the `terrain` model, so the
    /// executor can hide that tile when the hi-res terrainmesh at the same cell is resident.
    lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
    /// Dynamic lights harvested from `LightObject` COMPs (base `layers_static` + active overlays),
    /// in world space. Handed to `Scene::set_lights`; the scene uploads the nearest set each frame.
    lights: Vec<crate::render::GpuLight>,
}

/// Convert a harvested `LightObject` placement to a GPU point light, dropping degenerate lights
/// (non-positive / non-finite radius) so the inferred `params[0]=intensity`/`params[1]=radius`
/// mapping can never flood the scene. Color/intensity/radius are the authored on-disk values.
pub fn placed_lights_to_gpu(placed: &[mercs2_formats::placement::PlacedLight]) -> Vec<crate::render::GpuLight> {
    placed
        .iter()
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

/// Build a static additive glow card for an environmental light-shaft placement
/// (`global_particle_env_godray2` — the PMC hall god rays). Faithful + data-driven:
///  * position = the placement world position;
///  * size = the effect `TRFM` horizontal footprint (`0.5·(|sx|+|sz|)`), scaled up so the soft
///    additive sprite reads as a shaft of dusty light rather than a pinpoint;
///  * tint = the effect `COLR` peak colour (dim white ~0.25) lifted for the additive/HDR path.
///
/// The effect template name is the placement name with `"particle_"` removed (verified:
/// `global_particle_env_godray2` → `global_env_godray2`, m2 `0xDB331999`, in the `effects` block).
/// Falls back to the reversed retail constants if the effect can't be resolved.
pub fn glow_card_for_effect(w: &mut wad::Wad, placement_name: &str, pos: [f32; 3]) -> crate::particles::GlowCard {
    let (scale, color, alpha) = env_shaft_effect_params(w, placement_name)
        .unwrap_or(([2.892, 0.805, 2.892], [0.25, 0.24, 0.20], 0.16));
    let footprint = 0.5 * (scale[0].abs() + scale[2].abs()); // ~2.89
    // The retail COLR is dim (~0.25 white, alpha ~0.16); lift into an additive/HDR-friendly tint so
    // the soft glow is visible and blooms, without washing out (clamped).
    let boost = (alpha * 4.0 + 0.6).clamp(0.8, 2.5);
    let a = (alpha * 3.5).clamp(0.25, 0.85);
    crate::particles::GlowCard {
        pos,
        size: (footprint * 3.0).clamp(4.0, 24.0),
        color: [
            (color[0] * boost).min(1.5),
            (color[1] * boost).min(1.5),
            (color[2] * boost).min(1.5),
            a,
        ],
    }
}

/// Read an environmental light-shaft effect template and recover `(TRFM scale, COLR peak RGB, COLR
/// peak alpha)`. Returns `None` if the `effects` block or the effect can't be found.
fn env_shaft_effect_params(w: &mut wad::Wad, placement_name: &str) -> Option<([f32; 3], [f32; 3], f32)> {
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_formats::types::TYPE_HASH_EFFECT;

    let effect_name = placement_name.replace("particle_", "");
    let want = pandemic_hash_m2(&effect_name);
    let paths: Vec<String> = wad::block_paths(w).to_vec();
    let blk = paths.iter().position(|p| p.to_ascii_lowercase().contains("effect"))? as u16;
    let dec = wad::decompress_block_index(w, blk).ok()?;
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    let mut pos = 4 + count as usize * 16;
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.type_hash == TYPE_HASH_EFFECT && e.name_hash == want && end <= dec.len() {
            let c = &dec[pos..end];
            let mut scale = [1.0f32, 1.0, 1.0];
            let mut color = [0.25f32, 0.24, 0.20];
            let mut alpha = 0.16f32;
            for (tag, s, en) in ucfx_child_chunks(c) {
                let body = &c[s..en];
                match &tag {
                    b"TRFM" if body.len() >= 64 => {
                        scale = [rf32le(body, 0), rf32le(body, 20), rf32le(body, 40)];
                    }
                    b"COLR" => {
                        if let Some((rgb, a)) = colr_peak_bc(body) {
                            if rgb.iter().any(|&v| v > 0.02) {
                                color = rgb;
                            }
                            alpha = a;
                        }
                    }
                    _ => {}
                }
            }
            return Some((scale, color, alpha));
        }
        pos = end;
    }
    None
}

fn rf32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Walk a UCFX container's descriptor rows → `(tag, start, end)` for each child body (data-relative
/// offsets; `u0 == 0xFFFFFFFF` = container sentinel, skipped).
fn ucfx_child_chunks(c: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let dao = u32::from_le_bytes([c[4], c[5], c[6], c[7]]) as usize;
    let n = u32::from_le_bytes([c[16], c[17], c[18], c[19]]) as usize;
    for i in 0..n {
        let row = 20 + i * 20;
        if row + 20 > c.len() {
            break;
        }
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&c[row..row + 4]);
        let u0 = u32::from_le_bytes([c[row + 4], c[row + 5], c[row + 6], c[row + 7]]);
        if u0 == 0xFFFF_FFFF {
            continue;
        }
        let size = u32::from_le_bytes([c[row + 8], c[row + 9], c[row + 10], c[row + 11]]) as usize;
        let start = if dao > 0 { dao + u0 as usize } else { 8 + u0 as usize };
        let end = start + size;
        if end <= c.len() {
            out.push((tag, start, end));
        }
    }
    out
}

/// Decode the `global_env_godray2` `COLR` peak tint. Layout (reversed this session): 8-byte-stride
/// records `[0xBC, 0, 0, R, G, B, A, 0]` (RGBA8). Returns the greatest-luminance stop, or `None` if
/// the body isn't that shape.
fn colr_peak_bc(body: &[u8]) -> Option<([f32; 3], f32)> {
    if body.len() < 8 {
        return None;
    }
    let mut best = None;
    let mut best_lum = -1.0f32;
    let mut i = 0;
    while i + 8 <= body.len() {
        if body[i] != 0xBC {
            return None; // not the observed stop format — don't misread
        }
        let (r, g, b, a) = (body[i + 3], body[i + 4], body[i + 5], body[i + 6]);
        let lum = r as f32 + g as f32 + b as f32;
        if lum > best_lum {
            best_lum = lum;
            best = Some(([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0], a as f32 / 255.0));
        }
        i += 8;
    }
    best
}

/// Load the streaming world off-thread: open the WAD, merge the base terrain, build the world block
/// index + Layer-2 streaming catalog (c3-cell LOAD units + per-entity `ModelName` props with their
/// `HibernationControl` distances). Returns the data (incl. the WAD handle) for the render thread.
/// Load the streaming world (block index + `StreamingManager` + wake recipes + WAD handle) on the
/// background thread. `pub` for the K2 unification: `mercs2_game`'s playable boot calls this to get the
/// same streaming runtime the free-fly path uses (see `k2_streaming_unification_plan.md`).
pub fn load_streaming_world_data(
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
        draws.push(mesh::DrawGroup { index_start: start, index_count: count, diffuse, specular: None, normal: None, group_index: 0 });
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

    // Dynamic lights: harvest LightObject COMPs (joined to their entity Transform for a world
    // position) from the base layers_static block; overlays fold theirs in below.
    let mut lights = placed_lights_to_gpu(&mercs2_formats::placement::light_inventory(&ls));

    // Activate the save's vz_state OVERLAYS on top of the always-loaded base: resolve each active
    // layer name -> its WAD block, then fold its placements into the same streaming catalog so they
    // stream by proximity like the base. This is the game-state world (destruction/staging/faction +
    // the PMC interior) the save selects. Empty `overlays` = base world only.
    let (mut ov_resolved, mut ov_entities) = (0usize, 0usize);
    for layer in overlays {
        let Some(bi) = resolve_overlay_block(&w, layer) else { continue };
        let Ok(dec) = wad::decompress_block_index(&mut w, bi) else { continue };
        let (mn, nm) = add_overlay_to_catalog(&dec, cfg.default_distances, &mut manager, &mut props);
        lights.extend(placed_lights_to_gpu(&mercs2_formats::placement::light_inventory(&dec)));
        ov_resolved += 1;
        ov_entities += mn + nm;
    }
    if !overlays.is_empty() {
        println!("[stream] overlays: {ov_resolved}/{} vz_state layers resolved, +{ov_entities} entities", overlays.len());
    }

    println!(
        "[stream] catalog: {} c3-cell blocks, {} per-entity props ({base_props} base + {} overlay), {} hi-res terrain tiles, {} population regions",
        manager.block_count(), props.len(), props.len() - base_props, terrain_tiles.len(), manager.region_count()
    );

    // Seam A — E1 schema-driven component pass (ADDITIVE, alongside the oracle-driven catalog above):
    // build the kernel ComponentRegistry from the block's `schm` field schemas and deserialize its
    // generic COMP records, cross-checking agreement with the bespoke placement oracle. This wires the
    // field-schema deserializer into the live loader; it does not change what streams.
    let (comp_reg, sstats) = load_schema_components(&ls);
    println!(
        "[stream] schema (seam A): {} classes registered (pool budget {}), {} generic COMP groups / {} records deserialized; oracle agreement: HibernationControl {}/{}, ModelName {}/{}",
        sstats.classes, comp_reg.total_budget(), sstats.generic_groups, sstats.generic_records,
        sstats.hib_agree, sstats.hib_checked, sstats.model_agree, sstats.model_checked
    );
    if sstats.hib_agree != sstats.hib_checked || sstats.model_agree != sstats.model_checked {
        eprintln!(
            "[stream] WARNING: schema path disagreed with the placement oracle (HibernationControl {}/{}, ModelName {}/{}) — investigate before trusting schema-driven instantiation",
            sstats.hib_agree, sstats.hib_checked, sstats.model_agree, sstats.model_checked
        );
    }
    progress.step("streaming catalog");

    println!("[stream] dynamic lights: {} LightObject placements harvested", lights.len());
    Ok(StreamingWorldData { wad: w, terrain, manager, props, terrain_tiles, lowres_draw_by_cell, lights })
}

/// A per-frame stats snapshot for the streaming console/HUD line.
pub struct StreamStats {
    pub resident: usize,
    pub awake: usize,
    pub cached_regions: usize,
    pub regions: usize,
    pub block_ents: usize,
    pub props: usize,
    pub models: usize,
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

/// The reusable streaming-runtime **executor** (K2 unification,
/// `docs/modernization/k2_streaming_unification_plan.md`). Owns the decision [`StreamingManager`], the
/// WAD handle (for on-demand wake extraction), the wake recipes, and all live-executor bookkeeping, and
/// performs the per-frame GPU/ECS LOAD/WAKE/UNLOAD/HIBERNATE work in [`step`](Self::step). Extracted
/// **verbatim** from `run_game_world`'s closure so BOTH the free-fly boot and the playable TPS boot can
/// drive one streaming path — each layering its own camera/player on top. The one-time boot glue (base
/// terrain upload, the game populate hook, world lights) stays in the caller; [`new`](Self::new) takes
/// ownership of the streaming state after that.
pub struct StreamingWorld {
    wad: wad::Wad,
    manager: mercs2_core::streaming::StreamingManager,
    props: std::collections::HashMap<u32, PropSpawn>,
    terrain_tiles: std::collections::HashMap<u32, (u32, [f32; 3])>,
    lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
    terrain_hash: u32,
    prop_ents: std::collections::HashMap<u32, mercs2_core::Entity>,
    block_ents: std::collections::HashMap<u16, mercs2_core::Entity>,
    model_refs: std::collections::HashMap<u32, u32>,
    wake_failed: std::collections::HashSet<u32>,
    anim_store: std::collections::HashMap<u32, crate::scene::ModelAnim>,
    // --- collision delta (K2 S2) ---
    /// Per-model **local-space** collision triangles, cached on first load so a prop whose mesh is
    /// already resident (loaded by an earlier instance) still contributes collision without re-reading
    /// the WAD. Keyed by model hash.
    local_tris: std::collections::HashMap<u32, Vec<[mercs2_core::glam::Vec3; 3]>>,
    /// Live **world-space** collision soup, keyed per streamed unit (block or prop — see `block_key`/
    /// `prop_key`) so a UNLOAD/HIBERNATE removes exactly that unit's triangles. The consumer rebuilds
    /// its physics soup from [`collision_tris`](Self::collision_tris) whenever [`take_collision_dirty`]
    /// (Self::take_collision_dirty) reports a change.
    collision: std::collections::HashMap<u64, Vec<[mercs2_core::glam::Vec3; 3]>>,
    /// Set whenever `collision` changed this step (a unit woke/loaded or hibernated/unloaded).
    collision_dirty: bool,
}

impl StreamingWorld {
    /// Take ownership of the loaded streaming state. The caller has already uploaded the base terrain,
    /// run its populate hook, and set the world lights (one-time boot glue, not per-frame). `terrain_hash`
    /// is the low-res terrain model hash (for the tile LOD swap).
    pub fn new(
        wad: wad::Wad,
        manager: mercs2_core::streaming::StreamingManager,
        props: std::collections::HashMap<u32, PropSpawn>,
        terrain_tiles: std::collections::HashMap<u32, (u32, [f32; 3])>,
        lowres_draw_by_cell: std::collections::HashMap<usize, usize>,
        terrain_hash: u32,
    ) -> Self {
        StreamingWorld {
            wad,
            manager,
            props,
            terrain_tiles,
            lowres_draw_by_cell,
            terrain_hash,
            prop_ents: std::collections::HashMap::new(),
            block_ents: std::collections::HashMap::new(),
            model_refs: std::collections::HashMap::new(),
            wake_failed: std::collections::HashSet::new(),
            anim_store: std::collections::HashMap::new(),
            local_tris: std::collections::HashMap::new(),
            collision: std::collections::HashMap::new(),
            collision_dirty: false,
        }
    }

    /// Whether the collision soup changed since the last [`take_collision_dirty`](Self::take_collision_dirty).
    pub fn collision_dirty(&self) -> bool {
        self.collision_dirty
    }

    /// Read-and-clear the collision-dirty flag — the consumer calls this each step and, if `true`,
    /// rebuilds its physics soup from [`collision_tris`](Self::collision_tris).
    pub fn take_collision_dirty(&mut self) -> bool {
        std::mem::take(&mut self.collision_dirty)
    }

    /// The current world-space collision soup (all resident blocks + woken props), flattened. Cheap to
    /// rebuild since it only runs when [`take_collision_dirty`](Self::take_collision_dirty) is `true`.
    pub fn collision_tris(&self) -> Vec<[mercs2_core::glam::Vec3; 3]> {
        self.collision.values().flatten().copied().collect()
    }

    /// Live executor counts for the periodic stat line.
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            resident: self.manager.resident_count(),
            awake: self.manager.awake_count(),
            cached_regions: self.manager.cached_region_count(),
            regions: self.manager.region_count(),
            block_ents: self.block_ents.len(),
            props: self.prop_ents.len(),
            models: self.model_refs.len(),
        }
    }

    /// One streaming step at camera position `cam`: run the manager decision (block/prop diff + the
    /// RegionCache CacheIn/CacheOut layer), then execute the diff on the GPU/ECS — UNLOAD blocks that
    /// left the working radius, HIBERNATE far props, LOAD c3-cell geometry, and WAKE props/terrain tiles
    /// at their authored transforms. Verbatim from the original free-fly executor.
    pub fn step(&mut self, scene: &mut crate::scene::Scene, world: &mut mercs2_core::World, cam: [f32; 3]) {
        use mercs2_core::glam::{Quat, Vec3};
        use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
        const IDENTITY: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let Self {
            wad,
            manager,
            props,
            terrain_tiles,
            lowres_draw_by_cell,
            terrain_hash,
            prop_ents,
            block_ents,
            model_refs,
            wake_failed,
            anim_store,
            local_tris,
            collision,
            collision_dirty,
        } = self;
        let terrain_hash = *terrain_hash;

        let diff = manager.update(cam);
        // Seam B — drive the RegionCache decision layer each tick (PgSysPopulation CacheIn/CacheOut).
        let _region_diff = manager.update_regions(cam);

        // UNLOAD first (free GPU): blocks that left the working radius.
        for b in &diff.unload_blocks {
            if let Some(e) = block_ents.remove(b) {
                if let Ok(mr) = world.get::<&ModelRef>(e).map(|m| m.model) {
                    dec_model_ref(model_refs, mr, scene);
                }
                let _ = world.despawn(e);
                scene.forget_entity(e);
            }
            if collision.remove(&block_key(*b)).is_some() {
                *collision_dirty = true;
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
                    dec_model_ref(model_refs, mr, scene);
                }
                let _ = world.despawn(e);
                scene.forget_entity(e);
            }
            if collision.remove(&prop_key(*k)).is_some() {
                *collision_dirty = true;
            }
        }
        // LOAD c3-cell blocks (throttled by the manager's block budget).
        for b in &diff.load_blocks {
            if block_ents.contains_key(b) {
                continue;
            }
            if let Some((m, off)) = load_one_c3_cell(wad, *b) {
                if !scene.has_model(m.hash) {
                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                }
                // Collision (S2): c3 cell geometry placed at `off` (identity rotation).
                let lt = local_tris.entry(m.hash).or_insert_with(|| extract_local_tris(&m));
                collision.insert(block_key(*b), placed_tris(lt, Vec3::new(off[0], off[1], off[2]), Quat::IDENTITY));
                *collision_dirty = true;
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
        // WAKE props (throttled by the manager's entity budget): instantiate the ModelName mesh at the
        // authored Transform (identity fit + bone-count palette).
        for k in &diff.wake {
            if prop_ents.contains_key(k) {
                continue;
            }
            // Hi-res terrain tile? Load the terrainmesh (POFF-composed, world-placed via
            // TerrainObject->Transform) and spawn at identity (verts already world-space).
            if let Some(&(tm_hash, pos)) = terrain_tiles.get(k) {
                if !scene.has_model(tm_hash) {
                    match load_terrainmesh_tile(wad, tm_hash, pos) {
                        Some(m) => {
                            scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                            local_tris.entry(m.hash).or_insert_with(|| extract_local_tris(&m));
                        }
                        None => {
                            wake_failed.insert(*k);
                            continue;
                        }
                    }
                }
                // Collision (S2): terrain verts are already world-space; the tile spawns at identity.
                if let Some(lt) = local_tris.get(&tm_hash) {
                    collision.insert(prop_key(*k), lt.clone());
                    *collision_dirty = true;
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
                match load_model_by_hash(wad, spawn.model_hash) {
                    Some((m, _, _)) => {
                        scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                        local_tris.entry(m.hash).or_insert_with(|| extract_local_tris(&m));
                        // A rigged model that ships clips registers its rig + clips so the per-frame
                        // animation pass can pose it (no-op for clip-less props).
                        if !m.clips.is_empty() {
                            anim_store.entry(m.hash).or_insert_with(|| crate::scene::ModelAnim {
                                rig: m.skin.rig.clone(),
                                clips: m.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
                            });
                        }
                    }
                    None => {
                        wake_failed.insert(*k);
                        continue;
                    }
                }
            }
            // Collision (S2): the prop mesh placed at its authored pos+quat.
            if let Some(lt) = local_tris.get(&spawn.model_hash) {
                let q = Quat::from_xyzw(spawn.quat[0], spawn.quat[1], spawn.quat[2], spawn.quat[3]);
                let p = Vec3::new(spawn.pos[0], spawn.pos[1], spawn.pos[2]);
                collision.insert(prop_key(*k), placed_tris(lt, p, q));
                *collision_dirty = true;
            }
            let nbones = scene.model_bone_count(spawn.model_hash).max(1);
            let mut t = Transform::from_translation(Vec3::new(spawn.pos[0], spawn.pos[1], spawn.pos[2]));
            t.rotation = Quat::from_xyzw(spawn.quat[0], spawn.quat[1], spawn.quat[2], spawn.quat[3]);
            // Play the first discovered clip if this model has any; else stay static.
            let anim = match anim_store.get(&spawn.model_hash).and_then(|ma| ma.clips.keys().next()) {
                Some(&clip) => AnimState::playing(clip),
                None => AnimState::default(),
            };
            let e = world.spawn((
                t,
                ModelRef { model: spawn.model_hash },
                anim,
                SkinPalette { mats: vec![IDENTITY; nbones] },
            ));
            *model_refs.entry(spawn.model_hash).or_insert(0) += 1;
            prop_ents.insert(*k, e);
        }
        // diff.tier_changes carries the per-PROP hibernation LOD tier — informational only; props don't
        // ship alternate-LOD meshes (verified --lod-probe: 2/446).
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
    // live World / Scene / Wad so the GAME can spawn its own entities (player, PMC interior, mission
    // objects…). The engine itself is asset-agnostic and knows nothing about any of that — game
    // specifics live in `mercs2_game`, not here.
    mut populate: impl FnMut(&mut mercs2_core::World, &mut crate::scene::Scene, &mut crate::wad::Wad)
        + 'static,
) {
    use crate::scene::Scene;
    use mercs2_core::frame::{LayerStack, LayerTransition, LAYER_GAME};
    use mercs2_core::glam::{Mat4, Vec3};
    use mercs2_core::streaming::StreamingConfig;
    use mercs2_core::Time;
    use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform, World};
    use std::collections::HashSet;
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
    // Sky/atmosphere + HDR tone-map + bloom. The base-game default ("afternoon", from
    // mrxbootstrap.lua SetDefaultAtmosphere) drives the Rayleigh/Mie sky + fBloom* post chain. A
    // world's Lua can override this by parsing its `Graphics.Atmosphere.*` block into an
    // `Atmosphere` and calling `set_atmosphere` again (see mercs2_formats::atmosphere).
    scene.set_atmosphere(mercs2_formats::atmosphere::Atmosphere::default());
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
    // The streaming runtime executor (K2: the reusable StreamingWorld owns the manager + WAD handle +
    // all live bookkeeping). Wired in on loader completion; drives the world each frame via `step`.
    let mut stream: Option<StreamingWorld> = None;

    // Free-fly camera. Start over the PMC exterior spawn at a moderate height so nearby cells +
    // props stream in immediately; WASDQE + mouse-look fly around.
    // Free-fly camera start: the authored spawn if given (mercs2_game passes the authentic
    // PMC-interior start), else an elevated bird's-eye over the exterior pool for free exploration.
    // The authored spawn Y (MrxUtil._TeleportHero) is the hero ROOT — feet on the floor. A free-fly
    // camera sitting exactly there views from floor level, which makes the room floor read as ~2 m too
    // high. Start at standing EYE height above it so the floor sits where a standing player would see it.
    const EYE_HEIGHT: f32 = 1.7;
    let mut free_pos = match spawn {
        Some(s) => Vec3::new(s[0], s[1] + EYE_HEIGHT, s[2]),
        None => Vec3::new(EXTERIOR_SPAWN[0], 140.0, EXTERIOR_SPAWN[2]),
    };
    if let Some(s) = spawn {
        eprintln!(
            "[boot] interior camera at eye height: hero root Y {:.2} + {:.1} = {:.2}  (shell floor world Y = {:.1} = actor origin)",
            s[1], EYE_HEIGHT, s[1] + EYE_HEIGHT, 450.0
        );
    }
    // Initial heading: at a provided spawn (the PMC interior) face +Z, level — the room extends +Z
    // from the spawn's near edge, so you look INTO it rather than at the wall behind. The exterior
    // default is a downward bird's-eye facing -Z over the pool.
    let (mut free_yaw, mut free_pitch): (f32, f32) = match spawn {
        Some(_) => (0.0, 0.0),
        None => (PI, -0.35),
    };
    let mut held: HashSet<KeyCode> = HashSet::new();
    // Keystone C — the master frame spine (docs/reverse_engineer/scheduler_tick_code_map.md). The
    // shared fixed-sim `Time` clock drives the animation step (replacing the old private anim_dt); the
    // application-layer stack replaces the `loading` bool — LAYER_LOADING renders the plate/spinner
    // while the background loader runs, then loader-completion raises the target to LAYER_GAME and the
    // one Ascending(GAME) transition realizes the world (the recovered 0→4 climb, `FUN_004c15e0`).
    const LAYER_LOADING: usize = LAYER_GAME - 1;
    let mut time = Time::new(60.0);
    let mut layers = LayerStack::at(LAYER_LOADING);
    let mut pending: Option<StreamingWorldData> = None;
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
                    // ===== RunFrame (FUN_00630ef0) — faithful 9-stage per-frame order =====
                    // (docs/reverse_engineer/scheduler_tick_code_map.md §2). Stages that are pure
                    // platform glue on our side fold into wgpu/winit: (2) device re-init = the
                    // surface-lost/outdated recovery in render()'s Err arm; (8) vsync/cap + (9) present
                    // = wgpu's present mode + winit's AboutToWait redraw request.

                    // (1) frame-start timestamp — QPC on the exe; std Instant is our QPC source.
                    let now = std::time::Instant::now();
                    let real_dt = (now - last).as_secs_f32().min(0.1);
                    last = now;

                    // (5a) MASTER UPDATE — mode logic for the active application layer. While loading,
                    //      poll the background loader; on completion raise the target to the GAME layer.
                    if layers.active() == LAYER_LOADING {
                        match rx.try_recv() {
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                eprintln!("[stream] loader thread died"); elwt.exit(); return;
                            }
                            Ok(Err(e)) => { eprintln!("[stream] load failed: {e}"); elwt.exit(); return; }
                            Ok(Ok(data)) => { pending = Some(data); layers.set_target(LAYER_GAME); }
                        }
                    }
                    // (5b) climb the layer stack toward its target; realize the world exactly once on
                    //      entering the GAME layer (the former loader-complete branch).
                    while !layers.settled() {
                        if let Some(LayerTransition::Ascending(LAYER_GAME)) = layers.advance() {
                            let Some(mut data) = pending.take() else { continue };
                            // Base terrain: one static entity at identity (verts already world-space).
                            let terrain = data.terrain;
                            let terrain_hash = terrain.hash;
                            scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
                            world.spawn((
                                Transform::IDENTITY,
                                ModelRef { model: terrain.hash },
                                AnimState::default(),
                                SkinPalette { mats: vec![IDENTITY] },
                            ));
                            // GAME world population: hand the game the live World/Scene/Wad so it can
                            // spawn its own entities (player, PMC interior, …). The engine does not know
                            // what a "PMC interior" is — that lives in `mercs2_game`.
                            populate(&mut world, &mut scene, &mut data.wad);
                            // Hand the harvested world lights to the scene (static placements; the scene
                            // uploads the nearest set to the camera each frame).
                            scene.set_lights(std::mem::take(&mut data.lights));
                            // Take ownership of the streaming state into the reusable executor.
                            stream = Some(StreamingWorld::new(
                                data.wad,
                                data.manager,
                                data.props,
                                data.terrain_tiles,
                                data.lowres_draw_by_cell,
                                terrain_hash,
                            ));
                        }
                    }

                    // (6-loading) While still on the loading layer, render the plate/spinner and stop
                    //      here — the GAME-layer master update + render below only run once realized.
                    if layers.active() != LAYER_GAME {
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

                    // (5c) GAME-layer Update (layer 4). The camera is the free-fly DEBUG cam and stays
                    //      variable-rate (the real PgSysCamera is a fixed-sim layer-4 system); the sim
                    //      work (streaming decision + animation) drains the decoupled fixed-sim clock.
                    // (3+4) timestep compute + fixed-sim accumulator drain.
                    let steps = time.advance_frame(real_dt);
                    let look = 1.6 * real_dt;

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
                    if mv != Vec3::ZERO { free_pos += mv.normalize() * sp * real_dt; }
                    let view = Mat4::look_to_lh(free_pos, fwd, Vec3::Y);

                    // --- streaming tick: decide + execute the diff (the reusable StreamingWorld) ---
                    if let Some(sw) = stream.as_mut() {
                        sw.step(&mut scene, &mut world, [free_pos.x, free_pos.y, free_pos.z]);
                        // Periodic streaming stats to the console (proof the runtime is live).
                        if stat_last.elapsed().as_secs_f32() >= 1.0 {
                            stat_last = std::time::Instant::now();
                            let s = sw.stats();
                            println!(
                                "[stream] cam({:.0},{:.0},{:.0}) resident={} awake={} regions={}/{} | live_blk_ents={} props={} models={}",
                                free_pos.x, free_pos.y, free_pos.z,
                                s.resident, s.awake, s.cached_regions, s.regions,
                                s.block_ents, s.props, s.models
                            );
                        }
                    }

                    // --- animation pass (fixed-timestep): woken rigged props, via StreamingWorld::animate
                    //     on the shared mercs2_core `Time` clock (same pose math as the mercs2_game player
                    //     system). `steps == 0` (render faster than the sim tick) holds the previous pose.
                    if steps > 0 {
                        if let Some(sw) = stream.as_ref() {
                            sw.animate(&mut world, steps as f32 * time.fixed_dt);
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

#[cfg(test)]
mod glow_card_tests {
    use super::*;

    #[test]
    fn colr_peak_bc_picks_brightest_and_guards_format() {
        // Two 0xBC-tagged RGBA8 stops: dim then bright.
        let mut body = Vec::new();
        body.extend_from_slice(&[0xBC, 0, 0, 10, 10, 10, 5, 0]);
        body.extend_from_slice(&[0xBC, 0, 0, 63, 63, 60, 40, 0]);
        let (rgb, a) = colr_peak_bc(&body).unwrap();
        assert!((rgb[0] - 63.0 / 255.0).abs() < 1e-4);
        assert!((a - 40.0 / 255.0).abs() < 1e-4);
        // A body that isn't the 0xBC stop layout must not be misread.
        assert!(colr_peak_bc(&[0u8; 16]).is_none());
        assert!(colr_peak_bc(&[0xBC, 0]).is_none());
    }

    #[test]
    fn ucfx_child_chunks_walks_descriptor_rows() {
        // Minimal UCFX: header (dao=0 → abs = 8 + u0), 1 descriptor "TRFM" at u0=0, size=4.
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&0u32.to_le_bytes()); // data_area_off
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&1u32.to_le_bytes()); // n_desc
        c.extend_from_slice(b"TRFM");
        c.extend_from_slice(&0u32.to_le_bytes()); // u0 (rel offset into 8-based body)
        c.extend_from_slice(&4u32.to_le_bytes()); // size
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        // Body region begins at abs = 8; push 8 bytes of padding then the 4-byte payload.
        while c.len() < 8 {
            c.push(0);
        }
        // Ensure at least 8+4 bytes exist from the 8-based origin.
        c.resize(c.len().max(12), 0xAB);
        let out = ucfx_child_chunks(&c);
        assert_eq!(out.len(), 1);
        assert_eq!(&out[0].0, b"TRFM");
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

    /// Live: a KNOWN animated model (the Mattias avatar `0xA3C1FABC`) must auto-populate >0 clips
    /// through the generalised load path — the core deliverable. SKIPS (passes) when vz.wad is
    /// absent so `cargo test` stays green in CI.
    #[test]
    fn live_known_animated_model_yields_clips() {
        let path = std::env::var("VZ_WAD").unwrap_or_else(|_| {
            "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
        });
        let Ok(mut w) = wad::open(&path) else {
            eprintln!("skip: vz.wad not present at {path}");
            return;
        };
        let Some((m, _, _)) = load_model_by_hash(&mut w, 0xA3C1_FABC) else {
            eprintln!("skip: player model 0xA3C1FABC not in this WAD");
            return;
        };
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
}
