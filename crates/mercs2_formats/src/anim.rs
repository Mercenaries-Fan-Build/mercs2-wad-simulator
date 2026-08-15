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

// ============================ FORWARD (ENCODER) ============================
//
// The write side: invert each decode stage so a clip can be rebuilt from source into a native
// wavelet packfile WITHOUT the Havok DLL. Gated against the verified decoder (encode → decode ≈
// source) and, at the packfile level, diffed against AssetCc2's proven-in-retail output.

/// Invert an 8×8 matrix by Gauss–Jordan elimination. Used once to derive the forward wavelet
/// transform from [`INV_WAVELET_8`]; panics if the matrix is singular (it is not).
fn invert8(m: &[[f32; 8]; 8]) -> [[f32; 8]; 8] {
    // Work in f64 for conditioning, then narrow — the basis has exact dyadic entries so this is
    // effectively exact.
    let mut a = [[0f64; 16]; 8];
    for i in 0..8 {
        for j in 0..8 {
            a[i][j] = m[i][j] as f64;
        }
        a[i][8 + i] = 1.0;
    }
    for col in 0..8 {
        // partial pivot
        let mut piv = col;
        for r in (col + 1)..8 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        a.swap(col, piv);
        let d = a[col][col];
        assert!(d.abs() > 1e-12, "INV_WAVELET_8 singular");
        for j in 0..16 {
            a[col][j] /= d;
        }
        for r in 0..8 {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    for j in 0..16 {
                        a[r][j] -= f * a[col][j];
                    }
                }
            }
        }
    }
    let mut out = [[0f32; 8]; 8];
    for i in 0..8 {
        for j in 0..8 {
            out[i][j] = a[i][8 + j] as f32;
        }
    }
    out
}

/// The forward 8-point wavelet transform: `coeffs = INV_WAVELET_8⁻¹ · samples`. Exact inverse of
/// [`wv_inverse`] for `block_size == 8` (every retail clip). Cached — the inversion runs once.
pub fn forward_wavelet_8(samples: &[f32; 8]) -> [f32; 8] {
    use std::sync::OnceLock;
    static FWD: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    let fwd = FWD.get_or_init(|| invert8(&INV_WAVELET_8));
    let mut out = [0f32; 8];
    for (i, oi) in out.iter_mut().enumerate() {
        let mut s = 0.0f32;
        for j in 0..8 {
            s += fwd[i][j] * samples[j];
        }
        *oi = s;
    }
    out
}

/// Forward affine quantize of one DOF's `block_size` wavelet coefficients — the exact inverse of
/// [`wv_dequant`] (`value = code · 2^-bw · mult + off`). Chooses `off = min` and `mult` so the
/// coefficient range spans the full `2^bw` codes, then `code = round((coeff − off) / scale)`
/// clamped to `[0, 2^bw − 1]`. `preserved` leading coefficients are kept as raw f32 (retail
/// `preserved = 0`). Returns `(codes, mult, off)`; `mult == 0` for a constant block (all codes 0).
pub fn forward_quantize(coeffs: &[f32], bw: u32, preserved: usize) -> (Vec<u32>, f32, f32) {
    let n = coeffs.len();
    let dyn_coeffs = &coeffs[preserved.min(n)..];
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &c in dyn_coeffs {
        lo = lo.min(c);
        hi = hi.max(c);
    }
    if !lo.is_finite() {
        lo = 0.0;
        hi = 0.0;
    }
    let off = lo;
    let levels = ((1u64 << bw) - 1) as f32; // 2^bw − 1 usable codes
    let range = hi - lo;
    // scale = 2^-bw · mult ; choose mult so range maps onto `levels`.
    let (scale, mult) = if range > 0.0 {
        let scale = range / levels;
        (scale, scale * 2f32.powi(bw as i32))
    } else {
        (1.0, 0.0)
    };
    let maxcode = ((1u64 << bw) - 1) as i64;
    let mut codes = Vec::with_capacity(dyn_coeffs.len());
    for &c in dyn_coeffs {
        let q = (((c - off) / scale).round() as i64).clamp(0, maxcode);
        codes.push(q as u32);
    }
    (codes, mult, off)
}

