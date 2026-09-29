//! Wavebank codec + decode — the `wavebank` table (`0xF753F6D0`, `Sound.LoadWaveBank` `FUN_005e26b0`)
//! as exact bytes ([`WavebankFile`]) and as resident PCM clips for the mixer ([`Wavebank`]).
//!
//! ## Layout (measured on every wavebank in retail `vz.wad`, `English.wad` and `shell.wad`)
//!
//! ```text
//! header (little-endian)
//!   +0x00 u32  table version, 0x1D (the value every audio table carries — NOT a record count)
//!   +0x04 u32  bank hash = m2(bank name)
//!   +0x08 u16  record count
//!   +0x0A u16  0 = embedded bank; 1 = streamed bank (the records address a named .pws file)
//!   +0x0C u32  the bank hash again
//!   +0x10 u32  records offset: 24 for an embedded bank, 40 for a streamed bank
//!   +0x14 u32  0
//!   +0x18      streamed banks only: the .pws file name, NUL-padded to 16 bytes
//! record (36 bytes each, in index order)
//!   +0x00 u32  clip hash
//!   +0x04 u8x4 [0, channels, format, 0]; format = bytes per sample (2) when embedded, 4 when streamed
//!   +0x08 u32  sample rate
//!   +0x0C u32  data size in bytes
//!   +0x10 u32  frame count (samples per channel)
//!   +0x14      8 zero bytes
//!   +0x1C u32  0 when embedded; a per-record value of unknown meaning when streamed
//!   +0x20 u32  data offset — RELATIVE TO THE RECORD'S OWN START when embedded; the byte offset into
//!              the .pws file when streamed
//! embedded blob area
//!   each record's samples, in record order, each blob starting on a 16-byte boundary of the body
//!   (the first one at the first boundary at or after the end of the record table); every gap and
//!   the tail up to the next 16-byte boundary is zero-filled — the body length is itself a multiple
//!   of 16 (this is the "trailing padding" a body shows when its last blob does not end on a
//!   boundary)
//! ```
//!
//! Proof: with `+0x20` read record-relative, every one of the 1,943 mono + 100 stereo embedded clips
//! in `vz.wad` starts exactly at the 16-aligned end of its predecessor and every body ends at
//! `align16(last blob end)`; read body-relative, the offsets land inside the record table. `+0x0C`
//! equals `frames × channels × 2` on every embedded record, i.e. the payload is interleaved PCM16.
//! The byte-identical re-encode of every retail wavebank ([`WavebankFile::to_bytes`], exercised by
//! `tests/retail_banks.rs`) is the executable form of this proof.
//!
//! Anything outside this measured layout is a hard [`WaveError`], never a best-effort partial decode.
//!
//! The IMA ADPCM decoders below are not on the embedded PC path (every embedded clip is PCM16); they
//! stay `pub` because the console converter and the VO stream tools use them.

use crate::le::{align16, put_u16, put_u32, u16_at, u32_at, u8_at};

/// The table version every audio table (wavebank, soundbank, sounddb) carries at `+0x00`.
pub const TABLE_VERSION: u32 = 0x1D;
/// One clip record's fixed size (`FUN_00603110` record stride).
pub const RECORD_SIZE: usize = 36;
/// Header size of an embedded bank — where its record table starts.
pub const HEADER_SIZE: usize = 24;
/// Size of the NUL-padded `.pws` file-name field a streamed bank carries at `+0x18`.
pub const STREAM_NAME_FIELD: usize = 16;
/// Header size of a streamed bank (header + file-name field) — where its record table starts.
pub const STREAM_HEADER_SIZE: usize = HEADER_SIZE + STREAM_NAME_FIELD;
/// Every embedded blob starts on, and the body ends on, a multiple of this.
pub const BLOB_ALIGN: usize = 16;

/// Format byte `0x02` — **2 bytes per sample**, i.e. interleaved little-endian PCM16. This is what
/// every embedded clip in retail `vz.wad` / `English.wad` / `shell.wad` carries.
pub const BYTES_PER_SAMPLE_PCM16: u8 = 0x02;
/// Deprecated misnomer: the `0x02` in the format byte is a sample WIDTH, not an IMA codec id.
/// Kept so existing call sites still compile; prefer [`BYTES_PER_SAMPLE_PCM16`].
#[deprecated(note = "the format byte is bytes-per-sample; 0x02 = PCM16, not IMA")]
pub const CODEC_IMA: u8 = 0x02;
/// Format byte `0x00` — raw signed-16 PCM, embedded.
pub const CODEC_PCM: u8 = 0x00;
/// Format byte `0x04` — every record of a streamed bank carries it; its samples live in the `.pws`.
pub const CODEC_STREAM: u8 = 0x04;
/// Codec `0x01` / `0x69` — XMA (Xbox); not decodable on the PC path.
pub const CODEC_XMA: u8 = 0x01;
/// Codec `0x05` — Xbox ADPCM; not decodable on the PC path.
pub const CODEC_XBOX_ADPCM: u8 = 0x05;

