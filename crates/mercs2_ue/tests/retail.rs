//! The library against the retail game, through the C interface an Unreal plugin calls.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.

use mercs2_ue::data::{self, Placement, Section, Vertex};
use mercs2_ue::ffi;
use mercs2_ue::space;
use std::ffi::{CStr, CString};

/// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else.
fn vz_wad_path() -> String {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    path.to_str().unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display())).to_string()
}

fn last_error() -> String {
    unsafe { CStr::from_ptr(ffi::m2ue_last_error()) }.to_string_lossy().into_owned()
}

/// The game opened through the C interface, closed on drop.
struct Game(*mut data::Game);

impl Game {
    fn open() -> Game {
        let path = CString::new(vz_wad_path()).unwrap();
        let g = unsafe { ffi::m2ue_open(path.as_ptr()) };
        assert!(!g.is_null(), "m2ue_open: {}", last_error());
        Game(g)
    }

    fn mesh(&self, hash: u32, terrain: bool) -> Mesh {
        let m = unsafe { if terrain { ffi::m2ue_terrain_cell_load(self.0, hash) } else { ffi::m2ue_model_load(self.0, hash) } };
        assert!(!m.is_null(), "0x{hash:08X}: {}", last_error());
        Mesh(m)
    }

    fn stand_in(&self, hash: u32) -> Mesh {
        let m = unsafe { ffi::m2ue_stand_in_load(self.0, hash) };
        assert!(!m.is_null(), "stand-in 0x{hash:08X}: {}", last_error());
        Mesh(m)
    }

    fn placements(&self, layer: u32) -> Vec<Placement> {
        let l = unsafe { ffi::m2ue_layer_load(self.0, layer) };
        assert!(!l.is_null(), "layer 0x{layer:08X}: {}", last_error());
        let mut n = 0usize;
        let p = unsafe { ffi::m2ue_layer_placements(l, &mut n) };
        let out = if n == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(p, n) }.to_vec() };
        unsafe { ffi::m2ue_layer_free(l) };
        out
    }

    /// Every layer with the base name of the block that ships it.
    fn layers(&self) -> Vec<(data::LayerInfo, String)> {
        let l = unsafe { ffi::m2ue_layers(self.0) };
        assert!(!l.is_null(), "m2ue_layers: {}", last_error());
        let mut n = 0usize;
        let p = unsafe { ffi::m2ue_layers_infos(l, &mut n) };
        let infos = if n == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(p, n) }.to_vec() };
        let out = infos
            .into_iter()
            .enumerate()
            .map(|(i, info)| {
                let block = unsafe { ffi::m2ue_layers_block(l, i) };
                assert!(!block.is_null(), "layer {i}: {}", last_error());
                (info, unsafe { CStr::from_ptr(block) }.to_string_lossy().into_owned())
            })
            .collect();
        assert!(unsafe { ffi::m2ue_layers_block(l, n) }.is_null(), "a block name past the list");
        unsafe { ffi::m2ue_layers_free(l) };
        out
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        unsafe { ffi::m2ue_close(self.0) }
    }
}

/// A mesh from the C interface, freed on drop.
struct Mesh(*mut data::Mesh);

impl Mesh {
    fn vertices(&self) -> &[Vertex] {
        let mut n = 0usize;
        let p = unsafe { ffi::m2ue_mesh_vertices(self.0, &mut n) };
        if n == 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n) } }
    }

    fn indices(&self) -> &[u32] {
        let mut n = 0usize;
        let p = unsafe { ffi::m2ue_mesh_indices(self.0, &mut n) };
        if n == 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n) } }
    }

    fn sections(&self) -> &[Section] {
        let mut n = 0usize;
        let p = unsafe { ffi::m2ue_mesh_sections(self.0, &mut n) };
        if n == 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n) } }
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        unsafe { ffi::m2ue_mesh_free(self.0) }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Of the triangles with area and a normal: how many have `cross(b - a, c - a)` opposing the summed
/// vertex normals (Unreal's front face), and how many there are.
fn front_facing(m: &Mesh) -> (usize, usize) {
    let (v, mut opposing, mut total) = (m.vertices(), 0usize, 0usize);
    for t in m.indices().as_chunks::<3>().0 {
        let [a, b, c] = [v[t[0] as usize], v[t[1] as usize], v[t[2] as usize]];
        let face = cross(sub(b.position, a.position), sub(c.position, a.position));
        let normal = [0, 1, 2].map(|k| a.normal[k] + b.normal[k] + c.normal[k]);
        let d = dot(face, normal);
        if d == 0.0 {
            continue;
        }
        total += 1;
        if d < 0.0 {
            opposing += 1;
        }
    }
    (opposing, total)
}

