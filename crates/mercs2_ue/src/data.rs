//! The game's data, read by the engine's own decoders and converted to Unreal's space.
//!
//! Nothing here parses a game format. A model is [`mercs2_engine::model::Model`], a layer goes through
//! [`mercs2_engine::worldutil::PlacedCatalog`], a terrain cell through
//! [`mercs2_engine::mesh::build_terrain_cell`], a texture through the asset stack. This module poses
//! the result at rest, converts it with [`crate::space`], and lays it out in the `#[repr(C)]` records
//! `include/mercs2_ue.h` declares.

use crate::space;
use mercs2_engine::asset::AssetSource;
use mercs2_engine::mesh::{self, DrawGroup};
use mercs2_engine::model::Model;
use mercs2_engine::render::{material_slots, Slot1Role};
use mercs2_engine::render_state::RenderState;
use mercs2_formats::shader_registrations::PixelSamplers;
use mercs2_engine::worldutil::{NodeSource, PlacedCatalog, TemplateRecords, LAYER_TYPE_HASH, TERRAINMESH_TYPE_HASH};
use mercs2_formats::placement::LightObject;
use mercs2_formats::terrainmesh::TerrainFade;
use mercs2_formats::texture::TexFormat;
use mercs2_formats::types::TYPE_ID_LAYER;

/// Texture slots a material record carries (`Mtrl_Parse` `FUN_00858790` copies them into a 10-slot
/// array at material `+0x144`).
pub const MATERIAL_TEXTURE_SLOTS: usize = 10;

/// One vertex, in Unreal's space, posed at rest.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    /// The bitangent itself, not a sign: `cross(normal, tangent) · tangent.w` in the game's space (the
    /// engine's vertex shader), then converted like any direction.
    pub bitangent: [f32; 3],
    /// As authored: V = 0 is the top row of the texture.
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// The parameters of a terrain ground draw (`PgTerrainMeshVP*`/`PgTerrainMeshFP*`), in Unreal's space.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// The cell's own map, stretched over the whole cell.
    pub global: u32,
    /// The shared detail map; meaningful when `has_detail` is 1.
    pub detail: u32,
    /// The four detail layers' diffuse maps; layer `n` is meaningful when bit `n` of `layer_mask` is set.
    pub layers: [u32; 4],
    pub has_detail: u8,
    pub layer_mask: u8,
    pub _pad: [u8; 2],
    /// Each layer's planar projection: an Unreal WORLD position `P` (centimetres) samples layer `n` at
    /// `(layer_uv[n][0] · (P, 1), layer_uv[n][1] · (P, 1))`.
    pub layer_uv: [[[f32; 4]; 2]; 4],
    /// The ground material's UV scale (`mercs2_formats::terrainmesh::Material::uv_scale`), per
    /// centimetre where the game's is per metre.
    pub uv_scale: [f32; 2],
}

/// The alpha an alpha-tested material's pixel must exceed, of 255 (`ALPHAREF`).
pub const ALPHA_REF: u8 = mercs2_formats::texture::MATERIAL_ALPHA_REF;

/// A section's material kind.
pub const SECTION_MATERIAL: u8 = 0;
pub const SECTION_GROUND: u8 = 1;

/// One draw: a run of triangles with one material.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub index_start: u32,
    pub index_count: u32,
    /// The LOD tiers this draw is drawn at, bit `n` = tier `n` (`SEGM.state_mask`, after a finer block
    /// has taken over the tiers it re-authors). A terrain cell's draws are `0xFF`.
    pub lod_mask: u8,
    /// 1 when the draw's node is enabled as the entity activates with no saved destruction state
    /// (`mercs2_formats::render_node::activation_enable`); 0 for wreckage and break pieces.
    pub node_enabled: u8,
    /// The LOD block the geometry came from: 0 is the resident (coarsest) block.
    pub rung: u8,
    /// [`SECTION_MATERIAL`] or [`SECTION_GROUND`].
    pub kind: u8,
    /// The `HIER` node the draw hangs from; negative for none.
    pub node: i16,
    /// The material's on-disk `flags@104` word (`mercs2_formats::texture::runtime_material_flags`).
    pub material_flags: u16,
    /// 1 when the colour pass alpha-tests the material (`mercs2_formats::texture::material_alpha_tested`),
    /// against [`ALPHA_REF`].
    pub alpha_tested: u8,
    /// 1 when the material draws blended (`mercs2_formats::texture::material_blended`).
    pub blended: u8,
    /// On a stand-in's section: 1 for the part drawn while [`Section::object`] is ruined, 0 for the
    /// part drawn while it is intact. 0 elsewhere.
    pub ruined: u8,
    /// What [`Section::slot1_map`] is to the pixel shader: [`SLOT1_NONE`], [`SLOT1_SPECULAR`] or
    /// [`SLOT1_EMISSIVE`].
    pub slot1_role: u8,
    /// `pandemic_hash_m2` of the material's pixel shader name; 0 for no material.
    pub shader_key: u32,
    /// On a stand-in's section: the entity key of the world object the section stands in for. 0
    /// elsewhere.
    pub object: u32,
    /// The texture the pixel shader samples as `diffuseMap`; 0 when it samples none. These three are
    /// the slots of [`Section::textures`] the shader declares a sampler for
    /// (`mercs2_engine::render::material_slots`); all 0 on a ground section.
    pub diffuse_map: u32,
    /// The texture the shader samples at stage 1: the highlight mask (`specularMap`) or a map added
    /// after lighting, times the emissive constant (`emissiveMap`), as [`Section::slot1_role`] says.
    pub slot1_map: u32,
    /// The texture the shader samples as `normalMap`: tangent-space, X in alpha, Y in green, Z left to
    /// reconstruct. 0 when it samples none.
    pub normal_map: u32,
    /// Slot 0 diffuse, 1 specular, 2 normal, then whatever else the shader binds.
    pub texture_count: u32,
    pub textures: [u32; MATERIAL_TEXTURE_SLOTS],
    /// Pixel constants c93..c98: diffuse, specular, ambient, emissive, subsurface, constant0.
    pub material_data: [[f32; 4]; 6],
    /// Meaningful when `kind` is [`SECTION_GROUND`].
    pub ground: Ground,
}

