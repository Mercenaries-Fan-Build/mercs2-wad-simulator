//! Fix transposed FLOAT16 vertex positions in a converted model block.
//!
//! `ucfx_byteswap::apply_strm_vertex_fix` un-transposes the f16 pairs that the
//! generic u32 swap left swapped — but ONLY for streams whose decl is entirely
//! 2-byte (FLOAT16/SHORT). A character mesh's stream 0 is MIXED (the implicit
//! leading FLOAT16_4 position + UBYTE4 colour/blend), so it's skipped and the
//! **position** stays transposed: x↔y and z↔w swapped (the homogeneous `w=1.0`
//! lands in the z slot, height lands in x → a flat/degenerate mesh).
//!
//! This pass walks each STRM group's vertex buffer and, when the position is in
//! the transposed state (f16 `1.0` = `0x3C00` at +4 instead of +6), un-transposes
//! the position's two 4-byte groups per vertex, then recomputes the container CSUM.
//! Self-detecting: streams whose position already has `w=1.0` at +6 are untouched.

use mercs2_formats::crc32::crc32_mercs2;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
/// f16 `1.0` little-endian = bytes `00 3C`.
fn is_f16_one(b: &[u8], o: usize) -> bool {
    o + 2 <= b.len() && b[o] == 0x00 && b[o + 1] == 0x3C
}

/// Walk STRM groups in a UCFX container and un-transpose transposed positions.
/// Returns the number of vertex buffers fixed. Recomputes the trailing CSUM if any.
pub fn fix_container_vertices(container: &mut Vec<u8>) -> Result<usize, String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_base = rd_u32(container, 4) as usize;
    let n_desc = rd_u32(container, 16) as usize;

    // Collect descriptors: (tag, row_u0, body_size). A STRM sentinel has
    // row_u0 == 0xFFFFFFFF; its children (info/decl/data) follow until the next
    // sentinel.
    struct Desc {
        tag: [u8; 4],
        row_u0: u32,
        body_size: u32,
    }
    let mut descs = Vec::with_capacity(n_desc);
    for d in 0..n_desc {
        let o = 20 + d * 20;
        if o + 20 > container.len() {
            break;
        }
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&container[o..o + 4]);
        descs.push(Desc { tag, row_u0: rd_u32(container, o + 4), body_size: rd_u32(container, o + 8) });
    }

    // Find each STRM group's (info stride, data offset, data size).
    let mut fixes: Vec<(usize, usize, usize)> = Vec::new(); // (data_abs, stride, n_verts)
    let mut i = 0;
    while i < descs.len() {
        if &descs[i].tag != b"STRM" || descs[i].row_u0 != 0xFFFF_FFFF {
            i += 1;
            continue;
        }
        let mut stride: Option<usize> = None;
        let mut data: Option<(usize, usize)> = None;
        let mut j = i + 1;
        while j < descs.len() && descs[j].row_u0 != 0xFFFF_FFFF {
            let start = data_base + descs[j].row_u0 as usize;
            let size = descs[j].body_size as usize;
            match &descs[j].tag {
                b"info" if size >= 12 && start + 12 <= container.len() => {
                    stride = Some(rd_u32(container, start + 4) as usize);
                }
                b"data" if start + size <= container.len() => data = Some((start, size)),
                _ => {}
            }
            j += 1;
        }
        i = j;
        if let (Some(s), Some((doff, dsz))) = (stride, data) {
            if (6..=256).contains(&s) && dsz >= s {
                fixes.push((doff, s, dsz / s));
            }
        }
    }

    let mut fixed = 0usize;
    for (doff, stride, n_verts) in fixes {
        if n_verts == 0 {
            continue;
        }
        // Detect: position is FLOAT16_4 at vertex+0. Correct form has w=1.0 at +6.
        // Transposed form (the bug) has 1.0 at +4 (z slot) and not at +6.
        let v0 = doff;
        if is_f16_one(container, v0 + 6) {
            continue; // already correct
        }
        if !is_f16_one(container, v0 + 4) {
            continue; // not the known transposed signature — leave it
        }
        // Un-transpose the position's two 4-byte groups for every vertex:
        // swap the two f16 within bytes [0..4) and [4..8).
        for v in 0..n_verts {
            let p = doff + v * stride;
            if p + 8 > container.len() {
                break;
            }
            container.swap(p, p + 2);
            container.swap(p + 1, p + 3);
            container.swap(p + 4, p + 6);
            container.swap(p + 5, p + 7);
        }
        fixed += 1;
    }

    if fixed > 0 {
        let n = container.len();
        if n < 8 || &container[n - 8..n - 4] != b"CSUM" {
            return Err("container missing CSUM trailer".into());
        }
        let csum = crc32_mercs2(&container[..n - 8]);
        container[n - 4..n].copy_from_slice(&csum.to_le_bytes());
    }
    Ok(fixed)
}