const MODELS: [&str; 4] = ["pmcoutpost_interior_hq", "ch_veh_tank_ztz98", "pmc_hum_mattias", "global_moneylargea"];

#[test]
fn model_triangles_face_the_way_unreal_draws() {
    let game = Game::open();
    for name in MODELS {
        let m = game.mesh(data::hash(name), false);
        let (opposing, total) = front_facing(&m);
        println!("{name}: {opposing} of {total} triangles front-facing for Unreal");
        assert!(total > 0, "{name}: no triangle has area and a normal");
        assert!(opposing * 100 >= total * 95, "{name}: only {opposing} of {total} triangles face the way Unreal draws");
    }
}

#[test]
fn a_model_matches_the_engines_own_assembly() {
    let game = Game::open();
    let mut wad = mercs2_engine::wad::open(&vz_wad_path()).unwrap();
    for name in MODELS {
        let hash = data::hash(name);
        let m = game.mesh(hash, false);
        let native = mercs2_engine::model::Model::load(&mut wad, hash).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (verts, indices, draws, stats) = native.flatten();
        assert_eq!((m.vertices().len(), m.indices().len(), m.sections().len()), (verts.len(), indices.len(), draws.len()), "{name}");
        assert_eq!(unsafe { ffi::m2ue_mesh_lod_count(m.0) }, native.lod_count(), "{name}");
        // A vertex every one of whose bones rests at identity is only converted, bit for bit.
        const IDENT: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let palette = stats.skin_data().rest_palette(&verts).unwrap();
        let mut unposed = 0usize;
        for (got, v) in m.vertices().iter().zip(&verts) {
            let rests = (0..4).all(|k| v.weights[k] == 0 || palette[v.joints[k] as usize] == IDENT);
            if rests && v.weights.iter().filter(|&&w| w != 0).count() == 1 {
                assert_eq!(got.position, space::position(v.pos), "{name}");
                unposed += 1;
            }
            assert_eq!(got.uv, v.uv, "{name}");
        }
        println!("{name}: {} vertices, {unposed} resting at identity, {} sections", verts.len(), draws.len());
        for (s, d) in m.sections().iter().zip(&draws) {
            assert_eq!((s.index_start, s.index_count, s.lod_mask, s.node, s.rung), (d.index_start, d.index_count, d.lod_mask, d.node, d.rung), "{name}");
            assert_eq!(&s.textures[..s.texture_count as usize], d.textures.as_slice(), "{name}");
        }
        let (mut lo, mut hi) = ([0.0f32; 3], [0.0f32; 3]);
        assert_eq!(unsafe { ffi::m2ue_mesh_bounds(m.0, lo.as_mut_ptr(), hi.as_mut_ptr()) }, 1);
        assert!((0..3).all(|k| lo[k] <= hi[k]), "{name}: bounds {lo:?}..{hi:?}");
    }
}

/// A Merida house (`0xA1A48E75`, three tiers): each tier's switch distance is the header's reference
/// distance through the selector, and tier 0 has none.
#[test]
fn a_model_gives_its_tier_switch_distances() {
    let game = Game::open();
    let hash = 0xA1A4_8E75;
    let m = game.mesh(hash, false);
    let mut wad = mercs2_engine::wad::open(&vz_wad_path()).unwrap();
    let native = mercs2_engine::model::Model::load(&mut wad, hash).unwrap();
    let header = native.header.expect("the house has a model header");
    assert_eq!(unsafe { ffi::m2ue_mesh_lod_count(m.0) }, 3);
    let mut last = 0.0f32;
    for tier in 1..3u32 {
        let d = unsafe { ffi::m2ue_mesh_lod_switch_distance(m.0, tier) };
        println!("0x{hash:08X}: reference {} m, tier {tier} from {} m", header.lod_distance, d / space::UNITS_PER_METRE);
        let metres = d / space::UNITS_PER_METRE;
        assert_eq!(u32::from(mercs2_engine::render_state::lod_rung(metres * 1.001, header.lod_distance, data::MAIN_VIEW_LOD_FACTOR, 0, 3)), tier);
        assert_eq!(u32::from(mercs2_engine::render_state::lod_rung(metres * 0.999, header.lod_distance, data::MAIN_VIEW_LOD_FACTOR, 0, 3)), tier - 1);
        assert!(d > last);
        last = d;
    }
    assert_eq!(unsafe { ffi::m2ue_mesh_lod_switch_distance(m.0, 0) }, -1.0);
    assert!(last_error().contains("tier 0"), "{}", last_error());
    assert_eq!(unsafe { ffi::m2ue_mesh_lod_switch_distance(m.0, 3) }, -1.0);
}

