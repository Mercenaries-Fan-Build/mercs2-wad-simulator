//! Inject external mesh geometry into a real UCFX **model** donor container.
//!
//! This is the productionised (Rust) form of the proven "CJ recipe" — see
//! `output/_scratch/cj/injection_plan.md` and the `build_cj_model.py` prototype.
//! Rather than authoring a model UCFX from scratch (whose decl/material/shader
//! bindings the engine rejects — the root of the sarah-hang), we take a real
//! donor container the engine already accepts (obama_faithful4) and overwrite
//! only the geometry of a chosen drawing group, neutralising the others, then
//! repoint the MTRL diffuse hashes and recompute CSUM. Every byte we don't touch
//! stays valid.
//!
//! The descriptor walk, container-marker rule (`u0 == 0xFFFFFFFF`), leaf
//! resolution (`data_area_off + u0`), `crc32_mercs2` trailer and contiguous
//! repack all mirror [`crate::model_cubeize`] / [`crate::ucfx`]. This module
//! EXTENDS that family with a full geometry rebuild (STRM / IBUF / PRMT / decl)
//! instead of an in-place position rewrite.
//!
//! Vertex layout produced is the donor's 64-byte / stride-40 declaration WITH
//! TANGENT (the synthesised tangent is what fixed the CJ darkness — the donor
//! shader is tangent-space normal-mapped):
//! ```text
//!  +0  POSITION     f16x4  (x,y,z, w=1.0=0x3c00)
//!  +8  TEXCOORD0    f16x2  (u, v)        -- caller supplies final UVs (V-flip done upstream)
//!  +12 COLOR        bgra8  white 0xFFFFFFFF
//!  +16 BLENDINDICES u8x4   bone 0 (rigid root rig — proof-of-life)
//!  +20 BLENDWEIGHT  u8x4n  0xFF,0,0,0    (weight 1.0 to bone 0)
//!  +24 NORMAL       f16x4  (nx,ny,nz, w=1.0)  -- UNIT length
//!  +32 TANGENT      f16x4  (tx,ty,tz, sign)   -- UNIT length, synthesised
//! ```

use crate::crc32::crc32_mercs2;
use crate::ffcs::read_u32_le;

/// The exact 64-byte, stride-40 vertex declaration WITH TANGENT used for every
/// injected drawing group (verbatim from the donor groups 15/16/18). The TANGENT
/// (usage 6) is required by the donor's normal-map shader.
pub const DECL64: [u8; 64] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, // POSITION  s0 off0  t16 FLOAT16_4 u0
    0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00, // TEXCOORD0 s0 off8  t15 FLOAT16_2 u5
    0x00, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x0a, 0x00, // COLOR     s0 off12 t4  D3DCOLOR   u10
    0x00, 0x00, 0x10, 0x00, 0x05, 0x00, 0x02, 0x00, // BLENDIDX  s0 off16 t5  UBYTE4     u2
    0x00, 0x00, 0x14, 0x00, 0x08, 0x00, 0x01, 0x00, // BLENDWGT  s0 off20 t8  UBYTE4N    u1
    0x00, 0x00, 0x18, 0x00, 0x10, 0x00, 0x03, 0x00, // NORMAL    s0 off24 t16 FLOAT16_4 u3
    0x00, 0x00, 0x20, 0x00, 0x10, 0x00, 0x06, 0x00, // TANGENT   s0 off32 t16 FLOAT16_4 u6
    0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, // END
];

/// Parsed external mesh (already baked to donor frame: Y-up, feet at Y=0).
/// Positions are in DONOR space (post uniform scale); normals are unit and in
/// donor orientation (rotation applied, NOT the position scale). UVs are final
/// (any V-flip already applied). Triangles index `positions`/`normals`/`uvs`.
#[derive(Debug, Clone, Default)]
pub struct ExternalMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tris: Vec<[u32; 3]>,
    /// Per-vertex BLENDINDICES. Empty => rigid bone-0 fallback. For a rigid prop these are
    /// GLOBAL bone indices; for a faithful CHARACTER they must be PER-GROUP PALETTE-RELATIVE
    /// slots produced by [`crate::char_skin`] and injected via
    /// [`inject_character_into_donor_block`] (which also writes the matching `INFO(56)` range
    /// table). Do NOT hand-author global indices for a character group.
    pub joints: Vec<[u8; 4]>,
    /// Per-vertex BLENDWEIGHT (u8x4n, 0xFF=1.0). Empty => rigid [0xff,0,0,0] fallback.
    pub weights: Vec<[u8; 4]>,
}

/// MTRL diffuse-hash repoint: every 4-byte occurrence of `from` becomes `to`.
#[derive(Debug, Clone, Copy)]
pub struct MtrlRepoint {
    pub from: u32,
    pub to: u32,
}

/// Result of an injection, for logging / verification.
#[derive(Debug, Default, Clone)]
pub struct InjectStats {
    /// 0-based donor group index that now draws the injected mesh.
    pub target_group: usize,
    pub vertex_count: usize,
    pub strip_len: usize,
    pub triangle_count: usize,
    /// Other drawing groups whose PRMT draw-counts were zeroed.
    pub emptied_groups: Vec<usize>,
    /// (from, to, occurrences) for each MTRL repoint applied.
    pub mtrl_repoints: Vec<(u32, u32, usize)>,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// Average normal / tangent magnitude across injected verts (should be ~1.0).
    pub avg_normal_len: f32,
    pub avg_tangent_len: f32,
    /// SEGM row rewritten to `{node: -1, lod_mask: 0x7f}` to host the mesh unconditionally
    /// (always visible, every LOD tier, never superseded, model-space).
    pub unbound_seg: Option<usize>,
}

// --------------------------------------------------------------------- f16

/// Encode an f32 to a little-endian IEEE-754 half-float (round-to-nearest-even).
/// Clamps to the f16 finite range. Mirrors `model_cubeize::write_f16_le`.
pub fn f16_le(value: f32) -> [u8; 2] {
    let value = value.clamp(-65504.0, 65504.0);
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x007f_ffff;

    let h: u16 = if ((bits >> 23) & 0xff) == 0xff {
        sign | 0x7c00 | if mant != 0 { 0x0200 } else { 0 }
    } else if exp >= 0x1f {
        sign | 0x7c00
    } else if exp <= 0 {
        if exp < -10 {
            sign
        } else {
            let mant_with_implicit = mant | 0x0080_0000;
            let shift = (14 - exp) as u32;
            let mut m = mant_with_implicit >> shift;
            let round_bit = 1u32 << (shift - 1);
            if (mant_with_implicit & ((round_bit << 1) - 1)) > round_bit || (m & 1) == 1 {
                if (mant_with_implicit & (round_bit - 1)) != 0
                    || (mant_with_implicit & round_bit) != 0
                {
                    if (mant_with_implicit & ((round_bit << 1) - 1)) > round_bit || (m & 1) == 1 {
                        m += 1;
                    }
                }
            }
            sign | (m as u16)
        }
    } else {
        let mut half_mant = (mant >> 13) as u16;
        let round_rem = mant & 0x1fff;
        let mut hexp = exp as u16;
        if round_rem > 0x1000 || (round_rem == 0x1000 && (half_mant & 1) == 1) {
            half_mant += 1;
            if half_mant == 0x0400 {
                half_mant = 0;
                hexp += 1;
                if hexp >= 0x1f {
                    return (sign | 0x7c00).to_le_bytes();
                }
            }
        }
        sign | (hexp << 10) | half_mant
    };
    h.to_le_bytes()
}

/// Decode a little-endian IEEE-754 half-float (for verification / re-parse).
pub fn read_f16_le(d: &[u8], off: usize) -> f32 {
    let h = u16::from_le_bytes([d[off], d[off + 1]]);
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let frac = (h & 0x3ff) as u32;
    let val = if exp == 0 {
        (frac as f32 / 1024.0) * 2f32.powi(-14)
    } else if exp == 0x1f {
        if frac == 0 {
            f32::INFINITY
        } else {
            f32::NAN
        }
    } else {
        (1.0 + frac as f32 / 1024.0) * 2f32.powi(exp as i32 - 15)
    };
    if sign == 1 {
        -val
    } else {
        val
    }
}

// ------------------------------------------------------------------- strip

/// Degenerate-stitched triangle-strip builder (port of
/// `gltf_to_ucfx_model.to_strip`). Joins independent triangles with degenerate
/// bridges, padding so each real triangle starts at an even strip index (correct
/// winding). The engine consumes triangle STRIPS, not lists.
pub fn to_strip(tris: &[[u32; 3]]) -> Vec<u32> {
    let mut s: Vec<u32> = Vec::new();
    for &[a, b, c] in tris {
        if s.is_empty() {
            s.extend_from_slice(&[a, b, c]);
            continue;
        }
        let z = *s.last().unwrap();
        s.push(z);
        s.push(a);
        s.push(a);
        if s.len() % 2 == 0 {
            s.push(a);
        }
        s.push(b);
        s.push(c);
    }
    s
}

/// Adjacency-greedy triangle-strip builder. Chains shared-edge triangles into
/// runs (cost ~1 index/triangle inside a run) and bridges separate runs with
/// degenerate triples, preserving CCW winding by parity (a single degenerate
/// repeat flips parity when needed). Far cheaper than the per-triangle
/// `to_strip` for connected meshes, so a dense mesh fits a tight donor index
/// budget. Self-verifies identically via `strip_to_tris`. Generic over any mesh.
pub fn to_strip_connected(tris: &[[u32; 3]]) -> Vec<u32> {
    use std::collections::HashMap;
    let n = tris.len();
    if n == 0 {
        return Vec::new();
    }
    // edge (sorted pair) -> list of (tri_index, opposite_vertex)
    let mut edge_map: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (ti, t) in tris.iter().enumerate() {
        for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let k = if a < b { (a, b) } else { (b, a) };
            edge_map.entry(k).or_default().push(ti);
        }
    }
    let mut used = vec![false; n];
    // For a current strip ending with last three emitted verts, the next triangle
    // sharing the trailing edge (s[-2], s[-1]) continues the strip with one vertex.
    let edge_key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let find_next = |s: &[u32], used: &[bool], edge_map: &HashMap<(u32, u32), Vec<usize>>| {
        let l = s.len();
        if l < 2 {
            return None;
        }
        let (e0, e1) = (s[l - 2], s[l - 1]);
        let cands = edge_map.get(&edge_key(e0, e1))?;
        for &ti in cands {
            if used[ti] {
                continue;
            }
            // opposite vertex = the tri vertex not on edge (e0,e1)
            let t = tris[ti];
            let opp = t.iter().copied().find(|&v| v != e0 && v != e1);
            if let Some(opp) = opp {
                return Some((ti, opp));
            }
        }
        None
    };

    let mut s: Vec<u32> = Vec::new();
    let mut next_seed = 0usize;
    loop {
        // find an unused seed triangle
        while next_seed < n && used[next_seed] {
            next_seed += 1;
        }
        if next_seed >= n {
            break;
        }
        let seed = tris[next_seed];
        used[next_seed] = true;
        if s.is_empty() {
            s.extend_from_slice(&[seed[0], seed[1], seed[2]]);
        } else {
            // bridge: degenerate joins keeping even-parity start for the new tri
            let z = *s.last().unwrap();
            s.push(z);
            s.push(seed[0]);
            s.push(seed[0]);
            if s.len() % 2 == 0 {
                s.push(seed[0]);
            }
            s.push(seed[1]);
            s.push(seed[2]);
        }
        // extend the run along shared trailing edges
        loop {
            match find_next(&s, &used, &edge_map) {
                Some((ti, opp)) => {
                    used[ti] = true;
                    s.push(opp);
                }
                None => break,
            }
        }
    }
    s
}

/// Re-derive the triangle set a strip encodes (drops degenerate triples). Used
/// to self-verify `to_strip` reproduces the input triangles.
pub fn strip_to_tris(s: &[u32]) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    for i in 0..s.len().saturating_sub(2) {
        let (a, b, c) = (s[i], s[i + 1], s[i + 2]);
        if a == b || b == c || a == c {
            continue;
        }
        // Strip winding: odd-index triangles are reversed.
        if i % 2 == 0 {
            out.push([a, b, c]);
        } else {
            out.push([a, c, b]);
        }
    }
    out
}

// ---------------------------------------------------------------- tangents

/// Synthesise per-vertex tangents from UVs (Lengyel) + Gram-Schmidt against the
/// normal, returning unit `(tx,ty,tz,sign)`. Port of `build_cj_model.synth_tangents`.
fn synth_tangents(m: &ExternalMesh) -> Vec<[f32; 4]> {
    let n = m.positions.len();
    let mut tan = vec![[0.0f32; 3]; n];
    let mut bit = vec![[0.0f32; 3]; n];
    for &[i0, i1, i2] in &m.tris {
        let (i0, i1, i2) = (i0 as usize, i1 as usize, i2 as usize);
        let p0 = m.positions[i0];
        let p1 = m.positions[i1];
        let p2 = m.positions[i2];
        let w0 = m.uvs[i0];
        let w1 = m.uvs[i1];
        let w2 = m.uvs[i2];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let du1 = w1[0] - w0[0];
        let dv1 = w1[1] - w0[1];
        let du2 = w2[0] - w0[0];
        let dv2 = w2[1] - w0[1];
        let d = du1 * dv2 - du2 * dv1;
        if d.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / d;
        let sd = [
            (dv2 * e1[0] - dv1 * e2[0]) * r,
            (dv2 * e1[1] - dv1 * e2[1]) * r,
            (dv2 * e1[2] - dv1 * e2[2]) * r,
        ];
        let td = [
            (du1 * e2[0] - du2 * e1[0]) * r,
            (du1 * e2[1] - du2 * e1[1]) * r,
            (du1 * e2[2] - du2 * e1[2]) * r,
        ];
        for &i in &[i0, i1, i2] {
            for k in 0..3 {
                tan[i][k] += sd[k];
                bit[i][k] += td[k];
            }
        }
    }
    let mut out = vec![[0.0f32; 4]; n];
    for i in 0..n {
        let nrm = m.normals[i];
        let t = tan[i];
        let ndt = nrm[0] * t[0] + nrm[1] * t[1] + nrm[2] * t[2];
        let mut o = [
            t[0] - nrm[0] * ndt,
            t[1] - nrm[1] * ndt,
            t[2] - nrm[2] * ndt,
        ];
        let mut l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
        if l < 1e-8 {
            // degenerate UV -> pick an arbitrary perpendicular to the normal
            let mut a = if nrm[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let adt = nrm[0] * a[0] + nrm[1] * a[1] + nrm[2] * a[2];
            for k in 0..3 {
                a[k] -= nrm[k] * adt;
            }
            o = a;
            l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
        }
        for k in 0..3 {
            o[k] /= l;
        }
        // handedness sign = sign(dot(cross(n, o), bitangent))
        let cx = nrm[1] * o[2] - nrm[2] * o[1];
        let cy = nrm[2] * o[0] - nrm[0] * o[2];
        let cz = nrm[0] * o[1] - nrm[1] * o[0];
        let sign = if cx * bit[i][0] + cy * bit[i][1] + cz * bit[i][2] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        out[i] = [o[0], o[1], o[2], sign];
    }
    out
}

/// Encode the stride-40 vertex buffer (DECL64 layout) for the injected mesh.
fn encode_strm(m: &ExternalMesh, tans: &[[f32; 4]]) -> Vec<u8> {
    let mut vb = Vec::with_capacity(m.positions.len() * 40);
    for i in 0..m.positions.len() {
        let p = m.positions[i];
        let uv = m.uvs[i];
        let nrm = m.normals[i];
        let t = tans[i];
        vb.extend_from_slice(&f16_le(p[0]));
        vb.extend_from_slice(&f16_le(p[1]));
        vb.extend_from_slice(&f16_le(p[2]));
        vb.extend_from_slice(&[0x00, 0x3c]); // w = 1.0
        vb.extend_from_slice(&f16_le(uv[0]));
        vb.extend_from_slice(&f16_le(uv[1]));
        vb.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]); // COLOR white
        let bi = m.joints.get(i).copied().unwrap_or([0, 0, 0, 0]); // BLENDINDICES
        let bw = m.weights.get(i).copied().unwrap_or([0xff, 0, 0, 0]); // BLENDWEIGHT
        vb.extend_from_slice(&bi);
        vb.extend_from_slice(&bw);
        vb.extend_from_slice(&f16_le(nrm[0]));
        vb.extend_from_slice(&f16_le(nrm[1]));
        vb.extend_from_slice(&f16_le(nrm[2]));
        vb.extend_from_slice(&[0x00, 0x3c]); // normal w = 1.0
        vb.extend_from_slice(&f16_le(t[0]));
        vb.extend_from_slice(&f16_le(t[1]));
        vb.extend_from_slice(&f16_le(t[2]));
        vb.extend_from_slice(&f16_le(t[3]));
    }
    vb
}

// ----------------------------------------------------- container descriptor

#[derive(Debug, Clone)]
struct Row {
    tag: [u8; 4],
    u0: u32,
    size: u32,
    u2: u32,
    u3: u32,
}

/// A located donor drawing group (descriptor-row indices of each leaf).
#[derive(Debug, Clone)]
struct Group {
    strm_info: usize,
    strm_decl: usize,
    strm_data: usize,
    ibuf_info: usize,
    ibuf_data: usize,
    prmt: usize,
    // ★AREA = one f16 PER STRIP TRIANGLE (count == ibuf index count - 2): the triangle's
    // world-space surface area, 0.0 for the degenerate stitch triangles. Replace a group's geometry
    // and this array MUST be rebuilt to the new triangle count, or it still describes the donor's
    // mesh. (Proven: f16(AREA) vs recomputed area correlates 0.995, ratio median 1.00, and
    // AREA==0 matches degenerate triangles 100%.)
    area_info: Option<usize>,
    area_data: Option<usize>,
}

fn parse_rows(ucfx: &[u8]) -> (usize, usize, Vec<Row>) {
    let data_off = read_u32_le(ucfx, 4) as usize;
    let ndesc = read_u32_le(ucfx, 16) as usize;
    let mut rows = Vec::with_capacity(ndesc);
    for i in 0..ndesc {
        let ro = 20 + i * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&ucfx[ro..ro + 4]);
        rows.push(Row {
            tag,
            u0: read_u32_le(ucfx, ro + 4),
            size: read_u32_le(ucfx, ro + 8),
            u2: read_u32_le(ucfx, ro + 12),
            u3: read_u32_le(ucfx, ro + 16),
        });
    }
    (data_off, ndesc, rows)
}

/// Locate the donor's geometry sub-meshes: one group per `PRMG` container marker,
/// scanning leaves up to the next PRMG (mirrors `build_cj_model.find_meshes`).
fn find_groups(rows: &[Row]) -> Vec<Group> {
    let ndesc = rows.len();
    let prmg_rows: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| &r.tag == b"PRMG" && r.u0 == 0xFFFF_FFFF)
        .map(|(i, _)| i)
        .collect();
    let mut groups = Vec::new();
    for (gi, &pr) in prmg_rows.iter().enumerate() {
        let nxt = if gi + 1 < prmg_rows.len() {
            prmg_rows[gi + 1]
        } else {
            ndesc
        };
        let (mut strm_info, mut strm_decl, mut strm_data) = (None, None, None);
        let (mut ibuf_info, mut ibuf_data, mut prmt) = (None, None, None);
        let (mut area_info, mut area_data) = (None, None);
        let mut state = 0u8; // 1=STRM, 2=IBUF, 3=AREA
        for i in (pr + 1)..nxt {
            let r = &rows[i];
            let cm = r.u0 == 0xFFFF_FFFF;
            if &r.tag == b"STRM" && cm {
                state = 1;
            } else if &r.tag == b"IBUF" && cm {
                state = 2;
            } else if &r.tag == b"PRMT" && !cm && prmt.is_none() {
                prmt = Some(i);
            } else if &r.tag == b"info" && !cm {
                if state == 1 && strm_info.is_none() {
                    strm_info = Some(i);
                } else if state == 2 && ibuf_info.is_none() {
                    ibuf_info = Some(i);
                } else if state == 3 && area_info.is_none() {
                    area_info = Some(i);
                }
            } else if &r.tag == b"decl" && !cm && state == 1 && strm_decl.is_none() {
                strm_decl = Some(i);
            } else if &r.tag == b"AREA" && cm {
                state = 3;
            } else if &r.tag == b"data" && !cm {
                if state == 1 && strm_data.is_none() {
                    strm_data = Some(i);
                } else if state == 2 && ibuf_data.is_none() {
                    ibuf_data = Some(i);
                } else if state == 3 && area_data.is_none() {
                    area_data = Some(i);
                }
            }
        }
        if let (Some(si), Some(sd), Some(sda), Some(ii), Some(idd), Some(pt)) =
            (strm_info, strm_decl, strm_data, ibuf_info, ibuf_data, prmt)
        {
            groups.push(Group {
                area_info,
                area_data,
                strm_info: si,
                strm_decl: sd,
                strm_data: sda,
                ibuf_info: ii,
                ibuf_data: idd,
                prmt: pt,
            });
        }
    }
    groups
}

fn leaf<'a>(ucfx: &'a [u8], data_off: usize, r: &Row) -> &'a [u8] {
    let s = data_off + r.u0 as usize;
    &ucfx[s..s + r.size as usize]
}

/// Does a donor group currently draw? (IBUF index_count>0 and some PRMT rec count>0)
fn group_draws(ucfx: &[u8], data_off: usize, rows: &[Row], g: &Group) -> bool {
    let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0);
    if ic == 0 {
        return false;
    }
    let prmt = leaf(ucfx, data_off, &rows[g.prmt]);
    (0..prmt.len() / 16).any(|r| read_u32_le(prmt, r * 16 + 8) > 0)
}

/// Per-injected-group audit: injected counts vs the donor group's ORIGINAL
/// counts. The v1 crash (POST-LOAD AV WRITE at 0x006B82CD) was a per-group
/// skinning-output overrun: cesium's single group had MORE verts/indices than the
/// donor group it overwrote, so the engine's fixed-size output buffer (sized to
/// the donor original) overflowed. `inject_multi_into_donor_block` SPLITS the
/// mesh so every group is `<=` its donor original on BOTH vc and ic.
#[derive(Debug, Clone)]
pub struct GroupBudgetAudit {
    pub group: usize,
    pub injected_vc: u32,
    pub donor_vc: u32,
    pub injected_ic: u32,
    pub donor_ic: u32,
    pub triangles: usize,
}

/// Read a donor group's ORIGINAL (vertex_count, index_count) from its STRM info
/// and IBUF info leaves.
fn donor_group_caps(ucfx: &[u8], data_off: usize, rows: &[Row], g: &Group) -> (u32, u32) {
    let vc = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 8);
    let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0);
    (vc, ic)
}

/// A donor skinned vertex sampled from a stride-40 (DECL64) drawing group: its
/// model-space POSITION (decoded from f16x4 +0) and the verbatim BLENDINDICES
/// (+16, u8x4) + BLENDWEIGHT (+20, u8x4). Used as the source set for spatial
/// nearest-neighbour weight transfer onto a foreign mesh.
#[derive(Debug, Clone, Copy)]
pub struct DonorSkinVertex {
    pub pos: [f32; 3],
    pub joints: [u8; 4],
    pub weights: [u8; 4],
}

/// Collect the skinned vertices of the given donor drawing-group ordinals. Each
/// targeted group MUST be stride-40 (DECL64) and drawing. Reuses the same
/// descriptor/group walk the multi-group injector uses, so the STRM offsets are
/// resolved identically. Positions are decoded from f16x4; joints/weights are the
/// raw global bytes (no remap — donor uses direct global bone indexing).
pub fn collect_donor_skin_vertices(
    container_block: &[u8],
    group_ordinals: &[usize],
) -> Result<Vec<DonorSkinVertex>, String> {
    collect_donor_skin_vertices_filtered(container_block, group_ordinals, None)
}

