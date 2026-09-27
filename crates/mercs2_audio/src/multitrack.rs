//! The multi-track cue body — the form a soundbank cue takes when its `+0x05` byte is 1 (800 of the
//! 1,321 cues across retail `vz.wad`, `English.wad` and `shell.wad`).
//!
//! Every offset below is relative to the CUE's own start, and the engine reads the structure in place
//! (no load-time fixup): `FUN_0083bf70` finds track `i` at `cue + [cue+0x30] + [cue + [cue+0x34] + 4i]`,
//! `FUN_0083fc40` walks a track's sounds, `FUN_0083fee0` fires each sound at its start time and picks
//! one of its entries, and `FUN_00840280` → `FUN_008369e0` starts a sound instance on the picked
//! entry's group (`FUN_0082e7d0`: `{soundbank, u16 group index}`).
//!
//! ```text
//! cue +0x10  u8   loop count (copied to the instance at +0x15D by FUN_00834ad0; see crate::playback)
//!     +0x11  u8   A = event record count
//!     +0x12  u8   T = track count
//!     +0x13  u8   P = parameter hash count
//!     +0x14  u8   C = cue curve record count
//!     +0x15  u8   S = sound selection-state slots (FUN_0082e370 allocates S u32s per cue, 0xFFFFFFFF)
//!     +0x16  u16  0
//!     +0x18  f32  play probability (FUN_008354e0)
//!     +0x1C  f32  loop start     +0x20 f32 loop end (FUN_00834a20 / FUN_00834a50)
//!     +0x24  f32  unknown (FUN_00834ad0 loads it into the cue's runtime timer on each start)
//!     +0x28  u32  event records   (= 0x44)       +0x2C u32 event offset table
//!     +0x30  u32  track records                  +0x34 u32 track offset table
//!     +0x38  u32  parameter hashes
//!     +0x3C  u32  cue curve records              +0x40 u32 cue curve offset table
//!     +0x44       event records, event offsets, curve records, curve offsets, tracks, track offsets,
//!                 parameter hashes — contiguous, in that order; the cue ends after the hashes
//! ```
//!
//! Each "records + offset table" pair holds its records back to back, the table giving each record's
//! offset relative to the first record.
//!
//! **Track** (offsets relative to the track):
//!
//! ```text
//! +0x00 u8  loop count (copied to the track instance at +0xD5 by FUN_0083bfc0)
//! +0x01 u8  automation record count     +0x02 u8 sound count     +0x03 u8 0
//! +0x04 f32 loop start                   +0x08 f32 loop end (FUN_0083c070)
//! +0x0C u32 automation records (= 0x1C)  +0x10 u32 automation offset table
//! +0x14 u32 sound records                +0x18 u32 sound offset table (the track ends after it)
//! ```
//!
//! **Sound** (`FUN_0083fc40`, `FUN_0083fee0`):
//!
//! ```text
//! +0x00 u8  selection-state slot (index into the cue's S slots)
//! +0x01 u8  unknown   +0x02 u8 unknown
//! +0x03 u8  selection mode: 0 sequential, 1 weighted random, 2 weighted random without an immediate
//!           repeat; any other value starts nothing
//! +0x04 u8  entry count   +0x05..+0x07 0
//! +0x08 f32 start time: the sound fires once the track's elapsed time reaches it
//! +0x0C u32 entry list offset (= 0x10)
//! +0x10     entries: { u32 soundbank hash, u16 group index, u16 unknown, f32 weight }
//! ```
//!
//! **Automation records** (the same kinds appear in the event, cue-curve and track tables; their use
//! is in the track update `FUN_0083b4a0`): a `u32` kind, then
//!
//! ```text
//! kind 0 / 1  volume / pitch ramp:   f32 start, u32 mode, u32 unknown, f32 duration, f32 from, f32 to
//! kind 2 / 3  volume / pitch LFO:    f32 start, u32 mode, u32 unknown, f32 duration, f32 period,
//!                                    f32 depth
//! kind 5 / 6  volume / pitch curve on a parameter, and kind 8 (cue curve table only):
//!             u32 unknown, u32 point count n, u32 parameter hash, u32 points offset (= 0x14),
//!             n × {f32 x, f32 y}
//! kind 4      f32 start, 6 × u8 jitter flags, u16 0, 6 × f32 jitter offsets, 2 × u32 not read,
//!             6 × f32 base multipliers: the six output-channel multipliers (FUN_0083f8e0)
//! kind 7      f32 start, u32 child cue guid (started when the track or cue finishes)
//! kind 9      f32 start, u32 curve index or 0xFFFFFFFF, u32 curve index or 0xFFFFFFFF: evaluates
//!             kind-8 curves into cue +0x84 / +0x88 (the cue filter's parameters, INFERRED)
//! ```
//!
//! Any other kind, or a record whose size does not match its kind, is a hard error.

