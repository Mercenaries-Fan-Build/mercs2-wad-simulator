//! Every wavebank clip of the open archives by its clip hash — what a FaceFX take's voice line is
//! found by. A take is named for its line: `pandemic_hash_m2` of the anim's name is the clip hash of
//! the line in a wavebank (1,116 of the 1,122 retail takes; `mercs2_probe facefx --all`).
//!
//! Built once on a worker ([`spawn`]): every wavebank of every WAD, the stack in load order (a later
//! WAD's clip replaces an earlier one, the game's patch rule) then the side WADs for the clips the
//! stack lacks. Only the clip hashes are kept; a line's samples are read when it plays ([`pcm`]).

use std::collections::HashMap;
use std::fs::File;

use mercs2_engine::audio::wave::{Wavebank, WavebankFile};
use mercs2_formats::types::TYPE_HASH_WAVEBANK;

/// Where a clip's samples are.
#[derive(Clone, Debug, PartialEq)]
pub enum Voice {
    /// In the records of wavebank `bank` of the WAD at `path`.
    Embedded { path: String, bank: u32 },
    /// Record `wave` of the `vo_stream.english` bank, whose samples are in its `.pws`
    /// ([`crate::vostream::VoStream::clip_pcm`]).
    Stream { wave: u32 },
    /// A record of another streamed bank, whose `.pws` nothing here reads.
    OtherStream { bank: u32 },
}

#[derive(Default)]
pub struct Voices {
    pub by_clip: HashMap<u32, Voice>,
}

/// Read every wavebank of `stack` (in load order) then `side`, on a worker thread.
pub fn spawn(stack: Vec<String>, side: Vec<String>) -> std::sync::mpsc::Receiver<Result<Voices, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(read(&stack, &side));
    });
    rx
}

fn read(stack: &[String], side: &[String]) -> Result<Voices, String> {
    let vo_stream = mercs2_formats::hash::pandemic_hash_m2("vo_stream.english");
    let mut v = Voices::default();
    for (i, path) in stack.iter().chain(side).enumerate() {
        let stacked = i < stack.len();
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        let mut blocks: Vec<u16> = arch
            .aset
            .iter()
            .filter(|a| a.type_id == mercs2_formats::types::TYPE_ID_WAVEBANK)
            .map(|a| a.block_index())
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        for bi in blocks {
            let block = mercs2_formats::sges::decompress_block(&mut f, &arch.indx, bi).map_err(|e| format!("{path} block {bi}: {e}"))?;
            for (e, span) in mercs2_formats::ucfx::block_chunk_spans(&block).map_err(|e| format!("{path} block {bi}: {e}"))? {
                if e.type_hash != TYPE_HASH_WAVEBANK {
                    continue;
                }
                let body = mercs2_formats::ucfx::extract_chunk_body(&block[span], b"data")
                    .ok_or_else(|| format!("{path} block {bi}: wavebank 0x{:08X} has no data chunk", e.name_hash))?;
                let file = WavebankFile::parse(&body).map_err(|err| format!("{path} block {bi}: wavebank 0x{:08X}: {err:?}", e.name_hash))?;
                for (wave, r) in file.records.iter().enumerate() {
                    let voice = match (&file.stream_name, e.name_hash == vo_stream) {
                        (None, _) => Voice::Embedded { path: path.clone(), bank: e.name_hash },
                        (Some(_), true) => Voice::Stream { wave: wave as u32 },
                        (Some(_), false) => Voice::OtherStream { bank: e.name_hash },
                    };
                    if stacked || !v.by_clip.contains_key(&r.clip_hash) {
                        v.by_clip.insert(r.clip_hash, voice);
                    }
                }
            }
        }
    }
    Ok(v)
}

/// The samples of embedded clip `clip` of wavebank `bank` in the WAD at `path`: interleaved PCM16,
/// sample rate, channels.
pub fn pcm(path: &str, bank: u32, clip: u32) -> Result<(Vec<i16>, u32, u8), String> {
    let mut w = mercs2_engine::wad::open(path)?;
    let c = mercs2_engine::wad::extract_container_typed(&mut w, bank, TYPE_HASH_WAVEBANK)?;
    let body = mercs2_formats::ucfx::extract_chunk_body(&c, b"data").ok_or_else(|| format!("wavebank 0x{bank:08X}: no data chunk"))?;
    let bank_clips = Wavebank::parse(&body).map_err(|e| format!("wavebank 0x{bank:08X}: {e:?}"))?;
    let d = bank_clips.clip_by_hash(clip).ok_or_else(|| format!("wavebank 0x{bank:08X} holds no clip 0x{clip:08X}"))?;
    Ok((d.samples.clone(), d.sample_rate, d.channels))
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    /// `Mattias.Greeting01`'s line is a clip of an open wavebank, and it lasts as long as the take
    /// (its end, 1.574 s) to within a tenth of a second.
    #[test]
    fn a_take_finds_its_voice_line() {
        let vz = crate::app::retail_vz_wad();
        let dir = std::path::Path::new(&vz).parent().expect("vz.wad has a folder").to_path_buf();
        let stack: Vec<String> = ["Loading.wad", "vz.wad", "vz-patch.wad", "English.wad"]
            .iter()
            .map(|f| dir.join(f))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let v = super::read(&stack, &[]).unwrap_or_else(|e| panic!("{e}"));
        let clip = mercs2_formats::hash::pandemic_hash_m2("Mattias.Greeting01");
        let (samples, rate, ch) = match v.by_clip.get(&clip) {
            Some(super::Voice::Embedded { path, bank }) => super::pcm(path, *bank, clip).unwrap_or_else(|e| panic!("{e}")),
            other => panic!("Mattias.Greeting01: {other:?}"),
        };
        let secs = samples.len() as f32 / ch.max(1) as f32 / rate as f32;
        assert!((secs - 1.574).abs() < 0.1, "the line lasts {secs} s");
    }
}
