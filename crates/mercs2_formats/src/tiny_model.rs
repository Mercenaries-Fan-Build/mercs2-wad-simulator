//! The TINY model container: the far-distance stand-in mesh a `TinyGeometryObject` placement draws
//! for the world objects of one 200 m grid cell of one layer. Decoded in full and encoded back to
//! the same bytes.
//!
//! The container is a UCFX tree ([`crate::ucfx::parse_ucfx_tree`]) of this exact shape:
//!
//! ```text
//! INFO  72 B   model info: flags, bounds, counts, constants
//! HIER  176 B per node: one node per sub-object, named m2("pristine") or m2("ruin")
//! MTRL  120 B per material: 104-byte preamble, flags, one texture, pixel shader, trailing word
//! TINY  u32 N, then N world-object GUIDs ascending: the slot list
//! SEGM  4 B per sub-object: {u16 bone, u8 segment, u8 state mask}
//! GEOM
//!   INFO  u32 sub-object count
//!   INDX  u16 per sub-object: the HIER node it hangs from
//!   TINY  (one per sub-object)
//!     INFO  u32 group count
//!     PRMG  (one per group)
//!       INFO  60 B: three header words, main and shadow vertex shader, centre, radius, bounds
//!       STRM
//!         info  u32 flag, u32 stride (20), u32 vertex count
//!         decl  32 B: POSITION FLOAT16_4 @0, TEXCOORD FLOAT16_2 @8, NORMAL FLOAT16_4 @12
//!         data  20 B per vertex
//!       IBUF
//!         info  u32 index count
//!         data  u16 triangle strip
//!       PRMT  16 B per primitive record
//! ```
//!
//! `POSITION.w` of every vertex is the slot of the world object it belongs to in the top-level
//! `TINY` list; the vertex shaders `PgMeshTinyVP` / `PgMeshTinyVP_Ruin` read that slot's state from
//! `ObjectIDScaleArray`. Floats and half floats are kept as their stored bits, so a decoded container
//! encodes to the bytes it came from.
//!
//! Counts the tree implies (the `INFO` counts, the `GEOM` `INFO`, a sub-object's `INFO`, the `STRM`
//! vertex count, the `IBUF` index count) are not stored: [`TinyModel::decode`] checks each against
//! what it counts and refuses a container where they disagree, and [`TinyModel::encode`] writes them
//! from the data.

use crate::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode};

/// The vertex declaration of every TINY group: POSITION FLOAT16_4 at 0, TEXCOORD FLOAT16_2 at 8,
/// NORMAL FLOAT16_4 at 12, end. Each element is `[u16 stream][u16 offset][u8 type][u8 method]
/// [u8 usage][u8 usage index]`.
pub const TINY_DECL: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, // POSITION FLOAT16_4 @0
    0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00, // TEXCOORD FLOAT16_2 @8
    0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00, // NORMAL   FLOAT16_4 @12
    0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, // END
];

/// Bytes per vertex under [`TINY_DECL`].
pub const TINY_STRIDE: usize = 20;

/// Bytes of the top-level `INFO`.
pub const INFO_BYTES: usize = 72;
/// Bytes of one `HIER` node.
pub const HIER_NODE_BYTES: usize = 176;
/// Bytes of a material's preamble, before its flags word.
pub const MTRL_PREAMBLE_BYTES: usize = 104;
/// Bytes of a group's `INFO`.
pub const GROUP_INFO_BYTES: usize = 60;
/// Bytes of one `PRMT` record.
pub const PRMT_RECORD_BYTES: usize = 16;

// ── what every retail TINY container carries ────────────────────────────────────────────────────
//
// `tests/tiny_model_retail.rs` checks each of these against all 1,208 TINY containers of `vz.wad`.

