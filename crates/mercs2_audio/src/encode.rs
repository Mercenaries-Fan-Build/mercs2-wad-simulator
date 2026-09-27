//! Sound-bank encoder: build a bank's `wavebank`, `soundbank` and `sounddb` tables from a list of
//! cues, each a named PCM16 clip plus explicit group and cue parameters.
//!
//! Every cue becomes one embedded wave, one single-wave group and one single-track cue, all at the
//! cue's index, plus one sounddb entry routing `m2(name)` to that cue — the shape of the retail
//! single-track cues (`ui_PDA_Open_01_st` in `ui_hud` is one). The result is the three table bodies;
//! retail ships them as three entries of ONE block, all under the bank's name hash (soundbank type 21,
//! sounddb 13, wavebank 6), each wrapped exactly as `mercs2_formats::ucfx::build_wrapped_block` wraps
//! a payload.
//!
//! Fields whose meaning is not established are never filled in here: they are fields of
//! [`GroupParams`] / [`CueParams`] (and the ids of [`CueSpec`]) the caller passes. The
//! [`UI_PDA_OPEN_GROUP`] / [`UI_PDA_OPEN_CUE`] presets carry the retail values of one real UI cue for
//! callers that want a UI sound configured like the game's own.
//!
//! The derived fields are computed as retail carries them, and are checked against retail by
//! `tests/retail_banks.rs`: the cue length by [`crate::duration`] (bit-exact on 14,818 of the 14,834
//! retail cues), the sounddb sorted by guid, blobs 16-aligned. Every multi-track entry must name a
//! group of this bank, since the length needs the waves it can play.

use mercs2_formats::hash::pandemic_hash_m2;

use crate::duration::{self, DurationError};
use crate::multitrack::MultiTrackCue;
use crate::soundbank::{
    Cue, CueBody, Group, GroupForm, GroupHead, MultiGroup, Soundbank, SoundbankError, WaveRef,
};
use crate::sounddb::{CategoryEntry, CueEntry, SoundDb, SoundDbError, SOUNDDB_TAG};
use crate::wave::{WaveData, WaveError, WaveRecord, WavebankFile, BYTES_PER_SAMPLE_PCM16};

/// The category tree of the retail `Mercs2Globals` sounddb, verbatim (asserted against `vz.wad` by
/// `tests/retail_banks.rs`). A group's category must be one of these. Names are given where the hash
/// has been matched to a string; the rest are uncracked.
pub const RETAIL_CATEGORIES: [CategoryEntry; 19] = [
    CategoryEntry { category: 0x1693_0AFE, parent: 0xDB32_F53E }, // explosion → Non_Action_Hijack
    CategoryEntry { category: 0x2B8D_1FDF, parent: 0xDB32_F53E }, // vehicle → Non_Action_Hijack
    CategoryEntry { category: 0x4111_ECDA, parent: 0x6413_FB86 }, // music → (root)
    CategoryEntry { category: 0x531B_C4BF, parent: 0xDB32_F53E }, // collision → Non_Action_Hijack
    CategoryEntry { category: 0x6413_FB86, parent: 0x0000_0000 }, // (root, uncracked)
    CategoryEntry { category: 0x6C0A_B8B2, parent: 0xDB32_F53E }, // foley → Non_Action_Hijack
    CategoryEntry { category: 0x7674_95E2, parent: 0x6413_FB86 }, // sfx → (root)
    CategoryEntry { category: 0x7871_F925, parent: 0xDB32_F53E }, // ambience → Non_Action_Hijack
    CategoryEntry { category: 0x787C_0871, parent: 0xDB32_F53E }, // weapon → Non_Action_Hijack
    CategoryEntry { category: 0x8884_56AD, parent: 0xDB32_F53E }, // (uncracked)
    CategoryEntry { category: 0x8EC8_3583, parent: 0x7674_95E2 }, // ui → sfx
    CategoryEntry { category: 0x9FE0_DCAD, parent: 0x4111_ECDA }, // (uncracked) → music
    CategoryEntry { category: 0xA200_7430, parent: 0xDB32_F53E }, // (uncracked)
    CategoryEntry { category: 0xB91F_07F6, parent: 0x4111_ECDA }, // source → music
    CategoryEntry { category: 0xD221_DBE8, parent: 0x6413_FB86 }, // vo → (root)
    CategoryEntry { category: 0xD40A_D42A, parent: 0xEC8F_FB27 }, // (uncracked) → non_ui
    CategoryEntry { category: 0xDB32_F53E, parent: 0xEC8F_FB27 }, // Non_Action_Hijack → non_ui
    CategoryEntry { category: 0xEC8F_FB27, parent: 0x7674_95E2 }, // non_ui → sfx
    CategoryEntry { category: 0xFA0B_8DBC, parent: 0xD221_DBE8 }, // chatter → vo
];

