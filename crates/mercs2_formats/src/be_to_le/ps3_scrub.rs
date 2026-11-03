//! A PS3 `scrub` chunk in PC form.
//!
//! A PS3 scrub holds one tuft per mesh, in the forms of [`super::ps3_model`], and hands the SPU up to 0x80
//! instances a job. PC draws 16 instances a call from 16 copies of the tuft, so a mesh is written 16 times,
//! each copy's number in `TEXCOORD.z`. `docs/reverse_engineer/ps3_model_format.md` holds the layouts.
//!
//! `INST` is three arrays: `table` quaternions, each `{x, y, z, w}` half floats, then for each instance
//! `{tx, ty, tz, scale}` half floats, then for each instance a u16 index into the table (`FUN_0027c520`,
//! `FUN_0027d198`, `ps3-v100`). Instances reuse a table entry near their own rotation, so a rotation here
//! is PS3's and up to 10.02° from PC's. PC's is not in the data. The translations are PC's bits.
//!
//! `PTCH` keeps its bytes: PS3 ships PC's bounds with its own rotations.

use super::ps3_model::{be16, be32, be_tree, half_float, put_vector, read_triangles, row, unit_vector};
use crate::model_inject::f16_le;
use crate::scrub::ScrubPack;
use crate::ucfx::{write_ucfx_tree, UcfxNode, UCFX_CSUM_BYTES};

/// Copies of a tuft in a PC mesh.
const COPIES: usize = 16;
/// The half float 1.0.
const HALF_ONE: u16 = 0x3C00;
/// What a scale-0 instance holds for its quaternion.
const HALF_NAN: u16 = 0xFE00;

/// The 12 half floats of a PC instance, row major: rotation by scale, then translation.
fn instance(q: [u16; 4], t: [u16; 4]) -> Result<[u16; 12], String> {
    let [tx, ty, tz, scale] = t;
    if scale & 0x7FFF == 0 {
        if q != [HALF_NAN; 4] {
            return Err(format!("a scale-0 instance holds the quaternion {q:04X?}"));
        }
        return Ok([0, 0, 0, tx, 0, 0, 0, ty, 0, 0, 0, tz]);
    }
    if q.iter().chain(&[scale]).any(|h| h & 0x7C00 == 0x7C00) {
        return Err(format!("the instance {q:04X?} {t:04X?} is not finite"));
    }
    let [x, y, z, w] = q.map(half_float);
    let n = (x * x + y * y + z * z + w * w).sqrt();
    let [x, y, z, w] = [x / n, y / n, z / n, w / n];
    let s = half_float(scale);
    let h = |c: f32| u16::from_le_bytes(f16_le(c * s));
    Ok([
        h(1.0 - 2.0 * (y * y + z * z)),
        h(2.0 * (x * y - w * z)),
        h(2.0 * (x * z + w * y)),
        tx,
        h(2.0 * (x * y + w * z)),
        h(1.0 - 2.0 * (x * x + z * z)),
        h(2.0 * (y * z - w * x)),
        ty,
        h(2.0 * (x * z - w * y)),
        h(2.0 * (y * z + w * x)),
        h(1.0 - 2.0 * (x * x + y * y)),
        tz,
    ])
}

/// `INST` in PC form.
fn instances(inst: &[u8], count: usize, table: usize) -> Result<Vec<u8>, String> {
    if inst.len() != 8 * table + 10 * count {
        return Err(format!("an INST of {} bytes for {table} rotations and {count} instances", inst.len()));
    }
    let half4 = |at: usize| -> Result<[u16; 4], String> { Ok([be16(inst, at)?, be16(inst, at + 2)?, be16(inst, at + 4)?, be16(inst, at + 6)?]) };
    let mut out = Vec::with_capacity(24 * count);
    for i in 0..count {
        let index = be16(inst, 8 * table + 8 * count + 2 * i)? as usize;
        if index >= table {
            return Err(format!("instance {i} names rotation {index} of {table}"));
        }
        for h in instance(half4(8 * index)?, half4(8 * table + 8 * i)?)? {
            out.extend_from_slice(&h.to_le_bytes());
        }
    }
    Ok(out)
}

/// A triangle's three rotations, itself first.
fn rotations(t: [u16; 3]) -> [[u16; 3]; 3] {
    [t, [t[1], t[2], t[0]], [t[2], t[0], t[1]]]
}