/// A model or a terrain cell: one vertex buffer, one index buffer, its draws.
#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Triangle list.
    pub indices: Vec<u32>,
    pub sections: Vec<Section>,
    /// LOD tiers the model declares (header `+0x34`); 1 for a terrain cell.
    pub lod_count: u32,
    /// The model's LOD reference distance (header `+0x38`, the runtime's `M+0x84`), in Unreal units;
    /// 0 for a mesh of one tier.
    pub lod_distance: f32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// What the main camera's view passes the LOD selector (`FUN_00470740`) as the factor on a model's
/// reference distance: the float the selector's caller (`FUN_0047724e`, `0x0047739d`) loads from
/// `view + 0xe70`. Read from retail memory: `0x3F2C092D` at `0x017D17F0` (view 1, the one with the
/// live eye) in five v1.0 captures, at 640×480 and at 1600×900, and cached with the viewer distance in
/// a render object beside the camera (`OBJ + 0x368 + 0x24`). View 0's is 1.0. No writer of the field
/// is known, so what changes it (a scoped weapon's zoom is a candidate) is not either.
pub const MAIN_VIEW_LOD_FACTOR: f32 = f32::from_bits(0x3F2C_092D);

impl Mesh {
    /// The distance from the main camera, in Unreal units, at which the game starts drawing tier
    /// `tier` in place of the tier before it. The selector takes `x = distance / (reference × factor)`
    /// and picks tier `floor(log2 x) + 1`, held to the model's tiers, so tier `n` starts where `x`
    /// reaches `2^(n-1)`. Tier 0 has no such distance: it is what is drawn nearer than tier 1's.
    ///
    /// The runtime also holds a lowest tier (`M+0x80`) the selector never goes below; its source in
    /// the files is not known, it read 0 on every model sampled live, and it is taken as 0 here.
    pub fn lod_switch_distance(&self, tier: u32) -> Result<f32, String> {
        if tier == 0 || tier >= self.lod_count {
            return Err(format!("tier {tier} of a mesh of {} tiers has no switch distance", self.lod_count));
        }
        if self.lod_distance.is_nan() || self.lod_distance <= 0.0 {
            return Err(format!("a mesh of {} tiers has the reference distance {}", self.lod_count, self.lod_distance));
        }
        Ok(self.lod_distance * MAIN_VIEW_LOD_FACTOR * 2f32.powi(tier as i32 - 1))
    }
}

/// The pixel shader samples nothing at stage 1.
pub const SLOT1_NONE: u8 = 0;
/// Stage 1 is the shader's `specularMap`.
pub const SLOT1_SPECULAR: u8 = 1;
/// Stage 1 is the shader's `emissiveMap`.
pub const SLOT1_EMISSIVE: u8 = 2;

/// Block compression of a texture's bytes.
pub const TEXTURE_BC1: u32 = 1;
pub const TEXTURE_BC3: u32 = 3;

/// A texture: its whole mip chain, largest first, as the game stores it.
#[derive(Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// [`TEXTURE_BC1`] (DXT1) or [`TEXTURE_BC3`] (DXT5).
    pub format: u32,
    pub mip_count: u32,
    pub data: Vec<u8>,
    /// The largest mip decoded to 8-bit RGBA, `width · height · 4` bytes, row-major from the top row.
    pub rgba: Vec<u8>,
}

/// The first mip of a block-compressed chain, decoded (`mercs2_formats::texture_encode::decode_dxt_surface`).
fn decode_top_mip(width: u32, height: u32, format: TexFormat, chain: &[u8]) -> Result<Vec<u8>, String> {
    let (w, h) = (width as usize, height as usize);
    let (wb, hb) = (w.div_ceil(4), h.div_ceil(4));
    let texels = mercs2_formats::texture_encode::decode_dxt_surface(format.fourcc(), wb, hb, chain)?;
    // The decoder returns whole blocks; a size that is not a multiple of 4 keeps its top-left part.
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            out.extend_from_slice(&texels[y * wb * 4 + x]);
        }
    }
    Ok(out)
}

/// Which block ships a layer. [`LayerInfo::kind`].
pub const LAYER_OTHER: u32 = 0;
/// A `layers_static` block: the world that is always there.
pub const LAYER_STATIC: u32 = 1;
/// A `vz_state_*` block: an overlay the game's scripts load for a world state or a mission.
pub const LAYER_STATE_OVERLAY: u32 = 2;

