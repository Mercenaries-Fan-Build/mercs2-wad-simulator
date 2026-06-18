//! Engine-accurate wavebank consumption (LoadWaveBank path).

use std::path::Path;

use crate::audio::ima::{validate_ima_payload, DecodeError};
use mercs2_formats::safe_slice::{AccessViolation, SafeSlice};

pub const RECORD_SIZE: usize = 36;
pub const HEADER_SIZE: usize = 24;
pub const CODEC_IMA: u8 = 0x02;
pub const CODEC_XMA: u8 = 0x01;
pub const CODEC_XBOX_ADPCM: u8 = 0x05;
pub const CODEC_XMA2: u8 = 0x69;

#[derive(Debug, Clone)]
pub struct WaveClip {
    pub clip_hash: u32,
    pub channels: u8,
    pub codec: u8,
    pub sample_rate: u32,
    pub data_offset: u32,
    pub data_size: u32,
    pub external_stream: bool,
    pub decoded_samples: Option<usize>,
}

#[derive(Debug)]
pub struct LoadedWavebank {
    pub self_hash: u32,
    pub clips: Vec<WaveClip>,
    pub issues: Vec<String>,
    pub streaming_clip_count: usize,
}

#[derive(Debug)]
pub enum ConsumeError {
    Access(AccessViolation),
    Decode { clip_index: usize, detail: String },
}

impl From<AccessViolation> for ConsumeError {
    fn from(v: AccessViolation) -> Self {
        ConsumeError::Access(v)
    }
}

#[derive(Clone, Copy)]
pub struct WavebankConsumeOptions<'a> {
    /// Directory containing external `.pws` streaming files (e.g. `Data/Audios`).
    pub audios_dir: Option<&'a Path>,
    /// Clip hash → `.pws` filename (from dlc_audio_manifest.json).
    pub clip_pws_map: Option<&'a std::collections::HashMap<u32, String>>,
}

impl Default for WavebankConsumeOptions<'_> {
    fn default() -> Self {
        Self {
            audios_dir: None,
            clip_pws_map: None,
        }
    }
}

pub fn consume_wavebank(body: &SafeSlice) -> Result<LoadedWavebank, ConsumeError> {
    consume_wavebank_with_options(body, WavebankConsumeOptions::default())
}

