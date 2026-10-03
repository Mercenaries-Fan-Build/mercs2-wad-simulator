//! Xbox 360 wavebank → PC wavebank transcoder.
//!
//! See `docs/_xbox_wavebank_container.md` for the Xbox body layout and
//! `mercs2_audio::wave` for the PC embedded wavebank layout. Embedded PC
//! clips are interleaved little-endian PCM16 with `format = 0x02` and
//! `data_size = frames × channels × 2`; XMA/XMA2 are decoded by `ffmpeg`,
//! Xbox-PCM is a BE→LE byte swap.

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

pub const TABLE_VERSION: u32 = 0x1D;

pub const CODEC_PCM: u8 = 0x00;
pub const CODEC_XMA: u8 = 0x01;
pub const CODEC_XMA2: u8 = 0x05;

/// PC embedded wavebank record `+0x06` byte: 2 bytes per PCM16 sample.
pub const PC_FORMAT_PCM16: u8 = 0x02;
/// PC streamed wavebank record `+0x06` byte.
pub const PC_FORMAT_STREAM: u8 = 0x04;

/// Embedded (`+0x0A` = 0) wavebank header stride — same on Xbox and PC.
const HEADER_SIZE: usize = 24;
/// Streamed (`+0x0A` = 1) wavebank adds a 16-byte NUL-padded `.pws` name
/// field at `+0x18`, so the record table starts at `+0x28`.
const STREAM_NAME_FIELD: usize = 16;
const STREAM_HEADER_SIZE: usize = HEADER_SIZE + STREAM_NAME_FIELD;

/// Every record is 36 bytes (same stride on Xbox and PC).
const WAVEBANK_RECORD_SIZE: usize = 36;

/// Xbox XMA2 packets are 2048 bytes. The engine aligns the first embedded blob
/// to this boundary (proven for all 93 embedded wavebanks — see
/// `docs/_phase3a_inferred_claims_verified.md` §A.3).
const XMA2_PACKET: usize = 0x800;

/// PC wavebank blobs must start on, and the body must end on, a 16-byte
/// boundary (`mercs2_audio::wave::BLOB_ALIGN`).
const PC_BLOB_ALIGN: usize = 16;

/// `.pws` file carries a 4-byte header the PC path pins the first clip past.
const PC_PWS_HEADER_SIZE: u32 = 4;

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

