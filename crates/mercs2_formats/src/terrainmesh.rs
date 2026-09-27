//! **Hi-res terrain cells** (`terrainmesh`, UCFX `type_hash` `0x7C569307`, ASET `type_id` 32): a typed
//! decoder/encoder, vertical displacement editing, and collision regeneration.
//!
//! The world is 20 × 20 cells of 400 m. Each cell is ONE `terrainmesh` container, carried in a c3 cell
//! block next to its terrain texture and its `scrub` (`0x600B904E`) ground-cover packages. There is **no
//! heightmap**: the ground is a triangle mesh, and the collider is a Havok `WpMeshShape16` + MOPP
//! ([`crate::phy2_build`], `docs/reverse_engineer/terrain_collision_regeneration.md` §1).
//!
//! # Container layout
//!
//! Every retail cell (400/400) has exactly this descriptor tree, with bodies packed back to back in
//! descriptor order (no gaps, no padding) and a `CSUM` trailer:
//!
//! ```text
//! INFO  32 B  {vec3 min, vec3 max, u32 material_count, u32 geom_count}
//! MTRL        material_count × record {u32, f32[25], u16 flags, u16 tex_count, u32[tex_count], u32[2]}
//!             then material_count × 256-byte block
//! GEOM  × geom_count (16: one per 100 m patch, POFF x/z ∈ {±50, ±150})
//!   INFO  44 B  {u32 prmg_count, vec3 sphere_centre, f32 radius, vec3 aabb_min, vec3 aabb_max}
//!   POFF  12 B  vec3 patch offset (y = 0)
//!   PRMG  × prmg_count
//!     INFO  44 B  {u32 draw_count, u32 alt_draw_count, 3 × PassGroup}
//!     PRMT        (draw_count + alt_draw_count) × 20 B Draw
//!     STRM  → info {u32 decl_rows, u32 stride, u32 vertex_count} · decl (D3DVERTEXELEMENT9[]) · data
//!     IBUF  → info {u32 index_count} · data (u16 indices)
//! PHY2        [12 × u32 prefix][Havok 5.5 packfile][engine wrapper]
//! ```
//!
//! Descriptor rows are `{tag, u0, size, x2, x3}`: `u0 = 0xFFFFFFFF` marks a container row (size 0),
//! `x2` is the row's reverse ordinal among its siblings and `x3` its descendant count (0 on a leaf).
//! Every derived word — `x2`, `x3`, every count, `STRM info`, `IBUF info`, the root `INFO` counts — is
//! recomputed by [`TerrainCell::encode`], and [`TerrainCell::decode`] refuses a container in which any of
//! them disagrees with what it would recompute, so a decode that succeeds is one the encoder reproduces.
//!
//! # Geometry
//!
//! A `STRM`'s layout is its `decl`, never its stride. POSITION (usage 0) and NORMAL (usage 3) are always
//! `FLOAT16_4`. Retail carries four decls: the ground's 20-byte `POSITION·D3DCOLOR·NORMAL` (exactly one
//! draw group per patch, 6,400 of them), and 20/28/32-byte variants that add a `FLOAT16_2` texcoord and,
//! on the latter two, a `FLOAT16_4` tangent (2,459 groups in all). Positions are patch-local; cell-local
//! = position + `POFF`.
//!
//! Index buffers are **triangle strips**, one strip per draw. A [`Draw`] names its range as
//! `[start_index, start_index + prim_count + 2)`; winding alternates from the draw's own start, so
//! destripping the whole buffer as one strip is wrong (it invents junction triangles and flips parity).
//! Index slots no draw references are padding and are carried verbatim.
//!
//! # Bounds
//!
//! The `GEOM INFO` sphere is the AABB's centre and half-diagonal, and the root `INFO` box is the union of
//! the `GEOM` boxes offset by `POFF` — both hold on all 6,400 retail patches. A retail box can sit up to
//! one f16 step inside the stored positions (inferred: it was computed before the positions were
//! quantized); a recompute ([`TerrainCell::displace`]) takes the stored positions as the truth.
//!
//! # Editing
//!
//! [`TerrainCell::displace`] moves `POSITION.y` only and rewrites the normals of the vertices whose
//! incident triangles moved. Vertices on the cell edge are shared with the neighbouring cell's mesh, so
//! a displacement that would move one is refused rather than opening a crack.
//! [`TerrainCell::rebuild_collision`] regenerates `PHY2` from the (edited) render triangles.

use std::collections::HashMap;

use crate::crc32::crc32_mercs2;
use crate::model_inject::{f16_le, read_f16_le, strip_to_tris};
use crate::phy2_build::{build_phy2_multi_hashed, MeshSoup};

/// UCFX `type_hash` of a hi-res terrain cell (`terrainmesh`).
pub const TYPE_HASH: u32 = crate::types::TYPE_HASH_TERRAIN_MESH;

const MARKER: u32 = 0xFFFF_FFFF;
const HEADER: usize = 20;
const ROW: usize = 20;

/// `D3DDECL_END()`: stream `0xFF`, offset 0, type `UNUSED` (17), method/usage/index 0.
const DECL_END: [u8; 8] = [0xFF, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00];
const DECLTYPE_FLOAT16_4: u8 = 16;
const USAGE_POSITION: u8 = 0;
const USAGE_NORMAL: u8 = 3;

const ROOT_INFO_LEN: usize = 32;
const GEOM_INFO_LEN: usize = 44;
const PRMG_INFO_LEN: usize = 44;
const POFF_LEN: usize = 12;
const DRAW_LEN: usize = 20;
/// Fixed part of an MTRL record before its texture-hash array: 26 floats, then `u16 flags, u16 count`.
const MATERIAL_HEAD: usize = 104;
const MATERIAL_BLOCK_WORDS: usize = 64;
const PHY2_PREFIX_WORDS: usize = 12;
/// Word 0 of every retail `PHY2` prefix.
const PHY2_TAG: u32 = 0x39;

// ───────────────────────────────────────────────────────────── typed tree ──

/// One decoded terrain cell. Every field is either carried verbatim or edited by this module; nothing
/// derivable is stored (the encoder recomputes it).
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainCell {
    /// Root `INFO` box, cell-local.
    pub bounds: Aabb,
    pub mtrl: Mtrl,
    pub geoms: Vec<Geom>,
    pub phy2: Phy2,
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// The `MTRL` leaf: the material records, then one 256-byte block per material.
#[derive(Debug, Clone, PartialEq)]
pub struct Mtrl {
    pub materials: Vec<Material>,
    /// The 256-byte blocks after the records, as 64 little-endian words. Their count equals the material
    /// count in every retail cell; their contents are not decoded.
    pub blocks: Vec<[u32; MATERIAL_BLOCK_WORDS]>,
}

/// One `MTRL` record (layout per `texture::parse_mtrl`: record stride `116 + 4·tex_count`).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// The record's first word. It is not a float — read as one it takes values like `-8.8e17` and
    /// `3.2e37` from material to material — so it is carried as the raw word.
    pub word_0: u32,
    pub params: [f32; 25],
    pub flags: u16,
    /// Texture hashes; `texture::TERRAIN_LAYER_MARKER` delimits detail layers.
    pub textures: Vec<u32>,
    /// The two words closing the record (not decoded).
    pub tail: [u32; 2],
}

/// One 100 m patch.
#[derive(Debug, Clone, PartialEq)]
pub struct Geom {
    pub sphere_centre: [f32; 3],
    pub sphere_radius: f32,
    /// Patch-local box.
    pub aabb: Aabb,
    /// Patch offset: cell-local = patch-local + `poff`.
    pub poff: [f32; 3],
    pub prmgs: Vec<Prmg>,
}

/// One draw group: a vertex stream, its index buffer, and the draws over it.
#[derive(Debug, Clone, PartialEq)]
pub struct Prmg {
    /// The `PRMG INFO` pass groups. Their `draw_count`s must sum to `draws.len()` and their
    /// `alt_draw_count`s to `alt_draws.len()`.
    pub pass_groups: [PassGroup; 3],
    /// The first `INFO.draw_count` `PRMT` records.
    pub draws: Vec<Draw>,
    /// The remaining `INFO.alt_draw_count` `PRMT` records.
    pub alt_draws: Vec<Draw>,
    pub stride: u32,
    /// Vertex declaration, without the `D3DDECL_END` terminator.
    pub decl: Vec<DeclElement>,
    /// Raw vertex bytes, `stride` per vertex.
    pub vertices: Vec<u8>,
    pub indices: Vec<u16>,
}

