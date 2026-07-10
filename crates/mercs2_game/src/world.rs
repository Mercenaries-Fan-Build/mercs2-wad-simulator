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

use crate::pmc::load_pmc_interior;

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
        ..Default::default()
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

// `LoadProgress` now comes from the engine (`mercs2_engine::render::LoadProgress`, glob-imported above) —
// the game's byte-identical copy was removed so the loader shares the engine's staged progress type that
// `app::run` renders the bar off.

/// Everything `--world` needs loaded before play: plain CPU data (Send), so it can be produced
/// on a background thread while the window shows the loading spinner.
pub struct WorldData {
    terrain: LoadedModel,
    player: Option<LoadedModel>,
    /// The shared swim locomotion clip hash resolved for the hero (loaded into `player.clips`), so the
    /// controller can switch to it in water. `None` if unresolved.
    player_swim_clip: Option<u32>,
    /// The held-weapon model (global_weapon_ak47) + the hero rig's `bone_rhand` index it attaches to.
    weapon: Option<LoadedModel>,
    weapon_hand_bone: Option<usize>,
    cells: Vec<(LoadedModel, [f32; 3])>,
    /// Merged placement-marker mesh (one model + one static entity), when `--placements` is set.
    placements: Option<LoadedModel>,
    /// Named world markers → world position (lowercased name → pos), harvested from the placement
    /// records. This is the engine's `Pg.GetGuidByName`→position lookup: the base game spawns the hero
    /// at a NAMED marker (`SetSpawnLocations`/`CreatePlayerCharacter(location=…)`, e.g. `PmcCon001_Start1`
    /// / `Pmc_Entry1`) resolved here — NOT a hardcoded coordinate (see `vanilla_boot_load_order.md`).
    named_locations: std::collections::HashMap<String, [f32; 3]>,
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
    /// Static watermap (the `watr` singleton) — surface height + wet mask over the Maracaibo XZ grid.
    /// Drives the player's swim-state FSM (wade/swim/submerge) and buoyant float. `None` if the WAD has
    /// no watermap (e.g. the interior-only boot).
    watermap: Option<mercs2_engine::water_sim::Watermap>,
    /// The HQ-interior hero spawn, derived the base-game way (actor position + `hp_playerA_enter`
    /// hardpoint) when `spawn_interior`. `None` for the exterior boot (the hero uses a named marker).
    interior_spawn: Option<[f32; 3]>,
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
    /// Resident audio, read off the load thread: decompressed `wavebank` bodies (the audio engine
    /// decodes each to PCM) + per-bank `sounddb` bodies (the cue→wave routing catalog). Sourced from the
    /// always-resident banks (`MrxSoundBootstrap.LoadBanks`); applied to the shared `AudioEngine` when
    /// the load completes so scripted `Sound.*` cues play real decoded waves. Empty for interior-only.
    wavebank_bodies: Vec<Vec<u8>>,
    sounddb_bodies: Vec<Vec<u8>>,
}

/// The always-resident gameplay / UI / ambience sound banks (`MrxSoundBootstrap.LoadBanks`) — loaded
/// as both a `wavebank` (PCM) and a `sounddb` (cue routing) under the same asset name.
const RESIDENT_SOUND_BANKS: &[&str] = &[
    "ui_hud", "ui_shell", "wpn_shared", "veh_shared", "veh_support", "ambience", "amb_birds",
    "amb_shared", "collision_shared", "destruction_shared", "fol_shared", "music",
];

/// Extract the resident wavebank + sounddb bodies from the WAD (best-effort per bank). A bank that
/// doesn't resolve is skipped (logged once); a partial set is still useful — every bank that loads adds
/// audible cues. Runs on the load thread, so it hands back raw bytes the main thread feeds the engine.
fn load_resident_audio(w: &mut wad::Wad) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    const SOUNDDB_TYPE: u32 = 0xE527_3C14;
    let mut wavebanks = Vec::new();
    let mut sounddbs = Vec::new();
    for name in RESIDENT_SOUND_BANKS {
        let nh = mercs2_formats::hash::pandemic_hash_m2(name);
        if let Ok(c) = wad::extract_container_typed(w, nh, mercs2_formats::types::TYPE_HASH_WAVEBANK) {
            if let Some(body) = mercs2_formats::ucfx::extract_chunk_body(&c, b"data") {
                wavebanks.push(body);
            }
        }
        // The per-bank sounddb body is a `data` chunk or the raw container (starts with the 0x1D tag).
        if let Ok(c) = wad::extract_container_typed(w, nh, SOUNDDB_TYPE) {
            let body = mercs2_formats::ucfx::extract_chunk_body(&c, b"data").unwrap_or(c);
            sounddbs.push(body);
        }
    }
    (wavebanks, sounddbs)
}

/// The loader's real phases, in order — the SINGLE SOURCE OF TRUTH for the loading bar's total (no
/// hand-synced magic number). `load_world_data` steps once per entry, so adding/removing a phase here
/// (and its matching `progress.step`) keeps the bar honest automatically. These cover the WHOLE load,
/// including the tail (watermap / resident audio / hero spawn) that used to run AFTER the bar hit 100%.
const LOAD_PHASES: &[&str] = &[
    "terrain", "heightmap", "vertices", "player", "clips", "cells", "placements", "interior", "props",
    "interior props", "lights + fx", "watermap", "resident audio", "hero spawn",
];
pub(crate) const LOAD_STAGES: u32 = LOAD_PHASES.len() as u32;

/// Exterior prop bounding: load only props within this radius (m) of the pool spawn, capped at
/// `EXTERIOR_PROP_CAP` distinct meshes, so `--props` stays light next to the full map.
const EXTERIOR_PROP_RADIUS: f32 = 400.0;
const EXTERIOR_PROP_CAP: usize = 200;

/// Player weapon: eye/muzzle height above the feet, the raycast range, and the full-auto fire interval
/// (≈600 rpm) that gates PrimaryAttack.
const PLAYER_EYE_HEIGHT: f32 = 1.6;
const PLAYER_WEAPON_RANGE: f32 = 300.0;
const PLAYER_FIRE_INTERVAL: f32 = 0.1;

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

/// Resolve a swimming locomotion clip through the real ActionTable + AnimationLookup (resident block
/// 3185). Swim animations are SHARED across mercs — the AnimationLookup keys them under the NONE
/// character sentinel (`0x27DE7135`), not each merc's hash — so the engine plays the same swim clips
/// for everyone in the `Swim` stance (`m2("Swim") = 0x614DB965`). Returns a surface-swim clip,
/// preferring one that is not the `Dive` plunge (`m2("Dive")` handle → `0x64B3CC44`).
fn resolve_player_swim(w: &mut wad::Wad) -> Option<u32> {
    use mercs2_formats::anim_select::AnimSelector;
    const NONE_KEY: u32 = 0x27DE_7135; // shared/character-agnostic AnimationLookup key
    const DIVE_CLIP: u32 = 0x64B3_CC44; // the m2("Dive") swim clip — a plunge, not surface locomotion
    let dec = wad::decompress_block_index(w, 3185).ok()?;
    let sel = AnimSelector::from_resident_block(&dec)?;
    let swim = mercs2_formats::hash::pandemic_hash_m2("Swim");
    let mut fallback = None;
    for (st, ac) in sel.action_states() {
        if st != swim {
            continue;
        }
        for h in sel.handles_for_state(st, ac) {
            if let Some(c) = sel.resolve_handle(h, NONE_KEY) {
                if c != DIVE_CLIP {
                    return Some(c);
                }
                fallback = fallback.or(Some(c));
            }
        }
    }
    fallback
}