/// One layer of the mounted archives.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerInfo {
    pub hash: u32,
    /// [`LAYER_STATIC`], [`LAYER_STATE_OVERLAY`] or [`LAYER_OTHER`].
    pub kind: u32,
}

/// A layer and the base name of the block that ships it (`layers_static`, `vz_state_pmcinterior`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub info: LayerInfo,
    pub block: String,
}

/// What a placed entity is drawn as. Bits of [`Placement::kinds`].
pub const PLACEMENT_MODEL: u32 = 1 << 0;
pub const PLACEMENT_TERRAIN_CELL: u32 = 1 << 1;
pub const PLACEMENT_LOW_RES_TILE: u32 = 1 << 2;
pub const PLACEMENT_SCRUB: u32 = 1 << 3;
pub const PLACEMENT_LIGHT: u32 = 1 << 4;
/// Registered static in the hibernation grid (`PlacedEntity::is_static`).
pub const PLACEMENT_STATIC: u32 = 1 << 5;
/// A TINY stand-in (`TinyGeometryObject`): the far-distance model of the world objects of one 200 m
/// cell. The game draws an object's part of it only while that object is not itself drawn, so it is
/// never [`PLACEMENT_MODEL`]; [`Placement::model`] holds the stand-in's model.
pub const PLACEMENT_TINY_STAND_IN: u32 = 1 << 6;

/// A light's kind.
pub const LIGHT_POINT: u32 = LightObject::TYPE_POINT;
pub const LIGHT_SPOT: u32 = LightObject::TYPE_SPOT;

/// One placed entity of a layer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub key: u32,
    /// `Transform.Parent`: the worldentity template, 0 for none.
    pub template_key: u32,
    /// `PLACEMENT_*` bits; each hash below is meaningful only when its bit is set.
    pub kinds: u32,
    /// Its own `ModelName`, else its template's: with [`PLACEMENT_MODEL`] the model it is drawn as,
    /// with [`PLACEMENT_TINY_STAND_IN`] the stand-in's model.
    pub model: u32,
    /// Its `TerrainObject` terrainmesh.
    pub terrain_cell: u32,
    /// Its `LowResTerrainObject` tile mesh.
    pub low_res_tile: u32,
    /// Its `ScrubObject` ground cover.
    pub scrub: u32,
    pub position: [f32; 3],
    /// Quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    /// With [`PLACEMENT_TERRAIN_CELL`]: centimetres from the camera where the detail layers start to
    /// fade into the global map and where they are gone, then the same for the shared detail map, then
    /// the centimetres one tile of the shared detail map covers.
    pub terrain_fade: [f32; 5],
    /// With [`PLACEMENT_LIGHT`]: [`LIGHT_POINT`] or [`LIGHT_SPOT`].
    pub light_kind: u32,
    /// Linear RGB.
    pub light_color: [f32; 3],
    /// The game's own intensity value (`LightObject` `params[0]`), not converted to any Unreal unit.
    pub light_intensity: f32,
    /// Reach in centimetres: `max(range · range scale, 0.01 m)`, as the light's render node takes it.
    pub light_range: f32,
    /// A spot's inner cone half-angle, radians (`params[4]`).
    pub light_cone_inner: f32,
    /// A spot's full cone angle, radians: `clamp(angle · cone scale, 0, π)`.
    pub light_cone_angle: f32,
    /// A spot's axis, a unit direction in world space.
    pub light_axis: [f32; 3],
    /// How near a viewer must come, on the ground plane, for the game to wake the entity and draw it,
    /// in centimetres: the hibernation grid's radius for its `HibernationControl.Distance` in single
    /// player (`mercs2_core::hibernation::HibernationScale::radius`). Beyond it plus a 7 m band the
    /// entity sleeps and only its part of a TINY stand-in is drawn.
    pub wake_radius: f32,
    /// What the entity's template makes it ([`ARCHETYPE_PROP`], [`ARCHETYPE_VEHICLE`] or
    /// [`ARCHETYPE_CHARACTER`]): the physics component the template carries
    /// (`mercs2_engine::worldutil::TemplateRecords::archetypes`).
    pub archetype: u32,
}

/// A rendered prop: a template with no vehicle or character physics component, or no template.
pub const ARCHETYPE_PROP: u32 = 0;
/// A drivable vehicle.
pub const ARCHETYPE_VEHICLE: u32 = 1;
/// A person. Its model is a kit whose visible parts an outfit selects, on a skeleton an animation
/// drives; posed at rest with no outfit it draws nothing.
pub const ARCHETYPE_CHARACTER: u32 = 2;

/// The game, opened: the WAD stack around a level archive.
pub struct Game {
    pub(crate) assets: AssetSource,
    /// Read from the `worldentity` at the first layer.
    templates: Option<TemplateRecords>,
    /// What each retail pixel shader samples, from `shader3.bin` beside the level archive.
    samplers: PixelSamplers,
    /// The atmosphere settings, read at the first [`Game::atmosphere_at`].
    pub(crate) atmosphere: Option<crate::atmosphere::Settings>,
}

impl Game {
    /// Mount the retail WAD stack around the level archive at `level_wad` (`vz.wad`), as
    /// [`AssetSource::discover`] does, and read the pixel shaders' samplers from the `shader3.bin`
    /// beside it.
    pub fn open(level_wad: &str) -> Result<Game, String> {
        let assets = AssetSource::discover(level_wad, &[])?;
        let samplers = mercs2_engine::render::pixel_samplers(std::path::Path::new(level_wad))?;
        Ok(Game { assets, templates: None, samplers, atmosphere: None })
    }