/// Like `collect_donor_skin_vertices`, but the SAMPLE-set group ordinals are
/// decoupled from the geometry-HOST set and an optional bone-exclusion knob is
/// applied. DRIVER PARAM: `group_ordinals` is the weight-SAMPLE set (which donor
/// drawing groups supply NN source verts) — pass the FULL skinned body, not just
/// the geometry-host groups. `exclude_dominant_bone` (e.g. `Some(0)`) DROPS any
/// donor vert whose dominant bone (the BLENDINDICES slot with the max
/// BLENDWEIGHT) equals that bone, so a bone-0-dominant head shell can't pin
/// foreign head verts to bind. Generalizes to any foreign model on any donor.
pub fn collect_donor_skin_vertices_filtered(
    container_block: &[u8],
    group_ordinals: &[usize],
    exclude_dominant_bone: Option<u8>,
) -> Result<Vec<DonorSkinVertex>, String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let mut out = Vec::new();
    for &ord in group_ordinals {
        let g = groups
            .get(ord)
            .ok_or_else(|| format!("group ordinal {ord} out of range ({})", groups.len()))?;
        let stride = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 4) as usize;
        let vc = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 8) as usize;
        if stride != 40 {
            return Err(format!("group {ord} stride {stride} != 40 (not DECL64)"));
        }
        let body = leaf(ucfx, data_off, &rows[g.strm_data]);
        let n = vc.min(body.len() / 40);
        for v in 0..n {
            let o = v * 40;
            let joints = [body[o + 16], body[o + 17], body[o + 18], body[o + 19]];
            let weights = [body[o + 20], body[o + 21], body[o + 22], body[o + 23]];
            if let Some(excl) = exclude_dominant_bone {
                // dominant bone = BLENDINDICES slot with the max BLENDWEIGHT
                let dom = (0..4).max_by_key(|&i| weights[i]).unwrap();
                if joints[dom] == excl {
                    continue;
                }
            }
            out.push(DonorSkinVertex {
                pos: [
                    read_f16_le(body, o),
                    read_f16_le(body, o + 2),
                    read_f16_le(body, o + 4),
                ],
                joints,
                weights,
            });
        }
    }
    Ok(out)
}

/// Summary of one source drawing group, for the kitbash survey (STEP 1). All
/// fields are derived directly from the group's STRM/IBUF/MTRL leaves.
#[derive(Debug, Clone)]
pub struct GroupSurvey {
    /// Drawing-group ordinal (index into the donor's PRMG drawing groups).
    pub ordinal: usize,
    pub stride: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub draws: bool,
    /// Y bounds (height axis) of the group's positions, decoded from f16 POS.
    pub y_min: f32,
    pub y_max: f32,
    /// Full position bbox.
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// BLENDINDICES histogram: (bone_index, vertex_count) sorted desc, top entries.
    pub bone_hist: Vec<(u8, usize)>,
}

/// Survey EVERY drawing group of a source model block (STEP 1). For each stride-40
/// (DECL64) group: vertex/index counts, Y-bounds and a BLENDINDICES histogram (the
/// dominant bones). Non-DECL64 groups are reported with stride only. Reusable for
/// any human/character donor.
pub fn survey_groups(container_block: &[u8]) -> Result<Vec<GroupSurvey>, String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("payload is not UCFX".into());
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let mut out = Vec::new();
    for (ord, g) in groups.iter().enumerate() {
        let si = leaf(ucfx, data_off, &rows[g.strm_info]);
        let stride = read_u32_le(si, 4) as usize;
        let vc = read_u32_le(si, 8) as usize;
        let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0) as usize;
        let draws = group_draws(ucfx, data_off, &rows, g);
        let mut s = GroupSurvey {
            ordinal: ord,
            stride,
            vertex_count: vc,
            index_count: ic,
            draws,
            y_min: f32::INFINITY,
            y_max: f32::NEG_INFINITY,
            bbox_min: [f32::INFINITY; 3],
            bbox_max: [f32::NEG_INFINITY; 3],
            bone_hist: Vec::new(),
        };
        if stride == 40 {
            let body = leaf(ucfx, data_off, &rows[g.strm_data]);
            let n = vc.min(body.len() / 40);
            use std::collections::HashMap;
            let mut hist: HashMap<u8, usize> = HashMap::new();
            for v in 0..n {
                let o = v * 40;
                let p = [
                    read_f16_le(body, o),
                    read_f16_le(body, o + 2),
                    read_f16_le(body, o + 4),
                ];
                for k in 0..3 {
                    s.bbox_min[k] = s.bbox_min[k].min(p[k]);
                    s.bbox_max[k] = s.bbox_max[k].max(p[k]);
                }
                s.y_min = s.y_min.min(p[1]);
                s.y_max = s.y_max.max(p[1]);
                // dominant bone = BLENDINDICES slot with max BLENDWEIGHT
                let bi = [body[o + 16], body[o + 17], body[o + 18], body[o + 19]];
                let bw = [body[o + 20], body[o + 21], body[o + 22], body[o + 23]];
                let dom = (0..4).max_by_key(|&i| bw[i]).unwrap();
                *hist.entry(bi[dom]).or_insert(0) += 1;
            }
            let mut h: Vec<(u8, usize)> = hist.into_iter().collect();
            h.sort_by(|a, b| b.1.cmp(&a.1));
            s.bone_hist = h;
        }
        out.push(s);
    }
    Ok(out)
}

/// Extract a source drawing group's FULL geometry into an [`ExternalMesh`]
/// (positions/normals/uvs/tris/joints/weights) in SOURCE model space and SOURCE
/// bone indices (STEP 3). Decodes the stride-40 STRM (POS f16x4@0, UV f16x2@8,
/// BLENDINDICES u8x4@16, BLENDWEIGHT u8x4@20, NORMAL f16x4@24) and walks the IBUF
/// triangle STRIP back into a triangle list (winding-preserving). The group MUST
/// be stride-40 (DECL64). UVs are kept verbatim (already donor-frame V); positions
/// and normals untranslated/unrotated (the caller positions the part).
pub fn extract_group_mesh(
    container_block: &[u8],
    group_ordinal: usize,
) -> Result<ExternalMesh, String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("payload is not UCFX".into());
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let g = groups
        .get(group_ordinal)
        .ok_or_else(|| format!("group {group_ordinal} out of range ({})", groups.len()))?;
    let si = leaf(ucfx, data_off, &rows[g.strm_info]);
    let stride = read_u32_le(si, 4) as usize;
    if stride != 40 {
        return Err(format!("group {group_ordinal} stride {stride} != 40 (not DECL64)"));
    }
    let vc = read_u32_le(si, 8) as usize;
    let body = leaf(ucfx, data_off, &rows[g.strm_data]);
    let n = vc.min(body.len() / 40);
    let mut m = ExternalMesh {
        positions: Vec::with_capacity(n),
        normals: Vec::with_capacity(n),
        uvs: Vec::with_capacity(n),
        tris: Vec::new(),
        joints: Vec::with_capacity(n),
        weights: Vec::with_capacity(n),
    };
    for v in 0..n {
        let o = v * 40;
        m.positions.push([
            read_f16_le(body, o),
            read_f16_le(body, o + 2),
            read_f16_le(body, o + 4),
        ]);
        m.uvs.push([read_f16_le(body, o + 8), read_f16_le(body, o + 10)]);
        m.joints
            .push([body[o + 16], body[o + 17], body[o + 18], body[o + 19]]);
        m.weights
            .push([body[o + 20], body[o + 21], body[o + 22], body[o + 23]]);
        let nrm = [
            read_f16_le(body, o + 24),
            read_f16_le(body, o + 26),
            read_f16_le(body, o + 28),
        ];
        let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt().max(1e-8);
        m.normals.push([nrm[0] / l, nrm[1] / l, nrm[2] / l]);
    }
    // IBUF: triangle strip (u16). Walk it back to a triangle list, dropping
    // degenerates and flipping odd-index winding (mirrors strip_to_tris).
    let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0) as usize;
    let ibd = leaf(ucfx, data_off, &rows[g.ibuf_data]);
    let strip_n = ic.min(ibd.len() / 2);
    let idx = |i: usize| u16::from_le_bytes([ibd[i * 2], ibd[i * 2 + 1]]) as u32;
    for i in 0..strip_n.saturating_sub(2) {
        let (a, b, c) = (idx(i), idx(i + 1), idx(i + 2));
        if a == b || b == c || a == c {
            continue;
        }
        if a as usize >= n || b as usize >= n || c as usize >= n {
            continue;
        }
        if i % 2 == 0 {
            m.tris.push([a, b, c]);
        } else {
            m.tris.push([a, c, b]);
        }
    }
    Ok(m)
}

/// Cost (in strip indices) of a degenerate-stitched strip over `n` triangles.
#[allow(dead_code)]
fn strip_index_cost(n: usize) -> usize {
    if n == 0 {
        0
    } else {
        3 + 6 * (n - 1)
    }
}

/// Conservative index-cost estimate for an adjacency-stripped (`to_strip_connected`)
/// run of `n` triangles. A perfect single strip is `n + 2`, but real (non-grid)
/// meshes break into many short runs joined by 3-index degenerate bridges; the
/// observed worst case approaches ~3 indices/triangle. We budget 3/tri so the
/// greedy partition does not over-fill a low-index-budget donor group; the EXACT
/// emitted strip length is still asserted against the donor cap after the build.
fn strip_index_cost_connected(n: usize) -> usize {
    if n == 0 {
        0
    } else {
        3 * n + 2
    }
}

/// Inject `mesh` into the donor, SPLIT across multiple donor drawing groups so
/// every group is within the donor original's vertex AND index capacity (the v1
/// overrun fix). `target_group_ordinals` is the ordered list of donor drawing
/// groups to fill (e.g. `[16, 17, 15]`); triangles are packed greedily into each
/// One kitbash part: an external mesh routed STRICTLY to its own host drawing-group
/// ordinals (no balanced mixing with other parts), plus its MTRL diffuse repoint.
pub struct InjectPart<'a> {
    pub mesh: &'a ExternalMesh,
    /// Donor drawing-group ordinals that host THIS part only.
    pub hosts: &'a [usize],
    /// MTRL repoints to apply for this part (e.g. host diffuse -> part native skin).
    pub repoints: &'a [MtrlRepoint],
}

