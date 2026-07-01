//! Transform a UCFX **model** container into a cube, in place.
//!
//! Proof-of-concept for custom-geometry injection: rather than authoring a model
//! UCFX from scratch (whose vertex-declaration / material / shader bindings the
//! engine may reject), we take a *real* model container the engine already
//! accepts and rewrite only its geometry to a unit cube. Every chunk offset,
//! size, index buffer, vertex declaration, material and texture reference is left
//! byte-identical — only the per-vertex position bytes and the PRMG bounding
//! boxes change, then the CSUM trailer is recomputed. The output is therefore the
//! exact same length as the input and can be spliced back into its block with no
//! entry-table changes.
//!
//! The descriptor-walk here mirrors `wad_simulator::model` and [`crate::ucfx`]
//! precisely (flat 20-byte descriptor rows after a 20-byte header; `u0 ==
//! 0xFFFFFFFF` marks a container whose children are the following non-container
//! rows up to the next container marker; a real `u0` resolves to
//! `data_area_off + u0`).
//!
//! Positions are FLOAT16 vec3 at vertex offset 0; per-vertex stride is read from
//! the STRM `info` chunk (`u32` at +4), which is authoritative over the
//! decl-derived extent.

use crate::crc32::crc32_mercs2;
use crate::ffcs::{read_f32_le, read_u32_le};

/// Half-extent of the generated cube (cube spans `[-HALF, HALF]` in X/Z).
const HALF: f32 = 0.5;
/// Cube vertical span: it sits on the ground like a dropped crate, `y in [0, HEIGHT]`.
const HEIGHT: f32 = 1.0;

/// How to reshape vertex positions into the cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeShape {
    /// Snap every vertex to the nearest cube corner (sharp 8-corner cube). Many
    /// triangles become degenerate, but topology/buffer sizes are unchanged so it
    /// is structurally identical to `Clamp` (only position bytes differ).
    Corner,
    /// Clamp each coordinate into the cube bounds (non-degenerate, but keeps the
    /// source mesh's surface detail projected onto the faces).
    Clamp,
}

/// Result of a cube-ize pass, for logging / verification.
#[derive(Debug, Default, Clone)]
pub struct CubeizeStats {
    pub strm_meshes: usize,
    pub vertices_snapped: usize,
    pub prmg_bounds_rewritten: usize,
}

/// Rewrite a model UCFX container's geometry to a unit cube, in place (default Corner).
pub fn cubeize_model_container(container: &[u8]) -> Result<(Vec<u8>, CubeizeStats), String> {
    cubeize_model_container_with(container, CubeShape::Corner)
}

/// Cube-ize with an explicit [`CubeShape`].
pub fn cubeize_model_container_with(
    container: &[u8],
    shape: CubeShape,
) -> Result<(Vec<u8>, CubeizeStats), String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return Err(format!("descriptor count {n_desc} exceeds capacity {max_desc}"));
    }

    let resolve = |u0: u32, size: usize| -> Option<(usize, usize)> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start.checked_add(size)?;
        if end > container.len() {
            None
        } else {
            Some((start, end))
        }
    };

    let mut out = container.to_vec();
    let mut stats = CubeizeStats::default();

    let mut in_strm = false;
    let mut strm_stride: Option<usize> = None;
    let mut strm_data: Option<(usize, usize)> = None;

    for i in 0..n_desc {
        let row = 20 + i * 20;
        let tag = &container[row..row + 4];
        let u0 = read_u32_le(container, row + 4);
        let size = read_u32_le(container, row + 8) as usize;
        let is_container = u0 == 0xFFFF_FFFF;

        if is_container {
            if in_strm {
                snap_strm(&mut out, strm_stride, strm_data, shape, &mut stats);
            }
            in_strm = tag == b"STRM";
            strm_stride = None;
            strm_data = None;
            continue;
        }

        if in_strm {
            match tag {
                b"info" => {
                    if let Some((s, _)) = resolve(u0, size) {
                        if size >= 12 {
                            strm_stride = Some(read_u32_le(container, s + 4) as usize);
                        }
                    }
                }
                b"data" => {
                    strm_data = resolve(u0, size);
                }
                _ => {}
            }
            continue;
        }

        // PRMG bounding record: a 60-byte `INFO` leaf.
        if tag == b"INFO" && size == 60 {
            if let Some((s, _)) = resolve(u0, size) {
                write_prmg_bounds(&mut out, s);
                stats.prmg_bounds_rewritten += 1;
            }
        }
    }

    if in_strm {
        snap_strm(&mut out, strm_stride, strm_data, shape, &mut stats);
    }

    // Recompute the CSUM trailer over the modified body.
    let len = out.len();
    if len >= 8 && &out[len - 8..len - 4] == b"CSUM" {
        let csum = crc32_mercs2(&out[..len - 8]);
        out[len - 4..len].copy_from_slice(&csum.to_le_bytes());
    }

    Ok((out, stats))
}