/// A 12-byte `PRMG INFO` pass group. Across the 26,577 retail groups the `draw_count`s sum to the
/// primary draw count and the `alt_draw_count`s (always 1) to the alternate draw count, and `index` is
/// the group's position (0, 1, 2); `field_4`/`field_6` are not decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassGroup {
    pub draw_count: u32,
    pub field_4: u16,
    pub field_6: u16,
    pub alt_draw_count: u16,
    pub index: u16,
}

/// One 20-byte `PRMT` draw record. `min_index`/`max_index` are the smallest/largest index in the draw's
/// range in 130,049 of the 130,224 retail draws; `hash`, `field_4` and `field_18` are not decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
    pub hash: u32,
    pub field_4: u32,
    pub start_index: u32,
    pub prim_count: u16,
    pub min_index: u16,
    pub max_index: u16,
    pub field_18: u16,
}

/// One `D3DVERTEXELEMENT9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclElement {
    pub stream: u16,
    pub offset: u16,
    pub ty: u8,
    pub method: u8,
    pub usage: u8,
    pub usage_index: u8,
}

/// The `PHY2` leaf: a 12-word prefix, then the Havok packfile and its engine wrapper.
///
/// Every retail cell's prefix is `[0x39, cell hash, 2, 1, 1, 0, 0, collision vertex count, packfile
/// size, 0, 0, 0]`. Word 2 is 2 in all 400 cells while each carries ONE shape, so it is not a shape
/// count; its meaning is not established and it is carried from the cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Phy2 {
    pub prefix: [u32; PHY2_PREFIX_WORDS],
    pub payload: Vec<u8>,
}

// ─────────────────────────────────────────────────────────── raw layer ──

/// A descriptor-tree node: a leaf (`body`) or a container (`children`).
#[derive(Debug, Clone)]
enum Node {
    Leaf([u8; 4], Vec<u8>),
    Container([u8; 4], Vec<Node>),
}

impl Node {
    fn tag(&self) -> [u8; 4] {
        match self {
            Node::Leaf(t, _) | Node::Container(t, _) => *t,
        }
    }
    fn descendants(&self) -> usize {
        match self {
            Node::Leaf(..) => 0,
            Node::Container(_, c) => c.iter().map(|n| 1 + n.descendants()).sum(),
        }
    }
}

fn tag_str(t: &[u8; 4]) -> String {
    String::from_utf8_lossy(t).into_owned()
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn vec3_at(b: &[u8], o: usize) -> [f32; 3] {
    [f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8)]
}
fn put_vec3(out: &mut Vec<u8>, v: [f32; 3]) {
    for c in v {
        out.extend_from_slice(&c.to_le_bytes());
    }
}

/// Parse and validate the descriptor tree. Refuses anything [`emit_tree`] would not reproduce exactly.
fn parse_tree(c: &[u8]) -> Result<Vec<Node>, String> {
    if c.len() < HEADER + 8 || &c[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let csum_at = c.len() - 8;
    if &c[csum_at..csum_at + 4] != b"CSUM" {
        return Err("no CSUM trailer".into());
    }
    let stored = u32_at(c, csum_at + 4);
    let actual = crc32_mercs2(&c[..csum_at]);
    if stored != actual {
        return Err(format!(
            "CSUM mismatch: stored {stored:#010X}, computed {actual:#010X}"
        ));
    }
    let data_off = u32_at(c, 4) as usize;
    let n = u32_at(c, 16) as usize;
    if u32_at(c, 8) != 0 || u32_at(c, 12) != 0 {
        return Err(format!(
            "header words 8/12 are {:#X}/{:#X}; this codec writes 0/0",
            u32_at(c, 8),
            u32_at(c, 12)
        ));
    }
    if data_off != HEADER + ROW * n || data_off > csum_at {
        return Err(format!(
            "data area at {data_off}, expected {} for {n} rows",
            HEADER + ROW * n
        ));
    }
    struct Row {
        tag: [u8; 4],
        u0: u32,
        size: u32,
        x2: u32,
        x3: u32,
    }
    let rows: Vec<Row> = (0..n)
        .map(|i| {
            let r = HEADER + ROW * i;
            Row {
                tag: c[r..r + 4].try_into().unwrap(),
                u0: u32_at(c, r + 4),
                size: u32_at(c, r + 8),
                x2: u32_at(c, r + 12),
                x3: u32_at(c, r + 16),
            }
        })
        .collect();

    // Bodies must be packed in row order from the start of the data area to the CSUM.
    let mut cursor = 0usize;
    for (i, r) in rows.iter().enumerate() {
        if r.u0 == MARKER {
            if r.size != 0 {
                return Err(format!(
                    "container row {i} {} has size {}",
                    tag_str(&r.tag),
                    r.size
                ));
            }
            continue;
        }
        if r.u0 as usize != cursor {
            return Err(format!(
                "row {i} {} body at data+{}, expected data+{cursor} (bodies are packed in row order)",
                tag_str(&r.tag),
                r.u0
            ));
        }
        cursor += r.size as usize;
    }
    if data_off + cursor != csum_at {
        return Err(format!(
            "bodies end at {}, CSUM at {csum_at}: unreferenced bytes in the data area",
            data_off + cursor
        ));
    }

    fn build(
        rows: &[Row],
        c: &[u8],
        data_off: usize,
        start: usize,
        end: usize,
    ) -> Result<Vec<Node>, String> {
        let mut out = Vec::new();
        let mut i = start;
        while i < end {
            let r = &rows[i];
            let span = r.x3 as usize;
            if i + 1 + span > end {
                return Err(format!("row {i} {} spans past its parent", tag_str(&r.tag)));
            }
            if r.u0 == MARKER {
                out.push(Node::Container(
                    r.tag,
                    build(rows, c, data_off, i + 1, i + 1 + span)?,
                ));
            } else {
                if span != 0 {
                    return Err(format!("leaf row {i} {} has x3 = {span}", tag_str(&r.tag)));
                }
                let s = data_off + r.u0 as usize;
                out.push(Node::Leaf(r.tag, c[s..s + r.size as usize].to_vec()));
            }
            i += 1 + span;
        }
        // x2 is the reverse ordinal among siblings.
        let mut j = start;
        let count = out.len();
        for (k, node) in out.iter().enumerate() {
            let want = (count - 1 - k) as u32;
            if rows[j].x2 != want {
                return Err(format!(
                    "row {j} {} has x2 = {}, expected {want}",
                    tag_str(&rows[j].tag),
                    rows[j].x2
                ));
            }
            j += 1 + node.descendants();
        }
        Ok(out)
    }
    build(&rows, c, data_off, 0, n)
}

/// One flattened descriptor row: tag, body (`None` for a container), `x2`, `x3`.
type FlatRow<'a> = ([u8; 4], Option<&'a [u8]>, u32, u32);

