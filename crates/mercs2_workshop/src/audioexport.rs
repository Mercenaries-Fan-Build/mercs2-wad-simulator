//! Export voice-over clips: each as a 16-bit PCM WAV under the name it shipped with, and a
//! `captions.csv` beside them. The Audio page's EXPORT card and the headless `--audio-export` both
//! write through [`export`].

use mercs2_audio::encode::Pcm16;
use std::path::{Path, PathBuf};

/// One clip to export.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The record index the samples are read by.
    pub wave: u32,
    /// The file name it shipped with (`01595__chris-Banter-Contract-All01-29.wav`).
    pub file: String,
    /// The banks that play it, as listed (`vo_Chris`, or several joined with `;`).
    pub bank: String,
    pub speaker: String,
    pub caption: String,
    /// Length in seconds.
    pub length_s: f32,
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The folder the files are in: `<folder>/<name>`.
    pub dir: PathBuf,
    /// WAVs written.
    pub clips: usize,
}

/// Write `items` into `<folder>/<name>` (`name` as a folder name, `app::export_dir_name`): each
/// clip's samples from `pcm` as `<file>`, then `captions.csv` (`file,wave,bank,speaker,caption,length_s`,
/// RFC 4180 quoting). `progress` hears `(done, total)` after each clip. Existing files of the same
/// names are replaced. A clip whose samples cannot be read, or a file that cannot be written, stops
/// the export with an error naming it; nothing is skipped.
pub fn export(
    items: &[Item],
    folder: &Path,
    name: &str,
    mut pcm: impl FnMut(u32) -> Result<Pcm16, String>,
    mut progress: impl FnMut(usize, usize),
) -> Result<Report, String> {
    if items.is_empty() {
        return Err("nothing to export: no clips are in the chosen scope".into());
    }
    let dir = folder.join(crate::app::export_dir_name(name));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut csv = String::from("file,wave,bank,speaker,caption,length_s\n");
    for (i, item) in items.iter().enumerate() {
        if item.file.contains(['/', '\\']) || item.file.is_empty() {
            return Err(format!("clip {}: {:?} is not a file name", item.wave, item.file));
        }
        let samples = pcm(item.wave).map_err(|e| format!("clip {} ({}): {e}", item.wave, item.file))?;
        let bytes = mercs2_audio::wav::write_pcm16_wav(&samples).map_err(|e| format!("{}: {e}", item.file))?;
        let path = dir.join(&item.file);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        csv.push_str(
            &[
                field(&item.file),
                item.wave.to_string(),
                field(&item.bank),
                field(&item.speaker),
                field(&item.caption),
                format!("{:.3}", item.length_s),
            ]
            .join(","),
        );
        csv.push('\n');
        progress(i + 1, items.len());
    }
    let path = dir.join("captions.csv");
    std::fs::write(&path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Report { dir, clips: items.len() })
}

/// A CSV field: quoted, with quotes doubled, when it holds a comma, a quote or a line break.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ws_audio_export_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn item(wave: u32, file: &str, caption: &str) -> Item {
        Item { wave, file: file.into(), bank: "vo_Chris".into(), speaker: "Chris".into(), caption: caption.into(), length_s: 1.5 }
    }

    /// Each clip lands as a WAV the strict reader reads back as its samples, under the shipped name in
    /// `<folder>/<name>`, and captions.csv lists them in order, a caption with a comma and a quote
    /// quoted; progress counts every clip.
    #[test]
    fn clips_and_captions_land_under_the_folder() {
        let root = tmp("ok");
        let items = [item(3, "00003__a.wav", "Plain."), item(9, "00009__b.wav", "Well, \"yes\".")];
        let mut seen = Vec::new();
        let r = export(
            &items,
            &root,
            "vo Chris",
            |w| Ok(Pcm16 { channels: 1, sample_rate: 22050, samples: vec![w as i16; 4] }),
            |d, t| seen.push((d, t)),
        )
        .expect("exports");
        assert_eq!(r, Report { dir: root.join("vo_Chris"), clips: 2 });
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
        let back = mercs2_audio::wav::read_pcm16_wav(&std::fs::read(r.dir.join("00009__b.wav")).unwrap()).unwrap();
        assert_eq!(back.samples, vec![9; 4]);
        let csv = std::fs::read_to_string(r.dir.join("captions.csv")).unwrap();
        assert_eq!(
            csv,
            "file,wave,bank,speaker,caption,length_s\n00003__a.wav,3,vo_Chris,Chris,Plain.,1.500\n\
             00009__b.wav,9,vo_Chris,Chris,\"Well, \"\"yes\"\".\",1.500\n"
        );
    }

    /// An empty scope, an unreadable clip and a name with a path in it each stop the export, naming it.
    #[test]
    fn a_failure_stops_the_export_naming_it() {
        let root = tmp("bad");
        let ok = |_| Ok(Pcm16 { channels: 1, sample_rate: 8000, samples: vec![1, 2] });
        assert!(export(&[], &root, "x", ok, |_, _| {}).unwrap_err().contains("nothing to export"));
        let e = export(&[item(4, "00004__c.wav", "c")], &root, "x", |_| Err("codec 0x05".into()), |_, _| {}).unwrap_err();
        assert!(e.contains("clip 4") && e.contains("codec 0x05"), "{e}");
        let e = export(&[item(5, "../evil.wav", "c")], &root, "x", ok, |_, _| {}).unwrap_err();
        assert!(e.contains("not a file name"), "{e}");
    }
}