/// Read-only companion to [`cubeize_model_container`]: extract every STRM mesh's vertex
/// positions (FLOAT16 vec3 at vertex offset 0; per-vertex stride from the STRM `info` chunk
/// `u32` at +4). Returns one position list per STRM mesh, in descriptor order. Used by the
/// reimplementation engine (`mercs2_engine`) to render real geometry straight from a model
/// container — the descriptor walk is identical to the cube-ize pass, minus the mutation.
pub fn read_model_positions(container: &[u8]) -> Result<Vec<Vec<[f32; 3]>>, String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return Err(format!("descriptor count {n_desc} exceeds capacity {max_desc}"));
    }

    let resolve = |u0: u32, size: usize| -> Option<(usize, usize)> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start.checked_add(size)?;
        if end > container.len() {
            None
        } else {
            Some((start, end))
        }
    };

    let mut meshes: Vec<Vec<[f32; 3]>> = Vec::new();
    let mut in_strm = false;
    let mut stride: Option<usize> = None;
    let mut decl: Option<(usize, usize)> = None;
    let mut data: Option<(usize, usize)> = None;

    for i in 0..n_desc {
        let row = 20 + i * 20;
        let tag = &container[row..row + 4];
        let u0 = read_u32_le(container, row + 4);
        let size = read_u32_le(container, row + 8) as usize;
        if u0 == 0xFFFF_FFFF {
            if in_strm {
                collect_strm_positions(container, stride, decl, data, &mut meshes);
            }
            in_strm = tag == b"STRM";
            stride = None;
            decl = None;
            data = None;
            continue;
        }
        if in_strm {
            match tag {
                b"info" => {
                    if let Some((s, _)) = resolve(u0, size) {
                        if size >= 12 {
                            stride = Some(read_u32_le(container, s + 4) as usize);
                        }
                    }
                }
                b"decl" => decl = resolve(u0, size),
                b"data" => data = resolve(u0, size),
                _ => {}
            }
        }
    }
    if in_strm {
        collect_strm_positions(container, stride, decl, data, &mut meshes);
    }
    Ok(meshes)
}

/// One drawing group's geometry: local vertex positions + triangle-list indices into them.
/// One `SEGM` record: which HIER bone a sub-object attaches to, and its LOD/state mask.
#[derive(Debug, Clone, Copy, Default)]
pub struct SegRec {
    pub bone: u16,
    pub seg_id: u8,
    pub state_mask: u8,
}

/// Parse the `SEGM` chunk into records `{u16 bone@0, u8 seg_id@2, u8 state_mask@3}` (4 bytes each).
/// The k-th top-level `SKIN`/`MESH` sub-object under `GEOM` binds to record `k` (seg_id == k).
pub fn parse_segm(container: &[u8]) -> Vec<SegRec> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Vec::new();
    }
    let data_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max = container.len().saturating_sub(20) / 20;
    for i in 0..n_desc.min(max) {
        let ro = 20 + i * 20;
        if &container[ro..ro + 4] == b"SEGM" {
            let u0 = read_u32_le(container, ro + 4);
            if u0 == 0xFFFF_FFFF {
                continue;
            }
            let start = data_off + u0 as usize;
            let size = read_u32_le(container, ro + 8) as usize;
            let end = (start + size).min(container.len());
            let mut recs = Vec::new();
            let mut o = start;
            while o + 4 <= end {
                recs.push(SegRec {
                    bone: u16::from_le_bytes([container[o], container[o + 1]]),
                    seg_id: container[o + 2],
                    state_mask: container[o + 3],
                });
                o += 4;
            }
            return recs;
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, Default)]
pub struct ModelMesh {
    /// The PRMG group ordinal this mesh came from (for material/segment lookup by group).
    pub group_index: usize,
    /// Ordinal of the parent top-level `SKIN`/`MESH` sub-object under `GEOM` (== SEGM record index).
    pub sub_object: usize,
    /// True if the parent sub-object is `MESH` (rigid accessory in bone-local space); false = `SKIN`.
    pub rigid: bool,
    /// Attachment bone (HIER index) from `SEGM[sub_object]`. 0 = root for skinned body.
    pub bone: u16,
    /// LOD/state bitmask from `SEGM[sub_object]` (1/2/4/8 tiers; 0x0F = all).
    pub state_mask: u8,
    pub positions: Vec<[f32; 3]>,
    /// TEXCOORD0 per vertex (decl usage 5, FLOAT16_2). Empty if the group has no UVs.
    pub uvs: Vec<[f32; 2]>,
    /// NORMAL per vertex (decl usage 3, FLOAT16_4 xyz). Empty if absent. For lighting.
    pub normals: Vec<[f32; 3]>,
    /// TANGENT per vertex (decl usage 6, FLOAT16_4: xyz + w handedness). Empty if absent.
    pub tangents: Vec<[f32; 4]>,
    /// BLENDINDICES per vertex (decl usage 2, UBYTE4): 4 GLOBAL HIER bone indices. Empty if absent.
    pub joints: Vec<[u8; 4]>,
    /// BLENDWEIGHT per vertex (decl usage 1, UBYTE4N): 4 weights summing to 255. Empty if absent.
    pub weights: Vec<[u8; 4]>,
    /// Triangle-list indices into `positions` (the IBUF triangle strip, de-stripped).
    pub tris: Vec<[u32; 3]>,
}

