//! Read-only Lua source viewer with syntax highlighting.
//!
//! Opened from the "Game scripts mentioning …" rows so a corpus hit can be read in its full
//! context instead of a one-line excerpt. The highlighter is a small hand-rolled Lua lexer
//! (comments, strings incl. `[=[ long brackets ]=]`, keywords, numbers) — deterministic, no
//! extra dependencies. Every occurrence of the search needle is background-highlighted and the
//! header buttons jump between matches.

use egui::text::{LayoutJob, TextFormat};
use egui::Color32;

/// Lexer state carried across line boundaries (long strings / block comments span lines).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Carry {
    None,
    /// Inside a `--[=*[` block comment; payload = number of `=` in the bracket.
    Comment(u8),
    /// Inside a `[=*[` long string; payload = number of `=`.
    Str(u8),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    Plain,
    Comment,
    Str,
    Keyword,
    Number,
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// `[=*[` at `i` → (level, token length).
fn long_open(b: &[u8], i: usize) -> Option<(u8, usize)> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    let mut lvl = 0u8;
    while b.get(j) == Some(&b'=') {
        j += 1;
        lvl = lvl.saturating_add(1);
    }
    if b.get(j) == Some(&b'[') { Some((lvl, j - i + 1)) } else { None }
}

/// `]=*]` of exactly `lvl` at `i` → token length.
fn long_close(b: &[u8], i: usize, lvl: u8) -> Option<usize> {
    if b.get(i) != Some(&b']') {
        return None;
    }
    let mut j = i + 1;
    let mut k = 0u8;
    while b.get(j) == Some(&b'=') {
        j += 1;
        k = k.saturating_add(1);
    }
    if k == lvl && b.get(j) == Some(&b']') { Some(j - i + 1) } else { None }
}

