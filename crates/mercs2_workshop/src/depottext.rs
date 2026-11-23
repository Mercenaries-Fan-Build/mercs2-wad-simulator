//! The string-table editors of an item's `edit_stringdb`, `add_stringdb_keys`,
//! `replace_stringdb_text` and `add_language` contributions: the file's rows beside the shipped
//! table's text, the shipped table to pick rows from, and the table as `qm build` writes it.
//!
//! Every line is read and written with qm's own line grammar (`build::string_edit_line` /
//! `string_edit_text`, `build::text_pair_line` / `text_pair_text`). An edit rewrites only its own
//! line; comments, blank lines and the other lines are kept byte for byte. Before a write the new
//! text is read back line by line and must give exactly the rows intended, and the file on disk must
//! still be the text the edit was made from. The table after the build comes from qm
//! (`build::shipment_string_tables`, `build::language_table`), read on a worker.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::SystemTime;

use mercs2_formats::stringdb;
use mercs2_quartermaster::build as qb;
use mercs2_quartermaster::manifest::Contribution;

use crate::depot::Item;
use crate::gui::theme::{self, FieldState};
use crate::kindform::Sources;

/// Which file a text contribution's rows are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `edit_stringdb`: key → new text, for keys the table has.
    Edit,
    /// `add_stringdb_keys`: key → text, for keys the table lacks.
    Add,
    /// `replace_stringdb_text`: old text → new text.
    Replace,
    /// `add_language`: key → translation, against the base table.
    Language,
}

impl Which {
    fn line(self, raw: &str) -> Result<Option<(String, String)>, String> {
        match self {
            Which::Replace => qb::text_pair_line(raw),
            _ => qb::string_edit_line(raw),
        }
    }

    fn text(self, left: &str, right: &str) -> Result<String, String> {
        match self {
            Which::Replace => qb::text_pair_text(left, right),
            _ => qb::string_edit_text(left, right),
        }
    }

    /// What `qm build` says of the whole file: `Err` with its refusal.
    pub fn whole(self, text: &str, file: &str) -> Result<(), String> {
        match self {
            Which::Replace => qb::parse_text_pairs(text, file).map(|_| ()),
            _ => qb::parse_string_edits(text).map(|_| ()),
        }
    }

    fn columns(self) -> [&'static str; 3] {
        match self {
            Which::Edit => ["KEY", "SHIPPED", "YOURS"],
            Which::Add => ["NEW KEY", "SHIPPED", "TEXT"],
            Which::Replace => ["OLD TEXT", "KEYS", "NEW TEXT"],
            Which::Language => ["KEY", "BASE", "TRANSLATION"],
        }
    }

    fn use_label(self) -> Option<&'static str> {
        match self {
            Which::Edit => Some("Edit"),
            Which::Add => None,
            Which::Replace => Some("Replace"),
            Which::Language => Some("Translate"),
        }
    }
}

/// A text contribution: which file it is, the table its rows are read against, and the file.
pub fn of(c: &Contribution) -> Option<(Which, String, &Path)> {
    match c {
        Contribution::EditStringDb { target, strings } => Some((Which::Edit, target.clone(), strings.as_path())),
        Contribution::AddStringDbKeys { target, strings } => Some((Which::Add, target.clone(), strings.as_path())),
        Contribution::ReplaceStringDbText { target, pairs } => Some((Which::Replace, target.clone(), pairs.as_path())),
        Contribution::AddLanguage { strings, base, .. } => {
            Some((Which::Language, base.clone().unwrap_or_else(|| qb::LANGUAGE_BASE.to_string()), strings.as_path()))
        }
        _ => None,
    }
}

/// One line of the file: its text as stored (without the `\n`; a CRLF file's `\r` kept), and the
/// row it reads as — `None` for a blank or `#` line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub raw: String,
    pub row: Option<(String, String)>,
}

/// A strings or pairs file, line by line.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub which: Which,
    pub lines: Vec<Line>,
}