/// Inject MULTIPLE parts into DISJOINT host-group sets of one donor (the kitbash
/// path). Each part's triangles are balanced-split across ONLY that part's host
/// ordinals (a head part can never leak into a torso host). Host sets MUST be
/// pairwise disjoint. All parts' MTRL repoints are applied (global value-scan).
/// Returns the block, the union of per-host `GroupBudgetAudit`, and overall
/// `InjectStats`. Shares the chunking, strip, skinning and reassembly machinery
/// with the single-part injector via `inject_multi_into_donor_block` semantics.
///
/// `preserve_native_non_host` controls what happens to drawing groups that are NOT
/// a host for any injected part:
///   * `true` (KITBASH mode): leave every non-host group's native STRM/IBUF/PRMT/
///     SKIN/MTRL COMPLETELY UNTOUCHED so it keeps drawing the donor's own geometry,
///     weights and materials. Only the HOST groups are rewritten with the injected
///     part. This keeps the full donor body (legs, neck, feet, accessories) present
///     while swapping just the specified groups (the frankenstein body-preserve fix).
///   * `false` (WHOLE-MODEL-REPLACE mode): neutralise every non-host drawing group
///     (zero its PRMT draw-count) — the legacy cesium/mannequin behaviour where the
///     injected mesh legitimately REPLACES the entire model.
/// The ordinals of the donor's DRAWING groups (those that actually render), in PRMG descriptor
/// order — the valid `hosts` for [`inject_parts_into_donor_block`]. Same indexing as
/// [`crate::texture::group_material_indices`], so `gmi[ord]` is host `ord`'s material. Empty on a
/// non-UCFX block.
pub fn drawing_group_ordinals(container_block: &[u8]) -> Vec<usize> {
    if container_block.len() < 20 {
        return Vec::new();
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    if 20 + ucfx_len > container_block.len() {
        return Vec::new();
    }
    let ucfx = &container_block[20..20 + ucfx_len];
    if ucfx.len() < 4 || &ucfx[0..4] != b"UCFX" {
        return Vec::new();
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .collect()
}

/// Per drawing group: `(ordinal, vertex_capacity, triangle_capacity)`. Triangle capacity =
/// index-buffer length / 3 — the hard cap `inject_parts_into_donor_block` enforces per host. Same
/// ordinal space as [`drawing_group_ordinals`] / [`crate::texture::group_material_indices`].
pub fn drawing_group_caps(container_block: &[u8]) -> Vec<(usize, u32, u32)> {
    if container_block.len() < 20 {
        return Vec::new();
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    if 20 + ucfx_len > container_block.len() {
        return Vec::new();
    }
    let ucfx = &container_block[20..20 + ucfx_len];
    if ucfx.len() < 4 || &ucfx[0..4] != b"UCFX" {
        return Vec::new();
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .map(|gi| {
            let (vc, ic) = donor_group_caps(ucfx, data_off, &rows, &groups[gi]);
            (gi, vc, ic / 3)
        })
        .collect()
}

pub fn inject_parts_into_donor_block(
    container_block: &[u8],
    parts: &[InjectPart],
    new_name_hash: u32,
    preserve_native_non_host: bool,
    grow: bool,
    // SEGM record indices (INDX[group]) whose `state_mask` should get bit 0x01 OR-ed in, so the
    // group draws at the default LOD/view-state (else geometry injected into a non-LOD0 tier is
    // gated out at load). Empty = leave SEGM alone.
    promote_segm: &[usize],
) -> Result<(Vec<u8>, Vec<GroupBudgetAudit>, InjectStats), String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    // disjointness check
    let mut seen = std::collections::HashSet::new();
    for p in parts {
        for &h in p.hosts {
            if !seen.insert(h) {
                return Err(format!("host group {h} assigned to more than one part"));
            }
        }
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let model_type = read_u32_le(container_block, 8);
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let drawing: Vec<usize> = (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .collect();

    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> =
        std::collections::HashMap::new();
    let mut audits: Vec<GroupBudgetAudit> = Vec::new();
    let mut stats = InjectStats::default();
    let mut all_min = [f32::INFINITY; 3];
    let mut all_max = [f32::NEG_INFINITY; 3];
    let mut tot_nl = 0.0f64;
    let mut tot_tl = 0.0f64;
    let mut tot_v = 0usize;
    let mut all_targets: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for part in parts {
        let mesh = part.mesh;
        // resolve this part's hosts -> caps
        let mut targets: Vec<(usize, u32, u32)> = Vec::new();
        for &ord in part.hosts {
            if !drawing.contains(&ord) {
                return Err(format!("group {ord} is not a donor drawing group; drawing={drawing:?}"));
            }
            let (vc, ic) = donor_group_caps(ucfx, data_off, &rows, &groups[ord]);
            targets.push((ord, vc, ic));
            all_targets.insert(ord);
        }
        // balanced triangle partition across THIS part's hosts only
        struct Chunk {
            tris: Vec<[u32; 3]>,
            remap: std::collections::HashMap<u32, u32>,
        }
        let n_targets = targets.len().max(1);
        let balanced_cap = (mesh.tris.len() + n_targets - 1) / n_targets;
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut ti = 0usize;
        for (idx, &(_gi, donor_vc, donor_ic)) in targets.iter().enumerate() {
            let mut c = Chunk { tris: Vec::new(), remap: std::collections::HashMap::new() };
            let is_last = idx + 1 == targets.len();
            while ti < mesh.tris.len() {
                if !is_last && c.tris.len() >= balanced_cap {
                    break;
                }
                let t = mesh.tris[ti];
                let new_verts = t.iter().filter(|v| !c.remap.contains_key(v)).count();
                let next_vc = c.remap.len() + new_verts;
                let next_ic_lb = strip_index_cost_connected(c.tris.len() + 1);
                if !grow && (next_vc as u32 > donor_vc || next_ic_lb as u32 > donor_ic) {
                    break;
                }
                for &v in &t {
                    let n = c.remap.len() as u32;
                    c.remap.entry(v).or_insert(n);
                }
                c.tris.push(t);
                ti += 1;
            }
            chunks.push(c);
        }
        if ti < mesh.tris.len() {
            return Err(format!(
                "part insufficient capacity: placed {}/{} triangles across hosts {:?}",
                ti,
                mesh.tris.len(),
                part.hosts
            ));
        }

        // emit each chunk into its host group
        for (ci, (gi, donor_vc, donor_ic)) in targets.iter().enumerate() {
            let chunk = &chunks[ci];
            let g = &groups[*gi];
            if chunk.tris.is_empty() {
                let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
                let mut si = Vec::new();
                si.extend_from_slice(&f0.to_le_bytes());
                si.extend_from_slice(&40u32.to_le_bytes());
                si.extend_from_slice(&0u32.to_le_bytes());
                new_bodies.insert(g.strm_info, si);
                new_bodies.insert(g.strm_decl, DECL64.to_vec());
                new_bodies.insert(g.strm_data, Vec::new());
                new_bodies.insert(g.ibuf_info, 0u32.to_le_bytes().to_vec());
                new_bodies.insert(g.ibuf_data, Vec::new());
                let nrec = leaf(ucfx, data_off, &rows[g.prmt]).len() / 16;
                new_bodies.insert(g.prmt, vec![0u8; nrec * 16]);
                stats.emptied_groups.push(*gi);
                audits.push(GroupBudgetAudit { group: *gi, injected_vc: 0, donor_vc: *donor_vc, injected_ic: 0, donor_ic: *donor_ic, triangles: 0 });
                continue;
            }
            let mut order: Vec<(u32, u32)> = chunk.remap.iter().map(|(&g, &l)| (l, g)).collect();
            order.sort_unstable();
            let local_n = order.len();
            let has_skin = !mesh.joints.is_empty() && !mesh.weights.is_empty();
            let mut lm = ExternalMesh {
                positions: vec![[0.0; 3]; local_n],
                normals: vec![[0.0; 3]; local_n],
                uvs: vec![[0.0; 2]; local_n],
                tris: Vec::with_capacity(chunk.tris.len()),
                joints: if has_skin { vec![[0u8; 4]; local_n] } else { Vec::new() },
                weights: if has_skin { vec![[0u8; 4]; local_n] } else { Vec::new() },
            };
            for (l, gvid) in &order {
                lm.positions[*l as usize] = mesh.positions[*gvid as usize];
                lm.normals[*l as usize] = mesh.normals[*gvid as usize];
                lm.uvs[*l as usize] = mesh.uvs[*gvid as usize];
                if has_skin {
                    lm.joints[*l as usize] = mesh.joints[*gvid as usize];
                    lm.weights[*l as usize] = mesh.weights[*gvid as usize];
                }
            }
            for t in &chunk.tris {
                lm.tris.push([chunk.remap[&t[0]], chunk.remap[&t[1]], chunk.remap[&t[2]]]);
            }
            let strip = to_strip_connected(&lm.tris);
            {
                use std::collections::HashSet;
                let norm = |t: [u32; 3]| { let mut v = t; v.sort_unstable(); v };
                let got: HashSet<[u32; 3]> = strip_to_tris(&strip).into_iter().map(norm).collect();
                let want: HashSet<[u32; 3]> = lm.tris.iter().map(|&t| norm(t)).collect();
                if got != want {
                    return Err(format!("host {gi}: strip self-check failed"));
                }
            }
            let tans = synth_tangents(&lm);
            let vb = encode_strm(&lm, &tans);
            let vc = local_n as u32;
            let ic = strip.len() as u32;
            if !grow && (vc > *donor_vc || ic > *donor_ic) {
                return Err(format!("host {gi} budget violated: vc {vc}>{donor_vc} or ic {ic}>{donor_ic}"));
            }
            let mut ib = Vec::with_capacity(strip.len() * 2);
            for &x in &strip {
                ib.extend_from_slice(&(x as u16).to_le_bytes());
            }
            let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
            let mut si = Vec::new();
            si.extend_from_slice(&f0.to_le_bytes());
            si.extend_from_slice(&40u32.to_le_bytes());
            si.extend_from_slice(&vc.to_le_bytes());
            new_bodies.insert(g.strm_info, si);
            new_bodies.insert(g.strm_decl, DECL64.to_vec());
            new_bodies.insert(g.strm_data, vb);
            new_bodies.insert(g.ibuf_info, ic.to_le_bytes().to_vec());
            new_bodies.insert(g.ibuf_data, ib);
            let prmt_old = leaf(ucfx, data_off, &rows[g.prmt]);
            let nrec = (prmt_old.len() / 16).max(1);
            // PRESERVE the donor group's PRMT field0 (material/group tag) — hardcoding it (was `6`)
            // mis-tags the grown group so the reader skips it as an accessory (the 138-vert bug).
            let field0 = if prmt_old.len() >= 4 { read_u32_le(prmt_old, 0) } else { 6 };
            let mut rec = Vec::with_capacity(16);
            rec.extend_from_slice(&field0.to_le_bytes());
            rec.extend_from_slice(&0u32.to_le_bytes());
            rec.extend_from_slice(&(ic - 2).to_le_bytes());
            rec.extend_from_slice(&((vc - 1) as u16).to_le_bytes());
            rec.extend_from_slice(&(vc as u16).to_le_bytes());
            let mut prmt_body = Vec::with_capacity(nrec * 16);
            for _ in 0..nrec {
                prmt_body.extend_from_slice(&rec);
            }
            new_bodies.insert(g.prmt, prmt_body);
            // This group's PRMG INFO cull bounds -> fit the (possibly grown) injected geometry.
            // Without this the donor's tiny original bounds cull our higher-poly mesh at load
            // (the grow-mode 138-vert bug). Mirrors inject_static_into_donor_block.
            {
                let mut gbmin = [f32::INFINITY; 3];
                let mut gbmax = [f32::NEG_INFINITY; 3];
                for p in &lm.positions {
                    for k in 0..3 {
                        gbmin[k] = gbmin[k].min(p[k]);
                        gbmax[k] = gbmax[k].max(p[k]);
                    }
                }
                let gcen = [(gbmin[0] + gbmax[0]) * 0.5, (gbmin[1] + gbmax[1]) * 0.5, (gbmin[2] + gbmax[2]) * 0.5];
                let grad = {
                    let (dx, dy, dz) = ((gbmax[0] - gbmin[0]) * 0.5, (gbmax[1] - gbmin[1]) * 0.5, (gbmax[2] - gbmin[2]) * 0.5);
                    (dx * dx + dy * dy + dz * dz).sqrt()
                };
                if let Some(pir) =
                    (0..g.strm_info).rev().find(|&i| &rows[i].tag == b"INFO" && rows[i].u0 != 0xFFFF_FFFF)
                {
                    let mut pi = leaf(ucfx, data_off, &rows[pir]).to_vec();
                    if pi.len() >= 52 {
                        for k in 0..3 {
                            pi[20 + k * 4..24 + k * 4].copy_from_slice(&gcen[k].to_le_bytes());
                            pi[36 + k * 4..40 + k * 4].copy_from_slice(&gbmin[k].to_le_bytes());
                            pi[48 + k * 4..52 + k * 4].copy_from_slice(&gbmax[k].to_le_bytes());
                        }
                        pi[32..36].copy_from_slice(&grad.to_le_bytes());
                        new_bodies.insert(pir, pi);
                    }
                }
            }
            audits.push(GroupBudgetAudit { group: *gi, injected_vc: vc, donor_vc: *donor_vc, injected_ic: ic, donor_ic: *donor_ic, triangles: chunk.tris.len() });
            for p in &lm.positions {
                for k in 0..3 {
                    all_min[k] = all_min[k].min(p[k]);
                    all_max[k] = all_max[k].max(p[k]);
                }
            }
            for nrm in &lm.normals {
                tot_nl += ((nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]) as f64).sqrt();
            }
            for t in &tans {
                tot_tl += ((t[0] * t[0] + t[1] * t[1] + t[2] * t[2]) as f64).sqrt();
            }
            tot_v += local_n;
            stats.triangle_count += chunk.tris.len();
        }
    }

    // Non-host drawing groups. In KITBASH mode (preserve_native_non_host) we leave
    // them entirely untouched so the donor's native body keeps drawing; only the
    // host groups (above) were rewritten. In whole-model-replace mode we neutralise
    // every non-host group (zero its PRMT draw-count) — the legacy behaviour.
    if !preserve_native_non_host {
        for &gi in &drawing {
            if all_targets.contains(&gi) {
                continue;
            }
            let pg = &groups[gi];
            let mut p = leaf(ucfx, data_off, &rows[pg.prmt]).to_vec();
            for r in 0..p.len() / 16 {
                p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
            }
            new_bodies.insert(pg.prmt, p);
            stats.emptied_groups.push(gi);
        }
    }

    // MTRL repoint (all parts, global value-scan)
    let mtrl_row = rows.iter().position(|r| &r.tag == b"MTRL").ok_or("no MTRL chunk")?;
    let mut mtrl = leaf(ucfx, data_off, &rows[mtrl_row]).to_vec();
    for part in parts {
        for rp in part.repoints {
            let mut count = 0usize;
            let mut off = 0usize;
            while off + 4 <= mtrl.len() {
                if read_u32_le(&mtrl, off) == rp.from {
                    mtrl[off..off + 4].copy_from_slice(&rp.to.to_le_bytes());
                    count += 1;
                    off += 4;
                } else {
                    off += 1;
                }
            }
            stats.mtrl_repoints.push((rp.from, rp.to, count));
        }
    }
    new_bodies.insert(mtrl_row, mtrl);

    stats.bbox_min = all_min;
    stats.bbox_max = all_max;
    // Top INFO bounding box. In whole-model-replace mode the injected verts ARE the
    // whole model, so set the bbox to them. In KITBASH mode the native body (legs,
    // feet, accessories) extends well beyond the injected head/torso, so we must NOT
    // shrink the bbox to the injected parts — keep the donor's original top INFO
    // (already bounds the full mattias body) unioned with the injected extent.
    let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
    if top.len() >= 28 {
        if preserve_native_non_host {
            for k in 0..3 {
                let dmin = f32::from_le_bytes([top[4 + k * 4], top[5 + k * 4], top[6 + k * 4], top[7 + k * 4]]);
                let dmax = f32::from_le_bytes([top[16 + k * 4], top[17 + k * 4], top[18 + k * 4], top[19 + k * 4]]);
                let nmin = dmin.min(all_min[k]);
                let nmax = dmax.max(all_max[k]);
                top[4 + k * 4..8 + k * 4].copy_from_slice(&nmin.to_le_bytes());
                top[16 + k * 4..20 + k * 4].copy_from_slice(&nmax.to_le_bytes());
            }
        } else {
            for k in 0..3 {
                top[4 + k * 4..8 + k * 4].copy_from_slice(&all_min[k].to_le_bytes());
                top[16 + k * 4..20 + k * 4].copy_from_slice(&all_max[k].to_le_bytes());
            }
        }
    }
    new_bodies.insert(0, top);
    stats.vertex_count = tot_v;
    stats.avg_normal_len = (tot_nl / tot_v.max(1) as f64) as f32;
    stats.avg_tangent_len = (tot_tl / tot_v.max(1) as f64) as f32;

    // SEGM LOD-promotion: OR bit 0x01 into the state_mask of each named SEGM record so the injected
    // host groups draw at the default view-state. SEGM records are 4 bytes {u16 bone, u8 seg_id, u8
    // state_mask}; the mask is byte +3. Draw gate = ANY-bit overlap of state_mask with view_state.
    if !promote_segm.is_empty() {
        if let Some(segm_row) = rows.iter().position(|r| &r.tag == b"SEGM" && r.u0 != 0xFFFF_FFFF) {
            let mut segm = new_bodies
                .get(&segm_row)
                .cloned()
                .unwrap_or_else(|| leaf(ucfx, data_off, &rows[segm_row]).to_vec());
            for &si in promote_segm {
                let o = si * 4 + 3;
                if o < segm.len() {
                    segm[o] |= 0x01;
                }
            }
            new_bodies.insert(segm_row, segm);
        }
    }

    let block = reassemble(ucfx, &mut rows, ndesc, data_off, &new_bodies, model_type, new_name_hash);
    assert_no_empty_drawing_group(&block)
        .map_err(|e| format!("post-reassemble drawing-group gate FAILED: {e}"))?;
    Ok((block, audits, stats))
}

/// in turn. Every triangle MUST be placed — if the supplied groups lack capacity
/// an error is returned (decimate or add a group). Each placed group gets a local
/// 0-based vertex remap, its own STRM/IBUF/PRMT, MTRL repoint applied globally,
/// and every other drawing group is neutralised. Returns the block, per-group
/// `GroupBudgetAudit`, and overall `InjectStats`.
pub fn inject_multi_into_donor_block(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinals: &[usize],
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
) -> Result<(Vec<u8>, Vec<GroupBudgetAudit>, InjectStats), String> {
    inject_multi_into_donor_block_ex(
        container_block,
        mesh,
        target_group_ordinals,
        repoints,
        new_name_hash,
        false,
        false,
    )
}

/// Faithful CHARACTER variant of [`inject_multi_into_donor_block`]: writes each host group's own
/// palette-relative BLENDINDICES + `INFO(56)` range table, the way shipped characters store
/// skinning. Use this for a dense character that cannot fit one draw group — a single whole-model
/// palette would blow the 46-slot cap, whereas each group's own palette covers only its bones.
pub fn inject_character_multi_into_donor_block(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinals: &[usize],
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
    grow: bool,
) -> Result<(Vec<u8>, Vec<GroupBudgetAudit>, InjectStats), String> {
    inject_multi_into_donor_block_ex(
        container_block,
        mesh,
        target_group_ordinals,
        repoints,
        new_name_hash,
        true,
        grow,
    )
}

fn inject_multi_into_donor_block_ex(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinals: &[usize],
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
    skin_palette_per_group: bool,
    // Let a host group's STRM/IBUF exceed the DONOR's original vertex/index counts, bounded only
    // by the u16 ceiling. Needed when the imported character is denser than the donor it conforms
    // to; the block grows, so the packager MUST recompute page_count/packed_field afterwards.
    grow: bool,
) -> Result<(Vec<u8>, Vec<GroupBudgetAudit>, InjectStats), String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let model_type = read_u32_le(container_block, 8);
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let drawing: Vec<usize> = (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .collect();

    // resolve ordinals -> absolute group indices, with donor caps
    let mut targets: Vec<(usize, u32, u32)> = Vec::new(); // (gi, donor_vc, donor_ic)
    for &ord in target_group_ordinals {
        if !drawing.contains(&ord) {
            return Err(format!("group {ord} is not a donor drawing group; drawing={drawing:?}"));
        }
        let (vc, ic) = donor_group_caps(ucfx, data_off, &rows, &groups[ord]);
        let (vc, ic) = if grow { (65534u32, 65534u32) } else { (vc, ic) };
        targets.push((ord, vc, ic));
    }

    // ---- BALANCED triangle partition: distribute across ALL target groups ----
    // chunk = list of triangles + a local-vertex remap (global vid -> local idx).
    // Each chunk is capped at ~ceil(total_tris / n_targets) triangles so geometry
    // spreads over every host (a small mesh no longer all lands in group 0, which
    // would leave the other targets empty-but-registered -> null-vbuf draw AV). The
    // per-chunk donor vc/ic caps remain HARD upper bounds.
    struct Chunk {
        tris: Vec<[u32; 3]>,
        remap: std::collections::HashMap<u32, u32>,
    }
    let n_targets = targets.len().max(1);
    let balanced_cap = (mesh.tris.len() + n_targets - 1) / n_targets;
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut ti = 0usize;
    for (idx, &(_gi, donor_vc, donor_ic)) in targets.iter().enumerate() {
        let mut c = Chunk {
            tris: Vec::new(),
            remap: std::collections::HashMap::new(),
        };
        // The last target absorbs any remainder so every triangle is placed.
        let is_last = idx + 1 == targets.len();
        while ti < mesh.tris.len() {
            // Balanced soft cap: stop filling this chunk once it holds its even
            // share (unless this is the last chunk, which must take the rest).
            if !is_last && c.tris.len() >= balanced_cap {
                break;
            }
            let t = mesh.tris[ti];
            let new_verts = t.iter().filter(|v| !c.remap.contains_key(v)).count();
            let next_vc = c.remap.len() + new_verts;
            // Gate on the VERTEX cap (hard) and a connected-strip index LOWER bound
            // (~1 index/triangle; worst case is a few bridge triples). The exact
            // connected-strip cost is computed after the chunk is built and the
            // donor-ic budget is asserted then — see the post-build guard.
            let next_ic_lb = strip_index_cost_connected(c.tris.len() + 1);
            if next_vc as u32 > donor_vc || next_ic_lb as u32 > donor_ic {
                break;
            }
            for &v in &t {
                let n = c.remap.len() as u32;
                c.remap.entry(v).or_insert(n);
            }
            c.tris.push(t);
            ti += 1;
        }
        chunks.push(c);
    }
    if ti < mesh.tris.len() {
        return Err(format!(
            "insufficient group capacity: placed {}/{} triangles across {} groups; add a group or decimate",
            ti,
            mesh.tris.len(),
            targets.len()
        ));
    }

    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> =
        std::collections::HashMap::new();
    let mut audits: Vec<GroupBudgetAudit> = Vec::new();
    let mut stats = InjectStats::default();
    let mut all_min = [f32::INFINITY; 3];
    let mut all_max = [f32::NEG_INFINITY; 3];
    let mut tot_nl = 0.0f64;
    let mut tot_tl = 0.0f64;
    let mut tot_v = 0usize;

    // ---- emit each chunk into its donor group ----
    for (ci, (gi, donor_vc, donor_ic)) in targets.iter().enumerate() {
        let chunk = &chunks[ci];
        let g = &groups[*gi];

        // EMPTY-TARGET GUARD (latent-bug fix): a target group with 0 triangles
        // must NOT be left as a registered drawing group pointing at a null/zero
        // vbuf — that is the v1 wardrobe-select AV (null vbuf vcall, ESI=0). Emit
        // a valid zeroed STRM+IBUF and ZERO the PRMT draw-count, exactly like a
        // non-target group. NEVER emit the 0xFFFFFFFE (0 tris - 2) draw-count.
        if chunk.tris.is_empty() {
            let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
            let mut si_body = Vec::with_capacity(12);
            si_body.extend_from_slice(&f0.to_le_bytes());
            si_body.extend_from_slice(&40u32.to_le_bytes());
            si_body.extend_from_slice(&0u32.to_le_bytes());
            new_bodies.insert(g.strm_info, si_body);
            new_bodies.insert(g.strm_decl, DECL64.to_vec());
            new_bodies.insert(g.strm_data, Vec::new());
            new_bodies.insert(g.ibuf_info, 0u32.to_le_bytes().to_vec());
            new_bodies.insert(g.ibuf_data, Vec::new());
            let nrec = leaf(ucfx, data_off, &rows[g.prmt]).len() / 16;
            let mut prmt_body = Vec::with_capacity(nrec * 16);
            for _ in 0..nrec {
                prmt_body.extend_from_slice(&[0u8; 16]); // draw-count field stays 0
            }
            new_bodies.insert(g.prmt, prmt_body);
            stats.emptied_groups.push(*gi);
            audits.push(GroupBudgetAudit {
                group: *gi,
                injected_vc: 0,
                donor_vc: *donor_vc,
                injected_ic: 0,
                donor_ic: *donor_ic,
                triangles: 0,
            });
            continue;
        }

        // build local mesh (ordered by remap index)
        let mut order: Vec<(u32, u32)> = chunk.remap.iter().map(|(&g, &l)| (l, g)).collect();
        order.sort_unstable();
        let local_n = order.len();
        let has_skin = !mesh.joints.is_empty() && !mesh.weights.is_empty();
        let mut lm = ExternalMesh {
            positions: vec![[0.0; 3]; local_n],
            normals: vec![[0.0; 3]; local_n],
            uvs: vec![[0.0; 2]; local_n],
            tris: Vec::with_capacity(chunk.tris.len()),
            joints: if has_skin { vec![[0u8; 4]; local_n] } else { Vec::new() },
            weights: if has_skin { vec![[0u8; 4]; local_n] } else { Vec::new() },
        };
        for (l, gvid) in &order {
            lm.positions[*l as usize] = mesh.positions[*gvid as usize];
            lm.normals[*l as usize] = mesh.normals[*gvid as usize];
            lm.uvs[*l as usize] = mesh.uvs[*gvid as usize];
            if has_skin {
                lm.joints[*l as usize] = mesh.joints[*gvid as usize];
                lm.weights[*l as usize] = mesh.weights[*gvid as usize];
            }
        }
        for t in &chunk.tris {
            lm.tris.push([chunk.remap[&t[0]], chunk.remap[&t[1]], chunk.remap[&t[2]]]);
        }

        // PER-GROUP palette. A palette belongs to a DRAW GROUP, not to a model: each group's
        // INFO(56) leaf carries its own `{u16 hier_base, u16 count}` runs and its BLENDINDICES
        // are relative to those. `mesh.joints` arrives as GLOBAL donor HIER indices, so rewrite
        // them to THIS group's slots. Splitting is what keeps a dense character legal — one
        // whole-model palette overflows the 46-slot cap, three per-group palettes do not.
        if skin_palette_per_group && has_skin {
            let mut seen = std::collections::HashSet::new();
            let mut used: Vec<u32> = Vec::new();
            for (j, w) in lm.joints.iter().zip(lm.weights.iter()) {
                for k in 0..4 {
                    if w[k] > 0 && seen.insert(j[k] as u32) {
                        used.push(j[k] as u32);
                    }
                }
            }
            used.sort_unstable();
            let (ranges32, slot_of, slots) = crate::char_skin::build::build_palette_ranges(&used);
            if slots > crate::char_skin::build::PALETTE_CAP {
                return Err(format!(
                    "group {gi}: palette is {slots} slots over {} bones, above the {} the game \
                     ships; split across more groups",
                    used.len(),
                    crate::char_skin::build::PALETTE_CAP
                ));
            }
            for j in lm.joints.iter_mut() {
                for k in 0..4 {
                    j[k] = slot_of.get(&(j[k] as u32)).copied().unwrap_or(0);
                }
            }
            let ranges: Vec<(u16, u16)> =
                ranges32.iter().map(|&(b, c)| (b as u16, c as u16)).collect();
            let info_row = (0..g.strm_info)
                .rev()
                .find(|&i| &rows[i].tag == b"INFO" && rows[i].u0 != 0xFFFF_FFFF)
                .ok_or_else(|| format!("group {gi}: no INFO(56) leaf before strm_info"))?;
            let mut info = leaf(ucfx, data_off, &rows[info_row]).to_vec();
            crate::char_skin::patch_skin_info56(&mut info, &ranges)?;
            new_bodies.insert(info_row, info);
        }

        let strip = to_strip_connected(&lm.tris);
        // self-verify
        {
            use std::collections::HashSet;
            let norm = |t: [u32; 3]| {
                let mut v = t;
                v.sort_unstable();
                v
            };
            let got: HashSet<[u32; 3]> = strip_to_tris(&strip).into_iter().map(norm).collect();
            let want: HashSet<[u32; 3]> = lm.tris.iter().map(|&t| norm(t)).collect();
            if got != want {
                return Err(format!("group {gi}: strip self-check failed"));
            }
        }
        let tans = synth_tangents(&lm);
        let vb = encode_strm(&lm, &tans);
        let vc = local_n as u32;
        let ic = strip.len() as u32;

        // ASSERT injected <= donor original on BOTH counts (the v1-crash guard)
        if vc > *donor_vc || ic > *donor_ic {
            return Err(format!(
                "group {gi} budget violated: vc {vc}>{donor_vc} or ic {ic}>{donor_ic}"
            ));
        }

        let mut ib = Vec::with_capacity(strip.len() * 2);
        for &x in &strip {
            ib.extend_from_slice(&(x as u16).to_le_bytes());
        }

        // STRM info: keep field0, stride 40, local vc
        let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
        let mut si_body = Vec::with_capacity(12);
        si_body.extend_from_slice(&f0.to_le_bytes());
        si_body.extend_from_slice(&40u32.to_le_bytes());
        si_body.extend_from_slice(&vc.to_le_bytes());
        new_bodies.insert(g.strm_info, si_body);
        new_bodies.insert(g.strm_decl, DECL64.to_vec());
        new_bodies.insert(g.strm_data, vb);
        new_bodies.insert(g.ibuf_info, ic.to_le_bytes().to_vec());
        new_bodies.insert(g.ibuf_data, ib);
        // PRMT: one strip record per existing donor record slot
        let nrec = leaf(ucfx, data_off, &rows[g.prmt]).len() / 16;
        let mut rec = Vec::with_capacity(16);
        rec.extend_from_slice(&6u32.to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&(ic - 2).to_le_bytes());
        rec.extend_from_slice(&((vc - 1) as u16).to_le_bytes());
        rec.extend_from_slice(&(vc as u16).to_le_bytes());
        let mut prmt_body = Vec::with_capacity(nrec * 16);
        for _ in 0..nrec {
            prmt_body.extend_from_slice(&rec);
        }
        new_bodies.insert(g.prmt, prmt_body);

        audits.push(GroupBudgetAudit {
            group: *gi,
            injected_vc: vc,
            donor_vc: *donor_vc,
            injected_ic: ic,
            donor_ic: *donor_ic,
            triangles: chunk.tris.len(),
        });

        for p in &lm.positions {
            for k in 0..3 {
                all_min[k] = all_min[k].min(p[k]);
                all_max[k] = all_max[k].max(p[k]);
            }
        }
        for nrm in &lm.normals {
            tot_nl += ((nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]) as f64).sqrt();
        }
        for t in &tans {
            tot_tl += ((t[0] * t[0] + t[1] * t[1] + t[2] * t[2]) as f64).sqrt();
        }
        tot_v += local_n;
    }

    // ---- neutralise every drawing group that is NOT a target ----
    let target_set: std::collections::HashSet<usize> = targets.iter().map(|t| t.0).collect();
    for &gi in &drawing {
        if target_set.contains(&gi) {
            continue;
        }
        let pg = &groups[gi];
        let mut p = leaf(ucfx, data_off, &rows[pg.prmt]).to_vec();
        for r in 0..p.len() / 16 {
            p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
        }
        new_bodies.insert(pg.prmt, p);
        stats.emptied_groups.push(gi);
    }

    // ---- MTRL repoint (global value-scan) ----
    let mtrl_row = rows
        .iter()
        .position(|r| &r.tag == b"MTRL")
        .ok_or("no MTRL chunk")?;
    let mut mtrl = leaf(ucfx, data_off, &rows[mtrl_row]).to_vec();
    for rp in repoints {
        let mut count = 0usize;
        let mut off = 0usize;
        while off + 4 <= mtrl.len() {
            if read_u32_le(&mtrl, off) == rp.from {
                mtrl[off..off + 4].copy_from_slice(&rp.to.to_le_bytes());
                count += 1;
                off += 4;
            } else {
                off += 1;
            }
        }
        stats.mtrl_repoints.push((rp.from, rp.to, count));
    }
    new_bodies.insert(mtrl_row, mtrl);

    // ---- top INFO bbox over ALL injected verts ----
    stats.bbox_min = all_min;
    stats.bbox_max = all_max;
    let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
    if top.len() >= 28 {
        for k in 0..3 {
            top[4 + k * 4..8 + k * 4].copy_from_slice(&all_min[k].to_le_bytes());
            top[16 + k * 4..20 + k * 4].copy_from_slice(&all_max[k].to_le_bytes());
        }
    }
    new_bodies.insert(0, top);

    stats.vertex_count = tot_v;
    stats.triangle_count = mesh.tris.len();
    stats.avg_normal_len = (tot_nl / tot_v.max(1) as f64) as f32;
    stats.avg_tangent_len = (tot_tl / tot_v.max(1) as f64) as f32;

    let block = reassemble(ucfx, &mut rows, ndesc, data_off, &new_bodies, model_type, new_name_hash);
    // BUILD-TIME GATE: a registered drawing group must never point at a zero-size
    // vbuf/ibuf. Fail the build (don't ship a null-vbuf draw -> wardrobe AV).
    assert_no_empty_drawing_group(&block)
        .map_err(|e| format!("post-reassemble drawing-group gate FAILED: {e}"))?;
    Ok((block, audits, stats))
}

/// BUILD-TIME GATE (reusable). Scan every PRMG group of an emitted model block:
/// if any group is a DRAWING group — i.e. ANY PRMT record has a non-zero
/// draw-count, INCLUDING the 0xFFFFFFFE (0 tris - 2) underflow — then its STRM
/// `data` size MUST be > 0 AND its IBUF index count MUST be > 0. Otherwise the
/// engine's draw walk dereferences a null/zero vbuf and faults (the v1 AV at
/// 0x0085C8D0). Returns Err(group_ordinal + detail) for the first offender.
pub fn assert_no_empty_drawing_group(block: &[u8]) -> Result<(), String> {
    if block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(block, 16) as usize;
    let ucfx = &block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("payload is not UCFX".into());
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    for (gi, g) in find_groups(&rows).iter().enumerate() {
        // ★A zero-size buffer is fatal even when the group's PRMT draw-count is 0 — the engine BINDS
        // every drawing group's vertex buffer regardless and faults on the null surface (AV
        // 0x0085C8D0, the "zero-size vertex-buffer crash"). This check used to `continue` on a zero
        // draw-count, which let a "neutralised" group ship empty buffers and hard-crash the world
        // load. To HIDE a group, keep its buffers at full size and collapse the vertex POSITIONS to
        // the origin (every triangle degenerate) instead of emptying them.
        let vbuf_sz = rows[g.strm_data].size as usize;
        let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0) as usize;
        if vbuf_sz == 0 || ic == 0 {
            return Err(format!(
                "PRMG group {gi} has a ZERO-SIZE buffer (STRM data={vbuf_sz} bytes, IBUF \
                 index_count={ic}) — the engine binds it anyway -> null-surface AV at 0x0085C8D0. \
                 Collapse the vertex positions instead of emptying the buffers."
            ));
        }
    }
    Ok(())
}

/// ★Hide every drawing group of a UCFX container IN PLACE (byte-size preserving).
///
/// Collapses each drawing group's vertex POSITIONS to the origin (so every triangle is degenerate
/// and rasterises nothing) and zeroes its PRMT draw-counts, then recomputes the container CSUM.
/// Byte sizes are unchanged, so a raw block can be patched in place — which is the only way to reach
/// a vehicle's SUB-ENTRY LOD rungs (the ztz98's `_P003_Q0` and its `resident2-..._tracks_*` chain
/// have no model ASET row, so the container tooling cannot see them, yet they keep streaming the
/// DONOR's geometry in at close range).
///
/// ★Do NOT "hide" a group by emptying its buffers: the engine binds every drawing group's vertex
/// buffer even when the draw-count is 0, and a zero-size one is a null-surface AV at 0x0085C8D0.
pub fn collapse_drawing_groups_in_place(ucfx: &mut [u8]) -> Result<usize, String> {
    if ucfx.len() < 20 || &ucfx[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let mut writes: Vec<(usize, u8)> = Vec::new();
    let mut n = 0usize;
    for g in &groups {
        if !group_draws(ucfx, data_off, &rows, g) {
            continue;
        }
        let stride = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 4) as usize;
        let vc = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 8) as usize;
        let decl = parse_decl(leaf(ucfx, data_off, &rows[g.strm_decl]));
        let pos_off = decl.iter().find(|e| e.usage == 0).map(|e| e.offset);
        let vb_abs = data_off + rows[g.strm_data].u0 as usize;
        let vb_len = rows[g.strm_data].size as usize;
        if let Some(po) = pos_off {
            for v in 0..vc {
                let o = v * stride + po;
                if o + 6 <= vb_len {
                    for k in 0..6 {
                        writes.push((vb_abs + o + k, 0));
                    }
                }
            }
        }
        let pr_abs = data_off + rows[g.prmt].u0 as usize;
        let pr_len = rows[g.prmt].size as usize;
        for r in 0..pr_len / 16 {
            for k in 0..4 {
                writes.push((pr_abs + r * 16 + 8 + k, 0));
            }
        }
        n += 1;
    }
    if n == 0 {
        return Ok(0);
    }
    for (o, v) in writes {
        if o < ucfx.len() {
            ucfx[o] = v;
        }
    }
    // Recompute the container CSUM: crc over everything up to the trailing `CSUM` tag.
    let tag = ucfx
        .windows(4)
        .rposition(|w| w == b"CSUM")
        .ok_or("container has no CSUM trailer")?;
    if tag + 8 > ucfx.len() {
        return Err("truncated CSUM trailer".into());
    }
    let csum = crate::crc32::crc32_mercs2(&ucfx[..tag]);
    ucfx[tag + 4..tag + 8].copy_from_slice(&csum.to_le_bytes());
    Ok(n)
}

/// ★Empty EVERY drawing group in a model container — for the FINER LOD RUNGS of a model we have
/// re-skinned.
///
/// A vehicle's geometry is a LOD-BLOCK CHAIN: the resident `_P000_Q3` rung owns HIER/SEGM/MTRL and
/// the coarse geometry, and the finer `_P001_`/`_P002_` rungs are geometry-only refinements. Rungs
/// **refine** (finest wins per node+tier) — so conforming a novel model into the RESIDENT rung alone
/// leaves the DONOR's original high-res geometry in the finer rungs. It looks right from a distance
/// and then, as soon as the camera gets close enough to stream a finer rung in, the donor's hull is
/// drawn straight through ours: cracks, holes and floating shards (two interpenetrating vehicles).
///
/// Neutralising the finer rungs makes the resident rung (ours, `lod_mask 0x7F` = every tier) the
/// only geometry at every distance. The cost is that the model has no LOD refinement — the fix that
/// *keeps* it is to conform higher-poly parts into the finer rungs too.
pub fn neutralise_lod_rung(ucfx: &[u8], new_name_hash: u32) -> Result<(Vec<u8>, usize), String> {
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    if rows.is_empty() {
        return Err("no descriptor rows".into());
    }
    let groups = find_groups(&rows);
    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();
    let mut emptied = 0usize;
    for g in &groups {
        if !group_draws(ucfx, data_off, &rows, g) {
            continue;
        }
        // ★Keep the buffers at FULL SIZE and collapse the vertex POSITIONS to the origin. Emptying
        // them to zero size is fatal: the engine binds every drawing group's vertex buffer even when
        // its PRMT draw-count is 0, and faults on the null surface (AV 0x0085C8D0). Degenerate
        // triangles rasterise nothing, which is all we need.
        let stride = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 4) as usize;
        let vc = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 8) as usize;
        let decl = parse_decl(leaf(ucfx, data_off, &rows[g.strm_decl]));
        let pos_off = decl.iter().find(|e| e.usage == 0).map(|e| e.offset);
        let mut vb = leaf(ucfx, data_off, &rows[g.strm_data]).to_vec();
        if let Some(po) = pos_off {
            for v in 0..vc {
                let o = v * stride + po;
                if o + 6 <= vb.len() {
                    for byte in vb[o..o + 6].iter_mut() {
                        *byte = 0;
                    }
                }
            }
        }
        new_bodies.insert(g.strm_data, vb);
        let mut p = leaf(ucfx, data_off, &rows[g.prmt]).to_vec();
        for r in 0..p.len() / 16 {
            p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
        }
        new_bodies.insert(g.prmt, p);
        emptied += 1;
    }
    // A geometry-only rung has no leaf INFO at row 0 (row 0 is a CONTAINER row), so read the model
    // type from the first LEAF INFO instead of assuming row 0 like the resident rung's layout.
    let model_type = rows
        .iter()
        .find(|r| &r.tag == b"INFO" && r.u0 != 0xFFFF_FFFF)
        .map(|r| read_u32_le(leaf(ucfx, data_off, r), 0))
        .unwrap_or(19);
    let block = reassemble(ucfx, &mut rows, ndesc, data_off, &new_bodies, model_type, new_name_hash);
    // `reassemble` re-wraps in a 20-byte WAD-block header; `smuggler --inject-container` wants the
    // bare UCFX container (that is what the multi-part injector emits). Hand back the container.
    let ucfx_out = block.get(20..).ok_or("reassembled block too short")?.to_vec();
    if ucfx_out.get(0..4) != Some(b"UCFX") {
        return Err("neutralised rung is not a UCFX container".into());
    }
    Ok((ucfx_out, emptied))
}