/// Serialize a descriptor tree: header, rows (pre-order), packed bodies, `CSUM`.
fn emit_tree(roots: &[Node]) -> Vec<u8> {
    fn flatten<'a>(nodes: &'a [Node], rows: &mut Vec<FlatRow<'a>>) {
        let n = nodes.len();
        for (k, node) in nodes.iter().enumerate() {
            let x2 = (n - 1 - k) as u32;
            match node {
                Node::Leaf(t, b) => rows.push((*t, Some(b), x2, 0)),
                Node::Container(t, children) => {
                    rows.push((*t, None, x2, node.descendants() as u32));
                    flatten(children, rows);
                }
            }
        }
    }
    let mut rows = Vec::new();
    flatten(roots, &mut rows);
    let data_off = HEADER + ROW * rows.len();
    let body_len: usize = rows.iter().filter_map(|r| r.1.map(|b| b.len())).sum();
    let mut out = Vec::with_capacity(data_off + body_len + 8);
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    let mut cursor = 0u32;
    for (tag, body, x2, x3) in &rows {
        out.extend_from_slice(tag);
        match body {
            Some(b) => {
                out.extend_from_slice(&cursor.to_le_bytes());
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                cursor += b.len() as u32;
            }
            None => {
                out.extend_from_slice(&MARKER.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        out.extend_from_slice(&x2.to_le_bytes());
        out.extend_from_slice(&x3.to_le_bytes());
    }
    for (_, body, _, _) in &rows {
        if let Some(b) = body {
            out.extend_from_slice(b);
        }
    }
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

// ─────────────────────────────────────────────────────── typed decode ──

fn expect_children(nodes: &[Node], want: &[&[u8; 4]], ctx: &str) -> Result<(), String> {
    let got: Vec<[u8; 4]> = nodes.iter().map(|n| n.tag()).collect();
    if got.len() != want.len() || got.iter().zip(want).any(|(g, w)| g != *w) {
        return Err(format!(
            "{ctx}: children {:?}, expected {:?}",
            got.iter().map(tag_str).collect::<Vec<_>>(),
            want.iter().map(|w| tag_str(w)).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn leaf<'a>(n: &'a Node, ctx: &str) -> Result<&'a [u8], String> {
    match n {
        Node::Leaf(_, b) => Ok(b),
        Node::Container(t, _) => Err(format!(
            "{ctx}: {} is a container, expected a leaf",
            tag_str(t)
        )),
    }
}

fn children_of<'a>(n: &'a Node, ctx: &str) -> Result<&'a [Node], String> {
    match n {
        Node::Container(_, c) => Ok(c),
        Node::Leaf(t, _) => Err(format!(
            "{ctx}: {} is a leaf, expected a container",
            tag_str(t)
        )),
    }
}

fn exact_len(b: &[u8], want: usize, ctx: &str) -> Result<(), String> {
    if b.len() != want {
        return Err(format!("{ctx}: {} bytes, expected {want}", b.len()));
    }
    Ok(())
}

/// Byte size of a `D3DDECLTYPE`.
fn decl_type_size(ty: u8) -> Result<usize, String> {
    Ok(match ty {
        0 => 4,                      // FLOAT1
        1 => 8,                      // FLOAT2
        2 => 12,                     // FLOAT3
        3 => 16,                     // FLOAT4
        4 | 5 | 6 | 8 | 9 | 11 => 4, // D3DCOLOR, UBYTE4, SHORT2, UBYTE4N, SHORT2N, USHORT2N
        7 | 10 | 12 => 8,            // SHORT4, SHORT4N, USHORT4N
        13..=15 => 4,                // UDEC3, DEC3N, FLOAT16_2
        16 => 8,                     // FLOAT16_4
        _ => return Err(format!("unknown D3DDECLTYPE {ty}")),
    })
}

fn decode_mtrl(b: &[u8], material_count: usize) -> Result<Mtrl, String> {
    let mut p = 0usize;
    let mut materials = Vec::with_capacity(material_count);
    for m in 0..material_count {
        if p + MATERIAL_HEAD + 4 > b.len() {
            return Err(format!("MTRL: record {m} runs past the body"));
        }
        let mut params = [0f32; 25];
        for (k, v) in params.iter_mut().enumerate() {
            *v = f32_at(b, p + 4 + 4 * k);
        }
        let flags = u16_at(b, p + MATERIAL_HEAD);
        let count = u16_at(b, p + MATERIAL_HEAD + 2) as usize;
        let end = p + MATERIAL_HEAD + 4 + 4 * count + 8;
        if end > b.len() {
            return Err(format!(
                "MTRL: record {m} ({count} textures) runs past the body"
            ));
        }
        let textures = (0..count)
            .map(|k| u32_at(b, p + MATERIAL_HEAD + 4 + 4 * k))
            .collect();
        let t = p + MATERIAL_HEAD + 4 + 4 * count;
        materials.push(Material {
            word_0: u32_at(b, p),
            params,
            flags,
            textures,
            tail: [u32_at(b, t), u32_at(b, t + 4)],
        });
        p = end;
    }
    let rest = b.len() - p;
    if rest != material_count * MATERIAL_BLOCK_WORDS * 4 {
        return Err(format!(
            "MTRL: {rest} bytes follow the {material_count} records, expected {} ({material_count} × 256)",
            material_count * MATERIAL_BLOCK_WORDS * 4
        ));
    }
    let blocks = (0..material_count)
        .map(|k| {
            let mut w = [0u32; MATERIAL_BLOCK_WORDS];
            for (i, v) in w.iter_mut().enumerate() {
                *v = u32_at(b, p + k * 256 + 4 * i);
            }
            w
        })
        .collect();
    Ok(Mtrl { materials, blocks })
}

fn decode_draw(b: &[u8], o: usize) -> Draw {
    Draw {
        hash: u32_at(b, o),
        field_4: u32_at(b, o + 4),
        start_index: u32_at(b, o + 8),
        prim_count: u16_at(b, o + 12),
        min_index: u16_at(b, o + 14),
        max_index: u16_at(b, o + 16),
        field_18: u16_at(b, o + 18),
    }
}

fn decode_prmg(children: &[Node], ctx: &str) -> Result<Prmg, String> {
    expect_children(children, &[b"INFO", b"PRMT", b"STRM", b"IBUF"], ctx)?;
    let info = leaf(&children[0], ctx)?;
    exact_len(info, PRMG_INFO_LEN, &format!("{ctx} INFO"))?;
    let draw_count = u32_at(info, 0) as usize;
    let alt_count = u32_at(info, 4) as usize;
    let mut pass_groups = [PassGroup {
        draw_count: 0,
        field_4: 0,
        field_6: 0,
        alt_draw_count: 0,
        index: 0,
    }; 3];
    for (g, pg) in pass_groups.iter_mut().enumerate() {
        let o = 8 + 12 * g;
        *pg = PassGroup {
            draw_count: u32_at(info, o),
            field_4: u16_at(info, o + 4),
            field_6: u16_at(info, o + 6),
            alt_draw_count: u16_at(info, o + 8),
            index: u16_at(info, o + 10),
        };
    }
    let prmt = leaf(&children[1], ctx)?;
    exact_len(
        prmt,
        (draw_count + alt_count) * DRAW_LEN,
        &format!("{ctx} PRMT"),
    )?;
    let all: Vec<Draw> = (0..draw_count + alt_count)
        .map(|i| decode_draw(prmt, i * DRAW_LEN))
        .collect();

    let strm = children_of(&children[2], ctx)?;
    expect_children(strm, &[b"info", b"decl", b"data"], &format!("{ctx} STRM"))?;
    let sinfo = leaf(&strm[0], ctx)?;
    exact_len(sinfo, 12, &format!("{ctx} STRM info"))?;
    let decl_bytes = leaf(&strm[1], ctx)?;
    if decl_bytes.len() < 8
        || decl_bytes.len() % 8 != 0
        || decl_bytes[decl_bytes.len() - 8..] != DECL_END
    {
        return Err(format!(
            "{ctx}: decl is not a D3DDECL_END-terminated element array"
        ));
    }
    let decl: Vec<DeclElement> = decl_bytes[..decl_bytes.len() - 8]
        .chunks_exact(8)
        .map(|e| DeclElement {
            stream: u16::from_le_bytes([e[0], e[1]]),
            offset: u16::from_le_bytes([e[2], e[3]]),
            ty: e[4],
            method: e[5],
            usage: e[6],
            usage_index: e[7],
        })
        .collect();
    let vertices = leaf(&strm[2], ctx)?.to_vec();
    let stride = u32_at(sinfo, 4);

    let ibuf = children_of(&children[3], ctx)?;
    expect_children(ibuf, &[b"info", b"data"], &format!("{ctx} IBUF"))?;
    let iinfo = leaf(&ibuf[0], ctx)?;
    exact_len(iinfo, 4, &format!("{ctx} IBUF info"))?;
    let idata = leaf(&ibuf[1], ctx)?;
    if idata.len() % 2 != 0 {
        return Err(format!("{ctx}: IBUF data is {} bytes (odd)", idata.len()));
    }
    let indices: Vec<u16> = idata
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();

    let prmg = Prmg {
        pass_groups,
        draws: all[..draw_count].to_vec(),
        alt_draws: all[draw_count..].to_vec(),
        stride,
        decl,
        vertices,
        indices,
    };
    // Everything the encoder derives must agree with what is stored.
    prmg.validate().map_err(|e| format!("{ctx}: {e}"))?;
    let want_sinfo = prmg.strm_info();
    if sinfo != want_sinfo {
        return Err(format!(
            "{ctx}: STRM info {:?} is not the derived {:?}",
            sinfo, want_sinfo
        ));
    }
    if u32_at(iinfo, 0) as usize != prmg.indices.len() {
        return Err(format!(
            "{ctx}: IBUF info {} ≠ {} indices",
            u32_at(iinfo, 0),
            prmg.indices.len()
        ));
    }
    Ok(prmg)
}

impl TerrainCell {
    /// Decode a terrain-cell UCFX container (as extracted from its block, `CSUM` included).
    pub fn decode(container: &[u8]) -> Result<TerrainCell, String> {
        let roots = parse_tree(container)?;
        if roots.len() < 4 {
            return Err(format!(
                "{} root rows; expected INFO, MTRL, GEOM…, PHY2",
                roots.len()
            ));
        }
        let info = leaf(&roots[0], "root")?;
        if roots[0].tag() != *b"INFO" {
            return Err(format!(
                "root[0] is {}, expected INFO",
                tag_str(&roots[0].tag())
            ));
        }
        exact_len(info, ROOT_INFO_LEN, "root INFO")?;
        let bounds = Aabb {
            min: vec3_at(info, 0),
            max: vec3_at(info, 12),
        };
        let material_count = u32_at(info, 24) as usize;
        let geom_count = u32_at(info, 28) as usize;
        let mut want: Vec<&[u8; 4]> = vec![b"INFO", b"MTRL"];
        want.extend(std::iter::repeat_n(b"GEOM", geom_count));
        want.push(b"PHY2");
        expect_children(&roots, &want, "root")?;

        let mtrl = decode_mtrl(leaf(&roots[1], "root")?, material_count)?;

        let mut geoms = Vec::with_capacity(geom_count);
        for g in 0..geom_count {
            let ctx = format!("GEOM[{g}]");
            let kids = children_of(&roots[2 + g], &ctx)?;
            if kids.len() < 2 {
                return Err(format!("{ctx}: {} children", kids.len()));
            }
            let ginfo = leaf(&kids[0], &ctx)?;
            exact_len(ginfo, GEOM_INFO_LEN, &format!("{ctx} INFO"))?;
            let prmg_count = u32_at(ginfo, 0) as usize;
            let mut want: Vec<&[u8; 4]> = vec![b"INFO", b"POFF"];
            want.extend(std::iter::repeat_n(b"PRMG", prmg_count));
            expect_children(kids, &want, &ctx)?;
            let poff = leaf(&kids[1], &ctx)?;
            exact_len(poff, POFF_LEN, &format!("{ctx} POFF"))?;
            let mut prmgs = Vec::with_capacity(prmg_count);
            for (p, node) in kids[2..].iter().enumerate() {
                let pctx = format!("{ctx} PRMG[{p}]");
                prmgs.push(decode_prmg(children_of(node, &pctx)?, &pctx)?);
            }
            geoms.push(Geom {
                sphere_centre: vec3_at(ginfo, 4),
                sphere_radius: f32_at(ginfo, 16),
                aabb: Aabb {
                    min: vec3_at(ginfo, 20),
                    max: vec3_at(ginfo, 32),
                },
                poff: vec3_at(poff, 0),
                prmgs,
            });
        }

        let body = leaf(roots.last().unwrap(), "root")?;
        if body.len() < 4 * PHY2_PREFIX_WORDS {
            return Err(format!(
                "PHY2: {} bytes, shorter than its prefix",
                body.len()
            ));
        }
        let mut prefix = [0u32; PHY2_PREFIX_WORDS];
        for (k, w) in prefix.iter_mut().enumerate() {
            *w = u32_at(body, 4 * k);
        }
        let phy2 = Phy2 {
            prefix,
            payload: body[4 * PHY2_PREFIX_WORDS..].to_vec(),
        };
        phy2.validate()?;
        Ok(TerrainCell {
            bounds,
            mtrl,
            geoms,
            phy2,
        })
    }

    /// Encode back to a UCFX container (descriptor rows, packed bodies, `CSUM`). A decoded retail cell
    /// re-encodes byte-for-byte.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.mtrl.blocks.len() != self.mtrl.materials.len() {
            return Err(format!(
                "MTRL: {} blocks for {} materials",
                self.mtrl.blocks.len(),
                self.mtrl.materials.len()
            ));
        }
        self.phy2.validate()?;
        let mut roots = Vec::with_capacity(self.geoms.len() + 3);

        let mut info = Vec::with_capacity(ROOT_INFO_LEN);
        put_vec3(&mut info, self.bounds.min);
        put_vec3(&mut info, self.bounds.max);
        info.extend_from_slice(&(self.mtrl.materials.len() as u32).to_le_bytes());
        info.extend_from_slice(&(self.geoms.len() as u32).to_le_bytes());
        roots.push(Node::Leaf(*b"INFO", info));

        let mut m = Vec::new();
        for mat in &self.mtrl.materials {
            m.extend_from_slice(&mat.word_0.to_le_bytes());
            for v in mat.params {
                m.extend_from_slice(&v.to_le_bytes());
            }
            m.extend_from_slice(&mat.flags.to_le_bytes());
            let count = u16::try_from(mat.textures.len())
                .map_err(|_| format!("material has {} textures", mat.textures.len()))?;
            m.extend_from_slice(&count.to_le_bytes());
            for t in &mat.textures {
                m.extend_from_slice(&t.to_le_bytes());
            }
            m.extend_from_slice(&mat.tail[0].to_le_bytes());
            m.extend_from_slice(&mat.tail[1].to_le_bytes());
        }
        for block in &self.mtrl.blocks {
            for w in block {
                m.extend_from_slice(&w.to_le_bytes());
            }
        }
        roots.push(Node::Leaf(*b"MTRL", m));

        for (g, geom) in self.geoms.iter().enumerate() {
            let mut ginfo = Vec::with_capacity(GEOM_INFO_LEN);
            ginfo.extend_from_slice(&(geom.prmgs.len() as u32).to_le_bytes());
            put_vec3(&mut ginfo, geom.sphere_centre);
            ginfo.extend_from_slice(&geom.sphere_radius.to_le_bytes());
            put_vec3(&mut ginfo, geom.aabb.min);
            put_vec3(&mut ginfo, geom.aabb.max);
            let mut poff = Vec::with_capacity(POFF_LEN);
            put_vec3(&mut poff, geom.poff);
            let mut kids = vec![Node::Leaf(*b"INFO", ginfo), Node::Leaf(*b"POFF", poff)];
            for (p, prmg) in geom.prmgs.iter().enumerate() {
                prmg.validate()
                    .map_err(|e| format!("GEOM[{g}] PRMG[{p}]: {e}"))?;
                kids.push(prmg.to_node());
            }
            roots.push(Node::Container(*b"GEOM", kids));
        }

        let mut body = Vec::with_capacity(4 * PHY2_PREFIX_WORDS + self.phy2.payload.len());
        for w in self.phy2.prefix {
            body.extend_from_slice(&w.to_le_bytes());
        }
        body.extend_from_slice(&self.phy2.payload);
        roots.push(Node::Leaf(*b"PHY2", body));
        Ok(emit_tree(&roots))
    }
}

impl Phy2 {
    fn validate(&self) -> Result<(), String> {
        if self.prefix[0] != PHY2_TAG {
            return Err(format!(
                "PHY2: prefix word 0 is {:#X}, expected {PHY2_TAG:#X}",
                self.prefix[0]
            ));
        }
        if !self.payload.starts_with(&crate::havok::HAVOK_MAGIC) {
            return Err("PHY2: no Havok packfile after the prefix".into());
        }
        if self.prefix[8] as usize > self.payload.len() {
            return Err(format!(
                "PHY2: prefix names a {}-byte packfile in a {}-byte payload",
                self.prefix[8],
                self.payload.len()
            ));
        }
        Ok(())
    }
}

impl Prmg {
    /// Structural invariants the encoder relies on.
    fn validate(&self) -> Result<(), String> {
        let sum: u64 = self.pass_groups.iter().map(|g| g.draw_count as u64).sum();
        if sum != self.draws.len() as u64 {
            return Err(format!(
                "pass groups sum to {sum} draws, PRMT has {}",
                self.draws.len()
            ));
        }
        let alt: u64 = self
            .pass_groups
            .iter()
            .map(|g| g.alt_draw_count as u64)
            .sum();
        if alt != self.alt_draws.len() as u64 {
            return Err(format!(
                "pass groups name {alt} alternate draws, PRMT has {}",
                self.alt_draws.len()
            ));
        }
        let mut span = 0usize;
        for e in &self.decl {
            span = span.max(e.offset as usize + decl_type_size(e.ty)?);
        }
        if span as u32 != self.stride || self.stride == 0 {
            return Err(format!(
                "stride {} ≠ the decl's vertex size {span}",
                self.stride
            ));
        }
        if !self.vertices.len().is_multiple_of(self.stride as usize) {
            return Err(format!(
                "{} vertex bytes is not a multiple of stride {}",
                self.vertices.len(),
                self.stride
            ));
        }
        for usage in [USAGE_POSITION, USAGE_NORMAL] {
            self.f16x4_offset(usage)?;
        }
        Ok(())
    }

    fn strm_info(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(&(self.decl.len() as u32 + 1).to_le_bytes());
        v.extend_from_slice(&self.stride.to_le_bytes());
        v.extend_from_slice(&(self.vertex_count() as u32).to_le_bytes());
        v
    }

    fn to_node(&self) -> Node {
        let mut info = Vec::with_capacity(PRMG_INFO_LEN);
        info.extend_from_slice(&(self.draws.len() as u32).to_le_bytes());
        info.extend_from_slice(&(self.alt_draws.len() as u32).to_le_bytes());
        for g in &self.pass_groups {
            info.extend_from_slice(&g.draw_count.to_le_bytes());
            info.extend_from_slice(&g.field_4.to_le_bytes());
            info.extend_from_slice(&g.field_6.to_le_bytes());
            info.extend_from_slice(&g.alt_draw_count.to_le_bytes());
            info.extend_from_slice(&g.index.to_le_bytes());
        }
        let mut prmt = Vec::with_capacity((self.draws.len() + self.alt_draws.len()) * DRAW_LEN);
        for d in self.draws.iter().chain(&self.alt_draws) {
            prmt.extend_from_slice(&d.hash.to_le_bytes());
            prmt.extend_from_slice(&d.field_4.to_le_bytes());
            prmt.extend_from_slice(&d.start_index.to_le_bytes());
            prmt.extend_from_slice(&d.prim_count.to_le_bytes());
            prmt.extend_from_slice(&d.min_index.to_le_bytes());
            prmt.extend_from_slice(&d.max_index.to_le_bytes());
            prmt.extend_from_slice(&d.field_18.to_le_bytes());
        }
        let mut decl = Vec::with_capacity(8 * (self.decl.len() + 1));
        for e in &self.decl {
            decl.extend_from_slice(&e.stream.to_le_bytes());
            decl.extend_from_slice(&e.offset.to_le_bytes());
            decl.extend_from_slice(&[e.ty, e.method, e.usage, e.usage_index]);
        }
        decl.extend_from_slice(&DECL_END);
        let mut idata = Vec::with_capacity(2 * self.indices.len());
        for i in &self.indices {
            idata.extend_from_slice(&i.to_le_bytes());
        }
        Node::Container(
            *b"PRMG",
            vec![
                Node::Leaf(*b"INFO", info),
                Node::Leaf(*b"PRMT", prmt),
                Node::Container(
                    *b"STRM",
                    vec![
                        Node::Leaf(*b"info", self.strm_info()),
                        Node::Leaf(*b"decl", decl),
                        Node::Leaf(*b"data", self.vertices.clone()),
                    ],
                ),
                Node::Container(
                    *b"IBUF",
                    vec![
                        Node::Leaf(*b"info", (self.indices.len() as u32).to_le_bytes().to_vec()),
                        Node::Leaf(*b"data", idata),
                    ],
                ),
            ],
        )
    }

    /// Number of vertices in the stream.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / self.stride as usize
    }

    /// Byte offset of the single `FLOAT16_4` element with `usage`.
    fn f16x4_offset(&self, usage: u8) -> Result<usize, String> {
        let hits: Vec<&DeclElement> = self
            .decl
            .iter()
            .filter(|e| e.usage == usage && e.usage_index == 0)
            .collect();
        match hits.as_slice() {
            [e] if e.ty == DECLTYPE_FLOAT16_4 && e.stream == 0 => Ok(e.offset as usize),
            [e] => Err(format!(
                "usage {usage} is decl type {} on stream {}, expected FLOAT16_4 on stream 0",
                e.ty, e.stream
            )),
            [] => Err(format!("decl has no usage-{usage} element")),
            _ => Err(format!("decl has {} usage-{usage} elements", hits.len())),
        }
    }

    /// Patch-local position of vertex `i`.
    pub fn position(&self, i: usize) -> [f32; 3] {
        let o =
            i * self.stride as usize + self.f16x4_offset(USAGE_POSITION).expect("validated decl");
        [
            read_f16_le(&self.vertices, o),
            read_f16_le(&self.vertices, o + 2),
            read_f16_le(&self.vertices, o + 4),
        ]
    }

    /// Stored normal of vertex `i` (xyz; the w half is not part of the direction).
    pub fn normal(&self, i: usize) -> [f32; 3] {
        let o = i * self.stride as usize + self.f16x4_offset(USAGE_NORMAL).expect("validated decl");
        [
            read_f16_le(&self.vertices, o),
            read_f16_le(&self.vertices, o + 2),
            read_f16_le(&self.vertices, o + 4),
        ]
    }

    /// The index range a draw covers: `[start_index, start_index + prim_count + 2)`.
    pub fn draw_range(&self, d: &Draw) -> Result<std::ops::Range<usize>, String> {
        let s = d.start_index as usize;
        let e = s + d.prim_count as usize + 2;
        if e > self.indices.len() {
            return Err(format!(
                "draw [{s}, {e}) runs past the {}-index buffer",
                self.indices.len()
            ));
        }
        Ok(s..e)
    }

    /// Every distinct triangle the draws render (primary and alternate), in first-drawn order, each with
    /// the winding of its first occurrence. A triangle is identified by its vertex-index set.
    pub fn triangles(&self) -> Result<Vec<[u16; 3]>, String> {
        let nv = self.vertex_count();
        let mut seen: HashMap<[u16; 3], ()> = HashMap::new();
        let mut out = Vec::new();
        for d in self.draws.iter().chain(&self.alt_draws) {
            let r = self.draw_range(d)?;
            for t in destrip(&self.indices[r]) {
                if t.iter().any(|&v| v as usize >= nv) {
                    return Err(format!(
                        "triangle {t:?} indexes past the {nv}-vertex stream"
                    ));
                }
                let mut key = t;
                key.sort_unstable();
                if seen.insert(key, ()).is_none() {
                    out.push(t);
                }
            }
        }
        Ok(out)
    }
}