/// Interleaved little-endian PCM16 audio.
#[derive(Clone, Debug, PartialEq)]
pub struct Pcm16 {
    /// 1 (mono) or 2 (stereo).
    pub channels: u8,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Interleaved samples; the length is a whole number of frames.
    pub samples: Vec<i16>,
}

/// The single-wave group fields the caller supplies (see [`crate::soundbank`] for the offsets and
/// which names are inferred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupParams {
    /// `+0x10`, unknown.
    pub unknown_10: f32,
    /// `+0x14`, 0 or 1 in retail, unknown.
    pub unknown_14: u32,
    /// `+0x18` minimum distance.
    pub min_distance: f32,
    /// `+0x1C` maximum distance.
    pub max_distance: f32,
    /// `+0x20`, unknown.
    pub unknown_20: f32,
    /// `+0x24` distance fall-off exponent.
    pub distance_exponent: f32,
    /// `+0x28` Doppler scale.
    pub doppler_scale: f32,
    /// `+0x2C` linear gain.
    pub gain: f32,
    /// `+0x30`, unknown.
    pub unknown_30: f32,
    /// The wave reference's weight (1.0 in every retail single-wave group).
    pub wave_weight: f32,
}

/// The single-track cue fields the caller supplies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueParams {
    /// `+0x06` start limit: the engine starts the cue only while a counter in its runtime record is
    /// below this (0 = no limit; that the counter counts live instances is inferred).
    pub byte_06: u8,
    /// `+0x08` gain.
    pub gain: f32,
    /// `+0x16`, unknown (0 in most retail cues).
    pub unknown_16: u16,
}

/// Group 70 of retail `ui_hud` — the group cue `ui_PDA_Open_01_st` plays — as read from `vz.wad`
/// (asserted by `tests/retail_banks.rs`). Values are the exact f32 bit patterns.
pub const UI_PDA_OPEN_GROUP: GroupParams = GroupParams {
    unknown_10: f32::from_bits(0x3F73_3333), // 0.95
    unknown_14: 0,
    min_distance: f32::from_bits(0x4120_0000), // 10.0
    max_distance: f32::from_bits(0x447A_0000), // 1000.0
    unknown_20: f32::from_bits(0x3F80_0000),   // 1.0
    distance_exponent: f32::from_bits(0x3F80_0000), // 1.0
    doppler_scale: f32::from_bits(0x3F80_0000),     // 1.0
    gain: f32::from_bits(0x3F21_866C),         // 0.630957 (-4 dB)
    unknown_30: f32::from_bits(0x0000_0000),   // 0.0
    wave_weight: f32::from_bits(0x3F80_0000),  // 1.0
};

/// The cue-level fields of retail `ui_PDA_Open_01_st` (cue 57 of `ui_hud`).
pub const UI_PDA_OPEN_CUE: CueParams = CueParams {
    byte_06: 0,
    gain: f32::from_bits(0x3F00_4DCE), // 0.501187 (-6 dB)
    unknown_16: 0,
};

/// One cue to encode.
#[derive(Clone, Debug, PartialEq)]
pub struct CueSpec {
    /// Cue name; its guid is `m2(name)`, which is what `Sound.CueSound(name)` looks up.
    pub name: String,
    /// Category name; must hash to one of [`RETAIL_CATEGORIES`].
    pub category: String,
    /// The group's `+0x00` sound id (meaning unproven; `m2(name)` in `ui_PDA_Open_01_st`).
    pub sound_id: u32,
    /// The wave record's clip hash (`m2(name)` in `ui_PDA_Open_01_st`).
    pub clip_hash: u32,
    /// The audio.
    pub pcm: Pcm16,
    /// Group fields.
    pub group: GroupParams,
    /// Cue fields.
    pub cue: CueParams,
}

/// A bank to encode.
#[derive(Clone, Debug, PartialEq)]
pub struct BankSpec {
    /// Bank name; every table carries `m2(name)`.
    pub name: String,
    /// The cues, in the order they take in the soundbank and wavebank.
    pub cues: Vec<CueSpec>,
}

/// The three tables as structures, before serialization.
#[derive(Clone, Debug, PartialEq)]
pub struct BankTables {
    /// The wavebank.
    pub wavebank: WavebankFile,
    /// The soundbank.
    pub soundbank: Soundbank,
    /// The per-bank sounddb.
    pub sounddb: SoundDb,
}

/// The three serialized table bodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedBank {
    /// `m2(bank name)` — the name hash all three tables carry, and the block entries' name hash.
    pub bank_hash: u32,
    /// `wavebank` body (type `0xF753F6D0`, ASET type 6).
    pub wavebank: Vec<u8>,
    /// `soundbank` body (type `0x9F8BCA10`, ASET type 21).
    pub soundbank: Vec<u8>,
    /// `sounddb` body (type `0xE5273C14`, ASET type 13).
    pub sounddb: Vec<u8>,
}

