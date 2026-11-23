//! The parts every kind's field editor is built from (`docs/modernization/workshop_pages/
//! depot_kind_editors.md`): pickers over what the game ships, optional parts the author adds and
//! removes, list rows and map rows.

use std::collections::BTreeMap;

use mercs2_quartermaster::manifest::{self as m, Wearer};
use mercs2_quartermaster::names::NameTable;
use mercs2_quartermaster::template::{FieldInput, Records};

use crate::gui::theme::{self, FieldState};
use crate::index::AssetIndex;

/// Where the pickers' lists come from: the open game stack's catalog, the decompiled Lua corpus, and
/// the names table.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    pub index: Option<&'a AssetIndex>,
    pub corpus: &'a [(String, String, String)],
    pub names: Option<&'a NameTable>,
    /// The levels the game can boot by name, found beside the game archive (`crate::levels::found`).
    pub levels: &'a [crate::levels::Level],
}

/// A list a picker chooses from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum List {
    Models,
    Textures,
    Animations,
    Layers,
    /// The blocks that hold layers, by PTHS file name: what a layer needle is matched against.
    LayerBlocks,
    StringTables,
    SoundBanks,
    WaveBanks,
    Effects,
    /// The templates that start an effect.
    EffectTemplates,
    TerrainCells,
    /// The shipped scripts, by module name.
    Scripts,
    /// The store stems the retail shader registry loads.
    ShaderStems,
    /// qm's hooks (`data/hooks.tsv`).
    Hooks,
    /// The languages the engine runs in.
    Languages,
    /// The asset type names of the WAD's type table.
    AssetTypes,
    /// The source-rig conventions a retarget names.
    Conventions,
    /// The levels the game can boot by name.
    Levels,
}

/// One entry of a [`List`]: the value written into the manifest and what the list says about it.
#[derive(Clone, Debug)]
pub struct Choice {
    pub value: String,
    pub note: String,
}

const NO_INDEX: &str = "the game index is not loaded";
const NO_CORPUS: &str = "the reference bundle's Lua corpus is not loaded";
const NO_TEMPLATES: &str = "the world's templates are not loaded";
const NO_LEVELS: &str = "the levels beside the game archive are not read";

thread_local! {
    /// `--ui-state`'s `depot_pick`: the label of the picker to draw open, for a capture.
    static OPEN_FOR_CAPTURE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Draw the picker labelled `label` open on its next frame (`--ui-state`'s `depot_pick`).
pub fn open_for_capture(label: &str) {
    OPEN_FOR_CAPTURE.with(|o| *o.borrow_mut() = Some(label.to_string()));
}

fn hex(h: u32) -> String {
    format!("0x{h:08X}")
}

fn loaded<'s>(s: &Sources<'s>) -> Result<&'s AssetIndex, String> {
    match s.index {
        Some(i) if !i.by_type.is_empty() => Ok(i),
        _ => Err(NO_INDEX.to_string()),
    }
}

fn module_name(path: &str) -> &str {
    let leaf = path.rsplit(['/', '\\']).next().unwrap_or(path);
    leaf.strip_suffix(".lua").unwrap_or(leaf)
}

/// Every source rig a `retarget.from` can name.
pub const CONVENTIONS: [mercs2_formats::retarget::SourceRig; 6] = {
    use mercs2_formats::retarget::SourceRig as R;
    [R::ValveBiped, R::Mixamo, R::Unreal, R::CallOfDuty, R::Pandemic, R::Generic]
};

