//! Console (Xbox 360 / PS3) wavebank → PC wavebank transcoder.
//!
//! See `docs/_wavebank_container.md` for the console-side body layout
//! (both platforms share the same wavebank structure; the codec byte at
//! record `+0x06` picks the clip format) and `mercs2_audio::wave` for
//! the PC embedded wavebank layout. Embedded PC clips are interleaved
//! little-endian PCM16 with `format = 0x02` and
//! `data_size = frames × channels × 2`. XMA (`0x01`), XMA2 (`0x05`,
//! Xbox) and MP3 (`0x0C`, PS3) are decoded by ffmpeg; PCM (`0x00`) is a
//! BE→LE byte swap.

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

/// One channel of a PC `.pws` block: a predictor, a step index and a reserved byte, then the nibbles.
const PWS_MONO_BLOCK: usize = 36;
/// Frames a PC `.pws` block holds.
const PWS_BLOCK_FRAMES: usize = 64;
/// A PC `.pws` clip starts on this boundary, and the file ends on it.
const PWS_ALIGN: usize = 16;

pub const TABLE_VERSION: u32 = 0x1D;

pub const CODEC_PCM: u8 = 0x00;
pub const CODEC_XMA: u8 = 0x01;
pub const CODEC_XMA2: u8 = 0x05;
/// PS3 embedded wavebank codec byte: raw MP3 (MPEG-1 / MPEG-2.5 Layer III)
/// bitstream. PROVEN by byte-level inspection of two retail PS3 `vz.wad`
/// wavebanks (`wpn_designator_flare` block 4243, `wpn_sniperrifle` block
/// 2561): every record whose `+0x06` byte equals `0x0C` begins its clip
/// blob with a valid 11-bit MPEG frame sync (`0x7FF`), Layer-III header,
/// and the frame-count arithmetic `n_frames * 1152 == decoded_samples`
/// holds exactly (49 / 46 frames for the two `0x9996B5A6` clips, 12/12
/// for the sniper bank). ffmpeg's native `mp3float` decoder consumes
/// the raw blob with no RIFF wrapper (see `transcode_mp3_raw_to_pcm16`).
pub const CODEC_MP3: u8 = 0x0C;

/// PC embedded wavebank record `+0x06` byte: 2 bytes per PCM16 sample.
pub const PC_FORMAT_PCM16: u8 = 0x02;
/// PC streamed wavebank record `+0x06` byte.
pub const PC_FORMAT_STREAM: u8 = 0x04;

/// Embedded (`+0x0A` = 0) wavebank header stride — same on Xbox and PC.
const HEADER_SIZE: usize = 24;
/// Streamed (`+0x0A` = 1) wavebank adds a NUL-padded `.pws` name field at
/// `+0x18`, 16 bytes in the retail banks and 32 in the DLC's, so the record table
/// starts at `+0x28` or `+0x38`. The bank's `+0x10` says which.
const STREAM_HEADER_SIZES: [usize; 2] = [HEADER_SIZE + 16, HEADER_SIZE + 32];

/// Every record is 36 bytes (same stride on Xbox and PC).
const WAVEBANK_RECORD_SIZE: usize = 36;

/// Xbox XMA2 packets are 2048 bytes. The engine aligns the first embedded blob
/// to this boundary (proven for all 93 embedded wavebanks — see
/// `docs/_phase3a_inferred_claims_verified.md` §A.3).
const XMA2_PACKET: usize = 0x800;

/// PC wavebank blobs must start on, and the body must end on, a 16-byte
/// boundary (`mercs2_audio::wave::BLOB_ALIGN`).
const PC_BLOB_ALIGN: usize = 16;

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

/// One channel's nibbles for a block, each low nibble first: the 63 frames after the header sample at
/// `frame`, the last repeated past the clip's end. The 64th nibble stays 0, as it is in retail. `step`
/// carries on from the block before.
fn encode_channel(pcm: &[i16], channels: usize, ch: usize, frame: usize, step: &mut i32) -> (i16, u8, [u8; PWS_BLOCK_FRAMES / 2]) {
    let frames = pcm.len() / channels;
    let at = |f: usize| i32::from(pcm[f.min(frames - 1) * channels + ch]);
    let header = at(frame);
    let (mut predictor, start) = (header, *step);
    let mut data = [0u8; PWS_BLOCK_FRAMES / 2];
    for n in 0..PWS_BLOCK_FRAMES - 1 {
        let (nibble, p, s) = encode_sample(at(frame + 1 + n), predictor, *step);
        predictor = p;
        *step = s;
        data[n / 2] |= nibble << (4 * (n % 2));
    }
    (header as i16, start as u8, data)
}

/// Interleaved PCM16 as the IMA ADPCM a PC `.pws` holds: blocks of 64 frames each, 36 bytes mono and
/// 72 stereo. In every record of the 12,988 of `vo_stream`, the 145 of `music` and the 20 of `ambience`,
/// `size` is a whole number of those blocks, and a 56,692-block clip's `+0x10` is 16 bytes short of 64
/// frames a block. Each block carries the header sample and 63 nibbles: the 64th was 0 in all 1,053,712
/// blocks of 600 `vo_stream` clips read.
pub fn encode_pws_clip(pcm: &[i16], channels: usize) -> Result<Vec<u8>, AudioError> {
    if !matches!(channels, 1 | 2) {
        return Err(AudioError(format!("a .pws clip is mono or stereo, not {channels} channels")));
    }
    let frames = pcm.len() / channels;
    let blocks = frames.div_ceil(PWS_BLOCK_FRAMES);
    let mut out = Vec::with_capacity(blocks * PWS_MONO_BLOCK * channels);
    let mut steps = [0i32; 2];
    for k in 0..blocks {
        let frame = k * PWS_BLOCK_FRAMES;
        let (hl, sl, dl) = encode_channel(pcm, channels, 0, frame, &mut steps[0]);
        out.extend_from_slice(&hl.to_le_bytes());
        out.extend_from_slice(&[sl, 0]);
        if channels == 1 {
            out.extend_from_slice(&dl);
            continue;
        }
        let (hr, sr, dr) = encode_channel(pcm, channels, 1, frame, &mut steps[1]);
        out.extend_from_slice(&hr.to_le_bytes());
        out.extend_from_slice(&[sr, 0]);
        // Four bytes of the left channel, then four of the right.
        for (l, r) in dl.chunks(4).zip(dr.chunks(4)) {
            out.extend_from_slice(l);
            out.extend_from_slice(r);
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
/// 3. Walk up from the current working directory looking for
///    `tools/ffmpeg/bin/ffmpeg(.exe)` (useful under `cargo test`, which runs
///    from the package root).
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
/// retail Xbox wavebanks carry; see `docs/_wavebank_container.md` §6
/// and §8) in a RIFF / `XMA2WAVEFORMATEX` container that ffmpeg accepts.
///
/// Field choices follow the recipe in `_wavebank_container.md` §8:
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

/// Decode a raw MP3 bytestream (as retail PS3 wavebanks ship it) to
/// interleaved little-endian PCM16 bytes.
///
/// MP3 is self-describing — every frame carries its own sample-rate,
/// bitrate and channel-mode header — so no RIFF/WAVEFORMATEX wrapper is
/// built; the raw blob is handed straight to ffmpeg via
/// [`ffmpeg_decode_to_pcm16`]. Fails loudly if ffmpeg cannot be
/// resolved — never writes a silent-partial clip. The record's
/// `decoded_samples` and `sample_rate` are forwarded only so the caller
/// can trim/pad the result to the PC parser's exact
/// `frames * channels * 2` size relation; they do not influence the
/// decode itself.
pub fn transcode_mp3_raw_to_pcm16(
    mp3_raw: &[u8],
    channels: usize,
    _sample_rate: u32,
    _decoded_samples_per_channel: u32,
) -> Result<Vec<u8>, AudioError> {
    ffmpeg_decode_to_pcm16(mp3_raw, channels, "input.mp3")
}

/// Common ffmpeg invocation: write the bytestream to a temp file whose
/// name hints at the container (`in_name` — ffmpeg auto-detects from the
/// extension), decode to PCM16 LE WAV, return the raw interleaved PCM16
/// bytes (what the PC embedded wavebank path writes). Shared by XMA
/// (0x01, raw), XMA2 (0x05, in a caller-built RIFF container) and MP3
/// (0x0C, raw). Uses [`find_ffmpeg`] to locate the binary.
fn ffmpeg_decode_to_pcm16(bytes: &[u8], channels: usize, in_name: &str) -> Result<Vec<u8>, AudioError> {
    let ffmpeg = find_ffmpeg()?;
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mercs2_ffmpeg_{}_{n}", std::process::id()));
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
                "ffmpeg decode failed (in={in_name}, exit {:?}, ffmpeg={}): {}",
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
        CODEC_MP3 => transcode_mp3_raw_to_pcm16(clip, ch, sample_rate, decoded_samples_per_channel),
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
/// `docs/_wavebank_container.md` §"Record layout".
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
            data_offset: read_u32_be(body, rec_abs + 0x20),
        })
    }
}