// ─────────────────────────────────────────────────────── strip codec ──

/// Rotate a triangle so its smallest index leads — winding-preserving canonical form.
fn canonical(t: [u16; 3]) -> [u16; 3] {
    let k = (0..3).min_by_key(|&i| t[i]).unwrap();
    [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
}

/// Expand one draw's triangle strip into its triangles. Triples with a repeated index are the strip's
/// degenerate stitches and are dropped; odd positions are re-wound, counted from the start of `strip`
/// (a draw restarts the parity).
pub fn destrip(strip: &[u16]) -> Vec<[u16; 3]> {
    let wide: Vec<u32> = strip.iter().map(|&i| i as u32).collect();
    strip_to_tris(&wide)
        .into_iter()
        .map(|t| [t[0] as u16, t[1] as u16, t[2] as u16])
        .collect()
}

/// Build one degenerate-stitched triangle strip that [`destrip`]s back to exactly `tris` — the same
/// triangles, each with the same winding, each once.
///
/// Greedy: seed a run with the first unused triangle, extend it across the trailing edge while a
/// neighbour both shares that edge and has the winding the next strip position produces, and bridge
/// runs with repeated indices (a new run always starts on an even position). Refuses a triangle with a
/// repeated index — a strip cannot carry one — and verifies its own output before returning it.
pub fn stripify(tris: &[[u16; 3]]) -> Result<Vec<u16>, String> {
    for (i, t) in tris.iter().enumerate() {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            return Err(format!(
                "triangle {i} {t:?} repeats an index; a strip cannot carry it"
            ));
        }
    }
    let key = |a: u16, b: u16| if a < b { (a, b) } else { (b, a) };
    let mut edges: HashMap<(u16, u16), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.entry(key(a, b)).or_default().push(i);
        }
    }
    let mut used = vec![false; tris.len()];
    let mut s: Vec<u16> = Vec::with_capacity(tris.len() * 2);
    for seed in 0..tris.len() {
        if used[seed] {
            continue;
        }
        used[seed] = true;
        let [a, b, c] = tris[seed];
        if let Some(&z) = s.last() {
            s.extend_from_slice(&[z, a, a]);
            if s.len().is_multiple_of(2) {
                s.push(a);
            }
            s.extend_from_slice(&[b, c]);
        } else {
            s.extend_from_slice(&[a, b, c]);
        }
        loop {
            let l = s.len();
            let (e0, e1) = (s[l - 2], s[l - 1]);
            let even = (l - 2).is_multiple_of(2);
            let next = edges.get(&key(e0, e1)).and_then(|cands| {
                cands.iter().copied().find_map(|ti| {
                    if used[ti] {
                        return None;
                    }
                    let t = tris[ti];
                    let opp = *t.iter().find(|&&v| v != e0 && v != e1)?;
                    let emitted = if even { [e0, e1, opp] } else { [e0, opp, e1] };
                    (canonical(emitted) == canonical(t)).then_some((ti, opp))
                })
            });
            match next {
                Some((ti, opp)) => {
                    used[ti] = true;
                    s.push(opp);
                }
                None => break,
            }
        }
    }
    let mut want: Vec<[u16; 3]> = tris.iter().map(|&t| canonical(t)).collect();
    let mut got: Vec<[u16; 3]> = destrip(&s).into_iter().map(canonical).collect();
    want.sort_unstable();
    got.sort_unstable();
    if want != got {
        return Err("stripify: the strip does not reproduce its input triangles".into());
    }
    Ok(s)
}

