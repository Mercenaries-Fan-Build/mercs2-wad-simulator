//! Inspect's TEXT tab for string tables (`docs/modernization/workshop_pages/inspect_tabs.md`, Text):
//! every string table of every WAD the archive opened, side by side by key.
//!
//! The tables are read on a worker the first time one is opened ([`Panel::ensure`]): each copy of
//! each table — a table several WADs ship (`french` in `shell.wad` and `French.wad`) is read from
//! each — parsed with `mercs2_formats::stringdb`. A key is named when `names.bin` holds its hash or
//! a string literal of the decompiled Lua hashes to it (`stringdb::key_hash`); otherwise it shows
//! its hash.

use crate::gui::theme;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};

/// One copy of one string table: the asset, and the WAD it is read from.
#[derive(Clone, Debug)]
pub struct Source {
    pub hash: u32,
    pub label: String,
    pub path: String,
}

/// One copy of one table, read.
pub struct Copy {
    pub hash: u32,
    pub label: String,
    /// The WAD's file name.
    pub wad: String,
    pub text: HashMap<u32, String>,
}

/// Every copy of every table, and the keys across them.
pub struct Tables {
    pub copies: Vec<Copy>,
    /// Every key any table carries: named keys first (by name), then the rest by hash.
    pub keys: Vec<u32>,
    pub key_names: HashMap<u32, String>,
}

impl Tables {
    /// The tables in column order: each table once, in the order its first copy was given.
    pub fn tables(&self) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for c in &self.copies {
            if !out.contains(&c.hash) {
                out.push(c.hash);
            }
        }
        out
    }

    /// Indices into [`Self::copies`] of `table`'s copies, first given first.
    pub fn copies_of(&self, table: u32) -> Vec<usize> {
        (0..self.copies.len()).filter(|&i| self.copies[i].hash == table).collect()
    }

    /// The label of `key`: its name, else its hash.
    pub fn key_label(&self, key: u32) -> String {
        self.key_names.get(&key).cloned().unwrap_or_else(|| format!("0x{key:08X}"))
    }
}

/// An open font: its glyphs and its pages, decoded.
pub struct FontView {
    pub hash: u32,
    pub label: String,
    /// The WAD the font is read from (file name).
    pub wad: String,
    pub font: mercs2_formats::font::Font,
    /// Per page: its texture's label, and the decoded RGBA at the size it decoded to — or why the
    /// page did not read.
    pub pages: Vec<(String, Result<(u32, u32, Vec<u8>), String>)>,
    /// Uploaded page textures, by page (made the first time the page is drawn).
    textures: HashMap<usize, egui::TextureHandle>,
    pub page: usize,
    /// The glyph the inspector shows (an index into the font's glyphs).
    pub glyph: Option<usize>,
}

impl FontView {
    pub fn new(hash: u32, label: String, wad: String, font: mercs2_formats::font::Font, pages: Vec<(String, Result<(u32, u32, Vec<u8>), String>)>) -> FontView {
        FontView { hash, label, wad, font, pages, textures: HashMap::new(), page: 0, glyph: None }
    }
}

enum Load {
    NotStarted,
    Loading(Receiver<Result<Tables, String>>),
    Ready(Box<Tables>),
    Failed(String),
}

/// The page's state.
pub struct Panel {
    load: Load,
    /// The table the row opened: its column comes first.
    pub table: Option<u32>,
    pub filter: String,
    /// The key the inspector shows.
    pub key: Option<u32>,
    /// Per table, the copy its column shows (an index into [`Tables::copies`]); a table not here
    /// shows its first copy.
    pub chosen: HashMap<u32, usize>,
    /// The keys the filter keeps, in [`Tables::keys`] order (recomputed on a filter change).
    rows: Vec<u32>,
    /// The open font, when a font row was opened (it replaces an open table, and the reverse).
    pub font: Option<FontView>,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel { load: Load::NotStarted, table: None, filter: String::new(), key: None, chosen: HashMap::new(), rows: Vec::new(), font: None }
    }
}

/// What the page's widgets ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Filter(String),
    Key(u32),
    /// Show this copy (an index into [`Tables::copies`]) in its table's column.
    Copy(usize),
    /// Add an `edit_stringdb` of this table (hash, label) to the Shipment — handled by the app.
    Ship(u32, String),
    /// Show this page of the open font.
    Page(usize),
    /// Show this glyph of the open font in the inspector.
    Glyph(usize),
}

