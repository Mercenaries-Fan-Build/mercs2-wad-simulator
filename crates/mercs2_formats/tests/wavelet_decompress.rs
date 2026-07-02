//! Stage-1 gate: port of the StDecompressW/dequant/inverse-wavelet tree
//! (FUN_009f5b90 -> FUN_009f54f0 -> FUN_009ff120/FUN_009fdd50/FUN_009fe5b0)
//! + the frame/frac interp (FUN_009f0ee0), validated against the LIVE capture.
//!
//! Goal: decode `clip_data.hex` at the captured frame-position and reproduce
//! `coeffs_in.hex` (246 interpolated coefficients that feed StRecomposeW).
//!
//! The clip-object runtime fields were read live from the paused process
//! (clip @0x21f21d90): numPoses=78, blockSize=8, DOFcount=246, maxBitWidth=11,
//! preserved=0, and the blob-relative section offsets
//!   +0x34=820 (off array)  +0x38=1804 (mult array)  +0x3c=2788 (bitwidth array)
//!   +0x48=3036 (block-index array, count +0x4c=10)  +0x50=3076 (quant data base)
//! All constants (2^-bw table @0xb6b808, rounding bias _DAT_00bea940=0.0) were
//! read live; the inverse-wavelet 8x8 basis was reconstructed from the live
//! constants at 0xb6b900 and matches INV_WAVELET_8 in src/anim.rs exactly.

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wavelet_capture_2p567s")
}
fn load_hex(name: &str) -> Vec<u8> {
    let s = std::fs::read_to_string(fixture_dir().join(name)).expect(name);
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}
#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

// Inverse-wavelet 8x8 basis (verified live, == src/anim.rs INV_WAVELET_8).
const INV_WAVELET_8: [[f32; 8]; 8] = [
    [1.0, -0.5, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0],
    [1.0, -0.25, 0.0625, -0.0625, 0.625, -0.125, 0.0, 0.0],
    [1.0, 0.0, 0.625, -0.125, -0.25, -0.25, 0.0, 0.0],
    [1.0, 0.25, 0.1875, -0.1875, -0.125, 0.75, -0.125, 0.0],
    [1.0, 0.5, -0.25, -0.25, 0.0, -0.25, -0.25, 0.0],
    [1.0, 0.5, -0.25, 0.25, 0.0, -0.125, 0.75, -0.125],
    [1.0, 0.5, -0.25, 0.75, 0.0, 0.0, -0.25, -0.25],
    [1.0, 0.5, -0.25, 0.75, 0.0, 0.0, -0.25, 0.75],
];

const ROUND_BIAS: f32 = 0.0; // _DAT_00bea940 (read live)

/// FUN_009fd810: per-DOF packed byte budget for one block.
fn bit_budget(block_size: usize, bw: u32, preserved: usize) -> usize {
    let v = (block_size as i32 - preserved as i32) * bw as i32 + 7;
    let add = if v < 0 { (v >> 31) & 7 } else { 0 };
    (((v + add) >> 3) as usize) + preserved * 4
}