/// Reassemble a UCFX container (contiguous bodies, recomputed offsets, CSUM) and
/// re-wrap it in a WAD block. Shared by the single- and multi-group injectors.
fn reassemble(
    ucfx: &[u8],
    rows: &mut [Row],
    ndesc: usize,
    data_off: usize,
    new_bodies: &std::collections::HashMap<usize, Vec<u8>>,
    model_type: u32,
    new_name_hash: u32,
) -> Vec<u8> {
    let mut new_data: Vec<u8> = Vec::new();
    for (idx, r) in rows.iter_mut().enumerate() {
        if r.u0 == 0xFFFF_FFFF {
            continue;
        }
        let body = match new_bodies.get(&idx) {
            Some(b) => b.clone(),
            None => leaf(ucfx, data_off, r).to_vec(),
        };
        r.u0 = new_data.len() as u32;
        r.size = body.len() as u32;
        new_data.extend_from_slice(&body);
    }
    let new_data_off = (20 + ndesc * 20) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&new_data_off.to_le_bytes());
    out.extend_from_slice(&ucfx[8..16]);
    out.extend_from_slice(&(ndesc as u32).to_le_bytes());
    for r in rows.iter() {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&r.u0.to_le_bytes());
        out.extend_from_slice(&r.size.to_le_bytes());
        out.extend_from_slice(&r.u2.to_le_bytes());
        out.extend_from_slice(&r.u3.to_le_bytes());
    }
    out.extend_from_slice(&new_data);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());

    let mut block: Vec<u8> = Vec::with_capacity(20 + out.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&new_name_hash.to_le_bytes());
    block.extend_from_slice(&model_type.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(out.len() as u32).to_le_bytes());
    block.extend_from_slice(&out);
    block
}

/// Inject `mesh` into the donor `container_block` (the FULL WAD block: 20-byte
/// wrapper + UCFX + CSUM), targeting drawing group `target_group_ordinal`
/// (ordinal index into the donor's *drawing* groups, e.g. group 16 = torso),
/// repointing MTRL per `repoints`, and re-stamping the block name hash to
/// `new_name_hash`. Returns the new full WAD block bytes.
///
/// The mesh positions/normals/uvs must already be in donor frame (Y-up, feet at
/// Y=0, uniform scale applied to positions, normals unit & rotated only).
pub fn inject_into_donor_block(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinal: usize,
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
) -> Result<(Vec<u8>, InjectStats), String> {
    inject_into_donor_block_impl(
        container_block,
        mesh,
        target_group_ordinal,
        repoints,
        new_name_hash,
        None,
    )
}

/// Faithful CHARACTER injection: like [`inject_into_donor_block`], but the target group is
/// authored as a SKIN group — `mesh.joints` carry **palette-relative** BLENDINDICES and the
/// group's `INFO(56)` leaf is rewritten with the `ranges` palette table (`{u16 base, u16
/// count}`), the exact inverse of the expand in [`crate::model_cubeize`]. Produce `mesh` +
/// `ranges` with [`crate::char_skin::build_character`] (positions already re-posed, joints =
/// its palette slots, weights = its 255-normalised BLENDWEIGHT).
pub fn inject_character_into_donor_block(
    container_block: &[u8],
    mesh: &ExternalMesh,
    ranges: &[(u16, u16)],
    target_group_ordinal: usize,
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
) -> Result<(Vec<u8>, InjectStats), String> {
    inject_into_donor_block_impl(
        container_block,
        mesh,
        target_group_ordinal,
        repoints,
        new_name_hash,
        Some(ranges),
    )
}

fn inject_into_donor_block_impl(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinal: usize,
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
    skin_palette: Option<&[(u16, u16)]>,
) -> Result<(Vec<u8>, InjectStats), String> {
    // ---- unwrap the 20-byte WAD block wrapper ----
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let model_type = read_u32_le(container_block, 8);
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    if groups.is_empty() {
        return Err("no PRMG groups found in donor".into());
    }

    // Map drawing-group ordinal -> absolute group index.
    let drawing: Vec<usize> = (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .collect();
    let target_gi = *drawing.get(
        drawing
            .iter()
            .position(|&gi| gi == target_group_ordinal)
            .ok_or_else(|| {
                format!(
                    "target group {target_group_ordinal} is not a drawing group; drawing={drawing:?}"
                )
            })?,
    )
    .unwrap();

    // ---- build geometry ----
    if mesh.positions.len() > 65534 {
        return Err(format!("vertex count {} exceeds u16", mesh.positions.len()));
    }
    // The naive per-triangle strip costs a flat 6.0 indices/triangle, which overflows the u16
    // IBUF at ~10.9k triangles. That is an encoder artifact, not a format limit: the adjacency
    // strip chains shared-edge runs (~1 idx/tri inside a run, ~2.8 measured over a whole
    // character) and reproduces the identical triangle set. Only reach for it when the naive
    // encoding would not fit, so every mesh that already injected keeps its exact bytes.
    let mut strip = to_strip(&mesh.tris);
    if strip.len() > 65534 {
        strip = to_strip_connected(&mesh.tris);
    }
    if strip.len() > 65534 {
        return Err(format!(
            "strip length {} exceeds u16 even adjacency-stripped ({} tris); split across \
             {} groups via inject_parts_into_donor_block",
            strip.len(),
            mesh.tris.len(),
            strip.len().div_ceil(65534)
        ));
    }
    // self-verify the strip reproduces the triangle set
    {
        use std::collections::HashSet;
        let norm = |t: [u32; 3]| {
            let mut v = t;
            v.sort_unstable();
            v
        };
        let got: HashSet<[u32; 3]> = strip_to_tris(&strip).into_iter().map(norm).collect();
        let want: HashSet<[u32; 3]> = mesh.tris.iter().map(|&t| norm(t)).collect();
        if got != want {
            return Err(format!(
                "strip self-check failed: {} reconstructed vs {} input triangles",
                got.len(),
                want.len()
            ));
        }
    }
    let tans = synth_tangents(mesh);
    let vb = encode_strm(mesh, &tans);
    let vc = mesh.positions.len() as u32;
    let ic = strip.len() as u32;
    let mut ib = Vec::with_capacity(strip.len() * 2);
    for &x in &strip {
        ib.extend_from_slice(&(x as u16).to_le_bytes());
    }

    let mut stats = InjectStats {
        target_group: target_gi,
        vertex_count: vc as usize,
        strip_len: ic as usize,
        triangle_count: mesh.tris.len(),
        ..Default::default()
    };

    // new_bodies: descriptor-row index -> replacement body
    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> =
        std::collections::HashMap::new();

    // ---- write target group geometry ----
    let g = &groups[target_gi];
    let si = &rows[g.strm_info];
    let f0 = read_u32_le(leaf(ucfx, data_off, si), 0);
    let mut strm_info_body = Vec::with_capacity(12);
    strm_info_body.extend_from_slice(&f0.to_le_bytes());
    strm_info_body.extend_from_slice(&40u32.to_le_bytes()); // stride 40
    strm_info_body.extend_from_slice(&vc.to_le_bytes());
    new_bodies.insert(g.strm_info, strm_info_body);
    new_bodies.insert(g.strm_decl, DECL64.to_vec());
    new_bodies.insert(g.strm_data, vb);
    new_bodies.insert(g.ibuf_info, ic.to_le_bytes().to_vec());
    new_bodies.insert(g.ibuf_data, ib);
    // PRMT: one strip draw record per existing donor record slot (keep count)
    let prmt_old = leaf(ucfx, data_off, &rows[g.prmt]);
    let nrec = prmt_old.len() / 16;
    let mut rec = Vec::with_capacity(16);
    rec.extend_from_slice(&6u32.to_le_bytes()); // prim type 6 (strip)
    rec.extend_from_slice(&0u32.to_le_bytes()); // index_start
    rec.extend_from_slice(&(ic - 2).to_le_bytes()); // index_count = strip_len-2
    rec.extend_from_slice(&((vc - 1) as u16).to_le_bytes()); // max vert
    rec.extend_from_slice(&(vc as u16).to_le_bytes()); // vert count
    let mut prmt_body = Vec::with_capacity(nrec * 16);
    for _ in 0..nrec {
        prmt_body.extend_from_slice(&rec);
    }
    new_bodies.insert(g.prmt, prmt_body);

    // ---- SKIN palette: author the group's INFO(56) range table ----
    // For a faithful character the BLENDINDICES in `mesh.joints` are PALETTE-RELATIVE; the
    // group's INFO(56) leaf must carry the `{u16 hier_base, u16 count}` table the reader
    // (model_cubeize) expands. The leaf is the nearest INFO before this group's strm_info
    // (same locate rule inject_parts uses). Preserve its header (+0..20), rewrite +20/+24.
    if let Some(ranges) = skin_palette {
        let info_row = (0..g.strm_info)
            .rev()
            .find(|&i| &rows[i].tag == b"INFO" && rows[i].u0 != 0xFFFF_FFFF)
            .ok_or("SKIN inject: group has no INFO(56) leaf before strm_info")?;
        let mut info = leaf(ucfx, data_off, &rows[info_row]).to_vec();
        crate::char_skin::patch_skin_info56(&mut info, ranges)?;
        new_bodies.insert(info_row, info);
    }

    // ---- neutralise every OTHER drawing group (zero PRMT draw counts) ----
    for &gi in &drawing {
        if gi == target_gi {
            continue;
        }
        let pg = &groups[gi];
        let mut p = leaf(ucfx, data_off, &rows[pg.prmt]).to_vec();
        for r in 0..p.len() / 16 {
            p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
        }
        new_bodies.insert(pg.prmt, p);
        stats.emptied_groups.push(gi);
    }

    // ---- MTRL diffuse repoint (value-scan) ----
    let mtrl_row = rows
        .iter()
        .position(|r| &r.tag == b"MTRL")
        .ok_or("no MTRL chunk")?;
    let mut mtrl = leaf(ucfx, data_off, &rows[mtrl_row]).to_vec();
    for rp in repoints {
        let mut count = 0usize;
        let mut off = 0usize;
        while off + 4 <= mtrl.len() {
            if read_u32_le(&mtrl, off) == rp.from {
                mtrl[off..off + 4].copy_from_slice(&rp.to.to_le_bytes());
                count += 1;
                off += 4;
            } else {
                off += 1;
            }
        }
        stats.mtrl_repoints.push((rp.from, rp.to, count));
    }
    new_bodies.insert(mtrl_row, mtrl);

    // ---- top INFO bbox over injected verts ----
    let mut bmin = [f32::INFINITY; 3];
    let mut bmax = [f32::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for k in 0..3 {
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }
    stats.bbox_min = bmin;
    stats.bbox_max = bmax;
    let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
    if top.len() >= 28 {
        for k in 0..3 {
            top[4 + k * 4..8 + k * 4].copy_from_slice(&bmin[k].to_le_bytes());
            top[16 + k * 4..20 + k * 4].copy_from_slice(&bmax[k].to_le_bytes());
        }
    }
    new_bodies.insert(0, top);

    // normal/tangent length stats (over the encoded buffer would re-quantise; use
    // pre-quantisation values for a faithful report)
    let mut nl = 0.0f64;
    for nrm in &mesh.normals {
        nl += ((nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]) as f64).sqrt();
    }
    let mut tl = 0.0f64;
    for t in &tans {
        tl += ((t[0] * t[0] + t[1] * t[1] + t[2] * t[2]) as f64).sqrt();
    }
    stats.avg_normal_len = (nl / mesh.normals.len().max(1) as f64) as f32;
    stats.avg_tangent_len = (tl / tans.len().max(1) as f64) as f32;

    // ---- reassemble container: contiguous bodies, recompute every offset ----
    let mut new_data: Vec<u8> = Vec::new();
    for (idx, r) in rows.iter_mut().enumerate() {
        if r.u0 == 0xFFFF_FFFF {
            continue;
        }
        let body = match new_bodies.get(&idx) {
            Some(b) => b.clone(),
            None => leaf(ucfx, data_off, r).to_vec(),
        };
        r.u0 = new_data.len() as u32;
        r.size = body.len() as u32;
        new_data.extend_from_slice(&body);
    }
    let new_data_off = (20 + ndesc * 20) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&new_data_off.to_le_bytes());
    out.extend_from_slice(&ucfx[8..16]); // preserve header bytes [8:16]
    out.extend_from_slice(&(ndesc as u32).to_le_bytes());
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&r.u0.to_le_bytes());
        out.extend_from_slice(&r.size.to_le_bytes());
        out.extend_from_slice(&r.u2.to_le_bytes());
        out.extend_from_slice(&r.u3.to_le_bytes());
    }
    out.extend_from_slice(&new_data);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());

    // ---- re-wrap in the WAD block (re-stamp name hash + UCFX size) ----
    let mut block: Vec<u8> = Vec::with_capacity(20 + out.len());
    block.extend_from_slice(&1u32.to_le_bytes()); // flags/version
    block.extend_from_slice(&new_name_hash.to_le_bytes());
    block.extend_from_slice(&model_type.to_le_bytes()); // 0x5b724250
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(out.len() as u32).to_le_bytes());
    block.extend_from_slice(&out);

    Ok((block, stats))
}

// ============================================================================
// STATIC template injection (rigid props: heli/tank/dog/boat/building)
//
// Same conform principle as the skinned path, but the template is a rigid
// static/vehicle model (no bone weights) and its vertex `decl` is preserved
// VERBATIM — we encode the injected mesh into WHATEVER layout the template
// declares (POSITION/TEXCOORD/NORMAL/TANGENT/COLOR at the template's own
// offsets+types), so the shader binding the template already satisfies is never
// disturbed. This is the "engine-accepted structure, novel geometry" path.
// ============================================================================

fn put_f16(vb: &mut [u8], o: usize, v: f32) {
    let b = f16_le(v);
    vb[o] = b[0];
    vb[o + 1] = b[1];
}

/// One parsed decl vertex element.
struct DeclElem {
    offset: usize,
    typ: u16,   // 16 = FLOAT16_4, 15 = FLOAT16_2, 4 = D3DCOLOR, ...
    usage: u16, // 0=POS 1=BLENDWEIGHT 2=BLENDINDICES 3=NORMAL 5=TEXCOORD 6=TANGENT 7=BINORMAL 10=COLOR
}

/// Parse a `decl` chunk body into its element table (8B rows `{u16 stream,
/// u16 offset, u16 type, u16 usage}`, `0xFF` sentinel terminates).
fn parse_decl(decl: &[u8]) -> Vec<DeclElem> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[p], decl[p + 1]]);
        let offset = u16::from_le_bytes([decl[p + 2], decl[p + 3]]);
        let typ = u16::from_le_bytes([decl[p + 4], decl[p + 5]]);
        let usage = u16::from_le_bytes([decl[p + 6], decl[p + 7]]);
        if stream == 0xFF || offset == 0xFF {
            break;
        }
        out.push(DeclElem { offset: offset as usize, typ, usage });
        p += 8;
    }
    out
}

/// Encode the injected mesh into the template's exact vertex layout.
fn encode_strm_from_decl(
    m: &ExternalMesh,
    tans: &[[f32; 4]],
    elems: &[DeclElem],
    stride: usize,
) -> Vec<u8> {
    let n = m.positions.len();
    let mut vb = vec![0u8; n * stride];
    for i in 0..n {
        let base = i * stride;
        let p = m.positions[i];
        let uv = m.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
        let nrm = m.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
        let t = tans.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]);
        for e in elems {
            let o = base + e.offset;
            if o + 2 > vb.len() {
                continue;
            }
            match e.usage {
                0 => {
                    // POSITION
                    put_f16(&mut vb, o, p[0]);
                    put_f16(&mut vb, o + 2, p[1]);
                    put_f16(&mut vb, o + 4, p[2]);
                    if e.typ == 16 {
                        put_f16(&mut vb, o + 6, 1.0);
                    }
                }
                5 => {
                    // TEXCOORD
                    put_f16(&mut vb, o, uv[0]);
                    put_f16(&mut vb, o + 2, uv[1]);
                }
                3 => {
                    // NORMAL
                    put_f16(&mut vb, o, nrm[0]);
                    put_f16(&mut vb, o + 2, nrm[1]);
                    put_f16(&mut vb, o + 4, nrm[2]);
                    if e.typ == 16 {
                        put_f16(&mut vb, o + 6, 1.0);
                    }
                }
                6 => {
                    // TANGENT
                    put_f16(&mut vb, o, t[0]);
                    put_f16(&mut vb, o + 2, t[1]);
                    put_f16(&mut vb, o + 4, t[2]);
                    if e.typ == 16 {
                        put_f16(&mut vb, o + 6, t[3]);
                    }
                }
                7 => {
                    // BINORMAL = cross(normal, tangent)
                    let b = [
                        nrm[1] * t[2] - nrm[2] * t[1],
                        nrm[2] * t[0] - nrm[0] * t[2],
                        nrm[0] * t[1] - nrm[1] * t[0],
                    ];
                    put_f16(&mut vb, o, b[0]);
                    put_f16(&mut vb, o + 2, b[1]);
                    put_f16(&mut vb, o + 4, b[2]);
                    if e.typ == 16 {
                        put_f16(&mut vb, o + 6, 1.0);
                    }
                }
                10 => {
                    // COLOR (D3DCOLOR) white
                    if o + 4 <= vb.len() {
                        vb[o..o + 4].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
                    }
                }
                1 => {
                    // BLENDWEIGHT -> 1.0 to bone 0
                    if o < vb.len() {
                        vb[o] = 0xff;
                    }
                }
                _ => {} // BLENDINDICES(2) etc. stay zero
            }
        }
    }
    vb
}