impl Panel {
    /// Start reading `sources` (once), naming keys from `names` and the `lua` scripts' literals.
    pub fn ensure(&mut self, sources: Vec<Source>, names: HashMap<u32, String>, lua: Vec<String>) {
        if !matches!(self.load, Load::NotStarted) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(load(&sources, &names, &lua));
        });
        self.load = Load::Loading(rx);
    }

    /// Take the worker's result once it is ready.
    pub fn poll(&mut self) {
        if let Load::Loading(rx) = &self.load {
            match rx.try_recv() {
                Ok(Ok(t)) => {
                    self.load = Load::Ready(Box::new(t));
                    self.refilter();
                }
                Ok(Err(e)) => self.load = Load::Failed(e),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.load = Load::Failed("the string-table reader stopped".into()),
            }
        }
    }

    pub fn tables(&self) -> Option<&Tables> {
        match &self.load {
            Load::Ready(t) => Some(t),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.load {
            Load::Failed(e) => Some(e),
            _ => None,
        }
    }

    pub fn loading(&self) -> bool {
        matches!(self.load, Load::Loading(_))
    }

    /// The keys the filter keeps.
    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    /// The copy `table`'s column shows.
    pub fn shown_copy(&self, table: u32) -> Option<usize> {
        let t = self.tables()?;
        self.chosen.get(&table).copied().or_else(|| t.copies_of(table).first().copied())
    }

    /// The columns: the opened table first, then the others in their order.
    pub fn columns(&self) -> Vec<u32> {
        let Some(t) = self.tables() else { return Vec::new() };
        let mut cols = t.tables();
        if let Some(open) = self.table {
            if let Some(i) = cols.iter().position(|&c| c == open) {
                let c = cols.remove(i);
                cols.insert(0, c);
            }
        }
        cols
    }

    fn refilter(&mut self) {
        let Some(t) = self.tables() else { return };
        let needle = self.filter.trim().to_lowercase();
        let shown: Vec<usize> = t.tables().iter().filter_map(|&h| self.shown_copy(h)).collect();
        let rows = t
            .keys
            .iter()
            .copied()
            .filter(|k| {
                needle.is_empty()
                    || t.key_label(*k).to_lowercase().contains(&needle)
                    || shown.iter().any(|&c| t.copies[c].text.get(k).is_some_and(|s| s.to_lowercase().contains(&needle)))
            })
            .collect();
        self.rows = rows;
    }
}

/// Apply one of the page's acts.
pub fn apply(act: Act, p: &mut Panel) {
    match act {
        Act::Filter(f) => {
            p.filter = f;
            p.refilter();
        }
        Act::Key(k) => p.key = Some(k),
        Act::Copy(c) => {
            if let Some(h) = p.tables().and_then(|t| t.copies.get(c)).map(|c| c.hash) {
                p.chosen.insert(h, c);
                p.refilter();
            }
        }
        // The app routes it into the Shipment; the page has nothing to change.
        Act::Ship(..) => {}
        Act::Page(i) => {
            if let Some(f) = &mut p.font {
                if i < f.pages.len() {
                    f.page = i;
                }
            }
        }
        Act::Glyph(i) => {
            if let Some(f) = &mut p.font {
                if i < f.font.glyphs.len() {
                    f.glyph = Some(i);
                    f.page = f.font.glyphs[i].page as usize;
                }
            }
        }
    }
}

/// The printable form of a glyph's code point.
pub fn glyph_char(code: u16) -> String {
    match char::from_u32(code as u32) {
        Some(c) if !c.is_control() => c.to_string(),
        _ => format!("U+{code:04X}"),
    }
}

