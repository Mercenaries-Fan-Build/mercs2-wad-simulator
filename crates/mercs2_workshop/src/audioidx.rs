//! The captioned audio inventory — the Audio domain's real content.
//!
//! The WAD's own audio catalogue (`index::Kind::Audio`) is the ~97 wavebank / soundbank / sound-table
//! *assets* — the things `add_sound` replaces. This is the complementary view: a per-CLIP inventory
//! (12,988 voiceover lines, most captioned) with speaker, bank, caption text and duration, so the
//! Audio domain browses what the game actually *says*, grouped and searchable, rather than a handful
//! of opaque bank hashes.
//!
//! Bundled at `workshop_data/audio_manifest.json`, copied verbatim from the inventory the user hosts
//! (`https://outbreak.sfo3.digitaloceanspaces.com/mercs2/manifest.json`) — vendored in, per the
//! standing rule, rather than fetched at runtime. A missing file is not fatal: the Audio domain
//! degrades to the WAD's bank assets alone.

use std::path::Path;

/// One inventoried audio clip.
#[derive(Debug, Clone, Default)]
pub struct AudioClip {
    /// The source path/filename, e.g. `vo_Chris/01595__chris-Banter-Contract-All01-29.wav`.
    pub original: String,
    /// The audio bank it belongs to, e.g. `vo_Chris`.
    pub bank: String,
    /// The voice actor / speaker, when known (`Chris`, `Jen`, …). Empty for non-VO.
    pub speaker: String,
    /// The transcribed line, when captioned. Empty otherwise.
    pub caption: String,
    /// Duration in seconds.
    pub duration_s: f32,
}

impl AudioClip {
    /// The short stem for a list row — the filename without its bank prefix or extension.
    pub fn stem(&self) -> &str {
        self.original
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.original)
            .trim_end_matches(".wav")
    }
}

/// The loaded inventory, plus a lowercased search key per clip computed once.
#[derive(Debug, Default, Clone)]
pub struct AudioIndex {
    pub clips: Vec<AudioClip>,
    keys: Vec<String>,
}

impl AudioIndex {
    /// Load `audio_manifest.json` from a `workshop_data/` directory. Missing or malformed → an empty
    /// index (the Audio domain still shows the WAD's bank assets).
    pub fn load(data_home: &Path) -> AudioIndex {
        let mut idx = AudioIndex::default();
        let Ok(text) = std::fs::read_to_string(data_home.join("audio_manifest.json")) else {
            return idx;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return idx };
        let Some(entries) = v.get("entries").and_then(|e| e.as_array()) else { return idx };
        let s = |o: &serde_json::Value, k: &str| {
            o.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string()
        };
        for e in entries {
            let original = s(e, "original");
            if original.is_empty() {
                continue;
            }
            idx.clips.push(AudioClip {
                bank: s(e, "bank"),
                speaker: s(e, "speaker"),
                caption: s(e, "caption"),
                duration_s: e.get("duration_s").and_then(|d| d.as_f64()).unwrap_or(0.0) as f32,
                original,
            });
        }
        idx.clips.sort_by(|a, b| a.bank.cmp(&b.bank).then_with(|| a.original.cmp(&b.original)));
        idx.keys = idx
            .clips
            .iter()
            .map(|c| format!("{} {} {}", c.bank, c.speaker, c.caption).to_lowercase())
            .collect();
        idx
    }

    /// The banks present, each with its clip count — for a grouped browser, in name order.
    pub fn banks(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for c in &self.clips {
            *counts.entry(c.bank.as_str()).or_default() += 1;
        }
        counts.into_iter().map(|(b, n)| (b.to_string(), n)).collect()
    }

    /// Clips in one bank, in path order.
    pub fn by_bank(&self, bank: &str) -> Vec<&AudioClip> {
        self.clips.iter().filter(|c| c.bank == bank).collect()
    }

    /// Clips whose bank, speaker or caption contains every whitespace-separated term of `query`
    /// (case-insensitive), capped at `limit`. An empty query returns nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&AudioClip> {
        let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
        if terms.is_empty() {
            return Vec::new();
        }
        self.clips
            .iter()
            .zip(&self.keys)
            .filter(|(_, key)| terms.iter().all(|t| key.contains(t)))
            .map(|(c, _)| c)
            .take(limit)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx() -> AudioIndex {
        let mut i = AudioIndex {
            clips: vec![
                AudioClip { original: "vo_Chris/1.wav".into(), bank: "vo_Chris".into(), speaker: "Chris".into(), caption: "Now the CIA.".into(), duration_s: 4.9 },
                AudioClip { original: "vo_Chris/2.wav".into(), bank: "vo_Chris".into(), speaker: "Chris".into(), caption: "Rat bastard.".into(), duration_s: 2.0 },
                AudioClip { original: "vo_Jen/1.wav".into(), bank: "vo_Jen".into(), speaker: "Jen".into(), caption: "Now the CIA.".into(), duration_s: 3.1 },
            ],
            keys: vec![],
        };
        i.keys = i.clips.iter().map(|c| format!("{} {} {}", c.bank, c.speaker, c.caption).to_lowercase()).collect();
        i
    }

    #[test]
    fn banks_group_and_search_matches_caption_and_speaker() {
        let i = idx();
        assert_eq!(i.banks(), vec![("vo_Chris".to_string(), 2), ("vo_Jen".to_string(), 1)]);
        assert_eq!(i.by_bank("vo_Jen").len(), 1);
        // Caption term matches across banks; speaker term narrows.
        assert_eq!(i.search("cia", 10).len(), 2);
        assert_eq!(i.search("cia jen", 10).len(), 1);
        assert_eq!(i.search("cia jen", 10)[0].speaker, "Jen");
        assert!(i.search("", 10).is_empty());
        assert_eq!(i.clips[0].stem(), "1");
    }

    /// ★ THE SEAM. The bundled `audio_manifest.json` must parse and carry the inventory — a change in
    /// the hosted schema (renamed fields, a different envelope) fails here rather than leaving the
    /// Audio domain silently bank-only.
    #[test]
    fn the_bundled_manifest_parses_and_carries_the_inventory() {
        let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../workshop_data");
        if !home.join("audio_manifest.json").is_file() {
            eprintln!("SKIPPING: no bundled audio_manifest.json");
            return;
        }
        let i = AudioIndex::load(&home);
        assert!(!i.is_empty(), "the bundled manifest parsed to zero clips — the seam is broken");
        assert!(i.clips.len() > 10_000, "expected ~12,988 clips, got {}", i.clips.len());
        // The hero VO banks must be present and captioned.
        let banks = i.banks();
        for b in ["vo_Chris", "vo_Jen", "vo_mattias"] {
            assert!(banks.iter().any(|(n, _)| n == b), "the manifest lost bank {b}");
        }
        assert!(
            i.clips.iter().filter(|c| !c.caption.is_empty()).count() > 8_000,
            "expected most clips captioned"
        );
        eprintln!("audio inventory: {} clips across {} banks", i.clips.len(), i.banks().len());
    }
}