/// Pack one block's quantized codes into the raw on-disk entropy blob, ALL-PRESENT — the inverse of
/// [`wv_entropy_unpack`] with an all-zero bitmap (no run-fill). Layout:
/// `[preserved leading f32s][bitmap: (n+7)/8 zero bytes][codes bit-packed LSB-first at bw bits,
/// emitted as u16 LE words]`. All-present is a valid, slightly larger encoding (the run-fill
/// sparsification the decoder supports is a size optimization only) — it is the simplest correct
/// packer and what the encoder emits.
pub fn pack_block_all_present(codes: &[u32], bw: u32, preserved_f32: &[f32]) -> Vec<u8> {
    let n = codes.len();
    let mut out = Vec::new();
    for &f in preserved_f32 {
        out.extend_from_slice(&f.to_le_bytes());
    }
    // all-present bitmap: (n+7)/8 zero bytes (every code read from the word stream).
    let bm_bytes = (n + 7) >> 3;
    out.resize(out.len() + bm_bytes, 0);
    // Bit-pack codes LSB-first into a BYTE-rounded stream. The decoder reads it via 16-bit words
    // (`rd16`) and may over-read the final partial word into the next DOF's bytes, but only consumes
    // the exact bit count — so the on-disk blob is byte-rounded, matching `wv_entropy_advance`
    // (`bitmap n bits + present·bw bits`, byte-rounded). Word-padding here would desync every DOF
    // after the first.
    let mask: u64 = (1u64 << bw) - 1;
    let mut acc: u64 = 0;
    let mut nbits: u32 = 0;
    for &c in codes {
        acc |= (c as u64 & mask) << nbits;
        nbits += bw;
        while nbits >= 8 {
            out.push((acc & 0xff) as u8);
            acc >>= 8;
            nbits -= 8;
        }
    }
    if nbits > 0 {
        out.push((acc & 0xff) as u8);
    }
    out
}

/// Encode one dynamic DOF's per-frame values into per-block all-present entropy blobs plus the
/// SINGLE per-DOF quant descriptor `(mult, off)` the decoder applies to every block. This is the
/// inverse of the per-DOF loop in `decode_wavelet`: forward-transform each `block_size` window,
/// quantize ALL coefficients together (the decoder uses one `mult`/`off` per DOF, not per block),
/// then pack each block. The final block is padded to `block_size` with the last sample. Returns
/// `(per_block_blobs, mult, off)`.
pub fn encode_dof(frames: &[f32], block_size: usize, bw: u32) -> (Vec<Vec<u8>>, f32, f32) {
    assert!(block_size <= 8, "only block_size ≤ 8 is supported (retail is 8)");
    let n_blocks = frames.len().div_ceil(block_size).max(1);
    let last = frames.last().copied().unwrap_or(0.0);
    // Forward-transform each block → the full coefficient stream for this DOF.
    let mut all_coeffs: Vec<f32> = Vec::with_capacity(n_blocks * block_size);
    for b in 0..n_blocks {
        let mut samp = [0f32; 8];
        for (i, s) in samp.iter_mut().enumerate().take(block_size) {
            let fi = b * block_size + i;
            *s = if fi < frames.len() { frames[fi] } else { last };
        }
        let c = forward_wavelet_8(&samp);
        all_coeffs.extend_from_slice(&c[..block_size]);
    }
    // One affine quantizer for the whole DOF (matches the decoder's per-DOF descriptor).
    let (codes, mult, off) = forward_quantize(&all_coeffs, bw, 0);
    let blocks: Vec<Vec<u8>> = (0..n_blocks)
        .map(|b| pack_block_all_present(&codes[b * block_size..(b + 1) * block_size], bw, &[]))
        .collect();
    (blocks, mult, off)
}

