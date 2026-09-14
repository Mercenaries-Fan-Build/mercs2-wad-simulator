//! Xbox/XMA audio → PC IMA ADPCM transcoder, ported from the Python reference
//! (`tools/pws_xbox_to_pc.py` + `ucfx_be_to_le.py::_convert_wavebank_data`).
//!
//! This brings wavebank conversion into the single Rust converter so audio blocks
//! (which route to `byteswap_block_rust`) actually transcode their clips instead of
//! shipping Xbox codecs (XMA 0x01 / Xbox-ADPCM 0x05) that fail to decode on PC.
//! Mono Xbox-ADPCM is a lossless nibble-swap; stereo is full decode→re-encode; XMA
//! is decoded via `ffmpeg` (on PATH) → PCM → IMA. Mirrors the Python byte-for-byte
//! for the deterministic ADPCM/IMA paths (verified by `audio::tests`).

use std::process::Command;

/// Standard IMA ADPCM index adjustment table.
const IMA_INDEX_TABLE: [i32; 16] = [
    -1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8,
];

/// Standard IMA ADPCM step table (89 entries, index 0..=88).
const IMA_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408,
    449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066,
    2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const XBOX_MONO_BLOCK: usize = 36;
const XBOX_STEREO_BLOCK: usize = 72;
const XBOX_HEADER_SIZE: usize = 4; // int16 predictor + u8 step_index + u8 reserved

/// PC IMA target codec byte; Xbox source codecs that must be transcoded.
pub const CODEC_PCM: u8 = 0x00;
pub const CODEC_XMA: u8 = 0x01;
pub const CODEC_IMA_PC: u8 = 0x02;
pub const CODEC_XBOX_ADPCM: u8 = 0x05;
pub const CODEC_XMA2: u8 = 0x69;

#[derive(Debug)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for AudioError {}

fn clamp_i16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

/// Decode one IMA nibble → (new_predictor, new_step_index).
fn decode_nibble(nibble: u8, mut predictor: i32, mut step_index: i32) -> (i32, i32) {
    let step = IMA_STEP_TABLE[step_index as usize];
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
        predictor -= diff;
    } else {
        predictor += diff;
    }
    predictor = clamp_i16(predictor);
    step_index += IMA_INDEX_TABLE[nibble as usize];
    step_index = step_index.clamp(0, 88);
    (predictor, step_index)
}

/// Encode one PCM16 sample → (nibble, new_predictor, new_step_index).
fn encode_sample(sample: i32, predictor: i32, step_index: i32) -> (u8, i32, i32) {
    let step = IMA_STEP_TABLE[step_index as usize];
    let mut diff = sample - predictor;
    let mut nibble: u8 = 0;
    if diff < 0 {
        nibble = 8;
        diff = -diff;
    }
    if diff >= step {
        nibble |= 4;
        diff -= step;
    }
    if diff >= (step >> 1) {
        nibble |= 2;
        diff -= step >> 1;
    }
    if diff >= (step >> 2) {
        nibble |= 1;
    }
    // Reconstruct predictor exactly as the decoder would.
    let (new_predictor, new_step_index) = decode_nibble(nibble, predictor, step_index);
    (nibble, new_predictor, new_step_index)
}

/// Swap high/low nibbles in every byte (Xbox high-first ↔ MS-IMA low-first).
fn swap_nibbles_block(data: &[u8]) -> Vec<u8> {
    data.iter().map(|b| ((b >> 4) & 0x0F) | ((b & 0x0F) << 4)).collect()
}

/// One 36-byte Xbox ADPCM mono block → MS-IMA (lossless nibble swap; 4-byte header kept).
fn transcode_mono_block(block: &[u8]) -> Result<Vec<u8>, AudioError> {
    if block.len() < XBOX_MONO_BLOCK {
        return Err(AudioError(format!(
            "mono ADPCM block undersized: {} bytes (need {XBOX_MONO_BLOCK})",
            block.len()
        )));
    }
    let mut out = Vec::with_capacity(XBOX_MONO_BLOCK);
    out.extend_from_slice(&block[..XBOX_HEADER_SIZE]);
    out.extend_from_slice(&swap_nibbles_block(&block[XBOX_HEADER_SIZE..XBOX_MONO_BLOCK]));
    Ok(out)
}

