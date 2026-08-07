//! GAME render/boot: the full third-person + free-fly world render path — player avatar, TPS/free
//! camera toggle, and the 10-stage full load (terrain / heightmap / player / clips / c3 cells /
//! placements / interior / props). This is GAME code: it spawns the player + the PMC interior + world
//! content, driving the asset-agnostic engine through its public API (`mercs2_engine::{wad, mesh, pose,
//! scene, render, game_world, worldutil}` + `crate::pmc`).
//!
//! Recovered from the pre-teardown engine bin (git 78944ed) — it was wrongly deleted with that bin
//! (it is render/boot code, not the engine). `run_scene_world_loading` is the `--world` / TPS entry.

use std::collections::HashMap;

use mercs2_core::glam::{Mat4, Quat, Vec3};
use mercs2_core::{AnimState, Entity, ModelRef, SkinPalette, Transform, World};
use mercs2_engine::game_world::*;
use mercs2_engine::mesh::Vertex;
use mercs2_engine::render::*;
use mercs2_engine::scene::{AssetStore, ModelAnim};
use mercs2_engine::worldutil::*;
use mercs2_engine::{mesh, pose, wad};
use winit::keyboard::KeyCode;

use crate::pmc::load_pmc_interior;

// ---------------------------------------------------------------------------
//   Restored render/boot code (verbatim from the deleted engine bin, path-adapted via the `use`
//   prelude above). See git 78944ed:crates/mercs2_engine/src/main.rs.
//
//   W7 relocation: the engine mechanism this file used to hand-roll now lives in its owning crate —
//   `build_placement_markers` / `mesh_collision_tris` / `HeightMap::to_physics_heightmap` in
//   `worldutil`, `load_watermap` / `load_resident_audio` in `asset`, the clip resolvers in
//   `game_world`, `animate_assetstore` in `scene`, and `attach_to_bone` in `pose`. This file keeps
//   Mercs2 boot policy + orchestration.
// ---------------------------------------------------------------------------

/// Present the game's [`AssetStore`] as `mercs2_anim::AnimAssets`, so the faithful per-entity
/// [`mercs2_engine::anim::animation_system`] (data-driven clip selection + crossfade over
/// `AnimController` / `HumanAnimationSet`) can drive spawned population **Characters** using the SAME
/// rig + Havok clip decode the hero already loads — no second anim path. The engine's `mesh::BoneRig`
/// and the anim crate's `pose::BoneRig` are structurally identical; the rigs are converted once into an
/// owned map so `rig()` can return a borrow (the trait requires `&[BoneRig]`).
struct StoreAnimAssets<'a> {
    store: &'a AssetStore,
    rigs: HashMap<u32, Vec<mercs2_engine::anim::pose::BoneRig>>,
}

impl<'a> StoreAnimAssets<'a> {
    fn new(store: &'a AssetStore) -> Self {
        let rigs = store
            .models
            .iter()
            .map(|(&h, m)| (h, m.rig.iter().map(conv_bone_rig).collect()))
            .collect();
        StoreAnimAssets { store, rigs }
    }
}

/// Copy an engine `mesh::BoneRig` into the anim crate's structurally identical `pose::BoneRig`.
fn conv_bone_rig(b: &mesh::BoneRig) -> mercs2_engine::anim::pose::BoneRig {
    mercs2_engine::anim::pose::BoneRig {
        parent: b.parent,
        name_hash: b.name_hash,
        world_bind: b.world_bind,
        inv_bind: b.inv_bind,
        local_bind: b.local_bind,
    }
}

impl mercs2_engine::anim::AnimAssets for StoreAnimAssets<'_> {
    fn rig(&self, model: u32) -> Option<&[mercs2_engine::anim::pose::BoneRig]> {
        self.rigs.get(&model).map(|v| v.as_slice())
    }
    fn clip_duration(&self, model: u32, clip: u32) -> Option<f32> {
        let ca = self.store.models.get(&model)?.clips.get(&clip)?;
        Some(ca.clip.duration.max(1e-3))
    }
    fn sample(&self, model: u32, clip: u32, time: f32) -> Option<mercs2_engine::anim::SampledPose> {
        let ca = self.store.models.get(&model)?.clips.get(&clip)?;
        Some(mercs2_engine::anim::SampledPose {
            locals: ca.clip.sample_local(time),
            track_to_hier: ca.track_to_hier.clone(),
            num_transform_tracks: ca.num_transform_tracks,
        })
    }
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

/// One preloaded ambient-population NPC template: the loaded model (mesh + textures + rig, with its
/// idle clip already bound to its HIER) plus the resident idle clip name-hash. Built on the load thread
/// exactly like the player avatar and realized in `setup` — uploaded to the scene (GPU) + AssetStore
/// (anim) and registered on the runtime, so a population `ModelRef{template_hash}` draws and a spawned
/// Character animates. `idle_clip == 0` (or `confirm_live`) marks a template whose CharacterName→idle
/// did not resolve to a per-character clip (fell back to the shared/none idle, or none) — CONFIRM-LIVE.
struct NpcTemplateLoad {
    model: LoadedModel,
    idle_clip: u32,
    confirm_live: bool,
}

/// Everything `--world` needs loaded before play: plain CPU data (Send), so it can be produced
/// on a background thread while the window shows the loading spinner.
pub struct WorldData {
    /// The merged low-res terrain mesh. DEAD post-K2-streaming: `setup` uploads the streamer's own
    /// terrain (`into_streaming_world`), never this — kept only so the loader's "[world] terrain:" log
    /// + the "vertices" progress phase are unchanged. Remove when the terrain build is pruned.
    #[allow(dead_code)]
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
    /// records. These are entity-ized at setup so `Pg.GetGuidByName` resolves them to LIVE entities,
    /// which is how the boot flow places the hero: the master script picks a marker NAME
    /// (`VzaCon001_Start1` for a new game, `Pmc_Entry1` when resuming) and
    /// `CreatePlayerCharacter` turns it into a position via `Pg.GetGuidByName` → `Object.GetPosition`.
    /// Nothing here is a hardcoded coordinate (see `vanilla_boot_load_order.md`).
    named_locations: std::collections::HashMap<String, [f32; 3]>,
    /// The world's transit landing pads, from the `LandingZone` COMP joined to each pad's Transform/Name
    /// (`worldutil::landing_zone_pads`). Entity-ized at setup so `Pg.GetAllLandingZones` — which
    /// `MrxTransit.Reset` calls on the very first save the boot writes — has real objects to return.
    /// Independent of `--placements`: the transit system is not a debug view.
    landing_zones: Vec<mercs2_engine::worldutil::LandingZonePad>,
    /// Layer name → the objects it contains, so a completed `Pg.LoadLayer` can wake them.
    layer_index: mercs2_engine::worldutil::LayerIndex,
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
    /// Dynamic `LightObject` **spot** lights (`light_type` 3) from the same inventory, aimed along
    /// their placement's local −Y. Fed to `Scene::set_spot_lights` (the `_sl` per-pixel cone path).
    spot_lights: Vec<mercs2_engine::scene::SpotLightGpu>,
    /// Authored `global_particle_*` FX placements (effect name + world position) — each starts an
    /// emitter (classified by name). The faithful producer for environmental particle effects
    /// (fire/smoke/steam). Static environmental *glows* (god-ray light shafts) are split out into
    /// `glow_cards` at load, where the WAD is open to read their effect template.
    particle_fx: Vec<(mercs2_engine::particles::EmitterDesc, [f32; 3])>,
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
    /// The streaming-world runtime (K2 unification): block index + Layer-2 streaming catalog + WAD
    /// handle, built on this load thread (it is `Send`). REPLACES the static exterior-props + c3-cell
    /// preload — `setup` uploads its terrain and takes ownership of the [`StreamingWorld`] executor.
    /// Opaque here (its fields are crate-private to `mercs2_engine`); consumed via `into_streaming_world`.
    streaming: StreamingWorldData,
    /// The decompressed `layers_static` block (block 29), carried off the load thread so `setup` can feed
    /// its authored `PopulationSimpleSpawner` COMPs into the runtime's population manager
    /// (`GameRuntime::load_population_spawners`). Without this the spawner pool stays empty → no ambient
    /// crowds/traffic ever spawn. It is the SAME `ls` the terrain/placement/light passes read above.
    population_block: Vec<u8>,
    /// The bounded set of ambient-population NPC templates (`FACTION_TEMPLATE_NAMES`), preloaded on the
    /// load thread: mesh + textures + rig + bound idle clip per resolvable template. Realized in `setup`
    /// (scene + AssetStore + runtime registration) so population-spawned Characters render + animate.
    /// Shorter than 7 if any template failed to build (each failure is logged, never aborts the load).
    npc_templates: Vec<NpcTemplateLoad>,
}

/// The loader's real phases, in order — the SINGLE SOURCE OF TRUTH for the loading bar's total (no
/// hand-synced magic number). `load_world_data` steps once per entry, so adding/removing a phase here
/// (and its matching `progress.step`) keeps the bar honest automatically. These cover the WHOLE load,
/// including the tail (watermap / resident audio / hero spawn) that used to run AFTER the bar hit 100%.
const LOAD_PHASES: &[&str] = &[
    "terrain", "heightmap", "vertices", "player", "clips", "npc templates", "cells", "placements", "interior", "props",
    "interior props", "lights + fx", "watermap", "resident audio", "hero spawn",
    // The streaming-world build (`load_streaming_world_data`) steps these four itself — the K2
    // executor's block index + Layer-2 catalog, produced on this same load thread.
    "stream blocks", "stream terrain", "stream index", "stream catalog",
];
pub(crate) const LOAD_STAGES: u32 = LOAD_PHASES.len() as u32;

