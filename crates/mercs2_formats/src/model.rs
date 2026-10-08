//! The model container (`0x5B724250`) of a static or skinned model: every chunk decoded into typed
//! fields and encoded back to the same bytes.
//!
//! The container is a UCFX tree ([`crate::ucfx::parse_ucfx_tree`]) of this shape:
//!
//! ```text
//! INFO  72 B   object flags, bounds, counts, LOD constants              -> ModelInfo
//! HIER  176 B per node                                                  -> HierNode
//! MTRL  116 + 4·textures B per material                                 -> Material
//! BSHP  8 B per blend-shape channel (only when the model has channels)  -> BlendChannel
//! SEGM  4 B per row: {i16 node, u8 slot, u8 lod_mask}                   -> SegmRow
//! PHY2  48-byte prefix, Havok packfile, node wrapper (optional)         -> Phy2
//! GEOM                                                                  -> Geom
//!   INFO  u32 sub-object count
//!   INDX  u16 per sub-object: its renderable slot
//!   MESH | SKIN  (one per sub-object)
//!     INFO  u32 group count
//!     PRMG  (one per group)
//!       MESH: INFO 60 B, STRM, AREA, IBUF, PRMT                          -> MeshGroup
//!       SKIN: INFO 56 B, STRM, IBUF, [BSHP × n, BSHI], PRMT              -> SkinGroup
//! ```
//!
//! A model's coarser LOD blocks (`_P001_Q2`, `_P002_Q1`) carry a container holding only the `GEOM`
//! tree; [`LodBlock`] reads and writes those.
//!
//! Every count the tree implies is derived, not stored: the `INFO` node, material, `SEGM` row and
//! channel counts, the `GEOM` and sub-object `INFO` counts, the group `INFO` list and morph counts,
//! the `STRM` `info` words, the `IBUF`/`AREA` counts, the `BSHI` length and the `PHY2` packfile size.
//! [`Model::decode`] checks each against what it counts and refuses a container where they disagree;
//! [`Model::encode`] writes them from the data. Words the engine never reads and that hold one value
//! in every retail model are checked on decode and written on encode. Floats are kept as `f32` and
//! written back with their stored bits.
//!
//! Destructible models (a top-level `STAM` or `MIXR`), TINY containers and `TINY` sub-objects
//! ([`crate::tiny_model`]), and a `SWIT` row met at the top level or as a sub-object are refused
//! with an error naming the chunk; so is any other chunk outside the shape above.

use crate::crc32::crc32_mercs2;
use crate::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode, UCFX_CSUM_BYTES};
use crate::ucfx_codec::{
    count_word, exact_len, f32_at, f32s, leaf, marker, put_f32s, put_u16, put_u32, u16_at, u32_at,
};

/// Bytes of the top-level `INFO`.
pub const INFO_BYTES: usize = 72;
/// Bytes of one `HIER` node.
pub const HIER_NODE_BYTES: usize = 176;
/// Bytes of a material record without its texture hashes; each texture adds 4.
pub const MTRL_RECORD_BASE_BYTES: usize = 116;
/// Texture slots `Mtrl_Parse` fills; a record naming more writes past them.
pub const MTRL_MAX_TEXTURES: usize = 10;
/// Bytes of one top-level `BSHP` record.
pub const BSHP_RECORD_BYTES: usize = 8;
/// Bytes of one `SEGM` row.
pub const SEGM_ROW_BYTES: usize = 4;
/// Bytes of the `PHY2` prefix before the Havok packfile.
pub const PHY2_PREFIX_BYTES: usize = 48;
/// Bytes of a `MESH` group's `INFO`.
pub const MESH_GROUP_INFO_BYTES: usize = 60;
/// Bytes of a `SKIN` group's `INFO`.
pub const SKIN_GROUP_INFO_BYTES: usize = 56;
/// Bone-palette range slots in a `SKIN` group's `INFO`.
pub const SKIN_RANGE_SLOTS: usize = 8;
/// Bytes of one `PRMT` record.
pub const PRMT_RECORD_BYTES: usize = 16;
/// Bytes of one vertex declaration element.
pub const DECL_ELEMENT_BYTES: usize = 8;
/// `D3DDECL_END()`: stream 0xFF, offset 0, type UNUSED, method 0, usage 0, usage index 0.
pub const DECL_END: [u8; 8] = [0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00];
/// Bytes per vertex of a morph-target stream: one FLOAT16_4 delta.
pub const MORPH_STRIDE: usize = 8;
/// The word every retail `HIER` node holds at `+0x9C` and `+0xAC` (the fourth bound component,
/// dropped by the loader): `1.0f32`.
pub const HIER_BOUND_W: u32 = 0x3f80_0000;

// ── model INFO ──────────────────────────────────────────────────────────────────────────────────

/// `INFO +0x1C`: the seed the model renderable ORs into its object flags (`OBJ+0x10 |= flags | 4`).
///
/// Only these six bits occur in retail; any other bit is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectFlags {
    /// `0x10`: no consumer located.
    pub undecoded_0x10: bool,
    /// `0x20`: no consumer located.
    pub undecoded_0x20: bool,
    /// `0x80`: the far fade is applied smoothly; clear, it is a hard step.
    pub smooth_fade: bool,
    /// `0x200`: no consumer located.
    pub undecoded_0x200: bool,
    /// `0x400`: carried by every TINY container; no consumer located.
    pub tiny: bool,
    /// `0x800`: the instance builds a skin palette (`InvBind(HIER +80) × node model space`) every
    /// frame. Every model with a `SKIN` sub-object carries it.
    pub skin_palette: bool,
}

impl ObjectFlags {
    pub const UNDECODED_0X10: u32 = 0x10;
    pub const UNDECODED_0X20: u32 = 0x20;
    pub const SMOOTH_FADE: u32 = 0x80;
    pub const UNDECODED_0X200: u32 = 0x200;
    pub const TINY: u32 = 0x400;
    pub const SKIN_PALETTE: u32 = 0x800;
    const ALL: u32 = Self::UNDECODED_0X10
        | Self::UNDECODED_0X20
        | Self::SMOOTH_FADE
        | Self::UNDECODED_0X200
        | Self::TINY
        | Self::SKIN_PALETTE;

    pub fn from_bits(bits: u32) -> Result<ObjectFlags, String> {
        if bits & !Self::ALL != 0 {
            return Err(format!(
                "INFO +0x1C object flags 0x{bits:X} carry bits 0x{:X} outside the decoded set 0x{:X}",
                bits & !Self::ALL,
                Self::ALL
            ));
        }
        Ok(ObjectFlags {
            undecoded_0x10: bits & Self::UNDECODED_0X10 != 0,
            undecoded_0x20: bits & Self::UNDECODED_0X20 != 0,
            smooth_fade: bits & Self::SMOOTH_FADE != 0,
            undecoded_0x200: bits & Self::UNDECODED_0X200 != 0,
            tiny: bits & Self::TINY != 0,
            skin_palette: bits & Self::SKIN_PALETTE != 0,
        })
    }

    pub fn bits(&self) -> u32 {
        [
            (self.undecoded_0x10, Self::UNDECODED_0X10),
            (self.undecoded_0x20, Self::UNDECODED_0X20),
            (self.smooth_fade, Self::SMOOTH_FADE),
            (self.undecoded_0x200, Self::UNDECODED_0X200),
            (self.tiny, Self::TINY),
            (self.skin_palette, Self::SKIN_PALETTE),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |acc, (_, b)| acc | b)
    }
}

/// The top-level `INFO` (72 bytes).
///
/// The counts at `+0x20` (`HIER` nodes), `+0x24` (materials), `+0x28` (`SEGM` rows) and `+0x30`
/// (blend-shape channels) are derived from [`Model`]'s lists.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// `+0x00`: `0x39` in every retail model; the `INFO` loader never reads it.
    pub header_word: u32,
    /// `+0x04`, `+0x10`: the model bounds. The loader derives the centre and the radius
    /// (half the diagonal) from them.
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// `+0x1C`.
    pub object_flags: ObjectFlags,
    /// `+0x2C`: renderable slots; the loader allocates this many and each sub-object's `INDX` entry
    /// is its slot.
    pub slot_count: u32,
    /// `+0x34`: LOD rungs; the rung clamp max and the number of per-rung `SEGM` row lists.
    pub lod_count: u32,
    /// `+0x38`: the camera distance at which rung 0 hands off to rung 1; rung `n ≥ 1` covers
    /// `[2^(n−1)·D, 2^n·D)`.
    pub lod_base_distance: f32,
    /// `+0x3C`: LOD cross-fade sharpness (`clamp(frac·k, 0, 1)`).
    pub fade_sharpness: f32,
    /// `+0x40` .. `+0x47`: one rung mask per LOD block, indexed by the block's `Q` (`[3]` is the
    /// resident `P000_Q3` block). Loading block `k` sets the minimum rung to the lowest set bit of
    /// `[k]`.
    pub block_lod_masks: [u16; 4],
}

// ── HIER ────────────────────────────────────────────────────────────────────────────────────────

/// One `HIER` node (176 bytes).
///
/// `+0x0C` is 0 in every retail node and is not stored by the loader; decode refuses any other
/// value and encode writes 0. `+0x9C` and `+0xAC` (the fourth bound components, dropped by the
/// loader) are [`HIER_BOUND_W`] likewise.
#[derive(Debug, Clone, PartialEq)]
pub struct HierNode {
    /// `+0x00`: `pandemic_hash_m2(node name)`; the key animation tracks bind by.
    pub name_hash: u32,
    /// `+0x04` bit 0: the node belongs to the animation skeleton. Flagged nodes form a prefix of
    /// the node list. No other bit occurs in retail; any other bit is refused.
    pub skeleton: bool,
    /// `+0x06`: the first node of this node's child chain. The chain walks
    /// [`HierNode::next_sibling`] and visits every node whose parent is this one, once each; retail
    /// chains are not always in index order.
    pub first_child: Option<u16>,
    /// `+0x08`: the parent.
    pub parent: Option<u16>,
    /// `+0x0A`: the next node in the parent's child chain (the roots chain from the first root).
    pub next_sibling: Option<u16>,
    /// `+0x10`: the local transform, row-vector form (`world = local × world[parent]`).
    pub local: [f32; 16],
    /// `+0x50`: the inverse bind matrix the skin palette multiplies by.
    pub inverse_bind: [f32; 16],
    /// `+0x90`, `+0xA0`: the node bounds in the space the node's segments draw in.
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

// ── MTRL ────────────────────────────────────────────────────────────────────────────────────────

/// Material flag bits 0–2: the frame-buffer blend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// 0.
    Opaque,
    /// 1: SRCALPHA / INVSRCALPHA, ADD.
    Alpha,
    /// 2: ONE / ONE, ADD.
    Additive,
    /// 3: ONE / ONE, SUBTRACT.
    Subtract,
    /// 4: ONE / ONE, REVSUBTRACT.
    ReverseSubtract,
}