/// Encode a whole clip into a native `hkaWaveletSkeletalAnimation` struct + its dataBuffer (the
/// bytes `decode_wavelet` reads) — the top-level wavelet encoder. Uses the simplest valid encoding:
/// every transform track's 10 components (tx,ty,tz, qx,qy,qz,qw, sx,sy,sz) are DYNAMIC and
/// all-present (no static/identity groups, no run-fill), so the StaticMask is uniform and the
/// dof_map is `[0..10]` per track. Constant components fall out for free (a zero-range quantizer
/// emits all-zero codes that dequantize to the constant `off`). Wrap the result in a Havok packfile
/// with [`crate::havok_write::write_packfile`]. Frames are `[frame][track]`.
pub fn encode_wavelet_struct(frames: &[Vec<QsTransform>], duration: f32) -> Vec<u8> {
    let n_poses = frames.len();
    assert!(n_poses > 0, "clip has no frames");
    let n_tt = frames[0].len();
    let block_size = 8usize;
    let bw = 11u32;
    let n_blocks = n_poses.div_ceil(block_size);

    let comp = |t: &QsTransform, ci: usize| -> f32 {
        match ci {
            0..=2 => t.translation[ci],
            3..=6 => t.rotation[ci - 3],
            7..=9 => t.scale[ci - 7],
            _ => 0.0,
        }
    };

    // Per-DOF encode, all 10 components of every track (dof_map order 0..10 per track).
    let mut dof_blocks: Vec<Vec<Vec<u8>>> = Vec::with_capacity(n_tt * 10);
    let mut mult: Vec<f32> = Vec::new();
    let mut addend: Vec<f32> = Vec::new();
    for ti in 0..n_tt {
        for ci in 0..10 {
            let series: Vec<f32> = (0..n_poses).map(|f| comp(&frames[f][ti], ci)).collect();
            let (blocks, m, o) = encode_dof(&series, block_size, bw);
            dof_blocks.push(blocks);
            mult.push(m);
            addend.push(o);
        }
    }
    let num_d = mult.len();

    // quantData is block-major: block b = concat of every DOF's block-b blob, in DOF order.
    let mut quant_data: Vec<u8> = Vec::new();
    let mut block_off: Vec<u32> = Vec::with_capacity(n_blocks);
    for blk in 0..n_blocks {
        block_off.push(quant_data.len() as u32);
        for blocks in &dof_blocks {
            quant_data.extend_from_slice(&blocks[blk]);
        }
    }

    let mask = encode_static_mask(MaskGroup::Mixed, MaskGroup::Mixed, MaskGroup::Mixed, &[true; 10]);

    // dataBuffer layout (all offsets relative to the struct end): masks, then the per-DOF
    // offset/scale/bitWidth arrays, the block index, and finally the quantData.
    let mut db: Vec<u8> = Vec::new();
    let sm_idx = db.len() as u32;
    for _ in 0..n_tt {
        db.extend_from_slice(&mask.to_le_bytes());
    }
    while db.len() % 4 != 0 {
        db.push(0);
    }
    let offset_idx = db.len() as u32;
    for &o in &addend {
        db.extend_from_slice(&o.to_le_bytes());
    }
    let scale_idx = db.len() as u32;
    for &m in &mult {
        db.extend_from_slice(&m.to_le_bytes());
    }
    let bw_idx = db.len() as u32;
    for _ in 0..num_d {
        db.push(bw as u8);
    }
    while db.len() % 4 != 0 {
        db.push(0);
    }
    let bi_idx = db.len() as u32;
    for &bo in &block_off {
        db.extend_from_slice(&bo.to_le_bytes());
    }
    let qd_idx = db.len() as u32;
    db.extend_from_slice(&quant_data);
    let sd_idx = 0u32; // no static DOFs → never read

    // The 96-byte hkaWaveletSkeletalAnimation fixed part.
    let mut s = vec![0u8; WAVELET_STRUCT_SIZE];
    let put32 = |s: &mut [u8], off: usize, v: u32| s[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put32(&mut s, W_OFF_ANIM_TYPE, 3);
    s[W_OFF_DURATION..W_OFF_DURATION + 4].copy_from_slice(&duration.to_le_bytes());
    put32(&mut s, W_OFF_NUM_TT, n_tt as u32);
    // numFloatTracks @20 stays 0.
    put32(&mut s, W_OFF_NUM_POSES, n_poses as u32);
    put32(&mut s, W_OFF_BLOCK_SIZE, block_size as u32);
    let qf = W_OFF_QFMT;
    s[qf] = bw as u8; // maxBitWidth
    s[qf + QFMT_PRESERVED] = 0;
    put32(&mut s, qf + QFMT_NUM_D, num_d as u32);
    put32(&mut s, qf + QFMT_OFFSET_IDX, offset_idx);
    put32(&mut s, qf + QFMT_SCALE_IDX, scale_idx);
    put32(&mut s, qf + QFMT_BW_IDX, bw_idx);
    put32(&mut s, W_OFF_STATIC_MASK_IDX, sm_idx);
    put32(&mut s, W_OFF_STATIC_DOFS_IDX, sd_idx);
    put32(&mut s, W_OFF_BLOCK_INDEX_IDX, bi_idx);
    put32(&mut s, W_OFF_BLOCK_INDEX_SIZE, n_blocks as u32);
    put32(&mut s, W_OFF_QUANT_DATA_IDX, qd_idx);
    put32(&mut s, 84, quant_data.len() as u32); // quantDataSize
    put32(&mut s, 92, db.len() as u32); // numDataBuffer

    let mut out = s;
    out.extend_from_slice(&db);
    out
}

/// Group type in a StaticMask: `Identity` (0,0,0 / identity quat / 1,1,1), `AllStatic` (constant,
/// from the static-DOF array), or `Mixed` (type 0 — each component dynamic-or-static per selector
/// bit). Encoded as the 2-bit field per group: 2=identity, 1=all-static, 0=mixed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaskGroup {
    Mixed = 0,
    AllStatic = 1,
    Identity = 2,
}