use crate::le::{f32_at, put_f32, put_u16, put_u32, u16_at, u32_at, u8_at};

/// Size of the multi-track header, and the start of its first table.
pub const HEADER_END: usize = 0x44;
/// Size of a track header, and the start of its automation table.
pub const TRACK_HEADER: usize = 0x1C;
/// Size of a sound record before its entries, and the entry list offset every retail sound carries.
pub const SOUND_HEADER: usize = 0x10;
/// Size of one sound entry.
pub const SOUND_ENTRY: usize = 12;
/// Offset of a curve's points within the record.
pub const CURVE_POINTS: usize = 0x14;

/// What a ramp or LFO acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Kinds 0 (ramp) and 2 (LFO): multiplies the instance volume.
    Volume,
    /// Kinds 1 (ramp) and 3 (LFO): adds to the instance pitch.
    Pitch,
}

/// Which table a curve is: kind 5 (volume), 6 (pitch) or 8 (the cue curve table's kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveKind {
    /// Kind 5: multiplies volume by the curve's value at the parameter.
    Volume,
    /// Kind 6: adds the curve's value at the parameter to pitch.
    Pitch,
    /// Kind 8: the kind every cue-level curve carries; its use is not established.
    Cue,
}

impl CurveKind {
    fn code(self) -> u32 {
        match self {
            CurveKind::Volume => 5,
            CurveKind::Pitch => 6,
            CurveKind::Cue => 8,
        }
    }
}

/// One automation record.
#[derive(Clone, Debug, PartialEq)]
pub enum Automation {
    /// Kinds 0 / 1: a linear ramp from `from` to `to` over `duration_s` starting at `start_s`.
    Ramp {
        /// Volume (kind 0) or pitch (kind 1).
        target: Target,
        /// `+0x04` start time.
        start_s: f32,
        /// `+0x08` 0 = apply to the running value; non-zero = override it (FUN_0083b4a0).
        mode: u32,
        /// `+0x0C`, unknown.
        unknown_0c: u32,
        /// `+0x10` duration.
        duration_s: f32,
        /// `+0x14` start value.
        from: f32,
        /// `+0x18` end value.
        to: f32,
    },
    /// Kinds 2 / 3: a table-driven oscillation.
    Lfo {
        /// Volume (kind 2) or pitch (kind 3).
        target: Target,
        /// `+0x04` start time.
        start_s: f32,
        /// `+0x08` mode word (its low byte 0 = oscillate).
        mode: u32,
        /// `+0x0C`, unknown.
        unknown_0c: u32,
        /// `+0x10` duration.
        duration_s: f32,
        /// `+0x14` period.
        period_s: f32,
        /// `+0x18` depth.
        depth: f32,
    },
    /// Kinds 5 / 6 / 8: a piecewise curve over a game parameter.
    Curve {
        /// Which curve.
        kind: CurveKind,
        /// `+0x04`, unknown (0 in every retail curve).
        unknown_04: u32,
        /// `+0x0C` the parameter's hash (one of the cue's parameter hashes).
        param: u32,
        /// The `{x, y}` points (count at `+0x08`).
        points: Vec<(f32, f32)>,
    },
    /// Kind 4: the six output-channel multipliers (`FUN_0083f8e0`, see [`crate::automation`]).
    Kind4 {
        /// The words at `+0x04`..`+0x47`, bit-exact: `+0x04` activation time (`f32`), `+0x08`..`+0x0D`
        /// one jitter flag byte per record channel, `+0x10`..`+0x24` six jitter offsets, `+0x28` and
        /// `+0x2C` not read by the engine, `+0x30`..`+0x44` six base multipliers.
        words: [u32; 17],
    },
    /// Kind 7: the child cue a track (or, in the event table, the cue) starts when it finishes.
    Kind7 {
        /// `+0x04` activation time, as `f32` bits.
        start_bits: u32,
        /// `+0x08` the child cue's guid.
        cue: u32,
    },
    /// Kind 9: evaluates up to two of the cue's kind-8 curves into cue `+0x84` / `+0x88` (the cue's
    /// filter parameters, INFERRED; see [`crate::automation`]).
    Kind9 {
        /// `+0x04` activation time, as `f32` bits.
        start_bits: u32,
        /// `+0x08` index into the cue curve table (the engine uses its low byte), or 0xFFFFFFFF.
        curve_a: u32,
        /// `+0x0C` index into the cue curve table (the engine uses its low byte), or 0xFFFFFFFF.
        curve_b: u32,
    },
}