impl List {
    pub fn title(self) -> &'static str {
        match self {
            List::Models => "the game's models",
            List::Textures => "the game's textures",
            List::Animations => "the game's animations",
            List::Layers => "the game's layers",
            List::LayerBlocks => "the game's layer blocks",
            List::StringTables => "the game's string tables",
            List::SoundBanks => "the game's soundbanks",
            List::WaveBanks => "the game's wavebanks",
            List::Effects => "the game's effects",
            List::EffectTemplates => "the templates that start an effect",
            List::TerrainCells => "the game's terrain cells",
            List::Scripts => "the game's scripts",
            List::ShaderStems => "the stems the shader registry loads",
            List::Hooks => "qm's hooks",
            List::Languages => "the engine's languages",
            List::AssetTypes => "the asset types",
            List::Conventions => "the rig conventions",
            List::Levels => "the levels beside the game archive",
        }
    }

    /// The ASET type an asset list holds. The list is read from that type's own rows
    /// (`AssetIndex::by_type`): one hash can name several types (a bank's soundbank, sounddb and
    /// wavebank share the bank's hash), and a catalog category keeps one row per hash.
    fn type_id(self) -> Option<u32> {
        use mercs2_formats::types::*;
        Some(match self {
            List::Models => TYPE_ID_MODEL,
            List::Textures => TYPE_ID_TEXTURE,
            List::Animations => TYPE_ID_ANIMATION,
            List::Layers => TYPE_ID_LAYER,
            List::StringTables => TYPE_ID_STRINGDB,
            List::SoundBanks => TYPE_ID_SOUNDBANK,
            List::WaveBanks => TYPE_ID_WAVEBANK,
            List::Effects => TYPE_ID_EFFECT,
            List::TerrainCells => TYPE_ID_TERRAIN_MESH,
            _ => return None,
        })
    }

    /// Whether a value is matched by the hash it names (`manifest::asset_hash`), as every asset
    /// reference is, rather than by its text.
    fn by_hash(self) -> bool {
        self.type_id().is_some()
    }

    /// Every entry, or why the list cannot be read.
    pub fn choices(self, s: &Sources) -> Result<Vec<Choice>, String> {
        if let Some(ty) = self.type_id() {
            let i = loaded(s)?;
            let mut out: Vec<Choice> = i
                .by_type
                .get(&ty)
                .map(|hs| hs.iter().map(|h| Choice { value: i.names.get(h).cloned().unwrap_or_else(|| hex(*h)), note: hex(*h) }).collect())
                .unwrap_or_default();
            // Named entries first, by name; then the unnamed, by hash.
            out.sort_by(|a, b| (a.value.starts_with("0x"), &a.value).cmp(&(b.value.starts_with("0x"), &b.value)));
            return Ok(out);
        }
        Ok(match self {
            List::EffectTemplates => {
                let i = loaded(s)?;
                if i.effect_starters.is_empty() {
                    return Err(NO_TEMPLATES.to_string());
                }
                let mut by_template: BTreeMap<String, u32> = BTreeMap::new();
                for (effect, starters) in &i.effect_starters {
                    for st in starters {
                        by_template.entry(st.template.clone()).or_insert(*effect);
                    }
                }
                by_template
                    .into_iter()
                    .map(|(t, effect)| Choice {
                        note: format!("starts {}", i.names.get(&effect).cloned().unwrap_or_else(|| hex(effect))),
                        value: t,
                    })
                    .collect()
            }
            List::Scripts => {
                if s.corpus.is_empty() {
                    return Err(NO_CORPUS.to_string());
                }
                let mut names: Vec<&str> = s.corpus.iter().map(|(p, _, _)| module_name(p)).collect();
                names.sort_unstable();
                names.dedup();
                names
                    .into_iter()
                    .map(|n| Choice { value: n.to_string(), note: String::new() })
                    .collect()
            }
            List::ShaderStems => {
                let mut stems: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
                for r in mercs2_quartermaster::shader::registered() {
                    let stem = r.sho.strip_suffix(".sho").unwrap_or(&r.sho).to_string();
                    stems.entry(stem).or_default().insert(r.family.clone());
                }
                stems
                    .into_iter()
                    .map(|(stem, fams)| Choice {
                        value: stem,
                        note: fams.into_iter().collect::<Vec<_>>().join(", "),
                    })
                    .collect()
            }
            List::Hooks => mercs2_quartermaster::hooks::hooks()
                .iter()
                .map(|h| Choice {
                    value: h.id.clone(),
                    note: format!(
                        "{} \u{b7} {}",
                        h.sessions.iter().map(|s| s.token()).collect::<Vec<_>>().join(", "),
                        h.fires
                    ),
                })
                .collect(),
            List::Languages => m::Language::ALL
                .iter()
                .map(|l| Choice { value: l.token().to_string(), note: String::new() })
                .collect(),
            List::AssetTypes => {
                let mut seen: Vec<&str> = Vec::new();
                for (_, id) in mercs2_formats::types::TYPE_HASH_REGISTRY {
                    let n = mercs2_formats::types::type_name(*id);
                    if !seen.contains(&n) {
                        seen.push(n);
                    }
                }
                seen.into_iter()
                    .map(|n| Choice { value: n.to_string(), note: String::new() })
                    .collect()
            }
            List::Conventions => CONVENTIONS
                .iter()
                .map(|r| Choice { value: r.slug().to_string(), note: r.label().to_string() })
                .collect(),
            List::LayerBlocks => {
                let i = loaded(s)?;
                i.layer_blocks
                    .iter()
                    .map(|p| Choice { value: p.rsplit(['/', '\\']).next().unwrap_or(p).to_string(), note: p.clone() })
                    .collect()
            }
            List::Levels => {
                if s.levels.is_empty() {
                    return Err(NO_LEVELS.to_string());
                }
                s.levels
                    .iter()
                    .map(|l| Choice { value: l.name.clone(), note: l.wad.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() })
                    .collect()
            }
            _ => unreachable!("an asset list is answered from the catalog above"),
        })
    }

    /// Whether the list holds `value`, or why the list cannot be read.
    pub fn holds(self, s: &Sources, value: &str) -> Result<bool, String> {
        if self.by_hash() {
            let i = loaded(s)?;
            let h = m::asset_hash(value);
            let ty = self.type_id().expect("a list matched by hash has a type");
            return Ok(i.by_type.get(&ty).is_some_and(|hs| hs.binary_search(&h).is_ok()));
        }
        if self == List::Scripts {
            if s.corpus.is_empty() {
                return Err(NO_CORPUS.to_string());
            }
            return Ok(s.corpus.iter().any(|(p, _, _)| module_name(p) == value));
        }
        // A needle: the build takes the first layer block whose path holds it, case aside.
        if self == List::LayerBlocks {
            let needle = value.to_lowercase();
            return Ok(loaded(s)?.layer_blocks.iter().any(|p| p.to_lowercase().contains(&needle)));
        }
        Ok(self.choices(s)?.iter().any(|c| c.value == value))
    }
}

