//! A PS3 `model` chunk in PC form.
//!
//! PS3 draws a group as segments for the SPUs: each `PRMT` draw has its own vertices, in `SSPU` as half
//! float positions and packed normals, and its own triangles, compressed in `IBUF`, with a skin block of
//! `(weight, bone)` pairs after the vertices. `STRM` holds only UVs and colours. PC draws one interleaved
//! vertex buffer and one strip per group, so a group is rebuilt whole. Everything outside `GEOM` is the
//! same data as Xbox's and takes the normal big-endian route. `docs/reverse_engineer/ps3_model_format.md`
//! holds the layouts and what each is proven against.
//!
//! Positions, UVs, colours, weights, bones, morphs, triangles and winding come across exact. PS3 normalised
//! normals and tangents and kept 10 bits a component, so those are close, not equal. A palette is built from
//! the bones the vertices use, a list B from the materials, and `AREA` from the triangles.

use std::collections::{BTreeSet, HashSet};

use crate::crc32::crc32_mercs2;
use crate::model::{
    encode_mesh_group, encode_skin_group, encode_tiny_group, BoneRange, DeclElement, DeclType, DeclUsage, MeshGroup,
    MorphTarget, PrimRecord, SkinGroup, VertexStream, SKIN_RANGE_SLOTS,
};
use crate::model_inject::f16_le;
use crate::terrainmesh::stripify;
use crate::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode};

/// A material flag: drawn by the shadow pass alone, so in no list A.
const MATERIAL_SHADOW_ONLY: u16 = 0x800;
/// A material flag: alpha tested, so the shadow pass needs its own draw of it.
const MATERIAL_ALPHA_TESTED: u16 = 0x08;
/// Bones a skinned vertex shader's `BoneMatrixArray` holds.
const PALETTE_BONES: usize = 64;

/// `PRMT` draw flags.
const DRAW_SKIN: u32 = 1;
const DRAW_TINY: u32 = 2;
const DRAW_POSITION_W: u32 = 4;

/// The half float 1.0, a vertex's W where PS3 stores none.
const HALF_ONE: u16 = 0x3C00;

/// The PC vertex shader of each PS3 colour-pass key, by group kind: `{mesh with W, skin, skin with
/// morphs, skin with W}`. PS3 names one family and draws its segments on the SPUs; PC has a shader for
/// each. 57,082 retail groups map this way on both PC and Xbox, and the DLC holds no other pair.
const VERTEX_SHADERS: [(u32, [u32; 4]); 6] = [
    (0xDC35_1FCB, [0xFD98_2263, 0x4884_5C5F, 0x816E_B8F7, 0x517A_1957]),
    (0x7B15_3245, [0x6783_C8A9, 0x4E8F_3AE1, 0x6004_0CC9, 0x46AE_9815]),
    (0x79F4_CCB3, [0, 0xB267_7DD7, 0x22BC_2FEF, 0]),
    (0x3595_7B11, [0, 0x9C28_AE3D, 0, 0]),
    (0x98D6_86E5, [0xA591_DE9D, 0xA926_B029, 0, 0xB8C0_E6E1]),
    (0x03F6_D1F8, [0, 0x375F_656C, 0, 0]),
];
/// The same for the z and shadow pass.
const SHADOW_SHADERS: [(u32, [u32; 4]); 2] = [
    (0xA9EA_4CF7, [0xF971_E10F, 0x0F8F_AE93, 0x6104_00CB, 0x46A0_41AB]),
    (0x3B33_AA5E, [0xB45C_5A90, 0xDD36_22C2, 0xA243_2A1A, 0x6884_DC24]),
];
/// A TINY group's `(PC vertex shader, shadow of 0x3B33AA5E, shadow of 0xA9EA4CF7)`, by its draws' `e2`.
const TINY_SHADERS: [(u16, u32, u32, u32); 2] = [(1, 0xCFF1_99A5, 0x8239_633C, 0xC4D5_6BD5), (3, 0x1A8E_1E26, 0x2B87_4075, 0xBA10_97D6)];

pub(super) fn be16(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| format!("a u16 at {o} is past {} bytes", b.len()))
}

pub(super) fn be32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("a u32 at {o} is past {} bytes", b.len()))
}

/// Whether a big-endian container is a PS3 one with geometry: an `SSPU` row, stored reversed.
pub(super) fn has_sspu(container: &[u8]) -> bool {
    let rows = be32(container, 16).unwrap_or(0) as usize;
    (0..rows).any(|i| container.get(20 + i * 20..24 + i * 20) == Some(b"UPSS"))
}

/// The flags of each material in a big-endian `MTRL`: 116 bytes and 4 per texture a record, flags at
/// +104, the texture count at +106.
pub(super) fn material_flags(mtrl: &[u8]) -> Result<Vec<u16>, String> {
    let (mut out, mut p) = (Vec::new(), 0);
    while p < mtrl.len() {
        out.push(be16(mtrl, p + 104)?);
        p += 116 + 4 * be16(mtrl, p + 106)? as usize;
    }
    Ok(out)
}

/// The tree of a big-endian container: its rows read as little-endian, its bodies as stored.
pub(super) fn be_tree(container: &[u8]) -> Result<Vec<UcfxNode>, String> {
    let mut c = swapped_table(container, be32(container, 16)? as usize)?;
    let sum = crc32_mercs2(&c);
    c.extend_from_slice(b"CSUM");
    c.extend_from_slice(&sum.to_le_bytes());
    parse_ucfx_tree(&c)
}