/// Why a bank could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// The bank or a cue has an empty name.
    EmptyName,
    /// The bank has no cues.
    NoCues,
    /// The bank has no waves or no groups.
    NoWavesOrGroups,
    /// Two cues hash to the same guid.
    DuplicateCue { name: String, guid: u32 },
    /// A category name does not hash to a retail category.
    UnknownCategory { cue: String, category: String, hash: u32 },
    /// A cue's audio is not 1 or 2 channels.
    UnsupportedChannels { cue: String, channels: u8 },
    /// A cue's sample count is not a whole number of frames.
    PartialFrame { cue: String, samples: usize, channels: u8 },
    /// A cue has no audio.
    EmptyAudio { cue: String },
    /// A cue's sample rate is zero.
    ZeroRate { cue: String },
    /// A group names a wave index past this bank's waves.
    WaveOutOfRange { group: usize, wave: usize, waves: usize },
    /// A cue names a group index past this bank's groups.
    GroupOutOfRange { cue: String, group: usize, groups: usize },
    /// A multi-wave group lists no waves, or a multi-track sound lists no entries.
    EmptyChoice { what: String },
    /// A selection mode the engine picks nothing with (it knows 0, 1 and 2).
    SelectionMode { what: String, mode: u8 },
    /// A count does not fit its on-disk field.
    TooLarge { what: &'static str, value: usize },
    /// The wavebank serializer refused the table.
    Wavebank(WaveError),
    /// The soundbank serializer refused the table.
    Soundbank(SoundbankError),
    /// The sounddb serializer refused the table.
    SoundDb(SoundDbError),
    /// A cue's length could not be computed (e.g. it plays another bank's group).
    Length { cue: String, error: DurationError },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::EmptyName => write!(f, "encode: a bank or cue name is empty"),
            EncodeError::NoCues => write!(f, "encode: the bank has no cues"),
            EncodeError::NoWavesOrGroups => write!(f, "encode: the bank has no waves or no groups"),
            EncodeError::DuplicateCue { name, guid } => {
                write!(f, "encode: cue {name:?} hashes to 0x{guid:08X}, which another cue already has")
            }
            EncodeError::UnknownCategory { cue, category, hash } => write!(
                f,
                "encode: {cue} category {category:?} (0x{hash:08X}) is not a retail category"
            ),
            EncodeError::UnsupportedChannels { cue, channels } => {
                write!(f, "encode: {cue} has {channels} channels, expected 1 or 2")
            }
            EncodeError::PartialFrame { cue, samples, channels } => {
                write!(f, "encode: {cue} has {samples} samples, not a multiple of {channels}")
            }
            EncodeError::EmptyAudio { cue } => write!(f, "encode: {cue} has no samples"),
            EncodeError::ZeroRate { cue } => write!(f, "encode: {cue} has sample rate 0"),
            EncodeError::WaveOutOfRange { group, wave, waves } => {
                write!(f, "encode: group {group} names wave {wave}, past this bank's {waves} waves")
            }
            EncodeError::GroupOutOfRange { cue, group, groups } => {
                write!(f, "encode: cue {cue:?} names group {group}, past this bank's {groups} groups")
            }
            EncodeError::EmptyChoice { what } => write!(f, "encode: {what} lists nothing to pick"),
            EncodeError::SelectionMode { what, mode } => {
                write!(f, "encode: {what} has selection mode {mode}; the engine knows 0, 1 and 2")
            }
            EncodeError::TooLarge { what, value } => {
                write!(f, "encode: {what} = {value} does not fit its on-disk field")
            }
            EncodeError::Wavebank(e) => write!(f, "encode: {e}"),
            EncodeError::Soundbank(e) => write!(f, "encode: {e}"),
            EncodeError::SoundDb(e) => write!(f, "encode: {e}"),
            EncodeError::Length { cue, error } => write!(f, "encode: cue {cue:?}: {error}"),
        }
    }
}

impl std::error::Error for EncodeError {}


// ---- the general path: waves, groups and cues authored separately -----------------------------

/// One wave of a bank.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveSpec {
    /// The record's clip hash.
    pub clip_hash: u32,
    /// The audio.
    pub pcm: Pcm16,
}

/// The fields both group forms share (see [`crate::soundbank::GroupHead`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupHeadParams {
    /// `+0x10`, unknown.
    pub unknown_10: f32,
    /// `+0x14`, 0 or 1 in retail, unknown.
    pub unknown_14: u32,
    /// `+0x18` minimum distance.
    pub min_distance: f32,
    /// `+0x1C` maximum distance.
    pub max_distance: f32,
    /// `+0x20`, unknown.
    pub unknown_20: f32,
    /// `+0x24` distance fall-off exponent.
    pub distance_exponent: f32,
    /// `+0x28` Doppler scale.
    pub doppler_scale: f32,
}