// --- IMA ADPCM tables (identical to the retail-verified tool decoder) --------------------------------

const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

/// Mono IMA block: 4-byte header (predictor + step index) + 32 nibble bytes → 65 samples.
pub const MONO_BLOCK_SIZE: usize = 36;
/// Stereo IMA block: 8-byte dual header + 64 interleaved nibble bytes.
pub const STEREO_BLOCK_SIZE: usize = 72;

#[inline]
fn clamp_step_index(step_index: i32) -> i32 {
    step_index.clamp(0, STEP_TABLE.len() as i32 - 1)
}

#[inline]
fn decode_nibble(nibble: u8, predictor: i32, step_index: i32) -> (i32, i32) {
    let step = STEP_TABLE[clamp_step_index(step_index) as usize];
    let mut diff = step >> 3;
    if nibble & 1 != 0 {
        diff += step >> 2;
    }
    if nibble & 2 != 0 {
        diff += step >> 1;
    }
    if nibble & 4 != 0 {
        diff += step;
    }
    if nibble & 8 != 0 {
        diff = -diff;
    }
    let predictor_i = (predictor + diff).clamp(-32768, 32767);
    let new_step = clamp_step_index(step_index + INDEX_TABLE[(nibble & 0x0F) as usize]);
    (predictor_i, new_step)
}

/// Decode a mono IMA ADPCM blob to signed-16 PCM (36-byte blocks, step index clamped like the engine).
pub fn decode_ima_mono(data: &[u8]) -> Vec<i16> {
    let mut samples = Vec::new();
    let mut offset = 0usize;
    while offset + MONO_BLOCK_SIZE <= data.len() {
        let predictor = i16::from_le_bytes([data[offset], data[offset + 1]]);
        let mut step_index = clamp_step_index(i32::from(data[offset + 2]));
        let mut predictor_i = i32::from(predictor);
        samples.push(predictor);
        for byte_idx in 0..32 {
            let b = data[offset + 4 + byte_idx];
            for nibble in [b & 0x0F, b >> 4] {
                let (p, s) = decode_nibble(nibble, predictor_i, step_index);
                predictor_i = p;
                step_index = s;
                samples.push(predictor_i as i16);
            }
        }
        offset += MONO_BLOCK_SIZE;
    }
    samples
}

/// Decode a stereo IMA ADPCM blob; returns interleaved L/R signed-16 PCM (72-byte MS-IMA blocks).
pub fn decode_ima_stereo(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + STEREO_BLOCK_SIZE <= data.len() {
        let l_pred = i32::from(i16::from_le_bytes([data[offset], data[offset + 1]]));
        let mut l_step = clamp_step_index(i32::from(data[offset + 2]));
        let r_pred = i32::from(i16::from_le_bytes([data[offset + 4], data[offset + 5]]));
        let mut r_step = clamp_step_index(i32::from(data[offset + 6]));
        let mut l_pred_i = l_pred;
        let mut r_pred_i = r_pred;
        // The block header samples come first (L then R, interleaved).
        out.push(l_pred as i16);
        out.push(r_pred as i16);
        // Pending decoded samples are buffered per-channel then interleaved, since MS-IMA emits 8
        // L-samples then 8 R-samples per 8-byte group.
        let mut lbuf = Vec::with_capacity(64);
        let mut rbuf = Vec::with_capacity(64);
        for group in 0..8 {
            let base = offset + 8 + group * 8;
            for i in 0..4 {
                let lb = data[base + i];
                for nibble in [lb & 0x0F, lb >> 4] {
                    let (p, s) = decode_nibble(nibble, l_pred_i, l_step);
                    l_pred_i = p;
                    l_step = s;
                    lbuf.push(l_pred_i as i16);
                }
                let rb = data[base + 4 + i];
                for nibble in [rb & 0x0F, rb >> 4] {
                    let (p, s) = decode_nibble(nibble, r_pred_i, r_step);
                    r_pred_i = p;
                    r_step = s;
                    rbuf.push(r_pred_i as i16);
                }
            }
        }
        for (l, r) in lbuf.into_iter().zip(rbuf.into_iter()) {
            out.push(l);
            out.push(r);
        }
        offset += STEREO_BLOCK_SIZE;
    }
    out
}