/// One entry a sound can pick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundEntry {
    /// The soundbank holding the group.
    pub soundbank: u32,
    /// The group's index in that soundbank.
    pub group_index: u16,
    /// `+0x06`, unknown (0 in every retail entry).
    pub unknown_06: u16,
    /// Selection weight.
    pub weight: f32,
}

/// One sound of a track.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// `+0x00` selection-state slot.
    pub slot: u8,
    /// `+0x01`, unknown.
    pub byte_01: u8,
    /// `+0x02`, unknown.
    pub byte_02: u8,
    /// `+0x03` selection mode (0 sequential, 1 weighted random, 2 weighted random, no repeat).
    pub selection: u8,
    /// `+0x08` start time.
    pub start_s: f32,
    /// The entries.
    pub entries: Vec<SoundEntry>,
}

/// One track.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// `+0x00` loop count: 0 plays once, `0xFF` loops for ever (`FUN_0083c070`).
    pub byte_00: u8,
    /// `+0x04` loop start: where the track time resumes after a loop.
    pub unknown_04: f32,
    /// `+0x08` loop end: the track time at which it loops.
    pub unknown_08: f32,
    /// The track's automation records.
    pub automation: Vec<Automation>,
    /// The track's sounds.
    pub sounds: Vec<Sound>,
}

/// A multi-track cue body (everything after the 16-byte cue head).
#[derive(Clone, Debug, PartialEq)]
pub struct MultiTrackCue {
    /// `+0x10` loop count: 0 plays once, `0xFF` loops for ever (`FUN_00835060`).
    pub byte_10: u8,
    /// `+0x15` the number of selection-state slots the cue's sounds share.
    pub sound_slots: u8,
    /// `+0x18` play probability: the cue plays only if this is not below one draw (`FUN_008354e0`).
    pub unknown_18: f32,
    /// `+0x1C` loop start: where the cue time resumes after a loop.
    pub unknown_1c: f32,
    /// `+0x20` loop end: the cue time at which it loops.
    pub unknown_20: f32,
    /// `+0x24`, unknown (loaded into the cue's runtime timer at each start).
    pub unknown_24: f32,
    /// The event table.
    pub events: Vec<Automation>,
    /// The cue curve table.
    pub curves: Vec<Automation>,
    /// The tracks.
    pub tracks: Vec<Track>,
    /// The parameter hashes.
    pub params: Vec<u32>,
}

/// Why a multi-track body did not parse or encode. The offsets are within the cue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiTrackError {
    /// A read ran past the cue.
    Truncated { field: &'static str, offset: usize },
    /// An offset is not where the contiguous layout puts it.
    Layout { field: &'static str, found: u32, expected: u32 },
    /// A field every retail cue carries as a fixed value did not.
    Fixed { field: &'static str, offset: usize, found: u32, expected: u32 },
    /// An automation record of a kind whose layout is not known.
    UnknownKind { offset: usize, kind: u32 },
    /// An automation record whose size does not match its kind.
    KindSize { offset: usize, kind: u32, size: usize },
    /// A sound's slot is not below the cue's slot count.
    SlotOutOfRange { slot: u8, slots: u8 },
    /// A count does not fit its u8 field.
    TooMany { what: &'static str, count: usize },
}

impl std::fmt::Display for MultiTrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiTrackError::Truncated { field, offset } => {
                write!(f, "multi-track cue: {field} at +0x{offset:X} runs past the cue")
            }
            MultiTrackError::Layout { field, found, expected } => write!(
                f,
                "multi-track cue: {field} = 0x{found:X}, the contiguous layout puts it at 0x{expected:X}"
            ),
            MultiTrackError::Fixed { field, offset, found, expected } => write!(
                f,
                "multi-track cue: {field} at +0x{offset:X} = 0x{found:X}, every retail cue carries 0x{expected:X}"
            ),
            MultiTrackError::UnknownKind { offset, kind } => {
                write!(f, "multi-track cue: automation kind {kind} at +0x{offset:X} has no known layout")
            }
            MultiTrackError::KindSize { offset, kind, size } => write!(
                f,
                "multi-track cue: kind-{kind} record at +0x{offset:X} is {size} bytes, which its kind does not admit"
            ),
            MultiTrackError::SlotOutOfRange { slot, slots } => {
                write!(f, "multi-track cue: sound slot {slot} is not below the cue's {slots} slots")
            }
            MultiTrackError::TooMany { what, count } => {
                write!(f, "multi-track cue: {count} {what} exceed the u8 count")
            }
        }
    }
}