fn read_i16_le(b: &[u8], off: usize) -> i32 {
    i16::from_le_bytes([b[off], b[off + 1]]) as i32
}

/// Fully decode a 72-byte Xbox ADPCM stereo block → (left, right) PCM16.
fn decode_xbox_stereo_block(block: &[u8]) -> (Vec<i32>, Vec<i32>) {
    let mut l_pred = read_i16_le(block, 0);
    let mut l_step = (block[2] as i32).clamp(0, 88);
    let mut r_pred = read_i16_le(block, 4);
    let mut r_step = (block[6] as i32).clamp(0, 88);
    let mut left = vec![l_pred];
    let mut right = vec![r_pred];
    let data = &block[8..72]; // 64 bytes
    for group in 0..4 {
        let l_start = group * 16;
        for i in 0..8 {
            let byte_val = data[l_start + i];
            let hi = (byte_val >> 4) & 0x0F;
            let lo = byte_val & 0x0F;
            let (p, s) = decode_nibble(hi, l_pred, l_step);
            l_pred = p;
            l_step = s;
            left.push(l_pred);
            let (p, s) = decode_nibble(lo, l_pred, l_step);
            l_pred = p;
            l_step = s;
            left.push(l_pred);
        }
        let r_start = l_start + 8;
        for i in 0..8 {
            let byte_val = data[r_start + i];
            let hi = (byte_val >> 4) & 0x0F;
            let lo = byte_val & 0x0F;
            let (p, s) = decode_nibble(hi, r_pred, r_step);
            r_pred = p;
            r_step = s;
            right.push(r_pred);
            let (p, s) = decode_nibble(lo, r_pred, r_step);
            r_pred = p;
            r_step = s;
            right.push(r_pred);
        }
    }
    left.truncate(65);
    right.truncate(65);
    (left, right)
}

/// Pick the initial step index from the first sample delta (mirrors Python).
fn initial_step_index(samples: &[i32]) -> i32 {
    if samples.len() <= 1 {
        return 0;
    }
    let first_diff = (samples[1] - samples[0]).abs();
    for (i, &step_val) in IMA_STEP_TABLE.iter().enumerate() {
        if step_val >= first_diff {
            return (i as i32 - 1).max(0);
        }
    }
    88
}

/// Encode PCM16 samples → 36-byte MS-IMA mono block.
fn encode_ima_mono_block(samples: &[i32]) -> Vec<u8> {
    if samples.is_empty() {
        return vec![0u8; XBOX_MONO_BLOCK];
    }
    let mut predictor = clamp_i16(samples[0]);
    let mut step_index = initial_step_index(samples);
    let mut out = Vec::with_capacity(XBOX_MONO_BLOCK);
    out.extend_from_slice(&(predictor as i16).to_le_bytes());
    out.push(step_index as u8);
    out.push(0);
    let mut nibbles: Vec<u8> = Vec::with_capacity(64);
    for &s in samples.iter().take(65).skip(1) {
        let (nib, p, si) = encode_sample(s, predictor, step_index);
        predictor = p;
        step_index = si;
        nibbles.push(nib);
    }
    nibbles.resize(64, 0);
    let mut data = vec![0u8; 32];
    for i in 0..32 {
        let lo = nibbles[i * 2];
        let hi = nibbles[i * 2 + 1];
        data[i] = (hi << 4) | (lo & 0x0F);
    }
    out.extend_from_slice(&data);
    out
}