/// Convert a wavebank body from Xbox BE to the PC LE layout.
///
/// Byte layout per `docs/_wavebank_container.md` §"Wavebank body":
/// * `+0x00 u32 LE` version (`TABLE_VERSION` = 0x1D).
/// * `+0x04 u32 BE` bank_hash.
/// * `+0x08 u16 BE` record count.
/// * `+0x0A u8` kind flag (0 = embedded, non-zero = streamed; `+0x0B` padding).
/// * `+0x0C u32 BE` bank_hash (repeats `+0x04`).
/// * `+0x10 u32 BE` records_off (24 embedded / 40 or 56 streamed).
/// * `+0x14 u32 BE` zero.
/// * `+0x18..records_off` streamed only: NUL-padded `.pws` name.
/// * `+records_off` 36-byte records ([`WbRecord`]).
///
/// Embedded clips are decoded to interleaved LE PCM16 and emitted with
/// `format = PC_FORMAT_PCM16` and `data_size = frames × channels × 2`.
/// A streamed bank needs its [`PcStream`], whose records replace the
/// console's: the audio moved when it was re-encoded.
pub fn convert_wavebank_data(body_be: &[u8], stream: Option<&PcStream>) -> Result<Vec<u8>, AudioError> {
    let (bank_hash, streamed, xbox_records_offset, records) = read_bank(body_be)?;
    let count = records.len();
    let stream_name: Option<&[u8]> = streamed.then(|| &body_be[HEADER_SIZE..xbox_records_offset]);

    let mut order: Vec<usize> = (0..records.len())
        .filter(|&i| records[i].data_size > 0)
        .collect();
    order.sort_by_key(|&i| {
        xbox_records_offset + i * WAVEBANK_RECORD_SIZE + records[i].data_offset as usize
    });

    let pc_records_offset = xbox_records_offset as u32;
    let pc_table_end: usize = pc_records_offset as usize + count * WAVEBANK_RECORD_SIZE;
    let pc_audio_start: usize = align_up(pc_table_end, PC_BLOB_ALIGN);

    let mut pc_audio_blob: Vec<u8> = Vec::new();
    let mut new_placement: std::collections::HashMap<usize, (u32, u32, u8)> =
        std::collections::HashMap::new();

    let mut decoded: Option<Vec<u32>> = None;
    if streamed {
        let stream = stream.ok_or_else(|| {
            AudioError(format!("streamed wavebank 0x{bank_hash:08X} needs its .pws: this conversion was given no StreamTable"))
        })?;
        if stream.records.len() != records.len() {
            return Err(AudioError(format!(
                "streamed wavebank 0x{bank_hash:08X} has {} records and its PC stream {}",
                records.len(),
                stream.records.len()
            )));
        }
        for (i, r) in stream.records.iter().enumerate() {
            new_placement.insert(i, (r.offset, r.size, PC_FORMAT_STREAM));
        }
        decoded = Some(stream.records.iter().map(|r| r.decoded_bytes).collect());
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
        out[HEADER_SIZE..xbox_records_offset].copy_from_slice(name);
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
        let decoded_field = decoded.as_ref().map_or(rec.decoded_samples, |d| d[i]);
        out[pc_r + 16..pc_r + 20].copy_from_slice(&decoded_field.to_le_bytes());
        // The console's word counts its 64 KB XMA blocks; every `vo_stream` record holds 0.
        out[pc_r + 32..pc_r + 36].copy_from_slice(&pc_off.to_le_bytes());
    }

    if !streamed {
        out[pc_audio_start..pc_audio_start + pc_audio_blob.len()]
            .copy_from_slice(&pc_audio_blob);
    }

    Ok(out)
}

/// One record of a streamed bank as PC holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcStreamRecord {
    pub offset: u32,
    pub size: u32,
    /// `+0x10`: decoded PCM16 bytes, not frames, on a streamed record (`audio_code_map.md` §11.4).
    pub decoded_bytes: u32,
}

/// A streamed bank's audio re-encoded for PC: the `.pws` file and where each record landed in it.
#[derive(Debug)]
pub struct PcStream {
    pub pws: Vec<u8>,
    pub records: Vec<PcStreamRecord>,
    /// Frames ffmpeg's decode differed from the records' counts by, over the bank.
    pub fitted_frames: u64,
}

/// Clips one after another on [`PWS_ALIGN`] boundaries, the file ending on one. Every one of the 13,153
/// retail streamed records starts on one, with 0 or 8 bytes between neighbours.
fn lay_out_stream(clips: &mut [Vec<u8>]) -> (Vec<u8>, Vec<u32>) {
    let mut pws = Vec::new();
    let mut at = Vec::with_capacity(clips.len());
    for clip in clips.iter() {
        at.push(pws.len() as u32);
        pws.extend_from_slice(clip);
        pws.resize(align_up(pws.len(), PWS_ALIGN), 0);
    }
    (pws, at)
}

