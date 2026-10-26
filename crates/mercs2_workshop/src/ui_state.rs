//! `--ui-state <file.json>`: open the window in a given state, capture the frame, and exit.
//!
//! A diagnostic for comparing a page with its design artboard. The file names the state by the same
//! words the window shows — a rail label, a tab, a Lens, an asset name — and every name must
//! resolve: one that does not is an error listing the names it could have been.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The state to open in, applied once the reference data is loaded — or at once when the window
/// opens on Settings because a path chain has no answer. Every field but `capture` is optional; an
/// absent one leaves that part of the window as a normal launch has it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiState {
    /// A rail page by its label ("Inspect", "Shipment", "Settings", …) or "Live".
    pub page: Option<String>,
    /// A Settings section by its label ("Game archive", …).
    pub settings_section: Option<String>,
    /// An asset tab by its label ("MODELS", "AUDIO", …).
    pub tab: Option<String>,
    /// An Inspect Lens by its label ("World", "Characters", "Weapons"), or "All".
    pub lens: Option<String>,
    /// The browser's filter text.
    pub filter: Option<String>,
    /// A model to load into the viewport, by name.
    pub load: Option<String>,
    /// A Shipment folder to open.
    pub shipment: Option<PathBuf>,
    /// The contribution to select in that Shipment, by index.
    pub contribution: Option<usize>,
    /// A bench by its title ("Retarget rig", "Fit to donor"), opened with `donor` as its donor.
    pub bench: Option<String>,
    /// The bench's donor model, by name.
    pub donor: Option<String>,
    /// On the AUDIO tab: the bank to open, by the name the page lists it under.
    pub bank: Option<String>,
    /// On the AUDIO tab: the clip to make current, by record index.
    pub clip: Option<u32>,
    /// On the AUDIO tab: the clip search.
    pub search: Option<String>,
    /// On the AUDIO tab: the clips to select, by record index.
    pub select: Option<Vec<u32>>,
    /// Where to write the captured frame (PNG).
    pub capture: PathBuf,
    /// Frames to let the window settle after the state is applied, before the capture.
    #[serde(default = "default_settle_frames")]
    pub settle_frames: u32,
}

fn default_settle_frames() -> u32 {
    8
}

impl UiState {
    pub fn load(path: &Path) -> Result<UiState, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let st: UiState = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if st.donor.is_some() != st.bench.is_some() {
            return Err(format!("{}: `bench` and `donor` go together", path.display()));
        }
        if st.contribution.is_some() && st.shipment.is_none() {
            return Err(format!("{}: `contribution` needs a `shipment`", path.display()));
        }
        if st.audio() && !st.tab.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("AUDIO")) {
            return Err(format!("{}: `bank`, `clip`, `search` and `select` need `\"tab\": \"AUDIO\"`", path.display()));
        }
        Ok(st)
    }

    /// Whether any AUDIO-tab key is set: those wait for the tab's routing before they apply.
    pub fn audio(&self) -> bool {
        self.bank.is_some() || self.clip.is_some() || self.search.is_some() || self.select.is_some()
    }
}

/// The candidate whose name matches `wanted`, ignoring case; an error naming `what` and listing
/// every name it accepts otherwise.
pub fn pick<T: Copy>(what: &str, wanted: &str, candidates: &[(&str, T)]) -> Result<T, String> {
    candidates
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let names: Vec<&str> = candidates.iter().map(|(n, _)| *n).collect();
            format!("no {what} named {wanted:?}; one of: {}", names.join(", "))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("m2ws_ui_state");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn a_state_file_loads_with_its_defaults() {
        let p = write("ok.json", r#"{ "page": "Inspect", "lens": "Characters", "capture": "out.png" }"#);
        let st = UiState::load(&p).unwrap();
        assert_eq!(st.page.as_deref(), Some("Inspect"));
        assert_eq!(st.settle_frames, 8);
        assert_eq!(st.capture, PathBuf::from("out.png"));
    }

    /// A misspelled field is an error, not a silently ignored setting; so is a missing capture.
    #[test]
    fn unknown_fields_and_a_missing_capture_are_errors() {
        let p = write("typo.json", r#"{ "pgae": "Inspect", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("pgae"));
        let p = write("nocap.json", r#"{ "page": "Inspect" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("capture"));
    }

    #[test]
    fn half_a_pair_is_an_error() {
        let p = write("bench.json", r#"{ "bench": "Retarget rig", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("bench"));
        let p = write("contrib.json", r#"{ "contribution": 0, "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("shipment"));
    }

    /// The AUDIO-tab keys need the AUDIO tab.
    #[test]
    fn audio_keys_need_the_audio_tab() {
        let p = write("bank.json", r#"{ "page": "Inspect", "bank": "vo_Chris", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("AUDIO"));
        let p = write("audio.json", r#"{ "page": "Inspect", "tab": "audio", "bank": "vo_Chris", "clip": 1595, "select": [1595, 1598], "capture": "out.png" }"#);
        let st = UiState::load(&p).unwrap();
        assert!(st.audio());
        assert_eq!((st.clip, st.select.as_deref()), (Some(1595), Some(&[1595u32, 1598][..])));
    }

    #[test]
    fn pick_ignores_case_and_lists_the_names_it_accepts() {
        let c = [("Inspect", 1), ("Saves", 2)];
        assert_eq!(pick("page", "inspect", &c).unwrap(), 1);
        let e = pick("page", "Scene", &c).unwrap_err();
        assert!(e.contains("Scene") && e.contains("Inspect, Saves"), "{e}");
    }
}