/// How many entries an open list shows; the search narrows the rest.
const SHOWN: usize = 400;

/// An open picker's list, kept with the key of the sources it was built over.
type CachedChoices = (String, std::sync::Arc<Result<Vec<Choice>, String>>);

/// A required or optional reference chosen from `list`: the value as text (a name, or a bare hash
/// where qm accepts one), a Choose… list under it with a search, and what the value resolves to.
pub fn pick(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    list: List,
    s: &Sources,
    required: bool,
    hint: &str,
) -> bool {
    let mut commit = false;
    let empty = value.trim().is_empty();
    let state = if required && empty { FieldState::Bad } else { FieldState::Neutral };
    commit |= theme::text_field(ui, label, value, hint, state).lost_focus();
    let id = ui.make_persistent_id(("pick", label, list.title()));
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    if OPEN_FOR_CAPTURE.with(|o| o.borrow().as_deref() == Some(label)) {
        OPEN_FOR_CAPTURE.with(|o| *o.borrow_mut() = None);
        open = true;
    }
    if theme::value_column_button(ui, if open { "Close list" } else { "Choose\u{2026}" }).clicked() {
        open = !open;
    }
    if required && empty {
        theme::field_note(ui, FieldState::Bad, "required");
    } else if !empty {
        let h = m::asset_hash(value);
        let at = if list.by_hash() { format!("{} \u{b7} ", hex(h)) } else { String::new() };
        match list.holds(s, value) {
            Ok(true) => theme::field_note(ui, FieldState::Neutral, &format!("{at}in {}", list.title())),
            Ok(false) => theme::field_note(ui, FieldState::Warn, &format!("{at}not in {}", list.title())),
            Err(e) => theme::field_note(ui, FieldState::Neutral, &format!("{at}{e}")),
        }
        if list.by_hash() && m::bare_hash(value).is_some() {
            if let Some(name) = s.names.and_then(|n| n.reverse(h)) {
                theme::field_note(ui, FieldState::Warn, &format!("M0130 \u{2014} this is `{name}`; a name diffs, a hash does not"));
            }
        }
    }
    if open {
        let qid = id.with("q");
        let mut q: String = ui.data(|d| d.get_temp(qid)).unwrap_or_default();
        // The list is built once per open and kept while it stays open over the same sources.
        let key = (
            s.index.map(|i| (i as *const AssetIndex as usize, i.by_type.len(), i.names.len(), i.effect_starters.len())),
            s.corpus.len(),
            s.levels.len(),
        );
        let cid = id.with("choices");
        let cached: Option<CachedChoices> = ui.data(|d| d.get_temp(cid));
        let key_text = format!("{key:?}");
        let built = match cached {
            Some((k, v)) if k == key_text => v,
            _ => {
                let v = std::sync::Arc::new(list.choices(s));
                ui.data_mut(|d| d.insert_temp(cid, (key_text, v.clone())));
                v
            }
        };
        match built.as_ref() {
            Err(e) => theme::field_note(ui, FieldState::Bad, e),
            Ok(all) => {
                theme::text_field(ui, "Search", &mut q, "name or 0x\u{2026}", FieldState::Neutral);
                let ql = q.to_ascii_lowercase();
                let hits: Vec<&Choice> = all
                    .iter()
                    .filter(|c| {
                        ql.is_empty() || c.value.to_ascii_lowercase().contains(&ql) || c.note.to_ascii_lowercase().contains(&ql)
                    })
                    .collect();
                theme::field_note(
                    ui,
                    FieldState::Neutral,
                    &format!("{} of {} shown", hits.len().min(SHOWN), hits.len()),
                );
                egui::ScrollArea::vertical()
                    .id_salt(id.with("rows"))
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for c in hits.iter().take(SHOWN) {
                            let text = if c.note.is_empty() { c.value.clone() } else { format!("{}   {}", c.value, c.note) };
                            if ui.selectable_label(*value == c.value, egui::RichText::new(text).monospace()).clicked() {
                                *value = c.value.clone();
                                commit = true;
                                open = false;
                            }
                        }
                    });
            }
        }
        ui.data_mut(|d| d.insert_temp(qid, q));
    }
    ui.data_mut(|d| d.insert_temp(id, open));
    commit
}

