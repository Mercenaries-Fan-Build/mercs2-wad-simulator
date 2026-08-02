//! Streaming the game's voice-over straight from its own files — no download, no re-encode.
//!
//! The ~12,988 VO lines live as raw PCM16 in `data/Audios/vo_stream.english.pws` (~798 MB). Where each
//! one sits is described by the `vo_stream.english` **wavebank** (a WAD asset): record `i` carries the
//! clip's `(offset, size)` into the `.pws`, plus its sample rate and channel count. The captioned
//! [`audioidx`](crate::audioidx) inventory is index-aligned to those records (its `wave_index`), so a
//! browsed caption maps to a record maps to a byte range.
//!
//! The record table is **parsed once** at [`open`](VoStream::open) — 12,988 small records, held in
//! memory — so playing a clip is a single `seek` + `read` of its range, never a scan of the 798 MB
//! file. The `.pws` is opened per-clip and dropped, so nothing holds the huge file resident.

use mercs2_engine::wad;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The `vo_stream.english` wavebank's chunk type hash (`TYPE_ID_WAVEBANK` = 6).
const TH_WAVEBANK: u32 = 0xF753_F6D0;
/// Wavebank header/record layout (`mercs2_audio::wave` documents the same offsets).
const REC_SIZE: usize = 36;

/// One clip's location and format in the `.pws`.
#[derive(Debug, Clone, Copy)]
struct Rec {
    offset: u64,
    size: u32,
    rate: u32,
    channels: u8,
    /// The wavebank format byte at `+6`: `2` (bytes-per-sample) / `CODEC_PCM` = embedded PCM16. VO in
    /// the `.pws` is PCM16; other codecs (XMA) are left unplayable rather than mis-decoded.
    codec: u8,
}

/// Decoded PCM for one clip, ready to hand to an output stream.
pub struct Pcm {
    /// Interleaved int16 samples.
    pub samples: Vec<i16>,
    pub rate: u32,
    pub channels: u8,
}

impl Pcm {
    /// Length in seconds — the oracle the tests check against the manifest's `duration_s`.
    pub fn duration_s(&self) -> f32 {
        let frames = self.samples.len() / self.channels.max(1) as usize;
        frames as f32 / self.rate.max(1) as f32
    }
}

/// The parsed VO record index plus the path to the sample stream.
pub struct VoStream {
    records: Vec<Rec>,
    pws: PathBuf,
}