/// A container of `rows` rows with its header and rows byte-swapped and its bodies left alone.
fn swapped_table(container: &[u8], rows: usize) -> Result<Vec<u8>, String> {
    let mut c = container.to_vec();
    let end = 20 + rows * 20;
    if end > c.len() {
        return Err(format!("{rows} rows do not fit {} bytes", c.len()));
    }
    c[0..4].reverse();
    for w in c[4..end].chunks_exact_mut(4) {
        w.reverse();
    }
    Ok(c)
}

/// Convert a PS3 model. `flags` is the flags of each material of the model, which a lower rung's chunk
/// does not hold. `rest` converts a big-endian container of what is outside `GEOM`.
pub(super) fn convert(
    container: &[u8],
    flags: &[u16],
    rest: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let roots = be_tree(container)?;
    let at = roots.iter().position(|n| &n.tag == b"GEOM").ok_or("a PS3 model has no GEOM")?;
    let geom = convert_geom(&roots[at], flags)?;
    let others: Vec<UcfxNode> = roots.iter().enumerate().filter(|(i, _)| *i != at).map(|(_, n)| n.clone()).collect();
    let mut tree = if others.is_empty() {
        Vec::new()
    } else {
        let bytes = write_ucfx_tree(&others);
        let body = bytes.len() - crate::ucfx::UCFX_CSUM_BYTES;
        let rows = others.iter().map(UcfxNode::row_count).sum();
        let mut sealed = rest(&swapped_table(&bytes[..body], rows)?)?;
        let sum = crc32_mercs2(&sealed);
        sealed.extend_from_slice(b"CSUM");
        sealed.extend_from_slice(&sum.to_le_bytes());
        parse_ucfx_tree(&sealed)?
    };
    tree.insert(at, geom);
    let mut out = write_ucfx_tree(&tree);
    out.truncate(out.len() - crate::ucfx::UCFX_CSUM_BYTES);
    Ok(out)
}

pub(super) fn row<'a>(kids: &'a [UcfxNode], tag: &[u8; 4]) -> Option<&'a [u8]> {
    kids.iter().find(|n| &n.tag == tag).and_then(|n| n.body.as_deref())
}

fn convert_geom(geom: &UcfxNode, flags: &[u16]) -> Result<UcfxNode, String> {
    let mut kids = Vec::with_capacity(geom.children.len());
    for n in &geom.children {
        kids.push(match &n.tag {
            b"INFO" => UcfxNode::leaf(n.tag, n.body.as_deref().ok_or("GEOM INFO has no body")?.iter().rev().copied().collect()),
            b"INDX" => {
                let b = n.body.as_deref().ok_or("GEOM INDX has no body")?;
                UcfxNode::leaf(n.tag, b.chunks_exact(2).flat_map(|s| [s[1], s[0]]).collect())
            }
            b"MESH" | b"SKIN" | b"TINY" => {
                let mut sub = Vec::with_capacity(n.children.len());
                for k in &n.children {
                    sub.push(match &k.tag {
                        b"INFO" => UcfxNode::leaf(k.tag, k.body.as_deref().ok_or("a sub-object INFO has no body")?.iter().rev().copied().collect()),
                        b"PRMG" => convert_group(&k.children, flags)?,
                        t => return Err(format!("a PS3 sub-object holds {}", String::from_utf8_lossy(t))),
                    });
                }
                UcfxNode::marker(n.tag, sub)
            }
            t => return Err(format!("a PS3 GEOM holds {}", String::from_utf8_lossy(t))),
        });
    }
    Ok(UcfxNode::marker(*b"GEOM", kids))
}

/// One `PRMT` draw: row A `{u16 material, u16 hole, u32 base vertex}`, then row B.
pub(super) struct Draw {
    material: u16,
    base_vertex: usize,
    pub(super) vertices: usize,
    pub(super) sspu_offset: usize,
    sspu_bytes: usize,
    pub(super) format: u16,
    pub(super) indices: usize,
    pub(super) ibuf_offset: usize,
    ibuf_bytes: usize,
    e1: u16,
    e2: u16,
    e3: usize,
    pub(super) flags: u32,
}

/// The draw of the row B at `b`, with the fields of its row A.
pub(super) fn read_row_b(prmt: &[u8], b: usize, material: u16, base_vertex: usize) -> Result<Draw, String> {
    let format = be16(prmt, b + 8)?;
    if be16(prmt, b + 10)? != format {
        return Err("a draw names two formats".into());
    }
    Ok(Draw {
        material,
        base_vertex,
        sspu_bytes: be16(prmt, b)? as usize,
        vertices: be16(prmt, b + 2)? as usize,
        sspu_offset: be32(prmt, b + 4)? as usize,
        format,
        indices: be16(prmt, b + 12)? as usize,
        ibuf_bytes: be16(prmt, b + 14)? as usize,
        ibuf_offset: be32(prmt, b + 16)? as usize,
        e1: be16(prmt, b + 20)?,
        e2: be16(prmt, b + 22)?,
        e3: be32(prmt, b + 24)? as usize,
        flags: be32(prmt, b + 28)?,
    })
}

fn read_draws(prmt: &[u8], n: usize) -> Result<Vec<Draw>, String> {
    if prmt.len() != 40 * n {
        return Err(format!("a PRMT of {} bytes for {n} draws", prmt.len()));
    }
    (0..n).map(|d| read_row_b(prmt, 8 * n + 32 * d, be16(prmt, 8 * d)?, be32(prmt, 8 * d + 4)? as usize)).collect()
}

