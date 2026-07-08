//! Wavebank decode — turn a `LoadWaveBank` body into resident PCM clips for the mixer.
//!
//! **Oracle (audio_code_map.md §4.3, §7):** a `wavebank` asset (`0xF753F6D0`, `Sound.LoadWaveBank`
//! `FUN_005e26b0`) is a 24-byte header + N × 36-byte clip records, each carrying `clip_hash`, a 4-byte
//! format field (channels @+1, codec @+2), `sample_rate`, and an embedded `(data_offset, data_size)`
//! blob. Embedded clips are **IMA ADPCM** (codec `0x02`) or raw **PCM** (`0x00`); codec `0x04` clips are
//! streamed from an external `.pws` and carry no embedded samples.
//!
//! This is the **engine-crate home** of the IMA decoder + wavebank record parser that was proven
//! against retail in `crates/wad_simulator/src/audio/{ima,wavebank}.rs` (verified on `ui_hud` and the
//! streaming bank `0x7871F925`). It is ported here — rather than depended on — because that decoder
//! lives in the *tooling* binary crate, which the `mercs2_audio` engine crate cannot pull in. The
//! decode math is identical (same INDEX/STEP tables, same block layout), so both stay byte-faithful.
//!
//! The [`AudioEngine`](crate::AudioEngine) calls [`Wavebank::parse`] on a bank body and holds the
//! decoded clips resident; [`crate::AudioEngine::cue_sound`] then binds a clip's samples to the voice as
//! a [`PcmSource`](crate::mixer::PcmSource) so the cue is actually audible.

/// One 36-byte clip record's fixed size (`FUN_00603110` record stride).
pub const RECORD_SIZE: usize = 36;
/// Wavebank body header size (count / self_hash / populated / records_offset).
pub const HEADER_SIZE: usize = 24;

/// Codec `0x02` — IMA ADPCM, embedded (the PC build's embedded codec).
pub const CODEC_IMA: u8 = 0x02;
/// Codec `0x00` — raw signed-16 PCM, embedded.
pub const CODEC_PCM: u8 = 0x00;
/// Codec `0x04` — streamed: samples live in an external `.pws`, not in the bank body.
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
    /// m2 clip hash — how a cue's wave is looked up.
    pub clip_hash: u32,
    /// Channel count (1 mono / 2 stereo).
    pub channels: u8,
    /// The clip's native sample rate; resampled to the mixer rate at play time.
    pub sample_rate: u32,
    /// Interleaved int16 PCM (empty when the clip streams from an external `.pws`).
    pub samples: Vec<i16>,
    /// True when the clip's audio is external (codec `0x04`) — no embedded samples decoded.
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

/// A parsed wavebank: its self-hash + the index-ordered clips (the order a cue's `wave_index` indexes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wavebank {
    /// The bank's own hash (`+4` in the body).
    pub self_hash: u32,
    /// Clips in record order (index = the cue record's `wave_index`).
    pub clips: Vec<DecodedClip>,
}

impl Wavebank {
    /// Parse a decompressed wavebank body (`FUN_00603110` layout), decoding every embedded IMA/PCM clip.
    /// Streaming (codec `0x04`) and Xbox-only codecs yield a clip record with empty samples (faithful:
    /// the slot exists, the audio is elsewhere), never a hard error — a partial bank is still useful.
    pub fn parse(body: &[u8]) -> Wavebank {
        let mut bank = Wavebank::default();
        if body.len() < HEADER_SIZE {
            return bank;
        }
        let count = rd_u32(body, 0) as usize;
        bank.self_hash = rd_u32(body, 4);
        let populated = rd_u16(body, 8) as usize;
        let records_off = rd_u32(body, 16) as usize;

        // `count` @0 is capacity; `populated` @8 is how many records are actually present (retail
        // streaming banks ship fewer than capacity). Read the smaller so we never run off the body.
        let n = if populated > 0 && populated <= count { populated } else { count };

        for i in 0..n {
            let roff = records_off + i * RECORD_SIZE;
            if roff + RECORD_SIZE > body.len() {
                break;
            }
            let clip_hash = rd_u32(body, roff);
            // 4-byte format field at +4: [?, channels, codec, ?].
            let channels = {
                let c = body[roff + 5];
                if c == 0 { 1 } else { c }
            };
            let codec = body[roff + 6];
            let sample_rate = rd_u32(body, roff + 8);
            let data_offset = rd_u32(body, roff + 12) as usize;
            let data_size = rd_u32(body, roff + 16) as usize;

            // Empty padding record.
            if clip_hash == 0 && sample_rate == 0 && data_size == 0 {
                continue;
            }

            let mut samples = Vec::new();
            let mut streaming = false;
            let end = data_offset.saturating_add(data_size);
            let embedded = data_size > 0 && data_offset != 0xFFFF_FFFF && end <= body.len();

            if embedded {
                let blob = &body[data_offset..end];
                match codec {
                    CODEC_IMA => {
                        samples = if channels >= 2 {
                            decode_ima_stereo(blob)
                        } else {
                            decode_ima_mono(blob)
                        };
                    }
                    CODEC_PCM => {
                        // Raw interleaved int16 LE.
                        samples = blob
                            .chunks_exact(2)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]))
                            .collect();
                    }
                    // XMA / Xbox-ADPCM are not on the PC decode path; leave samples empty (slot present).
                    _ => {}
                }
            } else if data_size > 0 {
                // Audio is external (streamed .pws) — the slot exists, samples are not embedded.
                streaming = codec == CODEC_STREAM || true;
            }

            bank.clips.push(DecodedClip {
                clip_hash,
                channels,
                sample_rate,
                samples,
                streaming,
            });
        }
        bank
    }

    /// Find a resident clip by its hash.
    pub fn clip_by_hash(&self, hash: u32) -> Option<&DecodedClip> {
        self.clips.iter().find(|c| c.clip_hash == hash)
    }
}

