//! The `animation` asset container (type hash `0x18166555`, ASET type id 16): read it into its
//! chunks, and write it back byte-for-byte.
//!
//! # The container shape (measured, not assumed)
//!
//! Every `animation` container in retail `vz.wad` — 4,261 of them across 191 blocks — is one
//! **packed leaf UCFX container**:
//!
//! ```text
//! +0   "UCFX"
//! +4   data_area_off = 20 + 20·n
//! +8   0
//! +12  0
//! +16  n                          descriptor count
//! +20  n × 20-byte rows           [tag, off, size, n-1-k, 0]   (k = row index)
//!      data area                  the bodies, in row order, packed with NO padding:
//!                                 row k's `off` is the sum of the sizes of rows 0..k
//!      "CSUM" <u32>               CRC-32 (init 0, no final XOR) of everything above
//! ```
//!
//! No row is a nested-container sentinel (`off == 0xFFFFFFFF`), and the container's length is
//! exactly `data_area_off + Σ size + 8`. `fxdict::write_ucfx_container` does not produce this
//! shape (it pads bodies), which is why this module has its own writer.
//!
//! Two kinds of container share the type:
//!
//! * **Havok clips** — 4,232 containers, `info,data,trnm` (1,969) or `info,data,trnm,evnt`
//!   (2,263). `info` is always the two bytes `01 00`; `data` is a Havok 5.5 packfile (magic
//!   `57 E0 E0 57 10 C0 C0 10`); `trnm` binds the clip's transform tracks to HIER node name-hashes
//!   (see [`crate::animgroup`]); `evnt` is the optional event list decoded by [`parse_evnt`].
//! * **Pandemic keyframe animations** — 29 containers, a `MANM` record followed by `MINF`/`TRCK`
//!   groups (`MANM,MINF,TRCK,…`). No Havok packfile; the prop/object animation path the tag
//!   registry lists as `MANM`/`MINF`/`TRCK`. [`AnimContainerKind::Keyframe`] names them so a
//!   writer that only knows how to build Havok clips can refuse them by kind.
//!
//! Both kinds read and write through the same [`parse_container`] / [`build_container`] pair; the
//! round trip is byte-identical for all 4,261 (`tests/anim_container_roundtrip.rs`).

use crate::animgroup::read_clip_header;
use crate::crc32::crc32_mercs2;

/// The first eight bytes of every Havok 5.5 packfile a retail clip's `data` chunk carries.
pub const HAVOK_PACKFILE_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

/// The `info` chunk every retail Havok clip carries (4,232 of 4,232).
pub const CLIP_INFO: [u8; 2] = [0x01, 0x00];

const HEADER_LEN: usize = 20;
const ROW_LEN: usize = 20;
const CSUM_LEN: usize = 8;
const SENTINEL: u32 = 0xFFFF_FFFF;

/// One chunk of a packed leaf container: its FourCC and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub tag: [u8; 4],
    pub body: Vec<u8>,
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn tag_str(tag: &[u8; 4]) -> String {
    String::from_utf8_lossy(tag).into_owned()
}

