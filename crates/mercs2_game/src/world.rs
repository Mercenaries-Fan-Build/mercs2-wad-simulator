//! GAME render/boot: the full third-person + free-fly world render path — player avatar, TPS/free
//! camera toggle, and the 10-stage full load (terrain / heightmap / player / clips / c3 cells /
//! placements / interior / props). This is GAME code: it spawns the player + the PMC interior + world
//! content, driving the asset-agnostic engine through its public API (`mercs2_engine::{wad, mesh, pose,
//! scene, render, game_world, worldutil}` + `crate::pmc`).
//!
//! Recovered from the pre-teardown engine bin (git 78944ed) — it was wrongly deleted with that bin
//! (it is render/boot code, not the engine). `run_scene_world_loading` is the `--world` / TPS entry.

#![allow(dead_code, unused_imports, unused_variables, unused_mut, clippy::all)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mercs2_core::glam::{Mat4, Quat, Vec3};
use mercs2_core::{AnimState, Entity, ModelRef, Schedule, SkinPalette, Time, Transform, World};
use mercs2_engine::game_world::*;
use mercs2_engine::mesh::Vertex;
use mercs2_engine::render::*;
use mercs2_engine::scene::{AssetStore, ModelAnim, Scene};
use mercs2_engine::worldutil::*;
use mercs2_engine::{mesh, pose, wad};
use winit::event::{DeviceEvent, ElementState, Event, KeyEvent, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, WindowBuilder};

use crate::pmc::{load_pmc_interior, PMC_INTERIOR_SPAWN};