/// Read `text` line by line with qm's grammar. A line that does not read is an error naming it.
pub fn read(which: Which, text: &str) -> Result<Doc, String> {
    let lines = text
        .split('\n')
        .enumerate()
        .map(|(n, raw)| {
            which.line(raw).map(|row| Line { raw: raw.to_string(), row }).map_err(|m| format!("line {}: {m}", n + 1))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Doc { which, lines })
}

impl Doc {
    /// The file's text.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| l.raw.as_str()).collect::<Vec<_>>().join("\n")
    }

    fn crlf(&self) -> bool {
        self.lines.iter().any(|l| l.raw.ends_with('\r'))
    }

    fn line_for(&self, left: &str, right: &str) -> Result<String, String> {
        let mut raw = self.which.text(left, right)?;
        if self.crlf() {
            raw.push('\r');
        }
        Ok(raw)
    }

    /// Line `i` set to `left` / `right`; every other line as it is.
    pub fn set(&self, i: usize, left: &str, right: &str) -> Result<Doc, String> {
        let raw = self.line_for(left, right)?;
        let mut d = self.clone();
        let l = d.lines.get_mut(i).ok_or_else(|| format!("line {} is not in the file", i + 1))?;
        *l = Line { raw, row: Some((left.to_string(), right.to_string())) };
        Ok(d)
    }

    /// Line `i` taken out.
    pub fn remove(&self, i: usize) -> Doc {
        let mut d = self.clone();
        if i < d.lines.len() {
            d.lines.remove(i);
        }
        d
    }

    /// A row added after the last line (before the file's final newline).
    pub fn push(&self, left: &str, right: &str) -> Result<Doc, String> {
        let raw = self.line_for(left, right)?;
        let mut d = self.clone();
        let at = if d.lines.last().is_some_and(|l| l.raw.is_empty()) { d.lines.len() - 1 } else { d.lines.len() };
        d.lines.insert(at, Line { raw, row: Some((left.to_string(), right.to_string())) });
        Ok(d)
    }

    /// The text to write, once it reads back line by line as exactly these rows.
    pub fn checked(&self) -> Result<String, String> {
        let text = self.text();
        let back = read(self.which, &text).map_err(|e| format!("the edited file does not read back: {e}"))?;
        let rows = |d: &Doc| d.lines.iter().map(|l| l.row.clone()).collect::<Vec<_>>();
        if rows(&back) != rows(self) {
            return Err("the edited file does not read back as the rows written; it is left as it was".into());
        }
        Ok(text)
    }

    /// For each row line whose key (by key hash) another row line also writes: the line whose text
    /// the build applies, or `None` when the build refuses the file for it. `qm build` reads the file
    /// into a map keyed by the key as spelled (`build::parse_string_edits`), so a later line with the
    /// same spelling replaces an earlier one, and applies the spellings in sorted order, so the last
    /// spelling in that order wins; for `add_stringdb_keys` a second spelling of an added key is a
    /// second add, which it refuses. Empty for a pairs file.
    pub fn repeats(&self) -> HashMap<usize, Option<usize>> {
        let mut out = HashMap::new();
        if self.which == Which::Replace {
            return out;
        }
        let mut by_key: HashMap<u32, Vec<(usize, &str)>> = HashMap::new();
        for (i, l) in self.lines.iter().enumerate() {
            if let Some((k, _)) = &l.row {
                by_key.entry(stringdb::key_hash(k)).or_default().push((i, k.as_str()));
            }
        }
        for lines in by_key.values().filter(|v| v.len() > 1) {
            let last_spelling = lines.iter().map(|(_, k)| *k).max().expect("a group has lines");
            let spellings = lines.iter().map(|(_, k)| *k).collect::<std::collections::BTreeSet<_>>().len();
            let applied = lines.iter().rev().find(|(_, k)| *k == last_spelling).map(|(i, _)| *i);
            let applied = if self.which == Which::Add && spellings > 1 { None } else { applied };
            for (i, _) in lines {
                out.insert(*i, applied);
            }
        }
        out
    }
}

/// A shipped string table, as the build reads it (`GameStack::container_for_asset`).
pub struct Table {
    /// Key → text.
    pub text: HashMap<u32, String>,
    /// Every key: named keys first (by name), then the rest by hash.
    pub order: Vec<u32>,
    /// Key names from the names table and the decompiled Lua's string literals.
    pub names: HashMap<u32, String>,
    /// Text → every key that carries it.
    pub by_text: HashMap<String, Vec<u32>>,
}

impl Table {
    pub fn label(&self, key: u32) -> String {
        self.names.get(&key).cloned().unwrap_or_else(|| format!("0x{key:08X}"))
    }

    /// How a strings file writes `key`: `[name]` when the name hashes back to it, else `0xHASH`.
    pub fn spelled(&self, key: u32) -> String {
        match self.names.get(&key).map(|n| format!("[{n}]")) {
            Some(b) if stringdb::key_hash(&b) == key => b,
            _ => format!("0x{key:08X}"),
        }
    }
}

/// Read `target` from the game stack `wads`, naming its keys from `names` and the `lua` sources.
pub fn read_table(wads: &[PathBuf], target: &str, names: &HashMap<u32, String>, lua: &[String]) -> Result<Table, String> {
    use mercs2_formats::types::{TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB};
    let hash = mercs2_quartermaster::manifest::asset_hash(target);
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let c = game
        .container_for_asset(hash, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
        .ok_or_else(|| format!("{target:?} (0x{hash:08X}) is not a string table in the game stack"))?;
    let db = parse_container(&c).map_err(|e| format!("{target}: {e}"))?;
    let text: HashMap<u32, String> = db.entries.into_iter().map(|e| (e.key_hash, e.text)).collect();
    let keys: std::collections::HashSet<u32> = text.keys().copied().collect();
    let mut key_names: HashMap<u32, String> =
        names.iter().filter(|(h, _)| keys.contains(h)).map(|(h, n)| (*h, n.clone())).collect();
    for (h, n) in crate::textpage::lua_key_names(lua, &keys) {
        key_names.entry(h).or_insert(n);
    }
    let mut order: Vec<u32> = keys.into_iter().collect();
    order.sort_by(|a, b| match (key_names.get(a), key_names.get(b)) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.cmp(b),
    });
    let mut by_text: HashMap<String, Vec<u32>> = HashMap::new();
    for &k in &order {
        by_text.entry(text[&k].clone()).or_default().push(k);
    }
    Ok(Table { text, order, names: key_names, by_text })
}

fn parse_container(c: &[u8]) -> Result<stringdb::StringDb, String> {
    let keys = mercs2_formats::ucfx::extract_chunk_body(c, b"KEYS").ok_or("no KEYS chunk")?;
    let strs = mercs2_formats::ucfx::extract_chunk_body(c, b"STRS").ok_or("no STRS chunk")?;
    stringdb::parse(&keys, &strs)
}

/// One key the build changes: its text before (`None`: the key is new) and after.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub key: u32,
    pub before: Option<String>,
    pub after: String,
}

/// One copy of a table as the build writes it.
pub struct After {
    pub table: u32,
    pub target: String,
    /// vz.wad's copy, or shell.wad's.
    pub copy: qb::TableCopy,
    pub keys: usize,
    /// The keys the build changes, in the written table's order.
    pub changes: Vec<Change>,
    /// Key → its index in [`Self::changes`].
    pub at: HashMap<u32, usize>,
}