/// Starting health for a destructible prop. **A placeholder**: retail reads per-object HP from the
/// object's own data (Xbox `VehicleHealth` has a vehicle analogue; the prop field is not recovered),
/// so this is a single value until that lands. It only affects how much damage a prop absorbs, not
/// which states its machine reaches.
const DEFAULT_PROP_HEALTH: f32 = 250.0;

/// Player weapon: eye/muzzle height above the feet, the raycast range, and the full-auto fire interval
/// (≈600 rpm) that gates PrimaryAttack.
const PLAYER_EYE_HEIGHT: f32 = 1.6;
const PLAYER_WEAPON_RANGE: f32 = 300.0;
const PLAYER_FIRE_INTERVAL: f32 = 0.1;

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
    // Sample the hand-bone model matrix and compose `player_world · hand · grip` in the engine
    // (`pose::attach_to_bone`); decompose + write the weapon Transform here (the ECS glue).
    let player_world = Mat4::from_translation(ppos) * Mat4::from_quat(prot);
    let weapon_world = pose::attach_to_bone(
        &ma.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks, hand_bone, player_world, grip,
    );
    let (scale, rot, trans) = weapon_world.to_scale_rotation_translation();
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
    overlays: &[String],
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
        machine: None, hier: Vec::new(),
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
    // Held weapon: NOT loaded here (see the player-load block below) — the equipped weapon comes from
    // the hero's inventory, and in the PMC safe zone the hero is unarmed. These stay `None`.
    let weapon: Option<LoadedModel> = None;
    let weapon_hand_bone: Option<usize> = None;
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
            Some(LoadedModel { hash: h, verts: v, indices: i, draws: d, textures: t, skin: s, clips, machine: None, hier: Vec::new() })
        }
        Err(e) => {
            println!("[world] player avatar load failed: {e}");
            progress.step("player");
            None
        }
    };
    progress.step("clips");

    // ---- Ambient-population NPC templates (preload) ----
    // Make the BOUNDED set of faction human templates the population spawns (FACTION_TEMPLATE_NAMES)
    // resident at boot, the SAME way the hero is loaded: build each mesh+textures+rig via `load_from_wad`
    // (identity fit → world metres, like the player), then resolve + bind its idle clip through the SAME
    // resident-AnimationLookup path (`AnimSelector` CharacterName → `resolve_player_idle` →
    // `load_clips_for_rig`). `setup` uploads each to the scene (GPU) + AssetStore (anim) and registers it
    // on the runtime, so a population `ModelRef{template_hash}` draws + a spawned Character animates.
    // A template that fails to build is skipped (logged), never aborting the load.
    const NONE_ANIM_KEY: u32 = 0x27DE_7135; // shared/character-agnostic AnimationLookup key (idle fallback)
    let mut npc_templates: Vec<NpcTemplateLoad> = Vec::new();
    for name in mercs2_engine::population::slot_table::FACTION_TEMPLATE_NAMES {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        // Build via `game_world::load_model_by_hash` — the SAME loader the population/`slot_table`
        // verification uses. It handles the NON-PRIMARY human containers (e.g. `civ_hum_beachfemale_a`,
        // `vz_hum_soldierelite_a`) that `load_from_wad` (primary-model path) rejects — which is why the
        // default civilian was invisible before.
        let Some((mut m, _bmin, _bmax)) = mercs2_engine::game_world::load_model_by_hash(assets.base_mut(), hash)
        else {
            println!("[world] NPC template {name} (0x{hash:08X}) failed to build; skipping");
            continue;
        };
        // World scale + identity fit, matching the player so NPCs stand at hero size.
        m.skin.center = [0.0, 0.0, 0.0];
        m.skin.scale = 1.0;
        let hier: Vec<u32> = m.skin.rig.iter().map(|b| b.name_hash).collect();
        // Idle via the resident AnimationLookup keyed by the template's CharacterName; then the per-merc
        // static fallback (misses for NPCs); then the shared NONE-key idle → CONFIRM-LIVE.
        let character = mercs2_formats::anim_select::AnimSelector::character_name(name);
        let mut confirm_live = false;
        let resolved_idle = resolve_player_idle(assets.base_mut(), character)
            .or_else(|| mercs2_formats::anim_select::fallback_idle(character))
            .or_else(|| {
                confirm_live = true;
                resolve_player_idle(assets.base_mut(), NONE_ANIM_KEY)
            })
            .unwrap_or(0);
        // Bind the resolved idle to THIS model's HIER (the same loader the hero's clips use). Keep only
        // clips that actually decoded + bound; the registered idle is the one that loaded.
        let wanted: Vec<u32> = if resolved_idle != 0 { vec![resolved_idle] } else { Vec::new() };
        let clips: Vec<ClipAnim> = load_clips_for_rig(assets.base_mut(), &hier, &wanted)
            .into_iter()
            .flatten()
            .collect();
        let idle_clip = clips.first().map(|c| c.name_hash).unwrap_or(0);
        let confirm_live = confirm_live || idle_clip == 0;
        m.clips = clips;
        m.hier = Vec::new();
        println!(
            "[world] NPC template {name} (0x{:08X}): {} bones, idle 0x{idle_clip:08X}{}",
            m.hash,
            m.skin.rig.len(),
            if confirm_live { " (CONFIRM-LIVE: shared/no idle)" } else { "" }
        );
        npc_templates.push(NpcTemplateLoad { model: m, idle_clip, confirm_live });
    }
    println!("[world] NPC templates built: {}/7", npc_templates.len());
    progress.step("npc templates");

    // Static c3-cell preload REMOVED (K2 unification): the `StreamingWorld` executor now LOADs c3
    // streaming cells by camera distance each frame, so a fixed near-spawn preload would double-load
    // the same geometry. Kept as an empty vec so the `WorldData` shape (and setup's cell loop) is
    // unchanged. `load_cells` is retained for the CLI shape but no longer drives a static load.
    let _ = load_cells;
    let cells: Vec<(LoadedModel, [f32; 3])> = Vec::new();
    progress.step("cells (streaming)");

    // The world's name → position index, over EVERY placement-bearing block (not just `layers_static`).
    //
    // Deliberately NOT behind `--placements`. That flag gates a debug *marker mesh*; this is the table
    // `Pg.GetGuidByName` resolves against — 1240 corpus call sites, and the mission scripts' only way to
    // reach an authored entity. Gating it made name resolution a rendering option, which is why
    // `VzaCon001` parked on `VzaCon001_StartingBoat` in every run that did not pass the flag.
    let named_locations = mercs2_engine::worldutil::world_name_index(assets.base_mut(), &ls);

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
        machine: None, hier: Vec::new(),
                    hash: 0x504C_4143, // "PLAC" — arbitrary key for the merged marker mesh
                    verts,
                    indices,
                    draws,
                    textures: TexMap::new(),
                    skin: mesh::SkinData::identity(),
                    clips: Vec::new(),
                };
                let pmc = resolve_pmc_geometry(assets.base_mut(), &ls, &pl);
                (Some(markers), pmc)
            }
            Err(e) => {
                println!("[placements] load failed: {e}");
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

    // Static exterior props REMOVED (K2 unification): the `StreamingWorld` executor now WAKEs
    // `ModelName` props by proximity (its per-entity hibernation distances), replacing the capped
    // near-spawn bubble this used to load. Interior FURNITURE (below) stays static — the streamer does
    // not own the interior overlay. Empty vec keeps the `WorldData` shape + setup's prop loop unchanged.
    let _ = load_props;
    let props: Vec<(u32, LoadedModel, Vec<PropInstance>)> = Vec::new();
    progress.step("props (streaming)");

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
    let ls_lights = mercs2_formats::placement::light_inventory(&ls);
    let mut lights = mercs2_engine::game_world::placed_lights_to_gpu(&ls_lights);
    // The spot half of the same inventory (light_type 3) — aimed along each placement's local -Y.
    let mut spot_lights = mercs2_engine::game_world::placed_spot_lights_to_gpu(&ls_lights);
    // Environmental FX placements (Name-keyed `global_particle_*` Transforms) in the interior. These
    // split by kind: static environmental light-shaft glows (god rays) become additive glow cards
    // resolved against their effect template here (WAD open); fire/smoke/steam stay as particle
    // emitters classified by name at render-thread start.
    let mut particle_fx: Vec<(mercs2_engine::particles::EmitterDesc, [f32; 3])> = Vec::new();
    let mut glow_cards: Vec<mercs2_engine::particles::GlowCard> = Vec::new();
    if spawn_interior {
        if let Ok(dec) = wad::decompress_block_index(assets.base_mut(), PMC_INTERIOR_STATE_BLOCK) {
            let int_lights = mercs2_formats::placement::light_inventory(&dec);
            lights.extend(mercs2_engine::game_world::placed_lights_to_gpu(&int_lights));
            spot_lights.extend(mercs2_engine::game_world::placed_spot_lights_to_gpu(&int_lights));
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
                } else if let Some(base) = classify_particle(name) {
                    // Real authored effect params (COLR/FRCE/PTYP) if the template resolves; else the
                    // name-heuristic base shape. Resolved here where the WAD is open, once at load.
                    let hash = mercs2_formats::hash::pandemic_hash_m2(&name.replace("particle_", ""));
                    let desc = match mercs2_engine::game_world::load_effect_template(assets.base_mut(), hash) {
                        Some(t) => mercs2_engine::particles::EmitterDesc::from_effect_template(&t, base),
                        None => base,
                    };
                    particle_fx.push((desc, p.pos));
                }
            }
        }
    }
    println!(
        "[world] dynamic lights harvested: {} point + {} spot; particle placements: {}; light-shaft glows: {}",
        lights.len(), spot_lights.len(), particle_fx.len(), glow_cards.len()
    );
    // The lights/FX harvest above (incl. the interior state-block decompress) is real load work that
    // used to run AFTER the bar hit 100% — count it as the final stage so the progress reflects reality.
    progress.step("lights + fx");

    // Static watermap (the `watr` singleton in the resident block) — the surface-height + wet-mask grid
    // the player's swim FSM samples. Best-effort: a WAD without it (interior-only) just yields no swim.
    let watermap = assets.load_watermap();
    match &watermap {
        Some(wm) => {
            let wet = wm.wet_cell_count();
            let (lo, hi) = wm.wet_height_range().unwrap_or((f32::NAN, f32::NAN));
            println!(
                "[world] watermap loaded (swim enabled): {wet} wet / {} dry cells, surface {lo:.1}..{hi:.1} m",
                wm.width() * wm.height() - wet,
            );
        }
        None => println!("[world] no watermap in WAD (swim disabled)"),
    }
    progress.step("watermap");

    // Resident audio: the always-loaded gameplay/UI/ambience wavebanks + their cue-routing sounddbs.
    // (Reads + decompresses ~12 wavebanks + 11 sounddbs — real, slow load work, now counted.)
    let (wavebank_bodies, sounddb_bodies) =
        mercs2_engine::asset::load_resident_audio(assets.base_mut());
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

    // Transit landing pads (`LandingZone` COMP). Read unconditionally: `MrxTransit.Reset` runs on the
    // boot's first save regardless of any debug flag, and `SaveSingleton` (mrxtransit.lua:367) iterates
    // `_tLandingZones` with none of the `if not _tLandingZones` guards its siblings carry — an empty
    // list there is a shipped-bug crash, not a degraded view.
    let landing_zones = mercs2_engine::worldutil::landing_zone_pads(&ls);
    println!("[world] {} transit landing pads read from LandingZone COMP", landing_zones.len());

    // Which objects each streamable layer brings in — the data behind `Event.ObjectHibernation`.
    // Layers are ordinary ASET assets (type 9), so this is a straight archive read, not a side table.
    let layer_index = mercs2_engine::worldutil::layer_index(assets.base_mut());

    // Streaming world (K2 unification): build the whole block index + Layer-2 streaming catalog on THIS
    // load thread (it is `Send`), folding in the save's vz_state overlays. This REPLACES the static
    // exterior props + c3 preload removed above; `setup` uploads its terrain and drives the executor.
    // Budgets/radii mirror the free-fly boot (`FreeFlyGame::spawn_loader`) so both boots stream alike.
    // Its four `progress.step`s are the last four `LOAD_PHASES` ("stream …") — the bar stays honest.
    let stream_cfg = mercs2_core::streaming::StreamingConfig {
        block_unload_margin: 200.0,
        block_budget: 2,
        entity_budget: 6,
        entity_hysteresis: 15.0,
        entity_scan_cap: 700.0,
        grid_cell: 128.0,
        ..mercs2_core::streaming::StreamingConfig::default()
    };
    let streaming = mercs2_engine::game_world::load_streaming_world_data(wadpath, stream_cfg, overlays, progress)?;

    Ok(WorldData { terrain, player, player_swim_clip, weapon, weapon_hand_bone, cells, placements, named_locations, landing_zones, layer_index, pmc_models, interior, props, interior_props, hmap, watermap, interior_spawn, lights, spot_lights, particle_fx, glow_cards, wavebank_bodies, sounddb_bodies, streaming, population_block: ls, npc_templates })
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
/// Map a live decal instance's material row key back to its category index (0..4 in `PgDecalTable`
/// order — BulletHole/Blood/Scorch/TireTrack/DamageShadow), the index the decal shader selects its
/// look with. Falls back to 0 (bullet hole) for an unrecognised key.
fn decal_category_index(def_key: u32) -> u32 {
    mercs2_engine::decal::DecalType::all()
        .iter()
        .position(|t| t.hash() == def_key)
        .unwrap_or(0) as u32
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
    block: &[u8],
    placements: &[mercs2_formats::placement::Placement],
) -> Vec<(LoadedModel, [f32; 3], f32)> {
    use mercs2_formats::placement::yaw_from_quat;
    // The REAL entity-key -> model-asset hash mapping, from the block's `ModelName` COMP (joined to each
    // entity's Transform by `load_model_placements`). This replaces the old `pandemic_hash_m2(name)`
    // guess — that hashed the gameplay NAME (`_pmcoutpost_bld_barracks01`), which is not the model asset
    // hash, so it resolved almost nothing. `ModelName`'s second u32 IS the model ASET hash.
    let model_by_key: std::collections::HashMap<u32, u32> =
        mercs2_formats::placement::load_model_placements(block)
            .into_iter()
            .map(|mp| (mp.key, mp.model_hash))
            .collect();
    let subset: Vec<&mercs2_formats::placement::Placement> =
        placements.iter().filter(|p| placement_is_pmc_subset(p)).collect();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out: Vec<(LoadedModel, [f32; 3], f32)> = Vec::new();
    let (mut tried, mut ok) = (0u32, 0u32);
    for p in &subset {
        if out.len() >= 64 {
            break;
        }
        // Resolve by the entity's OWN key through the ModelName COMP. A PMC-subset entity without a
        // ModelName record is a pure gameplay marker (no mesh) — skipped, not guessed.
        let Some(&hash) = model_by_key.get(&p.key) else { continue };
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
                    let name = p.name.as_deref().unwrap_or("<unnamed>");
                    println!(
                        "[pmc-geo] '{name}' model=0x{hash:08X}: LOADED {} verts / {} tris",
                        verts.len(), indices.len() / 3
                    );
                    out.push((
                        LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new(), machine: None, hier: Vec::new() },
                        p.pos,
                        yaw_from_quat(&p.quat),
                    ));
                    ok += 1;
                }
                Err(e) => println!("[pmc-geo] model 0x{hash:08X}: container parse FAILED: {e}"),
            },
            Err(_) => { /* the ModelName pointed at a model ASET this base WAD lacks — logged by count */ }
        }
    }
    println!(
        "[pmc-geo] key->mesh via ModelName COMP: {} distinct models tried, {} resolved to a model ASET (of {} PMC-subset placements)",
        tried, ok, subset.len()
    );
    out
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