/// Place the held-weapon entity at the hero's right hand for this frame: sample the hero's current
/// clip pose, take the `hand_bone`'s model-space matrix, and compose `player_world · hand · grip` into
/// the weapon's `Transform`. The hero's fit is identity (set at load) and the weapon's fit is too, so
/// the model→world chains line up. `GRIP` seats the gun in the palm (barrel forward) — a first-pass
/// offset tunable against the running game.
fn update_held_weapon(
    world: &mut World,
    store: &AssetStore,
    player_e: Entity,
    weapon_e: Entity,
    player_model: u32,
    hand_bone: usize,
) {
    use mercs2_core::glam::{Mat4, Vec3};
    // Grip transform in the hand-bone frame (tunable, first-pass). NOTE: the AK itself is a HARDCODED
    // stand-in (see load_world_data) — the real held weapon should come from the hero's inventory, and in
    // the PMC safe zone the hero is unarmed. Calibrating this grip is deferred until that's wired.
    let grip = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2) * Mat4::from_translation(Vec3::new(0.05, 0.0, 0.0));
    let (ppos, prot, clip, time) = {
        let Ok(t) = world.get::<&Transform>(player_e) else { return };
        let Ok(a) = world.get::<&AnimState>(player_e) else { return };
        (t.translation, t.rotation, a.clip, a.time)
    };
    let Some(ma) = store.models.get(&player_model) else { return };
    let Some(ca) = ma.clips.get(&clip).or_else(|| ma.clips.values().next()) else { return };
    let sample = ca.clip.sample_local(time);
    // Hand bone model-space matrix (row-vector, row-major). `from_cols_array_2d` reads its rows as
    // glam columns == the transpose == the correct column-major matrix for the render pipeline.
    let hand_rm = pose::bone_model_matrix(&ma.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks, hand_bone);
    let hand = Mat4::from_cols_array_2d(&hand_rm);
    let player_world = Mat4::from_translation(ppos) * Mat4::from_quat(prot);
    let (scale, rot, trans) = (player_world * hand * grip).to_scale_rotation_translation();
    if let Ok(mut wt) = world.get::<&mut Transform>(weapon_e) {
        wt.translation = trans;
        wt.rotation = rot;
        wt.scale = if scale.is_finite() { scale } else { Vec3::ONE };
    }
}

