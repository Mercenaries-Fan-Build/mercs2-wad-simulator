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
/// Resolve a character's primary idle clip through the resident AnimationLookup
/// (data-driven: `ActionTable → Handle → AnimationLookup[CharacterName] → ASTO → clip`,
/// see `docs/modernization/human_animation_selection.md`). Tries the base resident
/// block first, then any `resident`-named block; `None` if the tables aren't present.
fn resolve_player_idle(w: &mut wad::Wad, character: u32) -> Option<u32> {
    use mercs2_formats::anim_select::AnimSelector;
    if let Ok(dec) = wad::decompress_block_index(w, 3185) {
        if let Some(c) = AnimSelector::from_resident_block(&dec).and_then(|s| s.primary_idle(character)) {
            return Some(c);
        }
    }
    // Collect resident-named block indices first (ends the block_paths borrow before we
    // take `w` mutably to decompress).
    let resident: Vec<usize> = {
        let paths = wad::block_paths(w);
        paths
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != 3185 && p.to_ascii_lowercase().contains("resident"))
            .map(|(i, _)| i)
            .collect()
    };
    for i in resident {
        if let Ok(dec) = wad::decompress_block_index(w, i as u16) {
            if let Some(c) = AnimSelector::from_resident_block(&dec).and_then(|s| s.primary_idle(character)) {
                return Some(c);
            }
        }
    }
    None
}