/// A decoded, resident clip: interleaved int16 samples plus the rate/channels the mixer needs to bind
/// it as a [`PcmSource`](crate::mixer::PcmSource) at the correct pitch.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedClip {
    /// m2 clip hash.
    pub clip_hash: u32,
    /// Channel count (1 mono / 2 stereo).
    pub channels: u8,
    /// The clip's native sample rate; resampled to the mixer rate at play time.
    pub sample_rate: u32,
    /// Interleaved int16 PCM (empty when the clip streams from an external `.pws`).
    pub samples: Vec<i16>,
    /// True when the clip belongs to a streamed bank: its samples live in the bank's `.pws` file, not
    /// in the bank body. Decided by the bank header (`+0x0A` = 1 and a `.pws` name), which is the
    /// only thing that makes a record's offset point outside the body.
    pub streaming: bool,
}

impl DecodedClip {
    /// Frame count (samples / channels).
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }
}

/// Everything that can be wrong with a wavebank body. Each is a hard error: a body outside the
/// measured layout is refused whole rather than partially decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaveError {
    /// A field ran past the end of the body.
    Truncated { field: &'static str, offset: usize, len: usize },
    /// `+0x00` was not [`TABLE_VERSION`].
    BadVersion(u32),
    /// `+0x0C` did not repeat the bank hash at `+0x04`.
    HashMismatch { at_4: u32, at_c: u32 },
    /// `+0x0A` was neither 0 (embedded) nor 1 (streamed).
    UnknownBankKind(u16),
    /// `+0x10` was not the records offset the bank kind implies.
    BadRecordsOffset { found: u32, expected: u32 },
    /// A field every retail bank carries as zero was not zero.
    NonZeroReserved { field: &'static str, offset: usize },
    /// The bank holds no records (no retail bank does; the layout of an empty one is unmeasured).
    Empty,
    /// More records than the `u16` count can hold.
    TooManyRecords(usize),
    /// The streamed bank's `.pws` name field was not ASCII followed by NUL padding.
    BadStreamName,
    /// A `.pws` name too long for the 16-byte field (which always ends in at least one NUL).
    StreamNameTooLong(String),
    /// A record's channel count is not 1 or 2.
    UnsupportedChannels { record: usize, channels: u8 },
    /// A record's format byte is not the one its bank kind carries (2 embedded, 4 streamed).
    UnsupportedFormat { record: usize, format: u8 },
    /// An embedded record's size is not `frames × channels × bytes per sample`.
    SizeMismatch { record: usize, size: u32, frames: u32, channels: u8, format: u8 },
    /// An embedded blob does not start at the 16-aligned end of the previous one.
    Misplaced { record: usize, found: usize, expected: usize },
    /// A padding byte between or after the blobs was not zero.
    NonZeroPadding { offset: usize },
    /// The body does not end at the 16-aligned end of its last blob.
    BadLength { found: usize, expected: usize },
    /// A record's data kind does not match its bank (embedded data in a streamed bank or vice versa).
    DataKindMismatch { record: usize },
    /// A number did not fit its on-disk field.
    FieldOverflow { field: &'static str, value: usize },
}

impl std::fmt::Display for WaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveError::Truncated { field, offset, len } => {
                write!(f, "wavebank: {field} at +0x{offset:X} runs past the {len}-byte body")
            }
            WaveError::BadVersion(v) => write!(f, "wavebank: version 0x{v:X}, expected 0x1D"),
            WaveError::HashMismatch { at_4, at_c } => {
                write!(f, "wavebank: hash +0x04 0x{at_4:08X} != +0x0C 0x{at_c:08X}")
            }
            WaveError::UnknownBankKind(k) => write!(f, "wavebank: +0x0A = {k}, expected 0 or 1"),
            WaveError::BadRecordsOffset { found, expected } => {
                write!(f, "wavebank: records offset {found}, expected {expected}")
            }
            WaveError::NonZeroReserved { field, offset } => {
                write!(f, "wavebank: reserved {field} at +0x{offset:X} is not zero")
            }
            WaveError::Empty => write!(f, "wavebank: no records (an empty bank's layout is unmeasured)"),
            WaveError::TooManyRecords(n) => write!(f, "wavebank: {n} records exceed the u16 count"),
            WaveError::BadStreamName => write!(f, "wavebank: .pws name field is not ASCII + NUL padding"),
            WaveError::StreamNameTooLong(n) => {
                write!(f, "wavebank: .pws name {n:?} does not fit the 16-byte NUL-terminated field")
            }
            WaveError::UnsupportedChannels { record, channels } => {
                write!(f, "wavebank: record {record} has {channels} channels, expected 1 or 2")
            }
            WaveError::UnsupportedFormat { record, format } => {
                write!(f, "wavebank: record {record} format byte 0x{format:02X} is not the bank kind's")
            }
            WaveError::SizeMismatch { record, size, frames, channels, format } => write!(
                f,
                "wavebank: record {record} size {size} != {frames} frames x {channels} ch x {format} B"
            ),
            WaveError::Misplaced { record, found, expected } => write!(
                f,
                "wavebank: record {record} blob at +0x{found:X}, expected the aligned +0x{expected:X}"
            ),
            WaveError::NonZeroPadding { offset } => {
                write!(f, "wavebank: padding byte at +0x{offset:X} is not zero")
            }
            WaveError::BadLength { found, expected } => {
                write!(f, "wavebank: body is {found} bytes, the layout implies {expected}")
            }
            WaveError::DataKindMismatch { record } => {
                write!(f, "wavebank: record {record}'s data kind does not match the bank kind")
            }
            WaveError::FieldOverflow { field, value } => {
                write!(f, "wavebank: {field} = {value} does not fit its on-disk field")
            }
        }
    }
}

