//! Animgroup / animation-skeleton / animation-binding reader.
//!
//! Parses a Mercenaries 2 "animation" WAD block (asset type hash
//! `0x18166555 = pandemic_hash_m2("animation")`) into character animation clips
//! plus the track→bone mapping needed to drive GPU skinning.
//!
//! # What the shipped data actually contains (verified against retail vz.wad)
//!
//! The animation spec (`docs/modernization/skinning_animation_spec.md` §2.2)
//! *predicted* that each animgroup embeds a Havok `hkaSkeleton`
//! (`m_bones`/`m_parentIndices`/`referencePose`) and an `hkaAnimationBinding`
//! (`transformTrackToBoneIndices`). A full census of every animation block in
//! retail vz.wad (190 blocks, 4232 clip packfiles) found **zero** instances of
//! either class — only `hkaAnimationContainer` + one `hkaSkeletalAnimation`
//! subclass (`hkaWavelet…` 4103, `hkaInterleaved…` 73, `hkaDelta…` 56) per clip.
//! Those two class *names* appear in the packfile `__classnames__` table but are
//! never instantiated (no virtual fixup points at them). This matches the prior
//! Python finding (`tools/hk_skeleton.py`: "0/190 slices contain an hkaSkeleton
//! object instance").
//!
//! Instead, the track→bone binding lives in a **Pandemic `trnm` chunk** inside
//! each clip's UCFX wrapper:
//!
//! ```text
//! animgroup block (UCFX entry table)
//!   [u32 entry_count][entry_count × 16B: name_hash, type_hash, field_c, size]
//!   then concatenated containers, one per entry
//!
//! each animation entry (type_hash 0x18166555) is a UCFX container:
//!   +4  data_area_off   +16 n_desc   then 20B descriptor rows
//!   desc "info"  — 2 bytes (version/flags)
//!   desc "data"  — the Havok-5.5.0-r1 32-bit packfile (container + one animation)
//!   desc "trnm"  — [u32 track_count][u32 leading entry][track_count × u32 bone_name_hash]
//!                  (verified `size == 8 + track_count*4`; the leading entry is a
//!                   non-track reference/motion node, see `read_trnm`)
//! ```
//!
//! `trnm` is the real, shipped equivalent of `transformTrackToBoneIndices`, but
//! addressed **by Pandemic node name-hash** (the same hash the mesh `HIER` stores
//! at node `+0`), not by int16 skeleton index. `track_count` equals the animation
//! object's `numTransformTracks` for every clip (verified), which is why `trnm`
//! is unambiguously the per-track bone binding.
//!
//! # Consequence for the renderer (spec Open Q#1, resolved)
//!
//! There is no separate animgroup skeleton to reconcile against the mesh HIER:
//! the animgroup binds tracks directly to HIER node **name-hashes**. To drive
//! skinning, resolve each track `t` to the mesh HIER bone whose `name_hash`
//! equals `binding.track_to_bone_hash[t]` (see [`AnimBinding::resolve_to_hier`]).
//! This is a name-hash lookup, not an index remap — robust to any HIER ordering.

use std::collections::BTreeMap;

use crate::ffcs::read_u32_le;
use crate::havok;
use crate::types::TYPE_HASH_ANIMATION;

/// A 48-byte Havok `hkQsTransform` reference-pose entry: translation (xyz + pad),
/// rotation quaternion (xyzw), scale (xyz + pad). Retained for API compatibility
/// with the spec; shipped Mercs2 animgroups carry **no** `hkaSkeleton`
/// referencePose, so [`AnimSkeleton::reference_pose`] is empty in practice (the
/// bind pose is taken from the mesh HIER instead — see module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QsTransform {
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub scale: [f32; 4],
}