/// Classify one line byte-wise given the carry state at its start; returns the carry at its
/// end. Class changes only happen at ASCII delimiters, so runs always cut on char boundaries.
fn classify_line(line: &str, mut carry: Carry, classes: &mut Vec<Class>) -> Carry {
    let b = line.as_bytes();
    classes.clear();
    classes.resize(b.len(), Class::Plain);
    let mut i = 0;
    while i < b.len() {
        match carry {
            Carry::Comment(lvl) | Carry::Str(lvl) => {
                let cls = if matches!(carry, Carry::Comment(_)) { Class::Comment } else { Class::Str };
                let mut j = i;
                let mut closed = None;
                while j < b.len() {
                    if let Some(l) = long_close(b, j, lvl) {
                        closed = Some(j + l);
                        break;
                    }
                    j += 1;
                }
                let end = closed.unwrap_or(b.len());
                for k in i..end {
                    classes[k] = cls;
                }
                i = end;
                if closed.is_some() {
                    carry = Carry::None;
                }
            }
            Carry::None => {
                let c = b[i];
                if c == b'-' && b.get(i + 1) == Some(&b'-') {
                    if let Some((lvl, l)) = long_open(b, i + 2) {
                        for k in i..i + 2 + l {
                            classes[k] = Class::Comment;
                        }
                        i += 2 + l;
                        carry = Carry::Comment(lvl);
                        continue;
                    }
                    for k in i..b.len() {
                        classes[k] = Class::Comment;
                    }
                    return Carry::None;
                }
                if let Some((lvl, l)) = long_open(b, i) {
                    for k in i..i + l {
                        classes[k] = Class::Str;
                    }
                    i += l;
                    carry = Carry::Str(lvl);
                    continue;
                }
                if c == b'"' || c == b'\'' {
                    classes[i] = Class::Str;
                    let mut j = i + 1;
                    while j < b.len() {
                        classes[j] = Class::Str;
                        if b[j] == b'\\' && j + 1 < b.len() {
                            classes[j + 1] = Class::Str;
                            j += 2;
                            continue;
                        }
                        if b[j] == c {
                            j += 1;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                if c.is_ascii_alphabetic() || c == b'_' {
                    let mut j = i;
                    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                        j += 1;
                    }
                    if KEYWORDS.contains(&&line[i..j]) {
                        for k in i..j {
                            classes[k] = Class::Keyword;
                        }
                    }
                    i = j;
                    continue;
                }
                if c.is_ascii_digit() {
                    let mut j = i;
                    while j < b.len()
                        && (b[j].is_ascii_alphanumeric() || b[j] == b'.' )
                    {
                        j += 1;
                    }
                    for k in i..j {
                        classes[k] = Class::Number;
                    }
                    i = j;
                    continue;
                }
                i += 1;
            }
        }
    }
    carry
}

pub struct LuaView {
    pub open: bool,
    path: String,
    lines: Vec<String>,
    /// Lexer carry at the START of each line (one whole-file pass at open).
    carries: Vec<Carry>,
    /// Line indices containing the needle (sorted).
    matches: Vec<usize>,
    cur: usize,
    needle: String,
    scroll_to: Option<usize>,
}

impl LuaView {
    pub fn new(path: &str, content: &str, needle: &str) -> LuaView {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut carries = Vec::with_capacity(lines.len());
        let mut carry = Carry::None;
        let mut scratch = Vec::new();
        for l in &lines {
            carries.push(carry);
            carry = classify_line(l, carry, &mut scratch);
        }
        let needle: String = needle.chars().map(|c| c.to_ascii_lowercase()).collect();
        let matches: Vec<usize> = if needle.is_empty() {
            Vec::new()
        } else {
            lines
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    let lower: String = l.chars().map(|c| c.to_ascii_lowercase()).collect();
                    lower.contains(&needle)
                })
                .map(|(i, _)| i)
                .collect()
        };
        let scroll_to = matches.first().copied();
        LuaView { open: true, path: path.into(), lines, carries, matches, cur: 0, needle, scroll_to }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let mut open = self.open;
        egui::Window::new(format!("Lua — {}", self.path))
            .id(egui::Id::new("lua_view"))
            .open(&mut open)
            .default_size([760.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.weak(format!("{} lines — read-only", self.lines.len()));
                    if !self.matches.is_empty() {
                        ui.separator();
                        ui.weak(format!(
                            "match {}/{} for \"{}\"",
                            self.cur + 1,
                            self.matches.len(),
                            self.needle
                        ));
                        if ui.small_button("◀").clicked() {
                            self.cur = (self.cur + self.matches.len() - 1) % self.matches.len();
                            self.scroll_to = Some(self.matches[self.cur]);
                        }
                        if ui.small_button("▶").clicked() {
                            self.cur = (self.cur + 1) % self.matches.len();
                            self.scroll_to = Some(self.matches[self.cur]);
                        }
                    }
                });
                ui.separator();
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
                let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
                if let Some(line) = self.scroll_to.take() {
                    // Park the target a few rows below the top edge.
                    area = area.vertical_scroll_offset((line as f32 - 6.0).max(0.0) * row_h);
                }
                area.show_rows(ui, row_h, self.lines.len(), |ui, range| {
                    for li in range {
                        self.row(ui, li);
                    }
                });
            });
        self.open = open;
    }

    fn row(&self, ui: &mut egui::Ui, li: usize) {
        const COMMENT: Color32 = Color32::from_rgb(0x6A, 0x99, 0x55);
        const STRING: Color32 = Color32::from_rgb(0xCE, 0x91, 0x78);
        const KEYWORD: Color32 = Color32::from_rgb(0x56, 0x9C, 0xD6);
        const NUMBER: Color32 = Color32::from_rgb(0xB5, 0xCE, 0xA8);
        const GOLD: Color32 = Color32::from_rgb(0xE8, 0xB4, 0x4A);
        const MATCH_BG: Color32 = Color32::from_rgb(0x5A, 0x4A, 0x18);

        let line = &self.lines[li];
        let mut classes = Vec::new();
        let _ = classify_line(line, self.carries[li], &mut classes);
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let is_match = self.matches.binary_search(&li).is_ok();

        let mut job = LayoutJob::default();
        job.append(
            &format!("{:>5}  ", li + 1),
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: if is_match { GOLD } else { ui.visuals().weak_text_color() },
                ..Default::default()
            },
        );
        // Needle occurrences (byte mask; ASCII needle in UTF-8 always cuts on char boundaries).
        let mut mask = vec![false; line.len()];
        if is_match && !self.needle.is_empty() {
            let lower: String = line.chars().map(|c| c.to_ascii_lowercase()).collect();
            let mut from = 0;
            while let Some(pos) = lower[from..].find(&self.needle) {
                let s = from + pos;
                for m in mask.iter_mut().skip(s).take(self.needle.len()) {
                    *m = true;
                }
                from = s + self.needle.len().max(1);
            }
        }
        let mut i = 0;
        while i < line.len() {
            let (cls, nm) = (classes[i], mask[i]);
            let mut j = i + 1;
            while j < line.len() && classes[j] == cls && mask[j] == nm {
                j += 1;
            }
            let color = match cls {
                Class::Plain => ui.visuals().text_color(),
                Class::Comment => COMMENT,
                Class::Str => STRING,
                Class::Keyword => KEYWORD,
                Class::Number => NUMBER,
            };
            let mut fmt = TextFormat { font_id: font.clone(), color, ..Default::default() };
            if nm {
                fmt.background = MATCH_BG;
            }
            job.append(&line[i..j], 0.0, fmt);
            i = j;
        }
        ui.add(egui::Label::new(job).extend());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes_of(line: &str, carry: Carry) -> (Vec<Class>, Carry) {
        let mut v = Vec::new();
        let c = classify_line(line, carry, &mut v);
        (v, c)
    }

    #[test]
    fn lexes_comments_strings_keywords() {
        let (c, carry) = classes_of("local x = \"hi\" -- note", Carry::None);
        assert_eq!(carry, Carry::None);
        assert_eq!(c[0], Class::Keyword); // 'l' of local
        assert_eq!(c[6], Class::Plain); // 'x'
        assert_eq!(c[10], Class::Str); // opening quote
        assert_eq!(c[15], Class::Comment); // '-'
    }

    #[test]
    fn block_comment_carries_across_lines() {
        let (c1, carry) = classes_of("a = 1 --[[ start", Carry::None);
        assert_eq!(c1[4], Class::Number);
        assert_eq!(carry, Carry::Comment(0));
        let (c2, carry2) = classes_of("still comment ]] b = 2", carry);
        assert_eq!(c2[0], Class::Comment);
        assert_eq!(carry2, Carry::None);
        assert_eq!(*c2.last().unwrap(), Class::Number);
    }

    #[test]
    fn long_string_with_level() {
        let (_, carry) = classes_of("s = [==[ text", Carry::None);
        assert_eq!(carry, Carry::Str(2));
        // A lower-level closer must NOT close it.
        let (_, carry2) = classes_of("not closed ]] ]=]", carry);
        assert_eq!(carry2, Carry::Str(2));
        let (_, carry3) = classes_of("done ]==] rest", carry2);
        assert_eq!(carry3, Carry::None);
    }
}