/// A section's maps are the slots its pixel shader declares a sampler for, as the engine's own
/// `material_slots` reads them: the tank has sections with a normal map and a specular mask, and the
/// maps are always slots 0, 1 and 2 of the section's own textures.
#[test]
fn a_section_names_the_maps_its_shader_samples() {
    let game = Game::open();
    for name in MODELS {
        let m = game.mesh(data::hash(name), false);
        let (mut normal, mut specular, mut emissive) = (0usize, 0usize, 0usize);
        for s in m.sections() {
            let slot = |i: usize| if i < s.texture_count as usize { s.textures[i] } else { 0 };
            assert!(s.diffuse_map == 0 || s.diffuse_map == slot(0), "{name}");
            assert!(s.slot1_map == 0 || s.slot1_map == slot(1), "{name}");
            assert!(s.normal_map == 0 || s.normal_map == slot(2), "{name}");
            match s.slot1_role {
                data::SLOT1_NONE => assert_eq!(s.slot1_map, 0, "{name}"),
                data::SLOT1_SPECULAR => specular += usize::from(s.slot1_map != 0),
                data::SLOT1_EMISSIVE => emissive += usize::from(s.slot1_map != 0),
                r => panic!("{name}: slot 1 role {r}"),
            }
            normal += usize::from(s.normal_map != 0);
        }
        println!("{name}: {} sections, {normal} with a normal map, {specular} with a specular mask, {emissive} with an emissive map", m.sections().len());
    }
    let tank = game.mesh(data::hash("ch_veh_tank_ztz98"), false);
    assert!(tank.sections().iter().any(|s| s.normal_map != 0 && s.slot1_role == data::SLOT1_SPECULAR && s.slot1_map != 0));
}

/// The hall's wreck-free state: the draws retail shows as the entity activates are a strict subset.
#[test]
fn a_destructible_model_marks_what_it_shows_undamaged() {
    let game = Game::open();
    let m = game.mesh(data::hash("ch_veh_tank_ztz98"), false);
    let enabled = m.sections().iter().filter(|s| s.node_enabled == 1).count();
    println!("ch_veh_tank_ztz98: {enabled} of {} sections enabled at activation", m.sections().len());
    assert!(enabled > 0 && enabled < m.sections().len());
}

#[test]
fn a_texture_ships_its_whole_mip_chain() {
    let game = Game::open();
    let m = game.mesh(data::hash("ch_veh_tank_ztz98"), false);
    let hash = m.sections().iter().find(|s| s.texture_count > 0).expect("the tank has a textured section").textures[0];
    let t = unsafe { ffi::m2ue_texture_load(game.0, hash) };
    assert!(!t.is_null(), "texture 0x{hash:08X}: {}", last_error());
    let mut info = ffi::TextureInfo::default();
    assert_eq!(unsafe { ffi::m2ue_texture_info(t, &mut info) }, 1);
    let mut size = 0usize;
    let p = unsafe { ffi::m2ue_texture_data(t, &mut size) };
    assert!(!p.is_null());
    let block = match info.format {
        data::TEXTURE_BC1 => 8,
        data::TEXTURE_BC3 => 16,
        f => panic!("format {f}"),
    };
    let want: usize = (0..info.mip_count)
        .map(|mip| {
            let (w, h) = ((info.width >> mip).max(1), (info.height >> mip).max(1));
            (w.div_ceil(4) * h.div_ceil(4)) as usize * block
        })
        .sum();
    println!("texture 0x{hash:08X}: {}x{} format {} mips {} bytes {size}", info.width, info.height, info.format, info.mip_count);
    assert_eq!(size, want);
    // The decoded top mip is whole and is not one flat colour.
    let mut rgba_size = 0usize;
    let rgba = unsafe { ffi::m2ue_texture_rgba(t, &mut rgba_size) };
    assert_eq!(rgba_size, (info.width * info.height * 4) as usize);
    let px = unsafe { std::slice::from_raw_parts(rgba, rgba_size) };
    assert!(px.as_chunks::<4>().0.iter().any(|p| p[..] != px[..4]), "every decoded texel equals the first");
    unsafe { ffi::m2ue_texture_free(t) };
}