/// Material flag bits `0x20` and `0x40`: the src/dest blend-factor row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactors {
    /// Neither bit.
    Default,
    /// `0x20`: SRCALPHA / ONE.
    SrcAlphaOne,
    /// `0x40`: DESTALPHA / ONE.
    DestAlphaOne,
    /// `0x60`: ONE / INVSRCALPHA.
    Premultiplied,
}

/// The on-disk `u16` material flags. Bits `0x1000` and above, and blend modes 5–7, are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialFlags {
    /// Bits 0–2.
    pub blend: BlendMode,
    /// `0x08`: ALPHATESTENABLE, ALPHAFUNC GREATER, ALPHAREF 0x4D.
    pub alpha_test: bool,
    /// `0x10`: CULLMODE NONE.
    pub two_sided: bool,
    /// `0x20` / `0x40`.
    pub blend_factors: BlendFactors,
    /// `0x80`: drawn by the shadow pass.
    pub casts_shadow: bool,
    /// `0x100`: texture slot 0 is replaced by the engine's global screen texture.
    pub refraction: bool,
    /// `0x200`: excluded from the colour pass and the z-prepass; purpose undecoded.
    pub undecoded_0x200: bool,
    /// `0x400`: colour sort class 11; purpose undecoded.
    pub undecoded_0x400: bool,
    /// `0x800`: z-prepass class 10 (full material bind); purpose undecoded.
    pub undecoded_0x800: bool,
}

impl MaterialFlags {
    pub fn from_bits(bits: u16) -> Result<MaterialFlags, String> {
        if bits & 0xF000 != 0 {
            return Err(format!("material flags 0x{bits:04X} carry bits 0x{:04X} the engine drops", bits & 0xF000));
        }
        let blend = match bits & 7 {
            0 => BlendMode::Opaque,
            1 => BlendMode::Alpha,
            2 => BlendMode::Additive,
            3 => BlendMode::Subtract,
            4 => BlendMode::ReverseSubtract,
            m => return Err(format!("material flags 0x{bits:04X} blend mode {m} is not a blend the engine maps")),
        };
        let blend_factors = match (bits >> 5) & 3 {
            0 => BlendFactors::Default,
            1 => BlendFactors::SrcAlphaOne,
            2 => BlendFactors::DestAlphaOne,
            _ => BlendFactors::Premultiplied,
        };
        Ok(MaterialFlags {
            blend,
            alpha_test: bits & 0x08 != 0,
            two_sided: bits & 0x10 != 0,
            blend_factors,
            casts_shadow: bits & 0x80 != 0,
            refraction: bits & 0x100 != 0,
            undecoded_0x200: bits & 0x200 != 0,
            undecoded_0x400: bits & 0x400 != 0,
            undecoded_0x800: bits & 0x800 != 0,
        })
    }

    pub fn bits(&self) -> u16 {
        let blend = match self.blend {
            BlendMode::Opaque => 0,
            BlendMode::Alpha => 1,
            BlendMode::Additive => 2,
            BlendMode::Subtract => 3,
            BlendMode::ReverseSubtract => 4,
        };
        let factors = match self.blend_factors {
            BlendFactors::Default => 0,
            BlendFactors::SrcAlphaOne => 0x20,
            BlendFactors::DestAlphaOne => 0x40,
            BlendFactors::Premultiplied => 0x60,
        };
        [
            (self.alpha_test, 0x08),
            (self.two_sided, 0x10),
            (self.casts_shadow, 0x80),
            (self.refraction, 0x100),
            (self.undecoded_0x200, 0x200),
            (self.undecoded_0x400, 0x400),
            (self.undecoded_0x800, 0x800),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(blend | factors, |acc, (_, b)| acc | b)
    }
}

/// One `MTRL` record: 26 preamble words, flags, texture count, texture hashes, pixel-shader key,
/// surface hash (`116 + 4·textures` bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// w0: the hash the material property animator matches materials by.
    pub name_hash: u32,
    /// w1–w3: `materialData.diffuse.rgb`.
    pub diffuse: [f32; 3],
    /// w4–w6: `materialData.specular.rgb`.
    pub specular: [f32; 3],
    /// w7–w9: `materialData.ambient.rgb`.
    pub ambient: [f32; 3],
    /// w10–w12: `materialData.emissive.rgb`.
    pub emissive: [f32; 3],
    /// w13: multiplies the diffuse texture alpha; below 1 the material draws in the translucent pass.
    pub opacity: f32,
    /// w14: the Blinn exponent (clamped to ≥ 1e-4 on load).
    pub specular_power: f32,
    /// w15: scales the reflection-map term.
    pub reflection_intensity: f32,
    /// w16: scales the screen-space refraction offset; Schlick F0 = ((n−1)/(n+1))².
    pub refraction_index: f32,
    /// w17–w20: `materialData.subsurface`: rgb scatter colour, w added to N·L.
    pub subsurface: [f32; 4],
    /// w21–w22: UV offset.
    pub uv_offset: [f32; 2],
    /// w23–w24: UV scale.
    pub uv_scale: [f32; 2],
    /// w25: UV rotation about (0.5, 0.5), radians.
    pub uv_rotation: f32,
    /// `+104`.
    pub flags: MaterialFlags,
    /// `+108`: texture hashes in slot order (diffuse, specular, normal, …), at most
    /// [`MTRL_MAX_TEXTURES`].
    pub textures: Vec<u32>,
    /// The pixel-shader registry key.
    pub pixel_shader: u32,
    /// The surface type (`pandemic_hash_m2(name)`): the row key of the impact/decal tables.
    pub surface: u32,
}

// ── BSHP / SEGM / PHY2 ──────────────────────────────────────────────────────────────────────────

/// One top-level `BSHP` record: a blend-shape channel. Its position is the channel index a group's
/// `BSHI` names. `+4` is 0 in every retail record and has no reader; decode refuses any other value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendChannel {
    /// The channel name hash (the facial-expression system looks channels up by it).
    pub name_hash: u32,
}

/// One `SEGM` row: a renderable slot drawn at a node over a band of LOD rungs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmRow {
    /// The `HIER` node whose world matrix and enable gate the row uses; negative draws at the
    /// instance root.
    pub node: i16,
    /// The renderable slot (an `INDX` value).
    pub slot: u8,
    /// Bit `n` set: the row draws at rung `n`.
    pub lod_mask: u8,
}

/// The 48-byte `PHY2` prefix. The packfile size (`w8`) is derived from [`Phy2::packfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phy2Prefix {
    /// w0: `0x39` on disk; the loader overwrites it with the chunk byte size.
    pub header_word: u32,
    /// w1: the model name hash (the physics registry key).
    pub name_hash: u32,
    /// w2: wrapper node records (`0xAAAAAAAA … 0xBBBBBBBB`).
    pub node_records: u32,
    /// w3: internal wrapper nodes (`0xCCCCCCCC … 0xDDDDDDDD` blocks).
    pub internal_nodes: u32,
    /// w4: leaf wrapper nodes (`0xEEEEEEEE` shape descriptors).
    pub leaf_nodes: u32,
    /// w5: 280-byte `0xAAAABBBB … 0xAAAACCCC` records.
    pub linked_records: u32,
    /// w6: the plane total of every convex hull.
    pub hull_planes: u32,
    /// w7: hull vertex totals plus each distinct mesh vertex pool's count.
    pub vertex_total: u32,
    /// w9–w11: 0 in every retail model; no reader located.
    pub reserved: [u32; 3],
}

/// A `PHY2` chunk: the prefix, the Havok packfile, and the node wrapper after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phy2 {
    pub prefix: Phy2Prefix,
    pub packfile: Vec<u8>,
    pub wrapper: Vec<u8>,
}

// ── vertex streams ──────────────────────────────────────────────────────────────────────────────

/// `D3DDECLTYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclType {
    Float1,
    Float2,
    Float3,
    Float4,
    D3dColor,
    UByte4,
    Short2,
    Short4,
    UByte4N,
    Short2N,
    Short4N,
    UShort2N,
    UShort4N,
    UDec3,
    Dec3N,
    Float16x2,
    Float16x4,
}

impl DeclType {
    pub fn from_u8(v: u8) -> Option<DeclType> {
        use DeclType::*;
        Some(match v {
            0 => Float1,
            1 => Float2,
            2 => Float3,
            3 => Float4,
            4 => D3dColor,
            5 => UByte4,
            6 => Short2,
            7 => Short4,
            8 => UByte4N,
            9 => Short2N,
            10 => Short4N,
            11 => UShort2N,
            12 => UShort4N,
            13 => UDec3,
            14 => Dec3N,
            15 => Float16x2,
            16 => Float16x4,
            _ => return None,
        })
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    /// Bytes one element of this type occupies.
    pub fn size(self) -> usize {
        use DeclType::*;
        match self {
            Float1 | D3dColor | UByte4 | Short2 | UByte4N | Short2N | UShort2N | UDec3 | Dec3N | Float16x2 => 4,
            Float2 | Short4 | Short4N | UShort4N | Float16x4 => 8,
            Float3 => 12,
            Float4 => 16,
        }
    }
}

/// `D3DDECLUSAGE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclUsage {
    Position,
    BlendWeight,
    BlendIndices,
    Normal,
    PSize,
    TexCoord,
    Tangent,
    Binormal,
    TessFactor,
    PositionT,
    Color,
    Fog,
    Depth,
    Sample,
}

