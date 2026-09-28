//! Soundbank codec — the `soundbank` table (`0x9F8BCA10`, `Sound.LoadSoundBank` `FUN_005e2630`): the
//! bank's sound **groups** (which wave(s) a sound plays and how) and its **cues** (what a cue name
//! triggers). A `sounddb` entry routes a cue name to `(soundbank, cue index)`; the cue names a group;
//! the group names `(wavebank, wave index)`.
//!
//! ## Layout (measured on every soundbank in retail `vz.wad`, `English.wad` and `shell.wad`)
//!
//! ```text
//! header (32 bytes, little-endian)
//!   +0x00 u32  table version 0x1D
//!   +0x04 u32  bank hash = m2(bank name)
//!   +0x08 u16  group count
//!   +0x0A u16  cue count
//!   +0x0C u32  the bank hash again
//!   +0x10 u32  0x20 — start of the group section
//!   +0x14 u32  offset of the group-offset table
//!   +0x18 u32  start of the cue section
//!   +0x1C u32  offset of the cue-offset table
//! group section   the groups, back to back, from 0x20
//! group table     group count × u32, each group's offset RELATIVE TO 0x20
//! cue section     the cues, back to back
//! cue table       cue count × u32, each cue's offset RELATIVE TO THE CUE SECTION; the body ends here
//! ```
//!
//! Sections are contiguous with no padding: the group table starts where the last group ends, the cue
//! section where the group table ends, the cue table where the last cue ends.
//!
//! **Group** — two forms, told apart by `+0x0C`:
//!
//! ```text
//! common head
//!   +0x00 u32  sound id (read only by FUN_008369e0's language gate; equals the playing cue's guid in
//!              some groups, not in others)
//!   +0x04 u32  category hash, m2 of a Mercs2Globals category (e.g. m2("ui") = 0x8EC83583)
//!   +0x08 u32  0
//!   +0x0C u32  form: 0 = single-wave (64 bytes), 1 = multi-wave (0x68 + 12 × waves bytes)
//!   +0x10 f32  priority (GetWavePriority, voice stealing)   +0x14 u32 0 | 1 positional
//!   +0x18 f32  min distance   +0x1C f32 max distance
//!   +0x20 f32  no engine reader known   +0x24 f32 distance exponent   +0x28 f32 Doppler scale
//! single-wave form
//!   +0x2C f32  base volume (FUN_0083d770)   +0x30 f32 base pitch, semitones (FUN_0083d700)
//!   +0x34 wave {wavebank hash, wave index, f32 weight}
//! multi-wave form
//!   +0x2C u8   wave loop count (copied to the sound instance at +0x80 by FUN_008369e0, and on to
//!              the wave; see crate::engine::CueError::Looping)
//!   +0x2D u8   wave count
//!   +0x2E u8   selection mode: 0 sequential, 1 weighted random, 2 weighted random without an
//!              immediate repeat (FUN_0083d410); any other value plays nothing
//!   +0x2F u8   unknown
//!   +0x30 f32, +0x34 f32 unknown   +0x38 u32 0x2C (the wave list is read at group + this + 0x3C)
//!   +0x3C f32 a, +0x40 f32 b: start delay drawn in [max(a − b, 0), b + a] (FUN_0083d7e0)
//!   +0x44 u32 0 (its low byte, when set, adds a distance delay)
//!   +0x48 u32  unknown flags   +0x4C f32 unknown
//!   +0x50 f32, +0x54 f32 base volume range (FUN_0083d770)   +0x58 f32 unknown
//!   +0x5C f32, +0x60 f32 base pitch range, semitones (FUN_0083d700)   +0x64 f32 unknown
//!   +0x68 wave count × {wavebank hash, wave index, f32 weight}
//! ```
//!
//! `+0x14`..`+0x2B` are the 3D parameters: when an instance plays through an emitter,
//! `FUN_00837830` (`0x00837C08`) hands `group + 0x14` to the wave (vtable `+0x60`, `0x00838F70`), which
//! copies the 24 bytes to its `+0x5C`. `+0x14` then gates the distance volume (`FUN_00839ae0`), which
//! `FUN_0083d3a0` computes from `+0x18` (minimum distance), `+0x1C` (maximum distance) and `+0x24` (the
//! exponent); `+0x28` scales the source's Doppler factor (`FUN_0083b120`, via wave `+0x70`). `+0x14` also
//! makes the instance positional (`FUN_00837830`, `0x008378A9`). The base volume, base pitch, start
//! delay and loop count are read in the engine functions named. How the engine picks a wave is in [`crate::select`].
//!
//! **Cue** — two forms, told apart by the byte at `+0x05`:
//!
//! ```text
//! common head
//!   +0x00 u32  cue guid = m2(cue name)
//!   +0x04 u8x4 [0, form, start limit, 0]; form 0 = single-track, 1 = multi-track
//!   +0x08 f32  gain
//!   +0x0C f32  length in seconds (frames / rate of the wave, for an embedded single-wave cue)
//! single-track form (24 bytes)
//!   +0x10 u32  soundbank hash
//!   +0x14 u16  group index      +0x16 u16 no engine reader known (0 in most cues; float-like high
//!              halves in others)
//! multi-track form
//!   +0x10      tracks of timed sounds, each picking a group — see [`crate::multitrack`]
//! ```
//!
//! [`Soundbank::parse`] and [`Soundbank::to_bytes`] are inverses over every retail soundbank
//! (`tests/retail_banks.rs`). Anything outside this layout is a hard [`SoundbankError`].