impl std::error::Error for WaveError {}

/// Where a record's samples are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaveData {
    /// Interleaved little-endian samples carried in the bank body.
    Embedded(Vec<u8>),
    /// Samples in the bank's `.pws` file.
    Streamed {
        /// Byte offset into the `.pws` file (`+0x20`).
        offset: u32,
        /// Byte size in the `.pws` file (`+0x0C`).
        size: u32,
        /// `+0x1C`, of unknown meaning (zero in some records, not in others).
        word_1c: u32,
    },
}

/// One 36-byte clip record plus its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaveRecord {
    /// `+0x00` clip hash.
    pub clip_hash: u32,
    /// `+0x05` channel count (1 or 2).
    pub channels: u8,
    /// `+0x06` format byte: bytes per sample (2) when embedded, 4 when streamed.
    pub format: u8,
    /// `+0x08` sample rate.
    pub sample_rate: u32,
    /// `+0x10` frame count (samples per channel).
    pub frames: u32,
    /// The samples, embedded or streamed.
    pub data: WaveData,
}

/// A wavebank table exactly as it sits in the `data` chunk: [`parse`](Self::parse) and
/// [`to_bytes`](Self::to_bytes) are inverses over every body the measured layout admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WavebankFile {
    /// `+0x04` / `+0x0C` bank hash (m2 of the bank name).
    pub bank_hash: u32,
    /// The `.pws` file a streamed bank's records address; `None` for an embedded bank.
    pub stream_name: Option<String>,
    /// The records, in index order.
    pub records: Vec<WaveRecord>,
}

fn rd32(b: &[u8], off: usize, field: &'static str) -> Result<u32, WaveError> {
    u32_at(b, off).ok_or(WaveError::Truncated { field, offset: off, len: b.len() })
}
fn rd16(b: &[u8], off: usize, field: &'static str) -> Result<u16, WaveError> {
    u16_at(b, off).ok_or(WaveError::Truncated { field, offset: off, len: b.len() })
}
fn rd8(b: &[u8], off: usize, field: &'static str) -> Result<u8, WaveError> {
    u8_at(b, off).ok_or(WaveError::Truncated { field, offset: off, len: b.len() })
}
fn zero(b: &[u8], off: usize, n: usize, field: &'static str) -> Result<(), WaveError> {
    let s = b.get(off..off + n).ok_or(WaveError::Truncated { field, offset: off, len: b.len() })?;
    if s.iter().any(|&x| x != 0) {
        return Err(WaveError::NonZeroReserved { field, offset: off });
    }
    Ok(())
}