/// Map each PRMG-marker row to its parent top-level sub-object: `(ordinal k, is_rigid)`. Walks
/// GEOM's direct children (each marker consumes `1 + x3` rows); the k-th `SKIN`/`MESH` child is
/// sub-object k → `SEGM` record k. See docs/modernization/accessory_bone_binding_A.md (double-blind).
fn map_prmg_subobjects(
    container: &[u8],
    n_desc: usize,
) -> std::collections::HashMap<usize, (usize, bool)> {
    let tag = |i: usize| &container[20 + i * 20..20 + i * 20 + 4];
    let is_marker = |i: usize| read_u32_le(container, 20 + i * 20 + 4) == 0xFFFF_FFFF;
    let x3 = |i: usize| read_u32_le(container, 20 + i * 20 + 16) as usize;

    let mut out = std::collections::HashMap::new();
    let Some(geom) = (0..n_desc).find(|&i| tag(i) == b"GEOM" && is_marker(i)) else {
        return out;
    };
    let geom_end = (geom + 1 + x3(geom)).min(n_desc);
    let mut k = 0usize;
    let mut r = geom + 1;
    while r < geom_end {
        if is_marker(r) {
            let child_end = (r + 1 + x3(r)).min(n_desc);
            let t = tag(r);
            if t == b"SKIN" || t == b"MESH" {
                let rigid = t == b"MESH";
                for p in (r + 1)..child_end {
                    if is_marker(p) && tag(p) == b"PRMG" {
                        out.insert(p, (k, rigid));
                    }
                }
                k += 1;
            }
            r = child_end;
        } else {
            r += 1;
        }
    }
    out
}

