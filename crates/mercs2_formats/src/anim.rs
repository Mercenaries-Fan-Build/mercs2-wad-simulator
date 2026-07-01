//! Havok 5.5.0-r1 **animation-clip decoder** — turns a serialized `hkaAnimation`
//! packfile into a sampleable per-bone local pose for the native engine.
//!
//! This is the read side that pairs with [`crate::havok`]: it reuses that
//! module's tested little-endian packfile walker
//! ([`havok::parse_packfile_raw`]) for the section-header / classname / fixup
//! pass, and adds decoders for the *animation* virtual-fixup classes (the
//! collision decoder in `havok.rs` handles the `hkp*Shape` classes). Do not
//! re-implement the walker here.
//!
//! ## Coordinate system / handedness (read this before integrating)
//! Values are returned **verbatim from Havok**, with **no coordinate or
//! handedness conversion applied**:
//! - Havok is **right-handed, +Y up, metres**; rotations are unit quaternions
//!   `(x, y, z, w)`; the reference/local frame is bone-parent-relative.
//! - Mercenaries-2 game space is **left-handed, +Y up** (see
//!   `docs/coordinate_systems.md`, per the modernization charter). The engine
//!   integrator is responsible for the RH→LH conversion (typically negate one
//!   axis of translation and the matching quaternion components, or flip Z).
//! - [`QsTransform`] is Havok's `hkQsTransform`: `translation` and `scale` are
//!   `hkVector4` truncated to xyz (the w lane is ignored), `rotation` is
//!   `hkQuaternion` xyzw. Compose as `parent * (T * R * S)` in Havok convention.
//!
//! ## Supported animation types
//! - `hkaInterleavedUncompressedAnimation` — fully decoded. `m_transforms` is a
//!   flat `hkQsTransform[numFrames * numTracks]` laid out frame-major
//!   (`frame f, track t → transforms[f * numTracks + t]`). Sampling is exact:
//!   per-track linear-interp of T/S and slerp of R between the two bracketing
//!   frames. **This is the faithful, verified path.**
//! - `hkaWaveletSkeletalAnimation` / `hkaDeltaCompressed*` — the *header* is
//!   decoded faithfully (duration, track counts, pose count = frame count, and
//!   the full wavelet quantization descriptor), but **per-frame decompression
//!   is not implemented**: the inverse-wavelet + dequantization + block
//!   reconstruction is proprietary Havok code that is not present in this
//!   workspace's decompilation and is not publicly documented. For these clips
//!   [`AnimClip::sample_local`] returns the neutral pose (identity rotation,
//!   zero translation, unit scale) for every track, and [`AnimClip::decoded`]
//!   is `false`. See the module-level report / MEMORY for the blocking detail.
//!
//! Layout facts are cross-checked against the golden fixture
//! `tests/fixtures/anim_ks750_le.bin` and against the BE→LE converter's class
//! registry in `crates/ucfx_byteswap/src/havok.rs` (the swap-width oracle).

use crate::havok::{parse_packfile_raw, RawPackfile, HAVOK_MAGIC};

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0
    }
}

#[inline]
fn i32_le(b: &[u8], o: usize) -> i32 {
    u32_le(b, o) as i32
}

#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    if o + 4 <= b.len() {
        f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0.0
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Havok `hkQsTransform` — a rigid+scale local transform (48 bytes on disk:
/// three `hkVector4`s). `rotation` is a unit quaternion `(x, y, z, w)`. Values
/// are raw Havok (right-handed); see the module header for handedness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QsTransform {
    pub translation: [f32; 3],
    /// Unit quaternion, `(x, y, z, w)` order.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl QsTransform {
    /// The neutral transform: no translation, identity rotation, unit scale.
    pub const IDENTITY: QsTransform = QsTransform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    };

    /// Read a `hkQsTransform` (48 bytes) at absolute offset `o`.
    fn read(b: &[u8], o: usize) -> QsTransform {
        QsTransform {
            translation: [f32_le(b, o), f32_le(b, o + 4), f32_le(b, o + 8)],
            rotation: [
                f32_le(b, o + 16),
                f32_le(b, o + 20),
                f32_le(b, o + 24),
                f32_le(b, o + 28),
            ],
            scale: [f32_le(b, o + 32), f32_le(b, o + 36), f32_le(b, o + 40)],
        }
    }
}

/// Which serialized Havok animation class the clip came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimType {
    /// `hkaInterleavedUncompressedAnimation` — decoded exactly.
    Interleaved,
    /// `hkaWaveletSkeletalAnimation` — header decoded, frames not (proprietary).
    Wavelet,
    /// `hkaDeltaCompressedSkeletalAnimation` — header decoded, frames not.
    Delta,
    /// `hkaSplineCompressedAnimation` — header decoded, frames not.
    Spline,
}