/// A multi-wave group's fields after the head (see [`crate::soundbank::MultiGroup`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MultiGroupParams {
    /// `+0x2C` the wave loop count (0 plays once; a cue that can reach a non-zero count is refused
    /// by [`crate::AudioEngine::cue_sound`]).
    pub byte_2c: u8,
    /// `+0x2E` selection mode: 0 sequential, 1 weighted random, 2 weighted random without an
    /// immediate repeat.
    pub selection: u8,
    /// `+0x2F`, unknown.
    pub byte_2f: u8,
    /// `+0x30`, unknown.
    pub unknown_30: f32,
    /// `+0x34`, unknown.
    pub unknown_34: f32,
    /// `+0x3C` start delay `a`: the delay is drawn in `[max(a − b, 0), b + a]`.
    pub unknown_3c: f32,
    /// `+0x40` start delay `b`.
    pub unknown_40: f32,
    /// `+0x48`, unknown flags.
    pub word_48: u32,
    /// `+0x4C`..`+0x63`: `[1]`/`[2]` the base volume range, `[4]`/`[5]` the base pitch range
    /// (semitones), `[0]` and `[3]` unknown.
    pub floats_4c: [f32; 6],
    /// `+0x64`, unknown.
    pub unknown_64: f32,
}

/// A group's form.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupFormSpec {
    /// One wave.
    Single {
        /// Index into [`TablesSpec::waves`].
        wave: usize,
        /// `+0x2C` the sound instance's base volume.
        gain: f32,
        /// `+0x30` the sound instance's base pitch, semitones.
        unknown_30: f32,
        /// The wave reference's weight.
        weight: f32,
    },
    /// Weighted waves the engine picks among ([`crate::select`]).
    Multi {
        /// The multi-wave fields.
        params: MultiGroupParams,
        /// `(index into [`TablesSpec::waves`], weight)`.
        waves: Vec<(usize, f32)>,
    },
}

/// One group of a bank.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupSpec {
    /// `+0x00` sound id (meaning unproven).
    pub sound_id: u32,
    /// Category name; must hash to one of [`RETAIL_CATEGORIES`].
    pub category: String,
    /// The shared head fields.
    pub head: GroupHeadParams,
    /// The form.
    pub form: GroupFormSpec,
}

/// A cue's body.
#[derive(Clone, Debug, PartialEq)]
pub enum CueBodySpec {
    /// Plays one group of this bank.
    SingleTrack {
        /// Index into [`TablesSpec::groups`].
        group: usize,
        /// `+0x16`, unknown (0 in most retail cues).
        unknown_16: u16,
    },
    /// Tracks of timed sounds. Every entry must name one of this bank's groups (`m2(name)`): the
    /// cue's length is computed from the waves they play.
    MultiTrack(MultiTrackCue),
}

/// One cue of a bank.
#[derive(Clone, Debug, PartialEq)]
pub struct CueDef {
    /// Cue name; its guid is `m2(name)`.
    pub name: String,
    /// `+0x06` start limit: the engine starts the cue only while a counter in its runtime record is
    /// below this (0 = no limit; that the counter counts live instances is inferred).
    pub byte_06: u8,
    /// `+0x08` gain.
    pub gain: f32,
    /// The body.
    pub body: CueBodySpec,
}

/// A whole bank, authored table by table.
#[derive(Clone, Debug, PartialEq)]
pub struct TablesSpec {
    /// Bank name; every table carries `m2(name)`.
    pub name: String,
    /// The waves, in wavebank order.
    pub waves: Vec<WaveSpec>,
    /// The groups, in soundbank order.
    pub groups: Vec<GroupSpec>,
    /// The cues, in soundbank order.
    pub cues: Vec<CueDef>,
}

fn check_pcm(what: &str, pcm: &Pcm16) -> Result<u32, EncodeError> {
    let ch = pcm.channels;
    if ch != 1 && ch != 2 {
        return Err(EncodeError::UnsupportedChannels { cue: what.to_string(), channels: ch });
    }
    if pcm.samples.is_empty() {
        return Err(EncodeError::EmptyAudio { cue: what.to_string() });
    }
    if !pcm.samples.len().is_multiple_of(ch as usize) {
        return Err(EncodeError::PartialFrame { cue: what.to_string(), samples: pcm.samples.len(), channels: ch });
    }
    if pcm.sample_rate == 0 {
        return Err(EncodeError::ZeroRate { cue: what.to_string() });
    }
    let frames = pcm.samples.len() / ch as usize;
    let bytes = pcm.samples.len() * 2;
    if u32::try_from(bytes).is_err() {
        return Err(EncodeError::TooLarge { what: "clip bytes", value: bytes });
    }
    u32::try_from(frames).map_err(|_| EncodeError::TooLarge { what: "frame count", value: frames })
}

