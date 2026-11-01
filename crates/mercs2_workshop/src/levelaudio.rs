//! A level's own audio: every wavebank of its WAD, each record a line of the Audio page.
//!
//! The base game's page lists the bundle's captioned inventory by the soundbanks that play it. A level
//! has no inventory, so its banks are its wavebanks and a line is a record of one, numbered across the
//! banks in entry-hash order. Streamed records read from the `.pws` the level's `Audios` folder holds
//! under the bank's entry name (`vo_stream_dlctest.english.pws`), the file `dlc_port` writes there.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use mercs2_audio::wave::{decode_ima_mono, decode_ima_stereo, WaveData, WavebankFile};
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::types::{TYPE_HASH_WAVEBANK, TYPE_ID_WAVEBANK};

use crate::audioidx::AudioClip;
use crate::audioroute::{Bank, Routes};
use crate::vostream::Pcm;

struct LevelBank {
    entry: u32,
    name: String,
    file: WavebankFile,
    /// A streamed bank's `.pws`.
    pws: Option<PathBuf>,
}

/// A level's banks, and which `(bank, record)` each line is.
pub struct LevelAudio {
    banks: Vec<LevelBank>,
    lines: Vec<(usize, usize)>,
}

/// The `.pws` files in `dir` by the hash of their stem.
fn streams_in(dir: &Path) -> Result<HashMap<u32, PathBuf>, String> {
    let mut out = HashMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("pws")) {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                out.insert(m2(stem), p.clone());
            }
        }
    }
    Ok(out)
}

impl LevelAudio {
    /// Read every wavebank of the level WAD at `wad_path`. A streamed bank with no `.pws` in the
    /// `Audios` folder beside it is an error naming the folder.
    pub fn read(wad_path: &Path, names: &HashMap<u32, String>) -> Result<LevelAudio, String> {
        let audios = wad_path.with_file_name("Audios");
        let streams = streams_in(&audios)?;
        let mut w = wad::open(&wad_path.to_string_lossy())?;
        let entries: BTreeMap<u32, ()> =
            wad::all_asets(&w).into_iter().filter(|(_, t, _)| *t == TYPE_ID_WAVEBANK).map(|(h, ..)| (h, ())).collect();
        let mut banks = Vec::new();
        for entry in entries.into_keys() {
            let c = wad::extract_container_typed(&mut w, entry, TYPE_HASH_WAVEBANK)?;
            let body = mercs2_formats::ucfx::extract_chunk_body(&c, b"data").ok_or_else(|| format!("wavebank 0x{entry:08X}: no data chunk"))?;
            let file = WavebankFile::parse(&body).map_err(|e| format!("wavebank 0x{entry:08X}: {e:?}"))?;
            let pws = match &file.stream_name {
                None => None,
                Some(alias) => Some(streams.get(&entry).cloned().ok_or_else(|| {
                    format!("streamed wavebank 0x{entry:08X} ({alias}) has no .pws in {}", audios.display())
                })?),
            };
            let stem = pws.as_deref().and_then(|p| p.file_stem()).and_then(|s| s.to_str()).map(str::to_string);
            let name = names.get(&entry).cloned().or(stem).unwrap_or_else(|| format!("0x{entry:08X}"));
            banks.push(LevelBank { entry, name, file, pws });
        }
        banks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let lines = banks.iter().enumerate().flat_map(|(b, k)| (0..k.file.records.len()).map(move |r| (b, r))).collect();
        Ok(LevelAudio { banks, lines })
    }

    fn line(&self, line: u32) -> Option<(&LevelBank, &mercs2_audio::wave::WaveRecord)> {
        let (b, r) = *self.lines.get(line as usize)?;
        let bank = &self.banks[b];
        Some((bank, &bank.file.records[r]))
    }

    /// Each line's length in seconds.
    fn lengths_s(&self) -> Vec<f32> {
        self.lines
            .iter()
            .map(|&(b, r)| {
                let rec = &self.banks[b].file.records[r];
                let frames = match rec.data {
                    // A streamed record's `+0x10` counts decoded PCM16 bytes.
                    WaveData::Streamed { .. } => rec.frames / (2 * u32::from(rec.channels.max(1))),
                    WaveData::Embedded(_) => rec.frames,
                };
                frames as f32 / rec.sample_rate.max(1) as f32
            })
            .collect()
    }

    /// The page's clip rows, one per line. A record's hash is its only name.
    pub fn clips(&self) -> Vec<AudioClip> {
        self.lines
            .iter()
            .zip(self.lengths_s())
            .enumerate()
            .map(|(i, (&(b, r), duration_s))| {
                let (bank, rec) = (&self.banks[b], &self.banks[b].file.records[r]);
                AudioClip {
                    original: format!("{}/{r:05}__0x{:08X}.wav", bank.name, rec.clip_hash),
                    bank: bank.name.clone(),
                    duration_s,
                    wave_index: Some(i as u32),
                    ..Default::default()
                }
            })
            .collect()
    }