/// One 36-byte Xbox ADPCM mono block → MS-IMA (lossless nibble swap; 4-byte
/// header kept). NOTE: Xbox-ADPCM is NOT observed in retail Xbox wavebanks
/// (every record carries codec `0x05 = XMA2`; see
/// `docs/_phase3a_inferred_claims_verified.md` §A.2). This helper is kept
/// because the Python-golden parity test still exercises it, and because the
/// VO-stream path (`crate::pws`) may still produce Xbox ADPCM bitstreams.
pub fn transcode_mono_block(block: &[u8]) -> Result<Vec<u8>, AudioError> {
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
/// Same provenance / status note as [`transcode_mono_block`].
pub fn decode_xbox_stereo_block(block: &[u8]) -> (Vec<i32>, Vec<i32>) {
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

#[cfg(test)]
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

#[cfg(test)]
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

/// Transcode a raw Xbox-ADPCM stream → PC MS-IMA (mono = nibble-swap, stereo
/// = re-encode). Still exported for the PWS/VO path; **not** called from the
/// wavebank path any more (retail wavebanks don't carry Xbox-ADPCM — see
/// Phase 3.A sweep).
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

/// Resolve the `ffmpeg` executable to use for XMA/XMA2 decode.
///
/// Resolution order (first hit wins):
/// 1. `MERCS2_FFMPEG` env var (absolute path to the binary).
/// 2. Walk up from the current executable looking for
///    `tools/ffmpeg/bin/ffmpeg(.exe)` — the repo-local bundle at
///    `tools/ffmpeg/` ships ffmpeg 2026-06 (gyan.dev essentials build)
///    with native `xma1`/`xma2` decoders enabled.
/// 3. `tools/ffmpeg/bin/ffmpeg(.exe)` resolved relative to the current
///    working directory (useful under `cargo test`, which runs from the
///    package root).
/// 4. `ffmpeg` on `$PATH`.
///
/// Fails loudly (`AudioError`) if none of the above resolve.
pub fn find_ffmpeg() -> Result<std::path::PathBuf, AudioError> {
    use std::path::{Path, PathBuf};
    let exe_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };

    if let Ok(explicit) = std::env::var("MERCS2_FFMPEG") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(AudioError(format!(
            "MERCS2_FFMPEG points at '{explicit}' but that is not a file"
        )));
    }

    let bundle = Path::new("tools").join("ffmpeg").join("bin").join(exe_name);
    if let Ok(mut cur) = std::env::current_exe() {
        while cur.pop() {
            let cand = cur.join(&bundle);
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }
    if let Ok(mut cur) = std::env::current_dir() {
        loop {
            let cand = cur.join(&bundle);
            if cand.is_file() {
                return Ok(cand);
            }
            if !cur.pop() {
                break;
            }
        }
    }

    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path.split(sep) {
            let cand = Path::new(dir).join(exe_name);
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }

    Err(AudioError(
        "ffmpeg not found. Set MERCS2_FFMPEG to the ffmpeg binary, place it under \
         tools/ffmpeg/bin/, or put it on PATH. The repo bundles one at \
         tools/ffmpeg/bin/ffmpeg.exe (gyan.dev essentials build with xma1/xma2 \
         decoders)."
            .into(),
    ))
}

/// Decode XMA (codec `0x01`, carries its own RIFF/XMA wrapper) via ffmpeg
/// to interleaved little-endian PCM16 bytes. The returned length is exactly
/// `samples × 2` (ffmpeg's actual decoded count; the caller re-pads to the
/// Xbox record's `decoded_samples × channels × 2` before writing).
pub fn transcode_xma_to_pcm16(xma: &[u8], channels: usize) -> Result<Vec<u8>, AudioError> {
    ffmpeg_decode_to_pcm16(xma, channels, "input.xma")
}

/// Wrap a raw XMA2 bytestream (packed 2048-byte packets, no RIFF — the shape
/// retail Xbox wavebanks carry; see `docs/_xbox_wavebank_container.md` §6
/// and §8) in a RIFF / `XMA2WAVEFORMATEX` container that ffmpeg accepts.
///
/// Field choices follow the recipe in `_xbox_wavebank_container.md` §8:
/// `wFormatTag = 0x0166`, `nBlockAlign = 2048`, `cbSize = 34` with the
/// 34-byte `XMA2WAVEFORMATEX` tail (`NumStreams = 1`, `ChannelMask` =
/// `SPEAKER_FRONT_CENTER` for mono / `FL|FR` for stereo, `SamplesEncoded =
/// decoded_samples`, `BytesPerBlock = 0x10000`, `PlayBegin = 0`,
/// `PlayLength = decoded_samples`, `LoopBegin/Length/Count = 0`,
/// `EncoderVersion = 4`, `BlockCount = ceil(size / 0x10000)`).
pub fn wrap_xma2_raw_as_riff(
    xma2_raw: &[u8],
    channels: u16,
    sample_rate: u32,
    decoded_samples_per_channel: u32,
) -> Vec<u8> {
    const WAVE_FORMAT_XMA2: u16 = 0x0166;
    const XMA2_BYTES_PER_BLOCK: u32 = 0x10000;
    const WAVEFORMATEX_SIZE: u32 = 18;
    const XMA2_EXT_TAIL: u32 = 34;
    const FMT_BODY: u32 = WAVEFORMATEX_SIZE + XMA2_EXT_TAIL; // 52

    let data_sz = xma2_raw.len() as u32;
    let channel_mask: u32 = if channels == 1 { 0x0000_0004 } else { 0x0000_0003 };
    let block_count: u32 =
        ((data_sz + XMA2_BYTES_PER_BLOCK - 1) / XMA2_BYTES_PER_BLOCK).max(1);
    // RIFF size = "WAVE" (4) + "fmt " + sz + body + "data" + sz + data
    let riff_sz: u32 = 4 + 8 + FMT_BODY + 8 + data_sz;

    let mut out = Vec::with_capacity((riff_sz as usize) + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_sz.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&FMT_BODY.to_le_bytes());

    // WAVEFORMATEX (18 bytes)
    out.extend_from_slice(&WAVE_FORMAT_XMA2.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    // nAvgBytesPerSec: XMA2 is VBR; the field is advisory. A PCM-equivalent
    // upper bound (ch * sr * 2) is accepted by ffmpeg.
    let nabps = sample_rate
        .saturating_mul(channels as u32)
        .saturating_mul(2);
    out.extend_from_slice(&nabps.to_le_bytes());
    out.extend_from_slice(&(XMA2_PACKET as u16).to_le_bytes()); // nBlockAlign = 2048
    out.extend_from_slice(&16u16.to_le_bytes()); // wBitsPerSample
    out.extend_from_slice(&(XMA2_EXT_TAIL as u16).to_le_bytes()); // cbSize = 34

    // XMA2WAVEFORMATEX tail (34 bytes): 2+4+4+4+4+4+4+4+1+1+2 = 34
    out.extend_from_slice(&1u16.to_le_bytes()); // NumStreams
    out.extend_from_slice(&channel_mask.to_le_bytes()); // ChannelMask
    out.extend_from_slice(&decoded_samples_per_channel.to_le_bytes()); // SamplesEncoded
    out.extend_from_slice(&XMA2_BYTES_PER_BLOCK.to_le_bytes()); // BytesPerBlock
    out.extend_from_slice(&0u32.to_le_bytes()); // PlayBegin
    out.extend_from_slice(&decoded_samples_per_channel.to_le_bytes()); // PlayLength
    out.extend_from_slice(&0u32.to_le_bytes()); // LoopBegin
    out.extend_from_slice(&0u32.to_le_bytes()); // LoopLength
    out.push(0); // LoopCount
    out.push(4); // EncoderVersion
    out.extend_from_slice(&(block_count as u16).to_le_bytes()); // BlockCount

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_sz.to_le_bytes());
    out.extend_from_slice(xma2_raw);
    out
}

/// Decode a raw XMA2 bytestream (as retail Xbox wavebanks ship it) to
/// interleaved little-endian PCM16 bytes.
///
/// Wraps the raw packet stream in a RIFF/`XMA2WAVEFORMATEX` container
/// ([`wrap_xma2_raw_as_riff`]), then invokes ffmpeg's native `xma2` decoder
/// via [`find_ffmpeg`]. Fails loudly if ffmpeg cannot be resolved — never
/// writes a silent-partial clip.
pub fn transcode_xma2_raw_to_pcm16(
    xma2_raw: &[u8],
    channels: usize,
    sample_rate: u32,
    decoded_samples_per_channel: u32,
) -> Result<Vec<u8>, AudioError> {
    let ch16 = u16::try_from(channels)
        .map_err(|_| AudioError(format!("XMA2 channel count {channels} out of range")))?;
    let riff = wrap_xma2_raw_as_riff(xma2_raw, ch16, sample_rate, decoded_samples_per_channel);
    ffmpeg_decode_to_pcm16(&riff, channels, "input.xma")
}

/// Common ffmpeg invocation: write the wrapped bytestream to a temp file,
/// decode to PCM16 LE WAV, return the raw interleaved PCM16 bytes (what
/// the PC embedded wavebank path writes). Shared by XMA (0x01) and XMA2
/// (0x05). Uses [`find_ffmpeg`] to locate the binary.
fn ffmpeg_decode_to_pcm16(bytes: &[u8], channels: usize, in_name: &str) -> Result<Vec<u8>, AudioError> {
    let ffmpeg = find_ffmpeg()?;
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mercs2_xma_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| AudioError(e.to_string()))?;
    let inp = dir.join(in_name);
    let wav_out = dir.join("decoded.wav");
    let _cleanup = scopeguard(&dir);
    std::fs::write(&inp, bytes).map_err(|e| AudioError(e.to_string()))?;
    // Decode → signed-16 PCM LE WAV. `-ac N` down/up-mixes to N channels; we
    // keep the native channel count (ffmpeg already matches what the RIFF
    // header advertised).
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-hide_banner", "-loglevel", "error",
            "-i",
        ])
        .arg(&inp)
        .args([
            "-ac", &channels.max(1).to_string(),
            "-f", "wav",
            "-acodec", "pcm_s16le",
        ])
        .arg(&wav_out)
        .output();
    match status {
        Ok(o) if o.status.success() && wav_out.is_file() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(if o.stderr.is_empty() { &o.stdout } else { &o.stderr });
            return Err(AudioError(format!(
                "ffmpeg XMA decode failed (exit {:?}, ffmpeg={}): {}",
                o.status.code(),
                ffmpeg.display(),
                err.chars().take(500).collect::<String>()
            )));
        }
        Err(e) => {
            return Err(AudioError(format!(
                "ffmpeg at '{}' failed to execute: {e}",
                ffmpeg.display()
            )))
        }
    }
    let wav = std::fs::read(&wav_out).map_err(|e| AudioError(e.to_string()))?;
    let (samples, det_ch) = decode_wav_pcm16(&wav)?;
    if det_ch != channels {
        return Err(AudioError(format!(
            "ffmpeg output had {det_ch} channel(s), expected {channels}"
        )));
    }
    // Interleaved i16 LE bytes.
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let clamped = s.clamp(-32768, 32767) as i16;
        out.extend_from_slice(&clamped.to_le_bytes());
    }
    Ok(out)
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