/// A decoded animation clip: enough to sample a local per-track pose over time.
#[derive(Debug, Clone)]
pub struct AnimClip {
    /// Source animation class.
    pub anim_type: AnimType,
    /// Clip length in seconds (`hkaAnimation::m_duration`).
    pub duration: f32,
    /// Number of transform tracks (`m_numberOfTransformTracks`).
    pub num_tracks: usize,
    /// Number of key frames / poses. For interleaved this is
    /// `m_transforms.len() / num_tracks`; for compressed it is
    /// `m_numberOfPoses`.
    pub num_frames: usize,
    /// `hkaAnimationBinding::m_transformTrackToBoneIndices`: animation track
    /// index → skeleton bone index. Empty if there is no binding (identity map).
    pub track_to_bone: Vec<i16>,
    /// `true` if per-frame transforms were actually decoded (interleaved only).
    /// `false` for the compressed classes — [`Self::sample_local`] then returns
    /// the neutral pose. Check this before trusting sampled motion.
    pub decoded: bool,
    /// Frame-major key transforms `[frame][track]`, present only when
    /// [`Self::decoded`]. Length `num_frames * num_tracks` in flat order.
    frames: Vec<QsTransform>,
}

impl AnimClip {
    /// Frame `f`, track `t` (both in range) → its stored transform.
    #[inline]
    fn frame(&self, f: usize, t: usize) -> QsTransform {
        self.frames[f * self.num_tracks + t]
    }

    /// The neutral per-track pose (identity) — the honest result for a clip
    /// whose frames could not be decoded.
    fn neutral_pose(&self) -> Vec<QsTransform> {
        vec![QsTransform::IDENTITY; self.num_tracks]
    }

    /// Local per-track pose at `time` seconds, linearly interpolated between the
    /// two bracketing key frames (translation/scale lerp, rotation slerp).
    /// Returns exactly [`Self::num_tracks`] transforms.
    ///
    /// Time is clamped to `[0, duration]`. For a non-decoded (compressed) clip
    /// this returns the neutral pose — see [`Self::decoded`].
    pub fn sample_local(&self, time: f32) -> Vec<QsTransform> {
        if !self.decoded || self.num_frames == 0 || self.num_tracks == 0 {
            return self.neutral_pose();
        }
        if self.num_frames == 1 {
            return (0..self.num_tracks).map(|t| self.frame(0, t)).collect();
        }
        // Uniform time-line: frame i sits at t = i * duration / (num_frames-1).
        let last = self.num_frames - 1;
        let t = time.clamp(0.0, self.duration);
        let step = if self.duration > 0.0 {
            self.duration / last as f32
        } else {
            0.0
        };
        let (f0, frac) = if step > 0.0 {
            let g = t / step;
            let f0 = (g.floor() as usize).min(last);
            (f0, g - f0 as f32)
        } else {
            (0usize, 0.0)
        };
        let f1 = (f0 + 1).min(last);
        (0..self.num_tracks)
            .map(|tr| lerp_qs(self.frame(f0, tr), self.frame(f1, tr), frac))
            .collect()
    }
}

/// Linear/slerp interpolation of two `hkQsTransform`s (`a` at frac 0, `b` at 1).
fn lerp_qs(a: QsTransform, b: QsTransform, frac: f32) -> QsTransform {
    let f = frac.clamp(0.0, 1.0);
    let lerp3 = |x: [f32; 3], y: [f32; 3]| {
        [
            x[0] + (y[0] - x[0]) * f,
            x[1] + (y[1] - x[1]) * f,
            x[2] + (y[2] - x[2]) * f,
        ]
    };
    QsTransform {
        translation: lerp3(a.translation, b.translation),
        rotation: slerp(a.rotation, b.rotation, f),
        scale: lerp3(a.scale, b.scale),
    }
}

/// Spherical-linear interpolation of two `(x,y,z,w)` unit quaternions, taking
/// the shorter arc. Falls back to normalized-lerp for nearly-parallel inputs.
fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        for c in b.iter_mut() {
            *c = -*c;
        }
        dot = -dot;
    }
    // Near-parallel: normalized linear interpolation avoids a divide-by-~0.
    if dot > 0.9995 {
        let mut q = [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ];
        normalize4(&mut q);
        return q;
    }
    let theta0 = dot.clamp(-1.0, 1.0).acos();
    let theta = theta0 * t;
    let sin0 = theta0.sin();
    let s0 = ((1.0 - t) * theta0).sin() / sin0;
    let s1 = theta.sin() / sin0;
    [
        a[0] * s0 + b[0] * s1,
        a[1] * s0 + b[1] * s1,
        a[2] * s0 + b[2] * s1,
        a[3] * s0 + b[3] * s1,
    ]
}

fn normalize4(q: &mut [f32; 4]) {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 0.0 {
        for c in q.iter_mut() {
            *c /= n;
        }
    }
}