    /// Whether the level archive ships a model under this asset hash. A placed entity can name one it
    /// does not ship, and the game then draws nothing for it.
    pub fn has_model(&self, hash: u32) -> bool {
        mercs2_engine::wad::has_model(self.assets.base(), hash)
    }

    /// A model by asset hash, every LOD block of its chain, posed at rest. Read from the level archive.
    pub fn model(&mut self, hash: u32) -> Result<Mesh, String> {
        let what = |e: String| format!("model 0x{hash:08X}: {e}");
        let model = Model::load(self.assets.base_mut(), hash).map_err(what)?;
        let enable = mercs2_formats::render_node::activation_enable(&model.resident).map_err(what)?;
        let state = RenderState { lod: 0, view_state: 0xFF, node_enable: enable };
        let (verts, indices, draws, stats) = model.flatten();
        let palette = stats.skin_data().rest_palette(&verts).map_err(what)?;
        let vertices = rest_vertices(&verts, &palette).map_err(what)?;
        let sections = draws
            .iter()
            .map(|d| section(d, state.node_visible(d.node), maps(d, &self.samplers)?))
            .collect::<Result<Vec<_>, _>>()
            .map_err(what)?;
        let mut mesh = finish(vertices, &indices, sections, model.lod_count()).map_err(what)?;
        if let Some(header) = &model.header {
            mesh.lod_distance = space::length(header.lod_distance);
        }
        Ok(mesh)
    }

    /// A hi-res terrain cell by its terrainmesh hash, cell-local: the entity that places it gives its
    /// position ([`Placement::position`]). Read from the level archive.
    pub fn terrain_cell(&mut self, hash: u32) -> Result<Mesh, String> {
        let what = |e: String| format!("terrainmesh 0x{hash:08X}: {e}");
        let container = mercs2_engine::wad::extract_container_typed(self.assets.base_mut(), hash, TERRAINMESH_TYPE_HASH).map_err(what)?;
        // The fade is the placing entity's and is not read from the cell; the builder only stores it.
        let (verts, indices, draws) = mesh::build_terrain_cell(&container, [0.0; 3], TerrainFade::default()).map_err(what)?;
        let palette = mesh::SkinData::identity().rest_palette(&verts).map_err(what)?;
        let vertices = rest_vertices(&verts, &palette).map_err(what)?;
        let sections = draws.iter().map(|d| section(d, true, maps(d, &self.samplers)?)).collect::<Result<Vec<_>, _>>().map_err(what)?;
        finish(vertices, &indices, sections, 1).map_err(what)
    }

    /// A TINY stand-in by its model hash, relative to its placement: the far-distance model of the
    /// world objects of one 200 m cell, one section per object, state and material. A section's
    /// [`Section::object`] is the entity the game draws it for, while that entity is not itself drawn;
    /// [`Section::ruined`] says which of its two states the section is. Read from the level archive.
    pub fn stand_in(&mut self, hash: u32) -> Result<Mesh, String> {
        use mercs2_formats::tiny_model::{TinyModel, NODE_PRISTINE, NODE_RUIN};
        let what = |e: String| format!("stand-in 0x{hash:08X}: {e}");
        let container = mercs2_engine::wad::extract_container(self.assets.base_mut(), hash).map_err(what)?;
        let model = TinyModel::decode(&container).map_err(what)?;
        let half = |h: u16| mercs2_formats::model_inject::read_f16_le(&h.to_le_bytes(), 0);
        let (pristine, ruin) = (crate::data::hash(NODE_PRISTINE), crate::data::hash(NODE_RUIN));
        let mut vertices = Vec::new();
        // (object, ruined, material) -> its triangles.
        let mut parts: std::collections::BTreeMap<(u32, bool, usize), Vec<u32>> = Default::default();
        for (si, sub) in model.sub_objects.iter().enumerate() {
            let node = model.nodes.get(sub.node as usize).ok_or_else(|| what(format!("sub-object {si} hangs from node {}, which the model lacks", sub.node)))?;
            let ruined = match node.name_hash {
                h if h == pristine => false,
                h if h == ruin => true,
                h => return Err(what(format!("sub-object {si} hangs from node 0x{h:08X}, neither pristine nor ruin"))),
            };
            for (gi, g) in sub.groups.iter().enumerate() {
                let at = |e: String| what(format!("sub-object {si} group {gi}: {e}"));
                let base = vertices.len() as u32;
                let mut objects = Vec::with_capacity(g.vertices.len());
                for v in &g.vertices {
                    let slot = half(v.position[3]);
                    let object = *model.slots.get(slot as usize).filter(|_| slot >= 0.0 && slot.fract() == 0.0).ok_or_else(|| at(format!("a vertex names slot {slot} of {}", model.slots.len())))?;
                    objects.push(object);
                    vertices.push(Vertex {
                        position: space::position([half(v.position[0]), half(v.position[1]), half(v.position[2])]),
                        normal: space::direction([half(v.normal[0]), half(v.normal[1]), half(v.normal[2])]),
                        tangent: [0.0; 3],
                        bitangent: [0.0; 3],
                        uv: [half(v.uv[0]), half(v.uv[1])],
                        color: [1.0; 4],
                    });
                }
                let material = g.prims.first().ok_or_else(|| at("no primitive record".into()))?.material as usize;
                if material >= model.materials.len() {
                    return Err(at(format!("material {material} of {}", model.materials.len())));
                }
                // The strip as triangles: a repeated index is a stitch, odd positions are re-wound.
                for k in 0..g.strip.len().saturating_sub(2) {
                    let (a, b, c) = (g.strip[k], g.strip[k + 1], g.strip[k + 2]);
                    if a == b || b == c || a == c {
                        continue;
                    }
                    let t = if k % 2 == 0 { [a, b, c] } else { [b, a, c] };
                    let object = *objects.get(t[0] as usize).ok_or_else(|| at(format!("the strip names vertex {} of {}", t[0], objects.len())))?;
                    for &i in &t {
                        if objects.get(i as usize) != Some(&object) {
                            return Err(at(format!("a triangle's vertices belong to different objects (vertex {i})")));
                        }
                    }
                    parts.entry((object, ruined, material)).or_default().extend(t.map(|i| base + u32::from(i)));
                }
            }
        }
        let (mut indices, mut sections) = (Vec::new(), Vec::new());
        for ((object, ruined, material), tris) in parts {
            let m = &model.materials[material];
            let d = DrawGroup {
                index_start: indices.len() as u32,
                index_count: tris.len() as u32,
                textures: m.textures.clone(),
                material_flags: m.flags,
                shader_key: m.pixel_shader,
                ..Default::default()
            };
            indices.extend(tris);
            let mut s = maps(&d, &self.samplers).and_then(|m| section(&d, true, m)).map_err(what)?;
            s.object = object;
            s.ruined = u8::from(ruined);
            sections.push(s);
        }
        finish(vertices, &indices, sections, 1).map_err(what)
    }

