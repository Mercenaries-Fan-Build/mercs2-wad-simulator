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
use crate::ffcs::read_u32_le;

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
