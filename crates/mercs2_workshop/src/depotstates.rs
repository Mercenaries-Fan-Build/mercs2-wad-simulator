//! The destruction-states editor of an `edit_state_machine` item (board `ShipmentDestruction`):
//! the switch nodes in the navigator, the selected node's states over the viewport, and the selected
//! state's Enter and Exit scripts as lines in the inspector, with the build's checks on the edited
//! machine (`statemachine::findings`).
//!
//! The `states:` file is read and written with qm's own `states::parse` / `states::extract`
//! (`statemachine::read` / `to_yaml`). A file that does not parse is shown with its error and never
//! written. Every write is checked first: the text about to be written must read back to a machine
//! that writes the same text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::SystemTime;

use egui::Color32;
use mercs2_formats::orchestrator::{self as orch, StateDef, StateMachine};
use mercs2_quartermaster::manifest::Contribution;
use mercs2_quartermaster::names::NameTable;

use crate::depot::Item;
use crate::gui::theme::{self, FieldState};
use crate::statemachine::{self as sm, Line, Script, Shipped};

/// A script line's text margin, each side.
const LINE_MARGIN: i8 = 4;

enum Loading {
    Reading(Receiver<Result<Shipped, String>>),
    Ready(Result<Rc<Shipped>, String>),
}

/// Each `states:` file read, by path: its modified time when read and its text and machine.
type YoursCache = HashMap<PathBuf, (SystemTime, Result<Rc<(String, StateMachine)>, String>)>;

/// The editor's reads and its selection.
#[derive(Default)]
pub struct States {
    /// What the game ships, by target, read on a worker the first time it is asked for.
    shipped: RefCell<HashMap<String, Loading>>,
    /// The item's `states:` file, by path, with the modification time it was read at.
    yours: RefCell<YoursCache>,
    /// The selected switch node and state.
    pub node: usize,
    pub state: usize,
    /// What the viewport was last told to show, so it is told again only when it changes.
    shown: RefCell<Option<String>>,
}

/// An item's `edit_state_machine`: its index, target and `states:` path.
pub fn of(item: &Item) -> Option<(usize, &str, &Path)> {
    item.file.as_ref().ok()?.contributions.iter().enumerate().find_map(|(i, c)| match c {
        Contribution::EditStateMachine { target, states } => Some((i, target.as_str(), states.as_path())),
        _ => None,
    })
}

impl States {
    /// Forget what was read from the game (a different stack).
    pub fn forget_game(&self) {
        self.shipped.borrow_mut().clear();
    }

    /// What the game ships for `target`: `None` while it is read.
    pub fn shipped(&self, game: &[PathBuf], target: &str) -> Option<Result<Rc<Shipped>, String>> {
        let mut map = self.shipped.borrow_mut();
        let entry = map.entry(target.to_string()).or_insert_with(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (wads, t) = (game.to_vec(), target.to_string());
            std::thread::spawn(move || {
                let _ = tx.send(if wads.is_empty() { Err("no game archive is open".to_string()) } else { sm::shipped(&wads, &t) });
            });
            Loading::Reading(rx)
        });
        if let Loading::Reading(rx) = entry {
            match rx.try_recv() {
                Ok(r) => *entry = Loading::Ready(r.map(Rc::new)),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => *entry = Loading::Ready(Err("the reader stopped without an answer".into())),
            }
        }
        match entry {
            Loading::Ready(r) => Some(r.clone()),
            Loading::Reading(_) => None,
        }
    }

    /// The item's machine at `abs`: `None` when the file does not exist. Re-read when it changes.
    pub fn yours(&self, abs: &Path) -> Option<Result<Rc<(String, StateMachine)>, String>> {
        let mtime = std::fs::metadata(abs).and_then(|m| m.modified()).ok()?;
        if let Some((t, r)) = self.yours.borrow().get(abs) {
            if *t == mtime {
                return Some(r.clone());
            }
        }
        let r = sm::read(abs).map(Rc::new);
        self.yours.borrow_mut().insert(abs.to_path_buf(), (mtime, r.clone()));
        Some(r)
    }

    /// Tell the viewport again on the next frame (it had no model to show it on).
    pub fn reshow(&self) {
        *self.shown.borrow_mut() = None;
    }

    /// The node and state shown, held inside `machine`.
    pub fn selection(&self, machine: &StateMachine) -> (usize, usize) {
        let node = self.node.min(machine.nodes.len().saturating_sub(1));
        let states = machine.nodes.get(node).map_or(0, |n| n.states.len());
        (node, self.state.min(states.saturating_sub(1)))
    }

    /// The state each node stands in for the viewport: each node's own default, the selected node at
    /// the selected state. `Some` only when that differs from what the viewport was last told.
    pub fn preview_change(&self, text: &str, machine: &StateMachine) -> Option<Vec<usize>> {
        let (node, state) = self.selection(machine);
        let mut chosen: Vec<usize> = machine.nodes.iter().map(orch::default_state_index).collect();
        if let Some(c) = chosen.get_mut(node) {
            *c = state;
        }
        let key = format!("{text}\n{chosen:?}");
        let mut shown = self.shown.borrow_mut();
        if shown.as_deref() == Some(key.as_str()) {
            return None;
        }
        *shown = Some(key);
        Some(chosen)
    }
}