/// Read a packed leaf container into its chunks, checking every invariant of the shape.
///
/// Strict on purpose: each rule below holds for every retail `animation` container, so a container
/// that breaks one is not a shape this module can promise to write back, and it says which rule.
pub fn parse_container(c: &[u8]) -> Result<Vec<Chunk>, String> {
    if c.len() < HEADER_LEN + CSUM_LEN || &c[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let n = u32_at(c, 16).ok_or("truncated header")? as usize;
    let data_area = u32_at(c, 4).ok_or("truncated header")? as usize;
    if n == 0 {
        return Err("container has no descriptors".into());
    }
    if data_area != HEADER_LEN + ROW_LEN * n {
        return Err(format!(
            "data area at {data_area}, expected {} for {n} descriptors",
            HEADER_LEN + ROW_LEN * n
        ));
    }
    if u32_at(c, 8) != Some(0) || u32_at(c, 12) != Some(0) {
        return Err("header words +8/+12 are not zero".into());
    }
    if data_area + CSUM_LEN > c.len() {
        return Err("descriptor table runs past the container".into());
    }
    let mut chunks = Vec::with_capacity(n);
    let mut expect_off = 0usize;
    for k in 0..n {
        let r = HEADER_LEN + ROW_LEN * k;
        let tag: [u8; 4] = c[r..r + 4].try_into().expect("4-byte slice");
        let off = u32_at(c, r + 4).expect("row in bounds");
        let size = u32_at(c, r + 8).expect("row in bounds") as usize;
        let back = u32_at(c, r + 12).expect("row in bounds") as usize;
        let last = u32_at(c, r + 16).expect("row in bounds");
        if off == SENTINEL {
            return Err(format!("row {k} ({}) is a nested container", tag_str(&tag)));
        }
        if off as usize != expect_off {
            return Err(format!(
                "row {k} ({}) body at +{off}, expected +{expect_off} (bodies are packed)",
                tag_str(&tag)
            ));
        }
        if back != n - 1 - k {
            return Err(format!(
                "row {k} ({}) word +12 is {back}, expected {}",
                tag_str(&tag),
                n - 1 - k
            ));
        }
        if last != 0 {
            return Err(format!("row {k} ({}) word +16 is {last}, expected 0", tag_str(&tag)));
        }
        let start = data_area + expect_off;
        let body = c
            .get(start..start + size)
            .ok_or_else(|| format!("row {k} ({}) body runs past the container", tag_str(&tag)))?;
        chunks.push(Chunk { tag, body: body.to_vec() });
        expect_off += size;
    }
    let csum_at = data_area + expect_off;
    if csum_at + CSUM_LEN != c.len() {
        return Err(format!(
            "container is {} bytes, expected {} (data area + bodies + CSUM)",
            c.len(),
            csum_at + CSUM_LEN
        ));
    }
    if &c[csum_at..csum_at + 4] != b"CSUM" {
        return Err("no CSUM trailer after the bodies".into());
    }
    let stored = u32_at(c, csum_at + 4).expect("CSUM in bounds");
    let actual = crc32_mercs2(&c[..csum_at]);
    if stored != actual {
        return Err(format!("CSUM 0x{stored:08X} does not match 0x{actual:08X}"));
    }
    Ok(chunks)
}

/// Write chunks as a packed leaf container — the exact inverse of [`parse_container`].
pub fn build_container(chunks: &[Chunk]) -> Vec<u8> {
    let n = chunks.len();
    let data_area = HEADER_LEN + ROW_LEN * n;
    let total: usize = chunks.iter().map(|c| c.body.len()).sum();
    let mut out = Vec::with_capacity(data_area + total + CSUM_LEN);
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(data_area as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    let mut off = 0usize;
    for (k, c) in chunks.iter().enumerate() {
        out.extend_from_slice(&c.tag);
        out.extend_from_slice(&(off as u32).to_le_bytes());
        out.extend_from_slice(&(c.body.len() as u32).to_le_bytes());
        out.extend_from_slice(&((n - 1 - k) as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        off += c.body.len();
    }
    for c in chunks {
        out.extend_from_slice(&c.body);
    }
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    out
}

/// Which of the two retail kinds a parsed `animation` container is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimContainerKind {
    /// `info,data,trnm[,evnt]` — a Havok clip.
    HavokClip { has_events: bool },
    /// `MANM` first, then `MINF`/`TRCK` — a Pandemic keyframe animation.
    Keyframe,
}

/// Classify a parsed container by its chunk tags. An unknown tag sequence is an error, not a guess.
pub fn classify(chunks: &[Chunk]) -> Result<AnimContainerKind, String> {
    let tags: Vec<&[u8; 4]> = chunks.iter().map(|c| &c.tag).collect();
    match tags.as_slice() {
        [b"info", b"data", b"trnm"] => Ok(AnimContainerKind::HavokClip { has_events: false }),
        [b"info", b"data", b"trnm", b"evnt"] => Ok(AnimContainerKind::HavokClip { has_events: true }),
        [b"MANM", rest @ ..]
            if !rest.is_empty() && rest.iter().all(|t| *t == b"MINF" || *t == b"TRCK") =>
        {
            Ok(AnimContainerKind::Keyframe)
        }
        _ => Err(format!(
            "unrecognised animation container chunk sequence: {}",
            tags.iter().map(|t| tag_str(t)).collect::<Vec<_>>().join(",")
        )),
    }
}

/// The track count a `trnm` body declares: the low 16 bits of its first word. The high half is a
/// flag retail sets to `0xFFFF` on a whole class of clips (see [`crate::animgroup`]).
///
/// Errors unless the body is exactly `8 + 4·count` bytes — the identity every retail `trnm` holds.
pub fn trnm_track_count(trnm: &[u8]) -> Result<u32, String> {
    let word = u32_at(trnm, 0).ok_or("trnm is shorter than its count word")?;
    let count = word & 0xFFFF;
    let want = 8 + 4 * count as usize;
    if trnm.len() != want {
        return Err(format!(
            "trnm is {} bytes but declares {count} tracks, which is {want} bytes \
             ([u16 count][u16 flags][u32 lead][count × u32 bone hash])",
            trnm.len()
        ));
    }
    Ok(count)
}

/// One event in an `evnt` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimEvent {
    /// When the event fires, seconds into the clip (an `f32`, compared bit-exactly on write).
    pub time: f32,
    /// The event's name — in retail a sound cue such as `ahj_footstep_solidmetal`, or a gameplay
    /// marker such as `opendoor`. May be empty.
    pub name: String,
    /// The event's category. Retail uses `sound` (4,435 of 9,645 events), empty (2,795),
    /// `sound_surface` (843), `vo` (806), `camera` (642), `sound_weapon` (89), `magazineA` (31) and
    /// `sound_suface` (4, spelled so in retail). Free text as far as the container is concerned.
    pub category: String,
}

fn read_cstr(b: &[u8], pos: &mut usize, what: &str) -> Result<String, String> {
    let rest = b.get(*pos..).ok_or_else(|| format!("{what}: past the end"))?;
    let len = rest
        .iter()
        .position(|&x| x == 0)
        .ok_or_else(|| format!("{what}: no NUL terminator"))?;
    let s = std::str::from_utf8(&rest[..len])
        .map_err(|_| format!("{what}: not UTF-8"))?
        .to_string();
    if !s.is_ascii() {
        return Err(format!("{what}: not ASCII ({s:?})"));
    }
    *pos += len + 1;
    Ok(s)
}

/// Parse an `evnt` chunk body:
///
/// ```text
/// [u32 count]
/// count × { [f32 time] [name, NUL-terminated ASCII] [category, NUL-terminated ASCII] }
/// ```
///
/// Strict: trailing bytes after the last event, a missing NUL, or a non-ASCII byte are errors.
pub fn parse_evnt(body: &[u8]) -> Result<Vec<AnimEvent>, String> {
    let count = u32_at(body, 0).ok_or("evnt is shorter than its count word")? as usize;
    // Each event is at least 6 bytes (time + two empty strings); bound the count by that before
    // allocating, so a corrupt count cannot ask for gigabytes.
    if count > body.len().saturating_sub(4) / 6 {
        return Err(format!("evnt declares {count} events, more than its {} bytes hold", body.len()));
    }
    let mut pos = 4usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bits = u32_at(body, pos).ok_or_else(|| format!("event {i}: time runs past the end"))?;
        pos += 4;
        let name = read_cstr(body, &mut pos, &format!("event {i} name"))?;
        let category = read_cstr(body, &mut pos, &format!("event {i} category"))?;
        out.push(AnimEvent { time: f32::from_bits(bits), name, category });
    }
    if pos != body.len() {
        return Err(format!(
            "evnt has {} trailing byte(s) after its {count} event(s)",
            body.len() - pos
        ));
    }
    Ok(out)
}

/// Write an `evnt` chunk body — the exact inverse of [`parse_evnt`].
pub fn build_evnt(events: &[AnimEvent]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for (i, e) in events.iter().enumerate() {
        for (what, s) in [("name", &e.name), ("category", &e.category)] {
            if !s.is_ascii() || s.as_bytes().contains(&0) {
                return Err(format!("event {i} {what} {s:?} is not NUL-free ASCII"));
            }
        }
        out.extend_from_slice(&e.time.to_bits().to_le_bytes());
        out.extend_from_slice(e.name.as_bytes());
        out.push(0);
        out.extend_from_slice(e.category.as_bytes());
        out.push(0);
    }
    Ok(out)
}

/// Everything wrong with a clip / `trnm` / `evnt` triple, in one pass.
///
/// The pairing rules every retail clip holds:
///
/// * the clip is a Havok 5.5 packfile ([`HAVOK_PACKFILE_MAGIC`]) whose `hkaAnimation` header reads;
/// * the `trnm` is well formed and its track count equals the clip's `numTransformTracks` — the
///   binding is per track, so a mismatch drives the wrong bones or none;
/// * the `evnt`, when present, parses, and its event times are finite, non-negative and in
///   non-decreasing order.
///
/// Event times are NOT bounded by the clip's duration: retail ships 35 events in 13 clips that fire
/// after the clip's `duration` (e.g. block 3272 `0x62991523`, 2.17 s long, carries footsteps out to
/// 3.97 s). A rule retail breaks would reject retail, so it is not one.
///
/// Returns every problem found, empty when the triple is consistent.
pub fn clip_pairing_problems(clip: &[u8], trnm: &[u8], evnt: Option<&[u8]>) -> Vec<String> {
    let mut out = Vec::new();
    let header = if clip.len() < 8 || clip[..8] != HAVOK_PACKFILE_MAGIC {
        out.push(format!(
            "clip is not a Havok 5.5 packfile (expected magic {:02X?})",
            HAVOK_PACKFILE_MAGIC
        ));
        None
    } else {
        let h = read_clip_header(clip);
        if h.is_none() {
            out.push("clip's packfile carries no readable hkaAnimation object".into());
        }
        h
    };
    match trnm_track_count(trnm) {
        Err(e) => out.push(e),
        Ok(count) => {
            if let Some(h) = &header {
                if count != h.num_transform_tracks {
                    out.push(format!(
                        "trnm binds {count} tracks but the clip has {} transform tracks \
                         (numTransformTracks)",
                        h.num_transform_tracks
                    ));
                }
            }
        }
    }
    if let Some(body) = evnt {
        match parse_evnt(body) {
            Err(e) => out.push(format!("evnt: {e}")),
            Ok(events) => {
                let mut previous = 0.0f32;
                for (i, e) in events.iter().enumerate() {
                    if !e.time.is_finite() || e.time < 0.0 {
                        out.push(format!(
                            "event {i} ({:?}) fires at {} s; an event time is a finite, \
                             non-negative number of seconds",
                            e.name, e.time
                        ));
                    } else if e.time < previous {
                        out.push(format!(
                            "event {i} ({:?}) fires at {} s, before event {} at {} s; events are \
                             listed in time order",
                            e.name,
                            e.time,
                            i - 1,
                            previous
                        ));
                    } else {
                        previous = e.time;
                    }
                }
            }
        }
    }
    out
}

/// Build a Havok clip container from its sources, refusing any triple
/// [`clip_pairing_problems`] objects to.
pub fn build_clip_container(clip: &[u8], trnm: &[u8], evnt: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let problems = clip_pairing_problems(clip, trnm, evnt);
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let mut chunks = vec![
        Chunk { tag: *b"info", body: CLIP_INFO.to_vec() },
        Chunk { tag: *b"data", body: clip.to_vec() },
        Chunk { tag: *b"trnm", body: trnm.to_vec() },
    ];
    if let Some(e) = evnt {
        chunks.push(Chunk { tag: *b"evnt", body: e.to_vec() });
    }
    Ok(build_container(&chunks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8; 4], body: &[u8]) -> Chunk {
        Chunk { tag: *tag, body: body.to_vec() }
    }

    #[test]
    fn build_then_parse_is_identity_and_rows_follow_the_retail_rule() {
        let chunks = vec![chunk(b"info", &CLIP_INFO), chunk(b"data", &[1, 2, 3]), chunk(b"trnm", &[9; 12])];
        let c = build_container(&chunks);
        assert_eq!(u32_at(&c, 4), Some(80));
        // row k: [tag, off, size, n-1-k, 0]
        assert_eq!(&c[20..24], b"info");
        assert_eq!(u32_at(&c, 24), Some(0));
        assert_eq!(u32_at(&c, 32), Some(2));
        assert_eq!(u32_at(&c, 44), Some(2)); // data off = 2 (no padding)
        assert_eq!(u32_at(&c, 52), Some(1));
        assert_eq!(u32_at(&c, 64), Some(5)); // trnm off
        assert_eq!(u32_at(&c, 72), Some(0));
        assert_eq!(c.len(), 80 + 2 + 3 + 12 + 8);
        assert_eq!(parse_container(&c).unwrap(), chunks);
    }

    #[test]
    fn a_bad_checksum_is_refused() {
        let mut c = build_container(&[chunk(b"info", &CLIP_INFO)]);
        let last = c.len() - 1;
        c[last] ^= 1;
        assert!(parse_container(&c).unwrap_err().contains("CSUM"));
    }

    #[test]
    fn a_padded_body_is_refused() {
        let mut c = build_container(&[chunk(b"info", &CLIP_INFO), chunk(b"data", &[7; 4])]);
        // Move `data` one byte later, as a padding writer would.
        c[44..48].copy_from_slice(&3u32.to_le_bytes());
        assert!(parse_container(&c).unwrap_err().contains("packed"));
    }

    #[test]
    fn evnt_round_trips_and_matches_a_retail_body() {
        // A retail body: 1 event at 0.2 s, name "opendoor", empty category.
        let retail = [
            0x01, 0, 0, 0, 0xCD, 0xCC, 0x4C, 0x3E, b'o', b'p', b'e', b'n', b'd', b'o', b'o', b'r', 0, 0,
        ];
        let ev = parse_evnt(&retail).unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].name, "opendoor");
        assert_eq!(ev[0].category, "");
        assert!((ev[0].time - 0.2).abs() < 1e-6);
        assert_eq!(build_evnt(&ev).unwrap(), retail);
    }

    #[test]
    fn evnt_trailing_bytes_and_missing_nul_are_errors() {
        let mut b = build_evnt(&[AnimEvent { time: 0.0, name: "a".into(), category: "sound".into() }]).unwrap();
        b.push(0);
        assert!(parse_evnt(&b).unwrap_err().contains("trailing"));
        let b = [1u8, 0, 0, 0, 0, 0, 0, 0, b'a'];
        assert!(parse_evnt(&b).is_err());
    }

    #[test]
    fn trnm_size_identity_uses_the_low_half_only() {
        let mut t = (3u32 | 0xFFFF_0000).to_le_bytes().to_vec();
        t.extend_from_slice(&[0; 4 + 12]);
        assert_eq!(trnm_track_count(&t), Ok(3));
        t.push(0);
        assert!(trnm_track_count(&t).is_err());
    }

    #[test]
    fn a_non_havok_clip_is_refused_with_every_problem_listed() {
        let t = {
            let mut t = 2u32.to_le_bytes().to_vec();
            t.extend_from_slice(&[0; 4 + 8]);
            t
        };
        let problems = clip_pairing_problems(b"not a packfile", &t, Some(&[5, 0, 0, 0]));
        assert!(problems.iter().any(|p| p.contains("Havok")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("evnt")), "{problems:?}");
        assert!(build_clip_container(b"not a packfile", &t, None).is_err());
    }

    fn trnm_of(count: u32) -> Vec<u8> {
        let mut t = count.to_le_bytes().to_vec();
        t.extend_from_slice(&0u32.to_le_bytes());
        for k in 0..count {
            t.extend_from_slice(&(0x1000 + k).to_le_bytes());
        }
        t
    }

    /// A real Havok clip (the KS-750 fixture) pairs with a `trnm` of its own track count, and the
    /// container it builds reads back as a Havok clip with the three sources verbatim.
    #[test]
    fn a_real_clip_builds_with_a_matching_trnm_and_is_refused_with_a_wrong_one() {
        let clip: &[u8] = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        let h = read_clip_header(clip).expect("fixture carries an hkaAnimation");
        assert!(h.num_transform_tracks > 0);
        let trnm = trnm_of(h.num_transform_tracks);
        let evnt = build_evnt(&[AnimEvent { time: 0.1, name: "step".into(), category: "sound".into() }]).unwrap();
        let c = build_clip_container(clip, &trnm, Some(&evnt)).expect("a consistent triple builds");
        let chunks = parse_container(&c).unwrap();
        assert_eq!(classify(&chunks), Ok(AnimContainerKind::HavokClip { has_events: true }));
        assert_eq!(chunks[1].body, clip);
        assert_eq!(chunks[2].body, trnm);
        assert_eq!(chunks[3].body, evnt);

        let wrong = trnm_of(h.num_transform_tracks + 1);
        let e = build_clip_container(clip, &wrong, None).unwrap_err();
        assert!(e.contains("numTransformTracks"), "{e}");
    }

    #[test]
    fn events_out_of_order_or_negative_are_refused() {
        let clip: &[u8] = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        let trnm = trnm_of(read_clip_header(clip).unwrap().num_transform_tracks);
        let ev = |t: f32| AnimEvent { time: t, name: "x".into(), category: String::new() };
        let backwards = build_evnt(&[ev(0.5), ev(0.25)]).unwrap();
        assert!(clip_pairing_problems(clip, &trnm, Some(&backwards))[0].contains("time order"));
        let negative = build_evnt(&[ev(-0.1)]).unwrap();
        assert!(clip_pairing_problems(clip, &trnm, Some(&negative))[0].contains("non-negative"));
        // Past the clip's duration is legal: retail does it (35 events in 13 clips).
        let late = build_evnt(&[ev(1.0e3)]).unwrap();
        assert!(clip_pairing_problems(clip, &trnm, Some(&late)).is_empty());
    }

    #[test]
    fn classify_names_both_kinds_and_refuses_the_rest() {
        let clip = [chunk(b"info", &[]), chunk(b"data", &[]), chunk(b"trnm", &[])];
        assert_eq!(classify(&clip), Ok(AnimContainerKind::HavokClip { has_events: false }));
        let kf = [chunk(b"MANM", &[]), chunk(b"MINF", &[]), chunk(b"TRCK", &[])];
        assert_eq!(classify(&kf), Ok(AnimContainerKind::Keyframe));
        assert!(classify(&[chunk(b"data", &[])]).is_err());
    }
}