/// An optional part: `+ Add <label>` while it is absent, which sets it to `make()`; while present,
/// its rows (`body`) and a Remove control that takes it back out.
pub fn optional<T>(
    ui: &mut egui::Ui,
    label: &str,
    slot: &mut Option<T>,
    make: impl FnOnce() -> T,
    body: impl FnOnce(&mut egui::Ui, &mut T) -> bool,
) -> bool {
    let mut commit = false;
    match slot {
        None => {
            ui.horizontal(|ui| {
                theme::field_label(ui, label);
                if ui.button(format!("+ Add {}", label.to_lowercase())).clicked() {
                    *slot = Some(make());
                    commit = true;
                }
            });
        }
        Some(v) => {
            commit |= body(ui, v);
            let remove = theme::value_column_button(ui, &format!("Remove {}", label.to_lowercase())).clicked();
            if remove {
                *slot = None;
                commit = true;
            }
        }
    }
    commit
}

/// A list of parts: each row's fields under its own header with move up, move down and remove, and
/// `+ Add <what>` appending `make()`.
pub fn list<T>(
    ui: &mut egui::Ui,
    what: &str,
    items: &mut Vec<T>,
    make: impl Fn() -> T,
    mut row: impl FnMut(&mut egui::Ui, usize, &mut T) -> bool,
) -> bool {
    let mut commit = false;
    let mut action: Option<(usize, i32)> = None;
    let n = items.len();
    for (i, item) in items.iter_mut().enumerate() {
        ui.push_id((what, i), |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                theme::eyebrow(ui, &format!("{what} {i}"));
                if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                    action = Some((i, -1));
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new("Down").small()).clicked() {
                    action = Some((i, 1));
                }
                if ui.small_button("Remove").clicked() {
                    action = Some((i, 0));
                }
            });
            commit |= row(ui, i, item);
        });
    }
    match action {
        Some((i, 0)) => {
            items.remove(i);
            commit = true;
        }
        Some((i, d)) => {
            items.swap(i, (i as i32 + d) as usize);
            commit = true;
        }
        None => {}
    }
    if ui.button(format!("+ Add {what}")).clicked() {
        items.push(make());
        commit = true;
    }
    commit
}

/// A map edited as rows. The rows are held as typed while a key is blank or repeats, with the
/// reason shown, and written once every key is set and unique.
pub fn map_rows<V: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    what: &str,
    map: &mut BTreeMap<String, V>,
    make: impl Fn() -> V,
    mut row: impl FnMut(&mut egui::Ui, &mut String, &mut V) -> bool,
) -> bool {
    #[derive(Clone)]
    struct Held<V> {
        origin: Vec<(String, V)>,
        rows: Vec<(String, V)>,
    }
    let id = ui.make_persistent_id(("map_rows", what));
    let origin: Vec<(String, V)> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut held = ui
        .data_mut(|d| d.get_temp::<Held<V>>(id))
        .filter(|h| h.origin == origin)
        .unwrap_or_else(|| Held { origin: origin.clone(), rows: origin.clone() });
    let mut touched = list(ui, what, &mut held.rows, || (String::new(), make()), |ui, _, (k, v)| row(ui, k, v));
    let mut seen: Vec<&str> = Vec::new();
    let mut problem: Option<String> = None;
    for (k, _) in &held.rows {
        if k.trim().is_empty() {
            problem = Some(format!("a {what} has no key"));
        } else if seen.contains(&k.as_str()) {
            problem = Some(format!("{k} is keyed twice"));
        }
        seen.push(k);
    }
    match &problem {
        Some(p) => {
            theme::field_note(ui, FieldState::Bad, p);
            touched = false;
        }
        None if touched => {
            *map = held.rows.iter().cloned().collect();
            held.origin = held.rows.clone();
        }
        None => {}
    }
    ui.data_mut(|d| d.insert_temp(id, held));
    touched
}