impl DeclUsage {
    pub fn from_u8(v: u8) -> Option<DeclUsage> {
        use DeclUsage::*;
        Some(match v {
            0 => Position,
            1 => BlendWeight,
            2 => BlendIndices,
            3 => Normal,
            4 => PSize,
            5 => TexCoord,
            6 => Tangent,
            7 => Binormal,
            8 => TessFactor,
            9 => PositionT,
            10 => Color,
            11 => Fog,
            12 => Depth,
            13 => Sample,
            _ => return None,
        })
    }

    pub fn code(self) -> u8 {
        self as u8
    }
}

/// One vertex declaration element (`D3DVERTEXELEMENT9`, method `D3DDECLMETHOD_DEFAULT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclElement {
    pub stream: u16,
    pub offset: u16,
    pub ty: DeclType,
    pub usage: DeclUsage,
    pub usage_index: u8,
}

/// A group's `STRM`: the vertex declaration (without its END element) and the stream-0 vertex
/// bytes. The `info` words are derived: element count including END, stride (the summed sizes of
/// the stream-0 elements), vertex count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexStream {
    pub elements: Vec<DeclElement>,
    pub data: Vec<u8>,
}

impl VertexStream {
    /// Bytes per stream-0 vertex.
    pub fn stride(&self) -> usize {
        self.elements.iter().filter(|e| e.stream == 0).map(|e| e.ty.size()).sum()
    }

    /// Vertices in [`VertexStream::data`].
    pub fn vertex_count(&self) -> usize {
        match self.stride() {
            0 => 0,
            s => self.data.len() / s,
        }
    }

    /// The stream-0 element with this usage and usage index.
    pub fn element(&self, usage: DeclUsage, usage_index: u8) -> Option<&DeclElement> {
        self.elements.iter().find(|e| e.stream == 0 && e.usage == usage && e.usage_index == usage_index)
    }
}

// ── geometry ────────────────────────────────────────────────────────────────────────────────────

/// One `PRMT` record: a triangle-strip draw over the group's `IBUF`. Indices are absolute (base
/// vertex 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimRecord {
    /// Material index into [`Model::mtrl`].
    pub material: u32,
    /// First strip index.
    pub start: u32,
    /// Triangles: strip length − 2.
    pub prims: u16,
    /// `MinIndex`: the lowest index in the strip.
    pub min_index: u16,
    /// The highest index the draw covers: `NumVertices = min(max − min + 1, vertex count)`. It is
    /// the strip's highest index in every retail record but one list-B record, which declares the
    /// whole vertex range (`0`, vertex count − 1, unique count = vertex count).
    pub max_index: u16,
    /// Distinct indices in the strip; read by no model draw.
    pub unique_count: u16,
}

/// A `MESH` group (`PRMG` with a 60-byte `INFO`).
///
/// `INFO +0x00`/`+0x04` (list A/B record counts) are derived from the lists; `+0x08` (morph
/// targets) is 0 in every retail `MESH` group and a non-zero count is refused.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshGroup {
    /// `INFO +0x0C`: the colour-pass vertex shader key.
    pub vertex_shader: u32,
    /// `INFO +0x10`: the z/shadow-pass vertex shader key.
    pub second_vertex_shader: u32,
    /// `INFO +0x14`, `+0x20`: the culling sphere.
    pub center: [f32; 3],
    pub radius: f32,
    /// `INFO +0x24`, `+0x30`: the group bounds.
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub stream: VertexStream,
    /// `AREA`: one half-float per strip triangle.
    pub area: Vec<u16>,
    /// `IBUF`: the u16 triangle strip.
    pub strip: Vec<u16>,
    /// `PRMT` list A: drawn by the colour and z passes.
    pub list_a: Vec<PrimRecord>,
    /// `PRMT` list B: drawn by the shadow pass.
    pub list_b: Vec<PrimRecord>,
}

/// One `{u16 hier_base, u16 count}` bone-palette range of a `SKIN` group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneRange {
    /// First `HIER` node of the range.
    pub hier_base: u16,
    /// Nodes in the range; three palette registers each.
    pub count: u16,
}

/// One morph target of a `SKIN` group: a `BSHP` stream of FLOAT16_4 position deltas and its `BSHI`
/// entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphTarget {
    /// The `BSHI` entry: the model channel ([`Model::bshp`] index) whose weight drives the target.
    pub channel: u16,
    /// Per vertex: half-float bits `dx, dy, dz, w`.
    pub deltas: Vec<[u16; 4]>,
}

/// A `SKIN` group (`PRMG` with a 56-byte `INFO`).
///
/// `INFO +0x00`/`+0x04` (list counts), `+0x08` (morph targets) and `+0x14` (range count) are
/// derived.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinGroup {
    /// `INFO +0x0C`: the colour-pass vertex shader key.
    pub vertex_shader: u32,
    /// `INFO +0x10`: the z/shadow-pass vertex shader key.
    pub second_vertex_shader: u32,
    /// `INFO +0x18`: the bone-palette ranges the draw uploads (`INFO +0x14` of them).
    pub bone_ranges: Vec<BoneRange>,
    /// The range slots after [`SkinGroup::bone_ranges`], to [`SKIN_RANGE_SLOTS`] in all. The loader
    /// copies them and the draw never reads them; retail groups hold exporter leftovers there
    /// (`{50516, 3345}`, `{12, 0}`, …). An authored group writes zeros.
    pub unread_range_slots: Vec<BoneRange>,
    pub stream: VertexStream,
    /// `IBUF`: the u16 triangle strip.
    pub strip: Vec<u16>,
    /// `BSHP` × n with `BSHI`.
    pub morphs: Vec<MorphTarget>,
    pub list_a: Vec<PrimRecord>,
    pub list_b: Vec<PrimRecord>,
}

/// One drawn sub-object: a `GEOM` `MESH` or `SKIN` child.
#[derive(Debug, Clone, PartialEq)]
pub enum SubObject {
    Mesh { groups: Vec<MeshGroup> },
    Skin { groups: Vec<SkinGroup> },
}

/// The `GEOM` tree. The `GEOM` `INFO` count is derived.
#[derive(Debug, Clone, PartialEq)]
pub struct Geom {
    /// One renderable slot per sub-object, in order.
    pub indx: Vec<u16>,
    pub subs: Vec<SubObject>,
}

/// A decoded model container.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub info: ModelInfo,
    pub hier: Vec<HierNode>,
    pub mtrl: Vec<Material>,
    pub bshp: Vec<BlendChannel>,
    pub segm: Vec<SegmRow>,
    pub phy2: Option<Phy2>,
    pub geom: Geom,
}

// ── little-endian reads and writes ──────────────────────────────────────────────────────────────

fn u16_list(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16_at(c, 0)).collect()
}
fn opt_index(v: u16) -> Option<u16> {
    (v != 0xFFFF).then_some(v)
}
fn put_u16s(v: &mut Vec<u8>, xs: &[u16]) {
    for &x in xs {
        put_u16(v, x);
    }
}
fn u32_bytes(x: u32) -> Vec<u8> {
    x.to_le_bytes().to_vec()
}

// ── tree helpers ────────────────────────────────────────────────────────────────────────────────

fn tags(nodes: &[UcfxNode]) -> String {
    nodes.iter().map(UcfxNode::tag_str).collect::<Vec<_>>().join(", ")
}

/// The `[info, data]` children of an `IBUF` or `AREA` marker: a u32 count and that many u16s.
fn counted_u16s(n: &UcfxNode, want: &[u8; 4], at: &str) -> Result<Vec<u16>, String> {
    let kids = marker(n, want, at)?;
    let what = String::from_utf8_lossy(want).into_owned();
    if kids.len() != 2 {
        return Err(format!("{at} {what} is [{}]; it is [info, data]", tags(kids)));
    }
    let count = count_word(leaf(&kids[0], b"info", at)?, &format!("{at} {what} info"))?;
    let data = leaf(&kids[1], b"data", at)?;
    exact_len(data, 2 * count, &format!("{at} {what} data of {count} entries"))?;
    Ok(u16_list(data))
}

fn counted_u16s_node(tag: [u8; 4], xs: &[u16]) -> UcfxNode {
    let mut data = Vec::with_capacity(2 * xs.len());
    put_u16s(&mut data, xs);
    UcfxNode::marker(tag, vec![UcfxNode::leaf(*b"info", u32_bytes(xs.len() as u32)), UcfxNode::leaf(*b"data", data)])
}

/// The error for a chunk outside the static/skinned model codec.
fn refused(tag: &[u8; 4], at: &str) -> Option<String> {
    match tag {
        b"STAM" | b"MIXR" | b"SWIT" => Some(format!(
            "{at}: {} is a destructible-model chunk; this codec reads static and skinned models only",
            String::from_utf8_lossy(tag)
        )),
        b"TINY" => Some(format!("{at}: TINY is a TINY-container chunk; decode it with tiny_model")),
        _ => None,
    }
}

// ── decode ──────────────────────────────────────────────────────────────────────────────────────

