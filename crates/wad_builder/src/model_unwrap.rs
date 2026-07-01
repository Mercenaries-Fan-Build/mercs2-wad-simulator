//! Surgery on Sarah's MESH-region eye sub-meshes.
//!
//! ROOT CAUSE (live x32dbg, 2026-06-29 + offline confirm): Sarah's two eye slots
//! (GEOM slots 15 & 16) are `MESH`-region groups using the STATIC-mesh pipeline
//! (`PgMeshNoTangentVP`/`PgMeshShadowVP` shaders, an `AREA` bounding-volume chunk,
//! and a NON-skinned vertex decl — POSITION/TEXCOORD/NORMAL only, no
//! BLENDINDICES/BLENDWEIGHT, no SKIN chunk). But the character object is SKINNED,
//! so every slot is walked by the skinned-mesh consumer `MeshSkin_ConsumeChunk`
//! (FUN_004796f0), which only knows INFO/IBUF/BSHI/STRM/BSHP/PRMT. It cannot load a
//! static `MESH` slot (it steps over the whole MESH subtree via its sibling-span),
//! so the slot's vertex buffer is built zero-size → D3D `CreateVertexBuffer` returns
//! null → crash at 0x0085C8D0. Working `pmc_hum_obama` is ALL-skinned (19/19 PgSkin,
//! zero MESH/AREA), so it never hits this.
//!
//! Two operations are provided:
//!   * [`unwrap_container_mesh`] — strip only the `AREA` chunks (insufficient on its
//!     own: the MESH wrapper still hides the subtree from the skinned walker; kept
//!     for reference / the AREA-removal primitive).
//!   * [`drop_container_mesh_slots`] — REMOVE the two static-mesh eye slots entirely
//!     (MESH wrapper + INFO + AREA + inner PRMG/INFO/STRM/IBUF/PRMT), drop the GEOM
//!     mesh-slot count 17→15 and trim the INDX list to 0..14. This yields an
//!     "eyeless" Sarah whose 15 skinned body groups all load — a de-risking build to
//!     confirm the rest of the model is sound. The eyes will later be re-injected as
//!     a head-bound SKINNED group (model_inject.rs), not via this drop.
//!
//! Both rebuild the descriptor table, sibling-spans (`u4`), body region + `u0`
//! offsets, `data_base`, `n_desc`, and recompute the CSUM (`crc32_mercs2` over
//! `[UCFX .. pre-CSUM]`, NOT including the CSUM tag).

use mercs2_formats::crc32::crc32_mercs2;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[derive(Clone)]
struct Desc {
    tag: [u8; 4],
    u0: u32,   // 0xFFFFFFFF for container/marker rows; else body offset within data region
    size: u32, // body_size
    u3: u32,   // back-reference index
    u4: u32,   // sibling subtree span
    body: Vec<u8>, // the chunk's body bytes (empty for markers)
}

const AREA: &[u8; 4] = b"AREA";
const MESH: &[u8; 4] = b"MESH";
const GEOM: &[u8; 4] = b"GEOM";
const INDX: &[u8; 4] = b"INDX";
const INFO: &[u8; 4] = b"INFO";
const SEGM: &[u8; 4] = b"SEGM";

/// Parse a UCFX container into descriptors (each carrying its body bytes).
fn parse_descs(container: &[u8]) -> Result<(usize, usize, Vec<Desc>), String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_base = rd_u32(container, 4) as usize;
    let n_desc = rd_u32(container, 16) as usize;
    if 20 + n_desc * 20 > container.len() {
        return Err("descriptor table out of range".into());
    }
    let mut descs = Vec::with_capacity(n_desc);
    for d in 0..n_desc {
        let o = 20 + d * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&container[o..o + 4]);
        let u0 = rd_u32(container, o + 4);
        let size = rd_u32(container, o + 8);
        let u3 = rd_u32(container, o + 12);
        let u4 = rd_u32(container, o + 16);
        let body = if u0 != 0xFFFF_FFFF && size > 0 {
            let start = data_base + u0 as usize;
            let end = start + size as usize;
            if end > container.len() {
                return Err(format!("body of desc {d} ({:?}) out of range", tag));
            }
            container[start..end].to_vec()
        } else {
            Vec::new()
        };
        descs.push(Desc { tag, u0, size, u3, u4, body });
    }
    Ok((data_base, n_desc, descs))
}