/// `n` bits at bit `at`, most significant first.
fn bits(b: &[u8], at: usize, n: usize) -> Result<u32, String> {
    (at..at + n).try_fold(0u32, |v, i| {
        let byte = b.get(i / 8).ok_or_else(|| format!("bit {i} is past {} bytes", b.len()))?;
        Ok((v << 1) | (byte >> (7 - i % 8) & 1) as u32)
    })
}

/// The explicit indices of a draw at byte `at`: `count` values of `width` bits, each less `base` and
/// added to the index 8 before it.
fn explicit_indices(piece: &[u8], at: usize, count: usize, width: usize, base: i32) -> Result<Vec<i32>, String> {
    let mut list = Vec::with_capacity(count);
    for i in 0..count {
        let value = bits(piece, at * 8 + i * width, width)? as i32 - base;
        list.push(value + if i >= 8 { list[i - 8] } else { 0 });
    }
    Ok(list)
}

/// One draw's triangles from `IBUF`, as indices into its vertices. A header `{u16 explicit count, u16
/// delta base, u16 flag bytes, u8 delta bits, u8 0}`, then a flag bit for each fresh corner (0 the next new
/// vertex, 1 the next explicit index), a 2-bit code for each triangle, and the explicit indices, each
/// delta-coded against the index 8 before it. Code 3 is three fresh corners; codes 0, 1 and 2 keep an edge
/// of the last triangle, `(a, c)`, `(c, b)` or `(b, a)`, and add one.
pub(super) fn read_triangles(piece: &[u8], indices: usize, vertices: usize) -> Result<Vec<[u16; 3]>, String> {
    if indices % 3 != 0 {
        return Err(format!("{indices} indices are no triangle list"));
    }
    let (explicit, base, flag_bytes) = (be16(piece, 0)? as usize, be16(piece, 2)? as i32, be16(piece, 4)? as usize);
    let width = *piece.get(6).ok_or("a short IBUF piece")? as usize;
    let tris = indices / 3;
    let codes_at = 8 + flag_bytes;
    let list = explicit_indices(piece, codes_at + (tris * 2).div_ceil(8), explicit, width, base)?;
    let (mut new, mut flag, mut next) = (0usize, 0usize, 0usize);
    let mut corner = || -> Result<u16, String> {
        flag += 1;
        if bits(piece, 8 * 8 + flag - 1, 1)? == 0 {
            new += 1;
            return Ok(new as u16 - 1);
        }
        next += 1;
        let v = *list.get(next - 1).ok_or("a corner past the explicit indices")?;
        u16::try_from(v).map_err(|_| format!("the index {v}"))
    };
    let (mut a, mut b, mut c) = (0u16, 0u16, 0u16);
    let mut out = Vec::with_capacity(tris);
    for t in 0..tris {
        match bits(piece, codes_at * 8 + t * 2, 2)? {
            3 => {
                a = corner()?;
                b = corner()?;
                c = corner()?;
            }
            0 => (b, c) = (c, corner()?),
            1 => (a, c) = (c, corner()?),
            _ => (a, b, c) = (b, a, corner()?),
        }
        out.push([a, b, c]);
    }
    if new != vertices {
        return Err(format!("a draw's indices name {new} new vertices of {vertices}"));
    }
    if next != explicit {
        return Err(format!("a draw's indices use {next} of {explicit} explicit values"));
    }
    if out.iter().flatten().any(|i| *i as usize >= vertices) {
        return Err(format!("a draw's index is past its {vertices} vertices"));
    }
    Ok(out)
}

/// One stored 10-bit component of a unit vector. Stored as a floor over `±√½`, so the bin's middle, save
/// the values a retail vector lands on exactly.
fn unit_component(v: u32) -> f32 {
    match v {
        1023 => std::f32::consts::FRAC_1_SQRT_2,
        0 => -std::f32::consts::FRAC_1_SQRT_2,
        512 => 0.0,
        _ => (v as f32 - 511.5) * std::f32::consts::SQRT_2 / 1024.0,
    }
}

/// A 3-byte unit vector, `p:10 q:10 axis:2 hand:1 sign:1` from the top bit: `axis` is the component
/// recomputed from `p` and `q`, the other two in axis order. Returns the vector and its handedness.
pub(super) fn unit_vector(b: &[u8]) -> Result<([f32; 3], f32), String> {
    let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
    let axis = (v >> 2 & 3) as usize;
    if axis == 3 {
        return Err(format!("the unit vector {v:06X} names axis 3"));
    }
    let (p, q) = (unit_component(v >> 14), unit_component(v >> 4 & 1023));
    let mut r = (1.0 - p * p - q * q).max(0.0).sqrt();
    if v & 1 == 0 {
        r = -r;
    }
    let mut out = [p, q, 0.0];
    out.copy_within(axis..2, axis + 1);
    out[axis] = r;
    let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
    Ok((out.map(|c| c / len), if v & 2 != 0 { 1.0 } else { -1.0 }))
}

/// A TINY group's normal, the same 11:11:10 as Xbox's `HEND3N`: `x` in the low bits.
fn hend3n(v: u32) -> [f32; 3] {
    let signed = |x: u32, bits: u32, max: f32| (if x >= 1 << (bits - 1) { x as i32 - (1 << bits) } else { x as i32 }) as f32 / max;
    [signed(v & 0x7FF, 11, 1023.0), signed(v >> 11 & 0x7FF, 11, 1023.0), signed(v >> 22, 10, 511.0)]
}