    /// A texture by asset hash, with its streamed mips where the archives ship them.
    pub fn texture(&mut self, hash: u32) -> Result<Texture, String> {
        let t = self.assets.extract_texture_hires(hash)?;
        let rgba = decode_top_mip(t.width, t.height, t.format, &t.all_mips).map_err(|e| format!("texture 0x{hash:08X}: {e}"))?;
        Ok(Texture {
            rgba,
            width: t.width,
            height: t.height,
            format: match t.format {
                TexFormat::Bc1 => TEXTURE_BC1,
                TexFormat::Bc3 => TEXTURE_BC3,
            },
            mip_count: t.mip_count,
            data: t.all_mips,
        })
    }

    /// Every layer the mounted archives carry, ascending by name hash, each with the block that ships
    /// it as [`mercs2_formats::world_index::WorldIndex`] catalogues that block. A layer two archives
    /// carry is the later-mounted one's, as the WAD stack resolves it.
    pub fn layers(&self) -> Result<Vec<Layer>, String> {
        use mercs2_formats::world_index::{BlockClass, WorldIndex};
        let mut layers = std::collections::BTreeMap::new();
        for path in self.assets.paths() {
            let mut f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
            let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
            let archive = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
            let index = WorldIndex::build(&archive, &mut f);
            for e in archive.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER) {
                let block = index
                    .block(e.block_index())
                    .ok_or_else(|| format!("{path}: layer 0x{:08X} names block {} of {}", e.asset_hash, e.block_index(), index.len()))?;
                let kind = match block.class {
                    BlockClass::LayersStatic => LAYER_STATIC,
                    BlockClass::VzStateOverlay => LAYER_STATE_OVERLAY,
                    _ => LAYER_OTHER,
                };
                layers.insert(e.asset_hash, Layer { info: LayerInfo { hash: e.asset_hash, kind }, block: block.name.clone() });
            }
        }
        Ok(layers.into_values().collect())
    }

    /// Every placed entity of a layer, by layer name hash, in data order. A model, light or fade an
    /// entity does not carry itself comes from its worldentity template.
    pub fn layer(&mut self, hash: u32) -> Result<Vec<Placement>, String> {
        let what = |e: String| format!("layer 0x{hash:08X}: {e}");
        if self.templates.is_none() {
            self.templates = Some(TemplateRecords::load(&mut self.assets)?);
        }
        let container = self
            .assets
            .ecs_container(LAYER_TYPE_HASH, hash)
            .map_err(what)?
            .ok_or_else(|| format!("layer 0x{hash:08X}: in no open wad ({:?})", self.assets.paths()))?;
        let templates = self.templates.as_ref().expect("loaded above");
        let mut catalog = PlacedCatalog::default();
        let keys = catalog.add_block(&container, templates).map_err(what)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let o = catalog.get(key).expect("add_block returned the key");
            let mut p = Placement {
                key: o.key,
                template_key: o.parent,
                kinds: 0,
                model: 0,
                terrain_cell: 0,
                low_res_tile: 0,
                scrub: 0,
                position: space::position(o.pos),
                rotation: space::rotation(o.quat),
                terrain_fade: [0.0; 5],
                light_kind: 0,
                light_color: [0.0; 3],
                light_intensity: 0.0,
                light_range: 0.0,
                light_cone_inner: 0.0,
                light_cone_angle: 0.0,
                light_axis: [0.0; 3],
                wake_radius: {
                    let r = o.registration();
                    space::length(f32::from(mercs2_core::hibernation::HibernationScale::single_player().radius(r.distance, r.is_static)))
                },
                archetype: match templates.archetypes.get(&o.parent) {
                    None | Some(mercs2_engine::spawn::Archetype::Prop) => ARCHETYPE_PROP,
                    Some(mercs2_engine::spawn::Archetype::Vehicle(_)) => ARCHETYPE_VEHICLE,
                    Some(mercs2_engine::spawn::Archetype::Character) => ARCHETYPE_CHARACTER,
                },
            };
            if o.is_static {
                p.kinds |= PLACEMENT_STATIC;
            }
            if let Some(m) = o.model {
                let tiny = matches!(o.node, NodeSource::Model { tiny_geometry: true, .. });
                p.kinds |= if tiny { PLACEMENT_TINY_STAND_IN } else { PLACEMENT_MODEL };
                p.model = m;
            }
            if let Some(t) = o.terrain_mesh {
                p.kinds |= PLACEMENT_TERRAIN_CELL;
                p.terrain_cell = t;
                let f = templates.terrain_fade.get(&o.parent).copied().unwrap_or_default();
                p.terrain_fade = [f.global_start, f.global_end, f.detail_start, f.detail_end, f.detail_size].map(space::length);
            }
            if let Some(t) = o.low_res_mesh {
                p.kinds |= PLACEMENT_LOW_RES_TILE;
                p.low_res_tile = t;
            }
            if let Some(s) = o.scrub {
                p.kinds |= PLACEMENT_SCRUB;
                p.scrub = s;
            }
            if let NodeSource::Light(light, scales) = o.node {
                if light.light_type != LIGHT_POINT && light.light_type != LIGHT_SPOT {
                    return Err(what(format!("entity 0x{key:08X}: LightObject type {} is neither point nor spot", light.light_type)));
                }
                p.kinds |= PLACEMENT_LIGHT;
                p.light_kind = light.light_type;
                p.light_color = light.color;
                p.light_intensity = light.intensity();
                p.light_range = space::length((light.radius() * scales.range).max(0.01));
                p.light_cone_inner = light.cone_inner();
                p.light_cone_angle = (light.params[5] * scales.cone).clamp(0.0, std::f32::consts::PI);
                let placed = mercs2_formats::placement::PlacedLight { key: o.key, name: None, pos: o.pos, quat: o.quat, light, sub_block: 0 };
                p.light_axis = space::direction(placed.cone_axis());
            }
            out.push(p);
        }
        Ok(out)
    }
}