pub(crate) fn load_world_data(
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
    // Cohesive asset layer: base vz.wad + auto-discovered `vz-patch.wad` overlay (the game's patch
    // mechanism), resolved last-writer-wins. `base_mut()` feeds the base-only loader helpers unchanged;
    // patch content wins wherever a call routes through the stack's `extract_*` resolvers.
    let mut assets = mercs2_engine::asset::AssetSource::discover(wadpath, &[])?;
    let (low, ls) = find_terrain_blocks(assets.base_mut())?;
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
            ..Default::default()
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
    let idle_clip = resolve_player_idle(assets.base_mut(), character)
        .or_else(|| mercs2_formats::anim_select::fallback_idle(character))
        .unwrap_or(0x24F8_C8E6);
    println!("[world] player merc '{merc}' (CharacterName 0x{character:08X}) → idle clip 0x{idle_clip:08X}");

    let mut player_swim_clip: Option<u32> = None;
    let mut weapon: Option<LoadedModel> = None;
    let mut weapon_hand_bone: Option<usize> = None;
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
            Err(e) => println!("[world] player model {name} (0x{hash:08X}) failed ({e}); trying next"),
        }
    }
    let player = match player_loaded.ok_or_else(|| "no player-model candidate built".to_string()) {
        Ok((v, i, d, t, mut s, _c, h, _)) => {
            progress.step("player");
            s.center = [0.0, 0.0, 0.0];
            s.scale = 1.0;
            let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
            // Swim locomotion clip (shared, data-driven from the ActionTable). 0 when unresolved → the
            // load below simply finds no clip for it, and the controller falls back to walk/run in water.
            let swim_clip = resolve_player_swim(assets.base_mut()).unwrap_or(0);
            if swim_clip != 0 {
                println!("[world] player swim clip 0x{swim_clip:08X} (shared Swim-stance anim)");
            }
            let wanted = [idle_clip, 0x5368_2784, 0x867B_166D, swim_clip]; // idle (per-merc), walk, run, swim
            let names = ["idle", "walk", "run", "swim"];
            player_swim_clip = (swim_clip != 0).then_some(swim_clip);

            // Held weapon: NOT loaded here. There is no weapon-to-hand mapping in the exe that hands the
            // hero a fixed gun — the equipped weapon is the hero's INVENTORY (`Human.Inventory`
            // GetPrimaryWeapon/SetAllWeapons), populated by the loadout Lua / the save's inventory, and in
            // the PMC safe zone the hero is UNARMED. So `weapon` stays `None` until that inventory is
            // wired; the attachment mechanism (`update_held_weapon`) activates only when a real weapon is
            // equipped. (Was a hardcoded `global_weapon_ak47` stand-in — removed: nothing in the
            // disassembly asks for it.)
            let mut clips: Vec<ClipAnim> = Vec::new();
            for (found, (&h, name)) in load_clips_for_rig(assets.base_mut(), &hier, &wanted)
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
                    None => println!("[world] {name} clip 0x{h:08X} not found"),
                }
            }
            Some(LoadedModel { hash: h, verts: v, indices: i, draws: d, textures: t, skin: s, clips })
        }
        Err(e) => {
            println!("[world] player avatar load failed: {e}");
            progress.step("player");
            None
        }
    };
    progress.step("clips");

    // Hi-res c3 streaming-cell geometry near the spawn (opt-in; default off keeps --world stable).
    let cells = if load_cells {
        load_c3_cells(assets.base_mut(), 400.0, 16)
    } else {
        Vec::new()
    };
    progress.step(if load_cells { "cells" } else { "cells (skipped)" });

    // World placements (layers_static block 29): a merged marker mesh + the interior-hunt report,
    // plus an attempt to resolve the PMC-subset to real geometry (opt-in via `--placements`).
    let (placements, pmc_models, named_locations) = if load_placements {
        match mercs2_formats::placement::load_placements(&ls) {
            Ok(pl) => {
                report_interior_hunt(&pl);
                // Named markers → world position: the engine's Pg.GetGuidByName→pos lookup the base
                // game resolves the hero spawn against (SetSpawnLocations/CreatePlayerCharacter).
                let named: std::collections::HashMap<String, [f32; 3]> = pl
                    .iter()
                    .filter_map(|p| p.name.as_ref().map(|n| (n.to_ascii_lowercase(), p.pos)))
                    .collect();
                println!("[placements] {} named markers indexed for spawn resolution", named.len());
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
                let pmc = resolve_pmc_geometry(assets.base_mut(), &pl);
                (Some(markers), pmc, named)
            }
            Err(e) => {
                println!("[placements] load failed: {e}");
                (None, Vec::new(), std::collections::HashMap::new())
            }
        }
    } else {
        (None, Vec::new(), std::collections::HashMap::new())
    };
    progress.step(if load_placements { "placements" } else { "placements (skipped)" });

    // PMC interior (`--interior`): placement-driven interior geometry from state block 667, placed
    // at authored world coords (floor Y≈450.8) so the spawn drops the player inside the room.
    let interior = if spawn_interior {
        match load_pmc_interior(assets.base_mut(), recruits, stockpile) {
            Ok(v) => v,
            Err(e) => {
                println!("[interior] load failed: {e}");
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
        load_model_props(assets.base_mut(), &ls, Some(EXTERIOR_SPAWN), EXTERIOR_PROP_RADIUS, EXTERIOR_PROP_CAP)
    } else {
        Vec::new()
    };
    progress.step(if load_props { "props" } else { "props (skipped)" });

    // Interior props (`--interior`): ALL ModelName furniture placements in state block 667, at
    // their authored world transforms (the same anchor the shells are centred on).
    let interior_props = if spawn_interior {
        match wad::decompress_block_index(assets.base_mut(), PMC_INTERIOR_STATE_BLOCK) {
            Ok(dec) => load_model_props(assets.base_mut(), &dec, None, 0.0, usize::MAX),
            Err(e) => {
                println!("[interior props] state block {PMC_INTERIOR_STATE_BLOCK} decompress failed: {e}");
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
        if let Ok(dec) = wad::decompress_block_index(assets.base_mut(), PMC_INTERIOR_STATE_BLOCK) {
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
                    glow_cards.push(mercs2_engine::game_world::glow_card_for_effect(assets.base_mut(), name, p.pos));
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
    // The lights/FX harvest above (incl. the interior state-block decompress) is real load work that
    // used to run AFTER the bar hit 100% — count it as the final stage so the progress reflects reality.
    progress.step("lights + fx");

    // Static watermap (the `watr` singleton in the resident block) — the surface-height + wet-mask grid
    // the player's swim FSM samples. Best-effort: a WAD without it (interior-only) just yields no swim.
    let watermap = load_watermap(assets.base_mut());
    match &watermap {
        Some(_) => println!("[world] watermap loaded (swim enabled)"),
        None => println!("[world] no watermap in WAD (swim disabled)"),
    }
    progress.step("watermap");

    // Resident audio: the always-loaded gameplay/UI/ambience wavebanks + their cue-routing sounddbs.
    // (Reads + decompresses ~12 wavebanks + 11 sounddbs — real, slow load work, now counted.)
    let (wavebank_bodies, sounddb_bodies) = load_resident_audio(assets.base_mut());
    println!(
        "[world] resident audio: {} wavebanks + {} sounddbs read from WAD",
        wavebank_bodies.len(),
        sounddb_bodies.len()
    );
    progress.step("resident audio");

    // HQ-interior hero spawn, derived the base-game way (actor position + hp_playerA_enter hardpoint) so
    // the hero lands ON the interior floor (where the collision is), not at an exterior marker.
    let interior_spawn = if spawn_interior {
        let sp = crate::pmc::derive_interior_spawn(assets.base_mut());
        println!("[world] interior spawn derived (actor + hp_playerA_enter): ({:.1}, {:.1}, {:.1})", sp[0], sp[1], sp[2]);
        Some(sp)
    } else {
        None
    };
    progress.step("hero spawn");

    Ok(WorldData { terrain, player, player_swim_clip, weapon, weapon_hand_bone, cells, placements, named_locations, pmc_models, interior, props, interior_props, hmap, watermap, interior_spawn, lights, particle_fx, glow_cards, wavebank_bodies, sounddb_bodies })
}

/// Load the static watermap singleton (`m2("watermap")`, type `0x4D7D30C4`) from the resident block:
/// resolve its UCFX container, pull the `watr` chunk body, and parse the height-field + wet mask.
fn load_watermap(w: &mut wad::Wad) -> Option<mercs2_engine::water_sim::Watermap> {
    let name_hash = mercs2_formats::hash::pandemic_hash_m2("watermap");
    let container =
        wad::extract_container_typed(w, name_hash, mercs2_formats::types::TYPE_HASH_WATERMAP).ok()?;
    let watr = mercs2_formats::ucfx::extract_chunk_body(&container, b"watr")?;
    match mercs2_engine::water_sim::Watermap::from_watr_bytes(&watr) {
        Ok(wm) => Some(wm),
        Err(e) => {
            println!("[world] watermap parse failed: {e:?}");
            None
        }
    }
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
/// Sample the render terrain's `HeightMap` onto a regular grid → a `mercs2_engine::physics::Heightmap` the
/// fleet physics can raycast (K2 S3). The terrain extent is the engine's ±4000 m world square; a
/// 257×257 grid (~31 m cells) bilinearly interpolates smoothly enough for vehicle ground contact. The
/// render HeightMap keeps the exact triangles for the player walk; this is the physics-side heightfield.
fn heightmap_to_physics(hm: &mercs2_engine::worldutil::HeightMap) -> mercs2_engine::physics::Heightmap {
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
    mercs2_engine::physics::Heightmap::new(MIN, MIN, cell, W, W, heights)
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
        println!(
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
                println!("[cells] cell {cid} block {blk}: decompress failed: {e}");
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
            println!("[cells] cell {cid} block {blk}: model entry vanished on full decompress");
            continue;
        };
        let (verts, indices, draws, stats) = match mesh::build_indexed_from_container(&dec[s0..s1]) {
            Ok(v) => v,
            Err(e) => {
                println!("[cells] cell {cid} block {blk} model 0x{hash:08X}: container parse FAILED: {e}");
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
/// Resolve a shell-menu selection into the boot configuration. `Some(path)` = parse that save
/// (recruit unlocks from the save's unlocked starters, stockpile cash from the header, and the
/// PLAYER MODEL from the saved hero + wardrobe outfit) — the same derivation `main.rs` uses for
/// direct boots. `None`, or an unreadable save, = new-game defaults (Mattias, Original outfit).
/// Hero template name (`Pg.Spawn`/`CreatePlayerCharacter` character) for the 1-based header hero index
/// (`@0x4D`: 1 Mattias / 2 Chris / 3 Jen) — matches the vanilla trace's `type = chris/jen/mattias`.
fn hero_character_name(hero_idx: u8) -> &'static str {
    match hero_idx {
        2 => "chris",
        3 => "jen",
        _ => "mattias",
    }
}

fn boot_config_from(
    sel: Option<&std::path::Path>,
) -> (crate::pmc::RecruitUnlocks, crate::pmc::Stockpile, Vec<String>, String, String, String) {
    // Retail new game = Mattias, upgrade tier 0, wardrobe untouched → his base/default skin
    // (matches the user's observed fresh retail saves). New game = the opening contract PmcCon001,
    // so the hero spawns at `PmcCon001_Start1` (the base-game first-contract start).
    let new_game_models = || crate::hero::player_model_candidates(1, 0, 0);
    let Some(path) = sel else {
        return (Default::default(), Default::default(), new_game_models(), "new game".into(), "PmcCon001".into(), "mattias".into());
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
            (recruits, stockpile, models, label, prof.active_contract().to_string(), hero_character_name(hero_idx).to_string())
        }
        Err(e) => {
            println!("[shell] save {} unreadable ({e}) — booting new game", path.display());
            (Default::default(), Default::default(), new_game_models(), "new game (save unreadable)".into(), "PmcCon001".into(), "mattias".into())
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
    // Cohesive asset layer: base vz.wad + auto-discovered `vz-patch.wad` overlay (the game's patch
    // mechanism), resolved last-writer-wins. `base_mut()` feeds the base-only loader helpers unchanged;
    // patch content wins wherever a call routes through the stack's `extract_*` resolvers.
    let mut assets = mercs2_engine::asset::AssetSource::discover(wadpath, &[])?;
    let models = wad::model_list(assets.base());
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
    let container = wad::extract_container(assets.base_mut(), hash)?;
    let (verts, indices, draws, s) = mesh::build_indexed_from_container(&container)?;

    // Extract each unique diffuse + normal-map texture (DXT/BC bytes) for the placed groups.
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                match wad::extract_texture(assets.base_mut(), h) {
                    Ok(t) => {
                        textures.insert(h, t);
                    }
                    Err(e) => println!("  texture 0x{h:08X} unavailable: {e}"),
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
        match load_clip_for_rig(assets.base_mut(), &hier, clip_hash) {
            Some(ca) => {
                let resolved = ca.track_to_hier.iter().filter(|r| r.is_some()).count();
                println!(
                    "animation: clip 0x{:08X} ({} tracks, {} frames, {:.2}s), {resolved} tracks -> HIER bones",
                    ca.name_hash, ca.clip.num_tracks, ca.clip.num_frames, ca.clip.duration
                );
                Some(ca)
            }
            None => {
                println!("animation: no decodable clip bound to this model — using synthetic driver");
                None
            }
        }
    } else {
        None
    };

    let title = format!("Mercs 2 — model 0x{hash:08X} ({ntris} tris)");
    Ok((verts, indices, draws, textures, s.skin_data(), clip, hash, title))
}


// ===========================================================================
//   Mercs2Game — the TPS boot as a `mercs2_engine::app::Game` (Phase 5b).
//
//   This is the relocation of `run_scene_world_loading`'s body onto the unified engine loop: the ~30
//   `let mut` locals become fields; the world-realize block becomes `setup`; the variable-rate camera +
//   player sim becomes `update`; the fixed sim tick becomes `fixed_update`; the per-frame FX/shadow
//   becomes `render_prep`; the shell menu becomes `menu`. Behaviour is preserved verbatim — the engine
//   now owns the window / event loop / loading screen / render that this used to duplicate.
// ===========================================================================

/// Third-person vs free-fly debug camera.
#[derive(PartialEq)]
enum CamMode {
    Free,
    ThirdPerson,
}

/// Row-major identity matrix for static-entity skin palettes.
const GAME_IDENTITY: [[f32; 4]; 4] =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
/// Animation crossfade duration on clip switches (was fn-local in run_scene_world_loading).
const GAME_ANIM_BLEND_SEC: f32 = 0.25;

/// The fixed-timestep animation system (idle/walk/run/swim clip playback + Havok crossfade), lifted
/// from the world loop's `Schedule` closure so it can run once per fixed step from `fixed_update`.
fn animate_world(world: &mut World, time: &Time, assets: &AssetStore) {
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
        if state.blend < 1.0 {
            if let Some(cp) = ma.clips.get(&state.prev_clip) {
                let pdur = cp.clip.duration.max(1e-3);
                state.prev_time = (state.prev_time + time.dt * state.speed) % pdur;
                state.blend = (state.blend + time.dt / GAME_ANIM_BLEND_SEC).min(1.0);
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
}

/// The Mercenaries 2 third-person game as a `Game` over the engine's unified `app::run` loop.
pub struct Mercs2Game {
    // Boot config (all `true` for the retail boot; `--interior-orbit` sets `interior_orbit`).
    wadpath: String,
    start_tps: bool,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    interior_orbit: bool,
    // Shell menu + the selected (or direct-boot default) save parameters.
    menu: Option<crate::menu::Menu>,
    recruits: crate::pmc::RecruitUnlocks,
    stockpile: crate::pmc::Stockpile,
    player_models: Vec<String>,
    active_contract: String,
    hero_character: String,
    test_world: bool,
    menu_gp_prev: [bool; 4],
    menu_open: std::time::Instant,
    // Input bindings (Mercs2.ini) — for mouse sensitivity / invert-Y in `update`.
    bindings: mercs2_engine::input::Bindings,
    // The ECS World is owned by the engine (`app::run`) and lent via `Ctx` — the game does NOT keep its
    // own World (that was a two-Worlds bug: models spawned into the game's copy never render, because the
    // engine renders ITS World). The guidmap + Lua host are game-held; the host is attached to the app's
    // World in `setup`.
    guids: std::rc::Rc<std::cell::RefCell<mercs2_core::GuidMap>>,
    script_host: std::rc::Rc<std::cell::RefCell<mercs2_engine::script_host::GameScriptHost>>,
    script: Option<mercs2_engine::script::ScriptHost>,
    audio: std::rc::Rc<std::cell::RefCell<mercs2_engine::audio::AudioEngine>>,
    store: std::rc::Rc<std::cell::RefCell<AssetStore>>,
    runtime: mercs2_engine::runtime::GameRuntime,
    // Gameplay/camera runtime state, wired in on load.
    player: mercs2_engine::player::PlayerController,
    mode: CamMode,
    free_pos: Vec3,
    free_yaw: f32,
    free_pitch: f32,
    tp_yaw: f32,
    tp_pitch: f32,
    collision_tris: Vec<[Vec3; 3]>,
    hmap: Option<HeightMap>,
    watermap: Option<mercs2_engine::water_sim::Watermap>,
    fire_cooldown: f32,
    weapon_entity: Option<Entity>,
    weapon_hand_bone: usize,
    weapon_player_model: u32,
    game_start: std::time::Instant,
    mouse_dbg_frames: u32,
}

impl Mercs2Game {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
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
    ) -> Self {
        use std::cell::RefCell;
        use std::rc::Rc;
        let guids = Rc::new(RefCell::new(mercs2_core::GuidMap::new()));
        // Persistent mission-Lua host. The live World is attached in `setup` (it's owned by the engine
        // loop, not available at construction) so `Object.*`/`Pg.GetGuidByName` resolve real entities;
        // seed the economy from the save now.
        let script_host = Rc::new(RefCell::new(mercs2_engine::script_host::GameScriptHost::new("vz")));
        script_host.borrow_mut().set_cash(stockpile.cash);
        let script = mercs2_engine::script_host::resident_script_host(script_host.clone());
        if script.is_some() {
            println!("[world] persistent mission-Lua host resident (Event.__pump + runtime Pg.Spawn live)");
        }
        // Shared audio engine (host + fleet drive ONE engine); route it to the output device.
        let audio = script_host.borrow().audio();
        if audio.borrow_mut().attach_output_device() {
            println!("[audio] output device attached — mixer live");
        } else {
            println!("[audio] no output device — running headless (silent)");
        }
        let runtime = mercs2_engine::runtime::GameRuntime::new(audio.clone());
        let bindings = mercs2_engine::input::find_mercs2_ini()
            .map(|p| mercs2_engine::input::Bindings::load(&p))
            .unwrap_or_default();
        Mercs2Game {
            wadpath,
            start_tps,
            load_cells,
            load_placements,
            spawn_interior,
            load_props,
            interior_orbit,
            menu,
            recruits,
            stockpile,
            player_models,
            active_contract: String::from("PmcCon001"),
            hero_character: String::from("mattias"),
            test_world: false,
            menu_gp_prev: [false; 4],
            menu_open: std::time::Instant::now(),
            bindings,
            guids,
            script_host,
            script,
            audio,
            store: Rc::new(RefCell::new(AssetStore::default())),
            runtime,
            player: mercs2_engine::player::PlayerController::new(Vec3::ZERO),
            mode: CamMode::Free,
            free_pos: Vec3::new(0.0, 2500.0, 4500.0),
            free_yaw: 0.0,
            free_pitch: -0.5,
            tp_yaw: 0.0,
            tp_pitch: -0.12,
            collision_tris: Vec::new(),
            hmap: None,
            watermap: None,
            fire_cooldown: 0.0,
            weapon_entity: None,
            weapon_hand_bone: 0,
            weapon_player_model: 0,
            game_start: std::time::Instant::now(),
            mouse_dbg_frames: 0,
        }
    }

    /// Resolve a picked save into the boot configuration and store it for `spawn_loader`.
    fn apply_boot(&mut self, sel: Option<std::path::PathBuf>) {
        let (r, sp, models, label, contract, character) = boot_config_from(sel.as_deref());
        self.recruits = r;
        self.stockpile = sp;
        self.player_models = models;
        self.active_contract = contract;
        self.hero_character = character;
        println!("[shell] boot: {label}");
    }
}

impl mercs2_engine::app::Game for Mercs2Game {
    type LoadData = WorldData;

    fn config(&self) -> mercs2_engine::app::GameConfig {
        // Interior: no outdoor sun + a dark neutral fog (metres, not km). Exterior: key light + thin haze.
        let (fog, sun) = if self.spawn_interior {
            (([0.16, 0.17, 0.18], 0.0075, 2.0), Some((0.0, 0.30)))
        } else {
            (([0.55, 0.62, 0.70], 0.00016, 60.0), Some((0.9, 0.35)))
        };
        let bindings = mercs2_engine::input::find_mercs2_ini()
            .map(|p| mercs2_engine::input::Bindings::load(&p))
            .unwrap_or_default();
        mercs2_engine::app::GameConfig {
            title: "Mercenaries 2 — world (Tab: free / third-person)".into(),
            size: (1280.0, 720.0),
            grab_cursor: true,
            fog,
            sun,
            atmosphere: None, // TPS boot is fog-only (no explicit atmosphere), preserved
            loading_plate_wad: Some(self.wadpath.clone()),
            load_stages: LOAD_STAGES,
            bindings,
        }
    }

    fn starts_at_menu(&self) -> bool {
        self.menu.is_some()
    }

    fn menu(&mut self, ctx: &mut mercs2_engine::app::Ctx) -> mercs2_engine::app::MenuOutcome {
        use mercs2_engine::app::MenuOutcome;
        use mercs2_engine::input::Action;
        const MENU_ARM_DELAY: f32 = 0.4;
        let armed = self.menu_open.elapsed().as_secs_f32() > MENU_ARM_DELAY;
        // Gamepad edge nav (dpad/stick + A/Start select, B back), edge-detected vs last frame.
        let (_, my) = ctx.input.move_vec();
        let now = [
            ctx.input.held(Action::SelectUp) || my > 0.5,
            ctx.input.held(Action::SelectDown) || my < -0.5,
            ctx.input.held(Action::Jump) || ctx.input.held(Action::Start),
            ctx.input.held(Action::Crouch),
        ];

        // Nav + draw + render inside a scope that borrows only `self.menu` (disjoint from the fields
        // `apply_boot`/`test_world` touch below), returning the resulting action as a value.
        let action = {
            let Some(m) = self.menu.as_mut() else { return MenuOutcome::StartLoad };
            let mut action = crate::menu::MenuAction::None;
            // Did the keyboard consume this frame's input? A MOVE (Up/Down/Back) returns
            // `MenuAction::None`, so we CANNOT gate the gamepad path on `action == None` — the arrow keys
            // also map to the `Select*` actions the gamepad reads, so a single Down would fire twice
            // (keyboard move + gamepad "SelectDown"), skipping every other row.
            let mut kbd_acted = false;
            // Keyboard edge nav (rising key edges the engine resolved this frame).
            if armed {
                for &code in ctx.pressed.iter() {
                    let nav = match code {
                        KeyCode::ArrowUp | KeyCode::KeyW => Some(crate::menu::Nav::Up),
                        KeyCode::ArrowDown | KeyCode::KeyS => Some(crate::menu::Nav::Down),
                        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Some(crate::menu::Nav::Select),
                        KeyCode::Escape | KeyCode::Backspace => Some(crate::menu::Nav::Back),
                        _ => None,
                    };
                    if let Some(nav) = nav {
                        action = m.nav(nav);
                        kbd_acted = true;
                        if !matches!(action, crate::menu::MenuAction::None) {
                            break;
                        }
                    }
                }
            }
            // Gamepad nav only if the keyboard didn't already provide input this frame.
            if !kbd_acted {
                let navs = [crate::menu::Nav::Up, crate::menu::Nav::Down, crate::menu::Nav::Select, crate::menu::Nav::Back];
                for i in 0..4 {
                    if armed && now[i] && !self.menu_gp_prev[i] {
                        action = m.nav(navs[i]);
                        if !matches!(action, crate::menu::MenuAction::None) {
                            break;
                        }
                    }
                }
            }
            let t = self.menu_open.elapsed().as_secs_f32();
            m.draw(ctx.scene, t);
            match ctx.scene.render_menu(t) {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => ctx.scene.resize(ctx.scene.size),
                Err(e) => println!("surface error: {e:?}"),
            }
            action
        };
        self.menu_gp_prev = now;

        match action {
            crate::menu::MenuAction::Boot(sel) => {
                self.apply_boot(sel);
                MenuOutcome::StartLoad
            }
            crate::menu::MenuAction::BootTestWorld => {
                self.test_world = true;
                self.apply_boot(None);
                MenuOutcome::StartLoad
            }
            crate::menu::MenuAction::Quit => MenuOutcome::Exit,
            crate::menu::MenuAction::None => MenuOutcome::Stay,
        }
    }

    fn spawn_loader(
        &self,
        progress: std::sync::Arc<LoadProgress>,
    ) -> std::sync::mpsc::Receiver<Result<WorldData, String>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let wadpath = self.wadpath.clone();
        let (lc, lp, si, lpr) = (self.load_cells, self.load_placements, self.spawn_interior, self.load_props);
        let recruits = self.recruits.clone();
        let stockpile = self.stockpile.clone();
        let models = self.player_models.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let r = load_world_data(&wadpath, lc, lp, si, lpr, recruits, &stockpile, &models, &progress);
            if r.is_ok() {
                println!("[load] done in {:.1}s", t0.elapsed().as_secs_f64());
            }
            let _ = tx.send(r);
        });
        rx
    }

    fn setup(&mut self, ctx: &mut mercs2_engine::app::Ctx, mut data: WorldData) {
        const IDENTITY: [[f32; 4]; 4] = GAME_IDENTITY;
        // Marry the Lua host to the engine's live World (the one the renderer draws): now `Object.*` /
        // `Pg.GetGuidByName` and every spawn below land in the World that actually renders.
        self.script_host.borrow_mut().attach_world(ctx.world.clone(), self.guids.clone());
        let scene = &mut *ctx.scene;
        // ---- Hero spawn (data-driven): interior derivation OR a named contract/HQ marker. ----
        if let Some(sp) = data.interior_spawn {
            self.player.pos = Vec3::new(sp[0], sp[1] + 2.0, sp[2]);
            println!("[world] hero spawn: HQ interior (actor + hardpoint) -> ({:.1}, {:.1}, {:.1})", self.player.pos.x, self.player.pos.y, self.player.pos.z);
        } else {
            let contract_marker = format!("{}_start1", self.active_contract.to_ascii_lowercase());
            let candidates = [contract_marker.as_str(), "pmc_entry1", "pmc_start1"];
            let resolved = candidates
                .iter()
                .filter(|&&n| !n.is_empty() && n != "_start1")
                .find_map(|&n| data.named_locations.get(n).map(|&p| (n, p)));
            match resolved {
                Some((name, p)) => {
                    self.player.pos = Vec3::new(p[0], p[1], p[2]);
                    println!("[world] hero spawn: marker '{name}' -> ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]);
                }
                None => {
                    println!("[world] SPAWN MARKER unresolved ({} named markers total; none of {candidates:?})", data.named_locations.len());
                }
            }
        }

        // Entity-ize the named world markers into the live World + guidmap (so `Pg.GetGuidByName`
        // resolves real entities), BEFORE the boot Lua flow's CreatePlayerCharacter → GetGuidByName.
        {
            let mut w = ctx.world.borrow_mut();
            let host = self.script_host.borrow();
            for (name, pos) in &data.named_locations {
                let e = w.spawn((Transform::from_translation(Vec3::from(*pos)),));
                host.register_named_entity(e, mercs2_formats::hash::pandemic_hash_m2(name));
            }
            println!("[world] {} named markers registered as live entities (guidmap)", data.named_locations.len());
        }
        if let Some(sh) = &self.script {
            self.script_host.borrow_mut().set_boot_context(self.hero_character.clone());
            mercs2_engine::script_host::run_boot_flow(sh, &self.script_host, &self.active_contract, &self.hero_character);
            if data.interior_spawn.is_none() {
                if let Some(p) = self.script_host.borrow_mut().take_hero_spawn() {
                    self.player.pos = Vec3::new(p[0], p[1], p[2]);
                    println!("[world] hero spawn via boot Lua flow: ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]);
                }
            }
        }

        // Terrain (skipped in --interior: it sits above the SE terrain peak and would occlude the room).
        let terrain = data.terrain;
        if !std::env::args().any(|a| a == "--interior") {
            scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
            ctx.world.borrow_mut().spawn((
                Transform::IDENTITY,
                ModelRef { model: terrain.hash },
                AnimState::default(),
                SkinPalette { mats: vec![IDENTITY] },
            ));
        }

        // Placement-marker DEBUG glyphs (`--markers`).
        if let (Some(pm), true) = (data.placements, std::env::args().any(|a| a == "--markers")) {
            scene.load_model(pm.hash, &pm.verts, &pm.indices, &pm.draws, &pm.textures, &pm.skin);
            ctx.world.borrow_mut().spawn((
                Transform::IDENTITY,
                ModelRef { model: pm.hash },
                AnimState::default(),
                SkinPalette { mats: vec![IDENTITY] },
            ));
        }

        // PMC-subset real geometry (`--placements`).
        for (m, pos, yaw) in data.pmc_models {
            scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
            t.rotation = Quat::from_rotation_y(yaw);
            ctx.world.borrow_mut().spawn((t, ModelRef { model: m.hash }, AnimState::default(), SkinPalette { mats: vec![IDENTITY] }));
        }

        // Hi-res c3 cell geometry (`--cells`) — collect world-space triangles for collision.
        for (m, off) in data.cells {
            scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            let tr = Vec3::new(off[0], off[1], off[2]);
            for idx in m.indices.chunks_exact(3) {
                self.collision_tris.push([
                    Vec3::from(m.verts[idx[0] as usize].pos) + tr,
                    Vec3::from(m.verts[idx[1] as usize].pos) + tr,
                    Vec3::from(m.verts[idx[2] as usize].pos) + tr,
                ]);
            }
            ctx.world.borrow_mut().spawn((
                Transform::from_translation(tr),
                ModelRef { model: m.hash },
                AnimState::default(),
                SkinPalette { mats: vec![IDENTITY] },
            ));
        }

        // PMC interior geometry (`--interior`) — shells are walls → collision.
        for (m, pos, quat) in data.interior {
            scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            let tr = Vec3::new(pos[0], pos[1], pos[2]);
            let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
            let mut t = Transform::from_translation(tr);
            t.rotation = q;
            for idx in m.indices.chunks_exact(3) {
                let w = |i: usize| q * Vec3::from(m.verts[i].pos) + tr;
                self.collision_tris.push([w(idx[0] as usize), w(idx[1] as usize), w(idx[2] as usize)]);
            }
            let nbones = m.skin.bones.len().max(1);
            ctx.world.borrow_mut().spawn((t, ModelRef { model: m.hash }, AnimState::default(), SkinPalette { mats: vec![IDENTITY; nbones] }));
        }

        // ModelName props (exterior + interior furniture) — each non-water instance blocks → collision.
        let mut prop_meshes = 0usize;
        let mut prop_instances = 0usize;
        for (hash, m, instances) in data.props.into_iter().chain(data.interior_props) {
            scene.load_model(hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            prop_meshes += 1;
            let nbones = m.skin.bones.len().max(1);
            for (pos, quat) in instances {
                let tr = Vec3::new(pos[0], pos[1], pos[2]);
                let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                for idx in m.indices.chunks_exact(3) {
                    let w = |i: u32| q * Vec3::from(m.verts[i as usize].pos) + tr;
                    self.collision_tris.push([w(idx[0]), w(idx[1]), w(idx[2])]);
                }
                let mut t = Transform::from_translation(tr);
                t.rotation = q;
                ctx.world.borrow_mut().spawn((t, ModelRef { model: hash }, AnimState::default(), SkinPalette { mats: vec![IDENTITY; nbones] }));
                prop_instances += 1;
            }
        }
        if prop_meshes > 0 {
            println!("[world] props spawned: {prop_meshes} distinct meshes, {prop_instances} instances");
        }

        // Player avatar.
        if let Some(p) = data.player {
            self.player.has_run = p.clips.iter().any(|c| c.name_hash == mercs2_engine::player::CLIP_RUN);
            self.player.swim_clip = data.player_swim_clip.unwrap_or(0);
            self.player.idle = p.clips.iter().map(|c| c.name_hash)
                .find(|h| *h != mercs2_engine::player::CLIP_WALK
                    && *h != mercs2_engine::player::CLIP_RUN
                    && Some(*h) != data.player_swim_clip)
                .unwrap_or(mercs2_engine::player::CLIP_IDLE);
            for c in &p.clips {
                let d = c.clip.duration.max(1e-3);
                let sp = pose::clip_root_speed(
                    &p.skin.rig,
                    &c.clip.sample_local(0.0),
                    &c.clip.sample_local(d * 0.999),
                    &c.track_to_hier,
                    c.num_transform_tracks,
                    d * 0.999,
                );
                if c.name_hash == mercs2_engine::player::CLIP_WALK {
                    self.player.dur_walk = d;
                    if sp > 0.1 { self.player.walk_speed = sp; }
                } else if c.name_hash == mercs2_engine::player::CLIP_RUN {
                    self.player.dur_run = d;
                    if sp > 0.1 { self.player.run_speed = sp; }
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
            let min_y = p.verts.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
            self.player.foot = if min_y.is_finite() { -min_y } else { 0.0 };
            let playing = !p.clips.is_empty();
            self.store.borrow_mut().models.insert(p.hash, ModelAnim {
                rig,
                clips: p.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
            });
            let anim = if playing { AnimState::playing(self.player.idle) } else { AnimState::default() };
            let npc_bind = if self.test_world { bind.clone() } else { Vec::new() };
            let mut t = Transform::from_translation(self.player.pos);
            t.rotation = Quat::from_rotation_y(0.0);
            self.player.entity = Some(ctx.world.borrow_mut().spawn((t, ModelRef { model: p.hash }, anim, SkinPalette { mats: bind })));
            if let Some(pe) = self.player.entity {
                self.script_host.borrow().register_entity(pe, mercs2_engine::script_host::HERO_GUID, None);
            }
            // TEST WORLD: a visible NPC (same merc model) facing the hero — an actor to build onto.
            if self.test_world {
                let npc_pos = self.player.pos + Vec3::new(3.0, 0.0, 12.0);
                let mut nt = Transform::from_translation(npc_pos);
                nt.rotation = Quat::from_rotation_y(std::f32::consts::PI);
                let npc_anim = if playing { AnimState::playing(self.player.idle) } else { AnimState::default() };
                ctx.world.borrow_mut().spawn((nt, ModelRef { model: p.hash }, npc_anim, SkinPalette { mats: npc_bind }));
                println!("[test-world] NPC placed at ({:.1},{:.1},{:.1}) facing the hero", npc_pos.x, npc_pos.y, npc_pos.z);
            }
            // Held weapon on the hero's right-hand bone.
            if let (Some(mut wm), Some(hb)) = (data.weapon, data.weapon_hand_bone) {
                wm.skin.center = [0.0, 0.0, 0.0];
                wm.skin.scale = 1.0;
                let ident = vec![IDENTITY; wm.skin.rig.len().max(1)];
                scene.load_model(wm.hash, &wm.verts, &wm.indices, &wm.draws, &wm.textures, &wm.skin);
                self.weapon_entity = Some(ctx.world.borrow_mut().spawn((
                    Transform::from_translation(self.player.pos),
                    ModelRef { model: wm.hash },
                    SkinPalette { mats: ident },
                )));
                self.weapon_hand_bone = hb;
                self.weapon_player_model = p.hash;
                println!("[world] held weapon 0x{:08X} on bone_rhand (rig idx {hb})", wm.hash);
            }
        }
        self.hmap = Some(data.hmap);
        self.watermap = data.watermap;

        // Resident audio: decode wavebanks + merge sounddbs into one cue catalog.
        if !data.wavebank_bodies.is_empty() {
            let mut a = self.audio.borrow_mut();
            let mut audible = 0usize;
            for body in &data.wavebank_bodies {
                audible += a.load_wavebank(body);
            }
            let mut catalog = mercs2_engine::audio::SoundDb::default();
            for body in &data.sounddb_bodies {
                if let Ok(db) = mercs2_engine::audio::SoundDb::parse(body) {
                    catalog.merge(&db);
                }
            }
            let cues = catalog.cues.len();
            a.set_sounddb(catalog);
            println!("[audio] resident: {} clips ({audible} audible), {cues} cues in catalog", a.resident_wave_count());
        }

        // Translucent water surface (render-graph node).
        if let Some(wm) = &self.watermap {
            let (wpos, widx) = wm.surface_mesh();
            let node = mercs2_engine::water::WaterNode::new(
                scene.device(),
                scene.surface_format(),
                &wpos,
                &widx,
                mercs2_engine::water::WaterStyle::default(),
            );
            if let Some(node) = node {
                println!("[world] water surface: {} quads", widx.len() / 6);
                scene.add_render_node(Box::new(node));
            }
        }
        println!("[world] collision: {} world-space triangles (buildings + interior shells)", self.collision_tris.len());
        self.runtime.set_collision(self.collision_tris.clone());
        if let Some(hm) = self.hmap.as_ref() {
            let phm = heightmap_to_physics(hm);
            println!("[world] terrain heightmap -> fleet physics ({}x{} grid)", phm.width, phm.depth);
            self.runtime.set_heightmap(Some(phm));
        }
        scene.set_lights(std::mem::take(&mut data.lights));
        // Environmental FX: glow cards (god rays) + particle emitters (fire/smoke/steam).
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
        if self.start_tps && self.player.entity.is_some() {
            self.mode = CamMode::ThirdPerson;
        }
        self.game_start = std::time::Instant::now();
    }

    fn update(&mut self, ctx: &mut mercs2_engine::app::Ctx) -> mercs2_engine::app::Camera {
        use mercs2_engine::input::Action;
        // Tab toggles the free / third-person camera (rising edge).
        if ctx.pressed.contains(&KeyCode::Tab) {
            self.mode = if self.mode == CamMode::Free { CamMode::ThirdPerson } else { CamMode::Free };
        }
        let dt = ctx.dt;
        let look = 1.6 * dt;
        // Mouse-look: apply ini sensitivity + invert-Y to the engine-resolved delta.
        let sens = self.bindings.mouse_rad_per_px;
        let inv_y = if self.bindings.invert_y { -1.0 } else { 1.0 };
        let src = ctx.mouse_delta;
        let mdx = src.0.clamp(-80.0, 80.0) * sens;
        let mdy = src.1.clamp(-80.0, 80.0) * sens * inv_y;
        if src != (0.0, 0.0) && self.mouse_dbg_frames < 20 {
            println!("[mouse] in=({:+.1},{:+.1}) applied=({:+.4},{:+.4})", src.0, src.1, mdx, mdy);
            self.mouse_dbg_frames += 1;
        }
        match self.mode {
            CamMode::Free => {
                self.free_yaw += mdx;
                self.free_pitch = (self.free_pitch - mdy).clamp(-1.5, 1.5);
            }
            CamMode::ThirdPerson => {
                self.tp_yaw += mdx;
                self.tp_pitch = (self.tp_pitch - mdy).clamp(-1.2, 0.6);
            }
        }

        let inp = ctx.input;
        let (gp_yaw, gp_pitch) = inp.look_delta(dt);
        let mut view = match self.mode {
            CamMode::Free => {
                if inp.keys.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { self.free_pitch += look; }
                if inp.keys.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { self.free_pitch -= look; }
                if inp.keys.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { self.free_yaw -= look; }
                if inp.keys.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { self.free_yaw += look; }
                self.free_yaw += gp_yaw;
                self.free_pitch = (self.free_pitch + gp_pitch).clamp(-1.5, 1.5);
                let fwd = Vec3::new(self.free_pitch.cos() * self.free_yaw.sin(), self.free_pitch.sin(), self.free_pitch.cos() * self.free_yaw.cos()).normalize();
                let right = fwd.cross(Vec3::Y).normalize();
                let (mx, my) = inp.move_vec();
                let mut mv = fwd * my + right * mx;
                if inp.held(Action::Jump) { mv += Vec3::Y; }
                if inp.held(Action::Crouch) { mv -= Vec3::Y; }
                let sp = if inp.held(Action::Sprint) { 3200.0 } else { 800.0 };
                if mv.length_squared() > 1e-6 { self.free_pos += mv.clamp_length_max(1.0) * sp * dt; }
                Mat4::look_to_lh(self.free_pos, fwd, Vec3::Y)
            }
            CamMode::ThirdPerson => {
                if inp.keys.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { self.tp_pitch += look; }
                if inp.keys.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { self.tp_pitch -= look; }
                if inp.keys.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { self.tp_yaw -= look; }
                if inp.keys.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { self.tp_yaw += look; }
                self.tp_yaw += gp_yaw;
                self.tp_pitch = (self.tp_pitch + gp_pitch).clamp(-1.2, 0.6);
                let fwd_flat = Vec3::new(self.tp_yaw.sin(), 0.0, self.tp_yaw.cos()).normalize();
                let right_flat = fwd_flat.cross(Vec3::Y).normalize();
                let (mx, my) = inp.move_vec();
                let mv = fwd_flat * my + right_flat * mx;
                self.player.update(
                    &mut ctx.world.borrow_mut(),
                    mv,
                    inp.held(Action::Sprint),
                    inp.held(Action::Jump),
                    &self.collision_tris,
                    self.hmap.as_ref(),
                    self.watermap.as_ref(),
                    self.spawn_interior,
                    dt,
                );
                // Player weapon fire — STAND-IN (raycast + invented range/interval; the real path is the
                // equipped weapon's `Weapon.*` fire through its `wpn_*` stats). Gated on actually holding a
                // weapon, so with no equipped gun (e.g. the unarmed PMC) there is no fire.
                self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);
                let can_fire = self.weapon_entity.is_some() && !self.player.swim.is_swimming();
                if can_fire && inp.held(Action::PrimaryAttack) && self.fire_cooldown <= 0.0 {
                    self.fire_cooldown = PLAYER_FIRE_INTERVAL;
                    let aim = Vec3::new(self.tp_pitch.cos() * self.tp_yaw.sin(), self.tp_pitch.sin(), self.tp_pitch.cos() * self.tp_yaw.cos()).normalize();
                    let eye = self.player.pos + Vec3::Y * PLAYER_EYE_HEIGHT;
                    if let Some(t) = mercs2_engine::physics::soup::raycast(&self.collision_tris, eye, aim, PLAYER_WEAPON_RANGE) {
                        let point = eye + aim * t;
                        self.runtime.push_impact(mercs2_engine::combat::Impact::from_hit(point, Vec3::ZERO, aim, false));
                    }
                }
                // Mode-based camera: pick the reflected preset from whatever the player is riding (on
                // foot → OnFoot). `ridden` stays `None` until vehicle-riding is wired; the selection +
                // preset are already the real engine shape.
                let preset = mercs2_engine::camera::CameraMode::for_ridden(None).preset();
                mercs2_engine::camera::view_with_preset(&preset, self.player.pos, self.tp_yaw, self.tp_pitch, &self.collision_tris)
            }
        };

        // Interior debug orbit (`--interior-orbit`): replace the view with an auto-orbit each frame.
        if self.interior_orbit {
            const ANCHOR: Vec3 = Vec3::new(3779.8, 454.7, -3879.6);
            const RADIUS: f32 = 38.0;
            const HEIGHT: f32 = 52.0;
            let ang = self.game_start.elapsed().as_secs_f32() * 0.25;
            let eye = ANCHOR + Vec3::new(RADIUS * ang.sin(), HEIGHT, RADIUS * ang.cos());
            view = Mat4::look_at_lh(eye, ANCHOR, Vec3::Y);
        }

        let pos = if self.mode == CamMode::Free { self.free_pos } else { self.player.pos };
        // Near/far: on foot use the reflected preset (PMC `SetNearFar(0, 0.3, 500, 0)` from the game's
        // Lua); free-fly/orbit keep the wide far so the whole world stays visible.
        let (near, far) = if self.interior_orbit || self.mode == CamMode::Free {
            (if self.interior_orbit { 1.0 } else { 0.5 }, 30000.0)
        } else {
            let p = mercs2_engine::camera::CameraMode::for_ridden(None).preset();
            (p.near, p.far)
        };
        mercs2_engine::app::Camera { view, pos, near, far }
    }

    fn fixed_update(&mut self, ctx: &mut mercs2_engine::app::Ctx) {
        // Animation (idle/walk/run/swim + crossfade) at the fixed tick.
        animate_world(&mut ctx.world.borrow_mut(), ctx.time, &self.store.borrow());
        // Fleet gameplay (vehicle/combat/physics/audio) + population, same fixed cadence.
        self.runtime.tick(&mut ctx.world.borrow_mut(), ctx.time.fixed_dt);
        self.runtime.tick_population(&mut ctx.world.borrow_mut(), ctx.time.fixed_dt, self.player.pos);
        // Persistent mission-Lua: advance the event/timer system, then realize its runtime Pg.Spawns.
        if let Some(sh) = &self.script {
            mercs2_engine::script_host::pump_resident(sh, ctx.time.fixed_dt);
            let new_spawns = self.script_host.borrow_mut().take_new_spawns();
            if !new_spawns.is_empty() {
                let realized = self.runtime.realize_spawns(&mut ctx.world.borrow_mut(), &new_spawns);
                {
                    let host = self.script_host.borrow();
                    for (req, (e, _)) in new_spawns.iter().zip(&realized) {
                        let nh = (!req.name.is_empty()).then(|| mercs2_formats::hash::pandemic_hash_m2(&req.name.to_ascii_lowercase()));
                        host.register_entity(*e, req.guid, nh);
                    }
                }
                println!("[world] realized {} runtime spawn(s) from mission Lua", realized.len());
            }
        }
    }

    fn render_prep(&mut self, ctx: &mut mercs2_engine::app::Ctx) {
        // Pump the software mixer at wall-clock rate.
        self.audio.borrow_mut().pump(ctx.dt);
        // Held weapon follows the hero's right-hand bone (after the anim schedule posed the hero).
        if let (Some(we), Some(pe)) = (self.weapon_entity, self.player.entity) {
            update_held_weapon(&mut ctx.world.borrow_mut(), &self.store.borrow(), pe, we, self.weapon_player_model, self.weapon_hand_bone);
        }
        // Combat impact FX: explosion → fireball, bullet → dust puff (blood is decal-only).
        for imp in self.runtime.take_render_impacts() {
            let desc = match imp.kind {
                mercs2_engine::combat::ImpactKind::Explosion => Some(mercs2_engine::particles::EmitterDesc::impact_fire()),
                mercs2_engine::combat::ImpactKind::Bullet => Some(mercs2_engine::particles::EmitterDesc::impact_puff()),
                mercs2_engine::combat::ImpactKind::Blood => None,
            };
            if let Some(d) = desc {
                ctx.scene.fx_start_desc(d, imp.position.to_array());
            }
        }
        // Directional shadow key light, centred on the player (overhead indoors, sun-aligned outdoors).
        let shadow_dir = if self.spawn_interior { [-0.15, -1.0, 0.1] } else { [-0.4, -0.7, 0.5] };
        ctx.scene.set_shadow(self.player.pos.to_array(), shadow_dir, 18.0);
    }
}