impl Model {
    /// Decode a model container. Strict: the tree shape, every body length, every implied count and
    /// every fixed word must be exactly what [`Model::encode`] writes.
    pub fn decode(container: &[u8]) -> Result<Model, String> {
        let roots = parse_ucfx_tree(container)?;
        for r in &roots {
            if let Some(e) = refused(&r.tag, "top level") {
                return Err(e);
            }
        }
        if roots.len() == 1 && &roots[0].tag == b"GEOM" {
            return Err("top level is [GEOM]: a LOD-block container; decode it with LodBlock::decode".into());
        }

        let mut rest: &[UcfxNode] = &roots;
        let mut take = |want: &[u8; 4], optional: bool| -> Result<Option<&UcfxNode>, String> {
            match rest.split_first() {
                Some((n, tail)) if &n.tag == want => {
                    rest = tail;
                    Ok(Some(n))
                }
                _ if optional => Ok(None),
                _ => Err(format!(
                    "top level is [{}]; a model is [INFO, HIER, MTRL, (BSHP), SEGM, (PHY2), GEOM] and {} is missing \
                     or out of place",
                    tags(&roots),
                    String::from_utf8_lossy(want)
                )),
            }
        };
        let info_n = take(b"INFO", false)?.expect("required");
        let hier_n = take(b"HIER", false)?.expect("required");
        let mtrl_n = take(b"MTRL", false)?.expect("required");
        let bshp_n = take(b"BSHP", true)?;
        let segm_n = take(b"SEGM", false)?.expect("required");
        let phy2_n = take(b"PHY2", true)?;
        let geom_n = take(b"GEOM", false)?.expect("required");
        if !rest.is_empty() {
            return Err(format!(
                "top level is [{}]; a model is [INFO, HIER, MTRL, (BSHP), SEGM, (PHY2), GEOM] and {} follows GEOM",
                tags(&roots),
                rest[0].tag_str()
            ));
        }

        let info_b = leaf(info_n, b"INFO", "top level")?;
        exact_len(info_b, INFO_BYTES, "top-level INFO")?;
        let info = ModelInfo {
            header_word: u32_at(info_b, 0x00),
            bbox_min: f32s(info_b, 0x04),
            bbox_max: f32s(info_b, 0x10),
            object_flags: ObjectFlags::from_bits(u32_at(info_b, 0x1C))?,
            slot_count: u32_at(info_b, 0x2C),
            lod_count: u32_at(info_b, 0x34),
            lod_base_distance: f32_at(info_b, 0x38),
            fade_sharpness: f32_at(info_b, 0x3C),
            block_lod_masks: std::array::from_fn(|k| u16_at(info_b, 0x40 + 2 * k)),
        };
        let node_count = u32_at(info_b, 0x20) as usize;
        let material_count = u32_at(info_b, 0x24) as usize;
        let segm_count = u32_at(info_b, 0x28) as usize;
        let channel_count = u32_at(info_b, 0x30) as usize;

        let hier_b = leaf(hier_n, b"HIER", "top level")?;
        exact_len(hier_b, node_count * HIER_NODE_BYTES, &format!("HIER of {node_count} nodes (INFO +0x20)"))?;
        let hier = hier_b
            .chunks_exact(HIER_NODE_BYTES)
            .enumerate()
            .map(|(k, n)| decode_hier_node(n, k))
            .collect::<Result<Vec<_>, _>>()?;

        let mtrl = decode_materials(leaf(mtrl_n, b"MTRL", "top level")?, material_count)?;

        let bshp = match bshp_n {
            None => Vec::new(),
            Some(n) => {
                let b = leaf(n, b"BSHP", "top level")?;
                if b.is_empty() {
                    return Err("top-level BSHP is empty; a model without channels carries no BSHP".into());
                }
                decode_channels(b)?
            }
        };
        if bshp.len() != channel_count {
            return Err(format!("INFO +0x30 counts {channel_count} channels and BSHP holds {}", bshp.len()));
        }

        let segm_b = leaf(segm_n, b"SEGM", "top level")?;
        exact_len(segm_b, segm_count * SEGM_ROW_BYTES, &format!("SEGM of {segm_count} rows (INFO +0x28)"))?;
        let segm = segm_b
            .chunks_exact(SEGM_ROW_BYTES)
            .map(|r| SegmRow { node: u16_at(r, 0) as i16, slot: r[2], lod_mask: r[3] })
            .collect();

        let phy2 = phy2_n.map(|n| decode_phy2(leaf(n, b"PHY2", "top level")?)).transpose()?;
        let geom = decode_geom(geom_n)?;

        Ok(Model { info, hier, mtrl, bshp, segm, phy2, geom })
    }

    /// Encode to a UCFX container. Refuses a model whose lists cannot be written as the counts the
    /// container stores (see [`Geom::encode_tree`]).
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut info = Vec::with_capacity(INFO_BYTES);
        put_u32(&mut info, self.info.header_word);
        put_f32s(&mut info, &self.info.bbox_min);
        put_f32s(&mut info, &self.info.bbox_max);
        put_u32(&mut info, self.info.object_flags.bits());
        for w in [
            self.hier.len() as u32,
            self.mtrl.len() as u32,
            self.segm.len() as u32,
            self.info.slot_count,
            self.bshp.len() as u32,
            self.info.lod_count,
        ] {
            put_u32(&mut info, w);
        }
        put_f32s(&mut info, &[self.info.lod_base_distance, self.info.fade_sharpness]);
        put_u16s(&mut info, &self.info.block_lod_masks);

        let mut hier = Vec::with_capacity(self.hier.len() * HIER_NODE_BYTES);
        for n in &self.hier {
            encode_hier_node(&mut hier, n);
        }

        let mut mtrl = Vec::new();
        for (k, m) in self.mtrl.iter().enumerate() {
            encode_material(&mut mtrl, m, k)?;
        }

        let mut top = vec![
            UcfxNode::leaf(*b"INFO", info),
            UcfxNode::leaf(*b"HIER", hier),
            UcfxNode::leaf(*b"MTRL", mtrl),
        ];
        if !self.bshp.is_empty() {
            let mut b = Vec::with_capacity(self.bshp.len() * BSHP_RECORD_BYTES);
            for c in &self.bshp {
                put_u32(&mut b, c.name_hash);
                put_u32(&mut b, 0);
            }
            top.push(UcfxNode::leaf(*b"BSHP", b));
        }
        let mut segm = Vec::with_capacity(self.segm.len() * SEGM_ROW_BYTES);
        for r in &self.segm {
            put_u16(&mut segm, r.node as u16);
            segm.push(r.slot);
            segm.push(r.lod_mask);
        }
        top.push(UcfxNode::leaf(*b"SEGM", segm));
        if let Some(p) = &self.phy2 {
            top.push(UcfxNode::leaf(*b"PHY2", encode_phy2(p)?));
        }
        top.push(self.geom.encode_tree()?);
        Ok(write_ucfx_tree(&top))
    }
}

/// How a LOD-block container ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trailer {
    /// `CSUM` and the CRC of everything before it.
    Csum,
    /// Eight zero bytes where the `CSUM` trailer sits. One retail LOD block ends this way
    /// (`0x402B00E9`, block 4156): its bodies end exactly 8 bytes before the container end.
    Zeroed,
}

/// A model's `_P001_Q2` / `_P002_Q1` LOD-block container: a top level of exactly one `GEOM`.
#[derive(Debug, Clone, PartialEq)]
pub struct LodBlock {
    pub geom: Geom,
    pub trailer: Trailer,
}

impl LodBlock {
    /// Decode a LOD-block container, strictly as [`Model::decode`] decodes a model's `GEOM`.
    pub fn decode(container: &[u8]) -> Result<LodBlock, String> {
        let at = container.len().saturating_sub(UCFX_CSUM_BYTES);
        let (trailer, roots) = if container.len() >= UCFX_CSUM_BYTES && container[at..].iter().all(|&b| b == 0) {
            let mut sealed = container[..at].to_vec();
            let sum = crc32_mercs2(&sealed);
            sealed.extend_from_slice(b"CSUM");
            sealed.extend_from_slice(&sum.to_le_bytes());
            (Trailer::Zeroed, parse_ucfx_tree(&sealed)?)
        } else {
            (Trailer::Csum, parse_ucfx_tree(container)?)
        };
        for r in &roots {
            if let Some(e) = refused(&r.tag, "top level") {
                return Err(e);
            }
        }
        match &roots[..] {
            [g] if &g.tag == b"GEOM" => Ok(LodBlock { geom: decode_geom(g)?, trailer }),
            _ => Err(format!("top level is [{}]; a LOD-block container is [GEOM]", tags(&roots))),
        }
    }

    /// Encode to a LOD-block container.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut c = write_ucfx_tree(&[self.geom.encode_tree()?]);
        if self.trailer == Trailer::Zeroed {
            let at = c.len() - UCFX_CSUM_BYTES;
            c[at..].fill(0);
        }
        Ok(c)
    }
}