/// The heroes an outfit is offered to, toggled: picked heroes keep their order, a new one goes last,
/// and one hero is written as a plain name. Unpicking the last leaves an empty list (M0141).
pub fn toggle_wearer(wearer: &mut Wearer, hero: &str) {
    let key = m::wearer_table_key;
    let mut heroes: Vec<String> = wearer.heroes().into_iter().map(str::to_string).collect();
    match heroes.iter().position(|h| key(h) == key(hero)) {
        Some(at) => {
            heroes.remove(at);
        }
        None => heroes.push(hero.to_string()),
    }
    *wearer = match heroes.as_slice() {
        [one] => Wearer::One(one.clone()),
        _ => Wearer::Many(heroes),
    };
}

/// The classes an `add_shader` of `family` registers, named `<name><suffix>` in light-class order
/// (`ShaderFamily::class_suffixes`). A class `keep` already holds at the same position keeps its
/// stem and sources.
pub fn shader_classes(
    family: mercs2_quartermaster::shader::ShaderFamily,
    name: &str,
    keep: &[m::ShaderClass],
) -> Vec<m::ShaderClass> {
    family
        .class_suffixes()
        .iter()
        .enumerate()
        .map(|(i, suffix)| match keep.get(i) {
            Some(k) => m::ShaderClass { name: format!("{name}{suffix}"), ..k.clone() },
            None => m::ShaderClass {
                name: format!("{name}{suffix}"),
                stem: name.to_string(),
                shader: m::ShaderSource::asm("src/shader.asm"),
                shader_low: m::ShaderSource::asm("src/shader_low.asm"),
            },
        })
        .collect()
}

/// A shader source: assembly or a compiled blob, and its file.
pub fn shader_source(ui: &mut egui::Ui, label: &str, src: &mut m::ShaderSource, root: &std::path::Path) -> bool {
    let mut commit = false;
    let mut asm = matches!(src, m::ShaderSource::Asm(_));
    if theme::combo_field(ui, &format!("{label} form"), &mut asm, &[(true, "SM3 assembly"), (false, "compiled blob")], FieldState::Neutral) {
        let path = src.path().to_path_buf();
        *src = if asm { m::ShaderSource::asm(path) } else { m::ShaderSource::blob(path) };
        commit = true;
    }
    let filters: &[&str] = if asm { &["asm", "txt"] } else { &["sho", "bin", "fxo"] };
    let mut path = src.path().to_path_buf();
    if crate::quartermaster::source_row(ui, label, &mut path, root, filters) {
        *src = if asm { m::ShaderSource::asm(path) } else { m::ShaderSource::blob(path) };
        commit = true;
    }
    commit
}

/// The kinds of value a template field takes.
const INPUT_KINDS: [(u8, &str); 5] = [(0, "bool"), (1, "integer"), (2, "number"), (3, "text"), (4, "list of numbers")];

fn input_kind(v: &FieldInput) -> u8 {
    match v {
        FieldInput::Bool(_) => 0,
        FieldInput::Int(_) => 1,
        FieldInput::Float(_) => 2,
        FieldInput::Text(_) => 3,
        FieldInput::List(_) => 4,
    }
}

