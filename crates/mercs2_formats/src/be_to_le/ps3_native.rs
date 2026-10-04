//! PS3 -> PC native re-interleave pass for the 1-to-1 compact-decl subset of DLC01.
//!
//! Scope (as cataloged in `docs/_descriptor_walker_oracle.md`): the six PS3
//! compact `decl` patterns whose Xbox-DOH oracle is **1-to-1** -- the five
//! `type_hash=0x7C569307` terrain patterns and the single `0x1602815C` lowres
//! pattern. These are the patterns that can be converted natively from PS3 bytes
//! alone, with no Xbox-DOH input, and whose PC output is byte-identical to the
//! Xbox-DOH -> PC conversion of the same block.
//!
//! The ambiguous mesh/foliage patterns (`03000802...`) are not in scope and fall
//! through to the existing `convert_decl_ps3_compact` loud-fail arm.

use crate::ffcs::read_u32_be;
use crate::tags::ChunkTag;
use crate::types;

/// A 1-to-1 PS3 compact-decl pattern + its Xbox-DOH oracle decl + the per-stream
/// element layout on the PS3 side.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ps3Pattern {
    pub ps3_decl: &'static [u8],
    pub ps3_stride: u16,
    pub xbox_decl: &'static [u8],
    pub ps3_elements: &'static [Ps3Element],
    #[allow(dead_code)]
    pub type_hash: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ps3Element {
    pub kind: Ps3ElemKind,
    pub ps3_offset: u16,
    pub pc_offset: u16,
    pub usage: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ps3ElemKind {
    PositionF16x3,
    PositionF16x4,
    D3dColor,
    UvF16x2,
    Dec3nNormal,
    Dec3nTangent,
}

pub(super) static PATTERNS: &[Ps3Pattern] = &[
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x04, 0x06, 0x03, 0x04, 0x06, 0x0a, 0x02, 0x01],
        ps3_stride: 14,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x18, 0x28, 0x86, 0x00, 0x0a, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x3, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::D3dColor, ps3_offset: 6, pc_offset: 8, usage: 10 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 10, pc_offset: 12, usage: 3 },
        ],
        type_hash: types::TYPE_HASH_TERRAIN_MESH,
    },
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x06, 0x06, 0x02, 0x01],
        ps3_stride: 10,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x3, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 6, pc_offset: 8, usage: 3 },
        ],
        type_hash: types::TYPE_HASH_LOWRES_TERRAIN,
    },
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x03, 0x06, 0x08, 0x02, 0x06, 0x0a, 0x02, 0x01],
        ps3_stride: 14,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x3, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::UvF16x2, ps3_offset: 6, pc_offset: 8, usage: 5 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 10, pc_offset: 12, usage: 3 },
        ],
        type_hash: types::TYPE_HASH_TERRAIN_MESH,
    },
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x03, 0x06, 0x08, 0x02, 0x04, 0x0a, 0x03, 0x04, 0x06, 0x0e, 0x02, 0x01],
        ps3_stride: 18,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x18, 0x28, 0x86, 0x00, 0x0a, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x10, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x3, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::UvF16x2, ps3_offset: 6, pc_offset: 8, usage: 5 },
            Ps3Element { kind: Ps3ElemKind::D3dColor, ps3_offset: 10, pc_offset: 12, usage: 10 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 14, pc_offset: 16, usage: 3 },
        ],
        type_hash: types::TYPE_HASH_TERRAIN_MESH,
    },
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x03, 0x06, 0x08, 0x02, 0x06, 0x0a, 0x02, 0x01, 0x03, 0x0e, 0x0e, 0x04],
        ps3_stride: 22,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x14, 0x00, 0x1a, 0x21, 0x87, 0x00, 0x06, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x4, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::UvF16x2, ps3_offset: 8, pc_offset: 8, usage: 5 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 12, pc_offset: 12, usage: 3 },
            Ps3Element { kind: Ps3ElemKind::Dec3nTangent, ps3_offset: 16, pc_offset: 20, usage: 6 },
        ],
        type_hash: types::TYPE_HASH_TERRAIN_MESH,
    },
    Ps3Pattern {
        ps3_decl: &[0x03, 0x00, 0x00, 0x03, 0x03, 0x06, 0x08, 0x02, 0x04, 0x0a, 0x03, 0x04, 0x06, 0x0e, 0x02, 0x01, 0x03, 0x12, 0x0e, 0x04],
        ps3_stride: 26,
        xbox_decl: &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x18, 0x28, 0x86, 0x00, 0x0a, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x10, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x14, 0x00, 0x1a, 0x21, 0x87, 0x00, 0x06, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ],
        ps3_elements: &[
            Ps3Element { kind: Ps3ElemKind::PositionF16x4, ps3_offset: 0, pc_offset: 0, usage: 0 },
            Ps3Element { kind: Ps3ElemKind::UvF16x2, ps3_offset: 8, pc_offset: 8, usage: 5 },
            Ps3Element { kind: Ps3ElemKind::D3dColor, ps3_offset: 12, pc_offset: 12, usage: 10 },
            Ps3Element { kind: Ps3ElemKind::Dec3nNormal, ps3_offset: 16, pc_offset: 16, usage: 3 },
            Ps3Element { kind: Ps3ElemKind::Dec3nTangent, ps3_offset: 20, pc_offset: 20, usage: 6 },
        ],
        type_hash: types::TYPE_HASH_TERRAIN_MESH,
    },
];

