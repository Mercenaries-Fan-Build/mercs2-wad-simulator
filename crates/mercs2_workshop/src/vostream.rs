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

use mercs2_audio::wave::{decode_ima_mono, decode_ima_stereo, WaveData, WaveRecord, WavebankFile};
use mercs2_engine::wad;
use mercs2_formats::types::TYPE_HASH_WAVEBANK;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// One wavebank record's samples. An embedded record's are its bytes, PCM16. A streamed record's are
/// its range of `pws`, the `.pws` its bank streams from, which the PC stores as IMA ADPCM (36-byte
/// mono, 72-byte stereo blocks); its last block runs past the clip, so the decode is cut at the
/// record's `+0x10`, which for a streamed record counts the decoded PCM16 bytes.
pub fn record_pcm(rec: &WaveRecord, pws: Option<&Path>) -> Result<Pcm, String> {
    if rec.sample_rate == 0 {
        return Err(format!("record 0x{:08X} carries no sample rate", rec.clip_hash));
    }
    let channels = rec.channels;
    let samples = match &rec.data {
        WaveData::Embedded(bytes) => bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
        WaveData::Streamed { offset, size, .. } => {
            let pws = pws.ok_or_else(|| format!("record 0x{:08X} streams from a .pws, and none was given", rec.clip_hash))?;
            let mut f = std::fs::File::open(pws).map_err(|e| format!("open {}: {e}", pws.display()))?;
            f.seek(SeekFrom::Start(u64::from(*offset))).map_err(|e| format!("{}: seek to {offset}: {e}", pws.display()))?;
            let mut buf = vec![0u8; *size as usize];
            f.read_exact(&mut buf).map_err(|e| format!("{}: {size} B at {offset}: {e}", pws.display()))?;
            let mut pcm = if channels >= 2 { decode_ima_stereo(&buf) } else { decode_ima_mono(&buf) };
            pcm.truncate(rec.frames as usize / 2);
            pcm
        }
    };
    Ok(Pcm { samples, rate: rec.sample_rate, channels })
}

/// A record's length in seconds: an embedded record's `+0x10` counts frames, a streamed record's the
/// decoded PCM16 bytes ([`record_pcm`]).
pub fn record_seconds(rec: &WaveRecord) -> f32 {
    let frames = match rec.data {
        WaveData::Streamed { .. } => rec.frames / (2 * u32::from(rec.channels.max(1))),
        WaveData::Embedded(_) => rec.frames,
    };
    frames as f32 / rec.sample_rate.max(1) as f32
}

/// The `.pws` a game wavebank streams from: the file named `stream` in the `Audios` folder beside
/// `vz`, matched case-insensitively.
pub fn game_pws(vz: &Path, stream: &str) -> Result<PathBuf, String> {
    let dir = vz.parent().map(|d| d.join("Audios")).ok_or_else(|| format!("{} has no parent folder", vz.display()))?;
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for e in entries {
        let p = e.map_err(|e| format!("reading {}: {e}", dir.display()))?.path();
        if p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(stream)) {
            return Ok(p);
        }
    }
    Err(format!("{stream} is not in {}", dir.display()))
}

/// Decoded PCM for one clip, ready to hand to an output stream.
#[derive(Debug)]
pub struct Pcm {
    /// Interleaved int16 samples.
    pub samples: Vec<i16>,
    pub rate: u32,
    pub channels: u8,
}

#[cfg(test)]
impl Pcm {
    /// Length in seconds — the oracle the tests check against the manifest's `duration_s`.
    pub fn duration_s(&self) -> f32 {
        let frames = self.samples.len() / self.channels.max(1) as usize;
        frames as f32 / self.rate.max(1) as f32
    }
}

/// The parsed VO record index plus the path to the sample stream.
pub struct VoStream {
    records: Vec<WaveRecord>,
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
        let container = wad::extract_container_typed(&mut wad, name_hash, TYPE_HASH_WAVEBANK)
            .map_err(|e| format!("vo_stream.english wavebank: {e}"))?;
        // The wavebank asset is a UCFX container; the record table is its `data` chunk.
        let body = mercs2_formats::ucfx::extract_chunk_body(&container, b"data")
            .ok_or("the vo_stream.english wavebank's container has no data chunk")?;
        let records = WavebankFile::parse(&body).map_err(|e| format!("vo_stream.english wavebank: {e}"))?.records;
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

    /// Decode one clip's PCM by its wave index — a single seek + read of its `.pws` range
    /// ([`record_pcm`]). Returns an error for an out-of-range index or an I/O failure.
    pub fn clip_pcm(&self, wave_index: u32) -> Result<Pcm, String> {
        let rec = self
            .records
            .get(wave_index as usize)
            .ok_or_else(|| format!("wave index {wave_index} is past the {} records", self.records.len()))?;
        record_pcm(rec, Some(&self.pws))
    }
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

/// Game-gated: built by the `retail` feature, reads the vz.wad named by the repo-root
/// `.mercs2-local.toml` and the `.pws` of the same install, and fails if either is absent.
#[cfg(all(test, feature = "retail"))]
mod retail {
    use super::*;

    /// ★ Decode straight from the retail game files and check it against an INDEPENDENT oracle: the
    /// bundled manifest's own `duration_s`, which the user measured from the extracted WAVs. If our
    /// record parse, `.pws` seek and PCM16 read are right, decoded seconds ≈ manifest seconds for
    /// every sampled clip.
    #[test]
    fn decodes_vo_clips_matching_the_manifest_durations() {
        let vzwad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{e}"));
        // vz.wad → the install's `data/` folder → the game dir.
        let data_dir = vzwad.parent().expect("vz.wad has a data folder");
        let game_dir = data_dir.parent().expect("the data folder has an install root");
        if find_pws(game_dir).is_none() {
            panic!(
                "no vo_stream.english.pws under {}/data/Audios, the install of {}",
                game_dir.display(),
                vzwad.display()
            );
        }
        let vo = VoStream::open(game_dir).expect("open vo stream");
        assert!(vo.len() > 10_000, "expected ~12,988 records, got {}", vo.len());

        let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../workshop_data");
        let idx = crate::audioidx::AudioIndex::load(&home).unwrap_or_else(|e| panic!("{e}"));
        assert!(!idx.is_empty(), "need the manifest as the oracle");

        // Sample captioned clips spread across the file; each decoded duration must match.
        let mut checked = 0;
        for clip in idx.clips.iter().filter(|c| c.wave_index.is_some() && c.duration_s > 0.3).step_by(1500) {
            let wi = clip.wave_index.unwrap();
            let pcm = vo
                .clip_pcm(wi)
                .unwrap_or_else(|e| panic!("clip {wi} ({}) failed to decode: {e}", clip.original));
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