/// The centre for an open font: one page at a time, scaled to fit with whole-pixel zoom, every
/// glyph's box drawn on it; a click on a box chooses that glyph.
pub fn font_center(ctx: &egui::Context, p: &mut Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(f) = &mut p.font else { return acts };
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::G0).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::disp_text(format!("FONT \u{b7} {}", f.label).to_uppercase(), theme::style_title(), theme::TX));
                ui.label(
                    egui::RichText::new(format!("{} glyphs \u{b7} {} pages \u{b7} {}", f.font.glyphs.len(), f.pages.len(), f.wad))
                        .color(theme::DIM),
                );
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (i, (name, page)) in f.pages.iter().enumerate() {
                    let n = f.font.glyphs.iter().filter(|g| g.page as usize == i).count();
                    let size = page.as_ref().map_or("unreadable".to_string(), |(w, h, _)| format!("{w}\u{d7}{h}"));
                    if ui.selectable_label(f.page == i, format!("{i} \u{b7} {name} \u{b7} {size} \u{b7} {n} glyphs")).clicked() {
                        acts.push(Act::Page(i));
                    }
                }
            });
            ui.add_space(8.0);
            let Some((_, page)) = f.pages.get(f.page) else { return };
            let (w, h, rgba) = match page {
                Ok(p) => p,
                Err(e) => {
                    ui.label(egui::RichText::new(format!("page {}: {e}", f.page)).color(theme::BAD));
                    return;
                }
            };
            let tex = f.textures.entry(f.page).or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], rgba);
                ctx.load_texture(format!("font_{:08X}_{}", f.hash, f.page), img, egui::TextureOptions::NEAREST)
            });
            // Whole-pixel zoom, as large as the width allows; a tall page scrolls.
            let zoom = (ui.available_width() / *w as f32).floor().clamp(1.0, 8.0);
            let size = egui::vec2(*w as f32 * zoom, *h as f32 * zoom);
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, theme::G2);
                painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                for (gi, g) in f.font.glyphs.iter().enumerate() {
                    if g.page as usize != f.page {
                        continue;
                    }
                    let (x0, y0, x1, y1) = g.box_px(*w, *h);
                    let r = egui::Rect::from_min_max(
                        rect.min + egui::vec2(x0 * zoom, y0.min(y1) * zoom),
                        rect.min + egui::vec2(x1 * zoom, y0.max(y1) * zoom),
                    );
                    let chosen = f.glyph == Some(gi);
                    let col = if chosen { theme::BRASS } else { theme::BRASS.gamma_multiply(0.45) };
                    painter.rect_stroke(r, 0.0, egui::Stroke::new(if chosen { 2.0 } else { 1.0 }, col));
                    let resp = ui.interact(r, egui::Id::new(("font_glyph", f.hash, gi)), egui::Sense::click());
                    let resp = resp.on_hover_text(format!("{} (U+{:04X}) {}\u{d7}{}", glyph_char(g.code), g.code, g.width, g.height));
                    if resp.clicked() {
                        acts.push(Act::Glyph(gi));
                    }
                }
            });
        });
    acts
}

/// The right column for an open font: its INFO, pages, the chosen glyph, and every glyph.
pub fn font_inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(f) = &p.font else { return acts };
    theme::card(ui, "Font", Some(&f.label), |ui| {
        theme::kv(ui, "glyphs", egui::RichText::new(f.font.glyphs.len().to_string()));
        theme::kv(ui, "pages", egui::RichText::new(f.pages.len().to_string()));
        theme::kv(ui, "widest / tallest", egui::RichText::new(format!("{} / {} px", f.font.max_width, f.font.max_height)));
        theme::kv(ui, "INFO u32 @4", egui::RichText::new(format!("{} \u{b7} meaning not established", f.font.info_u32_4)).color(theme::DIM));
        theme::kv(ui, "hash", egui::RichText::new(format!("0x{:08X}", f.hash)).monospace());
        theme::kv(ui, "read from", egui::RichText::new(&f.wad));
    });
    if let Some(g) = f.glyph.and_then(|i| f.font.glyphs.get(i)) {
        theme::card(ui, "Glyph", Some(&format!("U+{:04X}", g.code)), |ui| {
            ui.label(egui::RichText::new(glyph_char(g.code)).text_style(egui::TextStyle::Heading).color(theme::TX));
            theme::kv(ui, "page", egui::RichText::new(g.page.to_string()));
            theme::kv(ui, "box", egui::RichText::new(format!("{} \u{d7} {} px", g.width, g.height)));
            theme::kv(ui, "edges u / v", egui::RichText::new(format!("{}\u{2013}{} / {}\u{2013}{}", g.u0, g.u1, g.v0, g.v1)).monospace());
            theme::kv(ui, "byte 10", egui::RichText::new(format!("{} \u{b7} meaning not established", g.byte10)).color(theme::DIM));
        });
    }
    theme::section(ui, "Glyphs", Some(&f.font.glyphs.len().to_string()), true, |ui| {
        let row_h = 20.0;
        egui::ScrollArea::vertical().max_height(320.0).show_rows(ui, row_h, f.font.glyphs.len(), |ui, range| {
            for gi in range {
                let g = &f.font.glyphs[gi];
                // A code point with no printable form shows only its number.
                let shown = char::from_u32(g.code as u32).filter(|c| !c.is_control()).map_or("\u{b7}".to_string(), |c| c.to_string());
                let text = format!("{shown}  U+{:04X}  p{}  {}\u{d7}{}", g.code, g.page, g.width, g.height);
                if ui.selectable_label(f.glyph == Some(gi), egui::RichText::new(text).monospace()).clicked() {
                    acts.push(Act::Glyph(gi));
                }
            }
        });
    });
    acts
}