/// The animation skeleton for an animgroup.
///
/// In retail Mercs2 data no `hkaSkeleton` object is serialized, so `parents` and
/// `reference_pose` are empty and `bone_name_hashes` is derived from the union of
/// the clips' `trnm` bindings (the stable set of Pandemic node hashes the clips
/// drive). Consumers should treat `bone_name_hashes` as *identities* to match
/// against the paired model's HIER node name-hashes — NOT as a positional bone
/// array. If a future/DLC animgroup ever ships a real `hkaSkeleton`, this struct
/// is populated from it directly (`parents`/`reference_pose` non-empty).
pub struct AnimSkeleton {
    /// Per-bone Pandemic node name-hashes (HIER node `+0` hashes).
    pub bone_name_hashes: Vec<u32>,
    /// Parent index per bone (`hkaSkeleton::m_parentIndices`). Empty when the
    /// animgroup carries no `hkaSkeleton` (the retail case) — parents live in HIER.
    pub parents: Vec<i16>,
    /// One `hkQsTransform` per bone (`hkaSkeleton::referencePose`). Empty when the
    /// animgroup carries no `hkaSkeleton` (the retail case).
    pub reference_pose: Vec<QsTransform>,
}

/// Track → skeleton-bone mapping for one clip.
///
/// `track_to_bone` holds the int16 indices when a real `hkaAnimationBinding` is
/// present (never, in retail); `track_to_bone_hash` holds the shipped Pandemic
/// name-hash binding read from the `trnm` chunk (always present). Track `t`
/// drives the bone whose HIER name-hash is `track_to_bone_hash[t]`.
pub struct AnimBinding {
    /// `hkaAnimationBinding::transformTrackToBoneIndices` (empty in retail).
    pub track_to_bone: Vec<i16>,
    /// Per-track bone **name-hash** from the `trnm` chunk (the shipped binding).
    pub track_to_bone_hash: Vec<u32>,
}

impl AnimBinding {
    /// Resolve each animation track to a mesh-HIER bone index by name-hash.
    ///
    /// `hier_name_hashes[i]` is the name-hash of HIER node `i` (from
    /// `skeleton.rs` / the model block's HIER chunk, node `+0`). Returns a vector
    /// the length of the track count: `Some(hier_index)` when the track's bone
    /// name-hash is found in the HIER, `None` when the track drives a bone the
    /// mesh does not have (e.g. a prop/weapon node absent from this model).
    pub fn resolve_to_hier(&self, hier_name_hashes: &[u32]) -> Vec<Option<usize>> {
        let lut: BTreeMap<u32, usize> = hier_name_hashes
            .iter()
            .enumerate()
            .map(|(i, &h)| (h, i))
            .collect();
        self.track_to_bone_hash
            .iter()
            .map(|h| lut.get(h).copied())
            .collect()
    }
}

/// Compression class of a clip's `hkaSkeletalAnimation` subobject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipClass {
    /// `hkaWaveletSkeletalAnimation` (the shipped-predominant class).
    Wavelet,
    /// `hkaInterleavedUncompressedAnimation` / `hkaInterleavedSkeletalAnimation`.
    Interleaved,
    /// `hkaDeltaCompressedSkeletalAnimation`.
    Delta,
    /// `hkaSplineSkeletalAnimation`.
    Spline,
    /// A recognised `hka*Animation` class not in the list above.
    Other,
}

impl ClipClass {
    fn from_class_name(name: &str) -> Option<Self> {
        if !name.contains("Animation") {
            return None;
        }
        Some(if name.contains("Wavelet") {
            ClipClass::Wavelet
        } else if name.contains("Interleaved") {
            ClipClass::Interleaved
        } else if name.contains("Delta") {
            ClipClass::Delta
        } else if name.contains("Spline") {
            ClipClass::Spline
        } else if name == "hkaAnimationContainer" || name == "hkaAnimationBinding" {
            return None;
        } else {
            ClipClass::Other
        })
    }

    /// The lowercase decoder key (`wavelet`/`interleaved`/`delta`/…) used in the
    /// spec's §2.3 decoder table.
    pub fn key(self) -> &'static str {
        match self {
            ClipClass::Wavelet => "wavelet",
            ClipClass::Interleaved => "interleaved",
            ClipClass::Delta => "delta",
            ClipClass::Spline => "spline",
            ClipClass::Other => "other",
        }
    }
}