impl Geom {
    /// The `GEOM` node. Refuses an `INDX` list whose length differs from the sub-object count, a
    /// `SKIN` group whose ranges and unread range slots are not [`SKIN_RANGE_SLOTS`] in all, and a
    /// vertex stream whose bytes are not a whole number of vertices.
    pub fn encode_tree(&self) -> Result<UcfxNode, String> {
        if self.indx.len() != self.subs.len() {
            return Err(format!("GEOM INDX has {} entries for {} sub-objects", self.indx.len(), self.subs.len()));
        }
        let mut indx = Vec::with_capacity(2 * self.indx.len());
        put_u16s(&mut indx, &self.indx);
        let mut kids = vec![
            UcfxNode::leaf(*b"INFO", u32_bytes(self.subs.len() as u32)),
            UcfxNode::leaf(*b"INDX", indx),
        ];
        for (k, s) in self.subs.iter().enumerate() {
            let at = format!("sub-object {k}");
            let (tag, groups) = match s {
                SubObject::Mesh { groups } => (
                    *b"MESH",
                    groups
                        .iter()
                        .enumerate()
                        .map(|(g, x)| encode_mesh_group(x, &format!("{at} group {g}")))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                SubObject::Skin { groups } => (
                    *b"SKIN",
                    groups
                        .iter()
                        .enumerate()
                        .map(|(g, x)| encode_skin_group(x, &format!("{at} group {g}")))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };
            let mut sub = vec![UcfxNode::leaf(*b"INFO", u32_bytes(groups.len() as u32))];
            sub.extend(groups);
            kids.push(UcfxNode::marker(tag, sub));
        }
        Ok(UcfxNode::marker(*b"GEOM", kids))
    }
}

fn decode_hier_node(n: &[u8], k: usize) -> Result<HierNode, String> {
    let at = format!("HIER node {k}");
    let flags = u16_at(n, 0x04);
    if flags & !1 != 0 {
        return Err(format!("{at} +0x04 flags 0x{flags:04X}: only bit 0 (skeleton) is decoded"));
    }
    let w0c = u32_at(n, 0x0C);
    if w0c != 0 {
        return Err(format!("{at} +0x0C is 0x{w0c:08X}; it is 0"));
    }
    for o in [0x9C, 0xAC] {
        let w = u32_at(n, o);
        if w != HIER_BOUND_W {
            return Err(format!("{at} +0x{o:02X} is 0x{w:08X}; it is 1.0 (0x{HIER_BOUND_W:08X})"));
        }
    }
    Ok(HierNode {
        name_hash: u32_at(n, 0x00),
        skeleton: flags & 1 != 0,
        first_child: opt_index(u16_at(n, 0x06)),
        parent: opt_index(u16_at(n, 0x08)),
        next_sibling: opt_index(u16_at(n, 0x0A)),
        local: f32s(n, 0x10),
        inverse_bind: f32s(n, 0x50),
        bbox_min: f32s(n, 0x90),
        bbox_max: f32s(n, 0xA0),
    })
}

fn encode_hier_node(out: &mut Vec<u8>, n: &HierNode) {
    put_u32(out, n.name_hash);
    put_u16(out, n.skeleton as u16);
    for link in [n.first_child, n.parent, n.next_sibling] {
        put_u16(out, link.unwrap_or(0xFFFF));
    }
    put_u32(out, 0);
    put_f32s(out, &n.local);
    put_f32s(out, &n.inverse_bind);
    put_f32s(out, &n.bbox_min);
    put_u32(out, HIER_BOUND_W);
    put_f32s(out, &n.bbox_max);
    put_u32(out, HIER_BOUND_W);
}

fn decode_materials(body: &[u8], count: usize) -> Result<Vec<Material>, String> {
    let mut out = Vec::with_capacity(count);
    let mut p = 0usize;
    for k in 0..count {
        let at = format!("MTRL material {k} at +{p}");
        if p + MTRL_RECORD_BASE_BYTES > body.len() {
            return Err(format!("{at} needs {MTRL_RECORD_BASE_BYTES} bytes and the leaf has {}", body.len() - p));
        }
        let r = &body[p..];
        let tex_count = u16_at(r, 106) as usize;
        if tex_count > MTRL_MAX_TEXTURES {
            return Err(format!("{at} names {tex_count} textures; Mtrl_Parse fills {MTRL_MAX_TEXTURES} slots"));
        }
        let len = MTRL_RECORD_BASE_BYTES + 4 * tex_count;
        if p + len > body.len() {
            return Err(format!("{at} with {tex_count} textures needs {len} bytes and the leaf has {}", body.len() - p));
        }
        let flags = MaterialFlags::from_bits(u16_at(r, 104)).map_err(|e| format!("{at}: {e}"))?;
        let tail = 108 + 4 * tex_count;
        out.push(Material {
            name_hash: u32_at(r, 0),
            diffuse: f32s(r, 4),
            specular: f32s(r, 16),
            ambient: f32s(r, 28),
            emissive: f32s(r, 40),
            opacity: f32_at(r, 52),
            specular_power: f32_at(r, 56),
            reflection_intensity: f32_at(r, 60),
            refraction_index: f32_at(r, 64),
            subsurface: f32s(r, 68),
            uv_offset: f32s(r, 84),
            uv_scale: f32s(r, 92),
            uv_rotation: f32_at(r, 100),
            flags,
            textures: (0..tex_count).map(|t| u32_at(r, 108 + 4 * t)).collect(),
            pixel_shader: u32_at(r, tail),
            surface: u32_at(r, tail + 4),
        });
        p += len;
    }
    if p != body.len() {
        return Err(format!(
            "MTRL holds {} bytes and its {count} materials (INFO +0x24) at 116 + 4·textures each fill {p}",
            body.len()
        ));
    }
    Ok(out)
}

fn encode_material(out: &mut Vec<u8>, m: &Material, k: usize) -> Result<(), String> {
    if m.textures.len() > MTRL_MAX_TEXTURES {
        return Err(format!(
            "material {k} names {} textures; Mtrl_Parse fills {MTRL_MAX_TEXTURES} slots",
            m.textures.len()
        ));
    }
    put_u32(out, m.name_hash);
    put_f32s(out, &m.diffuse);
    put_f32s(out, &m.specular);
    put_f32s(out, &m.ambient);
    put_f32s(out, &m.emissive);
    put_f32s(out, &[m.opacity, m.specular_power, m.reflection_intensity, m.refraction_index]);
    put_f32s(out, &m.subsurface);
    put_f32s(out, &m.uv_offset);
    put_f32s(out, &m.uv_scale);
    put_f32s(out, &[m.uv_rotation]);
    put_u16(out, m.flags.bits());
    put_u16(out, m.textures.len() as u16);
    for &t in &m.textures {
        put_u32(out, t);
    }
    put_u32(out, m.pixel_shader);
    put_u32(out, m.surface);
    Ok(())
}

fn decode_channels(b: &[u8]) -> Result<Vec<BlendChannel>, String> {
    if !b.len().is_multiple_of(BSHP_RECORD_BYTES) {
        return Err(format!("top-level BSHP is {} bytes, not a multiple of {BSHP_RECORD_BYTES}", b.len()));
    }
    b.chunks_exact(BSHP_RECORD_BYTES)
        .enumerate()
        .map(|(k, r)| {
            let w = u32_at(r, 4);
            if w != 0 {
                return Err(format!("top-level BSHP channel {k} +4 is 0x{w:08X}; it is 0"));
            }
            Ok(BlendChannel { name_hash: u32_at(r, 0) })
        })
        .collect()
}

fn decode_phy2(b: &[u8]) -> Result<Phy2, String> {
    if b.len() < PHY2_PREFIX_BYTES {
        return Err(format!("PHY2 is {} bytes; its prefix alone is {PHY2_PREFIX_BYTES}", b.len()));
    }
    let w = |k: usize| u32_at(b, 4 * k);
    let packfile_len = w(8) as usize;
    let end = PHY2_PREFIX_BYTES
        .checked_add(packfile_len)
        .filter(|&e| e <= b.len())
        .ok_or_else(|| format!("PHY2 w8 packfile size {packfile_len} runs past the {}-byte chunk", b.len()))?;
    Ok(Phy2 {
        prefix: Phy2Prefix {
            header_word: w(0),
            name_hash: w(1),
            node_records: w(2),
            internal_nodes: w(3),
            leaf_nodes: w(4),
            linked_records: w(5),
            hull_planes: w(6),
            vertex_total: w(7),
            reserved: [w(9), w(10), w(11)],
        },
        packfile: b[PHY2_PREFIX_BYTES..end].to_vec(),
        wrapper: b[end..].to_vec(),
    })
}

fn encode_phy2(p: &Phy2) -> Result<Vec<u8>, String> {
    let packfile_len = u32::try_from(p.packfile.len())
        .map_err(|_| format!("PHY2 packfile of {} bytes does not fit w8", p.packfile.len()))?;
    let x = &p.prefix;
    let mut b = Vec::with_capacity(PHY2_PREFIX_BYTES + p.packfile.len() + p.wrapper.len());
    for w in [
        x.header_word,
        x.name_hash,
        x.node_records,
        x.internal_nodes,
        x.leaf_nodes,
        x.linked_records,
        x.hull_planes,
        x.vertex_total,
        packfile_len,
        x.reserved[0],
        x.reserved[1],
        x.reserved[2],
    ] {
        put_u32(&mut b, w);
    }
    b.extend_from_slice(&p.packfile);
    b.extend_from_slice(&p.wrapper);
    Ok(b)
}

fn decode_geom(n: &UcfxNode) -> Result<Geom, String> {
    let kids = marker(n, b"GEOM", "top level")?;
    if kids.len() < 2 {
        return Err(format!("GEOM is [{}]; it is [INFO, INDX, sub-objects…]", tags(kids)));
    }
    let count = count_word(leaf(&kids[0], b"INFO", "GEOM")?, "GEOM INFO")?;
    let indx_b = leaf(&kids[1], b"INDX", "GEOM")?;
    exact_len(indx_b, 2 * count, &format!("GEOM INDX for {count} sub-objects"))?;
    let subs_n = &kids[2..];
    if subs_n.len() != count {
        return Err(format!("GEOM INFO counts {count} sub-objects and GEOM holds {}", subs_n.len()));
    }
    let mut subs = Vec::with_capacity(count);
    for (k, s) in subs_n.iter().enumerate() {
        let at = format!("GEOM sub-object {k}");
        if let Some(e) = refused(&s.tag, &at) {
            return Err(e);
        }
        let groups_n = match &s.tag {
            b"MESH" => marker(s, b"MESH", &at)?,
            b"SKIN" => marker(s, b"SKIN", &at)?,
            other => {
                return Err(format!("{at} is {}; a sub-object is MESH or SKIN", String::from_utf8_lossy(other)));
            }
        };
        let Some((first, groups_n)) = groups_n.split_first() else {
            return Err(format!("{at} is empty; it starts with INFO"));
        };
        let group_count = count_word(leaf(first, b"INFO", &at)?, &format!("{at} INFO"))?;
        if group_count != groups_n.len() {
            return Err(format!("{at} INFO counts {group_count} groups and it holds {}", groups_n.len()));
        }
        subs.push(if &s.tag == b"MESH" {
            SubObject::Mesh {
                groups: groups_n
                    .iter()
                    .enumerate()
                    .map(|(g, x)| decode_mesh_group(x, &format!("{at} group {g}")))
                    .collect::<Result<_, _>>()?,
            }
        } else {
            SubObject::Skin {
                groups: groups_n
                    .iter()
                    .enumerate()
                    .map(|(g, x)| decode_skin_group(x, &format!("{at} group {g}")))
                    .collect::<Result<_, _>>()?,
            }
        });
    }
    Ok(Geom { indx: u16_list(indx_b), subs })
}

fn decode_decl(b: &[u8], at: &str) -> Result<Vec<DeclElement>, String> {
    if !b.len().is_multiple_of(DECL_ELEMENT_BYTES) || b.is_empty() {
        return Err(format!("{at} decl is {} bytes; it is 8 per element, END included", b.len()));
    }
    let n = b.len() / DECL_ELEMENT_BYTES;
    if b[b.len() - DECL_ELEMENT_BYTES..] != DECL_END {
        return Err(format!("{at} decl does not end with D3DDECL_END"));
    }
    (0..n - 1)
        .map(|k| {
            let e = &b[k * DECL_ELEMENT_BYTES..(k + 1) * DECL_ELEMENT_BYTES];
            let ea = format!("{at} decl element {k}");
            let ty = DeclType::from_u8(e[4]).ok_or_else(|| format!("{ea} type {} is not a D3DDECLTYPE", e[4]))?;
            if e[5] != 0 {
                return Err(format!("{ea} method {} is not D3DDECLMETHOD_DEFAULT", e[5]));
            }
            let usage = DeclUsage::from_u8(e[6]).ok_or_else(|| format!("{ea} usage {} is not a D3DDECLUSAGE", e[6]))?;
            Ok(DeclElement { stream: u16_at(e, 0), offset: u16_at(e, 2), ty, usage, usage_index: e[7] })
        })
        .collect()
}

fn encode_decl(elements: &[DeclElement]) -> Vec<u8> {
    let mut b = Vec::with_capacity((elements.len() + 1) * DECL_ELEMENT_BYTES);
    for e in elements {
        put_u16(&mut b, e.stream);
        put_u16(&mut b, e.offset);
        b.extend_from_slice(&[e.ty.code(), 0, e.usage.code(), e.usage_index]);
    }
    b.extend_from_slice(&DECL_END);
    b
}

fn decode_stream(n: &UcfxNode, at: &str) -> Result<VertexStream, String> {
    let kids = marker(n, b"STRM", at)?;
    if kids.len() != 3 {
        return Err(format!("{at} STRM is [{}]; it is [info, decl, data]", tags(kids)));
    }
    let info = leaf(&kids[0], b"info", at)?;
    exact_len(info, 12, &format!("{at} STRM info"))?;
    let elements = decode_decl(leaf(&kids[1], b"decl", at)?, &format!("{at} STRM"))?;
    let data = leaf(&kids[2], b"data", at)?;
    let stream = VertexStream { elements, data: data.to_vec() };
    let (w0, w1, w2) = (u32_at(info, 0) as usize, u32_at(info, 4) as usize, u32_at(info, 8) as usize);
    if w0 != stream.elements.len() + 1 {
        return Err(format!(
            "{at} STRM info +0 is {w0}; the decl has {} elements including END",
            stream.elements.len() + 1
        ));
    }
    if w1 != stream.stride() {
        return Err(format!("{at} STRM info +4 stride is {w1}; the stream-0 elements sum to {}", stream.stride()));
    }
    exact_len(data, w1 * w2, &format!("{at} STRM data of {w2} vertices at stride {w1}"))?;
    Ok(stream)
}

fn encode_stream(s: &VertexStream, at: &str) -> Result<UcfxNode, String> {
    let stride = s.stride();
    if stride == 0 || !s.data.len().is_multiple_of(stride) {
        return Err(format!("{at} STRM data of {} bytes is not a whole number of {stride}-byte vertices", s.data.len()));
    }
    let mut info = Vec::with_capacity(12);
    for w in [s.elements.len() + 1, stride, s.data.len() / stride] {
        put_u32(&mut info, w as u32);
    }
    Ok(UcfxNode::marker(
        *b"STRM",
        vec![
            UcfxNode::leaf(*b"info", info),
            UcfxNode::leaf(*b"decl", encode_decl(&s.elements)),
            UcfxNode::leaf(*b"data", s.data.clone()),
        ],
    ))
}

fn decode_prims(b: &[u8], a: usize, bcount: usize, at: &str) -> Result<(Vec<PrimRecord>, Vec<PrimRecord>), String> {
    exact_len(b, PRMT_RECORD_BYTES * (a + bcount), &format!("{at} PRMT of {a} + {bcount} records (INFO +0/+4)"))?;
    let mut all: Vec<PrimRecord> = b
        .chunks_exact(PRMT_RECORD_BYTES)
        .map(|r| PrimRecord {
            material: u32_at(r, 0),
            start: u32_at(r, 4),
            prims: u16_at(r, 8),
            min_index: u16_at(r, 10),
            max_index: u16_at(r, 12),
            unique_count: u16_at(r, 14),
        })
        .collect();
    let list_b = all.split_off(a);
    Ok((all, list_b))
}

fn encode_prims(a: &[PrimRecord], b: &[PrimRecord]) -> Vec<u8> {
    let mut out = Vec::with_capacity(PRMT_RECORD_BYTES * (a.len() + b.len()));
    for p in a.iter().chain(b) {
        put_u32(&mut out, p.material);
        put_u32(&mut out, p.start);
        put_u16s(&mut out, &[p.prims, p.min_index, p.max_index, p.unique_count]);
    }
    out
}

fn decode_mesh_group(n: &UcfxNode, at: &str) -> Result<MeshGroup, String> {
    let kids = marker(n, b"PRMG", at)?;
    if kids.len() != 5 {
        return Err(format!("{at} is [{}]; a MESH group is [INFO, STRM, AREA, IBUF, PRMT]", tags(kids)));
    }
    let info = leaf(&kids[0], b"INFO", at)?;
    exact_len(info, MESH_GROUP_INFO_BYTES, &format!("{at} INFO"))?;
    let morphs = u32_at(info, 8);
    if morphs != 0 {
        return Err(format!("{at} INFO +0x08 counts {morphs} morph targets; no retail MESH group has morph targets"));
    }
    let stream = decode_stream(&kids[1], at)?;
    let area = counted_u16s(&kids[2], b"AREA", at)?;
    let strip = counted_u16s(&kids[3], b"IBUF", at)?;
    let (list_a, list_b) =
        decode_prims(leaf(&kids[4], b"PRMT", at)?, u32_at(info, 0) as usize, u32_at(info, 4) as usize, at)?;
    Ok(MeshGroup {
        vertex_shader: u32_at(info, 0x0C),
        second_vertex_shader: u32_at(info, 0x10),
        center: f32s(info, 0x14),
        radius: f32_at(info, 0x20),
        bbox_min: f32s(info, 0x24),
        bbox_max: f32s(info, 0x30),
        stream,
        area,
        strip,
        list_a,
        list_b,
    })
}

fn encode_mesh_group(g: &MeshGroup, at: &str) -> Result<UcfxNode, String> {
    let mut info = Vec::with_capacity(MESH_GROUP_INFO_BYTES);
    for w in [g.list_a.len() as u32, g.list_b.len() as u32, 0, g.vertex_shader, g.second_vertex_shader] {
        put_u32(&mut info, w);
    }
    put_f32s(&mut info, &g.center);
    put_f32s(&mut info, &[g.radius]);
    put_f32s(&mut info, &g.bbox_min);
    put_f32s(&mut info, &g.bbox_max);
    Ok(UcfxNode::marker(
        *b"PRMG",
        vec![
            UcfxNode::leaf(*b"INFO", info),
            encode_stream(&g.stream, at)?,
            counted_u16s_node(*b"AREA", &g.area),
            counted_u16s_node(*b"IBUF", &g.strip),
            UcfxNode::leaf(*b"PRMT", encode_prims(&g.list_a, &g.list_b)),
        ],
    ))
}

fn decode_morph_stream(n: &UcfxNode, at: &str) -> Result<Vec<[u16; 4]>, String> {
    let kids = marker(n, b"BSHP", at)?;
    if kids.len() != 3 {
        return Err(format!("{at} BSHP is [{}]; it is [info, decl, data]", tags(kids)));
    }
    let info = leaf(&kids[0], b"info", at)?;
    exact_len(info, 12, &format!("{at} BSHP info"))?;
    let (w0, w1, count) = (u32_at(info, 0), u32_at(info, 4) as usize, u32_at(info, 8) as usize);
    if w0 != 1 || w1 != MORPH_STRIDE {
        return Err(format!(
            "{at} BSHP info is {{{w0}, {w1}, {count}}}; a morph stream is {{1, {MORPH_STRIDE}, vertex count}}"
        ));
    }
    if leaf(&kids[1], b"decl", at)? != DECL_END {
        return Err(format!("{at} BSHP decl is not D3DDECL_END alone"));
    }
    let data = leaf(&kids[2], b"data", at)?;
    exact_len(data, MORPH_STRIDE * count, &format!("{at} BSHP data of {count} deltas"))?;
    Ok(data.chunks_exact(MORPH_STRIDE).map(|d| std::array::from_fn(|k| u16_at(d, 2 * k))).collect())
}

fn encode_morph_stream(deltas: &[[u16; 4]]) -> UcfxNode {
    let mut info = Vec::with_capacity(12);
    for w in [1, MORPH_STRIDE as u32, deltas.len() as u32] {
        put_u32(&mut info, w);
    }
    let mut data = Vec::with_capacity(MORPH_STRIDE * deltas.len());
    for d in deltas {
        put_u16s(&mut data, d);
    }
    UcfxNode::marker(
        *b"BSHP",
        vec![
            UcfxNode::leaf(*b"info", info),
            UcfxNode::leaf(*b"decl", DECL_END.to_vec()),
            UcfxNode::leaf(*b"data", data),
        ],
    )
}

fn decode_skin_group(n: &UcfxNode, at: &str) -> Result<SkinGroup, String> {
    let kids = marker(n, b"PRMG", at)?;
    let shape_err = || {
        format!(
            "{at} is [{}]; a SKIN group is [INFO, STRM, IBUF, PRMT] or [INFO, STRM, IBUF, BSHP × n, BSHI, PRMT]",
            tags(kids)
        )
    };
    if kids.len() < 4 {
        return Err(shape_err());
    }
    let info = leaf(&kids[0], b"INFO", at)?;
    exact_len(info, SKIN_GROUP_INFO_BYTES, &format!("{at} INFO"))?;
    let morph_count = u32_at(info, 8) as usize;
    let expected = if morph_count == 0 { 4 } else { 5 + morph_count };
    if kids.len() != expected {
        return Err(format!("{} (INFO +0x08 counts {morph_count} morph targets)", shape_err()));
    }
    let stream = decode_stream(&kids[1], at)?;
    let strip = counted_u16s(&kids[2], b"IBUF", at)?;
    let mut morphs = Vec::with_capacity(morph_count);
    if morph_count > 0 {
        let bshi = leaf(&kids[3 + morph_count], b"BSHI", at)?;
        exact_len(bshi, 2 * morph_count, &format!("{at} BSHI of {morph_count} channels"))?;
        for k in 0..morph_count {
            morphs.push(MorphTarget {
                channel: u16_at(bshi, 2 * k),
                deltas: decode_morph_stream(&kids[3 + k], &format!("{at} morph {k}"))?,
            });
        }
    }
    let (list_a, list_b) =
        decode_prims(leaf(&kids[expected - 1], b"PRMT", at)?, u32_at(info, 0) as usize, u32_at(info, 4) as usize, at)?;

    let range_count = u32_at(info, 0x14) as usize;
    if range_count > SKIN_RANGE_SLOTS {
        return Err(format!("{at} INFO +0x14 counts {range_count} bone ranges; the INFO holds {SKIN_RANGE_SLOTS}"));
    }
    let slot = |k: usize| BoneRange { hier_base: u16_at(info, 0x18 + 4 * k), count: u16_at(info, 0x1A + 4 * k) };
    Ok(SkinGroup {
        vertex_shader: u32_at(info, 0x0C),
        second_vertex_shader: u32_at(info, 0x10),
        bone_ranges: (0..range_count).map(slot).collect(),
        unread_range_slots: (range_count..SKIN_RANGE_SLOTS).map(slot).collect(),
        stream,
        strip,
        morphs,
        list_a,
        list_b,
    })
}

fn encode_skin_group(g: &SkinGroup, at: &str) -> Result<UcfxNode, String> {
    if g.bone_ranges.len() + g.unread_range_slots.len() != SKIN_RANGE_SLOTS {
        return Err(format!(
            "{at} has {} bone ranges and {} unread range slots; the INFO holds {SKIN_RANGE_SLOTS} slots",
            g.bone_ranges.len(),
            g.unread_range_slots.len()
        ));
    }
    let mut info = Vec::with_capacity(SKIN_GROUP_INFO_BYTES);
    for w in [
        g.list_a.len() as u32,
        g.list_b.len() as u32,
        g.morphs.len() as u32,
        g.vertex_shader,
        g.second_vertex_shader,
        g.bone_ranges.len() as u32,
    ] {
        put_u32(&mut info, w);
    }
    for r in g.bone_ranges.iter().chain(&g.unread_range_slots) {
        put_u16s(&mut info, &[r.hier_base, r.count]);
    }
    let mut kids = vec![UcfxNode::leaf(*b"INFO", info), encode_stream(&g.stream, at)?, counted_u16s_node(*b"IBUF", &g.strip)];
    if !g.morphs.is_empty() {
        kids.extend(g.morphs.iter().map(|m| encode_morph_stream(&m.deltas)));
        let mut bshi = Vec::with_capacity(2 * g.morphs.len());
        for m in &g.morphs {
            put_u16(&mut bshi, m.channel);
        }
        kids.push(UcfxNode::leaf(*b"BSHI", bshi));
    }
    kids.push(UcfxNode::leaf(*b"PRMT", encode_prims(&g.list_a, &g.list_b)));
    Ok(UcfxNode::marker(*b"PRMG", kids))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [f32; 16] {
        std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 })
    }