/// Encode stereo PCM16 → 72-byte MS-IMA stereo block.
fn encode_ima_stereo_block(left: &[i32], right: &[i32]) -> Vec<u8> {
    let l_pred0 = left.first().copied().map(clamp_i16).unwrap_or(0);
    let r_pred0 = right.first().copied().map(clamp_i16).unwrap_or(0);
    let mut l_pred = l_pred0;
    let mut r_pred = r_pred0;
    let mut l_step = 0i32;
    let mut r_step = 0i32;

    let mut l_nibbles: Vec<u8> = Vec::with_capacity(64);
    for &s in left.iter().take(65).skip(1) {
        let (nib, p, si) = encode_sample(s, l_pred, l_step);
        l_pred = p;
        l_step = si;
        l_nibbles.push(nib);
    }
    l_nibbles.resize(64, 0);
    let mut r_nibbles: Vec<u8> = Vec::with_capacity(64);
    for &s in right.iter().take(65).skip(1) {
        let (nib, p, si) = encode_sample(s, r_pred, r_step);
        r_pred = p;
        r_step = si;
        r_nibbles.push(nib);
    }
    r_nibbles.resize(64, 0);

    let mut out = Vec::with_capacity(XBOX_STEREO_BLOCK);
    out.extend_from_slice(&(l_pred0 as i16).to_le_bytes());
    out.push(0);
    out.push(0);
    out.extend_from_slice(&(r_pred0 as i16).to_le_bytes());
    out.push(0);
    out.push(0);
    let mut data = vec![0u8; 64];
    for group in 0..8 {
        let l_base = group * 8;
        for i in 0..4 {
            let nib_idx = l_base + i * 2;
            let lo = l_nibbles[nib_idx];
            let hi = l_nibbles[nib_idx + 1];
            data[group * 8 + i] = (hi << 4) | (lo & 0x0F);
        }
        let r_base = group * 8;
        for i in 0..4 {
            let nib_idx = r_base + i * 2;
            let lo = r_nibbles[nib_idx];
            let hi = r_nibbles[nib_idx + 1];
            data[group * 8 + 4 + i] = (hi << 4) | (lo & 0x0F);
        }
    }
    out.extend_from_slice(&data);
    out
}

/// Transcode a raw Xbox-ADPCM stream → PC MS-IMA (mono = nibble-swap, stereo = re-encode).
pub fn transcode_pws_xbox_to_pc(xbox: &[u8], channels: usize) -> Result<Vec<u8>, AudioError> {
    if xbox.is_empty() {
        return Ok(xbox.to_vec());
    }
    let block_size = if channels == 1 { XBOX_MONO_BLOCK } else { XBOX_STEREO_BLOCK };
    let n_blocks = xbox.len() / block_size;
    let remainder = xbox.len() % block_size;
    let mut out = Vec::with_capacity(xbox.len());
    for i in 0..n_blocks {
        let block = &xbox[i * block_size..(i + 1) * block_size];
        if channels == 1 {
            out.extend_from_slice(&transcode_mono_block(block)?);
        } else {
            let (l, r) = decode_xbox_stereo_block(block);
            out.extend_from_slice(&encode_ima_stereo_block(&l, &r));
        }
    }
    if remainder != 0 {
        let mut padded = xbox[n_blocks * block_size..].to_vec();
        padded.resize(block_size, 0);
        if channels == 1 {
            out.extend_from_slice(&transcode_mono_block(&padded)?);
        } else {
            let (l, r) = decode_xbox_stereo_block(&padded);
            out.extend_from_slice(&encode_ima_stereo_block(&l, &r));
        }
    }
    Ok(out)
}

/// Encode interleaved PCM16 → raw MS-IMA block stream.
fn pcm16_to_ima_stream(samples: &[i32], channels: usize) -> Result<Vec<u8>, AudioError> {
    if channels == 1 {
        let mut out = Vec::new();
        let mut start = 0;
        while start < samples.len() {
            let chunk = &samples[start..(start + 65).min(samples.len())];
            if chunk.is_empty() {
                break;
            }
            out.extend_from_slice(&encode_ima_mono_block(chunk));
            start += 65;
        }
        return Ok(out);
    }
    if channels != 2 {
        return Err(AudioError(format!("unsupported channel count {channels}")));
    }
    let left: Vec<i32> = samples.iter().step_by(2).copied().collect();
    let right: Vec<i32> = samples.iter().skip(1).step_by(2).copied().collect();
    let n_frames = left.len().min(right.len());
    let mut out = Vec::new();
    let mut i = 0;
    while i < n_frames {
        let mut l_chunk: Vec<i32> = left[i..(i + 65).min(left.len())].to_vec();
        let mut r_chunk: Vec<i32> = right[i..(i + 65).min(right.len())].to_vec();
        if l_chunk.is_empty() && r_chunk.is_empty() {
            break;
        }
        while l_chunk.len() < 65 {
            l_chunk.push(*l_chunk.last().unwrap_or(&0));
        }
        while r_chunk.len() < 65 {
            r_chunk.push(*r_chunk.last().unwrap_or(&0));
        }
        out.extend_from_slice(&encode_ima_stereo_block(&l_chunk[..65], &r_chunk[..65]));
        i += 65;
    }
    Ok(out)
}