/// Build the three tables for a bank authored table by table (validated; nothing serialized yet).
pub fn build_general(spec: &TablesSpec) -> Result<BankTables, EncodeError> {
    if spec.name.is_empty() {
        return Err(EncodeError::EmptyName);
    }
    if spec.cues.is_empty() {
        return Err(EncodeError::NoCues);
    }
    if spec.waves.is_empty() || spec.groups.is_empty() {
        return Err(EncodeError::NoWavesOrGroups);
    }
    for (what, n) in [("wave count", spec.waves.len()), ("group count", spec.groups.len()), ("cue count", spec.cues.len())] {
        if n > u16::MAX as usize {
            return Err(EncodeError::TooLarge { what, value: n });
        }
    }
    let bank_hash = pandemic_hash_m2(&spec.name);

    let mut records = Vec::with_capacity(spec.waves.len());
    for (i, w) in spec.waves.iter().enumerate() {
        let frames = check_pcm(&format!("wave {i}"), &w.pcm)?;
        records.push(WaveRecord {
            clip_hash: w.clip_hash,
            channels: w.pcm.channels,
            format: BYTES_PER_SAMPLE_PCM16,
            sample_rate: w.pcm.sample_rate,
            frames,
            data: WaveData::Embedded(w.pcm.samples.iter().flat_map(|s| s.to_le_bytes()).collect()),
        });
    }

    let wave_ref = |g: usize, wave: usize, weight: f32| -> Result<WaveRef, EncodeError> {
        if wave >= spec.waves.len() {
            return Err(EncodeError::WaveOutOfRange { group: g, wave, waves: spec.waves.len() });
        }
        Ok(WaveRef { wavebank: bank_hash, index: wave as u32, weight })
    };
    let mut groups = Vec::with_capacity(spec.groups.len());
    for (g, gs) in spec.groups.iter().enumerate() {
        let category = pandemic_hash_m2(&gs.category);
        if !RETAIL_CATEGORIES.iter().any(|e| e.category == category) {
            return Err(EncodeError::UnknownCategory {
                cue: format!("group {g}"),
                category: gs.category.clone(),
                hash: category,
            });
        }
        let h = &gs.head;
        let form = match &gs.form {
            GroupFormSpec::Single { wave, gain, unknown_30, weight } => GroupForm::Single {
                gain: *gain,
                unknown_30: *unknown_30,
                wave: wave_ref(g, *wave, *weight)?,
            },
            GroupFormSpec::Multi { params: p, waves } => {
                if waves.is_empty() {
                    return Err(EncodeError::EmptyChoice { what: format!("group {g}") });
                }
                if p.selection > 2 {
                    return Err(EncodeError::SelectionMode { what: format!("group {g}"), mode: p.selection });
                }
                GroupForm::Multi(MultiGroup {
                    byte_2c: p.byte_2c,
                    selection: p.selection,
                    byte_2f: p.byte_2f,
                    unknown_30: p.unknown_30,
                    unknown_34: p.unknown_34,
                    unknown_3c: p.unknown_3c,
                    unknown_40: p.unknown_40,
                    word_48: p.word_48,
                    floats_4c: p.floats_4c,
                    unknown_64: p.unknown_64,
                    waves: waves.iter().map(|&(w, wt)| wave_ref(g, w, wt)).collect::<Result<_, _>>()?,
                })
            }
        };
        groups.push(Group {
            head: GroupHead {
                sound_id: gs.sound_id,
                category,
                unknown_10: h.unknown_10,
                unknown_14: h.unknown_14,
                min_distance: h.min_distance,
                max_distance: h.max_distance,
                unknown_20: h.unknown_20,
                distance_exponent: h.distance_exponent,
                doppler_scale: h.doppler_scale,
            },
            form,
        });
    }

    let mut cues = Vec::with_capacity(spec.cues.len());
    let mut entries: Vec<CueEntry> = Vec::with_capacity(spec.cues.len());
    for (i, c) in spec.cues.iter().enumerate() {
        if c.name.is_empty() {
            return Err(EncodeError::EmptyName);
        }
        let guid = pandemic_hash_m2(&c.name);
        if entries.iter().any(|e| e.guid == guid) {
            return Err(EncodeError::DuplicateCue { name: c.name.clone(), guid });
        }
        let group_in_range = |group: usize| -> Result<u16, EncodeError> {
            if group >= spec.groups.len() {
                return Err(EncodeError::GroupOutOfRange { cue: c.name.clone(), group, groups: spec.groups.len() });
            }
            Ok(group as u16)
        };
        let body = match &c.body {
            CueBodySpec::SingleTrack { group, unknown_16 } => CueBody::SingleTrack {
                soundbank: bank_hash,
                group_index: group_in_range(*group)?,
                unknown_16: *unknown_16,
            },
            CueBodySpec::MultiTrack(m) => {
                for (t, track) in m.tracks.iter().enumerate() {
                    for (k, s) in track.sounds.iter().enumerate() {
                        let what = format!("cue {:?} track {t} sound {k}", c.name);
                        if s.entries.is_empty() {
                            return Err(EncodeError::EmptyChoice { what });
                        }
                        if s.selection > 2 {
                            return Err(EncodeError::SelectionMode { what, mode: s.selection });
                        }
                        for e in &s.entries {
                            group_in_range(e.group_index as usize)?;
                        }
                    }
                }
                CueBody::MultiTrack(m.clone())
            }
        };
        // The `+0x0C` length, computed as the retail banks carry it (`crate::duration`).
        let length_s = duration::cue_length_s(
            &body,
            |sb, g| (sb == bank_hash).then(|| groups.get(g as usize)).flatten(),
            |wb, w| (wb == bank_hash).then(|| records.get(w as usize)).flatten(),
        )
        .map_err(|error| EncodeError::Length { cue: c.name.clone(), error })?;
        cues.push(Cue { guid, byte_06: c.byte_06, gain: c.gain, length_s, body });
        entries.push(CueEntry::routed(guid, bank_hash, i as u32));
    }
    entries.sort_by_key(|e| e.guid);

    Ok(BankTables {
        wavebank: WavebankFile { bank_hash, stream_name: None, records },
        soundbank: Soundbank { bank_hash, groups, cues },
        sounddb: SoundDb {
            version: SOUNDDB_TAG,
            self_hash: bank_hash,
            cues: entries,
            categories: Vec::new(),
            params: Vec::new(),
        },
    })
}