// ── serialized layouts (HK 5.5.0-r1, 32-bit LE) ──────────────────────────────
//
// hkReferencedObject header occupies +0..+7 (vtable ptr @0, memSizeAndFlags u16
// @4, referenceCount u16 @6). hkaAnimation base (verified against the SDK header
// and the BE→LE converter's `HKA_INTERLEAVED_SWAP`, where the first derived
// array — m_transforms — sits at +36):
//   +8  m_type (hkEnum<AnimationType, int>)
//   +12 m_duration (hkReal)
//   +16 m_numberOfTransformTracks (int)
//   +20 m_numberOfFloatTracks (int)
//   +24 m_extractedMotion (ptr)
//   +28 m_annotationTracks (ptr)  +32 m_numAnnotationTracks (int)   -> base ends +36
const OFF_TYPE: usize = 8;
const OFF_DURATION: usize = 12;
const OFF_NUM_TRANSFORM_TRACKS: usize = 16;
// hkaInterleavedUncompressedAnimation derived members:
//   +36 m_transforms.ptr  +40 m_transforms.size  +44 m_transforms.capAndFlags
const OFF_INTERLEAVED_TRANSFORMS_PTR: usize = 36;
const OFF_INTERLEAVED_TRANSFORMS_SIZE: usize = 40;
const QS_TRANSFORM_SIZE: usize = 48;
// hkaWaveletSkeletalAnimation derived members (see module report):
//   +36 m_numberOfPoses (int)
const OFF_WAVELET_NUM_POSES: usize = 36;

/// Read `hkaAnimationBinding::m_transformTrackToBoneIndices` if a binding object
/// is present. In this serialization the binding's first hkArray (an int16
/// array) is at object offset +4 (ptr) / +8 (size); the pointer is relocated by
/// a local fixup.
fn read_binding_track_to_bone(pk: &[u8], raw: &RawPackfile, src: usize) -> Vec<i16> {
    // m_transformTrackToBoneIndices: ptr @ +4, size @ +8 (hkArray, int16 elems).
    let size = i32_le(pk, raw.obj_abs(src) + 8).max(0) as usize;
    if size == 0 || size > 0x0010_0000 {
        return Vec::new();
    }
    let base = match raw.resolve_ptr(src, 4) {
        Some(b) => b,
        None => return Vec::new(),
    };
    (0..size)
        .map(|i| {
            let o = base + i * 2;
            if o + 2 <= pk.len() {
                i16::from_le_bytes([pk[o], pk[o + 1]])
            } else {
                0
            }
        })
        .collect()
}

// ── wavelet decompression (faithful port of hk_anim/wavelet.py) ──────────────
//
// HK550 32-bit layout offsets (from wavelet struct start). Matches the Python
// `_OFF_*` / `_WAVELET_STRUCT_SIZE` constants byte-for-byte.
const W_OFF_ANIM_TYPE: usize = 8;
const W_OFF_DURATION: usize = 12;
const W_OFF_NUM_TT: usize = 16;
const W_OFF_NUM_FT: usize = 20;
const W_OFF_NUM_POSES: usize = 36;
const W_OFF_BLOCK_SIZE: usize = 40;
const W_OFF_QFMT: usize = 44; // 20-byte QuantizationFormat
const W_OFF_STATIC_MASK_IDX: usize = 64;
const W_OFF_STATIC_DOFS_IDX: usize = 68;
const W_OFF_BLOCK_INDEX_IDX: usize = 72;
const W_OFF_BLOCK_INDEX_SIZE: usize = 76;
const W_OFF_QUANT_DATA_IDX: usize = 80;
const W_OFF_QUANT_DATA_SIZE: usize = 84;
const W_OFF_NUM_DATA_BUFFER: usize = 92;
const WAVELET_STRUCT_SIZE: usize = 96;

// TrackType: DYNAMIC=0, STATIC=1, IDENTITY=2 (2-bit fields in the static mask).
// MaskBit sub-track bit indices (from _decompress_common.MaskBit):
const MB_POS_Z: u32 = 6;
const MB_POS_Y: u32 = 7;
const MB_POS_X: u32 = 8;
const MB_ROT_W: u32 = 9;
const MB_ROT_Z: u32 = 10;
const MB_ROT_Y: u32 = 11;
const MB_ROT_X: u32 = 12;
const MB_SCALE_Z: u32 = 13;
const MB_SCALE_Y: u32 = 14;
const MB_SCALE_X: u32 = 15;

#[inline]
fn sm_pos_type(raw: u16) -> u32 {
    (raw as u32) & 3
}
#[inline]
fn sm_rot_type(raw: u16) -> u32 {
    ((raw as u32) >> 2) & 3
}
#[inline]
fn sm_scale_type(raw: u16) -> u32 {
    ((raw as u32) >> 4) & 3
}
#[inline]
fn sm_use_sub(raw: u16, bit: u32) -> bool {
    (raw & (1 << bit)) != 0
}

/// Extract `n_values` unsigned integers of `bit_width` bits from packed `buf`
/// starting at `bit_offset`. Mirrors `dequantize_bitstream`.
fn dequantize_bitstream(buf: &[u8], bit_offset: usize, bit_width: u32, n_values: usize) -> Vec<u32> {
    if bit_width == 0 {
        return vec![0; n_values];
    }
    let mask: u32 = if bit_width >= 32 {
        u32::MAX
    } else {
        (1u32 << bit_width) - 1
    };
    let mut out = Vec::with_capacity(n_values);
    let mut cur_bit = bit_offset;
    for _ in 0..n_values {
        let mut byte_idx = cur_bit >> 3;
        let mut bit_in_byte = (cur_bit & 7) as u32;
        let mut val: u32 = 0;
        let mut bits_read: u32 = 0;
        while bits_read < bit_width {
            if byte_idx >= buf.len() {
                break;
            }
            let available = 8 - bit_in_byte;
            let need = bit_width - bits_read;
            let take = available.min(need);
            let chunk = ((buf[byte_idx] as u32) >> bit_in_byte) & ((1u32 << take) - 1);
            val |= chunk << bits_read;
            bits_read += take;
            byte_idx += 1;
            bit_in_byte = 0;
        }
        out.push(val & mask);
        cur_bit += bit_width as usize;
    }
    out
}