/// One strip of `tris` in order. A run's first triangle is turned so its `(b, c)` is the edge it shares with
/// the next, and runs join by repeating the last index and the next run's first, a run starting at an even
/// place. A tuft's quads share `(a, c)`, which [`crate::terrainmesh::stripify`] cannot continue across.
fn strip_of(tris: &[[u16; 3]]) -> Vec<u16> {
    let mut strip: Vec<u16> = Vec::with_capacity(tris.len() * 2);
    let mut i = 0;
    while i < tris.len() {
        let shares = |r: &[u16; 3]| {
            tris.get(i + 1).is_some_and(|u| (0..3).any(|j| (u[j], u[(j + 1) % 3]) == (r[2], r[1])))
        };
        let first = rotations(tris[i]).into_iter().find(shares).unwrap_or(tris[i]);
        if let Some(&last) = strip.last() {
            strip.extend_from_slice(&[last, first[0]]);
            if strip.len() % 2 == 1 {
                strip.push(first[0]);
            }
        }
        strip.extend_from_slice(&first);
        i += 1;
        while let Some(&u) = tris.get(i) {
            let (e0, e1) = (strip[strip.len() - 2], strip[strip.len() - 1]);
            let mut others = u.iter().copied().filter(|v| *v != e0 && *v != e1);
            let (Some(next), None) = (others.next(), others.next()) else { break };
            let want = if strip.len() % 2 == 0 { [e0, e1, next] } else { [e0, next, e1] };
            if !rotations(u).contains(&want) {
                break;
            }
            strip.push(next);
            i += 1;
        }
    }
    strip
}

/// The strip of `COPIES` copies of a tuft of `vertices` vertices.
fn copied_strip(tuft: &[u16], vertices: usize) -> Result<Vec<u16>, String> {
    let mut strip: Vec<u16> = Vec::with_capacity(tuft.len() * COPIES + 3 * COPIES);
    for copy in 0..COPIES {
        let off = u16::try_from(copy * vertices).map_err(|_| format!("{vertices} vertices do not fit a strip"))?;
        let first = *tuft.first().ok_or("a tuft of no triangles")? + off;
        if let Some(&last) = strip.last() {
            strip.extend_from_slice(&[last, first]);
            if strip.len() % 2 == 1 {
                strip.push(first);
            }
        }
        strip.extend(tuft.iter().map(|i| i + off));
    }
    Ok(strip)
}

/// A PC scrub vertex declaration: `POSITION`, `TEXCOORD`, then `COLOR`, `NORMAL` and `TANGENT`.
fn declaration(colour: bool, tangent: bool) -> (Vec<u8>, usize) {
    let (mut out, mut at) = (Vec::new(), 0u16);
    let mut row = |ty: u8, size: u16, usage: u8| {
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&[ty, 0, usage, 0]);
        at += size;
    };
    row(16, 8, 0);
    row(16, 8, 5);
    if colour {
        row(4, 4, 10);
    }
    row(16, 8, 3);
    if tangent {
        row(16, 8, 6);
    }
    out.extend_from_slice(&[0xFF, 0, 0, 0, 0x11, 0, 0, 0]);
    (out, at as usize)
}

/// A big-endian `MTRL`: 26 u32, two u16, then u32 to its end.
fn material(b: &[u8]) -> Result<Vec<u8>, String> {
    if b.len() < 108 || (b.len() - 108) % 4 != 0 {
        return Err(format!("an MTRL of {} bytes", b.len()));
    }
    let mut out = Vec::with_capacity(b.len());
    out.extend(b[..104].chunks_exact(4).flat_map(|w| [w[3], w[2], w[1], w[0]]));
    out.extend(b[104..108].chunks_exact(2).flat_map(|w| [w[1], w[0]]));
    out.extend(b[108..].chunks_exact(4).flat_map(|w| [w[3], w[2], w[1], w[0]]));
    Ok(out)
}

