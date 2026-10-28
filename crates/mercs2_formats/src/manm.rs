//! `MANM` material animations: the 29 `animation` assets that are not Havok clips.
//!
//! Script plays them with `Object.PlayMaterialAnimation(guid, "<name>", loop)` (`resident/jammer.lua`,
//! `beacon.lua`, `mine.lua`, `treetrunk*.lua`, `materialanimation_*.lua`). The container is the packed
//! leaf shape of [`crate::anim_container`], its chunks `MANM, (MINF, TRCK×n)…`:
//!
//! ```text
//! MANM  16 B   [u32][u32][f32][u32]           the f32 is 29.97003, 59.94006 or 15.0 in retail
//! MINF   6 B   [u32 group hash][u16 track count]   the TRCK chunks that follow belong to it
//! TRCK         [u32 property hash][f32 divisor][u16 key count N]
//!              then N × [i16 time][i16 value]       times non-decreasing
//! ```
//!
//! Every retail container decodes under this layout (`tests/anim_container_roundtrip.rs`: 29
//! containers, 378 tracks). The sampler is the retail `FUN_0068E9A0` (pc-v10): both halves of a
//! key are read with `movswl` + `cvtsi2ss`, a value is scaled by `1 / divisor`, linear between two
//! keys with different times, the first key's value before it and the last key's value after the
//! last. [`Track::sample`] is that function, operation for operation.
//!
//! Not established, and so not modelled here: the unit of the key times and where the sampler's time
//! comes from (the controller `Object.PlayMaterialAnimation` allocates zeroes a time at `+0x20`;
//! what advances it is not read), what the three other `MANM` words mean, and what the group and
//! property hashes name. The per-frame apply `FUN_0068ECA0` writes each sampled value into a field
//! of a material-controller object chosen by the property hash; the fields are unnamed.

use crate::anim_container::{classify, parse_container, AnimContainerKind};

/// One property track: keyframes over time, sampled by [`Track::sample`].
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The hash naming the property the track drives (the switch key in `FUN_0068ECA0`).
    pub property: u32,
    /// Every sampled value is a key value times `1 / divisor`.
    pub divisor: f32,
    /// `(time, value)` in file order; times are non-decreasing.
    pub keys: Vec<(i16, i16)>,
}

/// A `MINF` group and the tracks that follow it.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub hash: u32,
    pub tracks: Vec<Track>,
}

/// A decoded `MANM` container.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialAnim {
    /// The `MANM` body's four words as stored.
    pub header: [u32; 4],
    pub groups: Vec<Group>,
}

impl MaterialAnim {
    /// The `MANM` body's f32 at +8.
    pub fn header_f32(&self) -> f32 {
        f32::from_bits(self.header[2])
    }

    /// The first and last key time over every track.
    pub fn time_span(&self) -> (i16, i16) {
        let times = self.groups.iter().flat_map(|g| &g.tracks).flat_map(|t| t.keys.iter().map(|k| k.0));
        times.fold((i16::MAX, i16::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)))
    }

    pub fn track_count(&self) -> usize {
        self.groups.iter().map(|g| g.tracks.len()).sum()
    }
}