impl WavebankFile {
    /// Parse a decompressed wavebank body. Refuses anything outside the measured layout.
    pub fn parse(body: &[u8]) -> Result<WavebankFile, WaveError> {
        let version = rd32(body, 0x00, "version")?;
        if version != TABLE_VERSION {
            return Err(WaveError::BadVersion(version));
        }
        let bank_hash = rd32(body, 0x04, "bank hash")?;
        let count = rd16(body, 0x08, "record count")? as usize;
        let kind = rd16(body, 0x0A, "bank kind")?;
        let at_c = rd32(body, 0x0C, "bank hash (repeat)")?;
        if at_c != bank_hash {
            return Err(WaveError::HashMismatch { at_4: bank_hash, at_c });
        }
        let records_off = rd32(body, 0x10, "records offset")?;
        zero(body, 0x14, 4, "header +0x14")?;
        let streamed = match kind {
            0 => false,
            1 => true,
            k => return Err(WaveError::UnknownBankKind(k)),
        };
        let expected_off = if streamed { STREAM_HEADER_SIZE } else { HEADER_SIZE } as u32;
        if records_off != expected_off {
            return Err(WaveError::BadRecordsOffset { found: records_off, expected: expected_off });
        }
        if count == 0 {
            return Err(WaveError::Empty);
        }
        let stream_name = if streamed {
            let field = body
                .get(HEADER_SIZE..STREAM_HEADER_SIZE)
                .ok_or(WaveError::Truncated { field: ".pws name", offset: HEADER_SIZE, len: body.len() })?;
            let n = field.iter().position(|&c| c == 0).ok_or(WaveError::BadStreamName)?;
            if n == 0 || !field[..n].is_ascii() || field[n..].iter().any(|&c| c != 0) {
                return Err(WaveError::BadStreamName);
            }
            Some(String::from_utf8(field[..n].to_vec()).map_err(|_| WaveError::BadStreamName)?)
        } else {
            None
        };

        let format_expected = if streamed { CODEC_STREAM } else { BYTES_PER_SAMPLE_PCM16 };
        let table_end = records_off as usize + count * RECORD_SIZE;
        let mut cursor = table_end; // end of the previous blob (embedded banks)
        let mut records = Vec::with_capacity(count);
        for i in 0..count {
            let r = records_off as usize + i * RECORD_SIZE;
            let clip_hash = rd32(body, r, "clip hash")?;
            if rd8(body, r + 4, "format[0]")? != 0 {
                return Err(WaveError::NonZeroReserved { field: "format[0]", offset: r + 4 });
            }
            let channels = rd8(body, r + 5, "channels")?;
            let format = rd8(body, r + 6, "format")?;
            if rd8(body, r + 7, "format[3]")? != 0 {
                return Err(WaveError::NonZeroReserved { field: "format[3]", offset: r + 7 });
            }
            if channels != 1 && channels != 2 {
                return Err(WaveError::UnsupportedChannels { record: i, channels });
            }
            if format != format_expected {
                return Err(WaveError::UnsupportedFormat { record: i, format });
            }
            let sample_rate = rd32(body, r + 8, "sample rate")?;
            let size = rd32(body, r + 12, "data size")?;
            let frames = rd32(body, r + 16, "frames")?;
            zero(body, r + 20, 8, "record +0x14")?;
            let word_1c = rd32(body, r + 28, "record +0x1C")?;
            let offset = rd32(body, r + 32, "data offset")?;
            let data = if streamed {
                WaveData::Streamed { offset, size, word_1c }
            } else {
                if word_1c != 0 {
                    return Err(WaveError::NonZeroReserved { field: "record +0x1C", offset: r + 28 });
                }
                let want = frames as u64 * channels as u64 * format as u64;
                if want != size as u64 {
                    return Err(WaveError::SizeMismatch { record: i, size, frames, channels, format });
                }
                let start = r + offset as usize;
                let expected = align16(cursor);
                if start != expected {
                    return Err(WaveError::Misplaced { record: i, found: start, expected });
                }
                check_padding(body, cursor, start)?;
                let end = start + size as usize;
                let blob = body
                    .get(start..end)
                    .ok_or(WaveError::Truncated { field: "clip data", offset: start, len: body.len() })?;
                cursor = end;
                WaveData::Embedded(blob.to_vec())
            };
            records.push(WaveRecord { clip_hash, channels, format, sample_rate, frames, data });
        }

        let expected_len = if streamed { table_end } else { align16(cursor) };
        if body.len() != expected_len {
            return Err(WaveError::BadLength { found: body.len(), expected: expected_len });
        }
        if !streamed {
            check_padding(body, cursor, expected_len)?;
        }
        Ok(WavebankFile { bank_hash, stream_name, records })
    }