/// Fog + sun for the two boot environments, as `((color, density, start), Some((intensity, ambient)))`.
/// Interior: no outdoor sun + a dark neutral fog (metres, not km). Exterior: key light + thin haze.
/// Shared by `config` (opening value) and `setup` (branch-correct value) so the two cannot drift.
fn atmosphere_for(interior: bool) -> (([f32; 3], f32, f32), Option<(f32, f32)>) {
    if interior {
        (([0.16, 0.17, 0.18], 0.0075, 2.0), Some((0.0, 0.30)))
    } else {
        (([0.55, 0.62, 0.70], 0.00016, 60.0), Some((0.9, 0.35)))
    }
}

/// Everything a shell selection resolves into before the world loads.
pub(crate) struct BootConfig {
    pub recruits: crate::pmc::RecruitUnlocks,
    pub stockpile: crate::pmc::Stockpile,
    pub models: Vec<String>,
    /// Human-readable one-liner for the `[shell] boot:` log.
    pub label: String,
    /// The save's active contract. **Display only** — it does NOT decide where the hero spawns; the
    /// master script's boot branch does (see [`mercs2_engine::script_host::run_boot_flow`]).
    pub contract: String,
    /// `mattias` / `chris` / `jen` — the `Pg.Spawn` template the boot flow creates.
    pub hero_character: String,
    /// The save being resumed, or `None` for a **new game**. This is the value that picks the master
    /// script's branch, and therefore the hero's start marker: `None` → `VzaCon001_Start1` (opening
    /// contract); `Some` with `retry_locations` → the mid-contract checkpoint marker (in the world);
    /// `Some` without → `Pmc_Entry1` (HQ entrance). See [`mercs2_engine::script_host::BootSaveState`].
    pub save: Option<mercs2_engine::script_host::BootSaveState>,
}

/// Translate a parsed save into the boot payload the master script's `LoadSingleton` reads. The field
/// mapping is one-to-one with `MrxMissionFlow.SaveSingleton` (`mrxmissionflow.lua:597`), which is where
/// these same fields were serialized from.
fn boot_save_from(state: &mercs2_formats::save::SaveState) -> mercs2_engine::script_host::BootSaveState {
    mercs2_engine::script_host::BootSaveState {
        flow_keys: state.completed_flow.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        culled_bindings: state.flow_chain.clone(),
        active_missions: state.active_missions.iter().map(|m| m.id.clone()).collect(),
        // The resume-spawn gate: a mid-contract save carries its checkpoint marker(s), which keeps the
        // master script off the `Pmc_Entry1` (PMC hub) path. See `BootSaveState::retry_locations`.
        retry_locations: state.retry_locations.clone(),
        layers: state.layers.clone(),
        transit_enabled: state.transit_enabled,
        transit_zones: state.transit_zones.clone(),
    }
}