/// Build and serialize a bank authored table by table.
pub fn encode_general(spec: &TablesSpec) -> Result<EncodedBank, EncodeError> {
    build_general(spec)?.to_bytes()
}

// ---- the simple path: one wave, one single-wave group and one single-track cue per cue ----------

/// Build the three tables for `spec` (validated; nothing serialized yet): cue `i` becomes wave `i`,
/// single-wave group `i` and single-track cue `i`, its length `frames / rate`.
pub fn build_tables(spec: &BankSpec) -> Result<BankTables, EncodeError> {
    if spec.cues.is_empty() {
        return Err(EncodeError::NoCues);
    }
    let mut general = TablesSpec { name: spec.name.clone(), waves: Vec::new(), groups: Vec::new(), cues: Vec::new() };
    for (i, c) in spec.cues.iter().enumerate() {
        check_pcm(&format!("cue {:?}", c.name), &c.pcm)?;
        let hash = pandemic_hash_m2(&c.category);
        if !RETAIL_CATEGORIES.iter().any(|e| e.category == hash) {
            return Err(EncodeError::UnknownCategory {
                cue: format!("cue {:?}", c.name),
                category: c.category.clone(),
                hash,
            });
        }
        let g = &c.group;
        general.waves.push(WaveSpec { clip_hash: c.clip_hash, pcm: c.pcm.clone() });
        general.groups.push(GroupSpec {
            sound_id: c.sound_id,
            category: c.category.clone(),
            head: GroupHeadParams {
                unknown_10: g.unknown_10,
                unknown_14: g.unknown_14,
                min_distance: g.min_distance,
                max_distance: g.max_distance,
                unknown_20: g.unknown_20,
                distance_exponent: g.distance_exponent,
                doppler_scale: g.doppler_scale,
            },
            form: GroupFormSpec::Single { wave: i, gain: g.gain, unknown_30: g.unknown_30, weight: g.wave_weight },
        });
        general.cues.push(CueDef {
            name: c.name.clone(),
            byte_06: c.cue.byte_06,
            gain: c.cue.gain,
            body: CueBodySpec::SingleTrack { group: i, unknown_16: c.cue.unknown_16 },
        });
    }
    build_general(&general)
}

impl BankTables {
    /// Serialize the three tables.
    pub fn to_bytes(&self) -> Result<EncodedBank, EncodeError> {
        Ok(EncodedBank {
            bank_hash: self.soundbank.bank_hash,
            wavebank: self.wavebank.to_bytes().map_err(EncodeError::Wavebank)?,
            soundbank: self.soundbank.to_bytes().map_err(EncodeError::Soundbank)?,
            sounddb: self.sounddb.to_bytes().map_err(EncodeError::SoundDb)?,
        })
    }
}