    /// Serialize to the exact on-disk layout (see the module docs).
    pub fn to_bytes(&self) -> Result<Vec<u8>, WaveError> {
        let count = self.records.len();
        if count == 0 {
            return Err(WaveError::Empty);
        }
        if count > u16::MAX as usize {
            return Err(WaveError::TooManyRecords(count));
        }
        let streamed = self.stream_name.is_some();
        let records_off = if streamed { STREAM_HEADER_SIZE } else { HEADER_SIZE };
        let table_end = records_off + count * RECORD_SIZE;

        // Place every embedded blob first: record i's data offset is relative to record i's start.
        let mut placements = Vec::with_capacity(count);
        let mut cursor = table_end;
        for (i, rec) in self.records.iter().enumerate() {
            if rec.channels != 1 && rec.channels != 2 {
                return Err(WaveError::UnsupportedChannels { record: i, channels: rec.channels });
            }
            match (&rec.data, streamed) {
                (WaveData::Embedded(bytes), false) => {
                    if rec.format != BYTES_PER_SAMPLE_PCM16 {
                        return Err(WaveError::UnsupportedFormat { record: i, format: rec.format });
                    }
                    let want = rec.frames as u64 * rec.channels as u64 * rec.format as u64;
                    if want != bytes.len() as u64 {
                        return Err(WaveError::SizeMismatch {
                            record: i,
                            size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                            frames: rec.frames,
                            channels: rec.channels,
                            format: rec.format,
                        });
                    }
                    let start = align16(cursor);
                    placements.push(start);
                    cursor = start + bytes.len();
                }
                (WaveData::Streamed { .. }, true) => {
                    if rec.format != CODEC_STREAM {
                        return Err(WaveError::UnsupportedFormat { record: i, format: rec.format });
                    }
                    placements.push(0);
                }
                _ => return Err(WaveError::DataKindMismatch { record: i }),
            }
        }
        let total = if streamed { table_end } else { align16(cursor) };
        if u32::try_from(total).is_err() {
            return Err(WaveError::FieldOverflow { field: "body length", value: total });
        }

        let mut out = Vec::with_capacity(total);
        put_u32(&mut out, TABLE_VERSION);
        put_u32(&mut out, self.bank_hash);
        put_u16(&mut out, count as u16);
        put_u16(&mut out, u16::from(streamed));
        put_u32(&mut out, self.bank_hash);
        put_u32(&mut out, records_off as u32);
        put_u32(&mut out, 0);
        if let Some(name) = &self.stream_name {
            if name.is_empty() || !name.is_ascii() || name.as_bytes().contains(&0) {
                return Err(WaveError::BadStreamName);
            }
            if name.len() >= STREAM_NAME_FIELD {
                return Err(WaveError::StreamNameTooLong(name.clone()));
            }
            out.extend_from_slice(name.as_bytes());
            out.resize(STREAM_HEADER_SIZE, 0);
        }
        for (i, rec) in self.records.iter().enumerate() {
            let r = out.len();
            put_u32(&mut out, rec.clip_hash);
            out.extend_from_slice(&[0, rec.channels, rec.format, 0]);
            put_u32(&mut out, rec.sample_rate);
            let (size, word_1c, offset) = match &rec.data {
                WaveData::Embedded(bytes) => (bytes.len() as u32, 0, (placements[i] - r) as u32),
                WaveData::Streamed { offset, size, word_1c } => (*size, *word_1c, *offset),
            };
            put_u32(&mut out, size);
            put_u32(&mut out, rec.frames);
            out.extend_from_slice(&[0u8; 8]);
            put_u32(&mut out, word_1c);
            put_u32(&mut out, offset);
        }
        for (i, rec) in self.records.iter().enumerate() {
            if let WaveData::Embedded(bytes) = &rec.data {
                out.resize(placements[i], 0);
                out.extend_from_slice(bytes);
            }
        }
        out.resize(total, 0);
        Ok(out)
    }
}

fn check_padding(body: &[u8], from: usize, to: usize) -> Result<(), WaveError> {
    let pad = body
        .get(from..to)
        .ok_or(WaveError::Truncated { field: "padding", offset: from, len: body.len() })?;
    match pad.iter().position(|&x| x != 0) {
        Some(p) => Err(WaveError::NonZeroPadding { offset: from + p }),
        None => Ok(()),
    }
}

/// A parsed wavebank as resident clips: its hash + the clips in record order (the order a group's
/// wave index addresses).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wavebank {
    /// The bank's own hash (`+0x04` in the body).
    pub self_hash: u32,
    /// Clips in record order (index = a soundbank group's wave index).
    pub clips: Vec<DecodedClip>,
}

impl Wavebank {
    /// Parse a decompressed wavebank body and decode every embedded clip to PCM16. A streamed bank's
    /// clips carry no samples and `streaming: true` (the audio is in the bank's `.pws`).
    pub fn parse(body: &[u8]) -> Result<Wavebank, WaveError> {
        Ok(Wavebank::from_file(&WavebankFile::parse(body)?))
    }