fn boot_config_from(sel: Option<&std::path::Path>) -> BootConfig {
    // Retail new game = Mattias, upgrade tier 0, wardrobe untouched → his base/default skin
    // (matches the user's observed fresh retail saves).
    //
    // NOTE: a new game carries NO save (`save: None`), which is the whole point — that is what makes
    // `xQ!L.LoadSingleton` take its new-game branch and start the hero at `VzaCon001_Start1`, the
    // opening contract, before the player owns the PMC. `contract` below is a display label for the
    // shell; it is deliberately not consulted when placing the hero.
    let new_game = |label: &str| BootConfig {
        recruits: Default::default(),
        stockpile: Default::default(),
        models: crate::hero::player_model_candidates(1, 0, 0),
        label: label.into(),
        contract: "VzaCon001".into(),
        hero_character: "mattias".into(),
        save: None,
    };
    let Some(path) = sel else { return new_game("new game") };
    let parsed = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| mercs2_formats::save::parse(&b));
    match parsed {
        Ok(prof) => {
            let state = prof.save_state().ok();
            let recruits = state
                .as_ref()
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
            // A save whose Lua payload wouldn't decode still RESUMES — it is not a new game. Handing the
            // master script an empty-but-present table keeps it on the resume branch (Pmc_Entry1) rather
            // than silently replaying the intro.
            let save = Some(state.as_ref().map(boot_save_from).unwrap_or_default());
            BootConfig {
                recruits,
                stockpile,
                models,
                label,
                contract: prof.active_contract().to_string(),
                hero_character: hero_character_name(hero_idx).to_string(),
                save,
            }
        }
        Err(e) => {
            println!("[shell] save {} unreadable ({e}) — booting new game", path.display());
            new_game("new game (save unreadable)")
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
//   Mercs2Game — the TPS boot as a `mercs2_engine::app::Game` (relocated onto the unified engine loop).
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

/// The Mercenaries 2 third-person game as a `Game` over the engine's unified `app::run` loop.
/// A minimal [`mercs2_core::PhysicsQuery`] over the game's static+streamed collision world, used to
/// ground the vehicle drive step's wheel raycasts (`mercs2_vehicle::drive_step_system`). Wraps the
/// engine's triangle-set raycast; the broadphase raycast returns only a hit distance, so the surface
/// normal is approximated as world-up — adequate for wheel suspension on mostly-flat ground, a
/// `// CONFIRM-LIVE:` refinement for steep terrain (the real path returns the triangle normal).
struct TriPhysicsQuery<'a> {
    tris: &'a [[Vec3; 3]],
}

/// The persistent-broadphase unit key for the STATIC collision baseline (interior shells + furniture),
/// inserted once in `setup`. Kept clear of the streamed block/prop keys (`block_key` ≈ `2^32+`, `prop_key`
/// < `2^32`) so it never aliases a streamed unit's insert/remove.
const STATIC_COLLISION_KEY: u64 = 1u64 << 60;

impl mercs2_core::PhysicsQuery for TriPhysicsQuery<'_> {
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<mercs2_core::RayHit> {
        mercs2_engine::physics::broadphase::raycast(self.tris, origin, dir, max).map(|t| mercs2_core::RayHit {
            point: origin + dir * t,
            normal: Vec3::Y,
            distance: t,
            entity: None,
        })
    }
    fn closest_point(&self, _p: Vec3, _max: f32) -> Option<mercs2_core::physics_query::ClosestPoint> {
        None
    }
    fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
        pos + delta
    }
}

/// A live death ragdoll: the recovered multi-body `mercs2_physics` sim plus the per-entity data its
/// per-tick step + skin read-back need (the character's rig, and the working model-space pose whose
/// driven bones the ragdoll overwrites while the rest stay frozen at the death frame). Held game-side in
/// [`Mercs2Game::death_ragdolls`] because the rigs live in the game's `AssetStore`.
struct ActiveRagdoll {
    rd: mercs2_engine::physics::ragdoll::Ragdoll,
    rig: Vec<mercs2_engine::anim::pose::BoneRig>,
    model_pose: Vec<[[f32; 4]; 4]>,
}

/// The engine `mesh::BoneRig` and the anim seam's `anim::pose::BoneRig` are field-identical but distinct
/// types; the ragdoll seed/read-back speak the anim one, so copy the fields across.
fn to_anim_rig(rig: &[mesh::BoneRig]) -> Vec<mercs2_engine::anim::pose::BoneRig> {
    rig.iter()
        .map(|b| mercs2_engine::anim::pose::BoneRig {
            parent: b.parent,
            name_hash: b.name_hash,
            world_bind: b.world_bind,
            inv_bind: b.inv_bind,
            local_bind: b.local_bind,
        })
        .collect()
}

pub struct Mercs2Game {
    /// Debris templates a destruction machine asked for while the model was not resident. Counted
    /// rather than dropped silently — a zero here means every requested piece actually spawned.
    debris_unresident: usize,
    /// Live death ragdolls keyed by corpse entity (W6): each killed rigged character's constrained
    /// multi-body ragdoll, spawned from its posed skeleton, stepped + skin-written each fixed tick.
    death_ragdolls: HashMap<Entity, ActiveRagdoll>,
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
    /// The dev/debug free-fly camera (eye + yaw/pitch); the fly math lives in `mercs2_engine::camera`.
    free_cam: mercs2_engine::camera::FreeCamera,
    tp_yaw: f32,
    tp_pitch: f32,
    /// The vehicle the hero is currently driving (seated in the driver seat), or `None` on foot.
    /// Real ride state (W4): enter/exit on `Use`, drive via the `mercs2_vehicle` sim, chase-cam via
    /// the recovered vehicle camera preset. Resolves to a real entity once a drivable vehicle is
    /// entered (drivable vehicles are fed into the ECS by the W1/W2 content streams).
    ridden: Option<Entity>,
    /// Rising-edge latch for the `Use` (enter/exit) button.
    use_prev: bool,
    /// The donut/turn sine LUT the vehicle drive step samples (built once, reused each ride frame).
    veh_lut: mercs2_engine::vehicle::DonutLut,
    /// The STATIC collision baseline — interior shells + interior furniture (the exterior props + c3
    /// cells that used to live here are streamed now). Inserted ONCE into the persistent broadphase as
    /// one unit ([`STATIC_COLLISION_KEY`]) in `setup`; the streamer then feeds per-unit WAKE/HIBERNATE
    /// deltas alongside it.
    collision_tris: Vec<[Vec3; 3]>,
    /// The save's vz_state overlay layer names (set in `apply_boot`) — passed to the streaming loader as
    /// the interior/staging/faction layers folded into the streaming catalog.
    overlays: Vec<String>,
    /// The reusable streaming executor (K2 unification), owned once `setup` hands it the loaded state.
    /// `None` before load, and on the static `--interior` boot (which streams nothing).
    stream: Option<StreamingWorld>,
    hmap: Option<HeightMap>,
    watermap: Option<mercs2_engine::water_sim::Watermap>,
    fire_cooldown: f32,
    weapon_entity: Option<Entity>,
    weapon_hand_bone: usize,
    weapon_player_model: u32,
    game_start: std::time::Instant,
    mouse_dbg_frames: u32,
    /// The live-bridge server (see [`crate::bridge_host`]), if the REPL port was free at boot. `update` drains
    /// its queued chunks each frame and evaluates them on this (the main) thread's Lua VM.
    bridge: Option<crate::bridge_host::BridgeHost>,
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
            debris_unresident: 0,
            death_ragdolls: HashMap::new(),
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
            free_cam: mercs2_engine::camera::FreeCamera::new(
                Vec3::new(0.0, 2500.0, 4500.0), 0.0, -0.5,
            ),
            tp_yaw: 0.0,
            tp_pitch: -0.12,
            ridden: None,
            use_prev: false,
            veh_lut: mercs2_engine::vehicle::DonutLut::new(),
            collision_tris: Vec::new(),
            overlays: Vec::new(),
            stream: None,
            hmap: None,
            watermap: None,
            fire_cooldown: 0.0,
            weapon_entity: None,
            weapon_hand_bone: 0,
            weapon_player_model: 0,
            game_start: std::time::Instant::now(),
            mouse_dbg_frames: 0,
            // Start the live-bridge server (see `bridge_host`). `None` if the port is busy (the retail
            // ASI is running, or another instance) — the game boots regardless.
            bridge: {
                let b = crate::bridge_host::BridgeHost::start();
                match &b {
                    Some(_) => println!(
                        "[mercs2_game] live bridge listening on {} — the Workshop console can attach",
                        mercs2_bridge::DEFAULT_ADDR
                    ),
                    None => println!(
                        "[mercs2_game] live bridge port {} busy — running without a console attach",
                        mercs2_bridge::DEFAULT_ADDR
                    ),
                }
                b
            },
        }
    }

    /// Resolve a picked save into the boot configuration and store it for `spawn_loader`.
    ///
    /// `sel = None` is **New Game**: no save reaches the script host, so the master script takes its
    /// new-game branch and the hero starts at the opening contract instead of inside the PMC HQ.
    pub(crate) fn apply_boot(&mut self, sel: Option<std::path::PathBuf>) {
        let cfg = boot_config_from(sel.as_deref());
        // The PMC interior is the HQ the player only occupies once they own it — the HUB resume
        // (between contracts). That is exactly the branch `xQ!L.LoadSingleton` marks `_bPmcRequired`:
        // a save with NO `retry_locations` (no live mid-contract checkpoint). A MID-CONTRACT save
        // (`retry_locations` present) resumes in the WORLD at its checkpoint, and a new game starts in
        // the exterior opening — neither is inside the PMC. Keying `spawn_interior` off `save.is_some()`
        // put every resume in interior collision/atmosphere mode: since the boot-spawn fix now drops a
        // mid-contract save at its exterior checkpoint, that meant the hero had no interior floor tris
        // under them (the terrain heightmap is only consulted when `interior == false`) → fell through
        // the world, under dark indoor fog. `spawn_interior` must track the HUB resume, not any save.
        self.spawn_interior = cfg.save.as_ref().map_or(false, |s| s.retry_locations.is_empty());
        self.recruits = cfg.recruits;
        self.stockpile = cfg.stockpile;
        self.player_models = cfg.models;
        self.hero_character = cfg.hero_character;
        // The save's vz_state world-state overlays drive the streaming catalog (interior/staging/faction
        // layers). Captured BEFORE `cfg.save` is moved into the host below — matching what main.rs
        // computes for the `--stream` free-fly boot (INTERIOR_OVERLAYS is empty, so it's just the save's
        // layers). A new game (`save = None`) streams the base world only.
        self.overlays = cfg.save.as_ref().map(|s| s.layers.clone()).unwrap_or_default();
        // Hand the save (or nothing) to the script host BEFORE the boot flow runs — `run_boot_flow`
        // answers `Pg.LoadGame` from it, and that answer picks the branch.
        self.script_host.borrow_mut().set_boot_save_state(cfg.save);
        println!("[shell] boot: {} [contract {}]", cfg.label, cfg.contract);
    }
}