/// The world layers a save restores: its `tLayerData`, the names `MrxLayerManager.SaveSingleton`
/// wrote, in the save's order. They are the layers the game's scripts had loaded on top of the
/// static world when the save was made; `MrxLayerManager.LoadSingleton` loads them again.
pub fn save_layers(profile: &std::path::Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(profile).map_err(|e| format!("{}: {e}", profile.display()))?;
    let save = mercs2_formats::save::parse(&bytes).map_err(|e| format!("{}: {e}", profile.display()))?;
    Ok(save.save_state().map_err(|e| format!("{}: {e}", profile.display()))?.layers)
}

/// `pandemic_hash_m2` of an asset, layer or shader name.
pub fn hash(name: &str) -> u32 {
    mercs2_formats::hash::pandemic_hash_m2(name)
}

/// Pose `verts` with `palette` the way the engine's vertex shader does (`shader.wgsl` `vs_main`: linear
/// blend of row-vector bone matrices, weights normalised by their sum), and convert the result.
fn rest_vertices(verts: &[mesh::Vertex], palette: &[[[f32; 4]; 4]]) -> Result<Vec<Vertex>, String> {
    let point = |m: &[[f32; 4]; 4], v: [f32; 3]| [0, 1, 2].map(|c| v[0] * m[0][c] + v[1] * m[1][c] + v[2] * m[2][c] + m[3][c]);
    let vector = |m: &[[f32; 4]; 4], v: [f32; 3]| [0, 1, 2].map(|c| v[0] * m[0][c] + v[1] * m[1][c] + v[2] * m[2][c]);
    let mut out = Vec::with_capacity(verts.len());
    for (i, v) in verts.iter().enumerate() {
        let total: f32 = v.weights.iter().map(|&w| f32::from(w)).sum();
        if total <= 0.0 {
            return Err(format!("vertex {i} has no blend weight"));
        }
        let (mut pos, mut normal, mut tangent) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
        for k in 0..4 {
            let w = f32::from(v.weights[k]) / total;
            if w <= 0.0 {
                continue;
            }
            let m = palette
                .get(v.joints[k] as usize)
                .ok_or_else(|| format!("vertex {i} names joint {} of a {}-entry palette", v.joints[k], palette.len()))?;
            let (p, n, t) = (point(m, v.pos), vector(m, v.normal), vector(m, [v.tangent[0], v.tangent[1], v.tangent[2]]));
            for c in 0..3 {
                pos[c] += w * p[c];
                normal[c] += w * n[c];
                tangent[c] += w * t[c];
            }
        }
        let (normal, tangent) = (unit(normal), unit(tangent));
        let bitangent = cross(normal, tangent).map(|c| c * v.tangent[3]);
        out.push(Vertex {
            position: space::position(pos),
            normal: space::direction(normal),
            tangent: space::direction(tangent),
            bitangent: space::direction(bitangent),
            uv: v.uv,
            color: v.color,
        });
    }
    Ok(out)
}