/// The top-level `INFO` flags word.
pub const INFO_FLAGS: u32 = 0x39;
/// The top-level `INFO` word at `+0x1C`.
pub const INFO_WORD_1C: u32 = 0x6b0;
/// The top-level `INFO` words at `+0x30` .. `+0x44` (`+0x38` is 10000.0, `+0x3C` is 5.0).
pub const INFO_TAIL: [u32; 6] = [0, 1, 0x461c_4000, 0x40a0_0000, 0, 0x10000];
/// The `HIER` node of the intact sub-object is named `m2("pristine")`, the ruined one `m2("ruin")`.
pub const NODE_PRISTINE: &str = "pristine";
pub const NODE_RUIN: &str = "ruin";
/// A material's preamble after its name word.
pub const MATERIAL_PREAMBLE: [u32; 25] = [
    0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000,
    0x3f80_0000, 0x3f80_0000, 0, 0, 0, 0x3f80_0000, 0x4180_0000, 0x3f80_0000, 0x3f80_0000, 0, 0, 0,
    0, 0, 0, 0x3f80_0000, 0x3f80_0000, 0,
];
/// The flags of an opaque material.
pub const MATERIAL_OPAQUE: u16 = 0x80;
/// The flags of an alpha-tested material: `0x08` is in the alpha mask `0x0B` that selects the `Tex`
/// shadow shaders.
pub const MATERIAL_ALPHATEST: u16 = 0x88;
/// The pixel shader of every material.
pub const MATERIAL_PIXEL_SHADER: &str = "PgDiffFP";
/// The word after the pixel shader key: `m2("ANY")`.
pub const MATERIAL_TRAILING: &str = "ANY";
/// A group `INFO`'s first three words.
pub const GROUP_HEADER: [u32; 3] = [1, 1, 0];
/// A group's `STRM` `info` flag word.
pub const STREAM_FLAG: u32 = 4;

/// The name a material's first word hashes: `tinygeometry_tgr<row>_tgc<col>_opaque` or
/// `…_alphatest`, two digits each.
pub fn material_name(row: u32, col: u32, alphatest: bool) -> String {
    format!(
        "tinygeometry_tgr{row:02}_tgc{col:02}_{}",
        if alphatest { "alphatest" } else { "opaque" }
    )
}

/// The `HIER` node of sub-object `k` of `count`: flags 1, no parent, the next node as its sibling
/// (none for the last), two identity matrices (the first with `-0.0` at element 2), and the
/// sub-object's bounds with a fourth component of 1.
pub fn hier_node(name_hash: u32, k: usize, count: usize, bbox_min: [f32; 3], bbox_max: [f32; 3]) -> HierNode {
    let identity: [f32; 16] = std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 });
    let mut matrix_a = identity;
    matrix_a[2] = -0.0;
    HierNode {
        name_hash,
        flags: 1,
        parent: -1,
        sibling: if k + 1 < count { (k + 1) as i16 } else { -1 },
        word_0c: 0,
        matrix_a,
        matrix_b: identity,
        bbox_min: [bbox_min[0], bbox_min[1], bbox_min[2], 1.0],
        bbox_max: [bbox_max[0], bbox_max[1], bbox_max[2], 1.0],
    }
}

/// The twin `PRMT` records of a group drawing all of its strip with one material.
pub fn group_prims(material: u32, strip_len: usize, vertex_count: usize) -> Vec<PrimRecord> {
    let rec = PrimRecord {
        material,
        start_index: 0,
        prim_count: (strip_len - 2) as u16,
        base_vertex: 0,
        max_index: (vertex_count - 1) as u16,
        vertex_count: vertex_count as u16,
    };
    vec![rec, rec]
}

/// The top-level `INFO`.
///
/// `material_count` is the word `Mtrl_Parse`'s caller reads (`+0x24`). Three other words hold the
/// sub-object count in every retail container; [`TinyModel::encode`] writes it to all three and
/// [`TinyModel::decode`] refuses a container where one differs.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// `+0x00`.
    pub flags: u32,
    /// `+0x04`, `+0x10`: the model's bounds.
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// `+0x1C`.
    pub word_1c: u32,
    /// `+0x30` .. `+0x44`: the six words after the counts.
    pub tail: [u32; 6],
}

/// One `HIER` node.
#[derive(Debug, Clone, PartialEq)]
pub struct HierNode {
    pub name_hash: u32,
    /// `+0x04`.
    pub flags: u16,
    /// `+0x06`: the parent node, `-1` for none.
    pub parent: i32,
    /// `+0x0A`: the next sibling, `-1` for none.
    pub sibling: i16,
    /// `+0x0C`.
    pub word_0c: u32,
    /// `+0x10` and `+0x50`: two 4×4 matrices.
    pub matrix_a: [f32; 16],
    pub matrix_b: [f32; 16],
    /// `+0x90` and `+0xA0`: the node's bounds, each with a fourth component.
    pub bbox_min: [f32; 4],
    pub bbox_max: [f32; 4],
}

/// One `MTRL` material.
#[derive(Debug, Clone, PartialEq)]
pub struct TinyMaterial {
    /// The preamble's first word (`Mtrl_Parse` stores it at material `+0x64`).
    pub name_hash: u32,
    /// The rest of the preamble, 25 words.
    pub preamble: [u32; 25],
    /// The on-disk flags word.
    pub flags: u16,
    /// The texture hashes, in slot order.
    pub textures: Vec<u32>,
    /// The pixel shader key.
    pub pixel_shader: u32,
    /// The word after the pixel shader key.
    pub trailing: u32,
}