impl std::error::Error for MultiTrackError {}

type R<T> = Result<T, MultiTrackError>;

fn r8(c: &[u8], o: usize, field: &'static str) -> R<u8> {
    u8_at(c, o).ok_or(MultiTrackError::Truncated { field, offset: o })
}
fn r16(c: &[u8], o: usize, field: &'static str) -> R<u16> {
    u16_at(c, o).ok_or(MultiTrackError::Truncated { field, offset: o })
}
fn r32(c: &[u8], o: usize, field: &'static str) -> R<u32> {
    u32_at(c, o).ok_or(MultiTrackError::Truncated { field, offset: o })
}
fn rf(c: &[u8], o: usize, field: &'static str) -> R<f32> {
    f32_at(c, o).ok_or(MultiTrackError::Truncated { field, offset: o })
}
fn layout(field: &'static str, found: u32, expected: usize) -> R<()> {
    if found as usize != expected {
        return Err(MultiTrackError::Layout { field, found, expected: expected as u32 });
    }
    Ok(())
}
fn fixed(field: &'static str, offset: usize, found: u32, expected: u32) -> R<()> {
    if found != expected {
        return Err(MultiTrackError::Fixed { field, offset, found, expected });
    }
    Ok(())
}

/// A "records back to back + offset table" pair: `count` records from `recs`, the table at `table`
/// (each entry relative to `recs`), the last record ending at `table`. Returns each record's
/// `(start, end)` within `c`.
fn record_spans(c: &[u8], recs: usize, table: usize, count: usize, what: &'static str) -> R<Vec<(usize, usize)>> {
    let offs: Vec<usize> =
        (0..count).map(|k| r32(c, table + 4 * k, what).map(|v| v as usize)).collect::<R<_>>()?;
    let mut spans = Vec::with_capacity(count);
    let mut expected = 0usize;
    for k in 0..count {
        layout(what, offs[k] as u32, expected)?;
        let start = recs + offs[k];
        let end = match offs.get(k + 1) {
            Some(&n) => recs + n,
            None => table,
        };
        if end < start {
            return Err(MultiTrackError::Layout { field: what, found: end as u32, expected: start as u32 });
        }
        expected = end - recs;
        spans.push((start, end));
    }
    if count == 0 {
        layout(what, table as u32, recs)?;
    }
    Ok(spans)
}