/// `machine` as the `states:` text to write over `prior`, once it is checked to read back as itself.
/// Over an existing file only what changed is rewritten, so the rest of the author's text stays
/// ([`mercs2_quartermaster::lossless`]).
pub fn checked_text(machine: &StateMachine, prior: &str, names: Option<&NameTable>) -> Result<String, String> {
    if !prior.is_empty() {
        return mercs2_quartermaster::lossless::rewrite(prior, mercs2_quartermaster::Format::Yaml, machine, mercs2_quartermaster::states::parse, |m| {
            Ok(sm::to_yaml(m, prior, names))
        });
    }
    let text = sm::to_yaml(machine, prior, names);
    let back = mercs2_quartermaster::states::parse(&text).map_err(|e| format!("the edited machine does not read back: {e}"))?;
    if sm::to_yaml(&back, &text, names) != text {
        return Err("the edited machine does not read back as itself; the file is left as it was".into());
    }
    Ok(text)
}

fn spell(v: u32, names: Option<&NameTable>) -> String {
    sm::spell(v, names.and_then(|n| n.reverse(v)))
}

fn state_label(h: u32, names: Option<&NameTable>) -> String {
    orch::state_name(h).map(String::from).or_else(|| names.and_then(|n| n.reverse(h)).map(String::from)).unwrap_or_else(|| format!("0x{h:08X}"))
}

/// The switch nodes, for the navigator: each node's name and state count, the selected one marked.
/// Returns the node clicked.
pub fn nodes(ui: &mut egui::Ui, st: &States, machine: &StateMachine, names: Option<&NameTable>) -> Option<usize> {
    let mut clicked = None;
    let (sel, _) = st.selection(machine);
    ui.horizontal(|ui| {
        theme::eyebrow(ui, "SWITCH NODES");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(machine.nodes.len().to_string()).monospace().color(theme::DIM));
        });
    });
    theme::scroll_vertical().id_salt("depot-sm-nodes").auto_shrink([false, false]).max_height(ui.available_height() - 24.0).show(ui, |ui| {
        for (i, n) in machine.nodes.iter().enumerate() {
            let name = names.and_then(|t| t.reverse(n.name_hash)).map(String::from).unwrap_or_else(|| format!("0x{:08X}", n.name_hash));
            let text = format!("{name}   {} states", n.states.len());
            if ui.selectable_label(i == sel, egui::RichText::new(text).monospace()).clicked() {
                clicked = Some(i);
            }
        }
    });
    let slots = machine.switch_slots.len();
    let states: usize = machine.nodes.iter().map(|n| n.states.len()).sum();
    ui.label(egui::RichText::new(format!("{slots} slots in SWIT \u{b7} {states} states")).small().color(theme::DIM));
    clicked
}

/// The viewport's state chips for the selected node and what the edited machine shows. Returns the
/// state clicked.
pub fn chips(ui: &mut egui::Ui, st: &States, machine: &StateMachine, names: Option<&NameTable>) -> Option<usize> {
    let mut clicked = None;
    let (node, state) = st.selection(machine);
    let n = machine.nodes.get(node)?;
    let showing = n.states.get(state).map(|s| state_label(s.name_hash, names)).unwrap_or_default();
    ui.label(theme::disp_text(format!("SHOWING \u{b7} {}", showing.to_uppercase()), theme::style_eyebrow(), theme::BRASS));
    ui.horizontal_wrapped(|ui| {
        for (j, s) in n.states.iter().enumerate() {
            if theme::pill(ui, &state_label(s.name_hash, names), j == state).clicked() {
                clicked = Some(j);
            }
        }
    });
    ui.label(
        egui::RichText::new(format!("node {node} \u{b7} 0x{:08X} \u{b7} {} states", n.name_hash, n.states.len()))
            .monospace()
            .color(theme::DIM),
    );
    clicked
}