    fn node(name_hash: u32, parent: Option<u16>, first_child: Option<u16>, next_sibling: Option<u16>) -> HierNode {
        HierNode {
            name_hash,
            skeleton: parent.is_none(),
            first_child,
            parent,
            next_sibling,
            local: identity(),
            inverse_bind: identity(),
            bbox_min: [-0.01; 3],
            bbox_max: [0.01; 3],
        }
    }

    fn el(stream: u16, offset: u16, ty: DeclType, usage: DeclUsage, usage_index: u8) -> DeclElement {
        DeclElement { stream, offset, ty, usage, usage_index }
    }

    fn prim(material: u32, n: u16) -> PrimRecord {
        PrimRecord { material, start: 0, prims: n - 2, min_index: 0, max_index: n - 1, unique_count: n }
    }

    fn mesh_group() -> MeshGroup {
        let elements = vec![
            el(0, 0, DeclType::Float16x4, DeclUsage::Position, 0),
            el(0, 8, DeclType::Float16x2, DeclUsage::TexCoord, 0),
            el(0, 12, DeclType::Float16x4, DeclUsage::Normal, 0),
        ];
        MeshGroup {
            vertex_shader: 0x1111_1111,
            second_vertex_shader: 0x2222_2222,
            center: [0.5, 0.5, 0.0],
            radius: 0.707,
            bbox_min: [0.0, 0.0, 0.0],
            bbox_max: [1.0, 1.0, 0.0],
            stream: VertexStream { elements, data: (0..4 * 20).map(|b| b as u8).collect() },
            area: vec![0x3800, 0x3800],
            strip: vec![0, 1, 2, 3],
            list_a: vec![prim(0, 4)],
            list_b: vec![prim(0, 4)],
        }
    }