fn parse_automation(c: &[u8], s: usize, e: usize) -> R<Automation> {
    let kind = r32(c, s, "automation kind")?;
    let size = e - s;
    let words = |n: usize| -> R<()> {
        if size != 4 + 4 * n {
            return Err(MultiTrackError::KindSize { offset: s, kind, size });
        }
        Ok(())
    };
    Ok(match kind {
        0..=3 => {
            words(6)?;
            let target = if kind % 2 == 0 { Target::Volume } else { Target::Pitch };
            let (start_s, mode, unknown_0c, duration_s, a, b) = (
                rf(c, s + 4, "start")?,
                r32(c, s + 8, "mode")?,
                r32(c, s + 12, "+0x0C")?,
                rf(c, s + 16, "duration")?,
                rf(c, s + 20, "value")?,
                rf(c, s + 24, "value")?,
            );
            if kind < 2 {
                Automation::Ramp { target, start_s, mode, unknown_0c, duration_s, from: a, to: b }
            } else {
                Automation::Lfo { target, start_s, mode, unknown_0c, duration_s, period_s: a, depth: b }
            }
        }
        5 | 6 | 8 => {
            let n = r32(c, s + 8, "point count")? as usize;
            fixed("curve points offset", s + 16, r32(c, s + 16, "points offset")?, CURVE_POINTS as u32)?;
            if size != CURVE_POINTS + 8 * n {
                return Err(MultiTrackError::KindSize { offset: s, kind, size });
            }
            let points = (0..n)
                .map(|k| Ok((rf(c, s + CURVE_POINTS + 8 * k, "x")?, rf(c, s + CURVE_POINTS + 8 * k + 4, "y")?)))
                .collect::<R<_>>()?;
            Automation::Curve {
                kind: match kind {
                    5 => CurveKind::Volume,
                    6 => CurveKind::Pitch,
                    _ => CurveKind::Cue,
                },
                unknown_04: r32(c, s + 4, "+0x04")?,
                param: r32(c, s + 12, "parameter")?,
                points,
            }
        }
        4 => {
            words(17)?;
            let mut w = [0u32; 17];
            for (k, v) in w.iter_mut().enumerate() {
                *v = r32(c, s + 4 + 4 * k, "kind-4 word")?;
            }
            Automation::Kind4 { words: w }
        }
        7 => {
            words(2)?;
            Automation::Kind7 { start_bits: r32(c, s + 4, "+0x04")?, cue: r32(c, s + 8, "child cue")? }
        }
        9 => {
            words(3)?;
            Automation::Kind9 {
                start_bits: r32(c, s + 4, "+0x04")?,
                curve_a: r32(c, s + 8, "curve index")?,
                curve_b: r32(c, s + 12, "curve index")?,
            }
        }
        kind => return Err(MultiTrackError::UnknownKind { offset: s, kind }),
    })
}

fn write_automation(out: &mut Vec<u8>, a: &Automation) {
    match a {
        Automation::Ramp { target, start_s, mode, unknown_0c, duration_s, from, to } => {
            put_u32(out, if *target == Target::Volume { 0 } else { 1 });
            put_f32(out, *start_s);
            put_u32(out, *mode);
            put_u32(out, *unknown_0c);
            put_f32(out, *duration_s);
            put_f32(out, *from);
            put_f32(out, *to);
        }
        Automation::Lfo { target, start_s, mode, unknown_0c, duration_s, period_s, depth } => {
            put_u32(out, if *target == Target::Volume { 2 } else { 3 });
            put_f32(out, *start_s);
            put_u32(out, *mode);
            put_u32(out, *unknown_0c);
            put_f32(out, *duration_s);
            put_f32(out, *period_s);
            put_f32(out, *depth);
        }
        Automation::Curve { kind, unknown_04, param, points } => {
            put_u32(out, kind.code());
            put_u32(out, *unknown_04);
            put_u32(out, points.len() as u32);
            put_u32(out, *param);
            put_u32(out, CURVE_POINTS as u32);
            for (x, y) in points {
                put_f32(out, *x);
                put_f32(out, *y);
            }
        }
        Automation::Kind4 { words } => {
            put_u32(out, 4);
            for w in words {
                put_u32(out, *w);
            }
        }
        Automation::Kind7 { start_bits, cue } => {
            put_u32(out, 7);
            put_u32(out, *start_bits);
            put_u32(out, *cue);
        }
        Automation::Kind9 { start_bits, curve_a, curve_b } => {
            put_u32(out, 9);
            put_u32(out, *start_bits);
            put_u32(out, *curve_a);
            put_u32(out, *curve_b);
        }
    }
}

fn parse_sound(c: &[u8], s: usize, e: usize, slots: u8) -> R<Sound> {
    let slot = r8(c, s, "sound slot")?;
    if slot >= slots {
        return Err(MultiTrackError::SlotOutOfRange { slot, slots });
    }
    let count = r8(c, s + 4, "entry count")? as usize;
    for k in 5..8 {
        fixed("sound +0x05..+0x07", s + k, r8(c, s + k, "sound pad")? as u32, 0)?;
    }
    fixed("sound entry offset", s + 12, r32(c, s + 12, "entry offset")?, SOUND_HEADER as u32)?;
    if e - s != SOUND_HEADER + SOUND_ENTRY * count {
        return Err(MultiTrackError::Layout {
            field: "sound size",
            found: (e - s) as u32,
            expected: (SOUND_HEADER + SOUND_ENTRY * count) as u32,
        });
    }
    let entries = (0..count)
        .map(|k| {
            let o = s + SOUND_HEADER + SOUND_ENTRY * k;
            Ok(SoundEntry {
                soundbank: r32(c, o, "entry soundbank")?,
                group_index: r16(c, o + 4, "entry group")?,
                unknown_06: r16(c, o + 6, "entry +0x06")?,
                weight: rf(c, o + 8, "entry weight")?,
            })
        })
        .collect::<R<_>>()?;
    Ok(Sound {
        slot,
        byte_01: r8(c, s + 1, "sound +0x01")?,
        byte_02: r8(c, s + 2, "sound +0x02")?,
        selection: r8(c, s + 3, "sound selection")?,
        start_s: rf(c, s + 8, "sound start")?,
        entries,
    })
}