/// The tables `contributions` (an item at `dir`) build: the key and text contributions merged by
/// qm into each copy of the table, each `add_language`'s table forked by qm. A shell.wad copy whose
/// changes are the vz.wad copy's is left out. `Err` is qm's refusal.
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution]) -> Result<Vec<After>, String> {
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let s = mercs2_quartermaster::discover::LoadedShipment {
        root: dir.to_path_buf(),
        manifest_path: dir.join(crate::depot::ITEM_FILE),
        format: mercs2_quartermaster::Format::Yaml,
        manifest: crate::depot::item_manifest(&item),
    };
    let mut writes = Vec::new();
    if contributions.iter().any(|c| matches!(of(c), Some((Which::Edit | Which::Add | Which::Replace, _, _)))) {
        writes.extend(qb::shipment_string_tables(&s, &mut game)?);
    }
    for c in contributions {
        if let Contribution::AddLanguage { strings, base, .. } = c {
            writes.push(qb::language_table(&mut game, dir, strings, base.as_deref())?.1);
        }
    }
    let all: Vec<After> = writes
        .into_iter()
        .map(|w| {
            let before: HashMap<u32, String> =
                parse_container(&w.base).map_err(|e| format!("{}: {e}", w.target))?.entries.into_iter().map(|e| (e.key_hash, e.text)).collect();
            let merged = parse_container(&w.merged).map_err(|e| format!("{} as built: {e}", w.target))?;
            let keys = merged.entries.len();
            let changes: Vec<Change> = merged
                .entries
                .into_iter()
                .filter(|e| before.get(&e.key_hash) != Some(&e.text))
                .map(|e| Change { key: e.key_hash, before: before.get(&e.key_hash).cloned(), after: e.text })
                .collect();
            let at = changes.iter().enumerate().map(|(i, c)| (c.key, i)).collect();
            Ok(After { table: w.table, target: w.target, copy: w.copy, keys, changes, at })
        })
        .collect::<Result<_, String>>()?;
    let same_as_vz = |a: &After| {
        a.copy == qb::TableCopy::Shell
            && all.iter().any(|v| v.copy == qb::TableCopy::Vz && v.table == a.table && v.keys == a.keys && v.changes == a.changes)
    };
    let keep: Vec<bool> = all.iter().map(|a| !same_as_vz(a)).collect();
    Ok(all.into_iter().zip(keep).filter_map(|(a, k)| k.then_some(a)).collect())
}

/// A read on a worker: its channel until it answers, then the answer.
pub(crate) enum Loading<T> {
    Reading(Receiver<Result<T, String>>),
    Ready(Result<Rc<T>, String>),
}

/// `f` started on a worker.
pub(crate) fn spawn<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Loading<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    Loading::Reading(rx)
}

/// The answer, once the worker has given it; `None` while it reads.
pub(crate) fn poll<T>(entry: &mut Loading<T>) -> Option<Result<Rc<T>, String>> {
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

/// What a text contribution's editor reads, with its table and file.
pub struct Subject<'a> {
    pub which: Which,
    pub target: &'a str,
    pub file: &'a Path,
}

/// What each item's build writes, by item folder: the stamp it was computed from and the computation.
type AfterCache = HashMap<PathBuf, (String, Loading<Vec<After>>)>;

/// Each text file read, by path: its modified time when read and its text and lines.
type DocCache = HashMap<PathBuf, (SystemTime, Rc<Result<(String, Doc), String>>)>;

/// The editors' reads.
#[derive(Default)]
pub struct Text {
    /// Shipped tables, by target and the name sources they were named from.
    tables: RefCell<HashMap<(String, usize, usize), Loading<Table>>>,
    /// What each item's build writes, by item folder, with what it was computed from.
    after: RefCell<AfterCache>,
    /// Files, by path, with the modification time they were read at.
    docs: RefCell<DocCache>,
}

impl Text {
    /// Forget what was read from the game (a different stack).
    pub fn forget_game(&self) {
        self.tables.borrow_mut().clear();
        self.after.borrow_mut().clear();
    }

    /// `target` as the game ships it: `None` while it is read.
    pub fn table(&self, game: &[PathBuf], target: &str, s: &Sources) -> Option<Result<Rc<Table>, String>> {
        let names_len = s.index.map_or(0, |i| i.names.len());
        let key = (target.to_string(), names_len, s.corpus.len());
        let mut map = self.tables.borrow_mut();
        let entry = map.entry(key).or_insert_with(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (wads, t) = (game.to_vec(), target.to_string());
            let names = s.index.map(|i| i.names.clone()).unwrap_or_default();
            let lua: Vec<String> = s.corpus.iter().map(|(_, src, _)| src.clone()).collect();
            std::thread::spawn(move || {
                let r = if wads.is_empty() { Err("no game archive is open".to_string()) } else { read_table(&wads, &t, &names, &lua) };
                let _ = tx.send(r);
            });
            Loading::Reading(rx)
        });
        poll(entry)
    }

    /// What `item`'s build writes: `None` while it is computed. Recomputed when the item's
    /// contributions or any of its text files change.
    pub fn after(&self, game: &[PathBuf], item: &Item) -> Option<Result<Rc<Vec<After>>, String>> {
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        let mut stamp = format!("{contributions:?}");
        for c in &contributions {
            if let Some((_, _, f)) = of(c) {
                let t = std::fs::metadata(item.dir.join(f)).and_then(|m| m.modified()).ok();
                stamp.push_str(&format!("|{t:?}"));
            }
        }
        let mut map = self.after.borrow_mut();
        let stale = map.get(&item.dir).is_none_or(|(s, _)| *s != stamp);
        if stale {
            let (tx, rx) = std::sync::mpsc::channel();
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            std::thread::spawn(move || {
                let r = if wads.is_empty() { Err("no game archive is open".to_string()) } else { after(&wads, &dir, &contributions) };
                let _ = tx.send(r);
            });
            map.insert(item.dir.clone(), (stamp, Loading::Reading(rx)));
        }
        let (_, entry) = map.get_mut(&item.dir)?;
        poll(entry)
    }

    /// The file at `abs`, read as `which`: its text and its lines. A file that is not there reads as
    /// empty.
    pub fn doc(&self, abs: &Path, which: Which) -> Rc<Result<(String, Doc), String>> {
        let Ok(mtime) = std::fs::metadata(abs).and_then(|m| m.modified()) else {
            return Rc::new(read(which, "").map(|d| (String::new(), d)));
        };
        if let Some((t, r)) = self.docs.borrow().get(abs) {
            if *t == mtime && r.as_ref().as_ref().is_ok_and(|(_, d)| d.which == which) {
                return r.clone();
            }
        }
        let r = Rc::new(
            std::fs::read_to_string(abs)
                .map_err(|e| format!("{}: {e}", abs.display()))
                .and_then(|text| read(which, &text).map(|d| (text, d)).map_err(|e| format!("{}: {e}", abs.display()))),
        );
        self.docs.borrow_mut().insert(abs.to_path_buf(), (mtime, r.clone()));
        r
    }
}