/// Given a `remove` mask, decrement every surviving container row's `u4` span by the
/// removed rows inside its subtree, then re-serialize the container (fresh body
/// region + `u0` offsets + `data_base` + `n_desc`) and recompute the CSUM.
fn rebuild_without(
    container_head_u1: &[u8],
    container_head_u2: &[u8],
    descs: &mut [Desc],
    remove: &[bool],
) -> Vec<u8> {
    let n_desc = descs.len();
    let mut removed_prefix = vec![0u32; n_desc + 1];
    for j in 0..n_desc {
        removed_prefix[j + 1] = removed_prefix[j] + if remove[j] { 1 } else { 0 };
    }
    for j in 0..n_desc {
        if remove[j] || descs[j].u4 == 0 {
            continue;
        }
        let lo = j + 1;
        let hi = (j + descs[j].u4 as usize).min(n_desc - 1);
        if lo <= hi {
            let removed_in = removed_prefix[hi + 1] - removed_prefix[lo];
            descs[j].u4 -= removed_in;
        }
    }

    let survivors: Vec<&Desc> =
        descs.iter().enumerate().filter(|(j, _)| !remove[*j]).map(|(_, d)| d).collect();
    let new_n = survivors.len();
    let new_data_base = 20 + new_n * 20;

    let mut out = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(new_data_base as u32).to_le_bytes());
    out.extend_from_slice(container_head_u1);
    out.extend_from_slice(container_head_u2);
    out.extend_from_slice(&(new_n as u32).to_le_bytes());

    let mut body_region: Vec<u8> = Vec::new();
    let mut rows_bytes: Vec<u8> = Vec::with_capacity(new_n * 20);
    for d in &survivors {
        let u0_out = if d.u0 != 0xFFFF_FFFF && d.size > 0 {
            let off = body_region.len() as u32;
            body_region.extend_from_slice(&d.body);
            off
        } else {
            d.u0
        };
        rows_bytes.extend_from_slice(&d.tag);
        rows_bytes.extend_from_slice(&u0_out.to_le_bytes());
        rows_bytes.extend_from_slice(&d.size.to_le_bytes());
        rows_bytes.extend_from_slice(&d.u3.to_le_bytes());
        rows_bytes.extend_from_slice(&d.u4.to_le_bytes());
    }
    out.extend_from_slice(&rows_bytes);
    out.extend_from_slice(&body_region);

    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    out
}

/// Strip every `AREA` chunk-group (the `AREA` marker + its span children) from a
/// container. Returns the number of AREA groups removed.
pub fn unwrap_container_mesh(container: &mut Vec<u8>) -> Result<usize, String> {
    let (_db, n_desc, mut descs) = parse_descs(container)?;
    let mut remove = vec![false; n_desc];
    let mut area_groups = 0usize;
    let mut i = 0usize;
    while i < n_desc {
        if &descs[i].tag == AREA {
            let span = descs[i].u4 as usize;
            for k in 0..=span {
                if i + k < n_desc {
                    remove[i + k] = true;
                }
            }
            area_groups += 1;
            i += span + 1;
        } else {
            i += 1;
        }
    }
    if area_groups == 0 {
        return Ok(0);
    }
    let u1 = container[8..12].to_vec();
    let u2 = container[12..16].to_vec();
    *container = rebuild_without(&u1, &u2, &mut descs, &remove);
    Ok(area_groups)
}

