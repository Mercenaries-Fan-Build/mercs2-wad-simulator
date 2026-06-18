//! IMA ADPCM decode (PC wavebank codec 0x02).
//! Mono: 36-byte blocks (4B header + 32B nibbles).
//! Stereo: 72-byte blocks (8B dual header + 64B interleaved nibbles).

const INDEX_TABLE: [i32; 16] = [
    -1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8,
];
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60,
    66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371,
    408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878,
    2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845,
    8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086,
    29794, 32767,
];

pub const MONO_BLOCK_SIZE: usize = 36;
pub const STEREO_BLOCK_SIZE: usize = 72;

#[derive(Debug)]
pub enum DecodeError {
    Empty,
    BlockTooSmall,
    /// Reserved — engine clamps step index; we no longer fail on this.
    #[allow(dead_code)]
    StepIndexOutOfRange,
}

fn clamp_step_index(step_index: i32) -> i32 {
    step_index.clamp(0, STEP_TABLE.len() as i32 - 1)
}

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

/// Decode mono IMA ADPCM blob (36-byte blocks: 4B header + 32B nibbles).
/// Step index is clamped to 0..88 like the real engine (MS ADPCM / Xbox ADPCM).
pub fn decode_ima_mono(data: &[u8]) -> Result<Vec<i16>, DecodeError> {
    if data.is_empty() {
        return Err(DecodeError::Empty);
    }
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
    if samples.is_empty() && !data.is_empty() {
        return Err(DecodeError::BlockTooSmall);
    }
    Ok(samples)
}

/// Decode stereo IMA ADPCM (72-byte blocks: 8B dual header + 64B data).
/// MS-IMA stereo layout: 4B left header, 4B right header, then 8 groups of
/// 4 bytes left + 4 bytes right (low nibble first per byte).
pub fn decode_ima_stereo(data: &[u8]) -> Result<(Vec<i16>, Vec<i16>), DecodeError> {
    if data.is_empty() {
        return Err(DecodeError::Empty);
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut offset = 0usize;
    while offset + STEREO_BLOCK_SIZE <= data.len() {
        let l_pred = i32::from(i16::from_le_bytes([data[offset], data[offset + 1]]));
        let mut l_step = clamp_step_index(i32::from(data[offset + 2]));
        let r_pred = i32::from(i16::from_le_bytes([data[offset + 4], data[offset + 5]]));
        let mut r_step = clamp_step_index(i32::from(data[offset + 6]));
        left.push(l_pred as i16);
        right.push(r_pred as i16);
        let mut l_pred_i = l_pred;
        let mut r_pred_i = r_pred;

        for group in 0..8 {
            let base = offset + 8 + group * 8;
            for i in 0..4 {
                let lb = data[base + i];
                for nibble in [lb & 0x0F, lb >> 4] {
                    let (p, s) = decode_nibble(nibble, l_pred_i, l_step);
                    l_pred_i = p;
                    l_step = s;
                    left.push(l_pred_i as i16);
                }
                let rb = data[base + 4 + i];
                for nibble in [rb & 0x0F, rb >> 4] {
                    let (p, s) = decode_nibble(nibble, r_pred_i, r_step);
                    r_pred_i = p;
                    r_step = s;
                    right.push(r_pred_i as i16);
                }
            }
        }
        offset += STEREO_BLOCK_SIZE;
    }
    if left.is_empty() && !data.is_empty() {
        return Err(DecodeError::BlockTooSmall);
    }
    Ok((left, right))
}

/// Validate mono or stereo IMA payload; returns Ok(sample_count) on success.
pub fn validate_ima_payload(data: &[u8], channels: u8) -> Result<usize, DecodeError> {
    if channels <= 1 {
        let samples = decode_ima_mono(data)?;
        Ok(samples.len())
    } else {
        let (l, r) = decode_ima_stereo(data)?;
        Ok(l.len().max(r.len()))
    }
}