/// Convert quantized integers to floats: `offset + q * scale * fractal`.
fn dequantize_values(quantized: &[u32], offset: f32, scale: f32, bit_width: u32) -> Vec<f32> {
    if bit_width == 0 {
        return vec![offset; quantized.len()];
    }
    let fractal = 1.0f32 / (((1u32 << bit_width) - 1) as f32);
    quantized
        .iter()
        .map(|&q| offset + (q as f32) * scale * fractal)
        .collect()
}

/// Reconstruct quaternion W when Havok stores the `±2` sentinel.
fn fix_quat_w_sentinel(qx: f32, qy: f32, qz: f32, qw: f32) -> (f32, f32, f32, f32) {
    if qw == 2.0 || qw == -2.0 {
        let basis = qw * 0.5; // ±1
        let w_sq = (1.0 - qx * qx - qy * qy - qz * qz).max(0.0);
        return (qx, qy, qz, basis * w_sq.sqrt());
    }
    (qx, qy, qz, qw)
}

/// Normalize quaternion components (indices 3..=6) in place, honoring the
/// ±2 W sentinel. Mirrors `_normalize_quat_inplace`.
fn normalize_quat_inplace(vals: &mut [f32; 10]) {
    let (qx, qy, qz, qw) = fix_quat_w_sentinel(vals[3], vals[4], vals[5], vals[6]);
    vals[3] = qx;
    vals[4] = qy;
    vals[5] = qz;
    vals[6] = qw;
    let qlen = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
    if qlen > 1e-8 {
        let inv = 1.0 / qlen;
        vals[3] *= inv;
        vals[4] *= inv;
        vals[5] *= inv;
        vals[6] *= inv;
    } else {
        vals[3] = 0.0;
        vals[4] = 0.0;
        vals[5] = 0.0;
        vals[6] = 1.0;
    }
}

/// Inverse Haar wavelet (lifting) transform. `coeffs` has `n` entries:
/// [average, detail_level0, detail_level1, ...]. Reconstructs `n` samples.
fn inverse_haar(coeffs: &[f32], n: usize) -> Vec<f32> {
    if n <= 1 {
        return coeffs[..n.min(coeffs.len())].to_vec();
    }
    let mut vals: Vec<f32> = coeffs[..n].to_vec();
    let mut level = 1usize;
    while level < n {
        let mut tmp = vals.clone();
        for i in 0..level {
            let a = vals[i];
            let d = if level + i < n { vals[level + i] } else { 0.0 };
            tmp[2 * i] = a + d;
            if 2 * i + 1 < n {
                tmp[2 * i + 1] = a - d;
            }
        }
        vals = tmp;
        level *= 2;
    }
    vals.truncate(n);
    vals
}

/// Locate the wavelet animation struct by scanning for type=3 + plausible
/// duration. Faithful port of `_find_wavelet_struct`.
fn find_wavelet_struct(blob: &[u8]) -> Option<usize> {
    if blob.len() < WAVELET_STRUCT_SIZE {
        return None;
    }
    let limit = (blob.len() - WAVELET_STRUCT_SIZE).min(4096);
    let mut off = 0usize;
    while off < limit {
        let t = u32_le(blob, off + W_OFF_ANIM_TYPE);
        if t != 3 {
            off += 4;
            continue;
        }
        let d = f32_le(blob, off + W_OFF_DURATION);
        if !(d.is_finite() && (0.001..=600.0).contains(&d)) {
            off += 4;
            continue;
        }
        let ntt = u32_le(blob, off + W_OFF_NUM_TT);
        if !(1..=500).contains(&ntt) {
            off += 4;
            continue;
        }
        let bs = u32_le(blob, off + W_OFF_BLOCK_SIZE);
        if !matches!(bs, 2 | 4 | 8 | 16 | 32 | 64) {
            off += 4;
            continue;
        }
        return Some(off);
    }
    None
}

/// Reconstructed wavelet clip: per-frame per-track 10-tuples (tx,ty,tz, qx,qy,qz,qw, sx,sy,sz).
struct WaveletDecoded {
    duration: f32,
    n_tt: usize,
    frames: Vec<Vec<[f32; 10]>>,
}