/// Read a model container as INDEXED triangle meshes — one per `PRMG` drawing group. Each group
/// pairs a `STRM` (vertex stream; POSITION read via its decl) with an `IBUF` (u16 triangle strip;
/// index count at `ibuf_info+0`). The strip is de-stripped to a triangle list (winding-aware).
/// This is the 1d path: solid surfaces instead of a point cloud.
pub fn read_model_meshes(container: &[u8]) -> Result<Vec<ModelMesh>, String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return Err(format!("descriptor count {n_desc} exceeds capacity {max_desc}"));
    }
    let resolve = |u0: u32, size: usize| -> Option<(usize, usize)> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start.checked_add(size)?;
        (end <= container.len()).then_some((start, end))
    };

    // Row-level scan: find PRMG group markers, then collect each group's STRM + IBUF leaves.
    let tag = |i: usize| &container[20 + i * 20..20 + i * 20 + 4];
    let u0 = |i: usize| read_u32_le(container, 20 + i * 20 + 4);
    let size = |i: usize| read_u32_le(container, 20 + i * 20 + 8) as usize;

    let prmg: Vec<usize> = (0..n_desc)
        .filter(|&i| tag(i) == b"PRMG" && u0(i) == 0xFFFF_FFFF)
        .collect();

    // Sub-object → bone/LOD binding (SEGM + GEOM tree walk).
    let subobj = map_prmg_subobjects(container, n_desc);
    let segm = parse_segm(container);

    let mut out = Vec::new();
    for (gi, &pr) in prmg.iter().enumerate() {
        let (sub_object, rigid) = subobj.get(&pr).copied().unwrap_or((0, false));
        let seg = segm.get(sub_object).copied().unwrap_or_default();
        let nxt = prmg.get(gi + 1).copied().unwrap_or(n_desc);
        let (mut strm_info, mut strm_decl, mut strm_data) = (None, None, None);
        let (mut ibuf_info, mut ibuf_data, mut prmt) = (None, None, None);
        let mut state = 0u8; // 1 = inside STRM, 2 = inside IBUF
        for i in (pr + 1)..nxt {
            let cm = u0(i) == 0xFFFF_FFFF;
            match (tag(i), cm) {
                (b"STRM", true) => state = 1,
                (b"IBUF", true) => state = 2,
                (b"PRMT", false) if prmt.is_none() => prmt = resolve(u0(i), size(i)),
                (b"info", false) if state == 1 && strm_info.is_none() => strm_info = resolve(u0(i), size(i)),
                (b"decl", false) if state == 1 && strm_decl.is_none() => strm_decl = resolve(u0(i), size(i)),
                (b"data", false) if state == 1 && strm_data.is_none() => strm_data = resolve(u0(i), size(i)),
                (b"info", false) if state == 2 && ibuf_info.is_none() => ibuf_info = resolve(u0(i), size(i)),
                (b"data", false) if state == 2 && ibuf_data.is_none() => ibuf_data = resolve(u0(i), size(i)),
                _ => {}
            }
        }

        let (Some((si, _)), Some((sds, sde)), Some((iis, _)), Some((ids, ide))) =
            (strm_info, strm_data, ibuf_info, ibuf_data)
        else {
            continue;
        };
        let stride = read_u32_le(container, si + 4) as usize;
        if !(6..=256).contains(&stride) {
            continue;
        }
        // POSITION (required) + optional TEXCOORD0 / NORMAL from the decl.
        let decl_slice = strm_decl.map(|(ds, de)| &container[ds..de]);
        let (pos_off, pos_type) = match decl_slice {
            Some(d) => match find_element(d, USAGE_POSITION) {
                Some(pe) => pe,
                None => continue, // decl with no position -> not a drawing stream
            },
            None => (0, 16),
        };
        let uv_el = decl_slice.and_then(|d| find_element(d, USAGE_TEXCOORD));
        let nrm_el = decl_slice.and_then(|d| find_element(d, USAGE_NORMAL));
        let tan_el = decl_slice.and_then(|d| find_element(d, USAGE_TANGENT));
        let jnt_el = decl_slice.and_then(|d| find_element(d, USAGE_BLENDINDICES));
        let wgt_el = decl_slice.and_then(|d| find_element(d, USAGE_BLENDWEIGHT));
        let read4 = |o: usize| -> Option<[u8; 4]> {
            (o + 4 <= sde).then(|| [container[o], container[o + 1], container[o + 2], container[o + 3]])
        };

        // Vertices.
        let vcount = (sde - sds) / stride;
        let mut positions = Vec::with_capacity(vcount);
        let mut uvs = Vec::new();
        let mut normals = Vec::new();
        let mut tangents = Vec::new();
        let mut joints = Vec::new();
        let mut weights = Vec::new();
        for v in 0..vcount {
            let base = sds + v * stride;
            match decode_pos(container, base + pos_off, sde, pos_type) {
                Some(p) => positions.push(p),
                None => break,
            }
            if let Some((uo, ut)) = uv_el {
                if let Some(uv) = decode_uv(container, base + uo, sde, ut) {
                    uvs.push(uv);
                }
            }
            if let Some((no, nt)) = nrm_el {
                if let Some(n) = decode_pos(container, base + no, sde, nt) {
                    normals.push(n);
                }
            }
            if let Some((to, tt)) = tan_el {
                if let Some(t) = decode_vec4(container, base + to, sde, tt) {
                    tangents.push(t);
                }
            }
            if let Some((jo, _)) = jnt_el {
                if let Some(j) = read4(base + jo) {
                    joints.push(j);
                }
            }
            if let Some((wo, _)) = wgt_el {
                if let Some(w) = read4(base + wo) {
                    weights.push(w);
                }
            }
        }
        if positions.is_empty() {
            continue;
        }
        // Keep per-vertex arrays aligned with positions.
        uvs.truncate(positions.len());
        normals.truncate(positions.len());
        tangents.truncate(positions.len());
        joints.truncate(positions.len());
        weights.truncate(positions.len());

        // Index strip (u16) → triangles. Index count at ibuf_info+0.
        let ic = read_u32_le(container, iis) as usize;
        let avail = (ide - ids) / 2;
        let n = ic.min(avail);
        let mut strip = Vec::with_capacity(n);
        let vmax = positions.len() as u32;
        for k in 0..n {
            let idx = u16::from_le_bytes([container[ids + k * 2], container[ids + k * 2 + 1]]) as u32;
            strip.push(idx);
        }
        // De-strip. A drawing group can hold several strips concatenated in one IBUF, one per
        // PRMT primitive record (16 bytes: index_start@4, index_count@8). De-stripping the whole
        // IBUF as one continuous strip would join the end of one sub-strip to the start of the
        // next, spanning a stray triangle across the mesh (the hand->back sliver). So de-strip each
        // PRMT range separately — with a sanity gate; if the ranges don't cover the strip cleanly
        // (e.g. PRMT layout differs), fall back to whole-strip de-stripping.
        let tris = destrip_by_prmt(container, &strip, prmt, vmax);

        out.push(ModelMesh {
            group_index: gi,
            sub_object,
            rigid,
            bone: seg.bone,
            state_mask: seg.state_mask,
            positions,
            uvs,
            normals,
            tangents,
            joints,
            weights,
            tris,
        });
    }
    Ok(out)
}