    fn skin_group(morphs: usize) -> SkinGroup {
        let mut elements = vec![
            el(0, 0, DeclType::Float16x4, DeclUsage::Position, 0),
            el(0, 8, DeclType::Float16x2, DeclUsage::TexCoord, 0),
            el(0, 12, DeclType::UByte4, DeclUsage::BlendIndices, 0),
            el(0, 16, DeclType::UByte4N, DeclUsage::BlendWeight, 0),
            el(0, 20, DeclType::Float16x4, DeclUsage::Normal, 0),
        ];
        if morphs > 0 {
            elements.extend((1..=6).map(|k| el(k, 0, DeclType::Float16x4, DeclUsage::TexCoord, k as u8)));
        }
        SkinGroup {
            vertex_shader: 0x3333_3333,
            second_vertex_shader: 0x4444_4444,
            bone_ranges: vec![BoneRange { hier_base: 0, count: 2 }, BoneRange { hier_base: 3, count: 1 }],
            unread_range_slots: (0..6).map(|k| BoneRange { hier_base: 12 * (k % 2), count: 0 }).collect(),
            stream: VertexStream { elements, data: vec![7; 3 * 28] },
            strip: vec![0, 1, 2],
            morphs: (0..morphs)
                .map(|k| MorphTarget { channel: k as u16, deltas: vec![[k as u16, 1, 2, 0x3c00]; 3] })
                .collect(),
            list_a: vec![prim(1, 3), prim(0, 3)],
            list_b: vec![prim(1, 3)],
        }
    }

    fn material(textures: usize) -> Material {
        Material {
            name_hash: 0xabcd_0001,
            diffuse: [1.0; 3],
            specular: [0.5; 3],
            ambient: [1.0; 3],
            emissive: [0.0; 3],
            opacity: 1.0,
            specular_power: 16.0,
            reflection_intensity: 1.0,
            refraction_index: 1.0,
            subsurface: [0.0; 4],
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            uv_rotation: 0.0,
            flags: MaterialFlags::from_bits(0x88).unwrap(),
            textures: (0..textures as u32).map(|t| 0x1000 + t).collect(),
            pixel_shader: 0x5555_5555,
            surface: 0x959d_8470,
        }
    }

    fn sample() -> Model {
        Model {
            info: ModelInfo {
                header_word: 0x39,
                bbox_min: [-1.0; 3],
                bbox_max: [1.0; 3],
                object_flags: ObjectFlags::from_bits(0x890).unwrap(),
                slot_count: 2,
                lod_count: 1,
                lod_base_distance: 30.0,
                fade_sharpness: 5.0,
                block_lod_masks: [0, 0, 0, 1],
            },
            hier: vec![node(0xcbc1_eb51, None, Some(1), None), node(0x1234_5678, Some(0), None, None)],
            mtrl: vec![material(3), material(1)],
            bshp: vec![BlendChannel { name_hash: 0xd3d1_37bc }, BlendChannel { name_hash: 0x416c_2f99 }],
            segm: vec![SegmRow { node: 1, slot: 0, lod_mask: 1 }, SegmRow { node: 0, slot: 1, lod_mask: 1 }],
            phy2: Some(Phy2 {
                prefix: Phy2Prefix {
                    header_word: 0x39,
                    name_hash: 0x0bad_f00d,
                    node_records: 2,
                    internal_nodes: 1,
                    leaf_nodes: 1,
                    linked_records: 0,
                    hull_planes: 0,
                    vertex_total: 8,
                    reserved: [0; 3],
                },
                packfile: vec![0x57, 0xe0, 0xe0, 0x57, 1, 2, 3],
                wrapper: vec![0xaa; 12],
            }),
            geom: Geom {
                indx: vec![0, 1],
                subs: vec![
                    SubObject::Mesh { groups: vec![mesh_group()] },
                    SubObject::Skin { groups: vec![skin_group(2), skin_group(0)] },
                ],
            },
        }
    }

    #[test]
    fn encode_then_decode_is_identity_and_reencodes_to_the_same_bytes() {
        let m = sample();
        let bytes = m.encode().unwrap();
        let back = Model::decode(&bytes).expect("decode");
        assert_eq!(back, m);
        assert_eq!(back.encode().unwrap(), bytes);
    }

