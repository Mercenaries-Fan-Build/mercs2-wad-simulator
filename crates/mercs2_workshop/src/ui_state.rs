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
    /// What to open, by name: a model into the viewport on the MODELS tab (or with no tab), else the
    /// row of the chosen tab listed under that label — opened as a click on it opens it. A row that
    /// reads on a worker (a string table) is captured once it has read.
    pub load: Option<String>,
    /// The hero for this run, by its model row's label — not saved to the settings. Applied before
    /// `load`, so an ANIMS row plays on it; without one, an ANIMS row opens the hero picker, which
    /// is captured once every skeleton has read.
    pub hero: Option<String>,
    /// On the FACEFX tab: the pose of the opened actor to hold at full weight, by its node name.
    pub face_pose: Option<String>,
    /// On the FACEFX tab: the time (seconds) to pause the opened set's first anim at.
    pub face_time: Option<f32>,
    /// On the EFFECTS tab: the time (seconds) to run the opened effect to and pause at.
    pub fx_time: Option<f32>,
    /// On the UI / GFX tab: the export of the opened movie to play alone, by its export name.
    pub export: Option<String>,
    /// On the UI / GFX tab: the named clip of the opened movie to draw alone, by its path
    /// (`_level0.buttonHolder_mc.button6_mc`).
    pub stage_clip: Option<String>,
    /// On the WORLD LAYERS tab: the entity of the opened layer to select and frame, by its label.
    pub entity: Option<String>,
    /// On the Live page: a `.replay` to play in place of the game, so the page captures with none running.
    pub live_replay: Option<PathBuf>,
    /// On the Live page: the camera's yaw, pitch (radians) and zoom, so a capture can aim it.
    pub live_camera: Option<[f32; 3]>,
    /// The model viewport's orbit: the camera's yaw and pitch (radians), so a capture can aim it.
    pub orbit: Option<[f32; 2]>,
    /// A Shipment folder to open.
    pub shipment: Option<PathBuf>,
    /// The contribution to select in that Shipment, by index.
    pub contribution: Option<usize>,
    /// A Depot workspace folder to read for this run, in place of the saved one.
    pub depot: Option<PathBuf>,
    /// The Depot item to select, by name.
    pub depot_item: Option<String>,
    /// Open `depot_item` in its editor rather than only selecting it.
    #[serde(default)]
    pub depot_open: bool,
    /// Open the Depot's New item dialog with this kind chosen.
    pub depot_new: Option<String>,
    /// With `depot_item`: draw the picker with this label (as its field shows it) open.
    pub depot_pick: Option<String>,
    /// With `depot_item`, an `edit_state_machine` item: the switch node and the state of it to select.
    pub depot_state: Option<[usize; 2]>,
    /// With `depot_item`, a world item: the record of its first world file to select; for an
    /// `activate_layer` item, the layer to draw (0 is the one it adds).
    pub depot_world_row: Option<usize>,
    /// The Depot's viewport draws the shipped asset rather than the item's (the edit_world,
    /// replace_terrain_cell and replace_fx Shipped choice).
    #[serde(default)]
    pub depot_shipped: bool,
    /// With `depot_item`, an effect item: the emitter of its first effect contribution to show.
    pub depot_fx_emitter: Option<usize>,
    /// Hold the right-hand inspector at this scroll offset, in points, to capture what is below.
    pub inspector_scroll: Option<f32>,
    /// With `depot_item`, an add_tiny_geometry item: what the viewport draws, `intact`, `ruined` or
    /// `objects`.
    pub depot_tiny_shown: Option<String>,
    /// Tint a stand-in's object parts by slot.
    #[serde(default)]
    pub depot_tint: bool,
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
    /// Once the state is applied: use this game archive for this run, as Settings' "Use for this
    /// run only…" does, and capture after the reload it starts has landed.
    pub run_wad: Option<PathBuf>,
    /// The same for the reference bundle.
    pub run_data: Option<PathBuf>,
    /// Once the state is applied: choose this level, as the picker does, for this run, and capture
    /// after the reload it starts has landed.
    pub pick_level: Option<String>,
    /// On the Missions page: the script to open, by the name the script list shows (its file stem); the
    /// capture waits for the page's corpus to load.
    pub script: Option<String>,
    /// With `script`: the function whose canvas to open, by the name the members list shows.
    pub function: Option<String>,
    /// On the Driving page: the system to open, by its tab's label. A car and a tank have
    /// "Chassis", "Powertrain", "Steering" and "Coupling"; a boat "Hull" in place of "Chassis"; a
    /// helicopter "Flight", "Yaw", "Attitude" and "Coupling"; a plane "Flyby" and "Coupling".
    pub system: Option<String>,
    /// On the Driving page, with a `load`: the template of the loaded model to read, by name.
    pub template: Option<String>,
    /// On the Driving page, with a `load`: the trailer template to put on the hitch, by name.
    pub trailer: Option<String>,
    /// On the Driving page: whether the exhausts play, "Off" or "On".
    pub exhaust: Option<String>,
    /// On the Driving page, with a `load` of a helicopter: its rotors, "Stopped" or "Spinning".
    pub rotors: Option<String>,
    /// On the Driving page: the overlays to turn on, each by its chip's label ("Show wheels", …);
    /// the others are off.
    pub overlays: Option<Vec<String>>,
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
        if (st.depot_pick.is_some() || st.depot_state.is_some() || st.depot_world_row.is_some()) && st.depot_item.is_none() {
            return Err(format!("{}: `depot_pick`, `depot_state` and `depot_world_row` need a `depot_item`", path.display()));
        }
        if st.contribution.is_some() && st.shipment.is_none() {
            return Err(format!("{}: `contribution` needs a `shipment`", path.display()));
        }
        if st.entity.is_some() && !(st.load.is_some() && st.tab.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("WORLD LAYERS"))) {
            return Err(format!("{}: `entity` needs `\"tab\": \"WORLD LAYERS\"` and a `load`", path.display()));
        }
        let effects_tab = st.load.is_some() && st.tab.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("EFFECTS"));
        if st.fx_time.is_some() && !(effects_tab || (st.depot_item.is_some() && st.depot_open)) {
            return Err(format!("{}: `fx_time` needs `\"tab\": \"EFFECTS\"` and a `load`, or an open `depot_item`", path.display()));
        }
        let ui_load = st.load.is_some() && st.tab.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("UI / GFX"));
        for (key, set) in [("export", st.export.is_some()), ("stage_clip", st.stage_clip.is_some())] {
            if set && !ui_load {
                return Err(format!("{}: `{key}` needs `\"tab\": \"UI / GFX\"` and a `load`", path.display()));
            }
        }
        let facefx_load = st.load.is_some() && st.tab.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("FACEFX"));
        for (key, set) in [("face_pose", st.face_pose.is_some()), ("face_time", st.face_time.is_some())] {
            if set && !facefx_load {
                return Err(format!("{}: `{key}` needs `\"tab\": \"FACEFX\"` and a `load`", path.display()));
            }
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

    /// An entity is selected in a layer the WORLD LAYERS tab opens, so it needs that tab and a `load`.
    #[test]
    fn an_entity_needs_the_layers_tab_and_a_load() {
        let p = write("entity_tab.json", r#"{ "load": "vz_state_pmccon004", "entity": "a", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("entity"));
        let p = write("entity_load.json", r#"{ "tab": "WORLD LAYERS", "entity": "a", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("entity"));
        let p = write("entity_ok.json", r#"{ "tab": "WORLD LAYERS", "load": "vz_state_pmccon004", "entity": "a", "capture": "out.png" }"#);
        assert_eq!(UiState::load(&p).unwrap().entity.as_deref(), Some("a"));
    }

    /// A face pose is held on an actor the FACEFX tab opens, so it needs that tab and a `load`.
    #[test]
    fn a_face_pose_needs_the_facefx_tab_and_a_load() {
        let p = write("face_tab.json", r#"{ "load": "pmc_hum_mattias", "face_pose": "Eat", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("face_pose"));
        let p = write("face_load.json", r#"{ "tab": "FACEFX", "face_pose": "Eat", "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("face_pose"));
        let p = write("face_ok.json", r#"{ "tab": "FACEFX", "load": "pmc_hum_mattias", "face_pose": "Eat", "capture": "out.png" }"#);
        assert_eq!(UiState::load(&p).unwrap().face_pose.as_deref(), Some("Eat"));
        let p = write("face_time_tab.json", r#"{ "load": "Global_Job_Briefing_Mattias", "face_time": 0.5, "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("face_time"));
        let p = write("face_time_ok.json", r#"{ "tab": "FACEFX", "load": "Global_Job_Briefing_Mattias", "face_time": 0.5, "capture": "out.png" }"#);
        assert_eq!(UiState::load(&p).unwrap().face_time, Some(0.5));
        let p = write("fx_time_tab.json", r#"{ "load": "global_explosion_c4", "fx_time": 0.5, "capture": "out.png" }"#);
        assert!(UiState::load(&p).unwrap_err().contains("fx_time"));
        let p = write("fx_time_ok.json", r#"{ "tab": "EFFECTS", "load": "global_explosion_c4", "fx_time": 0.5, "capture": "out.png" }"#);
        assert_eq!(UiState::load(&p).unwrap().fx_time, Some(0.5));
        let p = write("fx_time_depot.json", r#"{ "page": "Depot", "depot_item": "fx", "depot_open": true, "fx_time": 0.5, "capture": "out.png" }"#);
        assert_eq!(UiState::load(&p).unwrap().fx_time, Some(0.5));
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