/// De-strip a group's IBUF into triangles. A group's IBUF holds several sub-strips concatenated,
/// one per `PRMT` primitive record (16 bytes; `index_start` @4). Verified on real models: those
/// starts are monotonic strip boundaries (each primitive's count derives from the next start). We
/// split the strip at those boundaries and de-strip each range independently, so no triangle spans
/// across separate sub-strips (the hand→back sliver). Falls back to the whole strip if no PRMT.
fn destrip_by_prmt(
    container: &[u8],
    strip: &[u32],
    prmt: Option<(usize, usize)>,
    vmax: u32,
) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    if let Some((ps, pe)) = prmt {
        let nrec = (pe - ps) / 16;
        if nrec >= 1 {
            // Primitive boundaries = PRMT.index_start (@4), plus 0 and the strip end.
            let mut bounds: Vec<usize> = (0..nrec)
                .map(|r| read_u32_le(container, ps + r * 16 + 4) as usize)
                .filter(|&s| s <= strip.len())
                .collect();
            bounds.push(0);
            bounds.push(strip.len());
            bounds.sort_unstable();
            bounds.dedup();
            for w in bounds.windows(2) {
                strip_range_to_tris(strip, w[0], w[1], vmax, &mut out);
            }
            if !out.is_empty() {
                return out;
            }
            out.clear();
        }
    }
    strip_range_to_tris(strip, 0, strip.len(), vmax, &mut out);
    out
}

/// De-strip `strip[start..end]` into `out`. Winding parity uses the ABSOLUTE strip index (so a
/// sub-strip beginning at an odd offset is wound correctly — needed once backface culling is on).
/// Degenerate and out-of-range triangles are dropped.
fn strip_range_to_tris(strip: &[u32], start: usize, end: usize, vmax: u32, out: &mut Vec<[u32; 3]>) {
    let end = end.min(strip.len());
    let mut i = start;
    while i + 2 < end {
        let (a, b, c) = (strip[i], strip[i + 1], strip[i + 2]);
        if a != b && b != c && a != c && a < vmax && b < vmax && c < vmax {
            // Each PRMT sub-strip is an independent strip: parity resets at its start.
            if (i - start) % 2 == 0 {
                out.push([a, b, c]);
            } else {
                out.push([a, c, b]);
            }
        }
        i += 1;
    }
}

/// Per-STRM diagnostic used to pinpoint mis-read submeshes (e.g. floating accessories).
#[derive(Debug, Clone)]
pub struct StrmInfo {
    pub stride: usize,
    pub vcount: usize,
    pub decl_elems: usize,
    /// POSITION element: (stream, offset, d3ddecltype). None = no decl / no position.
    pub pos: Option<(u16, usize, u16)>,
    /// Bounding box of the decoded positions, if we produced any.
    pub bbox: Option<([f32; 3], [f32; 3])>,
}