/// Decode one Xbox embedded wavebank clip to interleaved little-endian PCM16
/// bytes. The caller trims or zero-pads the result to
/// `decoded_samples_per_channel × channels × 2`, which is what the PC
/// wavebank parser enforces at `+0x0C`.
///
/// `sample_rate` and `decoded_samples_per_channel` are forwarded to the
/// `XMA2WAVEFORMATEX` header for the ffmpeg wrapper.
pub fn normalize_embedded_wavebank_clip(
    clip: &[u8],
    codec: u8,
    channels: usize,
    sample_rate: u32,
    decoded_samples_per_channel: u32,
) -> Result<Vec<u8>, AudioError> {
    let ch = if channels > 0 { channels } else { 1 };
    match codec {
        CODEC_PCM => Ok(swap_pcm16_be_to_le(clip)),
        CODEC_XMA => transcode_xma_to_pcm16(clip, ch),
        CODEC_XMA2 => transcode_xma2_raw_to_pcm16(clip, ch, sample_rate, decoded_samples_per_channel),
        other => Err(AudioError(format!(
            "no embedded clip transcode for codec 0x{other:02X} ({} bytes)",
            clip.len()
        ))),
    }
}

/// Byte-swap an interleaved big-endian PCM16 buffer to little-endian in place.
fn swap_pcm16_be_to_le(pcm_be: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm_be.len());
    let mut i = 0;
    while i + 1 < pcm_be.len() {
        out.push(pcm_be[i + 1]);
        out.push(pcm_be[i]);
        i += 2;
    }
    if pcm_be.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// Fit a decoded PCM16 LE buffer to exactly `frames × channels × 2` bytes.
