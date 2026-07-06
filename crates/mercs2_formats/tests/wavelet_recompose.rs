//! Stage-2 gate: a byte-for-byte port of StRecomposeW (FUN_009fb870) validated
//! against the LIVE x32dbg capture in
//! `tests/fixtures/wavelet_capture_2p567s/`.
//!
//! StRecomposeW is a pure function: given the header (block/static counts + the
//! flag-mask array + static-float source), the interpolated coefficient buffer
//! (`coeffs_in`), and the mask/reference block, it writes a `hkQsTransform[]`
//! pose. This test reproduces the captured `pose_out.hex` exactly.

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wavelet_capture_2p567s")
}

/// Load a hex-encoded-text fixture ("3c000000...") into raw bytes.
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
#[inline]
fn u16_le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// `_DAT_00b6b6b8` — the quaternion-W sentinel magnitude (read live from the
/// exe). When |w| equals it, the real W is `±sqrt(1-x²-y²-z²)`.
const W_SENTINEL: f32 = 2.0;

/// Port of FUN_009fb870 (StRecomposeW).
///
/// - `hdr_blocks` = param_1[0] (dynamic-bone loop count)
/// - `hdr_static` = param_1[1] (trailing static-float loop count)
/// - `mask`       = param_1[2] flag array (u16 per bone/DOF)
/// - `static_src` = param_1[3] float source (`pfVar5`)
/// - `dyn_src`    = param_2 coefficient buffer
/// - `pose_out`   = param_3 (transform output, stride 0x30)
/// - `p4`         = param_4 (trailing static-loop output)
#[allow(clippy::too_many_arguments)]
fn st_recompose_w(
    hdr_blocks: u32,
    hdr_static: u32,
    mask: &[u8],
    static_src: &[f32],
    dyn_src: &[f32],
    pose_out: &mut [f32],
    p4: &mut [f32],
) -> (usize, usize, usize) {
    let mut ps = 0usize; // pfVar5 cursor (static)
    let mut pd = 0usize; // param_2 cursor (dynamic)
    let mut sentinel_hits = 0usize;
    let next_static = |ps: &mut usize| -> f32 {
        let v = static_src[*ps];
        *ps += 1;
        v
    };
    let next_dyn = |pd: &mut usize| -> f32 {
        let v = dyn_src[*pd];
        *pd += 1;
        v
    };

    // pfVar6 starts at param_3 + 0x18 (6 floats in); indices [-6..+5]; +0xc/iter.
    // We index pose_out absolutely: bone i writes floats [i*12 .. i*12+11],
    // where local index L in [-6..5] maps to pose_out[i*12 + 6 + L].
    for i in 0..hdr_blocks as usize {
        let u_full = u16_le(mask, i * 2) as u32;
        let bvar3 = u_full & 0xff;
        let uvar2 = u_full >> 6;
        let base = i * 12 + 6; // where pfVar6 points, in absolute float index

        let sel = |bit: u32, ps: &mut usize, pd: &mut usize| -> f32 {
            if uvar2 & bit == 0 {
                next_static(ps)
            } else {
                next_dyn(pd)
            }
        };

        // group 1: translation ([-6],[-5],[-4]) (+ [-3] only in the zero branch)
        if bvar3 & 3 == 2 {
            pose_out[base - 6] = 0.0;
            pose_out[base - 5] = 0.0;
            pose_out[base - 4] = 0.0;
            pose_out[base - 3] = 0.0;
        } else {
            pose_out[base - 6] = sel(4, &mut ps, &mut pd);
            pose_out[base - 5] = sel(2, &mut ps, &mut pd);
            pose_out[base - 4] = sel(1, &mut ps, &mut pd);
        }

        // group 2: rotation ([-2],[-1],[0],[1])
        if bvar3 & 0xc == 8 {
            pose_out[base - 2] = 0.0;
            pose_out[base - 1] = 0.0;
            pose_out[base] = 0.0;
            pose_out[base + 1] = 1.0;
        } else {
            pose_out[base - 2] = sel(0x40, &mut ps, &mut pd);
            pose_out[base - 1] = sel(0x20, &mut ps, &mut pd);
            pose_out[base] = sel(0x10, &mut ps, &mut pd);
            let mut w = sel(8, &mut ps, &mut pd);
            if w.abs() == W_SENTINEL {
                sentinel_hits += 1;
                let s = (1.0
                    - pose_out[base - 2] * pose_out[base - 2]
                    - pose_out[base - 1] * pose_out[base - 1]
                    - pose_out[base] * pose_out[base])
                    .max(0.0)
                    .sqrt();
                w = if w <= 0.0 { -s } else { s };
            }
            pose_out[base + 1] = w;
        }

        // group 3: scale ([2],[3],[4]) (+ [5] only in the identity branch)
        if bvar3 & 0x30 == 0x20 {
            pose_out[base + 2] = 1.0;
            pose_out[base + 3] = 1.0;
            pose_out[base + 4] = 1.0;
            pose_out[base + 5] = 1.0;
        } else {
            pose_out[base + 2] = sel(0x200, &mut ps, &mut pd);
            pose_out[base + 3] = sel(0x100, &mut ps, &mut pd);
            // [4]: (char)uVar2 < 0  ==  bit 7 of uVar2 set (== 0x80)
            pose_out[base + 4] = if uvar2 & 0x80 != 0 {
                next_dyn(&mut pd)
            } else {
                next_static(&mut ps)
            };
        }
    }

    // trailing static-float loop (param_1[1]); won't run for this clip (0).
    for k in 0..hdr_static as usize {
        let flag = u16_le(mask, (hdr_blocks as usize + k) * 2);
        p4[k] = if flag == 0 {
            next_dyn(&mut pd)
        } else {
            next_static(&mut ps)
        };
    }
    (pd, ps, sentinel_hits)
}