/// What the editor asks the page to do.
pub enum Ask {
    /// Write `text` over `path`, whose content was `prior` when the edit was made.
    Write { path: PathBuf, prior: String, text: String },
}

/// What a row's key (or old text) is in the shipped table, as a note.
fn resolve(which: Which, left: &str, table: Option<&Table>) -> (FieldState, String) {
    let Some(t) = table else { return (FieldState::Neutral, String::new()) };
    match which {
        Which::Replace => match t.by_text.get(left) {
            None => (FieldState::Bad, "matches no text in the shipped table".into()),
            Some(keys) => {
                let names: Vec<String> = keys.iter().take(3).map(|k| t.label(*k)).collect();
                let more = if keys.len() > 3 { format!(" and {} more", keys.len() - 3) } else { String::new() };
                (FieldState::Neutral, format!("{} key(s): {}{more}", keys.len(), names.join(", ")))
            }
        },
        _ => {
            let k = stringdb::key_hash(left);
            let has = t.text.contains_key(&k);
            match (which, has) {
                (Which::Add, true) => (FieldState::Bad, format!("0x{k:08X} \u{b7} the shipped table has this key; edit it with edit_stringdb")),
                (Which::Add, false) => (FieldState::Neutral, format!("0x{k:08X} \u{b7} not in the shipped table")),
                (_, true) => match t.names.get(&k) {
                    Some(n) => (FieldState::Neutral, format!("0x{k:08X} \u{b7} {n}")),
                    None => (FieldState::Neutral, format!("0x{k:08X} \u{b7} in the shipped table")),
                },
                (_, false) => (FieldState::Bad, format!("0x{k:08X} \u{b7} not a key of the shipped table")),
            }
        }
    }
}

/// The middle column of a row: the key's shipped text, or for a pair, how many keys the old text hits.
fn shipped_cell(which: Which, left: &str, table: Option<&Table>) -> String {
    let Some(t) = table else { return String::new() };
    match which {
        Which::Replace => t.by_text.get(left).map_or("0 keys".to_string(), |k| format!("{} key(s)", k.len())),
        _ => t.text.get(&stringdb::key_hash(left)).cloned().unwrap_or_else(|| "\u{2014}".to_string()),
    }
}

/// A key typed without brackets, bracketed; a bare `0xHASH` or a bracketed key as typed.
pub fn bracket(typed: &str) -> String {
    let t = typed.trim();
    let hash = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .is_some_and(|h| !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
    if t.is_empty() || hash || t.starts_with('[') {
        t.to_string()
    } else {
        format!("[{t}]")
    }
}

/// A left-aligned cell `w` wide.
fn cell(ui: &mut egui::Ui, w: f32, h: f32, text: egui::RichText, hover: Option<&str>) {
    ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(w);
        let r = ui.add(egui::Label::new(text).truncate());
        if let Some(h) = hover.filter(|h| !h.is_empty()) {
            r.on_hover_text(h);
        }
    });
}

fn button_w(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), egui::TextStyle::Button.resolve(ui.style()), theme::TX).size().x
        + 2.0 * ui.spacing().button_padding.x
        + ui.spacing().item_spacing.x
}

/// A cell's text field, holding what is typed until focus leaves it. Returns the text and whether
/// focus left it with a change.
fn held(ui: &mut egui::Ui, id: egui::Id, shown: &str, w: f32, bad: bool) -> (String, bool) {
    let mut text: String =
        ui.data(|d| d.get_temp::<(String, String)>(id)).filter(|(o, _)| o == shown).map(|(_, t)| t).unwrap_or_else(|| shown.to_string());
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .margin(theme::TEXT_EDIT_MARGIN)
            .desired_width(w - theme::TEXT_EDIT_MARGIN.sum().x)
            .text_color(if bad { theme::BAD } else { theme::TX }),
    );
    let commit = r.lost_focus() && text != shown;
    ui.data_mut(|d| d.insert_temp(id, (shown.to_string(), text.clone())));
    (text, commit)
}