/// Decode XMA via ffmpeg (on PATH) → PCM16 WAV → PC IMA stream.
pub fn transcode_xma_to_pc_ima(xma: &[u8], channels: usize) -> Result<Vec<u8>, AudioError> {
    let dir = std::env::temp_dir().join(format!("mercs2_xma_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| AudioError(e.to_string()))?;
    let inp = dir.join("input.xma");
    let wav_out = dir.join("decoded.wav");
    let _cleanup = scopeguard(&dir);
    std::fs::write(&inp, xma).map_err(|e| AudioError(e.to_string()))?;
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&inp)
        .args(["-ac", &channels.max(1).to_string()])
        .arg(&wav_out)
        .output();
    match status {
        Ok(o) if o.status.success() && wav_out.is_file() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(if o.stderr.is_empty() { &o.stdout } else { &o.stderr });
            return Err(AudioError(format!(
                "ffmpeg XMA decode failed (exit {:?}): {}",
                o.status.code(),
                err.chars().take(500).collect::<String>()
            )));
        }
        Err(e) => {
            return Err(AudioError(format!(
                "XMA payload requires ffmpeg on PATH for decode→IMA transcode ({e})"
            )))
        }
    }
    let wav = std::fs::read(&wav_out).map_err(|e| AudioError(e.to_string()))?;
    let (samples, det_ch) = decode_wav_pcm16(&wav)?;
    pcm16_to_ima_stream(&samples, det_ch)
}

/// Minimal RIFF/WAVE PCM16 reader → (interleaved samples, channels).
fn decode_wav_pcm16(wav: &[u8]) -> Result<(Vec<i32>, usize), AudioError> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err(AudioError("not a RIFF/WAVE file".into()));
    }
    let mut pos = 12;
    let mut channels = 0usize;
    let mut bits = 0u16;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let sz = u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        let body_start = pos + 8;
        let body_end = (body_start + sz).min(wav.len());
        match id {
            b"fmt " if sz >= 16 => {
                channels = u16::from_le_bytes([wav[body_start + 2], wav[body_start + 3]]) as usize;
                bits = u16::from_le_bytes([wav[body_start + 14], wav[body_start + 15]]);
            }
            b"data" => data = Some(&wav[body_start..body_end]),
            _ => {}
        }
        pos = body_start + sz + (sz & 1); // chunks are word-aligned
    }
    let data = data.ok_or_else(|| AudioError("WAV has no data chunk".into()))?;
    if bits != 16 {
        return Err(AudioError(format!("WAV sample width {bits} bits (expected 16)")));
    }
    let channels = channels.max(1);
    let samples: Vec<i32> = data
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as i32)
        .collect();
    Ok((samples, channels))
}

/// Convert one embedded wavebank clip to PC codec 0x02. Returns (pc_bytes, pc_codec).
pub fn normalize_embedded_wavebank_clip(
    clip: &[u8],
    codec: u8,
    channels: usize,
) -> Result<(Vec<u8>, u8), AudioError> {
    let ch = if channels > 0 { channels } else { 1 };
    match codec {
        CODEC_PCM => Ok((clip.to_vec(), CODEC_PCM)),
        CODEC_IMA_PC => Ok((clip.to_vec(), CODEC_IMA_PC)),
        CODEC_XBOX_ADPCM => Ok((transcode_pws_xbox_to_pc(clip, ch)?, CODEC_IMA_PC)),
        CODEC_XMA | CODEC_XMA2 => Ok((transcode_xma_to_pc_ima(clip, ch)?, CODEC_IMA_PC)),
        other => Err(AudioError(format!(
            "no embedded clip transcode for codec 0x{other:02X} ({} bytes)",
            clip.len()
        ))),
    }
}