pub(super) fn match_pattern(ps3_decl: &[u8]) -> Option<&'static Ps3Pattern> {
    PATTERNS.iter().find(|p| p.ps3_decl == ps3_decl)
}

pub(super) fn pc_decl_for(pattern: &Ps3Pattern) -> Vec<u8> {
    super::convert::ps3_native_translate_xbox_decl(pattern.xbox_decl)
}

pub(super) fn pc_stride(pattern: &Ps3Pattern) -> usize {
    let mut m: usize = 8;
    for e in pattern.ps3_elements {
        let w = match e.kind {
            Ps3ElemKind::PositionF16x3 | Ps3ElemKind::PositionF16x4 => 0,
            Ps3ElemKind::D3dColor => 4,
            Ps3ElemKind::UvF16x2 => 4,
            Ps3ElemKind::Dec3nNormal | Ps3ElemKind::Dec3nTangent => 8,
        };
        if w > 0 {
            let end = e.pc_offset as usize + w;
            if end > m {
                m = end;
            }
        }
    }
    m
}

pub(super) fn transcode_strm_data(
    ps3_src: &[u8],
    pattern: &Ps3Pattern,
    n_verts: usize,
) -> Result<Vec<u8>, String> {
    let src_stride = pattern.ps3_stride as usize;
    let dst_stride = pc_stride(pattern);
    if ps3_src.len() < n_verts * src_stride {
        return Err(format!(
            "ps3_native: STRM data too short ({} < {}x{})",
            ps3_src.len(),
            n_verts,
            src_stride
        ));
    }
    let mut out = vec![0u8; n_verts * dst_stride];
    for v in 0..n_verts {
        let so = v * src_stride;
        let doff = v * dst_stride;
        for e in pattern.ps3_elements {
            let s = so + e.ps3_offset as usize;
            let d = doff + e.pc_offset as usize;
            match e.kind {
                Ps3ElemKind::PositionF16x3 => {
                    for i in 0..3 {
                        out[d + i * 2] = ps3_src[s + i * 2 + 1];
                        out[d + i * 2 + 1] = ps3_src[s + i * 2];
                    }
                    out[d + 6] = 0x00;
                    out[d + 7] = 0x3c;
                }
                Ps3ElemKind::PositionF16x4 => {
                    for i in 0..4 {
                        out[d + i * 2] = ps3_src[s + i * 2 + 1];
                        out[d + i * 2 + 1] = ps3_src[s + i * 2];
                    }
                }
                Ps3ElemKind::UvF16x2 => {
                    for i in 0..2 {
                        out[d + i * 2] = ps3_src[s + i * 2 + 1];
                        out[d + i * 2 + 1] = ps3_src[s + i * 2];
                    }
                }
                Ps3ElemKind::D3dColor => {
                    out[d] = ps3_src[s + 3];
                    out[d + 1] = ps3_src[s + 2];
                    out[d + 2] = ps3_src[s + 1];
                    out[d + 3] = ps3_src[s];
                }
                Ps3ElemKind::Dec3nNormal | Ps3ElemKind::Dec3nTangent => {
                    let u = u32::from_be_bytes([
                        ps3_src[s], ps3_src[s + 1], ps3_src[s + 2], ps3_src[s + 3],
                    ]);
                    let ten_ten_ten = matches!(e.kind, Ps3ElemKind::Dec3nTangent);
                    let half4 = super::convert::ps3_native_dec3n_to_half4_le(u, ten_ten_ten);
                    out[d..d + 8].copy_from_slice(&half4);
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn classify_container(container: &[u8], type_hash: u32) -> Option<()> {
    if type_hash != types::TYPE_HASH_TERRAIN_MESH && type_hash != types::TYPE_HASH_LOWRES_TERRAIN {
        return None;
    }
    if container.len() < 20 {
        return None;
    }
    let magic = &container[0..4];
    if magic != b"XFCU" {
        return None;
    }
    let data_area_off = read_u32_be(container, 4) as usize;
    let n_desc = read_u32_be(container, 16) as usize;
    let desc_end = 20 + n_desc * 20;
    let data_start = if data_area_off > 0 { data_area_off } else { desc_end };
    if data_start > container.len() {
        return None;
    }
    let mut any_strm_decl = false;
    for di in 0..n_desc {
        let row = 20 + di * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&container[row..row + 4]);
        tag.reverse();
        if &tag != b"decl" {
            continue;
        }
        let u0 = read_u32_be(container, row + 4) as usize;
        let sz = read_u32_be(container, row + 8) as usize;
        if u0 == 0xFFFF_FFFF || sz == 0 {
            continue;
        }
        let abs = data_start + u0;
        if abs + sz > container.len() {
            return None;
        }
        let body = &container[abs..abs + sz];
        if body.len() < 2 || body[0] != 0x03 || body[1] != 0x00 {
            return None;
        }
        if match_pattern(body).is_none() {
            return None;
        }
        any_strm_decl = true;
    }
    if any_strm_decl { Some(()) } else { None }
}

pub(super) fn convert_container_ps3(
    container: &[u8],
    _entry_idx: usize,
    type_hash: u32,
) -> Result<Vec<u8>, String> {
    let data_area_off = read_u32_be(container, 4) as usize;
    let unk_08 = read_u32_be(container, 8);
    let unk_0c = read_u32_be(container, 12);
    let n_desc = read_u32_be(container, 16) as usize;
    let desc_end = 20 + n_desc * 20;
    let data_start = if data_area_off > 0 { data_area_off } else { desc_end };
    if data_start > container.len() {
        return Err("ps3_native: data_start past container".into());
    }

    #[derive(Clone)]
    struct Desc {
        tag_le: [u8; 4],
        tag: ChunkTag,
        row_u0: u32,
        body_size: u32,
        row_u3: u32,
        row_u4: u32,
    }
    let mut descs: Vec<Desc> = Vec::with_capacity(n_desc);
    for di in 0..n_desc {
        let row = 20 + di * 20;
        let mut tb = [0u8; 4];
        tb.copy_from_slice(&container[row..row + 4]);
        tb.reverse();
        descs.push(Desc {
            tag_le: tb,
            tag: ChunkTag::from_bytes(tb),
            row_u0: read_u32_be(container, row + 4),
            body_size: read_u32_be(container, row + 8),
            row_u3: read_u32_be(container, row + 12),
            row_u4: read_u32_be(container, row + 16),
        });
    }

    let mut parent_group: Vec<ChunkTag> = vec![ChunkTag::Unknown(*b"????"); n_desc];
    {
        let mut cur = ChunkTag::Unknown(*b"????");
        for i in 0..n_desc {
            if descs[i].row_u0 == 0xFFFF_FFFF {
                cur = descs[i].tag;
            }
            parent_group[i] = cur;
        }
    }

    let mut new_body: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();

    for i in 0..n_desc {
        if descs[i].row_u0 != 0xFFFF_FFFF || descs[i].tag != ChunkTag::Strm {
            continue;
        }
        let mut info_idx = None;
        let mut decl_idx = None;
        let mut data_idx = None;
        let mut j = i + 1;
        while j < n_desc && descs[j].row_u0 != 0xFFFF_FFFF {
            match descs[j].tag {
                ChunkTag::Info => info_idx = info_idx.or(Some(j)),
                ChunkTag::Decl => decl_idx = decl_idx.or(Some(j)),
                ChunkTag::Data => data_idx = data_idx.or(Some(j)),
                _ => {}
            }
            j += 1;
        }
        let (Some(ii), Some(di), Some(vi)) = (info_idx, decl_idx, data_idx) else {
            continue;
        };
        let info_off = data_start + descs[ii].row_u0 as usize;
        let info_sz = descs[ii].body_size as usize;
        let decl_off = data_start + descs[di].row_u0 as usize;
        let decl_sz = descs[di].body_size as usize;
        let vdata_off = data_start + descs[vi].row_u0 as usize;
        let vdata_sz = descs[vi].body_size as usize;
        if info_off + info_sz > container.len()
            || decl_off + decl_sz > container.len()
            || vdata_off + vdata_sz > container.len()
        {
            return Err("ps3_native: STRM child body out of range".into());
        }
        let decl_body = &container[decl_off..decl_off + decl_sz];
        let pattern = match_pattern(decl_body).ok_or_else(|| {
            format!(
                "ps3_native: STRM decl pattern not in 1-to-1 catalogue (hex={})",
                decl_body.iter().map(|b| format!("{:02x}", b)).collect::<String>()
            )
        })?;
        if info_sz < 12 {
            return Err("ps3_native: STRM info body <12 bytes".into());
        }
        let be_flag = read_u32_be(container, info_off);
        let be_stride = read_u32_be(container, info_off + 4) as usize;
        let vcount = read_u32_be(container, info_off + 8) as usize;
        if be_stride != pattern.ps3_stride as usize {
            return Err(format!(
                "ps3_native: STRM info stride {} != pattern expected {}",
                be_stride, pattern.ps3_stride
            ));
        }
        let pc_decl = pc_decl_for(pattern);
        new_body.insert(di, pc_decl);

        let ps3_data = &container[vdata_off..vdata_off + vdata_sz];
        let pc_data = transcode_strm_data(ps3_data, pattern, vcount)?;
        new_body.insert(vi, pc_data);

        let pc_stride_bytes = pc_stride(pattern) as u32;
        let mut pc_info = vec![0u8; info_sz];
        if info_sz >= 12 {
            pc_info[0..4].copy_from_slice(&be_flag.to_le_bytes());
            pc_info[4..8].copy_from_slice(&pc_stride_bytes.to_le_bytes());
            pc_info[8..12].copy_from_slice(&(vcount as u32).to_le_bytes());
        }
        new_body.insert(ii, pc_info);
    }

    for di in 0..n_desc {
        if descs[di].row_u0 == 0xFFFF_FFFF {
            continue;
        }
        if new_body.contains_key(&di) {
            continue;
        }
        let abs = data_start + descs[di].row_u0 as usize;
        let sz = descs[di].body_size as usize;
        if abs + sz > container.len() {
            return Err("ps3_native: non-STRM body out of range".into());
        }
        let be_body = &container[abs..abs + sz];
        let group_tag = parent_group[di];
        let pc_body = super::convert::ps3_native_translate_body(
            be_body,
            descs[di].tag,
            descs[di].tag_le,
            group_tag,
            type_hash,
        )?;
        new_body.insert(di, pc_body);
    }

    let mut out: Vec<u8> = Vec::with_capacity(container.len());
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(data_start as u32).to_le_bytes());
    out.extend_from_slice(&unk_08.to_le_bytes());
    out.extend_from_slice(&unk_0c.to_le_bytes());
    out.extend_from_slice(&(n_desc as u32).to_le_bytes());
    for d in &descs {
        out.extend_from_slice(&d.tag_le);
        out.extend_from_slice(&d.row_u0.to_le_bytes());
        out.extend_from_slice(&d.body_size.to_le_bytes());
        out.extend_from_slice(&d.row_u3.to_le_bytes());
        out.extend_from_slice(&d.row_u4.to_le_bytes());
    }
    if data_start > out.len() {
        out.extend_from_slice(&container[out.len()..data_start]);
    }
    let mut order: Vec<usize> = (0..n_desc)
        .filter(|&i| descs[i].row_u0 != 0xFFFF_FFFF)
        .collect();
    order.sort_by_key(|&i| descs[i].row_u0);
    let base = out.len();
    for &di in &order {
        let bytes = new_body
            .get(&di)
            .ok_or_else(|| format!("ps3_native: descriptor {} missing body", di))?;
        let off_in_data = (out.len() - base) as u32;
        let rf = 20 + di * 20;
        out[rf + 4..rf + 8].copy_from_slice(&off_in_data.to_le_bytes());
        out[rf + 8..rf + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }

    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;

    // Each pattern's PC decl bytes must equal running `convert_decl` on the
    // sibling Xbox fetch-decl -- i.e. the output path the Xbox-DOH route takes.
    // These expected hex strings are also byte-identical to the PC decl that
    // apply_decl_translate writes for the same block when the Xbox DOH route
    // is driven.
    const PC_DECL_STRIDE14_GROUND: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x0a, 0x00,
        0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];
    const PC_DECL_LOWRES: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x10, 0x00, 0x03, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];
    const PC_DECL_STRIDE14_UV_N: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00,
        0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];
    const PC_DECL_STRIDE18: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00,
        0x00, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x0a, 0x00,
        0x00, 0x00, 0x10, 0x00, 0x10, 0x00, 0x03, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];
    const PC_DECL_STRIDE22: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00,
        0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x14, 0x00, 0x10, 0x00, 0x06, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];
    // TANGENT PC offset = 24 (NORMAL ending auto-increment), not Xbox's `a` 20.
    const PC_DECL_STRIDE26: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00,
        0x00, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x0a, 0x00,
        0x00, 0x00, 0x10, 0x00, 0x10, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x18, 0x00, 0x10, 0x00, 0x06, 0x00,
        0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
    ];

    fn pc_decl_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn pattern_catalogue_pc_decls_match_oracle() {
        let expected: &[(&[u8], &[u8])] = &[
            (PATTERNS[0].ps3_decl, PC_DECL_STRIDE14_GROUND),
            (PATTERNS[1].ps3_decl, PC_DECL_LOWRES),
            (PATTERNS[2].ps3_decl, PC_DECL_STRIDE14_UV_N),
            (PATTERNS[3].ps3_decl, PC_DECL_STRIDE18),
            (PATTERNS[4].ps3_decl, PC_DECL_STRIDE22),
            (PATTERNS[5].ps3_decl, PC_DECL_STRIDE26),
        ];
        for (ps3_bytes, want) in expected {
            let pat = match_pattern(ps3_bytes).expect("pattern must be in catalogue");
            let got = pc_decl_for(pat);
            assert_eq!(
                got.as_slice(),
                *want,
                "pattern {} pc_decl mismatch: got={} want={}",
                pc_decl_hex(ps3_bytes),
                pc_decl_hex(&got),
                pc_decl_hex(want)
            );
        }
    }

    #[test]
    fn pattern_catalogue_covers_six_1to1_entries() {
        assert_eq!(PATTERNS.len(), 6);
    }

    // Vertex 0 of DLC01 terrain_r00_c00 STRM 0 is a stride-14 ground vertex
    // whose raw PS3 bytes decode as POS=(50, -1.21, -50), the SW corner of
    // the 100x100 terrain quad. The native transcode must widen POS to F16x4
    // with W=1.0, reverse D3DCOLOR to PC byte order, and decode NORMAL from
    // DEC3N packed `0x001ff000`.
    #[test]
    fn transcode_stride14_ground_vertex_byte_shape() {
        let ps3_vertex: [u8; 14] = [
            0x52, 0x40, 0xd0, 0xd6, 0xd2, 0x40, 0xfe, 0x00, 0x00, 0x00, 0x00, 0x1f, 0xf0, 0x00,
        ];
        let pattern = match_pattern(PATTERNS[0].ps3_decl).unwrap();
        let out = transcode_strm_data(&ps3_vertex, pattern, 1).unwrap();
        assert_eq!(out.len(), 20);
        // POS LE F16x4 = byte-swapped PS3 F16x3 + W=1.0 (0x3C00 LE).
        assert_eq!(&out[0..2], &[0x40, 0x52]);
        assert_eq!(&out[2..4], &[0xd6, 0xd0]);
        assert_eq!(&out[4..6], &[0x40, 0xd2]);
        assert_eq!(&out[6..8], &[0x00, 0x3c]);
        // COLOR = PS3 BE reversed to PC order.
        assert_eq!(&out[8..12], &[0x00, 0x00, 0x00, 0xfe]);
        // NORMAL = DEC3N 0x001ff000 decoded as HEND3N (11-11-10). The decoded
        // vector is close to +Y up (~0, 1, 0); after f32->f16 the W component
        // is exactly 1.0 (0x3c00 LE).
        assert_eq!(&out[18..20], &[0x00, 0x3c]);
    }

    #[test]
    fn classify_rejects_xbox_terrain_container() {
        // An Xbox fetch-decl starts with four zero bytes (`a` of the 12B
        // header), never with `03 00`. Build a minimal Xbox-shaped container.
        let mut c = Vec::new();
        c.extend_from_slice(b"XFCU");
        c.extend_from_slice(&0u32.to_be_bytes()); // data_area_off = 0
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes()); // n_desc
        assert!(classify_container(&c, types::TYPE_HASH_TERRAIN_MESH).is_none());
    }

    #[test]
    fn classify_rejects_non_terrain_type_hash() {
        let mut c = Vec::new();
        c.extend_from_slice(b"XFCU");
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes());
        assert!(classify_container(&c, 0xDEADBEEF).is_none());
    }
}