// ---------------------------------------------------------------------------
//   Restored render/boot code (verbatim from the deleted engine bin, path-adapted via the `use`
//   prelude above). See git 78944ed:crates/mercs2_engine/src/main.rs.
// ---------------------------------------------------------------------------
fn build_placement_markers(
    placements: &[mercs2_formats::placement::Placement],
) -> (Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>) {
    const H: f32 = 3.0; // marker height (m)
    const R: f32 = 0.9; // marker base half-width (m)
    // Upright tetra: apex above, 3 base corners. (LH +Y up.)
    let local: [[f32; 3]; 4] = [
        [0.0, H, 0.0],
        [-R, 0.0, -R],
        [R, 0.0, -R],
        [0.0, 0.0, R],
    ];
    let faces: [[u32; 3]; 4] = [[0, 2, 1], [0, 3, 2], [0, 1, 3], [1, 2, 3]];
    let mut verts: Vec<Vertex> = Vec::with_capacity(placements.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(placements.len() * 12);
    for p in placements {
        let color = if placement_is_pmc_subset(p) {
            [0.95, 0.35, 0.10] // PMC/base subset: warm orange
        } else if is_out_of_bounds(&p.pos) {
            [0.95, 0.90, 0.15] // off-map candidate: yellow
        } else {
            [0.20, 0.55, 0.90] // ordinary placement: cool blue
        };
        let base = verts.len() as u32;
        for l in &local {
            verts.push(Vertex {
                pos: [p.pos[0] + l[0], p.pos[1] + l[1], p.pos[2] + l[2]],
                color,
                uv: [0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            });
        }
        for f in &faces {
            indices.push(base + f[0]);
            indices.push(base + f[1]);
            indices.push(base + f[2]);
        }
    }
    let draws = vec![mesh::DrawGroup {
        index_start: 0,
        index_count: indices.len() as u32,
        diffuse: None, // vertex-color only (white fallback texture)
        specular: None,
        normal: None,
        group_index: 0,
    }];
    (verts, indices, draws)
}




/// One streamable prop's spawn recipe: the mesh it renders as + its authored world Transform
/// (pos + full quat, native game space, no flip), joined from the `ModelName`/`Transform` COMPs.


/// Headless LOD reverse-engineering probe. Answers two build-blocking questions with real data:
///  (a) PER-PROP LOD: do the 464 `ModelName` prop meshes carry multi-tier LOD sub-objects (distinct
///      `SEGM.state_mask` values within one container), or is LOD a building/vehicle-only feature?
///      The renderer currently hardcodes `LOD_BIT=0x01` (keeps tier-0 sub-objects, skips the rest).
///  (b) FINE-CELL QUADTREE: for a multi-tier c3 cell, are the fine leaf blocks spatially DISJOINT
///      (a real quadtree we can stream per-subregion by distance) or overlapping?

struct LoadProgress {
    current: std::sync::atomic::AtomicU32,
    total: std::sync::atomic::AtomicU32,
    t0: std::time::Instant,
}

impl LoadProgress {
    fn new(total: u32) -> Self {
        LoadProgress {
            current: std::sync::atomic::AtomicU32::new(0),
            total: std::sync::atomic::AtomicU32::new(total.max(1)),
            t0: std::time::Instant::now(),
        }
    }
    /// Mark a named stage complete (call AFTER the stage's work) and log it.
    fn step(&self, name: &str) {
        use std::sync::atomic::Ordering;
        let k = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let n = self.total.load(Ordering::Relaxed);
        println!("[load] stage {k}/{n}: {name} (+{:.1}s)", self.t0.elapsed().as_secs_f64());
    }
    /// Completed fraction 0..1 (the bar's target; the render loop eases toward it).
    fn fraction(&self) -> f32 {
        use std::sync::atomic::Ordering;
        self.current.load(Ordering::Relaxed) as f32 / self.total.load(Ordering::Relaxed) as f32
    }
}

/// Everything `--world` needs loaded before play: plain CPU data (Send), so it can be produced
/// on a background thread while the window shows the loading spinner.
struct WorldData {
    terrain: LoadedModel,
    player: Option<LoadedModel>,
    cells: Vec<(LoadedModel, [f32; 3])>,
    /// Merged placement-marker mesh (one model + one static entity), when `--placements` is set.
    placements: Option<LoadedModel>,
    /// PMC-subset real-geometry models resolved by name→mesh (currently none — see report).
    pmc_models: Vec<(LoadedModel, [f32; 3], f32)>,
    /// PMC interior instances (`--interior`): resolved interior geometry + authored world Transform
    /// (position + full quaternion, native game space, no flip).
    interior: Vec<(LoadedModel, [f32; 3], [f32; 4])>,
    /// Exterior `ModelName` props near the spawn (`--props`): distinct mesh + its placement instances.
    props: Vec<(u32, LoadedModel, Vec<PropInstance>)>,
    /// Interior `ModelName` furniture (`--interior`): distinct mesh + its placement instances (all).
    interior_props: Vec<(u32, LoadedModel, Vec<PropInstance>)>,
    hmap: HeightMap,
    /// Dynamic `LightObject` point lights harvested from layers_static + the interior state blocks
    /// (world-space). Fed to `Scene::set_lights`; the scene uploads the nearest set per frame.
    lights: Vec<mercs2_engine::render::GpuLight>,
    /// Authored `global_particle_*` FX placements (effect name + world position) — each starts an
    /// emitter (classified by name). The faithful producer for environmental particle effects
    /// (fire/smoke/steam). Static environmental *glows* (god-ray light shafts) are split out into
    /// `glow_cards` at load, where the WAD is open to read their effect template.
    particle_fx: Vec<(String, [f32; 3])>,
    /// Static additive glow cards for the environmental light-shaft FX (`global_particle_env_godray2`
    /// — the PMC hall god rays descending from the dome). Position/size/tint are data-driven from the
    /// placement + the effect's `TRFM`/`COLR` (see `mercs2_engine::game_world::glow_card_for_effect`).
    glow_cards: Vec<mercs2_engine::particles::GlowCard>,
}

/// Number of `progress.step` calls in `load_world_data` (keep in sync when adding stages).
const LOAD_STAGES: u32 = 10;

/// Exterior prop bounding: load only props within this radius (m) of the pool spawn, capped at
/// `EXTERIOR_PROP_CAP` distinct meshes, so `--props` stays light next to the full map.
const EXTERIOR_PROP_RADIUS: f32 = 400.0;
const EXTERIOR_PROP_CAP: usize = 200;

/// The `--world` loading work (WAD open, terrain merge, heightmap, player avatar + clips,
/// optional c3 cells + placement markers) — the former inline `run_world` body plus placements.
fn load_world_data(
    wadpath: &str,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    recruits: crate::pmc::RecruitUnlocks,
    stockpile: &crate::pmc::Stockpile,
    progress: &LoadProgress,
) -> Result<WorldData, String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
    let ntris = tm.indices.len() / 3;
    println!(
        "[world] terrain: {} verts / {ntris} tris / {} tiles placed / {} tiles decoded (TOC {})",
        tm.positions.len(), tm.tiles_placed, tm.tiles_decoded, tm.toc_entry_count
    );
    progress.step("terrain");
    let hmap = HeightMap::build(&tm);
    println!(
        "[world] heightmap: h(0,0)={:.2} h(100,100)={:.2} h(-100,100)={:.2} h(100,-100)={:.2} h(-100,-100)={:.2}",
        hmap.height_at(0.0, 0.0), hmap.height_at(100.0, 100.0), hmap.height_at(-100.0, 100.0),
        hmap.height_at(100.0, -100.0), hmap.height_at(-100.0, -100.0)
    );
    progress.step("heightmap");
    let textured = tm.texture.is_some();
    let verts = terrain_to_vertices(&tm, textured);
    let mut textures: TexMap = std::collections::HashMap::new();
    // One draw group spanning the whole mesh, bound to the shared atlas hash 0.
    let draws = if let Some(t) = tm.texture.clone() {
        textures.insert(0, t);
        vec![mesh::DrawGroup {
            index_start: 0,
            index_count: tm.indices.len() as u32,
            diffuse: Some(0),
            specular: None,
            normal: None,
            group_index: 0,
        }]
    } else {
        Vec::new()
    };

    let terrain = LoadedModel {
        hash: 0x7E44_A100, // arbitrary key for the merged terrain mesh
        verts,
        indices: tm.indices.clone(),
        draws,
        textures,
        skin: mesh::SkinData::identity(), // identity fit: terrain verts stay in world metres
        clips: Vec::new(),
    };
    progress.step("vertices");

    // Player avatar (Mattias) for the third-person view, at RAW model scale (identity fit) so it
    // sits in world metres alongside the terrain rather than fit-normalised. Idle clip 0x24F8C8E6
    // plus the walk clip 0x53682784 for WASD locomotion.
    // NOTE: world scale and facing are first-pass and not yet calibrated.
    // animate=false: skip load_from_wad's own animgroup scan — all three clips (idle/walk/run)
    // come from ONE cached scan below instead of three full-archive passes (~20 s -> ~7 s load).
    let player = match load_from_wad(wadpath, Some("0xA3C1FABC".to_string()), None, false, None) {
        Ok((v, i, d, t, mut s, _c, h, _)) => {
            progress.step("player");
            s.center = [0.0, 0.0, 0.0];
            s.scale = 1.0;
            let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
            let wanted = [0x24F8_C8E6u32, 0x5368_2784, 0x867B_166D]; // idle, walk, run
            let names = ["idle", "walk", "run"];
            let mut clips: Vec<ClipAnim> = Vec::new();
            for (found, (&h, name)) in load_clips_for_rig(&mut w, &hier, &wanted)
                .into_iter()
                .zip(wanted.iter().zip(names))
            {
                match found {
                    Some(ca) => {
                        println!(
                            "[world] {name} clip 0x{:08X}: {} tracks, {} frames, {:.2}s",
                            ca.name_hash, ca.clip.num_tracks, ca.clip.num_frames, ca.clip.duration
                        );
                        clips.push(ca);
                    }
                    None => eprintln!("[world] {name} clip 0x{h:08X} not found"),
                }
            }
            Some(LoadedModel { hash: h, verts: v, indices: i, draws: d, textures: t, skin: s, clips })
        }
        Err(e) => {
            eprintln!("[world] player avatar 0xA3C1FABC load failed: {e}");
            progress.step("player");
            None
        }
    };
    progress.step("clips");

    // Hi-res c3 streaming-cell geometry near the spawn (opt-in; default off keeps --world stable).
    let cells = if load_cells {
        load_c3_cells(&mut w, 400.0, 16)
    } else {
        Vec::new()
    };
    progress.step(if load_cells { "cells" } else { "cells (skipped)" });

    // World placements (layers_static block 29): a merged marker mesh + the interior-hunt report,
    // plus an attempt to resolve the PMC-subset to real geometry (opt-in via `--placements`).
    let (placements, pmc_models) = if load_placements {
        match mercs2_formats::placement::load_placements(&ls) {
            Ok(pl) => {
                report_interior_hunt(&pl);
                let (verts, indices, draws) = build_placement_markers(&pl);
                println!(
                    "[placements] marker mesh: {} placements -> {} verts / {} tris",
                    pl.len(),
                    verts.len(),
                    indices.len() / 3
                );
                let markers = LoadedModel {
                    hash: 0x504C_4143, // "PLAC" — arbitrary key for the merged marker mesh
                    verts,
                    indices,
                    draws,
                    textures: TexMap::new(),
                    skin: mesh::SkinData::identity(),
                    clips: Vec::new(),
                };
                let pmc = resolve_pmc_geometry(&mut w, &pl);
                (Some(markers), pmc)
            }
            Err(e) => {
                eprintln!("[placements] load failed: {e}");
                (None, Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };
    progress.step(if load_placements { "placements" } else { "placements (skipped)" });

    // PMC interior (`--interior`): placement-driven interior geometry from state block 667, placed
    // at authored world coords (floor Y≈450.8) so the spawn drops the player inside the room.
    let interior = if spawn_interior {
        match load_pmc_interior(&mut w, recruits, stockpile) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[interior] load failed: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    progress.step(if spawn_interior { "interior" } else { "interior (skipped)" });

    // Exterior props (`--props`): ModelName placements in layers_static (block 29) within
    // EXTERIOR_PROP_RADIUS of the pool spawn, cap EXTERIOR_PROP_CAP distinct meshes.
    let props = if load_props {
        load_model_props(&mut w, &ls, Some(EXTERIOR_SPAWN), EXTERIOR_PROP_RADIUS, EXTERIOR_PROP_CAP)
    } else {
        Vec::new()
    };
    progress.step(if load_props { "props" } else { "props (skipped)" });

    // Interior props (`--interior`): ALL ModelName furniture placements in state block 667, at
    // their authored world transforms (the same anchor the shells are centred on).
    let interior_props = if spawn_interior {
        match wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK) {
            Ok(dec) => load_model_props(&mut w, &dec, None, 0.0, usize::MAX),
            Err(e) => {
                eprintln!("[interior props] state block {PMC_INTERIOR_STATE_BLOCK} decompress failed: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    progress.step(if spawn_interior { "interior props" } else { "interior props (skipped)" });

    // Dynamic point lights: harvest `LightObject` COMPs (world-space) from layers_static (exterior) +
    // the interior state block (the villa's `Light_small_*`). The block cache makes the re-decompress
    // of the interior block a hit. Fed to Scene::set_lights so the shell/props are actually lit.
    let mut lights = mercs2_engine::game_world::placed_lights_to_gpu(
        &mercs2_formats::placement::light_inventory(&ls),
    );
    // Environmental FX placements (Name-keyed `global_particle_*` Transforms) in the interior. These
    // split by kind: static environmental light-shaft glows (god rays) become additive glow cards
    // resolved against their effect template here (WAD open); fire/smoke/steam stay as particle
    // emitters classified by name at render-thread start.
    let mut particle_fx: Vec<(String, [f32; 3])> = Vec::new();
    let mut glow_cards: Vec<mercs2_engine::particles::GlowCard> = Vec::new();
    if spawn_interior {
        if let Ok(dec) = wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK) {
            lights.extend(mercs2_engine::game_world::placed_lights_to_gpu(
                &mercs2_formats::placement::light_inventory(&dec),
            ));
            for p in mercs2_formats::placement::load_placements(&dec).unwrap_or_default() {
                let raw = p.name.as_deref().unwrap_or("");
                let name = raw.split(" 0x").next().unwrap_or(raw).trim_start_matches('_');
                if !name.starts_with("global_particle") {
                    continue;
                }
                if is_light_shaft_fx(name) {
                    glow_cards.push(mercs2_engine::game_world::glow_card_for_effect(&mut w, name, p.pos));
                } else {
                    particle_fx.push((name.to_string(), p.pos));
                }
            }
        }
    }
    println!(
        "[world] dynamic lights harvested: {}; particle placements: {}; light-shaft glows: {}",
        lights.len(), particle_fx.len(), glow_cards.len()
    );

    Ok(WorldData { terrain, player, cells, placements, pmc_models, interior, props, interior_props, hmap, lights, particle_fx, glow_cards })
}

/// Whether a `global_particle_*` name is a static environmental light-shaft ("god ray") FX. These are
/// authored as additive textured cards, not spewing emitters — reversed from `global_env_godray2`
/// (effects block: GEOM + TRFM + additive COLR, `EMTR` empty). Routed to a static additive glow card,
/// NOT the particle sim.
fn is_light_shaft_fx(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("godray") || n.contains("lightshaft") || n.contains("light_shaft") || n.contains("_env_light")
}

/// Classify a `global_particle_*` effect name → a billboard [`EmitterDesc`] for the particle sim.
/// Static light-shaft FX are handled separately (see [`is_light_shaft_fx`] / glow cards) and never
/// reach here. Name-heuristic mapping until the `EffectTemplate → EmitterDesc` decode is pinned.
fn classify_particle(name: &str) -> Option<mercs2_engine::particles::EmitterDesc> {
    use mercs2_engine::particles::EmitterDesc;
    let n = name.to_ascii_lowercase();
    if n.contains("fire") || n.contains("flame") || n.contains("ember") {
        return Some(EmitterDesc::demo_fire());
    }
    if n.contains("smoke") || n.contains("dust") || n.contains("steam") || n.contains("fog") {
        return Some(EmitterDesc::demo_smoke());
    }
    None // unknown effect type — don't fabricate one
}

/// Attempt to resolve the PMC-base subset of placements to REAL model geometry (Task 3).
///
/// CRITICAL GAP: `layers_static` Transform records key entities by a u32 *entity key* and carry
/// only pos/quat — NOT a model-asset hash. The `Name` COMP gives a gameplay name
/// (e.g. `_pmcoutpost_bld_barracks01`), not an asset hash either. Mapping name→mesh needs a
/// SEPARATE table that this block does not contain (candidates: the per-cell c3 `model` containers,
/// or an ASET/name-hash lookup — `pandemic_hash_m2(name)` is the natural first guess). We try that
/// hash as the model asset hash and load any that resolve; most will miss, which is the reportable
/// gap. Capped at 64 distinct models. Returns (model, world-pos, yaw) per resolved placement.
fn resolve_pmc_geometry(
    w: &mut wad::Wad,
    placements: &[mercs2_formats::placement::Placement],
) -> Vec<(LoadedModel, [f32; 3], f32)> {
    use mercs2_formats::placement::yaw_from_quat;
    let subset: Vec<&mercs2_formats::placement::Placement> =
        placements.iter().filter(|p| placement_is_pmc_subset(p)).collect();
    // Distinct candidate asset hashes = pandemic_hash_m2(name) for named subset entries.
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out: Vec<(LoadedModel, [f32; 3], f32)> = Vec::new();
    let (mut tried, mut ok) = (0u32, 0u32);
    for p in &subset {
        if out.len() >= 64 {
            break;
        }
        let Some(name) = p.name.as_deref() else { continue };
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        if !seen.insert(hash) {
            continue;
        }
        tried += 1;
        match wad::extract_container(w, hash) {
            Ok(container) => match mesh::build_indexed_from_container(&container) {
                Ok((verts, indices, draws, stats)) => {
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
                    println!(
                        "[pmc-geo] '{name}' hash=0x{hash:08X}: LOADED {} verts / {} tris",
                        verts.len(), indices.len() / 3
                    );
                    out.push((
                        LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
                        p.pos,
                        yaw_from_quat(&p.quat),
                    ));
                    ok += 1;
                }
                Err(e) => println!("[pmc-geo] '{name}' hash=0x{hash:08X}: container parse FAILED: {e}"),
            },
            Err(_) => { /* no model ASET for this name-hash — the expected gap */ }
        }
    }
    println!(
        "[pmc-geo] name->mesh via pandemic_hash_m2: {} distinct names tried, {} resolved to a model ASET (of {} PMC-subset placements)",
        tried, ok, subset.len()
    );
    out
}




/// Enumerate c3 streaming-cell blocks (PTHS paths matching `c3####`), keep the ones whose block
/// entry table carries a `model`-format container (type_hash 0x5B724250 — the SAME UCFX layout as
/// characters, so `mesh::build_indexed_from_container` parses them), and load the cells whose grid
/// centre lies within `radius` m of the spawn (0,0), capped at `cap`, nearest first. Returns
/// (model, cell-origin translation) pairs; translation is zero when the verts prove already
/// world-space (bbox centre inside the cell bounds — logged either way).
fn load_c3_cells(w: &mut wad::Wad, radius: f32, cap: usize) -> Vec<(LoadedModel, [f32; 3])> {
    use mercs2_formats::ucfx::parse_block_entry_table;

    let c3_blocks: Vec<(u16, u32)> = wad::block_paths(w)
        .iter()
        .enumerate()
        .filter_map(|(i, p)| c3_cell_id_from_path(p).map(|cid| (i as u16, cid)))
        .collect();
    let mut mesh_blocks: Vec<(u16, u32)> = Vec::new();
    for &(blk, cid) in &c3_blocks {
        let Ok(head) = wad::peek_block_head(w, blk, 16384) else { continue };
        let (_count, entries) = parse_block_entry_table(&head);
        if entries.iter().any(|e| e.type_hash == wad::MODEL_TYPE_HASH) {
            mesh_blocks.push((blk, cid));
        }
    }
    println!(
        "[cells] {} c3 blocks in PTHS; {} carry model-format (0x{:08X}) geometry",
        c3_blocks.len(),
        mesh_blocks.len(),
        wad::MODEL_TYPE_HASH
    );

    let mut all: Vec<(f32, u16, u32)> = mesh_blocks
        .iter()
        .map(|&(blk, cid)| {
            let (x, z) = c3_cell_centre(cid);
            ((x * x + z * z).sqrt(), blk, cid)
        })
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut near: Vec<(f32, u16, u32)> = all.iter().copied().filter(|&(d, _, _)| d <= radius).collect();
    if near.is_empty() && !all.is_empty() {
        // No mesh cell inside the strict radius (spawn sits in a mesh-less region of the grid):
        // fall back to the nearest cluster within 2× radius, logged so the miss is visible.
        eprintln!(
            "[cells] no mesh cell within {radius:.0} m of spawn (nearest = cell {} at {:.0} m); falling back to ≤{:.0} m",
            all[0].2, all[0].0, radius * 2.0
        );
        near = all.iter().copied().filter(|&(d, _, _)| d <= radius * 2.0).collect();
    }
    near.truncate(cap);

    let mut out: Vec<(LoadedModel, [f32; 3])> = Vec::new();
    for &(dist, blk, cid) in &near {
        let (cx, cz) = c3_cell_centre(cid);
        let dec = match wad::decompress_block_index(w, blk) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[cells] cell {cid} block {blk}: decompress failed: {e}");
                continue;
            }
        };
        // Slice the model container out of the block, keeping its name hash for the scene key.
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
        let Some((hash, s0, s1)) = model else {
            eprintln!("[cells] cell {cid} block {blk}: model entry vanished on full decompress");
            continue;
        };
        let (verts, indices, draws, stats) = match mesh::build_indexed_from_container(&dec[s0..s1]) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[cells] cell {cid} block {blk} model 0x{hash:08X}: container parse FAILED: {e}");
                continue;
            }
        };
        // World-space check: bbox centre already inside this cell's bounds ⇒ verts are
        // world-space (spawn at identity); otherwise cell-local (offset to the cell centre).
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
        skin.scale = 1.0; // native metres; placement comes from the entity transform
        println!(
            "[cells] cell {cid} block {blk} model 0x{hash:08X}: {} verts / {} tris / {} groups / {} textures, d={dist:.0} m, bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] -> {}",
            verts.len(),
            indices.len() / 3,
            draws.len(),
            textures.len(),
            stats.bbox_min[0], stats.bbox_max[0],
            stats.bbox_min[1], stats.bbox_max[1],
            stats.bbox_min[2], stats.bbox_max[2],
            if world_space { "WORLD-SPACE (identity)" } else { "cell-local (offset to cell centre)" }
        );
        out.push((
            LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
            offset,
        ));
    }
    println!("[cells] loaded {} of {} in-range cells (cap {cap})", out.len(), near.len());
    out
}


/// One resolved PMC-interior entity from `resolve_pmc_interior_entities`: its key, canonical name,
/// the block + AUTHORED world Transform (pos + full quat) that keyed it, and — when found — the mesh
/// hash it renders as (via a keyed `ModelName` COMP, else the `pandemic_hash_m2(name)` fallback).

fn tri_height_at(x: f32, z: f32, vx: [f32; 3], vz: [f32; 3], vy: [f32; 3]) -> Option<f32> {
    let d = (vz[1] - vz[2]) * (vx[0] - vx[2]) + (vx[2] - vx[1]) * (vz[0] - vz[2]);
    if d.abs() < 1e-9 {
        return None;
    }
    let a = ((vz[1] - vz[2]) * (x - vx[2]) + (vx[2] - vx[1]) * (z - vz[2])) / d;
    let b = ((vz[2] - vz[0]) * (x - vx[2]) + (vx[0] - vx[2]) * (z - vz[2])) / d;
    let c = 1.0 - a - b;
    if a < -1e-4 || b < -1e-4 || c < -1e-4 {
        return None;
    }
    Some(a * vy[0] + b * vy[1] + c * vy[2])
}

/// Scene path for the terrain: build ONE merged world-space mesh, load it as a
/// single model, spawn ONE static entity (identity transform / palette), and run
/// an elevated bird's-eye camera framing the whole grid.
/// World scene with two cameras: **free-fly** (dev/engine) and **third-person over-the-shoulder**
/// (gameplay), toggled with Tab. Terrain is a static entity; the optional player avatar is placed
/// on it and driven by WASD (camera-relative) with the camera trailing behind + above + shouldered.
/// The animation system idles the avatar (walk clip while moving). Ground height comes from
/// the heightmap. Start in third-person if `start_tps` and a player exists.
///
/// The window + `Scene` open IMMEDIATELY with an animated loading spinner; `load_world_data`
/// runs on a background thread and the loaded world is wired in when its result arrives.
pub async fn run_scene_world_loading(
    wadpath: String,
    start_tps: bool,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    interior_orbit: bool,
    recruits: crate::pmc::RecruitUnlocks,
    stockpile: crate::pmc::Stockpile,
) {
    use mercs2_engine::scene::{AssetStore, ModelAnim, Scene};
    use mercs2_core::glam::{Mat4, Quat, Vec3};
    use mercs2_core::{AnimState, Entity, ModelRef, Schedule, SkinPalette, Time, Transform, World};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::f32::consts::PI;
    use std::rc::Rc;
    use winit::event::{DeviceEvent, ElementState};
    use winit::window::CursorGrabMode;

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    const CLIP_IDLE: u32 = 0x24F8_C8E6;
    const CLIP_WALK: u32 = 0x5368_2784;
    const CLIP_RUN: u32 = 0x867B_166D;
    // Locomotion feel tunables.
    const ANIM_BLEND_SEC: f32 = 0.25; // crossfade duration on clip switches
    const TURN_RATE: f32 = 12.0; // rad/s exponential yaw damp toward the move direction
    // Human-scale locomotion (world units = metres): the 1.0 s walk cycle strides ~2 m, so
    // ~2 m/s walk keeps feet planted under FOOT_SYNC; sprint ~6.5 m/s. (The earlier 14/60
    // were vehicle speeds — user-confirmed mismatch against the geometry.)
    const WALK_SPEED: f32 = 2.2; // m/s
    const RUN_SPEED: f32 = 6.5; // m/s (Shift)
    const ACCEL: f32 = 12.0; // m/s^2 easing toward a higher target speed
    const DECEL: f32 = 16.0; // m/s^2 easing toward a lower target speed
    const FOOT_SYNC: bool = true; // scale locomotion playback by current/target speed (0.8..1.2)

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Mercenaries 2 — world (Tab: free / third-person)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );
    // Mouse-look: grab + hide the cursor on the world window (Confined preferred, Locked
    // fallback). Arrow keys stay as a fallback steer; Esc still exits.
    if let Err(e) = window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
    {
        eprintln!("[world] cursor grab unavailable ({e}); arrow keys still steer");
    }
    window.set_cursor_visible(false);
    let mut scene = Scene::new(window.clone()).await;
    // Placeholder distance fog + sky (stand-in for PgSky/PgSun/PgCloud). Tunables: warm-haze
    // color, density 0.00016 (~30% haze at 2.5 km, ~50% at 4.5 km — depth cue at ground level
    // without white-out from the aerial free cam; 0.00035 washed out the whole map), start 60 m.
    scene.set_fog([0.55, 0.62, 0.70], 0.00016, 60.0);
    // Real loading-screen art: the lti_precache1 plate from shell.wad (sibling of vz.wad),
    // extracted up front (fast) so the loading phase shows it; spinner-only if unavailable.
    match wad::shell_loading_plate(&wadpath) {
        Ok(td) => {
            println!(
                "[load] shell.wad loading plate lti_precache1 (0x7329D083) {}x{} {:?}",
                td.width, td.height, td.format
            );
            scene.set_loading_art(&td);
        }
        Err(e) => eprintln!("[load] shell.wad loading art unavailable ({e}); spinner only"),
    }
    let mut world = World::new();

    // Background loader: all WAD/terrain/player parsing happens off the render thread; the
    // result lands on this channel and is wired into the scene/world on arrival.
    let (tx, rx) = std::sync::mpsc::channel::<Result<WorldData, String>>();
    let progress = Arc::new(LoadProgress::new(LOAD_STAGES));
    let loader_progress = progress.clone();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = load_world_data(&wadpath, load_cells, load_placements, spawn_interior, load_props, recruits, &stockpile, &loader_progress);
        if r.is_ok() {
            println!("[load] done in {:.1}s", t0.elapsed().as_secs_f64());
        }
        let _ = tx.send(r);
    });

    // World-dependent state, wired in when the loader finishes (defaults until then).
    let mut hmap: Option<HeightMap> = None;
    let store = Rc::new(RefCell::new(AssetStore::default()));
    // Spawn at the PMC HQ compound (game coords, docs/coordinate_systems.md Example 1); Y is
    // terrain-snapped at spawn. The base GEOMETRY itself arrives with the placements brick — for
    // now this at least drops the player where the PMC is, not the empty map centre.
    // Spawn coords are the game's own boot-log values (MrxUtil._TeleportHero, mrxutil.lua:490),
    // used with the authored Y VERBATIM — no ground-snap at spawn in either mode:
    //   * `--interior`: the authored PMC INTERIOR teleport coord `PMC_INTERIOR_SPAWN`
    //     (3794.0427, 450.7505, -3911.0322) — the off-map, high-Y (above the ~393 terrain cap)
    //     SE-corner interior cell. Height-follow stays OFF (its floor is at ~450, not the terrain).
    //     The interior geometry is now placed at its OWN authored Transforms (no synthetic offset),
    //     so the spawn sits inside the assembled recruit-interior meshes.
    //   * default: the EXTERIOR back-door/pool coord (2560.26, -13.18, -926.25) near the PMC HQ.
    //     Per-frame terrain height-follow kicks in only while walking outdoors (below).
    let mut player_pos = if spawn_interior {
        println!("[world] --interior: spawning at PMC interior teleport coord (3794.043, 450.751, -3911.032) [interior placed at authored transforms; height-follow OFF]");
        Vec3::new(PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2])
    } else {
        Vec3::new(2560.2646, -13.1779, -926.2511)
    };
    let mut player_foot = 0.0f32;
    let mut player_entity: Option<Entity> = None;
    let mut player_yaw = 0.0; // matches the spawn rotation below (faces +Z, into the open hall)
    let mut player_speed = 0.0f32; // eased ground speed (m/s)
    let mut player_move_dir = Vec3::new(0.0, 0.0, 1.0); // last input direction (kept while decelerating)
    let mut has_run = false;
    let (mut dur_walk, mut dur_run) = (1.0f32, 1.0f32);

    // World-space collision triangle soup, filled from the structural geometry (interior shells +
    // c3 building cells) when the loader delivers. Consumed by the camera boom (raycast, so it
    // stops clipping through walls) and the player (capsule push-out, so you can't walk through
    // buildings). See `crate::collision`.
    let mut collision_tris: Vec<[Vec3; 3]> = Vec::new();

    // Animation system (idles/walks the avatar), same as the ECS scene except clips are selected
    // by `AnimState.clip` and root locomotion is stripped (the entity Transform drives movement).
    let mut time = Time::new(60.0);
    let mut schedule = Schedule::new();
    let assets = store.clone();
    schedule.add_system("animation", move |world: &mut World, time: &Time| {
        let assets = assets.borrow();
        for (_e, (state, palette, mref)) in world
            .query::<(&mut AnimState, &mut SkinPalette, &ModelRef)>()
            .iter()
        {
            if !state.playing {
                continue;
            }
            let Some(ma) = assets.models.get(&mref.model) else { continue };
            let Some(ca) = ma.clips.get(&state.clip).or_else(|| ma.clips.values().next()) else { continue };
            let dur = ca.clip.duration.max(1e-3);
            state.time = (state.time + time.dt * state.speed) % dur;
            // Crossfade: while the previous clip is still fading out, advance it on its own
            // duration and blend its pose toward the current clip's (Havok blendPoses math).
            if state.blend < 1.0 {
                if let Some(cp) = ma.clips.get(&state.prev_clip) {
                    let pdur = cp.clip.duration.max(1e-3);
                    state.prev_time = (state.prev_time + time.dt * state.speed) % pdur;
                    state.blend = (state.blend + time.dt / ANIM_BLEND_SEC).min(1.0);
                    let sa = cp.clip.sample_local(state.prev_time);
                    let sb = ca.clip.sample_local(state.time);
                    palette.mats = pose::havok_palette_blend_in_place(
                        &ma.rig,
                        &sa, &cp.track_to_hier, cp.num_transform_tracks,
                        &sb, &ca.track_to_hier, ca.num_transform_tracks,
                        state.blend,
                    );
                    continue;
                }
                state.blend = 1.0;
            }
            let sample = ca.clip.sample_local(state.time);
            palette.mats = pose::havok_palette_in_place(&ma.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks);
        }
    });

    // Camera state. Free-fly starts elevated over the map centre; third-person orbits the player.
    #[derive(PartialEq)]
    enum CamMode {
        Free,
        ThirdPerson,
    }
    let mut mode = CamMode::Free; // switched to third-person when the loaded player spawns
    let mut free_pos = Vec3::new(0.0, 2500.0, 4500.0);
    // Spawn camera rotated 180° from the original (was PI, facing -Z) so it opens looking INTO the room.
    let mut free_yaw: f32 = 0.0;
    let mut free_pitch: f32 = -0.5;
    // Third-person camera sits BEHIND the player (who spawns facing +Z), looking over the shoulder.
    // tp_yaw = 0 => camera dir = +Z, eye on the -Z side of the focus (behind the player). Player facing
    // and tp_yaw MUST stay consistent — a mismatch puts the eye in front (a face-cam) and swaps left/right.
    let mut tp_yaw: f32 = 0.0;
    let mut tp_pitch: f32 = -0.12;
    let mut held: HashSet<KeyCode> = HashSet::new();
    let mut mouse_btns: HashSet<winit::event::MouseButton> = HashSet::new();
    // Input bindings from the retail Mercs2.ini (falls back to retail defaults if absent).
    let bindings = mercs2_engine::input::find_mercs2_ini()
        .map(|p| mercs2_engine::input::Bindings::load(&p))
        .unwrap_or_default();
    let mut gamepad = mercs2_engine::input::Gamepad::new();
    use mercs2_engine::input::Action;
    let mut loading = true;
    let load_start = std::time::Instant::now();
    // Bar fill shown on the loading screen: eased toward the loader's staged fraction each
    // frame so stage completions animate instead of jumping.
    let mut bar_shown = 0.0f32;
    let mut bar_last_t = 0.0f32;
    let mut last = std::time::Instant::now();
    let mut mouse_acc: (f32, f32) = (0.0, 0.0); // cursor-path px accumulated between frames
    let mut mouse_raw_acc: (f32, f32) = (0.0, 0.0); // raw-delta px accumulated between frames
    let mut mouse_dbg_frames: u32 = 0;
    // Mouse source auto-detect. Normal 2026 input = raw deltas (DeviceEvent::MouseMotion).
    // Shadow cloud PCs stream ABSOLUTE 0..65535 coords through raw input, making those "deltas"
    // huge/one-signed garbage — detect that and latch to the CursorMoved+recentre fallback.
    // 0 = undecided (use cursor path), 1 = relative latched (raw), 2 = absolute latched (cursor).
    let mut mouse_src: u8 = 0;
    let mut mouse_sane_events: u32 = 0;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. },
                    ..
                } => match (code, state) {
                    (KeyCode::Escape, _) => elwt.exit(),
                    (KeyCode::Tab, ElementState::Pressed) => {
                        mode = if mode == CamMode::Free { CamMode::ThirdPerson } else { CamMode::Free };
                    }
                    (c, ElementState::Pressed) => {
                        held.insert(c);
                    }
                    (c, ElementState::Released) => {
                        held.remove(&c);
                    }
                },
                WindowEvent::MouseInput { button, state, .. } => {
                    if state == ElementState::Pressed {
                        mouse_btns.insert(button);
                    } else {
                        mouse_btns.remove(&button);
                    }
                }
                WindowEvent::Resized(size) => scene.resize(size),
                // Cursor-position look: delta from window centre, then recentre. Works on
                // absolute-input setups (streamed/cloud) where raw deltas are meaningless.
                WindowEvent::CursorMoved { position, .. } => {
                    let (cx, cy) = (scene.size.width as f64 / 2.0, scene.size.height as f64 / 2.0);
                    mouse_acc.0 += (position.x - cx) as f32;
                    mouse_acc.1 += (position.y - cy) as f32;
                    let _ = scene
                        .window
                        .set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                }
                WindowEvent::RedrawRequested => {
                    // Loading phase: animate the spinner until the background loader delivers,
                    // then wire the world in (GPU uploads + entity spawns) and start playing.
                    if loading {
                        match rx.try_recv() {
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                let t = load_start.elapsed().as_secs_f32();
                                let dt = (t - bar_last_t).max(0.0);
                                bar_last_t = t;
                                // Exponential ease toward the staged target (~6/s rate).
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
                                eprintln!("[world] loader thread died without a result");
                                elwt.exit();
                                return;
                            }
                            Ok(Err(e)) => {
                                eprintln!("[world] load failed: {e}");
                                elwt.exit();
                                return;
                            }
                            Ok(Ok(mut data)) => {
                                // Terrain: one static entity at identity (its verts are already world-space).
                                // Skipped in --interior mode: the interior is off-map at Y~450 sitting above
                                // the SE-corner terrain peak (~Y400), which otherwise occludes the whole room.
                                let terrain = data.terrain;
                                if !std::env::args().any(|a| a == "--interior") {
                                    scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
                                    world.spawn((
                                        Transform::IDENTITY,
                                        ModelRef { model: terrain.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // Placement markers (`--placements`): one merged static entity (its
                                // marker verts are already world-space).
                                if let Some(pm) = data.placements {
                                    scene.load_model(pm.hash, &pm.verts, &pm.indices, &pm.draws, &pm.textures, &pm.skin);
                                    world.spawn((
                                        Transform::IDENTITY,
                                        ModelRef { model: pm.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // PMC-subset real geometry (`--placements`): one static entity per
                                // resolved model at its placement Transform (pos + yaw from quat).
                                for (m, pos, yaw) in data.pmc_models {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
                                    t.rotation = Quat::from_rotation_y(yaw);
                                    world.spawn((
                                        t,
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // Hi-res c3 cell geometry (`--cells`): static entities at their grid-cell origins.
                                // These building cells are structural — collect their world-space triangles for
                                // collision (walls the player and camera must not pass through).
                                for (m, off) in data.cells {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    let tr = Vec3::new(off[0], off[1], off[2]);
                                    for idx in m.indices.chunks_exact(3) {
                                        collision_tris.push([
                                            Vec3::from(m.verts[idx[0] as usize].pos) + tr,
                                            Vec3::from(m.verts[idx[1] as usize].pos) + tr,
                                            Vec3::from(m.verts[idx[2] as usize].pos) + tr,
                                        ]);
                                    }
                                    world.spawn((
                                        Transform::from_translation(tr),
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // PMC interior geometry (`--interior`): one static entity per keyed
                                // interior entity at its AUTHORED world Transform (pos + full quat,
                                // native game space, no offset — floor Y≈450). A model may be uploaded
                                // once and referenced by several instances; `load_model` is idempotent
                                // on the hash key so repeats are cheap.
                                for (m, pos, quat) in data.interior {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    let tr = Vec3::new(pos[0], pos[1], pos[2]);
                                    let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                                    let mut t = Transform::from_translation(tr);
                                    t.rotation = q;
                                    // Interior shells (hall/suites/garage/bays) are the walls — collect their
                                    // world-space triangles for collision. Rigged furniture rides the props
                                    // path (interior_props) and is intentionally left non-solid.
                                    for idx in m.indices.chunks_exact(3) {
                                        let w = |i: usize| q * Vec3::from(m.verts[i].pos) + tr;
                                        collision_tris.push([w(idx[0] as usize), w(idx[1] as usize), w(idx[2] as usize)]);
                                    }
                                    // Identity palette sized to the mesh's bone count — a rigged prop's
                                    // verts index several bones; a 1-bone palette collapses the rest to origin.
                                    let nbones = m.skin.bones.len().max(1);
                                    world.spawn((
                                        t,
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY; nbones] },
                                    ));
                                }

                                // ModelName props (`--props` exterior, `--interior` furniture): each
                                // distinct mesh is uploaded ONCE, then one static entity is spawned per
                                // placement instance (Transform pos + FULL quat, native game space).
                                let mut prop_meshes = 0usize;
                                let mut prop_instances = 0usize;
                                for (hash, m, instances) in data.props.into_iter().chain(data.interior_props) {
                                    scene.load_model(hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    prop_meshes += 1;
                                    let nbones = m.skin.bones.len().max(1);
                                    for (pos, quat) in instances {
                                        let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
                                        t.rotation = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                                        world.spawn((
                                            t,
                                            ModelRef { model: hash },
                                            AnimState::default(),
                                            SkinPalette { mats: vec![IDENTITY; nbones] },
                                        ));
                                        prop_instances += 1;
                                    }
                                }
                                if prop_meshes > 0 {
                                    println!("[world] props spawned: {prop_meshes} distinct meshes, {prop_instances} instances");
                                }

                                // Player avatar (optional): near map centre, feet snapped to the terrain heightmap.
                                if let Some(p) = data.player {
                                    has_run = p.clips.iter().any(|c| c.name_hash == CLIP_RUN);
                                    for c in &p.clips {
                                        if c.name_hash == CLIP_WALK {
                                            dur_walk = c.clip.duration.max(1e-3);
                                        } else if c.name_hash == CLIP_RUN {
                                            dur_run = c.clip.duration.max(1e-3);
                                        }
                                    }
                                    scene.load_model(p.hash, &p.verts, &p.indices, &p.draws, &p.textures, &p.skin);
                                    let rig = p.skin.rig.clone();
                                    let bind = if rig.is_empty() {
                                        vec![IDENTITY]
                                    } else {
                                        let m = pose::model_poses(&rig, &pose::bind_qs(&rig));
                                        pose::skin_palette(&rig, &m)
                                    };
                                    // Feet offset: origin-to-lowest-vertex, so the avatar stands ON the ground sample.
                                    let min_y = p.verts.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
                                    player_foot = if min_y.is_finite() { -min_y } else { 0.0 };
                                    println!("[world] player foot offset = {player_foot:.3} (model min Y {min_y:.3})");
                                    // Spawn uses the boot-log authored Y verbatim (no snap) for BOTH
                                    // modes; per-frame height-follow (exterior only) takes over on move.
                                    let playing = !p.clips.is_empty();
                                    store.borrow_mut().models.insert(p.hash, ModelAnim {
                                        rig,
                                        clips: p.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
                                    });
                                    let anim = if playing {
                                        AnimState::playing(CLIP_IDLE)
                                    } else {
                                        AnimState::default()
                                    };
                                    // Spawn facing +Z (into the open hall / toward the exit archway, matching the
                                    // retail spawn), with the third-person camera behind on the -Z side (tp_yaw = 0)
                                    // so the over-the-shoulder view opens behind the player's back.
                                    let mut t = Transform::from_translation(player_pos);
                                    t.rotation = Quat::from_rotation_y(0.0);
                                    player_entity = Some(world.spawn((
                                        t,
                                        ModelRef { model: p.hash },
                                        anim,
                                        SkinPalette { mats: bind },
                                    )));
                                }
                                hmap = Some(data.hmap);
                                println!("[world] collision: {} world-space triangles (buildings + interior shells)", collision_tris.len());
                                // Feed the harvested dynamic point lights to the renderer (nearest set
                                // uploaded per frame). Without this the villa/world has no local lighting.
                                scene.set_lights(std::mem::take(&mut data.lights));
                                // Environmental FX (real, always-on, data-driven — no flag):
                                //  * particle emitters (fire/smoke/steam) — classified by name → desc;
                                //  * static light-shaft glows (god rays) — additive glow cards, already
                                //    resolved from their effect template at load. Both are faithful; the
                                //    god rays are the era-appropriate additive card, not a skipped gap.
                                {
                                    let cards = std::mem::take(&mut data.glow_cards);
                                    let glows = cards.len();
                                    scene.set_glow_cards(&cards);
                                    let (mut started, mut skipped) = (0usize, 0usize);
                                    for (name, pos) in std::mem::take(&mut data.particle_fx) {
                                        match classify_particle(&name) {
                                            Some(desc) => { scene.fx_start_desc(desc, pos); started += 1; }
                                            None => skipped += 1,
                                        }
                                    }
                                    if started + skipped + glows > 0 {
                                        println!("[world] particle FX: {started} emitters + {glows} light-shaft glows started, {skipped} unsupported skipped");
                                    }
                                }
                                if start_tps && player_entity.is_some() {
                                    mode = CamMode::ThirdPerson;
                                }
                                loading = false;
                            }
                        }
                    }
                    let now = std::time::Instant::now();
                    let dt = (now - last).as_secs_f32().min(0.1);
                    last = now;
                    let look = 1.6 * dt;

                    // Drain the frame's mouse input from the active source onto the ACTIVE camera.
                    // Per-frame total is clamped so event storms can't slam the pitch to a rail.
                    // Sensitivity + InvertY come from Mercs2.ini [Mouse].
                    let sens = bindings.mouse_rad_per_px; // rad per px (from [Mouse] Sensitivity)
                    let inv_y = if bindings.invert_y { -1.0 } else { 1.0 };
                    let src = if mouse_src == 1 { mouse_raw_acc } else { mouse_acc };
                    let mdx = src.0.clamp(-80.0, 80.0) * sens;
                    let mdy = src.1.clamp(-80.0, 80.0) * sens * inv_y;
                    if src != (0.0, 0.0) && mouse_dbg_frames < 20 {
                        eprintln!("[mouse] src={} in=({:+.1},{:+.1}) applied=({:+.4},{:+.4})", mouse_src, src.0, src.1, mdx, mdy);
                        mouse_dbg_frames += 1;
                    }
                    mouse_acc = (0.0, 0.0);
                    mouse_raw_acc = (0.0, 0.0);
                    match mode {
                        CamMode::Free => {
                            free_yaw += mdx;
                            free_pitch = (free_pitch - mdy).clamp(-1.5, 1.5);
                        }
                        CamMode::ThirdPerson => {
                            tp_yaw += mdx;
                            tp_pitch = (tp_pitch - mdy).clamp(-1.2, 0.6);
                        }
                    }

                    gamepad.update();
                    let inp = mercs2_engine::input::Input { bindings: &bindings, keys: &held, mouse: &mouse_btns, gamepad: &gamepad };
                    // Gamepad right-stick look (analog) this frame, shared by both cameras.
                    let (gp_yaw, gp_pitch) = inp.look_delta(dt);
                    let mut view = match mode {
                        CamMode::Free => {
                            // Keyboard look: ini LookUp/Down/Left/Right (I/K/J/L) + arrows (kb-only, so the
                            // right stick isn't double-counted); plus the analog right-stick delta.
                            if held.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { free_pitch += look; }
                            if held.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { free_pitch -= look; }
                            if held.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { free_yaw -= look; }
                            if held.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { free_yaw += look; }
                            free_yaw += gp_yaw;
                            free_pitch = (free_pitch + gp_pitch).clamp(-1.5, 1.5);
                            let fwd = Vec3::new(free_pitch.cos() * free_yaw.sin(), free_pitch.sin(), free_pitch.cos() * free_yaw.cos()).normalize();
                            // Strafe right is negated (fwd×Y, not Y×fwd) to match the clip-space X flip
                            // (handedness fix in scene.rs) — otherwise A/D are swapped on the correct image.
                            let right = fwd.cross(Vec3::Y).normalize();
                            // Analog planar move (KB + left stick), then free-fly vertical on Jump/Crouch.
                            let (mx, my) = inp.move_vec();
                            let mut mv = fwd * my + right * mx;
                            if inp.held(Action::Jump) { mv += Vec3::Y; }     // fly up (ini Jump)
                            if inp.held(Action::Crouch) { mv -= Vec3::Y; }   // fly down (ini Crouch)
                            let sp = if inp.held(Action::Sprint) { 3200.0 } else { 800.0 };
                            if mv.length_squared() > 1e-6 { free_pos += mv.clamp_length_max(1.0) * sp * dt; }
                            Mat4::look_to_lh(free_pos, fwd, Vec3::Y)
                        }
                        CamMode::ThirdPerson => {
                            if held.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { tp_pitch += look; }
                            if held.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { tp_pitch -= look; }
                            if held.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { tp_yaw -= look; }
                            if held.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { tp_yaw += look; }
                            tp_yaw += gp_yaw;
                            tp_pitch = (tp_pitch + gp_pitch).clamp(-1.2, 0.6);
                            let fwd_flat = Vec3::new(tp_yaw.sin(), 0.0, tp_yaw.cos()).normalize();
                            // Negated (fwd×Y) to match the clip-space X flip (handedness fix), so D=right.
                            let right_flat = fwd_flat.cross(Vec3::Y).normalize();
                            // Analog planar move (KB WASD + left stick).
                            let (mx, my) = inp.move_vec();
                            let mv = fwd_flat * my + right_flat * mx;
                            // Speed ramp: ease the ground speed toward the walk/run target (or 0)
                            // so starts, stops and gait changes aren't instant. Sprint = ini Sprint (LSHIFT).
                            let target_sp = if mv != Vec3::ZERO {
                                if inp.held(Action::Sprint) { RUN_SPEED } else { WALK_SPEED }
                            } else {
                                0.0
                            };
                            let rate = if target_sp > player_speed { ACCEL } else { DECEL };
                            player_speed += (target_sp - player_speed).clamp(-rate * dt, rate * dt);
                            if mv != Vec3::ZERO {
                                player_move_dir = mv.normalize();
                            }
                            let moving = player_speed > 1e-3;
                            if moving {
                                let horiz = player_move_dir * player_speed * dt;
                                if !collision_tris.is_empty() {
                                    // Capsule character controller: collide-and-slide against walls +
                                    // (interior) a downward ground probe so stairs/steps/ramps work and
                                    // the capsule rests AGAINST walls instead of penetrating then bouncing.
                                    // Mirrors the engine's Havok capsule (MatchCapsuleToPose). The exterior
                                    // still gets Y from the terrain heightmap below (follow_ground = false).
                                    const PLAYER_RADIUS: f32 = 0.35;
                                    const PLAYER_HEIGHT: f32 = 1.8;
                                    const STEP: f32 = 0.5;
                                    player_pos = crate::collision::move_character(
                                        &collision_tris, player_pos, horiz,
                                        PLAYER_RADIUS, PLAYER_HEIGHT, STEP, spawn_interior,
                                    );
                                } else {
                                    player_pos += horiz;
                                }
                            }
                            // Ground snap: feet follow the terrain heightmap. Hinted by the
                            // current ground Y so overhangs don't teleport the player up. Skipped
                            // for `--interior` (its floor is at Y≈450, off the terrain).
                            if !spawn_interior {
                                if let Some(hm) = &hmap {
                                    player_pos.y = hm
                                        .height_at_near(player_pos.x, player_pos.z, player_pos.y - player_foot)
                                        + player_foot;
                                }
                            }
                            if let Some(e) = player_entity {
                                if let Ok(mut t) = world.get::<&mut Transform>(e) {
                                    t.translation = player_pos;
                                    if moving {
                                        // Smooth turning: exponential yaw damp toward the move
                                        // direction, shortest arc.
                                        let target = player_move_dir.x.atan2(player_move_dir.z);
                                        let d = (target - player_yaw + PI).rem_euclid(2.0 * PI) - PI;
                                        player_yaw += d * (1.0 - (-TURN_RATE * dt).exp());
                                        t.rotation = Quat::from_rotation_y(player_yaw);
                                    }
                                }
                                // Run under Shift, walk while moving, idle otherwise. A switch
                                // crossfades from the old clip; walk<->run carries the normalized
                                // cycle phase so the feet stay in step (idle restarts at 0).
                                if let Ok(mut a) = world.get::<&mut AnimState>(e) {
                                    let want = if mv != Vec3::ZERO {
                                        if inp.held(Action::Sprint) && has_run { CLIP_RUN } else { CLIP_WALK }
                                    } else {
                                        CLIP_IDLE
                                    };
                                    if a.clip != want {
                                        a.prev_clip = a.clip;
                                        a.prev_time = a.time;
                                        a.blend = 0.0;
                                        a.time = if a.clip == CLIP_WALK && want == CLIP_RUN {
                                            a.time / dur_walk * dur_run
                                        } else if a.clip == CLIP_RUN && want == CLIP_WALK {
                                            a.time / dur_run * dur_walk
                                        } else {
                                            0.0
                                        };
                                        a.clip = want;
                                    }
                                    // Foot-slide reduction: playback rate tracks the eased speed.
                                    a.speed = if FOOT_SYNC && want != CLIP_IDLE && target_sp > 0.0 {
                                        (player_speed / target_sp).clamp(0.8, 1.2)
                                    } else {
                                        1.0
                                    };
                                }
                            }
                            let dir = Vec3::new(tp_pitch.cos() * tp_yaw.sin(), tp_pitch.sin(), tp_pitch.cos() * tp_yaw.cos()).normalize();
                            let focus = player_pos + Vec3::Y * 2.2;
                            let right = Vec3::Y.cross(dir).normalize();
                            // Desired over-the-shoulder eye 6 m back + 1.2 m to the side of the focus.
                            const BOOM: f32 = 6.0;
                            let want_eye = focus - dir * BOOM + right * 1.2;
                            // Boom collision: cast from the focus toward the desired eye and pull the eye in
                            // to just short of the nearest wall (CAM_RADIUS margin = the engine's camera
                            // collision radius²). Without this the boom clips straight through geometry.
                            let eye = if collision_tris.is_empty() {
                                want_eye
                            } else {
                                const CAM_RADIUS: f32 = 0.35;
                                let boom_vec = want_eye - focus;
                                let boom_len = boom_vec.length();
                                let boom_dir = boom_vec / boom_len;
                                match crate::collision::raycast(&collision_tris, focus, boom_dir, boom_len) {
                                    Some(hit) => focus + boom_dir * (hit - CAM_RADIUS).max(0.6),
                                    None => want_eye,
                                }
                            };
                            Mat4::look_to_lh(eye, (focus - eye).normalize(), Vec3::Y)
                        }
                    };

                    // Interior debug orbit (`--interior-orbit`): override the camera each frame with an
                    // elevated auto-orbit CENTERED on the interior anchor (3794,470,-3911), radius ~120 m,
                    // height ~+70, so the whole assembled room + player are framed from outside. The TPS
                    // sim above still runs (player movement/anim); only the view matrix is replaced.
                    if interior_orbit {
                        const ANCHOR: Vec3 = Vec3::new(3779.8, 454.7, -3879.6);
                        const RADIUS: f32 = 38.0;
                        const HEIGHT: f32 = 52.0;
                        let ang = load_start.elapsed().as_secs_f32() * 0.25; // ~24 s per revolution
                        let eye = ANCHOR + Vec3::new(RADIUS * ang.sin(), HEIGHT, RADIUS * ang.cos());
                        view = Mat4::look_at_lh(eye, ANCHOR, Vec3::Y);
                    }

                    schedule.run_fixed(&mut world, &mut time, dt);
                    scene.set_view(view, if interior_orbit { 1.0 } else { 0.5 }, 30000.0);
                    match scene.render(&world) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                        Err(e) => eprintln!("surface error: {e:?}"),
                    }
                }
                _ => {}
            },
            // Raw deltas: the normal game input path. Feeds the accumulator only while sane;
            // a single absurd event (absolute-coordinate stream, e.g. Shadow cloud PC) latches
            // the cursor fallback for the rest of the session.
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                let (dx, dy) = (delta.0 as f32, delta.1 as f32);
                if mouse_src != 2 {
                    if dx.abs() > 2000.0 || dy.abs() > 2000.0 {
                        mouse_src = 2; // absolute-coordinate stream detected -> cursor path
                        eprintln!("[mouse] absolute-coordinate raw input detected -> cursor-recentre mode");
                    } else {
                        mouse_raw_acc.0 += dx;
                        mouse_raw_acc.1 += dy;
                        if mouse_src == 0 && (dx != 0.0 || dy != 0.0) {
                            mouse_sane_events += 1;
                            if mouse_sane_events >= 10 {
                                mouse_src = 1; // healthy relative deltas -> raw path
                            }
                        }
                    }
                }
            }
            Event::AboutToWait => scene.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}


/// The control-driven streaming world with a free-fly camera (the no-arg default boot; also
/// `--stream`). Mirrors the original engine's ONE streaming system (spec §10): a background loader
/// builds the block index + Layer-2 decision catalog, then each frame the pure `StreamingManager`
/// turns the camera position into a load/unload/wake/hibernate diff, and this executor performs the
/// GPU work — LOAD c3-cell geometry + WAKE `ModelName` props (via the proven recipes), and the
/// net-new UNLOAD path (despawn + free GPU). Free-fly camera reuses the Shadow-PC dual-source mouse
/// input (CursorMoved+recentre fallback, never DeviceEvent on absolute-coordinate streams).
/// Parse `--spawn=X,Y,Z` (comma-separated world coords) into an initial free-fly camera position.
/// `mercs2_game` passes the authentic PMC-interior start; absent = the default exterior bird's-eye.

fn load_from_wad(
    wadpath: &str,
    model: Option<String>,
    index: Option<String>,
    animate: bool,
    clip_hash: Option<u32>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>, TexMap, mesh::SkinData, Option<ClipAnim>, u32, String), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    if models.is_empty() {
        return Err("no model assets in WAD".into());
    }
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models
            .get(n)
            .map(|&(h, _)| h)
            .ok_or_else(|| format!("--index {n} out of range (0..{})", models.len()))?
    } else {
        models[0].0
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, indices, draws, s) = mesh::build_indexed_from_container(&container)?;

    // Extract each unique diffuse + normal-map texture (DXT/BC bytes) for the placed groups.
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                match wad::extract_texture(&mut w, h) {
                    Ok(t) => {
                        textures.insert(h, t);
                    }
                    Err(e) => eprintln!("  texture 0x{h:08X} unavailable: {e}"),
                }
            }
        }
    }

    let ntris = indices.len() / 3;
    println!(
        "loaded model 0x{hash:08X}: {} verts / {ntris} tris / {} groups / {} textures ({} accessory groups skipped)",
        s.vertices, s.meshes, textures.len(), s.skipped
    );

    // Animation: bind the best-matching clip to this model's HIER (only when requested).
    let clip = if animate && !s.rig.is_empty() {
        let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
        match load_clip_for_rig(&mut w, &hier, clip_hash) {
            Some(ca) => {
                let resolved = ca.track_to_hier.iter().filter(|r| r.is_some()).count();
                println!(
                    "animation: clip 0x{:08X} ({} tracks, {} frames, {:.2}s), {resolved} tracks -> HIER bones",
                    ca.name_hash, ca.clip.num_tracks, ca.clip.num_frames, ca.clip.duration
                );
                Some(ca)
            }
            None => {
                eprintln!("animation: no decodable clip bound to this model — using synthetic driver");
                None
            }
        }
    } else {
        None
    };

    let title = format!("Mercs 2 — model 0x{hash:08X} ({ntris} tris)");
    Ok((verts, indices, draws, textures, s.skin_data(), clip, hash, title))
}