/// One `SEGM` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmRecord {
    pub bone: u16,
    pub segment: u8,
    pub state_mask: u8,
}

/// One vertex, as stored: half-float bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TinyVertex {
    /// x, y, z, and `w` = the object slot.
    pub position: [u16; 4],
    pub uv: [u16; 2],
    pub normal: [u16; 4],
}

/// One `PRMT` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimRecord {
    pub material: u32,
    pub start_index: u32,
    pub prim_count: u16,
    pub base_vertex: u16,
    pub max_index: u16,
    pub vertex_count: u16,
}

/// One primitive group (`PRMG`).
#[derive(Debug, Clone, PartialEq)]
pub struct PrimGroup {
    /// The group `INFO`'s first three words.
    pub header: [u32; 3],
    pub vertex_shader: u32,
    pub shadow_vertex_shader: u32,
    pub center: [f32; 3],
    pub radius: f32,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// The `STRM` `info` flag word.
    pub stream_flag: u32,
    pub vertices: Vec<TinyVertex>,
    /// The triangle strip.
    pub strip: Vec<u16>,
    pub prims: Vec<PrimRecord>,
}

/// One drawn sub-object: a `GEOM` `TINY` child.
#[derive(Debug, Clone, PartialEq)]
pub struct SubObject {
    /// Its `GEOM` `INDX` entry: the `HIER` node it hangs from.
    pub node: u16,
    pub groups: Vec<PrimGroup>,
}

/// A decoded TINY model container.
#[derive(Debug, Clone, PartialEq)]
pub struct TinyModel {
    pub info: ModelInfo,
    pub nodes: Vec<HierNode>,
    pub materials: Vec<TinyMaterial>,
    /// The slot list: world-object GUIDs.
    pub slots: Vec<u32>,
    pub segments: Vec<SegmRecord>,
    pub sub_objects: Vec<SubObject>,
}

// ── little-endian reads ─────────────────────────────────────────────────────────────────────────

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}
fn f32s<const N: usize>(b: &[u8], o: usize) -> [f32; N] {
    std::array::from_fn(|k| f32_at(b, o + 4 * k))
}
fn u32s<const N: usize>(b: &[u8], o: usize) -> [u32; N] {
    std::array::from_fn(|k| u32_at(b, o + 4 * k))
}
fn put_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put_f32s(v: &mut Vec<u8>, xs: &[f32]) {
    for x in xs {
        v.extend_from_slice(&x.to_bits().to_le_bytes());
    }
}

// ── tree helpers ────────────────────────────────────────────────────────────────────────────────

fn tag(n: &UcfxNode) -> String {
    n.tag_str()
}

/// The body of a node that must be a leaf with no children.
fn leaf<'a>(n: &'a UcfxNode, want: &[u8; 4], at: &str) -> Result<&'a [u8], String> {
    if &n.tag != want {
        return Err(format!("{at}: expected {} and found {}", String::from_utf8_lossy(want), tag(n)));
    }
    if !n.children.is_empty() {
        return Err(format!("{at}: {} has {} children; it is a leaf", tag(n), n.children.len()));
    }
    n.body.as_deref().ok_or_else(|| format!("{at}: {} is a marker row; it carries a body", tag(n)))
}

/// The children of a node that must be a marker.
fn marker<'a>(n: &'a UcfxNode, want: &[u8; 4], at: &str) -> Result<&'a [UcfxNode], String> {
    if &n.tag != want {
        return Err(format!("{at}: expected {} and found {}", String::from_utf8_lossy(want), tag(n)));
    }
    if n.body.is_some() {
        return Err(format!("{at}: {} carries a body; it is a marker row", tag(n)));
    }
    Ok(&n.children)
}

fn exact_len(body: &[u8], len: usize, what: &str) -> Result<(), String> {
    if body.len() != len {
        return Err(format!("{what} is {} bytes; it is {len}", body.len()));
    }
    Ok(())
}

fn count_word(body: &[u8], what: &str) -> Result<usize, String> {
    exact_len(body, 4, what)?;
    Ok(u32_at(body, 0) as usize)
}

// ── decode ──────────────────────────────────────────────────────────────────────────────────────