/// The verb bar over an open table: start editing its text in a Shipment, copy the chosen key.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(t) = p.tables() else { return acts };
    let Some(open) = p.table else { return acts };
    let label = t.copies_of(open).first().map_or_else(|| format!("0x{open:08X}"), |&c| t.copies[c].label.clone());
    if ui.button("Edit this text").on_hover_text("Adds an edit_stringdb of this table to the Shipment").clicked() {
        acts.push(Act::Ship(open, label));
    }
    let key = p.key.map(|k| t.key_label(k));
    if ui.add_enabled(key.is_some(), egui::Button::new("Copy key")).clicked() {
        if let Some(k) = key {
            ui.ctx().copy_text(k);
        }
    }
    acts
}

/// Read every source, then name the keys. A source that does not read is an error naming it.
pub fn load(sources: &[Source], names: &HashMap<u32, String>, lua: &[String]) -> Result<Tables, String> {
    use mercs2_formats::types::TYPE_HASH_STRINGDB;
    let mut wads: HashMap<String, mercs2_engine::wad::Wad> = HashMap::new();
    let mut copies = Vec::new();
    for s in sources {
        if !wads.contains_key(&s.path) {
            let w = mercs2_engine::wad::open(&s.path).map_err(|e| format!("{}: {e}", s.path))?;
            wads.insert(s.path.clone(), w);
        }
        let w = wads.get_mut(&s.path).ok_or("unreachable: the WAD was just opened")?;
        let what = format!("string table {} in {}", s.label, s.path);
        let c = mercs2_engine::wad::extract_container_typed(w, s.hash, TYPE_HASH_STRINGDB).map_err(|e| format!("{what}: {e}"))?;
        let keys = mercs2_formats::ucfx::extract_chunk_body(&c, b"KEYS").ok_or_else(|| format!("{what}: no KEYS chunk"))?;
        let strs = mercs2_formats::ucfx::extract_chunk_body(&c, b"STRS").ok_or_else(|| format!("{what}: no STRS chunk"))?;
        let db = mercs2_formats::stringdb::parse(&keys, &strs).map_err(|e| format!("{what}: {e}"))?;
        let wad = std::path::Path::new(&s.path).file_name().map_or(s.path.clone(), |f| f.to_string_lossy().into_owned());
        copies.push(Copy { hash: s.hash, label: s.label.clone(), wad, text: db.entries.into_iter().map(|e| (e.key_hash, e.text)).collect() });
    }
    let mut all: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for c in &copies {
        all.extend(c.text.keys().copied());
    }
    let mut key_names: HashMap<u32, String> = HashMap::new();
    for (&h, n) in names {
        if all.contains(&h) {
            key_names.insert(h, n.clone());
        }
    }
    for (h, n) in lua_key_names(lua, &all) {
        key_names.entry(h).or_insert(n);
    }
    let mut keys: Vec<u32> = all.into_iter().collect();
    keys.sort_by(|a, b| match (key_names.get(a), key_names.get(b)) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.cmp(b),
    });
    Ok(Tables { copies, keys, key_names })
}

/// The names the `lua` scripts give keys in `keys`: every string literal whose key hash
/// (`stringdb::key_hash`, brackets stripped) is a key, the first literal naming each. A literal that
/// writes a key as its hash (`"[0x5cb97d23]"`) names nothing.
pub fn lua_key_names(lua: &[String], keys: &std::collections::HashSet<u32>) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for script in lua {
        for lit in string_literals(script) {
            let name = mercs2_formats::stringdb::key_name(lit);
            if name.is_empty() || name.starts_with("0x") || name.starts_with("0X") {
                continue;
            }
            let h = mercs2_formats::stringdb::key_hash(lit);
            if keys.contains(&h) {
                out.entry(h).or_insert_with(|| name.to_string());
            }
        }
    }
    out
}