impl PcStream {
    /// Decode every clip `body_be` addresses in `console_pws` and encode it as PC stores a stream.
    pub fn convert(body_be: &[u8], console_pws: &[u8]) -> Result<PcStream, AudioError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (bank_hash, streamed, _, records) = read_bank(body_be)?;
        if !streamed {
            return Err(AudioError(format!("wavebank 0x{bank_hash:08X} is embedded, so it has no stream")));
        }
        let clip = |r: &WbRecord| -> Result<(Vec<u8>, u64), AudioError> {
            let (start, size) = (r.data_offset as usize, r.data_size as usize);
            let bytes = start
                .checked_add(size)
                .and_then(|end| console_pws.get(start..end))
                .ok_or_else(|| AudioError(format!("clip 0x{:08X}: {size} bytes at {start} run past the {}-byte .pws", r.clip_hash, console_pws.len())))?;
            let ch = usize::from(r.channels());
            let pcm = normalize_embedded_wavebank_clip(bytes, r.codec(), ch, r.sample_rate, r.decoded_samples)?;
            let want = r.decoded_samples as usize * ch * 2;
            let fitted = (pcm.len().abs_diff(want) / (ch * 2)) as u64;
            let pcm = pad_or_trim_pcm16(pcm, want);
            let samples: Vec<i16> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            Ok((encode_pws_clip(&samples, ch)?, fitted))
        };
        let next = AtomicUsize::new(0);
        let done = std::sync::Mutex::new(Vec::with_capacity(records.len()));
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(r) = records.get(i) else { return };
                    let got = clip(r);
                    done.lock().expect("no worker panics holding the list").push((i, got));
                });
            }
        });
        let mut done = done.into_inner().expect("no worker panicked");
        done.sort_by_key(|(i, _)| *i);
        let mut clips = Vec::with_capacity(records.len());
        let mut fitted_frames = 0;
        for (_, got) in done {
            let (c, fitted) = got?;
            clips.push(c);
            fitted_frames += fitted;
        }
        let (pws, at) = lay_out_stream(&mut clips);
        let records = records
            .iter()
            .zip(at)
            .zip(&clips)
            .map(|((r, offset), c)| PcStreamRecord {
                offset,
                size: c.len() as u32,
                decoded_bytes: r.decoded_samples * u32::from(r.channels()) * 2,
            })
            .collect();
        Ok(PcStream { pws, records, fitted_frames })
    }
}

/// An archive's streamed banks, by the bank entry's hash, each with its PC stream.
#[derive(Debug, Default)]
pub struct StreamTable {
    banks: std::collections::HashMap<u32, (String, PcStream)>,
}

impl StreamTable {
    /// Convert every streamed bank of a big-endian archive's `n` blocks. `files` are the package's
    /// `.pws` files as `(name, bytes)`. A language's file is named for its bank's entry
    /// (`vo_stream_dlctest.english.pws`), so a bank finds its file by hash; a streamed bank without a
    /// file, or a file no bank claims, is an error.
    pub fn read(n: usize, mut block: impl FnMut(usize) -> Result<Vec<u8>, String>, files: &[(String, Vec<u8>)]) -> Result<StreamTable, String> {
        let stem = |name: &str| name.rsplit_once('.').map_or(name, |(s, _)| s).to_ascii_lowercase();
        let by_hash: std::collections::HashMap<u32, usize> =
            files.iter().enumerate().map(|(i, (name, _))| (crate::hash::pandemic_hash_m2(&stem(name)), i)).collect();
        let mut banks = std::collections::HashMap::new();
        for i in 0..n {
            let bytes = block(i).map_err(|e| format!("block {i}: {e}"))?;
            for (hash, chunk) in super::texture::typed_chunks(&bytes, crate::types::TYPE_HASH_WAVEBANK).map_err(|e| format!("block {i}: {e}"))? {
                let (_, off, size) = super::texture::container_rows(chunk)?
                    .into_iter()
                    .find(|(t, ..)| t == b"data")
                    .ok_or_else(|| format!("wavebank 0x{hash:08X} has no data chunk"))?;
                let body = chunk.get(off..off + size).ok_or_else(|| format!("wavebank 0x{hash:08X}: data runs past the chunk"))?;
                if read_bank(body).map_err(|e| format!("wavebank 0x{hash:08X}: {e}"))?.1 {
                    let Some(&file) = by_hash.get(&hash) else {
                        return Err(format!("streamed wavebank 0x{hash:08X} is named by no .pws of the {} in the package", files.len()));
                    };
                    let (name, console) = &files[file];
                    let stream = PcStream::convert(body, console).map_err(|e| format!("{name}: {e}"))?;
                    banks.insert(hash, (format!("{}.pws", stem(name)), stream));
                }
            }
        }
        for (hash, i) in &by_hash {
            if !banks.contains_key(hash) {
                return Err(format!("{}: no streamed wavebank claims it", files[*i].0));
            }
        }
        Ok(StreamTable { banks })
    }

    /// The bank with entry hash `hash`.
    pub fn get(&self, hash: u32) -> Option<&PcStream> {
        self.banks.get(&hash).map(|(_, s)| s)
    }

    /// Every `.pws` file to write, `(name, stream)`, by name.
    pub fn streams(&self) -> Vec<(&str, &PcStream)> {
        let mut v: Vec<_> = self.banks.values().map(|(n, s)| (n.as_str(), s)).collect();
        v.sort_by_key(|(n, _)| *n);
        v
    }
}

