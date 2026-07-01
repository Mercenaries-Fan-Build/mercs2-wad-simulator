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
        AnimType::Wavelet | AnimType::Delta | AnimType::Spline => {
            // Header decoded faithfully; frame reconstruction is proprietary and
            // absent from this workspace — return a header-only clip.
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
    /// duration ~0.968 s, 30 poses). The header must decode faithfully even
    /// though the wavelet frames themselves are not reconstructable here.
    #[test]
    fn ks750_anim_header_decodes() {
        let buf: &[u8] = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        let clip = parse_anim(buf).expect("parse anim");

        assert_eq!(clip.anim_type, AnimType::Wavelet, "wavelet-compressed clip");
        assert!(clip.duration > 0.0, "duration = {}", clip.duration);
        assert!(clip.num_tracks > 0, "num_tracks = {}", clip.num_tracks);
        assert!(clip.num_frames >= 1, "num_frames = {}", clip.num_frames);
        // Concrete oracle values observed in the fixture bytes.
        assert_eq!(clip.num_tracks, 60);
        assert_eq!(clip.num_frames, 30);
        assert!(
            (clip.duration - 0.967_633).abs() < 1e-4,
            "duration = {}",
            clip.duration
        );

        // sample_local always yields one transform per track with unit quats.
        let pose = clip.sample_local(0.0);
        assert_eq!(pose.len(), clip.num_tracks);
        for t in &pose {
            assert!((qlen(t.rotation) - 1.0).abs() < 1e-3, "|q| = {}", qlen(t.rotation));
        }
        // Not decodable here → neutral pose, and honestly flagged.
        assert!(!clip.decoded, "wavelet frames are not reconstructed");
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
