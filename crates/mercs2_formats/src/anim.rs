//! Havok 5.5.0-r1 **animation-clip decoder** — turns a serialized `hkaAnimation`
//! packfile into a sampleable per-bone local pose for the native engine.
//!
//! This is the read side that pairs with [`crate::havok`]: it reuses that
//! module's tested little-endian packfile walker
//! ([`crate::havok::parse_packfile_raw`]) for the section-header / classname / fixup
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

// ── wavelet decompression — faithful port of the retail engine decoder ───────
//
// This replaces the old `hk_anim/wavelet.py` port (which was WRONG). It is a
// transcription of the retail `Mercenaries2.exe` `LtSampleWave` call-tree,
// symbolized by Ghidra from Havok debug strings in
// `output/_ghidra/all_functions_decomp.txt`:
//   FUN_009f5e40  LtSampleWave            (entry: decompress → interp → recompose)
//   FUN_009fa810  static-mask → DOF counts
//   FUN_009f0ee0  frame-pos → (int frame, interp fraction)
//   FUN_009f5b90  TtdecompressBlockCacheW (one block)
//   FUN_009f54f0  per-DOF dequantize      (drives the three sub-decoders)
//     FUN_009ff120  bitmap sparse-run entropy unpack
//     FUN_009fdd50  quantized-int → float dequant
//     FUN_009fe5b0  inverse-wavelet 8×8 basis (+ lifting passes for bs>8)
//     FUN_009fd810  per-DOF bit-budget
//   FUN_009fb870  StRecomposeW            (assemble hkQsTransform[] output)
//
// The 8×8 inverse-wavelet basis matrix and the scalar constants below were read
// LIVE from the running retail exe (x32dbg) at the .rdata addresses the decomp
// references (base 0x400000, decomp layout 1:1) — they are not present in the
// decomp text. Numeric gate: the live LtSampleWave capture in
// `tests/fixtures/wavelet_live_oracle.md`.
//
// HK550 32-bit ON-DISK layout offsets (from the serialized wavelet struct
// start). These are the on-disk `hkaWaveletSkeletalAnimation` fields; the
// runtime object (see the oracle capture) computes its +0x34..+0x54 section
// offsets from these on-disk indices at deserialize time, but because the
// coefficient/mask data blob is contiguous right after the header and all
// section addressing is index-relative to that blob, we decode directly from
// the on-disk indices below (verified: DOF sum, mask blob, and section layout
// all reconcile with the runtime capture for clip 0x24F8C8E6).
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
// (+84 quantDataSize, +92 numDataBuffer — present on disk but not needed here.)
const WAVELET_STRUCT_SIZE: usize = 96;

// QuantizationFormat sub-fields (bytes/dwords from W_OFF_QFMT). From the decomp
// reads in FUN_009f5b90/FUN_009f54f0 and confirmed against the on-disk clip.
//   +0 maxBitWidth (u8)   +1 preservedCount (u8)   +4 numD (u32, dynamic DOFs)
//   +8 offsetIdx (u32)    +12 scaleIdx (u32)        +16 bitWidthIdx (u32)
const QFMT_PRESERVED: usize = 1;
const QFMT_NUM_D: usize = 4;
const QFMT_OFFSET_IDX: usize = 8;
const QFMT_SCALE_IDX: usize = 12;
const QFMT_BW_IDX: usize = 16;

// The 8-point inverse-wavelet basis (FUN_009fe5b0). `out = M · coeffs` for a
// blockSize-8 block (the `if (8 < param_2)` lifting loop does not run for bs=8).
// Read live from the running exe: FUN_009fe5b0's `_DAT_00e76f..`/`_DAT_00e77..`
// runtime constants are copied from `DAT_00b6b8f0..0xb6b9ec`; symbolically
// executing the function on the 8 basis vectors yields this matrix.
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