/// Remove every `MESH`-region slot ENTIRELY (the MESH wrapper + its full subtree),
/// then drop the GEOM mesh-slot count by the number removed and trim the INDX list
/// to the surviving slot indices. Returns the number of MESH slots dropped.
pub fn drop_container_mesh_slots(container: &mut Vec<u8>) -> Result<usize, String> {
    let (_db, n_desc, mut descs) = parse_descs(container)?;

    // Mark each MESH marker + its whole subtree (u4 span) for removal.
    let mut remove = vec![false; n_desc];
    let mut dropped = 0usize;
    let mut i = 0usize;
    while i < n_desc {
        if &descs[i].tag == MESH {
            let span = descs[i].u4 as usize;
            for k in 0..=span {
                if i + k < n_desc {
                    remove[i + k] = true;
                }
            }
            dropped += 1;
            i += span + 1;
        } else {
            i += 1;
        }
    }
    if dropped == 0 {
        return Ok(0);
    }

    // Edit the GEOM mesh-slot count + INDX list BEFORE rebuild (their bodies change).
    // The GEOM slot-count is the first INFO(4) directly after the GEOM container; the
    // INDX is the u16 slot-index list. We trim INDX to its first (count-dropped) u16s
    // and set the GEOM INFO(4) to the new count.
    // Find GEOM, then the next INFO(4) (slot count) and the INDX chunk.
    let geom_idx = descs.iter().position(|d| &d.tag == GEOM);
    if let Some(gi) = geom_idx {
        // GEOM slot-count INFO(4): the first INFO with size 4 after GEOM.
        if let Some(info_i) = (gi + 1..n_desc)
            .find(|&j| &descs[j].tag == INFO && descs[j].size == 4 && !remove[j])
        {
            if descs[info_i].body.len() == 4 {
                let old = rd_u32(&descs[info_i].body, 0);
                let new = old.saturating_sub(dropped as u32);
                descs[info_i].body = new.to_le_bytes().to_vec();
            }
        }
        // INDX: trim the trailing `dropped` u16 slot entries.
        if let Some(indx_i) = (gi + 1..n_desc).find(|&j| &descs[j].tag == INDX && !remove[j]) {
            let b = &descs[indx_i].body;
            let entries = b.len() / 2;
            let keep = entries.saturating_sub(dropped);
            descs[indx_i].body = b[..keep * 2].to_vec();
            descs[indx_i].size = (keep * 2) as u32;
        }

        // CRITICAL: `u3` is a REVERSE-SIBLING index, LOCAL to each parent's child list
        // (verified vs obama: 16 slots → u3 15..0; the LAST child is always u3==0).
        // The removed MESH slots were the LAST `dropped` direct children of GEOM, so
        // every SURVIVING GEOM direct child's "siblings-after-me" count drops by
        // `dropped`. Without this, the engine walks past the last slot expecting more
        // siblings → garbage object → corrupted vtable vcall (C000001D at skin time).
        // GEOM's direct children: walk from gi+1, advancing by (u4+1) per child, within
        // GEOM's span; decrement each surviving direct child's u3 by `dropped`.
        let geom_span = descs[gi].u4 as usize;
        let geom_end = (gi + geom_span).min(n_desc - 1);
        let mut c = gi + 1;
        while c <= geom_end {
            if !remove[c] {
                descs[c].u3 = descs[c].u3.saturating_sub(dropped as u32);
            }
            // advance to next sibling: this child's subtree is (u4+1) rows.
            c += descs[c].u4 as usize + 1;
        }
    }

    // SEGM trim: the SEGM table has one record per geometry SEGMENT; the removed eye
    // slots' segments are the trailing `dropped` records (4 bytes each, the eye records
    // carry grp==removed-slot index + a non-zero bone remap). Trim them so nothing in
    // the skinning palette references the removed geometry.
    if let Some(seg_i) = descs.iter().position(|d| &d.tag == SEGM && d.size > 0) {
        let rec = 4usize;
        let recs = descs[seg_i].body.len() / rec;
        let keep = recs.saturating_sub(dropped);
        descs[seg_i].body.truncate(keep * rec);
        descs[seg_i].size = (keep * rec) as u32;
    }

    let u1 = container[8..12].to_vec();
    let u2 = container[12..16].to_vec();
    *container = rebuild_without(&u1, &u2, &mut descs, &remove);
    Ok(dropped)
}