fn half_of(b: &[u8], o: usize) -> Result<[u8; 2], String> {
    let v = be16(b, o)?;
    Ok(v.to_le_bytes())
}

pub(super) fn half_float(h: u16) -> f32 {
    let (s, e, m) = (h >> 15, (h >> 10 & 31) as i32, (h & 1023) as f32);
    let v = if e == 0 { m * 2f32.powi(-24) } else { (1.0 + m / 1024.0) * 2f32.powi(e - 15) };
    if s == 1 {
        -v
    } else {
        v
    }
}

pub(super) fn put_vector(out: &mut Vec<u8>, v: [f32; 3], w: f32) {
    for c in v {
        out.extend_from_slice(&f16_le(c));
    }
    out.extend_from_slice(&f16_le(w));
}

/// One group's vertex attributes as the `STRM` decl lays them out.
struct Stream<'a> {
    data: &'a [u8],
    stride: usize,
    /// The second row: `(type, offset)`.
    extra: Option<(u8, usize)>,
    uv: bool,
}

fn read_stream<'a>(kids: &'a [UcfxNode], draws: &[Draw]) -> Result<Stream<'a>, String> {
    let strm = kids.iter().find(|n| &n.tag == b"STRM").ok_or("a group has no STRM")?;
    let info = row(&strm.children, b"info").ok_or("a STRM has no info")?;
    let stride = be32(info, 4)? as usize;
    let data = row(&strm.children, b"data").unwrap_or(&[]);
    let decl = row(&strm.children, b"decl").unwrap_or(&[]);
    if data.len() != stride * be32(info, 8)? as usize {
        return Err(format!("a STRM of {} bytes for stride {stride}", data.len()));
    }
    let rows: Vec<&[u8]> = decl.chunks_exact(4).collect();
    if let Some(r) = rows.first() {
        if r[0] != 3 {
            return Err(format!("a STRM holds UVs of type {}", r[0]));
        }
    }
    let extra = rows.get(1).map(|r| (r[0], r[1] as usize));
    if extra.is_some_and(|(t, _)| t != 4 && t != 6) || rows.len() > 2 {
        return Err(format!("the STRM decl {decl:02X?}"));
    }
    let uv = !rows.is_empty() || draws.iter().all(|d| d.format == 3);
    Ok(Stream { data, stride, extra, uv })
}

/// The PC declaration of a group, and its stride.
fn pc_elements(uv: bool, colour: bool, skin: bool, tangent: bool, morphs: bool) -> Vec<DeclElement> {
    let mut at = 0u16;
    let mut out = Vec::new();
    let mut push = |ty: DeclType, usage: DeclUsage| {
        out.push(DeclElement { stream: 0, offset: at, ty, usage, usage_index: 0 });
        at += ty.size() as u16;
    };
    push(DeclType::Float16x4, DeclUsage::Position);
    if uv {
        push(DeclType::Float16x2, DeclUsage::TexCoord);
    }
    if colour {
        push(DeclType::D3dColor, DeclUsage::Color);
    }
    if skin {
        push(DeclType::UByte4, DeclUsage::BlendIndices);
        push(DeclType::UByte4N, DeclUsage::BlendWeight);
    }
    push(DeclType::Float16x4, DeclUsage::Normal);
    if tangent {
        push(DeclType::Float16x4, DeclUsage::Tangent);
    }
    if morphs {
        for k in 1..=6u8 {
            out.push(DeclElement { stream: k as u16, offset: 0, ty: DeclType::Float16x4, usage: DeclUsage::TexCoord, usage_index: k });
        }
    }
    out
}

/// Ranges of bones covering `used`: contiguous runs, the two with the smallest gap merged until they fit
/// the group's slots. Returns the ranges and each bone's place in them. Refuses more than the shader holds.
fn palette(used: &BTreeSet<u16>) -> Result<Vec<BoneRange>, String> {
    let mut runs: Vec<(u16, u16)> = Vec::new();
    for &b in used {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == b => *end = b,
            _ => runs.push((b, b)),
        }
    }
    while runs.len() > SKIN_RANGE_SLOTS {
        let at = (1..runs.len()).min_by_key(|&i| runs[i].0 - runs[i - 1].1).ok_or("no runs to merge")?;
        runs[at - 1].1 = runs[at].1;
        runs.remove(at);
    }
    let ranges: Vec<BoneRange> = runs.iter().map(|&(a, b)| BoneRange { hier_base: a, count: b - a + 1 }).collect();
    let total: usize = ranges.iter().map(|r| r.count as usize).sum();
    if total > PALETTE_BONES {
        return Err(format!("a group's palette needs {total} bones and a shader holds {PALETTE_BONES}"));
    }
    Ok(ranges)
}

fn palette_slot(ranges: &[BoneRange], bone: u16) -> Option<u8> {
    let mut at = 0u16;
    for r in ranges {
        if (r.hier_base..r.hier_base + r.count).contains(&bone) {
            return Some((at + bone - r.hier_base) as u8);
        }
        at += r.count;
    }
    None
}

/// Which column of the shader tables a group takes.
fn shader_column(flags: u32, morphs: bool) -> Result<Option<usize>, String> {
    match (flags, morphs) {
        (0, false) => Ok(None),
        (DRAW_POSITION_W, false) => Ok(Some(0)),
        (DRAW_SKIN, false) => Ok(Some(1)),
        (DRAW_SKIN, true) => Ok(Some(2)),
        (f, false) if f == DRAW_SKIN | DRAW_POSITION_W => Ok(Some(3)),
        (f, m) => Err(format!("a group of draw flags {f} and morphs {m}")),
    }
}