// ─────────────────────────────────────────────────────────── editing ──

/// What [`TerrainCell::displace`] changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Displacement {
    /// Vertices whose stored `POSITION.y` changed.
    pub moved_vertices: usize,
    /// Vertices whose stored normal was recomputed.
    pub renormalized_vertices: usize,
    /// Indices of the `GEOM`s whose bounds were recomputed.
    pub edited_geoms: Vec<usize>,
}

impl TerrainCell {
    /// Cell-local position of vertex `i` of `geoms[g].prmgs[p]`.
    pub fn cell_position(&self, g: usize, p: usize, i: usize) -> [f32; 3] {
        let v = self.geoms[g].prmgs[p].position(i);
        let o = self.geoms[g].poff;
        [v[0] + o[0], v[1] + o[1], v[2] + o[2]]
    }

    /// The cell's footprint as the cell-local XZ extent of its vertices: `([min_x, min_z], [max_x, max_z])`.
    /// A vertex on this boundary is shared with the neighbouring cell.
    pub fn edge_extent(&self) -> Result<([f32; 2], [f32; 2]), String> {
        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for (g, geom) in self.geoms.iter().enumerate() {
            for (p, prmg) in geom.prmgs.iter().enumerate() {
                for i in 0..prmg.vertex_count() {
                    let c = self.cell_position(g, p, i);
                    for (k, v) in [c[0], c[2]].into_iter().enumerate() {
                        lo[k] = lo[k].min(v);
                        hi[k] = hi[k].max(v);
                    }
                }
            }
        }
        if !lo.iter().chain(&hi).all(|v| v.is_finite()) {
            return Err("cell has no vertices".into());
        }
        Ok((lo, hi))
    }