impl TinyModel {
    /// Decode a TINY model container. Strict: the tree shape, every body length and every implied
    /// count must be exactly what [`TinyModel::encode`] writes.
    pub fn decode(container: &[u8]) -> Result<TinyModel, String> {
        let roots = parse_ucfx_tree(container)?;
        let tags: Vec<String> = roots.iter().map(tag).collect();
        if tags != ["INFO", "HIER", "MTRL", "TINY", "SEGM", "GEOM"] {
            return Err(format!(
                "top level is [{}]; a TINY container is [INFO, HIER, MTRL, TINY, SEGM, GEOM]",
                tags.join(", ")
            ));
        }

        let info_b = leaf(&roots[0], b"INFO", "top level")?;
        exact_len(info_b, INFO_BYTES, "top-level INFO")?;
        let info = ModelInfo {
            flags: u32_at(info_b, 0x00),
            bbox_min: f32s(info_b, 0x04),
            bbox_max: f32s(info_b, 0x10),
            word_1c: u32_at(info_b, 0x1C),
            tail: u32s(info_b, 0x30),
        };
        let [count_20, material_count, count_28, count_2c]: [u32; 4] = u32s(info_b, 0x20);

        let hier = leaf(&roots[1], b"HIER", "top level")?;
        if hier.len() % HIER_NODE_BYTES != 0 {
            return Err(format!("HIER is {} bytes, not a multiple of {HIER_NODE_BYTES}", hier.len()));
        }
        let nodes: Vec<HierNode> = hier
            .chunks_exact(HIER_NODE_BYTES)
            .map(|n| HierNode {
                name_hash: u32_at(n, 0x00),
                flags: u16_at(n, 0x04),
                parent: u32_at(n, 0x06) as i32,
                sibling: u16_at(n, 0x0A) as i16,
                word_0c: u32_at(n, 0x0C),
                matrix_a: f32s(n, 0x10),
                matrix_b: f32s(n, 0x50),
                bbox_min: f32s(n, 0x90),
                bbox_max: f32s(n, 0xA0),
            })
            .collect();

        let mtrl = leaf(&roots[2], b"MTRL", "top level")?;
        let materials = decode_materials(mtrl, material_count as usize)?;

        let list = leaf(&roots[3], b"TINY", "top level")?;
        if list.len() < 4 {
            return Err(format!("the TINY slot list is {} bytes; it starts with a u32 count", list.len()));
        }
        let n = u32_at(list, 0) as usize;
        exact_len(list, 4 + 4 * n, &format!("the TINY slot list of {n} GUIDs"))?;
        let slots: Vec<u32> = (0..n).map(|k| u32_at(list, 4 + 4 * k)).collect();

        let segm = leaf(&roots[4], b"SEGM", "top level")?;
        if segm.len() % 4 != 0 {
            return Err(format!("SEGM is {} bytes, not a multiple of 4", segm.len()));
        }
        let segments: Vec<SegmRecord> = segm
            .chunks_exact(4)
            .map(|r| SegmRecord { bone: u16_at(r, 0), segment: r[2], state_mask: r[3] })
            .collect();

        let geom = marker(&roots[5], b"GEOM", "top level")?;
        if geom.len() < 2 {
            return Err(format!("GEOM has {} children; it has INFO, INDX and the sub-objects", geom.len()));
        }
        let sub_count = count_word(leaf(&geom[0], b"INFO", "GEOM")?, "GEOM INFO")?;
        let indx = leaf(&geom[1], b"INDX", "GEOM")?;
        exact_len(indx, 2 * sub_count, &format!("GEOM INDX for {sub_count} sub-objects"))?;
        if geom.len() != 2 + sub_count {
            return Err(format!(
                "GEOM INFO counts {sub_count} sub-objects and GEOM holds {}",
                geom.len() - 2
            ));
        }
        let mut sub_objects = Vec::with_capacity(sub_count);
        for (k, s) in geom[2..].iter().enumerate() {
            let at = format!("sub-object {k}");
            let kids = marker(s, b"TINY", &at)?;
            let Some((first, groups)) = kids.split_first() else {
                return Err(format!("{at} is empty; it starts with INFO"));
            };
            let group_count = count_word(leaf(first, b"INFO", &at)?, &format!("{at} INFO"))?;
            if group_count != groups.len() {
                return Err(format!("{at} INFO counts {group_count} groups and it holds {}", groups.len()));
            }
            let groups = groups
                .iter()
                .enumerate()
                .map(|(g, node)| decode_group(node, &format!("{at} group {g}")))
                .collect::<Result<Vec<_>, _>>()?;
            sub_objects.push(SubObject { node: u16_at(indx, 2 * k), groups });
        }

        for (what, v) in [
            ("INFO +0x20", count_20),
            ("INFO +0x28", count_28),
            ("INFO +0x2C", count_2c),
        ] {
            if v as usize != sub_count {
                return Err(format!("{what} is {v} and the container holds {sub_count} sub-objects"));
            }
        }

        Ok(TinyModel { info, nodes, materials, slots, segments, sub_objects })
    }