/// Decode a `hkaWaveletSkeletalAnimation` from packfile bytes. Faithful port of
/// `hk_anim/wavelet.py::decode_wavelet`.
fn decode_wavelet(blob: &[u8]) -> Option<WaveletDecoded> {
    let struct_off = find_wavelet_struct(blob)?;

    let dur = f32_le(blob, struct_off + W_OFF_DURATION);
    let n_tt = u32_le(blob, struct_off + W_OFF_NUM_TT) as usize;
    let _n_ft = u32_le(blob, struct_off + W_OFF_NUM_FT) as usize;
    let n_poses = u32_le(blob, struct_off + W_OFF_NUM_POSES) as usize;
    let block_size = u32_le(blob, struct_off + W_OFF_BLOCK_SIZE) as usize;

    let _max_bw = *blob.get(struct_off + W_OFF_QFMT)? as u32;
    let preserved = *blob.get(struct_off + W_OFF_QFMT + 1)? as usize;
    let num_d = u32_le(blob, struct_off + W_OFF_QFMT + 4) as usize;
    let offset_idx = u32_le(blob, struct_off + W_OFF_QFMT + 8) as usize;
    let scale_idx = u32_le(blob, struct_off + W_OFF_QFMT + 12) as usize;
    let bw_idx = u32_le(blob, struct_off + W_OFF_QFMT + 16) as usize;

    let sm_idx = u32_le(blob, struct_off + W_OFF_STATIC_MASK_IDX) as usize;
    let sd_idx = u32_le(blob, struct_off + W_OFF_STATIC_DOFS_IDX) as usize;
    let bi_idx = u32_le(blob, struct_off + W_OFF_BLOCK_INDEX_IDX) as usize;
    let bi_size = u32_le(blob, struct_off + W_OFF_BLOCK_INDEX_SIZE) as usize;
    let qd_idx = u32_le(blob, struct_off + W_OFF_QUANT_DATA_IDX) as usize;
    let _qd_size = u32_le(blob, struct_off + W_OFF_QUANT_DATA_SIZE) as usize;
    let _num_data_buf = u32_le(blob, struct_off + W_OFF_NUM_DATA_BUFFER) as usize;

    if dur <= 0.0 || n_tt == 0 {
        return None;
    }

    let db_base = struct_off + WAVELET_STRUCT_SIZE;

    // --- Static masks (u16 each) ---
    let masks: Vec<u16> = (0..n_tt)
        .map(|i| {
            let o = db_base + sm_idx + i * 2;
            if o + 2 <= blob.len() {
                u16::from_le_bytes([blob[o], blob[o + 1]])
            } else {
                0
            }
        })
        .collect();

    // --- Read static DOF values (f32 each) ---
    let sd_start = db_base + sd_idx;
    let n_static_floats = (offset_idx.saturating_sub(sd_idx)) / 4;
    let static_dofs: Vec<f32> = (0..n_static_floats)
        .map(|i| f32_le(blob, sd_start + i * 4))
        .collect();

    // --- Per-dynamic-DOF offset/scale/bitWidth arrays ---
    let mut offsets = Vec::with_capacity(num_d);
    let mut scales = Vec::with_capacity(num_d);
    let mut bit_widths = Vec::with_capacity(num_d);
    for i in 0..num_d {
        offsets.push(f32_le(blob, db_base + offset_idx + i * 4));
        scales.push(f32_le(blob, db_base + scale_idx + i * 4));
    }
    for i in 0..num_d {
        bit_widths.push(*blob.get(db_base + bw_idx + i).unwrap_or(&0) as u32);
    }

    // --- Number of blocks ---
    let n_blocks = (n_poses + block_size - 1) / block_size;

    // --- Block index (byte offsets into quantized data, one per block) ---
    let block_offsets: Vec<usize> = if bi_size >= n_blocks {
        (0..n_blocks)
            .map(|i| u32_le(blob, db_base + bi_idx + i * 4) as usize)
            .collect()
    } else {
        vec![0; n_blocks]
    };

    let qd_start = db_base + qd_idx;

    // --- Static / identity rest-pose values per track (HavokLib static rules) ---
    let identity_vals: [f32; 10] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
    let mut static_trs_per_track: Vec<[f32; 10]> = Vec::with_capacity(n_tt);
    let mut sd_cursor = 0usize;
    let read_sd = |cursor: &mut usize| -> f32 {
        if *cursor >= static_dofs.len() {
            return 0.0;
        }
        let v = static_dofs[*cursor];
        *cursor += 1;
        v
    };

    for &m in masks.iter().take(n_tt) {
        let mut vals = identity_vals;
        // Position
        match sm_pos_type(m) {
            1 => {
                vals[0] = read_sd(&mut sd_cursor);
                vals[1] = read_sd(&mut sd_cursor);
                vals[2] = read_sd(&mut sd_cursor);
            }
            0 => {
                for (axis, bit) in [MB_POS_X, MB_POS_Y, MB_POS_Z].iter().enumerate() {
                    if !sm_use_sub(m, *bit) {
                        vals[axis] = read_sd(&mut sd_cursor);
                    }
                }
            }
            _ => {}
        }
        // Rotation
        match sm_rot_type(m) {
            1 => {
                vals[3] = read_sd(&mut sd_cursor);
                vals[4] = read_sd(&mut sd_cursor);
                vals[5] = read_sd(&mut sd_cursor);
                vals[6] = read_sd(&mut sd_cursor);
            }
            0 => {
                for (j, bit) in [MB_ROT_X, MB_ROT_Y, MB_ROT_Z, MB_ROT_W].iter().enumerate() {
                    if !sm_use_sub(m, *bit) {
                        vals[3 + j] = read_sd(&mut sd_cursor);
                    }
                }
            }
            2 => {
                vals[3] = 0.0;
                vals[4] = 0.0;
                vals[5] = 0.0;
                vals[6] = 1.0;
            }
            _ => {}
        }
        // Scale
        match sm_scale_type(m) {
            1 => {
                vals[7] = read_sd(&mut sd_cursor);
                vals[8] = read_sd(&mut sd_cursor);
                vals[9] = read_sd(&mut sd_cursor);
            }
            0 => {
                for (j, bit) in [MB_SCALE_X, MB_SCALE_Y, MB_SCALE_Z].iter().enumerate() {
                    if !sm_use_sub(m, *bit) {
                        vals[7 + j] = read_sd(&mut sd_cursor);
                    }
                }
            }
            2 => {
                vals[7] = 1.0;
                vals[8] = 1.0;
                vals[9] = 1.0;
            }
            _ => {}
        }

        normalize_quat_inplace(&mut vals);
        static_trs_per_track.push(vals);
    }

    // --- Mapping: dynamic DOF index -> (track, component) ---
    let mut dyn_dof_map: Vec<(usize, usize)> = Vec::with_capacity(num_d);
    for (ti, &m) in masks.iter().enumerate().take(n_tt) {
        if sm_pos_type(m) == 0 {
            for (j, b) in [MB_POS_X, MB_POS_Y, MB_POS_Z].iter().enumerate() {
                if sm_use_sub(m, *b) {
                    dyn_dof_map.push((ti, j));
                }
            }
        }
        if sm_rot_type(m) == 0 {
            for (j, b) in [MB_ROT_X, MB_ROT_Y, MB_ROT_Z, MB_ROT_W].iter().enumerate() {
                if sm_use_sub(m, *b) {
                    dyn_dof_map.push((ti, 3 + j));
                }
            }
        }
        if sm_scale_type(m) == 0 {
            for (j, b) in [MB_SCALE_X, MB_SCALE_Y, MB_SCALE_Z].iter().enumerate() {
                if sm_use_sub(m, *b) {
                    dyn_dof_map.push((ti, 7 + j));
                }
            }
        }
    }
    if dyn_dof_map.len() != num_d {
        return None;
    }

    // --- Per-block decode: interleave preserved f32 + packed quant stream per DOF ---
    let mut all_frames_trs: Vec<Vec<[f32; 10]>> = Vec::with_capacity(n_poses);
    let n_quant_per_dof = block_size - preserved;

    for blk in 0..n_blocks {
        let poses_in_block = block_size.min(n_poses - blk * block_size);

        let blk_byte_off = if blk < block_offsets.len() {
            block_offsets[blk]
        } else {
            0
        };

        let qd_blk_abs = qd_start + blk_byte_off;
        let mut bit_off = qd_blk_abs * 8;

        let mut frame_vals_per_dof: Vec<Vec<f32>> = Vec::with_capacity(num_d);
        for di in 0..num_d {
            bit_off = ((bit_off + 7) >> 3) << 3;
            let mut pv: Vec<f32> = Vec::with_capacity(preserved);
            for _pi in 0..preserved {
                let bidx = bit_off >> 3;
                if bidx + 4 <= blob.len() {
                    pv.push(f32_le(blob, bidx));
                } else {
                    pv.push(0.0);
                }
                bit_off += 32;
            }

            let bw = bit_widths[di];
            let dq: Vec<f32> = if n_quant_per_dof > 0 && bw > 0 {
                let quants = dequantize_bitstream(blob, bit_off, bw, n_quant_per_dof);
                let dq = dequantize_values(&quants, offsets[di], scales[di], bw);
                bit_off += (bw as usize) * n_quant_per_dof;
                dq
            } else {
                vec![0.0; n_quant_per_dof]
            };

            let mut coeffs: Vec<f32> = pv;
            coeffs.extend_from_slice(&dq);
            while coeffs.len() < block_size {
                coeffs.push(0.0);
            }

            let frame_values = inverse_haar(&coeffs, block_size);
            frame_vals_per_dof.push(frame_values[..poses_in_block].to_vec());
        }

        // Assemble TRS per frame in this block
        for fi in 0..poses_in_block {
            let mut frame_data = static_trs_per_track.clone();
            for di in 0..num_d {
                let (ti, ci) = dyn_dof_map[di];
                frame_data[ti][ci] = frame_vals_per_dof[di][fi];
            }
            for vals in frame_data.iter_mut() {
                normalize_quat_inplace(vals);
            }
            all_frames_trs.push(frame_data);
        }
    }

    Some(WaveletDecoded {
        duration: dur,
        n_tt,
        frames: all_frames_trs,
    })
}