    /// The page's routing: each wavebank a bank of its lines. No cue is read, so none is listed.
    pub fn routes(&self) -> Routes {
        let mut lines: Vec<Vec<u32>> = vec![Vec::new(); self.banks.len()];
        for (i, &(b, _)) in self.lines.iter().enumerate() {
            lines[b].push(i as u32);
        }
        Routes {
            clip_hashes: self.lines.iter().map(|&(b, r)| self.banks[b].file.records[r].clip_hash).collect(),
            lengths_s: self.lengths_s(),
            soundbanks: BTreeMap::new(),
            plays: BTreeMap::new(),
            banks: self.banks.iter().zip(lines).map(|(k, lines)| Bank { name: k.name.clone(), entry: k.entry, lines }).collect(),
            unplayed: Vec::new(),
        }
    }

    /// One line's samples.
    pub fn clip_pcm(&self, line: u32) -> Result<Pcm, String> {
        let (bank, rec) = self.line(line).ok_or_else(|| format!("line {line} is past the {} lines", self.lines.len()))?;
        let channels = rec.channels.max(1);
        let samples = match &rec.data {
            WaveData::Embedded(bytes) => bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
            WaveData::Streamed { offset, size, .. } => {
                let pws = bank.pws.as_ref().ok_or_else(|| format!("{} has no .pws", bank.name))?;
                let mut f = std::fs::File::open(pws).map_err(|e| format!("{}: {e}", pws.display()))?;
                f.seek(SeekFrom::Start(u64::from(*offset))).map_err(|e| format!("{}: {e}", pws.display()))?;
                let mut buf = vec![0u8; *size as usize];
                f.read_exact(&mut buf).map_err(|e| format!("{}: {size} bytes at {offset}: {e}", pws.display()))?;
                let mut pcm = if channels >= 2 { decode_ima_stereo(&buf) } else { decode_ima_mono(&buf) };
                pcm.truncate(rec.frames as usize / 2);
                pcm
            }
        };
        Ok(Pcm { samples, rate: rec.sample_rate, channels })
    }

    /// The `.pws` a line streams from, or that its samples are in the bank.
    pub fn stream_of(&self, line: u32) -> String {
        let pws = self.line(line).and_then(|(b, _)| b.pws.as_deref());
        pws.and_then(|p| p.file_name()).map_or("in the bank".to_string(), |n| n.to_string_lossy().into_owned())
    }

    /// What the status bar says a line played from.
    pub fn from_label(&self, line: u32) -> String {
        match self.line(line) {
            Some((bank, rec)) => format!("0x{:08X} from {}", rec.clip_hash, bank.name),
            None => format!("line {line}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_audio::wave::WaveRecord;

    fn audio(dir: &Path, stream: Vec<u8>) -> LevelAudio {
        let pws = dir.join("vo_x.english.pws");
        std::fs::write(&pws, stream).unwrap();
        let rec = |hash, frames, data| WaveRecord { clip_hash: hash, channels: 1, format: 4, sample_rate: 44100, frames, data };
        let banks = vec![
            LevelBank {
                entry: 7,
                name: "embedded".into(),
                file: WavebankFile { bank_hash: 7, stream_name: None, records: vec![rec(1, 2, WaveData::Embedded(vec![1, 0, 2, 0]))] },
                pws: None,
            },
            LevelBank {
                entry: 8,
                name: "streamed".into(),
                file: WavebankFile {
                    bank_hash: 8,
                    stream_name: Some("vo_x.pws".into()),
                    records: vec![rec(2, 20, WaveData::Streamed { offset: 36, size: 36, word_1c: 0 })],
                },
                pws: Some(pws),
            },
        ];
        LevelAudio { banks, lines: vec![(0, 0), (1, 0)] }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_levelaudio_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Lines number across the banks; each bank lists its own; a streamed line lasts its `+0x10` bytes.
    #[test]
    fn lines_run_across_the_banks() {
        let d = dir("lines");
        let a = audio(&d, Vec::new());
        let r = a.routes();
        assert_eq!(r.banks.iter().map(|b| b.lines.clone()).collect::<Vec<_>>(), vec![vec![0], vec![1]]);
        assert_eq!(r.clip_hashes, vec![1, 2]);
        let clips = a.clips();
        assert_eq!(clips[1].bank, "streamed");
        assert_eq!(clips[1].wave_index, Some(1));
        assert!((clips[1].duration_s - 10.0 / 44100.0).abs() < 1e-6);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// An embedded line is its bytes; a streamed line is decoded and stops at its `+0x10`.
    #[test]
    fn a_line_reads_from_where_its_record_says() {
        let d = dir("pcm");
        let mut stream = vec![0u8; 36];
        stream.extend_from_slice(&500i16.to_le_bytes());
        stream.resize(72, 0);
        let a = audio(&d, stream);
        assert_eq!(a.clip_pcm(0).unwrap().samples, vec![1, 2]);
        let pcm = a.clip_pcm(1).unwrap();
        assert_eq!(pcm.samples.len(), 10);
        assert_eq!(pcm.samples[0], 500);
        assert!(a.clip_pcm(2).unwrap_err().contains("past the 2 lines"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