/// FUN_009ff120 (general bw!=8,16 path): entropy-unpack one DOF's block into a
/// packed bit-stream of `n` bw-bit codes (bitmap-selected present vs fill),
/// preceded by `preserved` verbatim f32. Returns (packed_bytes, present_count,
/// advance) where advance = the exact quant-pointer delta (return of FUN_009ff120).
fn entropy_unpack(
    blob: &[u8],
    base: usize,
    block_size: usize,
    bw: u32,
    fill: u32,
    preserved: usize,
) -> (Vec<u8>, usize) {
    let budget = bit_budget(block_size, bw, preserved);
    let n = (((budget as i32 - preserved as i32 * 4) * 8) / bw as i32).max(0) as usize;
    let rd16 = |p: usize| -> u32 {
        if p + 2 <= blob.len() {
            (blob[p] as u32) | ((blob[p + 1] as u32) << 8)
        } else {
            0
        }
    };
    let mut out: Vec<u8> = Vec::new();
    // preserved verbatim dwords.
    for i in 0..preserved {
        let o = base + i * 4;
        out.extend_from_slice(blob.get(o..o + 4).unwrap_or(&[0, 0, 0, 0]));
    }
    let mask: u32 = (1u32 << bw) - 1;
    let mut li = base + preserved * 4; // bitmap byte pointer
    let bm_skip = (n + 7) >> 3;
    let mut reg = rd16(li + bm_skip);
    let mut word_ptr = li + bm_skip + 2;
    let mut avail: u32 = 0x10;
    let mut b_mask: u32 = 1;
    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    let mut present = 0usize;
    let mut obuf: Vec<u8> = Vec::new();
    for _ in 0..n {
        let bit = (li < blob.len()) && (blob[li] as u32 & b_mask) != 0;
        let code = if !bit {
            if avail < bw {
                reg |= rd16(word_ptr) << (avail & 0x1f);
                word_ptr += 2;
                avail += 0x10;
            }
            let v = (reg & 0xffff) & mask;
            reg >>= bw & 0x1f;
            avail -= bw;
            present += 1;
            v
        } else {
            fill
        };
        acc |= code << (acc_bits & 0x1f);
        let next = acc_bits + bw;
        if next > 0xf {
            obuf.push((acc & 0xff) as u8);
            obuf.push(((acc >> 8) & 0xff) as u8);
            acc >>= 0x10;
            acc_bits = next - 0x10;
        } else {
            acc_bits = next;
        }
        b_mask <<= 1;
        if b_mask == 0x100 {
            li += 1;
            b_mask = 1;
        }
    }
    if acc_bits != 0 {
        obuf.push((acc & 0xff) as u8);
        if acc_bits > 7 {
            obuf.push(((acc >> 8) & 0xff) as u8);
        }
    }
    out.extend_from_slice(&obuf);
    (out, present)
}

/// FUN_009ff120 return value: `(preserved*0x20 + 7 + bw*present + n) >> 3`.
fn entropy_advance(block_size: usize, bw: u32, preserved: usize, present: usize) -> usize {
    let budget = bit_budget(block_size, bw, preserved);
    let n = (((budget as i32 - preserved as i32 * 4) * 8) / bw as i32).max(0) as usize;
    (preserved * 0x20 + 7 + (bw as usize) * present + n) >> 3
}

/// FUN_009fdd50 (general path): dequant one DOF's block. Reads `preserved`
/// verbatim f32, then unpacks `n` bw-bit codes and maps
/// `value = ((float)code + ROUND_BIAS) * (2^-bw * mult) + off`.
fn dequant(stream: &[u8], block_size: usize, bw: u32, preserved: usize, mult: f32, off: f32) -> Vec<f32> {
    let mut out = vec![0.0f32; block_size];
    for i in 0..preserved {
        out[i] = f32_le(stream, i * 4);
    }
    let scale = mult * 2f32.powi(-(bw as i32));
    let n = block_size - preserved;
    let mut acc: u64 = 0;
    let mut nbits: u32 = 0;
    let mut bp = preserved * 4;
    let m: u64 = (1u64 << bw) - 1;
    for k in 0..n {
        while nbits < bw && bp < stream.len() {
            acc |= (stream[bp] as u64) << nbits;
            bp += 1;
            nbits += 8;
        }
        let v = (acc & m) as u32;
        acc >>= bw;
        nbits = nbits.saturating_sub(bw);
        out[preserved + k] = (v as f32 + ROUND_BIAS) * scale + off;
    }
    out
}

/// FUN_009fe5b0 base (block_size==8): 8x8 inverse-wavelet.
fn inverse8(coeffs: &[f32]) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    for (i, oi) in out.iter_mut().enumerate() {
        let mut s = 0.0f32;
        for j in 0..8 {
            s += INV_WAVELET_8[i][j] * coeffs[j];
        }
        *oi = s;
    }
    out
}

/// FUN_009f0ee0: `param_2` is a TIME in seconds. `g = (numPoses-1) * time /
/// duration`; `frame = ROUND(g)` clamped to numPoses-2; `frac = g - frame`.
/// (The decomp's `param_1[3]` that `g` divides by is the clip duration, not
/// blockSize — Ghidra mistyped the float field.)
fn frame_frac(time: f32, num_poses: usize, duration: f32) -> (usize, f32) {
    let g = (num_poses as f32 - 1.0) * (time / duration);
    let frame = g.round() as i64;
    let clamp = (num_poses as i64) - 2;
    if frame >= clamp {
        (clamp as usize, 1.0)
    } else {
        (frame.max(0) as usize, g - frame as f32)
    }
}

struct Decoded {
    // per-DOF: block_size values for the block that contains `frame`.
    per_dof_block: Vec<[f32; 8]>,
    block_base_frame: usize,
}