/// What the inspector asks the page to do.
pub enum Ask {
    /// Write this machine over the file (`checked_text` first).
    Write(Box<StateMachine>),
}

/// One script's lines: each line editable as typed (`statemachine::parse_line`), with what its token
/// is in this machine, a Remove control, and `+ Add line`. Returns the new lines on a change.
fn script(
    ui: &mut egui::Ui,
    title: &str,
    tokens: &[u32],
    node_hash: u32,
    machine: &StateMachine,
    shipped: Option<&Shipped>,
    names: Option<&NameTable>,
) -> Option<Vec<Line>> {
    let mut lines = sm::lines(tokens);
    let mut changed = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        theme::eyebrow(ui, title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("{} lines", lines.len())).small().color(theme::DIM));
        });
    });
    let hier: &[orch::HierNode] = shipped.map_or(&[], |s| s.hier.as_slice());
    let mut remove = None;
    for (i, line) in lines.iter_mut().enumerate() {
        ui.push_id((title, i), |ui| {
            let id = ui.make_persistent_id("line");
            let shown = sm::line_text(*line, |v| spell(v, names));
            let mut text: String = ui.data(|d| d.get_temp::<(String, String)>(id)).filter(|(o, _)| *o == shown).map(|(_, t)| t).unwrap_or_else(|| shown.clone());
            let parsed = sm::parse_line(&text);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{i:>2}")).monospace().color(theme::FAINT));
                let state = if parsed.is_ok() { FieldState::Neutral } else { FieldState::Bad };
                // The Remove button's own width, so the row ends at the column's edge.
                let remove_w = ui
                    .painter()
                    .layout_no_wrap("Remove".to_string(), egui::TextStyle::Button.resolve(ui.style()), theme::TX)
                    .size()
                    .x
                    + 2.0 * ui.spacing().button_padding.x
                    + ui.spacing().item_spacing.x;
                let w = (ui.available_width() - remove_w - 2.0 * LINE_MARGIN as f32).max(80.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .margin(egui::Margin::symmetric(LINE_MARGIN, 2))
                        .desired_width(w)
                        .text_color(if state == FieldState::Bad { theme::BAD } else { theme::TX }),
                );
                if r.lost_focus() {
                    if let Ok(l) = &parsed {
                        if *l != *line {
                            *line = *l;
                            changed = true;
                        }
                    }
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
            match &parsed {
                Err(e) => {
                    ui.label(egui::RichText::new(e).small().color(theme::BAD));
                }
                Ok(l) => {
                    let v = match l {
                        Line::Push(v) | Line::Call(v) | Line::Raw(v) => Some(*v),
                        Line::End => None,
                    };
                    if let Some(note) = v.map(|v| sm::annotate(v, node_hash, machine, hier, names)).filter(|n| !n.is_empty()) {
                        ui.label(egui::RichText::new(format!("    {note}")).small().color(theme::DIM));
                    }
                }
            }
            ui.data_mut(|d| d.insert_temp(id, (shown, text)));
        });
    }
    if let Some(i) = remove {
        lines.remove(i);
        changed = true;
    }
    if ui.button("+ Add line").clicked() {
        lines.push(Line::End);
        changed = true;
    }
    changed.then_some(lines)
}