    /// Encode to a UCFX container.
    pub fn encode(&self) -> Vec<u8> {
        let mut info = Vec::with_capacity(INFO_BYTES);
        put_u32(&mut info, self.info.flags);
        put_f32s(&mut info, &self.info.bbox_min);
        put_f32s(&mut info, &self.info.bbox_max);
        put_u32(&mut info, self.info.word_1c);
        let subs = self.sub_objects.len() as u32;
        for w in [subs, self.materials.len() as u32, subs, subs] {
            put_u32(&mut info, w);
        }
        for w in self.info.tail {
            put_u32(&mut info, w);
        }

        let mut hier = Vec::with_capacity(self.nodes.len() * HIER_NODE_BYTES);
        for n in &self.nodes {
            put_u32(&mut hier, n.name_hash);
            put_u16(&mut hier, n.flags);
            put_u32(&mut hier, n.parent as u32);
            put_u16(&mut hier, n.sibling as u16);
            put_u32(&mut hier, n.word_0c);
            put_f32s(&mut hier, &n.matrix_a);
            put_f32s(&mut hier, &n.matrix_b);
            put_f32s(&mut hier, &n.bbox_min);
            put_f32s(&mut hier, &n.bbox_max);
        }

        let mut mtrl = Vec::new();
        for m in &self.materials {
            put_u32(&mut mtrl, m.name_hash);
            for w in m.preamble {
                put_u32(&mut mtrl, w);
            }
            put_u16(&mut mtrl, m.flags);
            put_u16(&mut mtrl, m.textures.len() as u16);
            for &t in &m.textures {
                put_u32(&mut mtrl, t);
            }
            put_u32(&mut mtrl, m.pixel_shader);
            put_u32(&mut mtrl, m.trailing);
        }

        let mut list = Vec::with_capacity(4 + 4 * self.slots.len());
        put_u32(&mut list, self.slots.len() as u32);
        for &g in &self.slots {
            put_u32(&mut list, g);
        }

        let mut segm = Vec::with_capacity(4 * self.segments.len());
        for s in &self.segments {
            put_u16(&mut segm, s.bone);
            segm.push(s.segment);
            segm.push(s.state_mask);
        }

        let mut indx = Vec::with_capacity(2 * self.sub_objects.len());
        for s in &self.sub_objects {
            put_u16(&mut indx, s.node);
        }
        let mut geom = vec![
            UcfxNode::leaf(*b"INFO", subs.to_le_bytes().to_vec()),
            UcfxNode::leaf(*b"INDX", indx),
        ];
        for s in &self.sub_objects {
            let mut kids = vec![UcfxNode::leaf(*b"INFO", (s.groups.len() as u32).to_le_bytes().to_vec())];
            kids.extend(s.groups.iter().map(encode_group));
            geom.push(UcfxNode::marker(*b"TINY", kids));
        }

        write_ucfx_tree(&[
            UcfxNode::leaf(*b"INFO", info),
            UcfxNode::leaf(*b"HIER", hier),
            UcfxNode::leaf(*b"MTRL", mtrl),
            UcfxNode::leaf(*b"TINY", list),
            UcfxNode::leaf(*b"SEGM", segm),
            UcfxNode::marker(*b"GEOM", geom),
        ])
    }
}

fn decode_materials(body: &[u8], count: usize) -> Result<Vec<TinyMaterial>, String> {
    let mut out = Vec::with_capacity(count);
    let mut p = 0usize;
    for k in 0..count {
        let head = p + MTRL_PREAMBLE_BYTES + 4;
        if head > body.len() {
            return Err(format!("MTRL material {k} needs {head} bytes and the leaf has {}", body.len()));
        }
        let flags = u16_at(body, p + MTRL_PREAMBLE_BYTES);
        let tex_count = u16_at(body, p + MTRL_PREAMBLE_BYTES + 2) as usize;
        let end = head + 4 * tex_count + 8;
        if end > body.len() {
            return Err(format!(
                "MTRL material {k} with {tex_count} textures needs {end} bytes and the leaf has {}",
                body.len()
            ));
        }
        out.push(TinyMaterial {
            name_hash: u32_at(body, p),
            preamble: u32s(body, p + 4),
            flags,
            textures: (0..tex_count).map(|t| u32_at(body, head + 4 * t)).collect(),
            pixel_shader: u32_at(body, head + 4 * tex_count),
            trailing: u32_at(body, head + 4 * tex_count + 4),
        });
        p = end;
    }
    if p != body.len() {
        return Err(format!(
            "MTRL holds {} bytes and its {count} materials (INFO +0x24) fill {p}",
            body.len()
        ));
    }
    Ok(out)
}