/// The centre editor of one text contribution: the file's rows, the add row, and the shipped table
/// with the build's text beside it.
pub fn editor(ui: &mut egui::Ui, t: &Text, item: &Item, sub: Subject, game: &[PathBuf], s: &Sources) -> Vec<Ask> {
    let Subject { which, target, file } = sub;
    let mut asks = Vec::new();
    let abs = item.dir.join(file);
    let table = t.table(game, target, s);
    let table_ok = table.as_ref().and_then(|r| r.as_ref().ok()).map(|r| &**r);
    let after = t.after(game, item);
    let after_ok = after.as_ref().and_then(|r| r.as_ref().ok()).and_then(|v| {
        let h = mercs2_quartermaster::manifest::asset_hash(target);
        v.iter().find(|a| a.table == h)
    });
    let refused = matches!(after, Some(Err(_)));
    let doc = t.doc(&abs, which);
    let title = match which {
        Which::Replace => "Text replacements",
        Which::Language => "Translation",
        _ => "Strings",
    };
    let badge = format!("{} \u{b7} {}", target, file.display());
    theme::section(ui, title, Some(&badge), true, |ui| {
        let (prior, d) = match doc.as_ref() {
            Ok((p, d)) => (p, d),
            Err(e) => {
                ui.label(egui::RichText::new(e).color(theme::BAD));
                ui.label(egui::RichText::new("The file does not read, so it is not written.").small().color(theme::DIM));
                return;
            }
        };
        if let Err(e) = which.whole(prior, &file.display().to_string()) {
            theme::field_note(ui, FieldState::Bad, &format!("qm build: {e}"));
        }
        match &table {
            None => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(egui::RichText::new(format!("Reading {target}\u{2026}")).small().color(theme::DIM));
                });
            }
            Some(Err(e)) => theme::field_note(ui, FieldState::Bad, e),
            Some(Ok(_)) => {}
        }
        rows(ui, which, (&abs, prior), d, table_ok, &mut asks);
        ui.add_space(6.0);
        add_row(ui, which, (&abs, prior), d, table_ok, &mut asks);
    });
    if let Some(tb) = table_ok {
        browser(ui, which, (target, &abs), tb, (after_ok, refused), doc.as_ref().as_ref().ok().map(|(_, d)| d));
    }
    asks
}

/// The file's lines: each row's columns editable, with what it resolves to; comments as they are.
fn rows(ui: &mut egui::Ui, which: Which, (abs, prior): (&Path, &str), d: &Doc, table: Option<&Table>, asks: &mut Vec<Ask>) {
    let repeats = d.repeats();
    let n_rows = d.lines.iter().filter(|l| l.row.is_some()).count();
    let [c0, c1, c2] = which.columns();
    let num_w = 30.0;
    let gap = ui.spacing().item_spacing.x;
    let remove_w = button_w(ui, "Remove");
    let rest = (ui.available_width() - num_w - remove_w - 3.0 * gap).max(240.0);
    let (w0, w1) = (rest * 0.3, rest * 0.3);
    let w2 = rest - w0 - w1;
    ui.horizontal(|ui| {
        for (h, w) in [("LINE", num_w), (c0, w0), (c1, w1), (c2, w2)] {
            cell(ui, w, 16.0, theme::disp_text(h, theme::style_eyebrow(), theme::FAINT), None);
        }
    });
    if n_rows == 0 {
        ui.label(egui::RichText::new("No rows yet.").color(theme::DIM));
    }
    for (i, l) in d.lines.iter().enumerate() {
        let Some((left, right)) = &l.row else {
            if !l.raw.trim().is_empty() {
                ui.horizontal(|ui| {
                    cell(ui, num_w, 18.0, egui::RichText::new(format!("{}", i + 1)).monospace().color(theme::FAINT), None);
                    ui.label(egui::RichText::new(l.raw.trim_end_matches('\r')).monospace().italics().color(theme::FAINT));
                });
            }
            continue;
        };
        ui.push_id(("text-row", i), |ui| {
            let (state, note) = resolve(which, left, table);
            let mut edit = None;
            let mut remove = false;
            let mut refusal = None;
            ui.horizontal(|ui| {
                cell(ui, num_w, 18.0, egui::RichText::new(format!("{}", i + 1)).monospace().color(theme::FAINT), None);
                let id = ui.make_persistent_id("cells");
                let (l_now, l_commit) = held(ui, id.with(0), left, w0, state == FieldState::Bad);
                let new_left = if which == Which::Replace { l_now.clone() } else { bracket(&l_now) };
                let middle = shipped_cell(which, &new_left, table);
                cell(ui, w1, 18.0, egui::RichText::new(&middle).color(theme::DIM), Some(&middle));
                let (r_now, r_commit) = held(ui, id.with(2), right, w2, false);
                if (l_commit || r_commit) && (new_left != *left || r_now != *right) {
                    edit = Some((new_left.clone(), r_now.clone()));
                }
                if let Err(e) = which.text(&new_left, &r_now) {
                    refusal = Some(e);
                }
                if ui.small_button("Remove").clicked() {
                    remove = true;
                }
            });
            let indent = num_w + gap;
            let note_line = |ui: &mut egui::Ui, text: &str, colour| {
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.label(egui::RichText::new(text).small().color(colour));
                });
            };
            if let Some(e) = &refusal {
                note_line(ui, e, theme::BAD);
            } else if !note.is_empty() {
                note_line(ui, &note, if state == FieldState::Bad { theme::BAD } else { theme::DIM });
            }
            match repeats.get(&i) {
                Some(Some(w)) if *w == i => note_line(ui, "another line writes this key too; the build applies this line", theme::BRASS),
                Some(Some(w)) => note_line(ui, &format!("another line writes this key too; the build applies line {}", w + 1), theme::BRASS),
                Some(None) => note_line(ui, "another line adds this key under another spelling; the build refuses the second add", theme::BAD),
                None => {}
            }
            let next = if remove { Some(Ok(d.remove(i))) } else { edit.filter(|_| refusal.is_none()).map(|(a, b)| d.set(i, &a, &b)) };
            match next.map(|r| r.and_then(|nd| nd.checked())) {
                Some(Ok(text)) => asks.push(Ask::Write { path: abs.to_path_buf(), prior: prior.to_string(), text }),
                Some(Err(e)) => note_line(ui, &e, theme::BAD),
                None => {}
            }
        });
    }
}

