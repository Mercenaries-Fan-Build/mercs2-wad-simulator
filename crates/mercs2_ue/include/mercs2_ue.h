/* mercs2_ue — Mercenaries 2 game data for Unreal Engine.
 *
 * The C interface of the `mercs2_ue` static library (crates/mercs2_ue). Everything it returns is
 * already in Unreal's space: Z up, centimetres, left-handed, triangles ordered for Unreal's front
 * face. The caller converts nothing. `src/space.rs` holds the conversion and its evidence.
 *
 * Objects are opaque pointers freed with their own function. A call that fails returns NULL (or 0
 * where it returns a number) and leaves a message for m2ue_last_error(). Buffers belong to their
 * object and live until it is freed. A game handle is used by one thread at a time.
 *
 * The records below are `#[repr(C)]` structs in src/data.rs and src/ffi.rs; the static assertions
 * pin the sizes and offsets both sides were built with.
 */
#ifndef MERCS2_UE_H
#define MERCS2_UE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define M2UE_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
extern "C" {
#else
#define M2UE_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#endif

typedef struct M2ueGame M2ueGame;
typedef struct M2ueMesh M2ueMesh;
typedef struct M2ueTexture M2ueTexture;
typedef struct M2ueLayer M2ueLayer;
typedef struct M2ueLayerList M2ueLayerList;

/* Texture slots a material record carries. */
#define M2UE_MATERIAL_TEXTURE_SLOTS 10

/* One vertex, posed at rest. */
typedef struct M2ueVertex {
    float position[3];
    float normal[3];
    float tangent[3];
    /* The bitangent itself, not a sign. */
    float bitangent[3];
    /* As authored: V = 0 is the top row of the texture. */
    float uv[2];
    float color[4];
} M2ueVertex;

/* The parameters of a terrain ground draw. */
typedef struct M2ueGround {
    /* The cell's own map, stretched over the whole cell. */
    uint32_t global;
    /* The shared detail map; meaningful when has_detail is 1. */
    uint32_t detail;
    /* The four detail layers' diffuse maps; layer n is meaningful when bit n of layer_mask is set. */
    uint32_t layers[4];
    uint8_t has_detail;
    uint8_t layer_mask;
    uint8_t _pad[2];
    /* An Unreal WORLD position P (centimetres) samples layer n at
     * (layer_uv[n][0] . (P, 1), layer_uv[n][1] . (P, 1)). */
    float layer_uv[4][2][4];
    /* The ground material's UV scale, per centimetre. */
    float uv_scale[2];
} M2ueGround;

#define M2UE_SECTION_MATERIAL 0
#define M2UE_SECTION_GROUND 1

/* M2ueSection.slot1_role: what the pixel shader samples at stage 1. */
#define M2UE_SLOT1_NONE 0
#define M2UE_SLOT1_SPECULAR 1
#define M2UE_SLOT1_EMISSIVE 2

/* The alpha an alpha-tested material's pixel must exceed, of 255. */
#define M2UE_ALPHA_REF 0x4D

/* One draw: a run of triangles with one material. */
typedef struct M2ueSection {
    uint32_t index_start;
    uint32_t index_count;
    /* The LOD tiers this draw is drawn at: bit n = tier n. 0xFF on a terrain cell. */
    uint8_t lod_mask;
    /* 1 when the draw shows on an entity with no destruction state; 0 for wreckage and break pieces. */
    uint8_t node_enabled;
    /* The LOD block the geometry came from: 0 is the coarsest. */
    uint8_t rung;
    /* M2UE_SECTION_MATERIAL or M2UE_SECTION_GROUND. */
    uint8_t kind;
    /* The skeleton node the draw hangs from; negative for none. */
    int16_t node;
    /* The material's on-disk flags word: blend mode, alpha test, two-sided. */
    uint16_t material_flags;
    /* 1 when the material is alpha-tested: a pixel draws when its alpha exceeds M2UE_ALPHA_REF of 255. */
    uint8_t alpha_tested;
    /* 1 when the material draws blended. */
    uint8_t blended;
    /* On a stand-in's section: 1 for the part drawn while `object` is ruined, 0 for the part drawn
     * while it is intact. 0 elsewhere. */
    uint8_t ruined;
    /* What slot1_map is to the pixel shader: M2UE_SLOT1_NONE, M2UE_SLOT1_SPECULAR or
     * M2UE_SLOT1_EMISSIVE. */
    uint8_t slot1_role;
    /* The hash of the material's pixel shader name; 0 for no material. */
    uint32_t shader_key;
    /* On a stand-in's section: the entity key of the world object it stands in for. 0 elsewhere. */
    uint32_t object;
    /* The textures the pixel shader declares a sampler for; 0 where it samples none, and all 0 on a
     * ground section. diffuse_map is its diffuseMap. slot1_map is its specularMap (the highlight
     * mask) or its emissiveMap (added after lighting, times the emissive constant), as slot1_role
     * says. normal_map is its normalMap: tangent-space, X in alpha, Y in green, Z to reconstruct. */
    uint32_t diffuse_map;
    uint32_t slot1_map;
    uint32_t normal_map;
    /* Slot 0 diffuse, 1 specular, 2 normal, then whatever else the shader binds. */
    uint32_t texture_count;
    uint32_t textures[M2UE_MATERIAL_TEXTURE_SLOTS];
    /* Pixel constants: diffuse, specular, ambient, emissive, subsurface, constant0. */
    float material_data[6][4];
    /* Meaningful when kind is M2UE_SECTION_GROUND. */
    M2ueGround ground;
} M2ueSection;

#define M2UE_TEXTURE_BC1 1 /* DXT1 */
#define M2UE_TEXTURE_BC3 3 /* DXT5 */

typedef struct M2ueTextureInfo {
    uint32_t width;
    uint32_t height;
    /* M2UE_TEXTURE_BC1 or M2UE_TEXTURE_BC3. */
    uint32_t format;
    uint32_t mip_count;
} M2ueTextureInfo;

/* M2ueLayerInfo.kind: which block ships the layer. */
#define M2UE_LAYER_OTHER 0
/* A layers_static block: the world that is always there. */
#define M2UE_LAYER_STATIC 1
/* A vz_state_* block: an overlay the game's scripts load for a world state or a mission. */
#define M2UE_LAYER_STATE_OVERLAY 2

typedef struct M2ueLayerInfo {
    uint32_t hash;
    uint32_t kind;
} M2ueLayerInfo;

/* Bits of M2uePlacement.kinds. */
#define M2UE_PLACEMENT_MODEL (1u << 0)
#define M2UE_PLACEMENT_TERRAIN_CELL (1u << 1)
#define M2UE_PLACEMENT_LOW_RES_TILE (1u << 2)
#define M2UE_PLACEMENT_SCRUB (1u << 3)
#define M2UE_PLACEMENT_LIGHT (1u << 4)
/* The entity never moves. */
#define M2UE_PLACEMENT_STATIC (1u << 5)
/* A far-distance stand-in for the world objects of one 200 m cell. The game draws an object's part
 * of it only while that object is not itself drawn, so it is never M2UE_PLACEMENT_MODEL; `model`
 * holds the stand-in's model. */
#define M2UE_PLACEMENT_TINY_STAND_IN (1u << 6)

#define M2UE_LIGHT_POINT 2
#define M2UE_LIGHT_SPOT 3

/* One placed entity of a layer. */
typedef struct M2uePlacement {
    uint32_t key;
    /* The entity's template; 0 for none. */
    uint32_t template_key;
    /* M2UE_PLACEMENT_* bits; each hash below is meaningful only when its bit is set. */
    uint32_t kinds;
    /* With M2UE_PLACEMENT_MODEL the model the entity is drawn as; with
     * M2UE_PLACEMENT_TINY_STAND_IN the stand-in's model. */
    uint32_t model;
    uint32_t terrain_cell;
    uint32_t low_res_tile;
    uint32_t scrub;
    float position[3];
    /* Quaternion (x, y, z, w). */
    float rotation[4];
    /* With M2UE_PLACEMENT_TERRAIN_CELL, centimetres: where the detail layers start to fade into the
     * global map and where they are gone, the same two for the shared detail map, then the size of one
     * tile of the shared detail map. */
    float terrain_fade[5];
    /* With M2UE_PLACEMENT_LIGHT: M2UE_LIGHT_POINT or M2UE_LIGHT_SPOT. */
    uint32_t light_kind;
    /* Linear RGB. */
    float light_color[3];
    /* The game's own intensity value, not converted to any Unreal unit. */
    float light_intensity;
    /* Reach, centimetres. */
    float light_range;
    /* A spot's inner cone half-angle, radians. */
    float light_cone_inner;
    /* A spot's full cone angle, radians. */
    float light_cone_angle;
    /* A spot's axis, a unit direction in world space. */
    float light_axis[3];
    /* How near a viewer must come, on the ground plane, for the game to wake the entity and draw
     * it, in centimetres. Beyond it (plus a 7 m band) the entity sleeps and only its part of a
     * stand-in is drawn. */
    float wake_radius;
    /* What the entity's template makes it: M2UE_ARCHETYPE_PROP, _VEHICLE or _CHARACTER. */
    uint32_t archetype;
} M2uePlacement;

/* A rendered prop. */
#define M2UE_ARCHETYPE_PROP 0
/* A drivable vehicle. */
#define M2UE_ARCHETYPE_VEHICLE 1
/* A person: its model is a kit whose visible parts an outfit selects, on an animated skeleton.
 * Loaded as a mesh at rest with no outfit, it draws nothing. */
#define M2UE_ARCHETYPE_CHARACTER 2

M2UE_STATIC_ASSERT(sizeof(M2ueVertex) == 72, "M2ueVertex");
M2UE_STATIC_ASSERT(sizeof(M2ueGround) == 164, "M2ueGround");
M2UE_STATIC_ASSERT(sizeof(M2ueSection) == 344, "M2ueSection");
M2UE_STATIC_ASSERT(offsetof(M2ueSection, diffuse_map) == 28, "M2ueSection.diffuse_map");
M2UE_STATIC_ASSERT(offsetof(M2ueSection, textures) == 44, "M2ueSection.textures");
M2UE_STATIC_ASSERT(offsetof(M2ueSection, ground) == 180, "M2ueSection.ground");
M2UE_STATIC_ASSERT(sizeof(M2ueTextureInfo) == 16, "M2ueTextureInfo");
M2UE_STATIC_ASSERT(sizeof(M2ueLayerInfo) == 8, "M2ueLayerInfo");
M2UE_STATIC_ASSERT(sizeof(M2uePlacement) == 128, "M2uePlacement");
M2UE_STATIC_ASSERT(offsetof(M2uePlacement, archetype) == 124, "M2uePlacement.archetype");
M2UE_STATIC_ASSERT(offsetof(M2uePlacement, wake_radius) == 120, "M2uePlacement.wake_radius");
M2UE_STATIC_ASSERT(offsetof(M2uePlacement, position) == 28, "M2uePlacement.position");
M2UE_STATIC_ASSERT(offsetof(M2uePlacement, light_kind) == 76, "M2uePlacement.light_kind");

/* The message of the last call that failed on this thread, valid until the next call on it. */
const char *m2ue_last_error(void);

/* The game's name hash of a NUL-terminated UTF-8 asset, layer or shader name. */
uint32_t m2ue_hash(const char *name);

/* Convert a point from the game's space (three floats, metres) to Unreal's (three floats,
 * centimetres), for a position that comes from the game's scripts rather than from this library.
 * Returns 1, or 0 with an error. */
uint32_t m2ue_position(const float *game_metres, float *unreal);

/* Convert a length from the game's metres to Unreal's centimetres. */
float m2ue_length(float metres);

/* Open the game from the path of its level archive (vz.wad), NUL-terminated UTF-8. The archives
 * beside it (patches, language) are mounted with it, as the game mounts them. */
M2ueGame *m2ue_open(const char *level_wad);
void m2ue_close(M2ueGame *game);

/* 1 when the level archive ships a model under this hash, 0 when it does not: a placed entity can
 * name a model the game does not ship, and draws nothing for it. */
uint32_t m2ue_model_exists(const M2ueGame *game, uint32_t hash);
/* A model by asset hash: every LOD block of its chain in one vertex and index buffer. */
M2ueMesh *m2ue_model_load(M2ueGame *game, uint32_t hash);
/* A hi-res terrain cell by terrainmesh hash, cell-local: its placement gives its position. */
M2ueMesh *m2ue_terrain_cell_load(M2ueGame *game, uint32_t hash);
/* A far-distance stand-in by its model hash, relative to its placement: one section per world
 * object, state and material, each naming the object it stands in for (`object`, `ruined`). */
M2ueMesh *m2ue_stand_in_load(M2ueGame *game, uint32_t hash);
void m2ue_mesh_free(M2ueMesh *mesh);
const M2ueVertex *m2ue_mesh_vertices(const M2ueMesh *mesh, size_t *count);
/* The triangle list: three indices per triangle. */
const uint32_t *m2ue_mesh_indices(const M2ueMesh *mesh, size_t *count);
const M2ueSection *m2ue_mesh_sections(const M2ueMesh *mesh, size_t *count);
/* LOD tiers the model declares; 1 for a terrain cell. */
uint32_t m2ue_mesh_lod_count(const M2ueMesh *mesh);
/* The distance from the main camera, in Unreal units, at which the game starts drawing tier `tier`
 * (1 or above) in place of the tier before it: the model's reference distance x the main view's
 * factor x 2^(tier-1). Returns -1 with an error for tier 0 or a tier the mesh does not have. */
float m2ue_mesh_lod_switch_distance(const M2ueMesh *mesh, uint32_t tier);
/* Writes three floats to each of min and max. Returns 1, or 0 with an error. */
uint32_t m2ue_mesh_bounds(const M2ueMesh *mesh, float *min, float *max);

/* A texture by asset hash, with its streamed mips where the archives ship them. */
M2ueTexture *m2ue_texture_load(M2ueGame *game, uint32_t hash);
void m2ue_texture_free(M2ueTexture *texture);
/* Returns 1, or 0 with an error. */
uint32_t m2ue_texture_info(const M2ueTexture *texture, M2ueTextureInfo *info);
/* The whole mip chain, largest mip first. */
const uint8_t *m2ue_texture_data(const M2ueTexture *texture, size_t *size);
/* The largest mip decoded to 8-bit RGBA: width * height * 4 bytes, row-major from the top row. */
const uint8_t *m2ue_texture_rgba(const M2ueTexture *texture, size_t *size);

/* Every layer the mounted archives carry, ascending by name hash. */
M2ueLayerList *m2ue_layers(const M2ueGame *game);
void m2ue_layers_free(M2ueLayerList *list);
const M2ueLayerInfo *m2ue_layers_infos(const M2ueLayerList *list, size_t *count);
/* The base name of the block that ships layer `index` (layers_static, vz_state_pmcinterior). */
const char *m2ue_layers_block(const M2ueLayerList *list, size_t index);

/* Every placed entity of a layer, by layer name hash, in data order. */
M2ueLayer *m2ue_layer_load(M2ueGame *game, uint32_t hash);
void m2ue_layer_free(M2ueLayer *layer);
const M2uePlacement *m2ue_layer_placements(const M2ueLayer *layer, size_t *count);

/* The world layers a save file (*.profile) restores: the names the game's layer manager saved, in
 * the save's order, lower case. They are the layers loaded on top of the static world. */
typedef struct M2ueSaveLayers M2ueSaveLayers;
M2ueSaveLayers *m2ue_save_layers(const char *profile);
void m2ue_save_layers_free(M2ueSaveLayers *layers);
size_t m2ue_save_layers_count(const M2ueSaveLayers *layers);
const char *m2ue_save_layers_name(const M2ueSaveLayers *layers, size_t index);

/* The game's lighting under one atmosphere setting: the light block the game builds every frame
 * from its live atmosphere once that has settled on the setting. A setting is an AtmosphereBase
 * record the level ships on a region of the map (or on entity 0x900000CF, for outside every
 * region); the game blends the live atmosphere toward the setting of the region the viewer is in.
 * Directions are in Unreal's axes. */
typedef struct M2ueAtmosphere {
    /* The game's time of day, 0..1 across the day; 0.5 has the sun overhead. */
    float time;
    /* The unit direction toward the sun. The game's directional light travels the other way.
     * Derived from the setting's time by a fit to retail captures, not by code read from the exe. */
    float toward_sun[3];
    /* The directional light's colour, linear, light intensity folded in: a surface facing the sun
     * takes its diffuse colour times this. It is the setting's light multiplier x intensity; the
     * game's own is that times a tint that reddens as the sun gets lower, which is not read. */
    float light_color[3];
    float rim_color[3];
    /* The six ambient faces in the game's order, each the ambient cube's face times the ambient
     * colour. Which direction each face lights is not read from the game's shaders. */
    float ambient[6][3];
    /* The Henyey-Greenstein constant of the sky's Mie phase function. */
    float mie_g;
    /* The entity whose setting this is: a region, or 0x900000CF outside every region. */
    uint32_t entity;
} M2ueAtmosphere;
M2UE_STATIC_ASSERT(sizeof(M2ueAtmosphere) == 120, "M2ueAtmosphere");
M2UE_STATIC_ASSERT(offsetof(M2ueAtmosphere, ambient) == 40, "M2ueAtmosphere.ambient");

/* Writes the light of the setting named `setting` that applies at a point of the game's space
 * (three floats, metres; the height is not used). The region is the last one, in the records'
 * order, whose polygon holds the point; where regions overlap the game's own choice also depends on
 * the path the viewer took. A region's setting is "default" until a script changes it. Returns 1,
 * or 0 with an error (which lists the settings the entity has when it lacks the one named). */
uint32_t m2ue_atmosphere_at(M2ueGame *game, const float *game_position, const char *setting, M2ueAtmosphere *out);

#ifdef __cplusplus
}
#endif

#endif /* MERCS2_UE_H */