use crate::le::{f32_at, put_f32, put_u16, put_u32, u16_at, u32_at, u8_at};
use crate::multitrack::{MultiTrackCue, MultiTrackError};
use crate::wave::TABLE_VERSION;

/// Soundbank header size; also the start of the group section (`+0x10` always holds this).
pub const HEADER_SIZE: usize = 0x20;
/// Size of a single-wave group.
pub const SINGLE_GROUP_SIZE: usize = 0x40;
/// Size of a multi-wave group before its wave list.
pub const MULTI_GROUP_HEAD: usize = 0x68;
/// Size of one `{wavebank, index, weight}` wave reference.
pub const WAVE_REF_SIZE: usize = 12;
/// Size of a single-track cue.
pub const SINGLE_CUE_SIZE: usize = 24;
/// The constant a multi-wave group carries at `+0x38`.
pub const MULTI_GROUP_WORD_38: u32 = 0x2C;

/// One wave a group can play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveRef {
    /// Hash of the wavebank holding the wave.
    pub wavebank: u32,
    /// Index of the wave's record in that wavebank.
    pub index: u32,
    /// Weight (1.0 for a single-wave group; the multi-wave weights of a group sum to ~1).
    pub weight: f32,
}

/// The fields both group forms share (`+0x00`..`+0x2B`, less the fixed `+0x08` and the form word).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupHead {
    /// `+0x00` sound id. Its one known reader, `FUN_008369e0`, refuses to start a group whose id is
    /// `0xEA1343AA`, `0xC05D8686` or `0xBB8AE67D` unless the game runs in English.
    pub sound_id: u32,
    /// `+0x04` category hash.
    pub category: u32,
    /// `+0x10` priority: `GetWavePriority` returns it times the wave's distance volume, and with
    /// every voice busy a new instance takes the lowest-priority wave's voice only when its own is
    /// higher (`FUN_00837e10`, `FUN_00837830`).
    pub unknown_10: f32,
    /// `+0x14`, 0 or 1 in every retail group: the instance is positional and the wave takes a distance
    /// volume (module docs).
    pub unknown_14: u32,
    /// `+0x18` minimum distance: full volume up to it (`FUN_0083d3a0`).
    pub min_distance: f32,
    /// `+0x1C` maximum distance: silent from it (`FUN_0083d3a0`).
    pub max_distance: f32,
    /// `+0x20`: copied into the wave (`+0x68`), whose getter has no call site; no engine reader is
    /// known (1.0 in all but one retail group).
    pub unknown_20: f32,
    /// `+0x24` the distance fall-off exponent (`FUN_0083d3a0`).
    pub distance_exponent: f32,
    /// `+0x28` the Doppler scale (`FUN_0083b120`).
    pub doppler_scale: f32,
}

/// The multi-wave form's fields after the common head.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiGroup {
    /// `+0x2C` the wave loop count (0 plays once).
    pub byte_2c: u8,
    /// `+0x2E` selection mode: 0 sequential, 1 weighted random, 2 weighted random without an
    /// immediate repeat ([`crate::select`]).
    pub selection: u8,
    /// `+0x2F`, unknown.
    pub byte_2f: u8,
    /// `+0x30`, unknown.
    pub unknown_30: f32,
    /// `+0x34`, unknown.
    pub unknown_34: f32,
    /// `+0x3C` start delay `a`: the delay is drawn in `[max(a − b, 0), b + a]` (`FUN_0083d7e0`).
    pub unknown_3c: f32,
    /// `+0x40` start delay `b`.
    pub unknown_40: f32,
    /// `+0x48`, unknown flag bytes.
    pub word_48: u32,
    /// `+0x4C`..`+0x63`: `[0]` unknown, `[1]`/`[2]` the base volume range (`FUN_0083d770`), `[3]`
    /// unknown, `[4]`/`[5]` the base pitch range in semitones (`FUN_0083d700`).
    pub floats_4c: [f32; 6],
    /// `+0x64`, unknown.
    pub unknown_64: f32,
    /// `+0x68` onward; the count at `+0x2D` is this list's length.
    pub waves: Vec<WaveRef>,
}