/// Inject `mesh` into a rigid STATIC template container, targeting one drawing
/// group and neutralising the rest. The template's decl/material/shader/chunk
/// layout are preserved; only geometry (STRM data, IBUF, PRMG bounds, PRMT
/// ranges) is rebuilt and the top INFO bbox + CSUM recomputed. `repoints`
/// re-point material texture hashes (value-scan over the MTRL chunk).
pub fn inject_static_into_donor_block(
    container_block: &[u8],
    mesh: &ExternalMesh,
    target_group_ordinal: usize,
    repoints: &[MtrlRepoint],
    new_name_hash: u32,
    fit_to_template: bool,
    flip_winding: bool,
    keep_groups: bool,
    all_groups: bool,
    raw_targets: &[usize],
    scale_mult: f32,
    neutralize_only: bool,
) -> Result<(Vec<u8>, InjectStats), String> {
    if container_block.len() < 20 {
        return Err("block too small".into());
    }
    let ucfx_len = read_u32_le(container_block, 16) as usize;
    let model_type = read_u32_le(container_block, 8);
    let ucfx = &container_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    if groups.is_empty() {
        return Err("no PRMG groups found in donor".into());
    }
    let drawing: Vec<usize> = (0..groups.len())
        .filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi]))
        .collect();
    // Target selection:
    //   usize::MAX          -> the LARGEST drawing group (most indices).
    //   RAW_BASE + n        -> RAW group ordinal `n` (index into `groups`, NOT
    //                          `drawing`) — needed to hit the specific state-machine
    //                          RENDERED body group (e.g. UH1 group 14), which
    //                          group_draws()'s "has-geometry" filter can't isolate.
    //   otherwise           -> index into `drawing`.
    const RAW_BASE: usize = 0x1000_0000;
    let target_gi = if target_group_ordinal == usize::MAX {
        *drawing
            .iter()
            .max_by_key(|&&gi| read_u32_le(leaf(ucfx, data_off, &rows[groups[gi].ibuf_info]), 0))
            .ok_or("no drawing groups")?
    } else if target_group_ordinal >= RAW_BASE {
        let raw = target_group_ordinal - RAW_BASE;
        if raw >= groups.len() {
            return Err(format!("raw group {raw} out of range (0..{})", groups.len()));
        }
        raw
    } else {
        *drawing
            .get(target_group_ordinal)
            .ok_or_else(|| format!("target group {target_group_ordinal} out of range; drawing={drawing:?}"))?
    };

    // Auto-fit: uniform-scale + recenter the novel mesh into the template's REAL
    // GEOMETRY ENVELOPE — the union bbox of the ORIGINAL vertices of every drawing
    // group, i.e. the actual body the template occupies. NOT the top-INFO bbox:
    // that is padded out to the rotor/collision *sweep sphere* (e.g. UH1 INFO is
    // 17×6×17 m vs a real body of 5.5×3.2×11 m), so fitting to it oversizes the
    // mesh AND its inflated centre floats the mesh off the ground. Fitting to the
    // real envelope makes the novel mesh occupy exactly the replaced body's space:
    // correct size and ground contact. `scale_mult` fine-tunes (1.0 = exact fit).
    let fitted_store: Option<ExternalMesh> = if fit_to_template {
        // ★Fit target = the model header AABB (descriptor row 0's INFO, `+0x04` min / `+0x10` max) —
        // the container's MODEL-SPACE bounds (vehicle_model_spec.md §7; `model_cubeize` header doc).
        // Do NOT union the drawing groups' vertices: a rigid `MESH` sub-object is stored in its
        // BONE-LOCAL space, so that union mixes coordinate frames and reads far too small. Since the
        // host SEGM row is unbound to `node = -1`, no bone matrix is applied and our mesh is consumed
        // in model space — the same frame this AABB describes.
        let t = leaf(ucfx, data_off, &rows[0]);
        let (tmin, tmax) = if t.len() >= 28 {
            let rf = |o: usize| f32::from_bits(read_u32_le(t, o));
            ([rf(4), rf(8), rf(12)], [rf(16), rf(20), rf(24)])
        } else {
            ([f32::MAX; 3], [f32::MIN; 3])
        };
        if tmin[0] <= tmax[0] && !mesh.positions.is_empty() {
            let (mut mmin, mut mmax) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &mesh.positions {
                for k in 0..3 {
                    mmin[k] = mmin[k].min(p[k]);
                    mmax[k] = mmax[k].max(p[k]);
                }
            }
            let mut s = f32::MAX;
            for k in 0..3 {
                let ms = mmax[k] - mmin[k];
                if ms > 1e-4 {
                    s = s.min((tmax[k] - tmin[k]).abs() / ms);
                }
            }
            if !s.is_finite() || s <= 0.0 {
                s = 1.0;
            }
            s *= if scale_mult > 0.0 { scale_mult } else { 1.0 };
            // X/Z: centre on the envelope. Y: BOTTOM-align (mesh min-Y → envelope
            // min-Y) so the prop's feet/skids sit on the ground rather than floating
            // (centre-aligning a mesh shorter than the envelope leaves it hovering).
            let mcen = [(mmin[0] + mmax[0]) * 0.5, (mmin[1] + mmax[1]) * 0.5, (mmin[2] + mmax[2]) * 0.5];
            let tcen = [(tmin[0] + tmax[0]) * 0.5, (tmin[1] + tmax[1]) * 0.5, (tmin[2] + tmax[2]) * 0.5];
            let mut f = mesh.clone();
            for p in f.positions.iter_mut() {
                p[0] = (p[0] - mcen[0]) * s + tcen[0];
                p[1] = (p[1] - mmin[1]) * s + tmin[1];
                p[2] = (p[2] - mcen[2]) * s + tcen[2];
            }
            Some(f)
        } else {
            None
        }
    } else {
        None
    };
    let mesh: &ExternalMesh = fitted_store.as_ref().unwrap_or(mesh);

    if mesh.positions.len() > 65534 {
        return Err(format!("vertex count {} exceeds u16", mesh.positions.len()));
    }
    // Winding flip: fbx_preprocess maps Blender RH (Z-up,Y-fwd) -> engine LH
    // (Y-up,Z-fwd) via (x,z,-y) but does NOT reverse triangle winding, so faces
    // are inside-out for the engine → backface-culled → invisible. Reverse each
    // triangle before strip-ification to correct it.
    let flipped: Vec<[u32; 3]>;
    let tris: &[[u32; 3]] = if flip_winding {
        flipped = mesh.tris.iter().map(|&[a, b, c]| [a, c, b]).collect();
        &flipped
    } else {
        &mesh.tris
    };
    let strip = to_strip(tris);
    if strip.len() > 65534 {
        return Err(format!("strip length {} exceeds u16", strip.len()));
    }
    let tans = synth_tangents(mesh);
    let vc = mesh.positions.len() as u32;
    let ic = strip.len() as u32;
    let mut ib = Vec::with_capacity(strip.len() * 2);
    for &x in &strip {
        ib.extend_from_slice(&(x as u16).to_le_bytes());
    }

    // Injected-mesh bbox (drives PRMG group bounds + top INFO).
    let mut bmin = [f32::INFINITY; 3];
    let mut bmax = [f32::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for k in 0..3 {
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }
    let cen = [(bmin[0] + bmax[0]) * 0.5, (bmin[1] + bmax[1]) * 0.5, (bmin[2] + bmax[2]) * 0.5];
    let rad = {
        let (dx, dy, dz) = ((bmax[0] - bmin[0]) * 0.5, (bmax[1] - bmin[1]) * 0.5, (bmax[2] - bmin[2]) * 0.5);
        (dx * dx + dy * dy + dz * dz).sqrt()
    };

    let mut stats = InjectStats {
        target_group: target_gi,
        vertex_count: vc as usize,
        strip_len: ic as usize,
        triangle_count: mesh.tris.len(),
        ..Default::default()
    };
    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();

    // Which group(s) receive the geometry:
    //   raw_targets non-empty -> exactly those RAW group ordinals (the engine's
    //     actually-rendered set from build_indexed_state — needed because the real
    //     game requires ALL of a group's SEGM state-mask bits set, not just any,
    //     so a mask-0x03 body group is skipped at a fresh mask-0x01 spawn).
    //   all_groups            -> every drawing group.
    //   otherwise             -> the single target_gi.
    // `neutralize_only`: host NO geometry — every drawing group is emptied. This is how a vehicle's
    // FINER LOD rungs (`_P001_`, `_P002_`) are silenced so the template's original near-tier geometry
    // cannot draw over the conformed mesh (the resident rung alone would otherwise be out-detailed
    // at close range). See docs/modernization/vehicle_model_spec.md §1/§3.
    let targets: Vec<usize> = if neutralize_only {
        Vec::new()
    } else if !raw_targets.is_empty() {
        raw_targets.iter().copied().filter(|&g| g < groups.len()).collect()
    } else if all_groups {
        drawing.clone()
    } else {
        vec![target_gi]
    };
    for &tgi in &targets {
        let g = &groups[tgi];
        let stride = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 4) as usize;
        if !(8..=256).contains(&stride) {
            continue;
        }
        let decl_bytes = leaf(ucfx, data_off, &rows[g.strm_decl]).to_vec();
        let elems = parse_decl(&decl_bytes);
        if !elems.iter().any(|e| e.usage == 0) {
            continue;
        }
        let vb = encode_strm_from_decl(mesh, &tans, &elems, stride);
        // STRM info: keep template stride, new vcount. decl kept verbatim.
        let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
        let mut strm_info_body = Vec::with_capacity(12);
        strm_info_body.extend_from_slice(&f0.to_le_bytes());
        strm_info_body.extend_from_slice(&(stride as u32).to_le_bytes());
        strm_info_body.extend_from_slice(&vc.to_le_bytes());
        new_bodies.insert(g.strm_info, strm_info_body);
        new_bodies.insert(g.strm_data, vb);
        new_bodies.insert(g.ibuf_info, ic.to_le_bytes().to_vec());
        new_bodies.insert(g.ibuf_data, ib.clone());
        // PRMT: preserve field[0] (prim-type/matidx unresolved, registry §3).
        let prmt_old = leaf(ucfx, data_off, &rows[g.prmt]);
        let nrec = (prmt_old.len() / 16).max(1);
        let field0 = if prmt_old.len() >= 4 { read_u32_le(prmt_old, 0) } else { 6 };
        let mut rec = Vec::with_capacity(16);
        rec.extend_from_slice(&field0.to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&(ic - 2).to_le_bytes());
        rec.extend_from_slice(&((vc - 1) as u16).to_le_bytes());
        rec.extend_from_slice(&(vc as u16).to_le_bytes());
        let mut prmt_body = Vec::with_capacity(nrec * 16);
        for _ in 0..nrec {
            prmt_body.extend_from_slice(&rec);
        }
        new_bodies.insert(g.prmt, prmt_body);
        // This group's PRMG INFO cull bounds → fit the injected geometry.
        if let Some(pir) =
            (0..g.strm_info).rev().find(|&i| &rows[i].tag == b"INFO" && rows[i].u0 != 0xFFFF_FFFF)
        {
            let mut pi = leaf(ucfx, data_off, &rows[pir]).to_vec();
            if pi.len() >= 60 {
                for k in 0..3 {
                    pi[20 + k * 4..24 + k * 4].copy_from_slice(&cen[k].to_le_bytes());
                    pi[36 + k * 4..40 + k * 4].copy_from_slice(&bmin[k].to_le_bytes());
                    pi[48 + k * 4..52 + k * 4].copy_from_slice(&bmax[k].to_le_bytes());
                }
                pi[32..36].copy_from_slice(&rad.to_le_bytes());
                new_bodies.insert(pir, pi);
            }
        }
    }

    // Neutralise every drawing group NOT receiving geometry (unless keep_groups).
    // With all_groups/raw_targets covering the whole rendered set this empties
    // nothing; for a single target it empties the rest.
    if !keep_groups {
        for &gi in &drawing {
            if targets.contains(&gi) {
                continue;
            }
            let pg = &groups[gi];
            let mut p = leaf(ucfx, data_off, &rows[pg.prmt]).to_vec();
            let nr = p.len() / 16;
            for r in 0..nr {
                p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
            }
            new_bodies.insert(pg.prmt, p);
            stats.emptied_groups.push(gi);
        }
    }

    // MTRL texture repoint (value-scan).
    if let Some(mtrl_row) = rows.iter().position(|r| &r.tag == b"MTRL") {
        let mut mtrl = leaf(ucfx, data_off, &rows[mtrl_row]).to_vec();
        for rp in repoints {
            let mut count = 0usize;
            let mut off = 0usize;
            while off + 4 <= mtrl.len() {
                if read_u32_le(&mtrl, off) == rp.from {
                    mtrl[off..off + 4].copy_from_slice(&rp.to.to_le_bytes());
                    count += 1;
                    off += 4;
                } else {
                    off += 1;
                }
            }
            stats.mtrl_repoints.push((rp.from, rp.to, count));
        }
        new_bodies.insert(mtrl_row, mtrl);
    }

    // ★UNBIND THE HOST SEGM ROW (the corrected binding — vehicle_model_spec.md §2/§4).
    // The group's segment record is reached group → parent MESH/SKIN sub-object → INDX[sub_object]
    // = seg_id → SEGM[seg_id]  (NOT INDX[group], and the value is a seg_id, not a node).
    // Rewrite that record to `{node: -1, lod_mask: 0x7f}`, which makes the injected mesh:
    //   • pass draw-gate clause 3 unconditionally (`node < 0` = never destruction-gated),
    //   • never be superseded by a finer LOD rung (`apply_supersede` skips `node < 0`),
    //   • draw at EVERY LOD tier (0x7f), so it survives at any camera distance,
    //   • and be consumed in MODEL space — no bone world-rest matrix is applied, which is exactly
    //     the space our mesh is authored in (a rigid MESH on a real node would be interpreted as
    //     bone-LOCAL and get flung by that node's transform).
    // `SEGM[i].seg_id == i` is the format's self-check invariant — preserve it.
    if !neutralize_only {
        let seg_id = crate::model_cubeize::read_model_meshes(ucfx)
            .ok()
            .and_then(|ms| ms.iter().find(|m| m.group_index == target_gi).map(|m| m.seg_id));
        let segm_row = rows
            .iter()
            .position(|r| &r.tag == b"SEGM" && r.u0 != 0xFFFF_FFFF);
        if let (Some(seg_id), Some(sr)) = (seg_id, segm_row) {
            let mut segm = leaf(ucfx, data_off, &rows[sr]).to_vec();
            let o = seg_id * 4;
            if o + 4 <= segm.len() {
                segm[o..o + 2].copy_from_slice(&0xFFFFu16.to_le_bytes()); // node = -1 (i16)
                segm[o + 2] = seg_id as u8; // seg_id self-reference invariant
                segm[o + 3] = 0x7F; // present at every LOD tier
                new_bodies.insert(sr, segm);
                stats.unbound_seg = Some(seg_id);
            }
        }
    }

    // Top INFO bbox over injected verts (bmin/bmax computed above; per-group
    // PRMG cull bounds were fitted in the geometry-write loop).
    if !neutralize_only && bmin[0] <= bmax[0] {
        stats.bbox_min = bmin;
        stats.bbox_max = bmax;
        let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
        if top.len() >= 28 {
            for k in 0..3 {
                top[4 + k * 4..8 + k * 4].copy_from_slice(&bmin[k].to_le_bytes());
                top[16 + k * 4..20 + k * 4].copy_from_slice(&bmax[k].to_le_bytes());
            }
        }
        new_bodies.insert(0, top);
    }

    // Reassemble container, recompute offsets, CSUM, rewrap in WAD block.
    let mut new_data: Vec<u8> = Vec::new();
    for (idx, r) in rows.iter_mut().enumerate() {
        if r.u0 == 0xFFFF_FFFF {
            continue;
        }
        let body = match new_bodies.get(&idx) {
            Some(b) => b.clone(),
            None => leaf(ucfx, data_off, r).to_vec(),
        };
        r.u0 = new_data.len() as u32;
        r.size = body.len() as u32;
        new_data.extend_from_slice(&body);
    }
    let new_data_off = (20 + ndesc * 20) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&new_data_off.to_le_bytes());
    out.extend_from_slice(&ucfx[8..16]);
    out.extend_from_slice(&(ndesc as u32).to_le_bytes());
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&r.u0.to_le_bytes());
        out.extend_from_slice(&r.size.to_le_bytes());
        out.extend_from_slice(&r.u2.to_le_bytes());
        out.extend_from_slice(&r.u3.to_le_bytes());
    }
    out.extend_from_slice(&new_data);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());

    let mut block: Vec<u8> = Vec::with_capacity(20 + out.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&new_name_hash.to_le_bytes());
    block.extend_from_slice(&model_type.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(out.len() as u32).to_le_bytes());
    block.extend_from_slice(&out);
    Ok((block, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal UCFX with an INFO header (node count @0x20) + a HIER chunk of `positions.len()`
    /// nodes, node i parent = `parents[i]` (0xFFFF root), identity-rotation world at `positions[i]`.
    fn build_hier_donor(positions: &[[f32; 3]], parents: &[i32]) -> Vec<u8> {
        let stride = crate::skeleton::HIER_NODE_STRIDE;
        let n = positions.len();
        let mut top = vec![0u8; 72];
        top[0x20..0x24].copy_from_slice(&(n as u32).to_le_bytes());
        // world = local·parent, so LOCAL = translation relative to parent's world position.
        let mut hier = vec![0u8; n * stride];
        for i in 0..n {
            let o = i * stride;
            hier[o..o + 4].copy_from_slice(&(0x1000_0000u32 + i as u32).to_le_bytes());
            hier[o + 4..o + 8].copy_from_slice(&((0xFFFFu32 << 16) | 1).to_le_bytes());
            let p = parents[i];
            hier[o + 8..o + 10]
                .copy_from_slice(&(if p < 0 { 0xFFFFu16 } else { p as u16 }).to_le_bytes());
            hier[o + 10..o + 12].copy_from_slice(&0xFFFFu16.to_le_bytes());
            let ppos = if p < 0 { [0.0; 3] } else { positions[p as usize] };
            let mut m = [[0.0f32; 4]; 4];
            for d in 0..4 {
                m[d][d] = 1.0;
            }
            for d in 0..3 {
                m[3][d] = positions[i][d] - ppos[d];
            }
            for r in 0..4 {
                for c in 0..4 {
                    let off = o + 16 + (r * 4 + c) * 4;
                    hier[off..off + 4].copy_from_slice(&m[r][c].to_le_bytes());
                }
            }
        }
        // emit UCFX with two chunks: INFO(row0), HIER
        let ndesc = 2usize;
        let data_off = 20 + ndesc * 20;
        let mut data = Vec::new();
        let info_off = data.len() as u32;
        data.extend_from_slice(&top);
        let hier_off = data.len() as u32;
        data.extend_from_slice(&hier);
        let mut out = Vec::new();
        out.extend_from_slice(b"UCFX");
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(ndesc as u32).to_le_bytes());
        for (tag, off, sz) in
            [(b"INFO", info_off, top.len() as u32), (b"HIER", hier_off, hier.len() as u32)]
        {
            out.extend_from_slice(tag);
            out.extend_from_slice(&off.to_le_bytes());
            out.extend_from_slice(&sz.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        out.extend_from_slice(&data);
        let csum = crc32_mercs2(&out);
        out.extend_from_slice(b"CSUM");
        out.extend_from_slice(&csum.to_le_bytes());
        out
    }

    #[test]
    fn append_new_bones_grows_hier_and_links() {
        // root(0) -> body(1) at (0,0,0); rotor(2) at (0,8,0)
        let donor =
            build_hier_donor(&[[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 8.0, 0.0]], &[-1, 0, 0]);
        let base = crate::skeleton::Skeleton::from_block(&wrap_block(&donor, 0)).unwrap();
        assert_eq!(base.bones.len(), 3);

        // add a static bone under body(1) at (1,2,3) and a rotor-child bone under rotor(2) at (0,9,0)
        let bones = [
            NewBone {
                name_hash: 0xAAAA_0001,
                parent: 1,
                world_pos: [1.0, 2.0, 3.0],
                local_aabb: ([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]),
            },
            NewBone {
                name_hash: 0xAAAA_0002,
                parent: 2,
                world_pos: [0.0, 9.0, 0.0],
                local_aabb: ([-1.0, 0.0, -1.0], [1.0, 1.0, 1.0]),
            },
        ];
        let (out, idx) = append_new_bones(&donor, &bones).unwrap();
        assert_eq!(idx, vec![3, 4]);

        // header node count bumped
        let (data_off, _n, rows) = parse_rows(&out);
        let top = leaf(&out, data_off, &rows[0]);
        assert_eq!(read_u32_le(top, 0x20), 5);

        // re-parse: new bones exist at the requested WORLD positions with the right parents
        let sk = crate::skeleton::Skeleton::from_block(&wrap_block(&out, 0)).unwrap();
        assert_eq!(sk.bones.len(), 5);
        assert_eq!(sk.bones[3].parent, 1);
        assert_eq!(sk.bones[4].parent, 2);
        let wp3 = sk.bones[3].world_pos();
        let wp4 = sk.bones[4].world_pos();
        assert!((wp3[0] - 1.0).abs() < 1e-4 && (wp3[1] - 2.0).abs() < 1e-4 && (wp3[2] - 3.0).abs() < 1e-4, "bone3 {wp3:?}");
        assert!((wp4[1] - 9.0).abs() < 1e-4, "bone4 {wp4:?}");
        assert_eq!(sk.bones[3].name_hash, 0xAAAA_0001);

        // parent's first-child link now points at the prepended new bone
        let stride = crate::skeleton::HIER_NODE_STRIDE;
        let hier = rows.iter().position(|r| &r.tag == b"HIER" && r.u0 != 0xFFFF_FFFF).unwrap();
        let hbody = leaf(&out, data_off, &rows[hier]);
        let body_first_child = u16::from_le_bytes([hbody[1 * stride + 6], hbody[1 * stride + 7]]);
        assert_eq!(body_first_child, 3, "body(1) first-child should be new bone 3");
        let new_sibling = u16::from_le_bytes([hbody[3 * stride + 10], hbody[3 * stride + 11]]);
        assert_eq!(new_sibling, 0xFFFF, "bone3 next-sibling = body's old (none)");
    }

    #[test]
    fn f16_roundtrip() {
        for v in [-1.834f32, 0.0, 0.5, 1.0, 1.2176, -0.131] {
            let b = f16_le(v);
            let r = read_f16_le(&b, 0);
            assert!((r - v).abs() < 0.01, "f16 roundtrip {v} -> {r}");
        }
    }

    #[test]
    fn strip_roundtrips_triangles() {
        use std::collections::HashSet;
        let tris = vec![[0, 1, 2], [2, 1, 3], [4, 5, 6], [1, 7, 8]];
        let strip = to_strip(&tris);
        let norm = |t: [u32; 3]| {
            let mut v = t;
            v.sort_unstable();
            v
        };
        let got: HashSet<_> = strip_to_tris(&strip).into_iter().map(norm).collect();
        let want: HashSet<_> = tris.iter().map(|&t| norm(t)).collect();
        assert_eq!(got, want);
    }

    /// The adjacency-greedy stripper must encode exactly the input triangle set
    /// (winding-preserving: each input triangle appears with the SAME cyclic
    /// orientation in the strip reconstruction) and must be far cheaper than the
    /// per-triangle `to_strip` for a connected grid.
    #[test]
    fn connected_strip_roundtrips_and_is_cheap() {
        use std::collections::HashSet;
        // a connected grid mesh: WxH quads -> 2*W*H triangles, all CCW
        let (w, h) = (8u32, 8u32);
        let mut tris: Vec<[u32; 3]> = Vec::new();
        let vid = |x: u32, y: u32| y * (w + 1) + x;
        for y in 0..h {
            for x in 0..w {
                let (a, b, c, d) = (vid(x, y), vid(x + 1, y), vid(x + 1, y + 1), vid(x, y + 1));
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        let strip = to_strip_connected(&tris);
        // cyclic-orientation set: a triangle and its rotations are equal, mirror differs
        let cyc = |t: [u32; 3]| {
            let rots = [[t[0], t[1], t[2]], [t[1], t[2], t[0]], [t[2], t[0], t[1]]];
            *rots.iter().min().unwrap()
        };
        let got: HashSet<_> = strip_to_tris(&strip).into_iter().map(cyc).collect();
        let want: HashSet<_> = tris.iter().map(|&t| cyc(t)).collect();
        assert_eq!(got, want, "connected strip lost/flipped triangles");
        // cheaper than per-triangle: dense grid should be well under 6/tri
        assert!(
            strip.len() < tris.len() * 3,
            "connected strip not cheap: {} indices for {} tris",
            strip.len(),
            tris.len()
        );
    }

    #[test]
    fn tangents_are_unit() {
        // simple quad in XY, planar UVs
        let m = ExternalMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            tris: vec![[0, 1, 2], [0, 2, 3]],
            ..Default::default()
        };
        let tans = synth_tangents(&m);
        for t in &tans {
            let l = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            assert!((l - 1.0).abs() < 1e-4, "tangent not unit: {l}");
            assert!(t[3] == 1.0 || t[3] == -1.0);
        }
    }

    /// End-to-end: build a tiny synthetic donor block with two drawing groups,
    /// inject a triangle into group 0, confirm group 1 is neutralised, MTRL is
    /// repointed, CSUM verifies and the block re-parses.
    #[test]
    fn inject_minimal_donor() {
        let block = build_synthetic_donor();
        let mesh = ExternalMesh {
            positions: vec![[0.0, 0.0, 0.0], [0.5, 1.0, 0.1], [-0.5, 0.8, -0.1]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            tris: vec![[0, 1, 2]],
            ..Default::default()
        };
        let (out, stats) = inject_into_donor_block(
            &block,
            &mesh,
            0, // target drawing group 0
            &[MtrlRepoint { from: 0xAAAA_AAAA, to: 0xBBBB_BBBB }],
            0xC15489A1, // pmc_hum_cesium
        )
        .expect("inject");

        assert_eq!(stats.target_group, 0);
        assert_eq!(stats.vertex_count, 3);
        assert_eq!(stats.emptied_groups, vec![1]);
        assert_eq!(stats.mtrl_repoints[0].2, 1, "one MTRL repoint");
        assert!((stats.avg_normal_len - 1.0).abs() < 0.01);
        assert!((stats.avg_tangent_len - 1.0).abs() < 0.01);

        // re-parse output block
        assert_eq!(read_u32_le(&out, 4), 0xC15489A1); // name re-stamped
        let ulen = read_u32_le(&out, 16) as usize;
        let ucfx = &out[20..20 + ulen];
        assert_eq!(&ucfx[0..4], b"UCFX");
        // CSUM verify
        let body = &ucfx[..ucfx.len() - 8];
        assert_eq!(&ucfx[ucfx.len() - 8..ucfx.len() - 4], b"CSUM");
        let stored = read_u32_le(ucfx, ucfx.len() - 4);
        assert_eq!(crc32_mercs2(body), stored, "CSUM mismatch");

        // group 0 draws, group 1 zeroed
        let (data_off, _n, rows) = parse_rows(ucfx);
        let groups = find_groups(&rows);
        assert!(group_draws(ucfx, data_off, &rows, &groups[0]));
        assert!(!group_draws(ucfx, data_off, &rows, &groups[1]));
        // group 0 decl is 64-byte stride-40
        assert_eq!(rows[groups[0].strm_decl].size, 64);
        let si = leaf(ucfx, data_off, &rows[groups[0].strm_info]);
        assert_eq!(read_u32_le(si, 4), 40);
    }

    /// Full byte round-trip for the faithful character path: inject PALETTE-RELATIVE
    /// BLENDINDICES + a SKIN `INFO(56)` range table, read the block back with
    /// `model_cubeize` (the proven reader), and assert the decoded GLOBAL joints equal the
    /// palette expansion. Writer = the exact inverse of the reader, proven end-to-end
    /// through real bytes (not just the algorithm).
    #[test]
    fn inject_character_palette_roundtrips_through_reader() {
        let block = build_synthetic_donor();
        // palette range table: two runs → global HIER {3} and {20,21,22}. slots: 3→0,
        // 20→1, 21→2, 22→3.
        let ranges: Vec<(u16, u16)> = vec![(3, 1), (20, 3)];
        let palette = crate::char_skin::expand_ranges(&ranges); // [3,20,21,22]
        // three verts, each single-influence on a distinct palette SLOT.
        let vjoint_slots = [[0u8, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0]];
        let mesh = ExternalMesh {
            positions: vec![[0.0, 0.0, 0.0], [0.5, 1.0, 0.1], [-0.5, 0.8, -0.1]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            tris: vec![[0, 1, 2]],
            joints: vjoint_slots.to_vec(),
            weights: vec![[0xff, 0, 0, 0]; 3],
        };
        let (out, _stats) = inject_character_into_donor_block(
            &block,
            &mesh,
            &ranges,
            0,
            &[],
            0xC15489A1,
        )
        .expect("character inject");

        let ulen = read_u32_le(&out, 16) as usize;
        let ucfx = &out[20..20 + ulen];
        // CSUM must still verify after the INFO(56) rewrite
        let body = &ucfx[..ucfx.len() - 8];
        assert_eq!(crc32_mercs2(body), read_u32_le(ucfx, ucfx.len() - 4), "CSUM");

        // The reader expands the INFO(56) palette and returns GLOBAL joints.
        let meshes = crate::model_cubeize::read_model_meshes(ucfx).expect("read back");
        let g0 = meshes
            .iter()
            .find(|m| m.group_index == 0)
            .expect("group 0 in readback");
        assert_eq!(g0.joints.len(), 3, "vertex count");
        for (vi, slots) in vjoint_slots.iter().enumerate() {
            let expected_global = palette[slots[0] as usize];
            assert_eq!(
                g0.joints[vi][0] as u16, expected_global,
                "vertex {vi}: reader decoded slot {} to {} (expected global {expected_global})",
                slots[0], g0.joints[vi][0]
            );
        }
    }

    /// Multi-group split: a mesh too big for one synthetic group is partitioned
    /// across both groups, each within its donor original caps.
    #[test]
    fn inject_multi_splits_across_groups() {
        let block = build_synthetic_donor(); // two groups, each donor vc=3 ic=4
        // 2 triangles, 4 unique verts -> cannot fit one group (vc cap 3); must split.
        let mesh = ExternalMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [0.5, 1.0, 0.1],
                [-0.5, 0.8, -0.1],
                [0.2, 0.3, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.3, 0.3]],
            tris: vec![[0, 1, 2], [1, 2, 3]],
            ..Default::default()
        };
        let (out, audits, stats) = inject_multi_into_donor_block(
            &block,
            &mesh,
            &[0, 1],
            &[MtrlRepoint { from: 0xAAAA_AAAA, to: 0xBBBB_BBBB }],
            0xC15489A1,
        )
        .expect("inject multi");
        assert_eq!(audits.len(), 2);
        // every group <= donor original on BOTH counts
        for a in &audits {
            assert!(a.injected_vc <= a.donor_vc, "vc {} > {}", a.injected_vc, a.donor_vc);
            assert!(a.injected_ic <= a.donor_ic, "ic {} > {}", a.injected_ic, a.donor_ic);
        }
        // both triangles placed
        assert_eq!(audits.iter().map(|a| a.triangles).sum::<usize>(), 2);
        assert!(stats.emptied_groups.is_empty(), "no extra groups to empty");

        // CSUM verifies + both groups draw
        let ulen = read_u32_le(&out, 16) as usize;
        let ucfx = &out[20..20 + ulen];
        let body = &ucfx[..ucfx.len() - 8];
        assert_eq!(crc32_mercs2(body), read_u32_le(ucfx, ucfx.len() - 4));
        let (data_off, _n, rows) = parse_rows(ucfx);
        let groups = find_groups(&rows);
        assert!(group_draws(ucfx, data_off, &rows, &groups[0]));
        assert!(group_draws(ucfx, data_off, &rows, &groups[1]));
    }

    /// Over-capacity: a mesh exceeding total group capacity is rejected (not a
    /// silent overrun — the v1 bug).
    #[test]
    fn inject_multi_rejects_overflow() {
        let block = build_synthetic_donor(); // one usable group cap vc=3
        let big = ExternalMesh {
            positions: (0..10).map(|i| [i as f32, 0.0, 0.0]).collect(),
            normals: vec![[0.0, 0.0, 1.0]; 10],
            uvs: vec![[0.0, 0.0]; 10],
            tris: (0..8).map(|i| [i, i + 1, i + 2]).collect(),
            ..Default::default()
        };
        let r = inject_multi_into_donor_block(&block, &big, &[0], &[], 0xC15489A1);
        assert!(r.is_err(), "should reject insufficient capacity");
    }

    /// Build a 2-group synthetic UCFX model donor block for the e2e test. Layout
    /// per group: PRMG marker, INFO(56), STRM{info,decl,data}, IBUF{info,data},
    /// PRMT(16). Plus a top INFO(72) and an MTRL holding the repoint target.
    fn build_synthetic_donor() -> Vec<u8> {
        // descriptor rows we will emit (tag, is_container, body)
        struct R {
            tag: [u8; 4],
            body: Option<Vec<u8>>,
        }
        fn mk(tag: &[u8; 4], body: Option<Vec<u8>>) -> R {
            R { tag: *tag, body }
        }
        let decl = DECL64.to_vec();
        let mut top = vec![0u8; 72];
        top[0..4].copy_from_slice(&57u32.to_le_bytes());
        let mut mtrl = vec![0u8; 128];
        mtrl[108..112].copy_from_slice(&0xAAAA_AAAAu32.to_le_bytes());

        let mut make_group = |verts: u32, idx: u32| -> Vec<R> {
            let mut si = Vec::new();
            si.extend_from_slice(&7u32.to_le_bytes());
            si.extend_from_slice(&32u32.to_le_bytes());
            si.extend_from_slice(&verts.to_le_bytes());
            let data = vec![0u8; (verts * 32) as usize];
            let mut ii = Vec::new();
            ii.extend_from_slice(&idx.to_le_bytes());
            let ibd = vec![0u8; (idx * 2) as usize];
            let mut prmt = Vec::new();
            prmt.extend_from_slice(&6u32.to_le_bytes());
            prmt.extend_from_slice(&0u32.to_le_bytes());
            prmt.extend_from_slice(&idx.saturating_sub(2).to_le_bytes());
            prmt.extend_from_slice(&((verts - 1) as u16).to_le_bytes());
            prmt.extend_from_slice(&(verts as u16).to_le_bytes());
            vec![
                mk(b"PRMG", None),
                mk(b"INFO", Some(vec![0u8; 56])),
                mk(b"STRM", None),
                mk(b"info", Some(si)),
                mk(b"decl", Some(decl.clone())),
                mk(b"data", Some(data)),
                mk(b"IBUF", None),
                mk(b"info", Some(ii)),
                mk(b"data", Some(ibd)),
                mk(b"PRMT", Some(prmt)),
            ]
        };

        let mut rows: Vec<R> = vec![mk(b"INFO", Some(top)), mk(b"MTRL", Some(mtrl))];
        rows.extend(make_group(3, 6)); // group 0 (vc cap 3 forces the split; ic 6 fits a connected-strip tri)
        rows.extend(make_group(3, 6)); // group 1

        let ndesc = rows.len();
        let data_off = 20 + ndesc * 20;
        // assemble bodies contiguously, recording offsets
        let mut data = Vec::new();
        let mut descs = Vec::new();
        for r in &rows {
            match &r.body {
                Some(b) => {
                    let off = data.len() as u32;
                    descs.push((r.tag, off, b.len() as u32));
                    data.extend_from_slice(b);
                }
                None => descs.push((r.tag, 0xFFFF_FFFF, 0)),
            }
        }
        let mut ucfx = Vec::new();
        ucfx.extend_from_slice(b"UCFX");
        ucfx.extend_from_slice(&(data_off as u32).to_le_bytes());
        ucfx.extend_from_slice(&0u32.to_le_bytes());
        ucfx.extend_from_slice(&0u32.to_le_bytes());
        ucfx.extend_from_slice(&(ndesc as u32).to_le_bytes());
        for (tag, u0, sz) in &descs {
            ucfx.extend_from_slice(tag);
            ucfx.extend_from_slice(&u0.to_le_bytes());
            ucfx.extend_from_slice(&sz.to_le_bytes());
            ucfx.extend_from_slice(&0u32.to_le_bytes());
            ucfx.extend_from_slice(&0u32.to_le_bytes());
        }
        ucfx.extend_from_slice(&data);
        let csum = crc32_mercs2(&ucfx);
        ucfx.extend_from_slice(b"CSUM");
        ucfx.extend_from_slice(&csum.to_le_bytes());

        let mut block = Vec::new();
        block.extend_from_slice(&1u32.to_le_bytes());
        block.extend_from_slice(&0x786a_db07u32.to_le_bytes());
        block.extend_from_slice(&0x5b72_4250u32.to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&(ucfx.len() as u32).to_le_bytes());
        block.extend_from_slice(&ucfx);
        block
    }
}

// ============================================================================
// MULTI-PART conform (vehicles): several meshes -> several PRMG groups, each with
// its own material and its own SEGM binding.
//
// `inject_static_into_donor_block` hosts one rigid mesh in one group. A vehicle needs
// more (docs/modernization/vehicle_model_spec.md 2/4):
//   * one material PER GROUP  - PRMT word 0 IS the MTRL index, so body/gear/glass/rotor
//     each need their own group to carry their own skin;
//   * moving parts must be bound to the HIER NODE that moves them (the rotor node is
//     driven by BoneCtrlLocalRotation), and a rigid MESH is authored in that node LOCAL
//     space - so its verts go through inverse(node.world);
//   * static parts take node = -1: model space, always visible (clause 3 cannot gate a
//     negative node), every LOD tier, never superseded by a finer rung.
// ============================================================================

/// One part of a multi-part conform.
pub struct PartSpec {
    pub label: String,
    pub mesh: ExternalMesh,
    /// Host PRMG drawing-group ordinal.
    pub group: usize,
    /// HIER node to bind to. **ALWAYS give a REAL node** — see the `node = -1` trap below.
    ///
    /// ★`node = -1` IS NOT A VALID HOST for a rigid `MESH`. The draw gate treats a negative node
    /// as "always visible" (clause 3 can't gate it) and `apply_supersede` skips it, so the spec's
    /// "bound to no node" reads like a free pass — but a rigid MESH sub-object is authored in its
    /// node's LOCAL space and the engine MULTIPLIES it by that node's matrix. With `-1` the engine
    /// indexes the node-matrix array at -1 → out-of-bounds → a garbage matrix that changes every
    /// frame. Observed in-game: the mesh flickered ~1 frame in 60 and rendered wherever the CAMERA
    /// was looking (it was picking up view-matrix memory). Visibility was fine; the transform was junk.
    ///
    /// Bind static parts to a real, default-ENABLED, non-animated node instead — the intact-body
    /// slot `0x255EAB53` is the right one (translation (0,0,0), so its matrix is a no-op), and this
    /// conform pre-multiplies by `inverse(node.world)` so the geometry still lands in model space.
    pub node: i32,
    /// MTRL record index this group draws with (PRMT word 0).
    pub material_index: u32,
    /// Re-centre this part's X/Z onto its node's origin before binding.
    ///
    /// A SPINNING part must straddle its node's axis or it ORBITS instead of rotating: the engine
    /// spins the NODE, so geometry offset from the node origin sweeps a circle around it. Our
    /// rotor's hub sits wherever the source model put it, which is not where the template's mast
    /// node is — observed in-game as blades that spin visibly off-centre. Aligning the part's X/Z
    /// bbox centre to the node origin puts the hub on the axis. Y is left alone (the rotor's height
    /// comes from our own model, not the template's mast).
    ///
    /// Only for parts on a node that actually moves — never for static parts, which would slide.
    pub recenter_xz: bool,
}

#[derive(Default)]
pub struct PartStat {
    pub label: String,
    pub group: usize,
    pub node: i32,
    pub material_index: u32,
    pub seg_id: usize,
    pub vertex_count: usize,
    pub triangle_count: usize,
}

#[derive(Default)]
pub struct PartsStats {
    /// True when the HIER rig (all hardpoints) was scaled to match the model.
    pub rig_scaled: bool,
    /// Number of PHY2 convex collision hulls rescaled to match the model.
    pub phy2_hulls_scaled: usize,
    /// (node, new world position) for each --node-at retarget applied.
    pub nodes_moved: Vec<(usize, [f32; 3])>,
    pub fit_scale: f32,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub parts: Vec<PartStat>,
    pub emptied_groups: Vec<usize>,
    pub mtrl_repoints: usize,
}

/// Conform a multi-part model into a real vehicle template container (raw UCFX in, raw UCFX out).
pub fn inject_parts_into_template(
    ucfx: &[u8],
    parts: &[PartSpec],
    repoints: &[(u32, u32)],
    // Set a specific MTRL record's diffuse BY INDEX. Hash repoints cannot express per-part skins
    // when a template shares one texture across several materials (the ztz98 has 8 materials but
    // only 4 distinct diffuse hashes), yet a vehicle wants a separate skin per part — and the
    // TRACK materials must keep their own texture because they are the ones that scroll.
    mtrl_sets: &[(usize, usize, u32)],
    // APPEND a material: clone record `src` and give the copy a new diffuse. A novel model needs
    // more skins than the donor happens to carry, and NOT every donor material is usable — the
    // ztz98's materials 0/1/2 have flags 0x0000 / tex_count 2 (an untextured shader variant) and
    // render as a flat colour no matter what texture you bind. Only the flags-0x0080+ materials
    // sample a texture. Cloning a known-good one is how we get another valid skin slot.
    mtrl_adds: &[(usize, u32)],
    // REPLACE material `dst` in place with a clone of `src` + a new diffuse, keeping `dst`'s own
    // name hash and the record COUNT. Prefer this over `mtrl_adds`: growing the material set past
    // the donor's original count leaves the 9th material with no shader-registry slot, and the
    // renderer faults on a NULL shader at 0x00855691 the moment the model is drawn.
    mtrl_replaces: &[(usize, usize, u32)],
    // RETARGET a HIER node to a model-space point (post-fit): move the donor's RIG onto our model.
    // This is how a novel tank's turret/barrel get their OWN axes instead of inheriting the donor's.
    node_ats: &[(usize, [f32; 3])],
    new_name_hash: u32,
    scale_mult: f32,
    flip_winding: bool,
    y_offset: f32,
    // Percentile (0-100) used to measure the model for the FIT SCALE. 100 = the raw bbox.
    //
    // ★Use <100 when a thin outlier inflates a dimension. The uniform fit picks the TIGHTEST axis,
    // so a 20-vertex radio antenna sticking 33% above the turret makes "height" the binding axis and
    // shrinks the WHOLE tank by ~25% — the model reads squashed, and no donor ever "fits". Measuring
    // at the 99.5th percentile ignores such spikes. Position still uses the TRUE extents (the mast
    // is still drawn; it is just not allowed to dictate scale).
    fit_percentile: f32,
) -> Result<(Vec<u8>, PartsStats), String> {
    if ucfx.len() < 20 || &ucfx[0..4] != b"UCFX" {
        return Err("template is not a UCFX container".into());
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let groups = find_groups(&rows);
    let drawing: Vec<usize> =
        (0..groups.len()).filter(|&gi| group_draws(ucfx, data_off, &rows, &groups[gi])).collect();
    let mut stats = PartsStats { fit_scale: 1.0, ..Default::default() };

    // ---- ONE global fit for the whole model: union bbox of every part -> the template
    // model-space AABB (header INFO +0x04/+0x10). All parts MUST share one transform or they
    // fall apart relative to each other. X/Z centred, Y bottom-aligned (gear on the ground).
    let (mut umin, mut umax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in parts {
        for v in &p.mesh.positions {
            for k in 0..3 {
                umin[k] = umin[k].min(v[k]);
                umax[k] = umax[k].max(v[k]);
            }
        }
    }
    if umin[0] > umax[0] {
        return Err("parts have no geometry".into());
    }
    // Robust extents for the SCALE only (see `fit_percentile`): trim the tails per axis so a thin
    // antenna cannot dictate the uniform scale. `umin/umax` (true extents) still drive placement.
    let (mut smin, mut smax) = (umin, umax);
    if fit_percentile < 100.0 {
        let q = (fit_percentile.clamp(50.0, 100.0) / 100.0) as f64;
        for k in 0..3 {
            let mut vals: Vec<f32> =
                parts.iter().flat_map(|p| p.mesh.positions.iter().map(move |v| v[k])).collect();
            if vals.len() < 8 {
                continue;
            }
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = vals.len() as f64;
            let lo = (((1.0 - q) * n) as usize).min(vals.len() - 1);
            let hi = ((q * n) as usize).min(vals.len() - 1);
            smin[k] = vals[lo];
            smax[k] = vals[hi];
        }
    }
    let t = leaf(ucfx, data_off, &rows[0]);
    if t.len() < 28 {
        return Err("template header INFO too small for an AABB".into());
    }
    let rf = |o: usize| f32::from_bits(read_u32_le(t, o));
    let (tmin, tmax) = ([rf(4), rf(8), rf(12)], [rf(16), rf(20), rf(24)]);
    let mut s = f32::MAX;
    for k in 0..3 {
        let d = smax[k] - smin[k];
        if d > 1e-4 {
            s = s.min((tmax[k] - tmin[k]).abs() / d);
        }
    }
    if !s.is_finite() || s <= 0.0 {
        s = 1.0;
    }
    s *= if scale_mult > 0.0 { scale_mult } else { 1.0 };
    // X/Z centred on the template; Y BOTTOM-aligned to the ground plane.
    //
    // ★Ground = y 0, NOT the template AABB's min-y. A template's own min-y is wherever ITS lowest
    // geometry happens to sit (the Hind reads 0.17 m), so bottom-aligning to it left our gear
    // hovering that far up — in-game, a visible ~6-inch float. Vehicles rest on y = 0; put our
    // lowest vertex there. `y_offset` trims from that (negative sinks it in).
    // ★X/Z target = the template's ORIGIN (0,0), NOT its AABB centre. A vehicle's whole rig — turret
    // node, barrel node, seats, the physics hull — is built around the model ORIGIN, i.e. the
    // centreline. The AABB centre is NOT the centreline: one protruding fitting skews it (the ztz98's
    // box spans X -2.184..+2.520, so its box centre is +0.168 while every rig node sits at X=0).
    // Centring our tank on +0.168 parked its body a whole 17 cm off the axis the turret rotates about
    // — the body sat to one side of its own turret. Align the centreline to the centreline.
    let ucen = [(umin[0] + umax[0]) * 0.5, umin[1], (umin[2] + umax[2]) * 0.5];
    let tgt = [0.0, y_offset, 0.0];
    let _ = (tmin[0] + tmax[0]) * 0.5;
    let _ = (tmin[2] + tmax[2]) * 0.5;
    let fit = |p: [f32; 3]| {
        [
            (p[0] - ucen[0]) * s + tgt[0],
            (p[1] - ucen[1]) * s + tgt[1],
            (p[2] - ucen[2]) * s + tgt[2],
        ]
    };
    stats.fit_scale = s;

    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in parts {
        for v in &p.mesh.positions {
            let w = fit(*v);
            for k in 0..3 {
                bmin[k] = bmin[k].min(w[k]);
                bmax[k] = bmax[k].max(w[k]);
            }
        }
    }
    stats.bbox_min = bmin;
    stats.bbox_max = bmax;

    // HIER world-rest matrices: needed to push a node-bound part into its node LOCAL space.
    let wrapped = wrap_block(ucfx, new_name_hash);
    let skel = crate::skeleton::Skeleton::from_block(&wrapped).ok();
    let meshes = crate::model_cubeize::read_model_meshes(ucfx).unwrap_or_default();
    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();
    let mut segm_body: Option<Vec<u8>> = None;
    let segm_row = rows.iter().position(|r| &r.tag == b"SEGM" && r.u0 != 0xFFFF_FFFF);

    // ★PHY2 — scale the donor's COLLISION HULLS to match our model.
    //
    // The donor's Havok convex hulls are sized for the DONOR. Conform a model at a different scale
    // and the vehicle you SEE and the volume bullets/impacts HIT disagree (a 2x tank keeps a
    // half-size hit box). The fit scale is not the right number here — the donor's own geometry
    // already fills its own hull — so we scale by how much BIGGER we made the model than the donor,
    // i.e. `scale_mult` (1.0 = donor-sized = leave collision alone).
    if (scale_mult - 1.0).abs() > 1e-4 {
        if let Some(pi) = rows.iter().position(|r| &r.tag == b"PHY2" && r.u0 != 0xFFFF_FFFF) {
            let mut ph = leaf(ucfx, data_off, &rows[pi]).to_vec();
            match crate::havok::scale_phy2_hulls(&mut ph, scale_mult) {
                Ok(n) => {
                    stats.phy2_hulls_scaled = n;
                    new_bodies.insert(pi, ph);
                }
                Err(e) => return Err(format!("PHY2 scale: {e}")),
            }
        }
    }

    // ★NODE RETARGET — move the donor's RIG onto OUR model, not our model onto the donor's rig.
    //
    // A donor's turret/barrel nodes sit wherever ITS turret and barrel were. A novel tank's are
    // somewhere else, and the old fix (PartSpec::recenter_xz) slid our GEOMETRY sideways onto the
    // donor's node — which displaces the turret off the hull it is supposed to sit on. Wrong way
    // round. Here we instead rewrite the HIER node's LOCAL matrix so the node lands on our part's
    // real axis, leaving the geometry where it was authored.
    //
    // Retargeting a node rigidly carries its whole SUBTREE (moving the turret must move the barrel
    // with it), then a later, deeper retarget re-places the child precisely. HIER guarantees
    // parent < child, so applying in index order gives parents-before-children for free.
    let mut worlds: Vec<[[f32; 4]; 4]> =
        skel.as_ref().map(|s| s.bones.iter().map(|b| b.world).collect()).unwrap_or_default();

    // ★SCALE THE RIG WITH THE MODEL. Scaling the geometry (and PHY2) but not the HIER leaves every
    // hardpoint at DONOR scale: the seat, the exhaust points, the wheel points all stay where the
    // donor's were, so on a 2x tank the seat ends up buried inside a hull twice the size and the
    // vehicle becomes impossible to get into.
    //
    // Scaling every node's LOCAL translation by `s` is exactly equivalent to scaling every node's
    // WORLD translation by `s` (rotations are untouched, and the parent chain composes:
    // world_t(i) = s*t_local(i)·R_parent + world_t(parent), so by induction every world translation
    // scales by s). So just scale the world translations here, before any --node-at retarget (whose
    // coordinates are already given in final, post-scale model space).
    if (scale_mult - 1.0).abs() > 1e-4 {
        for w in worlds.iter_mut() {
            for k in 0..3 {
                w[3][k] *= scale_mult;
            }
        }
        stats.rig_scaled = true;
    }

    if !node_ats.is_empty() || stats.rig_scaled {
        if let Some(s) = skel.as_ref() {
            let n = s.bones.len();
            let parent: Vec<i32> = s.bones.iter().map(|b| b.parent).collect();
            let mut ats: Vec<(usize, [f32; 3])> = node_ats.to_vec();
            ats.sort_by_key(|(r, _)| *r);
            let _ = &parent;
            for (r, want) in ats {
                if r >= n {
                    return Err(format!("--node-at: node {r} not in HIER ({n} nodes)"));
                }
                let d = [
                    want[0] - worlds[r][3][0],
                    want[1] - worlds[r][3][1],
                    want[2] - worlds[r][3][2],
                ];
                for i in r..n {
                    // i is in r's subtree iff walking parents from i reaches r.
                    let mut p = i as i32;
                    while p >= 0 {
                        if p as usize == r {
                            for k in 0..3 {
                                worlds[i][3][k] += d[k];
                            }
                            break;
                        }
                        p = parent[p as usize];
                    }
                }
                stats.nodes_moved.push((r, want));
            }
            // Re-derive each LOCAL from the new worlds: local = world @ inverse(world_parent).
            if let Some(hi) = rows.iter().position(|rw| &rw.tag == b"HIER" && rw.u0 != 0xFFFF_FFFF) {
                let mut h = leaf(ucfx, data_off, &rows[hi]).to_vec();
                for i in 0..n {
                    let o = i * crate::skeleton::HIER_NODE_STRIDE + 16;
                    if o + 64 > h.len() {
                        break;
                    }
                    let p = parent[i];
                    let local = if p < 0 {
                        worlds[i]
                    } else {
                        crate::skeleton::mat4_mul(
                            &worlds[i],
                            &crate::skeleton::affine_inverse(&worlds[p as usize]),
                        )
                    };
                    for (rr, row) in local.iter().enumerate() {
                        for (cc, v) in row.iter().enumerate() {
                            let off = o + (rr * 4 + cc) * 4;
                            h[off..off + 4].copy_from_slice(&v.to_le_bytes());
                        }
                    }
                }
                new_bodies.insert(hi, h);
            }
        }
    }

    for spec in parts {
        let g = groups.get(spec.group).ok_or_else(|| format!("group {} out of range", spec.group))?;

        // Fit to model space, then (if node-bound) into that node LOCAL space, because a rigid
        // MESH sub-object is authored bone-local and the engine multiplies it by node.world.
        let mut m = spec.mesh.clone();
        // Use the RETARGETED world (see the node-retarget pass above), not the donor's original —
        // the geometry must be expressed relative to where the node now IS.
        let node_inv = if spec.node >= 0 {
            let w = worlds
                .get(spec.node as usize)
                .ok_or_else(|| format!("node {} not in HIER", spec.node))?;
            Some(crate::skeleton::affine_inverse(w))
        } else {
            None
        };
        // A spinning part must straddle its node's axis or it ORBITS the node instead of rotating
        // about itself. Shift the part's X/Z bbox centre onto the node origin (Y untouched).
        let mut shift = [0.0f32, 0.0, 0.0];
        if spec.recenter_xz && spec.node >= 0 {
            let (mut pmn, mut pmx) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &m.positions {
                let w = fit(*v);
                for k in 0..3 {
                    pmn[k] = pmn[k].min(w[k]);
                    pmx[k] = pmx[k].max(w[k]);
                }
            }
            if let Some(b) = skel.as_ref().and_then(|s| s.bones.get(spec.node as usize)) {
                shift[0] = b.world[3][0] - (pmn[0] + pmx[0]) * 0.5;
                shift[2] = b.world[3][2] - (pmn[2] + pmx[2]) * 0.5;
            }
        }

        for i in 0..m.positions.len() {
            let f = fit(m.positions[i]);
            let w = [f[0] + shift[0], f[1] + shift[1], f[2] + shift[2]];
            m.positions[i] = match &node_inv {
                Some(inv) => crate::skeleton::transform_point(inv, w),
                None => w,
            };
            if let Some(inv) = &node_inv {
                let n = crate::skeleton::transform_dir(inv, m.normals[i]);
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-8);
                m.normals[i] = [n[0] / l, n[1] / l, n[2] / l];
            }
        }

        let flipped: Vec<[u32; 3]>;
        let tris: &[[u32; 3]] = if flip_winding {
            flipped = m.tris.iter().map(|&[a, b, c]| [a, c, b]).collect();
            &flipped
        } else {
            &m.tris
        };
        let strip = to_strip(tris);
        if m.positions.len() > 65534 {
            return Err(format!("{}: {} verts exceeds u16", spec.label, m.positions.len()));
        }
        if strip.len() > 65534 {
            return Err(format!(
                "{}: strip {} exceeds u16 - lower this part triangle budget",
                spec.label,
                strip.len()
            ));
        }
        let tans = synth_tangents(&m);
        let (vc, ic) = (m.positions.len() as u32, strip.len() as u32);
        let mut ib = Vec::with_capacity(strip.len() * 2);
        for &x in &strip {
            ib.extend_from_slice(&(x as u16).to_le_bytes());
        }

        let stride = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 4) as usize;
        let decl = parse_decl(leaf(ucfx, data_off, &rows[g.strm_decl]));
        let vb = encode_strm_from_decl(&m, &tans, &decl, stride);
        let f0 = read_u32_le(leaf(ucfx, data_off, &rows[g.strm_info]), 0);
        let mut si = Vec::new();
        si.extend_from_slice(&f0.to_le_bytes());
        si.extend_from_slice(&(stride as u32).to_le_bytes());
        si.extend_from_slice(&vc.to_le_bytes());
        new_bodies.insert(g.strm_info, si);
        new_bodies.insert(g.strm_data, vb);
        new_bodies.insert(g.ibuf_info, ic.to_le_bytes().to_vec());
        new_bodies.insert(g.ibuf_data, ib);

        // ★AREA: one f16 per STRIP TRIANGLE (count = index_count - 2) holding that triangle's
        // surface area, 0.0 for the degenerate stitch triangles. It is indexed in lockstep with the
        // index buffer, so replacing the geometry without rebuilding it leaves an array that
        // describes the DONOR's mesh and is the wrong LENGTH (the ztz98 hull ships 402 entries; our
        // hull has 62,995 triangles).
        if let (Some(ai), Some(ad)) = (g.area_info, g.area_data) {
            let n_prim = strip.len().saturating_sub(2);
            let mut area = Vec::with_capacity(n_prim * 2);
            for w in 0..n_prim {
                let (i0, i1, i2) =
                    (strip[w] as usize, strip[w + 1] as usize, strip[w + 2] as usize);
                // A degenerate (repeated-index) stitch triangle has zero area — and the donor
                // stores exactly 0.0 for those.
                let a = if i0 == i1 || i1 == i2 || i0 == i2 {
                    0.0f32
                } else {
                    let (p0, p1, p2) = (m.positions[i0], m.positions[i1], m.positions[i2]);
                    let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                    let c = [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ];
                    0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
                };
                area.extend_from_slice(&f16_le(a));
            }
            new_bodies.insert(ai, (n_prim as u32).to_le_bytes().to_vec());
            new_bodies.insert(ad, area);
        }

        // PRMT: word 0 = the MTRL index (this is what gives the part its own skin).
        let prmt_old = leaf(ucfx, data_off, &rows[g.prmt]);
        let nrec = (prmt_old.len() / 16).max(1);
        let mut rec = Vec::with_capacity(16);
        rec.extend_from_slice(&spec.material_index.to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&(ic - 2).to_le_bytes());
        rec.extend_from_slice(&((vc - 1) as u16).to_le_bytes());
        rec.extend_from_slice(&(vc as u16).to_le_bytes());
        let mut pb = Vec::with_capacity(nrec * 16);
        pb.extend_from_slice(&rec);
        for _ in 1..nrec {
            pb.extend_from_slice(&[0u8; 16]); // extra sub-strips draw nothing
        }
        new_bodies.insert(g.prmt, pb);

        let (mut pmin, mut pmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &m.positions {
            for k in 0..3 {
                pmin[k] = pmin[k].min(v[k]);
                pmax[k] = pmax[k].max(v[k]);
            }
        }
        if let Some(pir) =
            (0..g.strm_info).rev().find(|&i| &rows[i].tag == b"INFO" && rows[i].u0 != 0xFFFF_FFFF)
        {
            let mut pi = leaf(ucfx, data_off, &rows[pir]).to_vec();
            if pi.len() >= 60 {
                let cen = [
                    (pmin[0] + pmax[0]) * 0.5,
                    (pmin[1] + pmax[1]) * 0.5,
                    (pmin[2] + pmax[2]) * 0.5,
                ];
                let (dx, dy, dz) = (
                    (pmax[0] - pmin[0]) * 0.5,
                    (pmax[1] - pmin[1]) * 0.5,
                    (pmax[2] - pmin[2]) * 0.5,
                );
                let rad = (dx * dx + dy * dy + dz * dz).sqrt();
                for k in 0..3 {
                    pi[20 + k * 4..24 + k * 4].copy_from_slice(&cen[k].to_le_bytes());
                    pi[36 + k * 4..40 + k * 4].copy_from_slice(&pmin[k].to_le_bytes());
                    pi[48 + k * 4..52 + k * 4].copy_from_slice(&pmax[k].to_le_bytes());
                }
                pi[32..36].copy_from_slice(&rad.to_le_bytes());
                new_bodies.insert(pir, pi);
            }
        }

        // SEGM: bind this group segment to the requested node, at EVERY LOD tier.
        // Reached: group -> parent sub-object -> INDX[sub_object] = seg_id -> SEGM[seg_id].
        let seg_id = meshes
            .iter()
            .find(|mm| mm.group_index == spec.group)
            .map(|mm| mm.seg_id)
            .ok_or_else(|| format!("group {} has no INDX/SEGM binding", spec.group))?;
        let body = segm_body.get_or_insert_with(|| {
            segm_row.map(|sr| leaf(ucfx, data_off, &rows[sr]).to_vec()).unwrap_or_default()
        });
        let o = seg_id * 4;
        if o + 4 > body.len() {
            return Err(format!("seg_id {seg_id} outside SEGM"));
        }
        body[o..o + 2].copy_from_slice(&(spec.node as i16).to_le_bytes());
        body[o + 2] = seg_id as u8; // SEGM[i].seg_id == i invariant
        body[o + 3] = 0x7F; // present at every LOD tier

        stats.parts.push(PartStat {
            label: spec.label.clone(),
            group: spec.group,
            node: spec.node,
            material_index: spec.material_index,
            seg_id,
            vertex_count: vc as usize,
            triangle_count: m.tris.len(),
        });
    }
    if let (Some(sr), Some(b)) = (segm_row, segm_body) {
        new_bodies.insert(sr, b);
    }

    // ★Every group we did NOT host geometry in must draw NOTHING — and that means EMPTYING IT, not
    // just zeroing one PRMT word.
    //
    // Zeroing the PRMT primitive count alone leaves the donor's vertex + index buffers fully
    // populated (the ztz98 keeps 2,327 verts / 7,873 indices in group 10 that way). Those groups
    // hold the donor's own armour panels and destruction break-pieces, and if ANY path into the
    // renderer reads the geometry through a field we did not zero, the donor's plates get drawn
    // straight through our model — flat metal shards interpenetrating the hull. Empty the vertex
    // buffer, the index buffer, the AREA array AND the PRMT records, so there is nothing left to
    // draw whichever field the engine trusts.
    let hosted: Vec<usize> = parts.iter().map(|p| p.group).collect();
    for &gi in &drawing {
        if hosted.contains(&gi) {
            continue;
        }
        // ★DO NOT empty the buffers to zero size — the engine cannot take a drawing group with a
        // zero-size vertex buffer and dies binding it (AV at 0x0085C8D0; this is the same
        // "zero-size vertex-buffer crash" that `wad_builder unwrap-mesh` exists to fix).
        //
        // Instead keep every buffer at its ORIGINAL length and COLLAPSE all vertex positions to the
        // origin: every triangle becomes degenerate, so the group rasterises nothing, while the
        // vertex/index buffers stay valid to bind. Belt-and-braces with zeroing the PRMT primitive
        // count, so the donor's spare wreck body + break-piece armour panels cannot surface however
        // the engine reaches them.
        let pg = &groups[gi];
        let stride = read_u32_le(leaf(ucfx, data_off, &rows[pg.strm_info]), 4) as usize;
        let vc = read_u32_le(leaf(ucfx, data_off, &rows[pg.strm_info]), 8) as usize;
        let decl = parse_decl(leaf(ucfx, data_off, &rows[pg.strm_decl]));
        let pos_off = decl.iter().find(|e| e.usage == 0).map(|e| e.offset);
        let mut vb = leaf(ucfx, data_off, &rows[pg.strm_data]).to_vec();
        if let Some(po) = pos_off {
            for v in 0..vc {
                let o = v * stride + po;
                // POSITION is FLOAT16_4 (x,y,z,w) — zero x/y/z, leave w.
                if o + 6 <= vb.len() {
                    for b in vb[o..o + 6].iter_mut() {
                        *b = 0;
                    }
                }
            }
        }
        new_bodies.insert(pg.strm_data, vb);
        let mut p = leaf(ucfx, data_off, &rows[pg.prmt]).to_vec();
        for r in 0..p.len() / 16 {
            p[r * 16 + 8..r * 16 + 12].copy_from_slice(&0u32.to_le_bytes());
        }
        new_bodies.insert(pg.prmt, p);
        stats.emptied_groups.push(gi);
    }

    // REPLACE a material IN PLACE: overwrite record `dst` with a clone of record `src` (a new
    // diffuse, but keeping `dst`'s own NAME HASH at +0x00). Unlike `mtrl_adds` this keeps the
    // material COUNT unchanged, which matters: the ztz98's unused materials 0/1/2 are an untextured
    // shader variant (flags 0x0000 / tex_count 2), so converting one into a copy of a known-good
    // textured material is how we get an extra usable skin slot WITHOUT growing the record set.
    if !mtrl_replaces.is_empty() {
        if let Some(mi) = rows.iter().position(|r| &r.tag == b"MTRL" && r.u0 != 0xFFFF_FFFF) {
            let m = new_bodies
                .get(&mi)
                .cloned()
                .unwrap_or_else(|| leaf(ucfx, data_off, &rows[mi]).to_vec());
            // Split into records first: a replacement can change a record's stride (124 -> 128),
            // so rebuild the chunk from the record list rather than patching in place.
            let mut recs: Vec<Vec<u8>> = Vec::new();
            let mut o = 0usize;
            while o + 112 <= m.len() {
                let texc = u16::from_le_bytes([m[o + 106], m[o + 107]]) as usize;
                if !(1..=10).contains(&texc) {
                    break;
                }
                let stride = 116 + texc * 4;
                if o + stride > m.len() {
                    break;
                }
                recs.push(m[o..o + stride].to_vec());
                o += stride;
            }
            for &(dst, src, tex) in mtrl_replaces {
                if dst >= recs.len() || src >= recs.len() {
                    continue;
                }
                let keep_hash = recs[dst][0..4].to_vec();
                let mut rec = recs[src].clone();
                rec[0..4].copy_from_slice(&keep_hash);
                rec[108..112].copy_from_slice(&tex.to_le_bytes());
                recs[dst] = rec;
                stats.mtrl_repoints += 1;
            }
            new_bodies.insert(mi, recs.concat());
        }
    }

    // APPEND cloned materials first, so --set-mtrl indices can refer to them.
    if !mtrl_adds.is_empty() {
        if let Some(mi) = rows.iter().position(|r| &r.tag == b"MTRL" && r.u0 != 0xFFFF_FFFF) {
            let mut m = new_bodies
                .get(&mi)
                .cloned()
                .unwrap_or_else(|| leaf(ucfx, data_off, &rows[mi]).to_vec());
            // Index the existing records (offset, stride).
            let mut recs: Vec<(usize, usize)> = Vec::new();
            let mut o = 0usize;
            while o + 112 <= m.len() {
                let texc = u16::from_le_bytes([m[o + 106], m[o + 107]]) as usize;
                if !(1..=10).contains(&texc) {
                    break;
                }
                let stride = 116 + texc * 4;
                recs.push((o, stride));
                o += stride;
            }
            for &(src, tex) in mtrl_adds {
                let Some(&(so, stride)) = recs.get(src) else { continue };
                let mut rec = m[so..so + stride].to_vec();
                rec[108..112].copy_from_slice(&tex.to_le_bytes());
                // ★A material record's first u32 is its NAME HASH, and the engine registers
                // materials into the shader registry by that hash, FIRST-WINS. A verbatim clone
                // therefore keeps the source's hash, loses the race, and never gets a registry
                // slot -- so at draw time `shader_table[mtrl_idx]` is NULL and the renderer faults
                // dereferencing it (+0x182) in FUN_00855420. Give the copy its own hash.
                let name = format!("mtrl_clone_{src}_{tex:08x}");
                rec[0..4].copy_from_slice(&crate::hash::pandemic_hash_m2(&name).to_le_bytes());
                let no = m.len();
                m.extend_from_slice(&rec);
                recs.push((no, stride));
                stats.mtrl_repoints += 1;
            }
            new_bodies.insert(mi, m);
        }
    }

    // MTRL diffuse BY MATERIAL INDEX. Record stride = 116 + tex_count*4; flags@104, tex_count@106,
    // texture hashes from @108 (diffuse = the first). Walk records and rewrite the requested ones.
    if !mtrl_sets.is_empty() {
        if let Some(mi) = rows.iter().position(|r| &r.tag == b"MTRL" && r.u0 != 0xFFFF_FFFF) {
            let mut m = new_bodies
                .get(&mi)
                .cloned()
                .unwrap_or_else(|| leaf(ucfx, data_off, &rows[mi]).to_vec());
            let mut o = 0usize;
            let mut idx = 0usize;
            while o + 112 <= m.len() {
                let texc = u16::from_le_bytes([m[o + 106], m[o + 107]]) as usize;
                if !(1..=10).contains(&texc) {
                    break;
                }
                // ★slot 0 = diffuse (`_dm`), slot 1 = specular (`_sm`), slot 2 = normal (`_nm`,
                // DXT5nm) — verified 2026-07-19 by decoding a donor MTRL and hash-matching its slot
                // hashes to `<name>_sm`/`<name>_nm` (matches MtrlMaterial::specular; the earlier
                // "slot 1 = normal" note here was wrong). Writing only the diffuse leaves the DONOR's
                // normal map bound, and the shader then samples its normals through OUR UV layout ->
                // garbage
                // per-pixel normals -> flat armour renders as CRUMPLED, creased, blotchy metal.
                // Every slot a part uses must be repointed, not just slot 0.
                for &(i, slot, tex) in mtrl_sets.iter().filter(|(i, _, _)| *i == idx) {
                    let _ = i;
                    let so = o + 108 + slot * 4;
                    if slot < texc && so + 4 <= m.len() {
                        m[so..so + 4].copy_from_slice(&tex.to_le_bytes());
                        stats.mtrl_repoints += 1;
                    }
                }
                o += 116 + texc * 4;
                idx += 1;
            }
            new_bodies.insert(mi, m);
        }
    }

    // MTRL texture repoints (give each hosted material our skin).
    if !repoints.is_empty() {
        for (i, r) in rows.iter().enumerate() {
            if &r.tag != b"MTRL" || r.u0 == 0xFFFF_FFFF {
                continue;
            }
            let mut m = leaf(ucfx, data_off, r).to_vec();
            let mut n = 0usize;
            let mut o = 0usize;
            while o + 4 <= m.len() {
                let v = read_u32_le(&m, o);
                if let Some(&(_, to)) = repoints.iter().find(|(from, _)| *from == v) {
                    m[o..o + 4].copy_from_slice(&to.to_le_bytes());
                    n += 1;
                }
                o += 4;
            }
            if n > 0 {
                stats.mtrl_repoints += n;
                new_bodies.insert(i, m);
            }
        }
    }

    let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
    if top.len() >= 28 {
        for k in 0..3 {
            top[4 + k * 4..8 + k * 4].copy_from_slice(&bmin[k].to_le_bytes());
            top[16 + k * 4..20 + k * 4].copy_from_slice(&bmax[k].to_le_bytes());
        }
    }
    new_bodies.insert(0, top);

    // Reassemble + CSUM.
    let mut new_data: Vec<u8> = Vec::new();
    for (idx, r) in rows.iter_mut().enumerate() {
        if r.u0 == 0xFFFF_FFFF {
            continue;
        }
        let body = match new_bodies.get(&idx) {
            Some(b) => b.clone(),
            None => leaf(ucfx, data_off, r).to_vec(),
        };
        r.u0 = new_data.len() as u32;
        r.size = body.len() as u32;
        new_data.extend_from_slice(&body);
    }
    let new_data_off = (20 + ndesc * 20) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&new_data_off.to_le_bytes());
    out.extend_from_slice(&ucfx[8..16]);
    out.extend_from_slice(&(ndesc as u32).to_le_bytes());
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&r.u0.to_le_bytes());
        out.extend_from_slice(&r.size.to_le_bytes());
        out.extend_from_slice(&r.u2.to_le_bytes());
        out.extend_from_slice(&r.u3.to_le_bytes());
    }
    out.extend_from_slice(&new_data);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    // The zero-size-buffer gate now also guards the multi-part path (it only ran on the other
    // injectors, so nothing caught the crash this introduced).
    assert_no_empty_drawing_group(&wrap_block(&out, new_name_hash))
        .map_err(|e| format!("post-build drawing-group gate FAILED: {e}"))?;
    Ok((out, stats))
}

/// A novel leaf HIER node to author into a donor container (see `append_new_bones`).
pub struct NewBone {
    /// `pandemic_hash_m2(bone_name)` — name the machine/spin binds to (e.g. `bone_rotor`), or a
    /// fresh unique hash for a purely structural attachment node.
    pub name_hash: u32,
    /// Parent HIER node index. May be an existing donor node OR an earlier bone in this same call
    /// (its index = old_node_count + its position in the slice), so a small novel sub-tree is legal.
    pub parent: usize,
    /// World-rest position (identity rotation). The bone attaches its part here; a part bound to it
    /// draws in this node's LOCAL space and rides the node's motion (parent it under the donor's
    /// engine-spun rotor node and it spins).
    pub world_pos: [f32; 3],
    /// The bound part's AABB in this node's LOCAL space (min, max) — written into the node's cull
    /// box (@0x90/@0xA0). Leave a real box or the engine may cull the part. `([0;3],[0;3])` = a
    /// degenerate box (safe only for a node that hosts no geometry).
    pub local_aabb: ([f32; 3], [f32; 3]),
}

/// Author novel **leaf** HIER nodes into a raw UCFX model container — the primitive behind a
/// "fresh skeleton" import (novel bones at each part's position) that does NOT overwrite an existing
/// vehicle. Each new node is APPENDED (so every existing node index — and the hash-addressed
/// destruction/spin machine that binds to it — stays valid) and PREPENDED into its parent's
/// first-child / next-sibling list (so subtree traversal, SHOW/HIDE and culling still reach it).
///
/// Record layout authored (176-byte stride, verified against the retail Mi-26 container):
/// `@0` name hash · `@4` `first_child<<16 | 1` (0xFFFF = leaf) · `@8` parent u16 · `@10` next
/// sibling u16 · `@16` LOCAL 4×4 (row-major, translation row 3) · `@80` inverse-bind (= inv(world))
/// · `@144` min+1.0 · `@160` max+1.0. The header INFO node count (`@0x20`) is bumped.
///
/// Returns the new raw UCFX (`UCFX`..`CSUM`, same form `inject_parts_into_template` consumes) and
/// the assigned node indices (aligned to `bones`). Bones parent-before-child is the caller's job
/// (a bone may only parent an EARLIER slice entry).
pub fn append_new_bones(ucfx: &[u8], bones: &[NewBone]) -> Result<(Vec<u8>, Vec<usize>), String> {
    if ucfx.len() < 20 || &ucfx[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    if bones.is_empty() {
        return Ok((ucfx.to_vec(), Vec::new()));
    }
    let (data_off, ndesc, mut rows) = parse_rows(ucfx);
    let hi = rows
        .iter()
        .position(|r| &r.tag == b"HIER" && r.u0 != 0xFFFF_FFFF)
        .ok_or("no HIER chunk")?;
    let stride = crate::skeleton::HIER_NODE_STRIDE; // 176
    let mut hier = leaf(ucfx, data_off, &rows[hi]).to_vec();
    let old_n = hier.len() / stride;

    // World-rest matrices of the EXISTING nodes (needed to derive each new node's LOCAL from its
    // parent's world). New nodes' worlds are identity-rotation translations we set below.
    let skel = crate::skeleton::Skeleton::from_block(&wrap_block(ucfx, 0))
        .map_err(|e| format!("HIER did not parse for world-rest derivation: {e}"))?;
    let mut worlds: Vec<[[f32; 4]; 4]> = skel.bones.iter().map(|b| b.world).collect();
    if worlds.len() != old_n {
        return Err(format!("HIER node count mismatch: {} bytes vs {} parsed", old_n, worlds.len()));
    }

    let write_mat = |buf: &mut [u8], o: usize, m: &[[f32; 4]; 4]| {
        for r in 0..4 {
            for c in 0..4 {
                let off = o + (r * 4 + c) * 4;
                buf[off..off + 4].copy_from_slice(&m[r][c].to_le_bytes());
            }
        }
    };

    let mut indices = Vec::with_capacity(bones.len());
    for (k, nb) in bones.iter().enumerate() {
        let ni = old_n + k;
        if nb.parent >= ni {
            return Err(format!(
                "new bone {k} parent {} must be an existing node or an earlier new bone (< {ni})",
                nb.parent
            ));
        }
        // world (identity rotation, translation = world_pos)
        let mut wm = [[0.0f32; 4]; 4];
        for i in 0..4 {
            wm[i][i] = 1.0;
        }
        wm[3][0] = nb.world_pos[0];
        wm[3][1] = nb.world_pos[1];
        wm[3][2] = nb.world_pos[2];
        worlds.push(wm);
        let pw = worlds[nb.parent];
        let local = crate::skeleton::mat4_mul(&wm, &crate::skeleton::affine_inverse(&pw));
        let inv_bind = crate::skeleton::affine_inverse(&wm);

        // append a fresh record
        let base = hier.len();
        hier.extend(std::iter::repeat(0u8).take(stride));
        let rec = &mut hier[base..base + stride];
        rec[0..4].copy_from_slice(&nb.name_hash.to_le_bytes());
        // @4: first_child = none (leaf) | flag 1
        rec[4..8].copy_from_slice(&((0xFFFFu32 << 16) | 0x0001).to_le_bytes());
        rec[8..10].copy_from_slice(&(nb.parent as u16).to_le_bytes());
        // @10 next_sibling is set below (prepend into parent's child list); @12/@14 stay 0.
        rec[10..12].copy_from_slice(&0xFFFFu16.to_le_bytes());
        write_mat(rec, 16, &local);
        write_mat(rec, 80, &inv_bind);
        let (mn, mx) = nb.local_aabb;
        for j in 0..3 {
            rec[144 + j * 4..148 + j * 4].copy_from_slice(&mn[j].to_le_bytes());
            rec[160 + j * 4..164 + j * 4].copy_from_slice(&mx[j].to_le_bytes());
        }
        rec[156..160].copy_from_slice(&1.0f32.to_le_bytes());
        rec[172..176].copy_from_slice(&1.0f32.to_le_bytes());

        // Prepend into parent's first-child / next-sibling list.
        let po = nb.parent * stride;
        let parent_first_child = u16::from_le_bytes([hier[po + 6], hier[po + 7]]);
        hier[base + 10..base + 12].copy_from_slice(&parent_first_child.to_le_bytes());
        hier[po + 6..po + 8].copy_from_slice(&(ni as u16).to_le_bytes());

        indices.push(ni);
    }

    // Bump header INFO node count (@0x20).
    let mut top = leaf(ucfx, data_off, &rows[0]).to_vec();
    if top.len() < 0x24 {
        return Err("header INFO too small for node count".into());
    }
    top[0x20..0x24].copy_from_slice(&((old_n + bones.len()) as u32).to_le_bytes());

    // Reassemble raw UCFX (reflow offsets + CSUM), same emission as inject_parts_into_template.
    let mut new_bodies: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();
    new_bodies.insert(hi, hier);
    new_bodies.insert(0, top);
    let mut new_data: Vec<u8> = Vec::new();
    for (idx, r) in rows.iter_mut().enumerate() {
        if r.u0 == 0xFFFF_FFFF {
            continue;
        }
        let body = match new_bodies.get(&idx) {
            Some(b) => b.clone(),
            None => leaf(ucfx, data_off, r).to_vec(),
        };
        r.u0 = new_data.len() as u32;
        r.size = body.len() as u32;
        new_data.extend_from_slice(&body);
    }
    let new_data_off = (20 + ndesc * 20) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&new_data_off.to_le_bytes());
    out.extend_from_slice(&ucfx[8..16]);
    out.extend_from_slice(&(ndesc as u32).to_le_bytes());
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&r.u0.to_le_bytes());
        out.extend_from_slice(&r.size.to_le_bytes());
        out.extend_from_slice(&r.u2.to_le_bytes());
        out.extend_from_slice(&r.u3.to_le_bytes());
    }
    out.extend_from_slice(&new_data);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    Ok((out, indices))
}