fn pc_shader(table: &[(u32, [u32; 4])], key: u32, column: Option<usize>) -> Result<u32, String> {
    let Some(c) = column else { return Ok(key) };
    match table.iter().find(|(k, _)| *k == key).map(|(_, row)| row[c]) {
        Some(pc) if pc != 0 => Ok(pc),
        _ => Err(format!("vertex shader 0x{key:08X} has no PC key for column {c}")),
    }
}

/// A list B or list A record over a strip of `run` at `at`.
fn record(material: u16, at: usize, run: &[u16]) -> PrimRecord {
    let distinct: HashSet<&u16> = run.iter().collect();
    PrimRecord {
        material: material as u32,
        start: at as u32,
        prims: run.len().saturating_sub(2) as u16,
        min_index: run.iter().copied().min().unwrap_or(0),
        max_index: run.iter().copied().max().unwrap_or(0),
        unique_count: distinct.len() as u16,
    }
}

/// A draw's strip, degenerate triangles dropped: a strip cannot carry one and they draw nothing.
fn strip_of(tris: &[[u16; 3]]) -> Result<Vec<u16>, String> {
    let real: Vec<[u16; 3]> = tris.iter().copied().filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    stripify(&real)
}

/// The group's strip and its two lists. List A is one record per draw, in PS3's order, less the shadow-only
/// materials. List B is one record per alpha-tested material and one for all the rest, a draw's own record
/// where that is one draw, a strip of its own after list A's otherwise.
fn lists(
    draws: &[Draw],
    triangles: &[Vec<[u16; 3]>],
    flags: &[u16],
) -> Result<(Vec<u16>, Vec<PrimRecord>, Vec<PrimRecord>), String> {
    let flag = |m: u16| flags.get(m as usize).copied().ok_or_else(|| format!("material {m} of {}", flags.len()));
    let mut strip = Vec::new();
    let mut own: Vec<Option<PrimRecord>> = Vec::with_capacity(draws.len());
    for (d, t) in draws.iter().zip(triangles) {
        if flag(d.material)? & MATERIAL_SHADOW_ONLY != 0 {
            own.push(None);
            continue;
        }
        let run = strip_of(t)?;
        own.push(Some(record(d.material, strip.len(), &run)));
        strip.extend_from_slice(&run);
    }
    let list_a: Vec<PrimRecord> = own.iter().flatten().copied().collect();

    // Draws by shadow record, in the order each record first appears.
    let mut shadow: Vec<(Option<u16>, Vec<usize>)> = Vec::new();
    for (i, d) in draws.iter().enumerate() {
        let key = (flag(d.material)? & MATERIAL_ALPHA_TESTED != 0).then_some(d.material);
        match shadow.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(i),
            None => shadow.push((key, vec![i])),
        }
    }
    let mut list_b = Vec::with_capacity(shadow.len());
    for (_, members) in shadow {
        if let ([only], Some(Some(r))) = (&members[..], members.first().map(|&m| own[m])) {
            debug_assert_eq!(draws[*only].material as u32, r.material);
            list_b.push(r);
            continue;
        }
        let tris: Vec<[u16; 3]> = members.iter().flat_map(|&m| triangles[m].iter().copied()).collect();
        let run = strip_of(&tris)?;
        list_b.push(record(draws[members[0]].material, strip.len(), &run));
        strip.extend_from_slice(&run);
    }
    Ok((strip, list_a, list_b))
}