/// `mrxhq.lua` spawns the `HqInterior` actor at (3750, 450, -3840); the layer's furniture stands around it.
#[test]
fn the_pmc_interior_layer_places_its_furniture_around_the_hall() {
    let game = Game::open();
    let placements = game.placements(data::hash("Vz_State_PmcInterior"));
    let models: Vec<&Placement> = placements.iter().filter(|p| p.kinds & data::PLACEMENT_MODEL != 0).collect();
    println!("Vz_State_PmcInterior: {} entities, {} with a model", placements.len(), models.len());
    assert!(!models.is_empty());
    let hall = space::position([3750.0, 450.0, -3840.0]);
    assert_eq!(hall, [375_000.0, -384_000.0, 45_000.0]);
    for p in &models {
        let off = sub(p.position, hall);
        assert!(off.iter().all(|c| c.abs() < 20_000.0), "entity 0x{:08X} at {:?} is not within 200 m of the hall", p.key, p.position);
        let n: f32 = p.rotation.iter().map(|c| c * c).sum();
        assert!((n - 1.0).abs() < 1e-3, "entity 0x{:08X} rotation {:?}", p.key, p.rotation);
    }
    // Every model the layer places is shipped and assembles; a made-up hash is not shipped.
    let mut distinct: Vec<u32> = models.iter().map(|p| p.model).collect();
    distinct.sort_unstable();
    distinct.dedup();
    for h in &distinct {
        assert_eq!(unsafe { ffi::m2ue_model_exists(game.0, *h) }, 1, "model 0x{h:08X}");
        game.mesh(*h, false);
    }
    assert_eq!(unsafe { ffi::m2ue_model_exists(game.0, 0xDEAD_BEEF) }, 0);
    assert_eq!(ffi::m2ue_length(1.5), 150.0);
    println!("Vz_State_PmcInterior: {} distinct models assemble", distinct.len());
}