const WAVEBANK_RECORD_SIZE: usize = 36;
const PC_PWS_HEADER_SIZE: u32 = 4;

fn read_u32_be(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u16_be(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}
fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

struct WbRecord {
    clip_hash: u32,
    fmt_bytes: [u8; 4],
    sample_rate: u32,
    data_offset: u32,
    data_size: u32,
    extra_20_28: [u8; 12],
    field_32: u32,
}

/// Convert a wavebank BODY (Xbox BE) → PC LE, transcoding embedded clips to PC IMA.
/// Faithful port of `ucfx_be_to_le.py::_convert_wavebank_data`. The `field_32`/header
/// fields are endian-swapped; populated clips are transcoded (ADPCM nibble-swap / XMA
/// via ffmpeg); streaming refs keep their offset (+PC header). A clip that still carries
/// an Xbox codec after the attempt raises (loud failure) rather than ship un-decodable.
pub fn convert_wavebank_data(body_be: &[u8]) -> Result<Vec<u8>, AudioError> {
    if body_be.len() < 24 {
        return Err(AudioError(format!(
            "wavebank body too short for header: {} bytes (need 24)",
            body_be.len()
        )));
    }
    let count = read_u32_le(body_be, 0) as usize;
    let self_hash = read_u32_be(body_be, 4);
    let populated_count = read_u16_be(body_be, 8);
    let more_flags = read_u16_be(body_be, 10);
    let self_hash2 = read_u32_be(body_be, 12);
    let xbox_records_offset = read_u32_be(body_be, 16) as usize;

    if count > 10000 {
        return Err(AudioError(format!("wavebank record count implausible: {count}")));
    }
    if xbox_records_offset > body_be.len() {
        return Err(AudioError(format!(
            "wavebank records_offset {xbox_records_offset} exceeds body {}",
            body_be.len()
        )));
    }
    let pop = if populated_count > 0 {
        (populated_count as usize).min(count)
    } else {
        count
    };
    let pc_records_offset: u32 = 24;

    // Parse Xbox records.
    let mut records: Vec<WbRecord> = Vec::with_capacity(count);
    for i in 0..count {
        let roff = xbox_records_offset + i * WAVEBANK_RECORD_SIZE;
        if roff + WAVEBANK_RECORD_SIZE > body_be.len() {
            break;
        }
        let mut fmt_bytes = [0u8; 4];
        fmt_bytes.copy_from_slice(&body_be[roff + 4..roff + 8]);
        let mut extra = [0u8; 12];
        extra.copy_from_slice(&body_be[roff + 20..roff + 32]);
        records.push(WbRecord {
            clip_hash: read_u32_be(body_be, roff),
            fmt_bytes,
            sample_rate: read_u32_be(body_be, roff + 8),
            data_offset: read_u32_be(body_be, roff + 12),
            data_size: read_u32_be(body_be, roff + 16),
            extra_20_28: extra,
            field_32: read_u32_be(body_be, roff + 32),
        });
    }

    // Transcode populated clips, sorted by Xbox offset.
    let mut order: Vec<usize> = (0..records.len().min(pop))
        .filter(|&i| records[i].data_size > 0)
        .collect();
    order.sort_by_key(|&i| records[i].data_offset);

    let mut pc_audio_blob: Vec<u8> = Vec::new();
    let pc_audio_start = pc_records_offset as usize + count * WAVEBANK_RECORD_SIZE;
    // index -> (pc_offset, pc_size)
    let mut new_offsets: std::collections::HashMap<usize, (u32, u32)> =
        std::collections::HashMap::new();
    let mut codec_rewrite: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for &idx in &order {
        let xbox_off = records[idx].data_offset as usize;
        let xbox_sz = records[idx].data_size as usize;
        if xbox_off + xbox_sz > body_be.len() {
            // Streaming reference — audio lives in external PWS; adjust for PC header.
            new_offsets.insert(idx, (records[idx].data_offset + PC_PWS_HEADER_SIZE, records[idx].data_size));
            codec_rewrite.insert(idx);
            continue;
        }
        let xbox_clip = &body_be[xbox_off..xbox_off + xbox_sz];
        let channels = if records[idx].fmt_bytes[1] > 0 {
            records[idx].fmt_bytes[1] as usize
        } else {
            1
        };
        let codec = records[idx].fmt_bytes[2];
        let (pc_clip, _new_codec) = normalize_embedded_wavebank_clip(xbox_clip, codec, channels)?;
        let pc_offset = pc_audio_start + pc_audio_blob.len();
        new_offsets.insert(idx, (pc_offset as u32, pc_clip.len() as u32));
        codec_rewrite.insert(idx);
        pc_audio_blob.extend_from_slice(&pc_clip);
    }

    // Build PC output.
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(count as u32).to_le_bytes());
    out.extend_from_slice(&self_hash.to_le_bytes());
    out.extend_from_slice(&populated_count.to_le_bytes());
    out.extend_from_slice(&more_flags.to_le_bytes());
    out.extend_from_slice(&self_hash2.to_le_bytes());
    out.extend_from_slice(&pc_records_offset.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);

    for (i, rec) in records.iter().enumerate() {
        out.extend_from_slice(&rec.clip_hash.to_le_bytes());
        let mut pc_fmt = rec.fmt_bytes;
        if codec_rewrite.contains(&i) && matches!(pc_fmt[2], CODEC_XBOX_ADPCM | CODEC_XMA | CODEC_XMA2) {
            pc_fmt[2] = CODEC_IMA_PC;
        }
        // Loud-fail backstop: any clip still carrying an Xbox-only codec after the
        // conversion attempt is not PC-decodable.
        if matches!(pc_fmt[2], CODEC_XBOX_ADPCM | CODEC_XMA | CODEC_XMA2) {
            return Err(AudioError(format!(
                "wavebank clip[{i}] hash=0x{:08X} retains Xbox codec 0x{:02X} after conversion \
                 (data_size={}, populated={}) — not PC-decodable",
                rec.clip_hash,
                pc_fmt[2],
                rec.data_size,
                codec_rewrite.contains(&i)
            )));
        }
        out.extend_from_slice(&pc_fmt);
        out.extend_from_slice(&rec.sample_rate.to_le_bytes());
        let (off, sz) = new_offsets.get(&i).copied().unwrap_or((0, 0));
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&sz.to_le_bytes());
        // extra_20_28: 3×u32, swap BE→LE.
        for j in (0..12).step_by(4) {
            out.extend_from_slice(&read_u32_be(&rec.extra_20_28, j).to_le_bytes());
        }
        out.extend_from_slice(&rec.field_32.to_le_bytes());
    }
    out.extend_from_slice(&pc_audio_blob);
    Ok(out)
}