// --- little-endian readers (wavebank bodies are LE on PC) --------------------------------------------
#[inline]
fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
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

    #[test]
    fn wavebank_parses_and_decodes_an_embedded_ima_clip() {
        // One mono IMA clip laid out as: [24B header][1 record][audio blob].
        let audio = mono_block(500);
        let records_off = HEADER_SIZE;
        let data_off = records_off + RECORD_SIZE;

        let mut body = vec![0u8; data_off + audio.len()];
        body[0..4].copy_from_slice(&1u32.to_le_bytes()); // count (capacity)
        body[4..8].copy_from_slice(&0xABCD_1234u32.to_le_bytes()); // self_hash
        body[8..10].copy_from_slice(&1u16.to_le_bytes()); // populated
        body[16..20].copy_from_slice(&(records_off as u32).to_le_bytes()); // records_offset

        let roff = records_off;
        body[roff..roff + 4].copy_from_slice(&0x5FBA_3915u32.to_le_bytes()); // clip_hash
        body[roff + 5] = 1; // channels
        body[roff + 6] = CODEC_IMA; // codec
        body[roff + 8..roff + 12].copy_from_slice(&22050u32.to_le_bytes()); // sample_rate
        body[roff + 12..roff + 16].copy_from_slice(&(data_off as u32).to_le_bytes()); // data_offset
        body[roff + 16..roff + 20].copy_from_slice(&(audio.len() as u32).to_le_bytes()); // data_size
        body[data_off..].copy_from_slice(&audio);

        let bank = Wavebank::parse(&body);
        assert_eq!(bank.self_hash, 0xABCD_1234);
        assert_eq!(bank.clips.len(), 1);
        let clip = &bank.clips[0];
        assert_eq!(clip.clip_hash, 0x5FBA_3915);
        assert_eq!(clip.channels, 1);
        assert_eq!(clip.sample_rate, 22050);
        assert!(!clip.streaming);
        assert_eq!(clip.frames(), 65, "65 mono samples decoded");
        assert_eq!(clip.samples[0], 500);
        assert!(bank.clip_by_hash(0x5FBA_3915).is_some());
    }

    #[test]
    fn streaming_clip_has_no_embedded_samples() {
        // A record whose (offset,size) points outside the body → external stream, empty samples.
        let records_off = HEADER_SIZE;
        let mut body = vec![0u8; records_off + RECORD_SIZE];
        body[0..4].copy_from_slice(&1u32.to_le_bytes());
        body[8..10].copy_from_slice(&1u16.to_le_bytes());
        body[16..20].copy_from_slice(&(records_off as u32).to_le_bytes());
        let roff = records_off;
        body[roff..roff + 4].copy_from_slice(&0x1111_2222u32.to_le_bytes());
        body[roff + 5] = 2; // stereo
        body[roff + 6] = CODEC_STREAM;
        body[roff + 8..roff + 12].copy_from_slice(&44100u32.to_le_bytes());
        body[roff + 12..roff + 16].copy_from_slice(&0x0010_0000u32.to_le_bytes()); // far offset
        body[roff + 16..roff + 20].copy_from_slice(&0x0020_0000u32.to_le_bytes()); // big size

        let bank = Wavebank::parse(&body);
        assert_eq!(bank.clips.len(), 1);
        assert!(bank.clips[0].streaming);
        assert!(bank.clips[0].samples.is_empty());
    }
}