#[test]
fn every_layer_reads_and_a_terrain_cell_is_a_ground_mesh() {
    let game = Game::open();
    let layers = game.layers();
    assert!(!layers.is_empty());
    // The interior is a state overlay in its own block; the always-there world is `layers_static`.
    let interior = layers.iter().find(|(i, _)| i.hash == data::hash("Vz_State_PmcInterior")).expect("the interior layer is listed");
    assert_eq!((interior.0.kind, interior.1.as_str()), (data::LAYER_STATE_OVERLAY, "vz_state_pmcinterior"));
    let of = |kind: u32| layers.iter().filter(|(i, _)| i.kind == kind).count();
    println!("layers: {} static, {} state overlay, {} other", of(data::LAYER_STATIC), of(data::LAYER_STATE_OVERLAY), of(data::LAYER_OTHER));
    assert!(of(data::LAYER_STATIC) > 0 && of(data::LAYER_STATE_OVERLAY) > 0);
    assert!(layers.iter().filter(|(i, _)| i.kind == data::LAYER_STATIC).all(|(_, b)| b.to_lowercase().contains("layers_static")));
    let (mut entities, mut lights, mut cell) = (0usize, 0usize, None);
    let (mut stand_ins, mut merida_stand_in) = (0usize, None);
    let mut wake: std::collections::HashMap<u32, f32> = Default::default();
    for l in layers.iter().map(|(i, _)| i.hash) {
        for p in game.placements(l) {
            entities += 1;
            assert!(p.wake_radius > 0.0, "entity 0x{:08X} wakes at {}", p.key, p.wake_radius);
            wake.insert(p.key, p.wake_radius);
            if p.kinds & data::PLACEMENT_TINY_STAND_IN != 0 {
                stand_ins += 1;
                assert_eq!(p.kinds & data::PLACEMENT_MODEL, 0, "stand-in 0x{:08X} is also a model", p.key);
                // `vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece`: row 12, column 29 of the 200 m grid.
                if p.key == 0x0014_4ECE {
                    merida_stand_in = Some(p);
                }
            }
            if p.kinds & data::PLACEMENT_LIGHT != 0 {
                lights += 1;
                assert!(p.light_range > 0.0, "light 0x{:08X} has range {}", p.key, p.light_range);
            }
            if p.kinds & data::PLACEMENT_TERRAIN_CELL != 0 && cell.is_none() {
                cell = Some(p);
            }
        }
    }
    println!("{} layers, {entities} entities, {lights} lights, {stand_ins} TINY stand-ins", layers.len());
    // A stand-in sits unrotated at its cell's centre, (-3900 + 200·col, 0, -3900 + 200·row).
    let s = merida_stand_in.expect("the Merida stand-in 0x00144ECE is placed");
    assert_eq!(s.model, data::hash("vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece"));
    assert_eq!(s.position, space::position([-3900.0 + 200.0 * 29.0, 0.0, -3900.0 + 200.0 * 12.0]));
    assert_eq!(s.rotation, [0.0, 0.0, 0.0, 1.0]);
    // An entity with no HibernationControl of its own or its template's has Distance 100, and wakes
    // inside trunc(1.2 x 100) m.
    // The soldier kit 0x4AB6F9B6 is placed by character templates only; the interior's furniture is props.
    let mut soldier = [0usize; 3];
    for (i, _) in &layers {
        for p in game.placements(i.hash).iter().filter(|p| p.kinds & data::PLACEMENT_MODEL != 0 && p.model == 0x4AB6_F9B6) {
            soldier[p.archetype as usize] += 1;
        }
    }
    println!("model 0x4AB6F9B6 placements by archetype [prop, vehicle, character]: {soldier:?}");
    assert!(soldier[data::ARCHETYPE_CHARACTER as usize] > 0 && soldier[data::ARCHETYPE_PROP as usize] == 0 && soldier[data::ARCHETYPE_VEHICLE as usize] == 0);
    assert!(game.placements(data::hash("Vz_State_PmcInterior")).iter().filter(|p| p.kinds & data::PLACEMENT_MODEL != 0).all(|p| p.archetype == data::ARCHETYPE_PROP));
    let radii: std::collections::BTreeSet<u32> = wake.values().map(|r| *r as u32).collect();
    println!("wake radii: {} distinct, {} cm .. {} cm", radii.len(), radii.first().unwrap(), radii.last().unwrap());
    assert!(radii.contains(&12_000), "no entity wakes at the default 120 m");
    // The stand-in's sections each stand in for one placed world object, in one of its two states.
    let m = game.stand_in(s.model);
    let (mut objects, mut intact, mut ruined) = (std::collections::BTreeSet::new(), 0usize, 0usize);
    for sec in m.sections() {
        assert!(wake.contains_key(&sec.object), "stand-in section for 0x{:08X}, which no layer places", sec.object);
        assert!(sec.ruined <= 1 && sec.index_count >= 3);
        objects.insert(sec.object);
        if sec.ruined == 1 { ruined += 1 } else { intact += 1 }
    }
    println!("stand-in 0x{:08X}: {} vertices, {} sections ({intact} intact, {ruined} ruined) for {} objects", s.model, m.vertices().len(), m.sections().len(), objects.len());
    assert!(intact > 0 && objects.len() > 1);
    // Relative to its placement, a stand-in stays about its 200 m cell.
    for v in m.vertices() {
        assert!(v.position[0].abs() <= 15_000.0 && v.position[1].abs() <= 15_000.0, "stand-in vertex at {:?}", v.position);
    }
    let (opposing, total) = front_facing(&m);
    println!("stand-in: {opposing} of {total} triangles front-facing for Unreal");
    assert!(opposing * 100 >= total * 95);
    let cell = cell.expect("some layer places a terrain cell");
    assert!(cell.terrain_fade[0] < cell.terrain_fade[1], "fade {:?}", cell.terrain_fade);
    let m = game.mesh(cell.terrain_cell, true);
    let ground = m.sections().iter().filter(|s| s.kind == data::SECTION_GROUND).count();
    println!("terrain cell 0x{:08X}: {} vertices, {} sections, {ground} ground", cell.terrain_cell, m.vertices().len(), m.sections().len());
    assert!(ground > 0);
    // A cell is 400 m square and cell-local.
    for v in m.vertices() {
        assert!(v.position[0].abs() <= 20_100.0 && v.position[1].abs() <= 20_100.0, "vertex at {:?}", v.position);
    }
    let (opposing, total) = front_facing(&m);
    println!("terrain cell: {opposing} of {total} triangles front-facing for Unreal");
    assert!(opposing * 100 >= total * 95);
}