/// A group's form-specific fields.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupForm {
    /// Form 0: exactly one wave.
    Single {
        /// `+0x2C` the sound instance's base volume (`FUN_0083d770`).
        gain: f32,
        /// `+0x30` the sound instance's base pitch in semitones (`FUN_0083d700`).
        unknown_30: f32,
        /// `+0x34` the wave.
        wave: WaveRef,
    },
    /// Form 1: a list of weighted waves.
    Multi(MultiGroup),
}

/// A sound group.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The shared head.
    pub head: GroupHead,
    /// The form-specific tail.
    pub form: GroupForm,
}

impl Group {
    /// The waves this group can play.
    pub fn waves(&self) -> &[WaveRef] {
        match &self.form {
            GroupForm::Single { wave, .. } => std::slice::from_ref(wave),
            GroupForm::Multi(m) => &m.waves,
        }
    }

    fn encoded_len(&self) -> usize {
        match &self.form {
            GroupForm::Single { .. } => SINGLE_GROUP_SIZE,
            GroupForm::Multi(m) => MULTI_GROUP_HEAD + m.waves.len() * WAVE_REF_SIZE,
        }
    }
}

/// A cue's form-specific fields.
#[derive(Clone, Debug, PartialEq)]
pub enum CueBody {
    /// Form 0: the cue plays one group.
    SingleTrack {
        /// `+0x10` the soundbank holding the group (every retail cue names its own bank).
        soundbank: u32,
        /// `+0x14` index of the group in that soundbank.
        group_index: u16,
        /// `+0x16`: no engine reader is known — the group reference is read at `+0x10` and `+0x14`
        /// only (0 in most cues).
        unknown_16: u16,
    },
    /// Form 1: tracks of timed sounds ([`crate::multitrack`]).
    MultiTrack(MultiTrackCue),
}

/// A cue.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    /// `+0x00` cue guid = m2(cue name).
    pub guid: u32,
    /// `+0x06` start limit: the engine starts the cue only while fewer than this many of its
    /// instances play (`FUN_00834ad0`; `FUN_008354e0` counts one up as an instance plays,
    /// `FUN_00835850` one down as it finishes); 0 = no limit.
    pub byte_06: u8,
    /// `+0x08` gain.
    pub gain: f32,
    /// `+0x0C` length in seconds.
    pub length_s: f32,
    /// The form-specific tail.
    pub body: CueBody,
}

/// A soundbank table exactly as it sits in the `data` chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Soundbank {
    /// `+0x04` / `+0x0C` bank hash.
    pub bank_hash: u32,
    /// The groups, in index order.
    pub groups: Vec<Group>,
    /// The cues, in index order (the order a sounddb entry's cue index addresses).
    pub cues: Vec<Cue>,
}