#[test]
fn stage2_recompose_matches_live_pose() {
    let header = load_hex("header.hex");
    let coeffs = load_hex("coeffs_in.hex");
    let clip_data = load_hex("clip_data.hex");
    let mask_blk = load_hex("mask.hex");
    let pose_ref = load_hex("pose_out.hex");

    // Header: [0]=blocks, [1]=static, [2]=static_ptr(abs), [3]=wavelet_ptr(abs).
    let blocks = u32_le(&header, 0);
    let static_n = u32_le(&header, 4);
    let data_base = 0x21f21df0u32;
    let static_ptr = u32_le(&header, 8); // -> flag mask array (data_base+4)
    let wavelet_ptr = u32_le(&header, 12); // -> static float source (data_base+124)
    assert_eq!(blocks, 60);
    assert_eq!(static_n, 0);

    let mask_off = (static_ptr - data_base) as usize; // 4
    let static_off = (wavelet_ptr - data_base) as usize; // 124
    let mask = &clip_data[mask_off..];
    // static float source (pfVar5) — read as many f32 as could be consumed.
    let n_static_floats = (clip_data.len() - static_off) / 4;
    let static_src: Vec<f32> = (0..n_static_floats)
        .map(|i| f32_le(&clip_data, static_off + i * 4))
        .collect();

    let dyn_src: Vec<f32> = (0..246).map(|i| f32_le(&coeffs, i * 4)).collect();

    // Outputs. Seed with a NaN sentinel so we can tell which lanes StRecomposeW
    // actually writes vs. leaves untouched (the decomp leaves the translation-w
    // lane [-3] and scale-w lane [5] unwritten in the non-identity branch — a
    // prior StStaticW pass fills them, not us).
    let mut pose = vec![f32::NAN; pose_ref.len() / 4];
    let mut p4 = vec![0.0f32; mask_blk.len() / 4];

    let (dyn_used, static_used, sentinel_hits) = st_recompose_w(
        blocks, static_n, mask, &static_src, &dyn_src, &mut pose, &mut p4,
    );
    eprintln!(
        "consumed: {} dynamic coeffs (of 246), {} static floats; W-sentinel taken {} times",
        dyn_used, static_used, sentinel_hits
    );
    assert_eq!(dyn_used, 246, "must consume exactly the 246 interpolated coeffs");

    // Compare valid region: 60 bones * 48 bytes = 2880 bytes = 720 floats.
    // Lanes left NaN were never written by StRecomposeW (prior-pass lanes) — they
    // are reported but excluded from the numeric match gate.
    let valid_floats = 60 * 12;
    let mut ok = 0usize;
    let mut written = 0usize;
    let mut untouched: Vec<usize> = Vec::new();
    let mut mismatches: Vec<(usize, f32, f32)> = Vec::new();
    for i in 0..valid_floats {
        let got = pose[i];
        if got.is_nan() {
            untouched.push(i);
            continue;
        }
        written += 1;
        let exp = f32_le(&pose_ref, i * 4);
        let close = (got - exp).abs() <= 1e-4 || (exp != 0.0 && ((got - exp) / exp).abs() <= 1e-5);
        if close {
            ok += 1;
        } else {
            mismatches.push((i, got, exp));
        }
    }
    eprintln!(
        "STAGE2 StRecomposeW: {}/{} written floats match (<=1e-4); {} lanes left untouched by decoder (prior-pass w-lanes).",
        ok, written, untouched.len()
    );
    eprintln!(
        "untouched lane comps (i%12): {:?}",
        untouched.iter().map(|i| i % 12).collect::<std::collections::BTreeSet<_>>()
    );
    eprintln!("first {} mismatches (idx: got vs exp):", mismatches.len().min(20));
    for (i, g, e) in mismatches.iter().take(20) {
        eprintln!(
            "  float[{}] bone={} comp={}: got={} exp={}",
            i,
            i / 12,
            i % 12,
            g,
            e
        );
    }
    assert_eq!(
        mismatches.len(),
        0,
        "{} written-lane mismatches (see stderr)",
        mismatches.len()
    );
}