/// The id of the add row's two fields.
fn add_id(abs: &Path) -> egui::Id {
    egui::Id::unique(("depot-text-add", abs.to_path_buf()))
}

/// The row added at the end of the file: its two fields and the add button.
fn add_row(ui: &mut egui::Ui, which: Which, (abs, prior): (&Path, &str), d: &Doc, table: Option<&Table>, asks: &mut Vec<Ask>) {
    let id = add_id(abs);
    let (mut a, mut b): (String, String) = ui.data(|m| m.get_temp(id)).unwrap_or_default();
    let [c0, _, c2] = which.columns();
    let hint0 = match which {
        Which::Replace => "the text as the game shows it",
        _ => "Menu.Play or 0x0000B29D",
    };
    theme::text_field(ui, &c0.to_lowercase(), &mut a, hint0, FieldState::Neutral);
    let left = if which == Which::Replace { a.clone() } else { bracket(&a) };
    if !a.is_empty() {
        let (state, note) = resolve(which, &left, table);
        if !note.is_empty() {
            theme::field_note(ui, state, &note);
        }
    }
    theme::text_field(ui, &c2.to_lowercase(), &mut b, "", FieldState::Neutral);
    let line = which.text(&left, &b);
    if let (Err(e), false) = (&line, a.is_empty()) {
        theme::field_note(ui, FieldState::Bad, e);
    }
    let lw = theme::field_label_w(ui.available_width());
    ui.horizontal(|ui| {
        ui.add_space(lw + 4.0);
        let label = match which {
            Which::Replace => "+ Add replacement",
            Which::Language => "+ Add translation",
            _ => "+ Add key",
        };
        if theme::primary_button(ui, label, line.is_ok()).clicked() {
            match d.push(&left, &b).and_then(|nd| nd.checked()) {
                Ok(text) => {
                    asks.push(Ask::Write { path: abs.to_path_buf(), prior: prior.to_string(), text });
                    a.clear();
                    b.clear();
                }
                Err(e) => theme::field_note(ui, FieldState::Bad, &e),
            }
        }
    });
    ui.data_mut(|m| m.insert_temp(id, (a, b)));
}

/// The shipped table, with the build's text beside each key it changes. A row's button fills the
/// add row with that key (or text).
fn browser(ui: &mut egui::Ui, which: Which, (target, abs): (&str, &Path), t: &Table, (after, refused): (Option<&After>, bool), d: Option<&Doc>) {
    let new_keys: Vec<u32> = after.map(|a| a.changes.iter().filter(|c| c.before.is_none()).map(|c| c.key).collect()).unwrap_or_default();
    let keys_n = t.text.len() + new_keys.len();
    let badge = match after {
        Some(a) => format!("{keys_n} keys \u{b7} {} changed by the build", a.changes.len()),
        None if refused => format!("{keys_n} keys \u{b7} the build refuses the file"),
        None => format!("{keys_n} keys"),
    };
    theme::section(ui, &format!("Table \u{b7} {target}"), Some(&badge), true, |ui| {
        let fid = egui::Id::unique(("depot-text-filter", abs.to_path_buf()));
        let (mut q, mut only): (String, bool) = ui.data(|m| m.get_temp(fid)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut q).hint_text("Filter keys and text").margin(theme::TEXT_EDIT_MARGIN).desired_width(260.0));
            if theme::pill(ui, "Changed by the build", only).clicked() {
                only = !only;
            }
        });
        // Keys the file already writes.
        let in_file: std::collections::HashSet<u32> = d
            .map(|d| d.lines.iter().filter_map(|l| l.row.as_ref()).map(|(k, _)| stringdb::key_hash(k)).collect())
            .unwrap_or_default();
        let ql = q.to_lowercase();
        let after_text = |k: u32| after.and_then(|a| a.at.get(&k).map(|&i| a.changes[i].after.as_str()));
        let keys: Vec<u32> = t
            .order
            .iter()
            .chain(new_keys.iter())
            .copied()
            .filter(|&k| !only || after_text(k).is_some())
            .filter(|&k| {
                ql.is_empty()
                    || t.label(k).to_lowercase().contains(&ql)
                    || t.text.get(&k).is_some_and(|s| s.to_lowercase().contains(&ql))
                    || after_text(k).is_some_and(|s| s.to_lowercase().contains(&ql))
            })
            .collect();
        ui.label(egui::RichText::new(format!("{} shown", keys.len())).small().color(theme::DIM));
        let gap = ui.spacing().item_spacing.x;
        let use_w = which.use_label().map_or(0.0, |l| button_w(ui, l));
        let rest = (ui.available_width() - use_w - 3.0 * gap).max(240.0);
        let (kw, sw) = (rest * 0.28, rest * 0.36);
        let aw = rest - kw - sw;
        ui.horizontal(|ui| {
            for (h, w) in [("KEY", kw), ("SHIPPED", sw), ("AFTER THE BUILD", aw)] {
                cell(ui, w, 16.0, theme::disp_text(h, theme::style_eyebrow(), theme::FAINT), None);
            }
        });
        let row_h = 20.0;
        let mut fill: Option<(String, String)> = None;
        egui::ScrollArea::vertical().id_salt(("depot-text-table", abs.to_path_buf())).max_height(360.0).auto_shrink([false, true]).show_rows(
            ui,
            row_h,
            keys.len(),
            |ui, range| {
                for &k in &keys[range] {
                    ui.horizontal(|ui| {
                        let named = t.names.contains_key(&k);
                        let label = t.label(k);
                        let key_text = egui::RichText::new(&label).monospace().color(if named { theme::TX } else { theme::DIM });
                        cell(ui, kw, row_h - 2.0, key_text, Some(&label));
                        let shipped = t.text.get(&k).map_or("\u{2014}", |s| s.as_str());
                        cell(ui, sw, row_h - 2.0, egui::RichText::new(shipped).color(theme::DIM), Some(shipped));
                        let a = after_text(k).unwrap_or("");
                        cell(ui, aw, row_h - 2.0, egui::RichText::new(a).color(theme::BRASS), Some(a));
                        if let Some(u) = which.use_label() {
                            let can = t.text.contains_key(&k) && (which == Which::Replace || !in_file.contains(&k));
                            if ui.add_enabled(can, egui::Button::new(u).small()).clicked() {
                                let shipped = t.text.get(&k).cloned().unwrap_or_default();
                                fill = Some(match which {
                                    Which::Replace => (shipped.clone(), shipped),
                                    _ => (t.spelled(k), shipped),
                                });
                            }
                        }
                    });
                }
            },
        );
        if let Some(f) = fill {
            ui.data_mut(|m| m.insert_temp(add_id(abs), f));
        }
        ui.data_mut(|m| m.insert_temp(fid, (q, only)));
    });
}