/// FUN_009fd810 (decomp line ~919799): per-DOF byte budget for one block's
/// packed quant stream. `((blockSize - preserved)*bw + 7) >> 3 + preserved*4`
/// (arithmetic-shift rounding on negative preserved-adjusted values).
#[inline]
fn wv_bit_budget(block_size: usize, bw: u32, preserved: usize) -> usize {
    let v = (block_size as i32 - preserved as i32) * bw as i32 + 7;
    let add = if v < 0 { (v >> 31) & 7 } else { 0 };
    (((v + add) >> 3) as usize) + preserved * 4
}

/// FUN_009ff120 (decomp line ~920668): bitmap sparse-run entropy unpack for one
/// DOF's block. A leading bitmap selects, per coefficient, either a value read
/// from the packed 16-bit-word stream (`bit==0`) or the run-fill value `fill`
/// (`bit!=0`). Returns the unpacked byte stream consumed by [`wv_dequant`].
///
/// This is the general (`bw != 8 && bw != 16`) path; retail Mercs2 wavelet
/// clips use `bw = maxBitWidth` (11 for the gated clip) so the general path is
/// the one that runs. The specialized bw==8/bw==16 paths in the decomp are a
/// byte/word fast copy of the same logic and are not needed here.
fn wv_entropy_unpack(
    blob: &[u8],
    base: usize,
    bw: u32,
    fill: u32,
    preserved: usize,
    budget: usize,
) -> (Vec<u8>, Vec<bool>) {
    let n = ((budget as i32 + preserved as i32 * -4) * 8) / bw as i32;
    let n = n.max(0) as usize;
    let mut out: Vec<u8> = Vec::new();
    let mut is_fill: Vec<bool> = Vec::with_capacity(preserved + n);
    // preserved leading coefficients: copied verbatim (as raw f32 bytes).
    for i in 0..preserved {
        let o = base + i * 4;
        if o + 4 <= blob.len() {
            out.extend_from_slice(&blob[o..o + 4]);
        } else {
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
        is_fill.push(false);
    }
    let rd16 = |p: usize| -> u32 {
        if p + 2 <= blob.len() {
            (blob[p] as u32) | ((blob[p + 1] as u32) << 8)
        } else {
            0
        }
    };
    let mut li = base + preserved * 4; // bitmap byte pointer
    let bm_bytes = (n + 7) >> 3;
    let mut reg = rd16(li + bm_bytes);
    let mut word_ptr = li + bm_bytes + 2;
    let mut avail: u32 = 0x10;
    let mut b_mask: u32 = 1;
    let out_mask: u32 = if bw >= 16 { 0xffff } else { (1u32 << bw) - 1 };
    let mut acc: u32 = 0; // output bit accumulator (param3 in decomp)
    let mut acc_bits: u32 = 0; // bVar5
    let mut obuf: Vec<u8> = Vec::new();
    for _ in 0..n {
        let bit = (li < blob.len()) && (blob[li] as u32 & b_mask) != 0;
        if !bit {
            // present: read `bw` bits from the 16-bit-word stream.
            if avail < bw {
                reg |= rd16(word_ptr) << (avail & 0x1f);
                word_ptr += 2;
                avail += 0x10;
            }
            let val = (reg & 0xffff) & out_mask;
            let next = acc_bits + bw;
            acc |= val << (acc_bits & 0x1f);
            let mut nb = next;
            if nb > 0xf {
                obuf.push((acc & 0xff) as u8);
                obuf.push(((acc >> 8) & 0xff) as u8);
                acc >>= 0x10;
                nb -= 0x10;
            }
            reg >>= bw & 0x1f;
            avail -= bw;
            acc_bits = nb;
            is_fill.push(false);
        } else {
            // run-fill (FUN_009ff120 line ~920802: `param_2[1]` written *unmasked*).
            let next = acc_bits + bw;
            acc |= fill << (acc_bits & 0x1f);
            let mut nb = next;
            if nb > 0xf {
                obuf.push((acc & 0xff) as u8);
                obuf.push(((acc >> 8) & 0xff) as u8);
                acc >>= 0x10;
                nb -= 0x10;
            }
            acc_bits = nb;
            is_fill.push(true);
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
    (out, is_fill)
}

/// The per-DOF byte-advance of one entropy block = FUN_009ff120's return value
/// `(preserved*0x20 + 7 + bw*present + n) >> 3`, where `present` is the number
/// of *present* (bitmap bit == 0, read-from-stream) codes — NOT all `n`. Only
/// present codes consume stream bits; run-fill codes do not advance the input.
/// (Verified against the live 2.5673 s capture: using `n` here drifts the
/// per-DOF quant pointer.)
#[inline]
fn wv_entropy_advance(block_size: usize, bw: u32, preserved: usize, present: usize) -> usize {
    let n = wv_entropy_n(block_size, bw, preserved);
    (preserved * 0x20 + 7 + (bw as usize) * present + n) >> 3
}

/// Number of non-preserved codes in one block's entropy stream (FUN_009ff120
/// `uVar8 = ((budget - preserved*4) * 8) / bw`).
#[inline]
fn wv_entropy_n(block_size: usize, bw: u32, preserved: usize) -> usize {
    let budget = wv_bit_budget(block_size, bw, preserved);
    (((budget as i32 - preserved as i32 * 4) * 8) / bw as i32).max(0) as usize
}

/// FUN_009fdd50 (decomp line ~919935): quantized-int → float dequant of one
/// DOF's block. `value = ((float)code + bias) * (2^-bw * mult) + off`, where
/// `2^-bw` is the `DAT_00b6b808` power-of-two scale table and `bias =
/// _DAT_00bea940 = 0.0` (both read live from the exe). `mult` is the clip's
/// per-DOF scale array (obj+0x38), `off` the offset array (obj+0x34). Returns
/// `block_size` wavelet coefficients (still in wavelet space; caller applies
/// the inverse transform).
fn wv_dequant(
    stream: &[u8],
    bw: u32,
    preserved: usize,
    mult: f32,
    off: f32,
    block_size: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; block_size];
    for i in 0..preserved {
        out[i] = f32_le(stream, i * 4);
    }
    let scale = mult * 2f32.powi(-(bw as i32)); // DAT_00b6b808[bw] * mult
    let n = block_size - preserved;
    let mut acc: u64 = 0;
    let mut nbits: u32 = 0;
    let mut bp = preserved * 4;
    let mask: u64 = (1u64 << bw) - 1;
    for k in 0..n {
        while nbits < bw && bp < stream.len() {
            acc |= (stream[bp] as u64) << nbits;
            bp += 1;
            nbits += 8;
        }
        let v = (acc & mask) as u32;
        acc >>= bw;
        nbits = nbits.saturating_sub(bw);
        out[preserved + k] = v as f32 * scale + off; // + _DAT_00bea940 (0.0)
    }
    out
}

/// FUN_009fe5b0 (decomp line ~920311): inverse-wavelet reconstruction of one
/// `block_size`-sample DOF from its wavelet coefficients. For `block_size == 8`
/// this is a single 8×8 basis multiply ([`INV_WAVELET_8`]); the decomp's
/// `if (8 < param_2)` predict/update/deinterleave lifting passes (FUN_009fe250
/// / FUN_009fe180 / FUN_009fe4b0) extend it to larger blocks — retail Mercs2
/// clips are all blockSize 8, so only the base multiply is implemented here.
fn wv_inverse(coeffs: &[f32], block_size: usize) -> Vec<f32> {
    if block_size == 8 {
        let mut out = [0.0f32; 8];
        for (i, oi) in out.iter_mut().enumerate() {
            let mut s = 0.0f32;
            for j in 0..8 {
                s += INV_WAVELET_8[i][j] * coeffs[j];
            }
            *oi = s;
        }
        return out.to_vec();
    }
    // blockSize != 8 lifting passes not implemented (no such retail clip).
    coeffs[..block_size.min(coeffs.len())].to_vec()
}

/// StRecomposeW (FUN_009fb870, decomp line ~918497) reconstructs the quaternion
/// W from the `±2` sentinel: when the stored W has magnitude `_DAT_00b6b6b8`
/// (= 2.0, read live from the exe), the real W is `±sqrt(1 - x² - y² - z²)` with
/// the sign taken from the sentinel. (This is the `if (ABS(fVar1) == 2.0)`
/// branch in the decomp.)
fn wv_quat_w_sentinel(qx: f32, qy: f32, qz: f32, qw: f32) -> f32 {
    if qw.abs() == 2.0 {
        let w = (1.0 - qx * qx - qy * qy - qz * qz).max(0.0).sqrt();
        if qw <= 0.0 {
            -w
        } else {
            w
        }
    } else {
        qw
    }
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

/// Reconstructed wavelet clip: per-frame per-track `hkQsTransform`s
/// (frame-major, `[frame][track]`).
struct WaveletDecoded {
    duration: f32,
    n_tt: usize,
    frames: Vec<Vec<QsTransform>>,
}

/// StRecomposeW (FUN_009fb870) mask-selector layout. The mask u16 has 2-bit
/// *type* fields at bits 0-1 (pos), 2-3 (rot), 4-5 (scale) where 2 = identity,
/// 1 = all-static, 0 = per-sub-bit; and per-component *dynamic* selector bits
/// (tested via `mask>>6`):
///   pos:   x=bit8, y=bit7, z=bit6   (u&4 / u&2 / u&1)
///   rot:   x=bit12,y=bit11,z=bit10,w=bit9  (u&0x40 / u&0x20 / u&0x10 / u&8)
///   scale: x=bit15,y=bit14,z=bit13  (u&0x200 / u&0x100 / u&0x80)
/// Set selector ⇒ dynamic (from the coefficient buffer); clear ⇒ static (from
/// the static-DOF float array). Walking these in order also yields the
/// dynamic-DOF index → (track, component) scatter map.
// Component indices are into the 10-float track tuple
// (tx,ty,tz, qx,qy,qz,qw, sx,sy,sz): pos 0..2, rot 3..6, scale 7..9.
const POS_SUBS: [(u32, usize); 3] = [(4, 0), (2, 1), (1, 2)];
const ROT_SUBS: [(u32, usize); 4] = [(0x40, 3), (0x20, 4), (0x10, 5), (8, 6)];
const SCALE_SUBS: [(u32, usize); 3] = [(0x200, 7), (0x100, 8), (0x80, 9)];

/// Decode a `hkaWaveletSkeletalAnimation` — the LtSampleWave (FUN_009f5e40)
/// call-tree, transcribed from the retail decomp. Reconstructs every pose into
/// a frame-major `hkQsTransform` array; [`AnimClip::sample_local`] then does the
/// engine's linear (T/S) / slerp (R) interpolation between bracketing frames.
///
/// (The engine interpolates the *coefficient* buffer between two frames and
/// recomposes once — FUN_009f5e40 lines ~914563-914675, `local_54 = 1-frac`,
/// `local_60 = frac`. Recomposing every frame and interpolating the resulting
/// transforms is equivalent for translation/scale and near-equivalent for the
/// rotation; keeping the per-frame form lets wavelet clips share the tested
/// interleaved sampler.)
fn decode_wavelet(blob: &[u8]) -> Option<WaveletDecoded> {
    let so = find_wavelet_struct(blob)?;
    let dur = f32_le(blob, so + W_OFF_DURATION);
    let n_tt = u32_le(blob, so + W_OFF_NUM_TT) as usize;
    let n_ft = u32_le(blob, so + W_OFF_NUM_FT) as usize;
    let n_poses = u32_le(blob, so + W_OFF_NUM_POSES) as usize;
    let block_size = u32_le(blob, so + W_OFF_BLOCK_SIZE) as usize;
    if dur <= 0.0 || n_tt == 0 || n_poses == 0 || block_size == 0 {
        return None;
    }

    // QuantizationFormat.
    let qf = so + W_OFF_QFMT;
    let preserved = *blob.get(qf + QFMT_PRESERVED)? as usize;
    let num_d = u32_le(blob, qf + QFMT_NUM_D) as usize;
    let offset_idx = u32_le(blob, qf + QFMT_OFFSET_IDX) as usize;
    let scale_idx = u32_le(blob, qf + QFMT_SCALE_IDX) as usize;
    let bw_idx = u32_le(blob, qf + QFMT_BW_IDX) as usize;

    // Section indices (relative to the data blob right after the header).
    let sm_idx = u32_le(blob, so + W_OFF_STATIC_MASK_IDX) as usize;
    let sd_idx = u32_le(blob, so + W_OFF_STATIC_DOFS_IDX) as usize;
    let bi_idx = u32_le(blob, so + W_OFF_BLOCK_INDEX_IDX) as usize;
    let bi_size = u32_le(blob, so + W_OFF_BLOCK_INDEX_SIZE) as usize;
    let qd_idx = u32_le(blob, so + W_OFF_QUANT_DATA_IDX) as usize;
    let db = so + WAVELET_STRUCT_SIZE;

    // StRecomposeW walks `numTransformTracks + numFloatTracks` mask entries; the
    // live capture's param_3 (=64) is exactly that total for the gated clip.
    let n_masks = n_tt + n_ft;
    let masks: Vec<u16> = (0..n_masks)
        .map(|i| {
            let o = db + sm_idx + i * 2;
            if o + 2 <= blob.len() {
                u16::from_le_bytes([blob[o], blob[o + 1]])
            } else {
                0
            }
        })
        .collect();

    // Per-dynamic-DOF quant descriptors. FUN_009fdd50 uses `*(pbVar7+4)` as the
    // multiplier and `*(pbVar7+8)` as the additive offset; tracing the FUN_009f54f0
    // stack these are the obj+0x38 and obj+0x34 arrays respectively. On disk those
    // map to `scale_idx` (QFMT+12 → multiplier) and `offset_idx` (QFMT+8 → offset).
    // Verified live: with this ordering the 3.3366 s oracle clip decodes 64/64
    // rotation tracks (swapping them gives 19/64).
    let mult: Vec<f32> = (0..num_d).map(|i| f32_le(blob, db + scale_idx + i * 4)).collect();
    let addend: Vec<f32> = (0..num_d).map(|i| f32_le(blob, db + offset_idx + i * 4)).collect();
    let bw: Vec<u32> = (0..num_d).map(|i| *blob.get(db + bw_idx + i).unwrap_or(&0) as u32).collect();

    // Block index (byte offset of each block's quant data).
    let n_blocks = (n_poses + block_size - 1) / block_size;
    let block_off: Vec<usize> = if bi_size >= n_blocks {
        (0..n_blocks).map(|i| u32_le(blob, db + bi_idx + i * 4) as usize).collect()
    } else {
        vec![0; n_blocks]
    };
    let qd_base = db + qd_idx;

    // Dynamic-DOF index → (track, component), in StRecomposeW consumption order.
    let mut dof_map: Vec<(usize, usize)> = Vec::with_capacity(num_d);
    for (ti, &m) in masks.iter().enumerate() {
        let low = m as u32;
        let u = (m as u32) >> 6;
        if low & 3 != 2 {
            for (bit, comp) in POS_SUBS {
                if u & bit != 0 {
                    dof_map.push((ti, comp));
                }
            }
        }
        if (low >> 2) & 3 != 2 {
            for (bit, comp) in ROT_SUBS {
                if u & bit != 0 {
                    dof_map.push((ti, comp));
                }
            }
        }
        if (low >> 4) & 3 != 2 {
            for (bit, comp) in SCALE_SUBS {
                if u & bit != 0 {
                    dof_map.push((ti, comp));
                }
            }
        }
    }
    if dof_map.len() != num_d {
        return None;
    }

    // Decompress every block → per-DOF `block_size` reconstructed frame values.
    // FUN_009f54f0 loop (line ~913955): entropy-unpack → dequant → inverse
    // wavelet per DOF, advancing the quant pointer by FUN_009ff120's return.
    let mut per_dof_frames: Vec<Vec<f32>> = vec![Vec::with_capacity(n_poses); num_d];
    for (blk, &boff) in block_off.iter().enumerate() {
        let poses_here = block_size.min(n_poses - blk * block_size);
        let mut p = qd_base + boff;
        for d in 0..num_d {
            let bwd = bw[d];
            if bwd == 0 || bwd >= 16 {
                // Only the general (bw<16) path is exercised by retail clips;
                // guard the shift/mask arithmetic against a degenerate width.
                for _ in 0..poses_here {
                    per_dof_frames[d].push(addend[d]);
                }
                let bw1 = bwd.max(1);
                p += wv_entropy_advance(block_size, bw1, preserved, wv_entropy_n(block_size, bw1, preserved));
                continue;
            }
            // Fill (run) value = the quantized code that dequantizes to ≈ -addend
            // so an omitted detail coefficient contributes ~0. `bias =
            // ROUND(-addend·2^bw / mult)` clamped away from 2^bw (FUN_009f54f0
            // lines ~913966-913971).
            let ival = 1i64 << bwd;
            let bias_unclamped = if mult[d] != 0.0 {
                (-addend[d] * ival as f32 / mult[d]).round() as i64
            } else {
                0
            };
            let bias = (if bias_unclamped == ival { ival - 1 } else { bias_unclamped }) as u32;

            let budget = wv_bit_budget(block_size, bwd, preserved);
            let (stream, is_fill) = wv_entropy_unpack(blob, p, bwd, bias, preserved, budget);
            let present = is_fill[preserved..].iter().filter(|&&f| !f).count();
            let coeffs = wv_dequant(&stream, bwd, preserved, mult[d], addend[d], block_size);
            let frames = wv_inverse(&coeffs, block_size);
            for f in frames.into_iter().take(poses_here) {
                per_dof_frames[d].push(f);
            }
            p += wv_entropy_advance(block_size, bwd, preserved, present);
        }
    }

    // Assemble one hkQsTransform per (frame, track) with StRecomposeW's rules.
    let sd_base = db + sd_idx;
    let next_static = |sc: &mut usize| -> f32 {
        let v = f32_le(blob, sd_base + *sc * 4);
        *sc += 1;
        v
    };
    let mut out: Vec<Vec<QsTransform>> = Vec::with_capacity(n_poses);
    for f in 0..n_poses {
        // Static-DOF cursor resets per frame (static values shared across frames);
        // dynamic values are this frame's decoded coefficients, in DOF order.
        let mut sc = 0usize;
        let mut dc = 0usize;
        let mut track = Vec::with_capacity(n_tt);
        for &m in masks.iter().take(n_tt) {
            let low = m as u32;
            let u = (m as u32) >> 6;
            let mut v = [0.0f32; 10]; // tx,ty,tz, qx,qy,qz,qw, sx,sy,sz
            let take = |dynamic: bool, sc: &mut usize, dc: &mut usize| -> f32 {
                if dynamic {
                    let x = *per_dof_frames[*dc].get(f).unwrap_or(&0.0);
                    *dc += 1;
                    x
                } else {
                    next_static(sc)
                }
            };
            // position
            if low & 3 == 2 {
                v[0] = 0.0;
                v[1] = 0.0;
                v[2] = 0.0;
            } else {
                for (bit, comp) in POS_SUBS {
                    v[comp] = take(u & bit != 0, &mut sc, &mut dc);
                }
            }
            // rotation
            if (low >> 2) & 3 == 2 {
                v[3] = 0.0;
                v[4] = 0.0;
                v[5] = 0.0;
                v[6] = 1.0;
            } else {
                for (bit, comp) in ROT_SUBS {
                    v[comp] = take(u & bit != 0, &mut sc, &mut dc);
                }
                v[6] = wv_quat_w_sentinel(v[3], v[4], v[5], v[6]);
            }
            // scale
            if (low >> 4) & 3 == 2 {
                v[7] = 1.0;
                v[8] = 1.0;
                v[9] = 1.0;
            } else {
                for (bit, comp) in SCALE_SUBS {
                    v[comp] = take(u & bit != 0, &mut sc, &mut dc);
                }
            }
            track.push(QsTransform {
                translation: [v[0], v[1], v[2]],
                rotation: [v[3], v[4], v[5], v[6]],
                scale: [v[7], v[8], v[9]],
            });
        }
        out.push(track);
    }

    Some(WaveletDecoded {
        duration: dur,
        n_tt,
        frames: out,
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
            // Faithful transcription of the retail LtSampleWave call-tree
            // (FUN_009f5e40 …). Reconstructs every pose into frame-major
            // hkQsTransforms; sample_local then interpolates.
            if let Some(w) = decode_wavelet(pk) {
                let num_frames = w.frames.len();
                let num_tracks = w.n_tt;
                let mut frames = Vec::with_capacity(num_frames * num_tracks);
                for frame in &w.frames {
                    frames.extend_from_slice(frame);
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

    /// KS-750 motorcycle wavelet fixture (retail PC, LE): one
    /// `hkaWaveletSkeletalAnimation`, 60 transform tracks, ~0.968 s, 30 poses,
    /// blockSize 8, all bit-widths 11. This exercises the real LtSampleWave
    /// decoder end-to-end (header + every block decompressed + assembled). The
    /// numeric oracle lives in the vz.wad gate test below (`wavelet_gate_*`);
    /// here we only assert the structural invariants the honest decoder
    /// guarantees (no `hk_anim/wavelet.py` goldens — that decoder was WRONG).
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

        // Every reconstructed value is finite (no decode-pointer overrun / NaN).
        for f in 0..clip.num_frames {
            for tr in 0..clip.num_tracks {
                let q = clip.frame(f, tr);
                for c in q.translation.iter().chain(q.rotation.iter()).chain(q.scale.iter()) {
                    assert!(c.is_finite(), "f{f}t{tr} non-finite {c}");
                }
            }
        }

        // sample_local yields one transform per track and is finite everywhere.
        let pose = clip.sample_local(0.0);
        assert_eq!(pose.len(), clip.num_tracks);
        let mid = clip.sample_local(clip.duration * 0.5);
        assert_eq!(mid.len(), clip.num_tracks);
    }

    /// Sample the clip at an absolute *frame-position* (frame index + fraction),
    /// bypassing the seconds↔frame conversion. The live capture fed
    /// `param_2 = 1.496` straight into StRecomposeW's interp (→ frame 1, frac
    /// 0.496), so the gate is expressed in frame-position, not seconds.
    fn sample_at_framepos(clip: &AnimClip, fp: f32) -> Vec<QsTransform> {
        let last = clip.num_frames.saturating_sub(1);
        let f0 = (fp.floor() as usize).min(last);
        let f1 = (f0 + 1).min(last);
        let frac = (fp - f0 as f32).clamp(0.0, 1.0);
        (0..clip.num_tracks)
            .map(|t| lerp_qs(clip.frame(f0, t), clip.frame(f1, t), frac))
            .collect()
    }

    /// GATE (project lead): decode the exact vz.wad clip captured live
    /// (name-hash 0x24F8C8E6, vz.wad block 3362 — duration 3.3366 s, 101 poses,
    /// 64 transform tracks, blockSize 8, 322 dynamic DOF) and reproduce
    /// `tests/fixtures/wavelet_live_oracle.md`'s param_4 output buffer.
    ///
    /// The capture's `param_2 = 1.496` is a TIME in seconds; FUN_009f0ee0 maps it
    /// to `g = (numPoses-1)*time/duration = 100*1.496/3.3366 = 44.83` → frame 45,
    /// frac ≈ -0.166 (verified against the 2.5673 s live capture, whose
    /// StDecompressW input is reproduced to 246/246 in `wavelet_decompress.rs`).
    ///
    /// Runs only when the clip fixture is present (dumped out-of-band from
    /// vz.wad); otherwise it no-ops so CI without the retail WAD stays green.
    #[test]
    fn wavelet_gate_oracle_clip_frame_1_496() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/oracle_clip.bin");
        let Ok(buf) = std::fs::read(path) else {
            eprintln!("skip: {path} not present (dump from vz.wad block 3362)");
            return;
        };
        let clip = parse_anim(&buf).expect("parse oracle clip");
        assert_eq!(clip.anim_type, AnimType::Wavelet);
        assert!(clip.decoded);
        assert_eq!(clip.num_frames, 101);
        assert_eq!(clip.num_tracks, 64, "64 mask entries (61 xform + 3 float)");
        assert!((clip.duration - 3.3366).abs() < 1e-3, "dur = {}", clip.duration);

        // FUN_009f0ee0: time 1.496 s → g on the [0, numPoses-1] frame timeline.
        let time = 1.496f32;
        let g = (clip.num_frames as f32 - 1.0) * (time / clip.duration);
        let pose = sample_at_framepos(&clip, g);

        // track 0 — identity.
        let t0 = pose[0];
        assert!(t0.translation.iter().all(|c| c.abs() < 1e-3), "t0 T {:?}", t0.translation);
        assert!((t0.rotation[3] - 1.0).abs() < 1e-3 && t0.rotation[..3].iter().all(|c| c.abs() < 1e-3),
            "t0 R {:?}", t0.rotation);
        assert!(t0.scale.iter().all(|s| (s - 1.0).abs() < 1e-3), "t0 S {:?}", t0.scale);

        // Full-buffer check: compare the rotation quaternion of every track to the
        // captured oracle output buffer and report the exact match count. The
        // oracle was captured at frame 45 (time-based); a per-track slerp of the
        // two bracketing decoded frames reproduces the rotations.
        let ob = oracle_output_buffer();
        let mut ok = 0usize;
        let mut mism: Vec<usize> = Vec::new();
        for t in 0..clip.num_tracks {
            let o = t * 48;
            let orr = [
                f32_le(&ob, o + 16),
                f32_le(&ob, o + 20),
                f32_le(&ob, o + 24),
                f32_le(&ob, o + 28),
            ];
            let close = (0..4).all(|i| (pose[t].rotation[i] - orr[i]).abs() < 3e-3);
            if close {
                ok += 1;
            } else {
                mism.push(t);
            }
        }
        eprintln!(
            "oracle-clip rotations: {}/{} tracks within 3e-3; mismatched tracks: {:?}",
            ok, clip.num_tracks, mism
        );
        // Time-based sampling (frame 45) + the corrected mult/addend, entropy
        // advance (present-count), and 0.0 dequant bias reproduce every rotation
        // track of the captured oracle. (The 2.5673 s live capture validates the
        // decoder even more tightly: stage-1 246/246 in wavelet_decompress.rs,
        // stage-2 660/660 in wavelet_recompose.rs.)
        assert_eq!(ok, clip.num_tracks, "all rotation tracks must match (see stderr)");
    }

    /// The captured `param_4` output buffer (64 × 48-byte hkQsTransform, 3072 B)
    /// from `wavelet_live_oracle.md`, as raw little-endian bytes.
    fn oracle_output_buffer() -> Vec<u8> {
        let md = include_str!("../tests/fixtures/wavelet_live_oracle.md");
        let start = md.find("Full raw output buffer").expect("oracle buffer header");
        let after = &md[start..];
        let b = after.find("```").expect("open fence") + 3;
        let e = after[b..].find("```").expect("close fence");
        let hex: String = after[b..b + e].chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
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