fn words_le(b: &[u8], words: usize) -> Result<Vec<u8>, String> {
    b.get(..4 * words).map(|s| s.chunks_exact(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect()).ok_or_else(|| format!("{} bytes for {words} words", b.len()))
}

/// One `SCRB`: `INFO MTRL STRM SSPU IBUF PRMT` to PC's `INFO MTRL STRM IBUF`.
fn mesh(kids: &[UcfxNode]) -> Result<UcfxNode, String> {
    let info = row(kids, b"INFO").ok_or("an SCRB has no INFO")?;
    let prmt = row(kids, b"PRMT").ok_or("an SCRB has no PRMT")?;
    let sspu = row(kids, b"SSPU").ok_or("an SCRB has no SSPU")?;
    let ibuf = row(kids, b"IBUF").ok_or("an SCRB has no IBUF")?;
    let strm = kids.iter().find(|n| &n.tag == b"STRM").ok_or("an SCRB has no STRM")?;
    let data = row(&strm.children, b"data").ok_or("an SCRB STRM has no data")?;
    let decl = row(&strm.children, b"decl").ok_or("an SCRB STRM has no decl")?;
    let d = super::ps3_model::read_row_b(prmt, 0, 0, 0)?;
    let (colour, tangent) = (decl.len() == 8 && decl[4] == 4, d.format == 2);
    if !matches!(d.format, 1 | 2) || d.flags != 0 || d.sspu_offset != 0 || d.ibuf_offset != 0 {
        return Err(format!("an SCRB draw of format {}, flags {}", d.format, d.flags));
    }
    if decl.first() != Some(&3) || !(decl.len() == 4 || colour) {
        return Err(format!("an SCRB decl {decl:02X?}"));
    }
    let stride = if tangent { 12 } else { 9 };
    let source = if colour { 8 } else { 4 };
    if sspu.len() != d.vertices * stride || data.len() != d.vertices * source {
        return Err(format!("{} SSPU and {} STRM bytes for {} vertices", sspu.len(), data.len(), d.vertices));
    }
    let tuft = read_triangles(ibuf, d.indices, d.vertices)?;
    let (decl_bytes, pc_stride) = declaration(colour, tangent);
    let mut vertices = Vec::with_capacity(COPIES * d.vertices * pc_stride);
    for copy in 0..COPIES {
        let number = f16_le(copy as f32);
        for i in 0..d.vertices {
            let v = &sspu[i * stride..(i + 1) * stride];
            let s = &data[i * source..(i + 1) * source];
            for o in [0, 2, 4] {
                vertices.extend_from_slice(&be16(v, o)?.to_le_bytes());
            }
            vertices.extend_from_slice(&HALF_ONE.to_le_bytes());
            vertices.extend_from_slice(&be16(s, 0)?.to_le_bytes());
            vertices.extend_from_slice(&be16(s, 2)?.to_le_bytes());
            vertices.extend_from_slice(&number);
            vertices.extend_from_slice(&[0, 0]);
            if colour {
                let lift = |x: u8| if x == 0xFE { 0xFF } else { x };
                vertices.extend_from_slice(&[lift(s[6]), lift(s[5]), lift(s[4]), lift(s[7])]);
            }
            put_vector(&mut vertices, unit_vector(&v[6..9])?.0, 1.0);
            if tangent {
                let (t, hand) = unit_vector(&v[9..12])?;
                put_vector(&mut vertices, t, hand);
            }
        }
    }
    let strip = copied_strip(&strip_of(&tuft), d.vertices)?;
    let mut stream_info = Vec::with_capacity(12);
    for w in [decl_bytes.len() / 8, pc_stride, COPIES * d.vertices] {
        stream_info.extend_from_slice(&(w as u32).to_le_bytes());
    }
    Ok(UcfxNode::marker(
        *b"SCRB",
        vec![
            UcfxNode::leaf(*b"INFO", words_le(info, 5)?),
            UcfxNode::leaf(*b"MTRL", material(row(kids, b"MTRL").ok_or("an SCRB has no MTRL")?)?),
            UcfxNode::marker(
                *b"STRM",
                vec![UcfxNode::leaf(*b"info", stream_info), UcfxNode::leaf(*b"decl", decl_bytes), UcfxNode::leaf(*b"data", vertices)],
            ),
            UcfxNode::marker(
                *b"IBUF",
                vec![
                    UcfxNode::leaf(*b"info", (strip.len() as u32).to_le_bytes().to_vec()),
                    UcfxNode::leaf(*b"data", strip.iter().flat_map(|x| x.to_le_bytes()).collect()),
                ],
            ),
        ],
    ))
}

/// Convert a PS3 scrub, without its `CSUM` trailer. The result must read back as a scrub.
pub(super) fn convert(container: &[u8]) -> Result<Vec<u8>, String> {
    let roots = be_tree(container)?;
    let info = row(&roots, b"INFO").ok_or("a scrub has no INFO")?;
    let (count, table) = (be32(info, 12)? as usize, be32(info, 16)? as usize);
    let mut tree = vec![UcfxNode::leaf(*b"INFO", words_le(info, 4)?)];
    for n in roots.iter().filter(|n| &n.tag == b"SCRB") {
        tree.push(mesh(&n.children)?);
    }
    tree.push(UcfxNode::leaf(*b"INST", instances(row(&roots, b"INST").ok_or("a scrub has no INST")?, count, table)?));
    let ptch = row(&roots, b"PTCH").ok_or("a scrub has no PTCH")?;
    let mut patches = Vec::with_capacity(ptch.len());
    for p in ptch.chunks_exact(56) {
        patches.extend(p[..52].chunks_exact(4).flat_map(|w| [w[3], w[2], w[1], w[0]]));
        patches.extend(p[52..].chunks_exact(2).flat_map(|w| [w[1], w[0]]));
    }
    tree.push(UcfxNode::leaf(*b"PTCH", patches));
    let ptms = row(&roots, b"PTMS").ok_or("a scrub has no PTMS")?;
    let ranges = ptms.chunks_exact(8).flat_map(|r| [r[3], r[2], r[1], r[0], r[5], r[4], r[7], r[6]]).collect();
    tree.push(UcfxNode::leaf(*b"PTMS", ranges));
    let mut out = write_ucfx_tree(&tree);
    ScrubPack::decode(&out)?;
    out.truncate(out.len() - UCFX_CSUM_BYTES);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: u16 = 0x3C00;
    const TWO: u16 = 0x4000;

    #[test]
    fn an_instance_is_its_rotation_by_scale_and_its_translation() {
        let m = instance([0, 0, 0, ONE], [0x1111, 0x2222, 0x3333, TWO]).unwrap();
        assert_eq!(m, [TWO, 0, 0, 0x1111, 0, TWO, 0, 0x2222, 0, 0, TWO, 0x3333]);
    }

    #[test]
    fn a_quarter_turn_about_y_moves_x_to_minus_z() {
        // q = (0, sin 45, 0, cos 45), the half float 0.7071 being 0x3B50.
        let m = instance([0, 0x3B50, 0, 0x3B50], [0, 0, 0, ONE]).unwrap();
        assert_eq!(half_float(m[2]), 1.0, "row 0 of R, column 2");
        assert_eq!(half_float(m[8]), -1.0, "row 2 of R, column 0");
    }

    #[test]
    fn an_instance_of_scale_zero_has_no_rotation_and_keeps_its_translation() {
        let m = instance([HALF_NAN; 4], [0x1111, 0x2222, 0x3333, 0]).unwrap();
        assert_eq!(m, [0, 0, 0, 0x1111, 0, 0, 0, 0x2222, 0, 0, 0, 0x3333]);
        assert!(instance([0; 4], [0, 0, 0, 0]).is_err());
    }

    #[test]
    fn inst_is_a_table_then_transforms_then_indices() {
        let mut b = Vec::new();
        for h in [0, 0, 0, ONE, 0x1111, 0x2222, 0x3333, TWO, 0] {
            b.extend_from_slice(&u16::to_be_bytes(h));
        }
        let out = instances(&b, 1, 1).unwrap();
        assert_eq!(out.len(), 24);
        assert_eq!(u16::from_le_bytes([out[6], out[7]]), 0x1111);
        assert!(instances(&b, 1, 2).is_err());
    }

    /// The triangles a strip draws, each turned to start at its lowest index.
    fn drawn(s: &[u16]) -> Vec<[u16; 3]> {
        let mut out: Vec<[u16; 3]> = crate::terrainmesh::destrip(s)
            .into_iter()
            .map(|t| *rotations(t).iter().min().unwrap())
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn a_quad_sharing_its_ac_edge_is_one_run() {
        let tris = [[0, 1, 2], [0, 2, 3]];
        let s = strip_of(&tris);
        assert_eq!(s.len(), 4, "no stitch");
        assert_eq!(drawn(&s), drawn(&[1, 2, 0, 3]));
    }

    #[test]
    fn runs_join_with_no_triangle_of_their_own() {
        let tris = [[0, 1, 2], [0, 2, 3], [4, 5, 6]];
        let mut want: Vec<[u16; 3]> = tris.iter().map(|t| *rotations(*t).iter().min().unwrap()).collect();
        want.sort_unstable();
        assert_eq!(drawn(&strip_of(&tris)), want);
    }

    #[test]
    fn each_copy_of_a_tuft_draws_its_own_triangles() {
        let s = copied_strip(&strip_of(&[[0, 1, 2]]), 3).unwrap();
        let d = drawn(&s);
        assert_eq!(d.len(), COPIES);
        assert_eq!(d[15], [45, 46, 47]);
    }

    #[test]
    fn a_declaration_sets_its_stride_by_its_rows() {
        assert_eq!(declaration(false, false).1, 24);
        assert_eq!(declaration(true, false).1, 28);
        assert_eq!(declaration(false, true).1, 32);
        assert_eq!(declaration(true, false).0.len(), 5 * 8);
    }
}