/// Describe every STRM group in a model container (stride, vcount, decl, POSITION element, bbox).
/// Read-only; used by the engine's `--meshes` diagnostic to find the group that renders wrong.
pub fn describe_model_strms(container: &[u8]) -> Result<Vec<StrmInfo>, String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return Err(format!("descriptor count {n_desc} exceeds capacity {max_desc}"));
    }
    let resolve = |u0: u32, size: usize| -> Option<(usize, usize)> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start.checked_add(size)?;
        (end <= container.len()).then_some((start, end))
    };

    let mut out = Vec::new();
    let mut in_strm = false;
    let (mut stride, mut vcount): (Option<usize>, usize) = (None, 0);
    let mut decl: Option<(usize, usize)> = None;
    let mut data: Option<(usize, usize)> = None;

    let flush = |stride: Option<usize>,
                 vcount: usize,
                 decl: Option<(usize, usize)>,
                 data: Option<(usize, usize)>,
                 out: &mut Vec<StrmInfo>| {
        let Some(stride) = stride else { return };
        let (decl_elems, pos) = match decl {
            Some((ds, de)) => {
                let d = &container[ds..de];
                (d.len() / 8, find_position_stream(d))
            }
            None => (0, None),
        };
        let mut bbox = None;
        if let (Some((start, end)), Some((_s, off, typ))) = (data, pos) {
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            let count = (end - start) / stride;
            for i in 0..count {
                let o = start + i * stride + off;
                let p = decode_pos(container, o, end, typ);
                if let Some(p) = p {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
            if lo[0].is_finite() {
                bbox = Some((lo, hi));
            }
        }
        out.push(StrmInfo {
            stride,
            vcount,
            decl_elems,
            pos,
            bbox,
        });
    };

    for i in 0..n_desc {
        let row = 20 + i * 20;
        let tag = &container[row..row + 4];
        let u0 = read_u32_le(container, row + 4);
        let size = read_u32_le(container, row + 8) as usize;
        if u0 == 0xFFFF_FFFF {
            if in_strm {
                flush(stride, vcount, decl, data, &mut out);
            }
            in_strm = tag == b"STRM";
            stride = None;
            vcount = 0;
            decl = None;
            data = None;
            continue;
        }
        if in_strm {
            match tag {
                b"info" => {
                    if let Some((s, _)) = resolve(u0, size) {
                        if size >= 12 {
                            stride = Some(read_u32_le(container, s + 4) as usize);
                            vcount = read_u32_le(container, s + 8) as usize;
                        }
                    }
                }
                b"decl" => decl = resolve(u0, size),
                b"data" => data = resolve(u0, size),
                _ => {}
            }
        }
    }
    if in_strm {
        flush(stride, vcount, decl, data, &mut out);
    }
    Ok(out)
}

/// Like [`find_position_element`] but also reports the stream index.
fn find_position_stream(decl: &[u8]) -> Option<(u16, usize, u16)> {
    let mut i = 0;
    while i + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[i], decl[i + 1]]);
        if stream == 0x00FF {
            break;
        }
        let offset = u16::from_le_bytes([decl[i + 2], decl[i + 3]]) as usize;
        let typ = u16::from_le_bytes([decl[i + 4], decl[i + 5]]);
        if decl[i + 6] == USAGE_POSITION {
            return Some((stream, offset, typ));
        }
        i += 8;
    }
    None
}

fn decode_pos(c: &[u8], o: usize, end: usize, typ: u16) -> Option<[f32; 3]> {
    match typ {
        15 | 16 if o + 6 <= end => Some([
            read_f16_le(c, o),
            read_f16_le(c, o + 2),
            read_f16_le(c, o + 4),
        ]),
        2 | 3 if o + 12 <= end => Some([
            read_f32_le(c, o),
            read_f32_le(c, o + 4),
            read_f32_le(c, o + 8),
        ]),
        _ => None,
    }
}

/// Vertex-declaration semantics we care about (D3DDECLUSAGE).
const USAGE_POSITION: u8 = 0;
const USAGE_BLENDWEIGHT: u8 = 1;
const USAGE_BLENDINDICES: u8 = 2;
const USAGE_NORMAL: u8 = 3;
const USAGE_TEXCOORD: u8 = 5;
const USAGE_TANGENT: u8 = 6;

/// Find a decl element by D3DDECLUSAGE. Each element is 8 bytes:
/// `[stream:u16][offset:u16][type:u16][usage:u16]`, terminated by `stream == 0x00FF`.
/// `usage` low byte is the D3DDECLUSAGE; `type` is the D3DDECLTYPE. Returns `(offset, type)`.
fn find_element(decl: &[u8], usage: u8) -> Option<(usize, u16)> {
    let mut i = 0;
    while i + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[i], decl[i + 1]]);
        if stream == 0x00FF {
            break; // END marker
        }
        let offset = u16::from_le_bytes([decl[i + 2], decl[i + 3]]) as usize;
        let typ = u16::from_le_bytes([decl[i + 4], decl[i + 5]]);
        if decl[i + 6] == usage {
            return Some((offset, typ));
        }
        i += 8;
    }
    None
}

fn find_position_element(decl: &[u8]) -> Option<(usize, u16)> {
    find_element(decl, USAGE_POSITION)
}

/// Decode a 2-component UV (FLOAT16_2 = type 15, or FLOAT2/3/4).
fn decode_uv(c: &[u8], o: usize, end: usize, typ: u16) -> Option<[f32; 2]> {
    match typ {
        15 | 16 if o + 4 <= end => Some([read_f16_le(c, o), read_f16_le(c, o + 2)]),
        1 | 2 | 3 if o + 8 <= end => Some([read_f32_le(c, o), read_f32_le(c, o + 4)]),
        _ => None,
    }
}

/// Decode a 4-component vector (FLOAT16_4 = type 16, or FLOAT4). Used for TANGENT (xyz + w).
fn decode_vec4(c: &[u8], o: usize, end: usize, typ: u16) -> Option<[f32; 4]> {
    match typ {
        16 if o + 8 <= end => Some([
            read_f16_le(c, o),
            read_f16_le(c, o + 2),
            read_f16_le(c, o + 4),
            read_f16_le(c, o + 6),
        ]),
        3 if o + 16 <= end => Some([
            read_f32_le(c, o),
            read_f32_le(c, o + 4),
            read_f32_le(c, o + 8),
            read_f32_le(c, o + 12),
        ]),
        _ => None,
    }
}