/// Decode the wavelet block containing `frame` for every DOF, in DOF order.
fn decode_block(
    blob: &[u8],
    num_d: usize,
    block_size: usize,
    max_bw: u32,
    preserved: usize,
    off_arr: usize,
    mult_arr: usize,
    bw_arr: usize,
    block_index: usize,
    quant_base: usize,
    frame: usize,
) -> Decoded {
    let blk = frame / block_size;
    let boff = u32_le(blob, block_index + blk * 4) as usize;
    let mut p = quant_base + boff;
    let mut per_dof_block = Vec::with_capacity(num_d);
    for d in 0..num_d {
        let bw = blob[bw_arr + d] as u32;
        let bw = if bw == 0 { max_bw } else { bw };
        let mult = f32_le(blob, mult_arr + d * 4);
        let off = f32_le(blob, off_arr + d * 4);
        // fill code = ROUND(-off * 2^bw / mult), clamped away from 2^bw.
        let ival = 1i64 << bw;
        let fill_u = if mult != 0.0 {
            (-off * ival as f32 / mult).round() as i64
        } else {
            0
        };
        let fill = (if fill_u == ival { ival - 1 } else { fill_u }) as u32 & ((1u32 << bw) - 1);
        let (stream, present) = entropy_unpack(blob, p, block_size, bw, fill, preserved);
        let coeffs = dequant(&stream, block_size, bw, preserved, mult, off);
        per_dof_block.push(inverse8(&coeffs));
        p += entropy_advance(block_size, bw, preserved, present);
    }
    Decoded {
        per_dof_block,
        block_base_frame: blk * block_size,
    }
}

#[test]
fn stage1_decompress_matches_coeffs_in() {
    let clip = load_hex("clip_data.hex");
    let coeffs_ref = load_hex("coeffs_in.hex");

    // Runtime clip fields (read live from clip @0x21f21d90).
    let num_poses = 78usize;
    let block_size = 8usize;
    let num_d = 246usize;
    let max_bw = 11u32;
    let preserved = 0usize;
    // blob-relative section offsets (all + data_base internally; blob == clip_data).
    let off_arr = 820usize; // clip+0x34
    let mult_arr = 1804usize; // clip+0x38
    let bw_arr = 2788usize; // clip+0x3c
    let block_index = 3036usize; // clip+0x48
    let quant_base = 3076usize; // clip+0x50

    let duration = 2.56923f32; // clip @+0x0c (read live)
    let time = f32::from_bits(0x3fc148a7); // ~1.5101 s (param_2)
    let (frame, frac) = frame_frac(time, num_poses, duration);
    let w0 = 1.0 - frac;
    let w1 = frac;
    eprintln!("frame={frame} frac={frac} (w0={w0}, w1={w1})");

    let dec = decode_block(
        &clip, num_d, block_size, max_bw, preserved, off_arr, mult_arr, bw_arr, block_index,
        quant_base, frame,
    );
    // frame and frame+1 both live in the same block here (frame%bs != bs-1).
    let i0 = frame - dec.block_base_frame;
    let i1 = (i0 + 1).min(block_size - 1);

    let mut got = vec![0.0f32; num_d];
    for d in 0..num_d {
        got[d] = dec.per_dof_block[d][i0] * w0 + dec.per_dof_block[d][i1] * w1;
    }

    let mut ok3 = 0usize; // <= 1e-3
    let mut ok4 = 0usize; // <= 1e-4
    let mut worst = 0.0f32;
    let mut mism: Vec<(usize, f32, f32)> = Vec::new();
    for d in 0..num_d {
        let exp = f32_le(&coeffs_ref, d * 4);
        let g = got[d];
        let ad = (g - exp).abs();
        worst = worst.max(ad);
        if ad <= 1e-3 {
            ok3 += 1;
        } else {
            mism.push((d, g, exp));
        }
        if ad <= 1e-4 {
            ok4 += 1;
        }
    }
    eprintln!(
        "STAGE1 decompress: {}/{} coeffs match (<=1e-3), {}/{} (<=1e-4), worst abs err={:.2e}",
        ok3, num_d, ok4, num_d, worst
    );
    for (d, g, e) in mism.iter().take(30) {
        eprintln!("  coeff[{d}]: got={g} exp={e}");
    }
    assert!(ok3 >= num_d, "{} coeff mismatches >1e-3 (see stderr)", mism.len());
}