pub fn consume_wavebank_with_options(
    body: &SafeSlice,
    opts: WavebankConsumeOptions<'_>,
) -> Result<LoadedWavebank, ConsumeError> {
    let mut issues = Vec::new();

    let count = body.read_u32_le(0, "count")?;
    let self_hash = body.read_u32_le(4, "self_hash")?;
    let populated_count = body.read_u16_le(8, "populated_count")?;
    let _more_flags = body.read_u16_le(10, "more_flags")?;
    let records_offset = body.read_u32_le(16, "records_offset")? as usize;

    if records_offset < HEADER_SIZE {
        issues.push(format!("records_offset {records_offset} < {HEADER_SIZE}"));
    }
    let records_end = records_offset
        .checked_add(count as usize * RECORD_SIZE)
        .unwrap_or(usize::MAX);
    if records_end > body.len() {
        issues.push(format!(
            "records extend past body: end 0x{records_end:X} > len 0x{:X}",
            body.len()
        ));
    }

    let check_count = if populated_count > 0 && u32::from(populated_count) <= count {
        populated_count as usize
    } else {
        count as usize
    };

    let mut clips = Vec::new();
    let mut streaming_clip_count = 0usize;

    for i in 0..check_count {
        let roff = records_offset + i * RECORD_SIZE;
        let clip_hash = body.read_u32_le(roff, &format!("clip[{i}].hash"))?;
        let channels = body.read_u8(roff + 5, &format!("clip[{i}].channels"))?;
        let channels = if channels == 0 { 1 } else { channels };
        let codec = body.read_u8(roff + 6, &format!("clip[{i}].codec"))?;
        let sample_rate = body.read_u32_le(roff + 8, &format!("clip[{i}].sample_rate"))?;
        let data_offset = body.read_u32_le(roff + 12, &format!("clip[{i}].data_offset"))?;
        let data_size = body.read_u32_le(roff + 16, &format!("clip[{i}].data_size"))?;

        if clip_hash == 0 && sample_rate == 0 && data_size == 0 {
            continue;
        }

        if codec == CODEC_XBOX_ADPCM {
            return Err(ConsumeError::Decode {
                clip_index: i,
                detail: format!(
                    "codec 0x05 (Xbox ADPCM) on PC build for clip_hash=0x{clip_hash:08X}"
                ),
            });
        }
        if codec == CODEC_XMA || codec == CODEC_XMA2 {
            return Err(ConsumeError::Decode {
                clip_index: i,
                detail: format!(
                    "codec 0x{codec:02X} (XMA) on PC build for clip_hash=0x{clip_hash:08X}"
                ),
            });
        }

        // P2-6: Structural checks for embedded clips
        if codec != CODEC_IMA {
            issues.push(format!(
                "clip[{i}] 0x{clip_hash:08X}: unexpected codec 0x{codec:02X} (expected 0x02 IMA ADPCM)"
            ));
        }

        if sample_rate < 8000 || sample_rate > 48000 {
            issues.push(format!(
                "clip[{i}] 0x{clip_hash:08X}: sample_rate {sample_rate} outside [8000, 48000]"
            ));
        }

        if data_size > 0 && codec == CODEC_IMA {
            let block_align = 36 * channels as u32;
            if block_align > 0 && data_size % block_align != 0 {
                issues.push(format!(
                    "clip[{i}] 0x{clip_hash:08X}: IMA data_size {data_size} not aligned to block size {} (36*{channels}ch)",
                    block_align
                ));
            }
        }

        let mut decoded_samples = None;
        let mut external_stream = false;

        if data_size > 0 && data_offset != 0xFFFF_FFFF {
            let off = data_offset as usize;
            let sz = data_size as usize;
            let end = off.saturating_add(sz);

            if end > body.len() {
                // Streaming: audio lives in external .pws, not embedded in wavebank body.
                external_stream = true;
                streaming_clip_count += 1;
                if let Some(dir) = opts.audios_dir {
                    if !validate_streaming_pws_present(
                        dir,
                        clip_hash,
                        end,
                        opts.clip_pws_map,
                    ) {
                        return Err(ConsumeError::Decode {
                            clip_index: i,
                            detail: format!(
                                "streaming clip 0x{clip_hash:08X} needs .pws \
                                 (offset=0x{off:X} size=0x{sz:X} end=0x{end:X}) in {}",
                                dir.display()
                            ),
                        });
                    }
                } else {
                    return Err(ConsumeError::Decode {
                        clip_index: i,
                        detail: format!(
                            "streaming clip 0x{clip_hash:08X} external ref \
                             0x{off:X}+0x{sz:X} but no --audios-dir"
                        ),
                    });
                }
            } else {
                let audio = body.slice(off, sz, &format!("clip[{i}].audio_blob"))?;
                if codec == CODEC_IMA || codec == 0x01 {
                    match validate_ima_payload(audio.as_bytes(), channels) {
                        Ok(n) => decoded_samples = Some(n),
                        Err(DecodeError::BlockTooSmall) => {
                            return Err(ConsumeError::Decode {
                                clip_index: i,
                                detail: "IMA block too small".into(),
                            });
                        }
                        Err(DecodeError::Empty) => {
                            return Err(ConsumeError::Decode {
                                clip_index: i,
                                detail: "empty IMA payload".into(),
                            });
                        }
                        Err(DecodeError::StepIndexOutOfRange) => unreachable!(),
                    }
                }
            }
        }

        clips.push(WaveClip {
            clip_hash,
            channels,
            codec,
            sample_rate,
            data_offset,
            data_size,
            external_stream,
            decoded_samples,
        });
    }

    Ok(LoadedWavebank {
        self_hash,
        clips,
        issues,
        streaming_clip_count,
    })
}

/// Validate streaming clip has a `.pws` large enough to hold data at
/// `data_offset + data_size` (passed as `need_end`).
fn validate_streaming_pws_present(
    dir: &Path,
    clip_hash: u32,
    need_end: usize,
    clip_pws_map: Option<&std::collections::HashMap<u32, String>>,
) -> bool {
    if let Some(map) = clip_pws_map {
        if let Some(name) = map.get(&clip_hash) {
            let path = dir.join(name);
            if path.is_file() {
                if let Ok(meta) = std::fs::metadata(&path) {
                    return meta.len() as usize >= need_end;
                }
            }
        }
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pws") {
            continue;
        }
        if let Ok(meta) = ent.metadata() {
            if meta.len() as usize >= need_end {
                return true;
            }
        }
    }
    false
}

pub fn clip_by_hash<'a>(bank: &'a LoadedWavebank, hash: u32) -> Option<&'a WaveClip> {
    bank.clips.iter().find(|c| c.clip_hash == hash)
}