fn parse_track(c: &[u8], s: usize, e: usize, slots: u8) -> R<Track> {
    let n_auto = r8(c, s + 1, "automation count")? as usize;
    let n_sound = r8(c, s + 2, "sound count")? as usize;
    fixed("track +0x03", s + 3, r8(c, s + 3, "track +0x03")? as u32, 0)?;
    let o_auto = r32(c, s + 0x0C, "automation records")? as usize;
    let o_auto_t = r32(c, s + 0x10, "automation table")? as usize;
    let o_sound = r32(c, s + 0x14, "sound records")? as usize;
    let o_sound_t = r32(c, s + 0x18, "sound table")? as usize;
    layout("track automation records", o_auto as u32, TRACK_HEADER)?;
    layout("track sound records", o_sound as u32, o_auto_t + 4 * n_auto)?;
    layout("track end", (e - s) as u32, o_sound_t + 4 * n_sound)?;
    let automation = record_spans(c, s + o_auto, s + o_auto_t, n_auto, "track automation")?
        .into_iter()
        .map(|(a, z)| parse_automation(c, a, z))
        .collect::<R<_>>()?;
    let sounds = record_spans(c, s + o_sound, s + o_sound_t, n_sound, "track sounds")?
        .into_iter()
        .map(|(a, z)| parse_sound(c, a, z, slots))
        .collect::<R<_>>()?;
    Ok(Track {
        byte_00: r8(c, s, "track +0x00")?,
        unknown_04: rf(c, s + 4, "track +0x04")?,
        unknown_08: rf(c, s + 8, "track +0x08")?,
        automation,
        sounds,
    })
}

impl MultiTrackCue {
    /// Parse the body of multi-track cue `c` (the whole cue, head included — offsets are cue-relative).
    pub fn parse(c: &[u8]) -> Result<MultiTrackCue, MultiTrackError> {
        let n_events = r8(c, 0x11, "event count")? as usize;
        let n_tracks = r8(c, 0x12, "track count")? as usize;
        let n_params = r8(c, 0x13, "parameter count")? as usize;
        let n_curves = r8(c, 0x14, "curve count")? as usize;
        let sound_slots = r8(c, 0x15, "sound slots")?;
        fixed("+0x16", 0x16, r16(c, 0x16, "+0x16")? as u32, 0)?;
        let o: Vec<usize> = (0..7).map(|k| r32(c, 0x28 + 4 * k, "table offset").map(|v| v as usize)).collect::<R<_>>()?;
        let (ev, ev_t, tr, tr_t, par, cv, cv_t) = (o[0], o[1], o[2], o[3], o[4], o[5], o[6]);
        layout("+0x28 event records", ev as u32, HEADER_END)?;
        layout("+0x3C curve records", cv as u32, ev_t + 4 * n_events)?;
        layout("+0x30 track records", tr as u32, cv_t + 4 * n_curves)?;
        layout("+0x38 parameters", par as u32, tr_t + 4 * n_tracks)?;
        layout("cue end", c.len() as u32, par + 4 * n_params)?;
        let events = record_spans(c, ev, ev_t, n_events, "event table")?
            .into_iter()
            .map(|(a, z)| parse_automation(c, a, z))
            .collect::<R<_>>()?;
        let curves = record_spans(c, cv, cv_t, n_curves, "curve table")?
            .into_iter()
            .map(|(a, z)| parse_automation(c, a, z))
            .collect::<R<_>>()?;
        let tracks = record_spans(c, tr, tr_t, n_tracks, "track table")?
            .into_iter()
            .map(|(a, z)| parse_track(c, a, z, sound_slots))
            .collect::<R<_>>()?;
        let params = (0..n_params).map(|k| r32(c, par + 4 * k, "parameter")).collect::<R<_>>()?;
        Ok(MultiTrackCue {
            byte_10: r8(c, 0x10, "+0x10")?,
            sound_slots,
            unknown_18: rf(c, 0x18, "+0x18")?,
            unknown_1c: rf(c, 0x1C, "+0x1C")?,
            unknown_20: rf(c, 0x20, "+0x20")?,
            unknown_24: rf(c, 0x24, "+0x24")?,
            events,
            curves,
            tracks,
            params,
        })
    }