/// One template field value: its kind, then the value. Changing the kind starts the value over.
pub fn field_input(ui: &mut egui::Ui, label: &str, v: &mut FieldInput) -> bool {
    let mut commit = false;
    let mut k = input_kind(v);
    if theme::combo_field(ui, &format!("{label} kind"), &mut k, &INPUT_KINDS, FieldState::Neutral) {
        *v = match k {
            0 => FieldInput::Bool(false),
            1 => FieldInput::Int(0),
            2 => FieldInput::Float(0.0),
            3 => FieldInput::Text(String::new()),
            _ => FieldInput::List(Vec::new()),
        };
        commit = true;
    }
    match v {
        FieldInput::Bool(b) => {
            ui.horizontal(|ui| {
                theme::field_label(ui, label);
                commit |= ui.checkbox(b, "").changed();
            });
        }
        FieldInput::Int(i) => {
            commit |= crate::quartermaster::number_row(ui, label, i, "integer", |v| v.to_string(), |t| {
                t.trim().parse::<i64>().map_err(|_| format!("{t:?} is not an integer"))
            });
        }
        FieldInput::Float(x) => {
            commit |= crate::quartermaster::number_row(ui, label, x, "number", |v| v.to_string(), crate::quartermaster::parse_f64);
        }
        FieldInput::Text(t) => {
            commit |= theme::text_field(ui, label, t, "a name or 0xHASH", FieldState::Neutral).lost_focus();
        }
        FieldInput::List(xs) => {
            commit |= list(ui, "number", xs, || 0.0, |ui, i, x| {
                crate::quartermaster::number_row(ui, &format!("{label} [{i}]"), x, "number", |v| v.to_string(), crate::quartermaster::parse_f64)
            });
        }
    }
    commit
}

/// One record: field (a name or `0xHHHHHHHH`) → value.
pub fn record(ui: &mut egui::Ui, what: &str, fields: &mut BTreeMap<String, FieldInput>) -> bool {
    map_rows(ui, what, fields, || FieldInput::Int(0), |ui, key, v| {
        let mut c = theme::text_field(ui, "Field", key, "name or 0xHASH", FieldState::Neutral).lost_focus();
        c |= field_input(ui, "Value", v);
        c
    })
}

/// A template's components: class → one record, or several for a class held more than once.
pub fn components(ui: &mut egui::Ui, comps: &mut BTreeMap<String, Records>) -> bool {
    map_rows(ui, "component", comps, || Records::One(BTreeMap::new()), |ui, class, recs| {
        let mut c = theme::text_field(ui, "Class", class, "RedEffectComponent", FieldState::Neutral).lost_focus();
        let mut many = matches!(recs, Records::Many(_));
        if theme::combo_field(ui, "Records", &mut many, &[(false, "one"), (true, "several")], FieldState::Neutral) {
            *recs = match std::mem::replace(recs, Records::One(BTreeMap::new())) {
                Records::One(r) if many => Records::Many(vec![r]),
                Records::Many(mut v) if !many => Records::One(if v.is_empty() { BTreeMap::new() } else { v.remove(0) }),
                same => same,
            };
            c = true;
        }
        c |= match recs {
            Records::One(r) => record(ui, "field", r),
            Records::Many(v) => list(ui, "record", v, BTreeMap::new, |ui, _, r| record(ui, "field", r)),
        };
        c
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_a_hero_twice_returns_the_wearer() {
        let mut w = Wearer::One("mattias".into());
        toggle_wearer(&mut w, "chris");
        assert_eq!(w, Wearer::Many(vec!["mattias".into(), "chris".into()]));
        toggle_wearer(&mut w, "chris");
        assert_eq!(w, Wearer::One("mattias".into()));
    }

    #[test]
    fn a_family_change_keeps_the_classes_it_still_has() {
        use mercs2_quartermaster::shader::ShaderFamily;
        let px = shader_classes(ShaderFamily::Pixel, "my", &[]);
        assert_eq!(px.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["my", "my_pl", "my_sl", "my_pl_sl"]);
        let mut first = px[0].clone();
        first.stem = "kept".into();
        let vx = shader_classes(ShaderFamily::Vertex, "my", &[first]);
        assert_eq!(vx.len(), ShaderFamily::Vertex.class_count());
        assert_eq!(vx[0].stem, "kept");
    }

    #[test]
    fn every_list_without_the_game_says_so_or_answers() {
        let s = Sources { index: None, corpus: &[], names: None, levels: &[] };
        for l in [List::Models, List::Textures, List::TerrainCells, List::EffectTemplates] {
            assert_eq!(l.choices(&s).unwrap_err(), NO_INDEX, "{l:?}");
        }
        assert_eq!(List::Scripts.choices(&s).unwrap_err(), NO_CORPUS);
        assert_eq!(List::Levels.choices(&s).unwrap_err(), NO_LEVELS);
        for l in [List::ShaderStems, List::Hooks, List::Languages, List::AssetTypes, List::Conventions] {
            assert!(!l.choices(&s).expect("from qm's own data").is_empty(), "{l:?}");
        }
    }
}