/// Build and serialize the three tables for `spec`.
pub fn encode_bank(spec: &BankSpec) -> Result<EncodedBank, EncodeError> {
    build_tables(spec)?.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wave::Wavebank;

    fn cue(name: &str, channels: u8, samples: Vec<i16>) -> CueSpec {
        CueSpec {
            name: name.to_string(),
            category: "ui".to_string(),
            sound_id: pandemic_hash_m2(name),
            clip_hash: pandemic_hash_m2(name),
            pcm: Pcm16 { channels, sample_rate: 22050, samples },
            group: UI_PDA_OPEN_GROUP,
            cue: UI_PDA_OPEN_CUE,
        }
    }

    fn spec() -> BankSpec {
        BankSpec {
            name: "mod_ui_sounds".to_string(),
            cues: vec![
                cue("mod_click", 1, (0..1001).map(|i| (i * 7) as i16).collect()),
                cue("mod_whoosh", 2, (0..2000).map(|i| (i * 3) as i16).collect()),
                cue("mod_beep", 1, vec![1234; 17]),
            ],
        }
    }

    /// Encode → parse each table → the parsed tables are the built ones, and the parsed wavebank
    /// decodes to the input PCM.
    #[test]
    fn synthetic_bank_round_trips_through_all_three_parsers() {
        let s = spec();
        let tables = build_tables(&s).expect("builds");
        let enc = tables.to_bytes().expect("encodes");
        assert_eq!(enc.bank_hash, pandemic_hash_m2("mod_ui_sounds"));

        assert_eq!(WavebankFile::parse(&enc.wavebank).expect("wavebank parses"), tables.wavebank);
        assert_eq!(Soundbank::parse(&enc.soundbank).expect("soundbank parses"), tables.soundbank);
        assert_eq!(SoundDb::parse(&enc.sounddb).expect("sounddb parses"), tables.sounddb);

        let wb = Wavebank::parse(&enc.wavebank).expect("decodes");
        for (clip, c) in wb.clips.iter().zip(&s.cues) {
            assert_eq!(clip.samples, c.pcm.samples, "{}: PCM survives verbatim", c.name);
            assert_eq!(clip.channels, c.pcm.channels);
            assert_eq!(clip.sample_rate, 22050);
        }
        // The three tables share the bank hash at +0x04, as retail's do.
        for body in [&enc.wavebank, &enc.soundbank, &enc.sounddb] {
            assert_eq!(&body[4..8], &enc.bank_hash.to_le_bytes());
        }
    }

    #[test]
    fn derived_fields_follow_retail_rules() {
        let tables = build_tables(&spec()).expect("builds");
        // The sounddb is guid-sorted and each entry's third field is the SOUNDBANK cue index.
        for e in &tables.sounddb.cues {
            assert_eq!(tables.soundbank.cues[e.cue_index as usize].guid, e.guid);
        }
        assert!(tables.sounddb.cues.windows(2).all(|w| w[0].guid < w[1].guid));
        // Cue length = frames / rate for a single-wave cue.
        assert_eq!(tables.soundbank.cues[0].length_s, (1001.0f64 / 22050.0) as f32);
        // The group's wave is this bank's wave at the cue's own index.
        let g = &tables.soundbank.groups[2];
        assert_eq!(g.waves()[0].wavebank, tables.soundbank.bank_hash);
        assert_eq!(g.waves()[0].index, 2);
        assert_eq!(g.head.category, 0x8EC8_3583, "m2(\"ui\")");
    }

    #[test]
    fn bad_input_is_refused() {
        let mut s = spec();
        s.cues[1].name = "mod_click".to_string();
        assert!(matches!(encode_bank(&s), Err(EncodeError::DuplicateCue { .. })));

        let mut s = spec();
        s.cues[0].category = "not_a_category".to_string();
        assert!(matches!(encode_bank(&s), Err(EncodeError::UnknownCategory { .. })));

        let mut s = spec();
        s.cues[1].pcm.samples.pop();
        assert!(matches!(encode_bank(&s), Err(EncodeError::PartialFrame { .. })));

        let mut s = spec();
        s.cues[0].pcm.channels = 6;
        assert!(matches!(encode_bank(&s), Err(EncodeError::UnsupportedChannels { .. })));

        let mut s = spec();
        s.cues[2].pcm.samples.clear();
        assert!(matches!(encode_bank(&s), Err(EncodeError::EmptyAudio { .. })));

        let mut s = spec();
        s.cues.clear();
        assert_eq!(encode_bank(&s), Err(EncodeError::NoCues));
    }

    /// A bank with a multi-wave group and a multi-track cue: it round-trips through the parsers, the
    /// engine resolves every path of the multi-track cue to the authored PCM, and the engine's picks
    /// follow the authored selection modes.
    #[test]
    fn multi_wave_groups_and_multi_track_cues_author_and_resolve() {
        use crate::multitrack::{MultiTrackCue, Sound, SoundEntry, Track};
        use crate::sounddb::SoundDb;
        use crate::AudioEngine;

        let bank = pandemic_hash_m2("mod_layers");
        let head = GroupHeadParams {
            unknown_10: 0.95,
            unknown_14: 0,
            min_distance: 10.0,
            max_distance: 1000.0,
            unknown_20: 1.0,
            distance_exponent: 1.0,
            doppler_scale: 1.0,
        };
        let multi = MultiGroupParams {
            byte_2c: 0,
            selection: 0, // sequential: picks 0, 1, 2, 0, ...
            byte_2f: 1,
            unknown_30: 0.0,
            unknown_34: 0.0,
            unknown_3c: 0.0,
            unknown_40: 0.0,
            word_48: 0,
            floats_4c: [0.5, 0.5, 0.5, 0.0, 0.0, 0.0],
            unknown_64: 0.0,
        };
        let wave = |v: i16| WaveSpec { clip_hash: 0x1000 + v as u32, pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![v; 64] } };
        let spec = TablesSpec {
            name: "mod_layers".to_string(),
            waves: vec![wave(1), wave(2), wave(3), wave(4)],
            groups: vec![
                GroupSpec {
                    sound_id: 1,
                    category: "sfx".to_string(),
                    head,
                    form: GroupFormSpec::Multi { params: multi, waves: vec![(0, 0.3), (1, 0.3), (2, 0.4)] },
                },
                GroupSpec {
                    sound_id: 2,
                    category: "sfx".to_string(),
                    head,
                    form: GroupFormSpec::Single { wave: 3, gain: 1.0, unknown_30: 0.0, weight: 1.0 },
                },
            ],
            cues: vec![CueDef {
                name: "mod_layered_hit".to_string(),
                byte_06: 0,
                gain: 1.0,
                body: CueBodySpec::MultiTrack(MultiTrackCue {
                    byte_10: 0,
                    sound_slots: 1,
                    unknown_18: 1.0,
                    unknown_1c: -1.0,
                    unknown_20: -1.0,
                    unknown_24: 0.0,
                    events: vec![],
                    curves: vec![],
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
                                entries: vec![SoundEntry { soundbank: bank, group_index: 0, unknown_06: 0, weight: 1.0 }],
                            }],
                        },
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
                                start_s: 0.25,
                                entries: vec![SoundEntry { soundbank: bank, group_index: 1, unknown_06: 0, weight: 1.0 }],
                            }],
                        },
                    ],
                    params: vec![],
                }),
            }],
        };
        let tables = build_general(&spec).expect("builds");
        let enc = tables.to_bytes().expect("encodes");
        assert_eq!(Soundbank::parse(&enc.soundbank).expect("parses"), tables.soundbank);

        let mut eng = AudioEngine::default();
        eng.set_rng_seed(1);
        eng.set_sounddb(SoundDb::parse(&enc.sounddb).unwrap());
        eng.load_soundbank(&enc.soundbank).unwrap();
        eng.load_wavebank(&enc.wavebank).unwrap();
        let entry = *eng.sounddb.find_cue_by_name("mod_layered_hit").unwrap();
        let resolved = eng.resolve_cue(&entry).expect("every path resolves");
        let clips: Vec<u32> = resolved.waves().map(|w| w.clip_hash).collect();
        assert_eq!(clips, vec![0x1001, 0x1002, 0x1003, 0x1004], "both tracks, every wave");

        // Track 0's group is sequential; track 1 plays wave 3 once its track passes 0.25 s.
        for want in [0u32, 1, 2, 0] {
            let h = eng.cue_sound(entry.guid, None).expect("the cue starts");
            eng.tick(0.1);
            let first: Vec<u32> = eng.cue_instances(h).iter().map(|i| i.wave.expect("a picked wave").index).collect();
            assert_eq!(first, vec![want], "only track 0's sound has started");
            eng.tick(0.1);
            eng.tick(0.1);
            let all: Vec<u32> = eng.cue_instances(h).iter().map(|i| i.wave.expect("a picked wave").index).collect();
            assert_eq!(all, vec![want, 3]);
            eng.stop_sound(h);
        }

        let mut bad = spec.clone();
        if let GroupFormSpec::Multi { params, .. } = &mut bad.groups[0].form {
            params.selection = 3;
        }
        assert!(matches!(build_general(&bad), Err(EncodeError::SelectionMode { mode: 3, .. })));
        let mut bad = spec;
        bad.groups[0].form = GroupFormSpec::Single { wave: 9, gain: 1.0, unknown_30: 0.0, weight: 1.0 };
        assert!(matches!(build_general(&bad), Err(EncodeError::WaveOutOfRange { wave: 9, .. })));
    }

    #[test]
    fn retail_categories_are_sorted_like_the_table_they_came_from() {
        assert!(RETAIL_CATEGORIES.windows(2).all(|w| w[0].category < w[1].category));
        for name in ["ui", "sfx", "vo", "music", "vehicle", "weapon", "collision", "ambience"] {
            let h = pandemic_hash_m2(name);
            assert!(RETAIL_CATEGORIES.iter().any(|c| c.category == h), "{name}");
        }
    }
}