fn decode_group(node: &UcfxNode, at: &str) -> Result<PrimGroup, String> {
    let kids = marker(node, b"PRMG", at)?;
    let tags: Vec<String> = kids.iter().map(tag).collect();
    if tags != ["INFO", "STRM", "IBUF", "PRMT"] {
        return Err(format!("{at} is [{}]; a TINY group is [INFO, STRM, IBUF, PRMT]", tags.join(", ")));
    }
    let info = leaf(&kids[0], b"INFO", at)?;
    exact_len(info, GROUP_INFO_BYTES, &format!("{at} INFO"))?;

    let strm = marker(&kids[1], b"STRM", at)?;
    let stags: Vec<String> = strm.iter().map(tag).collect();
    if stags != ["info", "decl", "data"] {
        return Err(format!("{at} STRM is [{}]; it is [info, decl, data]", stags.join(", ")));
    }
    let sinfo = leaf(&strm[0], b"info", at)?;
    exact_len(sinfo, 12, &format!("{at} STRM info"))?;
    let stride = u32_at(sinfo, 4) as usize;
    if stride != TINY_STRIDE {
        return Err(format!("{at} STRM stride is {stride}; a TINY group's is {TINY_STRIDE}"));
    }
    let decl = leaf(&strm[1], b"decl", at)?;
    if decl != TINY_DECL {
        return Err(format!("{at} STRM decl is not the TINY declaration (POSITION, TEXCOORD, NORMAL)"));
    }
    let data = leaf(&strm[2], b"data", at)?;
    let vcount = u32_at(sinfo, 8) as usize;
    exact_len(data, vcount * TINY_STRIDE, &format!("{at} STRM data of {vcount} vertices"))?;
    let vertices = data
        .chunks_exact(TINY_STRIDE)
        .map(|v| TinyVertex {
            position: std::array::from_fn(|k| u16_at(v, 2 * k)),
            uv: std::array::from_fn(|k| u16_at(v, 8 + 2 * k)),
            normal: std::array::from_fn(|k| u16_at(v, 12 + 2 * k)),
        })
        .collect();

    let ibuf = marker(&kids[2], b"IBUF", at)?;
    let itags: Vec<String> = ibuf.iter().map(tag).collect();
    if itags != ["info", "data"] {
        return Err(format!("{at} IBUF is [{}]; it is [info, data]", itags.join(", ")));
    }
    let icount = count_word(leaf(&ibuf[0], b"info", at)?, &format!("{at} IBUF info"))?;
    let idata = leaf(&ibuf[1], b"data", at)?;
    exact_len(idata, 2 * icount, &format!("{at} IBUF data of {icount} indices"))?;
    let strip = idata.chunks_exact(2).map(|i| u16_at(i, 0)).collect();

    let prmt = leaf(&kids[3], b"PRMT", at)?;
    if prmt.len() % PRMT_RECORD_BYTES != 0 {
        return Err(format!("{at} PRMT is {} bytes, not a multiple of {PRMT_RECORD_BYTES}", prmt.len()));
    }
    let prims = prmt
        .chunks_exact(PRMT_RECORD_BYTES)
        .map(|r| PrimRecord {
            material: u32_at(r, 0),
            start_index: u32_at(r, 4),
            prim_count: u16_at(r, 8),
            base_vertex: u16_at(r, 10),
            max_index: u16_at(r, 12),
            vertex_count: u16_at(r, 14),
        })
        .collect();

    Ok(PrimGroup {
        header: u32s(info, 0),
        vertex_shader: u32_at(info, 12),
        shadow_vertex_shader: u32_at(info, 16),
        center: f32s(info, 20),
        radius: f32_at(info, 32),
        bbox_min: f32s(info, 36),
        bbox_max: f32s(info, 48),
        stream_flag: u32_at(sinfo, 0),
        vertices,
        strip,
        prims,
    })
}