    #[test]
    fn the_chunk_layout_and_derived_words_are_written() {
        let bytes = sample().encode().unwrap();
        let tree = parse_ucfx_tree(&bytes).unwrap();
        let top: Vec<String> = tree.iter().map(UcfxNode::tag_str).collect();
        assert_eq!(top, ["INFO", "HIER", "MTRL", "BSHP", "SEGM", "PHY2", "GEOM"]);
        let info = tree[0].body.as_ref().unwrap();
        assert_eq!(info.len(), INFO_BYTES);
        let words: Vec<u32> = (0..7).map(|k| u32_at(info, 0x1C + 4 * k)).collect();
        assert_eq!(words, [0x890, 2, 2, 2, 2, 2, 1]);
        assert_eq!(&info[0x40..0x48], &[0, 0, 0, 0, 0, 0, 1, 0]);
        // HIER: fixed words at +0x0C, +0x9C, +0xAC; absent links as 0xFFFF.
        let hier = tree[1].body.as_ref().unwrap();
        assert_eq!(hier.len(), 2 * HIER_NODE_BYTES);
        assert_eq!((u16_at(hier, 6), u16_at(hier, 8), u16_at(hier, 10)), (1, 0xFFFF, 0xFFFF));
        assert_eq!((u32_at(hier, 0x0C), u32_at(hier, 0x9C), u32_at(hier, 0xAC)), (0, HIER_BOUND_W, HIER_BOUND_W));
        // MTRL: 116 + 4·3 and 116 + 4·1.
        assert_eq!(tree[2].body.as_ref().unwrap().len(), 128 + 120);
        assert_eq!(tree[3].body.as_ref().unwrap(), &[0xbc, 0x37, 0xd1, 0xd3, 0, 0, 0, 0, 0x99, 0x2f, 0x6c, 0x41, 0, 0, 0, 0]);
        // PHY2: w8 = packfile length.
        let phy2 = tree[5].body.as_ref().unwrap();
        assert_eq!(u32_at(phy2, 32), 7);
        assert_eq!(phy2.len(), PHY2_PREFIX_BYTES + 7 + 12);

        let geom = &tree[6].children;
        assert_eq!(geom[0].body.as_ref().unwrap(), &2u32.to_le_bytes());
        let mesh_prmg = &geom[2].children[1].children;
        let tags: Vec<String> = mesh_prmg.iter().map(UcfxNode::tag_str).collect();
        assert_eq!(tags, ["INFO", "STRM", "AREA", "IBUF", "PRMT"]);
        let strm_info = mesh_prmg[1].children[0].body.as_ref().unwrap();
        assert_eq!((u32_at(strm_info, 0), u32_at(strm_info, 4), u32_at(strm_info, 8)), (4, 20, 4));
        let minfo = mesh_prmg[0].body.as_ref().unwrap();
        assert_eq!((u32_at(minfo, 0), u32_at(minfo, 4), u32_at(minfo, 8)), (1, 1, 0));

        let skin_prmg = &geom[3].children[1].children;
        let tags: Vec<String> = skin_prmg.iter().map(UcfxNode::tag_str).collect();
        assert_eq!(tags, ["INFO", "STRM", "IBUF", "BSHP", "BSHP", "BSHI", "PRMT"]);
        let sinfo = skin_prmg[0].body.as_ref().unwrap();
        assert_eq!(sinfo.len(), SKIN_GROUP_INFO_BYTES);
        assert_eq!((u32_at(sinfo, 0), u32_at(sinfo, 4), u32_at(sinfo, 8), u32_at(sinfo, 0x14)), (2, 1, 2, 2));
        assert_eq!(&sinfo[0x18..0x20], &[0, 0, 2, 0, 3, 0, 1, 0]);
        assert_eq!(&sinfo[0x20..0x28], &[0, 0, 0, 0, 12, 0, 0, 0]);
        let strm_info = skin_prmg[1].children[0].body.as_ref().unwrap();
        assert_eq!((u32_at(strm_info, 0), u32_at(strm_info, 4)), (12, 28));
        let morph_info = skin_prmg[3].children[0].body.as_ref().unwrap();
        assert_eq!(morph_info, &[1, 0, 0, 0, 8, 0, 0, 0, 3, 0, 0, 0]);
        let plain = &geom[3].children[2].children;
        let tags: Vec<String> = plain.iter().map(UcfxNode::tag_str).collect();
        assert_eq!(tags, ["INFO", "STRM", "IBUF", "PRMT"]);
    }

    #[test]
    fn a_lod_block_container_round_trips() {
        let block = LodBlock { geom: sample().geom, trailer: Trailer::Csum };
        let bytes = block.encode().unwrap();
        assert_eq!(LodBlock::decode(&bytes).unwrap(), block);
        let err = Model::decode(&bytes).unwrap_err();
        assert!(err.contains("LodBlock::decode"), "{err}");

        let zeroed = LodBlock { trailer: Trailer::Zeroed, ..block };
        let z = zeroed.encode().unwrap();
        assert_eq!(&z[..z.len() - 8], &bytes[..bytes.len() - 8]);
        assert_eq!(&z[z.len() - 8..], &[0; 8]);
        assert_eq!(LodBlock::decode(&z).unwrap(), zeroed);
        assert!(Model::decode(&z).unwrap_err().contains("CSUM"));
    }

    fn with_top(extra: UcfxNode, at: usize) -> Vec<u8> {
        let mut tree = parse_ucfx_tree(&sample().encode().unwrap()).unwrap();
        tree.insert(at, extra);
        write_ucfx_tree(&tree)
    }

    #[test]
    fn destructible_and_tiny_chunks_are_refused_by_name() {
        let err = Model::decode(&with_top(UcfxNode::leaf(*b"STAM", vec![0; 4]), 5)).unwrap_err();
        assert!(err.contains("STAM") && err.contains("destructible"), "{err}");
        let err = Model::decode(&with_top(UcfxNode::leaf(*b"MIXR", vec![0; 4]), 5)).unwrap_err();
        assert!(err.contains("MIXR"), "{err}");
        let err = Model::decode(&with_top(UcfxNode::leaf(*b"TINY", vec![0; 4]), 3)).unwrap_err();
        assert!(err.contains("TINY") && err.contains("tiny_model"), "{err}");

        let mut tree = parse_ucfx_tree(&sample().encode().unwrap()).unwrap();
        tree[6].children[2].tag = *b"SWIT";
        let err = Model::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("SWIT") && err.contains("sub-object 0"), "{err}");
        let mut tree = parse_ucfx_tree(&sample().encode().unwrap()).unwrap();
        tree[6].children[3].tag = *b"TINY";
        let err = Model::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("TINY") && err.contains("sub-object 1"), "{err}");

        let err = Model::decode(&with_top(UcfxNode::leaf(*b"ABCD", vec![]), 7)).unwrap_err();
        assert!(err.contains("ABCD"), "{err}");
    }

    #[test]
    fn the_material_stride_is_116_plus_4_per_texture() {
        let bytes = sample().encode().unwrap();
        // One byte more than two records fill.
        let mut tree = parse_ucfx_tree(&bytes).unwrap();
        tree[2].body.as_mut().unwrap().push(0);
        let err = Model::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("116 + 4·textures"), "{err}");
        // Material 0 claiming 4 textures runs record 1 past the leaf.
        let mut tree = parse_ucfx_tree(&bytes).unwrap();
        tree[2].body.as_mut().unwrap()[106] = 4;
        assert!(Model::decode(&write_ucfx_tree(&tree)).is_err());
        // Eleven textures exceed the slots.
        let mut m = sample();
        m.mtrl[1] = material(11);
        assert!(m.encode().unwrap_err().contains("10 slots"));
    }

    #[test]
    fn stream_info_words_must_match_the_declaration() {
        let bytes = sample().encode().unwrap();
        for (offset, value, needle) in [(0usize, 5u8, "including END"), (4, 24, "stride"), (8, 5, "vertices")] {
            let mut tree = parse_ucfx_tree(&bytes).unwrap();
            tree[6].children[2].children[1].children[1].children[0].body.as_mut().unwrap()[offset] = value;
            let err = Model::decode(&write_ucfx_tree(&tree)).unwrap_err();
            assert!(err.contains(needle), "{needle}: {err}");
        }
        let mut m = sample();
        let SubObject::Mesh { groups } = &mut m.geom.subs[0] else { unreachable!() };
        groups[0].stream.data.push(0);
        assert!(m.encode().unwrap_err().contains("whole number"));
    }

    #[test]
    fn fixed_words_and_undecoded_bits_are_refused() {
        let bytes = sample().encode().unwrap();
        let edit = |f: &dyn Fn(&mut Vec<UcfxNode>)| {
            let mut tree = parse_ucfx_tree(&bytes).unwrap();
            f(&mut tree);
            Model::decode(&write_ucfx_tree(&tree)).unwrap_err()
        };
        assert!(edit(&|t| t[0].body.as_mut().unwrap()[0x1C] = 0x91).contains("+0x1C"));
        assert!(edit(&|t| t[0].body.as_mut().unwrap()[0x20] = 3).contains("HIER of 3 nodes"));
        assert!(edit(&|t| t[1].body.as_mut().unwrap()[4] = 2).contains("+0x04"));
        assert!(edit(&|t| t[1].body.as_mut().unwrap()[0x0C] = 1).contains("+0x0C"));
        assert!(edit(&|t| t[1].body.as_mut().unwrap()[0x9C] = 1).contains("+0x9C"));
        assert!(edit(&|t| t[2].body.as_mut().unwrap()[105] = 0x10).contains("drops"));
        assert!(edit(&|t| t[2].body.as_mut().unwrap()[104] = 0x85).contains("blend mode 5"));
        assert!(edit(&|t| t[3].body.as_mut().unwrap()[4] = 1).contains("BSHP channel 0"));
        let mut m = sample();
        let SubObject::Skin { groups } = &mut m.geom.subs[1] else { unreachable!() };
        groups[0].unread_range_slots.pop();
        assert!(m.encode().unwrap_err().contains("unread range slots"));
        let err = edit(&|t| t[6].children[2].children[1].children[0].body.as_mut().unwrap()[8] = 1);
        assert!(err.contains("morph"), "{err}");
    }

    #[test]
    fn flag_words_round_trip_through_their_named_bits() {
        for bits in [0x6B0, 0x290, 0x2B0, 0x890, 0x90, 0xA90, 0xB0, 0x280] {
            assert_eq!(ObjectFlags::from_bits(bits).unwrap().bits(), bits);
        }
        for bits in 0u16..0x1000 {
            match MaterialFlags::from_bits(bits) {
                Ok(f) => assert_eq!(f.bits(), bits),
                Err(_) => assert!(bits & 7 > 4),
            }
        }
        let f = MaterialFlags::from_bits(0x1F9).unwrap();
        assert_eq!(f.blend, BlendMode::Alpha);
        assert!(f.alpha_test && f.two_sided && f.casts_shadow && f.refraction);
        assert_eq!(f.blend_factors, BlendFactors::Premultiplied);
    }
}