/// Everything that can be wrong with a soundbank body. Each is a hard error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundbankError {
    /// A field ran past the end of the body.
    Truncated { field: &'static str, offset: usize, len: usize },
    /// `+0x00` was not 0x1D.
    BadVersion(u32),
    /// `+0x0C` did not repeat the bank hash at `+0x04`.
    HashMismatch { at_4: u32, at_c: u32 },
    /// A section offset in the header is not where the contiguous layout puts it.
    BadSectionOffset { field: &'static str, found: u32, expected: u32 },
    /// No groups or no cues (no retail bank is empty; an empty bank's layout is unmeasured).
    Empty,
    /// More groups or cues than the `u16` counts hold.
    TooMany { what: &'static str, count: usize },
    /// A group or cue offset does not continue the contiguous run.
    NotContiguous { what: &'static str, index: usize, found: u32, expected: u32 },
    /// A group's form word was neither 0 nor 1.
    UnknownGroupForm { group: usize, form: u32 },
    /// A cue's form byte was neither 0 nor 1.
    UnknownCueForm { cue: usize, form: u8 },
    /// A field every retail record carries as a fixed value did not.
    FixedField { what: &'static str, index: usize, offset: usize, found: u32, expected: u32 },
    /// A cue's extent (from the offset table) does not fit its form.
    BadCueLength { cue: usize, len: usize },
    /// A multi-wave group lists more waves than its count byte holds.
    TooManyWaves { group: usize, count: usize },
    /// The body does not end at the end of the cue-offset table.
    BadLength { found: usize, expected: usize },
    /// A multi-track cue's body is outside the measured layout.
    MultiTrack { cue: usize, error: MultiTrackError },
}

impl std::fmt::Display for SoundbankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoundbankError::Truncated { field, offset, len } => {
                write!(f, "soundbank: {field} at +0x{offset:X} runs past the {len}-byte body")
            }
            SoundbankError::BadVersion(v) => write!(f, "soundbank: version 0x{v:X}, expected 0x1D"),
            SoundbankError::HashMismatch { at_4, at_c } => {
                write!(f, "soundbank: hash +0x04 0x{at_4:08X} != +0x0C 0x{at_c:08X}")
            }
            SoundbankError::BadSectionOffset { field, found, expected } => {
                write!(f, "soundbank: {field} = 0x{found:X}, the contiguous layout puts it at 0x{expected:X}")
            }
            SoundbankError::Empty => {
                write!(f, "soundbank: no groups or no cues (an empty bank's layout is unmeasured)")
            }
            SoundbankError::TooMany { what, count } => {
                write!(f, "soundbank: {count} {what} exceed the u16 count")
            }
            SoundbankError::NotContiguous { what, index, found, expected } => write!(
                f,
                "soundbank: {what} {index} at offset 0x{found:X}, the contiguous layout puts it at 0x{expected:X}"
            ),
            SoundbankError::UnknownGroupForm { group, form } => {
                write!(f, "soundbank: group {group} form word {form}, expected 0 or 1")
            }
            SoundbankError::UnknownCueForm { cue, form } => {
                write!(f, "soundbank: cue {cue} form byte {form}, expected 0 or 1")
            }
            SoundbankError::FixedField { what, index, offset, found, expected } => write!(
                f,
                "soundbank: {what} {index} +0x{offset:X} = 0x{found:X}, every retail record carries 0x{expected:X}"
            ),
            SoundbankError::BadCueLength { cue, len } => {
                write!(f, "soundbank: cue {cue} spans {len} bytes, which its form does not admit")
            }
            SoundbankError::TooManyWaves { group, count } => {
                write!(f, "soundbank: group {group} lists {count} waves, more than its u8 count holds")
            }
            SoundbankError::BadLength { found, expected } => {
                write!(f, "soundbank: body is {found} bytes, the layout implies {expected}")
            }
            SoundbankError::MultiTrack { cue, error } => write!(f, "soundbank: cue {cue}: {error}"),
        }
    }
}

impl std::error::Error for SoundbankError {}

fn rd32(b: &[u8], off: usize, field: &'static str) -> Result<u32, SoundbankError> {
    u32_at(b, off).ok_or(SoundbankError::Truncated { field, offset: off, len: b.len() })
}
fn rdf(b: &[u8], off: usize, field: &'static str) -> Result<f32, SoundbankError> {
    f32_at(b, off).ok_or(SoundbankError::Truncated { field, offset: off, len: b.len() })
}
fn rd16(b: &[u8], off: usize, field: &'static str) -> Result<u16, SoundbankError> {
    u16_at(b, off).ok_or(SoundbankError::Truncated { field, offset: off, len: b.len() })
}
fn rd8(b: &[u8], off: usize, field: &'static str) -> Result<u8, SoundbankError> {
    u8_at(b, off).ok_or(SoundbankError::Truncated { field, offset: off, len: b.len() })
}
fn fixed(
    b: &[u8],
    base: usize,
    rel: usize,
    expected: u32,
    what: &'static str,
    index: usize,
) -> Result<(), SoundbankError> {
    let found = rd32(b, base + rel, what)?;
    if found != expected {
        return Err(SoundbankError::FixedField { what, index, offset: rel, found, expected });
    }
    Ok(())
}
fn wave_ref(b: &[u8], off: usize) -> Result<WaveRef, SoundbankError> {
    Ok(WaveRef {
        wavebank: rd32(b, off, "wave bank")?,
        index: rd32(b, off + 4, "wave index")?,
        weight: rdf(b, off + 8, "wave weight")?,
    })
}

impl Soundbank {
    /// Parse a decompressed soundbank body. Refuses anything outside the measured layout.
    pub fn parse(body: &[u8]) -> Result<Soundbank, SoundbankError> {
        let version = rd32(body, 0x00, "version")?;
        if version != TABLE_VERSION {
            return Err(SoundbankError::BadVersion(version));
        }
        let bank_hash = rd32(body, 0x04, "bank hash")?;
        let group_count = rd16(body, 0x08, "group count")? as usize;
        let cue_count = rd16(body, 0x0A, "cue count")? as usize;
        let at_c = rd32(body, 0x0C, "bank hash (repeat)")?;
        if at_c != bank_hash {
            return Err(SoundbankError::HashMismatch { at_4: bank_hash, at_c });
        }
        if group_count == 0 || cue_count == 0 {
            return Err(SoundbankError::Empty);
        }
        let data_start = rd32(body, 0x10, "group section")?;
        if data_start != HEADER_SIZE as u32 {
            return Err(SoundbankError::BadSectionOffset {
                field: "+0x10 group section",
                found: data_start,
                expected: HEADER_SIZE as u32,
            });
        }
        let group_table = rd32(body, 0x14, "group table")? as usize;
        let cue_section = rd32(body, 0x18, "cue section")? as usize;
        let cue_table = rd32(body, 0x1C, "cue table")? as usize;

        let mut groups = Vec::with_capacity(group_count);
        let mut expected = 0u32;
        for i in 0..group_count {
            let rel = rd32(body, group_table + 4 * i, "group offset")?;
            if rel != expected {
                return Err(SoundbankError::NotContiguous { what: "group", index: i, found: rel, expected });
            }
            let g = HEADER_SIZE + rel as usize;
            let group = parse_group(body, g, i)?;
            expected += group.encoded_len() as u32;
            groups.push(group);
        }
        let groups_end = HEADER_SIZE as u32 + expected;
        if group_table as u32 != groups_end {
            return Err(SoundbankError::BadSectionOffset {
                field: "+0x14 group table",
                found: group_table as u32,
                expected: groups_end,
            });
        }
        let cue_section_expected = (group_table + 4 * group_count) as u32;
        if cue_section as u32 != cue_section_expected {
            return Err(SoundbankError::BadSectionOffset {
                field: "+0x18 cue section",
                found: cue_section as u32,
                expected: cue_section_expected,
            });
        }

        let cue_offsets: Vec<u32> = (0..cue_count)
            .map(|i| rd32(body, cue_table + 4 * i, "cue offset"))
            .collect::<Result<_, _>>()?;
        let mut cues = Vec::with_capacity(cue_count);
        let mut expected = 0u32;
        for i in 0..cue_count {
            if cue_offsets[i] != expected {
                return Err(SoundbankError::NotContiguous {
                    what: "cue",
                    index: i,
                    found: cue_offsets[i],
                    expected,
                });
            }
            let start = cue_section + cue_offsets[i] as usize;
            let end = match cue_offsets.get(i + 1) {
                Some(&next) => cue_section + next as usize,
                None => cue_table,
            };
            if end < start {
                return Err(SoundbankError::BadCueLength { cue: i, len: 0 });
            }
            let cue = parse_cue(body, start, end, i)?;
            expected += (end - start) as u32;
            cues.push(cue);
        }
        if cue_table as u32 != cue_section as u32 + expected {
            return Err(SoundbankError::BadSectionOffset {
                field: "+0x1C cue table",
                found: cue_table as u32,
                expected: cue_section as u32 + expected,
            });
        }
        let len = cue_table + 4 * cue_count;
        if body.len() != len {
            return Err(SoundbankError::BadLength { found: body.len(), expected: len });
        }
        Ok(Soundbank { bank_hash, groups, cues })
    }

    /// Serialize to the exact on-disk layout (see the module docs).
    pub fn to_bytes(&self) -> Result<Vec<u8>, SoundbankError> {
        if self.groups.is_empty() || self.cues.is_empty() {
            return Err(SoundbankError::Empty);
        }
        for (what, count) in [("groups", self.groups.len()), ("cues", self.cues.len())] {
            if count > u16::MAX as usize {
                return Err(SoundbankError::TooMany { what, count });
            }
        }
        let group_bytes: usize = self.groups.iter().map(Group::encoded_len).sum();
        let group_table = HEADER_SIZE + group_bytes;
        let cue_section = group_table + 4 * self.groups.len();
        let mut cue_bytes = Vec::new();
        let mut cue_offsets = Vec::with_capacity(self.cues.len());
        for (i, c) in self.cues.iter().enumerate() {
            cue_offsets.push(cue_bytes.len() as u32);
            write_cue(&mut cue_bytes, c, i)?;
        }
        let cue_table = cue_section + cue_bytes.len();

        let mut out = Vec::with_capacity(cue_table + 4 * self.cues.len());
        put_u32(&mut out, TABLE_VERSION);
        put_u32(&mut out, self.bank_hash);
        put_u16(&mut out, self.groups.len() as u16);
        put_u16(&mut out, self.cues.len() as u16);
        put_u32(&mut out, self.bank_hash);
        put_u32(&mut out, HEADER_SIZE as u32);
        put_u32(&mut out, group_table as u32);
        put_u32(&mut out, cue_section as u32);
        put_u32(&mut out, cue_table as u32);

        let mut group_offsets = Vec::with_capacity(self.groups.len());
        for (i, g) in self.groups.iter().enumerate() {
            group_offsets.push((out.len() - HEADER_SIZE) as u32);
            write_group(&mut out, g, i)?;
        }
        for off in group_offsets {
            put_u32(&mut out, off);
        }
        out.extend_from_slice(&cue_bytes);
        for off in cue_offsets {
            put_u32(&mut out, off);
        }
        Ok(out)
    }
}

fn parse_group(b: &[u8], g: usize, i: usize) -> Result<Group, SoundbankError> {
    fixed(b, g, 0x08, 0, "group", i)?;
    let head = GroupHead {
        sound_id: rd32(b, g, "group sound id")?,
        category: rd32(b, g + 0x04, "group category")?,
        unknown_10: rdf(b, g + 0x10, "group +0x10")?,
        unknown_14: rd32(b, g + 0x14, "group +0x14")?,
        min_distance: rdf(b, g + 0x18, "group min distance")?,
        max_distance: rdf(b, g + 0x1C, "group max distance")?,
        unknown_20: rdf(b, g + 0x20, "group +0x20")?,
        distance_exponent: rdf(b, g + 0x24, "group distance exponent")?,
        doppler_scale: rdf(b, g + 0x28, "group Doppler scale")?,
    };
    let form = match rd32(b, g + 0x0C, "group form")? {
        0 => GroupForm::Single {
            gain: rdf(b, g + 0x2C, "group gain")?,
            unknown_30: rdf(b, g + 0x30, "group +0x30")?,
            wave: wave_ref(b, g + 0x34)?,
        },
        1 => {
            fixed(b, g, 0x38, MULTI_GROUP_WORD_38, "group", i)?;
            fixed(b, g, 0x44, 0, "group", i)?;
            let count = rd8(b, g + 0x2D, "group wave count")? as usize;
            let mut floats_4c = [0f32; 6];
            for (k, v) in floats_4c.iter_mut().enumerate() {
                *v = rdf(b, g + 0x4C + 4 * k, "group +0x4C")?;
            }
            let waves = (0..count)
                .map(|k| wave_ref(b, g + MULTI_GROUP_HEAD + WAVE_REF_SIZE * k))
                .collect::<Result<_, _>>()?;
            GroupForm::Multi(MultiGroup {
                byte_2c: rd8(b, g + 0x2C, "group +0x2C")?,
                selection: rd8(b, g + 0x2E, "group selection")?,
                byte_2f: rd8(b, g + 0x2F, "group +0x2F")?,
                unknown_30: rdf(b, g + 0x30, "group +0x30")?,
                unknown_34: rdf(b, g + 0x34, "group +0x34")?,
                unknown_3c: rdf(b, g + 0x3C, "group +0x3C")?,
                unknown_40: rdf(b, g + 0x40, "group +0x40")?,
                word_48: rd32(b, g + 0x48, "group +0x48")?,
                floats_4c,
                unknown_64: rdf(b, g + 0x64, "group +0x64")?,
                waves,
            })
        }
        form => return Err(SoundbankError::UnknownGroupForm { group: i, form }),
    };
    Ok(Group { head, form })
}

fn write_wave_ref(out: &mut Vec<u8>, w: &WaveRef) {
    put_u32(out, w.wavebank);
    put_u32(out, w.index);
    put_f32(out, w.weight);
}

fn write_group(out: &mut Vec<u8>, g: &Group, i: usize) -> Result<(), SoundbankError> {
    let h = &g.head;
    put_u32(out, h.sound_id);
    put_u32(out, h.category);
    put_u32(out, 0);
    put_u32(out, matches!(g.form, GroupForm::Multi(_)) as u32);
    put_f32(out, h.unknown_10);
    put_u32(out, h.unknown_14);
    put_f32(out, h.min_distance);
    put_f32(out, h.max_distance);
    put_f32(out, h.unknown_20);
    put_f32(out, h.distance_exponent);
    put_f32(out, h.doppler_scale);
    match &g.form {
        GroupForm::Single { gain, unknown_30, wave } => {
            put_f32(out, *gain);
            put_f32(out, *unknown_30);
            write_wave_ref(out, wave);
        }
        GroupForm::Multi(m) => {
            let count = u8::try_from(m.waves.len())
                .map_err(|_| SoundbankError::TooManyWaves { group: i, count: m.waves.len() })?;
            out.extend_from_slice(&[m.byte_2c, count, m.selection, m.byte_2f]);
            put_f32(out, m.unknown_30);
            put_f32(out, m.unknown_34);
            put_u32(out, MULTI_GROUP_WORD_38);
            put_f32(out, m.unknown_3c);
            put_f32(out, m.unknown_40);
            put_u32(out, 0);
            put_u32(out, m.word_48);
            for v in m.floats_4c {
                put_f32(out, v);
            }
            put_f32(out, m.unknown_64);
            for w in &m.waves {
                write_wave_ref(out, w);
            }
        }
    }
    Ok(())
}

fn parse_cue(b: &[u8], start: usize, end: usize, i: usize) -> Result<Cue, SoundbankError> {
    let len = end - start;
    if len < 0x10 || !len.is_multiple_of(4) {
        return Err(SoundbankError::BadCueLength { cue: i, len });
    }
    for rel in [0x04, 0x07] {
        let v = rd8(b, start + rel, "cue flag byte")?;
        if v != 0 {
            return Err(SoundbankError::FixedField {
                what: "cue",
                index: i,
                offset: rel,
                found: v as u32,
                expected: 0,
            });
        }
    }
    let guid = rd32(b, start, "cue guid")?;
    let form = rd8(b, start + 0x05, "cue form")?;
    let byte_06 = rd8(b, start + 0x06, "cue +0x06")?;
    let gain = rdf(b, start + 0x08, "cue gain")?;
    let length_s = rdf(b, start + 0x0C, "cue length")?;
    let body = match form {
        0 => {
            if len != SINGLE_CUE_SIZE {
                return Err(SoundbankError::BadCueLength { cue: i, len });
            }
            CueBody::SingleTrack {
                soundbank: rd32(b, start + 0x10, "cue soundbank")?,
                group_index: rd16(b, start + 0x14, "cue group index")?,
                unknown_16: rd16(b, start + 0x16, "cue +0x16")?,
            }
        }
        1 => {
            let cue = b
                .get(start..end)
                .ok_or(SoundbankError::Truncated { field: "cue tracks", offset: start, len: b.len() })?;
            CueBody::MultiTrack(
                MultiTrackCue::parse(cue).map_err(|error| SoundbankError::MultiTrack { cue: i, error })?,
            )
        }
        form => return Err(SoundbankError::UnknownCueForm { cue: i, form }),
    };
    Ok(Cue { guid, byte_06, gain, length_s, body })
}

fn write_cue(out: &mut Vec<u8>, c: &Cue, i: usize) -> Result<(), SoundbankError> {
    let head_start = out.len();
    put_u32(out, c.guid);
    let form = match &c.body {
        CueBody::SingleTrack { .. } => 0,
        CueBody::MultiTrack(_) => 1,
    };
    out.extend_from_slice(&[0, form, c.byte_06, 0]);
    put_f32(out, c.gain);
    put_f32(out, c.length_s);
    match &c.body {
        CueBody::SingleTrack { soundbank, group_index, unknown_16 } => {
            put_u32(out, *soundbank);
            put_u16(out, *group_index);
            put_u16(out, *unknown_16);
        }
        CueBody::MultiTrack(m) => {
            // The body's offsets are cue-relative: hand it a buffer holding exactly this cue's head.
            let mut cue = out.split_off(head_start);
            m.write(&mut cue).map_err(|error| SoundbankError::MultiTrack { cue: i, error })?;
            out.extend_from_slice(&cue);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(sound_id: u32) -> GroupHead {
        GroupHead {
            sound_id,
            category: 0x8EC8_3583,
            unknown_10: 0.95,
            unknown_14: 0,
            min_distance: 10.0,
            max_distance: 1000.0,
            unknown_20: 1.0,
            distance_exponent: 1.0,
            doppler_scale: 1.0,
        }
    }

    fn two_form_bank() -> Soundbank {
        Soundbank {
            bank_hash: 0xDD45_73C5,
            groups: vec![
                Group {
                    head: head(0x967E_196A),
                    form: GroupForm::Single {
                        gain: 0.630_957_4,
                        unknown_30: 0.0,
                        wave: WaveRef { wavebank: 0xDD45_73C5, index: 43, weight: 1.0 },
                    },
                },
                Group {
                    head: head(0x1234_5678),
                    form: GroupForm::Multi(MultiGroup {
                        byte_2c: 0xFF,
                        selection: 1,
                        byte_2f: 1,
                        unknown_30: 0.1,
                        unknown_34: 0.0,
                        unknown_3c: 0.0,
                        unknown_40: 0.0,
                        word_48: 0x101,
                        floats_4c: [0.5, 0.5, 0.5, 0.0, -1.0, 1.0],
                        unknown_64: 0.0,
                        waves: vec![
                            WaveRef { wavebank: 0xDD45_73C5, index: 1, weight: 0.5 },
                            WaveRef { wavebank: 0xDD45_73C5, index: 2, weight: 0.5 },
                        ],
                    }),
                },
            ],
            cues: vec![
                Cue {
                    guid: 0x967E_196A,
                    byte_06: 0,
                    gain: 0.501_187_2,
                    length_s: 0.857_687,
                    body: CueBody::SingleTrack { soundbank: 0xDD45_73C5, group_index: 0, unknown_16: 0 },
                },
                Cue {
                    guid: 0x0000_0BAD,
                    byte_06: 5,
                    gain: 1.0,
                    length_s: -1.0,
                    body: CueBody::MultiTrack(crate::multitrack::MultiTrackCue {
                        byte_10: 0,
                        sound_slots: 1,
                        unknown_18: 1.0,
                        unknown_1c: -1.0,
                        unknown_20: -1.0,
                        unknown_24: 0.0,
                        events: vec![],
                        curves: vec![],
                        tracks: vec![crate::multitrack::Track {
                            byte_00: 0,
                            unknown_04: -1.0,
                            unknown_08: -1.0,
                            automation: vec![],
                            sounds: vec![crate::multitrack::Sound {
                                slot: 0,
                                byte_01: 2,
                                byte_02: 2,
                                selection: 1,
                                start_s: 0.0,
                                entries: vec![crate::multitrack::SoundEntry {
                                    soundbank: 0xDD45_73C5,
                                    group_index: 1,
                                    unknown_06: 0,
                                    weight: 1.0,
                                }],
                            }],
                        }],
                        params: vec![],
                    }),
                },
            ],
        }
    }

    #[test]
    fn both_forms_round_trip() {
        let sb = two_form_bank();
        let bytes = sb.to_bytes().expect("encodes");
        // header + 64 + (0x68 + 24) + 2 group offsets + 24 + 24 + 2 cue offsets
        assert_eq!(bytes.len(), 0x20 + 64 + 0x80 + 8 + 24 + 0x84 + 8);
        assert_eq!(Soundbank::parse(&bytes).expect("parses"), sb);
    }

    #[test]
    fn header_offsets_follow_the_measured_layout() {
        let bytes = two_form_bank().to_bytes().expect("encodes");
        let r = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(r(0), 0x1D);
        assert_eq!(r(0x10), 0x20);
        assert_eq!(r(0x14), 0x20 + 64 + 0x80, "group table follows the groups");
        assert_eq!(r(0x18), r(0x14) + 8, "cue section follows the group table");
        assert_eq!(r(0x1C), r(0x18) + 24 + 0x84, "cue table follows the cues");
        assert_eq!(r(r(0x14) as usize + 4), 64, "group offsets are relative to 0x20");
        assert_eq!(r(r(0x1C) as usize + 4), 24, "cue offsets are relative to the cue section");
        assert_eq!(bytes[0x20 + 64 + 0x2D], 2, "multi-wave count byte");
    }

    #[test]
    fn unknown_forms_and_bad_framing_are_hard_errors() {
        let bytes = two_form_bank().to_bytes().expect("encodes");
        let mut g = bytes.clone();
        g[0x20 + 0x0C] = 2;
        assert_eq!(Soundbank::parse(&g), Err(SoundbankError::UnknownGroupForm { group: 0, form: 2 }));
        let mut c = bytes.clone();
        let cue_section = u32::from_le_bytes(bytes[0x18..0x1C].try_into().unwrap()) as usize;
        c[cue_section + 5] = 7;
        assert_eq!(Soundbank::parse(&c), Err(SoundbankError::UnknownCueForm { cue: 0, form: 7 }));
        let mut t = bytes.clone();
        t.push(0);
        assert!(matches!(Soundbank::parse(&t), Err(SoundbankError::BadLength { .. })));
        let mut v = bytes;
        v[0] = 0x1C;
        assert_eq!(Soundbank::parse(&v), Err(SoundbankError::BadVersion(0x1C)));
    }
}