/// A console bank's header: its hash, whether it streams, where its records start, and the records.
fn read_bank(body_be: &[u8]) -> Result<(u32, bool, usize, Vec<WbRecord>), AudioError> {
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
    let offset_known = if streamed { STREAM_HEADER_SIZES.contains(&xbox_records_offset) } else { xbox_records_offset == HEADER_SIZE };
    if !offset_known {
        return Err(AudioError(format!(
            "wavebank records_offset {xbox_records_offset} is not one the format has (streamed={streamed})"
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

    let mut records: Vec<WbRecord> = Vec::with_capacity(count);
    for i in 0..count {
        let rec_abs = xbox_records_offset + i * WAVEBANK_RECORD_SIZE;
        records.push(WbRecord::read(body_be, rec_abs)?);
    }
    Ok((bank_hash, streamed, xbox_records_offset, records))
}

/// A console sound table put in PC byte order one field at a time (`audio_code_map.md` §11.3, §11.4,
/// §11.7). Offsets and counts are read from the console bytes before they are swapped. Every byte of
/// the body belongs to exactly one field: a byte no field claims, or two fields claim, is an error, so a
/// layout this walk does not know cannot pass through.
struct SoundTable<'a> {
    be: &'a [u8],
    out: Vec<u8>,
    claimed: Vec<bool>,
    what: &'static str,
}

impl<'a> SoundTable<'a> {
    fn new(be: &'a [u8], what: &'static str) -> Self {
        SoundTable { be, out: be.to_vec(), claimed: vec![false; be.len()], what }
    }

    fn claim(&mut self, o: usize, n: usize, field: &str) -> Result<(), AudioError> {
        let end = o.checked_add(n).filter(|&e| e <= self.be.len()).ok_or_else(|| {
            AudioError(format!("{}: {field} at +0x{o:X} runs past the {}-byte body", self.what, self.be.len()))
        })?;
        if let Some(i) = (o..end).find(|&i| self.claimed[i]) {
            return Err(AudioError(format!("{}: {field} at +0x{o:X} overlaps another field at +0x{i:X}", self.what)));
        }
        self.claimed[o..end].fill(true);
        Ok(())
    }

    fn peek32(&self, o: usize, field: &str) -> Result<u32, AudioError> {
        self.be
            .get(o..o + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
            .ok_or_else(|| AudioError(format!("{}: {field} at +0x{o:X} runs past the {}-byte body", self.what, self.be.len())))
    }

    fn peek8(&self, o: usize, field: &str) -> Result<u8, AudioError> {
        self.be
            .get(o)
            .copied()
            .ok_or_else(|| AudioError(format!("{}: {field} at +0x{o:X} runs past the {}-byte body", self.what, self.be.len())))
    }

    /// A u32 or f32: swapped; returns its value.
    fn word(&mut self, o: usize, field: &str) -> Result<u32, AudioError> {
        let v = self.peek32(o, field)?;
        self.claim(o, 4, field)?;
        self.out[o..o + 4].reverse();
        Ok(v)
    }

    /// A word that must hold `want`.
    fn word_is(&mut self, o: usize, want: u32, field: &str) -> Result<(), AudioError> {
        let v = self.word(o, field)?;
        if v != want {
            return Err(AudioError(format!("{}: {field} at +0x{o:X} is 0x{v:X}, the layout puts 0x{want:X} there", self.what)));
        }
        Ok(())
    }

    /// A u16: swapped; returns its value.
    fn half(&mut self, o: usize, field: &str) -> Result<u16, AudioError> {
        self.claim(o, 2, field)?;
        let v = u16::from_be_bytes([self.be[o], self.be[o + 1]]);
        self.out[o..o + 2].reverse();
        Ok(v)
    }

    /// Single bytes, kept in order.
    fn bytes(&mut self, o: usize, n: usize, field: &str) -> Result<(), AudioError> {
        self.claim(o, n, field)
    }

    /// The table version, which both platforms store little-endian.
    fn version(&mut self) -> Result<(), AudioError> {
        self.claim(0, 4, "version")?;
        let v = read_u32_le(self.be, 0);
        if v != TABLE_VERSION {
            return Err(AudioError(format!("{}: version 0x{v:X}, expected 0x{TABLE_VERSION:X}", self.what)));
        }
        Ok(())
    }

    /// A `{u32 table hash, u16 index, u16 0, f32 weight}` reference: a wave in a group, a group in a
    /// multi-track sound's entry.
    fn reference(&mut self, o: usize, field: &str) -> Result<(), AudioError> {
        self.word(o, field)?;
        self.half(o + 4, field)?;
        let pad = self.half(o + 6, field)?;
        if pad != 0 {
            return Err(AudioError(format!("{}: {field} at +0x{o:X} carries 0x{pad:X} after its index", self.what)));
        }
        self.word(o + 8, field)?;
        Ok(())
    }

    /// `count` records back to back from `recs`, each record's offset from `recs` in the u32 table at
    /// `table`; the last record ends at `table`. Returns each record's `(start, end)`.
    fn records(&mut self, recs: usize, table: usize, count: usize, field: &str) -> Result<Vec<(usize, usize)>, AudioError> {
        let mut starts = Vec::with_capacity(count);
        for k in 0..count {
            starts.push(recs + self.word(table + 4 * k, field)? as usize);
        }
        let mut spans = Vec::with_capacity(count);
        let mut at = recs;
        for k in 0..count {
            let end = starts.get(k + 1).copied().unwrap_or(table);
            if starts[k] != at || end < at {
                return Err(AudioError(format!("{}: {field} record {k} at +0x{:X}, the contiguous layout puts it at +0x{at:X}", self.what, starts[k])));
            }
            spans.push((at, end));
            at = end;
        }
        if at != table {
            return Err(AudioError(format!("{}: {field} records end at +0x{at:X}, their offset table is at +0x{table:X}", self.what)));
        }
        Ok(spans)
    }

    /// An automation record (§11.7): a `u32` kind and the fields that kind has.
    fn automation(&mut self, s: usize, e: usize) -> Result<(), AudioError> {
        let kind = self.word(s, "automation kind")?;
        let (size, what) = (e - s, self.what);
        let sized = |n: usize| -> Result<(), AudioError> {
            if size != n {
                return Err(AudioError(format!("{what}: an automation record of kind {kind} at +0x{s:X} spans {size} bytes, not {n}")));
            }
            Ok(())
        };
        match kind {
            0..=3 => {
                sized(28)?;
                self.word(s + 4, "ramp start")?;
                // The mode: a byte and three of 0 on both platforms (§11.7, "low byte of mode").
                self.bytes(s + 8, 4, "ramp mode")?;
                for o in (s + 12..e).step_by(4) {
                    self.word(o, "ramp field")?;
                }
            }
            7 | 9 => {
                sized(if kind == 7 { 12 } else { 16 })?;
                for o in (s + 4..e).step_by(4) {
                    self.word(o, "automation field")?;
                }
            }
            4 => {
                sized(72)?;
                self.word(s + 4, "kind-4 start")?;
                self.bytes(s + 8, 6, "kind-4 jitter flags")?;
                self.half(s + 14, "kind-4 +0x0E")?;
                for o in (s + 16..e).step_by(4) {
                    self.word(o, "kind-4 field")?;
                }
            }
            5 | 6 | 8 => {
                self.word(s + 4, "curve +0x04")?;
                // The point count: a byte and three of 0 on both platforms.
                self.bytes(s + 8, 4, "curve point count")?;
                let n = read_u32_le(self.be, s + 8) as usize;
                self.word(s + 12, "curve parameter")?;
                self.word_is(s + 16, 0x14, "curve points offset")?;
                sized(0x14 + 8 * n)?;
                for o in (s + 0x14..e).step_by(4) {
                    self.word(o, "curve point")?;
                }
            }
            _ => return Err(AudioError(format!("{}: automation kind {kind} at +0x{s:X} is not one the format has", self.what))),
        }
        Ok(())
    }

    /// A multi-track cue's body (§11.7), `c` the cue's start and `end` its end.
    fn multi_track(&mut self, c: usize, end: usize) -> Result<(), AudioError> {
        let count = |t: &Self, rel: usize, field| t.peek8(c + rel, field).map(usize::from);
        let (events, tracks, params, curves) =
            (count(self, 0x11, "event count")?, count(self, 0x12, "track count")?, count(self, 0x13, "parameter count")?, count(self, 0x14, "curve count")?);
        self.bytes(c + 0x10, 6, "multi-track counts")?;
        self.half(c + 0x16, "multi-track +0x16")?;
        for rel in (0x18..0x28).step_by(4) {
            self.word(c + rel, "multi-track field")?;
        }
        let mut o = [0usize; 7];
        for (k, v) in o.iter_mut().enumerate() {
            *v = c + self.word(c + 0x28 + 4 * k, "multi-track table offset")? as usize;
        }
        let [ev, ev_t, tr, tr_t, par, cv, cv_t] = o;
        if par + 4 * params != end {
            return Err(AudioError(format!("{}: multi-track cue at +0x{c:X} ends at +0x{end:X}, its parameters at +0x{:X}", self.what, par + 4 * params)));
        }
        for (s, e) in self.records(ev, ev_t, events, "event")? {
            self.automation(s, e)?;
        }
        for (s, e) in self.records(cv, cv_t, curves, "cue curve")? {
            self.automation(s, e)?;
        }
        for (s, e) in self.records(tr, tr_t, tracks, "track")? {
            self.track(s, e)?;
        }
        for k in 0..params {
            self.word(par + 4 * k, "parameter hash")?;
        }
        Ok(())
    }

    fn track(&mut self, s: usize, e: usize) -> Result<(), AudioError> {
        let (autos, sounds) = (usize::from(self.peek8(s + 1, "automation count")?), usize::from(self.peek8(s + 2, "sound count")?));
        self.bytes(s, 4, "track counts")?;
        self.word(s + 4, "track loop start")?;
        self.word(s + 8, "track loop end")?;
        let mut o = [0usize; 4];
        for (k, v) in o.iter_mut().enumerate() {
            *v = s + self.word(s + 0x0C + 4 * k, "track table offset")? as usize;
        }
        let [au, au_t, so, so_t] = o;
        if so_t + 4 * sounds != e {
            return Err(AudioError(format!("{}: track at +0x{s:X} ends at +0x{e:X}, its sound table at +0x{:X}", self.what, so_t + 4 * sounds)));
        }
        for (a, z) in self.records(au, au_t, autos, "track automation")? {
            self.automation(a, z)?;
        }
        for (a, z) in self.records(so, so_t, sounds, "track sound")? {
            let n = usize::from(self.peek8(a + 4, "sound entry count")?);
            self.bytes(a, 8, "sound bytes")?;
            self.word(a + 8, "sound start time")?;
            self.word_is(a + 0x0C, 0x10, "sound entry offset")?;
            if z - a != 0x10 + 12 * n {
                return Err(AudioError(format!("{}: sound at +0x{a:X} spans {} bytes for {n} entries", self.what, z - a)));
            }
            for k in 0..n {
                self.reference(a + 0x10 + 12 * k, "sound entry")?;
            }
        }
        Ok(())
    }

    fn group(&mut self, g: usize, end: usize, i: usize) -> Result<(), AudioError> {
        self.word(g, "group sound id")?;
        self.word(g + 0x04, "group category")?;
        self.word_is(g + 0x08, 0, "group +0x08")?;
        // The form: a byte and three of 0 on both platforms, so PC's u32 read gets the same value.
        self.bytes(g + 0x0C, 4, "group form")?;
        let form = read_u32_le(self.be, g + 0x0C);
        self.word(g + 0x10, "group priority")?;
        // The positional flag: a byte, which the engine reads as one (`FUN_00837830`, `0x008378A9`).
        self.bytes(g + 0x14, 4, "group positional flag")?;
        for rel in (0x18..0x2C).step_by(4) {
            self.word(g + rel, "group distance field")?;
        }
        let size = match form {
            0 => {
                self.word(g + 0x2C, "group volume")?;
                self.word(g + 0x30, "group pitch")?;
                self.reference(g + 0x34, "group wave")?;
                0x40
            }
            1 => {
                let waves = usize::from(self.peek8(g + 0x2D, "group wave count")?);
                self.bytes(g + 0x2C, 4, "group loop, count, selection")?;
                for rel in (0x30..0x48).step_by(4) {
                    self.word(g + rel, "group field")?;
                }
                self.bytes(g + 0x48, 4, "group flag bytes")?;
                for rel in (0x4C..0x68).step_by(4) {
                    self.word(g + rel, "group field")?;
                }
                for k in 0..waves {
                    self.reference(g + 0x68 + 12 * k, "group wave")?;
                }
                0x68 + 12 * waves
            }
            _ => return Err(AudioError(format!("{}: group {i} has form {form}, not 0 or 1", self.what))),
        };
        if end - g != size {
            return Err(AudioError(format!("{}: group {i} spans {} bytes, its form {size}", self.what, end - g)));
        }
        Ok(())
    }

    fn cue(&mut self, c: usize, end: usize, i: usize) -> Result<(), AudioError> {
        let form = self.peek8(c + 5, "cue form")?;
        self.word(c, "cue guid")?;
        self.bytes(c + 4, 4, "cue form and limit")?;
        self.word(c + 0x08, "cue gain")?;
        self.word(c + 0x0C, "cue length")?;
        match form {
            0 => {
                if end - c != 24 {
                    return Err(AudioError(format!("{}: single-track cue {i} spans {} bytes, not 24", self.what, end - c)));
                }
                self.word(c + 0x10, "cue soundbank")?;
                self.half(c + 0x14, "cue group index")?;
                self.half(c + 0x16, "cue +0x16")?;
            }
            1 => self.multi_track(c, end)?,
            _ => return Err(AudioError(format!("{}: cue {i} has form {form}, not 0 or 1", self.what))),
        }
        Ok(())
    }

    fn finish(self) -> Result<Vec<u8>, AudioError> {
        match self.claimed.iter().position(|c| !c) {
            Some(i) => Err(AudioError(format!("{}: byte +0x{i:X} of {} belongs to no field", self.what, self.be.len()))),
            None => Ok(self.out),
        }
    }
}

/// A console `soundbank` body in PC form (`audio_code_map.md` §11.4, §11.7). Its counts are two u16s,
/// a single-track cue's group index a u16 beside another, a wave reference `{u32, u16, u16, f32}`, and
/// the flag and count bytes single bytes, so a word swap of the whole body sends every cue to group 0.
pub fn convert_soundbank_data(be: &[u8]) -> Result<Vec<u8>, AudioError> {
    let mut t = SoundTable::new(be, "soundbank");
    t.version()?;
    let hash = t.word(0x04, "bank hash")?;
    let groups = usize::from(t.half(0x08, "group count")?);
    let cues = usize::from(t.half(0x0A, "cue count")?);
    t.word_is(0x0C, hash, "bank hash repeat")?;
    let group_section = t.word(0x10, "group section")? as usize;
    let group_table = t.word(0x14, "group table")? as usize;
    let cue_section = t.word(0x18, "cue section")? as usize;
    let cue_table = t.word(0x1C, "cue table")? as usize;
    if group_section != 0x20 || cue_section != group_table + 4 * groups || cue_table + 4 * cues != be.len() {
        return Err(AudioError(format!(
            "soundbank 0x{hash:08X}: sections at 0x{group_section:X}, 0x{group_table:X}, 0x{cue_section:X}, 0x{cue_table:X} for {groups} groups and {cues} cues in {} bytes are not contiguous",
            be.len()
        )));
    }
    for (i, (s, e)) in t.records(group_section, group_table, groups, "group")?.into_iter().enumerate() {
        t.group(s, e, i)?;
    }
    for (i, (s, e)) in t.records(cue_section, cue_table, cues, "cue")?.into_iter().enumerate() {
        t.cue(s, e, i)?;
    }
    t.finish()
}

/// A console `sounddb` body in PC form (`audio_code_map.md` §11.3). Its cue and category counts are two
/// u16s, its parameter count a u16 and a u16 of 0, and each cue entry's soundbank cue index a u16 and a
/// u16 of 0, so a word swap of the whole body reads 0 cues: `FUN_0083c610`, the search FindCue
/// (`FUN_00835a70`) runs over a table of at most 0x400 cues, is bounded by the u16 at `+0x08`.
pub fn convert_sounddb_data(be: &[u8]) -> Result<Vec<u8>, AudioError> {
    let mut t = SoundTable::new(be, "sounddb");
    t.version()?;
    let hash = t.word(0x04, "bank hash")?;
    let cues = usize::from(t.half(0x08, "cue count")?);
    let categories = usize::from(t.half(0x0A, "category count")?);
    let params = usize::from(t.half(0x0C, "parameter count")?);
    if t.half(0x0E, "parameter count high half")? != 0 {
        return Err(AudioError(format!("sounddb 0x{hash:08X}: +0x0E is not 0, so the parameter count is not one u16")));
    }
    let cue_table = 0x1C;
    let category_table = cue_table + 12 * cues;
    let param_table = category_table + 8 * categories;
    t.word_is(0x10, cue_table as u32, "cue table offset")?;
    t.word_is(0x14, category_table as u32, "category table offset")?;
    t.word_is(0x18, param_table as u32, "parameter table offset")?;
    if param_table + 4 * params != be.len() {
        return Err(AudioError(format!(
            "sounddb 0x{hash:08X}: {cues} cues, {categories} categories and {params} parameters in {} bytes",
            be.len()
        )));
    }
    for k in 0..cues {
        let e = cue_table + 12 * k;
        t.word(e, "cue guid")?;
        t.word(e + 4, "cue soundbank")?;
        t.half(e + 8, "soundbank cue index")?;
        if t.half(e + 10, "soundbank cue index high half")? != 0 {
            return Err(AudioError(format!("sounddb 0x{hash:08X}: cue entry {k}'s index is not one u16")));
        }
    }
    for k in 0..categories {
        t.word(category_table + 8 * k, "category")?;
        t.word(category_table + 8 * k + 4, "parent category")?;
    }
    for k in 0..params {
        t.word(param_table + 4 * k, "parameter hash")?;
    }
    t.finish()
}

/// A big-endian sound-table chunk (one `data` row) in PC form, without its `CSUM` trailer.
fn convert_sound_table(container: &[u8], body: fn(&[u8]) -> Result<Vec<u8>, AudioError>) -> Result<Vec<u8>, String> {
    let roots = super::ps3_model::be_tree(container)?;
    let [root] = roots.as_slice() else {
        return Err(format!("a sound table with {} root rows, expected 1", roots.len()));
    };
    if &root.tag != b"data" || !root.children.is_empty() {
        return Err(format!("a sound table whose root row is {}, expected one data row", root.tag_str()));
    }
    let be = root.body.as_deref().ok_or("a sound table's data row has no body")?;
    let mut out = crate::ucfx::write_ucfx_tree(&[crate::ucfx::UcfxNode::leaf(*b"data", body(be).map_err(|e| e.0)?)]);
    out.truncate(out.len() - crate::ucfx::UCFX_CSUM_BYTES);
    Ok(out)
}

/// Convert a big-endian `soundbank` chunk, without its `CSUM` trailer.
pub fn convert_soundbank(container: &[u8]) -> Result<Vec<u8>, String> {
    convert_sound_table(container, convert_soundbank_data)
}

/// Convert a big-endian `sounddb` chunk, without its `CSUM` trailer.
pub fn convert_sounddb(container: &[u8]) -> Result<Vec<u8>, String> {
    convert_sound_table(container, convert_sounddb_data)
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

    /// A clip decoded the way the header and nibbles are laid out, 64 frames a block per channel.
    fn decode_channel(clip: &[u8], channels: usize, ch: usize) -> Vec<i32> {
        let mut out = Vec::new();
        for block in clip.chunks(PWS_MONO_BLOCK * channels) {
            let head = 4 * ch;
            let mut predictor = i32::from(i16::from_le_bytes([block[head], block[head + 1]]));
            let mut step = i32::from(block[head + 2]);
            let data: Vec<u8> = if channels == 1 {
                block[4..].to_vec()
            } else {
                block[8..].chunks(4).skip(ch).step_by(2).flatten().copied().collect()
            };
            out.push(predictor);
            for n in 0..PWS_BLOCK_FRAMES - 1 {
                let nibble = (data[n / 2] >> (4 * (n % 2))) & 0x0F;
                (predictor, step) = decode_nibble(nibble, predictor, step);
                out.push(predictor);
            }
        }
        out
    }

    #[test]
    fn a_clip_encodes_to_whole_blocks_and_tracks_its_signal() {
        let tone = |f: f32, frames: usize| -> Vec<i16> {
            (0..frames).map(|i| ((i as f32 * f).sin() * 12000.0) as i16).collect()
        };
        for frames in [64, 65, 1000] {
            let clip = encode_pws_clip(&tone(0.05, frames), 1).unwrap();
            assert_eq!(clip.len(), frames.div_ceil(64) * 36);
            assert!(clip.chunks(36).all(|b| b[35] >> 4 == 0), "the 64th nibble is 0");
            let back = decode_channel(&clip, 1, 0);
            let mae: f32 = tone(0.05, frames).iter().zip(&back).map(|(a, b)| (i32::from(*a) - b).abs() as f32).sum::<f32>() / frames as f32;
            // The first block ramps up from step 0; the signal spans 24,000.
            assert!(mae < 900.0, "{frames} frames: mean error {mae}");
        }
    }

    #[test]
    fn stereo_keeps_its_channels_apart() {
        let frames = 2000;
        let pcm: Vec<i16> = (0..frames)
            .flat_map(|i| [((i as f32 * 0.05).sin() * 12000.0) as i16, ((i as f32 * 0.011).cos() * 9000.0) as i16])
            .collect();
        let clip = encode_pws_clip(&pcm, 2).unwrap();
        assert_eq!(clip.len(), 2000usize.div_ceil(64) * 72);
        for ch in 0..2 {
            let back = decode_channel(&clip, 2, ch);
            let late: f32 = pcm.chunks(2).zip(&back).skip(200).map(|(p, b)| (i32::from(p[ch]) - b).abs() as f32).sum::<f32>() / 1800.0;
            assert!(late < 300.0, "channel {ch}: mean error {late}");
        }
    }

    #[test]
    fn a_stream_lays_clips_on_16_byte_boundaries() {
        let mut clips = vec![vec![1u8; 36], vec![2u8; 72], vec![3u8; 36]];
        let (pws, at) = lay_out_stream(&mut clips);
        assert_eq!(at, [0, 48, 128]);
        assert_eq!(pws.len(), 176);
        assert_eq!(pws[48], 2);
    }

    #[test]
    fn a_streamed_bank_without_its_stream_is_refused() {
        let mut b = vec![0u8; 24 + 16 + 36];
        b[0] = 0x1D;
        b[8..10].copy_from_slice(&1u16.to_be_bytes());
        b[10] = 1;
        b[16..20].copy_from_slice(&40u32.to_be_bytes());
        let e = convert_wavebank_data(&b, None).unwrap_err();
        assert!(e.0.contains("needs its .pws"), "{e}");
    }

    /// The word swap `convert_block` gave every sound table before it had a converter of its own.
    fn word_sweep(be: &[u8]) -> Vec<u8> {
        be.chunks(4).flat_map(|w| w.iter().rev().copied().collect::<Vec<_>>()).collect()
    }

    /// A console sounddb of the shape the DLC's VO tables have: `cues` entries, no categories or
    /// parameters, each entry `{guid, bank, u16 index, u16 0}`.
    fn console_sounddb(bank: u32, cues: &[(u32, u16)]) -> Vec<u8> {
        let mut b = TABLE_VERSION.to_le_bytes().to_vec();
        b.extend_from_slice(&bank.to_be_bytes());
        b.extend_from_slice(&(cues.len() as u16).to_be_bytes());
        b.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let end = 0x1C + 12 * cues.len() as u32;
        for v in [0x1C, end, end] {
            b.extend_from_slice(&u32::to_be_bytes(v));
        }
        for &(guid, index) in cues {
            b.extend_from_slice(&guid.to_be_bytes());
            b.extend_from_slice(&bank.to_be_bytes());
            b.extend_from_slice(&index.to_be_bytes());
            b.extend_from_slice(&[0, 0]);
        }
        b
    }

    #[test]
    fn a_console_sounddb_keeps_its_cue_count_and_indices() {
        let be = console_sounddb(0xAD2A_9B89, &[(0x34AA_F50C, 62), (0x480C_25F2, 75)]);
        let pc = convert_sounddb_data(&be).unwrap();
        assert_eq!(&pc[..4], &TABLE_VERSION.to_le_bytes());
        assert_eq!(u16::from_le_bytes([pc[8], pc[9]]), 2, "cue count");
        assert_eq!(u16::from_le_bytes([pc[10], pc[11]]), 0, "category count");
        assert_eq!(read_u32_le(&pc, 0x1C), 0x34AA_F50C);
        assert_eq!(read_u32_le(&pc, 0x1C + 8), 62, "the soundbank cue index, as PC reads it");
        assert_eq!(read_u32_le(&pc, 0x1C + 12 + 8), 75);
        // The word swap reads no cues, so FindCue's binary search over the u16 at +0x08 misses every guid.
        let swept = word_sweep(&be);
        assert_eq!(u16::from_le_bytes([swept[8], swept[9]]), 0);
        assert_eq!(read_u32_le(&swept, 0x1C + 8), 62 << 16);
    }

    /// A console soundbank of `groups` single-wave groups and one single-track cue for each `(guid, group)`.
    fn console_soundbank(bank: u32, wavebank: u32, groups: usize, cues: &[(u32, u16)]) -> Vec<u8> {
        let group_table = 0x20 + 0x40 * groups;
        let cue_section = group_table + 4 * groups;
        let cue_table = cue_section + 24 * cues.len();
        let mut b = TABLE_VERSION.to_le_bytes().to_vec();
        b.extend_from_slice(&bank.to_be_bytes());
        b.extend_from_slice(&(groups as u16).to_be_bytes());
        b.extend_from_slice(&(cues.len() as u16).to_be_bytes());
        b.extend_from_slice(&bank.to_be_bytes());
        for v in [0x20, group_table, cue_section, cue_table] {
            b.extend_from_slice(&(v as u32).to_be_bytes());
        }
        for i in 0..groups {
            for w in [0x9D63_2E49u32, 0xD221_DBE8, 0] {
                b.extend_from_slice(&w.to_be_bytes());
            }
            b.extend_from_slice(&[0, 0, 0, 0]); // form 0
            b.extend_from_slice(&0.98f32.to_be_bytes());
            b.extend_from_slice(&[1, 0, 0, 0]); // positional
            for f in [20.0f32, 120.0, 1.0, 1.0, 1.0, 0.75, 0.0] {
                b.extend_from_slice(&f.to_be_bytes());
            }
            b.extend_from_slice(&wavebank.to_be_bytes());
            b.extend_from_slice(&(i as u16).to_be_bytes());
            b.extend_from_slice(&[0, 0]);
            b.extend_from_slice(&1.0f32.to_be_bytes());
        }
        for i in 0..groups {
            b.extend_from_slice(&(0x40 * i as u32).to_be_bytes());
        }
        for &(guid, group) in cues {
            b.extend_from_slice(&guid.to_be_bytes());
            b.extend_from_slice(&[0, 0, 0, 0]);
            b.extend_from_slice(&1.0f32.to_be_bytes());
            b.extend_from_slice(&1.18f32.to_be_bytes());
            b.extend_from_slice(&bank.to_be_bytes());
            b.extend_from_slice(&group.to_be_bytes());
            b.extend_from_slice(&[0, 0]);
        }
        for i in 0..cues.len() {
            b.extend_from_slice(&(24 * i as u32).to_be_bytes());
        }
        b
    }

    #[test]
    fn a_console_soundbank_cue_keeps_its_group_and_its_waves_their_index() {
        let be = console_soundbank(0xAD2A_9B89, 0xF53C_3969, 3, &[(0x34AA_F50C, 2), (0x480C_25F2, 1)]);
        let pc = convert_soundbank_data(&be).unwrap();
        let cue_section = read_u32_le(&pc, 0x18) as usize;
        let group = |c: usize| u16::from_le_bytes([pc[c + 0x14], pc[c + 0x15]]);
        assert_eq!((read_u32_le(&pc, cue_section), group(cue_section)), (0x34AA_F50C, 2));
        assert_eq!(group(cue_section + 24), 1);
        let g2 = 0x20 + 2 * 0x40;
        assert_eq!(read_u32_le(&pc, g2 + 0x0C), 0, "form");
        assert_eq!(pc[g2 + 0x14], 1, "the positional flag byte, which FUN_00837830 reads as a char");
        assert_eq!((read_u32_le(&pc, g2 + 0x34), read_u32_le(&pc, g2 + 0x38)), (0xF53C_3969, 2));
        // The word swap puts the group index at +0x16, so every cue plays group 0.
        let swept = word_sweep(&be);
        assert_eq!(u16::from_le_bytes([swept[cue_section + 0x14], swept[cue_section + 0x15]]), 0);
    }

    /// A big-endian block of one-row `data` containers, as a console archive stores sound tables.
    fn console_block(entries: &[(u32, u32, &[u8])]) -> Vec<u8> {
        let mut block = (entries.len() as u32).to_be_bytes().to_vec();
        let containers: Vec<Vec<u8>> = entries
            .iter()
            .map(|(_, _, body)| {
                let mut c = b"XFCU".to_vec();
                for w in [40u32, 0, 0, 1] {
                    c.extend_from_slice(&w.to_be_bytes());
                }
                c.extend_from_slice(b"atad");
                for w in [0u32, body.len() as u32, 0, 0] {
                    c.extend_from_slice(&w.to_be_bytes());
                }
                c.extend_from_slice(body);
                c.extend_from_slice(b"MUSC\0\0\0\0");
                c
            })
            .collect();
        for ((name, ty, _), c) in entries.iter().zip(&containers) {
            for w in [*name, *ty, 0, c.len() as u32] {
                block.extend_from_slice(&w.to_be_bytes());
            }
        }
        containers.iter().for_each(|c| block.extend_from_slice(c));
        block
    }

    #[test]
    fn convert_block_sends_sound_tables_to_their_converters() {
        use crate::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_SOUNDDB};
        let db = console_sounddb(0xAD2A_9B89, &[(0x34AA_F50C, 1), (0x480C_25F2, 0)]);
        let bank = console_soundbank(0xAD2A_9B89, 0xF53C_3969, 2, &[(0x480C_25F2, 0), (0x34AA_F50C, 1)]);
        let block = console_block(&[(0x1084_AE93, TYPE_HASH_SOUNDDB, &db), (0x1084_AE93, TYPE_HASH_SOUNDBANK, &bank)]);
        let le = super::super::convert::convert_block(&block, false, None, None, None, None).unwrap();
        let bodies: Vec<Vec<u8>> = crate::ucfx::block_chunk_spans(&le)
            .unwrap()
            .into_iter()
            .map(|(_, r)| crate::ucfx::parse_ucfx_tree(&le[r]).unwrap().remove(0).body.unwrap())
            .collect();
        assert_eq!(bodies, [convert_sounddb_data(&db).unwrap(), convert_soundbank_data(&bank).unwrap()]);
        assert_eq!(u16::from_le_bytes([bodies[0][8], bodies[0][9]]), 2, "the sounddb's cue count");
    }

    #[test]
    fn a_sound_table_byte_no_field_claims_is_refused() {
        let mut be = console_sounddb(1, &[(2, 0)]);
        be.extend_from_slice(&[0, 0, 0, 0]);
        let e = convert_sounddb_data(&be).unwrap_err();
        assert!(e.0.contains("1 cues, 0 categories and 0 parameters in 44 bytes"), "{e}");
        let mut be = console_soundbank(1, 2, 1, &[(3, 0)]);
        be[0x20 + 0x0C] = 2;
        let e = convert_soundbank_data(&be).unwrap_err();
        assert!(e.0.contains("group 0 has form 2"), "{e}");
    }

    #[test]
    fn step_table_has_89_entries() {
        assert_eq!(IMA_STEP_TABLE.len(), 89);
        assert_eq!(IMA_STEP_TABLE[88], 32767);
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
            assert_eq!(read_u32_be(b, rec_abs + 0x1C), 0);
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

    const RETAIL_FIXTURE_PS3: &[u8] =
        include_bytes!("../../tests/fixtures/ps3_wavebank_emb_block4243_be.bin");
    const RETAIL_FIXTURE_PS3_SHA256: &str =
        "e9d8967c91c953700a327d9e307eacf7b994cc5f093ebd6b7937cae075fc66a1";

    fn assert_fixture_provenance_ps3() {
        assert_eq!(sha256_hex(RETAIL_FIXTURE_PS3), RETAIL_FIXTURE_PS3_SHA256);
        assert_eq!(RETAIL_FIXTURE_PS3.len(), 0x6F80);
    }

    /// Parse the retail-PS3 `0x9996B5A6` wavebank body (block 4243 of
    /// `game-files/ps3-VZ.WAD`, same bank name as the Xbox block 3322
    /// fixture, 2 MP3 mono clips at 44100 Hz, 0x6F80 bytes) and confirm
    /// every header + record field decodes to its retail value. The two
    /// embedded blobs are 4-byte-aligned raw MP3 bitstreams (no 2048-byte
    /// XMA2 alignment on PS3) and their first 4 bytes carry a valid MPEG
    /// Layer-III frame sync (`0x7FF`, Layer III, protection = 0).
    #[test]
    fn retail_ps3_wavebank_block_4243_parses() {
        assert_fixture_provenance_ps3();
        let b = RETAIL_FIXTURE_PS3;

        assert_eq!(read_u32_le(b, 0), TABLE_VERSION);
        assert_eq!(read_u32_be(b, 4), 0x9996_B5A6);
        assert_eq!(read_u16_be(b, 8), 2);
        assert_eq!(b[0x0A], 0);
        assert_eq!(read_u32_be(b, 12), 0x9996_B5A6);
        let records_off = read_u32_be(b, 16) as usize;
        assert_eq!(records_off, 24);

        // (clip_hash, data_size, decoded_samples, data_offset, abs_blob_start)
        let expected = [
            (0x8E16_4121u32, 0x373Du32, 56_448u32, 0x68u32, 0x80usize),
            (0x0C0E_B8B6u32, 0x3706u32, 52_992u32, 0x37C4u32, 0x3800usize),
        ];
        for (i, (xclip, xsize, xframes, xdoff, xabs)) in expected.iter().enumerate() {
            let rec_abs = records_off + i * WAVEBANK_RECORD_SIZE;
            let rec = WbRecord::read(b, rec_abs).unwrap();
            assert_eq!(rec.clip_hash, *xclip);
            assert_eq!(rec.channels(), 1);
            assert_eq!(rec.codec(), CODEC_MP3);
            assert_eq!(rec.sample_rate, 44100);
            assert_eq!(rec.data_size, *xsize);
            assert_eq!(rec.decoded_samples, *xframes);
            assert_eq!(read_u32_be(b, rec_abs + 0x1C), 0);
            assert_eq!(rec.data_offset, *xdoff);
            let blob_abs = rec_abs + rec.data_offset as usize;
            assert_eq!(blob_abs, *xabs);
            assert!(blob_abs + rec.data_size as usize <= b.len());
            // MP3 blobs are only 4-byte aligned on PS3 (no 2048 packet alignment).
            assert_eq!(blob_abs % 4, 0);

            // First 4 bytes = MPEG Layer-III frame header: 11-bit sync, Layer III, mono.
            let hdr = read_u32_be(b, blob_abs);
            let sync = (hdr >> 21) & 0x7FF;
            let layer = (hdr >> 17) & 0x3;
            let crc_bit = (hdr >> 16) & 0x1;
            let channel_mode = (hdr >> 6) & 0x3;
            assert_eq!(sync, 0x7FF, "clip {i}: not an MPEG frame");
            assert_eq!(layer, 1, "clip {i}: not Layer III");
            assert_eq!(crc_bit, 0, "clip {i}: CRC-protection bit expected 0");
            assert_eq!(channel_mode, 3, "clip {i}: expected mono (channel_mode 3)");
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