impl Track {
    /// The retail sampler (`FUN_0068E9A0`, pc-v10 `0x68E9A0`–`0x68EAD4`) at time `t`, in key-time
    /// units, with its f32 operation order: `r = 1/divisor`; find the first key `j` whose time is
    /// greater than `t`; none → the last value; `j == 0` → key 0's value; otherwise `vp + (vc − vp)
    /// · (t − tᵢ) / (tⱼ − tᵢ)` with `i = j − 1`, `vp = vᵢ·r`, `vc = vⱼ·r`. So a run of keys sharing
    /// a time `t` gives the last of them. [`parse_material_anim`] refuses a track with no keys (the
    /// retail sampler would read the two bytes before its array).
    pub fn sample(&self, t: f32) -> f32 {
        let r = 1.0f32 / self.divisor;
        let time = |k: usize| self.keys[k].0 as f32;
        let value = |k: usize| self.keys[k].1 as f32 * r;
        let n = self.keys.len();
        let Some(j) = (0..n).find(|&k| time(k) > t) else { return value(n - 1) };
        if j == 0 {
            return value(0);
        }
        let i = j - 1;
        let (vp, vc) = (value(i), value(j));
        (vc - vp) * (t - time(i)) / (time(j) - time(i)) + vp
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn i16_at(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([b[o], b[o + 1]])
}

/// Decode one `animation` container that holds a material animation. A Havok clip, or a chunk
/// whose size or count disagrees with the layout above, is an error naming the chunk.
pub fn parse_material_anim(container: &[u8]) -> Result<MaterialAnim, String> {
    let chunks = parse_container(container)?;
    if classify(&chunks)? != AnimContainerKind::Keyframe {
        return Err("a Havok clip, not a MANM material animation".into());
    }
    let manm = &chunks[0].body;
    if manm.len() != 16 {
        return Err(format!("MANM is {} bytes, expected 16", manm.len()));
    }
    let header = [u32_at(manm, 0), u32_at(manm, 4), u32_at(manm, 8), u32_at(manm, 12)];
    let mut groups: Vec<(Group, usize)> = Vec::new();
    for (i, c) in chunks.iter().enumerate().skip(1) {
        let b = &c.body;
        match &c.tag {
            b"MINF" => {
                if b.len() != 6 {
                    return Err(format!("chunk {i}: MINF is {} bytes, expected 6", b.len()));
                }
                let n = u16::from_le_bytes([b[4], b[5]]) as usize;
                groups.push((Group { hash: u32_at(b, 0), tracks: Vec::with_capacity(n) }, n));
            }
            b"TRCK" => {
                let (g, _) = groups.last_mut().ok_or(format!("chunk {i}: TRCK before any MINF"))?;
                if b.len() < 10 {
                    return Err(format!("chunk {i}: TRCK is {} bytes, shorter than its header", b.len()));
                }
                let n = u16::from_le_bytes([b[8], b[9]]) as usize;
                if n == 0 {
                    return Err(format!("chunk {i}: TRCK has no keys"));
                }
                if b.len() != 10 + 4 * n {
                    return Err(format!("chunk {i}: TRCK is {} bytes, {n} keys need {}", b.len(), 10 + 4 * n));
                }
                let keys: Vec<(i16, i16)> =
                    (0..n).map(|k| (i16_at(b, 10 + 4 * k), i16_at(b, 12 + 4 * k))).collect();
                if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                    return Err(format!("chunk {i}: TRCK key times decrease"));
                }
                g.tracks.push(Track { property: u32_at(b, 0), divisor: f32::from_bits(u32_at(b, 4)), keys });
            }
            other => return Err(format!("chunk {i}: unexpected {}", String::from_utf8_lossy(other))),
        }
    }
    for (g, n) in &groups {
        if g.tracks.len() != *n {
            return Err(format!("group 0x{:08X}: MINF says {n} tracks, {} follow", g.hash, g.tracks.len()));
        }
    }
    Ok(MaterialAnim { header, groups: groups.into_iter().map(|(g, _)| g).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim_container::{build_container, Chunk};

    fn trck(property: u32, divisor: f32, keys: &[(i16, i16)]) -> Chunk {
        let mut b = Vec::new();
        b.extend_from_slice(&property.to_le_bytes());
        b.extend_from_slice(&divisor.to_bits().to_le_bytes());
        b.extend_from_slice(&(keys.len() as u16).to_le_bytes());
        for (t, v) in keys {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        Chunk { tag: *b"TRCK", body: b }
    }

    fn minf(hash: u32, n: u16) -> Chunk {
        let mut b = hash.to_le_bytes().to_vec();
        b.extend_from_slice(&n.to_le_bytes());
        Chunk { tag: *b"MINF", body: b }
    }

    fn manm() -> Chunk {
        let mut b = Vec::new();
        for w in [2u32, 2, 29.97003f32.to_bits(), 3] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        Chunk { tag: *b"MANM", body: b }
    }

    #[test]
    fn decodes_groups_and_tracks() {
        let c = build_container(&[
            manm(),
            minf(0xAAAA_0001, 2),
            trck(0x1111_1111, 32767.0, &[(0, 0), (10, 32767)]),
            trck(0x2222_2222, 1.0, &[(5, -3)]),
            minf(0xAAAA_0002, 1),
            trck(0x3333_3333, 2.0, &[(0, 4), (0, 8), (4, 8)]),
        ]);
        let m = parse_material_anim(&c).expect("decode");
        assert_eq!(m.groups.len(), 2);
        assert_eq!(m.track_count(), 3);
        assert_eq!(m.groups[0].tracks[1].keys, vec![(5, -3)]);
        assert_eq!(m.time_span(), (0, 10));
        assert!((m.header_f32() - 29.97003).abs() < 1e-5);
    }

    #[test]
    fn sampler_matches_the_retail_rules() {
        let t = Track { property: 0, divisor: 2.0, keys: vec![(2, 4), (2, 8), (6, 16)] };
        assert_eq!(t.sample(1.0), 2.0, "the first key's value before it");
        assert_eq!(t.sample(2.0), 4.0, "at a shared time the last key of the run gives the value");
        assert_eq!(t.sample(4.0), 6.0, "linear between keys with different times");
        assert_eq!(t.sample(6.0), 8.0, "the last key's value at its time");
        assert_eq!(t.sample(9.0), 8.0, "and after it");
        let one = Track { property: 0, divisor: 4.0, keys: vec![(3, 2)] };
        assert_eq!((one.sample(0.0), one.sample(9.0)), (0.5, 0.5), "one key: its value everywhere");
    }

    /// The scale is a multiply by the reciprocal, as retail does it, not a divide: the two differ
    /// in the last bit for some divisors.
    #[test]
    fn values_scale_by_the_reciprocal() {
        let d = 3.0f32;
        let t = Track { property: 0, divisor: d, keys: vec![(0, 32767)] };
        assert_eq!(t.sample(0.0).to_bits(), (32767.0f32 * (1.0 / d)).to_bits());
    }

    #[test]
    fn a_track_with_no_keys_is_an_error() {
        let c = build_container(&[manm(), minf(1, 1), trck(1, 1.0, &[])]);
        assert!(parse_material_anim(&c).unwrap_err().contains("no keys"));
    }

    #[test]
    fn a_track_count_mismatch_is_an_error() {
        let c = build_container(&[manm(), minf(1, 2), trck(1, 1.0, &[(0, 1)])]);
        let e = parse_material_anim(&c).unwrap_err();
        assert!(e.contains("MINF says 2 tracks, 1 follow"), "{e}");
    }

    #[test]
    fn a_short_trck_is_an_error() {
        let mut bad = trck(1, 1.0, &[(0, 1), (1, 2)]);
        bad.body.truncate(14);
        let c = build_container(&[manm(), minf(1, 1), bad]);
        assert!(parse_material_anim(&c).unwrap_err().contains("2 keys need 18"));
    }
}
