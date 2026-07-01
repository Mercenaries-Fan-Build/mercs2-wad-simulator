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
    /// Per-vertex BLENDINDICES (global bone indices). Empty => rigid bone-0 fallback.
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
        let mut state = 0u8; // 1=STRM, 2=IBUF
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
                }
            } else if &r.tag == b"decl" && !cm && state == 1 && strm_decl.is_none() {
                strm_decl = Some(i);
            } else if &r.tag == b"data" && !cm {
                if state == 1 && strm_data.is_none() {
                    strm_data = Some(i);
                } else if state == 2 && ibuf_data.is_none() {
                    ibuf_data = Some(i);
                }
            }
        }
        if let (Some(si), Some(sd), Some(sda), Some(ii), Some(idd), Some(pt)) =
            (strm_info, strm_decl, strm_data, ibuf_info, ibuf_data, prmt)
        {
            groups.push(Group {
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

/// CROSS-SKELETON INVERSE-BIND RE-POSE (the reusable kitbash deformation fix).
///
/// A borrowed part's vertices are stored in the SOURCE skeleton's bind/model
/// space and are bound to SOURCE bones. If we simply remap the bone INDICES to the
/// donor (mattias) and inject, the engine skins those verts with the donor bones'
/// `InvBind_M · BoneAnim_M` — but the verts live in the SOURCE bind frame, so the
/// `InvBind_M` factor is wrong and the part COLLAPSES the moment a bone moves off
/// bind. This driver re-poses every vertex into the DONOR bind frame so that
/// donor-rig skinning reproduces the part's intended shape and deforms rigidly:
///
/// ```text
///   v' = Σ_b  w_b · ( World_M[map(b)] · InvBind_S[b] ) · v
///   n' = normalize( Σ_b  w_b · R3x3( World_M[map(b)] · InvBind_S[b] ) · n )
/// ```
///
/// where `World_M[map(b)]` is the donor bone's world-bind, `InvBind_S[b]` the
/// inverse of the SOURCE bone's world-bind, and `map()` the SAME bone-hash match
/// (with parent fallback) used to remap the indices afterwards. `mesh.joints` MUST
/// still carry SOURCE bone indices when this is called (run BEFORE the index
/// remap). Weights are untouched. Per-influence matrices are weight-blended per
/// vertex (linear blend skinning), exactly mirroring the engine.
///
/// `map[b]` is the donor bone index for source bone `b` (length = source bone
/// count). Returns the part bbox after re-pose. GENERALISES to any known-good part
/// on any donor: supply the source/donor skeletons and the index map.
pub fn repose_part_cross_skeleton(
    mesh: &mut ExternalMesh,
    src: &crate::skeleton::Skeleton,
    dst: &crate::skeleton::Skeleton,
    map: &[usize],
) -> ([f32; 3], [f32; 3]) {
    use crate::skeleton::{affine_inverse, mat4_mul, transform_dir, transform_point};
    // Per-source-bone re-pose matrix M[b] = World_M[map(b)] · InvBind_S[b]
    // (row-vector convention: p' = p @ InvBind_S @ World_M).
    let n_src = src.bones.len();
    let mut rep: Vec<[[f32; 4]; 4]> = Vec::with_capacity(n_src);
    for b in 0..n_src {
        let inv_s = affine_inverse(&src.bones[b].world);
        let world_m = dst.bones[map[b.min(map.len() - 1)]].world;
        rep.push(mat4_mul(&inv_s, &world_m));
    }
    let mut bmin = [f32::INFINITY; 3];
    let mut bmax = [f32::NEG_INFINITY; 3];
    for i in 0..mesh.positions.len() {
        let p = mesh.positions[i];
        let (joints, weights) = if mesh.joints.is_empty() {
            ([0u8; 4], [255u8, 0, 0, 0])
        } else {
            (mesh.joints[i], mesh.weights[i])
        };
        let wsum: u32 = weights.iter().map(|&w| w as u32).sum();
        let wsum = if wsum == 0 { 255 } else { wsum } as f32;
        let mut acc = [0.0f32; 3];
        let mut nacc = [0.0f32; 3];
        let nrm = mesh.normals[i];
        for k in 0..4 {
            if weights[k] == 0 {
                continue;
            }
            let w = weights[k] as f32 / wsum;
            let b = joints[k] as usize;
            let m = &rep[b.min(n_src - 1)];
            let pp = transform_point(m, p);
            let nn = transform_dir(m, nrm);
            for j in 0..3 {
                acc[j] += w * pp[j];
                nacc[j] += w * nn[j];
            }
        }
        mesh.positions[i] = acc;
        let l = (nacc[0] * nacc[0] + nacc[1] * nacc[1] + nacc[2] * nacc[2])
            .sqrt()
            .max(1e-8);
        mesh.normals[i] = [nacc[0] / l, nacc[1] / l, nacc[2] / l];
        for j in 0..3 {
            bmin[j] = bmin[j].min(acc[j]);
            bmax[j] = bmax[j].max(acc[j]);
        }
    }
    (bmin, bmax)
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
pub fn inject_parts_into_donor_block(
    container_block: &[u8],
    parts: &[InjectPart],
    new_name_hash: u32,
    preserve_native_non_host: bool,
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
            if vc > *donor_vc || ic > *donor_ic {
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
        let prmt = leaf(ucfx, data_off, &rows[g.prmt]);
        // ANY non-zero draw-count (including 0xFFFFFFFE underflow) = drawing.
        let draws = (0..prmt.len() / 16).any(|r| read_u32_le(prmt, r * 16 + 8) != 0);
        if !draws {
            continue;
        }
        let vbuf_sz = rows[g.strm_data].size as usize;
        let ic = read_u32_le(leaf(ucfx, data_off, &rows[g.ibuf_info]), 0) as usize;
        if vbuf_sz == 0 || ic == 0 {
            return Err(format!(
                "PRMG group {gi} draws (PRMT draw-count != 0) but has zero-size buffer: \
                 STRM data={vbuf_sz} bytes, IBUF index_count={ic}"
            ));
        }
    }
    Ok(())
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
    let strip = to_strip(&mesh.tris);
    if strip.len() > 65534 {
        return Err(format!("strip length {} exceeds u16", strip.len()));
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

#[cfg(test)]
mod tests {
    use super::*;

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