/// The inspector's Destruction states card for an item whose file read as `yours`.
pub fn editor(
    ui: &mut egui::Ui,
    st: &States,
    yours: &(String, StateMachine),
    shipped: Option<&Result<Rc<Shipped>, String>>,
    names: Option<&NameTable>,
) -> (Option<Ask>, Option<String>) {
    let (_, machine) = yours;
    let mut ask = None;
    let mut error = None;
    let (node, state) = st.selection(machine);
    let shipped_ok = shipped.and_then(|r| r.as_ref().ok()).map(|r| &**r);
    let Some(n) = machine.nodes.get(node) else {
        ui.label(egui::RichText::new("The machine has no switch node.").color(theme::BAD));
        return (None, None);
    };
    let Some(s) = n.states.get(state) else {
        ui.label(egui::RichText::new(format!("Node {node} has no state.")).color(theme::DIM));
        if ui.button("+ Add state").clicked() {
            let mut m = machine.clone();
            m.nodes[node].states.push(StateDef { name_hash: orch::STATE_INIT, enter: Vec::new(), exit: Vec::new() });
            ask = Some(Ask::Write(Box::new(m)));
        }
        return (ask, None);
    };

    let choices: Vec<u32> = match shipped_ok {
        Some(sh) => sm::state_choices(&sh.machine),
        None => orch::STATE_VOCABULARY.iter().filter(|(_, n)| !n.is_empty()).map(|(h, _)| *h).collect(),
    };
    let mut options: Vec<(u32, String)> = choices.iter().map(|h| (*h, state_label(*h, names))).collect();
    if !choices.contains(&s.name_hash) {
        options.insert(0, (s.name_hash, state_label(s.name_hash, names)));
    }
    let opt_refs: Vec<(u32, &str)> = options.iter().map(|(h, t)| (*h, t.as_str())).collect();
    let mut name = s.name_hash;
    if theme::combo_field(ui, "State", &mut name, &opt_refs, FieldState::Neutral) {
        let mut m = machine.clone();
        m.nodes[node].states[state].name_hash = name;
        ask = Some(Ask::Write(Box::new(m)));
    }
    theme::kv(ui, "hash", egui::RichText::new(format!("0x{:08X}", s.name_hash)).monospace());
    ui.horizontal(|ui| {
        if ui.button("+ Add state").clicked() {
            let fresh = choices.iter().copied().find(|h| !n.states.iter().any(|x| x.name_hash == *h)).unwrap_or(orch::STATE_INIT);
            let mut m = machine.clone();
            m.nodes[node].states.push(StateDef { name_hash: fresh, enter: Vec::new(), exit: Vec::new() });
            ask = Some(Ask::Write(Box::new(m)));
        }
        if ui.button("Remove this state").clicked() {
            let mut m = machine.clone();
            m.nodes[node].states.remove(state);
            ask = Some(Ask::Write(Box::new(m)));
        }
    });

    for (title, which, toks) in [("ENTER SCRIPT", Script::Enter, &s.enter), ("EXIT SCRIPT", Script::Exit, &s.exit)] {
        if let Some(lines) = script(ui, title, toks, n.name_hash, machine, shipped_ok, names) {
            let mut m = machine.clone();
            match sm::set_script(&mut m, node, state, which, &lines) {
                Ok(()) => ask = Some(Ask::Write(Box::new(m))),
                Err(e) => error = Some(e),
            }
        }
    }

    ui.add_space(8.0);
    theme::eyebrow(ui, "SWITCH SLOTS");
    let mut slots = machine.switch_slots.clone();
    let mut slots_changed = false;
    // Each slot as the manifest spells a token (`statemachine::spell`), held as typed until it loses
    // focus; every box as wide as the widest, so the grid wraps inside the column.
    let shown: Vec<String> = slots.iter().map(|v| spell(*v, names)).collect();
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let slot_w = shown.iter().map(|t| ui.painter().layout_no_wrap(t.clone(), font.clone(), theme::TX).size().x).fold(0.0f32, f32::max)
        + 2.0 * LINE_MARGIN as f32
        + 8.0;
    ui.horizontal_wrapped(|ui| {
        for (i, v) in slots.iter_mut().enumerate() {
            let id = ui.make_persistent_id(("switch_slot", i));
            let mut t: String = ui
                .data(|d| d.get_temp::<(String, String)>(id))
                .filter(|(o, _)| *o == shown[i])
                .map(|(_, t)| t)
                .unwrap_or_else(|| shown[i].clone());
            let r = ui.add_sized(
                [slot_w, 20.0],
                egui::TextEdit::singleline(&mut t).font(egui::TextStyle::Monospace).margin(egui::Margin::symmetric(LINE_MARGIN, 2)),
            );
            if r.lost_focus() && !t.trim().is_empty() {
                let x = sm::word(t.trim());
                if x != *v {
                    *v = x;
                    slots_changed = true;
                }
            }
            ui.data_mut(|d| d.insert_temp(id, (shown[i].clone(), t)));
        }
    });
    if slots_changed {
        let mut m = machine.clone();
        m.switch_slots = slots;
        ask = Some(Ask::Write(Box::new(m)));
    }
    (ask, error)
}