/// RAII temp-dir cleanup.
struct ScopeGuard(std::path::PathBuf);
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scopeguard(p: &std::path::Path) -> ScopeGuard {
    ScopeGuard(p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_block_is_lossless_nibble_swap() {
        // Header preserved; data nibbles swapped (Xbox high-first ↔ MS-IMA low-first).
        let mut block = vec![0x34u8, 0x12, 0x05, 0x00]; // predictor=0x1234, step=5
        for i in 0..32 {
            block.push((i as u8) << 4 | 0x0A); // hi=i, lo=0xA
        }
        let out = transcode_mono_block(&block).unwrap();
        assert_eq!(&out[..4], &block[..4]); // header unchanged
        for i in 0..32 {
            // swapped: hi<->lo
            assert_eq!(out[4 + i], (0x0A << 4) | (i as u8 & 0x0F));
        }
        // Double swap returns the original data area.
        let back = transcode_mono_block(&out).unwrap();
        assert_eq!(back, block);
    }

    #[test]
    fn ima_roundtrip_tracks_signal() {
        // Encode a ramp then decode; IMA is lossy but should track a smooth signal.
        let samples: Vec<i32> = (0..64).map(|i| (i as i32 - 32) * 200).collect();
        let block = encode_ima_mono_block(&samples);
        assert_eq!(block.len(), XBOX_MONO_BLOCK);
        // Decode it back via the standard IMA path.
        let mut predictor = read_i16_le(&block, 0);
        let mut step = block[2] as i32;
        let mut decoded = vec![predictor];
        for &byte in &block[4..36] {
            let lo = byte & 0x0F;
            let hi = (byte >> 4) & 0x0F;
            let (p, s) = decode_nibble(lo, predictor, step);
            predictor = p;
            step = s;
            decoded.push(predictor);
            let (p, s) = decode_nibble(hi, predictor, step);
            predictor = p;
            step = s;
            decoded.push(predictor);
        }
        // Mean abs error should be small relative to the signal range (~12800).
        let err: i64 = samples
            .iter()
            .skip(1)
            .zip(decoded.iter().skip(1))
            .map(|(a, b)| (a - b).unsigned_abs() as i64)
            .sum();
        let mae = err / (samples.len() as i64 - 1);
        assert!(mae < 1500, "IMA mean-abs-error too high: {mae}");
    }

    #[test]
    fn step_table_has_89_entries() {
        assert_eq!(IMA_STEP_TABLE.len(), 89);
        assert_eq!(IMA_STEP_TABLE[88], 32767);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// Byte-exact parity with the Python reference (`pws_xbox_to_pc.transcode_pws_xbox_to_pc`)
    /// for the deterministic ADPCM paths — goldens captured from the real Python.
    #[test]
    fn xbox_adpcm_matches_python_byte_exact() {
        // (input_hex, output_hex, channels)
        let cases = [
            // MONO 36B → nibble-swap
            ("3412050000070e151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d9",
             "341205000070e051c132a21383f364d445b526960777e758c839a91a8afa6bdb4cbc2d9d", 1),
            // STEREO 72B → full decode/re-encode
            ("10270900f0d80c0005121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f3000d1a2734414e5b6875828f9ca9b6c3d0ddeaf704111e2b38",
             "10270000f0d800007134f1d2f7977970935426061aeabb8c5d2efddfd3a475469051420217e7b809492bfbcc88a273070c783b0000f0b4840508f80aad80780288703c0c00f0b283", 2),
            // MONO 2-block stream (72B) → nibble-swap per block
            ("3412050000070e151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d900802c00000306090c0f1215181b1e2124272a2d303336393c3f4245484b4e5154575a5d",
             "341205000070e051c132a21383f364d445b526960777e758c839a91a8afa6bdb4cbc2d9d00802c0000306090c0f0215181b1e1124272a2d203336393c3f3245484b4e4154575a5d5", 1),
        ];
        for (i, (inp, out, ch)) in cases.iter().enumerate() {
            let got = transcode_pws_xbox_to_pc(&hex(inp), *ch).unwrap();
            assert_eq!(got, hex(out), "case {i} (ch={ch}) diverged from Python golden");
        }
    }

    /// Byte-exact parity with `ucfx_be_to_le.py::_convert_wavebank_data` for a 2-clip
    /// (mono + stereo ADPCM) wavebank — header swap, record swap, clip transcode, and
    /// offset recompute. Golden captured from the real Python.
    #[test]
    fn wavebank_matches_python_byte_exact() {
        let inp = "02000000aabbccdd000200001122334400000018000000000000111100010500000056220000006000000024000000000000000000000000000000000000222200020500000056220000008400000048000000000000000000000000000000000010050000070e151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d910270900f0d80c00000d1a2734414e5b6875828f9ca9b6c3d0ddeaf704111e2b3845525f6c798693a0adbac7d4e1eefb0815222f3c495663707d8a97a4b1becbd8e5f2ff0c192633";
        let out = "02000000ddccbbaa02000000443322111800000000000000111100000001020022560000600000002400000000000000000000000000000000000000222200000002020022560000840000004800000000000000000000000000000000000000001005000070e051c132a21383f364d445b526960777e758c839a91a8afa6bdb4cbc2d9d10270000f0d8000021f1b073577721f83416f3b5ca9a5c3c0cceaf7e835425f55011d0b2c69768390adbbb7c805102f04d1e8e80c393010027e0a860bf6c2d8a4a1bab8080a27302";
        let got = convert_wavebank_data(&hex(inp)).unwrap();
        assert_eq!(got, hex(out), "wavebank conversion diverged from Python golden");
    }
}