fn load_world_data(
    wadpath: &str,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    recruits: crate::pmc::RecruitUnlocks,
    stockpile: &crate::pmc::Stockpile,
    player_models: &[String],
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

    // Player avatar for the third-person view, at RAW model scale (identity fit) so it sits in
    // world metres alongside the terrain rather than fit-normalised. The MODEL comes from the
    // SAVE (hero + wardrobe outfit, `crate::hero::player_model_candidates`) — candidates are
    // tried in order (saved outfit → hero Original → proven-good fallback).
    // NOTE: world scale and facing are first-pass and not yet calibrated.
    // animate=false: skip load_from_wad's own animgroup scan — all three clips (idle/walk/run)
    // come from ONE cached scan below instead of three full-archive passes (~20 s -> ~7 s load).
    //
    // Per-character idle, DATA-DRIVEN (was hardcoded to Jennifer's `0x24F8C8E6` for everyone —
    // the hip-swing that warped Mattias). The merc identity is the hero's base model, always in
    // the candidate list (`pmc_hum_{mattias,chris,jen}`). Resolve their real idle through the
    // resident AnimationLookup; fall back to the validated per-merc hash, then the old constant.
    let merc = if player_models.iter().any(|m| m.contains("_jen")) {
        "jennifer"
    } else if player_models.iter().any(|m| m.contains("_chris")) {
        "chris"
    } else {
        "mattias"
    };
    let character = mercs2_formats::anim_select::AnimSelector::character_name(merc);
    let idle_clip = resolve_player_idle(&mut w, character)
        .or_else(|| mercs2_formats::anim_select::fallback_idle(character))
        .unwrap_or(0x24F8_C8E6);
    println!("[world] player merc '{merc}' (CharacterName 0x{character:08X}) → idle clip 0x{idle_clip:08X}");

    let mut player_loaded = None;
    for name in player_models {
        let hash = name
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name));
        match load_from_wad(wadpath, Some(format!("0x{hash:08X}")), None, false, None) {
            Ok(ok) => {
                println!("[world] player model: {name} (0x{hash:08X})");
                player_loaded = Some(ok);
                break;
            }
            Err(e) => eprintln!("[world] player model {name} (0x{hash:08X}) failed ({e}); trying next"),
        }
    }
    let player = match player_loaded.ok_or_else(|| "no player-model candidate built".to_string()) {
        Ok((v, i, d, t, mut s, _c, h, _)) => {
            progress.step("player");
            s.center = [0.0, 0.0, 0.0];
            s.scale = 1.0;
            let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
            let wanted = [idle_clip, 0x5368_2784, 0x867B_166D]; // idle (per-merc, data-driven), walk, run
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
            eprintln!("[world] player avatar load failed: {e}");
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
                // Named interior lights (`Light_small_<hue>[_dim]`) carry no LightObject COMP — their
                // colour/brightness is the NAME convention. Turn each into a coloured point light.
                if let Some(l) = interior_named_light(name, p.pos) {
                    lights.push(l);
                    continue;
                }
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

/// A named interior light placement (`Light_small_<hue>[_dim]`) → a coloured point light. These have
/// no `LightObject` COMP; the game reads the light from the NAME. Colour = hue token, brightness =
/// `dim`, range = `small`. Returns `None` for non-light names. Values are first-pass tunables.
fn interior_named_light(name: &str, pos: [f32; 3]) -> Option<mercs2_engine::render::GpuLight> {
    let n = name.to_ascii_lowercase();
    // Two kinds of interior light: authored `Light_<hue>` markers, and lamp/stage-light PROPS that are
    // also physical meshes (the floating military lamp heads) — both illuminate the room.
    let is_lamp = n.contains("lamppost")
        || n.contains("portablelight")
        || n.contains("spotlight")
        || n.contains("stagelight");
    if !n.starts_with("light_") && !is_lamp {
        return None;
    }
    let color = if n.contains("darkblue") {
        [0.18, 0.30, 0.85]
    } else if n.contains("blue") {
        [0.35, 0.55, 1.0]
    } else if n.contains("yellow") {
        [1.0, 0.82, 0.42]
    } else if n.contains("red") {
        [1.0, 0.32, 0.22]
    } else if n.contains("green") {
        [0.40, 1.0, 0.45]
    } else {
        [1.0, 0.95, 0.85] // warm white (lamps + default)
    };
    // Intensity multiplies the surface albedo, so these stay < 1 (accent) to not blow out the baked
    // room. Lamp/stage-light PROPS are the room's REAL sources → brighter + wider than the small
    // authored markers. All tunable.
    let (intensity, radius) = if is_lamp {
        (0.60, 12.0)
    } else if n.contains("dim") {
        (0.20, 7.0)
    } else if n.contains("small") {
        (0.45, 7.0)
    } else {
        (0.45, 12.0)
    };
    Some(mercs2_engine::render::GpuLight::point(pos, color, intensity, radius))
}

/// Classify a `global_particle_*` effect name → a billboard [`EmitterDesc`] for the particle sim.
/// Static light-shaft FX are handled separately (see [`is_light_shaft_fx`] / glow cards) and never
/// reach here. Name-heuristic mapping until the `EffectTemplate → EmitterDesc` decode is pinned.
/// Sample the render terrain's `HeightMap` onto a regular grid → a `mercs2_physics::Heightmap` the
/// fleet physics can raycast (K2 S3). The terrain extent is the engine's ±4000 m world square; a
/// 257×257 grid (~31 m cells) bilinearly interpolates smoothly enough for vehicle ground contact. The
/// render HeightMap keeps the exact triangles for the player walk; this is the physics-side heightfield.
fn heightmap_to_physics(hm: &mercs2_engine::worldutil::HeightMap) -> mercs2_physics::Heightmap {
    const W: usize = 257;
    const MIN: f32 = -4000.0;
    const MAX: f32 = 4000.0;
    let cell = (MAX - MIN) / (W as f32 - 1.0);
    let mut heights = Vec::with_capacity(W * W);
    for iz in 0..W {
        let z = MIN + iz as f32 * cell;
        for ix in 0..W {
            let x = MIN + ix as f32 * cell;
            heights.push(hm.height_at(x, z));
        }
    }
    mercs2_physics::Heightmap::new(MIN, MIN, cell, W, W, heights)
}

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
///
/// `menu`: `Some` = open on the SHELL MENU (main menu + save browser, `crate::menu`) and only
/// start the world load when the player picks a save / new game — the retail boot flow. The
/// `recruits`/`stockpile` arguments are then the NEW-GAME defaults, overridden by the selected
/// save. `None` = boot straight into the load (explicit `.profile` CLI arg / dev flows).
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
    player_models: Vec<String>,
    menu: Option<crate::menu::Menu>,
) {
    use mercs2_engine::scene::{AssetStore, ModelAnim, Scene};
    use mercs2_core::frame::{LayerStack, LayerTransition, LAYER_GAME};
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
    // Locomotion clip hashes + feel tunables now live on `crate::player::PlayerController`
    // (extracted for unit testing). Only the schedule's animation crossfade duration is used here.
    const ANIM_BLEND_SEC: f32 = 0.25; // crossfade duration on clip switches

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Mercenaries 2 — world (Tab: free / third-person)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );
    // Mouse-look: grab + hide the cursor on the world window (Confined preferred, Locked
    // fallback). Arrow keys stay as a fallback steer; Esc still exits. While the shell menu is
    // up the cursor stays free/visible — the grab happens when the world boot starts.
    let grab_cursor = |window: &winit::window::Window| {
        if let Err(e) = window
            .set_cursor_grab(CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
        {
            eprintln!("[world] cursor grab unavailable ({e}); arrow keys still steer");
        }
        window.set_cursor_visible(false);
    };
    if menu.is_none() {
        grab_cursor(&window);
    }
    let mut scene = Scene::new(window.clone()).await;
    // Placeholder distance fog + sky (stand-in for PgSky/PgSun/PgCloud). Tunables: warm-haze
    // color, density 0.00016 (~30% haze at 2.5 km, ~50% at 4.5 km — depth cue at ground level
    // without white-out from the aerial free cam; 0.00035 washed out the whole map), start 60 m.
    // Fog + sun differ indoors vs out. INTERIOR: no outdoor sun (windowless — baked lighting + interior
    // point lights + ambient) and a DARK neutral-gray fog so distance recedes into shadow (a vignette
    // feel), denser than the exterior since interior depths are metres not kilometres, start ~2 m.
    // EXTERIOR: directional key light + the thin aerial haze. All tunable.
    if spawn_interior {
        scene.set_fog([0.16, 0.17, 0.18], 0.0075, 2.0);
        scene.set_sun(0.0, 0.30);
    } else {
        scene.set_fog([0.55, 0.62, 0.70], 0.00016, 60.0);
        scene.set_sun(0.9, 0.35);
    }
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

    // Persistent mission-Lua host (keystone K1): resident across the whole loop — NOT the one-shot
    // interior-boot host that is dropped after harvesting spawns. Each fixed step the loop pumps its
    // Lua event/timer system (`Event.__pump`) and drains the runtime `Pg.Spawn`s it records into the
    // ECS via the resolver. Its `AudioEngine` is SHARED with the fleet below, so the Lua `Sound.*` cues
    // and `GameplaySystems::tick`→`audio.tick` drive one engine (fixes the split-brain audio seam).
    let script_host = Rc::new(RefCell::new(crate::script_host::GameScriptHost::new("vz")));
    let script = crate::script_host::resident_script_host(script_host.clone());
    if script.is_some() {
        println!("[world] persistent mission-Lua host resident (Event.__pump + runtime Pg.Spawn live)");
    }

    // Per-frame game update: the fleet gameplay systems (physics/vehicle/combat/audio) + the
    // template→entity spawn resolver, bundled in crate::runtime::GameRuntime. Idle until entities
    // carry their components; the audio engine mixes/advances from frame 1. Audio is the host's engine
    // so scripted cues are audible.
    let audio = script_host.borrow().audio();
    let mut runtime = crate::runtime::GameRuntime::new(audio.clone());

    // Background loader: all WAD/terrain/player parsing happens off the render thread; the
    // result lands on this channel and is wired into the scene/world on arrival. With the shell
    // menu up, the spawn is DEFERRED until the player picks a save (the retail flow); direct
    // boots spawn it immediately as before.
    let (tx, rx) = std::sync::mpsc::channel::<Result<WorldData, String>>();
    let progress = Arc::new(LoadProgress::new(LOAD_STAGES));
    let spawn_loader = {
        let progress = progress.clone();
        let wadpath = wadpath.clone();
        move |recruits: crate::pmc::RecruitUnlocks, stockpile: crate::pmc::Stockpile, player_models: Vec<String>| {
            let tx = tx.clone();
            let progress = progress.clone();
            let wadpath = wadpath.clone();
            std::thread::spawn(move || {
                let t0 = std::time::Instant::now();
                let r = load_world_data(&wadpath, load_cells, load_placements, spawn_interior, load_props, recruits, &stockpile, &player_models, &progress);
                if r.is_ok() {
                    println!("[load] done in {:.1}s", t0.elapsed().as_secs_f64());
                }
                let _ = tx.send(r);
            });
        }
    };
    let mut menu = menu;
    if menu.is_none() {
        spawn_loader(recruits.clone(), stockpile.clone(), player_models.clone());
    }

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
    let spawn_pos = if spawn_interior {
        println!("[world] --interior: spawning at PMC interior teleport coord (3794.043, 450.751, -3911.032) [interior placed at authored transforms; height-follow OFF]");
        Vec3::new(PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2])
    } else {
        Vec3::new(2560.2646, -13.1779, -926.2511)
    };
    // Third-person player locomotion — the extracted, unit-tested `crate::player::PlayerController`.
    // Its foot offset / clip durations / ground speeds (baked-root-stride derived) / idle / entity are
    // filled when the avatar loads (below); WALK_SPEED/RUN_SPEED are the fallbacks.
    let mut player = crate::player::PlayerController::new(spawn_pos);

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
    // Keystone C — the master frame spine (docs/reverse_engineer/scheduler_tick_code_map.md). The
    // shell-menu / loading / in-game phases are the engine's lower→upper application layers; the
    // LayerStack climbs frontend → loading → GAME (`FUN_004c15e0`, 0→4), replacing the old
    // `menu.is_some()` + `loading` bool phase gates. A direct boot (no shell menu) starts already on
    // the loading layer; the shell-menu boot starts on the frontend layer and climbs on selection.
    const LAYER_MENU: usize = LAYER_GAME - 2;
    const LAYER_LOADING: usize = LAYER_GAME - 1;
    let mut layers = if menu.is_some() {
        LayerStack::at(LAYER_MENU)
    } else {
        LayerStack::at(LAYER_LOADING)
    };
    let mut load_start = std::time::Instant::now();
    // Shell-menu bookkeeping: a selection made in a keyboard/gamepad handler is parked here and
    // executed at the top of the next redraw (ONE boot site). Gamepad nav is edge-detected
    // against the previous frame's held-state.
    let mut pending_boot: Option<Option<std::path::PathBuf>> = None;
    let mut menu_gp_prev = [false; 4]; // up, down, select, back
    let menu_open = std::time::Instant::now();
    // Ignore menu activations for a short arm delay after opening: the keystroke that launched
    // the exe from a terminal (locally or Shadow-streamed) can land on the freshly-focused window
    // as a REAL keydown (winit's `is_synthetic` does not flag it) and would instantly select
    // "Continue". Observed on the Shadow PC; retail shells latch input the same way.
    const MENU_ARM_DELAY: f32 = 0.4;
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
                    is_synthetic,
                    ..
                } => {
                    // Shell menu owns the keyboard while it is up (retail shell state machine).
                    // Synthetic presses (keys already down when the window gains focus — e.g. the
                    // terminal Enter that launched the exe) must not activate a menu row.
                    if let Some(m) = menu.as_mut() {
                        if state == ElementState::Pressed
                            && !is_synthetic
                            && menu_open.elapsed().as_secs_f32() > MENU_ARM_DELAY
                        {
                            let nav = match code {
                                KeyCode::ArrowUp | KeyCode::KeyW => Some(crate::menu::Nav::Up),
                                KeyCode::ArrowDown | KeyCode::KeyS => Some(crate::menu::Nav::Down),
                                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                                    Some(crate::menu::Nav::Select)
                                }
                                KeyCode::Escape | KeyCode::Backspace => Some(crate::menu::Nav::Back),
                                _ => None,
                            };
                            if let Some(nav) = nav {
                                match m.nav(nav) {
                                    crate::menu::MenuAction::Boot(sel) => pending_boot = Some(sel),
                                    crate::menu::MenuAction::Quit => elwt.exit(),
                                    crate::menu::MenuAction::None => {}
                                }
                            }
                        }
                        return;
                    }
                    match (code, state) {
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
                    }
                }
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
                    // Shell menu: the cursor is free/visible — no look-accumulate, no recentre.
                    if menu.is_some() {
                        return;
                    }
                    let (cx, cy) = (scene.size.width as f64 / 2.0, scene.size.height as f64 / 2.0);
                    mouse_acc.0 += (position.x - cx) as f32;
                    mouse_acc.1 += (position.y - cy) as f32;
                    let _ = scene
                        .window
                        .set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                }
                WindowEvent::RedrawRequested => {
                    // ===== RunFrame (FUN_00630ef0) — faithful 9-stage per-frame order =====
                    // (docs/reverse_engineer/scheduler_tick_code_map.md §2). Platform-glue stages fold
                    // into wgpu/winit: (2) device re-init = render()'s surface-lost recovery; (8)/(9)
                    // vsync/present = wgpu's present mode + winit's AboutToWait redraw request.

                    // (5a) MASTER UPDATE — mode logic for the active application layer. Shell menu
                    //      (frontend) → loading → GAME are the engine's climbing application layers.
                    if layers.active() == LAYER_MENU {
                        // Execute a parked selection: resolve the save → boot config, start the world
                        // load, and raise the target to the loading layer (the climb below grabs the
                        // cursor, resets the loading clock, and drops the menu). Otherwise draw the
                        // menu and stay on the frontend layer for this frame.
                        if let Some(sel) = pending_boot.take() {
                            let (r, sp, models, label) = boot_config_from(sel.as_deref());
                            println!("[shell] boot: {label}");
                            spawn_loader(r, sp, models);
                            layers.set_target(LAYER_LOADING);
                        } else {
                            let m = menu.as_mut().unwrap();
                            // Gamepad nav, edge-detected: dpad/stick up/down, A/Start = select,
                            // B = back (ini [Controller] Up/Down/Jump/Start/Crouch bindings).
                            gamepad.update();
                            let inp = mercs2_engine::input::Input {
                                bindings: &bindings, keys: &held, mouse: &mouse_btns, gamepad: &gamepad,
                            };
                            let (_, my) = inp.move_vec();
                            let now = [
                                inp.held(Action::SelectUp) || my > 0.5,
                                inp.held(Action::SelectDown) || my < -0.5,
                                inp.held(Action::Jump) || inp.held(Action::Start),
                                inp.held(Action::Crouch),
                            ];
                            let navs = [
                                crate::menu::Nav::Up,
                                crate::menu::Nav::Down,
                                crate::menu::Nav::Select,
                                crate::menu::Nav::Back,
                            ];
                            let armed = menu_open.elapsed().as_secs_f32() > MENU_ARM_DELAY;
                            for i in 0..4 {
                                if armed && now[i] && !menu_gp_prev[i] {
                                    match m.nav(navs[i]) {
                                        crate::menu::MenuAction::Boot(sel) => pending_boot = Some(sel),
                                        crate::menu::MenuAction::Quit => elwt.exit(),
                                        crate::menu::MenuAction::None => {}
                                    }
                                }
                            }
                            menu_gp_prev = now;
                            let t = menu_open.elapsed().as_secs_f32();
                            m.draw(&mut scene, t);
                            match scene.render_menu(t) {
                                Ok(()) => {}
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("surface error: {e:?}"),
                            }
                            return;
                        }
                    }
                    // (5a cont.) Loading layer: poll the background loader; when it delivers, wire the
                    // world in (GPU uploads + entity spawns) and raise the target to the GAME layer.
                    // The Empty case falls through to the shared frontend render below.
                    if layers.active() == LAYER_LOADING {
                        match rx.try_recv() {
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
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
                                    player.has_run = p.clips.iter().any(|c| c.name_hash == crate::player::CLIP_RUN);
                                    // The idle clip = the loaded player clip that isn't walk/run
                                    // (idle was resolved per-merc in load_world_data).
                                    player.idle = p.clips.iter().map(|c| c.name_hash)
                                        .find(|h| *h != crate::player::CLIP_WALK && *h != crate::player::CLIP_RUN)
                                        .unwrap_or(crate::player::CLIP_IDLE);
                                    for c in &p.clips {
                                        let d = c.clip.duration.max(1e-3);
                                        // Authentic ground speed = the clip's baked root stride / duration.
                                        let sp = pose::clip_root_speed(
                                            &p.skin.rig,
                                            &c.clip.sample_local(0.0),
                                            &c.clip.sample_local(d * 0.999),
                                            &c.track_to_hier,
                                            c.num_transform_tracks,
                                            d * 0.999,
                                        );
                                        if c.name_hash == crate::player::CLIP_WALK {
                                            player.dur_walk = d;
                                            if sp > 0.1 { player.walk_speed = sp; }
                                            println!("[world] walk clip stride -> {sp:.2} m/s (fallback {})", crate::player::WALK_SPEED);
                                        } else if c.name_hash == crate::player::CLIP_RUN {
                                            player.dur_run = d;
                                            if sp > 0.1 { player.run_speed = sp; }
                                            println!("[world] run clip stride -> {sp:.2} m/s (fallback {})", crate::player::RUN_SPEED);
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
                                    player.foot = if min_y.is_finite() { -min_y } else { 0.0 };
                                    println!("[world] player foot offset = {:.3} (model min Y {min_y:.3})", player.foot);
                                    // Spawn uses the boot-log authored Y verbatim (no snap) for BOTH
                                    // modes; per-frame height-follow (exterior only) takes over on move.
                                    let playing = !p.clips.is_empty();
                                    store.borrow_mut().models.insert(p.hash, ModelAnim {
                                        rig,
                                        clips: p.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
                                    });
                                    let anim = if playing {
                                        AnimState::playing(player.idle)
                                    } else {
                                        AnimState::default()
                                    };
                                    // Spawn facing +Z (into the open hall / toward the exit archway, matching the
                                    // retail spawn), with the third-person camera behind on the -Z side (tp_yaw = 0)
                                    // so the over-the-shoulder view opens behind the player's back.
                                    let mut t = Transform::from_translation(player.pos);
                                    t.rotation = Quat::from_rotation_y(0.0);
                                    player.entity = Some(world.spawn((
                                        t,
                                        ModelRef { model: p.hash },
                                        anim,
                                        SkinPalette { mats: bind },
                                    )));
                                }
                                hmap = Some(data.hmap);
                                println!("[world] collision: {} world-space triangles (buildings + interior shells)", collision_tris.len());
                                // Hand the streamed collision soup to the fleet physics world so the
                                // vehicle/weapon systems can raycast against it via PhysicsQuery.
                                runtime.set_collision(collision_tris.clone());
                                // K2 S3: hand the terrain heightfield to the fleet physics too, so ground
                                // raycasts resolve over open terrain (not just where a building cell
                                // supplies triangles) — vehicles no longer fall through open ground.
                                if let Some(hm) = hmap.as_ref() {
                                    let phm = heightmap_to_physics(hm);
                                    println!("[world] terrain heightmap -> fleet physics ({}x{} grid)", phm.width, phm.depth);
                                    runtime.set_heightmap(Some(phm));
                                }
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
                                if start_tps && player.entity.is_some() {
                                    mode = CamMode::ThirdPerson;
                                }
                                layers.set_target(LAYER_GAME);
                            }
                        }
                    }

                    // (5b) climb the layer stack toward its target, firing enter transitions. The one
                    //      transition with side effects is entering the loading layer FROM the shell
                    //      menu (grab the cursor, reset the loading clock, drop the frontend menu).
                    while !layers.settled() {
                        if let Some(LayerTransition::Ascending(LAYER_LOADING)) = layers.advance() {
                            grab_cursor(&scene.window);
                            load_start = std::time::Instant::now();
                            menu = None;
                        }
                    }

                    // (6-frontend) Not in-game yet — the menu already returned above, so this is the
                    //      loading layer: ease + render the loading plate/spinner, then stop this frame.
                    if layers.active() != LAYER_GAME {
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

                    // ===== (5c) GAME layer (4) — camera + player sim + fixed-tick Schedule + render.
                    // Camera + player controller are variable-rate; the `animation` system runs at the
                    // fixed sim tick inside `schedule.run_fixed` (the shared mercs2_core `Time` clock).
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
                            // Player locomotion (eased speed + collide-and-slide + terrain ground-snap +
                            // walk/run/idle clip FSM) is the extracted, unit-tested PlayerController.
                            player.update(
                                &mut world,
                                mv,
                                inp.held(Action::Sprint),
                                &collision_tris,
                                hmap.as_ref(),
                                spawn_interior,
                                dt,
                            );
                            // Over-the-shoulder framing + boom-collision — the extracted, unit-tested
                            // crate::camera::third_person_view (pure geometry of player pos + look angles).
                            crate::camera::third_person_view(player.pos, tp_yaw, tp_pitch, &collision_tris)
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

                    let steps = schedule.run_fixed(&mut world, &mut time, dt);
                    // Tick the fleet gameplay systems (vehicle/combat/physics/audio) at the SAME fixed
                    // cadence the animation schedule just ran — the layer-4 gameplay tick, now driven.
                    for _ in 0..steps {
                        runtime.tick(&mut world, time.fixed_dt);
                        // Population update uses the player as the camera/death-distance anchor; its
                        // spawn requests are realized through the shared resolver. Idle until spawners
                        // are registered (the living-world data path is a later wire).
                        runtime.tick_population(&mut world, time.fixed_dt, player.pos);
                        // Persistent mission-Lua (K1): advance the Lua event/timer system, then realize
                        // any runtime Pg.Spawns the script recorded this step through the same resolver.
                        // The render layer attaches visuals to the returned entities (model resolution is
                        // the render seam; bare-transform actors until a template→model map lands).
                        if let Some(sh) = &script {
                            crate::script_host::pump_resident(sh, time.fixed_dt);
                            let new_spawns = script_host.borrow_mut().take_new_spawns();
                            if !new_spawns.is_empty() {
                                let realized = runtime.realize_spawns(&mut world, &new_spawns);
                                println!("[world] realized {} runtime spawn(s) from mission Lua", realized.len());
                            }
                        }
                    }
                    // Combat impact FX: each resolved hit (decal already spawned in the runtime) also
                    // emits a particle burst — explosion → fireball, bullet → dust puff. Blood is
                    // decal-only (no particle desc). The FX sink lives on the Scene, so it's drained
                    // here rather than in the GPU-free runtime bundle.
                    for imp in runtime.take_render_impacts() {
                        let desc = match imp.kind {
                            mercs2_combat::ImpactKind::Explosion => {
                                Some(mercs2_engine::particles::EmitterDesc::demo_fire())
                            }
                            mercs2_combat::ImpactKind::Bullet => {
                                Some(mercs2_engine::particles::EmitterDesc::demo_smoke())
                            }
                            mercs2_combat::ImpactKind::Blood => None,
                        };
                        if let Some(d) = desc {
                            scene.fx_start_desc(d, imp.position.to_array());
                        }
                    }
                    // Directional shadow key light, centred on the player. The travel direction is the
                    // negation of the main shader's fixed sun_dir (0.4,0.7,-0.5 = direction TO the sun),
                    // so the cast shadows line up with the sun shading. half_extent ~18 m covers the
                    // over-the-shoulder view around the player. (All three are tuning knobs.)
                    // Indoors (sun off) the shadow comes from OVERHEAD (ceiling/interior lighting) so it
                    // reads as a grounding contact shadow under the character, not an outdoor sun shadow;
                    // outdoors it aligns to the sun. (Direction/extent are tuning knobs.)
                    let shadow_dir = if spawn_interior { [-0.15, -1.0, 0.1] } else { [-0.4, -0.7, 0.5] };
                    scene.set_shadow(player.pos.to_array(), shadow_dir, 18.0);
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

/// Resolve a shell-menu selection into the boot configuration. `Some(path)` = parse that save
/// (recruit unlocks from the save's unlocked starters, stockpile cash from the header, and the
/// PLAYER MODEL from the saved hero + wardrobe outfit) — the same derivation `main.rs` uses for
/// direct boots. `None`, or an unreadable save, = new-game defaults (Mattias, Original outfit).
fn boot_config_from(
    sel: Option<&std::path::Path>,
) -> (crate::pmc::RecruitUnlocks, crate::pmc::Stockpile, Vec<String>, String) {
    // Retail new game = Mattias, upgrade tier 0, wardrobe untouched → his base/default skin
    // (matches the user's observed fresh retail saves).
    let new_game_models = || crate::hero::player_model_candidates(1, 0, 0);
    let Some(path) = sel else {
        return (Default::default(), Default::default(), new_game_models(), "new game".into());
    };
    let parsed = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| mercs2_formats::save::parse(&b));
    match parsed {
        Ok(prof) => {
            let recruits = prof
                .save_state()
                .ok()
                .map(|s| crate::pmc::RecruitUnlocks::from_starters(&s.unlocked_starters))
                .unwrap_or_default();
            let stockpile = crate::pmc::Stockpile { cash: prof.cash as i64, ..Default::default() };
            let hero_idx = prof.character_index; // header @0x4D, 1-based
            // Costume file byte not yet located (0 in every observed save — wardrobe unused);
            // the look is the upgrade tier's template model until then.
            let models = crate::hero::player_model_candidates(hero_idx, prof.upgrade_index, 0);
            let label = format!(
                "{} ({}, ${}, {}s played) as {} [{}]",
                prof.save_name(),
                prof.active_contract(),
                prof.cash,
                prof.play_time_seconds,
                crate::hero::hero(hero_idx).display,
                crate::hero::look_label(hero_idx, prof.upgrade_index, 0),
            );
            (recruits, stockpile, models, label)
        }
        Err(e) => {
            eprintln!("[shell] save {} unreadable ({e}) — booting new game", path.display());
            (Default::default(), Default::default(), new_game_models(), "new game (save unreadable)".into())
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