/// Decode a Havok animation packfile into a sampleable [`AnimClip`].
///
/// `packfile` may start at (or before) the `__classnames__` section table, or at
/// the 8-byte Havok magic; the embedded packfile is located automatically.
pub fn parse_anim(packfile: &[u8]) -> Result<AnimClip, String> {
    // Accept a buffer that has junk before the magic (e.g. a chunk prefix).
    let start = find_sub(packfile, &HAVOK_MAGIC).unwrap_or(0);
    let pk = &packfile[start..];
    let raw = parse_packfile_raw(pk)?;

    // Locate the animation object and (optionally) its binding.
    let mut anim: Option<(usize, AnimType)> = None;
    let mut binding_src: Option<usize> = None;
    for (src, cname) in &raw.vfixups {
        match cname.as_str() {
            "hkaInterleavedUncompressedAnimation" | "hkaInterleavedSkeletalAnimation" => {
                anim = Some((*src, AnimType::Interleaved));
            }
            "hkaWaveletCompressedAnimation"
            | "hkaWaveletCompressedSkeletalAnimation"
            | "hkaWaveletSkeletalAnimation" => {
                anim.get_or_insert((*src, AnimType::Wavelet));
            }
            "hkaDeltaCompressedAnimation"
            | "hkaDeltaCompressedSkeletalAnimation"
            | "hkaDeltaSkeletalAnimation" => {
                anim.get_or_insert((*src, AnimType::Delta));
            }
            "hkaSplineCompressedAnimation" | "hkaSplineSkeletalAnimation" => {
                anim.get_or_insert((*src, AnimType::Spline));
            }
            "hkaAnimationBinding" => binding_src = Some(*src),
            _ => {}
        }
    }
    let (src, anim_type) = anim.ok_or("no hkaAnimation-derived object in packfile")?;
    let obj = raw.obj_abs(src);

    let duration = f32_le(pk, obj + OFF_DURATION);
    let num_tracks = i32_le(pk, obj + OFF_NUM_TRANSFORM_TRACKS).max(0) as usize;
    // m_type is informational; the class name already told us the encoding.
    let _m_type = i32_le(pk, obj + OFF_TYPE);

    let track_to_bone = binding_src
        .map(|b| read_binding_track_to_bone(pk, &raw, b))
        .unwrap_or_default();

    match anim_type {
        AnimType::Interleaved => {
            let total = i32_le(pk, obj + OFF_INTERLEAVED_TRANSFORMS_SIZE).max(0) as usize;
            let base = raw
                .resolve_ptr(src, OFF_INTERLEAVED_TRANSFORMS_PTR)
                .ok_or("interleaved animation: m_transforms pointer not relocated")?;
            if num_tracks == 0 {
                return Err("interleaved animation: zero transform tracks".into());
            }
            let num_frames = total / num_tracks;
            let mut frames = Vec::with_capacity(total);
            for i in 0..(num_frames * num_tracks) {
                frames.push(QsTransform::read(pk, base + i * QS_TRANSFORM_SIZE));
            }
            Ok(AnimClip {
                anim_type,
                duration,
                num_tracks,
                num_frames,
                track_to_bone,
                decoded: true,
                frames,
            })
        }
        AnimType::Wavelet => {
            // Faithful port of hk_anim/wavelet.py::decode_wavelet. Reconstructs
            // per-frame QsTransform via static-mask + inverse-Haar lifting.
            if let Some(w) = decode_wavelet(pk) {
                let num_frames = w.frames.len();
                let num_tracks = w.n_tt;
                let mut frames = Vec::with_capacity(num_frames * num_tracks);
                for frame in &w.frames {
                    for v in frame {
                        frames.push(QsTransform {
                            translation: [v[0], v[1], v[2]],
                            rotation: [v[3], v[4], v[5], v[6]],
                            scale: [v[7], v[8], v[9]],
                        });
                    }
                }
                return Ok(AnimClip {
                    anim_type,
                    duration: w.duration,
                    num_tracks,
                    num_frames,
                    track_to_bone,
                    decoded: true,
                    frames,
                });
            }
            // Fall through to header-only if the struct could not be located.
            let num_frames = i32_le(pk, obj + OFF_WAVELET_NUM_POSES).max(0) as usize;
            Ok(AnimClip {
                anim_type,
                duration,
                num_tracks,
                num_frames,
                track_to_bone,
                decoded: false,
                frames: Vec::new(),
            })
        }
        AnimType::Delta | AnimType::Spline => {
            // Header decoded faithfully; frame reconstruction is proprietary and
            // absent from this workspace — return a header-only clip. (delta.py is
            // header-only too; detection is enough — don't crash on it.)
            let num_frames = i32_le(pk, obj + OFF_WAVELET_NUM_POSES).max(0) as usize;
            Ok(AnimClip {
                anim_type,
                duration,
                num_tracks,
                num_frames,
                track_to_bone,
                decoded: false,
                frames: Vec::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline]
    fn qlen(q: [f32; 4]) -> f32 {
        (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
    }

    /// Golden fixture: the KS-750 motorcycle animation packfile (retail PC, LE).
    /// It contains one `hkaWaveletSkeletalAnimation` (60 transform tracks,
    /// duration ~0.968 s, 30 poses). The full wavelet reconstruction is a
    /// faithful port of `hk_anim/wavelet.py`; golden values below are that
    /// Python decoder's output on this fixture (tolerance 1e-3).
    #[test]
    fn ks750_anim_wavelet_decodes() {
        let buf: &[u8] = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        let clip = parse_anim(buf).expect("parse anim");

        assert_eq!(clip.anim_type, AnimType::Wavelet, "wavelet-compressed clip");
        assert!(clip.decoded, "wavelet frames must be reconstructed");
        assert_eq!(clip.num_tracks, 60);
        assert_eq!(clip.num_frames, 30);
        assert!(
            (clip.duration - 0.967_633_4).abs() < 1e-3,
            "duration = {}",
            clip.duration
        );

        // frame 0, track 0: translation.
        let f0t0 = clip.frame(0, 0);
        let t = f0t0.translation;
        assert!((t[0] - 0.958_11).abs() < 1e-3, "f0t0.tx = {}", t[0]);
        assert!((t[1] - 1.146_91).abs() < 1e-3, "f0t0.ty = {}", t[1]);
        assert!((t[2] - 0.659_49).abs() < 1e-3, "f0t0.tz = {}", t[2]);

        // frame 0, track 30: rotation (quat xyzw).
        let r = clip.frame(0, 30).rotation;
        assert!((r[0] - 0.526_59).abs() < 1e-3, "f0t30.qx = {}", r[0]);
        assert!((r[1] - -0.315_43).abs() < 1e-3, "f0t30.qy = {}", r[1]);
        assert!((r[2] - -0.451_79).abs() < 1e-3, "f0t30.qz = {}", r[2]);
        assert!((r[3] - 0.647_38).abs() < 1e-3, "f0t30.qw = {}", r[3]);

        // frame 15, track 30: rotation.
        let r = clip.frame(15, 30).rotation;
        assert!((r[0] - 0.514_44).abs() < 1e-3, "f15t30.qx = {}", r[0]);
        assert!((r[1] - 0.376_07).abs() < 1e-3, "f15t30.qy = {}", r[1]);
        assert!((r[2] - 0.381_30).abs() < 1e-3, "f15t30.qz = {}", r[2]);
        assert!((r[3] - 0.669_72).abs() < 1e-3, "f15t30.qw = {}", r[3]);

        // Every track's quaternion is unit length; every scale ≈ [1,1,1].
        for f in 0..clip.num_frames {
            for tr in 0..clip.num_tracks {
                let q = clip.frame(f, tr);
                assert!(
                    (qlen(q.rotation) - 1.0).abs() < 1e-3,
                    "f{f}t{tr} |q| = {}",
                    qlen(q.rotation)
                );
                for (a, &s) in q.scale.iter().enumerate() {
                    assert!((s - 1.0).abs() < 1e-3, "f{f}t{tr} scale[{a}] = {s}");
                }
            }
        }

        // sample_local yields one transform per track with unit quats.
        let pose = clip.sample_local(0.0);
        assert_eq!(pose.len(), clip.num_tracks);
        for tx in &pose {
            assert!((qlen(tx.rotation) - 1.0).abs() < 1e-3, "|q| = {}", qlen(tx.rotation));
        }
        let mid = clip.sample_local(clip.duration * 0.5);
        assert_eq!(mid.len(), clip.num_tracks);
    }

    /// Synthetic interleaved clip: verifies the *decode + sample* path end to
    /// end (this is the path the engine actually consumes for uncompressed
    /// clips). Two frames, two tracks; frame 0 = identity, frame 1 rotated 90°
    /// about Y on track 0 and translated on track 1. Sampling the midpoint must
    /// interpolate.
    #[test]
    fn interleaved_sampling_interpolates() {
        let clip = synthetic_interleaved();
        assert!(clip.decoded);
        assert_eq!(clip.num_tracks, 2);
        assert_eq!(clip.num_frames, 2);

        let f0 = clip.sample_local(0.0);
        assert_eq!(f0[0].rotation, [0.0, 0.0, 0.0, 1.0]);

        let mid = clip.sample_local(clip.duration * 0.5);
        // Track 0 rotation slerps to ~45° about Y: y≈sin(22.5°)=0.3827, w≈0.9239.
        assert!((mid[0].rotation[1] - 0.3827).abs() < 1e-2, "y = {}", mid[0].rotation[1]);
        assert!((qlen(mid[0].rotation) - 1.0).abs() < 1e-3);
        // Track 1 translation lerps halfway.
        assert!((mid[1].translation[0] - 0.5).abs() < 1e-4, "tx = {}", mid[1].translation[0]);
        // Midpoint differs from frame 0.
        assert_ne!(mid[0].rotation, f0[0].rotation);
    }

    /// Build an `AnimClip` directly (bypassing the packfile) to exercise the
    /// interpolation math without needing an interleaved fixture on disk.
    fn synthetic_interleaved() -> AnimClip {
        let s = std::f32::consts::FRAC_1_SQRT_2; // sin/cos 45°
        let ident = QsTransform::IDENTITY;
        let rot_y_90 = QsTransform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, s, 0.0, s], // 90° about +Y
            scale: [1.0, 1.0, 1.0],
        };
        let trans_x1 = QsTransform {
            translation: [1.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        // frame-major [f0t0, f0t1, f1t0, f1t1]
        let frames = vec![ident, ident, rot_y_90, trans_x1];
        AnimClip {
            anim_type: AnimType::Interleaved,
            duration: 1.0,
            num_tracks: 2,
            num_frames: 2,
            track_to_bone: Vec::new(),
            decoded: true,
            frames,
        }
    }
}