    /// Move every vertex vertically by `dy(x, z)` (cell-local metres), recompute the normals the move
    /// affects, and recompute the bounds of every patch that changed and of the cell.
    ///
    /// * Only `POSITION.y` and the NORMAL's xyz halves are written; x, z, colours, texcoords, tangents and
    ///   the normal's w half are untouched. Every draw group moves, so overlays stay on the ground.
    /// * A vertex "moves" when its stored f16 changes; a `dy` below the f16 step leaves it in place.
    /// * Normals are recomputed, per draw group, for the vertices of every triangle that has a moved
    ///   corner: the area-weighted sum of its incident triangles' face normals, `(b−a)×(c−a)` in draw
    ///   winding. That is the convention the retail normals follow: on cell `0xA241BC0C`'s ground it
    ///   reproduces them to a median of 0.96° (90th percentile 6.1°; the retail normals are not an exact
    ///   function of the stored triangles — inferred: they were baked from finer source geometry).
    /// * Cell-edge vertices are never written: a `dy` that would move one is an error, and their normals
    ///   are kept, so the seam with the neighbouring cell stays closed.
    ///
    /// Fails — leaving `self` unchanged — on a non-finite `dy`, a height outside the f16 range, a moved
    /// cell-edge vertex, or a recomputed normal with no area behind it.
    pub fn displace<F: Fn(f32, f32) -> f32>(&mut self, dy: F) -> Result<Displacement, String> {
        let mut next = self.clone();
        let report = next.displace_in_place(&dy)?;
        *self = next;
        Ok(report)
    }

    fn displace_in_place(&mut self, dy: &dyn Fn(f32, f32) -> f32) -> Result<Displacement, String> {
        let (lo, hi) = self.edge_extent()?;
        let on_edge =
            |c: [f32; 3]| c[0] == lo[0] || c[0] == hi[0] || c[2] == lo[1] || c[2] == hi[1];
        let mut report = Displacement::default();

        for g in 0..self.geoms.len() {
            let mut geom_moved = false;
            for p in 0..self.geoms[g].prmgs.len() {
                let pos_off = self.geoms[g].prmgs[p].f16x4_offset(USAGE_POSITION)?;
                let nrm_off = self.geoms[g].prmgs[p].f16x4_offset(USAGE_NORMAL)?;
                let stride = self.geoms[g].prmgs[p].stride as usize;
                let n = self.geoms[g].prmgs[p].vertex_count();
                let mut moved = vec![false; n];
                let mut edge = vec![false; n];
                for (i, (m, e)) in moved.iter_mut().zip(edge.iter_mut()).enumerate() {
                    let c = self.cell_position(g, p, i);
                    *e = on_edge(c);
                    let d = dy(c[0], c[2]);
                    if !d.is_finite() {
                        return Err(format!("dy({}, {}) = {d} is not finite", c[0], c[2]));
                    }
                    let local_y = self.geoms[g].prmgs[p].position(i)[1] + d;
                    if local_y.abs() > 65504.0 {
                        return Err(format!(
                            "vertex at ({}, {}) would sit at y = {local_y}, outside f16",
                            c[0], c[2]
                        ));
                    }
                    let o = i * stride + pos_off + 2;
                    let bits = f16_le(local_y);
                    let verts = &mut self.geoms[g].prmgs[p].vertices;
                    if verts[o..o + 2] == bits {
                        continue;
                    }
                    if *e {
                        return Err(format!(
                            "dy({}, {}) = {d} moves a cell-edge vertex; its twin in the neighbouring cell \
                             would stay put and open a crack",
                            c[0], c[2]
                        ));
                    }
                    verts[o..o + 2].copy_from_slice(&bits);
                    *m = true;
                    report.moved_vertices += 1;
                }
                if !moved.contains(&true) {
                    continue;
                }
                geom_moved = true;

                // Renormalize the corners of every triangle touching a moved vertex.
                let prmg = &self.geoms[g].prmgs[p];
                let tris = prmg.triangles()?;
                let mut affected = vec![false; n];
                for t in &tris {
                    if t.iter().any(|&v| moved[v as usize]) {
                        for &v in t {
                            affected[v as usize] = !edge[v as usize];
                        }
                    }
                }
                let pos: Vec<[f64; 3]> = (0..n)
                    .map(|i| {
                        let v = prmg.position(i);
                        [v[0] as f64, v[1] as f64, v[2] as f64]
                    })
                    .collect();
                let mut acc = vec![[0f64; 3]; n];
                for t in &tris {
                    if !t.iter().any(|&v| affected[v as usize]) {
                        continue;
                    }
                    let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                    let (u, w) = (
                        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                    );
                    let f = [
                        u[1] * w[2] - u[2] * w[1],
                        u[2] * w[0] - u[0] * w[2],
                        u[0] * w[1] - u[1] * w[0],
                    ];
                    for &v in t {
                        for k in 0..3 {
                            acc[v as usize][k] += f[k];
                        }
                    }
                }
                let verts = &mut self.geoms[g].prmgs[p].vertices;
                for i in (0..n).filter(|&i| affected[i]) {
                    let s = acc[i];
                    let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                    if len.is_nan() || len <= 0.0 {
                        return Err(format!(
                            "GEOM[{g}] PRMG[{p}] vertex {i}: its incident triangles have no area, so it has no normal"
                        ));
                    }
                    let o = i * stride + nrm_off;
                    for k in 0..3 {
                        verts[o + 2 * k..o + 2 * k + 2]
                            .copy_from_slice(&f16_le((s[k] / len) as f32));
                    }
                    report.renormalized_vertices += 1;
                }
            }
            if geom_moved {
                self.recompute_geom_bounds(g);
                report.edited_geoms.push(g);
            }
        }
        if !report.edited_geoms.is_empty() {
            self.recompute_root_bounds();
        }
        Ok(report)
    }

    /// Recompute a patch's vertical extent from its stored positions (x and z are unchanged by a vertical
    /// edit and keep their authored values), then its bounding sphere from the box.
    fn recompute_geom_bounds(&mut self, g: usize) {
        let geom = &mut self.geoms[g];
        let (mut ylo, mut yhi) = (f32::INFINITY, f32::NEG_INFINITY);
        for prmg in &geom.prmgs {
            for i in 0..prmg.vertex_count() {
                let y = prmg.position(i)[1];
                ylo = ylo.min(y);
                yhi = yhi.max(y);
            }
        }
        geom.aabb.min[1] = ylo;
        geom.aabb.max[1] = yhi;
        let (centre, radius) = sphere_of(&geom.aabb);
        geom.sphere_centre = centre;
        geom.sphere_radius = radius;
    }