    /// Append the body (cue `+0x10` onward) to `out`, which holds exactly the 16-byte cue head.
    pub fn write(&self, out: &mut Vec<u8>) -> Result<(), MultiTrackError> {
        let cue_start = out.len() - 0x10;
        let count = |what: &'static str, n: usize| -> R<u8> {
            u8::try_from(n).map_err(|_| MultiTrackError::TooMany { what, count: n })
        };
        for t in &self.tracks {
            for s in &t.sounds {
                if s.slot >= self.sound_slots {
                    return Err(MultiTrackError::SlotOutOfRange { slot: s.slot, slots: self.sound_slots });
                }
                count("sound entries", s.entries.len())?;
            }
            count("track automation records", t.automation.len())?;
            count("track sounds", t.sounds.len())?;
        }
        let mut events = Vec::new();
        let ev_offs = pack(&mut events, &self.events, write_automation);
        let mut curves = Vec::new();
        let cv_offs = pack(&mut curves, &self.curves, write_automation);
        let mut tracks = Vec::new();
        let tr_offs = pack(&mut tracks, &self.tracks, write_track);
        let ev = HEADER_END;
        let ev_t = ev + events.len();
        let cv = ev_t + 4 * self.events.len();
        let cv_t = cv + curves.len();
        let tr = cv_t + 4 * self.curves.len();
        let tr_t = tr + tracks.len();
        let par = tr_t + 4 * self.tracks.len();

        out.extend_from_slice(&[
            self.byte_10,
            count("events", self.events.len())?,
            count("tracks", self.tracks.len())?,
            count("parameters", self.params.len())?,
            count("curves", self.curves.len())?,
            self.sound_slots,
        ]);
        put_u16(out, 0);
        put_f32(out, self.unknown_18);
        put_f32(out, self.unknown_1c);
        put_f32(out, self.unknown_20);
        put_f32(out, self.unknown_24);
        for v in [ev, ev_t, tr, tr_t, par, cv, cv_t] {
            put_u32(out, v as u32);
        }
        debug_assert_eq!(out.len() - cue_start, HEADER_END);
        out.extend_from_slice(&events);
        ev_offs.iter().for_each(|&v| put_u32(out, v));
        out.extend_from_slice(&curves);
        cv_offs.iter().for_each(|&v| put_u32(out, v));
        out.extend_from_slice(&tracks);
        tr_offs.iter().for_each(|&v| put_u32(out, v));
        self.params.iter().for_each(|&v| put_u32(out, v));
        Ok(())
    }
}

/// Write `items` back to back into `buf`, returning each one's offset from the start of `buf`.
fn pack<T>(buf: &mut Vec<u8>, items: &[T], mut write: impl FnMut(&mut Vec<u8>, &T)) -> Vec<u32> {
    items
        .iter()
        .map(|it| {
            let off = buf.len() as u32;
            write(buf, it);
            off
        })
        .collect()
}

fn write_track(out: &mut Vec<u8>, t: &Track) {
    let start = out.len();
    let mut autos = Vec::new();
    let auto_offs = pack(&mut autos, &t.automation, write_automation);
    let mut sounds = Vec::new();
    let sound_offs = pack(&mut sounds, &t.sounds, write_sound);
    let o_auto_t = TRACK_HEADER + autos.len();
    let o_sound = o_auto_t + 4 * t.automation.len();
    let o_sound_t = o_sound + sounds.len();
    out.extend_from_slice(&[t.byte_00, t.automation.len() as u8, t.sounds.len() as u8, 0]);
    put_f32(out, t.unknown_04);
    put_f32(out, t.unknown_08);
    for v in [TRACK_HEADER, o_auto_t, o_sound, o_sound_t] {
        put_u32(out, v as u32);
    }
    debug_assert_eq!(out.len() - start, TRACK_HEADER);
    out.extend_from_slice(&autos);
    auto_offs.iter().for_each(|&v| put_u32(out, v));
    out.extend_from_slice(&sounds);
    sound_offs.iter().for_each(|&v| put_u32(out, v));
}