/// ffmpeg's output can over- or under-shoot the record's `decoded_samples`
/// by a tail frame; the PC parser enforces the exact size relation, so we
/// trim overshoot and zero-pad undershoot.
fn pad_or_trim_pcm16(mut pcm_le: Vec<u8>, want_bytes: usize) -> Vec<u8> {
    if pcm_le.len() > want_bytes {
        pcm_le.truncate(want_bytes);
    } else if pcm_le.len() < want_bytes {
        pcm_le.resize(want_bytes, 0);
    }
    pcm_le
}

fn read_u32_be(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u16_be(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}
fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// One 36-byte Xbox wavebank record (BE), per
/// `docs/_xbox_wavebank_container.md` §"Record layout".
///
/// ```text
/// +0x00  u32 BE   clip_hash
/// +0x04  u8       0
/// +0x05  u8       channels (1 or 2)
/// +0x06  u8       codec (0x05 = XMA2 in retail)
/// +0x07  u8       0
/// +0x08  u32 BE   sample_rate
/// +0x0C  u32 BE   data_size
/// +0x10  u32 BE   decoded_samples (per channel)
/// +0x14  8 × u8   zero
/// +0x1C  u32 BE   0 embedded / per-record streamed
/// +0x20  u32 BE   data_offset (record-relative embedded; .pws offset streamed)
/// ```
#[derive(Debug, Clone)]
struct WbRecord {
    clip_hash: u32,
    fmt_bytes: [u8; 4],
    sample_rate: u32,
    data_size: u32,
    decoded_samples: u32,
    word_1c: u32,
    data_offset: u32,
}

impl WbRecord {
    fn channels(&self) -> u8 {
        self.fmt_bytes[1]
    }
    fn codec(&self) -> u8 {
        self.fmt_bytes[2]
    }
    fn read(body: &[u8], rec_abs: usize) -> Result<Self, AudioError> {
        if rec_abs + WAVEBANK_RECORD_SIZE > body.len() {
            return Err(AudioError(format!(
                "wavebank record at +0x{rec_abs:X} runs past the {}-byte body",
                body.len()
            )));
        }
        let mut fmt_bytes = [0u8; 4];
        fmt_bytes.copy_from_slice(&body[rec_abs + 4..rec_abs + 8]);
        Ok(Self {
            clip_hash: read_u32_be(body, rec_abs),
            fmt_bytes,
            sample_rate: read_u32_be(body, rec_abs + 0x08),
            data_size: read_u32_be(body, rec_abs + 0x0C),
            decoded_samples: read_u32_be(body, rec_abs + 0x10),
            word_1c: read_u32_be(body, rec_abs + 0x1C),
            data_offset: read_u32_be(body, rec_abs + 0x20),
        })
    }
}

/// Convert a wavebank body from Xbox BE to the PC LE layout.
///
/// Byte layout per `docs/_xbox_wavebank_container.md` §"Wavebank body":
/// * `+0x00 u32 LE` version (`TABLE_VERSION` = 0x1D).
/// * `+0x04 u32 BE` bank_hash.
/// * `+0x08 u16 BE` record count.
/// * `+0x0A u8` kind flag (0 = embedded, non-zero = streamed; `+0x0B` padding).
/// * `+0x0C u32 BE` bank_hash (repeats `+0x04`).
/// * `+0x10 u32 BE` records_off (24 embedded / 40 streamed).
/// * `+0x14 u32 BE` zero.
/// * `+0x18..+0x27` streamed only: 16-byte NUL-padded `.pws` name.
/// * `+records_off` 36-byte records ([`WbRecord`]).
///
/// Embedded clips are decoded to interleaved LE PCM16 and emitted with
/// `format = PC_FORMAT_PCM16` and `data_size = frames × channels × 2`;
/// streamed records carry their `.pws` offset forward with the PC 4-byte
/// `.pws` header shift.
pub fn convert_wavebank_data(body_be: &[u8]) -> Result<Vec<u8>, AudioError> {
    if body_be.len() < HEADER_SIZE {
        return Err(AudioError(format!(
            "wavebank body too short for header: {} bytes (need {HEADER_SIZE})",
            body_be.len()
        )));
    }
    let version = read_u32_le(body_be, 0);
    if version != TABLE_VERSION {
        return Err(AudioError(format!(
            "wavebank version 0x{version:X}, expected 0x{TABLE_VERSION:X} (TABLE_VERSION)"
        )));
    }
    let bank_hash = read_u32_be(body_be, 4);
    let count = read_u16_be(body_be, 8) as usize;
    let kind_flag = body_be[0x0A];
    let streamed = kind_flag != 0;
    let bank_hash2 = read_u32_be(body_be, 12);
    if bank_hash2 != bank_hash {
        return Err(AudioError(format!(
            "wavebank bank_hash mismatch: +0x04=0x{bank_hash:08X} +0x0C=0x{bank_hash2:08X}"
        )));
    }
    let xbox_records_offset = read_u32_be(body_be, 16) as usize;
    let expected_off = if streamed { STREAM_HEADER_SIZE } else { HEADER_SIZE };
    if xbox_records_offset != expected_off {
        return Err(AudioError(format!(
            "wavebank records_offset {xbox_records_offset}, expected {expected_off} \
             (streamed={streamed})"
        )));
    }
    if count == 0 {
        return Err(AudioError("wavebank has 0 records (empty-bank layout is unmeasured)".into()));
    }
    if count > 10000 {
        return Err(AudioError(format!("wavebank record count implausible: {count}")));
    }
    if xbox_records_offset + count * WAVEBANK_RECORD_SIZE > body_be.len() {
        return Err(AudioError(format!(
            "wavebank records table (count={count} @+0x{xbox_records_offset:X}) runs past the {}-byte body",
            body_be.len()
        )));
    }

    let stream_name: Option<&[u8]> = if streamed {
        if body_be.len() < STREAM_HEADER_SIZE {
            return Err(AudioError("wavebank is streamed but body too short for .pws name".into()));
        }
        Some(&body_be[HEADER_SIZE..STREAM_HEADER_SIZE])
    } else {
        None
    };

    let mut records: Vec<WbRecord> = Vec::with_capacity(count);
    for i in 0..count {
        let rec_abs = xbox_records_offset + i * WAVEBANK_RECORD_SIZE;
        records.push(WbRecord::read(body_be, rec_abs)?);
    }

    let mut order: Vec<usize> = (0..records.len())
        .filter(|&i| records[i].data_size > 0)
        .collect();
    order.sort_by_key(|&i| {
        xbox_records_offset + i * WAVEBANK_RECORD_SIZE + records[i].data_offset as usize
    });

    let pc_records_offset: u32 = if streamed { STREAM_HEADER_SIZE } else { HEADER_SIZE } as u32;
    let pc_table_end: usize = pc_records_offset as usize + count * WAVEBANK_RECORD_SIZE;
    let pc_audio_start: usize = align_up(pc_table_end, PC_BLOB_ALIGN);

    let mut pc_audio_blob: Vec<u8> = Vec::new();
    let mut new_placement: std::collections::HashMap<usize, (u32, u32, u8)> =
        std::collections::HashMap::new();

    if streamed {
        for i in 0..records.len() {
            let r = &records[i];
            if r.data_size == 0 {
                new_placement.insert(i, (0, 0, PC_FORMAT_STREAM));
                continue;
            }
            let pc_off = r.data_offset.saturating_add(PC_PWS_HEADER_SIZE);
            new_placement.insert(i, (pc_off, r.data_size, PC_FORMAT_STREAM));
        }
    } else {
        for &idx in &order {
            let r = &records[idx];
            let xbox_rec_abs = xbox_records_offset + idx * WAVEBANK_RECORD_SIZE;
            let xbox_data_abs = xbox_rec_abs
                .checked_add(r.data_offset as usize)
                .ok_or_else(|| AudioError(format!(
                    "wavebank clip[{idx}] data_offset 0x{:X} overflows", r.data_offset
                )))?;
            let xbox_end = xbox_data_abs
                .checked_add(r.data_size as usize)
                .ok_or_else(|| AudioError(format!(
                    "wavebank clip[{idx}] data_size 0x{:X} overflows", r.data_size
                )))?;
            if xbox_end > body_be.len() {
                return Err(AudioError(format!(
                    "wavebank clip[{idx}] hash=0x{:08X} data range \
                     [0x{xbox_data_abs:X}..0x{xbox_end:X}] runs past the {}-byte body",
                    r.clip_hash,
                    body_be.len()
                )));
            }
            let xbox_clip = &body_be[xbox_data_abs..xbox_end];
            let channels = if r.channels() > 0 { r.channels() as usize } else { 1 };
            let pcm_le = normalize_embedded_wavebank_clip(
                xbox_clip,
                r.codec(),
                channels,
                r.sample_rate,
                r.decoded_samples,
            )?;
            let want_bytes = r.decoded_samples as usize * channels * 2;
            let pc_clip = pad_or_trim_pcm16(pcm_le, want_bytes);

            let pc_write_pos: usize = align_up(pc_audio_start + pc_audio_blob.len(), PC_BLOB_ALIGN);
            while pc_audio_blob.len() + pc_audio_start < pc_write_pos {
                pc_audio_blob.push(0);
            }
            let pc_rec_abs = pc_records_offset as usize + idx * WAVEBANK_RECORD_SIZE;
            let pc_data_offset: u32 = (pc_write_pos as i64 - pc_rec_abs as i64)
                .try_into()
                .map_err(|_| AudioError(format!(
                    "wavebank clip[{idx}] PC data_offset would be negative"
                )))?;
            new_placement.insert(idx, (pc_data_offset, pc_clip.len() as u32, PC_FORMAT_PCM16));
            pc_audio_blob.extend_from_slice(&pc_clip);
        }
    }

    let total_body = if streamed {
        pc_table_end
    } else {
        align_up(pc_audio_start + pc_audio_blob.len(), PC_BLOB_ALIGN)
    };
    let mut out: Vec<u8> = vec![0u8; total_body];

    out[0..4].copy_from_slice(&TABLE_VERSION.to_le_bytes());
    out[4..8].copy_from_slice(&bank_hash.to_le_bytes());
    out[8..10].copy_from_slice(&(count as u16).to_le_bytes());
    let pc_kind: u16 = if streamed { 1 } else { 0 };
    out[10..12].copy_from_slice(&pc_kind.to_le_bytes());
    out[12..16].copy_from_slice(&bank_hash.to_le_bytes());
    out[16..20].copy_from_slice(&pc_records_offset.to_le_bytes());
    out[20..24].copy_from_slice(&0u32.to_le_bytes());

    if let Some(name) = stream_name {
        out[HEADER_SIZE..STREAM_HEADER_SIZE].copy_from_slice(name);
    }

    for (i, rec) in records.iter().enumerate() {
        let pc_r = pc_records_offset as usize + i * WAVEBANK_RECORD_SIZE;
        let default_fmt = if streamed { PC_FORMAT_STREAM } else { PC_FORMAT_PCM16 };
        let (pc_off, pc_size, pc_fmt) =
            new_placement.get(&i).copied().unwrap_or((0, 0, default_fmt));
        out[pc_r..pc_r + 4].copy_from_slice(&rec.clip_hash.to_le_bytes());
        out[pc_r + 4] = 0;
        out[pc_r + 5] = rec.channels();
        out[pc_r + 6] = pc_fmt;
        out[pc_r + 7] = 0;
        out[pc_r + 8..pc_r + 12].copy_from_slice(&rec.sample_rate.to_le_bytes());
        out[pc_r + 12..pc_r + 16].copy_from_slice(&pc_size.to_le_bytes());
        out[pc_r + 16..pc_r + 20].copy_from_slice(&rec.decoded_samples.to_le_bytes());
        out[pc_r + 28..pc_r + 32].copy_from_slice(&rec.word_1c.to_le_bytes());
        out[pc_r + 32..pc_r + 36].copy_from_slice(&pc_off.to_le_bytes());
    }

    if !streamed {
        out[pc_audio_start..pc_audio_start + pc_audio_blob.len()]
            .copy_from_slice(&pc_audio_blob);
    }

    Ok(out)
}

fn align_up(x: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
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

    /// PWS/VO Xbox-ADPCM transcode goldens for `transcode_pws_xbox_to_pc`.
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

    const RETAIL_FIXTURE: &[u8] =
        include_bytes!("../../tests/fixtures/xbox_wavebank_emb_block3322_be.bin");
    const RETAIL_FIXTURE_SHA256: &str =
        "a278e393501d254548a7245c401449574c1c16e8c3b032e0a568bd0eaf34d00a";

    fn assert_fixture_provenance() {
        assert_eq!(sha256_hex(RETAIL_FIXTURE), RETAIL_FIXTURE_SHA256);
        assert_eq!(RETAIL_FIXTURE.len(), 0x8000);
    }

    /// Parse the retail-Xbox `0x9996B5A6` wavebank body (block 3322, 2 XMA2
    /// mono clips, 44100 Hz, 32 KB) and confirm every header + record field
    /// decodes to its known retail value and both XMA2 blobs sit on 2048-byte
    /// boundaries with a valid first-packet header.
    #[test]
    fn retail_wavebank_block_3322_parses() {
        assert_fixture_provenance();
        let b = RETAIL_FIXTURE;

        assert_eq!(read_u32_le(b, 0), TABLE_VERSION);
        assert_eq!(read_u32_be(b, 4), 0x9996_B5A6);
        assert_eq!(read_u16_be(b, 8), 2);
        assert_eq!(b[0x0A], 0);
        assert_eq!(read_u32_be(b, 12), 0x9996_B5A6);
        let records_off = read_u32_be(b, 16) as usize;
        assert_eq!(records_off, 24);

        let expected = [
            (0x8E16_4121u32, 0x4000u32, 55_296u32, 0x7E8u32, 0x800usize),
            (0x0C0E_B8B6u32, 0x3800u32, 51_968u32, 0x47C4u32, 0x4800usize),
        ];
        for (i, (xclip, xsize, xframes, xdoff, xabs)) in expected.iter().enumerate() {
            let rec_abs = records_off + i * WAVEBANK_RECORD_SIZE;
            let rec = WbRecord::read(b, rec_abs).unwrap();
            assert_eq!(rec.clip_hash, *xclip);
            assert_eq!(rec.channels(), 1);
            assert_eq!(rec.codec(), CODEC_XMA2);
            assert_eq!(rec.sample_rate, 44100);
            assert_eq!(rec.data_size, *xsize);
            assert_eq!(rec.decoded_samples, *xframes);
            assert_eq!(rec.word_1c, 0);
            assert_eq!(rec.data_offset, *xdoff);
            let blob_abs = rec_abs + rec.data_offset as usize;
            assert_eq!(blob_abs, *xabs);
            assert!(blob_abs + rec.data_size as usize <= b.len());
            assert_eq!(blob_abs % XMA2_PACKET, 0);

            let hdr = read_u32_be(b, blob_abs);
            let frame_count = (hdr >> 26) & 0x3F;
            let frame_offset_bits = (hdr >> 11) & 0x7FFF;
            let meta = (hdr >> 8) & 0x07;
            let skip = hdr & 0xFF;
            assert!((1..=63).contains(&frame_count));
            assert_eq!(meta, 1);
            assert_eq!(skip, 0);
            assert_eq!(frame_offset_bits, 0);
        }
    }

    /// End-to-end: `convert_wavebank_data` on the retail Xbox fixture must
    /// produce a PC body that `mercs2_audio::wave::WavebankFile::parse`
    /// accepts, with the two clips' sample counts and sample rates
    /// preserved and non-silent PCM16 payloads from ffmpeg.
    #[test]
    fn retail_wavebank_block_3322_roundtrips_through_pc_parser() {
        assert_fixture_provenance();
        if find_ffmpeg().is_err() {
            panic!(
                "ffmpeg not resolvable — set MERCS2_FFMPEG or install under tools/ffmpeg/bin/"
            );
        }

        let pc = convert_wavebank_data(RETAIL_FIXTURE).expect("Xbox→PC convert succeeds");

        assert_eq!(pc.len() % mercs2_audio::wave::BLOB_ALIGN, 0);
        assert_eq!(
            u32::from_le_bytes(pc[0..4].try_into().unwrap()),
            mercs2_audio::wave::TABLE_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(pc[4..8].try_into().unwrap()),
            0x9996_B5A6
        );
        assert_eq!(u16::from_le_bytes(pc[8..10].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(pc[10..12].try_into().unwrap()), 0);

        let file = mercs2_audio::wave::WavebankFile::parse(&pc).expect("PC parse accepts output");
        assert_eq!(file.bank_hash, 0x9996_B5A6);
        assert_eq!(file.records.len(), 2);

        let expected = [
            (0x8E16_4121u32, 55_296u32),
            (0x0C0E_B8B6u32, 51_968u32),
        ];
        for (rec, (xclip, xframes)) in file.records.iter().zip(expected.iter()) {
            assert_eq!(rec.clip_hash, *xclip);
            assert_eq!(rec.channels, 1);
            assert_eq!(rec.format, mercs2_audio::wave::BYTES_PER_SAMPLE_PCM16);
            assert_eq!(rec.sample_rate, 44100);
            assert_eq!(rec.frames, *xframes);
            let bytes = match &rec.data {
                mercs2_audio::wave::WaveData::Embedded(b) => b,
                mercs2_audio::wave::WaveData::Streamed { .. } => {
                    panic!("embedded fixture produced a streamed record")
                }
            };
            assert_eq!(bytes.len(), *xframes as usize * 2);
            assert!(
                bytes.iter().any(|&b| b != 0),
                "ffmpeg-decoded PCM16 should not be all-zero"
            );
        }
    }

    /// SHA-256 of a byte slice (fixture provenance check). Tiny inline impl
    /// so the test crate stays dep-free.
    fn sha256_hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        h.finalize_hex()
    }

    // --- Minimal SHA-256 implementation (RFC 6234). Only used by the test
    // fixture provenance check above; `#[cfg(test)]` keeps it out of release.
    struct Sha256 {
        state: [u32; 8],
        buf: [u8; 64],
        buf_len: usize,
        total: u64,
    }
    impl Sha256 {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
            0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
            0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
            0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
            0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];
        fn new() -> Self {
            Self {
                state: [
                    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                    0x1f83d9ab, 0x5be0cd19,
                ],
                buf: [0u8; 64],
                buf_len: 0,
                total: 0,
            }
        }
        fn update(&mut self, data: &[u8]) {
            // Push bytes into buffer; whenever we have 64, compress one block.
            self.total = self.total.wrapping_add(data.len() as u64);
            let mut i = 0;
            while i < data.len() {
                let take = (64 - self.buf_len).min(data.len() - i);
                self.buf[self.buf_len..self.buf_len + take]
                    .copy_from_slice(&data[i..i + take]);
                self.buf_len += take;
                i += take;
                if self.buf_len == 64 {
                    let b = self.buf;
                    self.compress(&b);
                    self.buf_len = 0;
                }
            }
        }
        fn compress(&mut self, b: &[u8; 64]) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let [mut a, mut b_, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(Self::K[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b_) ^ (a & c) ^ (b_ & c);
                let temp2 = s0.wrapping_add(maj);
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b_;
                b_ = a;
                a = temp1.wrapping_add(temp2);
            }
            self.state[0] = self.state[0].wrapping_add(a);
            self.state[1] = self.state[1].wrapping_add(b_);
            self.state[2] = self.state[2].wrapping_add(c);
            self.state[3] = self.state[3].wrapping_add(d);
            self.state[4] = self.state[4].wrapping_add(e);
            self.state[5] = self.state[5].wrapping_add(f);
            self.state[6] = self.state[6].wrapping_add(g);
            self.state[7] = self.state[7].wrapping_add(h);
        }
        fn finalize_hex(mut self) -> String {
            let bits = self.total.wrapping_mul(8);
            // Pad: 0x80 then zeros to leave 8 bytes before the next 64-boundary,
            // then the 64-bit BE length.
            let one = [0x80u8];
            self.update(&one);
            while self.buf_len != 56 {
                self.update(&[0u8]);
            }
            self.update(&bits.to_be_bytes());
            let mut out = String::with_capacity(64);
            for word in self.state.iter() {
                out.push_str(&format!("{:08x}", word));
            }
            out
        }
    }

    #[test]
    fn xma2_riff_wrapper_shape() {
        let raw = vec![0u8; 2048];
        let out = wrap_xma2_raw_as_riff(&raw, 1, 44100, 48896);
        assert_eq!(&out[0..4], b"RIFF");
        let riff_sz = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(riff_sz as usize, out.len() - 8);
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[12..16], b"fmt ");
        let fmt_sz = u32::from_le_bytes(out[16..20].try_into().unwrap());
        assert_eq!(fmt_sz, 52, "fmt chunk body = 18 WAVEFORMATEX + 34 XMA2 tail");
        // wFormatTag = 0x0166 (XMA2)
        assert_eq!(u16::from_le_bytes(out[20..22].try_into().unwrap()), 0x0166);
        // nChannels, nSamplesPerSec
        assert_eq!(u16::from_le_bytes(out[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(out[24..28].try_into().unwrap()), 44100);
        // nBlockAlign = 2048
        assert_eq!(u16::from_le_bytes(out[32..34].try_into().unwrap()), 0x0800);
        // cbSize = 34
        assert_eq!(u16::from_le_bytes(out[36..38].try_into().unwrap()), 34);
        // XMA2 tail: NumStreams = 1
        assert_eq!(u16::from_le_bytes(out[38..40].try_into().unwrap()), 1);
        // ChannelMask = 0x4 (SPEAKER_FRONT_CENTER) for mono
        assert_eq!(u32::from_le_bytes(out[40..44].try_into().unwrap()), 0x0000_0004);
        // SamplesEncoded = 48896
        assert_eq!(u32::from_le_bytes(out[44..48].try_into().unwrap()), 48896);
        // data chunk follows at: 20 (RIFF+WAVE) + 8 (fmt header) + 52 (fmt body) = 80
        assert_eq!(&out[72..76], b"data");
        let data_sz = u32::from_le_bytes(out[76..80].try_into().unwrap());
        assert_eq!(data_sz, 2048);
        assert_eq!(out.len(), 80 + 2048);
    }
}