/// `AREA`: the area of each strip triangle a record draws as a half float, and 0 for the rest.
fn areas(strip: &[u16], records: &[PrimRecord], positions: &[[f32; 3]]) -> Vec<u16> {
    let mut out = vec![0u16; strip.len().saturating_sub(2)];
    for r in records {
        for i in r.start as usize..r.start as usize + r.prims as usize {
            let [a, b, c] = [strip[i], strip[i + 1], strip[i + 2]].map(|k| positions[k as usize]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let area = 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            out[i] = u16::from_le_bytes(f16_le(area));
        }
    }
    out
}

/// Each vertex's object number from a TINY draw's `TDTA` records, `{u16 first index, u16 last index, u16
/// object}`, as the half float a PC TINY vertex holds in W.
fn tiny_objects(tdta: &[u8], d: &Draw, tris: &[[u16; 3]]) -> Result<Vec<u16>, String> {
    let mut object: Vec<Option<u16>> = vec![None; d.vertices];
    for i in 0..d.e1 as usize {
        let at = d.e3 + 6 * i;
        let (first, last, id) = (be16(tdta, at)? as usize, be16(tdta, at + 2)? as usize, be16(tdta, at + 4)?);
        for t in tris.get(first / 3..(last + 1) / 3).ok_or("a TDTA record past its triangles")? {
            for &v in t {
                object[v as usize] = Some(id);
            }
        }
    }
    object
        .into_iter()
        .map(|o| o.map(|id| u16::from_le_bytes(f16_le(id as f32))).ok_or_else(|| "a TINY vertex is in no object".to_string()))
        .collect()
}

fn convert_group(kids: &[UcfxNode], flags: &[u16]) -> Result<UcfxNode, String> {
    let info = row(kids, b"INFO").ok_or("a group has no INFO")?;
    let word = |k: usize| be32(info, 4 * k);
    let (position_bytes, skin_bytes, vs, vs2, morph_word) = match info.len() {
        64 => (word(2)? as usize, 0, word(4)?, word(5)?, word(1)?),
        32 => (word(2)? as usize, word(3)? as usize, word(6)?, word(7)?, word(1)?),
        24 => (word(1)? as usize, 0, word(4)?, word(5)?, 0),
        n => return Err(format!("a group INFO of {n} bytes")),
    };
    let draws = read_draws(row(kids, b"PRMT").ok_or("a group has no PRMT")?, word(0)? as usize)?;
    let first = draws.first().ok_or("a group of no draws")?;
    if draws.iter().any(|d| d.flags != first.flags || d.format != first.format) {
        return Err("a group's draws differ in format".into());
    }
    let tiny = info.len() == 24;
    let sspu = row(kids, b"SSPU").ok_or("a group has no SSPU")?;
    let ibuf = row(kids, b"IBUF").ok_or("a group has no IBUF")?;
    if sspu.len() != position_bytes + skin_bytes {
        return Err(format!("an SSPU of {} bytes for {position_bytes} + {skin_bytes}", sspu.len()));
    }
    let stream = read_stream(kids, &draws)?;
    let colour = stream.extra.is_some_and(|(t, _)| t == 4);
    let skin = first.flags & DRAW_SKIN != 0;
    let tangent = first.format == 2;
    let morph_rows: Vec<&[u8]> = kids.iter().filter(|n| &n.tag == b"BSHP").filter_map(|n| n.body.as_deref()).collect();
    if morph_rows.len() != (morph_word >> 16) as usize {
        return Err(format!("{} morph rows for INFO's {}", morph_rows.len(), morph_word >> 16));
    }
    let elements = pc_elements(stream.uv, colour, skin, tangent, !morph_rows.is_empty());
    let stride: usize = elements.iter().filter(|e| e.stream == 0).map(|e| e.ty.size()).sum();

    let triangles: Vec<Vec<[u16; 3]>> = draws
        .iter()
        .map(|d| {
            let piece = ibuf.get(d.ibuf_offset..d.ibuf_offset + d.ibuf_bytes).ok_or("a draw past its IBUF")?;
            read_triangles(piece, d.indices, d.vertices)
        })
        .collect::<Result<_, String>>()?;

    let mut used = BTreeSet::new();
    if skin {
        for d in &draws {
            if d.e2 == 0xFFFF {
                return Err("a skinned draw of bone base 0xFFFF".into());
            }
            for i in 0..d.vertices {
                let s = sspu.get(position_bytes + d.e3 + i * 8..position_bytes + d.e3 + i * 8 + 8).ok_or("a skin record past its SSPU")?;
                used.extend(s.chunks_exact(2).filter(|p| p[0] != 0).map(|p| d.e2 + p[1] as u16));
            }
        }
    }
    let ranges = palette(&used)?;

    let total: usize = draws.iter().map(|d| d.vertices).sum();
    if total > 0x1_0000 {
        return Err(format!("a group of {total} vertices"));
    }
    let mut data = Vec::with_capacity(total * stride);
    let mut positions = Vec::with_capacity(total);
    let mut offsets = Vec::with_capacity(draws.len());
    let tdta = row(kids, b"TDTA");
    for (d, tris) in draws.iter().zip(&triangles) {
        offsets.push(positions.len() as u16);
        let (w_bytes, tail) = (if d.flags & DRAW_POSITION_W != 0 { 2 } else { 0 }, [0, 3, 6, 7].get(d.format as usize).copied());
        let tail = tail.ok_or_else(|| format!("draw format {}", d.format))?;
        let vertex = 6 + w_bytes + tail;
        if vertex * d.vertices != d.sspu_bytes {
            return Err(format!("{} SSPU bytes for {} vertices of {vertex}", d.sspu_bytes, d.vertices));
        }
        let objects = if d.flags & DRAW_TINY != 0 { Some(tiny_objects(tdta.ok_or("a TINY draw has no TDTA")?, d, tris)?) } else { None };
        for i in 0..d.vertices {
            let v = sspu.get(d.sspu_offset + i * vertex..d.sspu_offset + (i + 1) * vertex).ok_or("a vertex past its SSPU")?;
            let strm = stream.data.get((d.base_vertex + i) * stream.stride..(d.base_vertex + i + 1) * stream.stride).unwrap_or(&[]);
            positions.push([half_float(be16(v, 0)?), half_float(be16(v, 2)?), half_float(be16(v, 4)?)]);
            for o in [0, 2, 4] {
                data.extend_from_slice(&half_of(v, o)?);
            }
            match &objects {
                Some(ids) => data.extend_from_slice(&ids[i].to_le_bytes()),
                None if w_bytes != 0 => data.extend_from_slice(&half_of(v, 6)?),
                None => data.extend_from_slice(&HALF_ONE.to_le_bytes()),
            }
            let after = 6 + w_bytes;
            if stream.uv {
                let (from, o) = if d.format == 3 { (v, after + 3) } else { (strm, 0) };
                data.extend_from_slice(&half_of(from, o)?);
                data.extend_from_slice(&half_of(from, o + 2)?);
            }
            if colour {
                let o = stream.extra.ok_or("a colour with no row")?.1;
                let c = strm.get(o..o + 4).ok_or("a colour past its STRM")?;
                let lift = |x: u8| if x == 0xFE { 0xFF } else { x };
                data.extend_from_slice(&[lift(c[2]), lift(c[1]), lift(c[0]), lift(c[3])]);
            }
            if skin {
                let s = sspu.get(position_bytes + d.e3 + i * 8..position_bytes + d.e3 + i * 8 + 8).ok_or("a skin record past its SSPU")?;
                let mut slots = [0u8; 4];
                let mut weights = [0u8; 4];
                for (k, p) in s.chunks_exact(2).enumerate() {
                    weights[k] = p[0];
                    if p[0] != 0 {
                        slots[k] = palette_slot(&ranges, d.e2 + p[1] as u16).ok_or("a bone outside its palette")?;
                    }
                }
                data.extend_from_slice(&slots);
                data.extend_from_slice(&weights);
            }
            if d.format == 0 {
                let o = stream.extra.ok_or("a format 0 draw has no normal row")?.1;
                put_vector(&mut data, hend3n(be32(strm, o)?), 1.0);
            } else {
                let (n, _) = unit_vector(v.get(after..after + 3).ok_or("a short vertex")?)?;
                put_vector(&mut data, n, 1.0);
            }
            if tangent {
                let (t, hand) = unit_vector(v.get(after + 3..after + 6).ok_or("a short vertex")?)?;
                put_vector(&mut data, t, hand);
            }
        }
    }

    let global: Vec<Vec<[u16; 3]>> = triangles.iter().zip(&offsets).map(|(t, &o)| t.iter().map(|x| x.map(|i| i + o)).collect()).collect();
    let (strip, list_a, list_b) = lists(&draws, &global, flags)?;
    let vstream = VertexStream { elements, data };

    if tiny {
        let key = TINY_SHADERS.iter().find(|t| t.0 == first.e2).ok_or_else(|| format!("a TINY e2 of {}", first.e2))?;
        let vertex_shader = key.1;
        let second_vertex_shader = match vs2 {
            0x3B33_AA5E => key.2,
            0xA9EA_4CF7 => key.3,
            k => return Err(format!("a TINY shadow shader 0x{k:08X}")),
        };
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let half = [hi[0] - center[0], hi[1] - center[1], hi[2] - center[2]];
        let radius = (half[0] * half[0] + half[1] * half[1] + half[2] * half[2]).sqrt();
        let g = MeshGroup { vertex_shader, second_vertex_shader, center, radius, bbox_min: lo, bbox_max: hi, stream: vstream, area: Vec::new(), strip, list_a, list_b };
        return encode_tiny_group(&g, "a PS3 TINY group");
    }
    let column = shader_column(first.flags, !morph_rows.is_empty())?;
    let vertex_shader = pc_shader(&VERTEX_SHADERS, vs, column)?;
    let second_vertex_shader = pc_shader(&SHADOW_SHADERS, vs2, column)?;
    if skin {
        let morph_stride = (morph_word & 0xFFFF) as usize;
        let channels = row(kids, b"BSHI").unwrap_or(&[]);
        let mut morphs = Vec::with_capacity(morph_rows.len());
        for (m, body) in morph_rows.iter().enumerate() {
            let (mut deltas, mut at) = (Vec::with_capacity(total), 0usize);
            for d in &draws {
                for i in 0..d.vertices {
                    let o = at + i * morph_stride;
                    deltas.push([be16(body, o)?, be16(body, o + 2)?, be16(body, o + 4)?, HALF_ONE]);
                }
                at += (d.vertices * morph_stride).next_multiple_of(16);
            }
            morphs.push(MorphTarget { channel: be16(channels, 2 * m)?, deltas });
        }
        let g = SkinGroup {
            vertex_shader,
            second_vertex_shader,
            unread_range_slots: vec![BoneRange { hier_base: 0, count: 0 }; SKIN_RANGE_SLOTS - ranges.len()],
            bone_ranges: ranges,
            stream: vstream,
            strip,
            morphs,
            list_a,
            list_b,
        };
        return encode_skin_group(&g, "a PS3 SKIN group");
    }
    let f = |k: usize| word(k).map(f32::from_bits);
    let mut records = list_a.clone();
    records.extend_from_slice(&list_b);
    let g = MeshGroup {
        vertex_shader,
        second_vertex_shader,
        center: [f(6)?, f(7)?, f(8)?],
        radius: f(9)?,
        bbox_min: [f(10)?, f(11)?, f(12)?],
        bbox_max: [f(13)?, f(14)?, f(15)?],
        area: areas(&strip, &records, &positions),
        stream: vstream,
        strip,
        list_a,
        list_b,
    };
    encode_mesh_group(&g, "a PS3 MESH group")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bits written most significant first.
    fn pack(fields: &[(u32, usize)]) -> Vec<u8> {
        let mut bits: Vec<u8> = Vec::new();
        for &(v, n) in fields {
            bits.extend((0..n).rev().map(|i| (v >> i & 1) as u8));
        }
        bits.chunks(8).map(|c| c.iter().enumerate().fold(0, |b, (i, x)| b | x << (7 - i))).collect()
    }

    #[test]
    fn indices_share_an_edge_with_the_last_triangle() {
        // Two triangles: three new corners, then code 0, keeping (a, c) with a new corner.
        let flags = pack(&[(0, 4)]);
        let codes = pack(&[(3, 2), (0, 2)]);
        let piece = [vec![0, 0, 0, 0, 0, 1, 0, 0], flags, codes].concat();
        assert_eq!(read_triangles(&piece, 6, 4).unwrap(), [[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn an_explicit_index_is_a_delta_from_the_one_eight_before() {
        // Base 1, so each value is 1 high; the ninth is 3 past the first, 1.
        let mut fields: Vec<(u32, usize)> = (1..=8).map(|v| (v + 1, 5)).collect();
        fields.push((4, 5));
        assert_eq!(explicit_indices(&pack(&fields), 0, 9, 5, 1).unwrap(), [1, 2, 3, 4, 5, 6, 7, 8, 4]);
    }

    #[test]
    fn indices_must_use_every_vertex() {
        let piece = [vec![0, 0, 0, 0, 0, 1, 0, 0], pack(&[(0, 3)]), pack(&[(3, 2)])].concat();
        assert!(read_triangles(&piece, 3, 4).is_err());
    }

    #[test]
    fn a_unit_vector_recomputes_its_axis_with_its_sign() {
        // p = q = 0, axis z, positive, hand set.
        let v = 512u32 << 14 | 512 << 4 | 2 << 2 | 1 << 1 | 1;
        let (n, hand) = unit_vector(&[(v >> 16) as u8, (v >> 8) as u8, v as u8]).unwrap();
        assert_eq!(n, [0.0, 0.0, 1.0]);
        assert_eq!(hand, 1.0);
        let down = v & !3;
        let (n, hand) = unit_vector(&[(down >> 16) as u8, (down >> 8) as u8, down as u8]).unwrap();
        assert_eq!(n, [0.0, 0.0, -1.0]);
        assert_eq!(hand, -1.0);
    }

    #[test]
    fn a_unit_vector_of_axis_three_is_refused() {
        assert!(unit_vector(&[0, 0, 3 << 2]).is_err());
    }

    #[test]
    fn a_palette_is_the_used_runs_with_the_nearest_merged_to_fit() {
        let used: BTreeSet<u16> = [1, 2, 3, 7].into_iter().collect();
        let r = palette(&used).unwrap();
        assert_eq!(r, [BoneRange { hier_base: 1, count: 3 }, BoneRange { hier_base: 7, count: 1 }]);
        assert_eq!(palette_slot(&r, 7), Some(3));
        assert_eq!(palette_slot(&r, 4), None);

        // Nine runs: the two closest, 40..=41 and 50, merge, taking the bones between.
        let used: BTreeSet<u16> = (0..9u16).map(|i| i * 10).chain([41]).collect();
        let r = palette(&used).unwrap();
        assert_eq!(r.len(), SKIN_RANGE_SLOTS);
        assert!(r.iter().any(|x| x.hier_base == 40 && x.count == 11));
    }

    #[test]
    fn a_palette_past_the_shader_is_refused() {
        let used: BTreeSet<u16> = (0..65).collect();
        assert!(palette(&used).is_err());
    }

    fn draw(material: u16) -> Draw {
        Draw { material, base_vertex: 0, vertices: 0, sspu_offset: 0, sspu_bytes: 0, format: 1, indices: 0, ibuf_offset: 0, ibuf_bytes: 0, e1: 0, e2: 0, e3: 0, flags: 0 }
    }

    #[test]
    fn list_a_leaves_out_a_shadow_only_material_and_list_b_holds_it() {
        let draws = [draw(0), draw(1)];
        let tris = [vec![[0, 1, 2]], vec![[3, 4, 5]]];
        let (strip, a, b) = lists(&draws, &tris, &[0, MATERIAL_SHADOW_ONLY]).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].material, 0);
        assert_eq!(b.len(), 1, "the materials are not alpha tested, so one shadow record");
        let (start, prims) = (b[0].start as usize, b[0].prims as usize);
        assert_eq!(crate::terrainmesh::destrip(&strip[start..start + prims + 2]).len(), 2, "both draws' triangles");
    }

    #[test]
    fn each_alpha_tested_material_keeps_its_own_shadow_record() {
        let draws = [draw(0), draw(1), draw(2)];
        let tris = [vec![[0, 1, 2]], vec![[3, 4, 5]], vec![[6, 7, 8]]];
        let (_, a, b) = lists(&draws, &tris, &[MATERIAL_ALPHA_TESTED, 0, MATERIAL_ALPHA_TESTED]).unwrap();
        assert_eq!(a.len(), 3);
        assert_eq!(b.iter().map(|r| r.material).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(b, a, "each record is one draw, so it shares that draw's range");
    }

    #[test]
    fn a_shader_outside_the_tables_is_refused() {
        assert_eq!(pc_shader(&VERTEX_SHADERS, 0xDC35_1FCB, None).unwrap(), 0xDC35_1FCB);
        assert_eq!(pc_shader(&VERTEX_SHADERS, 0xDC35_1FCB, Some(1)).unwrap(), 0x4884_5C5F);
        assert!(pc_shader(&VERTEX_SHADERS, 0x1234_5678, Some(1)).is_err());
        assert!(pc_shader(&VERTEX_SHADERS, 0x79F4_CCB3, Some(0)).is_err());
    }

    #[test]
    fn a_skinned_vertex_declares_its_blend_data_before_its_normal() {
        let e = pc_elements(true, true, true, true, false);
        let offsets: Vec<u16> = e.iter().map(|x| x.offset).collect();
        assert_eq!(offsets, [0, 8, 12, 16, 20, 24, 32]);
        assert_eq!(e[3].usage, DeclUsage::BlendIndices);
        assert_eq!(pc_elements(true, true, true, true, true).len(), 13, "six morph streams");
    }
}