/// The atmosphere settings retail ships: 56 records on the regions of `vz_atmosphere_regions` and the
/// global `default` in the `worldentity`. A settled capture is one of them: the PMC capture's light
/// block (`time_0.51`) is the ambient faces, rim colour and sun direction of a shipped record, and its
/// light colour is that record's within the sun-height tint. Merida, where the Unreal slice is, lies
/// in a region.
#[test]
fn the_atmosphere_settings_give_the_captured_light() {
    use mercs2_ue::atmosphere::{captures, Settings, GLOBAL_ENTITY};
    let mut game = data::Game::open(&vz_wad_path()).unwrap();
    let settings = Settings::read(&mut game).unwrap();
    let default = data::hash("default");
    let placed = settings.settings.iter().filter(|s| s.entity < 0x8000_0000).count();
    println!(
        "{} settings ({placed} placed), {} regions, {} entities with a default",
        settings.settings.len(),
        settings.regions.len(),
        settings.settings.iter().filter(|s| s.name == default).count()
    );
    assert_eq!(placed, 56);
    assert!(settings.regions.len() >= 30 && settings.regions.iter().all(|r| r.polygon.len() >= 3));
    assert!(settings.settings.iter().any(|s| s.entity == GLOBAL_ENTITY && s.name == default));
    // Every record reads as a light block, with the sun above the horizon and no negative light.
    let blocks: Vec<_> = settings.settings.iter().map(|s| s.atmosphere().unwrap_or_else(|e| panic!("0x{:08X}: {e}", s.entity))).collect();
    for a in &blocks {
        assert!(a.toward_sun[2] >= 0.0 && a.light_color.iter().chain(a.ambient.iter().flatten()).all(|v| *v >= 0.0), "{a:?}");
    }
    // The PMC capture: taken settled (a second capture, 20261021_mattias-nilson-jacket-skin, holds
    // the same block).
    let noon = captures().unwrap().iter().find(|c| c.name.to_str().unwrap() == "time_0.51").unwrap().atmosphere;
    let close = |a: &[f32; 3], b: &[f32; 3], tolerance: f32| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tolerance);
    let matches: Vec<_> = blocks
        .iter()
        .zip(&settings.settings)
        .filter(|(a, _)| (a.time - noon.time).abs() < 1e-4 && (0..6).all(|f| close(&a.ambient[f], &noon.ambient[f], 2e-5)))
        .collect();
    println!("records equal to the time_0.51 capture's ambient faces: {:08X?}", matches.iter().map(|(_, s)| (s.entity, s.name)).collect::<Vec<_>>());
    let (a, _) = matches.first().expect("no shipped record has the PMC capture's ambient faces");
    assert!(close(&a.rim_color, &noon.rim_color, 2e-5), "rim {:?} vs {:?}", a.rim_color, noon.rim_color);
    assert!(close(&a.toward_sun, &noon.toward_sun, 1e-3), "sun {:?} vs {:?}", a.toward_sun, noon.toward_sun);
    assert!((a.mie_g - noon.mie_g).abs() < 1e-5);
    let tint: Vec<f32> = (0..3).map(|c| noon.light_color[c] / a.light_color[c]).collect();
    println!("captured light colour / record's: {tint:?}");
    assert!(tint.iter().all(|t| (0.8..=1.12).contains(t)), "{tint:?}");
    // The slice's centre, game (2000, -1400): a region, with a default setting, through the C interface.
    let merida = settings.entity_at(2000.0, -1400.0);
    println!("the entity at game (2000, -1400): 0x{merida:08X}");
    assert_ne!(merida, GLOBAL_ENTITY);
    let through = Game::open();
    let (at, name) = ([2000.0f32, 0.0, -1400.0], CString::new("default").unwrap());
    let mut out = blocks[0];
    assert_eq!(unsafe { ffi::m2ue_atmosphere_at(through.0, at.as_ptr(), name.as_ptr(), &mut out) }, 1, "{}", last_error());
    assert_eq!(out, settings.at(2000.0, -1400.0, "default").unwrap().atmosphere().unwrap());
    println!("Merida default: time {}, sun {:?}, light {:?}, ambient face 2 {:?}", out.time, out.toward_sun, out.light_color, out.ambient[2]);
    let missing = CString::new("no_such_setting").unwrap();
    assert_eq!(unsafe { ffi::m2ue_atmosphere_at(through.0, at.as_ptr(), missing.as_ptr(), &mut out) }, 0);
    assert!(last_error().contains("no setting \"no_such_setting\""), "{}", last_error());
}