/// Author `n` extra drawing groups into a model container by CLONING the first `MESH` sub-object
/// under `GEOM`, so a fresh-skeleton import can give MORE parts their own bone than the donor's
/// resident block happens to ship (each group binds exactly one SEGM row → one bone). The clones are
/// appended at the end of `GEOM`'s subtree (GEOM is the last top-level chunk), `GEOM`'s subtree
/// row-count (`x3`) and sub-object count (its `INFO` leaf) are bumped, and one fresh `INDX` entry
/// (a spare `seg_id`, `SEGM[i].seg_id==i`) is appended per clone. The clones carry the donor mesh's
/// geometry as a valid placeholder — `inject_parts_into_template` overwrites each with a real part.
///
/// Descriptor layout (verified on the Mi-26 resident): `GEOM(marker,x3)[ INFO(count), INDX(u16×k),
/// MESH(marker,x3=14)[ INFO(1), PRMG(marker,x3=12)[ INFO, STRM{info,decl,data}, AREA{info,data},
/// IBUF{info,data}, PRMT ] ]… ]`. Container markers carry `u0=0xFFFFFFFF` and `u3 = subtree row
/// count`; only `GEOM`'s `x3` changes (its children's are byte-identical clones).
///
/// Returns the grown raw UCFX and the ordinals of the new drawing groups (in `find_groups` order,
/// which is `old_group_count .. old_group_count+n`).
pub fn append_draw_groups(ucfx: &[u8], n: usize) -> Result<(Vec<u8>, Vec<usize>), String> {
    if ucfx.len() < 20 || &ucfx[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    if n == 0 {
        let g = find_groups(&parse_rows(ucfx).2).len();
        return Ok((ucfx.to_vec(), (0..0).map(|_: usize| g).collect()));
    }
    let (data_off, ndesc, rows) = parse_rows(ucfx);
    let is_marker = |r: &Row| r.u0 == 0xFFFF_FFFF;

    // Locate GEOM (top-level marker) and, inside it, its INFO (sub-object count) + INDX + first MESH.
    let geom = (0..ndesc)
        .find(|&i| &rows[i].tag == b"GEOM" && is_marker(&rows[i]))
        .ok_or("no GEOM chunk")?;
    let geom_end = (geom + 1 + rows[geom].u3 as usize).min(ndesc);
    let mut geom_info: Option<usize> = None;
    let mut indx: Option<usize> = None;
    let mut mesh_start: Option<usize> = None;
    {
        // Walk GEOM's DIRECT children (skip nested subtrees via x3).
        let mut r = geom + 1;
        while r < geom_end {
            let row = &rows[r];
            if is_marker(row) {
                if &row.tag == b"MESH" && mesh_start.is_none() {
                    mesh_start = Some(r);
                }
                r = (r + 1 + row.u3 as usize).min(geom_end);
            } else {
                if &row.tag == b"INFO" && geom_info.is_none() {
                    geom_info = Some(r);
                } else if &row.tag == b"INDX" && indx.is_none() {
                    indx = Some(r);
                }
                r += 1;
            }
        }
    }
    let geom_info = geom_info.ok_or("GEOM has no INFO (sub-object count)")?;
    let indx = indx.ok_or("GEOM has no INDX")?;
    let mesh_start = mesh_start.ok_or("GEOM has no MESH sub-object to clone")?;
    let mesh_len = 1 + rows[mesh_start].u3 as usize; // MESH marker + its subtree
    if mesh_start + mesh_len > geom_end {
        return Err("MESH subtree overruns GEOM".into());
    }

    // Free seg_ids: SEGM[i].seg_id==i, so any index not already named by INDX is a spare row we can
    // bind. (inject_parts_into_template rebinds SEGM[seg_id].bone for each hosted group.)
    let segm_row = rows
        .iter()
        .position(|r| &r.tag == b"SEGM" && !is_marker(r))
        .ok_or("no SEGM chunk")?;
    let segm = leaf(ucfx, data_off, &rows[segm_row]);
    let seg_count = segm.len() / 4;
    let old_indx = leaf(ucfx, data_off, &rows[indx]);
    let used: std::collections::HashSet<u16> =
        old_indx.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let mut free: Vec<u16> =
        (0..seg_count as u16).filter(|s| !used.contains(s)).collect();
    if free.len() < n {
        return Err(format!("only {} free SEGM rows, need {n}", free.len()));
    }
    let new_segs: Vec<u16> = free.drain(..n).collect();

    // ---- Build the output descriptor list (original rows + n cloned MESH subtrees appended at the
    // end of GEOM's subtree). Bodies: leaves copied from the donor (offsets reflow on emit). ----
    struct Out {
        tag: [u8; 4],
        marker: bool,
        size: u32,
        u2: u32,
        u3: u32,
        body: Option<Vec<u8>>,
    }
    let body_of = |i: usize| leaf(ucfx, data_off, &rows[i]).to_vec();
    let mut out: Vec<Out> = Vec::with_capacity(ndesc + n * mesh_len);

    // GEOM subtree ends at the tail of the descriptor table (GEOM is the last top-level chunk); we
    // append the clones right before geom_end. Emit rows [0, geom_end), then clones, then the tail.
    for i in 0..geom_end {
        let r = &rows[i];
        let marker = is_marker(r);
        let (mut u3, mut body) = (r.u3, if marker { None } else { Some(body_of(i)) });
        if i == geom {
            u3 = r.u3 + (n * mesh_len) as u32; // GEOM gains the cloned subtrees
        } else if i == geom_info {
            // sub-object count += n
            let old = read_u32_le(&body_of(i), 0);
            body = Some((old + n as u32).to_le_bytes().to_vec());
        } else if i == indx {
            let mut v = body_of(i);
            for &s in &new_segs {
                v.extend_from_slice(&s.to_le_bytes());
            }
            body = Some(v);
        }
        out.push(Out { tag: r.tag, marker, size: r.size, u2: r.u2, u3, body });
    }
    // n cloned MESH subtrees (byte-identical descriptors + donor geometry placeholder).
    for _ in 0..n {
        for j in mesh_start..(mesh_start + mesh_len) {
            let r = &rows[j];
            let marker = is_marker(r);
            out.push(Out {
                tag: r.tag,
                marker,
                size: r.size,
                u2: r.u2,
                u3: r.u3,
                body: if marker { None } else { Some(body_of(j)) },
            });
        }
    }
    // tail: rows after GEOM's subtree (usually none — GEOM is last).
    for i in geom_end..ndesc {
        let r = &rows[i];
        let marker = is_marker(r);
        out.push(Out {
            tag: r.tag,
            marker,
            size: r.size,
            u2: r.u2,
            u3: r.u3,
            body: if marker { None } else { Some(body_of(i)) },
        });
    }

    // ---- Emit ----
    let new_ndesc = out.len();
    let new_data_off = (20 + new_ndesc * 20) as u32;
    let mut data: Vec<u8> = Vec::new();
    let mut descs: Vec<(([u8; 4]), u32, u32, u32, u32)> = Vec::with_capacity(new_ndesc);
    for o in &out {
        if let Some(b) = &o.body {
            let off = data.len() as u32;
            descs.push((o.tag, off, b.len() as u32, o.u2, o.u3));
            data.extend_from_slice(b);
        } else {
            descs.push((o.tag, 0xFFFF_FFFF, o.size, o.u2, o.u3));
        }
    }
    let mut res = Vec::with_capacity(20 + new_ndesc * 20 + data.len() + 8);
    res.extend_from_slice(b"UCFX");
    res.extend_from_slice(&new_data_off.to_le_bytes());
    res.extend_from_slice(&ucfx[8..16]);
    res.extend_from_slice(&(new_ndesc as u32).to_le_bytes());
    for (tag, u0, size, u2, u3) in &descs {
        res.extend_from_slice(tag);
        res.extend_from_slice(&u0.to_le_bytes());
        res.extend_from_slice(&size.to_le_bytes());
        res.extend_from_slice(&u2.to_le_bytes());
        res.extend_from_slice(&u3.to_le_bytes());
    }
    res.extend_from_slice(&data);
    let csum = crc32_mercs2(&res);
    res.extend_from_slice(b"CSUM");
    res.extend_from_slice(&csum.to_le_bytes());

    let base_groups = find_groups(&rows).len();
    let new_ords: Vec<usize> = (base_groups..base_groups + n).collect();
    Ok((res, new_ords))
}

/// One logical part of a novel model to conform onto its OWN new bone (see `inject_fresh_skeleton`).
pub struct SkelPart {
    pub label: String,
    /// WORLD-space (pre-fit) geometry, exactly as authored in the source file.
    pub mesh: ExternalMesh,
    /// Bone name hash for this part's novel node. Use `pandemic_hash_m2("bone_rotor")` for the main
    /// rotor so the donor's spin command binds to it; a fresh unique hash for a plain attach node.
    pub bone_name_hash: u32,
    /// Existing donor HIER node this part's novel bone is parented UNDER — the "spin wiring". Parent
    /// a rotor part under the donor's engine-spun rotor node and it rides the spin; parent a static
    /// part under the intact-body node (`0x255EAB53`, index resolved by the caller) so destruction
    /// hides it with the body.
    pub parent_node: usize,
    /// MTRL record index this part draws with.
    pub material_index: u32,
}

/// Greedy triangle chunker: split a mesh into sub-meshes each safely under the u16 strip/vertex
/// limit, with a compact local vertex remap per chunk. `tri_cap` bounds triangles per chunk (a
/// conservative worst-case-strip bound); vertices are also capped at 60000.
fn split_mesh_u16(mesh: &ExternalMesh, tri_cap: usize) -> Vec<ExternalMesh> {
    let mut out = Vec::new();
    let mut ti = 0usize;
    while ti < mesh.tris.len() {
        let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        while ti < mesh.tris.len() && tris.len() < tri_cap && remap.len() < 60000 {
            let t = mesh.tris[ti];
            // If adding this triangle would blow the vertex cap, flush first.
            let new_v = t.iter().filter(|v| !remap.contains_key(v)).count();
            if remap.len() + new_v > 60000 && !tris.is_empty() {
                break;
            }
            let mut lt = [0u32; 3];
            for (j, &v) in t.iter().enumerate() {
                let idx = *remap.entry(v).or_insert_with(|| {
                    let n = positions.len() as u32;
                    positions.push(mesh.positions[v as usize]);
                    normals.push(mesh.normals[v as usize]);
                    uvs.push(mesh.uvs.get(v as usize).copied().unwrap_or([0.0, 0.0]));
                    n
                });
                lt[j] = idx;
            }
            tris.push(lt);
            ti += 1;
        }
        out.push(ExternalMesh { positions, normals, uvs, tris, joints: Vec::new(), weights: Vec::new() });
    }
    out
}

/// Conform a **novel multi-part model onto a FRESH SKELETON of novel bones** into a donor vehicle
/// container, minted under a NEW hash (does NOT overwrite the donor). Each `SkelPart` gets its own
/// authored HIER node at that part's fitted position (`append_new_bones`), parented under a donor
/// node for its articulation ("spin wiring"); large parts are auto-split across as many donor
/// drawing groups as needed to respect the u16 strip limit; every donor drawing group not hosting a
/// part is neutralised. Because the result ships under a new hash as a single block, it has no
/// streamed finer LOD rungs of its own — so nothing supersedes our geometry up close.
///
/// `donor_block` is the wrapped single-entry block. Returns `(new wrapped block, report)`.
pub fn inject_fresh_skeleton(
    donor_block: &[u8],
    parts: &[SkelPart],
    // Point a donor MTRL record's texture slot at a hash: `(record, slot, tex_hash)`, slot 0=diffuse.
    // This is how each part gets its OWN skin (its `material_index` selects the record edited here).
    mtrl_sets: &[(usize, usize, u32)],
    new_name_hash: u32,
    scale_mult: f32,
    y_offset: f32,
    flip_winding: bool,
    fit_percentile: f32,
) -> Result<(Vec<u8>, String), String> {
    if donor_block.len() < 20 {
        return Err("donor block too small".into());
    }
    let ucfx_len = read_u32_le(donor_block, 16) as usize;
    let model_type = read_u32_le(donor_block, 8);
    if 20 + ucfx_len > donor_block.len() {
        return Err("donor block truncated".into());
    }
    let ucfx = &donor_block[20..20 + ucfx_len];
    if &ucfx[0..4] != b"UCFX" {
        return Err("donor payload is not UCFX".into());
    }
    if parts.is_empty() {
        return Err("no parts".into());
    }

    // ---- ONE global fit: union bbox of every part -> template model-space AABB (header @4/@16),
    // X/Z centred, Y bottom-aligned. MUST match inject_parts_into_template's fit exactly so the
    // bones we place agree with the geometry it will place. ----
    let (data_off, _ndesc, rows) = parse_rows(ucfx);
    let hdr = leaf(ucfx, data_off, &rows[0]);
    if hdr.len() < 28 {
        return Err("template header INFO too small".into());
    }
    let rf = |o: usize| f32::from_bits(read_u32_le(hdr, o));
    let (tmin, tmax) = ([rf(4), rf(8), rf(12)], [rf(16), rf(20), rf(24)]);
    let (mut umin, mut umax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in parts {
        for v in &p.mesh.positions {
            for k in 0..3 {
                umin[k] = umin[k].min(v[k]);
                umax[k] = umax[k].max(v[k]);
            }
        }
    }
    if umin[0] > umax[0] {
        return Err("parts have no geometry".into());
    }
    let (mut smin, mut smax) = (umin, umax);
    if fit_percentile < 100.0 {
        let q = (fit_percentile.clamp(50.0, 100.0) / 100.0) as f64;
        for k in 0..3 {
            let mut vals: Vec<f32> =
                parts.iter().flat_map(|p| p.mesh.positions.iter().map(move |v| v[k])).collect();
            if vals.len() < 8 {
                continue;
            }
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = vals.len() as f64;
            smin[k] = vals[(((1.0 - q) * n) as usize).min(vals.len() - 1)];
            smax[k] = vals[((q * n) as usize).min(vals.len() - 1)];
        }
    }
    let mut s = f32::MAX;
    for k in 0..3 {
        let d = smax[k] - smin[k];
        if d > 1e-4 {
            s = s.min((tmax[k] - tmin[k]).abs() / d);
        }
    }
    if !s.is_finite() || s <= 0.0 {
        s = 1.0;
    }
    s *= if scale_mult > 0.0 { scale_mult } else { 1.0 };
    let ucen = [(umin[0] + umax[0]) * 0.5, umin[1], (umin[2] + umax[2]) * 0.5];
    let fit = |p: [f32; 3]| {
        [(p[0] - ucen[0]) * s, (p[1] - ucen[1]) * s + y_offset, (p[2] - ucen[2]) * s]
    };

    // ---- author one novel bone per part at its fitted centroid ----
    let mut bones: Vec<NewBone> = Vec::with_capacity(parts.len());
    for p in parts {
        let (mut pmn, mut pmx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &p.mesh.positions {
            let w = fit(*v);
            for k in 0..3 {
                pmn[k] = pmn[k].min(w[k]);
                pmx[k] = pmx[k].max(w[k]);
            }
        }
        let wp = [(pmn[0] + pmx[0]) * 0.5, (pmn[1] + pmx[1]) * 0.5, (pmn[2] + pmx[2]) * 0.5];
        // local AABB relative to the bone origin (identity rotation).
        let laabb = (
            [pmn[0] - wp[0], pmn[1] - wp[1], pmn[2] - wp[2]],
            [pmx[0] - wp[0], pmx[1] - wp[1], pmx[2] - wp[2]],
        );
        bones.push(NewBone {
            name_hash: p.bone_name_hash,
            parent: p.parent_node,
            world_pos: wp,
            local_aabb: laabb,
        });
    }
    // ---- split each part to u16 FIRST so we know how many host groups we need ----
    let part_subs: Vec<Vec<ExternalMesh>> =
        parts.iter().map(|p| split_mesh_u16(&p.mesh, 8000)).collect();
    let needed: usize = part_subs.iter().map(|s| s.len()).sum();

    // ---- grow the donor's draw-group pool if it can't host one group per sub-part (each group
    // binds exactly one bone, so per-part bones need one group per sub-part) ----
    let base_drawing = {
        let g = find_groups(&rows);
        (0..g.len()).filter(|&gi| group_draws(ucfx, data_off, &rows, &g[gi])).count()
    };
    let mut ucfx_g = std::borrow::Cow::Borrowed(ucfx);
    if needed > base_drawing {
        let (grown, _) = append_draw_groups(ucfx, needed - base_drawing)?;
        ucfx_g = std::borrow::Cow::Owned(grown);
    }

    // ---- author one novel bone per part, then enumerate the (grown) drawing-group pool ----
    let (grown_ucfx, bone_indices) = append_new_bones(&ucfx_g, &bones)?;
    let (gdata_off, _gn, grows) = parse_rows(&grown_ucfx);
    let ggroups = find_groups(&grows);
    let drawing: Vec<usize> = (0..ggroups.len())
        .filter(|&gi| group_draws(&grown_ucfx, gdata_off, &grows, &ggroups[gi]))
        .collect();

    // ---- assign a distinct drawing group per sub-part; all sub-parts of a part share its bone ----
    let mut specs: Vec<PartSpec> = Vec::new();
    let mut next_host = 0usize;
    let mut report = String::new();
    for (pi, p) in parts.iter().enumerate() {
        for (si, sub) in part_subs[pi].iter().enumerate() {
            if next_host >= drawing.len() {
                return Err(format!(
                    "out of drawing groups after grow: need {needed}, have {}",
                    drawing.len()
                ));
            }
            let group = drawing[next_host];
            next_host += 1;
            report.push_str(&format!(
                "  {}[{}] -> group {} node {} mat {} ({} tris)\n",
                p.label, si, group, bone_indices[pi], p.material_index, sub.tris.len()
            ));
            specs.push(PartSpec {
                label: format!("{}[{}]", p.label, si),
                mesh: sub.clone(),
                group,
                node: bone_indices[pi] as i32,
                material_index: p.material_index,
                recenter_xz: false,
            });
        }
    }

    // ---- conform (the appended template re-fits geometry identically; parts bind to new bones) ----
    let (out_ucfx, stats) = inject_parts_into_template(
        &grown_ucfx, &specs, &[], mtrl_sets, &[], &[], &[], new_name_hash, scale_mult, flip_winding,
        y_offset, fit_percentile,
    )?;

    let block = {
        let mut b = Vec::with_capacity(20 + out_ucfx.len());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&new_name_hash.to_le_bytes());
        b.extend_from_slice(&model_type.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&(out_ucfx.len() as u32).to_le_bytes());
        b.extend_from_slice(&out_ucfx);
        b
    };
    let summary = format!(
        "fresh skeleton: {} parts -> {} novel bones, {} host groups, {} emptied; fit scale {:.4}, bbox {:?}..{:?}\n{}",
        parts.len(), bones.len(), specs.len(), stats.emptied_groups.len(),
        stats.fit_scale, stats.bbox_min, stats.bbox_max, report
    );
    Ok((block, summary))
}

/// Wrap a raw UCFX container in the 20-byte single-entry block header Skeleton::from_block wants.
fn wrap_block(ucfx: &[u8], name_hash: u32) -> Vec<u8> {
    const MODEL_TYPE_HASH: u32 = 0x5B72_4250;
    let mut b = Vec::with_capacity(20 + ucfx.len());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&name_hash.to_le_bytes());
    b.extend_from_slice(&MODEL_TYPE_HASH.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(ucfx.len() as u32).to_le_bytes());
    b.extend_from_slice(ucfx);
    b
}
