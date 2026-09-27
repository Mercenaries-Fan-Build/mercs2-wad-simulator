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
//! The derived fields are computed as retail computes them, and are checked against retail by
//! `tests/retail_banks.rs`: the cue length is `frames / rate` (bit-exact on every embedded single-wave
//! retail cue), the sounddb is sorted by guid, blobs are 16-aligned.

use mercs2_formats::hash::pandemic_hash_m2;

use crate::soundbank::{
    Cue, CueBody, Group, GroupForm, GroupHead, Soundbank, SoundbankError, WaveRef,
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
    /// `+0x24` pitch.
    pub pitch: f32,
    /// `+0x28`, unknown.
    pub unknown_28: f32,
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
    /// `+0x06`, unknown.
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
    pitch: f32::from_bits(0x3F80_0000),        // 1.0
    unknown_28: f32::from_bits(0x3F80_0000),   // 1.0
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
    /// A count does not fit its on-disk field.
    TooLarge { what: &'static str, value: usize },
    /// The wavebank serializer refused the table.
    Wavebank(WaveError),
    /// The soundbank serializer refused the table.
    Soundbank(SoundbankError),
    /// The sounddb serializer refused the table.
    SoundDb(SoundDbError),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::EmptyName => write!(f, "encode: a bank or cue name is empty"),
            EncodeError::NoCues => write!(f, "encode: the bank has no cues"),
            EncodeError::DuplicateCue { name, guid } => {
                write!(f, "encode: cue {name:?} hashes to 0x{guid:08X}, which another cue already has")
            }
            EncodeError::UnknownCategory { cue, category, hash } => write!(
                f,
                "encode: cue {cue:?} category {category:?} (0x{hash:08X}) is not a retail category"
            ),
            EncodeError::UnsupportedChannels { cue, channels } => {
                write!(f, "encode: cue {cue:?} has {channels} channels, expected 1 or 2")
            }
            EncodeError::PartialFrame { cue, samples, channels } => {
                write!(f, "encode: cue {cue:?} has {samples} samples, not a multiple of {channels}")
            }
            EncodeError::EmptyAudio { cue } => write!(f, "encode: cue {cue:?} has no samples"),
            EncodeError::ZeroRate { cue } => write!(f, "encode: cue {cue:?} has sample rate 0"),
            EncodeError::TooLarge { what, value } => {
                write!(f, "encode: {what} = {value} does not fit its on-disk field")
            }
            EncodeError::Wavebank(e) => write!(f, "encode: {e}"),
            EncodeError::Soundbank(e) => write!(f, "encode: {e}"),
            EncodeError::SoundDb(e) => write!(f, "encode: {e}"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// The cue length retail stores for a clip: `frames / rate` seconds, rounded to `f32`.
pub fn cue_length_s(frames: u32, sample_rate: u32) -> f32 {
    (f64::from(frames) / f64::from(sample_rate)) as f32
}

/// Build the three tables for `spec` (validated; nothing serialized yet).
pub fn build_tables(spec: &BankSpec) -> Result<BankTables, EncodeError> {
    if spec.name.is_empty() {
        return Err(EncodeError::EmptyName);
    }
    if spec.cues.is_empty() {
        return Err(EncodeError::NoCues);
    }
    if spec.cues.len() > u16::MAX as usize {
        return Err(EncodeError::TooLarge { what: "cue count", value: spec.cues.len() });
    }
    let bank_hash = pandemic_hash_m2(&spec.name);

    let mut records = Vec::with_capacity(spec.cues.len());
    let mut groups = Vec::with_capacity(spec.cues.len());
    let mut cues = Vec::with_capacity(spec.cues.len());
    let mut entries = Vec::with_capacity(spec.cues.len());
    for (i, c) in spec.cues.iter().enumerate() {
        if c.name.is_empty() {
            return Err(EncodeError::EmptyName);
        }
        let guid = pandemic_hash_m2(&c.name);
        if entries.iter().any(|e: &CueEntry| e.guid == guid) {
            return Err(EncodeError::DuplicateCue { name: c.name.clone(), guid });
        }
        let category = pandemic_hash_m2(&c.category);
        if !RETAIL_CATEGORIES.iter().any(|e| e.category == category) {
            return Err(EncodeError::UnknownCategory {
                cue: c.name.clone(),
                category: c.category.clone(),
                hash: category,
            });
        }
        let ch = c.pcm.channels;
        if ch != 1 && ch != 2 {
            return Err(EncodeError::UnsupportedChannels { cue: c.name.clone(), channels: ch });
        }
        if c.pcm.samples.is_empty() {
            return Err(EncodeError::EmptyAudio { cue: c.name.clone() });
        }
        if c.pcm.samples.len() % ch as usize != 0 {
            return Err(EncodeError::PartialFrame {
                cue: c.name.clone(),
                samples: c.pcm.samples.len(),
                channels: ch,
            });
        }
        if c.pcm.sample_rate == 0 {
            return Err(EncodeError::ZeroRate { cue: c.name.clone() });
        }
        let frames_usize = c.pcm.samples.len() / ch as usize;
        let frames = u32::try_from(frames_usize)
            .map_err(|_| EncodeError::TooLarge { what: "frame count", value: frames_usize })?;
        let data: Vec<u8> = c.pcm.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        if u32::try_from(data.len()).is_err() {
            return Err(EncodeError::TooLarge { what: "clip bytes", value: data.len() });
        }

        records.push(WaveRecord {
            clip_hash: c.clip_hash,
            channels: ch,
            format: BYTES_PER_SAMPLE_PCM16,
            sample_rate: c.pcm.sample_rate,
            frames,
            data: WaveData::Embedded(data),
        });
        let g = &c.group;
        groups.push(Group {
            head: GroupHead {
                sound_id: c.sound_id,
                category,
                unknown_10: g.unknown_10,
                unknown_14: g.unknown_14,
                min_distance: g.min_distance,
                max_distance: g.max_distance,
                unknown_20: g.unknown_20,
                pitch: g.pitch,
                unknown_28: g.unknown_28,
            },
            form: GroupForm::Single {
                gain: g.gain,
                unknown_30: g.unknown_30,
                wave: WaveRef { wavebank: bank_hash, index: i as u32, weight: g.wave_weight },
            },
        });
        cues.push(Cue {
            guid,
            byte_06: c.cue.byte_06,
            gain: c.cue.gain,
            length_s: cue_length_s(frames, c.pcm.sample_rate),
            body: CueBody::SingleTrack {
                soundbank: bank_hash,
                group_index: i as u16,
                unknown_16: c.cue.unknown_16,
            },
        });
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
        // Cue length = frames / rate.
        assert_eq!(tables.soundbank.cues[0].length_s, cue_length_s(1001, 22050));
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

    #[test]
    fn retail_categories_are_sorted_like_the_table_they_came_from() {
        assert!(RETAIL_CATEGORIES.windows(2).all(|w| w[0].category < w[1].category));
        for name in ["ui", "sfx", "vo", "music", "vehicle", "weapon", "collision", "ambience"] {
            let h = pandemic_hash_m2(name);
            assert!(RETAIL_CATEGORIES.iter().any(|c| c.category == h), "{name}");
        }
    }
}