/// Decode one STRM group's vertex positions using its declaration so we read the POSITION
/// element at the right offset/format — NOT a blind f16 @ offset 0 (which garbles groups whose
/// position is float32 or at a non-zero offset, and mis-reads non-drawing streams).
fn collect_strm_positions(
    container: &[u8],
    stride: Option<usize>,
    decl: Option<(usize, usize)>,
    data: Option<(usize, usize)>,
    out: &mut Vec<Vec<[f32; 3]>>,
) {
    let (Some(stride), Some((start, end))) = (stride, data) else {
        return;
    };
    if !(6..=256).contains(&stride) {
        return;
    }
    // Position element from the decl. If a decl is present but has NO position, this is not a
    // drawing vertex stream (e.g. a normal/UV/skin stream) — skip it. Only when there is no decl
    // at all do we fall back to the legacy FLOAT16_4 @ 0 assumption.
    let (pos_off, pos_type) = match decl {
        Some((ds, de)) => match find_position_element(&container[ds..de]) {
            Some(pe) => pe,
            None => return,
        },
        None => (0, 16),
    };

    let count = (end - start) / stride;
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let o = start + i * stride + pos_off;
        let p = match pos_type {
            // FLOAT16_2 / FLOAT16_4 (xyz are the first 3 halfs)
            15 | 16 => {
                if o + 6 > end {
                    break;
                }
                [
                    read_f16_le(container, o),
                    read_f16_le(container, o + 2),
                    read_f16_le(container, o + 4),
                ]
            }
            // FLOAT3 / FLOAT4
            2 | 3 => {
                if o + 12 > end {
                    break;
                }
                [
                    read_f32_le(container, o),
                    read_f32_le(container, o + 4),
                    read_f32_le(container, o + 8),
                ]
            }
            // Unknown position format — skip this group rather than emit garbage.
            _ => return,
        };
        v.push(p);
    }
    if !v.is_empty() {
        out.push(v);
    }
}

/// Reshape a STRM `data` buffer's FLOAT16 positions (offset 0) into the cube.
/// Clamp keeps every sub-mesh non-degenerate; Corner gives a sharp 8-corner cube.
fn snap_strm(
    out: &mut [u8],
    stride: Option<usize>,
    data: Option<(usize, usize)>,
    shape: CubeShape,
    stats: &mut CubeizeStats,
) {
    let (Some(stride), Some((start, end))) = (stride, data) else {
        return;
    };
    if stride < 6 || stride > 256 {
        return;
    }
    stats.strm_meshes += 1;
    let count = (end - start) / stride;
    for v in 0..count {
        let o = start + v * stride;
        if o + 6 > end {
            break;
        }
        let x = read_f16_le(out, o);
        let y = read_f16_le(out, o + 2);
        let z = read_f16_le(out, o + 4);
        let (nx, ny, nz) = match shape {
            CubeShape::Corner => (
                if x < 0.0 { -HALF } else { HALF },
                if y < HEIGHT * 0.5 { 0.0 } else { HEIGHT },
                if z < 0.0 { -HALF } else { HALF },
            ),
            CubeShape::Clamp => (
                x.clamp(-HALF, HALF),
                y.clamp(0.0, HEIGHT),
                z.clamp(-HALF, HALF),
            ),
        };
        write_f16_le(out, o, nx);
        write_f16_le(out, o + 2, ny);
        write_f16_le(out, o + 4, nz);
        stats.vertices_snapped += 1;
    }
}

/// Overwrite a 60-byte PRMG INFO record's bounding sphere + AABB with the cube's.
/// +20 center.xyz, +32 radius, +36 min.xyz, +48 max.xyz (all f32).
fn write_prmg_bounds(out: &mut [u8], base: usize) {
    let cy = HEIGHT * 0.5;
    let radius = (HALF * HALF + cy * cy + HALF * HALF).sqrt();
    let fields: [(usize, f32); 10] = [
        (20, 0.0),
        (24, cy),
        (28, 0.0),
        (32, radius),
        (36, -HALF),
        (40, 0.0),
        (44, -HALF),
        (48, HALF),
        (52, HEIGHT),
        (56, HALF),
    ];
    for (off, val) in fields {
        out[base + off..base + off + 4].copy_from_slice(&val.to_le_bytes());
    }
}