/// The double- and single-quoted string literals of a Lua source, without their quotes (escapes
/// are left as written: a key never holds one).
pub fn string_literals(src: &str) -> impl Iterator<Item = &str> {
    let b = src.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            let q = b[i];
            if q == b'"' || q == b'\'' {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j] != q && b[j] != b'\n' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                i = j + 1;
                if j <= b.len() && start <= j.min(b.len()) {
                    return src.get(start..j.min(b.len()));
                }
            } else {
                i += 1;
            }
        }
        None
    })
}

/// The centre: the table of keys and every table's text.
pub fn center(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::G0).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
        .show(ctx, |ui| {
            if let Some(e) = p.error() {
                ui.label(egui::RichText::new(e).color(theme::BAD));
                return;
            }
            let Some(t) = p.tables() else {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(egui::RichText::new("Reading every string table\u{2026}").color(theme::FAINT));
                });
                return;
            };
            let cols = p.columns();
            let open = p.table.and_then(|h| p.shown_copy(h)).map(|c| &t.copies[c]);
            ui.horizontal(|ui| {
                let title = open.map_or("STRING TABLES".to_string(), |c| format!("STRING TABLE \u{b7} {}", c.label));
                ui.label(theme::disp_text(title.to_uppercase(), theme::style_title(), theme::TX));
                ui.label(
                    egui::RichText::new(format!("{} keys \u{b7} {} named \u{b7} {} shown", t.keys.len(), t.key_names.len(), p.rows().len()))
                        .color(theme::DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut f = p.filter.clone();
                    if ui.add(egui::TextEdit::singleline(&mut f).hint_text("Filter keys and text").desired_width(220.0)).changed() {
                        acts.push(Act::Filter(f));
                    }
                });
            });
            ui.add_space(6.0);
            // Every table gets a column at least `MIN_COL` wide; when they do not fit the centre, the
            // table scrolls sideways rather than dropping the columns past the edge.
            const MIN_COL: f32 = 110.0;
            let key_w = 220.0;
            let gap = ui.spacing().item_spacing.x;
            let n = cols.len().max(1) as f32;
            let col_w = ((ui.available_width() - key_w) / n - gap).max(MIN_COL);
            let total = key_w + n * (col_w + gap);
            fn cell(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui)) {
                ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_width(w);
                    add(ui);
                });
            }
            egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(total);
            ui.horizontal(|ui| {
                cell(ui, key_w, 16.0, |ui| {
                    ui.label(theme::disp_text("KEY", theme::style_eyebrow(), theme::FAINT));
                });
                for (i, &h) in cols.iter().enumerate() {
                    let c = p.shown_copy(h).map(|c| &t.copies[c]);
                    let name = c.map_or(format!("0x{h:08X}"), |c| format!("{} \u{b7} {}", c.label, c.wad));
                    let col = if i == 0 && p.table.is_some() { theme::BRASS } else { theme::FAINT };
                    cell(ui, col_w, 16.0, |ui| {
                        ui.add(egui::Label::new(theme::disp_text(name.to_uppercase(), theme::style_eyebrow(), col)).truncate());
                    });
                }
            });
            let row_h = 22.0;
            let rows = p.rows();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
                for &k in &rows[range] {
                    let (rect, row) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h - 2.0), egui::Sense::click());
                    let fill = if p.key == Some(k) { theme::G2 } else { egui::Color32::TRANSPARENT };
                    ui.painter().rect_filled(rect, egui::Rounding::same(3.0), fill);
                    let mut content = ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center), None);
                    let ui = &mut content;
                    let named = t.key_names.contains_key(&k);
                    cell(ui, key_w, row_h - 4.0, |ui| {
                        let txt = egui::RichText::new(t.key_label(k)).monospace().color(if named { theme::TX } else { theme::DIM });
                        ui.add(egui::Label::new(txt).truncate());
                    });
                    for &h in &cols {
                        let s = p.shown_copy(h).and_then(|c| t.copies[c].text.get(&k));
                        // A key the table carries with no text reads "empty"; one it lacks, "—".
                        cell(ui, col_w, row_h - 4.0, |ui| match s {
                            Some(s) if s.is_empty() => {
                                ui.label(egui::RichText::new("empty").italics().color(theme::FAINT));
                            }
                            Some(s) => {
                                ui.add(egui::Label::new(egui::RichText::new(s.as_str()).color(theme::TX)).truncate()).on_hover_text(s.as_str());
                            }
                            None => {
                                ui.label(egui::RichText::new("\u{2014}").color(theme::FAINT));
                            }
                        });
                    }
                    if row.clicked() {
                        acts.push(Act::Key(k));
                    }
                }
            });
            });
        });
    acts
}