fn encode_group(g: &PrimGroup) -> UcfxNode {
    let mut info = Vec::with_capacity(GROUP_INFO_BYTES);
    for w in g.header {
        put_u32(&mut info, w);
    }
    put_u32(&mut info, g.vertex_shader);
    put_u32(&mut info, g.shadow_vertex_shader);
    put_f32s(&mut info, &g.center);
    put_f32s(&mut info, &[g.radius]);
    put_f32s(&mut info, &g.bbox_min);
    put_f32s(&mut info, &g.bbox_max);

    let mut sinfo = Vec::with_capacity(12);
    for w in [g.stream_flag, TINY_STRIDE as u32, g.vertices.len() as u32] {
        put_u32(&mut sinfo, w);
    }
    let mut data = Vec::with_capacity(g.vertices.len() * TINY_STRIDE);
    for v in &g.vertices {
        for h in v.position.iter().chain(&v.uv).chain(&v.normal) {
            put_u16(&mut data, *h);
        }
    }
    let mut idata = Vec::with_capacity(2 * g.strip.len());
    for &i in &g.strip {
        put_u16(&mut idata, i);
    }
    let mut prmt = Vec::with_capacity(PRMT_RECORD_BYTES * g.prims.len());
    for p in &g.prims {
        put_u32(&mut prmt, p.material);
        put_u32(&mut prmt, p.start_index);
        for h in [p.prim_count, p.base_vertex, p.max_index, p.vertex_count] {
            put_u16(&mut prmt, h);
        }
    }

    UcfxNode::marker(
        *b"PRMG",
        vec![
            UcfxNode::leaf(*b"INFO", info),
            UcfxNode::marker(
                *b"STRM",
                vec![
                    UcfxNode::leaf(*b"info", sinfo),
                    UcfxNode::leaf(*b"decl", TINY_DECL.to_vec()),
                    UcfxNode::leaf(*b"data", data),
                ],
            ),
            UcfxNode::marker(
                *b"IBUF",
                vec![
                    UcfxNode::leaf(*b"info", (g.strip.len() as u32).to_le_bytes().to_vec()),
                    UcfxNode::leaf(*b"data", idata),
                ],
            ),
            UcfxNode::leaf(*b"PRMT", prmt),
        ],
    )
}