/// Decode a little-endian IEEE-754 half-float.
fn read_f16_le(d: &[u8], off: usize) -> f32 {
    let h = u16::from_le_bytes([d[off], d[off + 1]]);
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let frac = (h & 0x3ff) as u32;
    let val = if exp == 0 {
        (frac as f32 / 1024.0) * 2f32.powi(-14)
    } else if exp == 0x1f {
        if frac == 0 { f32::INFINITY } else { f32::NAN }
    } else {
        (1.0 + frac as f32 / 1024.0) * 2f32.powi(exp as i32 - 15)
    };
    if sign == 1 { -val } else { val }
}

/// Encode an f32 to a little-endian IEEE-754 half-float (round-to-nearest-even).
fn write_f16_le(d: &mut [u8], off: usize, value: f32) {
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
            if (mant_with_implicit & (round_bit - 1)) != 0 || (mant_with_implicit & round_bit) != 0 {
                if (mant_with_implicit & ((round_bit << 1) - 1)) > round_bit || (m & 1) == 1 {
                    m += 1;
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
                    d[off..off + 2].copy_from_slice(&(sign | 0x7c00).to_le_bytes());
                    return;
                }
            }
        }
        sign | (hexp << 10) | half_mant
    };
    d[off..off + 2].copy_from_slice(&h.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_roundtrip_cube_coords() {
        let mut buf = [0u8; 2];
        for v in [-0.5f32, 0.0, 0.5, 1.0, -1.0] {
            write_f16_le(&mut buf, 0, v);
            assert_eq!(read_f16_le(&buf, 0), v, "f16 roundtrip failed for {v}");
        }
    }

    #[test]
    fn cubeize_minimal_container() {
        use crate::ucfx::verify_ucfx_container;

        let stride = 8usize;
        let nverts = 3usize;
        let info_body = {
            let mut b = vec![0u8; 12];
            b[4..8].copy_from_slice(&(stride as u32).to_le_bytes());
            b[8..12].copy_from_slice(&(nverts as u32).to_le_bytes());
            b
        };
        let mut data_body = vec![0u8; stride * nverts];
        let pos = [(-0.77f32, 0.05, -0.46), (0.6, 0.9, 0.3), (-0.2, 0.8, 0.1)];
        for (i, (x, y, z)) in pos.iter().enumerate() {
            let o = i * stride;
            write_f16_le(&mut data_body, o, *x);
            write_f16_le(&mut data_body, o + 2, *y);
            write_f16_le(&mut data_body, o + 4, *z);
        }
        let prmg_info = vec![0u8; 60];

        let info_off = 0u32;
        let data_off = info_body.len() as u32;
        let prmg_off = (info_body.len() + data_body.len()) as u32;
        let mut data_area = Vec::new();
        data_area.extend_from_slice(&info_body);
        data_area.extend_from_slice(&data_body);
        data_area.extend_from_slice(&prmg_info);

        let data_area_off = (20 + 4 * 20) as u32;
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&data_area_off.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&4u32.to_le_bytes());
        let mut row = |tag: &[u8; 4], u0: u32, size: u32| {
            c.extend_from_slice(tag);
            c.extend_from_slice(&u0.to_le_bytes());
            c.extend_from_slice(&size.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
        };
        row(b"INFO", prmg_off, 60);
        row(b"STRM", 0xFFFF_FFFF, 0);
        row(b"info", info_off, info_body.len() as u32);
        row(b"data", data_off, data_body.len() as u32);
        c.extend_from_slice(&data_area);
        c.extend_from_slice(b"CSUM");
        c.extend_from_slice(&0u32.to_le_bytes());

        let (out, stats) = cubeize_model_container(&c).expect("cubeize");
        assert_eq!(out.len(), c.len());
        assert_eq!(stats.strm_meshes, 1);
        assert_eq!(stats.vertices_snapped, 3);
        assert_eq!(stats.prmg_bounds_rewritten, 1);

        let dstart = data_area_off as usize + data_off as usize;
        for i in 0..nverts {
            let o = dstart + i * stride;
            let x = read_f16_le(&out, o);
            let y = read_f16_le(&out, o + 2);
            let z = read_f16_le(&out, o + 4);
            assert!(x == -HALF || x == HALF, "x={x}");
            assert!(z == -HALF || z == HALF, "z={z}");
            assert!(y == 0.0 || y == HEIGHT, "y={y}");
        }

        let (clamped, _) = cubeize_model_container_with(&c, CubeShape::Clamp).expect("clamp");
        assert_eq!(read_f16_le(&clamped, dstart), -HALF);

        let model_type = crate::hash::pandemic_hash_m2("model");
        if let Some(issues) = verify_ucfx_container(&out, "test", model_type) {
            for is in &issues {
                assert!(!is.detail.contains("CSUM mismatch"), "CSUM: {}", is.detail);
            }
        }
    }
}