/// The checks card: what `qm build` would say about the edited machine.
pub fn checks(ui: &mut egui::Ui, machine: &StateMachine, shipped: Option<&Result<Rc<Shipped>, String>>) {
    match shipped {
        None => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            ui.label(egui::RichText::new("Reading the model from the game\u{2026}").small().color(theme::DIM));
        }
        Some(Err(e)) => {
            ui.label(egui::RichText::new(e).small().color(theme::BAD));
        }
        Some(Ok(sh)) => {
            let found = sm::findings(machine, sh);
            if found.is_empty() {
                ui.label(egui::RichText::new("0 problems").color(theme::GOOD));
            }
            for f in found {
                let colour: Color32 = if f.error { theme::BAD } else { theme::BRASS };
                ui.label(egui::RichText::new(format!("{} {}", f.code, f.message)).small().color(colour));
            }
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!(
                        "shipped: {} nodes \u{b7} {} states \u{b7} {} rung(s)",
                        sh.machine.nodes.len(),
                        sh.machine.nodes.iter().map(|n| n.states.len()).sum::<usize>(),
                        sh.rungs
                    ))
                    .small()
                    .color(theme::DIM),
                )
                .wrap(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retail destroyer's machine, as Start from shipped writes it, is written again unchanged
    /// by an editor write that changes nothing: no node, state or token is dropped on the way.
    #[cfg(feature = "retail")]
    #[test]
    fn an_unedited_retail_machine_writes_back_byte_identical() {
        let wad = PathBuf::from(crate::app::retail_vz_wad());
        let shipped = sm::shipped(&[wad], "al_veh_boat_destroyer").expect("the destroyer has a machine");
        let first = checked_text(&shipped.machine, "", None).expect("the shipped machine writes");
        let back = mercs2_quartermaster::states::parse(&first).unwrap();
        let again = checked_text(&back, &first, None).expect("it writes again");
        assert_eq!(again, first);
        assert_eq!(back.nodes.len(), shipped.machine.nodes.len());
        let tokens = |m: &StateMachine| m.nodes.iter().flat_map(|n| n.states.iter().flat_map(|s| s.enter.iter().chain(&s.exit).copied())).collect::<Vec<u32>>();
        assert_eq!(tokens(&back), tokens(&shipped.machine));
        assert_eq!(back.switch_slots, shipped.machine.switch_slots);
    }

    #[test]
    fn a_checked_write_reads_back_as_itself() {
        let m = StateMachine {
            switch_slots: vec![0, 1],
            nodes: vec![orch::SwitchNodeDef {
                name_hash: 0x1234_5678,
                states: vec![StateDef { name_hash: orch::STATE_INIT, enter: vec![1, 7, 2, 0xDEAD_BEEF, 3], exit: vec![] }],
            }],
        };
        let text = checked_text(&m, "", None).expect("a machine of whole tokens writes");
        let back = mercs2_quartermaster::states::parse(&text).unwrap();
        assert_eq!(back.nodes[0].states[0].enter, m.nodes[0].states[0].enter);
        assert_eq!(back.switch_slots, m.switch_slots);
    }

    /// A states file with the author's comments takes an edit to one script in place.
    #[test]
    fn an_edit_keeps_the_comments_of_a_hand_written_states_file() {
        let m = StateMachine {
            switch_slots: vec![0, 1],
            nodes: vec![orch::SwitchNodeDef {
                name_hash: 0x1234_5678,
                states: vec![StateDef { name_hash: orch::STATE_INIT, enter: vec![1, 7, 2, 0xDEAD_BEEF, 3], exit: vec![] }],
            }],
        };
        let ours = checked_text(&m, "", None).expect("a machine of whole tokens writes");
        let hand = format!("# the turret's switch\n{ours}");
        let mut next = mercs2_quartermaster::states::parse(&hand).unwrap();
        next.nodes[0].states[0].exit = vec![3];
        let text = checked_text(&next, &hand, None).unwrap_or_else(|e| panic!("{e}"));
        assert!(text.starts_with("# the turret's switch\n"), "{text}");
        assert_eq!(text, format!("# the turret's switch\n{}", checked_text(&next, "", None).unwrap()));
    }
}