impl VoStream {
    /// Build the index from the game directory (the folder that contains `data/`, or `data/`
    /// itself). The `vo_stream.english` wavebank lives in **English.wad** (not vz.wad), so this opens
    /// that archive itself, extracts the bank, parses every record, and locates
    /// `data/Audios/vo_stream.english.pws`. Errors if English.wad, the bank, or the `.pws` is missing.
    pub fn open(game_dir: &Path) -> Result<VoStream, String> {
        let english = find_wad(game_dir, "English.wad")
            .ok_or_else(|| format!("English.wad not found under {}", game_dir.display()))?;
        let mut wad = wad::open(&english.to_string_lossy())
            .map_err(|e| format!("open {}: {e}", english.display()))?;
        let name_hash = mercs2_formats::hash::pandemic_hash_m2("vo_stream.english");
        let container = wad::extract_container_typed(&mut wad, name_hash, TH_WAVEBANK)
            .map_err(|e| format!("vo_stream.english wavebank: {e}"))?;
        // The wavebank asset is a UCFX container; the record table is its `data` chunk.
        let body = mercs2_formats::ucfx::extract_chunk_body(&container, b"data")
            .unwrap_or(container);
        let records = parse_records(&body)?;
        let pws = find_pws(game_dir).ok_or_else(|| {
            format!("vo_stream.english.pws not found under {} (data/Audios/)", game_dir.display())
        })?;
        Ok(VoStream { records, pws })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Decode one clip's PCM by its wave index — a single seek + read of its `.pws` range. Returns an
    /// error for an out-of-range index, a non-PCM codec, or an I/O failure.
    pub fn clip_pcm(&self, wave_index: u32) -> Result<Pcm, String> {
        let rec = self
            .records
            .get(wave_index as usize)
            .ok_or_else(|| format!("wave index {wave_index} is past the {} records", self.records.len()))?;
        // `4` = CODEC_STREAM: the samples live in the `.pws` and are PCM16 there (this is the whole
        // streaming-VO bank). `1`/`2` are embedded PCM16. Anything else is a console codec (XMA) we do
        // not decode on PC — say so rather than play noise.
        if !matches!(rec.codec, 1 | 2 | 4) {
            return Err(format!("clip {wave_index} is codec 0x{:02X}, not PC PCM16", rec.codec));
        }
        let mut f = std::fs::File::open(&self.pws).map_err(|e| format!("open {}: {e}", self.pws.display()))?;
        f.seek(SeekFrom::Start(rec.offset)).map_err(|e| format!("seek: {e}"))?;
        let mut buf = vec![0u8; rec.size as usize];
        f.read_exact(&mut buf).map_err(|e| format!("read {} B at {}: {e}", rec.size, rec.offset))?;
        // The PC `.pws` is raw IMA ADPCM (36-byte mono / 72-byte stereo blocks), NOT PCM16 — reading
        // the bytes as PCM produces static at ~1/3.6 the true length. Decode with the game's own IMA
        // decoders, the ones `vo_stream_extract` proved against the retail stream.
        let channels = rec.channels.max(1);
        let samples = if channels >= 2 {
            mercs2_audio::wave::decode_ima_stereo(&buf)
        } else {
            mercs2_audio::wave::decode_ima_mono(&buf)
        };
        let rate = if rec.rate == 0 { 44100 } else { rec.rate };
        Ok(Pcm { samples, rate, channels })
    }
}

/// Parse the wavebank body's record table. `+8` (`populated`) is the count — NOT the word at `+0`,
/// which reads 29 on this bank while it holds 12,988 (the clamp bug `mercs2_audio::wave` documents).
fn parse_records(body: &[u8]) -> Result<Vec<Rec>, String> {
    if body.len() < 20 {
        return Err("wavebank body too short".into());
    }
    let populated = u16::from_le_bytes([body[8], body[9]]) as usize;
    let roff = u32::from_le_bytes([body[16], body[17], body[18], body[19]]) as usize;
    let max_fit = body.len().saturating_sub(roff) / REC_SIZE;
    let n = populated.min(max_fit);
    let mut recs = Vec::with_capacity(n);
    for i in 0..n {
        let r = roff + i * REC_SIZE;
        if r + REC_SIZE > body.len() {
            break;
        }
        let rd = |o: usize| u32::from_le_bytes([body[r + o], body[r + o + 1], body[r + o + 2], body[r + o + 3]]);
        recs.push(Rec {
            channels: { let c = body[r + 5]; if c == 0 { 1 } else { c } },
            codec: body[r + 6],
            rate: rd(8),
            size: rd(12),
            offset: rd(32) as u64,
        });
    }
    Ok(recs)
}

/// Locate `vo_stream.english.pws`. `game_dir` may be the install root (has `data/`) or the `data/`
/// folder itself; the stream lives under `Audios/` either way.
fn find_pws(game_dir: &Path) -> Option<PathBuf> {
    for base in [game_dir.join("data").join("Audios"), game_dir.join("Audios")] {
        let p = base.join("vo_stream.english.pws");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Locate a WAD (`English.wad`, …) whether `game_dir` is the install root or `data/` itself.
fn find_wad(game_dir: &Path, name: &str) -> Option<PathBuf> {
    for base in [game_dir.join("data"), game_dir.to_path_buf()] {
        let p = base.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ Decode straight from the retail game files and check it against an INDEPENDENT oracle: the
    /// bundled manifest's own `duration_s`, which the user measured from the extracted WAVs. If our
    /// record parse, `.pws` seek and PCM16 read are right, decoded seconds ≈ manifest seconds for
    /// every sampled clip. Runs only when the game + `.pws` are present.
    #[test]
    fn decodes_vo_clips_matching_the_manifest_durations() {
        let Some(found) = mercs2_quartermaster::game::discover() else {
            eprintln!("SKIPPING: no game stack");
            return;
        };
        // vz.wad → the install's `data/` folder → the game dir.
        let vzwad = std::path::Path::new(&found.path).to_path_buf();
        let data_dir = vzwad.parent().unwrap_or(Path::new("."));
        let game_dir = data_dir.parent().unwrap_or(data_dir);
        if super::find_pws(game_dir).is_none() {
            eprintln!("SKIPPING: no vo_stream.english.pws under {}", game_dir.display());
            return;
        }
        let vo = VoStream::open(game_dir).expect("open vo stream");
        assert!(vo.len() > 10_000, "expected ~12,988 records, got {}", vo.len());

        let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../workshop_data");
        let idx = crate::audioidx::AudioIndex::load(&home);
        assert!(!idx.is_empty(), "need the manifest as the oracle");

        // Sample captioned clips spread across the file; each decoded duration must match.
        let mut checked = 0;
        for clip in idx.clips.iter().filter(|c| c.wave_index.is_some() && c.duration_s > 0.3).step_by(1500) {
            let wi = clip.wave_index.unwrap();
            let Ok(pcm) = vo.clip_pcm(wi) else { continue };
            let got = pcm.duration_s();
            assert!(
                (got - clip.duration_s).abs() < 0.15,
                "clip {wi} ({}): decoded {got:.3}s vs manifest {:.3}s",
                clip.original,
                clip.duration_s
            );
            assert!(pcm.rate >= 8000 && pcm.rate <= 48000, "clip {wi} rate {} implausible", pcm.rate);
            checked += 1;
        }
        assert!(checked >= 3, "expected to verify several clips, only did {checked}");
        eprintln!("vo stream: {} records; verified {checked} clip durations against the manifest", vo.len());
    }
}