impl mercs2_engine::app::Game for Mercs2Game {
    type LoadData = WorldData;

    fn config(&self) -> mercs2_engine::app::GameConfig {
        // NOTE: the engine reads this ONCE, before the menu runs (`app::run`), so on a menu boot the
        // interior/exterior branch is not decided yet — `apply_boot` only learns it when a save is
        // picked. `setup` re-applies the branch-correct pair via `Scene::set_fog`/`set_sun`; what is
        // chosen here is just the opening value.
        let (fog, sun) = atmosphere_for(self.spawn_interior);
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
        let overlays = self.overlays.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let r = load_world_data(&wadpath, lc, lp, si, lpr, recruits, &stockpile, &models, &overlays, &progress);
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
        // Re-apply the atmosphere now that the boot branch IS known (see `config`): a new game starts
        // outdoors at the opening contract and must not inherit the interior's sunless dark fog.
        let (fog, sun) = atmosphere_for(self.spawn_interior);
        scene.set_fog(fog.0, fog.1, fog.2);
        if let Some((intensity, ambient)) = sun {
            scene.set_sun(intensity, ambient);
        }
        // ---- Ambient population: load the authored spawner definitions ----
        // Feed the `layers_static` `PopulationSimpleSpawner` COMPs into the runtime's population manager
        // (the realize path — `tick_population` → resolver — is already wired in `fixed_update`). Before
        // this the spawner pool was empty, so no crowds/traffic ever ran. Base-game content, loaded on
        // every boot branch (the spawners live in the always-present base layer).
        let pop_spawners = self.runtime.load_population_spawners(&data.population_block);
        println!("[world] population: registered {pop_spawners} ambient spawners from layers_static");
        // ---- Provisional hero spawn ----
        // The AUTHORITATIVE spawn comes from the boot Lua flow below (the master script picks the start
        // marker; `CreatePlayerCharacter` resolves it through `Pg.GetGuidByName` against the live world).
        // This is only the fallback for a boot where the script host is absent or the flow never reached
        // `Pg.Spawn` — without it the player would sit at the origin. It is NOT the game's answer, so it
        // is deliberately overwritten, not consulted, once the flow has run.
        if let Some(sp) = data.interior_spawn {
            self.player.pos = Vec3::new(sp[0], sp[1] + 2.0, sp[2]);
            println!("[world] provisional spawn: HQ interior (actor + hardpoint) -> ({:.1}, {:.1}, {:.1})", self.player.pos.x, self.player.pos.y, self.player.pos.z);
        }

        // Entity-ize the named world markers into the live World + guidmap (so `Pg.GetGuidByName`
        // resolves real entities), BEFORE the boot Lua flow's CreatePlayerCharacter → GetGuidByName.
        mercs2_engine::script_host::register_named_markers(
            &self.script_host,
            &ctx.world,
            &data.named_locations,
        );
        // The layer → objects index, AFTER the markers: it wakes objects by name through the guidmap,
        // so the entities those names resolve to have to exist first.
        self.script_host.borrow_mut().set_layer_index(data.layer_index.clone());
        // The transit landing pads, AFTER the named markers so a pad that carries a `Name` reuses that
        // one entity instead of creating a second at the same spot.
        mercs2_engine::script_host::register_landing_zones(
            &self.script_host,
            &ctx.world,
            &data.landing_zones,
        );
        if let Some(sh) = &self.script {
            self.script_host.borrow_mut().set_boot_context(self.hero_character.clone());
            mercs2_engine::script_host::run_boot_flow(sh, &self.script_host, &self.hero_character);
            // The game's own answer, unconditionally. `Pg.Spawn(hero, x,y,z)` at the end of
            // `CreatePlayerCharacter` is where retail places the hero, so whatever it produced wins over
            // the provisional guess above — including on the interior boot, where the flow resolves
            // `Pmc_Entry1` against the live marker entity rather than a derived hardpoint.
            match self.script_host.borrow_mut().take_hero_spawn() {
                Some(p) => {
                    self.player.pos = Vec3::new(p[0], p[1], p[2]);
                    println!("[world] hero spawn via boot Lua flow: ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]);
                }
                None => println!(
                    "[world] boot Lua flow produced no hero spawn — keeping the provisional position \
                     ({:.1}, {:.1}, {:.1}). The flow's CreatePlayerCharacter never reached Pg.Spawn.",
                    self.player.pos.x, self.player.pos.y, self.player.pos.z
                ),
            }
        }

        // Base streaming terrain + take ownership of the executor (K2 unification). `into_streaming_world`
        // hands back the low-res per-tile terrain (the streamer hides a tile when its hi-res terrainmesh
        // wakes) and the overlay lights, then yields the `StreamingWorld`. Skipped in --interior: the room
        // sits above the SE terrain peak and the exterior would occlude it, so that boot stays fully
        // static (no streaming) — preserving the interior-orbit debug view.
        let interior_only = std::env::args().any(|a| a == "--interior");
        let (stream_terrain, stream_lights, stream) = data.streaming.into_streaming_world();
        if !interior_only {
            // Fold the overlay-layer point lights (which the static harvest does not cover) into the
            // scene light set before `set_lights` runs below.
            data.lights.extend(stream_lights);
            scene.load_model(
                stream_terrain.hash,
                &stream_terrain.verts,
                &stream_terrain.indices,
                &stream_terrain.draws,
                &stream_terrain.textures,
                &stream_terrain.skin,
            );
            ctx.world.borrow_mut().spawn((
                Transform::IDENTITY,
                ModelRef { model: stream_terrain.hash },
                AnimState::default(),
                SkinPalette { mats: vec![IDENTITY] },
            ));
            self.stream = Some(stream);
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
            self.collision_tris.extend(mercs2_engine::worldutil::mesh_collision_tris(
                &m.verts, &m.indices, off, [0.0, 0.0, 0.0, 1.0],
            ));
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
            self.collision_tris.extend(mercs2_engine::worldutil::mesh_collision_tris(
                &m.verts, &m.indices, pos, quat,
            ));
            let nbones = m.skin.bones.len().max(1);
            ctx.world.borrow_mut().spawn((t, ModelRef { model: m.hash }, AnimState::default(), SkinPalette { mats: vec![IDENTITY; nbones] }));
        }

        // ModelName props (exterior + interior furniture) — each non-water instance blocks → collision.
        let mut prop_meshes = 0usize;
        let mut prop_instances = 0usize;
        let mut prop_destructibles = 0usize;
        for (hash, m, instances) in data.props.into_iter().chain(data.interior_props) {
            scene.load_model(hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            prop_meshes += 1;
            // Destruction: hand this model's state machine + HIER to the store, then give every
            // instance of a machine-bearing model the components that drive it. A prop WITHOUT a
            // machine is simply indestructible and gets neither — that is the engine's own
            // distinction, not a budget decision, and it keeps `Health` off every fence and bollard.
            self.runtime
                .gameplay
                .destruction_store_mut()
                .insert(hash, m.machine.clone(), m.hier.clone());
            let destructible = m.machine.is_some();
            if destructible {
                prop_destructibles += 1;
            }
            let nbones = m.skin.bones.len().max(1);
            for (pos, quat) in instances {
                let tr = Vec3::new(pos[0], pos[1], pos[2]);
                let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                self.collision_tris.extend(mercs2_engine::worldutil::mesh_collision_tris(
                    &m.verts, &m.indices, pos, quat,
                ));
                let mut t = Transform::from_translation(tr);
                t.rotation = q;
                let e = ctx.world.borrow_mut().spawn((t, ModelRef { model: hash }, AnimState::default(), SkinPalette { mats: vec![IDENTITY; nbones] }));
                if destructible {
                    // `Health` is the destruction system's INPUT; `Destructible` is where the machine
                    // keeps its position and the node-enable table the draw gate reads.
                    let _ = ctx.world.borrow_mut().insert(
                        e,
                        (
                            mercs2_core::Health::new(DEFAULT_PROP_HEALTH),
                            mercs2_core::Destructible::default(),
                        ),
                    );
                }
                prop_instances += 1;
            }
        }
        if prop_meshes > 0 {
            println!(
                "[world] props spawned: {prop_meshes} distinct meshes, {prop_instances} instances                  ({prop_destructibles} destructible)"
            );
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

        // ---- Ambient-population NPC templates: make them resident + animatable ----
        // Upload each preloaded faction template to the scene (GPU) + AssetStore (rig + clips), EXACTLY as
        // the player avatar above, then register its render/anim metadata on the runtime. A
        // population-spawned Character carries `ModelRef{template_hash}` (from `spawn_character`); with the
        // model now resident under that hash it draws, and `tick_population` gives it this template's
        // rig-sized bind palette + resident idle so `animation_system` advances + samples it. Reuses the
        // hero's `load_model` / `ModelAnim` / `skin_palette` path — no new loader.
        let mut npc_resident = 0usize;
        let mut npc_confirm_live = 0usize;
        for nt in data.npc_templates {
            let NpcTemplateLoad { model: m, idle_clip, confirm_live } = nt;
            let LoadedModel { hash, verts, indices, draws, textures, skin, clips, .. } = m;
            scene.load_model(hash, &verts, &indices, &draws, &textures, &skin);
            let rig = skin.rig.clone();
            let bind = if rig.is_empty() {
                vec![IDENTITY]
            } else {
                let mp = pose::model_poses(&rig, &pose::bind_qs(&rig));
                pose::skin_palette(&rig, &mp)
            };
            self.store.borrow_mut().models.insert(hash, ModelAnim {
                rig,
                clips: clips.into_iter().map(|c| (c.name_hash, c)).collect(),
            });
            // Hand the runtime this template's bind palette (sized to the rig) + resident idle, so
            // `tick_population` gives each spawned actor of this template a correct `SkinPalette` + clip.
            self.runtime.register_npc_model(hash, bind, idle_clip);
            npc_resident += 1;
            if confirm_live {
                npc_confirm_live += 1;
            }
        }
        println!(
            "[world] preloaded {npc_resident}/7 NPC templates (scene+store+runtime); {npc_confirm_live} idle CONFIRM-LIVE"
        );

        self.hmap = Some(data.hmap);
        self.watermap = data.watermap;
        // The sim system's water mechanism gets the same map. Without this `WaterWorld::tick` is a
        // permanent no-op (it idles on `watermap: None`), so every `Swimmer` in the ECS — NPCs and
        // anything else that floats — stays OnLand no matter how deep it is. The player controller
        // reads the map through `SceneLocomotion` instead; both now see one loaded watermap.
        if let Some(wm) = &self.watermap {
            self.runtime.water.set_watermap(wm.clone());
        }

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
        // Streaming PRE-WARM: step the streamer a bounded number of times at the hero's spawn BEFORE the
        // loading bar clears, so the immediate surroundings (ground/buildings/props) are already resident
        // instead of popping in over the first seconds of play — and so the hero has streamed collision
        // under them the instant control starts. Each step loads a small block/prop budget; stop early
        // once the resident set stops growing (the ring around the spawn is filled). Exterior boots only
        // (`self.stream` is `None` for the interior boot).
        if let Some(sw) = self.stream.as_mut() {
            const PREWARM_MAX_STEPS: usize = 80;
            let cam = self.player.pos.to_array();
            let (mut last, mut stable) = (usize::MAX, 0u32);
            for _ in 0..PREWARM_MAX_STEPS {
                {
                    let mut w = ctx.world.borrow_mut();
                    sw.step(&mut *scene, &mut w, cam);
                }
                let r = sw.stats().resident;
                if r == last {
                    stable += 1;
                    if stable >= 3 {
                        break;
                    }
                } else {
                    stable = 0;
                    last = r;
                }
            }
            println!("[world] streaming pre-warm: {} block(s) resident around spawn", sw.stats().resident);
        }
        // Seed the PERSISTENT broadphase (retail `hkpWorld`): the STATIC baseline (interior shells +
        // furniture) as ONE unit, then the pre-warmed streamed prop/building units applied incrementally
        // (drain the ops the pre-warm steps accumulated). This is the SINGLE structure every consumer reads
        // — the fleet runtime's own fields AND the player/vehicle/camera/weapon via
        // `runtime.gameplay.physics()` — so the hero stands on streamed ground from frame 0 and later
        // WAKE/HIBERNATE deltas mutate it in place (no clone/regrid). See `fixed_update`.
        {
            let phys = self.runtime.gameplay.physics_mut();
            phys.clear();
            phys.insert_unit(STATIC_COLLISION_KEY, &self.collision_tris);
        }
        if let Some(sw) = self.stream.as_mut() {
            let ops = sw.take_collision_ops();
            let phys = self.runtime.gameplay.physics_mut();
            for op in ops {
                match op {
                    mercs2_engine::game_world::CollisionOp::Insert(key, tris) => phys.insert_unit(key, &tris),
                    mercs2_engine::game_world::CollisionOp::Remove(key) => phys.remove_unit(key),
                }
            }
            println!(
                "[world] collision broadphase seeded: {} resident collision tris (static baseline + pre-warmed units)",
                self.runtime.gameplay.physics().tris().len()
            );
        }
        // Projected-decal render node (W5): register the `DecalNode` at the `PassId::Blob` seam so the
        // live decal pool (bullet holes / scorch / blood, spawned by combat impacts) actually draws.
        // `render_prep` feeds it each frame from `self.runtime.decal` via `scene.set_decals`.
        scene.enable_decals();
        if let Some(hm) = self.hmap.as_ref() {
            let phm = hm.to_physics_heightmap();
            println!("[world] terrain heightmap -> fleet physics ({}x{} grid)", phm.width, phm.depth);
            self.runtime.set_heightmap(Some(phm));
        }
        scene.set_lights(std::mem::take(&mut data.lights));
        scene.set_spot_lights(std::mem::take(&mut data.spot_lights));
        // Environmental FX: glow cards (god rays) + particle emitters (fire/smoke/steam).
        {
            let cards = std::mem::take(&mut data.glow_cards);
            let glows = cards.len();
            scene.set_glow_cards(&cards);
            let mut started = 0usize;
            // Emitters were resolved to real (or heuristic-base) descriptors at load (WAD open); just
            // start each at its placement here on the render thread.
            for (desc, pos) in std::mem::take(&mut data.particle_fx) {
                scene.fx_start_desc(desc, pos);
                started += 1;
            }
            if started + glows > 0 {
                println!("[world] particle FX: {started} emitters + {glows} light-shaft glows started");
            }
        }
        if self.start_tps && self.player.entity.is_some() {
            self.mode = CamMode::ThirdPerson;
        }
        self.game_start = std::time::Instant::now();
    }

    fn update(&mut self, ctx: &mut mercs2_engine::app::Ctx) -> mercs2_engine::app::Camera {
        use mercs2_engine::input::Action;
        // ── Live bridge (see `bridge_host`): answer any REPL chunks the worker thread queued since the
        // last frame, HERE on the main thread — the only one that may touch the Lua VM. Collect first
        // so the immutable borrow of `self.bridge` is released before we reach for `self.script`. ──
        let pending = self.bridge.as_ref().map(|b| b.take_pending()).unwrap_or_default();
        for req in pending {
            let out = match &self.script {
                // `exec` runs the chunk in the game's own VM; the reply is what the console sees. A
                // call the reimpl has not bound yet (`Loader.Printf`, an unimplemented `Object.*`)
                // returns its Lua error verbatim — an honest report of what this engine serves today,
                // which grows as the bindings do.
                // Mirror the retail lua-bridge: run the chunk and return its VALUE, stringified — so
                // the console and the A/B harness (`mercs2_repl --ab`) diff actual results, not just
                // "ok". Wrapping in a function lets a chunk `return X` or run as statements; a throw
                // (e.g. an `Object.*` the reimpl hasn't bound) surfaces verbatim, an honest report of
                // what this engine serves today.
                Some(sh) => {
                    let wrapped = format!(
                        "local __f = function() {} end\nlocal __ok, __r = pcall(__f)\nif not __ok then error(__r, 0) end\nreturn tostring(__r)",
                        req.chunk()
                    );
                    match sh.eval::<String>(&wrapped) {
                        Ok(s) => s,
                        Err(e) => format!("error: {e}"),
                    }
                }
                None => "<no script host loaded>".to_string(),
            };
            req.respond(out);
        }
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
            CamMode::Free => self.free_cam.apply_mouse(mdx, mdy),
            CamMode::ThirdPerson => {
                self.tp_yaw += mdx;
                self.tp_pitch = (self.tp_pitch - mdy).clamp(-1.2, 0.6);
            }
        }

        let inp = ctx.input;
        let (gp_yaw, gp_pitch) = inp.look_delta(dt);
        let mut view = match self.mode {
            CamMode::Free => {
                // Arrow-key + gamepad look, accumulated then applied once (clamped) — the engine's
                // fly-cam owns the trig / clamp / movement integration (`FreeCamera`); the game keeps
                // only this input mapping.
                let mut dyaw = gp_yaw;
                let mut dpitch = gp_pitch;
                if inp.keys.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { dpitch += look; }
                if inp.keys.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { dpitch -= look; }
                if inp.keys.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { dyaw -= look; }
                if inp.keys.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { dyaw += look; }
                self.free_cam.add_look(dyaw, dpitch);
                let fwd = self.free_cam.forward();
                let right = fwd.cross(Vec3::Y).normalize();
                let (mx, my) = inp.move_vec();
                let mut mv = fwd * my + right * mx;
                if inp.held(Action::Jump) { mv += Vec3::Y; }
                if inp.held(Action::Crouch) { mv -= Vec3::Y; }
                let sp = if inp.held(Action::Sprint) { 3200.0 } else { 800.0 };
                self.free_cam.translate(mv, sp, dt);
                self.free_cam.view()
            }
            CamMode::ThirdPerson => {
                if inp.keys.contains(&KeyCode::ArrowUp) || inp.kb_held(Action::LookUp) { self.tp_pitch += look; }
                if inp.keys.contains(&KeyCode::ArrowDown) || inp.kb_held(Action::LookDown) { self.tp_pitch -= look; }
                if inp.keys.contains(&KeyCode::ArrowLeft) || inp.kb_held(Action::LookLeft) { self.tp_yaw -= look; }
                if inp.keys.contains(&KeyCode::ArrowRight) || inp.kb_held(Action::LookRight) { self.tp_yaw += look; }
                self.tp_yaw += gp_yaw;
                self.tp_pitch = (self.tp_pitch + gp_pitch).clamp(-1.2, 0.6);
                let (mx, my) = inp.move_vec();
                // ── Vehicle ride (W4). `ridden` is the seated vehicle entity, or `None` on foot.
                // Enter/exit on `Use` (rising edge); while seated, feed input to the vehicle's controls
                // and step the real `mercs2_vehicle` drive sim, then follow with the recovered vehicle
                // camera preset (`for_ridden(Some(class))`). This is the real ride mechanism — it
                // activates whenever a drivable `Vehicle`-tagged entity exists in the ECS. ──
                use mercs2_engine::vehicle::{SeatKind, Vehicle, VehicleClass, VehicleControls};
                let use_edge = inp.held(Action::Use) && !self.use_prev;
                self.use_prev = inp.held(Action::Use);

                // Enter: seat the hero in the nearest usable vehicle within reach (4 m).
                if self.ridden.is_none() && use_edge {
                    if let Some(player_ent) = self.player.entity {
                        let mut w = ctx.world.borrow_mut();
                        let mut best: Option<(Entity, f32)> = None;
                        for (e, (_v, xf)) in w.query::<(&Vehicle, &Transform)>().iter() {
                            let d = xf.translation.distance_squared(self.player.pos);
                            if d < 16.0 && best.map_or(true, |(_, bd)| d < bd) {
                                best = Some((e, d));
                            }
                        }
                        if let Some((veh, _)) = best {
                            if mercs2_engine::vehicle::lua_surface::enter(&mut w, veh, player_ent, SeatKind::Driver).is_some() {
                                self.ridden = Some(veh);
                            }
                        }
                    }
                }

                // Exit: on `Use` while seated, dismount and step out beside the vehicle.
                if let Some(veh) = self.ridden {
                    if use_edge {
                        let mut w = ctx.world.borrow_mut();
                        if let Some(player_ent) = self.player.entity {
                            mercs2_engine::vehicle::lua_surface::exit(&mut w, player_ent);
                        }
                        if let Ok(xf) = w.get::<&Transform>(veh) {
                            self.player.pos = xf.translation + xf.rotation * Vec3::new(2.5, 0.0, 0.0);
                        }
                        drop(w);
                        self.ridden = None;
                    }
                }

                if let Some(veh) = self.ridden {
                    // Drive: input → controls, step the vehicle sim over the collision world, chase-cam.
                    let (veh_pos, class) = {
                        let mut w = ctx.world.borrow_mut();
                        if let Ok(mut c) = w.get::<&mut VehicleControls>(veh) {
                            c.accel = my.max(0.0);
                            c.brake = (-my).max(0.0);
                            c.turn = -mx;
                            c.handbrake = if inp.held(Action::Jump) { 1.0 } else { 0.0 };
                        }
                        let q = TriPhysicsQuery { tris: self.runtime.gameplay.physics().tris() };
                        mercs2_engine::vehicle::drive_step_system(&mut w, &q, &self.veh_lut, dt);
                        let pos = w.get::<&Transform>(veh).map(|x| x.translation).unwrap_or(self.player.pos);
                        let class = w.get::<&Vehicle>(veh).map(|v| v.class).unwrap_or(VehicleClass::Car);
                        (pos, class)
                    };
                    // Keep the hero coupled to the vehicle so exit + other player-pos consumers follow.
                    self.player.pos = veh_pos;
                    let class_name = match class {
                        VehicleClass::Tank => "CameraTank",
                        VehicleClass::Helicopter => "CameraHelicopter",
                        _ => "CameraCarPreset",
                    };
                    let preset = mercs2_engine::camera::CameraMode::for_ridden(Some(class_name)).preset();
                    mercs2_engine::camera::view_with_preset(&preset, veh_pos, self.tp_yaw, self.tp_pitch, self.runtime.gameplay.physics().tris())
                } else {
                    // ── On foot ──
                    let fwd_flat = Vec3::new(self.tp_yaw.sin(), 0.0, self.tp_yaw.cos()).normalize();
                    let right_flat = fwd_flat.cross(Vec3::Y).normalize();
                    let mv = fwd_flat * my + right_flat * mx;
                    // The controller reads the world through `LocomotionQuery` rather than taking the collider,
                    // heightmap and watermap as separate parameters — it lives in `mercs2_player` now, and a
                    // leaf crate cannot name `mercs2_water` or the engine's heightmap. `SceneLocomotion`
                    // borrows all three, so building one per frame is free.
                    let q = mercs2_engine::locomotion::SceneLocomotion {
                        tris: self.runtime.gameplay.physics().tris(),
                        hmap: self.hmap.as_ref(),
                        // Baked hi-res terrain heightfield (retail hkpHeightFieldShape): the resident
                        // `terrainmesh` tiles' near surface, sampled O(1). `None` on the interior boot
                        // (no streamer). This is what the hero stands on outdoors — the terrain tris are
                        // not in the collision broadphase (`runtime.gameplay.physics()`).
                        terrain: self.stream.as_ref().map(|sw| sw.terrain_field()),
                        water: self.watermap.as_ref(),
                        interior: self.spawn_interior,
                    };
                    self.player.update(
                        &mut ctx.world.borrow_mut(),
                        mercs2_engine::player::LocomotionInput {
                            move_dir: mv,
                            sprint: inp.held(Action::Sprint),
                            jump: inp.held(Action::Jump),
                        },
                        &q,
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
                        if let Some(t) = mercs2_engine::physics::broadphase::raycast(self.runtime.gameplay.physics().tris(), eye, aim, PLAYER_WEAPON_RANGE) {
                            let point = eye + aim * t;
                            self.runtime.push_impact(mercs2_engine::combat::Impact::from_hit(point, Vec3::ZERO, aim, false));
                        }
                    }
                    // Mode-based camera: on foot → the RE-pinned `HumanCameraModifier` preset.
                    let preset = mercs2_engine::camera::CameraMode::for_ridden(None).preset();
                    mercs2_engine::camera::view_with_preset(&preset, self.player.pos, self.tp_yaw, self.tp_pitch, self.runtime.gameplay.physics().tris())
                }
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

        let pos = if self.mode == CamMode::Free { self.free_cam.pos } else { self.player.pos };
        // Near/far: on foot use the reflected preset (PMC `SetNearFar(0, 0.3, 500, 0)` from the game's
        // Lua); free-fly/orbit keep the wide far so the whole world stays visible.
        let (near, far) = if self.interior_orbit || self.mode == CamMode::Free {
            (if self.interior_orbit { 1.0 } else { 0.5 }, 30000.0)
        } else if self.ridden.is_some() {
            // Riding → the recovered vehicle preset near/far (CameraCarPreset far f13 = 150).
            let p = mercs2_engine::camera::CameraMode::Car.preset();
            (p.near, p.far)
        } else {
            let p = mercs2_engine::camera::CameraMode::for_ridden(None).preset();
            (p.near, p.far)
        };
        mercs2_engine::app::Camera { view, pos, near, far }
    }

    fn fixed_update(&mut self, ctx: &mut mercs2_engine::app::Ctx) {
        // Streaming tick (K2 unification): decide + execute the LOAD/WAKE/UNLOAD/HIBERNATE diff around
        // the hero, then — only when the streamed collision set actually changed — rebuild the runtime
        // physics collider as the STATIC interior shells MERGED with the streamer's live world-space collider.
        // `animate` poses any woken rigged props (no-op for the clip-less props that dominate).
        let cam = self.player.pos.to_array();
        if let Some(sw) = self.stream.as_mut() {
            {
                let mut w = ctx.world.borrow_mut();
                sw.step(&mut *ctx.scene, &mut w, cam);
            }
            let ops = sw.take_collision_ops();
            if !ops.is_empty() {
                // Apply the per-unit deltas INCREMENTALLY to the ONE persistent broadphase — `insert_unit`
                // on a WAKE, `remove_unit` on a HIBERNATE (retail `hkpWorld::addEntity`/`removeEntity`).
                // The player controller / vehicle / camera / weapon read this same structure via
                // `runtime.gameplay.physics()`, and the fleet uses it natively — so hero and fleet collide
                // with the identical streamed tris, with NO whole-grid clone and NO grid rebuild.
                //
                // Per-delta work is O(the changed units' triangles), not O(all resident tris): a wake
                // inserts only that unit's tris into the cells they overlap; a hibernate removes only that
                // unit's tris from the cells they occupied. Streamed hi-res TERRAIN never enters here (it
                // bakes into the streamer's `terrain_field` heightfield), so moving across terrain is free.
                let (mut ins, mut rem, mut touched) = (0usize, 0usize, 0usize);
                {
                    let phys = self.runtime.gameplay.physics_mut();
                    for op in &ops {
                        match op {
                            mercs2_engine::game_world::CollisionOp::Insert(key, tris) => {
                                ins += 1;
                                touched += tris.len();
                                phys.insert_unit(*key, tris);
                            }
                            mercs2_engine::game_world::CollisionOp::Remove(key) => {
                                rem += 1;
                                phys.remove_unit(*key);
                            }
                        }
                    }
                }
                // Per-delta cost line: work is O(changed units) — `touched` tris across `ins+rem` units —
                // NOT O(resident). `resident` is the full collider size for context (it is NOT re-touched).
                let st = sw.stats();
                println!(
                    "[world] streamed collision Δ: +{ins} / -{rem} unit(s), {touched} tri(s) touched (O(changed)) | resident={} authored tris | authored_phy2={} no_authored={}",
                    self.runtime.gameplay.physics().tris().len(), st.coll_authored, st.coll_no_authored
                );
            }
            sw.animate(&mut ctx.world.borrow_mut(), ctx.time.fixed_dt);
        }
        // Animation (idle/walk/run/swim + crossfade) at the fixed tick.
        // (a) The hero + AnimState-driven rigs, over the AssetStore.
        mercs2_engine::scene::animate_assetstore(
            &mut ctx.world.borrow_mut(), &self.store.borrow(), ctx.time.dt, GAME_ANIM_BLEND_SEC,
        );
        // (b) Spawned population Characters carry the NEWER `AnimController` + `HumanAnimationSet`
        // bundle (attached by `spawn_character`), not `AnimState`. Drive them through the faithful
        // per-entity `animation_system`, reusing the SAME AssetStore rig/clip decode via the adapter
        // above — so an NPC advances + samples its clip into a `SkinPalette` once it carries a model.
        // Picker = None: NPC clip SELECTION (a `ClipPicker` keyed by the AnimationLookup
        // `CharacterName`, plus the model→character mapping) is the render/asset-seam workstream —
        // CONFIRM-LIVE. With None the system still advances + samples whatever clip a controller holds,
        // so the path lights up the moment the render seam attaches a resident `ModelRef` + clip.
        {
            let store = self.store.borrow();
            let assets = StoreAnimAssets::new(&store);
            mercs2_engine::anim::animation_system(
                &mut ctx.world.borrow_mut(), None, &assets, ctx.time.dt,
            );
        }
        // Fleet gameplay (player roster → vehicle/combat/physics/audio) + population, same fixed cadence.
        // The player concern is owned by the script host — Lua is its primary driver — so the tick
        // borrows it rather than holding its own copy. Scoped so the host borrow ends before the
        // mission-Lua pump below takes its own.
        {
            let mut host = self.script_host.borrow_mut();
            self.runtime.tick(&mut ctx.world.borrow_mut(), host.player_mut(), ctx.time.fixed_dt);
        }
        self.runtime.tick_population(&mut ctx.world.borrow_mut(), ctx.time.fixed_dt, self.player.pos);
        // Death → constrained multi-body ragdoll (W6). The weapon/damage pass inside `runtime.tick`
        // lowered `Health` and, on a lethal blast, flagged victims with `combat::Ragdoll` carrying their
        // blast-seed velocity. Here — the GAME layer, which owns the model rigs — snaps
        // `mercs2_physics::ragdoll::Ragdoll::human()` onto each newly-killed rigged character's CURRENT
        // posed skeleton, stops its clip, then steps every live ragdoll against the SAME collision world
        // the fleet uses (`gameplay.physics()`) and writes it back into the `SkinPalette`, so the corpse
        // visibly goes limp and settles. Pure glue over the recovered `death_ragdoll` seam — no new math.
        {
            use mercs2_engine::gameplay::death_ragdoll;
            let dt = ctx.time.fixed_dt;
            let w = ctx.world.borrow_mut();

            // Phase A — collect newly-dead rigged `Ragdollable`s (release the query borrow before we
            // reach into the asset store / mutate the World). `Option<&Ragdoll>` carries the blast seed.
            let mut newly_dead: Vec<(Entity, u32, Vec3, Vec<[[f32; 4]; 4]>)> = Vec::new();
            for (e, (health, _r, mref, skin, seed)) in w
                .query::<(
                    &mercs2_core::Health,
                    &mercs2_engine::combat::Ragdollable,
                    &ModelRef,
                    &SkinPalette,
                    Option<&mercs2_engine::combat::Ragdoll>,
                )>()
                .iter()
            {
                if !health.is_dead() || self.death_ragdolls.contains_key(&e) {
                    continue;
                }
                let seed_vel = seed.map(|r| r.seed_velocity).unwrap_or(Vec3::ZERO);
                newly_dead.push((e, mref.model, seed_vel, skin.mats.clone()));
            }

            // Phase A2 — spawn each ragdoll from the victim's posed skeleton and stop its clip.
            {
                let store = self.store.borrow();
                for (e, model, seed_vel, skin_mats) in newly_dead {
                    let Some(ma) = store.models.get(&model) else { continue };
                    if ma.rig.is_empty() || skin_mats.len() != ma.rig.len() {
                        continue; // unrigged / palette not sized to the rig — not ragdollable here
                    }
                    let Ok(tf) = w.get::<&Transform>(e).map(|t| *t) else { continue };
                    let rig = to_anim_rig(&ma.rig);
                    let model_pose = death_ragdoll::model_pose_from_skin(&rig, &skin_mats);
                    // Snap the ragdoll onto the posed skeleton lifted into WORLD space by the corpse's
                    // Transform, so it simulates against the real world collider; the read-back pulls it back
                    // into the model-space SkinPalette. Stop animation so the ragdoll owns the pose.
                    let Some((rd, work_pose)) =
                        death_ragdoll::spawn(&rig, &model_pose, &tf, seed_vel)
                    else {
                        continue; // a non-human rig missing ragdoll bones — leave it a static corpse
                    };
                    if let Ok(mut a) = w.get::<&mut AnimState>(e) {
                        a.playing = false;
                    }
                    self.death_ragdolls.insert(e, ActiveRagdoll { rd, rig, model_pose: work_pose });
                }
            }

            // Phase B — step every live ragdoll against the shared world collider and write it back to the
            // skin. Settled ragdolls hold their final pose (already in the palette) and are skipped.
            let pq: &dyn mercs2_core::PhysicsQuery = self.runtime.gameplay.physics();
            let mut done: Vec<Entity> = Vec::new();
            for (&e, ar) in self.death_ragdolls.iter_mut() {
                let tf = match w.get::<&Transform>(e) {
                    Ok(t) => *t,
                    Err(_) => {
                        done.push(e); // corpse despawned — drop its ragdoll
                        continue;
                    }
                };
                if ar.rd.settled() {
                    continue;
                }
                death_ragdoll::step_writeback(&mut ar.rd, &ar.rig, &mut ar.model_pose, &tf, pq, dt);
                if let Ok(mut skin) = w.get::<&mut SkinPalette>(e) {
                    death_ragdoll::recompose_skin(&ar.rig, &ar.model_pose, &mut skin.mats);
                }
                if ar.rd.settled() {
                    if let Ok(mut rag) = w.get::<&mut mercs2_engine::combat::Ragdoll>(e) {
                        rag.state = mercs2_engine::combat::RagdollState::Settled;
                    }
                }
            }
            for e in done {
                self.death_ragdolls.remove(&e);
            }
        }
        // Persistent mission-Lua: advance the event/timer system, then realize its runtime Pg.Spawns.
        if let Some(sh) = &self.script {
            mercs2_engine::script_host::pump_resident(sh, &self.script_host, ctx.time.fixed_dt);
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
        // Destruction side effects. The leaf crate RECORDS these (it can neither spawn entities nor
        // drive FX); this is where they happen. Both are positioned at the destroyed entity's own
        // transform — retail resolves a hardpoint for emitters (`FUN_004D28C0`), which we do not have
        // yet, so the object origin is a documented approximation, not the engine's placement.
        for intent in self.runtime.gameplay.take_destruction_intents() {
            let at = ctx
                .world
                .borrow()
                .get::<&Transform>(intent.entity)
                .map(|t| t.translation)
                .unwrap_or(Vec3::ZERO);
            match intent.kind {
                mercs2_engine::destruction::IntentKind::StartEmitter => {
                    ctx.scene.fx_start_desc(
                        mercs2_engine::particles::EmitterDesc::impact_fire(),
                        at.to_array(),
                    );
                }
                mercs2_engine::destruction::IntentKind::CreateObject => {
                    // Debris — a shed hood, a blown-off turret. `CreateObject` carries five
                    // arguments and the template slot is not confirmed; `template()` returns the
                    // last, the only one observed to vary per spawn. We can only realize it if that
                    // model is already resident; a not-yet-streamed template is counted rather than
                    // silently dropped, because "debris quietly missing" is exactly the kind of gap
                    // that reads as working.
                    match intent.template() {
                        Some(tpl) if ctx.scene.has_model(tpl) => {
                            let t = Transform::from_translation(at);
                            ctx.world.borrow_mut().spawn((t, ModelRef { model: tpl }));
                        }
                        _ => self.debris_unresident += 1,
                    }
                }
            }
        }

        // Projected decals (W5): feed this frame's live decal pool into the `DecalNode` registered in
        // `setup`. Each `DecalInstance` (bullet hole / scorch / blood, spawned + aged by the combat
        // runtime) becomes a `DecalDraw` — hit point + surface basis + footprint + category + fade —
        // that the node projects as a depth-tested oriented quad. Empty when nothing has been shot yet.
        let decals: Vec<mercs2_engine::scene::DecalDraw> = self
            .runtime
            .decal
            .iter_live()
            .map(|d| mercs2_engine::scene::DecalDraw {
                position: d.position.to_array(),
                normal: d.normal.to_array(),
                tangent: d.tangent.to_array(),
                size: d.size,
                category: decal_category_index(d.def_key),
                alpha: d.alpha(),
            })
            .collect();
        ctx.scene.set_decals(&decals);

        // Directional shadow key light, centred on the player (overhead indoors, sun-aligned outdoors).
        let shadow_dir = if self.spawn_interior { [-0.15, -1.0, 0.1] } else { [-0.4, -0.7, 0.5] };
        ctx.scene.set_shadow(self.player.pos.to_array(), shadow_dir, 18.0);
    }
}