/// Which of the 10 transform components (tx,ty,tz,qx,qy,qz,qw,sx,sy,sz) a track's StaticMask marks
/// DYNAMIC (read from the coefficient buffer). A component is dynamic only when its group is `Mixed`
/// (type 0) AND its selector bit is set. Inverse of [`encode_static_mask`].
pub fn static_mask_dynamic(mask: u16) -> [bool; 10] {
    let mut dyn_ = [false; 10];
    let u = (mask >> 6) as u32;
    if mask & 0x3 == 0 {
        for &(bit, ci) in &POS_SUBS {
            if u & bit != 0 {
                dyn_[ci] = true;
            }
        }
    }
    if (mask >> 2) & 0x3 == 0 {
        for &(bit, ci) in &ROT_SUBS {
            if u & bit != 0 {
                dyn_[ci] = true;
            }
        }
    }
    if (mask >> 4) & 0x3 == 0 {
        for &(bit, ci) in &SCALE_SUBS {
            if u & bit != 0 {
                dyn_[ci] = true;
            }
        }
    }
    dyn_
}

/// Build a track's StaticMask u16 from the three group types and the per-component dynamic flags
/// (only consulted for `Mixed` groups). Inverse of [`static_mask_dynamic`].
pub fn encode_static_mask(
    pos: MaskGroup,
    rot: MaskGroup,
    scale: MaskGroup,
    dynamic: &[bool; 10],
) -> u16 {
    let mut m = (pos as u16) | ((rot as u16) << 2) | ((scale as u16) << 4);
    let mut u = 0u32;
    if pos == MaskGroup::Mixed {
        for &(bit, ci) in &POS_SUBS {
            if dynamic[ci] {
                u |= bit;
            }
        }
    }
    if rot == MaskGroup::Mixed {
        for &(bit, ci) in &ROT_SUBS {
            if dynamic[ci] {
                u |= bit;
            }
        }
    }
    if scale == MaskGroup::Mixed {
        for &(bit, ci) in &SCALE_SUBS {
            if dynamic[ci] {
                u |= bit;
            }
        }
    }
    m |= (u as u16) << 6;
    m
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
    let mult: Vec<f32> = (0..num_d)
        .map(|i| f32_le(blob, db + scale_idx + i * 4))
        .collect();
    let addend: Vec<f32> = (0..num_d)
        .map(|i| f32_le(blob, db + offset_idx + i * 4))
        .collect();
    let bw: Vec<u32> = (0..num_d)
        .map(|i| *blob.get(db + bw_idx + i).unwrap_or(&0) as u32)
        .collect();

    // Block index (byte offset of each block's quant data).
    let n_blocks = (n_poses + block_size - 1) / block_size;
    let block_off: Vec<usize> = if bi_size >= n_blocks {
        (0..n_blocks)
            .map(|i| u32_le(blob, db + bi_idx + i * 4) as usize)
            .collect()
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
                p += wv_entropy_advance(
                    block_size,
                    bw1,
                    preserved,
                    wv_entropy_n(block_size, bw1, preserved),
                );
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
            let bias = (if bias_unclamped == ival {
                ival - 1
            } else {
                bias_unclamped
            }) as u32;

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

    /// The forward transform must EXACTLY invert the decoder's `wv_inverse` for block-8 (every
    /// retail clip). This is the mathematical foundation of the encoder — if it doesn't round-trip
    /// to machine precision, nothing built on it can. Tested across several deterministic blocks.
    #[test]
    fn forward_wavelet_inverts_the_decoder() {
        let blocks: [[f32; 8]; 4] = [
            [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            [1.0, -1.0, 0.5, -0.5, 0.25, -0.25, 0.125, -0.125],
            [3.14, 2.71, -1.41, 0.0, 9.8, -6.0, 0.577, 42.0],
            [-0.001, 0.002, -0.003, 100.0, -100.0, 0.0, 0.5, 0.5],
        ];
        for (bi, samples) in blocks.iter().enumerate() {
            let coeffs = forward_wavelet_8(samples);
            let recovered = wv_inverse(&coeffs, 8);
            for i in 0..8 {
                let d = (recovered[i] - samples[i]).abs();
                assert!(
                    d < 1e-4,
                    "block {bi} dof {i}: forward→inverse drift {d:.3e} (got {}, want {})",
                    recovered[i],
                    samples[i]
                );
            }
        }
    }

    /// The full per-DOF codec round-trips within quantization error: samples → forward transform →
    /// forward quantize → (dequant → inverse) ≈ samples, at the retail bit width (11). This is the
    /// encoder's core promise — an authored curve survives the wavelet compression.
    #[test]
    fn wavelet_codec_round_trips_within_quant_error() {
        let bw = 11u32;
        let cases: [[f32; 8]; 3] = [
            [0.10, 0.15, 0.20, 0.18, 0.12, 0.05, -0.10, -0.20],
            [1.0, 1.02, 1.05, 1.03, 0.98, 0.95, 0.9, 0.88],
            [-2.0, -1.5, 0.0, 1.5, 2.0, 1.0, 0.0, -1.0],
        ];
        for (ci, samples) in cases.iter().enumerate() {
            let coeffs = forward_wavelet_8(samples);
            let (codes, mult, off) = forward_quantize(&coeffs, bw, 0);
            // Mirror wv_dequant, then the inverse transform (the decode side).
            let scale = mult * 2f32.powi(-(bw as i32));
            let recon: Vec<f32> = codes.iter().map(|&c| c as f32 * scale + off).collect();
            let out = wv_inverse(&recon, 8);
            for i in 0..8 {
                let d = (out[i] - samples[i]).abs();
                assert!(d < 2e-3, "case {ci} dof {i}: codec drift {d:.3e}");
            }
        }
    }

    /// The all-present bitstream packer must invert the decoder's entropy stage: pack codes → the
    /// raw blob → `wv_entropy_unpack` → `wv_dequant` recovers exactly those codes. This is the
    /// trickiest encoder piece; gated against the real decode path at the retail bit width 11.
    #[test]
    fn entropy_packer_round_trips_through_the_decoder() {
        let bw = 11u32;
        let block_size = 8usize;
        let cases: [[u32; 8]; 3] = [
            [0, 5, 100, 2047, 1024, 512, 3, 88],
            [2047, 2047, 0, 0, 1, 2046, 1023, 1024],
            [1, 2, 4, 8, 16, 32, 64, 128],
        ];
        for (ci, codes) in cases.iter().enumerate() {
            let mut blob = pack_block_all_present(codes, bw, &[]);
            // The decoder reads via 16-bit words and over-reads the final partial word; in the real
            // block-contiguous layout the next DOF's bytes follow, so pad a standalone blob.
            blob.extend_from_slice(&[0u8; 4]);
            let budget = wv_bit_budget(block_size, bw, 0);
            let (stream, _is_fill) = wv_entropy_unpack(&blob, 0, bw, 0, 0, budget);
            // Decode with mult=1, off=0 so the recovered float IS code · 2^-bw.
            let out = wv_dequant(&stream, bw, 0, 1.0, 0.0, block_size);
            let scale = 2f32.powi(-(bw as i32));
            for i in 0..block_size {
                let want = codes[i] as f32 * scale;
                let d = (out[i] - want).abs();
                assert!(
                    d < 1e-6,
                    "case {ci} code {i}: recovered {} want {} (Δ{d:.2e}) — packer/decoder mismatch",
                    out[i],
                    want
                );
            }
        }
    }

    /// THE WAVELET CODEC CAPSTONE: encode a block through all three Rust stages (transform →
    /// quantize → all-present pack), then decode it through the REAL decoder (entropy → dequant →
    /// inverse), and recover the source within quantization error. This proves the complete wavelet
    /// codec works in native Rust with no Havok DLL in the loop.
    #[test]
    fn full_block_codec_round_trips_end_to_end() {
        let bw = 11u32;
        let cases: [[f32; 8]; 3] = [
            [0.10, 0.15, 0.20, 0.18, 0.12, 0.05, -0.10, -0.20],
            [1.0, 1.02, 1.05, 1.03, 0.98, 0.95, 0.9, 0.88],
            [-2.0, -1.5, 0.0, 1.5, 2.0, 1.0, 0.0, -1.0],
        ];
        for (ci, samples) in cases.iter().enumerate() {
            // ENCODE (Rust)
            let coeffs = forward_wavelet_8(samples);
            let (codes, mult, off) = forward_quantize(&coeffs, bw, 0);
            let mut blob = pack_block_all_present(&codes, bw, &[]);
            blob.extend_from_slice(&[0u8; 4]); // standalone-block padding for the decoder's word over-read
            // DECODE (the verified decoder path)
            let budget = wv_bit_budget(8, bw, 0);
            let (stream, _) = wv_entropy_unpack(&blob, 0, bw, 0, 0, budget);
            let recon = wv_dequant(&stream, bw, 0, mult, off, 8);
            let out = wv_inverse(&recon, 8);
            for i in 0..8 {
                let d = (out[i] - samples[i]).abs();
                assert!(d < 2e-3, "case {ci} dof {i}: end-to-end codec drift {d:.3e}");
            }
        }
    }

    /// The StaticMask encode/decode pair must invert: build a mask from group types + dynamic
    /// flags, and the decoder's dynamic-DOF reader must recover exactly those flags. Covers all-
    /// dynamic (the simplest encoder target), mixed, and static/identity groups.
    #[test]
    fn static_mask_round_trips() {
        use MaskGroup::*;
        // (pos, rot, scale, dynamic[10]) cases → encode → decode dynamic → must match the Mixed
        // groups' flags exactly (static/identity groups contribute no dynamic DOFs).
        let cases: &[(MaskGroup, MaskGroup, MaskGroup, [bool; 10])] = &[
            // all components dynamic (every group Mixed, all bits set) — the simplest encoding.
            (Mixed, Mixed, Mixed, [true; 10]),
            // rotation-only animated; translation constant, scale identity.
            (
                AllStatic,
                Mixed,
                Identity,
                [false, false, false, true, true, true, true, false, false, false],
            ),
            // mixed within a group: tx,tz dynamic, ty static.
            (
                Mixed,
                AllStatic,
                AllStatic,
                [true, false, true, false, false, false, false, false, false, false],
            ),
        ];
        for (pi, &(p, r, s, dynflags)) in cases.iter().enumerate() {
            let mask = encode_static_mask(p, r, s, &dynflags);
            let decoded = static_mask_dynamic(mask);
            // A component can only be reported dynamic if its group is Mixed; compare on that basis.
            let mut want = [false; 10];
            if p == Mixed {
                for c in 0..3 {
                    want[c] = dynflags[c];
                }
            }
            if r == Mixed {
                for c in 3..7 {
                    want[c] = dynflags[c];
                }
            }
            if s == Mixed {
                for c in 7..10 {
                    want[c] = dynflags[c];
                }
            }
            assert_eq!(decoded, want, "case {pi}: mask 0x{mask:04X} dynamic-DOF mismatch");
        }
    }

    /// A full multi-block DOF must round-trip: `encode_dof` (all blocks, one quantizer) → decode
    /// each block through the real decoder (entropy → dequant with the per-DOF mult/off → inverse)
    /// → recover the per-frame values within quantization error. This is the heart of the full-clip
    /// encode — a whole animated curve rebuilt in native Rust.
    #[test]
    fn full_dof_encode_round_trips() {
        let bw = 11u32;
        let bs = 8usize;
        // 20 frames = 3 blocks (last partial), a smooth-ish animated curve.
        let frames: Vec<f32> = (0..20).map(|i| (i as f32 * 0.3).sin() * 0.5 + 0.1).collect();
        let (blocks, mult, off) = encode_dof(&frames, bs, bw);
        assert_eq!(blocks.len(), 3);
        let budget = wv_bit_budget(bs, bw, 0);
        let mut recovered: Vec<f32> = Vec::new();
        for blob in &blocks {
            let mut b = blob.clone();
            b.extend_from_slice(&[0u8; 4]); // standalone-block padding for the decoder's word over-read
            let (stream, _) = wv_entropy_unpack(&b, 0, bw, 0, 0, budget);
            let coeffs = wv_dequant(&stream, bw, 0, mult, off, bs);
            recovered.extend_from_slice(&wv_inverse(&coeffs, bs));
        }
        for i in 0..frames.len() {
            let d = (recovered[i] - frames[i]).abs();
            assert!(d < 3e-3, "frame {i}: DOF round-trip drift {d:.3e}");
        }
    }

    /// THE FULL-CLIP CAPSTONE: encode a whole synthetic clip (multi-track, multi-block, animated +
    /// constant components) into a native wavelet struct+dataBuffer, then decode it straight back
    /// through the REAL `decode_wavelet` and recover every per-frame transform within quant error.
    /// This proves the complete wavelet encoder — a clip built from source in native Rust that the
    /// decoder reads correctly. (The packfile wrapping is separately proven byte-exact.)
    #[test]
    fn full_clip_encode_round_trips_through_decoder() {
        let n_tt = 3usize;
        let n_poses = 20usize;
        let mut frames: Vec<Vec<QsTransform>> = Vec::with_capacity(n_poses);
        for f in 0..n_poses {
            let mut track = Vec::with_capacity(n_tt);
            for t in 0..n_tt {
                let p = f as f32 * 0.12 + t as f32 * 0.7;
                track.push(QsTransform {
                    translation: [p.sin() * 0.5, (p * 1.3).cos() * 0.3, 0.2], // z constant
                    rotation: [(p * 0.2).sin() * 0.1, (p * 0.15).cos() * 0.08, 0.0, 0.99],
                    scale: [1.0, 1.0, 1.0], // constant
                });
            }
            frames.push(track);
        }
        let encoded = encode_wavelet_struct(&frames, 0.66);
        let decoded = decode_wavelet(&encoded).expect("decode the Rust-encoded clip");
        assert_eq!(decoded.n_tt, n_tt);
        assert_eq!(decoded.frames.len(), n_poses);
        for f in 0..n_poses {
            for t in 0..n_tt {
                let a = &frames[f][t];
                let b = &decoded.frames[f][t];
                for k in 0..3 {
                    assert!(
                        (a.translation[k] - b.translation[k]).abs() < 5e-3,
                        "f{f} t{t} trans[{k}]: {} vs {}",
                        a.translation[k],
                        b.translation[k]
                    );
                    assert!((a.scale[k] - b.scale[k]).abs() < 5e-3, "f{f} t{t} scale[{k}]");
                }
                // Quaternion x,y,z (w is decoder-recomputed via the sentinel path).
                for k in 0..3 {
                    assert!(
                        (a.rotation[k] - b.rotation[k]).abs() < 5e-3,
                        "f{f} t{t} rot[{k}]: {} vs {}",
                        a.rotation[k],
                        b.rotation[k]
                    );
                }
            }
        }
    }

    /// Parse `__classnames__` records (signature, name) — the fixed 30-class set an anim packfile
    /// carries, read from the oracle fixture for the integration test.
    fn parse_cn(body: &[u8]) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let mut p = 0;
        while p + 4 <= body.len() {
            let sig = u32::from_le_bytes([body[p], body[p + 1], body[p + 2], body[p + 3]]);
            if sig == 0xFFFF_FFFF {
                break;
            }
            p += 5; // sig + 0x09 separator
            let start = p;
            while p < body.len() && body[p] != 0 {
                p += 1;
            }
            out.push((sig, String::from_utf8_lossy(&body[start..p]).into_owned()));
            p += 1;
        }
        out
    }

    /// THE FULL INTEGRATION GATE: compose the encoder + serializer — encode a clip, wrap the wavelet
    /// object in a 48-byte `hkaAnimationContainer` and a full Havok packfile via `write_packfile`
    /// (with the exact oracle fixups) — then decode the WHOLE thing through the public `parse_anim`.
    /// If this passes, `frames → native Rust packfile → parse_anim` works with ZERO DLL in the loop.
    #[test]
    fn full_encode_to_packfile_decodes_via_parse_anim() {
        use crate::havok_write::{write_packfile, DataSection};
        let n_tt = 4usize;
        let n_poses = 24usize;
        let mut frames: Vec<Vec<QsTransform>> = Vec::with_capacity(n_poses);
        for f in 0..n_poses {
            let mut track = Vec::with_capacity(n_tt);
            for t in 0..n_tt {
                let p = f as f32 * 0.1 + t as f32 * 0.5;
                track.push(QsTransform {
                    translation: [p.sin() * 0.4, (p * 1.1).cos() * 0.25, 0.15],
                    rotation: [(p * 0.2).sin() * 0.12, (p * 0.13).cos() * 0.05, 0.0, 0.98],
                    scale: [1.0, 1.0, 1.0],
                });
            }
            frames.push(track);
        }
        let dur = 0.8f32;
        let wavelet = encode_wavelet_struct(&frames, dur);

        // data object body: 48-byte container + 16-byte array storage (all zero except
        // m_animations.m_size@0xC = 1), then the wavelet object @0x40; padded to 16.
        let mut body = vec![0u8; 0x40];
        body[0x0C] = 1;
        body.extend_from_slice(&wavelet);
        while body.len() % 16 != 0 {
            body.push(0);
        }

        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        let classes = parse_cn(&oracle[0xD0..0x390]);
        let refs: Vec<(u32, &str)> = classes.iter().map(|(s, n)| (*s, n.as_str())).collect();
        let data = DataSection {
            body,
            local: vec![(0x08, 0x30), (0x98, 0xA0)],
            global: vec![(0x30, 2, 0x40)],
            virt: vec![(0x00, 0, 0x272), (0x40, 0, 0x1B8)],
        };
        let packfile = write_packfile(&refs, 0x272, &data);

        let clip = parse_anim(&packfile).expect("parse_anim must read the Rust-built packfile");
        assert_eq!(clip.num_tracks, n_tt, "track count");
        assert_eq!(clip.num_frames, n_poses, "frame count");
        for f in 0..n_poses {
            let t = dur * f as f32 / (n_poses - 1) as f32;
            let sampled = clip.sample_local(t);
            for tr in 0..n_tt {
                let a = &frames[f][tr];
                let b = &sampled[tr];
                for k in 0..3 {
                    assert!(
                        (a.translation[k] - b.translation[k]).abs() < 1e-2,
                        "f{f} tr{tr} trans[{k}]: {} vs {}",
                        a.translation[k],
                        b.translation[k]
                    );
                    assert!((a.scale[k] - b.scale[k]).abs() < 1e-2, "f{f} tr{tr} scale[{k}]");
                }
            }
        }
    }

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
                for c in q
                    .translation
                    .iter()
                    .chain(q.rotation.iter())
                    .chain(q.scale.iter())
                {
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
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/oracle_clip.bin"
        );
        let Ok(buf) = std::fs::read(path) else {
            eprintln!("skip: {path} not present (dump from vz.wad block 3362)");
            return;
        };
        let clip = parse_anim(&buf).expect("parse oracle clip");
        assert_eq!(clip.anim_type, AnimType::Wavelet);
        assert!(clip.decoded);
        assert_eq!(clip.num_frames, 101);
        assert_eq!(clip.num_tracks, 64, "64 mask entries (61 xform + 3 float)");
        assert!(
            (clip.duration - 3.3366).abs() < 1e-3,
            "dur = {}",
            clip.duration
        );

        // FUN_009f0ee0: time 1.496 s → g on the [0, numPoses-1] frame timeline.
        let time = 1.496f32;
        let g = (clip.num_frames as f32 - 1.0) * (time / clip.duration);
        let pose = sample_at_framepos(&clip, g);

        // track 0 — identity.
        let t0 = pose[0];
        assert!(
            t0.translation.iter().all(|c| c.abs() < 1e-3),
            "t0 T {:?}",
            t0.translation
        );
        assert!(
            (t0.rotation[3] - 1.0).abs() < 1e-3 && t0.rotation[..3].iter().all(|c| c.abs() < 1e-3),
            "t0 R {:?}",
            t0.rotation
        );
        assert!(
            t0.scale.iter().all(|s| (s - 1.0).abs() < 1e-3),
            "t0 S {:?}",
            t0.scale
        );

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
        assert_eq!(
            ok, clip.num_tracks,
            "all rotation tracks must match (see stderr)"
        );
    }

    /// The captured `param_4` output buffer (64 × 48-byte hkQsTransform, 3072 B)
    /// from `wavelet_live_oracle.md`, as raw little-endian bytes.
    fn oracle_output_buffer() -> Vec<u8> {
        let md = include_str!("../tests/fixtures/wavelet_live_oracle.md");
        let start = md
            .find("Full raw output buffer")
            .expect("oracle buffer header");
        let after = &md[start..];
        let b = after.find("```").expect("open fence") + 3;
        let e = after[b..].find("```").expect("close fence");
        let hex: String = after[b..b + e]
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
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
        assert!(
            (mid[0].rotation[1] - 0.3827).abs() < 1e-2,
            "y = {}",
            mid[0].rotation[1]
        );
        assert!((qlen(mid[0].rotation) - 1.0).abs() < 1e-3);
        // Track 1 translation lerps halfway.
        assert!(
            (mid[1].translation[0] - 0.5).abs() < 1e-4,
            "tx = {}",
            mid[1].translation[0]
        );
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