/// One animation clip in an animgroup.
pub struct ClipEntry {
    /// Pandemic name-hash of this clip (the animgroup entry's `name_hash`). Clip
    /// names are stripped on disk; resolve via the rainbow table if a name is
    /// needed. Rendered here as `"0x########"`.
    pub name: String,
    /// The clip's `name_hash` (numeric form of `name`).
    pub name_hash: u32,
    /// Byte offset of the embedded Havok packfile within `block_bytes`.
    pub havok_offset: usize,
    /// Compression class of the clip's animation object.
    pub class: String,
    /// Number of transform tracks (`hkaAnimation::numTransformTracks`, equals the
    /// `trnm` track count).
    pub num_transform_tracks: u32,
    /// Number of float tracks (`hkaAnimation::numFloatTracks`).
    pub num_float_tracks: u32,
    /// Clip duration in seconds (`hkaAnimation::duration`).
    pub duration: f32,
    /// Frame/pose count (`numberOfPoses` for wavelet/delta; `numFrames` for
    /// interleaved). `0` when not applicable/unreadable.
    pub num_poses: u32,
    /// Per-track bone name-hash binding for this clip (from `trnm`).
    pub binding: AnimBinding,
}

/// A parsed animgroup.
pub struct AnimGroup {
    /// Derived skeleton (see [`AnimSkeleton`] — retail: name-hashes only).
    pub skeleton: Option<AnimSkeleton>,
    /// The primary track→bone binding — the binding of the widest clip (most
    /// transform tracks), representative of the full rig. `None` if no clip had a
    /// `trnm`.
    pub binding: Option<AnimBinding>,
    /// Every clip in the animgroup.
    pub clips: Vec<ClipEntry>,
    /// Number of embedded Havok packfiles carved (one per clip, plus any others).
    pub packfile_count: usize,
    /// Aggregate class census across all embedded packfiles.
    pub class_census: BTreeMap<String, u32>,
}

/// Parse the outer UCFX entry table: `[u32 count][count × 16B entry]`.
fn parse_entry_table(block: &[u8]) -> Vec<(u32, u32, u32, u32)> {
    if block.len() < 4 {
        return Vec::new();
    }
    let count = read_u32_le(block, 0) as usize;
    // Bound the count so a corrupt header can't allocate wildly.
    let max = (block.len().saturating_sub(4)) / 16;
    let count = count.min(max);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let b = 4 + i * 16;
        out.push((
            read_u32_le(block, b),
            read_u32_le(block, b + 4),
            read_u32_le(block, b + 8),
            read_u32_le(block, b + 12),
        ));
    }
    out
}

/// Read a `trnm` chunk (`[u32 count][count × u32 name_hash]`) from an animation
/// UCFX container. Returns the per-track bone name-hash list.
fn read_trnm(container: &[u8]) -> Option<Vec<u32>> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > container.len() {
            break;
        }
        if &container[row..row + 4] != b"trnm" {
            continue;
        }
        let u0 = read_u32_le(container, row + 4);
        if u0 == 0xFFFF_FFFF {
            continue;
        }
        let size = read_u32_le(container, row + 8) as usize;
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start + size;
        if end > container.len() || size < 4 {
            return None;
        }
        let t = &container[start..end];
        // trnm body layout (verified against retail vz.wad, all human clips):
        //   [u32 count][u32 leading entry][count × u32 bone_name_hash]
        // The count word is followed by ONE extra leading word before the per-track
        // hashes: `size == 8 + count*4` exactly (e.g. count=105 => 428 bytes,
        // count=61 => 252, count=60 => 248). That leading word is NOT a transform
        // track's bone hash — it is not present in the paired model's HIER and is
        // constant per rig (the reference-frame/motion entry, mirroring how the
        // wavelet stream's track 0 is the first real transform track and root motion
        // is carried separately in m_extractedMotion). Reading the per-track hashes
        // from offset +4 (skipping only the count) shifts every track by one; the
        // correct per-track hashes start at offset +8. Cross-confirmed three ways:
        // (a) `size == 8 + count*4`; (b) the wavelet frame-0 local translations match
        // the resolved HIER bind-local translations only under the +8 read
        // (mean|Δ| 0.028 vs 0.117); (c) the leading word resolves to no HIER bone.
        let count = read_u32_le(t, 0) as usize;
        // A valid trnm holds a leading word + count u32s in `size-4` bytes.
        if count > (size - 4) / 4 {
            return None;
        }
        let mut v = Vec::with_capacity(count);
        for k in 0..count {
            v.push(read_u32_le(t, 8 + k * 4));
        }
        return Some(v);
    }
    None
}