/// The inspector's card: the tables as the build writes them.
pub fn after_card(ui: &mut egui::Ui, t: &Text, game: &[PathBuf], item: &Item) {
    theme::card(ui, "After qm build", None, |ui| match t.after(game, item) {
        None => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(egui::RichText::new("Applying the rows\u{2026}").small().color(theme::DIM));
            });
        }
        Some(Err(e)) => {
            ui.label(egui::RichText::new(e).small().color(theme::BAD));
        }
        Some(Ok(tables)) => {
            for a in tables.iter() {
                let added = a.changes.iter().filter(|c| c.before.is_none()).count();
                theme::kv(ui, "table", egui::RichText::new(format!("{} \u{b7} 0x{:08X}", a.target, a.table)).monospace());
                if a.copy == qb::TableCopy::Shell {
                    theme::kv(ui, "copy", egui::RichText::new("shell.wad"));
                }
                theme::kv(ui, "keys", egui::RichText::new(a.keys.to_string()));
                theme::kv(ui, "changed", egui::RichText::new((a.changes.len() - added).to_string()));
                theme::kv(ui, "added", egui::RichText::new(added.to_string()));
                const SHOWN: usize = 12;
                for c in a.changes.iter().take(SHOWN) {
                    let before = c.before.as_deref().map_or("new key".to_string(), |b| format!("\u{201c}{b}\u{201d}"));
                    ui.label(egui::RichText::new(format!("0x{:08X}  {before} \u{2192} \u{201c}{}\u{201d}", c.key, c.after)).small().color(theme::TX));
                }
                if a.changes.len() > SHOWN {
                    ui.label(egui::RichText::new(format!("{} more in the table", a.changes.len() - SHOWN)).small().color(theme::DIM));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unedited_file_writes_back_as_it_was_and_an_edit_changes_only_its_line() {
        for (which, text) in [
            (Which::Edit, "# fixes\r\n[Menu.Play] = Play\r\n\r\n0x0000B29D: Quit\r\n"),
            (Which::Replace, "# from the forum\nPress Start\tPress Enter\n\nOld  spaced \t new\n"),
            (Which::Language, "[A] = a"),
        ] {
            let d = read(which, text).unwrap();
            assert_eq!(d.checked().unwrap(), text);
            let i = d.lines.iter().position(|l| l.row.is_some()).unwrap();
            let (left, _) = d.lines[i].row.clone().unwrap();
            let edited = d.set(i, &left, "Changed").unwrap().checked().unwrap();
            let (a, b): (Vec<&str>, Vec<&str>) = (text.split('\n').collect(), edited.split('\n').collect());
            assert_eq!(a.len(), b.len());
            let differing: Vec<usize> = (0..a.len()).filter(|&n| a[n] != b[n]).collect();
            assert_eq!(differing, vec![i], "{which:?}: {edited:?}");
        }
    }

    #[test]
    fn rows_are_added_before_the_final_newline_and_removed_by_line() {
        let d = read(Which::Edit, "").unwrap();
        assert_eq!(d.push("[A]", "a").unwrap().checked().unwrap(), "[A] = a\n");
        let d = read(Which::Edit, "# c\n[A] = a\n").unwrap();
        let two = d.push("[B]", "b").unwrap().checked().unwrap();
        assert_eq!(two, "# c\n[A] = a\n[B] = b\n");
        assert_eq!(read(Which::Edit, &two).unwrap().remove(1).checked().unwrap(), "# c\n[B] = b\n");
        let d = read(Which::Replace, "x\ty").unwrap();
        assert_eq!(d.push("p", "q").unwrap().checked().unwrap(), "x\ty\np\tq");
    }

    #[test]
    fn what_a_line_cannot_hold_is_refused_and_the_file_is_not_written() {
        let d = read(Which::Edit, "[A] = a\n").unwrap();
        assert!(d.set(0, "[A]", "two\nlines").unwrap_err().contains("line break"));
        assert!(d.push("[B]", " padded").unwrap_err().contains("whitespace"));
        let p = read(Which::Replace, "a\tb\n").unwrap();
        assert!(p.set(0, "a", "with\ttab").unwrap_err().contains("tab"));
    }

    /// Lines writing one key (by its hash) are flagged with the line qm applies: the last line of the
    /// spelling that sorts last. The map qm reads the file into says the same.
    #[test]
    fn a_key_written_twice_is_flagged_with_the_line_the_build_applies() {
        let h = stringdb::key_hash("[Menu.Play]");
        let text = format!("[Menu.Play] = a\n# c\n[menu.play] = b\n0x{h:08X} = c\n[Other] = d\n[menu.play] = e\n");
        let d = read(Which::Edit, &text).unwrap();
        let r = d.repeats();
        assert_eq!(r.len(), 4);
        for i in [0, 2, 3, 5] {
            assert_eq!(r.get(&i), Some(&Some(5)), "line {i}");
        }
        // The applied line's text is the one qm's map applies last for that key.
        let map = qb::parse_string_edits(&text).unwrap();
        let last = map.iter().rfind(|(k, _)| stringdb::key_hash(k) == h).unwrap();
        assert_eq!(last.1, "e");
        // Two spellings of one added key: the build refuses the second add.
        let d = read(Which::Add, &format!("[New.Key] = a\n0x{:08X} = b\n", stringdb::key_hash("[New.Key]"))).unwrap();
        assert_eq!(d.repeats().get(&0), Some(&None));
    }

    #[test]
    fn a_bare_name_is_bracketed_and_a_hash_or_bracket_kept() {
        assert_eq!(bracket("Menu.Play"), "[Menu.Play]");
        assert_eq!(bracket(" [Menu.Play] "), "[Menu.Play]");
        assert_eq!(bracket("0x0000B29D"), "0x0000B29D");
        assert_eq!(bracket(""), "");
    }

    /// Retail english, every key whose text a strings line can hold, written as a strings file:
    /// it reads and writes back byte-identical, and the build (qm's merge) of a one-row file changes
    /// exactly that key.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_strings_file_round_trips_and_the_build_changes_only_the_edited_key() {
        let wad = PathBuf::from(crate::app::retail_vz_wad());
        let t = read_table(std::slice::from_ref(&wad), "english", &HashMap::new(), &[]).expect("english reads");
        assert!(t.text.len() > 10_000, "english has {} keys", t.text.len());
        let mut text = String::from("# every english key\n");
        let mut held = 0;
        for k in &t.order {
            if let Ok(line) = qb::string_edit_text(&t.spelled(*k), &t.text[k]) {
                text.push_str(&line);
                text.push('\n');
                held += 1;
            }
        }
        println!("english: {} keys, {held} written, {} refused by the line format", t.text.len(), t.text.len() - held);
        let d = read(Which::Edit, &text).unwrap();
        assert_eq!(d.checked().unwrap(), text);
        let i = d.lines.iter().position(|l| l.row.is_some()).unwrap();
        let (key, _) = d.lines[i].row.clone().unwrap();
        let one = read(Which::Edit, "").unwrap().push(&key, "QM DEPOT EDIT").unwrap().checked().unwrap();

        let dir = std::env::temp_dir().join(format!("depottext-retail-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/strings.txt"), &one).unwrap();
        let c = Contribution::EditStringDb { target: "english".into(), strings: PathBuf::from("src/strings.txt") };
        let a = after(&[wad], &dir, &[c]);
        std::fs::remove_dir_all(&dir).unwrap();
        let a = a.expect("the build applies the edit");
        assert_eq!(a.len(), 1, "shell.wad's english changes as vz.wad's does, so it is one row");
        assert_eq!(a[0].copy, qb::TableCopy::Vz);
        let k = stringdb::key_hash(&key);
        assert_eq!(a[0].changes, vec![Change { key: k, before: Some(t.text[&k].clone()), after: "QM DEPOT EDIT".into() }]);
    }

    /// Retail english's texts as replacement pairs that change nothing round-trip; one replacement
    /// changes every key carrying its text, as the build applies it.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_pairs_file_round_trips_and_a_replacement_hits_every_key_with_the_text() {
        let wad = PathBuf::from(crate::app::retail_vz_wad());
        let t = read_table(std::slice::from_ref(&wad), "english", &HashMap::new(), &[]).expect("english reads");
        let mut texts: Vec<&String> = t.by_text.keys().collect();
        texts.sort();
        let mut text = String::new();
        for s in &texts {
            if let Ok(line) = qb::text_pair_text(s, s) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        let d = read(Which::Replace, &text).unwrap();
        assert_eq!(d.checked().unwrap(), text);

        let (old, keys) = t
            .by_text
            .iter()
            .filter(|(s, k)| k.len() > 1 && qb::text_pair_text(s, "X").is_ok())
            .max_by_key(|(s, k)| (k.len(), (*s).clone()))
            .unwrap();
        let one = read(Which::Replace, "").unwrap().push(old, "QM DEPOT REPLACED").unwrap().checked().unwrap();
        let dir = std::env::temp_dir().join(format!("depottext-pairs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/pairs.txt"), &one).unwrap();
        let c = Contribution::ReplaceStringDbText { target: "english".into(), pairs: PathBuf::from("src/pairs.txt") };
        let a = after(&[wad], &dir, &[c]);
        std::fs::remove_dir_all(&dir).unwrap();
        let a = a.expect("the build applies the replacement");
        let mut hit: Vec<u32> = a[0].changes.iter().map(|c| c.key).collect();
        let mut want = keys.clone();
        hit.sort();
        want.sort();
        assert_eq!(hit, want, "{old:?}");
    }
}