/// The right column: the open table, its copies, and the chosen key in every table.
pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(t) = p.tables() else {
        if p.loading() {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.weak("Reading every string table\u{2026}");
            });
        }
        return acts;
    };
    if let Some(open) = p.table {
        let copies = t.copies_of(open);
        let first = copies.first().map(|&c| &t.copies[c]);
        theme::card(ui, "String table", first.map(|c| c.label.as_str()), |ui| {
            if let Some(c) = first {
                theme::kv(ui, "keys", egui::RichText::new(c.text.len().to_string()));
            }
            theme::kv(ui, "hash", egui::RichText::new(format!("0x{open:08X}")).monospace());
            // Every WAD that ships the table; a copy whose text differs from the first's says so.
            for &ci in &copies {
                let c = &t.copies[ci];
                let differs = first.is_some_and(|f| f.text != c.text);
                let text = if differs { format!("{} \u{b7} differs", c.wad) } else { c.wad.clone() };
                if ui.selectable_label(p.shown_copy(open) == Some(ci), text).clicked() {
                    acts.push(Act::Copy(ci));
                }
            }
        });
    }
    match p.key {
        None => {
            ui.weak("No key chosen \u{2014} pick a row in the table.");
        }
        Some(k) => {
            theme::card(ui, "Key", Some(&format!("0x{k:08X}")), |ui| {
                ui.label(egui::RichText::new(t.key_label(k)).monospace().color(theme::TX));
                for h in p.columns() {
                    let Some(c) = p.shown_copy(h).map(|c| &t.copies[c]) else { continue };
                    ui.add_space(4.0);
                    ui.label(theme::disp_text(format!("{} \u{b7} {}", c.label, c.wad).to_uppercase(), theme::style_eyebrow(), theme::FAINT));
                    match c.text.get(&k) {
                        Some(s) if s.is_empty() => ui.label(egui::RichText::new("empty string").italics().color(theme::FAINT)),
                        Some(s) => ui.label(egui::RichText::new(s.as_str()).color(theme::TX)),
                        None => ui.label(egui::RichText::new("not in this table").color(theme::FAINT)),
                    };
                }
            });
        }
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_read_with_either_quote_and_escapes_skipped() {
        let src = "Gui.Text(\"[Menu.Play]\") x = 'single' y = \"a\\\"b\" -- \"c\"";
        let got: Vec<&str> = string_literals(src).collect();
        assert_eq!(got, vec!["[Menu.Play]", "single", "a\\\"b", "c"]);
    }

    /// A bracketed or bare literal names its key; one written as the hash names nothing.
    #[test]
    fn lua_literals_name_keys_but_not_by_their_hash() {
        let play = mercs2_formats::hash::pandemic_hash_m2("Menu.Play");
        let keys = std::collections::HashSet::from([play, 0x5CB9_7D23]);
        let lua = vec!["x = \"[Menu.Play]\" y = \"[0x5cb97d23]\" z = 'other'".to_string()];
        let got = lua_key_names(&lua, &keys);
        assert_eq!(got.get(&play).map(String::as_str), Some("Menu.Play"));
        assert!(!got.contains_key(&0x5CB9_7D23));
        assert_eq!(got.len(), 1);
    }

    fn tables() -> Tables {
        let text = |pairs: &[(u32, &str)]| pairs.iter().map(|(k, v)| (*k, v.to_string())).collect();
        Tables {
            copies: vec![
                Copy { hash: 1, label: "english".into(), wad: "shell.wad".into(), text: text(&[(10, "Play"), (11, "Quit")]) },
                Copy { hash: 2, label: "french".into(), wad: "shell.wad".into(), text: text(&[(10, "Jouer")]) },
                Copy { hash: 2, label: "french".into(), wad: "French.wad".into(), text: text(&[(10, "Jouer!")]) },
            ],
            keys: vec![10, 11],
            key_names: HashMap::from([(10, "Menu.Play".to_string())]),
        }
    }

    /// The opened table's column comes first; the filter matches key names and every shown table's
    /// text; choosing another copy shows its text and filters on it.
    #[test]
    fn columns_filter_and_copies() {
        let mut p = Panel { load: Load::Ready(Box::new(tables())), table: Some(2), ..Panel::default() };
        p.refilter();
        assert_eq!(p.columns(), vec![2, 1]);
        assert_eq!(p.rows(), &[10, 11]);
        apply(Act::Filter("quit".into()), &mut p);
        assert_eq!(p.rows(), &[11]);
        apply(Act::Filter("menu.".into()), &mut p);
        assert_eq!(p.rows(), &[10]);
        apply(Act::Filter("jouer!".into()), &mut p);
        assert!(p.rows().is_empty(), "the French.wad copy is not shown yet");
        apply(Act::Copy(2), &mut p);
        assert_eq!(p.shown_copy(2), Some(2));
        assert_eq!(p.rows(), &[10]);
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail {
    use super::*;

    /// Every string table the retail install ships reads: English has 18,299 keys, and French is
    /// read from both shell.wad and French.wad.
    #[test]
    fn every_retail_string_table_reads() {
        let w = crate::app::WadStack::open(&crate::app::retail_vz_wad(), &[]).expect("open retail vz.wad");
        let index = crate::index::AssetIndex::build(&w.wads, &w.side, HashMap::new());
        let mut sources = Vec::new();
        for r in index.rows(crate::index::Kind::Text) {
            if r.type_id != mercs2_formats::types::TYPE_ID_STRINGDB {
                continue;
            }
            for src in std::iter::once(r.src).chain(r.others.iter().copied()) {
                sources.push(Source { hash: r.hash, label: r.label(), path: w.source_path(src).to_string() });
            }
        }
        let t = load(&sources, &HashMap::new(), &[]).unwrap_or_else(|e| panic!("{e}"));
        let english = mercs2_formats::hash::pandemic_hash_m2("english");
        let en = t.copies.iter().find(|c| c.hash == english).expect("no english table");
        assert_eq!(en.text.len(), 18_299);
        let french = mercs2_formats::hash::pandemic_hash_m2("french");
        let fr: Vec<&str> = t.copies.iter().filter(|c| c.hash == french).map(|c| c.wad.as_str()).collect();
        assert!(fr.iter().any(|w| w.eq_ignore_ascii_case("shell.wad")), "{fr:?}");
        assert!(fr.iter().any(|w| w.eq_ignore_ascii_case("french.wad")), "{fr:?}");
    }

    /// Every retail font reads, from every WAD that ships it; every english_18 and english_20 glyph
    /// box lies inside its page and is the record's width × height to half a pixel.
    #[test]
    fn every_retail_font_reads_and_english_boxes_fit_their_pages() {
        use mercs2_formats::types::{TYPE_HASH_FONT, TYPE_ID_FONT};
        let mut w = crate::app::WadStack::open(&crate::app::retail_vz_wad(), &[]).expect("open retail vz.wad");
        let index = crate::index::AssetIndex::build(&w.wads, &w.side, HashMap::new());
        let rows: Vec<_> = index.rows(crate::index::Kind::Text).iter().filter(|r| r.type_id == TYPE_ID_FONT).cloned().collect();
        assert!(!rows.is_empty());
        let mut checked = 0;
        for r in &rows {
            for src in std::iter::once(r.src).chain(r.others.iter().copied()) {
                let wad = w.source_mut(src).expect("source");
                let c = mercs2_engine::wad::extract_container_typed(wad, r.hash, TYPE_HASH_FONT).unwrap_or_else(|e| panic!("{e}"));
                let font = mercs2_formats::font::parse_font(&c).unwrap_or_else(|e| panic!("font 0x{:08X}: {e}", r.hash));
                let english = ["english_18", "english_20"].iter().any(|n| mercs2_formats::hash::pandemic_hash_m2(n) == r.hash);
                if !english {
                    continue;
                }
                for g in &font.glyphs {
                    let page = w.texture_best(font.pages[g.page as usize]).unwrap_or_else(|e| panic!("{e}"));
                    let (x0, y0, x1, y1) = g.box_px(page.width, page.height);
                    assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= page.width as f32 && y1 <= page.height as f32, "U+{:04X} off its page", g.code);
                    assert!((x1 - x0 - g.width as f32).abs() <= 0.5 && (y1 - y0 - g.height as f32).abs() <= 0.5, "U+{:04X} box", g.code);
                    checked += 1;
                }
            }
        }
        assert!(checked >= 2 * 271, "only {checked} english glyphs checked");
    }
}