/// The `data` descriptor body (the Havok packfile) of an animation UCFX container.
fn find_data_body(container: &[u8]) -> Option<(usize, &[u8])> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > container.len() {
            break;
        }
        if &container[row..row + 4] != b"data" {
            continue;
        }
        let u0 = read_u32_le(container, row + 4);
        if u0 == 0xFFFF_FFFF {
            continue;
        }
        let size = read_u32_le(container, row + 8) as usize;
        let start = if data_area_off > 0 {
            data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start + size;
        if end > container.len() {
            return None;
        }
        return Some((start, &container[start..end]));
    }
    None
}

/// Havok-5.5.0-r1 32-bit `hkaAnimation` base header, read from the animation
/// object located via the packfile's virtual fixups.
struct AnimHeader {
    class: ClipClass,
    class_name: String,
    duration: f32,
    num_transform_tracks: u32,
    num_float_tracks: u32,
    num_poses: u32,
}

/// Walk a Havok packfile's virtual fixups, find the `hka*Animation` object, and
/// read the `hkaAnimation` base fields. Layout (HK550, 32-bit, from
/// `tools/hk_class_layouts.py`, verified against vz.wad): `animationType@+8`,
/// `duration@+12` (f32), `numTransformTracks@+16`, `numFloatTracks@+20`;
/// wavelet/delta `numberOfPoses@+36`; interleaved `numFrames@+44`.
fn read_anim_header(packfile: &[u8]) -> Option<AnimHeader> {
    const SECTION_HDR: usize = 48;
    let sh = find_sub(packfile, b"__classnames__")?;
    if sh + 3 * SECTION_HDR > packfile.len() {
        return None;
    }
    let mut secs = [[0u32; 7]; 3];
    for (s, sec) in secs.iter_mut().enumerate() {
        for (k, field) in sec.iter_mut().enumerate() {
            *field = read_u32_le(packfile, sh + s * SECTION_HDR + 20 + k * 4);
        }
    }
    let body0 = sh + 3 * SECTION_HDR;
    let cn_len = secs[0][1] as usize;
    let data_pk = body0 + secs[0][6] as usize + secs[1][6] as usize;
    let (d_vf, d_end) = (secs[2][3] as usize, secs[2][4] as usize);

    // classname table: { offset-in-classnames-body : name }
    let cn_end = (body0 + cn_len).min(packfile.len());
    let mut names: BTreeMap<usize, String> = BTreeMap::new();
    let mut p = body0;
    while p + 5 <= cn_end {
        if read_u32_le(packfile, p) == 0xFFFF_FFFF {
            break;
        }
        let mut q = p + 5;
        while q < cn_end && packfile[q] != 0 {
            q += 1;
        }
        if let Ok(s) = std::str::from_utf8(&packfile[p + 5..q]) {
            if !s.is_empty() {
                names.insert(p + 5 - body0, s.to_string());
            }
        }
        p = q + 1;
    }

    // virtual fixups: pick the first hka*Animation instance.
    let vf_end = (data_pk + d_end).min(packfile.len());
    let mut k = data_pk + d_vf;
    while k + 12 <= vf_end {
        let src = read_u32_le(packfile, k) as usize;
        if src == 0xFFFF_FFFF {
            break;
        }
        let cnoff = read_u32_le(packfile, k + 8) as usize;
        k += 12;
        let cname = match names.get(&cnoff) {
            Some(c) => c,
            None => continue,
        };
        let Some(class) = ClipClass::from_class_name(cname) else {
            continue;
        };
        let obj = data_pk + src;
        if obj + 24 > packfile.len() {
            return None;
        }
        let duration = f32::from_bits(read_u32_le(packfile, obj + 12));
        let num_transform_tracks = read_u32_le(packfile, obj + 16);
        let num_float_tracks = read_u32_le(packfile, obj + 20);
        let num_poses = match class {
            ClipClass::Wavelet | ClipClass::Delta if obj + 40 <= packfile.len() => {
                read_u32_le(packfile, obj + 36)
            }
            ClipClass::Interleaved if obj + 48 <= packfile.len() => {
                // hkaInterleaved: numTransforms@+40; frames = numTransforms/tracks.
                let num_transforms = read_u32_le(packfile, obj + 40);
                if num_transform_tracks > 0 {
                    num_transforms / num_transform_tracks
                } else {
                    0
                }
            }
            _ => 0,
        };
        return Some(AnimHeader {
            class,
            class_name: cname.clone(),
            duration,
            num_transform_tracks,
            num_float_tracks,
            num_poses,
        });
    }
    None
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Parse an animation ("animgroup") WAD block: its clips + track→bone bindings.
///
/// `block_bytes` is the **decompressed** block (the outer UCFX entry table). See
/// the module docs for the verified on-disk layout. Returns an [`AnimGroup`] with
/// one [`ClipEntry`] per animation entry, each carrying its own `trnm` binding,
/// plus a derived [`AnimSkeleton`] (name-hashes only, in retail data).
pub fn parse_animgroup(block_bytes: &[u8]) -> Result<AnimGroup, String> {
    let entries = parse_entry_table(block_bytes);
    if entries.is_empty() {
        return Err("animgroup: empty/invalid UCFX entry table".into());
    }

    // Aggregate packfile census over the whole block (cheap, robust to layout).
    let mut class_census: BTreeMap<String, u32> = BTreeMap::new();
    let packfiles = havok::find_packfiles(block_bytes);
    for (_, pf) in &packfiles {
        for (name, count) in &pf.class_counts {
            *class_census.entry(name.clone()).or_insert(0) += count;
        }
    }

    let mut clips = Vec::new();
    let mut pos = 4 + entries.len() * 16;
    for (name_hash, type_hash, _field_c, size) in &entries {
        let size = *size as usize;
        if pos + size > block_bytes.len() {
            break;
        }
        let container_start = pos;
        let container = &block_bytes[pos..pos + size];
        pos += size;
        if *type_hash != TYPE_HASH_ANIMATION {
            continue;
        }

        // trnm = per-track bone name-hash binding.
        let track_to_bone_hash = read_trnm(container).unwrap_or_default();

        // data = the Havok packfile; read its animation header.
        let (class, class_name, duration, ntt, nft, poses, havok_offset) =
            match find_data_body(container) {
                Some((data_rel, data_body)) => match read_anim_header(data_body) {
                    Some(h) => (
                        h.class,
                        h.class_name,
                        h.duration,
                        h.num_transform_tracks,
                        h.num_float_tracks,
                        h.num_poses,
                        container_start + data_rel,
                    ),
                    None => (
                        ClipClass::Other,
                        String::new(),
                        0.0,
                        track_to_bone_hash.len() as u32,
                        0,
                        0,
                        container_start + data_rel,
                    ),
                },
                None => (
                    ClipClass::Other,
                    String::new(),
                    0.0,
                    track_to_bone_hash.len() as u32,
                    0,
                    0,
                    container_start,
                ),
            };
        let _ = class_name;

        clips.push(ClipEntry {
            name: format!("0x{name_hash:08X}"),
            name_hash: *name_hash,
            havok_offset,
            class: class.key().to_string(),
            num_transform_tracks: ntt,
            num_float_tracks: nft,
            duration,
            num_poses: poses,
            binding: AnimBinding {
                track_to_bone: Vec::new(),
                track_to_bone_hash,
            },
        });
    }

    if clips.is_empty() {
        return Err("animgroup: no animation entries in block".into());
    }

    // Primary binding = the widest clip's trnm (the full rig).
    let primary = clips
        .iter()
        .filter(|c| !c.binding.track_to_bone_hash.is_empty())
        .max_by_key(|c| c.binding.track_to_bone_hash.len());
    let binding = primary.map(|c| AnimBinding {
        track_to_bone: Vec::new(),
        track_to_bone_hash: c.binding.track_to_bone_hash.clone(),
    });

    // Derived skeleton: the stable union of bone name-hashes across all clips,
    // ordered by the widest clip's track order (the representative rig order).
    let skeleton = binding.as_ref().map(|b| {
        let mut seen: std::collections::BTreeSet<u32> = b.track_to_bone_hash.iter().copied().collect();
        let mut bone_name_hashes = b.track_to_bone_hash.clone();
        for c in &clips {
            for &h in &c.binding.track_to_bone_hash {
                if seen.insert(h) {
                    bone_name_hashes.push(h);
                }
            }
        }
        AnimSkeleton {
            bone_name_hashes,
            parents: Vec::new(),
            reference_pose: Vec::new(),
        }
    });

    Ok(AnimGroup {
        skeleton,
        binding,
        clips,
        packfile_count: packfiles.len(),
        class_census,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal animgroup block: one animation entry whose UCFX container
    /// carries a `trnm` chunk with three bone name-hashes and a `data` chunk that
    /// is NOT a real packfile (so the header read fails gracefully).
    fn synth_block(track_hashes: &[u32]) -> Vec<u8> {
        // trnm body: [count][leading entry][hashes...] — matches retail layout
        // (`size == 8 + count*4`; the per-track hashes start at offset +8).
        let mut trnm = Vec::new();
        trnm.extend_from_slice(&(track_hashes.len() as u32).to_le_bytes());
        trnm.extend_from_slice(&0xDEAD_0000u32.to_le_bytes()); // leading reference/motion entry
        for &h in track_hashes {
            trnm.extend_from_slice(&h.to_le_bytes());
        }
        // A single UCFX container with one descriptor: trnm.
        // header: "UCFX", data_area_off, _, _, n_desc=1, then a 20B row.
        let n_desc = 1u32;
        let data_area_off = 20 + 20 * n_desc as usize;
        let mut container = vec![0u8; data_area_off];
        container[0..4].copy_from_slice(b"UCFX");
        container[4..8].copy_from_slice(&(data_area_off as u32).to_le_bytes());
        container[16..20].copy_from_slice(&n_desc.to_le_bytes());
        // trnm row at +20: tag, u0=0 (rel to data_area), size=trnm.len()
        container[20..24].copy_from_slice(b"trnm");
        container[24..28].copy_from_slice(&0u32.to_le_bytes());
        container[28..32].copy_from_slice(&(trnm.len() as u32).to_le_bytes());
        container.extend_from_slice(&trnm);

        // Outer entry table: 1 entry (animation type), then the container.
        let mut block = Vec::new();
        block.extend_from_slice(&1u32.to_le_bytes());
        block.extend_from_slice(&0xABCD_1234u32.to_le_bytes()); // name_hash
        block.extend_from_slice(&TYPE_HASH_ANIMATION.to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes()); // field_c
        block.extend_from_slice(&(container.len() as u32).to_le_bytes());
        block.extend_from_slice(&container);
        block
    }

    #[test]
    fn parses_trnm_binding() {
        let hashes = [0x1111_1111, 0x2222_2222, 0x3333_3333];
        let block = synth_block(&hashes);
        let ag = parse_animgroup(&block).expect("parse");
        assert_eq!(ag.clips.len(), 1);
        let clip = &ag.clips[0];
        assert_eq!(clip.name_hash, 0xABCD_1234);
        assert_eq!(clip.binding.track_to_bone_hash, hashes);
        // trnm count is the fallback track count when the header can't be read.
        assert_eq!(clip.num_transform_tracks, 3);
    }

    #[test]
    fn resolve_to_hier_by_name_hash() {
        // HIER order deliberately different from track order.
        let hier = [0x3333_3333u32, 0x9999_9999, 0x1111_1111, 0x2222_2222];
        let binding = AnimBinding {
            track_to_bone: Vec::new(),
            track_to_bone_hash: vec![0x1111_1111, 0x2222_2222, 0x3333_3333, 0xDEAD_BEEF],
        };
        let resolved = binding.resolve_to_hier(&hier);
        assert_eq!(resolved, vec![Some(2), Some(3), Some(0), None]);
    }

    #[test]
    fn skeleton_is_union_of_track_hashes() {
        let block = synth_block(&[0xAA, 0xBB, 0xCC]);
        let ag = parse_animgroup(&block).unwrap();
        let skel = ag.skeleton.expect("derived skeleton");
        assert_eq!(skel.bone_name_hashes, vec![0xAA, 0xBB, 0xCC]);
        assert!(skel.parents.is_empty(), "no hkaSkeleton in retail => no parents");
        assert!(skel.reference_pose.is_empty());
    }

    #[test]
    fn empty_block_errs() {
        assert!(parse_animgroup(&[]).is_err());
        assert!(parse_animgroup(&[0, 0, 0, 0]).is_err());
    }

    /// Live smoke test against retail vz.wad — SKIPS (passes) when the WAD is
    /// absent (CI), so `cargo test -p mercs2_formats` stays green. Runnable in
    /// full via the `animgroup_dump` example. Verifies the known human rig
    /// (block 3315): 16 wavelet clips, ~105-track primary binding, no hkaSkeleton.
    #[test]
    fn live_human_animgroup_if_wad_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let path = std::env::var("VZ_WAD").unwrap_or_else(|_| {
            "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
        });
        let Ok(mut f) = std::fs::File::open(&path) else {
            eprintln!("skip: vz.wad not present at {path}");
            return;
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
        let data = decompress_block(&mut f, &arch.indx, 3315).expect("decompress 3315");
        let ag = parse_animgroup(&data).expect("parse animgroup 3315");

        assert!(ag.clips.len() >= 8, "expected multiple clips, got {}", ag.clips.len());
        assert!(ag.clips.iter().all(|c| c.class == "wavelet"), "all shipped clips wavelet");
        let binding = ag.binding.as_ref().expect("primary binding");
        assert!(binding.track_to_bone_hash.len() >= 60, "human rig ≥60 tracks");
        // Confirmed absent in retail: no hkaSkeleton / hkaAnimationBinding instances.
        assert!(!ag.class_census.contains_key("hkaSkeleton"));
        assert!(!ag.class_census.contains_key("hkaAnimationBinding"));
        assert!(ag.class_census.contains_key("hkaWaveletSkeletalAnimation"));
        // trnm track count == animation numTransformTracks for the widest clip.
        let widest = ag.clips.iter().max_by_key(|c| c.binding.track_to_bone_hash.len()).unwrap();
        assert_eq!(
            widest.binding.track_to_bone_hash.len(),
            widest.num_transform_tracks as usize,
            "trnm count must equal numTransformTracks"
        );
    }

    #[test]
    fn clip_class_keys() {
        assert_eq!(
            ClipClass::from_class_name("hkaWaveletSkeletalAnimation"),
            Some(ClipClass::Wavelet)
        );
        assert_eq!(
            ClipClass::from_class_name("hkaInterleavedUncompressedAnimation"),
            Some(ClipClass::Interleaved)
        );
        assert_eq!(
            ClipClass::from_class_name("hkaDeltaCompressedSkeletalAnimation"),
            Some(ClipClass::Delta)
        );
        assert_eq!(ClipClass::from_class_name("hkaAnimationContainer"), None);
        assert_eq!(ClipClass::from_class_name("hkaAnimationBinding"), None);
    }
}