/// Whether a container is a TINY model container: its top level carries a `TINY` slot list.
pub fn is_tiny_container(container: &[u8]) -> bool {
    crate::ucfx::read_ucfx_rows(container)
        .map(|rows| {
            let mut i = 0usize;
            while i < rows.len() {
                if &rows[i].tag == b"TINY" {
                    return true;
                }
                i += 1 + rows[i].x3 as usize;
            }
            false
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [f32; 16] {
        let mut m = [0.0; 16];
        for k in 0..4 {
            m[5 * k] = 1.0;
        }
        m
    }

    fn group(vs: u32, slots: &[u16]) -> PrimGroup {
        let vertices: Vec<TinyVertex> = slots
            .iter()
            .enumerate()
            .map(|(k, &s)| TinyVertex {
                position: [k as u16, 0x3c00, 0, s],
                uv: [0, 0x3c00],
                normal: [0, 0x3c00, 0, 0],
            })
            .collect();
        let n = vertices.len() as u16;
        PrimGroup {
            header: [1, 1, 0],
            vertex_shader: vs,
            shadow_vertex_shader: 0x1234_5678,
            center: [1.0, 2.0, 3.0],
            radius: 4.0,
            bbox_min: [0.0, 1.0, 2.0],
            bbox_max: [2.0, 3.0, 4.0],
            stream_flag: 4,
            vertices,
            strip: (0..n).collect(),
            prims: vec![
                PrimRecord { material: 0, start_index: 0, prim_count: n - 2, base_vertex: 0, max_index: n - 1, vertex_count: n };
                2
            ],
        }
    }

    fn sample() -> TinyModel {
        let node = |name_hash, sibling| HierNode {
            name_hash,
            flags: 1,
            parent: -1,
            sibling,
            word_0c: 0,
            matrix_a: identity(),
            matrix_b: identity(),
            bbox_min: [0.0, 0.0, 0.0, 1.0],
            bbox_max: [1.0, 1.0, 1.0, 1.0],
        };
        TinyModel {
            info: ModelInfo {
                flags: 0x39,
                bbox_min: [-1.0, -2.0, -3.0],
                bbox_max: [1.0, 2.0, 3.0],
                word_1c: 0x6b0,
                tail: [0, 1, 0x461c_4000, 0x40a0_0000, 0, 0x10000],
            },
            nodes: vec![node(0x86de_6639, 1), node(0xb5d7_712f, -1)],
            materials: vec![TinyMaterial {
                name_hash: 0xaabb_ccdd,
                preamble: [0x3f80_0000; 25],
                flags: 0x80,
                textures: vec![0x1111_2222],
                pixel_shader: 0x343a_f931,
                trailing: 0xed05_7225,
            }],
            slots: vec![10, 20, 30],
            segments: vec![
                SegmRecord { bone: 0, segment: 0, state_mask: 1 },
                SegmRecord { bone: 1, segment: 1, state_mask: 1 },
            ],
            sub_objects: vec![
                SubObject { node: 0, groups: vec![group(0xcff1_99a5, &[0, 0, 0, 1, 1])] },
                SubObject { node: 1, groups: vec![group(0x1a8e_1e26, &[2, 2, 2])] },
            ],
        }
    }

    #[test]
    fn encode_then_decode_is_identity_and_reencodes_to_the_same_bytes() {
        let m = sample();
        let bytes = m.encode();
        let back = TinyModel::decode(&bytes).expect("decode");
        assert_eq!(back, m);
        assert_eq!(back.encode(), bytes);
        assert!(is_tiny_container(&bytes));
    }

    #[test]
    fn the_chunk_layout_is_the_retail_shape() {
        let bytes = sample().encode();
        let tree = parse_ucfx_tree(&bytes).unwrap();
        let top: Vec<String> = tree.iter().map(|n| n.tag_str()).collect();
        assert_eq!(top, ["INFO", "HIER", "MTRL", "TINY", "SEGM", "GEOM"]);
        let list = tree[3].body.as_ref().unwrap();
        assert_eq!(list, &[3, 0, 0, 0, 10, 0, 0, 0, 20, 0, 0, 0, 30, 0, 0, 0]);
        let info = tree[0].body.as_ref().unwrap();
        assert_eq!(info.len(), INFO_BYTES);
        // sub-object count at +0x20, +0x28, +0x2C; material count at +0x24
        assert_eq!(u32s::<4>(info, 0x20), [2, 1, 2, 2]);
        assert_eq!(tree[1].body.as_ref().unwrap().len(), 2 * HIER_NODE_BYTES);
        assert_eq!(tree[2].body.as_ref().unwrap().len(), 120);
        let geom = &tree[5].children;
        assert_eq!(geom[0].body.as_ref().unwrap(), &2u32.to_le_bytes());
        assert_eq!(geom[1].body.as_ref().unwrap(), &[0, 0, 1, 0]);
        let prmg = &geom[2].children[1].children;
        assert_eq!(prmg[1].children[1].body.as_ref().unwrap(), &TINY_DECL);
        let ginfo = prmg[0].body.as_ref().unwrap();
        assert_eq!(u32_at(ginfo, 12), 0xcff1_99a5);
    }

    #[test]
    fn decode_refuses_a_count_word_that_disagrees() {
        let bytes = sample().encode();
        let mut tree = parse_ucfx_tree(&bytes).unwrap();
        let info = tree[0].body.as_mut().unwrap();
        info[0x28..0x2C].copy_from_slice(&3u32.to_le_bytes());
        let err = TinyModel::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("INFO +0x28"), "{err}");

        let mut tree = parse_ucfx_tree(&bytes).unwrap();
        tree[3].body.as_mut().unwrap()[0] = 4;
        let err = TinyModel::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("slot list"), "{err}");
    }

    #[test]
    fn the_helpers_spell_the_retail_names_and_records() {
        use crate::hash::pandemic_hash_m2;
        // retail: vz_merida_tiny_tinygeometry_tgr11_tgc30_0x00144ed3 material 0
        assert_eq!(material_name(11, 30, false), "tinygeometry_tgr11_tgc30_opaque");
        assert_eq!(pandemic_hash_m2(&material_name(11, 30, false)), 0x7d4d_5235);
        assert_eq!(material_name(3, 7, true), "tinygeometry_tgr03_tgc07_alphatest");
        assert_eq!(pandemic_hash_m2(NODE_PRISTINE), 0x86de_6639);
        assert_eq!(pandemic_hash_m2(NODE_RUIN), 0xb5d7_712f);
        assert_eq!(pandemic_hash_m2(MATERIAL_PIXEL_SHADER), 0x343a_f931);
        assert_eq!(pandemic_hash_m2(MATERIAL_TRAILING), 0xed05_7225);

        let a = hier_node(1, 0, 2, [0.0; 3], [1.0; 3]);
        let b = hier_node(2, 1, 2, [0.0; 3], [1.0; 3]);
        assert_eq!((a.sibling, b.sibling, a.parent), (1, -1, -1));
        assert_eq!(a.matrix_a[2].to_bits(), 0x8000_0000);
        assert_eq!(a.bbox_max, [1.0, 1.0, 1.0, 1.0]);

        let p = group_prims(1, 112, 76);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], p[1]);
        assert_eq!((p[0].material, p[0].prim_count, p[0].max_index, p[0].vertex_count), (1, 110, 75, 76));
    }

    #[test]
    fn decode_refuses_another_vertex_declaration() {
        let bytes = sample().encode();
        let mut tree = parse_ucfx_tree(&bytes).unwrap();
        let decl = tree[5].children[2].children[1].children[1].children[1].body.as_mut().unwrap();
        decl[4] = 0x02;
        let err = TinyModel::decode(&write_ucfx_tree(&tree)).unwrap_err();
        assert!(err.contains("decl"), "{err}");
    }
}