fn write_sound(out: &mut Vec<u8>, s: &Sound) {
    out.extend_from_slice(&[s.slot, s.byte_01, s.byte_02, s.selection, s.entries.len() as u8, 0, 0, 0]);
    put_f32(out, s.start_s);
    put_u32(out, SOUND_HEADER as u32);
    for e in &s.entries {
        put_u32(out, e.soundbank);
        put_u16(out, e.group_index);
        put_u16(out, e.unknown_06);
        put_f32(out, e.weight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MultiTrackCue {
        MultiTrackCue {
            byte_10: 0,
            sound_slots: 2,
            unknown_18: 1.0,
            unknown_1c: -1.0,
            unknown_20: -1.0,
            unknown_24: 0.5,
            events: vec![Automation::Ramp {
                target: Target::Volume,
                start_s: 10.0,
                mode: 0,
                unknown_0c: 0,
                duration_s: 20.0,
                from: 1.0,
                to: 0.0,
            }],
            curves: vec![Automation::Curve {
                kind: CurveKind::Cue,
                unknown_04: 0,
                param: 0xCBE8_ED58,
                points: vec![(0.0, 0.5), (1.0, 1.0)],
            }],
            tracks: vec![
                Track {
                    byte_00: 0,
                    unknown_04: -1.0,
                    unknown_08: -1.0,
                    automation: vec![],
                    sounds: vec![Sound {
                        slot: 0,
                        byte_01: 2,
                        byte_02: 2,
                        selection: 1,
                        start_s: 0.0,
                        entries: vec![SoundEntry { soundbank: 0x07F4_236F, group_index: 12, unknown_06: 0, weight: 1.0 }],
                    }],
                },
                Track {
                    byte_00: 0xFF,
                    unknown_04: 0.0,
                    unknown_08: 1.0,
                    automation: vec![
                        Automation::Lfo {
                            target: Target::Pitch,
                            start_s: 0.0,
                            mode: 0,
                            unknown_0c: 0,
                            duration_s: 1000.0,
                            period_s: 2.0,
                            depth: 0.1,
                        },
                        Automation::Kind9 { start_bits: 0, curve_a: 0, curve_b: 0xFFFF_FFFF },
                    ],
                    sounds: vec![Sound {
                        slot: 1,
                        byte_01: 2,
                        byte_02: 2,
                        selection: 2,
                        start_s: 0.25,
                        entries: vec![
                            SoundEntry { soundbank: 0x07F4_236F, group_index: 1, unknown_06: 0, weight: 0.5 },
                            SoundEntry { soundbank: 0x07F4_236F, group_index: 2, unknown_06: 0, weight: 0.5 },
                        ],
                    }],
                },
            ],
            params: vec![0xCBE8_ED58, 0x15BA_509E],
        }
    }

    fn encode(m: &MultiTrackCue) -> Vec<u8> {
        let mut out = vec![0u8; 0x10];
        m.write(&mut out).expect("writes");
        out
    }

    #[test]
    fn round_trips_every_table() {
        let m = sample();
        let bytes = encode(&m);
        assert_eq!(MultiTrackCue::parse(&bytes).expect("parses"), m);
    }

    #[test]
    fn offsets_follow_the_measured_order() {
        let bytes = encode(&sample());
        let r = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
        assert_eq!(r(0x28), 0x44);
        assert_eq!(r(0x3C), r(0x2C) + 4, "curves follow the event offset table");
        assert_eq!(r(0x30), r(0x40) + 4, "tracks follow the curve offset table");
        assert_eq!(r(0x38), r(0x34) + 8, "parameters follow the track offset table");
        assert_eq!(bytes.len(), r(0x38) + 8);
        assert_eq!(&bytes[0x10..0x16], &[0, 1, 2, 2, 1, 2]);
    }

    #[test]
    fn unknown_kinds_and_bad_slots_are_hard_errors() {
        let mut m = sample();
        m.tracks[1].sounds[0].slot = 2;
        let mut out = vec![0u8; 0x10];
        assert_eq!(m.write(&mut out), Err(MultiTrackError::SlotOutOfRange { slot: 2, slots: 2 }));

        let mut bytes = encode(&sample());
        bytes[0x44] = 10; // the event record's kind
        assert!(matches!(MultiTrackCue::parse(&bytes), Err(MultiTrackError::UnknownKind { kind: 10, .. })));
    }
}