/// `v` at unit length; a zero vector stays zero.
fn unit(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 0.0 {
        v.map(|c| c / len)
    } else {
        v
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The textures a draw's pixel shader samples, by the sampler it declares for each.
#[derive(Clone, Copy, Default)]
struct Maps {
    diffuse: u32,
    slot1: u32,
    normal: u32,
    slot1_role: u8,
}

/// A draw's [`Maps`]. A ground draw has none here: its slots are its terrain shader's, in
/// [`Section::ground`].
fn maps(d: &DrawGroup, samplers: &PixelSamplers) -> Result<Maps, String> {
    if d.ground.is_some() {
        return Ok(Maps::default());
    }
    let slots = material_slots(d, samplers)?;
    let slot1_role = match slots.slot1_role {
        Slot1Role::None => SLOT1_NONE,
        Slot1Role::Specular => SLOT1_SPECULAR,
        Slot1Role::Emissive => SLOT1_EMISSIVE,
    };
    Ok(Maps { diffuse: slots.diffuse.unwrap_or(0), slot1: slots.slot1.unwrap_or(0), normal: slots.normal.unwrap_or(0), slot1_role })
}

fn section(d: &DrawGroup, node_enabled: bool, maps: Maps) -> Result<Section, String> {
    if d.textures.len() > MATERIAL_TEXTURE_SLOTS {
        return Err(format!("a material binds {} textures; a material record holds {MATERIAL_TEXTURE_SLOTS}", d.textures.len()));
    }
    if d.scrub.is_some() {
        return Err("a ground cover draw in a model or terrain cell".into());
    }
    let mut textures = [0u32; MATERIAL_TEXTURE_SLOTS];
    textures[..d.textures.len()].copy_from_slice(&d.textures);
    let Maps { diffuse: diffuse_map, slot1: slot1_map, normal: normal_map, slot1_role } = maps;
    let ground = match &d.ground {
        Some(g) => {
            // A row is applied to a game position (metres); the same sample from an Unreal position
            // (centimetres, Y and Z exchanged) takes the row with those two coefficients exchanged and
            // the three of them per centimetre.
            let row = |r: [f32; 4]| {
                let d = space::direction([r[0], r[1], r[2]]).map(|c| c / space::UNITS_PER_METRE);
                [d[0], d[1], d[2], r[3]]
            };
            let mut layers = [0u32; 4];
            let mut layer_mask = 0u8;
            for (n, l) in g.layers.iter().enumerate() {
                if let Some(h) = l {
                    layers[n] = *h;
                    layer_mask |= 1 << n;
                }
            }
            Ground {
                global: g.global,
                detail: g.detail.unwrap_or(0),
                layers,
                has_detail: u8::from(g.detail.is_some()),
                layer_mask,
                _pad: [0; 2],
                layer_uv: g.layer_uv.map(|uv| uv.map(row)),
                uv_scale: g.uv_scale.map(|s| s / space::UNITS_PER_METRE),
            }
        }
        None => Ground { global: 0, detail: 0, layers: [0; 4], has_detail: 0, layer_mask: 0, _pad: [0; 2], layer_uv: [[[0.0; 4]; 2]; 4], uv_scale: [0.0; 2] },
    };
    Ok(Section {
        index_start: d.index_start,
        index_count: d.index_count,
        lod_mask: d.lod_mask,
        node_enabled: u8::from(node_enabled),
        rung: d.rung,
        kind: if d.ground.is_some() { SECTION_GROUND } else { SECTION_MATERIAL },
        node: d.node,
        material_flags: d.material_flags,
        alpha_tested: u8::from(mercs2_formats::texture::material_alpha_tested(d.material_flags)),
        blended: u8::from(mercs2_formats::texture::material_blended(d.material_flags)),
        ruined: 0,
        slot1_role,
        shader_key: d.shader_key,
        object: 0,
        diffuse_map,
        slot1_map,
        normal_map,
        texture_count: d.textures.len() as u32,
        textures,
        material_data: d.material_data,
        ground,
    })
}

/// Check the buffers against each other, order every triangle for Unreal, and take the bounds.
fn finish(vertices: Vec<Vertex>, indices: &[u32], sections: Vec<Section>, lod_count: u32) -> Result<Mesh, String> {
    let (triangles, rest) = indices.as_chunks::<3>();
    if !rest.is_empty() {
        return Err(format!("{} indices are not whole triangles", indices.len()));
    }
    if let Some(i) = indices.iter().find(|&&i| i as usize >= vertices.len()) {
        return Err(format!("index {i} is past the {} vertices", vertices.len()));
    }
    for (n, s) in sections.iter().enumerate() {
        let end = u64::from(s.index_start) + u64::from(s.index_count);
        if s.index_count % 3 != 0 || end > indices.len() as u64 {
            return Err(format!("section {n} covers indices {}..{end} of {}", s.index_start, indices.len()));
        }
    }
    let indices: Vec<u32> = triangles.iter().flat_map(|t| space::triangle(*t)).collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &vertices {
        for c in 0..3 {
            lo[c] = lo[c].min(v.position[c]);
            hi[c] = hi[c].max(v.position[c]);
        }
    }
    if vertices.is_empty() {
        (lo, hi) = ([0.0; 3], [0.0; 3]);
    }
    Ok(Mesh { vertices, indices, sections, lod_count, lod_distance: 0.0, bounds_min: lo, bounds_max: hi })
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENT: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

    fn vert(pos: [f32; 3], joints: [u8; 4], weights: [u8; 4]) -> mesh::Vertex {
        mesh::Vertex { pos, color: [1.0; 4], uv: [0.25, 0.75], normal: [0.0, 1.0, 0.0], tangent: [1.0, 0.0, 0.0, -1.0], joints, weights }
    }

    #[test]
    fn an_identity_palette_only_converts() {
        let v = rest_vertices(&[vert([1.0, 2.0, 3.0], [0; 4], [255, 0, 0, 0])], &[IDENT]).unwrap();
        assert_eq!(v[0].position, [100.0, 300.0, 200.0]);
        assert_eq!(v[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(v[0].tangent, [1.0, 0.0, 0.0]);
        // cross(+Y, +X) = -Z in the game's space, times w = -1 is +Z, which is Unreal's +Y.
        assert_eq!(v[0].bitangent, [0.0, 1.0, 0.0]);
        assert_eq!(v[0].uv, [0.25, 0.75]);
    }

    #[test]
    fn weights_blend_row_vector_bone_matrices() {
        let mut lifted = IDENT;
        lifted[3] = [0.0, 4.0, 0.0, 1.0];
        let v = rest_vertices(&[vert([1.0, 0.0, 0.0], [0, 1, 0, 0], [51, 204, 0, 0])], &[IDENT, lifted]).unwrap();
        // 20% unmoved + 80% lifted 4 m: y = 3.2 m, which is Unreal's Z in centimetres.
        assert!((v[0].position[2] - 320.0).abs() < 1e-3, "{:?}", v[0].position);
        assert_eq!(v[0].position[0], 100.0);
    }

    /// A switch distance is where the engine's port of the selector (`render_state::lod_rung`) steps
    /// to that tier, given the main view's factor.
    #[test]
    fn a_switch_distance_is_where_the_selector_steps_to_the_tier() {
        use mercs2_engine::render_state::lod_rung;
        let reference = 30.0f32;
        let mut mesh = finish(Vec::new(), &[], Vec::new(), 4).unwrap();
        mesh.lod_distance = space::length(reference);
        for tier in 1..4u32 {
            let metres = mesh.lod_switch_distance(tier).unwrap() / space::UNITS_PER_METRE;
            assert_eq!(u32::from(lod_rung(metres * 1.001, reference, MAIN_VIEW_LOD_FACTOR, 0, 4)), tier);
            assert_eq!(u32::from(lod_rung(metres * 0.999, reference, MAIN_VIEW_LOD_FACTOR, 0, 4)), tier - 1);
        }
        assert!(mesh.lod_switch_distance(0).unwrap_err().contains("tier 0"));
        assert!(mesh.lod_switch_distance(4).unwrap_err().contains("tier 4"));
        let one = finish(Vec::new(), &[], Vec::new(), 1).unwrap();
        assert!(one.lod_switch_distance(1).is_err());
        let unset = finish(Vec::new(), &[], Vec::new(), 3).unwrap();
        assert!(unset.lod_switch_distance(1).unwrap_err().contains("reference distance"));
    }

    #[test]
    fn a_vertex_with_no_weight_is_an_error() {
        let e = rest_vertices(&[vert([0.0; 3], [0; 4], [0; 4])], &[IDENT]).unwrap_err();
        assert!(e.contains("no blend weight"), "{e}");
    }

    #[test]
    fn a_joint_past_the_palette_is_an_error() {
        let e = rest_vertices(&[vert([0.0; 3], [3, 0, 0, 0], [255, 0, 0, 0])], &[IDENT]).unwrap_err();
        assert!(e.contains("joint 3"), "{e}");
    }

    #[test]
    fn a_ground_row_samples_the_same_from_an_unreal_position() {
        let g = mesh::TerrainGround {
            global: 7,
            detail: None,
            layers: [Some(9), None, None, Some(11)],
            layer_uv: [[[0.5, 0.25, -0.125, 3.0], [0.0, 1.0, 2.0, -1.0]]; 4],
            uv_scale: [-0.0025, -0.0025],
            centre: [0.0; 2],
            fade: TerrainFade::default(),
        };
        let d = DrawGroup { index_count: 3, ground: Some(g.clone()), ..Default::default() };
        let s = section(&d, true, Maps::default()).unwrap();
        assert_eq!(s.kind, SECTION_GROUND);
        assert_eq!((s.ground.layer_mask, s.ground.has_detail), (0b1001, 0));
        let game = [12.0f32, -3.0, 40.0];
        let unreal = space::position(game);
        for row in 0..2 {
            let want: f32 = (0..3).map(|c| g.layer_uv[0][row][c] * game[c]).sum::<f32>() + g.layer_uv[0][row][3];
            let got: f32 = (0..3).map(|c| s.ground.layer_uv[0][row][c] * unreal[c]).sum::<f32>() + s.ground.layer_uv[0][row][3];
            assert!((want - got).abs() < 1e-4, "row {row}: {got} is not {want}");
        }
        assert_eq!(s.ground.uv_scale, [-0.000025, -0.000025]);
    }

    #[test]
    fn a_section_past_the_index_buffer_is_an_error() {
        let d = DrawGroup { index_start: 3, index_count: 3, ..Default::default() };
        let e = finish(Vec::new(), &[], vec![section(&d, true, Maps::default()).unwrap()], 1).unwrap_err();
        assert!(e.contains("section 0"), "{e}");
    }
}