    /// Decode an already-parsed [`WavebankFile`].
    pub fn from_file(file: &WavebankFile) -> Wavebank {
        let clips = file
            .records
            .iter()
            .map(|rec| {
                let (samples, streaming) = match &rec.data {
                    // WavebankFile::parse admits embedded data only at 2 bytes per sample.
                    WaveData::Embedded(bytes) => (
                        bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
                        false,
                    ),
                    WaveData::Streamed { .. } => (Vec::new(), true),
                };
                DecodedClip {
                    clip_hash: rec.clip_hash,
                    channels: rec.channels,
                    sample_rate: rec.sample_rate,
                    samples,
                    streaming,
                }
            })
            .collect();
        Wavebank { self_hash: file.bank_hash, clips }
    }

    /// Find a resident clip by its hash.
    pub fn clip_by_hash(&self, hash: u32) -> Option<&DecodedClip> {
        self.clips.iter().find(|c| c.clip_hash == hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one mono IMA block from a predictor + 64 zero nibbles: all-zero nibbles keep the predictor
    /// nearly constant, so the decoded block is a known-length run near the seed value.
    fn mono_block(seed: i16) -> Vec<u8> {
        let mut b = Vec::with_capacity(MONO_BLOCK_SIZE);
        b.extend_from_slice(&seed.to_le_bytes());
        b.push(0); // step index
        b.push(0); // reserved
        b.extend(std::iter::repeat(0u8).take(32)); // 64 zero nibbles
        b
    }

    #[test]
    fn ima_mono_block_decodes_to_65_samples() {
        let blk = mono_block(1000);
        let s = decode_ima_mono(&blk);
        // 1 header sample + 64 nibble samples.
        assert_eq!(s.len(), 65);
        assert_eq!(s[0], 1000, "first sample is the block predictor");
        // With all-zero nibbles the predictor drifts by only ±(step>>3); it stays in a tight band.
        assert!(s.iter().all(|&x| (x - 1000).abs() < 40), "near-constant run");
    }

    fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn embedded(clip_hash: u32, channels: u8, samples: &[i16]) -> WaveRecord {
        WaveRecord {
            clip_hash,
            channels,
            format: BYTES_PER_SAMPLE_PCM16,
            sample_rate: 22050,
            frames: (samples.len() / channels as usize) as u32,
            data: WaveData::Embedded(pcm_bytes(samples)),
        }
    }

    /// A two-clip bank, hand-assembled byte by byte from the measured layout, so the codec is checked
    /// against the layout rather than against itself.
    fn hand_built_two_clip_bank() -> Vec<u8> {
        let a: Vec<i16> = (0..5).collect(); // 10 bytes
        let b: Vec<i16> = (100..106).collect(); // stereo, 3 frames, 12 bytes
        let mut body = Vec::new();
        body.extend_from_slice(&0x1Du32.to_le_bytes());
        body.extend_from_slice(&0xABCD_1234u32.to_le_bytes());
        body.extend_from_slice(&2u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&0xABCD_1234u32.to_le_bytes());
        body.extend_from_slice(&24u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        // table ends at 24 + 72 = 96 (already 16-aligned): blob a at 96..106, blob b at 112..124,
        // body ends at 128.
        for (r, hash, ch, n, frames, start) in
            [(24usize, 0x1111u32, 1u8, 10u32, 5u32, 96usize), (60, 0x2222, 2, 12, 3, 112)]
        {
            body.extend_from_slice(&hash.to_le_bytes());
            body.extend_from_slice(&[0, ch, 2, 0]);
            body.extend_from_slice(&22050u32.to_le_bytes());
            body.extend_from_slice(&n.to_le_bytes());
            body.extend_from_slice(&frames.to_le_bytes());
            body.extend_from_slice(&[0u8; 12]);
            body.extend_from_slice(&((start - r) as u32).to_le_bytes()); // record-relative
        }
        assert_eq!(body.len(), 96);
        body.extend_from_slice(&pcm_bytes(&a));
        body.resize(112, 0);
        body.extend_from_slice(&pcm_bytes(&b));
        body.resize(128, 0);
        body
    }

    #[test]
    fn parses_the_measured_layout_with_record_relative_offsets() {
        let body = hand_built_two_clip_bank();
        let file = WavebankFile::parse(&body).expect("hand-built bank parses");
        assert_eq!(file.bank_hash, 0xABCD_1234);
        assert_eq!(file.stream_name, None);
        assert_eq!(file.records.len(), 2);
        assert_eq!(file.records[1].channels, 2);
        assert_eq!(file.records[1].frames, 3);

        let bank = Wavebank::from_file(&file);
        assert_eq!(bank.clips[0].samples, (0..5).collect::<Vec<i16>>(), "PCM16 decoded verbatim");
        assert_eq!(bank.clips[1].samples, (100..106).collect::<Vec<i16>>());
        assert_eq!(bank.clips[1].frames(), 3);
        assert!(!bank.clips[0].streaming);
        assert!(bank.clip_by_hash(0x2222).is_some());

        assert_eq!(file.to_bytes().expect("encodes"), body, "the encoder reproduces the hand layout");
    }

    #[test]
    fn a_body_relative_offset_is_refused() {
        // Rewrite record 0's +0x20 as the BODY-relative 96 instead of the record-relative 72.
        let mut body = hand_built_two_clip_bank();
        body[24 + 32..24 + 36].copy_from_slice(&96u32.to_le_bytes());
        assert!(matches!(WavebankFile::parse(&body), Err(WaveError::Misplaced { record: 0, .. })));
    }

    #[test]
    fn plus_zero_is_the_version_not_a_count() {
        let mut body = hand_built_two_clip_bank();
        body[0..4].copy_from_slice(&2u32.to_le_bytes()); // what a "count" reading would expect
        assert_eq!(WavebankFile::parse(&body), Err(WaveError::BadVersion(2)));
    }

    #[test]
    fn padding_and_tail_are_exact() {
        let body = hand_built_two_clip_bank();
        let mut dirty = body.clone();
        dirty[106] = 1; // gap between blob a and blob b
        assert_eq!(WavebankFile::parse(&dirty), Err(WaveError::NonZeroPadding { offset: 106 }));
        let mut short = body.clone();
        short.truncate(124); // drop the tail padding
        assert!(matches!(WavebankFile::parse(&short), Err(WaveError::BadLength { .. })));
        let mut long = body;
        long.extend_from_slice(&[0u8; 16]);
        assert!(matches!(WavebankFile::parse(&long), Err(WaveError::BadLength { .. })));
    }

    #[test]
    fn embedded_round_trip_with_odd_sizes() {
        let file = WavebankFile {
            bank_hash: 0x5FBA_3915,
            stream_name: None,
            records: vec![
                embedded(1, 1, &[1, 2, 3]),
                embedded(2, 2, &[4, 5, 6, 7, 8, 9, 10, 11]),
                embedded(3, 1, &[12]),
            ],
        };
        let bytes = file.to_bytes().expect("encodes");
        assert_eq!(bytes.len() % BLOB_ALIGN, 0);
        assert_eq!(WavebankFile::parse(&bytes).expect("parses"), file);
    }

    #[test]
    fn streamed_bank_round_trips_and_decodes_no_samples() {
        let file = WavebankFile {
            bank_hash: 0x7871_F925,
            stream_name: Some("ambience.pws".to_string()),
            records: vec![WaveRecord {
                clip_hash: 0x54FF_867B,
                channels: 2,
                format: CODEC_STREAM,
                sample_rate: 44100,
                frames: 0x008A_3344,
                data: WaveData::Streamed { offset: 0, size: 0x0026_DEA0, word_1c: 0x0002_43C0 },
            }],
        };
        let bytes = file.to_bytes().expect("encodes");
        assert_eq!(bytes.len(), STREAM_HEADER_SIZE + RECORD_SIZE);
        assert_eq!(&bytes[24..37], b"ambience.pws\0");
        assert_eq!(WavebankFile::parse(&bytes).expect("parses"), file);
        let bank = Wavebank::parse(&bytes).expect("decodes");
        assert!(bank.clips[0].streaming);
        assert!(bank.clips[0].samples.is_empty());
    }

    #[test]
    fn the_encoder_refuses_what_it_cannot_lay_out() {
        let none = WavebankFile { bank_hash: 1, stream_name: None, records: vec![] };
        assert_eq!(none.to_bytes(), Err(WaveError::Empty));
        let long = WavebankFile {
            bank_hash: 1,
            stream_name: Some("sixteen_chars.pw".to_string()),
            records: vec![WaveRecord {
                clip_hash: 1,
                channels: 1,
                format: CODEC_STREAM,
                sample_rate: 1,
                frames: 1,
                data: WaveData::Streamed { offset: 0, size: 1, word_1c: 0 },
            }],
        };
        assert!(matches!(long.to_bytes(), Err(WaveError::StreamNameTooLong(_))));
        let mixed = WavebankFile {
            bank_hash: 1,
            stream_name: Some("x.pws".to_string()),
            records: vec![embedded(1, 1, &[0])],
        };
        assert_eq!(mixed.to_bytes(), Err(WaveError::DataKindMismatch { record: 0 }));
        let mut wrong = embedded(1, 1, &[0, 1]);
        wrong.frames = 5;
        let bad = WavebankFile { bank_hash: 1, stream_name: None, records: vec![wrong] };
        assert!(matches!(bad.to_bytes(), Err(WaveError::SizeMismatch { .. })));
    }
}