    /// The cell box is the union of the patch boxes, each offset by its `POFF`.
    fn recompute_root_bounds(&mut self) {
        let mut b = Aabb {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
        for geom in &self.geoms {
            for k in 0..3 {
                b.min[k] = b.min[k].min(geom.aabb.min[k] + geom.poff[k]);
                b.max[k] = b.max[k].max(geom.aabb.max[k] + geom.poff[k]);
            }
        }
        self.bounds = b;
    }
}

/// A box's bounding sphere as the retail `GEOM INFO` stores it: the centre, and the half-diagonal.
pub fn sphere_of(b: &Aabb) -> ([f32; 3], f32) {
    let c = [0, 1, 2].map(|k| (b.min[k] + b.max[k]) * 0.5);
    let h = [0, 1, 2].map(|k| (b.max[k] - b.min[k]) * 0.5);
    (c, (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt())
}

// ───────────────────────────────────────────────────────── collision ──

impl TerrainCell {
    /// The render triangles as collision soups, one per patch, in cell-local metres (the frame the retail
    /// collider uses: its vertices span ±200 m).
    ///
    /// Each patch's draw groups are merged: vertices with identical cell-local positions are welded,
    /// triangles that cover the same three welded vertices are kept once, and triangles whose corners
    /// weld together (zero area, nothing to collide with) are dropped. Winding follows the draw.
    pub fn collision_soups(&self) -> Result<Vec<MeshSoup>, String> {
        let mut soups = Vec::with_capacity(self.geoms.len());
        for (g, geom) in self.geoms.iter().enumerate() {
            let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
            let mut verts: Vec<[f32; 3]> = Vec::new();
            let mut seen: HashMap<[u32; 3], ()> = HashMap::new();
            let mut tris: Vec<[u32; 3]> = Vec::new();
            for (p, prmg) in geom.prmgs.iter().enumerate() {
                let remap: Vec<u32> = (0..prmg.vertex_count())
                    .map(|i| {
                        let c = self.cell_position(g, p, i);
                        *ids.entry(c.map(f32::to_bits)).or_insert_with(|| {
                            verts.push(c);
                            verts.len() as u32 - 1
                        })
                    })
                    .collect();
                for t in prmg.triangles()? {
                    let w = t.map(|v| remap[v as usize]);
                    if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
                        continue;
                    }
                    let mut k = w;
                    k.sort_unstable();
                    if seen.insert(k, ()).is_none() {
                        tris.push(w);
                    }
                }
            }
            if tris.is_empty() {
                return Err(format!("GEOM[{g}] renders no triangles to collide with"));
            }
            soups.push((tris, verts));
        }
        Ok(soups)
    }

    /// Replace `PHY2` with a collider built from the current render triangles
    /// ([`Self::collision_soups`]) — one `WpMeshShape16` + MOPP per patch, via
    /// [`crate::phy2_build::build_phy2_multi_hashed`] keyed by `cell_hash`.
    ///
    /// The prefix's word 2 is carried from the cell's own `PHY2` (see [`Phy2`]); every other prefix word
    /// is the builder's and must equal the retail form `[0x39, hash, _, 1, 1, 0, 0, verts, size, 0, 0, 0]`.
    /// Each baked MOPP is walked before it is accepted and must yield every one of its patch's triangle
    /// keys exactly once. Fails if `cell_hash` is not the hash the cell's `PHY2` already carries.
    pub fn rebuild_collision(&mut self, cell_hash: u32) -> Result<(), String> {
        if self.phy2.prefix[1] != cell_hash {
            return Err(format!(
                "PHY2 belongs to {:#010X}, not {cell_hash:#010X}",
                self.phy2.prefix[1]
            ));
        }
        let soups = self.collision_soups()?;
        let body = build_phy2_multi_hashed(cell_hash, &soups)?;
        let packfile = crate::havok::parse_phy2_body(&body)?;
        let meshes = packfile
            .shapes
            .iter()
            .filter(|s| matches!(s, crate::havok::Shape::Mesh(_)))
            .count();
        if meshes != soups.len() {
            return Err(format!(
                "rebuilt PHY2 re-parses to {meshes} meshes, built {}",
                soups.len()
            ));
        }
        let mopps = crate::mopp::extract_mopp_buffers(&body);
        if mopps.len() != soups.len() {
            return Err(format!(
                "rebuilt PHY2 carries {} MOPPs for {} meshes",
                mopps.len(),
                soups.len()
            ));
        }
        for (i, (code, (tris, _))) in mopps.iter().zip(&soups).enumerate() {
            let walk = crate::mopp::decode(code);
            if let Some(e) = walk.error {
                return Err(format!("patch {i}: baked MOPP does not walk: {e}"));
            }
            let mut keys = walk.keys;
            keys.sort_unstable();
            if keys.len() != tris.len() || keys.iter().enumerate().any(|(k, &v)| v != k as u32) {
                return Err(format!(
                    "patch {i}: baked MOPP yields {} keys, not each of 0..{} once",
                    keys.len(),
                    tris.len()
                ));
            }
        }
        let mut prefix = [0u32; PHY2_PREFIX_WORDS];
        for (k, w) in prefix.iter_mut().enumerate() {
            *w = u32_at(&body, 4 * k);
        }
        let fixed = [
            (0, PHY2_TAG),
            (3, 1),
            (4, 1),
            (5, 0),
            (6, 0),
            (9, 0),
            (10, 0),
            (11, 0),
        ];
        for (k, want) in fixed {
            if prefix[k] != want {
                return Err(format!(
                    "builder wrote PHY2 prefix word {k} = {:#X}, retail form is {want:#X}",
                    prefix[k]
                ));
            }
        }
        prefix[2] = self.phy2.prefix[2];
        self.phy2 = Phy2 {
            prefix,
            payload: body[4 * PHY2_PREFIX_WORDS..].to_vec(),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A patch-local ground grid, `n × n` quads of `cell` metres centred on the origin, heights from `h`,
    /// drawn as one strip per row pair with the retail 20-byte `POSITION·D3DCOLOR·NORMAL` decl.
    fn ground(n: u16, cell: f32, h: impl Fn(f32, f32) -> f32) -> Prmg {
        let half = n as f32 * cell / 2.0;
        let decl = vec![
            DeclElement {
                stream: 0,
                offset: 0,
                ty: 16,
                method: 0,
                usage: 0,
                usage_index: 0,
            },
            DeclElement {
                stream: 0,
                offset: 8,
                ty: 4,
                method: 0,
                usage: 10,
                usage_index: 0,
            },
            DeclElement {
                stream: 0,
                offset: 12,
                ty: 16,
                method: 0,
                usage: 3,
                usage_index: 0,
            },
        ];
        let mut vertices = Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                let (px, pz) = (x as f32 * cell - half, z as f32 * cell - half);
                for c in [px, h(px, pz), pz, 1.0] {
                    vertices.extend_from_slice(&f16_le(c));
                }
                vertices.extend_from_slice(&[0x80, 0x40, 0x20, 0xFF]);
                for c in [0.0, 1.0, 0.0, 1.0] {
                    vertices.extend_from_slice(&f16_le(c));
                }
            }
        }
        let w = n + 1;
        let mut tris = Vec::new();
        for z in 0..n {
            for x in 0..n {
                let a = z * w + x;
                tris.push([a, a + w, a + 1]);
                tris.push([a + 1, a + w, a + w + 1]);
            }
        }
        let indices = stripify(&tris).unwrap();
        let prim_count = (indices.len() - 2) as u16;
        let draw = Draw {
            hash: 0x16E4_944B,
            field_4: 0,
            start_index: 0,
            prim_count,
            min_index: 0,
            max_index: w * w - 1,
            field_18: w * w,
        };
        let group = |i: u16, c: u32| PassGroup {
            draw_count: c,
            field_4: 0,
            field_6: 1,
            alt_draw_count: 1,
            index: i,
        };
        Prmg {
            pass_groups: [group(0, 1), group(1, 0), group(2, 0)],
            draws: vec![draw],
            alt_draws: vec![draw; 3],
            stride: 20,
            decl,
            vertices,
            indices,
        }
    }

    /// A 2 × 2-patch synthetic cell (200 m) with a real collider, bounds consistent with its vertices.
    fn synthetic_cell() -> TerrainCell {
        let hgt = |x: f32, z: f32| 0.05 * x - 0.03 * z;
        let mut geoms = Vec::new();
        for (px, pz) in [(-50.0, -50.0), (50.0, -50.0), (-50.0, 50.0), (50.0, 50.0)] {
            let prmg = ground(10, 10.0, |x, z| hgt(x + px, z + pz));
            let mut aabb = Aabb {
                min: [-50.0, f32::INFINITY, -50.0],
                max: [50.0, f32::NEG_INFINITY, 50.0],
            };
            for i in 0..prmg.vertex_count() {
                let y = prmg.position(i)[1];
                aabb.min[1] = aabb.min[1].min(y);
                aabb.max[1] = aabb.max[1].max(y);
            }
            let (sphere_centre, sphere_radius) = sphere_of(&aabb);
            geoms.push(Geom {
                sphere_centre,
                sphere_radius,
                aabb,
                poff: [px, 0.0, pz],
                prmgs: vec![prmg],
            });
        }
        let material = Material {
            word_0: 0xCC71_C3C2,
            params: [1.0; 25],
            flags: 0,
            textures: vec![0x4EEE_3BCA, 0xA3CD_72A7],
            tail: [7, 9],
        };
        let mut cell = TerrainCell {
            bounds: Aabb {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            mtrl: Mtrl {
                materials: vec![material],
                blocks: vec![[0x3F80_0000; MATERIAL_BLOCK_WORDS]],
            },
            geoms,
            phy2: Phy2 {
                prefix: [0; PHY2_PREFIX_WORDS],
                payload: Vec::new(),
            },
        };
        cell.recompute_root_bounds();
        let body = build_phy2_multi_hashed(0x1234_5678, &cell.collision_soups().unwrap()).unwrap();
        let mut prefix = [0u32; PHY2_PREFIX_WORDS];
        for (k, w) in prefix.iter_mut().enumerate() {
            *w = u32_at(&body, 4 * k);
        }
        prefix[2] = 2;
        cell.phy2 = Phy2 {
            prefix,
            payload: body[48..].to_vec(),
        };
        cell
    }

    #[test]
    fn synthetic_cell_round_trips() {
        let cell = synthetic_cell();
        let bytes = cell.encode().unwrap();
        let back = TerrainCell::decode(&bytes).unwrap();
        assert_eq!(back, cell);
        assert_eq!(back.encode().unwrap(), bytes);
        assert!(crate::ucfx::verify_ucfx_container(&bytes, "synthetic", TYPE_HASH).is_none());
    }

    #[test]
    fn decode_refuses_a_stale_checksum_and_a_gap() {
        let bytes = synthetic_cell().encode().unwrap();
        let mut bad = bytes.clone();
        let at = bad.len() - 20;
        bad[at] ^= 1;
        assert!(TerrainCell::decode(&bad).unwrap_err().contains("CSUM"));

        // Row 1 (MTRL) moved one byte later: a gap the encoder would close.
        let mut gap = bytes.clone();
        let r = HEADER + ROW + 4;
        let u0 = u32_at(&gap, r) + 1;
        gap[r..r + 4].copy_from_slice(&u0.to_le_bytes());
        let body = gap.len() - 8;
        let crc = crc32_mercs2(&gap[..body]);
        gap[body + 4..].copy_from_slice(&crc.to_le_bytes());
        assert!(TerrainCell::decode(&gap).unwrap_err().contains("packed"));
    }

    #[test]
    fn stripify_then_destrip_keeps_every_triangle_and_its_winding() {
        let prmg = ground(12, 1.0, |_, _| 0.0);
        let mut tris = prmg.triangles().unwrap();
        // Mixed orientation and a detached, clockwise triangle: extension must refuse wrong-winding
        // neighbours rather than flip them.
        tris[5] = [tris[5][0], tris[5][2], tris[5][1]];
        tris.push([500, 400, 300]);
        tris.push([300, 400, 500]);
        let strip = stripify(&tris).unwrap();
        let mut want: Vec<[u16; 3]> = tris.iter().map(|&t| canonical(t)).collect();
        let mut got: Vec<[u16; 3]> = destrip(&strip).into_iter().map(canonical).collect();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(got, want);
        assert!(stripify(&[[1, 1, 2]]).is_err());
    }

    #[test]
    fn a_draw_restarts_strip_parity() {
        // [0,1,2,3] then a second draw [2,3,4,5]: each draw's first triangle is un-reversed.
        let s = [0u16, 1, 2, 3, 2, 3, 4, 5];
        assert_eq!(destrip(&s[..4]), vec![[0, 1, 2], [1, 3, 2]]);
        assert_eq!(destrip(&s[4..]), vec![[2, 3, 4], [3, 5, 4]]);
    }

    #[test]
    fn displacement_touches_only_position_y_and_normals() {
        let cell = synthetic_cell();
        let mut edited = cell.clone();
        let bump =
            |x: f32, z: f32| 8.0 * (1.0 - ((x - 10.0).abs().max((z + 5.0).abs()) / 30.0)).max(0.0);
        let r = edited.displace(bump).unwrap();
        assert!(r.moved_vertices > 0 && r.renormalized_vertices > 0);
        for (g, (a, b)) in cell.geoms.iter().zip(&edited.geoms).enumerate() {
            for (pa, pb) in a.prmgs.iter().zip(&b.prmgs) {
                assert_eq!(
                    (&pa.decl, &pa.indices, &pa.draws, &pa.alt_draws),
                    (&pb.decl, &pb.indices, &pb.draws, &pb.alt_draws)
                );
                for i in 0..pa.vertex_count() {
                    for byte in 0..20 {
                        let (x, y) = (pa.vertices[i * 20 + byte], pb.vertices[i * 20 + byte]);
                        let allowed = matches!(byte, 2 | 3 | 12..=17);
                        assert!(
                            x == y || allowed,
                            "GEOM[{g}] vertex {i} byte {byte} changed"
                        );
                    }
                }
            }
            if !r.edited_geoms.contains(&g) {
                assert_eq!(a, b, "an unedited patch changed");
            }
        }
        assert_eq!(cell.mtrl, edited.mtrl);
        assert_eq!(cell.phy2, edited.phy2);
    }

    #[test]
    fn a_zero_displacement_is_a_no_op() {
        let cell = synthetic_cell();
        let mut same = cell.clone();
        let r = same.displace(|_, _| 0.0).unwrap();
        assert_eq!(r, Displacement::default());
        assert_eq!(same.encode().unwrap(), cell.encode().unwrap());
    }

    #[test]
    fn moving_a_cell_edge_vertex_is_refused_and_leaves_the_cell_unchanged() {
        let cell = synthetic_cell();
        let mut edited = cell.clone();
        let err = edited
            .displace(|x, _| if x > 90.0 { 1.0 } else { 0.0 })
            .unwrap_err();
        assert!(err.contains("cell-edge"), "{err}");
        assert_eq!(edited, cell);
    }

    #[test]
    fn rebuilt_collision_parses_and_never_misses_an_edited_triangle() {
        let mut cell = synthetic_cell();
        cell.displace(|x, z| {
            12.0 * (1.0 - ((x + 40.0).abs().max((z - 30.0).abs()) / 25.0)).max(0.0)
        })
        .unwrap();
        cell.rebuild_collision(0x1234_5678).unwrap();
        assert!(cell
            .rebuild_collision(0xDEAD_BEEF)
            .unwrap_err()
            .contains("belongs to"));
        let bytes = cell.encode().unwrap();
        let back = TerrainCell::decode(&bytes).unwrap();
        let body: Vec<u8> = back
            .phy2
            .prefix
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .chain(back.phy2.payload.clone())
            .collect();
        crate::havok::parse_phy2_body(&body).unwrap();
        assert_eq!(back.phy2.prefix[2], 2, "word 2 is carried from the cell");
        let soups = back.collision_soups().unwrap();
        let mopps = crate::mopp::extract_mopp_with_info(&body);
        assert_eq!(mopps.len(), soups.len());
        for ((code, info), (tris, verts)) in mopps.iter().zip(&soups) {
            for (k, t) in tris.iter().enumerate() {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for &v in t {
                    for a in 0..3 {
                        lo[a] = lo[a].min(verts[v as usize][a]);
                        hi[a] = hi[a].max(verts[v as usize][a]);
                    }
                }
                assert!(
                    crate::mopp::query_aabb(code, info, lo, hi).contains(&(k as u32)),
                    "MOPP misses triangle {k}"
                );
            }
        }
    }
}
